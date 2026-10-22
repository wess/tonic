#[path = "support/process.rs"]
mod process;

use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn compiled_calls_reach_public_functions_created_by_evaluation() {
    let dir = std::env::temp_dir().join(format!("tonicevaluation{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.exs");
    fs::write(&source, r#"
defmodule EvaluationCalls do
  def attempt(fun) do
    try do
      fun.()
    rescue
      error -> {:error, error.__struct__}
    catch
      kind, reason -> {kind, reason}
    end
  end
  def direct(value), do: Runtime.Evaluated.echo(value)
  def native(module, 0), do: module.done()
  def native(module, count), do: module.native(module, count - 1)
  def done(), do: :native_tail
end
IO.inspect(Code.loaded?(Runtime.Evaluated), label: "before")
Code.eval_string("""
defmodule Runtime.Evaluated do
  def answer, do: :runtime
  def echo(value), do: {value, hidden()}
  defp hidden, do: :inside
  def guarded(value) when is_integer(value), do: value + 1
  def crash, do: raise(ArgumentError, "evaluated failure")
  def tossed(value), do: throw(value)
  def exited(value), do: exit(value)
  defmacro visible(), do: quote(do: :visible)
  defmacrop secret(), do: quote(do: :secret)
end
""")
module = Module.concat(["Runtime", "Evaluated"])
name = String.to_atom(Enum.join(["ec", "ho"]))
IO.inspect({Runtime.Evaluated.answer(), module.answer, apply(module, :answer, [])}, label: "zero")
IO.inspect({EvaluationCalls.direct("direct"), module.echo("module"), apply(module, name, ["apply"])}, label: "calls")
IO.inspect(({&Runtime.Evaluated.echo/1, Function.capture(module, name, 1)}) |> Tuple.to_list() |> Enum.map(& &1.("capture")), label: "captures")
IO.inspect({function_exported?(module, :answer, 0), function_exported?(module, :hidden, 0), function_exported?(module, :missing, 0), Code.loaded?(module)}, label: "reflection")
IO.inspect({macro_exported?(module, :visible, 0), macro_exported?(module, :secret, 0), macro_exported?(module, :absent, 0), function_exported?(module, :"MACRO-visible", 1)}, label: "macros")
IO.inspect({apply(module, :"MACRO-visible", [__ENV__]) |> Code.eval_quoted() |> elem(0), Function.capture(module, :"MACRO-visible", 1).(__ENV__) |> Code.eval_quoted() |> elem(0)}, label: "macro invocation")
IO.inspect(EvaluationCalls.attempt(fn -> apply(module, :"MACRO-secret", [__ENV__]) end), label: "private macro")
input = for index <- 1..100, do: %{index: index, payload: :binary.copy("value", 20)}
IO.inspect({module.echo(input) == {input, :inside}, Function.capture(module, name, 1).(input) == {input, :inside}}, label: "heap args")
IO.inspect(EvaluationCalls.attempt(fn -> apply(module, :hidden, []) end), label: "private")
IO.inspect(EvaluationCalls.attempt(fn -> Function.capture(module, :hidden, 0).() end), label: "private capture")
IO.inspect(EvaluationCalls.attempt(fn -> module.missing() end), label: "missing")
IO.inspect(EvaluationCalls.attempt(fn -> Runtime.Evaluated.guarded(:not_integer) end), label: "guard")
IO.inspect(EvaluationCalls.attempt(fn -> Runtime.Evaluated.crash() end), label: "raise")
IO.inspect(EvaluationCalls.attempt(fn -> module.tossed({:payload, [1, 2]}) end), label: "throw")
IO.inspect(EvaluationCalls.attempt(fn -> apply(module, :exited, [:stopped]) end), label: "exit")
IO.inspect(EvaluationCalls.attempt(fn -> apply(123, :answer, []) end), label: "bad module")
IO.inspect(EvaluationCalls.attempt(fn -> apply(module, "answer", []) end), label: "bad function")
IO.inspect(EvaluationCalls.attempt(fn -> apply(module, :echo, [1 | :bad_tail]) end), label: "bad arguments")
IO.inspect({function_exported?(module, :answer, -1), function_exported?(module, :answer, 256)}, label: "reflection arities")
IO.inspect({function_exported?(module, :answer, 4294967296), function_exported?(module, :echo, -4294967295)}, label: "reflection wrapping")
IO.inspect({EvaluationCalls.attempt(fn -> function_exported?(module, :answer, 576460752303423488) end), EvaluationCalls.attempt(fn -> function_exported?(module, :answer, -576460752303423489) end)}, label: "reflection integer bounds")
IO.inspect(EvaluationCalls.attempt(fn -> function_exported?(module, :answer, :bad_arity) end), label: "bad reflection")
IO.inspect({EvaluationCalls.attempt(fn -> macro_exported?(module, :visible, -1) end), EvaluationCalls.attempt(fn -> macro_exported?(module, :visible, 256) end)}, label: "macro arities")
IO.inspect(EvaluationCalls.attempt(fn -> apply(module, :missing, List.duplicate(nil, 256)) end), label: "large invocation")
IO.inspect({String.upcase("native"), EvaluationCalls.native(EvaluationCalls, 20000)}, label: "native")
"#).unwrap();
    let mut reference = Command::new("elixir");
    reference.arg(&source);
    let reference = process::execute(reference, Duration::from_secs(60));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let binary = dir.join("evaluation");
    let mut build = Command::new(env!("CARGO_BIN_EXE_tonic"));
    build.arg("build").arg(&source).arg("-o").arg(&binary);
    let build = process::execute(build, Duration::from_secs(300));
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    for stress in ["0", "1"] {
        let mut native = Command::new(&binary);
        native.env("TONIC_GC_STRESS", stress);
        let native = process::execute(native, Duration::from_secs(60));
        assert!(
            native.status.success(),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&reference.stdout),
            "GC stress={stress}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
