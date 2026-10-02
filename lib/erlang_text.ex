# :lists, :string, :unicode (ports of OTP 27.3 stdlib)
# Modified for Tonic; Erlang/OTP 27.3 source/port. Apache-2.0; see licenses/sources.json and notice.
# Intrinsics resolved by the compiler before module functions (not redefined
# here): :lists.reverse/1,2, :lists.member/2, :lists.keyfind/3, :lists.sort/1,
# :lists.last/1, :lists.seq/3, :unicode.characters_to_binary/1.

defmodule :lists do
  # ---------------------------------------------------------------------------
  # Basic list functions

  def append(l1, l2), do: l1 ++ l2

  def append([e]), do: e
  def append([h | t]), do: h ++ append(t)
  def append([]), do: []

  def subtract(l1, l2), do: l1 -- l2

  def nth(1, [h | _]), do: h
  def nth(n, [_ | _] = l) when is_integer(n) and n > 1, do: nth_1(n, l)

  defp nth_1(1, [h | _]), do: h
  defp nth_1(n, [_ | t]), do: nth_1(n - 1, t)

  def nthtail(0, []), do: []
  def nthtail(0, [_ | _] = l), do: l
  def nthtail(1, [_ | t]), do: t
  def nthtail(n, [_ | _] = l) when is_integer(n) and n > 1, do: nthtail_1(n, l)

  defp nthtail_1(1, [_ | t]), do: t
  defp nthtail_1(n, [_ | t]), do: nthtail_1(n - 1, t)

  def prefix([x | pre_tail], [x | tail]), do: prefix(pre_tail, tail)
  def prefix([], list) when is_list(list), do: true
  def prefix([_ | _], list) when is_list(list), do: false

  def suffix(suffix, list) do
    delta = :erlang.length(list) - :erlang.length(suffix)
    delta >= 0 and nthtail(delta, list) === suffix
  end

  def droplast([_t]), do: []
  def droplast([h | t]), do: [h | droplast(t)]

  def seq(first, last)
      when is_integer(first) and is_integer(last) and first - 1 <= last do
    seq_loop(last - first + 1, last, [])
  end

  defp seq_loop(n, x, l) when n >= 4, do: seq_loop(n - 4, x - 4, [x - 3, x - 2, x - 1, x | l])
  defp seq_loop(n, x, l) when n >= 2, do: seq_loop(n - 2, x - 2, [x - 1, x | l])
  defp seq_loop(1, x, l), do: [x | l]
  defp seq_loop(0, _, l), do: l

  def sum(l), do: sum(l, 0)

  defp sum([h | t], acc), do: sum(t, acc + h)
  defp sum([], acc), do: acc

  def duplicate(n, x) when is_integer(n) and n >= 0, do: duplicate_1(n, x, [])

  defp duplicate_1(0, _, l), do: l
  defp duplicate_1(n, x, l), do: duplicate_1(n - 1, x, [x | l])

  def min([h | t]), do: min_1(t, h)

  defp min_1([h | t], m) when h < m, do: min_1(t, h)
  defp min_1([_ | t], m), do: min_1(t, m)
  defp min_1([], m), do: m

  def max([h | t]), do: max_1(t, h)

  defp max_1([h | t], m) when h > m, do: max_1(t, h)
  defp max_1([_ | t], m), do: max_1(t, m)
  defp max_1([], m), do: m

  def sublist(list, 1, l) when is_list(list) and is_integer(l) and l >= 0, do: sublist(list, l)
  def sublist([], s, _l) when is_integer(s) and s >= 2, do: []
  def sublist([_h | t], s, l) when is_integer(s) and s >= 2, do: sublist(t, s - 1, l)

  def sublist(list, l) when is_integer(l) and is_list(list), do: sublist_2(list, l)

  defp sublist_2([h | t], l) when l > 0, do: [h | sublist_2(t, l - 1)]
  defp sublist_2(_, 0), do: []
  defp sublist_2(list, l) when is_list(list) and l > 0, do: []

  def delete(item, [item | rest]), do: rest
  def delete(item, [h | rest]), do: [h | delete(item, rest)]
  def delete(_, []), do: []

  # ---------------------------------------------------------------------------
  # zip / unzip

  def zip(xs, ys), do: zip(xs, ys, :fail)

  def zip([x | xs], [y | ys], how), do: [{x, y} | zip(xs, ys, how)]
  def zip([], [], :fail), do: []
  def zip([], [], :trim), do: []
  def zip([], [], {:pad, {_, _}}), do: []
  def zip([_ | _], [], :trim), do: []
  def zip([], [_ | _], :trim), do: []
  def zip([], [_ | _] = ys, {:pad, {x, _}}), do: for(y <- ys, do: {x, y})
  def zip([_ | _] = xs, [], {:pad, {_, y}}), do: for(x <- xs, do: {x, y})

  def unzip(ts), do: unzip(ts, [], [])

  defp unzip([{x, y} | ts], xs, ys), do: unzip(ts, [x | xs], [y | ys])
  defp unzip([], xs, ys), do: {:lists.reverse(xs), :lists.reverse(ys)}

  def zip3(xs, ys, zs), do: zip3(xs, ys, zs, :fail)

  def zip3([x | xs], [y | ys], [z | zs], how), do: [{x, y, z} | zip3(xs, ys, zs, how)]
  def zip3([], [], [], :fail), do: []
  def zip3([], [], [], :trim), do: []
  def zip3(xs, ys, zs, :trim) when is_list(xs) and is_list(ys) and is_list(zs), do: []
  def zip3([], [], [], {:pad, {_, _, _}}), do: []
  def zip3([], [], [_ | _] = zs, {:pad, {x, y, _}}), do: for(z <- zs, do: {x, y, z})
  def zip3([], [_ | _] = ys, [], {:pad, {x, _, z}}), do: for(y <- ys, do: {x, y, z})
  def zip3([_ | _] = xs, [], [], {:pad, {_, y, z}}), do: for(x <- xs, do: {x, y, z})
  def zip3([], [y | ys], [z | zs], {:pad, {x, _, _}} = how), do: [{x, y, z} | zip3([], ys, zs, how)]
  def zip3([x | xs], [], [z | zs], {:pad, {_, y, _}} = how), do: [{x, y, z} | zip3(xs, [], zs, how)]
  def zip3([x | xs], [y | ys], [], {:pad, {_, _, z}} = how), do: [{x, y, z} | zip3(xs, ys, [], how)]

  def unzip3(ts), do: unzip3(ts, [], [], [])

  defp unzip3([{x, y, z} | ts], xs, ys, zs), do: unzip3(ts, [x | xs], [y | ys], [z | zs])
  defp unzip3([], xs, ys, zs), do: {:lists.reverse(xs), :lists.reverse(ys), :lists.reverse(zs)}

  def zipwith(f, xs, ys), do: zipwith(f, xs, ys, :fail)

  def zipwith(f, [x | xs], [y | ys], how), do: [f.(x, y) | zipwith(f, xs, ys, how)]
  def zipwith(f, [], [], :fail) when is_function(f, 2), do: []
  def zipwith(f, [], [], :trim) when is_function(f, 2), do: []
  def zipwith(f, [], [], {:pad, {_, _}}) when is_function(f, 2), do: []
  def zipwith(f, [_ | _], [], :trim) when is_function(f, 2), do: []
  def zipwith(f, [], [_ | _], :trim) when is_function(f, 2), do: []
  def zipwith(f, [], [_ | _] = ys, {:pad, {x, _}}), do: for(y <- ys, do: f.(x, y))
  def zipwith(f, [_ | _] = xs, [], {:pad, {_, y}}), do: for(x <- xs, do: f.(x, y))

  def zipwith3(f, xs, ys, zs), do: zipwith3(f, xs, ys, zs, :fail)

  def zipwith3(f, [x | xs], [y | ys], [z | zs], how), do: [f.(x, y, z) | zipwith3(f, xs, ys, zs, how)]
  def zipwith3(f, [], [], [], :fail) when is_function(f, 3), do: []
  def zipwith3(f, [], [], [], :trim) when is_function(f, 3), do: []

  def zipwith3(f, xs, ys, zs, :trim)
      when is_function(f, 3) and is_list(xs) and is_list(ys) and is_list(zs),
      do: []

  def zipwith3(f, [], [], [], {:pad, {_, _, _}}) when is_function(f, 3), do: []
  def zipwith3(f, [], [], [_ | _] = zs, {:pad, {x, y, _}}), do: for(z <- zs, do: f.(x, y, z))
  def zipwith3(f, [], [_ | _] = ys, [], {:pad, {x, _, z}}), do: for(y <- ys, do: f.(x, y, z))
  def zipwith3(f, [_ | _] = xs, [], [], {:pad, {_, y, z}}), do: for(x <- xs, do: f.(x, y, z))

  def zipwith3(f, [], [y | ys], [z | zs], {:pad, {x, _, _}} = how),
    do: [f.(x, y, z) | zipwith3(f, [], ys, zs, how)]

  def zipwith3(f, [x | xs], [], [z | zs], {:pad, {_, y, _}} = how),
    do: [f.(x, y, z) | zipwith3(f, xs, [], zs, how)]

  def zipwith3(f, [x | xs], [y | ys], [], {:pad, {_, _, z}} = how),
    do: [f.(x, y, z) | zipwith3(f, xs, ys, [], how)]

  # ---------------------------------------------------------------------------
  # concat / flatten

  def concat(list), do: flatmap(&thing_to_list/1, list)

  defp thing_to_list(x) when is_integer(x), do: :erlang.integer_to_list(x)
  defp thing_to_list(x) when is_float(x), do: :erlang.binary_to_list(:erlang.float_to_binary(x, [{:scientific, 20}]))
  defp thing_to_list(x) when is_atom(x), do: :erlang.atom_to_list(x)
  defp thing_to_list(x) when is_list(x), do: x

  def flatten(list) when is_list(list), do: do_flatten(list, [])

  def flatten(list, tail) when is_list(list) and is_list(tail), do: do_flatten(list, tail)

  defp do_flatten([h | t], tail) when is_list(h), do: do_flatten(h, do_flatten(t, tail))
  defp do_flatten([h | t], tail), do: [h | do_flatten(t, tail)]
  defp do_flatten([], tail), do: tail

  def flatlength(list), do: flatlength(list, 0)

  defp flatlength([h | t], l) when is_list(h), do: flatlength(h, flatlength(t, l))
  defp flatlength([_ | t], l), do: flatlength(t, l + 1)
  defp flatlength([], l), do: l

  # ---------------------------------------------------------------------------
  # Key functions

  def keymember(key, n, list) when is_integer(n) and n > 0 do
    :lists.keyfind(key, n, list) != false
  end

  def keysearch(key, n, list) when is_integer(n) and n > 0 do
    case :lists.keyfind(key, n, list) do
      false -> false
      tuple -> {:value, tuple}
    end
  end

  def keydelete(k, n, l) when is_integer(n) and n > 0, do: keydelete3(k, n, l)

  defp keydelete3(key, n, [h | t]) do
    if is_tuple(h) and tuple_size(h) >= n and :erlang.element(n, h) == key do
      t
    else
      [h | keydelete3(key, n, t)]
    end
  end

  defp keydelete3(_, _, []), do: []

  def keyreplace(k, n, l, new) when is_integer(n) and n > 0 and is_tuple(new),
    do: keyreplace3(k, n, l, new)

  defp keyreplace3(key, pos, [tup | tail], new) do
    if is_tuple(tup) and tuple_size(tup) >= pos and :erlang.element(pos, tup) == key do
      [new | tail]
    else
      [tup | keyreplace3(key, pos, tail, new)]
    end
  end

  defp keyreplace3(_, _, [], _), do: []

  def keytake(key, n, l) when is_integer(n) and n > 0, do: keytake(key, n, l, [])

  defp keytake(key, n, [h | t], l) do
    if is_tuple(h) and tuple_size(h) >= n and :erlang.element(n, h) == key do
      {:value, h, :lists.reverse(l, t)}
    else
      keytake(key, n, t, [h | l])
    end
  end

  defp keytake(_k, _n, [], _l), do: false

  def keystore(k, n, l, new) when is_integer(n) and n > 0 and is_tuple(new),
    do: keystore2(k, n, l, new)

  defp keystore2(key, n, [h | t], new) do
    if is_tuple(h) and tuple_size(h) >= n and :erlang.element(n, h) == key do
      [new | t]
    else
      [h | keystore2(key, n, t, new)]
    end
  end

  defp keystore2(_key, _n, [], new), do: [new]

  def keymap(fun, index, [tup | tail]) do
    [:erlang.setelement(index, tup, fun.(:erlang.element(index, tup))) | keymap(fun, index, tail)]
  end

  def keymap(fun, index, []) when is_integer(index) and index >= 1 and is_function(fun, 1), do: []

  # ---------------------------------------------------------------------------
  # Sorting and merging. Stable merge sorts with the same observable
  # semantics as OTP's (ties keep original order; merges prefer the first
  # list; the "u" variants keep the first of equal elements).

  def keysort(i, l) when is_integer(i) and i > 0 do
    case l do
      [] -> l
      [_] -> l
      _ -> msort(l, fn x, y -> :erlang.element(i, x) <= :erlang.element(i, y) end)
    end
  end

  def ukeysort(i, l) when is_integer(i) and i > 0 do
    case l do
      [] -> l
      [_] -> l
      _ ->
        sorted = msort(l, fn x, y -> :erlang.element(i, x) <= :erlang.element(i, y) end)
        dedup(sorted, fn a, b -> :erlang.element(i, a) == :erlang.element(i, b) end)
    end
  end

  def keymerge(index, l1, l2) when is_integer(index) and index > 0 do
    merge_by(l1, l2, fn a, b -> :erlang.element(index, a) <= :erlang.element(index, b) end)
  end

  def ukeymerge(index, l1, l2) when is_integer(index) and index > 0 do
    umerge_by(l1, l2,
      fn a, b -> :erlang.element(index, a) <= :erlang.element(index, b) end,
      fn h2, hdm -> :erlang.element(index, h2) == :erlang.element(index, hdm) end)
  end

  def rkeymerge(index, l1, l2) when is_integer(index) and index > 0 do
    rmerge_by(l1, l2, fn a, b -> :erlang.element(index, a) <= :erlang.element(index, b) end)
  end

  def rukeymerge(index, l1, l2) when is_integer(index) and index > 0 do
    rumerge_by(l1, l2,
      fn a, b -> :erlang.element(index, a) <= :erlang.element(index, b) end,
      fn a, b -> :erlang.element(index, a) == :erlang.element(index, b) end)
  end

  def sort(fun, []) when is_function(fun, 2), do: []
  def sort(fun, [_] = l) when is_function(fun, 2), do: l
  def sort(fun, [_, _ | _] = l) when is_function(fun, 2), do: msort(l, fun)

  def usort([]), do: []
  def usort([_] = l), do: l
  def usort(l) when is_list(l), do: dedup(msort(l, fn a, b -> a <= b end), fn a, b -> a == b end)

  def usort(fun, [_] = l) when is_function(fun, 2), do: l
  def usort(fun, [] = l) when is_function(fun, 2), do: l

  def usort(fun, [_ | _] = l) when is_function(fun, 2) do
    dedup(msort(l, fun), fn a, b -> fun.(a, b) and fun.(b, a) end)
  end

  def merge(l) when is_list(l), do: mergel(l)

  defp mergel([]), do: []
  defp mergel([l]) when is_list(l), do: l
  defp mergel([l | ls]) when is_list(l), do: merge(l, mergel(ls))

  def merge(l1, l2) when is_list(l1) and is_list(l2), do: merge_by(l1, l2, fn a, b -> a <= b end)

  def merge(fun, l1, l2) when is_function(fun, 2), do: merge_by(l1, l2, fun)

  def merge3(l1, l2, l3), do: merge(l1, merge(l2, l3))

  def rmerge(l1, l2), do: rmerge_by(l1, l2, fn a, b -> a <= b end)
  def rmerge(fun, l1, l2) when is_function(fun, 2), do: rmerge_by(l1, l2, fun)
  def rmerge3(l1, l2, l3), do: rmerge(l1, rmerge(l2, l3))

  def umerge(l) when is_list(l), do: umergel(l)

  defp umergel([]), do: []
  defp umergel([l]) when is_list(l), do: l
  defp umergel([l | ls]) when is_list(l), do: umerge(l, umergel(ls))

  def umerge(l1, l2) when is_list(l1) and is_list(l2),
    do: umerge_by(l1, l2, fn a, b -> a <= b end, fn a, b -> a == b end)

  def umerge(fun, l1, l2) when is_function(fun, 2),
    do: umerge_by(l1, l2, fun, fn h2, hdm -> fun.(h2, hdm) end)

  def umerge3(l1, l2, l3), do: umerge(l1, umerge(l2, l3))

  def rumerge(l1, l2), do: rumerge_by(l1, l2, fn a, b -> a <= b end, fn a, b -> a == b end)

  def rumerge(fun, l1, l2) when is_function(fun, 2),
    do: rumerge_by(l1, l2, fun, fn h1, h2 -> fun.(h2, h1) end)

  def rumerge3(l1, l2, l3), do: rumerge(l1, rumerge(l2, l3))

  # Stable top-down merge sort; `le.(a, b)` true means a may precede b.
  defp msort(l, le) do
    n = :erlang.length(l)
    {sorted, _} = msort_n(l, n, le)
    sorted
  end

  defp msort_n([x | rest], 1, _le), do: {[x], rest}
  defp msort_n([x, y | rest], 2, le) do
    if le.(x, y), do: {[x, y], rest}, else: {[y, x], rest}
  end
  defp msort_n(l, n, le) do
    h = div(n, 2)
    {a, rest1} = msort_n(l, h, le)
    {b, rest2} = msort_n(rest1, n - h, le)
    {merge_by(a, b, le), rest2}
  end

  defp merge_by(l1, l2, le), do: merge_by(l1, l2, le, [])

  defp merge_by([h1 | t1] = l1, [h2 | t2] = l2, le, acc) do
    if le.(h1, h2) do
      merge_by(t1, l2, le, [h1 | acc])
    else
      merge_by(l1, t2, le, [h2 | acc])
    end
  end

  defp merge_by([], l2, _le, acc), do: :lists.reverse(acc, l2)
  defp merge_by(l1, [], _le, acc), do: :lists.reverse(acc, l1)

  # Unique merge: ties keep the element of the first list.
  # `eq.(h2, hdm)` tells whether h2 equals the last element taken from l1.
  defp umerge_by([_ | _] = l1, [_ | _] = l2, le, eq), do: umerge_by(l1, l2, le, eq, :none, [])
  defp umerge_by([_ | _] = l1, [], _le, _eq), do: l1
  defp umerge_by([], l2, _le, _eq) when is_list(l2), do: l2

  defp umerge_by([h1 | t1] = l1, [h2 | t2] = l2, le, eq, hdm, acc) do
    if le.(h1, h2) do
      umerge_by(t1, l2, le, eq, {:some, h1}, [h1 | acc])
    else
      case hdm do
        {:some, m} ->
          if eq.(h2, m) do
            umerge_by(l1, t2, le, eq, :none, acc)
          else
            umerge_by(l1, t2, le, eq, :none, [h2 | acc])
          end
        :none ->
          umerge_by(l1, t2, le, eq, :none, [h2 | acc])
      end
    end
  end

  defp umerge_by([], [h2 | t2], _le, eq, hdm, acc) do
    case hdm do
      {:some, m} ->
        if eq.(h2, m), do: :lists.reverse(acc, t2), else: :lists.reverse(acc, [h2 | t2])
      :none ->
        :lists.reverse(acc, [h2 | t2])
    end
  end

  defp umerge_by(l1, [], _le, _eq, _hdm, acc), do: :lists.reverse(acc, l1)

  # Merge of two lists sorted in descending order (used by OTP internally).
  defp rmerge_by(l1, l2, le), do: rmerge_by(l1, l2, le, [])

  defp rmerge_by([h1 | t1] = l1, [h2 | t2] = l2, le, acc) do
    if le.(h1, h2) do
      rmerge_by(l1, t2, le, [h2 | acc])
    else
      rmerge_by(t1, l2, le, [h1 | acc])
    end
  end

  defp rmerge_by([], l2, _le, acc), do: :lists.reverse(acc, l2)
  defp rmerge_by(l1, [], _le, acc), do: :lists.reverse(acc, l1)

  defp rumerge_by(l1, l2, le, eq), do: rumerge_by(l1, l2, le, eq, [])

  defp rumerge_by([h1 | t1] = l1, [h2 | t2] = l2, le, eq, acc) do
    cond do
      not le.(h1, h2) -> rumerge_by(t1, l2, le, eq, [h1 | acc])
      eq.(h1, h2) -> rumerge_by(l1, t2, le, eq, acc)
      true -> rumerge_by(l1, t2, le, eq, [h2 | acc])
    end
  end

  defp rumerge_by([], l2, _le, _eq, acc), do: :lists.reverse(acc, l2)
  defp rumerge_by(l1, [], _le, _eq, acc), do: :lists.reverse(acc, l1)

  # Remove consecutive elements equal (per eq) to the last kept one.
  defp dedup([], _eq), do: []
  defp dedup([x | rest], eq), do: dedup(rest, x, eq, [x])

  defp dedup([y | rest], last, eq, acc) do
    if eq.(last, y), do: dedup(rest, last, eq, acc), else: dedup(rest, y, eq, [y | acc])
  end

  defp dedup([], _last, _eq, acc), do: :lists.reverse(acc)

  # ---------------------------------------------------------------------------
  # enumerate

  def enumerate(list1), do: enumerate(1, 1, list1)

  def enumerate(index, list1), do: enumerate(index, 1, list1)

  def enumerate(index, step, list1) when is_integer(index) and is_integer(step),
    do: enumerate_1(index, step, list1)

  defp enumerate_1(index, step, [h | t]), do: [{index, h} | enumerate_1(index + step, step, t)]
  defp enumerate_1(_index, _step, []), do: []

  # ---------------------------------------------------------------------------
  # Higher-order functions

  def all(pred, list) when is_function(pred, 1), do: all_1(pred, list)

  defp all_1(pred, [hd | tail]) do
    case pred.(hd) do
      true -> all_1(pred, tail)
      false -> false
    end
  end

  defp all_1(_pred, []), do: true

  def any(pred, list) when is_function(pred, 1), do: any_1(pred, list)

  defp any_1(pred, [hd | tail]) do
    case pred.(hd) do
      true -> true
      false -> any_1(pred, tail)
    end
  end

  defp any_1(_pred, []), do: false

  def map(f, list) when is_function(f, 1), do: map_1(f, list)

  defp map_1(f, [hd | tail]), do: [f.(hd) | map_1(f, tail)]
  defp map_1(_f, []), do: []

  def flatmap(f, list) when is_function(f, 1), do: flatmap_1(f, list)

  defp flatmap_1(f, [hd | tail]), do: f.(hd) ++ flatmap_1(f, tail)
  defp flatmap_1(_f, []), do: []

  def foldl(f, accu, list) when is_function(f, 2), do: foldl_1(f, accu, list)

  defp foldl_1(f, accu, [hd | tail]), do: foldl_1(f, f.(hd, accu), tail)
  defp foldl_1(_f, accu, []), do: accu

  def foldr(f, accu, list) when is_function(f, 2), do: foldr_1(f, accu, list)

  defp foldr_1(f, accu, [hd | tail]), do: f.(hd, foldr_1(f, accu, tail))
  defp foldr_1(_f, accu, []), do: accu

  def filter(pred, list) when is_function(pred, 1) and is_list(list), do: filter_1(pred, list)

  defp filter_1(pred, [h | t]) do
    case pred.(h) do
      true -> [h | filter_1(pred, t)]
      false -> filter_1(pred, t)
    end
  end

  defp filter_1(_pred, []), do: []

  def partition(pred, l) when is_function(pred, 1), do: partition_1(pred, l, [], [])

  defp partition_1(pred, [h | t], as, bs) do
    case pred.(h) do
      true -> partition_1(pred, t, [h | as], bs)
      false -> partition_1(pred, t, as, [h | bs])
    end
  end

  defp partition_1(_pred, [], as, bs), do: {:lists.reverse(as), :lists.reverse(bs)}

  def filtermap(f, list) when is_function(f, 1), do: filtermap_1(f, list)

  defp filtermap_1(f, [hd | tail]) do
    case f.(hd) do
      true -> [hd | filtermap_1(f, tail)]
      {true, val} -> [val | filtermap_1(f, tail)]
      false -> filtermap_1(f, tail)
    end
  end

  defp filtermap_1(_f, []), do: []

  def zf(f, l), do: filtermap(f, l)

  def foreach(f, list) when is_function(f, 1), do: foreach_1(f, list)

  defp foreach_1(f, [hd | tail]) do
    f.(hd)
    foreach_1(f, tail)
  end

  defp foreach_1(_f, []), do: :ok

  def mapfoldl(f, accu, list) when is_function(f, 2), do: mapfoldl_1(f, accu, list)

  defp mapfoldl_1(f, accu0, [hd | tail]) do
    {r, accu1} = f.(hd, accu0)
    {rs, accu2} = mapfoldl_1(f, accu1, tail)
    {[r | rs], accu2}
  end

  defp mapfoldl_1(_f, accu, []), do: {[], accu}

  def mapfoldr(f, accu, list) when is_function(f, 2), do: mapfoldr_1(f, accu, list)

  defp mapfoldr_1(f, accu0, [hd | tail]) do
    {rs, accu1} = mapfoldr_1(f, accu0, tail)
    {r, accu2} = f.(hd, accu1)
    {[r | rs], accu2}
  end

  defp mapfoldr_1(_f, accu, []), do: {[], accu}

  def takewhile(pred, list) when is_function(pred, 1), do: takewhile_1(pred, list)

  defp takewhile_1(pred, [hd | tail]) do
    case pred.(hd) do
      true -> [hd | takewhile_1(pred, tail)]
      false -> []
    end
  end

  defp takewhile_1(_pred, []), do: []

  def dropwhile(pred, list) when is_function(pred, 1), do: dropwhile_1(pred, list)

  defp dropwhile_1(pred, [hd | tail] = rest) do
    case pred.(hd) do
      true -> dropwhile_1(pred, tail)
      false -> rest
    end
  end

  defp dropwhile_1(_pred, []), do: []

  def search(pred, list) when is_function(pred, 1), do: search_1(pred, list)

  defp search_1(pred, [hd | tail]) do
    case pred.(hd) do
      true -> {:value, hd}
      false -> search_1(pred, tail)
    end
  end

  defp search_1(_pred, []), do: false

  def splitwith(pred, list) when is_function(pred, 1), do: splitwith_1(pred, list, [])

  defp splitwith_1(pred, [hd | tail], taken) do
    case pred.(hd) do
      true -> splitwith_1(pred, tail, [hd | taken])
      false -> {:lists.reverse(taken), [hd | tail]}
    end
  end

  defp splitwith_1(_pred, [], taken), do: {:lists.reverse(taken), []}

  def split(n, list) when is_integer(n) and n >= 0 and is_list(list) do
    case split_1(n, list, []) do
      {_, _} = result -> result
      fault when is_atom(fault) -> :erlang.error(fault, [n, list])
    end
  end

  def split(n, list), do: :erlang.error(:badarg, [n, list])

  defp split_1(0, l, r), do: {:lists.reverse(r, []), l}
  defp split_1(n, [h | t], r), do: split_1(n - 1, t, [h | r])
  defp split_1(_, [], _), do: :badarg

  def join(_sep, []), do: []
  def join(sep, [h | t]), do: [h | join_prepend(sep, t)]

  defp join_prepend(_sep, []), do: []
  defp join_prepend(sep, [h | t]), do: [sep, h | join_prepend(sep, t)]

  def uniq(l), do: uniq_1(l, %{})

  defp uniq_1([x | xs], m) do
    if is_map_key(m, x), do: uniq_1(xs, m), else: [x | uniq_1(xs, Map.put(m, x, true))]
  end

  defp uniq_1([], _), do: []

  def uniq(f, l) when is_function(f, 1), do: uniq_2(l, f, %{})

  defp uniq_2([x | xs], f, m) do
    key = f.(x)
    if is_map_key(m, key), do: uniq_2(xs, f, m), else: [x | uniq_2(xs, f, Map.put(m, key, true))]
  end

  defp uniq_2([], _, _), do: []
end

defmodule :unicode do
  # characters_to_binary/1 is a compiler intrinsic.

  def characters_to_list(ml), do: characters_to_list(ml, :unicode)

  def characters_to_list(ml, in_enc) do
    case fast_utf8(ml, in_enc, :unicode) do
      {:ok, bin} ->
        :tonic.str_to_charlist(bin)

      :slow ->
        case decode(ml, norm_enc(in_enc), 0x10FFFF) do
          {:ok, acc} -> :lists.reverse(acc)
          {:error, acc, rest} -> {:error, :lists.reverse(acc), rest}
          {:incomplete, acc, rest} -> {:incomplete, :lists.reverse(acc), rest}
        end
    end
  end

  def characters_to_binary(ml) when is_binary(ml), do: ml
  def characters_to_binary(ml), do: characters_to_binary(ml, :unicode, :unicode)
  def characters_to_binary(ml, in_enc), do: characters_to_binary(ml, in_enc, :unicode)

  def characters_to_binary(ml, in_enc, out_enc) do
    if no_conversion_needed(ml, in_enc, out_enc) do
      ml
    else
      case fast_utf8(ml, in_enc, out_enc) do
        {:ok, bin} ->
          bin

        :slow ->
          out = norm_enc(out_enc)
          limit = if out == :latin1, do: 255, else: 0x10FFFF

          case decode(ml, norm_enc(in_enc), limit) do
            {:ok, acc} -> encode(:lists.reverse(acc), out)
            {:error, acc, rest} -> {:error, encode(:lists.reverse(acc), out), rest}
            {:incomplete, acc, rest} -> {:incomplete, encode(:lists.reverse(acc), out), rest}
          end
      end
    end
  end

  def characters_to_list_int(ml, enc), do: characters_to_list(ml, enc)
  def characters_to_binary_int(ml, enc), do: characters_to_binary(ml, enc, :unicode)
  def characters_to_binary_int(ml, in_enc, out_enc), do: characters_to_binary(ml, in_enc, out_enc)

  defp no_conversion_needed(ml, :latin1, :latin1), do: is_binary(ml)

  defp no_conversion_needed(ml, in_enc, out_enc) do
    case {in_enc, out_enc} do
      {:latin1, :utf8} -> bin_is_7bit(ml)
      {:latin1, :unicode} -> bin_is_7bit(ml)
      {:utf8, :latin1} -> bin_is_7bit(ml)
      {:unicode, :latin1} -> bin_is_7bit(ml)
      _ -> false
    end
  end

  # Fast path for UTF-8 → UTF-8 conversion of valid data.
  defp fast_utf8(ml, in_enc, out_enc)
       when (in_enc == :unicode or in_enc == :utf8) and (out_enc == :unicode or out_enc == :utf8) do
    cond do
      is_binary(ml) ->
        if String.valid?(ml), do: {:ok, ml}, else: :slow

      is_list(ml) ->
        bin =
          try do
            :tonic.chardata_to_binary(ml)
          rescue
            _ -> nil
          end

        if is_binary(bin) and String.valid?(bin), do: {:ok, bin}, else: :slow

      true ->
        :erlang.error(:badarg, [ml, in_enc])
    end
  end

  defp fast_utf8(_ml, _in, _out), do: :slow

  defp norm_enc(:unicode), do: :utf8
  defp norm_enc(:utf8), do: :utf8
  defp norm_enc(:latin1), do: :latin1
  defp norm_enc(:utf16), do: {:utf16, :big}
  defp norm_enc({:utf16, :big}), do: {:utf16, :big}
  defp norm_enc({:utf16, :little}), do: {:utf16, :little}
  defp norm_enc(:utf32), do: {:utf32, :big}
  defp norm_enc({:utf32, :big}), do: {:utf32, :big}
  defp norm_enc({:utf32, :little}), do: {:utf32, :little}
  defp norm_enc(other), do: :erlang.error(:badarg, [other])

  # decode(data, enc, limit) -> {:ok, rev_cps} | {:error, rev_cps, rest} | {:incomplete, rev_cps, rest}
  defp decode(bin, enc, limit) when is_binary(bin), do: decode_bin(bin, enc, limit, [])
  defp decode(list, enc, limit) when is_list(list), do: decode_list(list, enc, limit, [])
  defp decode(other, _enc, _limit), do: :erlang.error(:badarg, [other])

  defp decode_list([], _enc, _limit, acc), do: {:ok, acc}

  defp decode_list([h | t] = l, enc, limit, acc) when is_integer(h) do
    if valid_int(h, enc, limit) do
      decode_list(t, enc, limit, [h | acc])
    else
      {:error, acc, l}
    end
  end

  defp decode_list([h | t], enc, limit, acc) when is_binary(h) do
    case decode_bin(h, enc, limit, acc) do
      {:ok, acc2} -> decode_list(t, enc, limit, acc2)
      {:error, acc2, rest} -> {:error, acc2, [rest | t]}
      {:incomplete, acc2, rest} -> cont_incomplete(rest, t, enc, limit, acc2)
    end
  end

  defp decode_list([h | t], enc, limit, acc) when is_list(h) do
    case decode_list(h, enc, limit, acc) do
      {:ok, acc2} -> decode_list(t, enc, limit, acc2)
      {:error, acc2, rest} -> {:error, acc2, [rest | t]}
      {:incomplete, acc2, rest} -> cont_incomplete(rest, t, enc, limit, acc2)
    end
  end

  defp decode_list(bin, enc, limit, acc) when is_binary(bin), do: decode_bin(bin, enc, limit, acc)
  defp decode_list(other, _enc, _limit, _acc), do: :erlang.error(:badarg, [other])

  defp cont_incomplete(rest, t, enc, limit, acc) do
    case pull_bin(t) do
      {:bin, b, r} -> decode_list([rest <> b | r], enc, limit, acc)
      :int -> {:error, acc, [rest | t]}
      :empty -> {:incomplete, acc, rest}
    end
  end

  defp pull_bin([]), do: :empty
  defp pull_bin(b) when is_binary(b), do: {:bin, b, []}
  defp pull_bin([b | r]) when is_binary(b), do: {:bin, b, r}
  defp pull_bin([i | _]) when is_integer(i), do: :int

  defp pull_bin([l | r]) when is_list(l) do
    case pull_bin(l) do
      {:bin, b, lr} -> {:bin, b, [lr | r]}
      :empty -> pull_bin(r)
      :int -> :int
    end
  end

  defp pull_bin(other), do: :erlang.error(:badarg, [other])

  defp valid_int(i, :latin1, limit), do: i >= 0 and i <= 255 and i <= limit

  defp valid_int(i, _enc, limit) do
    i >= 0 and i <= limit and i <= 0x10FFFF and (i < 0xD800 or i > 0xDFFF)
  end

  defp decode_bin(<<>>, _enc, _limit, acc), do: {:ok, acc}

  defp decode_bin(bin, :latin1, _limit, acc), do: {:ok, :lists.reverse(:erlang.binary_to_list(bin), acc)}

  defp decode_bin(bin, :utf8, limit, acc) do
    case bin do
      <<c::utf8, rest::binary>> ->
        if c <= limit and (c < 0xD800 or c > 0xDFFF) do
          decode_bin(rest, :utf8, limit, [c | acc])
        else
          {:error, acc, bin}
        end

      _ ->
        if utf8_incomplete?(bin), do: {:incomplete, acc, bin}, else: {:error, acc, bin}
    end
  end

  defp decode_bin(bin, {:utf16, endian}, limit, acc) do
    case read16(bin, endian) do
      {w, rest} when w >= 0xD800 and w <= 0xDBFF ->
        case read16(rest, endian) do
          {w2, rest2} when w2 >= 0xDC00 and w2 <= 0xDFFF ->
            c = 0x10000 + Bitwise.bsl(w - 0xD800, 10) + (w2 - 0xDC00)
            if c <= limit, do: decode_bin(rest2, {:utf16, endian}, limit, [c | acc]), else: {:error, acc, bin}

          {_w2, _} ->
            {:error, acc, bin}

          :short ->
            if utf16_incomplete?(bin, endian), do: {:incomplete, acc, bin}, else: {:error, acc, bin}
        end

      {w, _rest} when w >= 0xDC00 and w <= 0xDFFF ->
        {:error, acc, bin}

      {w, rest} ->
        if w <= limit, do: decode_bin(rest, {:utf16, endian}, limit, [w | acc]), else: {:error, acc, bin}

      :short ->
        if utf16_incomplete?(bin, endian), do: {:incomplete, acc, bin}, else: {:error, acc, bin}
    end
  end

  defp decode_bin(bin, {:utf32, endian}, limit, acc) do
    case read32(bin, endian) do
      {c, rest} ->
        if c <= limit and c <= 0x10FFFF and (c < 0xD800 or c > 0xDFFF) do
          decode_bin(rest, {:utf32, endian}, limit, [c | acc])
        else
          {:error, acc, bin}
        end

      :short ->
        if utf32_incomplete?(bin, endian), do: {:incomplete, acc, bin}, else: {:error, acc, bin}
    end
  end

  defp read16(<<w::16, rest::binary>>, :big), do: {w, rest}
  defp read16(<<w::16-little, rest::binary>>, :little), do: {w, rest}
  defp read16(_, _), do: :short

  defp read32(<<w::32, rest::binary>>, :big), do: {w, rest}
  defp read32(<<w::32-little, rest::binary>>, :little), do: {w, rest}
  defp read32(_, _), do: :short

  # Mirrors cbv/2 in unicode.erl: is `bin` a truncated but valid prefix?
  defp utf8_incomplete?(<<a>>) when a >= 0xC0 and a <= 0xF7, do: true
  defp utf8_incomplete?(<<a, b>>) when a >= 0xE0 and a <= 0xF7 and b >= 0x80 and b <= 0xBF, do: true

  defp utf8_incomplete?(<<a, b, c>>)
       when a >= 0xF0 and a <= 0xF7 and b >= 0x80 and b <= 0xBF and c >= 0x80 and c <= 0xBF,
       do: true

  defp utf8_incomplete?(_), do: false

  defp utf16_incomplete?(<<a>>, :big), do: a <= 215 or a >= 224 or (a >= 0xD8 and a <= 0xDB)
  defp utf16_incomplete?(<<a, _>>, :big), do: a >= 0xD8 and a <= 0xDB
  defp utf16_incomplete?(<<a, _, c>>, :big), do: a >= 0xD8 and a <= 0xDB and c >= 0xDC and c <= 0xDF
  defp utf16_incomplete?(<<_>>, :little), do: true
  defp utf16_incomplete?(<<_, b>>, :little), do: b >= 0xD8 and b <= 0xDB
  defp utf16_incomplete?(<<_, b, _>>, :little), do: b >= 0xD8 and b <= 0xDB
  defp utf16_incomplete?(_, _), do: false

  defp utf32_incomplete?(<<0>>, :big), do: true
  defp utf32_incomplete?(<<0, x>>, :big), do: x <= 16
  defp utf32_incomplete?(<<0, x, y>>, :big), do: x <= 16 and (x > 0 or y <= 215 or y >= 224)
  defp utf32_incomplete?(<<_>>, :little), do: true
  defp utf32_incomplete?(<<_, _>>, :little), do: true
  defp utf32_incomplete?(<<x, 255, 0>>, :little) when x == 254 or x == 255, do: false
  defp utf32_incomplete?(<<_, y, x>>, :little), do: x <= 16 and (x > 0 or y <= 215 or y >= 224)
  defp utf32_incomplete?(_, _), do: false

  defp encode(cps, :utf8), do: :unicode.characters_to_binary(cps)
  defp encode(cps, :latin1), do: :erlang.list_to_binary(cps)
  defp encode(cps, {:utf16, :big}), do: :erlang.list_to_binary(for(c <- cps, do: <<c::utf16>>))
  defp encode(cps, {:utf16, :little}), do: :erlang.list_to_binary(for(c <- cps, do: <<c::utf16-little>>))
  defp encode(cps, {:utf32, :big}), do: :erlang.list_to_binary(for(c <- cps, do: <<c::32>>))
  defp encode(cps, {:utf32, :little}), do: :erlang.list_to_binary(for(c <- cps, do: <<c::32-little>>))

  def bin_is_7bit(bin) when is_binary(bin), do: is_7bit(bin)
  def bin_is_7bit(_), do: false

  defp is_7bit(<<c, rest::binary>>) when c < 128, do: is_7bit(rest)
  defp is_7bit(<<>>), do: true
  defp is_7bit(_), do: false

  def bom_to_encoding(<<239, 187, 191, _::binary>>), do: {:utf8, 3}
  def bom_to_encoding(<<0, 0, 254, 255, _::binary>>), do: {{:utf32, :big}, 4}
  def bom_to_encoding(<<255, 254, 0, 0, _::binary>>), do: {{:utf32, :little}, 4}
  def bom_to_encoding(<<254, 255, _::binary>>), do: {{:utf16, :big}, 2}
  def bom_to_encoding(<<255, 254, _::binary>>), do: {{:utf16, :little}, 2}
  def bom_to_encoding(bin) when is_binary(bin), do: {:latin1, 0}

  def encoding_to_bom(:unicode), do: <<239, 187, 191>>
  def encoding_to_bom(:utf8), do: <<239, 187, 191>>
  def encoding_to_bom(:utf16), do: <<254, 255>>
  def encoding_to_bom({:utf16, :big}), do: <<254, 255>>
  def encoding_to_bom({:utf16, :little}), do: <<255, 254>>
  def encoding_to_bom(:utf32), do: <<0, 0, 254, 255>>
  def encoding_to_bom({:utf32, :big}), do: <<0, 0, 254, 255>>
  def encoding_to_bom({:utf32, :little}), do: <<255, 254, 0, 0>>
  def encoding_to_bom(:latin1), do: <<>>

  def characters_to_nfd_list(cd), do: norm_list(cd, 1)
  def characters_to_nfc_list(cd), do: norm_list(cd, 0)
  def characters_to_nfkd_list(cd), do: norm_list(cd, 3)
  def characters_to_nfkc_list(cd), do: norm_list(cd, 2)
  def characters_to_nfd_binary(cd), do: norm_bin(cd, 1)
  def characters_to_nfc_binary(cd), do: norm_bin(cd, 0)
  def characters_to_nfkd_binary(cd), do: norm_bin(cd, 3)
  def characters_to_nfkc_binary(cd), do: norm_bin(cd, 2)

  defp norm_list(cd, form) do
    case norm_bin(cd, form) do
      b when is_binary(b) -> :tonic.str_to_charlist(b)
      {:error, b, rest} -> {:error, :tonic.str_to_charlist(b), rest}
    end
  end

  defp norm_bin(cd, form) do
    case characters_to_binary(cd, :unicode, :unicode) do
      b when is_binary(b) -> :tonic.unorm(b, form)
      {:incomplete, b, rest} -> {:error, :tonic.unorm(b, form), rest}
      {:error, b, rest} -> {:error, :tonic.unorm(b, form), rest}
    end
  end
end

defmodule :string do
  # Port of OTP 27 string.erl. Grapheme clustering is delegated to the
  # runtime's String.next_grapheme/graphemes. Results follow OTP: binary
  # input gives binary results; any other chardata gives (flat) charlists.

  @whitespace [[13, 10], 9, 10, 11, 12, 13, 32, 133, 8206, 8207, 8232, 8233]

  # ---------------------------------------------------------------------------
  # Internal helpers

  # Flatten chardata to a codepoint list, raising {:badarg, _} if invalid.
  defp cps(cd) when is_list(cd) do
    case :unicode.characters_to_list(cd) do
      l when is_list(l) -> l
      _ -> :erlang.error({:badarg, cd})
    end
  end

  defp cps(cd) when is_binary(cd) do
    if String.valid?(cd), do: :tonic.str_to_charlist(cd), else: :erlang.error({:badarg, cd})
  end

  defp cps(cd), do: :erlang.error({:badarg, cd})

  defp to_bin(cd) when is_binary(cd) do
    if String.valid?(cd), do: cd, else: :erlang.error({:badarg, cd})
  end

  defp to_bin(cd) when is_list(cd) do
    case :unicode.characters_to_binary(cd, :unicode) do
      b when is_binary(b) -> b
      _ -> :erlang.error({:badarg, cd})
    end
  end

  defp to_bin(cd), do: :erlang.error({:badarg, cd})

  # Grapheme cluster list: each element is a codepoint or a list of codepoints.
  defp gcs(cd) do
    bin = to_bin(cd)
    if ascii?(bin), do: :erlang.binary_to_list(bin), else: gcs_bin(String.graphemes(bin))
  end

  defp gcs_bin([g | gs]), do: [gc_of(g) | gcs_bin(gs)]
  defp gcs_bin([]), do: []

  defp ascii?(<<c, rest::binary>>) when c < 128 and c != 13, do: ascii?(rest)
  defp ascii?(<<>>), do: true
  defp ascii?(_), do: false

  defp gc_of(g) do
    case g do
      <<c::utf8>> -> c
      _ -> :tonic.str_to_charlist(g)
    end
  end

  # Output helpers: build a result of the same kind as the input.
  defp out(gcs, true), do: :unicode.characters_to_binary(gcs)
  defp out(gcs, false), do: :lists.flatten(gcs)

  defp empty(true), do: <<>>
  defp empty(false), do: []

  # unicode_util:cp/1
  defp cp([c | _] = l) when is_integer(c), do: l
  defp cp([]), do: []

  defp cp([h | t]) do
    case cp(h) do
      [] -> cp(t)
      [c | r] -> [c | stack(r, t)]
      {:error, e} -> {:error, stack(e, t)}
    end
  end

  defp cp(<<c::utf8, r::binary>>), do: [c | r]
  defp cp(<<>>), do: []
  defp cp(bin) when is_binary(bin), do: {:error, bin}

  defp stack(r, []), do: r
  defp stack(<<>>, t), do: t
  defp stack([], t), do: t
  defp stack(r, t), do: [r | t]

  # unicode_util:gc/1
  defp gc(<<>>), do: []

  defp gc(bin) when is_binary(bin), do: :tonic.unicode_gc(bin)
  @doc false
  def __gc__(cd), do: gc(cd)

  defp gc(cd) do
    case cp(cd) do
      [] -> []
      {:error, _} = err -> err
      [c1 | rest] ->
        case cp(rest) do
          [] -> [c1 | rest]
          {:error, _} -> [c1 | rest]
          [c2 | _] when c2 < 0x300 and c1 != 13 and c1 != 0x200D and c1 < 0x1F1E6 -> [c1 | rest]
          _ -> gc_slow(c1, rest, 8)
        end
    end
  end

  defp gc_slow(c1, rest, n) do
    {taken, more?} = take_cps(rest, n - 1, [c1])
    bin = :unicode.characters_to_binary(taken)
    {g, _} = String.next_grapheme(bin)

    if byte_size(g) == byte_size(bin) and more? do
      gc_slow(c1, rest, n * 2)
    else
      case :tonic.str_to_charlist(g) do
        [c] -> [c | rest]
        cs -> [cs | drop_cps(rest, :erlang.length(cs) - 1)]
      end
    end
  end

  defp take_cps(cd, 0, acc), do: {:lists.reverse(acc), cp(cd) != []}

  defp take_cps(cd, n, acc) do
    case cp(cd) do
      [c | r] -> take_cps(r, n - 1, [c | acc])
      _ -> {:lists.reverse(acc), false}
    end
  end

  defp drop_cps(cd, 0), do: cd

  defp drop_cps(cd, n) do
    [_ | r] = cp(cd)
    drop_cps(r, n - 1)
  end

  # Does a grapheme cluster boundary follow the codepoint of `lsz` bytes
  # ending at byte `pos` of `bin`?
  defp bboundary(bin, pos, lsz) do
    size = byte_size(bin)

    if pos >= size do
      true
    else
      start = pos - lsz
      len = if size - start > lsz + 16, do: lsz + 16, else: size - start
      {g, _} = String.next_grapheme(:binary.part(bin, start, len))
      byte_size(g) == lsz
    end
  end

  defp cp_size(c) when c < 0x80, do: 1
  defp cp_size(c) when c < 0x800, do: 2
  defp cp_size(c) when c < 0x10000, do: 3
  defp cp_size(_), do: 4

  # prefix_1/2 on flat codepoint lists
  defp lprefix(cs, [g]) do
    case gc(cs) do
      [^g | rest] -> rest
      _ -> :nomatch
    end
  end

  defp lprefix([c | cs], [c | pre]), do: lprefix(cs, pre)
  defp lprefix(_, _), do: :nomatch

  # First valid match of needle `nb` at or after byte `start`.
  defp bmatch(bin, start, nb, lsz) do
    size = byte_size(bin)

    if start + byte_size(nb) > size do
      :nomatch
    else
      case :binary.match(:binary.part(bin, start, size - start), nb) do
        :nomatch ->
          :nomatch

        {i, l} ->
          pos = start + i
          if bboundary(bin, pos + l, lsz), do: pos, else: bmatch(bin, pos + 1, nb, lsz)
      end
    end
  end

  defp bmatch_last(bin, start, nb, lsz, last) do
    case bmatch(bin, start, nb, lsz) do
      :nomatch -> last
      pos -> bmatch_last(bin, pos + 1, nb, lsz, pos)
    end
  end

  defp bpart(bin, from), do: :binary.part(bin, from, byte_size(bin) - from)

  # ---------------------------------------------------------------------------
  # Public API

  def is_empty([]), do: true
  def is_empty(<<>>), do: true
  def is_empty([l | r]), do: is_empty(l) and is_empty(r)
  def is_empty(_), do: false

  def length(cd) do
    bin = to_bin(cd)
    if ascii?(bin), do: byte_size(bin), else: :erlang.length(String.graphemes(bin))
  end

  def to_graphemes(cd), do: gcs(cd)

  def equal(a, b) when is_binary(a) and is_binary(b), do: a === b
  def equal(a, b), do: cps(a) === cps(b)

  def equal(a, b, false), do: equal(a, b)
  def equal(a, b, true), do: a === b or cps(casefold(a)) === cps(casefold(b))

  def equal(a, b, case_insensitive, :none), do: equal(a, b, case_insensitive)

  def equal(a, b, false, norm) when norm in [:nfc, :nfd, :nfkc, :nfkd],
    do: cps(normalize(a, norm)) === cps(normalize(b, norm))

  def equal(a, b, true, norm) when norm in [:nfc, :nfd, :nfkc, :nfkd],
    do: cps(casefold(normalize(a, norm))) === cps(casefold(normalize(b, norm)))

  defp normalize(cd, :nfc), do: :unicode.characters_to_nfc_list(cd)
  defp normalize(cd, :nfd), do: :unicode.characters_to_nfd_list(cd)
  defp normalize(cd, :nfkc), do: :unicode.characters_to_nfkc_list(cd)
  defp normalize(cd, :nfkd), do: :unicode.characters_to_nfkd_list(cd)

  def reverse(cd), do: :lists.reverse(gcs(cd))

  def slice(cd, n) when is_integer(n) and n >= 0 do
    bin? = is_binary(cd)
    out(Enum.drop(gcs(cd), n), bin?)
  end

  def slice(cd, n, length) when is_integer(n) and n >= 0 and is_integer(length) and length > 0 do
    bin? = is_binary(cd)
    out(Enum.take(Enum.drop(gcs(cd), n), length), bin?)
  end

  def slice(cd, n, :infinity) when is_integer(n) and n >= 0, do: slice(cd, n)
  def slice(cd, _, 0), do: empty(is_binary(cd))

  def pad(cd, length), do: pad(cd, length, :trailing, ?\s)
  def pad(cd, length, dir), do: pad(cd, length, dir, ?\s)

  def pad(cd, length, :leading, char) when is_integer(length) do
    len = :string.length(cd)
    [:lists.duplicate(:erlang.max(0, length - len), char), cd]
  end

  def pad(cd, length, :trailing, char) when is_integer(length) do
    len = :string.length(cd)
    [cd | :lists.duplicate(:erlang.max(0, length - len), char)]
  end

  def pad(cd, length, :both, char) when is_integer(length) do
    len = :string.length(cd)
    size = :erlang.max(0, length - len)
    pre = :lists.duplicate(div(size, 2), char)
    post = if rem(size, 2) == 1, do: [char], else: []
    [pre, cd, pre | post]
  end

  def trim(str), do: trim(str, :both, @whitespace)
  def trim(str, dir), do: trim(str, dir, @whitespace)

  def trim(str, dir, []) when dir in [:leading, :trailing, :both], do: str

  def trim(str, :leading, seps) when is_list(seps) do
    bin? = is_binary(str)
    out(trim_l(gcs(str), seps), bin?)
  end

  def trim(str, :trailing, seps) when is_list(seps) do
    bin? = is_binary(str)
    out(trim_t(gcs(str), seps), bin?)
  end

  def trim(str, :both, seps) when is_list(seps) do
    bin? = is_binary(str)
    out(trim_t(trim_l(gcs(str), seps), seps), bin?)
  end

  defp trim_l([g | gs] = l, seps) do
    if :lists.member(g, seps), do: trim_l(gs, seps), else: l
  end

  defp trim_l([], _), do: []

  defp trim_t(gs, seps), do: :lists.reverse(trim_l(:lists.reverse(gs), seps))

  def chomp(str), do: trim(str, :trailing, [[13, 10], 10])

  def take(str, sep), do: take(str, sep, false, :leading)
  def take(str, sep, complement), do: take(str, sep, complement, :leading)

  def take(str, [], complement, dir) do
    e = empty(is_binary(str))

    case {complement, dir} do
      {false, :leading} -> {e, str}
      {false, :trailing} -> {str, e}
      {true, :leading} -> {str, e}
      {true, :trailing} -> {e, str}
    end
  end

  def take(str, seps, complement, :leading) when is_boolean(complement) and is_list(seps) do
    bin? = is_binary(str)
    {head, tail} = take_while(gcs(str), seps, not complement, [])
    {out(head, bin?), out(tail, bin?)}
  end

  def take(str, seps, complement, :trailing) when is_boolean(complement) and is_list(seps) do
    bin? = is_binary(str)
    {tail_rev, head_rev} = take_while(:lists.reverse(gcs(str)), seps, not complement, [])
    {out(:lists.reverse(head_rev), bin?), out(:lists.reverse(tail_rev), bin?)}
  end

  defp take_while([g | gs] = l, seps, member?, acc) do
    if :lists.member(g, seps) == member?,
      do: take_while(gs, seps, member?, [g | acc]),
      else: {:lists.reverse(acc), l}
  end

  defp take_while([], _seps, _member?, acc), do: {:lists.reverse(acc), []}

  def uppercase(cd), do: change_case(cd, &String.upcase/1)
  def lowercase(cd), do: change_case(cd, &String.downcase/1)
  def casefold(cd), do: change_case(cd, &fold_bin/1)

  defp change_case(cd, f) when is_binary(cd), do: f.(to_bin(cd))

  defp change_case(cd, f) when is_list(cd) do
    bin = to_bin(cd)
    res = f.(bin)
    if res == bin, do: cd, else: :tonic.str_to_charlist(res)
  end

  defp change_case(cd, _f), do: :erlang.error({:badarg, cd})

  defp fold_bin(bin) do
    if ascii?(bin) do
      String.downcase(bin)
    else
      :unicode.characters_to_binary(for c <- :tonic.str_to_charlist(bin), do: fold_cp(c))
    end
  end

  # Approximates Unicode CaseFolding (C+F) with lower(upper(c)).
  defp fold_cp(c) when c < 128, do: String.downcase(<<c>>)
  defp fold_cp(0x131), do: <<0x131::utf8>>
  defp fold_cp(c), do: String.downcase(String.upcase(<<c::utf8>>))

  def titlecase(cd) when is_list(cd) do
    case gc(cd) do
      [g | tail] -> title_gc(g) ++ (if is_binary(tail), do: [tail], else: tail)
      [] -> cd
      {:error, _} -> :erlang.error({:badarg, cd})
    end
  end

  def titlecase(cd) when is_binary(cd) do
    case gc(cd) do
      [g | tail] -> :unicode.characters_to_binary(title_gc(g)) <> tail
      [] -> <<>>
      {:error, err} -> :erlang.error({:badarg, err})
    end
  end

  defp title_gc(g) do
    [c | rest] = List.wrap(g)
    :tonic.char_titlecase(c) ++ rest
  end

  def to_integer(string) do
    taken =
      try do
        {:ok, take(string, ~c"+-0123456789")}
      rescue
        _ -> :error
      end

    case taken do
      :error ->
        {:error, :badarg}

      {:ok, {head, tail}} ->
        if is_empty(head) do
          {:error, :no_integer}
        else
          list = :unicode.characters_to_list(head)

          case list_to_integer(list) do
            {:error, _} = err -> err
            {int, rest} -> to_number(string, int, rest, list, tail)
          end
        end
    end
  end

  def to_float(string) do
    taken =
      try do
        {:ok, take(string, ~c"+-0123456789eE.,")}
      rescue
        _ -> :error
      end

    case taken do
      :error ->
        {:error, :badarg}

      {:ok, {head, tail}} ->
        if is_empty(head) do
          {:error, :no_float}
        else
          list = :unicode.characters_to_list(head)

          case list_to_float(list) do
            {:error, _} = err -> err
            {float, rest} -> to_number(string, float, rest, list, tail)
          end
        end
    end
  end

  defp to_number(string, number, rest, list, _tail) when is_binary(string) do
    bsz = :erlang.length(list) - :erlang.length(rest)
    {number, :binary.part(string, bsz, byte_size(string) - bsz)}
  end

  defp to_number(_, number, rest, _, tail), do: {number, rest ++ out_list(tail)}

  defp out_list(t) when is_binary(t), do: :tonic.str_to_charlist(t)
  defp out_list(t), do: t

  # string:list_to_integer/1 (erts_internal:list_to_integer semantics)
  def list_to_integer(list) when is_list(list) do
    {sign, rest} =
      case list do
        [?- | r] -> {-1, r}
        [?+ | r] -> {1, r}
        r -> {1, r}
      end

    case digits(rest, []) do
      {[], _} -> {:error, :no_integer}
      {ds, r} -> {sign * :erlang.list_to_integer(ds), r}
    end
  end

  def list_to_integer(_), do: {:error, :not_a_list}

  defp digits([d | r], acc) when d >= ?0 and d <= ?9, do: digits(r, [d | acc])
  defp digits(r, acc), do: {:lists.reverse(acc), r}

  # string:list_to_float/1
  def list_to_float(list) when is_list(list) do
    {sign, rest} =
      case list do
        [?- | r] -> {~c"-", r}
        [?+ | r] -> {[], r}
        r -> {[], r}
      end

    with {[_ | _] = int, [?. | r1]} <- digits(rest, []),
         {[_ | _] = frac, r2} <- digits(r1, []) do
      {exp, r3} =
        case r2 do
          [e | r4] when e == ?e or e == ?E ->
            {esign, r5} =
              case r4 do
                [?- | r6] -> {~c"-", r6}
                [?+ | r6] -> {[], r6}
                r6 -> {[], r6}
              end

            case digits(r5, []) do
              {[], _} -> {[], r2}
              {ed, r7} -> {[?e | esign ++ ed], r7}
            end

          _ ->
            {[], r2}
        end

      text = :erlang.list_to_binary(sign ++ int ++ [?. | frac] ++ exp)

      try do
        {:erlang.binary_to_float(text), r3}
      rescue
        _ -> {:error, :badarg}
      end
    else
      _ -> {:error, :no_float}
    end
  end

  def list_to_float(_), do: {:error, :not_a_list}

  def prefix(str, prefix0) do
    case cps(prefix0) do
      [] ->
        str

      pre when is_binary(str) ->
        pb = :unicode.characters_to_binary(pre)
        n = byte_size(pb)
        last = :lists.last(pre)

        if byte_size(str) >= n and :binary.part(str, 0, n) == pb and bboundary(str, n, cp_size(last)) do
          bpart(str, n)
        else
          :nomatch
        end

      pre ->
        case lprefix(cps(str), pre) do
          :nomatch -> :nomatch
          rest -> rest
        end
    end
  end

  def split(string, search_pattern), do: split(string, search_pattern, :leading)

  def split(string, search_pattern, where) when where in [:leading, :trailing, :all] do
    if is_empty(search_pattern) do
      [string]
    else
      needle = cps(search_pattern)

      if is_binary(string) do
        bsplit(to_bin(string), :unicode.characters_to_binary(needle), cp_size(:lists.last(needle)), where)
      else
        case lsplit(cps(string), needle, where, [], []) do
          {_curr, []} -> [string]
          {_curr, acc} when where == :trailing -> acc
          {curr, acc} when where == :all -> :lists.reverse([curr | acc])
          acc when is_list(acc) -> acc
        end
      end
    end
  end

  defp bsplit(bin, nb, lsz, :leading) do
    case bmatch(bin, 0, nb, lsz) do
      :nomatch -> [bin]
      pos -> [:binary.part(bin, 0, pos), bpart(bin, pos + byte_size(nb))]
    end
  end

  defp bsplit(bin, nb, lsz, :trailing) do
    case bmatch_last(bin, 0, nb, lsz, :nomatch) do
      :nomatch -> [bin]
      pos -> [:binary.part(bin, 0, pos), bpart(bin, pos + byte_size(nb))]
    end
  end

  defp bsplit(bin, nb, lsz, :all), do: bsplit_all(bin, 0, nb, lsz, [])

  defp bsplit_all(bin, start, nb, lsz, acc) do
    case bmatch(bin, start, nb, lsz) do
      :nomatch ->
        :lists.reverse([bpart(bin, start) | acc])

      pos ->
        bsplit_all(bin, pos + byte_size(nb), nb, lsz, [:binary.part(bin, start, pos - start) | acc])
    end
  end

  defp lsplit([c | cs] = cs0, [c | _] = needle, where, curr, acc) do
    case lprefix(cs0, needle) do
      :nomatch ->
        lsplit(cs, needle, where, [c | curr], acc)

      rest ->
        case where do
          :leading -> [:lists.reverse(curr), rest]
          :trailing -> lsplit(cs, needle, where, [c | curr], [:lists.reverse(curr), rest])
          :all -> lsplit(rest, needle, where, [], [:lists.reverse(curr) | acc])
        end
    end
  end

  defp lsplit([c | cs], needle, where, curr, acc), do: lsplit(cs, needle, where, [c | curr], acc)
  defp lsplit([], _needle, _where, curr, acc), do: {:lists.reverse(curr), acc}

  def replace(string, search_pattern, replacement),
    do: :lists.join(replacement, split(string, search_pattern))

  def replace(string, search_pattern, replacement, where),
    do: :lists.join(replacement, split(string, search_pattern, where))

  def lexemes([], _), do: []
  def lexemes(str, []), do: [str]

  def lexemes(str, seps) when is_list(seps) do
    bin? = is_binary(str)
    for lex <- lexemes_gc(gcs(str), seps, [], []), do: out(lex, bin?)
  end

  defp lexemes_gc([g | gs], seps, cur, acc) do
    if :lists.member(g, seps) do
      case cur do
        [] -> lexemes_gc(gs, seps, [], acc)
        _ -> lexemes_gc(gs, seps, [], [:lists.reverse(cur) | acc])
      end
    else
      lexemes_gc(gs, seps, [g | cur], acc)
    end
  end

  defp lexemes_gc([], _seps, [], acc), do: :lists.reverse(acc)
  defp lexemes_gc([], _seps, cur, acc), do: :lists.reverse([:lists.reverse(cur) | acc])

  def nth_lexeme(str, 1, []), do: str

  def nth_lexeme(str, n, seps) when is_list(seps) and is_integer(n) and n > 0 do
    bin? = is_binary(str)

    case Enum.drop(lexemes_gc(gcs(str), seps, [], []), n - 1) do
      [lex | _] -> out(lex, bin?)
      [] -> empty(bin?)
    end
  end

  def find(string, search_pattern), do: find(string, search_pattern, :leading)

  def find(string, [], dir) when dir in [:leading, :trailing], do: string
  def find(string, <<>>, dir) when dir in [:leading, :trailing], do: string

  def find(string, search_pattern, dir) when dir in [:leading, :trailing] do
    needle = cps(search_pattern)

    if is_binary(string) do
      bin = to_bin(string)
      nb = :unicode.characters_to_binary(needle)
      lsz = cp_size(:lists.last(needle))

      res =
        if dir == :leading, do: bmatch(bin, 0, nb, lsz), else: bmatch_last(bin, 0, nb, lsz, :nomatch)

      if res == :nomatch, do: :nomatch, else: bpart(bin, res)
    else
      if dir == :leading,
        do: find_l(cps(string), needle),
        else: find_r(cps(string), needle, :nomatch)
    end
  end

  defp find_l([c | cs] = cs0, [c | _] = needle) do
    case lprefix(cs0, needle) do
      :nomatch -> find_l(cs, needle)
      _ -> cs0
    end
  end

  defp find_l([_ | cs], needle), do: find_l(cs, needle)
  defp find_l([], _needle), do: :nomatch

  defp find_r([c | cs] = cs0, [c | _] = needle, res) do
    case lprefix(cs0, needle) do
      :nomatch -> find_r(cs, needle, res)
      _ -> find_r(cs, needle, cs0)
    end
  end

  defp find_r([_ | cs], needle, res), do: find_r(cs, needle, res)
  defp find_r([], _needle, res), do: res

  def next_grapheme(cd) do
    case gc(cd) do
      {:error, _} = err -> err
      res -> res
    end
  end

  def next_codepoint(cd), do: cp(cd)

  # ---------------------------------------------------------------------------
  # jaro_similarity

  def jaro_similarity(a0, b0) do
    a = gcs(a0)
    alen = :erlang.length(a)
    b_gcs = gcs(b0)
    blen = :erlang.length(b_gcs)
    b = str_to_indexmap(b_gcs)
    dist = :erlang.max(1, div(:erlang.max(alen, blen), 2))
    {am, bm} = jaro_match(a, b, -dist, dist, [], [])

    cond do
      alen == 0 and blen == 0 -> 1.0
      alen == 0 or blen == 0 -> 0.0
      am == [] -> 0.0
      true ->
        {m, t} = jaro_calc_mt(am, bm, 0, 0)
        (m / alen + m / blen + (m - t / 2) / m) / 3
    end
  end

  defp str_to_indexmap(gs) do
    {m, _} =
      Enum.reduce(gs, {%{}, 0}, fn g, {m, i} ->
        {Map.update(m, g, [i], fn l -> l ++ [i] end), i + 1}
      end)

    m
  end

  defp jaro_match([a | as], b0, min, max, am, bm) do
    case jaro_detect(Map.get(b0, a, []), min, max) do
      false ->
        jaro_match(as, b0, min + 1, max + 1, am, bm)

      {j, remain} ->
        b = Map.put(b0, a, remain)
        jaro_match(as, b, min + 1, max + 1, [a | am], add_rsorted({j, a}, bm))
    end
  end

  defp jaro_match(_a, _b, _min, _max, am, bm), do: {am, bm}

  defp jaro_detect([idx | rest], min, max) when min < idx and idx < max, do: {idx, rest}
  defp jaro_detect([idx | rest], min, max) when idx < max, do: jaro_detect(rest, min, max)
  defp jaro_detect(_, _, _), do: false

  defp jaro_calc_mt([char_a | am], [{_, char_a} | bm], m, t), do: jaro_calc_mt(am, bm, m + 1, t)
  defp jaro_calc_mt([_ | am], [_ | bm], m, t), do: jaro_calc_mt(am, bm, m + 1, t + 1)
  defp jaro_calc_mt([], [], m, t), do: {m, t}

  defp add_rsorted(a, [h | _] = bm) when a > h, do: [a | bm]
  defp add_rsorted(a, [h | bm]), do: [h | add_rsorted(a, bm)]
  defp add_rsorted(a, []), do: [a]

  # ---------------------------------------------------------------------------
  # Obsolete API (flat character lists)

  def len(s), do: :erlang.length(s)

  def concat(s1, s2), do: s1 ++ s2

  def chr(s, c) when is_integer(c), do: chr(s, c, 1)

  defp chr([c | _cs], c, i), do: i
  defp chr([_ | cs], c, i), do: chr(cs, c, i + 1)
  defp chr([], _c, _i), do: 0

  def rchr(s, c) when is_integer(c), do: rchr(s, c, 1, 0)

  defp rchr([c | cs], c, i, _l), do: rchr(cs, c, i + 1, i)
  defp rchr([_ | cs], c, i, l), do: rchr(cs, c, i + 1, l)
  defp rchr([], _c, _i, l), do: l

  def str(s, sub) when is_list(sub), do: str(s, sub, 1)

  defp str([c | s], [c | sub], i) do
    if l_prefix(sub, s), do: i, else: str(s, [c | sub], i + 1)
  end

  defp str([_ | s], sub, i), do: str(s, sub, i + 1)
  defp str([], _sub, _i), do: 0

  def rstr(s, sub) when is_list(sub), do: rstr(s, sub, 1, 0)

  defp rstr([c | s], [c | sub], i, l) do
    if l_prefix(sub, s), do: rstr(s, [c | sub], i + 1, i), else: rstr(s, [c | sub], i + 1, l)
  end

  defp rstr([_ | s], sub, i, l), do: rstr(s, sub, i + 1, l)
  defp rstr([], _sub, _i, l), do: l

  defp l_prefix([c | pre], [c | string]), do: l_prefix(pre, string)
  defp l_prefix([], string) when is_list(string), do: true
  defp l_prefix(pre, string) when is_list(pre) and is_list(string), do: false

  def span(s, cs) when is_list(cs), do: span(s, cs, 0)

  defp span([c | s], cs, i) do
    if :lists.member(c, cs), do: span(s, cs, i + 1), else: i
  end

  defp span([], _cs, i), do: i

  def cspan(s, cs) when is_list(cs), do: cspan(s, cs, 0)

  defp cspan([c | s], cs, i) do
    if :lists.member(c, cs), do: i, else: cspan(s, cs, i + 1)
  end

  defp cspan([], _cs, i), do: i

  def substr(string, 1) when is_list(string), do: string
  def substr(string, s) when is_integer(s) and s > 1, do: substr2(string, s)

  def substr(string, s, l) when is_integer(s) and s >= 1 and is_integer(l) and l >= 0,
    do: substr1(substr2(string, s), l)

  defp substr1([c | string], l) when l > 0, do: [c | substr1(string, l - 1)]
  defp substr1(string, _l) when is_list(string), do: []

  defp substr2(string, 1) when is_list(string), do: string
  defp substr2([_ | string], s), do: substr2(string, s - 1)

  def tokens(s, seps) do
    case seps do
      [] ->
        case s do
          [] -> []
          [_ | _] -> [s]
        end

      [c] ->
        tokens_single_1(:lists.reverse(s), c, [])

      [_ | _] ->
        tokens_multiple_1(:lists.reverse(s), seps, [])
    end
  end

  defp tokens_single_1([sep | s], sep, toks), do: tokens_single_1(s, sep, toks)
  defp tokens_single_1([c | s], sep, toks), do: tokens_single_2(s, sep, toks, [c])
  defp tokens_single_1([], _, toks), do: toks

  defp tokens_single_2([sep | s], sep, toks, tok), do: tokens_single_1(s, sep, [tok | toks])
  defp tokens_single_2([c | s], sep, toks, tok), do: tokens_single_2(s, sep, toks, [c | tok])
  defp tokens_single_2([], _sep, toks, tok), do: [tok | toks]

  defp tokens_multiple_1([c | s], seps, toks) do
    if :lists.member(c, seps),
      do: tokens_multiple_1(s, seps, toks),
      else: tokens_multiple_2(s, seps, toks, [c])
  end

  defp tokens_multiple_1([], _seps, toks), do: toks

  defp tokens_multiple_2([c | s], seps, toks, tok) do
    if :lists.member(c, seps),
      do: tokens_multiple_1(s, seps, [tok | toks]),
      else: tokens_multiple_2(s, seps, toks, [c | tok])
  end

  defp tokens_multiple_2([], _seps, toks, tok), do: [tok | toks]

  def chars(c, n), do: chars(c, n, [])

  def chars(c, n, tail) when is_integer(n) and n > 0, do: chars(c, n - 1, [c | tail])
  def chars(c, 0, tail) when is_integer(c), do: tail

  def copies(char_list, num) when is_list(char_list) and is_integer(num) and num >= 0,
    do: copies(char_list, num, [])

  defp copies(_char_list, 0, r), do: r
  defp copies(char_list, num, r), do: copies(char_list, num - 1, char_list ++ r)

  def words(string), do: words(string, ?\s)

  def words(string, char) when is_integer(char), do: w_count(strip(string, :both, char), char, 0)

  defp w_count([], _, num), do: num + 1
  defp w_count([h | t], h, num), do: w_count(strip(t, :left, h), h, num + 1)
  defp w_count([_h | t], char, num), do: w_count(t, char, num)

  def sub_word(string, index), do: sub_word(string, index, ?\s)

  def sub_word(string, index, char) when is_integer(index) and is_integer(char) do
    if words(string, char) < index do
      []
    else
      s_word(strip(string, :left, char), index, char, 1, [])
    end
  end

  defp s_word([], _, _, _, res), do: :lists.reverse(res)
  defp s_word([char | _], index, char, index, res), do: :lists.reverse(res)
  defp s_word([h | t], index, char, index, res), do: s_word(t, index, char, index, [h | res])

  defp s_word([char | t], stop, char, index, res) when index < stop,
    do: s_word(strip(t, :left, char), stop, char, index + 1, res)

  defp s_word([_ | t], stop, char, index, res) when index < stop,
    do: s_word(t, stop, char, index, res)

  def strip(string), do: strip(string, :both)

  def strip(string, :left), do: strip_left(string, ?\s)
  def strip(string, :right), do: strip_right(string, ?\s)
  def strip(string, :both), do: strip_right(strip_left(string, ?\s), ?\s)

  def strip(string, :right, char), do: strip_right(string, char)
  def strip(string, :left, char), do: strip_left(string, char)
  def strip(string, :both, char), do: strip_right(strip_left(string, char), char)

  defp strip_left([sc | s], sc), do: strip_left(s, sc)
  defp strip_left([_ | _] = s, sc) when is_integer(sc), do: s
  defp strip_left([], sc) when is_integer(sc), do: []

  defp strip_right([sc | s], sc) do
    case strip_right(s, sc) do
      [] -> []
      t -> [sc | t]
    end
  end

  defp strip_right([c | s], sc), do: [c | strip_right(s, sc)]
  defp strip_right([], sc) when is_integer(sc), do: []

  def left(string, len) when is_integer(len), do: left(string, len, ?\s)

  def left(string, len, char) when is_integer(len) and is_integer(char) do
    slen = :erlang.length(string)

    cond do
      slen > len -> substr(string, 1, len)
      slen < len -> l_pad(string, len - slen, char)
      true -> string
    end
  end

  defp l_pad(string, num, char), do: string ++ chars(char, num)

  def right(string, len) when is_integer(len), do: right(string, len, ?\s)

  def right(string, len, char) when is_integer(len) and is_integer(char) do
    slen = :erlang.length(string)

    cond do
      slen > len -> substr(string, slen - len + 1)
      slen < len -> r_pad(string, len - slen, char)
      true -> string
    end
  end

  defp r_pad(string, num, char), do: chars(char, num, string)

  def centre(string, len) when is_integer(len), do: centre(string, len, ?\s)

  def centre(string, 0, char) when is_list(string) and is_integer(char), do: []

  def centre(string, len, char) when is_integer(len) and is_integer(char) do
    slen = :erlang.length(string)

    cond do
      slen > len ->
        substr(string, div(slen - len, 2) + 1, len)

      slen < len ->
        n = div(len - slen, 2)
        r_pad(l_pad(string, len - (slen + n), char), n, char)

      true ->
        string
    end
  end

  def sub_string(string, start), do: substr(string, start)

  def sub_string(string, start, stop) when is_integer(start) and is_integer(stop),
    do: substr(string, start, stop - start + 1)

  defp to_lower_char(c) when is_integer(c) and c >= ?A and c <= ?Z, do: c + 32
  defp to_lower_char(c) when is_integer(c) and c >= 0xC0 and c <= 0xD6, do: c + 32
  defp to_lower_char(c) when is_integer(c) and c >= 0xD8 and c <= 0xDE, do: c + 32
  defp to_lower_char(c), do: c

  defp to_upper_char(c) when is_integer(c) and c >= ?a and c <= ?z, do: c - 32
  defp to_upper_char(c) when is_integer(c) and c >= 0xE0 and c <= 0xF6, do: c - 32
  defp to_upper_char(c) when is_integer(c) and c >= 0xF8 and c <= 0xFE, do: c - 32
  defp to_upper_char(c), do: c

  def to_lower(s) when is_list(s), do: for(c <- s, do: to_lower_char(c))
  def to_lower(c) when is_integer(c), do: to_lower_char(c)

  def to_upper(s) when is_list(s), do: for(c <- s, do: to_upper_char(c))
  def to_upper(c) when is_integer(c), do: to_upper_char(c)

  def join([], sep) when is_list(sep), do: []
  def join([h | t], sep), do: h ++ :lists.append(for x <- t, do: sep ++ x)
end

defmodule :unicode_util do
  def gc(cd), do: :string.__gc__(cd)

  def cp(<<c::utf8, r::binary>>), do: [c | r]
  def cp(<<>>), do: []
  def cp(bin) when is_binary(bin), do: {:error, bin}
  def cp([c | _] = l) when is_integer(c), do: l
  def cp([]), do: []
  def cp([h | t]), do: (case cp(h) do
    [] -> cp(t)
    [c | r] -> [c | (if r == [] or r == "", do: t, else: [r | t])]
    e -> e
  end)
end

defmodule String.Break do
  def split(string), do: :tonic.str_split_ws(string)
  def trim_leading(string), do: :tonic.str_trim(string, 1)
  def trim_trailing(string), do: :tonic.str_trim(string, 2)
end

defmodule String.Unicode do
  def upcase(string, _acc, mode), do: :tonic.ucase(string, true, mode_n(mode))
  def downcase(string, _acc, mode), do: :tonic.ucase(string, false, mode_n(mode))
  defp mode_n(:greek), do: 1
  defp mode_n(:turkic), do: 2
  defp mode_n(_), do: 0
end

defmodule :elixir_utils do
  def relative_to_cwd(path) do
    try do
      if :elixir_config.get(:relative_paths, true), do: Path.relative_to_cwd(path), else: path
    catch
      _, _ -> path
    end
  end

  def get_line(opts) when is_list(opts) do
    case :lists.keyfind(:line, 1, opts) do
      {:line, line} when is_integer(line) and line >= 0 -> line
      _ -> 0
    end
  end

  def meta_keep(meta) do
    case :lists.keyfind(:keep, 1, meta) do
      {:keep, {_, int}} -> [{:line, int} | :lists.keydelete(:line, 1, meta)]
      _ -> meta
    end
  end

  def characters_to_list(data) when is_list(data), do: data

  def characters_to_list(data) do
    case :unicode.characters_to_list(data) do
      result when is_list(result) -> result
      {:error, encoded, rest} -> raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :invalid
      {:incomplete, encoded, rest} -> raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :incomplete
    end
  end

  def characters_to_binary(data) when is_binary(data), do: data

  def characters_to_binary(data) do
    case :unicode.characters_to_binary(data) do
      result when is_binary(result) -> result
      {:error, encoded, rest} -> raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :invalid
      {:incomplete, encoded, rest} -> raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :incomplete
    end
  end

  def split_last([]), do: {[], []}
  def split_last(list), do: split_last(list, [])
  defp split_last([h], acc), do: {:lists.reverse(acc), h}
  defp split_last([h | t], acc), do: split_last(t, [h | acc])

  # Port of elixir_utils:jaro_similarity/2.
  def jaro_similarity(a0, b0) do
    {a, alen} = gcl_and_length(:unicode_util.gc(a0), [], 0)
    {b, blen} = str_to_indexmap(b0)
    dist = div(max(alen, blen), 2)
    {am, bm} = jaro_match(a, b, -dist, dist, [], [])

    cond do
      alen == 0 and blen == 0 -> 1.0
      alen == 0 or blen == 0 -> 0.0
      am == [] -> 0.0
      true ->
        {m, t} = jaro_calc_mt(am, bm, 0, 0)
        (m / alen + m / blen + (m - t / 2) / m) / 3
    end
  end

  defp jaro_match([a | as], b0, min, max, am, bm) do
    case jaro_detect(Map.get(b0, a, []), min, max) do
      false -> jaro_match(as, b0, min + 1, max + 1, am, bm)
      {j, remain} -> jaro_match(as, Map.put(b0, a, remain), min + 1, max + 1, [a | am], add_rsorted({j, a}, bm))
    end
  end

  defp jaro_match(_a, _b, _min, _max, am, bm), do: {am, bm}

  defp jaro_detect([idx | rest], min, max) when min < idx and idx < max, do: {idx, rest}
  defp jaro_detect([idx | rest], min, max) when idx < max, do: jaro_detect(rest, min, max)
  defp jaro_detect(_, _, _), do: false

  defp jaro_calc_mt([c | am], [{_, c} | bm], m, t), do: jaro_calc_mt(am, bm, m + 1, t)
  defp jaro_calc_mt([_ | am], [_ | bm], m, t), do: jaro_calc_mt(am, bm, m + 1, t + 1)
  defp jaro_calc_mt([], [], m, t), do: {m, t}

  defp gcl_and_length([c | str], acc, n), do: gcl_and_length(:unicode_util.gc(str), [c | acc], n + 1)
  defp gcl_and_length([], acc, n), do: {:lists.reverse(acc), n}
  defp gcl_and_length({:error, err}, _, _), do: :erlang.error({:badarg, err})

  defp str_to_indexmap(s) do
    [m | l] = str_to_map(:unicode_util.gc(s), 0)
    {m, l}
  end

  defp str_to_map([], l), do: [%{} | l]

  defp str_to_map([g | gs], i) do
    [m | l] = str_to_map(:unicode_util.gc(gs), i + 1)
    [Map.put(m, g, [i | Map.get(m, g, [])]) | l]
  end

  defp str_to_map({:error, error}, _), do: :erlang.error({:badarg, error})

  defp add_rsorted(a, [h | _] = bm) when a > h, do: [a | bm]
  defp add_rsorted(a, [h | bm]), do: [h | add_rsorted(a, bm)]
  defp add_rsorted(a, []), do: [a]
end
