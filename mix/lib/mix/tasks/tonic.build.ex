defmodule Mix.Tasks.Tonic.Build do
  use Mix.Task
  @shortdoc "Build the current Mix project as a native executable"
  @moduledoc "Builds using tonic. Pass native compiler options directly, or --deps-get to fetch dependencies first."
  def run(args), do: Tonic.run("build", args)
end
