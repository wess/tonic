defmodule Registry do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.












































































































































































  @keys [:unique, :duplicate]
  @all_info -1
  @key_info -2




















































  ## Via callbacks


  def whereis_name({registry, key}), do: whereis_name(registry, key)
  def whereis_name({registry, key, _value}), do: whereis_name(registry, key)

  defp whereis_name(registry, key) do
    case key_info!(registry) do
      {:unique, partitions, key_ets} ->
        key_ets = key_ets || key_ets!(registry, key, partitions)

        case safe_lookup_second(key_ets, key) do
          {pid, _} ->
            if Process.alive?(pid), do: pid, else: :undefined

          _ ->
            :undefined
        end

      {kind, _, _} ->
        raise ArgumentError, ":via is not supported for #{kind} registries"
    end
  end


  def register_name({registry, key}, pid), do: register_name(registry, key, nil, pid)
  def register_name({registry, key, value}, pid), do: register_name(registry, key, value, pid)

  defp register_name(registry, key, value, pid) when pid == self() do
    case register(registry, key, value) do
      {:ok, _} -> :yes
      {:error, _} -> :no
    end
  end


  def send({registry, key}, msg) do
    case lookup(registry, key) do
      [{pid, _}] -> Kernel.send(pid, msg)
      [] -> :erlang.error(:badarg, [{registry, key}, msg])
    end
  end

  def send({registry, key, _value}, msg) do
    Registry.send({registry, key}, msg)
  end


  def unregister_name({registry, key}), do: unregister(registry, key)
  def unregister_name({registry, key, _value}), do: unregister(registry, key)

  ## Registry API

















































  def start_link(options) do
    keys = Keyword.get(options, :keys)

    if keys not in @keys do
      raise ArgumentError,
            "expected :keys to be given and be one of :unique or :duplicate, got: #{inspect(keys)}"
    end

    name =
      case Keyword.fetch(options, :name) do
        {:ok, name} when is_atom(name) ->
          name

        {:ok, other} ->
          raise ArgumentError, "expected :name to be an atom, got: #{inspect(other)}"

        :error ->
          raise ArgumentError, "expected :name option to be present"
      end

    meta = Keyword.get(options, :meta, [])

    if not Keyword.keyword?(meta) do
      raise ArgumentError, "expected :meta to be a keyword list, got: #{inspect(meta)}"
    end

    partitions = Keyword.get(options, :partitions, 1)

    if not (is_integer(partitions) and partitions >= 1) do
      raise ArgumentError,
            "expected :partitions to be a positive integer, got: #{inspect(partitions)}"
    end

    listeners = Keyword.get(options, :listeners, [])

    if not (is_list(listeners) and Enum.all?(listeners, &is_atom/1)) do
      raise ArgumentError,
            "expected :listeners to be a list of named processes, got: #{inspect(listeners)}"
    end

    compressed = Keyword.get(options, :compressed, false)

    if not is_boolean(compressed) do
      raise ArgumentError,
            "expected :compressed to be a boolean, got: #{inspect(compressed)}"
    end

    # The @info format must be kept in sync with Registry.Partition optimization.
    entries = [
      {@all_info, {keys, partitions, nil, nil, listeners}},
      {@key_info, {keys, partitions, nil}} | meta
    ]

    Registry.Supervisor.start_link(keys, name, partitions, listeners, entries, compressed)
  end



  def start_link(keys, name, options \\ []) when keys in @keys and is_atom(name) do
    start_link([keys: keys, name: name] ++ options)
  end








  def child_spec(options) do
    %{
      id: Keyword.get(options, :name, Registry),
      start: {Registry, :start_link, [options]},
      type: :supervisor
    }
  end
























  def update_value(registry, key, callback) when is_atom(registry) and is_function(callback, 1) do
    case key_info!(registry) do
      {:unique, partitions, key_ets} ->
        key_ets = key_ets || key_ets!(registry, key, partitions)

        try do
          :ets.lookup_element(key_ets, key, 2)
        catch
          :error, :badarg -> :error
        else
          {pid, old_value} when pid == self() ->
            new_value = callback.(old_value)
            :ets.insert(key_ets, {key, {pid, new_value}})
            {new_value, old_value}

          {_, _} ->
            :error
        end

      {kind, _, _} ->
        raise ArgumentError, "Registry.update_value/3 is not supported for #{kind} registries"
    end
  end





















  def dispatch(registry, key, mfa_or_fun, opts \\ [])
      when is_atom(registry) and is_function(mfa_or_fun, 1)
      when is_atom(registry) and tuple_size(mfa_or_fun) == 3 do
    case key_info!(registry) do
      {:unique, partitions, key_ets} ->
        (key_ets || key_ets!(registry, key, partitions))
        |> safe_lookup_second(key)
        |> List.wrap()
        |> apply_non_empty_to_mfa_or_fun(mfa_or_fun)

      {:duplicate, 1, key_ets} ->
        key_ets
        |> safe_lookup_second(key)
        |> apply_non_empty_to_mfa_or_fun(mfa_or_fun)

      {:duplicate, partitions, _} ->
        if Keyword.get(opts, :parallel, false) do
          registry
          |> dispatch_parallel(key, mfa_or_fun, partitions)
          |> Enum.each(&Task.await(&1, :infinity))
        else
          dispatch_serial(registry, key, mfa_or_fun, partitions)
        end
    end

    :ok
  end

  defp dispatch_serial(_registry, _key, _mfa_or_fun, 0) do
    :ok
  end

  defp dispatch_serial(registry, key, mfa_or_fun, partition) do
    partition = partition - 1

    registry
    |> key_ets!(partition)
    |> safe_lookup_second(key)
    |> apply_non_empty_to_mfa_or_fun(mfa_or_fun)

    dispatch_serial(registry, key, mfa_or_fun, partition)
  end

  defp dispatch_parallel(_registry, _key, _mfa_or_fun, 0) do
    []
  end

  defp dispatch_parallel(registry, key, mfa_or_fun, partition) do
    partition = partition - 1
    parent = self()

    task =
      Task.async(fn ->
        registry
        |> key_ets!(partition)
        |> safe_lookup_second(key)
        |> apply_non_empty_to_mfa_or_fun(mfa_or_fun)

        Process.unlink(parent)
        :ok
      end)

    [task | dispatch_parallel(registry, key, mfa_or_fun, partition)]
  end

  defp apply_non_empty_to_mfa_or_fun([], _mfa_or_fun) do
    :ok
  end

  defp apply_non_empty_to_mfa_or_fun(entries, {module, function, args}) do
    apply(module, function, [entries | args])
  end

  defp apply_non_empty_to_mfa_or_fun(entries, fun) do
    fun.(entries)
  end






































  def lookup(registry, key) when is_atom(registry) do
    case key_info!(registry) do
      {:unique, partitions, key_ets} ->
        key_ets = key_ets || key_ets!(registry, key, partitions)

        case safe_lookup_second(key_ets, key) do
          {_, _} = pair ->
            [pair]

          _ ->
            []
        end

      {:duplicate, 1, key_ets} ->
        safe_lookup_second(key_ets, key)

      {:duplicate, partitions, _key_ets} ->
        for partition <- 0..(partitions - 1),
            pair <- safe_lookup_second(key_ets!(registry, partition), key),
            do: pair
    end
  end





















































  def lock(registry, lock_key, function)
      when is_atom(registry) and is_function(function, 0) do
    {_kind, partitions, _, pid_ets, _} = info!(registry)
    {pid_server, _pid_ets} = pid_ets || pid_ets!(registry, lock_key, partitions)
    Registry.Partition.lock(pid_server, lock_key, function)
  end















































  def match(registry, key, pattern, guards \\ []) when is_atom(registry) and is_list(guards) do
    guards = [{:"=:=", {:element, 1, :"$_"}, {:const, key}} | guards]
    spec = [{{:_, {:_, pattern}}, guards, [{:element, 2, :"$_"}]}]

    case key_info!(registry) do
      {:unique, partitions, key_ets} ->
        key_ets = key_ets || key_ets!(registry, key, partitions)
        :ets.select(key_ets, spec)

      {:duplicate, 1, key_ets} ->
        :ets.select(key_ets, spec)

      {:duplicate, partitions, _key_ets} ->
        for partition <- 0..(partitions - 1),
            pair <- :ets.select(key_ets!(registry, partition), spec),
            do: pair
    end
  end



































  def keys(registry, pid) when is_atom(registry) and is_pid(pid) do
    {kind, partitions, _, pid_ets, _} = info!(registry)
    {_, pid_ets} = pid_ets || pid_ets!(registry, pid, partitions)

    keys =
      try do
        spec = [{{pid, :"$1", :"$2", :_}, [], [{{:"$1", :"$2"}}]}]
        :ets.select(pid_ets, spec)
      catch
        :error, :badarg -> []
      end

    # Handle the possibility of fake keys
    keys = gather_keys(keys, [], false)

    cond do
      kind == :unique -> Enum.uniq(keys)
      true -> keys
    end
  end

  defp gather_keys([{key, {_, remaining}} | rest], acc, _fake) do
    gather_keys(rest, [key | acc], {key, remaining})
  end

  defp gather_keys([{key, _} | rest], acc, fake) do
    gather_keys(rest, [key | acc], fake)
  end

  defp gather_keys([], acc, {key, remaining}) do
    List.duplicate(key, remaining) ++ Enum.reject(acc, &(&1 === key))
  end

  defp gather_keys([], acc, false) do
    acc
  end








































  def values(registry, key, pid) when is_atom(registry) do
    case key_info!(registry) do
      {:unique, partitions, key_ets} ->
        key_ets = key_ets || key_ets!(registry, key, partitions)

        case safe_lookup_second(key_ets, key) do
          {^pid, value} ->
            [value]

          _ ->
            []
        end

      {:duplicate, partitions, key_ets} ->
        key_ets = key_ets || key_ets!(registry, pid, partitions)
        for {^pid, value} <- safe_lookup_second(key_ets, key), do: value
    end
  end









































  def unregister(registry, key) when is_atom(registry) do
    self = self()
    {kind, partitions, key_ets, pid_ets, listeners} = info!(registry)
    {key_partition, pid_partition} = partitions(kind, key, self, partitions)
    key_ets = key_ets || key_ets!(registry, key_partition)
    {pid_server, pid_ets} = pid_ets || pid_ets!(registry, pid_partition)

    # Remove first from the key_ets because in case of crashes
    # the pid_ets will still be able to clean up. The last step is
    # to clean if we have no more entries.
    true = __unregister__(key_ets, {key, {self, :_}}, 1)
    true = __unregister__(pid_ets, {self, key, key_ets, :_}, 2)

    unlink_if_unregistered(pid_server, pid_ets, self)

    for listener <- listeners do
      Kernel.send(listener, {:unregister, registry, key, self})
    end

    :ok
  end









































  def unregister_match(registry, key, pattern, guards \\ []) when is_list(guards) do
    self = self()

    {kind, partitions, key_ets, pid_ets, listeners} = info!(registry)
    {key_partition, pid_partition} = partitions(kind, key, self, partitions)
    key_ets = key_ets || key_ets!(registry, key_partition)
    {pid_server, pid_ets} = pid_ets || pid_ets!(registry, pid_partition)

    # Remove first from the key_ets because in case of crashes
    # the pid_ets will still be able to clean up. The last step is
    # to clean if we have no more entries.

    # Here we want to count all entries for this pid under this key, regardless of pattern.
    underscore_guard = {:"=:=", {:element, 1, :"$_"}, {:const, key}}
    total_spec = [{{:_, {self, :_}}, [underscore_guard], [true]}]
    total = :ets.select_count(key_ets, total_spec)

    # We only want to delete things that match the pattern
    delete_spec = [{{:_, {self, pattern}}, [underscore_guard | guards], [true]}]

    case :ets.select_delete(key_ets, delete_spec) do
      # We deleted everything, we can just delete the object
      ^total ->
        true = __unregister__(pid_ets, {self, key, key_ets, :_}, 2)
        unlink_if_unregistered(pid_server, pid_ets, self)

        for listener <- listeners do
          Kernel.send(listener, {:unregister, registry, key, self})
        end

      0 ->
        :ok

      deleted ->
        # There are still entries remaining for this pid. delete_object/2 with
        # duplicate_bag tables will remove every entry, but we only want to
        # remove those we have deleted. The solution is to introduce a temp_entry
        # that indicates how many keys WILL be remaining after the delete operation.
        counter = System.unique_integer()
        remaining = total - deleted
        temp_entry = {self, key, {key_ets, remaining}, counter}
        true = :ets.insert(pid_ets, temp_entry)
        true = __unregister__(pid_ets, {self, key, key_ets, :_}, 2)
        real_keys = List.duplicate({self, key, key_ets, counter}, remaining)
        true = :ets.insert(pid_ets, real_keys)
        # We've recreated the real remaining key entries, so we can now delete
        # our temporary entry.
        true = :ets.delete_object(pid_ets, temp_entry)
    end

    :ok
  end













































  def register(registry, key, value) when is_atom(registry) do
    self = self()
    {kind, partitions, key_ets, pid_ets, listeners} = info!(registry)
    {key_partition, pid_partition} = partitions(kind, key, self, partitions)
    key_ets = key_ets || key_ets!(registry, key_partition)
    {pid_server, pid_ets} = pid_ets || pid_ets!(registry, pid_partition)

    # Note that we write first to the pid_ets table because it will
    # always be able to do the cleanup. If we register first to the
    # key one and the process crashes, the key will stay there forever.
    Process.link(pid_server)

    counter = System.unique_integer()
    true = :ets.insert(pid_ets, {self, key, key_ets, counter})

    case register_key(kind, key_ets, key, {key, {self, value}}) do
      :ok ->
        for listener <- listeners do
          Kernel.send(listener, {:register, registry, key, self, value})
        end

        {:ok, pid_server}

      {:error, {:already_registered, ^self}} = error ->
        true = :ets.delete_object(pid_ets, {self, key, key_ets, counter})
        error

      {:error, _} = error ->
        true = :ets.delete_object(pid_ets, {self, key, key_ets, counter})
        unlink_if_unregistered(pid_server, pid_ets, self)
        error
    end
  end

  defp register_key(:duplicate, key_ets, _key, entry) do
    true = :ets.insert(key_ets, entry)
    :ok
  end

  defp register_key(:unique, key_ets, key, entry) do
    if :ets.insert_new(key_ets, entry) do
      :ok
    else
      # Note that we have to call register_key recursively
      # because we are always at odds of a race condition.
      case :ets.lookup(key_ets, key) do
        [{^key, {pid, _}} = current] ->
          if Process.alive?(pid) do
            {:error, {:already_registered, pid}}
          else
            :ets.delete_object(key_ets, current)
            register_key(:unique, key_ets, key, entry)
          end

        [] ->
          register_key(:unique, key_ets, key, entry)
      end
    end
  end

















  def meta(registry, key) when is_atom(registry) and (is_atom(key) or is_tuple(key)) do
    try do
      :ets.lookup(registry, key)
    catch
      :error, :badarg ->
        raise ArgumentError,
              "unknown registry: #{inspect(registry)}. Either the registry name is invalid or " <>
                "the registry is not running, possibly because its application isn't started"
    else
      [{^key, value}] -> {:ok, value}
      _ -> :error
    end
  end





















  def put_meta(registry, key, value) when is_atom(registry) and (is_atom(key) or is_tuple(key)) do
    try do
      :ets.insert(registry, {key, value})
      :ok
    catch
      :error, :badarg ->
        raise ArgumentError, "unknown registry: #{inspect(registry)}"
    end
  end



















  def delete_meta(registry, key) when is_atom(registry) and (is_atom(key) or is_tuple(key)) do
    try do
      :ets.delete(registry, key)
      :ok
    catch
      :error, :badarg ->
        raise ArgumentError, "unknown registry: #{inspect(registry)}"
    end
  end






























  def count(registry) when is_atom(registry) do
    case key_info!(registry) do
      {_kind, partitions, nil} ->
        Enum.sum_by(0..(partitions - 1), fn partition_index ->
          safe_size(key_ets!(registry, partition_index))
        end)

      {_kind, 1, key_ets} ->
        safe_size(key_ets)
    end
  end

  defp safe_size(ets) do
    try do
      :ets.info(ets, :size)
    catch
      :error, :badarg -> 0
    end
  end














































  def count_match(registry, key, pattern, guards \\ [])
      when is_atom(registry) and is_list(guards) do
    guards = [{:"=:=", {:element, 1, :"$_"}, {:const, key}} | guards]
    spec = [{{:_, {:_, pattern}}, guards, [true]}]

    case key_info!(registry) do
      {:unique, partitions, key_ets} ->
        key_ets = key_ets || key_ets!(registry, key, partitions)
        :ets.select_count(key_ets, spec)

      {:duplicate, 1, key_ets} ->
        :ets.select_count(key_ets, spec)

      {:duplicate, partitions, _key_ets} ->
        Enum.sum_by(0..(partitions - 1), fn partition_index ->
          :ets.select_count(key_ets!(registry, partition_index), spec)
        end)
    end
  end



















































  def select(registry, spec)
      when is_atom(registry) and is_list(spec) do
    spec = group_match_headers(spec, __ENV__.function)

    case key_info!(registry) do
      {_kind, partitions, nil} ->
        Enum.flat_map(0..(partitions - 1), fn partition_index ->
          :ets.select(key_ets!(registry, partition_index), spec)
        end)

      {_kind, 1, key_ets} ->
        :ets.select(key_ets, spec)
    end
  end

















  def count_select(registry, spec)
      when is_atom(registry) and is_list(spec) do
    spec = group_match_headers(spec, __ENV__.function)

    case key_info!(registry) do
      {_kind, partitions, nil} ->
        Enum.sum_by(0..(partitions - 1), fn partition_index ->
          :ets.select_count(key_ets!(registry, partition_index), spec)
        end)

      {_kind, 1, key_ets} ->
        :ets.select_count(key_ets, spec)
    end
  end

  defp group_match_headers(spec, {fun, arity}) do
    for part <- spec do
      case part do
        {{key, pid, value}, guards, select} ->
          {{key, {pid, value}}, guards, select}

        _ ->
          raise ArgumentError,
                "invalid match specification in Registry.#{fun}/#{arity}: #{inspect(spec)}"
      end
    end
  end

  ## Helpers



  defp hash(term, limit) do
    :erlang.phash2(term, limit)
  end

  defp info!(registry) do
    try do
      :ets.lookup_element(registry, @all_info, 2)
    catch
      :error, :badarg ->
        raise ArgumentError, "unknown registry: #{inspect(registry)}"
    end
  end

  defp key_info!(registry) do
    try do
      :ets.lookup_element(registry, @key_info, 2)
    catch
      :error, :badarg ->
        raise ArgumentError, "unknown registry: #{inspect(registry)}"
    end
  end

  defp key_ets!(registry, key, partitions) do
    :ets.lookup_element(registry, hash(key, partitions), 2)
  end

  defp key_ets!(registry, partition) do
    :ets.lookup_element(registry, partition, 2)
  end

  defp pid_ets!(registry, key, partitions) do
    :ets.lookup_element(registry, hash(key, partitions), 3)
  end

  defp pid_ets!(registry, partition) do
    :ets.lookup_element(registry, partition, 3)
  end

  defp safe_lookup_second(ets, key) do
    try do
      :ets.lookup_element(ets, key, 2)
    catch
      :error, :badarg -> []
    end
  end

  defp partitions(:unique, key, pid, partitions) do
    {hash(key, partitions), hash(pid, partitions)}
  end

  defp partitions(:duplicate, _key, pid, partitions) do
    partition = hash(pid, partitions)
    {partition, partition}
  end

  defp unlink_if_unregistered(pid_server, pid_ets, self) do
    if not :ets.member(pid_ets, self) do
      Process.unlink(pid_server)
    end
  end


  def __unregister__(table, match, pos) do
    key = :erlang.element(pos, match)

    # We need to perform an element comparison if we have an special atom key.
    if is_atom(key) and reserved_atom?(Atom.to_string(key)) do
      match = :erlang.setelement(pos, match, :_)
      guard = {:"=:=", {:element, pos, :"$_"}, {:const, key}}
      :ets.select_delete(table, [{match, [guard], [true]}]) >= 0
    else
      :ets.match_delete(table, match)
    end
  end

  defp reserved_atom?("_"), do: true
  defp reserved_atom?("$" <> _), do: true
  defp reserved_atom?(_), do: false
end

defmodule Registry.Supervisor do

  use Supervisor

  def start_link(kind, registry, partitions, listeners, entries, compressed) do
    arg = {kind, registry, partitions, listeners, entries, compressed}
    Supervisor.start_link(__MODULE__, arg, name: registry)
  end

  def init({kind, registry, partitions, listeners, entries, compressed}) do
    ^registry = :ets.new(registry, [:set, :public, :named_table, read_concurrency: true])
    true = :ets.insert(registry, entries)

    children =
      for i <- 0..(partitions - 1) do
        key_partition = Registry.Partition.key_name(registry, i)
        pid_partition = Registry.Partition.pid_name(registry, i)
        arg = {kind, registry, i, partitions, key_partition, pid_partition, listeners, compressed}

        %{
          id: pid_partition,
          start: {Registry.Partition, :start_link, [pid_partition, arg]}
        }
      end

    Supervisor.init(children, strategy: strategy_for_kind(kind))
  end

  # Unique registries have their key partition hashed by key.
  # This means that, if a PID partition crashes, it may have
  # entries from all key partitions, so we need to crash all.
  defp strategy_for_kind(:unique), do: :one_for_all

  # Duplicate registries have both key and pid partitions hashed
  # by pid. This means that, if a PID partition crashes, all of
  # its associated entries are in its sibling table, so we crash one.
  defp strategy_for_kind(:duplicate), do: :one_for_one
end

defmodule Registry.Partition do


  # This process owns the equivalent key and pid ETS tables
  # and is responsible for linking to processes that map to
  # its own pid table.
  use GenServer
  @all_info -1
  @key_info -2





  def key_name(registry, partition) do
    Module.concat(registry, "KeyPartition" <> Integer.to_string(partition))
  end





  def pid_name(name, partition) do
    Module.concat(name, "PIDPartition" <> Integer.to_string(partition))
  end






  def start_link(registry, arg) do
    GenServer.start_link(__MODULE__, arg, name: registry)
  end




  def lock(pid, key, lock) do
    ref = GenServer.call(pid, {:lock, key})

    try do
      lock.()
    after
      send(pid, {:unlock, key, ref})
    end
  end

  ## Callbacks

  def init({kind, registry, i, partitions, key_partition, pid_partition, listeners, compressed}) do
    Process.flag(:trap_exit, true)
    key_ets = init_key_ets(kind, key_partition, compressed)
    pid_ets = init_pid_ets(kind, pid_partition)

    # If we have only one partition, we do an optimization which
    # is to write the table information alongside the registry info.
    if partitions == 1 do
      entries = [
        {@key_info, {kind, partitions, key_ets}},
        {@all_info, {kind, partitions, key_ets, {self(), pid_ets}, listeners}}
      ]

      true = :ets.insert(registry, entries)
    else
      true = :ets.insert(registry, {i, key_ets, {self(), pid_ets}})
    end

    {:ok, {pid_ets, %{}}}
  end

  # The key partition is a set for unique keys,
  # duplicate bag for duplicate ones.
  defp init_key_ets(:unique, key_partition, compressed) do
    opts = [:set, :public, read_concurrency: true, write_concurrency: true]
    :ets.new(key_partition, compression_opt(opts, compressed))
  end

  defp init_key_ets(:duplicate, key_partition, compressed) do
    opts = [:duplicate_bag, :public, read_concurrency: true, write_concurrency: true]
    :ets.new(key_partition, compression_opt(opts, compressed))
  end

  defp compression_opt(opts, compressed) do
    if compressed, do: [:compressed] ++ opts, else: opts
  end

  # A process can always have multiple keys, so the
  # pid partition is always a duplicate bag.
  defp init_pid_ets(_, pid_partition) do
    :ets.new(pid_partition, [
      :duplicate_bag,
      :public,
      read_concurrency: true,
      write_concurrency: true
    ])
  end

  def handle_call(:sync, _, state) do
    {:reply, :ok, state}
  end

  def handle_call({:lock, key}, from, {ets, lock}) do
    lock =
      case lock do
        %{^key => queue} ->
          Map.put(lock, key, :queue.in(from, queue))

        %{} ->
          go(from, key)
          Map.put(lock, key, :queue.new())
      end

    {:noreply, {ets, lock}}
  end

  def handle_info({:EXIT, pid, _reason}, {ets, lock}) do
    entries = :ets.take(ets, pid)

    for {_pid, key, key_ets, _counter} <- entries do
      key_ets =
        case key_ets do
          # In case the fake key_ets is being used. See unregister_match/2.
          {key_ets, _} ->
            key_ets

          _ ->
            key_ets
        end

      try do
        Registry.__unregister__(key_ets, {key, {pid, :_}}, 1)
      catch
        :error, :badarg -> :badarg
      end
    end

    {:noreply, {ets, lock}}
  end

  def handle_info({{:unlock, key}, _ref, :process, _pid, _reason}, state) do
    unlock(key, state)
  end

  def handle_info({:unlock, key, ref}, state) do
    Process.demonitor(ref, [:flush])
    unlock(key, state)
  end

  defp unlock(key, {ets, lock}) do
    %{^key => queue} = lock

    lock =
      case dequeue(queue, key) do
        :empty -> Map.delete(lock, key)
        {:not_empty, queue} -> Map.put(lock, key, queue)
      end

    {:noreply, {ets, lock}}
  end

  defp dequeue(queue, key) do
    case :queue.out(queue) do
      {:empty, _} ->
        :empty

      {{:value, {pid, _tag} = from}, queue} ->
        if node(pid) != node() or Process.alive?(pid) do
          go(from, key)
          {:not_empty, queue}
        else
          dequeue(queue, key)
        end
    end
  end

  defp go({pid, _tag} = from, key) do
    ref = Process.monitor(pid, tag: {:unlock, key})
    GenServer.reply(from, ref)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/registry.ex (docs and specs stripped;
# line numbers match the original).
