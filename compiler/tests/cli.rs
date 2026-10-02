use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "tonictest{}{}{}",
        std::process::id(),
        name,
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn output(mut command: Command) -> Output {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        let mut stream = stdout;
        stream.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        let mut stream = stderr;
        stream.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(180);
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
            child.wait().unwrap();
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

fn tonic(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command.args(args);
    output(command)
}

fn success(result: &Output) {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn modules_and_scripts_build_and_run_with_arguments() {
    let dir = scratch("scripts");
    let module = dir.join("library.ex");
    let first = dir.join("first.exs");
    let second = dir.join("second.exs");
    let binary = dir.join("program");
    let llvm = dir.join("program.ll");
    fs::write(&module, "defmodule Library do\n  def answer, do: 42\nend\n").unwrap();
    fs::write(&first, "IO.puts(Library.answer())\n").unwrap();
    fs::write(&second, "IO.inspect(System.argv())\n").unwrap();
    let result = tonic(&[
        "run",
        text(&module),
        text(&first),
        text(&second),
        "--",
        "one",
        "two",
    ]);
    success(&result);
    assert_eq!(result.stdout, b"42\n[\"one\", \"two\"]\n");
    success(&tonic(&[
        "build",
        text(&module),
        text(&first),
        text(&second),
        "-o",
        text(&binary),
    ]));
    let mut command = Command::new(&binary);
    command.arg("three");
    let result = output(command);
    success(&result);
    assert_eq!(result.stdout, b"42\n[\"three\"]\n");
    success(&tonic(&["check", text(&module)]));
    success(&tonic(&[
        "emit-llvm",
        text(&module),
        text(&first),
        text(&second),
        "-o",
        text(&llvm),
    ]));
    let ir = fs::read_to_string(&llvm).unwrap();
    assert!(ir.contains("define i64 @tonic_main_entry"));
    assert!(ir.contains("tailcc"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn script_exit_and_compile_errors_reach_the_caller() {
    let dir = scratch("errors");
    let script = dir.join("script.exs");
    fs::write(&script, "System.halt(7)\n").unwrap();
    assert_eq!(tonic(&["run", text(&script)]).status.code(), Some(7));
    fs::write(&script, "defmodule Broken do\n  def x(\nend\n").unwrap();
    let result = tonic(&["check", text(&script)]);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("CompileError"));
    assert!(error.contains(text(&script)));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn checking_a_module_does_not_invoke_the_llvm_toolchain() {
    let dir = scratch("check");
    let source = dir.join("library.ex");
    fs::write(&source, "defmodule Checked do\n def value, do: 42\nend\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command
        .env("TONIC_CC", dir.join("missingcompiler"))
        .args(["check", text(&source)]);
    success(&output(command));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn after_compilation_receives_the_emitted_llvm_artifact() {
    let dir = scratch("artifact");
    let source = dir.join("artifact.ex");
    let llvm = dir.join("program.ll");
    fs::write(
        &source,
        r#"defmodule ArtifactHook do
  def __after_compile__(env, artifact) do
    unless is_binary(artifact) and String.contains?(artifact, "@tonic_main_entry"), do: raise "invalid native artifact"
    File.write!(env.file <> ".ll", artifact)
  end
end
defmodule Compiled do
  @after_compile ArtifactHook
  def value, do: 42
end
"#,
    )
    .unwrap();
    success(&tonic(&["check", text(&source)]));
    assert!(!source.with_extension("ex.ll").exists());
    success(&tonic(&["emit-llvm", text(&source), "-o", text(&llvm)]));
    assert_eq!(
        fs::read(source.with_extension("ex.ll")).unwrap(),
        fs::read(&llvm).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mix_projects_load_environment_config_and_path_dependencies() {
    let dir = scratch("mix");
    let project = dir.join("project");
    let dependency = dir.join("dependency");
    fs::create_dir_all(project.join("lib")).unwrap();
    fs::create_dir_all(project.join("config")).unwrap();
    fs::create_dir_all(dependency.join("lib")).unwrap();
    fs::write(dependency.join("mix.exs"), "defmodule Dependency.MixProject do\n use Mix.Project\n def project, do: [app: :dependency, version: \"0.1.0\"]\nend\n").unwrap();
    fs::write(
        dependency.join("lib/dependency.ex"),
        "defmodule Dependency do\n def answer, do: 42\nend\n",
    )
    .unwrap();
    fs::write(
        project.join("mix.exs"),
        r#"defmodule Audit.MixProject do
  use Mix.Project
  def project, do: [app: :audit, version: "0.1.0", deps: deps(), escript: [main_module: Audit]]
  def application, do: [extra_applications: [:logger]]
  defp deps, do: [{:dependency, path: "../dependency"}]
end
"#,
    )
    .unwrap();
    fs::write(project.join("lib/audit.ex"), r#"defmodule Audit do
  def main(args), do: IO.inspect({Dependency.answer(), Application.get_env(:audit, :value), Application.get_env(:audit, :runtime), args})
end
"#).unwrap();
    fs::write(
        project.join("config/config.exs"),
        "import Config\nimport_config \"#{config_env()}.exs\"\n",
    )
    .unwrap();
    fs::write(
        project.join("config/test.exs"),
        "import Config\nconfig :audit, value: :test\n",
    )
    .unwrap();
    fs::write(
        project.join("config/runtime.exs"),
        "import Config\nconfig :audit, runtime: :loaded\n",
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command
        .env("MIX_ENV", "test")
        .args(["run", text(&project), "--", "hello"]);
    let result = output(command);
    success(&result);
    assert_eq!(result.stdout, b"{42, :test, :loaded, [\"hello\"]}\n");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn moving_gc_preserves_closures_maps_and_process_messages() {
    let dir = scratch("gc");
    let script = dir.join("gc.exs");
    fs::write(&script, r#"parent = self()
f = fn x -> %{value: x, text: "kept"} end
pid = spawn(fn -> send(parent, {:result, Enum.map(1..30, f)}) end)
result = receive do
  {:result, values} -> values
after
  2000 -> raise "message timeout"
end
IO.inspect({length(result), Enum.sum(Enum.map(result, & &1.value)), Enum.all?(result, &(&1.text == "kept")), is_pid(pid)})
"#).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command
        .env("TONIC_GC_STRESS", "1")
        .args(["run", text(&script)]);
    let result = output(command);
    success(&result);
    assert_eq!(result.stdout, b"{30, 465, true, true}\n");
    fs::remove_dir_all(dir).unwrap();
}
