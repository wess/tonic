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
          unless dep.manager in [:mix, :rebar3],
            do:
              Mix.raise(
                "Tonic cannot compile #{dep.app}: unsupported dependency build manager #{inspect(dep.manager)}"
              )

          Mix.Dep.in_dependency(dep, fn _ ->
            metadata =
              if dep.manager == :rebar3,
                do: rebar(dep.app, dep.deps),
                else: application(dep.app, dep.deps)

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

            %{metadata | sources: Enum.reject(metadata.sources, &mix_source?/1)}
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

      # Mix.Project.config/0 at compile time: each project by its root, the
      # directory the macro host compiles that project's code from
      projects =
        Map.new(applications ++ [current], fn metadata ->
          {metadata.root, [app: metadata.app, version: metadata.version]}
        end)

      prefix = prefix <> ":application.set_env(:tonic_config, :projects, #{literal(projects)})\n"

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
        native_sources: Enum.flat_map(applications ++ [current], & &1.native),
        inputs: [configuration] ++ sources ++ runtime_inputs ++ [entry],
        # each mix.exs too: code may call its project module at compile time
        # (Req's `Req.MixProject.legacy_headers_as_lists?()`)
        macro_inputs: Enum.flat_map(applications ++ [current], &mix_file/1) ++ sources,
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
      status = Mix.Dep.format_status(checked)
      # some statuses already say to run mix deps.get
      hint = if status =~ "deps.get", do: "", else: ". Run mix deps.get in this project"
      Mix.raise("Tonic dependency #{dep.app}: #{status}#{hint}")
    end
  end

  defp application(app, children) do
    config = Mix.Project.config()
    compilers = config[:compilers] || Mix.compilers()
    unsupported = compilers -- [:yecc, :leex, :erlang, :elixir, :app, :tonic, :elixir_make]

    unless unsupported == [],
      do:
        Mix.raise("Tonic cannot compile #{app}: unsupported Mix compiler #{inspect(unsupported)}")

    root = File.cwd!()

    native =
      Path.wildcard(Path.join(root, "priv/**/*"))
      |> Enum.filter(&(Path.extname(&1) in [".so", ".dylib", ".dll"]))

    unless native == [] or :elixir_make in compilers,
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
    native = native_sources(app, root, compilers)
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
      |> Keyword.put(:tonic_priv_dir, String.to_charlist(Path.join(root, "priv")))

    [app: app, version: config[:version] || "0.0.0", spec: spec, sources: sources, native: native, root: root]
    |> Map.new()
  end

  # NIF libraries built by elixir_make are compiled from their C sources and
  # linked into the executable. Each entry is "source\tflag\tflag...".
  defp native_sources(app, root, compilers) do
    sources = Path.wildcard(Path.join(root, "c_src/**/*.c")) |> Enum.sort()
    nif? = Enum.any?(sources, &(File.read!(&1) =~ "ERL_NIF_INIT"))

    cond do
      nif? ->
        includes = for dir <- Enum.uniq(Enum.map(sources, &Path.dirname/1)), do: "-I" <> dir
        flags = includes ++ makefile_defines(root)
        for source <- sources, do: Enum.join([source | flags], "\t")

      :elixir_make in compilers ->
        Mix.raise("Tonic cannot compile #{app}: elixir_make builds without NIF C sources in c_src/ are unsupported")

      true ->
        []
    end
  end

  # -D options the dependency's Makefile adds to CFLAGS.
  defp makefile_defines(root) do
    makefile = Enum.find(["Makefile", "GNUmakefile", "makefile"], &File.regular?(Path.join(root, &1)))

    if makefile do
      Regex.scan(~r/^\s*CFLAGS\s*[+:?]?=(.*)$/m, File.read!(Path.join(root, makefile)))
      |> Enum.flat_map(fn [_, value] -> Regex.scan(~r/-D[A-Za-z0-9_]+(?:=[^\s"']+)?/, value) end)
      |> Enum.map(&hd/1)
      |> Enum.reject(&String.starts_with?(&1, "-DSTATIC_ERLANG_NIF"))
      |> Enum.uniq()
    else
      []
    end
  end

  # Mix tasks shipped by dependencies need Mix, which a native program lacks.
  defp mix_source?(path), do: String.contains?(path, "/lib/mix/")

  defp rebar(app, children) do
    root = File.cwd!()

    native =
      Path.wildcard(Path.join(root, "c_src/**/*")) ++
        Path.wildcard(Path.join(root, "priv/**/*.{so,dylib,dll}"))

    unless native == [],
      do:
        Mix.raise(
          "Tonic cannot compile #{app}: NIF/native libraries are unsupported: #{Enum.join(native, ", ")}"
        )

    {:ok, [{:application, ^app, properties}]} =
      root
      |> Path.join("src/#{app}.app.src")
      |> String.to_charlist()
      |> :file.consult()

    required = properties[:applications] || []

    deps =
      for dep <- children, dep.app not in required, Keyword.get(dep.opts, :runtime, true),
          do: dep.app

    spec =
      properties
      |> Keyword.take([:description, :vsn, :mod, :env, :registered, :applications, :optional_applications, :included_applications])
      |> Keyword.put(:applications, required ++ deps)
      |> Keyword.put(:tonic_priv_dir, String.to_charlist(Path.join(root, "priv")))

    [app: app, version: to_string(properties[:vsn]), spec: spec, sources: files(root, ["src"], ".erl"), native: [], root: root]
    |> Map.new()
  end

  defp mix_file(%{root: root}) do
    path = Path.join(root, "mix.exs")
    if File.regular?(path), do: [path], else: []
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
