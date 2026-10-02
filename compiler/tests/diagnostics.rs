use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn execute(mut command: Command) -> Output {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
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
            let _ = child.wait();
            panic!("compiler regression timed out: {command:?}");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

fn run_case(name: &str, source: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("tonicdiagnostics{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.exs"));
    std::fs::write(&path, source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command.arg("run").arg(&path);
    execute(command)
}

#[test]
fn executable_declarations_fail_explicitly() {
    for (name, source, diagnostic) in [
        (
            "local",
            "defmodule Broken do\n  missing()\nend",
            "unsupported module-level call missing/0",
        ),
        (
            "remote",
            "defmodule Broken do\n  IO.puts(:discarded)\nend",
            "unsupported module-level call IO.puts/1",
        ),
        (
            "assignment",
            "defmodule Broken do\n  x = missing()\nend",
            "unsupported module-level assignment",
        ),
        (
            "onload",
            "defmodule Broken do\n  @on_load :init\n  def init(), do: :ok\nend",
            "@on_load callbacks are not supported",
        ),
        (
            "for",
            "defmodule Broken do\n  for x <- unknown(), do: x\nend",
            "unsupported module-level for",
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
