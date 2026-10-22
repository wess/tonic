defmodule ReqProbe.MixProject do
  use Mix.Project
  def project do
    [app: :reqprobe, version: "0.1.0", deps: [{:req, "0.7.5"}],
      escript: [main_module: ReqProbe]]
  end
  def application, do: [extra_applications: [:logger, :ssl]]
end
