#[path = "support/process.rs"]
mod process;

use std::fs;
use std::process::Command;
use std::time::Duration;

fn execute(command: Command) -> std::process::Output {
    process::execute(command, Duration::from_secs(300))
}

#[test]
fn guard_defaults_macro_clauses_and_argument_evaluation_match_beam() {
    let dir = std::env::temp_dir().join(format!("tonicguards{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("guards.exs");
    fs::write(&source, r#"
defmodule Defaults do
  defguard zero when true
  defguard duplicate(x, x) when is_integer(x)
  @default 42
  defguard snapshot(a \\ @default) when a == 42
  @default 43
  defguard integer(a \\ 42) when is_integer(a)
  defguard positions(a \\ :ok, b, c \\ 3) when is_atom(a) and is_integer(b) and is_integer(c)
  defguard tuple(a \\ {1, 2}) when is_tuple(a) and tuple_size(a) == 2
  defguard phase(a \\ (IO.puts(:guard_default_expanded); nil)) when is_nil(a)
  defguard twice(a) when is_integer(a) and a > 0
  defguard unused(a, _b) when is_integer(a)
  defguardp private(a \\ :ok) when is_atom(a)
  def direct(), do: {integer(), integer(:no), positions(2), positions(:yes, 2), positions(:yes, 2, 4), tuple(), private(), snapshot(), zero(), duplicate(1, 1)}
  def compiled_phase(), do: phase()
  def clause(a) when integer() and integer(a), do: :integer
  def clause(_), do: :other
  def next() do
    Process.put(:guard_calls, Process.get(:guard_calls, 0) + 1)
    1
  end
  def once(), do: twice(next())
  def skipped(), do: unused(1, Process.put(:guard_calls, 999))
end

defmodule MacroFirst do
  defmacro pick(:special), do: true
  defmacro pick({:+, _, _}), do: true
  defguard pick(a) when is_integer(a)
  def direct(), do: {pick(:special), pick(1 + 2), pick(1), pick(:other)}
  def clause(a) when pick(a), do: :integer
  def clause(_), do: :other
end

defmodule GuardFirst do
  defguard pick(a) when is_integer(a)
  defmacro pick(:special), do: true
  def direct(), do: {pick(:special), pick(1), pick(:other)}
end

defmodule PriorDefault do
  defmacro pick(), do: false
  defguard pick(a \\ 2) when is_integer(a)
  def direct(), do: pick()
end

defmodule LaterDefault do
  defguard pick(a) when is_integer(a)
  defguard pick(a \\ 2) when is_atom(a)
  def direct(), do: pick()
end

defmodule InterveningDefault do
  defguard pick(a) when is_integer(a)
  defmacro pick(), do: false
  defguard pick(a \\ 2) when is_atom(a)
  def direct(), do: pick()
end

defmodule MacroDefaults do
  defguard pick(a) when is_integer(a)
  defmacro pick(), do: false
  defmacro pick(a \\ 2), do: a
  def direct(), do: pick()
end

defmodule Dynamic do
  require Defaults
  require MacroFirst
  require GuardFirst
  require PriorDefault
  require InterveningDefault
  require MacroDefaults
  def run() do
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: Defaults.integer()), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: Defaults.positions(2)), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: Defaults.tuple()), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: Defaults.snapshot()), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: MacroFirst.pick(:special)), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: MacroFirst.pick(:other)), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: GuardFirst.pick(:special)), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: PriorDefault.pick()), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: InterveningDefault.pick()), __ENV__)) |> elem(0))
    IO.inspect(Code.eval_quoted(Macro.expand_once(quote(do: MacroDefaults.pick()), __ENV__)) |> elem(0))
    Process.put(:guard_calls, 0)
    quoted = quote do
      Defaults.twice(Defaults.next())
    end
    IO.inspect({Code.eval_quoted(Macro.expand_once(quoted, __ENV__)) |> elem(0), Process.get(:guard_calls)})
  end
end
IO.inspect(Defaults.direct())
IO.inspect({Defaults.compiled_phase(), Defaults.clause(1), Defaults.clause(:no)})
Process.put(:guard_calls, 0)
IO.inspect({Defaults.once(), Defaults.skipped(), Process.get(:guard_calls)})
IO.inspect({MacroFirst.direct(), MacroFirst.clause(1), MacroFirst.clause(:no), GuardFirst.direct(), PriorDefault.direct(), LaterDefault.direct(), InterveningDefault.direct(), MacroDefaults.direct()})
IO.inspect({macro_exported?(Defaults, :integer, 0), macro_exported?(Defaults, :positions, 1), macro_exported?(Defaults, :private, 0)})
Dynamic.run()
"#).unwrap();
    let mut beam = Command::new("elixir");
    beam.arg(&source);
    let beam = execute(beam);
    assert!(beam.status.success(), "{}", String::from_utf8_lossy(&beam.stderr));
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.arg("run").arg(&source);
    let native = execute(native);
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    let reference = String::from_utf8_lossy(&beam.stdout);
    assert_eq!(reference.lines().filter(|line| *line == "guard_default_expanded").count(), 1);
    assert_eq!(String::from_utf8_lossy(&native.stderr).lines().filter(|line| *line == "guard_default_expanded").count(), 1);
    let reference = reference.lines().filter(|line| *line != "guard_default_expanded").collect::<Vec<_>>().join("\n") + "\n";
    assert_eq!(String::from_utf8_lossy(&native.stdout), reference);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn guard_default_conflicts_are_rejected_like_beam() {
    let dir = std::env::temp_dir().join(format!("tonicguarderrors{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (i, code) in [
        "defguard g(a \\\\ 1) when is_integer(a)\ndefguard g(a \\\\ 2) when is_atom(a)",
        "defguard g(a \\\\ 1) when is_integer(a)\ndefmacro g(a \\\\ 2), do: a",
        "defguard g(a \\\\ 1) when is_integer(a)\ndefmacro g(), do: false",
        "defguard g(a \\\\ {1,2,3}) when is_tuple(a)\ndef t(), do: g()",
        "defguard g(x, x) when is_integer(x)\ndef t(), do: g(1, 2)",
        "def g(), do: 1\ndefguard g(a \\\\ 1) when is_integer(a)",
    ].iter().enumerate() {
        let source = dir.join(format!("invalid{i}.exs"));
        fs::write(&source, format!("defmodule Invalid do\n{code}\nend\n")).unwrap();
        let mut beam = Command::new("elixir");
        beam.arg(&source);
        let beam = execute(beam);
        assert!(!beam.status.success(), "reference accepted {code}");
        let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
        native.arg("check").arg(&source);
        let native = execute(native);
        assert!(!native.status.success(), "native accepted {code}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lexical_quote_bindings_and_eex_default_macro_wrappers_match_beam() {
    let dir = std::env::temp_dir().join(format!("toniceex{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("zero.eex"), "hello").unwrap();
    fs::write(dir.join("arg.eex"), "value=<%= value %>").unwrap();
    let source = dir.join("eex.exs");
    fs::write(&source, r#"
defmodule BindingQuote do
  def ast(x) do
    y = x + 1
    x = x * 2
    quote bind_quoted: binding() do
      {x, y}
    end
  end
  def context(x) do
    quote bind_quoted: Kernel.binding() do
      x
    end
  end
  def named() do
    var!(x, :quoted) = 11
    quote bind_quoted: Kernel.binding(:quoted) do
      x
    end
  end
  def empty() do
    quote bind_quoted: binding(Context) do
      :empty
    end
  end
end

defmodule Templates do
  require EEx
  EEx.function_from_file(:def, :noargs, "zero.eex")
  EEx.function_from_file(:def, :withargs, "arg.eex", [:value])
  EEx.function_from_file(:def, :withopts, "arg.eex", [:value], trim: true)
  EEx.function_from_string(:def, :from_string, "hello")
end
IO.inspect(Code.eval_quoted(BindingQuote.ast(5)) |> elem(0))
IO.inspect(Code.eval_quoted(BindingQuote.context(9)) |> elem(0))
IO.inspect(Code.eval_quoted(BindingQuote.named()) |> elem(0))
IO.inspect(Code.eval_quoted(BindingQuote.empty()) |> elem(0))
IO.inspect({Templates.noargs(), Templates.withargs(7), Templates.withopts(8), Templates.from_string()})
IO.inspect(Enum.map(4..6, &function_exported?(EEx, :"MACRO-function_from_file", &1)))
result = Code.eval_string("""
defmodule RuntimeTemplates do
  require EEx
  EEx.function_from_file(:def, :noargs, "zero.eex")
  EEx.function_from_file(:def, :withargs, "arg.eex", [:value])
  EEx.function_from_file(:def, :withopts, "arg.eex", [:value], trim: true)
  EEx.function_from_string(:def, :from_string, "hello")
end
{RuntimeTemplates.noargs(), RuntimeTemplates.withargs(9), RuntimeTemplates.withopts(10), RuntimeTemplates.from_string()}
""")
IO.inspect(elem(result, 0))
"#).unwrap();
    let mut beam = Command::new("elixir");
    beam.current_dir(&dir).arg(&source);
    let beam = execute(beam);
    assert!(beam.status.success(), "{}", String::from_utf8_lossy(&beam.stderr));
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.current_dir(&dir).arg("run").arg(&source);
    let native = execute(native);
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(native.stdout, beam.stdout);
    fs::remove_dir_all(dir).unwrap();
}
