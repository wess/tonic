ExUnit.start()

defmodule Tonic.ArchiveTest do
  use ExUnit.Case, async: false

  setup do
    root = Path.join(System.tmp_dir!(), "tonicmix#{System.unique_integer([:positive])}")
    File.mkdir_p!(Path.join(root, "lib"))
    File.write!(Path.join(root, "lib/main.ex"), "deliberately invalid BEAM source")
    executable = Path.join(root, "native")

    File.write!(executable, """
    #!/usr/bin/env python3
    import json, os, sys
    with open(os.environ['CAPTURE'], 'a') as capture:
        capture.write(json.dumps(sys.argv[1:]) + '\\n')
    if sys.argv[1] == 'build' and '-o' in sys.argv:
        open(sys.argv[sys.argv.index('-o') + 1], 'w').write(os.environ.get('NATIVEVALUE', 'native artifact'))
    print('native output')
    if os.environ.get('NATIVEDIAGNOSTIC'):
        print('native diagnostic', file=sys.stderr)
    sys.exit(int(os.environ.get('FAKESTATUS', '0')))
    """)

    File.chmod!(executable, 0o755)

    File.write!(Path.join(root, "mix.exs"), """
    defmodule Sample.MixProject do
      use Mix.Project
      def project, do: [app: :sample, version: "0.1.0", compilers: [:tonic], prune_code_paths: false, consolidate_protocols: false, tonic: [executable: #{inspect(executable)}]]
      def application, do: [mod: {MissingApplication, []}]
    end
    """)

    archives = Path.join(root, "archives")
    env = [{"MIX_ARCHIVES", archives}, {"TONIC", nil}, {"CAPTURE", Path.join(root, "capture")}]
    archive = Path.expand("../tonic.ez", __DIR__)

    {_, 0} =
      System.cmd("mix", ["archive.install", archive, "--force"],
        env: env,
        cd: root,
        stderr_to_stdout: true
      )

    on_exit(fn -> File.rm_rf!(root) end)
    {:ok, root: root, env: env, executable: executable}
  end

  defp mix(context, args, extraenv \\ []) do
    System.cmd("mix", args,
      cd: context.root,
      env: context.env ++ extraenv,
      stderr_to_stdout: true
    )
  end

  defp capture(context),
    do: File.read!(Path.join(context.root, "capture")) |> String.split("\n", trim: true)

  test "archive tasks build run check without BEAM compilation or startup", context do
    assert {_, 0} = mix(context, ["tonic.build", "-O2"])
    assert File.read!(Path.join(context.root, "_build/dev/tonic/sample")) == "native artifact"

    assert {"native output\n", 0} =
             mix(context, ["tonic.run", "--", "with spaces", "$(literal)", "--deps-get"])

    assert {"native output\n", 0} = mix(context, ["tonic.check"])
    [build, run, check] = capture(context)
    assert build =~ ~s|["build", ".", "-O2", "-o",|
    assert run == ~s|["run", ".", "--", "with spaces", "$(literal)", "--deps-get"]|
    assert check == ~s|["check", "."]|
    assert Path.wildcard(Path.join(context.root, "_build/**/*.beam")) == []
  end

  test "explicit build output and exit status pass through", context do
    assert {_, 0} = mix(context, ["tonic.build", "-o", "custom"])
    assert File.regular?(Path.join(context.root, "custom"))
    assert {_, 0} = mix(context, ["tonic.build", "--", "--literal"])
    assert Enum.at(capture(context), 1) =~ ~r/"-o".*"--".*"--literal"/
    assert {"native output\n", 17} = mix(context, ["tonic.run"], [{"FAKESTATUS", "17"}])
  end

  test "TONIC overrides project executable and unavailable executable reports clearly", context do
    assert {output, status} = mix(context, ["tonic.check"], [{"TONIC", "unavailabletonicbinary"}])
    assert status != 0
    assert output =~ "was not found"
    assert {_, 0} = mix(context, ["tonic.check"], [{"TONIC", context.executable}])
  end

  test "compiler rebuilds evaluated configuration and clean removes native outputs", context do
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])
    assert length(capture(context)) == 2
    assert Path.wildcard(Path.join(context.root, "_build/**/*.beam")) == []
    File.write!(Path.join(context.root, "lib/main.ex"), "changed source")
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])
    assert {_, 0} = mix(context, ["compile", "--no-deps-check", "--force"])
    assert length(capture(context)) == 4
    assert {_, 0} = mix(context, ["clean"])
    refute File.exists?(Path.join(context.root, "_build/dev/tonic/sample"))
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])
    assert length(capture(context)) == 5
  end

  test "failed compilation never updates successful manifest", context do
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])
    manifest = Path.join(context.root, "_build/dev/lib/sample/.mix/compile.tonic")
    original = File.read!(manifest)
    File.write!(Path.join(context.root, "lib/main.ex"), "changed source")

    assert {_, status} =
             mix(context, ["compile", "--no-deps-check"], [
               {"FAKESTATUS", "19"},
               {"NATIVEVALUE", "failed artifact"}
             ])

    assert status != 0
    assert File.read!(manifest) == original
    assert File.read!(Path.join(context.root, "_build/dev/tonic/sample")) == "native artifact"
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])
    assert length(capture(context)) == 3
  end

  test "custom source paths and native environment changes rebuild", context do
    project = Path.join(context.root, "mix.exs")

    File.write!(
      project,
      String.replace(
        File.read!(project),
        "version: \"0.1.0\",",
        "version: \"0.1.0\", elixirc_paths: [\"source\"],"
      )
    )

    File.mkdir_p!(Path.join(context.root, "source"))
    source = Path.join(context.root, "source/main.ex")
    File.write!(source, "custom source")
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])
    File.write!(source, "modified custom source")
    assert {_, 0} = mix(context, ["compile", "--no-deps-check"])

    assert {_, 0} =
             mix(context, ["compile", "--no-deps-check"], [{"TONIC_CC", "anothercompiler"}])

    assert length(capture(context)) == 3
  end

  test "dependency fetch convenience stays out of native compiler options", context do
    assert {_, 0} = mix(context, ["tonic.check", "--deps-get"])
    assert capture(context) == [~s|["check", "."]|]
  end

  test "native stderr is kept separate from stdout", context do
    assert {"native output\n", 0} =
             System.cmd("mix", ["tonic.run"],
               cd: context.root,
               env: context.env ++ [{"NATIVEDIAGNOSTIC", "1"}]
             )
  end

  test "archive includes its own complete license and notice" do
    archive = Path.expand("../tonic.ez", __DIR__)
    assert {:ok, files} = :zip.extract(String.to_charlist(archive), [:memory])

    license =
      Enum.find_value(files, fn {path, contents} ->
        if List.to_string(path) == "tonic/priv/license", do: contents
      end)

    notice =
      Enum.find_value(files, fn {path, contents} ->
        if List.to_string(path) == "tonic/priv/notice", do: contents
      end)

    assert license == File.read!(Path.expand("../../license", __DIR__))
    assert notice =~ "Copyright 2026 Tonic contributors"
    assert notice =~ "does not bundle"
  end
end
