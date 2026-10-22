use std::process::Command;

#[test]
fn inventory_reports_declarations_without_the_llvm_toolchain() {
  let output = Command::new(env!("CARGO_BIN_EXE_tonic"))
    .arg("inventory")
    .env("TONIC_CC", "/no/compiler")
    .output()
    .unwrap();
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let mut child = Command::new("python3")
    .args(["-c", r#"
import json, sys
data = json.load(sys.stdin)
rows = {(r["module"], r["name"], r["arity"], r["kind"]): r for r in data["exports"]}
assert ("Elixir.Enum", "map", 2, "function") in rows
assert ("Elixir.Kernel", "if", 2, "macro") in rows
assert ("erlang", "is_integer", 1, "function") in rows
assert len(rows) == len(data["exports"])
assert all(not r["module"].startswith("Elixir.Tonic.") for r in data["exports"])
assert all(not r["source"].startswith("<prelude>") for r in data["exports"])
"#])
    .stdin(std::process::Stdio::piped())
    .spawn()
    .unwrap();
  use std::io::Write;
  child.stdin.take().unwrap().write_all(&output.stdout).unwrap();
  assert!(child.wait().unwrap().success());
}
