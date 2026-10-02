# Ports of OTP 27.3 stdlib data-structure modules: :queue, :proplists,
# :orddict, :ordsets, :sets, :dict, :gb_trees and :gb_sets.
# Modified for Tonic; Erlang/OTP 27.3 source/port. Apache-2.0; see licenses/sources.json and notice.
# The internal representations and algorithms follow the OTP sources exactly,
# so the raw terms (as seen by IO.inspect) match those built by Erlang/OTP.

defmodule Tonic.ErlangDS do
  @moduledoc false
  # Shared helpers: ports of the :lists functions used by the modules below
  # (with Erlang semantics, e.g. `==` for usort), and erlang:phash/2.

  # lists:split/2
  def lsplit(n, l), do: lsplit(n, l, [])
  defp lsplit(0, l, acc), do: {:lists.reverse(acc), l}
  defp lsplit(n, [h | t], acc), do: lsplit(n - 1, t, [h | acc])

  # lists:filtermap/2
  def filtermap(f, [h | t]) do
    case f.(h) do
      true -> [h | filtermap(f, t)]
      {true, v} -> [v | filtermap(f, t)]
      false -> filtermap(f, t)
    end
  end

  def filtermap(f, []) when is_function(f, 1), do: []

  def foldl(f, acc, [h | t]), do: foldl(f, f.(h, acc), t)
  def foldl(f, acc, []) when is_function(f, 2), do: acc

  def foldr(f, acc, [h | t]), do: f.(h, foldr(f, acc, t))
  def foldr(f, acc, []) when is_function(f, 2), do: acc

  def any(pred, [h | t]) do
    case pred.(h) do
      true -> true
      false -> any(pred, t)
    end
  end

  def any(pred, []) when is_function(pred, 1), do: false

  def all(pred, [h | t]) do
    case pred.(h) do
      true -> all(pred, t)
      false -> false
    end
  end

  def all(pred, []) when is_function(pred, 1), do: true

  # Stable merge sort with an `le` predicate.
  def msort([], _le), do: []
  def msort([_] = l, _le), do: l

  def msort(l, le) do
    {a, b} = lsplit(div(length(l), 2), l)
    mmerge(msort(a, le), msort(b, le), le)
  end

  defp mmerge([], b, _le), do: b
  defp mmerge(a, [], _le), do: a

  defp mmerge([x | xs] = a, [y | ys] = b, le) do
    if le.(x, y), do: [x | mmerge(xs, b, le)], else: [y | mmerge(a, ys, le)]
  end

  # lists:usort/1 (removes elements comparing == to an earlier kept one)
  def usort(l), do: udedup(msort(l, fn a, b -> a <= b end))

  defp udedup([x, y | t]) when x == y, do: udedup([x | t])
  defp udedup([x | t]), do: [x | udedup(t)]
  defp udedup([]), do: []

  # lists:ukeysort(1, L)
  def ukeysort1(l), do: ukdedup(msort(l, fn a, b -> elem(a, 0) <= elem(b, 0) end))

  defp ukdedup([x, y | t]) when elem(x, 0) == elem(y, 0), do: ukdedup([x | t])
  defp ukdedup([x | t]), do: [x | ukdedup(t)]
  defp ukdedup([]), do: []

  # lists:umerge/1
  def umerge(lists), do: usort(concat(lists))

  defp concat([l | ls]), do: l ++ concat(ls)
  defp concat([]), do: []

  # ---------------------------------------------------------------------
  # erlang:phash/2 -- a port of make_hash() from erl_term_hashing.c (OTP 27)

  @f1 268_440_163
  @f2 268_439_161
  @f3 268_435_459
  @f4 268_436_141
  @f6 268_437_017
  @f8 268_437_511
  @f9 268_439_627
  @f12 268_440_581
  @f13 268_440_593
  @f14 268_440_611
  @m32 0xFFFFFFFF
  @max_small 576_460_752_303_423_487
  @min_small -576_460_752_303_423_488

  def phash(term, range) when is_integer(range) and range > 0 and range <= @m32 do
    rem(make_hash(term), range) + 1
  end

  def phash(term, range) when range === 0 or range === 4_294_967_296 do
    make_hash(term) + 1
  end

  def phash(term, range), do: :erlang.error(:badarg, [term, range])

  def make_hash(term), do: m32(mh(term, 0))

  defp m32(x), do: :erlang.band(x, @m32)

  defp mh([], h), do: m32(h * @f3 + 1)
  defp mh(t, h) when is_atom(t), do: m32(h * @f1 + atom_hval(t))

  defp mh(t, h) when is_integer(t) and t >= @min_small and t <= @max_small do
    y2 = abs(t)
    h = step(h, m32(y2), @f2)
    h = if :erlang.bsr(y2, 32) != 0, do: step(h, :erlang.bsr(y2, 32), @f2), else: h
    m32(h * if(t < 0, do: @f4, else: @f3))
  end

  defp mh(t, h) when is_integer(t) do
    h = mh_big(abs(t), h)
    m32(h * if(t < 0, do: @f4, else: @f3))
  end

  defp mh(t, h) when is_float(t) do
    x =
      if t == 0.0 do
        0
      else
        <<w::64>> = <<t::float>>
        :erlang.bxor(:erlang.bsr(w, 32), m32(w))
      end

    m32(h * @f6 + x)
  end

  defp mh(t, h) when is_bitstring(t) do
    n = bit_size(t)
    nb = div(n, 8)
    tb = rem(n, 8)
    <<bytes::binary-size(nb), rest::bitstring>> = t
    h = hash_bytes(:erlang.binary_to_list(bytes), h)

    h =
      if tb > 0 do
        <<b::size(tb)>> = rest
        m32(m32(h * @f1 + b) * @f12 + tb)
      else
        h
      end

    m32(h * @f4 + nb)
  end

  defp mh([_ | _] = l, h), do: mh_list(l, h)

  defp mh(t, h) when is_tuple(t) do
    h = mh_tuple(t, 0, tuple_size(t), h)
    m32(h * @f9 + tuple_size(t))
  end

  # Maps use make_hash2 on the BEAM; pids/refs/funs depend on VM internals.
  # These are not reproduced exactly.
  defp mh(t, h) when is_map(t), do: m32(h * @f13 + @f14 + :erlang.phash2(t))
  defp mh(t, h), do: m32(h * @f1 + :erlang.phash2(t))

  defp mh_tuple(_t, i, n, h) when i >= n, do: h
  defp mh_tuple(t, i, n, h), do: mh_tuple(t, i + 1, n, mh(elem(t, i), h))

  defp mh_list([e | rest], h) do
    h =
      if is_integer(e) and e >= 0 and e <= 255 do
        m32(h * @f2 + e)
      else
        mh(e, h)
      end

    case rest do
      [_ | _] -> mh_list(rest, h)
      _ -> m32(mh(rest, h) * @f8)
    end
  end

  defp step(h, x, p) do
    h = m32(h * p + :erlang.band(x, 0xFF))
    h = m32(h * p + :erlang.band(:erlang.bsr(x, 8), 0xFF))
    h = m32(h * p + :erlang.band(:erlang.bsr(x, 16), 0xFF))
    m32(h * p + :erlang.bsr(x, 24))
  end

  # Bignum digits are 64 bits, hashed bytewise from the least significant;
  # the top digit contributes only 4 bytes when its upper half is zero.
  defp mh_big(v, h) when v > 0xFFFFFFFFFFFFFFFF do
    h = bytes_step(v, 8, h)
    mh_big(:erlang.bsr(v, 64), h)
  end

  defp mh_big(d, h) do
    if :erlang.bsr(d, 32) == 0, do: bytes_step(d, 4, h), else: bytes_step(d, 8, h)
  end

  defp bytes_step(_d, 0, h), do: h
  defp bytes_step(d, k, h), do: bytes_step(:erlang.bsr(d, 8), k - 1, m32(h * @f2 + :erlang.band(d, 0xFF)))

  defp hash_bytes([b | bs], h), do: hash_bytes(bs, m32(h * @f1 + b))
  defp hash_bytes([], h), do: h

  # atom_hash() from erl_atom_table: hashpjw over the name bytes with the
  # "latin1 clutch" for 2-byte UTF-8 sequences.
  defp atom_hval(a), do: pjw(:erlang.binary_to_list(Atom.to_string(a)), 0)

  defp pjw([v, n | rest], h)
       when :erlang.band(v, 0xFE) == 0xC2 and :erlang.band(n, 0xC0) == 0x80 do
    v2 = :erlang.band(:erlang.bor(:erlang.bsl(v, 6), :erlang.band(n, 0x3F)), 0xFF)
    pjw(rest, pjw_step(h, v2))
  end

  defp pjw([v | rest], h), do: pjw(rest, pjw_step(h, v))
  defp pjw([], h), do: h

  defp pjw_step(h, v) do
    h = :erlang.bsl(h, 4) + v
    g = :erlang.band(h, 0xF0000000)

    if g != 0 do
      :erlang.bxor(:erlang.bxor(h, :erlang.bsr(g, 24)), g)
    else
      h
    end
  end

  # ---------------------------------------------------------------------
  # Linear hash table machinery shared by sets (v1) and dict. Both records
  # have the same field layout:
  #   {Tag, size, n, maxn, bso, exp_size, con_size, empty, segs}

  @seg_size 16
  @expand_load 5
  @contract_load 3

  def mk_seg, do: :erlang.make_tuple(@seg_size, [])

  def get_slot(t, key) do
    h = phash(key, elem(t, 3))
    if h > elem(t, 2), do: h - elem(t, 4), else: h
  end

  def get_bucket(t, slot), do: get_bucket_s(elem(t, 8), slot)

  def get_bucket_s(segs, slot) do
    seg_i = div(slot - 1, @seg_size) + 1
    bkt_i = rem(slot - 1, @seg_size) + 1
    elem(elem(segs, seg_i - 1), bkt_i - 1)
  end

  def put_bucket_s(segs, slot, bkt) do
    seg_i = div(slot - 1, @seg_size) + 1
    bkt_i = rem(slot - 1, @seg_size) + 1
    seg = put_elem(elem(segs, seg_i - 1), bkt_i - 1, bkt)
    put_elem(segs, seg_i - 1, seg)
  end

  # maybe_expand for sets (ic = 1) and dict's maybe_expand_aux.
  def maybe_expand(t0, ic) do
    if elem(t0, 1) + ic > elem(t0, 5) do
      t = maybe_expand_segs(t0)
      {tag, size, n0, maxn, bso, _, _, empty, segs0} = t
      n = n0 + 1
      slot1 = n - bso
      b = get_bucket_s(segs0, slot1)
      slot2 = n
      {b1, b2} = rehash(tag, b, slot1, slot2, maxn)
      segs1 = put_bucket_s(segs0, slot1, b1)
      segs2 = put_bucket_s(segs1, slot2, b2)
      {tag, size + ic, n, maxn, bso, n * @expand_load, n * @contract_load, empty, segs2}
    else
      put_elem(t0, 1, elem(t0, 1) + ic)
    end
  end

  defp maybe_expand_segs({tag, size, n, maxn, bso, es, cs, empty, segs}) when n === maxn do
    {tag, size, n, 2 * maxn, 2 * bso, es, cs, empty, expand_segs(segs, empty)}
  end

  defp maybe_expand_segs(t), do: t

  def maybe_contract({tag, size, n, maxn, bso, _es, con_size, empty, segs0}, dc)
      when size - dc < con_size and n > @seg_size do
    slot1 = n - bso
    b1 = get_bucket_s(segs0, slot1)
    slot2 = n
    b2 = get_bucket_s(segs0, slot2)
    segs1 = put_bucket_s(segs0, slot1, b1 ++ b2)
    segs2 = put_bucket_s(segs1, slot2, [])
    n1 = n - 1

    maybe_contract_segs(
      {tag, size - dc, n1, maxn, bso, n1 * @expand_load, n1 * @contract_load, empty, segs2}
    )
  end

  def maybe_contract(t, dc), do: put_elem(t, 1, elem(t, 1) - dc)

  defp maybe_contract_segs({tag, size, n, maxn, bso, es, cs, empty, segs}) when n === bso do
    {tag, size, n, div(maxn, 2), div(bso, 2), es, cs, empty, contract_segs(segs)}
  end

  defp maybe_contract_segs(t), do: t

  defp rehash(tag, [e | t], slot1, slot2, maxn) do
    {l1, l2} = rehash(tag, t, slot1, slot2, maxn)
    key = if tag === :dict, do: hd(e), else: e
    h = phash(key, maxn)

    cond do
      h === slot1 -> {[e | l1], l2}
      h === slot2 -> {l1, [e | l2]}
    end
  end

  defp rehash(_tag, [], _, _, _), do: {[], []}

  defp expand_segs(segs, empty) do
    :erlang.list_to_tuple(
      :erlang.tuple_to_list(segs) ++ List.duplicate(empty, tuple_size(segs))
    )
  end

  defp contract_segs(segs) do
    ss = div(tuple_size(segs), 2)
    :erlang.list_to_tuple(Enum.take(:erlang.tuple_to_list(segs), ss))
  end

  # Fold over all buckets, last segment / last bucket first.
  def fold_segs(f, acc, segs, i) when i >= 1 do
    seg = elem(segs, i - 1)
    fold_segs(f, fold_seg(f, acc, seg, tuple_size(seg)), segs, i - 1)
  end

  def fold_segs(_f, acc, _, _), do: acc

  defp fold_seg(f, acc, seg, i) when i >= 1 do
    fold_seg(f, f.(acc, elem(seg, i - 1)), seg, i - 1)
  end

  defp fold_seg(_f, acc, _, _), do: acc
end

defmodule :queue do
  alias Tonic.ErlangDS, as: L

  def new, do: {[], []}

  def is_queue({r, f}) when is_list(r) and is_list(f), do: true
  def is_queue(_), do: false

  def is_empty({[], []}), do: true
  def is_empty({i, o}) when is_list(i) and is_list(o), do: false
  def is_empty(q), do: :erlang.error(:badarg, [q])

  def len({r, f}) when is_list(r) and is_list(f), do: length(r) + length(f)
  def len(q), do: :erlang.error(:badarg, [q])

  def to_list({i, o}) when is_list(i) and is_list(o), do: o ++ :lists.reverse(i, [])
  def to_list(q), do: :erlang.error(:badarg, [q])

  def from_list(l) when is_list(l), do: f2r(l)
  def from_list(l), do: :erlang.error(:badarg, [l])

  def member(x, {r, f}) when is_list(r) and is_list(f) do
    :lists.member(x, r) or :lists.member(x, f)
  end

  def member(x, q), do: :erlang.error(:badarg, [x, q])

  # queue:in/2 -- `in` is a reserved word in Elixir, so `def in(...)` does not
  # parse in tonic yet. Enable this once the parser accepts it:
  def in(x, q), do: in_(x, q)
  defp in_(x, {[_] = i, []}), do: {[x], i}
  defp in_(x, {i, o}) when is_list(i) and is_list(o), do: {[x | i], o}
  defp in_(x, q), do: :erlang.error(:badarg, [x, q])

  def in_r(x, {[], [_] = f}), do: {f, [x]}
  def in_r(x, {r, f}) when is_list(r) and is_list(f), do: {r, [x | f]}
  def in_r(x, q), do: :erlang.error(:badarg, [x, q])

  def out({[], []} = q), do: {:empty, q}
  def out({[v], []}), do: {{:value, v}, {[], []}}

  def out({[y | i], []}) do
    [v | o] = :lists.reverse(i, [])
    {{:value, v}, {[y], o}}
  end

  def out({i, [v]}) when is_list(i), do: {{:value, v}, r2f(i)}
  def out({i, [v | o]}) when is_list(i), do: {{:value, v}, {i, o}}
  def out(q), do: :erlang.error(:badarg, [q])

  def out_r({[], []} = q), do: {:empty, q}
  def out_r({[], [v]}), do: {{:value, v}, {[], []}}

  def out_r({[], [y | o]}) do
    [v | i] = :lists.reverse(o, [])
    {{:value, v}, {i, [y]}}
  end

  def out_r({[v], o}) when is_list(o), do: {{:value, v}, f2r(o)}
  def out_r({[v | i], o}) when is_list(o), do: {{:value, v}, {i, o}}
  def out_r(q), do: :erlang.error(:badarg, [q])

  def get({[], []} = q), do: :erlang.error(:empty, [q])
  def get({r, f}) when is_list(r) and is_list(f), do: get(r, f)
  def get(q), do: :erlang.error(:badarg, [q])

  defp get(r, [h | _]) when is_list(r), do: h
  defp get([h], []), do: h
  defp get([_ | r], []), do: :lists.last(r)

  def get_r({[], []} = q), do: :erlang.error(:empty, [q])
  def get_r({[h | _], f}) when is_list(f), do: h
  def get_r({[], [h]}), do: h
  def get_r({[], [_ | f]}), do: :lists.last(f)
  def get_r(q), do: :erlang.error(:badarg, [q])

  def peek({[], []}), do: :empty
  def peek({r, [h | _]}) when is_list(r), do: {:value, h}
  def peek({[h], []}), do: {:value, h}
  def peek({[_ | r], []}), do: {:value, :lists.last(r)}
  def peek(q), do: :erlang.error(:badarg, [q])

  def peek_r({[], []}), do: :empty
  def peek_r({[h | _], f}) when is_list(f), do: {:value, h}
  def peek_r({[], [h]}), do: {:value, h}
  def peek_r({[], [_ | r]}), do: {:value, :lists.last(r)}
  def peek_r(q), do: :erlang.error(:badarg, [q])

  def drop({[], []} = q), do: :erlang.error(:empty, [q])
  def drop({[_], []}), do: {[], []}

  def drop({[y | r], []}) do
    [_ | f] = :lists.reverse(r, [])
    {[y], f}
  end

  def drop({r, [_]}) when is_list(r), do: r2f(r)
  def drop({r, [_ | f]}) when is_list(r), do: {r, f}
  def drop(q), do: :erlang.error(:badarg, [q])

  def drop_r({[], []} = q), do: :erlang.error(:empty, [q])
  def drop_r({[], [_]}), do: {[], []}

  def drop_r({[], [y | f]}) do
    [_ | r] = :lists.reverse(f, [])
    {r, [y]}
  end

  def drop_r({[_], f}) when is_list(f), do: f2r(f)
  def drop_r({[_ | r], f}) when is_list(f), do: {r, f}
  def drop_r(q), do: :erlang.error(:badarg, [q])

  def reverse({r, f}) when is_list(r) and is_list(f), do: {f, r}
  def reverse(q), do: :erlang.error(:badarg, [q])

  def join({r, f} = q, {[], []}) when is_list(r) and is_list(f), do: q
  def join({[], []}, {r, f} = q) when is_list(r) and is_list(f), do: q

  def join({r1, f1}, {r2, f2})
      when is_list(r1) and is_list(f1) and is_list(r2) and is_list(f2) do
    {r2, f1 ++ :lists.reverse(r1, f2)}
  end

  def join(q1, q2), do: :erlang.error(:badarg, [q1, q2])

  def split(0, {r, f} = q) when is_list(r) and is_list(f), do: {{[], []}, q}

  def split(n, {r, f} = q) when is_integer(n) and n >= 1 and is_list(r) and is_list(f) do
    lf = length(f)

    cond do
      n < lf ->
        [x | f1] = f
        split_f1_to_r2(n - 1, r, f1, [], [x])

      n > lf ->
        lr = length(r)
        m = lr - (n - lf)

        cond do
          m < 0 ->
            :erlang.error(:badarg, [n, q])

          m > 0 ->
            [x | r1] = r
            split_r1_to_f2(m - 1, r1, f, [x], [])

          true ->
            {q, {[], []}}
        end

      true ->
        {f2r(f), r2f(r)}
    end
  end

  def split(n, q), do: :erlang.error(:badarg, [n, q])

  defp split_f1_to_r2(0, r1, f1, r2, f2), do: {{r2, f2}, {r1, f1}}
  defp split_f1_to_r2(n, r1, [x | f1], r2, f2), do: split_f1_to_r2(n - 1, r1, f1, [x | r2], f2)

  defp split_r1_to_f2(0, r1, f1, r2, f2), do: {{r1, f1}, {r2, f2}}
  defp split_r1_to_f2(n, [x | r1], f1, r2, f2), do: split_r1_to_f2(n - 1, r1, f1, r2, [x | f2])

  def filter(fun, {r0, f0}) when is_function(fun, 1) and is_list(r0) and is_list(f0) do
    f = filter_f(fun, f0)
    r = filter_r(fun, r0)

    cond do
      r === [] -> f2r(f)
      f === [] -> r2f(r)
      true -> {r, f}
    end
  end

  def filter(fun, q), do: :erlang.error(:badarg, [fun, q])

  defp filter_f(_, []), do: []

  defp filter_f(fun, [x | f]) do
    case fun.(x) do
      true -> [x | filter_f(fun, f)]
      [y] -> [y | filter_f(fun, f)]
      false -> filter_f(fun, f)
      [] -> filter_f(fun, f)
      l when is_list(l) -> l ++ filter_f(fun, f)
    end
  end

  defp filter_r(_, []), do: []

  defp filter_r(fun, [x | r0]) do
    r = filter_r(fun, r0)

    case fun.(x) do
      true -> [x | r]
      [y] -> [y | r]
      false -> r
      [] -> r
      l when is_list(l) -> :lists.reverse(l, r)
    end
  end

  def filtermap(fun, {r0, f0}) when is_function(fun, 1) and is_list(r0) and is_list(f0) do
    f = L.filtermap(fun, f0)
    r = filtermap_r(fun, r0)

    cond do
      r === [] -> f2r(f)
      f === [] -> r2f(r)
      true -> {r, f}
    end
  end

  def filtermap(fun, q), do: :erlang.error(:badarg, [fun, q])

  defp filtermap_r(_, []), do: []

  defp filtermap_r(fun, [x | r0]) do
    r = filtermap_r(fun, r0)

    case fun.(x) do
      true -> [x | r]
      {true, y} -> [y | r]
      false -> r
    end
  end

  def fold(fun, acc0, {r, f}) when is_function(fun, 2) and is_list(r) and is_list(f) do
    acc1 = L.foldl(fun, acc0, f)
    L.foldr(fun, acc1, r)
  end

  def fold(fun, acc0, q), do: :erlang.error(:badarg, [fun, acc0, q])

  def any(pred, {r, f}) when is_function(pred, 1) and is_list(r) and is_list(f) do
    L.any(pred, f) or L.any(pred, r)
  end

  def any(pred, q), do: :erlang.error(:badarg, [pred, q])

  def all(pred, {r, f}) when is_function(pred, 1) and is_list(r) and is_list(f) do
    L.all(pred, f) and L.all(pred, r)
  end

  def all(pred, q), do: :erlang.error(:badarg, [pred, q])

  def delete(item, {r0, f0} = q) when is_list(r0) and is_list(f0) do
    case delete_front(item, f0) do
      false ->
        case delete_rear(item, r0) do
          false -> q
          [] -> f2r(f0)
          r1 -> {r1, f0}
        end

      [] ->
        r2f(r0)

      f1 ->
        {r0, f1}
    end
  end

  def delete(item, q), do: :erlang.error(:badarg, [item, q])

  def delete_r(item, {r0, f0}) when is_list(r0) and is_list(f0) do
    {f1, r1} = delete(item, {f0, r0})
    {r1, f1}
  end

  def delete_r(item, q), do: :erlang.error(:badarg, [item, q])

  defp delete_front(item, [item | rest]), do: rest

  defp delete_front(item, [x | rest]) do
    case delete_front(item, rest) do
      false -> false
      f -> [x | f]
    end
  end

  defp delete_front(_, []), do: false

  defp delete_rear(item, [x | rest]) do
    case delete_rear(item, rest) do
      false when x === item -> rest
      false -> false
      r -> [x | r]
    end
  end

  defp delete_rear(_, []), do: false

  def delete_with(pred, {r0, f0} = q) when is_function(pred, 1) and is_list(r0) and is_list(f0) do
    case delete_with_front(pred, f0) do
      false ->
        case delete_with_rear(pred, r0) do
          false -> q
          [] -> f2r(f0)
          r1 -> {r1, f0}
        end

      [] ->
        r2f(r0)

      f1 ->
        {r0, f1}
    end
  end

  def delete_with(pred, q), do: :erlang.error(:badarg, [pred, q])

  def delete_with_r(pred, {r0, f0}) when is_function(pred, 1) and is_list(r0) and is_list(f0) do
    {f1, r1} = delete_with(pred, {f0, r0})
    {r1, f1}
  end

  def delete_with_r(pred, q), do: :erlang.error(:badarg, [pred, q])

  defp delete_with_front(pred, [x | rest]) do
    case pred.(x) do
      true ->
        rest

      false ->
        case delete_with_front(pred, rest) do
          false -> false
          f -> [x | f]
        end
    end
  end

  defp delete_with_front(_, []), do: false

  defp delete_with_rear(pred, [x | rest]) do
    case delete_with_rear(pred, rest) do
      false ->
        case pred.(x) do
          true -> rest
          false -> false
        end

      r ->
        [x | r]
    end
  end

  defp delete_with_rear(_, []), do: false

  # Okasaki API
  def cons(x, q), do: in_r(x, q)

  def head({[], []} = q), do: :erlang.error(:empty, [q])
  def head({r, f}) when is_list(r) and is_list(f), do: get(r, f)
  def head(q), do: :erlang.error(:badarg, [q])

  def tail(q), do: drop(q)
  def snoc(q, x), do: in_(x, q)
  def daeh(q), do: get_r(q)
  def last(q), do: get_r(q)
  def liat(q), do: drop_r(q)
  def lait(q), do: drop_r(q)
  def init(q), do: drop_r(q)

  defp r2f([]), do: {[], []}
  defp r2f([_] = r), do: {[], r}
  defp r2f([y, x]), do: {[y], [x]}

  defp r2f(list) do
    {rr, ff} = L.lsplit(div(length(list), 2), list)
    {rr, :lists.reverse(ff, [])}
  end

  defp f2r([]), do: {[], []}
  defp f2r([_] = f), do: {f, []}
  defp f2r([x, y]), do: {[y], [x]}

  defp f2r(list) do
    {ff, rr} = L.lsplit(div(length(list), 2), list)
    {:lists.reverse(rr, []), ff}
  end
end

defmodule :proplists do
  def property({key, true}) when is_atom(key), do: key
  def property(property), do: property

  def property(key, true) when is_atom(key), do: key
  def property(key, value), do: {key, value}

  def unfold([p | ps]) when is_atom(p), do: [{p, true} | unfold(ps)]
  def unfold([p | ps]), do: [p | unfold(ps)]
  def unfold([]), do: []

  def compact(list_in), do: for(p <- list_in, do: property(p))

  def lookup(key, [p | _]) when is_atom(p) and p === key, do: {key, true}
  def lookup(key, [p | _]) when tuple_size(p) >= 1 and elem(p, 0) === key, do: p
  def lookup(key, [_ | ps]), do: lookup(key, ps)
  def lookup(_key, []), do: :none

  def lookup_all(key, [p | ps]) when is_atom(p) and p === key, do: [{key, true} | lookup_all(key, ps)]
  def lookup_all(key, [p | ps]) when tuple_size(p) >= 1 and elem(p, 0) === key, do: [p | lookup_all(key, ps)]
  def lookup_all(key, [_ | ps]), do: lookup_all(key, ps)
  def lookup_all(_key, []), do: []

  def is_defined(key, [p | _]) when is_atom(p) and p === key, do: true
  def is_defined(key, [p | _]) when tuple_size(p) >= 1 and elem(p, 0) === key, do: true
  def is_defined(key, [_ | ps]), do: is_defined(key, ps)
  def is_defined(_key, []), do: false

  def get_value(key, list), do: get_value(key, list, :undefined)

  def get_value(key, [p | _], _default) when is_atom(p) and p === key, do: true

  def get_value(key, [p | _], default) when tuple_size(p) >= 1 and elem(p, 0) === key do
    case p do
      {_, value} -> value
      _ -> default
    end
  end

  def get_value(key, [_ | ps], default), do: get_value(key, ps, default)
  def get_value(_key, [], default), do: default

  def get_all_values(key, [p | ps]) when is_atom(p) and p === key, do: [true | get_all_values(key, ps)]

  def get_all_values(key, [p | ps]) when tuple_size(p) >= 1 and elem(p, 0) === key do
    case p do
      {_, value} -> [value | get_all_values(key, ps)]
      _ -> get_all_values(key, ps)
    end
  end

  def get_all_values(key, [_ | ps]), do: get_all_values(key, ps)
  def get_all_values(_key, []), do: []

  def append_values(key, [p | ps]) when is_atom(p) and p === key, do: [true | append_values(key, ps)]

  def append_values(key, [p | ps]) when tuple_size(p) >= 1 and elem(p, 0) === key do
    case p do
      {_, value} when is_list(value) -> value ++ append_values(key, ps)
      {_, value} -> [value | append_values(key, ps)]
      _ -> append_values(key, ps)
    end
  end

  def append_values(key, [_ | ps]), do: append_values(key, ps)
  def append_values(_key, []), do: []

  def get_bool(key, [p | _]) when is_atom(p) and p === key, do: true

  def get_bool(key, [p | _]) when tuple_size(p) >= 1 and elem(p, 0) === key do
    case p do
      {_, true} -> true
      _ -> false
    end
  end

  def get_bool(key, [_ | ps]), do: get_bool(key, ps)
  def get_bool(_key, []), do: false

  def get_keys(ps), do: :sets.to_list(get_keys(ps, :sets.new()))

  defp get_keys([p | ps], keys) when is_atom(p), do: get_keys(ps, :sets.add_element(p, keys))

  defp get_keys([p | ps], keys) when tuple_size(p) >= 1 do
    get_keys(ps, :sets.add_element(elem(p, 0), keys))
  end

  defp get_keys([_ | ps], keys), do: get_keys(ps, keys)
  defp get_keys([], keys), do: keys

  def delete(key, [p | ps]) when is_atom(p) and p === key, do: delete(key, ps)
  def delete(key, [p | ps]) when tuple_size(p) >= 1 and elem(p, 0) === key, do: delete(key, ps)
  def delete(key, [p | ps]), do: [p | delete(key, ps)]
  def delete(_, []), do: []

  def substitute_aliases(as, props), do: for(p <- props, do: substitute_aliases_1(as, p))

  defp substitute_aliases_1([{key, key1} | _], p) when is_atom(p) and p === key do
    property(key1, true)
  end

  defp substitute_aliases_1([{key, key1} | _], p) when tuple_size(p) >= 1 and elem(p, 0) === key do
    property(put_elem(p, 0, key1))
  end

  defp substitute_aliases_1([_ | as], p), do: substitute_aliases_1(as, p)
  defp substitute_aliases_1([], p), do: p

  def substitute_negations(as, props), do: for(p <- props, do: substitute_negations_1(as, p))

  defp substitute_negations_1([{key, key1} | _], p) when is_atom(p) and p === key do
    property(key1, false)
  end

  defp substitute_negations_1([{key, key1} | _], p)
       when tuple_size(p) >= 1 and elem(p, 0) === key do
    case p do
      {_, true} -> property(key1, false)
      {_, false} -> property(key1, true)
      _ -> property(key1, true)
    end
  end

  defp substitute_negations_1([_ | as], p), do: substitute_negations_1(as, p)
  defp substitute_negations_1([], p), do: p

  def expand(es, ps) when is_list(ps) do
    es1 = for {p, v} <- es, do: {property(p), v}
    flatten(expand_0(key_uniq(es1), ps))
  end

  defp expand_0([{p, l} | es], ps), do: expand_0(es, expand_1(p, l, ps))
  defp expand_0([], ps), do: ps

  defp expand_1(p, l, ps) when is_atom(p), do: expand_2(p, p, l, ps)
  defp expand_1(p, l, ps) when tuple_size(p) >= 1, do: expand_2(elem(p, 0), p, l, ps)
  defp expand_1(_p, _l, ps), do: ps

  defp expand_2(key, p1, l, [p | ps]) when is_atom(p) and p === key, do: expand_3(key, p1, p, l, ps)

  defp expand_2(key, p1, l, [p | ps]) when tuple_size(p) >= 1 and elem(p, 0) === key do
    expand_3(key, p1, property(p), l, ps)
  end

  defp expand_2(key, p1, l, [p | ps]), do: [p | expand_2(key, p1, l, ps)]
  defp expand_2(_, _, _, []), do: []

  defp expand_3(key, p1, p, l, ps) do
    if p1 === p, do: [l | delete(key, ps)], else: [p | ps]
  end

  defp key_uniq([{k, v} | ps]), do: [{k, v} | key_uniq_1(k, ps)]
  defp key_uniq([]), do: []

  defp key_uniq_1(k, [{k1, v} | ps]) do
    if k === k1, do: key_uniq_1(k, ps), else: [{k1, v} | key_uniq_1(k1, ps)]
  end

  defp key_uniq_1(_, []), do: []

  defp flatten([e | es]) when is_list(e), do: e ++ flatten(es)
  defp flatten([e | es]), do: [e | flatten(es)]
  defp flatten([]), do: []

  def normalize(l, stages), do: compact(apply_stages(l, stages))

  defp apply_stages(l, [{:aliases, as} | xs]), do: apply_stages(substitute_aliases(as, l), xs)
  defp apply_stages(l, [{:expand, es} | xs]), do: apply_stages(expand(es, l), xs)
  defp apply_stages(l, [{:negations, ns} | xs]), do: apply_stages(substitute_negations(ns, l), xs)
  defp apply_stages(l, []), do: l

  def split(list, keys) do
    {store, rest} = split(list, Map.new(keys, fn k -> {k, []} end), [])
    {for(k <- keys, do: :lists.reverse(:erlang.map_get(k, store))), :lists.reverse(rest)}
  end

  defp split([p | ps], store, rest) when is_atom(p) do
    if is_map_key(store, p) do
      split(ps, maps_prepend(p, p, store), rest)
    else
      split(ps, store, [p | rest])
    end
  end

  defp split([p | ps], store, rest) when tuple_size(p) >= 1 do
    key = elem(p, 0)

    if is_map_key(store, key) do
      split(ps, maps_prepend(key, p, store), rest)
    else
      split(ps, store, [p | rest])
    end
  end

  defp split([p | ps], store, rest), do: split(ps, store, [p | rest])
  defp split([], store, rest), do: {store, rest}

  defp maps_prepend(key, val, dict), do: %{dict | key => [val | :erlang.map_get(key, dict)]}

  def to_map(list) do
    Tonic.ErlangDS.foldr(
      fn
        {k, v}, m -> Map.put(m, k, v)
        t, m when is_tuple(t) and tuple_size(t) >= 1 -> Map.delete(m, elem(t, 0))
        k, m when is_atom(k) -> Map.put(m, k, true)
        _, m -> m
      end,
      %{},
      list
    )
  end

  def to_map(list, stages), do: to_map(apply_stages(list, stages))

  def from_map(map), do: :maps.to_list(map)
end

defmodule :orddict do
  def new, do: []

  def is_key(key, [{k, _} | _]) when key < k, do: false
  def is_key(key, [{k, _} | dict]) when key > k, do: is_key(key, dict)
  def is_key(_key, [{_k, _val} | _]), do: true
  def is_key(_, []), do: false

  def to_list(dict), do: dict

  def from_list([]), do: []
  def from_list([{_, _}] = pair), do: pair
  def from_list(pairs), do: Tonic.ErlangDS.ukeysort1(reverse_pairs(pairs, []))

  def size(d), do: length(d)

  def is_empty([]), do: true
  def is_empty([_ | _]), do: false

  def fetch(key, [{k, _} | d]) when key > k, do: fetch(key, d)
  def fetch(key, [{k, value} | _]) when key == k, do: value

  def find(key, [{k, _} | _]) when key < k, do: :error
  def find(key, [{k, _} | d]) when key > k, do: find(key, d)
  def find(_key, [{_k, value} | _]), do: {:ok, value}
  def find(_, []), do: :error

  def fetch_keys([{key, _} | dict]), do: [key | fetch_keys(dict)]
  def fetch_keys([]), do: []

  def erase(key, [{k, _} = e | dict]) when key < k, do: [e | dict]
  def erase(key, [{k, _} = e | dict]) when key > k, do: [e | erase(key, dict)]
  def erase(_key, [{_k, _val} | dict]), do: dict
  def erase(_, []), do: []

  def take(key, dict), do: take_1(key, dict, [])

  defp take_1(key, [{k, _} | _], _acc) when key < k, do: :error
  defp take_1(key, [{k, _} = p | d], acc) when key > k, do: take_1(key, d, [p | acc])
  defp take_1(_key, [{_k, value} | d], acc), do: {value, :lists.reverse(acc, d)}
  defp take_1(_, [], _), do: :error

  def store(key, new, [{k, _} | _] = dict) when key < k, do: [{key, new} | dict]
  def store(key, new, [{k, _} = e | dict]) when key > k, do: [e | store(key, new, dict)]
  def store(key, new, [{_k, _old} | dict]), do: [{key, new} | dict]
  def store(key, new, []), do: [{key, new}]

  def append(key, new, [{k, _} | _] = dict) when key < k, do: [{key, [new]} | dict]
  def append(key, new, [{k, _} = e | dict]) when key > k, do: [e | append(key, new, dict)]
  def append(key, new, [{_k, old} | dict]), do: [{key, old ++ [new]} | dict]
  def append(key, new, []), do: [{key, [new]}]

  def append_list(key, new_list, [{k, _} | _] = dict) when key < k, do: [{key, new_list} | dict]

  def append_list(key, new_list, [{k, _} = e | dict]) when key > k do
    [e | append_list(key, new_list, dict)]
  end

  def append_list(key, new_list, [{_k, old} | dict]), do: [{key, old ++ new_list} | dict]
  def append_list(key, new_list, []), do: [{key, new_list}]

  def update(key, fun, [{k, _} = e | dict]) when key > k, do: [e | update(key, fun, dict)]
  def update(key, fun, [{k, val} | dict]) when key == k, do: [{key, fun.(val)} | dict]

  def update(key, _, init, [{k, _} | _] = dict) when key < k, do: [{key, init} | dict]
  def update(key, fun, init, [{k, _} = e | dict]) when key > k, do: [e | update(key, fun, init, dict)]
  def update(key, fun, _init, [{_k, val} | dict]), do: [{key, fun.(val)} | dict]
  def update(key, _, init, []), do: [{key, init}]

  def update_counter(key, incr, [{k, _} | _] = dict) when key < k, do: [{key, incr} | dict]

  def update_counter(key, incr, [{k, _} = e | dict]) when key > k do
    [e | update_counter(key, incr, dict)]
  end

  def update_counter(key, incr, [{_k, val} | dict]), do: [{key, val + incr} | dict]
  def update_counter(key, incr, []), do: [{key, incr}]

  def fold(f, acc, [{key, val} | d]), do: fold(f, f.(key, val, acc), d)
  def fold(f, acc, []) when is_function(f, 3), do: acc

  def map(f, [{key, val} | d]), do: [{key, f.(key, val)} | map(f, d)]
  def map(f, []) when is_function(f, 2), do: []

  def filter(f, [{key, val} = e | d]) do
    case f.(key, val) do
      true -> [e | filter(f, d)]
      false -> filter(f, d)
    end
  end

  def filter(f, []) when is_function(f, 2), do: []

  def merge(f, [{k1, _} = e1 | d1], [{k2, _} = e2 | d2]) when k1 < k2 do
    [e1 | merge(f, d1, [e2 | d2])]
  end

  def merge(f, [{k1, _} = e1 | d1], [{k2, _} = e2 | d2]) when k1 > k2 do
    [e2 | merge(f, [e1 | d1], d2)]
  end

  def merge(f, [{k1, v1} | d1], [{_k2, v2} | d2]), do: [{k1, f.(k1, v1, v2)} | merge(f, d1, d2)]
  def merge(f, [], d2) when is_function(f, 3), do: d2
  def merge(f, d1, []) when is_function(f, 3), do: d1

  defp reverse_pairs([{_, _} = h | t], acc), do: reverse_pairs(t, [h | acc])
  defp reverse_pairs([], acc), do: acc
end

defmodule :ordsets do
  def new, do: []

  def is_set([e | es]), do: is_set(es, e)
  def is_set([]), do: true
  def is_set(_), do: false

  defp is_set([e2 | es], e1) when e1 < e2, do: is_set(es, e2)
  defp is_set([_ | _], _), do: false
  defp is_set([], _), do: true

  def size(s), do: length(s)

  def is_empty(s), do: s === []

  def is_equal(s1, s2) when is_list(s1) and is_list(s2), do: s1 == s2

  def to_list(s), do: s

  def from_list(l), do: Tonic.ErlangDS.usort(l)

  def is_element(e, [h | es]) when e > h, do: is_element(e, es)
  def is_element(e, [h | _]) when e < h, do: false
  def is_element(_e, [_h | _]), do: true
  def is_element(_, []), do: false

  def add_element(e, [h | es]) when e > h, do: [h | add_element(e, es)]
  def add_element(e, [h | _] = set) when e < h, do: [e | set]
  def add_element(_e, [_h | _] = set), do: set
  def add_element(e, []), do: [e]

  def del_element(e, [h | es]) when e > h, do: [h | del_element(e, es)]
  def del_element(e, [h | _] = set) when e < h, do: set
  def del_element(_e, [_h | es]), do: es
  def del_element(_, []), do: []

  def union([e1 | es1], [e2 | _] = set2) when e1 < e2, do: [e1 | union(es1, set2)]
  def union([e1 | _] = set1, [e2 | es2]) when e1 > e2, do: [e2 | union(es2, set1)]
  def union([e1 | es1], [_e2 | es2]), do: [e1 | union(es1, es2)]
  def union([], es2), do: es2
  def union(es1, []), do: es1

  def union(ordset_list), do: Tonic.ErlangDS.umerge(ordset_list)

  def intersection([e1 | es1], [e2 | _] = set2) when e1 < e2, do: intersection(es1, set2)
  def intersection([e1 | _] = set1, [e2 | es2]) when e1 > e2, do: intersection(es2, set1)
  def intersection([e1 | es1], [_e2 | es2]), do: [e1 | intersection(es1, es2)]
  def intersection([], _), do: []
  def intersection(_, []), do: []

  def intersection([s1, s2 | ss]), do: intersection1(intersection(s1, s2), ss)
  def intersection([s]), do: s

  defp intersection1(s1, [s2 | ss]), do: intersection1(intersection(s1, s2), ss)
  defp intersection1(s1, []), do: s1

  def is_disjoint([e1 | es1], [e2 | _] = set2) when e1 < e2, do: is_disjoint(es1, set2)
  def is_disjoint([e1 | _] = set1, [e2 | es2]) when e1 > e2, do: is_disjoint(es2, set1)
  def is_disjoint([_e1 | _es1], [_e2 | _es2]), do: false
  def is_disjoint([], _), do: true
  def is_disjoint(_, []), do: true

  def subtract([e1 | es1], [e2 | _] = set2) when e1 < e2, do: [e1 | subtract(es1, set2)]
  def subtract([e1 | _] = set1, [e2 | es2]) when e1 > e2, do: subtract(set1, es2)
  def subtract([_e1 | es1], [_e2 | es2]), do: subtract(es1, es2)
  def subtract([], _), do: []
  def subtract(es1, []), do: es1

  def is_subset([e1 | _], [e2 | _]) when e1 < e2, do: false
  def is_subset([e1 | _] = set1, [e2 | es2]) when e1 > e2, do: is_subset(set1, es2)
  def is_subset([_e1 | es1], [_e2 | es2]), do: is_subset(es1, es2)
  def is_subset([], _), do: true
  def is_subset(_, []), do: false

  def fold(f, acc, set), do: Tonic.ErlangDS.foldl(f, acc, set)

  def filter(f, set), do: for(x <- set, f.(x), do: x)

  def map(f, set), do: from_list(for(x <- set, do: f.(x)))

  def filtermap(f, set), do: from_list(Tonic.ErlangDS.filtermap(f, set))
end

defmodule :sets do
  alias Tonic.ErlangDS, as: H

  # Version 1: #set{size, n, maxn, bso, exp_size, con_size, empty, segs}
  # Version 2: #{Element => []}

  def new do
    empty = H.mk_seg()
    {:set, 0, 16, 16, 8, 80, 48, empty, {empty}}
  end

  def new([{:version, 2}]), do: %{}

  def new(opts) do
    case :proplists.get_value(:version, opts, 1) do
      1 -> new()
      2 -> new([{:version, 2}])
    end
  end

  def from_list(ls), do: H.foldl(fn e, s -> add_element(e, s) end, new(), ls)

  def from_list(ls, [{:version, 2}]), do: Map.new(ls, fn k -> {k, []} end)

  def from_list(ls, opts) do
    case :proplists.get_value(:version, opts, 1) do
      1 -> from_list(ls)
      2 -> from_list(ls, [{:version, 2}])
    end
  end

  def is_set(s) when is_map(s), do: true
  def is_set({:set, _, _, _, _, _, _, _, _}), do: true
  def is_set(_), do: false

  def size(s) when is_map(s), do: map_size(s)
  def size({:set, size, _, _, _, _, _, _, _}), do: size

  def is_empty(s) when is_map(s), do: map_size(s) === 0
  def is_empty({:set, size, _, _, _, _, _, _, _}), do: size === 0

  def is_equal(s1, s2) do
    if size(s1) === size(s2) do
      if s1 === s2, do: true, else: canonicalize_v2(s1) === canonicalize_v2(s2)
    else
      false
    end
  end

  defp canonicalize_v2(s), do: from_list(to_list(s), [{:version, 2}])

  def to_list(s) when is_map(s), do: :maps.keys(s)
  def to_list({:set, _, _, _, _, _, _, _, _} = s), do: fold(fn elem, list -> [elem | list] end, [], s)

  def is_element(e, s) when is_map(s), do: is_map_key(s, e)

  def is_element(e, {:set, _, _, _, _, _, _, _, _} = s) do
    slot = H.get_slot(s, e)
    bkt = H.get_bucket(s, slot)
    :lists.member(e, bkt)
  end

  def add_element(e, s) when is_map(s), do: Map.put(s, e, [])

  def add_element(e, {:set, _, _, _, _, _, _, _, _} = s0) do
    slot = H.get_slot(s0, e)
    bkt = H.get_bucket(s0, slot)

    case :lists.member(e, bkt) do
      true ->
        s0

      false ->
        s1 = update_bucket(s0, slot, [e | bkt])
        H.maybe_expand(s1, 1)
    end
  end

  def del_element(e, s) when is_map(s), do: :maps.remove(e, s)

  def del_element(e, {:set, _, _, _, _, _, _, _, _} = s0) do
    slot = H.get_slot(s0, e)
    bkt = H.get_bucket(s0, slot)

    case :lists.member(e, bkt) do
      false ->
        s0

      true ->
        s1 = update_bucket(s0, slot, lists_delete(e, bkt))
        H.maybe_contract(s1, 1)
    end
  end

  # lists:delete/2 (removes the first element that matches exactly)
  defp lists_delete(item, [item | rest]), do: rest
  defp lists_delete(item, [h | rest]), do: [h | lists_delete(item, rest)]
  defp lists_delete(_, []), do: []

  defp update_bucket(set, slot, new_bucket), do: put_elem(set, 8, H.put_bucket_s(elem(set, 8), slot, new_bucket))

  def union(s1, s2) when is_map(s1) and is_map(s2), do: Map.merge(s1, s2)

  def union(s1, s2) do
    if size(s1) < size(s2) do
      fold(fn e, s -> add_element(e, s) end, s2, s1)
    else
      fold(fn e, s -> add_element(e, s) end, s1, s2)
    end
  end

  def union([s1, s2 | ss]), do: union1(union(s1, s2), ss)
  def union([s]), do: s
  def union([]), do: new()

  defp union1(s1, [s2 | ss]), do: union1(union(s1, s2), ss)
  defp union1(s1, []), do: s1

  # The v2 result is a map, so the OTP heuristics (which only affect the
  # amount of work done) are not needed to produce an identical term.
  def intersection(s1, s2) when is_map(s1) and is_map(s2) do
    {small, big} = if map_size(s1) < map_size(s2), do: {s1, s2}, else: {s2, s1}
    Map.new(for({k, _} <- :maps.to_list(small), is_map_key(big, k), do: {k, []}))
  end

  def intersection(s1, s2) do
    if size(s1) < size(s2) do
      filter(fn e -> is_element(e, s2) end, s1)
    else
      filter(fn e -> is_element(e, s1) end, s2)
    end
  end

  def intersection([s1, s2 | ss]), do: intersection1(intersection(s1, s2), ss)
  def intersection([s]), do: s

  defp intersection1(s1, [s2 | ss]), do: intersection1(intersection(s1, s2), ss)
  defp intersection1(s1, []), do: s1

  def is_disjoint(s1, s2) when is_map(s1) and is_map(s2) do
    {small, big} = if map_size(s1) < map_size(s2), do: {s1, s2}, else: {s2, s1}
    not Enum.any?(:maps.keys(small), fn k -> is_map_key(big, k) end)
  end

  def is_disjoint(s1, s2) do
    {a, b} = if size(s1) < size(s2), do: {s1, s2}, else: {s2, s1}

    fold(
      fn
        _, false -> false
        e, true -> not is_element(e, b)
      end,
      true,
      a
    )
  end

  def subtract(lhs, rhs) when is_map(lhs) and is_map(rhs) do
    Map.new(for({k, _} <- :maps.to_list(lhs), not is_map_key(rhs, k), do: {k, []}))
  end

  def subtract(lhs, rhs), do: filter(fn e -> not is_element(e, rhs) end, lhs)

  def is_subset(s1, s2) when is_map(s1) and is_map(s2) do
    if map_size(s1) > map_size(s2) do
      false
    else
      Enum.all?(:maps.keys(s1), fn k -> is_map_key(s2, k) end)
    end
  end

  def is_subset(s1, s2), do: fold(fn e, sub -> sub and is_element(e, s2) end, true, s1)

  def fold(f, acc, d) when is_function(f, 2) and is_map(d) do
    H.foldl(fn {k, _}, a -> f.(k, a) end, acc, :maps.to_list(d))
  end

  def fold(f, acc, {:set, _, _, _, _, _, _, _, segs}) when is_function(f, 2) do
    H.fold_segs(fn a, bkt -> fold_bucket(f, a, bkt) end, acc, segs, tuple_size(segs))
  end

  defp fold_bucket(f, acc, [e | bkt]), do: fold_bucket(f, f.(e, acc), bkt)
  defp fold_bucket(_, acc, []), do: acc

  def filter(f, d) when is_function(f, 1) and is_map(d) do
    Map.new(for({k, _} <- :maps.to_list(d), f.(k), do: {k, []}))
  end

  def filter(f, {:set, _, _, _, _, _, _, _, segs} = d) when is_function(f, 1) do
    {segs1, fc} = filter_seg_list(f, :erlang.tuple_to_list(segs), [], 0)
    H.maybe_contract(put_elem(d, 8, :erlang.list_to_tuple(segs1)), fc)
  end

  defp filter_seg_list(f, [seg | segs], fss, fc0) do
    {bkts1, fc1} = filter_bkt_list(f, :erlang.tuple_to_list(seg), [], fc0)
    filter_seg_list(f, segs, [:erlang.list_to_tuple(bkts1) | fss], fc1)
  end

  defp filter_seg_list(_, [], fss, fc), do: {:lists.reverse(fss, []), fc}

  defp filter_bkt_list(f, [bkt0 | bkts], fbs, fc0) do
    {bkt1, fc1} = filter_bucket(f, bkt0, [], fc0)
    filter_bkt_list(f, bkts, [bkt1 | fbs], fc1)
  end

  defp filter_bkt_list(_, [], fbs, fc), do: {:lists.reverse(fbs), fc}

  defp filter_bucket(f, [e | bkt], fb, fc) do
    case f.(e) do
      true -> filter_bucket(f, bkt, [e | fb], fc)
      false -> filter_bucket(f, bkt, fb, fc + 1)
    end
  end

  defp filter_bucket(_, [], fb, fc), do: {fb, fc}

  def map(f, d) when is_function(f, 1) and is_map(d) do
    Map.new(for({k, _} <- :maps.to_list(d), do: {f.(k), []}))
  end

  def map(f, {:set, _, _, _, _, _, _, _, _} = d) when is_function(f, 1) do
    fold(fn e, acc -> add_element(f.(e), acc) end, new([{:version, 1}]), d)
  end

  def filtermap(f, d) when is_function(f, 1) and is_map(d) do
    Map.new(H.filtermap(f, to_list(d)), fn k -> {k, []} end)
  end

  def filtermap(f, {:set, _, _, _, _, _, _, _, _} = d) when is_function(f, 1) do
    fold(
      fn e0, acc ->
        case f.(e0) do
          true -> add_element(e0, acc)
          {true, e1} -> add_element(e1, acc)
          false -> acc
        end
      end,
      new([{:version, 1}]),
      d
    )
  end
end

defmodule :dict do
  alias Tonic.ErlangDS, as: H

  # #dict{size, n, maxn, bso, exp_size, con_size, empty, segs}; bucket
  # entries are improper lists [Key | Value].

  def new do
    empty = H.mk_seg()
    {:dict, 0, 16, 16, 8, 80, 48, empty, {empty}}
  end

  def is_key(key, d) do
    slot = H.get_slot(d, key)
    bkt = H.get_bucket(d, slot)
    find_key(key, bkt)
  end

  defp find_key(k, [[k | _val] | _]), do: true
  defp find_key(k, [_ | bkt]), do: find_key(k, bkt)
  defp find_key(_, []), do: false

  def to_list(d), do: fold(fn key, val, list -> [{key, val} | list] end, [], d)

  def from_list(l), do: H.foldl(fn {k, v}, d -> store(k, v, d) end, new(), l)

  def size({:dict, n, _, _, _, _, _, _, _}) when is_integer(n) and n >= 0, do: n

  def is_empty({:dict, n, _, _, _, _, _, _, _}), do: n === 0

  def fetch(key, d) do
    slot = H.get_slot(d, key)
    bkt = H.get_bucket(d, slot)

    try do
      fetch_val(key, bkt)
    catch
      :badarg -> :erlang.error(:badarg, [key, d])
    end
  end

  defp fetch_val(k, [[k | val] | _]), do: val
  defp fetch_val(k, [_ | bkt]), do: fetch_val(k, bkt)
  defp fetch_val(_, []), do: throw(:badarg)

  def find(key, d) do
    slot = H.get_slot(d, key)
    bkt = H.get_bucket(d, slot)
    find_val(key, bkt)
  end

  defp find_val(k, [[k | val] | _]), do: {:ok, val}
  defp find_val(k, [_ | bkt]), do: find_val(k, bkt)
  defp find_val(_, []), do: :error

  def fetch_keys(d), do: fold(fn key, _val, keys -> [key | keys] end, [], d)

  def erase(key, d0) do
    slot = H.get_slot(d0, key)
    {d1, dc} = on_bucket(fn b0 -> erase_key(key, b0) end, d0, slot)
    H.maybe_contract(d1, dc)
  end

  defp erase_key(key, [[key | _val] | bkt]), do: {bkt, 1}

  defp erase_key(key, [e | bkt0]) do
    {bkt1, dc} = erase_key(key, bkt0)
    {[e | bkt1], dc}
  end

  defp erase_key(_, []), do: {[], 0}

  def take(key, d0) do
    slot = H.get_slot(d0, key)

    case on_bucket(fn b0 -> take_key(key, b0) end, d0, slot) do
      {d1, {value, dc}} -> {value, H.maybe_contract(d1, dc)}
      {_, :error} -> :error
    end
  end

  defp take_key(key, [[key | val] | bkt]), do: {bkt, {val, 1}}

  defp take_key(key, [e | bkt0]) do
    {bkt1, res} = take_key(key, bkt0)
    {[e | bkt1], res}
  end

  defp take_key(_, []), do: {[], :error}

  def store(key, val, d0) do
    slot = H.get_slot(d0, key)
    {d1, ic} = on_bucket(fn b0 -> store_bkt_val(key, val, b0) end, d0, slot)
    maybe_expand(d1, ic)
  end

  defp store_bkt_val(key, new, [[key | _old] | bkt]), do: {[[key | new] | bkt], 0}

  defp store_bkt_val(key, new, [other | bkt0]) do
    {bkt1, ic} = store_bkt_val(key, new, bkt0)
    {[other | bkt1], ic}
  end

  defp store_bkt_val(key, new, []), do: {[[key | new]], 1}

  def append(key, val, d0) do
    slot = H.get_slot(d0, key)
    {d1, ic} = on_bucket(fn b0 -> append_bkt(key, val, b0) end, d0, slot)
    maybe_expand(d1, ic)
  end

  defp append_bkt(key, val, [[key | bag] | bkt]), do: {[[key | bag ++ [val]] | bkt], 0}

  defp append_bkt(key, val, [other | bkt0]) do
    {bkt1, ic} = append_bkt(key, val, bkt0)
    {[other | bkt1], ic}
  end

  defp append_bkt(key, val, []), do: {[[key | [val]]], 1}

  def append_list(key, l, d0) do
    slot = H.get_slot(d0, key)
    {d1, ic} = on_bucket(fn b0 -> app_list_bkt(key, l, b0) end, d0, slot)
    maybe_expand(d1, ic)
  end

  defp app_list_bkt(key, l, [[key | bag] | bkt]), do: {[[key | bag ++ l] | bkt], 0}

  defp app_list_bkt(key, l, [other | bkt0]) do
    {bkt1, ic} = app_list_bkt(key, l, bkt0)
    {[other | bkt1], ic}
  end

  defp app_list_bkt(key, l, []), do: {[[key | l]], 1}

  def update(key, f, d0) do
    slot = H.get_slot(d0, key)

    {d1, _uv} =
      try do
        on_bucket(fn b0 -> update_bkt(key, f, b0) end, d0, slot)
      catch
        :badarg -> :erlang.error(:badarg, [key, f, d0])
      end

    d1
  end

  defp update_bkt(key, f, [[key | val] | bkt]) do
    upd = f.(val)
    {[[key | upd] | bkt], upd}
  end

  defp update_bkt(key, f, [other | bkt0]) do
    {bkt1, upd} = update_bkt(key, f, bkt0)
    {[other | bkt1], upd}
  end

  defp update_bkt(_key, _f, []), do: throw(:badarg)

  def update(key, f, init, d0) do
    slot = H.get_slot(d0, key)
    {d1, ic} = on_bucket(fn b0 -> update_bkt(key, f, init, b0) end, d0, slot)
    maybe_expand(d1, ic)
  end

  defp update_bkt(key, f, _, [[key | val] | bkt]), do: {[[key | f.(val)] | bkt], 0}

  defp update_bkt(key, f, i, [other | bkt0]) do
    {bkt1, ic} = update_bkt(key, f, i, bkt0)
    {[other | bkt1], ic}
  end

  defp update_bkt(key, f, i, []) when is_function(f, 1), do: {[[key | i]], 1}

  def update_counter(key, incr, d0) when is_number(incr) do
    slot = H.get_slot(d0, key)
    {d1, ic} = on_bucket(fn b0 -> counter_bkt(key, incr, b0) end, d0, slot)
    maybe_expand(d1, ic)
  end

  defp counter_bkt(key, i, [[key | val] | bkt]), do: {[[key | val + i] | bkt], 0}

  defp counter_bkt(key, i, [other | bkt0]) do
    {bkt1, ic} = counter_bkt(key, i, bkt0)
    {[other | bkt1], ic}
  end

  defp counter_bkt(key, i, []), do: {[[key | i]], 1}

  def fold(f, acc, d), do: fold_dict(f, acc, d)

  def map(f, d), do: map_dict(f, d)

  def filter(f, d), do: filter_dict(f, d)

  def merge(f, d1, d2) when elem(d1, 1) < elem(d2, 1) do
    fold_dict(fn k, v1, d -> update(k, fn v2 -> f.(k, v1, v2) end, v1, d) end, d2, d1)
  end

  def merge(f, d1, d2) do
    fold_dict(fn k, v2, d -> update(k, fn v1 -> f.(k, v1, v2) end, v2, d) end, d1, d2)
  end

  defp maybe_expand(t, 0), do: H.maybe_expand(t, 0)
  defp maybe_expand(t, 1), do: H.maybe_expand(t, 1)

  defp on_bucket(f, t, slot) do
    seg_i = div(slot - 1, 16) + 1
    bkt_i = rem(slot - 1, 16) + 1
    segs = elem(t, 8)
    seg = elem(segs, seg_i - 1)
    b0 = elem(seg, bkt_i - 1)
    {b1, res} = f.(b0)
    {put_elem(t, 8, put_elem(segs, seg_i - 1, put_elem(seg, bkt_i - 1, b1))), res}
  end

  defp fold_dict(f, acc, {:dict, 0, _, _, _, _, _, _, _}) when is_function(f, 3), do: acc

  defp fold_dict(f, acc, {:dict, _, _, _, _, _, _, _, segs}) do
    H.fold_segs(fn a, bkt -> fold_bucket(f, a, bkt) end, acc, segs, tuple_size(segs))
  end

  defp fold_bucket(f, acc, [[key | val] | bkt]), do: fold_bucket(f, f.(key, val, acc), bkt)
  defp fold_bucket(f, acc, []) when is_function(f, 3), do: acc

  defp map_dict(f, {:dict, 0, _, _, _, _, _, _, _} = dict) when is_function(f, 2), do: dict

  defp map_dict(f, d) do
    segs1 = map_seg_list(f, :erlang.tuple_to_list(elem(d, 8)))
    put_elem(d, 8, :erlang.list_to_tuple(segs1))
  end

  defp map_seg_list(f, [seg | segs]) do
    bkts1 = map_bkt_list(f, :erlang.tuple_to_list(seg))
    [:erlang.list_to_tuple(bkts1) | map_seg_list(f, segs)]
  end

  defp map_seg_list(f, []) when is_function(f, 2), do: []

  defp map_bkt_list(f, [bkt0 | bkts]), do: [map_bucket(f, bkt0) | map_bkt_list(f, bkts)]
  defp map_bkt_list(f, []) when is_function(f, 2), do: []

  defp map_bucket(f, [[key | val] | bkt]), do: [[key | f.(key, val)] | map_bucket(f, bkt)]
  defp map_bucket(f, []) when is_function(f, 2), do: []

  defp filter_dict(f, {:dict, 0, _, _, _, _, _, _, _} = dict) when is_function(f, 2), do: dict

  defp filter_dict(f, d) do
    {segs1, fc} = filter_seg_list(f, :erlang.tuple_to_list(elem(d, 8)), [], 0)
    H.maybe_contract(put_elem(d, 8, :erlang.list_to_tuple(segs1)), fc)
  end

  defp filter_seg_list(f, [seg | segs], fss, fc0) do
    {bkts1, fc1} = filter_bkt_list(f, :erlang.tuple_to_list(seg), [], fc0)
    filter_seg_list(f, segs, [:erlang.list_to_tuple(bkts1) | fss], fc1)
  end

  defp filter_seg_list(f, [], fss, fc) when is_function(f, 2), do: {:lists.reverse(fss, []), fc}

  defp filter_bkt_list(f, [bkt0 | bkts], fbs, fc0) do
    {bkt1, fc1} = filter_bucket(f, bkt0, [], fc0)
    filter_bkt_list(f, bkts, [bkt1 | fbs], fc1)
  end

  defp filter_bkt_list(f, [], fbs, fc) when is_function(f, 2), do: {:lists.reverse(fbs), fc}

  defp filter_bucket(f, [[key | val] = e | bkt], fb, fc) do
    case f.(key, val) do
      true -> filter_bucket(f, bkt, [e | fb], fc)
      false -> filter_bucket(f, bkt, fb, fc + 1)
    end
  end

  defp filter_bucket(f, [], fb, fc) when is_function(f, 2), do: {:lists.reverse(fb), fc}
end

defmodule :gb_trees do
  def empty, do: {0, nil}

  def is_empty({0, nil}), do: true
  def is_empty(_), do: false

  def size({size, _}) when is_integer(size) and size >= 0, do: size

  def lookup(key, {_, t}), do: lookup_1(key, t)

  defp lookup_1(key, {key1, _, smaller, _}) when key < key1, do: lookup_1(key, smaller)
  defp lookup_1(key, {key1, _, _, bigger}) when key > key1, do: lookup_1(key, bigger)
  defp lookup_1(_, {_, value, _, _}), do: {:value, value}
  defp lookup_1(_, nil), do: :none

  def is_defined(key, {_, t}), do: is_defined_1(key, t)

  defp is_defined_1(key, {key1, _, smaller, _}) when key < key1, do: is_defined_1(key, smaller)
  defp is_defined_1(key, {key1, _, _, bigger}) when key > key1, do: is_defined_1(key, bigger)
  defp is_defined_1(_, {_, _, _, _}), do: true
  defp is_defined_1(_, nil), do: false

  def get(key, {_, t}), do: get_1(key, t)

  defp get_1(key, {key1, _, smaller, _}) when key < key1, do: get_1(key, smaller)
  defp get_1(key, {key1, _, _, bigger}) when key > key1, do: get_1(key, bigger)
  defp get_1(_, {_, value, _, _}), do: value

  def update(key, val, {s, t}), do: {s, update_1(key, val, t)}

  defp update_1(key, value, {key1, v, smaller, bigger}) when key < key1 do
    {key1, v, update_1(key, value, smaller), bigger}
  end

  defp update_1(key, value, {key1, v, smaller, bigger}) when key > key1 do
    {key1, v, smaller, update_1(key, value, bigger)}
  end

  defp update_1(key, value, {_, _, smaller, bigger}), do: {key, value, smaller, bigger}

  def insert(key, val, {s, t}) when is_integer(s) do
    s1 = s + 1
    {s1, insert_1(key, val, t, s1 * s1)}
  end

  defp insert_1(key, value, {key1, v, smaller, bigger}, s) when key < key1 do
    case insert_1(key, value, smaller, :erlang.bsr(s, 1)) do
      {t1, h1, s1} when is_integer(h1) and is_integer(s1) ->
        t = {key1, v, t1, bigger}
        {h2, s2} = count(bigger)
        h = :erlang.bsl(:erlang.max(h1, h2), 1)
        ss = s1 + s2 + 1
        p = ss * ss
        if h > p, do: balance(t, ss), else: {t, h, ss}

      t1 ->
        {key1, v, t1, bigger}
    end
  end

  defp insert_1(key, value, {key1, v, smaller, bigger}, s) when key > key1 do
    case insert_1(key, value, bigger, :erlang.bsr(s, 1)) do
      {t1, h1, s1} when is_integer(h1) and is_integer(s1) ->
        t = {key1, v, smaller, t1}
        {h2, s2} = count(smaller)
        h = :erlang.bsl(:erlang.max(h1, h2), 1)
        ss = s1 + s2 + 1
        p = ss * ss
        if h > p, do: balance(t, ss), else: {t, h, ss}

      t1 ->
        {key1, v, smaller, t1}
    end
  end

  defp insert_1(key, value, nil, s) when s === 0, do: {{key, value, nil, nil}, 1, 1}
  defp insert_1(key, value, nil, _s), do: {key, value, nil, nil}
  defp insert_1(key, _, _, _), do: :erlang.error({:key_exists, key})

  def enter(key, val, t) do
    case is_defined(key, t) do
      true -> update(key, val, t)
      false -> insert(key, val, t)
    end
  end

  defp count({_, _, nil, nil}), do: {1, 1}

  defp count({_, _, sm, bi}) do
    {h1, s1} = count(sm)
    {h2, s2} = count(bi)
    {:erlang.bsl(:erlang.max(h1, h2), 1), s1 + s2 + 1}
  end

  defp count(nil), do: {1, 0}

  def balance({s, t}) when is_integer(s) and s >= 0, do: {s, balance(t, s)}

  defp balance(t, s), do: balance_list(to_list_1(t), s)

  defp balance_list(l, s) do
    {t, []} = balance_list_1(l, s)
    t
  end

  defp balance_list_1(l, s) when s > 1 do
    sm = s - 1
    s2 = div(sm, 2)
    s1 = sm - s2
    {t1, [{k, v} | l1]} = balance_list_1(l, s1)
    {t2, l2} = balance_list_1(l1, s2)
    {{k, v, t1, t2}, l2}
  end

  defp balance_list_1([{key, val} | l], 1), do: {{key, val, nil, nil}, l}
  defp balance_list_1(l, 0), do: {nil, l}

  def from_orddict(l) do
    s = length(l)
    {s, balance_list(l, s)}
  end

  def delete_any(key, t) do
    case is_defined(key, t) do
      true -> delete(key, t)
      false -> t
    end
  end

  def delete(key, {s, t}) when is_integer(s) and s >= 0, do: {s - 1, delete_1(key, t)}

  defp delete_1(key, {key1, value, smaller, larger}) when key < key1 do
    {key1, value, delete_1(key, smaller), larger}
  end

  defp delete_1(key, {key1, value, smaller, bigger}) when key > key1 do
    {key1, value, smaller, delete_1(key, bigger)}
  end

  defp delete_1(_, {_, _, smaller, larger}), do: merge(smaller, larger)

  defp merge(smaller, nil), do: smaller
  defp merge(nil, larger), do: larger

  defp merge(smaller, larger) do
    {key, value, larger1} = take_smallest1(larger)
    {key, value, smaller, larger1}
  end

  def take_any(key, tree) do
    case is_defined(key, tree) do
      true -> take(key, tree)
      false -> :error
    end
  end

  def take(key, {s, t}) when is_integer(s) and s >= 0 do
    {value, res} = take_1(key, t)
    {value, {s - 1, res}}
  end

  defp take_1(key, {key1, value, smaller, larger}) when key < key1 do
    {value2, smaller1} = take_1(key, smaller)
    {value2, {key1, value, smaller1, larger}}
  end

  defp take_1(key, {key1, value, smaller, bigger}) when key > key1 do
    {value2, bigger1} = take_1(key, bigger)
    {value2, {key1, value, smaller, bigger1}}
  end

  defp take_1(_, {_key, value, smaller, larger}), do: {value, merge(smaller, larger)}

  def take_smallest({size, tree}) when is_integer(size) and size >= 0 do
    {key, value, larger} = take_smallest1(tree)
    {key, value, {size - 1, larger}}
  end

  defp take_smallest1({key, value, nil, larger}), do: {key, value, larger}

  defp take_smallest1({key, value, smaller, larger}) do
    {key1, value1, smaller1} = take_smallest1(smaller)
    {key1, value1, {key, value, smaller1, larger}}
  end

  def smallest({_, tree}), do: smallest_1(tree)

  defp smallest_1({key, value, nil, _larger}), do: {key, value}
  defp smallest_1({_key, _value, smaller, _larger}), do: smallest_1(smaller)

  def take_largest({size, tree}) when is_integer(size) and size >= 0 do
    {key, value, smaller} = take_largest1(tree)
    {key, value, {size - 1, smaller}}
  end

  defp take_largest1({key, value, smaller, nil}), do: {key, value, smaller}

  defp take_largest1({key, value, smaller, larger}) do
    {key1, value1, larger1} = take_largest1(larger)
    {key1, value1, {key, value, smaller, larger1}}
  end

  def largest({_, tree}), do: largest_1(tree)

  defp largest_1({key, value, _smaller, nil}), do: {key, value}
  defp largest_1({_key, _value, _smaller, larger}), do: largest_1(larger)

  def smaller(key, {_, tree_node}), do: smaller_1(key, tree_node)

  defp smaller_1(_key, nil), do: :none

  defp smaller_1(key, {key1, value, _smaller, larger}) when key > key1 do
    case smaller_1(key, larger) do
      :none -> {key1, value}
      entry -> entry
    end
  end

  defp smaller_1(key, {_key, _value, smaller, _larger}), do: smaller_1(key, smaller)

  def larger(key, {_, tree_node}), do: larger_1(key, tree_node)

  defp larger_1(_key, nil), do: :none

  defp larger_1(key, {key1, value, smaller, _larger}) when key < key1 do
    case larger_1(key, smaller) do
      :none -> {key1, value}
      entry -> entry
    end
  end

  defp larger_1(key, {_key, _value, _smaller, larger}), do: larger_1(key, larger)

  def to_list({_, t}), do: to_list(t, [])

  defp to_list_1(t), do: to_list(t, [])

  defp to_list({key, value, small, big}, l), do: to_list(small, [{key, value} | to_list(big, l)])
  defp to_list(nil, l), do: l

  def keys({_, t}), do: keys(t, [])

  defp keys({key, _value, small, big}, l), do: keys(small, [key | keys(big, l)])
  defp keys(nil, l), do: l

  def values({_, t}), do: values(t, [])

  defp values({_key, value, small, big}, l), do: values(small, [value | values(big, l)])
  defp values(nil, l), do: l

  def iterator(tree), do: iterator(tree, :ordered)

  def iterator({_, t}, :ordered), do: {:ordered, iterator_1(t, [])}
  def iterator({_, t}, :reversed), do: {:reversed, iterator_r(t, [])}

  defp iterator_1({_, _, nil, _} = t, as), do: [t | as]
  defp iterator_1({_, _, l, _} = t, as), do: iterator_1(l, [t | as])
  defp iterator_1(nil, as), do: as

  defp iterator_r({_, _, _, nil} = t, as), do: [t | as]
  defp iterator_r({_, _, _, r} = t, as), do: iterator_r(r, [t | as])
  defp iterator_r(nil, as), do: as

  def iterator_from(key, tree), do: iterator_from(key, tree, :ordered)

  def iterator_from(s, {_, t}, :ordered), do: {:ordered, iterator_from_1(s, t, [])}
  def iterator_from(s, {_, t}, :reversed), do: {:reversed, iterator_from_r(s, t, [])}

  defp iterator_from_1(s, {k, _, _, t}, as) when k < s, do: iterator_from_1(s, t, as)
  defp iterator_from_1(_, {_, _, nil, _} = t, as), do: [t | as]
  defp iterator_from_1(s, {_, _, l, _} = t, as), do: iterator_from_1(s, l, [t | as])
  defp iterator_from_1(_, nil, as), do: as

  defp iterator_from_r(s, {k, _, t, _}, as) when k > s, do: iterator_from_r(s, t, as)
  defp iterator_from_r(_, {_, _, _, nil} = t, as), do: [t | as]
  defp iterator_from_r(s, {_, _, _, r} = t, as), do: iterator_from_r(s, r, [t | as])
  defp iterator_from_r(_, nil, as), do: as

  def next({:ordered, [{x, v, _, t} | as]}), do: {x, v, {:ordered, iterator_1(t, as)}}
  def next({:reversed, [{x, v, t, _} | as]}), do: {x, v, {:reversed, iterator_r(t, as)}}
  def next({_, []}), do: :none

  def map(f, {size, tree}) when is_function(f, 2), do: {size, map_1(f, tree)}

  defp map_1(_, nil), do: nil
  defp map_1(f, {k, v, smaller, larger}), do: {k, f.(k, v), map_1(f, smaller), map_1(f, larger)}
end

defmodule :gb_sets do
  def empty, do: {0, nil}

  def new, do: empty()

  def is_empty({0, nil}), do: true
  def is_empty(_), do: false

  def size({size, _}), do: size

  def is_equal({size, s1}, {size, _} = s2) do
    try do
      is_equal_1(s1, to_list(s2))
    catch
      :not_equal -> false
    else
      [] -> true
    end
  end

  def is_equal({_, _}, {_, _}), do: false

  defp is_equal_1(nil, keys), do: keys

  defp is_equal_1({key1, smaller, bigger}, keys0) do
    [key2 | keys] = is_equal_1(smaller, keys0)
    if key1 == key2, do: is_equal_1(bigger, keys), else: throw(:not_equal)
  end

  def singleton(key), do: {1, {key, nil, nil}}

  def is_element(key, s), do: is_member(key, s)

  def is_member(key, {_, t}), do: is_member_1(key, t)

  defp is_member_1(key, {key1, smaller, _}) when key < key1, do: is_member_1(key, smaller)
  defp is_member_1(key, {key1, _, bigger}) when key > key1, do: is_member_1(key, bigger)
  defp is_member_1(_, {_, _, _}), do: true
  defp is_member_1(_, nil), do: false

  def insert(key, {s, t}) when is_integer(s) and s >= 0 do
    s1 = s + 1
    {s1, insert_1(key, t, s1 * s1)}
  end

  defp insert_1(key, {key1, smaller, bigger}, s) when key < key1 do
    case insert_1(key, smaller, :erlang.bsr(s, 1)) do
      {t1, h1, s1} when is_integer(h1) and is_integer(s1) ->
        t = {key1, t1, bigger}
        {h2, s2} = count(bigger)
        h = :erlang.bsl(:erlang.max(h1, h2), 1)
        ss = s1 + s2 + 1
        p = ss * ss
        if h > p, do: balance(t, ss), else: {t, h, ss}

      t1 ->
        {key1, t1, bigger}
    end
  end

  defp insert_1(key, {key1, smaller, bigger}, s) when key > key1 do
    case insert_1(key, bigger, :erlang.bsr(s, 1)) do
      {t1, h1, s1} when is_integer(h1) and is_integer(s1) ->
        t = {key1, smaller, t1}
        {h2, s2} = count(smaller)
        h = :erlang.bsl(:erlang.max(h1, h2), 1)
        ss = s1 + s2 + 1
        p = ss * ss
        if h > p, do: balance(t, ss), else: {t, h, ss}

      t1 ->
        {key1, smaller, t1}
    end
  end

  defp insert_1(key, nil, 0), do: {{key, nil, nil}, 1, 1}
  defp insert_1(key, nil, _), do: {key, nil, nil}
  defp insert_1(key, _, _), do: :erlang.error({:key_exists, key})

  defp count({_, nil, nil}), do: {1, 1}

  defp count({_, sm, bi}) do
    {h1, s1} = count(sm)
    {h2, s2} = count(bi)
    {:erlang.bsl(:erlang.max(h1, h2), 1), s1 + s2 + 1}
  end

  defp count(nil), do: {1, 0}

  def balance({s, t}) when is_integer(s) and s >= 0, do: {s, balance(t, s)}

  defp balance(t, s), do: balance_list(to_list_1(t), s)

  defp balance_list(l, s) do
    {t, _} = balance_list_1(l, s)
    t
  end

  defp balance_list_1(l, s) when s > 1 do
    sm = s - 1
    s2 = div(sm, 2)
    s1 = sm - s2
    {t1, [k | l1]} = balance_list_1(l, s1)
    {t2, l2} = balance_list_1(l1, s2)
    {{k, t1, t2}, l2}
  end

  defp balance_list_1([key | l], 1), do: {{key, nil, nil}, l}
  defp balance_list_1(l, 0), do: {nil, l}

  def add_element(x, s), do: add(x, s)

  def add(x, s) do
    case is_member(x, s) do
      true -> s
      false -> insert(x, s)
    end
  end

  def from_list(l), do: from_ordset(:ordsets.from_list(l))

  def from_ordset(l) do
    s = length(l)
    {s, balance_list(l, s)}
  end

  def del_element(key, s), do: delete_any(key, s)

  def delete_any(key, s) do
    case is_member(key, s) do
      true -> delete(key, s)
      false -> s
    end
  end

  def delete(key, {s, t}), do: {s - 1, delete_1(key, t)}

  defp delete_1(key, {key1, smaller, larger}) when key < key1 do
    {key1, delete_1(key, smaller), larger}
  end

  defp delete_1(key, {key1, smaller, bigger}) when key > key1 do
    {key1, smaller, delete_1(key, bigger)}
  end

  defp delete_1(_, {_, smaller, larger}), do: merge(smaller, larger)

  defp merge(smaller, nil), do: smaller
  defp merge(nil, larger), do: larger

  defp merge(smaller, larger) do
    {key, larger1} = take_smallest1(larger)
    {key, smaller, larger1}
  end

  def take_smallest({s, t}) do
    {key, larger} = take_smallest1(t)
    {key, {s - 1, larger}}
  end

  defp take_smallest1({key, nil, larger}), do: {key, larger}

  defp take_smallest1({key, smaller, larger}) do
    {key1, smaller1} = take_smallest1(smaller)
    {key1, {key, smaller1, larger}}
  end

  def smallest({_, t}), do: smallest_1(t)

  defp smallest_1({key, nil, _larger}), do: key
  defp smallest_1({_key, smaller, _larger}), do: smallest_1(smaller)

  def take_largest({s, t}) do
    {key, smaller} = take_largest1(t)
    {key, {s - 1, smaller}}
  end

  defp take_largest1({key, smaller, nil}), do: {key, smaller}

  defp take_largest1({key, smaller, larger}) do
    {key1, larger1} = take_largest1(larger)
    {key1, {key, smaller, larger1}}
  end

  def largest({_, t}), do: largest_1(t)

  defp largest_1({key, _smaller, nil}), do: key
  defp largest_1({_key, _smaller, larger}), do: largest_1(larger)

  def smaller(key, {_, t}), do: smaller_1(key, t)

  defp smaller_1(_key, nil), do: :none

  defp smaller_1(key, {key1, _smaller, larger}) when key > key1 do
    case smaller_1(key, larger) do
      :none -> {:found, key1}
      found -> found
    end
  end

  defp smaller_1(key, {_key, smaller, _larger}), do: smaller_1(key, smaller)

  def larger(key, {_, t}), do: larger_1(key, t)

  defp larger_1(_key, nil), do: :none

  defp larger_1(key, {key1, smaller, _larger}) when key < key1 do
    case larger_1(key, smaller) do
      :none -> {:found, key1}
      found -> found
    end
  end

  defp larger_1(key, {_key, _smaller, larger}), do: larger_1(key, larger)

  def to_list({_, t}), do: to_list(t, [])

  defp to_list_1(t), do: to_list(t, [])

  defp to_list({key, small, big}, l), do: to_list(small, [key | to_list(big, l)])
  defp to_list(nil, l), do: l

  def iterator(set), do: iterator(set, :ordered)

  def iterator({_, t}, :ordered), do: {:ordered, iterator_1(t, [])}
  def iterator({_, t}, :reversed), do: {:reversed, iterator_r(t, [])}

  defp iterator_1({_, nil, _} = t, as), do: [t | as]
  defp iterator_1({_, l, _} = t, as), do: iterator_1(l, [t | as])
  defp iterator_1(nil, as), do: as

  defp iterator_r({_, _, nil} = t, as), do: [t | as]
  defp iterator_r({_, _, r} = t, as), do: iterator_r(r, [t | as])
  defp iterator_r(nil, as), do: as

  def iterator_from(element, set), do: iterator_from(element, set, :ordered)

  def iterator_from(s, {_, t}, :ordered), do: {:ordered, iterator_from_1(s, t, [])}
  def iterator_from(s, {_, t}, :reversed), do: {:reversed, iterator_from_r(s, t, [])}

  defp iterator_from_1(s, {k, _, t}, as) when k < s, do: iterator_from_1(s, t, as)
  defp iterator_from_1(_, {_, nil, _} = t, as), do: [t | as]
  defp iterator_from_1(s, {_, l, _} = t, as), do: iterator_from_1(s, l, [t | as])
  defp iterator_from_1(_, nil, as), do: as

  defp iterator_from_r(s, {k, t, _}, as) when k > s, do: iterator_from_r(s, t, as)
  defp iterator_from_r(_, {_, _, nil} = t, as), do: [t | as]
  defp iterator_from_r(s, {_, _, r} = t, as), do: iterator_from_r(s, r, [t | as])
  defp iterator_from_r(_, nil, as), do: as

  def next({:ordered, [{x, _, t} | as]}), do: {x, {:ordered, iterator_1(t, as)}}
  def next({:reversed, [{x, t, _} | as]}), do: {x, {:reversed, iterator_r(t, as)}}
  def next({_, []}), do: :none

  # 1 / ln 2
  defp c_log(n), do: round(1.46 * :math.log(n))

  def union({n1, t1}, {n2, t2}) when is_integer(n1) and is_integer(n2) and n2 < n1 do
    union(to_list_1(t2), n2, t1, n1)
  end

  def union({n1, t1}, {n2, t2}) when is_integer(n1) and is_integer(n2) do
    union(to_list_1(t1), n1, t2, n2)
  end

  defp union(l, n1, t2, n2) when n2 < 10, do: union_2(l, to_list_1(t2), n1 + n2)

  defp union(l, n1, t2, n2) do
    x = n1 * c_log(n2)

    if n2 < x do
      union_2(l, to_list_1(t2), n1 + n2)
    else
      union_1(l, mk_set(n2, t2))
    end
  end

  defp mk_set(n, t), do: {n, t}

  defp union_1([x | xs], s), do: union_1(xs, add(x, s))
  defp union_1([], s), do: s

  defp union_2(xs, ys, s), do: union_2(xs, ys, [], s)

  defp union_2([x | xs1], [y | _] = ys, as, s) when x < y, do: union_2(xs1, ys, [x | as], s)
  defp union_2([x | _] = xs, [y | ys1], as, s) when x > y, do: union_2(ys1, xs, [y | as], s)
  defp union_2([x | xs1], [_ | ys1], as, s), do: union_2(xs1, ys1, [x | as], s - 1)
  defp union_2([], ys, as, s), do: {s, balance_revlist(push(ys, as), s)}
  defp union_2(xs, [], as, s), do: {s, balance_revlist(push(xs, as), s)}

  defp push([x | xs], as), do: push(xs, [x | as])
  defp push([], as), do: as

  defp balance_revlist(l, s) when is_integer(s) do
    {t, _} = balance_revlist_1(l, s)
    t
  end

  defp balance_revlist_1(l, s) when s > 1 do
    sm = s - 1
    s2 = div(sm, 2)
    s1 = sm - s2
    {t2, [k | l1]} = balance_revlist_1(l, s1)
    {t1, l2} = balance_revlist_1(l1, s2)
    {{k, t1, t2}, l2}
  end

  defp balance_revlist_1([key | l], 1), do: {{key, nil, nil}, l}
  defp balance_revlist_1(l, 0), do: {nil, l}

  def union([s | ss]), do: union_list(s, ss)
  def union([]), do: empty()

  defp union_list(s, [s1 | ss]), do: union_list(union(s, s1), ss)
  defp union_list(s, []), do: s

  def intersection({n1, t1}, {n2, t2}) when is_integer(n1) and is_integer(n2) and n2 < n1 do
    intersection(to_list_1(t2), n2, t1, n1)
  end

  def intersection({n1, t1}, {n2, t2}) when is_integer(n1) and is_integer(n2) do
    intersection(to_list_1(t1), n1, t2, n2)
  end

  defp intersection(l, _n1, t2, n2) when n2 < 10, do: intersection_2(l, to_list_1(t2))

  defp intersection(l, n1, t2, n2) do
    x = n1 * c_log(n2)
    if n2 < x, do: intersection_2(l, to_list_1(t2)), else: intersection_1(l, t2)
  end

  defp intersection_1(xs, t), do: intersection_1(xs, t, [], 0)

  defp intersection_1([x | xs], t, as, n) do
    case is_member_1(x, t) do
      true -> intersection_1(xs, t, [x | as], n + 1)
      false -> intersection_1(xs, t, as, n)
    end
  end

  defp intersection_1([], _, as, n), do: {n, balance_revlist(as, n)}

  defp intersection_2(xs, ys), do: intersection_2(xs, ys, [], 0)

  defp intersection_2([x | xs1], [y | _] = ys, as, s) when x < y, do: intersection_2(xs1, ys, as, s)
  defp intersection_2([x | _] = xs, [y | ys1], as, s) when x > y, do: intersection_2(ys1, xs, as, s)
  defp intersection_2([x | xs1], [_ | ys1], as, s), do: intersection_2(xs1, ys1, [x | as], s + 1)
  defp intersection_2([], _, as, s), do: {s, balance_revlist(as, s)}
  defp intersection_2(_, [], as, s), do: {s, balance_revlist(as, s)}

  def intersection([s | ss]), do: intersection_list(s, ss)

  defp intersection_list(s, [s1 | ss]), do: intersection_list(intersection(s, s1), ss)
  defp intersection_list(s, []), do: s

  def is_disjoint({n1, t1}, {n2, t2}) when n1 < n2, do: is_disjoint_1(t1, t2)
  def is_disjoint({_, t1}, {_, t2}), do: is_disjoint_1(t2, t1)

  defp is_disjoint_1({k1, smaller1, bigger}, {k2, smaller2, _} = tree) when k1 < k2 do
    not is_member_1(k1, smaller2) and is_disjoint_1(smaller1, smaller2) and
      is_disjoint_1(bigger, tree)
  end

  defp is_disjoint_1({k1, smaller, bigger1}, {k2, _, bigger2} = tree) when k1 > k2 do
    not is_member_1(k1, bigger2) and is_disjoint_1(bigger1, bigger2) and
      is_disjoint_1(smaller, tree)
  end

  defp is_disjoint_1({_k1, _, _}, {_k2, _, _}), do: false
  defp is_disjoint_1(nil, _), do: true
  defp is_disjoint_1(_, nil), do: true

  def subtract(s1, s2), do: difference(s1, s2)

  def difference({n1, t1}, {n2, t2})
      when is_integer(n1) and n1 >= 0 and is_integer(n2) and n2 >= 0 do
    difference(to_list_1(t1), n1, t2, n2)
  end

  defp difference(l, n1, t2, n2) when n2 < 10, do: difference_2(l, to_list_1(t2), n1)

  defp difference(l, n1, t2, n2) do
    x = n1 * c_log(n2)
    if n2 < x, do: difference_2(l, to_list_1(t2), n1), else: difference_1(l, t2)
  end

  defp difference_1(xs, t), do: difference_1(xs, t, [], 0)

  defp difference_1([x | xs], t, as, n) do
    case is_member_1(x, t) do
      true -> difference_1(xs, t, as, n)
      false -> difference_1(xs, t, [x | as], n + 1)
    end
  end

  defp difference_1([], _, as, n), do: {n, balance_revlist(as, n)}

  defp difference_2(xs, ys, s), do: difference_2(xs, ys, [], s)

  defp difference_2([x | xs1], [y | _] = ys, as, s) when x < y, do: difference_2(xs1, ys, [x | as], s)
  defp difference_2([x | _] = xs, [y | ys1], as, s) when x > y, do: difference_2(xs, ys1, as, s)
  defp difference_2([_x | xs1], [_y | ys1], as, s), do: difference_2(xs1, ys1, as, s - 1)
  defp difference_2([], _ys, as, s), do: {s, balance_revlist(as, s)}
  defp difference_2(xs, [], as, s), do: {s, balance_revlist(push(xs, as), s)}

  def is_subset({n1, t1}, {n2, t2})
      when is_integer(n1) and n1 >= 0 and is_integer(n2) and n2 >= 0 do
    is_subset(to_list_1(t1), n1, t2, n2)
  end

  defp is_subset(l, _n1, t2, n2) when n2 < 10, do: is_subset_2(l, to_list_1(t2))

  defp is_subset(l, n1, t2, n2) do
    x = n1 * c_log(n2)
    if n2 < x, do: is_subset_2(l, to_list_1(t2)), else: is_subset_1(l, t2)
  end

  defp is_subset_1([x | xs], t) do
    case is_member_1(x, t) do
      true -> is_subset_1(xs, t)
      false -> false
    end
  end

  defp is_subset_1([], _), do: true

  defp is_subset_2([x | _], [y | _]) when x < y, do: false
  defp is_subset_2([x | _] = xs, [y | ys1]) when x > y, do: is_subset_2(xs, ys1)
  defp is_subset_2([_ | xs1], [_ | ys1]), do: is_subset_2(xs1, ys1)
  defp is_subset_2([], _), do: true
  defp is_subset_2(_, []), do: false

  def is_set({0, nil}), do: true
  def is_set({n, {_, _, _}}) when is_integer(n) and n >= 0, do: true
  def is_set(_), do: false

  def filter(f, s) when is_function(f, 1), do: from_ordset(for(x <- to_list(s), f.(x), do: x))

  def map(f, {_, t}) when is_function(f, 1), do: from_list(map_1(t, f, []))

  defp map_1({key, small, big}, f, l), do: map_1(small, f, [f.(key) | map_1(big, f, l)])
  defp map_1(nil, _f, l), do: l

  def filtermap(f, {_, t}) when is_function(f, 1), do: from_list(filtermap_1(t, f, []))

  defp filtermap_1({key, small, big}, f, l) do
    case f.(key) do
      true -> filtermap_1(small, f, [key | filtermap_1(big, f, l)])
      {true, val} -> filtermap_1(small, f, [val | filtermap_1(big, f, l)])
      false -> filtermap_1(small, f, filtermap_1(big, f, l))
    end
  end

  defp filtermap_1(nil, _f, l), do: l

  def fold(f, a, {_, t}) when is_function(f, 2), do: fold_1(f, a, t)

  defp fold_1(f, acc0, {key, small, big}) do
    acc1 = fold_1(f, acc0, small)
    acc = f.(key, acc1)
    fold_1(f, acc, big)
  end

  defp fold_1(_, acc, _), do: acc
end
