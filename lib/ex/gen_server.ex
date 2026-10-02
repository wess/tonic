defmodule GenServer do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.






















































































































































































































































































































































































































































































































































































































































































































































































































  # TODO: Remove this on v2.0




  @optional_callbacks code_change: 3,
                      terminate: 2,
                      handle_info: 2,
                      handle_cast: 2,
                      handle_call: 3,
                      format_status: 1,
                      format_status: 2,
                      handle_continue: 2






































  defmacro __using__(opts) do
    quote location: :keep, bind_quoted: [opts: opts] do
      @behaviour GenServer

      if not Module.has_attribute?(__MODULE__, :doc) do





      end

      def child_spec(init_arg) do
        default = %{
          id: __MODULE__,
          start: {__MODULE__, :start_link, [init_arg]}
        }

        Supervisor.child_spec(default, unquote(Macro.escape(opts)))
      end

      defoverridable child_spec: 1

      # TODO: Remove this on v2.0
      @before_compile GenServer


      def handle_call(msg, _from, state) do
        proc =
          case Process.info(self(), :registered_name) do
            {_, []} -> self()
            {_, name} -> name
          end

        # We do this to trick Dialyzer to not complain about non-local returns.
        case :erlang.phash2(1, 1) do
          0 ->
            raise "attempted to call GenServer #{inspect(proc)} but no handle_call/3 clause was provided"

          1 ->
            {:stop, {:bad_call, msg}, state}
        end
      end


      def handle_info(msg, state) do
        proc =
          case Process.info(self(), :registered_name) do
            {_, []} -> self()
            {_, name} -> name
          end

        :logger.error(
          %{
            label: {GenServer, :no_handle_info},
            report: %{
              module: __MODULE__,
              message: msg,
              name: proc
            }
          },
          %{
            domain: [:otp, :elixir],
            error_logger: %{tag: :error_msg},
            report_cb: &GenServer.format_report/1
          }
        )

        {:noreply, state}
      end


      def handle_cast(msg, state) do
        proc =
          case Process.info(self(), :registered_name) do
            {_, []} -> self()
            {_, name} -> name
          end

        # We do this to trick Dialyzer to not complain about non-local returns.
        case :erlang.phash2(1, 1) do
          0 ->
            raise "attempted to cast GenServer #{inspect(proc)} but no handle_cast/2 clause was provided"

          1 ->
            {:stop, {:bad_cast, msg}, state}
        end
      end


      def terminate(_reason, _state) do
        :ok
      end


      def code_change(_old, state, _extra) do
        {:ok, state}
      end

      defoverridable code_change: 3, terminate: 2, handle_info: 2, handle_cast: 2, handle_call: 3
    end
  end

  defmacro __before_compile__(env) do
    if not Module.defines?(env.module, {:init, 1}) do
      message = """
      function init/1 required by behaviour GenServer is not implemented \
      (in module #{inspect(env.module)}).

      We will inject a default implementation for now:

          def init(init_arg) do
            {:ok, init_arg}
          end

      You can copy the implementation above or define your own that converts \
      the arguments given to GenServer.start_link/3 to the server state.
      """

      IO.warn(message, env)

      quote do

        def init(init_arg) do
          {:ok, init_arg}
        end

        defoverridable init: 1
      end
    end
  end















































  def start_link(module, init_arg, options \\ []) when is_atom(module) and is_list(options) do
    do_start(:link, module, init_arg, options)
  end







  def start(module, init_arg, options \\ []) when is_atom(module) and is_list(options) do
    do_start(:nolink, module, init_arg, options)
  end

  defp do_start(link, module, init_arg, options) do
    case Keyword.pop(options, :name) do
      {nil, opts} ->
        :gen.start(:gen_server, link, module, init_arg, opts)

      {atom, opts} when is_atom(atom) ->
        :gen.start(:gen_server, link, {:local, atom}, module, init_arg, opts)

      {{:global, _term} = tuple, opts} ->
        :gen.start(:gen_server, link, tuple, module, init_arg, opts)

      {{:via, via_module, _term} = tuple, opts} when is_atom(via_module) ->
        :gen.start(:gen_server, link, tuple, module, init_arg, opts)

      {other, _} ->
        raise ArgumentError, """
        expected :name option to be one of the following:

          * nil
          * atom
          * {:global, term}
          * {:via, module, term}

        Got: #{inspect(other)}
        """
    end
  end













  def stop(server, reason \\ :normal, timeout \\ :infinity) do
    case whereis(server) do
      nil ->
        exit({:noproc, {__MODULE__, :stop, [server, reason, timeout]}})

      pid when pid == self() ->
        exit({:calling_self, {__MODULE__, :stop, [server, reason, timeout]}})

      pid ->
        try do
          :proc_lib.stop(pid, reason, timeout)
        catch
          :exit, err ->
            exit({err, {__MODULE__, :stop, [server, reason, timeout]}})
        end
    end
  end
























  def call(server, request, timeout \\ 5000)
      when (is_integer(timeout) and timeout >= 0) or timeout == :infinity do
    case whereis(server) do
      nil ->
        exit({:noproc, {__MODULE__, :call, [server, request, timeout]}})

      pid ->
        try do
          :gen.call(pid, :"$gen_call", request, timeout)
        catch
          :exit, reason ->
            exit({reason, {__MODULE__, :call, [server, request, timeout]}})
        else
          {:ok, res} -> res
        end
    end
  end













  def cast(server, request)

  def cast({:global, name}, request) do
    try do
      :global.send(name, cast_msg(request))
      :ok
    catch
      _, _ -> :ok
    end
  end

  def cast({:via, mod, name}, request) do
    try do
      mod.send(name, cast_msg(request))
      :ok
    catch
      _, _ -> :ok
    end
  end

  def cast({name, node}, request) when is_atom(name) and is_atom(node),
    do: do_send({name, node}, cast_msg(request))

  def cast(dest, request) when is_atom(dest) or is_pid(dest), do: do_send(dest, cast_msg(request))










  def abcast(nodes \\ [node() | Node.list()], name, request)
      when is_list(nodes) and is_atom(name) do
    msg = cast_msg(request)
    _ = for node <- nodes, do: do_send({name, node}, msg)
    :abcast
  end

  defp cast_msg(req) do
    {:"$gen_cast", req}
  end

  defp do_send(dest, msg) do
    try do
      send(dest, msg)
      :ok
    catch
      _, _ -> :ok
    end
  end




























  def multi_call(nodes \\ [node() | Node.list()], name, request, timeout \\ :infinity) do
    :gen_server.multi_call(nodes, name, request, timeout)
  end
































  def reply(client, reply) do
    :gen.reply(client, reply)
  end


















  def whereis(server)

  def whereis(pid) when is_pid(pid), do: pid

  def whereis(name) when is_atom(name) do
    Process.whereis(name)
  end

  def whereis({:global, name}) do
    case :global.whereis_name(name) do
      pid when is_pid(pid) -> pid
      :undefined -> nil
    end
  end

  def whereis({:via, mod, name}) do
    case apply(mod, :whereis_name, [name]) do
      pid when is_pid(pid) -> pid
      :undefined -> nil
    end
  end

  def whereis({name, local}) when is_atom(name) and local == node() do
    Process.whereis(name)
  end

  def whereis({name, node} = server) when is_atom(name) and is_atom(node) do
    server
  end


  def format_report(%{
        label: {GenServer, :no_handle_info},
        report: %{module: mod, message: msg, name: proc}
      }) do
    {~c"~p ~p received unexpected message in handle_info/2: ~p~n", [mod, proc, msg]}
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/gen_server.ex (docs and specs stripped;
# line numbers match the original).
