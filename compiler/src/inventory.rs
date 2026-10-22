//! Export declarations for comparison with a pinned Elixir/OTP installation.

use crate::{bifs, collect, expand, prelude};
use std::collections::BTreeMap;
use std::fmt::Write;

fn json(value: &str) -> String {
  let mut out = String::from("\"");
  for c in value.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      c if c.is_control() => { let _ = write!(out, "\\u{:04x}", c as u32); }
      c => out.push(c),
    }
  }
  out.push('"');
  out
}

pub fn run() -> Result<String, String> {
  let mut collected = collect::Collector::new();
  for (name, source) in prelude::FILES {
    collected.add_file(source, &format!("<prelude>/{}", name), true)?;
  }
  collected.finalize_uses();
  let expanded = expand::Expander::new(collected);
  let mut exports = BTreeMap::new();
  for ((module, name, arity), public) in &expanded.funs {
    if *public {
      let source = expanded.mods.get(module).map(|m| m.file.as_str()).unwrap_or("");
      exports.insert((module.clone(), name.clone(), *arity, "function"), source.to_string());
    }
  }
  for ((module, name, arity), definition) in &expanded.macros {
    if definition.public {
      exports.insert((module.clone(), name.clone(), *arity, "macro"), definition.env.file.clone());
    }
  }
  for (module, metadata) in &expanded.mods {
    for (name, arity) in metadata.guards.keys() {
      if !metadata.private_guards.contains(&(name.clone(), *arity)) {
        exports.insert((module.clone(), name.clone(), *arity, "macro"), metadata.file.clone());
      }
    }
  }
  for line in include_str!("../data/kernel_macros.txt").lines() {
    if let Some((name, arity)) = line.split_once(' ') {
      if let Ok(arity) = arity.parse() {
        exports.entry(("Elixir.Kernel".into(), name.into(), arity, "macro"))
          .or_insert_with(|| "compiler/src/expr.rs".into());
      }
    }
  }
  for (module, name, arity, _) in bifs::entries() {
    if module != "tonic" {
      exports.entry((module.into(), name.into(), arity, "function"))
        .or_insert_with(|| "compiler/src/bifs.rs".into());
    }
  }
  let mut out = format!("{{\"version\":{},\"exports\":[", json(env!("CARGO_PKG_VERSION")));
  let mut separator = "";
  for ((module, name, arity, kind), source) in exports {
    if module == "tonic" || module.starts_with("Elixir.Tonic.") {
      continue;
    }
    let source = source.replace("<prelude>/", "lib/");
    let _ = write!(out, "{}{{\"module\":{},\"name\":{},\"arity\":{},\"kind\":{},\"source\":{}}}",
      separator, json(&module), json(&name), arity, json(kind), json(&source));
    separator = ",";
  }
  out.push_str("]}\n");
  Ok(out)
}
