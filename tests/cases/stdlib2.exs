defmodule User do
  @derive {JSON.Encoder, only: [:name, :age]}
  defstruct [:name, :age, :password]
end
defmodule Pt do
  @derive JSON.Encoder
  defstruct x: 0, y: 0
end
IO.inspect JSON.encode!(%{a: 1, b: [true, nil, 1.5, "x\"y\n\u0001é"], c: %{"d" => :e}}) |> JSON.decode!() |> Enum.sort()
IO.inspect JSON.encode!(struct(User, name: "w", age: 3, password: "s")) |> JSON.decode!() |> Enum.sort()
IO.inspect JSON.decode!(JSON.encode!([struct(Pt), ~D[2024-01-02]])) == [%{"x" => 0, "y" => 0}, "2024-01-02"]
IO.inspect JSON.decode(~s([null,123,"string",{"key":"value"}, -1.5e3, 1E2, "\\u00e9\\ud83d\\ude00\\n"]))
IO.inspect JSON.decode("[1,2")
IO.inspect JSON.decode("[1,x]")
IO.inspect JSON.decode("tru")
IO.inspect JSON.decode(~s("\\ud800"))
IO.inspect JSON.decode("{\"a\":1} x")
IO.inspect JSON.decode!(" {\"a\" : [ ] , \"b\":{}} ")
try do
  JSON.decode!("[1,]")
rescue
  e -> IO.inspect e
end
IO.inspect JSON.decode("[1,2]", [], array_push: fn e, a -> [e * 10 | a] end)
IO.inspect JSON.encode_to_iodata!([1]) |> IO.iodata_to_binary()
IO.inspect ~r/a(b)?c/iu
IO.inspect Regex.run(~r/c(?<foo>d)/, "abcd", capture: ["foo", "bar"])
IO.inspect Regex.split(~r//, "abc")
IO.inspect Regex.split(~r/a(?<second>b)c/, "abc", on: [:second])
IO.inspect Regex.split(~r/a(?<second>b)c/, "abc", on: [:second], include_captures: true)
IO.inspect String.match?("josé", ~r/^[[:lower:]]+$/u)
IO.inspect Regex.replace(~r/\s/, "Unicode spaces", "-")
IO.inspect URI.parse("https://elixir-lang.org/")
IO.inspect Regex.scan(~r/\d+/, "a1 b22 c333")
IO.inspect Regex.named_captures(~r/(?<y>\d{4})-(?<m>\d{2})/, "2024-05")
IO.inspect String.split("a,b;c", ~r/[,;]/)
IO.inspect Regex.replace(~r/(\w+)@(\w+)/, "me@x you@y", "\\2 at \\1")
IO.inspect Regex.replace(~r/o/, "foo", fn m -> String.upcase(m) end, global: false)
IO.inspect "hello world" =~ ~r/wor(?=ld)/
IO.inspect Regex.run(~r/(a)(x)?(b)?/, "a")
IO.inspect Regex.names(~r/(?<b>.)(?<a>.)/)
IO.inspect Regex.compile("(")
IO.inspect Regex.escape("a.b*c")
IO.inspect String.replace("a-b-c", ~r/-/, "+")
IO.inspect Regex.run(~r/é/u, "café", return: :index)
:rand.seed(:exsss, {100, 101, 102})
IO.inspect Enum.random([1, 2, 3])
IO.inspect Enum.random([1, 2, 3])
IO.inspect Enum.random(1..1_000)
:rand.seed(:exsss, {1, 2, 3})
IO.inspect Stream.repeatedly(&:rand.uniform/0) |> Enum.take(3)
:rand.seed(:exsss, {1, 2, 3})
IO.inspect Enum.take_random(1..10, 2)
IO.inspect Enum.shuffle(1..10)
IO.inspect :rand.uniform(1_000_000_000_000_000_000_000)
IO.inspect {Float.floor(12.52, 2), Float.ceil(-12.52, 2), Float.round(5.5675, 3), Float.round(12.5), Float.round(-0.01), Float.floor(-56.5), Float.ceil(34.251, 2), Float.round(12.341444444444441, 15), Float.round(2.5678, 2)}
try do Float.round(1.0, 16) rescue e -> IO.inspect e end
IO.inspect Float.ratio(0.75)
IO.inspect Float.ratio(-3.14)
IO.inspect Float.ratio(1.0e20)
IO.inspect String.replace_invalid("asd" <> <<0xFF::8>>)
IO.inspect String.replace_invalid("nem rán b" <> <<225, 187>> <> " bề", "ERROR!")
send(self(), 1)
IO.inspect(receive do: (x when is_integer(x) -> x))
f = fn x -> case x, do: (1 -> :one; _ -> :other) end
IO.inspect {f.(1), f.(2)}
IO.inspect (case 3 do
  3 -> (
    a = 1
    a + 2)
end)
IO.inspect(..)
IO.inspect String.slice("elixir", ..)
IO.inspect Enum.slice([1,2,3], ..)
IO.inspect Enum.slice([1,2,3], 1..-1//1)
x = 1..3
IO.inspect x
IO.inspect binary_slice("elixir", 0..-1//2)
IO.inspect binary_slice("elixir", -4..-1)
IO.inspect binary_slice("elixir", 1, 3)
IO.inspect is_exception(%ArgumentError{}, ArgumentError)
