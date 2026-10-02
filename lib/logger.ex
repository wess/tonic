defmodule Logger do
  # Elixir's Logger API on top of the :logger shim below (Erlang's logger
  # pipeline: primary level/filters -> handlers with formatters).

  @levels [:emergency, :alert, :critical, :error, :warning, :notice, :info, :debug]

  def levels, do: @levels

  defp normalize(:warn), do: :warning
  defp normalize(level), do: level

  def level do
    %{level: level} = :logger.get_primary_config()
    level
  end

  def configure(options) do
    for {k, v} <- options do
      Application.put_env(:logger, k, v)

      case k do
        :level -> :logger.set_primary_config(:level, normalize(v))
        _ -> :ok
      end
    end

    :ok
  end

  def compare_levels(left, right), do: :logger.compare_levels(normalize(left), normalize(right))

  def enabled?(pid \\ self())
  def enabled?(pid) when pid == self(), do: Process.get(:"$logger_disabled") != true
  def enabled?(_pid), do: true
  def enable(_pid), do: (Process.delete(:"$logger_disabled"); :ok)
  def disable(_pid), do: (Process.put(:"$logger_disabled", true); :ok)

  def metadata do
    case :logger.get_process_metadata() do
      :undefined -> []
      map -> Map.to_list(map)
    end
  end

  def metadata(kw) do
    current = case :logger.get_process_metadata() do
      :undefined -> %{}
      map -> map
    end

    new =
      Enum.reduce(kw, current, fn
        {k, nil}, acc -> Map.delete(acc, k)
        {k, v}, acc -> Map.put(acc, k, v)
      end)

    :logger.set_process_metadata(new)
    :ok
  end

  def reset_metadata(kw \\ []) do
    :logger.set_process_metadata(%{})
    metadata(kw)
  end

  def flush, do: :ok
  def add_backend(_backend, _opts \\ []), do: {:ok, self()}
  def remove_backend(_backend, _opts \\ []), do: :ok
  def put_module_level(mod, level), do: :logger.set_module_level(mod, normalize(level))
  def delete_module_level(mod), do: :logger.unset_module_level(mod)
  def put_process_level(pid, level) when pid == self(), do: (Process.put(:"$logger_process_level", normalize(level)); :ok)
  def put_process_level(_pid, _level), do: :ok
  def get_process_level(pid) when pid == self(), do: Process.get(:"$logger_process_level")
  def get_process_level(_pid), do: nil
  def delete_process_level(pid) when pid == self(), do: (Process.delete(:"$logger_process_level"); :ok)
  def delete_process_level(_pid), do: :ok
  def put_application_level(_app, _level), do: :ok

  def default_formatter(overrides \\ []) when is_list(overrides) do
    Application.get_env(:logger, :default_formatter, [])
    |> Keyword.merge(overrides)
    |> Logger.Formatter.new()
  end

  for level <- [:emergency, :alert, :critical, :error, :warning, :notice, :info, :debug] do
    def unquote(level)(message_or_fun, metadata \\ []), do: bare_log(unquote(level), message_or_fun, metadata)
  end

  def warn(message_or_fun, metadata \\ []), do: bare_log(:warning, message_or_fun, metadata)

  def log(level, message_or_fun, metadata \\ []), do: bare_log(level, message_or_fun, metadata)

  def bare_log(level, message_or_fun, metadata \\ []) do
    level = normalize(level)

    if enabled?() and :logger.allow(level, nil) do
      __do_log__(level, message_or_fun, %{}, Map.new(metadata))
    end

    :ok
  end

  @doc false
  def __do_log__(level, fun, location, metadata) when is_function(fun, 0) do
    case fun.() do
      {msg, meta} -> __do_log__(level, msg, location, Enum.into(meta, metadata))
      :skip -> :ok
      msg -> __do_log__(level, msg, location, metadata)
    end
  end

  def __do_log__(level, msg, location, metadata) do
    meta = add_elixir_domain(metadata)

    if is_binary(msg) or is_list(msg) or is_map(msg) do
      :logger.macro_log(location, level, msg, meta)
    else
      :logger.macro_log(location, level, to_string(msg), meta)
    end
  end

  defp add_elixir_domain(%{domain: domain} = metadata) when is_list(domain), do: %{metadata | domain: [:elixir | domain]}
  defp add_elixir_domain(metadata), do: Map.put(metadata, :domain, [:elixir])
end

defmodule :logger do
  # Erlang's logger pipeline: primary config (level), primary filters (Elixir's
  # translator), handlers (`module.log(event, config)`); the :default handler
  # formats with Logger.Formatter and writes to the standard output.

  @levels [:emergency, :alert, :critical, :error, :warning, :notice, :info, :debug]
  @table :"$tonic_logger"

  defp level_int(:all), do: 8
  defp level_int(:none), do: -1
  defp level_int(:emergency), do: 0
  defp level_int(:alert), do: 1
  defp level_int(:critical), do: 2
  defp level_int(:error), do: 3
  defp level_int(:warning), do: 4
  defp level_int(:warn), do: 4
  defp level_int(:notice), do: 5
  defp level_int(:info), do: 6
  defp level_int(:debug), do: 7

  def compare_levels(a, b) do
    x = level_int(a)
    y = level_int(b)

    cond do
      x == y -> :eq
      x < y -> :gt
      true -> :lt
    end
  end

  # ---- configuration (public ETS table, created on first use)

  defp table do
    if :ets.whereis(@table) == :undefined do
      parent = self()

      spawn(fn ->
        try do
          :ets.new(@table, [:set, :public, :named_table])
          level = Application.get_env(:logger, :level, :debug)
          :ets.insert(@table, {:primary, %{level: level, filter_default: :log, filters: [], metadata: %{}}})
          :ets.insert(@table, {:translator, true})
          :ets.insert(@table, {{:handler, :default}, default_handler()})
        rescue
          _ -> :ok
        end

        send(parent, :tonic_logger_ready)

        receive do
          :tonic_never -> :ok
        end
      end)

      receive do
        :tonic_logger_ready -> :ok
      end
    end

    @table
  end

  defp default_handler do
    %{id: :default, module: :logger_std_h, level: :all, filter_default: :log, filters: [],
      formatter: Logger.default_formatter(), config: %{type: :standard_io}}
  end

  defp lookup(key, default) do
    case :ets.lookup(table(), key) do
      [{_, v}] -> v
      [] -> default
    end
  end

  def get_primary_config, do: lookup(:primary, %{level: :debug, filter_default: :log, filters: [], metadata: %{}})

  def set_primary_config(key, value) do
    :ets.insert(table(), {:primary, Map.put(get_primary_config(), key, value)})
    :ok
  end

  def set_primary_config(map) when is_map(map), do: (:ets.insert(table(), {:primary, Map.merge(get_primary_config(), map)}); :ok)
  def update_primary_config(map) when is_map(map), do: set_primary_config(map)

  def add_primary_filter(:logger_translator, _filter), do: (:ets.insert(table(), {:translator, true}); :ok)
  def add_primary_filter(_id, _filter), do: :ok
  def remove_primary_filter(:logger_translator), do: (:ets.insert(table(), {:translator, false}); :ok)
  def remove_primary_filter(_id), do: :ok
  def add_handler_filter(_h, _id, _filter), do: :ok
  def remove_handler_filter(_h, _id), do: :ok

  def get_handler_ids do
    for [id] <- :ets.match(table(), {{:handler, :"$1"}, :_}), do: id
  end

  def get_handler_config(id) do
    case lookup({:handler, id}, nil) do
      nil -> {:error, {:not_found, id}}
      config -> {:ok, config}
    end
  end

  def get_handler_config, do: for(id <- get_handler_ids(), {:ok, c} = get_handler_config(id), do: c)

  def add_handler(id, module, config) do
    case lookup({:handler, id}, nil) do
      nil ->
        config =
          Map.merge(%{id: id, module: module, level: :all, filter_default: :log, filters: [], formatter: {:logger_formatter, %{}}}, config)

        config =
          if function_exported?(module, :adding_handler, 1) do
            case module.adding_handler(config) do
              {:ok, c} -> c
              _ -> config
            end
          else
            config
          end

        :ets.insert(table(), {{:handler, id}, config})
        :ok

      _ ->
        {:error, {:already_exist, id}}
    end
  end

  def remove_handler(id) do
    case lookup({:handler, id}, nil) do
      nil ->
        {:error, {:not_found, id}}

      config ->
        :ets.delete(table(), {:handler, id})
        mod = config.module
        if function_exported?(mod, :removing_handler, 1), do: mod.removing_handler(config)
        :ok
    end
  end

  def set_handler_config(id, key, value) do
    case get_handler_config(id) do
      {:ok, c} -> (:ets.insert(table(), {{:handler, id}, Map.put(c, key, value)}); :ok)
      e -> e
    end
  end

  def update_handler_config(id, key, value) when is_atom(key), do: set_handler_config(id, key, value)

  def update_handler_config(id, map) when is_map(map) do
    case get_handler_config(id) do
      {:ok, c} -> (:ets.insert(table(), {{:handler, id}, Map.merge(c, map)}); :ok)
      e -> e
    end
  end

  def get_config, do: %{primary: get_primary_config(), handlers: get_handler_config(), module_levels: []}
  def set_module_level(_m, _l), do: :ok
  def unset_module_level(_m), do: :ok
  def set_application_level(_a, _l), do: :ok
  def get_process_metadata, do: Process.get(:"$logger_metadata$", :undefined)
  def set_process_metadata(md) when is_map(md), do: (Process.put(:"$logger_metadata$", md); :ok)
  def update_process_metadata(md) when is_map(md), do: set_process_metadata(Map.merge(Process.get(:"$logger_metadata$", %{}), md))
  def unset_process_metadata, do: (Process.delete(:"$logger_metadata$"); :ok)

  def allow(level, _module) do
    min = Logger.get_process_level(self()) || get_primary_config().level
    compare_levels(level, min) != :lt
  end

  # ---- logging

  def macro_log(location, level, string_or_report, meta) when is_map(meta) do
    msg =
      cond do
        is_map(string_or_report) -> {:report, string_or_report}
        is_list(string_or_report) and string_or_report != [] and Keyword.keyword?(string_or_report) -> {:report, string_or_report}
        true -> {:string, string_or_report}
      end

    log_event(level, msg, Map.merge(location, meta))
  end

  def macro_log(location, level, format, args) when is_list(args), do: log_event(level, {format, args}, location)
  def macro_log(location, level, string_or_report), do: macro_log(location, level, string_or_report, %{})
  def macro_log(location, level, format, args, meta), do: log_event(level, {format, args}, Map.merge(location, meta))

  def log(level, string_or_report), do: log(level, string_or_report, %{})

  def log(level, format, args) when is_list(args) and not is_map(args) do
    if allow(level, nil), do: log_event(level, {format, args}, %{}), else: :ok
  end

  def log(level, string_or_report, meta) when is_map(meta) do
    if allow(level, nil), do: macro_log(%{}, level, string_or_report, meta), else: :ok
  end

  def log(level, format, args, meta) do
    if allow(level, nil), do: log_event(level, {format, args}, meta), else: :ok
  end

  for level <- @levels do
    def unquote(level)(string_or_report), do: log(unquote(level), string_or_report)
    def unquote(level)(a, b), do: log(unquote(level), a, b)
    def unquote(level)(a, b, c), do: log(unquote(level), a, b, c)
  end

  defp log_event(level, msg, meta) do
    meta =
      %{pid: self(), gl: Process.group_leader(), time: :os.system_time(:microsecond)}
      |> Map.merge(Process.get(:"$logger_metadata$", %{}))
      |> Map.merge(meta)

    event = %{level: level, msg: msg, meta: meta}

    case primary_filters(event) do
      :stop -> :ok
      event -> for {{:handler, _}, config} <- :ets.tab2list(table()), do: call_handler(event, config)
    end

    :ok
  end

  # Logger.Utils.translator with Elixir's default configuration
  # (sasl reports dropped, translators: [{Logger.Translator, :translate}]).
  defp primary_filters(%{meta: meta} = event) do
    case Map.get(meta, :domain) do
      [:otp, :sasl | _] -> :stop
      [:supervisor_report | _] -> :stop
      _ ->
        if lookup(:translator, true) do
          case Logger.Utils.translator(event, %{otp: true, sasl: false, translators: [{Logger.Translator, :translate}]}) do
            :stop -> :stop
            :ignore -> event
            event -> event
          end
        else
          event
        end
    end
  end

  defp call_handler(%{level: level} = event, %{level: hlevel, module: mod} = config) do
    if compare_levels(level, hlevel) != :lt do
      try do
        case mod do
          :logger_std_h -> std_log(event, config)
          _ -> mod.log(event, config)
        end
      rescue
        _ -> :ok
      end
    end
  end

  defp std_log(event, %{formatter: {fmod, fconfig}}) do
    :tonic.io_write(:stdio, fmod.format(event, fconfig))
  end

  def format_otp_report(report), do: {~c"~p", [report]}
end

defmodule :logger_config do
  def allow(level), do: :logger.allow(level, nil)
  def allow(level, module), do: :logger.allow(level, module)
end
