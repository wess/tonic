//! Phase 1: walk parsed files, collect modules, functions, structs,
//! protocols and implementations. Module attributes are substituted here.

use crate::ast::*;
use crate::parser::parse_source;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

pub type R<T> = Result<T, String>;

#[derive(Clone, Debug)]
pub struct Import {
    pub module: String,
    pub only: Option<Vec<(String, usize)>>,
    pub except: Vec<(String, usize)>,
}

#[derive(Clone, Debug)]
pub struct ModEnv {
    pub module: String,
    pub aliases: HashMap<String, String>,
    pub imports: Vec<Import>,
    pub file: String,
}

#[derive(Clone, Debug)]
pub struct DefC {
    pub name: String,
    pub arity: usize,
    pub public: bool,
    pub args: Vec<E>,
    pub guard: Option<E>,
    pub body: Option<E>,
    pub env: Rc<ModEnv>,
    pub line: u32,
    pub is_macro: bool,
    pub super_target: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub name: String,
    pub file: String,
    pub defs: Vec<DefC>,
    pub struct_fields: Option<Vec<(String, E)>>,
    pub enforce: Vec<String>,
    pub exception: bool,
    pub guards: HashMap<(String, usize), (Vec<String>, E)>,
    pub derive: Vec<String>,
    pub is_prelude: bool,
    pub line: u32,
    pub env: Option<Rc<ModEnv>>,
    pub uses: Vec<String>,
    pub exunit: Option<crate::exunit::ExUnitState>,
    /// Defined by a `defmodule` inside a function body (compile errors in
    /// its functions are deferred to runtime, as Elixir would raise them).
    pub hoisted: bool,
    pub overridable: HashSet<(String, usize)>,
    pub super_targets: HashMap<(String, usize), String>,
    pub after_compile: bool,
    pub callbacks: HashSet<(String, usize)>,
}

#[derive(Clone, Debug, Default)]
pub struct Protocol {
    pub name: String,
    pub funs: Vec<(String, usize)>,
    pub fallback_any: bool,
    pub impls: Vec<String>,
}

pub struct Collector {
    pub mods: BTreeMap<String, Module>,
    pub protocols: BTreeMap<String, Protocol>,
    pub script: Vec<(E, Rc<ModEnv>)>,
    pub warnings: Vec<String>,
    pub prelude: bool,
    /// Standard library modules the program redefines.
    pub redefined: Vec<String>,
    /// Modules that use ExUnit.Case, in definition order.
    pub exunit_modules: Vec<String>,
}

pub fn elixir_name(a: &str) -> String {
    if a.starts_with("Elixir.") || a == "Elixir" {
        a.to_string()
    } else {
        format!("Elixir.{}", a)
    }
}

/// Resolve an alias using the environment.
pub fn resolve_alias(env: &ModEnv, a: &str) -> String {
    if a == "Elixir" {
        return "Elixir".into();
    }
    if let Some(rest) = a.strip_prefix("Elixir.") {
        return format!("Elixir.{}", rest);
    }
    let (first, rest) = match a.find('.') {
        Some(i) => (&a[..i], Some(&a[i + 1..])),
        None => (a, None),
    };
    if let Some(full) = env.aliases.get(first) {
        match rest {
            Some(r) => format!("{}.{}", full, r),
            None => full.clone(),
        }
    } else {
        elixir_name(a)
    }
}

impl Collector {
    pub fn new() -> Collector {
        Collector {
            mods: BTreeMap::new(),
            protocols: BTreeMap::new(),
            script: Vec::new(),
            warnings: Vec::new(),
            prelude: false,
            redefined: Vec::new(),
            exunit_modules: Vec::new(),
        }
    }

    pub fn add_file(&mut self, src: &str, file: &str, prelude: bool) -> R<()> {
        let t = std::time::Instant::now();
        let es = if file.ends_with(".erl") {
            {
                // Includes: tonic's bundled headers, else next to the source
                // file (or in its ../include).
                let dir = std::path::Path::new(file)
                    .parent()
                    .map(|d| d.to_path_buf())
                    .unwrap_or_default();
                let inc = move |name: &str| -> Option<String> {
                    crate::prelude::erl_include(name)
                        .or_else(|| std::fs::read_to_string(dir.join(name)).ok())
                        .or_else(|| {
                            std::fs::read_to_string(dir.join("..").join("include").join(name)).ok()
                        })
                };
                vec![crate::erl_trans::compile_erl(src, file, &inc)?]
            }
        } else {
            parse_source(src, file)?
        };
        if std::env::var("TONIC_TIME").is_ok() {
            eprintln!("parse {} {:?}", file, t.elapsed());
        }
        self.prelude = prelude;
        let mut env = ModEnv {
            module: "Elixir.Tonic.Script".into(),
            aliases: HashMap::new(),
            imports: Vec::new(),
            file: file.to_string(),
        };
        let mut attrs: HashMap<String, E> = HashMap::new();
        for e in es {
            self.top_stmt(e, &mut env, &mut attrs, prelude)?;
        }
        Ok(())
    }

    fn top_stmt(
        &mut self,
        e: E,
        env: &mut ModEnv,
        attrs: &mut HashMap<String, E>,
        prelude: bool,
    ) -> R<()> {
        if let K::Call { name, args, .. } = &e.k {
            match name.as_str() {
                "defmodule" | "defprotocol" | "defimpl" => {
                    let before = self.exunit_modules.len();
                    self.module_stmt(&e, env, None, attrs)?;
                    // ExUnit.Case.__after_compile__/2 registers test modules
                    // with ExUnit.Server where the module is defined.
                    if !prelude {
                        let l = e.line;
                        for m in self.exunit_modules[before..].to_vec() {
                            if self.mods.get(&m).map(|x| x.hoisted).unwrap_or(false) {
                                continue;
                            }
                            let call = E::new(
                                K::Remote {
                                    recv: Box::new(E::new(K::Alias("ExUnit.Case".into()), l)),
                                    name: "__after_compile__".into(),
                                    args: vec![
                                        E::new(
                                            K::Map(vec![(E::atom("module", l), E::atom(&m, l))]),
                                            l,
                                        ),
                                        E::nil(l),
                                    ],
                                    parens: true,
                                },
                                l,
                            );
                            self.script.push((call, Rc::new(env.clone())));
                        }
                    }
                    return Ok(());
                }
                "if" | "unless" if args.len() == 2 && args[1].kw_get("do").is_some() => {
                    let mut definitions = false;
                    map_expr(&e, &mut |node| {
                        if matches!(&node.k, K::Call { name, .. } if matches!(name.as_str(), "defmodule" | "defprotocol" | "defimpl"))
                        {
                            definitions = true;
                        }
                        None
                    });
                    if definitions {
                        let vars = crate::ctime::Vars::new();
                        let value = crate::ctime::eval_bool(&args[0], &vars, attrs)
                            .map(|value| E::atom(if value { "true" } else { "false" }, e.line))
                            .or_else(|| crate::ctime::eval(&args[0], &vars, attrs));
                        let value = match value {
                            Some(value) => value,
                            None => crate::macrohost::evaluate(&args[0], &env.file, e.line)?,
                        };
                        let truth =
                            !matches!(&value.k, K::Atom(atom) if atom == "false" || atom == "nil");
                        let branch = args[1].kw_get(if truth == (name == "if") {
                            "do"
                        } else {
                            "else"
                        });
                        if let Some(branch) = branch {
                            let statements = match &branch.k {
                                K::Block(statements) => statements.clone(),
                                _ => vec![branch.clone()],
                            };
                            for statement in statements {
                                self.top_stmt(statement, env, attrs, prelude)?;
                            }
                        }
                        return Ok(());
                    }
                }
                "alias" | "import" | "require" => {
                    self.env_stmt(name, args, env, e.line)?;
                    if !prelude {
                        // also keep for runtime semantics (no-op)
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        if prelude {
            return Err(format!(
                "{}:{}: prelude files may only contain modules",
                env.file, e.line
            ));
        }
        let e = subst_attrs(&e, attrs, &mut self.warnings, &env.file);
        self.script.push((e, Rc::new(env.clone())));
        Ok(())
    }

    /// Process alias/import/require statements.
    pub fn env_stmt(&mut self, name: &str, args: &[E], env: &mut ModEnv, line: u32) -> R<()> {
        match name {
            "alias" => {
                let target = &args[0];
                let opts = args.get(1);
                match &target.k {
                    K::Alias(a) => {
                        let full = resolve_alias(env, a);
                        let as_name = match opts.and_then(|o| o.kw_get("as")) {
                            Some(E { k: K::Alias(n), .. }) => n.clone(),
                            _ => full.rsplit('.').next().unwrap().to_string(),
                        };
                        env.aliases.insert(as_name, full);
                    }
                    K::MultiAlias(base, items) => {
                        let base = match &base.k {
                            K::Alias(a) => resolve_alias(env, a),
                            K::Var(v) if v == "__MODULE__" => env.module.clone(),
                            _ => return Err(format!("{}:{}: invalid alias", env.file, line)),
                        };
                        for it in items {
                            if let K::Alias(a) = &it.k {
                                let full = format!("{}.{}", base, a);
                                let last = a.rsplit('.').next().unwrap().to_string();
                                env.aliases.insert(last, full);
                            }
                        }
                    }
                    K::Var(v) if v == "__MODULE__" => {
                        let last = env.module.rsplit('.').next().unwrap().to_string();
                        let m = env.module.clone();
                        env.aliases.insert(last, m);
                    }
                    K::Bin { op, l, r } if op == ".alias" => {
                        if let (K::Var(v), K::Alias(a)) = (&l.k, &r.k) {
                            if v == "__MODULE__" {
                                let full = format!("{}.{}", env.module, a);
                                let as_name = match opts.and_then(|o| o.kw_get("as")) {
                                    Some(E { k: K::Alias(n), .. }) => n.clone(),
                                    _ => a.rsplit('.').next().unwrap().to_string(),
                                };
                                env.aliases.insert(as_name, full);
                            }
                        }
                    }
                    _ => {}
                }
            }
            "import" => {
                let module = match &args[0].k {
                    K::Alias(a) => resolve_alias(env, a),
                    K::Atom(a) => a.clone(),
                    _ => return Err(format!("{}:{}: invalid import", env.file, line)),
                };
                let mut imp = Import {
                    module,
                    only: None,
                    except: vec![],
                };
                if let Some(opts) = args.get(1) {
                    if let Some(only) = opts.kw_get("only") {
                        if let Some(kw) = only.as_keyword() {
                            imp.only = Some(
                                kw.iter()
                                    .filter_map(|(k, v)| match &v.k {
                                        K::Int(n) => Some((k.clone(), n.parse().unwrap_or(0))),
                                        _ => None,
                                    })
                                    .collect(),
                            );
                        }
                    }
                    if let Some(ex) = opts.kw_get("except") {
                        if let Some(kw) = ex.as_keyword() {
                            imp.except = kw
                                .iter()
                                .filter_map(|(k, v)| match &v.k {
                                    K::Int(n) => Some((k.clone(), n.parse().unwrap_or(0))),
                                    _ => None,
                                })
                                .collect();
                        }
                    }
                }
                env.imports.retain(|i| i.module != imp.module);
                env.imports.push(imp);
            }
            "require" => {
                if let Some(opts) = args.get(1) {
                    if let (K::Alias(a), Some(E { k: K::Alias(n), .. })) =
                        (&args[0].k, opts.kw_get("as"))
                    {
                        let full = resolve_alias(env, a);
                        env.aliases.insert(n.clone(), full);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn module_name(&self, e: &E, env: &ModEnv, parent: Option<&str>) -> R<String> {
        match &e.k {
            K::Alias(a) => Ok(match parent {
                Some(p) => {
                    // nested: an alias to an existing module is resolved; otherwise nest
                    let first = a.split('.').next().unwrap();
                    if let Some(full) = env.aliases.get(first) {
                        if !full.starts_with(&format!("{}.", p)) {
                            format!("{}.{}", p, a)
                        } else {
                            resolve_alias(env, a)
                        }
                    } else {
                        format!("{}.{}", p, a)
                    }
                }
                None => resolve_alias(env, a),
            }),
            K::Atom(a) => Ok(a.clone()),
            K::Var(v) if v == "__MODULE__" => Ok(env.module.clone()),
            K::Bin { op, l, r } if op == ".alias" => {
                let base = self.module_name(l, env, None)?;
                if let K::Alias(a) = &r.k {
                    Ok(format!("{}.{}", base, a))
                } else {
                    Err("bad module name".into())
                }
            }
            _ => Err(format!("{}:{}: invalid module name", env.file, e.line)),
        }
    }

    fn new_module(&mut self, name: &str, file: &str, line: u32) {
        let prelude = self.prelude;
        // A program redefining a standard library module replaces it.
        if !prelude && self.mods.get(name).map(|m| m.is_prelude).unwrap_or(false) {
            self.mods.remove(name);
            self.redefined.push(name.to_string());
        }
        self.mods.entry(name.to_string()).or_insert_with(|| Module {
            name: name.to_string(),
            file: file.to_string(),
            defs: Vec::new(),
            struct_fields: None,
            enforce: Vec::new(),
            exception: false,
            guards: HashMap::new(),
            derive: Vec::new(),
            is_prelude: prelude,
            line,
            env: None,
            uses: Vec::new(),
            exunit: None,
            hoisted: false,
            overridable: HashSet::new(),
            super_targets: HashMap::new(),
            after_compile: false,
            callbacks: HashSet::new(),
        });
    }

    /// defmodule / defprotocol / defimpl at top level or nested.
    fn module_stmt(
        &mut self,
        e: &E,
        outer: &mut ModEnv,
        parent: Option<&str>,
        attrs: &HashMap<String, E>,
    ) -> R<()> {
        let (kind, args) = match &e.k {
            K::Call { name, args, .. } => (name.as_str(), args),
            _ => unreachable!(),
        };
        let file = outer.file.clone();
        let body_kw = args
            .last()
            .ok_or_else(|| format!("{}:{}: missing do block", file, e.line))?;
        let body = body_kw
            .kw_get("do")
            .cloned()
            .ok_or_else(|| format!("{}:{}: missing do block", file, e.line))?;
        match kind {
            "defmodule" => {
                let name = self.module_name(&args[0], outer, parent)?;
                // alias nested module in the enclosing scope
                if parent.is_some() {
                    if let K::Alias(a) = &args[0].k {
                        let first = a.split('.').next().unwrap().to_string();
                        let full = format!("{}.{}", parent.unwrap(), first);
                        outer.aliases.insert(first, full);
                    }
                }
                self.new_module(&name, &file, e.line);
                let mut env = ModEnv {
                    module: name.clone(),
                    aliases: outer.aliases.clone(),
                    imports: outer.imports.clone(),
                    file: file.clone(),
                };
                let mut mattrs = attrs.clone();
                mattrs.retain(|_, _| false);
                self.module_body(&name, body, &mut env, &mut mattrs)?;
                Ok(())
            }
            "defprotocol" => {
                let name = self.module_name(&args[0], outer, parent)?;
                if parent.is_some() {
                    if let K::Alias(a) = &args[0].k {
                        let first = a.split('.').next().unwrap().to_string();
                        let full = format!("{}.{}", parent.unwrap(), first);
                        outer.aliases.insert(first, full);
                    }
                }
                self.new_module(&name, &file, e.line);
                let mut p = self.protocols.remove(&name).unwrap_or_default();
                p.name = name.clone();
                let stmts = block_stmts(&body);
                let mut penv = ModEnv {
                    module: name.clone(),
                    aliases: outer.aliases.clone(),
                    imports: outer.imports.clone(),
                    file: file.clone(),
                };
                let pattrs: HashMap<String, E> = HashMap::new();
                for s in stmts {
                    match &s.k {
                        // Nested modules/impls and helper functions.
                        K::Call { name: dn, .. }
                            if matches!(dn.as_str(), "defimpl" | "defmodule") =>
                        {
                            self.module_stmt(&s, &mut penv, Some(&name), &pattrs)?;
                        }
                        K::Call {
                            name: dn,
                            args: dargs,
                            ..
                        } if dn == "defp" || (dn == "def" && dargs.len() == 2) => {
                            self.add_def(&name, dn, dargs, &penv, &pattrs, s.line)?;
                        }
                        K::Call {
                            name: dn,
                            args: dargs,
                            ..
                        } if dn == "def" => {
                            let head = &dargs[0];
                            let (fname, fargs) = match &head.k {
                                K::Call { name, args, .. } => (name.clone(), args.len()),
                                K::Var(n) => (n.clone(), 0),
                                _ => continue,
                            };
                            if !p.funs.contains(&(fname.clone(), fargs)) {
                                p.funs.push((fname, fargs));
                            }
                        }
                        K::Un { op, e: inner } if op == "@" => {
                            if let K::Call {
                                name: an, args: aa, ..
                            } = &inner.k
                            {
                                if an == "fallback_to_any" {
                                    if let Some(E { k: K::Atom(v), .. }) = aa.first() {
                                        p.fallback_any = v == "true";
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                self.protocols.insert(name, p);
                Ok(())
            }
            "defimpl" => {
                let proto = self.module_name(&args[0], outer, None)?;
                let fors: Vec<String> = match args.get(1).and_then(|o| o.kw_get("for")) {
                    Some(E {
                        k: K::List(items, _),
                        ..
                    }) => items
                        .iter()
                        .map(|i| self.module_name(i, outer, None))
                        .collect::<R<Vec<_>>>()?,
                    Some(f) => vec![self.module_name(f, outer, None)?],
                    None => match parent {
                        Some(p) => vec![p.to_string()],
                        None => return Err(format!("{}:{}: defimpl requires for:", file, e.line)),
                    },
                };
                for f in fors {
                    let short = f.strip_prefix("Elixir.").unwrap_or(&f).to_string();
                    let name = format!("{}.{}", proto, short);
                    self.new_module(&name, &file, e.line);
                    let mut env = ModEnv {
                        module: name.clone(),
                        aliases: outer.aliases.clone(),
                        imports: outer.imports.clone(),
                        file: file.clone(),
                    };
                    let mut mattrs: HashMap<String, E> = HashMap::new();
                    mattrs.insert("for".into(), E::atom(&f, e.line));
                    mattrs.insert("protocol".into(), E::atom(&proto, e.line));
                    self.module_body(&name, body.clone(), &mut env, &mut mattrs)?;
                    let p = self.protocols.entry(proto.clone()).or_default();
                    p.name = proto.clone();
                    if !p.impls.contains(&f) {
                        p.impls.push(f.clone());
                    }
                }
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    fn module_body(
        &mut self,
        name: &str,
        body: E,
        env: &mut ModEnv,
        attrs: &mut HashMap<String, E>,
    ) -> R<()> {
        let body_line = body.line;
        let stmts = block_stmts(&body);
        for s in stmts {
            self.module_stmt_inner(name, s, env, attrs)?;
        }
        if !self.prelude && attrs.contains_key("\u{0}callbacks") {
            let expanded = crate::macrohost::callback_expansions(name, &env.file, body_line)?;
            let snapshot = crate::macrohost::callback_attributes(name, &env.file, 0)?;
            if let K::Map(values) = snapshot.k {
                for (key, value) in values {
                    if let K::Atom(name) = key.k {
                        attrs.insert(name, value);
                    }
                }
            }
            for stmt in block_stmts(&expanded) {
                self.module_stmt_inner(name, stmt, env, attrs)?;
            }
        }
        self.exunit_finish(name, env, attrs)?;
        let m = self.mods.get_mut(name).unwrap();
        m.env = Some(Rc::new(env.clone()));
        Ok(())
    }

    pub(crate) fn add_def(
        &mut self,
        module: &str,
        kind: &str,
        args: &[E],
        env: &ModEnv,
        attrs: &HashMap<String, E>,
        line: u32,
    ) -> R<()> {
        let public = kind == "def" || kind == "defmacro";
        let is_macro = kind.starts_with("defmacro");
        let head = &args[0];
        let (head, guard) = match &head.k {
            K::Bin { op, l, r } if op == "when" => {
                ((**l).clone(), Some(flatten_when_guard((**r).clone())))
            }
            _ => (head.clone(), None),
        };
        let (fname, fargs): (String, Vec<E>) = match &head.k {
            K::Call { name, args, .. } => (name.clone(), args.clone()),
            // def unquote(name)(args) after unquote substitution
            K::AnonCall { f, args } if matches!(f.k, K::Atom(_)) => match &f.k {
                K::Atom(a) => (a.clone(), args.clone()),
                _ => unreachable!(),
            },
            K::Atom(a) => (a.clone(), vec![]),
            K::Var(n) => (n.clone(), vec![]),
            K::Bin { op, l, r } => (op.clone(), vec![(**l).clone(), (**r).clone()]),
            K::Un { op, e } => (op.clone(), vec![(**e).clone()]),
            _ => return Err(format!("{}:{}: invalid function head", env.file, line)),
        };
        let body = match args.get(1) {
            Some(kw) => {
                let kws = kw
                    .as_keyword()
                    .ok_or_else(|| format!("{}:{}: invalid def body", env.file, line))?;
                let do_body = kws
                    .iter()
                    .find(|(k, _)| k == "do")
                    .map(|(_, v)| (*v).clone());
                let has_try = kws
                    .iter()
                    .any(|(k, _)| k == "rescue" || k == "catch" || k == "after");
                if has_try {
                    // implicit try
                    Some(E::new(
                        K::Call {
                            name: "try".into(),
                            args: vec![kw.clone()],
                            parens: false,
                        },
                        line,
                    ))
                } else {
                    do_body
                }
            }
            None => None,
        };
        // Modules defined inside function bodies (common in tests) are
        // hoisted: defined at compile time, nested in this module, and the
        // `defmodule` expression evaluates to `{:module, Name, nil, nil}`.
        let mut denv = env.clone();
        let body = match body {
            Some(b) if !is_macro => {
                let mut found: Vec<E> = Vec::new();
                let raise_arg = |msg: E, l: u32| {
                    E::call(
                        "raise",
                        vec![E::new(K::Alias("ArgumentError".into()), l), msg],
                        l,
                    )
                };
                let mut nb = map_expr(&b, &mut |x: &E| match &x.k {
                    K::Call { name, args, .. } if name == "defmodule" && args.len() == 2 => {
                        let l = x.line;
                        let valid = match &args[0].k {
                            K::Alias(_) => true,
                            K::Atom(a) => !matches!(a.as_str(), "nil" | "true" | "false"),
                            K::Var(v) => v == "__MODULE__",
                            K::Bin { op, .. } => op == ".alias",
                            _ => false,
                        };
                        if !valid {
                            let msg = E::bin(
                                "<>",
                                E::new(K::Str(b"invalid module name: ".to_vec()), l),
                                E::call("inspect", vec![args[0].clone()], l),
                                l,
                            );
                            return Some(raise_arg(msg, l));
                        }
                        found.push(x.clone());
                        Some(E::tuple(
                            vec![E::atom("module", l), args[0].clone(), E::nil(l), E::nil(l)],
                            l,
                        ))
                    }
                    _ => None,
                });
                let mut failed: Vec<(u32, String)> = Vec::new();
                for d in found {
                    let before: std::collections::HashSet<String> =
                        self.mods.keys().cloned().collect();
                    let r = self.module_stmt(&d, &mut denv, Some(module), attrs);
                    for (n, m) in self.mods.iter_mut() {
                        if !before.contains(n) {
                            m.hoisted = true;
                        }
                    }
                    if let Err(msg) = r {
                        // Elixir defines these at runtime, so a broken module
                        // only fails when the function runs.
                        self.warnings
                            .push(format!("warning: {} (deferred to runtime)", msg));
                        failed.push((d.line, msg));
                    }
                }
                if !failed.is_empty() {
                    nb = map_expr(&nb, &mut |x: &E| match &x.k {
                        K::Tuple(items)
                            if items.len() == 4
                                && matches!(&items[0].k, K::Atom(a) if a == "module") =>
                        {
                            failed.iter().find(|(l, _)| *l == x.line).map(|(l, m)| {
                                let m = m.splitn(2, ": ").nth(1).unwrap_or(m).to_string();
                                raise_arg(E::new(K::Str(m.into_bytes()), *l), *l)
                            })
                        }
                        _ => None,
                    });
                }
                Some(nb)
            }
            other => other,
        };
        let env = &denv;
        let mut warnings = std::mem::take(&mut self.warnings);
        let sub = |e: &E, w: &mut Vec<String>| subst_attrs(e, attrs, w, &env.file);
        let mut fargs: Vec<E> = fargs.iter().map(|a| sub(a, &mut warnings)).collect();
        let guard = guard.map(|g| sub(&g, &mut warnings));
        let body = body.map(|b| sub(&b, &mut warnings));
        self.warnings = warnings;
        let m = self.mods.get_mut(module).unwrap();
        let key = (fname.clone(), fargs.len());
        if m.overridable.remove(&key) {
            let hidden = format!("__super__{}__{}__{}", fname, fargs.len(), m.defs.len());
            if let Some(previous) = m
                .defs
                .iter()
                .find(|d| d.name == fname && d.arity == fargs.len())
            {
                for (arg, old) in fargs.iter_mut().zip(&previous.args) {
                    if let K::Bin { op, r, .. } = &old.k {
                        if op == "\\\\" && !matches!(&arg.k, K::Bin { op, .. } if op == "\\\\") {
                            *arg = E::bin(op, arg.clone(), (**r).clone(), arg.line);
                        }
                    }
                }
            }
            for old in &mut m.defs {
                if old.name == fname && old.arity == fargs.len() {
                    old.name = hidden.clone();
                    old.public = false;
                }
            }
            m.super_targets.insert(key.clone(), hidden);
        }
        let super_target = m.super_targets.get(&key).cloned();
        let d = DefC {
            name: fname,
            arity: fargs.len(),
            public,
            args: fargs,
            guard,
            body,
            env: Rc::new(env.clone()),
            line,
            is_macro,
            super_target,
        };
        self.mods.get_mut(module).unwrap().defs.push(d);
        Ok(())
    }

    fn constant_attribute(
        &self,
        expression: &E,
        env: &ModEnv,
        vars: &crate::ctime::Vars,
        attrs: &HashMap<String, E>,
    ) -> Option<E> {
        if let Some(value) = crate::ctime::eval(expression, vars, attrs) {
            return Some(value);
        }
        let (receiver, name, arguments) = match &expression.k {
            K::Remote {
                recv, name, args, ..
            } => (recv, name, args),
            _ => return None,
        };
        let target = match &receiver.k {
            K::Alias(name) => resolve_alias(env, name),
            _ => return None,
        };
        let definition = self.mods.get(&target)?.defs.iter().find(|definition| {
            !definition.is_macro
                && definition.name == *name
                && definition.arity == arguments.len()
                && definition.guard.is_none()
        })?;
        let values = arguments
            .iter()
            .map(|argument| crate::ctime::eval(argument, vars, attrs))
            .collect::<Option<Vec<_>>>()?;
        let mut bindings = crate::ctime::Vars::new();
        for (pattern, value) in definition.args.iter().zip(&values) {
            if !crate::ctime::bind(pattern, value, &mut bindings) {
                return None;
            }
        }
        crate::ctime::eval(definition.body.as_ref()?, &bindings, attrs)
    }

    pub(crate) fn module_stmt_inner(
        &mut self,
        module: &str,
        s: E,
        env: &mut ModEnv,
        attrs: &mut HashMap<String, E>,
    ) -> R<()> {
        let line = s.line;
        // Module-level variables (bound to literal values) for unquote/1.
        let vars: crate::ctime::Vars = attrs
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix("\u{0}var:")
                    .map(|n| (n.to_string(), v.clone()))
            })
            .collect();
        let s = if matches!(&s.k, K::Call { name, .. } if matches!(name.as_str(), "def" | "defp" | "defmacro" | "defmacrop" | "defguard" | "defguardp" | "defdelegate"))
            && has_unquote(&s)
        {
            crate::ctime::subst_unquote(&s, &vars, attrs)
        } else {
            s
        };
        match &s.k {
            K::Bin { op, l, r } if op == "=" => {
                if let Some((value, generated)) = crate::ctime::unroll_reduce(r, &vars, attrs) {
                    for (statement, bindings) in generated {
                        self.module_stmt_inner(
                            module,
                            crate::ctime::subst_unquote(&statement, &bindings, attrs),
                            env,
                            attrs,
                        )?;
                    }
                    let mut out = crate::ctime::Vars::new();
                    if !crate::ctime::bind(l, &value, &mut out) {
                        return Err(format!(
                            "{}:{}: compile-time reduction pattern did not match",
                            env.file, line
                        ));
                    }
                    for (key, value) in out {
                        attrs.insert(format!("\u{0}var:{}", key), value);
                    }
                    return Ok(());
                }
                let value = match crate::ctime::eval(r, &vars, attrs) {
                    Some(value) => Some(value),
                    None if !self.prelude => Some(
                        crate::macrohost::attribute_value(module, "tonic_binding", &env.file, line)
                            .map_err(|error| {
                                format!(
                                    "{}:{}: unsupported module-level assignment: {}",
                                    env.file, line, error
                                )
                            })?,
                    ),
                    None => None,
                };
                match value {
                    Some(v) => {
                        let mut out = crate::ctime::Vars::new();
                        if crate::ctime::bind(l, &v, &mut out) {
                            for (k, v) in out {
                                attrs.insert(format!("\u{0}var:{}", k), v);
                            }
                            return Ok(());
                        }
                    }
                    None => {}
                }
                return Err(format!("{}:{}: unsupported module-level assignment: expression cannot be evaluated at compile time", env.file, line));
            }
            K::Call { name, args, .. } if name == "for" && !args.is_empty() => {
                match crate::ctime::unroll_for(args, &vars, attrs) {
                    Some(items) => {
                        for (stmt, env_vars) in items {
                            let saved: Vec<(String, Option<E>)> = env_vars
                                .keys()
                                .map(|k| {
                                    let key = format!("\u{0}var:{}", k);
                                    let old = attrs.get(&key).cloned();
                                    (key, old)
                                })
                                .collect();
                            for (k, v) in &env_vars {
                                attrs.insert(format!("\u{0}var:{}", k), v.clone());
                            }
                            let stmt = crate::ctime::subst_unquote(&stmt, &env_vars, attrs);
                            let r = self.module_stmt_inner(module, stmt, env, attrs);
                            for (k, old) in saved {
                                match old {
                                    Some(v) => attrs.insert(k, v),
                                    None => attrs.remove(&k),
                                };
                            }
                            r?;
                        }
                    }
                    None => return Err(format!("{}:{}: unsupported module-level for: generator cannot be evaluated at compile time", env.file, line)),
                }
                return Ok(());
            }
            K::Call { name, args, .. }
                if name != "defstruct"
                    && !name.starts_with("def")
                    && args.iter().any(has_defstruct) =>
            {
                return Err(format!(
                    "{}:{}: unsupported executable expression around defstruct",
                    env.file, line
                ));
            }
            K::Un { op, e } if op == "@" && matches!(e.k, K::Alias(_)) => {
                return Err(format!(
                    "{}:{}: module attributes set via @ cannot start with an uppercase letter",
                    env.file, line
                ));
            }
            K::Call { name, args, .. }
                if (name == "if" || name == "unless")
                    && args.len() == 2
                    && args[1].kw_get("do").is_some() =>
            {
                let truth = match crate::ctime::eval_bool(&args[0], &vars, attrs)
                    .map(|b| E::atom(if b { "true" } else { "false" }, line))
                    .or_else(|| crate::ctime::eval(&args[0], &vars, attrs))
                {
                    Some(E { k: K::Atom(a), .. }) => !(a == "nil" || a == "false"),
                    Some(_) => true,
                    None => {
                        if self.prelude {
                            return Err(format!(
                                "{}:{}: unsupported compile-time module condition",
                                env.file, line
                            ));
                        } else {
                            let condition = map_expr(&args[0], &mut |node| match &node.k {
                                K::Var(name) => vars.get(name).cloned(),
                                K::Attr(name) => attrs.get(name).cloned(),
                                _ => None,
                            });
                            let value = crate::macrohost::evaluate(&condition, &env.file, line)?;
                            !matches!(&value.k, K::Atom(atom) if atom == "false" || atom == "nil")
                        }
                    }
                };
                let take_do = truth == (name == "if");
                let branch = if take_do {
                    args[1].kw_get("do").cloned()
                } else {
                    args[1].kw_get("else").cloned()
                };
                if let Some(b) = branch {
                    let stmts = match b.k {
                        K::Block(es) => es,
                        _ => vec![b],
                    };
                    for st in stmts {
                        self.module_stmt_inner(module, st, env, attrs)?;
                    }
                }
                return Ok(());
            }
            _ => {}
        }
        match &s.k {
            K::Call { name, args, .. } => match name.as_str() {
                "def" | "defp" | "defmacro" | "defmacrop" => {
                    if args.is_empty() {
                        return Err(format!("{}:{}: invalid def", env.file, line));
                    }
                    self.add_def(module, name, args, env, attrs, line)?;
                    if !self.prelude && attrs.contains_key("\u{0}on_definition") {
                        let snapshot =
                            crate::macrohost::callback_attributes(module, &env.file, line)?;
                        if let K::Map(values) = snapshot.k {
                            for (key, value) in values {
                                if let K::Atom(name) = key.k {
                                    attrs.insert(name, value);
                                }
                            }
                        }
                    }
                }
                "defdelegate" => {
                    // defdelegate f(a, b), to: Mod, as: name
                    let head = &args[0];
                    let opts = args.get(1).ok_or("defdelegate requires to:")?;
                    let (fname, fargs) = match &head.k {
                        K::Call { name, args, .. } => (name.clone(), args.clone()),
                        K::Var(n) => (n.clone(), vec![]),
                        _ => return Err(format!("{}:{}: invalid defdelegate", env.file, line)),
                    };
                    let to = opts
                        .kw_get("to")
                        .cloned()
                        .ok_or("defdelegate requires to:")?;
                    let as_name = match opts.kw_get("as") {
                        Some(E { k: K::Atom(a), .. }) => a.clone(),
                        _ => fname.clone(),
                    };
                    // strip defaults for the call args
                    let call_args: Vec<E> = fargs
                        .iter()
                        .map(|a| match &a.k {
                            K::Bin { op, l, .. } if op == "\\\\" => (**l).clone(),
                            _ => a.clone(),
                        })
                        .collect();
                    let body = E::new(
                        K::Remote {
                            recv: Box::new(to),
                            name: as_name,
                            args: call_args,
                            parens: true,
                        },
                        line,
                    );
                    let def_args = vec![
                        E::call(&fname, fargs, line),
                        E::list(vec![E::tuple(vec![E::atom("do", line), body], line)], line),
                    ];
                    self.add_def(module, "def", &def_args, env, attrs, line)?;
                }
                "defguard" | "defguardp" => {
                    // defguard name(args) when expr
                    if let K::Bin { op, l, r } = &args[0].k {
                        if op == "when" {
                            if let K::Call {
                                name: gname,
                                args: gargs,
                                ..
                            } = &l.k
                            {
                                let params: Vec<String> = gargs
                                    .iter()
                                    .map(|a| match &a.k {
                                        K::Var(v) => Ok(v.clone()),
                                        _ => Err(format!(
                                            "{}:{}: defguard params must be variables",
                                            env.file, line
                                        )),
                                    })
                                    .collect::<R<_>>()?;
                                let mut w = std::mem::take(&mut self.warnings);
                                let body = subst_attrs(r, attrs, &mut w, &env.file);
                                self.warnings = w;
                                self.mods
                                    .get_mut(module)
                                    .unwrap()
                                    .guards
                                    .insert((gname.clone(), params.len()), (params, body));
                                return Ok(());
                            }
                        }
                    }
                    return Err(format!("{}:{}: invalid defguard", env.file, line));
                }
                "defstruct" => {
                    let fields = struct_fields(&args[0], attrs, &env.file)?;
                    let m = self.mods.get_mut(module).unwrap();
                    m.struct_fields = Some(fields);
                    if let Some(ek) = attrs.get("enforce_keys") {
                        if let K::List(items, _) = &ek.k {
                            m.enforce = items
                                .iter()
                                .filter_map(|i| match &i.k {
                                    K::Atom(a) => Some(a.clone()),
                                    _ => None,
                                })
                                .collect();
                        }
                    }
                    if let Some(d) = attrs.get("derive") {
                        let items = match &d.k {
                            K::List(items, _) => items.clone(),
                            _ => vec![d.clone()],
                        };
                        let mut inspect_opts: Option<E> = None;
                        let mut all_opts: Vec<(E, E)> = Vec::new();
                        for it in items {
                            let (target, opts) = match &it.k {
                                K::Tuple(t) if t.len() == 2 => (t[0].clone(), Some(t[1].clone())),
                                K::Tuple(t) if !t.is_empty() => (t[0].clone(), None),
                                _ => (it.clone(), None),
                            };
                            if let K::Alias(a) = &target.k {
                                let full = resolve_alias(env, a);
                                if let Some(o) = &opts {
                                    all_opts.push((target.clone(), o.clone()));
                                }
                                if full == "Elixir.Inspect" {
                                    inspect_opts = opts;
                                }
                                m.derive.push(full);
                            }
                        }
                        if !m.derive.is_empty() {
                            for (t, o) in all_opts {
                                let def_args = vec![
                                    E::call("__derive_opts__", vec![t], line),
                                    E::list(
                                        vec![E::tuple(vec![E::atom("do", line), o], line)],
                                        line,
                                    ),
                                ];
                                self.add_def(module, "def", &def_args, env, attrs, line)?;
                            }
                            let def_args = vec![
                                E::call("__derive_opts__", vec![E::var("_", line)], line),
                                E::list(
                                    vec![E::tuple(
                                        vec![E::atom("do", line), E::list(vec![], line)],
                                        line,
                                    )],
                                    line,
                                ),
                            ];
                            self.add_def(module, "def", &def_args, env, attrs, line)?;
                        }
                        if let Some(o) = inspect_opts {
                            let def_args = vec![
                                E::call("__inspect_derive__", vec![], line),
                                E::list(vec![E::tuple(vec![E::atom("do", line), o], line)], line),
                            ];
                            self.add_def(module, "def", &def_args, env, attrs, line)?;
                        }
                    }
                }
                "defexception" => {
                    let fields = struct_fields(&args[0], attrs, &env.file)?;
                    let m = self.mods.get_mut(module).unwrap();
                    m.struct_fields = Some(fields);
                    m.exception = true;
                }
                "defmodule" | "defprotocol" | "defimpl" => {
                    let parent = module.to_string();
                    self.module_stmt(&s, env, Some(&parent), attrs)?;
                }
                "alias" | "import" | "require" => {
                    self.env_stmt(name, args, env, line)?;
                }
                "use" => {
                    let target = match &args[0].k {
                        K::Alias(a) => resolve_alias(env, a),
                        _ => return Err(format!("{}:{}: invalid use", env.file, line)),
                    };
                    let opts = args.get(1).cloned();
                    self.do_use(module, &target, opts, env, attrs, line)?;
                }
                "Module" => {}
                "defoverridable" => {
                    // Mark existing definitions overridable: a later definition
                    // with the same name/arity replaces them.
                    let mut fas: Vec<(String, Option<usize>)> = Vec::new();
                    if let Some(a) = args.first() {
                        match a.as_keyword() {
                            Some(kw) => {
                                for (n, ar) in kw {
                                    if let K::Int(s) = &ar.k {
                                        fas.push((n, s.parse().ok()));
                                    }
                                }
                            }
                            None => {
                                let target = self.module_name(a, env, None)?;
                                let behaviour = self.mods.get(&target).ok_or_else(|| {
                                    format!(
                                        "{}:{}: behaviour {} is not available",
                                        env.file, line, target
                                    )
                                })?;
                                fas.extend(
                                    behaviour
                                        .callbacks
                                        .iter()
                                        .filter(|(n, a)| {
                                            self.mods
                                                .get(module)
                                                .unwrap()
                                                .defs
                                                .iter()
                                                .any(|d| &d.name == n && d.arity == *a)
                                        })
                                        .map(|(n, a)| (n.clone(), Some(*a))),
                                );
                            }
                        }
                    }
                    let m = self.mods.get_mut(module).unwrap();
                    for (n, a) in fas {
                        let keys: Vec<_> = m
                            .defs
                            .iter()
                            .filter(|d| {
                                !d.name.starts_with("__super__")
                                    && (n == "*"
                                        || (d.name == n && a.map(|a| a == d.arity).unwrap_or(true)))
                            })
                            .map(|d| (d.name.clone(), d.arity))
                            .collect();
                        if keys.is_empty() && n != "*" {
                            return Err(format!("{}:{}: cannot make function {}/{} overridable because it was not defined", env.file, line, n, a.unwrap_or(0)));
                        }
                        m.overridable.extend(keys);
                    }
                }
                "@" => {}
                _ => {
                    if self.exunit_stmt(module, name, args, env, attrs, line)? {
                        return Ok(());
                    }
                    if let Some(stmts) = self.expand_module_macro(module, name, args, env, attrs) {
                        for st in stmts {
                            self.module_stmt_inner(module, st, env, attrs)?;
                        }
                        return Ok(());
                    }
                    // Other user macros run in the macro host.
                    let mut cands: Vec<String> = vec![module.to_string()];
                    cands.extend(env.imports.iter().map(|im| im.module.clone()));
                    if let Some(mm) = cands
                        .into_iter()
                        .find(|m| self.defines_macro(m, name, args.len()))
                    {
                        let e = crate::macrohost::expand(
                            &mm,
                            name,
                            args,
                            module,
                            None,
                            &env.file,
                            &env.aliases,
                            line,
                        )?;
                        return self.module_stmt_inner(module, e, env, attrs);
                    }
                    if crate::ctime::eval(&s, &vars, attrs).is_none() {
                        return Err(format!(
                            "{}:{}: unsupported module-level call {}/{}",
                            env.file,
                            line,
                            name,
                            args.len()
                        ));
                    }
                }
            },
            K::Un { op, e } if op == "@" => {
                if let K::Call { name, args, .. } = &e.k {
                    match name.as_str() {
                        "doc" | "moduledoc" | "spec" | "impl" | "type" | "typep" | "opaque"
                        | "typedoc" | "since" | "deprecated" | "behaviour" | "dialyzer"
                        | "optional_callbacks" | "external_resource" | "vsn" => {}
                        "compile" => {
                            fn hint(value: &E) -> bool {
                                match &value.k {
                                    K::Atom(name) => matches!(
                                        name.as_str(),
                                        "inline"
                                            | "no_inline"
                                            | "debug_info"
                                            | "no_debug_info"
                                            | "bin_opt_info"
                                    ),
                                    K::Tuple(items) if items.len() == 2 => {
                                        matches!(&items[0].k, K::Atom(name) if matches!(name.as_str(), "inline" | "no_inline" | "no_warn_undefined"))
                                    }
                                    K::List(items, None) => items.iter().all(hint),
                                    _ => false,
                                }
                            }
                            if args.len() != 1 || !hint(&args[0]) {
                                return Err(format!("{}:{}: unsupported @compile option: executable compiler transformations are not supported", env.file, line));
                            }
                        }
                        "on_load" => {
                            return Err(format!(
                                "{}:{}: @on_load callbacks are not supported by Tonic",
                                env.file, line
                            ))
                        }
                        "callback" | "macrocallback" => {
                            if let Some(spec) = args.first() {
                                fn signature(e: &E) -> Option<(String, usize)> {
                                    match &e.k {
                                        K::Bin { op, l, .. } if op == "when" || op == "::" => {
                                            signature(l)
                                        }
                                        K::Call { name, args, .. } => {
                                            Some((name.clone(), args.len()))
                                        }
                                        K::Var(name) => Some((name.clone(), 0)),
                                        _ => None,
                                    }
                                }
                                if let Some(key) = signature(spec) {
                                    self.mods.get_mut(module).unwrap().callbacks.insert(key);
                                }
                            }
                        }
                        "before_compile" | "after_compile" | "on_definition" => {
                            attrs.insert("\u{0}callbacks".into(), E::atom("true", line));
                            if name == "after_compile" && !self.prelude {
                                self.mods.get_mut(module).unwrap().after_compile = true;
                            }
                            if name == "on_definition" {
                                attrs.insert("\u{0}on_definition".into(), E::atom("true", line));
                            }
                        }
                        "tag" | "moduletag" | "describetag"
                            if args.len() == 1
                                && self
                                    .mods
                                    .get(module)
                                    .map(|m| m.exunit.is_some())
                                    .unwrap_or(false) =>
                        {
                            let vars: crate::ctime::Vars = attrs
                                .iter()
                                .filter_map(|(k, v)| {
                                    k.strip_prefix("\u{0}var:")
                                        .map(|nm| (nm.to_string(), v.clone()))
                                })
                                .collect();
                            let v = if vars.is_empty() {
                                args[0].clone()
                            } else {
                                map_expr(&args[0], &mut |x: &E| match &x.k {
                                    K::Var(n) => vars.get(n).cloned(),
                                    _ => None,
                                })
                            };
                            let _ = self.exunit_tag(module, name, &v);
                        }
                        _ => {
                            if let Some(v) = args.first() {
                                let mut w = std::mem::take(&mut self.warnings);
                                let v = subst_attrs(v, attrs, &mut w, &env.file);
                                self.warnings = w;
                                // Fold calls that can run at compile time; module-level
                                // variables (e.g. from a `for`) are substituted.
                                let vars: crate::ctime::Vars = attrs
                                    .iter()
                                    .filter_map(|(k, v)| {
                                        k.strip_prefix("\u{0}var:")
                                            .map(|nm| (nm.to_string(), v.clone()))
                                    })
                                    .collect();
                                let expression = map_expr(&v, &mut |node| match &node.k {
                                    K::Var(name) if name == "__MODULE__" => {
                                        Some(E::atom(module, node.line))
                                    }
                                    K::Var(name) => vars.get(name).cloned(),
                                    _ => None,
                                });
                                let v =
                                    match self.constant_attribute(&expression, env, &vars, attrs) {
                                        Some(value) => value,
                                        None if !self.prelude => crate::macrohost::attribute_value(
                                            module, name, &env.file, line,
                                        )?,
                                        None => {
                                            return Err(format!(
                                            "{}:{}: unsupported compile-time expression for @{}",
                                            env.file, line, name
                                        ))
                                        }
                                    };
                                attrs.insert(name.clone(), v);
                            }
                        }
                    }
                }
            }
            K::Block(es) => {
                for e in es.clone() {
                    self.module_stmt_inner(module, e, env, attrs)?;
                }
            }
            // e.g. a macro returning a list of quoted definitions
            K::List(es, None) => {
                for e in es.clone() {
                    self.module_stmt_inner(module, e, env, attrs)?;
                }
            }
            K::Remote {
                recv, name, args, ..
            } if matches!(recv.k, K::Alias(_)) => {
                let target = match &recv.k {
                    K::Alias(a) => resolve_alias(env, a),
                    _ => unreachable!(),
                };
                if target == "Elixir.Enum" && matches!(name.as_str(), "each" | "map") && args.len() == 2 {
                    let mut last = line;
                    map_expr(&s, &mut |node| { last = last.max(node.line); None });
                    let definitions = crate::macrohost::generated_definitions(module, &env.file, line, last)?;
                    return self.module_stmt_inner(module, definitions, env, attrs);
                }
                if self.defines_macro(&target, name, args.len()) {
                    let e = crate::macrohost::expand(
                        &target,
                        name,
                        args,
                        module,
                        None,
                        &env.file,
                        &env.aliases,
                        line,
                    )?;
                    return self.module_stmt_inner(module, e, env, attrs);
                }
                if crate::ctime::eval(&s, &vars, attrs).is_none() {
                    return Err(format!(
                        "{}:{}: unsupported module-level call {}.{}/{}",
                        env.file,
                        line,
                        target.trim_start_matches("Elixir."),
                        name,
                        args.len()
                    ));
                }
            }
            _ => {
                if crate::ctime::eval(&s, &vars, attrs).is_none() {
                    return Err(format!(
                        "{}:{}: unsupported executable module-level expression",
                        env.file, line
                    ));
                }
            }
        }
        Ok(())
    }

    fn defines_macro(&self, m: &str, name: &str, n: usize) -> bool {
        self.mods.get(m).map(|md| {
            md.defs.iter().any(|d| {
                let ndef = d
                    .args
                    .iter()
                    .filter(|a| matches!(&a.k, K::Bin { op, .. } if op == "\\\\"))
                    .count();
                d.is_macro && d.name == name && d.arity >= n && d.arity - ndef <= n
            })
        }) == Some(true)
    }

    /// Expand a module-level call to a `defmacro` whose body is a `quote`
    /// (optionally with `bind_quoted:`), evaluating its arguments at compile
    /// time. Returns the statements to process.
    fn expand_module_macro(
        &self,
        module: &str,
        name: &str,
        args: &[E],
        env: &ModEnv,
        attrs: &HashMap<String, E>,
    ) -> Option<Vec<E>> {
        let n = args.len();
        let mut candidates: Vec<&str> = vec![module];
        for im in &env.imports {
            candidates.push(im.module.as_str());
        }
        let def = candidates.iter().find_map(|m| {
            self.mods.get(*m)?.defs.iter().find(|d| {
                let ndef = d
                    .args
                    .iter()
                    .filter(|a| matches!(&a.k, K::Bin { op, .. } if op == "\\\\"))
                    .count();
                d.is_macro && d.name == name && d.arity >= n && d.arity - ndef <= n
            })
        })?;
        if !self
            .mods
            .get(&def.env.module)
            .map(|m| m.is_prelude)
            .unwrap_or(false)
        {
            return None;
        }
        let vars: crate::ctime::Vars = attrs
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix("\u{0}var:")
                    .map(|nm| (nm.to_string(), v.clone()))
            })
            .collect();
        let mut pvars = crate::ctime::Vars::new();
        for (i, p) in def.args.iter().enumerate() {
            let (pname, default) = match &p.k {
                K::Var(v) => (v.clone(), None),
                K::Bin { op, l, r } if op == "\\\\" => match &l.k {
                    K::Var(v) => (v.clone(), Some((**r).clone())),
                    _ => return None,
                },
                _ => return None,
            };
            let val = if i < n {
                crate::ctime::eval(&args[i], &vars, attrs)?
            } else {
                crate::ctime::eval(default.as_ref()?, &pvars, attrs)?
            };
            pvars.insert(pname, val);
        }
        let body = def.body.as_ref()?;
        let body = match &body.k {
            K::Block(es) if es.len() == 1 => es[0].clone(),
            _ => body.clone(),
        };
        let (qargs, inner) = match &body.k {
            K::Call { name, args, .. } if name == "quote" && !args.is_empty() => {
                let last = args.last().unwrap();
                (args[..args.len() - 1].to_vec(), last.kw_get("do")?.clone())
            }
            _ => return None,
        };
        let mut uvars = pvars.clone();
        let opts = qargs.first().cloned().or_else(|| {
            // `quote bind_quoted: [...] do` puts options and do in one list
            match &args_kw_rest(&body)? {
                x => Some(x.clone()),
            }
        });
        if let Some(o) = opts {
            if let Some(bq) = o.kw_get("bind_quoted") {
                if let K::List(items, _) = &bq.k {
                    uvars = crate::ctime::Vars::new();
                    for it in items {
                        if let K::Tuple(kv) = &it.k {
                            if let (K::Atom(k), v) = (&kv[0].k, &kv[1]) {
                                uvars.insert(k.clone(), crate::ctime::eval(v, &pvars, attrs)?);
                            }
                        }
                    }
                }
            }
        }
        let stmts = match &inner.k {
            K::Block(es) => es.clone(),
            _ => vec![inner.clone()],
        };
        Some(
            stmts
                .iter()
                .map(|st| crate::ctime::subst_unquote(st, &uvars, attrs))
                .collect(),
        )
    }

    /// `use X, opts`: built-in behaviours inject default callbacks; user
    /// modules with a `__using__` macro get its quoted body injected.
    fn do_use(
        &mut self,
        module: &str,
        target: &str,
        opts: Option<E>,
        env: &mut ModEnv,
        attrs: &mut HashMap<String, E>,
        line: u32,
    ) -> R<()> {
        self.mods
            .get_mut(module)
            .unwrap()
            .uses
            .push(target.to_string());
        if target == "Elixir.ExUnit.Case" {
            let st = crate::exunit::ExUnitState {
                opts: opts.clone(),
                ..Default::default()
            };
            self.mods.get_mut(module).unwrap().exunit = Some(st);
        }
        let template: Option<String> = match target {
            "Elixir.Application" => Some(String::new()),
            _ => None,
        };
        if let Some(t) = template {
            // Defaults are added at the end of collection if not defined.
            let stmts = parse_source(&t, &format!("<use {}>", target))?;
            let _ = opts;
            for st in stmts {
                if let K::Call { name, args, .. } = &st.k {
                    if name == "def" || name == "defp" {
                        let mut a2 = attrs.clone();
                        a2.insert("__use_default__".into(), E::atom("true", line));
                        self.add_def(module, name, args, env, &a2, line)?;
                        let m = self.mods.get_mut(module).unwrap();
                        let d = m.defs.last_mut().unwrap();
                        d.name = format!("__default__{}", d.name);
                    }
                }
            }
            return Ok(());
        }
        // User-defined __using__ macro: inject the quoted body.
        let using = self.mods.get(target).and_then(|m| {
            m.defs
                .iter()
                .find(|d| d.is_macro && d.name == "__using__")
                .cloned()
        });
        match using {
            Some(d) => {
                if !self.mods.get(target).map(|m| m.is_prelude).unwrap_or(false) {
                    let options = opts.unwrap_or_else(|| E::list(vec![], line));
                    let expanded = crate::macrohost::expand(
                        target,
                        "__using__",
                        &[options],
                        module,
                        None,
                        &env.file,
                        &env.aliases,
                        line,
                    )?;
                    for stmt in block_stmts(&expanded) {
                        self.module_stmt_inner(module, stmt, env, attrs)?;
                    }
                    return Ok(());
                }
                let body = d.body.clone().unwrap_or(E::nil(line));
                let quoted = find_quote(&body).ok_or_else(|| {
                    format!(
                        "{}:{}: tonic only supports __using__ macros whose body is a single quote block",
                        env.file, line
                    )
                })?;
                // bind the opts parameter for unquote()
                let mut bindings: HashMap<String, E> = HashMap::new();
                if let Some(p) = d.args.first() {
                    if let K::Var(v) = &p.k {
                        bindings.insert(v.clone(), opts.clone().unwrap_or(E::list(vec![], line)));
                    }
                }
                // `quote bind_quoted: [k: v] do`: unquote is off in the body and
                // the bindings become module-level variables (used by unquote
                // fragments in defs).
                let bq = quote_opts(&body).and_then(|o| o.kw_get("bind_quoted").cloned());
                let quoted = match bq {
                    Some(E {
                        k: K::List(items, _),
                        ..
                    }) => {
                        for it in &items {
                            if let K::Tuple(kv) = &it.k {
                                if let K::Atom(k) = &kv[0].k {
                                    let v = unquote_subst(&kv[1], &bindings);
                                    let v = crate::ctime::eval(&v, &bindings, attrs).unwrap_or(v);
                                    attrs.insert(format!("\u{0}var:{}", k), v);
                                }
                            }
                        }
                        quoted
                    }
                    _ => unquote_subst(&quoted, &bindings),
                };
                for st in block_stmts(&quoted) {
                    self.module_stmt_inner(module, st, env, attrs)?;
                }
                Ok(())
            }
            None => {
                if self.mods.contains_key(target) {
                    Ok(())
                } else {
                    Err(format!(
                        "{}:{}: module {} is not loaded and could not be found",
                        env.file,
                        line,
                        target.trim_start_matches("Elixir.")
                    ))
                }
            }
        }
    }

    /// Resolve default callback definitions injected by `use`.
    pub fn finalize_uses(&mut self) {
        for m in self.mods.values_mut() {
            let defined: HashSet<(String, usize)> = m
                .defs
                .iter()
                .filter(|d| !d.name.starts_with("__default__"))
                .map(|d| (d.name.clone(), d.arity))
                .collect();
            m.defs.retain(|d| {
                if let Some(n) = d.name.strip_prefix("__default__") {
                    !defined.contains(&(n.to_string(), d.arity))
                } else {
                    true
                }
            });
            for d in m.defs.iter_mut() {
                if let Some(n) = d.name.strip_prefix("__default__") {
                    d.name = n.to_string();
                }
            }
        }
    }
}

const GENSERVER_USING: &str = r#"
def child_spec(init_arg) do
  %{id: __MODULE__, start: {__MODULE__, :start_link, [init_arg]}}
end
def handle_call(msg, _from, _state) do
  raise "attempted to call GenServer #{inspect(self())} but no handle_call/3 clause was provided (got: #{inspect(msg)})"
end
def handle_cast(msg, _state) do
  raise "attempted to cast GenServer #{inspect(self())} but no handle_cast/2 clause was provided (got: #{inspect(msg)})"
end
def handle_info(_msg, state) do
  {:noreply, state}
end
def terminate(_reason, _state) do
  :ok
end
def code_change(_old, state, _extra) do
  {:ok, state}
end
"#;

const AGENT_USING: &str = r#"
def child_spec(arg) do
  %{id: __MODULE__, start: {__MODULE__, :start_link, [arg]}}
end
"#;

const TASK_USING: &str = r#"
def child_spec(arg) do
  %{id: __MODULE__, start: {__MODULE__, :start_link, [arg]}, restart: :temporary}
end
"#;

const SUPERVISOR_USING: &str = r#"
def child_spec(arg) do
  %{id: __MODULE__, start: {__MODULE__, :start_link, [arg]}, type: :supervisor}
end
"#;

pub fn block_stmts(e: &E) -> Vec<E> {
    match &e.k {
        K::Block(es) => es.clone(),
        _ => vec![e.clone()],
    }
}

fn find_quote(e: &E) -> Option<E> {
    match &e.k {
        K::Call { name, args, .. } if name == "quote" => {
            args.last().and_then(|kw| kw.kw_get("do").cloned())
        }
        K::Block(es) => es.iter().rev().find_map(find_quote),
        _ => None,
    }
}

/// Replace `unquote(var)` with bound expressions.
pub fn unquote_subst(e: &E, b: &HashMap<String, E>) -> E {
    map_expr(e, &mut |x: &E| {
        if let K::Call { name, args, .. } = &x.k {
            if name == "unquote" && args.len() == 1 {
                if let K::Var(v) = &args[0].k {
                    if let Some(r) = b.get(v) {
                        return Some(r.clone());
                    }
                }
            }
        }
        None
    })
}

fn struct_fields(e: &E, attrs: &HashMap<String, E>, file: &str) -> R<Vec<(String, E)>> {
    let mut w = Vec::new();
    let e = subst_attrs(e, attrs, &mut w, file);
    let mut out = Vec::new();
    if let K::List(items, _) = &e.k {
        for it in items {
            match &it.k {
                K::Atom(a) => out.push((a.clone(), E::nil(it.line))),
                K::Tuple(t) if t.len() == 2 => {
                    if let K::Atom(a) = &t[0].k {
                        out.push((a.clone(), t[1].clone()));
                    }
                }
                _ => return Err(format!("{}:{}: invalid struct field", file, it.line)),
            }
        }
        return Ok(out);
    }
    Err(format!("{}:{}: defstruct expects a list", file, e.line))
}

/// Generic expression rewriter: `f` returns Some(replacement) to replace a node.
pub fn map_expr(e: &E, f: &mut dyn FnMut(&E) -> Option<E>) -> E {
    if let Some(r) = f(e) {
        return r;
    }
    let line = e.line;
    let m = |x: &E, f: &mut dyn FnMut(&E) -> Option<E>| map_expr(x, f);
    let k = match &e.k {
        K::Interp(ps) => K::Interp(map_parts(ps, f)),
        K::Charlist(ps) => K::Charlist(map_parts(ps, f)),
        K::AtomInterp(ps) => K::AtomInterp(map_parts(ps, f)),
        K::Sigil { ch, parts, mods } => K::Sigil {
            ch: *ch,
            parts: map_parts(parts, f),
            mods: mods.clone(),
        },
        K::Call { name, args, parens } => K::Call {
            name: name.clone(),
            args: args.iter().map(|a| m(a, f)).collect(),
            parens: *parens,
        },
        K::Remote {
            recv,
            name,
            args,
            parens,
        } => K::Remote {
            recv: Box::new(m(recv, f)),
            name: name.clone(),
            args: args.iter().map(|a| m(a, f)).collect(),
            parens: *parens,
        },
        K::AnonCall { f: fe, args } => K::AnonCall {
            f: Box::new(m(fe, f)),
            args: args.iter().map(|a| m(a, f)).collect(),
        },
        K::Access { e: x, key } => K::Access {
            e: Box::new(m(x, f)),
            key: Box::new(m(key, f)),
        },
        K::Bin { op, l, r } => K::Bin {
            op: op.clone(),
            l: Box::new(m(l, f)),
            r: Box::new(m(r, f)),
        },
        K::Un { op, e: x } => K::Un {
            op: op.clone(),
            e: Box::new(m(x, f)),
        },
        K::Tuple(items) => K::Tuple(items.iter().map(|a| m(a, f)).collect()),
        K::List(items, t) => K::List(
            items.iter().map(|a| m(a, f)).collect(),
            t.as_ref().map(|t| Box::new(m(t, f))),
        ),
        K::Map(ps) => K::Map(ps.iter().map(|(a, b)| (m(a, f), m(b, f))).collect()),
        K::MapUpd(b, ps) => K::MapUpd(
            Box::new(m(b, f)),
            ps.iter().map(|(a, b)| (m(a, f), m(b, f))).collect(),
        ),
        K::Struct { name, base, pairs } => K::Struct {
            name: Box::new(m(name, f)),
            base: base.as_ref().map(|b| Box::new(m(b, f))),
            pairs: pairs.iter().map(|(a, b)| (m(a, f), m(b, f))).collect(),
        },
        K::Bits(items) => K::Bits(items.iter().map(|a| m(a, f)).collect()),
        K::Fn(cs) => K::Fn(cs.iter().map(|c| map_clause(c, f)).collect()),
        K::Block(items) => K::Block(items.iter().map(|a| m(a, f)).collect()),
        K::Clauses(cs) => K::Clauses(cs.iter().map(|c| map_clause(c, f)).collect()),
        K::Capture(x) => K::Capture(Box::new(m(x, f))),
        K::ParenArgs(items) => K::ParenArgs(items.iter().map(|a| m(a, f)).collect()),
        K::MultiAlias(b, items) => {
            K::MultiAlias(Box::new(m(b, f)), items.iter().map(|a| m(a, f)).collect())
        }
        other => other.clone(),
    };
    E { k, line }
}

fn map_parts(ps: &[IPart], f: &mut dyn FnMut(&E) -> Option<E>) -> Vec<IPart> {
    ps.iter()
        .map(|p| match p {
            IPart::Lit(b) => IPart::Lit(b.clone()),
            IPart::Expr(e) => IPart::Expr(map_expr(e, f)),
        })
        .collect()
}

fn map_clause(c: &Clause, f: &mut dyn FnMut(&E) -> Option<E>) -> Clause {
    Clause {
        args: c.args.iter().map(|a| map_expr(a, f)).collect(),
        guard: c.guard.as_ref().map(|g| map_expr(g, f)),
        body: map_expr(&c.body, f),
        line: c.line,
    }
}

/// Replace `@attr` reads with their current values.
pub fn subst_attrs(e: &E, attrs: &HashMap<String, E>, warnings: &mut Vec<String>, file: &str) -> E {
    map_expr(e, &mut |x: &E| {
        if let K::Call { name, args, parens } = &x.k {
            if name == "quote" {
                let unquote = !args.iter().any(|a| a.kw_get("bind_quoted").is_some() || matches!(a.kw_get("unquote"), Some(E { k: K::Atom(v), .. }) if v == "false"));
                let args = args
                    .iter()
                    .map(|a| {
                        if let K::List(items, None) = &a.k {
                            let items = items
                                .iter()
                                .map(|item| {
                                    if let K::Tuple(pair) = &item.k {
                                        if pair.len() == 2
                                            && matches!(&pair[0].k, K::Atom(n) if n == "do")
                                        {
                                            let body =
                                                map_expr(&pair[1], &mut |part: &E| match &part.k {
                                                    K::Call { name, args, parens }
                                                        if unquote
                                                            && (name == "unquote"
                                                                || name == "unquote_splicing") =>
                                                    {
                                                        Some(E::new(
                                                            K::Call {
                                                                name: name.clone(),
                                                                args: args
                                                                    .iter()
                                                                    .map(|a| {
                                                                        subst_attrs(
                                                                            a, attrs, warnings,
                                                                            file,
                                                                        )
                                                                    })
                                                                    .collect(),
                                                                parens: *parens,
                                                            },
                                                            part.line,
                                                        ))
                                                    }
                                                    K::Call { name, .. } if name == "quote" => {
                                                        Some(part.clone())
                                                    }
                                                    _ => None,
                                                });
                                            return E::tuple(
                                                vec![pair[0].clone(), body],
                                                item.line,
                                            );
                                        }
                                    }
                                    subst_attrs(item, attrs, warnings, file)
                                })
                                .collect();
                            E::list(items, a.line)
                        } else {
                            subst_attrs(a, attrs, warnings, file)
                        }
                    })
                    .collect();
                return Some(E::new(
                    K::Call {
                        name: name.clone(),
                        args,
                        parens: *parens,
                    },
                    x.line,
                ));
            }
        }
        if let K::Attr(name) = &x.k {
            return Some(match attrs.get(name) {
                Some(v) => v.clone(),
                None => {
                    warnings.push(format!(
                        "{}:{}: warning: undefined module attribute @{}, please remove access to @{} or explicitly set it before access",
                        file, x.line, name, name
                    ));
                    E::nil(x.line)
                }
            });
        }
        None
    })
}

/// `a when b` inside a guard position means `a or b`.
pub fn flatten_when_guard(e: E) -> E {
    if let K::Bin { op, l, r } = &e.k {
        if op == "when" {
            let line = e.line;
            return E::bin("or", (**l).clone(), flatten_when_guard((**r).clone()), line);
        }
    }
    e
}

/// For `quote bind_quoted: [...] do ... end`, the options live in the same
/// keyword list as `do:`; return that list.
fn args_kw_rest(body: &E) -> Option<E> {
    if let K::Call { args, .. } = &body.k {
        if let Some(last) = args.last() {
            if last.kw_get("bind_quoted").is_some() {
                return Some(last.clone());
            }
        }
    }
    None
}

fn has_defstruct(e: &E) -> bool {
    let mut found = false;
    map_expr(e, &mut |x: &E| {
        if matches!(&x.k, K::Call { name, .. } if name == "defstruct") {
            found = true;
        }
        None
    });
    found
}

/// Options of a `quote` call (`quote opts do ... end` or `quote opts ++ [do: ...]`).
fn quote_opts(body: &E) -> Option<E> {
    let q = find_quote_call(body)?;
    if let K::Call { args, .. } = &q.k {
        if args.len() == 2 {
            return Some(args[0].clone());
        }
        if let Some(last) = args.last() {
            if last.kw_get("do").is_some()
                && last.as_keyword().map(|k| k.len() > 1).unwrap_or(false)
            {
                return Some(last.clone());
            }
        }
    }
    None
}

fn find_quote_call(e: &E) -> Option<E> {
    match &e.k {
        K::Call { name, .. } if name == "quote" => Some(e.clone()),
        K::Block(es) => es.iter().rev().find_map(find_quote_call),
        _ => None,
    }
}

/// Whether `e` contains an `unquote(...)` outside of `quote` blocks.
fn has_unquote(e: &E) -> bool {
    let mut found = false;
    let _ = map_expr(e, &mut |x: &E| match &x.k {
        K::Call { name, .. } if name == "quote" => Some(x.clone()),
        K::Call { name, args, .. } if name == "unquote" && args.len() == 1 => {
            found = true;
            Some(x.clone())
        }
        _ => None,
    });
    found
}
