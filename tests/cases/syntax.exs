defmodule Outer do
  defmodule Inner do
    def hello(name \\ "world"), do: "hello #{name}"
  end

  alias Outer.Inner, as: I
  def call, do: I.hello()
  def call2, do: Inner.hello("inner")
end

defmodule Guards do
  defguard is_small(x) when is_integer(x) and x < 10
  def check(x) when is_small(x), do: :small
  def check(x) when x in [10, 20, 30], do: :listed
  def check(x) when x in 100..200, do: :ranged
  def check(_), do: :other
end

defmodule Defaults do
  def f(a, b \\ 2, c \\ 3), do: {a, b, c}
end

defmodule MyMacros do
  defmacro unless_(cond, do: body) do
    quote do
      if unquote(cond), do: nil, else: unquote(body)
    end
  end
end

defmodule UseMacro do
  require MyMacros
  def t, do: MyMacros.unless_(false, do: :ran)
end

IO.puts(Outer.call())
IO.puts(Outer.call2())
IO.inspect(Enum.map([5, 20, 150, 50], &Guards.check/1))
IO.inspect(Defaults.f(1))
IO.inspect(Defaults.f(1, :b))
IO.inspect(Defaults.f(1, :b, :c))
IO.inspect(UseMacro.t())

result =
  [1, 2, 3]
  |> Enum.map(fn x -> x * 10 end)
  |> Enum.filter(&(&1 > 10))
  |> Enum.sum()
IO.puts(result)

doc = """
  Heredoc line 1
    indented
  line 3
  """
IO.write(doc)
IO.puts(~s(sigil "string" #{1 + 1}))
IO.puts(~S(raw #{no}))
add = fn a, b -> a + b end
IO.puts(add.(2, 3))
multi = fn
  {:ok, v} -> "ok #{v}"
  {:error, e} -> "err #{e}"
end
IO.puts(multi.({:ok, 1}))
IO.puts(multi.({:error, :x}))
IO.puts(5 |> then(&(&1 * 2)))
cap = &String.upcase/1
IO.puts(cap.("abc"))
f = &{&1, &2}
IO.inspect(f.(1, 2))
counter = Enum.reduce(1..3, %{}, fn x, acc -> Map.update(acc, rem(x, 2), [x], &[x | &1]) end)
IO.inspect(counter)
import String, only: [upcase: 1]
IO.puts(upcase("imported"))
x = 10
y = if x > 5 do
  "big"
else
  "small"
end
IO.puts(y)
IO.puts(case %{type: :circle, r: 2} do
  %{type: :circle, r: r} -> "circle #{r}"
  _ -> "other"
end)
{:ok, value} = {:ok, [1, 2, 3]}
IO.inspect(value)
list = for n <- 1..3, m <- 1..3, n != m, do: {n, m}
IO.inspect(list)
IO.inspect(Enum.map(1..3, fn
  1 -> :one
  n -> n
end))
IO.inspect(is_nil(nil))
require Integer
IO.inspect(Integer.is_even(4))
IO.inspect(Integer.is_odd(3))
nums = [3, 1, 2]
[first | _] = Enum.sort(nums)
IO.puts(first)
IO.inspect(match?({:ok, _}, {:ok, 1}))
IO.inspect(!true)
IO.inspect(nil || :default)
IO.inspect(false && :x)
IO.inspect(1 in [1, 2])
IO.inspect(5 not in 1..3)
str = "multi
line"
IO.puts(str)
IO.puts(?a)
IO.inspect(?\n)
IO.inspect(0x1F + 0b101 + 0o17 + 1_000)
IO.inspect(Kernel.+(1, 2))
IO.inspect(apply(Enum, :sum, [[1, 2, 3]]))
mod = Enum
IO.inspect(mod.count([1, 2]))
IO.inspect(__MODULE__)
