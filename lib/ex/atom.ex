defmodule Atom do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.























































  def to_string(atom) do
    :erlang.atom_to_binary(atom)
  end













  def to_charlist(atom) do
    :erlang.atom_to_list(atom)
  end




  def to_char_list(atom), do: Atom.to_charlist(atom)
end

# Imported from Elixir 1.18.3 lib/elixir/lib/atom.ex (docs and specs stripped;
# line numbers match the original).
