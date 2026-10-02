defmodule Task.Supervisor do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.




















































































  def child_spec(opts) when is_list(opts) do
    id =
      case Keyword.get(opts, :name, Task.Supervisor) do
        name when is_atom(name) -> name
        {:global, name} -> name
        {:via, _module, name} -> name
      end

    %{
      id: id,
      start: {Task.Supervisor, :start_link, [opts]},
      type: :supervisor
    }
  end

































  def start_link(options \\ []) do
    {restart, options} = Keyword.pop(options, :restart)
    {shutdown, options} = Keyword.pop(options, :shutdown)

    if restart || shutdown do
      IO.warn(
        ":restart and :shutdown options in Task.Supervisor.start_link/1 " <>
          "are deprecated. Please pass those options on start_child/3 instead"
      )
    end

    keys = [:max_children, :max_seconds, :max_restarts]
    {sup_opts, start_opts} = Keyword.split(options, keys)
    restart_and_shutdown = {restart || :temporary, shutdown || 5000}
    DynamicSupervisor.start_link(__MODULE__, {restart_and_shutdown, sup_opts}, start_opts)
  end


  def init({{_restart, _shutdown} = arg, options}) do
    Process.put(__MODULE__, arg)
    DynamicSupervisor.init([strategy: :one_for_one] ++ options)
  end



















  def async(supervisor, fun, options \\ []) do
    async(supervisor, :erlang, :apply, [fun, []], options)
  end



















  def async(supervisor, module, fun, args, options \\ []) do
    async(supervisor, :link, module, fun, args, options)
  end




















































































  def async_nolink(supervisor, fun, options \\ []) do
    async_nolink(supervisor, :erlang, :apply, [fun, []], options)
  end
















  def async_nolink(supervisor, module, fun, args, options \\ []) do
    async(supervisor, :nolink, module, fun, args, options)
  end



































































  def async_stream(supervisor, enumerable, module, function, args, options \\ [])
      when is_atom(module) and is_atom(function) and is_list(args) do
    build_stream(supervisor, :link, enumerable, {module, function, args}, options)
  end


















  def async_stream(supervisor, enumerable, fun, options \\ []) when is_function(fun, 1) do
    build_stream(supervisor, :link, enumerable, fun, options)
  end




















  def async_stream_nolink(supervisor, enumerable, module, function, args, options \\ [])
      when is_atom(module) and is_atom(function) and is_list(args) do
    build_stream(supervisor, :nolink, enumerable, {module, function, args}, options)
  end



















































  def async_stream_nolink(supervisor, enumerable, fun, options \\ []) when is_function(fun, 1) do
    build_stream(supervisor, :nolink, enumerable, fun, options)
  end





  def terminate_child(supervisor, pid) when is_pid(pid) do
    DynamicSupervisor.terminate_child(supervisor, pid)
  end









  def children(supervisor) do
    for {_, pid, _, _} <- DynamicSupervisor.which_children(supervisor), is_pid(pid), do: pid
  end




























  def start_child(supervisor, fun, options \\ []) do
    restart = options[:restart]
    shutdown = options[:shutdown]
    args = [get_owner(self()), get_callers(self()), {:erlang, :apply, [fun, []]}]
    start_child_with_spec(supervisor, args, restart, shutdown)
  end









  def start_child(supervisor, module, fun, args, options \\ [])
      when is_atom(fun) and is_list(args) do
    restart = options[:restart]
    shutdown = options[:shutdown]
    mfa = {module, fun, args}
    owner = get_owner(self())
    callers = get_callers(self())

    if restart == :temporary or restart == nil do
      start_child_maybe_temporary(supervisor, owner, callers, restart, shutdown, mfa)
    else
      start_child_with_spec(supervisor, [owner, callers, mfa], restart, shutdown)
    end
  end

  defp start_child_maybe_temporary(supervisor, owner, callers, restart, shutdown, mfa) do
    case start_child_with_spec(supervisor, [owner, :monitor], restart, shutdown) do
      # TODO: This only exists because we need to support reading restart/shutdown
      # from two different places. Remove this, the init function and the associated
      # clause in DynamicSupervisor on Elixir v2.0
      {:restart, restart} ->
        start_child_with_spec(supervisor, [owner, callers, mfa], restart, shutdown)

      {:ok, pid} ->
        # We mimic async but there is nothing to reply to
        alias = make_ref()
        send(pid, {self(), alias, alias, callers, mfa})
        {:ok, pid}

      {:error, _} = error ->
        error
    end
  end

  defp start_child_with_spec(supervisor, args, restart, shutdown) do
    GenServer.call(supervisor, {:start_task, args, restart, shutdown}, :infinity)
  end

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

  defp async(supervisor, link_type, module, fun, args, options) do
    owner = self()
    shutdown = options[:shutdown]

    case start_child_with_spec(supervisor, [get_owner(owner), :monitor], :temporary, shutdown) do
      {:ok, pid} ->
        if link_type == :link, do: Process.link(pid)
        alias = :erlang.monitor(:process, pid, alias: :demonitor)
        send(pid, {owner, alias, alias, get_callers(owner), {module, fun, args}})
        %Task{pid: pid, ref: alias, owner: owner, mfa: {module, fun, length(args)}}

      {:error, :max_children} ->
        raise """
        reached the maximum number of tasks for this task supervisor. The maximum number \
        of tasks that are allowed to run at the same time under this supervisor can be \
        configured with the :max_children option passed to Task.Supervisor.start_link/1\
        """
    end
  end

  defp build_stream(supervisor, link_type, enumerable, fun, options) do
    shutdown = Keyword.get(options, :shutdown, 5000)

    if not ((is_integer(shutdown) and shutdown >= 0) or shutdown == :brutal_kill) do
      raise ArgumentError, ":shutdown must be either a positive integer or :brutal_kill"
    end

    options = Task.Supervised.validate_stream_options(options)

    fn acc, acc_fun ->
      owner = get_owner(self())

      Task.Supervised.stream(enumerable, acc, acc_fun, get_callers(self()), fun, options, fn ->
        args = [owner, :monitor]

        case start_child_with_spec(supervisor, args, :temporary, shutdown) do
          {:ok, pid} ->
            if link_type == :link, do: Process.link(pid)
            {:ok, link_type, pid}

          {:error, :max_children} ->
            {:error, :max_children}
        end
      end)
    end
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/task/supervisor.ex (docs and specs stripped;
# line numbers match the original).
