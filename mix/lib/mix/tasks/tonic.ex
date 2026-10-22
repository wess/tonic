defmodule Mix.Tasks.Tonic do
  use Mix.Task
  @shortdoc "Build, run or check the current Mix project natively with tonic"
  @moduledoc """
  Compiles the current Mix project to a native executable with tonic.

      mix tonic build [-o PATH] [options]   build the executable (default _build/ENV/tonic/APP)
      mix tonic run [options] [-- ARGS]     build and run it, passing ARGS
      mix tonic check [options]             parse and expand without building
      mix tonic test [FILES] [options]      run ExUnit tests natively (MIX_ENV=test)
      mix tonic release [-o DIR] [options]  bundle a native executable and priv assets
      mix tonic version                     print the tonic compiler version

  Options are passed to the tonic compiler (for example -O3 or --keep);
  --deps-get fetches dependencies first. The same commands are available as
  `mix tonic.build`, `mix tonic.run` and `mix tonic.check`.
  """

  @commands ["build", "run", "check", "test", "release"]

  def run([command | args]) when command in @commands, do: Tonic.run(command, args)

  def run(["version" | _]) do
    status = Tonic.command(["--version"])
    if status != 0, do: System.halt(status)
    :ok
  end

  def run([help | _]) when help in ["help", "--help", "-h"], do: Mix.shell().info(@moduledoc)
  def run([]), do: Mix.shell().info(@moduledoc)

  def run([other | _]) do
    Mix.raise(
      "unknown tonic command #{inspect(other)}; expected one of: build, run, check, test, release, version"
    )
  end
end
