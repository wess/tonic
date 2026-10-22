#[path = "support/process.rs"]
mod process;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "tonictooling{}{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, path: &str, contents: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    fn tonic(&self, command: &str) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tonic"));
        cmd.current_dir(&self.0).args([command, "."]);
        cmd
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(command: Command) -> Output {
    process::execute(command, Duration::from_secs(240))
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn project(f: &Fixture) {
    f.write(
        "mix.exs",
        r#"defmodule Tooling.MixProject do
  use Mix.Project
  def project, do: [app: :tooling, version: "0.1.0", escript: [main_module: Tooling]]
end
"#,
    );
    f.write("lib/tooling.ex", "defmodule Tooling do\n def answer, do: 42\n def main(args), do: IO.inspect({File.read!(Path.join(:code.priv_dir(:tooling), \"data\")), args})\nend\n");
    f.write("priv/data", "relocated");
}

#[test]
fn native_exunit_runs_helpers_and_propagates_failures() {
    let f = Fixture::new();
    project(&f);
    f.write(
        "config/config.exs",
        "import Config\nconfig :tooling, :compiled_environment, config_env()\n",
    );
    f.write(
        "test/test_helper.exs",
        "ExUnit.start()\nApplication.put_env(:tooling, :helper, 40)\n",
    );
    f.write("test/nested/value_test.exs", "defmodule ToolingTest do\n use ExUnit.Case\n test \"native value\" do\n assert Tooling.answer() == Application.fetch_env!(:tooling, :helper) + 2\n assert Application.fetch_env!(:tooling, :compiled_environment) == :test\n end\nend\n");
    let result = run(f.tonic("test"));
    success(&result);
    assert!(String::from_utf8_lossy(&result.stdout).contains("1 test, 0 failures"));
    f.write("test/nested/value_test.exs", "defmodule ToolingTest do\n use ExUnit.Case\n test \"native failure\" do\n assert Tooling.answer() == 43\n end\nend\n");
    let result = run(f.tonic("test"));
    assert_eq!(
        result.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("1 test, 1 failure"));
    assert!(!f.0.join("_build").exists());
}

#[test]
fn umbrella_root_builds_interdependent_children() {
    let f = Fixture::new();
    f.write("mix.exs", "defmodule Suite.MixProject do\n use Mix.Project\n def project, do: [apps_path: \"apps\", version: \"0.1.0\", escript: [main_module: Beta]]\nend\n");
    f.write("apps/alpha/mix.exs", "defmodule Alpha.MixProject do\n use Mix.Project\n def project, do: [app: :alpha, version: \"0.1.0\"]\nend\n");
    f.write("apps/beta/mix.exs", "defmodule Beta.MixProject do\n use Mix.Project\n def project, do: [app: :beta, version: \"0.1.0\", deps: [{:alpha, in_umbrella: true}]]\nend\n");
    f.write(
        "apps/alpha/lib/alpha.ex",
        "defmodule Alpha do\n def answer, do: 42\nend\n",
    );
    f.write(
        "apps/beta/lib/beta.ex",
        "defmodule Beta do\n def main(args), do: IO.inspect({Alpha.answer(), args})\nend\n",
    );
    let mut cmd = f.tonic("run");
    cmd.args(["--", "umbrella"]);
    let result = run(cmd);
    success(&result);
    assert_eq!(result.stdout, b"{42, [\"umbrella\"]}\n");
    f.write("apps/alpha/test/test_helper.exs", "ExUnit.start()\n");
    f.write("apps/alpha/test/alpha_test.exs", "defmodule AlphaTest do\n use ExUnit.Case\n test \"alpha\", do: assert(Alpha.answer() == 42)\nend\n");
    f.write("apps/beta/test/test_helper.exs", "ExUnit.start()\n");
    f.write("apps/beta/test/beta_test.exs", "defmodule BetaTest do\n use ExUnit.Case\n test \"beta\", do: assert(Alpha.answer() == 42)\nend\n");
    let result = run(f.tonic("test"));
    success(&result);
    assert!(String::from_utf8_lossy(&result.stdout).contains("2 tests, 0 failures"));
}

#[test]
fn mix_native_release_moves_with_its_priv_assets() {
    let f = Fixture::new();
    project(&f);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let archive = f.0.join("tonic.ez");
    let archives = f.0.join("archives");
    let mut cmd = Command::new("mix");
    cmd.current_dir(repo.join("mix"))
        .env("MIX_BUILD_PATH", f.0.join("archivebuild"))
        .args(["archive.build", "-o"])
        .arg(&archive);
    success(&run(cmd));
    let mut cmd = Command::new("mix");
    cmd.current_dir(&f.0)
        .env("MIX_ARCHIVES", &archives)
        .args(["archive.install"])
        .arg(&archive)
        .arg("--force");
    success(&run(cmd));
    f.write(
        "mix.exs",
        r#"defmodule Tooling.MixProject do
  use Mix.Project
  def project, do: Tonic.native(app: :tooling, version: "0.1.0", escript: [main_module: Tooling])
  def application, do: [mod: {Tooling.Application, []}]
end
"#,
    );
    f.write("lib/tooling.ex", r#"defmodule Tooling.Application do
  use Application
  def start(_, _) do
    Application.put_env(:tooling, :started, true)
    Supervisor.start_link([], strategy: :one_for_one)
  end
end
defmodule Tooling do
  def answer, do: 42
  def main(args), do: IO.inspect({File.read!(Path.join(:code.priv_dir(:tooling), "data")), Application.get_env(:tooling, :started), Application.get_env(:tooling, :runtime), args})
end
"#);
    f.write("config/runtime.exs", "import Config\nconfig :tooling, :runtime, System.get_env(\"TOOLING_CONFIG\", \"default\")\n");
    f.write("test/test_helper.exs", "ExUnit.start()\n");
    f.write("test/tooling_test.exs", "defmodule ToolingTest do\n use ExUnit.Case\n test \"application starts before tests\", do: assert(Application.get_env(:tooling, :started))\nend\n");
    let native_mix = |args: &[&str]| {
        let mut cmd = Command::new("mix");
        cmd.current_dir(&f.0)
            .env("MIX_ARCHIVES", &archives)
            .env("TONIC", env!("CARGO_BIN_EXE_tonic"))
            .args(args);
        run(cmd)
    };
    let result = native_mix(&["run", "--", "ordinary"]);
    success(&result);
    assert_eq!(
        result.stdout,
        b"{\"relocated\", true, \"default\", [\"ordinary\"]}\n"
    );
    let result = native_mix(&["test"]);
    success(&result);
    assert!(String::from_utf8_lossy(&result.stdout).contains("1 test, 0 failures"));
    f.write("test/tooling_test.exs", "defmodule ToolingTest do\n use ExUnit.Case\n test \"exit status\", do: assert(false)\nend\n");
    assert_eq!(native_mix(&["test"]).status.code(), Some(2));
    let original = f.0.join("bundle");
    success(&native_mix(&[
        "release",
        "-o",
        original.to_str().unwrap(),
        "--",
        "-O0",
    ]));
    let moved = f.0.join("moved bundle");
    fs::rename(&original, &moved).unwrap();
    fs::remove_dir_all(f.0.join("priv")).unwrap();
    let mut cmd = Command::new(moved.join("bin/tooling"));
    cmd.current_dir("/")
        .env("PATH", "/nonexistent")
        .env("TOOLING_CONFIG", "moved")
        .args(["with spaces", "$(literal)"]);
    let result = run(cmd);
    success(&result);
    assert_eq!(
        result.stdout,
        b"{\"relocated\", true, \"moved\", [\"with spaces\", \"$(literal)\"]}\n"
    );
    assert!(moved.join("release.exs").is_file());
    assert!(moved.join("lib/tooling").is_file());
}

#[test]
fn mix_yecc_sources_generate_and_run_natively_without_modifying_sources() {
    let f = Fixture::new();
    f.write("mix.exs", "defmodule Grammar.MixProject do\n use Mix.Project\n def project, do: [app: :grammar, version: \"0.1.0\", escript: [main_module: Grammar]]\nend\n");
    f.write("lib/grammar.ex", "defmodule Grammar do\n def main(_) do\n {:ok, tokens, _} = :scanner.string(~c\"42\")\n IO.inspect(:value.parse(tokens))\n end\nend\n");
    f.write("src/value.yrl", "Nonterminals value.\nTerminals integer.\nRootsymbol value.\nvalue -> integer : element(3, '$1').\nErlang code.\n");
    f.write("src/scanner.xrl", "Definitions.\nD = [0-9]\nRules.\n{D}+ : {token, {integer, TokenLine, list_to_integer(TokenChars)}}.\nErlang code.\n");
    let result = run(f.tonic("run"));
    success(&result);
    assert_eq!(result.stdout, b"{:ok, 42}\n");
    assert!(!f.0.join("src/value.erl").exists());
    assert!(!f.0.join("src/scanner.erl").exists());
}

#[test]
fn declared_mix_source_generators_run_before_native_collection() {
    let f = Fixture::new();
    f.write("mix.exs", r#"defmodule Generated.MixProject do
  use Mix.Project
  def project, do: [app: :generated, version: "0.1.0", compilers: [:value] ++ Mix.compilers(), tonic: [generators: [:value]], escript: [main_module: Generated]]
end
"#);
    f.write(
        "lib/mix/tasks/compile.value.ex",
        r#"defmodule Mix.Tasks.Compile.Value do
  use Mix.Task.Compiler
  def run(_) do
    File.write!("lib/generated.ex", "defmodule Generated do\n def main(_), do: IO.puts(42)\nend\n")
    {:ok, []}
  end
end
"#,
    );
    let result = run(f.tonic("run"));
    success(&result);
    assert_eq!(result.stdout, b"42\n");
    assert!(f.0.join("lib/generated.ex").is_file());
    assert!(!f.0.join("_build").exists());
}

#[path = "tooling/filtering.rs"]
mod filtering;
