defmodule Duration do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.




























































































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

# Imported from Elixir 1.18.3 lib/elixir/lib/calendar/duration.ex (docs and specs stripped;
# line numbers match the original).
