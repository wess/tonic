defmodule SoakWorker do
  use GenServer

  def start_link(parent), do: GenServer.start_link(__MODULE__, parent, name: __MODULE__)
  def init(parent) do
    send(parent, {:worker_started, self()})
    {:ok, 0}
  end

  def handle_call({:compute, producer, sequence}, _, count) do
    seed = producer * 1000 + sequence
    mapper = fn n -> %{value: seed + n, text: "retained", nested: {producer, sequence}} end
    values = Enum.map(1..32, mapper)
    result = {Enum.sum(Enum.map(values, & &1.value)), List.last(values).nested, Enum.all?(values, &(&1.text == "retained"))}
    {:reply, result, count + 1}
  end
end

defmodule Soak do
  def run(deadline, cycles, requests, restarts) do
    if System.monotonic_time(:millisecond) < deadline or cycles < 3 do
      parent = self()
      producers = for producer <- 1..4 do
        spawn_monitor(fn ->
          for sequence <- 1..15 do
            expected = 32 * (producer * 1000 + sequence) + 528
            {^expected, {^producer, ^sequence}, true} = GenServer.call(SoakWorker, {:compute, producer, sequence}, 10_000)
          end
          send(parent, {:producer_done, self()})
        end)
      end
      Enum.each(producers, fn {pid, ref} ->
        receive do
          {:producer_done, ^pid} -> :ok
        after
          15_000 -> raise "producer failed to finish"
        end
        receive do
          {:DOWN, ^ref, :process, ^pid, :normal} -> :ok
          {:DOWN, ^ref, :process, ^pid, reason} -> raise "producer failed: #{inspect(reason)}"
        after
          15_000 -> raise "producer failed to exit"
        end
      end)
      old = Process.whereis(SoakWorker)
      ref = Process.monitor(old)
      Process.exit(old, :kill)
      receive do
        {:DOWN, ^ref, :process, ^old, :killed} -> :ok
      after
        15_000 -> raise "worker monitor failed"
      end
      restarted = receive do
        {:worker_started, pid} -> pid
      after
        15_000 -> raise "supervisor failed to restart child"
      end
      unless restarted != old and Process.alive?(restarted), do: raise "invalid child restart"
      run(deadline, cycles + 1, requests + 60, restarts + 1)
    else
      {cycles, requests, restarts}
    end
  end
end

[seconds] = System.argv()
seconds = String.to_integer(seconds)
unless seconds >= 1 and seconds <= 120, do: raise "soak duration must be between 1 and 120 seconds"
Process.flag(:trap_exit, true)
{:ok, supervisor} = Supervisor.start_link([{SoakWorker, self()}], strategy: :one_for_one, max_restarts: 10_000, max_seconds: 1)
receive do
  {:worker_started, _} -> :ok
end
{cycles, requests, restarts} = Soak.run(System.monotonic_time(:millisecond) + seconds * 1000, 0, 0, 0)
unless cycles >= 3 and requests == cycles * 60 and restarts == cycles, do: raise "soak counters failed"
Supervisor.stop(supervisor)
IO.puts("soak:ok cycles=#{cycles} requests=#{requests} restarts=#{restarts}")
