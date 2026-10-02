defmodule Tuple do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.




























































  def duplicate(data, size) when is_integer(size) and size >= 0 do
    :erlang.make_tuple(size, data)
  end




















  def insert_at(tuple, index, value) when is_integer(index) and index >= 0 do
    :erlang.insert_element(index + 1, tuple, value)
  end



  def append(tuple, value) do
    :erlang.append_element(tuple, value)
  end


















  def delete_at(tuple, index) when is_integer(index) and index >= 0 do
    :erlang.delete_element(index + 1, tuple)
  end















  def sum(tuple), do: sum(tuple, tuple_size(tuple))

  defp sum(_tuple, 0), do: 0
  defp sum(tuple, index), do: :erlang.element(index, tuple) + sum(tuple, index - 1)















  def product(tuple), do: product(tuple, tuple_size(tuple))

  defp product(_tuple, 0), do: 1
  defp product(tuple, index), do: :erlang.element(index, tuple) * product(tuple, index - 1)
















  def to_list(tuple) do
    :erlang.tuple_to_list(tuple)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/tuple.ex (docs and specs stripped;
# line numbers match the original).
