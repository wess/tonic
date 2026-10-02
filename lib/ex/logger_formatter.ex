import Kernel, except: [inspect: 2]
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
defmodule Logger.Formatter do






























































































  @valid_patterns [:time, :date, :message, :level, :node, :metadata, :levelpad]
  @default_pattern "\n$time $metadata[$level] $message\n"
  @replacement "�"

  ## Formatter API

  defstruct [:template, :truncate, :metadata, :colors, :utc_log?]















































  def new(options \\ []) do
    template = compile(options[:format])
    colors = colors(options[:colors] || [])
    truncate = options[:truncate] || Application.fetch_env!(:logger, :truncate)
    metadata = options[:metadata] || []
    utc_log? = Keyword.get(options, :utc_log, Application.fetch_env!(:logger, :utc_log))

    {__MODULE__,
     %__MODULE__{
       template: template,
       truncate: truncate,
       metadata: metadata,
       colors: colors,
       utc_log?: utc_log?
     }}
  end

  defp colors(colors) do
    warning =
      Keyword.get_lazy(colors, :warning, fn ->
        # TODO: Deprecate :warn option on Elixir v1.19
        if warn = Keyword.get(colors, :warn) do
          warn
        else
          :yellow
        end
      end)

    %{
      emergency: Keyword.get(colors, :error, :red),
      alert: Keyword.get(colors, :error, :red),
      critical: Keyword.get(colors, :error, :red),
      error: Keyword.get(colors, :error, :red),
      warning: warning,
      notice: Keyword.get(colors, :info, :normal),
      info: Keyword.get(colors, :info, :normal),
      debug: Keyword.get(colors, :debug, :cyan),
      enabled: Keyword.get(colors, :enabled, IO.ANSI.enabled?())
    }
  end


  def format(%{level: level, meta: meta} = event, %__MODULE__{} = config) do
    %{
      utc_log?: utc_log?,
      metadata: metadata_keys,
      template: template,
      colors: colors,
      truncate: truncate
    } = config

    system_time =
      case meta do
        %{time: time} when is_integer(time) and time >= 0 -> time
        _ -> :os.system_time(:microsecond)
      end

    date_time_ms = system_time_to_date_time_ms(system_time, utc_log?)

    meta_list =
      case metadata_keys do
        :all -> Map.to_list(meta)
        keys -> for key <- keys, value = compute_meta(key, meta), do: {key, value}
      end

    chardata = format_event(event, truncate)

    template
    |> format(level, chardata, date_time_ms, meta_list)
    |> colorize(level, colors, meta)
  end

  def format(_event, _config) do
    raise "invalid configuration for Logger.Formatter. " <>
            "Use Logger.Formatter.new/1 to define a formatter"
  end

  defp compute_meta(:module, %{mfa: {mod, _, _}}), do: mod
  defp compute_meta(:function, %{mfa: {_, fun, arity}}), do: format_fa(fun, arity)
  defp compute_meta(key, meta), do: meta[key]

  defp format_fa(fun, arity), do: [Atom.to_string(fun), "/", Integer.to_string(arity)]

  defp colorize(data, _level, %{enabled: false}, _md), do: data

  defp colorize(data, level, %{enabled: true} = colors, md) do
    color = md[:ansi_color] || Map.fetch!(colors, level)
    [IO.ANSI.format_fragment(color, true), data | IO.ANSI.reset()]
  end





  def format_event(%{msg: msg, meta: meta} = _log_event, truncate) do
    format_message(msg, meta, truncate)
  end

  defp format_message({:string, message}, _metadata, truncate) do
    wrapped_truncate(message, truncate)
  end

  defp format_message({:report, data}, %{report_cb: callback} = meta, truncate) do
    cond do
      is_function(callback, 1) and callback != (&:logger.format_otp_report/1) ->
        format_message(callback.(data), meta, truncate)

      is_function(callback, 2) ->
        callback.(data, %{depth: :unlimited, chars_limit: truncate, single_line: false})

      true ->
        format_report(data, truncate)
    end
  end

  defp format_message({:report, data}, _meta, truncate) do
    format_report(data, truncate)
  end

  defp format_message({format, args}, _meta, truncate) do
    format
    |> Logger.Utils.scan_inspect(args, truncate)
    |> :io_lib.build_text()
    |> wrapped_truncate(truncate)
  end

  defp format_report(%{} = data, truncate) do
    wrapped_truncate(Kernel.inspect(Map.to_list(data), translator_inspect_opts()), truncate)
  end

  defp format_report(data, truncate) do
    wrapped_truncate(Kernel.inspect(data, translator_inspect_opts()), truncate)
  end

  defp translator_inspect_opts() do
    Application.fetch_env!(:logger, :translator_inspect_opts)
  end

  defp wrapped_truncate(data, n) when is_binary(data), do: truncate(data, n)

  defp wrapped_truncate(data, n) when is_list(data) do
    truncate(data, n)
  rescue
    msg in ArgumentError -> Exception.message(msg)
  end









  def truncate(chardata, :infinity) when is_binary(chardata) or is_list(chardata) do
    chardata
  end

  def truncate(chardata, n) when n >= 0 do
    {chardata, n} = Logger.Utils.truncate_n(chardata, n)
    if n >= 0, do: chardata, else: [chardata, " (truncated)"]
  end







  def prune(binary) when is_binary(binary), do: prune_binary(binary, "")
  def prune([h | t]) when h in 0..1_114_111, do: [h | prune(t)]
  def prune([h | t]), do: [prune(h) | prune(t)]
  def prune([]), do: []
  def prune(_), do: @replacement

  defp prune_binary(<<h::utf8, t::binary>>, acc), do: prune_binary(t, <<acc::binary, h::utf8>>)
  defp prune_binary(<<_, t::binary>>, acc), do: prune_binary(t, <<acc::binary, @replacement>>)
  defp prune_binary(<<>>, acc), do: acc





  def format_time({hh, mi, ss, ms} = _time_ms_tuple) do
    [pad2(hh), ?:, pad2(mi), ?:, pad2(ss), ?., pad3(ms)]
  end





  def format_date({yy, mm, dd} = _date_tuple) do
    [Integer.to_string(yy), ?-, pad2(mm), ?-, pad2(dd)]
  end

  defp pad3(int) when int < 10, do: [?0, ?0, Integer.to_string(int)]
  defp pad3(int) when int < 100, do: [?0, Integer.to_string(int)]
  defp pad3(int), do: Integer.to_string(int)

  defp pad2(int) when int < 10, do: [?0, Integer.to_string(int)]
  defp pad2(int), do: Integer.to_string(int)





  def system_time_to_date_time_ms(system_time, utc_log? \\ false) do
    micro = rem(system_time, 1_000_000)

    {date, {hours, minutes, seconds}} =
      case utc_log? do
        true -> :calendar.system_time_to_universal_time(system_time, :microsecond)
        false -> :calendar.system_time_to_local_time(system_time, :microsecond)
      end

    {date, {hours, minutes, seconds, div(micro, 1000)}}
  end




























  def compile(pattern_or_function)

  def compile(nil), do: compile(@default_pattern)
  def compile({mod, fun}) when is_atom(mod) and is_atom(fun), do: {mod, fun}

  def compile(str) when is_binary(str) do
    regex = ~r/(?<head>)\$[a-z]+(?<tail>)/

    for part <- Regex.split(regex, str, on: [:head, :tail], trim: true) do
      case part do
        "$" <> code -> compile_code(String.to_atom(code))
        _ -> part
      end
    end
  end

  defp compile_code(:levelpad) do
    IO.warn("$levelpad in Logger message format is deprecated, please remove it")
    :levelpad
  end

  defp compile_code(key) when key in @valid_patterns, do: key

  defp compile_code(key) when is_atom(key) do
    raise ArgumentError, "$#{key} is an invalid format pattern"
  end

































  def format(pattern_or_function, level, message, timestamp, metadata)

  def format({mod, fun}, level, msg, timestamp, metadata) do
    apply(mod, fun, [level, msg, timestamp, metadata])
  end

  def format(config, level, msg, timestamp, metadata) do
    for config_option <- config do
      output(config_option, level, msg, timestamp, metadata)
    end
  end

  defp output(:message, _, msg, _, _), do: msg
  defp output(:date, _, _, {date, _time}, _), do: format_date(date)
  defp output(:time, _, _, {_date, time}, _), do: format_time(time)
  defp output(:level, level, _, _, _), do: Atom.to_string(level)
  defp output(:node, _, _, _, _), do: Atom.to_string(node())
  defp output(:metadata, _, _, _, []), do: ""
  defp output(:metadata, _, _, _, meta), do: metadata(meta)
  defp output(:levelpad, level, _, _, _), do: levelpad(level)
  defp output(other, _, _, _, _), do: other

  defp levelpad(:info), do: " "
  defp levelpad(:warn), do: " "
  defp levelpad(_), do: ""

  defp metadata([{key, value} | metadata]) do
    if formatted = metadata(key, value) do
      [to_string(key), ?=, formatted, ?\s | metadata(metadata)]
    else
      metadata(metadata)
    end
  end

  defp metadata([]) do
    []
  end

  defp metadata(:time, _), do: nil
  defp metadata(:gl, _), do: nil
  defp metadata(:report_cb, _), do: nil

  defp metadata(_, nil), do: nil
  defp metadata(_, string) when is_binary(string), do: string
  defp metadata(_, integer) when is_integer(integer), do: Integer.to_string(integer)
  defp metadata(_, float) when is_float(float), do: Float.to_string(float)
  defp metadata(_, pid) when is_pid(pid), do: :erlang.pid_to_list(pid)

  defp metadata(_, atom) when is_atom(atom) do
    case Atom.to_string(atom) do
      "Elixir." <> rest -> rest
      binary -> binary
    end
  end

  defp metadata(_, ref) when is_reference(ref) do
    ~c"#Ref" ++ rest = :erlang.ref_to_list(ref)
    rest
  end

  defp metadata(_, port) when is_port(port) do
    ~c"#Port" ++ rest = :erlang.port_to_list(port)
    rest
  end

  defp metadata(:domain, [head | tail]) when is_atom(head) do
    Enum.map_intersperse([head | tail], ?., &Atom.to_string/1)
  end

  defp metadata(:mfa, {mod, fun, arity})
       when is_atom(mod) and is_atom(fun) and is_integer(arity) do
    Exception.format_mfa(mod, fun, arity)
  end

  defp metadata(:initial_call, {mod, fun, arity})
       when is_atom(mod) and is_atom(fun) and is_integer(arity) do
    Exception.format_mfa(mod, fun, arity)
  end

  defp metadata(:function, function) when is_list(function), do: function
  defp metadata(:file, file) when is_list(file), do: file
  defp metadata(_, list) when is_list(list), do: nil

  defp metadata(_, other) do
    case String.Chars.impl_for(other) do
      nil -> nil
      impl -> impl.to_string(other)
    end
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/logger/formatter.ex (docs and specs stripped;
# line numbers match the original).
