//! LLVM IR generation.
//!
//! Every Elixir value that is live across a call lives in a frame slot. Each
//! function allocates a frame `[prev, n, function_info, line, slot0..slotN]` on the native stack and
//! links it into `ctx->frame`; the garbage collector walks this chain to find
//! and update roots, which is what lets it move objects. Immediates (atoms,
//! small integers, booleans) may stay in SSA registers since the GC never
//! needs to update them.
//!
//! Calling convention: `tailcc i64 f(ptr ctx, i64 args...)`. A return value
//! of 0 means an exception is pending in `ctx`; callers propagate it to the
//! innermost `try` handler or return 0 themselves. Calls in tail position are
//! `tail call tailcc`, which LLVM guarantees to turn into jumps.

use crate::bifs;
use crate::core::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::rc::Rc;
use tonic_shared::enc;

const TRUE_A: u64 = enc::atom(1);
const FALSE_A: u64 = enc::atom(2);
const NIL_A: u64 = enc::atom(0);
const MAX_TRAMPOLINE_ARITY: usize = 24;

#[derive(Clone, Debug)]
enum Op {
    Imm(u64),
    Slot(u32),
    Global(String),
    /// Immediate-only SSA value (never a pointer), safe across GC.
    Ssa(String),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    /// One object with everything (no precompiled prelude).
    Whole,
    /// The precompiled prelude object.
    Prelude,
    /// A program linked against the precompiled prelude.
    Program,
}

pub struct Gen {
    pub mode: Mode,
    script_file_static: Option<String>,
    /// Functions defined in the other object (prelude <-> program).
    pub externs: HashSet<String>,
    ext_used: std::collections::BTreeMap<String, usize>,
    /// (module, lambda id) -> per-module anonymous function index
    lambda_index: HashMap<(String, usize), usize>,
    lambda_count: HashMap<String, usize>,
    atoms: Vec<String>,
    beam: crate::beam_atoms::Assigner,
    /// module -> source file (for stack traces)
    pub files: HashMap<String, String>,
    atom_idx: HashMap<String, u64>,
    statics: String,
    static_keys: HashMap<String, String>,
    static_counter: usize,
    decls: HashMap<String, usize>,
    funcs: String,
    lambda_queue: Vec<(Rc<Lambda>, Vec<VarId>)>,
    emitted_lambdas: HashSet<usize>,
    fun_syms: HashSet<String>,
    ref_wrappers: HashMap<String, String>,
    pub fun_table: Vec<(String, String, usize, String)>,
    max_arity: usize,
}

fn q(name: &str) -> String {
    let mut s = String::from("\"");
    for b in name.bytes() {
        if b == b'"' || b == b'\\' || !(0x20..0x7f).contains(&b) {
            let _ = write!(s, "\\{:02X}", b);
        } else {
            s.push(b as char);
        }
    }
    s.push('"');
    s
}

fn cbytes(b: &[u8]) -> String {
    let mut s = String::from("c\"");
    for &x in b {
        if x == b'"' || x == b'\\' || !(0x20..0x7f).contains(&x) {
            let _ = write!(s, "\\{:02X}", x);
        } else {
            s.push(x as char);
        }
    }
    s.push('"');
    s
}

fn small(i: i64) -> u64 {
    enc::small(i)
}

/// Decimal string → (negative, u64 limbs little endian)
fn big_limbs(s: &str) -> (bool, Vec<u64>) {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    let mut limbs: Vec<u64> = vec![0];
    for c in digits.bytes() {
        let d = (c - b'0') as u128;
        let mut carry = d;
        for l in limbs.iter_mut() {
            let v = (*l as u128) * 10 + carry;
            *l = v as u64;
            carry = v >> 64;
        }
        if carry > 0 {
            limbs.push(carry as u64);
        }
    }
    while limbs.len() > 1 && *limbs.last().unwrap() == 0 {
        limbs.pop();
    }
    (neg, limbs)
}

impl Gen {
    pub fn new(initial_atoms: &[String], beam: crate::beam_atoms::Assigner) -> Gen {
        let mut g = Gen {
            mode: Mode::Whole,
            script_file_static: None,
            externs: HashSet::new(),
            ext_used: std::collections::BTreeMap::new(),
            beam,
            files: HashMap::new(),
            atoms: Vec::new(),
            atom_idx: HashMap::new(),
            statics: String::new(),
            static_keys: HashMap::new(),
            static_counter: 0,
            decls: HashMap::new(),
            funcs: String::new(),
            lambda_queue: Vec::new(),
            lambda_index: HashMap::new(),
            lambda_count: HashMap::new(),
            emitted_lambdas: HashSet::new(),
            fun_syms: HashSet::new(),
            ref_wrappers: HashMap::new(),
            fun_table: Vec::new(),
            max_arity: 8,
        };
        for a in tonic_shared::FIXED_ATOMS {
            g.atom(a);
        }
        for a in initial_atoms {
            g.atom(a);
        }
        g
    }

    /// Atoms in table order.
    pub fn atom_names(&self) -> &[String] {
        &self.atoms
    }

    /// Whether `sym` is defined here or in the other object; records uses
    /// of external symbols so they get declared.
    fn known_fun(&mut self, sym: &str, arity: usize) -> bool {
        if self.fun_syms.contains(sym) {
            return true;
        }
        if self.externs.contains(sym) {
            self.ext_used.insert(sym.to_string(), arity);
            return true;
        }
        false
    }

    pub fn atom(&mut self, a: &str) -> u64 {
        if let Some(&i) = self.atom_idx.get(a) {
            return enc::atom(i);
        }
        let i = self.atoms.len() as u64;
        self.atoms.push(a.to_string());
        self.atom_idx.insert(a.to_string(), i);
        enc::atom(i)
    }

    fn new_static(&mut self, key: String, ty: &str, init: &str) -> String {
        if let Some(n) = self.static_keys.get(&key) {
            return n.clone();
        }
        self.static_counter += 1;
        let name = format!("lit.{}", self.static_counter);
        let _ = writeln!(self.statics, "@{} = private unnamed_addr constant {} {}, align 8", q(&name), ty, init);
        self.static_keys.insert(key, name.clone());
        name
    }

    /// Static frame info `{module, name, arity, file}` for stack traces.
    fn frame_info(&mut self, module: &str, name: &str, arity: usize) -> String {
        let file = self.files.get(module).cloned().unwrap_or_default();
        // Script top level looks like Elixir's `:elixir_compiler_N.__FILE__/1`.
        let (module, name, arity) = if module == "Elixir.Tonic.Script" {
            let n = if name == "__script__" { "__FILE__".to_string() } else { name.replace("__script__/0", "__FILE__/1") };
            ("elixir_compiler_1", n, if name == "__script__" { 1 } else { arity })
        } else {
            (module, name.to_string(), arity)
        };
        let name = name.as_str();
        let mut fb = file.into_bytes();
        fb.push(0);
        let fname = self.new_static(format!("file:{}", String::from_utf8_lossy(&fb)), &format!("[{} x i8]", fb.len()), &cbytes(&fb));
        let m = self.atom(module);
        let n = self.atom(name);
        self.new_static(
            format!("finfo:{}:{}:{}", module, name, arity),
            "{ i64, i64, i64, ptr }",
            &format!("{{ i64 {}, i64 {}, i64 {}, ptr @{} }}", m, n, arity, q(&fname)),
        )
    }

    fn static_bin(&mut self, b: &[u8]) -> String {
        let padded = (b.len() + 7) / 8 * 8;
        let mut bytes = b.to_vec();
        bytes.resize(padded, 0);
        let hdr = enc::header(enc::T_BINARY, 1 + (padded / 8) as u64);
        let ty = format!("<{{ i64, i64, [{} x i8] }}>", padded);
        let init = format!("<{{ i64 {}, i64 {}, [{} x i8] {} }}>", hdr, b.len(), padded, cbytes(&bytes));
        let key = format!("bin:{:?}", b);
        self.new_static(key, &ty, &init)
    }

    fn static_float(&mut self, f: f64) -> String {
        let key = format!("float:{}", f.to_bits());
        let hdr = enc::header(enc::T_FLOAT, 1);
        self.new_static(key, "{ i64, i64 }", &format!("{{ i64 {}, i64 {} }}", hdr, f.to_bits() as i64))
    }

    fn static_big(&mut self, s: &str) -> String {
        let (neg, limbs) = big_limbs(s);
        let hdr = enc::header(enc::T_BIGINT, 1 + limbs.len() as u64);
        let mut ty = String::from("{ i64, i64");
        let mut init = format!("{{ i64 {}, i64 {}", hdr, neg as u64);
        for l in &limbs {
            ty.push_str(", i64");
            let _ = write!(init, ", i64 {}", *l as i64);
        }
        ty.push_str(" }");
        init.push_str(" }");
        self.new_static(format!("big:{}", s), &ty, &init)
    }

    fn op_field(&self, op: &Op) -> (String, String) {
        match op {
            Op::Imm(v) => ("i64".into(), format!("i64 {}", *v as i64)),
            Op::Global(n) => ("ptr".into(), format!("ptr @{}", q(n))),
            _ => panic!("non-static op in static aggregate"),
        }
    }

    fn static_agg(&mut self, hdr: u64, fields: &[Op], extra_pad: bool) -> String {
        let mut ty = String::from("{ i64");
        let mut init = format!("{{ i64 {}", hdr);
        for f in fields {
            let (t, v) = self.op_field(f);
            ty.push_str(", ");
            ty.push_str(&t);
            init.push_str(", ");
            init.push_str(&v);
        }
        if extra_pad {
            ty.push_str(", i64");
            init.push_str(", i64 0");
        }
        ty.push_str(" }");
        init.push_str(" }");
        let key = format!("agg:{}", init);
        self.new_static(key, &ty, &init)
    }

    fn static_empty_map(&mut self) -> String {
        let hdr = enc::header(enc::T_MAP, 2);
        self.new_static("emptymap".into(), "{ i64, i64, i64 }", &format!("{{ i64 {}, i64 0, i64 0 }}", hdr))
    }

    fn fun_info(&mut self, kind: u64, module: &str, name: &str, arity: usize, index: usize) -> String {
        let m = self.atom(module);
        let n = self.atom(name);
        let key = format!("info:{}:{}:{}:{}:{}", kind, module, name, arity, index);
        self.new_static(
            key,
            "{ i64, i64, i64, i64, i64 }",
            &format!("{{ i64 {}, i64 {}, i64 {}, i64 {}, i64 {} }}", kind, m, n, arity, index),
        )
    }

    fn static_closure(&mut self, fsym: &str, arity: usize, info: &str) -> String {
        let hdr = enc::header(enc::T_CLOSURE, 3);
        let key = format!("clo:{}", fsym);
        self.new_static(
            key,
            "{ i64, ptr, i64, ptr }",
            &format!("{{ i64 {}, ptr @{}, i64 {}, ptr @{} }}", hdr, q(fsym), arity, q(info)),
        )
    }

    fn declare(&mut self, sym: &str, nargs: usize) {
        self.decls.entry(sym.to_string()).or_insert(nargs);
    }

    // ------------------------------------------------------------------

    pub fn gen_program(&mut self, defs: &[FunDef]) -> String {
        for d in defs {
            self.fun_syms.insert(d.key.symbol());
            self.max_arity = self.max_arity.max(d.params.len());
        }
        for d in defs {
            let fi = self.frame_info(&d.key.module, &d.key.name, d.key.arity);
            let mut f = FnGen::new(self, &d.key.symbol(), &d.key.module, &format!("{}/{}", d.key.name, d.key.arity));
            f.finfo = fi;
            f.gen_named(d);
            let text = f.finish();
            self.funcs.push_str(&text);
            if d.public {
                self.fun_table.push((d.key.module.clone(), d.key.name.clone(), d.key.arity, d.key.symbol()));
            }
            self.flush_lambdas();
        }
        if self.mode == Mode::Program {
            let sf = self.files.get("Elixir.Tonic.Script").cloned().unwrap_or_default();
            let mut fb = sf.into_bytes();
            fb.push(0);
            let g = self.new_static("script_file".into(), &format!("[{} x i8]", fb.len()), &cbytes(&fb));
            self.script_file_static = Some(g);
        }
        self.assemble()
    }

    fn flush_lambdas(&mut self) {
        while let Some((lam, free)) = self.lambda_queue.pop() {
            if !self.emitted_lambdas.insert(lam.id) {
                continue;
            }
            let sym = lambda_sym(&lam);
            let fi = self.frame_info(&lam.module, &format!("-{}-fun-0-", lam.parent), lam.arity);
            let mut f = FnGen::new(self, &sym, &lam.module, &lam.parent);
            f.finfo = fi;
            f.gen_lambda(&lam, &free);
            let text = f.finish();
            self.funcs.push_str(&text);
        }
    }

    fn assemble(&mut self) -> String {
        let mut out = String::new();
        out.push_str("; generated by tonic\n\n");
        out.push_str(RUNTIME_DECLS);
        out.push_str(INLINE_HELPERS);
        // Fun table
        let mut entries = Vec::new();
        let ft = std::mem::take(&mut self.fun_table);
        for (m, n, a, sym) in &ft {
            let ma = self.atom(m);
            let na = self.atom(n);
            entries.push(format!("{{ i64, i64, i64, ptr }} {{ i64 {}, i64 {}, i64 {}, ptr @{} }}", ma, na, a, q(sym)));
        }
        self.fun_table = ft;
        let funs_name = if self.mode == Mode::Prelude { "tonic_prelude_funs" } else { "tonic_funs" };
        let _ = writeln!(
            out,
            "@{} = {}constant [{} x {{ i64, i64, i64, ptr }}] [{}], align 8",
            funs_name,
            if self.mode == Mode::Prelude { "" } else { "private " },
            entries.len(),
            entries.join(", ")
        );
        if self.mode == Mode::Prelude {
            let _ = writeln!(out, "@tonic_prelude_nfuns = constant i64 {}, align 8", entries.len());
        }
        if self.mode == Mode::Program {
            out.push_str("@tonic_prelude_funs = external constant [0 x { i64, i64, i64, ptr }], align 8\n");
            out.push_str("@tonic_prelude_nfuns = external constant i64, align 8\n");
        }
        // Trampolines (need max arity)
        let tramp = if self.mode == Mode::Prelude { String::new() } else { self.trampolines() };
        if self.mode == Mode::Prelude {
            out.push_str(&self.statics);
            out.push('\n');
            self.emit_decls(&mut out);
            out.push_str(&self.funcs);
            return out;
        }
        // Atoms
        let mut atom_refs = Vec::new();
        for (i, a) in self.atoms.iter().enumerate() {
            let mut b = a.as_bytes().to_vec();
            b.push(0);
            let _ = writeln!(out, "@atom.{} = private unnamed_addr constant [{} x i8] {}", i, b.len(), cbytes(&b));
            atom_refs.push(format!("ptr @atom.{}", i));
        }
        let _ = writeln!(
            out,
            "@tonic_atoms = private constant [{} x ptr] [{}]",
            atom_refs.len(),
            atom_refs.join(", ")
        );
        let beam: Vec<String> = {
            let names = self.atoms.clone();
            names.iter().map(|a| format!("i32 {}", self.beam.index(a))).collect()
        };
        let _ = writeln!(
            out,
            "@tonic_atom_beam = private constant [{} x i32] [{}]",
            beam.len(),
            beam.join(", ")
        );
        out.push_str(&self.statics);
        out.push('\n');
        self.emit_decls(&mut out);
        out.push('\n');
        out.push_str(&tramp);
        out.push_str(&self.funcs);
        let _ = writeln!(
            out,
            "define i64 @tonic_main_entry(ptr %ctx) {{\n  %r = call tailcc i64 @{}(ptr %ctx)\n  ret i64 %r\n}}\n",
            q("Elixir.Tonic.Script.__script__/0")
        );
        let (pf, np) = if self.mode == Mode::Program {
            out.push_str("define internal i64 @tonic_prelude_n() {\n  %n = load i64, ptr @tonic_prelude_nfuns\n  ret i64 %n\n}\n");
            ("ptr @tonic_prelude_funs", "%pn")
        } else {
            ("ptr null", "0")
        };
        let pre = if self.mode == Mode::Program { "  %pn = call i64 @tonic_prelude_n()\n" } else { "" };
        let pre = format!("{}  call void @tn_set_ext_tramps(ptr @tonic_ext_tramps, i64 {})\n", pre, EXT_FUN_MAX_ARITY + 1);
        let pre = match self.script_file_static.clone() {
            Some(g) => {
                out.push_str("declare void @tn_set_script_file(ptr)\n");
                format!("{}  call void @tn_set_script_file(ptr @{})\n", pre, q(&g))
            }
            None => pre,
        };
        let _ = writeln!(
            out,
            "define i32 @main(i32 %argc, ptr %argv) {{\n{}  %r = call i32 @tonic_start(i32 %argc, ptr %argv, ptr @tonic_atoms, i64 {}, ptr @tonic_atom_beam, ptr @tonic_funs, i64 {}, ptr @tonic_main_entry, {}, i64 {})\n  ret i32 %r\n}}",
            pre,
            self.atoms.len(),
            self.fun_table.len(),
            pf,
            np
        );
        out
    }

    fn emit_decls(&self, out: &mut String) {
        let predeclared: HashSet<&str> = RUNTIME_DECLS
            .lines()
            .filter_map(|l| l.strip_prefix("declare ").and_then(|r| r.split('@').nth(1)).and_then(|r| r.split('(').next()))
            .collect();
        let mut decls: Vec<(&String, &usize)> = self.decls.iter().collect();
        decls.sort();
        for (sym, n) in decls {
            if predeclared.contains(sym.as_str()) {
                continue;
            }
            let mut params = vec!["ptr".to_string()];
            for _ in 0..*n {
                params.push("i64".into());
            }
            let _ = writeln!(out, "declare i64 @{}({})", sym, params.join(", "));
        }
        for (sym, n) in &self.ext_used {
            let mut params = vec!["ptr".to_string()];
            for _ in 0..*n {
                params.push("i64".into());
            }
            let _ = writeln!(out, "declare tailcc i64 @{}({})", q(sym), params.join(", "));
        }
    }

    fn trampolines(&self) -> String {
        let n = self.max_arity.max(MAX_TRAMPOLINE_ARITY);
        let mut s = String::new();
        // Entry points of dynamically created external funs (`:erlang.make_fun/3`):
        // closure = [hdr, tramp, arity, info, module, name].
        let mut tr = Vec::new();
        for a in 0..=EXT_FUN_MAX_ARITY {
            let mut params = vec!["ptr %ctx".to_string(), "i64 %env".to_string()];
            let mut args = vec!["ptr %ctx".to_string()];
            for i in 0..a {
                params.push(format!("i64 %a{}", i));
                args.push(format!("i64 %a{}", i));
            }
            let _ = writeln!(s, "define private tailcc i64 @tonic_ext_tramp_{}({}) {{", a, params.join(", "));
            s.push_str("  %p = inttoptr i64 %env to ptr\n  %mp = getelementptr i64, ptr %p, i64 4\n  %m = load i64, ptr %mp\n  %np = getelementptr i64, ptr %p, i64 5\n  %n = load i64, ptr %np\n");
            let _ = writeln!(s, "  %fp = call ptr @tn_lookup_mfa_q(ptr %ctx, i64 %m, i64 %n, i64 {})", a);
            s.push_str("  %z = icmp eq ptr %fp, null\n  br i1 %z, label %bad, label %ok\nbad:\n");
            let _ = writeln!(s, "  %arr = alloca [{} x i64]", a.max(1));
            for i in 0..a {
                let _ = writeln!(s, "  %e{} = getelementptr i64, ptr %arr, i64 {}\n  store i64 %a{}, ptr %e{}", i, i, i, i);
            }
            let _ = writeln!(s, "  call void @tn_undef_raise(ptr %ctx, i64 %m, i64 %n, ptr %arr, i64 {})", a);
            s.push_str("  ret i64 0\nok:\n");
            let _ = writeln!(s, "  %r = tail call tailcc i64 %fp({})\n  ret i64 %r\n}}\n", args.join(", "));
            tr.push(format!("ptr @tonic_ext_tramp_{}", a));
        }
        let _ = writeln!(s, "@tonic_ext_tramps = private constant [{} x ptr] [{}]\n", tr.len(), tr.join(", "));
        for (name, has_clo) in [("tonic_call_closure", true), ("tonic_call_fnptr", false)] {
            if has_clo {
                let _ = writeln!(s, "define i64 @{}(ptr %ctx, i64 %clo, i64 %n, ptr %args) {{", name);
                s.push_str("entry:\n  %p = inttoptr i64 %clo to ptr\n  %fpp = getelementptr i64, ptr %p, i64 1\n  %fp = load ptr, ptr %fpp\n");
            } else {
                let _ = writeln!(s, "define i64 @{}(ptr %ctx, ptr %fp, i64 %n, ptr %args) {{", name);
                s.push_str("entry:\n");
            }
            s.push_str("  switch i64 %n, label %bad [");
            for i in 0..=n {
                let _ = write!(s, " i64 {}, label %c{}", i, i);
            }
            s.push_str(" ]\n");
            for i in 0..=n {
                let _ = writeln!(s, "c{}:", i);
                let mut args = vec!["ptr %ctx".to_string()];
                if has_clo {
                    args.push("i64 %clo".into());
                }
                for j in 0..i {
                    let _ = writeln!(s, "  %a{}_{}p = getelementptr i64, ptr %args, i64 {}", i, j, j);
                    let _ = writeln!(s, "  %a{}_{} = load i64, ptr %a{}_{}p", i, j, i, j);
                    args.push(format!("i64 %a{}_{}", i, j));
                }
                let _ = writeln!(s, "  %r{} = call tailcc i64 %fp({})", i, args.join(", "));
                let _ = writeln!(s, "  ret i64 %r{}", i);
            }
            s.push_str("bad:\n  ret i64 0\n}\n\n");
        }
        s
    }

    /// Wrapper function used by `&Mod.fun/arity` closures.
    fn ref_wrapper(&mut self, key: &FunKey, target: &Target) -> String {
        let wsym = format!("{}$ref", key.symbol());
        if let Some(s) = self.ref_wrappers.get(&wsym) {
            return s.clone();
        }
        self.ref_wrappers.insert(wsym.clone(), wsym.clone());
        let mut s = String::new();
        let mut params = vec!["ptr %ctx".to_string(), "i64 %env".to_string()];
        let mut args = vec!["ptr %ctx".to_string()];
        for i in 0..key.arity {
            params.push(format!("i64 %a{}", i));
            args.push(format!("i64 %a{}", i));
        }
        let _ = writeln!(s, "define private tailcc i64 @{}({}) {{", q(&wsym), params.join(", "));
        match target {
            Target::Fun(k) => {
                let _ = self.known_fun(&k.symbol(), k.arity);
                let _ = writeln!(s, "  %r = tail call tailcc i64 @{}({})", q(&k.symbol()), args.join(", "));
                s.push_str("  ret i64 %r\n}\n\n");
            }
            Target::Bif(sym) => {
                self.declare(sym, key.arity);
                let _ = writeln!(s, "  %r = call i64 @{}({})", sym, args.join(", "));
                s.push_str("  ret i64 %r\n}\n\n");
            }
            Target::Dynamic => {
                let m = self.atom(&key.module);
                let n = self.atom(&key.name);
                let _ = writeln!(s, "  %fp = call ptr @tn_lookup_mfa(ptr %ctx, i64 {}, i64 {}, i64 {})", m, n, key.arity);
                s.push_str("  %z = icmp eq ptr %fp, null\n  br i1 %z, label %bad, label %ok\nbad:\n  ret i64 0\nok:\n");
                let _ = writeln!(s, "  %r = tail call tailcc i64 %fp({})", args.join(", "));
                s.push_str("  ret i64 %r\n}\n\n");
            }
        }
        self.funcs.push_str(&s);
        wsym
    }
}

fn lambda_sym(l: &Lambda) -> String {
    format!("{}.{}$fn{}", l.module, l.parent, l.id)
}

// ---------------------------------------------------------------------------
// Free variables
// ---------------------------------------------------------------------------

#[derive(Default)]
struct VarSets {
    used: Vec<VarId>,
    bound: HashSet<VarId>,
    binds: HashMap<VarId, u32>,
}

impl VarSets {
    fn use_(&mut self, v: VarId) {
        if !self.used.contains(&v) {
            self.used.push(v);
        }
    }
}

fn fv_expr(e: &CE, s: &mut VarSets) {
    match e {
        CE::Lit(_) => {}
        CE::Var(v) => s.use_(*v),
        CE::Tuple(v) | CE::Block(v) | CE::Interp(v) | CE::Bif(_, v) => v.iter().for_each(|x| fv_expr(x, s)),
        CE::Cons(a, b) => {
            fv_expr(a, s);
            fv_expr(b, s);
        }
        CE::Map(ps) | CE::Struct(_, ps) => ps.iter().for_each(|(a, b)| {
            fv_expr(a, s);
            fv_expr(b, s);
        }),
        CE::MapUpdate(b, ps, _) => {
            fv_expr(b, s);
            ps.iter().for_each(|(a, b)| {
                fv_expr(a, s);
                fv_expr(b, s);
            });
        }
        CE::Bin(segs) => segs.iter().for_each(|sg| {
            fv_expr(&sg.val, s);
            if let Some(z) = &sg.size {
                fv_expr(z, s);
            }
        }),
        CE::Call(_, args) => args.iter().for_each(|x| fv_expr(x, s)),
        CE::DynCall(m, _, args) => {
            fv_expr(m, s);
            args.iter().for_each(|x| fv_expr(x, s));
        }
        CE::Apply(f, args) => {
            fv_expr(f, s);
            args.iter().for_each(|x| fv_expr(x, s));
        }
        CE::Fn(l) => {
            for v in lambda_free(l) {
                s.use_(v);
            }
        }
        CE::FunRef(..) => {}
        CE::Match(p, e) => {
            fv_expr(e, s);
            fv_pat(p, s);
        }
        CE::Case(subs, cs, _) => {
            subs.iter().for_each(|x| fv_expr(x, s));
            cs.iter().for_each(|c| fv_clause(c, s));
        }
        CE::If(a, b, c) => {
            fv_expr(a, s);
            fv_expr(b, s);
            fv_expr(c, s);
        }
        CE::Receive(cs, after) => {
            cs.iter().for_each(|c| fv_clause(c, s));
            if let Some((t, b)) = after {
                fv_expr(t, s);
                fv_expr(b, s);
            }
        }
        CE::Try(t) => {
            fv_expr(&t.body, s);
            t.catches.iter().chain(t.else_clauses.iter()).for_each(|c| fv_clause(c, s));
            if let Some(a) = &t.after {
                fv_expr(a, s);
            }
        }
    }
}

fn fv_clause(c: &Clause, s: &mut VarSets) {
    c.pats.iter().for_each(|p| fv_pat(p, s));
    if let Some(g) = &c.guard {
        fv_expr(g, s);
    }
    fv_expr(&c.body, s);
}

fn fv_pat(p: &Pat, s: &mut VarSets) {
    match p {
        Pat::Wild | Pat::Lit(_) => {}
        Pat::Bind(v) => {
            s.bound.insert(*v);
            *s.binds.entry(*v).or_insert(0) += 1;
        }
        Pat::Eq(v) => s.use_(*v),
        Pat::Tuple(ps) => ps.iter().for_each(|x| fv_pat(x, s)),
        Pat::Cons(a, b) | Pat::Alias(a, b) => {
            fv_pat(a, s);
            fv_pat(b, s);
        }
        Pat::Atom(a) => fv_pat(a, s),
        Pat::Map(ps) => ps.iter().for_each(|(k, v)| {
            fv_expr(k, s);
            fv_pat(v, s);
        }),
        Pat::Bin(segs) => segs.iter().for_each(|sg| {
            fv_pat(&sg.val, s);
            if let Some(z) = &sg.size {
                fv_expr(z, s);
            }
        }),
    }
}

fn lambda_free(l: &Lambda) -> Vec<VarId> {
    let mut s = VarSets::default();
    for p in &l.params {
        s.bound.insert(*p);
    }
    for c in &l.clauses {
        fv_clause(c, &mut s);
    }
    s.used.into_iter().filter(|v| !s.bound.contains(v)).collect()
}

// ---------------------------------------------------------------------------
// Per-function generation
// ---------------------------------------------------------------------------

struct FnGen<'a> {
    g: &'a mut Gen,
    sym: String,
    module: String,
    parent: String,
    code: String,
    nslots: u32,
    tmp: u32,
    lbl: u32,
    vars: HashMap<VarId, u32>,
    exc: String,
    guard_fail: Option<String>,
    terminated: bool,
    header: String,
    param_slots: Vec<u32>,
    /// Number of pattern bindings of each variable in this function.
    bind_counts: HashMap<VarId, u32>,
    /// static frame info global for this function
    finfo: String,
    /// source line of the call being generated (innermost `__at`)
    line_stack: Vec<u32>,
    /// line known to be stored in the frame on the current path
    stored_line: Option<u32>,
}

enum Flow {
    Val(Op),
    Done,
}

impl<'a> FnGen<'a> {
    fn new(g: &'a mut Gen, sym: &str, module: &str, parent: &str) -> FnGen<'a> {
        FnGen {
            g,
            sym: sym.to_string(),
            module: module.to_string(),
            parent: parent.to_string(),
            code: String::new(),
            nslots: 0,
            tmp: 0,
            lbl: 0,
            vars: HashMap::new(),
            exc: "unwind".into(),
            guard_fail: None,
            terminated: false,
            header: String::new(),
            param_slots: Vec::new(),
            bind_counts: HashMap::new(),
            finfo: String::new(),
            line_stack: Vec::new(),
            stored_line: None,
        }
    }

    fn t(&mut self) -> String {
        self.tmp += 1;
        format!("%t{}", self.tmp)
    }
    fn label(&mut self) -> String {
        self.lbl += 1;
        format!("L{}", self.lbl)
    }
    fn emit(&mut self, s: &str) {
        if self.terminated {
            // unreachable code after a terminator: open a dead block
            let l = self.label();
            let _ = writeln!(self.code, "{}:", l);
            self.terminated = false;
        }
        self.code.push_str("  ");
        self.code.push_str(s);
        self.code.push('\n');
    }
    fn term(&mut self, s: &str) {
        self.emit(s);
        self.terminated = true;
    }
    fn block(&mut self, l: &str) {
        if !self.terminated {
            let _ = writeln!(self.code, "  br label %{}", l);
        }
        let _ = writeln!(self.code, "{}:", l);
        self.terminated = false;
        self.stored_line = None;
    }
    fn br(&mut self, l: &str) {
        self.term(&format!("br label %{}", l));
    }
    fn new_slot(&mut self) -> u32 {
        self.nslots += 1;
        self.nslots - 1
    }
    fn slots(&mut self, n: usize) -> u32 {
        let base = self.nslots;
        self.nslots += n as u32;
        base
    }
    fn slot_ptr(&mut self, s: u32) -> String {
        let p = self.t();
        self.emit(&format!("{} = getelementptr i64, ptr %fr, i64 {}", p, s + 4));
        p
    }
    fn var_slot(&mut self, v: VarId) -> u32 {
        if let Some(s) = self.vars.get(&v) {
            return *s;
        }
        let s = self.new_slot();
        self.vars.insert(v, s);
        s
    }
    fn load(&mut self, op: &Op) -> String {
        match op {
            Op::Imm(v) => format!("{}", *v as i64),
            Op::Ssa(s) => s.clone(),
            Op::Slot(s) => {
                let p = self.slot_ptr(*s);
                let t = self.t();
                self.emit(&format!("{} = load i64, ptr {}", t, p));
                t
            }
            Op::Global(n) => {
                let t = self.t();
                self.emit(&format!("{} = ptrtoint ptr @{} to i64", t, q(n)));
                t
            }
        }
    }
    fn store(&mut self, slot: u32, v: &str) {
        let p = self.slot_ptr(slot);
        self.emit(&format!("store i64 {}, ptr {}", v, p));
    }
    fn to_slot(&mut self, op: Op) -> Op {
        match op {
            Op::Slot(_) => op,
            other => {
                let v = self.load(&other);
                let s = self.new_slot();
                self.store(s, &v);
                Op::Slot(s)
            }
        }
    }
    /// Store an operand into a specific slot.
    fn put(&mut self, slot: u32, op: &Op) {
        let v = self.load(op);
        self.store(slot, &v);
    }

    /// After a call that may return 0: branch to the exception target.
    fn check(&mut self, r: &str) {
        let c = self.t();
        let ok = self.label();
        self.emit(&format!("{} = icmp eq i64 {}, 0", c, r));
        let target = match &self.guard_fail {
            Some(g) => g.clone(),
            None => self.exc.clone(),
        };
        self.term(&format!("br i1 {}, label %{}, label %{}", c, target, ok));
        // The ok block has a single predecessor: the stored line is still valid.
        let keep = self.stored_line;
        self.block(&ok);
        self.stored_line = keep;
    }

    fn ret(&mut self, v: &str) {
        self.emit("store ptr %prev, ptr %ctx");
        self.term(&format!("ret i64 {}", v));
    }

    fn finish(self) -> String {
        let n = self.nslots.max(1);
        let mut s = String::new();
        s.push_str(&self.header);
        let _ = writeln!(s, "entry:");
        let _ = writeln!(s, "  %fr = alloca i64, i64 {}, align 16", n + 4);
        let _ = writeln!(s, "  %prev = load ptr, ptr %ctx");
        let _ = writeln!(s, "  store ptr %prev, ptr %fr");
        let _ = writeln!(s, "  %fr.n = getelementptr i64, ptr %fr, i64 1");
        let _ = writeln!(s, "  store i64 {}, ptr %fr.n", n);
        let _ = writeln!(s, "  %fr.i = getelementptr i64, ptr %fr, i64 2");
        if self.finfo.is_empty() {
            let _ = writeln!(s, "  store i64 0, ptr %fr.i");
        } else {
            let _ = writeln!(s, "  store ptr @{}, ptr %fr.i", q(&self.finfo));
        }
        let _ = writeln!(s, "  %fr.l = getelementptr i64, ptr %fr, i64 3");
        let _ = writeln!(s, "  store i64 0, ptr %fr.l");
        let _ = writeln!(s, "  %fr.s = getelementptr i64, ptr %fr, i64 4");
        let _ = writeln!(s, "  call void @llvm.memset.p0.i64(ptr align 8 %fr.s, i8 0, i64 {}, i1 false)", n * 8);
        let _ = writeln!(s, "  store ptr %fr, ptr %ctx");
        s.push_str(&self.code);
        if !self.terminated {
            s.push_str("  unreachable\n");
        }
        s.push_str("unwind:\n  store ptr %prev, ptr %ctx\n  ret i64 0\n}\n\n");
        s
    }

    fn prologue_reductions(&mut self) {
        self.emit("%rp = getelementptr i8, ptr %ctx, i64 24");
        self.emit("%rd = load i64, ptr %rp");
        self.emit("%rd2 = sub i64 %rd, 1");
        self.emit("store i64 %rd2, ptr %rp");
        self.emit("%rdy = icmp slt i64 %rd2, 0");
        self.term("br i1 %rdy, label %yield, label %body");
        self.block("yield");
        self.emit("%yr = call i64 @tn_yield(ptr %ctx)");
        self.emit("%yz = icmp eq i64 %yr, 0");
        self.term("br i1 %yz, label %unwind, label %body");
        self.block("body");
    }

    fn gen_named(&mut self, d: &FunDef) {
        let mut params = vec!["ptr %ctx".to_string()];
        for i in 0..d.params.len() {
            params.push(format!("i64 %a{}", i));
        }
        self.header = format!("define tailcc i64 @{}({}) {{\n", q(&self.sym), params.join(", "));
        // params in contiguous slots
        let base = self.slots(d.params.len());
        for (i, p) in d.params.iter().enumerate() {
            self.vars.insert(*p, base + i as u32);
            self.param_slots.push(base + i as u32);
            self.store(base + i as u32, &format!("%a{}", i));
        }
        let mut vs = VarSets::default();
        fv_expr(&d.body, &mut vs);
        self.bind_counts = vs.binds;
        self.prologue_reductions();
        // The script body keeps its frame (like erl_eval) so stack traces
        // end in `(file)`.
        let tail = !(d.key.module == "Elixir.Tonic.Script" && d.key.name == "__script__");
        let r = self.expr(&d.body, tail);
        if let Flow::Val(op) = r {
            let v = self.load(&op);
            self.ret(&v);
        }
    }

    fn gen_lambda(&mut self, l: &Lambda, free: &[VarId]) {
        let mut params = vec!["ptr %ctx".to_string(), "i64 %env".to_string()];
        for i in 0..l.arity {
            params.push(format!("i64 %a{}", i));
        }
        self.header = format!("define private tailcc i64 @{}({}) {{\n", q(&self.sym), params.join(", "));
        let base = self.slots(l.arity);
        for (i, p) in l.params.iter().enumerate() {
            self.vars.insert(*p, base + i as u32);
            self.param_slots.push(base + i as u32);
            self.store(base + i as u32, &format!("%a{}", i));
        }
        if !free.is_empty() {
            self.emit("%envp = inttoptr i64 %env to ptr");
            for (i, v) in free.iter().enumerate() {
                let p = self.t();
                self.emit(&format!("{} = getelementptr i64, ptr %envp, i64 {}", p, 4 + i));
                let x = self.t();
                self.emit(&format!("{} = load i64, ptr {}", x, p));
                let s = self.var_slot(*v);
                self.store(s, &x);
            }
        }
        let mut vs = VarSets::default();
        for c in &l.clauses {
            fv_clause(c, &mut vs);
        }
        self.bind_counts = vs.binds;
        self.prologue_reductions();
        let subjects: Vec<CE> = l.params.iter().map(|p| CE::Var(*p)).collect();
        let body = CE::Case(
            subjects,
            l.clauses.clone(),
            {
                let (fmod, fname, _) = self.lambda_erl_name(l);
                Fail::FunctionClause { module: fmod, name: fname, line: l.line }
            },
        );
        let r = self.expr(&body, true);
        if let Flow::Val(op) = r {
            let v = self.load(&op);
            self.ret(&v);
        }
    }

    fn parent_name(&self) -> String {
        self.parent.split('/').next().unwrap_or("").to_string()
    }

    // ------------------------------------------------------------------
    // expressions
    // ------------------------------------------------------------------

    fn val(&mut self, e: &CE) -> Option<Op> {
        match self.expr(e, false) {
            Flow::Val(o) => Some(o),
            Flow::Done => None,
        }
    }

    /// Return Flow::Val(op) or emit a return if `tail`.
    fn finish_val(&mut self, op: Op, tail: bool) -> Flow {
        if tail {
            let v = self.load(&op);
            self.ret(&v);
            Flow::Done
        } else {
            Flow::Val(op)
        }
    }

    fn static_of(&mut self, e: &CE) -> Option<Op> {
        match e {
            CE::Lit(l) => Some(self.lit(l)),
            CE::Tuple(items) => {
                let mut ops = Vec::new();
                for it in items {
                    ops.push(self.static_of(it)?);
                }
                if ops.is_empty() {
                    return Some(Op::Global(self.g.static_agg(enc::header(enc::T_TUPLE, 0), &[], true)));
                }
                Some(Op::Global(self.g.static_agg(enc::header(enc::T_TUPLE, ops.len() as u64), &ops, false)))
            }
            CE::Cons(h, t) => {
                let ho = self.static_of(h)?;
                let to = self.static_of(t)?;
                Some(Op::Global(self.g.static_agg(enc::header(enc::T_CONS, 2), &[ho, to], false)))
            }
            CE::Map(ps) if ps.is_empty() => Some(Op::Global(self.g.static_empty_map())),
            CE::FunRef(k, t) => Some(self.funref(k, t)),
            _ => None,
        }
    }

    fn lit(&mut self, l: &Lit) -> Op {
        match l {
            Lit::Int(i) => Op::Imm(small(*i)),
            Lit::Big(s) => Op::Global(self.g.static_big(s)),
            Lit::Float(f) => Op::Global(self.g.static_float(*f)),
            Lit::Atom(a) => Op::Imm(self.g.atom(a)),
            Lit::Bin(b) => Op::Global(self.g.static_bin(b)),
            Lit::Nil => Op::Imm(enc::NIL_LIST),
        }
    }

    fn funref(&mut self, k: &FunKey, t: &Target) -> Op {
        let w = self.g.ref_wrapper(k, t);
        // Captures of functions Elixir inlines to Erlang show the Erlang
        // function (`&Kernel.is_atom/1` is `&:erlang.is_atom/1`).
        let (im, iname) = inline_target(&k.module, &k.name, k.arity)
            .map(|(m, f)| (m.to_string(), f.to_string()))
            .unwrap_or_else(|| (k.module.clone(), k.name.clone()));
        let info = self.g.fun_info(1, &im, &iname, k.arity, 0);
        Op::Global(self.g.static_closure(&w, k.arity, &info))
    }

    fn expr(&mut self, e: &CE, tail: bool) -> Flow {
        if let CE::Bif(s, a) = e {
            if s == "__at" && a.len() == 2 {
                let line = match &a[0] {
                    CE::Lit(Lit::Int(n)) => *n as u32,
                    _ => 0,
                };
                self.line_stack.push(line);
                let r = self.expr(&a[1], tail);
                self.line_stack.pop();
                return r;
            }
        }
        match e {
            CE::Lit(l) => {
                let o = self.lit(l);
                self.finish_val(o, tail)
            }
            CE::Var(v) => {
                let s = self.var_slot(*v);
                self.finish_val(Op::Slot(s), tail)
            }
            CE::Tuple(items) => {
                if let Some(o) = self.static_of(e) {
                    return self.finish_val(o, tail);
                }
                let mut ops = Vec::new();
                for it in items {
                    match self.val(it) {
                        Some(o) => ops.push(o),
                        None => return Flow::Done,
                    }
                }
                let o = self.alloc_obj(enc::header(enc::T_TUPLE, ops.len() as u64), &ops);
                self.finish_val(o, tail)
            }
            CE::Cons(..) => {
                if let Some(o) = self.static_of(e) {
                    return self.finish_val(o, tail);
                }
                let mut items = Vec::new();
                let mut cur = e;
                while let CE::Cons(h, t) = cur {
                    items.push(&**h);
                    cur = t;
                }
                let mut ops = Vec::new();
                for it in items {
                    match self.val(it) {
                        Some(o) => ops.push(o),
                        None => return Flow::Done,
                    }
                }
                let tail_op = match self.val(cur) {
                    Some(o) => o,
                    None => return Flow::Done,
                };
                let o = self.alloc_list(&ops, &tail_op);
                self.finish_val(o, tail)
            }
            CE::Map(pairs) => {
                if pairs.is_empty() {
                    let o = Op::Global(self.g.static_empty_map());
                    return self.finish_val(o, tail);
                }
                let base = self.slots(pairs.len() * 2);
                for (i, (k, v)) in pairs.iter().enumerate() {
                    let Some(ko) = self.val(k) else { return Flow::Done };
                    self.put(base + 2 * i as u32, &ko);
                    let Some(vo) = self.val(v) else { return Flow::Done };
                    self.put(base + 2 * i as u32 + 1, &vo);
                }
                let p = self.slot_ptr(base);
                let r = self.t();
                self.emit(&format!("{} = call i64 @tn_map_build(ptr %ctx, ptr {}, i64 {})", r, p, pairs.len()));
                let s = self.new_slot();
                self.store(s, &r);
                self.finish_val(Op::Slot(s), tail)
            }
            CE::MapUpdate(b, pairs, strict) => {
                let Some(bo) = self.val(b) else { return Flow::Done };
                let bo = self.to_slot(bo);
                let base = self.slots(pairs.len() * 2);
                for (i, (k, v)) in pairs.iter().enumerate() {
                    let Some(ko) = self.val(k) else { return Flow::Done };
                    self.put(base + 2 * i as u32, &ko);
                    let Some(vo) = self.val(v) else { return Flow::Done };
                    self.put(base + 2 * i as u32 + 1, &vo);
                }
                let bv = self.load(&bo);
                let p = self.slot_ptr(base);
                let r = self.t();
                let f = if *strict { "tn_map_update_many" } else { "tn_map_put_many" };
                self.emit(&format!("{} = call i64 @{}(ptr %ctx, i64 {}, ptr {}, i64 {})", r, f, bv, p, pairs.len()));
                self.check(&r);
                let s = self.new_slot();
                self.store(s, &r);
                self.finish_val(Op::Slot(s), tail)
            }
            CE::Struct(m, pairs) => {
                let key = FunKey::new(m, "__struct__", 0);
                let _ = self.g.known_fun(&key.symbol(), 0);
                let r = self.t();
                self.emit(&format!("{} = call tailcc i64 @{}(ptr %ctx)", r, q(&key.symbol())));
                self.check(&r);
                let s = self.new_slot();
                self.store(s, &r);
                if pairs.is_empty() {
                    return self.finish_val(Op::Slot(s), tail);
                }
                let base = self.slots(pairs.len() * 2);
                for (i, (k, v)) in pairs.iter().enumerate() {
                    let Some(ko) = self.val(k) else { return Flow::Done };
                    self.put(base + 2 * i as u32, &ko);
                    let Some(vo) = self.val(v) else { return Flow::Done };
                    self.put(base + 2 * i as u32 + 1, &vo);
                }
                let dv = self.load(&Op::Slot(s));
                let p = self.slot_ptr(base);
                let r2 = self.t();
                self.emit(&format!("{} = call i64 @tn_map_put_many(ptr %ctx, i64 {}, ptr {}, i64 {})", r2, dv, p, pairs.len()));
                self.check(&r2);
                let s2 = self.new_slot();
                self.store(s2, &r2);
                self.finish_val(Op::Slot(s2), tail)
            }
            CE::Bin(segs) => {
                let o = match self.bin_construct(segs) {
                    Some(o) => o,
                    None => return Flow::Done,
                };
                self.finish_val(o, tail)
            }
            CE::Interp(parts) => {
                let o = match self.interp(parts) {
                    Some(o) => o,
                    None => return Flow::Done,
                };
                self.finish_val(o, tail)
            }
            CE::Call(k, args) => self.call(k, args, tail),
            CE::Bif(sym, args) => {
                let o = match self.bif(sym, args) {
                    Some(o) => o,
                    None => return Flow::Done,
                };
                self.finish_val(o, tail)
            }
            CE::DynCall(m, name, args) => {
                let Some(mo) = self.val(m) else { return Flow::Done };
                let mo = self.to_slot(mo);
                let mut ops = Vec::new();
                for a in args {
                    let Some(o) = self.val(a) else { return Flow::Done };
                    ops.push(o);
                }
                let mv = self.load(&mo);
                let na = self.g.atom(name);
                self.store_line();
                let fp = self.t();
                self.emit(&format!(
                    "{} = call ptr @tn_lookup_mfa_q(ptr %ctx, i64 {}, i64 {}, i64 {})",
                    fp,
                    mv,
                    na,
                    args.len()
                ));
                let z = self.t();
                self.emit(&format!("{} = icmp eq ptr {}, null", z, fp));
                let ok = self.label();
                let undefb = self.label();
                let target = self.exc.clone();
                self.term(&format!("br i1 {}, label %{}, label %{}", z, undefb, ok));
                // undef: the top stack entry carries the call's arguments
                self.block(&undefb);
                let base = self.slots(ops.len().max(1));
                for (i, o) in ops.iter().enumerate() {
                    self.put(base + i as u32, o);
                }
                let ap = self.slot_ptr(base);
                let mv2 = self.load(&mo);
                self.emit(&format!(
                    "call void @tn_undef_raise(ptr %ctx, i64 {}, i64 {}, ptr {}, i64 {})",
                    mv2,
                    na,
                    ap,
                    ops.len()
                ));
                self.term(&format!("br label %{}", target));
                self.block(&ok);
                let mut av = vec!["ptr %ctx".to_string()];
                for o in &ops {
                    let v = self.load(o);
                    av.push(format!("i64 {}", v));
                }
                self.emit_call(&format!("{}", fp), &av, tail)
            }
            CE::Apply(f, args) => {
                let Some(fo) = self.val(f) else { return Flow::Done };
                let fo = self.to_slot(fo);
                let base = self.slots(args.len().max(1));
                for (i, a) in args.iter().enumerate() {
                    let Some(o) = self.val(a) else { return Flow::Done };
                    self.put(base + i as u32, &o);
                }
                let fv = self.load(&fo);
                let okf = self.t();
                self.emit(&format!("{} = call i1 @tn_is_fun_arity(i64 {}, i64 {})", okf, fv, args.len()));
                let good = self.label();
                let bad = self.label();
                self.term(&format!("br i1 {}, label %{}, label %{}", okf, good, bad));
                self.block(&bad);
                let ap = self.slot_ptr(base);
                let br = self.t();
                self.emit(&format!("{} = call i64 @tn_bad_fun(ptr %ctx, i64 {}, ptr {}, i64 {})", br, fv, ap, args.len()));
                let target = self.exc.clone();
                self.br(&target);
                self.block(&good);
                let fv2 = self.load(&fo);
                let p = self.t();
                self.emit(&format!("{} = inttoptr i64 {} to ptr", p, fv2));
                let fpp = self.t();
                self.emit(&format!("{} = getelementptr i64, ptr {}, i64 1", fpp, p));
                let fp = self.t();
                self.emit(&format!("{} = load ptr, ptr {}", fp, fpp));
                let mut av = vec!["ptr %ctx".to_string(), format!("i64 {}", fv2)];
                for i in 0..args.len() {
                    let v = self.load(&Op::Slot(base + i as u32));
                    av.push(format!("i64 {}", v));
                }
                self.emit_call(&fp, &av, tail)
            }
            CE::Fn(l) => {
                let o = self.closure(l);
                self.finish_val(o, tail)
            }
            CE::FunRef(k, t) => {
                let o = self.funref(k, t);
                self.finish_val(o, tail)
            }
            CE::Block(es) => {
                if es.is_empty() {
                    return self.finish_val(Op::Imm(NIL_A), tail);
                }
                for (i, x) in es.iter().enumerate() {
                    let last = i == es.len() - 1;
                    match self.expr(x, tail && last) {
                        Flow::Val(o) => {
                            if last {
                                return Flow::Val(o);
                            }
                        }
                        Flow::Done => return Flow::Done,
                    }
                }
                unreachable!()
            }
            CE::Match(p, x) => {
                let Some(o) = self.val(x) else { return Flow::Done };
                let o = match o {
                    Op::Ssa(_) => self.to_slot(o),
                    o => o,
                };
                let fail = self.label();
                let ok = self.label();
                self.pattern(p, &o, &fail);
                self.br(&ok);
                self.block(&fail);
                let v = self.load(&o);
                let r = self.t();
                self.emit(&format!("{} = call i64 @tn_raise_match(ptr %ctx, i64 {})", r, v));
                let target = self.exc.clone();
                self.br(&target);
                self.block(&ok);
                self.finish_val(o, tail)
            }
            CE::Case(subjects, clauses, fail) => self.case(subjects, clauses, fail, tail),
            CE::If(c, t, f) => {
                let Some(co) = self.val(c) else { return Flow::Done };
                let cv = self.load(&co);
                let b = self.t();
                self.emit(&format!("{} = call i1 @tn_truthy(i64 {})", b, cv));
                let lt = self.label();
                let lf = self.label();
                let end = self.label();
                self.term(&format!("br i1 {}, label %{}, label %{}", b, lt, lf));
                let rs = if tail { None } else { Some(self.new_slot()) };
                let mut falls = false;
                for (l, x) in [(lt, t), (lf, f)] {
                    self.block(&l);
                    match self.expr(x, tail) {
                        Flow::Val(o) => {
                            if let Some(r) = rs {
                                self.put(r, &o);
                            }
                            self.br(&end);
                            falls = true;
                        }
                        Flow::Done => {}
                    }
                }
                if !falls {
                    return Flow::Done;
                }
                self.block(&end);
                Flow::Val(Op::Slot(rs.unwrap()))
            }
            CE::Receive(clauses, after) => self.receive(clauses, after, tail),
            CE::Try(t) => self.try_expr(t, tail),
        }
    }

    fn store_line(&mut self) {
        if let Some(&l) = self.line_stack.last() {
            if self.stored_line != Some(l) {
                self.emit(&format!("store i64 {}, ptr %fr.l", l));
                self.stored_line = Some(l);
            }
        }
    }

    fn emit_call(&mut self, callee: &str, args: &[String], tail: bool) -> Flow {
        self.store_line();
        if tail && self.exc == "unwind" && self.guard_fail.is_none() {
            let r = self.t();
            self.emit("store ptr %prev, ptr %ctx");
            self.emit(&format!("{} = tail call tailcc i64 {}({})", r, callee, args.join(", ")));
            self.term(&format!("ret i64 {}", r));
            return Flow::Done;
        }
        let r = self.t();
        self.emit(&format!("{} = call tailcc i64 {}({})", r, callee, args.join(", ")));
        self.check(&r);
        let s = self.new_slot();
        self.store(s, &r);
        self.finish_val(Op::Slot(s), tail)
    }

    fn call(&mut self, k: &FunKey, args: &[CE], tail: bool) -> Flow {
        let mut ops = Vec::new();
        for a in args {
            let Some(o) = self.val(a) else { return Flow::Done };
            ops.push(o);
        }
        let mut av = vec!["ptr %ctx".to_string()];
        for o in &ops {
            let v = self.load(o);
            av.push(format!("i64 {}", v));
        }
        let callee = format!("@{}", q(&k.symbol()));
        if !self.g.known_fun(&k.symbol(), k.arity) {
            // Should not happen (unreachable functions are filtered); emit a
            // dynamic lookup that raises UndefinedFunctionError.
            let m = self.g.atom(&k.module);
            let n = self.g.atom(&k.name);
            let fp = self.t();
            self.emit(&format!("{} = call ptr @tn_lookup_mfa(ptr %ctx, i64 {}, i64 {}, i64 {})", fp, m, n, k.arity));
            let target = self.exc.clone();
            let z = self.t();
            self.emit(&format!("{} = icmp eq ptr {}, null", z, fp));
            let ok = self.label();
            self.term(&format!("br i1 {}, label %{}, label %{}", z, target, ok));
            self.block(&ok);
            return self.emit_call(&fp, &av, tail);
        }
        self.emit_call(&callee, &av, tail)
    }

    fn inline_helper(sym: &str) -> Option<&'static str> {
        Some(match sym {
            "tn_add" => "tn_add_i",
            "tn_sub" => "tn_sub_i",
            "tn_mul" => "tn_mul_i",
            "tn_div" => "tn_div_i",
            "tn_rem" => "tn_rem_i",
            "tn_eq" => "tn_eq_i",
            "tn_ne" => "tn_ne_i",
            "tn_eqx" => "tn_eqx_i",
            "tn_nex" => "tn_nex_i",
            "tn_lt" => "tn_lt_i",
            "tn_gt" => "tn_gt_i",
            "tn_le" => "tn_le_i",
            "tn_ge" => "tn_ge_i",
            "tn_is_atom" => "tn_is_atom_i",
            "tn_is_integer" => "tn_is_integer_i",
            "tn_is_list" => "tn_is_list_i",
            "tn_is_tuple" => "tn_is_tuple_i",
            "tn_is_map" => "tn_is_map_i",
            "tn_is_binary" => "tn_is_binary_i",
            "tn_is_bitstring" => "tn_is_bitstring_i",
            "tn_is_float" => "tn_is_float_i",
            "tn_is_number" => "tn_is_number_i",
            "tn_is_function" => "tn_is_function_i",
            "tn_is_pid" => "tn_is_pid_i",
            "tn_is_boolean" => "tn_is_boolean_i",
            "tn_hd" => "tn_hd_i",
            "tn_tl" => "tn_tl_i",
            "tn_elem" => "tn_elem_i",
            "tn_tuple_size" => "tn_tuple_size_i",
            "tn_map_size" => "tn_map_size_i",
            "tn_not" => "tn_not_i",
            _ => return None,
        })
    }

    /// Results of these are always immediates (atoms/small ints).
    fn immediate_result(sym: &str) -> bool {
        matches!(
            sym,
            "tn_eq" | "tn_ne" | "tn_eqx" | "tn_nex" | "tn_lt" | "tn_gt" | "tn_le" | "tn_ge" | "tn_is_atom"
                | "tn_is_integer" | "tn_is_list" | "tn_is_tuple" | "tn_is_map" | "tn_is_binary"
                | "tn_is_bitstring" | "tn_is_float" | "tn_is_number" | "tn_is_function" | "tn_is_pid"
                | "tn_is_boolean" | "tn_not" | "tn_is_map_key" | "tn_is_struct" | "tn_is_struct2" | "tn_is_non_struct_map"
                | "tn_is_exception" | "tn_is_exception2" | "tn_is_function2" | "tn_tuple_size" | "tn_map_size" | "tn_length"
                | "tn_byte_size" | "tn_is_reference" | "tn_is_port" | "tn_is_nil" | "tn_lists_member"
                | "tn_self" | "tn_str_printable" | "tn_list_ascii_printable" | "tn_bit_size"
        )
    }

    fn bif(&mut self, sym: &str, args: &[CE]) -> Option<Op> {
        match sym {
            "__and" | "__or" | "__not" | "__gor" => {
                // Boolean value of a guard sub-expression.
                let r = self.new_slot();
                let lf = self.label();
                let end = self.label();
                // An exception anywhere in a guard fails the whole guard.
                let saved = self.guard_fail.clone();
                if saved.is_none() {
                    self.guard_fail = Some(lf.clone());
                }
                self.guard_test(&CE::Bif(sym.to_string(), args.to_vec()), &lf);
                self.guard_fail = saved;
                self.store(r, &format!("{}", TRUE_A));
                self.br(&end);
                self.block(&lf);
                self.store(r, &format!("{}", FALSE_A));
                self.br(&end);
                self.block(&end);
                return Some(Op::Slot(r));
            }
            _ => {}
        }
        let mut ops = Vec::new();
        for a in args {
            ops.push(self.val(a)?);
        }
        let mut av = vec!["ptr %ctx".to_string()];
        for o in &ops {
            let v = self.load(o);
            av.push(format!("i64 {}", v));
        }
        let (callee, inline) = match Self::inline_helper(sym) {
            Some(h) => (h.to_string(), true),
            None => {
                self.g.declare(sym, args.len());
                (sym.to_string(), false)
            }
        };
        let _ = inline;
        let nofail = bifs::nofail_sym(sym);
        if !nofail && self.guard_fail.is_none() {
            self.store_line();
        }
        let r = self.t();
        self.emit(&format!("{} = call i64 @{}({})", r, callee, av.join(", ")));
        if !nofail {
            match bifs::erl_origin(sym) {
                Some((em, ef)) if self.guard_fail.is_none() && args.len() <= 4 => {
                    // On failure, add the BIF's own `{m, f, args, _}` frame on top.
                    let c = self.t();
                    let fl = self.label();
                    let ok = self.label();
                    self.emit(&format!("{} = icmp eq i64 {}, 0", c, r));
                    self.term(&format!("br i1 {}, label %{}, label %{}", c, fl, ok));
                    let keep = self.stored_line;
                    self.block(&fl);
                    let ma = self.g.atom(em);
                    let fa = self.g.atom(ef);
                    let mut fargs = vec![
                        "ptr %ctx".to_string(),
                        format!("i64 {}", ma),
                        format!("i64 {}", fa),
                        format!("i64 {}", args.len()),
                    ];
                    for i in 0..4 {
                        fargs.push(match av.get(i + 1) {
                            Some(a) => a.clone(),
                            None => "i64 0".to_string(),
                        });
                    }
                    self.g.declare("tn_bif_frame", 7);
                    let _ = self.t();
                    self.emit(&format!("call i64 @tn_bif_frame({})", fargs.join(", ")));
                    let exc = self.exc.clone();
                    self.br(&exc);
                    self.block(&ok);
                    self.stored_line = keep;
                }
                _ => self.check(&r),
            }
        }
        if Self::immediate_result(sym) {
            return Some(Op::Ssa(r));
        }
        let s = self.new_slot();
        self.store(s, &r);
        Some(Op::Slot(s))
    }

    fn alloc_obj(&mut self, hdr: u64, ops: &[Op]) -> Op {
        let n = ops.len();
        let words = 1 + n.max(1);
        let p = self.t();
        self.emit(&format!("{} = call ptr @tn_alloc(ptr %ctx, i64 {})", p, words));
        self.emit(&format!("store i64 {}, ptr {}", hdr as i64, p));
        for (i, o) in ops.iter().enumerate() {
            let v = self.load(o);
            let fp = self.t();
            self.emit(&format!("{} = getelementptr i64, ptr {}, i64 {}", fp, p, i + 1));
            self.emit(&format!("store i64 {}, ptr {}", v, fp));
        }
        let t = self.t();
        self.emit(&format!("{} = ptrtoint ptr {} to i64", t, p));
        let s = self.new_slot();
        self.store(s, &t);
        Op::Slot(s)
    }

    fn alloc_list(&mut self, items: &[Op], tail: &Op) -> Op {
        let n = items.len();
        let p = self.t();
        self.emit(&format!("{} = call ptr @tn_alloc(ptr %ctx, i64 {})", p, 3 * n));
        let hdr = enc::header(enc::T_CONS, 2) as i64;
        for (i, o) in items.iter().enumerate() {
            let cell = self.t();
            self.emit(&format!("{} = getelementptr i64, ptr {}, i64 {}", cell, p, 3 * i));
            self.emit(&format!("store i64 {}, ptr {}", hdr, cell));
            let v = self.load(o);
            let hp = self.t();
            self.emit(&format!("{} = getelementptr i64, ptr {}, i64 {}", hp, p, 3 * i + 1));
            self.emit(&format!("store i64 {}, ptr {}", v, hp));
            let tp = self.t();
            self.emit(&format!("{} = getelementptr i64, ptr {}, i64 {}", tp, p, 3 * i + 2));
            let tv = if i + 1 < n {
                let nx = self.t();
                self.emit(&format!("{} = getelementptr i64, ptr {}, i64 {}", nx, p, 3 * (i + 1)));
                let nv = self.t();
                self.emit(&format!("{} = ptrtoint ptr {} to i64", nv, nx));
                nv
            } else {
                self.load(tail)
            };
            self.emit(&format!("store i64 {}, ptr {}", tv, tp));
        }
        let t = self.t();
        self.emit(&format!("{} = ptrtoint ptr {} to i64", t, p));
        let s = self.new_slot();
        self.store(s, &t);
        Op::Slot(s)
    }

    /// BEAM-style identity of a lambda: (module, "-parent/arity-fun-N-", N),
    /// N counting the module's anonymous functions.
    fn lambda_erl_name(&mut self, l: &Lambda) -> (String, String, usize) {
        let idx = match self.g.lambda_index.get(&(l.module.clone(), l.id)) {
            Some(i) => *i,
            None => {
                let c = self.g.lambda_count.entry(l.module.clone()).or_insert(0);
                let i = *c;
                *c += 1;
                self.g.lambda_index.insert((l.module.clone(), l.id), i);
                i
            }
        };
        let pname = l.parent.split('/').next().unwrap_or("");
        let (fmod, parent) = if l.module == "Elixir.Tonic.Script" && pname == "__script__" {
            ("elixir_compiler_1".to_string(), "__FILE__/1".to_string())
        } else {
            (l.module.clone(), l.parent.clone())
        };
        (fmod, format!("-{}-fun-{}-", parent, idx), idx)
    }

    fn closure(&mut self, l: &Rc<Lambda>) -> Op {
        let free = lambda_free(l);
        let sym = lambda_sym(l);
        self.g.lambda_queue.push((l.clone(), free.clone()));
        self.g.max_arity = self.g.max_arity.max(l.arity);
        let (fmod, fname, idx) = self.lambda_erl_name(l);
        let info = self.g.fun_info(0, &fmod, &fname, l.arity, idx);
        if free.is_empty() {
            return Op::Global(self.g.static_closure(&sym, l.arity, &info));
        }
        let n = free.len();
        let p = self.t();
        self.emit(&format!("{} = call ptr @tn_alloc(ptr %ctx, i64 {})", p, 4 + n));
        self.emit(&format!("store i64 {}, ptr {}", enc::header(enc::T_CLOSURE, 3 + n as u64) as i64, p));
        let f1 = self.t();
        self.emit(&format!("{} = getelementptr i64, ptr {}, i64 1", f1, p));
        self.emit(&format!("store ptr @{}, ptr {}", q(&sym), f1));
        let f2 = self.t();
        self.emit(&format!("{} = getelementptr i64, ptr {}, i64 2", f2, p));
        self.emit(&format!("store i64 {}, ptr {}", l.arity, f2));
        let f3 = self.t();
        self.emit(&format!("{} = getelementptr i64, ptr {}, i64 3", f3, p));
        self.emit(&format!("store ptr @{}, ptr {}", q(&info), f3));
        for (i, v) in free.iter().enumerate() {
            let s = self.var_slot(*v);
            let x = self.load(&Op::Slot(s));
            let fp = self.t();
            self.emit(&format!("{} = getelementptr i64, ptr {}, i64 {}", fp, p, 4 + i));
            self.emit(&format!("store i64 {}, ptr {}", x, fp));
        }
        let t = self.t();
        self.emit(&format!("{} = ptrtoint ptr {} to i64", t, p));
        let s = self.new_slot();
        self.store(s, &t);
        Op::Slot(s)
    }

    fn interp(&mut self, parts: &[CE]) -> Option<Op> {
        if parts.len() == 1 {
            if let CE::Lit(Lit::Bin(_)) = &parts[0] {
                return self.val(&parts[0]);
            }
        }
        let base = self.slots(parts.len());
        for (i, p) in parts.iter().enumerate() {
            let o = self.val(p)?;
            if let CE::Lit(Lit::Bin(_)) = p {
                self.put(base + i as u32, &o);
                continue;
            }
            let v = self.load(&o);
            let isb = self.t();
            self.emit(&format!("{} = call i1 @tn_is_bin_raw(i64 {})", isb, v));
            let lb = self.label();
            let lc = self.label();
            let end = self.label();
            self.term(&format!("br i1 {}, label %{}, label %{}", isb, lb, lc));
            self.block(&lb);
            self.store(base + i as u32, &v);
            self.br(&end);
            self.block(&lc);
            let key = FunKey::new("Elixir.String.Chars", "to_string", 1);
            let v2 = self.load(&o);
            let r = self.t();
            if self.g.known_fun(&key.symbol(), 1) {
                self.emit(&format!("{} = call tailcc i64 @{}(ptr %ctx, i64 {})", r, q(&key.symbol()), v2));
            } else {
                self.g.declare("tn_debug_to_string", 1);
                self.emit(&format!("{} = call i64 @tn_debug_to_string(ptr %ctx, i64 {})", r, v2));
            }
            self.check(&r);
            self.store(base + i as u32, &r);
            self.br(&end);
            self.block(&end);
        }
        let p = self.slot_ptr(base);
        let r = self.t();
        self.emit(&format!("{} = call i64 @tn_bin_concat_n(ptr %ctx, ptr {}, i64 {})", r, p, parts.len()));
        self.check(&r);
        let s = self.new_slot();
        self.store(s, &r);
        Some(Op::Slot(s))
    }

    fn bin_construct(&mut self, segs: &[BinSeg<CE>]) -> Option<Op> {
        // All-literal binary?
        let mut lit_bytes = Vec::new();
        let mut all_lit = true;
        for s in segs {
            match (&s.val, &s.ty, &s.size) {
                (CE::Lit(Lit::Bin(b)), BinType::Bin, None) => lit_bytes.extend(b),
                (CE::Lit(Lit::Int(v)), BinType::Int, Some(CE::Lit(Lit::Int(8)))) if (0..256).contains(v) || (-128..0).contains(v) => {
                    lit_bytes.push(*v as u8)
                }
                _ => {
                    all_lit = false;
                    break;
                }
            }
        }
        if all_lit {
            return Some(Op::Global(self.g.static_bin(&lit_bytes)));
        }
        let base = self.slots(segs.len());
        for (i, s) in segs.iter().enumerate() {
            let vo = self.val(&s.val)?;
            if let (CE::Lit(Lit::Bin(_)), BinType::Bin, None) = (&s.val, &s.ty, &s.size) {
                self.put(base + i as u32, &vo);
                continue;
            }
            let vo = self.to_slot(vo);
            let so = match &s.size {
                Some(sz) => self.val(sz)?,
                None => Op::Imm(small(-1)),
            };
            let so = self.to_slot(so);
            let ty = match s.ty {
                BinType::Int => 0,
                BinType::Bin => 1,
                BinType::Float => 2,
                BinType::Utf8 => 3,
                BinType::Utf16 => 4,
                BinType::Utf32 => 5,
                BinType::Bits => 6,
            };
            let flags = ty | if s.little { 1 << 8 } else { 0 } | if s.signed { 1 << 9 } else { 0 };
            let v = self.load(&vo);
            let sz = self.load(&so);
            let r = self.t();
            self.emit(&format!("{} = call i64 @tn_bs_seg(ptr %ctx, i64 {}, i64 {}, i64 {})", r, v, sz, flags));
            self.check(&r);
            self.store(base + i as u32, &r);
        }
        let p = self.slot_ptr(base);
        let r = self.t();
        self.emit(&format!("{} = call i64 @tn_bs_concat_n(ptr %ctx, ptr {}, i64 {})", r, p, segs.len()));
        self.check(&r);
        let s = self.new_slot();
        self.store(s, &r);
        Some(Op::Slot(s))
    }

    // ------------------------------------------------------------------
    // case / patterns / guards
    // ------------------------------------------------------------------

    fn case(&mut self, subjects: &[CE], clauses: &[Clause], fail: &Fail, tail: bool) -> Flow {
        let mut sops = Vec::new();
        for s in subjects {
            let Some(o) = self.val(s) else { return Flow::Done };
            let o = match o {
                Op::Ssa(_) => self.to_slot(o),
                o => o,
            };
            sops.push(o);
        }
        let rs = if tail { None } else { Some(self.new_slot()) };
        let end = self.label();
        let mut falls = false;
        for c in clauses {
            let next = self.label();
            for (p, o) in c.pats.iter().zip(sops.iter()) {
                self.pattern(p, o, &next);
            }
            if let Some(g) = &c.guard {
                self.guard(g, &next);
            }
            match self.expr(&c.body, tail) {
                Flow::Val(o) => {
                    if let Some(r) = rs {
                        self.put(r, &o);
                    }
                    self.br(&end);
                    falls = true;
                }
                Flow::Done => {}
            }
            self.block(&next);
        }
        // No clause matched.
        match fail {
            Fail::Passthrough => {
                let o = sops[0].clone();
                if tail {
                    let v = self.load(&o);
                    self.ret(&v);
                } else {
                    self.put(rs.unwrap(), &o);
                    self.br(&end);
                    falls = true;
                }
            }
            _ => {
                let r = self.t();
                match fail {
                    Fail::CaseClause | Fail::Match | Fail::WithClause | Fail::TryClause => {
                        let f = match fail {
                            Fail::CaseClause => "tn_raise_case_clause",
                            Fail::Match => "tn_raise_match",
                            Fail::WithClause => "tn_raise_with_clause",
                            _ => "tn_raise_try_clause",
                        };
                        let v = self.load(&sops[0]);
                        self.emit(&format!("{} = call i64 @{}(ptr %ctx, i64 {})", r, f, v));
                    }
                    Fail::FunctionClause { module, name, line } => {
                        if *line > 0 {
                            self.emit(&format!("store i64 {}, ptr %fr.l", line));
                            self.stored_line = Some(*line);
                        }
                        let m = self.g.atom(module);
                        let n = self.g.atom(name);
                        let n_args = sops.len();
                        // params are contiguous in slots [param_slots[0]..]
                        let p = if n_args > 0 {
                            let s0 = self.param_slots.first().copied().unwrap_or(0);
                            self.slot_ptr(s0)
                        } else {
                            "null".to_string()
                        };
                        self.emit(&format!(
                            "{} = call i64 @tn_raise_function_clause(ptr %ctx, i64 {}, i64 {}, ptr {}, i64 {})",
                            r, m, n, p, n_args
                        ));
                    }
                    Fail::Passthrough => unreachable!(),
                }
                let target = self.exc.clone();
                self.br(&target);
            }
        }
        if !falls {
            return Flow::Done;
        }
        self.block(&end);
        Flow::Val(Op::Slot(rs.unwrap()))
    }

    fn guard(&mut self, g: &CE, fail: &str) {
        let saved = self.guard_fail.replace(fail.to_string());
        self.guard_test(g, fail);
        self.guard_fail = saved;
    }

    /// Emit code that branches to `fail` unless `g` evaluates to true.
    fn guard_test(&mut self, g: &CE, fail: &str) {
        match g {
            CE::Bif(s, a) if s == "__and" => {
                self.guard_test(&a[0], fail);
                self.guard_test(&a[1], fail);
            }
            CE::Bif(s, a) if s == "__gor" => {
                let second = self.label();
                let ok = self.label();
                let saved = self.guard_fail.replace(second.clone());
                self.guard_test(&a[0], &second);
                self.guard_fail = saved;
                self.br(&ok);
                self.block(&second);
                self.guard_test(&a[1], fail);
                self.br(&ok);
                self.block(&ok);
            }
            CE::Bif(s, a) if s == "__or" => {
                let second = self.label();
                let ok = self.label();
                self.guard_test(&a[0], &second);
                self.br(&ok);
                self.block(&second);
                self.guard_test(&a[1], fail);
                self.br(&ok);
                self.block(&ok);
            }
            CE::Bif(s, a) if s == "__not" => {
                // succeed iff inner is exactly false
                let inner_true = self.label();
                let inner_false = self.label();
                let saved = self.guard_fail.replace(inner_false.clone());
                let o = match &a[0] {
                    CE::Bif(s2, _) if s2 == "__and" || s2 == "__or" || s2 == "__not" => {
                        self.guard_test(&a[0], &inner_false);
                        self.guard_fail = saved.clone();
                        self.br(&inner_true);
                        self.block(&inner_true);
                        self.br(fail);
                        self.block(&inner_false);
                        return;
                    }
                    other => self.val(other),
                };
                self.guard_fail = saved;
                let Some(o) = o else {
                    return;
                };
                let v = self.load(&o);
                let c = self.t();
                self.emit(&format!("{} = icmp eq i64 {}, {}", c, v, FALSE_A));
                let ok = self.label();
                self.term(&format!("br i1 {}, label %{}, label %{}", c, ok, fail));
                // inner_false label reached when inner errored: guard fails
                self.block(&inner_false);
                self.br(fail);
                self.block(&inner_true);
                self.br(fail);
                self.block(&ok);
            }
            _ => {
                let Some(o) = self.val(g) else { return };
                let v = self.load(&o);
                let c = self.t();
                self.emit(&format!("{} = icmp eq i64 {}, {}", c, v, TRUE_A));
                let ok = self.label();
                self.term(&format!("br i1 {}, label %{}, label %{}", c, ok, fail));
                self.block(&ok);
            }
        }
    }

    fn cond_br(&mut self, cond: &str, fail: &str) {
        let ok = self.label();
        self.term(&format!("br i1 {}, label %{}, label %{}", cond, ok, fail));
        self.block(&ok);
    }

    /// Match `p` against `o`; branch to `fail` on mismatch.
    fn pattern(&mut self, p: &Pat, o: &Op, fail: &str) {
        match p {
            Pat::Wild => {}
            Pat::Bind(v) => {
                // A variable bound once to a parameter shares its slot.
                if let Op::Slot(os) = o {
                    if !self.vars.contains_key(v)
                        && self.param_slots.contains(os)
                        && self.bind_counts.get(v).copied().unwrap_or(0) == 1
                    {
                        self.vars.insert(*v, *os);
                        return;
                    }
                }
                let s = self.var_slot(*v);
                if let Op::Slot(os) = o {
                    if *os == s {
                        return;
                    }
                }
                self.put(s, o);
            }
            Pat::Eq(v) => {
                let s = self.var_slot(*v);
                let a = self.load(o);
                let b = self.load(&Op::Slot(s));
                let r = self.t();
                self.emit(&format!("{} = call i64 @tn_eqx_i(ptr %ctx, i64 {}, i64 {})", r, a, b));
                let c = self.t();
                self.emit(&format!("{} = icmp eq i64 {}, {}", c, r, TRUE_A));
                self.cond_br(&c, fail);
            }
            Pat::Lit(l) => {
                let lo = self.lit(l);
                let a = self.load(o);
                match lo {
                    Op::Imm(x) => {
                        let c = self.t();
                        self.emit(&format!("{} = icmp eq i64 {}, {}", c, a, x as i64));
                        self.cond_br(&c, fail);
                    }
                    other => {
                        let b = self.load(&other);
                        let r = self.t();
                        self.emit(&format!("{} = call i64 @tn_eqx_i(ptr %ctx, i64 {}, i64 {})", r, a, b));
                        let c = self.t();
                        self.emit(&format!("{} = icmp eq i64 {}, {}", c, r, TRUE_A));
                        self.cond_br(&c, fail);
                    }
                }
            }
            Pat::Tuple(ps) => {
                let a = self.load(o);
                let c = self.t();
                let hdr = enc::header(enc::T_TUPLE, ps.len() as u64);
                self.emit(&format!("{} = call i1 @tn_has_header(i64 {}, i64 {})", c, a, hdr as i64));
                self.cond_br(&c, fail);
                let elems = self.extract(o, ps.len(), 1);
                for (p, e) in ps.iter().zip(elems.iter()) {
                    self.pattern(p, e, fail);
                }
            }
            Pat::Cons(h, t) => {
                let a = self.load(o);
                let c = self.t();
                let hdr = enc::header(enc::T_CONS, 2);
                self.emit(&format!("{} = call i1 @tn_has_header(i64 {}, i64 {})", c, a, hdr as i64));
                self.cond_br(&c, fail);
                let elems = self.extract(o, 2, 1);
                self.pattern(h, &elems[0], fail);
                self.pattern(t, &elems[1], fail);
            }
            Pat::Map(pairs) => {
                let a = self.load(o);
                let c = self.t();
                let hdr = enc::header(enc::T_MAP, 2);
                self.emit(&format!("{} = call i1 @tn_has_header(i64 {}, i64 {})", c, a, hdr as i64));
                self.cond_br(&c, fail);
                let mut vals = Vec::new();
                for (k, _) in pairs {
                    let Some(ko) = self.val(k) else { return };
                    let a = self.load(o);
                    let kv = self.load(&ko);
                    let r = self.t();
                    self.emit(&format!("{} = call i64 @tn_map_find(ptr %ctx, i64 {}, i64 {})", r, a, kv));
                    let z = self.t();
                    self.emit(&format!("{} = icmp ne i64 {}, 0", z, r));
                    self.cond_br(&z, fail);
                    let s = self.new_slot();
                    self.store(s, &r);
                    vals.push(Op::Slot(s));
                }
                for ((_, p), v) in pairs.iter().zip(vals.iter()) {
                    self.pattern(p, v, fail);
                }
            }
            Pat::Alias(a, b) => {
                let o = match o {
                    Op::Ssa(_) => self.to_slot(o.clone()),
                    o => o.clone(),
                };
                self.pattern(a, &o, fail);
                self.pattern(b, &o, fail);
            }
            Pat::Bin(segs) => self.bin_pattern(segs, o, fail),
            Pat::Atom(inner) => {
                let o = match o {
                    Op::Ssa(_) => self.to_slot(o.clone()),
                    o => o.clone(),
                };
                let a = self.load(&o);
                let r = self.t();
                self.emit(&format!("{} = call i64 @tn_is_atom_i(ptr %ctx, i64 {})", r, a));
                let c = self.t();
                self.emit(&format!("{} = icmp eq i64 {}, {}", c, r, TRUE_A));
                self.cond_br(&c, fail);
                self.pattern(inner, &o, fail);
            }
        }
    }

    /// Load `n` words starting at word `start` of the boxed object into slots.
    fn extract(&mut self, o: &Op, n: usize, start: usize) -> Vec<Op> {
        let a = self.load(o);
        let p = self.t();
        self.emit(&format!("{} = inttoptr i64 {} to ptr", p, a));
        let mut out = Vec::new();
        for i in 0..n {
            let fp = self.t();
            self.emit(&format!("{} = getelementptr i64, ptr {}, i64 {}", fp, p, start + i));
            let v = self.t();
            self.emit(&format!("{} = load i64, ptr {}", v, fp));
            let s = self.new_slot();
            self.store(s, &v);
            out.push(Op::Slot(s));
        }
        out
    }

    fn bin_pattern(&mut self, segs: &[BinSeg<Pat>], o: &Op, fail: &str) {
        let o = match o {
            Op::Slot(_) | Op::Global(_) => o.clone(),
            other => self.to_slot(other.clone()),
        };
        let a = self.load(&o);
        let sz = self.t();
        self.emit(&format!("{} = call i64 @tn_bs_size(ptr %ctx, i64 {})", sz, a));
        let z = self.t();
        self.emit(&format!("{} = icmp ne i64 {}, 0", z, sz));
        self.cond_br(&z, fail);
        let size_slot = self.new_slot();
        self.store(size_slot, &sz);
        let off = self.new_slot();
        self.store(off, &format!("{}", small(0)));
        let mut consumed_rest = false;
        for (i, s) in segs.iter().enumerate() {
            let last = i == segs.len() - 1;
            match (&s.ty, &s.val) {
                (BinType::Bin, Pat::Lit(Lit::Bin(b))) if s.size.is_none() => {
                    let lit = Op::Global(self.g.static_bin(b));
                    let bv = self.load(&o);
                    let ov = self.load(&Op::Slot(off));
                    let lv = self.load(&lit);
                    let r = self.t();
                    self.emit(&format!("{} = call i64 @tn_bs_lit(ptr %ctx, i64 {}, i64 {}, i64 {})", r, bv, ov, lv));
                    let c = self.t();
                    self.emit(&format!("{} = icmp eq i64 {}, {}", c, r, TRUE_A));
                    self.cond_br(&c, fail);
                    self.add_off(off, &Op::Imm(small(b.len() as i64)), 8);
                }
                (BinType::Bin, _) | (BinType::Bits, _) => {
                    let aligned = if matches!(s.ty, BinType::Bin) { TRUE_A } else { FALSE_A };
                    match &s.size {
                        None => {
                            if !last {
                                // binary without size must be last; treat as fail-safe
                            }
                            let bv = self.load(&o);
                            let ov = self.load(&Op::Slot(off));
                            let r = self.t();
                            self.emit(&format!("{} = call i64 @tn_bs_rest(ptr %ctx, i64 {}, i64 {}, i64 {})", r, bv, ov, aligned));
                            let c = self.t();
                            self.emit(&format!("{} = icmp ne i64 {}, 0", c, r));
                            self.cond_br(&c, fail);
                            let sl = self.new_slot();
                            self.store(sl, &r);
                            consumed_rest = true;
                            self.pattern(&s.val, &Op::Slot(sl), fail);
                            let szv = self.load(&Op::Slot(size_slot));
                            self.store(off, &szv);
                        }
                        Some(sz) => {
                            let Some(so) = self.val(sz) else { return };
                            let so = self.to_slot(so);
                            let bv = self.load(&o);
                            let ov = self.load(&Op::Slot(off));
                            let sv = self.load(&so);
                            let r = self.t();
                            self.emit(&format!("{} = call i64 @tn_bs_bin(ptr %ctx, i64 {}, i64 {}, i64 {})", r, bv, ov, sv));
                            let c = self.t();
                            self.emit(&format!("{} = icmp ne i64 {}, 0", c, r));
                            self.cond_br(&c, fail);
                            let sl = self.new_slot();
                            self.store(sl, &r);
                            self.add_off(off, &so, 1);
                            self.pattern(&s.val, &Op::Slot(sl), fail);
                        }
                    }
                }
                (BinType::Int, _) | (BinType::Float, _) => {
                    let Some(so) = self.val(s.size.as_ref().unwrap()) else { return };
                    let so = self.to_slot(so);
                    let bv = self.load(&o);
                    let ov = self.load(&Op::Slot(off));
                    let sv = self.load(&so);
                    let flags = if s.little { 1 << 8 } else { 0 } | if s.signed { 1 << 9 } else { 0 };
                    let f = if matches!(s.ty, BinType::Int) { "tn_bs_int" } else { "tn_bs_float" };
                    let r = self.t();
                    self.emit(&format!("{} = call i64 @{}(ptr %ctx, i64 {}, i64 {}, i64 {}, i64 {})", r, f, bv, ov, sv, flags));
                    let c = self.t();
                    self.emit(&format!("{} = icmp ne i64 {}, 0", c, r));
                    self.cond_br(&c, fail);
                    let sl = self.new_slot();
                    self.store(sl, &r);
                    self.add_off(off, &so, 1);
                    self.pattern(&s.val, &Op::Slot(sl), fail);
                }
                (BinType::Utf8, _) => {
                    let bv = self.load(&o);
                    let ov = self.load(&Op::Slot(off));
                    let r = self.t();
                    self.emit(&format!("{} = call i64 @tn_bs_utf8(ptr %ctx, i64 {}, i64 {})", r, bv, ov));
                    let c = self.t();
                    self.emit(&format!("{} = icmp ne i64 {}, 0", c, r));
                    self.cond_br(&c, fail);
                    let sl = self.new_slot();
                    self.store(sl, &r);
                    let l = self.t();
                    self.emit(&format!("{} = call i64 @tn_utf8_len_i(i64 {})", l, r));
                    self.add_off(off, &Op::Ssa(l), 8);
                    self.pattern(&s.val, &Op::Slot(sl), fail);
                }
                (BinType::Utf16, _) | (BinType::Utf32, _) => {
                    let bv = self.load(&o);
                    let ov = self.load(&Op::Slot(off));
                    let flags = (if matches!(s.ty, BinType::Utf16) { 4 } else { 5 }) | if s.little { 1 << 8 } else { 0 };
                    let r = self.t();
                    self.emit(&format!("{} = call i64 @tn_bs_utfn(ptr %ctx, i64 {}, i64 {}, i64 {})", r, bv, ov, flags));
                    let c = self.t();
                    self.emit(&format!("{} = icmp ne i64 {}, 0", c, r));
                    self.cond_br(&c, fail);
                    let sl = self.new_slot();
                    self.store(sl, &r);
                    let l = self.t();
                    self.emit(&format!("{} = call i64 @tn_bs_utfn_bits(ptr %ctx, i64 {}, i64 {})", l, r, flags));
                    self.add_off(off, &Op::Ssa(l), 1);
                    self.pattern(&s.val, &Op::Slot(sl), fail);
                }
                _ => {
                    self.br(fail);
                    return;
                }
            }
        }
        if !consumed_rest {
            let ov = self.load(&Op::Slot(off));
            let szv = self.load(&Op::Slot(size_slot));
            let c = self.t();
            self.emit(&format!("{} = icmp eq i64 {}, {}", c, ov, szv));
            self.cond_br(&c, fail);
        }
    }

    /// off += amount (tagged small ints); `bits` divides by 8 first.
    fn add_off(&mut self, off: u32, amount: &Op, mult: i64) {
        let ov = self.load(&Op::Slot(off));
        let av = self.load(amount);
        let n = self.t();
        self.emit(&format!("{} = ashr i64 {}, 1", n, av));
        let n2 = if mult != 1 {
            let d = self.t();
            self.emit(&format!("{} = mul i64 {}, {}", d, n, mult));
            d
        } else {
            n
        };
        let sh = self.t();
        self.emit(&format!("{} = shl i64 {}, 1", sh, n2));
        let r = self.t();
        self.emit(&format!("{} = add i64 {}, {}", r, ov, sh));
        self.store(off, &r);
    }

    // ------------------------------------------------------------------
    // receive / try
    // ------------------------------------------------------------------

    fn receive(&mut self, clauses: &[Clause], after: &Option<(Box<CE>, Box<CE>)>, tail: bool) -> Flow {
        let timeout = match after {
            Some((t, _)) => {
                let Some(o) = self.val(t) else { return Flow::Done };
                self.to_slot(o)
            }
            None => Op::Imm(enc::atom(7)),
        };
        let rs = if tail { None } else { Some(self.new_slot()) };
        let lp = self.label();
        let got = self.label();
        let to_l = self.label();
        let end = self.label();
        self.block(&lp);
        let tv = self.load(&timeout);
        let m = self.t();
        self.emit(&format!("{} = call i64 @tn_recv_next(ptr %ctx, i64 {})", m, tv));
        self.check(&m);
        let isto = self.t();
        self.emit(&format!("{} = icmp eq i64 {}, {}", isto, m, enc::TIMEOUT_MARK));
        self.term(&format!("br i1 {}, label %{}, label %{}", isto, to_l, got));
        self.block(&got);
        let ms = self.new_slot();
        self.store(ms, &m);
        let mut falls = false;
        for c in clauses {
            let next = self.label();
            self.pattern(&c.pats[0], &Op::Slot(ms), &next);
            if let Some(g) = &c.guard {
                self.guard(g, &next);
            }
            self.emit("call i64 @tn_recv_accept(ptr %ctx)");
            match self.expr(&c.body, tail) {
                Flow::Val(o) => {
                    if let Some(r) = rs {
                        self.put(r, &o);
                    }
                    self.br(&end);
                    falls = true;
                }
                Flow::Done => {}
            }
            self.block(&next);
        }
        self.br(&lp);
        self.block(&to_l);
        match after {
            Some((_, body)) => match self.expr(body, tail) {
                Flow::Val(o) => {
                    if let Some(r) = rs {
                        self.put(r, &o);
                    }
                    self.br(&end);
                    falls = true;
                }
                Flow::Done => {}
            },
            None => {
                self.term("unreachable");
            }
        }
        if !falls {
            return Flow::Done;
        }
        self.block(&end);
        Flow::Val(Op::Slot(rs.unwrap()))
    }

    fn try_expr(&mut self, t: &TryE, tail: bool) -> Flow {
        let outer = self.exc.clone();
        let handler = self.label();
        let after_exc = self.label();
        let end = self.label();
        let rs = self.new_slot();
        let exc_after = if t.after.is_some() { after_exc.clone() } else { outer.clone() };
        let mut falls = false;

        // body
        self.exc = handler.clone();
        let body = self.expr(&t.body, false);
        self.exc = exc_after.clone();
        if let Flow::Val(o) = body {
            if t.else_clauses.is_empty() {
                self.put(rs, &o);
                self.run_after(t, &outer, &end, rs, &mut falls);
            } else {
                let o = self.to_slot(o);
                let bs = self.new_slot();
                self.put(bs, &o);
                // match else clauses against body value
                let vid = u32::MAX - bs; // synthetic var id bound to slot
                self.vars.insert(vid, bs);
                let res = self.case(&[CE::Var(vid)], &t.else_clauses, &Fail::TryClause, false);
                if let Flow::Val(ro) = res {
                    self.put(rs, &ro);
                    self.run_after(t, &outer, &end, rs, &mut falls);
                }
            }
        }

        // handler
        self.block(&handler);
        self.exc = exc_after.clone();
        let e = self.t();
        self.emit(&format!("{} = call i64 @tn_exc_get(ptr %ctx)", e));
        // killed: skip handlers and after blocks
        let ze = self.t();
        self.emit(&format!("{} = icmp eq i64 {}, 0", ze, e));
        let hk = self.label();
        self.term(&format!("br i1 {}, label %{}, label %{}", ze, outer, hk));
        self.block(&hk);
        let es = self.new_slot();
        self.store(es, &e);
        for c in &t.catches {
            let next = self.label();
            self.pattern(&c.pats[0], &Op::Slot(es), &next);
            if let Some(g) = &c.guard {
                self.guard(g, &next);
            }
            match self.expr(&c.body, false) {
                Flow::Val(o) => {
                    self.put(rs, &o);
                    self.run_after(t, &outer, &end, rs, &mut falls);
                }
                Flow::Done => {}
            }
            self.block(&next);
        }
        // no handler matched: re-raise (after runs via exc_after)
        let ev = self.load(&Op::Slot(es));
        let rr = self.t();
        self.emit(&format!("{} = call i64 @tn_exc_reraise(ptr %ctx, i64 {})", rr, ev));
        self.br(&exc_after);

        // after on exceptional path
        if let Some(a) = &t.after {
            self.block(&after_exc);
            self.exc = outer.clone();
            let e2 = self.t();
            self.emit(&format!("{} = call i64 @tn_exc_get(ptr %ctx)", e2));
            let z2 = self.t();
            self.emit(&format!("{} = icmp eq i64 {}, 0", z2, e2));
            let cont = self.label();
            self.term(&format!("br i1 {}, label %{}, label %{}", z2, outer, cont));
            self.block(&cont);
            let s2 = self.new_slot();
            self.store(s2, &e2);
            if let Flow::Val(_) = self.expr(a, false) {
                let v = self.load(&Op::Slot(s2));
                let r = self.t();
                self.emit(&format!("{} = call i64 @tn_exc_reraise(ptr %ctx, i64 {})", r, v));
                self.br(&outer);
            }
        }
        self.exc = outer;
        if !falls {
            return Flow::Done;
        }
        self.block(&end);
        self.finish_val(Op::Slot(rs), tail)
    }

    fn run_after(&mut self, t: &TryE, outer: &str, end: &str, _rs: u32, falls: &mut bool) {
        let saved = self.exc.clone();
        self.exc = outer.to_string();
        if let Some(a) = &t.after {
            if let Flow::Val(_) = self.expr(a, false) {
                self.br(end);
                *falls = true;
            }
        } else {
            self.br(end);
            *falls = true;
        }
        self.exc = saved;
    }
}

const RUNTIME_DECLS: &str = r#"declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)
declare {i64, i1} @llvm.sadd.with.overflow.i64(i64, i64)
declare {i64, i1} @llvm.ssub.with.overflow.i64(i64, i64)
declare {i64, i1} @llvm.smul.with.overflow.i64(i64, i64)
declare i32 @tonic_start(i32, ptr, ptr, i64, ptr, ptr, i64, ptr)
declare ptr @tn_alloc(ptr, i64)
declare i64 @tn_yield(ptr)
declare ptr @tn_lookup_mfa(ptr, i64, i64, i64)
declare void @tn_undef_args(ptr, ptr, i64)
declare ptr @tn_lookup_mfa_q(ptr, i64, i64, i64)
declare void @tn_undef_raise(ptr, i64, i64, ptr, i64)
declare void @tn_set_ext_tramps(ptr, i64)
declare i64 @tn_map_build(ptr, ptr, i64)
declare i64 @tn_map_put_many(ptr, i64, ptr, i64)
declare i64 @tn_map_update_many(ptr, i64, ptr, i64)
declare i64 @tn_bin_concat_n(ptr, ptr, i64)
declare i64 @tn_bad_fun(ptr, i64, ptr, i64)
declare i64 @tn_raise_function_clause(ptr, i64, i64, ptr, i64)
declare i64 @tn_raise_case_clause(ptr, i64)
declare i64 @tn_raise_match(ptr, i64)
declare i64 @tn_raise_with_clause(ptr, i64)
declare i64 @tn_raise_try_clause(ptr, i64)
declare i64 @tn_exc_get(ptr)
declare i64 @tn_exc_reraise(ptr, i64)
declare i64 @tn_recv_next(ptr, i64)
declare i64 @tn_recv_accept(ptr)
declare i64 @tn_map_find(ptr, i64, i64)
declare i64 @tn_bs_seg(ptr, i64, i64, i64)
declare i64 @tn_bs_size(ptr, i64)
declare i64 @tn_bs_int(ptr, i64, i64, i64, i64)
declare i64 @tn_bs_float(ptr, i64, i64, i64, i64)
declare i64 @tn_bs_bin(ptr, i64, i64, i64)
declare i64 @tn_bs_rest(ptr, i64, i64, i64)
declare i64 @tn_bs_concat_n(ptr, ptr, i64)
declare i64 @tn_bs_utf8(ptr, i64, i64)
declare i64 @tn_bs_utfn(ptr, i64, i64, i64)
declare i64 @tn_bs_utfn_bits(ptr, i64, i64)
declare i64 @tn_bs_lit(ptr, i64, i64, i64)
declare i64 @tn_add(ptr, i64, i64)
declare i64 @tn_sub(ptr, i64, i64)
declare i64 @tn_mul(ptr, i64, i64)
declare i64 @tn_div(ptr, i64, i64)
declare i64 @tn_rem(ptr, i64, i64)
declare i64 @tn_eq(ptr, i64, i64)
declare i64 @tn_ne(ptr, i64, i64)
declare i64 @tn_eqx(ptr, i64, i64)
declare i64 @tn_nex(ptr, i64, i64)
declare i64 @tn_lt(ptr, i64, i64)
declare i64 @tn_gt(ptr, i64, i64)
declare i64 @tn_le(ptr, i64, i64)
declare i64 @tn_ge(ptr, i64, i64)
declare i64 @tn_is_integer(ptr, i64)
declare i64 @tn_is_number(ptr, i64)
declare i64 @tn_hd(ptr, i64)
declare i64 @tn_tl(ptr, i64)
declare i64 @tn_elem(ptr, i64, i64)
declare i64 @tn_tuple_size(ptr, i64)
declare i64 @tn_map_size(ptr, i64)
declare i64 @tn_not(ptr, i64)

"#;

const INLINE_HELPERS: &str = r#"
define internal i1 @tn_truthy(i64 %v) alwaysinline {
  %a = icmp ne i64 %v, 2
  %b = icmp ne i64 %v, 18
  %c = and i1 %a, %b
  ret i1 %c
}

define internal i1 @tn_is_ptr(i64 %v) alwaysinline {
  %l = and i64 %v, 7
  %a = icmp eq i64 %l, 0
  %b = icmp ne i64 %v, 0
  %c = and i1 %a, %b
  ret i1 %c
}

define internal i64 @tn_tag(i64 %v) alwaysinline {
  %p = inttoptr i64 %v to ptr
  %h = load i64, ptr %p
  %t = and i64 %h, 255
  ret i64 %t
}

define internal i1 @tn_has_header(i64 %v, i64 %hdr) alwaysinline {
entry:
  %ip = call i1 @tn_is_ptr(i64 %v)
  br i1 %ip, label %chk, label %no
chk:
  %p = inttoptr i64 %v to ptr
  %h = load i64, ptr %p
  %e = icmp eq i64 %h, %hdr
  ret i1 %e
no:
  ret i1 false
}

define internal i1 @tn_has_tag(i64 %v, i64 %tag) alwaysinline {
entry:
  %ip = call i1 @tn_is_ptr(i64 %v)
  br i1 %ip, label %chk, label %no
chk:
  %t = call i64 @tn_tag(i64 %v)
  %e = icmp eq i64 %t, %tag
  ret i1 %e
no:
  ret i1 false
}

define internal i1 @tn_is_bin_raw(i64 %v) alwaysinline {
entry:
  %ip = call i1 @tn_is_ptr(i64 %v)
  br i1 %ip, label %chk, label %no
chk:
  %t = call i64 @tn_tag(i64 %v)
  %a = icmp eq i64 %t, 5
  %b = icmp eq i64 %t, 11
  %c = or i1 %a, %b
  ret i1 %c
no:
  ret i1 false
}

define internal i1 @tn_is_fun_arity(i64 %f, i64 %n) alwaysinline {
entry:
  %ok = call i1 @tn_has_tag(i64 %f, i64 8)
  br i1 %ok, label %chk, label %no
chk:
  %p = inttoptr i64 %f to ptr
  %ap = getelementptr i64, ptr %p, i64 2
  %a = load i64, ptr %ap
  %e = icmp eq i64 %a, %n
  ret i1 %e
no:
  ret i1 false
}

define internal i64 @tn_bool(i1 %b) alwaysinline {
  %r = select i1 %b, i64 10, i64 18
  ret i64 %r
}

define internal i1 @tn_both_small(i64 %a, i64 %b) alwaysinline {
  %t = and i64 %a, %b
  %t1 = and i64 %t, 1
  %r = icmp ne i64 %t1, 0
  ret i1 %r
}

define internal i64 @tn_add_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  br i1 %s, label %fast, label %slow
fast:
  %b1 = sub i64 %b, 1
  %r = call {i64, i1} @llvm.sadd.with.overflow.i64(i64 %a, i64 %b1)
  %v = extractvalue {i64, i1} %r, 0
  %o = extractvalue {i64, i1} %r, 1
  br i1 %o, label %slow, label %done
done:
  ret i64 %v
slow:
  %x = call i64 @tn_add(ptr %ctx, i64 %a, i64 %b)
  ret i64 %x
}

define internal i64 @tn_sub_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  br i1 %s, label %fast, label %slow
fast:
  %b1 = sub i64 %b, 1
  %r = call {i64, i1} @llvm.ssub.with.overflow.i64(i64 %a, i64 %b1)
  %v = extractvalue {i64, i1} %r, 0
  %o = extractvalue {i64, i1} %r, 1
  br i1 %o, label %slow, label %done
done:
  ret i64 %v
slow:
  %x = call i64 @tn_sub(ptr %ctx, i64 %a, i64 %b)
  ret i64 %x
}

define internal i64 @tn_mul_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  br i1 %s, label %fast, label %slow
fast:
  %a1 = ashr i64 %a, 1
  %b1 = sub i64 %b, 1
  %r = call {i64, i1} @llvm.smul.with.overflow.i64(i64 %a1, i64 %b1)
  %v = extractvalue {i64, i1} %r, 0
  %o = extractvalue {i64, i1} %r, 1
  br i1 %o, label %slow, label %done
done:
  %v1 = or i64 %v, 1
  ret i64 %v1
slow:
  %x = call i64 @tn_mul(ptr %ctx, i64 %a, i64 %b)
  ret i64 %x
}

define internal i64 @tn_div_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  %nz = icmp ne i64 %b, 1
  %nm = icmp ne i64 %b, -1
  %ok0 = and i1 %s, %nz
  %ok = and i1 %ok0, %nm
  br i1 %ok, label %fast, label %slow
fast:
  %x = ashr i64 %a, 1
  %y = ashr i64 %b, 1
  %q = sdiv i64 %x, %y
  %q2 = shl i64 %q, 1
  %q3 = or i64 %q2, 1
  ret i64 %q3
slow:
  %r = call i64 @tn_div(ptr %ctx, i64 %a, i64 %b)
  ret i64 %r
}

define internal i64 @tn_rem_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  %nz = icmp ne i64 %b, 1
  %nm = icmp ne i64 %b, -1
  %ok0 = and i1 %s, %nz
  %ok = and i1 %ok0, %nm
  br i1 %ok, label %fast, label %slow
fast:
  %x = ashr i64 %a, 1
  %y = ashr i64 %b, 1
  %q = srem i64 %x, %y
  %q2 = shl i64 %q, 1
  %q3 = or i64 %q2, 1
  ret i64 %q3
slow:
  %r = call i64 @tn_rem(ptr %ctx, i64 %a, i64 %b)
  ret i64 %r
}

define internal i64 @tn_eqx_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %e = icmp eq i64 %a, %b
  br i1 %e, label %yes, label %chk
chk:
  %pa = call i1 @tn_is_ptr(i64 %a)
  %pb = call i1 @tn_is_ptr(i64 %b)
  %pp = and i1 %pa, %pb
  br i1 %pp, label %slow, label %no
yes:
  ret i64 10
no:
  ret i64 18
slow:
  %r = call i64 @tn_eqx(ptr %ctx, i64 %a, i64 %b)
  ret i64 %r
}

define internal i64 @tn_nex_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
  %r = call i64 @tn_eqx_i(ptr %ctx, i64 %a, i64 %b)
  %t = icmp eq i64 %r, 10
  %v = select i1 %t, i64 18, i64 10
  ret i64 %v
}

define internal i64 @tn_eq_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %e = icmp eq i64 %a, %b
  br i1 %e, label %yes, label %chk
chk:
  %pa = call i1 @tn_is_ptr(i64 %a)
  %pb = call i1 @tn_is_ptr(i64 %b)
  %pp = or i1 %pa, %pb
  br i1 %pp, label %slow, label %no
yes:
  ret i64 10
no:
  ret i64 18
slow:
  %r = call i64 @tn_eq(ptr %ctx, i64 %a, i64 %b)
  ret i64 %r
}

define internal i64 @tn_ne_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
  %r = call i64 @tn_eq_i(ptr %ctx, i64 %a, i64 %b)
  %t = icmp eq i64 %r, 10
  %v = select i1 %t, i64 18, i64 10
  ret i64 %v
}

define internal i64 @tn_lt_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  br i1 %s, label %fast, label %slow
fast:
  %c = icmp slt i64 %a, %b
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
slow:
  %x = call i64 @tn_lt(ptr %ctx, i64 %a, i64 %b)
  ret i64 %x
}

define internal i64 @tn_gt_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  br i1 %s, label %fast, label %slow
fast:
  %c = icmp sgt i64 %a, %b
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
slow:
  %x = call i64 @tn_gt(ptr %ctx, i64 %a, i64 %b)
  ret i64 %x
}

define internal i64 @tn_le_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  br i1 %s, label %fast, label %slow
fast:
  %c = icmp sle i64 %a, %b
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
slow:
  %x = call i64 @tn_le(ptr %ctx, i64 %a, i64 %b)
  ret i64 %x
}

define internal i64 @tn_ge_i(ptr %ctx, i64 %a, i64 %b) alwaysinline {
entry:
  %s = call i1 @tn_both_small(i64 %a, i64 %b)
  br i1 %s, label %fast, label %slow
fast:
  %c = icmp sge i64 %a, %b
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
slow:
  %x = call i64 @tn_ge(ptr %ctx, i64 %a, i64 %b)
  ret i64 %x
}

define internal i64 @tn_is_atom_i(ptr %ctx, i64 %a) alwaysinline {
  %l = and i64 %a, 7
  %c = icmp eq i64 %l, 2
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
}

define internal i64 @tn_is_pid_i(ptr %ctx, i64 %a) alwaysinline {
  %l = and i64 %a, 7
  %c = icmp eq i64 %l, 6
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
}

define internal i64 @tn_is_boolean_i(ptr %ctx, i64 %a) alwaysinline {
  %x = icmp eq i64 %a, 10
  %y = icmp eq i64 %a, 18
  %c = or i1 %x, %y
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
}

define internal i64 @tn_is_integer_i(ptr %ctx, i64 %a) alwaysinline {
entry:
  %l = and i64 %a, 1
  %s = icmp ne i64 %l, 0
  br i1 %s, label %yes, label %chk
chk:
  %b = call i1 @tn_has_tag(i64 %a, i64 4)
  %r = call i64 @tn_bool(i1 %b)
  ret i64 %r
yes:
  ret i64 10
}

define internal i64 @tn_is_number_i(ptr %ctx, i64 %a) alwaysinline {
entry:
  %l = and i64 %a, 1
  %s = icmp ne i64 %l, 0
  br i1 %s, label %yes, label %chk
chk:
  %b = call i1 @tn_has_tag(i64 %a, i64 4)
  %f = call i1 @tn_has_tag(i64 %a, i64 3)
  %c = or i1 %b, %f
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
yes:
  ret i64 10
}

define internal i64 @tn_is_list_i(ptr %ctx, i64 %a) alwaysinline {
entry:
  %n = icmp eq i64 %a, 4
  br i1 %n, label %yes, label %chk
chk:
  %b = call i1 @tn_has_tag(i64 %a, i64 2)
  %r = call i64 @tn_bool(i1 %b)
  ret i64 %r
yes:
  ret i64 10
}

define internal i64 @tn_is_tuple_i(ptr %ctx, i64 %a) alwaysinline {
  %b = call i1 @tn_has_tag(i64 %a, i64 1)
  %r = call i64 @tn_bool(i1 %b)
  ret i64 %r
}

define internal i64 @tn_is_map_i(ptr %ctx, i64 %a) alwaysinline {
  %b = call i1 @tn_has_tag(i64 %a, i64 6)
  %r = call i64 @tn_bool(i1 %b)
  ret i64 %r
}

define internal i64 @tn_is_float_i(ptr %ctx, i64 %a) alwaysinline {
  %b = call i1 @tn_has_tag(i64 %a, i64 3)
  %r = call i64 @tn_bool(i1 %b)
  ret i64 %r
}

define internal i64 @tn_is_function_i(ptr %ctx, i64 %a) alwaysinline {
  %b = call i1 @tn_has_tag(i64 %a, i64 8)
  %r = call i64 @tn_bool(i1 %b)
  ret i64 %r
}

define internal i64 @tn_is_bitstring_i(ptr %ctx, i64 %a) alwaysinline {
entry:
  %ip = call i1 @tn_is_ptr(i64 %a)
  br i1 %ip, label %chk, label %no
chk:
  %t = call i64 @tn_tag(i64 %a)
  %x = icmp eq i64 %t, 5
  %y = icmp eq i64 %t, 11
  %z = icmp eq i64 %t, 12
  %xy = or i1 %x, %y
  %c = or i1 %xy, %z
  %r = call i64 @tn_bool(i1 %c)
  ret i64 %r
no:
  %f = call i64 @tn_bool(i1 false)
  ret i64 %f
}

define internal i64 @tn_is_binary_i(ptr %ctx, i64 %a) alwaysinline {
  %b = call i1 @tn_is_bin_raw(i64 %a)
  %r = call i64 @tn_bool(i1 %b)
  ret i64 %r
}

define internal i64 @tn_hd_i(ptr %ctx, i64 %a) alwaysinline {
entry:
  %b = call i1 @tn_has_tag(i64 %a, i64 2)
  br i1 %b, label %fast, label %slow
fast:
  %p = inttoptr i64 %a to ptr
  %hp = getelementptr i64, ptr %p, i64 1
  %h = load i64, ptr %hp
  ret i64 %h
slow:
  %r = call i64 @tn_hd(ptr %ctx, i64 %a)
  ret i64 %r
}

define internal i64 @tn_tl_i(ptr %ctx, i64 %a) alwaysinline {
entry:
  %b = call i1 @tn_has_tag(i64 %a, i64 2)
  br i1 %b, label %fast, label %slow
fast:
  %p = inttoptr i64 %a to ptr
  %tp = getelementptr i64, ptr %p, i64 2
  %t = load i64, ptr %tp
  ret i64 %t
slow:
  %r = call i64 @tn_tl(ptr %ctx, i64 %a)
  ret i64 %r
}

define internal i64 @tn_elem_i(ptr %ctx, i64 %t, i64 %i) alwaysinline {
entry:
  %b = call i1 @tn_has_tag(i64 %t, i64 1)
  %il = and i64 %i, 1
  %is = icmp ne i64 %il, 0
  %ok0 = and i1 %b, %is
  br i1 %ok0, label %chk, label %slow
chk:
  %p = inttoptr i64 %t to ptr
  %h = load i64, ptr %p
  %n = lshr i64 %h, 8
  %idx = ashr i64 %i, 1
  %inr = icmp ult i64 %idx, %n
  br i1 %inr, label %fast, label %slow
fast:
  %idx1 = add i64 %idx, 1
  %ep = getelementptr i64, ptr %p, i64 %idx1
  %e = load i64, ptr %ep
  ret i64 %e
slow:
  %r = call i64 @tn_elem(ptr %ctx, i64 %t, i64 %i)
  ret i64 %r
}

define internal i64 @tn_tuple_size_i(ptr %ctx, i64 %t) alwaysinline {
entry:
  %b = call i1 @tn_has_tag(i64 %t, i64 1)
  br i1 %b, label %fast, label %slow
fast:
  %p = inttoptr i64 %t to ptr
  %h = load i64, ptr %p
  %n = lshr i64 %h, 8
  %n2 = shl i64 %n, 1
  %n3 = or i64 %n2, 1
  ret i64 %n3
slow:
  %r = call i64 @tn_tuple_size(ptr %ctx, i64 %t)
  ret i64 %r
}

define internal i64 @tn_map_size_i(ptr %ctx, i64 %m) alwaysinline {
entry:
  %b = call i1 @tn_has_tag(i64 %m, i64 6)
  br i1 %b, label %fast, label %slow
fast:
  %p = inttoptr i64 %m to ptr
  %cp = getelementptr i64, ptr %p, i64 1
  %n = load i64, ptr %cp
  %n2 = shl i64 %n, 1
  %n3 = or i64 %n2, 1
  ret i64 %n3
slow:
  %r = call i64 @tn_map_size(ptr %ctx, i64 %m)
  ret i64 %r
}

define internal i64 @tn_not_i(ptr %ctx, i64 %a) alwaysinline {
entry:
  %t = icmp eq i64 %a, 10
  br i1 %t, label %f, label %c
c:
  %fl = icmp eq i64 %a, 18
  br i1 %fl, label %tr, label %slow
f:
  ret i64 18
tr:
  ret i64 10
slow:
  %r = call i64 @tn_not(ptr %ctx, i64 %a)
  ret i64 %r
}

define internal i64 @tn_utf8_len_i(i64 %cp) alwaysinline {
  %v = ashr i64 %cp, 1
  %a = icmp slt i64 %v, 128
  %b = icmp slt i64 %v, 2048
  %c = icmp slt i64 %v, 65536
  %r3 = select i1 %c, i64 7, i64 9
  %r2 = select i1 %b, i64 5, i64 %r3
  %r1 = select i1 %a, i64 3, i64 %r2
  ret i64 %r1
}

"#;

#[allow(dead_code)]
fn _unused(_: BTreeSet<u8>) {}

/// elixir_rewrite's inline table: (module, fun, arity) -> (erl module, fun).
const EXT_FUN_MAX_ARITY: usize = 12;

pub(crate) fn inline_target(module: &str, name: &str, arity: usize) -> Option<(&'static str, &'static str)> {
    static TABLE: &str = include_str!("../data/inline_funs.txt");
    let ar = arity.to_string();
    TABLE.lines().find_map(|l| {
        let mut it = l.split(' ');
        let (m, f, n, em, ef) = (it.next()?, it.next()?, it.next()?, it.next()?, it.next()?);
        if m == module && f == name && n == ar {
            Some((em, ef))
        } else {
            None
        }
    })
}
