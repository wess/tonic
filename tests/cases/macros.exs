defmodule MyMacros do
  defmacro unless_nil(value, do: block) do
    quote do
      case unquote(value) do
        nil -> nil
        _ -> unquote(block)
      end
    end
  end

  defmacro swap({a, b}) do
    quote do: {unquote(b), unquote(a)}
  end

  defmacro my_assert({op, _, [l, r]} = expr) when op in [:==, :>, :<] do
    code = Macro.to_string(expr)

    quote do
      lv = unquote(l)
      rv = unquote(r)

      if unquote(op)(lv, rv) do
        :ok
      else
        {:failed, unquote(code), lv, rv}
      end
    end
  end

  defmacro defgetters(fields) do
    for {name, value} <- fields do
      quote do
        def unquote(name)(), do: unquote(value)
      end
    end
  end

  defmacro hygienic do
    quote do
      x = :macro_x
      x
    end
  end

  defmacro unhygienic do
    quote do
      var!(y) = :set_by_macro
    end
  end

  defmacro count_args(args) do
    length(args)
  end

  defmacro my_pipe(expr) do
    {result, _} =
      Macro.prewalk(expr, 0, fn
        n, acc when is_integer(n) -> {n * 10, acc + 1}
        other, acc -> {other, acc}
      end)

    result
  end
end

defmodule Config do
  require MyMacros
  MyMacros.defgetters(port: 4000, host: "localhost", debug: false)
end

defmodule Demo do
  require MyMacros
  import MyMacros

  def run do
    IO.inspect(unless_nil(5, do: :has_value))
    IO.inspect(unless_nil(nil, do: :has_value))
    IO.inspect(swap({1, 2}))
    IO.inspect(my_assert(1 + 1 == 2))
    IO.inspect(my_assert(3 > 5))
    x = :outer_x
    IO.inspect(hygienic())
    IO.inspect(x)
    unhygienic()
    IO.inspect(y)
    IO.inspect(count_args([1, 2, 3]))
    IO.inspect(my_pipe(1 + 2 * 3))
  end
end

Demo.run()
IO.inspect({Config.port(), Config.host(), Config.debug()})
IO.puts(Macro.to_string(quote(do: Enum.map(list, fn x -> x * 2 end))))

