# Erlang's :re on top of PCRE2 (runtime/src/re.rs).
#
# A compiled pattern is {:re_pattern, capture_count, unicode, 0, {source, flags}}
# where flags is the runtime's option bitset; the runtime caches compiled code.

defmodule :re do
  import Bitwise

  @f_caseless 1
  @f_multiline 2
  @f_dotall 4
  @f_extended 8
  @f_unicode 16
  @f_ucp 32
  @f_ungreedy 64
  @f_firstline 128
  @f_anchored 256
  @f_dollar_endonly 512
  @f_dupnames 1024
  @f_no_auto_capture 2048
  @f_newline_any 4096
  @f_newline_crlf 8192
  @f_newline_anycrlf 16384
  @f_newline_cr 32768

  @m_notempty 1
  @m_notempty_atstart 2
  @m_anchored 4
  @m_notbol 8
  @m_noteol 16

  def version, do: "8.45 2021-06-15"

  def compile(re), do: compile(re, [])

  def compile(re, opts) when is_list(re), do: compile(:unicode.characters_to_binary(re), opts)

  def compile(re, opts) when is_binary(re) and is_list(opts) do
    {flags, _match} = split_opts(opts)

    case :tonic.pcre_compile(re, flags) do
      {:ok, ncaps, _names} ->
        {:ok, {:re_pattern, ncaps, if((flags &&& @f_unicode) != 0, do: 1, else: 0), 0, {re, flags}}}

      {:error, {msg, off}} ->
        {:error, {:tonic.str_to_charlist(msg), off}}
    end
  end

  def inspect({:re_pattern, _, _, _, {src, flags}}, :namelist) do
    {:ok, _, names} = :tonic.pcre_compile(src, flags)
    {:namelist, Enum.map(names, &elem(&1, 0))}
  end

  # Splits option list into {compile_flags, other_options}.
  defp split_opts(opts), do: split_opts(opts, 0, [])

  defp split_opts([], f, acc), do: {f, Enum.reverse(acc)}

  defp split_opts([o | t], f, acc) do
    case o do
      :caseless -> split_opts(t, f ||| @f_caseless, acc)
      :multiline -> split_opts(t, f ||| @f_multiline, acc)
      :dotall -> split_opts(t, f ||| @f_dotall, acc)
      :extended -> split_opts(t, f ||| @f_extended, acc)
      :unicode -> split_opts(t, f ||| @f_unicode, acc)
      :ucp -> split_opts(t, f ||| @f_ucp, acc)
      :ungreedy -> split_opts(t, f ||| @f_ungreedy, acc)
      :firstline -> split_opts(t, f ||| @f_firstline, acc)
      :dollar_endonly -> split_opts(t, f ||| @f_dollar_endonly, acc)
      :dupnames -> split_opts(t, f ||| @f_dupnames, acc)
      :no_auto_capture -> split_opts(t, f ||| @f_no_auto_capture, acc)
      {:newline, :any} -> split_opts(t, f ||| @f_newline_any, acc)
      {:newline, :crlf} -> split_opts(t, f ||| @f_newline_crlf, acc)
      {:newline, :anycrlf} -> split_opts(t, f ||| @f_newline_anycrlf, acc)
      {:newline, :cr} -> split_opts(t, f ||| @f_newline_cr, acc)
      {:newline, :lf} -> split_opts(t, f, acc)
      :bsr_anycrlf -> split_opts(t, f, acc)
      :bsr_unicode -> split_opts(t, f, acc)
      :no_start_optimize -> split_opts(t, f, acc)
      :never_utf -> split_opts(t, f, acc)
      :anchored -> split_opts(t, f, [o | acc])
      _ -> split_opts(t, f, [o | acc])
    end
  end

  def run(subject, re), do: run(subject, re, [])

  def run(subject, re, opts) when is_list(subject),
    do: run(:unicode.characters_to_binary(subject), re, opts)

  def run(subject, re, opts) when is_list(re), do: run(subject, :unicode.characters_to_binary(re), opts)

  def run(subject, re, opts) when is_binary(re) do
    {flags, rest} = split_opts(opts)

    case :tonic.pcre_compile(re, flags) do
      {:ok, ncaps, names} -> do_run(subject, re, flags, ncaps, names, rest)
      {:error, {msg, off}} -> {:error, {:compile, {:tonic.str_to_charlist(msg), off}}}
    end
  end

  def run(subject, {:re_pattern, _, _, _, {src, flags}}, opts) do
    {:ok, ncaps, names} = :tonic.pcre_compile(src, flags)
    {_f, rest} = split_opts(opts)
    do_run(subject, src, flags, ncaps, names, rest)
  end

  defp do_run(subject, src, flags, ncaps, names, opts) do
    global = :global in opts
    offset = Keyword.get(opts, :offset, 0)
    unicode = (flags &&& @f_unicode) != 0

    mflags =
      Enum.reduce(opts, 0, fn
        :notempty, a -> a ||| @m_notempty
        :notempty_atstart, a -> a ||| @m_notempty_atstart
        :anchored, a -> a ||| @m_anchored
        :notbol, a -> a ||| @m_notbol
        :noteol, a -> a ||| @m_noteol
        _, a -> a
      end)

    {spec, type} =
      case List.keyfind(opts, :capture, 0) do
        {:capture, s} -> {s, :index}
        {:capture, s, t} -> {s, t}
        nil -> {:all, :index}
      end

    # An empty capture list reports a bare :match (as OTP).
    spec = if spec == [] or (spec == :all_names and names == []), do: :none, else: spec

    if global do
      case global_loop(subject, src, flags, offset, mflags, unicode, []) do
        [] -> :nomatch
        ms when spec == :none -> if ms == [], do: :nomatch, else: :match
        ms -> {:match, Enum.map(ms, &format(&1, spec, type, subject, ncaps, names, unicode))}
      end
    else
      case :tonic.pcre_match(src, flags, subject, offset, mflags) do
        nil -> :nomatch
        _ when spec == :none -> :match
        m -> {:match, format(m, spec, type, subject, ncaps, names, unicode)}
      end
    end
  end

  # Erlang's global matching: after an empty match retry at the same offset
  # with [notempty_atstart, anchored], else advance one character.
  defp global_loop(subject, src, flags, offset, mflags, unicode, acc) do
    case :tonic.pcre_match(src, flags, subject, offset, mflags) do
      nil ->
        Enum.reverse(acc)

      [{s, 0} | _] = m ->
        acc = [m | acc]

        case :tonic.pcre_match(src, flags, subject, s, mflags ||| @m_notempty_atstart ||| @m_anchored) do
          nil ->
            if s >= byte_size(subject) do
              Enum.reverse(acc)
            else
              global_loop(subject, src, flags, s + char_len(subject, s, unicode), mflags, unicode, acc)
            end

          [{s2, l2} | _] = m2 ->
            global_loop(subject, src, flags, s2 + l2, mflags, unicode, [m2 | acc])
        end

      [{s, l} | _] = m ->
        global_loop(subject, src, flags, s + l, mflags, unicode, [m | acc])
    end
  end

  defp char_len(_subject, _pos, false), do: 1

  defp char_len(subject, pos, true) do
    case binary_part(subject, pos, byte_size(subject) - pos) do
      <<c::utf8, _::binary>> -> byte_size(<<c::utf8>>)
      _ -> 1
    end
  end

  defp format(m, spec, type, subject, ncaps, names, unicode) do
    groups =
      case spec do
        :all -> m
        :all_but_first -> tl(m)
        :first -> [hd(m)]
        :all_names -> Enum.map(names, fn {_, i} -> group(m, i) end)
        list when is_list(list) -> Enum.map(list, &group(m, index_of(&1, names, ncaps)))
      end

    Enum.map(groups, &convert(&1, type, subject, unicode))
  end

  defp index_of(i, _names, _ncaps) when is_integer(i), do: i

  defp index_of(name, names, _ncaps) do
    name =
      cond do
        is_atom(name) -> Atom.to_string(name)
        is_list(name) -> List.to_string(name)
        true -> name
      end

    case List.keyfind(names, name, 0) do
      {_, i} -> i
      nil -> -1
    end
  end

  defp group(_m, -1), do: {-1, 0}

  defp group(m, i) do
    case Enum.at(m, i) do
      nil -> {-1, 0}
      g -> g
    end
  end

  defp convert(g, :index, _subject, _unicode), do: g
  defp convert({-1, 0}, :binary, _subject, _u), do: ""
  defp convert({s, l}, :binary, subject, _u), do: binary_part(subject, s, l)
  defp convert({-1, 0}, :list, _subject, _u), do: []
  defp convert({s, l}, :list, subject, true), do: :unicode.characters_to_list(binary_part(subject, s, l))
  defp convert({s, l}, :list, subject, false), do: :erlang.binary_to_list(binary_part(subject, s, l))

  def replace(subject, re, replacement), do: replace(subject, re, replacement, [])

  def replace(subject, re, replacement, opts) do
    subject = if is_list(subject), do: :unicode.characters_to_binary(subject), else: subject
    replacement = if is_list(replacement), do: :unicode.characters_to_binary(replacement), else: replacement
    {ret, opts} = take_return(opts, :iodata)

    case run(subject, re, [{:capture, :all, :index} | opts]) do
      :nomatch ->
        ret_conv(subject, ret)

      {:match, ms} ->
        ms = if :global in opts, do: ms, else: [ms]
        {out, pos} =
          Enum.reduce(ms, {[], 0}, fn [{s, l} | _] = groups, {acc, pos} ->
            rep = expand_rep(replacement, groups, subject)
            {[acc, binary_part(subject, pos, s - pos), rep], s + l}
          end)

        ret_conv(IO.iodata_to_binary([out, binary_part(subject, pos, byte_size(subject) - pos)]), ret)
    end
  end

  defp take_return(opts, default) do
    case List.keytake(opts, :return, 0) do
      {{:return, r}, rest} -> {r, rest}
      nil -> {default, opts}
    end
  end

  defp ret_conv(b, :binary), do: b
  defp ret_conv(b, :iodata), do: b
  defp ret_conv(b, :list), do: :unicode.characters_to_list(b)

  defp expand_rep(<<>>, _g, _s), do: []
  defp expand_rep(<<?\\, ?\\, r::binary>>, g, s), do: [?\\ | expand_rep(r, g, s)]
  defp expand_rep(<<?\\, ?&, r::binary>>, g, s), do: [?& | expand_rep(r, g, s)]
  defp expand_rep(<<?&, r::binary>>, g, s), do: [grp(g, 0, s) | expand_rep(r, g, s)]

  defp expand_rep(<<?\\, d, r::binary>>, g, s) when d in ?0..?9,
    do: [grp(g, d - ?0, s) | expand_rep(r, g, s)]

  defp expand_rep(<<c, r::binary>>, g, s), do: [c | expand_rep(r, g, s)]

  defp grp(g, i, s) do
    case Enum.at(g, i) do
      {st, l} when st >= 0 -> binary_part(s, st, l)
      _ -> ""
    end
  end

  def split(subject, re), do: split(subject, re, [])

  def split(subject, re, opts) do
    subject = if is_list(subject), do: :unicode.characters_to_binary(subject), else: subject
    {ret, opts} = take_return(opts, :iodata)
    {parts, opts} = case List.keytake(opts, :parts, 0) do
      {{:parts, p}, rest} -> {p, rest}
      nil -> {:infinity, opts}
    end
    trim = :trim in opts
    opts = List.delete(opts, :trim)

    ms =
      case run(subject, re, [:global, {:capture, :all, :index} | opts]) do
        :nomatch -> []
        {:match, ms} -> ms
      end

    ms = if parts == :infinity, do: ms, else: Enum.take(ms, max(parts - 1, 0))

    {pieces, pos} =
      Enum.reduce(ms, {[], 0}, fn [{s, l} | groups], {acc, pos} ->
        caps = Enum.map(groups, fn {gs, gl} -> if gs >= 0, do: binary_part(subject, gs, gl), else: "" end)
        {Enum.reverse(caps, [binary_part(subject, pos, s - pos) | acc]), s + l}
      end)

    pieces = Enum.reverse([binary_part(subject, pos, byte_size(subject) - pos) | pieces])
    pieces = if trim, do: pieces |> Enum.reverse() |> Enum.drop_while(&(&1 == "")) |> Enum.reverse(), else: pieces

    case ret do
      :list -> Enum.map(pieces, &:unicode.characters_to_list/1)
      _ -> pieces
    end
  end
end
