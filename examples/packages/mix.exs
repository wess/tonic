defmodule Packages.MixProject do
  use Mix.Project

  def project do
    [
      app: :packages,
      version: "0.1.0",
      compilers: [:tonic],
      prune_code_paths: false,
      consolidate_protocols: false,
      deps: [{:decimal, "3.0.0"}],
      escript: [main_module: Packages]
    ]
  end
end
