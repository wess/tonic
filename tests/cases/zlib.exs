# :zlib one-shot helpers, streams, dictionaries and errors. Exact bytes are
# printed only for small inputs: larger outputs can vary with the zlib build,
# and byte 9 of a gzip header is the OS code (19 on macOS, 3 on Linux).
defmodule Z do
  def text(0, _s, acc), do: IO.iodata_to_binary(acc)

  def text(n, s, acc) do
    s = rem(s * 1_103_515_245 + 12345, 2_147_483_648)
    text(n - 1, s, [acc, rem(div(s, 65536), 23) + ?a])
  end

  def attempt(f) do
    f.()
  rescue
    e -> {:raised, Map.get(e, :original, Exception.message(e))}
  catch
    k, v -> {k, v}
  end

  def gzip_header(b), do: binary_part(b, 0, 9)
end

big = :binary.copy("The quick brown fox jumps over the lazy dog. ", 2000)
noise = Z.text(50_000, 42, [])

IO.inspect(:zlib.compress("hello hello hello"))
IO.inspect(:zlib.zip("hello"))
IO.inspect(Z.gzip_header(:zlib.gzip("hello")))
IO.inspect(binary_part(:zlib.gzip("hello"), 10, 15))
IO.inspect({:zlib.compress(""), :zlib.zip("")})
IO.inspect({:zlib.uncompress(:zlib.compress(["ab", ?c | "de"])), :zlib.unzip(:zlib.zip("hi")), :zlib.gunzip(:zlib.gzip("yo"))})

for d <- [big, noise] do
  IO.inspect({:zlib.gunzip(:zlib.gzip(d)) == d, :zlib.uncompress(:zlib.compress(d)) == d, :zlib.unzip(:zlib.zip(d)) == d})
end

for level <- [:none, :best_speed, :best_compression, 4],
    strategy <- [:default, :filtered, :huffman_only, :rle],
    {bits, mem} <- [{15, 8}, {-15, 8}, {31, 9}, {9, 1}] do
  z = :zlib.open()
  :ok = :zlib.deflateInit(z, level, :deflated, bits, mem, strategy)
  out = :zlib.deflate(z, noise, :finish)
  :ok = :zlib.deflateEnd(z)
  :ok = :zlib.inflateInit(z, bits)
  back = :zlib.inflate(z, out)
  :ok = :zlib.inflateEnd(z)
  :ok = :zlib.close(z)
  if IO.iodata_to_binary(back) != noise, do: IO.inspect({:mismatch, level, strategy, bits, mem})
end

z = :zlib.open()
:ok = :zlib.deflateInit(z)
parts = [:zlib.deflate(z, "abc"), :zlib.deflate(z, "def", :sync), :zlib.deflate(z, big, :full), :zlib.deflate(z, [], :finish)]
IO.inspect(Enum.take(parts, 2))
:ok = :zlib.deflateEnd(z)
:ok = :zlib.inflateInit(z)
IO.inspect(IO.iodata_to_binary(:zlib.inflate(z, parts)) == "abcdef" <> big)
:ok = :zlib.inflateEnd(z)
:ok = :zlib.close(z)

z = :zlib.open()
:ok = :zlib.deflateInit(z, :best_speed)
a = :zlib.deflate(z, big)
:ok = :zlib.deflateParams(z, :best_compression, :filtered)
b = :zlib.deflate(z, noise, :finish)
IO.inspect(:zlib.uncompress([a, b]) == big <> noise)
:ok = :zlib.deflateReset(z)
IO.inspect(IO.iodata_to_binary(:zlib.deflate(z, "again", :finish)))
:ok = :zlib.deflateEnd(z)
:ok = :zlib.close(z)

dict = "hello world dictionary"
z = :zlib.open()
:ok = :zlib.deflateInit(z)
IO.inspect(:zlib.deflateSetDictionary(z, dict))
c = IO.iodata_to_binary(:zlib.deflate(z, "hello world hello dictionary", :finish))
IO.inspect(c)
:ok = :zlib.deflateEnd(z)
:ok = :zlib.inflateInit(z)
IO.inspect(:zlib.inflate(z, c, exception_on_need_dict: false))
:ok = :zlib.inflateSetDictionary(z, dict)
IO.inspect(:zlib.inflate(z, []))
IO.inspect(:zlib.inflateGetDictionary(z))
:ok = :zlib.inflateEnd(z)
:ok = :zlib.inflateInit(z)
IO.inspect(Z.attempt(fn -> :zlib.inflate(z, c) end))
:ok = :zlib.inflateSetDictionary(z, dict)
IO.inspect(:zlib.inflate(z, []))
:ok = :zlib.inflateEnd(z)
:ok = :zlib.close(z)

z = :zlib.open()
:ok = :zlib.inflateInit(z)
drain = fn drain, res, acc ->
  case res do
    {:continue, out} -> drain.(drain, :zlib.safeInflate(z, []), [acc, out])
    {:finished, out} -> [acc, out]
  end
end
IO.inspect(IO.iodata_to_binary(drain.(drain, :zlib.safeInflate(z, :zlib.compress(big)), [])) == big)
:ok = :zlib.inflateEnd(z)
:ok = :zlib.close(z)

IO.inspect(:zlib.gunzip(:zlib.gzip("one") <> :zlib.gzip("two")))
z = :zlib.open()
:ok = :zlib.inflateInit(z, 15, :cut)
IO.inspect(:zlib.inflate(z, :zlib.compress("x") <> "trailing"))
IO.inspect(:zlib.inflate(z, "more"))
:ok = :zlib.inflateEnd(z)
:ok = :zlib.inflateInit(z, 15, :error)
IO.inspect(:zlib.inflate(z, :zlib.compress("x")))
IO.inspect(Z.attempt(fn -> :zlib.inflate(z, "x") end))
IO.inspect(Z.attempt(fn -> :zlib.inflateEnd(z) end))
:ok = :zlib.close(z)

IO.inspect(Z.attempt(fn -> :zlib.uncompress("short") end))
IO.inspect(Z.attempt(fn -> :zlib.uncompress("not zlib data at all") end))
IO.inspect(Z.attempt(fn -> :zlib.uncompress(:foo) end))
IO.inspect(Z.attempt(fn -> :zlib.gunzip("not gzip data") end))
IO.inspect(Z.attempt(fn -> :zlib.unzip(<<255, 255, 255>>) end))
IO.inspect(Z.attempt(fn -> :zlib.gzip(:foo) end))
IO.inspect(Z.attempt(fn -> :zlib.uncompress(binary_part(:zlib.compress(big), 0, 100)) end))
IO.inspect(Z.attempt(fn -> :zlib.gunzip(binary_part(:zlib.gzip(big), 0, 50)) end))

z = :zlib.open()
IO.inspect(Z.attempt(fn -> :zlib.deflate(z, "x") end))
IO.inspect(Z.attempt(fn -> :zlib.inflateEnd(z) end))
IO.inspect(Z.attempt(fn -> :zlib.deflateInit(z, 10) end))
IO.inspect(Z.attempt(fn -> :zlib.deflateInit(z, :default, :deflated, 7, 8, :default) end))
IO.inspect(Z.attempt(fn -> :zlib.deflateInit(z, :default, :deflated, -8, 8, :default) end))
IO.inspect(Z.attempt(fn -> :zlib.deflateInit(z, :default, :stored, 15, 8, :default) end))
IO.inspect(Z.attempt(fn -> :zlib.deflateInit(z, :default, :deflated, 15, 0, :default) end))
IO.inspect(Z.attempt(fn -> :zlib.deflateInit(z, :default, :deflated, 15, 8, :fast) end))
IO.inspect(Z.attempt(fn -> :zlib.inflateInit(z, 15, :maybe) end))
:ok = :zlib.deflateInit(z)
IO.inspect(Z.attempt(fn -> :zlib.inflateInit(z) end))
IO.inspect(Z.attempt(fn -> :zlib.deflate(z, "x", :sometimes) end))
_ = :zlib.deflate(z, "pending")
IO.inspect(Z.attempt(fn -> :zlib.deflateEnd(z) end))
IO.inspect(Z.attempt(fn -> :zlib.deflateEnd(z) end))
IO.inspect(:zlib.close(z))
IO.inspect(Z.attempt(fn -> :zlib.close(make_ref()) end))

z = :zlib.open()
IO.inspect(is_reference(z))
parent = self()
spawn(fn -> send(parent, Z.attempt(fn -> :zlib.deflateInit(z) end)) end)
receive do
  m -> IO.inspect(m)
end
other = spawn(fn -> receive do: (:go -> send(parent, {:other, :zlib.deflateInit(z)})) end)
:ok = :zlib.set_controlling_process(z, other)
send(other, :go)
receive do
  m -> IO.inspect(m)
end
IO.inspect(Z.attempt(fn -> :zlib.deflate(z, "x") end))
