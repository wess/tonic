defmodule ExUnit.EventManager do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
  @timeout :infinity










  def start_link() do
    {:ok, sup} = DynamicSupervisor.start_link(strategy: :one_for_one)
    {:ok, event} = :gen_event.start_link()
    {:ok, {sup, event}}
  end

  def stop({sup, event}) do
    for {_, pid, _, _} <- DynamicSupervisor.which_children(sup) do
      GenServer.stop(pid, :normal, @timeout)
    end

    DynamicSupervisor.stop(sup)
    :gen_event.stop(event)
  end

  def add_handler({sup, event}, handler, opts) do
    if Code.ensure_loaded?(handler) and function_exported?(handler, :handle_call, 2) do
      IO.warn(
        "passing GenEvent handlers (#{inspect(handler)} in this case) in " <>
          "the :formatters option of ExUnit is deprecated, please pass a " <>
          "GenServer instead. Check the documentation for the ExUnit.Formatter " <>
          "module for more information"
      )

      :gen_event.add_handler(event, handler, opts)
    else
      DynamicSupervisor.start_child(sup, %{
        id: GenServer,
        start: {GenServer, :start_link, [handler, opts]},
        restart: :temporary
      })
    end
  end

  def suite_started(manager, opts) do
    notify(manager, {:suite_started, opts})
  end

  def suite_finished(manager, times_us) do
    notify(manager, {:suite_finished, times_us})
  end

  def module_started(manager, test_module) do
    # TODO: Remove case_started on v2.0
    notify(manager, {:case_started, Map.put(test_module, :__struct__, ExUnit.TestCase)})
    notify(manager, {:module_started, test_module})
  end

  def module_finished(manager, test_module) do
    # TODO: Remove case_finished on v2.0
    notify(manager, {:case_finished, Map.put(test_module, :__struct__, ExUnit.TestCase)})
    notify(manager, {:module_finished, test_module})
  end

  def sigquit(manager, current) do
    notify(manager, {:sigquit, current})
  end

  def test_started(manager, test) do
    notify(manager, {:test_started, test})
  end

  def test_finished(manager, test) do
    notify(manager, {:test_finished, test})
  end

  def max_failures_reached(manager) do
    notify(manager, :max_failures_reached)
  end

  defp notify({sup, event}, msg) do
    :gen_event.notify(event, msg)

    for {_, pid, _, _} <- Supervisor.which_children(sup) do
      GenServer.cast(pid, msg)
    end

    :ok
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../ex_unit/ex_unit/event_manager.ex (docs and specs stripped;
# line numbers match the original).
