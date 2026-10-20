#[path = "support/process.rs"]
mod process;

use std::process::{Command, Output};
use std::time::Duration;

fn execute(command: Command) -> Output {
    process::execute(command, Duration::from_secs(180))
}

fn run_case(name: &str, source: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("tonicdiagnostics{}{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.exs"));
    std::fs::write(&path, source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command.arg("run").arg(&path);
    let output = execute(command);
    let _ = std::fs::remove_dir_all(&dir);
    output
}

#[test]
fn executable_declarations_fail_explicitly() {
    for (name, source, diagnostic) in [
        (
            "local",
            "defmodule Broken do\n  missing()\nend",
            "undefined function missing/0",
        ),
        (
            "assignment",
            "defmodule Broken do\n  x = missing()\nend",
            "undefined function missing/0",
        ),
        (
            "for",
            "defmodule Broken do\n  for x <- unknown(), do: x\nend",
            "undefined function unknown/0",
        ),
    ] {
        let output = run_case(name, source);
        assert!(!output.status.success(), "{name} succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(diagnostic), "{name}: {stderr}");
        assert!(
            stderr.contains(&format!("{name}.exs:2")),
            "missing source location: {stderr}"
        );
    }
}

#[test]
fn module_bodies_run_at_compile_time_and_on_load_at_startup() {
    let output = run_case(
        "bodies",
        "defmodule Body do\n  IO.puts(:compile_time)\n  @on_load :init\n  def init() do\n    IO.puts(:loaded)\n    :ok\n  end\nend\nIO.puts(:runtime)\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("compile_time"));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "loaded\nruntime\n");
}

#[test]
fn attribute_values_capture_source_snapshots_and_effects_once() {
    let output = run_case(
        "attributes",
        r#"
defmodule AttributeHelper do
  def value(x) do
    IO.puts("attribute #{x}")
    x
  end
end
defmodule AttributeConsumer do
  @value AttributeHelper.value(10)
  def first(), do: @value
  @value AttributeHelper.value(20)
  def second(), do: @value
  @doc "metadata is source data"
  @spec values() :: {integer(), integer()}
  def values(), do: {first(), second()}
end
IO.inspect(AttributeConsumer.values())
"#,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "{10, 20}\n");
    assert_eq!(stderr.matches("attribute 10").count(), 1, "{stderr}");
    assert_eq!(stderr.matches("attribute 20").count(), 1, "{stderr}");
}

#[test]
fn unused_variables_warn_like_elixir() {
    let output = run_case(
        "unused",
        "defmodule U do\n  def a(x, y), do: x\n  def b({p, p}, _skip), do: binding()\n  def c(w) do\n    w = w * 2\n    :ok\n  end\nend\nIO.inspect U.a(1, 2)\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let unused: Vec<&str> = stderr.lines().filter(|l| l.contains("is unused")).collect();
    assert_eq!(unused.len(), 2, "{stderr}");
    assert!(unused[0].contains("unused.exs:2: warning: variable \"y\" is unused (if the variable"), "{stderr}");
    assert!(unused[1].contains("unused.exs:5: warning: variable \"w\" is unused (there is a variable with the same name"), "{stderr}");
}

#[test]
fn invalid_control_flow_is_rejected() {
    for (name, source, diagnostic) in [
        ("cond", "cond do end\n", "expected -> clauses for :do in \"cond\""),
        ("case", "case 1 do end\n", "expected -> clauses for :do in \"case\""),
        ("ifkeys", "if true, do: 1, foo: 2\n", "invalid or duplicate keys for if"),
    ] {
        let output = run_case(name, source);
        assert!(!output.status.success(), "{name} succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(diagnostic), "{name}: {stderr}");
    }
    assert!(run_case("receive", "IO.inspect(receive do\nafter 0 -> :t\nend)\n").status.success());
}
