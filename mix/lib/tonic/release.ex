defmodule Tonic.Release do
  @moduledoc false

  def run(args) do
    Mix.Project.get!()

    {options, args, invalid} =
      OptionParser.parse(args, strict: [output: :string], aliases: [o: :output])

    if invalid != [],
      do:
        Mix.raise(
          "unsupported release options: #{inspect(invalid)}; compiler options go after --"
        )

    args = Enum.drop_while(args, &(&1 == "--"))

    destination =
      Path.expand(options[:output] || Path.join([Mix.Project.build_path(), "tonic", "release"]))

    stage = destination <> ".#{System.pid()}.#{System.unique_integer([:positive])}"
    name = Tonic.app()

    if File.exists?(destination),
      do:
        Mix.raise(
          "release destination already exists: #{destination}; choose -o DIR or remove it first"
        )

    try do
      File.mkdir_p!(Path.join(stage, "lib"))
      File.mkdir_p!(Path.join(stage, "bin"))
      native = Path.join(stage, "lib/#{name}")
      status = Tonic.command(["build", ".", "-o", native | args], [{"TONIC_RELEASE_BUILD", "1"}])

      if status != 0 do
        File.rm_rf!(stage)
        System.halt(status)
      end

      copy_priv(stage)
      launcher = Path.join(stage, "bin/#{name}")

      File.write!(launcher, """
      #!/bin/sh
      set -eu
      TONIC_RELEASE_ROOT=$(CDPATH= cd -- "${0%/*}/.." && pwd)
      export TONIC_RELEASE_ROOT
      exec "$TONIC_RELEASE_ROOT/lib/#{name}" "$@"
      """)

      File.chmod!(launcher, 0o755)

      File.write!(
        Path.join(stage, "release.exs"),
        inspect([app: name, version: Mix.Project.config()[:version], target: :native],
          pretty: true
        ) <> "\n"
      )

      File.rename!(stage, destination)
      Mix.shell().info("Native release created at #{destination}")
    after
      File.rm_rf!(stage)
    end
  end

  defp copy_priv(stage) do
    deps = Enum.map(Mix.Dep.cached(), &{to_string(&1.app), &1.opts[:dest]})

    projects =
      if Mix.Project.umbrella?(),
        do: Enum.map(Mix.Project.apps_paths(), fn {app, path} -> {to_string(app), path} end),
        else: [{Tonic.app(), File.cwd!()}]

    Enum.each(Enum.uniq(deps ++ projects), fn {app, root} ->
      source = Path.join(root, "priv")

      if File.dir?(source) do
        destination = Path.join([stage, "priv", app])
        File.mkdir_p!(Path.dirname(destination))
        File.cp_r!(source, destination)
      end
    end)
  end
end
