# Kernel's macros as Elixir 1.18.3 defines them, for expansion at run time
# (Macro.expand/2 and friends). Compiled code never calls these: the compiler
# expands Kernel macros itself.
defmodule Tonic.KernelMacros do
  @moduledoc false

  @macros [
    {:if, 2}, {:unless, 2}, {:&&, 2}, {:||, 2}, {:!, 1}, {:|>, 2},
    {:is_nil, 1}, {:then, 2}, {:tap, 2}, {:to_string, 1}, {:to_charlist, 1}, {:match?, 2}
  ]

  def macro?(name, arity), do: {name, arity} in @macros

  # the variable Kernel's own quotes bind
  defp x, do: Macro.var(:x, Kernel)

  defp annotate_case(meta, {:case, case_meta, args}), do: {:case, case_meta ++ meta, args}

  defp falsy(var), do: quote(do: :"Elixir.Kernel".in(unquote(var), [false, nil]))

  def expand(:if, [condition, clauses]), do: build_if(condition, clauses)
  def expand(:unless, [condition, clauses]), do: build_unless(condition, clauses)

  def expand(:&&, [left, right]) do
    annotate_case(
      [type_check: :expr],
      quote do
        case unquote(left) do
          unquote(x()) when unquote(falsy(x())) -> unquote(x())
          _ -> unquote(right)
        end
      end
    )
  end

  def expand(:||, [left, right]) do
    annotate_case(
      [type_check: :expr],
      quote do
        case unquote(left) do
          unquote(x()) when unquote(falsy(x())) -> unquote(right)
          unquote(x()) -> unquote(x())
        end
      end
    )
  end

  def expand(:!, [{:!, _, [value]}]) do
    annotate_case(
      [optimize_boolean: true, type_check: :expr],
      quote do
        case unquote(value) do
          unquote(x()) when unquote(falsy(x())) -> false
          _ -> true
        end
      end
    )
  end

  def expand(:!, [value]) do
    annotate_case(
      [optimize_boolean: true, type_check: :expr],
      quote do
        case unquote(value) do
          unquote(x()) when unquote(falsy(x())) -> true
          _ -> false
        end
      end
    )
  end

  def expand(:|>, [left, right]) do
    :lists.foldl(fn {call, pos}, acc -> Macro.pipe(acc, call, pos) end, left, Macro.unpipe(right))
  end

  def expand(:is_nil, [term]), do: quote(do: unquote(term) == nil)
  def expand(:then, [value, fun]), do: quote(do: unquote(fun).(unquote(value)))

  def expand(:tap, [value, fun]) do
    quote bind_quoted: [fun: fun, value: value] do
      _ = fun.(value)
      value
    end
  end

  def expand(:to_string, [term]), do: quote(do: :"Elixir.String.Chars".to_string(unquote(term)))
  def expand(:to_charlist, [term]), do: quote(do: List.Chars.to_charlist(unquote(term)))

  def expand(:match?, [pattern, expr]) do
    success = quote(do: (unquote(pattern) -> true))
    failure = quote(generated: true, do: (_ -> false))
    {:case, [], [expr, [do: success ++ failure]]}
  end

  defp build_if(condition, do: do_clause), do: build_if(condition, do: do_clause, else: nil)

  defp build_if(condition, do: do_clause, else: else_clause) do
    annotate_case(
      [optimize_boolean: true, type_check: :expr],
      quote do
        case unquote(condition) do
          unquote(x()) when unquote(falsy(x())) -> unquote(else_clause)
          _ -> unquote(do_clause)
        end
      end
    )
  end

  defp build_if(_condition, _arguments) do
    raise ArgumentError, "invalid or duplicate keys for if, only \"do\" and an optional \"else\" are permitted"
  end

  defp build_unless(condition, do: do_clause), do: build_unless(condition, do: do_clause, else: nil)

  defp build_unless(condition, do: do_clause, else: else_clause) do
    quote(do: if(unquote(condition), do: unquote(else_clause), else: unquote(do_clause)))
  end

  defp build_unless(_condition, _arguments) do
    raise ArgumentError,
          "invalid or duplicate keys for unless, only \"do\" and an optional \"else\" are permitted"
  end
end
