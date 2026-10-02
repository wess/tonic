defmodule Mix.Tasks.Compile.Tonic do
  use Mix.Task.Compiler
  @shortdoc "Compile a native artifact with tonic"
  @moduledoc """
  Opt in with compilers: [:tonic], prune_code_paths: false so Mix retains archive tasks,
  and consolidate_protocols: false to avoid BEAM protocol consolidation.
  Produces a native executable and manifest, without producing BEAM modules.
  Rebuilds on every invocation: evaluated Mix configuration, dependencies, and
  native toolchain settings can change independently of source modification times.
  """

  def run(_args) do
    artifact = Tonic.output()
    File.mkdir_p!(Path.dirname(artifact))
    temporary = artifact <> ".#{System.pid()}.#{System.unique_integer([:positive])}"
    result = Tonic.command(["build", ".", "-o", temporary])

    case result do
      0 ->
        File.rename!(temporary, artifact)
        File.mkdir_p!(Path.dirname(manifest()))
        File.write!(manifest(), :erlang.term_to_binary(artifact))
        {:ok, []}

      status ->
        File.rm(temporary)
        Mix.shell().error("Tonic compilation failed with exit status #{status}")
        {:error, []}
    end
  end

  def manifests, do: [manifest()]

  def clean do
    case File.read(manifest()) do
      {:ok, receipt} -> File.rm(:erlang.binary_to_term(receipt, [:safe]))
      _ -> :ok
    end

    File.rm(Tonic.output())
    File.rm(manifest())
    :ok
  end

  defp manifest, do: Path.join(Mix.Project.manifest_path(), "compile.tonic")
end
