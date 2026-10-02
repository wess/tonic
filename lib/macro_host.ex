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
      [cmd, size] when cmd in ["LOAD", "EXPAND", "CALLBACKS", "ATTRS", "AFTER", "EVAL", "LOADCONFIG", "ATTRIBUTE", "DEFINITIONS"] ->
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
    load_modules(quoted, path)
    "ok"
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
    ast = Tonic.Eval.Modules.call_macro(mod, name, args, caller)
    id = :erlang.unique_integer([:positive])
    ast |> hygiene(id) |> Macro.to_string()
  end

  # Load definitions with the file's lexical directives; skip runtime expressions.
  defp load_modules({:__block__, _, forms}, path), do: load_forms(forms, path, [])
  defp load_modules(form, path), do: load_forms([form], path, [])

  defp load_forms(forms, path, directives) do
    Enum.reduce(forms, directives, fn
      {kind, _, _} = form, directives when kind in [:alias, :import, :require] ->
        directives ++ [form]
      {:defmodule, _, _} = form, directives ->
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
      {:defmodule, _, _} = node, _ -> {node, true}
      node, found -> {node, found}
    end)
    found
  end

  defp load_module({:defmodule, _, [name, _]} = form, path, directives) do
    {mod, _} = Tonic.Eval.eval_quoted({:__block__, [], directives ++ [name]}, [], file: path)
    try do
      Tonic.Eval.eval_quoted({:__block__, [], directives ++ [form]}, [], file: path)
      Process.put(:tonic_macro_load_errors, Map.delete(Process.get(:tonic_macro_load_errors), mod))
    rescue
      e -> store_load_error(mod, path, Exception.format(:error, e, __STACKTRACE__))
    catch
      kind, reason -> store_load_error(mod, path, Exception.format(kind, reason, __STACKTRACE__))
    end
  end

  defp store_load_error(nil, path, error), do: raise(CompileError, file: path, description: error)
  defp store_load_error(mod, path, error) do
    errors = Process.get(:tonic_macro_load_errors)
    Process.put(:tonic_macro_load_errors, Map.put(errors, mod, "could not load #{inspect(mod)} from #{path}:\n#{error}"))
  end

  defp check_loaded!(mod) do
    case Map.get(Process.get(:tonic_macro_load_errors), mod) do
      nil -> :ok
      error -> raise CompileError, description: error
    end
  end

  defp handle("ATTRIBUTE", text) do
    {{mod, file, line, name}, _} = Code.eval_string(text)
    check_loaded!(mod)
    value = Tonic.Eval.Modules.attribute_value(mod, file, line, name)
    unless embeddable?(value), do: raise(CompileError, file: file, line: line, description: "@#{name} contains a runtime value that cannot be embedded in native code")
    value |> Tonic.Eval.escape() |> Macro.to_string()
  end

  defp embeddable?(value) when is_atom(value) or is_number(value) or is_bitstring(value), do: true
  defp embeddable?(value) when is_list(value), do: Enum.all?(value, &embeddable?/1)
  defp embeddable?(value) when is_tuple(value), do: value |> Tuple.to_list() |> Enum.all?(&embeddable?/1)
  defp embeddable?(value) when is_map(value), do: Enum.all?(value, fn {key, item} -> embeddable?(key) and embeddable?(item) end)
  defp embeddable?(_), do: false

  defp handle("DEFINITIONS", text) do
    {{module, file, first, last}, _} = Code.eval_string(text)
    check_loaded!(module)
    definitions = Tonic.Eval.Modules.source_definitions(module, file, first, last)
    if definitions == [], do: raise(CompileError, file: file, line: first, description: "module expression did not generate definitions")
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
    Macro.prewalk(ast, fn
      {:__aliases__, meta, _} = a ->
        case Keyword.get(meta, :alias) do
          mod when is_atom(mod) and mod not in [nil, false] -> mod
          _ -> a
        end

      {name, meta, ctx} when is_atom(name) and is_atom(ctx) and ctx != nil and name != :_ ->
        if special_var?(name) do
          {name, meta, ctx}
        else
          {:"#{name}_m#{id}#{hash(ctx)}", meta, nil}
        end

      other ->
        other
    end)
  end

  defp preserve_bit_types({name, meta, context}) when is_atom(context) and name in [:integer, :float, :binary, :bitstring, :bytes, :bits, :utf8, :utf16, :utf32, :signed, :unsigned, :native, :big, :little], do: {name, meta, nil}
  defp preserve_bit_types({name, _, arguments} = spec) when name in [:size, :unit] and is_list(arguments), do: spec
  defp preserve_bit_types({name, meta, arguments}) when is_list(arguments), do: {name, meta, Enum.map(arguments, &preserve_bit_types/1)}
  defp preserve_bit_types(spec), do: spec

  defp special_var?(name), do: name in [:__MODULE__, :__ENV__, :__CALLER__, :__DIR__, :__STACKTRACE__]

  defp hash(ctx), do: :erlang.phash2(ctx, 1000)
end
