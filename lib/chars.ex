defprotocol String.Chars do
  def to_string(term)
end
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
defimpl String.Chars, for: Atom do
  def to_string(nil), do: ""
  def to_string(atom), do: Atom.to_string(atom)
end

defimpl String.Chars, for: BitString do
  def to_string(term) when is_binary(term), do: term
end

defimpl String.Chars, for: List do
  def to_string(charlist), do: List.to_string(charlist)
end

defimpl String.Chars, for: Integer do
  def to_string(term), do: Integer.to_string(term)
end

defimpl String.Chars, for: Float do
  def to_string(term), do: :tonic.float_short(term)
end

defprotocol List.Chars do
  def to_charlist(term)
end

defimpl List.Chars, for: Atom do
  def to_charlist(nil), do: []
  def to_charlist(atom), do: Atom.to_charlist(atom)
end

defimpl List.Chars, for: BitString do
  def to_charlist(term) when is_binary(term), do: String.to_charlist(term)
end

defimpl List.Chars, for: List do
  def to_charlist(list), do: list
end

defimpl List.Chars, for: Integer do
  def to_charlist(term), do: Integer.to_charlist(term)
end

defimpl List.Chars, for: Float do
  def to_charlist(term), do: String.to_charlist(:tonic.float_short(term))
end
