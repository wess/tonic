defmodule Range do
# Modified for Tonic; Elixir 1.18.3 source/port. Apache-2.0; see licenses/sources.json and notice.




























































































































































  @enforce_keys [:first, :last, :step]
  defstruct first: nil, last: nil, step: nil

























  def new(first, last) when is_integer(first) and is_integer(last) do
    step =
      if first <= last do
        1
      else
        # TODO: Remove me on v2.0
        IO.warn_once(
          {__MODULE__, :new},
          fn ->
            "Range.new/2 and first..last default to a step of -1 when last < first. Use Range.new(first, last, -1) or first..last//-1, or pass 1 if that was your intention"
          end,
          3
        )

        -1
      end

    %Range{first: first, last: last, step: step}
  end

  def new(first, last) do
    raise ArgumentError,
          "ranges (first..last) expect both sides to be integers, " <>
            "got: #{inspect(first)}..#{inspect(last)}"
  end












  def new(first, last, step)
      when is_integer(first) and is_integer(last) and is_integer(step) and step != 0 do
    %Range{first: first, last: last, step: step}
  end

  def new(first, last, step) do
    raise ArgumentError,
          "ranges (first..last//step) expect both sides to be integers and the step to be a " <>
            "non-zero integer, got: #{inspect(first)}..#{inspect(last)}//#{inspect(step)}"
  end



























  def size(range)
  def size(first..last//step) when step > 0 and first > last, do: 0
  def size(first..last//step) when step < 0 and first < last, do: 0
  def size(first..last//step), do: abs(div(last - first, step)) + 1

  # TODO: Remove me on v2.0
  def size(%{__struct__: Range, first: first, last: last} = range) do
    step = if first <= last, do: 1, else: -1
    size(Map.put(range, :step, step))
  end



















  def shift(first..last//step, steps_to_shift)
      when is_integer(steps_to_shift) do
    new(first + steps_to_shift * step, last + steps_to_shift * step, step)
  end
























































































  def split(first..last//step = range, split) when is_integer(split) do
    if split >= 0 do
      split(first, last, step, split)
    else
      split(first, last, step, size(range) + split)
    end
  end

  defp split(first, last, step, split) when first < last or (first == last and step > 0) do
    if step > 0 do
      mid = max(min(first + step * (split - 1), last), first - step)
      {first..mid//step, (mid + step)..last//step}
    else
      {first..(first - step)//step, (last + step)..last//step}
    end
  end

  defp split(last, first, step, split) do
    if step < 0 do
      mid = min(max(last + step * (split - 1), first), last - step)
      {last..mid//step, (mid + step)..first//step}
    else
      {last..(last - step)//step, (first + step)..first//step}
    end
  end














  def to_list(first..last//step)
      when step > 0 and first <= last
      when step < 0 and first >= last do
    :lists.seq(first, last, step)
  end

  def to_list(_first.._last//_step) do
    []
  end

  # TODO: Remove me on v2.0
  def to_list(%{__struct__: Range, first: first, last: last}) do
    step = if first <= last, do: 1, else: -1
    :lists.seq(first, last, step)
  end











































  def disjoint?(first1..last1//step1 = range1, first2..last2//step2 = range2) do
    if size(range1) == 0 or size(range2) == 0 do
      true
    else
      {first1, last1, step1} = normalize(first1, last1, step1)
      {first2, last2, step2} = normalize(first2, last2, step2)

      cond do
        last2 < first1 or last1 < first2 ->
          true

        abs(step1) == 1 and abs(step2) == 1 ->
          false

        true ->
          # We need to find the first intersection of two arithmetic
          # progressions and see if they belong within the ranges
          # https://math.stackexchange.com/questions/1656120/formula-to-find-the-first-intersection-of-two-arithmetic-progressions
          {gcd, u, v} = Integer.extended_gcd(-step1, step2)

          if rem(first2 - first1, gcd) == 0 do
            c = first1 - first2 + step2 - step1
            t1 = -c / step2 * u
            t2 = -c / step1 * v
            t = max(floor(t1) + 1, floor(t2) + 1)
            x = div(c * u + t * step2, gcd) - 1
            y = div(c * v + t * step1, gcd) - 1

            x < 0 or first1 + x * step1 > last1 or
              y < 0 or first2 + y * step2 > last2
          else
            true
          end
      end
    end
  end


  defp normalize(first, last, step) when first > last,
    do: {first - abs(div(first - last, step) * step), first, -step}

  defp normalize(first, last, step), do: {first, last, step}



  def range?(%{__struct__: Range, first: first, last: last})
      when is_integer(first) and is_integer(last),
      do: true

  def range?(_), do: false
end

defimpl Enumerable, for: Range do
  def reduce(first..last//step, acc, fun) do
    reduce(first, last, acc, fun, step)
  end

  # TODO: Remove me on v2.0
  def reduce(%{__struct__: Range, first: first, last: last} = range, acc, fun) do
    step = if first <= last, do: 1, else: -1
    reduce(Map.put(range, :step, step), acc, fun)
  end

  defp reduce(_first, _last, {:halt, acc}, _fun, _step) do
    {:halted, acc}
  end

  defp reduce(first, last, {:suspend, acc}, fun, step) do
    {:suspended, acc, &reduce(first, last, &1, fun, step)}
  end

  defp reduce(first, last, {:cont, acc}, fun, step)
       when step > 0 and first <= last
       when step < 0 and first >= last do
    reduce(first + step, last, fun.(first, acc), fun, step)
  end

  defp reduce(_, _, {:cont, acc}, _fun, _up) do
    {:done, acc}
  end

  def member?(first..last//step, value) when is_integer(value) do
    if step > 0 do
      {:ok, first <= value and value <= last and rem(value - first, step) == 0}
    else
      {:ok, last <= value and value <= first and rem(value - first, step) == 0}
    end
  end

  # TODO: Remove me on v2.0
  def member?(%{__struct__: Range, first: first, last: last} = range, value)
      when is_integer(value) do
    step = if first <= last, do: 1, else: -1
    member?(Map.put(range, :step, step), value)
  end

  def member?(_, _value) do
    {:ok, false}
  end

  def count(range) do
    {:ok, Range.size(range)}
  end

  def slice(first.._//step = range) do
    {:ok, Range.size(range), &slice(first + &1 * step, step + &3 - 1, &2)}
  end

  # TODO: Remove me on v2.0
  def slice(%{__struct__: Range, first: first, last: last} = range) do
    step = if first <= last, do: 1, else: -1
    slice(Map.put(range, :step, step))
  end

  defp slice(_current, _step, 0), do: []
  defp slice(current, step, remaining), do: [current | slice(current + step, step, remaining - 1)]
end

defimpl Inspect, for: Range do
  import Inspect.Algebra
  import Kernel, except: [inspect: 2]

  def inspect(first..last//1, opts) when last >= first do
    concat([to_doc(first, opts), "..", to_doc(last, opts)])
  end

  def inspect(first..last//step, opts) do
    concat([to_doc(first, opts), "..", to_doc(last, opts), "//", to_doc(step, opts)])
  end

  # TODO: Remove me on v2.0
  def inspect(%{__struct__: Range, first: first, last: last} = range, opts) do
    step = if first <= last, do: 1, else: -1
    inspect(Map.put(range, :step, step), opts)
  end
end

# Imported from Elixir 1.18.3 lib/elixir/lib/range.ex (docs and specs stripped;
# line numbers match the original).
