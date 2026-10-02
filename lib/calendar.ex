# ---- calendar.ex
defmodule Calendar do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
  # General Helpers

  def compatible_calendars?(calendar, calendar), do: true

  def compatible_calendars?(calendar1, calendar2) do
    calendar1.day_rollover_relative_to_midnight_utc() ==
      calendar2.day_rollover_relative_to_midnight_utc()
  end

  def truncate(microsecond_tuple, :microsecond), do: microsecond_tuple

  def truncate({microsecond, precision}, :millisecond) do
    output_precision = min(precision, 3)
    {div(microsecond, 1000) * 1000, output_precision}
  end

  def truncate(_, :second), do: {0, 0}

  def put_time_zone_database(database) when is_atom(database) do
    Application.put_env(:elixir, :time_zone_database, database)
  end

  def get_time_zone_database() do
    Application.fetch_env!(:elixir, :time_zone_database)
  end

  def strftime(date_or_time_or_datetime, string_format, user_options \\ [])
      when is_map(date_or_time_or_datetime) and is_binary(string_format) do
    parse(
      string_format,
      date_or_time_or_datetime,
      options(user_options),
      []
    )
    |> IO.iodata_to_binary()
  end

  defp parse("", _datetime, _format_options, acc),
    do: Enum.reverse(acc)

  defp parse("%" <> rest, datetime, format_options, acc),
    do: parse_modifiers(rest, nil, nil, {datetime, format_options, acc})

  defp parse(<<char, rest::binary>>, datetime, format_options, acc),
    do: parse(rest, datetime, format_options, [char | acc])

  defp parse_modifiers("-" <> rest, width, nil, parser_data) do
    parse_modifiers(rest, width, "", parser_data)
  end

  defp parse_modifiers("0" <> rest, nil, nil, parser_data) do
    parse_modifiers(rest, nil, ?0, parser_data)
  end

  defp parse_modifiers("_" <> rest, width, nil, parser_data) do
    parse_modifiers(rest, width, ?\s, parser_data)
  end

  defp parse_modifiers(<<digit, rest::binary>>, width, pad, parser_data) when digit in ?0..?9 do
    new_width = (width || 0) * 10 + (digit - ?0)

    parse_modifiers(rest, new_width, pad, parser_data)
  end

  # set default padding if none was specified
  defp parse_modifiers(<<format, _::binary>> = rest, width, nil, parser_data) do
    parse_modifiers(rest, width, default_pad(format), parser_data)
  end

  # set default width if none was specified
  defp parse_modifiers(<<format, _::binary>> = rest, nil, pad, parser_data) do
    parse_modifiers(rest, default_width(format), pad, parser_data)
  end

  defp parse_modifiers(rest, width, pad, {datetime, format_options, acc}) do
    format_modifiers(rest, width, pad, datetime, format_options, acc)
  end

  defp am_pm(hour, format_options) when hour > 11 do
    format_options.am_pm_names.(:pm)
  end

  defp am_pm(hour, format_options) when hour <= 11 do
    format_options.am_pm_names.(:am)
  end

  defp default_pad(format) when format in ~c"aAbBpPZ", do: ?\s
  defp default_pad(_format), do: ?0

  defp default_width(format) when format in ~c"dHImMSy", do: 2
  defp default_width(?j), do: 3
  defp default_width(format) when format in ~c"Yz", do: 4
  defp default_width(_format), do: 0

  # Literally just %
  defp format_modifiers("%" <> rest, width, pad, datetime, format_options, acc) do
    parse(rest, datetime, format_options, [pad_leading("%", width, pad) | acc])
  end

  # Abbreviated name of day
  defp format_modifiers("a" <> rest, width, pad, datetime, format_options, acc) do
    result =
      datetime
      |> Date.day_of_week()
      |> format_options.abbreviated_day_of_week_names.()
      |> pad_leading(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Full name of day
  defp format_modifiers("A" <> rest, width, pad, datetime, format_options, acc) do
    result =
      datetime
      |> Date.day_of_week()
      |> format_options.day_of_week_names.()
      |> pad_leading(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Abbreviated month name
  defp format_modifiers("b" <> rest, width, pad, datetime, format_options, acc) do
    result =
      datetime.month
      |> format_options.abbreviated_month_names.()
      |> pad_leading(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Full month name
  defp format_modifiers("B" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.month |> format_options.month_names.() |> pad_leading(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Preferred date+time representation
  defp format_modifiers(
         "c" <> _rest,
         _width,
         _pad,
         _datetime,
         %{preferred_datetime_invoked: true},
         _acc
       ) do
    raise ArgumentError,
          "tried to format preferred_datetime within another preferred_datetime format"
  end

  defp format_modifiers("c" <> rest, width, pad, datetime, format_options, acc) do
    result =
      format_options.preferred_datetime
      |> parse(datetime, %{format_options | preferred_datetime_invoked: true}, [])
      |> pad_preferred(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Day of the month
  defp format_modifiers("d" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.day |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Microseconds
  defp format_modifiers("f" <> rest, _width, _pad, datetime, format_options, acc) do
    {microsecond, precision} = datetime.microsecond

    result =
      microsecond
      |> Integer.to_string()
      |> String.pad_leading(6, "0")
      |> binary_part(0, max(precision, 1))

    parse(rest, datetime, format_options, [result | acc])
  end

  # Hour using a 24-hour clock
  defp format_modifiers("H" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.hour |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Hour using a 12-hour clock
  defp format_modifiers("I" <> rest, width, pad, datetime, format_options, acc) do
    result = (rem(datetime.hour + 23, 12) + 1) |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Day of the year
  defp format_modifiers("j" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime |> Date.day_of_year() |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Month
  defp format_modifiers("m" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.month |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Minute
  defp format_modifiers("M" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.minute |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # "AM" or "PM" (noon is "PM", midnight as "AM")
  defp format_modifiers("p" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.hour |> am_pm(format_options) |> String.upcase() |> pad_leading(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # "am" or "pm" (noon is "pm", midnight as "am")
  defp format_modifiers("P" <> rest, width, pad, datetime, format_options, acc) do
    result =
      datetime.hour
      |> am_pm(format_options)
      |> String.downcase()
      |> pad_leading(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Quarter
  defp format_modifiers("q" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime |> Date.quarter_of_year() |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Second
  defp format_modifiers("S" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.second |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Day of the week
  defp format_modifiers("u" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime |> Date.day_of_week() |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Preferred date (without time) representation
  defp format_modifiers(
         "x" <> _rest,
         _width,
         _pad,
         _datetime,
         %{preferred_date_invoked: true},
         _acc
       ) do
    raise ArgumentError,
          "tried to format preferred_date within another preferred_date format"
  end

  defp format_modifiers("x" <> rest, width, pad, datetime, format_options, acc) do
    result =
      format_options.preferred_date
      |> parse(datetime, %{format_options | preferred_date_invoked: true}, [])
      |> pad_preferred(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Preferred time (without date) representation
  defp format_modifiers(
         "X" <> _rest,
         _width,
         _pad,
         _datetime,
         %{preferred_time_invoked: true},
         _acc
       ) do
    raise ArgumentError,
          "tried to format preferred_time within another preferred_time format"
  end

  defp format_modifiers("X" <> rest, width, pad, datetime, format_options, acc) do
    result =
      format_options.preferred_time
      |> parse(datetime, %{format_options | preferred_time_invoked: true}, [])
      |> pad_preferred(width, pad)

    parse(rest, datetime, format_options, [result | acc])
  end

  # Year as 2-digits
  defp format_modifiers("y" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime.year |> rem(100) |> Integer.to_string() |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  # Year
  defp format_modifiers("Y" <> rest, width, pad, datetime, format_options, acc) do
    {sign, year} =
      if datetime.year < 0 do
        {?-, -datetime.year}
      else
        {[], datetime.year}
      end

    result = [sign | year |> Integer.to_string() |> pad_leading(width, pad)]
    parse(rest, datetime, format_options, [result | acc])
  end

  # Epoch time for DateTime with time zones
  defp format_modifiers(
         "s" <> rest,
         _width,
         _pad,
         datetime = %{utc_offset: _utc_offset, std_offset: _std_offset},
         format_options,
         acc
       ) do
    result =
      datetime
      |> DateTime.shift_zone!("Etc/UTC")
      |> NaiveDateTime.diff(~N[1970-01-01 00:00:00])
      |> Integer.to_string()

    parse(rest, datetime, format_options, [result | acc])
  end

  # Epoch time
  defp format_modifiers("s" <> rest, _width, _pad, datetime, format_options, acc) do
    result =
      datetime
      |> NaiveDateTime.diff(~N[1970-01-01 00:00:00])
      |> Integer.to_string()

    parse(rest, datetime, format_options, [result | acc])
  end

  # +hhmm/-hhmm time zone offset from UTC (empty string if naive)
  defp format_modifiers(
         "z" <> rest,
         width,
         pad,
         datetime = %{utc_offset: utc_offset, std_offset: std_offset},
         format_options,
         acc
       ) do
    absolute_offset = abs(utc_offset + std_offset)

    offset_number =
      Integer.to_string(div(absolute_offset, 3600) * 100 + rem(div(absolute_offset, 60), 60))

    sign = if utc_offset + std_offset >= 0, do: "+", else: "-"
    result = "#{sign}#{pad_leading(offset_number, width, pad)}"
    parse(rest, datetime, format_options, [result | acc])
  end

  defp format_modifiers("z" <> rest, _width, _pad, datetime, format_options, acc) do
    parse(rest, datetime, format_options, ["" | acc])
  end

  # Time zone abbreviation (empty string if naive)
  defp format_modifiers("Z" <> rest, width, pad, datetime, format_options, acc) do
    result = datetime |> Map.get(:zone_abbr, "") |> pad_leading(width, pad)
    parse(rest, datetime, format_options, [result | acc])
  end

  defp format_modifiers(rest, _width, _pad, _datetime, _format_options, _acc) do
    {next, _rest} = String.next_grapheme(rest) || {"", ""}
    raise ArgumentError, "invalid strftime format: %#{next}"
  end

  defp pad_preferred(result, width, pad) when length(result) < width do
    pad_preferred([pad | result], width, pad)
  end

  defp pad_preferred(result, _width, _pad), do: result

  defp pad_leading(string, count, padding) do
    to_pad = count - byte_size(string)
    if to_pad > 0, do: do_pad_leading(to_pad, padding, string), else: string
  end

  defp do_pad_leading(0, _, acc), do: acc

  defp do_pad_leading(count, padding, acc),
    do: do_pad_leading(count - 1, padding, [padding | acc])

  defp options(user_options) do
    default_options = %{
      preferred_date: "%Y-%m-%d",
      preferred_time: "%H:%M:%S",
      preferred_datetime: "%Y-%m-%d %H:%M:%S",
      am_pm_names: fn
        :am -> "am"
        :pm -> "pm"
      end,
      month_names: fn month ->
        {"January", "February", "March", "April", "May", "June", "July", "August", "September",
         "October", "November", "December"}
        |> elem(month - 1)
      end,
      day_of_week_names: fn day_of_week ->
        {"Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"}
        |> elem(day_of_week - 1)
      end,
      abbreviated_month_names: fn month ->
        {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"}
        |> elem(month - 1)
      end,
      abbreviated_day_of_week_names: fn day_of_week ->
        {"Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"} |> elem(day_of_week - 1)
      end,
      preferred_datetime_invoked: false,
      preferred_date_invoked: false,
      preferred_time_invoked: false
    }

    Enum.reduce(user_options, default_options, fn {key, value}, acc ->
      if Map.has_key?(acc, key) do
        %{acc | key => value}
      else
        raise ArgumentError, "unknown option #{inspect(key)} given to Calendar.strftime/3"
      end
    end)
  end
end

# ---- calendar/iso.ex
defmodule Calendar.ISO do

  @behaviour Calendar

  @unix_epoch 62_167_219_200
  @unix_range_microseconds -377_705_116_800_000_000..253_402_300_799_999_999

  defguardp is_format(term) when term in [:basic, :extended]

  @seconds_per_minute 60
  @seconds_per_hour 60 * 60
  # Note that this does *not* handle leap seconds.
  @seconds_per_day 24 * 60 * 60
  @last_second_of_the_day @seconds_per_day - 1
  @microseconds_per_second 1_000_000
  @parts_per_day @seconds_per_day * @microseconds_per_second

  @datetime_seps [?\s, ?T]
  @ext_date_sep ?-
  @ext_time_sep ?:

  @days_per_nonleap_year 365
  @days_per_leap_year 366

  # The ISO epoch starts, in this implementation,
  # with ~D[0000-01-01]. Era "1" starts
  # on ~D[0001-01-01] which is 366 days later.
  @iso_epoch 366



  defguardp is_year(year) when is_integer(year)
  defguardp is_year_BCE(year) when year <= 0
  defguardp is_year_CE(year) when year >= 1
  defguardp is_month(month) when month in 1..12
  defguardp is_day(day) when day in 1..31
  defguardp is_hour(hour) when hour in 0..23
  defguardp is_minute(minute) when minute in 0..59
  defguardp is_second(second) when second in 0..59

  defguardp is_microsecond(microsecond, precision)
            when microsecond in 0..999_999 and precision in 0..6

  defguardp is_time_zone(term) when is_binary(term)
  defguardp is_zone_abbr(term) when is_binary(term)
  defguardp is_utc_offset(offset) when is_integer(offset)
  defguardp is_std_offset(offset) when is_integer(offset)

  def time_unit_to_precision(:nanosecond), do: 6
  def time_unit_to_precision(:microsecond), do: 6
  def time_unit_to_precision(:millisecond), do: 3
  def time_unit_to_precision(:second), do: 0
  def time_unit_to_precision(int) when is_integer(int), do: 6

  def parse_time(string) when is_binary(string),
    do: parse_time(string, :extended)

  def parse_time(string, format) when is_binary(string) and is_format(format) do
    case string do
      "T" <> rest -> do_parse_time(rest, format)
      _ -> do_parse_time(string, format)
    end
  end

  defp do_parse_time(<<h1, h2, i1, i2, s1, s2, rest::binary>>, :basic)
       when (h1 >= ?0 and h1 <= ?9 and h2 >= ?0 and h2 <= ?9 and i1 >= ?0 and i1 <= ?9 and i2 >= ?0 and i2 <= ?9 and s1 >= ?0 and s1 <= ?9 and s2 >= ?0 and s2 <= ?9) do
    {hour, minute, second} = ({(h1 - ?0) * 10 + (h2 - ?0), (i1 - ?0) * 10 + (i2 - ?0), (s1 - ?0) * 10 + (s2 - ?0)})
    parse_formatted_time(hour, minute, second, rest)
  end

  defp do_parse_time(<<h1, h2, @ext_time_sep, i1, i2, @ext_time_sep, s1, s2, rest::binary>>, :extended)
       when (h1 >= ?0 and h1 <= ?9 and h2 >= ?0 and h2 <= ?9 and i1 >= ?0 and i1 <= ?9 and i2 >= ?0 and i2 <= ?9 and s1 >= ?0 and s1 <= ?9 and s2 >= ?0 and s2 <= ?9) do
    {hour, minute, second} = ({(h1 - ?0) * 10 + (h2 - ?0), (i1 - ?0) * 10 + (i2 - ?0), (s1 - ?0) * 10 + (s2 - ?0)})
    parse_formatted_time(hour, minute, second, rest)
  end

  defp do_parse_time(_, _) do
    {:error, :invalid_format}
  end

  defp parse_formatted_time(hour, minute, second, rest) do
    with {microsecond, rest} <- parse_microsecond(rest),
         {_offset, ""} <- parse_offset(rest) do
      if valid_time?(hour, minute, second, microsecond) do
        {:ok, {hour, minute, second, microsecond}}
      else
        {:error, :invalid_time}
      end
    else
      _ -> {:error, :invalid_format}
    end
  end

  def parse_date(string) when is_binary(string),
    do: parse_date(string, :extended)

  def parse_date(string, format) when is_binary(string) and is_format(format),
    do: parse_date_guarded(string, format)

  defp parse_date_guarded("-" <> string, format),
    do: do_parse_date(string, -1, format)

  defp parse_date_guarded("+" <> string, format),
    do: do_parse_date(string, 1, format)

  defp parse_date_guarded(string, format),
    do: do_parse_date(string, 1, format)

  defp do_parse_date(<<y1, y2, y3, y4, m1, m2, d1, d2>>, multiplier, :basic) when (y1 >= ?0 and y1 <= ?9 and y2 >= ?0 and y2 <= ?9 and y3 >= ?0 and y3 <= ?9 and y4 >= ?0 and y4 <= ?9 and m1 >= ?0 and m1 <= ?9 and m2 >= ?0 and m2 <= ?9 and d1 >= ?0 and d1 <= ?9 and d2 >= ?0 and d2 <= ?9) do
    {year, month, day} = ({(y1 - ?0) * 1000 + (y2 - ?0) * 100 + (y3 - ?0) * 10 + (y4 - ?0), (m1 - ?0) * 10 + (m2 - ?0), (d1 - ?0) * 10 + (d2 - ?0)})
    parse_formatted_date(year, month, day, multiplier)
  end

  defp do_parse_date(<<y1, y2, y3, y4, @ext_date_sep, m1, m2, @ext_date_sep, d1, d2>>, multiplier, :extended) when (y1 >= ?0 and y1 <= ?9 and y2 >= ?0 and y2 <= ?9 and y3 >= ?0 and y3 <= ?9 and y4 >= ?0 and y4 <= ?9 and m1 >= ?0 and m1 <= ?9 and m2 >= ?0 and m2 <= ?9 and d1 >= ?0 and d1 <= ?9 and d2 >= ?0 and d2 <= ?9) do
    {year, month, day} = ({(y1 - ?0) * 1000 + (y2 - ?0) * 100 + (y3 - ?0) * 10 + (y4 - ?0), (m1 - ?0) * 10 + (m2 - ?0), (d1 - ?0) * 10 + (d2 - ?0)})
    parse_formatted_date(year, month, day, multiplier)
  end

  defp do_parse_date(_, _, _) do
    {:error, :invalid_format}
  end

  defp parse_formatted_date(year, month, day, multiplier) do
    year = multiplier * year

    if valid_date?(year, month, day) do
      {:ok, {year, month, day}}
    else
      {:error, :invalid_date}
    end
  end

  def parse_naive_datetime(string) when is_binary(string),
    do: parse_naive_datetime(string, :extended)

  def parse_naive_datetime(string, format) when is_binary(string) and is_format(format),
    do: parse_naive_datetime_guarded(string, format)

  defp parse_naive_datetime_guarded("-" <> string, format),
    do: do_parse_naive_datetime(string, -1, format)

  defp parse_naive_datetime_guarded("+" <> string, format),
    do: do_parse_naive_datetime(string, 1, format)

  defp parse_naive_datetime_guarded(string, format),
    do: do_parse_naive_datetime(string, 1, format)

  defp do_parse_naive_datetime(
         <<y1, y2, y3, y4, m1, m2, d1, d2, datetime_sep, h1, h2, i1, i2, s1, s2, rest::binary>>,
         multiplier,
         :basic
       )
       when (y1 >= ?0 and y1 <= ?9 and y2 >= ?0 and y2 <= ?9 and y3 >= ?0 and y3 <= ?9 and y4 >= ?0 and y4 <= ?9 and m1 >= ?0 and m1 <= ?9 and m2 >= ?0 and m2 <= ?9 and d1 >= ?0 and d1 <= ?9 and d2 >= ?0 and d2 <= ?9) and datetime_sep in @datetime_seps and (h1 >= ?0 and h1 <= ?9 and h2 >= ?0 and h2 <= ?9 and i1 >= ?0 and i1 <= ?9 and i2 >= ?0 and i2 <= ?9 and s1 >= ?0 and s1 <= ?9 and s2 >= ?0 and s2 <= ?9) do
    {year, month, day} = ({(y1 - ?0) * 1000 + (y2 - ?0) * 100 + (y3 - ?0) * 10 + (y4 - ?0), (m1 - ?0) * 10 + (m2 - ?0), (d1 - ?0) * 10 + (d2 - ?0)})
    {hour, minute, second} = ({(h1 - ?0) * 10 + (h2 - ?0), (i1 - ?0) * 10 + (i2 - ?0), (s1 - ?0) * 10 + (s2 - ?0)})
    parse_formatted_naive_datetime(year, month, day, hour, minute, second, rest, multiplier)
  end

  defp do_parse_naive_datetime(
         <<y1, y2, y3, y4, @ext_date_sep, m1, m2, @ext_date_sep, d1, d2, datetime_sep, h1, h2, @ext_time_sep, i1, i2, @ext_time_sep, s1, s2, rest::binary>>,
         multiplier,
         :extended
       )
       when (y1 >= ?0 and y1 <= ?9 and y2 >= ?0 and y2 <= ?9 and y3 >= ?0 and y3 <= ?9 and y4 >= ?0 and y4 <= ?9 and m1 >= ?0 and m1 <= ?9 and m2 >= ?0 and m2 <= ?9 and d1 >= ?0 and d1 <= ?9 and d2 >= ?0 and d2 <= ?9) and datetime_sep in @datetime_seps and (h1 >= ?0 and h1 <= ?9 and h2 >= ?0 and h2 <= ?9 and i1 >= ?0 and i1 <= ?9 and i2 >= ?0 and i2 <= ?9 and s1 >= ?0 and s1 <= ?9 and s2 >= ?0 and s2 <= ?9) do
    {year, month, day} = ({(y1 - ?0) * 1000 + (y2 - ?0) * 100 + (y3 - ?0) * 10 + (y4 - ?0), (m1 - ?0) * 10 + (m2 - ?0), (d1 - ?0) * 10 + (d2 - ?0)})
    {hour, minute, second} = ({(h1 - ?0) * 10 + (h2 - ?0), (i1 - ?0) * 10 + (i2 - ?0), (s1 - ?0) * 10 + (s2 - ?0)})
    parse_formatted_naive_datetime(year, month, day, hour, minute, second, rest, multiplier)
  end

  defp do_parse_naive_datetime(_, _, _) do
    {:error, :invalid_format}
  end

  defp parse_formatted_naive_datetime(year, month, day, hour, minute, second, rest, multiplier) do
    year = multiplier * year

    with {microsecond, rest} <- parse_microsecond(rest),
         {_offset, ""} <- parse_offset(rest) do
      cond do
        not valid_date?(year, month, day) ->
          {:error, :invalid_date}

        not valid_time?(hour, minute, second, microsecond) ->
          {:error, :invalid_time}

        true ->
          {:ok, {year, month, day, hour, minute, second, microsecond}}
      end
    else
      _ -> {:error, :invalid_format}
    end
  end

  def parse_utc_datetime(string) when is_binary(string),
    do: parse_utc_datetime(string, :extended)

  def parse_utc_datetime(string, format) when is_binary(string) and is_format(format),
    do: parse_utc_datetime_guarded(string, format)

  defp parse_utc_datetime_guarded("-" <> string, format),
    do: do_parse_utc_datetime(string, -1, format)

  defp parse_utc_datetime_guarded("+" <> string, format),
    do: do_parse_utc_datetime(string, 1, format)

  defp parse_utc_datetime_guarded(string, format),
    do: do_parse_utc_datetime(string, 1, format)

  defp do_parse_utc_datetime(
         <<y1, y2, y3, y4, m1, m2, d1, d2, datetime_sep, h1, h2, i1, i2, s1, s2, rest::binary>>,
         multiplier,
         :basic
       )
       when (y1 >= ?0 and y1 <= ?9 and y2 >= ?0 and y2 <= ?9 and y3 >= ?0 and y3 <= ?9 and y4 >= ?0 and y4 <= ?9 and m1 >= ?0 and m1 <= ?9 and m2 >= ?0 and m2 <= ?9 and d1 >= ?0 and d1 <= ?9 and d2 >= ?0 and d2 <= ?9) and datetime_sep in @datetime_seps and (h1 >= ?0 and h1 <= ?9 and h2 >= ?0 and h2 <= ?9 and i1 >= ?0 and i1 <= ?9 and i2 >= ?0 and i2 <= ?9 and s1 >= ?0 and s1 <= ?9 and s2 >= ?0 and s2 <= ?9) do
    {year, month, day} = ({(y1 - ?0) * 1000 + (y2 - ?0) * 100 + (y3 - ?0) * 10 + (y4 - ?0), (m1 - ?0) * 10 + (m2 - ?0), (d1 - ?0) * 10 + (d2 - ?0)})
    {hour, minute, second} = ({(h1 - ?0) * 10 + (h2 - ?0), (i1 - ?0) * 10 + (i2 - ?0), (s1 - ?0) * 10 + (s2 - ?0)})
    parse_formatted_utc_datetime(year, month, day, hour, minute, second, rest, multiplier)
  end

  defp do_parse_utc_datetime(
         <<y1, y2, y3, y4, @ext_date_sep, m1, m2, @ext_date_sep, d1, d2, datetime_sep, h1, h2, @ext_time_sep, i1, i2, @ext_time_sep, s1, s2, rest::binary>>,
         multiplier,
         :extended
       )
       when (y1 >= ?0 and y1 <= ?9 and y2 >= ?0 and y2 <= ?9 and y3 >= ?0 and y3 <= ?9 and y4 >= ?0 and y4 <= ?9 and m1 >= ?0 and m1 <= ?9 and m2 >= ?0 and m2 <= ?9 and d1 >= ?0 and d1 <= ?9 and d2 >= ?0 and d2 <= ?9) and datetime_sep in @datetime_seps and (h1 >= ?0 and h1 <= ?9 and h2 >= ?0 and h2 <= ?9 and i1 >= ?0 and i1 <= ?9 and i2 >= ?0 and i2 <= ?9 and s1 >= ?0 and s1 <= ?9 and s2 >= ?0 and s2 <= ?9) do
    {year, month, day} = ({(y1 - ?0) * 1000 + (y2 - ?0) * 100 + (y3 - ?0) * 10 + (y4 - ?0), (m1 - ?0) * 10 + (m2 - ?0), (d1 - ?0) * 10 + (d2 - ?0)})
    {hour, minute, second} = ({(h1 - ?0) * 10 + (h2 - ?0), (i1 - ?0) * 10 + (i2 - ?0), (s1 - ?0) * 10 + (s2 - ?0)})
    parse_formatted_utc_datetime(year, month, day, hour, minute, second, rest, multiplier)
  end

  defp do_parse_utc_datetime(_, _, _) do
    {:error, :invalid_format}
  end

  defp parse_formatted_utc_datetime(year, month, day, hour, minute, second, rest, multiplier) do
    year = multiplier * year

    with {microsecond, rest} <- parse_microsecond(rest),
         {offset, ""} <- parse_offset(rest) do
      cond do
        not valid_date?(year, month, day) ->
          {:error, :invalid_date}

        not valid_time?(hour, minute, second, microsecond) ->
          {:error, :invalid_time}

        offset == 0 ->
          {:ok, {year, month, day, hour, minute, second, microsecond}, offset}

        is_nil(offset) ->
          {:error, :missing_offset}

        true ->
          day_fraction = time_to_day_fraction(hour, minute, second, {0, 0})

          {{year, month, day}, {hour, minute, second, _}} =
            case add_day_fraction_to_iso_days({0, day_fraction}, -offset, 86400) do
              {0, day_fraction} ->
                {{year, month, day}, time_from_day_fraction(day_fraction)}

              {extra_days, day_fraction} ->
                base_days = date_to_iso_days(year, month, day)
                {date_from_iso_days(base_days + extra_days), time_from_day_fraction(day_fraction)}
            end

          {:ok, {year, month, day, hour, minute, second, microsecond}, offset}
      end
    else
      _ -> {:error, :invalid_format}
    end
  end

  def parse_duration("P" <> string) when byte_size(string) > 0 do
    parse_duration_date(string, [], year: ?Y, month: ?M, week: ?W, day: ?D)
  end

  def parse_duration("+P" <> string) when byte_size(string) > 0 do
    parse_duration_date(string, [], year: ?Y, month: ?M, week: ?W, day: ?D)
  end

  def parse_duration("-P" <> string) when byte_size(string) > 0 do
    with {:ok, fields} <- parse_duration_date(string, [], year: ?Y, month: ?M, week: ?W, day: ?D) do
      {:ok,
       Enum.map(fields, fn
         {:microsecond, {value, precision}} -> {:microsecond, {-value, precision}}
         {unit, value} -> {unit, -value}
       end)}
    end
  end

  def parse_duration(_) do
    {:error, :invalid_duration}
  end

  defp parse_duration_date("", acc, _allowed), do: {:ok, acc}

  defp parse_duration_date("T" <> string, acc, _allowed) when byte_size(string) > 0 do
    parse_duration_time(string, acc, hour: ?H, minute: ?M, second: ?S)
  end

  defp parse_duration_date(string, acc, allowed) do
    with {integer, <<next, rest::binary>>} <- Integer.parse(string),
         {key, allowed} <- find_unit(allowed, next) do
      parse_duration_date(rest, [{key, integer} | acc], allowed)
    else
      _ -> {:error, :invalid_date_component}
    end
  end

  defp parse_duration_time("", acc, _allowed), do: {:ok, acc}

  defp parse_duration_time(string, acc, allowed) do
    case Integer.parse(string) do
      {second, <<delimiter, _::binary>> = rest} when delimiter in [?., ?,] ->
        case parse_microsecond(rest) do
          {{ms, precision}, "S"} ->
            ms =
              case string do
                "-" <> _ ->
                  -ms

                _ ->
                  ms
              end

            {:ok, [second: second, microsecond: {ms, precision}] ++ acc}

          _ ->
            {:error, :invalid_time_component}
        end

      {integer, <<next, rest::binary>>} ->
        case find_unit(allowed, next) do
          {key, allowed} -> parse_duration_time(rest, [{key, integer} | acc], allowed)
          false -> {:error, :invalid_time_component}
        end

      _ ->
        {:error, :invalid_time_component}
    end
  end

  defp find_unit([{key, unit} | rest], unit), do: {key, rest}
  defp find_unit([_ | rest], unit), do: find_unit(rest, unit)
  defp find_unit([], _unit), do: false

  def naive_datetime_to_iso_days(year, month, day, hour, minute, second, microsecond) do
    {date_to_iso_days(year, month, day), time_to_day_fraction(hour, minute, second, microsecond)}
  end

  def naive_datetime_from_iso_days({days, day_fraction}) do
    {year, month, day} = date_from_iso_days(days)
    {hour, minute, second, microsecond} = time_from_day_fraction(day_fraction)
    {year, month, day, hour, minute, second, microsecond}
  end

  def time_to_day_fraction(0, 0, 0, {0, _}) do
    {0, @parts_per_day}
  end

  def time_to_day_fraction(hour, minute, second, {microsecond, _}) do
    combined_seconds = hour * @seconds_per_hour + minute * @seconds_per_minute + second
    {combined_seconds * @microseconds_per_second + microsecond, @parts_per_day}
  end

  def time_from_day_fraction({0, _}) do
    {0, 0, 0, {0, 6}}
  end

  def time_from_day_fraction({parts_in_day, parts_per_day}) do
    total_microseconds = divide_by_parts_per_day(parts_in_day, parts_per_day)

    {hours, rest_microseconds1} =
      div_rem(total_microseconds, @seconds_per_hour * @microseconds_per_second)

    {minutes, rest_microseconds2} =
      div_rem(rest_microseconds1, @seconds_per_minute * @microseconds_per_second)

    {seconds, microseconds} = div_rem(rest_microseconds2, @microseconds_per_second)
    {hours, minutes, seconds, {microseconds, 6}}
  end

  defp divide_by_parts_per_day(parts_in_day, @parts_per_day), do: parts_in_day

  defp divide_by_parts_per_day(parts_in_day, parts_per_day),
    do: div(parts_in_day * @parts_per_day, parts_per_day)

  # Converts year, month, day to count of days since 0000-01-01.
  def date_to_iso_days(0, 1, 1) do
    0
  end

  def date_to_iso_days(1970, 1, 1) do
    719_528
  end

  def date_to_iso_days(year, month, day) do
    ensure_day_in_month!(year, month, day)

    days_in_previous_years(year) + days_before_month(month) + leap_day_offset(year, month) + day -
      1
  end

  # Converts count of days since 0000-01-01 to {year, month, day} tuple.
  def date_from_iso_days(days) do
    {year, day_of_year} = days_to_year(days)
    extra_day = if leap_year?(year), do: 1, else: 0
    {month, day_in_month} = year_day_to_year_date(extra_day, day_of_year)
    {year, month, day_in_month + 1}
  end

  defp div_rem(int1, int2) do
    div = div(int1, int2)
    rem = int1 - div * int2

    if rem >= 0 do
      {div, rem}
    else
      {div - 1, rem + int2}
    end
  end

  def days_in_month(year, month) when is_year(year) and is_month(month) do
    days_in_month_guarded(year, month)
  end

  defp days_in_month_guarded(year, 2) do
    if leap_year?(year), do: 29, else: 28
  end

  defp days_in_month_guarded(_, month) when month in [4, 6, 9, 11], do: 30
  defp days_in_month_guarded(_, _), do: 31

  def months_in_year(year) when is_year(year) do
    12
  end

  def leap_year?(year) when is_year(year) do
    rem(year, 4) === 0 and (rem(year, 100) !== 0 or rem(year, 400) === 0)
  end

  def day_of_week(year, month, day) do
    day_of_week(year, month, day, :default) |> elem(0)
  end

  def day_of_week(year, month, day, starting_on) do
    iso_days = date_to_iso_days(year, month, day)
    {iso_days_to_day_of_week(iso_days, starting_on), 1, 7}
  end

  def iso_days_to_day_of_week(iso_days, starting_on) do
    Integer.mod(iso_days + day_of_week_offset(starting_on), 7) + 1
  end

  defp day_of_week_offset(:default), do: 5
  defp day_of_week_offset(:wednesday), do: 3
  defp day_of_week_offset(:thursday), do: 2
  defp day_of_week_offset(:friday), do: 1
  defp day_of_week_offset(:saturday), do: 0
  defp day_of_week_offset(:sunday), do: 6
  defp day_of_week_offset(:monday), do: 5
  defp day_of_week_offset(:tuesday), do: 4

  def day_of_year(year, month, day) do
    ensure_day_in_month!(year, month, day)
    days_before_month(month) + leap_day_offset(year, month) + day
  end

  def quarter_of_year(year, month, day)
      when is_year(year) and is_month(month) and is_day(day) do
    div(month - 1, 3) + 1
  end

  def year_of_era(year) when is_year_CE(year), do: {year, 1}
  def year_of_era(year) when is_year_BCE(year), do: {abs(year) + 1, 0}

  def year_of_era(year, _month, _day), do: year_of_era(year)

  def day_of_era(year, month, day) when is_year_CE(year) do
    day = date_to_iso_days(year, month, day) - @iso_epoch + 1
    {day, 1}
  end

  def day_of_era(year, month, day) when is_year_BCE(year) do
    day = abs(date_to_iso_days(year, month, day) - @iso_epoch)
    {day, 0}
  end

  def time_to_string(
        hour,
        minute,
        second,
        {ms_value, ms_precision} = microsecond,
        format \\ :extended
      )
      when is_hour(hour) and is_minute(minute) and is_second(second) and
             is_microsecond(ms_value, ms_precision) and format in [:basic, :extended] do
    time_to_string_guarded(hour, minute, second, microsecond, format)
  end

  defp time_to_string_guarded(hour, minute, second, {_, 0}, format) do
    time_to_string_format(hour, minute, second, format)
  end

  defp time_to_string_guarded(hour, minute, second, {microsecond, precision}, format) do
    time_to_string_format(hour, minute, second, format) <>
      "." <> (microsecond |> zero_pad(6) |> binary_part(0, precision))
  end

  defp time_to_string_format(hour, minute, second, :extended) do
    zero_pad(hour, 2) <> ":" <> zero_pad(minute, 2) <> ":" <> zero_pad(second, 2)
  end

  defp time_to_string_format(hour, minute, second, :basic) do
    zero_pad(hour, 2) <> zero_pad(minute, 2) <> zero_pad(second, 2)
  end

  def date_to_string(year, month, day, format \\ :extended)
      when is_integer(year) and is_integer(month) and is_integer(day) and
             format in [:basic, :extended] do
    date_to_string_guarded(year, month, day, format)
  end

  defp date_to_string_guarded(year, month, day, :extended) do
    zero_pad(year, 4) <> "-" <> zero_pad(month, 2) <> "-" <> zero_pad(day, 2)
  end

  defp date_to_string_guarded(year, month, day, :basic) do
    zero_pad(year, 4) <> zero_pad(month, 2) <> zero_pad(day, 2)
  end

  def naive_datetime_to_string(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        format \\ :extended
      ) do
    date_to_string(year, month, day, format) <>
      " " <> time_to_string(hour, minute, second, microsecond, format)
  end

  def datetime_to_string(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        time_zone,
        zone_abbr,
        utc_offset,
        std_offset,
        format \\ :extended
      )
      when is_time_zone(time_zone) and is_zone_abbr(zone_abbr) and is_utc_offset(utc_offset) and
             is_std_offset(std_offset) do
    date_to_string(year, month, day, format) <>
      " " <>
      time_to_string(hour, minute, second, microsecond, format) <>
      offset_to_string(utc_offset, std_offset, time_zone, format) <>
      zone_to_string(utc_offset, std_offset, zone_abbr, time_zone)
  end

  def offset_to_string(0, 0, "Etc/UTC", _format), do: "Z"

  def offset_to_string(utc, std, _zone, format) do
    total = utc + std
    second = abs(total)
    minute = second |> rem(3600) |> div(60)
    hour = div(second, 3600)
    format_offset(total, hour, minute, format)
  end

  defp format_offset(total, hour, minute, :extended) do
    sign(total) <> zero_pad(hour, 2) <> ":" <> zero_pad(minute, 2)
  end

  defp format_offset(total, hour, minute, :basic) do
    sign(total) <> zero_pad(hour, 2) <> zero_pad(minute, 2)
  end

  defp zone_to_string(_, _, _, "Etc/UTC"), do: ""
  defp zone_to_string(_, _, abbr, zone), do: " " <> abbr <> " " <> zone

  def valid_date?(year, month, day)
      when is_integer(year) and is_integer(month) and is_integer(day) do
    is_month(month) and day in 1..days_in_month(year, month)
  end

  def valid_time?(hour, minute, second, {ms_value, ms_precision} = _microsecond)
      when is_integer(hour) and is_integer(minute) and is_integer(second) and is_integer(ms_value) and
             is_integer(ms_value) do
    is_hour(hour) and is_minute(minute) and is_second(second) and
      is_microsecond(ms_value, ms_precision)
  end

  def day_rollover_relative_to_midnight_utc() do
    {0, 1}
  end

  defp sign(total) when total < 0, do: "-"
  defp sign(_), do: "+"

  defp zero_pad(val, count) when val >= 0 do
    num = Integer.to_string(val)
    :binary.copy("0", max(count - byte_size(num), 0)) <> num
  end

  defp zero_pad(val, count) do
    "-" <> zero_pad(-val, count)
  end

  def iso_days_to_beginning_of_day({days, _day_fraction}) do
    {days, {0, @parts_per_day}}
  end

  def iso_days_to_end_of_day({days, _day_fraction}) do
    {days, {@parts_per_day - 1, @parts_per_day}}
  end

  def shift_date(year, month, day, duration) do
    shift_options = shift_date_options(duration)

    Enum.reduce(shift_options, {year, month, day}, fn
      {_, 0}, date ->
        date

      {:month, value}, date ->
        shift_months(date, value)

      {:day, value}, date ->
        shift_days(date, value)
    end)
  end

  def shift_naive_datetime(year, month, day, hour, minute, second, microsecond, duration) do
    shift_options = shift_datetime_options(duration)

    Enum.reduce(shift_options, {year, month, day, hour, minute, second, microsecond}, fn
      {_, 0}, naive_datetime ->
        naive_datetime

      {:month, value}, {year, month, day, hour, minute, second, microsecond} ->
        {new_year, new_month, new_day} = shift_months({year, month, day}, value)
        {new_year, new_month, new_day, hour, minute, second, microsecond}

      {time_unit, value}, naive_datetime ->
        shift_time_unit(naive_datetime, value, time_unit)
    end)
  end

  def shift_time(hour, minute, second, microsecond, duration) do
    shift_options = shift_time_options(duration)

    Enum.reduce(shift_options, {hour, minute, second, microsecond}, fn
      {_, 0}, time ->
        time

      {time_unit, value}, time ->
        shift_time_unit(time, value, time_unit)
    end)
  end

  def shift_days({year, month, day}, days) do
    {year, month, day} =
      date_to_iso_days(year, month, day)
      |> Kernel.+(days)
      |> date_from_iso_days()

    {year, month, day}
  end

  defp shift_months({year, month, day}, months) do
    months_in_year = 12
    total_months = year * months_in_year + month + months - 1

    new_year = Integer.floor_div(total_months, months_in_year)

    new_month =
      case rem(total_months, months_in_year) + 1 do
        new_month when new_month < 1 -> new_month + months_in_year
        new_month -> new_month
      end

    new_day = min(day, days_in_month(new_year, new_month))

    {new_year, new_month, new_day}
  end

  def shift_time_unit({year, month, day, hour, minute, second, microsecond}, value, unit)
      when unit in [:second, :millisecond, :microsecond, :nanosecond] or is_integer(unit) do
    {value, precision} = shift_time_unit_values(value, microsecond)

    {year, month, day, hour, minute, second, {ms_value, _}} =
      naive_datetime_to_iso_days(year, month, day, hour, minute, second, microsecond)
      |> shift_time_unit(value, unit)
      |> naive_datetime_from_iso_days()

    {year, month, day, hour, minute, second, {ms_value, precision}}
  end

  def shift_time_unit({hour, minute, second, microsecond}, value, unit)
      when unit in [:second, :millisecond, :microsecond, :nanosecond] or is_integer(unit) do
    {value, precision} = shift_time_unit_values(value, microsecond)

    {_days, day_fraction} =
      shift_time_unit({0, time_to_day_fraction(hour, minute, second, microsecond)}, value, unit)

    {hour, minute, second, {microsecond, _}} = time_from_day_fraction(day_fraction)

    {hour, minute, second, {microsecond, precision}}
  end

  def shift_time_unit({_days, _day_fraction} = iso_days, value, unit)
      when unit in [:second, :millisecond, :microsecond, :nanosecond] or is_integer(unit) do
    ppd = System.convert_time_unit(86400, :second, unit)
    add_day_fraction_to_iso_days(iso_days, value, ppd)
  end

  defp shift_time_unit_values({0, _}, {_, original_precision}) do
    {0, original_precision}
  end

  defp shift_time_unit_values({ms_value, ms_precision}, {_, _}) do
    {ms_value, ms_precision}
  end

  defp shift_time_unit_values(value, {_, original_precision}) do
    {value, original_precision}
  end

  defp shift_date_options(%Duration{
         year: year,
         month: month,
         week: week,
         day: day,
         hour: 0,
         minute: 0,
         second: 0,
         microsecond: {0, _precision}
       }) do
    [
      month: year * 12 + month,
      day: week * 7 + day
    ]
  end

  defp shift_date_options(_duration) do
    raise ArgumentError,
          "cannot shift date by time scale unit. Expected :year, :month, :week, :day"
  end

  defp shift_datetime_options(%Duration{
         year: year,
         month: month,
         week: week,
         day: day,
         hour: hour,
         minute: minute,
         second: second,
         microsecond: microsecond
       }) do
    [
      month: year * 12 + month,
      second: week * 7 * 86400 + day * 86400 + hour * 3600 + minute * 60 + second,
      microsecond: microsecond
    ]
  end

  defp shift_time_options(%Duration{
         year: 0,
         month: 0,
         week: 0,
         day: 0,
         hour: hour,
         minute: minute,
         second: second,
         microsecond: microsecond
       }) do
    [
      second: hour * 3600 + minute * 60 + second,
      microsecond: microsecond
    ]
  end

  defp shift_time_options(_duration) do
    raise ArgumentError,
          "cannot shift time by date scale unit. Expected :hour, :minute, :second, :microsecond"
  end

  ## Helpers

  def from_unix(integer, unit) when is_integer(integer) do
    total = System.convert_time_unit(integer, unit, :microsecond)

    if total in @unix_range_microseconds do
      microseconds = Integer.mod(total, @microseconds_per_second)
      seconds = @unix_epoch + Integer.floor_div(total, @microseconds_per_second)
      precision = precision_for_unit(unit)
      {date, time} = iso_seconds_to_datetime(seconds)
      {:ok, date, time, {microseconds, precision}}
    else
      {:error, :invalid_unix_time}
    end
  end

  defp precision_for_unit(unit) do
    case System.convert_time_unit(1, :second, unit) do
      1 -> 0
      10 -> 1
      100 -> 2
      1_000 -> 3
      10_000 -> 4
      100_000 -> 5
      _ -> 6
    end
  end

  defp parse_microsecond("." <> rest) do
    case parse_microsecond(rest, 0, "") do
      {"", 0, _} ->
        :error

      {microsecond, precision, rest} when precision in 1..6 ->
        pad = String.duplicate("0", 6 - byte_size(microsecond))
        {{String.to_integer(microsecond <> pad), precision}, rest}

      {microsecond, _precision, rest} ->
        {{String.to_integer(binary_part(microsecond, 0, 6)), 6}, rest}
    end
  end

  defp parse_microsecond("," <> rest) do
    parse_microsecond("." <> rest)
  end

  defp parse_microsecond(rest) do
    {{0, 0}, rest}
  end

  defp parse_microsecond(<<head, tail::binary>>, precision, acc) when head in ?0..?9,
    do: parse_microsecond(tail, precision + 1, <<acc::binary, head>>)

  defp parse_microsecond(rest, precision, acc), do: {acc, precision, rest}

  defp parse_offset(""), do: {nil, ""}
  defp parse_offset("Z"), do: {0, ""}
  defp parse_offset("-00:00"), do: :error

  defp parse_offset(<<?+, hour::2-bytes, ?:, min::2-bytes, rest::binary>>),
    do: parse_offset(1, hour, min, rest)

  defp parse_offset(<<?-, hour::2-bytes, ?:, min::2-bytes, rest::binary>>),
    do: parse_offset(-1, hour, min, rest)

  defp parse_offset(<<?+, hour::2-bytes, min::2-bytes, rest::binary>>),
    do: parse_offset(1, hour, min, rest)

  defp parse_offset(<<?-, hour::2-bytes, min::2-bytes, rest::binary>>),
    do: parse_offset(-1, hour, min, rest)

  defp parse_offset(<<?+, hour::2-bytes, rest::binary>>), do: parse_offset(1, hour, "00", rest)
  defp parse_offset(<<?-, hour::2-bytes, rest::binary>>), do: parse_offset(-1, hour, "00", rest)
  defp parse_offset(_), do: :error

  defp parse_offset(sign, hour, min, rest) do
    with {hour, ""} when hour < 24 <- Integer.parse(hour),
         {min, ""} when min < 60 <- Integer.parse(min) do
      {(hour * 60 + min) * 60 * sign, rest}
    else
      _ -> :error
    end
  end

  def gregorian_seconds_to_iso_days(seconds, microsecond) do
    {days, rest_seconds} = div_rem(seconds, @seconds_per_day)
    microseconds_in_day = rest_seconds * @microseconds_per_second + microsecond
    day_fraction = {microseconds_in_day, @parts_per_day}
    {days, day_fraction}
  end

  def iso_days_to_unit({days, {parts, ppd}}, unit) do
    day_microseconds = days * @parts_per_day
    microseconds = divide_by_parts_per_day(parts, ppd)
    System.convert_time_unit(day_microseconds + microseconds, :microsecond, unit)
  end

  def add_day_fraction_to_iso_days({days, {parts, ppd}}, add, ppd) do
    normalize_iso_days(days, parts + add, ppd)
  end

  def add_day_fraction_to_iso_days({days, {parts, ppd}}, add, add_ppd) do
    parts = parts * add_ppd
    add = add * ppd
    gcd = Integer.gcd(ppd, add_ppd)
    result_parts = div(parts + add, gcd)
    result_ppd = div(ppd * add_ppd, gcd)
    normalize_iso_days(days, result_parts, result_ppd)
  end

  defp normalize_iso_days(days, parts, ppd) do
    days_offset = div(parts, ppd)
    parts = rem(parts, ppd)

    if parts < 0 do
      {days + days_offset - 1, {parts + ppd, ppd}}
    else
      {days + days_offset, {parts, ppd}}
    end
  end

  # Note that this function does not add the extra leap day for a leap year.
  # If you want to add that leap day when appropriate,
  # add the result of leap_day_offset/2 to the result of days_before_month/1.
  defp days_before_month(1), do: 0
  defp days_before_month(2), do: 31
  defp days_before_month(3), do: 59
  defp days_before_month(4), do: 90
  defp days_before_month(5), do: 120
  defp days_before_month(6), do: 151
  defp days_before_month(7), do: 181
  defp days_before_month(8), do: 212
  defp days_before_month(9), do: 243
  defp days_before_month(10), do: 273
  defp days_before_month(11), do: 304
  defp days_before_month(12), do: 334

  defp leap_day_offset(_year, month) when month < 3, do: 0

  defp leap_day_offset(year, _month) do
    if leap_year?(year), do: 1, else: 0
  end

  defp days_to_year(days) when days < 0 do
    year_estimate = -div(-days, @days_per_nonleap_year) - 1

    {year, days_before_year} =
      days_to_year(year_estimate, days, days_to_end_of_epoch(year_estimate))

    leap_year_pad = if leap_year?(year), do: 1, else: 0
    {year, leap_year_pad + @days_per_nonleap_year + days - days_before_year}
  end

  defp days_to_year(days) do
    year_estimate = div(days, @days_per_nonleap_year)

    {year, days_before_year} =
      days_to_year(year_estimate, days, days_in_previous_years(year_estimate))

    {year, days - days_before_year}
  end

  defp days_to_year(year, days1, days2) when year < 0 and days1 >= days2 do
    days_to_year(year + 1, days1, days_to_end_of_epoch(year + 1))
  end

  defp days_to_year(year, days1, days2) when year >= 0 and days1 < days2 do
    days_to_year(year - 1, days1, days_in_previous_years(year - 1))
  end

  defp days_to_year(year, _days1, days2) do
    {year, days2}
  end

  defp days_to_end_of_epoch(year) when year < 0 do
    previous_year = year + 1

    div(previous_year, 4) - div(previous_year, 100) + div(previous_year, 400) +
      previous_year * @days_per_nonleap_year
  end

  defp days_in_previous_years(0), do: 0

  # A concise version of the algorithm would use floor_div instead of div.
  # However, floor_div would check the operands on every operation.
  # We optimize this by providing a positive and negative version of each algorithm.
  defp days_in_previous_years(year) when year > 0 do
    previous_year = year - 1

    div(previous_year, 4) - div(previous_year, 100) +
      div(previous_year, 400) + previous_year * @days_per_nonleap_year +
      @days_per_leap_year
  end

  defp days_in_previous_years(year) when year < 0 do
    previous_year = year - 1

    div(year, 4) - div(year, 100) +
      div(year, 400) - 1 + previous_year * @days_per_nonleap_year +
      @days_per_leap_year
  end

  # Note that 0 is the first day of the month.
  defp year_day_to_year_date(_extra_day, day_of_year) when day_of_year < 31 do
    {1, day_of_year}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 59 + extra_day do
    {2, day_of_year - 31}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 90 + extra_day do
    {3, day_of_year - (59 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 120 + extra_day do
    {4, day_of_year - (90 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 151 + extra_day do
    {5, day_of_year - (120 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 181 + extra_day do
    {6, day_of_year - (151 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 212 + extra_day do
    {7, day_of_year - (181 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 243 + extra_day do
    {8, day_of_year - (212 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 273 + extra_day do
    {9, day_of_year - (243 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 304 + extra_day do
    {10, day_of_year - (273 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) when day_of_year < 334 + extra_day do
    {11, day_of_year - (304 + extra_day)}
  end

  defp year_day_to_year_date(extra_day, day_of_year) do
    {12, day_of_year - (334 + extra_day)}
  end

  defp iso_seconds_to_datetime(seconds) do
    {days, rest_seconds} = div_rem(seconds, @seconds_per_day)

    date = date_from_iso_days(days)
    time = seconds_to_time(rest_seconds)
    {date, time}
  end

  defp seconds_to_time(seconds) when seconds in 0..@last_second_of_the_day do
    {hour, rest_seconds} = div_rem(seconds, @seconds_per_hour)
    {minute, second} = div_rem(rest_seconds, @seconds_per_minute)

    {hour, minute, second}
  end

  defp ensure_day_in_month!(year, month, day) when is_integer(day) do
    if day < 1 or day > days_in_month(year, month) do
      raise ArgumentError, "invalid date: #{date_to_string(year, month, day)}"
    end
  end
end

# ---- calendar/date.ex
defmodule Date do

  @enforce_keys [:year, :month, :day]
  defstruct [:year, :month, :day, calendar: Calendar.ISO]

  def range(%{calendar: calendar} = first, %{calendar: calendar} = last) do
    {first_days, _} = to_iso_days(first)
    {last_days, _} = to_iso_days(last)

    step =
      if first_days <= last_days do
        1
      else
        IO.warn(
          "a negative range was inferred for Date.range/2, call Date.range/3 instead with -1 as third argument"
        )

        -1
      end

    range(first, first_days, last, last_days, calendar, step)
  end

  def range(%{calendar: _, year: _, month: _, day: _}, %{calendar: _, year: _, month: _, day: _}) do
    raise ArgumentError, "both dates must have matching calendars"
  end

  def range(%{calendar: calendar} = first, %{calendar: calendar} = last, step)
      when is_integer(step) and step != 0 do
    {first_days, _} = to_iso_days(first)
    {last_days, _} = to_iso_days(last)
    range(first, first_days, last, last_days, calendar, step)
  end

  def range(
        %{calendar: _, year: _, month: _, day: _} = first,
        %{calendar: _, year: _, month: _, day: _} = last,
        step
      ) do
    raise ArgumentError,
          "both dates must have matching calendar and the step must be a " <>
            "non-zero integer, got: #{inspect(first)}, #{inspect(last)}, #{step}"
  end

  defp range(first, first_days, last, last_days, calendar, step) do
    %Date.Range{
      first: %Date{calendar: calendar, year: first.year, month: first.month, day: first.day},
      last: %Date{calendar: calendar, year: last.year, month: last.month, day: last.day},
      first_in_iso_days: first_days,
      last_in_iso_days: last_days,
      step: step
    }
  end

  def utc_today(calendar \\ Calendar.ISO)

  def utc_today(Calendar.ISO) do
    {:ok, {year, month, day}, _, _} = Calendar.ISO.from_unix(System.os_time(), :native)
    %Date{year: year, month: month, day: day}
  end

  def utc_today(calendar) do
    calendar
    |> DateTime.utc_now()
    |> DateTime.to_date()
  end

  def leap_year?(date)

  def leap_year?(%{calendar: calendar, year: year}) do
    calendar.leap_year?(year)
  end

  def days_in_month(date)

  def days_in_month(%{calendar: calendar, year: year, month: month}) do
    calendar.days_in_month(year, month)
  end

  def months_in_year(date)

  def months_in_year(%{calendar: calendar, year: year}) do
    calendar.months_in_year(year)
  end

  def new(year, month, day, calendar \\ Calendar.ISO) do
    if calendar.valid_date?(year, month, day) do
      {:ok, %Date{year: year, month: month, day: day, calendar: calendar}}
    else
      {:error, :invalid_date}
    end
  end

  def new!(year, month, day, calendar \\ Calendar.ISO) do
    case new(year, month, day, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError, "cannot build date, reason: #{inspect(reason)}"
    end
  end

  def to_string(date)

  def to_string(%{calendar: calendar, year: year, month: month, day: day}) do
    calendar.date_to_string(year, month, day)
  end

  def from_iso8601(string, calendar \\ Calendar.ISO) do
    with {:ok, {year, month, day}} <- Calendar.ISO.parse_date(string) do
      convert(%Date{year: year, month: month, day: day}, calendar)
    end
  end

  def from_iso8601!(string, calendar \\ Calendar.ISO) do
    case from_iso8601(string, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError, "cannot parse #{inspect(string)} as date, reason: #{inspect(reason)}"
    end
  end

  def to_iso8601(date, format \\ :extended)

  def to_iso8601(%{calendar: Calendar.ISO} = date, format) when format in [:basic, :extended] do
    %{year: year, month: month, day: day} = date
    Calendar.ISO.date_to_string(year, month, day, format)
  end

  def to_iso8601(%{calendar: _} = date, format) when format in [:basic, :extended] do
    date
    |> convert!(Calendar.ISO)
    |> to_iso8601()
  end

  def to_erl(date) do
    %{year: year, month: month, day: day} = convert!(date, Calendar.ISO)
    {year, month, day}
  end

  def from_erl(tuple, calendar \\ Calendar.ISO)

  def from_erl({year, month, day}, calendar) do
    with {:ok, date} <- new(year, month, day, Calendar.ISO), do: convert(date, calendar)
  end

  def from_erl!(tuple, calendar \\ Calendar.ISO) do
    case from_erl(tuple, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError,
              "cannot convert #{inspect(tuple)} to date, reason: #{inspect(reason)}"
    end
  end

  def from_gregorian_days(days, calendar \\ Calendar.ISO) when is_integer(days) do
    from_iso_days({days, 0}, calendar)
  end

  def to_gregorian_days(date) do
    {days, _} = to_iso_days(date)
    days
  end

  def compare(%{calendar: calendar} = date1, %{calendar: calendar} = date2) do
    %{year: year1, month: month1, day: day1} = date1
    %{year: year2, month: month2, day: day2} = date2

    case {{year1, month1, day1}, {year2, month2, day2}} do
      {first, second} when first > second -> :gt
      {first, second} when first < second -> :lt
      _ -> :eq
    end
  end

  def compare(%{calendar: calendar1} = date1, %{calendar: calendar2} = date2) do
    if Calendar.compatible_calendars?(calendar1, calendar2) do
      case {to_iso_days(date1), to_iso_days(date2)} do
        {first, second} when first > second -> :gt
        {first, second} when first < second -> :lt
        _ -> :eq
      end
    else
      raise ArgumentError, """
      cannot compare #{inspect(date1)} with #{inspect(date2)}.

      This comparison would be ambiguous as their calendars have incompatible day rollover moments.
      Specify an exact time of day (using DateTime) to resolve this ambiguity
      """
    end
  end

  def before?(date1, date2) do
    compare(date1, date2) == :lt
  end

  def after?(date1, date2) do
    compare(date1, date2) == :gt
  end

  def convert(%{calendar: calendar, year: year, month: month, day: day}, calendar) do
    {:ok, %Date{calendar: calendar, year: year, month: month, day: day}}
  end

  def convert(%{calendar: calendar} = date, target_calendar) do
    if Calendar.compatible_calendars?(calendar, target_calendar) do
      result_date =
        date
        |> to_iso_days()
        |> from_iso_days(target_calendar)

      {:ok, result_date}
    else
      {:error, :incompatible_calendars}
    end
  end

  def convert!(date, calendar) do
    case convert(date, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError,
              "cannot convert #{inspect(date)} to target calendar #{inspect(calendar)}, " <>
                "reason: #{inspect(reason)}"
    end
  end

  def add(%{calendar: Calendar.ISO} = date, days) do
    %{year: year, month: month, day: day} = date
    {year, month, day} = Calendar.ISO.shift_days({year, month, day}, days)
    %Date{calendar: Calendar.ISO, year: year, month: month, day: day}
  end

  def add(%{calendar: calendar} = date, days) do
    {base_days, fraction} = to_iso_days(date)
    from_iso_days({base_days + days, fraction}, calendar)
  end

  def diff(%{calendar: Calendar.ISO} = date1, %{calendar: Calendar.ISO} = date2) do
    %{year: year1, month: month1, day: day1} = date1
    %{year: year2, month: month2, day: day2} = date2

    Calendar.ISO.date_to_iso_days(year1, month1, day1) -
      Calendar.ISO.date_to_iso_days(year2, month2, day2)
  end

  def diff(%{calendar: calendar1} = date1, %{calendar: calendar2} = date2) do
    if Calendar.compatible_calendars?(calendar1, calendar2) do
      {days1, _} = to_iso_days(date1)
      {days2, _} = to_iso_days(date2)
      days1 - days2
    else
      raise ArgumentError,
            "cannot calculate the difference between #{inspect(date1)} and #{inspect(date2)} because their calendars are not compatible and thus the result would be ambiguous"
    end
  end

  def shift(%{calendar: calendar} = date, duration) do
    %{year: year, month: month, day: day} = date
    {year, month, day} = calendar.shift_date(year, month, day, __duration__!(duration))
    %Date{calendar: calendar, year: year, month: month, day: day}
  end

  def __duration__!(%Duration{} = duration) do
    duration
  end

  # This part is inlined by the compiler on constant values
  def __duration__!(unit_pairs) do
    Enum.each(unit_pairs, &validate_duration_unit!/1)
    struct!(Duration, unit_pairs)
  end

  defp validate_duration_unit!({unit, _value})
       when unit in [:hour, :minute, :second, :microsecond] do
    raise ArgumentError, "unsupported unit #{inspect(unit)}. Expected :year, :month, :week, :day"
  end

  defp validate_duration_unit!({unit, _value}) when unit not in [:year, :month, :week, :day] do
    raise ArgumentError, "unknown unit #{inspect(unit)}. Expected :year, :month, :week, :day"
  end

  defp validate_duration_unit!({_unit, value}) when is_integer(value) do
    :ok
  end

  defp validate_duration_unit!({unit, value}) do
    raise ArgumentError,
          "unsupported value #{inspect(value)} for #{inspect(unit)}. Expected an integer"
  end

  def to_iso_days(%{calendar: Calendar.ISO, year: year, month: month, day: day}) do
    {Calendar.ISO.date_to_iso_days(year, month, day), {0, 86_400_000_000}}
  end

  def to_iso_days(%{calendar: calendar, year: year, month: month, day: day}) do
    calendar.naive_datetime_to_iso_days(year, month, day, 0, 0, 0, {0, 0})
  end

  defp from_iso_days({days, _}, Calendar.ISO) do
    {year, month, day} = Calendar.ISO.date_from_iso_days(days)
    %Date{year: year, month: month, day: day, calendar: Calendar.ISO}
  end

  defp from_iso_days(iso_days, target_calendar) do
    {year, month, day, _, _, _, _} = target_calendar.naive_datetime_from_iso_days(iso_days)
    %Date{year: year, month: month, day: day, calendar: target_calendar}
  end

  def day_of_week(date, starting_on \\ :default)

  def day_of_week(%{calendar: calendar, year: year, month: month, day: day}, starting_on) do
    {day_of_week, _first, _last} = calendar.day_of_week(year, month, day, starting_on)
    day_of_week
  end

  def beginning_of_week(date, starting_on \\ :default)

  def beginning_of_week(%{calendar: Calendar.ISO} = date, starting_on) do
    %{year: year, month: month, day: day} = date
    iso_days = Calendar.ISO.date_to_iso_days(year, month, day)

    {year, month, day} =
      case Calendar.ISO.iso_days_to_day_of_week(iso_days, starting_on) do
        1 ->
          {year, month, day}

        day_of_week ->
          Calendar.ISO.date_from_iso_days(iso_days - day_of_week + 1)
      end

    %Date{calendar: Calendar.ISO, year: year, month: month, day: day}
  end

  def beginning_of_week(%{calendar: calendar} = date, starting_on) do
    %{year: year, month: month, day: day} = date

    case calendar.day_of_week(year, month, day, starting_on) do
      {day_of_week, day_of_week, _} ->
        %Date{calendar: calendar, year: year, month: month, day: day}

      {day_of_week, first_day_of_week, _} ->
        add(date, -(day_of_week - first_day_of_week))
    end
  end

  def end_of_week(date, starting_on \\ :default)

  def end_of_week(%{calendar: Calendar.ISO} = date, starting_on) do
    %{year: year, month: month, day: day} = date
    iso_days = Calendar.ISO.date_to_iso_days(year, month, day)

    {year, month, day} =
      case Calendar.ISO.iso_days_to_day_of_week(iso_days, starting_on) do
        7 ->
          {year, month, day}

        day_of_week ->
          Calendar.ISO.date_from_iso_days(iso_days + 7 - day_of_week)
      end

    %Date{calendar: Calendar.ISO, year: year, month: month, day: day}
  end

  def end_of_week(%{calendar: calendar} = date, starting_on) do
    %{year: year, month: month, day: day} = date

    case calendar.day_of_week(year, month, day, starting_on) do
      {day_of_week, _, day_of_week} ->
        %Date{calendar: calendar, year: year, month: month, day: day}

      {day_of_week, _, last_day_of_week} ->
        add(date, last_day_of_week - day_of_week)
    end
  end

  def day_of_year(date)

  def day_of_year(%{calendar: calendar, year: year, month: month, day: day}) do
    calendar.day_of_year(year, month, day)
  end

  def quarter_of_year(date)

  def quarter_of_year(%{calendar: calendar, year: year, month: month, day: day}) do
    calendar.quarter_of_year(year, month, day)
  end

  def year_of_era(date)

  def year_of_era(%{calendar: calendar, year: year, month: month, day: day}) do
    calendar.year_of_era(year, month, day)
  end

  def day_of_era(date)

  def day_of_era(%{calendar: calendar, year: year, month: month, day: day}) do
    calendar.day_of_era(year, month, day)
  end

  def beginning_of_month(date)

  def beginning_of_month(%{year: year, month: month, calendar: calendar}) do
    %Date{year: year, month: month, day: 1, calendar: calendar}
  end

  def end_of_month(date)

  def end_of_month(%{year: year, month: month, calendar: calendar} = date) do
    day = Date.days_in_month(date)
    %Date{year: year, month: month, day: day, calendar: calendar}
  end

  ## Helpers

  defimpl String.Chars do
    def to_string(%{calendar: calendar, year: year, month: month, day: day}) do
      calendar.date_to_string(year, month, day)
    end
  end

  defimpl Inspect do
    def inspect(%{calendar: calendar, year: year, month: month, day: day}, _)
        when calendar != Calendar.ISO or year in -9999..9999 do
      "~D[" <> calendar.date_to_string(year, month, day) <> suffix(calendar) <> "]"
    end

    def inspect(%{calendar: Calendar.ISO, year: year, month: month, day: day}, _) do
      "Date.new!(#{Integer.to_string(year)}, #{Integer.to_string(month)}, #{Integer.to_string(day)})"
    end

    def inspect(%{calendar: calendar, year: year, month: month, day: day}, _) do
      "Date.new!(#{Integer.to_string(year)}, #{Integer.to_string(month)}, #{Integer.to_string(day)}, #{inspect(calendar)})"
    end

    defp suffix(Calendar.ISO), do: ""
    defp suffix(calendar), do: " " <> inspect(calendar)
  end
end

# ---- calendar/time.ex
defmodule Time do

  @enforce_keys [:hour, :minute, :second]
  defstruct [:hour, :minute, :second, microsecond: {0, 0}, calendar: Calendar.ISO]

  @seconds_per_day 24 * 60 * 60

  def utc_now(calendar \\ Calendar.ISO) do
    {:ok, _, time, microsecond} = Calendar.ISO.from_unix(:os.system_time(), :native)
    {hour, minute, second} = time

    iso_time = %Time{
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      calendar: Calendar.ISO
    }

    convert!(iso_time, calendar)
  end

  def new(hour, minute, second, microsecond \\ {0, 0}, calendar \\ Calendar.ISO)

  def new(hour, minute, second, microsecond, calendar) when is_integer(microsecond) do
    new(hour, minute, second, {microsecond, 6}, calendar)
  end

  def new(hour, minute, second, {microsecond, precision}, calendar)
      when is_integer(hour) and is_integer(minute) and is_integer(second) and
             is_integer(microsecond) and is_integer(precision) do
    case calendar.valid_time?(hour, minute, second, {microsecond, precision}) do
      true ->
        time = %Time{
          hour: hour,
          minute: minute,
          second: second,
          microsecond: {microsecond, precision},
          calendar: calendar
        }

        {:ok, time}

      false ->
        {:error, :invalid_time}
    end
  end

  def new!(hour, minute, second, microsecond \\ {0, 0}, calendar \\ Calendar.ISO) do
    case new(hour, minute, second, microsecond, calendar) do
      {:ok, time} ->
        time

      {:error, reason} ->
        raise ArgumentError, "cannot build time, reason: #{inspect(reason)}"
    end
  end

  def to_string(time)

  def to_string(%{
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        calendar: calendar
      }) do
    calendar.time_to_string(hour, minute, second, microsecond)
  end

  def from_iso8601(string, calendar \\ Calendar.ISO) do
    with {:ok, {hour, minute, second, microsecond}} <- Calendar.ISO.parse_time(string) do
      convert(
        %Time{hour: hour, minute: minute, second: second, microsecond: microsecond},
        calendar
      )
    end
  end

  def from_iso8601!(string, calendar \\ Calendar.ISO) do
    case from_iso8601(string, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError, "cannot parse #{inspect(string)} as time, reason: #{inspect(reason)}"
    end
  end

  def to_iso8601(time, format \\ :extended)

  def to_iso8601(%{calendar: Calendar.ISO} = time, format) when format in [:extended, :basic] do
    %{
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    } = time

    Calendar.ISO.time_to_string(hour, minute, second, microsecond, format)
  end

  def to_iso8601(%{calendar: _} = time, format) when format in [:extended, :basic] do
    time
    |> convert!(Calendar.ISO)
    |> to_iso8601(format)
  end

  def to_erl(time) do
    %{hour: hour, minute: minute, second: second} = convert!(time, Calendar.ISO)
    {hour, minute, second}
  end

  def from_erl(tuple, microsecond \\ {0, 0}, calendar \\ Calendar.ISO)

  def from_erl({hour, minute, second}, microsecond, calendar) do
    with {:ok, time} <- new(hour, minute, second, microsecond, Calendar.ISO),
         do: convert(time, calendar)
  end

  def from_erl!(tuple, microsecond \\ {0, 0}, calendar \\ Calendar.ISO) do
    case from_erl(tuple, microsecond, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError,
              "cannot convert #{inspect(tuple)} to time, reason: #{inspect(reason)}"
    end
  end

  def from_seconds_after_midnight(seconds, microsecond \\ {0, 0}, calendar \\ Calendar.ISO)
      when is_integer(seconds) do
    seconds_in_day = Integer.mod(seconds, @seconds_per_day)

    {hour, minute, second, {_, _}} =
      calendar.time_from_day_fraction({seconds_in_day, @seconds_per_day})

    %Time{
      calendar: calendar,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    }
  end

  def to_seconds_after_midnight(%{microsecond: {microsecond, _precision}} = time) do
    iso_days = {0, to_day_fraction(time)}
    {Calendar.ISO.iso_days_to_unit(iso_days, :second), microsecond}
  end

  def add(time, amount_to_add, unit \\ :second)

  def add(time, amount_to_add, :hour) when is_integer(amount_to_add) do
    add(time, amount_to_add * 3600, :second)
  end

  def add(time, amount_to_add, :minute) when is_integer(amount_to_add) do
    add(time, amount_to_add * 60, :second)
  end

  def add(%{calendar: calendar, microsecond: {_, precision}} = time, amount_to_add, unit)
      when is_integer(amount_to_add) do
    valid? =
      if is_integer(unit),
        do: unit > 0,
        else: unit in ~w(second millisecond microsecond nanosecond)a

    if not valid? do
      raise ArgumentError,
            "unsupported time unit. Expected :hour, :minute, :second, :millisecond, :microsecond, :nanosecond, or a positive integer, got #{inspect(unit)}"
    end

    %{hour: hour, minute: minute, second: second, microsecond: microsecond} = time

    precision = max(Calendar.ISO.time_unit_to_precision(unit), precision)

    {hour, minute, second, {microsecond, _precision}} =
      Calendar.ISO.shift_time_unit(
        {hour, minute, second, microsecond},
        amount_to_add,
        unit
      )

    %Time{
      hour: hour,
      minute: minute,
      second: second,
      microsecond: {microsecond, precision},
      calendar: calendar
    }
  end

  def shift(%{calendar: calendar} = time, duration) do
    %{hour: hour, minute: minute, second: second, microsecond: microsecond} = time

    {hour, minute, second, microsecond} =
      calendar.shift_time(hour, minute, second, microsecond, __duration__!(duration))

    %Time{
      calendar: calendar,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    }
  end

  def __duration__!(%Duration{} = duration) do
    duration
  end

  # This part is inlined by the compiler on constant values
  def __duration__!(unit_pairs) do
    Enum.each(unit_pairs, &validate_duration_unit!/1)
    struct!(Duration, unit_pairs)
  end

  defp validate_duration_unit!({:microsecond, {ms, precision}})
       when is_integer(ms) and precision in 0..6 do
    :ok
  end

  defp validate_duration_unit!({:microsecond, microsecond}) do
    raise ArgumentError,
          "unsupported value #{inspect(microsecond)} for :microsecond. Expected a tuple {ms, precision} where precision is an integer from 0 to 6"
  end

  defp validate_duration_unit!({unit, _value}) when unit in [:year, :month, :week, :day] do
    raise ArgumentError,
          "unsupported unit #{inspect(unit)}. Expected :hour, :minute, :second, :microsecond"
  end

  defp validate_duration_unit!({unit, _value})
       when unit not in [:hour, :minute, :second, :microsecond] do
    raise ArgumentError,
          "unknown unit #{inspect(unit)}. Expected :hour, :minute, :second, :microsecond"
  end

  defp validate_duration_unit!({_unit, value}) when is_integer(value) do
    :ok
  end

  defp validate_duration_unit!({unit, value}) do
    raise ArgumentError,
          "unsupported value #{inspect(value)} for #{inspect(unit)}. Expected an integer"
  end

  def compare(%{calendar: calendar} = time1, %{calendar: calendar} = time2) do
    %{hour: hour1, minute: minute1, second: second1, microsecond: {microsecond1, _}} = time1
    %{hour: hour2, minute: minute2, second: second2, microsecond: {microsecond2, _}} = time2

    case {{hour1, minute1, second1, microsecond1}, {hour2, minute2, second2, microsecond2}} do
      {first, second} when first > second -> :gt
      {first, second} when first < second -> :lt
      _ -> :eq
    end
  end

  def compare(time1, time2) do
    {parts1, ppd1} = to_day_fraction(time1)
    {parts2, ppd2} = to_day_fraction(time2)

    case {parts1 * ppd2, parts2 * ppd1} do
      {first, second} when first > second -> :gt
      {first, second} when first < second -> :lt
      _ -> :eq
    end
  end

  def before?(time1, time2) do
    compare(time1, time2) == :lt
  end

  def after?(time1, time2) do
    compare(time1, time2) == :gt
  end

  # Keep it multiline for proper function clause errors.
  def convert(
        %{
          calendar: calendar,
          hour: hour,
          minute: minute,
          second: second,
          microsecond: microsecond
        },
        calendar
      ) do
    time = %Time{
      calendar: calendar,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    }

    {:ok, time}
  end

  def convert(%{microsecond: {_, precision}} = time, calendar) do
    {hour, minute, second, {microsecond, _}} =
      time
      |> to_day_fraction()
      |> calendar.time_from_day_fraction()

    time = %Time{
      calendar: calendar,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: {microsecond, precision}
    }

    {:ok, time}
  end

  def convert!(time, calendar) do
    {:ok, value} = convert(time, calendar)
    value
  end

  def diff(time1, time2, unit \\ :second)

  def diff(time1, time2, :hour) do
    diff(time1, time2, :second) |> div(3600)
  end

  def diff(time1, time2, :minute) do
    diff(time1, time2, :second) |> div(60)
  end

  def diff(
        %{
          calendar: Calendar.ISO,
          hour: hour1,
          minute: minute1,
          second: second1,
          microsecond: {microsecond1, _}
        },
        %{
          calendar: Calendar.ISO,
          hour: hour2,
          minute: minute2,
          second: second2,
          microsecond: {microsecond2, _}
        },
        unit
      ) do
    total =
      (hour1 - hour2) * 3_600_000_000 + (minute1 - minute2) * 60_000_000 +
        (second1 - second2) * 1_000_000 + (microsecond1 - microsecond2)

    System.convert_time_unit(total, :microsecond, unit)
  end

  def diff(time1, time2, unit) do
    fraction1 = to_day_fraction(time1)
    fraction2 = to_day_fraction(time2)

    Calendar.ISO.iso_days_to_unit({0, fraction1}, unit) -
      Calendar.ISO.iso_days_to_unit({0, fraction2}, unit)
  end

  def truncate(%Time{microsecond: microsecond} = time, precision) do
    %{time | microsecond: Calendar.truncate(microsecond, precision)}
  end

  ## Helpers

  defp to_day_fraction(%{
         hour: hour,
         minute: minute,
         second: second,
         microsecond: {_, _} = microsecond,
         calendar: calendar
       }) do
    calendar.time_to_day_fraction(hour, minute, second, microsecond)
  end

  defimpl String.Chars do
    def to_string(time) do
      %{
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        calendar: calendar
      } = time

      calendar.time_to_string(hour, minute, second, microsecond)
    end
  end

  defimpl Inspect do
    def inspect(time, _) do
      %{
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        calendar: calendar
      } = time

      "~T[" <>
        calendar.time_to_string(hour, minute, second, microsecond) <> suffix(calendar) <> "]"
    end

    defp suffix(Calendar.ISO), do: ""
    defp suffix(calendar), do: " " <> inspect(calendar)
  end
end

# ---- calendar/naive_datetime.ex
defmodule NaiveDateTime do

  @enforce_keys [:year, :month, :day, :hour, :minute, :second]
  defstruct [
    :year,
    :month,
    :day,
    :hour,
    :minute,
    :second,
    microsecond: {0, 0},
    calendar: Calendar.ISO
  ]

  @seconds_per_day 24 * 60 * 60

  def utc_now(calendar_or_time_unit \\ Calendar.ISO)

  def utc_now(time_unit) when time_unit in [:microsecond, :millisecond, :second, :native] do
    utc_now(time_unit, Calendar.ISO)
  end

  def utc_now(calendar) do
    utc_now(:native, calendar)
  end

  def utc_now(time_unit, calendar)
      when time_unit in [:native, :microsecond, :millisecond, :second] do
    {:ok, {year, month, day}, {hour, minute, second}, microsecond} =
      Calendar.ISO.from_unix(System.os_time(time_unit), time_unit)

    %NaiveDateTime{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      calendar: Calendar.ISO
    }
    |> convert!(calendar)
  end

  def local_now(calendar \\ Calendar.ISO)

  def local_now(Calendar.ISO) do
    {{year, month, day}, {hour, minute, second}} = :erlang.localtime()
    {:ok, ndt} = NaiveDateTime.new(year, month, day, hour, minute, second)
    ndt
  end

  def local_now(calendar) do
    naive_datetime = local_now()

    case convert(naive_datetime, calendar) do
      {:ok, value} ->
        value

      {:error, :incompatible_calendars} ->
        raise ArgumentError,
              ~s(cannot get "local now" in target calendar #{inspect(calendar)}, ) <>
                "reason: cannot convert from Calendar.ISO to #{inspect(calendar)}."
    end
  end

  def new(year, month, day, hour, minute, second, microsecond \\ {0, 0}, calendar \\ Calendar.ISO)

  def new(year, month, day, hour, minute, second, microsecond, calendar)
      when is_integer(microsecond) do
    new(year, month, day, hour, minute, second, {microsecond, 6}, calendar)
  end

  def new(year, month, day, hour, minute, second, microsecond, calendar) do
    cond do
      not calendar.valid_date?(year, month, day) ->
        {:error, :invalid_date}

      not calendar.valid_time?(hour, minute, second, microsecond) ->
        {:error, :invalid_time}

      true ->
        naive_datetime = %NaiveDateTime{
          calendar: calendar,
          year: year,
          month: month,
          day: day,
          hour: hour,
          minute: minute,
          second: second,
          microsecond: microsecond
        }

        {:ok, naive_datetime}
    end
  end

  def new!(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond \\ {0, 0},
        calendar \\ Calendar.ISO
      )

  def new!(year, month, day, hour, minute, second, microsecond, calendar) do
    case new(year, month, day, hour, minute, second, microsecond, calendar) do
      {:ok, naive_datetime} ->
        naive_datetime

      {:error, reason} ->
        raise ArgumentError, "cannot build naive datetime, reason: #{inspect(reason)}"
    end
  end

  def new(date, time)

  def new(%Date{calendar: calendar} = date, %Time{calendar: calendar} = time) do
    %{year: year, month: month, day: day} = date
    %{hour: hour, minute: minute, second: second, microsecond: microsecond} = time

    naive_datetime = %NaiveDateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    }

    {:ok, naive_datetime}
  end

  def new!(date, time)

  def new!(%Date{calendar: calendar} = date, %Time{calendar: calendar} = time) do
    {:ok, naive_datetime} = new(date, time)
    naive_datetime
  end

  def add(naive_datetime, amount_to_add, unit \\ :second)

  def add(naive_datetime, amount_to_add, :day) when is_integer(amount_to_add) do
    add(naive_datetime, amount_to_add * 86400, :second)
  end

  def add(naive_datetime, amount_to_add, :hour) when is_integer(amount_to_add) do
    add(naive_datetime, amount_to_add * 3600, :second)
  end

  def add(naive_datetime, amount_to_add, :minute) when is_integer(amount_to_add) do
    add(naive_datetime, amount_to_add * 60, :second)
  end

  def add(
        %{calendar: calendar, microsecond: {_, precision}} = naive_datetime,
        amount_to_add,
        unit
      )
      when is_integer(amount_to_add) do
    if not is_integer(unit) and unit not in ~w(second millisecond microsecond nanosecond)a do
      raise ArgumentError,
            "unsupported time unit. Expected :day, :hour, :minute, :second, :millisecond, :microsecond, :nanosecond, or a positive integer, got #{inspect(unit)}"
    end

    precision = max(Calendar.ISO.time_unit_to_precision(unit), precision)

    naive_datetime
    |> to_iso_days()
    |> Calendar.ISO.shift_time_unit(amount_to_add, unit)
    |> from_iso_days(calendar, precision)
  end

  def diff(naive_datetime1, naive_datetime2, unit \\ :second)

  def diff(naive_datetime1, naive_datetime2, :day) do
    diff(naive_datetime1, naive_datetime2, :second) |> div(86400)
  end

  def diff(naive_datetime1, naive_datetime2, :hour) do
    diff(naive_datetime1, naive_datetime2, :second) |> div(3600)
  end

  def diff(naive_datetime1, naive_datetime2, :minute) do
    diff(naive_datetime1, naive_datetime2, :second) |> div(60)
  end

  def diff(
        %{calendar: calendar1} = naive_datetime1,
        %{calendar: calendar2} = naive_datetime2,
        unit
      ) do
    if not Calendar.compatible_calendars?(calendar1, calendar2) do
      raise ArgumentError,
            "cannot calculate the difference between #{inspect(naive_datetime1)} and " <>
              "#{inspect(naive_datetime2)} because their calendars are not compatible " <>
              "and thus the result would be ambiguous"
    end

    if not is_integer(unit) and
         unit not in ~w(second millisecond microsecond nanosecond)a do
      raise ArgumentError,
            "unsupported time unit. Expected :day, :hour, :minute, :second, :millisecond, :microsecond, :nanosecond, or a positive integer, got #{inspect(unit)}"
    end

    units1 = naive_datetime1 |> to_iso_days() |> Calendar.ISO.iso_days_to_unit(unit)
    units2 = naive_datetime2 |> to_iso_days() |> Calendar.ISO.iso_days_to_unit(unit)
    units1 - units2
  end

  def shift(%{calendar: calendar} = naive_datetime, duration) do
    %{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    } = naive_datetime

    {year, month, day, hour, minute, second, microsecond} =
      calendar.shift_naive_datetime(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        __duration__!(duration)
      )

    %NaiveDateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    }
  end

  defdelegate __duration__!(params), to: Duration, as: :new!

  def truncate(%NaiveDateTime{microsecond: microsecond} = naive_datetime, precision) do
    %{naive_datetime | microsecond: Calendar.truncate(microsecond, precision)}
  end

  def truncate(
        %{
          calendar: calendar,
          year: year,
          month: month,
          day: day,
          hour: hour,
          minute: minute,
          second: second,
          microsecond: microsecond
        },
        precision
      ) do
    %NaiveDateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: Calendar.truncate(microsecond, precision)
    }
  end

  def to_date(%{
        year: year,
        month: month,
        day: day,
        calendar: calendar,
        hour: _,
        minute: _,
        second: _,
        microsecond: _
      }) do
    %Date{year: year, month: month, day: day, calendar: calendar}
  end

  def to_time(%{
        year: _,
        month: _,
        day: _,
        calendar: calendar,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond
      }) do
    %Time{
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      calendar: calendar
    }
  end

  def to_string(%{calendar: calendar} = naive_datetime) do
    %{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    } = naive_datetime

    calendar.naive_datetime_to_string(year, month, day, hour, minute, second, microsecond)
  end

  def from_iso8601(string, calendar \\ Calendar.ISO) do
    with {:ok, {year, month, day, hour, minute, second, microsecond}} <-
           Calendar.ISO.parse_naive_datetime(string) do
      convert(
        %NaiveDateTime{
          year: year,
          month: month,
          day: day,
          hour: hour,
          minute: minute,
          second: second,
          microsecond: microsecond
        },
        calendar
      )
    end
  end

  def from_iso8601!(string, calendar \\ Calendar.ISO) do
    case from_iso8601(string, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError,
              "cannot parse #{inspect(string)} as naive datetime, reason: #{inspect(reason)}"
    end
  end

  def to_iso8601(naive_datetime, format \\ :extended)

  def to_iso8601(%{calendar: Calendar.ISO} = naive_datetime, format)
      when format in [:basic, :extended] do
    %{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    } = naive_datetime

    Calendar.ISO.date_to_string(year, month, day, format) <>
      "T" <> Calendar.ISO.time_to_string(hour, minute, second, microsecond, format)
  end

  def to_iso8601(%{calendar: _} = naive_datetime, format) when format in [:basic, :extended] do
    naive_datetime
    |> convert!(Calendar.ISO)
    |> to_iso8601(format)
  end

  def to_erl(%{calendar: _} = naive_datetime) do
    %{year: year, month: month, day: day, hour: hour, minute: minute, second: second} =
      convert!(naive_datetime, Calendar.ISO)

    {{year, month, day}, {hour, minute, second}}
  end

  def from_erl(tuple, microsecond \\ {0, 0}, calendar \\ Calendar.ISO)

  def from_erl({{year, month, day}, {hour, minute, second}}, microsecond, calendar) do
    with {:ok, iso_naive_dt} <- new(year, month, day, hour, minute, second, microsecond),
         do: convert(iso_naive_dt, calendar)
  end

  def from_erl!(tuple, microsecond \\ {0, 0}, calendar \\ Calendar.ISO) do
    case from_erl(tuple, microsecond, calendar) do
      {:ok, value} ->
        value

      {:error, reason} ->
        raise ArgumentError,
              "cannot convert #{inspect(tuple)} to naive datetime, reason: #{inspect(reason)}"
    end
  end

  def from_gregorian_seconds(seconds, microsecond_precision \\ {0, 0}, calendar \\ Calendar.ISO)

  def from_gregorian_seconds(seconds, {microsecond, precision}, Calendar.ISO)
      when is_integer(seconds) do
    {days, seconds} = div_rem(seconds, 24 * 60 * 60)
    {hours, seconds} = div_rem(seconds, 60 * 60)
    {minutes, seconds} = div_rem(seconds, 60)
    {year, month, day} = Calendar.ISO.date_from_iso_days(days)

    %NaiveDateTime{
      calendar: Calendar.ISO,
      year: year,
      month: month,
      day: day,
      hour: hours,
      minute: minutes,
      second: seconds,
      microsecond: {microsecond, precision}
    }
  end

  def from_gregorian_seconds(seconds, {microsecond, precision}, calendar)
      when is_integer(seconds) do
    iso_days = Calendar.ISO.gregorian_seconds_to_iso_days(seconds, microsecond)

    {year, month, day, hour, minute, second, {microsecond, _}} =
      calendar.naive_datetime_from_iso_days(iso_days)

    %NaiveDateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: {microsecond, precision}
    }
  end

  defp div_rem(int1, int2) do
    div = div(int1, int2)
    rem = int1 - div * int2

    if rem >= 0 do
      {div, rem}
    else
      {div - 1, rem + int2}
    end
  end

  def to_gregorian_seconds(%{
        calendar: calendar,
        year: year,
        month: month,
        day: day,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: {microsecond, precision}
      }) do
    {days, day_fraction} =
      calendar.naive_datetime_to_iso_days(
        year,
        month,
        day,
        hour,
        minute,
        second,
        {microsecond, precision}
      )

    seconds_in_day = seconds_from_day_fraction(day_fraction)
    {days * @seconds_per_day + seconds_in_day, microsecond}
  end

  def compare(%{calendar: calendar1} = naive_datetime1, %{calendar: calendar2} = naive_datetime2) do
    if Calendar.compatible_calendars?(calendar1, calendar2) do
      case {to_iso_days(naive_datetime1), to_iso_days(naive_datetime2)} do
        {first, second} when first > second -> :gt
        {first, second} when first < second -> :lt
        _ -> :eq
      end
    else
      raise ArgumentError, """
      cannot compare #{inspect(naive_datetime1)} with #{inspect(naive_datetime2)}.

      This comparison would be ambiguous as their calendars have incompatible day rollover moments.
      Specify an exact time of day (using `DateTime`s) to resolve this ambiguity
      """
    end
  end

  def before?(naive_datetime1, naive_datetime2) do
    compare(naive_datetime1, naive_datetime2) == :lt
  end

  def after?(naive_datetime1, naive_datetime2) do
    compare(naive_datetime1, naive_datetime2) == :gt
  end

  # Keep it multiline for proper function clause errors.
  def convert(%NaiveDateTime{calendar: calendar} = ndt, calendar) do
    {:ok, ndt}
  end

  def convert(
        %{
          calendar: calendar,
          year: year,
          month: month,
          day: day,
          hour: hour,
          minute: minute,
          second: second,
          microsecond: microsecond
        },
        calendar
      ) do
    naive_datetime = %NaiveDateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    }

    {:ok, naive_datetime}
  end

  def convert(%{calendar: ndt_calendar, microsecond: {_, precision}} = naive_datetime, calendar) do
    if Calendar.compatible_calendars?(ndt_calendar, calendar) do
      result_naive_datetime =
        naive_datetime
        |> to_iso_days
        |> from_iso_days(calendar, precision)

      {:ok, result_naive_datetime}
    else
      {:error, :incompatible_calendars}
    end
  end

  def convert!(naive_datetime, calendar) do
    case convert(naive_datetime, calendar) do
      {:ok, value} ->
        value

      {:error, :incompatible_calendars} ->
        raise ArgumentError,
              "cannot convert #{inspect(naive_datetime)} to target calendar #{inspect(calendar)}, " <>
                "reason: #{inspect(naive_datetime.calendar)} and #{inspect(calendar)} " <>
                "have different day rollover moments, making this conversion ambiguous"
    end
  end

  def beginning_of_day(%{calendar: calendar, microsecond: {_, precision}} = naive_datetime) do
    naive_datetime
    |> to_iso_days()
    |> calendar.iso_days_to_beginning_of_day()
    |> from_iso_days(calendar, precision)
  end

  def end_of_day(%{calendar: calendar, microsecond: {_, precision}} = naive_datetime) do
    end_of_day =
      naive_datetime
      |> to_iso_days()
      |> calendar.iso_days_to_end_of_day()
      |> from_iso_days(calendar, precision)

    %{microsecond: {microsecond, precision}} = end_of_day

    multiplier = 10 ** (6 - precision)

    %{end_of_day | microsecond: {div(microsecond, multiplier) * multiplier, precision}}
  end

  ## Helpers

  defp seconds_from_day_fraction({parts_in_day, @seconds_per_day}),
    do: parts_in_day

  defp seconds_from_day_fraction({parts_in_day, parts_per_day}),
    do: div(parts_in_day * @seconds_per_day, parts_per_day)

  # Keep it multiline for proper function clause errors.
  defp to_iso_days(%{
         calendar: calendar,
         year: year,
         month: month,
         day: day,
         hour: hour,
         minute: minute,
         second: second,
         microsecond: microsecond
       }) do
    calendar.naive_datetime_to_iso_days(year, month, day, hour, minute, second, microsecond)
  end

  defp from_iso_days(iso_days, calendar, precision) do
    {year, month, day, hour, minute, second, {microsecond, _}} =
      calendar.naive_datetime_from_iso_days(iso_days)

    %NaiveDateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: {microsecond, precision}
    }
  end

  defimpl String.Chars do
    def to_string(naive_datetime) do
      %{
        calendar: calendar,
        year: year,
        month: month,
        day: day,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond
      } = naive_datetime

      calendar.naive_datetime_to_string(year, month, day, hour, minute, second, microsecond)
    end
  end

  defimpl Inspect do
    def inspect(naive_datetime, _) do
      %{
        year: year,
        month: month,
        day: day,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        calendar: calendar
      } = naive_datetime

      if calendar != Calendar.ISO or year in -9999..9999 do
        formatted =
          calendar.naive_datetime_to_string(year, month, day, hour, minute, second, microsecond)

        "~N[" <> formatted <> suffix(calendar) <> "]"
      else
        "NaiveDateTime.new!(#{Integer.to_string(year)}, #{Integer.to_string(month)}, #{Integer.to_string(day)}, " <>
          "#{Integer.to_string(hour)}, #{Integer.to_string(minute)}, #{Integer.to_string(second)}, #{inspect(microsecond)})"
      end
    end

    defp suffix(Calendar.ISO), do: ""
    defp suffix(calendar), do: " " <> inspect(calendar)
  end
end

# ---- calendar/datetime.ex
defmodule DateTime do

  @enforce_keys [:year, :month, :day, :hour, :minute, :second] ++
                  [:time_zone, :zone_abbr, :utc_offset, :std_offset]

  defstruct [
    :year,
    :month,
    :day,
    :hour,
    :minute,
    :second,
    :time_zone,
    :zone_abbr,
    :utc_offset,
    :std_offset,
    microsecond: {0, 0},
    calendar: Calendar.ISO
  ]

  @unix_days :calendar.date_to_gregorian_days({1970, 1, 1})
  @seconds_per_day 24 * 60 * 60

  def utc_now(calendar_or_time_unit \\ Calendar.ISO) do
    case calendar_or_time_unit do
      unit when unit in [:microsecond, :millisecond, :second, :native] ->
        utc_now(unit, Calendar.ISO)

      calendar ->
        System.os_time() |> from_unix!(:native, calendar)
    end
  end

  def utc_now(time_unit, calendar)
      when time_unit in [:native, :microsecond, :millisecond, :second] do
    System.os_time(time_unit) |> from_unix!(time_unit, calendar)
  end

  def new(
        date,
        time,
        time_zone \\ "Etc/UTC",
        time_zone_database \\ Calendar.get_time_zone_database()
      )

  def new(%Date{calendar: calendar} = date, %Time{calendar: calendar} = time, "Etc/UTC", _db) do
    %{year: year, month: month, day: day} = date
    %{hour: hour, minute: minute, second: second, microsecond: microsecond} = time

    datetime = %DateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      std_offset: 0,
      utc_offset: 0,
      zone_abbr: "UTC",
      time_zone: "Etc/UTC"
    }

    {:ok, datetime}
  end

  def new(date, time, time_zone, time_zone_database) do
    with {:ok, naive_datetime} <- NaiveDateTime.new(date, time) do
      from_naive(naive_datetime, time_zone, time_zone_database)
    end
  end

  def new!(
        date,
        time,
        time_zone \\ "Etc/UTC",
        time_zone_database \\ Calendar.get_time_zone_database()
      )

  def new!(date, time, time_zone, time_zone_database) do
    case new(date, time, time_zone, time_zone_database) do
      {:ok, datetime} ->
        datetime

      {:ambiguous, dt1, dt2} ->
        raise ArgumentError,
              "cannot build datetime with #{inspect(date)} and #{inspect(time)} because such " <>
                "instant is ambiguous in time zone #{time_zone} as there is an overlap " <>
                "between #{inspect(dt1)} and #{inspect(dt2)}"

      {:gap, dt1, dt2} ->
        raise ArgumentError,
              "cannot build datetime with #{inspect(date)} and #{inspect(time)} because such " <>
                "instant does not exist in time zone #{time_zone} as there is a gap " <>
                "between #{inspect(dt1)} and #{inspect(dt2)}"

      {:error, reason} ->
        raise ArgumentError,
              "cannot build datetime with #{inspect(date)} and #{inspect(time)}, reason: #{inspect(reason)}"
    end
  end

  def from_unix(integer, unit \\ :second, calendar \\ Calendar.ISO) when is_integer(integer) do
    case Calendar.ISO.from_unix(integer, unit) do
      {:ok, {year, month, day}, {hour, minute, second}, microsecond} ->
        iso_datetime = %DateTime{
          year: year,
          month: month,
          day: day,
          hour: hour,
          minute: minute,
          second: second,
          microsecond: microsecond,
          std_offset: 0,
          utc_offset: 0,
          zone_abbr: "UTC",
          time_zone: "Etc/UTC"
        }

        convert(iso_datetime, calendar)

      {:error, _} = error ->
        error
    end
  end

  def from_unix!(integer, unit \\ :second, calendar \\ Calendar.ISO) do
    case from_unix(integer, unit, calendar) do
      {:ok, datetime} ->
        datetime

      {:error, :invalid_unix_time} ->
        raise ArgumentError, "invalid Unix time #{integer}"
    end
  end

  def from_naive(
        naive_datetime,
        time_zone,
        time_zone_database \\ Calendar.get_time_zone_database()
      )

  def from_naive(naive_datetime, "Etc/UTC", _) do
    utc_period = %{std_offset: 0, utc_offset: 0, zone_abbr: "UTC"}
    {:ok, from_naive_with_period(naive_datetime, "Etc/UTC", utc_period)}
  end

  def from_naive(%{calendar: Calendar.ISO} = naive_datetime, time_zone, time_zone_database) do
    case time_zone_database.time_zone_periods_from_wall_datetime(naive_datetime, time_zone) do
      {:ok, period} ->
        {:ok, from_naive_with_period(naive_datetime, time_zone, period)}

      {:ambiguous, first_period, second_period} ->
        first_datetime = from_naive_with_period(naive_datetime, time_zone, first_period)
        second_datetime = from_naive_with_period(naive_datetime, time_zone, second_period)
        {:ambiguous, first_datetime, second_datetime}

      {:gap, {first_period, first_period_until_wall}, {second_period, second_period_from_wall}} ->
        # `until_wall` is not valid, but any time just before is.
        # So by subtracting a second and adding .999999 seconds
        # we get the last microsecond just before.
        before_naive =
          first_period_until_wall
          |> Map.replace!(:microsecond, {999_999, 6})
          |> NaiveDateTime.add(-1)

        after_naive = second_period_from_wall

        latest_datetime_before = from_naive_with_period(before_naive, time_zone, first_period)
        first_datetime_after = from_naive_with_period(after_naive, time_zone, second_period)
        {:gap, latest_datetime_before, first_datetime_after}

      {:error, _} = error ->
        error
    end
  end

  def from_naive(%{calendar: calendar} = naive_datetime, time_zone, time_zone_database)
      when calendar != Calendar.ISO do
    # For non-ISO calendars, convert to ISO, create ISO DateTime, and then
    # convert to original calendar
    iso_result =
      with {:ok, in_iso} <- NaiveDateTime.convert(naive_datetime, Calendar.ISO) do
        from_naive(in_iso, time_zone, time_zone_database)
      end

    case iso_result do
      {:ok, dt} ->
        convert(dt, calendar)

      {:ambiguous, dt1, dt2} ->
        with {:ok, dt1converted} <- convert(dt1, calendar),
             {:ok, dt2converted} <- convert(dt2, calendar),
             do: {:ambiguous, dt1converted, dt2converted}

      {:gap, dt1, dt2} ->
        with {:ok, dt1converted} <- convert(dt1, calendar),
             {:ok, dt2converted} <- convert(dt2, calendar),
             do: {:gap, dt1converted, dt2converted}

      {:error, _} = error ->
        error
    end
  end

  defp from_naive_with_period(naive_datetime, time_zone, period) do
    %{std_offset: std_offset, utc_offset: utc_offset, zone_abbr: zone_abbr} = period

    %{
      calendar: calendar,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      year: year,
      month: month,
      day: day
    } = naive_datetime

    %DateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      std_offset: std_offset,
      utc_offset: utc_offset,
      zone_abbr: zone_abbr,
      time_zone: time_zone
    }
  end

  def from_naive!(
        naive_datetime,
        time_zone,
        time_zone_database \\ Calendar.get_time_zone_database()
      ) do
    case from_naive(naive_datetime, time_zone, time_zone_database) do
      {:ok, datetime} ->
        datetime

      {:ambiguous, dt1, dt2} ->
        raise ArgumentError,
              "cannot convert #{inspect(naive_datetime)} to datetime because such " <>
                "instant is ambiguous in time zone #{time_zone} as there is an overlap " <>
                "between #{inspect(dt1)} and #{inspect(dt2)}"

      {:gap, dt1, dt2} ->
        raise ArgumentError,
              "cannot convert #{inspect(naive_datetime)} to datetime because such " <>
                "instant does not exist in time zone #{time_zone} as there is a gap " <>
                "between #{inspect(dt1)} and #{inspect(dt2)}"

      {:error, reason} ->
        raise ArgumentError,
              "cannot convert #{inspect(naive_datetime)} to datetime, reason: #{inspect(reason)}"
    end
  end

  def shift_zone(datetime, time_zone, time_zone_database \\ Calendar.get_time_zone_database())

  def shift_zone(%{time_zone: time_zone} = datetime, time_zone, _) do
    {:ok, datetime}
  end

  def shift_zone(datetime, time_zone, time_zone_database) do
    %{
      std_offset: std_offset,
      utc_offset: utc_offset,
      calendar: calendar,
      microsecond: {_, precision}
    } = datetime

    datetime
    |> to_iso_days()
    |> apply_tz_offset(utc_offset + std_offset)
    |> shift_zone_for_iso_days_utc(calendar, precision, time_zone, time_zone_database)
  end

  defp shift_zone_for_iso_days_utc(iso_days_utc, calendar, precision, time_zone, time_zone_db) do
    case time_zone_db.time_zone_period_from_utc_iso_days(iso_days_utc, time_zone) do
      {:ok, %{std_offset: std_offset, utc_offset: utc_offset, zone_abbr: zone_abbr}} ->
        {year, month, day, hour, minute, second, {microsecond_without_precision, _}} =
          iso_days_utc
          |> apply_tz_offset(-(utc_offset + std_offset))
          |> calendar.naive_datetime_from_iso_days()

        datetime = %DateTime{
          calendar: calendar,
          year: year,
          month: month,
          day: day,
          hour: hour,
          minute: minute,
          second: second,
          microsecond: {microsecond_without_precision, precision},
          std_offset: std_offset,
          utc_offset: utc_offset,
          zone_abbr: zone_abbr,
          time_zone: time_zone
        }

        {:ok, datetime}

      {:error, _} = error ->
        error
    end
  end

  def shift_zone!(datetime, time_zone, time_zone_database \\ Calendar.get_time_zone_database()) do
    case shift_zone(datetime, time_zone, time_zone_database) do
      {:ok, datetime} ->
        datetime

      {:error, reason} ->
        raise ArgumentError,
              "cannot shift #{inspect(datetime)} to #{inspect(time_zone)} time zone" <>
                ", reason: #{inspect(reason)}"
    end
  end

  def now(time_zone, time_zone_database \\ Calendar.get_time_zone_database())

  def now("Etc/UTC", _) do
    {:ok, utc_now()}
  end

  def now(time_zone, time_zone_database) do
    shift_zone(utc_now(), time_zone, time_zone_database)
  end

  def now!(time_zone, time_zone_database \\ Calendar.get_time_zone_database()) do
    case now(time_zone, time_zone_database) do
      {:ok, datetime} ->
        datetime

      {:error, reason} ->
        raise ArgumentError,
              "cannot get current datetime in #{inspect(time_zone)} time zone, reason: " <>
                inspect(reason)
    end
  end

  def to_unix(datetime, unit \\ :second)

  def to_unix(%{utc_offset: utc_offset, std_offset: std_offset} = datetime, unit) do
    {days, fraction} = to_iso_days(datetime)
    unix_units = Calendar.ISO.iso_days_to_unit({days - @unix_days, fraction}, unit)
    offset_units = System.convert_time_unit(utc_offset + std_offset, :second, unit)
    unix_units - offset_units
  end

  def to_naive(datetime)

  def to_naive(%{
        calendar: calendar,
        year: year,
        month: month,
        day: day,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        time_zone: _
      }) do
    %NaiveDateTime{
      year: year,
      month: month,
      day: day,
      calendar: calendar,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    }
  end

  def to_date(datetime)

  def to_date(%{
        year: year,
        month: month,
        day: day,
        calendar: calendar,
        hour: _,
        minute: _,
        second: _,
        microsecond: _,
        time_zone: _
      }) do
    %Date{year: year, month: month, day: day, calendar: calendar}
  end

  def to_time(datetime)

  def to_time(%{
        year: _,
        month: _,
        day: _,
        calendar: calendar,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        time_zone: _
      }) do
    %Time{
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      calendar: calendar
    }
  end

  def to_iso8601(datetime, format \\ :extended, offset \\ nil)

  def to_iso8601(%{calendar: Calendar.ISO} = datetime, format, nil)
      when format in [:extended, :basic] do
    %{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      time_zone: time_zone,
      utc_offset: utc_offset,
      std_offset: std_offset
    } = datetime

    datetime_to_string(year, month, day, hour, minute, second, microsecond, format) <>
      Calendar.ISO.offset_to_string(utc_offset, std_offset, time_zone, format)
  end

  def to_iso8601(
        %{calendar: Calendar.ISO, microsecond: {_, precision}, time_zone: "Etc/UTC"} = datetime,
        format,
        0
      )
      when format in [:extended, :basic] do
    {year, month, day, hour, minute, second, {microsecond, _}} = shift_by_offset(datetime, 0)

    datetime_to_string(year, month, day, hour, minute, second, {microsecond, precision}, format) <>
      "Z"
  end

  def to_iso8601(%{calendar: Calendar.ISO} = datetime, format, offset)
      when format in [:extended, :basic] do
    {_, precision} = datetime.microsecond
    {year, month, day, hour, minute, second, {microsecond, _}} = shift_by_offset(datetime, offset)

    datetime_to_string(year, month, day, hour, minute, second, {microsecond, precision}, format) <>
      Calendar.ISO.offset_to_string(offset, 0, nil, format)
  end

  def to_iso8601(%{calendar: _} = datetime, format, offset) when format in [:extended, :basic] do
    datetime
    |> convert!(Calendar.ISO)
    |> to_iso8601(format, offset)
  end

  defp shift_by_offset(%{calendar: calendar} = datetime, offset) do
    total_offset = datetime.utc_offset + datetime.std_offset

    datetime
    |> to_iso_days()
    # Subtract total original offset in order to get UTC and add the new offset
    |> Calendar.ISO.add_day_fraction_to_iso_days(offset - total_offset, 86400)
    |> calendar.naive_datetime_from_iso_days()
  end

  defp datetime_to_string(year, month, day, hour, minute, second, microsecond, format) do
    Calendar.ISO.date_to_string(year, month, day, format) <>
      "T" <>
      Calendar.ISO.time_to_string(hour, minute, second, microsecond, format)
  end

  def from_iso8601(string, format_or_calendar \\ Calendar.ISO)

  def from_iso8601(string, format) when format in [:basic, :extended] do
    from_iso8601(string, Calendar.ISO, format)
  end

  def from_iso8601(string, calendar) when is_atom(calendar) do
    from_iso8601(string, calendar, :extended)
  end

  def from_iso8601(string, calendar, format) do
    with {:ok, {year, month, day, hour, minute, second, microsecond}, offset} <-
           Calendar.ISO.parse_utc_datetime(string, format) do
      datetime = %DateTime{
        year: year,
        month: month,
        day: day,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        std_offset: 0,
        utc_offset: 0,
        zone_abbr: "UTC",
        time_zone: "Etc/UTC"
      }

      with {:ok, converted} <- convert(datetime, calendar) do
        {:ok, converted, offset}
      end
    end
  end

  def from_gregorian_seconds(
        seconds,
        {microsecond, precision} \\ {0, 0},
        calendar \\ Calendar.ISO
      )
      when is_integer(seconds) do
    iso_days = Calendar.ISO.gregorian_seconds_to_iso_days(seconds, microsecond)

    {year, month, day, hour, minute, second, {microsecond, _}} =
      calendar.naive_datetime_from_iso_days(iso_days)

    %DateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: {microsecond, precision},
      std_offset: 0,
      utc_offset: 0,
      zone_abbr: "UTC",
      time_zone: "Etc/UTC"
    }
  end

  def to_gregorian_seconds(
        %{
          std_offset: std_offset,
          utc_offset: utc_offset,
          microsecond: {microsecond, _}
        } = datetime
      ) do
    {days, day_fraction} =
      datetime
      |> to_iso_days()
      |> apply_tz_offset(utc_offset + std_offset)

    seconds_in_day = seconds_from_day_fraction(day_fraction)
    {days * @seconds_per_day + seconds_in_day, microsecond}
  end

  def to_string(%{calendar: calendar} = datetime) do
    %{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      time_zone: time_zone,
      zone_abbr: zone_abbr,
      utc_offset: utc_offset,
      std_offset: std_offset
    } = datetime

    calendar.datetime_to_string(
      year,
      month,
      day,
      hour,
      minute,
      second,
      microsecond,
      time_zone,
      zone_abbr,
      utc_offset,
      std_offset
    )
  end

  def compare(
        %{utc_offset: utc_offset1, std_offset: std_offset1} = datetime1,
        %{utc_offset: utc_offset2, std_offset: std_offset2} = datetime2
      ) do
    {days1, {parts1, ppd1}} =
      datetime1
      |> to_iso_days()
      |> apply_tz_offset(utc_offset1 + std_offset1)

    {days2, {parts2, ppd2}} =
      datetime2
      |> to_iso_days()
      |> apply_tz_offset(utc_offset2 + std_offset2)

    # Ensure fraction tuples have same denominator.
    first = {days1, parts1 * ppd2}
    second = {days2, parts2 * ppd1}

    cond do
      first > second -> :gt
      first < second -> :lt
      true -> :eq
    end
  end

  def before?(datetime1, datetime2) do
    compare(datetime1, datetime2) == :lt
  end

  def after?(datetime1, datetime2) do
    compare(datetime1, datetime2) == :gt
  end

  def diff(datetime1, datetime2, unit \\ :second)

  def diff(datetime1, datetime2, :day) do
    diff(datetime1, datetime2, :second) |> div(86400)
  end

  def diff(datetime1, datetime2, :hour) do
    diff(datetime1, datetime2, :second) |> div(3600)
  end

  def diff(datetime1, datetime2, :minute) do
    diff(datetime1, datetime2, :second) |> div(60)
  end

  def diff(
        %{utc_offset: utc_offset1, std_offset: std_offset1} = datetime1,
        %{utc_offset: utc_offset2, std_offset: std_offset2} = datetime2,
        unit
      ) do
    if not is_integer(unit) and
         unit not in ~w(second millisecond microsecond nanosecond)a do
      raise ArgumentError,
            "unsupported time unit. Expected :day, :hour, :minute, :second, :millisecond, :microsecond, :nanosecond, or a positive integer, got #{inspect(unit)}"
    end

    naive_diff =
      (datetime1 |> to_iso_days() |> Calendar.ISO.iso_days_to_unit(unit)) -
        (datetime2 |> to_iso_days() |> Calendar.ISO.iso_days_to_unit(unit))

    offset_diff = utc_offset2 + std_offset2 - (utc_offset1 + std_offset1)
    naive_diff + System.convert_time_unit(offset_diff, :second, unit)
  end

  def add(
        datetime,
        amount_to_add,
        unit \\ :second,
        time_zone_database \\ Calendar.get_time_zone_database()
      )

  def add(datetime, amount_to_add, :day, time_zone_database) when is_integer(amount_to_add) do
    add(datetime, amount_to_add * 86400, :second, time_zone_database)
  end

  def add(datetime, amount_to_add, :hour, time_zone_database) when is_integer(amount_to_add) do
    add(datetime, amount_to_add * 3600, :second, time_zone_database)
  end

  def add(datetime, amount_to_add, :minute, time_zone_database) when is_integer(amount_to_add) do
    add(datetime, amount_to_add * 60, :second, time_zone_database)
  end

  def add(%{calendar: calendar} = datetime, amount_to_add, unit, time_zone_database)
      when is_integer(amount_to_add) do
    %{
      microsecond: {_, precision},
      time_zone: time_zone,
      utc_offset: utc_offset,
      std_offset: std_offset
    } = datetime

    if not is_integer(unit) and unit not in ~w(second millisecond microsecond nanosecond)a do
      raise ArgumentError,
            "unsupported time unit. Expected :day, :hour, :minute, :second, :millisecond, :microsecond, :nanosecond, or a positive integer, got #{inspect(unit)}"
    end

    precision = max(Calendar.ISO.time_unit_to_precision(unit), precision)

    result =
      datetime
      |> to_iso_days()
      |> Calendar.ISO.shift_time_unit(amount_to_add, unit)
      |> apply_tz_offset(utc_offset + std_offset)
      |> shift_zone_for_iso_days_utc(calendar, precision, time_zone, time_zone_database)

    case result do
      {:ok, result_datetime} ->
        result_datetime

      {:error, error} ->
        raise ArgumentError,
              "cannot add #{amount_to_add} #{unit} to #{inspect(datetime)} (with time zone " <>
                "database #{inspect(time_zone_database)}), reason: #{inspect(error)}"
    end
  end

  def shift(datetime, duration, time_zone_database \\ Calendar.get_time_zone_database())

  def shift(%{calendar: calendar, time_zone: "Etc/UTC"} = datetime, duration, _time_zone_database) do
    %{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond
    } = datetime

    {year, month, day, hour, minute, second, microsecond} =
      calendar.shift_naive_datetime(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        __duration__!(duration)
      )

    %DateTime{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      time_zone: "Etc/UTC",
      zone_abbr: "UTC",
      std_offset: 0,
      utc_offset: 0
    }
  end

  def shift(%{calendar: calendar} = datetime, duration, time_zone_database) do
    %{
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: microsecond,
      std_offset: std_offset,
      utc_offset: utc_offset,
      time_zone: time_zone
    } = datetime

    {year, month, day, hour, minute, second, {_, precision} = microsecond} =
      calendar.shift_naive_datetime(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        __duration__!(duration)
      )

    result =
      calendar.naive_datetime_to_iso_days(year, month, day, hour, minute, second, microsecond)
      |> apply_tz_offset(utc_offset + std_offset)
      |> shift_zone_for_iso_days_utc(calendar, precision, time_zone, time_zone_database)

    case result do
      {:ok, result_datetime} ->
        result_datetime

      {:error, error} ->
        raise ArgumentError,
              "cannot shift #{inspect(datetime)} to #{inspect(duration)} (with time zone " <>
                "database #{inspect(time_zone_database)}), reason: #{inspect(error)}"
    end
  end

  defdelegate __duration__!(params), to: Duration, as: :new!

  def truncate(%DateTime{microsecond: microsecond} = datetime, precision) do
    %{datetime | microsecond: Calendar.truncate(microsecond, precision)}
  end

  def truncate(%{} = datetime_map, precision) do
    truncate(from_map(datetime_map), precision)
  end

  def convert(%DateTime{calendar: calendar} = datetime, calendar) do
    {:ok, datetime}
  end

  def convert(%{calendar: calendar} = datetime, calendar) do
    {:ok, from_map(datetime)}
  end

  def convert(%{calendar: dt_calendar, microsecond: {_, precision}} = datetime, calendar) do
    if Calendar.compatible_calendars?(dt_calendar, calendar) do
      result_datetime =
        datetime
        |> to_iso_days
        |> from_iso_days(datetime, calendar, precision)

      {:ok, result_datetime}
    else
      {:error, :incompatible_calendars}
    end
  end

  def convert!(datetime, calendar) do
    case convert(datetime, calendar) do
      {:ok, value} ->
        value

      {:error, :incompatible_calendars} ->
        raise ArgumentError,
              "cannot convert #{inspect(datetime)} to target calendar #{inspect(calendar)}, " <>
                "reason: #{inspect(datetime.calendar)} and #{inspect(calendar)} have different " <>
                "day rollover moments, making this conversion ambiguous"
    end
  end

  # Keep it multiline for proper function clause errors.
  defp to_iso_days(%{
         calendar: calendar,
         year: year,
         month: month,
         day: day,
         hour: hour,
         minute: minute,
         second: second,
         microsecond: microsecond
       }) do
    calendar.naive_datetime_to_iso_days(year, month, day, hour, minute, second, microsecond)
  end

  defp from_iso_days(iso_days, datetime, calendar, precision) do
    %{time_zone: time_zone, zone_abbr: zone_abbr, utc_offset: utc_offset, std_offset: std_offset} =
      datetime

    {year, month, day, hour, minute, second, {microsecond, _}} =
      calendar.naive_datetime_from_iso_days(iso_days)

    %DateTime{
      calendar: calendar,
      year: year,
      month: month,
      day: day,
      hour: hour,
      minute: minute,
      second: second,
      microsecond: {microsecond, precision},
      time_zone: time_zone,
      zone_abbr: zone_abbr,
      utc_offset: utc_offset,
      std_offset: std_offset
    }
  end

  defp apply_tz_offset(iso_days, 0) do
    iso_days
  end

  defp apply_tz_offset(iso_days, offset) do
    Calendar.ISO.add_day_fraction_to_iso_days(iso_days, -offset, 86400)
  end

  defp from_map(%{} = datetime_map) do
    %DateTime{
      year: datetime_map.year,
      month: datetime_map.month,
      day: datetime_map.day,
      hour: datetime_map.hour,
      minute: datetime_map.minute,
      second: datetime_map.second,
      microsecond: datetime_map.microsecond,
      time_zone: datetime_map.time_zone,
      zone_abbr: datetime_map.zone_abbr,
      utc_offset: datetime_map.utc_offset,
      std_offset: datetime_map.std_offset
    }
  end

  defp seconds_from_day_fraction({parts_in_day, @seconds_per_day}),
    do: parts_in_day

  defp seconds_from_day_fraction({parts_in_day, parts_per_day}),
    do: div(parts_in_day * @seconds_per_day, parts_per_day)

  defimpl String.Chars do
    def to_string(datetime) do
      %{
        calendar: calendar,
        year: year,
        month: month,
        day: day,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        time_zone: time_zone,
        zone_abbr: zone_abbr,
        utc_offset: utc_offset,
        std_offset: std_offset
      } = datetime

      calendar.datetime_to_string(
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
        time_zone,
        zone_abbr,
        utc_offset,
        std_offset
      )
    end
  end

  defimpl Inspect do
    def inspect(datetime, _) do
      %{
        year: year,
        month: month,
        day: day,
        hour: hour,
        minute: minute,
        second: second,
        microsecond: microsecond,
        time_zone: time_zone,
        zone_abbr: zone_abbr,
        utc_offset: utc_offset,
        std_offset: std_offset,
        calendar: calendar
      } = datetime

      formatted =
        calendar.datetime_to_string(
          year,
          month,
          day,
          hour,
          minute,
          second,
          microsecond,
          time_zone,
          zone_abbr,
          utc_offset,
          std_offset
        )

      case datetime do
        %{utc_offset: 0, std_offset: 0, time_zone: "Etc/UTC", year: year}
        when calendar != Calendar.ISO or year in -9999..9999 ->
          "~U[" <> formatted <> suffix(calendar) <> "]"

        _ ->
          "#DateTime<" <> formatted <> suffix(calendar) <> ">"
      end
    end

    defp suffix(Calendar.ISO), do: ""
    defp suffix(calendar), do: " " <> inspect(calendar)
  end
end

# ---- calendar/date_range.ex
defmodule Date.Range do

  @enforce_keys [:first, :last, :first_in_iso_days, :last_in_iso_days, :step]
  defstruct [:first, :last, :first_in_iso_days, :last_in_iso_days, :step]

  defimpl Enumerable do
    def member?(
          %Date.Range{
            first: %{calendar: calendar},
            first_in_iso_days: first_days,
            last_in_iso_days: last_days,
            step: step
          } = range,
          %Date{calendar: calendar} = date
        ) do
      {days, _} = Date.to_iso_days(date)

      cond do
        empty?(range) ->
          {:ok, false}

        first_days <= last_days ->
          {:ok, first_days <= days and days <= last_days and rem(days - first_days, step) == 0}

        true ->
          {:ok, last_days <= days and days <= first_days and rem(days - first_days, step) == 0}
      end
    end

    def member?(%Date.Range{step: _}, _) do
      {:ok, false}
    end

    # TODO: Remove me on v2.0
    def member?(
          %{__struct__: Date.Range, first_in_iso_days: first_days, last_in_iso_days: last_days} =
            date_range,
          date
        ) do
      step = if first_days <= last_days, do: 1, else: -1
      member?(Map.put(date_range, :step, step), date)
    end

    def count(range) do
      {:ok, size(range)}
    end

    def slice(
          %Date.Range{
            first_in_iso_days: first,
            first: %{calendar: calendar},
            step: step
          } = range
        ) do
      {:ok, size(range), &slice(first + &1 * step, step + &3 - 1, &2, calendar)}
    end

    # TODO: Remove me on v2.0
    def slice(
          %{__struct__: Date.Range, first_in_iso_days: first_days, last_in_iso_days: last_days} =
            date_range
        ) do
      step = if first_days <= last_days, do: 1, else: -1
      slice(Map.put(date_range, :step, step))
    end

    defp slice(current, _step, 1, calendar) do
      [date_from_iso_days(current, calendar)]
    end

    defp slice(current, step, remaining, calendar) do
      [
        date_from_iso_days(current, calendar)
        | slice(current + step, step, remaining - 1, calendar)
      ]
    end

    def reduce(
          %Date.Range{
            first_in_iso_days: first_days,
            last_in_iso_days: last_days,
            first: %{calendar: calendar},
            step: step
          },
          acc,
          fun
        ) do
      reduce(first_days, last_days, acc, fun, step, calendar)
    end

    # TODO: Remove me on v2.0
    def reduce(
          %{__struct__: Date.Range, first_in_iso_days: first_days, last_in_iso_days: last_days} =
            date_range,
          acc,
          fun
        ) do
      step = if first_days <= last_days, do: 1, else: -1
      reduce(Map.put(date_range, :step, step), acc, fun)
    end

    defp reduce(_first_days, _last_days, {:halt, acc}, _fun, _step, _calendar) do
      {:halted, acc}
    end

    defp reduce(first_days, last_days, {:suspend, acc}, fun, step, calendar) do
      {:suspended, acc, &reduce(first_days, last_days, &1, fun, step, calendar)}
    end

    defp reduce(first_days, last_days, {:cont, acc}, fun, step, calendar)
         when step > 0 and first_days <= last_days
         when step < 0 and first_days >= last_days do
      reduce(
        first_days + step,
        last_days,
        fun.(date_from_iso_days(first_days, calendar), acc),
        fun,
        step,
        calendar
      )
    end

    defp reduce(_, _, {:cont, acc}, _fun, _step, _calendar) do
      {:done, acc}
    end

    defp date_from_iso_days(days, Calendar.ISO) do
      {year, month, day} = Calendar.ISO.date_from_iso_days(days)
      %Date{year: year, month: month, day: day, calendar: Calendar.ISO}
    end

    defp date_from_iso_days(days, calendar) do
      {year, month, day, _, _, _, _} =
        calendar.naive_datetime_from_iso_days({days, {0, 86_400_000_000}})

      %Date{year: year, month: month, day: day, calendar: calendar}
    end

    defp size(%Date.Range{first_in_iso_days: first_days, last_in_iso_days: last_days, step: step})
         when step > 0 and first_days > last_days,
         do: 0

    defp size(%Date.Range{first_in_iso_days: first_days, last_in_iso_days: last_days, step: step})
         when step < 0 and first_days < last_days,
         do: 0

    defp size(%Date.Range{first_in_iso_days: first_days, last_in_iso_days: last_days, step: step}),
      do: abs(div(last_days - first_days, step)) + 1

    # TODO: Remove me on v2.0
    defp size(
           %{__struct__: Date.Range, first_in_iso_days: first_days, last_in_iso_days: last_days} =
             date_range
         ) do
      step = if first_days <= last_days, do: 1, else: -1
      size(Map.put(date_range, :step, step))
    end

    defp empty?(%Date.Range{
           first_in_iso_days: first_days,
           last_in_iso_days: last_days,
           step: step
         })
         when step > 0 and first_days > last_days,
         do: true

    defp empty?(%Date.Range{
           first_in_iso_days: first_days,
           last_in_iso_days: last_days,
           step: step
         })
         when step < 0 and first_days < last_days,
         do: true

    defp empty?(%Date.Range{step: _}), do: false

    # TODO: Remove me on v2.0
    defp empty?(
           %{__struct__: Date.Range, first_in_iso_days: first_days, last_in_iso_days: last_days} =
             date_range
         ) do
      step = if first_days <= last_days, do: 1, else: -1
      empty?(Map.put(date_range, :step, step))
    end
  end

  defimpl Inspect do
    import Kernel, except: [inspect: 2]

    def inspect(%Date.Range{first: first, last: last, step: 1}, _) do
      "Date.range(" <> inspect(first) <> ", " <> inspect(last) <> ")"
    end

    def inspect(%Date.Range{first: first, last: last, step: step}, _) do
      "Date.range(" <> inspect(first) <> ", " <> inspect(last) <> ", #{step})"
    end

    # TODO: Remove me on v2.0
    def inspect(%{__struct__: Date.Range, first: first, last: last} = date_range, opts) do
      step = if first <= last, do: 1, else: -1
      inspect(Map.put(date_range, :step, step), opts)
    end
  end
end

# ---- calendar/duration.ex
defmodule Duration do

  @derive {Inspect, optional: [:year, :month, :week, :day, :hour, :minute, :second, :microsecond]}
  defstruct year: 0,
            month: 0,
            week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            microsecond: {0, 0}

  @microseconds_per_second 1_000_000

  def new!(%Duration{} = duration) do
    duration
  end

  def new!(unit_pairs) do
    Enum.each(unit_pairs, &validate_unit!/1)
    struct!(Duration, unit_pairs)
  end

  defp validate_unit!({:microsecond, {ms, precision}})
       when is_integer(ms) and precision in 0..6 do
    :ok
  end

  defp validate_unit!({:microsecond, microsecond}) do
    raise ArgumentError,
          "unsupported value #{inspect(microsecond)} for :microsecond. Expected a tuple {ms, precision} where precision is an integer from 0 to 6"
  end

  defp validate_unit!({unit, _value})
       when unit not in [:year, :month, :week, :day, :hour, :minute, :second] do
    raise ArgumentError,
          "unknown unit #{inspect(unit)}. Expected :year, :month, :week, :day, :hour, :minute, :second, :microsecond"
  end

  defp validate_unit!({_unit, value}) when is_integer(value) do
    :ok
  end

  defp validate_unit!({unit, value}) do
    raise ArgumentError,
          "unsupported value #{inspect(value)} for #{inspect(unit)}. Expected an integer"
  end

  def add(%Duration{} = d1, %Duration{} = d2) do
    {m1, p1} = d1.microsecond
    {m2, p2} = d2.microsecond

    %Duration{
      year: d1.year + d2.year,
      month: d1.month + d2.month,
      week: d1.week + d2.week,
      day: d1.day + d2.day,
      hour: d1.hour + d2.hour,
      minute: d1.minute + d2.minute,
      second: d1.second + d2.second,
      microsecond: {m1 + m2, max(p1, p2)}
    }
  end

  def subtract(%Duration{} = d1, %Duration{} = d2) do
    {m1, p1} = d1.microsecond
    {m2, p2} = d2.microsecond

    %Duration{
      year: d1.year - d2.year,
      month: d1.month - d2.month,
      week: d1.week - d2.week,
      day: d1.day - d2.day,
      hour: d1.hour - d2.hour,
      minute: d1.minute - d2.minute,
      second: d1.second - d2.second,
      microsecond: {m1 - m2, max(p1, p2)}
    }
  end

  def multiply(%Duration{microsecond: {ms, p}} = duration, integer) when is_integer(integer) do
    %Duration{
      year: duration.year * integer,
      month: duration.month * integer,
      week: duration.week * integer,
      day: duration.day * integer,
      hour: duration.hour * integer,
      minute: duration.minute * integer,
      second: duration.second * integer,
      microsecond: {ms * integer, p}
    }
  end

  def negate(%Duration{microsecond: {ms, p}} = duration) do
    %Duration{
      year: -duration.year,
      month: -duration.month,
      week: -duration.week,
      day: -duration.day,
      hour: -duration.hour,
      minute: -duration.minute,
      second: -duration.second,
      microsecond: {-ms, p}
    }
  end

  def from_iso8601(string) when is_binary(string) do
    case Calendar.ISO.parse_duration(string) do
      {:ok, duration} ->
        {:ok, new!(duration)}

      error ->
        error
    end
  end

  def from_iso8601!(string) when is_binary(string) do
    case from_iso8601(string) do
      {:ok, duration} ->
        duration

      {:error, reason} ->
        raise ArgumentError, ~s/failed to parse duration "#{string}". reason: #{inspect(reason)}/
    end
  end

  def to_string(%Duration{} = duration, opts \\ []) do
    units = Keyword.get(opts, :units, [])
    separator = Keyword.get(opts, :separator, " ")

    case to_string_year(duration, [], units) do
      [] ->
        "0" <> Keyword.get(units, :second, "s")

      [part] ->
        IO.iodata_to_binary(part)

      parts ->
        parts |> Enum.reduce(&[&1, separator | &2]) |> IO.iodata_to_binary()
    end
  end

  defp to_string_part(0, _units, _key, _default, acc),
    do: acc

  defp to_string_part(x, units, key, default, acc),
    do: [[Integer.to_string(x) | Keyword.get(units, key, default)] | acc]

  defp to_string_year(%{year: year} = duration, acc, units) do
    to_string_month(duration, to_string_part(year, units, :year, "a", acc), units)
  end

  defp to_string_month(%{month: month} = duration, acc, units) do
    to_string_week(duration, to_string_part(month, units, :month, "mo", acc), units)
  end

  defp to_string_week(%{week: week} = duration, acc, units) do
    to_string_day(duration, to_string_part(week, units, :week, "wk", acc), units)
  end

  defp to_string_day(%{day: day} = duration, acc, units) do
    to_string_hour(duration, to_string_part(day, units, :day, "d", acc), units)
  end

  defp to_string_hour(%{hour: hour} = duration, acc, units) do
    to_string_minute(duration, to_string_part(hour, units, :hour, "h", acc), units)
  end

  defp to_string_minute(%{minute: minute} = duration, acc, units) do
    to_string_second(duration, to_string_part(minute, units, :minute, "min", acc), units)
  end

  defp to_string_second(%{second: 0, microsecond: {0, _}}, acc, _units) do
    acc
  end

  defp to_string_second(%{second: s, microsecond: {ms, p}}, acc, units) do
    [[second_component(s, ms, p) | Keyword.get(units, :second, "s")] | acc]
  end

  def to_iso8601(%Duration{} = duration) do
    case {to_iso8601_duration_date(duration), to_iso8601_duration_time(duration)} do
      {[], []} -> "PT0S"
      {date, time} -> IO.iodata_to_binary([?P, date, time])
    end
  end

  defp to_iso8601_duration_date(%{year: 0, month: 0, week: 0, day: 0}) do
    []
  end

  defp to_iso8601_duration_date(%{year: year, month: month, week: week, day: day}) do
    [pair(year, ?Y), pair(month, ?M), pair(week, ?W), pair(day, ?D)]
  end

  defp to_iso8601_duration_time(%{hour: 0, minute: 0, second: 0, microsecond: {0, _}}) do
    []
  end

  defp to_iso8601_duration_time(%{hour: hour, minute: minute} = d) do
    [?T, pair(hour, ?H), pair(minute, ?M), second_component(d)]
  end

  defp second_component(%{second: 0, microsecond: {0, _}}) do
    []
  end

  defp second_component(%{second: second, microsecond: {ms, p}}) do
    [second_component(second, ms, p), ?S]
  end

  defp second_component(second, _ms, 0) do
    Integer.to_string(second)
  end

  defp second_component(second, ms, p) do
    total_ms = second * @microseconds_per_second + ms
    second = total_ms |> div(@microseconds_per_second) |> abs()
    ms = total_ms |> rem(@microseconds_per_second) |> abs()
    sign = if total_ms < 0, do: ?-, else: []

    [
      sign,
      Integer.to_string(second),
      ?.,
      ms |> Integer.to_string() |> String.pad_leading(6, "0") |> binary_part(0, p)
    ]
  end

  defp pair(0, _key), do: []
  defp pair(num, key), do: [Integer.to_string(num), key]
end

# ---- calendar/time_zone_database.ex
defmodule Calendar.TimeZoneDatabase do

end

defmodule Calendar.UTCOnlyTimeZoneDatabase do

  @behaviour Calendar.TimeZoneDatabase

  def time_zone_period_from_utc_iso_days(_, "Etc/UTC"),
    do: {:ok, %{std_offset: 0, utc_offset: 0, zone_abbr: "UTC"}}

  def time_zone_period_from_utc_iso_days(_, _),
    do: {:error, :utc_only_time_zone_database}

  def time_zone_periods_from_wall_datetime(_, "Etc/UTC"),
    do: {:ok, %{std_offset: 0, utc_offset: 0, zone_abbr: "UTC"}}

  def time_zone_periods_from_wall_datetime(_, _),
    do: {:error, :utc_only_time_zone_database}
end
