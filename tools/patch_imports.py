"""Local patches to imported Elixir sources. Each replacement keeps the
number of lines (padding with blank lines) so line numbers stay aligned."""
import sys, os

d = sys.argv[1]

def patch(fname, old, new):
    p = os.path.join(d, fname)
    s = open(p).read()
    assert old in s, (fname, old[:60])
    n_old = old.count('\n')
    n_new = new.count('\n')
    assert n_new <= n_old, (fname, 'replacement longer than original', old[:60])
    new = new + '\n' * (n_old - n_new)
    open(p, 'w').write(s.replace(old, new, 1))

# String: native fast paths for grapheme operations.
patch('string.ex', 'import Kernel, except: [length: 1]\n', '\n')
patch('string.ex', '''  def reverse(string) when is_binary(string) do
    do_reverse(:unicode_util.gc(string), [])
  end
''', '''  def reverse(string) when is_binary(string), do: :tonic.str_reverse(string)
''')
patch('string.ex', '''  def codepoints(string) when is_binary(string) do
    do_codepoints(string)
  end
''', '''  def codepoints(string) when is_binary(string), do: :tonic.str_codepoints(string)
''')
patch('string.ex', '''  def graphemes(string) when is_binary(string), do: do_graphemes(string)''',
      '''  def graphemes(string) when is_binary(string), do: :tonic.str_graphemes(string)''')
patch('string.ex', '''  def length(string) when is_binary(string), do: length(string, 0)''',
      '''  def length(string) when is_binary(string), do: :tonic.str_length(string)''')
patch('string.ex', '''  defp byte_size_remaining_at(unicode, n) do
    case :unicode_util.gc(unicode) do
      [_] -> 0
      [_ | rest] -> byte_size_remaining_at(rest, n - 1)
      [] -> 0
      {:error, <<_, bin::bits>>} -> byte_size_remaining_at(bin, n - 1)
    end
  end

''', '''  defp byte_size_remaining_at(unicode, n) when is_binary(unicode), do: :tonic.str_remaining_at(unicode, n)
  defp byte_size_remaining_at(unicode, n) do
    case :unicode_util.gc(unicode) do
      [_] -> 0
      [_ | rest] -> byte_size_remaining_at(rest, n - 1)
      [] -> 0
      {:error, <<_, bin::bits>>} -> byte_size_remaining_at(bin, n - 1)
    end
  end
''')

# Code.Formatter: module attributes computed with calls tonic's compile-time
# evaluator does not run.
patch('code_formatter.ex', '  @empty empty()\n', '  @empty :doc_nil\n')
patch('code_formatter.ex', '  @ampersand_prec Code.Identifier.unary_op(:&) |> elem(1)\n', '  @ampersand_prec 90\n')


def drop_defs(fname, names, macros_only=()):
    """Blank out (keeping line numbers) every def/defp/defmacro clause of the
    given names: compile-time macro helpers tonic implements natively."""
    import re as _re
    p = os.path.join(d, fname)
    lines = open(p).read().split('\n')
    start_re = _re.compile(r'^  (def|defp|defmacro|defmacrop) ([a-z_][\w?!]*)')
    i = 0
    while i < len(lines):
        m = start_re.match(lines[i])
        if m and (m.group(2) in names or (m.group(2) in macros_only and m.group(1).startswith('defmacro'))):
            j = i + 1
            # until the next top-level definition/attribute or module end
            while j < len(lines) and not (_re.match(r'^  (def|defp|defmacro|defmacrop|@)\b', lines[j]) or lines[j].startswith('end')):
                j += 1
            for k in range(i, j):
                lines[k] = ''
            i = j
        else:
            i += 1
    open(p, 'w').write('\n'.join(lines))


# ExUnit.Assertions: the macros are expanded by the compiler (exunit.rs).
drop_defs('ex_unit_assertions.ex', {
    'assert_receive', 'assert_received', 'refute_receive', 'refute_received',
    'catch_throw', 'catch_exit', 'catch_error', 'translate_assertion', 'translate_operator',
    'escape_quoted', 'extract_args', '__match__', 'collect_pins_from_pattern', 'var_context',
    'collect_vars_from_pattern', 'collect_vars_from_binary', 'has_var?', 'mark_as_generated',
    '__expand_pattern__', 'expand_pattern', 'suppress_warning', 'do_catch', 'do_refute_receive', 'refute_receive_clause',
}, macros_only={'assert', 'refute'})

# JSON: @derive JSON.Encoder runs at runtime (tonic has no compile-time
# __deriving__ expansion): encode derived structs from __derive_opts__/1.
patch('json.ex', '''  defmacro __deriving__(module, opts) do
    fields = module |> Macro.struct_info!(__CALLER__) |> Enum.map(& &1.field)
    fields = fields_to_encode(fields, opts)
    vars = Macro.generate_arguments(length(fields), __MODULE__)
    kv = Enum.zip(fields, vars)

    {io, _prefix} =
      Enum.flat_map_reduce(kv, ?{, fn {field, value}, prefix ->
        key = IO.iodata_to_binary([prefix, :elixir_json.encode_binary(Atom.to_string(field)), ?:])
        {[key, quote(do: encoder.(unquote(value), encoder))], ?,}
      end)

    io = if io == [], do: "{}", else: io ++ [?}]

    quote do
      defimpl JSON.Encoder, for: unquote(module) do
        def encode(%{unquote_splicing(kv)}, encoder) do
          unquote(io)
        end
      end
    end
  end
''', '''  defimpl JSON.Encoder, for: Any do
    def encode(%mod{} = struct, encoder) do
      fields = mod.__info__(:struct) |> Enum.map(& &1.field)
      fields = JSON.Encoder.__fields_to_encode__(fields, mod.__derive_opts__(JSON.Encoder))

      {io, _prefix} =
        Enum.flat_map_reduce(fields, ?{, fn field, prefix ->
          key = IO.iodata_to_binary([prefix, :elixir_json.encode_binary(Atom.to_string(field)), ?:])
          {[key, encoder.(Map.fetch!(struct, field), encoder)], ?,}
        end)

      if io == [], do: "{}", else: io ++ [?}]
    end
  end
''')

# Application: compile_env/compile_env! are plain functions (tonic has no
# __CALLER__-based compile-time tracking).
patch('application.ex', '''  defmacro compile_env(app, key_or_path, default \\\\ nil) do
    if __CALLER__.function do
      raise "Application.compile_env/3 cannot be called inside functions, only in the module body"
    end

    key_or_path = Macro.expand_literals(key_or_path, %{__CALLER__ | function: {:__info__, 1}})

    quote do
      Application.compile_env(__ENV__, unquote(app), unquote(key_or_path), unquote(default))
    end
  end
''', '''  def compile_env(app, key_or_path, default \\\\ nil) when is_atom(app) do
    case fetch_compile_env(app, key_or_path, %{tracers: []}) do
      {:ok, value} -> value
      :error -> default
    end
  end
''')
patch('application.ex', '''  defmacro compile_env!(app, key_or_path) do
    if __CALLER__.function do
      raise "Application.compile_env!/2 cannot be called inside functions, only in the module body"
    end

    key_or_path = Macro.expand_literals(key_or_path, %{__CALLER__ | function: {:__info__, 1}})

    quote do
      Application.compile_env!(__ENV__, unquote(app), unquote(key_or_path))
    end
  end
''', '''  def compile_env!(app, key_or_path) when is_atom(app),
    do: compile_env!(%Macro.Env{}, app, key_or_path)
''')

# ExUnit.Callbacks: setup/setup_all/describe are compiled by exunit.rs.
drop_defs('ex_unit_callbacks.ex', {
    '__register__', '__callbacks__', '__before_compile__', 'setup', 'do_setup', '__setup__',
    'setup_all', 'do_setup_all', '__setup_all__', 'no_describe!', 'validate_callbacks!',
    '__callback__', 'escape', '__describe__', 'compile_setup', 'compile_setup_call',
})
patch('json.ex', '  defp fields_to_encode(fields, opts) do\n', '  def __fields_to_encode__(fields, opts) do\n')

# Inspect: @derive Inspect is handled at runtime by Inspect.Any (tonic
# generates __inspect_derive__/0 for deriving structs).
drop_defs('inspect.ex', {'__deriving__', 'validate_option'})
patch('inspect.ex', '''      {dunder, fields} ->
        if Map.keys(dunder) == Map.keys(struct) do
          infos =
            for %{field: field} = info <- fields,
                field not in [:__struct__, :__exception__],
                do: info

          Inspect.Map.inspect(struct, Macro.inspect_atom(:literal, module), infos, opts)''', '''      {dunder, fields} ->
        if Map.keys(dunder) == Map.keys(struct) do
          {inspect_module, infos} = Tonic.Internal.inspect_infos(module, struct, dunder, fields)
          inspect_module.inspect(struct, Macro.inspect_atom(:literal, module), infos, opts)''')
patch('inspect.ex', '''require Protocol

Protocol.derive(
  Inspect,
  Macro.Env,
  only: [
    :module,
    :file,
    :line,
    :function,
    :context,
    :aliases,
    :requires,
    :functions,
    :macros,
    :macro_aliases,
    :context_modules,
    :lexical_tracker
  ]
)''', '')
# Function: tonic's native formatting of closures.
patch('inspect.ex', '''    fun_info = Function.info(function)
    mod = fun_info[:module]
    name = fun_info[:name]

    cond do''', '''    if function, do: :tonic.fun_to_string(function), else: nil
  end

  def __real_inspect__(fun_info, mod, name) do
    cond do''')
