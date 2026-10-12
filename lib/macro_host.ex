# Compile-time macro expansion for the tonic compiler.
#
# tonic compiles Elixir ahead of time in Rust; macros whose bodies are more
# than a quote template run here instead: the compiler starts this program
# (built once and cached), has it load the program's modules into the
# interpreter (Tonic.Eval), and asks it to expand macro calls. Requests and
# replies are framed as "CMD <bytes>\n<payload>" on stdin/stdout.
defmodule Tonic.MacroHost do
  @moduledoc false

  def main do
    Process.put(:tonic_macro_host, true)
    Process.put(:tonic_macro_load_errors, %{})
    Process.put(:tonic_macro_output, spawn(fn -> output_loop() end))
    loop()
  end

  defp loop do
    case IO.read(:stdio, :line) do
      :eof ->
        :ok

      {:error, _} ->
        :ok

      header ->

        reply =
          try do
            {cmd, payload} = read_frame(header)
            {:ok, dispatch(cmd, payload)}
          rescue
            e -> {:error, Exception.format(:error, e, __STACKTRACE__)}
          catch
            kind, reason -> {:error, Exception.format(kind, reason, __STACKTRACE__)}
          end

        case reply do
          {:ok, out} -> send_frame("OK", out)
          {:error, msg} -> send_frame("ERR", msg)
        end

        loop()
    end
  end

  defp dispatch(cmd, payload) do
    leader = :tonic.group_leader_of(self())
    :tonic.set_group_leader(self(), Process.get(:tonic_macro_output))
    try do
      handle(cmd, payload)
    after
      :tonic.set_group_leader(self(), leader)
    end
  end

  defp output_loop do
    receive do
      {:io_request, from, ref, request} ->
        result = output_request(request)
        send(from, {:io_reply, ref, result})
        output_loop()
      _ -> output_loop()
    end
  end

  defp output_request({:put_chars, _, chars}), do: :tonic.io_write(:stderr, chars)
  defp output_request({:put_chars, chars}), do: :tonic.io_write(:stderr, chars)
  defp output_request({:put_chars, _, m, f, a}), do: :tonic.io_write(:stderr, apply(m, f, a))
  defp output_request({:requests, requests}), do: Enum.reduce(requests, :ok, fn request, _ -> output_request(request) end)
  defp output_request(:getopts), do: [binary: true, encoding: :unicode]
  defp output_request({:setopts, _}), do: :ok
  defp output_request(_), do: {:error, :enotsup}

  defp read_frame(header) do
    case String.split(String.trim_trailing(header, "\n"), " ") do
      [cmd, size] when cmd in ["LOAD", "EXPAND", "DERIVE", "CHECK", "CALLBACKS", "ATTRS", "AFTER", "EVAL", "LOADCONFIG", "ATTRIBUTE", "DEFINITIONS"] ->
        len = String.to_integer(size)
        limit = if cmd == "AFTER", do: 268435456, else: 16777216
        if len < 0 or len > limit, do: raise(ArgumentError, "macro host request exceeds its size limit")
        payload = if len == 0, do: "", else: :tonic.io_read_bytes(len)
        if payload == :eof, do: raise(ArgumentError, "incomplete macro host request body")
        {cmd, payload}
      _ -> raise ArgumentError, "invalid macro host request header"
    end
  end

  defp send_frame(tag, data) do
    data = IO.iodata_to_binary(data)
    IO.binwrite(:stdio, [tag, " ", Integer.to_string(byte_size(data)), "\n", data])
    :tonic.io_flush()
  end

  defp handle("LOADCONFIG", path) do
    File.read!(path) |> Code.string_to_quoted!(file: path) |> Tonic.Eval.eval_quoted([], file: path)
    "ok"
  end

  defp handle("LOAD", path) do
    src = File.read!(path)
    quoted = Code.string_to_quoted!(src, file: path)
    in_root(path, fn -> load_modules(quoted, path) end)
    "ok"
  end

  # Like Mix, compile each project's code from its root, so relative paths
  # read at compile time resolve against the dependency that holds them.
  defp in_root(file, fun) do
    case root(file) do
      nil ->
        fun.()

      root ->
        previous = File.cwd!()
        File.cd!(root)

        try do
          fun.()
        after
          File.cd!(previous)
        end
    end
  end

  defp root(file) when is_binary(file) do
    cache = Process.get(:tonic_macro_roots, %{})

    case Map.fetch(cache, file) do
      {:ok, root} ->
        root

      :error ->
        root = find_root(Path.dirname(Path.expand(file)))
        Process.put(:tonic_macro_roots, Map.put(cache, file, root))
        root
    end
  end

  defp root(_), do: nil

  defp find_root(dir) do
    cond do
      File.regular?(Path.join(dir, "mix.exs")) -> dir
      Path.dirname(dir) == dir -> nil
      true -> find_root(Path.dirname(dir))
    end
  end

  defp handle("EXPAND", text) do
    {{mod, name, args, env}, _} = Code.eval_string(text)

    caller = %Macro.Env{
      module: env[:module],
      function: env[:function],
      file: env[:file] || "nofile",
      line: env[:line] || 1,
      aliases: env[:aliases] || [],
      context: env[:context]
    }

    check_loaded!(mod)

    unless Tonic.Eval.Modules.macro?(mod, name, length(args), true) do
      raise CompileError, description: "macro #{inspect(mod)}.#{name}/#{length(args)} is not available at compile time"
    end

    args = Tonic.Eval.expand_unquotes(args, caller)
    ast = in_root(caller.file, fn -> Tonic.Eval.Modules.call_macro(mod, name, args, caller) end)
    id = :erlang.unique_integer([:positive])
    ast |> hygiene(id) |> Macro.to_string()
  end

  defp handle("CHECK", text) do
    {mod, _} = Code.eval_string(text)
    check_loaded!(mod)
    "ok"
  end

  # @derive through the Any implementation's __deriving__/3 macro, called with
  # the struct itself, as Protocol.derive does.
  defp handle("DERIVE", text) do
    {{impl, module, opts, file, line}, _} = Code.eval_string(text)
    check_loaded!(module)
    struct = Map.new([{:__struct__, module} | Tonic.Eval.Modules.struct_fields(module) || []])
    caller = %Macro.Env{module: module, file: file, line: line}
    ast = in_root(file, fn -> Tonic.Eval.Modules.call_macro(impl, :__deriving__, [module, struct, opts], caller) end)
    ast |> hygiene(:erlang.unique_integer([:positive])) |> Macro.to_string()
  end

  # Load definitions with the file's lexical directives; skip runtime expressions.
  defp load_modules({:__block__, _, forms}, path), do: load_forms(forms, path, [])
  defp load_modules(form, path), do: load_forms([form], path, [])

  defp load_forms(forms, path, directives) do
    Enum.reduce(forms, directives, fn
      {kind, _, _} = form, directives when kind in [:alias, :import, :require] ->
        directives ++ [form]
      {kind, _, _} = form, directives when kind in [:defmodule, :defprotocol, :defimpl] ->
        load_module(form, path, directives)
        directives
      {kind, _, [condition, options]} = form, directives when kind in [:if, :unless] ->
        if contains_module?(form) do
          {value, _} = Tonic.Eval.eval_quoted({:__block__, [], directives ++ [condition]}, [], file: path)
          truth = value not in [false, nil]
          body = Keyword.get(options, if truth == (kind == :if), do: :do, else: :else)
          forms = case body do
            {:__block__, _, expressions} -> expressions
            nil -> []
            expression -> [expression]
          end
          load_forms(forms, path, directives)
        end
        directives
      _, directives -> directives
    end)
    :ok
  end

  defp contains_module?(form) do
    {_, found} = Macro.prewalk(form, false, fn
      {kind, _, _} = node, _ when kind in [:defmodule, :defprotocol, :defimpl] -> {node, true}
      node, found -> {node, found}
    end)
    found
  end

  defp load_module(form, path, directives) do
    mod = loaded_name(form, path, directives)
    try do
      Tonic.Eval.eval_quoted({:__block__, [], directives ++ [form]}, [], file: path)
      Process.put(:tonic_macro_load_errors, Map.delete(Process.get(:tonic_macro_load_errors), mod))
    rescue
      e -> store_load_error(mod, path, format_error(:error, e, __STACKTRACE__))
    catch
      kind, reason -> store_load_error(mod, path, format_error(kind, reason, __STACKTRACE__))
    end

    if mod != nil and Map.has_key?(Process.get(:tonic_macro_load_errors), mod) do
      retries = Process.get(:tonic_macro_retries) || %{}
      Process.put(:tonic_macro_retries, Map.put(retries, mod, fn -> in_root(path, fn -> load_module(form, path, directives) end) end))
    end
  end

  defp loaded_name({kind, _, [name | _]}, path, directives) when kind in [:defmodule, :defprotocol] do
    {mod, _} = Tonic.Eval.eval_quoted({:__block__, [], directives ++ [name]}, [], file: path)
    mod
  end

  defp loaded_name({:defimpl, _, [protocol | rest]}, path, directives) do
    target = Enum.find_value(rest, fn options -> is_list(options) && Keyword.get(options, :for) end)
    {protocol, _} = Tonic.Eval.eval_quoted({:__block__, [], directives ++ [protocol]}, [], file: path)
    {target, _} = Tonic.Eval.eval_quoted({:__block__, [], directives ++ [target]}, [], file: path)
    if is_atom(target), do: Module.concat(protocol, target)
  end

  defp format_error(kind, reason, stacktrace) do
    Exception.format(kind, reason, stacktrace)
  rescue
    _ -> Exception.format_banner(kind, reason, [])
  end

  defp store_load_error(nil, path, error), do: raise(CompileError, file: path, description: error)
  defp store_load_error(mod, path, error) do
    errors = Process.get(:tonic_macro_load_errors)
    Process.put(:tonic_macro_load_errors, Map.put(errors, mod, "could not load #{inspect(mod)} from #{path}:\n#{error}"))
  end

  defp check_loaded!(mod) do
    if Map.has_key?(Process.get(:tonic_macro_load_errors), mod), do: Tonic.Eval.Modules.retry_load(mod)

    case Map.get(Process.get(:tonic_macro_load_errors), mod) do
      nil -> :ok
      error -> raise CompileError, description: error
    end
  end

  defp handle("ATTRIBUTE", text) do
    {request, _} = Code.eval_string(text)

    {mod, file, line, name, index} =
      case request do
        {mod, file, line, name} -> {mod, file, line, name, nil}
        {_, _, _, _, _} = request -> request
      end

    check_loaded!(mod)
    value = Tonic.Eval.Modules.attribute_value(mod, file, line, name, index)
    if name == :tonic_binding and not embeddable?(value) do
      "__tonic_runtime_binding__()"
    else
      embed_attribute(value, file, line, name)
    end
  end

  defp embed_attribute(value, file, line, name) do
    unless embeddable?(value), do: raise(CompileError, file: file, line: line, description: "@#{name} contains a runtime value that cannot be embedded in native code")
    value |> Tonic.Eval.escape() |> Macro.to_string()
  end

  defp embeddable?(value) when is_atom(value) or is_number(value) or is_bitstring(value), do: true
  defp embeddable?(value) when is_list(value), do: Enum.all?(value, &embeddable?/1)
  defp embeddable?(value) when is_tuple(value), do: value |> Tuple.to_list() |> Enum.all?(&embeddable?/1)
  defp embeddable?(value) when is_map(value), do: Enum.all?(Map.to_list(value), fn {key, item} -> embeddable?(key) and embeddable?(item) end)
  defp embeddable?(_), do: false

  defp handle("DEFINITIONS", text) do
    {request, _} = Code.eval_string(text)
    {module, file, first, last, allow_empty} =
      case request do
        {module, file, first, last} -> {module, file, first, last, false}
        {_, _, _, _, _} = request -> request
      end
    check_loaded!(module)
    definitions = Tonic.Eval.Modules.source_definitions(module, file, first, last)
    if definitions == [] and not allow_empty, do: raise(CompileError, file: file, line: first, description: "module expression did not generate definitions")
    Macro.to_string({:__block__, [], definitions})
  end

  defp handle("EVAL", text) do
    {{ast, file}, _} = Code.eval_string(text)
    {value, _} = Tonic.Eval.eval_quoted(ast, [], file: file)
    value |> Tonic.Eval.escape() |> Macro.to_string()
  end

  defp handle("AFTER", text) do
    [module, artifact] = :binary.split(text, "\n")
    {mod, _} = Code.eval_string(module)
    check_loaded!(mod)
    Tonic.Eval.Modules.run_after_callbacks(mod, artifact)
    "ok"
  end

  defp handle("ATTRS", text) do
    {{mod, line}, _} = Code.eval_string(text)
    check_loaded!(mod)
    Tonic.Eval.Modules.definition_attributes(mod, line) |> Tonic.Eval.escape() |> Macro.to_string()
  end

  defp handle("CALLBACKS", text) do
    {mod, _} = Code.eval_string(text)
    check_loaded!(mod)
    {:__block__, [], Tonic.Eval.Modules.callback_expansions(mod)} |> Macro.to_string()
  end


  # Variables introduced by the macro (non-nil context) get names unique to
  # this expansion so they cannot clash with the caller's.
  defp hygiene(ast, id) do
    ast = Macro.prewalk(ast, fn
      {sigil, _, [{:<<>>, _, parts}, _]} = literal when sigil in [:sigil_c, :sigil_C, :sigil_s, :sigil_S, :sigil_w, :sigil_W] ->
        if Enum.all?(parts, &is_binary/1) do
          {value, _} = Tonic.Eval.eval_quoted(literal, [], [])
          Tonic.Eval.escape(value)
        else
          literal
        end
      {:"::", meta, [value, spec]} -> {:"::", meta, [value, preserve_bit_types(spec)]}
      other -> other
    end)
    rename(ast, bound_vars(ast), id)
  end

  # Renames the variables the expansion binds; attribute names and the names
  # in definition heads are not variables.
  defp rename({:__aliases__, meta, _} = a, _bound, _id) do
    case Keyword.get(meta, :alias) do
      mod when is_atom(mod) and mod not in [nil, false] -> mod
      _ -> a
    end
  end

  defp rename({:@, meta, [{name, name_meta, ctx}]}, _bound, _id) when is_atom(name) and is_atom(ctx),
    do: {:@, meta, [{name, name_meta, ctx}]}

  defp rename({:@, meta, [{name, name_meta, args}]}, bound, id) when is_atom(name) and is_list(args),
    do: {:@, meta, [{name, name_meta, rename(args, bound, id)}]}

  defp rename({kind, meta, [head | rest]}, bound, id) when kind in [:def, :defp, :defmacro, :defmacrop, :defguard, :defguardp],
    do: {kind, meta, [rename_head(head, bound, id) | rename(rest, bound, id)]}

  defp rename({name, meta, ctx} = var, bound, id) when is_atom(name) and is_atom(ctx) and ctx != nil and name != :_ do
    if special_var?(name) or not MapSet.member?(bound, {name, ctx}) do
      var
    else
      {hygienic_name(name, id, ctx), meta, nil}
    end
  end

  defp rename({left, meta, args}, bound, id) when is_list(args), do: {rename(left, bound, id), meta, rename(args, bound, id)}
  defp rename({left, right}, bound, id), do: {rename(left, bound, id), rename(right, bound, id)}
  defp rename(list, bound, id) when is_list(list), do: Enum.map(list, &rename(&1, bound, id))
  defp rename(other, _bound, _id), do: other

  defp rename_head({:when, meta, [call | guards]}, bound, id), do: {:when, meta, [rename_head(call, bound, id) | rename(guards, bound, id)]}
  defp rename_head({name, meta, args}, bound, id) when is_atom(name) and is_list(args), do: {name, meta, rename(args, bound, id)}
  defp rename_head({name, _, ctx} = head, _bound, _id) when is_atom(name) and is_atom(ctx), do: head
  defp rename_head(head, bound, id), do: rename(head, bound, id)

  # The expansion travels as source text, where a variable loses its context.
  # Only variables the expansion binds are renamed; other context-tagged
  # names (zero-arity calls, function heads, attributes) stay as written.
  defp hygienic_name(name, id, ctx) do
    string = Atom.to_string(name)
    suffix = "_m#{id}#{hash(ctx)}"

    case String.last(string) do
      mark when mark in ["?", "!"] -> :"#{String.slice(string, 0..-2//1)}#{suffix}#{mark}"
      _ -> :"#{string}#{suffix}"
    end
  end

  defp bound_vars(ast) do
    {_, bound} =
      Macro.prewalk(ast, MapSet.new(), fn
        {:=, _, [left, _]} = node, acc -> {node, pattern_vars(left, acc)}
        {:<-, _, [left, _]} = node, acc -> {node, pattern_vars(left, acc)}
        {:->, _, [args, _]} = node, acc -> {node, pattern_vars(args, acc)}
        {kind, _, [head | _]} = node, acc when kind in [:def, :defp, :defmacro, :defmacrop] ->
          {node, pattern_vars(head_args(head), acc)}
        node, acc -> {node, acc}
      end)

    bound
  end

  defp head_args({:when, _, [head | _]}), do: head_args(head)
  defp head_args({_, _, args}) when is_list(args), do: args
  defp head_args(_), do: []

  defp pattern_vars(pattern, acc) do
    {_, acc} =
      Macro.prewalk(pattern, acc, fn
        {:^, _, _}, acc -> {:ok, acc}
        {:@, _, _}, acc -> {:ok, acc}
        {name, _, ctx} = var, acc when is_atom(name) and is_atom(ctx) and ctx != nil -> {var, MapSet.put(acc, {name, ctx})}
        node, acc -> {node, acc}
      end)

    acc
  end

  defp preserve_bit_types({name, meta, context}) when is_atom(context) and name in [:integer, :float, :binary, :bitstring, :bytes, :bits, :utf8, :utf16, :utf32, :signed, :unsigned, :native, :big, :little], do: {name, meta, nil}
  defp preserve_bit_types({name, _, arguments} = spec) when name in [:size, :unit] and is_list(arguments), do: spec
  defp preserve_bit_types({name, meta, arguments}) when is_list(arguments), do: {name, meta, Enum.map(arguments, &preserve_bit_types/1)}
  defp preserve_bit_types(spec), do: spec

  defp special_var?(name), do: name in [:__MODULE__, :__ENV__, :__CALLER__, :__DIR__, :__STACKTRACE__]

  defp hash(ctx), do: :erlang.phash2(ctx, 1000)
end
