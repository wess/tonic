#[path = "support/process.rs"]
mod process;

use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn persistent_array_operations_and_invalid_arguments_match_otp() {
    let root = std::env::temp_dir().join(format!("tonicarraycoverage{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("arrays.exs");
    fs::write(&source, r#"
defmodule ArrayCoverage do
  def observe(name, fun) do
    value = try do
      {:ok, fun.()}
    catch
      kind, reason -> {kind, reason}
    end
    IO.inspect({name, value}, limit: :infinity, width: :infinity)
  end

  def snapshot(array) do
    {:array.size(array), :array.default(array), :array.is_fix(array),
     :array.to_list(array), :array.to_orddict(array),
     :array.sparse_to_list(array), :array.sparse_to_orddict(array), :array.sparse_size(array)}
  end
end

empty = :array.new()
ArrayCoverage.observe(:empty, fn -> {ArrayCoverage.snapshot(empty), :array.get(1000, empty), :array.is_array(empty), :array.is_array(:other)} end)
for {name, options} <- [{:integer_size, 3}, {:explicit_options, [size: 3, default: :hole, fixed: false]}, {:fixed_first, [fixed: false, size: 3]}, {:default_option, {:default, 1}}, {:fixed_option, :fixed}] do
  ArrayCoverage.observe(name, fn -> ArrayCoverage.snapshot(:array.new(options)) end)
end
ArrayCoverage.observe(:constructor_two_arguments, fn -> ArrayCoverage.snapshot(:array.new(3, [default: :hole, fixed: false])) end)
ArrayCoverage.observe(:constructor_size_override, fn -> ArrayCoverage.snapshot(:array.new(3, 0)) end)
values = :array.from_list([:hole, :a, :hole, :b, :hole], :hole)
ArrayCoverage.observe(:from_list, fn -> ArrayCoverage.snapshot(values) end)
grown = :array.set(7, :c, values)
ArrayCoverage.observe(:persistent_set, fn -> {ArrayCoverage.snapshot(values), ArrayCoverage.snapshot(grown), :array.get(6, grown)} end)
ArrayCoverage.observe(:reset_existing, fn -> ArrayCoverage.snapshot(:array.reset(7, grown)) end)
ArrayCoverage.observe(:reset_outside, fn -> ArrayCoverage.snapshot(:array.reset(1000, values)) end)
ArrayCoverage.observe(:resize_shrink_and_expand, fn -> ArrayCoverage.snapshot(:array.resize(9, :array.resize(3, grown))) end)
shrunken = :array.resize(3, grown)
ArrayCoverage.observe(:hidden_entries_read_as_default, fn -> {:array.get(3, shrunken), :array.get(7, shrunken), ArrayCoverage.snapshot(shrunken)} end)
ArrayCoverage.observe(:reset_hidden_entry, fn -> ArrayCoverage.snapshot(:array.resize(9, :array.reset(7, shrunken))) end)
ArrayCoverage.observe(:set_after_shrink, fn -> ArrayCoverage.snapshot(:array.resize(9, :array.set(5, :d, shrunken))) end)
ArrayCoverage.observe(:resize_sparse, fn -> ArrayCoverage.snapshot(:array.resize(values)) end)
ArrayCoverage.observe(:set_default_extends, fn -> ArrayCoverage.snapshot(:array.set(8, :hole, values)) end)
ArrayCoverage.observe(:empty_from_list, fn -> ArrayCoverage.snapshot(:array.from_list([], nil)) end)
ordered = :array.from_orddict([{2, :a}, {5, :hole}, {7, :b}], :hole)
ArrayCoverage.observe(:ordered_gaps, fn -> ArrayCoverage.snapshot(ordered) end)
ArrayCoverage.observe(:exact_numeric_default, fn -> ArrayCoverage.snapshot(:array.from_list([1, 1.0, 2, 1], 1)) end)
fixed = :array.fix(values)
ArrayCoverage.observe(:fixed_read_outside, fn -> :array.get(5, fixed) end)
ArrayCoverage.observe(:fixed_set_outside, fn -> :array.set(5, :x, fixed) end)
ArrayCoverage.observe(:fixed_reset_outside, fn -> :array.reset(5, fixed) end)
ArrayCoverage.observe(:relaxed_grows, fn -> ArrayCoverage.snapshot(:array.set(6, :x, :array.relax(fixed))) end)
ArrayCoverage.observe(:fixed_explicit_resize, fn -> ArrayCoverage.snapshot(:array.resize(7, fixed)) end)

for {name, fun} <- [
  {:negative_new, fn -> :array.new(-1) end},
  {:invalid_new_option, fn -> :array.new([{:fixed, :maybe}]) end},
  {:negative_two_argument_new, fn -> :array.new(-1, []) end},
  {:negative_get, fn -> :array.get(-1, empty) end},
  {:float_get, fn -> :array.get(1.0, empty) end},
  {:negative_set, fn -> :array.set(-1, :x, empty) end},
  {:negative_reset, fn -> :array.reset(-1, empty) end},
  {:negative_resize, fn -> :array.resize(-1, empty) end},
  {:negative_ordered_index, fn -> :array.from_orddict([{-1, :x}]) end},
  {:descending_ordered_index, fn -> :array.from_orddict([{2, :a}, {1, :b}]) end},
  {:duplicate_ordered_index, fn -> :array.from_orddict([{1, :a}, {1, :b}]) end},
  {:float_ordered_index, fn -> :array.from_orddict([{1.0, :a}]) end},
  {:invalid_ordered_tuple, fn -> :array.from_orddict([{1, :a, :extra}]) end},
  {:invalid_ordered_tail, fn -> :array.from_orddict([{0, :valid}, {2, :a, :extra}]) end},
  {:invalid_ordered_entry, fn -> :array.from_orddict([:a]) end},
  {:invalid_ordered_list, fn -> :array.from_orddict(:a) end},
  {:invalid_from_list, fn -> :array.from_list(:a) end},
  {:invalid_array_size, fn -> :array.size({:bad}) end},
  {:invalid_array_default, fn -> :array.default({:bad}) end},
  {:invalid_array_fixed, fn -> :array.is_fix({:bad}) end},
  {:invalid_array_fix, fn -> :array.fix({:bad}) end},
  {:invalid_array_relax, fn -> :array.relax({:bad}) end},
  {:invalid_array_sparse_size, fn -> :array.sparse_size({:bad}) end},
  {:invalid_array_conversion, fn -> :array.to_list({:bad}) end},
  {:invalid_array_sparse_conversion, fn -> :array.sparse_to_orddict({:bad}) end}
] do
  ArrayCoverage.observe(name, fun)
end
"#).unwrap();
    let mut reference = Command::new("elixir");
    reference.arg(&source);
    let reference = process::execute(reference, Duration::from_secs(60));
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.args(["run", source.to_str().unwrap()]);
    let native = process::execute(native, Duration::from_secs(240));
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let native = String::from_utf8(native.stdout).unwrap();
    let reference = String::from_utf8(reference.stdout).unwrap();
    let expected: Vec<_> = reference.lines().collect();
    let actual: Vec<_> = native.lines().collect();
    assert_eq!(actual.len(), expected.len(), "{native}");
    let differences: Vec<_> = actual
        .iter()
        .zip(expected.iter())
        .filter(|(actual, expected)| actual != expected)
        .map(|(actual, expected)| format!("OTP: {expected}\nTonic: {actual}"))
        .collect();
    assert!(differences.is_empty(), "{}", differences.join("\n\n"));
    fs::remove_dir_all(root).unwrap();
}
