# A bank account GenServer supervised by a Supervisor, running on tokio.
defmodule Bank.Account do
  use GenServer

  def start_link(opts) do
    GenServer.start_link(__MODULE__, Keyword.get(opts, :balance, 0), name: __MODULE__)
  end

  def deposit(amount), do: GenServer.call(__MODULE__, {:deposit, amount})
  def withdraw(amount), do: GenServer.call(__MODULE__, {:withdraw, amount})
  def balance, do: GenServer.call(__MODULE__, :balance)

  @impl true
  def init(balance), do: {:ok, balance}

  @impl true
  def handle_call({:deposit, amount}, _from, balance) when amount > 0 do
    {:reply, {:ok, balance + amount}, balance + amount}
  end

  def handle_call({:withdraw, amount}, _from, balance) when amount <= balance do
    {:reply, {:ok, balance - amount}, balance - amount}
  end

  def handle_call({:withdraw, _amount}, _from, balance) do
    {:reply, {:error, :insufficient_funds}, balance}
  end

  def handle_call(:balance, _from, balance), do: {:reply, balance, balance}
end

{:ok, _sup} = Supervisor.start_link([{Bank.Account, balance: 100}], strategy: :one_for_one)

IO.inspect(Bank.Account.deposit(50))
IO.inspect(Bank.Account.withdraw(500))
IO.inspect(Bank.Account.withdraw(30))

# Crash the account; the supervisor restarts it with its initial balance.
pid = Process.whereis(Bank.Account)
Process.exit(pid, :kill)
Process.sleep(20)
IO.puts("restarted: #{Process.whereis(Bank.Account) != pid}")
IO.inspect(Bank.Account.balance())

# Fan out work across processes.
1..8
|> Enum.map(fn i -> Task.async(fn -> Enum.sum(1..(i * 1_000_000)) end) end)
|> Task.await_many(30_000)
|> IO.inspect()
