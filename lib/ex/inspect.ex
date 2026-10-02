import Kernel, except: [inspect: 1]
import Inspect.Algebra
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
alias Code.Identifier

defprotocol Inspect do




































































































































  # Handle structs in Any
  @fallback_to_any true

























































































  def inspect(term, opts)
end

defimpl Inspect, for: Atom do
  require Macro

  def inspect(atom, opts) do
    color_doc(Macro.inspect_atom(:literal, atom), color_key(atom), opts)
  end

  defp color_key(atom) when is_boolean(atom), do: :boolean
  defp color_key(nil), do: nil
  defp color_key(_), do: :atom
end

defimpl Inspect, for: BitString do
  def inspect(term, opts) when is_binary(term) do
    %Inspect.Opts{binaries: bins, base: base, printable_limit: printable_limit} = opts

    if bins == :as_strings or
         (bins == :infer and String.printable?(term, printable_limit) and base == :decimal) do
      inspected =
        case Identifier.escape(term, ?", printable_limit) do
          {escaped, ""} -> [?", escaped, ?"]
          {escaped, _} -> [?", escaped, ?", " <> ..."]
        end

      color_doc(IO.iodata_to_binary(inspected), :string, opts)
    else
      inspect_bitstring(term, opts)
    end
  end

  def inspect(term, opts) do
    inspect_bitstring(term, opts)
  end

  defp inspect_bitstring("", opts) do
    color_doc("<<>>", :binary, opts)
  end

  defp inspect_bitstring(bitstring, opts) do
    left = color_doc("<<", :binary, opts)
    right = color_doc(">>", :binary, opts)
    inner = each_bit(bitstring, opts.limit, opts)
    group(concat(concat(left, nest(inner, 2)), right))
  end

  defp each_bit(_, 0, _) do
    "..."
  end

  defp each_bit(<<>>, _counter, _opts) do
    :doc_nil
  end

  defp each_bit(<<h::8>>, _counter, opts) do
    Inspect.Integer.inspect(h, opts)
  end

  defp each_bit(<<h, t::bitstring>>, counter, opts) do
    flex_glue(
      concat(Inspect.Integer.inspect(h, opts), ","),
      each_bit(t, decrement(counter), opts)
    )
  end

  defp each_bit(bitstring, _counter, opts) do
    size = bit_size(bitstring)
    <<h::size(^size)>> = bitstring
    concat(Inspect.Integer.inspect(h, opts), "::size(" <> Integer.to_string(size) <> ")")
  end


  defp decrement(:infinity), do: :infinity
  defp decrement(counter), do: counter - 1
end

defimpl Inspect, for: List do
  def inspect([], opts) do
    color_doc("[]", :list, opts)
  end

  # TODO: Remove :char_list and :as_char_lists handling on v2.0
  def inspect(term, opts) do
    %Inspect.Opts{
      charlists: lists,
      char_lists: lists_deprecated,
      printable_limit: printable_limit
    } = opts

    lists =
      if lists == :infer and lists_deprecated != :infer do
        case lists_deprecated do
          :as_char_lists ->
            IO.warn(
              "the :char_lists inspect option and its :as_char_lists " <>
                "value are deprecated, use the :charlists option and its " <>
                ":as_charlists value instead"
            )

            :as_charlists

          _ ->
            IO.warn("the :char_lists inspect option is deprecated, use :charlists instead")
            lists_deprecated
        end
      else
        lists
      end

    open = color_doc("[", :list, opts)
    sep = color_doc(",", :list, opts)
    close = color_doc("]", :list, opts)

    cond do
      lists == :as_charlists or (lists == :infer and List.ascii_printable?(term, printable_limit)) ->
        inspected =
          case Identifier.escape(IO.chardata_to_string(term), ?", printable_limit) do
            {escaped, ""} -> [?~, ?c, ?", escaped, ?"]
            {escaped, _} -> [?~, ?c, ?", escaped, ?", " ++ ..."]
          end

        color_doc(IO.iodata_to_binary(inspected), :charlist, opts)

      keyword?(term) ->
        container_doc(open, term, close, opts, &keyword/2, separator: sep, break: :strict)

      true ->
        container_doc(open, term, close, opts, &to_doc/2, separator: sep)
    end
  end


  def keyword({key, value}, opts) do
    key = color_doc(Macro.inspect_atom(:key, key), :atom, opts)
    concat(key, concat(" ", to_doc(value, opts)))
  end


  def keyword?([{key, _value} | rest]) when is_atom(key) do
    case Atom.to_charlist(key) do
      [?E, ?l, ?i, ?x, ?i, ?r, ?.] ++ _ -> false
      _ -> keyword?(rest)
    end
  end

  def keyword?([]), do: true
  def keyword?(_other), do: false
end

defimpl Inspect, for: Tuple do
  def inspect(tuple, opts) do
    open = color_doc("{", :tuple, opts)
    sep = color_doc(",", :tuple, opts)
    close = color_doc("}", :tuple, opts)
    container_opts = [separator: sep, break: :flex]
    container_doc(open, Tuple.to_list(tuple), close, opts, &to_doc/2, container_opts)
  end
end

defimpl Inspect, for: Map do
  def inspect(map, opts) do
    list =
      if Keyword.get(opts.custom_options, :sort_maps) do
        map |> Map.to_list() |> :lists.sort()
      else
        Map.to_list(map)
      end

    fun =
      if Inspect.List.keyword?(list) do
        &Inspect.List.keyword/2
      else
        sep = color_doc(" => ", :map, opts)
        &to_assoc(&1, &2, sep)
      end

    map_container_doc(list, "", opts, fun)
  end

  def inspect(map, name, infos, opts) do
    fun = fn %{field: field}, opts -> Inspect.List.keyword({field, Map.get(map, field)}, opts) end
    map_container_doc(infos, name, opts, fun)
  end

  defp to_assoc({key, value}, opts, sep) do
    concat(concat(to_doc(key, opts), sep), to_doc(value, opts))
  end

  defp map_container_doc(list, name, opts, fun) do
    open = color_doc("%" <> name <> "{", :map, opts)
    sep = color_doc(",", :map, opts)
    close = color_doc("}", :map, opts)
    container_doc(open, list, close, opts, fun, separator: sep, break: :strict)
  end
end

defimpl Inspect, for: Integer do
  def inspect(term, %Inspect.Opts{base: base} = opts) do
    inspected = Integer.to_string(term, base_to_value(base)) |> prepend_prefix(base)
    color_doc(inspected, :number, opts)
  end

  defp base_to_value(base) do
    case base do
      :binary -> 2
      :decimal -> 10
      :octal -> 8
      :hex -> 16
    end
  end

  defp prepend_prefix(value, :decimal), do: value

  defp prepend_prefix(<<?-, value::binary>>, base) do
    "-" <> prepend_prefix(value, base)
  end

  defp prepend_prefix(value, base) do
    prefix =
      case base do
        :binary -> "0b"
        :octal -> "0o"
        :hex -> "0x"
      end

    prefix <> value
  end
end

defimpl Inspect, for: Float do
  def inspect(float, opts) do
    abs = abs(float)

    formatted =
      if abs >= 1.0 and abs < 1.0e16 and trunc(float) == float do
        [Integer.to_string(trunc(float)), ?., ?0]
      else
        Float.to_charlist(float)
      end

    color_doc(IO.iodata_to_binary(formatted), :number, opts)
  end
end

defimpl Inspect, for: Regex do
  def inspect(regex = %{opts: regex_opts}, opts) when is_list(regex_opts) do
    case translate_options(regex_opts, []) do
      :error ->
        concat([
          "Regex.compile!(",
          Inspect.BitString.inspect(regex.source, opts),
          ", ",
          Inspect.List.inspect(regex_opts, opts),
          ")"
        ])

      translated_opts ->
        {escaped, _} =
          regex.source
          |> normalize(<<>>)
          |> Identifier.escape(?/, :infinity, &escape_map/1)

        source = IO.iodata_to_binary([?~, ?r, ?/, escaped, ?/, translated_opts])
        color_doc(source, :regex, opts)
    end
  end

  defp translate_options([:dotall, {:newline, :anycrlf} | t], acc),
    do: translate_options(t, [?s | acc])

  defp translate_options([:unicode, :ucp | t], acc), do: translate_options(t, [?u | acc])
  defp translate_options([:caseless | t], acc), do: translate_options(t, [?i | acc])
  defp translate_options([:extended | t], acc), do: translate_options(t, [?x | acc])
  defp translate_options([:firstline | t], acc), do: translate_options(t, [?f | acc])
  defp translate_options([:ungreedy | t], acc), do: translate_options(t, [?U | acc])
  defp translate_options([:multiline | t], acc), do: translate_options(t, [?m | acc])
  defp translate_options([], acc), do: acc
  defp translate_options(_t, _acc), do: :error

  defp normalize(<<?\\, ?\\, rest::binary>>, acc), do: normalize(rest, <<acc::binary, ?\\, ?\\>>)
  defp normalize(<<?\\, ?/, rest::binary>>, acc), do: normalize(rest, <<acc::binary, ?/>>)
  defp normalize(<<?\\, ?#, ?{, rest::binary>>, acc), do: normalize(rest, <<acc::binary, ?#, ?{>>)
  defp normalize(<<char, rest::binary>>, acc), do: normalize(rest, <<acc::binary, char>>)
  defp normalize(<<>>, acc), do: acc

  defp escape_map(?\a), do: [?\\, ?a]
  defp escape_map(?\f), do: [?\\, ?f]
  defp escape_map(?\n), do: [?\\, ?n]
  defp escape_map(?\r), do: [?\\, ?r]
  defp escape_map(?\t), do: [?\\, ?t]
  defp escape_map(?\v), do: [?\\, ?v]
  defp escape_map(_), do: false
end

defimpl Inspect, for: Function do
  @elixir_compiler :binary.bin_to_list("elixir_compiler_")

  def inspect(function, _opts) do
    if function, do: :tonic.fun_to_string(function), else: nil
  end

  def __real_inspect__(fun_info, mod, name) do
    cond do
      not is_atom(mod) ->
        "#Function<#{uniq(fun_info)}/#{fun_info[:arity]}>"

      fun_info[:type] == :external and fun_info[:env] == [] ->
        inspected_as_atom = Macro.inspect_atom(:literal, mod)
        inspected_as_function = Macro.inspect_atom(:remote_call, name)
        "&#{inspected_as_atom}.#{inspected_as_function}/#{fun_info[:arity]}"

      match?(@elixir_compiler ++ _, Atom.to_charlist(mod)) ->
        if function_exported?(mod, :__RELATIVE__, 0) do
          "#Function<#{uniq(fun_info)} in file:#{mod.__RELATIVE__()}>"
        else
          default_inspect(mod, fun_info)
        end

      true ->
        default_inspect(mod, fun_info)
    end
  end

  defp default_inspect(mod, fun_info) do
    inspected_as_atom = Macro.inspect_atom(:literal, mod)
    extracted_name = extract_name(fun_info[:name])
    "#Function<#{uniq(fun_info)}/#{fun_info[:arity]} in #{inspected_as_atom}#{extracted_name}>"
  end

  defp extract_name([]) do
    ""
  end

  defp extract_name(name) do
    case Identifier.extract_anonymous_fun_parent(name) do
      {name, arity} ->
        "." <> Macro.inspect_atom(:remote_call, name) <> "/" <> arity

      :error ->
        "." <> Macro.inspect_atom(:remote_call, name)
    end
  end

  defp uniq(fun_info) do
    Integer.to_string(fun_info[:new_index]) <> "." <> Integer.to_string(fun_info[:uniq])
  end
end

defimpl Inspect, for: Inspect.Error do

  def inspect(%{stacktrace: stacktrace} = inspect_error, _opts) do
    message = Exception.message(inspect_error)
    format_output(message, stacktrace)
  end

  defp format_output(message, [_ | _] = stacktrace) do
    stacktrace = Exception.format_stacktrace(stacktrace)

    """
    #Inspect.Error<
    #{Inspect.Error.pad(message, 2)}

      Stacktrace:

    #{stacktrace}
    >\
    """
  end

  defp format_output(message, []) do
    """
    #Inspect.Error<
      #{Inspect.Error.pad(message, 2)}
    >\
    """
  end
end

defimpl Inspect, for: PID do
  def inspect(pid, _opts) do
    "#PID" <> IO.iodata_to_binary(:erlang.pid_to_list(pid))
  end
end

defimpl Inspect, for: Port do
  def inspect(port, _opts) do
    IO.iodata_to_binary(:erlang.port_to_list(port))
  end
end

defimpl Inspect, for: Reference do
  def inspect(ref, _opts) do
    [?#, ?R, ?e, ?f] ++ rest = :erlang.ref_to_list(ref)
    "#Reference" <> IO.iodata_to_binary(rest)
  end
end

defimpl Inspect, for: Any do
  def inspect(%module{} = struct, opts) do
    try do
      {module.__struct__(), module.__info__(:struct)}
    rescue
      _ -> Inspect.Map.inspect(struct, opts)
    else
      {dunder, fields} ->
        if Map.keys(dunder) == Map.keys(struct) do
          {inspect_module, infos} = Tonic.Internal.inspect_infos(module, struct, dunder, fields)
          inspect_module.inspect(struct, Macro.inspect_atom(:literal, module), infos, opts)




        else
          Inspect.Map.inspect(struct, opts)
        end
    end
  end

  def inspect(map, name, infos, opts) do
    open = color_doc("#" <> name <> "<", :map, opts)
    sep = color_doc(",", :map, opts)
    close = color_doc(">", :map, opts)

    fun = fn
      %{field: field}, opts -> Inspect.List.keyword({field, Map.get(map, field)}, opts)
      :..., _opts -> "..."
    end

    container_doc(open, infos ++ [:...], close, opts, fun, separator: sep, break: :strict)
  end
end






















# Imported from Elixir 1.18.3 lib/elixir/lib/inspect.ex (docs and specs stripped;
# line numbers match the original).
