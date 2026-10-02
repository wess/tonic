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

#[test]
fn dependency_order_and_compile_time_definition_generation() {
    let dir = std::env::temp_dir().join(format!("toniccompiletime{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let definitions = dir.join("a.ex");
    let macros = dir.join("z.ex");
    let script = dir.join("entry.exs");
    let before = dir.join("before.exs");
    let middle = dir.join("middle.ex");
    std::fs::write(&before, "IO.puts(\"before\")\n").unwrap();
    std::fs::write(&middle, "IO.puts(\"middle\")\n").unwrap();
    std::fs::write(
        &macros,
        r#"
defmodule Helpers do
  defmacro inherited do
    quote do
      def inherited(), do: 7
    end
  end
end
"#,
    )
    .unwrap();
    std::fs::write(
        &definitions,
        r#"
alias Helpers, as: H
defmodule Arithmetic do
  import H
  inherited()
  defstruct []
  @limit :erlang.binary_to_integer("1" <> String.duplicate("0", 2000))
  def limit_length(), do: String.length(Integer.to_string(@limit))

  defmacrop echo(value) do
    quote bind_quoted: binding() do
      value
    end
  end
  def bound(value), do: echo(value)

  final = Enum.reduce(0..50, 1, fn exponent, accumulator ->
    def power(unquote(exponent)), do: unquote(accumulator)
    accumulator * 10
  end)
  def final(), do: unquote(final)

  if Version.compare(System.version(), "1.0.0") == :gt do
    def current(), do: true
  else
    def current(), do: false
  end
end

if Code.ensure_loaded?(String.Chars) do
  defimpl String.Chars, for: Arithmetic do
    def to_string(_), do: "generated protocol"
  end
end
"#,
    )
    .unwrap();
    std::fs::write(&script, r#"
IO.inspect({Arithmetic.inherited(), Arithmetic.bound(4), Arithmetic.current(), Arithmetic.limit_length()})
IO.puts(Arithmetic.power(50))
IO.puts(Arithmetic.final())
IO.puts(to_string(%Arithmetic{}))
"#).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_tonic"));
    command
        .arg("run")
        .arg(&before)
        .arg(&definitions)
        .arg(&macros)
        .arg(&middle)
        .arg(&script);
    let result = execute(command);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        format!(
            "before\nmiddle\n{{7, 4, true, 2001}}\n1{}\n1{}\ngenerated protocol\n",
            "0".repeat(50),
            "0".repeat(51)
        )
    );
    std::fs::remove_dir_all(dir).unwrap();
}
