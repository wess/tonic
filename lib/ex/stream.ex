defmodule Stream do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.















































































































  defstruct enum: nil, funs: [], accs: [], done: nil










  # Require Stream.Reducers and its callbacks
  require Stream.Reducers, as: R

  defmacrop skip(acc) do
    {:cont, acc}
  end

  defmacrop next(fun, entry, acc) do
    quote(do: unquote(fun).(unquote(entry), unquote(acc)))
  end

  defmacrop acc(head, state, tail) do
    quote(do: [unquote(head), unquote(state) | unquote(tail)])
  end

  defmacrop next_with_acc(fun, entry, head, state, tail) do
    quote do
      {reason, [head | tail]} = unquote(fun).(unquote(entry), [unquote(head) | unquote(tail)])
      {reason, [head, unquote(state) | tail]}
    end
  end

  ## Transformers



  def chunk(enum, n), do: chunk(enum, n, n, nil)



  def chunk(enum, n, step) do
    chunk_every(enum, n, step, nil)
  end



  def chunk(enum, n, step, leftover)
      when is_integer(n) and n > 0 and is_integer(step) and step > 0 do
    chunk_every(enum, n, step, leftover || :discard)
  end






  def chunk_every(enum, count), do: chunk_every(enum, count, count, [])






































  def chunk_every(enum, count, step, leftover \\ [])
      when is_integer(count) and count > 0 and is_integer(step) and step > 0 do
    R.chunk_every(&chunk_while/4, enum, count, step, leftover)
  end














  def chunk_by(enum, fun) when is_function(fun, 1) do
    R.chunk_by(&chunk_while/4, enum, fun)
  end






































  def chunk_while(enum, acc, chunk_fun, after_fun)
      when is_function(chunk_fun, 2) and is_function(after_fun, 1) do
    lazy(
      enum,
      [acc | after_fun],
      fn f1 -> chunk_while_fun(chunk_fun, f1) end,
      &after_chunk_while/2
    )
  end

  defp chunk_while_fun(callback, fun) do
    fn entry, acc(head, [acc | after_fun], tail) ->
      case callback.(entry, acc) do
        {:cont, emit, acc} ->
          # If we emit an element and then we have to halt,
          # we need to disable the after_fun callback to
          # avoid emitting even more elements.
          case next(fun, emit, [head | tail]) do
            {:halt, [head | tail]} -> {:halt, acc(head, [acc | &{:cont, &1}], tail)}
            {command, [head | tail]} -> {command, acc(head, [acc | after_fun], tail)}
          end

        {:cont, acc} ->
          skip(acc(head, [acc | after_fun], tail))

        {:halt, acc} ->
          {:halt, acc(head, [acc | after_fun], tail)}
      end
    end
  end

  defp after_chunk_while(acc(h, [acc | after_fun], t), f1) do
    case after_fun.(acc) do
      {:cont, emit, acc} -> next_with_acc(f1, emit, h, [acc | after_fun], t)
      {:cont, acc} -> {:cont, acc(h, [acc | after_fun], t)}
    end
  end















  def dedup(enum) do
    dedup_by(enum, fn x -> x end)
  end












  def dedup_by(enum, fun) when is_function(fun, 1) do
    lazy(enum, nil, fn f1 -> R.dedup(fun, f1) end)
  end





















  def drop(enum, n) when is_integer(n) and n >= 0 do
    lazy(enum, n, fn f1 -> R.drop(f1) end)
  end

  def drop(enum, n) when is_integer(n) and n < 0 do
    n = abs(n)

    lazy(enum, {0, [], []}, fn f1 ->
      fn
        entry, [h, {count, buf1, []} | t] ->
          do_drop(:cont, n, entry, h, count, buf1, [], t)

        entry, [h, {count, buf1, [next | buf2]} | t] ->
          {reason, [h | t]} = f1.(next, [h | t])
          do_drop(reason, n, entry, h, count, buf1, buf2, t)
      end
    end)
  end

  defp do_drop(reason, n, entry, h, count, buf1, buf2, t) do
    buf1 = [entry | buf1]
    count = count + 1

    if count == n do
      {reason, [h, {0, [], :lists.reverse(buf1)} | t]}
    else
      {reason, [h, {count, buf1, buf2} | t]}
    end
  end
























  def drop_every(enum, nth)
  def drop_every(enum, 0), do: %Stream{enum: enum}
  def drop_every([], _nth), do: %Stream{enum: []}

  def drop_every(enum, nth) when is_integer(nth) and nth > 0 do
    lazy(enum, nth, fn f1 -> R.drop_every(nth, f1) end)
  end













  def drop_while(enum, fun) when is_function(fun, 1) do
    lazy(enum, true, fn f1 -> R.drop_while(fun, f1) end)
  end




























  def duplicate(value, n) when is_integer(n) and n >= 0 do
    unfold(n, fn
      0 -> nil
      remaining -> {value, remaining - 1}
    end)
  end






















  def each(enum, fun) when is_function(fun, 1) do
    lazy(enum, fn f1 ->
      fn x, acc ->
        fun.(x)
        f1.(x, acc)
      end
    end)
  end



















  def flat_map(enum, mapper) when is_function(mapper, 1) do
    transform(enum, nil, fn val, nil -> {mapper.(val), nil} end)
  end













  def filter(enum, fun) when is_function(fun, 1) do
    lazy(enum, fn f1 -> R.filter(fun, f1) end)
  end



  def filter_map(enum, filter, mapper) do
    lazy(enum, fn f1 -> R.filter_map(filter, mapper, f1) end)
  end




















  def interval(n)
      when is_integer(n) and n >= 0
      when n == :infinity do
    unfold(0, fn count ->
      Process.sleep(n)
      {count, count + 1}
    end)
  end








  def into(enum, collectable, transform \\ fn x -> x end) when is_function(transform, 1) do
    &do_into(enum, collectable, transform, &1, &2)
  end

  defp do_into(enum, collectable, transform, acc, fun) do
    {initial, into} = Collectable.into(collectable)

    composed = fn x, [acc | collectable] ->
      collectable = into.(collectable, {:cont, transform.(x)})
      {reason, acc} = fun.(x, acc)
      {reason, [acc | collectable]}
    end

    do_into(&Enumerable.reduce(enum, &1, composed), initial, into, acc)
  end

  defp do_into(reduce, collectable, into, {command, acc}) do
    try do
      reduce.({command, [acc | collectable]})
    catch
      kind, reason ->
        into.(collectable, :halt)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      {:suspended, [acc | collectable], continuation} ->
        {:suspended, acc, &do_into(continuation, collectable, into, &1)}

      {reason, [acc | collectable]} ->
        into.(collectable, :done)
        {reason, acc}
    end
  end













  def map(enum, fun) when is_function(fun, 1) do
    lazy(enum, fn f1 -> R.map(fun, f1) end)
  end


























  def map_every(enum, nth, fun) when is_integer(nth) and nth >= 0 and is_function(fun, 1) do
    map_every_after_guards(enum, nth, fun)
  end

  defp map_every_after_guards(enum, 1, fun), do: map(enum, fun)
  defp map_every_after_guards(enum, 0, _fun), do: %Stream{enum: enum}
  defp map_every_after_guards([], _nth, _fun), do: %Stream{enum: []}

  defp map_every_after_guards(enum, nth, fun) do
    lazy(enum, nth, fn f1 -> R.map_every(nth, fun, f1) end)
  end













  def reject(enum, fun) when is_function(fun, 1) do
    lazy(enum, fn f1 -> R.reject(fun, f1) end)
  end





















  def run(stream) do
    _ = Enumerable.reduce(stream, {:cont, nil}, fn _, _ -> {:cont, nil} end)
    :ok
  end















  def scan(enum, fun) when is_function(fun, 2) do
    lazy(enum, :first, fn f1 -> R.scan2(fun, f1) end)
  end














  def scan(enum, acc, fun) when is_function(fun, 2) do
    lazy(enum, acc, fn f1 -> R.scan3(fun, f1) end)
  end



























  def take(enum, count) when is_integer(count) do
    take_after_guards(enum, count)
  end

  defp take_after_guards(_enum, 0), do: %Stream{enum: []}

  defp take_after_guards([], _count), do: %Stream{enum: []}

  defp take_after_guards(enum, count) when count > 0 do
    lazy(enum, count, fn f1 -> R.take(f1) end)
  end

  defp take_after_guards(enum, count) when count < 0 do
    &Enumerable.reduce(Enum.take(enum, count), &1, &2)
  end
























  def take_every(enum, nth) when is_integer(nth) and nth >= 0 do
    take_every_after_guards(enum, nth)
  end

  defp take_every_after_guards(_enum, 0), do: %Stream{enum: []}

  defp take_every_after_guards([], _nth), do: %Stream{enum: []}

  defp take_every_after_guards(enum, nth) do
    lazy(enum, nth, fn f1 -> R.take_every(nth, f1) end)
  end













  def take_while(enum, fun) when is_function(fun, 1) do
    lazy(enum, fn f1 -> R.take_while(fun, f1) end)
  end














  def timer(n)
      when is_integer(n) and n >= 0
      when n == :infinity do
    take(interval(n), 1)
  end
































  def transform(enum, acc, reducer) when is_function(reducer, 2) do
    &do_transform(enum, fn -> acc end, reducer, &1, &2, nil, fn acc -> acc end)
  end












  def transform(enum, start_fun, reducer, after_fun)
      when is_function(start_fun, 0) and is_function(reducer, 2) and is_function(after_fun, 1) do
    &do_transform(enum, start_fun, reducer, &1, &2, nil, after_fun)
  end























  def transform(enum, start_fun, reducer, last_fun, after_fun)
      when is_function(start_fun, 0) and is_function(reducer, 2) and is_function(last_fun, 1) and
             is_function(after_fun, 1) do
    &do_transform(enum, start_fun, reducer, &1, &2, last_fun, after_fun)
  end

  defp do_transform(enumerables, user_acc, user, inner_acc, fun, last_fun, after_fun) do
    inner = &do_transform_each(&1, &2, fun)
    step = &do_transform_step(&1, &2)
    next = &Enumerable.reduce(enumerables, &1, step)
    funs = {user, fun, inner, last_fun, after_fun}
    do_transform(user_acc.(), :cont, next, inner_acc, funs)
  end

  defp do_transform(user_acc, _next_op, next, {:halt, inner_acc}, funs) do
    {_, _, _, _, after_fun} = funs
    next.({:halt, []})
    after_fun.(user_acc)
    {:halted, inner_acc}
  end

  defp do_transform(user_acc, next_op, next, {:suspend, inner_acc}, funs) do
    {:suspended, inner_acc, &do_transform(user_acc, next_op, next, &1, funs)}
  end

  defp do_transform(user_acc, :cont, next, inner_acc, funs) do
    {_, _, _, _, after_fun} = funs

    try do
      next.({:cont, []})
    catch
      kind, reason ->
        after_fun.(user_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      {:suspended, vals, next} ->
        do_transform_user(:lists.reverse(vals), user_acc, :cont, next, inner_acc, funs)

      {_, vals} ->
        # Do not attempt to call the resource again, it has either done or halted
        next = fn _ -> {:done, []} end
        do_transform_user(:lists.reverse(vals), user_acc, :last, next, inner_acc, funs)
    end
  end

  defp do_transform(user_acc, :last, next, inner_acc, funs) do
    {_, _, _, last_fun, after_fun} = funs

    if last_fun do
      try do
        last_fun.(user_acc)
      catch
        kind, reason ->
          after_fun.(user_acc)
          :erlang.raise(kind, reason, __STACKTRACE__)
      else
        result -> do_transform_result(result, [], :halt, next, inner_acc, funs)
      end
    else
      do_transform(user_acc, :halt, next, inner_acc, funs)
    end
  end

  defp do_transform(user_acc, :halt, _next, inner_acc, funs) do
    {_, _, _, _, after_fun} = funs
    after_fun.(user_acc)
    {:halted, elem(inner_acc, 1)}
  end

  defp do_transform_user([], user_acc, next_op, next, inner_acc, funs) do
    do_transform(user_acc, next_op, next, inner_acc, funs)
  end

  defp do_transform_user([val | vals], user_acc, next_op, next, inner_acc, funs) do
    {user, _, _, _, after_fun} = funs

    try do
      user.(val, user_acc)
    catch
      kind, reason ->
        next.({:halt, []})
        after_fun.(user_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      result -> do_transform_result(result, vals, next_op, next, inner_acc, funs)
    end
  end

  defp do_transform_result(result, vals, next_op, next, inner_acc, funs) do
    {_, fun, inner, _, after_fun} = funs

    case result do
      {[], user_acc} ->
        do_transform_user(vals, user_acc, next_op, next, inner_acc, funs)

      {list, user_acc} when is_list(list) ->
        reduce = &Enumerable.List.reduce(list, &1, fun)
        do_transform_inner_list(vals, user_acc, next_op, next, inner_acc, reduce, funs)

      {:halt, user_acc} ->
        next.({:halt, []})
        after_fun.(user_acc)
        {:halted, elem(inner_acc, 1)}

      {other, user_acc} ->
        reduce = &Enumerable.reduce(other, &1, inner)
        do_transform_inner_enum(vals, user_acc, next_op, next, inner_acc, reduce, funs)
    end
  end

  defp do_transform_inner_list(vals, user_acc, next_op, next, inner_acc, reduce, funs) do
    {_, _, _, _, after_fun} = funs

    try do
      reduce.(inner_acc)
    catch
      kind, reason ->
        next.({:halt, []})
        after_fun.(user_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      {:done, acc} ->
        do_transform_user(vals, user_acc, next_op, next, {:cont, acc}, funs)

      {:halted, acc} ->
        next.({:halt, []})
        after_fun.(user_acc)
        {:halted, acc}

      {:suspended, acc, continuation} ->
        resume = &do_transform_inner_list(vals, user_acc, next_op, next, &1, continuation, funs)
        {:suspended, acc, resume}
    end
  end

  defp do_transform_inner_enum(vals, user_acc, next_op, next, {op, inner_acc}, reduce, funs) do
    {_, _, _, _, after_fun} = funs

    try do
      reduce.({op, [:outer | inner_acc]})
    catch
      kind, reason ->
        next.({:halt, []})
        after_fun.(user_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      # Only take into account outer halts when the op is not halt itself.
      # Otherwise, we were the ones wishing to halt, so we should just stop.
      {:halted, [:outer | acc]} when op != :halt ->
        do_transform_user(vals, user_acc, next_op, next, {:cont, acc}, funs)

      {:halted, [_ | acc]} ->
        next.({:halt, []})
        after_fun.(user_acc)
        {:halted, acc}

      {:done, [_ | acc]} ->
        do_transform_user(vals, user_acc, next_op, next, {:cont, acc}, funs)

      {:suspended, [_ | acc], continuation} ->
        resume = &do_transform_inner_enum(vals, user_acc, next_op, next, &1, continuation, funs)
        {:suspended, acc, resume}
    end
  end

  defp do_transform_each(x, [:outer | acc], f) do
    case f.(x, acc) do
      {:halt, res} -> {:halt, [:inner | res]}
      {op, res} -> {op, [:outer | res]}
    end
  end

  defp do_transform_step(x, acc) do
    {:suspend, [x | acc]}
  end
















  def uniq(enum) do
    uniq_by(enum, fn x -> x end)
  end



  def uniq(enum, fun) do
    uniq_by(enum, fun)
  end























  def uniq_by(enum, fun) when is_function(fun, 1) do
    lazy(enum, %{}, fn f1 -> R.uniq_by(fun, f1) end)
  end



























  def from_index(fun_or_offset \\ 0)

  def from_index(offset) when is_integer(offset) do
    unfold(offset, &{&1, &1 + 1})
  end

  def from_index(fun) when is_function(fun) do
    unfold(0, &{fun.(&1), &1 + 1})
  end































  def with_index(enum, fun_or_offset \\ 0)

  def with_index(enum, offset) when is_integer(offset) do
    lazy(enum, offset, fn f1 -> R.with_index(f1) end)
  end

  def with_index(enum, fun) when is_function(fun, 2) do
    lazy(enum, 0, fn f1 -> R.with_index(fun, f1) end)
  end

  ## Combiners












  def concat(enumerables) do
    flat_map(enumerables, & &1)
  end


















  def concat(first, second) do
    flat_map([first, second], & &1)
  end






















  def zip(enumerable1, enumerable2) do
    zip_with(enumerable1, enumerable2, fn left, right -> {left, right} end)
  end

















  def zip(enumerables) do
    zip_with(enumerables, &List.to_tuple(&1))
  end


















  def zip_with(enumerable1, enumerable2, zip_fun)
      when is_list(enumerable1) and is_list(enumerable2) and is_function(zip_fun, 2) do
    &zip_pair(enumerable1, enumerable2, &1, &2, zip_fun)
  end

  def zip_with(enumerable1, enumerable2, zip_fun) when is_function(zip_fun, 2) do
    zip_with([enumerable1, enumerable2], fn [left, right] -> zip_fun.(left, right) end)
  end

  defp zip_pair(_list1, _list2, {:halt, acc}, _fun, _zip_fun) do
    {:halted, acc}
  end

  defp zip_pair(list1, list2, {:suspend, acc}, fun, zip_fun) do
    {:suspended, acc, &zip_pair(list1, list2, &1, fun, zip_fun)}
  end

  defp zip_pair([], _list2, {:cont, acc}, _fun, _zip_fun), do: {:done, acc}
  defp zip_pair(_list1, [], {:cont, acc}, _fun, _zip_fun), do: {:done, acc}

  defp zip_pair([head1 | tail1], [head2 | tail2], {:cont, acc}, fun, zip_fun) do
    zip_pair(tail1, tail2, fun.(zip_fun.(head1, head2), acc), fun, zip_fun)
  end

























  def zip_with(enumerables, zip_fun) do
    R.zip_with(enumerables, zip_fun)
  end

  ## Sources













  def cycle(enumerable)

  def cycle([]) do
    raise ArgumentError, "cannot cycle over an empty enumerable"
  end

  def cycle(enumerable) when is_list(enumerable) do
    unfold({enumerable, enumerable}, fn
      {source, [h | t]} -> {h, {source, t}}
      {source = [h | t], []} -> {h, {source, t}}
    end)
  end

  def cycle(enumerable) do
    fn acc, fun ->
      step = &do_cycle_step(&1, &2)
      cycle = &Enumerable.reduce(enumerable, &1, step)
      reduce = check_cycle_first_element(cycle)
      do_cycle(reduce, [], cycle, acc, fun)
    end
  end

  defp do_cycle(reduce, inner_acc, _cycle, {:halt, acc}, _fun) do
    reduce.({:halt, inner_acc})
    {:halted, acc}
  end

  defp do_cycle(reduce, inner_acc, cycle, {:suspend, acc}, fun) do
    {:suspended, acc, &do_cycle(reduce, inner_acc, cycle, &1, fun)}
  end

  defp do_cycle(reduce, inner_acc, cycle, {:cont, acc}, fun) do
    case reduce.({:cont, inner_acc}) do
      {:suspended, [element], new_reduce} ->
        do_cycle(new_reduce, inner_acc, cycle, fun.(element, acc), fun)

      {_, [element]} ->
        do_cycle(cycle, [], cycle, fun.(element, acc), fun)

      {_, []} ->
        do_cycle(cycle, [], cycle, {:cont, acc}, fun)
    end
  end

  defp do_cycle_step(x, acc) do
    {:suspend, [x | acc]}
  end

  defp check_cycle_first_element(reduce) do
    fn acc ->
      case reduce.(acc) do
        {state, []} when state in [:done, :halted] and elem(acc, 0) != :halt ->
          raise ArgumentError, "cannot cycle over an empty enumerable"

        other ->
          other
      end
    end
  end














  def iterate(start_value, next_fun) when is_function(next_fun, 1) do
    unfold({:ok, start_value}, fn
      {:ok, value} ->
        {value, {:next, value}}

      {:next, value} ->
        next = next_fun.(value)
        {next, {:next, next}}
    end)
  end













  def repeatedly(generator_fun) when is_function(generator_fun, 0) do
    &do_repeatedly(generator_fun, &1, &2)
  end

  defp do_repeatedly(generator_fun, {:suspend, acc}, fun) do
    {:suspended, acc, &do_repeatedly(generator_fun, &1, fun)}
  end

  defp do_repeatedly(_generator_fun, {:halt, acc}, _fun) do
    {:halted, acc}
  end

  defp do_repeatedly(generator_fun, {:cont, acc}, fun) do
    do_repeatedly(generator_fun, fun.(generator_fun.(), acc), fun)
  end
















































  def resource(start_fun, next_fun, after_fun)
      when is_function(start_fun, 0) and is_function(next_fun, 1) and is_function(after_fun, 1) do
    &do_resource(start_fun.(), next_fun, &1, &2, after_fun)
  end

  defp do_resource(next_acc, next_fun, {:suspend, acc}, fun, after_fun) do
    {:suspended, acc, &do_resource(next_acc, next_fun, &1, fun, after_fun)}
  end

  defp do_resource(next_acc, _next_fun, {:halt, acc}, _fun, after_fun) do
    after_fun.(next_acc)
    {:halted, acc}
  end

  defp do_resource(next_acc, next_fun, {:cont, acc}, fun, after_fun) do
    try do
      next_fun.(next_acc)
    catch
      kind, reason ->
        after_fun.(next_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      {:halt, next_acc} ->
        do_resource(next_acc, next_fun, {:halt, acc}, fun, after_fun)

      {[], next_acc} ->
        do_resource(next_acc, next_fun, {:cont, acc}, fun, after_fun)

      {[v], next_acc} ->
        do_element_resource(next_acc, next_fun, acc, fun, after_fun, v)

      {list, next_acc} when is_list(list) ->
        reduce = &Enumerable.List.reduce(list, &1, fun)
        do_list_resource(next_acc, next_fun, {:cont, acc}, fun, after_fun, reduce)

      {enum, next_acc} ->
        inner = &do_resource_each(&1, &2, fun)
        reduce = &Enumerable.reduce(enum, &1, inner)
        do_enum_resource(next_acc, next_fun, {:cont, acc}, fun, after_fun, reduce)
    end
  end

  defp do_element_resource(next_acc, next_fun, acc, fun, after_fun, v) do
    try do
      fun.(v, acc)
    catch
      kind, reason ->
        after_fun.(next_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      acc ->
        do_resource(next_acc, next_fun, acc, fun, after_fun)
    end
  end

  defp do_list_resource(next_acc, next_fun, acc, fun, after_fun, reduce) do
    try do
      reduce.(acc)
    catch
      kind, reason ->
        after_fun.(next_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      {:done, acc} ->
        do_resource(next_acc, next_fun, {:cont, acc}, fun, after_fun)

      {:halted, acc} ->
        do_resource(next_acc, next_fun, {:halt, acc}, fun, after_fun)

      {:suspended, acc, c} ->
        {:suspended, acc, &do_list_resource(next_acc, next_fun, &1, fun, after_fun, c)}
    end
  end

  defp do_enum_resource(next_acc, next_fun, {op, acc}, fun, after_fun, reduce) do
    try do
      reduce.({op, [:outer | acc]})
    catch
      kind, reason ->
        after_fun.(next_acc)
        :erlang.raise(kind, reason, __STACKTRACE__)
    else
      {:halted, [:outer | acc]} ->
        do_resource(next_acc, next_fun, {:cont, acc}, fun, after_fun)

      {:halted, [:inner | acc]} ->
        do_resource(next_acc, next_fun, {:halt, acc}, fun, after_fun)

      {:done, [_ | acc]} ->
        do_resource(next_acc, next_fun, {:cont, acc}, fun, after_fun)

      {:suspended, [_ | acc], c} ->
        {:suspended, acc, &do_enum_resource(next_acc, next_fun, &1, fun, after_fun, c)}
    end
  end

  defp do_resource_each(x, [:outer | acc], f) do
    case f.(x, acc) do
      {:halt, res} -> {:halt, [:inner | res]}
      {op, res} -> {op, [:outer | res]}
    end
  end
































  def unfold(next_acc, next_fun) when is_function(next_fun, 1) do
    &do_unfold(next_acc, next_fun, &1, &2)
  end

  defp do_unfold(next_acc, next_fun, {:suspend, acc}, fun) do
    {:suspended, acc, &do_unfold(next_acc, next_fun, &1, fun)}
  end

  defp do_unfold(_next_acc, _next_fun, {:halt, acc}, _fun) do
    {:halted, acc}
  end

  defp do_unfold(next_acc, next_fun, {:cont, acc}, fun) do
    case next_fun.(next_acc) do
      nil -> {:done, acc}
      {v, next_acc} -> do_unfold(next_acc, next_fun, fun.(v, acc), fun)
    end
  end


















  def intersperse(enumerable, intersperse_element) do
    Stream.transform(enumerable, false, fn
      element, true -> {[intersperse_element, element], true}
      element, false -> {[element], true}
    end)
  end

  ## Helpers



  defp lazy(%Stream{done: nil, funs: funs} = lazy, fun), do: %{lazy | funs: [fun | funs]}
  defp lazy(enum, fun), do: %Stream{enum: enum, funs: [fun]}

  defp lazy(%Stream{done: nil, funs: funs, accs: accs} = lazy, acc, fun),
    do: %{lazy | funs: [fun | funs], accs: [acc | accs]}

  defp lazy(enum, acc, fun), do: %Stream{enum: enum, funs: [fun], accs: [acc]}

  defp lazy(%Stream{done: nil, funs: funs, accs: accs} = lazy, acc, fun, done),
    do: %{lazy | funs: [fun | funs], accs: [acc | accs], done: done}

  defp lazy(enum, acc, fun, done), do: %Stream{enum: enum, funs: [fun], accs: [acc], done: done}
end

defimpl Enumerable, for: Stream do


  def count(_lazy), do: {:error, __MODULE__}

  def member?(_lazy, _value), do: {:error, __MODULE__}

  def slice(_lazy), do: {:error, __MODULE__}

  def reduce(lazy, acc, fun) do
    do_reduce(lazy, acc, fn x, [acc] ->
      {reason, acc} = fun.(x, acc)
      {reason, [acc]}
    end)
  end

  defp do_reduce(%Stream{enum: enum, funs: funs, accs: accs, done: done}, acc, fun) do
    composed = :lists.foldl(fn entry_fun, acc -> entry_fun.(acc) end, fun, funs)
    reduce = &Enumerable.reduce(enum, &1, composed)
    do_each(reduce, done && {done, fun}, :lists.reverse(accs), acc)
  end

  defp do_each(reduce, done, accs, {command, acc}) do
    case reduce.({command, [acc | accs]}) do
      {:suspended, [acc | accs], continuation} ->
        {:suspended, acc, &do_each(continuation, done, accs, &1)}

      {:halted, accs} ->
        do_done({:halted, accs}, done)

      {:done, accs} ->
        do_done({:done, accs}, done)
    end
  end

  defp do_done({reason, [acc | _]}, nil), do: {reason, acc}

  defp do_done({reason, [acc | t]}, {done, fun}) do
    [h | _] = :lists.reverse(t)

    case done.([acc, h], fun) do
      {:cont, [acc | _]} -> {reason, acc}
      {:halt, [acc | _]} -> {:halted, acc}
      {:suspend, [acc | _]} -> {:suspended, acc, &{:done, elem(&1, 1)}}
    end
  end
end

defimpl Inspect, for: Stream do
  import Inspect.Algebra

  def inspect(%{enum: enum, funs: funs}, opts) do
    inner = [enum: enum, funs: :lists.reverse(funs)]
    concat(["#Stream<", to_doc(inner, opts), ">"])
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/stream.ex (docs and specs stripped;
# line numbers match the original).
