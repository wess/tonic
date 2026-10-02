defmodule Regex do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
























































































































































































  defstruct re_pattern: nil, source: "", opts: [], re_version: ""



  defmodule CompileError do




    defexception message: "regex could not be compiled"
  end

























  def compile(source, opts \\ "") when is_binary(source) do
    compile(source, opts, version())
  end

  defp compile(source, opts, version) when is_binary(opts) do
    case translate_options(opts, []) do
      {:error, rest} ->
        {:error, {:invalid_option, rest}}

      translated_opts ->
        compile(source, translated_opts, version)
    end
  end

  defp compile(source, opts, version) when is_list(opts) do
    case :re.compile(source, opts) do
      {:ok, re_pattern} ->
        {:ok, %Regex{re_pattern: re_pattern, re_version: version, source: source, opts: opts}}

      error ->
        error
    end
  end





  def compile!(source, options \\ "") when is_binary(source) do
    case compile(source, options) do
      {:ok, regex} -> regex
      {:error, {reason, at}} -> raise Regex.CompileError, "#{reason} at position #{at}"
    end
  end









  def recompile(%Regex{} = regex) do
    version = version()

    case regex do
      %{re_version: ^version} ->
        {:ok, regex}

      _ ->
        %{source: source, opts: opts} = regex
        compile(source, opts, version)
    end
  end






  def recompile!(regex) do
    case recompile(regex) do
      {:ok, regex} -> regex
      {:error, {reason, at}} -> raise Regex.CompileError, "#{reason} at position #{at}"
    end
  end






  def version do
    {:re.version(), :erlang.system_info(:endian)}
  end

















  def match?(%Regex{} = regex, string) when is_binary(string) do
    safe_run(regex, string, [{:capture, :none}]) == :match
  end



  def regex?(term)
  def regex?(%Regex{}), do: true
  def regex?(_), do: false

































  def run(regex, string, options \\ [])

  def run(%Regex{} = regex, string, options) when is_binary(string) do
    return = Keyword.get(options, :return, :binary)
    captures = Keyword.get(options, :capture, :all)
    offset = Keyword.get(options, :offset, 0)

    case safe_run(regex, string, [{:capture, captures, return}, {:offset, offset}]) do
      :nomatch -> nil
      :match -> []
      {:match, results} -> results
    end
  end






















  def named_captures(regex, string, options \\ []) when is_binary(string) do
    names = names(regex)
    options = Keyword.put(options, :capture, names)
    results = run(regex, string, options)
    if results, do: Enum.zip(names, results) |> Enum.into(%{})
  end





  def re_pattern(%Regex{re_pattern: compiled}) do
    compiled
  end











  def source(%Regex{source: source}) do
    source
  end
















  def opts(%Regex{opts: opts}) do
    opts
  end











  def names(%Regex{re_pattern: compiled, re_version: version, source: source}) do
    re_pattern =
      case version() do
        ^version ->
          compiled

        _ ->
          {:ok, recompiled} = :re.compile(source)
          recompiled
      end

    {:namelist, names} = :re.inspect(re_pattern, :namelist)
    names
  end












































  def scan(regex, string, options \\ [])

  def scan(%Regex{} = regex, string, options) when is_binary(string) do
    return = Keyword.get(options, :return, :binary)
    captures = Keyword.get(options, :capture, :all)
    offset = Keyword.get(options, :offset, 0)
    options = [{:capture, captures, return}, :global, {:offset, offset}]

    case safe_run(regex, string, options) do
      :match -> []
      :nomatch -> []
      {:match, results} -> results
    end
  end

  defp safe_run(
         %Regex{re_pattern: compiled, source: source, re_version: version, opts: compile_opts},
         string,
         options
       ) do
    case version() do
      ^version ->
        :re.run(string, compiled, options)

      _ when is_list(compile_opts) ->
        :re.run(string, source, compile_opts ++ options)

      # TODO: This clause is kept for compatibility with previous Elixir versions. Remove on v2.0+.
      _ when is_binary(compile_opts) ->
        :re.run(string, source, translate_options(compile_opts, options))
    end
  end























































  def split(regex, string, options \\ [])

  def split(%Regex{}, "", opts) do
    if Keyword.get(opts, :trim, false) do
      []
    else
      [""]
    end
  end

  def split(%Regex{} = regex, string, opts)
      when is_binary(string) and is_list(opts) do
    on = Keyword.get(opts, :on, :first)

    case safe_run(regex, string, [:global, capture: on]) do
      {:match, matches} ->
        index = parts_to_index(Keyword.get(opts, :parts, :infinity))
        trim = Keyword.get(opts, :trim, false)
        include_captures = Keyword.get(opts, :include_captures, false)
        do_split(matches, string, 0, index, trim, include_captures)

      :match ->
        [string]

      :nomatch ->
        [string]
    end
  end

  defp parts_to_index(:infinity), do: 0
  defp parts_to_index(n) when is_integer(n) and n > 0, do: n

  defp do_split(_, string, offset, _counter, true, _with_captures)
       when byte_size(string) <= offset do
    []
  end

  defp do_split(_, string, offset, 1, _trim, _with_captures),
    do: [binary_part(string, offset, byte_size(string) - offset)]

  defp do_split([], string, offset, _counter, _trim, _with_captures),
    do: [binary_part(string, offset, byte_size(string) - offset)]

  defp do_split([[{pos, _} | h] | t], string, offset, counter, trim, with_captures)
       when pos - offset < 0 do
    do_split([h | t], string, offset, counter, trim, with_captures)
  end

  defp do_split([[] | t], string, offset, counter, trim, with_captures),
    do: do_split(t, string, offset, counter, trim, with_captures)

  defp do_split([[{pos, length} | h] | t], string, offset, counter, trim, true) do
    new_offset = pos + length
    keep = pos - offset

    <<_::binary-size(^offset), part::binary-size(^keep), match::binary-size(^length), _::binary>> =
      string

    if keep == 0 and trim do
      [match | do_split([h | t], string, new_offset, counter - 1, trim, true)]
    else
      [part, match | do_split([h | t], string, new_offset, counter - 1, trim, true)]
    end
  end

  defp do_split([[{pos, length} | h] | t], string, offset, counter, trim, false) do
    new_offset = pos + length
    keep = pos - offset

    if keep == 0 and trim do
      do_split([h | t], string, new_offset, counter, trim, false)
    else
      <<_::binary-size(^offset), part::binary-size(^keep), _::binary>> = string
      [part | do_split([h | t], string, new_offset, counter - 1, trim, false)]
    end
  end





















































  def replace(%Regex{} = regex, string, replacement, options \\ [])
      when is_binary(string) and is_list(options) do
    opts = if Keyword.get(options, :global) != false, do: [:global], else: []
    opts = [{:capture, :all, :index} | opts]

    case safe_run(regex, string, opts) do
      :nomatch ->
        string

      {:match, [mlist | t]} when is_list(mlist) ->
        apply_list(string, precompile_replacement(replacement), [mlist | t])
        |> IO.iodata_to_binary()

      {:match, slist} ->
        apply_list(string, precompile_replacement(replacement), [slist])
        |> IO.iodata_to_binary()
    end
  end

  defp precompile_replacement(replacement) when is_function(replacement) do
    {:arity, arity} = Function.info(replacement, :arity)
    {replacement, arity}
  end

  defp precompile_replacement(""), do: []

  defp precompile_replacement(<<?\\, ?g, ?{, rest::binary>>) when byte_size(rest) > 0 do
    {ns, <<?}, rest::binary>>} = pick_int(rest)
    [List.to_integer(ns) | precompile_replacement(rest)]
  end

  defp precompile_replacement(<<?\\, ?\\, rest::binary>>) do
    [<<?\\>> | precompile_replacement(rest)]
  end

  defp precompile_replacement(<<?\\, x, rest::binary>>) when x in ?0..?9 do
    {ns, rest} = pick_int(rest)
    [List.to_integer([x | ns]) | precompile_replacement(rest)]
  end

  defp precompile_replacement(<<x, rest::binary>>) do
    case precompile_replacement(rest) do
      [head | t] when is_binary(head) ->
        [<<x, head::binary>> | t]

      other ->
        [<<x>> | other]
    end
  end

  defp pick_int(<<x, rest::binary>>) when x in ?0..?9 do
    {found, rest} = pick_int(rest)
    {[x | found], rest}
  end

  defp pick_int(bin) do
    {[], bin}
  end

  defp apply_list(string, replacement, list) do
    apply_list(string, string, 0, replacement, list)
  end

  defp apply_list(_, "", _, _, []) do
    []
  end

  defp apply_list(_, string, _, _, []) do
    string
  end

  defp apply_list(whole, string, pos, replacement, [[{mpos, _} | _] | _] = list)
       when mpos > pos do
    length = mpos - pos
    <<untouched::binary-size(^length), rest::binary>> = string
    [untouched | apply_list(whole, rest, mpos, replacement, list)]
  end

  defp apply_list(whole, string, pos, replacement, [[{pos, length} | _] = head | tail]) do
    <<_::size(^length)-binary, rest::binary>> = string
    new_data = apply_replace(whole, replacement, head)
    [new_data | apply_list(whole, rest, pos + length, replacement, tail)]
  end

  defp apply_replace(string, {fun, arity}, indexes) do
    apply(fun, get_indexes(string, indexes, arity))
  end

  defp apply_replace(_, [bin], _) when is_binary(bin) do
    bin
  end

  defp apply_replace(string, repl, indexes) do
    indexes = List.to_tuple(indexes)

    for part <- repl do
      cond do
        is_binary(part) ->
          part

        part >= tuple_size(indexes) ->
          ""

        true ->
          get_index(string, elem(indexes, part))
      end
    end
  end

  defp get_index(_string, {pos, _length}) when pos < 0 do
    ""
  end

  defp get_index(string, {pos, length}) do
    <<_::size(^pos)-binary, res::size(^length)-binary, _::binary>> = string
    res
  end

  defp get_indexes(_string, _, 0) do
    []
  end

  defp get_indexes(string, [], arity) do
    ["" | get_indexes(string, [], arity - 1)]
  end

  defp get_indexes(string, [h | t], arity) do
    [get_index(string, h) | get_indexes(string, t, arity - 1)]
  end














  def escape(string) when is_binary(string) do
    string
    |> escape(_length = 0, string)
    |> IO.iodata_to_binary()
  end

  @escapable :binary.bin_to_list(".^$*+?()[]{}|#-\\\t\n\v\f\r\s")

  defp escape(<<char, rest::binary>>, length, original) when char in @escapable do
    escape_char(rest, length, original, char)
  end

  defp escape(<<_, rest::binary>>, length, original) do
    escape(rest, length + 1, original)
  end

  defp escape(<<>>, _length, original) do
    original
  end

  defp escape_char(<<rest::binary>>, 0, _original, char) do
    [?\\, char | escape(rest, 0, rest)]
  end

  defp escape_char(<<rest::binary>>, length, original, char) do
    [binary_part(original, 0, length), ?\\, char | escape(rest, 0, rest)]
  end

  # Helpers

  defp translate_options(<<?s, t::binary>>, acc),
    do: translate_options(t, [:dotall, {:newline, :anycrlf} | acc])

  defp translate_options(<<?u, t::binary>>, acc), do: translate_options(t, [:unicode, :ucp | acc])
  defp translate_options(<<?i, t::binary>>, acc), do: translate_options(t, [:caseless | acc])
  defp translate_options(<<?x, t::binary>>, acc), do: translate_options(t, [:extended | acc])
  defp translate_options(<<?f, t::binary>>, acc), do: translate_options(t, [:firstline | acc])
  defp translate_options(<<?U, t::binary>>, acc), do: translate_options(t, [:ungreedy | acc])
  defp translate_options(<<?m, t::binary>>, acc), do: translate_options(t, [:multiline | acc])

  defp translate_options(<<?r, t::binary>>, acc) do
    IO.warn("the /r modifier in regular expressions is deprecated, please use /U instead")
    translate_options(t, [:ungreedy | acc])
  end

  defp translate_options(<<>>, acc), do: acc
  defp translate_options(t, _acc), do: {:error, t}
end

# Imported from Elixir 1.18.3 lib/elixir/lib/regex.ex (docs and specs stripped;
# line numbers match the original).
