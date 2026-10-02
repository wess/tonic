defmodule Mix.Tasks.Tonic.Check do
  use Mix.Task
  @shortdoc "Check the current Mix project's native compilation"
  @moduledoc "Checks the current project using tonic. Pass --deps-get to fetch dependencies first."
  def run(args), do: Tonic.run("check", args)
end
