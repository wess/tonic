//! Load evaluated Mix projects and their fetched dependency sources.
use crate::ast::{E, K};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Project {
    pub app: String,
    pub inputs: Vec<String>,
    pub macro_inputs: Vec<String>,
    pub config_inputs: Vec<String>,
    scratch: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

fn strings(value: Option<&E>, name: &str) -> Result<Vec<String>, String> {
    match value.map(|e| &e.k) {
        Some(K::List(items, None)) => items
            .iter()
            .map(|item| match &item.k {
                K::Str(bytes) => String::from_utf8(bytes.clone()).map_err(|e| e.to_string()),
                _ => Err(format!(
                    "invalid Mix metadata: {} must contain strings",
                    name
                )),
            })
            .collect(),
        _ => Err(format!("invalid Mix metadata: missing {}", name)),
    }
}

pub fn load(input: &str) -> Result<Option<Project>, String> {
    let path = Path::new(input);
    let root = if path.is_dir() && path.join("mix.exs").is_file() {
        path
    } else if path.file_name().is_some_and(|name| name == "mix.exs") {
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    } else {
        return Ok(None);
    };
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let scratch = std::env::temp_dir().join(format!(
        "tonicproject{}{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let mut project = Project {
        app: String::new(),
        inputs: vec![],
        macro_inputs: vec![],
        config_inputs: vec![],
        scratch,
    };
    let bridge = project.scratch.join("project.exs");
    std::fs::write(&bridge, include_str!("project.exs")).map_err(|e| e.to_string())?;
    let mix = std::env::var_os("TONIC_MIX").unwrap_or_else(|| "mix".into());
    let output = Command::new(&mix)
        .current_dir(&project.scratch)
        .env("HEX_OFFLINE", "1")
        .args(["run", "--no-mix-exs", "--no-compile", "--no-deps-check", "--no-start"])
        .arg(&bridge).arg(&root).arg(&project.scratch)
        .stdin(Stdio::null()).output()
        .map_err(|e| format!("could not run Mix ({:?}): {}. Install Elixir/OTP and Mix or set TONIC_MIX; standalone source files do not require Mix", mix, e))?;
    if !output.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    if !output.status.success() {
        return Err(format!(
            "Mix project loading failed for {} ({})",
            root.display(),
            output.status
        ));
    }
    let manifest = project.scratch.join("manifest.exs");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("missing Mix project metadata: {}", e))?;
    let parsed = crate::parser::parse_source(&text, &manifest.to_string_lossy())?;
    let data = parsed.first().ok_or("empty Mix project metadata")?;
    project.app = match data.kw_get("app").map(|e| &e.k) {
        Some(K::Str(bytes)) => String::from_utf8(bytes.clone()).map_err(|e| e.to_string())?,
        _ => return Err("invalid Mix metadata: missing app".into()),
    };
    project.inputs = strings(data.kw_get("inputs"), "inputs")?;
    project.macro_inputs = strings(data.kw_get("macro_inputs"), "macro_inputs")?;
    project.config_inputs = strings(data.kw_get("config_inputs"), "config_inputs")?;
    Ok(Some(project))
}
