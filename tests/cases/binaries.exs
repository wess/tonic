defmodule Parse do
  def count_chars(<<>>, acc), do: acc
  def count_chars(<<_::utf8, rest::binary>>, acc), do: count_chars(rest, acc + 1)

  def header(<<version::8, len::16, payload::binary-size(len), rest::binary>>) do
    {version, len, payload, rest}
  end
end
IO.inspect(Parse.count_chars("héllo wörld", 0))
IO.inspect(Parse.header(<<1, 0, 3, "abc", "rest">>))
IO.inspect(<<1, 2, 3>>)
IO.inspect(<<104, 101, 108, 108, 111>>)
IO.inspect(<<256::16>>)
IO.inspect(<<-1::32-signed>>)
<<a::4-binary, _::binary>> = "abcdefg"
IO.inspect(a)
IO.inspect(byte_size(<<1::32, 2::little-16>>))
<<x::little-16>> = <<1, 2>>
IO.inspect(x)
<<f::float>> = <<64, 9, 33, 251, 84, 68, 45, 24>>
IO.inspect(f)
IO.inspect(<<3.5::float>>)
s = "a" <> <<0>>
IO.inspect(s)
IO.inspect(:binary.bin_to_list("AB"))
IO.inspect(for <<c::utf8 <- "añb">>, do: c)
IO.inspect(String.codepoints("añb"))
IO.inspect(<<"x", ?y, "z">>)
IO.inspect("tab\there \"quoted\" \\ back")
IO.inspect("new\nline")
IO.inspect("\#{not interp}")
