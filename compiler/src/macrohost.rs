//! Compile-time macro expansion through the macro host.
//!
//! Macros whose bodies are quote templates are expanded natively
//! (expr.rs). Anything else runs in the macro host: a small program built
//! from the prelude (lib/macro_host.ex, compiled once per tonic build and
//! cached) that interprets the program's modules with Tonic.Eval. The
//! compiler sends it the quoted arguments, it runs the macro and replies
//! with the expansion as Elixir source, which is parsed back here.

use crate::ast::*;
use crate::collect::R;
use crate::core::*;
use crate::expand::{Cx, Expander};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

struct Host {
    child: Child,
    stderr: Arc<Mutex<Vec<u8>>>,
    drain: Option<JoinHandle<()>>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

static INPUTS: OnceLock<Vec<String>> = OnceLock::new();
static CONFIGS: OnceLock<Vec<String>> = OnceLock::new();
static HOST: Mutex<Option<Result<Host, String>>> = Mutex::new(None);

/// Source files of the program being compiled (loaded into the host).
pub fn set_inputs(files: Vec<String>) {
    let _ = INPUTS.set(files);
}

/// Generated project configuration evaluated before loading source modules.
pub fn set_config_inputs(files: Vec<String>) {
    let _ = CONFIGS.set(files);
}

fn cache_key() -> Result<String, String> {
    use std::hash::{Hash, Hasher};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&exe).map_err(|e| e.to_string())?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let runtime = crate::find_runtime()?;
    std::fs::read(&runtime)
        .map_err(|e| format!("{}: {}", runtime.display(), e))?
        .hash(&mut h);
    Ok(format!("{:016x}", h.finish()))
}

fn cache_dir() -> PathBuf {
    if let Ok(d) = std::env::var("TONIC_CACHE") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache").join("tonic")
}

/// The host executable, built on first use.
fn host_exe() -> Result<PathBuf, String> {
    let dir = cache_dir();
    let key = cache_key()?;
    let path = dir.join(format!("macrohost-{}", key));
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tag = format!("{}", std::process::id());
    let script = dir.join(format!("macrohost-{}.{}.exs", key, tag));
    let tmp = dir.join(format!("macrohost-{}.{}", key, tag));
    std::fs::write(&script, "Tonic.MacroHost.main()\n").map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let output = Command::new(exe)
        .env("TONIC_BUILDING_HOST", "1")
        .arg("build")
        .arg(&script)
        .arg("-o")
        .arg(&tmp)
        .arg("-O1")
        .stdout(Stdio::null())
        .output();
    let _ = std::fs::remove_file(&script);
    let output = output.map_err(|e| format!("could not build the macro host: {}", e))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "could not build the macro host ({}):\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        ));
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;

    Ok(path)
}

fn start() -> Result<Host, String> {
    let exe = host_exe()?;
    let child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start the macro host: {}", e))?;
    let mut h = Host::from_child(child);
    for file in CONFIGS.get().cloned().unwrap_or_default() {
        h.request("LOADCONFIG", &file).map_err(|error| {
            format!(
                "could not load compile-time configuration {}:\n{}",
                file, error
            )
        })?;
    }
    for f in INPUTS.get().cloned().unwrap_or_default() {
        if f.ends_with(".erl") {
            continue;
        }
        let abs = std::fs::canonicalize(&f)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| f.clone());
        h.request("LOAD", &abs)
            .map_err(|e| format!("could not load {} into the macro host:\n{}", abs, e))?;
    }
    Ok(h)
}

impl Host {
    fn from_child(mut child: Child) -> Self {
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut pipe = child.stderr.take().unwrap();
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let capture = Arc::clone(&stderr);
        let drain = std::thread::spawn(move || {
            let mut buf = [0; 4096];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                #[cfg(not(test))]
                {
                    let _ = std::io::stderr().write_all(&buf[..n]);
                }
                let mut bytes = capture.lock().unwrap();
                bytes.extend_from_slice(&buf[..n]);
                if bytes.len() > 65536 {
                    let excess = bytes.len() - 65536;
                    bytes.drain(..excess);
                }
            }
        });
        Host {
            child,
            stdin,
            stdout,
            stderr,
            drain: Some(drain),
        }
    }

    fn request(&mut self, cmd: &str, payload: &str) -> Result<String, String> {
        let limit = if cmd == "AFTER" {
            256 * 1024 * 1024
        } else {
            16 * 1024 * 1024
        };
        if payload.len() > limit {
            return Err(format!(
                "macro host: {} request exceeds {} MiB",
                cmd,
                limit / 1024 / 1024
            ));
        }
        let reply = (|| {
            let io_err = |e: std::io::Error| format!("macro host: {}", e);
            write!(self.stdin, "{} {}\n", cmd, payload.len()).map_err(io_err)?;
            self.stdin.write_all(payload.as_bytes()).map_err(io_err)?;
            self.stdin.flush().map_err(io_err)?;
            read_reply(&mut self.stdout)
        })();
        reply.map_err(|error| {
            if let Ok(Some(status)) = self.child.try_wait() {
                if let Some(drain) = self.drain.take() {
                    let _ = drain.join();
                }
                let bytes = self.stderr.lock().unwrap();
                let diagnostics = String::from_utf8_lossy(&bytes);
                format!(
                    "{} (host exited with {})\n{}",
                    error,
                    status,
                    diagnostics.trim_end()
                )
            } else {
                let bytes = self.stderr.lock().unwrap();
                let diagnostics = String::from_utf8_lossy(&bytes);
                if diagnostics.is_empty() {
                    error
                } else {
                    format!("{}\n{}", error, diagnostics.trim_end())
                }
            }
        })
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(drain) = self.drain.take() {
            let _ = drain.join();
        }
    }
}

fn read_reply(reader: &mut impl BufRead) -> Result<String, String> {
    let mut header = Vec::new();
    reader
        .take(129)
        .read_until(b'\n', &mut header)
        .map_err(|e| format!("macro host reply: {}", e))?;
    if header.is_empty() {
        return Err("macro host closed stdout before replying".into());
    }
    if header.len() > 128 || !header.ends_with(b"\n") {
        return Err("macro host: invalid or truncated reply header".into());
    }
    let header = std::str::from_utf8(&header)
        .map_err(|_| "macro host: reply header is not UTF-8".to_string())?;
    let (tag, size) = header
        .trim_end_matches('\n')
        .split_once(' ')
        .ok_or("macro host: invalid reply header")?;
    if tag != "OK" && tag != "ERR" {
        return Err(format!("macro host: unknown reply tag {:?}", tag));
    }
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_digit()) {
        return Err("macro host: invalid reply length".into());
    }
    let len = size
        .parse::<usize>()
        .map_err(|_| "macro host: invalid reply length")?;
    if len > 16 * 1024 * 1024 {
        return Err("macro host: reply exceeds 16 MiB".into());
    }
    let mut body = vec![0; len];
    reader
        .read_exact(&mut body)
        .map_err(|e| format!("macro host: incomplete reply body: {}", e))?;
    let body =
        String::from_utf8(body).map_err(|_| "macro host: reply body is not UTF-8".to_string())?;
    if tag == "OK" {
        Ok(body)
    } else {
        Err(body)
    }
}

fn request(cmd: &str, payload: &str) -> Result<String, String> {
    if std::env::var_os("TONIC_BUILDING_HOST").is_some() {
        return Err(
            "macro host bootstrap requires native expansion; recursive host compilation refused"
                .into(),
        );
    }
    let mut g = HOST.lock().unwrap();
    if g.is_none() {
        *g = Some(start());
    }
    match g.as_mut().unwrap() {
        Ok(h) => h.request(cmd, payload),
        Err(e) => Err(e.clone()),
    }
}

/// Reaps the host and finishes forwarding its diagnostic stream.
pub fn shutdown() {
    if let Ok(mut host) = HOST.lock() {
        let _ = host.take();
    }
}

/// Elixir source for a literal term built by the quoter.
pub fn term_text(c: &CE, out: &mut String) -> Result<(), String> {
    match c {
        CE::Lit(l) => match l {
            Lit::Int(i) => out.push_str(&i.to_string()),
            Lit::Big(s) => out.push_str(s),
            Lit::Float(f) => {
                let mut s = format!("{:?}", f);
                if s.contains('e') && !s.contains('.') {
                    s = s.replacen('e', ".0e", 1);
                }
                out.push_str(&s)
            }
            Lit::Atom(a) => atom_text(a, out),
            Lit::Bin(b) => bin_text(b, out),
            Lit::Nil => out.push_str("[]"),
        },
        CE::Tuple(items) => {
            out.push('{');
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                term_text(it, out)?;
            }
            out.push('}');
        }
        CE::Cons(_, _) => {
            out.push('[');
            let mut cur = c;
            let mut first = true;
            loop {
                match cur {
                    CE::Cons(h, t) => {
                        if !first {
                            out.push_str(", ");
                        }
                        first = false;
                        term_text(h, out)?;
                        cur = t;
                    }
                    CE::Lit(Lit::Nil) => break,
                    other => {
                        out.push_str(" | ");
                        term_text(other, out)?;
                        break;
                    }
                }
            }
            out.push(']');
        }
        CE::Map(pairs) => {
            out.push_str("%{");
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                term_text(k, out)?;
                out.push_str(" => ");
                term_text(v, out)?;
            }
            out.push('}');
        }
        _ => return Err("macro arguments are not literal".into()),
    }
    Ok(())
}

fn atom_text(a: &str, out: &mut String) {
    match a {
        "nil" | "true" | "false" => out.push_str(a),
        _ if a
            .as_bytes()
            .first()
            .map(|b| b.is_ascii_lowercase() || *b == b'_')
            .unwrap_or(false)
            && a.strip_suffix('!')
                .or_else(|| a.strip_suffix('?'))
                .unwrap_or(a)
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_') =>
        {
            out.push(':');
            out.push_str(a);
        }
        _ => {
            out.push_str(":\"");
            for ch in a.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '#' => out.push_str("\\#"),
                    '\n' => out.push_str("\\n"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\x{{{:x}}}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
    }
}

fn bin_text(b: &[u8], out: &mut String) {
    match std::str::from_utf8(b) {
        Ok(s) => {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '#' => out.push_str("\\#"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                        out.push_str(&format!("\\x{{{:x}}}", c as u32))
                    }
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Err(_) => {
            out.push_str("<<");
            for (i, x) in b.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&x.to_string());
            }
            out.push_str(">>");
        }
    }
}

/// Expands `module.name(args)` in the macro host: the expansion's
/// statements, parsed on the caller's line.
pub fn expand(
    module: &str,
    name: &str,
    args: &[E],
    caller_module: &str,
    caller_fun: Option<(&str, usize)>,
    file: &str,
    aliases: &std::collections::HashMap<String, String>,
    line: u32,
) -> R<E> {
    expand_context(
        module,
        name,
        args,
        caller_module,
        caller_fun,
        file,
        aliases,
        line,
        None,
    )
}

fn expand_context(
    module: &str,
    name: &str,
    args: &[E],
    caller_module: &str,
    caller_fun: Option<(&str, usize)>,
    file: &str,
    aliases: &std::collections::HashMap<String, String>,
    line: u32,
    context: Option<&str>,
) -> R<E> {
    let mut text = String::from("{");
    atom_text(module, &mut text);
    text.push_str(", ");
    atom_text(name, &mut text);
    text.push_str(", [");
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            text.push_str(", ");
        }
        let q = crate::quote::quote_literal(a)?;
        term_text(&q, &mut text).map_err(|e| format!("{}:{}: {}", file, line, e))?;
    }
    text.push_str("], [module: ");
    if caller_module.is_empty() || caller_module == "Elixir.Tonic.Script" {
        text.push_str("nil");
    } else {
        atom_text(caller_module, &mut text);
    }
    text.push_str(", function: ");
    match caller_fun {
        Some((f, a)) => {
            text.push('{');
            atom_text(f, &mut text);
            text.push_str(&format!(", {}}}", a));
        }
        None => text.push_str("nil"),
    }
    text.push_str(", context: ");
    atom_text(context.unwrap_or("nil"), &mut text);
    text.push_str(", file: ");
    bin_text(file.as_bytes(), &mut text);
    text.push_str(&format!(", line: {}, aliases: [", line));
    let mut first = true;
    for (short, full) in aliases.iter() {
        if !first {
            text.push_str(", ");
        }
        first = false;
        text.push('{');
        atom_text(&format!("Elixir.{}", short), &mut text);
        text.push_str(", ");
        atom_text(full, &mut text);
        text.push('}');
    }
    text.push_str("]]}");
    if std::env::var("TONIC_DBG").is_ok() {
        eprintln!("EXPAND {}", text);
    }
    let src = request("EXPAND", &text).map_err(|e| {
        format!(
            "{}:{}: ** (CompileError) error while expanding macro {}.{}/{}:\n{}",
            file,
            line,
            module.trim_start_matches("Elixir."),
            name,
            args.len(),
            e.trim_end()
        )
    })?;
    let padded = format!("{}{}", "\n".repeat(line.saturating_sub(1) as usize), src);
    let es = crate::parser::parse_source(&padded, file)
        .map_err(|e| format!("{} (in the expansion of {}.{}: {})", e, module, name, src))?;
    Ok(match es.len() {
        1 => es.into_iter().next().unwrap(),
        _ => E::new(K::Block(es), line),
    })
}

pub fn generated_definitions(module: &str, file: &str, first: u32, last: u32) -> R<E> {
    let canonical = std::fs::canonicalize(file)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file.to_string());
    let mut text = String::from("{");
    atom_text(module, &mut text);
    text.push_str(", ");
    bin_text(canonical.as_bytes(), &mut text);
    text.push_str(&format!(", {}, {}}}", first, last));
    let source = request("DEFINITIONS", &text)
        .map_err(|error| format!("{}:{}: generated definitions failed: {}", file, first, error))?;
    Ok(E::new(K::Block(crate::parser::parse_source(&source, file)?), first))
}

/// Captures the value of an attribute at its original declaration, without repeating effects.
pub fn attribute_value(module: &str, name: &str, file: &str, line: u32) -> R<E> {
    let canonical = std::fs::canonicalize(file)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file.to_string());
    let mut text = String::from("{");
    atom_text(module, &mut text);
    text.push_str(", ");
    bin_text(canonical.as_bytes(), &mut text);
    text.push_str(&format!(", {}, ", line));
    atom_text(name, &mut text);
    text.push('}');
    let source = request("ATTRIBUTE", &text).map_err(|error| {
        format!(
            "{}:{}: could not evaluate @{} at compile time:\n{}",
            file, line, name, error
        )
    })?;
    let mut values = crate::parser::parse_source(&source, file)?;
    if values.len() != 1 {
        return Err(format!(
            "{}:{}: invalid compile-time attribute value",
            file, line
        ));
    }
    Ok(values.remove(0))
}

/// Evaluates a compile-time expression against the native host's available APIs.
pub fn evaluate(expression: &E, file: &str, line: u32) -> R<E> {
    let quoted = crate::quote::quote_literal(expression)?;
    let mut text = String::from("{");
    term_text(&quoted, &mut text)?;
    text.push_str(", ");
    bin_text(file.as_bytes(), &mut text);
    text.push('}');
    let source = request("EVAL", &text).map_err(|error| {
        format!(
            "{}:{}: compile-time evaluation failed:\n{}",
            file, line, error
        )
    })?;
    let mut expressions = crate::parser::parse_source(&source, file)?;
    if expressions.len() != 1 {
        return Err(format!("{}:{}: invalid compile-time value", file, line));
    }
    Ok(expressions.remove(0))
}

/// Runs after_compile callbacks with the generated native LLVM artifact.
pub fn run_after_callbacks(module: &str, artifact: &str, file: &str, line: u32) -> R<()> {
    let mut text = String::new();
    atom_text(module, &mut text);
    text.push('\n');
    text.push_str(artifact);
    request("AFTER", &text).map_err(|e| {
        format!(
            "{}:{}: error in after_compile callbacks for {}:\n{}",
            file, line, module, e
        )
    })?;
    Ok(())
}

/// Attribute values after a definition callback at the given source line.
pub fn callback_attributes(module: &str, file: &str, line: u32) -> R<E> {
    let mut text = String::from("{");
    atom_text(module, &mut text);
    text.push_str(&format!(", {}}}", line));
    let src = request("ATTRS", &text).map_err(|e| {
        format!(
            "{}:{}: error in definition callbacks for {}:\n{}",
            file, line, module, e
        )
    })?;
    let mut es = crate::parser::parse_source(&src, file)?;
    if es.len() != 1 {
        return Err(format!(
            "{}:{}: invalid attributes from macro host",
            file, line
        ));
    }
    Ok(es.remove(0))
}

/// Definitions generated by a module's before_compile callbacks.
pub fn callback_expansions(module: &str, file: &str, line: u32) -> R<E> {
    let mut text = String::new();
    atom_text(module, &mut text);
    let src = request("CALLBACKS", &text).map_err(|e| {
        format!(
            "{}:{}: error in compilation callbacks for {}:\n{}",
            file, line, module, e
        )
    })?;
    let es = crate::parser::parse_source(&src, file)?;
    Ok(E::new(K::Block(es), line))
}

impl Expander {
    /// Expands `module.name(args)` in the macro host; returns the code.
    pub fn host_expand(
        &mut self,
        module: &str,
        name: &str,
        args: &[E],
        line: u32,
        cx: &mut Cx,
    ) -> R<E> {
        let fun = match cx.fun.rsplit_once('/') {
            Some((f, a)) if !f.starts_with("__script__") => {
                a.parse::<usize>().ok().map(|a| (f.to_string(), a))
            }
            _ => None,
        };
        let m = cx.module().to_string();
        let file = cx.file().to_string();
        let aliases = cx.env.aliases.clone();
        expand_context(
            module,
            name,
            args,
            &m,
            fun.as_ref().map(|(f, a)| (f.as_str(), *a)),
            &file,
            &aliases,
            line,
            cx.macro_context
                .or(if cx.guard { Some("guard") } else { None }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::read_reply;
    use std::io::Cursor;

    #[test]
    fn framed_replies_preserve_bytes_and_next_frame() {
        let mut input = Cursor::new("OK 2\néERR 4\noops".as_bytes());
        assert_eq!(read_reply(&mut input).unwrap(), "é");
        assert_eq!(read_reply(&mut input).unwrap_err(), "oops");
    }

    #[test]
    fn reply_rejects_eof_truncation_unknown_tags_and_unbounded_lengths() {
        for bytes in [
            b"".as_slice(),
            b"OK 1",
            b"OK 2\na",
            b"BAD 0\n",
            b"OK -1\n",
            b"OK 16777217\n",
            b"OK 1\n\xff",
        ] {
            assert!(
                read_reply(&mut Cursor::new(bytes)).is_err(),
                "accepted {:?}",
                bytes
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn child_stderr_is_drained_without_blocking_and_child_is_reaped() {
        use std::process::{Command, Stdio};
        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg("read line; dd if=/dev/zero bs=1024 count=128 1>&2 2>/dev/null; printf 'host crashed' >&2; exit 7")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let mut host = super::Host::from_child(child);
        let error = host.request("LOAD", "").unwrap_err();
        assert!(error.contains("closed stdout"), "{}", error);
        let status = host.child.wait().unwrap();
        assert_eq!(status.code(), Some(7));
        if let Some(drain) = host.drain.take() {
            drain.join().unwrap();
        }
        let diagnostics = host.stderr.lock().unwrap();
        assert!(diagnostics.len() <= 65536);
        assert!(diagnostics.ends_with(b"host crashed"));
    }
    #[cfg(unix)]
    #[test]
    fn dropping_live_host_terminates_the_child() {
        use std::process::{Command, Stdio};
        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg("exec sleep 60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        drop(super::Host::from_child(child));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
