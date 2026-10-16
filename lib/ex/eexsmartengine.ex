defmodule EEx.SmartEngine do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.


































  @behaviour EEx.Engine


  defdelegate init(opts), to: EEx.Engine


  defdelegate handle_body(state), to: EEx.Engine


  defdelegate handle_begin(state), to: EEx.Engine


  defdelegate handle_end(state), to: EEx.Engine


  defdelegate handle_text(state, meta, text), to: EEx.Engine


  def handle_expr(state, marker, expr) do
    expr = Macro.prewalk(expr, &EEx.Engine.handle_assign/1)
    EEx.Engine.handle_expr(state, marker, expr)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../../eex/lib/eex/smart_engine.ex (docs and specs stripped;
# line numbers match the original).
