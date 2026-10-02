defmodule MapSet do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.
























































  # The key name is :map because the MapSet implementation used to be based on top of maps before
  # Elixir 1.15 (and Erlang/OTP 24, which introduced :sets version 2). :sets v2's internal
  # representation is, anyways, exactly the same as MapSet's previous implementation. We cannot
  # change the :map key name here because we'd break backwards compatibility with code compiled
  # with Elixir 1.14 and earlier and executed on Elixir 1.15+.
  defstruct map: :sets.new(version: 2)











  def new(), do: %MapSet{}













  def new(enumerable)

  def new(%__MODULE__{} = map_set), do: map_set

  def new(enumerable) do
    set =
      enumerable
      |> Enum.to_list()
      |> :sets.from_list(version: 2)

    %MapSet{map: set}
  end











  def new(enumerable, transform) when is_function(transform, 1) do
    set =
      enumerable
      |> Enum.map(transform)
      |> :sets.from_list(version: 2)

    %MapSet{map: set}
  end
















  def delete(%MapSet{map: set} = map_set, value) do
    %{map_set | map: :sets.del_element(value, set)}
  end











  def difference(%MapSet{map: set1} = map_set1, %MapSet{map: set2} = _map_set2) do
    %{map_set1 | map: :sets.subtract(set1, set2)}
  end












  def symmetric_difference(%MapSet{map: set1} = map_set1, %MapSet{map: set2} = _map_set2) do
    {small, large} = if :sets.size(set1) <= :sets.size(set2), do: {set1, set2}, else: {set2, set1}

    disjointer_fun = fn elem, {small, acc} ->
      if :sets.is_element(elem, small) do
        {:sets.del_element(elem, small), acc}
      else
        {small, [elem | acc]}
      end
    end

    {new_small, list} = :sets.fold(disjointer_fun, {small, []}, large)
    %{map_set1 | map: :sets.union(new_small, :sets.from_list(list, version: 2))}
  end













  def disjoint?(%MapSet{map: set1}, %MapSet{map: set2}) do
    :sets.is_disjoint(set1, set2)
  end



















  def equal?(%MapSet{map: set1}, %MapSet{map: set2}) do
    set1 === set2
  end














  def intersection(%MapSet{map: set1} = map_set1, %MapSet{map: set2} = _map_set2) do
    %{map_set1 | map: :sets.intersection(set1, set2)}
  end













  def member?(%MapSet{map: set}, value) do
    :sets.is_element(value, set)
  end













  def put(%MapSet{map: set} = map_set, value) do
    %{map_set | map: :sets.add_element(value, set)}
  end











  def size(%MapSet{map: set}) do
    :sets.size(set)
  end















  def subset?(%MapSet{map: set1}, %MapSet{map: set2}) do
    :sets.is_subset(set1, set2)
  end











  def to_list(%MapSet{map: set}) do
    :sets.to_list(set)
  end











  def union(%MapSet{map: set1} = map_set1, %MapSet{map: set2} = _map_set2) do
    %{map_set1 | map: :sets.union(set1, set2)}
  end


























  def filter(%MapSet{map: set} = map_set, fun) when is_function(fun) do
    pred = fn element -> !!fun.(element) end
    %{map_set | map: :sets.filter(pred, set)}
  end


















  def reject(%MapSet{map: set} = map_set, fun) when is_function(fun) do
    pred = fn element -> !fun.(element) end
    %{map_set | map: :sets.filter(pred, set)}
  end


























  def split_with(%MapSet{map: map}, fun) when is_function(fun, 1) do
    {while_true, while_false} = Map.split_with(map, fn {key, _} -> fun.(key) end)
    {%MapSet{map: while_true}, %MapSet{map: while_false}}
  end

  defimpl Enumerable do
    def count(map_set) do
      {:ok, MapSet.size(map_set)}
    end

    def member?(map_set, val) do
      {:ok, MapSet.member?(map_set, val)}
    end

    def slice(map_set) do
      size = MapSet.size(map_set)
      {:ok, size, &MapSet.to_list/1}
    end

    def reduce(map_set, acc, fun) do
      Enumerable.List.reduce(MapSet.to_list(map_set), acc, fun)
    end
  end

  defimpl Collectable do
    def into(%@for{map: set} = map_set) do
      fun = fn
        list, {:cont, x} -> [x | list]
        list, :done -> %{map_set | map: :sets.union(set, :sets.from_list(list, version: 2))}
        _, :halt -> :ok
      end

      {[], fun}
    end
  end

  defimpl Inspect do
    import Inspect.Algebra

    def inspect(map_set, opts) do
      opts = %Inspect.Opts{opts | charlists: :as_lists}
      concat(["MapSet.new(", Inspect.List.inspect(MapSet.to_list(map_set), opts), ")"])
    end
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/map_set.ex (docs and specs stripped;
# line numbers match the original).
