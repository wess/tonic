defmodule Map do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.










































































































































  defdelegate from_keys(keys, value), to: :maps













  defdelegate keys(map), to: :maps













  defdelegate values(map), to: :maps


















  defdelegate to_list(map), to: :maps











  def new, do: %{}















  def new(enumerable)
  def new(list) when is_list(list), do: :maps.from_list(list)
  def new(%_{} = struct), do: new_from_enum(struct)
  def new(%{} = map), do: map
  def new(enum), do: new_from_enum(enum)

  defp new_from_enum(enumerable) do
    enumerable
    |> Enum.to_list()
    |> :maps.from_list()
  end
















  def new(enumerable, transform)
  def new(%_{} = enumerable, transform), do: new_from_enum(enumerable, transform)
  def new(%{} = map, transform), do: new_from_map(map, transform)
  def new(enumerable, transform), do: new_from_enum(enumerable, transform)

  defp new_from_map(map, transform) when is_function(transform, 1) do
    iter = :maps.iterator(map)
    next = :maps.next(iter)
    :maps.from_list(do_map(next, transform))
  end

  defp do_map(:none, _fun), do: []

  defp do_map({key, value, iter}, transform) do
    [transform.({key, value}) | do_map(:maps.next(iter), transform)]
  end

  defp new_from_enum(enumerable, transform) when is_function(transform, 1) do
    enumerable
    |> Enum.map(transform)
    |> :maps.from_list()
  end















  def has_key?(map, key), do: :maps.is_key(key, map)


















  def fetch(map, key), do: :maps.find(key, map)

















  def fetch!(map, key) do
    :maps.get(key, map)
  end














  def put_new(map, key, value) do
    case map do
      %{^key => _value} ->
        map

      %{} ->
        put(map, key, value)

      other ->
        :erlang.error({:badmap, other})
    end
  end















  def replace(map, key, value) do
    case map do
      %{^key => _value} ->
        %{map | key => value}

      %{} ->
        map

      other ->
        :erlang.error({:badmap, other})
    end
  end



















  def replace!(map, key, value) do
    :maps.update(key, value, map)
  end




















  def replace_lazy(map, key, fun) when is_map(map) and is_function(fun, 1) do
    case map do
      %{^key => val} -> %{map | key => fun.(val)}
      %{} -> map
    end
  end























  def put_new_lazy(map, key, fun) when is_function(fun, 0) do
    case map do
      %{^key => _value} ->
        map

      %{} ->
        put(map, key, fun.())

      other ->
        :erlang.error({:badmap, other})
    end
  end














  def take(map, keys)

  def take(map, keys) when is_map(map) and is_list(keys) do
    take(keys, map, _acc = [])
  end

  def take(map, keys) when is_map(map) do
    IO.warn(
      "Map.take/2 with an Enumerable of keys that is not a list is deprecated. " <>
        " Use a list of keys instead."
    )

    take(map, Enum.to_list(keys))
  end

  def take(non_map, _keys) do
    :erlang.error({:badmap, non_map})
  end

  defp take([], _map, acc) do
    :maps.from_list(acc)
  end

  defp take([key | rest], map, acc) do
    acc =
      case map do
        %{^key => value} -> [{key, value} | acc]
        %{} -> acc
      end

    take(rest, map, acc)
  end
























  def get(map, key, default \\ nil) do
    case map do
      %{^key => value} ->
        value

      %{} ->
        default

      other ->
        :erlang.error({:badmap, other}, [map, key, default])
    end
  end
























  def get_lazy(map, key, fun) when is_function(fun, 0) do
    case map do
      %{^key => value} ->
        value

      %{} ->
        fun.()

      other ->
        :erlang.error({:badmap, other}, [map, key, fun])
    end
  end















  def put(map, key, value) do
    :maps.put(key, value, map)
  end

















  def delete(map, key), do: :maps.remove(key, map)





















  defdelegate merge(map1, map2), to: :maps



















  def merge(map1, map2, fun) when is_function(fun, 3) do
    :maps.merge_with(fun, map1, map2)
  end



















  def update(map, key, default, fun) when is_function(fun, 1) do
    case map do
      %{^key => value} ->
        %{map | key => fun.(value)}

      %{} ->
        put(map, key, default)

      other ->
        :erlang.error({:badmap, other}, [map, key, default, fun])
    end
  end



















  def pop(map, key, default \\ nil) do
    case :maps.take(key, map) do
      {_, _} = tuple -> tuple
      :error -> {default, map}
    end
  end



















  def pop!(map, key) do
    case :maps.take(key, map) do
      {_, _} = tuple -> tuple
      :error -> raise KeyError, key: key, term: map
    end
  end


























  def pop_lazy(map, key, fun) when is_function(fun, 0) do
    case :maps.take(key, map) do
      {_, _} = tuple -> tuple
      :error -> {fun.(), map}
    end
  end













  def drop(map, keys)

  def drop(map, keys) when is_map(map) and is_list(keys) do
    drop_keys(keys, map)
  end

  def drop(map, keys) when is_map(map) do
    IO.warn(
      "Map.drop/2 with an Enumerable of keys that is not a list is deprecated. " <>
        " Use a list of keys instead."
    )

    drop(map, Enum.to_list(keys))
  end

  def drop(non_map, keys) do
    :erlang.error({:badmap, non_map}, [non_map, keys])
  end

  defp drop_keys([], acc), do: acc

  defp drop_keys([key | rest], acc) do
    drop_keys(rest, delete(acc, key))
  end
















  def split(map, keys)

  def split(map, keys) when is_map(map) and is_list(keys) do
    split(keys, [], map)
  end

  def split(map, keys) when is_map(map) do
    IO.warn(
      "Map.split/2 with an Enumerable of keys that is not a list is deprecated. " <>
        " Use a list of keys instead."
    )

    split(map, Enum.to_list(keys))
  end

  def split(non_map, keys) do
    :erlang.error({:badmap, non_map}, [non_map, keys])
  end

  defp split([], included, excluded) do
    {:maps.from_list(included), excluded}
  end

  defp split([key | rest], included, excluded) do
    case excluded do
      %{^key => value} ->
        split(rest, [{key, value} | included], delete(excluded, key))

      _other ->
        split(rest, included, excluded)
    end
  end


























  def split_with(map, fun) when is_map(map) and is_function(fun, 1) do
    iter = :maps.iterator(map)
    next = :maps.next(iter)

    do_split_with(next, [], [], fun)
  end

  defp do_split_with(:none, while_true, while_false, _fun) do
    {:maps.from_list(while_true), :maps.from_list(while_false)}
  end

  defp do_split_with({key, value, iter}, while_true, while_false, fun) do
    if fun.({key, value}) do
      do_split_with(:maps.next(iter), [{key, value} | while_true], while_false, fun)
    else
      do_split_with(:maps.next(iter), while_true, [{key, value} | while_false], fun)
    end
  end


















  def update!(map, key, fun) when is_function(fun, 1) do
    value = fetch!(map, key)
    %{map | key => fun.(value)}
  end




































  def get_and_update(map, key, fun) when is_function(fun, 1) do
    current = get(map, key)

    case fun.(current) do
      {get, update} ->
        {get, put(map, key, update)}

      :pop ->
        {current, delete(map, key)}

      other ->
        raise "the given function must return a two-element tuple or :pop, got: #{inspect(other)}"
    end
  end




























  def get_and_update!(map, key, fun) when is_function(fun, 1) do
    value = fetch!(map, key)

    case fun.(value) do
      {get, update} ->
        {get, %{map | key => update}}

      :pop ->
        {value, delete(map, key)}

      other ->
        raise "the given function must return a two-element tuple or :pop, got: #{inspect(other)}"
    end
  end


















  def from_struct(struct) when is_atom(struct) do
    IO.warn("Map.from_struct/1 with a module is deprecated, please pass a struct instead")
    delete(struct.__struct__(), :__struct__)
  end

  def from_struct(%_{} = struct) do
    delete(struct, :__struct__)
  end



























  def equal?(map1, map2)

  def equal?(%{} = map1, %{} = map2), do: map1 === map2
  def equal?(%{} = map1, map2), do: :erlang.error({:badmap, map2}, [map1, map2])
  def equal?(term, other), do: :erlang.error({:badmap, term}, [term, other])



  def size(map) do
    map_size(map)
  end


























  def filter(map, fun) when is_map(map) and is_function(fun, 1) do
    iter = :maps.iterator(map)
    next = :maps.next(iter)
    :maps.from_list(do_filter(next, fun))
  end

  defp do_filter(:none, _fun), do: []

  defp do_filter({key, value, iter}, fun) do
    if fun.({key, value}) do
      [{key, value} | do_filter(:maps.next(iter), fun)]
    else
      do_filter(:maps.next(iter), fun)
    end
  end















  def reject(map, fun) when is_map(map) and is_function(fun, 1) do
    iter = :maps.iterator(map)
    next = :maps.next(iter)
    :maps.from_list(do_reject(next, fun))
  end

  defp do_reject(:none, _fun), do: []

  defp do_reject({key, value, iter}, fun) do
    if fun.({key, value}) do
      do_reject(:maps.next(iter), fun)
    else
      [{key, value} | do_reject(:maps.next(iter), fun)]
    end
  end
















  defdelegate intersect(map1, map2), to: :maps


















  def intersect(map1, map2, fun) when is_function(fun, 3) do
    :maps.intersect_with(fun, map1, map2)
  end



  def map(map, fun) when is_map(map) do
    :maps.map(fn k, v -> fun.({k, v}) end, map)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/map.ex (docs and specs stripped;
# line numbers match the original).
