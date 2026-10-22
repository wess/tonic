defmodule Portability.Captures do
  def identity(value), do: value
  def handlers, do: %{identity: &__MODULE__.identity/1, upper: &String.upcase/1}
end

defmodule Portability.Attributes do
  @handlers Portability.Captures.handlers()
  def invoke(name, value), do: Map.fetch!(@handlers, name).(value)
  def handler, do: @handlers.identity
end

IO.inspect({Portability.Attributes.invoke(:identity, :portable), Portability.Attributes.invoke(:upper, "héllo")})
IO.inspect(Function.info(Portability.Attributes.handler(), :type))

values = [nil, false, true, 0, [], "", {}, %{}]
IO.inspect(Enum.map(values, fn value -> {value && :yes, value || :no, !value} end))
IO.inspect({1 == 1.0, 1 === 1.0, map_size(%{1 => :integer, 1.0 => :float})})
IO.inspect(Enum.map([nil, false, 1, 1.0, "1", [], {}], &is_integer/1))
IO.inspect({byte_size("é"), String.length("é"), length(String.to_charlist("é"))})
IO.inspect(Keyword.get_values([x: 1, x: 2], :x))
IO.inspect(div(1_000_000_000_000_000_000_000 * 3, 3))

defmodule Portability.Guards do
  defguard nonnegative(value) when is_integer(value) and value >= 0
  def classify(value) when length(value) > 0, do: :list
  def classify(_), do: :other
end

IO.inspect({Portability.Guards.classify(:invalid), Portability.Guards.classify([1])})
defmodule Portability.Expansion do
  require Portability.Guards
  def run do
    expanded = Macro.expand_once(quote(do: Portability.Guards.nonnegative(-1)), __ENV__)
    {result, _} = Code.eval_quoted(expanded)
    {result, Portability.Guards.__info__(:macros)}
  end
end
IO.inspect(Portability.Expansion.run())

defmodule Portability.Docs do
  @doc "consumed by the next definition"
  def documented, do: :ok
  @consumed Module.get_attribute(__MODULE__, :doc)
  def consumed, do: @consumed
end
IO.inspect(Portability.Docs.consumed())
IO.inspect(try do
  Map.fetch!(%{}, :missing)
rescue
  error in KeyError -> {error.__struct__, error.key}
end)

parent = self()
{pid, reference} = spawn_monitor(fn -> send(parent, {:first, 1}); send(parent, {:second, 2}) end)
receive do {:second, 2} -> :ok end
receive do {:first, 1} -> IO.puts("selective receive retained the first message") end
receive do {:DOWN, ^reference, :process, ^pid, :normal} -> IO.puts("monitored exit") end

module = Module.concat([Enum.join(["Str", "ing"])])
function = String.to_atom(Enum.join(["up", "case"]))
IO.puts(apply(module, function, ["portable"]))
