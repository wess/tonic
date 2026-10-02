defmodule PartitionSupervisor do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.










































































































































  @behaviour Supervisor

  @registry PartitionSupervisor.Registry








  def child_spec(opts) when is_list(opts) do
    id =
      case Keyword.get(opts, :name, PartitionSupervisor) do
        name when is_atom(name) -> name
        {:via, _module, name} -> name
      end

    %{
      id: id,
      start: {PartitionSupervisor, :start_link, [opts]},
      type: :supervisor
    }
  end































































  def start_link(opts) when is_list(opts) do
    name = opts[:name]

    if !name do
      raise ArgumentError, "the :name option must be given to PartitionSupervisor"
    end

    {child_spec, opts} = Keyword.pop(opts, :child_spec)

    if !child_spec do
      raise ArgumentError, "the :child_spec option must be given to PartitionSupervisor"
    end

    {partitions, opts} = Keyword.pop(opts, :partitions, System.schedulers_online())

    if not (is_integer(partitions) and partitions >= 1) do
      raise ArgumentError,
            "the :partitions option must be a positive integer, got: #{inspect(partitions)}"
    end

    {with_arguments, opts} = Keyword.pop(opts, :with_arguments, fn args, _partition -> args end)

    if not is_function(with_arguments, 2) do
      raise ArgumentError,
            "the :with_arguments option must be a function that receives two arguments, " <>
              "the current call arguments and the partition, got: #{inspect(with_arguments)}"
    end

    %{start: {mod, fun, args}} = map = Supervisor.child_spec(child_spec, [])
    modules = map[:modules] || [mod]

    children =
      for partition <- 0..(partitions - 1) do
        args = with_arguments.(args, partition)

        if not is_list(args) do
          raise "the call to the function in :with_arguments must return a list, got: #{inspect(args)}"
        end

        start = {__MODULE__, :start_child, [mod, fun, args, partition]}
        Map.merge(map, %{id: partition, start: start, modules: modules})
      end

    auto_shutdown = Keyword.get(opts, :auto_shutdown, :never)

    if auto_shutdown != :never do
      raise ArgumentError,
            "the :auto_shutdown option must be :never, got: #{inspect(auto_shutdown)}"
    end

    {init_opts, start_opts} =
      Keyword.split(opts, [:strategy, :max_seconds, :max_restarts, :auto_shutdown])

    Supervisor.start_link(__MODULE__, {name, partitions, children, init_opts}, start_opts)
  end


  def start_child(mod, fun, args, partition) do
    case apply(mod, fun, args) do
      {:ok, pid} ->
        register_child(partition, pid)
        {:ok, pid}

      {:ok, pid, info} ->
        register_child(partition, pid)
        {:ok, pid, info}

      other ->
        other
    end
  end

  defp register_child(partition, pid) do
    :ets.insert(Process.get(:ets_table), {partition, pid})
  end


  def init({name, partitions, children, init_opts}) do
    table = init_table(name)
    :ets.insert(table, {:partitions, partitions, partitions})
    Process.put(:ets_table, table)
    Supervisor.init(children, Keyword.put_new(init_opts, :strategy, :one_for_one))
  end

  defp init_table(name) when is_atom(name) do
    :ets.new(name, [:set, :named_table, :public, read_concurrency: true])
  end

  defp init_table({:via, _, _}) do
    table = :ets.new(__MODULE__, [:set, :public, read_concurrency: true])
    ensure_registry()
    Registry.register(@registry, self(), table)
    table
  end

  defp ensure_registry do
    if Process.whereis(@registry) == nil do
      Supervisor.start_child(:elixir_sup, {Registry, keys: :unique, name: @registry})
    end
  end














  def resize!(name, partitions) when is_integer(partitions) do
    supervisor =
      GenServer.whereis(name) || exit({:noproc, {__MODULE__, :resize!, [name, partitions]}})

    table = table(name)
    ensure_registry()

    Registry.lock(@registry, supervisor, fn ->
      case :ets.lookup(table, :partitions) do
        [{:partitions, _current, max}] when partitions not in 0..max//1 ->
          raise ArgumentError,
                "the number of partitions to resize to must be a number between 0 and #{max}, got: #{partitions}"

        [{:partitions, current, max}] when partitions > current ->
          for id <- current..(partitions - 1) do
            case Supervisor.restart_child(supervisor, id) do
              {:ok, _} ->
                :ok

              {:ok, _, _} ->
                :ok

              {:error, reason} ->
                raise "cannot restart partition #{id} of PartitionSupervisor #{inspect(name)} due to reason #{inspect(reason)}"
            end
          end

          :ets.insert(table, {:partitions, partitions, max})
          current

        [{:partitions, current, max}] when partitions < current ->
          :ets.insert(table, {:partitions, partitions, max})

          for id <- partitions..(current - 1) do
            case Supervisor.terminate_child(supervisor, id) do
              :ok ->
                :ok

              {:error, reason} ->
                raise "cannot terminate partition #{id} of PartitionSupervisor #{inspect(name)} due to reason #{inspect(reason)}"
            end
          end

          current

        [{:partitions, current, _max}] ->
          current
      end
    end)
  end






  def partitions(name) do
    name |> table() |> partitions(name)
  end

  defp partitions(table, name) do
    try do
      :ets.lookup_element(table, :partitions, 2)
    rescue
      _ -> exit({:noproc, {__MODULE__, :partitions, [name]}})
    end
  end

  defp table(name) when is_atom(name) do
    name
  end

  # For whereis_name, we want to lookup on GenServer.whereis/1
  # just once, so we lookup the name and partitions together.
  defp table(name) when is_tuple(name) do
    with pid when is_pid(pid) <- GenServer.whereis(name),
         [{_, table}] <- Registry.lookup(@registry, pid) do
      table
    else
      _ -> exit({:noproc, {__MODULE__, :partitions, [name]}})
    end
  end






















  def which_children(name) when is_atom(name) or elem(name, 0) == :via do
    Supervisor.which_children(name)
  end

























  def count_children(supervisor) when is_atom(supervisor) do
    Supervisor.count_children(supervisor)
  end













  def stop(supervisor, reason \\ :normal, timeout \\ :infinity) when is_atom(supervisor) do
    Supervisor.stop(supervisor, reason, timeout)
  end

  ## Via callbacks


  def whereis_name({name, key}) when is_atom(name) or is_tuple(name) do
    table = table(name)
    partitions = partitions(table, name)

    if partitions == 0 do
      raise ArgumentError, "PartitionSupervisor #{inspect(name)} has zero partitions"
    end

    partition =
      if is_integer(key), do: rem(abs(key), partitions), else: :erlang.phash2(key, partitions)

    :ets.lookup_element(table, partition, 2)
  end


  def send(name_key, msg) do
    Kernel.send(whereis_name(name_key), msg)
  end


  def register_name(_, _) do
    raise "{:via, PartitionSupervisor, _} cannot be given on registration"
  end


  def unregister_name(_, _) do
    raise "{:via, PartitionSupervisor, _} cannot be given on unregistration"
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/partition_supervisor.ex (docs and specs stripped;
# line numbers match the original).
