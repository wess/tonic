defmodule PackageCoverage.MixProject do
  use Mix.Project
  def project do
    [app: :packagecoverage, version: "0.0.1", deps: dependencies(), escript: [main_module: PackageCoverage]]
  end
  defp dependencies do
    [{:jason, "1.4.4"}, {:nimble_options, "1.1.1"}, {:deep_merge, "1.0.0"}]
  end
end
