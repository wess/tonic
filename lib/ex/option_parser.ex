defmodule OptionParser do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.





































  defmodule ParseError do






    defexception [:message]
  end




















































































































































































































  def parse(argv, opts \\ []) when is_list(argv) and is_list(opts) do
    do_parse(argv, build_config(opts), [], [], [], true)
  end


































  def parse!(argv, opts \\ []) when is_list(argv) and is_list(opts) do
    case parse(argv, opts) do
      {parsed, args, []} -> {parsed, args}
      {_, _, errors} -> raise ParseError, format_errors(errors, opts)
    end
  end























  def parse_head(argv, opts \\ []) when is_list(argv) and is_list(opts) do
    do_parse(argv, build_config(opts), [], [], [], false)
  end



































  def parse_head!(argv, opts \\ []) when is_list(argv) and is_list(opts) do
    case parse_head(argv, opts) do
      {parsed, args, []} -> {parsed, args}
      {_, _, errors} -> raise ParseError, format_errors(errors, opts)
    end
  end

  defp do_parse([], _config, opts, args, invalid, _all?) do
    {Enum.reverse(opts), Enum.reverse(args), Enum.reverse(invalid)}
  end

  defp do_parse(argv, %{switches: switches} = config, opts, args, invalid, all?) do
    case next_with_config(argv, config) do
      {:ok, option, value, rest} ->
        # the option exists and it was successfully parsed
        kinds = List.wrap(Keyword.get(switches, option))
        new_opts = store_option(opts, option, value, kinds)
        do_parse(rest, config, new_opts, args, invalid, all?)

      {:invalid, option, value, rest} ->
        # the option exist but it has wrong value
        do_parse(rest, config, opts, args, [{option, value} | invalid], all?)

      {:undefined, option, _value, rest} ->
        invalid = if config.strict?, do: [{option, nil} | invalid], else: invalid
        do_parse(rest, config, opts, args, invalid, all?)

      {:error, ["--" | rest] = remaining_args} ->
        args =
          if config.return_separator? do
            Enum.reverse(args, remaining_args)
          else
            Enum.reverse(args, rest)
          end

        {Enum.reverse(opts), args, Enum.reverse(invalid)}

      {:error, [arg | rest] = remaining_args} ->
        # there is no option
        if all? do
          do_parse(rest, config, opts, [arg | args], invalid, all?)
        else
          {Enum.reverse(opts), Enum.reverse(args, remaining_args), Enum.reverse(invalid)}
        end
    end
  end


























  def next(argv, opts \\ []) when is_list(argv) and is_list(opts) do
    next_with_config(argv, build_config(opts))
  end

  defp next_with_config([], _config) do
    {:error, []}
  end

  defp next_with_config(["--" | _] = argv, _config) do
    {:error, argv}
  end

  defp next_with_config(["-" | _] = argv, _config) do
    {:error, argv}
  end

  defp next_with_config(["- " <> _ | _] = argv, _config) do
    {:error, argv}
  end

  # Handles --foo or --foo=bar
  defp next_with_config(["--" <> option | rest], config) do
    {option, value} = split_option(option)

    if String.contains?(option, ["_"]) do
      {:undefined, "--" <> option, value, rest}
    else
      tagged = tag_option(option, config)
      next_tagged(tagged, value, "--" <> option, rest, config)
    end
  end

  # Handles -a, -abc, -abc=something, -n2
  defp next_with_config(["-" <> option | rest] = argv, config) do
    {option, value} = split_option(option)
    original = "-" <> option

    cond do
      is_nil(value) and starts_with_number?(option) ->
        {:error, argv}

      String.contains?(option, ["-", "_"]) ->
        {:undefined, original, value, rest}

      String.length(option) == 1 ->
        # We have a regular one-letter alias here
        tagged = tag_oneletter_alias(option, config)
        next_tagged(tagged, value, original, rest, config)

      true ->
        key = get_option_key(option, config.allow_nonexistent_atoms?)
        option_key = config.aliases[key]

        if key && option_key do
          IO.warn("multi-letter aliases are deprecated, got: #{inspect(key)}")
          next_tagged({:default, option_key}, value, original, rest, config)
        else
          next_with_config(expand_multiletter_alias(option, value) ++ rest, config)
        end
    end
  end

  defp next_with_config(argv, _config) do
    {:error, argv}
  end

  defp next_tagged(:unknown, value, original, rest, _) do
    {value, _kinds, rest} = normalize_value(value, [], rest)
    {:undefined, original, value, rest}
  end

  defp next_tagged({tag, option}, value, original, rest, %{switches: switches, strict?: strict?}) do
    if strict? and not Keyword.has_key?(switches, option) do
      {:undefined, original, value, rest}
    else
      {kinds, value} = normalize_tag(tag, option, value, switches)
      {value, kinds, rest} = normalize_value(value, kinds, rest)

      case validate_option(value, kinds) do
        {:ok, new_value} -> {:ok, option, new_value, rest}
        :invalid -> {:invalid, original, value, rest}
      end
    end
  end






























  def to_argv(enum, options \\ []) do
    switches = Keyword.get(options, :switches, [])

    Enum.flat_map(enum, fn
      {_key, nil} -> []
      {key, true} -> [to_switch(key)]
      {key, false} -> [to_switch(key, "--no-")]
      {key, value} -> to_argv(key, value, switches)
    end)
  end

  defp to_argv(key, value, switches) do
    if switches[key] == :count do
      List.duplicate(to_switch(key), value)
    else
      [to_switch(key), to_string(value)]
    end
  end

  defp to_switch(key, prefix \\ "--") when is_atom(key) do
    prefix <> String.replace(Atom.to_string(key), "_", "-")
  end

















  def split(string) when is_binary(string) do
    do_split(String.trim_leading(string, " "), "", [], nil)
  end

  # If we have an escaped quote, simply remove the escape
  defp do_split(<<?\\, quote, t::binary>>, buffer, acc, quote),
    do: do_split(t, <<buffer::binary, quote>>, acc, quote)

  # If we have a quote and we were not in a quote, start one
  defp do_split(<<quote, t::binary>>, buffer, acc, nil) when quote in [?", ?'],
    do: do_split(t, buffer, acc, quote)

  # If we have a quote and we were inside it, close it
  defp do_split(<<quote, t::binary>>, buffer, acc, quote), do: do_split(t, buffer, acc, nil)

  # If we have an escaped quote/space, simply remove the escape as long as we are not inside a quote
  defp do_split(<<?\\, h, t::binary>>, buffer, acc, nil) when h in [?\s, ?', ?"],
    do: do_split(t, <<buffer::binary, h>>, acc, nil)

  # If we have space and we are outside of a quote, start new segment
  defp do_split(<<?\s, t::binary>>, buffer, acc, nil),
    do: do_split(String.trim_leading(t, " "), "", [buffer | acc], nil)

  # All other characters are moved to buffer
  defp do_split(<<h, t::binary>>, buffer, acc, quote) do
    do_split(t, <<buffer::binary, h>>, acc, quote)
  end

  # Finish the string expecting a nil marker
  defp do_split(<<>>, "", acc, nil), do: Enum.reverse(acc)

  defp do_split(<<>>, buffer, acc, nil), do: Enum.reverse([buffer | acc])

  # Otherwise raise
  defp do_split(<<>>, _, _acc, marker) do
    raise "argv string did not terminate properly, a #{<<marker>>} was opened but never closed"
  end

  ## Helpers

  defp build_config(opts) do
    {switches, strict?} =
      cond do
        opts[:switches] && opts[:strict] ->
          raise ArgumentError, ":switches and :strict cannot be given together"

        switches = opts[:switches] ->
          validate_switches(switches)
          {switches, false}

        strict = opts[:strict] ->
          validate_switches(strict)
          {strict, true}

        true ->
          IO.warn("not passing the :switches or :strict option to OptionParser is deprecated")
          {[], false}
      end

    %{
      aliases: opts[:aliases] || [],
      allow_nonexistent_atoms?: opts[:allow_nonexistent_atoms] || false,
      return_separator?: opts[:return_separator] || false,
      strict?: strict?,
      switches: switches
    }
  end

  defp validate_switches(switches) do
    Enum.map(switches, &validate_switch/1)
  end

  defp validate_switch({_name, type_or_type_and_modifiers}) do
    valid = [:boolean, :count, :integer, :float, :string, :keep]
    invalid = List.wrap(type_or_type_and_modifiers) -- valid

    if invalid != [] do
      raise ArgumentError,
            "invalid switch types/modifiers: " <> Enum.map_join(invalid, ", ", &inspect/1)
    end
  end

  defp validate_option(value, kinds) do
    {invalid?, value} =
      cond do
        :invalid in kinds ->
          {true, value}

        :boolean in kinds ->
          case value do
            t when t in [true, "true"] -> {false, true}
            f when f in [false, "false"] -> {false, false}
            _ -> {true, value}
          end

        :count in kinds ->
          case value do
            nil -> {false, 1}
            _ -> {true, value}
          end

        :integer in kinds ->
          case Integer.parse(value) do
            {value, ""} -> {false, value}
            _ -> {true, value}
          end

        :float in kinds ->
          case Float.parse(value) do
            {value, ""} -> {false, value}
            _ -> {true, value}
          end

        true ->
          {false, value}
      end

    if invalid? do
      :invalid
    else
      {:ok, value}
    end
  end

  defp store_option(dict, option, value, kinds) do
    cond do
      :count in kinds ->
        Keyword.update(dict, option, value, &(&1 + 1))

      :keep in kinds ->
        [{option, value} | dict]

      true ->
        [{option, value} | Keyword.delete(dict, option)]
    end
  end

  defp tag_option("no-" <> option = original, config) do
    %{switches: switches, allow_nonexistent_atoms?: allow_nonexistent_atoms?} = config

    cond do
      (negated = get_option_key(option, allow_nonexistent_atoms?)) &&
          :boolean in List.wrap(switches[negated]) ->
        {:negated, negated}

      option_key = get_option_key(original, allow_nonexistent_atoms?) ->
        {:default, option_key}

      true ->
        :unknown
    end
  end

  defp tag_option(option, config) do
    %{allow_nonexistent_atoms?: allow_nonexistent_atoms?} = config

    if option_key = get_option_key(option, allow_nonexistent_atoms?) do
      {:default, option_key}
    else
      :unknown
    end
  end

  defp tag_oneletter_alias(alias, config) when is_binary(alias) do
    %{aliases: aliases, allow_nonexistent_atoms?: allow_nonexistent_atoms?} = config

    if option_key = aliases[to_existing_key(alias, allow_nonexistent_atoms?)] do
      {:default, option_key}
    else
      :unknown
    end
  end

  defp expand_multiletter_alias(options, value) do
    {options, maybe_integer} =
      options
      |> String.to_charlist()
      |> Enum.split_while(&(&1 not in ?0..?9))

    {last, expanded} =
      options
      |> List.to_string()
      |> String.graphemes()
      |> Enum.map(&("-" <> &1))
      |> List.pop_at(-1)

    expanded ++
      [
        last <>
          if(maybe_integer != [], do: "=#{maybe_integer}", else: "") <>
          if(value, do: "=#{value}", else: "")
      ]
  end

  defp normalize_tag(:negated, option, value, switches) do
    if value do
      {[:invalid], value}
    else
      {List.wrap(switches[option]), false}
    end
  end

  defp normalize_tag(:default, option, value, switches) do
    {List.wrap(switches[option]), value}
  end

  defp normalize_value(nil, kinds, t) do
    cond do
      :boolean in kinds ->
        {true, kinds, t}

      :count in kinds ->
        {nil, kinds, t}

      value_in_tail?(t) ->
        [h | t] = t
        {h, kinds, t}

      kinds == [] ->
        {true, kinds, t}

      true ->
        {nil, [:invalid], t}
    end
  end

  defp normalize_value(value, kinds, t) do
    {value, kinds, t}
  end

  defp value_in_tail?(["-" | _]), do: true
  defp value_in_tail?(["- " <> _ | _]), do: true
  defp value_in_tail?(["-" <> arg | _]), do: starts_with_number?(arg)
  defp value_in_tail?([]), do: false
  defp value_in_tail?(_), do: true

  defp split_option(option) do
    case :binary.split(option, "=") do
      [h] -> {h, nil}
      [h, t] -> {h, t}
    end
  end

  defp to_underscore(option), do: to_underscore(option, <<>>)
  defp to_underscore("-" <> rest, acc), do: to_underscore(rest, acc <> "_")
  defp to_underscore(<<c>> <> rest, acc), do: to_underscore(rest, <<acc::binary, c>>)
  defp to_underscore(<<>>, acc), do: acc

  defp get_option_key(option, allow_nonexistent_atoms?) do
    option
    |> to_underscore()
    |> to_existing_key(allow_nonexistent_atoms?)
  end

  defp to_existing_key(option, true), do: String.to_atom(option)

  defp to_existing_key(option, false) do
    try do
      String.to_existing_atom(option)
    rescue
      ArgumentError -> nil
    end
  end

  defp starts_with_number?(<<char, _::binary>>) when char in ?0..?9, do: true
  defp starts_with_number?(_), do: false

  defp format_errors([_ | _] = errors, opts) do
    types = opts[:switches] || opts[:strict]
    error_count = length(errors)
    error = if error_count == 1, do: "error", else: "errors"

    "#{error_count} #{error} found!\n" <>
      Enum.map_join(errors, "\n", &format_error(&1, opts, types))
  end

  defp format_error({option, nil}, opts, types) do
    if type = get_type(option, opts, types) do
      if String.contains?(option, "_") do
        msg = "#{option} : Unknown option"

        msg <> ". Did you mean #{String.replace(option, "_", "-")}?"
      else
        "#{option} : Missing argument of type #{type}"
      end
    else
      msg = "#{option} : Unknown option"

      case did_you_mean(option, types) do
        {similar, score} when score > 0.8 ->
          msg <> ". Did you mean --#{similar}?"

        _ ->
          msg
      end
    end
  end

  defp format_error({option, value}, opts, types) do
    type = get_type(option, opts, types)
    "#{option} : Expected type #{type}, got #{inspect(value)}"
  end

  defp get_type(option, opts, types) do
    allow_nonexistent_atoms? = opts[:allow_nonexistent_atoms] || false
    key = option |> String.trim_leading("-") |> get_option_key(allow_nonexistent_atoms?)

    if option_key = opts[:aliases][key] do
      types[option_key]
    else
      types[key]
    end
  end

  defp did_you_mean(option, types) do
    key = option |> String.trim_leading("-") |> String.replace("-", "_")
    Enum.reduce(types, {nil, 0}, &max_similar(&1, key, &2))
  end

  defp max_similar({source, _}, target, {_, current} = best) do
    source = Atom.to_string(source)

    score = String.jaro_distance(source, target)
    option = String.replace(source, "_", "-")
    if score < current, do: best, else: {option, score}
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/option_parser.ex (docs and specs stripped;
# line numbers match the original).
