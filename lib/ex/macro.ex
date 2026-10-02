import Kernel, except: [to_string: 1]
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
defmodule Macro do






























































  alias Code.Identifier


























































































































































  def unpipe(expr) do
    :lists.reverse(unpipe(expr, []))
  end

  defp unpipe({:|>, _, [left, right]}, acc) do
    unpipe(right, unpipe(left, acc))
  end

  defp unpipe(other, acc) do
    [{other, 0} | acc]
  end





















  def pipe(expr, call_args, position)

  def pipe(expr, {:&, _, _} = call_args, _integer) do
    raise ArgumentError, bad_pipe(expr, call_args)
  end

  def pipe(expr, {tuple_or_map, _, _} = call_args, _integer) when tuple_or_map in [:{}, :%{}] do
    raise ArgumentError, bad_pipe(expr, call_args)
  end

  # Without this, `Macro |> Env == Macro.Env`.
  def pipe(expr, {:__aliases__, _, _} = call_args, _integer) do
    raise ArgumentError, bad_pipe(expr, call_args)
  end

  def pipe(expr, {:<<>>, _, _} = call_args, _integer) do
    raise ArgumentError, bad_pipe(expr, call_args)
  end

  def pipe(expr, {unquote, _, []}, _integer) when unquote in [:unquote, :unquote_splicing] do
    raise ArgumentError,
          "cannot pipe #{to_string(expr)} into the special form #{unquote}/1 " <>
            "since #{unquote}/1 is used to build the Elixir AST itself"
  end

  # {:fn, _, _} is what we get when we pipe into an anonymous function without
  # calling it, for example, `:foo |> (fn x -> x end)`.
  def pipe(expr, {:fn, _, _}, _integer) do
    raise ArgumentError,
          "cannot pipe #{to_string(expr)} into an anonymous function without" <>
            " calling the function; use Kernel.then/2 instead or" <>
            " define the anonymous function as a regular private function"
  end

  def pipe(expr, {call, line, atom}, integer) when is_atom(atom) do
    {call, line, List.insert_at([], integer, expr)}
  end

  def pipe(_expr, {op, _line, [arg]}, _integer) when op == :+ or op == :- do
    raise ArgumentError,
          "piping into a unary operator is not supported, please use the qualified name: " <>
            "Kernel.#{op}(#{to_string(arg)}), instead of #{op}#{to_string(arg)}"
  end

  # Piping to an Access.get/2,3 call in the form of brackets
  # (foo |> bar[]) raises a nice error.
  def pipe(
        expr,
        {{_, meta, [Access, :get] = op}, _meta, [first, second]} = _op_args,
        integer
      ) do
    if {:from_brackets, true} in meta do
      raise ArgumentError, """
      wrong operator precedence when piping into bracket-based access

       Instead of:

           #{to_string(expr)} |> #{to_string(first)}[#{to_string(second)}]

       You should write:

           (#{to_string(expr)} |> #{to_string(first)})[#{to_string(second)}]
      """
    else
      {op, meta, List.insert_at([first, second], integer, expr)}
    end
  end

  def pipe(expr, {op, line, args} = op_args, integer) when is_list(args) do
    cond do
      is_atom(op) and operator?(op, 1) ->
        raise ArgumentError,
              "cannot pipe #{to_string(expr)} into #{to_string(op_args)}, " <>
                "the #{to_string(op)} operator can only take one argument"

      is_atom(op) and operator?(op, 2) ->
        raise ArgumentError,
              "cannot pipe #{to_string(expr)} into #{to_string(op_args)}, " <>
                "the #{to_string(op)} operator can only take two arguments"

      true ->
        {op, line, List.insert_at(args, integer, expr)}
    end
  end

  def pipe(expr, call_args, _integer) do
    raise ArgumentError, bad_pipe(expr, call_args)
  end

  defp bad_pipe(expr, call_args) do
    "cannot pipe #{to_string(expr)} into #{to_string(call_args)}, " <>
      "can only pipe into local calls foo(), remote calls Foo.bar() or anonymous function calls foo.()"
  end

















  def update_meta(quoted, fun)

  def update_meta({left, meta, right}, fun) when is_list(meta) do
    {left, fun.(meta), right}
  end

  def update_meta(other, _fun) do
    other
  end



















  def generate_arguments(amount, context), do: generate_arguments(amount, context, &var/2)




































  def path(ast, fun) when is_function(fun, 1) do
    path(ast, [], fun)
  end

  defp path({form, _, args} = ast, acc, fun) when is_atom(form) do
    acc = [ast | acc]

    if fun.(ast) do
      acc
    else
      path_args(args, acc, fun)
    end
  end

  defp path({form, _meta, args} = ast, acc, fun) do
    acc = [ast | acc]

    if fun.(ast) do
      acc
    else
      path(form, acc, fun) || path_args(args, acc, fun)
    end
  end

  defp path({left, right} = ast, acc, fun) do
    acc = [ast | acc]

    if fun.(ast) do
      acc
    else
      path(left, acc, fun) || path(right, acc, fun)
    end
  end

  defp path(list, acc, fun) when is_list(list) do
    acc = [list | acc]

    if fun.(list) do
      acc
    else
      path_list(list, acc, fun)
    end
  end

  defp path(ast, acc, fun) do
    if fun.(ast) do
      [ast | acc]
    end
  end

  defp path_args(atom, _acc, _fun) when is_atom(atom), do: nil
  defp path_args(list, acc, fun) when is_list(list), do: path_list(list, acc, fun)

  defp path_list([], _acc, _fun) do
    nil
  end

  defp path_list([arg | args], acc, fun) do
    path(arg, acc, fun) || path_list(args, acc, fun)
  end



















  def generate_unique_arguments(amount, context),
    do: generate_arguments(amount, context, &unique_var/2)

  defp generate_arguments(0, context, _fun) when is_atom(context), do: []

  defp generate_arguments(amount, context, fun)
       when is_integer(amount) and amount > 0 and is_atom(context) do
    for id <- 1..amount, do: fun.(String.to_atom("arg" <> Integer.to_string(id)), context)
  end



























  def var(var, context) when is_atom(var) and is_atom(context) do
    {var, [], context}
  end



















  def unique_var(var, context) when is_atom(var) and is_atom(context) do
    {var, [counter: :elixir_module.next_counter(context)], context}
  end




































  def traverse(ast, acc, pre, post) when is_function(pre, 2) and is_function(post, 2) do
    {ast, acc} = pre.(ast, acc)
    do_traverse(ast, acc, pre, post)
  end

  defp do_traverse({form, meta, args}, acc, pre, post) when is_atom(form) do
    {args, acc} = do_traverse_args(args, acc, pre, post)
    post.({form, meta, args}, acc)
  end

  defp do_traverse({form, meta, args}, acc, pre, post) do
    {form, acc} = pre.(form, acc)
    {form, acc} = do_traverse(form, acc, pre, post)
    {args, acc} = do_traverse_args(args, acc, pre, post)
    post.({form, meta, args}, acc)
  end

  defp do_traverse({left, right}, acc, pre, post) do
    {left, acc} = pre.(left, acc)
    {left, acc} = do_traverse(left, acc, pre, post)
    {right, acc} = pre.(right, acc)
    {right, acc} = do_traverse(right, acc, pre, post)
    post.({left, right}, acc)
  end

  defp do_traverse(list, acc, pre, post) when is_list(list) do
    {list, acc} = do_traverse_args(list, acc, pre, post)
    post.(list, acc)
  end

  defp do_traverse(x, acc, _pre, post) do
    post.(x, acc)
  end

  defp do_traverse_args(args, acc, _pre, _post) when is_atom(args) do
    {args, acc}
  end

  defp do_traverse_args(args, acc, pre, post) when is_list(args) do
    :lists.mapfoldl(
      fn x, acc ->
        {x, acc} = pre.(x, acc)
        do_traverse(x, acc, pre, post)
      end,
      acc,
      args
    )
  end
























  def prewalk(ast, fun) when is_function(fun, 1) do
    elem(prewalk(ast, nil, fn x, nil -> {fun.(x), nil} end), 0)
  end


























  def prewalk(ast, acc, fun) when is_function(fun, 2) do
    traverse(ast, acc, fun, fn x, a -> {x, a} end)
  end






  def postwalk(ast, fun) when is_function(fun, 1) do
    elem(postwalk(ast, nil, fn x, nil -> {fun.(x), nil} end), 0)
  end






  def postwalk(ast, acc, fun) when is_function(fun, 2) do
    traverse(ast, acc, fn x, a -> {x, a} end, fun)
  end





























  def decompose_call(ast)

  def decompose_call({:{}, _, args}) when is_list(args), do: :error

  def decompose_call({{:., _, [remote, function]}, _, args})
      when is_tuple(remote) or is_atom(remote),
      do: {remote, function, args}

  def decompose_call({name, _, args}) when is_atom(name) and is_atom(args), do: {name, []}

  def decompose_call({name, _, args}) when is_atom(name) and is_list(args), do: {name, args}

  def decompose_call(_), do: :error































































  def escape(expr, opts \\ []) do
    unquote = Keyword.get(opts, :unquote, false)
    kind = if Keyword.get(opts, :prune_metadata, false), do: :prune_metadata, else: :none
    :elixir_quote.escape(expr, kind, unquote)
  end

  # TODO: Deprecate me on Elixir v1.22

  def struct!(module, env) when is_atom(module) do
    pairs =
      for %{field: field, default: default} <- struct_info!(module, env), do: {field, default}

    :maps.from_list([__struct__: module] ++ pairs)
  end

















  def struct_info!(module, env) when is_atom(module) do
    case :elixir_map.maybe_load_struct_info([line: env.line], module, [], true, env) do
      {:ok, info} -> info
      {:error, desc} -> raise ArgumentError, List.to_string(:elixir_map.format_error(desc))
    end
  end

























  def validate(expr) do
    find_invalid(expr) || :ok
  end

  defp find_invalid({left, right}), do: find_invalid(left) || find_invalid(right)

  defp find_invalid({left, meta, right})
       when is_list(meta) and (is_atom(right) or is_list(right)),
       do: find_invalid(left) || find_invalid(right)

  defp find_invalid(list) when is_list(list), do: Enum.find_value(list, &find_invalid/1)

  defp find_invalid(pid) when is_pid(pid), do: nil
  defp find_invalid(atom) when is_atom(atom), do: nil
  defp find_invalid(num) when is_number(num), do: nil
  defp find_invalid(bin) when is_binary(bin), do: nil

  defp find_invalid(fun) when is_function(fun) do
    if Function.info(fun, :env) != {:env, []} or
         Function.info(fun, :type) != {:type, :external} do
      {:error, fun}
    end
  end

  defp find_invalid(other), do: {:error, other}














  def prewalker(ast) do
    &prewalker([ast], &1, &2)
  end

  defp prewalker(_buffer, {:halt, acc}, _fun) do
    {:halted, acc}
  end

  defp prewalker(buffer, {:suspend, acc}, fun) do
    {:suspended, acc, &prewalker(buffer, &1, fun)}
  end

  defp prewalker([], {:cont, acc}, _fun) do
    {:done, acc}
  end

  defp prewalker([{left, right} = node | tail], {:cont, acc}, fun) do
    prewalker([left, right | tail], fun.(node, acc), fun)
  end

  defp prewalker([{left, meta, right} = node | tail], {:cont, acc}, fun)
       when is_atom(left) and is_list(meta) do
    if is_atom(right) do
      prewalker(tail, fun.(node, acc), fun)
    else
      prewalker(right ++ tail, fun.(node, acc), fun)
    end
  end

  defp prewalker([{left, meta, right} = node | tail], {:cont, acc}, fun) when is_list(meta) do
    if is_atom(right) do
      prewalker([left | tail], fun.(node, acc), fun)
    else
      prewalker([left | right] ++ tail, fun.(node, acc), fun)
    end
  end

  defp prewalker([list | tail], {:cont, acc}, fun) when is_list(list) do
    prewalker(list ++ tail, fun.(list, acc), fun)
  end

  defp prewalker([head | tail], {:cont, acc}, fun) do
    prewalker(tail, fun.(head, acc), fun)
  end














  def postwalker(ast) do
    &postwalker([ast], make_ref(), &1, &2)
  end

  defp postwalker(_buffer, _ref, {:halt, acc}, _fun) do
    {:halted, acc}
  end

  defp postwalker(buffer, ref, {:suspend, acc}, fun) do
    {:suspended, acc, &postwalker(buffer, ref, &1, fun)}
  end

  defp postwalker([], _ref, {:cont, acc}, _fun) do
    {:done, acc}
  end

  defp postwalker([{ref, head} | tail], ref, {:cont, acc}, fun) do
    postwalker(tail, ref, fun.(head, acc), fun)
  end

  defp postwalker([{left, right} = node | tail], ref, {:cont, acc}, fun) do
    postwalker([right, {ref, node} | tail], ref, fun.(left, acc), fun)
  end

  defp postwalker([{left, meta, right} = node | tail], ref, {:cont, acc}, fun)
       when is_atom(left) and is_list(meta) do
    if is_atom(right) do
      postwalker(tail, ref, fun.(node, acc), fun)
    else
      postwalker(right ++ [{ref, node} | tail], ref, {:cont, acc}, fun)
    end
  end

  defp postwalker([{left, meta, right} = node | tail], ref, cont_acc, fun)
       when is_list(meta) do
    if is_atom(right) do
      postwalker([left, {ref, node} | tail], ref, cont_acc, fun)
    else
      postwalker([left | right] ++ [{ref, node} | tail], ref, cont_acc, fun)
    end
  end

  defp postwalker([list | tail], ref, cont_acc, fun) when is_list(list) do
    postwalker(list ++ [{ref, list} | tail], ref, cont_acc, fun)
  end

  defp postwalker([head | tail], ref, {:cont, acc}, fun) do
    postwalker(tail, ref, fun.(head, acc), fun)
  end



























  def unescape_string(string) do
    :elixir_interpolation.unescape_string(string)
  end











































  def unescape_string(string, map) do
    :elixir_interpolation.unescape_string(string, map)
  end



  def unescape_tokens(tokens) do
    for token <- tokens do
      if is_binary(token), do: unescape_string(token), else: token
    end
  end



  def unescape_tokens(tokens, map) do
    for token <- tokens do
      if is_binary(token), do: unescape_string(token, map), else: token
    end
  end





















  def to_string(tree) do
    doc =
      Inspect.Algebra.format(Code.quoted_to_algebra(tree, migrate_charlists_as_sigils: true), 98)

    IO.iodata_to_binary(doc)
  end























  def to_string(tree, fun)

  # Variables
  def to_string({var, _, context} = ast, fun) when is_atom(var) and is_atom(context) do
    fun.(ast, Atom.to_string(var))
  end

  # Aliases
  def to_string({:__aliases__, _, refs} = ast, fun) do
    fun.(ast, Enum.map_join(refs, ".", &call_to_string(&1, fun)))
  end

  # Blocks
  def to_string({:__block__, _, [expr]} = ast, fun) do
    fun.(ast, to_string(expr, fun))
  end

  def to_string({:__block__, _, _} = ast, fun) do
    block = adjust_new_lines(block_to_string(ast, fun), "\n  ")
    fun.(ast, "(\n  " <> block <> "\n)")
  end

  # Bits containers
  def to_string({:<<>>, _, parts} = ast, fun) do
    if interpolated?(ast) do
      fun.(ast, interpolate(ast, fun))
    else
      result =
        Enum.map_join(parts, ", ", fn part ->
          str = bitpart_to_string(part, fun)

          if :binary.first(str) == ?< or :binary.last(str) == ?> do
            "(" <> str <> ")"
          else
            str
          end
        end)

      fun.(ast, "<<" <> result <> ">>")
    end
  end

  # Tuple containers
  def to_string({:{}, _, args} = ast, fun) do
    tuple = "{" <> Enum.map_join(args, ", ", &to_string(&1, fun)) <> "}"
    fun.(ast, tuple)
  end

  # Map containers
  def to_string({:%{}, _, args} = ast, fun) do
    map = "%{" <> map_to_string(args, fun) <> "}"
    fun.(ast, map)
  end

  def to_string({:%, _, [struct_name, map]} = ast, fun) do
    {:%{}, _, args} = map
    struct = "%" <> to_string(struct_name, fun) <> "{" <> map_to_string(args, fun) <> "}"
    fun.(ast, struct)
  end

  # Fn keyword
  def to_string({:fn, _, [{:->, _, [_, tuple]}] = arrow} = ast, fun)
      when not is_tuple(tuple) or elem(tuple, 0) != :__block__ do
    fun.(ast, "fn " <> arrow_to_string(arrow, fun) <> " end")
  end

  def to_string({:fn, _, [{:->, _, _}] = block} = ast, fun) do
    fun.(ast, "fn " <> block_to_string(block, fun) <> "\nend")
  end

  def to_string({:fn, _, block} = ast, fun) do
    block = adjust_new_lines(block_to_string(block, fun), "\n  ")
    fun.(ast, "fn\n  " <> block <> "\nend")
  end

  # left -> right
  def to_string([{:->, _, _} | _] = ast, fun) do
    fun.(ast, "(" <> arrow_to_string(ast, fun, true) <> ")")
  end

  # left when right
  def to_string({:when, _, [left, right]} = ast, fun) do
    right =
      if right != [] and Keyword.keyword?(right) do
        kw_list_to_string(right, fun)
      else
        fun.(ast, op_to_string(right, fun, :when, :right))
      end

    fun.(ast, op_to_string(left, fun, :when, :left) <> " when " <> right)
  end

  # Splat when
  def to_string({:when, _, args} = ast, fun) do
    {left, right} = split_last(args)

    result =
      "(" <> Enum.map_join(left, ", ", &to_string(&1, fun)) <> ") when " <> to_string(right, fun)

    fun.(ast, result)
  end

  # Capture
  def to_string({:&, _, [{:/, _, [{name, _, ctx}, arity]}]} = ast, fun)
      when is_atom(name) and is_atom(ctx) and is_integer(arity) do
    result = "&" <> Atom.to_string(name) <> "/" <> to_string(arity, fun)
    fun.(ast, result)
  end

  def to_string({:&, _, [{:/, _, [{{:., _, [mod, name]}, _, []}, arity]}]} = ast, fun)
      when is_atom(name) and is_integer(arity) do
    result =
      "&" <> to_string(mod, fun) <> "." <> Atom.to_string(name) <> "/" <> to_string(arity, fun)

    fun.(ast, result)
  end

  def to_string({:&, _, [arg]} = ast, fun) when not is_integer(arg) do
    fun.(ast, "&(" <> to_string(arg, fun) <> ")")
  end

  # left not in right
  def to_string({:not, _, [{:in, _, [left, right]}]} = ast, fun) do
    fun.(ast, to_string(left, fun) <> " not in " <> to_string(right, fun))
  end

  # Access
  def to_string({{:., _, [Access, :get]}, _, [left, right]} = ast, fun) do
    if op_expr?(left) do
      fun.(ast, "(" <> to_string(left, fun) <> ")" <> to_string([right], fun))
    else
      fun.(ast, to_string(left, fun) <> to_string([right], fun))
    end
  end

  # foo.{bar, baz}
  def to_string({{:., _, [left, :{}]}, _, args} = ast, fun) do
    fun.(ast, to_string(left, fun) <> ".{" <> args_to_string(args, fun) <> "}")
  end

  # All other calls
  def to_string({{:., _, [left, _]} = target, meta, []} = ast, fun) do
    to_string = call_to_string(target, fun)

    if is_tuple(left) && meta[:no_parens] do
      fun.(ast, to_string)
    else
      fun.(ast, to_string <> "()")
    end
  end

  def to_string({target, _, args} = ast, fun) when is_list(args) do
    with :error <- unary_call(ast, fun),
         :error <- op_call(ast, fun),
         :error <- sigil_call(ast, fun) do
      {list, last} = split_last(args)

      result =
        if kw_blocks?(last) do
          case list do
            [] -> call_to_string(target, fun) <> kw_blocks_to_string(last, fun)
            _ -> call_to_string_with_args(target, list, fun) <> kw_blocks_to_string(last, fun)
          end
        else
          call_to_string_with_args(target, args, fun)
        end

      fun.(ast, result)
    else
      {:ok, value} -> value
    end
  end

  # Two-element tuples
  def to_string({left, right}, fun) do
    to_string({:{}, [], [left, right]}, fun)
  end

  # Lists
  def to_string(list, fun) when is_list(list) do
    result =
      cond do
        list == [] ->
          "[]"

        :io_lib.printable_list(list) ->
          {escaped, _} = Identifier.escape(IO.chardata_to_string(list), ?")
          IO.iodata_to_binary([?~, ?c, ?", escaped, ?"])

        Inspect.List.keyword?(list) ->
          "[" <> kw_list_to_string(list, fun) <> "]"

        true ->
          "[" <> Enum.map_join(list, ", ", &to_string(&1, fun)) <> "]"
      end

    fun.(list, result)
  end

  # All other structures
  def to_string(other, fun) do
    fun.(other, inspect_no_limit(other))
  end

  defp inspect_no_limit(value) do
    Kernel.inspect(value, limit: :infinity, printable_limit: :infinity)
  end

  defp bitpart_to_string({:"::", meta, [left, right]} = ast, fun) do
    result =
      if meta[:inferred_bitstring_spec] do
        to_string(left, fun)
      else
        op_to_string(left, fun, :"::", :left) <>
          "::" <> bitmods_to_string(right, fun, :"::", :right)
      end

    fun.(ast, result)
  end

  defp bitpart_to_string(ast, fun) do
    to_string(ast, fun)
  end

  defp bitmods_to_string({op, _, [left, right]} = ast, fun, _, _) when op in [:*, :-] do
    result =
      bitmods_to_string(left, fun, op, :left) <>
        Atom.to_string(op) <> bitmods_to_string(right, fun, op, :right)

    fun.(ast, result)
  end

  defp bitmods_to_string(other, fun, parent_op, side) do
    op_to_string(other, fun, parent_op, side)
  end

  # Block keywords
  kw_keywords = [:do, :rescue, :catch, :else, :after]

  defp kw_blocks?([{:do, _} | _] = kw) do
    Enum.all?(kw, &match?({x, _} when x in unquote(kw_keywords), &1))
  end

  defp kw_blocks?(_), do: false

  # Check if we have an interpolated string.
  defp interpolated?({:<<>>, _, [_ | _] = parts}) do
    Enum.all?(parts, fn
      {:"::", _, [{{:., _, [Kernel, :to_string]}, _, [_]}, {:binary, _, _}]} -> true
      binary when is_binary(binary) -> true
      _ -> false
    end)
  end

  defp interpolated?(_) do
    false
  end

  defp interpolate(ast, fun), do: interpolate(ast, "\"", "\"", fun)

  defp interpolate({:<<>>, _, [parts]}, left, right, _) when left in [~s["""\n], ~s['''\n]] do
    <<left::binary, parts::binary, right::binary>>
  end

  defp interpolate({:<<>>, _, parts}, left, right, fun) do
    parts =
      Enum.map_join(parts, "", fn
        {:"::", _, [{{:., _, [Kernel, :to_string]}, _, [arg]}, {:binary, _, _}]} ->
          "\#{" <> to_string(arg, fun) <> "}"

        binary when is_binary(binary) ->
          escape_sigil(binary, left)
      end)

    <<left::binary, parts::binary, right::binary>>
  end

  defp escape_sigil(parts, "("), do: String.replace(parts, ")", ~S"\)")
  defp escape_sigil(parts, "{"), do: String.replace(parts, "}", ~S"\}")
  defp escape_sigil(parts, "["), do: String.replace(parts, "]", ~S"\]")
  defp escape_sigil(parts, "<"), do: String.replace(parts, ">", ~S"\>")
  defp escape_sigil(parts, delimiter), do: String.replace(parts, delimiter, "\\#{delimiter}")

  defp module_to_string(atom, _fun) when is_atom(atom) do
    inspect_no_limit(atom)
  end

  defp module_to_string({:&, _, [val]} = expr, fun) when not is_integer(val) do
    "(" <> to_string(expr, fun) <> ")"
  end

  defp module_to_string({:fn, _, _} = expr, fun) do
    "(" <> to_string(expr, fun) <> ")"
  end

  defp module_to_string({_, _, [_ | _] = args} = expr, fun) do
    if kw_blocks?(List.last(args)) do
      "(" <> to_string(expr, fun) <> ")"
    else
      to_string(expr, fun)
    end
  end

  defp module_to_string(expr, fun) do
    to_string(expr, fun)
  end

  defp unary_call({op, _, [arg]} = ast, fun) when is_atom(op) do
    if operator?(op, 1) do
      if op == :not or op_expr?(arg) do
        {:ok, fun.(ast, Atom.to_string(op) <> "(" <> to_string(arg, fun) <> ")")}
      else
        {:ok, fun.(ast, Atom.to_string(op) <> to_string(arg, fun))}
      end
    else
      :error
    end
  end

  defp unary_call(_, _) do
    :error
  end

  defp op_call({:..//, _, [left, middle, right]} = ast, fun) do
    left = op_to_string(left, fun, :.., :left)
    middle = op_to_string(middle, fun, :.., :right)
    right = op_to_string(right, fun, :"//", :right)
    {:ok, fun.(ast, left <> ".." <> middle <> "//" <> right)}
  end

  defp op_call({op, _, [left, right]} = ast, fun) when is_atom(op) do
    if operator?(op, 2) do
      left = op_to_string(left, fun, op, :left)
      right = op_to_string(right, fun, op, :right)
      op = if op in [:..], do: "#{op}", else: " #{op} "
      {:ok, fun.(ast, left <> op <> right)}
    else
      :error
    end
  end

  defp op_call(_, _) do
    :error
  end

  defp sigil_call({sigil, meta, [{:<<>>, _, _} = parts, args]} = ast, fun)
       when is_atom(sigil) and is_list(args) do
    delimiter = Keyword.get(meta, :delimiter, "\"")
    {left, right} = delimiter_pair(delimiter)

    case Atom.to_string(sigil) do
      <<"sigil_", first, rest::binary>> when first >= ?A and first <= ?Z ->
        if upcase_letters?(rest) do
          args = sigil_args(args, fun)
          {:<<>>, _, [binary]} = parts

          formatted =
            <<?~, first, rest::binary, left::binary, binary::binary, right::binary, args::binary>>

          {:ok, fun.(ast, formatted)}
        else
          :error
        end

      <<"sigil_", name>> when name >= ?a and name <= ?z ->
        args = sigil_args(args, fun)
        formatted = "~" <> <<name>> <> interpolate(parts, left, right, fun) <> args
        {:ok, fun.(ast, formatted)}

      _ ->
        :error
    end
  end

  defp sigil_call(_other, _fun) do
    :error
  end

  defp upcase_letters?(<<letter, rest::binary>>) when letter >= ?A and letter <= ?Z,
    do: upcase_letters?(rest)

  defp upcase_letters?(<<_>>),
    do: false

  defp upcase_letters?(<<>>),
    do: true

  defp delimiter_pair("["), do: {"[", "]"}
  defp delimiter_pair("{"), do: {"{", "}"}
  defp delimiter_pair("("), do: {"(", ")"}
  defp delimiter_pair("<"), do: {"<", ">"}
  defp delimiter_pair("\"\"\""), do: {"\"\"\"\n", "\"\"\""}
  defp delimiter_pair("'''"), do: {"'''\n", "'''"}
  defp delimiter_pair(str), do: {str, str}

  defp sigil_args([], _fun), do: ""
  defp sigil_args(args, fun), do: fun.(args, List.to_string(args))

  defp op_expr?(expr) do
    case expr do
      {op, _, [_, _]} -> operator?(op, 2)
      {op, _, [_]} -> operator?(op, 1)
      _ -> false
    end
  end

  defp call_to_string(atom, _fun) when is_atom(atom), do: Atom.to_string(atom)
  defp call_to_string({:., _, [arg]}, fun), do: module_to_string(arg, fun) <> "."

  defp call_to_string({:., _, [left, right]}, fun) when is_atom(right),
    do: module_to_string(left, fun) <> "." <> call_to_string_for_atom(right)

  defp call_to_string({:., _, [left, right]}, fun),
    do: module_to_string(left, fun) <> "." <> call_to_string(right, fun)

  defp call_to_string(other, fun), do: to_string(other, fun)

  defp call_to_string_with_args(target, args, fun) do
    target = call_to_string(target, fun)
    args = args_to_string(args, fun)
    target <> "(" <> args <> ")"
  end

  defp call_to_string_for_atom(atom) do
    Macro.inspect_atom(:remote_call, atom)
  end

  defp args_to_string(args, fun) do
    {list, last} = split_last(args)

    if last != [] and Inspect.List.keyword?(last) do
      prefix =
        case list do
          [] -> ""
          _ -> Enum.map_join(list, ", ", &to_string(&1, fun)) <> ", "
        end

      prefix <> kw_list_to_string(last, fun)
    else
      Enum.map_join(args, ", ", &to_string(&1, fun))
    end
  end

  defp kw_blocks_to_string(kw, fun) do
    Enum.reduce(unquote(kw_keywords), " ", fn x, acc ->
      case Keyword.has_key?(kw, x) do
        true -> acc <> kw_block_to_string(x, Keyword.get(kw, x), fun)
        false -> acc
      end
    end) <> "end"
  end

  defp kw_block_to_string(key, value, fun) do
    block = adjust_new_lines(block_to_string(value, fun), "\n  ")
    Atom.to_string(key) <> "\n  " <> block <> "\n"
  end

  defp block_to_string([{:->, _, _} | _] = block, fun) do
    Enum.map_join(block, "\n", fn {:->, _, [left, right]} ->
      left = comma_join_or_empty_paren(left, fun, false)
      left <> "->\n  " <> adjust_new_lines(block_to_string(right, fun), "\n  ")
    end)
  end

  defp block_to_string({:__block__, _, exprs}, fun) do
    Enum.map_join(exprs, "\n", &to_string(&1, fun))
  end

  defp block_to_string(other, fun), do: to_string(other, fun)

  defp map_to_string([{:|, _, [update_map, update_args]}], fun) do
    to_string(update_map, fun) <> " | " <> map_to_string(update_args, fun)
  end

  defp map_to_string(list, fun) do
    cond do
      Inspect.List.keyword?(list) -> kw_list_to_string(list, fun)
      true -> map_list_to_string(list, fun)
    end
  end

  defp kw_list_to_string(list, fun) do
    Enum.map_join(list, ", ", fn {key, value} ->
      Macro.inspect_atom(:key, key) <> " " <> to_string(value, fun)
    end)
  end

  defp map_list_to_string(list, fun) do
    Enum.map_join(list, ", ", fn
      {key, value} -> to_string(key, fun) <> " => " <> to_string(value, fun)
      other -> to_string(other, fun)
    end)
  end

  defp wrap_in_parenthesis(expr, fun) do
    "(" <> to_string(expr, fun) <> ")"
  end

  defp op_to_string({op, _, [_, _]} = expr, fun, parent_op, side) when is_atom(op) do
    case Identifier.binary_op(op) do
      {_, prec} ->
        {parent_assoc, parent_prec} = Identifier.binary_op(parent_op)

        cond do
          parent_prec < prec -> to_string(expr, fun)
          parent_prec > prec -> wrap_in_parenthesis(expr, fun)
          parent_assoc == side -> to_string(expr, fun)
          true -> wrap_in_parenthesis(expr, fun)
        end

      :error ->
        to_string(expr, fun)
    end
  end

  defp op_to_string(expr, fun, _, _), do: to_string(expr, fun)

  defp arrow_to_string(pairs, fun, paren \\ false) do
    Enum.map_join(pairs, "; ", fn {:->, _, [left, right]} ->
      left = comma_join_or_empty_paren(left, fun, paren)
      left <> "-> " <> to_string(right, fun)
    end)
  end

  defp comma_join_or_empty_paren([], _fun, true), do: "() "
  defp comma_join_or_empty_paren([], _fun, false), do: ""

  defp comma_join_or_empty_paren(left, fun, _) do
    Enum.map_join(left, ", ", &to_string(&1, fun)) <> " "
  end

  defp split_last([]) do
    {[], []}
  end

  defp split_last(args) do
    {left, [right]} = Enum.split(args, -1)
    {left, right}
  end

  defp adjust_new_lines(block, replacement) do
    for <<x <- block>>, into: "" do
      case x == ?\n do
        true -> replacement
        false -> <<x>>
      end
    end
  end








  def compile_apply(mod, fun, args, caller) do
    :elixir_env.trace({:remote_macro, [], mod, fun, length(args)}, caller)
    Kernel.apply(mod, fun, args)
  end


















































































  def expand_once(ast, env) do
    elem(do_expand_once(ast, env), 0)
  end

  defp do_expand_once({:__aliases__, meta, list} = alias, env) do
    case :elixir_aliases.expand_or_concat(meta, list, env, true) do
      receiver when is_atom(receiver) ->
        :elixir_env.trace({:alias_reference, meta, receiver}, env)
        {receiver, true}

      [head | tail] ->
        {head, _} = do_expand_once(head, env)

        case is_atom(head) do
          true ->
            receiver = :elixir_aliases.concat([head | tail])
            :elixir_env.trace({:alias_reference, meta, receiver}, env)
            {receiver, true}

          false ->
            {alias, false}
        end
    end
  end

  # Expand compilation environment macros
  defp do_expand_once({:__MODULE__, _, atom}, env) when is_atom(atom), do: {env.module, true}

  defp do_expand_once({:__DIR__, _, atom}, env) when is_atom(atom),
    do: {:filename.dirname(env.file), true}

  defp do_expand_once({:__ENV__, _, atom}, env) when is_atom(atom) and env.context != :match do
    env = update_in(env.versioned_vars, &maybe_escape_map/1)
    {maybe_escape_map(env), true}
  end

  defp do_expand_once({{:., _, [{:__ENV__, _, atom}, field]}, _, []} = original, env)
       when is_atom(atom) and is_atom(field) and env.context != :match do
    if Map.has_key?(env, field) do
      {maybe_escape_map(Map.get(env, field)), true}
    else
      {original, false}
    end
  end

  defp do_expand_once({name, meta, context} = original, _env)
       when is_atom(name) and is_list(meta) and is_atom(context) do
    {original, false}
  end

  defp do_expand_once({name, meta, args} = original, env)
       when is_atom(name) and is_list(args) and is_list(meta) do
    arity = length(args)

    case Macro.Env.expand_import(env, meta, name, arity) do
      {:macro, _receiver, expander} ->
        # We don't want the line to propagate yet, but generated might!
        {expander.(Keyword.take(meta, [:generated]), args), true}

      {:function, Kernel, op} when op in [:+, :-] and arity == 1 ->
        case expand_once(hd(args), env) do
          integer when is_integer(integer) -> {apply(Kernel, op, [integer]), true}
          _ -> {original, false}
        end

      {:function, _receiver, _name} ->
        {original, false}

      {:error, :not_found} ->
        {original, false}

      {:error, other} ->
        :elixir_errors.file_error(meta, env, :elixir_dispatch, {:import, other, name, arity})
    end
  end

  # Expand possible macro require invocation
  defp do_expand_once({{:., _, [left, name]}, meta, args} = original, env) when is_atom(name) do
    {receiver, _} = do_expand_once(left, env)

    case is_atom(receiver) do
      false ->
        {original, false}

      true ->
        case Macro.Env.expand_require(env, meta, receiver, name, length(args)) do
          {:macro, _receiver, expander} ->
            # We don't want the line to propagate yet, but generated might!
            {expander.(Keyword.take(meta, [:generated]), args), true}

          :error ->
            {original, false}
        end
    end
  end

  # Anything else is just returned
  defp do_expand_once(other, _env), do: {other, false}

  defp maybe_escape_map(map) when is_map(map), do: {:%{}, [], Map.to_list(map)}
  defp maybe_escape_map(other), do: other






  def special_form?(name, arity) when is_atom(name) and is_integer(arity) do
    :elixir_import.special_form(name, arity)
  end




















  def operator?(name, arity)

  def operator?(:..//, 3),
    do: true

  # Code.Identifier treats :// as a binary operator for precedence
  # purposes but it isn't really one, so we explicitly skip it.
  def operator?(name, 2) when is_atom(name),
    do: Identifier.binary_op(name) != :error and name != :"//"

  def operator?(name, 1) when is_atom(name),
    do: Identifier.unary_op(name) != :error

  def operator?(:.., 0), do: true
  def operator?(:..., 0), do: true

  def operator?(name, arity) when is_atom(name) and is_integer(arity), do: false



























  def quoted_literal?(term)

  def quoted_literal?({:__aliases__, _, args}),
    do: quoted_literal?(args)

  def quoted_literal?({:%, _, [left, right]}),
    do: quoted_literal?(left) and quoted_literal?(right)

  def quoted_literal?({:%{}, _, args}), do: quoted_literal?(args)
  def quoted_literal?({:{}, _, args}), do: quoted_literal?(args)
  def quoted_literal?({:__MODULE__, _, ctx}) when is_atom(ctx), do: true
  def quoted_literal?({:<<>>, _, segments}), do: Enum.all?(segments, &quoted_bitstring_segment?/1)
  def quoted_literal?({left, right}), do: quoted_literal?(left) and quoted_literal?(right)
  def quoted_literal?(list) when is_list(list), do: :lists.all(&quoted_literal?/1, list)
  def quoted_literal?(term), do: is_atom(term) or is_number(term) or is_binary(term)

  defp quoted_bitstring_segment?(term) when is_integer(term) or is_binary(term), do: true

  defp quoted_bitstring_segment?({:"::", _, [term, modifier]})
       when is_integer(term) or is_binary(term),
       do: quoted_bitstring_modifier?(modifier)

  defp quoted_bitstring_segment?(_other), do: false

  defp quoted_bitstring_modifier?({:-, _, [left, right]}),
    do: quoted_bitstring_modifier?(left) and quoted_bitstring_modifier?(right)

  defp quoted_bitstring_modifier?({atom, _, [size]})
       when atom in [:size, :unit] and is_integer(size),
       do: true

  defp quoted_bitstring_modifier?({:*, _, [left, right]})
       when is_integer(left) and is_integer(right),
       do: true

  defp quoted_bitstring_modifier?({modifier, _, ctx}) when is_atom(ctx) or ctx == [],
    do: :elixir_bitstring.validate_spec(modifier, nil) != :none

  defp quoted_bitstring_modifier?(_other), do: false



  def expand_literal(ast, env) do
    expand_literals(ast, env)
  end





















  def expand_literals(ast, env) do
    {ast, :ok} = expand_literals(ast, :ok, fn node, :ok -> {expand(node, env), :ok} end)
    ast
  end











  def expand_literals(ast, acc, fun)

  def expand_literals({:__aliases__, meta, args}, acc, fun) do
    {args, acc} = expand_literals(args, acc, fun)

    if :lists.all(&is_atom/1, args) do
      fun.({:__aliases__, meta, args}, acc)
    else
      {{:__aliases__, meta, args}, acc}
    end
  end

  def expand_literals({:__MODULE__, _meta, ctx} = node, acc, fun) when is_atom(ctx) do
    fun.(node, acc)
  end

  def expand_literals({:%, meta, [left, right]}, acc, fun) do
    {left, acc} = expand_literals(left, acc, fun)
    {right, acc} = expand_literals(right, acc, fun)
    {{:%, meta, [left, right]}, acc}
  end

  def expand_literals({:%{}, meta, args}, acc, fun) do
    {args, acc} = expand_literals(args, acc, fun)
    {{:%{}, meta, args}, acc}
  end

  def expand_literals({:{}, meta, args}, acc, fun) do
    {args, acc} = expand_literals(args, acc, fun)
    {{:{}, meta, args}, acc}
  end

  def expand_literals({left, right}, acc, fun) do
    {left, acc} = expand_literals(left, acc, fun)
    {right, acc} = expand_literals(right, acc, fun)
    {{left, right}, acc}
  end

  def expand_literals(list, acc, fun) when is_list(list) do
    :lists.mapfoldl(&expand_literals(&1, &2, fun), acc, list)
  end

  def expand_literals(
        {{:., _, [{:__aliases__, _, [:Application]}, :compile_env]} = node, meta,
         [app, key, default]},
        acc,
        fun
      ) do
    {default, acc} = expand_literals(default, acc, fun)
    {{node, meta, [app, key, default]}, acc}
  end

  def expand_literals(term, acc, _fun), do: {term, acc}












  def expand(ast, env) do
    expand_until({ast, true}, env)
  end

  defp expand_until({ast, true}, env) do
    expand_until(do_expand_once(ast, env), env)
  end

  defp expand_until({ast, false}, _env) do
    ast
  end








































  def underscore(atom_or_string)

  def underscore(atom) when is_atom(atom) do
    "Elixir." <> rest = Atom.to_string(atom)
    underscore(rest)
  end

  def underscore(<<h, t::binary>>) do
    <<to_lower_char(h)>> <> do_underscore(t, h)
  end

  def underscore("") do
    ""
  end

  defp do_underscore(<<h, t, rest::binary>>, _)
       when h >= ?A and h <= ?Z and not (t >= ?A and t <= ?Z) and not (t >= ?0 and t <= ?9) and
              t != ?. and t != ?_ do
    <<?_, to_lower_char(h), t>> <> do_underscore(rest, t)
  end

  defp do_underscore(<<h, t::binary>>, prev)
       when h >= ?A and h <= ?Z and not (prev >= ?A and prev <= ?Z) and prev != ?_ do
    <<?_, to_lower_char(h)>> <> do_underscore(t, h)
  end

  defp do_underscore(<<?., t::binary>>, _) do
    <<?/>> <> underscore(t)
  end

  defp do_underscore(<<h, t::binary>>, _) do
    <<to_lower_char(h)>> <> do_underscore(t, h)
  end

  defp do_underscore(<<>>, _) do
    <<>>
  end



























  def camelize(string)

  def camelize(""), do: ""
  def camelize(<<?_, t::binary>>), do: camelize(t)
  def camelize(<<h, t::binary>>), do: <<to_upper_char(h)>> <> do_camelize(t)

  defp do_camelize(<<?_, ?_, t::binary>>), do: do_camelize(<<?_, t::binary>>)

  defp do_camelize(<<?_, h, t::binary>>) when h >= ?a and h <= ?z,
    do: <<to_upper_char(h)>> <> do_camelize(t)

  defp do_camelize(<<?_, h, t::binary>>) when h >= ?0 and h <= ?9, do: <<h>> <> do_camelize(t)
  defp do_camelize(<<?_>>), do: <<>>
  defp do_camelize(<<?/, t::binary>>), do: <<?.>> <> camelize(t)
  defp do_camelize(<<h, t::binary>>), do: <<h>> <> do_camelize(t)
  defp do_camelize(<<>>), do: <<>>

  defp to_upper_char(char) when char >= ?a and char <= ?z, do: char - 32
  defp to_upper_char(char), do: char

  defp to_lower_char(char) when char >= ?A and char <= ?Z, do: char + 32
  defp to_lower_char(char), do: char

  ## Atom handling






































  def classify_atom(atom) do
    case inner_classify(atom) do
      :alias -> :alias
      :identifier -> :identifier
      type when type in [:unquoted_operator, :not_callable] -> :unquoted
      _ -> :quoted
    end
  end



































































  def inspect_atom(source_format, atom, opts \\ [])

  def inspect_atom(:literal, atom, _opts) when is_nil(atom) or is_boolean(atom) do
    Atom.to_string(atom)
  end

  def inspect_atom(:literal, atom, opts) when is_atom(atom) do
    binary = Atom.to_string(atom)

    case classify_atom(atom) do
      :alias ->
        case binary do
          binary when binary in ["Elixir", "Elixir.Elixir"] -> binary
          "Elixir.Elixir." <> _rest -> binary
          "Elixir." <> rest -> rest
        end

      :quoted ->
        escaped = inspect_atom_escape(opts, binary, ?")
        IO.iodata_to_binary([?:, ?", escaped, ?"])

      _ ->
        ":" <> binary
    end
  end

  def inspect_atom(:key, atom, opts) when is_atom(atom) do
    binary = Atom.to_string(atom)

    case classify_atom(atom) do
      :alias ->
        IO.iodata_to_binary([?", binary, ?", ?:])

      :quoted ->
        escaped = inspect_atom_escape(opts, binary, ?")
        IO.iodata_to_binary([?", escaped, ?", ?:])

      _ ->
        IO.iodata_to_binary([binary, ?:])
    end
  end

  def inspect_atom(:remote_call, atom, opts) when is_atom(atom) do
    binary = Atom.to_string(atom)

    case inner_classify(atom) do
      type when type in [:identifier, :unquoted_operator, :quoted_operator] ->
        binary

      type ->
        escaped =
          if type in [:not_callable, :alias] do
            binary
          else
            inspect_atom_escape(opts, binary, ?")
          end

        IO.iodata_to_binary([?", escaped, ?"])
    end
  end

  defp inspect_atom_escape(opts, string, char) do
    if escape = opts[:escape] do
      escape.(string, char)
    else
      {escaped, _} = Code.Identifier.escape(string, char)
      escaped
    end
  end

  # Classifies the given atom into one of the following categories:
  #
  #   * `:alias` - a valid Elixir alias, like `Foo`, `Foo.Bar` and so on
  #
  #   * `:identifier` - an atom that can be used as a variable/local call;
  #     this category includes identifiers like `:foo`
  #
  #   * `:unquoted_operator` - all callable operators, such as `:<>`. Note
  #     operators such as `:..` are not callable because of ambiguity
  #
  #   * `:quoted_operator` - callable operators that must be wrapped in quotes when
  #     defined as an atom. For example, `::` must be written as `:"::"` to avoid
  #     the ambiguity between the atom and the keyword identifier
  #
  #   * `:not_callable` - an atom that cannot be used as a function call after the
  #     `.` operator. Those are typically AST nodes that are special forms (such as
  #     `:%{}` and `:<<>>>`) as well as nodes that are ambiguous in calls (such as
  #     `:..` and `:...`). This category also includes atoms like `:Foo`, since
  #     they are valid identifiers but they need quotes to be used in function
  #     calls (`Foo."Bar"`)
  #
  #   * `:other` - any other atom (these are usually escaped when inspected, like
  #     `:"foo and bar"`)
  #
  defp inner_classify(atom) when is_atom(atom) do
    cond do
      atom in [:%, :%{}, :{}, :<<>>, :..., :.., :., :..//, :->] ->
        :not_callable

      # <|>, ^^^, and ~~~ are deprecated
      atom in [:"::", :"^^^", :"~~~", :"<|>"] ->
        :quoted_operator

      operator?(atom, 1) or operator?(atom, 2) ->
        :unquoted_operator

      true ->
        charlist = Atom.to_charlist(atom)

        if valid_alias?(charlist) do
          :alias
        else
          case :elixir_config.identifier_tokenizer().tokenize(charlist) do
            {kind, _acc, [], _, _, special} ->
              cond do
                kind != :identifier or :lists.member(:at, special) ->
                  :not_callable

                # identifier_tokenizer used to return errors for non-nfc, but
                # now it nfc-normalizes everything. However, lack of nfc is
                # still a good reason to quote an atom when printing.
                :lists.member(:nfkc, special) ->
                  :other

                true ->
                  :identifier
              end

            _ ->
              :other
          end
        end
    end
  end

  defp valid_alias?([?E, ?l, ?i, ?x, ?i, ?r] ++ rest), do: valid_alias_piece?(rest)
  defp valid_alias?(_other), do: false

  defp valid_alias_piece?([?., char | rest]) when char >= ?A and char <= ?Z,
    do: valid_alias_piece?(trim_leading_while_valid_identifier(rest))

  defp valid_alias_piece?([]), do: true
  defp valid_alias_piece?(_other), do: false

  defp trim_leading_while_valid_identifier([char | rest])
       when char >= ?a and char <= ?z
       when char >= ?A and char <= ?Z
       when char >= ?0 and char <= ?9
       when char == ?_ do
    trim_leading_while_valid_identifier(rest)
  end

  defp trim_leading_while_valid_identifier(other) do
    other
  end




















  def dbg(code, options, %Macro.Env{} = env) do
    case env.context do
      :match ->
        raise ArgumentError,
              "invalid expression in match, dbg is not allowed in patterns " <>
                "such as function clauses, case clauses or on the left side of the = operator"

      :guard ->
        raise ArgumentError,
              "invalid expression in guard, dbg is not allowed in guards. " <>
                "To learn more about guards, visit: https://hexdocs.pm/elixir/patterns-and-guards.html"

      _ ->
        :ok
    end

    header = dbg_format_header(env)

    quote do
      to_debug = unquote(dbg_ast_to_debuggable(code, env))
      unquote(__MODULE__).__dbg__(unquote(header), to_debug, unquote(options))
    end
  end

  # Pipelines.
  defp dbg_ast_to_debuggable({:|>, _meta, _args} = pipe_ast, _env) do
    value_var = unique_var(:value, __MODULE__)
    values_acc_var = unique_var(:values, __MODULE__)

    [start_ast | rest_asts] = asts = for {ast, 0} <- unpipe(pipe_ast), do: ast
    rest_asts = Enum.map(rest_asts, &pipe(value_var, &1, 0))

    initial_acc =
      quote do
        unquote(value_var) = unquote(start_ast)
        unquote(values_acc_var) = [unquote(value_var)]
      end

    values_ast =
      for step_ast <- rest_asts, reduce: initial_acc do
        ast_acc ->
          quote do
            unquote(ast_acc)
            unquote(value_var) = unquote(step_ast)
            unquote(values_acc_var) = [unquote(value_var) | unquote(values_acc_var)]
          end
      end

    quote do
      unquote(values_ast)

      {:pipe, unquote(escape(asts)), Enum.reverse(unquote(values_acc_var))}
    end
  end

  dbg_decomposed_binary_operators = [:&&, :||, :and, :or]

  # Logic operators.
  defp dbg_ast_to_debuggable({op, _meta, [_left, _right]} = ast, _env)
       when op in unquote(dbg_decomposed_binary_operators) do
    acc_var = unique_var(:acc, __MODULE__)
    result_var = unique_var(:result, __MODULE__)

    quote do
      unquote(acc_var) = []
      unquote(dbg_boolean_tree(ast, acc_var, result_var))
      {:logic_op, Enum.reverse(unquote(acc_var)), unquote(result_var)}
    end
  end

  defp dbg_ast_to_debuggable({:__block__, _meta, exprs} = ast, _env) when exprs != [] do
    acc_var = unique_var(:acc, __MODULE__)
    result_var = unique_var(:result, __MODULE__)

    quote do
      unquote(acc_var) = []
      unquote(dbg_block(ast, acc_var, result_var))
      {:block, Enum.reverse(unquote(acc_var)), unquote(result_var)}
    end
  end

  defp dbg_ast_to_debuggable({:case, _meta, [expr, [do: clauses]]} = ast, _env) do
    clauses_returning_index =
      Enum.with_index(clauses, fn {:->, meta, [left, right]}, index ->
        {:->, meta, [left, {right, index}]}
      end)

    quote do
      expr = unquote(expr)

      {result, clause_index} =
        case expr do
          unquote(clauses_returning_index)
        end

      {:case, unquote(escape(ast)), expr, clause_index, result}
    end
  end

  defp dbg_ast_to_debuggable({:cond, _meta, [[do: clauses]]} = ast, _env) do
    modified_clauses =
      Enum.with_index(clauses, fn {:->, _meta, [[left], right]}, index ->
        hd(
          quote do
            clause_value = unquote(left) ->
              {unquote(escape(left)), clause_value, unquote(index), unquote(right)}
          end
        )
      end)

    quote do
      {clause_ast, clause_value, clause_index, value} =
        cond do
          unquote(modified_clauses)
        end

      {:cond, unquote(escape(ast)), clause_ast, clause_value, clause_index, value}
    end
  end

  defp dbg_ast_to_debuggable({:if, meta, [condition_ast, clauses]} = ast, env) do
    case Macro.Env.lookup_import(env, {:if, 2}) do
      [macro: Kernel] ->
        condition_result_var = unique_var(:condition_result, __MODULE__)

        quote do
          unquote(condition_result_var) = unquote(condition_ast)
          result = unquote({:if, meta, [condition_result_var, clauses]})

          {:if, unquote(escape(ast)), unquote(escape(condition_ast)),
           unquote(condition_result_var), result}
        end

      _ ->
        quote do: {:value, unquote(escape(ast)), unquote(ast)}
    end
  end

  defp dbg_ast_to_debuggable({:with, meta, args} = ast, _env) do
    {opts, clauses} = List.pop_at(args, -1)

    acc_var = unique_var(:acc, __MODULE__)

    modified_clauses =
      Enum.flat_map(clauses, fn
        # We only detail assignments and pattern-matching clauses that
        # can be helpful to understand how the result is constructed.
        {:<-, _meta, [left, right]} ->
          modified_left =
            case left do
              {:when, meta, [pattern, guard]} -> {:when, meta, [{pattern, quote(do: _)}, guard]}
              pattern -> {pattern, quote(do: _)}
            end

          quote do
            [
              value = unquote(right),
              unquote(acc_var) = [{unquote(escape(right)), value} | unquote(acc_var)],
              unquote(modified_left) <- {value, unquote(acc_var)}
            ]
          end

        {:=, _meta, [left, right]} ->
          quote do
            [
              value = unquote(right),
              unquote(acc_var) = [{unquote(escape(right)), value} | unquote(acc_var)],
              unquote(left) = value
            ]
          end

        # Other expressions like side effects are omitted.
        expr ->
          [expr]
      end)

    modified_opts =
      Enum.map(opts, fn
        {:do, do_block} ->
          {:do, {do_block, acc_var}}

        {:else, else_block} ->
          clauses =
            Enum.map(else_block, fn
              {:->, meta, [[{:when, meta2, [pattern, guard]}], right]} ->
                {:->, meta, [[{:when, meta2, [{pattern, acc_var}, guard]}], {right, acc_var}]}

              {:->, meta, [[left], right]} ->
                {:->, meta, [[{left, acc_var}], {right, acc_var}]}

              invalid ->
                invalid
            end)

          error_clause =
            quote generated: true do
              {other, _acc} -> raise WithClauseError, term: other
            end

          {:else, clauses ++ error_clause}

        invalid ->
          invalid
      end)

    modified_with_ast = {:with, meta, modified_clauses ++ [modified_opts]}

    quote do
      unquote(acc_var) = []

      {value, acc} = unquote(modified_with_ast)

      {:with, unquote(escape(ast)), Enum.reverse(acc), value}
    end
  end

  # Any other AST.
  defp dbg_ast_to_debuggable(ast, _env) do
    quote do: {:value, unquote(escape(ast)), unquote(ast)}
  end

  # This is a binary operator. We replace the left side with a recursive call to
  # this function to decompose it, and then execute the operation and add it to the acc.
  defp dbg_boolean_tree({op, _meta, [left, right]} = ast, acc_var, result_var)
       when op in unquote(dbg_decomposed_binary_operators) do
    replaced_left = dbg_boolean_tree(left, acc_var, result_var)

    quote do
      unquote(result_var) = unquote(op)(unquote(replaced_left), unquote(right))

      unquote(acc_var) = [
        {unquote(escape(ast)), unquote(result_var)} | unquote(acc_var)
      ]

      unquote(result_var)
    end
  end

  # This is finally an expression, so we assign "result = expr", add it to the acc, and
  # return the result.
  defp dbg_boolean_tree(ast, acc_var, result_var) do
    quote do
      unquote(result_var) = unquote(ast)
      unquote(acc_var) = [{unquote(escape(ast)), unquote(result_var)} | unquote(acc_var)]
      unquote(result_var)
    end
  end

  defp dbg_block({:__block__, meta, exprs}, acc_var, result_var) do
    modified_exprs =
      Enum.map(exprs, fn expr ->
        quote do
          unquote(result_var) = unquote(expr)
          unquote(acc_var) = [{unquote(escape(expr)), unquote(result_var)} | unquote(acc_var)]
        end
      end)

    {:__block__, meta, modified_exprs}
  end

  # Made public to be called from Macro.dbg/3, so that we generate as little code
  # as possible and call out into a function as soon as we can.

  def __dbg__(header_string, to_debug, options) do
    {print_location?, options} = Keyword.pop(options, :print_location, true)
    syntax_colors = if IO.ANSI.enabled?(), do: IO.ANSI.syntax_colors(), else: []
    options = Keyword.merge([width: 80, pretty: true, syntax_colors: syntax_colors], options)

    {formatted, result} = dbg_format_ast_to_debug(to_debug, options)

    formatted =
      if print_location? do
        [:cyan, :italic, header_string, :reset, "\n", formatted, "\n"]
      else
        [formatted, "\n"]
      end

    ansi_enabled? = options[:syntax_colors] != []
    :ok = IO.write(IO.ANSI.format(formatted, ansi_enabled?))

    result
  end

  defp dbg_format_ast_to_debug({:pipe, code_asts, values}, options) do
    result = List.last(values)
    code_strings = Enum.map(code_asts, &to_string_with_colors(&1, options))
    [{first_ast, first_value} | asts_with_values] = Enum.zip(code_strings, values)
    first_formatted = [dbg_format_ast(first_ast), " ", inspect(first_value, options), ?\n]

    rest_formatted =
      Enum.map(asts_with_values, fn {code_ast, value} ->
        [:faint, "|> ", :reset, dbg_format_ast(code_ast), " ", inspect(value, options), ?\n]
      end)

    {[first_formatted | rest_formatted], result}
  end

  defp dbg_format_ast_to_debug({:logic_op, components, value}, options) do
    formatted =
      Enum.map(components, fn {ast, value} ->
        [dbg_format_ast(to_string_with_colors(ast, options)), " ", inspect(value, options), ?\n]
      end)

    {formatted, value}
  end

  defp dbg_format_ast_to_debug({:block, components, value}, options) do
    formatted =
      [
        dbg_maybe_underline("Code block", options),
        ":\n(\n",
        Enum.map(components, fn {ast, value} ->
          ["  ", dbg_format_ast_with_value(ast, value, options)]
        end),
        ")\n"
      ]

    {formatted, value}
  end

  defp dbg_format_ast_to_debug({:case, ast, expr_value, clause_index, value}, options) do
    {:case, _meta, [expr_ast, _]} = ast

    formatted = [
      dbg_maybe_underline("Case argument", options),
      ":\n",
      dbg_format_ast_with_value(expr_ast, expr_value, options),
      ?\n,
      dbg_maybe_underline("Case expression", options),
      " (clause ##{clause_index + 1} matched):\n",
      dbg_format_ast_with_value(ast, value, options)
    ]

    {formatted, value}
  end

  defp dbg_format_ast_to_debug(
         {:cond, ast, clause_ast, clause_value, clause_index, value},
         options
       ) do
    formatted = [
      dbg_maybe_underline("Cond clause", options),
      " (clause ##{clause_index + 1} matched):\n",
      dbg_format_ast_with_value(clause_ast, clause_value, options),
      ?\n,
      dbg_maybe_underline("Cond expression", options),
      ":\n",
      dbg_format_ast_with_value(ast, value, options)
    ]

    {formatted, value}
  end

  defp dbg_format_ast_to_debug(
         {:if, ast, condition_ast, condition_result, result},
         options
       ) do
    formatted = [
      dbg_maybe_underline("If condition", options),
      ":\n",
      dbg_format_ast_with_value(condition_ast, condition_result, options),
      ?\n,
      dbg_maybe_underline("If expression", options),
      ":\n",
      dbg_format_ast_with_value(ast, result, options)
    ]

    {formatted, result}
  end

  defp dbg_format_ast_to_debug({:with, ast, clauses, result}, options) do
    formatted_clauses =
      Enum.map(clauses, fn {clause_ast, clause_result} ->
        dbg_format_ast_with_value(clause_ast, clause_result, options)
      end)

    formatted = [
      dbg_maybe_underline("With clauses", options),
      ":\n",
      formatted_clauses,
      ?\n,
      dbg_maybe_underline("With expression", options),
      ":\n",
      dbg_format_ast_with_value(ast, result, options)
    ]

    {formatted, result}
  end

  defp dbg_format_ast_to_debug({:value, code_ast, value}, options) do
    {dbg_format_ast_with_value(code_ast, value, options), value}
  end

  defp dbg_format_ast_with_value(ast, value, options) do
    [dbg_format_ast(to_string_with_colors(ast, options)), " ", inspect(value, options), ?\n]
  end

  defp to_string_with_colors(ast, options) do
    options = Keyword.take(options, [:syntax_colors])

    algebra = Code.quoted_to_algebra(ast, options)
    IO.iodata_to_binary(Inspect.Algebra.format(algebra, 98))
  end

  defp dbg_format_header(env) do
    env = Map.update!(env, :file, &(&1 && Path.relative_to_cwd(&1)))
    [stacktrace_entry] = Macro.Env.stacktrace(env)
    "[" <> Exception.format_stacktrace_entry(stacktrace_entry) <> "]"
  end

  defp dbg_maybe_underline(string, options) do
    if options[:syntax_colors] != [] do
      IO.ANSI.format([:underline, string, :reset])
    else
      string
    end
  end

  defp dbg_format_ast(ast) do
    [ast, :faint, " #=>", :reset]
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/macro.ex (docs and specs stripped;
# line numbers match the original).
