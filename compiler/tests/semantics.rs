#[path = "support/process.rs"]
mod process;

use std::fs;
use std::process::Command;
use std::time::Duration;

fn run(command: Command) -> std::process::Output {
    process::execute(command, Duration::from_secs(240))
}

#[test]
fn invalid_programs_agree_with_elixir() {
    let dir = std::env::temp_dir().join(format!("tonicinvalid{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (name, source, diagnostic) in [
        ("spec", "defmodule M do\n@spec absent(integer()) :: integer()\nend", "spec for undefined function absent/1"),
        ("bindquoted", "bindings = [x: 1]\nIO.inspect(quote bind_quoted: bindings do\n x\nend)", "invalid :bind_quoted"),
        ("tuple", "IO.inspect({a: 1})", "keyword list inside tuple"),
        ("nestedalias", "alias Foo.Bar, as: Foo.Baz", "expected a simple alias"),
        ("multialias", "alias Foo.{Bar, Baz}, as: Q", ":as option is not supported"),
        ("modulealias", "defmodule M do\nalias __MODULE__, as: Foo.Bar\nend", "expected a simple alias"),
        ("guardfunction", "defmodule M do\ndefguard x(a) when is_atom(a)\ndef x(a), do: a\nend", "already defined as defmacro"),
        ("functionguard", "defmodule M do\ndef x(a), do: a\ndefguard x(a) when is_atom(a)\nend", "already defined as def"),
        ("guardvisibility", "defmodule M do\ndefguard x(a) when is_atom(a)\ndefguardp x(a) when is_integer(a)\nend", "already defined as defmacro"),
        ("privateguardfunction", "defmodule M do\ndefguardp x(a) when is_atom(a)\ndefp x(a), do: a\nend", "already defined as defmacrop"),
        ("invalidguardbody", "defmodule M do\ndefguard x(a) when Enum.empty?(a)\nend", "inside guard"),
        ("guardmatch", "defmodule M do\ndefguard x(a) when (a = 1)\nend", "inside guard"),
        ("privateimport", "defmodule M do\ndefguardp x(a) when is_atom(a)\ndef use_it(a), do: x(a)\nend\ndefmodule N do\nimport M\ndef use_it(a) when x(a), do: a\nend", "undefined function x/1"),
        ("privateremote", "defmodule M do\ndefguardp x(a) when is_atom(a)\ndef use_it(a), do: x(a)\nend\ndefmodule N do\nrequire M\ndef use_it(a) when M.x(a), do: a\nend", "undefined or private"),
        ("repeatedoverride", "defmodule M do\ndef x, do: 1\ndefoverridable x: 0\ndefoverridable x: 0\nend", "was not defined"),
    ] {
        let file = dir.join(format!("{name}.exs"));
        fs::write(&file, source).unwrap();
        let mut beam = Command::new("elixir");
        beam.arg(&file);
        let beam = run(beam);
        assert!(!beam.status.success(), "reference accepted {name}");
        let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
        native.arg("check").arg(&file);
        let native = run(native);
        let stderr = String::from_utf8_lossy(&native.stderr);
        assert!(!native.status.success(), "native accepted {name}");
        assert!(stderr.contains(diagnostic), "{name}: {stderr}");
        assert!(stderr.contains(file.to_str().unwrap()), "missing source location: {stderr}");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn alias_guard_and_override_results_agree_with_elixir() {
    let dir = std::env::temp_dir().join(format!("tonicsemantics{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("valid.exs");
    fs::write(
        &file,
        r#"
defmodule Source do
  @spec answer() :: integer()
  def answer(), do: 42
end
alias Source, as: Elixir.Short

defmodule Guards do
  defguard accepted(a) when is_atom(a)
  defguard accepted(a) when is_integer(a)
  defguardp local(a) when is_integer(a)
  def test(a), do: {accepted(a), local(a)}
end

defmodule Override do
  def value(), do: 1
  defoverridable value: 0
  def value(), do: super() + 1
  defoverridable value: 0
  def value(), do: super() + 1
end
IO.inspect({Short.answer(), Guards.test(:atom), Guards.test(1), Override.value(), {1, a: 2}})
value = 123
quoted = quote bind_quoted: [value: value] do
  value + 1
end
{:__block__, [], [{:=, [], [{:value, meta, Elixir}, 123]}, _]} = quoted
IO.inspect(meta)

defmodule Binding do
  defmacro unused do
    quote do
      var!(unused) = 1
    end
  end
end
defmodule Consumer do
  require Binding
  def run(), do: Binding.unused()
end
Consumer.run()
IO.inspect({macro_exported?(Guards, :accepted, 1), macro_exported?(Guards, :local, 1), Guards.__info__(:macros)})

defmodule GuardA do
  defguard ok(a) when is_atom(a)
end
defmodule GuardB do
  defguard ok(a) when is_integer(a)
end
defmodule GuardScope do
  alias GuardA, as: Current
  require Current
  defguard ok(a) when Current.ok(a)
  alias GuardB, as: Current
  def test(a), do: ok(a)
end
IO.inspect({GuardScope.test(:atom), GuardScope.test(1)})
"#,
    )
    .unwrap();
    let mut beam = Command::new("elixir");
    beam.arg(&file);
    let beam = run(beam);
    assert!(
        beam.status.success(),
        "{}",
        String::from_utf8_lossy(&beam.stderr)
    );
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.arg("run").arg(&file);
    let native = run(native);
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&native.stdout), String::from_utf8_lossy(&beam.stdout));
    assert!(String::from_utf8_lossy(&beam.stderr).contains("variable \"unused\" is unused"));
    assert!(String::from_utf8_lossy(&native.stderr).contains("variable \"unused\" is unused"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn attribute_snapshot_index_preserves_nil_deletes_accumulation_and_prior_values() {
    let dir = std::env::temp_dir().join(format!("tonicsnapshots{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("snapshots.exs");
    fs::write(&source, r#"
m = SnapshotSubject
other = SnapshotUnrelated
Tonic.Eval.Modules.create(m, "snapshot.ex")
Tonic.Eval.Modules.create(other, "other.ex")
Tonic.Eval.Modules.put_attribute(other, :unrelated, :kept)
Tonic.Eval.Modules.register_attribute(m, :events, accumulate: true)
Tonic.Eval.Modules.snapshot_attributes(m, 1)
%{events: []} = Tonic.Eval.Modules.definition_attributes(m, 1)
Tonic.Eval.Modules.put_attribute(m, :present_nil, nil)
Tonic.Eval.Modules.put_attribute(m, :payload, %{value: [1, 2]})
Tonic.Eval.Modules.snapshot_attributes(m, 2)
%{events: [], present_nil: nil, payload: %{value: [1, 2]}} = Tonic.Eval.Modules.definition_attributes(m, 2)
Tonic.Eval.Modules.put_attribute(m, :events, :first)
Tonic.Eval.Modules.put_attribute(m, :payload, %{value: [3]})
Tonic.Eval.Modules.delete_attribute(m, :present_nil)
[:first] = Tonic.Eval.Modules.delete_attribute(m, :events)
Tonic.Eval.Modules.snapshot_attributes(m, 3)
%{payload: %{value: [3]}} = Tonic.Eval.Modules.definition_attributes(m, 3)
false = Map.has_key?(Tonic.Eval.Modules.definition_attributes(m, 3), :present_nil)
false = Map.has_key?(Tonic.Eval.Modules.definition_attributes(m, 3), :events)
Tonic.Eval.Modules.put_attribute(m, :present_nil, nil)
Tonic.Eval.Modules.put_attribute(m, :events, :last)
Tonic.Eval.Modules.snapshot_attributes(m, 4)
%{events: [:last], present_nil: nil, payload: %{value: [3]}} = Tonic.Eval.Modules.definition_attributes(m, 4)
%{events: []} = Tonic.Eval.Modules.definition_attributes(m, 1)
%{events: [], present_nil: nil, payload: %{value: [1, 2]}} = Tonic.Eval.Modules.definition_attributes(m, 2)
Tonic.Eval.Modules.create(m, "replacement.ex")
Tonic.Eval.Modules.snapshot_attributes(m, 5)
0 = map_size(Tonic.Eval.Modules.definition_attributes(m, 5))
:kept = Tonic.Eval.Modules.get_attribute(other, :unrelated)
IO.puts("snapshots preserved")
"#).unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.arg("run").arg(&source);
    let native = run(native);
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(native.stdout, b"snapshots preserved\n");
    fs::remove_dir_all(dir).unwrap();
}
