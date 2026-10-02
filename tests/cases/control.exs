defmodule Control do
  def classify(n) when is_integer(n) and n < 0, do: :negative
  def classify(0), do: :zero
  def classify(n) when is_integer(n), do: :positive
  def classify(_), do: :not_a_number

  def fizzbuzz(n) do
    cond do
      rem(n, 15) == 0 -> "FizzBuzz"
      rem(n, 3) == 0 -> "Fizz"
      rem(n, 5) == 0 -> "Buzz"
      true -> Integer.to_string(n)
    end
  end
end

IO.inspect(Enum.map([-5, 0, 5, :x], &Control.classify/1))
IO.puts(Enum.map_join(1..15, " ", &Control.fizzbuzz/1))

if true, do: IO.puts("yes"), else: IO.puts("no")
unless false do
  IO.puts("unless ok")
end

result =
  with {:ok, a} <- {:ok, 1},
       {:ok, b} <- {:ok, a + 1} do
    a + b
  end

IO.inspect(result)

r2 =
  with {:ok, a} <- {:ok, 1},
       {:ok, _b} <- {:error, :nope} do
    a
  else
    {:error, reason} -> {:failed, reason}
  end

IO.inspect(r2)

IO.inspect(for x <- 1..5, do: x * x)
IO.inspect(for x <- 1..10, rem(x, 2) == 0, do: x)
IO.inspect(for x <- [1, 2], y <- [:a, :b], do: {x, y})
IO.inspect((for {k, v} <- %{a: 1, b: 2}, into: %{}, do: {k, v * 10}) |> Enum.sort())
IO.inspect(for x <- 1..5, reduce: 0 do
  acc -> acc + x
end)
IO.inspect(for <<c <- "abc">>, do: c)
IO.inspect(for x <- [1, 1, 2, 3, 3], uniq: true, do: x)

try do
  raise "boom"
rescue
  e in RuntimeError -> IO.puts("rescued: " <> e.message)
end

try do
  raise ArgumentError, "bad arg"
rescue
  e -> IO.inspect(e)
end

try do
  throw(:thrown)
catch
  :throw, v -> IO.inspect({:caught, v})
end

try do
  exit(:bye)
catch
  :exit, reason -> IO.inspect({:exit, reason})
end

x =
  try do
    1 / 0
  rescue
    ArithmeticError -> :div_by_zero
  after
    IO.puts("after runs")
  end

IO.inspect(x)

try do
  %{a: 1}.b
rescue
  e in KeyError -> IO.puts(Exception.message(e))
end

try do
  {:ok, _} = {:error, 1}
rescue
  e in MatchError -> IO.puts(Exception.message(e))
end

try do
  case 3 do
    1 -> :one
  end
rescue
  e -> IO.puts(Exception.message(e))
end

try do
  Control.classify()
rescue
  e -> IO.puts(Exception.message(e))
catch
  kind, v -> IO.inspect({kind, v})
end
