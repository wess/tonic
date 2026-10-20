# :erlang.phash2/1,2 and :erlang.phash/2 must equal OTP's values bit for bit.
# Pids, refs and local funs are left out: their identity differs per runtime.
defmodule PH do
  def show(t) do
    IO.puts("#{inspect(t, limit: 8, printable_limit: 16)} " <>
      "#{:erlang.phash2(t)} #{:erlang.phash2(t, 1000)} #{:erlang.phash2(t, 4294967296)} " <>
      "#{:erlang.phash(t, 65536)} #{:erlang.phash(t, 4294967296)}")
  end

  def try_range(f) do
    try do
      f.()
    rescue
      e -> IO.puts(Exception.message(e))
    end
  end
end

big = Bitwise.bsl(1, 59)
integers =
  [0, 1, -1, 255, 256, -255, 134_217_727, 134_217_728, -134_217_728, -134_217_729,
   4_294_967_295, 4_294_967_296, -4_294_967_296, big - 1, big, -big, -big - 1,
   Bitwise.bsl(1, 62) - 1, Bitwise.bsl(1, 62), -Bitwise.bsl(1, 62) - 1, Bitwise.bsl(1, 64) - 1,
   Bitwise.bsl(1, 64), -Bitwise.bsl(1, 64), Integer.pow(3, 100), -Integer.pow(7, 77)]
Enum.each(integers, &PH.show/1)

floats = [0.0, -0.0, 1.0, -1.0, 0.5, 1.0e300, -2.5e-308, 5.0e-324, 3.141592653589793, 1.0e15]
Enum.each(floats, &PH.show/1)

atoms = [:a, :ok, nil, true, false, :"", :"Elixir.Enum", :"héllo", :"ÿ", :"日本語", :"€", :"with space",
         :abcdefghijklmnopqrstuvwxyz_abcdefghijklmnopqrstuvwxyz]
Enum.each(atoms, &PH.show/1)

whole = :erlang.list_to_binary(Enum.to_list(0..255))
<<_::3, odd::bitstring>> = "hello world"
bins =
  [<<>>, "a", "abc", "abcdefghijkl", "abcdefghijklm", whole, :binary.copy(whole, 20),
   binary_part(whole, 7, 50), binary_part(whole, 255, 0), <<1::1>>, <<5::3>>, <<1, 2, 3::4>>,
   <<"twelve bytes", 3::7>>, odd, binary_part("prefix-and-suffix", 7, 3)]
Enum.each(bins, &PH.show/1)

lists =
  [[], [1], [1, 2, 3], ~c"abcd", ~c"abcde", ~c"abcdefgh", [255, 256, 0], [1 | 2], [1, 2 | :tail],
   [:a, 1, :b, 2], [-1, 1, 2, 3, 4, 5], ~c"héllo", [[], [[]]], ["bin" | "tail"], Enum.to_list(1..300),
   [1, 2, 3, {:x}, 4, 5, 6, 7, 8], [~c"ab", 99 | ~c"cd"]]
Enum.each(lists, &PH.show/1)

tuples = [{}, {1}, {:a, :b}, {{}, {{}}}, {1, "two", 3.0, [4], %{5 => 6}}, List.to_tuple(Enum.to_list(1..40))]
Enum.each(tuples, &PH.show/1)

small = Map.new(1..10, &{&1, &1 * &1})
large = Map.new(1..100, &{&1, Integer.to_string(&1)})
maps =
  [%{}, %{a: 1}, %{1 => 2, 2 => 1}, %{2 => 1, 1 => 2}, small, large, %{%{a: 1} => %{b: [2]}},
   %{large => small}, Map.new(1..40, &{{&1}, [&1]}), %{nil => [], [] => nil},
   %{"key" => <<1::1>>, 1.5 => -0.0}, Map.delete(large, 50)]
Enum.each(maps, &PH.show/1)

PH.show(%URI{host: "example.com", port: 443, scheme: "https"})
PH.show(%{a: %{b: %{c: %{d: [1, {2, %{e: "f"}}]}}}})
PH.show({:ok, [name: "x", tags: [:a, :b]], %{list: ~c"chars", float: 2.5}})
PH.show(Enum.to_list(1..5000))
PH.show(Enum.map(1..200, &{&1, :erlang.integer_to_binary(&1)}))

for f <- [&Enum.map/2, &:erlang.phash2/1, &String.length/1, :erlang.make_fun(:foo, :bar, 3)] do
  IO.puts("#{:erlang.phash2(f)} #{:erlang.phash(f, 65536)}")
end

# phash2 equality follows term equality
IO.inspect(:erlang.phash2(0.0) == :erlang.phash2(-0.0))
IO.inspect(:erlang.phash2(1) == :erlang.phash2(1.0))
IO.inspect(:erlang.phash2(Map.new(1..50, &{&1, &1})) == :erlang.phash2(Map.new(50..1//-1, &{&1, &1})))

for r <- [0, -1, 1.0, :a, 4_294_967_297] do
  PH.try_range(fn -> :erlang.phash2(:x, r) end)
  PH.try_range(fn -> :erlang.phash(:x, r) end)
end
IO.inspect({:erlang.phash2(:x, 1), :erlang.phash(:x, 1)})
