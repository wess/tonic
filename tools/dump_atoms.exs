# Dumps the BEAM atom table as it exists when a script starts running
# (everything created before this script was tokenized), one atom per line in
# index order. tonic uses it to give atoms the same relative order as the BEAM,
# which decides the iteration/printing order of map keys.
#
#   elixir tools/dump_atoms.exs compiler/src/beam_atoms.txt
marker = :tonic_atom_dump_marker_7f3a
tab = for i <- 0..(:erlang.system_info(:atom_count) - 1), do: :erlang.binary_to_term(<<131, 75, i::24>>)
start = Enum.find_index(tab, &(&1 === marker))
names =
  for a <- Enum.take(tab, start) do
    :erlang.atom_to_binary(a) |> String.replace("\\", "\\\\") |> String.replace("\n", "\\n")
  end
File.write!(hd(System.argv()), Enum.join(names, "\n") <> "\n")
