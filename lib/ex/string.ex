# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.

defmodule String do





































































































































































































































































































  @conditional_mappings [:greek, :turkic]
























  def printable?(string, character_limit \\ :infinity)
      when is_binary(string) and
             (character_limit == :infinity or
                (is_integer(character_limit) and character_limit >= 0)) do
    recur_printable?(string, character_limit)
  end

  defp recur_printable?(<<_::binary>>, 0), do: true
  defp recur_printable?(<<>>, _character_limit), do: true

  for char <- 0x20..0x7E do
    defp recur_printable?(<<unquote(char), rest::binary>>, character_limit) do
      recur_printable?(rest, decrement(character_limit))
    end
  end

  for char <- [?\n, ?\r, ?\t, ?\v, ?\b, ?\f, ?\e, ?\d, ?\a] do
    defp recur_printable?(<<unquote(char), rest::binary>>, character_limit) do
      recur_printable?(rest, decrement(character_limit))
    end
  end

  defp recur_printable?(<<char::utf8, rest::binary>>, character_limit)
       when char in 0xA0..0xD7FF
       when char in 0xE000..0xFFFD
       when char in 0x10000..0x10FFFF do
    recur_printable?(rest, decrement(character_limit))
  end

  defp recur_printable?(_string, _character_limit) do
    false
  end

  defp decrement(:infinity), do: :infinity
  defp decrement(character_limit), do: character_limit - 1





























  defdelegate split(binary), to: String.Break



















































































































  def split(string, pattern, options \\ [])

  def split(string, %Regex{} = pattern, options) when is_binary(string) and is_list(options) do
    Regex.split(pattern, string, options)
  end

  def split(string, "", options) when is_binary(string) and is_list(options) do
    parts = Keyword.get(options, :parts, :infinity)
    index = parts_to_index(parts)
    trim = Keyword.get(options, :trim, false)

    if trim == false and index != 1 do
      ["" | split_empty(string, trim, index - 1)]
    else
      split_empty(string, trim, index)
    end
  end

  def split(string, [], options) when is_binary(string) and is_list(options) do
    if string == "" and Keyword.get(options, :trim, false) do
      []
    else
      [string]
    end
  end

  def split(string, pattern, options) when is_binary(string) and is_list(options) do
    parts = Keyword.get(options, :parts, :infinity)
    trim = Keyword.get(options, :trim, false)

    case {parts, trim} do
      {:infinity, false} ->
        :binary.split(string, pattern, [:global])

      {:infinity, true} ->
        :binary.split(string, pattern, [:global, :trim_all])

      {2, false} ->
        :binary.split(string, pattern)

      _ ->
        pattern = maybe_compile_pattern(pattern)
        split_each(string, pattern, trim, parts_to_index(parts))
    end
  end

  defp parts_to_index(:infinity), do: 0
  defp parts_to_index(n) when is_integer(n) and n > 0, do: n

  defp split_empty("", true, 1), do: []
  defp split_empty(string, _, 1), do: [IO.iodata_to_binary(string)]

  defp split_empty(string, trim, count) do
    case :unicode_util.gc(string) do
      [gc] -> [grapheme_to_binary(gc) | split_empty("", trim, 1)]
      [gc | rest] -> [grapheme_to_binary(gc) | split_empty(rest, trim, count - 1)]
      [] -> split_empty("", trim, 1)
      {:error, <<byte, rest::bits>>} -> [<<byte>> | split_empty(rest, trim, count - 1)]
    end
  end

  defp split_each("", _pattern, true, 1), do: []
  defp split_each(string, _pattern, _trim, 1) when is_binary(string), do: [string]

  defp split_each(string, pattern, trim, count) do
    case do_splitter(string, pattern, trim) do
      {h, t} -> [h | split_each(t, pattern, trim, count - 1)]
      nil -> []
    end
  end



































  def splitter(string, pattern, options \\ [])

  def splitter(string, "", options) when is_binary(string) and is_list(options) do
    if Keyword.get(options, :trim, false) do
      Stream.unfold(string, &next_grapheme/1)
    else
      Stream.unfold(:match, &do_empty_splitter(&1, string))
    end
  end

  def splitter(string, [], options) when is_binary(string) and is_list(options) do
    if string == "" and Keyword.get(options, :trim, false) do
      Stream.duplicate(string, 0)
    else
      Stream.duplicate(string, 1)
    end
  end

  def splitter(string, pattern, options) when is_binary(string) and is_list(options) do
    pattern = maybe_compile_pattern(pattern)
    trim = Keyword.get(options, :trim, false)
    Stream.unfold(string, &do_splitter(&1, pattern, trim))
  end

  defp do_empty_splitter(:match, string), do: {"", string}
  defp do_empty_splitter(:nomatch, _string), do: nil
  defp do_empty_splitter("", _), do: {"", :nomatch}
  defp do_empty_splitter(string, _), do: next_grapheme(string)

  defp do_splitter(:nomatch, _pattern, _), do: nil
  defp do_splitter("", _pattern, false), do: {"", :nomatch}
  defp do_splitter("", _pattern, true), do: nil

  defp do_splitter(bin, pattern, trim) do
    case :binary.split(bin, pattern) do
      ["", second] when trim -> do_splitter(second, pattern, trim)
      [first, second] -> {first, second}
      [first] -> {first, :nomatch}
    end
  end

  defp maybe_compile_pattern(pattern) when is_tuple(pattern), do: pattern
  defp maybe_compile_pattern(pattern), do: :binary.compile_pattern(pattern)


































  def split_at(string, position)

  def split_at(string, position)
      when is_binary(string) and is_integer(position) and position >= 0 do
    do_split_at(string, position)
  end

  def split_at(string, position)
      when is_binary(string) and is_integer(position) and position < 0 do
    position = length(string) + position

    case position >= 0 do
      true -> do_split_at(string, position)
      false -> {"", string}
    end
  end

  defp do_split_at(string, position) do
    remaining = byte_size_remaining_at(string, position)
    start = byte_size(string) - remaining
    <<left::size(^start)-binary, right::size(^remaining)-binary>> = string
    {left, right}
  end





























  def equivalent?(string1, string2) when is_binary(string1) and is_binary(string2) do
    normalize(string1, :nfd) == normalize(string2, :nfd)
  end




















































  def normalize(string, form)

  def normalize(string, :nfd) when is_binary(string) do
    case :unicode.characters_to_nfd_binary(string) do
      string when is_binary(string) -> string
      {:error, good, <<head, rest::binary>>} -> good <> <<head>> <> normalize(rest, :nfd)
    end
  end

  def normalize(string, :nfc) when is_binary(string) do
    case :unicode.characters_to_nfc_binary(string) do
      string when is_binary(string) -> string
      {:error, good, <<head, rest::binary>>} -> good <> <<head>> <> normalize(rest, :nfc)
    end
  end

  def normalize(string, :nfkd) when is_binary(string) do
    case :unicode.characters_to_nfkd_binary(string) do
      string when is_binary(string) -> string
      {:error, good, <<head, rest::binary>>} -> good <> <<head>> <> normalize(rest, :nfkd)
    end
  end

  def normalize(string, :nfkc) when is_binary(string) do
    case :unicode.characters_to_nfkc_binary(string) do
      string when is_binary(string) -> string
      {:error, good, <<head, rest::binary>>} -> good <> <<head>> <> normalize(rest, :nfkc)
    end
  end






































  def upcase(string, mode \\ :default)

  def upcase("", _mode) do
    ""
  end

  def upcase(string, :default) when is_binary(string) do
    String.Unicode.upcase(string, [], :default)
  end

  def upcase(string, :ascii) when is_binary(string) do
    IO.iodata_to_binary(upcase_ascii(string))
  end

  def upcase(string, mode) when is_binary(string) and mode in @conditional_mappings do
    String.Unicode.upcase(string, [], mode)
  end

  defp upcase_ascii(<<char, rest::bits>>) when char >= ?a and char <= ?z,
    do: [char - 32 | upcase_ascii(rest)]

  defp upcase_ascii(<<char, rest::bits>>), do: [char | upcase_ascii(rest)]
  defp upcase_ascii(<<>>), do: []















































  def downcase(string, mode \\ :default)

  def downcase("", _mode) do
    ""
  end

  def downcase(string, :default) when is_binary(string) do
    String.Unicode.downcase(string, [], :default)
  end

  def downcase(string, :ascii) when is_binary(string) do
    IO.iodata_to_binary(downcase_ascii(string))
  end

  def downcase(string, mode) when is_binary(string) and mode in @conditional_mappings do
    String.Unicode.downcase(string, [], mode)
  end

  defp downcase_ascii(<<char, rest::bits>>) when char >= ?A and char <= ?Z,
    do: [char + 32 | downcase_ascii(rest)]

  defp downcase_ascii(<<char, rest::bits>>), do: [char | downcase_ascii(rest)]
  defp downcase_ascii(<<>>), do: []





























  def capitalize(string, mode \\ :default)

  def capitalize(<<char, rest::binary>>, :ascii) do
    char = if char >= ?a and char <= ?z, do: char - 32, else: char
    <<char>> <> downcase(rest, :ascii)
  end

  @letter_I <<0x0049::utf8>>
  @letter_i <<0x0069::utf8>>
  @letter_I_dot_above <<0x0130::utf8>>

  def capitalize(<<@letter_i, right::binary>>, mode) do
    if(mode == :turkic, do: @letter_I_dot_above, else: @letter_I) <> downcase(right, mode)
  end

  def capitalize(string, mode) when is_binary(string) do
    case :unicode_util.gc(string) do
      [gc] -> grapheme_to_binary(:string.titlecase([gc]))
      [gc, rest] -> grapheme_to_binary(:string.titlecase([gc])) <> downcase(rest, mode)
      [gc | rest] -> grapheme_to_binary(:string.titlecase([gc])) <> downcase(rest, mode)
      [] -> ""
      {:error, <<byte, rest::bits>>} -> <<byte>> <> downcase(rest, mode)
    end
  end



  defdelegate rstrip(binary), to: String.Break, as: :trim_trailing



  def rstrip(string, char) when is_integer(char) do
    replace_trailing(string, <<char::utf8>>, "")
  end



























  def replace_leading(string, match, replacement)
      when is_binary(string) and is_binary(match) and is_binary(replacement) do
    if match == "" do
      raise ArgumentError, "cannot use an empty string as the match to replace"
    end

    prefix_size = byte_size(match)
    suffix_size = byte_size(string) - prefix_size
    replace_leading(string, match, replacement, prefix_size, suffix_size, 0)
  end

  defp replace_leading(string, match, replacement, prefix_size, suffix_size, acc)
       when suffix_size >= 0 do
    case string do
      <<prefix::size(^prefix_size)-binary, suffix::binary>> when prefix == match ->
        replace_leading(
          suffix,
          match,
          replacement,
          prefix_size,
          suffix_size - prefix_size,
          acc + 1
        )

      _ ->
        prepend_unless_empty(duplicate(replacement, acc), string)
    end
  end

  defp replace_leading(string, _match, replacement, _prefix_size, _suffix_size, acc) do
    prepend_unless_empty(duplicate(replacement, acc), string)
  end



























  def replace_trailing(string, match, replacement)
      when is_binary(string) and is_binary(match) and is_binary(replacement) do
    if match == "" do
      raise ArgumentError, "cannot use an empty string as the match to replace"
    end

    suffix_size = byte_size(match)
    prefix_size = byte_size(string) - suffix_size
    replace_trailing(string, match, replacement, prefix_size, suffix_size, 0)
  end

  defp replace_trailing(string, match, replacement, prefix_size, suffix_size, acc)
       when prefix_size >= 0 do
    case string do
      <<prefix::size(^prefix_size)-binary, suffix::binary>> when suffix == match ->
        replace_trailing(
          prefix,
          match,
          replacement,
          prefix_size - suffix_size,
          suffix_size,
          acc + 1
        )

      _ ->
        append_unless_empty(string, duplicate(replacement, acc))
    end
  end

  defp replace_trailing(string, _match, replacement, _prefix_size, _suffix_size, acc) do
    append_unless_empty(string, duplicate(replacement, acc))
  end






























  def replace_prefix(string, match, replacement)
      when is_binary(string) and is_binary(match) and is_binary(replacement) do
    prefix_size = byte_size(match)

    case string do
      <<prefix::size(^prefix_size)-binary, suffix::binary>> when prefix == match ->
        prepend_unless_empty(replacement, suffix)

      _ ->
        string
    end
  end






























  def replace_suffix(string, match, replacement)
      when is_binary(string) and is_binary(match) and is_binary(replacement) do
    suffix_size = byte_size(match)
    prefix_size = byte_size(string) - suffix_size

    case string do
      <<prefix::size(^prefix_size)-binary, suffix::binary>> when suffix == match ->
        append_unless_empty(prefix, replacement)

      _ ->
        string
    end
  end



  defp prepend_unless_empty("", suffix), do: suffix
  defp prepend_unless_empty(prefix, suffix), do: prefix <> suffix

  defp append_unless_empty(prefix, ""), do: prefix
  defp append_unless_empty(prefix, suffix), do: prefix <> suffix



  defdelegate lstrip(binary), to: String.Break, as: :trim_leading



  def lstrip(string, char) when is_integer(char) do
    replace_leading(string, <<char::utf8>>, "")
  end



  def strip(string) do
    trim(string)
  end



  def strip(string, char) do
    trim(string, <<char::utf8>>)
  end












  defdelegate trim_leading(string), to: String.Break














  def trim_leading(string, to_trim)
      when is_binary(string) and is_binary(to_trim) do
    replace_leading(string, to_trim, "")
  end












  defdelegate trim_trailing(string), to: String.Break














  def trim_trailing(string, to_trim)
      when is_binary(string) and is_binary(to_trim) do
    replace_trailing(string, to_trim, "")
  end












  def trim(string) when is_binary(string) do
    string
    |> trim_leading()
    |> trim_trailing()
  end












  def trim(string, to_trim) when is_binary(string) and is_binary(to_trim) do
    string
    |> trim_leading(to_trim)
    |> trim_trailing(to_trim)
  end
































  def pad_leading(string, count, padding \\ [" "])

  def pad_leading(string, count, padding) when is_binary(padding) do
    pad_leading(string, count, graphemes(padding))
  end

  def pad_leading(string, count, [_ | _] = padding)
      when is_binary(string) and is_integer(count) and count >= 0 do
    pad(:leading, string, count, padding)
  end
































  def pad_trailing(string, count, padding \\ [" "])

  def pad_trailing(string, count, padding) when is_binary(padding) do
    pad_trailing(string, count, graphemes(padding))
  end

  def pad_trailing(string, count, [_ | _] = padding)
      when is_binary(string) and is_integer(count) and count >= 0 do
    pad(:trailing, string, count, padding)
  end

  defp pad(kind, string, count, padding) do
    string_length = length(string)

    if string_length >= count do
      string
    else
      filler = build_filler(count - string_length, padding, padding, 0, [])

      case kind do
        :leading -> [filler | string]
        :trailing -> [string | filler]
      end
      |> IO.iodata_to_binary()
    end
  end

  defp build_filler(0, _source, _padding, _size, filler), do: filler

  defp build_filler(count, source, [], size, filler) do
    rem_filler =
      rem(count, size)
      |> build_filler(source, source, 0, [])

    filler =
      filler
      |> IO.iodata_to_binary()
      |> duplicate(div(count, size) + 1)

    [filler | rem_filler]
  end

  defp build_filler(count, source, [elem | rest], size, filler)
       when is_binary(elem) do
    build_filler(count - 1, source, rest, size + 1, [filler | elem])
  end

  defp build_filler(_count, _source, [elem | _rest], _size, _filler) do
    raise ArgumentError, "expected a string padding element, got: #{inspect(elem)}"
  end



  def rjust(subject, length) do
    rjust(subject, length, ?\s)
  end



  def rjust(subject, length, pad) when is_integer(pad) and is_integer(length) and length >= 0 do
    pad(:leading, subject, length, [<<pad::utf8>>])
  end



  def ljust(subject, length) do
    ljust(subject, length, ?\s)
  end



  def ljust(subject, length, pad) when is_integer(pad) and is_integer(length) and length >= 0 do
    pad(:trailing, subject, length, [<<pad::utf8>>])
  end
















































































  def replace(subject, pattern, replacement, options \\ [])
      when is_binary(subject) and
             (is_binary(replacement) or is_function(replacement, 1)) and
             is_list(options) do
    replace_guarded(subject, pattern, replacement, options)
  end

  defp replace_guarded(subject, %{__struct__: Regex} = regex, replacement, options) do
    Regex.replace(regex, subject, replacement, options)
  end

  defp replace_guarded(subject, "", "", _) do
    subject
  end

  defp replace_guarded(subject, [], _, _) do
    subject
  end

  defp replace_guarded(subject, "", replacement_binary, options)
       when is_binary(replacement_binary) do
    if Keyword.get(options, :global, true) do
      intersperse_bin(subject, replacement_binary, [replacement_binary])
    else
      replacement_binary <> subject
    end
  end

  defp replace_guarded(subject, "", replacement_fun, options) do
    if Keyword.get(options, :global, true) do
      intersperse_fun(subject, replacement_fun, [replacement_fun.("")])
    else
      IO.iodata_to_binary([replacement_fun.("") | subject])
    end
  end

  defp replace_guarded(subject, pattern, replacement, options) do
    if insert = Keyword.get(options, :insert_replaced) do
      IO.warn(
        "String.replace/4 with :insert_replaced option is deprecated. " <>
          "Please use :binary.replace/4 instead or pass an anonymous function as replacement"
      )

      binary_options = if Keyword.get(options, :global) != false, do: [:global], else: []
      :binary.replace(subject, pattern, replacement, [insert_replaced: insert] ++ binary_options)
    else
      matches =
        if Keyword.get(options, :global, true) do
          :binary.matches(subject, pattern)
        else
          case :binary.match(subject, pattern) do
            :nomatch -> []
            match -> [match]
          end
        end

      IO.iodata_to_binary(do_replace(subject, matches, replacement, 0))
    end
  end

  defp intersperse_bin(subject, replacement, acc) do
    case :unicode_util.gc(subject) do
      [current | rest] ->
        intersperse_bin(rest, replacement, [replacement, current | acc])

      [] ->
        reverse_characters_to_binary(acc)

      {:error, <<byte, rest::bits>>} ->
        reverse_characters_to_binary(acc) <>
          <<byte>> <> intersperse_bin(rest, replacement, [replacement])
    end
  end

  defp intersperse_fun(subject, replacement, acc) do
    case :unicode_util.gc(subject) do
      [current | rest] ->
        intersperse_fun(rest, replacement, [replacement.(""), current | acc])

      [] ->
        reverse_characters_to_binary(acc)

      {:error, <<byte, rest::bits>>} ->
        reverse_characters_to_binary(acc) <>
          <<byte>> <> intersperse_fun(rest, replacement, [replacement.("")])
    end
  end

  defp do_replace(subject, [], _, n) do
    [binary_part(subject, n, byte_size(subject) - n)]
  end

  defp do_replace(subject, [{start, length} | matches], replacement, n) do
    prefix = binary_part(subject, n, start - n)

    middle =
      if is_binary(replacement) do
        replacement
      else
        replacement.(binary_part(subject, start, length))
      end

    [prefix, middle | do_replace(subject, matches, replacement, start + length)]
  end
































  def reverse(string) when is_binary(string), do: :tonic.str_reverse(string)



  defp do_reverse([grapheme | rest], acc),
    do: do_reverse(:unicode_util.gc(rest), [grapheme | acc])

  defp do_reverse([], acc),
    do: :unicode.characters_to_binary(acc)

  defp do_reverse({:error, <<byte, rest::bits>>}, acc),
    do: :unicode.characters_to_binary(acc) <> <<byte>> <> do_reverse(:unicode_util.gc(rest), [])




















  def duplicate(subject, n) when is_binary(subject) and is_integer(n) and n >= 0 do
    :binary.copy(subject, n)
  end




























  def codepoints(string) when is_binary(string), do: :tonic.str_codepoints(string)



  defp do_codepoints(<<codepoint::utf8, rest::bits>>) do
    [<<codepoint::utf8>> | do_codepoints(rest)]
  end

  defp do_codepoints(<<byte, rest::bits>>) do
    [<<byte>> | do_codepoints(rest)]
  end

  defp do_codepoints(<<>>), do: []











































  def next_codepoint(<<cp::utf8, rest::binary>>), do: {<<cp::utf8>>, rest}
  def next_codepoint(<<byte, rest::binary>>), do: {<<byte>>, rest}
  def next_codepoint(<<>>), do: nil















































  def valid?(string, algorithm \\ :default)

  def valid?(<<string::binary>>, :default), do: valid_utf8?(string)
  def valid?(<<string::binary>>, :fast_ascii), do: valid_utf8_fast_ascii?(string)

  defp valid_utf8?(<<_::utf8, rest::bits>>), do: valid_utf8?(rest)
  defp valid_utf8?(<<>>), do: true
  defp valid_utf8?(_), do: false

  defp valid_utf8_fast_ascii?(<<a::56, rest::bits>>)
       when Bitwise.band(0x80808080808080, a) == 0 do
    valid_utf8_fast_ascii?(rest)
  end

  defp valid_utf8_fast_ascii?(<<_::utf8, rest::bits>>), do: valid_utf8_fast_ascii?(rest)
  defp valid_utf8_fast_ascii?(<<>>), do: true
  defp valid_utf8_fast_ascii?(_), do: false



  def valid_character?(string) do
    case string do
      <<_::utf8>> -> valid?(string)
      _ -> false
    end
  end

  defguardp replace_invalid_ii_of_iii(i, ii)
            when Bitwise.bor(Bitwise.bsl(i, 6), ii) in 32..863 or
                   Bitwise.bor(Bitwise.bsl(i, 6), ii) in 896..1023

  defguardp replace_invalid_ii_of_iv(i, ii)
            when Bitwise.bor(Bitwise.bsl(i, 6), ii) in 16..271

  defguardp replace_invalid_iii_of_iv(i, ii, iii)
            when Bitwise.bor(Bitwise.bor(Bitwise.bsl(i, 12), Bitwise.bsl(ii, 6)), iii) in 1024..17407

  defguardp replace_invalid_is_next(next) when Bitwise.bsr(next, 6) !== 0b10




















  def replace_invalid(bytes, replacement \\ "�")
      when is_binary(bytes) and is_binary(replacement) do
    do_replace_invalid(bytes, replacement, <<>>)
  end

  # Valid ASCII (for better average speed)
  defp do_replace_invalid(<<ascii::8, next::8, _::binary>> = rest, rep, acc)
       when ascii in 0..127 and replace_invalid_is_next(next) do
    <<_::8, rest::binary>> = rest
    do_replace_invalid(rest, rep, acc <> <<ascii::8>>)
  end

  # Valid UTF-8
  defp do_replace_invalid(<<grapheme::utf8, rest::binary>>, rep, acc) do
    do_replace_invalid(rest, rep, acc <> <<grapheme::utf8>>)
  end

  # 2/3 truncated sequence
  defp do_replace_invalid(<<0b1110::4, i::4, 0b10::2, ii::6>>, rep, acc)
       when replace_invalid_ii_of_iii(i, ii) do
    acc <> rep
  end

  defp do_replace_invalid(
         <<0b1110::4, i::4, 0b10::2, ii::6, next::8, _::binary>> = rest,
         rep,
         acc
       )
       when replace_invalid_ii_of_iii(i, ii) and replace_invalid_is_next(next) do
    <<_::16, rest::binary>> = rest
    do_replace_invalid(rest, rep, acc <> rep)
  end

  # 2/4
  defp do_replace_invalid(<<0b11110::5, i::3, 0b10::2, ii::6>>, rep, acc)
       when replace_invalid_ii_of_iv(i, ii) do
    acc <> rep
  end

  defp do_replace_invalid(
         <<0b11110::5, i::3, 0b10::2, ii::6, next::8, _::binary>> = rest,
         rep,
         acc
       )
       when replace_invalid_ii_of_iv(i, ii) and replace_invalid_is_next(next) do
    <<_::16, rest::binary>> = rest
    do_replace_invalid(rest, rep, acc <> rep)
  end

  # 3/4
  defp do_replace_invalid(<<0b11110::5, i::3, 0b10::2, ii::6, 0b10::2, iii::6>>, rep, acc)
       when replace_invalid_iii_of_iv(i, ii, iii) do
    acc <> rep
  end

  defp do_replace_invalid(
         <<0b11110::5, i::3, 0b10::2, ii::6, 0b10::2, iii::6, next::8, _::binary>> = rest,
         rep,
         acc
       )
       when replace_invalid_iii_of_iv(i, ii, iii) and replace_invalid_is_next(next) do
    <<_::24, rest::binary>> = rest
    do_replace_invalid(rest, rep, acc <> rep)
  end

  # Everything else
  defp do_replace_invalid(<<_, rest::binary>>, rep, acc),
    do: do_replace_invalid(rest, rep, acc <> rep)

  # Final
  defp do_replace_invalid(<<>>, _, acc), do: acc































  def chunk(string, trait)

  def chunk("", _), do: []

  def chunk(string, trait) when is_binary(string) and trait in [:valid, :printable] do
    {cp, _} = next_codepoint(string)
    pred_fn = make_chunk_pred(trait)
    do_chunk(string, pred_fn.(cp), pred_fn)
  end

  defp do_chunk(string, flag, pred_fn), do: do_chunk(string, [], <<>>, flag, pred_fn)

  defp do_chunk(<<>>, acc, <<>>, _, _), do: Enum.reverse(acc)

  defp do_chunk(<<>>, acc, chunk, _, _), do: Enum.reverse(acc, [chunk])

  defp do_chunk(string, acc, chunk, flag, pred_fn) do
    {cp, rest} = next_codepoint(string)

    if pred_fn.(cp) != flag do
      do_chunk(rest, [chunk | acc], cp, not flag, pred_fn)
    else
      do_chunk(rest, acc, chunk <> cp, flag, pred_fn)
    end
  end

  defp make_chunk_pred(:valid), do: &valid?/1
  defp make_chunk_pred(:printable), do: &printable?/1
























  def graphemes(string) when is_binary(string), do: :tonic.str_graphemes(string)

  defp do_graphemes(gcs) do
    case :unicode_util.gc(gcs) do
      [gc | rest] -> [grapheme_to_binary(gc) | do_graphemes(rest)]
      [] -> []
      {:error, <<byte, rest::bits>>} -> [<<byte>> | do_graphemes(rest)]
    end
  end



















  def next_grapheme(string) when is_binary(string) do
    case :unicode_util.gc(string) do
      [gc] -> {grapheme_to_binary(gc), <<>>}
      [gc, rest] -> {grapheme_to_binary(gc), rest}
      [gc | rest] -> {grapheme_to_binary(gc), rest}
      [] -> nil
      {:error, <<byte, rest::bits>>} -> {<<byte>>, rest}
    end
  end


















  def next_grapheme_size(string) when is_binary(string) do
    case :unicode_util.gc(string) do
      [gc] -> {grapheme_byte_size(gc), <<>>}
      [gc, rest] -> {grapheme_byte_size(gc), rest}
      [gc | rest] -> {grapheme_byte_size(gc), rest}
      [] -> nil
      {:error, <<_, rest::bits>>} -> {1, rest}
    end
  end


















  def first(string) when is_binary(string) do
    case :unicode_util.gc(string) do
      [gc | _] -> grapheme_to_binary(gc)
      [] -> nil
      {:error, <<byte, _::bits>>} -> <<byte>>
    end
  end




















  def last(""), do: nil
  def last(string) when is_binary(string), do: do_last(:unicode_util.gc(string), nil)

  defp do_last([gc | rest], _), do: do_last(:unicode_util.gc(rest), gc)
  defp do_last([], acc) when is_binary(acc), do: acc
  defp do_last([], acc), do: :unicode.characters_to_binary([acc])
  defp do_last({:error, <<byte, rest::bits>>}, _), do: do_last(:unicode_util.gc(rest), <<byte>>)














  def length(string) when is_binary(string), do: :tonic.str_length(string)

  defp length(<<byte1, byte2, rest::binary>> = binary, acc)
       when byte1 <= 127 and byte1 != ?\r and byte2 <= 127 and byte2 != ?\r do
    skip = skip_length(rest, 1)
    length(binary_part(binary, skip, byte_size(binary) - skip), acc + skip)
  end

  defp length(gcs, acc) do
    case :unicode_util.gc(gcs) do
      [_ | rest] -> length(rest, acc + 1)
      [] -> acc
      {:error, <<_, rest::bits>>} -> length(rest, acc + 1)
    end
  end

  defp skip_length(<<byte, rest::binary>>, acc)
       when byte <= 127 and byte != ?\r,
       do: skip_length(rest, acc + 1)

  defp skip_length(_binary, acc),
    do: acc































  def at(string, position) when is_binary(string) and is_integer(position) and position >= 0 do
    do_at(string, position)
  end

  def at(string, position) when is_binary(string) and is_integer(position) and position < 0 do
    position = length(string) + position

    case position >= 0 do
      true -> do_at(string, position)
      false -> nil
    end
  end

  defp do_at(string, position) do
    left = byte_size_remaining_at(string, position)

    string
    |> binary_part(byte_size(string) - left, left)
    |> first()
  end










































  def slice(_, _, 0) do
    ""
  end

  def slice(string, start, length)
      when is_binary(string) and is_integer(start) and is_integer(length) and start >= 0 and
             length >= 0 do
    do_slice(string, start, length)
  end

  def slice(string, start, length)
      when is_binary(string) and is_integer(start) and is_integer(length) and start < 0 and
             length >= 0 do
    start = max(length(string) + start, 0)
    do_slice(string, start, length)
  end

  defp do_slice(string, start, length) do
    from_start = byte_size_remaining_at(string, start)
    rest = binary_part(string, byte_size(string) - from_start, from_start)

    from_end = byte_size_remaining_at(rest, length)
    binary_part(rest, 0, from_start - from_end)
  end































































  def slice(string, first..last//step = range) when is_binary(string) do
    # TODO: Support negative steps as a reverse on Elixir v2.0.
    cond do
      step > 0 ->
        slice_range(string, first, last, step)

      step == -1 and first > last ->
        IO.warn(
          "negative steps are not supported in String.slice/2, pass #{first}..#{last}//1 instead"
        )

        slice_range(string, first, last, 1)

      true ->
        raise ArgumentError,
              "String.slice/2 does not accept ranges with negative steps, got: #{inspect(range)}"
    end
  end

  # TODO: Remove me on v2.0
  def slice(string, %{__struct__: Range, first: first, last: last} = range)
      when is_binary(string) do
    step = if first <= last, do: 1, else: -1
    slice(string, Map.put(range, :step, step))
  end

  defp slice_range("", _, _, _), do: ""

  defp slice_range(_string, first, last, _step) when first >= 0 and last >= 0 and first > last do
    ""
  end

  defp slice_range(string, first, last, step) when first >= 0 do
    from_start = byte_size_remaining_at(string, first)
    rest = binary_part(string, byte_size(string) - from_start, from_start)

    cond do
      last == -1 ->
        slice_every(rest, byte_size(rest), step)

      last >= 0 and step == 1 ->
        from_end = byte_size_remaining_at(rest, last - first + 1)
        binary_part(rest, 0, from_start - from_end)

      last >= 0 ->
        slice_every(rest, last - first + 1, step)

      true ->
        rest
        |> slice_range_negative(0, last)
        |> slice_every(byte_size(string), step)
    end
  end

  defp slice_range(string, first, last, step) do
    string
    |> slice_range_negative(first, last)
    |> slice_every(byte_size(string), step)
  end

  defp slice_range_negative(string, first, last) do
    {reversed_bytes, length} = acc_bytes(string, [], 0)
    first = add_if_negative(first, length) |> max(0)
    last = add_if_negative(last, length)

    if first > last or first > length do
      ""
    else
      last = min(last + 1, length)
      reversed_bytes = Enum.drop(reversed_bytes, length - last)
      {length_bytes, start_bytes} = split_bytes(reversed_bytes, 0, last - first)
      binary_part(string, start_bytes, length_bytes)
    end
  end

  defp slice_every(string, _count, 1), do: string
  defp slice_every(string, count, step), do: slice_every(string, count, step, [])

  defp slice_every(string, count, to_drop, acc) when count > 0 do
    case :unicode_util.gc(string) do
      [current | rest] ->
        rest
        |> drop(to_drop)
        |> slice_every(count - to_drop, to_drop, [current | acc])

      [] ->
        reverse_characters_to_binary(acc)

      {:error, <<byte, rest::bits>>} ->
        reverse_characters_to_binary(acc) <>
          <<byte>> <> slice_every(drop(rest, to_drop), count - to_drop, to_drop, [])
    end
  end

  defp slice_every(_string, _count, _to_drop, acc) do
    reverse_characters_to_binary(acc)
  end

  defp drop(string, 1), do: string

  defp drop(string, count) do
    case :unicode_util.gc(string) do
      [_ | rest] -> drop(rest, count - 1)
      [] -> ""
      {:error, <<_, rest::bits>>} -> drop(rest, count - 1)
    end
  end

  defp acc_bytes(string, bytes, length) do
    case :unicode_util.gc(string) do
      [gc | rest] -> acc_bytes(rest, [grapheme_byte_size(gc) | bytes], length + 1)
      [] -> {bytes, length}
      {:error, <<_, rest::bits>>} -> acc_bytes(rest, [1 | bytes], length + 1)
    end
  end

  defp add_if_negative(value, to_add) when value < 0, do: value + to_add
  defp add_if_negative(value, _to_add), do: value

  defp split_bytes(rest, acc, 0), do: {acc, Enum.sum(rest)}
  defp split_bytes([], acc, _), do: {acc, 0}
  defp split_bytes([head | tail], acc, count), do: split_bytes(tail, head + acc, count - 1)
























































  def byte_slice(string, start_bytes, size_bytes)
      when is_binary(string) and is_integer(start_bytes) and is_integer(size_bytes) and
             size_bytes >= 0 do
    total = byte_size(string)
    start_bytes = if start_bytes < 0, do: max(total + start_bytes, 0), else: start_bytes

    if start_bytes < total do
      :erlang.binary_part(string, start_bytes, total - start_bytes)
      |> invalid_prefix()
      |> invalid_suffix(size_bytes)
    else
      ""
    end
  end

  defp invalid_prefix(<<0b10::2, _::6, rest::binary>>), do: invalid_prefix(rest)
  defp invalid_prefix(rest), do: rest

  defp invalid_suffix(string, size) do
    last = invalid_suffix(string, min(size, byte_size(string)) - 1, 0)
    :erlang.binary_part(string, 0, last)
  end

  defp invalid_suffix(string, last, truncated) when last >= 0 do
    byte = :binary.at(string, last)

    cond do
      # ASCII byte, discard all truncated entries
      byte <= 127 ->
        last + 1

      # In the middle of a codepoint
      byte <= 191 ->
        invalid_suffix(string, last - 1, truncated + 1)

      # 2 bytes codepoint start
      byte <= 223 ->
        if truncated == 1, do: last + truncated + 1, else: last

      # 3 bytes codepoint start
      byte <= 239 ->
        if truncated == 2, do: last + truncated + 1, else: last

      # 4 bytes codepoint start
      byte <= 247 ->
        if truncated == 3, do: last + truncated + 1, else: last

      # Invalid codepoint, discard it, stop checking
      true ->
        last + 1
    end
  end

  defp invalid_suffix(_string, _last, _truncated), do: 0

































  def starts_with?(string, prefix) when is_binary(string) and is_binary(prefix) do
    starts_with_string?(string, byte_size(string), prefix)
  end

  def starts_with?(string, prefix) when is_binary(string) and is_list(prefix) do
    string_size = byte_size(string)
    Enum.any?(prefix, &starts_with_string?(string, string_size, &1))
  end

  def starts_with?(string, prefix) when is_binary(string) do
    IO.warn("compiled patterns are deprecated in starts_with?")
    Kernel.match?({0, _}, :binary.match(string, prefix))
  end


  defp starts_with_string?(string, string_size, prefix) when is_binary(prefix) do
    prefix_size = byte_size(prefix)

    if prefix_size <= string_size do
      prefix == binary_part(string, 0, prefix_size)
    else
      false
    end
  end
























  def ends_with?(string, suffix) when is_binary(string) and is_binary(suffix) do
    ends_with_string?(string, byte_size(string), suffix)
  end

  def ends_with?(string, suffix) when is_binary(string) and is_list(suffix) do
    string_size = byte_size(string)
    Enum.any?(suffix, &ends_with_string?(string, string_size, &1))
  end


  defp ends_with_string?(string, string_size, suffix) when is_binary(suffix) do
    suffix_size = byte_size(suffix)

    if suffix_size <= string_size do
      suffix == binary_part(string, string_size - suffix_size, suffix_size)
    else
      false
    end
  end
















  def match?(string, regex) when is_binary(string) do
    Regex.match?(regex, string)
  end




























































  def contains?(string, contents) when is_binary(string) and is_list(contents) do
    list_contains?(string, byte_size(string), contents, [])
  end

  def contains?(string, contents) when is_binary(string) do
    "" == contents or :binary.match(string, contents) != :nomatch
  end

  defp list_contains?(string, size, [head | tail], acc) do
    case byte_size(head) do
      0 -> true
      head_size when head_size > size -> list_contains?(string, size, tail, acc)
      _ -> list_contains?(string, size, tail, [head | acc])
    end
  end

  defp list_contains?(_string, _size, [], []),
    do: false

  defp list_contains?(string, _size, [], contents),
    do: :binary.match(string, contents) != :nomatch


















  def to_charlist(string) when is_binary(string) do
    case :unicode.characters_to_list(string) do
      result when is_list(result) ->
        result

      {:error, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :invalid

      {:incomplete, encoded, rest} ->
        raise UnicodeConversionError, encoded: encoded, rest: rest, kind: :incomplete
    end
  end























  def to_atom(string) when is_binary(string) do
    :erlang.binary_to_atom(string, :utf8)
  end






























  def to_existing_atom(string) when is_binary(string) do
    :erlang.binary_to_existing_atom(string, :utf8)
  end























  def to_integer(string) when is_binary(string) do
    :erlang.binary_to_integer(string)
  end













  def to_integer(string, base) when is_binary(string) and is_integer(base) do
    :erlang.binary_to_integer(string, base)
  end


























  def to_float(string) when is_binary(string) do
    :erlang.binary_to_float(string)
  end































  def bag_distance(_string, ""), do: 0.0
  def bag_distance("", _string), do: 0.0

  def bag_distance(string1, string2) when is_binary(string1) and is_binary(string2) do
    {bag1, length1} = string_to_bag(string1, %{}, 0)
    {bag2, length2} = string_to_bag(string2, %{}, 0)

    diff1 = bag_difference(bag1, bag2)
    diff2 = bag_difference(bag2, bag1)

    1 - max(diff1, diff2) / max(length1, length2)
  end

  defp string_to_bag(string, bag, length) do
    case :unicode_util.gc(string) do
      [gc | rest] -> string_to_bag(rest, bag_store(bag, gc), length + 1)
      [] -> {bag, length}
      {:error, <<byte, rest::bits>>} -> string_to_bag(rest, bag_store(bag, <<byte>>), length + 1)
    end
  end

  defp bag_store(bag, gc) do
    case bag do
      %{^gc => current} -> %{bag | gc => current + 1}
      %{} -> Map.put(bag, gc, 1)
    end
  end

  defp bag_difference(bag1, bag2) do
    Enum.sum_by(bag1, fn {char, count1} ->
      case bag2 do
        %{^char => count2} -> max(count1 - count2, 0)
        %{} -> count1
      end
    end)
  end


























  def jaro_distance(string1, string2)

  def jaro_distance(string, string) when is_binary(string), do: 1.0
  def jaro_distance(_string, ""), do: 0.0
  def jaro_distance("", _string), do: 0.0

  def jaro_distance(string1, string2) when is_binary(string1) and is_binary(string2) do
    # TODO: Replace by :string.jaro_similarity/2 when we require Erlang/OTP 27+
    :elixir_utils.jaro_similarity(string1, string2)
  end
















  def myers_difference(string1, string2) when is_binary(string1) and is_binary(string2) do
    graphemes(string1)
    |> List.myers_difference(graphemes(string2))
    |> Enum.map(fn {kind, chars} -> {kind, IO.iodata_to_binary(chars)} end)
  end




  def to_char_list(string), do: String.to_charlist(string)

  ## Helpers







  defp byte_size_unicode(binary) when is_binary(binary), do: byte_size(binary)
  defp byte_size_unicode([head]), do: byte_size_unicode(head)
  defp byte_size_unicode([head | tail]), do: byte_size_unicode(head) + byte_size_unicode(tail)

  defp byte_size_remaining_at(unicode, 0) do
    byte_size_unicode(unicode)
  end

  defp byte_size_remaining_at(unicode, n) when is_binary(unicode), do: :tonic.str_remaining_at(unicode, n)
  defp byte_size_remaining_at(unicode, n) do
    case :unicode_util.gc(unicode) do
      [_] -> 0
      [_ | rest] -> byte_size_remaining_at(rest, n - 1)
      [] -> 0
      {:error, <<_, bin::bits>>} -> byte_size_remaining_at(bin, n - 1)
    end
  end
  defp codepoint_byte_size(cp) when cp <= 0x007F, do: 1
  defp codepoint_byte_size(cp) when cp <= 0x07FF, do: 2
  defp codepoint_byte_size(cp) when cp <= 0xFFFF, do: 3
  defp codepoint_byte_size(_), do: 4

  defp grapheme_to_binary(cp) when is_integer(cp), do: <<cp::utf8>>
  defp grapheme_to_binary(gc) when is_list(gc), do: for(cp <- gc, do: <<cp::utf8>>, into: "")

  defp grapheme_byte_size(cp) when is_integer(cp), do: codepoint_byte_size(cp)
  defp grapheme_byte_size(cps), do: grapheme_byte_size(cps, 0)

  defp grapheme_byte_size([cp | cps], acc),
    do: grapheme_byte_size(cps, acc + codepoint_byte_size(cp))

  defp grapheme_byte_size([], acc),
    do: acc

  defp reverse_characters_to_binary(acc),
    do: acc |> :lists.reverse() |> :unicode.characters_to_binary()
end

# Imported from Elixir 1.18.3 lib/elixir/lib/string.ex (docs and specs stripped;
# line numbers match the original).
