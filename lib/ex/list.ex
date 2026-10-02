defmodule List do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.


























































































































































  def delete(list, element)
  def delete([element | list], element), do: list
  def delete([other | list], element), do: [other | delete(list, element)]
  def delete([], _element), do: []

























  def duplicate(elem, n) do
    :lists.duplicate(n, elem)
  end
















  def flatten(list) do
    :lists.flatten(list)
  end



















  def flatten(list, tail) do
    :lists.flatten(list, tail)
  end


















  def foldl(list, acc, fun) when is_list(list) and is_function(fun) do
    :lists.foldl(fun, acc, list)
  end















  def foldr(list, acc, fun) when is_list(list) and is_function(fun) do
    :lists.foldr(fun, acc, list)
  end























  def first(list, default \\ nil)
  def first([], default), do: default
  def first([head | _], _default), do: head
























  def last(list, default \\ nil)
  def last([], default), do: default
  def last([head], _default), do: head
  def last([_ | tail], default), do: last(tail, default)


























  def keyfind(list, key, position, default \\ nil) when is_integer(position) do
    :lists.keyfind(key, position + 1, list) || default
  end



























  def keyfind!(list, key, position) when is_integer(position) do
    :lists.keyfind(key, position + 1, list) ||
      raise KeyError,
        key: key,
        term: list,
        message:
          "key #{inspect(key)} at position #{inspect(position)} not found in: #{inspect(list)}"
  end
























  def keymember?(list, key, position) when is_integer(position) do
    :lists.keymember(key, position + 1, list)
  end




















  def keyreplace(list, key, position, new_tuple) when is_integer(position) do
    :lists.keyreplace(key, position + 1, list, new_tuple)
  end

























































  def keysort(list, position, sorter \\ :asc)

  def keysort(list, position, :asc) when is_list(list) and is_integer(position) do
    :lists.keysort(position + 1, list)
  end

  def keysort(list, position, sorter) when is_list(list) and is_integer(position) do
    :lists.sort(keysort_fun(sorter, position + 1), list)
  end

  defp keysort_fun(sorter, position) when is_function(sorter, 2),
    do: &sorter.(:erlang.element(position, &1), :erlang.element(position, &2))

  defp keysort_fun(:desc, position),
    do: &(:erlang.element(position, &1) >= :erlang.element(position, &2))

  defp keysort_fun(module, position) when is_atom(module),
    do: &(module.compare(:erlang.element(position, &1), :erlang.element(position, &2)) != :gt)

  defp keysort_fun({:asc, module}, position) when is_atom(module),
    do: &(module.compare(:erlang.element(position, &1), :erlang.element(position, &2)) != :gt)

  defp keysort_fun({:desc, module}, position) when is_atom(module),
    do: &(module.compare(:erlang.element(position, &1), :erlang.element(position, &2)) != :lt)






















  def keystore(list, key, position, new_tuple) when is_integer(position) do
    :lists.keystore(key, position + 1, list, new_tuple)
  end
























  def keydelete(list, key, position) when is_integer(position) do
    :lists.keydelete(key, position + 1, list)
  end


























  def keytake(list, key, position) when is_integer(position) do
    case :lists.keytake(key, position + 1, list) do
      {:value, element, list} -> {element, list}
      false -> nil
    end
  end




















  def wrap(term)

  def wrap(list) when is_list(list) do
    list
  end

  def wrap(nil) do
    []
  end

  def wrap(other) do
    [other]
  end


  # We keep the old implementation because it also supported lists
  # of tuples, even though this was not included in its @spec.
  def zip([]), do: []
  def zip(list_of_lists) when is_list(list_of_lists), do: do_zip(list_of_lists, [])

  defp do_zip(list, acc) do
    converter = fn x, acc -> do_zip_each(to_list(x), acc) end

    case :lists.mapfoldl(converter, [], list) do
      {_, nil} ->
        :lists.reverse(acc)

      {mlist, heads} ->
        do_zip(mlist, [to_tuple(:lists.reverse(heads)) | acc])
    end
  end

  defp do_zip_each(_, nil) do
    {nil, nil}
  end

  defp do_zip_each([head | tail], acc) do
    {tail, [head | acc]}
  end

  defp do_zip_each([], _) do
    {nil, nil}
  end

  defp to_list(tuple) when is_tuple(tuple), do: Tuple.to_list(tuple)
  defp to_list(list) when is_list(list), do: list














































  def ascii_printable?(list, limit \\ :infinity)
      when is_list(list) and (limit == :infinity or (is_integer(limit) and limit >= 0)) do
    ascii_printable_guarded?(list, limit)
  end

  defp ascii_printable_guarded?(_, 0) do
    true
  end

  defp ascii_printable_guarded?([char | rest], counter)
       # 7..13 is the range '\a\b\t\n\v\f\r'. 32..126 are ASCII printables.
       when is_integer(char) and
              ((char >= 7 and char <= 13) or char == ?\e or (char >= 32 and char <= 126)) do
    ascii_printable_guarded?(rest, decrement(counter))
  end

  defp ascii_printable_guarded?([], _counter), do: true
  defp ascii_printable_guarded?(_, _counter), do: false


  defp decrement(:infinity), do: :infinity
  defp decrement(counter), do: counter - 1















  def improper?(list) when is_list(list) and length(list) >= 0, do: false
  def improper?(list) when is_list(list), do: true























  def insert_at(list, index, value) when is_list(list) and is_integer(index) do
    case index do
      -1 ->
        list ++ [value]

      _ when index < 0 ->
        case length(list) + index + 1 do
          index when index < 0 -> [value | list]
          index -> do_insert_at(list, index, value)
        end

      _ ->
        do_insert_at(list, index, value)
    end
  end























  def replace_at(list, index, value) when is_list(list) and is_integer(index) do
    if index < 0 do
      case length(list) + index do
        index when index < 0 -> list
        index -> do_replace_at(list, index, value)
      end
    else
      do_replace_at(list, index, value)
    end
  end























  def update_at(list, index, fun) when is_list(list) and is_function(fun) and is_integer(index) do
    if index < 0 do
      case length(list) + index do
        index when index < 0 -> list
        index -> do_update_at(list, index, fun)
      end
    else
      do_update_at(list, index, fun)
    end
  end




















  def delete_at(list, index) when is_integer(index) do
    elem(pop_at(list, index), 1)
  end





















  def pop_at(list, index, default \\ nil) when is_integer(index) do
    if index < 0 do
      do_pop_at(list, length(list) + index, default, [])
    else
      do_pop_at(list, index, default, [])
    end
  end

























  def starts_with?(list, prefix)

  def starts_with?([head | tail], [head | prefix_tail]), do: starts_with?(tail, prefix_tail)
  def starts_with?(list, []) when is_list(list), do: true
  def starts_with?(list, [_ | _]) when is_list(list), do: false

























  def ends_with?(list, suffix) do
    :lists.suffix(suffix, list)
  end



















  def to_atom(charlist) do
    :erlang.list_to_atom(charlist)
  end






























  def to_existing_atom(charlist) do
    :erlang.list_to_existing_atom(charlist)
  end













  def to_float(charlist) do
    :erlang.list_to_float(charlist)
  end













  def to_integer(charlist) do
    :erlang.list_to_integer(charlist)
  end















  def to_integer(charlist, base) do
    :erlang.list_to_integer(charlist, base)
  end













  def to_tuple(list) do
    :erlang.list_to_tuple(list)
  end
































  def to_string(list) when is_list(list) do
    try do
      :unicode.characters_to_binary(list)
    rescue
      ArgumentError ->
        raise ArgumentError, """
        cannot convert the given list to a string.

        To be converted to a string, a list must either be empty or only
        contain the following elements:

          * strings
          * integers representing Unicode code points
          * a list containing one of these three elements

        Please check the given list or call inspect/1 to get the list representation, got:

        #{inspect(list)}
        """
    else
      result when is_binary(result) ->
        result

      {:error, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :invalid

      {:incomplete, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :incomplete
    end
  end























  def to_charlist(list) when is_list(list) do
    try do
      :unicode.characters_to_list(list)
    rescue
      ArgumentError ->
        raise ArgumentError, """
        cannot convert the given list to a charlist.

        To be converted to a charlist, a list must contain only:

          * strings
          * integers representing Unicode code points
          * or a list containing one of these three elements

        Please check the given list or call inspect/1 to get the list representation, got:

        #{inspect(list)}
        """
    else
      result when is_list(result) ->
        result

      {:error, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :invalid

      {:incomplete, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :incomplete
    end
  end
























  def myers_difference(list1, list2) when is_list(list1) and is_list(list2) do
    myers_difference_with_diff_script(list1, list2, nil)
  end


















  def myers_difference(list1, list2, diff_script)
      when is_list(list1) and is_list(list2) and is_function(diff_script) do
    myers_difference_with_diff_script(list1, list2, diff_script)
  end

  defp myers_difference_with_diff_script(list1, list2, diff_script) do
    path = {0, list1, list2, []}
    find_script(0, length(list1) + length(list2), [path], diff_script)
  end

  defp find_script(envelope, max, paths, diff_script) do
    case each_diagonal(-envelope, envelope, paths, [], diff_script) do
      {:done, edits} -> compact_reverse(edits, [])
      {:next, paths} -> find_script(envelope + 1, max, paths, diff_script)
    end
  end

  defp compact_reverse([], acc), do: acc

  defp compact_reverse([{:diff, _} = fragment | rest], acc) do
    compact_reverse(rest, [fragment | acc])
  end

  defp compact_reverse([{kind, elem} | rest], [{kind, result} | acc]) do
    compact_reverse(rest, [{kind, [elem | result]} | acc])
  end

  defp compact_reverse(rest, [{:eq, elem}, {:ins, elem}, {:eq, other} | acc]) do
    compact_reverse(rest, [{:ins, elem}, {:eq, elem ++ other} | acc])
  end

  defp compact_reverse([{kind, elem} | rest], acc) do
    compact_reverse(rest, [{kind, [elem]} | acc])
  end

  defp each_diagonal(diag, limit, _paths, next_paths, _diff_script) when diag > limit do
    {:next, :lists.reverse(next_paths)}
  end

  defp each_diagonal(diag, limit, paths, next_paths, diff_script) do
    {path, rest} = proceed_path(diag, limit, paths, diff_script)

    case follow_snake(path) do
      {:cont, path} -> each_diagonal(diag + 2, limit, rest, [path | next_paths], diff_script)
      {:done, edits} -> {:done, edits}
    end
  end

  defp proceed_path(0, 0, [path], _diff_script), do: {path, []}

  defp proceed_path(diag, limit, [path | _] = paths, diff_script) when diag == -limit do
    {move_down(path, diff_script), paths}
  end

  defp proceed_path(diag, limit, [path], diff_script) when diag == limit do
    {move_right(path, diff_script), []}
  end

  defp proceed_path(_diag, _limit, [path1, path2 | rest], diff_script) do
    if elem(path1, 0) > elem(path2, 0) do
      {move_right(path1, diff_script), [path2 | rest]}
    else
      {move_down(path2, diff_script), [path2 | rest]}
    end
  end

  defp move_right({y, [elem1 | rest1] = list1, [elem2 | rest2], edits}, diff_script)
       when diff_script != nil do
    if diff = diff_script.(elem1, elem2) do
      {y + 1, rest1, rest2, [{:diff, diff} | edits]}
    else
      {y, list1, rest2, [{:ins, elem2} | edits]}
    end
  end

  defp move_right({y, list1, [elem | rest], edits}, _diff_script) do
    {y, list1, rest, [{:ins, elem} | edits]}
  end

  defp move_right({y, list1, [], edits}, _diff_script) do
    {y, list1, [], edits}
  end

  defp move_down({y, [elem1 | rest1], [elem2 | rest2] = list2, edits}, diff_script)
       when diff_script != nil do
    if diff = diff_script.(elem1, elem2) do
      {y + 1, rest1, rest2, [{:diff, diff} | edits]}
    else
      {y + 1, rest1, list2, [{:del, elem1} | edits]}
    end
  end

  defp move_down({y, [elem | rest], list2, edits}, _diff_script) do
    {y + 1, rest, list2, [{:del, elem} | edits]}
  end

  defp move_down({y, [], list2, edits}, _diff_script) do
    {y + 1, [], list2, edits}
  end

  defp follow_snake({y, [elem | rest1], [elem | rest2], edits}) do
    follow_snake({y + 1, rest1, rest2, [{:eq, elem} | edits]})
  end

  defp follow_snake({_y, [], [], edits}) do
    {:done, edits}
  end

  defp follow_snake(path) do
    {:cont, path}
  end

  ## Helpers

  # replace_at

  defp do_replace_at([], _index, _value) do
    []
  end

  defp do_replace_at([_old | rest], 0, value) do
    [value | rest]
  end

  defp do_replace_at([head | tail], index, value) do
    [head | do_replace_at(tail, index - 1, value)]
  end

  # insert_at

  defp do_insert_at([], _index, value) do
    [value]
  end

  defp do_insert_at(list, 0, value) do
    [value | list]
  end

  defp do_insert_at([head | tail], index, value) do
    [head | do_insert_at(tail, index - 1, value)]
  end

  # update_at

  defp do_update_at([value | list], 0, fun) do
    [fun.(value) | list]
  end

  defp do_update_at([head | tail], index, fun) do
    [head | do_update_at(tail, index - 1, fun)]
  end

  defp do_update_at([], _index, _fun) do
    []
  end

  # pop_at

  defp do_pop_at([], _index, default, acc) do
    {default, :lists.reverse(acc)}
  end

  defp do_pop_at([head | tail], 0, _default, acc) do
    {head, :lists.reverse(acc, tail)}
  end

  defp do_pop_at([head | tail], index, default, acc) do
    do_pop_at(tail, index - 1, default, [head | acc])
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/list.ex (docs and specs stripped;
# line numbers match the original).
