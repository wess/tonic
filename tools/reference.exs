apps = [:elixir, :eex, :logger, :ex_unit, :erts, :kernel, :stdlib, :crypto, :ssl, :public_key]

exports =
  for app <- apps,
      _ = Application.load(app),
      module <- Application.spec(app, :modules) || [],
      Code.ensure_loaded?(module),
      {kind, entries} <-
        if(function_exported?(module, :__info__, 1),
          do: [{:function, module.__info__(:functions)}, {:macro, module.__info__(:macros)}],
          else: [{:function, module.module_info(:exports)}]
        ),
      {name, arity} <- entries,
      name not in [:module_info, :__info__] do
    %{module: Atom.to_string(module), name: Atom.to_string(name), arity: arity,
      kind: Atom.to_string(kind), application: Atom.to_string(app)}
  end

reference = %{
  elixir: System.version(),
  otp: List.to_string(:erlang.system_info(:otp_release)),
  erts: List.to_string(:erlang.system_info(:version)),
  applications: Enum.map(apps, &Atom.to_string/1),
  exports: Enum.sort_by(exports, &{&1.module, &1.name, &1.arity, &1.kind})
}

IO.puts(:json.encode(reference))
