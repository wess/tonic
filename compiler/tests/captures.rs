#[path = "support/process.rs"]
mod process;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "toniccaptures{}{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("source.exs"), source).unwrap();
        Self(root)
    }
    fn compare(&self, expected: &[u8]) {
        for (executable, native) in [("elixir", false), (env!("CARGO_BIN_EXE_tonic"), true)] {
            let mut command = Command::new(executable);
            if native {
                command.arg("run");
            }
            command.arg(self.0.join("source.exs"));
            let output = process::execute(command, Duration::from_secs(240));
            assert!(
                output.status.success(),
                "{executable}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected, "{executable}");
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn anonymous_closure_environment_cannot_impersonate_a_remote_capture() {
    let fixture = Fixture::new(
        r#"
defmodule CaptureCollision do
  def make(target), do: fn value -> {target, value} end
end
target = {:tonic_capture, String, :upcase, 1}
fun = CaptureCollision.make(target)
{:env, [^target]} = Function.info(fun, :env)
{^target, :value} = fun.(:value)
result = try do
  if Code.ensure_loaded?(Tonic.Eval), do: Tonic.Eval.escape(fun), else: Macro.escape(fun)
  :accepted
rescue
  ArgumentError -> :rejected
end
IO.inspect(result)
"#,
    );
    fixture.compare(b":rejected\n");
}

#[test]
fn interpreted_remote_captures_keep_identity_when_embedded_in_attributes() {
    let fixture = Fixture::new(
        r#"
defmodule CaptureTargets do
  def identity(value), do: value
  def handlers, do: %{identity: &__MODULE__.identity/1, upper: &String.upcase/1}
end
defmodule EmbeddedCaptures do
  @handlers CaptureTargets.handlers()
  def identity, do: @handlers.identity
  def upper, do: @handlers.upper
end
identity = EmbeddedCaptures.identity()
upper = EmbeddedCaptures.upper()
IO.inspect({identity.(:portable), upper.("héllo")})
IO.inspect(Enum.map([identity, upper], fn fun ->
  {Function.info(fun, :type), Function.info(fun, :module), Function.info(fun, :name), Function.info(fun, :arity)}
end))
"#,
    );
    fixture.compare(b"{:portable, \"H\xc3\x89LLO\"}\n[\n  {{:type, :external}, {:module, CaptureTargets}, {:name, :identity},\n   {:arity, 1}},\n  {{:type, :external}, {:module, String}, {:name, :upcase}, {:arity, 1}}\n]\n");
}
