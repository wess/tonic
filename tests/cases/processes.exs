defmodule Counter do
  use GenServer

  def start_link(initial), do: GenServer.start_link(__MODULE__, initial, name: __MODULE__)
  def increment, do: GenServer.cast(__MODULE__, :inc)
  def get, do: GenServer.call(__MODULE__, :get)

  @impl true
  def init(initial), do: {:ok, initial}

  @impl true
  def handle_cast(:inc, n), do: {:noreply, n + 1}

  @impl true
  def handle_call(:get, _from, n), do: {:reply, n, n}
end

defmodule Stack do
  use GenServer

  def init(items), do: {:ok, items}
  def handle_call(:pop, _from, [h | t]), do: {:reply, h, t}
  def handle_call(:pop, _from, []), do: {:reply, nil, []}
  def handle_cast({:push, x}, items), do: {:noreply, [x | items]}
end

parent = self()
pid = spawn(fn -> send(parent, {:hello, self()}) end)

receive do
  {:hello, ^pid} -> IO.puts("got hello from child")
end

pids = for i <- 1..10, do: spawn(fn -> send(parent, {:square, i, i * i}) end)
IO.puts(length(pids))

results =
  for _ <- 1..10 do
    receive do
      {:square, i, sq} -> {i, sq}
    end
  end

IO.inspect(Enum.sort(results))

receive do
  :never -> :ok
after
  50 -> IO.puts("timeout ok")
end

{:ok, _} = Counter.start_link(10)
Counter.increment()
Counter.increment()
IO.puts(Counter.get())

{:ok, stack} = GenServer.start_link(Stack, [1, 2])
GenServer.cast(stack, {:push, 0})
IO.inspect(GenServer.call(stack, :pop))
IO.inspect(GenServer.call(stack, :pop))

{:ok, agent} = Agent.start_link(fn -> %{} end)
Agent.update(agent, &Map.put(&1, :k, :v))
IO.inspect(Agent.get(agent, & &1))

task = Task.async(fn -> Enum.sum(1..100) end)
IO.puts(Task.await(task))

tasks = Enum.map(1..5, fn i -> Task.async(fn -> i * 10 end) end)
IO.inspect(Task.await_many(tasks))

ref = make_ref()
send(self(), {ref, :self_msg})
receive do
  {^ref, m} -> IO.inspect(m)
end

Process.flag(:trap_exit, true)
child = spawn_link(fn -> exit(:crashed) end)
receive do
  {:EXIT, ^child, reason} -> IO.inspect({:exit, reason})
end

{pid2, mref} = spawn_monitor(fn -> :ok end)
receive do
  {:DOWN, ^mref, :process, ^pid2, reason} -> IO.inspect({:down, reason})
end

# many processes ring
n = 1000
first =
  Enum.reduce(1..n, self(), fn _, next ->
    spawn(fn ->
      receive do
        {:token, t} -> send(next, {:token, t + 1})
      end
    end)
  end)

send(first, {:token, 0})
receive do
  {:token, t} -> IO.puts("ring done: #{t}")
end

Process.put(:key, 42)
IO.inspect(Process.get(:key))
IO.inspect(Process.alive?(self()))
