defmodule Keyword do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.

























































































































  def from_keys(keys, value) when is_list(keys) do
    :lists.map(&{&1, value}, keys)
  end























  def keyword?(term)

  def keyword?([{key, _value} | rest]) when is_atom(key), do: keyword?(rest)
  def keyword?([]), do: true
  def keyword?(_other), do: false











  def new, do: []


















  def new(pairs) do
    new(pairs, fn pair -> pair end)
  end















  def new(pairs, transform) when is_function(transform, 1) do
    fun = fn el, acc ->
      {k, v} = transform.(el)
      put_new(acc, k, v)
    end

    :lists.foldl(fun, [], Enum.reverse(pairs))
  end
















































  def validate(keyword, values) when is_list(keyword) and is_list(values) do
    validate(keyword, values, [], [], [])
  end

  defp validate([{key, _} = pair | keyword], values1, values2, acc, bad_keys) when is_atom(key) do
    case find_key!(key, values1, values2) do
      {values1, values2} ->
        validate(keyword, values1, values2, [pair | acc], bad_keys)

      :error ->
        case find_key!(key, values2, values1) do
          {values1, values2} ->
            validate(keyword, values1, values2, [pair | acc], bad_keys)

          :error ->
            validate(keyword, values1, values2, acc, [key | bad_keys])
        end
    end
  end

  defp validate([], values1, values2, acc, []) do
    {:ok, move_pairs!(values1, move_pairs!(values2, acc))}
  end

  defp validate([], _values1, _values2, _acc, bad_keys) do
    {:error, bad_keys}
  end

  defp validate([pair | _], _values1, _values2, _acc, []) do
    raise ArgumentError,
          "expected a keyword list as first argument, got invalid entry: #{inspect(pair)}"
  end

  defp find_key!(key, [key | rest], acc), do: {rest, acc}
  defp find_key!(key, [{key, _} | rest], acc), do: {rest, acc}
  defp find_key!(key, [head | tail], acc), do: find_key!(key, tail, [head | acc])
  defp find_key!(_key, [], _acc), do: :error

  defp move_pairs!([key | rest], acc) when is_atom(key),
    do: move_pairs!(rest, acc)

  defp move_pairs!([{key, _} = pair | rest], acc) when is_atom(key),
    do: move_pairs!(rest, [pair | acc])

  defp move_pairs!([], acc),
    do: acc

  defp move_pairs!([other | _], _) do
    raise ArgumentError,
          "expected the second argument to be a list of atoms or tuples, got: #{inspect(other)}"
  end
































  def validate!(keyword, values) do
    case validate(keyword, values) do
      {:ok, kw} ->
        kw

      {:error, invalid_keys} ->
        keys =
          for value <- values,
              do: if(is_atom(value), do: value, else: elem(value, 0))

        message =
          case Enum.split_with(invalid_keys, &(&1 in keys)) do
            {_, [_ | _] = unknown} ->
              "unknown keys #{inspect(unknown)} in #{inspect(keyword)}, " <>
                "the allowed keys are: #{inspect(keys)}"

            {[_ | _] = known, _} ->
              "duplicate keys #{inspect(known)} in #{inspect(keyword)}"
          end

        raise ArgumentError, message
    end
  end






























  def get(keywords, key, default \\ nil) when is_list(keywords) and is_atom(key) do
    case :lists.keyfind(key, 1, keywords) do
      {^key, value} -> value
      false -> default
    end
  end


























  def get_lazy(keywords, key, fun)
      when is_list(keywords) and is_atom(key) and is_function(fun, 0) do
    case :lists.keyfind(key, 1, keywords) do
      {^key, value} -> value
      false -> fun.()
    end
  end









































  def get_and_update(keywords, key, fun)
      when is_list(keywords) and is_atom(key),
      do: get_and_update(keywords, [], key, fun)

  defp get_and_update([{key, current} | t], acc, key, fun) do
    case fun.(current) do
      {get, value} ->
        {get, :lists.reverse(acc, [{key, value} | t])}

      :pop ->
        {current, :lists.reverse(acc, t)}

      other ->
        raise "the given function must return a two-element tuple or :pop, got: #{inspect(other)}"
    end
  end

  defp get_and_update([{_, _} = h | t], acc, key, fun), do: get_and_update(t, [h | acc], key, fun)

  defp get_and_update([], acc, key, fun) do
    case fun.(nil) do
      {get, update} ->
        {get, [{key, update} | :lists.reverse(acc)]}

      :pop ->
        {nil, :lists.reverse(acc)}

      other ->
        raise "the given function must return a two-element tuple or :pop, got: #{inspect(other)}"
    end
  end

































  def get_and_update!(keywords, key, fun) do
    get_and_update!(keywords, key, fun, [])
  end

  defp get_and_update!([{key, value} | keywords], key, fun, acc) do
    case fun.(value) do
      {get, value} ->
        {get, :lists.reverse(acc, [{key, value} | delete(keywords, key)])}

      :pop ->
        {value, :lists.reverse(acc, keywords)}

      other ->
        raise "the given function must return a two-element tuple or :pop, got: #{inspect(other)}"
    end
  end

  defp get_and_update!([{_, _} = e | keywords], key, fun, acc) do
    get_and_update!(keywords, key, fun, [e | acc])
  end

  defp get_and_update!([], key, _fun, acc) when is_atom(key) do
    raise KeyError, key: key, term: acc
  end















  def fetch(keywords, key) when is_list(keywords) and is_atom(key) do
    case :lists.keyfind(key, 1, keywords) do
      {^key, value} -> {:ok, value}
      false -> :error
    end
  end















  def fetch!(keywords, key) when is_list(keywords) and is_atom(key) do
    case :lists.keyfind(key, 1, keywords) do
      {^key, value} -> value
      false -> raise KeyError, key: key, term: keywords
    end
  end















  def get_values(keywords, key) when is_list(keywords) and is_atom(key) do
    get_values(keywords, key, [])
  end

  defp get_values([{key, value} | tail], key, values), do: get_values(tail, key, [value | values])
  defp get_values([{_, _} | tail], key, values), do: get_values(tail, key, values)
  defp get_values([], _key, values), do: :lists.reverse(values)



















  def keys(keywords) when is_list(keywords) do
    :lists.map(
      fn
        {key, _} when is_atom(key) -> key
        element -> throw(element)
      end,
      keywords
    )
  catch
    element ->
      raise ArgumentError,
            "expected a keyword list, but an entry in the list is not a two-element tuple " <>
              "with an atom as its first element, got: #{inspect(element)}"
  end















  def values(keywords) when is_list(keywords) do
    :lists.map(fn {_, v} -> v end, keywords)
  end



  def delete(keywords, key, value) when is_list(keywords) and is_atom(key) do
    case :lists.keymember(key, 1, keywords) do
      true -> delete_key_value(keywords, key, value)
      _ -> keywords
    end
  end

  defp delete_key_value([{key, value} | tail], key, value) do
    delete_key_value(tail, key, value)
  end

  defp delete_key_value([{_, _} = pair | tail], key, value) do
    [pair | delete_key_value(tail, key, value)]
  end

  defp delete_key_value([], _key, _value) do
    []
  end




















  def delete(keywords, key) when is_list(keywords) and is_atom(key) do
    case :lists.keymember(key, 1, keywords) do
      true -> delete_key(keywords, key)
      _ -> keywords
    end
  end

  defp delete_key([{key, _} | tail], key), do: delete_key(tail, key)
  defp delete_key([{_, _} = pair | tail], key), do: [pair | delete_key(tail, key)]
  defp delete_key([], _key), do: []















  def delete_first(keywords, key) when is_list(keywords) and is_atom(key) do
    case :lists.keymember(key, 1, keywords) do
      true -> delete_first_key(keywords, key)
      _ -> keywords
    end
  end

  defp delete_first_key([{key, _} | tail], key) do
    tail
  end

  defp delete_first_key([{_, _} = pair | tail], key) do
    [pair | delete_first_key(tail, key)]
  end

  defp delete_first_key([], _key) do
    []
  end


















  def put(keywords, key, value) when is_list(keywords) and is_atom(key) do
    [{key, value} | delete(keywords, key)]
  end






















  def put_new_lazy(keywords, key, fun)
      when is_list(keywords) and is_atom(key) and is_function(fun, 0) do
    case :lists.keyfind(key, 1, keywords) do
      {^key, _} -> keywords
      false -> [{key, fun.()} | keywords]
    end
  end













  def put_new(keywords, key, value) when is_list(keywords) and is_atom(key) do
    case :lists.keyfind(key, 1, keywords) do
      {^key, _} -> keywords
      false -> [{key, value} | keywords]
    end
  end


















  def replace(keywords, key, value) when is_list(keywords) and is_atom(key) do
    do_replace(keywords, key, value)
  end

  defp do_replace([{key, _} | keywords], key, value) do
    [{key, value} | delete(keywords, key)]
  end

  defp do_replace([{_, _} = e | keywords], key, value) do
    [e | do_replace(keywords, key, value)]
  end

  defp do_replace([], _key, _value) do
    []
  end



















  def replace!(keywords, key, value) when is_list(keywords) and is_atom(key) do
    replace!(keywords, key, value, keywords)
  end

  defp replace!([{key, _} | keywords], key, value, _original) do
    [{key, value} | delete(keywords, key)]
  end

  defp replace!([{_, _} = e | keywords], key, value, original) do
    [e | replace!(keywords, key, value, original)]
  end

  defp replace!([], key, _value, original) do
    raise KeyError, key: key, term: original
  end























  def replace_lazy(keywords, key, fun)
      when is_list(keywords) and is_atom(key) and is_function(fun, 1) do
    do_replace_lazy(keywords, key, fun)
  end

  defp do_replace_lazy([{key, value} | keywords], key, fun) do
    [{key, fun.(value)} | delete(keywords, key)]
  end

  defp do_replace_lazy([{_, _} = e | keywords], key, fun) do
    [e | do_replace_lazy(keywords, key, fun)]
  end

  defp do_replace_lazy([], _key, _value), do: []
























  def equal?(left, right) when is_list(left) and is_list(right) do
    :lists.sort(left) === :lists.sort(right)
  end




















  def intersect(keyword1, keyword2, fun \\ fn _key, _v1, v2 -> v2 end)

  def intersect([{k, v1} | keyword1], keyword2, fun) do
    case :lists.keyfind(k, 1, keyword2) do
      {_, v2} -> [{k, fun.(k, v1, v2)} | intersect(keyword1, keyword2, fun)]
      false -> intersect(keyword1, keyword2, fun)
    end
  end

  def intersect([], _keyword2, _fun), do: []






















  def merge(keywords1, keywords2)

  def merge(keywords1, []) when is_list(keywords1), do: keywords1
  def merge([], keywords2) when is_list(keywords2), do: keywords2

  def merge(keywords1, keywords2) when is_list(keywords1) and is_list(keywords2) do
    if keyword?(keywords2) do
      fun = fn
        {key, _value} when is_atom(key) ->
          not has_key?(keywords2, key)

        _ ->
          raise ArgumentError,
                "expected a keyword list as the first argument, got: #{inspect(keywords1)}"
      end

      :lists.filter(fun, keywords1) ++ keywords2
    else
      raise ArgumentError,
            "expected a keyword list as the second argument, got: #{inspect(keywords2)}"
    end
  end




































  def merge(keywords1, keywords2, fun)
      when is_list(keywords1) and is_list(keywords2) and is_function(fun, 3) do
    if keyword?(keywords1) do
      do_merge(keywords2, [], keywords1, keywords1, fun, keywords2)
    else
      raise ArgumentError,
            "expected a keyword list as the first argument, got: #{inspect(keywords1)}"
    end
  end

  defp do_merge([{key, value2} | tail], acc, rest, original, fun, keywords2) when is_atom(key) do
    case :lists.keyfind(key, 1, original) do
      {^key, value1} ->
        acc = [{key, fun.(key, value1, value2)} | acc]
        original = :lists.keydelete(key, 1, original)
        do_merge(tail, acc, delete(rest, key), original, fun, keywords2)

      false ->
        do_merge(tail, [{key, value2} | acc], rest, original, fun, keywords2)
    end
  end

  defp do_merge([], acc, rest, _original, _fun, _keywords2) do
    rest ++ :lists.reverse(acc)
  end

  defp do_merge(_other, _acc, _rest, _original, _fun, keywords2) do
    raise ArgumentError,
          "expected a keyword list as the second argument, got: #{inspect(keywords2)}"
  end













  def has_key?(keywords, key) when is_list(keywords) and is_atom(key) do
    :lists.keymember(key, 1, keywords)
  end




















  def update!(keywords, key, fun)
      when is_list(keywords) and is_atom(key) and is_function(fun, 1) do
    update!(keywords, key, fun, keywords)
  end

  defp update!([{key, value} | keywords], key, fun, _original) do
    [{key, fun.(value)} | delete(keywords, key)]
  end

  defp update!([{_, _} = pair | keywords], key, fun, original) do
    [pair | update!(keywords, key, fun, original)]
  end

  defp update!([], key, _fun, original) do
    raise KeyError, key: key, term: original
  end






















  def update(keywords, key, default, fun)
      when is_list(keywords) and is_atom(key) and is_function(fun, 1) do
    update_guarded(keywords, key, default, fun)
  end

  defp update_guarded([{key, value} | keywords], key, _default, fun) do
    [{key, fun.(value)} | delete(keywords, key)]
  end

  defp update_guarded([{_, _} = pair | keywords], key, default, fun) do
    [pair | update_guarded(keywords, key, default, fun)]
  end

  defp update_guarded([], key, default, _fun) do
    [{key, default}]
  end




















  def split(keywords, keys) when is_list(keywords) and is_list(keys) do
    fun = fn {k, v}, {take, drop} ->
      case k in keys do
        true -> {[{k, v} | take], drop}
        false -> {take, [{k, v} | drop]}
      end
    end

    acc = {[], []}
    {take, drop} = :lists.foldl(fun, acc, keywords)
    {:lists.reverse(take), :lists.reverse(drop)}
  end




























  def split_with(keywords, fun) when is_list(keywords) and is_function(fun, 1) do
    fun = fn key_value_pair, {while_true, while_false} ->
      if fun.(key_value_pair) do
        {[key_value_pair | while_true], while_false}
      else
        {while_true, [key_value_pair | while_false]}
      end
    end

    :lists.foldr(fun, {[], []}, keywords)
  end
















  def take(keywords, keys) when is_list(keywords) and is_list(keys) do
    :lists.filter(fn {k, _} -> :lists.member(k, keys) end, keywords)
  end

















  def drop(keywords, keys) when is_list(keywords) and is_list(keys) do
    :lists.filter(fn {k, _} -> k not in keys end, keywords)
  end
























  def pop(keywords, key, default \\ nil) when is_list(keywords) and is_atom(key) do
    case fetch(keywords, key) do
      {:ok, value} -> {value, delete(keywords, key)}
      :error -> {default, keywords}
    end
  end




















  def pop!(keywords, key) when is_list(keywords) and is_atom(key) do
    case fetch(keywords, key) do
      {:ok, value} -> {value, delete(keywords, key)}
      :error -> raise KeyError, key: key, term: keywords
    end
  end























  def pop_values(keywords, key) when is_list(keywords) and is_atom(key) do
    pop_values(:lists.reverse(keywords), key, [], [])
  end

  defp pop_values([{key, value} | tail], key, values, acc),
    do: pop_values(tail, key, [value | values], acc)

  defp pop_values([{_, _} = pair | tail], key, values, acc),
    do: pop_values(tail, key, values, [pair | acc])

  defp pop_values([], _key, values, acc),
    do: {values, acc}























  def pop_lazy(keywords, key, fun)
      when is_list(keywords) and is_atom(key) and is_function(fun, 0) do
    case fetch(keywords, key) do
      {:ok, value} -> {value, delete(keywords, key)}
      :error -> {fun.(), keywords}
    end
  end



















  def pop_first(keywords, key, default \\ nil) when is_list(keywords) and is_atom(key) do
    case :lists.keytake(key, 1, keywords) do
      {:value, {^key, value}, rest} -> {value, rest}
      false -> {default, keywords}
    end
  end











  def to_list(keywords) when is_list(keywords) do
    keywords
  end



  def size(keywords) do
    length(keywords)
  end
















  def filter(keywords, fun) when is_list(keywords) and is_function(fun, 1) do
    do_filter(keywords, fun)
  end

  defp do_filter([], _fun), do: []

  defp do_filter([{_, _} = entry | entries], fun) do
    if fun.(entry) do
      [entry | do_filter(entries, fun)]
    else
      do_filter(entries, fun)
    end
  end















  def reject(keywords, fun) when is_list(keywords) and is_function(fun, 1) do
    do_reject(keywords, fun)
  end

  defp do_reject([], _fun), do: []

  defp do_reject([{_, _} = entry | entries], fun) do
    if fun.(entry) do
      do_reject(entries, fun)
    else
      [entry | do_reject(entries, fun)]
    end
  end



  def map(keywords, fun) when is_list(keywords) do
    Enum.map(keywords, fn {k, v} -> {k, fun.({k, v})} end)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/keyword.ex (docs and specs stripped;
# line numbers match the original).
