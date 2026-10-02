defmodule E do
  def run(f) do
    try do
      f.()
    rescue
      e ->
        message = Exception.message(e)
        message = if e.__struct__ == BadArityError, do: Regex.replace(~r/#Function<\d+\.\d+ in/, message, "#Function<id in"), else: message
        IO.puts("#{inspect(e.__struct__)}: #{message}")
    catch
      kind, val -> IO.puts("caught #{kind}: #{inspect(val)}")
    end
  end
end

E.run(fn -> 1 / 0 end)
E.run(fn -> :erlang.error(:badarg) end)
E.run(fn -> String.to_integer("abc") end)
E.run(fn -> Map.fetch!(%{a: 1}, :b) end)
E.run(fn -> Keyword.fetch!([a: 1], :b) end)
E.run(fn -> elem({1, 2}, 5) end)
E.run(fn -> hd([]) end)
E.run(fn -> [1, 2] ++ :x end)
E.run(fn -> String.upcase(:atom) end)
E.run(fn -> Enum.at(:not_enum, 1) end)
E.run(fn -> raise "plain message" end)
E.run(fn -> raise ArgumentError end)
E.run(fn -> raise ArgumentError, "custom arg" end)
E.run(fn -> throw({:my, :value}) end)
E.run(fn -> exit(:normal_exit) end)
E.run(fn -> apply(String, :nope, [1]) end)
E.run(fn -> x = 5; case x do 1 -> :one end end)
E.run(fn -> {:ok, y} = {:error, 1}; y end)
E.run(fn -> cond do false -> 1 end end)
E.run(fn -> with {:ok, a} <- {:ok, 1}, {:ok, b} <- {:error, a} do b else {:fail, _} -> 0 end end)
E.run(fn -> Integer.parse(nil) end)
E.run(fn -> :ok = :not_ok end)
E.run(fn -> fn x -> x end.(1, 2) end)
E.run(fn -> 1 + :a end)
E.run(fn -> List.first(:x) end)
E.run(fn -> Enum.fetch!([1, 2], 10) end)
E.run(fn -> Enum.max([]) end)
E.run(fn -> String.to_existing_atom("definitely_not_an_existing_atom_xyz") end)
E.run(fn -> :math.log(-1) end)
E.run(fn -> Map.update!(%{}, :k, & &1) end)
E.run(fn -> struct!(URI, nope: 1) end)
E.run(fn -> Atom.to_string("str") end)
E.run(fn -> Tuple.append(:x, 1) end)
E.run(fn -> String.duplicate("a", -1) end)
E.run(fn -> Integer.to_string(1.5) end)
E.run(fn -> binary_part("abc", 2, 5) end)
E.run(fn -> <<x::8>> = "ab"; x end)
E.run(fn -> raise KeyError, key: :k, term: %{a: 1} end)
e = RuntimeError.exception("boom")
IO.inspect(e)
IO.puts(Exception.format(:error, e))
IO.puts(Exception.format(:throw, :thing))
IO.puts(Exception.format(:exit, {:shutdown, :x}))
IO.inspect(Exception.message(%ArgumentError{message: "m"}))
try do
  raise "x"
rescue
  e in [RuntimeError, ArgumentError] -> IO.puts("multi #{e.message}")
end
try do
  :ok
rescue
  _ -> :never
else
  :ok -> IO.puts("else branch")
after
  IO.puts("after branch")
end
r = try do
  throw(:t)
catch
  :throw, v -> {:caught, v}
end
IO.inspect(r)
IO.inspect(catch_it = (try do exit(:bye) catch :exit, r -> r end))
_ = catch_it
