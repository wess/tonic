defmodule Tonic do
  @moduledoc false

  def executable do
    configured = System.get_env("TONIC") || config()[:executable] || "tonic"

    System.find_executable(configured) ||
      Mix.raise(
        "Tonic executable #{inspect(configured)} was not found; install tonic or set TONIC"
      )
  end

  def output do
    config()[:output] ||
      Path.join([Mix.Project.build_path(), "tonic", to_string(Mix.Project.config()[:app])])
  end

  def run(command, args) do
    Mix.Project.get!()
    {options, runtime} = Enum.split_while(args, &(&1 != "--"))
    {fetch, options} = Enum.split_with(options, &(&1 == "--deps-get"))
    if fetch != [], do: Mix.Task.run("deps.get")

    options =
      if command == "build" and "-o" not in options,
        do: options ++ ["-o", output()],
        else: options

    if command == "build", do: File.mkdir_p!(Path.dirname(outputarg(options)))
    args = options ++ runtime
    status = command([command, "." | args])
    if status != 0, do: System.halt(status)
    :ok
  end

  def command(args) do
    {_, status} =
      System.cmd(executable(), args, into: IO.stream(:stdio, :line))

    status
  end

  defp config, do: Mix.Project.config()[:tonic] || []

  defp outputarg(args) do
    case Enum.drop_while(args, &(&1 != "-o")) do
      ["-o", path | _] -> path
      _ -> Mix.raise("-o requires an output path")
    end
  end
end
