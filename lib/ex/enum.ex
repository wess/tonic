defprotocol Enumerable do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.



































































































































































  def reduce(enumerable, acc, fun)











  def count(enumerable)















  def member?(enumerable, element)



































  def slice(enumerable)
end

defmodule Enum do
  import Kernel, except: [max: 2, min: 2]































































  require Stream.Reducers, as: R

  defmacrop skip(acc) do
    acc
  end

  defmacrop next(_, entry, acc) do
    quote(do: [unquote(entry) | unquote(acc)])
  end

  defmacrop acc(head, state, _) do
    quote(do: {unquote(head), unquote(state)})
  end

  defmacrop next_with_acc(_, entry, head, state, _) do
    quote do
      {[unquote(entry) | unquote(head)], unquote(state)}
    end
  end




















  def all?(enumerable) when is_list(enumerable) do
    all_list(enumerable)
  end

  def all?(enumerable) do
    Enumerable.reduce(enumerable, {:cont, true}, fn entry, _ ->
      if entry, do: {:cont, true}, else: {:halt, false}
    end)
    |> elem(1)
  end


























  def all?(enumerable, fun) when is_list(enumerable) do
    predicate_list(enumerable, true, fun)
  end

  def all?(first..last//step, fun) do
    predicate_range(first, last, step, true, fun)
  end

  def all?(enumerable, fun) do
    Enumerable.reduce(enumerable, {:cont, true}, fn entry, _ ->
      if fun.(entry), do: {:cont, true}, else: {:halt, false}
    end)
    |> elem(1)
  end




















  def any?(enumerable) when is_list(enumerable) do
    any_list(enumerable)
  end

  def any?(enumerable) do
    Enumerable.reduce(enumerable, {:cont, false}, fn entry, _ ->
      if entry, do: {:halt, true}, else: {:cont, false}
    end)
    |> elem(1)
  end





















  def any?(enumerable, fun) when is_list(enumerable) do
    predicate_list(enumerable, false, fun)
  end

  def any?(first..last//step, fun) do
    predicate_range(first, last, step, false, fun)
  end

  def any?(enumerable, fun) do
    Enumerable.reduce(enumerable, {:cont, false}, fn entry, _ ->
      if fun.(entry), do: {:halt, true}, else: {:cont, false}
    end)
    |> elem(1)
  end


























  def at(enumerable, index, default \\ nil) when is_integer(index) do
    case slice_forward(enumerable, index, 1, 1) do
      [value] -> value
      [] -> default
    end
  end



  def chunk(enumerable, count), do: chunk(enumerable, count, count, nil)



  def chunk(enum, n, step) do
    chunk_every(enum, n, step, :discard)
  end



  def chunk(enumerable, count, step, leftover) do
    chunk_every(enumerable, count, step, leftover || :discard)
  end






  def chunk_every(enumerable, count), do: chunk_every(enumerable, count, count, [])











































  def chunk_every(enumerable, count, step, leftover \\ [])
      when is_integer(count) and count > 0 and is_integer(step) and step > 0 do
    R.chunk_every(&chunk_while/4, enumerable, count, step, leftover)
  end



















































  def chunk_while(enumerable, acc, chunk_fun, after_fun) do
    {_, {res, acc}} =
      Enumerable.reduce(enumerable, {:cont, {[], acc}}, fn entry, {buffer, acc} ->
        case chunk_fun.(entry, acc) do
          {:cont, chunk, acc} -> {:cont, {[chunk | buffer], acc}}
          {:cont, acc} -> {:cont, {buffer, acc}}
          {:halt, acc} -> {:halt, {buffer, acc}}
        end
      end)

    case after_fun.(acc) do
      {:cont, _acc} -> :lists.reverse(res)
      {:cont, chunk, _acc} -> :lists.reverse([chunk | res])
    end
  end














  def chunk_by(enumerable, fun) do
    R.chunk_by(&chunk_while/4, enumerable, fun)
  end















  def concat(enumerables)

  def concat(list) when is_list(list) do
    concat_list(list)
  end

  def concat(enums) do
    concat_enum(enums)
  end


















  def concat(left, right) when is_list(left) and is_list(right) do
    left ++ right
  end

  def concat(left, right) do
    concat_enum([left, right])
  end











  def count(enumerable) when is_list(enumerable) do
    length(enumerable)
  end

  def count(enumerable) do
    case Enumerable.count(enumerable) do
      {:ok, value} when is_integer(value) ->
        value

      {:error, module} ->
        enumerable |> module.reduce({:cont, 0}, fn _, acc -> {:cont, acc + 1} end) |> elem(1)
    end
  end












  def count(enumerable, fun) do
    reduce(enumerable, 0, fn entry, acc ->
      if(fun.(entry), do: acc + 1, else: acc)
    end)
  end































  def count_until(enumerable, limit) when is_integer(limit) and limit > 0 do
    stop_at = limit - 1

    case Enumerable.count(enumerable) do
      {:ok, value} ->
        Kernel.min(value, limit)

      {:error, module} ->
        enumerable
        |> module.reduce(
          {:cont, 0},
          fn
            _, ^stop_at ->
              {:halt, limit}

            _, acc ->
              {:cont, acc + 1}
          end
        )
        |> elem(1)
    end
  end















  def count_until(enumerable, fun, limit) when is_integer(limit) and limit > 0 do
    stop_at = limit - 1

    Enumerable.reduce(enumerable, {:cont, 0}, fn
      entry, ^stop_at ->
        if fun.(entry) do
          {:halt, limit}
        else
          {:cont, stop_at}
        end

      entry, acc ->
        if fun.(entry) do
          {:cont, acc + 1}
        else
          {:cont, acc}
        end
    end)
    |> elem(1)
  end




















  def dedup(enumerable) when is_list(enumerable) do
    dedup_list(enumerable, []) |> :lists.reverse()
  end

  def dedup(enumerable) do
    reduce(enumerable, [], fn x, acc ->
      case acc do
        [^x | _] -> acc
        _ -> [x | acc]
      end
    end)
    |> :lists.reverse()
  end


















  def dedup_by(enumerable, fun) do
    {list, _} = reduce(enumerable, {[], []}, R.dedup(fun))
    :lists.reverse(list)
  end
























  def drop(enumerable, amount)
      when is_list(enumerable) and is_integer(amount) and amount >= 0 do
    drop_list(enumerable, amount)
  end

  def drop(enumerable, 0) do
    to_list(enumerable)
  end

  def drop(enumerable, amount) when is_integer(amount) and amount > 0 do
    {result, _} = reduce(enumerable, {[], amount}, R.drop())
    if is_list(result), do: :lists.reverse(result), else: []
  end

  def drop(enumerable, amount) when is_integer(amount) and amount < 0 do
    {count, fun} = slice_count_and_fun(enumerable, 1)
    amount = Kernel.min(amount + count, count)

    if amount > 0 do
      fun.(0, amount, 1)
    else
      []
    end
  end























  def drop_every(enumerable, nth)

  def drop_every(_enumerable, 1), do: []
  def drop_every(enumerable, 0), do: to_list(enumerable)
  def drop_every([], nth) when is_integer(nth), do: []

  def drop_every(enumerable, nth) when is_integer(nth) and nth > 1 do
    {res, _} = reduce(enumerable, {[], :first}, R.drop_every(nth))
    :lists.reverse(res)
  end












  def drop_while(enumerable, fun) when is_list(enumerable) do
    drop_while_list(enumerable, fun)
  end

  def drop_while(enumerable, fun) do
    {res, _} = reduce(enumerable, {[], true}, R.drop_while(fun))
    :lists.reverse(res)
  end















  def each(enumerable, fun) when is_list(enumerable) do
    :lists.foreach(fun, enumerable)
  end

  def each(enumerable, fun) do
    reduce(enumerable, nil, fn entry, _ ->
      fun.(entry)
      nil
    end)

    :ok
  end
















  def empty?(enumerable) when is_list(enumerable) do
    enumerable == []
  end

  def empty?(enumerable) do
    case Enumerable.slice(enumerable) do
      {:ok, value, _} ->
        value == 0

      {:error, module} ->
        enumerable
        |> module.reduce({:cont, true}, fn _, _ -> {:halt, false} end)
        |> elem(1)
    end
  end


























  def fetch(enumerable, index) when is_integer(index) do
    case slice_forward(enumerable, index, 1, 1) do
      [value] -> {:ok, value}
      [] -> :error
    end
  end




















  def fetch!(enumerable, index) when is_integer(index) do
    case slice_forward(enumerable, index, 1, 1) do
      [value] -> value
      [] -> raise Enum.OutOfBoundsError
    end
  end




































  def filter(enumerable, fun) when is_list(enumerable) do
    filter_list(enumerable, fun)
  end

  def filter(enumerable, fun) do
    reduce(enumerable, [], R.filter(fun)) |> :lists.reverse()
  end



  def filter_map(enumerable, filter, mapper) when is_list(enumerable) do
    for element <- enumerable, filter.(element), do: mapper.(element)
  end

  def filter_map(enumerable, filter, mapper) do
    enumerable
    |> reduce([], R.filter_map(filter, mapper))
    |> :lists.reverse()
  end

















  def find(enumerable, default \\ nil, fun)

  def find(enumerable, default, fun) when is_list(enumerable) do
    find_list(enumerable, default, fun)
  end

  def find(enumerable, default, fun) do
    Enumerable.reduce(enumerable, {:cont, default}, fn entry, default ->
      if fun.(entry), do: {:halt, entry}, else: {:cont, default}
    end)
    |> elem(1)
  end















  def find_index(enumerable, fun) when is_list(enumerable) do
    find_index_list(enumerable, 0, fun)
  end

  def find_index(enumerable, fun) do
    result =
      Enumerable.reduce(enumerable, {:cont, {:not_found, 0}}, fn entry, {_, index} ->
        if fun.(entry), do: {:halt, {:found, index}}, else: {:cont, {:not_found, index + 1}}
      end)

    case elem(result, 1) do
      {:found, index} -> index
      {:not_found, _} -> nil
    end
  end



























  def find_value(enumerable, default \\ nil, fun)

  def find_value(enumerable, default, fun) when is_list(enumerable) do
    find_value_list(enumerable, default, fun)
  end

  def find_value(enumerable, default, fun) do
    Enumerable.reduce(enumerable, {:cont, default}, fn entry, default ->
      fun_entry = fun.(entry)
      if fun_entry, do: {:halt, fun_entry}, else: {:cont, default}
    end)
    |> elem(1)
  end





























  def flat_map(enumerable, fun) when is_list(enumerable) do
    flat_map_list(enumerable, fun)
  end

  def flat_map(enumerable, fun) do
    reduce(enumerable, [], fn entry, acc ->
      case fun.(entry) do
        [] -> acc
        list when is_list(list) -> [list | acc]
        other -> [to_list(other) | acc]
      end
    end)
    |> flat_reverse([])
  end

  # the first clause is an optimization
  defp flat_reverse([[elem] | t], acc), do: flat_reverse(t, [elem | acc])
  defp flat_reverse([h | t], acc), do: flat_reverse(t, h ++ acc)
  defp flat_reverse([], acc), do: acc
























  def flat_map_reduce(enumerable, acc, fun) do
    {_, {list, acc}} =
      Enumerable.reduce(enumerable, {:cont, {[], acc}}, fn entry, {list, acc} ->
        case fun.(entry, acc) do
          {:halt, acc} ->
            {:halt, {list, acc}}

          {[], acc} ->
            {:cont, {list, acc}}

          {[entry], acc} ->
            {:cont, {[entry | list], acc}}

          {entries, acc} ->
            {:cont, {reduce(entries, list, &[&1 | &2]), acc}}
        end
      end)

    {:lists.reverse(list), acc}
  end













  def frequencies(enumerable) do
    reduce(enumerable, %{}, fn key, acc ->
      case acc do
        %{^key => value} -> %{acc | key => value + 1}
        %{} -> Map.put(acc, key, 1)
      end
    end)
  end
















  def frequencies_by(enumerable, key_fun) when is_function(key_fun) do
    reduce(enumerable, %{}, fn entry, acc ->
      key = key_fun.(entry)

      case acc do
        %{^key => value} -> %{acc | key => value + 1}
        %{} -> Map.put(acc, key, 1)
      end
    end)
  end













































  def group_by(enumerable, key_fun, value_fun \\ fn x -> x end)

  def group_by(enumerable, key_fun, value_fun) when is_function(key_fun) do
    reduce(reverse(enumerable), %{}, fn entry, acc ->
      key = key_fun.(entry)
      value = value_fun.(entry)

      case acc do
        %{^key => existing} -> %{acc | key => [value | existing]}
        %{} -> Map.put(acc, key, [value])
      end
    end)
  end

  def group_by(enumerable, dict, fun) do
    IO.warn(
      "Enum.group_by/3 with a map/dictionary as second element is deprecated. " <>
        "A map is used by default and it is no longer required to pass one to this function"
    )

    # Avoid warnings about Dict
    dict_module = String.to_atom("Dict")

    reduce(reverse(enumerable), dict, fn entry, categories ->
      dict_module.update(categories, fun.(entry), [entry], &[entry | &1])
    end)
  end

















  def intersperse(enumerable, separator) when is_list(enumerable) do
    case enumerable do
      [] -> []
      list -> intersperse_non_empty_list(list, separator)
    end
  end

  def intersperse(enumerable, separator) do
    list =
      enumerable
      |> reduce([], fn x, acc -> [x, separator | acc] end)
      |> :lists.reverse()

    # Head is a superfluous separator
    case list do
      [] -> []
      [_ | t] -> t
    end
  end




























  def into(enumerable, collectable)

  def into(enumerable, []) do
    to_list(enumerable)
  end

  def into(%_{} = enumerable, collectable) do
    into_protocol(enumerable, collectable)
  end

  def into(enumerable, %_{} = collectable) do
    into_protocol(enumerable, collectable)
  end

  def into(enumerable, %{} = collectable) do
    if map_size(collectable) == 0 do
      into_map(enumerable)
    else
      into_map(enumerable, collectable)
    end
  end

  def into(enumerable, collectable) do
    into_protocol(enumerable, collectable)
  end

  defp into_map(%{} = enumerable), do: enumerable
  defp into_map(enumerable) when is_list(enumerable), do: :maps.from_list(enumerable)
  defp into_map(enumerable), do: enumerable |> Enum.to_list() |> :maps.from_list()

  defp into_map(%{} = enumerable, collectable), do: Map.merge(collectable, enumerable)

  defp into_map(enumerable, collectable) when is_list(enumerable),
    do: Map.merge(collectable, :maps.from_list(enumerable))

  defp into_map(enumerable, collectable),
    do: reduce(enumerable, collectable, fn {key, val}, acc -> Map.put(acc, key, val) end)

  defp into_protocol(enumerable, collectable) do
    {initial, fun} = Collectable.into(collectable)

    try do
      reduce_into_protocol(enumerable, initial, fun)
    catch
      kind, reason ->
        fun.(initial, :halt)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      acc -> fun.(acc, :done)
    end
  end

  defp reduce_into_protocol(enumerable, initial, fun) when is_list(enumerable) do
    :lists.foldl(fn x, acc -> fun.(acc, {:cont, x}) end, initial, enumerable)
  end

  defp reduce_into_protocol(enumerable, initial, fun) do
    enumerable
    |> Enumerable.reduce({:cont, initial}, fn x, acc ->
      {:cont, fun.(acc, {:cont, x})}
    end)
    |> elem(1)
  end















  def into(enumerable, [], transform) do
    map(enumerable, transform)
  end

  def into(%_{} = enumerable, collectable, transform) do
    into_protocol(enumerable, collectable, transform)
  end

  def into(enumerable, %_{} = collectable, transform) do
    into_protocol(enumerable, collectable, transform)
  end

  def into(enumerable, %{} = collectable, transform) do
    if map_size(collectable) == 0 do
      enumerable |> map(transform) |> :maps.from_list()
    else
      reduce(enumerable, collectable, fn entry, acc ->
        {key, val} = transform.(entry)
        Map.put(acc, key, val)
      end)
    end
  end

  def into(enumerable, collectable, transform) do
    into_protocol(enumerable, collectable, transform)
  end

  defp into_protocol(enumerable, collectable, transform) do
    {initial, fun} = Collectable.into(collectable)

    try do
      reduce_into_protocol(enumerable, initial, transform, fun)
    catch
      kind, reason ->
        fun.(initial, :halt)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      acc -> fun.(acc, :done)
    end
  end

  defp reduce_into_protocol(enumerable, initial, transform, fun) when is_list(enumerable) do
    :lists.foldl(fn x, acc -> fun.(acc, {:cont, transform.(x)}) end, initial, enumerable)
  end

  defp reduce_into_protocol(enumerable, initial, transform, fun) do
    enumerable
    |> Enumerable.reduce({:cont, initial}, fn x, acc ->
      {:cont, fun.(acc, {:cont, transform.(x)})}
    end)
    |> elem(1)
  end























  def join(enumerable, joiner \\ "")

  def join(enumerable, "") do
    enumerable
    |> map(&entry_to_string(&1))
    |> IO.iodata_to_binary()
  end

  def join(enumerable, joiner) when is_list(enumerable) and is_binary(joiner) do
    join_list(enumerable, joiner)
  end

  def join(enumerable, joiner) when is_binary(joiner) do
    reduced =
      reduce(enumerable, :first, fn
        entry, :first -> entry_to_string(entry)
        entry, acc -> [acc, joiner | entry_to_string(entry)]
      end)

    if reduced == :first do
      ""
    else
      IO.iodata_to_binary(reduced)
    end
  end

















  def map(enumerable, fun)

  def map(enumerable, fun) when is_list(enumerable) do
    :lists.map(fun, enumerable)
  end

  def map(first..last//step, fun) do
    map_range(first, last, step, fun)
  end

  def map(enumerable, fun) do
    reduce(enumerable, [], R.map(fun)) |> :lists.reverse()
  end






























  def map_every(enumerable, nth, fun)

  def map_every(enumerable, 1, fun), do: map(enumerable, fun)
  def map_every(enumerable, 0, _fun), do: to_list(enumerable)
  def map_every([], nth, _fun) when is_integer(nth) and nth > 1, do: []

  def map_every(enumerable, nth, fun) when is_integer(nth) and nth > 1 do
    {res, _} = reduce(enumerable, {[], :first}, R.map_every(nth, fun))
    :lists.reverse(res)
  end











  def map_intersperse(enumerable, separator, mapper)

  def map_intersperse(enumerable, separator, mapper) when is_list(enumerable) do
    map_intersperse_list(enumerable, separator, mapper)
  end

  def map_intersperse(enumerable, separator, mapper) do
    reduced =
      reduce(enumerable, :first, fn
        entry, :first -> [mapper.(entry)]
        entry, acc -> [mapper.(entry), separator | acc]
      end)

    if reduced == :first do
      []
    else
      :lists.reverse(reduced)
    end
  end



















  def map_join(enumerable, joiner \\ "", mapper) when is_binary(joiner) do
    enumerable
    |> map_intersperse(joiner, &entry_to_string(mapper.(&1)))
    |> IO.iodata_to_binary()
  end





















  def map_reduce(enumerable, acc, fun) when is_list(enumerable) do
    :lists.mapfoldl(fun, acc, enumerable)
  end

  def map_reduce(enumerable, acc, fun) do
    {list, acc} =
      reduce(enumerable, {[], acc}, fn entry, {list, acc} ->
        {new_entry, acc} = fun.(entry, acc)
        {[new_entry | list], acc}
      end)

    {:lists.reverse(list), acc}
  end


  def max(list = [_ | _]), do: :lists.max(list)


  def max(list = [_ | _], empty_fallback) when is_function(empty_fallback, 0) do
    :lists.max(list)
  end



  def max(enumerable, empty_fallback) when is_function(empty_fallback, 0) do
    max(enumerable, &>=/2, empty_fallback)
  end

















































  def max(enumerable, sorter \\ &>=/2, empty_fallback \\ fn -> raise Enum.EmptyError end) do
    aggregate(enumerable, max_sort_fun(sorter), empty_fallback)
  end

  defp max_sort_fun(sorter) when is_function(sorter, 2), do: sorter
  defp max_sort_fun(module) when is_atom(module), do: &(module.compare(&1, &2) != :lt)








  def max_by(enumerable, fun, empty_fallback)
      when is_function(fun, 1) and is_function(empty_fallback, 0) do
    max_by(enumerable, fun, &>=/2, empty_fallback)
  end




















































  def max_by(enumerable, fun, sorter \\ &>=/2, empty_fallback \\ fn -> raise Enum.EmptyError end)
      when is_function(fun, 1) do
    aggregate_by(enumerable, fun, max_sort_fun(sorter), empty_fallback)
  end


























  def member?(enumerable, element) when is_list(enumerable) do
    :lists.member(element, enumerable)
  end

  def member?(enumerable, element) do
    case Enumerable.member?(enumerable, element) do
      {:ok, element} when is_boolean(element) ->
        element

      {:error, module} ->
        module.reduce(enumerable, {:cont, false}, fn
          v, _ when v === element -> {:halt, true}
          _, _ -> {:cont, false}
        end)
        |> elem(1)
    end
  end


  def min(list = [_ | _]), do: :lists.min(list)


  def min(list = [_ | _], empty_fallback) when is_function(empty_fallback, 0) do
    :lists.min(list)
  end



  def min(enumerable, empty_fallback) when is_function(empty_fallback, 0) do
    min(enumerable, &<=/2, empty_fallback)
  end

















































  def min(enumerable, sorter \\ &<=/2, empty_fallback \\ fn -> raise Enum.EmptyError end) do
    aggregate(enumerable, min_sort_fun(sorter), empty_fallback)
  end

  defp min_sort_fun(sorter) when is_function(sorter, 2), do: sorter
  defp min_sort_fun(module) when is_atom(module), do: &(module.compare(&1, &2) != :gt)








  def min_by(enumerable, fun, empty_fallback)
      when is_function(fun, 1) and is_function(empty_fallback, 0) do
    min_by(enumerable, fun, &<=/2, empty_fallback)
  end




















































  def min_by(enumerable, fun, sorter \\ &<=/2, empty_fallback \\ fn -> raise Enum.EmptyError end)
      when is_function(fun, 1) do
    aggregate_by(enumerable, fun, min_sort_fun(sorter), empty_fallback)
  end






















  def min_max(enumerable, empty_fallback \\ fn -> raise Enum.EmptyError end)

  def min_max(first..last//step = range, empty_fallback) when is_function(empty_fallback, 0) do
    case Range.size(range) do
      0 ->
        empty_fallback.()

      _ ->
        last = last - rem(last - first, step)
        {Kernel.min(first, last), Kernel.max(first, last)}
    end
  end

  def min_max(enumerable, empty_fallback) when is_function(empty_fallback, 0) do
    first_fun = &[&1 | &1]

    reduce_fun = fn entry, [min | max] ->
      [Kernel.min(min, entry) | Kernel.max(max, entry)]
    end

    case reduce_by(enumerable, first_fun, reduce_fun) do
      :empty -> empty_fallback.()
      [min | max] -> {min, max}
    end
  end




  def min_max_by(enumerable, fun, empty_fallback)
      when is_function(fun, 1) and is_function(empty_fallback, 0) do
    min_max_by(enumerable, fun, &</2, empty_fallback)
  end























































  def min_max_by(
        enumerable,
        fun,
        sorter_or_empty_fallback \\ &</2,
        empty_fallback \\ fn -> raise Enum.EmptyError end
      )

  def min_max_by(enumerable, fun, sorter, empty_fallback)
      when is_function(fun, 1) and is_atom(sorter) and is_function(empty_fallback, 0) do
    min_max_by(enumerable, fun, min_max_by_sort_fun(sorter), empty_fallback)
  end

  def min_max_by(enumerable, fun, sorter, empty_fallback)
      when is_function(fun, 1) and is_function(sorter, 2) and is_function(empty_fallback, 0) do
    first_fun = fn entry ->
      fun_entry = fun.(entry)
      {entry, entry, fun_entry, fun_entry}
    end

    reduce_fun = fn entry, {prev_min, prev_max, fun_min, fun_max} = acc ->
      fun_entry = fun.(entry)

      cond do
        sorter.(fun_entry, fun_min) ->
          {entry, prev_max, fun_entry, fun_max}

        sorter.(fun_max, fun_entry) ->
          {prev_min, entry, fun_min, fun_entry}

        true ->
          acc
      end
    end

    case reduce_by(enumerable, first_fun, reduce_fun) do
      :empty -> empty_fallback.()
      {min, max, _, _} -> {min, max}
    end
  end

  defp min_max_by_sort_fun(module) when is_atom(module), do: &(module.compare(&1, &2) == :lt)































  def split_with(enumerable, fun) do
    {acc1, acc2} =
      reduce(enumerable, {[], []}, fn entry, {acc1, acc2} ->
        if fun.(entry) do
          {[entry | acc1], acc2}
        else
          {acc1, [entry | acc2]}
        end
      end)

    {:lists.reverse(acc1), :lists.reverse(acc2)}
  end



  def partition(enumerable, fun) do
    split_with(enumerable, fun)
  end




































  def random(enumerable)

  def random(enumerable) when is_list(enumerable) do
    case length(enumerable) do
      0 -> raise Enum.EmptyError
      length -> enumerable |> drop_list(random_count(length)) |> hd()
    end
  end

  def random(first.._//step = range) do
    case Range.size(range) do
      0 -> raise Enum.EmptyError
      size -> first + random_count(size) * step
    end
  end

  def random(enumerable) do
    result =
      case Enumerable.slice(enumerable) do
        {:ok, 0, _} ->
          []

        {:ok, count, fun} when is_function(fun, 1) ->
          slice_list(fun.(enumerable), random_count(count), 1, 1)

        {:ok, count, fun} when is_function(fun, 3) ->
          fun.(random_count(count), 1, 1)

        # TODO: Remove deprecation on Elixir v1.20.
        {:ok, count, fun} when is_function(fun, 2) ->
          IO.warn(
            "#{inspect(Enumerable.impl_for(enumerable))} must return a three arity function on slice/1"
          )

          fun.(random_count(count), 1)

        {:error, _} ->
          take_random(enumerable, 1)
      end

    case result do
      [] -> raise Enum.EmptyError
      [elem] -> elem
    end
  end

  defp random_count(count) do
    :rand.uniform(count) - 1
  end




























  def reduce(enumerable, fun)

  def reduce([h | t], fun) do
    reduce(t, h, fun)
  end

  def reduce([], _fun) do
    raise Enum.EmptyError
  end

  def reduce(enumerable, fun) do
    Enumerable.reduce(enumerable, {:cont, :first}, fn
      x, {:acc, acc} -> {:cont, {:acc, fun.(x, acc)}}
      x, :first -> {:cont, {:acc, x}}
    end)
    |> elem(1)
    |> case do
      :first -> raise Enum.EmptyError
      {:acc, acc} -> acc
    end
  end










































  def reduce(enumerable, acc, fun) when is_list(enumerable) do
    :lists.foldl(fun, acc, enumerable)
  end

  def reduce(first..last//step, acc, fun) do
    reduce_range(first, last, step, acc, fun)
  end

  def reduce(%_{} = enumerable, acc, fun) do
    reduce_enumerable(enumerable, acc, fun)
  end

  def reduce(%{} = enumerable, acc, fun) do
    :maps.fold(fn k, v, acc -> fun.({k, v}, acc) end, acc, enumerable)
  end

  def reduce(enumerable, acc, fun) do
    reduce_enumerable(enumerable, acc, fun)
  end



































  def reduce_while(enumerable, acc, fun) do
    Enumerable.reduce(enumerable, {:cont, acc}, fun) |> elem(1)
  end














  def reject(enumerable, fun) when is_list(enumerable) do
    reject_list(enumerable, fun)
  end

  def reject(enumerable, fun) do
    reduce(enumerable, [], R.reject(fun)) |> :lists.reverse()
  end











  def reverse(enumerable)

  def reverse([]), do: []
  def reverse([_] = list), do: list
  def reverse([element1, element2]), do: [element2, element1]
  def reverse([element1, element2 | rest]), do: :lists.reverse(rest, [element2, element1])
  def reverse(enumerable), do: reduce(enumerable, [], &[&1 | &2])















  def reverse(enumerable, tail) when is_list(enumerable) do
    :lists.reverse(enumerable, to_list(tail))
  end

  def reverse(enumerable, tail) do
    reduce(enumerable, to_list(tail), fn entry, acc ->
      [entry | acc]
    end)
  end















  def reverse_slice(enumerable, start_index, count)
      when is_integer(start_index) and start_index >= 0 and is_integer(count) and count >= 0 do
    list = reverse(enumerable)
    length = length(list)
    count = Kernel.min(count, length - start_index)

    if count > 0 do
      reverse_slice(list, length, start_index + count, count, [])
    else
      :lists.reverse(list)
    end
  end

















































  def slide(enumerable, range_or_single_index, insertion_index)

  def slide(enumerable, single_index, insertion_index) when is_integer(single_index) do
    slide(enumerable, single_index..single_index, insertion_index)
  end

  # This matches the behavior of Enum.slice/2
  def slide(_, _.._//step = index_range, _insertion_index) when step != 1 do
    raise ArgumentError,
          "Enum.slide/3 does not accept ranges with custom steps, got: #{inspect(index_range)}"
  end

  # Normalize negative input ranges like Enum.slice/2
  def slide(enumerable, first..last//_, insertion_index)
      when first < 0 or last < 0 or insertion_index < 0 do
    count = Enum.count(enumerable)
    normalized_first = if first >= 0, do: first, else: Kernel.max(first + count, 0)
    normalized_last = if last >= 0, do: last, else: last + count

    normalized_insertion_index =
      if insertion_index >= 0, do: insertion_index, else: insertion_index + count

    if normalized_first < count and normalized_first != normalized_insertion_index do
      normalized_range = normalized_first..normalized_last//1
      slide(enumerable, normalized_range, normalized_insertion_index)
    else
      Enum.to_list(enumerable)
    end
  end

  def slide(enumerable, insertion_index.._//_, insertion_index) do
    Enum.to_list(enumerable)
  end

  def slide(_, first..last//_, insertion_index)
      when insertion_index > first and insertion_index <= last do
    raise ArgumentError,
          "insertion index for slide must be outside the range being moved " <>
            "(tried to insert #{first}..#{last} at #{insertion_index})"
  end

  def slide(enumerable, first..last//_, _insertion_index) when first > last do
    Enum.to_list(enumerable)
  end

  # Guarantees at this point: step size == 1 and first <= last and (insertion_index < first or insertion_index > last)
  def slide(enumerable, first..last//_, insertion_index) do
    impl = if is_list(enumerable), do: &slide_list_start/4, else: &slide_any/4

    cond do
      insertion_index <= first -> impl.(enumerable, insertion_index, first, last)
      insertion_index > last -> impl.(enumerable, first, last + 1, insertion_index)
    end
  end

  # Takes the range from middle..last and moves it to be in front of index start
  defp slide_any(enumerable, start, middle, last) do
    # We're going to deal with 4 "chunks" of the enumerable:
    # 0. "Head," before the start index
    # 1. "Slide back," between start (inclusive) and middle (exclusive)
    # 2. "Slide front," between middle (inclusive) and last (inclusive)
    # 3. "Tail," after last
    #
    # But, we're going to accumulate these into only two lists: pre and post.
    # We'll reverse-accumulate the head into our pre list, then "slide back" into post,
    # then "slide front" into pre, then "tail" into post.
    #
    # Then at the end, we're going to reassemble and reverse them, and end up with the
    # chunks in the correct order.
    {_size, pre, post} =
      reduce(enumerable, {0, [], []}, fn item, {index, pre, post} ->
        {pre, post} =
          cond do
            index < start -> {[item | pre], post}
            index >= start and index < middle -> {pre, [item | post]}
            index >= middle and index <= last -> {[item | pre], post}
            true -> {pre, [item | post]}
          end

        {index + 1, pre, post}
      end)

    :lists.reverse(pre, :lists.reverse(post))
  end

  # Like slide_any/4 above, this optimized implementation of slide for lists depends
  # on the indices being sorted such that we're moving middle..last to be in front of start.
  defp slide_list_start([h | t], start, middle, last)
       when start > 0 and start <= middle and middle <= last do
    [h | slide_list_start(t, start - 1, middle - 1, last - 1)]
  end

  defp slide_list_start(list, 0, middle, last), do: slide_list_middle(list, middle, last, [])
  defp slide_list_start([], _start, _middle, _last), do: []

  defp slide_list_middle([h | t], middle, last, acc) when middle > 0 do
    slide_list_middle(t, middle - 1, last - 1, [h | acc])
  end

  defp slide_list_middle(list, 0, last, start_to_middle) do
    {slid_range, tail} = slide_list_last(list, last + 1, [])
    slid_range ++ :lists.reverse(start_to_middle, tail)
  end

  # You asked for a middle index off the end of the list... you get what we've got
  defp slide_list_middle([], _, _, acc) do
    :lists.reverse(acc)
  end

  defp slide_list_last([h | t], last, acc) when last > 0 do
    slide_list_last(t, last - 1, [h | acc])
  end

  defp slide_list_last(rest, 0, acc) do
    {:lists.reverse(acc), rest}
  end

  defp slide_list_last([], _, acc) do
    {:lists.reverse(acc), []}
  end














  def scan(enumerable, fun)

  def scan([], _fun), do: []

  def scan([elem | rest], fun) do
    scanned = scan_list(rest, elem, fun)
    [elem | scanned]
  end

  def scan(enumerable, fun) do
    {res, _} = reduce(enumerable, {[], :first}, R.scan2(fun))
    :lists.reverse(res)
  end













  def scan(enumerable, acc, fun) when is_list(enumerable) do
    scan_list(enumerable, acc, fun)
  end

  def scan(enumerable, acc, fun) do
    {res, _} = reduce(enumerable, {[], acc}, R.scan3(fun))
    :lists.reverse(res)
  end






















  def shuffle(enumerable) do
    randomized =
      reduce(enumerable, [], fn x, acc ->
        [{:rand.uniform(), x} | acc]
      end)

    shuffle_unwrap(:lists.keysort(1, randomized))
  end

  defp shuffle_unwrap([{_, h} | rest]), do: [h | shuffle_unwrap(rest)]
  defp shuffle_unwrap([]), do: []


































































  def slice(enumerable, first..last//step = index_range) do
    # TODO: Support negative steps as a reverse on Elixir v2.0.
    cond do
      step > 0 ->
        slice_range(enumerable, first, last, step)

      step == -1 and first > last ->
        IO.warn(
          "negative steps are not supported in Enum.slice/2, pass #{first}..#{last}//1 instead"
        )

        slice_range(enumerable, first, last, 1)

      true ->
        raise ArgumentError,
              "Enum.slice/2 does not accept ranges with negative steps, got: #{inspect(index_range)}"
    end
  end

  # TODO: Remove me on v2.0
  def slice(enumerable, %{__struct__: Range, first: first, last: last} = index_range) do
    step = if first <= last, do: 1, else: -1
    slice(enumerable, Map.put(index_range, :step, step))
  end

  defp slice_range(enumerable, first, -1, step) when first >= 0 do
    if step == 1 do
      drop(enumerable, first)
    else
      enumerable |> drop(first) |> take_every_list(step - 1)
    end
  end

  defp slice_range(enumerable, first, last, step)
       when last >= first and last >= 0 and first >= 0 do
    slice_forward(enumerable, first, last - first + 1, step)
  end

  defp slice_range(enumerable, first, last, step) do
    {count, fun} = slice_count_and_fun(enumerable, step)
    first = if first >= 0, do: first, else: Kernel.max(first + count, 0)
    last = if last >= 0, do: last, else: last + count
    amount = last - first + 1

    if first < count and amount > 0 do
      amount = Kernel.min(amount, count - first)
      amount = amount_with_step(amount, step)
      fun.(first, amount, step)
    else
      []
    end
  end

  defp amount_with_step(amount, 1), do: amount
  defp amount_with_step(amount, step), do: div(amount - 1, step) + 1







































  def slice(_enumerable, start_index, 0) when is_integer(start_index), do: []

  def slice(enumerable, start_index, amount)
      when is_integer(start_index) and start_index < 0 and is_integer(amount) and amount >= 0 do
    {count, fun} = slice_count_and_fun(enumerable, 1)
    start_index = Kernel.max(count + start_index, 0)
    amount = Kernel.min(amount, count - start_index)

    if amount > 0 do
      fun.(start_index, amount, 1)
    else
      []
    end
  end

  def slice(enumerable, start_index, amount)
      when is_integer(start_index) and is_integer(amount) and amount >= 0 do
    slice_forward(enumerable, start_index, amount, 1)
  end














  def sort(enumerable) when is_list(enumerable) do
    :lists.sort(enumerable)
  end

  def sort(enumerable) do
    sort(enumerable, &(&1 <= &2))
  end













































































  def sort(enumerable, sorter) when is_list(enumerable) do
    case sorter do
      :asc -> :lists.sort(enumerable)
      :desc -> :lists.sort(enumerable) |> :lists.reverse()
      _ -> :lists.sort(to_sort_fun(sorter), enumerable)
    end
  end

  def sort(enumerable, sorter) do
    fun = to_sort_fun(sorter)

    reduce(enumerable, [], &sort_reducer(&1, &2, fun))
    |> sort_terminator(fun)
  end

  defp to_sort_fun(sorter) when is_function(sorter, 2), do: sorter
  defp to_sort_fun(:asc), do: &<=/2
  defp to_sort_fun(:desc), do: &>=/2
  defp to_sort_fun(module) when is_atom(module), do: &(module.compare(&1, &2) != :gt)
  defp to_sort_fun({:asc, module}) when is_atom(module), do: &(module.compare(&1, &2) != :gt)
  defp to_sort_fun({:desc, module}) when is_atom(module), do: &(module.compare(&1, &2) != :lt)
















































































































  def sort_by(enumerable, mapper, sorter \\ :asc)

  def sort_by(enumerable, mapper, :desc) when is_function(mapper, 1) do
    enumerable
    |> reduce([], &[{&1, mapper.(&1)} | &2])
    |> List.keysort(1, :asc)
    |> List.foldl([], &[elem(&1, 0) | &2])
  end

  def sort_by(enumerable, mapper, sorter) when is_function(mapper, 1) do
    enumerable
    |> map(&{&1, mapper.(&1)})
    |> List.keysort(1, sorter)
    |> map(&elem(&1, 0))
  end































  def split(enumerable, count) when is_list(enumerable) and is_integer(count) and count >= 0 do
    split_list(enumerable, count, [])
  end

  def split(enumerable, count) when is_integer(count) and count >= 0 do
    {_, list1, list2} =
      reduce(enumerable, {count, [], []}, fn entry, {counter, acc1, acc2} ->
        if counter > 0 do
          {counter - 1, [entry | acc1], acc2}
        else
          {counter, acc1, [entry | acc2]}
        end
      end)

    {:lists.reverse(list1), :lists.reverse(list2)}
  end

  def split(enumerable, count) when is_integer(count) and count < 0 do
    split_reverse_list(reverse(enumerable), -count, [])
  end





















  def split_while(enumerable, fun) when is_list(enumerable) do
    split_while_list(enumerable, fun, [])
  end

  def split_while(enumerable, fun) do
    {list1, list2} =
      reduce(enumerable, {[], []}, fn
        entry, {acc1, []} ->
          if(fun.(entry), do: {[entry | acc1], []}, else: {acc1, [entry]})

        entry, {acc1, acc2} ->
          {acc1, [entry | acc2]}
      end)

    {:lists.reverse(list1), :lists.reverse(list2)}
  end





















  def sum(enumerable)

  def sum(first..last//step = range) do
    range
    |> Range.size()
    |> Kernel.*(first + last - rem(last - first, step))
    |> div(2)
  end

  def sum(enumerable) do
    reduce(enumerable, 0, &+/2)
  end

























  def sum_by(enumerable, mapper)

  def sum_by(list, mapper) when is_list(list) and is_function(mapper, 1) do
    sum_by_list(list, mapper, 0)
  end

  def sum_by(enumerable, mapper) when is_function(mapper, 1) do
    reduce(enumerable, 0, fn x, acc -> acc + mapper.(x) end)
  end




















  def product(enumerable) do
    reduce(enumerable, 1, &*/2)
  end

























  def product_by(enumerable, mapper)

  def product_by(list, mapper) when is_list(list) and is_function(mapper, 1) do
    product_by_list(list, mapper, 1)
  end

  def product_by(enumerable, mapper) when is_function(mapper, 1) do
    reduce(enumerable, 1, fn x, acc -> acc * mapper.(x) end)
  end





























  def take(enumerable, amount)

  def take(_enumerable, 0), do: []

  def take(enumerable, amount)
      when is_list(enumerable) and is_integer(amount) and amount > 0 do
    take_list(enumerable, amount)
  end

  def take(enumerable, amount) when is_integer(amount) and amount > 0 do
    {_, {res, _}} =
      Enumerable.reduce(enumerable, {:cont, {[], amount}}, fn entry, {list, n} ->
        case n do
          1 -> {:halt, {[entry | list], n - 1}}
          _ -> {:cont, {[entry | list], n - 1}}
        end
      end)

    :lists.reverse(res)
  end

  def take(enumerable, amount) when is_integer(amount) and amount < 0 do
    {count, fun} = slice_count_and_fun(enumerable, 1)
    first = Kernel.max(amount + count, 0)
    fun.(first, count - first, 1)
  end























  def take_every(enumerable, nth)

  def take_every(_enumerable, 0), do: []
  def take_every(enumerable, 1), do: to_list(enumerable)

  def take_every(list, nth) when is_list(list) and is_integer(nth) and nth > 1 do
    take_every_list(list, nth - 1)
  end

  def take_every(enumerable, nth) when is_integer(nth) and nth > 1 do
    {res, _} = reduce(enumerable, {[], :first}, R.take_every(nth))
    :lists.reverse(res)
  end




















  def take_random(enumerable, count)
  def take_random(_enumerable, 0), do: []
  def take_random([], _), do: []

  def take_random(enumerable, 1) do
    enumerable
    |> reduce({0, 0, 1.0, nil}, fn
      elem, {idx, idx, w, _current} ->
        {jdx, w} = take_jdx_w(idx, w, 1)
        {idx + 1, jdx, w, elem}

      _elem, {idx, jdx, w, current} ->
        {idx + 1, jdx, w, current}
    end)
    |> case do
      {0, 0, 1.0, nil} -> []
      {_idx, _jdx, _w, current} -> [current]
    end
  end

  def take_random(enumerable, count) when count in 0..128 do
    sample = Tuple.duplicate(nil, count)

    reducer = fn
      elem, {idx, jdx, w, sample} when idx < count ->
        rand = take_index(idx)
        sample = sample |> put_elem(idx, elem(sample, rand)) |> put_elem(rand, elem)

        if idx == jdx do
          {jdx, w} = take_jdx_w(idx, w, count)
          {idx + 1, jdx, w, sample}
        else
          {idx + 1, jdx, w, sample}
        end

      elem, {idx, idx, w, sample} ->
        pos = :rand.uniform(count) - 1
        {jdx, w} = take_jdx_w(idx, w, count)
        {idx + 1, jdx, w, put_elem(sample, pos, elem)}

      _elem, {idx, jdx, w, sample} ->
        {idx + 1, jdx, w, sample}
    end

    {size, _, _, sample} = reduce(enumerable, {0, count - 1, 1.0, sample}, reducer)

    if count < size do
      Tuple.to_list(sample)
    else
      take_tupled(sample, size, [])
    end
  end

  def take_random(enumerable, count) when is_integer(count) and count >= 0 do
    reducer = fn
      elem, {idx, jdx, w, sample} when idx < count ->
        rand = take_index(idx)
        sample = sample |> Map.put(idx, Map.get(sample, rand)) |> Map.put(rand, elem)

        if idx == jdx do
          {jdx, w} = take_jdx_w(idx, w, count)
          {idx + 1, jdx, w, sample}
        else
          {idx + 1, jdx, w, sample}
        end

      elem, {idx, idx, w, sample} ->
        pos = :rand.uniform(count) - 1
        {jdx, w} = take_jdx_w(idx, w, count)
        {idx + 1, jdx, w, %{sample | pos => elem}}

      _elem, {idx, jdx, w, sample} ->
        {idx + 1, jdx, w, sample}
    end

    {size, _, _, sample} = reduce(enumerable, {0, count - 1, 1.0, %{}}, reducer)
    take_mapped(sample, Kernel.min(count, size), [])
  end


  defp take_jdx_w(idx, w, count) do
    w = w * :math.exp(:math.log(:rand.uniform()) / count)
    jdx = idx + floor(:math.log(:rand.uniform()) / :math.log(1 - w)) + 1
    {jdx, w}
  end

  defp take_index(0), do: 0
  defp take_index(idx), do: :rand.uniform(idx + 1) - 1

  defp take_tupled(_sample, 0, acc), do: acc

  defp take_tupled(sample, position, acc) do
    position = position - 1
    take_tupled(sample, position, [elem(sample, position) | acc])
  end

  defp take_mapped(_sample, 0, acc), do: acc

  defp take_mapped(sample, position, acc) do
    position = position - 1
    take_mapped(sample, position, [Map.fetch!(sample, position) | acc])
  end












  def take_while(enumerable, fun) when is_list(enumerable) do
    take_while_list(enumerable, fun)
  end

  def take_while(enumerable, fun) do
    {_, res} =
      Enumerable.reduce(enumerable, {:cont, []}, fn entry, acc ->
        if fun.(entry) do
          {:cont, [entry | acc]}
        else
          {:halt, acc}
        end
      end)

    :lists.reverse(res)
  end











  def to_list(enumerable) when is_list(enumerable), do: enumerable
  def to_list(%{__struct__: Range} = range), do: Range.to_list(range)
  def to_list(%_{} = enumerable), do: reverse(enumerable) |> :lists.reverse()
  def to_list(%{} = enumerable), do: Map.to_list(enumerable)
  def to_list(enumerable), do: reverse(enumerable) |> :lists.reverse()














  def uniq(enumerable) do
    uniq_by(enumerable, fn x -> x end)
  end



  def uniq(enumerable, fun) do
    uniq_by(enumerable, fun)
  end























  def uniq_by(enumerable, fun) when is_list(enumerable) do
    uniq_list(enumerable, %{}, fun)
  end

  def uniq_by(enumerable, fun) do
    {list, _} = reduce(enumerable, {[], %{}}, R.uniq_by(fun))
    :lists.reverse(list)
  end




















  def unzip([_ | _] = list) do
    :lists.reverse(list) |> unzip([], [])
  end

  def unzip([]) do
    {[], []}
  end

  def unzip(enumerable) do
    {list1, list2} =
      reduce(enumerable, {[], []}, fn {el1, el2}, {list1, list2} ->
        {[el1 | list1], [el2 | list2]}
      end)

    {:lists.reverse(list1), :lists.reverse(list2)}
  end

  defp unzip([{el1, el2} | reversed_list], list1, list2) do
    unzip(reversed_list, [el1 | list1], [el2 | list2])
  end

  defp unzip([], list1, list2) do
    {list1, list2}
  end

























  def with_index(enumerable, fun_or_offset \\ 0)

  def with_index(enumerable, offset) when is_list(enumerable) and is_integer(offset) do
    with_index_list(enumerable, offset)
  end

  def with_index(enumerable, fun) when is_list(enumerable) and is_function(fun, 2) do
    with_index_list(enumerable, 0, fun)
  end

  def with_index(enumerable, offset) when is_integer(offset) do
    enumerable
    |> map_reduce(offset, fn x, i -> {{x, i}, i + 1} end)
    |> elem(0)
  end

  def with_index(enumerable, fun) when is_function(fun, 2) do
    enumerable
    |> map_reduce(0, fn x, i -> {fun.(x, i), i + 1} end)
    |> elem(0)
  end
























  def zip(enumerable1, enumerable2) when is_list(enumerable1) and is_list(enumerable2) do
    zip_list(enumerable1, enumerable2, [])
  end

  def zip(enumerable1, enumerable2) do
    zip([enumerable1, enumerable2])
  end


















  def zip([]), do: []

  def zip(enumerables) do
    zip_reduce(enumerables, [], &[List.to_tuple(&1) | &2])
    |> :lists.reverse()
  end









































  def zip_with(enumerable1, enumerable2, zip_fun)
      when is_list(enumerable1) and is_list(enumerable2) and is_function(zip_fun, 2) do
    zip_with_list(enumerable1, enumerable2, zip_fun)
  end

  def zip_with(enumerable1, enumerable2, zip_fun) when is_function(zip_fun, 2) do
    zip_reduce(enumerable1, enumerable2, [], fn l, r, acc -> [zip_fun.(l, r) | acc] end)
    |> :lists.reverse()
  end
























  def zip_with([], _fun), do: []

  def zip_with(enumerables, zip_fun) do
    zip_reduce(enumerables, [], fn values, acc -> [zip_fun.(values) | acc] end)
    |> :lists.reverse()
  end





















  def zip_reduce(left, right, acc, reducer)
      when is_list(left) and is_list(right) and is_function(reducer, 3) do
    zip_reduce_list(left, right, acc, reducer)
  end

  def zip_reduce(left, right, acc, reducer) when is_function(reducer, 3) do
    reduce = fn [l, r], acc -> {:cont, reducer.(l, r, acc)} end
    R.zip_with([left, right], & &1).({:cont, acc}, reduce) |> elem(1)
  end






























  def zip_reduce([], acc, reducer) when is_function(reducer, 2), do: acc

  def zip_reduce(enums, acc, reducer) when is_function(reducer, 2) do
    R.zip_with(enums, & &1).({:cont, acc}, &{:cont, reducer.(&1, &2)}) |> elem(1)
  end

  ## Helpers









  defp entry_to_string(entry) when is_binary(entry), do: entry
  defp entry_to_string(entry), do: String.Chars.to_string(entry)

  defp aggregate([head | tail], fun, _empty) do
    aggregate_list(tail, head, fun)
  end

  defp aggregate([], _fun, empty) do
    empty.()
  end

  defp aggregate(first..last//step = range, fun, empty) do
    case Range.size(range) do
      0 ->
        empty.()

      _ ->
        last = last - rem(last - first, step)

        case fun.(first, last) do
          true -> first
          false -> last
        end
    end
  end

  defp aggregate(enumerable, fun, empty) do
    ref = make_ref()

    enumerable
    |> reduce(ref, fn
      element, ^ref ->
        element

      element, acc ->
        case fun.(acc, element) do
          true -> acc
          false -> element
        end
    end)
    |> case do
      ^ref -> empty.()
      result -> result
    end
  end

  defp aggregate_list([head | tail], acc, fun) do
    acc =
      case fun.(acc, head) do
        true -> acc
        false -> head
      end

    aggregate_list(tail, acc, fun)
  end

  defp aggregate_list([], acc, _fun), do: acc

  defp aggregate_by(enumerable, fun, sorter, empty_fallback) do
    first_fun = &[&1 | fun.(&1)]

    reduce_fun = fn entry, [_ | fun_ref] = old ->
      fun_entry = fun.(entry)

      case sorter.(fun_ref, fun_entry) do
        true -> old
        false -> [entry | fun_entry]
      end
    end

    case reduce_by(enumerable, first_fun, reduce_fun) do
      :empty -> empty_fallback.()
      [entry | _] -> entry
    end
  end

  defp reduce_by([head | tail], first, fun) do
    :lists.foldl(fun, first.(head), tail)
  end

  defp reduce_by([], _first, _fun) do
    :empty
  end

  defp reduce_by(enumerable, first, fun) do
    reduce(enumerable, :empty, fn
      element, :empty -> first.(element)
      element, acc -> fun.(element, acc)
    end)
  end

  ## Implementations

  ## all?/1

  defp all_list([h | t]) do
    if h do
      all_list(t)
    else
      false
    end
  end

  defp all_list([]) do
    true
  end

  ## any?/1

  defp any_list([h | t]) do
    if h do
      true
    else
      any_list(t)
    end
  end

  defp any_list([]) do
    false
  end

  ## any?/2 all?/2

  defp predicate_list([h | t], initial, fun) do
    if !!fun.(h) == initial do
      predicate_list(t, initial, fun)
    else
      not initial
    end
  end

  defp predicate_list([], initial, _) do
    initial
  end

  defp predicate_range(first, last, step, initial, fun)
       when step > 0 and first <= last
       when step < 0 and first >= last do
    if !!fun.(first) == initial do
      predicate_range(first + step, last, step, initial, fun)
    else
      not initial
    end
  end

  defp predicate_range(_first, _last, _step, initial, _fun) do
    initial
  end

  ## concat

  defp concat_list([h | t]) when is_list(h), do: h ++ concat_list(t)
  defp concat_list([h | t]), do: concat_enum([h | t])
  defp concat_list([]), do: []

  defp concat_enum(enum) do
    fun = &[&1 | &2]
    enum |> reduce([], &reduce(&1, &2, fun)) |> :lists.reverse()
  end

  # dedup

  defp dedup_list([value | tail], acc) do
    acc =
      case acc do
        [^value | _] -> acc
        _ -> [value | acc]
      end

    dedup_list(tail, acc)
  end

  defp dedup_list([], acc) do
    acc
  end

  ## drop

  defp drop_list(list, 0), do: list
  defp drop_list([_ | tail], counter), do: drop_list(tail, counter - 1)
  defp drop_list([], _), do: []

  ## drop_while

  defp drop_while_list([head | tail], fun) do
    if fun.(head) do
      drop_while_list(tail, fun)
    else
      [head | tail]
    end
  end

  defp drop_while_list([], _) do
    []
  end

  ## filter

  defp filter_list([head | tail], fun) do
    if fun.(head) do
      [head | filter_list(tail, fun)]
    else
      filter_list(tail, fun)
    end
  end

  defp filter_list([], _fun) do
    []
  end

  ## find

  defp find_list([head | tail], default, fun) do
    if fun.(head) do
      head
    else
      find_list(tail, default, fun)
    end
  end

  defp find_list([], default, _) do
    default
  end

  ## find_index

  defp find_index_list([head | tail], counter, fun) do
    if fun.(head) do
      counter
    else
      find_index_list(tail, counter + 1, fun)
    end
  end

  defp find_index_list([], _, _) do
    nil
  end

  ## find_value

  defp find_value_list([head | tail], default, fun) do
    fun.(head) || find_value_list(tail, default, fun)
  end

  defp find_value_list([], default, _) do
    default
  end

  ## flat_map

  defp flat_map_list([head | tail], fun) do
    case fun.(head) do
      # the two first clauses are an optimization
      [] -> flat_map_list(tail, fun)
      [elem] -> [elem | flat_map_list(tail, fun)]
      list when is_list(list) -> list ++ flat_map_list(tail, fun)
      other -> to_list(other) ++ flat_map_list(tail, fun)
    end
  end

  defp flat_map_list([], _fun) do
    []
  end

  ## intersperse

  defp intersperse_non_empty_list([head], _separator), do: [head]

  defp intersperse_non_empty_list([head | rest], separator) do
    [head, separator | intersperse_non_empty_list(rest, separator)]
  end

  ## join

  defp join_list([], _joiner), do: ""

  defp join_list(list, joiner) do
    join_non_empty_list(list, joiner, [])
    |> :lists.reverse()
    |> IO.iodata_to_binary()
  end

  defp join_non_empty_list([first], _joiner, acc), do: [entry_to_string(first) | acc]

  defp join_non_empty_list([first | rest], joiner, acc) do
    join_non_empty_list(rest, joiner, [joiner, entry_to_string(first) | acc])
  end

  ## map

  defp map_range(first, last, step, fun)
       when step > 0 and first <= last
       when step < 0 and first >= last do
    [fun.(first) | map_range(first + step, last, step, fun)]
  end

  defp map_range(_first, _last, _step, _fun) do
    []
  end

  ## map_intersperse

  defp map_intersperse_list([], _, _),
    do: []

  defp map_intersperse_list([last], _, mapper),
    do: [mapper.(last)]

  defp map_intersperse_list([head | rest], separator, mapper),
    do: [mapper.(head), separator | map_intersperse_list(rest, separator, mapper)]

  ## reduce

  defp reduce_range(first, last, step, acc, fun)
       when step > 0 and first <= last
       when step < 0 and first >= last do
    reduce_range(first + step, last, step, fun.(first, acc), fun)
  end

  defp reduce_range(_first, _last, _step, acc, _fun) do
    acc
  end

  defp reduce_enumerable(enumerable, acc, fun) do
    Enumerable.reduce(enumerable, {:cont, acc}, fn x, acc -> {:cont, fun.(x, acc)} end) |> elem(1)
  end

  ## reject

  defp reject_list([head | tail], fun) do
    if fun.(head) do
      reject_list(tail, fun)
    else
      [head | reject_list(tail, fun)]
    end
  end

  defp reject_list([], _fun) do
    []
  end

  ## reverse_slice

  defp reverse_slice(rest, idx, idx, count, acc) do
    {slice, rest} = head_slice(rest, count, [])
    :lists.reverse(rest, :lists.reverse(slice, acc))
  end

  defp reverse_slice([elem | rest], idx, start, count, acc) do
    reverse_slice(rest, idx - 1, start, count, [elem | acc])
  end

  defp head_slice(rest, 0, acc), do: {acc, rest}

  defp head_slice([elem | rest], count, acc) do
    head_slice(rest, count - 1, [elem | acc])
  end

  ## scan

  defp scan_list([], _acc, _fun), do: []

  defp scan_list([elem | rest], acc, fun) do
    acc = fun.(elem, acc)
    [acc | scan_list(rest, acc, fun)]
  end

  ## slice

  defp slice_forward(enumerable, start, amount, step) when start < 0 do
    {count, fun} = slice_count_and_fun(enumerable, step)
    start = count + start

    if start >= 0 do
      amount = Kernel.min(amount, count - start)
      amount = amount_with_step(amount, step)
      fun.(start, amount, step)
    else
      []
    end
  end

  defp slice_forward(list, start, amount, step) when is_list(list) do
    amount = amount_with_step(amount, step)
    slice_list(list, start, amount, step)
  end

  defp slice_forward(enumerable, start, amount, step) do
    case Enumerable.slice(enumerable) do
      {:ok, count, _} when start >= count ->
        []

      {:ok, count, fun} when is_function(fun, 1) ->
        amount = Kernel.min(amount, count - start) |> amount_with_step(step)
        enumerable |> fun.() |> slice_exact(start, amount, step, count)

      {:ok, count, fun} when is_function(fun, 3) ->
        amount = Kernel.min(amount, count - start) |> amount_with_step(step)
        fun.(start, amount, step)

      # TODO: Remove me on v2.0.
      {:ok, count, fun} when is_function(fun, 2) ->
        IO.warn(
          "#{inspect(Enumerable.impl_for(enumerable))} must return a three arity function on slice/1"
        )

        amount = Kernel.min(amount, count - start)

        if step == 1 do
          fun.(start, amount)
        else
          fun.(start, Kernel.min(amount * step, count - start))
          |> take_every_list(amount, step - 1)
        end

      {:error, module} ->
        slice_enum(enumerable, module, start, amount, step)
    end
  end

  defp slice_list(list, start, amount, step) do
    if step == 1 do
      list |> drop_list(start) |> take_list(amount)
    else
      list |> drop_list(start) |> take_every_list(amount, step - 1)
    end
  end

  defp slice_enum(enumerable, module, start, amount, 1) do
    {_, {_, _, slice}} =
      module.reduce(enumerable, {:cont, {start, amount, []}}, fn
        _entry, {start, amount, _list} when start > 0 ->
          {:cont, {start - 1, amount, []}}

        entry, {start, amount, list} when amount > 1 ->
          {:cont, {start, amount - 1, [entry | list]}}

        entry, {start, amount, list} ->
          {:halt, {start, amount, [entry | list]}}
      end)

    :lists.reverse(slice)
  end

  defp slice_enum(enumerable, module, start, amount, step) do
    {_, {_, _, _, slice}} =
      module.reduce(enumerable, {:cont, {start, amount, 1, []}}, fn
        _entry, {start, amount, to_drop, _list} when start > 0 ->
          {:cont, {start - 1, amount, to_drop, []}}

        entry, {start, amount, to_drop, list} when amount > 1 ->
          case to_drop do
            1 -> {:cont, {start, amount - 1, step, [entry | list]}}
            _ -> {:cont, {start, amount - 1, to_drop - 1, list}}
          end

        entry, {start, amount, to_drop, list} ->
          case to_drop do
            1 -> {:halt, {start, amount, to_drop, [entry | list]}}
            _ -> {:halt, {start, amount, to_drop, list}}
          end
      end)

    :lists.reverse(slice)
  end

  defp slice_count_and_fun(list, _step) when is_list(list) do
    length = length(list)
    {length, &slice_exact(list, &1, &2, &3, length)}
  end

  defp slice_count_and_fun(enumerable, step) do
    case Enumerable.slice(enumerable) do
      {:ok, count, fun} when is_function(fun, 1) ->
        {count, &slice_exact(fun.(enumerable), &1, &2, &3, count)}

      {:ok, count, fun} when is_function(fun, 3) ->
        {count, fun}

      # TODO: Remove me on v2.0
      {:ok, count, fun} when is_function(fun, 2) ->
        IO.warn(
          "#{inspect(Enumerable.impl_for(enumerable))} must return a three arity function on slice/1"
        )

        if step == 1 do
          {count, fn start, amount, 1 -> fun.(start, amount) end}
        else
          {count,
           fn start, amount, step ->
             fun.(start, Kernel.min(amount * step, count - start))
             |> take_every_list(amount, step - 1)
           end}
        end

      {:error, module} ->
        {list, count} =
          enumerable
          |> module.reduce({:cont, {[], 0}}, fn elem, {acc, count} ->
            {:cont, {[elem | acc], count + 1}}
          end)
          |> elem(1)

        {count,
         fn start, amount, step ->
           list |> :lists.reverse() |> slice_exact(start, amount, step, count)
         end}
    end
  end

  # Slice a list when we know the bounds
  defp slice_exact(_list, _start, 0, _step, _), do: []

  defp slice_exact(list, start, amount, 1, size) when start + amount == size,
    do: list |> drop_exact(start)

  defp slice_exact(list, start, amount, 1, _),
    do: list |> drop_exact(start) |> take_exact(amount)

  defp slice_exact(list, start, amount, step, _),
    do: list |> drop_exact(start) |> take_every_list(amount, step - 1)

  defp drop_exact(list, 0), do: list
  defp drop_exact([_ | tail], amount), do: drop_exact(tail, amount - 1)

  defp take_exact(_list, 0), do: []
  defp take_exact([head | tail], amount), do: [head | take_exact(tail, amount - 1)]

  ## sort

  defp sort_reducer(entry, {:split, y, x, r, rs, bool}, fun) do
    cond do
      fun.(y, entry) == bool ->
        {:split, entry, y, [x | r], rs, bool}

      fun.(x, entry) == bool ->
        {:split, y, entry, [x | r], rs, bool}

      r == [] ->
        {:split, y, x, [entry], rs, bool}

      true ->
        {:pivot, y, x, r, rs, entry, bool}
    end
  end

  defp sort_reducer(entry, {:pivot, y, x, r, rs, s, bool}, fun) do
    cond do
      fun.(y, entry) == bool ->
        {:pivot, entry, y, [x | r], rs, s, bool}

      fun.(x, entry) == bool ->
        {:pivot, y, entry, [x | r], rs, s, bool}

      fun.(s, entry) == bool ->
        {:split, entry, s, [], [[y, x | r] | rs], bool}

      true ->
        {:split, s, entry, [], [[y, x | r] | rs], bool}
    end
  end

  defp sort_reducer(entry, [x], fun) do
    {:split, entry, x, [], [], fun.(x, entry)}
  end

  defp sort_reducer(entry, acc, _fun) do
    [entry | acc]
  end

  defp sort_terminator({:split, y, x, r, rs, bool}, fun) do
    sort_merge([[y, x | r] | rs], fun, bool)
  end

  defp sort_terminator({:pivot, y, x, r, rs, s, bool}, fun) do
    sort_merge([[s], [y, x | r] | rs], fun, bool)
  end

  defp sort_terminator(acc, _fun) do
    acc
  end

  defp sort_merge(list, fun, true), do: reverse_sort_merge(list, [], fun, true)

  defp sort_merge(list, fun, false), do: sort_merge(list, [], fun, false)

  defp sort_merge([t1, [h2 | t2] | l], acc, fun, true),
    do: sort_merge(l, [sort_merge1(t1, h2, t2, [], fun, false) | acc], fun, true)

  defp sort_merge([[h2 | t2], t1 | l], acc, fun, false),
    do: sort_merge(l, [sort_merge1(t1, h2, t2, [], fun, false) | acc], fun, false)

  defp sort_merge([l], [], _fun, _bool), do: l

  defp sort_merge([l], acc, fun, bool),
    do: reverse_sort_merge([:lists.reverse(l, []) | acc], [], fun, bool)

  defp sort_merge([], acc, fun, bool), do: reverse_sort_merge(acc, [], fun, bool)

  defp reverse_sort_merge([[h2 | t2], t1 | l], acc, fun, true),
    do: reverse_sort_merge(l, [sort_merge1(t1, h2, t2, [], fun, true) | acc], fun, true)

  defp reverse_sort_merge([t1, [h2 | t2] | l], acc, fun, false),
    do: reverse_sort_merge(l, [sort_merge1(t1, h2, t2, [], fun, true) | acc], fun, false)

  defp reverse_sort_merge([l], acc, fun, bool),
    do: sort_merge([:lists.reverse(l, []) | acc], [], fun, bool)

  defp reverse_sort_merge([], acc, fun, bool), do: sort_merge(acc, [], fun, bool)

  defp sort_merge1([h1 | t1], h2, t2, m, fun, bool) do
    if fun.(h1, h2) == bool do
      sort_merge2(h1, t1, t2, [h2 | m], fun, bool)
    else
      sort_merge1(t1, h2, t2, [h1 | m], fun, bool)
    end
  end

  defp sort_merge1([], h2, t2, m, _fun, _bool), do: :lists.reverse(t2, [h2 | m])

  defp sort_merge2(h1, t1, [h2 | t2], m, fun, bool) do
    if fun.(h1, h2) == bool do
      sort_merge2(h1, t1, t2, [h2 | m], fun, bool)
    else
      sort_merge1(t1, h2, t2, [h1 | m], fun, bool)
    end
  end

  defp sort_merge2(h1, t1, [], m, _fun, _bool), do: :lists.reverse(t1, [h1 | m])

  ## split

  defp split_list([head | tail], counter, acc) when counter > 0 do
    split_list(tail, counter - 1, [head | acc])
  end

  defp split_list(list, 0, acc) do
    {:lists.reverse(acc), list}
  end

  defp split_list([], _, acc) do
    {:lists.reverse(acc), []}
  end

  defp split_reverse_list([head | tail], counter, acc) when counter > 0 do
    split_reverse_list(tail, counter - 1, [head | acc])
  end

  defp split_reverse_list(list, 0, acc) do
    {:lists.reverse(list), acc}
  end

  defp split_reverse_list([], _, acc) do
    {[], acc}
  end

  ## split_while

  defp split_while_list([head | tail], fun, acc) do
    if fun.(head) do
      split_while_list(tail, fun, [head | acc])
    else
      {:lists.reverse(acc), [head | tail]}
    end
  end

  defp split_while_list([], _, acc) do
    {:lists.reverse(acc), []}
  end

  ## sum_by

  defp sum_by_list([], _, acc), do: acc
  defp sum_by_list([h | t], mapper, acc), do: sum_by_list(t, mapper, acc + mapper.(h))

  ## product_by

  defp product_by_list([], _, acc), do: acc
  defp product_by_list([h | t], mapper, acc), do: product_by_list(t, mapper, acc * mapper.(h))

  ## take

  defp take_list(_list, 0), do: []
  defp take_list([head | tail], counter), do: [head | take_list(tail, counter - 1)]
  defp take_list([], _counter), do: []

  defp take_every_list([head | tail], to_drop),
    do: [head | tail |> drop_list(to_drop) |> take_every_list(to_drop)]

  defp take_every_list([], _to_drop), do: []

  defp take_every_list(_list, 0, _to_drop), do: []

  defp take_every_list([head | tail], counter, to_drop),
    do: [head | tail |> drop_list(to_drop) |> take_every_list(counter - 1, to_drop)]

  defp take_every_list([], _counter, _to_drop), do: []

  ## take_while

  defp take_while_list([head | tail], fun) do
    if fun.(head) do
      [head | take_while_list(tail, fun)]
    else
      []
    end
  end

  defp take_while_list([], _) do
    []
  end

  ## uniq

  defp uniq_list([head | tail], set, fun) do
    value = fun.(head)

    case set do
      %{^value => true} -> uniq_list(tail, set, fun)
      %{} -> [head | uniq_list(tail, Map.put(set, value, true), fun)]
    end
  end

  defp uniq_list([], _set, _fun) do
    []
  end

  ## with_index

  defp with_index_list([head | tail], offset) do
    [{head, offset} | with_index_list(tail, offset + 1)]
  end

  defp with_index_list([], _offset), do: []

  defp with_index_list([head | tail], offset, fun) do
    [fun.(head, offset) | with_index_list(tail, offset + 1, fun)]
  end

  defp with_index_list([], _offset, _fun), do: []

  ## zip

  defp zip_list([head1 | next1], [head2 | next2], acc) do
    zip_list(next1, next2, [{head1, head2} | acc])
  end

  defp zip_list([], _, acc), do: :lists.reverse(acc)
  defp zip_list(_, [], acc), do: :lists.reverse(acc)

  defp zip_with_list([head1 | next1], [head2 | next2], fun) do
    [fun.(head1, head2) | zip_with_list(next1, next2, fun)]
  end

  defp zip_with_list(_, [], _fun), do: []
  defp zip_with_list([], _, _fun), do: []

  defp zip_reduce_list([head1 | next1], [head2 | next2], acc, fun) do
    zip_reduce_list(next1, next2, fun.(head1, head2, acc), fun)
  end

  defp zip_reduce_list(_, [], acc, _fun), do: acc
  defp zip_reduce_list([], _, acc, _fun), do: acc
end

defimpl Enumerable, for: List do
  def count(list), do: {:ok, length(list)}

  def member?([], _value), do: {:ok, false}
  def member?(_list, _value), do: {:error, __MODULE__}

  def slice([]), do: {:ok, 0, fn _, _, _ -> [] end}
  def slice(_list), do: {:error, __MODULE__}

  def reduce(_list, {:halt, acc}, _fun), do: {:halted, acc}
  def reduce(list, {:suspend, acc}, fun), do: {:suspended, acc, &reduce(list, &1, fun)}
  def reduce([], {:cont, acc}, _fun), do: {:done, acc}
  def reduce([head | tail], {:cont, acc}, fun), do: reduce(tail, fun.(head, acc), fun)
end

defimpl Enumerable, for: Map do
  def count(map) do
    {:ok, map_size(map)}
  end

  def member?(map, {key, value}) do
    {:ok, match?(%{^key => ^value}, map)}
  end

  def member?(_map, _other) do
    {:ok, false}
  end

  def slice(map) do
    size = map_size(map)
    {:ok, size, &:maps.to_list/1}
  end

  def reduce(map, acc, fun) do
    Enumerable.List.reduce(:maps.to_list(map), acc, fun)
  end
end

defimpl Enumerable, for: Function do
  def count(_function), do: {:error, __MODULE__}
  def member?(_function, _value), do: {:error, __MODULE__}
  def slice(_function), do: {:error, __MODULE__}

  def reduce(function, acc, fun) when is_function(function, 2), do: function.(acc, fun)

  def reduce(function, _acc, _fun) do
    raise Protocol.UndefinedError,
      protocol: @protocol,
      value: function,
      description: "only anonymous functions of arity 2 are enumerable"
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/enum.ex (docs and specs stripped;
# line numbers match the original).
