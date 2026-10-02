defmodule ExUnit.Callbacks do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.




















































































































































































































































































































































































































































































































  def on_exit(name_or_ref \\ make_ref(), callback) when is_function(callback, 0) do
    case ExUnit.OnExitHandler.add(self(), name_or_ref, callback) do
      :ok ->
        :ok

      :error ->
        raise ArgumentError, "on_exit/2 callback can only be invoked from the test process"
    end
  end

















































  def start_supervised(child_spec_or_module, opts \\ []) do
    sup =
      case ExUnit.fetch_test_supervisor() do
        {:ok, sup} ->
          sup

        :error ->
          raise ArgumentError, "start_supervised/2 can only be invoked from the test process"
      end

    child_spec = Supervisor.child_spec(child_spec_or_module, opts)
    Supervisor.start_child(sup, child_spec)
  end







  def start_supervised!(child_spec_or_module, opts \\ []) do
    case start_supervised(child_spec_or_module, opts) do
      {:ok, pid} ->
        pid

      {:ok, pid, _info} ->
        pid

      {:error, reason} ->
        raise "failed to start child with the spec #{inspect(child_spec_or_module)}.\n" <>
                "Reason: #{start_supervised_error(reason)}"
    end
  end

  defp start_supervised_error({{:EXIT, reason}, info}) when is_tuple(info),
    do: Exception.format_exit(reason)

  defp start_supervised_error({reason, info}) when is_tuple(info),
    do: Exception.format_exit(reason)

  defp start_supervised_error(reason), do: Exception.format_exit({:start_spec, reason})













  def start_link_supervised!(child_spec_or_module, opts \\ []) do
    pid = start_supervised!(child_spec_or_module, opts)
    Process.link(pid)
    pid
  end















  def stop_supervised(id) do
    case ExUnit.OnExitHandler.get_supervisor(self()) do
      {:ok, nil} ->
        {:error, :not_found}

      {:ok, sup} ->
        pid = pid_for_child(sup, id)

        if pid do
          Process.unlink(pid)
        end

        with :ok <- Supervisor.terminate_child(sup, id) do
          # If the terminated child was temporary, delete_child returns {:error, :not_found}.
          # Since the child was successfully terminated, we treat this result as a success.
          true = Supervisor.delete_child(sup, id) in [:ok, {:error, :not_found}]
          :ok
        end

      :error ->
        raise ArgumentError, "stop_supervised/1 can only be invoked from the test process"
    end
  end

  defp pid_for_child(sup, id) do
    children = Supervisor.which_children(sup)

    with {_id, pid, _type, _modules} <- List.keyfind(children, id, 0) do
      pid
    end
  end






  def stop_supervised!(id) do
    case stop_supervised(id) do
      :ok ->
        :ok

      {:error, :not_found} ->
        raise "could not stop child ID #{inspect(id)} because it was not found"
    end
  end

  ## Helpers

  @reserved [:case, :file, :line, :test, :async, :registered, :describe]
















  def __merge__(_mod, _kind, context, :ok) do
    context
  end

  def __merge__(mod, kind, context, {:ok, value}) do
    unwrapped_merge(mod, kind, context, value, {:ok, value})
  end

  def __merge__(mod, kind, context, value) do
    unwrapped_merge(mod, kind, context, value, value)
  end

  defp unwrapped_merge(mod, kind, _context, %_{}, original_value) do
    raise_merge_failed!(mod, kind, original_value)
  end

  defp unwrapped_merge(mod, kind, context, data, _original_value) when is_list(data) do
    context_merge(mod, kind, context, Map.new(data))
  end

  defp unwrapped_merge(mod, kind, context, data, _original_value) when is_map(data) do
    context_merge(mod, kind, context, data)
  end

  defp unwrapped_merge(mod, kind, _, _return_value, original_value) do
    raise_merge_failed!(mod, kind, original_value)
  end

  defp context_merge(mod, kind, context, data) do
    Map.merge(context, data, fn
      k, v1, v2 when k in @reserved ->
        if v1 == v2, do: v1, else: raise_merge_reserved!(mod, kind, k, v2)

      _, _, v ->
        v
    end)
  end

  defp raise_merge_failed!(mod, kind, return_value) do
    raise "expected ExUnit #{kind} callback in #{inspect(mod)} to " <>
            "return the atom :ok, a keyword, or a map, got #{inspect(return_value)} instead"
  end

  defp raise_merge_reserved!(mod, kind, key, value) do
    raise "ExUnit #{kind} callback in #{inspect(mod)} is trying to set " <>
            "reserved field #{inspect(key)} to #{inspect(value)}"
  end

















































































































































  def __noop__, do: :noop
end

# Imported from Elixir 1.18.3 lib/elixir/lib/../ex_unit/ex_unit/callbacks.ex (docs and specs stripped;
# line numbers match the original).
