# Runtime evaluation of quoted Elixir (Code.eval_string/eval_quoted).
#
# tonic compiles ahead of time, so code built at runtime is interpreted:
# the quoted form (from the real Elixir parser) is evaluated directly, with
# Kernel's macros and special forms implemented here and every other call
# dispatched to the compiled modules.
defmodule Tonic.Eval do
  @moduledoc false

  defmodule State do
    @moduledoc false
    defstruct vars: %{},
              aliases: [],
              imports: [],
              requires: [],
              module: nil,
              function: nil,
              file: "nofile",
              line: 1,
              stacktrace: nil,
              in_guard: false,
              bound: nil,
              in_module: false,
              super_target: nil,
              arguments: [],
              macro_function: false,
              quote_unquote: true
  end

  @kernel_ops %{
    {:+, 2} => {:erlang, :+},
    {:-, 2} => {:erlang, :-},
    {:*, 2} => {:erlang, :*},
    {:/, 2} => {:erlang, :/},
    {:+, 1} => {:erlang, :+},
    {:-, 1} => {:erlang, :-},
    {:==, 2} => {:erlang, :==},
    {:!=, 2} => {:erlang, :"/="},
    {:===, 2} => {:erlang, :"=:="},
    {:!==, 2} => {:erlang, :"=/="},
    {:<, 2} => {:erlang, :<},
    {:>, 2} => {:erlang, :>},
    {:<=, 2} => {:erlang, :"=<"},
    {:>=, 2} => {:erlang, :>=},
    {:++, 2} => {:erlang, :++},
    {:--, 2} => {:erlang, :--}
  }

  ## Entry points

  def eval_quoted(quoted, binding, opts) do
    st = new_state(binding, opts)
    {value, st} = expr(quoted, st)
    {value, dump_binding(st)}
  end

  def eval_quoted_with_env(quoted, binding, env) do
    st = new_state(binding, env)
    {value, st} = expr(quoted, st)
    env = %{env_struct(env) | aliases: st.aliases, versioned_vars: %{}}
    {value, dump_binding(st), env}
  end

  defp env_struct(%Macro.Env{} = env), do: env
  defp env_struct(_), do: %Macro.Env{}

  defp new_state(binding, opts) do
    vars =
      Map.new(binding, fn
        {{name, ctx}, v} when is_atom(name) and is_atom(ctx) -> {{name, ctx}, v}
        {name, v} when is_atom(name) -> {{name, nil}, v}
      end)

    case opts do
      %Macro.Env{} = env ->
        %State{
          vars: vars,
          aliases: env.aliases,
          module: env.module,
          function: env.function,
          file: env.file,
          line: env.line
        }

      opts when is_list(opts) ->
        %State{vars: vars, file: Keyword.get(opts, :file, "nofile"), line: Keyword.get(opts, :line, 1)}
    end
  end

  defp dump_binding(st) do
    st.vars
    |> Enum.map(fn
      {{name, nil}, v} -> {name, v}
      {key, v} -> {key, v}
    end)
    |> Enum.sort_by(fn {k, _} -> k end)
  end

  ## Errors

  # As elixir_expand: the error is logged as a diagnostic, then the
  # evaluation fails with a generic CompileError.
  defp compile_error(st, meta, msg) do
    line = Keyword.get(meta, :line, st.line)
    position = if col = Keyword.get(meta, :column), do: {line, col}, else: line
    :elixir_errors.emit_diagnostic(:error, position, st.file, msg, [], read_snippet: true)
    raise CompileError, description: "cannot compile code (errors have been logged)"
  end

  ## Expressions

  def expr({:__block__, _, []}, st), do: {nil, st}

  def expr({:__block__, _, exprs}, st) do
    Enum.reduce(exprs, {nil, st}, fn e, {_, st} -> expr(e, st) end)
  end

  def expr(list, st) when is_list(list), do: list_expr(list, st)

  def expr({a, b}, st) do
    {a, st} = expr(a, st)
    {b, st} = expr(b, st)
    {{a, b}, st}
  end

  def expr({:{}, _, args}, st) do
    {args, st} = exprs(args, st)
    {List.to_tuple(args), st}
  end

  def expr({:%{}, _, [{:|, _, [map, kvs]}]}, st) do
    {map, st} = expr(map, st)
    {kvs, st} = pairs(kvs, st)

    case map do
      %{} ->
        value =
          Enum.reduce(kvs, map, fn {k, v}, acc ->
            if is_map_key(acc, k), do: Map.put(acc, k, v), else: :erlang.error({:badkey, k, map})
          end)

        {value, st}

      other ->
        :erlang.error({:badmap, other})
    end
  end

  def expr({:%{}, _, kvs}, st) do
    {kvs, st} = pairs(kvs, st)
    {Map.new(kvs), st}
  end

  def expr({:%, meta, [mod_ast, {:%{}, _, [{:|, _, [base, kvs]}]}]}, st) do
    {mod, st} = struct_module(mod_ast, st, meta)
    {base, st} = expr(base, st)
    {kvs, st} = pairs(kvs, st)

    case base do
      %{__struct__: ^mod} ->
        {Enum.reduce(kvs, base, fn {k, v}, acc -> %{acc | k => v} end), st}

      _ ->
        raise BadStructError, struct: mod, term: base
    end
  end

  def expr({:%, meta, [mod_ast, {:%{}, _, kvs}]}, st) do
    {mod, st} = struct_module(mod_ast, st, meta)
    {kvs, st} = pairs(kvs, st)

    case Tonic.Eval.Modules.struct_fields(mod) do
      nil ->
        {struct!(mod, kvs), st}

      fields ->
        check_enforced(mod, Enum.map(kvs, &elem(&1, 0)))
        base = Map.new([{:__struct__, mod} | fields])

        value =
          Enum.reduce(kvs, base, fn {k, v}, acc ->
            if is_map_key(acc, k), do: Map.put(acc, k, v), else: raise(KeyError, key: k, term: mod)
          end)

        {value, st}
    end
  end

  def expr({:<<>>, _, segs}, st), do: binary_expr(segs, st)

  def expr({:__aliases__, _, _} = a, st), do: {expand_alias(a, st), st}

  def expr({:__MODULE__, _, ctx}, st) when is_atom(ctx), do: {st.module, st}
  def expr({:__DIR__, _, ctx}, st) when is_atom(ctx), do: {Path.dirname(Path.expand(st.file)), st}
  def expr({:__ENV__, _, ctx}, st) when is_atom(ctx), do: {env_of(st), st}
  def expr({:__CALLER__, _, ctx}, st) when is_atom(ctx), do: {Map.get(st.vars, {:tonic_caller, nil}), st}

  def expr({:__STACKTRACE__, meta, ctx}, st) when is_atom(ctx) do
    case st.stacktrace do
      nil -> compile_error(st, meta, "__STACKTRACE__ is available only inside catch and rescue clauses of try expressions")
      s -> {s, st}
    end
  end

  def expr({name, meta, ctx} = var, st) when is_atom(name) and is_atom(ctx) do
    case Map.fetch(st.vars, var_key(var)) do
      {:ok, v} ->
        {v, st}

      :error ->
        if ctx == nil and Keyword.get(meta, :if_undefined) == :apply do
          call_local(name, [], meta, st)
        else
          compile_error(st, meta, "undefined variable #{inspect(Atom.to_string(name))}")
        end
    end
  end

  def expr({:=, meta, [pat, e]}, st) do
    {v, st} = expr(e, st)
    if st.in_module and st.function == nil do
      Tonic.Eval.Modules.record_attribute_value(st.module, st.file, Keyword.get(meta, :line, st.line), :tonic_binding, v)
    end

    case match(pat, v, st) do
      {:ok, st} -> {v, st}
      :error -> raise MatchError, term: v
    end
    |> tap(fn _ -> meta end)
  end

  def expr({:^, meta, _}, st), do: compile_error(st, meta, "misplaced operator ^/1")

  def expr({:fn, _, clauses}, st), do: {make_fun(clauses, st), st}

  def expr({:case, _, [e, [do: clauses]]}, st) do
    {v, st} = expr(e, st)
    {run_clauses(clauses, [v], st, fn -> :erlang.error({:case_clause, v}) end), st}
  end

  def expr({:cond, _, [[do: clauses]]}, st) do
    {cond_clauses(clauses, st), st}
  end

  def expr({:if, _, [c, kw]}, st) when is_list(kw) do
    check_if_keys(:if, kw)
    {v, st} = expr(c, st)

    branch = if v not in [false, nil], do: Keyword.get(kw, :do), else: Keyword.get(kw, :else)
    {value, _} = expr(branch, st)
    {value, st}
  end

  def expr({:unless, meta, [c, kw]}, st) when is_list(kw) do
    check_if_keys(:unless, kw)
    expr({:if, meta, [c, [do: Keyword.get(kw, :else), else: Keyword.get(kw, :do)]]}, st)
  end

  def expr({:with, _, args}, st), do: {with_expr(args, st), st}
  def expr({:for, _, args}, st), do: {for_expr(args, st), st}
  def expr({:try, _, [kw]}, st), do: {try_expr(kw, st), st}
  def expr({:receive, _, [kw]}, st), do: {receive_expr(kw, st), st}

  def expr({:&, meta, [arg]}, st), do: {capture(arg, meta, st), st}

  def expr({:quote, _, arguments}, st) do
    options = Enum.reduce(arguments, [], fn option, acc -> acc ++ option end)
    body = Keyword.fetch!(options, :do)
    bindings = Keyword.get(options, :bind_quoted)
    unquote = Keyword.get(options, :unquote, bindings == nil)
    quoted = do_quote(body, %{st | quote_unquote: unquote})

    if bindings == nil do
      {quoted, st}
    else
      {values, _} = expr(bindings, st)
      assignments = Enum.map(values, fn {name, value} ->
        {:=, [], [{name, [], quote_context(st)}, value]}
      end)
      {{:__block__, [], assignments ++ [quoted]}, st}
    end
  end

  def expr({:tonic_macro_forward, _, [name, arguments]}, st) do
    {values, _} = exprs(arguments, st)
    {Tonic.Eval.Modules.call_macro(st.module, name, tl(values), hd(values), true), st}
  end


  def expr({:|>, _, [left, right]}, st) do
    expr(pipe_into(left, right), st)
  end

  defp pipe_into(left, {call, meta, args}) when is_list(args), do: {call, meta, [left | args]}
  defp pipe_into(left, {name, meta, ctx}) when is_atom(name) and is_atom(ctx), do: {name, meta, [left]}
  defp pipe_into(_left, right), do: raise(ArgumentError, "cannot pipe into #{Macro.to_string(right)}")

  def expr({:&&, _, [a, b]}, st) do
    {va, st} = expr(a, st)
    if va in [false, nil], do: {va, st}, else: expr(b, st)
  end

  def expr({:||, _, [a, b]}, st) do
    {va, st} = expr(a, st)
    if va in [false, nil], do: expr(b, st), else: {va, st}
  end

  def expr({:and, _, [a, b]}, st) do
    {va, st} = expr(a, st)

    case va do
      false -> {false, st}
      true -> expr(b, st)
      other -> :erlang.error({:badbool, :and, other})
    end
  end

  def expr({:or, _, [a, b]}, st) do
    {va, st} = expr(a, st)

    case va do
      true -> {true, st}
      false -> expr(b, st)
      other -> :erlang.error({:badbool, :or, other})
    end
  end

  def expr({:!, _, [a]}, st) do
    {va, st} = expr(a, st)
    {va in [false, nil], st}
  end

  def expr({:in, _, [l, r]}, st) do
    {vl, st} = expr(l, st)
    {vr, st} = expr(r, st)
    {in?(vl, vr), st}
  end

  def expr({:not, _, [{:in, m, args}]}, st) do
    {v, st} = expr({:in, m, args}, st)
    {not v, st}
  end

  def expr({:.., _, [first, last]}, st) do
    {f, st} = expr(first, st)
    {l, st} = expr(last, st)
    {Range.new(f, l), st}
  end

  def expr({:..//, _, [first, last, step]}, st) do
    {f, st} = expr(first, st)
    {l, st} = expr(last, st)
    {s, st} = expr(step, st)
    {Range.new(f, l, s), st}
  end

  def expr({:.., _, []}, st), do: {0..-1//1, st}

  def expr({:<>, _, [a, b]}, st) do
    {va, st} = expr(a, st)
    {vb, st} = expr(b, st)
    {va <> vb, st}
  end


  def expr({:alias, meta, [{{:., _, [base, :{}]}, _, names} | rest]}, st) do
    base = expand_alias(base, st)
    Enum.reduce(names, {nil, st}, fn {:__aliases__, _, segments}, {_, state} ->
      expr({:alias, meta, [Module.concat([base | segments]) | rest]}, state)
    end)
  end

  def expr({:alias, meta, [a | rest]}, st) do
    mod = expand_alias(a, st)
    opts = List.first(rest) || []
    as = Keyword.get(opts, :as)

    short =
      case as do
        nil -> Module.concat([List.last(Module.split(mod))])
        {:__aliases__, _, [n]} -> Module.concat([n])
        other -> other
      end

    _ = meta
    {mod, %{st | aliases: List.keystore(st.aliases, short, 0, {short, mod})}}
  end

  def expr({:import, _, [a | rest]}, st) do
    mod = expand_alias(a, st)
    {mod, %{st | imports: [{mod, List.first(rest) || []} | st.imports]}}
  end

  def expr({:require, _, [a | rest]}, st) do
    mod = expand_alias(a, st)

    st =
      case List.first(rest) do
        [as: {:__aliases__, _, [n]}] -> %{st | aliases: List.keystore(st.aliases, Module.concat([n]), 0, {Module.concat([n]), mod})}
        _ -> st
      end

    {mod, st}
  end

  def expr({:defmodule, meta, [name_ast, [do: body]]}, st) do
    {name, st} = module_name(name_ast, st, meta)
    Tonic.Eval.Modules.create(name, st.file)

    mst = %{st | module: name, function: nil, vars: %{}, in_module: true, line: Keyword.get(meta, :line, st.line)}

    # The macro host loads modules leniently: a statement it cannot run
    # (ExUnit's test/2, ...) is skipped instead of aborting the module.
    {_, mst} =
      if Process.get(:tonic_eval_lenient) do
        stmts =
          case body do
            {:__block__, _, es} -> es
            e -> [e]
          end

        Enum.reduce(stmts, {nil, mst}, fn e, {v, mst} ->
          try do
            expr(e, mst)
          rescue
            _ -> {v, mst}
          catch
            _, _ -> {v, mst}
          end
        end)
      else
        expr(body, mst)
      end
    mst = Tonic.Eval.Modules.finish(name, mst)

    st =
      case {name_ast, st.module} do
        {{:__aliases__, _, [h]}, outer} when outer != nil ->
          short = Module.concat([h])
          %{st | aliases: List.keystore(st.aliases, short, 0, {short, name})}

        _ ->
          st
      end

    {{:module, name, nil, nil}, st}
  end

  def expr({kind, meta, [head | rest]}, %{in_module: true} = st) when kind in [:def, :defp, :defmacro, :defmacrop] do
    body =
      case rest do
        [] -> nil
        [kw] -> kw
      end

    define(kind, head, body, meta, st)
  end

  def expr({:@, _, [{name, _, [spec]}]}, %{in_module: true} = st) when name in [:callback, :macrocallback, :spec, :type, :typep, :opaque] do
    Tonic.Eval.Modules.put_attribute(st.module, name, spec)
    {:ok, st}
  end

  def expr({:@, meta, [{name, _, args}]}, %{in_module: true} = st) when is_atom(name) do
    case args do
      ctx when is_atom(ctx) or args == [] ->
        {Tonic.Eval.Modules.get_attribute(st.module, name), st}

      [value_ast] ->
        {v, st} = expr(value_ast, st)
        Tonic.Eval.Modules.put_attribute(st.module, name, v)
        Tonic.Eval.Modules.record_attribute_value(st.module, st.file, Keyword.get(meta, :line, st.line), name, v)
        {:ok, st}

      _ ->
        compile_error(st, meta, "expected 0 or 1 argument for @#{name}, got #{length(args)}")
    end
  end

  def expr({:@, meta, _}, st), do: compile_error(st, meta, "cannot use module attributes outside of a module")

  def expr({:defstruct, _, [fields]}, %{in_module: true} = st) do
    {fields, st} = expr(fields, st)
    mod = st.module

    if Tonic.Eval.Modules.struct_fields(mod) != nil do
      raise ArgumentError,
            "defstruct has already been called for #{inspect(mod)}, defstruct can only be called once per module"
    end

    unless is_list(fields) do
      raise ArgumentError, "struct fields definition must be list, got: #{inspect(fields)}"
    end

    fields =
      Enum.map(fields, fn
        {:__struct__, _} -> raise ArgumentError, "cannot set :__struct__ in struct definition"
        :__struct__ -> raise ArgumentError, "cannot set :__struct__ in struct definition"
        {k, v} when is_atom(k) -> {k, v}
        k when is_atom(k) -> {k, nil}
        other -> raise ArgumentError, "struct field names must be atoms, got: #{inspect(other)}"
      end)

    enforce = List.wrap(Tonic.Eval.Modules.get_attribute(mod, :enforce_keys))

    Enum.each(enforce, fn
      k when is_atom(k) -> :ok
      k -> raise ArgumentError, "keys given to @enforce_keys must be atoms, got: #{inspect(k)}"
    end)

    Tonic.Eval.Modules.put_struct(mod, fields, false, enforce)
    {:ok, st}
  end

  def expr({:defexception, _, [fields]}, %{in_module: true} = st) do
    {fields, st} = expr(fields, st)

    fields =
      Enum.map(fields, fn
        {k, v} -> {k, v}
        k when is_atom(k) -> {k, nil}
      end)

    Tonic.Eval.Modules.put_struct(st.module, fields, true)
    {:ok, st}
  end

  def expr({:use, meta, [{{:., _, [base, :{}]}, _, children} | rest]}, st) do
    base = expand_alias(base, st)

    Enum.reduce(children, {:ok, st}, fn child, {_, st} ->
      mod =
        case child do
          {:__aliases__, _, parts} -> Module.concat([base | parts])
          other -> raise ArgumentError, "invalid arguments for use, expected a compile time atom or alias, got: #{Macro.to_string(other)}"
        end

      expr({:use, meta, [mod | rest]}, st)
    end)
  end

  def expr({:use, _meta, [mod_ast | _]}, _st)
      when not is_atom(mod_ast) and not (is_tuple(mod_ast) and elem(mod_ast, 0) == :__aliases__) do
    raise ArgumentError,
          "invalid arguments for use, expected a compile time atom or alias, got: #{Macro.to_string(mod_ast)}"
  end

  def expr({:use, meta, [mod_ast | rest]}, st) do
    mod = expand_alias(mod_ast, st)
    opts = List.first(rest) || []

    cond do
      Tonic.Eval.Modules.macro?(mod, :__using__, 1) ->
        {opts_v, st} = expr(opts, st)
        ast = Tonic.Eval.Modules.call_macro(mod, :__using__, [opts_v], env_of(st))
        expr(ast, st)

      true ->
        _ = meta
        {:ok, st}
    end
  end

  def expr({:defdelegate, _, [{:when, _, _} = head, _]}, %{in_module: true}) do
    raise ArgumentError, "guards are not allowed in defdelegate/2, got: when #{Macro.to_string(List.last(elem(head, 2)))}"
  end

  def expr({:defdelegate, _, [head, opts]}, %{in_module: true} = st) do
    {opts, st} = expr(opts, st)
    {name, args} = Macro.decompose_call(head)

    Enum.each(args, fn
      {:\\, _, [{v, _, c}, _]} when is_atom(v) and is_atom(c) -> :ok
      {v, _, c} when is_atom(v) and is_atom(c) -> :ok
      {:\\, _, [other, _]} -> raise ArgumentError, "defdelegate/2 only accepts function parameters, got: #{Macro.to_string(other)}"
      other -> raise ArgumentError, "defdelegate/2 only accepts function parameters, got: #{Macro.to_string(other)}"
    end)

    to = Keyword.fetch!(opts, :to)

    if to == st.module and not Keyword.has_key?(opts, :as) do
      raise ArgumentError,
            "defdelegate function is calling itself, which will lead to an infinite loop. You should either change the value of the :to option or specify the :as option"
    end
    as = Keyword.get(opts, :as, name)
    vars = Enum.map(args, fn {:\\, _, [v, _]} -> v; v -> v end)
    body = [do: {{:., [], [to, as]}, [], vars}]
    define(:def, {name, [], args}, body, [], st)
  end

  def expr({:defoverridable, _, [functions]}, %{in_module: true} = st) do
    {functions, st} = expr(functions, st)
    Tonic.Eval.Modules.make_overridable(st.module, functions)
    {:ok, st}
  end

  def expr({:super, meta, args}, st) do
    if st.super_target == nil, do: compile_error(st, meta, "super is not allowed here")
    {vals, st} = if is_atom(args), do: {st.arguments, st}, else: exprs(args, st)
    vals = if st.macro_function, do: [hd(st.arguments) | vals], else: vals
    if length(vals) != length(st.arguments), do: compile_error(st, meta, "super must be called with #{length(st.arguments)} arguments")
    {Tonic.Eval.Modules.apply(st.module, st.super_target, vals, true), st}
  end

  def expr({:sigil_r, _, [{:<<>>, _, parts}, mods]}, st) do
    {src, st} = sigil_source(parts, st)
    {Regex.compile!(src, List.to_string(mods)), st}
  end

  def expr({:sigil_R, m, args}, st), do: expr({:sigil_r, m, args}, st)

  def expr({sigil, _, [{:<<>>, _, parts}, mods]}, st)
       when sigil in [:sigil_s, :sigil_S, :sigil_c, :sigil_C, :sigil_w, :sigil_W, :sigil_D, :sigil_T, :sigil_N, :sigil_U] do
    parts = if sigil in [:sigil_s, :sigil_c, :sigil_w], do: Macro.unescape_tokens(parts), else: parts
    {src, st} = sigil_source(parts, st)

    value =
      case sigil do
        s when s in [:sigil_s, :sigil_S] -> src
        s when s in [:sigil_c, :sigil_C] -> String.to_charlist(src)
        s when s in [:sigil_w, :sigil_W] -> split_words(src, mods)
        :sigil_D -> calendar_sigil(src, :parse_date, "Date")
        :sigil_T -> calendar_sigil(src, :parse_time, "Time")
        :sigil_N -> calendar_sigil(src, :parse_naive_datetime, "NaiveDateTime")
        :sigil_U -> calendar_sigil(src, :parse_utc_datetime, "UTC DateTime")
      end

    {value, st}
  end

  # As Kernel's ~D/~T/~N/~U (parse_with_calendar!).
  defp calendar_sigil(string, fun, type) do
    {calendar, string} =
      case :binary.split(string, " ", [:global]) do
        [_] ->
          {Calendar.ISO, string}

        parts ->
          case List.last(parts) do
            <<c, _::binary>> = last when c >= ?A and c <= ?Z ->
              {String.to_atom("Elixir." <> last), binary_part(string, 0, byte_size(string) - byte_size(last) - 1)}

            _ ->
              {Calendar.ISO, string}
          end
      end

    fail = fn reason ->
      raise ArgumentError, "cannot parse #{inspect(string)} as #{type} for #{inspect(calendar)}, reason: #{inspect(reason)}"
    end

    case apply(calendar, fun, [string]) do
      {:error, reason} ->
        fail.(reason)

      {:ok, {y, m, d}} ->
        %Date{calendar: calendar, year: y, month: m, day: d}

      {:ok, {h, mi, s, us}} ->
        %Time{calendar: calendar, hour: h, minute: mi, second: s, microsecond: us}

      {:ok, {y, m, d, h, mi, s, us}} ->
        %NaiveDateTime{calendar: calendar, year: y, month: m, day: d, hour: h, minute: mi, second: s, microsecond: us}

      {:ok, {y, m, d, h, mi, s, us}, offset} ->
        if offset != 0, do: fail.(:non_utc_offset)

        %DateTime{
          calendar: calendar, year: y, month: m, day: d, hour: h, minute: mi, second: s, microsecond: us,
          time_zone: "Etc/UTC", zone_abbr: "UTC", utc_offset: 0, std_offset: 0
        }
    end
  end

  # Remote and anonymous calls.
  def expr({{:., _, [fun]}, _meta, args}, st) do
    {f, st} = expr(fun, st)
    {args, st} = exprs(args, st)
    {apply(f, args), st}
  end

  def expr({{:., meta, [left, right]}, call_meta, args}, st) when is_atom(right) do
    {recv, st} = expr(left, st)

    cond do
      is_atom(recv) ->
        remote_call(recv, right, args, call_meta, st)

      args == [] and Keyword.get(call_meta, :no_parens, false) ->
        case recv do
          %{^right => v} -> {v, st}
          %{} -> :erlang.error({:badkey, right, recv})
          other -> :erlang.error({:badmap, other})
        end
        |> tap(fn _ -> meta end)

      true ->
        {args, st} = exprs(args, st)
        {apply(recv, right, args), st}
    end
  end

  def expr({name, meta, args}, st) when is_atom(name) and is_list(args) do
    special_or_local(name, meta, args, st)
  end

  def expr(other, st) when is_atom(other) or is_number(other) or is_binary(other) or is_pid(other) or is_reference(other) or is_function(other) or is_map(other) do
    {other, st}
  end

  def expr(other, st) when is_tuple(other), do: {other, st}

  defp exprs(list, st) do
    {vs, st} = Enum.reduce(list, {[], st}, fn e, {acc, st} -> {v, st} = expr(e, st); {[v | acc], st} end)
    {Enum.reverse(vs), st}
  end

  defp list_expr(list, st) do
    case :lists.reverse(list) do
      [{:|, _, [h, t]} | rest] ->
        {front, st} = exprs(:lists.reverse(rest) ++ [h], st)
        {tail, st} = expr(t, st)
        {front ++ tail, st}

      _ ->
        exprs(list, st)
    end
  end

  defp pairs(kvs, st) do
    {vs, st} =
      Enum.reduce(kvs, {[], st}, fn {k, v}, {acc, st} ->
        {k, st} = expr(k, st)
        {v, st} = expr(v, st)
        {[{k, v} | acc], st}
      end)

    {Enum.reverse(vs), st}
  end

  defp struct_module(mod_ast, st, meta) do
    {mod, st} = expr(mod_ast, st)

    if is_atom(mod) do
      {mod, st}
    else
      compile_error(st, meta, "expected struct name to be a compile time atom or alias, got: #{Macro.to_string(mod_ast)}")
    end
  end

  defp var_key({name, meta, ctx}) do
    case Keyword.get(meta, :counter) do
      nil -> {name, ctx}
      _ -> {name, ctx}
    end
  end

  def env_of(st) do
    %Macro.Env{
      module: st.module,
      file: st.file,
      line: st.line,
      function: st.function,
      aliases: st.aliases
    }
  end

  ## Aliases

  defp expand_alias({:__aliases__, meta, [h | t]}, st) when is_atom(h) do
    first = Module.concat([h])

    case List.keyfind(st.aliases, first, 0) do
      {_, mod} when t != [] -> Module.concat([mod | t])
      {_, mod} -> mod
      nil -> Module.concat([h | t])
    end
    |> tap(fn _ -> meta end)
  end

  defp expand_alias({:__aliases__, _, [h | t]}, st) do
    {base, _} = expr(h, st)
    Module.concat([base | t])
  end

  defp expand_alias({:__MODULE__, _, _}, st), do: st.module
  defp expand_alias(atom, _st) when is_atom(atom), do: atom

  ## Calls

  defp remote_call(Kernel, fun, args, meta, st) do
    if kernel_macro(fun, length(args)) do
      special_or_local(fun, meta, args, st)
    else
      {vals, st} = exprs(args, st)
      {apply(Kernel, fun, vals), st}
    end
  end

  defp remote_call(Module, fun, args, _meta, st) when fun in [:get_attribute, :put_attribute, :delete_attribute, :register_attribute] do
    {vals, st} = exprs(args, st)
    value =
      case {fun, vals} do
        {:get_attribute, [mod, name]} -> Tonic.Eval.Modules.get_attribute(mod, name)
        {:get_attribute, [mod, name, default]} -> Tonic.Eval.Modules.get_attribute(mod, name) || default
        {:put_attribute, [mod, name, value]} -> Tonic.Eval.Modules.put_attribute(mod, name, value)
        {:delete_attribute, [mod, name]} -> Tonic.Eval.Modules.delete_attribute(mod, name)
        {:register_attribute, [mod, name, opts]} -> Tonic.Eval.Modules.register_attribute(mod, name, opts)
      end
    {value, st}
  end

  defp remote_call(mod, fun, args, meta, st) do
    arity = length(args)

    cond do
      Tonic.Eval.Modules.macro?(mod, fun, arity) ->
        ast = Tonic.Eval.Modules.call_macro(mod, fun, args, env_of(%{st | line: Keyword.get(meta, :line, st.line)}))
        expr(ast, st)

      true ->
        {vals, st} = exprs(args, st)
        {Tonic.Eval.Modules.apply(mod, fun, vals), st}
    end
  end

  defp module_name({:__aliases__, _, [h | t]} = a, st, _meta) do
    first = Module.concat([h])

    case {List.keyfind(st.aliases, first, 0), st.module} do
      {nil, outer} when outer != nil -> {Module.concat([outer, h | t]), st}
      _ -> {expand_alias(a, st), st}
    end
  end

  defp module_name(other, st, meta) do
    {v, st} = expr(other, st)

    if is_atom(v) and v not in [nil, true, false] do
      {v, st}
    else
      raise ArgumentError, "invalid module name: #{inspect(v)}"
    end
    |> tap(fn _ -> meta end)
  end

  # def/defp/defmacro/defmacrop inside an evaluated module: unquote
  # fragments are resolved now; default arguments add lower arities.
  defp check_if_keys(kind, kw) do
    case Keyword.keys(kw) do
      [:do] -> :ok
      [:do, :else] -> :ok
      _ -> raise ArgumentError, "invalid or duplicate keys for #{kind}, only \"do\" and an optional \"else\" are permitted"
    end
  end

  defp check_enforced(mod, keys) do
    case Tonic.Eval.Modules.struct_enforce(mod) -- keys do
      [] -> :ok
      missing -> raise ArgumentError, "the following keys must also be given when building struct #{inspect(mod)}: #{inspect(missing)}"
    end
  end

  # %Struct{...} in a definition is expanded when it is compiled.
  defp check_struct_literals(ast, st) do
    Macro.prewalk(ast, fn
      {:%, _, [{:__aliases__, _, _} = a, {:%{}, _, kvs}]} = node when is_list(kvs) ->
        mod = expand_alias(a, st)

        if is_atom(mod) and Tonic.Eval.Modules.struct_fields(mod) != nil and Keyword.keyword?(kvs) do
          check_enforced(mod, Keyword.keys(kvs))
        end

        node

      {:%, _, [{:__MODULE__, _, ctx}, {:%{}, _, kvs}]} = node when is_atom(ctx) and is_list(kvs) ->
        if Tonic.Eval.Modules.struct_fields(st.module) != nil and Keyword.keyword?(kvs) do
          check_enforced(st.module, Keyword.keys(kvs))
        end

        node

      other ->
        other
    end)

    :ok
  end

  defp define(kind, head, body, meta, st) do
    {head, guard} =
      case head do
        {:when, _, [h, g]} -> {h, g}
        h -> {h, true}
      end

    head = unquote_fragments(head, st)
    guard = unquote_fragments(guard, st)
    body = unquote_fragments(body, st)

    {name, args} =
      case head do
        {name, _, ctx} when is_atom(name) and is_atom(ctx) -> {name, []}
        {name, _, args} when is_atom(name) and is_list(args) -> {name, args}
        _ -> compile_error(st, meta, "invalid syntax in def #{Macro.to_string(head)}")
      end

    expanded_head = if guard == true, do: head, else: {:when, meta, [head, guard]}
    Tonic.Eval.Modules.record_definition(st.module, st.file, Keyword.get(meta, :line, st.line), {kind, meta, [expanded_head, body]})

    args = if kind in [:defmacro, :defmacrop], do: [{:tonic_caller, [], nil} | args], else: args

    defaults = for {:\\, _, [_, d]} <- args, do: d
    plain = Enum.map(args, fn {:\\, _, [v, _]} -> v; v -> v end)
    arity = length(plain)

    body_ast =
      case body do
        nil -> nil
        kw when is_list(kw) -> if(Keyword.has_key?(kw, :do) and length(kw) == 1, do: kw[:do], else: {:try, [], [kw]})
      end

    env = %{aliases: st.aliases, imports: st.imports, file: st.file, super_target: Tonic.Eval.Modules.prepare_override(st.module, name, arity), macro_function: kind in [:defmacro, :defmacrop]}

    if body_ast != nil and kind in [:def, :defp], do: check_struct_literals(body_ast, st)

    if body_ast != nil do
      Tonic.Eval.Modules.add_clause(st.module, kind, name, arity, {plain, guard, body_ast, env})
    end

    # f(a, b \\ 1) also defines f/1 calling f/2
    n_def = length(defaults)

    if n_def > 0 do
      for missing <- 1..n_def do
        keep = n_def - missing
        {call_args, params, _} =
          Enum.reduce(args, {[], [], keep}, fn
            {:\\, _, [_, d]}, {ca, ps, k} when k == 0 -> {[d | ca], ps, 0}
            {:\\, _, [_, _]}, {ca, ps, k} ->
              v = {:"tonic_d#{length(ps)}", [], __MODULE__}
              {[v | ca], [v | ps], k - 1}
            _, {ca, ps, k} ->
              v = {:"tonic_d#{length(ps)}", [], __MODULE__}
              {[v | ca], [v | ps], k}
          end)

        call = if kind in [:defmacro, :defmacrop], do: {:tonic_macro_forward, [], [name, Enum.reverse(call_args)]}, else: {name, [], Enum.reverse(call_args)}
        Tonic.Eval.Modules.add_clause(st.module, kind, name, arity - missing, {Enum.reverse(params), true, call, env})
      end
    end

    callback_args = if kind in [:defmacro, :defmacrop], do: tl(args), else: args
    callback_guards = if guard == true, do: [], else: [guard]
    Tonic.Eval.Modules.on_definition(%{st | line: Keyword.get(meta, :line, st.line)}, kind, name, callback_args, callback_guards, body)
    Tonic.Eval.Modules.snapshot_attributes(st.module, Keyword.get(meta, :line, st.line))
    {:ok, st}
  end

  # unquote/1 fragments outside quote blocks
  def expand_unquotes(ast, env) do
    state = new_state([], env)
    caller_unquotes(ast, %{state | in_module: env.module != nil})
  end

  defp caller_unquotes({:quote, _, _} = ast, _state), do: ast
  defp caller_unquotes({:unquote, _, [expression]}, state) do
    {value, _} = expr(expression, state)
    value
  end
  defp caller_unquotes({name, meta, arguments}, state) when is_list(arguments), do: {caller_unquotes(name, state), meta, Enum.map(arguments, &caller_unquotes(&1, state))}
  defp caller_unquotes({left, right}, state), do: {caller_unquotes(left, state), caller_unquotes(right, state)}
  defp caller_unquotes(arguments, state) when is_list(arguments), do: Enum.map(arguments, &caller_unquotes(&1, state))
  defp caller_unquotes(value, _state), do: value

  defp unquote_fragments({:quote, _, _} = q, _st), do: q

  defp unquote_fragments({:unquote, _, [e]}, st) do
    {v, _} = expr(e, st)
    escape(v)
  end

  defp unquote_fragments({:@, _, [{name, _, ctx}]}, %{in_module: true} = st) when is_atom(name) and is_atom(ctx),
    do: escape(Tonic.Eval.Modules.get_attribute(st.module, name))

  defp unquote_fragments({a, m, args}, st) when is_list(args),
    do: {unquote_fragments(a, st), m, Enum.map(args, &unquote_fragments(&1, st))}

  defp unquote_fragments({a, b}, st), do: {unquote_fragments(a, st), unquote_fragments(b, st)}
  defp unquote_fragments(list, st) when is_list(list), do: Enum.map(list, &unquote_fragments(&1, st))
  defp unquote_fragments(other, _st), do: other

  @doc false
  def escape(v) when is_tuple(v) and tuple_size(v) == 2, do: {escape(elem(v, 0)), escape(elem(v, 1))}
  def escape(v) when is_tuple(v), do: {:{}, [], Enum.map(Tuple.to_list(v), &escape/1)}
  def escape(v) when is_list(v), do: Enum.map(v, &escape/1)

  def escape(%{} = m) do
    case m do
      %{__struct__: mod} -> {:%, [], [mod, {:%{}, [], Enum.map(Map.delete(m, :__struct__), fn {k, v} -> {escape(k), escape(v)} end)}]}
      _ -> {:%{}, [], Enum.map(m, fn {k, v} -> {escape(k), escape(v)} end)}
    end
  end

  def escape(v), do: v

  ## Interpreted functions

  @doc false
  def run_function(clauses, args, mod, name) do
    result =
      Enum.find_value(clauses, :tonic_nomatch, fn {params, guard, body, env} ->
        st = %State{module: mod, function: {name, length(args)}, file: env.file, aliases: env.aliases, imports: env.imports, super_target: Map.get(env, :super_target), arguments: args, macro_function: Map.get(env, :macro_function, false)}

        case match_list(params, args, st) do
          {:ok, st2} ->
            if guard(guard, st2) do
              {v, _} = expr(body, st2)
              {:tonic_value, v}
            end

          :error ->
            nil
        end
      end)

    case result do
      {:tonic_value, v} -> v
      :tonic_nomatch -> :erlang.error(:function_clause, args)
    end
  end

  defp kernel_macro(name, arity) do
    {name, arity} in [{:if, 2}, {:unless, 2}, {:to_string, 1}, {:is_nil, 1}, {:then, 2}, {:tap, 2}, {:raise, 1}, {:raise, 2}]
  end

  defp special_or_local(:raise, _meta, [msg], st) do
    {v, st} = expr(msg, st)
    _ = st

    exception =
      cond do
        is_binary(v) -> RuntimeError.exception(v)
        is_atom(v) -> Tonic.Eval.Modules.apply(v, :exception, [[]])
        is_exception(v) -> v
        true -> ArgumentError.exception("raise/1 and reraise/2 expect a module name, string or exception as the first argument, got: #{inspect(v)}")
      end

    :erlang.error(exception)
  end

  defp special_or_local(:raise, _meta, [mod, attrs], st) do
    {m, st} = expr(mod, st)
    {a, _st} = expr(attrs, st)
    :erlang.error(Tonic.Eval.Modules.apply(m, :exception, [a]))
  end

  defp special_or_local(:reraise, _meta, [e, stack], st) do
    {v, st} = expr(e, st)
    {s, _st} = expr(stack, st)
    exception = if is_binary(v), do: RuntimeError.exception(v), else: v
    :erlang.raise(:error, exception, s)
  end

  defp special_or_local(:reraise, _meta, [mod, attrs, stack], st) do
    {m, st} = expr(mod, st)
    {a, st} = expr(attrs, st)
    {s, _st} = expr(stack, st)
    :erlang.raise(:error, m.exception(a), s)
  end

  defp special_or_local(:is_nil, _meta, [a], st) do
    {v, st} = expr(a, st)
    {v == nil, st}
  end

  defp special_or_local(:to_string, _meta, [a], st) do
    {v, st} = expr(a, st)
    {String.Chars.to_string(v), st}
  end

  defp special_or_local(:to_charlist, _meta, [a], st) do
    {v, st} = expr(a, st)
    {List.Chars.to_charlist(v), st}
  end

  defp special_or_local(:then, _meta, [v, f], st) do
    {v, st} = expr(v, st)
    {f, st} = expr(f, st)
    {f.(v), st}
  end

  defp special_or_local(:tap, _meta, [v, f], st) do
    {v, st} = expr(v, st)
    {f, st} = expr(f, st)
    f.(v)
    {v, st}
  end

  defp special_or_local(:match?, _meta, [pat, e], st) do
    {v, st} = expr(e, st)

    {pat, guard} =
      case pat do
        {:when, _, [p, g]} -> {p, g}
        p -> {p, true}
      end

    result =
      case match(pat, v, st) do
        {:ok, st2} -> guard(guard, st2)
        :error -> false
      end

    {result, st}
  end

  defp special_or_local(:binding, _meta, [], st), do: {dump_binding(st), st}

  defp special_or_local(:var!, _meta, [var | _], st), do: expr(var, st)

  defp special_or_local(:dbg, _meta, [code | _], st) do
    {v, st} = expr(code, st)
    IO.puts(:stderr, "[#{st.file}:#{st.line}]\n#{Macro.to_string(code)} #=> #{inspect(v, pretty: true)}\n")
    {v, st}
  end

  defp special_or_local(:destructure, _meta, [left, right], st) do
    {v, st} = expr(right, st)
    n = length(left)

    values =
      case v do
        nil -> List.duplicate(nil, n)
        list -> Enum.take(list ++ List.duplicate(nil, n), n)
      end

    Enum.zip(left, values)
    |> Enum.reduce({values, st}, fn {pat, val}, {acc, st} ->
      case match(pat, val, st) do
        {:ok, st} -> {acc, st}
        :error -> raise MatchError, term: val
      end
    end)
  end

  defp special_or_local(:throw, _meta, [a], st) do
    {v, _} = expr(a, st)
    throw(v)
  end

  defp special_or_local(:exit, _meta, [a], st) do
    {v, _} = expr(a, st)
    exit(v)
  end

  defp special_or_local(:__block__, _meta, exprs, st), do: expr({:__block__, [], exprs}, st)

  defp special_or_local(:sigil_r, meta, args, st), do: expr({:sigil_r, meta, args}, st)

  defp special_or_local(name, meta, args, st) do
    arity = length(args)

    case Map.fetch(@kernel_ops, {name, arity}) do
      {:ok, {m, f}} ->
        {args, st} = exprs(args, st)
        {apply(m, f, args), st}

      :error ->
        call_local(name, args, meta, st)
    end
  end

  defp call_local(name, args, meta, st) do
    arity = length(args)

    cond do
      st.module != nil and Tonic.Eval.Modules.macro?(st.module, name, arity, true) ->
        ast = Tonic.Eval.Modules.call_macro(st.module, name, args, env_of(st), true)
        expr(ast, st)

      st.module != nil and Tonic.Eval.Modules.defines?(st.module, name, arity) ->
        {vals, st} = exprs(args, st)
        {Tonic.Eval.Modules.apply(st.module, name, vals, true), st}

      true ->
        call_import(name, args, arity, meta, st)
    end
  end

  defp call_import(name, args, arity, meta, st) do
    target =
      Enum.find_value(st.imports, fn {mod, opts} ->
        only = Keyword.get(opts, :only)
        except = Keyword.get(opts, :except, [])
        macro = Tonic.Eval.Modules.macro?(mod, name, arity)
        function = Tonic.Eval.Modules.defines?(mod, name, arity, false) or function_exported?(mod, name, arity)

        cond do
          only != nil and only not in [:functions, :macros, :sigils] and {name, arity} not in only -> nil
          {name, arity} in except -> nil
          macro and only != :functions -> {mod, :macro}
          function and only != :macros -> {mod, :function}
          true -> nil
        end
      end)

    case target do
      {mod, :macro} ->
        caller = env_of(%{st | line: Keyword.get(meta, :line, st.line)})
        ast = Tonic.Eval.Modules.call_macro(mod, name, args, caller)
        expr(ast, st)

      {mod, :function} ->
        {vals, st} = exprs(args, st)
        {Tonic.Eval.Modules.apply(mod, name, vals), st}

      nil ->
        if function_exported?(Kernel, name, arity) do
          {vals, st} = exprs(args, st)
          {apply(Kernel, name, vals), st}
        else
          compile_error(st, meta, "undefined function #{name}/#{arity} (there is no such import)")
        end
    end
  end

  ## Functions

  defp make_fun([{:->, _, [args, _]} | _] = clauses, st) do
    arity =
      case args do
        [{:when, _, wargs}] -> length(wargs) - 1
        _ -> length(args)
      end

    run = fn vals ->
      run_clauses(clauses, vals, st, fn -> :erlang.error({:badarity, {:fn, vals}}) |> function_clause(vals) end)
    end

    case arity do
      0 -> fn -> run.([]) end
      1 -> fn a -> run.([a]) end
      2 -> fn a, b -> run.([a, b]) end
      3 -> fn a, b, c -> run.([a, b, c]) end
      4 -> fn a, b, c, d -> run.([a, b, c, d]) end
      5 -> fn a, b, c, d, e -> run.([a, b, c, d, e]) end
      6 -> fn a, b, c, d, e, f -> run.([a, b, c, d, e, f]) end
      7 -> fn a, b, c, d, e, f, g -> run.([a, b, c, d, e, f, g]) end
      8 -> fn a, b, c, d, e, f, g, h -> run.([a, b, c, d, e, f, g, h]) end
      9 -> fn a, b, c, d, e, f, g, h, i -> run.([a, b, c, d, e, f, g, h, i]) end
      10 -> fn a, b, c, d, e, f, g, h, i, j -> run.([a, b, c, d, e, f, g, h, i, j]) end
    end
  end

  defp function_clause(_, _vals), do: :erlang.error(:function_clause)

  defp run_clauses(clauses, vals, st, no_match) do
    Enum.find_value(clauses, :tonic_nomatch, fn {:->, _, [pats, body]} ->
      {pats, guard} = split_guard(pats)

      if length(pats) == length(vals) do
        case match_list(pats, vals, st) do
          {:ok, st2} ->
            if guard(guard, st2) do
              {v, _} = expr(body, st2)
              {:tonic_value, v}
            end

          :error ->
            nil
        end
      end
    end)
    |> case do
      {:tonic_value, v} -> v
      :tonic_nomatch -> no_match.()
    end
  end

  defp split_guard([{:when, _, args}]) do
    {pats, [guard]} = Enum.split(args, -1)
    {pats, guard}
  end

  defp split_guard(pats), do: {pats, true}

  defp guard(true, _st), do: true

  defp guard({:when, _, [a, b]}, st), do: guard(a, st) or guard(b, st)

  defp guard(g, st) do
    try do
      {v, _} = expr(g, %{st | in_guard: true})
      v == true
    rescue
      _ -> false
    catch
      _, _ -> false
    end
  end

  defp cond_clauses([], _st), do: raise(CondClauseError)

  defp cond_clauses([{:->, _, [[c], body]} | rest], st) do
    {v, st2} = expr(c, st)

    if v in [false, nil] do
      cond_clauses(rest, st)
    else
      {value, _} = expr(body, st2)
      value
    end
  end

  ## with / for / try / receive

  defp with_expr(args, st) do
    {clauses, [kw]} = Enum.split(args, -1)
    do_with(clauses, kw, st)
  end

  defp do_with([], kw, st) do
    {v, _} = expr(Keyword.fetch!(kw, :do), st)
    v
  end

  defp do_with([{:<-, _, [pat, e]} | rest], kw, st) do
    {v, st} = expr(e, st)
    {pat, g} = split_guard([pat])

    ok =
      case match_list(pat, [v], st) do
        {:ok, st2} -> if guard(g, st2), do: {:ok, st2}, else: :error
        :error -> :error
      end

    case ok do
      {:ok, st2} ->
        do_with(rest, kw, st2)

      :error ->
        case Keyword.get(kw, :else) do
          nil -> v
          clauses -> run_clauses(clauses, [v], st, fn -> :erlang.error({:else_clause, v}) end)
        end
    end
  end

  defp do_with([e | rest], kw, st) do
    {_, st} = expr(e, st)
    do_with(rest, kw, st)
  end

  defp for_expr(args, st) do
    {gens, [opts]} =
      case List.last(args) do
        kw when is_list(kw) -> Enum.split(args, -1)
        _ -> {args, [[]]}
      end

    into = Keyword.get(opts, :into)
    uniq = Keyword.get(opts, :uniq, false)

    case Keyword.fetch(opts, :reduce) do
      {:ok, acc_ast} ->
        {acc, st} = expr(acc_ast, st)
        clauses = Keyword.fetch!(opts, :do)

        for_loop(gens, st, acc, fn st2, acc ->
          run_clauses(clauses, [acc], st2, fn -> :erlang.error({:case_clause, acc}) end)
        end)

      :error ->
        body = Keyword.fetch!(opts, :do)

        items =
          for_loop(gens, st, [], fn st2, acc ->
            {v, _} = expr(body, st2)
            [v | acc]
          end)
          |> Enum.reverse()

        items = if uniq, do: Enum.uniq(items), else: items

        case into do
          nil ->
            items

          into_ast ->
            {coll, _} = expr(into_ast, st)
            Enum.into(items, coll)
        end
    end
  end

  defp for_loop([], st, acc, fun), do: fun.(st, acc)

  defp for_loop([{:<-, _, [pat, e]} | rest], st, acc, fun) do
    {enum, st} = expr(e, st)
    {pat, g} = split_guard([pat])

    Enum.reduce(enum, acc, fn v, acc ->
      case match_list(pat, [v], st) do
        {:ok, st2} -> if guard(g, st2), do: for_loop(rest, st2, acc, fun), else: acc
        :error -> acc
      end
    end)
  end

  defp for_loop([{:<<>>, _, [{:<-, _, [pat, e]}]} | rest], st, acc, fun) do
    {bin, st} = expr(e, st)
    for_bin(pat, bin, rest, st, acc, fun)
  end

  defp for_loop([filter | rest], st, acc, fun) do
    {v, st2} = expr(filter, st)
    if v in [false, nil], do: acc, else: for_loop(rest, st2, acc, fun)
  end

  defp for_bin(_pat, <<>>, _rest, _st, acc, _fun), do: acc

  defp for_bin({:<<>>, m, segs} = pat, bin, rest, st, acc, fun) do
    case match_binary(segs ++ [{:"::", m, [{:tonic_rest, [], nil}, {:bits, [], nil}]}], bin, st) do
      {:ok, st2} ->
        remaining = Map.fetch!(st2.vars, {:tonic_rest, nil})
        st3 = %{st2 | vars: Map.delete(st2.vars, {:tonic_rest, nil})}
        acc = for_loop(rest, st3, acc, fun)
        for_bin(pat, remaining, rest, st, acc, fun)

      :error ->
        acc
    end
  end

  defp try_expr(kw, st) do
    body = Keyword.fetch!(kw, :do)
    rescue_clauses = Keyword.get(kw, :rescue)
    catch_clauses = Keyword.get(kw, :catch)
    else_clauses = Keyword.get(kw, :else)
    after_body = Keyword.get(kw, :after)

    try do
      {v, st2} = expr(body, st)

      case else_clauses do
        nil -> v
        clauses -> run_clauses(clauses, [v], st2, fn -> :erlang.error({:try_clause, v}) end)
      end
    catch
      kind, reason ->
        stack = __STACKTRACE__
        handle_exception(kind, reason, stack, rescue_clauses, catch_clauses, st)
    after
      if after_body, do: expr(after_body, st)
    end
  end

  defp handle_exception(kind, reason, stack, rescue_clauses, catch_clauses, st) do
    st = %{st | stacktrace: stack}

    result =
      if kind == :error and rescue_clauses do
        exception = Exception.normalize(:error, reason, stack)
        rescue_match(rescue_clauses, exception, st)
      end

    case result do
      {:tonic_value, v} ->
        v

      _ ->
        result =
          if catch_clauses do
            Enum.find_value(catch_clauses, fn {:->, _, [pats, body]} ->
              {pats, g} = split_guard(pats)

              pats =
                case pats do
                  [p] -> [:throw, p]
                  [k, p] -> [k, p]
                end

              case match_list(pats, [kind, reason], st) do
                {:ok, st2} ->
                  if guard(g, st2) do
                    {v, _} = expr(body, st2)
                    {:tonic_value, v}
                  end

                :error ->
                  nil
              end
            end)
          end

        case result do
          {:tonic_value, v} -> v
          _ -> :erlang.raise(kind, reason, stack)
        end
    end
  end

  defp rescue_match(clauses, exception, st) do
    Enum.find_value(clauses, fn {:->, _, [[pat], body]} ->
      {var, mods} =
        case pat do
          {:in, _, [var, mods]} -> {var, mods}
          {name, _, ctx} = var when is_atom(name) and is_atom(ctx) -> {var, :any}
          other -> {nil, [other]}
        end

      matches =
        case mods do
          :any ->
            true

          mods ->
            {mods, _} = expr(List.wrap(mods), st)
            exception.__struct__ in List.wrap(mods)
        end

      if matches do
        st2 =
          case var do
            nil -> st
            {:_, _, _} -> st
            v -> %{st | vars: Map.put(st.vars, var_key(v), exception)}
          end

        {v, _} = expr(body, st2)
        {:tonic_value, v}
      end
    end)
  end

  defp receive_expr(kw, st) do
    clauses = Keyword.get(kw, :do, [])

    {timeout, after_body} =
      case Keyword.get(kw, :after) do
        [{:->, _, [[t], body]}] ->
          {t, _} = expr(t, st)
          {t, body}

        nil ->
          {:infinity, nil}
      end

    deadline = if timeout == :infinity, do: :infinity, else: System.monotonic_time(:millisecond) + timeout
    receive_loop(clauses, deadline, after_body, st)
  end

  defp receive_loop(clauses, deadline, after_body, st) do
    {:messages, msgs} = Process.info(self(), :messages)

    found =
      Enum.find_value(msgs, fn msg ->
        Enum.find_value(clauses || [], fn {:->, _, [pats, body]} ->
          {pats, g} = split_guard(pats)

          case match_list(pats, [msg], st) do
            {:ok, st2} -> if guard(g, st2), do: {msg, body, st2}
            :error -> nil
          end
        end)
      end)

    case found do
      {msg, body, st2} ->
        receive do
          ^msg -> :ok
        end

        {v, _} = expr(body, st2)
        v

      nil ->
        now = System.monotonic_time(:millisecond)

        cond do
          deadline != :infinity and now >= deadline ->
            {v, _} = expr(after_body, st)
            v

          true ->
            wait = if deadline == :infinity, do: :infinity, else: deadline - now
            n = length(msgs)

            receive do
              _ when false -> :ok
            after
              0 -> :ok
            end

            wait_new_message(n, wait)
            receive_loop(clauses, deadline, after_body, st)
        end
    end
  end

  defp wait_new_message(n, wait) do
    {:message_queue_len, len} = Process.info(self(), :message_queue_len)

    if len > n do
      :ok
    else
      step = if wait == :infinity, do: 5, else: min(wait, 5)

      if step <= 0 do
        :ok
      else
        Process.sleep(step)
        wait_new_message(n, if(wait == :infinity, do: :infinity, else: wait - step))
      end
    end
  end

  ## Captures

  defp capture({:/, _, [{{:., _, [mod, fun]}, _, []}, arity]}, _meta, st) when is_integer(arity) do
    {m, _} = expr(mod, st)

    if Tonic.Eval.Modules.loaded?(m) do
      args = for i <- 1..arity//1, do: {:"tonic_c#{i}", [], __MODULE__}
      make_fun([{:->, [], [args, {{:., [], [m, fun]}, [], args}]}], st)
    else
      Function.capture(m, fun, arity)
    end
  end

  defp capture({:/, meta, [{name, _, ctx}, arity]}, _meta, st) when is_atom(name) and is_atom(ctx) and is_integer(arity) do
    if st.module != nil and Tonic.Eval.Modules.defines?(st.module, name, arity) do
      args = for i <- 1..arity//1, do: {:"tonic_c#{i}", [], __MODULE__}
      make_fun([{:->, [], [args, {name, meta, args}]}], st)
    else
      target =
        Enum.find_value(st.imports, fn {mod, _} ->
          if Tonic.Eval.Modules.defines?(mod, name, arity, false) or function_exported?(mod, name, arity), do: mod
        end) || if(function_exported?(Kernel, name, arity), do: Kernel)

      case target do
        nil -> compile_error(st, meta, "undefined function #{name}/#{arity}")
        mod -> capture({:/, meta, [{{:., meta, [mod, name]}, meta, []}, arity]}, meta, st)
      end
    end
  end

  defp capture(expr_ast, meta, st) do
    {body, max} =
      Macro.prewalk(expr_ast, 0, fn
        {:&, _, [n]}, acc when is_integer(n) -> {{:"tonic_capture_#{n}", [], __MODULE__}, max(acc, n)}
        other, acc -> {other, acc}
      end)

    if max == 0 do
      compile_error(st, meta, "invalid args for &, expected one of:\n\n  * &Mod.fun/arity to capture a remote function\n  * &fun/arity to capture a local function\n  * &expr to create an anonymous function")
    end

    args = for i <- 1..max, do: {:"tonic_capture_#{i}", [], __MODULE__}
    make_fun([{:->, [], [args, body]}], st)
  end

  ## quote / unquote

  defp quote_expr(body, st) do
    do_quote(body, st)
  end

  defp do_quote({:unquote, _, [e]}, %{quote_unquote: true} = st) do
    {v, _} = expr(e, st)
    v
  end

  defp do_quote({:__aliases__, _meta, [h | t] = args}, st) when is_atom(h) do
    case List.keyfind(st.aliases, Module.concat([h]), 0) do
      {_, mod} -> {:__aliases__, [alias: Module.concat([mod | t])], args}
      nil -> {:__aliases__, [alias: false], args}
    end
  end

  defp do_quote({:__aliases__, _meta, args}, _st), do: {:__aliases__, [alias: false], args}

  defp do_quote({name, _meta, ctx}, st) when is_atom(name) and is_atom(ctx),
    do: {name, [], quote_context(st)}

  defp do_quote({name, _meta, args}, st) when is_atom(name) and is_list(args) do
    imports = :elixir_dispatch.find_imports([], name, nil)

    meta =
      if imports != [] and not :elixir_import.special_form(name, length(args)),
        do: [context: quote_context(st), imports: imports],
        else: []

    {name, meta, quote_list(args, st)}
  end

  defp do_quote({left, _meta, args}, st) when is_list(args) do
    {do_quote(left, st), [], quote_list(args, st)}
  end

  defp do_quote({a, b}, st), do: {do_quote(a, st), do_quote(b, st)}
  defp do_quote(list, st) when is_list(list), do: quote_list(list, st)
  defp do_quote(other, _st), do: other

  defp quote_context(st), do: st.module || Elixir

  defp quote_list(list, st) do
    Enum.flat_map(list, fn
      {:unquote_splicing, _, [e]} = splice ->
        if st.quote_unquote do
          {v, _} = expr(e, st)
          v
        else
          [do_quote(splice, st)]
        end

      other ->
        [do_quote(other, st)]
    end)
  end

  ## Sigils

  defp sigil_source(parts, st) do
    {vals, st} =
      Enum.reduce(parts, {[], st}, fn
        b, {acc, st} when is_binary(b) ->
          {[b | acc], st}

        other, {acc, st} ->
          {v, st} = expr({:<<>>, [], [other]}, st)
          {[v | acc], st}
      end)

    {IO.iodata_to_binary(Enum.reverse(vals)), st}
  end

  defp split_words(src, mods) do
    words = String.split(src)

    case mods do
      [?a] -> Enum.map(words, &String.to_atom/1)
      [?c] -> Enum.map(words, &String.to_charlist/1)
      _ -> words
    end
  end

  ## in

  defp in?(v, %Range{} = r), do: v in r
  defp in?(v, list) when is_list(list), do: :lists.member(v, list)
  defp in?(v, other), do: Enum.member?(other, v)

  ## Binaries

  defp binary_expr(segs, st) do
    {parts, st} =
      Enum.reduce(segs, {[], st}, fn seg, {acc, st} ->
        {bits, st} = segment(seg, st)
        {[bits | acc], st}
      end)

    {Enum.reduce(Enum.reverse(parts), <<>>, fn b, acc -> <<acc::bitstring, b::bitstring>> end), st}
  end

  defp segment({:"::", _, [value, spec]}, st) do
    {v, st} = expr(value, st)
    {type, size, unit, sign, endian} = seg_spec(spec, st)
    {encode_segment(v, type, size, unit, sign, endian), st}
  end

  defp segment(value, st) do
    {v, st} = expr(value, st)

    cond do
      is_binary(v) -> {v, st}
      is_integer(v) -> {<<v>>, st}
      is_float(v) -> {<<v::float>>, st}
      is_bitstring(v) -> {v, st}
      true -> raise ArgumentError, "argument error"
    end
  end

  defp seg_spec(spec, st) do
    Enum.reduce(seg_parts(spec), {nil, :default, nil, :unsigned, :big}, fn part, {t, s, u, sg, e} ->
      case part do
        {name, _, ctx} when is_atom(ctx) and name in [:integer, :float, :binary, :bits, :bitstring, :bytes, :utf8, :utf16, :utf32] ->
          {name, s, u, sg, e}

        {name, _, ctx} when is_atom(ctx) and name in [:signed, :unsigned] ->
          {t, s, u, name, e}

        {name, _, ctx} when is_atom(ctx) and name in [:big, :little, :native] ->
          {t, s, u, sg, name}

        {:size, _, [n]} ->
          {v, _} = expr(n, st)
          {t, v, u, sg, e}

        {:unit, _, [n]} ->
          {t, s, n, sg, e}

        n when is_integer(n) ->
          {t, n, u, sg, e}

        {name, _, ctx} when is_atom(ctx) ->
          raise CompileError, description: "unknown bitstring specifier: #{name}"
      end
    end)
  end

  defp seg_parts({:-, _, [a, b]}), do: seg_parts(a) ++ seg_parts(b)
  defp seg_parts({:*, _, [s, u]}), do: [{:size, [], [s]}, {:unit, [], [u]}]
  defp seg_parts(other), do: [other]

  defp encode_segment(v, type, size, unit, sign, endian) do
    type = type || :integer

    case type do
      :integer ->
        bits = if size == :default, do: 8, else: size * (unit || 1)
        int_bits(v, bits, sign, endian)

      :float ->
        bits = if size == :default, do: 64, else: size * (unit || 1)

        case endian do
          :little -> <<v::float-little-size(bits)>>
          _ -> <<v::float-size(bits)>>
        end

      t when t in [:binary, :bytes] ->
        if size == :default, do: <<v::binary>>, else: binary_part(v, 0, size * (unit || 8) |> div(8))

      t when t in [:bits, :bitstring] ->
        if size == :default, do: v, else: <<v::bitstring-size(size)>>

      :utf8 ->
        <<v::utf8>>

      :utf16 ->
        if endian == :little, do: <<v::utf16-little>>, else: <<v::utf16>>

      :utf32 ->
        if endian == :little, do: <<v::utf32-little>>, else: <<v::utf32>>
    end
  end

  defp int_bits(v, bits, _sign, :little), do: <<v::little-size(bits)>>
  defp int_bits(v, bits, _sign, _), do: <<v::size(bits)>>

  ## Pattern matching

  def match(pat, v, st) do
    check_pattern(pat)

    case mt(pat, v, %{st | bound: MapSet.new()}) do
      {:ok, st} -> {:ok, %{st | bound: nil}}
      :error -> :error
    end
  end

  defp match_list(pats, vals, st) do
    case mt_list(pats, vals, %{st | bound: MapSet.new()}) do
      {:ok, st} -> {:ok, %{st | bound: nil}}
      :error -> :error
    end
  end

  defp mt_list(pats, vals, st) do
    Enum.zip(pats, vals)
    |> Enum.reduce_while({:ok, st}, fn {p, v}, {:ok, st} ->
      case mt(p, v, st) do
        {:ok, st} -> {:cont, {:ok, st}}
        :error -> {:halt, :error}
      end
    end)
  end

  defp check_pattern({:dbg, _, args}) when is_list(args) do
    raise ArgumentError, "invalid expression in match, dbg is not allowed in patterns"
  end

  defp check_pattern({:^, _, _}), do: :ok
  defp check_pattern({_, _, args}) when is_list(args), do: Enum.each(args, &check_pattern/1)
  defp check_pattern({a, b}), do: check_pattern(a) && check_pattern(b)
  defp check_pattern(list) when is_list(list), do: Enum.each(list, &check_pattern/1)
  defp check_pattern(_), do: :ok

  defp mt({:_, _, ctx}, _v, st) when is_atom(ctx), do: {:ok, st}

  defp mt({:^, meta, [var]}, v, st) do
    case Map.fetch(st.vars, var_key(var)) do
      {:ok, ^v} -> {:ok, st}
      {:ok, _} -> :error
      :error -> compile_error(st, meta, "undefined variable ^#{elem(var, 0)}")
    end
  end

  defp mt({:=, _, [a, b]}, v, st) do
    with {:ok, st} <- mt(a, v, st), do: mt(b, v, st)
  end

  defp mt({name, _, ctx} = var, v, st) when is_atom(name) and is_atom(ctx) and name not in [:__MODULE__] do
    key = var_key(var)

    if String.starts_with?(Atom.to_string(name), "_") do
      {:ok, st}
    else
      bound = st.bound || MapSet.new()

      case {MapSet.member?(bound, key), Map.fetch(st.vars, key)} do
        {true, {:ok, ^v}} ->
          {:ok, st}

        {true, {:ok, _}} ->
          :error

        _ ->
          {:ok, %{st | bound: MapSet.put(bound, key), vars: Map.put(st.vars, key, v)}}
      end
    end
  end

  defp mt(list, v, st) when is_list(list), do: match_list_pat(list, v, st)

  defp mt({a, b}, {va, vb}, st) do
    with {:ok, st} <- mt(a, va, st), do: mt(b, vb, st)
  end

  defp mt({_, _}, _, _st), do: :error

  defp mt({:{}, _, pats}, v, st) when is_tuple(v) do
    if tuple_size(v) == length(pats), do: mt_list(pats, Tuple.to_list(v), st), else: :error
  end

  defp mt({:{}, _, _}, _, _st), do: :error

  defp mt({:%{}, _, kvs}, v, st) when is_map(v) do
    Enum.reduce_while(kvs, {:ok, st}, fn {k, p}, {:ok, st} ->
      {key, _} = expr(unpin(k), st)

      case v do
        %{^key => val} ->
          case mt(p, val, st) do
            {:ok, st} -> {:cont, {:ok, st}}
            :error -> {:halt, :error}
          end

        _ ->
          {:halt, :error}
      end
    end)
  end

  defp mt({:%{}, _, _}, _, _st), do: :error

  defp mt({:%, _, [mod_ast, {:%{}, m, kvs}]}, v, st) when is_map(v) do
    case mod_ast do
      {:^, _, [var]} ->
        {mod, _} = expr(var, st)
        if Map.get(v, :__struct__) == mod, do: mt({:%{}, m, kvs}, v, st), else: :error

      {name, _, ctx} = var when is_atom(name) and is_atom(ctx) ->
        case Map.fetch(v, :__struct__) do
          {:ok, mod} when is_atom(mod) ->
            with {:ok, st} <- mt(var, mod, st), do: mt({:%{}, m, kvs}, v, st)

          _ ->
            :error
        end

      _ ->
        {mod, _} = expr(mod_ast, st)
        if Map.get(v, :__struct__) == mod, do: mt({:%{}, m, kvs}, v, st), else: :error
    end
  end

  defp mt({:%, _, _}, _, _st), do: :error

  defp mt({:<<>>, _, segs}, v, st) when is_bitstring(v), do: match_binary(segs, v, st)
  defp mt({:<<>>, _, _}, _, _st), do: :error

  defp mt({:<>, _, [prefix, rest]}, v, st) when is_binary(v) do
    {p, _} = expr(prefix, st)
    size = byte_size(p)

    case v do
      <<^p::binary-size(size), r::binary>> -> mt(rest, r, st)
      _ -> :error
    end
  end

  defp mt({:<>, _, _}, _, _st), do: :error

  defp mt({:when, meta, _}, _v, st), do: compile_error(st, meta, "invalid pattern")

  defp mt({name, meta, args} = pattern, value, st) when is_atom(name) and is_list(args) do
    target =
      cond do
        st.module != nil and Tonic.Eval.Modules.macro?(st.module, name, length(args), true) -> st.module
        true -> Enum.find_value(st.imports, fn {module, options} ->
          only = Keyword.get(options, :only)
          except = Keyword.get(options, :except, [])
          permitted = only == nil or only == :macros or (is_list(only) and {name, length(args)} in only)
          if permitted and {name, length(args)} not in except and Tonic.Eval.Modules.macro?(module, name, length(args)), do: module
        end)
      end
    if target do
      caller = %{env_of(st) | context: :match, line: Keyword.get(meta, :line, st.line)}
      expanded = Tonic.Eval.Modules.call_macro(target, name, args, caller, true)
      mt(expanded, value, st)
    else
      match_literal(pattern, value, st)
    end
  end

  defp mt(lit, v, st), do: match_literal(lit, v, st)

  defp match_literal(lit, v, st) do
    case lit do
      {_, meta, args} when is_list(args) ->
        {value, _} =
          try do
            expr(lit, st)
          rescue
            _ -> compile_error(st, meta, "invalid pattern in match")
          end

        if value === v, do: {:ok, st}, else: :error

      _ ->
        if lit === v, do: {:ok, st}, else: :error
    end
  end

  defp unpin({:^, _, [var]}), do: var
  defp unpin(other), do: other

  defp match_list_pat(pats, v, st) do
    case :lists.reverse(pats) do
      [{:|, _, [h, t]} | rest] ->
        front = :lists.reverse(rest) ++ [h]
        n = length(front)

        if is_list(v) and length_at_least(v, n) do
          {vf, vt} = Enum.split(v, n)
          with {:ok, st} <- mt_list(front, vf, st), do: mt(t, vt, st)
        else
          :error
        end

      _ ->
        if is_list(v) and length(v) == length(pats), do: mt_list(pats, v, st), else: :error
    end
  end

  defp length_at_least(_, 0), do: true
  defp length_at_least([_ | t], n), do: length_at_least(t, n - 1)
  defp length_at_least(_, _), do: false

  defp match_binary([], <<>>, st), do: {:ok, st}
  defp match_binary([], _, _st), do: :error

  defp match_binary([seg | rest], bin, st) do
    {pat, spec} =
      case seg do
        {:"::", _, [p, spec]} -> {p, spec}
        p -> {p, nil}
      end

    {type, size, unit, sign, endian} =
      case spec do
        nil -> if is_binary(pat), do: {:binary, byte_size(pat), 8, :unsigned, :big}, else: {:integer, :default, nil, :unsigned, :big}
        spec -> seg_spec(spec, st)
      end

    type = type || :integer

    taken =
      case type do
        :integer ->
          bits = if size == :default, do: 8, else: size * (unit || 1)

          case {bin, sign, endian} do
            {<<x::signed-little-size(bits), r::bits>>, :signed, :little} -> {x, r}
            {<<x::signed-size(bits), r::bits>>, :signed, _} -> {x, r}
            {<<x::little-size(bits), r::bits>>, _, :little} -> {x, r}
            {<<x::size(bits), r::bits>>, _, _} -> {x, r}
            _ -> :error
          end

        :float ->
          bits = if size == :default, do: 64, else: size * (unit || 1)

          case bin do
            <<x::float-size(bits), r::bits>> -> {x, r}
            _ -> :error
          end

        t when t in [:binary, :bytes] ->
          cond do
            size == :default and rest == [] and is_binary(bin) -> {bin, <<>>}
            size == :default -> :error
            true -> take_bits(bin, size * (unit || 8))
          end

        t when t in [:bits, :bitstring] ->
          if size == :default, do: {bin, <<>>}, else: take_bits(bin, size * (unit || 1))

        :utf8 ->
          case bin do
            <<c::utf8, r::bits>> -> {c, r}
            _ -> :error
          end

        :utf16 ->
          case bin do
            <<c::utf16, r::bits>> -> {c, r}
            _ -> :error
          end

        :utf32 ->
          case bin do
            <<c::utf32, r::bits>> -> {c, r}
            _ -> :error
          end
      end

    case taken do
      :error ->
        :error

      {x, r} ->
        with {:ok, st} <- mt(pat, x, st), do: match_binary(rest, r, st)
    end
  end

  defp take_bits(bin, n) do
    case bin do
      <<x::bitstring-size(n), r::bitstring>> -> {x, r}
      _ -> :error
    end
  end
end

defmodule Tonic.Eval.Modules do
  @moduledoc false
  # Registry of modules defined by evaluated code (defmodule inside
  # Code.eval_string): their functions run in the interpreter.
  @tab :"$tonic_eval_modules"

  defp tab do
    if :ets.whereis(@tab) == :undefined do
      parent = self()

      spawn(fn ->
        try do
          :ets.new(@tab, [:set, :public, :named_table])
        rescue
          _ -> :ok
        end

        Kernel.send(parent, :tonic_evmod_ready)

        receive do
          :tonic_never -> :ok
        end
      end)

      receive do
        :tonic_evmod_ready -> :ok
      end
    end

    @tab
  end

  def create(mod, file) do
    t = tab()
    # redefinition replaces the previous version
    for {{_, ^mod, _, _} = k, _, _} <- :ets.tab2list(t), do: :ets.delete(t, k)
    :ets.match_delete(t, {{:attr, mod, :_}, :_})
    :ets.match_delete(t, {{:accumulate, mod, :_}, :_})
    :ets.delete(t, {:struct, mod})
    :ets.delete(t, {:enforce, mod})
    :ets.insert(t, {{:mod, mod}, file})
    :ok
  end

  def finish(mod, st) do
    env = Tonic.Eval.env_of(st)
    callbacks = Enum.reverse(List.wrap(get_attribute(mod, :before_compile)))
    st = Enum.reduce(callbacks, st, fn callback, current ->
      {target, fun} = callback_target(callback, :__before_compile__)
      ast =
        if macro?(target, fun, 1) do
          call_macro(target, fun, [env], env)
        else
          apply(target, fun, [env])
        end
      if ast != nil do
        put_attribute(mod, :tonic_callback_expansions, callback_expansions(mod) ++ [ast])
        {_, next} = Tonic.Eval.expr(ast, current)
        next
      else
        current
      end
    end)
    snapshot_attributes(mod, 0)
    :ets.insert(tab(), {{:compile_env, mod}, Tonic.Eval.env_of(st)})
    unless Process.get(:tonic_macro_host), do: run_after_callbacks(mod, <<>>)
    st
  end

  def run_after_callbacks(mod, artifact) do
    [{_, env}] = :ets.lookup(tab(), {:compile_env, mod})
    Enum.each(Enum.reverse(List.wrap(get_attribute(mod, :after_compile))), fn callback ->
      {target, fun} = callback_target(callback, :__after_compile__)
      apply(target, fun, [env, artifact])
    end)
    :ok
  end

  def record_definition(mod, file, line, definition) do
    key = {:source_definitions, mod}
    entries = case :ets.lookup(tab(), key) do
      [{_, definitions}] -> definitions
      [] -> []
    end
    :ets.insert(tab(), {key, [{file, line, definition} | entries]})
  end

  def source_definitions(mod, file, first, last) do
    entries = case :ets.lookup(tab(), {:source_definitions, mod}) do
      [{_, definitions}] -> definitions
      [] -> []
    end
    entries
    |> Enum.reverse()
    |> Enum.filter(fn {source, line, _} -> source == file and line >= first and line <= last end)
    |> Enum.map(fn {_, _, definition} -> definition end)
  end

  def record_attribute_value(mod, file, line, name, value) do
    :ets.insert(tab(), {{:attribute_value, mod, file, line, name}, value})
    :ok
  end

  def attribute_value(mod, file, line, name) do
    case :ets.lookup(tab(), {:attribute_value, mod, file, line, name}) do
      [{_, value}] -> value
      [] -> raise CompileError, file: file, line: line, description: "no compile-time snapshot available for @#{name}"
    end
  end

  def snapshot_attributes(mod, line) do
    attributes = for {{:attr, ^mod, name}, value} <- :ets.tab2list(tab()), name not in [:before_compile, :after_compile, :on_definition, :tonic_callback_expansions], into: %{}, do: {name, value}
    :ets.insert(tab(), {{:snapshot, mod, line}, attributes})
    :ok
  end

  def definition_attributes(mod, line) do
    case :ets.lookup(tab(), {:snapshot, mod, line}) do
      [{_, attributes}] -> attributes
      [] -> %{}
    end
  end

  def callback_expansions(mod), do: get_attribute(mod, :tonic_callback_expansions) || []

  def on_definition(st, kind, name, args, guards, body) do
    Enum.each(Enum.reverse(List.wrap(get_attribute(st.module, :on_definition))), fn callback ->
      {target, fun} = callback_target(callback, :__on_definition__)
      apply(target, fun, [Tonic.Eval.env_of(st), kind, name, args, guards, body])
    end)
  end

  defp callback_target({mod, fun}, _default), do: {mod, fun}
  defp callback_target(mod, default), do: {mod, default}

  defp check_load_error(mod) do
    if error = Map.get(Process.get(:tonic_macro_load_errors) || %{}, mod), do: raise(CompileError, description: error)
  end

  def loaded?(mod), do: is_atom(mod) and :ets.member(tab(), {:mod, mod})

  def make_overridable(mod, behaviour) when is_atom(behaviour) do
    callbacks = List.wrap(get_attribute(behaviour, :callback)) ++ List.wrap(get_attribute(behaviour, :macrocallback))
    functions = Enum.flat_map(callbacks, fn spec ->
      case callback_signature(spec) do
        {name, arity} -> if(defines?(mod, name, arity) or macro?(mod, name, arity, true), do: [{name, arity}], else: [])
        _ -> []
      end
    end)
    make_overridable(mod, functions)
  end

  defp callback_signature({:when, _, [spec | _]}), do: callback_signature(spec)
  defp callback_signature({:"::", _, [head, _]}), do: callback_signature(head)
  defp callback_signature({name, _, args}) when is_atom(name) and is_list(args), do: {name, length(args)}
  defp callback_signature({name, _, ctx}) when is_atom(name) and is_atom(ctx), do: {name, 0}
  defp callback_signature(_), do: nil

  def make_overridable(mod, functions) do
    Enum.each(functions, fn {name, arity} ->
      arity = if macro?(mod, name, arity, true), do: arity + 1, else: arity
      unless lookup(mod, name, arity), do: raise(ArgumentError, "cannot make undefined function overridable")
      :ets.insert(tab(), {{:overridable, mod, name, arity}, true})
    end)
  end

  def prepare_override(mod, name, arity) do
    if :ets.member(tab(), {:overridable, mod, name, arity}) do
      :ets.delete(tab(), {:overridable, mod, name, arity})
      {kind, clauses} = lookup(mod, name, arity)
      hidden = String.to_atom("__super__#{name}__#{:erlang.unique_integer([:positive])}")
      :ets.insert(tab(), {{:def, mod, hidden, arity}, :defp, clauses})
      :ets.delete(tab(), {:def, mod, name, arity})
      :ets.insert(tab(), {{:super, mod, name, arity}, hidden})
    end
    case :ets.lookup(tab(), {:super, mod, name, arity}) do
      [{_, hidden}] -> hidden
      [] -> nil
    end
  end

  def add_clause(mod, kind, name, arity, clause) do
    key = {:def, mod, name, arity}

    case :ets.lookup(tab(), key) do
      [{_, k, clauses}] when k == kind -> :ets.insert(tab(), {key, kind, clauses ++ [clause]})
      [{_, _k, _}] -> raise CompileError, description: "#{kind} #{name}/#{arity} already defined as a different kind"
      [] -> :ets.insert(tab(), {key, kind, [clause]})
    end
  end

  def get_attribute(mod, name) do
    case :ets.lookup(tab(), {:attr, mod, name}) do
      [{_, v}] -> v
      [] -> nil
    end
  end

  def put_attribute(mod, name, value) do
    accumulated = name in [:before_compile, :after_compile, :on_definition, :callback, :macrocallback] or :ets.member(tab(), {:accumulate, mod, name})
    value = if accumulated, do: [value | (get_attribute(mod, name) || [])], else: value
    :ets.insert(tab(), {{:attr, mod, name}, value})
    :ok
  end

  def register_attribute(mod, name, opts) do
    if opts[:accumulate] do
      :ets.insert(tab(), {{:accumulate, mod, name}, true})
      if get_attribute(mod, name) == nil, do: :ets.insert(tab(), {{:attr, mod, name}, []})
    end
    :ok
  end

  def delete_attribute(mod, name) do
    value = get_attribute(mod, name)
    :ets.delete(tab(), {:attr, mod, name})
    value
  end

  def put_struct(mod, fields, exception?, enforce \\ []) do
    fields = if exception?, do: [{:__exception__, true} | fields], else: fields
    :ets.insert(tab(), {{:struct, mod}, fields})
    :ets.insert(tab(), {{:enforce, mod}, enforce})
  end

  def struct_enforce(mod) do
    case :ets.lookup(tab(), {:enforce, mod}) do
      [{_, e}] -> e
      [] -> []
    end
  end

  def struct_fields(mod) when is_atom(mod) do
    if :ets.whereis(@tab) == :undefined do
      nil
    else
      case :ets.lookup(@tab, {:struct, mod}) do
        [{_, f}] -> f
        [] -> nil
      end
    end
  end

  def struct_fields(_), do: nil

  defp lookup(mod, name, arity) do
    if is_atom(mod) and :ets.whereis(@tab) != :undefined do
      case :ets.lookup(@tab, {:def, mod, name, arity}) do
        [{_, kind, clauses}] -> {kind, clauses}
        [] -> nil
      end
    end
  end

  def macro?(mod, name, arity, private \\ false) do
    case lookup(mod, name, arity + 1) do
      {:defmacro, _} -> true
      {:defmacrop, _} -> private
      {_, _} -> false
      nil -> compiled_macro?(mod, name, arity)
    end
  end

  # Macros of compiled (non-prelude) modules are also compiled as
  # :"MACRO-name"/(arity + 1) functions taking the caller's env first.
  defp compiled_macro?(mod, name, arity) do
    is_atom(mod) and not loaded?(mod) and Code.ensure_loaded?(mod) and
      function_exported?(mod, :"MACRO-#{name}", arity + 1)
  end

  def defines?(mod, name, arity, private \\ true) do
    case lookup(mod, name, arity) do
      {:def, _} -> true
      {:defp, _} -> private
      _ -> false
    end
  end

  def call_macro(mod, name, args, caller, _private \\ false) do
    check_load_error(mod)
    case lookup(mod, name, length(args) + 1) do
      {_, clauses} -> Tonic.Eval.run_function(clauses, [caller | args], mod, name)
      nil -> Kernel.apply(mod, :"MACRO-#{name}", [caller | args])
    end
  end

  def apply(mod, fun, args, private \\ false) do
    check_load_error(mod)
    arity = length(args)

    case lookup(mod, fun, arity) do
      {:def, clauses} ->
        Tonic.Eval.run_function(clauses, args, mod, fun)

      {:defp, clauses} when private ->
        Tonic.Eval.run_function(clauses, args, mod, fun)

      _ ->
        case {struct_fields(mod), fun, args} do
          {nil, _, _} ->
            Kernel.apply(mod, fun, args)

          {fields, :__struct__, []} ->
            Map.new([{:__struct__, mod} | fields])

          {fields, :__struct__, [kv]} ->
            Enum.reduce(kv, Map.new([{:__struct__, mod} | fields]), fn {k, v}, acc -> %{acc | k => v} end)

          {fields, :exception, [msg]} when is_binary(msg) ->
            %{Map.new([{:__struct__, mod} | fields]) | message: msg}

          {fields, :exception, [kv]} when is_list(kv) ->
            Enum.reduce(kv, Map.new([{:__struct__, mod} | fields]), fn {k, v}, acc -> %{acc | k => v} end)

          {_fields, :message, [e]} ->
            Map.get(e, :message)

          _ ->
            Kernel.apply(mod, fun, args)
        end
    end
  end
end
