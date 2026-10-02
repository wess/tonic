defmodule :application do
  # OTP application controller: application specs, environment, loading,
  # starting (an application master per started application) and stopping.
  # State lives in public ETS tables owned by the registered
  # :application_controller process.

  @env :"$tonic_app_env"
  @apps :"$tonic_apps"

  @tonic_vsn ~c"1.18.3"
  @otp_desc ~c"ERTS  CXC 138 10"

  defp builtin(:kernel),
    do: [description: @otp_desc, vsn: ~c"10.2", registered: [], applications: [], env: []]

  defp builtin(:stdlib),
    do: [description: @otp_desc, vsn: ~c"6.2", registered: [], applications: [:kernel], env: []]

  defp builtin(:compiler),
    do: [description: @otp_desc, vsn: ~c"8.5.4", registered: [], applications: [:kernel, :stdlib], env: []]

  defp builtin(:elixir),
    do: [
      description: ~c"elixir",
      vsn: @tonic_vsn,
      registered: [:elixir_sup, :elixir_config, :elixir_code_server],
      applications: [:kernel, :stdlib, :compiler],
      mod: {:elixir, []},
      env: [
        ansi_syntax_colors: [],
        check_endianness: true,
        dbg_callback: {Macro, :dbg, []},
        time_zone_database: Calendar.UTCOnlyTimeZoneDatabase
      ]
    ]

  defp builtin(:logger),
    do: [
      description: ~c"logger",
      vsn: @tonic_vsn,
      registered: [Logger.Supervisor],
      applications: [:kernel, :stdlib, :elixir],
      mod: {Logger.App, []},
      env: [
        utc_log: false,
        truncate: 8096,
        translators: [{Logger.Translator, :translate}],
        translator_inspect_opts: [],
        start_options: []
      ]
    ]

  defp builtin(:ex_unit),
    do: [
      description: ~c"ex_unit",
      vsn: @tonic_vsn,
      registered: [ExUnit.CaptureServer, ExUnit.OnExitHandler, ExUnit.Server, ExUnit.Supervisor],
      applications: [:kernel, :stdlib, :elixir],
      mod: {ExUnit, []},
      env: [
        assert_receive_timeout: 100,
        autorun: true,
        capture_log: false,
        colors: [],
        exclude: [],
        exit_status: 2,
        formatters: [ExUnit.CLIFormatter],
        include: [],
        max_failures: :infinity,
        rand_algorithm: :exsss,
        refute_receive_timeout: 100,
        slowest: 0,
        slowest_modules: 0,
        stacktrace_depth: 20,
        timeout: 60000,
        trace: false,
        after_suite: [],
        repeat_until_failure: 0
      ]
    ]

  defp builtin(_), do: nil

  # Applications running when a script starts (as `elixir` boots them).
  @boot [:kernel, :stdlib, :compiler, :elixir, :logger]

  ## Controller process / tables

  defp ctl do
    case :erlang.whereis(:application_controller) do
      :undefined ->
        parent = self()

        spawn(fn ->
          try do
            :erlang.register(:application_controller, self())
          rescue
            _ ->
              Kernel.send(parent, :tonic_app_ready)
              exit(:normal)
          end

          :ets.new(@env, [:set, :public, :named_table])
          :ets.new(@apps, [:set, :public, :named_table])
          :ets.insert(@env, {{:elixir, :ansi_enabled}, :tonic.isatty(1)})

          for app <- @boot do
            spec = builtin(app)
            :ets.insert(@apps, {{:spec, app}, spec})
            for {k, v} <- Keyword.get(spec, :env, []), do: :ets.insert_new(@env, {{app, k}, v})
          end

          :ets.insert(@apps, {:started, Enum.reverse(Enum.map(@boot, &{&1, :permanent, nil}))})
          Kernel.send(parent, :tonic_app_ready)
          controller_loop()
        end)

        receive do
          :tonic_app_ready -> :ok
        end

        wait_tables()

      _ ->
        :ok
    end
  end

  defp wait_tables do
    if :ets.whereis(@apps) == :undefined do
      receive do
      after
        1 -> wait_tables()
      end
    else
      :ok
    end
  end

  defp controller_loop do
    receive do
      _ -> controller_loop()
    end
  end

  defp started_list do
    ctl()

    case :ets.lookup(@apps, :started) do
      [{_, l}] -> l
      [] -> []
    end
  end

  defp put_started(l), do: :ets.insert(@apps, {:started, l})

  defp spec_of(app) do
    ctl()

    case :ets.lookup(@apps, {:spec, app}) do
      [{_, spec}] -> spec
      [] -> nil
    end
  end

  ## Environment

  def get_env(_key), do: :undefined

  def get_env(app, key) do
    ctl()

    case :ets.lookup(@env, {app, key}) do
      [{_, v}] -> {:ok, v}
      [] -> :undefined
    end
  end

  def get_env(app, key, default) do
    case get_env(app, key) do
      {:ok, v} -> v
      :undefined -> default
    end
  end

  def get_all_env(app) do
    ctl()
    for {{^app, k}, v} <- :ets.tab2list(@env), do: {k, v}
  end

  def get_all_env, do: []

  def set_env(app, key, val), do: set_env(app, key, val, [])

  def set_env(app, key, val, _opts) do
    ctl()
    :ets.insert(@env, {{app, key}, val})
    :ok
  end

  def set_env(config, opts) when is_list(config) do
    for {app, kvs} <- config do
      for {k, v} <- kvs, do: set_env(app, k, v, opts)
    end

    :ok
  end

  def set_env(config), do: set_env(config, [])

  def unset_env(app, key), do: unset_env(app, key, [])

  def unset_env(app, key, _opts) do
    ctl()
    :ets.delete(@env, {app, key})
    :ok
  end

  ## Keys / specs

  def get_application, do: :undefined
  def get_application(pid) when is_pid(pid), do: :undefined
  def get_application(module) when is_atom(module), do: Tonic.Internal.module_app(module)
  def get_application(_), do: :undefined

  def get_key(_key), do: :undefined

  def get_key(app, key) do
    case spec_of(app) do
      nil ->
        :undefined

      spec ->
        case full_spec(app, spec) |> List.keyfind(key, 0) do
          {_, v} -> {:ok, v}
          nil -> :undefined
        end
    end
  end

  def get_all_key, do: :undefined

  def get_all_key(app) do
    case spec_of(app) do
      nil -> :undefined
      spec -> {:ok, full_spec(app, spec)}
    end
  end

  defp full_spec(app, spec) do
    [
      description: Keyword.get(spec, :description, ~c""),
      id: Keyword.get(spec, :id, []),
      vsn: Keyword.get(spec, :vsn, ~c""),
      modules: Keyword.get(spec, :modules, []),
      maxP: Keyword.get(spec, :maxP, :infinity),
      maxT: Keyword.get(spec, :maxT, :infinity),
      registered: Keyword.get(spec, :registered, []),
      included_applications: Keyword.get(spec, :included_applications, []),
      optional_applications: Keyword.get(spec, :optional_applications, []),
      applications: Keyword.get(spec, :applications, []),
      env: get_all_env(app),
      mod: Keyword.get(spec, :mod, []),
      start_phases: Keyword.get(spec, :start_phases, :undefined)
    ]
  end

  def loaded_applications do
    ctl()

    for {{:spec, app}, spec} <- :ets.tab2list(@apps) do
      {app, Keyword.get(spec, :description, ~c""), Keyword.get(spec, :vsn, ~c"")}
    end
    |> order_like_started()
  end

  defp order_like_started(list) do
    started = for {a, _, _} <- started_list(), do: a
    {s, rest} = Enum.split_with(list, fn {a, _, _} -> a in started end)
    rest ++ Enum.sort_by(s, fn {a, _, _} -> Enum.find_index(started, &(&1 == a)) end)
  end

  def which_applications, do: which_applications(5000)

  def which_applications(_timeout) do
    for {app, _, _} <- started_list() do
      spec = spec_of(app) || []
      {app, Keyword.get(spec, :description, ~c""), Keyword.get(spec, :vsn, ~c"")}
    end
  end

  def info do
    [
      loaded: loaded_applications(),
      loading: [],
      started: for({a, t, _} <- started_list(), do: {a, t}),
      start_p_false: [],
      running: for({a, _, m} <- started_list(), do: {a, m || :undefined}),
      starting: []
    ]
  end

  ## Loading

  def load(app), do: load(app, [])

  def load({:application, app, keys}, _distnodes) when is_atom(app) and is_list(keys) do
    do_load(app, keys)
  end

  def load(app, _distnodes) when is_atom(app) do
    case spec_of(app) || builtin(app) do
      nil -> {:error, {~c"no such file or directory", Atom.to_charlist(app) ++ ~c".app"}}
      keys -> do_load(app, keys)
    end
  end

  def load(app, _), do: {:error, {:invalid_name, app}}

  defp do_load(app, keys) do
    if spec_of(app) != nil do
      {:error, {:already_loaded, app}}
    else
      :ets.insert(@apps, {{:spec, app}, keys})
      for {k, v} <- Keyword.get(keys, :env, []), do: :ets.insert_new(@env, {{app, k}, v})
      :ok
    end
  end

  def unload(app) do
    cond do
      List.keymember?(started_list(), app, 0) ->
        {:error, {:running, app}}

      spec_of(app) == nil ->
        {:error, {:not_loaded, app}}

      true ->
        :ets.delete(@apps, {:spec, app})
        for {{^app, k}, _} <- :ets.tab2list(@env), do: :ets.delete(@env, {app, k})
        :ok
    end
  end

  ## Starting

  def start(app), do: start(app, :temporary)

  def start(app, type) when type in [:temporary, :transient, :permanent] do
    loaded =
      case load(app) do
        :ok -> :ok
        {:error, {:already_loaded, _}} -> :ok
        err -> err
      end

    with :ok <- loaded do
      if List.keymember?(started_list(), app, 0) do
        {:error, {:already_started, app}}
      else
        spec = spec_of(app)
        deps = Keyword.get(spec, :applications, [])
        optional = Keyword.get(spec, :optional_applications, [])
        started = for {a, _, _} <- started_list(), do: a

        case Enum.find(deps, &(&1 not in started and &1 not in optional)) do
          nil -> start_master(app, type, spec)
          dep -> {:error, {:not_started, dep}}
        end
      end
    end
  end

  def start(_app, type), do: {:error, {:invalid_restart_type, type}}

  def ensure_started(app), do: ensure_started(app, :temporary)

  def ensure_started(app, type) do
    case start(app, type) do
      :ok -> :ok
      {:error, {:already_started, ^app}} -> :ok
      err -> err
    end
  end

  def ensure_all_started(apps), do: ensure_all_started(apps, :temporary, :serial)
  def ensure_all_started(apps, type), do: ensure_all_started(apps, type, :serial)

  def ensure_all_started(app, type, mode) when is_atom(app), do: ensure_all_started([app], type, mode)

  def ensure_all_started(apps, type, _mode) when is_list(apps) do
    result =
      Enum.reduce_while(apps, {:ok, []}, fn app, {:ok, acc} ->
        case ensure_one(app, type, acc) do
          {:ok, acc} -> {:cont, {:ok, acc}}
          {:error, _} = e -> {:halt, {e, acc}}
        end
      end)

    case result do
      {:ok, acc} ->
        {:ok, Enum.reverse(acc)}

      {{:error, reason}, acc} ->
        Enum.each(acc, &stop/1)
        {:error, reason}
    end
  end

  defp ensure_one(app, type, acc) do
    loaded =
      case load(app) do
        :ok -> :ok
        {:error, {:already_loaded, _}} -> :ok
        {:error, reason} -> {:error, {app, reason}}
      end

    with :ok <- loaded do
      if List.keymember?(started_list(), app, 0) do
        {:ok, acc}
      else
        spec = spec_of(app)
        optional = Keyword.get(spec, :optional_applications, [])

        deps_result =
          Enum.reduce_while(Keyword.get(spec, :applications, []), {:ok, acc}, fn dep, {:ok, acc} ->
            if dep in optional and builtin(dep) == nil and spec_of(dep) == nil do
              {:cont, {:ok, acc}}
            else
              case ensure_one(dep, type, acc) do
                {:ok, acc} -> {:cont, {:ok, acc}}
                err -> {:halt, err}
              end
            end
          end)

        with {:ok, acc} <- deps_result do
          case start(app, type) do
            :ok -> {:ok, [app | acc]}
            {:error, {:already_started, _}} -> {:ok, acc}
            {:error, reason} -> {:error, {app, reason}}
          end
        end
      end
    end
  end

  defp start_master(app, type, spec) do
    caller = self()
    ref = make_ref()

    master =
      spawn(fn ->
        Process.flag(:trap_exit, true)

        case Keyword.get(spec, :mod, []) do
          {mod, args} ->
            mfa = {mod, :start, [:normal, args]}

            res =
              try do
                mod.start(:normal, args)
              catch
                :exit, r -> {:tonic_exit, r}
                :error, r -> {:tonic_exit, {r, __STACKTRACE__}}
                :throw, r -> {:tonic_exit, {{:nocatch, r}, __STACKTRACE__}}
              end

            case res do
              {:ok, pid} when is_pid(pid) ->
                Kernel.send(caller, {ref, :ok})
                master_loop(app, type, mod, pid, [])

              {:ok, pid, state} when is_pid(pid) ->
                Kernel.send(caller, {ref, :ok})
                master_loop(app, type, mod, pid, state)

              {:error, reason} ->
                Kernel.send(caller, {ref, {:error, {reason, mfa}}})

              {:tonic_exit, reason} ->
                Kernel.send(caller, {ref, {:error, {:bad_return, {mfa, {:EXIT, reason}}}}})

              other ->
                Kernel.send(caller, {ref, {:error, {:bad_return, {mfa, other}}}})
            end

          _ ->
            Kernel.send(caller, {ref, :ok})
            master_loop(app, type, nil, nil, [])
        end
      end)

    mref = Process.monitor(master)

    receive do
      {^ref, :ok} ->
        Process.demonitor(mref, [:flush])
        put_started([{app, type, master} | started_list()])
        :ok

      {^ref, {:error, reason} = err} ->
        Process.demonitor(mref, [:flush])
        report_exit(app, reason, type)
        err

      {:DOWN, ^mref, _, _, reason} ->
        report_exit(app, reason, type)
        {:error, reason}
    end
  end

  defp master_loop(app, type, mod, pid, state) do
    receive do
      {:EXIT, ^pid, reason} ->
        put_started(List.keydelete(started_list(), app, 0))

        if function_exported?(mod, :stop, 1) do
          try do
            mod.stop(state)
          catch
            _, _ -> :ok
          end
        end

        report_exit(app, reason, type)

        if type == :permanent or (type == :transient and reason != :normal) do
          System.stop(1)
        end

      {:tonic_app_stop, from, ref} ->
        state =
          if mod != nil and function_exported?(mod, :prep_stop, 1) do
            mod.prep_stop(state)
          else
            state
          end

        if pid != nil do
          Process.exit(pid, :shutdown)

          receive do
            {:EXIT, ^pid, _} -> :ok
          end
        end

        if mod != nil and function_exported?(mod, :stop, 1), do: mod.stop(state)
        put_started(List.keydelete(started_list(), app, 0))
        report_exit(app, :stopped, type)
        Kernel.send(from, {ref, :ok})

      _ ->
        master_loop(app, type, mod, pid, state)
    end
  end

  defp report_exit(app, reason, type) do
    :logger.notice(
      %{
        label: {:application_controller, :exit},
        report: [application: app, exited: reason, type: type]
      },
      %{domain: [:otp], error_logger: %{tag: :info_report, type: :std_info}}
    )
  end

  def stop(app) do
    case List.keyfind(started_list(), app, 0) do
      nil ->
        {:error, {:not_started, app}}

      {_, _, nil} ->
        put_started(List.keydelete(started_list(), app, 0))
        :ok

      {_, _, master} ->
        ref = make_ref()
        mref = Process.monitor(master)
        Kernel.send(master, {:tonic_app_stop, self(), ref})

        receive do
          {^ref, :ok} ->
            Process.demonitor(mref, [:flush])
            :ok

          {:DOWN, ^mref, _, _, _} ->
            :ok
        end
    end
  end
end

defmodule Logger.App do
  def start(_type, _args), do: {:ok, spawn_link(fn -> receive do: (:tonic_never -> :ok) end)}
  def stop(_), do: :ok
end
