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
        ("trybare", "try do\n  1\nend\n", "missing :catch/:rescue/:after option in \"try\""),
        ("kinds", "defmodule A do\n  def f(x), do: x\n  defp f(y), do: y\nend\n", "defp f/1 already defined as def in"),
        ("macrokind", "defmodule A do\n  defmacro m(x), do: x\n  def m(x), do: x\nend\n", "def m/1 already defined as defmacro in"),
        ("defaults", "defmodule A do\n  def f(x \\\\ 1), do: x\n  def f(x \\\\ 2), do: x\nend\n", "def f/1 defines defaults multiple times"),
        ("aliasas", "defmodule A do\n  alias Foo.Bar, as: :baz\nend\n", "invalid value for option :as, expected an alias, got: :baz"),
    ] {
        let output = run_case(name, source);
        assert!(!output.status.success(), "{name} succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(diagnostic), "{name}: {stderr}");
    }
    assert!(run_case("receive", "IO.inspect(receive do\nafter 0 -> :t\nend)\n").status.success());
}

#[test]
fn deep_recursion_grows_the_stack() {
    let source = r#"
defmodule Deep do
  def sum(0), do: 0
  def sum(n), do: n + sum(n - 1)
  def wait(0, parent), do: (send(parent, :deep); receive do: (:go -> 0))
  def wait(n, parent), do: 1 + wait(n - 1, parent)
end
n = String.to_integer(System.get_env("DEPTH"))
IO.inspect Deep.sum(n)
IO.inspect Task.await(Task.async(fn -> Enum.map(1..n, &(&1 * 2)) |> Enum.sum() end), :infinity)
IO.inspect Task.await(Task.async(fn -> Enum.reduce(1..3, 0, fn _, a -> a + Deep.sum(div(n, 2)) end) end), :infinity)
parent = self()
pid = spawn(fn -> send(parent, {:done, Deep.wait(n, parent)}) end)
receive do: (:deep -> send(pid, :go))
receive do: ({:done, n} -> IO.inspect n)
victim = spawn(fn -> Deep.wait(n, parent) end)
ref = Process.monitor(victim)
receive do: (:deep -> Process.exit(victim, :kill))
receive do: ({:DOWN, ^ref, _, _, reason} -> IO.inspect reason)
"#;
    let dir = std::env::temp_dir().join(format!("tonicdeep{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("deep.exs");
    std::fs::write(&path, source).unwrap();
    // 1 MB first stacks: both depths continue on new segments
    for (stress, depth) in [("0", 200_000u64), ("1", 20_000)] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
        command
            .arg("run")
            .arg(&path)
            .env("TONIC_PROC_STACK_MB", "1")
            .env("TONIC_MAIN_STACK_MB", "1")
            .env("TONIC_GC_STRESS", stress)
            .env("DEPTH", depth.to_string());
        let output = execute(command);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "stress={stress}: {}", String::from_utf8_lossy(&output.stderr));
        let (sum, half) = (depth * (depth + 1) / 2, depth / 2 * (depth / 2 + 1) / 2);
        let expected = format!("{}\n{}\n{}\n{}\n:killed\n", sum, 2 * sum, 3 * half, depth);
        assert_eq!(stdout, expected, "stress={stress}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ungrouped_clauses_and_duplicate_keys_warn_like_elixir() {
    let output = run_case(
        "grouping",
        "defmodule G do\n  def f(1), do: 1\n  def g, do: %{a: 1, a: 2}\n  def f(2), do: %{\"x\" => 1, \"x\" => 2, {:t} => 1, {:t} => 2}\nend\nIO.inspect G.f(1)\n",
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let warnings: Vec<&str> = stderr.lines().filter(|l| l.contains("warning:")).collect();
    assert_eq!(warnings.len(), 3, "{stderr}");
    assert!(warnings[0].contains("grouping.exs:3: warning: key :a will be overridden in map"), "{stderr}");
    assert!(warnings[1].contains("grouping.exs:4: warning: clauses with the same name and arity (number of arguments) should be grouped together, \"def f/1\" was previously defined ("), "{stderr}");
    assert!(warnings[2].contains("grouping.exs:4: warning: key \"x\" will be overridden in map"), "{stderr}");
    for (name, source) in [("kwmap", "x = 1\nIO.inspect %{b: 2, x => 1}\n"), ("kwlist", "IO.inspect [a: 1, :b]\n")] {
        let output = run_case(name, source);
        assert!(!output.status.success(), "{name} succeeded");
        assert!(String::from_utf8_lossy(&output.stderr).contains("Keyword lists must always come last"), "{name}");
    }
}

#[test]
fn main_process_exit_banners_match_elixir() {
    for (name, source, expected) in [
        ("linked", "spawn_link(fn -> exit(:boom) end)\nProcess.sleep(1000)\n", ") :boom"),
        ("shutdown", "p = self()\nspawn(fn -> Process.exit(p, {:shutdown, 3}) end)\nProcess.sleep(1000)\n", ") shutdown: 3"),
    ] {
        let output = run_case(name, source);
        assert_eq!(output.status.code(), Some(1), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.starts_with("** (EXIT from #PID<") && stderr.contains(expected), "{name}: {stderr}");
    }
    let output = run_case("clause", "defmodule X do\n  def f(:a), do: 1\nend\nX.f(:b)\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("** (FunctionClauseError) no function clause matching in X.f/1    \n    \n    The following arguments were given to X.f/1:\n    \n        # 1\n        :b\n"),
        "{stderr}"
    );
}
