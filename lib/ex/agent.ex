defmodule Agent do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.





































































































































































































  def child_spec(arg) do
    %{
      id: Agent,
      start: {Agent, :start_link, [arg]}
    }
  end


  defmacro __using__(opts) do
    quote location: :keep, bind_quoted: [opts: opts] do
      if not Module.has_attribute?(__MODULE__, :doc) do





      end

      def child_spec(arg) do
        default = %{
          id: __MODULE__,
          start: {__MODULE__, :start_link, [arg]}
        }

        Supervisor.child_spec(default, unquote(Macro.escape(opts)))
      end

      defoverridable child_spec: 1
    end
  end














































  def start_link(fun, options \\ []) when is_function(fun, 0) do
    GenServer.start_link(Agent.Server, fun, options)
  end









  def start_link(module, fun, args, options \\ []) do
    GenServer.start_link(Agent.Server, {module, fun, args}, options)
  end














  def start(fun, options \\ []) when is_function(fun, 0) do
    GenServer.start(Agent.Server, fun, options)
  end







  def start(module, fun, args, options \\ []) do
    GenServer.start(Agent.Server, {module, fun, args}, options)
  end






















  def get(agent, fun, timeout \\ 5000) when is_function(fun, 1) do
    GenServer.call(agent, {:get, fun}, timeout)
  end









  def get(agent, module, fun, args, timeout \\ 5000) do
    GenServer.call(agent, {:get, {module, fun, args}}, timeout)
  end


























  def get_and_update(agent, fun, timeout \\ 5000) when is_function(fun, 1) do
    GenServer.call(agent, {:get_and_update, fun}, timeout)
  end









  def get_and_update(agent, module, fun, args, timeout \\ 5000) do
    GenServer.call(agent, {:get_and_update, {module, fun, args}}, timeout)
  end


























  def update(agent, fun, timeout \\ 5000) when is_function(fun, 1) do
    GenServer.call(agent, {:update, fun}, timeout)
  end


















  def update(agent, module, fun, args, timeout \\ 5000) do
    GenServer.call(agent, {:update, {module, fun, args}}, timeout)
  end





















  def cast(agent, fun) when is_function(fun, 1) do
    GenServer.cast(agent, {:cast, fun})
  end


















  def cast(agent, module, fun, args) do
    GenServer.cast(agent, {:cast, {module, fun, args}})
  end




















  def stop(agent, reason \\ :normal, timeout \\ :infinity) do
    GenServer.stop(agent, reason, timeout)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/agent.ex (docs and specs stripped;
# line numbers match the original).
