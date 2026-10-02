defmodule :array do
  def new(), do: {:tonic_array, 0, false, :undefined, %{}}
  def new(size) when is_integer(size) and size >= 0, do: {:tonic_array, size, true, :undefined, %{}}
  def new(options) when is_list(options) do
    Enum.reduce(options, new(), fn
      size, array when is_integer(size) and size >= 0 -> resize(size, array) |> fix()
      {:size, size}, array when is_integer(size) and size >= 0 -> resize(size, array) |> fix()
      :fixed, array -> fix(array)
      {:fixed, true}, array -> fix(array)
      {:fixed, false}, array -> relax(array)
      {:default, value}, {:tonic_array, size, fixed, _, entries} -> {:tonic_array, size, fixed, value, entries}
      _, _ -> :erlang.error(:badarg)
    end)
  end
  def new(option), do: new([option])
  def new(size, options), do: new([size | List.wrap(options)])

  def is_array({:tonic_array, size, fixed, _, entries}), do: is_integer(size) and size >= 0 and is_boolean(fixed) and is_map(entries)
  def is_array(_), do: false
  def size({:tonic_array, size, _, _, _}), do: size
  def size(_), do: :erlang.error(:badarg)
  def default({:tonic_array, _, _, default, _}), do: default
  def default(_), do: :erlang.error(:badarg)
  def is_fix({:tonic_array, _, fixed, _, _}), do: fixed
  def fix({:tonic_array, size, _, default, entries}), do: {:tonic_array, size, true, default, entries}
  def relax({:tonic_array, size, _, default, entries}), do: {:tonic_array, size, false, default, entries}

  def get(index, {:tonic_array, size, fixed, default, entries}) when is_integer(index) and index >= 0 do
    if fixed and index >= size, do: :erlang.error(:badarg), else: if(index >= size, do: default, else: Map.get(entries, index, default))
  end
  def get(_, _), do: :erlang.error(:badarg)
  def set(index, value, {:tonic_array, size, fixed, default, entries}) when is_integer(index) and index >= 0 do
    if fixed and index >= size, do: :erlang.error(:badarg)
    entries = if value === default, do: Map.delete(entries, index), else: Map.put(entries, index, value)
    {:tonic_array, max(size, index + 1), fixed, default, entries}
  end
  def set(_, _, _), do: :erlang.error(:badarg)
  def reset(index, {:tonic_array, size, fixed, default, entries}) when is_integer(index) and index >= 0 do
    if fixed and index >= size, do: :erlang.error(:badarg)
    entries = if index >= size, do: entries, else: Map.delete(entries, index)
    {:tonic_array, size, fixed, default, entries}
  end
  def reset(_, _), do: :erlang.error(:badarg)
  def resize(array), do: resize(sparse_size(array), array)
  def resize(size, {:tonic_array, _, fixed, default, entries}) when is_integer(size) and size >= 0 do
    {:tonic_array, size, fixed, default, entries}
  end
  def resize(_, _), do: :erlang.error(:badarg)
  def sparse_size(array), do: Enum.reduce(sparse_to_orddict(array), 0, fn {index, _}, size -> max(size, index + 1) end)

  def from_orddict(entries), do: from_orddict(entries, :undefined)
  def from_orddict(entries, default) when is_list(entries) do
    build_orddict(entries, {:tonic_array, 0, false, default, %{}}, -1)
  end
  defp build_orddict([], array, _), do: array
  defp build_orddict([{index, value} | rest], array, last) when is_integer(index) and index > last do
    build_orddict(rest, set(index, value, array), index)
  end
  defp build_orddict(remaining, _, _), do: :erlang.error({:badarg, remaining})
  def from_orddict(_, _), do: :erlang.error(:badarg)
  def from_list(values), do: from_list(values, :undefined)
  def from_list(values, default) when is_list(values), do: values |> Enum.with_index() |> Enum.map(fn {value, index} -> {index, value} end) |> from_orddict(default)
  def from_list(_, _), do: :erlang.error(:badarg)
  def to_orddict(array), do: Enum.map(indices(size(array)), fn index -> {index, get(index, array)} end)
  def to_list(array), do: Enum.map(to_orddict(array), fn {_, value} -> value end)
  def sparse_to_orddict({:tonic_array, size, _, _, entries}), do: entries |> Enum.filter(fn {index, _} -> index < size end) |> Enum.sort()
  def sparse_to_orddict(_), do: :erlang.error(:badarg)
  def sparse_to_list(array), do: Enum.map(sparse_to_orddict(array), fn {_, value} -> value end)
  defp indices(0), do: []
  defp indices(size), do: 0..(size - 1)
end
