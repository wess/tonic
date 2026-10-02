defmodule :timer do
  def sleep(ms), do: Process.sleep(ms)
  def seconds(s), do: s * 1000
  def minutes(m), do: m * 60_000
  def hours(h), do: h * 3_600_000
# Modified for Tonic; Erlang/OTP 27.3 source/port. Apache-2.0; see licenses/sources.json and notice.
  def tc(fun) do
    t = System.monotonic_time(:microsecond)
    r = fun.()
    {System.monotonic_time(:microsecond) - t, r}
  end

  def tc(fun, args) do
    t = System.monotonic_time(:microsecond)
    r = apply(fun, args)
    {System.monotonic_time(:microsecond) - t, r}
  end

  def tc(m, f, a) do
    t = System.monotonic_time(:microsecond)
    r = apply(m, f, a)
    {System.monotonic_time(:microsecond) - t, r}
  end
end

defmodule :maps do
  # Built only on runtime intrinsics (:maps.get/find/put/remove/merge/keys/
  # values/to_list/from_list/is_key/size) to avoid recursion through Map.

  def fold(f, acc, m) when is_map(m), do: fold_list(iter_list(m), f, acc)
  def fold(f, acc, {:tonic_iter, l}), do: fold_list(l, f, acc)
  defp fold_list([], _f, acc), do: acc
  defp fold_list([{k, v} | t], f, acc), do: fold_list(t, f, f.(k, v, acc))

  def map(f, m) when is_map(m), do: :maps.from_list(for {k, v} <- :maps.to_list(m), do: {k, f.(k, v)})
  def filter(f, m) when is_map(m), do: :maps.from_list(for {k, v} <- :maps.to_list(m), f.(k, v), do: {k, v})
  def filtermap(f, m) when is_map(m) do
    :maps.from_list(
      for {k, v} <- :maps.to_list(m), r = f.(k, v), r != false do
        if r == true, do: {k, v}, else: {k, elem(r, 1)}
      end
    )
  end
  def foreach(f, m) when is_map(m), do: (Enum.each(:maps.to_list(m), fn {k, v} -> f.(k, v) end); :ok)

  def update(k, v, m) do
    if :maps.is_key(k, m), do: :maps.put(k, v, m), else: raise(KeyError, key: k, term: m)
  end

  def with(ks, m), do: :maps.from_list(for k <- ks, :maps.is_key(k, m), do: {k, :maps.get(k, m)})
  def without(ks, m), do: List.foldl(ks, m, fn k, acc -> :maps.remove(k, acc) end)

  def take(k, m) do
    case :maps.find(k, m) do
      {:ok, v} -> {v, :maps.remove(k, m)}
      :error -> :error
    end
  end

  def new, do: %{}
  def from_keys(keys, value), do: :maps.from_list(for k <- keys, do: {k, value})

  def update_with(k, f, m) do
    case :maps.find(k, m) do
      {:ok, v} -> :maps.put(k, f.(v), m)
      :error -> raise(KeyError, key: k, term: m)
    end
  end

  def update_with(k, f, init, m) do
    case :maps.find(k, m) do
      {:ok, v} -> :maps.put(k, f.(v), m)
      :error -> :maps.put(k, init, m)
    end
  end

  def merge_with(f, m1, m2) do
    List.foldl(:maps.to_list(m2), m1, fn {k, v2}, acc ->
      case :maps.find(k, acc) do
        {:ok, v1} -> :maps.put(k, f.(k, v1, v2), acc)
        :error -> :maps.put(k, v2, acc)
      end
    end)
  end

  def intersect(m1, m2), do: intersect_with(fn _k, _v1, v2 -> v2 end, m1, m2)

  def intersect_with(f, m1, m2) do
    :maps.from_list(
      for {k, v1} <- :maps.to_list(m1), :maps.is_key(k, m2), do: {k, f.(k, v1, :maps.get(k, m2))}
    )
  end

  def groups_from_list(key_fun, list), do: groups_from_list(key_fun, fn x -> x end, list)

  def groups_from_list(key_fun, value_fun, list) do
    m =
      List.foldl(list, %{}, fn x, acc ->
        k = key_fun.(x)
        case :maps.find(k, acc) do
          {:ok, l} -> :maps.put(k, [value_fun.(x) | l], acc)
          :error -> :maps.put(k, [value_fun.(x)], acc)
        end
      end)

    :maps.map(fn _k, l -> :lists.reverse(l) end, m)
  end

  # Iterators: forward HAMT order (the reverse of to_list for large maps).
  defp iter_list(m) do
    l = :maps.to_list(m)
    if :maps.size(m) > 32, do: :lists.reverse(l), else: l
  end

  def iterator(m) when is_map(m), do: {:tonic_iter, iter_list(m)}
  def iterator(m, :undefined) when is_map(m), do: iterator(m)
  def iterator(m, :ordered) when is_map(m), do: {:tonic_iter, :lists.sort(:maps.to_list(m))}
  def iterator(m, :reversed) when is_map(m), do: {:tonic_iter, :lists.reverse(:lists.sort(:maps.to_list(m)))}
  def iterator(m, cmp) when is_map(m) and is_function(cmp, 2), do: {:tonic_iter, Enum.sort(:maps.to_list(m), fn {a, _}, {b, _} -> cmp.(a, b) end)}

  def next({:tonic_iter, [{k, v} | rest]}), do: {k, v, {:tonic_iter, rest}}
  def next({:tonic_iter, []}), do: :none
  def next(:none), do: :none
end

defmodule :elixir_config do
  def identifier_tokenizer, do: String.Tokenizer
  def is_bootstrap, do: false
  def get(key, default \\ nil), do: :application.get_env(:tonic_elixir_config, key, default_value(key, default))
  def put(key, value), do: :application.set_env(:tonic_elixir_config, key, value)

  defp default_value({:uri, scheme}, default) do
    case scheme do
      "ftp" -> 21
      "sftp" -> 22
      "tftp" -> 69
      "http" -> 80
      "https" -> 443
      "ldap" -> 389
      "ws" -> 80
      "wss" -> 443
      _ -> default
    end
  end

  defp default_value(_key, default), do: default
end

defmodule :uri_string do
  # RFC 3986, appendix B.
  def parse(uri) when is_binary(uri) do
    try do
      do_parse(uri)
    catch
      :throw, {:uri_invalid, c} -> {:error, :invalid_uri, String.to_charlist(c)}
    end
  end

  defp do_parse(uri) do
    case Regex.run(~r/^(([a-zA-Z][a-zA-Z0-9+.-]*):)?(\/\/([^\/?#]*))?([^?#]*)(\?([^#]*))?(#(.*))?$/s, uri, return: :index) do
      nil ->
        {:error, :invalid_uri, ""}

      caps ->
        part = fn i ->
          case Enum.at(caps, i) do
            {-1, _} -> nil
            nil -> nil
            {s, l} -> binary_part(uri, s, l)
          end
        end

        bad =
          Enum.find_value([{part.(5), ~c"/:@"}, {part.(7), ~c"/:@?"}, {part.(9), ~c"/:@?"}], fn
            {nil, _} -> nil
            {str, extra} -> invalid_char(str, extra)
          end)

        if bad, do: throw({:uri_invalid, bad})

        # path-noscheme: a relative reference's first segment has no ':'
        if part.(2) == nil and part.(4) == nil do
          [first | _] = String.split(part.(5) || "", "/", parts: 2)
          if String.contains?(first, ":"), do: throw({:uri_invalid, ":"})
        end

        m = %{path: part.(5) || ""}
        m = if (s = part.(2)) != nil, do: Map.put(m, :scheme, s), else: m
        m = if (q = part.(7)) != nil, do: Map.put(m, :query, q), else: m
        m = if (f = part.(9)) != nil, do: Map.put(m, :fragment, f), else: m

        case part.(4) do
          nil ->
            m

          auth ->
            {userinfo, hostport} =
              case :binary.split(auth, "@") do
                [u, hp] -> {u, hp}
                [hp] -> {nil, hp}
              end

            m = if userinfo, do: Map.put(m, :userinfo, userinfo), else: m

            {host, port} =
              case Regex.run(~r/^(\[[^\]]*\]|[^:]*)(:([0-9]*))?$/, hostport) do
                [_, h] -> {h, nil}
                [_, h, _, ""] -> {h, :empty}
                [_, h, _, p] -> {h, String.to_integer(p)}
                _ -> {hostport, nil}
              end

            if host != nil and String.contains?(host, ["%", " "]) and not String.starts_with?(host, "[") and false do
              {:error, :invalid_uri, ":"}
            else
              host = String.trim_leading(host, "[") |> String.trim_trailing("]")
              m = Map.put(m, :host, host)

              case port do
                nil -> m
                :empty -> m
                p -> Map.put(m, :port, p)
              end
            end
        end
    end
  end

  def parse(uri) when is_list(uri), do: parse(List.to_string(uri))

  # First character not allowed by RFC 3986 (pchar plus `extra`); Unicode
  # characters are accepted as in OTP.
  defp invalid_char(<<>>, _extra), do: nil

  defp invalid_char(<<?%, a, b, rest::binary>>, extra)
       when a in ?0..?9 or a in ?a..?f or a in ?A..?F,
       do: if(b in ?0..?9 or b in ?a..?f or b in ?A..?F, do: invalid_char(rest, extra), else: <<?%>>)

  defp invalid_char(<<?%, _::binary>>, _extra), do: <<?%>>

  defp invalid_char(<<c, rest::binary>>, extra)
       when c in ?a..?z or c in ?A..?Z or c in ?0..?9 or c in ~c"-._~!$&'()*+,;=",
       do: invalid_char(rest, extra)

  defp invalid_char(<<c::utf8, rest::binary>>, extra) do
    if c > 127 or c in extra, do: invalid_char(rest, extra), else: <<c::utf8>>
  end

  defp invalid_char(<<c, _::binary>>, _extra), do: <<c>>
end

defmodule :math do
  def sin(x), do: :tonic.math1(1, x)
  def cos(x), do: :tonic.math1(2, x)
  def tan(x), do: :tonic.math1(3, x)
  def asin(x), do: :tonic.math1(4, x)
  def acos(x), do: :tonic.math1(5, x)
  def atan(x), do: :tonic.math1(6, x)
  def exp(x), do: :tonic.math1(7, x)
  def log(x), do: :tonic.math1(8, x)
  def log2(x), do: :tonic.math1(9, x)
  def log10(x), do: :tonic.math1(10, x)
  def floor(x), do: :tonic.math1(11, x)
  def ceil(x), do: :tonic.math1(12, x)
  def sinh(x), do: :tonic.math1(13, x)
  def cosh(x), do: :tonic.math1(14, x)
  def tanh(x), do: :tonic.math1(15, x)
  def atan2(y, x), do: :tonic.math2(1, y, x)
  def fmod(x, y), do: :tonic.math2(2, x, y)
end

defmodule :rand do
  # Port of OTP rand.erl's default algorithm (exsss, Xorshift116**) so seeded
  # sequences match the BEAM exactly.
  import Bitwise

  @m58 (1 <<< 58) - 1
  @m64 (1 <<< 64) - 1
  @two_pow_minus53 1.11022302462515657e-16

  defp mk_alg(:exsss) do
    %{
      type: :exsss,
      bits: 58,
      next: &:rand.exsss_next/1,
      uniform: &:rand.exsss_uniform/1,
      uniform_n: &:rand.exsss_uniform/2
    }
  end

  defp mk_alg(:default), do: mk_alg(:exsss)
  defp mk_alg(_other), do: mk_alg(:exsss)

  def seed(alg_or_state), do: seed_put(seed_s(alg_or_state))
  def seed(alg, seed), do: seed_put(seed_s(alg, seed))

  def seed_s({handler, _} = state) when is_map(handler), do: state
  def seed_s({alg, alg_state}) when is_atom(alg), do: {mk_alg(alg), alg_state}
  def seed_s(alg) when is_atom(alg), do: seed_s(alg, default_seed())

  def seed_s(alg, seed), do: {mk_alg(alg), exsss_seed(seed)}

  defp default_seed do
    {:erlang.phash2([{node(), self()}]), :erlang.system_time(), :erlang.unique_integer()}
  end

  def export_seed do
    case Process.get(:rand_seed) do
      {%{type: alg}, alg_state} -> {alg, alg_state}
      _ -> :undefined
    end
  end

  def export_seed_s({%{type: alg}, alg_state}), do: {alg, alg_state}

  defp seed_put(seed) do
    Process.put(:rand_seed, seed)
    seed
  end

  defp seed_get do
    case Process.get(:rand_seed) do
      nil -> seed(:exsss)
      old -> old
    end
  end

  def uniform do
    {x, state} = uniform_s(seed_get())
    seed_put(state)
    x
  end

  def uniform(n) do
    {x, state} = uniform_s(n, seed_get())
    seed_put(state)
    x
  end

  def uniform_s({%{uniform: u}, _} = state), do: u.(state)
  def uniform_s(n, {%{uniform_n: u}, _} = state) when is_integer(n) and n >= 1, do: u.(n, state)

  def uniform_real do
    {x, state} = uniform_real_s(seed_get())
    seed_put(state)
    x
  end

  # Simplified: same as uniform but never exactly 0.0.
  def uniform_real_s(state) do
    case uniform_s(state) do
      {x, s} when x > 0.0 -> {x, s}
      {_, s} -> uniform_real_s(s)
    end
  end

  def bytes(n) do
    {b, state} = bytes_s(n, seed_get())
    seed_put(state)
    b
  end

  def bytes_s(n, state), do: bytes_s(n, state, <<>>)
  defp bytes_s(0, state, acc), do: {acc, state}

  defp bytes_s(n, {h, r}, acc) do
    {v, r} = exsss_next(r)
    take = min(n, 7)
    b = v >>> (58 - take * 8)
    bytes_s(n - take, {h, r}, <<acc::binary, b::size(take * 8)>>)
  end

  def normal do
    {x, state} = normal_s(seed_get())
    seed_put(state)
    x
  end

  def normal(mean, variance), do: mean + :math.sqrt(variance) * normal()

  def normal_s(state) do
    {u1, state} = uniform_real_s(state)
    {u2, state} = uniform_s(state)
    {:math.sqrt(-2 * :math.log(u1)) * :math.cos(2 * :math.pi() * u2), state}
  end

  def exsss_seed(l) when is_list(l) do
    [s0, s1] = seed58_nz(2, l)
    [s0 | s1]
  end

  def exsss_seed(x) when is_integer(x) do
    [s0, s1] = seed58(2, x)
    [s0 | s1]
  end

  def exsss_seed({a1, a2, a3}) do
    {_, x0} = seed58(a1)
    {s0, x1} = seed58(bxor(a2, x0))
    {s1, _} = seed58(bxor(a3, x1))
    [s0 | s1]
  end

  defp bsl58(x, n), do: (x &&& ((1 <<< (58 - n)) - 1)) <<< n

  def exsss_next([s1 | s0]) do
    s0_1 = s0 &&& @m58
    s1_b = bxor(s1 &&& @m58, bsl58(s1, 24))
    new_s1 = s1_b |> bxor(s0_1) |> bxor(s1_b >>> 11) |> bxor(s0_1 >>> 41)
    v_a = s0_1 + bsl58(s0_1, 2)
    v_b = bsl58(v_a, 7) ||| ((v_a >>> 51) &&& 127)
    {(v_b + bsl58(v_b, 3)) &&& @m58, [s0_1 | new_s1]}
  end

  def exsss_uniform({h, r0}) do
    {i, r1} = exsss_next(r0)
    {(i >>> 5) * @two_pow_minus53, {h, r1}}
  end

  def exsss_uniform(range, {h, r}) do
    {v, r1} = exsss_next(r)
    max_minus_range = (1 <<< 58) - range

    if 0 <= max_minus_range do
      if v < range do
        {v + 1, {h, r1}}
      else
        i = rem(v, range)

        if v - i <= max_minus_range do
          {i + 1, {h, r1}}
        else
          exsss_uniform(range, {h, r1})
        end
      end
    else
      uniform_range(range, h, r1, v)
    end
  end

  defp uniform_range(range, %{next: next, bits: bits} = h, r, v) do
    shift = bits
    shift_mask = bnot(0)
    rm1 = range - 1

    if (range &&& rm1) == 0 do
      {v1, r1, _} = uniform_range(range >>> bits, next, r, v, shift_mask, shift, bits)
      {(v1 &&& rm1) + 1, {h, r1}}
    else
      {v1, r1, b} = uniform_range(range >>> (bits - 2), next, r, v, shift_mask, shift, bits)
      i = rem(v1, range)

      if v1 - i <= (1 <<< b) - range do
        {i + 1, {h, r1}}
      else
        {v2, r2} = next.(r1)
        uniform_range(range, h, r2, v2)
      end
    end
  end

  defp uniform_range(range, next, r, v, shift_mask, shift, b) do
    if range <= 1 do
      {v, r, b}
    else
      {v1, r1} = next.(r)
      uniform_range(range >>> shift, next, r1, ((v &&& shift_mask) <<< shift) ||| v1, shift_mask, shift, b + shift)
    end
  end

  defp seed58_nz(n, ss), do: seed_nz(n, ss, false)

  defp seed_nz(_n, [], false), do: :erlang.error(:zero_seed)
  defp seed_nz(0, [_ | _], _nz), do: :erlang.error(:too_many_seed_integers)
  defp seed_nz(0, [], _nz), do: []
  defp seed_nz(n, [], true), do: [0 | seed_nz(n - 1, [], true)]

  defp seed_nz(n, [s | ss], nz) when is_integer(s) do
    r = s &&& @m58
    [r | seed_nz(n - 1, ss, nz or r != 0)]
  end

  defp seed_nz(_n, _ss, _nz), do: :erlang.error(:non_integer_seed)

  defp seed58(0, _x), do: []

  defp seed58(n, x) do
    {z, new_x} = seed58(x)
    [z | seed58(n - 1, new_x)]
  end

  defp seed58(x_0) do
    {z0, x} = splitmix64_next(x_0)

    case z0 &&& @m58 do
      0 -> seed58(x)
      z -> {z, x}
    end
  end

  def splitmix64_next(x_0) do
    x = (x_0 + 0x9E3779B97F4A7C15) &&& @m64
    z_0 = (bxor(x, x >>> 30) * 0xBF58476D1CE4E5B9) &&& @m64
    z_1 = (bxor(z_0, z_0 >>> 27) * 0x94D049BB133111EB) &&& @m64
    {bxor(z_1, z_1 >>> 31) &&& @m64, x}
  end
end

defmodule :erlang do
  # Functions of the :erlang module that are not runtime intrinsics.

  def fun_info(fun) when is_function(fun) do
    {module, name, arity} = :tonic.fun_info(fun)

    if is_atom(name) and String.starts_with?(Atom.to_string(name), "-") do
      [pid: self(), module: module, new_index: 0, new_uniq: <<0::128>>, index: 0, uniq: 0, name: name, arity: arity, env: [], type: :local]
    else
      [module: module, name: name, arity: arity, env: [], type: :external]
    end
  end

  def fun_info(fun, item) when is_function(fun) and is_atom(item) do
    case List.keyfind(fun_info(fun), item, 0) do
      {^item, value} -> {item, value}
      nil -> {item, :undefined}
    end
  end

  def monotonic_time, do: :erlang.monotonic_time(:native)
  def system_time, do: :erlang.system_time(:native)
  def time_offset, do: 0
  def time_offset(_unit), do: 0
  def unique_integer, do: :tonic.unique_integer()
  def unique_integer(_modifiers), do: :tonic.unique_integer()

  def timestamp do
    us = :erlang.system_time(:microsecond)
    {div(us, 1_000_000_000_000), rem(div(us, 1_000_000), 1_000_000), rem(us, 1_000_000)}
  end

  def now, do: timestamp()

  def convert_time_unit(time, from, to), do: System.convert_time_unit(time, from, to)

  def universaltime, do: :calendar.system_time_to_universal_time(:erlang.system_time(:second), :second)
  def localtime, do: :calendar.local_time()
  def date, do: elem(localtime(), 0)
  def time, do: elem(localtime(), 1)


  def list_to_existing_atom(list), do: :erlang.binary_to_existing_atom(List.to_string(list))
  def list_to_float(list), do: :erlang.binary_to_float(List.to_string(list))
  def list_to_integer(list, base), do: :erlang.binary_to_integer(List.to_string(list), base)
  def float_to_list(float), do: :tonic.str_to_charlist(:erlang.float_to_binary(float))
  def float_to_list(float, options), do: :tonic.str_to_charlist(:erlang.float_to_binary(float, options))
  def float_to_binary(float, options), do: :tonic.float_to_binary_opts(float, options)
  def binary_to_list(binary, from, to), do: :binary.bin_to_list(binary, from - 1, to - from + 1)
  def list_to_bitstring(list), do: :erlang.iolist_to_binary(list)
  def bitstring_to_list(bin), do: :erlang.binary_to_list(bin)
  def split_binary(bin, pos), do: {:binary.part(bin, 0, pos), :binary.part(bin, pos, byte_size(bin) - pos)}
  def atom_to_binary(atom, _encoding), do: :erlang.atom_to_binary(atom)
  def binary_to_atom(bin, _encoding), do: :erlang.binary_to_atom(bin)
  def binary_to_existing_atom(bin, _encoding), do: :erlang.binary_to_existing_atom(bin)
  def iolist_to_iovec(iodata), do: [:erlang.iolist_to_binary(iodata)]
  def is_record(term, tag), do: is_tuple(term) and tuple_size(term) > 0 and elem(term, 0) == tag
  def is_record(term, tag, size), do: is_record(term, tag) and tuple_size(term) == size

  def spawn(module, fun, args), do: :tonic.spawn_mfa(module, fun, args, false, false)
  def spawn_link(module, fun, args), do: :tonic.spawn_mfa(module, fun, args, true, false)
  def spawn_monitor(module, fun, args), do: :tonic.spawn_mfa(module, fun, args, false, true)
  def spawn(_node, module, fun, args), do: spawn(module, fun, args)
  def spawn_link(_node, module, fun, args), do: spawn_link(module, fun, args)
  def spawn_opt(fun, opts) when is_function(fun, 0), do: do_spawn_opt(fun, opts)
  def spawn_opt(_node, fun, opts) when is_function(fun, 0), do: do_spawn_opt(fun, opts)
  def spawn_opt(m, f, a, opts), do: do_spawn_opt({m, f, a}, opts)
  def spawn_opt(_node, m, f, a, opts), do: do_spawn_opt({m, f, a}, opts)

  defp do_spawn_opt(fun_or_mfa, opts) do
    link = :link in opts
    monitor = Enum.any?(opts, &(&1 == :monitor or match?({:monitor, _}, &1)))

    r =
      case fun_or_mfa do
        {m, f, a} -> :tonic.spawn_mfa(m, f, a, link, monitor)
        fun -> :tonic.spawn_opt(fun, link, monitor)
      end

    case List.keyfind(opts, :priority, 0) do
      {:priority, p} ->
        pid = if is_tuple(r), do: elem(r, 0), else: r
        :tonic.set_priority_of(pid, p)

      nil ->
        :ok
    end

    r
  end

  def monitor(type, {name, node} = item) when is_atom(name) and is_atom(node) do
    if node == node(), do: :tonic.monitor2(type, name), else: :erlang.error(:badarg, [type, item])
  end

  def monitor(type, item), do: :tonic.monitor2(type, item)

  def monitor(type, {name, node} = item, opts) when is_atom(name) and is_atom(node) do
    if node == node(), do: monitor(type, name, opts), else: :erlang.error(:badarg, [type, item, opts])
  end

  def monitor(type, item, opts) do
    ref =
      case List.keyfind(opts, :tag, 0) do
        {:tag, tag} -> :tonic.monitor_tag(item, tag)
        nil -> :tonic.monitor2(type, item)
      end

    case List.keyfind(opts, :alias, 0) do
      {:alias, :explicit_unalias} -> :tonic.alias_ref(ref, false)
      {:alias, _} -> :tonic.alias_ref(ref, true)
      nil -> ref
    end
  end

  def demonitor(ref, opts) when is_list(opts) do
    found = :tonic.demonitor_info(ref)

    if :flush in opts do
      receive do
        {_, ^ref, _, _, _} -> :ok
      after
        0 -> :ok
      end
    end

    if :info in opts, do: found, else: true
  end

  def alias, do: :tonic.alias()

  def alias(opts) do
    ref = :tonic.alias()
    if :reply in opts, do: :tonic.alias_reply(ref)
    ref
  end
  def unalias(ref), do: :tonic.unalias(ref)

  def hibernate(m, f, a) do
    :tonic.wait_message()
    apply(m, f, a)
  end

  def send_after(time, dest, msg, opts) do
    time = if Keyword.get(opts, :abs, false), do: max(time - :erlang.monotonic_time(:millisecond), 0), else: time
    :erlang.send_after(time, dest, msg)
  end

  def cancel_timer(ref, opts) do
    r = :erlang.cancel_timer(ref)

    cond do
      Keyword.get(opts, :async, false) ->
        if Keyword.get(opts, :info, true), do: send(self(), {:cancel_timer, ref, r})
        :ok

      Keyword.get(opts, :info, true) ->
        r

      true ->
        :ok
    end
  end

  def start_timer(time, dest, msg), do: :tonic.start_timer(time, dest, msg)
  def start_timer(time, dest, msg, _opts), do: :tonic.start_timer(time, dest, msg)
  def read_timer(ref), do: :tonic.read_timer(ref)
  def read_timer(ref, _opts), do: :tonic.read_timer(ref)

  def send(dest, msg, _opts) do
    :erlang.send(dest, msg)
    :ok
  end

  def fun_info_mfa(fun) do
    {:module, m} = fun_info(fun, :module)
    {:name, n} = fun_info(fun, :name)
    {:arity, a} = fun_info(fun, :arity)
    {m, n, a}
  end

  def fun_to_list(fun) when is_function(fun), do: :tonic.str_to_charlist(:tonic.fun_to_string(fun))
  def ref_to_list(ref) when is_reference(ref), do: :tonic.str_to_charlist("#Ref" <> String.trim_leading(:tonic.ref_to_string(ref), "#Reference"))
  def port_to_list(port), do: :tonic.str_to_charlist("#Port<0.0>") |> tap(fn _ -> port end)
  def pid_to_list(pid) when is_pid(pid), do: :tonic.str_to_charlist(String.trim_leading(:tonic.pid_to_string(pid), "#PID"))
  def yield, do: :tonic.yield()
  def garbage_collect(_pid), do: :erlang.garbage_collect()
  def erase, do: Enum.map(:erlang.get(), fn {k, v} -> :erlang.erase(k); {k, v} end)
  def get_keys, do: Enum.map(:erlang.get(), &elem(&1, 0))
  def get_keys(value), do: for({k, ^value} <- :erlang.get(), do: k)
  @default_info [:current_function, :initial_call, :status, :message_queue_len, :links, :dictionary,
                 :trap_exit, :error_handler, :priority, :group_leader, :total_heap_size, :heap_size,
                 :stack_size, :reductions, :garbage_collection, :suspending]

  def process_info(pid) when is_pid(pid) do
    case process_info(pid, @default_info) do
      :undefined -> :undefined
      info -> Enum.reject(info, &(elem(&1, 0) == :registered_name and elem(&1, 1) == []))
    end
  end

  def process_info(pid, items) when is_pid(pid) and is_list(items) do
    raw =
      if pid == self() do
        :tonic.process_info_self(items)
      else
        case :tonic.info_request(pid, items) do
          :undefined ->
            :undefined

          ref ->
            mref = :erlang.monitor(:process, pid)

            receive do
              {:"$tonic_info", ^ref, info} ->
                :erlang.demonitor(mref, [:flush])
                info

              {:DOWN, ^mref, _, _, _} ->
                :undefined
            end
        end
      end

    case raw do
      :undefined -> :undefined
      info -> Enum.map(info, &fix_info/1)
    end
  end

  def process_info(pid, {:dictionary, key} = item) when is_pid(pid) do
    case process_info(pid, :dictionary) do
      :undefined ->
        :undefined

      {:dictionary, dict} ->
        case List.keyfind(dict, key, 0) do
          {_, v} -> {item, v}
          nil -> {item, :undefined}
        end
    end
  end

  def process_info(pid, item) when is_pid(pid) and is_atom(item) do
    case process_info(pid, [item]) do
      :undefined -> :undefined
      [tuple] -> tuple
      [] -> :erlang.error(:badarg, [pid, item])
    end
  end

  defp fix_info({:group_leader, nil}), do: {:group_leader, Tonic.StdIO.pid()}
  defp fix_info(other), do: other
  def processes, do: Process.list()
  def halt, do: System.halt(0)
  def halt(status), do: System.halt(status)
  def halt(status, _opts), do: System.halt(status)
  def node(_term), do: :nonode@nohost
  def nodes, do: []
  def is_alive, do: false
  def get_cookie, do: :nocookie
  def bump_reductions(_n), do: true
  def nif_error(reason), do: :erlang.error(reason)
  def xor(a, b) when is_boolean(a) and is_boolean(b), do: a != b

  def make_fun(m, f, arity), do: :tonic.make_ext_fun(m, f, arity)

  def group_leader, do: group_leader_of(self())

  def group_leader(leader, pid) when is_pid(leader) and is_pid(pid) do
    if leader == Tonic.StdIO.pid() do
      :tonic.set_group_leader(pid, nil)
    else
      :tonic.set_group_leader(pid, leader)
    end

    true
  end

  @doc false
  def group_leader_of(pid), do: :tonic.group_leader_of(pid) || Tonic.StdIO.pid()

  def system_flag(flag, value) when is_atom(flag) do
    key = {:tonic_system_flag, flag}
    old = :persistent_term.get(key, system_flag_default(flag))
    :persistent_term.put(key, value)
    old
  end

  defp system_flag_default(:backtrace_depth), do: 8
  defp system_flag_default(:schedulers_online), do: :tonic.schedulers()
  defp system_flag_default(_), do: false

  def system_info(:schedulers), do: :tonic.schedulers()
  def system_info(:schedulers_online), do: :tonic.schedulers()
  def system_info(:logical_processors), do: :tonic.schedulers()
  def system_info(:logical_processors_available), do: :tonic.schedulers()
  def system_info(:otp_release), do: ~c"27"
  def system_info(:version), do: ~c"15.2.3"
  def system_info(:wordsize), do: 8
  def system_info(:endian), do: :little
  def system_info({:wordsize, :external}), do: 8
  def system_info(:process_count), do: length(Process.list())
  def system_info(:process_limit), do: 1_048_576
  def system_info(:atom_count), do: :tonic.atom_count()
  def system_info(:atom_limit), do: 1_048_576
  def system_info(:machine), do: ~c"BEAM"
  def system_info(:system_architecture), do: :tonic.system_architecture()
  def system_info(:thread_pool_size), do: 1
  def system_info(:emu_flavor), do: :jit
  def system_info(item), do: :erlang.error(:badarg, [item])

  def statistics(:wall_clock) do
    t = :erlang.monotonic_time(:millisecond)
    {t, 0}
  end

  def statistics(:runtime) do
    t = :erlang.monotonic_time(:millisecond)
    {t, 0}
  end

  def statistics(:reductions), do: {0, 0}
  def statistics(:run_queue), do: 0
  def statistics(:total_run_queue_lengths), do: 0

  def memory, do: [total: 0, processes: 0, processes_used: 0, system: 0, atom: 0, atom_used: 0, binary: 0, code: 0, ets: 0]
  def memory(type) when is_atom(type), do: Keyword.get(memory(), type, 0)

  def crc32(data), do: :tonic.crc32(0, :erlang.iolist_to_binary(data))
  def crc32(crc, data), do: :tonic.crc32(crc, :erlang.iolist_to_binary(data))
  def adler32(data), do: :tonic.adler32(1, :erlang.iolist_to_binary(data))
  def adler32(a, data), do: :tonic.adler32(a, :erlang.iolist_to_binary(data))
  def md5(data), do: :tonic.md5(:erlang.iolist_to_binary(data))
  def phash(term, range), do: :erlang.phash2(term, range) + 1

  def term_to_binary(term), do: :tonic.term_to_binary(term)
  def term_to_binary(term, _opts), do: :tonic.term_to_binary(term)
  def term_to_iovec(term), do: [:tonic.term_to_binary(term)]
  def binary_to_term(bin), do: :tonic.binary_to_term(bin)
  def binary_to_term(bin, _opts), do: :tonic.binary_to_term(bin)
  def external_size(term), do: byte_size(:tonic.term_to_binary(term))
  def external_size(term, _opts), do: byte_size(:tonic.term_to_binary(term))
end

defmodule :calendar do
  @days_per_400_years 146_097

  def local_time, do: :tonic.localtime()
  def universal_time, do: system_time_to_universal_time(:erlang.system_time(:second), :second)

  def system_time_to_universal_time(time, unit) do
    secs = System.convert_time_unit(time, unit, :second)
    gregorian_seconds_to_datetime(secs + 62_167_219_200)
  end

  def system_time_to_local_time(time, unit) do
    secs = System.convert_time_unit(time, unit, :second)
    gregorian_seconds_to_datetime(secs + 62_167_219_200 + :tonic.utc_offset())
  end

  def universal_time_to_local_time(dt), do: gregorian_seconds_to_datetime(datetime_to_gregorian_seconds(dt) + :tonic.utc_offset())
  def local_time_to_universal_time(dt), do: gregorian_seconds_to_datetime(datetime_to_gregorian_seconds(dt) - :tonic.utc_offset())

  def is_leap_year(y) when is_integer(y), do: rem(y, 4) == 0 and (rem(y, 100) != 0 or rem(y, 400) == 0)

  def last_day_of_the_month(y, 2), do: if(is_leap_year(y), do: 29, else: 28)
  def last_day_of_the_month(_y, m) when m in [4, 6, 9, 11], do: 30
  def last_day_of_the_month(_y, m) when is_integer(m) and m >= 1 and m <= 12, do: 31

  def valid_date({y, m, d}), do: valid_date(y, m, d)
  def valid_date(y, m, d) when is_integer(y) and is_integer(m) and is_integer(d) do
    m >= 1 and m <= 12 and d >= 1 and d <= last_day_of_the_month(y, m)
  end
  def valid_date(_, _, _), do: false

  def date_to_gregorian_days({y, m, d}), do: date_to_gregorian_days(y, m, d)

  def date_to_gregorian_days(year, month, day) do
    # days from 0000-01-01
    y = if month <= 2, do: year - 1, else: year
    era = div(if(y >= 0, do: y, else: y - 399), 400)
    yoe = y - era * 400
    mp = rem(month + 9, 12)
    doy = div(153 * mp + 2, 5) + day - 1
    doe = yoe * 365 + div(yoe, 4) - div(yoe, 100) + doy
    era * @days_per_400_years + doe + 60
  end

  def gregorian_days_to_date(days) do
    z = days - 60
    era = div(if(z >= 0, do: z, else: z - @days_per_400_years + 1), @days_per_400_years)
    doe = z - era * @days_per_400_years
    yoe = div(doe - div(doe, 1460) + div(doe, 36524) - div(doe, @days_per_400_years - 1), 365)
    doy = doe - (365 * yoe + div(yoe, 4) - div(yoe, 100))
    mp = div(5 * doy + 2, 153)
    d = doy - div(153 * mp + 2, 5) + 1
    m = if mp < 10, do: mp + 3, else: mp - 9
    y = yoe + era * 400 + if(m <= 2, do: 1, else: 0)
    {y, m, d}
  end

  def datetime_to_gregorian_seconds({date, {h, mi, s}}) do
    date_to_gregorian_days(date) * 86400 + h * 3600 + mi * 60 + s
  end

  def gregorian_seconds_to_datetime(secs) when is_integer(secs) do
    days = div(secs, 86400)
    rest = rem(secs, 86400)
    {gregorian_days_to_date(days), {div(rest, 3600), div(rem(rest, 3600), 60), rem(rest, 60)}}
  end

  def day_of_the_week({y, m, d}), do: day_of_the_week(y, m, d)
  def day_of_the_week(y, m, d), do: rem(date_to_gregorian_days(y, m, d) + 5, 7) + 1

  def time_to_seconds({h, m, s}), do: h * 3600 + m * 60 + s
  def seconds_to_time(secs), do: {div(secs, 3600), div(rem(secs, 3600), 60), rem(secs, 60)}
  def seconds_to_daystime(secs), do: {div(secs, 86400), seconds_to_time(rem(secs, 86400))}

  def time_difference(t1, t2) do
    seconds_to_daystime(datetime_to_gregorian_seconds(t2) - datetime_to_gregorian_seconds(t1))
  end
end

defmodule :crypto do
  def hash(type, data), do: :tonic.crypto_hash(type, IO.iodata_to_binary(data))
  def mac(:hmac, type, key, data), do: :tonic.crypto_mac(type, IO.iodata_to_binary(key), IO.iodata_to_binary(data))
  def hmac(type, key, data), do: mac(:hmac, type, key, data)
  def strong_rand_bytes(n), do: :tonic.strong_rand_bytes(n)
  def rand_uniform(lo, hi), do: lo + :rand.uniform(hi - lo) - 1
  def hash_init(type), do: {:tonic_hash, type, []}
  def hash_update({:tonic_hash, type, acc}, data), do: {:tonic_hash, type, [acc, data]}
  def hash_final({:tonic_hash, type, acc}), do: hash(type, acc)
  def exor(a, b), do: :crypto.exor_bin(IO.iodata_to_binary(a), IO.iodata_to_binary(b))
  def exor_bin(a, b), do: for({x, y} <- Enum.zip(:binary.bin_to_list(a), :binary.bin_to_list(b)), into: <<>>, do: <<Bitwise.bxor(x, y)>>)
end

defmodule :os do
  def type do
    case :tonic.system_architecture() |> List.to_string() do
      "aarch64-apple-darwin" <> _ -> {:unix, :darwin}
      "x86_64-apple-darwin" <> _ -> {:unix, :darwin}
      _ -> {:unix, :linux}
    end
  end

  def getenv(name), do: (case System.get_env(List.to_string(name)) do nil -> false; v -> :tonic.str_to_charlist(v) end)
  def getenv(name, default), do: (case getenv(name) do false -> default; v -> v end)
  def getenv, do: Enum.map(System.get_env(), fn {k, v} -> :tonic.str_to_charlist(k <> "=" <> v) end)
  def putenv(name, value), do: (System.put_env(List.to_string(name), List.to_string(value)); true)
  def unsetenv(name), do: (System.delete_env(List.to_string(name)); true)
  def getpid, do: :tonic.str_to_charlist(System.pid())
  def system_time, do: :erlang.system_time()
  def system_time(unit), do: :erlang.system_time(unit)
  def timestamp, do: :erlang.timestamp()
  def cmd(command), do: :tonic.str_to_charlist(elem(System.shell(List.to_string(command)), 0))
end


defmodule :io do
  def printable_range, do: :unicode

  def scan_erl_form(prompt), do: scan_erl_form(:standard_io, prompt, 1, [])
  def scan_erl_form(io, prompt), do: scan_erl_form(io, prompt, 1, [])
  def scan_erl_form(io, prompt, start), do: scan_erl_form(io, prompt, start, [])

  def scan_erl_form(io, prompt, start, opts),
    do: request(io, {:get_until, :unicode, prompt, :erl_scan, :tokens, [start, opts]})

  def scan_erl_exprs(prompt), do: scan_erl_exprs(:standard_io, prompt, 1, [])
  def scan_erl_exprs(io, prompt), do: scan_erl_exprs(io, prompt, 1, [])
  def scan_erl_exprs(io, prompt, start), do: scan_erl_exprs(io, prompt, start, [])

  def scan_erl_exprs(io, prompt, start, opts),
    do: request(io, {:get_until, :unicode, prompt, :erl_scan, :tokens, [start, opts]})
  def format(format), do: format(format, [])
  def format(format, args) when is_list(args), do: IO.write(List.to_string(:io_lib.format(format, args)))
  def format(device, format, args), do: IO.write(device, List.to_string(:io_lib.format(format, args)))
  def fwrite(format), do: format(format, [])
  def fwrite(format, args), do: format(format, args)
  def fwrite(device, format, args), do: format(device, format, args)
  def put_chars(chars), do: IO.write(chars)
  def put_chars(device, chars), do: IO.write(device, chars)
  def nl, do: IO.write("\n")
  def nl(device), do: IO.write(device, "\n")
  def write(term), do: IO.write(:tonic.erl_write(term, false))
  def write(device, term), do: IO.write(device, :tonic.erl_write(term, false))
  def get_line(prompt), do: (case IO.gets(prompt) do s when is_binary(s) -> :tonic.str_to_charlist(s); other -> other end)
  def get_line(device, prompt) when is_pid(device), do: :file.io_get_line(device, :unicode, prompt)
  def get_line(_device, prompt), do: get_line(prompt)
  def get_chars(prompt, count), do: get_chars(:standard_io, prompt, count)
  def get_chars(device, prompt, count), do: :file.io_get_chars(device, :unicode, prompt, count)
  def columns, do: {:error, :enotsup}
  def columns(_device), do: {:error, :enotsup}
  def rows, do: {:error, :enotsup}
  def setopts(_opts), do: :ok
  def setopts(device, opts) when is_pid(device), do: Tonic.FileIO.request(device, {:setopts, opts})
  def setopts(_device, _opts), do: :ok
  def getopts(device) when is_pid(device), do: Tonic.FileIO.request(device, :getopts)
  def getopts(_device), do: [binary: true, encoding: :unicode]
  def getopts, do: []

  def request(request), do: request(:standard_io, request)

  def request(device, request) do
    dev = resolve_device(device)

    cond do
      is_pid(dev) -> Tonic.FileIO.request(dev, request)
      true -> {:error, :request}
    end
  end

  def requests(requests), do: requests(:standard_io, requests)
  def requests(device, requests), do: request(device, {:requests, requests})

  def get_password, do: get_password(:standard_io)

  def get_password(device) do
    case request(device, {:get_password, :unicode}) do
      {:error, _} -> get_line(device, ~c"")
      other -> other
    end
  end

  def read(prompt), do: read(:standard_io, prompt)
  def read(_device, _prompt), do: {:error, :enotsup}
  def fread(prompt, format), do: fread(:standard_io, prompt, format)

  def fread(device, prompt, format) do
    case get_line(device, prompt) do
      :eof -> :eof
      {:error, _} = e -> e
      line ->
        case :io_lib_fread.fread(:tonic.str_to_charlist(IO.chardata_to_string(line)), format) do
          {:ok, results, _rest} -> {:ok, results}
          other -> other
        end
    end
  end

  defp resolve_device(:standard_io) do
    case :tonic.group_leader_of(self()) do
      nil -> Tonic.StdIO.pid()
      gl -> gl
    end
  end

  defp resolve_device(:user), do: Tonic.StdIO.pid()
  defp resolve_device(name) when is_atom(name), do: Process.whereis(name)
  defp resolve_device(pid) when is_pid(pid), do: pid
end

defmodule :ets do
  def new(name, options), do: :tonic.ets_new(name, options)
  def insert(tab, objects), do: :tonic.ets_insert(tab, objects)
  def insert_new(tab, objects), do: :tonic.ets_insert_new(tab, objects)
  def lookup(tab, key), do: :tonic.ets_lookup(tab, key)
  def lookup_element(tab, key, pos), do: :tonic.ets_lookup_element(tab, key, pos, [])
  def lookup_element(tab, key, pos, default), do: :tonic.ets_lookup_element(tab, key, pos, {default})
  def member(tab, key), do: :tonic.ets_member(tab, key)
  def delete(tab), do: :tonic.ets_delete_table(tab)
  def delete(tab, key), do: :tonic.ets_delete(tab, key)
  def delete_object(tab, object), do: :tonic.ets_delete_object(tab, object)
  def delete_all_objects(tab), do: :tonic.ets_delete_all_objects(tab)
  def take(tab, key), do: :tonic.ets_take(tab, key)
  def tab2list(tab), do: :tonic.ets_tab2list(tab)
  def first(tab), do: :tonic.ets_first(tab, true)
  def last(tab), do: :tonic.ets_first(tab, false)
  def next(tab, key), do: :tonic.ets_next(tab, key, true)
  def prev(tab, key), do: :tonic.ets_next(tab, key, false)
  def whereis(name), do: :tonic.ets_whereis(name)
  def all, do: :tonic.ets_all()
  def rename(tab, name), do: :tonic.ets_rename(tab, name)
  def give_away(_tab, _pid, _data), do: true
  def setopts(_tab, _opts), do: true
  def safe_fixtable(_tab, _fix), do: true

  def info(tab) do
    case :tonic.ets_info(tab, :size) do
      :undefined ->
        :undefined

      _ ->
        for item <- [:id, :decentralized_counters, :read_concurrency, :write_concurrency, :compressed, :memory, :owner, :heir, :name, :size, :node, :named_table, :type, :keypos, :protection] do
          {item, if(item == :node, do: :nonode@nohost, else: :tonic.ets_info(tab, item))}
        end
    end
  end

  def info(tab, item), do: :tonic.ets_info(tab, item)

  def update_counter(tab, key, op), do: do_update_counter(tab, key, op, [])
  def update_counter(tab, key, op, default), do: do_update_counter(tab, key, op, default)

  defp do_update_counter(tab, key, incr, default) when is_integer(incr) do
    [v] = :tonic.ets_update_counter(tab, key, [{2, incr}], default)
    v
  end

  defp do_update_counter(tab, key, op, default) when is_tuple(op) do
    [v] = :tonic.ets_update_counter(tab, key, [op], default)
    v
  end

  defp do_update_counter(tab, key, ops, default) when is_list(ops) do
    :tonic.ets_update_counter(tab, key, ops, default)
  end

  def update_element(tab, key, {pos, value}), do: update_element(tab, key, [{pos, value}])

  def update_element(tab, key, updates) when is_list(updates) do
    case lookup(tab, key) do
      [] ->
        false

      [obj | _] ->
        obj = Enum.reduce(updates, obj, fn {pos, value}, acc -> put_elem(acc, pos - 1, value) end)
        :tonic.ets_replace(tab, obj)
    end
  end

  def update_element(tab, key, update, default) do
    if member(tab, key), do: update_element(tab, key, update), else: (insert(tab, default); update_element(tab, key, update))
  end

  def foldl(fun, acc, tab), do: List.foldl(tab2list(tab), acc, fun)
  def foldr(fun, acc, tab), do: List.foldr(tab2list(tab), acc, fun)

  # ---- match specifications (evaluated over a snapshot of the table) ----

  def match_object(tab, pattern), do: for(obj <- tab2list(tab), match?({:ok, _}, ms_match(pattern, obj, %{})), do: obj)
  def match_object(tab, pattern, limit), do: chunked(match_object(tab, pattern), limit)

  def match(tab, pattern) do
    for obj <- tab2list(tab), {:ok, b} <- [ms_match(pattern, obj, %{})] do
      b |> Enum.sort() |> Enum.map(&elem(&1, 1))
    end
  end

  def match(tab, pattern, limit), do: chunked(match(tab, pattern), limit)
  def match_delete(tab, pattern), do: (Enum.each(match_object(tab, pattern), &delete_object(tab, &1)); true)

  def select(tab, ms), do: :ets.select_impl(tab2list(tab), ms)
  def select(tab, ms, limit), do: chunked(select(tab, ms), limit)
  def select({results, :"$end_of_table"}), do: {results, :"$end_of_table"}
  def select(:"$end_of_table"), do: :"$end_of_table"
  def select_reverse(tab, ms), do: Enum.reverse(select(tab, ms))
  def select_count(tab, ms), do: Enum.count(:ets.select_impl(tab2list(tab), ms), &(&1 == true))

  def select_delete(tab, ms) do
    objs = for obj <- tab2list(tab), ms_run(ms, obj) == {:ok, true}, do: obj
    Enum.each(objs, &delete_object(tab, &1))
    length(objs)
  end

  def select_replace(tab, ms) do
    n =
      for obj <- tab2list(tab), {:ok, new} <- [ms_run(ms, obj)], is_tuple(new) do
        delete_object(tab, obj)
        insert(tab, new)
      end

    length(n)
  end

  def test_ms(tuple, ms), do: (case ms_run(ms, tuple) do {:ok, r} -> {:ok, r}; :nomatch -> {:ok, false} end)

  def match_spec_compile(ms), do: ms
  def match_spec_run(list, ms), do: :ets.select_impl(list, ms)
  def is_compiled_ms(_), do: true

  def fun2ms(_fun), do: :erlang.error(:undef)

  @doc false
  def select_impl(objs, ms) do
    for obj <- objs, {:ok, r} <- [ms_run(ms, obj)], do: r
  end

  defp chunked([], _limit), do: :"$end_of_table"
  defp chunked(list, _limit), do: {list, :"$end_of_table"}

  defp ms_run([], _obj), do: :nomatch

  defp ms_run([{head, guards, body} | rest], obj) do
    with {:ok, b} <- ms_match(head, obj, %{}),
         true <- Enum.all?(guards, &(ms_eval(&1, b, obj) == true)) do
      {:ok, ms_body(body, b, obj)}
    else
      _ -> ms_run(rest, obj)
    end
  end

  defp ms_body([], _b, _obj), do: nil
  defp ms_body(body, b, obj), do: body |> Enum.map(&ms_eval(&1, b, obj)) |> List.last()

  defp ms_match(:_, _v, b), do: {:ok, b}

  defp ms_match(p, v, b) when is_atom(p) do
    case ms_var(p) do
      nil -> if p == v, do: {:ok, b}, else: :nomatch
      n ->
        case b do
          %{^n => ^v} -> {:ok, b}
          %{^n => _} -> :nomatch
          _ -> {:ok, Map.put(b, n, v)}
        end
    end
  end

  defp ms_match(p, v, b) when is_tuple(p) and is_tuple(v) and tuple_size(p) == tuple_size(v) do
    ms_match(Tuple.to_list(p), Tuple.to_list(v), b)
  end

  defp ms_match([ph | pt], [vh | vt], b) do
    with {:ok, b} <- ms_match(ph, vh, b), do: ms_match(pt, vt, b)
  end

  defp ms_match(p, v, b) when is_map(p) and is_map(v) do
    Enum.reduce_while(p, {:ok, b}, fn {k, pv}, {:ok, b} ->
      case Map.fetch(v, k) do
        {:ok, vv} -> (case ms_match(pv, vv, b) do {:ok, b} -> {:cont, {:ok, b}}; :nomatch -> {:halt, :nomatch} end)
        :error -> {:halt, :nomatch}
      end
    end)
  end

  defp ms_match(p, v, b), do: if(p === v, do: {:ok, b}, else: :nomatch)

  defp ms_var(a) do
    case Atom.to_string(a) do
      "$" <> rest -> (case Integer.parse(rest) do {n, ""} -> n; _ -> nil end)
      _ -> nil
    end
  end

  defp ms_eval(:"$_", _b, obj), do: obj
  defp ms_eval(:"$$", b, _obj), do: b |> Enum.sort() |> Enum.map(&elem(&1, 1))

  defp ms_eval(a, b, _obj) when is_atom(a) do
    case ms_var(a) do
      nil -> a
      n -> Map.fetch!(b, n)
    end
  end

  defp ms_eval({:const, c}, _b, _obj), do: c
  defp ms_eval({t}, b, obj) when is_tuple(t), do: t |> Tuple.to_list() |> Enum.map(&ms_eval(&1, b, obj)) |> List.to_tuple()
  defp ms_eval({:andalso, x, y}, b, obj), do: ms_eval(x, b, obj) == true and ms_eval(y, b, obj) == true
  defp ms_eval({:orelse, x, y}, b, obj), do: ms_eval(x, b, obj) == true or ms_eval(y, b, obj) == true
  defp ms_eval({:and, x, y}, b, obj), do: ms_eval(x, b, obj) == true and ms_eval(y, b, obj) == true
  defp ms_eval({:or, x, y}, b, obj), do: ms_eval(x, b, obj) == true or ms_eval(y, b, obj) == true
  defp ms_eval({:not, x}, b, obj), do: not (ms_eval(x, b, obj) == true)

  defp ms_eval({op, x, y}, b, obj) when op in [:==, :"=:=", :"/=", :"=/=", :<, :>, :"=<", :>=, :+, :-, :*, :/, :div, :rem, :band, :bor, :element, :map_get, :is_map_key] do
    x = ms_eval(x, b, obj)
    y = ms_eval(y, b, obj)

    case op do
      :== -> x == y
      :"=:=" -> x === y
      :"/=" -> x != y
      :"=/=" -> x !== y
      :< -> x < y
      :> -> x > y
      :"=<" -> x <= y
      :>= -> x >= y
      :+ -> x + y
      :- -> x - y
      :* -> x * y
      :/ -> x / y
      :div -> div(x, y)
      :rem -> rem(x, y)
      :band -> Bitwise.band(x, y)
      :bor -> Bitwise.bor(x, y)
      :element -> elem(y, x - 1)
      :map_get -> Map.fetch!(y, x)
      :is_map_key -> Map.has_key?(y, x)
    end
  end

  defp ms_eval({f, x}, b, obj) when f in [:is_atom, :is_integer, :is_float, :is_number, :is_binary, :is_list, :is_tuple, :is_map, :is_pid, :is_function, :is_boolean, :hd, :tl, :length, :abs, :size, :tuple_size, :map_size, :byte_size, :-] do
    x = ms_eval(x, b, obj)

    case f do
      :is_atom -> is_atom(x)
      :is_integer -> is_integer(x)
      :is_float -> is_float(x)
      :is_number -> is_number(x)
      :is_binary -> is_binary(x)
      :is_list -> is_list(x)
      :is_tuple -> is_tuple(x)
      :is_map -> is_map(x)
      :is_pid -> is_pid(x)
      :is_function -> is_function(x)
      :is_boolean -> is_boolean(x)
      :hd -> hd(x)
      :tl -> tl(x)
      :length -> length(x)
      :abs -> abs(x)
      :size -> if is_tuple(x), do: tuple_size(x), else: byte_size(x)
      :tuple_size -> tuple_size(x)
      :map_size -> map_size(x)
      :byte_size -> byte_size(x)
      :- -> -x
    end
  end

  defp ms_eval(l, b, obj) when is_list(l), do: Enum.map(l, &ms_eval(&1, b, obj))
  defp ms_eval(m, b, obj) when is_map(m), do: Map.new(m, fn {k, v} -> {ms_eval(k, b, obj), ms_eval(v, b, obj)} end)
  defp ms_eval(other, _b, _obj), do: other
end

defmodule :otp_internal do
  @moduledoc false
  def obsolete(_module, _function, _arity), do: :no
end

defmodule :code do
  @moduledoc false
  def all_available, do: []
  def all_loaded, do: []
  def which(_module), do: :non_existing
  def is_loaded(module), do: if(Code.ensure_loaded?(module), do: {:file, :in_memory}, else: false)
  def ensure_loaded(module), do: if(Code.ensure_loaded?(module), do: {:module, module}, else: {:error, :nofile})
  def get_object_code(_module), do: :error
  def priv_dir(_app), do: {:error, :bad_name}
  def lib_dir, do: ~c"/usr/lib/erlang/lib"

  # Modules are compiled into the executable: nothing to purge or delete.
  def purge(_module), do: false
  def soft_purge(_module), do: true
  def delete(_module), do: false

  def lib_dir(app) when is_atom(app) do
    case :application.get_key(app, :vsn) do
      {:ok, vsn} when app in [:elixir, :logger, :ex_unit, :iex, :mix, :eex] ->
        _ = vsn
        ~c"/usr/lib/elixir/lib/" ++ Atom.to_charlist(app)

      {:ok, vsn} ->
        ~c"/usr/lib/erlang/lib/" ++ Atom.to_charlist(app) ++ ~c"-" ++ vsn

      :undefined ->
        {:error, :bad_name}
    end
  end
  def root_dir, do: ~c"/usr/lib/erlang"
  def ensure_modules_loaded(_mods), do: :ok
end

defmodule :epp do
  @moduledoc false
  def default_encoding, do: :utf8
end


defmodule :erl_erts_errors do
  # The runtime attaches per-argument descriptions to the failing BIF's
  # stack frame (error_info `tonic_args`), mirroring OTP's error_info.
  def format_error(_reason, [{_m, _f, _args, info} | _]) when is_list(info) do
    case Keyword.get(info, :error_info) do
      %{tonic_args: args} when is_map(args) -> args
      _ -> %{}
    end
  end

  def format_error(_reason, _stacktrace), do: %{}
end

defmodule :error_logger do
  def get_format_depth, do: :unlimited
  def limit_term(term), do: term
  def format(format, args), do: :logger.error(format, args)
  def error_msg(format), do: :logger.error(format, [])
  def error_msg(format, args), do: :logger.error(format, args)
  def warning_msg(format), do: :logger.warning(format, [])
  def warning_msg(format, args), do: :logger.warning(format, args)
  def info_msg(format), do: :logger.info(format, [])
  def info_msg(format, args), do: :logger.info(format, args)
  def error_report(report), do: :logger.error(report)
  def info_report(report), do: :logger.info(report)
  def warning_report(report), do: :logger.warning(report)
end

defmodule :persistent_term do
  @tab :"$tonic_persistent_term"

  defp tab do
    if :ets.whereis(@tab) == :undefined do
      parent = self()

      spawn(fn ->
        try do
          :ets.new(@tab, [:set, :public, :named_table, read_concurrency: true])
        rescue
          _ -> :ok
        end

        Kernel.send(parent, :tonic_pt_ready)

        receive do
          :tonic_never -> :ok
        end
      end)

      receive do
        :tonic_pt_ready -> :ok
      end
    end

    @tab
  end

  def put(key, value) do
    :ets.insert(tab(), {key, value})
    :ok
  end

  def get(key) do
    case :ets.lookup(tab(), key) do
      [{_, v}] -> v
      [] -> :erlang.error(:badarg, [key])
    end
  end

  def get(key, default) do
    case :ets.lookup(tab(), key) do
      [{_, v}] -> v
      [] -> default
    end
  end

  def get, do: :ets.tab2list(tab())

  def erase(key) do
    case :ets.lookup(tab(), key) do
      [_] ->
        :ets.delete(tab(), key)
        true

      [] ->
        false
    end
  end

  def info, do: %{count: :ets.info(tab(), :size), memory: :ets.info(tab(), :memory)}
end

defmodule :erl_features do
  # OTP 27: maybe_expr is enabled by default.
  def keywords, do: [:maybe, :else]
  def keywords(:maybe_expr), do: [:maybe, :else]
  def keywords(_), do: []
  def enabled, do: [:maybe_expr]
  def used(_), do: []
  def all, do: [:maybe_expr]
  def configurable, do: [:maybe_expr]
end

defmodule :elixir_env do
  @moduledoc false
  def trace(_event, _env), do: :ok
end

defmodule :elixir_dispatch do
  @moduledoc false
  # Import lookup used when quoting: tonic's Kernel imports.
  def find_import(_meta, name, arity, _env) do
    if function_exported?(Kernel, name, arity) or macro_exported?(Kernel, name, arity), do: Kernel, else: false
  end

  def find_imports(_meta, name, _env) do
    for arity <- 0..9,
        function_exported?(Kernel, name, arity) or macro_exported?(Kernel, name, arity),
        do: {arity, Kernel}
  end
end

defmodule :elixir_module do
  @moduledoc false
  def next_counter(_module), do: :erlang.unique_integer([:positive])
end

defmodule :elixir_import do
  @moduledoc false
  def special_form(name, arity) do
    {name, arity} in [
      {:%, 2}, {:%{}, :_}, {:&, 1}, {:., 2}, {:"::", 2}, {:<<>>, :_}, {:=, 2}, {:^, 1},
      {:__aliases__, :_}, {:__block__, :_}, {:__CALLER__, 0}, {:__DIR__, 0}, {:__ENV__, 0},
      {:__MODULE__, 0}, {:__STACKTRACE__, 0}, {:alias, 1}, {:alias, 2}, {:case, 2}, {:cond, 1},
      {:fn, :_}, {:for, :_}, {:import, 1}, {:import, 2}, {:quote, 1}, {:quote, 2},
      {:receive, 1}, {:require, 1}, {:require, 2}, {:super, :_}, {:try, 1}, {:unquote, 1},
      {:unquote_splicing, 1}, {:with, :_}, {:{}, :_}
    ] or {name, :_} in [{:%{}, :_}, {:<<>>, :_}, {:__aliases__, :_}, {:__block__, :_}, {:fn, :_}, {:for, :_}, {:super, :_}, {:with, :_}, {:{}, :_}]
  end
end
