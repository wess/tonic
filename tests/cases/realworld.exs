defmodule Calc.Lexer do
  def tokenize(str), do: tokenize(String.to_charlist(str), [])
  defp tokenize([], acc), do: Enum.reverse(acc)
  defp tokenize([c | rest], acc) when c in ~c" \t", do: tokenize(rest, acc)
  defp tokenize([c | _] = s, acc) when c in ?0..?9 do
    {digits, rest} = Enum.split_while(s, &(&1 in ?0..?9))
    tokenize(rest, [{:num, List.to_integer(digits)} | acc])
  end
  defp tokenize([c | rest], acc) when c in ~c"+-*/()", do: tokenize(rest, [{:op, <<c>>} | acc])
  defp tokenize([c | _], _acc), do: throw({:bad_char, <<c::utf8>>})
end

defmodule Calc.Parser do
  # expr := term (('+'|'-') term)*
  def parse(tokens) do
    case expr(tokens) do
      {ast, []} -> {:ok, ast}
      {_, rest} -> {:error, {:trailing, rest}}
    end
  catch
    {:bad_char, c} -> {:error, {:bad_char, c}}
  end

  defp expr(tokens) do
    {left, rest} = term(tokens)
    expr_rest(left, rest)
  end

  defp expr_rest(left, [{:op, op} | rest]) when op in ["+", "-"] do
    {right, rest} = term(rest)
    expr_rest({String.to_atom(op), left, right}, rest)
  end

  defp expr_rest(left, rest), do: {left, rest}

  defp term(tokens) do
    {left, rest} = factor(tokens)
    term_rest(left, rest)
  end

  defp term_rest(left, [{:op, op} | rest]) when op in ["*", "/"] do
    {right, rest} = factor(rest)
    term_rest({String.to_atom(op), left, right}, rest)
  end

  defp term_rest(left, rest), do: {left, rest}

  defp factor([{:num, n} | rest]), do: {n, rest}

  defp factor([{:op, "("} | rest]) do
    case expr(rest) do
      {e, [{:op, ")"} | rest]} -> {e, rest}
      {_, rest} -> throw({:bad_char, inspect(rest)})
    end
  end

  defp factor([{:op, "-"} | rest]) do
    {e, rest} = factor(rest)
    {{:neg, e}, rest}
  end
end

defmodule Calc do
  def eval(n) when is_integer(n), do: n
  def eval({:neg, e}), do: -eval(e)
  def eval({:+, a, b}), do: eval(a) + eval(b)
  def eval({:-, a, b}), do: eval(a) - eval(b)
  def eval({:*, a, b}), do: eval(a) * eval(b)
  def eval({:/, a, b}) do
    case eval(b) do
      0 -> raise ArithmeticError, message: "division by zero in expression"
      d -> div(eval(a), d)
    end
  end

  def run(str) do
    with tokens when is_list(tokens) <- catch_lex(str),
         {:ok, ast} <- Calc.Parser.parse(tokens) do
      {:ok, eval(ast)}
    else
      {:error, reason} -> {:error, reason}
      {:bad_char, c} -> {:error, {:bad_char, c}}
    end
  rescue
    e in ArithmeticError -> {:error, Exception.message(e)}
  end

  defp catch_lex(str) do
    Calc.Lexer.tokenize(str)
  catch
    t -> t
  end
end

for s <- ["1 + 2 * 3", "(1 + 2) * 3", "-4 * (2 - 5)", "10 / (5 - 5)", "2 $ 3", "100 / 7 / 2"] do
  IO.puts("#{String.pad_trailing(s, 14)} => #{inspect(Calc.run(s))}")
end

defmodule Stats do
  defstruct count: 0, sum: 0, min: nil, max: nil

  def add(%__MODULE__{count: c, sum: s, min: mn, max: mx}, x) do
    %__MODULE__{count: c + 1, sum: s + x, min: min(mn || x, x), max: max(mx || x, x)}
  end

  def mean(%__MODULE__{count: 0}), do: nil
  def mean(%__MODULE__{count: c, sum: s}), do: s / c
end

defimpl String.Chars, for: Stats do
  def to_string(s), do: "Stats(n=#{s.count}, mean=#{Float.round(Stats.mean(s), 2)}, range=#{s.min}..#{s.max})"
end

stats = Enum.reduce([3, 1, 4, 1, 5, 9, 2, 6], struct(Stats), &Stats.add(&2, &1))
IO.puts(stats)
IO.inspect(stats)

defmodule WordCount do
  use GenServer

  def start_link(_), do: GenServer.start_link(__MODULE__, %{}, name: __MODULE__)
  def add(text), do: GenServer.cast(__MODULE__, {:add, text})
  def top(n), do: GenServer.call(__MODULE__, {:top, n})

  @impl true
  def init(state), do: {:ok, state}

  @impl true
  def handle_cast({:add, text}, state) do
    words = text |> String.downcase() |> String.split(~r/[^a-z']+/, trim: true)
    {:noreply, Enum.reduce(words, state, fn w, acc -> Map.update(acc, w, 1, &(&1 + 1)) end)}
  end

  @impl true
  def handle_call({:top, n}, _from, state) do
    top = state |> Enum.sort_by(fn {w, c} -> {-c, w} end) |> Enum.take(n)
    {:reply, top, state}
  end
end

{:ok, _} = WordCount.start_link([])
WordCount.add("The quick brown fox jumps over the lazy dog.")
WordCount.add("The dog barks; the fox runs. Quick, quick!")
IO.inspect(WordCount.top(4))

results =
  1..8
  |> Task.async_stream(fn i -> {i, Enum.sum(1..(i * 1000))} end, max_concurrency: 3)
  |> Enum.map(fn {:ok, v} -> v end)

IO.inspect(results)

table =
  [{"alice", 31, :admin}, {"bob", 25, :user}, {"carol", 47, :user}]
  |> Enum.map(fn {n, a, r} -> "| #{String.pad_trailing(n, 6)}| #{String.pad_leading(Integer.to_string(a), 3)} | #{r} |" end)

Enum.each(table, &IO.puts/1)
IO.puts(:io_lib.format("~-8s|~5.2f|~p~n", ["pi", 3.14159, {:ok, [1, 2]}]))
IO.inspect(for <<c <- "hello">>, c in ?a..?m, into: "", do: <<c - 32>>)
IO.inspect(Enum.group_by(~w(apple avocado banana blueberry cherry), &String.first/1))
IO.inspect(Map.new([b: 2, a: 1, c: 3]) |> Map.to_list() |> Enum.sort())
