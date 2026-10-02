defmodule Greeter do
  def greet(name), do: "Hello, #{name}!"
end

IO.puts(Greeter.greet("world"))
