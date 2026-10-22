defmodule Mix.Tasks.Tonic.Release do
  use Mix.Task
  @shortdoc "Bundle a native executable and application priv assets"
  @moduledoc "Creates a relocatable native directory with bin/APP, lib/APP and priv/APP. Use -o DIR for the destination and -- before compiler options. No BEAM or ERTS files are produced."
  def run(args), do: Tonic.run("release", args)
end
