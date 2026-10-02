defmodule Packages do
  def main(args) do
    [left, right] = if args == [], do: ["1.25", "2.50"], else: args
    left |> Decimal.new() |> Decimal.add(Decimal.new(right)) |> Decimal.to_string() |> IO.puts()
  end
end
