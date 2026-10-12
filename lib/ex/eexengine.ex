defmodule EEx.Engine do






































































  defmacro __using__(_) do
    quote do
      @behaviour EEx.Engine

      def init(opts) do
        EEx.Engine.init(opts)
      end

      def handle_body(state) do
        EEx.Engine.handle_body(state)
      end

      def handle_begin(state) do
        EEx.Engine.handle_begin(state)
      end

      def handle_end(state) do
        EEx.Engine.handle_end(state)
      end

      def handle_text(state, text) do
        EEx.Engine.handle_text(state, [], text)
      end

      def handle_expr(state, marker, expr) do
        EEx.Engine.handle_expr(state, marker, expr)
      end

      defoverridable EEx.Engine
    end
  end

















  def handle_assign({:@, meta, [{name, _, atom}]}) when is_atom(name) and is_atom(atom) do
    line = meta[:line] || 0
    quote(line: line, do: EEx.Engine.fetch_assign!(var!(assigns), unquote(name)))
  end

  def handle_assign(arg) do
    arg
  end


  # TODO: Raise on v2.0

  def fetch_assign!(assigns, key) do
    case Access.fetch(assigns, key) do
      {:ok, val} ->
        val

      :error ->
        keys = Enum.map(assigns, &elem(&1, 0))

        IO.warn(
          "assign @#{key} not available in EEx template. " <>
            "Please ensure all assigns are given as options. " <>
            "Available assigns: #{inspect(keys)}"
        )

        nil
    end
  end


  def init(_opts) do
    %{
      binary: [],
      dynamic: [],
      vars_count: 0
    }
  end


  def handle_begin(state) do
    check_state!(state)
    %{state | binary: [], dynamic: []}
  end


  def handle_end(quoted) do
    handle_body(quoted)
  end


  def handle_body(state) do
    check_state!(state)
    %{binary: binary, dynamic: dynamic} = state
    binary = {:<<>>, [], Enum.reverse(binary)}
    dynamic = [binary | dynamic]
    {:__block__, [], Enum.reverse(dynamic)}
  end


  def handle_text(state, _meta, text) do
    check_state!(state)
    %{binary: binary} = state
    %{state | binary: [text | binary]}
  end


  def handle_expr(state, "=", ast) do
    check_state!(state)
    %{binary: binary, dynamic: dynamic, vars_count: vars_count} = state
    var = Macro.var(:"arg#{vars_count}", __MODULE__)

    ast =
      quote do
        unquote(var) = String.Chars.to_string(unquote(ast))
      end

    segment =
      quote do
        unquote(var) :: binary
      end

    %{state | dynamic: [ast | dynamic], binary: [segment | binary], vars_count: vars_count + 1}
  end

  def handle_expr(state, "", ast) do
    %{dynamic: dynamic} = state
    %{state | dynamic: [ast | dynamic]}
  end

  def handle_expr(_state, marker, _ast) when marker in ["/", "|"] do
    raise EEx.SyntaxError,
          "unsupported EEx syntax <%#{marker} %> (the syntax is valid but not supported by the current EEx engine)"
  end

  defp check_state!(%{binary: _, dynamic: _, vars_count: _}), do: :ok

  defp check_state!(state) do
    raise "unexpected EEx.Engine state: #{inspect(state)}. " <>
            "This typically means a bug or an outdated EEx.Engine or tool"
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../../eex/lib/eex/engine.ex (docs and specs stripped;
# line numbers match the original).
