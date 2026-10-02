defmodule Tree do
  defstruct [:value, left: nil, right: nil]

  def insert(nil, v), do: %Tree{value: v}
  def insert(%Tree{value: x} = t, v) when v < x, do: %{t | left: insert(t.left, v)}
  def insert(%Tree{} = t, v), do: %{t | right: insert(t.right, v)}

  def to_list(nil), do: []
  def to_list(%Tree{left: l, value: v, right: r}), do: to_list(l) ++ [v] ++ to_list(r)

  def depth(nil), do: 0
  def depth(%Tree{left: l, right: r}), do: 1 + max(depth(l), depth(r))
end

defmodule JSON do
  def encode(nil), do: "null"
  def encode(true), do: "true"
  def encode(false), do: "false"
  def encode(n) when is_number(n), do: to_string(n)
  def encode(s) when is_binary(s), do: "\"" <> String.replace(s, "\"", "\\\"") <> "\""
  def encode(a) when is_atom(a), do: encode(Atom.to_string(a))
  def encode(list) when is_list(list), do: "[" <> Enum.map_join(list, ",", &encode/1) <> "]"

  def encode(map) when is_map(map) do
    body = map |> Enum.sort() |> Enum.map_join(",", fn {k, v} -> encode(to_string(k)) <> ":" <> encode(v) end)
    "{" <> body <> "}"
  end
end

defmodule Primes do
  def sieve(n) do
    2..n |> Enum.reduce(MapSet.new(2..n), fn i, set ->
      if MapSet.member?(set, i) do
        Enum.reduce((i * i)..n//i, set, &MapSet.delete(&2, &1))
      else
        set
      end
    end) |> MapSet.to_list() |> Enum.sort()
  end
end

defmodule WordCount do
  def count(text) do
    text
    |> String.downcase()
    |> String.split(~r/[^a-z]+/, trim: true)
    |> Enum.frequencies()
    |> Enum.sort_by(fn {w, c} -> {-c, w} end)
    |> Enum.take(3)
  end
end

t = Enum.reduce([5, 3, 8, 1, 4, 7, 9, 2, 6], nil, &Tree.insert(&2, &1))
IO.inspect(Tree.to_list(t))
IO.inspect(Tree.depth(t))
IO.puts(JSON.encode(%{name: "tonic", tags: ["fast", "native"], version: 1.0, ok: true, extra: nil}))
IO.inspect(Primes.sieve(50))
IO.inspect(WordCount.count("The quick brown fox jumps over the lazy dog. The dog sleeps; the fox runs."))

parent = self()
spawn(fn ->
  defmodule_depth = fn f, n -> if n == 0, do: 0, else: 1 + f.(f, n - 1) end
  send(parent, {:depth, defmodule_depth.(defmodule_depth, 100_000)})
end)
receive do
  {:depth, d} -> IO.inspect(d)
end

fact = fn
  f, 0 -> 1
  f, n -> n * f.(f, n - 1)
end
IO.inspect(fact.(fact, 25))
IO.inspect(Enum.reduce(1..20, 1, &*/2))
IO.inspect(:math.sqrt(16))
IO.inspect(:math.pi())
IO.inspect(Integer.pow(3, 40))
IO.inspect(Integer.gcd(48, 18))
