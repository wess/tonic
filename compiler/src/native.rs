//! NIF libraries compiled from their C sources and linked into the program.
//!
//! The sources include tonic's `erl_nif.h`, whose `ERL_NIF_INIT` registers the
//! library with the runtime before `main`. Objects are cached by the hash of
//! the source, flags, header and compiler.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::Command;

const HEADER: &str = include_str!("../../runtime/include/erl_nif.h");

fn digest(parts: &[&[u8]]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for part in parts {
        part.hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

/// Compiles every "source\tflag..." entry and returns the object files.
pub fn objects(entries: &[String], cc: &str, cache: &PathBuf) -> Result<Vec<PathBuf>, String> {
    if entries.is_empty() {
        return Ok(vec![]);
    }
    let include = cache.join("include").join(digest(&[HEADER.as_bytes()]));
    if !include.join("erl_nif.h").is_file() {
        std::fs::create_dir_all(&include).map_err(|e| e.to_string())?;
        let tmp = include.join(format!("erl_nif.h.{}", std::process::id()));
        std::fs::write(&tmp, HEADER).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, include.join("erl_nif.h")).map_err(|e| e.to_string())?;
    }
    let dir = cache.join("nif");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let results: Vec<Result<PathBuf, String>> = std::thread::scope(|scope| {
        let jobs: Vec<_> = entries
            .iter()
            .map(|entry| {
                let include = include.clone();
                let dir = dir.clone();
                scope.spawn(move || compile(entry, cc, &include, &dir))
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap_or_else(|_| Err("NIF compilation panicked".into()))).collect()
    });
    results.into_iter().collect()
}

fn compile(entry: &str, cc: &str, include: &PathBuf, dir: &PathBuf) -> Result<PathBuf, String> {
    let mut parts = entry.split('\t');
    let source = parts.next().unwrap_or_default();
    let flags: Vec<&str> = parts.collect();
    let text = std::fs::read(source).map_err(|e| format!("{}: {}", source, e))?;
    let key = digest(&[&text, flags.join(" ").as_bytes(), HEADER.as_bytes(), cc.as_bytes(), source.as_bytes()]);
    let object = dir.join(format!("{}.o", key));
    if object.is_file() {
        return Ok(object);
    }
    let tmp = dir.join(format!("{}.{}.o", key, std::process::id()));
    let output = Command::new(cc)
        .args(["-c", "-O2", "-w", "-fPIC", "-DNDEBUG=1"])
        .arg(format!("-I{}", include.display()))
        .args(&flags)
        .arg(source)
        .arg("-o")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("failed to run {}: {}", cc, e))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "compiling NIF source {} failed:\n{}",
            source,
            String::from_utf8_lossy(&output.stderr).trim_end()
        ));
    }
    std::fs::rename(&tmp, &object).map_err(|e| e.to_string())?;
    Ok(object)
}
