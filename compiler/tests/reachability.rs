#[path = "support/process.rs"]
mod process;

#[path = "../src/core.rs"]
#[allow(dead_code)]
mod ir;

use std::fs;
use std::process::Command;
use std::time::Duration;

#[test]
fn generated_names_retain_targets_without_keep_all() {
    let root = std::env::temp_dir().join(format!("tonicreach{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for (name, source, args, expected) in [
    ("apply", "[module, fun] = System.argv()\nIO.puts(apply(Module.concat([module]), String.to_atom(fun), [\"x\", 3, \"0\"]))\n",
      vec!["String", "pad_leading"], "00x\n"),
    ("capture", "[module, name] = System.argv()\nfun = Function.capture(Module.concat([module]), String.to_atom(name), 3)\nIO.puts(fun.(\"x\", 3, \"0\"))\n",
      vec!["String", "pad_leading"], "00x\n"),
    ("reflection", "[module, name] = System.argv()\nmodule = Module.concat([module])\nCode.ensure_loaded?(module)\nIO.inspect(function_exported?(module, String.to_atom(name), 3))\n",
      vec!["String", "pad_leading"], "true\n"),
    ("operator", "defmodule Star do\n def unquote(:*)(left, right), do: String.pad_leading(left, right, \"0\")\nend\nIO.puts(Star.*(\"x\", 3))\n", vec![], "00x\n"),
    ("visibility", "defmodule VisibilityProbe do\n def public_value, do: :public\n def use_private, do: private_value()\n defp private_value, do: :secret\nend\n[module, public, private] = System.argv()\nmodule = Module.concat([module])\nIO.inspect({function_exported?(module, String.to_atom(public), 0), function_exported?(module, String.to_atom(private), 0)})\nIO.inspect(VisibilityProbe.use_private())\n",
      vec!["VisibilityProbe", "public_value", "private_value"], "{true, false}\n:secret\n"),
    ("visibilityapply", "defmodule VisibilityProbe do\n def public_value, do: :public\n def use_private, do: private_value()\n defp private_value, do: :secret\nend\n[module, public, private] = System.argv()\nmodule = Module.concat([module])\nIO.inspect(apply(module, String.to_atom(public), []))\nIO.inspect(try do\n apply(module, String.to_atom(private), [])\nrescue\n UndefinedFunctionError -> :private\nend)\n",
      vec!["VisibilityProbe", "public_value", "private_value"], ":public\n:private\n"),
  ] {
    let file = root.join(format!("{name}.exs"));
    fs::write(&file, source).unwrap();
    let mut beam = Command::new("elixir");
    beam.arg(&file).args(&args);
    let beam = process::execute(beam, Duration::from_secs(60));
    assert!(beam.status.success(), "{}", String::from_utf8_lossy(&beam.stderr));
    assert_eq!(String::from_utf8_lossy(&beam.stdout), expected);
    let mut native = Command::new(env!("CARGO_BIN_EXE_tonic"));
    native.arg("run").arg(&file).arg("--").args(&args).env_remove("TONIC_KEEP_ALL");
    let native = process::execute(native, Duration::from_secs(300));
    assert!(native.status.success(), "{name}: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout), expected, "{name}");
  }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn operator_names_are_distinct_from_dynamic_target_patterns() {
    use ir::FunKey;
    assert_ne!(
        FunKey::new("Elixir.Star", "*", 2),
        FunKey::dynamic(Some("Elixir.Star"), None, 2)
    );
    assert_ne!(
        FunKey::new("*", "target", 2),
        FunKey::dynamic(None, Some("target"), 2)
    );
    assert_ne!(FunKey::new("*", "*", 2), FunKey::dynamic(None, None, 2));
    assert_ne!(
        FunKey::dynamic(None, Some("*"), 2),
        FunKey::dynamic(None, None, 2)
    );
}
