defmodule Mix.Tasks.Tonic.Test do
  use Mix.Task
  @shortdoc "Compile and run the project's ExUnit suite natively"
  @moduledoc "Runs the configured suite natively, or selected files/directories and file:line locations. Supports --only, --include, --exclude, --seed, --max-failures, --timeout and --trace. Defaults to MIX_ENV=test. Test failures exit with status 2."
  def run(args), do: Tonic.run("test", args)
end
