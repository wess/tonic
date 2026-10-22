defmodule Tonic do
  @moduledoc """
  Mix integration for native projects.

  Wrap the keyword list returned by `project/0` with `Tonic.native/1` to route
  `mix run`, `mix test`, and `mix release` to the native tasks. Omitting the
  wrapper leaves the normal BEAM workflows intact.
  """

  def native(config) do
    aliases =
      Keyword.merge(config[:aliases] || [],
        run: "tonic.run",
        test: "tonic.test",
        release: "tonic.release"
      )

    Keyword.merge(config,
      compilers: [:tonic],
      prune_code_paths: false,
      consolidate_protocols: false,
      aliases: aliases
    )
  end

  def executable do
    configured = System.get_env("TONIC") || config()[:executable] || "tonic"

    System.find_executable(configured) ||
      Mix.raise(
        "Tonic executable #{inspect(configured)} was not found; install tonic or set TONIC"
      )
  end

  def output do
    config()[:output] ||
      Path.join([Mix.Project.build_path(), "tonic", app()])
  end

  def app, do: to_string(Mix.Project.config()[:app] || Path.basename(File.cwd!()))

  def run("release", args), do: Tonic.Release.run(args)

  def run(command, args) do
    if command == "test" and is_nil(System.get_env("MIX_ENV")),
      do: System.put_env("MIX_ENV", "test")

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

    native_args =
      if command == "test" do
        {compiler, tests} =
          Enum.split_with(args, &(&1 in ["-O0", "-O1", "-O2", "-O3", "-v", "--keep"]))

        [command] ++ compiler ++ ["."] ++ tests
      else
        [command, "." | args]
      end

    status = command(native_args)
    if status != 0, do: System.halt(status)
    :ok
  end

  # nouse_stdio moves the port's pipes to fds 3/4, so tonic and the program it
  # runs write straight to the caller's stdout and stderr. Port programs do not
  # see the BEAM's stdin, so it is copied into a FIFO the program reads instead;
  # closing the FIFO at end of input gives the program a real EOF.
  def command(args, env \\ []) do
    fifo =
      Path.join(
        System.tmp_dir!(),
        "tonicstdin#{System.pid()}x#{System.unique_integer([:positive])}"
      )

    {_, 0} = System.cmd("mkfifo", [fifo])

    try do
      port =
        Port.open({:spawn_executable, System.find_executable("sh")}, [
          :nouse_stdio,
          :exit_status,
          :binary,
          env:
            Enum.map(env, fn {key, value} ->
              {String.to_charlist(key), String.to_charlist(value)}
            end),
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
