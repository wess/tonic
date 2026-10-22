use super::{project, run, Fixture};
use std::fs;
use std::process::Command;

#[test]
fn native_test_selection_and_runner_options_match_mix() {
    let f = Fixture::new();
    project(&f);
    f.write(
        "test/test_helper.exs",
        "ExUnit.start(exclude: [:external], seed: 99)\n",
    );
    let selected = r#"defmodule SelectionTest do
  use ExUnit.Case
  defp record(name), do: File.write!(System.fetch_env!("SELECTION_LOG"), name <> "\n", [:append])
  @tag :fast
  test "fast" do
    record("fast")
    if System.get_env("CHECK_OPTIONS") do
      opts = ExUnit.configuration()
      assert opts[:seed] == 0
      assert opts[:timeout] == 700
      assert opts[:max_failures] == 1
      assert opts[:trace]
    end
  end
  @tag :slow
  @tag group: :alpha
  test "slow", do: record("slow")
  @tag :external
  test "external", do: record("external")
  test "plain", do: record("plain")
end
"#;
    f.write("test/selection_test.exs", selected);
    f.write("test/nested/other_test.exs", "defmodule OtherTest do\n use ExUnit.Case\n test \"other\", do: File.write!(System.fetch_env!(\"SELECTION_LOG\"), \"other\\n\", [:append])\nend\n");
    f.write("test/failing_test.exs", "defmodule FailingTest do\n use ExUnit.Case\n test \"first\" do\n File.write!(System.fetch_env!(\"SELECTION_LOG\"), \"failure\\n\", [:append])\n assert false\n end\n test \"second\" do\n File.write!(System.fetch_env!(\"SELECTION_LOG\"), \"failure\\n\", [:append])\n assert false\n end\nend\n");
    f.write("test/timeout_test.exs", "defmodule TimeoutTest do\n use ExUnit.Case\n test \"times out\", do: Process.sleep(100)\nend\n");
    let log = f.0.join("selectionlog");
    let compare = |args: &[&str], expected: &[&str], status: i32, check_options: bool| {
        let mut reference = Command::new("mix");
        reference
            .current_dir(&f.0)
            .env("MIX_ENV", "test")
            .env("SELECTION_LOG", &log)
            .arg("test")
            .args(args);
        if check_options {
            reference.env("CHECK_OPTIONS", "1");
        }
        fs::write(&log, "").unwrap();
        let reference = run(reference);
        assert_eq!(
            reference.status.code(),
            Some(status),
            "reference: {} {}",
            String::from_utf8_lossy(&reference.stdout),
            String::from_utf8_lossy(&reference.stderr)
        );
        let reference_log = fs::read_to_string(&log).unwrap();
        let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
        native.current_dir(&f.0).arg("test");
        native
            .env("MIX_ENV", "test")
            .env("SELECTION_LOG", &log)
            .args(args);
        if check_options {
            native.env("CHECK_OPTIONS", "1");
        }
        fs::write(&log, "").unwrap();
        let native = run(native);
        assert_eq!(
            native.status.code(),
            Some(status),
            "native: {} {}",
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&native.stderr)
        );
        let native_log = fs::read_to_string(&log).unwrap();
        let ordered = |value: &str| {
            let mut lines: Vec<String> = value.lines().map(str::to_owned).collect();
            lines.sort();
            lines
        };
        assert_eq!(ordered(&native_log), ordered(&reference_log));
        let mut expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(ordered(&native_log), expected);
    };
    compare(&["--only", "fast", "--seed", "0"], &["fast"], 0, false);
    compare(&["test/nested"], &["other"], 0, false);
    compare(
        &["test/selection_test.exs", "--include", "external"],
        &["fast", "slow", "external", "plain"],
        0,
        false,
    );
    compare(
        &[
            "test/selection_test.exs",
            "--exclude",
            "slow",
            "--exclude",
            "fast",
        ],
        &["plain"],
        0,
        false,
    );
    compare(
        &[
            "test/selection_test.exs",
            "--exclude",
            "slow",
            "--seed",
            "0",
            "--max-failures",
            "1",
            "--timeout",
            "700",
            "--trace",
        ],
        &["fast", "plain"],
        0,
        true,
    );
    let line = selected
        .lines()
        .position(|line| line.contains("test \"slow\""))
        .unwrap()
        + 1;
    let location = format!("test/selection_test.exs:{line}");
    compare(&[&location], &["slow"], 0, false);
    compare(
        &["--only", "group:alpha", "--only", "fast"],
        &["fast", "slow"],
        0,
        false,
    );
    compare(&["--only", "missing"], &[], 1, false);
    compare(
        &[
            "test/failing_test.exs",
            "--max-failures",
            "1",
            "--seed",
            "0",
        ],
        &["failure"],
        2,
        false,
    );
    compare(&["test/timeout_test.exs", "--timeout", "1"], &[], 2, false);
    let mut command = f.tonic("test");
    command
        .args(["--unsupported-filter"])
        .env("TONIC_CC", "/missing/compiler");
    let result = run(command);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported native test options"));
}
