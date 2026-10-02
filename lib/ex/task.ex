defmodule Task do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.

















































































































































































































































































  @enforce_keys [:mfa, :owner, :pid, :ref]
  defstruct @enforce_keys





























  defguardp is_timeout(timeout)
            when timeout == :infinity or (is_integer(timeout) and timeout >= 0)












  def child_spec(arg) do
    %{
      id: Task,
      start: {Task, :start_link, [arg]},
      restart: :temporary
    }
  end


  defmacro __using__(opts) do
    quote location: :keep, bind_quoted: [opts: opts] do
      if not Module.has_attribute?(__MODULE__, :doc) do









      end

      def child_spec(arg) do
        default = %{
          id: __MODULE__,
          start: {__MODULE__, :start_link, [arg]},
          restart: :temporary
        }

        Supervisor.child_spec(default, unquote(Macro.escape(opts)))
      end

      defoverridable child_spec: 1
    end
  end









  def start_link(fun) when is_function(fun, 0) do
    start_link(:erlang, :apply, [fun, []])
  end








  def start_link(module, function, args)
      when is_atom(module) and is_atom(function) and is_list(args) do
    mfa = {module, function, args}
    Task.Supervised.start_link(get_owner(self()), get_callers(self()), mfa)
  end
















  def start(fun) when is_function(fun, 0) do
    start(:erlang, :apply, [fun, []])
  end














  def start(module, function_name, args)
      when is_atom(module) and is_atom(function_name) and is_list(args) do
    mfa = {module, function_name, args}
    Task.Supervised.start(get_owner(self()), get_callers(self()), mfa)
  end










































































  def async(fun) when is_function(fun, 0) do
    async(:erlang, :apply, [fun, []])
  end










  def async(module, function_name, args)
      when is_atom(module) and is_atom(function_name) and is_list(args) do
    mfargs = {module, function_name, args}
    owner = self()
    # No need to monitor because the processes are linked
    {:ok, pid} = Task.Supervised.start_link(get_owner(owner), :nomonitor)

    alias = build_alias(pid)
    send(pid, {owner, alias, alias, get_callers(owner), mfargs})
    %Task{pid: pid, ref: alias, owner: owner, mfa: {module, function_name, length(args)}}
  end


































  def completed(result) do
    ref = make_ref()
    owner = self()

    # "complete" the task immediately
    send(owner, {ref, result})

    %Task{pid: nil, ref: ref, owner: owner, mfa: {Task, :completed, 1}}
  end








































































































































  def async_stream(enumerable, module, function_name, args, options \\ [])
      when is_atom(module) and is_atom(function_name) and is_list(args) do
    build_stream(enumerable, {module, function_name, args}, options)
  end

























  def async_stream(enumerable, fun, options \\ [])
      when is_function(fun, 1) and is_list(options) do
    build_stream(enumerable, fun, options)
  end

  defp build_stream(enumerable, fun, options) do
    options = Task.Supervised.validate_stream_options(options)

    fn acc, acc_fun ->
      owner = get_owner(self())

      Task.Supervised.stream(enumerable, acc, acc_fun, get_callers(self()), fun, options, fn ->
        # No need to monitor because the processes are linked
        {:ok, pid} = Task.Supervised.start_link(owner, :nomonitor)
        {:ok, :link, pid}
      end)
    end
  end

  # Returns a tuple with the node where this is executed and either the
  # registered name of the given PID or the PID of where this is executed. Used
  # when exiting from tasks to print out from where the task was started.
  defp get_owner(pid) do
    self_or_name =
      case Process.info(pid, :registered_name) do
        {:registered_name, name} when is_atom(name) -> name
        _ -> pid
      end

    {node(), self_or_name, pid}
  end

  defp get_callers(owner) do
    case :erlang.get(:"$callers") do
      [_ | _] = list -> [owner | list]
      _ -> [owner]
    end
  end









































































































  def await(%Task{ref: ref, owner: owner} = task, timeout \\ 5000) when is_timeout(timeout) do
    if owner != self() do
      raise ArgumentError, invalid_owner_error(task)
    end

    await_receive(ref, task, timeout)
  end

  defp await_receive(ref, task, timeout) do
    receive do
      {^ref, reply} ->
        demonitor(ref)
        reply

      {:DOWN, ^ref, _, proc, reason} ->
        exit({reason(reason, proc), {__MODULE__, :await, [task, timeout]}})
    after
      timeout ->
        demonitor(ref)
        exit({:timeout, {__MODULE__, :await, [task, timeout]}})
    end
  end
















  def ignore(%Task{ref: ref, pid: pid, owner: owner} = task) do
    if owner != self() do
      raise ArgumentError, invalid_owner_error(task)
    end

    ignore_receive(ref, pid, task)
  end

  defp ignore_receive(ref, pid, task) do
    receive do
      {^ref, reply} ->
        pid && Process.unlink(pid)
        demonitor(ref)
        {:ok, reply}

      {:DOWN, ^ref, _, proc, :noconnection} ->
        exit({reason(:noconnection, proc), {__MODULE__, :ignore, [task]}})

      {:DOWN, ^ref, _, _, reason} ->
        {:exit, reason}
    after
      0 ->
        pid && Process.unlink(pid)
        demonitor(ref)
        nil
    end
  end











































  def await_many(tasks, timeout \\ 5000) when is_timeout(timeout) do
    awaiting =
      Map.new(tasks, fn %Task{ref: ref, owner: owner} = task ->
        if owner != self() do
          raise ArgumentError, invalid_owner_error(task)
        end

        {ref, true}
      end)

    timeout_ref = make_ref()

    timer_ref =
      if timeout != :infinity do
        Process.send_after(self(), timeout_ref, timeout)
      end

    try do
      await_many(tasks, timeout, awaiting, %{}, timeout_ref)
    after
      timer_ref && Process.cancel_timer(timer_ref)
      receive do: (^timeout_ref -> :ok), after: (0 -> :ok)
    end
  end

  defp await_many(tasks, _timeout, awaiting, replies, _timeout_ref)
       when map_size(awaiting) == 0 do
    for %{ref: ref} <- tasks, do: Map.fetch!(replies, ref)
  end

  defp await_many(tasks, timeout, awaiting, replies, timeout_ref) do
    receive do
      ^timeout_ref ->
        demonitor_pending_tasks(awaiting)
        exit({:timeout, {__MODULE__, :await_many, [tasks, timeout]}})

      {:DOWN, ref, _, proc, reason} when is_map_key(awaiting, ref) ->
        demonitor_pending_tasks(awaiting)
        exit({reason(reason, proc), {__MODULE__, :await_many, [tasks, timeout]}})

      {ref, reply} when is_map_key(awaiting, ref) ->
        demonitor(ref)

        await_many(
          tasks,
          timeout,
          Map.delete(awaiting, ref),
          Map.put(replies, ref, reply),
          timeout_ref
        )
    end
  end

  defp demonitor_pending_tasks(awaiting) do
    Enum.each(awaiting, fn {ref, _} ->
      demonitor(ref)
    end)
  end



  def find(tasks, {ref, reply}) when is_reference(ref) do
    Enum.find_value(tasks, fn
      %Task{ref: ^ref} = task ->
        demonitor(ref)
        {reply, task}

      %Task{} ->
        nil
    end)
  end

  def find(tasks, {:DOWN, ref, _, proc, reason} = msg) when is_reference(ref) do
    find = fn %Task{ref: task_ref} -> task_ref == ref end

    if Enum.find(tasks, find) do
      exit({reason(reason, proc), {__MODULE__, :find, [tasks, msg]}})
    end
  end

  def find(_tasks, _msg) do
    nil
  end






















































  def yield(%Task{ref: ref, owner: owner} = task, timeout \\ 5000) when is_timeout(timeout) do
    if owner != self() do
      raise ArgumentError, invalid_owner_error(task)
    end

    yield_receive(ref, task, timeout)
  end

  defp yield_receive(ref, task, timeout) do
    receive do
      {^ref, reply} ->
        demonitor(ref)
        {:ok, reply}

      {:DOWN, ^ref, _, proc, :noconnection} ->
        exit({reason(:noconnection, proc), {__MODULE__, :yield, [task, timeout]}})

      {:DOWN, ^ref, _, _, reason} ->
        {:exit, reason}
    after
      timeout ->
        nil
    end
  end



























































































  def yield_many(tasks, opts \\ [])

  def yield_many(tasks, timeout) when is_timeout(timeout) do
    yield_many(tasks, timeout: timeout)
  end

  def yield_many(tasks, opts) when is_list(opts) do
    refs =
      Map.new(tasks, fn %Task{ref: ref, owner: owner} = task ->
        if owner != self() do
          raise ArgumentError, invalid_owner_error(task)
        end

        {ref, nil}
      end)

    on_timeout = Keyword.get(opts, :on_timeout, :nothing)
    timeout = Keyword.get(opts, :timeout, 5_000)
    limit = Keyword.get(opts, :limit, map_size(refs))
    timeout_ref = make_ref()

    timer_ref =
      if timeout != :infinity do
        Process.send_after(self(), timeout_ref, timeout)
      end

    try do
      yield_many(limit, refs, timeout_ref, timer_ref)
    catch
      {:noconnection, reason} ->
        exit({reason, {__MODULE__, :yield_many, [tasks, timeout]}})
    else
      {timed_out?, refs} ->
        for task <- tasks do
          value =
            with nil <- Map.fetch!(refs, task.ref) do
              case on_timeout do
                _ when not timed_out? -> nil
                :nothing -> nil
                :kill_task -> shutdown(task, :brutal_kill)
                :ignore -> ignore(task)
              end
            end

          {task, value}
        end
    end
  end

  defp yield_many(0, refs, timeout_ref, timer_ref) do
    timer_ref && Process.cancel_timer(timer_ref)
    receive do: (^timeout_ref -> :ok), after: (0 -> :ok)
    {false, refs}
  end

  defp yield_many(limit, refs, timeout_ref, timer_ref) do
    receive do
      {ref, reply} when is_map_key(refs, ref) ->
        demonitor(ref)
        yield_many(limit - 1, %{refs | ref => {:ok, reply}}, timeout_ref, timer_ref)

      {:DOWN, ref, _, proc, reason} when is_map_key(refs, ref) ->
        if reason == :noconnection do
          throw({:noconnection, reason(:noconnection, proc)})
        else
          yield_many(limit - 1, %{refs | ref => {:exit, reason}}, timeout_ref, timer_ref)
        end

      ^timeout_ref ->
        {true, refs}
    end
  end






























  def shutdown(task, shutdown \\ 5000)

  def shutdown(%Task{pid: nil} = task, _) do
    ignore(task)
  end

  def shutdown(%Task{owner: owner} = task, _) when owner != self() do
    raise ArgumentError, invalid_owner_error(task)
  end

  def shutdown(%Task{pid: pid, ref: ref} = task, :brutal_kill) do
    mon = build_monitor(pid)
    shutdown_send(pid, :kill)

    case shutdown_receive(ref, mon, task, :brutal_kill, :infinity) do
      {:down, proc, :noconnection} ->
        exit({reason(:noconnection, proc), {__MODULE__, :shutdown, [task, :brutal_kill]}})

      {:down, _, reason} ->
        {:exit, reason}

      result ->
        result
    end
  end

  def shutdown(%Task{pid: pid, ref: ref} = task, timeout) when is_timeout(timeout) do
    mon = build_monitor(pid)
    shutdown_send(pid, :shutdown)

    case shutdown_receive(ref, mon, task, :shutdown, timeout) do
      {:down, proc, :noconnection} ->
        exit({reason(:noconnection, proc), {__MODULE__, :shutdown, [task, timeout]}})

      {:down, _, reason} ->
        {:exit, reason}

      result ->
        result
    end
  end

  # Spawn a process to ensure task gets exit signal
  # if process dies from exit signal between unlink and exit.
  defp shutdown_send(pid, reason) do
    caller = self()
    ref = make_ref()
    enforcer = spawn(fn -> shutdown_send(pid, reason, caller, ref) end)
    Process.unlink(pid)
    Process.exit(pid, reason)
    send(enforcer, {:done, ref})
    :ok
  end

  defp shutdown_send(pid, reason, caller, ref) do
    mon = Process.monitor(caller)

    receive do
      {:done, ^ref} -> :ok
      {:DOWN, ^mon, _, _, _} -> Process.exit(pid, reason)
    end
  end

  defp shutdown_receive(ref, mon, task, type, timeout) do
    receive do
      {:DOWN, ^mon, _, _, :shutdown} when type in [:shutdown, :timeout_kill] ->
        demonitor(ref)
        flush_reply(ref)

      {:DOWN, ^mon, _, _, :killed} when type == :brutal_kill ->
        demonitor(ref)
        flush_reply(ref)

      {:DOWN, ^mon, _, proc, :noproc} ->
        reason = flush_noproc(ref, proc, type)
        flush_reply(ref) || reason

      {:DOWN, ^mon, _, proc, reason} ->
        demonitor(ref)
        flush_reply(ref) || {:down, proc, reason}
    after
      timeout ->
        Process.exit(task.pid, :kill)
        shutdown_receive(ref, mon, task, :timeout_kill, :infinity)
    end
  end

  defp flush_reply(ref) do
    receive do
      {^ref, reply} -> {:ok, reply}
    after
      0 -> nil
    end
  end

  defp flush_noproc(ref, proc, type) do
    receive do
      {:DOWN, ^ref, _, _, :shutdown} when type in [:shutdown, :timeout_kill] ->
        nil

      {:DOWN, ^ref, _, _, :killed} when type == :brutal_kill ->
        nil

      {:DOWN, ^ref, _, _, reason} ->
        {:down, proc, reason}
    after
      0 ->
        demonitor(ref)
        {:down, proc, :noproc}
    end
  end

  ## Optimizations

  defp build_monitor(pid) do
    :erlang.monitor(:process, pid)
  end

  defp build_alias(pid) do
    :erlang.monitor(:process, pid, alias: :demonitor)
  end


  # This instructs the Erlang compiler to apply selective
  # receive optimizations to several functions in this module.
  # This function is never invoked directly, it is only here
  # for compiler optimization purposes.
  #
  # To verify which functions have been optimized, run the
  # following command after Elixir is compiled from the project
  # root:
  #
  #     ERL_COMPILER_OPTIONS=recv_opt_info elixir lib/elixir/lib/task.ex
  #
  def __recv_opt_info__(pid, task) do
    await_receive(build_alias(pid), task, :infinity)
    shutdown_receive(build_alias(pid), build_monitor(pid), task, :shutdown, :infinity)
    yield_receive(build_alias(pid), task, :infinity)
    ignore_receive(build_alias(pid), pid, task)
  end

  ## Helpers

  defp demonitor(ref) when is_reference(ref) do
    Process.demonitor(ref, [:flush])
    :ok
  end

  defp reason(:noconnection, proc), do: {:nodedown, monitor_node(proc)}
  defp reason(reason, _), do: reason

  defp monitor_node(pid) when is_pid(pid), do: node(pid)
  defp monitor_node({_, node}), do: node

  defp invalid_owner_error(task) do
    "task #{inspect(task)} must be queried from the owner but was queried from #{inspect(self())}"
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/task.ex (docs and specs stripped;
# line numbers match the original).
