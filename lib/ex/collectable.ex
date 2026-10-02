defprotocol Collectable do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.

























































































  def into(collectable)
end

defimpl Collectable, for: List do
  def into(list) do
    # TODO: Change the behavior so the into always comes last on Elixir v2.0
    if list != [] do
      IO.warn(
        "the Collectable protocol is deprecated for non-empty lists. The behavior of " <>
          "Enum.into/2 and \"for\" comprehensions with an :into option is incorrect " <>
          "when collecting into non-empty lists. If you're collecting into a non-empty keyword " <>
          "list, consider using Keyword.merge/2 instead. If you're collecting into a non-empty " <>
          "list, consider concatenating the two lists with the ++ operator."
      )
    end

    fun = fn
      list_acc, {:cont, elem} ->
        [elem | list_acc]

      list_acc, :done ->
        list ++ :lists.reverse(list_acc)

      _list_acc, :halt ->
        :ok
    end

    {[], fun}
  end
end

defimpl Collectable, for: BitString do
  def into(binary) when is_binary(binary) do
    fun = fn
      acc, {:cont, x} when is_binary(x) and is_list(acc) ->
        [acc | x]

      acc, {:cont, x} when is_bitstring(x) and is_bitstring(acc) ->
        <<acc::bitstring, x::bitstring>>

      acc, {:cont, x} when is_bitstring(x) ->
        <<IO.iodata_to_binary(acc)::bitstring, x::bitstring>>

      acc, :done when is_bitstring(acc) ->
        acc

      acc, :done ->
        IO.iodata_to_binary(acc)

      __acc, :halt ->
        :ok

      _acc, {:cont, other} ->
        raise ArgumentError,
              "collecting into a binary requires a bitstring, got: #{inspect(other)}"
    end

    {[binary], fun}
  end

  def into(bitstring) do
    fun = fn
      acc, {:cont, x} when is_bitstring(x) ->
        <<acc::bitstring, x::bitstring>>

      acc, :done ->
        acc

      _acc, :halt ->
        :ok

      _acc, {:cont, other} ->
        raise ArgumentError,
              "collecting into a bitstring requires a bitstring, got: #{inspect(other)}"
    end

    {bitstring, fun}
  end
end

defimpl Collectable, for: Map do
  def into(map) do
    fun = fn
      map_acc, {:cont, {key, value}} ->
        Map.put(map_acc, key, value)

      map_acc, :done ->
        map_acc

      _map_acc, :halt ->
        :ok

      _map_acc, {:cont, other} ->
        raise ArgumentError,
              "collecting into a map requires {key, value} tuples, got: #{inspect(other)}"
    end

    {map, fun}
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/collectable.ex (docs and specs stripped;
# line numbers match the original).
