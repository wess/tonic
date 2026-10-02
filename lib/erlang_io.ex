# :binary, :filename, :filelib, :file (OTP 27 compatible)
# Modified for Tonic; Erlang/OTP 27.3 source/port. Apache-2.0; see licenses/sources.json and notice.
defmodule :binary do
  # part/3, at/2, copy/2, bin_to_list/1, match/2 and matches/2 are compiler intrinsics.

  def bin_to_list(subject, {pos, len}), do: bin_to_list(subject, pos, len)
  def bin_to_list(_subject, _bad), do: :erlang.error(:badarg)

  def bin_to_list(subject, pos, len)
      when not is_binary(subject) or not is_integer(pos) or not is_integer(len),
      do: :erlang.error(:badarg)

  def bin_to_list(subject, pos, 0) when pos >= 0 and pos <= byte_size(subject), do: []
  def bin_to_list(subject, pos, len), do: :binary.bin_to_list(:binary.part(subject, pos, len))

  def compile_pattern(pattern) do
    if valid_pattern?(pattern), do: pattern, else: :erlang.error(:badarg)
  end

  defp valid_pattern?(p) when is_binary(p), do: byte_size(p) > 0
  defp valid_pattern?([_ | _] = l), do: Enum.all?(l, fn x -> is_binary(x) and byte_size(x) > 0 end)
  defp valid_pattern?(_), do: false

  def copy(subject), do: :binary.copy(subject, 1)

  def decode_hex(hex), do: :tonic.bf_decode_hex(hex)

  def encode_hex(bin), do: :tonic.bf_encode_hex(bin, true)
  def encode_hex(bin, :uppercase), do: :tonic.bf_encode_hex(bin, true)
  def encode_hex(bin, :lowercase), do: :tonic.bf_encode_hex(bin, false)
  def encode_hex(_bin, _case), do: :erlang.error(:badarg)

  def decode_unsigned(subject), do: decode_unsigned(subject, :big)

  def decode_unsigned(subject, :big) when is_binary(subject), do: dec_unsigned(:binary.bin_to_list(subject), 0)

  def decode_unsigned(subject, :little) when is_binary(subject),
    do: dec_unsigned(:lists.reverse(:binary.bin_to_list(subject)), 0)

  def decode_unsigned(_subject, _endianness), do: :erlang.error(:badarg)

  defp dec_unsigned([], acc), do: acc
  defp dec_unsigned([b | t], acc), do: dec_unsigned(t, acc * 256 + b)

  def encode_unsigned(unsigned), do: encode_unsigned(unsigned, :big)

  def encode_unsigned(unsigned, endianness)
      when is_integer(unsigned) and unsigned >= 0 and endianness in [:big, :little] do
    bytes = enc_unsigned(unsigned, [])
    bytes = if endianness == :little, do: :lists.reverse(bytes), else: bytes
    :erlang.list_to_binary(bytes)
  end

  def encode_unsigned(_unsigned, _endianness), do: :erlang.error(:badarg)

  defp enc_unsigned(n, acc) when n < 256, do: [n | acc]
  defp enc_unsigned(n, acc), do: enc_unsigned(div(n, 256), [rem(n, 256) | acc])

  def first(<<f, _::binary>>), do: f
  def first(_), do: :erlang.error(:badarg)

  def last(subject) when is_binary(subject) and byte_size(subject) > 0,
    do: :binary.at(subject, byte_size(subject) - 1)

  def last(_), do: :erlang.error(:badarg)

  def list_to_bin(list) when is_list(list), do: :erlang.list_to_binary(list)
  def list_to_bin(_), do: :erlang.error(:badarg)

  def longest_common_prefix(binaries), do: :tonic.bf_common(binaries, false)
  def longest_common_suffix(binaries), do: :tonic.bf_common(binaries, true)

  def match(subject, pattern, options) do
    {s, l} = scope_opt(subject, options)
    :tonic.bf_match(subject, pattern, s, l, false)
  end

  def matches(subject, pattern, options) do
    {s, l} = scope_opt(subject, options)
    :tonic.bf_match(subject, pattern, s, l, true)
  end

  def part(subject, {pos, len}), do: :binary.part(subject, pos, len)
  def part(_subject, _poslen), do: :erlang.error(:badarg)

  def referenced_byte_size(binary) when is_binary(binary), do: byte_size(binary)
  def referenced_byte_size(_), do: :erlang.error(:badarg)

  def split(subject, pattern), do: split(subject, pattern, [])

  def split(subject, pattern, options) when is_list(options) do
    {flags, scope} = split_opts(options, 0, nil)
    {s, l} = norm_scope(subject, scope)
    :tonic.bf_split(subject, pattern, s, l, flags)
  end

  def split(_subject, _pattern, _options), do: :erlang.error(:badarg)

  defp split_opts([], flags, scope), do: {flags, scope}
  defp split_opts([:global | t], flags, scope), do: split_opts(t, Bitwise.bor(flags, 1), scope)
  defp split_opts([:trim | t], flags, scope), do: split_opts(t, Bitwise.bor(flags, 2), scope)
  defp split_opts([:trim_all | t], flags, scope), do: split_opts(t, Bitwise.bor(flags, 4), scope)
  defp split_opts([{:scope, {_, _} = s} | t], flags, _scope), do: split_opts(t, flags, s)
  defp split_opts(_, _flags, _scope), do: :erlang.error(:badarg)

  defp scope_opt(subject, options) when is_list(options) do
    scope =
      Enum.reduce(options, nil, fn
        {:scope, {_, _} = s}, _ -> s
        _, _ -> :erlang.error(:badarg)
      end)

    norm_scope(subject, scope)
  end

  defp scope_opt(_subject, _options), do: :erlang.error(:badarg)

  defp norm_scope(subject, nil) when is_binary(subject), do: {0, byte_size(subject)}
  defp norm_scope(_subject, nil), do: :erlang.error(:badarg)

  defp norm_scope(subject, {s, l}) when is_integer(s) and is_integer(l) and is_binary(subject) do
    {s, l} = if l < 0, do: {s + l, -l}, else: {s, l}
    if s < 0 or s + l > byte_size(subject), do: :erlang.error(:badarg), else: {s, l}
  end

  defp norm_scope(_subject, _scope), do: :erlang.error(:badarg)

  def replace(subject, pattern, replacement), do: replace(subject, pattern, replacement, [])

  def replace(subject, pattern, replacement, options)
      when (is_binary(replacement) or is_function(replacement, 1)) and is_list(options) do
    {scope, global, insert} = replace_opts(options, {nil, false, []})

    if is_binary(replacement) and insert == [] do
      scope = if scope == nil, do: nil, else: norm_scope(subject, scope)
      :tonic.bf_replace(subject, pattern, replacement, scope, global)
    else
      mopts = if scope == nil, do: [], else: [{:scope, scope}]

      mlist =
        if global do
          matches(subject, pattern, mopts)
        else
          case match(subject, pattern, mopts) do
            :nomatch -> []
            m -> [m]
          end
        end

      repl =
        cond do
          is_function(replacement, 1) ->
            replacement

          is_integer(insert) ->
            <<front::binary-size(insert), rear::binary>> = replacement
            fn m -> [front, m, rear] end

          is_list(insert) ->
            splits = splitat(replacement, 0, :lists.sort(insert))
            fn m -> Enum.intersperse(splits, m) end
        end

      :erlang.iolist_to_binary(do_replace(subject, mlist, repl, 0))
    end
  end

  def replace(_subject, _pattern, _replacement, _options), do: :erlang.error(:badarg)

  defp do_replace(h, [], _, n), do: [:binary.part(h, n, byte_size(h) - n)]

  defp do_replace(h, [{a, b} | t], repl, n),
    do: [:binary.part(h, n, a - n), repl.(:binary.part(h, a, b)) | do_replace(h, t, repl, a + b)]

  defp splitat(h, n, []), do: [:binary.part(h, n, byte_size(h) - n)]
  defp splitat(h, n, [i | t]), do: [:binary.part(h, n, i - n) | splitat(h, i, t)]

  defp replace_opts([], acc), do: acc
  defp replace_opts([{:scope, {_, _} = s} | t], {_, g, i}), do: replace_opts(t, {s, g, i})
  defp replace_opts([:global | t], {s, _, i}), do: replace_opts(t, {s, true, i})
  defp replace_opts([{:insert_replaced, n} | t], {s, g, _}), do: replace_opts(t, {s, g, n})
  defp replace_opts(_, _), do: :erlang.error(:badarg)
end

defmodule :filename do
  # Port of OTP 27 filename.erl (unix flavour).

  def absname(name) do
    {:ok, cwd} = :file.get_cwd()
    absname(name, cwd)
  end

  def absname(name, abs_base) when is_binary(name) and is_list(abs_base),
    do: absname(name, fstb(abs_base))

  def absname(name, abs_base) when is_list(name) and is_binary(abs_base),
    do: absname(fstb(name), abs_base)

  def absname(name, abs_base) do
    case pathtype(name) do
      :relative -> absname_join(abs_base, name)
      _ -> join([flatten(name)])
    end
  end

  def absname_join(abs_base, name), do: join(abs_base, flatten(name))

  def basename(name) when is_binary(name) do
    parts = for x <- :binary.split(name, ["/"], [:global]), x != <<>>, do: x
    if parts == [], do: <<>>, else: :lists.last(parts)
  end

  def basename(name0) do
    name = flatten(name0)
    basename1(name, name)
  end

  defp basename1([?/], tail0) do
    [_ | tail] = :lists.reverse(tail0)
    :lists.reverse(tail)
  end

  defp basename1([?/ | rest], _tail), do: basename1(rest, rest)
  defp basename1([char | rest], tail) when is_integer(char), do: basename1(rest, tail)
  defp basename1([], tail), do: tail

  def basename(name, ext) when is_binary(name) and is_list(ext), do: basename(name, fstb(ext))
  def basename(name, ext) when is_list(name) and is_binary(ext), do: basename(fstb(name), ext)

  def basename(name, ext) when is_binary(name) and is_binary(ext) do
    bname = basename(name)
    lall = byte_size(name)
    ln = byte_size(bname)
    le = byte_size(ext)

    case ln - le do
      neg when neg < 0 ->
        bname

      pos ->
        start_len = lall - pos - le

        case name do
          <<_::binary-size(start_len), part::binary-size(pos), ^ext::binary>> -> part
          _ -> bname
        end
    end
  end

  def basename(name0, ext0) do
    name = flatten(name0)
    ext = flatten(ext0)
    basename4(name, ext, [])
  end

  defp basename4(ext, ext, tail), do: :lists.reverse(tail)
  defp basename4([?/], ext, tail), do: basename4([], ext, tail)
  defp basename4([?/ | rest], ext, _tail), do: basename4(rest, ext, [])
  defp basename4([char | rest], ext, tail) when is_integer(char), do: basename4(rest, ext, [char | tail])
  defp basename4([], _ext, tail), do: :lists.reverse(tail)

  def dirname(name) when is_binary(name) do
    parts0 = :binary.split(name, ["/"], [:global])

    parts =
      case parts0 do
        [] -> []
        _ -> :lists.reverse(fstrip(tl(:lists.reverse(parts0))))
      end

    xpart = if parts == [], do: ".", else: <<>>
    dirjoin(parts, xpart, "/")
  end

  def dirname(name0) do
    name = flatten(name0)
    dirname4(name, [], [])
  end

  defp dirname4([?/ | rest], dir, file), do: dirname4(rest, file ++ dir, [?/])
  defp dirname4([char | rest], dir, file) when is_integer(char), do: dirname4(rest, dir, [char | file])

  defp dirname4([], [], file) do
    case :lists.reverse(file) do
      [?/ | _] -> [?/]
      _ -> ~c"."
    end
  end

  defp dirname4([], [?/ | rest], file), do: dirname4([], rest, file)
  defp dirname4([], dir, _), do: :lists.reverse(dir)

  defp fstrip([<<>>, x | y]), do: fstrip([x | y])
  defp fstrip(a), do: a

  defp dirjoin([<<>> | t], acc, sep), do: dirjoin1(t, <<acc::binary, "/">>, sep)
  defp dirjoin(a, b, c), do: dirjoin1(a, b, c)

  defp dirjoin1([], acc, _), do: acc
  defp dirjoin1([one], acc, _), do: <<acc::binary, one::binary>>
  defp dirjoin1([h | t], acc, sep), do: dirjoin(t, <<acc::binary, h::binary, sep::binary>>, sep)

  def extension(name) when is_binary(name) do
    case :binary.matches(name, ["."]) do
      [] ->
        <<>>

      list ->
        case :lists.last(list) do
          {0, _} ->
            <<>>

          {pos, _} ->
            part = :binary.part(name, pos - 1, byte_size(name) - pos + 1)

            case :binary.match(part, ["/"]) do
              :nomatch -> :binary.part(name, pos, byte_size(name) - pos)
              _ -> <<>>
            end
        end
    end
  end

  def extension(name0) do
    name = flatten(name0)
    extension3([?/ | name], [])
  end

  defp extension3([?. | rest] = result, _result), do: extension3(rest, result)
  defp extension3([?/, ?. | rest], _result), do: extension3(rest, [])
  defp extension3([char | rest], []) when is_integer(char), do: extension3(rest, [])
  defp extension3([?/ | rest], _result), do: extension3(rest, [])
  defp extension3([char | rest], result) when is_integer(char), do: extension3(rest, result)
  defp extension3([], result), do: result

  def join([name1, name2 | rest]), do: join([join(name1, name2) | rest])
  def join([name]) when is_list(name), do: join1(name, [], [])
  def join([name]) when is_binary(name), do: join1b(name, <<>>, [])
  def join([name]) when is_atom(name), do: join([:erlang.atom_to_list(name)])

  def join(name1, name2) when is_list(name1) and is_list(name2) do
    case pathtype(name2) do
      :relative -> join1(name1, name2, [])
      _ -> join1(name2, [], [])
    end
  end

  def join(name1, name2) when is_binary(name1) and is_list(name2), do: join(name1, fstb(name2))
  def join(name1, name2) when is_list(name1) and is_binary(name2), do: join(fstb(name1), name2)

  def join(name1, name2) when is_binary(name1) and is_binary(name2) do
    case pathtype(name2) do
      :relative -> join1b(name1, name2, [])
      _ -> join1b(name2, <<>>, [])
    end
  end

  def join(name1, name2) when is_atom(name1), do: join(:erlang.atom_to_list(name1), name2)
  def join(name1, name2) when is_atom(name2), do: join(name1, :erlang.atom_to_list(name2))

  defp join1([?/ | rest], rel, [?., ?/ | result]), do: join1(rest, rel, [?/ | result])
  defp join1([?/ | rest], rel, [?/ | result]), do: join1(rest, rel, [?/ | result])
  defp join1([], [], result), do: maybe_remove_dirsep(result)
  defp join1([], rel, [?/ | result]), do: join1(rel, [], [?/ | result])
  defp join1([], rel, [?., ?/ | result]), do: join1(rel, [], [?/ | result])
  defp join1([], rel, result), do: join1(rel, [], [?/ | result])
  defp join1([[_ | _] = list | rest], rel, result), do: join1(list ++ rest, rel, result)
  defp join1([[] | rest], rel, result), do: join1(rest, rel, result)
  defp join1([char | rest], rel, result) when is_integer(char), do: join1(rest, rel, [char | result])

  defp join1([atom | rest], rel, result) when is_atom(atom),
    do: join1(:erlang.atom_to_list(atom) ++ rest, rel, result)

  defp join1b(<<?/, rest::binary>>, rel, [?., ?/ | result]), do: join1b(rest, rel, [?/ | result])
  defp join1b(<<?/, rest::binary>>, rel, [?/ | result]), do: join1b(rest, rel, [?/ | result])
  defp join1b(<<>>, <<>>, result), do: :erlang.list_to_binary(maybe_remove_dirsep(result))
  defp join1b(<<>>, rel, [?/ | result]), do: join1b(rel, <<>>, [?/ | result])
  defp join1b(<<>>, rel, [?., ?/ | result]), do: join1b(rel, <<>>, [?/ | result])
  defp join1b(<<>>, rel, result), do: join1b(rel, <<>>, [?/ | result])
  defp join1b(<<char, rest::binary>>, rel, result), do: join1b(rest, rel, [char | result])

  defp maybe_remove_dirsep([?/]), do: [?/]
  defp maybe_remove_dirsep([?/ | name]), do: :lists.reverse(name)
  defp maybe_remove_dirsep(name), do: :lists.reverse(name)

  def append(dir, name) when is_binary(dir) and is_binary(name), do: <<dir::binary, ?/, name::binary>>
  def append(dir, name) when is_binary(dir), do: append(dir, fstb(name))
  def append(dir, name) when is_binary(name), do: append(fstb(dir), name)
  def append(dir, name), do: dir ++ [?/ | name]

  def pathtype(atom) when is_atom(atom), do: pathtype(:erlang.atom_to_list(atom))
  def pathtype(name) when is_list(name) or is_binary(name), do: unix_pathtype(name)

  defp unix_pathtype(<<?/, _::binary>>), do: :absolute
  defp unix_pathtype([?/ | _]), do: :absolute
  defp unix_pathtype([list | rest]) when is_list(list), do: unix_pathtype(list ++ rest)
  defp unix_pathtype([atom | rest]) when is_atom(atom), do: unix_pathtype(:erlang.atom_to_list(atom) ++ rest)
  defp unix_pathtype(_), do: :relative

  def rootname(name) when is_binary(name),
    do: :erlang.list_to_binary(rootname(:binary.bin_to_list(name)))

  def rootname(name0) do
    name = flatten(name0)
    rootname4(name, [], [])
  end

  defp rootname4([?/ | rest], root, ext), do: rootname4(rest, [?/] ++ ext ++ root, [])
  defp rootname4([?. | rest], [?/ | _] = root, []), do: rootname4(rest, [?. | root], [])
  defp rootname4([?. | rest], root, ext), do: rootname4(rest, ext ++ root, ~c".")
  defp rootname4([char | rest], root, []) when is_integer(char), do: rootname4(rest, [char | root], [])
  defp rootname4([char | rest], root, ext) when is_integer(char), do: rootname4(rest, root, [char | ext])
  defp rootname4([], root, _ext), do: :lists.reverse(root)

  def rootname(name, ext) when is_binary(name) and is_binary(ext),
    do: :erlang.list_to_binary(rootname(:binary.bin_to_list(name), :binary.bin_to_list(ext)))

  def rootname(name, ext) when is_binary(name), do: rootname(name, fstb(ext))
  def rootname(name, ext) when is_binary(ext), do: rootname(fstb(name), ext)

  def rootname(name0, ext0) do
    name = flatten(name0)
    ext = flatten(ext0)
    rootname2(name, ext, [])
  end

  defp rootname2(ext, ext, [?/ | _] = result), do: :lists.reverse(result, ext)
  defp rootname2(ext, ext, result), do: :lists.reverse(result)
  defp rootname2([], _ext, result), do: :lists.reverse(result)
  defp rootname2([char | rest], ext, result) when is_integer(char), do: rootname2(rest, ext, [char | result])

  def split(name) when is_binary(name) do
    l = :binary.split(name, ["/"], [:global])

    ll =
      case l do
        [<<>> | rest] when rest != [] -> ["/" | rest]
        _ -> l
      end

    for x <- ll, x != <<>>, do: x
  end

  def split(name0) do
    name = flatten(name0)
    split3(name, [])
  end

  defp split3([?/ | rest], components), do: split4(rest, [], [[?/] | components])
  defp split3(rel, components), do: split4(rel, [], components)

  defp split4([?/ | rest], [], components), do: split4(rest, [], components)
  defp split4([?/ | rest], comp, components), do: split4(rest, [], [:lists.reverse(comp) | components])
  defp split4([char | rest], comp, components) when is_integer(char), do: split4(rest, [char | comp], components)
  defp split4([], [], components), do: :lists.reverse(components)
  defp split4([], comp, components), do: split4([], [], [:lists.reverse(comp) | components])

  def nativename(name0), do: join([name0])

  def flatten(bin) when is_binary(bin), do: bin
  def flatten(list), do: do_flatten(list, [])

  defp do_flatten([h | t], tail) when is_list(h), do: do_flatten(h, do_flatten(t, tail))
  defp do_flatten([h | t], tail) when is_atom(h), do: :erlang.atom_to_list(h) ++ do_flatten(t, tail)
  defp do_flatten([h | t], tail), do: [h | do_flatten(t, tail)]
  defp do_flatten([], tail), do: tail
  defp do_flatten(atom, tail) when is_atom(atom), do: :erlang.atom_to_list(atom) ++ flatten(tail)

  defp fstb(list) do
    case flatten(list) do
      l when is_list(l) ->
        try do
          :unicode.characters_to_binary(l)
        rescue
          _ -> :erlang.error(:badarg)
        end

      b ->
        b
    end
  end

  def basedir(type, application) when is_atom(type) and (is_list(application) or is_binary(application)),
    do: basedir(type, application, %{})

  def basedir(type, application, opts) when is_atom(type) and is_map(opts) do
    os = basedir_os_from_opts(opts)
    name = basedir_name_from_opts(os, application, opts)
    base = basedir_from_os(type, os)

    case {type, os} do
      {:user_log, :linux} -> join([base, name, ~c"log"])
      {:user_log, :windows} -> join([base, name, ~c"Logs"])
      {:user_cache, :windows} -> join([base, name, ~c"Cache"])
      {t, _} when t in [:site_config, :site_data] -> for b <- base, do: join([b, name])
      _ -> join([base, name])
    end
  end

  defp basedir_os_from_opts(%{os: os}) when os in [:linux, :windows, :darwin], do: os

  defp basedir_os_from_opts(_) do
    case :os.type() do
      {:unix, :darwin} -> :darwin
      {:win32, _} -> :windows
      _ -> :linux
    end
  end

  defp basedir_name_from_opts(:windows, app, %{author: author, version: vsn}), do: join([author, app, vsn])
  defp basedir_name_from_opts(:windows, app, %{author: author}), do: join([author, app])
  defp basedir_name_from_opts(_, app, %{version: vsn}), do: join([app, vsn])
  defp basedir_name_from_opts(_, app, _), do: app

  defp basedir_from_os(type, :linux) do
    case type do
      :user_data -> bgetenv(~c"XDG_DATA_HOME", ~c".local/share", true)
      :user_config -> bgetenv(~c"XDG_CONFIG_HOME", ~c".config", true)
      :user_cache -> bgetenv(~c"XDG_CACHE_HOME", ~c".cache", true)
      :user_log -> bgetenv(~c"XDG_CACHE_HOME", ~c".cache", true)
      :site_data -> lexemes(bgetenv(~c"XDG_DATA_DIRS", ~c"/usr/local/share/:/usr/share/", false))
      :site_config -> lexemes(bgetenv(~c"XDG_CONFIG_DIRS", ~c"/etc/xdg", false))
    end
  end

  defp basedir_from_os(type, :darwin) do
    case type do
      :user_data -> join_home(~c"Library/Application Support")
      :user_config -> join_home(~c"Library/Application Support")
      :user_cache -> join_home(~c"Library/Caches")
      :user_log -> join_home(~c"Library/Logs")
      :site_data -> [~c"/Library/Application Support"]
      :site_config -> [~c"/Library/Application Support"]
    end
  end

  defp basedir_from_os(type, :windows) do
    case :os.getenv(~c"APPDATA") do
      invalid when invalid in [false, []] ->
        case type do
          :user_data -> join_home(~c"Local")
          :user_config -> join_home(~c"Roaming")
          :user_cache -> join_home(~c"Local")
          :user_log -> join_home(~c"Local")
          _ -> []
        end

      app_data ->
        case type do
          :user_config -> app_data
          t when t in [:site_data, :site_config] -> []
          _ -> bgetenv(~c"LOCALAPPDATA", app_data)
        end
    end
  end

  defp lexemes(str), do: str |> List.to_string() |> String.split(":", trim: true) |> Enum.map(&String.to_charlist/1)

  defp bgetenv(k, default, false), do: bgetenv(k, default)
  defp bgetenv(k, default, true), do: bgetenv(k, join_home(default))

  defp bgetenv(k, default) do
    case :os.getenv(k) do
      [] -> default
      false -> default
      val -> val
    end
  end

  defp join_home(dir) do
    case :os.getenv(~c"HOME") do
      false -> join(~c"/", dir)
      home -> join(home, dir)
    end
  end

  def validate(file_name) when is_binary(file_name) do
    byte_size(file_name) > 0 and :binary.match(file_name, <<0>>) == :nomatch
  end

  def validate(file_name) when is_list(file_name) or is_atom(file_name) do
    try do
      validate_list(file_name, 0) > 0
    catch
      _, _ -> false
    end
  end

  defp validate_list([], chars), do: chars

  defp validate_list(c, chars) when is_integer(c) do
    if c < 1 or c >= 0x110000 or (c >= 0xD800 and c <= 0xDFFF), do: throw(:invalid)
    chars + 1
  end

  defp validate_list(a, chars) when is_atom(a), do: validate_list(:erlang.atom_to_list(a), chars)
  defp validate_list([h | t], chars), do: validate_list(t, validate_list(h, chars))
end

defmodule :filelib do
  # Port of OTP 27 filelib.erl (wildcards, is_dir/is_file, ensure_dir, safe_relative_path, ...).

  def wildcard(pattern) when is_list(pattern), do: do_wildcard(pattern, ~c".", :file)
  def wildcard(pattern, cwd) when is_list(pattern) and is_list(cwd), do: do_wildcard(pattern, cwd, :file)
  def wildcard(pattern, mod) when is_list(pattern) and is_atom(mod), do: do_wildcard(pattern, ~c".", mod)

  def wildcard(pattern, cwd, mod) when is_list(pattern) and is_list(cwd) and is_atom(mod),
    do: do_wildcard(pattern, cwd, mod)

  def is_dir(dir), do: do_is_dir(dir, :file)
  def is_dir(dir, mod) when is_atom(mod), do: do_is_dir(dir, mod)
  def is_file(file), do: do_is_file(file, :file)
  def is_file(file, mod) when is_atom(mod), do: do_is_file(file, mod)
  def is_regular(file), do: do_is_regular(file, :file)
  def is_regular(file, mod) when is_atom(mod), do: do_is_regular(file, mod)

  def fold_files(dir, regexp, recursive, fun, acc), do: do_fold_files(dir, regexp, recursive, fun, acc, :file)

  def fold_files(dir, regexp, recursive, fun, acc, mod) when is_atom(mod),
    do: do_fold_files(dir, regexp, recursive, fun, acc, mod)

  def last_modified(file), do: do_last_modified(file, :file)
  def last_modified(file, mod) when is_atom(mod), do: do_last_modified(file, mod)
  def file_size(file), do: do_file_size(file, :file)
  def file_size(file, mod) when is_atom(mod), do: do_file_size(file, mod)

  defp do_is_dir(dir, mod) do
    case eval_read_file_info(dir, mod) do
      {:ok, info} when elem(info, 2) == :directory -> true
      _ -> false
    end
  end

  defp do_is_file(file, mod) do
    case eval_read_file_info(file, mod) do
      {:ok, info} when elem(info, 2) in [:regular, :directory] -> true
      _ -> false
    end
  end

  defp do_is_regular(file, mod) do
    case eval_read_file_info(file, mod) do
      {:ok, info} when elem(info, 2) == :regular -> true
      _ -> false
    end
  end

  defp do_fold_files(dir, regexp, recursive, fun, acc, mod) do
    re = Regex.compile!(IO.chardata_to_string(regexp))
    do_fold_files1(dir, re, recursive, fun, acc, mod)
  end

  defp do_fold_files1(dir, re, recursive, fun, acc, mod) do
    case eval_list_dir(dir, mod) do
      {:ok, files} -> do_fold_files2(files, dir, re, recursive, fun, acc, mod)
      {:error, _} -> acc
    end
  end

  defp do_fold_files2([], _dir, _re, _recursive, _fun, acc, _mod), do: acc

  defp do_fold_files2([file | t], dir, re, recursive, fun, acc0, mod) do
    full_name = :filename.join(dir, file)

    if do_is_regular(full_name, mod) do
      acc =
        if Regex.match?(re, IO.chardata_to_string(file)), do: fun.(full_name, acc0), else: acc0

      do_fold_files2(t, dir, re, recursive, fun, acc, mod)
    else
      if recursive and do_is_dir(full_name, mod) do
        acc1 = do_fold_files1(full_name, re, recursive, fun, acc0, mod)
        do_fold_files2(t, dir, re, recursive, fun, acc1, mod)
      else
        do_fold_files2(t, dir, re, recursive, fun, acc0, mod)
      end
    end
  end

  defp do_last_modified(file, mod) do
    case eval_read_file_info(file, mod) do
      {:ok, info} -> elem(info, 5)
      _ -> 0
    end
  end

  defp do_file_size(file, mod) do
    case eval_read_file_info(file, mod) do
      {:ok, info} -> elem(info, 1)
      _ -> 0
    end
  end

  def ensure_dir(~c"/"), do: :ok
  def ensure_dir("/"), do: :ok
  def ensure_dir(f), do: ensure_path(:filename.dirname(f))

  def ensure_path(~c"/"), do: :ok
  def ensure_path("/"), do: :ok

  def ensure_path(path) do
    if do_is_dir(path, :file) do
      :ok
    else
      case :filename.dirname(path) do
        ^path ->
          {:error, :einval}

        parent ->
          _ = ensure_path(parent)

          case :file.make_dir(path) do
            {:error, :eexist} = eexist -> if do_is_dir(path, :file), do: :ok, else: eexist
            other -> other
          end
      end
    end
  end

  defp do_wildcard(pattern, cwd, mod) do
    {compiled, prefix_len} = compile_wildcard(pattern, cwd)
    files0 = do_wildcard_1(compiled, mod)
    files = if prefix_len == 0, do: files0, else: for(file <- files0, do: Enum.drop(file, prefix_len))
    :lists.sort(files)
  end

  defp do_wildcard_1({:exists, file}, mod) do
    if exists(file, mod), do: [file], else: []
  end

  defp do_wildcard_1([base | rest], mod), do: do_wildcard_2([base], rest, [], mod)

  defp do_wildcard_2([file | rest], pattern, result, mod),
    do: do_wildcard_2(rest, pattern, do_wildcard_3(file, pattern, result, mod), mod)

  defp do_wildcard_2([], _, result, _mod), do: result

  defp do_wildcard_3(base, [[:double_star] | rest], result, mod),
    do: do_double_star(~c".", [base], rest, result, mod, true)

  defp do_wildcard_3(base, [~c".." | rest], result, mod) do
    if do_is_dir(base, mod) do
      do_wildcard_2([:filename.join(base, ~c"..")], rest, result, mod)
    else
      result
    end
  end

  defp do_wildcard_3(base0, [pattern | rest], result, mod) do
    case eval_list_dir(base0, mod) do
      {:ok, files} ->
        base = prepare_base(base0)
        matches = do_wildcard_4(pattern, base, files)
        do_wildcard_2(matches, rest, result, mod)

      _ ->
        result
    end
  end

  defp do_wildcard_3(base, [], result, _mod), do: [base | result]

  defp do_wildcard_4(pattern, base, files) do
    if will_always_match(pattern) do
      for f <- files, do: base ++ f
    else
      for f <- files, match_part(pattern, f), do: base ++ f
    end
  end

  defp match_part([:question | rest1], [_ | rest2]), do: match_part(rest1, rest2)
  defp match_part([:accept], _), do: true
  defp match_part([:double_star], _), do: true
  defp match_part([:star | rest], file), do: do_star(rest, file)
  defp match_part([{:one_of, set} | rest], [c | file]), do: MapSet.member?(set, c) and match_part(rest, file)
  defp match_part([{:alt, alts}], file), do: do_alt(alts, file)
  defp match_part([c | rest1], [c | rest2]) when is_integer(c), do: match_part(rest1, rest2)
  defp match_part([x | _], [y | _]) when is_integer(x) and is_integer(y), do: false
  defp match_part([], []), do: true
  defp match_part([], [_ | _]), do: false
  defp match_part([_ | _], []), do: false
  defp match_part(_, _), do: false

  defp will_always_match([:accept]), do: true
  defp will_always_match([:double_star]), do: true
  defp will_always_match(_), do: false

  defp prepare_base(base0) do
    base1 = :filename.join(base0, ~c"x")
    [?x | base2] = :lists.reverse(base1)
    :lists.reverse(base2)
  end

  defp do_double_star(base, [h | t], patterns, result0, mod, root) do
    full = if root, do: h, else: :filename.join(base, h)

    result1 =
      case eval_list_dir(full, mod) do
        {:ok, files} -> do_double_star(full, files, patterns, result0, mod, false)
        _ -> result0
      end

    result2 =
      cond do
        root ->
          result1

        patterns == [] ->
          [full | result1]

        true ->
          [pattern | rest] = patterns
          if match_part(pattern, h), do: do_wildcard_2([full], rest, result1, mod), else: result1
      end

    do_double_star(base, t, patterns, result2, mod, root)
  end

  defp do_double_star(_base, [], _patterns, result, _mod, _root), do: result

  defp do_star(pattern, [_ | rest] = file), do: match_part(pattern, file) or do_star(pattern, rest)
  defp do_star(pattern, []), do: match_part(pattern, [])

  defp do_alt([alt | rest], file), do: match_part(alt, file) or do_alt(rest, file)
  defp do_alt([], _file), do: false

  def compile_wildcard(pattern) when is_list(pattern), do: {:compiled_wildcard, compile_wildcard(pattern, ~c".")}

  defp compile_wildcard(pattern0, cwd0) do
    pattern = convert_escapes(pattern0)
    [root | rest] = :filename.split(pattern)

    case :filename.pathtype(root) do
      :relative ->
        cwd = prepare_base(cwd0)
        compile_wildcard_2([root | rest], {:cwd, cwd})

      _ ->
        compile_wildcard_2(rest, {:root, 0, root})
    end
  end

  defp compile_wildcard_2([part | rest], root) do
    pattern = compile_part(part)

    if is_literal_pattern(pattern) do
      compile_wildcard_2(rest, compile_join(root, pattern))
    else
      compile_wildcard_3(rest, [pattern, root])
    end
  end

  defp compile_wildcard_2([], {:root, prefix_len, root}), do: {{:exists, root}, prefix_len}

  defp is_literal_pattern([h | t]), do: is_integer(h) and is_literal_pattern(t)
  defp is_literal_pattern([]), do: true

  defp compile_wildcard_3([part | rest], result), do: compile_wildcard_3(rest, [compile_part(part) | result])

  defp compile_wildcard_3([], result) do
    case :lists.reverse(result) do
      [{:root, prefix_len, root} | compiled] -> {[root | compiled], prefix_len}
      [{:cwd, root} | compiled] -> {[root | compiled], length(:filename.join(root, ~c"x")) - 1}
    end
  end

  defp compile_join({:cwd, ~c"."}, file), do: {:root, 0, file}

  defp compile_join({:cwd, cwd}, file0) do
    file = :filename.join([file0])
    root = :filename.join(cwd, file)
    {:root, length(root) - length(file), root}
  end

  defp compile_join({:root, prefix_len, root}, file), do: {:root, prefix_len, :filename.join(root, file)}

  defp compile_part(part0), do: compile_part(wrap_escapes(part0), false, [])

  defp compile_part_to_sep(part), do: compile_part(part, true, [])

  defp compile_part([], true, _), do: :erlang.error({:badpattern, :missing_delimiter})
  defp compile_part([?, | rest], true, result), do: {:ok, ?,, :lists.reverse(result), rest}
  defp compile_part([?} | rest], true, result), do: {:ok, ?}, :lists.reverse(result), rest}
  defp compile_part([?? | rest], upto, result), do: compile_part(rest, upto, [:question | result])
  defp compile_part([?*, ?*], upto, result), do: compile_part([], upto, [:double_star | result])
  defp compile_part([?*, ?* | rest], upto, result), do: compile_part(rest, upto, [:star | result])
  defp compile_part([?*], upto, result), do: compile_part([], upto, [:accept | result])
  defp compile_part([?* | rest], upto, result), do: compile_part(rest, upto, [:star | result])

  defp compile_part([?[ | rest], upto, result) do
    case compile_charset(rest, MapSet.new()) do
      {:ok, charset, rest1} -> compile_part(rest1, upto, [charset | result])
      :error -> compile_part(rest, upto, [?[ | result])
    end
  end

  defp compile_part([?{ | rest], upto, result) do
    case compile_alt(rest) do
      {:ok, alt} -> :lists.reverse(result, [alt])
      :error -> compile_part(rest, upto, [?{ | result])
    end
  end

  defp compile_part([{:escaped, x} | rest], upto, result), do: compile_part(rest, upto, [x | result])
  defp compile_part([x | rest], upto, result), do: compile_part(rest, upto, [x | result])
  defp compile_part([], _upto, result), do: :lists.reverse(result)

  defp compile_charset([?] | rest], set), do: compile_charset1(rest, MapSet.put(set, ?]))
  defp compile_charset([], _set), do: :error
  defp compile_charset(list, set), do: compile_charset1(list, set)

  defp compile_charset1([lower, ?-, upper | rest], set) when is_integer(lower) and is_integer(upper) and lower <= upper,
    do: compile_charset1(rest, Enum.reduce(lower..upper, set, &MapSet.put(&2, &1)))

  defp compile_charset1([?] | rest], set), do: {:ok, {:one_of, set}, rest}
  defp compile_charset1([{:escaped, x} | rest], set), do: compile_charset1(rest, MapSet.put(set, x))
  defp compile_charset1([x | rest], set), do: compile_charset1(rest, MapSet.put(set, x))
  defp compile_charset1([], _set), do: :error

  defp compile_alt(pattern), do: compile_alt(pattern, [])

  defp compile_alt(pattern, result) do
    case compile_part_to_sep(pattern) do
      {:ok, ?,, alt_pattern, rest} ->
        compile_alt(rest, [alt_pattern | result])

      {:ok, ?}, alt_pattern, rest} ->
        new_result = [alt_pattern | result]
        rest_pattern = compile_part(rest)
        {:ok, {:alt, for(alt <- new_result, do: alt ++ rest_pattern)}}

      _ ->
        :error
    end
  end

  defp convert_escapes([?@ | t]), do: [?@, ?@ | convert_escapes(t)]
  defp convert_escapes([?\\ | t]), do: [?@, ?e | convert_escapes(t)]
  defp convert_escapes([h | t]), do: [h | convert_escapes(t)]
  defp convert_escapes([]), do: []

  defp wrap_escapes([?@, ?@ | t]), do: [?@ | wrap_escapes(t)]
  defp wrap_escapes([?@, ?e, c | t]), do: [{:escaped, c} | wrap_escapes(t)]
  defp wrap_escapes([?@, ?e]), do: []
  defp wrap_escapes([h | t]), do: [h | wrap_escapes(t)]
  defp wrap_escapes([]), do: []

  defp exists(file, mod) do
    case eval_read_link_info(file, mod) do
      {:error, _} -> false
      {:ok, _} -> true
    end
  end

  defp eval_read_file_info(file, :file), do: :file.read_file_info(file)
  defp eval_read_file_info(file, mod), do: mod.read_file_info(file)
  defp eval_read_link_info(file, :file), do: :file.read_link_info(file)
  defp eval_read_link_info(file, mod), do: mod.read_link_info(file)
  defp eval_list_dir(dir, :file), do: :file.list_dir(dir)
  defp eval_list_dir(dir, mod), do: mod.list_dir(dir)

  def safe_relative_path(path, ""), do: safe_relative_path(path, ".")
  def safe_relative_path(path, []), do: safe_relative_path(path, ~c".")
  def safe_relative_path(path, cwd), do: srp_path(:filename.split(path), cwd, MapSet.new(), [])

  defp srp_path([], _cwd, _seen, []), do: ""
  defp srp_path([], _cwd, _seen, acc), do: :filename.join(acc)
  defp srp_path([seg | segs], cwd, seen, acc) when seg in [~c".", "."], do: srp_path(segs, cwd, seen, acc)
  defp srp_path([seg | _segs], _cwd, _seen, []) when seg in [~c"..", ".."], do: :unsafe

  defp srp_path([seg | segs], cwd, seen, [_ | _] = acc) when seg in [~c"..", ".."],
    do: srp_path(segs, cwd, seen, Enum.drop(acc, -1))

  defp srp_path([:clear | segs], cwd, _seen, acc), do: srp_path(segs, cwd, MapSet.new(), acc)

  defp srp_path([seg | _] = segs, cwd, seen, acc) do
    case :filename.pathtype(seg) do
      :relative -> srp_segment(segs, cwd, seen, acc)
      _ -> :unsafe
    end
  end

  defp srp_segment([seg | segs], cwd, seen, acc) do
    path = :filename.join([cwd | acc])

    case :file.read_link(:filename.join(path, seg)) do
      {:ok, link_path} -> srp_link(path, link_path, segs, cwd, seen, acc)
      {:error, _} -> srp_path(segs, cwd, seen, acc ++ [seg])
    end
  end

  defp srp_link(path, link_path, segs, cwd, seen, acc) do
    full_link_path = :filename.join(path, link_path)

    if MapSet.member?(seen, full_link_path) do
      :unsafe
    else
      srp_path(:filename.split(link_path) ++ [:clear | segs], cwd, MapSet.put(seen, full_link_path), acc)
    end
  end
end

defmodule Tonic.FileIO do
  # Open (non-raw) files are owned by a small io-server process speaking the
  # Erlang I/O protocol ({:io_request, ...}) plus {:file_request, ...}.

  def start(id, owner, binary, enc) do
    spawn(fn ->
      ref = Process.monitor(owner)
      loop(%{id: id, binary: binary, enc: enc, owner: ref})
    end)
  end

  defp loop(st) do
    receive do
      {:io_request, from, rref, req} ->
        {reply, st} = io_req(req, st)
        send(from, {:io_reply, rref, reply})
        loop(st)

      {:file_request, from, rref, :close} ->
        send(from, {:file_reply, rref, :tonic.fio_close(st.id)})

      {:file_request, from, rref, req} ->
        send(from, {:file_reply, rref, file_req(req, st)})
        loop(st)

      {:DOWN, ref, _, _, _} when ref == st.owner ->
        :tonic.fio_close(st.id)

      _ ->
        loop(st)
    end
  end

  # ---- client side

  # The registered :standard_error device (started at boot).
  def standard_error_loop do
    receive do
      {:io_request, from, ref, req} ->
        send(from, {:io_reply, ref, stderr_request(req)})
        standard_error_loop()

      _ ->
        standard_error_loop()
    end
  end

  defp stderr_request({:put_chars, _enc, chars}) do
    :tonic.io_write(:stderr, chars)
    :ok
  end

  defp stderr_request({:put_chars, chars}), do: stderr_request({:put_chars, :latin1, chars})

  defp stderr_request({:put_chars, enc, m, f, a}) do
    stderr_request({:put_chars, enc, apply(m, f, a)})
  rescue
    _ -> {:error, :put_chars}
  end

  defp stderr_request({:requests, reqs}) do
    Enum.reduce_while(reqs, :ok, fn r, _ ->
      case stderr_request(r) do
        :ok -> {:cont, :ok}
        err -> {:halt, err}
      end
    end)
  end

  defp stderr_request({:setopts, _}), do: :ok
  defp stderr_request(:getopts), do: {:ok, [binary: true, encoding: :unicode]}
  defp stderr_request({:get_geometry, _}), do: {:error, :enotsup}
  defp stderr_request(_), do: {:error, :request}

  def request(pid, req) when is_pid(pid) do
    ref = Process.monitor(pid)
    send(pid, {:io_request, self(), ref, req})

    receive do
      {:io_reply, ^ref, reply} ->
        Process.demonitor(ref, [:flush])
        reply

      {:DOWN, ^ref, _, _, _} ->
        {:error, :terminated}
    end
  end

  def request(name, req) when is_atom(name) do
    case Process.whereis(name) do
      nil -> {:error, :arguments}
      pid -> request(pid, req)
    end
  end

  def file_request(pid, req) when is_pid(pid) do
    ref = Process.monitor(pid)
    send(pid, {:file_request, self(), ref, req})

    receive do
      {:file_reply, ^ref, reply} ->
        Process.demonitor(ref, [:flush])
        reply

      {:DOWN, ^ref, _, _, _} ->
        {:error, :terminated}
    end
  end

  # ---- server side

  defp io_req({:put_chars, enc, chars}, st), do: {put_chars(enc, chars, st), st}

  defp io_req({:put_chars, enc, m, f, a}, st) do
    try do
      {put_chars(enc, apply(m, f, a), st), st}
    rescue
      _ -> {{:error, :put_chars}, st}
    end
  end

  defp io_req({:put_chars, chars}, st), do: {put_chars(:latin1, chars, st), st}
  defp io_req({:get_line, enc, _prompt}, %{buf: [_ | _] = buf} = st) do
    {line, rest} = Enum.split_while(buf, &(&1 != ?\n))

    {line, rest} =
      case rest do
        [?\n | r] -> {line ++ [?\n], r}
        [] -> {line, []}
      end

    {from_chars(line, enc, st), %{st | buf: rest}}
  end

  defp io_req({:get_line, enc, _prompt}, st), do: {get_line(enc, st), st}
  defp io_req({:get_line, prompt}, st), do: io_req({:get_line, :latin1, prompt}, st)

  defp io_req({:get_until, prompt, m, f, a}, st), do: io_req({:get_until, :latin1, prompt, m, f, a}, st)

  defp io_req({:get_until, enc, _prompt, m, f, a}, st), do: get_until(enc, m, f, a, [], st)
  defp io_req({:get_chars, enc, _prompt, n}, st), do: {get_chars(enc, n, st), st}
  defp io_req({:get_chars, _prompt, n}, st), do: {get_chars(:latin1, n, st), st}

  defp io_req({:setopts, opts}, st) do
    st =
      Enum.reduce(opts, st, fn
        :binary, st -> %{st | binary: true}
        :list, st -> %{st | binary: false}
        {:binary, b}, st -> %{st | binary: b}
        {:encoding, e}, st -> %{st | enc: norm_enc(e)}
        _, st -> st
      end)

    {:ok, st}
  end

  defp io_req(:getopts, st), do: {[binary: st.binary, encoding: st.enc], st}
  defp io_req({:getopts}, st), do: io_req(:getopts, st)

  defp io_req({:requests, reqs}, st) do
    Enum.reduce(reqs, {:ok, st}, fn req, {_, st} -> io_req(req, st) end)
  end

  defp io_req(_, st), do: {{:error, :request}, st}

  def norm_enc(e) when e in [:unicode, :utf8], do: :unicode
  def norm_enc(e), do: e

  defp put_chars(enc, chars, st) do
    data =
      try do
        to_device_bytes(enc, chars, st.enc)
      rescue
        _ -> {:error, :no_translation}
      end

    case data do
      {:error, _} = e -> e
      bin -> :tonic.fio_write(st.id, bin)
    end
  end

  defp to_device_bytes(:latin1, chars, :latin1), do: :erlang.iolist_to_binary(chars)
  defp to_device_bytes(:latin1, chars, _), do: :tonic.fio_recode(:erlang.iolist_to_binary(chars), true)
  defp to_device_bytes(_, chars, :latin1), do: chars |> :unicode.characters_to_binary() |> latin1_or_error()
  defp to_device_bytes(_, chars, _), do: :unicode.characters_to_binary(chars)

  defp latin1_or_error(bin) do
    case :tonic.fio_recode(bin, false) do
      nil -> {:error, :no_translation}
      b -> b
    end
  end

  # get_until: feed lines to m:f(cont, chars, a...) until it is done;
  # leftover characters stay buffered for later reads.
  defp get_until(enc, m, f, a, cont, st) do
    {chars, st} =
      case Map.get(st, :buf, []) do
        [_ | _] = buf ->
          {buf, Map.put(st, :buf, [])}

        _ ->
          case :tonic.fio_read_line(st.id, true) do
            {:ok, bytes} -> {:unicode.characters_to_list(bytes, if(st.enc == :latin1, do: :latin1, else: :unicode)), st}
            :eof -> {:eof, st}
            other -> {other, st}
          end
      end

    case chars do
      {:error, _} = e ->
        {e, st}

      _ ->
        case apply(m, f, [cont, chars | a]) do
          {:done, result, rest} ->
            rest = if rest == :eof, do: [], else: rest
            result = if is_list(result) and st.binary and norm_enc(enc) != :latin1 and m != :erl_scan, do: List.to_string(result), else: result
            {result, Map.put(st, :buf, rest)}

          {:more, cont} ->
            if chars == :eof, do: {:eof, st}, else: get_until(enc, m, f, a, cont, st)
        end
    end
  end

  defp from_chars(chars, enc, st) do
    if st.binary, do: :unicode.characters_to_binary(chars, :unicode, norm_enc(enc)), else: chars
  end

  defp get_line(enc, st) do
    case :tonic.fio_read_line(st.id, true) do
      {:ok, bytes} -> convert_in(bytes, enc, st)
      other -> other
    end
  end

  defp get_chars(_enc, 0, st), do: if(st.binary, do: "", else: [])

  defp get_chars(enc, n, st) do
    case :tonic.fio_read(st.id, n, st.enc != :latin1) do
      {:ok, bytes} -> convert_in(bytes, enc, st)
      other -> other
    end
  end

  defp convert_in(bytes, enc, st) do
    data =
      case {st.enc, norm_enc(enc)} do
        {:latin1, :latin1} -> bytes
        {:latin1, _} -> :tonic.fio_recode(bytes, true)
        {_, :latin1} -> :tonic.fio_recode(bytes, false)
        {_, _} -> bytes
      end

    cond do
      data == nil -> {:error, {:no_translation, :unicode, :latin1}}
      st.binary -> data
      norm_enc(enc) == :latin1 -> :binary.bin_to_list(data)
      true -> :tonic.str_to_charlist(data)
    end
  end

  defp file_req({:position, at}, st) do
    case :file.__position_args__(at) do
      {w, o} -> :tonic.fio_position(st.id, w, o)
      err -> err
    end
  end

  defp file_req({:pread, pos, n}, st) do
    case :tonic.fio_pread(st.id, pos, n) do
      {:ok, b} -> {:ok, if(st.binary, do: b, else: :binary.bin_to_list(b))}
      other -> other
    end
  end

  defp file_req({:pread, list}, st) do
    Enum.reduce_while(list, {:ok, []}, fn {pos, n}, {:ok, acc} ->
      case file_req({:pread, pos, n}, st) do
        {:ok, d} -> {:cont, {:ok, [d | acc]}}
        :eof -> {:cont, {:ok, [:eof | acc]}}
        err -> {:halt, err}
      end
    end)
    |> case do
      {:ok, acc} -> {:ok, :lists.reverse(acc)}
      err -> err
    end
  end

  defp file_req({:pwrite, pos, data}, st), do: :tonic.fio_pwrite(st.id, pos, data)

  defp file_req({:pwrite, list}, st) do
    Enum.reduce_while(list, :ok, fn {pos, data}, :ok ->
      case :tonic.fio_pwrite(st.id, pos, data) do
        :ok -> {:cont, :ok}
        err -> {:halt, err}
      end
    end)
  end

  defp file_req(:sync, st), do: :tonic.fio_ctl(st.id, 0)
  defp file_req(:datasync, st), do: :tonic.fio_ctl(st.id, 1)
  defp file_req(:truncate, st), do: :tonic.fio_ctl(st.id, 2)
  defp file_req(_, _st), do: {:error, :enotsup}
end

defmodule :file do
  # Raw files are {:file_descriptor, :prim_file, {handle, binary?}}; other open
  # files are io-server pids (see Tonic.FileIO).

  def native_name_encoding, do: :utf8

  def format_error({line, mod, reason}),
    do: :tonic.str_to_charlist("#{line}: " <> IO.chardata_to_string(mod.format_error(reason)))

  def format_error(:badarg), do: ~c"bad argument"
  def format_error(:system_limit), do: ~c"a system limit was hit, probably not enough ports"
  def format_error(:terminated), do: ~c"the file server process is terminated"

  def format_error(reason) do
    case :tonic.fio_posix_msg(reason) do
      nil -> :tonic.str_to_charlist(inspect(reason))
      msg -> :tonic.str_to_charlist(msg)
    end
  end

  # ---- name handling

  defp name(n) when is_binary(n), do: n
  defp name(n) when is_atom(n), do: Atom.to_string(n)

  defp name(n) when is_list(n) do
    case :filename.flatten(n) do
      l when is_list(l) -> :unicode.characters_to_binary(l)
      b -> b
    end
  end

  defp with_name(n, fun) do
    case valid_name(n) do
      nil -> {:error, :badarg}
      b -> fun.(b)
    end
  end

  defp valid_name(n) do
    try do
      b = name(n)
      if is_binary(b), do: b, else: nil
    rescue
      _ -> nil
    end
  end

  # ---- file server functions

  def get_cwd, do: :tonic.fio_get_cwd()

  def get_cwd([drive, ?:]) when is_integer(drive), do: {:error, :enotsup}
  def get_cwd(_), do: {:error, :badarg}

  def set_cwd(dir), do: with_name(dir, &:tonic.fio_op(6, &1, nil))

  def read_file(file), do: with_name(file, &:tonic.fio_read_file(&1))

  def write_file(file, bytes), do: write_file(file, bytes, [])

  def write_file(file, bytes, modes) when is_list(modes) do
    case parse_modes(modes) do
      {:error, _} = e -> e
      m -> with_name(file, &:tonic.fio_write_file(&1, bytes, m.flags))
    end
  end

  def list_dir(dir), do: with_name(dir, &:tonic.fio_list_dir(&1))
  def list_dir_all(dir), do: list_dir(dir)

  def make_dir(dir), do: with_name(dir, &:tonic.fio_op(0, &1, nil))
  def del_dir(dir), do: with_name(dir, &:tonic.fio_op(1, &1, nil))
  def del_dir_r(file), do: with_name(file, &:tonic.fio_op(8, &1, nil))
  def delete(file), do: with_name(file, &:tonic.fio_op(2, &1, nil))
  def delete(file, _opts), do: delete(file)

  def rename(source, destination) do
    case {valid_name(source), valid_name(destination)} do
      {s, d} when is_binary(s) and is_binary(d) -> :tonic.fio_op(3, s, d)
      _ -> {:error, :badarg}
    end
  end

  def make_link(existing, new) do
    case {valid_name(existing), valid_name(new)} do
      {s, d} when is_binary(s) and is_binary(d) -> :tonic.fio_op(4, s, d)
      _ -> {:error, :badarg}
    end
  end

  def make_symlink(existing, new) do
    case {valid_name(existing), valid_name(new)} do
      {s, d} when is_binary(s) and is_binary(d) -> :tonic.fio_op(5, s, d)
      _ -> {:error, :badarg}
    end
  end

  def read_link(name), do: with_name(name, &:tonic.fio_read_link(&1))
  def read_link_all(name), do: read_link(name)

  def read_file_info(file), do: read_file_info(file, [])
  def read_file_info(file, opts), do: with_name(file, &:tonic.fio_info(&1, true, time_mode(opts)))
  def read_link_info(file), do: read_link_info(file, [])
  def read_link_info(file, opts), do: with_name(file, &:tonic.fio_info(&1, false, time_mode(opts)))

  def write_file_info(file, info), do: write_file_info(file, info, [])
  def write_file_info(file, info, opts), do: with_name(file, &:tonic.fio_write_info(&1, info, time_mode(opts)))

  def change_mode(file, mode), do: with_name(file, &:tonic.fio_op(7, &1, mode))
  def change_owner(file, uid), do: with_name(file, &:tonic.fio_op(9, &1, uid))

  def change_owner(file, uid, gid) do
    with :ok <- change_owner(file, uid), do: change_group(file, gid)
  end

  def change_group(file, gid), do: with_name(file, &:tonic.fio_op(10, &1, gid))

  def change_time(file, mtime), do: change_time(file, mtime, mtime)

  def change_time(file, atime, mtime) do
    mode = if is_integer(atime), do: 2, else: 0
    with_name(file, &:tonic.fio_set_times(&1, atime, mtime, mode))
  end

  defp time_mode(opts) do
    case :lists.keyfind(:time, 1, opts) do
      {:time, :universal} -> 1
      {:time, :posix} -> 2
      _ -> 0
    end
  end

  # ---- consult

  def consult(file) do
    case read_file(file) do
      {:ok, bin} -> Tonic.ErlTerms.parse_all(bin)
      err -> err
    end
  end

  # ---- open files

  def open(file, modes) when is_list(modes) do
    case parse_modes(modes) do
      {:error, _} = e ->
        e

      %{ram: true} = m ->
        # A RAM file: an unlinked temporary file holding the data.
        path = Path.join(System.tmp_dir!(), "tonic-ram-#{System.unique_integer([:positive])}")

        with :ok <- write_file(path, file),
             {:ok, id} <- :tonic.fio_open(path, m.flags) do
          _ = delete(path)
          {:ok, {:file_descriptor, :prim_file, {id, m.binary}}}
        else
          err ->
            _ = delete(path)
            err
        end

      m ->
        with_name(file, fn path ->
          case :tonic.fio_open(path, m.flags) do
            {:ok, id} ->
              if m.raw do
                {:ok, {:file_descriptor, :prim_file, {id, m.binary}}}
              else
                {:ok, Tonic.FileIO.start(id, self(), m.binary, m.enc)}
              end

            err ->
              err
          end
        end)
    end
  end

  def open(_file, _modes), do: {:error, :badarg}

  defp parse_modes(modes) do
    Enum.reduce(modes, %{flags: 0, raw: false, binary: false, enc: :latin1, ram: false}, fn
      _, {:error, _} = e -> e
      :read, m -> %{m | flags: Bitwise.bor(m.flags, 1)}
      :write, m -> %{m | flags: Bitwise.bor(m.flags, 2)}
      :append, m -> %{m | flags: Bitwise.bor(m.flags, 4)}
      :exclusive, m -> %{m | flags: Bitwise.bor(m.flags, 8)}
      :sync, m -> %{m | flags: Bitwise.bor(m.flags, 16)}
      :raw, m -> %{m | raw: true}
      :binary, m -> %{m | binary: true}
      :list, m -> %{m | binary: false}
      {:binary, b}, m -> %{m | binary: b}
      {:encoding, e}, m -> %{m | enc: Tonic.FileIO.norm_enc(e)}
      :utf8, m -> %{m | enc: :unicode}
      :read_ahead, m -> m
      {:read_ahead, _}, m -> m
      :delayed_write, m -> m
      {:delayed_write, _, _}, m -> m
      :compressed, m -> m
      :compressed_one, m -> m
      :ram, m -> %{m | ram: true, raw: true}
      :directory, m -> m
      _, _ -> {:error, :badarg}
    end)
  end

  def close({:file_descriptor, :prim_file, {id, _}}), do: :tonic.fio_close(id)
  def close(pid) when is_pid(pid), do: Tonic.FileIO.file_request(pid, :close)
  def close(_), do: {:error, :badarg}

  defp to_mode(bin, true), do: bin
  defp to_mode(bin, false), do: :binary.bin_to_list(bin)

  def read({:file_descriptor, :prim_file, {id, bin?}}, n) when is_integer(n) and n >= 0 do
    case :tonic.fio_read(id, n, false) do
      {:ok, b} -> {:ok, to_mode(b, bin?)}
      other -> other
    end
  end

  def read(dev, n) when (is_pid(dev) or is_atom(dev)) and is_integer(n) and n >= 0 do
    case io_get_chars(dev, :latin1, n) do
      data when is_list(data) or is_binary(data) -> {:ok, data}
      other -> other
    end
  end

  def read(_dev, _n), do: {:error, :badarg}

  def read_line({:file_descriptor, :prim_file, {id, bin?}}) do
    case :tonic.fio_read_line(id, true) do
      {:ok, b} -> {:ok, to_mode(b, bin?)}
      other -> other
    end
  end

  def read_line(dev) when is_pid(dev) or is_atom(dev) do
    case io_get_line(dev, :latin1) do
      data when is_list(data) or is_binary(data) -> {:ok, data}
      other -> other
    end
  end

  def read_line(_), do: {:error, :badarg}

  def write({:file_descriptor, :prim_file, {id, _}}, bytes) do
    try do
      :tonic.fio_write(id, bytes)
    rescue
      _ -> {:error, :badarg}
    end
  end

  def write(dev, bytes) when is_pid(dev) or is_atom(dev), do: io_put_chars(dev, :latin1, bytes)
  def write(_, _), do: {:error, :badarg}

  def position({:file_descriptor, :prim_file, {id, _}}, at) do
    case __position_args__(at) do
      {w, o} -> :tonic.fio_position(id, w, o)
      err -> err
    end
  end

  def position(pid, at) when is_pid(pid), do: Tonic.FileIO.file_request(pid, {:position, at})
  def position(_, _), do: {:error, :badarg}

  def __position_args__(at) do
    case at do
      n when is_integer(n) -> {0, n}
      {:bof, n} when is_integer(n) -> {0, n}
      {:cur, n} when is_integer(n) -> {1, n}
      {:eof, n} when is_integer(n) -> {2, n}
      :bof -> {0, 0}
      :cur -> {1, 0}
      :eof -> {2, 0}
      _ -> {:error, :einval}
    end
  end

  def pread({:file_descriptor, :prim_file, {id, bin?}}, pos, n) do
    case :tonic.fio_pread(id, pos, n) do
      {:ok, b} -> {:ok, to_mode(b, bin?)}
      other -> other
    end
  end

  def pread(pid, pos, n) when is_pid(pid), do: Tonic.FileIO.file_request(pid, {:pread, pos, n})

  def pread(dev, locnums) when is_list(locnums) do
    Enum.reduce_while(locnums, {:ok, []}, fn {pos, n}, {:ok, acc} ->
      case pread(dev, pos, n) do
        {:ok, d} -> {:cont, {:ok, [d | acc]}}
        :eof -> {:cont, {:ok, [:eof | acc]}}
        err -> {:halt, err}
      end
    end)
    |> case do
      {:ok, acc} -> {:ok, :lists.reverse(acc)}
      err -> err
    end
  end

  def pwrite({:file_descriptor, :prim_file, {id, _}}, pos, bytes), do: :tonic.fio_pwrite(id, pos, bytes)
  def pwrite(pid, pos, bytes) when is_pid(pid), do: Tonic.FileIO.file_request(pid, {:pwrite, pos, bytes})

  def pwrite(dev, locbytes) when is_list(locbytes) do
    Enum.reduce_while(locbytes, :ok, fn {pos, bytes}, :ok ->
      case pwrite(dev, pos, bytes) do
        :ok -> {:cont, :ok}
        err -> {:halt, err}
      end
    end)
  end

  def sync({:file_descriptor, :prim_file, {id, _}}), do: :tonic.fio_ctl(id, 0)
  def sync(pid) when is_pid(pid), do: Tonic.FileIO.file_request(pid, :sync)
  def datasync({:file_descriptor, :prim_file, {id, _}}), do: :tonic.fio_ctl(id, 1)
  def datasync(pid) when is_pid(pid), do: Tonic.FileIO.file_request(pid, :datasync)
  def truncate({:file_descriptor, :prim_file, {id, _}}), do: :tonic.fio_ctl(id, 2)
  def truncate(pid) when is_pid(pid), do: Tonic.FileIO.file_request(pid, :truncate)

  def advise(_dev, _offset, _length, _advise), do: :ok
  def allocate(_dev, _offset, _length), do: :ok

  # ---- copy

  def copy(source, destination), do: copy(source, destination, :infinity)

  def copy(source, destination, length) do
    case {copy_name(source), copy_name(destination)} do
      {{:name, s, _}, {:name, d, dmodes}} ->
        case parse_modes(dmodes) do
          {:error, _} = e -> e
          m -> :tonic.fio_copy(s, d, if(length == :infinity, do: -1, else: length), m.flags)
        end

      {src, dst} ->
        with {:ok, sdev, sclose} <- copy_open(src, [:read, :binary]),
             {:ok, ddev, dclose} <- copy_open(dst, [:write, :binary]) do
          result = copy_loop(sdev, ddev, length, 0)
          if sclose, do: close(sdev)
          if dclose, do: close(ddev)
          result
        end
    end
  end

  defp copy_name({n, modes}) when is_list(modes) and not is_integer(n) do
    case valid_name(n) do
      nil -> {:dev, {n, modes}}
      b -> {:name, b, modes}
    end
  end

  defp copy_name(dev) when is_pid(dev), do: {:dev, dev}
  defp copy_name({:file_descriptor, _, _} = dev), do: {:dev, dev}

  defp copy_name(n) do
    case valid_name(n) do
      nil -> {:dev, n}
      b -> {:name, b, []}
    end
  end

  defp copy_open({:dev, dev}, _), do: {:ok, dev, false}

  defp copy_open({:name, n, modes}, base) do
    case open(n, base ++ modes) do
      {:ok, dev} -> {:ok, dev, true}
      err -> err
    end
  end

  defp copy_loop(_s, _d, 0, acc), do: {:ok, acc}

  defp copy_loop(s, d, left, acc) do
    n = if left == :infinity, do: 65536, else: min(left, 65536)

    case read(s, n) do
      {:ok, data} ->
        case write(d, data) do
          :ok ->
            sz = IO.iodata_length(data)
            copy_loop(s, d, if(left == :infinity, do: :infinity, else: left - sz), acc + sz)

          err ->
            err
        end

      :eof ->
        {:ok, acc}

      err ->
        err
    end
  end

  # ---- io protocol helpers (also used by IO)

  # :standard_io goes to the process' group leader when one was set (e.g. by
  # ExUnit.CaptureIO); otherwise straight to stdout.
  def io_put_chars(:standard_io, enc, chars) do
    case :tonic.group_leader_of(self()) do
      nil -> :tonic.io_write(:stdio, chars)
      gl -> Tonic.FileIO.request(gl, {:put_chars, enc, chars})
    end
  end

  def io_put_chars(:user, _enc, chars), do: :tonic.io_write(:stdio, chars)

  def io_put_chars(:standard_error, enc, chars) do
    case :erlang.whereis(:standard_error) do
      :undefined -> :tonic.io_write(:stderr, chars)
      pid -> Tonic.FileIO.request(pid, {:put_chars, enc, chars})
    end
  end

  def io_put_chars(dev, enc, chars), do: Tonic.FileIO.request(dev, {:put_chars, enc, chars})

  def io_get_line(dev, enc), do: io_get_line(dev, enc, ~c"")

  def io_get_line(:standard_io, enc, prompt) do
    case :tonic.group_leader_of(self()) do
      nil -> :tonic.io_gets(prompt)
      gl -> Tonic.FileIO.request(gl, {:get_line, enc, prompt})
    end
  end

  def io_get_line(:user, _enc, prompt), do: :tonic.io_gets(prompt)
  def io_get_line(:standard_error, _enc, _prompt), do: {:error, :enotsup}
  def io_get_line(dev, enc, prompt), do: Tonic.FileIO.request(dev, {:get_line, enc, prompt})

  def io_get_chars(dev, enc, n), do: io_get_chars(dev, enc, ~c"", n)

  def io_get_chars(:standard_io, enc, prompt, n) do
    case :tonic.group_leader_of(self()) do
      nil -> io_get_chars(:user, enc, prompt, n)
      gl -> Tonic.FileIO.request(gl, {:get_chars, enc, prompt, n})
    end
  end

  def io_get_chars(:user, enc, prompt, n) do
    :tonic.io_write(:stdio, prompt)
    :tonic.fio_stdin_read(n, enc != :latin1)
  end

  def io_get_chars(:standard_error, _enc, _prompt, _n), do: {:error, :enotsup}
  def io_get_chars(dev, enc, prompt, n), do: Tonic.FileIO.request(dev, {:get_chars, enc, prompt, n})
end

defmodule Tonic.ErlTerms do
  # Minimal Erlang term reader for :file.consult/1 (terms terminated by ".").

  def parse_all(bin) do
    try do
      toks = tokens(bin, 1, [])
      {:ok, terms(toks, [])}
    catch
      {:erl_parse, line} -> {:error, {line, :erl_parse, ~c"syntax error"}}
    end
  end

  defp terms([], acc), do: :lists.reverse(acc)

  defp terms(toks, acc) do
    {t, rest} = expr(toks)

    case rest do
      [{:dot, _} | rest] -> terms(rest, [t | acc])
      [{_, l} | _] -> throw({:erl_parse, l})
      [{_, l, _} | _] -> throw({:erl_parse, l})
      [] -> throw({:erl_parse, 0})
    end
  end

  defp expr([{:-, _}, {:int, _, n} | rest]), do: {-n, rest}
  defp expr([{:-, _}, {:float, _, n} | rest]), do: {-n, rest}
  defp expr([{:int, _, n} | rest]), do: {n, rest}
  defp expr([{:float, _, n} | rest]), do: {n, rest}
  defp expr([{:atom, _, a} | rest]), do: {a, rest}
  defp expr([{:string, _, s} | rest]), do: more_strings(rest, s)
  defp expr([{:"{", _}, {:"}", _} | rest]), do: {{}, rest}

  defp expr([{:"{", _} | rest]) do
    {items, rest} = seq(rest, [])

    case rest do
      [{:"}", _} | rest] -> {List.to_tuple(items), rest}
      _ -> err(rest)
    end
  end

  defp expr([{:"[", _}, {:"]", _} | rest]), do: {[], rest}

  defp expr([{:"[", _} | rest]) do
    {items, rest} = seq(rest, [])

    case rest do
      [{:"]", _} | rest] ->
        {items, rest}

      [{:|, _} | rest] ->
        {tail, rest} = expr(rest)

        case rest do
          [{:"]", _} | rest] -> {items ++ tail, rest}
          _ -> err(rest)
        end

      _ ->
        err(rest)
    end
  end

  defp expr([{:map_open, _}, {:"}", _} | rest]), do: {%{}, rest}

  defp expr([{:map_open, _} | rest]) do
    {pairs, rest} = pairs(rest, [])

    case rest do
      [{:"}", _} | rest] -> {:maps.from_list(pairs), rest}
      _ -> err(rest)
    end
  end

  defp expr([{:"<<", _}, {:">>", _} | rest]), do: {"", rest}

  defp expr([{:"<<", _} | rest]) do
    {items, rest} = seq(rest, [])

    case rest do
      [{:">>", _} | rest] ->
        {:erlang.list_to_binary(Enum.map(items, fn i when is_list(i) -> :erlang.list_to_binary(i); i -> i end)), rest}

      _ ->
        err(rest)
    end
  end

  defp expr(toks), do: err(toks)

  defp more_strings([{:string, _, s2} | rest], s), do: more_strings(rest, s ++ s2)
  defp more_strings(rest, s), do: {s, rest}

  defp seq(toks, acc) do
    {t, rest} = expr(toks)

    case rest do
      [{:",", _} | rest] -> seq(rest, [t | acc])
      _ -> {:lists.reverse([t | acc]), rest}
    end
  end

  defp pairs(toks, acc) do
    {k, rest} = expr(toks)

    case rest do
      [{:"=>", _} | rest] ->
        {v, rest} = expr(rest)

        case rest do
          [{:",", _} | rest] -> pairs(rest, [{k, v} | acc])
          _ -> {:lists.reverse([{k, v} | acc]), rest}
        end

      _ ->
        err(rest)
    end
  end

  defp err([{_, l} | _]), do: throw({:erl_parse, l})
  defp err([{_, l, _} | _]), do: throw({:erl_parse, l})
  defp err([]), do: throw({:erl_parse, 0})

  # ---- tokenizer

  defp tokens(<<>>, _l, acc), do: :lists.reverse(acc)
  defp tokens(<<?\n, r::binary>>, l, acc), do: tokens(r, l + 1, acc)
  defp tokens(<<c, r::binary>>, l, acc) when c in [?\s, ?\t, ?\r], do: tokens(r, l, acc)
  defp tokens(<<?%, r::binary>>, l, acc), do: tokens(skip_line(r), l, acc)
  defp tokens(<<?#, ?{, r::binary>>, l, acc), do: tokens(r, l, [{:map_open, l} | acc])
  defp tokens(<<"<<", r::binary>>, l, acc), do: tokens(r, l, [{:"<<", l} | acc])
  defp tokens(<<">>", r::binary>>, l, acc), do: tokens(r, l, [{:">>", l} | acc])
  defp tokens(<<"=>", r::binary>>, l, acc), do: tokens(r, l, [{:"=>", l} | acc])

  defp tokens(<<?., r::binary>>, l, acc) do
    case r do
      <<c, _::binary>> when c in [?\s, ?\t, ?\r, ?\n, ?%] -> tokens(r, l, [{:dot, l} | acc])
      <<>> -> tokens(r, l, [{:dot, l} | acc])
      _ -> throw({:erl_parse, l})
    end
  end

  defp tokens(<<c, r::binary>>, l, acc) when c in [?{, ?}, ?[, ?], ?(, ?), ?,, ?|, ?-],
    do: tokens(r, l, [{List.to_atom([c]), l} | acc])

  defp tokens(<<?$, ?\\, r::binary>>, l, acc) do
    {c, r} = escape(r)
    tokens(r, l, [{:int, l, c} | acc])
  end

  defp tokens(<<?$, r::binary>>, l, acc) do
    <<c::utf8, r::binary>> = r
    tokens(r, l, [{:int, l, c} | acc])
  end

  defp tokens(<<?", r::binary>>, l, acc) do
    {s, r, l2} = quoted(r, ?", [], l)
    tokens(r, l2, [{:string, l, s} | acc])
  end

  defp tokens(<<?', r::binary>>, l, acc) do
    {s, r, l2} = quoted(r, ?', [], l)
    tokens(r, l2, [{:atom, l, List.to_atom(s)} | acc])
  end

  defp tokens(<<c, _::binary>> = b, l, acc) when c in ?0..?9 do
    {tok, r} = number(b, l)
    tokens(r, l, [tok | acc])
  end

  defp tokens(<<c, _::binary>> = b, l, acc) when c in ?a..?z do
    {name, r} = take_name(b, [])
    tokens(r, l, [{:atom, l, List.to_atom(name)} | acc])
  end

  defp tokens(_, l, _acc), do: throw({:erl_parse, l})

  defp skip_line(<<?\n, _::binary>> = r), do: r
  defp skip_line(<<_, r::binary>>), do: skip_line(r)
  defp skip_line(<<>>), do: <<>>

  defp take_name(<<c, r::binary>>, acc) when c in ?a..?z or c in ?A..?Z or c in ?0..?9 or c in [?_, ?@],
    do: take_name(r, [c | acc])

  defp take_name(r, acc), do: {:lists.reverse(acc), r}

  defp quoted(<<q, r::binary>>, q, acc, l), do: {:lists.reverse(acc), r, l}

  defp quoted(<<?\\, r::binary>>, q, acc, l) do
    {c, r} = escape(r)
    quoted(r, q, [c | acc], l)
  end

  defp quoted(<<?\n, r::binary>>, q, acc, l), do: quoted(r, q, [?\n | acc], l + 1)
  defp quoted(<<c::utf8, r::binary>>, q, acc, l), do: quoted(r, q, [c | acc], l)
  defp quoted(_, _q, _acc, l), do: throw({:erl_parse, l})

  defp escape(<<?n, r::binary>>), do: {?\n, r}
  defp escape(<<?t, r::binary>>), do: {?\t, r}
  defp escape(<<?r, r::binary>>), do: {?\r, r}
  defp escape(<<?s, r::binary>>), do: {?\s, r}
  defp escape(<<?e, r::binary>>), do: {27, r}
  defp escape(<<?b, r::binary>>), do: {?\b, r}
  defp escape(<<?f, r::binary>>), do: {?\f, r}
  defp escape(<<?v, r::binary>>), do: {?\v, r}
  defp escape(<<?d, r::binary>>), do: {127, r}

  defp escape(<<a, b, c, r::binary>>) when a in ?0..?7 and b in ?0..?7 and c in ?0..?7,
    do: {(a - ?0) * 64 + (b - ?0) * 8 + (c - ?0), r}

  defp escape(<<c::utf8, r::binary>>), do: {c, r}

  defp number(b, l) do
    {digits, r} = take_digits(b, [])

    case r do
      <<?#, r2::binary>> ->
        {ds, r3} = take_alnum(r2, [])
        {{:int, l, List.to_integer(ds, List.to_integer(digits))}, r3}

      <<?., d, _::binary>> when d in ?0..?9 ->
        <<?., r2::binary>> = r
        {frac, r3} = take_digits(r2, [])
        {exp, r4} = exponent(r3)
        {{:float, l, List.to_float(digits ++ [?. | frac] ++ exp)}, r4}

      _ ->
        {{:int, l, List.to_integer(digits)}, r}
    end
  end

  defp exponent(<<e, s, d, r::binary>>) when e in [?e, ?E] and s in [?+, ?-] and d in ?0..?9 do
    {ds, r} = take_digits(<<d, r::binary>>, [])
    {[?e, s | ds], r}
  end

  defp exponent(<<e, d, r::binary>>) when e in [?e, ?E] and d in ?0..?9 do
    {ds, r} = take_digits(<<d, r::binary>>, [])
    {[?e | ds], r}
  end

  defp exponent(r), do: {[], r}

  defp take_digits(<<c, r::binary>>, acc) when c in ?0..?9, do: take_digits(r, [c | acc])
  defp take_digits(<<?_, c, r::binary>>, acc) when c in ?0..?9, do: take_digits(r, [c | acc])
  defp take_digits(r, acc), do: {:lists.reverse(acc), r}

  defp take_alnum(<<c, r::binary>>, acc) when c in ?0..?9 or c in ?a..?z or c in ?A..?Z, do: take_alnum(r, [c | acc])
  defp take_alnum(r, acc), do: {:lists.reverse(acc), r}
end
