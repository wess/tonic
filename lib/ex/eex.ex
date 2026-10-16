defmodule EEx.SyntaxError do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
  defexception [:file, :line, :column, :snippet, message: "syntax error"]


  def message(exception) do
    %{file: file, line: line, column: column, message: message, snippet: snippet} = exception

    Exception.format_file_line_column(file && Path.relative_to_cwd(file), line, column, " ") <>
      message <> (snippet || "")
  end
end

defmodule EEx do




























































































































  defmacro function_from_string(kind, name, template, args \\ [], options \\ []) do
    quote bind_quoted: binding() do
      info = Keyword.merge([file: __ENV__.file, line: __ENV__.line], options)
      args = Enum.map(args, fn arg -> {arg, [line: info[:line]], nil} end)
      compiled = EEx.compile_string(template, info)

      case kind do
        :def -> def unquote(name)(unquote_splicing(args)), do: unquote(compiled)
        :defp -> defp unquote(name)(unquote_splicing(args)), do: unquote(compiled)
      end
    end
  end































  defmacro function_from_file(kind, name, file, args \\ [], options \\ []) do
    quote bind_quoted: binding() do
      info = Keyword.merge([file: IO.chardata_to_string(file), line: 1], options)
      args = Enum.map(args, fn arg -> {arg, [line: 1], nil} end)
      compiled = EEx.compile_file(file, info)

      @external_resource file
      @file file
      case kind do
        :def -> def unquote(name)(unquote_splicing(args)), do: unquote(compiled)
        :defp -> defp unquote(name)(unquote_splicing(args)), do: unquote(compiled)
      end
    end
  end

























  def compile_string(source, options \\ []) when is_binary(source) and is_list(options) do
    case tokenize(source, options) do
      {:ok, tokens} ->
        EEx.Compiler.compile(tokens, source, options)

      {:error, message, %{column: column, line: line}} ->
        file = options[:file] || "nofile"
        raise EEx.SyntaxError, file: file, line: line, column: column, message: message
    end
  end





























  def compile_file(filename, options \\ []) when is_list(options) do
    filename = IO.chardata_to_string(filename)
    options = Keyword.merge([file: filename, line: 1], options)
    compile_string(File.read!(filename), options)
  end













  def eval_string(source, bindings \\ [], options \\ [])
      when is_binary(source) and is_list(bindings) and is_list(options) do
    compiled = compile_string(source, options)
    do_eval(compiled, bindings, options)
  end

















  def eval_file(filename, bindings \\ [], options \\ [])
      when is_list(bindings) and is_list(options) do
    filename = IO.chardata_to_string(filename)
    options = Keyword.put_new(options, :file, filename)
    compiled = compile_file(filename, options)
    do_eval(compiled, bindings, options)
  end


































  def tokenize(contents, opts \\ []) do
    EEx.Compiler.tokenize(contents, opts)
  end

  ### Helpers

  defp do_eval(compiled, bindings, options) do
    {result, _} = Code.eval_quoted(compiled, bindings, options)
    result
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../../eex/lib/eex.ex (docs and specs stripped;
# line numbers match the original).
