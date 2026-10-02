use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tonicpackages{}{}{}",
        std::process::id(),
        name,
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn execute(mut command: Command, timeout: Duration) -> Output {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .unwrap_or_else(|error| panic!("cannot run {command:?}: {error}"));
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |mut stream: Box<dyn std::io::Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            bytes
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            unsafe {
                extern "C" {
                    fn kill(pid: i32, signal: i32) -> i32;
                }
                kill(-(child.id() as i32), 9);
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("command timed out: {command:?}");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

fn tonic(root: &Path, env: &str, target: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command
        .current_dir(root)
        .env("MIX_ENV", env)
        .env("MIX_TARGET", target)
        .env("TONIC_PACKAGE_RUNTIME", "duringrun")
        .args(args);
    execute(command, Duration::from_secs(180))
}

fn success(result: &Output) {
    assert!(
        result.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

fn dependency(root: &Path, app: &str, project: &str, source: &str) {
    write(
        root,
        "mix.exs",
        &format!(
            "defmodule {project}.MixProject do\n use Mix.Project\n def project, do: [app: :{app}, version: \"0.1.0\"]\nend\n"
        ),
    );
    write(root, "lib/library.ex", source);
}

#[test]
fn computed_mix_projects_resolve_transitive_dependencies_and_start_applications() {
    let root = scratch("graph");
    let project = root.join("project");
    write(
        &root.join("leaf"),
        "mix.exs",
        r#"defmodule PackageLeaf.MixProject do
  use Mix.Project
  def project, do: [app: :packageleaf, version: "0.1.0"]
  def application, do: [mod: {PackageLeaf, []}]
end
"#,
    );
    write(
        &root.join("leaf"),
        "lib/leaf.ex",
        r#"defmodule PackageLeaf do
  def start(_, _), do: (Application.put_env(:packageleaf, :started, true); {:ok, self()})
  def answer(), do: 20
end
"#,
    );
    write(
        &root.join("middle"),
        "mix.exs",
        r#"defmodule PackageMiddle.MixProject do
  use Mix.Project
  def project, do: [app: :packagemiddle, version: "0.1.0", deps: dependencies()]
  defp dependencies, do: [{:packageleaf, path: "../leaf"}]
end
"#,
    );
    write(
        &root.join("middle"),
        "lib/middle.ex",
        "defmodule PackageMiddle do\n def answer(), do: PackageLeaf.answer() + 1\nend\n",
    );
    dependency(
        &root.join("testonly"),
        "packagetest",
        "PackageTest",
        "defmodule PackageTest do\n def value(), do: :present\nend\n",
    );
    dependency(
        &root.join("targetonly"),
        "packagetarget",
        "PackageTarget",
        "defmodule PackageTarget do\n def value(), do: :embedded\nend\n",
    );
    write(
        &root.join("compileonly"),
        "mix.exs",
        r#"defmodule PackageCompile.MixProject do
  use Mix.Project
  def project, do: [app: :packagecompile, version: "0.1.0"]
  def application, do: [mod: {PackageCompile, []}]
end
"#,
    );
    write(
        &root.join("compileonly"),
        "lib/compile.ex",
        r#"defmodule PackageCompile do
  def start(_, _), do: raise("runtime false dependency started")
  def answer(), do: 21
end
"#,
    );
    write(
        &project,
        "mix.exs",
        r#"defmodule PackageRoot.MixProject do
  use Mix.Project
  def project do
    [app: application_name(), version: "0.1.0", elixirc_paths: source_paths(Mix.env()), deps: dependencies(), escript: [main_module: PackageRoot]]
  end
  def application, do: [mod: {PackageRoot, []}, extra_applications: [:logger]]
  defp application_name, do: String.to_atom("package" <> "root")
  defp source_paths(:test), do: ["lib", "support"]
  defp source_paths(_), do: ["lib"]
  defp dependencies do
    [{:packagemiddle, path: "../middle"}, {:packagecompile, path: "../compileonly", runtime: false}, {:packagetest, path: "../testonly", only: :test}, {:packagetarget, path: "../targetonly", targets: :embedded}]
  end
end
"#,
    );
    write(
        &project,
        "lib/root.ex",
        r#"defmodule PackageRoot do
  @environment Application.compile_env(:packageroot, :environment)
  def start(_, _) do
    unless Application.get_env(:packageleaf, :started), do: raise("dependency was not started before root")
    Application.put_env(:packageroot, :started, true)
    {:ok, self()}
  end
  def main(args) do
    IO.inspect({PackageMiddle.answer() + PackageCompile.answer(), @environment, Application.get_env(:packageroot, :target), Application.get_env(:packageroot, :runtime), Application.get_env(:packageroot, :started), Code.ensure_loaded?(PackageTest), Code.ensure_loaded?(PackageSupport), Code.ensure_loaded?(PackageTarget), args})
  end
end
"#,
    );
    write(
        &project,
        "support/support.ex",
        "defmodule PackageSupport do\n def value(), do: :included\nend\n",
    );
    write(
        &project,
        "config/config.exs",
        "import Config\nconfig :packageroot, environment: config_env(), target: config_target()\n",
    );
    write(
        &project,
        "config/runtime.exs",
        "import Config\nconfig :packageroot, runtime: System.get_env(\"TONIC_PACKAGE_RUNTIME\") || \"default\", environment: :runtime_shadow\n",
    );
    let dev = tonic(&project, "dev", "host", &["run", ".", "--", "one"]);
    success(&dev);
    assert_eq!(
        dev.stdout,
        b"{42, :dev, :host, \"duringrun\", true, false, false, false, [\"one\"]}\n"
    );
    let test = tonic(&project, "test", "embedded", &["run", ".", "--", "two"]);
    success(&test);
    assert_eq!(
        test.stdout,
        b"{42, :test, :embedded, \"duringrun\", true, true, true, true, [\"two\"]}\n"
    );
    let binary = root.join("program");
    success(&tonic(
        &project,
        "test",
        "embedded",
        &["build", ".", "-o", binary.to_str().unwrap()],
    ));
    let mut command = Command::new(&binary);
    command
        .current_dir(&root)
        .env("TONIC_PACKAGE_RUNTIME", "standalone")
        .arg("standalone");
    let result = execute(command, Duration::from_secs(60));
    success(&result);
    assert_eq!(
        result.stdout,
        b"{42, :test, :embedded, \"standalone\", true, true, true, true, [\"standalone\"]}\n"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_fetched_packages_report_the_dependency_and_fetch_command() {
    let root = scratch("missing");
    write(
        &root,
        "mix.exs",
        r#"defmodule MissingPackage.MixProject do
  use Mix.Project
  def project, do: [app: :missingpackage, version: "0.1.0", deps: [{:decimal, "~> 2.0"}]]
end
"#,
    );
    write(
        &root,
        "lib/main.ex",
        "defmodule MissingPackage do\n def main(_), do: :ok\nend\n",
    );
    let result = tonic(&root, "dev", "host", &["check", "."]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("decimal"), "{error}");
    assert!(error.contains("mix deps.get"), "{error}");
    assert!(!root.join("deps/decimal").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_build_requirements_fail_with_actionable_diagnostics() {
    let root = scratch("native");
    let project = root.join("project");
    write(
        &project,
        "mix.exs",
        r#"defmodule NativeConsumer.MixProject do
  use Mix.Project
  def project, do: [app: :nativeconsumer, version: "0.1.0", deps: [{:nativepackage, path: "../dependency"}]]
end
"#,
    );
    write(
        &project,
        "lib/main.ex",
        "defmodule NativeConsumer do\n def main(_), do: :ok\nend\n",
    );
    let dep = root.join("dependency");
    write(
        &dep,
        "mix.exs",
        r#"defmodule NativePackage.MixProject do
  use Mix.Project
  def project, do: [app: :nativepackage, version: "0.1.0", compilers: [:elixir_make] ++ Mix.compilers()]
end
"#,
    );
    write(
        &dep,
        "lib/native.ex",
        "defmodule NativePackage do\n def value(), do: :ok\nend\n",
    );
    let result = tonic(&project, "dev", "host", &["check", "."]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("nativepackage"), "{error}");
    assert!(
        error.contains("elixir_make") || error.contains("compiler"),
        "{error}"
    );
    write(
        &dep,
        "mix.exs",
        r#"defmodule NativePackage.MixProject do
  use Mix.Project
  def project, do: [app: :nativepackage, version: "0.1.0"]
end
"#,
    );
    write(&dep, "priv/native.so", "native artifact fixture");
    let result = tonic(&project, "dev", "host", &["check", "."]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("nativepackage"), "{error}");
    assert!(
        error.contains("NIF") || error.contains("native artifact"),
        "{error}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires a real Hex download; set TONIC_TEST_HEX=1 and pass --ignored"]
fn real_hex_decimal_package_compiles_and_executes() {
    assert_eq!(
        std::env::var("TONIC_TEST_HEX").as_deref(),
        Ok("1"),
        "set TONIC_TEST_HEX=1 to authorize the network package test"
    );
    let root = scratch("hex");
    let project = root.join("project");
    let mix_home = root.join("mixhome");
    let hex_home = root.join("hexhome");
    write(
        &project,
        "mix.exs",
        r#"defmodule HexConsumer.MixProject do
  use Mix.Project
  def project, do: [app: :hexconsumer, version: "0.1.0", deps: [{:decimal, "3.0.0"}], escript: [main_module: HexConsumer]]
end
"#,
    );
    write(
        &project,
        "lib/main.ex",
        r#"defmodule HexConsumer do
  def main(_), do: IO.puts(Decimal.to_string(Decimal.add(Decimal.new("1.25"), Decimal.new("2.50"))))
end
"#,
    );
    let mix = |args: &[&str]| {
        let mut command = Command::new("mix");
        command
            .current_dir(&project)
            .env("MIX_HOME", &mix_home)
            .env("HEX_HOME", &hex_home)
            .env("MIX_ENV", "dev")
            .env("MIX_TARGET", "host")
            .args(args);
        execute(command, Duration::from_secs(180))
    };
    success(&mix(&["local.hex", "--force"]));
    success(&mix(&["deps.get"]));
    assert!(project.join("deps/decimal/lib/decimal.ex").exists());
    assert!(fs::read_to_string(project.join("mix.lock"))
        .unwrap()
        .contains("decimal"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command
        .current_dir(&project)
        .env("MIX_HOME", &mix_home)
        .env("HEX_HOME", &hex_home)
        .env("HEX_OFFLINE", "1")
        .env("MIX_ENV", "dev")
        .env("MIX_TARGET", "host")
        .args(["run", "."]);
    let result = execute(command, Duration::from_secs(180));
    success(&result);
    assert_eq!(result.stdout, b"3.75\n");
    let lock_path = project.join("mix.lock");
    let original_lock = fs::read_to_string(&lock_path).unwrap();
    let checksum = original_lock
        .split('"')
        .find(|part| part.len() == 64 && part.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap();
    let replacement = format!(
        "{}{}",
        if checksum.starts_with('a') { "b" } else { "a" },
        &checksum[1..]
    );
    fs::write(
        &lock_path,
        original_lock.replacen(checksum, &replacement, 1),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command
        .current_dir(&project)
        .env("MIX_HOME", &mix_home)
        .env("HEX_HOME", &hex_home)
        .env("HEX_OFFLINE", "1")
        .env("MIX_ENV", "dev")
        .env("MIX_TARGET", "host")
        .args(["check", "."]);
    let stale = execute(command, Duration::from_secs(180));
    fs::write(&lock_path, &original_lock).unwrap();
    assert!(!stale.status.success());
    let error = String::from_utf8_lossy(&stale.stderr);
    assert!(
        error.contains("decimal") && error.contains("mix deps.get"),
        "{error}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn path_dependencies_must_satisfy_the_declared_version_requirement() {
    let root = scratch("version");
    let project = root.join("project");
    dependency(
        &root.join("dependency"),
        "packageversion",
        "PackageVersion",
        "defmodule PackageVersion do\n def value(), do: :ok\nend\n",
    );
    write(
        &project,
        "mix.exs",
        r#"defmodule VersionConsumer.MixProject do
  use Mix.Project
  def project, do: [app: :versionconsumer, version: "0.1.0", deps: [{:packageversion, "~> 2.0", path: "../dependency"}]]
end
"#,
    );
    write(
        &project,
        "lib/main.ex",
        "defmodule VersionConsumer do\n def main(_), do: :ok\nend\n",
    );
    let result = tonic(&project, "dev", "host", &["check", "."]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("packageversion"), "{error}");
    assert!(
        error.contains("version") || error.contains("requirement"),
        "{error}"
    );
    assert!(error.contains("2.0") && error.contains("0.1.0"), "{error}");
    fs::remove_dir_all(root).unwrap();
}
