defmodule User do
  defstruct name: "anon", age: 0, email: nil

  def new(name, age), do: %User{name: name, age: age}
  def greet(%User{name: name}), do: "Hi #{name}"
end

defprotocol Shape do
  def area(shape)
  def name(shape)
end

defmodule Circle do
  defstruct r: 1
end

defmodule Rect do
  defstruct w: 1, h: 1
end

defimpl Shape, for: Circle do
  def area(%Circle{r: r}), do: 3.14159 * r * r
  def name(_), do: "circle"
end

defimpl Shape, for: Rect do
  def area(%Rect{w: w, h: h}), do: w * h
  def name(_), do: "rect"
end

defimpl String.Chars, for: User do
  def to_string(u), do: "User(#{u.name})"
end

defmodule MyError do
  defexception message: "something went wrong", code: 0
end

defmodule NotFound do
  defexception [:id]

  @impl true
  def message(%{id: id}), do: "item #{id} not found"
end

defmodule Main do
  def run do
    u = User.new("Ann", 30)
    IO.inspect(u)
    IO.inspect(%User{})
    IO.puts(User.greet(u))
    IO.puts("#{u}")
    IO.inspect(u.age)
    u2 = %{u | age: 31}
    IO.inspect(u2)
    IO.inspect(is_struct(u))
    IO.inspect(is_struct(u, User))
    IO.inspect(Map.from_struct(u))
    %User{name: n} = u
    IO.puts(n)
    shapes = [%Circle{r: 2}, %Rect{w: 3, h: 4}]
    for s <- shapes, do: IO.puts("#{Shape.name(s)}: #{Shape.area(s)}")
    IO.inspect(Enum.map(shapes, &Shape.area/1))

    try do
      raise MyError
    rescue
      e in MyError -> IO.inspect(e)
    end

    try do
      raise MyError, message: "custom", code: 42
    rescue
      e in MyError -> IO.puts("#{e.message} #{e.code}")
    end

    try do
      raise NotFound, id: 7
    rescue
      e -> IO.puts(Exception.message(e))
    end

    try do
      Shape.area(:not_a_shape)
    rescue
      e in Protocol.UndefinedError -> IO.puts(Exception.message(e))
    end

    IO.inspect(struct(User, name: "Bob"))
    IO.inspect(Map.keys(%User{}) |> Enum.sort())
  end
end

Main.run()
