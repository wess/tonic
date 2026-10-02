for source <- ["(", ")", "*", "a**", "[", "[z-a]"] do
  IO.inspect({source, Regex.compile(source)})
end

IO.inspect(Regex.run(~r/(?<word>caf\x{e9})(?=!)/iu, "CAFÉ!", capture: :all_names))
IO.inspect(Regex.match?(~r/^(\w+)\s+\1$/, "same same"))
IO.inspect(Regex.match?(~r/foo(?!bar)/, "foobar"))
