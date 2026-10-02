# Macro.Env (subset); Macro itself is imported from Elixir (lib/ex/macro.ex).
defmodule Macro.Env do
  defstruct [
    :module,
    :file,
    :line,
    :function,
    :context,
    aliases: [],
    requires: [],
    functions: [],
    macros: [],
    macro_aliases: [],
    context_modules: [],
    lexical_tracker: nil,
    tracers: [],
    versioned_vars: %{}
  ]

  def stacktrace(%{module: nil, file: file, line: line}), do: [{:elixir_compiler_0, :__FILE__, 1, rel_location(file, line)}]
  def stacktrace(%{module: module, function: nil, file: file, line: line}), do: [{module, :__MODULE__, 0, rel_location(file, line)}]
  def stacktrace(%{module: module, function: {name, arity}, file: file, line: line}), do: [{module, name, arity, rel_location(file, line)}]

  defp rel_location(file, line), do: [file: String.to_charlist(Path.relative_to_cwd(file)), line: line]

  def location(%{file: file, line: line}), do: [file: file, line: line]
  def vars(_env), do: []

  def in_match?(%{context: context}), do: context == :match
  def in_guard?(%{context: context}), do: context == :guard
  def has_var?(_env, _var), do: false
  def required?(%{requires: requires}, mod) when is_atom(mod), do: mod in requires
  def to_match(env), do: %{env | context: :match}
  def prune_compile_info(env), do: %{env | lexical_tracker: nil, tracers: []}

  def fetch_alias(%{aliases: aliases}, atom) when is_atom(atom),
    do: Keyword.fetch(aliases, :"Elixir.#{atom}")

  def define_alias(env, _meta, module, opts \\ []) do
    as = Keyword.get(opts, :as, Module.concat([List.last(Module.split(module))]))
    {:ok, %{env | aliases: List.keystore(env.aliases, as, 0, {as, module})}}
  end

  def define_require(env, _meta, module, _opts \\ []), do: {:ok, %{env | requires: [module | env.requires]}}
  def define_import(env, _meta, _module, _opts \\ []), do: {:ok, env}

  def expand_alias(env, meta, list, _opts \\ []) do
    case :elixir_aliases.expand(meta, list, env, false) do
      atom when is_atom(atom) -> {:alias, atom}
      _ -> :error
    end
  end

  # Macros available to tonic at this point are those of modules
  # evaluated by Tonic.Eval (the macro host); Kernel's are compiled in.
  def expand_import(env, _meta, name, arity, _opts \\ []) do
    if arity == 2 and name in [:sigil_c, :sigil_C, :sigil_s, :sigil_S, :sigil_w, :sigil_W] do
      {:macro, Kernel, fn meta, args ->
        {value, _} = Tonic.Eval.eval_quoted({name, meta, args}, [], env)
        Tonic.Eval.escape(value)
      end}
    else
      expand_registered_import(env, name, arity)
    end
  end

  defp expand_registered_import(env, name, arity) do
    candidates = [env.module | Enum.map(env.macros, &elem(&1, 0))] |> Enum.reject(&is_nil/1)

    case Enum.find(candidates, &Tonic.Eval.Modules.macro?(&1, name, arity, &1 == env.module)) do
      nil ->
        if function_exported?(Kernel, name, arity), do: {:function, Kernel, name}, else: {:error, :not_found}

      mod ->
        {:macro, mod, fn _meta, args -> Tonic.Eval.Modules.call_macro(mod, name, args, env) end}
    end
  end

  def expand_require(env, _meta, module, name, arity, _opts \\ []) do
    if Tonic.Eval.Modules.macro?(module, name, arity) do
      {:macro, module, fn _meta, args -> Tonic.Eval.Modules.call_macro(module, name, args, env) end}
    else
      :error
    end
  end
end
