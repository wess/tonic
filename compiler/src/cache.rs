//! The on-disk cache for the compiled prelude, the macro host and NIF objects.
//!
//! Entries are keyed by the identity of the tonic binary and runtime library.
//! Each new tonic build adds entries, so entries for other builds are removed
//! once they have gone unused for a week.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

const STALE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub fn dir() -> PathBuf {
    if let Ok(d) = std::env::var("TONIC_CACHE") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache").join("tonic")
}

/// Path, size and modification time: enough to tell builds apart without
/// reading and hashing tens of megabytes on every compile.
fn stamp(path: &Path, h: &mut impl Hasher) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    path.hash(h);
    meta.len().hash(h);
    meta.modified().ok().hash(h);
    Ok(())
}

/// Key of the running tonic binary.
pub fn compiler_key() -> Result<String, String> {
    static KEY: OnceLock<Result<String, String>> = OnceLock::new();
    KEY.get_or_init(|| {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        stamp(&exe, &mut h)?;
        Ok(format!("{:016x}", h.finish()))
    })
    .clone()
}

/// Key of the running tonic binary together with a runtime library.
pub fn toolchain_key(runtime: &Path) -> Result<String, String> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    compiler_key()?.hash(&mut h);
    stamp(runtime, &mut h)?;
    Ok(format!("{:016x}", h.finish()))
}

/// Marks a cache entry as used, so pruning keeps it. Read-only on purpose:
/// opening a running executable for writing gets it killed on macOS.
pub fn touch(path: &Path) {
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

/// Removes `<prefix>-*` entries that do not belong to `key` and have not been
/// used for a week, including temporaries left by interrupted builds.
pub fn prune(prefix: &str, key: &str) {
    let Ok(entries) = std::fs::read_dir(dir()) else { return };
    let own = format!("{}-{}", prefix, key);
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(&format!("{}-", prefix)) || name.starts_with(&own) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > STALE);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
