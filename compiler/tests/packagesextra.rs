#[path = "support/process.rs"]
mod process;

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn copy(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

#[test]
#[ignore = "requires pinned Hex downloads; set TONIC_TEST_HEX=1 and pass --ignored"]
fn pinned_hex_packages_match_elixir() {
    assert_eq!(std::env::var("TONIC_TEST_HEX").as_deref(), Ok("1"));
    let root = std::env::temp_dir().join(format!("tonicreleasepackages{}", std::process::id()));
    let project = root.join("project");
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/packages"),
        &project,
    );
    let command = |executable: &str, args: &[&str], offline: bool| {
        let mut cmd = Command::new(executable);
        cmd.current_dir(&project)
            .env("MIX_HOME", root.join("mixhome"))
            .env("HEX_HOME", root.join("hexhome"))
            .env("MIX_ENV", "dev")
            .env("MIX_TARGET", "host")
            .args(args);
        if offline {
            cmd.env("HEX_OFFLINE", "1");
        }
        let result = process::execute(cmd, Duration::from_secs(240));
        assert!(
            result.status.success(),
            "project: {}\nstdout: {}\nstderr: {}",
            project.display(),
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        result
    };
    command("mix", &["local.hex", "--force"], false);
    command("mix", &["deps.get"], false);
    command("mix", &["compile"], true);
    let reference = command(
        "mix",
        &["run", "--no-compile", "-e", "PackageCoverage.main([])"],
        true,
    );
    assert_eq!(
        reference.stdout,
        b"{\"ok\", [1, true, nil]}\n{3, :safe, true}\n{3, true, \"merged\", \"left\"}\n"
    );
    let native = command(env!("CARGO_BIN_EXE_tonic"), &["run", "."], true);
    assert_eq!(native.stdout, reference.stdout);
    let original_lock =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/packages/mix.lock"))
            .unwrap();
    assert_eq!(fs::read(project.join("mix.lock")).unwrap(), original_lock);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn quoted_map_splices_preserve_entries_and_generated_patterns() {
    let root = std::env::temp_dir().join(format!("tonicmapsplice{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("splice.exs");
    fs::write(
        &source,
        r#"
defmodule MapSplice do
  defmacro matching() do
    fields = [{:a, {:value, [], __MODULE__}}]
    quote do
      def match(%{unquote_splicing(fields)}), do: :matched
    end
  end
end
defmodule MapConsumer do
  require MapSplice
  MapSplice.matching()
end
entries = [a: 1, b: 2]
quoted = quote do: %{unquote_splicing(entries), c: 3}
{value, _} = Code.eval_quoted(quoted)
IO.inspect(Enum.sort(value))
IO.inspect(MapConsumer.match(%{a: 8}))
"#,
    )
    .unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", source.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(180));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let mut reference = Command::new("elixir");
    reference.arg(&source);
    let reference = process::execute(reference, Duration::from_secs(60));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert_eq!(reference.stdout, b"[a: 1, b: 2, c: 3]\n:matched\n");
    assert_eq!(native.stdout, reference.stdout);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn record_macros_preserve_defaults_patterns_indexes_and_updates() {
    let root = std::env::temp_dir().join(format!("tonicrecords{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("record.exs");
    fs::write(&source, r#"
defmodule Records do
  import Record
  defrecordp :person, :human, name: "bob", age: 0
  defrecord :public, value: nil
  @callbacks [fetch: &__MODULE__.fetch/0]
  def fetch, do: :fetched
  def main do
    IO.inspect(@callbacks[:fetch].())
    value = person(age: 4)
    person(name: name) = value
    IO.inspect({person(), value, name, person(:age), person(value, :age), person(value, name: "alice")})
  end
end
Records.main()
defmodule PublicConsumer do
  require Records
  def main, do: IO.inspect(Records.public(value: 8))
end
PublicConsumer.main()
"#).unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", source.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let mut reference = Command::new("elixir");
    reference.arg(&source);
    let reference = process::execute(reference, Duration::from_secs(60));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert_eq!(native.stdout, reference.stdout);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn compile_time_bindings_are_captured_once_at_the_original_line() {
    let root = std::env::temp_dir().join(format!("tonicbindings{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("bindings.exs");
    fs::write(
        &source,
        r#"
defmodule Bindings do
  initial = try do
    IO.puts(:first_binding)
    :erlang.float_to_binary(1.0, [:short])
  catch
    _, _ -> false
  else
    _ -> true
  end
  @initial initial
  initial = try do
    IO.puts(:second_binding)
    false
  end
  @second initial
  def values, do: {@initial, @second}
end
IO.inspect(Bindings.values())
"#,
    )
    .unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", source.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, b"{true, false}\n");
    let stderr = String::from_utf8_lossy(&native.stderr);
    assert_eq!(stderr.matches("first_binding").count(), 1, "{stderr}");
    assert_eq!(stderr.matches("second_binding").count(), 1, "{stderr}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn array_defaults_resize_and_ordered_conversion_match_otp() {
    let root = std::env::temp_dir().join(format!("tonicarrays{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("array.exs");
    fs::write(
        &source,
        r#"
empty = :array.from_orddict([], :missing)
IO.inspect({:array.size(empty), :array.to_orddict(empty), :array.get(20, empty)})
array = :array.from_orddict([{2, :a}, {4, :missing}, {6, :b}], :missing)
IO.inspect({:array.to_orddict(array), :array.sparse_to_orddict(array), :array.sparse_size(array)})
IO.inspect(:array.to_orddict(:array.resize(3, array)))
IO.inspect(:array.to_orddict(:array.resize(9, array)))
IO.inspect(:array.to_orddict(:array.resize(:array.reset(6, array))))
fixed = :array.new([{:size, 2}, {:fixed, false}, {:default, :hole}])
IO.inspect({:array.is_fix(fixed), :array.to_list(:array.set(3, :x, fixed))})
IO.inspect({:array.is_fix(:array.new(2)), :array.is_fix(:array.new([{:fixed, false}, {:size, 2}]))})
IO.inspect(:array.sparse_to_orddict(:array.from_orddict([{0, 1}, {1, 1.0}], 1)))
IO.inspect(:array.to_list(:array.resize(8, :array.resize(3, array))))
for entries <- [[{-1, :bad}], [{2, :a}, {1, :b}], [{1, :a}, {1, :b}]] do
  result = try do
    :array.from_orddict(entries)
    :unexpected
  rescue
    ArgumentError -> :badarg
  end
  IO.inspect(result)
end
"#,
    )
    .unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", source.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let mut reference = Command::new("elixir");
    reference.arg(&source);
    let reference = process::execute(reference, Duration::from_secs(60));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert_eq!(native.stdout, reference.stdout);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn module_each_captures_generated_clauses_without_repeating_effects() {
    let root = std::env::temp_dir().join(format!("toniceach{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("each.exs");
    fs::write(&source, r#"
defmodule Generated do
  @offset 10
  entries = try do
    IO.puts(:iterable_once)
    [{1, :one}, {2, :two}]
  end
  def value(:existing), do: :before
  Enum.each(entries, fn {number, name} ->
    IO.puts("generated #{number}")
    result = @offset + number
    def value(unquote(name)), do: unquote(result)
  end)
  def value(:after), do: :after
end
IO.inspect({Generated.value(:existing), Generated.value(:one), Generated.value(:two), Generated.value(:after)})
"#).unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", source.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(native.stdout, b"{:before, 11, 12, :after}\n");
    let stderr = String::from_utf8_lossy(&native.stderr);
    assert_eq!(stderr.matches("iterable_once").count(), 1, "{stderr}");
    assert_eq!(stderr.matches("generated 1").count(), 1, "{stderr}");
    assert_eq!(stderr.matches("generated 2").count(), 1, "{stderr}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn literal_sigil_expansion_and_quoted_bit_types_match_elixir() {
    let root = std::env::temp_dir().join(format!("tonicsigiltypes{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("sigil.exs");
    fs::write(
        &source,
        r#"
defmodule LiteralMacros do
  defmacro characters, do: Macro.expand(quote(do: ~c"a\nb"), __CALLER__)
  defmacro words, do: Macro.expand(quote(do: ~w"one two"a), __CALLER__)
  defmacro range(value) do
    values = Enum.to_list(value)
    quote(do: unquote(values))
  end
  defmacro decode do
    quote do
      fn <<character::utf8, rest::binary>> -> {character, rest} end
    end
  end
end
defmodule LiteralConsumer do
  require LiteralMacros
  def main do
    IO.inspect(LiteralMacros.characters())
    IO.inspect(LiteralMacros.words())
    IO.inspect(LiteralMacros.range(unquote(0..3)))
    decode = LiteralMacros.decode()
    IO.inspect(decode.("érest"))
  end
end
LiteralConsumer.main()
"#,
    )
    .unwrap();
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", source.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let mut reference = Command::new("elixir");
    reference.arg(&source);
    let reference = process::execute(reference, Duration::from_secs(60));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert_eq!(native.stdout, reference.stdout);
    fs::remove_dir_all(root).unwrap();
}
