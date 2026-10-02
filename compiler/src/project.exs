defmodule Tonic.ProjectBridge do
  def export(root, output) do
    Mix.Project.in_project(:tonic_metadata, root, fn _ ->
      config = Mix.Project.config()

      if Mix.Project.umbrella?(),
        do:
          Mix.raise("Tonic requires an application project; build each umbrella child separately")

      compile_config = read_config(config[:config_path] || "config/config.exs")
      Application.put_all_env(compile_config, persistent: true)
      deps = Mix.Dep.cached()
      Enum.each(deps, &validate_dependency/1)

      applications =
        Enum.map(deps, fn dep ->
          if dep.manager != :mix,
            do:
              Mix.raise(
                "Tonic cannot compile #{dep.app}: unsupported dependency build manager #{inspect(dep.manager)}"
              )

          Mix.Dep.in_dependency(dep, fn _ ->
            metadata = application(dep.app, dep.deps)

            matches =
              cond do
                is_nil(dep.requirement) ->
                  true

                is_struct(dep.requirement, Regex) ->
                  Regex.match?(dep.requirement, metadata.version)

                true ->
                  Version.match?(metadata.version, dep.requirement)
              end

            unless matches,
              do:
                Mix.raise(
                  "Tonic dependency #{dep.app}: version #{metadata.version} does not match requirement #{inspect(dep.requirement)}. Run mix deps.get in this project"
                )

            metadata
          end)
        end)

      app = Keyword.fetch!(config, :app)
      current = application(app, Enum.filter(deps, & &1.top_level))
      configuration = Path.join(output, "configuration.exs")
      entry = Path.join(output, "entry.exs")

      prefix =
        ":application.set_env(:tonic_config, :env, #{inspect(Mix.env())})\n" <>
          ":application.set_env(:tonic_config, :target, #{inspect(Mix.target())})\n"

      settings =
        for {name, pairs} <- compile_config, {key, value} <- pairs do
          ":application.set_env(#{literal(name)}, #{literal(key)}, #{literal(value)})\n"
        end

      specs =
        Enum.map(applications ++ [current], fn metadata ->
          ":application.load({:application, #{literal(metadata.app)}, #{literal(metadata.spec)}})\n"
        end)

      File.write!(configuration, [prefix, specs, settings])
      runtime = config[:runtime_config_path] || "config/runtime.exs"
      runtime_inputs = if File.regular?(runtime), do: [Path.expand(runtime)], else: []
      main = get_in(config, [:escript, :main_module])
      start = "{:ok, _} = Application.ensure_all_started(#{literal(app)})\n"

      invoke =
        cond do
          main -> "#{inspect(main)}.main(System.argv())\n"
          Keyword.has_key?(current.spec, :mod) -> "Process.sleep(:infinity)\n"
          true -> ""
        end

      File.write!(entry, [start, invoke])
      sources = Enum.flat_map(applications ++ [current], & &1.sources) |> Enum.uniq()

      metadata = [
        app: Atom.to_string(app),
        inputs: [configuration] ++ sources ++ runtime_inputs ++ [entry],
        macro_inputs: sources,
        config_inputs: [configuration]
      ]

      File.write!(Path.join(output, "manifest.exs"), literal(metadata))
    end)
  end

  defp read_config(path) do
    if File.regular?(path) do
      Config.Reader.read!(path, env: Mix.env(), target: Mix.target())
    else
      []
    end
  end

  defp validate_dependency(dep) do
    checked = Mix.Dep.check_lock(dep)

    valid =
      case checked.status do
        {:ok, _} -> true
        :compile -> true
        {:noappfile, _} -> true
        {:elixirlock, _} -> true
        {:scmlock, _} -> true
        _ -> false
      end

    unless valid do
      Mix.raise(
        "Tonic dependency #{dep.app}: #{Mix.Dep.format_status(checked)}. Run mix deps.get in this project"
      )
    end
  end

  defp application(app, children) do
    config = Mix.Project.config()
    compilers = config[:compilers] || Mix.compilers()
    unsupported = compilers -- [:yecc, :leex, :erlang, :elixir, :app, :tonic]

    unless unsupported == [],
      do:
        Mix.raise("Tonic cannot compile #{app}: unsupported Mix compiler #{inspect(unsupported)}")

    root = File.cwd!()

    native =
      Path.wildcard(Path.join(root, "priv/**/*"))
      |> Enum.filter(&(Path.extname(&1) in [".so", ".dylib", ".dll"]))

    unless native == [],
      do:
        Mix.raise(
          "Tonic cannot compile #{app}: NIF/native libraries are unsupported: #{Enum.join(native, ", ")}"
        )

    elixir_paths = config[:elixirc_paths] || ["lib"]
    erlang_paths = config[:erlc_paths] || ["src"]

    generated =
      Enum.flat_map(erlang_paths, &Path.wildcard(Path.join([root, &1, "**/*"])))
      |> Enum.filter(&(Path.extname(&1) in [".xrl", ".yrl"]))

    unless generated == [],
      do:
        Mix.raise(
          "Tonic cannot compile #{app}: generated lexer/parser sources require an unsupported Mix compiler"
        )

    sources = files(root, elixir_paths, ".ex") ++ files(root, erlang_paths, ".erl")
    project = Mix.Project.get()

    properties =
      if function_exported?(project, :application, 0), do: project.application(), else: []

    included = properties[:included_applications] || []

    runtime_deps =
      for dep <- children,
          Keyword.get(dep.opts, :runtime, true),
          Keyword.get(dep.opts, :app, true) != false,
          dep.app not in included,
          do:
            {dep.app, if(Keyword.get(dep.opts, :optional, false), do: :optional, else: :required)}

    {extra, properties} = Keyword.pop(properties, :extra_applications, [])

    {apps, optional} =
      Mix.Tasks.Compile.App.project_apps(properties, config, extra, fn -> runtime_deps end)

    spec =
      properties
      |> Keyword.put(:applications, apps)
      |> Keyword.put(:optional_applications, optional)
      |> Keyword.put(
        :description,
        String.to_charlist(config[:description] || Atom.to_string(app))
      )
      |> Keyword.put(:vsn, String.to_charlist(config[:version] || "0.0.0"))

    [app: app, version: config[:version] || "0.0.0", spec: spec, sources: sources] |> Map.new()
  end

  defp files(root, paths, extension) do
    Enum.flat_map(paths, fn directory ->
      Path.wildcard(Path.join([root, directory, "**/*#{extension}"]))
    end)
    |> Enum.sort()
  end

  defp literal(value) do
    reject_runtime_values(value)

    inspect(value,
      limit: :infinity,
      printable_limit: :infinity,
      charlists: :as_lists,
      structs: false
    )
  end

  defp reject_runtime_values(value)
       when is_function(value) or is_pid(value) or is_port(value) or is_reference(value),
       do:
         Mix.raise(
           "Tonic project metadata/configuration cannot embed runtime value #{inspect(value)}"
         )

  defp reject_runtime_values(value) when is_map(value),
    do: Enum.each(Map.to_list(value), &reject_runtime_values/1)

  defp reject_runtime_values(value) when is_tuple(value),
    do: Enum.each(Tuple.to_list(value), &reject_runtime_values/1)

  defp reject_runtime_values(value) when is_list(value),
    do: Enum.each(value, &reject_runtime_values/1)

  defp reject_runtime_values(_), do: :ok
end

[root, output] = System.argv()
Tonic.ProjectBridge.export(root, output)
