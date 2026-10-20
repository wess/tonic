# Behaviour fixed while preparing the beta; each section once diverged from Elixir.

defmodule Regressions.Accumulate do
  Module.register_attribute(__MODULE__, :acc, accumulate: true)
  Module.register_attribute(__MODULE__, :none, accumulate: true)
  @acc :a
  @acc {:b, 1}
  def acc, do: @acc
  def none, do: {@none, length(@none)}
  @acc String.upcase("dyn")
  def later, do: @acc
end

defmodule Regressions.Routes do
  defmacro __using__(_) do
    quote do
      import Regressions.Routes
      Module.register_attribute(__MODULE__, :routes, accumulate: true)
      @before_compile Regressions.Routes
    end
  end

  defmacro get(path, to: handler), do: quote(do: @routes({unquote(path), unquote(handler)}))
  defmacro __before_compile__(_env), do: quote(do: def(routes, do: Enum.reverse(@routes)))
end

defmodule Regressions.Router do
  use Regressions.Routes
  get "/", to: :index
  get "/x", to: :x
end

defmodule Regressions.Sigils do
  defmacro sigil_q({:<<>>, _, [s]}, mods), do: {String.upcase(s), mods}
  def sigil_LIST(s, []), do: String.split(s, ",")
  def sigil_i(s, ~c"n"), do: -String.to_integer(s)
  def sigil_i(s, []), do: String.to_integer(s)
end

defmodule Regressions.UseSigils do
  import Regressions.Sigils
  def run(x), do: {~q(hello), ~q[a b]xy, ~LIST(a,b,c), ~i(42), ~i(42)n, ~i(#{x})}
end

IO.inspect({Regressions.Accumulate.acc(), Regressions.Accumulate.none(), Regressions.Accumulate.later()})
IO.inspect(Regressions.Router.routes())
IO.inspect(Regressions.UseSigils.run(7))

# binary generators
IO.inspect(for <<x <- <<1, 2::4>> >>, do: x)
IO.inspect(for <<c::utf8 <- <<"ab", 255, "c">> >>, do: c)
IO.inspect(for <<1, x <- <<1, 2, 3, 4, 1, 5>> >>, do: x)
IO.inspect(for <<len, data::binary-size(len) <- <<2, "ab", 1, "c">> >>, do: data)

# floats
IO.inspect([192_757_651_696_438.125, 1_524_698_487_110_590.25, 0.1 + 0.2])

for {f, d} <- [{2.5, 0}, {0.125, 2}, {0.045, 2}, {1.45, 1}, {-2.5, 0}, {1.0e22, 0}, {1.0, 253}] do
  IO.inspect({f, d, :erlang.float_to_binary(f, decimals: d) |> byte_size(),
              :erlang.float_to_binary(f, [:compact, decimals: min(d, 20)])})
end

IO.inspect(try do :erlang.float_to_binary(1.0, decimals: 254) rescue e -> e.__struct__ end)
IO.inspect(try do :erlang.float(Integer.pow(2, 1024)) rescue e -> e.__struct__ end)
IO.inspect(try do String.to_integer("5", 37) rescue e -> e.__struct__ end)
z = -0.0
IO.inspect([0.0 === z, 0.0 == z, Map.get(%{0.0 => :a}, z), MapSet.size(MapSet.new([0.0, z]))])

# integer limits
import Bitwise
IO.inspect(try do bsl(1, 1_000_000_000_000) rescue e -> e.__struct__ end)
IO.inspect(try do bsl(1, 4_194_240) rescue e -> e.__struct__ end)
IO.inspect({bsr(1, 2 ** 64), bsr(-1, 2 ** 64), bsl(0, 2 ** 64)})

# imported OTP modules and filled-in functions
IO.inspect({:base64.encode("hello world"), :base64.decode("aGk="), :base64.encode(<<255, 254>>, %{mode: :urlsafe, padding: false})})
a = :array.from_list([1, 2, :x, 4], :x)
IO.inspect({:array.foldl(fn i, v, acc -> [{i, v} | acc] end, [], a), :array.sparse_foldr(fn i, v, acc -> [{i, v} | acc] end, [], a)})
IO.inspect(Enum.map([{2024, 12, 30}, {2021, 1, 3}, {2026, 10, 15}], &:calendar.iso_week_number/1))
IO.inspect({:lists.seq(1, 10, 3), :lists.seq(10, 1, -4), :lists.seq(1, 1, 0)})
IO.inspect(try do :lists.seq(5, 1, 1) rescue e -> e.__struct__ end)
IO.inspect(try do System.fetch_env!("TONIC_SURELY_UNSET") rescue e -> {e.__struct__, Exception.message(e)} end)
IO.inspect(:erlang.binary_part("hello", {5, -2}))
IO.inspect(:timer.hms(1, 2, 3))

# ets counters across collections
t = :ets.new(:counters, [])
:ets.insert(t, {:k, 0, :x})
for _ <- 1..3_000, do: :ets.update_counter(t, :k, 1)
IO.inspect(:ets.lookup(t, :k))
IO.inspect(:ets.update_counter(t, :k, [{2, 100, 3_050, 7}, {2, -1}]))
:ets.insert(t, {:big, 2 ** 70})
IO.inspect(:ets.update_counter(t, :big, 1))
IO.inspect(try do :ets.update_counter(t, :k, {3, 1}) rescue e -> e.__struct__ end)

# ets chunked select
o = :ets.new(:ordered, [:ordered_set])
for k <- 1..7, do: :ets.insert(o, {k})
{first, cont} = :ets.select(o, [{{:"$1"}, [], [:"$1"]}], 3)
{second, cont} = :ets.select(cont)
{third, cont} = :ets.select(cont)
IO.inspect({first, second, third, :ets.select(cont)})

# atomics are atomic
ref = :atomics.new(1, [])
cas = fn cas ->
  old = :atomics.get(ref, 1)
  if :atomics.compare_exchange(ref, 1, old, old + 1) != :ok, do: cas.(cas)
end
1..8 |> Enum.map(fn _ -> Task.async(fn -> for _ <- 1..500, do: cas.(cas) end) end) |> Task.await_many()
IO.inspect(:atomics.get(ref, 1))
u = :atomics.new(1, signed: false)
:atomics.sub(u, 1, 1)
IO.inspect({:atomics.get(u, 1), :atomics.exchange(u, 1, 5), :atomics.info(u).max})
c = :counters.new(2, [])
for _ <- 1..2_000, do: :counters.add(c, 2, 3)
IO.inspect({:counters.get(c, 1), :counters.get(c, 2)})

# processes and signals
IO.inspect(length(Process.list()) >= 1)
p = spawn(fn -> Process.sleep(:infinity) end)
Process.exit(p, :kill)
IO.inspect(Process.alive?(p))
ref = Process.monitor(p)
receive do
  {:DOWN, ^ref, :process, _, reason} -> IO.inspect(reason)
end

w = spawn(fn -> receive do :stop -> :ok end end)
Process.register(w, :regressions_watched)
ref = Process.monitor(:regressions_watched)
send(w, :stop)
receive do
  {:DOWN, ^ref, :process, object, reason} -> IO.inspect({object, reason})
end

ref = Process.monitor(:regressions_nobody)
receive do
  {:DOWN, ^ref, :process, object, reason} -> IO.inspect({object, reason})
end

{:ok, tref} = :timer.send_after(5, :hi)
receive do
  :hi -> IO.inspect(elem(tref, 0))
end

{:ok, interval} = :timer.send_interval(2, :tick)
for _ <- 1..3, do: (receive do :tick -> :ok end)
IO.inspect(:timer.cancel(interval))
IO.inspect(:timer.cancel(:bad))

# appends extend buffers in place; earlier views never change
base = String.duplicate("a", 100)
x = base <> "b"
y = x <> "c"
z = x <> "d"
w = y <> "e"
IO.inspect({byte_size(x), binary_part(y, 100, 2), binary_part(z, 100, 2), binary_part(w, 100, 3), y == z})
views = Enum.scan(1..300, "", fn i, acc -> acc <> <<rem(i, 256)>> end)
IO.inspect(Enum.with_index(views, 1) |> Enum.all?(fn {v, i} -> byte_size(v) == i and :binary.last(v) == rem(i, 256) end))
built = Enum.reduce(1..3_000, "", fn i, acc -> acc <> Integer.to_string(i) end)
IO.inspect({byte_size(built), :erlang.md5(built) |> Base.encode16()})
send(self(), {:copy, w})
receive do
  {:copy, copy} -> IO.inspect(copy == w and :erlang.binary_to_term(:erlang.term_to_binary(w)) == w)
end

# definitions generated by any module-level call, as Decimal 3.2 does with Enum.reduce
defmodule Regressions.Generated do
  Enum.reduce(0..5, 1, fn int, acc ->
    defp pow10(unquote(int)), do: unquote(acc)
    acc * 10
  end)

  defp pow10(n) when n > 5, do: 10 * pow10(n - 1)

  Enum.flat_map([:a, :b], fn n ->
    def unquote(:"is_#{n}")(x), do: x == unquote(n)
    []
  end)

  def run, do: {Enum.map(0..8, &pow10/1), is_a(:a), is_b(:a)}
end

IO.inspect(Regressions.Generated.run())

# second verification round
defmodule Regressions.Hooks do
  defmacro hook(do: body) do
    quote do
      def go(var!(payload)), do: unquote(body)
    end
  end

  defguard is_pos(x) when is_integer(x) and x > 0
end

defmodule Regressions.Second do
  require Regressions.Hooks
  require Record
  Record.defrecord(:user, id: nil, name: "x")
  Regressions.Hooks.hook do
    payload * 2
  end

  defguardp is_digit(c) when c in ?0..?9
  defmacrop twice(x), do: quote(do: unquote(x) * 2)
  def captures(l), do: {Enum.filter(l, &is_digit/1), Enum.map([1, 2], &twice/1), Enum.filter([-1, 3], &Regressions.Hooks.is_pos/1)}
  def records, do: {user({:user, 1, "n"}), user(user(id: 3)), kind({:user, 1, 2}), kind({:other})}
  defp kind(x) when Record.is_record(x, :user), do: :user
  defp kind(x) when Record.is_record(x), do: :record
  @callback cb(integer) :: integer
end

defprotocol Regressions.Shape do
  def name(s, prefix)
  def area(s)
end

a = 1
b = 2
IO.inspect(case %{{1, 2} => :hit} do %{{^a, ^b} => v} -> v end)
IO.inspect({Regressions.Second.go(21), Regressions.Second.captures(~c"1a2"), Regressions.Second.records()})
IO.inspect({%{1.5 => :a, 7 => :b}, Map.keys(%{2.0 => :a, 3 => :c, -1 => :d}), MapSet.new([1, 2, 1.0])})
IO.inspect({Regressions.Shape.__protocol__(:functions), Regressions.Shape.__protocol__(:module)})
IO.inspect({:erlang.bitstring_to_list(<<255, 1::3>>), :erlang.list_to_bitstring([1, <<3::2>>])})
IO.inspect({Regressions.Second.behaviour_info(:callbacks), Regressions.Second.module_info(:module)})
IO.inspect(try do Tuple.duplicate(0, 16_777_216) rescue e -> e.__struct__ end)
owner = self()
protected = :ets.new(:protected, [:protected])
private = :ets.new(:private, [:private])
:ets.insert(protected, {1, 1})
other = fn f -> spawn(fn -> send(owner, {:r, (try do f.() rescue e -> e.__struct__ end)}) end); receive do {:r, r} -> r end end
IO.inspect([other.(fn -> :ets.insert(protected, {2}) end), other.(fn -> :ets.lookup(protected, 1) end), other.(fn -> :ets.lookup(private, 1) end)])

defmodule Regressions.Docs do
  @moduledoc "module docs"
  @doc "f docs"
  def f, do: {@moduledoc, @doc}
  def g, do: @doc
end

defmodule Regressions.Expand do
  defmacro up(x), do: quote(do: String.upcase(unquote(x)))
end

# record fields from a module-level variable (Mint's HTTP/2 frames)
defmodule Regressions.Frames do
  import Record
  shared = [:stream_id, {:flags, 0x00}]
  defrecord :data, shared ++ [:data, :padding]
  defrecord :ping, shared ++ [:opaque]

  def run do
    d = data(stream_id: 3, data: "x")
    {d, data(d, :flags), match?(data(stream_id: 3), d), ping()}
  end
end

# compile-time code Mint and Req use: attributes in a private macro's quotes
# and unquotes, Bitwise in unquote, `Mod.unquote(name)()` patterns, and the
# evaluator's apply/3, struct!/2 and update_in/2 on modules it interprets
defmodule Regressions.Words do
  @word 7
  @ones Enum.reduce(1..@word, 0, fn _, acc -> Bitwise.bor(Bitwise.bsl(acc, 8), 0x01) end)
  @highs @ones * 0x80

  defmacrop has_byte(word), do: quote(do: Bitwise.band(unquote(word) - @ones, @highs))
  defmacrop scaled(n), do: quote(do: unquote(@ones * 2) * unquote(n))

  def run(x), do: {has_byte(x), scaled(1), lowest(0x8000), :erlang.size({1, 2}), :erlang.size("abc")}

  for k <- 0..(@word - 1) do
    defp lowest(mask) when Bitwise.band(mask, unquote(Bitwise.bsl(0x80, 8 * k))) != 0, do: unquote(k)
  end
end

defmodule Regressions.Dispatch do
  require Regressions.Frames
  alias Regressions.Frames

  for name <- [:data, :ping] do
    defp kind(Frames.unquote(name)() = frame), do: {unquote(name), elem(frame, 1)}
  end

  def run, do: {kind(Frames.data(stream_id: 1)), kind(Frames.ping(stream_id: 2))}
end

defmodule Regressions.HostStruct do
  defstruct a: 1, b: [], set: MapSet.new()
  def new(options), do: struct!(__MODULE__, options)
  def grow(s), do: update_in(s.b, &[0 | &1])
end

defmodule Regressions.HostUse do
  @made apply(Regressions.HostStruct, :new, [[b: [2]]]) |> Regressions.HostStruct.grow()
  def run, do: {@made.a, @made.b}
end

defmodule Regressions.Third do
  import Regressions.Expand
  def expansions do
    env = __ENV__
    for q <- [quote(do: unless(a, do: b)), quote(do: x |> f() |> g(1)), quote(do: a || b), quote(do: !a), quote(do: up("x"))] do
      {Macro.to_string(Macro.expand(q, env)), Macro.to_string(Macro.expand_once(q, env))}
    end
  end
  def unrequired, do: Regressions.Expand.up("y")
  def line(x) do
    {a, b} = x
    a + b
  end
end

trace = fn f -> try do f.() rescue e -> {e.__struct__, Enum.map(Enum.take(__STACKTRACE__, 1), fn {m, f, a, l} -> {m, f, if(is_list(a), do: length(a), else: a), l[:line]} end)} end end
IO.inspect({Regressions.Docs.f(), Regressions.Docs.g()})
IO.inspect(Regressions.Third.expansions())
IO.inspect(trace.(fn -> Regressions.Third.unrequired() end))
IO.inspect(trace.(fn -> Regressions.Third.line({1, 2, 3}) end))
IO.inspect(trace.(fn -> abs(:a) end))
IO.inspect(Regressions.Frames.run())
IO.inspect({Regressions.Words.run(0x4242424242), Regressions.Dispatch.run(), Regressions.HostUse.run()})
