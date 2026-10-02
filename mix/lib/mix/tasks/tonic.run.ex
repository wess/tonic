defmodule Mix.Tasks.Tonic.Run do
  use Mix.Task
  @shortdoc "Compile and run the current Mix project with tonic"
  @moduledoc "Runs using tonic without starting the BEAM application. Arguments after -- reach the native program."
  def run(args), do: Tonic.run("run", args)
end
