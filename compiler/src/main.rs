//! tonic — an ahead-of-time compiler for Elixir targeting LLVM.

mod ast;
mod beam_atoms;
mod bifs;
mod codegen;
mod collect;
mod core;
mod ctime;
mod erl_lex;
mod erl_parse;
mod erl_pp;
mod erl_trans;
mod expand;
mod expr;
mod exunit;
mod lexer;
mod macrohost;
mod order;
mod parser;
mod prelude;
mod project;
mod quote;

use std::collections::HashSet;
use crate::core::{FunDef, FunKey};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

struct Opts {
    cmd: String,
    inputs: Vec<String>,
    macro_inputs: Vec<String>,
    config_inputs: Vec<String>,
    projects: Vec<project::Project>,
    output: Option<String>,
    opt: String,
    keep: bool,
    args: Vec<String>,
    verbose: bool,
}

fn usage() -> ! {
    eprintln!(
        "tonic {} — Elixir to native code via LLVM

usage:
  tonic run <file.ex|.exs>... [-- args]   compile and run
  tonic build <file.ex>... [-o out]       compile to an executable
  tonic build|run <project dir>           compile a Mix project (lib/, config/)
  tonic emit-llvm <file.ex>... [-o out.ll] write LLVM IR
  tonic check <file.ex>...                parse and expand only

options:
  -O0 | -O1 | -O2 | -O3   optimization level (default: -O0 for run, -O2 for build)
  --keep                  keep intermediate files
  -v                      verbose

environment:
  TONIC_CC        C compiler/linker to use (default: clang)
  TONIC_RUNTIME   path to libtonic_rt.a
  TONIC_MIX       Mix executable for project loading (default: mix)",
        env!("CARGO_PKG_VERSION")
    );
    std::process::exit(2)
}

fn parse_args() -> Opts {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    let cmd = args.remove(0);
    if cmd == "-h" || cmd == "--help" || cmd == "help" {
        usage();
    }
    if cmd == "--version" || cmd == "version" {
        println!("tonic {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    let mut o = Opts {
        cmd,
        inputs: vec![],
        macro_inputs: vec![],
        config_inputs: vec![],
        projects: vec![],
        output: None,
        opt: String::new(),
        keep: false,
        args: vec![],
        verbose: false,
    };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-o" => {
                i += 1;
                o.output = args.get(i).cloned();
            }
            "-O0" | "-O1" | "-O2" | "-O3" => o.opt = a.clone(),
            "--keep" => o.keep = true,
            "-v" => o.verbose = true,
            "--" => {
                o.args = args[i + 1..].to_vec();
                break;
            }
            _ => {
                if o.cmd == "run" && !o.inputs.is_empty() && !(a.ends_with(".ex") || a.ends_with(".exs") || a.ends_with(".erl")) {
                    o.args = args[i..].to_vec();
                    break;
                }
                o.inputs.push(a.clone())
            }
        }
        i += 1;
    }
    if o.inputs.is_empty() {
        usage();
    }
    // Mix projects: a directory with mix.exs (or the mix.exs file).
    let mut inputs = Vec::new();
    for i in std::mem::take(&mut o.inputs) {
        match project::load(&i) {
            Ok(Some(p)) => {
                if o.output.is_none() && o.cmd == "build" {
                    o.output = Some(p.app.clone());
                }
                inputs.extend(p.inputs.iter().cloned());
                o.macro_inputs.extend(p.macro_inputs.iter().cloned());
                o.config_inputs.extend(p.config_inputs.iter().cloned());
                o.projects.push(p);
            }
            Ok(None) => {
                o.macro_inputs.push(i.clone());
                inputs.push(i);
            }
            Err(e) => {
                eprintln!("** (Mix) {}", e);
                std::process::exit(1);
            }
        }
    }
    o.inputs = inputs;
    if o.opt.is_empty() {
        // `run` favours fast compiles; `build` favours fast code.
        o.opt = if o.cmd == "run" { "-O0".into() } else { "-O2".into() };
    }
    o
}

/// Atoms in user source order, so maps with new atom keys print in the
/// order they were written (OTP orders small-map atom keys by creation).
/// Also returns every atom the Elixir tokenizer would create (identifiers and
/// alias segments too), in order, for BEAM atom-index emulation.
fn user_atoms(src: &str, file: &str) -> (Vec<String>, Vec<String>) {
    let mut out = Vec::new();
    let mut created = Vec::new();
    if let Ok(toks) = lexer::tokenize(src, file, 1) {
        for t in toks {
            match t.t {
                lexer::T::Atom(a) | lexer::T::KwKey(a) => {
                    created.push(a.clone());
                    out.push(a)
                }
                lexer::T::Ident(a) => created.push(a),
                lexer::T::Alias(a) => created.extend(a.split('.').map(|x| x.to_string())),
                _ => {}
            }
        }
    }
    (out, created)
}

fn compile_to_ir(o: &Opts) -> Result<(String, Option<PathBuf>), String> {
    let tp = std::time::Instant::now();
    let mut c = collect::Collector::new();
    for (name, src) in prelude::FILES {
        c.add_file(src, &format!("<prelude>/{}", name), true)?;
    }
    if o.verbose {
        eprintln!("tonic:   prelude parse+collect {:?}", tp.elapsed());
    }
    let inputs = order::sources(&o.inputs)?;
    macrohost::set_config_inputs(o.config_inputs.clone());
    macrohost::set_inputs(order::sources(&o.macro_inputs)?);
    let mut atoms = Vec::new();
    let mut created = Vec::new();
    let mut user_mods: HashSet<String> = HashSet::new();
    let before: HashSet<String> = c.mods.keys().cloned().collect();
    for f in &inputs {
        let src = std::fs::read_to_string(f).map_err(|e| format!("could not read {}: {}", f, e))?;
        let (a, cr) = user_atoms(&src, f);
        atoms.extend(a);
        created.extend(cr);
        c.add_file(&src, f, false)?;
    }
    for (k, m) in c.mods.iter() {
        if !before.contains(k) || !m.is_prelude {
            user_mods.insert(k.clone());
        }
    }
    let redefines_stdlib = !c.redefined.is_empty();
    let exunit_modules: Vec<String> = c.exunit_modules.clone();
    c.finalize_uses();
    let script = std::mem::take(&mut c.script);
    let te = std::time::Instant::now();
    let mut ex = expand::Expander::new(c);
    ex.consolidated = !o.inputs.iter().any(|f| f.ends_with(".exs"));
    let r = ex.run(script);
    for w in &ex.warnings {
        if !w.contains("<prelude>") || o.verbose {
            eprintln!("{}", w);
        }
    }
    r?;
    if o.cmd == "check" {
        return Ok((String::new(), None));
    }
    // Protocol implementation modules for user types are reachable via the
    // dispatch functions; user modules are roots.
    if o.verbose {
        eprintln!("tonic:   expand {:?}", te.elapsed());
    }
    // Tonic.Internal.module_app/1: OTP application of each prelude module
    // (for stack traces' "(elixir 1.18.3)" prefixes).
    {
        use crate::core::*;
        let mut clauses = Vec::new();
        for (k, m) in ex.mods.iter() {
            if !m.file.starts_with("<prelude>/") || k == "erlang" || k == "tonic" {
                continue;
            }
            let app = if k.starts_with("Elixir.Logger") {
                "logger"
            } else if k.starts_with("Elixir.ExUnit") {
                "ex_unit"
            } else if k.starts_with("Elixir.") || k.starts_with("elixir_") {
                "elixir"
            } else if matches!(
                k.as_str(),
                "file" | "io" | "os" | "code" | "global" | "application" | "filename_kernel" | "inet"
                    | "gen_tcp" | "gen_udp" | "rpc" | "error_logger" | "net_kernel" | "erl_ddll" | "seq_trace"
                    | "logger" | "user_drv" | "group" | "disk_log" | "pg" | "erpc" | "auth" | "application_controller" | "application_master"
            ) {
                "kernel"
            } else {
                "stdlib"
            };
            clauses.push(Clause {
                pats: vec![Pat::Lit(Lit::Atom(k.clone()))],
                guard: None,
                body: CE::Tuple(vec![CE::atom("ok"), CE::atom(app)]),
            });
        }
        clauses.push(Clause { pats: vec![Pat::Wild], guard: None, body: CE::atom("undefined") });
        let v = ex.new_var();
        ex.out.push(FunDef {
            key: FunKey::new("Elixir.Tonic.Internal", "module_app", 1),
            params: vec![v],
            body: CE::Case(vec![CE::Var(v)], clauses, Fail::CaseClause),
            public: true,
        });
    }
    // Tonic.Internal.ex_unit_modules/0: the program's ExUnit.Case modules.
    {
        use crate::core::*;
        let mut l = CE::Lit(Lit::Nil);
        for m in exunit_modules.iter().rev() {
            l = CE::Cons(Box::new(CE::atom(m)), Box::new(l));
        }
        ex.out.push(FunDef {
            key: FunKey::new("Elixir.Tonic.Internal", "ex_unit_modules", 0),
            params: vec![],
            body: l,
            public: true,
        });
    }
    // Tonic.Internal.macro_exported?/3 for Kernel's and the program's macros.
    {
        use crate::core::*;
        let mut keys: Vec<(String, String, usize)> = include_str!("../data/kernel_macros.txt")
            .lines()
            .filter_map(|l| {
                let (n, a) = l.split_once(' ')?;
                Some(("Elixir.Kernel".to_string(), n.to_string(), a.parse().ok()?))
            })
            .collect();
        keys.extend(ex.macros.keys().cloned());
        keys.sort();
        keys.dedup();
        let mut clauses: Vec<Clause> = keys
            .into_iter()
            .map(|(m, f, a)| Clause {
                pats: vec![Pat::Lit(Lit::Atom(m)), Pat::Lit(Lit::Atom(f)), Pat::Lit(Lit::Int(a as i64))],
                guard: None,
                body: CE::atom("true"),
            })
            .collect();
        clauses.push(Clause { pats: vec![Pat::Wild, Pat::Wild, Pat::Wild], guard: None, body: CE::atom("false") });
        let vs: Vec<VarId> = (0..3).map(|_| ex.new_var()).collect();
        ex.out.push(FunDef {
            key: FunKey::new("Elixir.Tonic.Internal", "macro_exported?", 3),
            params: vs.clone(),
            body: CE::Case(vs.iter().map(|v| CE::Var(*v)).collect(), clauses, Fail::CaseClause),
            public: true,
        });
    }
    // Split: functions of prelude modules go to the precompiled prelude
    // object; protocol dispatchers and generated tables depend on the
    // program and are compiled with it.
    let per_program_mod = |m: &str| ex.protocols.contains_key(m) || m == "Elixir.Tonic.Script";
    let generated: HashSet<FunKey> = [
        FunKey::new("Elixir.Tonic.Internal", "module_app", 1),
        FunKey::new("Elixir.Tonic.Internal", "macro_exported?", 3),
        FunKey::new("Elixir.Tonic.Internal", "ex_unit_modules", 0),
    ]
    .into_iter()
    .collect();
    let is_prelude_def = |f: &FunDef| {
        ex.mods.get(&f.key.module).map(|m| m.is_prelude).unwrap_or(false)
            && !per_program_mod(&f.key.module)
            && !generated.contains(&f.key)
    };
    // A program that redefines standard library modules is compiled whole.
    let use_cache = std::env::var("TONIC_NO_PRELUDE_CACHE").is_err() && !redefines_stdlib;
    let mut prelude: Option<(PathBuf, Vec<String>)> = None;
    if use_cache {
        let prelude_defs: Vec<FunDef> = ex.out.iter().filter(|f| is_prelude_def(f)).cloned().collect();
        let per_program_syms: HashSet<String> = ex
            .out
            .iter()
            .filter(|f| !is_prelude_def(f) && (per_program_mod(&f.key.module) || generated.contains(&f.key)) && f.key.module != "Elixir.Tonic.Script")
            .map(|f| f.key.symbol())
            .collect();
        match prelude_object(o, &ex, &prelude_defs, &per_program_syms) {
            Ok(p) => prelude = Some(p),
            Err(e) => eprintln!("tonic: warning: precompiled prelude unavailable ({}); compiling it inline", e),
        }
    }
    let tr = std::time::Instant::now();
    let extra_roots: Vec<FunKey> = if prelude.is_some() {
        ex.out.iter().filter(|f| !is_prelude_def(f) && !user_mods.contains(&f.key.module)).map(|f| f.key.clone()).collect()
    } else {
        Vec::new()
    };
    let mut defs = ex.reachable(&user_mods, &extra_roots);
    if prelude.is_some() {
        defs.retain(|f| !is_prelude_def(f));
    }
    if o.verbose {
        eprintln!("tonic:   reachable {:?} ({} functions)", tr.elapsed(), defs.len());
    }
    let tg = std::time::Instant::now();
    let mut seed: Vec<String> = Vec::new();
    if let Some((_, patoms)) = &prelude {
        seed.extend(patoms.iter().cloned());
    }
    seed.extend(atoms.iter().cloned());
    let mut g = codegen::Gen::new(&seed, beam_atoms::Assigner::new(&created));
    if prelude.is_some() {
        g.mode = codegen::Mode::Program;
        g.externs = ex.out.iter().filter(|f| is_prelude_def(f)).map(|f| f.key.symbol()).collect();
    }
    g.files = module_files(&ex);

    // Top-level script code lives in the (first) .exs input.
    if let Some(first) = o.inputs.iter().find(|f| f.ends_with(".exs")).or(o.inputs.first()) {
        g.files.insert("Elixir.Tonic.Script".into(), first.clone());
    }
    let ir = g.gen_program(&defs);
    for module in ex.mods.values().filter(|m| !m.is_prelude && m.after_compile) {
        macrohost::run_after_callbacks(&module.name, &ir, &module.file, module.line)?;
    }
    if o.verbose {
        eprintln!("tonic:   codegen {:?}", tg.elapsed());
    }
    Ok((ir, prelude.map(|p| p.0)))
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn find_runtime() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("TONIC_RUNTIME") {
        return Ok(PathBuf::from(p));
    }
    let d = exe_dir();
    for cand in [
        d.join("libtonic_rt.a"),
        d.join("../lib/libtonic_rt.a"),
        d.join("../../target/release/libtonic_rt.a"),
    ] {
        if cand.exists() {
            return Ok(cand);
        }
    }
    Err("could not find libtonic_rt.a (set TONIC_RUNTIME)".into())
}

fn link_flags() -> Vec<&'static str> {
    if cfg!(target_os = "macos") {
        vec!["-lSystem", "-lc", "-lm", "-liconv", "-framework", "CoreFoundation"]
    } else {
        vec!["-lpthread", "-ldl", "-lm", "-lc", "-lgcc_s"]
    }
}

fn build(o: &Opts, ir: &str, prelude: Option<&Path>, out: &Path) -> Result<(), String> {
    let cc = std::env::var("TONIC_CC").unwrap_or_else(|_| "clang".into());
    let tmp = std::env::temp_dir().join(format!("tonic-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let ll = tmp.join("program.ll");
    let obj = tmp.join("program.o");
    std::fs::write(&ll, ir).map_err(|e| e.to_string())?;
    let t0 = std::time::Instant::now();
    let st = Command::new(&cc)
        .arg(&o.opt)
        .arg("-c")
        .arg("-Wno-override-module")
        .arg(&ll)
        .arg("-o")
        .arg(&obj)
        .status()
        .map_err(|e| format!("failed to run {}: {}", cc, e))?;
    if !st.success() {
        return Err(format!("LLVM compilation failed (IR kept at {})", ll.display()));
    }
    if o.verbose {
        eprintln!("tonic: llvm {:?}", t0.elapsed());
    }
    let rt = find_runtime()?;
    let mut cmd = Command::new(&cc);
    cmd.arg(&obj);
    if let Some(p) = prelude {
        cmd.arg(p);
    }
    cmd.arg(&rt).arg("-o").arg(out);
    for f in link_flags() {
        cmd.arg(f);
    }
    if cfg!(target_os = "macos") {
        cmd.arg("-Wl,-dead_strip");
    } else {
        cmd.arg("-Wl,--gc-sections");
    }
    let st = cmd.status().map_err(|e| format!("failed to run linker: {}", e))?;
    if !st.success() {
        return Err("linking failed".into());
    }
    if !o.keep {
        let _ = std::fs::remove_dir_all(&tmp);
    } else {
        eprintln!("tonic: intermediate files in {}", tmp.display());
    }
    Ok(())
}

fn main() -> ExitCode {
    let o = parse_args();
    let t0 = std::time::Instant::now();
    let compiled = compile_to_ir(&o);
    macrohost::shutdown();
    let (ir, prelude_obj) = match compiled {
        Ok(x) => x,
        Err(e) => {
            eprintln!("** (CompileError) {}", e);
            return ExitCode::from(1);
        }
    };
    if o.verbose {
        eprintln!("tonic: frontend {:?}", t0.elapsed());
    }
    match o.cmd.as_str() {
        "check" => ExitCode::SUCCESS,
        "emit-llvm" => {
            let out = o.output.clone().unwrap_or_else(|| {
                Path::new(&o.inputs[0]).with_extension("ll").to_string_lossy().into_owned()
            });
            if let Err(e) = std::fs::write(&out, ir) {
                eprintln!("{}", e);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        "build" => {
            let out = o.output.clone().unwrap_or_else(|| {
                Path::new(&o.inputs[0]).with_extension("").to_string_lossy().into_owned()
            });
            match build(&o, &ir, prelude_obj.as_deref(), Path::new(&out)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("tonic: {}", e);
                    ExitCode::from(1)
                }
            }
        }
        "run" => {
            let tmp = std::env::temp_dir().join(format!("tonic-run-{}", std::process::id()));
            if let Err(e) = build(&o, &ir, prelude_obj.as_deref(), &tmp) {
                eprintln!("tonic: {}", e);
                return ExitCode::from(1);
            }
            if o.verbose {
                eprintln!("tonic: total compile {:?}", t0.elapsed());
            }
            let st = Command::new(&tmp).args(&o.args).status();
            let _ = std::fs::remove_file(&tmp);
            match st {
                Ok(s) => match s.code() {
                    Some(c) => ExitCode::from(c as u8),
                    None => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            let sig = s.signal().unwrap_or(0);
                            let name = match sig {
                                11 => "Segmentation fault",
                                10 if cfg!(target_os = "macos") => "Bus error",
                                7 if !cfg!(target_os = "macos") => "Bus error",
                                6 => "Abort trap",
                                9 => "Killed",
                                _ => "Terminated by signal",
                            };
                            eprintln!("tonic: program crashed: {} (signal {})", name, sig);
                            return ExitCode::from((128 + sig) as u8);
                        }
                        #[allow(unreachable_code)]
                        ExitCode::from(1)
                    }
                },
                Err(e) => {
                    eprintln!("tonic: {}", e);
                    ExitCode::from(1)
                }
            }
        }
        _ => usage(),
    }
}

/// The file under lib/elixir/ that defines a stdlib module in Elixir 1.18.
fn elixir_module_file(module: &str) -> Option<&'static str> {
    static TABLE: &str = include_str!("../data/elixir_module_files.txt");
    TABLE.lines().find_map(|l| {
        let (m, f) = l.split_once(' ')?;
        if m == module {
            Some(f)
        } else {
            None
        }
    })
}

/// Source file shown in stack traces for each module.
fn module_files(ex: &expand::Expander) -> std::collections::HashMap<String, String> {
    ex.mods
        .iter()
        .map(|(k, m)| {
            let f = match m.file.strip_prefix("<prelude>/") {
                Some(rest) => {
                    if let Some(real) = k.strip_prefix("Elixir.").and_then(elixir_module_file) {
                        real.to_string()
                    } else if k.starts_with("Elixir.") {
                        let base = rest.trim_start_matches("ex/");
                        let imported = include_str!("../../tools/imports.txt").lines().find_map(|l| {
                            let mut it = l.split_whitespace();
                            (it.next() == Some(base)).then(|| it.next()).flatten()
                        });
                        match imported {
                            Some(rel) => format!("lib/{}", rel.trim_start_matches("../ex_unit/")),
                            None => format!("lib/{}", base),
                        }
                    } else {
                        format!("{}.erl", k)
                    }
                }
                None => m.file.clone(),
            };
            (k.clone(), f)
        })
        .collect()
}

fn cache_dir() -> PathBuf {
    if let Ok(d) = std::env::var("TONIC_CACHE") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cache").join("tonic")
}

/// The prelude compiled once (at -O2) per tonic build and cached:
/// returns the object path and the atom table order its code assumes.
fn prelude_object(
    o: &Opts,
    ex: &expand::Expander,
    defs: &[core::FunDef],
    per_program: &HashSet<String>,
) -> Result<(PathBuf, Vec<String>), String> {
    use std::hash::{Hash, Hasher};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&exe).map_err(|e| e.to_string())?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let key = format!("{:016x}", h.finish());
    let dir = cache_dir();
    let obj = dir.join(format!("prelude-{}.o", key));
    let atoms_file = dir.join(format!("prelude-{}.atoms", key));
    if obj.exists() && atoms_file.exists() {
        let text = std::fs::read_to_string(&atoms_file).map_err(|e| e.to_string())?;
        return Ok((obj, text.lines().map(|l| l.to_string()).collect()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if o.verbose {
        eprintln!("tonic: compiling the prelude (once per tonic build)...");
    }
    let t0 = std::time::Instant::now();
    let mut g = codegen::Gen::new(&[], beam_atoms::Assigner::new(&[]));
    g.mode = codegen::Mode::Prelude;
    g.externs = per_program.clone();
    g.files = module_files(ex);
    let ir = g.gen_program(defs);
    let atoms: Vec<String> = g.atom_names().to_vec();
    let tag = format!("{}.{}", std::process::id(), t0.elapsed().as_nanos());
    let ll = dir.join(format!("prelude-{}.{}.ll", key, tag));
    let tmp_obj = dir.join(format!("prelude-{}.{}.o", key, tag));
    std::fs::write(&ll, &ir).map_err(|e| e.to_string())?;
    let cc = std::env::var("TONIC_CC").unwrap_or_else(|_| "clang".into());
    let st = Command::new(&cc)
        .arg("-O2")
        .arg("-c")
        .arg("-Wno-override-module")
        .arg(&ll)
        .arg("-o")
        .arg(&tmp_obj)
        .status()
        .map_err(|e| format!("failed to run {}: {}", cc, e))?;
    let _ = std::fs::remove_file(&ll);
    if !st.success() {
        return Err("prelude compilation failed".into());
    }
    let tmp_atoms = dir.join(format!("prelude-{}.{}.atoms", key, tag));
    std::fs::write(&tmp_atoms, atoms.join("\n")).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_atoms, &atoms_file).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp_obj, &obj).map_err(|e| e.to_string())?;
    if o.verbose {
        eprintln!("tonic: prelude compiled in {:?}", t0.elapsed());
    }
    Ok((obj, atoms))
}
