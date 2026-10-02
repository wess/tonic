defmodule DynamicSupervisor do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.




































































































































































































  @behaviour GenServer




































  # In this struct, `args` refers to the arguments passed to init/1 (the `init_arg`).
  defstruct [
    :args,
    :extra_arguments,
    :mod,
    :name,
    :strategy,
    :max_children,
    :max_restarts,
    :max_seconds,
    children: %{},
    restarts: []
  ]









  def child_spec(options) when is_list(options) do
    id =
      case Keyword.get(options, :name, DynamicSupervisor) do
        name when is_atom(name) -> name
        {:global, name} -> name
        {:via, _module, name} -> name
      end

    %{
      id: id,
      start: {DynamicSupervisor, :start_link, [options]},
      type: :supervisor
    }
  end


  defmacro __using__(opts) do
    quote location: :keep, bind_quoted: [opts: opts] do
      @behaviour DynamicSupervisor
      if not Module.has_attribute?(__MODULE__, :doc) do





      end

      def child_spec(arg) do
        default = %{
          id: __MODULE__,
          start: {__MODULE__, :start_link, [arg]},
          type: :supervisor
        }

        Supervisor.child_spec(default, unquote(Macro.escape(opts)))
      end

      defoverridable child_spec: 1
    end
  end




















































  def start_link(options) when is_list(options) do
    keys = [:extra_arguments, :max_children, :max_seconds, :max_restarts, :strategy]
    {sup_opts, start_opts} = Keyword.split(options, keys)
    start_link(Supervisor.Default, init(sup_opts), start_opts)
  end





































  def start_link(module, init_arg, opts \\ []) do
    GenServer.start_link(__MODULE__, {module, init_arg, opts[:name]}, opts)
  end



































  def start_child(supervisor, {_, _, _, _, _, _} = child_spec) do
    validate_and_start_child(supervisor, child_spec)
  end

  def start_child(supervisor, child_spec) do
    validate_and_start_child(supervisor, Supervisor.child_spec(child_spec, []))
  end

  defp validate_and_start_child(supervisor, child_spec) do
    case validate_child(child_spec) do
      {:ok, child} -> call(supervisor, {:start_child, child})
      error -> {:error, error}
    end
  end

  defp validate_child(%{id: _, start: {mod, _, _} = start} = child) do
    restart = Map.get(child, :restart, :permanent)
    type = Map.get(child, :type, :worker)
    modules = Map.get(child, :modules, [mod])
    significant = Map.get(child, :significant, false)

    shutdown =
      case type do
        :worker -> Map.get(child, :shutdown, 5_000)
        :supervisor -> Map.get(child, :shutdown, :infinity)
      end

    validate_child(start, restart, shutdown, type, modules, significant)
  end

  defp validate_child({_, start, restart, shutdown, type, modules}) do
    validate_child(start, restart, shutdown, type, modules, false)
  end

  defp validate_child(other) do
    {:invalid_child_spec, other}
  end

  defp validate_child(start, restart, shutdown, type, modules, significant) do
    with :ok <- validate_start(start),
         :ok <- validate_restart(restart),
         :ok <- validate_shutdown(shutdown),
         :ok <- validate_type(type),
         :ok <- validate_modules(modules),
         :ok <- validate_significant(significant) do
      {:ok, {start, restart, shutdown, type, modules}}
    end
  end

  defp validate_start({m, f, args}) when is_atom(m) and is_atom(f) and is_list(args), do: :ok
  defp validate_start(mfa), do: {:invalid_mfa, mfa}

  defp validate_type(type) when type in [:supervisor, :worker], do: :ok
  defp validate_type(type), do: {:invalid_child_type, type}

  defp validate_restart(restart) when restart in [:permanent, :temporary, :transient], do: :ok
  defp validate_restart(restart), do: {:invalid_restart_type, restart}

  defp validate_shutdown(shutdown) when is_integer(shutdown) and shutdown >= 0, do: :ok
  defp validate_shutdown(shutdown) when shutdown in [:infinity, :brutal_kill], do: :ok
  defp validate_shutdown(shutdown), do: {:invalid_shutdown, shutdown}

  defp validate_significant(false), do: :ok
  defp validate_significant(significant), do: {:invalid_significant, significant}

  defp validate_modules(:dynamic), do: :ok

  defp validate_modules(mods) do
    if is_list(mods) and Enum.all?(mods, &is_atom/1) do
      :ok
    else
      {:invalid_modules, mods}
    end
  end









  def terminate_child(supervisor, pid) when is_pid(pid) do
    call(supervisor, {:terminate_child, pid})
  end


























  def which_children(supervisor) do
    call(supervisor, :which_children)
  end

























  def count_children(supervisor) do
    call(supervisor, :count_children) |> :maps.from_list()
  end













  def stop(supervisor, reason \\ :normal, timeout \\ :infinity) do
    GenServer.stop(supervisor, reason, timeout)
  end




















  def init(options) when is_list(options) do
    strategy = Keyword.get(options, :strategy, :one_for_one)
    intensity = Keyword.get(options, :max_restarts, 3)
    period = Keyword.get(options, :max_seconds, 5)
    max_children = Keyword.get(options, :max_children, :infinity)
    extra_arguments = Keyword.get(options, :extra_arguments, [])

    flags = %{
      strategy: strategy,
      intensity: intensity,
      period: period,
      max_children: max_children,
      extra_arguments: extra_arguments
    }

    {:ok, flags}
  end

  ## Callbacks


  def init({mod, init_arg, name}) do
    Process.put(:"$initial_call", {:supervisor, mod, 1})
    Process.flag(:trap_exit, true)

    case mod.init(init_arg) do
      {:ok, flags} when is_map(flags) ->
        name =
          cond do
            is_nil(name) -> {self(), mod}
            is_atom(name) -> {:local, name}
            is_tuple(name) -> name
          end

        state = %DynamicSupervisor{mod: mod, args: init_arg, name: name}

        case init(state, flags) do
          {:ok, state} -> {:ok, state}
          {:error, reason} -> {:stop, {:supervisor_data, reason}}
        end

      :ignore ->
        :ignore

      other ->
        {:stop, {:bad_return, {mod, :init, other}}}
    end
  end

  defp init(state, flags) do
    extra_arguments = Map.get(flags, :extra_arguments, [])
    max_children = Map.get(flags, :max_children, :infinity)
    max_restarts = Map.get(flags, :intensity, 1)
    max_seconds = Map.get(flags, :period, 5)
    strategy = Map.get(flags, :strategy, :one_for_one)
    auto_shutdown = Map.get(flags, :auto_shutdown, :never)

    with :ok <- validate_strategy(strategy),
         :ok <- validate_restarts(max_restarts),
         :ok <- validate_seconds(max_seconds),
         :ok <- validate_dynamic(max_children),
         :ok <- validate_extra_arguments(extra_arguments),
         :ok <- validate_auto_shutdown(auto_shutdown) do
      {:ok,
       %{
         state
         | extra_arguments: extra_arguments,
           max_children: max_children,
           max_restarts: max_restarts,
           max_seconds: max_seconds,
           strategy: strategy
       }}
    end
  end

  defp validate_strategy(strategy) when strategy in [:one_for_one], do: :ok
  defp validate_strategy(strategy), do: {:error, {:invalid_strategy, strategy}}

  defp validate_restarts(restart) when is_integer(restart) and restart >= 0, do: :ok
  defp validate_restarts(restart), do: {:error, {:invalid_intensity, restart}}

  defp validate_seconds(seconds) when is_integer(seconds) and seconds > 0, do: :ok
  defp validate_seconds(seconds), do: {:error, {:invalid_period, seconds}}

  defp validate_dynamic(:infinity), do: :ok
  defp validate_dynamic(dynamic) when is_integer(dynamic) and dynamic >= 0, do: :ok
  defp validate_dynamic(dynamic), do: {:error, {:invalid_max_children, dynamic}}

  defp validate_extra_arguments(list) when is_list(list), do: :ok
  defp validate_extra_arguments(extra), do: {:error, {:invalid_extra_arguments, extra}}

  defp validate_auto_shutdown(auto_shutdown) when auto_shutdown in [:never], do: :ok

  defp validate_auto_shutdown(auto_shutdown),
    do: {:error, {:invalid_auto_shutdown, auto_shutdown}}


  def handle_call(:which_children, _from, state) do
    %{children: children} = state

    reply =
      for {pid, args} <- children do
        case args do
          {:restarting, {_, _, _, type, modules}} ->
            {:undefined, :restarting, type, modules}

          {_, _, _, type, modules} ->
            {:undefined, pid, type, modules}
        end
      end

    {:reply, reply, state}
  end

  def handle_call(:count_children, _from, state) do
    %{children: children} = state
    specs = map_size(children)

    {active, workers, supervisors} =
      Enum.reduce(children, {0, 0, 0}, fn
        {_pid, {:restarting, {_, _, _, :worker, _}}}, {active, worker, supervisor} ->
          {active, worker + 1, supervisor}

        {_pid, {:restarting, {_, _, _, :supervisor, _}}}, {active, worker, supervisor} ->
          {active, worker, supervisor + 1}

        {_pid, {_, _, _, :worker, _}}, {active, worker, supervisor} ->
          {active + 1, worker + 1, supervisor}

        {_pid, {_, _, _, :supervisor, _}}, {active, worker, supervisor} ->
          {active + 1, worker, supervisor + 1}
      end)

    reply = [specs: specs, active: active, supervisors: supervisors, workers: workers]
    {:reply, reply, state}
  end

  def handle_call({:terminate_child, pid}, _from, %{children: children} = state) do
    case children do
      %{^pid => info} ->
        :ok = terminate_children(%{pid => info}, state)
        {:reply, :ok, delete_child(pid, state)}

      %{} ->
        {:reply, {:error, :not_found}, state}
    end
  end

  def handle_call({:start_task, args, restart, shutdown}, from, state) do
    {init_restart, init_shutdown} = Process.get(Task.Supervisor)

    if restart == nil and init_restart != :temporary do
      {:reply, {:restart, init_restart}, state}
    else
      restart = restart || init_restart
      shutdown = shutdown || init_shutdown

      child =
        {{Task.Supervised, :start_link, args}, restart, shutdown, :worker, [Task.Supervised]}

      handle_call({:start_child, child}, from, state)
    end
  end

  def handle_call({:start_child, child}, _from, state) do
    %{children: children, max_children: max_children} = state

    if map_size(children) < max_children do
      handle_start_child(child, state)
    else
      {:reply, {:error, :max_children}, state}
    end
  end

  defp handle_start_child({{m, f, args} = mfa, restart, shutdown, type, modules}, state) do
    %{extra_arguments: extra} = state

    case reply = start_child(m, f, extra ++ args) do
      {:ok, pid, _} ->
        {:reply, reply, save_child(pid, mfa, restart, shutdown, type, modules, state)}

      {:ok, pid} ->
        {:reply, reply, save_child(pid, mfa, restart, shutdown, type, modules, state)}

      _ ->
        {:reply, reply, state}
    end
  end

  defp start_child(m, f, a) do
    try do
      apply(m, f, a)
    catch
      kind, reason ->
        {:error, exit_reason(kind, reason, __STACKTRACE__)}
    else
      {:ok, pid, extra} when is_pid(pid) -> {:ok, pid, extra}
      {:ok, pid} when is_pid(pid) -> {:ok, pid}
      :ignore -> :ignore
      {:error, _} = error -> error
      other -> {:error, other}
    end
  end

  defp save_child(pid, mfa, restart, shutdown, type, modules, state) do
    mfa = mfa_for_restart(mfa, restart)
    put_in(state.children[pid], {mfa, restart, shutdown, type, modules})
  end

  defp mfa_for_restart({m, f, _}, :temporary), do: {m, f, :undefined}
  defp mfa_for_restart(mfa, _), do: mfa

  defp exit_reason(:exit, reason, _), do: reason
  defp exit_reason(:error, reason, stack), do: {reason, stack}
  defp exit_reason(:throw, value, stack), do: {{:nocatch, value}, stack}


  def handle_cast(_msg, state) do
    {:noreply, state}
  end


  def handle_info({:EXIT, pid, reason}, state) do
    case maybe_restart_child(pid, reason, state) do
      {:ok, state} -> {:noreply, state}
      {:shutdown, state} -> {:stop, :shutdown, state}
    end
  end

  def handle_info({:"$gen_restart", pid}, state) do
    %{children: children} = state

    case children do
      %{^pid => restarting_args} ->
        {:restarting, child} = restarting_args

        case restart_child(pid, child, state) do
          {:ok, state} -> {:noreply, state}
          {:shutdown, state} -> {:stop, :shutdown, state}
        end

      # We may hit clause if we send $gen_restart and then
      # someone calls terminate_child, removing the child.
      %{} ->
        {:noreply, state}
    end
  end

  def handle_info(msg, state) do
    :logger.error(
      %{
        label: {DynamicSupervisor, :unexpected_msg},
        report: %{
          msg: msg
        }
      },
      %{
        domain: [:otp, :elixir],
        error_logger: %{tag: :error_msg},
        report_cb: &__MODULE__.format_report/1
      }
    )

    {:noreply, state}
  end


  def code_change(_, %{mod: mod, args: init_arg} = state, _) do
    case mod.init(init_arg) do
      {:ok, flags} when is_map(flags) ->
        case init(state, flags) do
          {:ok, state} -> {:ok, state}
          {:error, reason} -> {:error, {:supervisor_data, reason}}
        end

      :ignore ->
        {:ok, state}

      error ->
        error
    end
  end


  def terminate(_, %{children: children} = state) do
    :ok = terminate_children(children, state)
  end

  defp terminate_children(children, state) do
    {pids, times, stacks} = monitor_children(children)
    size = map_size(pids)

    timers =
      Enum.reduce(times, %{}, fn {time, pids}, acc ->
        Map.put(acc, :erlang.start_timer(time, self(), :kill), pids)
      end)

    stacks = wait_children(pids, size, timers, stacks)

    for {pid, {child, reason}} <- stacks do
      report_error(:shutdown_error, reason, pid, child, state)
    end

    :ok
  end

  defp monitor_children(children) do
    Enum.reduce(children, {%{}, %{}, %{}}, fn
      {_, {:restarting, _}}, acc ->
        acc

      {pid, {_, restart, _, _, _} = child}, {pids, times, stacks} ->
        case monitor_child(pid) do
          :ok ->
            times = exit_child(pid, child, times)
            {Map.put(pids, pid, child), times, stacks}

          {:error, :normal} when restart != :permanent ->
            {pids, times, stacks}

          {:error, reason} ->
            {pids, times, Map.put(stacks, pid, {child, reason})}
        end
    end)
  end

  defp monitor_child(pid) do
    ref = Process.monitor(pid)
    Process.unlink(pid)

    receive do
      {:EXIT, ^pid, reason} ->
        receive do
          {:DOWN, ^ref, :process, ^pid, _} -> {:error, reason}
        end
    after
      0 -> :ok
    end
  end

  defp exit_child(pid, {_, _, shutdown, _, _}, times) do
    case shutdown do
      :brutal_kill ->
        Process.exit(pid, :kill)
        times

      :infinity ->
        Process.exit(pid, :shutdown)
        times

      time ->
        Process.exit(pid, :shutdown)
        Map.update(times, time, [pid], &[pid | &1])
    end
  end

  defp wait_children(_pids, 0, timers, stacks) do
    for {timer, _} <- timers do
      _ = :erlang.cancel_timer(timer)

      receive do
        {:timeout, ^timer, :kill} -> :ok
      after
        0 -> :ok
      end
    end

    stacks
  end

  defp wait_children(pids, size, timers, stacks) do
    receive do
      {:DOWN, _ref, :process, pid, reason} ->
        case pids do
          %{^pid => child} ->
            stacks = wait_child(pid, child, reason, stacks)
            wait_children(pids, size - 1, timers, stacks)

          %{} ->
            wait_children(pids, size, timers, stacks)
        end

      {:timeout, timer, :kill} ->
        for pid <- Map.fetch!(timers, timer), do: Process.exit(pid, :kill)
        wait_children(pids, size, Map.delete(timers, timer), stacks)
    end
  end

  defp wait_child(pid, {_, _, :brutal_kill, _, _} = child, reason, stacks) do
    case reason do
      :killed -> stacks
      _ -> Map.put(stacks, pid, {child, reason})
    end
  end

  defp wait_child(pid, {_, restart, _, _, _} = child, reason, stacks) do
    case reason do
      {:shutdown, _} -> stacks
      :shutdown -> stacks
      :normal when restart != :permanent -> stacks
      reason -> Map.put(stacks, pid, {child, reason})
    end
  end

  defp maybe_restart_child(pid, reason, %{children: children} = state) do
    case children do
      %{^pid => {_, restart, _, _, _} = child} ->
        maybe_restart_child(restart, reason, pid, child, state)

      %{} ->
        {:ok, state}
    end
  end

  defp maybe_restart_child(:permanent, reason, pid, child, state) do
    report_error(:child_terminated, reason, pid, child, state)
    restart_child(pid, child, state)
  end

  defp maybe_restart_child(_, :normal, pid, _child, state) do
    {:ok, delete_child(pid, state)}
  end

  defp maybe_restart_child(_, :shutdown, pid, _child, state) do
    {:ok, delete_child(pid, state)}
  end

  defp maybe_restart_child(_, {:shutdown, _}, pid, _child, state) do
    {:ok, delete_child(pid, state)}
  end

  defp maybe_restart_child(:transient, reason, pid, child, state) do
    report_error(:child_terminated, reason, pid, child, state)
    restart_child(pid, child, state)
  end

  defp maybe_restart_child(:temporary, reason, pid, child, state) do
    report_error(:child_terminated, reason, pid, child, state)
    {:ok, delete_child(pid, state)}
  end

  defp delete_child(pid, %{children: children} = state) do
    %{state | children: Map.delete(children, pid)}
  end

  defp restart_child(pid, child, state) do
    case add_restart(state) do
      {:ok, %{strategy: strategy} = state} ->
        case restart_child(strategy, pid, child, state) do
          {:ok, state} ->
            {:ok, state}

          {:try_again, state} ->
            send(self(), {:"$gen_restart", pid})
            {:ok, state}
        end

      {:shutdown, state} ->
        report_error(:shutdown, :reached_max_restart_intensity, pid, child, state)
        {:shutdown, delete_child(pid, state)}
    end
  end

  defp add_restart(state) do
    %{max_seconds: max_seconds, max_restarts: max_restarts, restarts: restarts} = state

    now = :erlang.monotonic_time(1)
    restarts = add_restart([now | restarts], now, max_seconds)
    state = %{state | restarts: restarts}

    if length(restarts) <= max_restarts do
      {:ok, state}
    else
      {:shutdown, state}
    end
  end

  defp add_restart(restarts, now, period) do
    for then <- restarts, now <= then + period, do: then
  end

  defp restart_child(:one_for_one, current_pid, child, state) do
    {{m, f, args} = mfa, restart, shutdown, type, modules} = child
    %{extra_arguments: extra} = state

    case start_child(m, f, extra ++ args) do
      {:ok, pid, _} ->
        state = delete_child(current_pid, state)
        {:ok, save_child(pid, mfa, restart, shutdown, type, modules, state)}

      {:ok, pid} ->
        state = delete_child(current_pid, state)
        {:ok, save_child(pid, mfa, restart, shutdown, type, modules, state)}

      :ignore ->
        {:ok, delete_child(current_pid, state)}

      {:error, reason} ->
        report_error(:start_error, reason, {:restarting, current_pid}, child, state)
        state = put_in(state.children[current_pid], {:restarting, child})
        {:try_again, state}
    end
  end

  defp report_error(error, reason, pid, child, %{name: name, extra_arguments: extra}) do
    :logger.error(
      %{
        label: {:supervisor, error},
        report: [
          {:supervisor, name},
          {:errorContext, error},
          {:reason, reason},
          {:offender, extract_child(pid, child, extra)}
        ]
      },
      %{
        domain: [:otp, :sasl],
        report_cb: &:logger.format_otp_report/1,
        logger_formatter: %{title: "SUPERVISOR REPORT"},
        error_logger: %{tag: :error_report, type: :supervisor_report}
      }
    )
  end

  defp extract_child(pid, {{m, f, args}, restart, shutdown, type, _modules}, extra) do
    [
      pid: pid,
      id: :undefined,
      mfargs: {m, f, extra ++ args},
      restart_type: restart,
      shutdown: shutdown,
      child_type: type
    ]
  end

  ## Helpers



  defp call(supervisor, req) do
    GenServer.call(supervisor, req, :infinity)
  end


  def format_report(%{
        label: {__MODULE__, :unexpected_msg},
        report: %{msg: msg}
      }) do
    {~c"DynamicSupervisor received unexpected message: ~p~n", [msg]}
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/dynamic_supervisor.ex (docs and specs stripped;
# line numbers match the original).
