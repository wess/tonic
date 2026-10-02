defmodule :global do
  # Single-node :global name registry.
  def register_name(name, pid), do: register_name(name, pid, nil)

  def register_name(name, pid, _resolve) do
    ensure()

    if :ets.insert_new(:"$tonic_global", {name, pid}) do
      :yes
    else
      case :ets.lookup(:"$tonic_global", name) do
        [{_, old}] -> if Process.alive?(old), do: :no, else: (:ets.insert(:"$tonic_global", {name, pid}); :yes)
        [] -> :no
      end
    end
  end

  def whereis_name(name) do
    ensure()

    case :ets.lookup(:"$tonic_global", name) do
      [{_, pid}] -> if Process.alive?(pid), do: pid, else: :undefined
      [] -> :undefined
    end
  end

  def unregister_name(name) do
    ensure()
    :ets.delete(:"$tonic_global", name)
    :ok
  end

  def send(name, msg) do
    case whereis_name(name) do
      :undefined -> :erlang.error(:badarg, [name, msg])
      pid -> Kernel.send(pid, msg)
    end
  end

  def registered_names do
    ensure()
    for {n, pid} <- :ets.tab2list(:"$tonic_global"), Process.alive?(pid), do: n
  end

  defp ensure do
    if :ets.whereis(:"$tonic_global") == :undefined do
      parent = self()
      # The table is owned by a long-lived process so it survives callers.
      spawn(fn ->
        try do
          :ets.new(:"$tonic_global", [:set, :public, :named_table])
        rescue
          _ -> :ok
        end

        Kernel.send(parent, :tonic_global_ready)
        receive do
          :tonic_never -> :ok
        end
      end)

      receive do
        :tonic_global_ready -> :ok
      end
    end

    :ok
  end
end
