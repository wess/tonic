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

  # nouse_stdio moves the port's pipes to fds 3/4, so tonic and the program it
  # runs write straight to the caller's stdout and stderr. Port programs do not
  # see the BEAM's stdin, so it is copied into a FIFO the program reads instead;
  # closing the FIFO at end of input gives the program a real EOF.
  def command(args) do
    fifo = Path.join(System.tmp_dir!(), "tonicstdin#{System.unique_integer([:positive])}")
    {_, 0} = System.cmd("mkfifo", [fifo])

    try do
      port =
        Port.open({:spawn_executable, System.find_executable("sh")}, [
          :nouse_stdio,
          :exit_status,
          :binary,
          args: ["-c", ~s(f=$1; shift; exec "$@" < "$f"), "sh", fifo, executable() | args]
        ])

      forwarder = spawn(fn -> forward(fifo) end)

      receive do
        {^port, {:exit_status, status}} ->
          Process.exit(forwarder, :kill)
          status
      end
    after
      File.rm(fifo)
    end
  end

  defp forward(fifo) do
    {:ok, out} = File.open(fifo, [:write, :raw, :binary])
    copy(out)
  end

  defp copy(out) do
    case IO.binread(:stdio, 65_536) do
      data when is_binary(data) ->
        if :file.write(out, data) == :ok, do: copy(out), else: File.close(out)

      _ ->
        File.close(out)
    end
  end

  defp config, do: Mix.Project.config()[:tonic] || []

  defp outputarg(args) do
    case Enum.drop_while(args, &(&1 != "-o")) do
      ["-o", path | _] -> path
      _ -> Mix.raise("-o requires an output path")
    end
  end
end
