//! Phase 2: expand collected definitions into Core IR functions.

use crate::ast::*;
use crate::collect::*;
use crate::core::*;
use crate::core::Clause;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

pub struct Expander {
    /// Protocols are consolidated (like a Mix build) unless the program is a
    /// script: `elixir foo.exs` never consolidates.
    pub consolidated: bool,
    pub mods: BTreeMap<String, Module>,
    pub protocols: BTreeMap<String, Protocol>,
    /// (module, name, arity) -> public?
    pub funs: HashMap<(String, String, usize), bool>,
    pub macros: HashMap<(String, String, usize), DefC>,
    pub next_var: u32,
    pub hyg_counter: usize,
    pub next_lambda: usize,
    pub out: Vec<FunDef>,
    pub warnings: Vec<String>,
    /// variables the user's own code binds: (id, name, file, line, shadows
    /// a variable already in scope)
    pub var_sites: Vec<(VarId, String, String, u32, bool)>,
    pub used_vars: HashSet<VarId>,
}

pub struct Cx {
    pub scopes: Vec<HashMap<String, VarId>>,
    pub env: ModEnv,
    pub fun: String,
    pub guard: bool,
    pub cap: Option<Vec<VarId>>,
    pub stacktrace: Option<VarId>,
    pub lambda_index: usize,
    pub macro_depth: usize,
    pub macro_context: Option<&'static str>,
    pub super_call: Option<(String, Vec<VarId>)>,
}

impl Cx {
    pub fn new(env: ModEnv, fun: &str) -> Cx {
        Cx {
            scopes: vec![HashMap::new()],
            env,
            fun: fun.to_string(),
            guard: false,
            cap: None,
            stacktrace: None,
            lambda_index: 0,
            macro_depth: 0,
            macro_context: None,
            super_call: None,
        }
    }
    pub fn module(&self) -> &str {
        &self.env.module
    }
    pub fn file(&self) -> &str {
        &self.env.file
    }
}

pub fn err<T>(cx: &Cx, line: u32, msg: &str) -> R<T> {
    Err(format!("{}:{}: {}", cx.file(), line, msg))
}

impl Expander {
    pub fn new(c: Collector) -> Expander {
        let mut e = Expander {
            consolidated: true,
            mods: c.mods,
            protocols: c.protocols,
            funs: HashMap::new(),
            macros: HashMap::new(),
            next_var: 1,
            hyg_counter: 0,
            next_lambda: 0,
            out: Vec::new(),
            warnings: c.warnings,
            var_sites: Vec::new(),
            used_vars: HashSet::new(),
        };
        e.index_functions();
        e
    }

    /// Elixir's unused variable warnings, in source order. A clause compiled
    /// more than once warns only if no copy used the variable.
    pub fn unused_var_warnings(&mut self) {
        let mut sites: BTreeMap<(String, u32, String), (bool, bool)> = BTreeMap::new();
        for (id, name, file, line, shadows) in &self.var_sites {
            let site = sites.entry((file.clone(), *line, name.clone())).or_insert((false, *shadows));
            site.0 |= self.used_vars.contains(id);
        }
        for ((file, line, name), (used, shadows)) in sites {
            if used {
                continue;
            }
            let hint = if shadows {
                "there is a variable with the same name in the context, use the pin operator (^) to match on it or prefix this variable with underscore if it is not meant to be used"
            } else {
                "if the variable is not meant to be used, prefix it with an underscore"
            };
            self.warnings.push(format!("{}:{}: warning: variable \"{}\" is unused ({})", file, line, name, hint));
        }
    }

    pub fn new_var(&mut self) -> VarId {
        self.next_var += 1;
        self.next_var
    }

    /// Record every function (including generated ones) so calls resolve.
    fn index_functions(&mut self) {
        let mut funs = HashMap::new();
        let mut macros = HashMap::new();
        for m in self.mods.values() {
            for d in &m.defs {
                if d.is_macro {
                    let ndef = d
                        .args
                        .iter()
                        .filter(|a| matches!(&a.k, K::Bin { op, .. } if op == "\\\\"))
                        .count();
                    for ar in (d.arity - ndef)..=d.arity {
                        macros.insert((m.name.clone(), d.name.clone(), ar), d.clone());
                    }
                    continue;
                }
                let ndef = d
                    .args
                    .iter()
                    .filter(|a| matches!(&a.k, K::Bin { op, .. } if op == "\\\\"))
                    .count();
                for ar in (d.arity - ndef)..=d.arity {
                    let e = funs.entry((m.name.clone(), d.name.clone(), ar)).or_insert(false);
                    *e = *e || d.public;
                }
            }
            if !m.defs.iter().any(|d| d.name == "__info__" && d.arity == 1) {
                funs.insert((m.name.clone(), "__info__".into(), 1), true);
                funs.insert((m.name.clone(), "module_info".into(), 0), true);
                funs.insert((m.name.clone(), "module_info".into(), 1), true);
                if !m.callbacks.is_empty() {
                    funs.insert((m.name.clone(), "behaviour_info".into(), 1), true);
                }
            }
            if m.struct_fields.is_some() {
                funs.insert((m.name.clone(), "__struct__".into(), 0), true);
                funs.insert((m.name.clone(), "__struct__".into(), 1), true);
                funs.insert((m.name.clone(), "__struct_fields__".into(), 0), true);
                if m.exception {
                    funs.insert((m.name.clone(), "exception".into(), 1), true);
                    funs.insert((m.name.clone(), "message".into(), 1), true);
                }
            }
        }
        for p in self.protocols.values() {
            for (f, a) in &p.funs {
                funs.insert((p.name.clone(), f.clone(), *a), true);
            }
            funs.insert((p.name.clone(), "impl_for".into(), 1), true);
            funs.insert((p.name.clone(), "impl_for!".into(), 1), true);
            funs.insert((p.name.clone(), "__protocol__".into(), 1), true);
        }
        self.funs = funs;
        self.macros = macros;
    }

    /// Real functions for BIFs of stdlib modules, so that dynamic calls
    /// (`apply(:erlang, :apply, [f, args])`) and the function table find
    /// them. Static calls keep using the BIF directly.
    fn gen_bif_wrappers(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for (m, n, a, sym) in crate::bifs::entries() {
            if m == "tonic" || !seen.insert((m, n, a)) {
                continue;
            }
            if !self.mods.get(m).map(|md| md.is_prelude).unwrap_or(false) {
                continue;
            }
            if self.funs.contains_key(&(m.to_string(), n.to_string(), a)) {
                continue;
            }
            let params: Vec<VarId> = (0..a).map(|_| self.new_var()).collect();
            self.out.push(FunDef {
                key: FunKey::new(m, n, a),
                params: params.clone(),
                body: CE::Bif(sym.to_string(), params.iter().map(|v| CE::Var(*v)).collect()),
                public: true,
            });
        }
    }

    pub fn fun_exists(&self, m: &str, n: &str, a: usize) -> Option<bool> {
        self.funs.get(&(m.to_string(), n.to_string(), a)).copied()
    }

    pub fn run(&mut self, script: Vec<(E, Rc<ModEnv>)>) -> R<()> {
        let names: Vec<String> = self.mods.keys().cloned().collect();
        let mut errors = Vec::new();
        for name in names {
            if let Err(e) = self.expand_module(&name) {
                errors.push(e);
            }
        }
        let pnames: Vec<String> = self.protocols.keys().cloned().collect();
        for p in pnames {
            self.gen_protocol(&p);
        }
        self.gen_bif_wrappers();
        if let Err(e) = self.expand_script(script) {
            errors.push(e);
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        Ok(())
    }

    fn expand_module(&mut self, name: &str) -> R<()> {
        let m = self.mods.get(name).unwrap().clone();
        let env = m.env.clone().map(|e| (*e).clone()).unwrap_or(ModEnv {
            module: name.to_string(),
            aliases: HashMap::new(),
            imports: vec![],
            file: m.file.clone(),
            requires: HashSet::new(),
        });
        self.validate_module_guards(&m, &env)?;
        // Group clauses by name/arity preserving order.
        let mut order: Vec<(String, usize)> = Vec::new();
        let mut groups: HashMap<(String, usize), Vec<DefC>> = HashMap::new();
        for d in &m.defs {
            if d.is_macro {
                continue;
            }
            let k = (d.name.clone(), d.arity);
            if !groups.contains_key(&k) {
                order.push(k.clone());
            }
            groups.entry(k).or_default().push(d.clone());
        }
        let mut errors = Vec::new();
        for k in &order {
            let clauses = groups.remove(k).unwrap();
            let first = clauses[0].clone();
            let sites = self.var_sites.len();
            if let Err(e) = self.expand_function(name, &k.0, k.1, clauses) {
                self.var_sites.truncate(sites);
                if m.hoisted {
                    // Elixir compiles such modules when the enclosing code
                    // runs; surface the error then instead of failing the build.
                    self.warnings.push(format!("warning: {} (deferred to runtime)", e));
                    let l = first.line;
                    let msg = e.splitn(2, ": ").nth(1).unwrap_or(&e).to_string();
                    let mut stub = first;
                    stub.args = (0..k.1).map(|_| E::var("_", l)).collect();
                    stub.guard = None;
                    stub.body = Some(E::call(
                        "raise",
                        vec![
                            E::new(K::Alias("CompileError".into()), l),
                            E::list(vec![E::tuple(vec![E::atom("description", l), E::new(K::Str(msg.into_bytes()), l)], l)], l),
                        ],
                        l,
                    ));
                    if let Err(e2) = self.expand_function(name, &k.0, k.1, vec![stub]) {
                        errors.push(e2);
                    }
                } else {
                    errors.push(e);
                }
            }
        }
        if !m.is_prelude || m.name == "Elixir.Record" || m.name == "Elixir.EEx" || !m.guards.is_empty() {
            self.gen_runtime_macros(&m);
        }
        if let Some(fields) = &m.struct_fields {
            self.gen_struct_funs(&m, fields, &env)?;
        }
        if !m.defs.iter().any(|d| d.name == "__info__" && d.arity == 1) {
            self.gen_info(&m, &env)?;
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        Ok(())
    }

    /// Public macros of user modules are also compiled as functions
    /// `:"MACRO-name"/(arity + 1)` (the caller's env first, like the BEAM),
    /// so code evaluated at runtime (Code.eval_string, ...) can expand them.
    /// Macros whose bodies only make sense at compile time are skipped.
    fn gen_runtime_macros(&mut self, m: &crate::collect::Module) {
        let mut order: Vec<(String, usize)> = Vec::new();
        let mut groups: HashMap<(String, usize), Vec<(DefC, Option<usize>)>> = HashMap::new();
        let mut definitions: Vec<(usize, bool, DefC, Option<usize>)> = m.defs.iter().cloned().enumerate().map(|(i, d)| (i, false, d, None)).collect();
        for ((name, arity), (params, body)) in &m.guards {
            let key = (name.clone(), *arity);
            if m.private_guards.contains(&key) { continue; }
            let line = body.line;
            let template = E::call("quote", vec![E::list(vec![E::tuple(vec![E::atom("do", line), body.clone()], line)], line)], line);
            let expansion = if m.guard_generated.contains(&key) {
                let args = match &body.k { K::Call { args, .. } => args.clone(), _ => unreachable!() };
                E::new(K::Remote {
                    recv: Box::new(E::atom(&m.name, line)), name: format!("MACRO-{}", name),
                    args: std::iter::once(E::var("__CALLER__", line)).chain(args).collect(), parens: true,
                }, line)
            } else {
                E::remote("Tonic.Eval", "guard_expansion", vec![
                E::list(params.iter().map(|name| E::atom(name, line)).collect(), line),
                template,
                E::list(params.iter().map(|name| E::var(name, line)).collect(), line),
                E::var("__CALLER__", line),
                E::atom(&m.name, line),
            ], line)
            };
            let definition = DefC {
                name: name.clone(), arity: *arity, public: true,
                args: params.iter().map(|name| E::var(name, line)).collect(),
                guard: None, body: Some(expansion),
                env: m.guard_envs.get(&key).cloned().or_else(|| m.env.clone()).unwrap_or_else(|| Rc::new(ModEnv {
                    module: m.name.clone(), aliases: HashMap::new(), imports: vec![],
                    file: m.file.clone(), requires: HashSet::new(),
                })),
                line, is_macro: true, super_target: None,
            };
            let target = if m.guard_generated.contains(&key) {
                match &body.k { K::Call { args, .. } => Some(args.len()), _ => unreachable!() }
            } else { None };
            definitions.push((m.guard_order.get(&key).copied().unwrap_or(m.defs.len()), true, definition, target));
        }
        definitions.sort_by_key(|(order, guard, _, _)| (*order, !*guard));
        let mut ordered = Vec::new();
        for (_, _, mut definition, target) in definitions {
            let original = definition.args.clone();
            let count = original.iter().filter(|arg| matches!(&arg.k, K::Bin { op, .. } if op == "\\\\")).count();
            definition.args = original.iter().map(|arg| match &arg.k {
                K::Bin { op, l, .. } if op == "\\\\" => (**l).clone(),
                _ => arg.clone(),
            }).collect();
            ordered.push((definition.clone(), target));
            if !definition.is_macro { continue; }
            for missing in 1..=count {
                let mut keep = count - missing;
                let mut params = Vec::new();
                let mut arguments = vec![E::var("__CALLER__", definition.line)];
                for arg in &original {
                    if let K::Bin { op, r, .. } = &arg.k {
                        if op == "\\\\" {
                            if keep == 0 { arguments.push((**r).clone()); continue; }
                            keep -= 1;
                        }
                    }
                    let param = E::var(&format!("tonic_default_{}", params.len()), definition.line);
                    arguments.push(param.clone());
                    params.push(param);
                }
                let mut wrapper = definition.clone();
                wrapper.arity = params.len();
                wrapper.args = params;
                wrapper.guard = None;
                wrapper.body = Some(E::new(K::Remote {
                    recv: Box::new(E::atom(&m.name, definition.line)),
                    name: format!("MACRO-{}", definition.name), args: arguments, parens: true,
                }, definition.line));
                ordered.push((wrapper, Some(definition.arity)));
            }
        }
        for (d, target) in &ordered {
            if !d.is_macro || !d.public {
                continue;
            }
            let mut d = d.clone();
            let l = d.line;
            let caller = E::var("tonic_caller", l);
            let fix = |e: &E| {
                crate::collect::map_expr(e, &mut |x: &E| match &x.k {
                    K::Var(v) if v == "__CALLER__" => Some(E::var("tonic_caller", x.line)),
                    _ => None,
                })
            };
            d.args = std::iter::once(caller).chain(d.args.iter().cloned()).collect();
            d.body = d.body.as_ref().map(fix);
            d.guard = d.guard.as_ref().map(fix);
            d.name = format!("MACRO-{}", d.name);
            d.arity += 1;
            d.is_macro = false;
            let k = (d.name.clone(), d.arity);
            if !groups.contains_key(&k) {
                order.push(k.clone());
            }
            groups.entry(k).or_default().push((d, target.map(|arity| arity + 1)));
        }
        // A default wrapper is usable only if its full-arity target compiled.
        // Keep explicit shorter clauses even when a compile-time-only body failed.
        order.sort_by_key(|(_, arity)| std::cmp::Reverse(*arity));
        let mut compiled = HashSet::new();
        for k in order {
            let clauses = groups.remove(&k).unwrap().into_iter()
                .filter(|(_, target)| target.map_or(true, |arity| compiled.contains(&(k.0.clone(), arity))))
                .map(|(definition, _)| definition).collect::<Vec<_>>();
            if clauses.is_empty() { continue; }
            let n_out = self.out.len();
            let saved_warnings = self.warnings.len();
            let saved_sites = self.var_sites.len();
            if self.expand_function(&m.name, &k.0, k.1, clauses).is_err() {
                self.out.truncate(n_out);
                self.warnings.truncate(saved_warnings);
                self.var_sites.truncate(saved_sites);
            } else {
                compiled.insert(k);
            }
        }
    }

    fn expand_function(&mut self, module: &str, name: &str, arity: usize, clauses: Vec<DefC>) -> R<()> {
        let public = clauses.iter().any(|c| c.public);
        // Defaults: taken from the first clause that has them.
        let mut defaults: Vec<Option<E>> = vec![None; arity];
        for c in &clauses {
            for (i, a) in c.args.iter().enumerate() {
                if let K::Bin { op, r, .. } = &a.k {
                    if op == "\\\\" {
                        defaults[i] = Some((**r).clone());
                    }
                }
            }
        }
        let env0 = (*clauses[0].env).clone();
        let body_clauses: Vec<&DefC> = clauses.iter().filter(|c| c.body.is_some()).collect();
        if body_clauses.is_empty() {
            return Err(format!(
                "{}:{}: function {}/{} has no body",
                env0.file, clauses[0].line, name, arity
            ));
        }
        let key = FunKey::new(module, name, arity);
        let mut cx = Cx::new(env0.clone(), &format!("{}/{}", name, arity));
        let params: Vec<VarId> = (0..arity).map(|_| self.new_var()).collect();
        let mut cclauses = Vec::new();
        for c in body_clauses {
            cx.env = (*c.env).clone();
            cx.super_call = c.super_target.as_ref().map(|name| (name.clone(), params.clone()));
            cx.scopes = vec![HashMap::new()];
            let mut bound = HashMap::new();
            let mut pats = Vec::new();
            for a in &c.args {
                let a = match &a.k {
                    K::Bin { op, l, .. } if op == "\\\\" => (**l).clone(),
                    _ => a.clone(),
                };
                pats.push(self.pat(&a, &mut cx, &mut bound)?);
            }
            for (k, v) in bound {
                cx.scopes.last_mut().unwrap().insert(k, v);
            }
            let guard = match &c.guard {
                Some(g) => Some(self.guard(g, &mut cx)?),
                None => None,
            };
            let body = self.expr(c.body.as_ref().unwrap(), &mut cx)?;
            cclauses.push(Clause { pats, guard, body });
        }
        let body = CE::Case(
            params.iter().map(|p| CE::Var(*p)).collect(),
            cclauses,
            Fail::FunctionClause {
                module: module.to_string(),
                name: name.to_string(),
                line: clauses[0].line,
            },
        );
        self.out.push(FunDef {
            key: key.clone(),
            params,
            body,
            public,
        });
        // Wrappers for default arguments.
        let ndef = defaults.iter().filter(|d| d.is_some()).count();
        let required = arity - ndef;
        let user = self.mods.get(module).map_or(false, |m| !m.is_prelude);
        // only a definition after the one declaring the defaults conflicts;
        // an earlier one gains the default clause (Decimal.new/1 and new/2)
        let declared = self.mods.get(module).and_then(|m| {
            m.defs.iter().position(|d| {
                d.name == name && d.arity == arity && d.args.iter().any(|a| matches!(&a.k, K::Bin { op, .. } if op == "\\\\"))
            })
        });
        for k in required..arity {
            if let (true, Some(at)) = (user, declared) {
                if let Some(other) = self.mods[module].defs.iter().skip(at + 1).find(|d| d.name == name && d.arity == k && !d.is_macro) {
                    let kind = if other.public { "def" } else { "defp" };
                    return Err(format!(
                        "{}:{}: {} {}/{} conflicts with defaults from {}/{}",
                        env0.file, other.line, kind, name, k, name, arity
                    ));
                }
            }
            let provided = k - required;
            let mut cx = Cx::new(env0.clone(), &format!("{}/{}", name, k));
            let wkey = FunKey::new(module, name, k);
            let existing = self.out.iter().position(|f| f.key == wkey);
            let ps: Vec<VarId> = match existing {
                Some(i) => self.out[i].params.clone(),
                None => (0..k).map(|_| self.new_var()).collect(),
            };
            let mut args = Vec::new();
            let mut pi = 0;
            let mut di = 0;
            for d in defaults.iter() {
                match d {
                    None => {
                        args.push(CE::Var(ps[pi]));
                        pi += 1;
                    }
                    Some(dexpr) => {
                        if di < provided {
                            args.push(CE::Var(ps[pi]));
                            pi += 1;
                        } else {
                            args.push(self.expr(dexpr, &mut cx)?);
                        }
                        di += 1;
                    }
                }
            }
            // An explicitly defined function of that arity gets the
            // default-argument wrapper as its last clause.
            if let Some(i) = existing {
                if let CE::Case(_, cls, _) = &mut self.out[i].body {
                    cls.push(Clause {
                        pats: (0..k).map(|_| Pat::Wild).collect(),
                        guard: None,
                        body: CE::Call(key.clone(), args),
                    });
                }
                continue;
            }
            self.out.push(FunDef {
                key: wkey,
                params: ps,
                body: CE::Call(key.clone(), args),
                public,
            });
        }
        Ok(())
    }

    /// `Mod.__info__/1`: module, functions, macros, struct, compile, attributes, md5.
    fn gen_info(&mut self, m: &Module, env: &ModEnv) -> R<()> {
        let hidden = ["__info__", "__struct_fields__", "__inspect_derive__", "__derive_opts__", "module_info", "behaviour_info"];
        let mut fs: Vec<(String, usize)> = self
            .funs
            .iter()
            .filter(|((mm, n, _), public)| **public && mm == &m.name && !hidden.contains(&n.as_str()))
            .map(|((_, n, a), _)| (n.clone(), *a))
            .collect();
        let mut ms: Vec<(String, usize)> = self
            .macros
            .iter()
            .filter(|((mm, _, _), d)| mm == &m.name && d.public)
            .map(|((_, n, a), _)| (n.clone(), *a))
            .collect();
        ms.extend(m.guards.keys().filter(|key| !m.private_guards.contains(*key)).cloned());
        if m.name == "Elixir.Kernel" {
            for (n, arities) in crate::quote::kernel_exports() {
                for a in arities {
                    let k = (n.clone(), *a);
                    if !fs.contains(&k) && !ms.contains(&k) {
                        ms.push(k);
                    }
                }
            }
        }
        let key = |x: &(String, usize)| (x.0.as_bytes().to_vec(), x.1);
        fs.sort_by_key(key);
        fs.dedup();
        ms.sort_by_key(key);
        ms.dedup();
        let atom = |n: &str| {
            let mut out = String::from(":\"");
            for ch in n.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '#' => out.push_str("\\#"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        };
        let kw = |xs: &[(String, usize)]| {
            let items: Vec<String> = xs.iter().map(|(n, a)| format!("{{{}, {}}}", atom(n), a)).collect();
            format!("[{}]", items.join(", "))
        };
        let st = if m.struct_fields.is_some() {
            "for f <- __struct_fields__(), f != :__exception__, do: %{field: f, default: Map.get(__struct__(), f)}".to_string()
        } else {
            "nil".to_string()
        };
        let mut src = format!(
            "def __info__(:module), do: __MODULE__\ndef __info__(:functions), do: {}\ndef __info__(:macros), do: {}\ndef __info__(:struct), do: {}\ndef __info__(:compile), do: [version: ~c\"8.6\", source: ~c\"\", options: []]\ndef __info__(:attributes), do: [vsn: [0]]\ndef __info__(:md5), do: <<0::128>>\ndef __info__(:exports_md5), do: <<0::128>>\ndef __info__(:deprecated), do: []\n",
            kw(&fs),
            kw(&ms),
            st
        );
        // the Erlang view: exports include macros as MACRO-name/arity+1
        let mut exports: Vec<(String, usize)> = vec![("__info__".into(), 1)];
        exports.extend(fs.iter().cloned());
        exports.extend(ms.iter().map(|(n, a)| (format!("MACRO-{}", n), a + 1)));
        exports.push(("module_info".into(), 0));
        exports.push(("module_info".into(), 1));
        src.push_str(&format!(
            "def module_info(), do: [module: __MODULE__, exports: module_info(:exports), attributes: [vsn: [0]], compile: [version: ~c\"8.6\", options: [], source: ~c\"\"], md5: <<0::128>>]\ndef module_info(:module), do: __MODULE__\ndef module_info(:exports), do: {ex}\ndef module_info(:functions), do: {ex}\ndef module_info(:attributes), do: [vsn: [0]]\ndef module_info(:compile), do: [version: ~c\"8.6\", options: [], source: ~c\"\"]\ndef module_info(:md5), do: <<0::128>>\ndef module_info(:nifs), do: []\n",
            ex = kw(&exports)
        ));
        if !m.callbacks.is_empty() {
            let mut cbs: Vec<(String, usize)> = m.callbacks.iter().cloned().collect();
            cbs.sort_by_key(key);
            let mut optional = m.optional_callbacks.clone();
            optional.sort_by_key(key);
            src.push_str(&format!(
                "def behaviour_info(:callbacks), do: {}\ndef behaviour_info(:optional_callbacks), do: {}\n",
                kw(&cbs),
                kw(&optional)
            ));
        }
        self.gen_src_defs(m, env, &src, "__info__")
    }

    fn gen_src_defs(&mut self, m: &Module, env: &ModEnv, src: &str, what: &str) -> R<()> {
        let stmts = crate::parser::parse_source(src, &format!("<{} {}>", what, m.name))?;
        let mut groups: BTreeMap<(String, usize), Vec<DefC>> = BTreeMap::new();
        for s in stmts {
            if let K::Call { args, .. } = &s.k {
                let head = &args[0];
                let (head, guard) = match &head.k {
                    K::Bin { op, l, r } if op == "when" => ((**l).clone(), Some((**r).clone())),
                    _ => (head.clone(), None),
                };
                if let K::Call { name, args: fargs, .. } = &head.k {
                    groups.entry((name.clone(), fargs.len())).or_default().push(DefC {
                        name: name.clone(),
                        arity: fargs.len(),
                        public: true,
                        args: fargs.clone(),
                        guard,
                        body: args[1].kw_get("do").cloned(),
                        env: Rc::new(env.clone()),
                        line: s.line,
                        is_macro: false,
                        super_target: None,
                    });
                }
            }
        }
        for ((n, a), cs) in groups {
            self.expand_function(&m.name, &n, a, cs)?;
        }
        Ok(())
    }

    fn gen_struct_funs(&mut self, m: &Module, fields: &[(String, E)], env: &ModEnv) -> R<()> {
        let mut cx = Cx::new(env.clone(), "__struct__/0");
        let mut pairs = vec![(CE::atom("__struct__"), CE::atom(&m.name))];
        if m.exception {
            pairs.push((CE::atom("__exception__"), CE::atom("true")));
        }
        for (f, d) in fields {
            if f == "__exception__" {
                continue;
            }
            pairs.push((CE::atom(f), self.expr(d, &mut cx)?));
        }
        self.out.push(FunDef {
            key: FunKey::new(&m.name, "__struct__", 0),
            params: vec![],
            body: CE::Map(pairs),
            public: true,
        });
        let mut l = CE::Lit(Lit::Nil);
        for (f, _) in fields.iter().rev() {
            l = CE::Cons(Box::new(CE::atom(f)), Box::new(l));
        }
        self.out.push(FunDef {
            key: FunKey::new(&m.name, "__struct_fields__", 0),
            params: vec![],
            body: l,
            public: true,
        });
        let mut src = String::new();
        {
            let enforce: Vec<String> = m.enforce.iter().map(|k| format!(":{}", k)).collect();
            src.push_str(&format!(
                "def __struct__(kv) do\n  {{map, keys}} = Enum.reduce(kv, {{__struct__(), [{}]}}, fn {{key, val}}, {{map, keys}} -> if(:erlang.is_map_key(key, map), do: {{:maps.put(key, val, map), List.delete(keys, key)}}, else: raise(KeyError, key: key)) end)\n  case keys do\n    [] -> map\n    _ -> raise ArgumentError, \"the following keys must also be given when building struct #{{inspect(__MODULE__)}}: #{{inspect(keys)}}\"\n  end\nend\n",
                enforce.join(", ")
            ));
        }
        if m.exception {
            let has = |n: &str| m.defs.iter().any(|d| d.name == n && d.arity == 1);
            let has_message_field = fields.iter().any(|(f, _)| f == "message");
            if !has("exception") {
                if has_message_field {
                    src.push_str("def exception(msg) when is_binary(msg), do: %__MODULE__{message: msg}\n");
                }
                src.push_str("def exception(args) when is_list(args), do: Kernel.struct!(__MODULE__, args)\n");
            }
            if !has("message") {
                if has_message_field {
                    src.push_str("def message(exception), do: exception.message\n");
                } else {
                    src.push_str("def message(exception), do: \"got \" <> inspect(exception.__struct__) <> \" with message nil\"\n");
                }
            }
        }
        {
            if !src.is_empty() {
                let stmts = crate::parser::parse_source(&src, &format!("<defexception {}>", m.name))?;
                let mut defs = Vec::new();
                for s in stmts {
                    if let K::Call { args, .. } = &s.k {
                        let head = &args[0];
                        let (head, guard) = match &head.k {
                            K::Bin { op, l, r } if op == "when" => ((**l).clone(), Some((**r).clone())),
                            _ => (head.clone(), None),
                        };
                        if let K::Call { name, args: fargs, .. } = &head.k {
                            defs.push(DefC {
                                name: name.clone(),
                                arity: fargs.len(),
                                public: true,
                                args: fargs.clone(),
                                guard,
                                body: args[1].kw_get("do").cloned(),
                                env: Rc::new(env.clone()),
                                line: s.line,
                                is_macro: false,
                        super_target: None,
                            });
                        }
                    }
                }
                let mut groups: BTreeMap<(String, usize), Vec<DefC>> = BTreeMap::new();
                for d in defs {
                    groups.entry((d.name.clone(), d.arity)).or_default().push(d);
                }
                for ((n, a), cs) in groups {
                    self.expand_function(&m.name, &n, a, cs)?;
                }
            }
        }
        Ok(())
    }

    fn gen_protocol(&mut self, pname: &str) {
        let p = self.protocols.get(pname).unwrap().clone();
        let has_any = p.impls.iter().any(|i| i == "Elixir.Any");
        // Structs deriving this protocol.
        let derived: Vec<String> = self
            .mods
            .values()
            .filter(|m| m.derive.contains(&p.name))
            .map(|m| m.name.clone())
            .collect();
        let impl_mod = |t: &str| format!("{}.{}", p.name, t.strip_prefix("Elixir.").unwrap_or(t));
        for (f, arity) in &p.funs {
            let params: Vec<VarId> = (0..*arity).map(|_| self.new_var()).collect();
            let args: Vec<CE> = params.iter().map(|v| CE::Var(*v)).collect();
            let mut clauses = Vec::new();
            for t in &p.impls {
                if t == "Elixir.Any" {
                    continue;
                }
                clauses.push(Clause {
                    pats: vec![Pat::Lit(Lit::Atom(t.clone()))],
                    guard: None,
                    body: CE::Call(FunKey::new(&impl_mod(t), f, *arity), args.clone()),
                });
            }
            for d in &derived {
                if p.impls.contains(d) {
                    continue;
                }
                clauses.push(Clause {
                    pats: vec![Pat::Lit(Lit::Atom(d.clone()))],
                    guard: None,
                    body: CE::Call(FunKey::new(&impl_mod("Elixir.Any"), f, *arity), args.clone()),
                });
            }
            let fallback = if has_any && p.fallback_any {
                CE::Call(FunKey::new(&impl_mod("Elixir.Any"), f, *arity), args.clone())
            } else {
                CE::Call(FunKey::new(&p.name, "impl_for!", 1), vec![args[0].clone()])
            };
            clauses.push(Clause {
                pats: vec![Pat::Wild],
                guard: None,
                body: fallback,
            });
            let subject = if *arity > 0 {
                vec![CE::bif("tn_impl_for", vec![args[0].clone()])]
            } else {
                vec![CE::nil()]
            };
            self.out.push(FunDef {
                key: FunKey::new(&p.name, f, *arity),
                params,
                body: CE::Case(subject, clauses, Fail::CaseClause),
                public: true,
            });
        }
        // impl_for/1
        let v = self.new_var();
        let mut clauses = Vec::new();
        for t in &p.impls {
            if t == "Elixir.Any" {
                continue;
            }
            clauses.push(Clause {
                pats: vec![Pat::Lit(Lit::Atom(t.clone()))],
                guard: None,
                body: CE::atom(&impl_mod(t)),
            });
        }
        for d in &derived {
            clauses.push(Clause {
                pats: vec![Pat::Lit(Lit::Atom(d.clone()))],
                guard: None,
                body: CE::atom(&impl_mod("Elixir.Any")),
            });
        }
        clauses.push(Clause {
            pats: vec![Pat::Wild],
            guard: None,
            body: if has_any && p.fallback_any {
                CE::atom(&impl_mod("Elixir.Any"))
            } else {
                CE::nil()
            },
        });
        self.out.push(FunDef {
            key: FunKey::new(&p.name, "impl_for", 1),
            params: vec![v],
            body: CE::Case(vec![CE::bif("tn_impl_for", vec![CE::Var(v)])], clauses, Fail::CaseClause),
            public: true,
        });
        // impl_for!/1: raises Protocol.UndefinedError from its own frame
        let v3 = self.new_var();
        let exc = CE::Call(
            FunKey::new("Elixir.Protocol.UndefinedError", "exception", 1),
            vec![CE::Cons(
                Box::new(CE::Tuple(vec![CE::atom("protocol"), CE::atom(&p.name)])),
                Box::new(CE::Cons(
                    Box::new(CE::Tuple(vec![CE::atom("value"), CE::Var(v3)])),
                    Box::new(CE::Lit(Lit::Nil)),
                )),
            )],
        );
        let v4 = self.new_var();
        self.out.push(FunDef {
            key: FunKey::new(&p.name, "impl_for!", 1),
            params: vec![v3],
            body: CE::Case(
                vec![CE::Call(FunKey::new(&p.name, "impl_for", 1), vec![CE::Var(v3)])],
                vec![
                    Clause {
                        pats: vec![Pat::Lit(Lit::Atom("nil".into()))],
                        guard: None,
                        body: CE::bif("tn_error", vec![exc]),
                    },
                    Clause { pats: vec![Pat::Bind(v4)], guard: None, body: CE::Var(v4) },
                ],
                Fail::CaseClause,
            ),
            public: true,
        });
        let v2 = self.new_var();
        let mut impls: Vec<String> = p.impls.clone();
        impls.sort();
        let mut impl_list = CE::Lit(Lit::Nil);
        for i in impls.iter().rev() {
            impl_list = CE::Cons(Box::new(CE::atom(i)), Box::new(impl_list));
        }
        let mut funs = p.funs.clone();
        funs.sort();
        let mut functions = CE::Lit(Lit::Nil);
        for (name, arity) in funs.iter().rev() {
            let pair = CE::Tuple(vec![CE::atom(name), CE::Lit(Lit::Int(*arity as i64))]);
            functions = CE::Cons(Box::new(pair), Box::new(functions));
        }
        self.out.push(FunDef {
            key: FunKey::new(&p.name, "__protocol__", 1),
            params: vec![v2],
            body: CE::Case(
                vec![CE::Var(v2)],
                vec![
                    Clause {
                        pats: vec![Pat::Lit(Lit::Atom("impls".into()))],
                        guard: None,
                        body: if self.consolidated {
                            CE::Tuple(vec![CE::atom("consolidated"), impl_list])
                        } else {
                            CE::atom("not_consolidated")
                        },
                    },
                    Clause {
                        pats: vec![Pat::Lit(Lit::Atom("functions".into()))],
                        guard: None,
                        body: functions,
                    },
                    Clause {
                        pats: vec![Pat::Lit(Lit::Atom("consolidated?".into()))],
                        guard: None,
                        body: CE::atom(if self.consolidated { "true" } else { "false" }),
                    },
                    Clause {
                        pats: vec![Pat::Lit(Lit::Atom("module".into()))],
                        guard: None,
                        body: CE::atom(&p.name),
                    },
                ],
                Fail::CaseClause,
            ),
            public: true,
        });
    }

    fn expand_script(&mut self, script: Vec<(E, Rc<ModEnv>)>) -> R<()> {
        let env0 = script
            .first()
            .map(|(_, e)| (**e).clone())
            .unwrap_or(ModEnv {
                module: "Elixir.Tonic.Script".into(),
                aliases: HashMap::new(),
                imports: vec![],
                file: "<script>".into(),
                requires: HashSet::new(),
            });
        let mut cx = Cx::new(env0, "__script__/0");
        let mut body = Vec::new();
        let mut errors = Vec::new();
        for (e, env) in &script {
            cx.env = (**env).clone();
            match self.expr(e, &mut cx) {
                Ok(c) => body.push(c),
                Err(er) => errors.push(er),
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        body.insert(0, CE::Call(FunKey::new("Elixir.Tonic.Internal", "__boot__", 0), vec![]));
        // System.at_exit callbacks run once the script has finished.
        body.push(CE::Call(FunKey::new("Elixir.Tonic.Internal", "__run_at_exit__", 1), vec![CE::int(0)]));
        self.out.push(FunDef {
            key: FunKey::new("Elixir.Tonic.Script", "__script__", 0),
            params: vec![],
            body: CE::Block(body),
            public: true,
        });
        Ok(())
    }

    /// Keep only functions reachable from the script, user modules and the
    /// runtime entry points.
    pub fn reachable(&self, user_modules: &HashSet<String>, extra_roots: &[FunKey]) -> Vec<FunDef> {
        let by_key: HashMap<&FunKey, &FunDef> = self.out.iter().map(|f| (&f.key, f)).collect();
        let mut seen: HashSet<FunKey> = HashSet::new();
        let mut stack: Vec<FunKey> = Vec::new();
        let mut seen_mods: HashSet<String> = HashSet::new();
        let mut mod_queue: Vec<String> = Vec::new();
        let debug = std::env::var("TONIC_DEBUG_REACH").is_ok();
        let mut why: HashMap<String, String> = HashMap::new();
        for f in &self.out {
            if user_modules.contains(&f.key.module) || f.key.module == "Elixir.Tonic.Script" {
                stack.push(f.key.clone());
            }
        }
        stack.extend(extra_roots.iter().cloned());
        // debugging aid: extra roots as lines "Module.name/arity"
        if let Ok(path) = std::env::var("TONIC_DEBUG_ROOTS") {
            for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
                if let Some((mn, ar)) = line.rsplit_once('/') {
                    if let (Some((m, n)), Ok(ar)) = (mn.rsplit_once('.'), ar.parse()) {
                        stack.push(FunKey::new(m, n, ar));
                    }
                }
            }
        }
        for extra in [
            FunKey::new("Elixir.Tonic.Internal", "format_exit", 2),
            FunKey::new("Elixir.Tonic.Internal", "protocol_undefined", 2),
            // the runtime names this module in the error info of failed BIFs
            FunKey::new("erl_erts_errors", "format_error", ANY_ARITY),
        ] {
            stack.push(extra);
        }
        let all_mods: HashSet<&str> = self.out.iter().map(|f| f.key.module.as_str()).collect();
        // Public functions by module, for modules that may be called dynamically.
        let mut publics: HashMap<&str, Vec<&FunKey>> = HashMap::new();
        for f in &self.out {
            if f.public {
                publics.entry(f.key.module.as_str()).or_default().push(&f.key);
            }
        }
        // Atoms in reachable code; a dynamic call needs its function's name as one.
        let mut live_names: HashSet<String> = HashSet::new();
        // `mod.fun(args)` sites with an unknown module, and every atom that names a
        // module: each such module keeps its public fun/arity for each site
        let mut dyn_sites: HashSet<(String, usize)> = HashSet::new();
        let mut module_atoms: HashSet<String> = HashSet::new();
        // Code evaluated at run time can call any function by name, so a
        // program that evaluates code keeps every public function of a live module.
        let eval_entry = |k: &FunKey| {
            (k.module == "Elixir.Code"
                && (k.name.starts_with("eval") || k.name.starts_with("compile") || k.name == "require_file"))
                || k.module == "Elixir.Tonic.Eval"
        };
        // TONIC_KEEP_ALL=1 keeps every public function, for programs whose
        // dynamic calls the analysis cannot see (names built at run time)
        let mut keep_all = std::env::var("TONIC_KEEP_ALL").map(|v| v == "1").unwrap_or(false);
        // Modules the runtime refers to by fixed atoms (exceptions, etc.).
        // The builtin type names (Integer ... Port) are only protocol
        // dispatch keys, never call targets.
        let type_names = tonic_shared::atom::E_INTEGER as usize..=tonic_shared::atom::E_PORT as usize;
        for (i, a) in tonic_shared::FIXED_ATOMS.iter().enumerate() {
            if type_names.contains(&i) {
                continue;
            }
            if a.starts_with("Elixir.") && all_mods.contains(a) && seen_mods.insert(a.to_string()) {
                mod_queue.push(a.to_string());
            }
        }
        // Protocol dispatch clauses for struct types are only needed once the
        // struct's module is live: (struct module, refs, atoms).
        let mut conditional: Vec<(String, Vec<FunKey>, Vec<String>)> = Vec::new();
        let is_struct_mod = |m: &str| self.mods.get(m).map(|x| x.struct_fields.is_some()).unwrap_or(false);
        let mut arities: HashMap<(&str, &str), Vec<&FunKey>> = HashMap::new();
        for f in &self.out {
            arities.entry((f.key.module.as_str(), f.key.name.as_str())).or_default().push(&f.key);
        }
        // The evaluator counts only if the program certainly runs it: it is in
        // the static call closure of the roots, not just in code kept in case
        // of a dynamic call. Then every public function stays.
        {
            let mut certain: HashSet<FunKey> = HashSet::new();
            let mut work: Vec<FunKey> = stack.clone();
            while let Some(k) = work.pop() {
                if k.dynamic_module || k.dynamic_name {
                    if !certain.insert(k.clone()) {
                        continue;
                    }
                    work.extend(self.out.iter().filter(|f| dynamic_match(&k, f)).map(|f| f.key.clone()));
                    continue;
                }
                if k.arity == ANY_ARITY {
                    work.extend(arities.get(&(k.module.as_str(), k.name.as_str())).into_iter().flatten().map(|x| (*x).clone()));
                    continue;
                }
                if !certain.insert(k.clone()) {
                    continue;
                }
                if eval_entry(&k) {
                    keep_all = true;
                    break;
                }
                if let Some(f) = by_key.get(&k) {
                    let (mut r, mut a) = (Vec::new(), Vec::new());
                    collect_refs(&f.body, &mut r, &mut a);
                    work.extend(r.into_iter().filter(|x| !certain.contains(x)));
                }
            }
        }
        if keep_all {
            for m in &all_mods {
                if seen_mods.insert(m.to_string()) {
                    mod_queue.push(m.to_string());
                }
            }
        }
        let mut progressed_names = false;
        loop {
            while let Some(k) = stack.pop() {
                if k.dynamic_module || k.dynamic_name {
                    if !seen.insert(k.clone()) {
                        continue;
                    }
                    stack.extend(self.out.iter().filter(|f| dynamic_match(&k, f)).map(|f| f.key.clone()));
                    continue;
                }
                if k.arity == ANY_ARITY {
                    for key in arities.get(&(k.module.as_str(), k.name.as_str())).map(|v| v.as_slice()).unwrap_or(&[]) {
                        if !seen.contains(*key) {
                            stack.push((*key).clone());
                        }
                    }
                    continue;
                }
                if !seen.insert(k.clone()) {
                    continue;
                }
                if let Some(f) = by_key.get(&k) {
                    let mut refs = Vec::new();
                    let mut atoms = Vec::new();
                    let is_proto = self.protocols.contains_key(&f.key.module);
                    let app_spec = f.key.module == "application" && f.key.name == "builtin";
                    if let (true, CE::Case(_, clauses, _)) = (app_spec, &f.body) {
                        // an application's callback module and settings matter
                        // only once something uses that application
                        for c in clauses {
                            let (mut r2, mut a2) = (Vec::new(), Vec::new());
                            collect_refs(&c.body, &mut r2, &mut a2);
                            match c.pats.first() {
                                Some(Pat::Lit(Lit::Atom(app))) if app == "ex_unit" || app == "logger" => {
                                    let m = if app == "ex_unit" { "Elixir.ExUnit" } else { "Elixir.Logger" };
                                    conditional.push((m.to_string(), r2, a2));
                                }
                                Some(Pat::Lit(Lit::Atom(app))) if app == "elixir" => {
                                    // dbg_callback is never applied at run time: the
                                    // compiler expands dbg itself
                                    refs.extend(r2.into_iter().filter(|r| r.module != "Elixir.Macro"));
                                    atoms.extend(a2);
                                }
                                _ => {
                                    refs.extend(r2);
                                    atoms.extend(a2);
                                }
                            }
                        }
                    } else if is_proto && f.key.name == "__protocol__" {
                        // impl lists are informational; don't keep impls alive
                    } else if let (true, CE::Case(subj, clauses, _)) = (is_proto, &f.body) {
                        for sj in subj {
                            collect_refs(sj, &mut refs, &mut atoms);
                        }
                        for c in clauses {
                            match c.pats.first() {
                                Some(Pat::Lit(Lit::Atom(t))) if is_struct_mod(t) => {
                                    let mut r2 = Vec::new();
                                    let mut a2 = Vec::new();
                                    collect_refs(&c.body, &mut r2, &mut a2);
                                    conditional.push((t.clone(), r2, a2));
                                }
                                _ => collect_refs(&c.body, &mut refs, &mut atoms),
                            }
                        }
                    } else {
                        collect_refs(&f.body, &mut refs, &mut atoms);
                    }
                    for r in refs {
                        if !seen.contains(&r) {
                            if debug {
                                why.entry(format!("{}.{}/{}", r.module, r.name, r.arity)).or_insert(format!("{}.{}/{}", k.module, k.name, k.arity));
                                why.entry(r.module.clone()).or_insert(format!("{}.{}/{}", k.module, k.name, k.arity));
                            }
                            stack.push(r);
                        }
                    }
                    // atoms these functions hold are data (a struct's own tag,
                    // application metadata, cosmetic stack frames), never call targets
                    let data_only = matches!(
                        k.name.as_str(),
                        "__struct__" | "__struct_fields__" | "__inspect_derive__" | "__derive_opts__"
                    ) || matches!(
                        (k.module.as_str(), k.name.as_str()),
                        ("Elixir.Tonic.Internal", "format_exit") | ("elixir_errors", "parse_error")
                    );
                    let debug_atom = std::env::var("TONIC_DEBUG_ATOM").ok();
                    for a in atoms {
                        if debug_atom.as_deref() == Some(a.as_str()) {
                            eprintln!("atom {} held by {}.{}/{}", a, k.module, k.name, k.arity);
                        }
                        if let Some(site) = a.strip_prefix(DYN_SITE) {
                            if let Some((n, ar)) = site.rsplit_once('/') {
                                if dyn_sites.insert((n.to_string(), ar.parse().unwrap_or(0))) {
                                    progressed_names = true;
                                }
                            }
                            continue;
                        }
                        if all_mods.contains(a.as_str()) && module_atoms.insert(a.clone()) {
                            progressed_names = true;
                        }
                        if let Some(name) = a.strip_prefix(AST_ATOMS) {
                            // still a possible module for mod.fun(...) call sites
                            if all_mods.contains(name) && module_atoms.insert(name.to_string()) {
                                progressed_names = true;
                            }
                            live_names.insert(name.to_string());
                            continue;
                        }
                        if data_only {
                            live_names.insert(a);
                            continue;
                        }
                        // stdlib code is known, so its Erlang module atoms only
                        // matter through the call-site rules; user code is not
                        // stdlib code is known: its dynamic calls are covered by the
                        // call-site rules (callback tuples, consecutive literal
                        // arguments, mod.fun(...) sites, protocol dispatch). Atoms
                        // in user code may reach any function of their module.
                        let module_atom = user_modules.contains(&k.module) || k.module == "Elixir.Tonic.Script";
                        if module_atom && all_mods.contains(a.as_str()) && seen_mods.insert(a.clone()) {
                            if debug {
                                why.entry(a.clone()).or_insert(format!("atom in {}.{}/{}", k.module, k.name, k.arity));
                                eprintln!("dyn {} <- {}.{}/{}", a, k.module, k.name, k.arity);
                            }
                            mod_queue.push(a.clone());
                        }
                        live_names.insert(a);
                    }
                }
            }
            // Conditional protocol edges whose struct module became live.
            let live = |m: &str, seen: &HashSet<FunKey>, seen_mods: &HashSet<String>| {
                seen_mods.contains(m) || seen.iter().any(|k| k.module == m)
            };
            let mut progressed = false;
            let mut i = 0;
            while i < conditional.len() {
                if live(&conditional[i].0, &seen, &seen_mods) {
                    let (_, r, a) = conditional.swap_remove(i);
                    for x in r {
                        if !seen.contains(&x) {
                            stack.push(x);
                            progressed = true;
                        }
                    }
                    for m in a {
                        if m.starts_with("Elixir.") && all_mods.contains(m.as_str()) && seen_mods.insert(m.clone()) {
                            mod_queue.push(m.clone());
                            progressed = true;
                        }
                        live_names.insert(m);
                    }
                } else {
                    i += 1;
                }
            }
            // A module referenced by atom may be called dynamically. That only
            // reaches its public functions, and only those whose names are atoms
            // somewhere in reachable code; private functions stay if a kept one
            // calls them.
            mod_queue.clear();
            if progressed_names {
                progressed_names = false;
                for m in &module_atoms {
                    for (n, ar) in &dyn_sites {
                        let k = FunKey::new(m, n, *ar);
                        if !seen.contains(&k) && by_key.get(&k).is_some_and(|f| f.public) {
                            stack.push(k);
                            progressed = true;
                        }
                    }
                }
            }
            for m in &seen_mods {
                for k in publics.get(m.as_str()).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if !seen.contains(*k) && (keep_all || live_names.contains(&k.name)) {
                        if debug {
                            why.entry(format!("{}.{}/{}", k.module, k.name, k.arity)).or_insert(format!("dynamic {}", m));
                        }
                        stack.push((*k).clone());
                        progressed = true;
                    }
                }
            }
            if stack.is_empty() && !progressed {
                break;
            }
        }
        if let Ok(path) = std::env::var("TONIC_DEBUG_REACH_LIST") {
            let mut names: Vec<String> = seen.iter().map(|k| format!("{}.{}/{}", k.module, k.name, k.arity)).collect();
            names.sort();
            let _ = std::fs::write(path, names.join("\n"));
        }
        if std::env::var("TONIC_DEBUG_REACH").is_ok() {
            let mut per: HashMap<&str, usize> = HashMap::new();
            for k in &seen {
                *per.entry(k.module.as_str()).or_default() += 1;
            }
            let mut v: Vec<_> = per.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            for (m, n) in v.iter().take(40) {
                eprintln!("reach {} {}{}", n, m, if seen_mods.contains(*m) { " (by atom)" } else { "" });
            }
            eprintln!("reach modules {} by-atom {}", v.len(), seen_mods.len());
            if let Ok(target) = std::env::var("TONIC_DEBUG_WHY") {
                let mut cur = target;
                for _ in 0..30 {
                    let Some(p) = why.get(&cur).cloned() else { break };
                    eprintln!("why {} <- {}", cur, p);
                    cur = p.trim_start_matches("atom in ").to_string();
                    if !why.contains_key(&cur) {
                        let m = cur.rsplit_once('.').map(|x| x.0.to_string()).unwrap_or_default();
                        if why.contains_key(&m) { cur = m; }
                    }
                }
            }
        }
        self.out
            .iter()
            .filter(|f| seen.contains(&f.key))
            .cloned()
            .collect()
    }
}

/// `spawn(:proc_lib, :init_p, args)`, `apply(Mod, :fun, args)`: consecutive
/// literal atom arguments name the function a call reaches dynamically.
fn mfa_args(args: &[CE], refs: &mut Vec<FunKey>, atoms: &mut Vec<String>) {
    for w in args.windows(2) {
        if let (CE::Lit(Lit::Atom(m)), CE::Lit(Lit::Atom(f))) = (&w[0], &w[1]) {
            refs.push(FunKey::new(m, f, ANY_ARITY));
        }
    }
    // (module, :name, arity) as in make_fun / function_exported?, and
    // (module, :name, [args]) as in apply / spawn: a call site with a known
    // name and arity whatever the module is
    for w in args.windows(2) {
        if let CE::Lit(Lit::Atom(f)) = &w[0] {
            let arity = match &w[1] {
                CE::Lit(Lit::Int(n)) if *n >= 0 => Some(*n as usize),
                list @ (CE::Cons(..) | CE::Lit(Lit::Nil)) => fixed_length(list),
                _ => None,
            };
            if let Some(n) = arity {
                atoms.push(format!("{}{}/{}", DYN_SITE, f, n));
            }
        }
    }
}

/// Length of a list literal with no unknown tail.
fn fixed_length(e: &CE) -> Option<usize> {
    match e {
        CE::Lit(Lit::Nil) => Some(0),
        CE::Cons(_, t) => fixed_length(t).map(|n| n + 1),
        _ => None,
    }
}

/// Arity of a reference to every arity of a function (from a callback tuple).
const ANY_ARITY: usize = usize::MAX;
/// A `mod.fun(args)` call site with an unknown module: "name/arity".
const DYN_SITE: &str = "\u{0}dyn:";
/// Marks atoms from quoted AST data.
const AST_ATOMS: &str = "\u{0}ast:";

fn dynamic_match(target: &FunKey, function: &FunDef) -> bool {
    function.public
        && (target.dynamic_module || target.module == function.key.module)
        && (target.dynamic_name || target.name == function.key.name)
        && (target.arity == ANY_ARITY || target.arity == function.key.arity)
}

fn dynamic_mfa(module: &CE, name: &CE, arguments: &CE, refs: &mut Vec<FunKey>) {
    let module = match module { CE::Lit(Lit::Atom(value)) => Some(value.as_str()), _ => None };
    let name = match name { CE::Lit(Lit::Atom(value)) => Some(value.as_str()), _ => None };
    if module.is_none() || name.is_none() {
        let arity = match arguments {
            CE::Lit(Lit::Int(arity)) if *arity >= 0 => Some(*arity as usize),
            _ => fixed_length(arguments),
        };
        refs.push(FunKey::dynamic(module, name, arity.unwrap_or(ANY_ARITY)));
    }
}

fn collect_refs(e: &CE, refs: &mut Vec<FunKey>, atoms: &mut Vec<String>) {
    match e {
        CE::Lit(Lit::Atom(a)) => atoms.push(a.clone()),
        CE::Lit(_) | CE::Var(_) => {}
        // quoted AST {name, meta, args} built at run time: code only the
        // evaluator would run, so its atoms are data
        CE::Tuple(v) if v.len() == 3 && matches!(v[0], CE::Lit(Lit::Atom(_))) && matches!(v[1], CE::Cons(..) | CE::Lit(Lit::Nil)) => {
            let mut inner = Vec::new();
            for x in v {
                collect_refs(x, refs, &mut inner);
            }
            atoms.extend(inner.into_iter().map(|a| if a.starts_with(AST_ATOMS) || a.starts_with(DYN_SITE) { a } else { format!("{}{}", AST_ATOMS, a) }));
        }
        // {Module, :fun} / {Module, :fun, args}: a callback naming one function
        // (any arity); reachable() checks that Module is a module
        CE::Tuple(v)
            if (v.len() == 2 || v.len() == 3)
                && matches!((&v[0], &v[1]), (CE::Lit(Lit::Atom(_)), CE::Lit(Lit::Atom(_)))) =>
        {
            if let (CE::Lit(Lit::Atom(m)), CE::Lit(Lit::Atom(f))) = (&v[0], &v[1]) {
                refs.push(FunKey::new(m, f, ANY_ARITY));
                // the same shape is a child spec {Module, arg}
                refs.push(FunKey::new(m, "child_spec", ANY_ARITY));
                refs.push(FunKey::new(m, "start_link", ANY_ARITY));
                // the atoms stay ordinary too: {:via, Registry, name} and
                // {:key, Module} have this shape, and their modules are reached
                // through mod.fun(...) call sites
                atoms.push(m.clone());
                atoms.push(f.clone());
            }
            for x in &v[2..] {
                collect_refs(x, refs, atoms);
            }
        }
        CE::Bif(symbol, v) => {
            if matches!(symbol.as_str(), "tn_apply_mfa" | "tn_make_ext_fun" | "tn_spawn_mfa") && v.len() >= 3 {
                dynamic_mfa(&v[0], &v[1], &v[2], refs);
            }
            if symbol != "tn_function_exported" {
                mfa_args(v, refs, atoms);
            }
            for x in v {
                collect_refs(x, refs, atoms)
            }
        }
        CE::Tuple(v) | CE::Block(v) | CE::Interp(v) => {
            for x in v {
                collect_refs(x, refs, atoms)
            }
        }
        CE::Cons(a, b) => {
            collect_refs(a, refs, atoms);
            collect_refs(b, refs, atoms);
        }
        CE::Map(ps) => {
            for (a, b) in ps {
                if let (CE::Lit(Lit::Atom(k)), CE::Lit(Lit::Atom(m))) = (a, b) {
                    if k == "__struct__" {
                        // A struct literal makes the struct live without
                        // keeping every function of its module.
                        refs.push(FunKey::new(m, "__struct__", 0));
                        refs.push(FunKey::new(m, "__struct__", 1));
                        refs.push(FunKey::new(m, "__struct_fields__", 0));
                        refs.push(FunKey::new(m, "__inspect_derive__", 0));
                        refs.push(FunKey::new(m, "__derive_opts__", 1));
                        continue;
                    }
                }
                collect_refs(a, refs, atoms);
                collect_refs(b, refs, atoms);
            }
        }
        CE::MapUpdate(b, ps, _) => {
            collect_refs(b, refs, atoms);
            for (a, b) in ps {
                collect_refs(a, refs, atoms);
                collect_refs(b, refs, atoms);
            }
        }
        CE::Struct(m, ps) => {
            refs.push(FunKey::new(m, "__struct__", 0));
            refs.push(FunKey::new(m, "__struct_fields__", 0));
            refs.push(FunKey::new(m, "__inspect_derive__", 0));
            refs.push(FunKey::new(m, "__derive_opts__", 1));
            for (a, b) in ps {
                collect_refs(a, refs, atoms);
                collect_refs(b, refs, atoms);
            }
        }
        CE::Bin(segs) => {
            for s in segs {
                collect_refs(&s.val, refs, atoms);
                if let Some(sz) = &s.size {
                    collect_refs(sz, refs, atoms);
                }
            }
        }
        CE::Call(k, args) => {
            if matches!(k.module.as_str(), "erlang" | "Elixir.Kernel") && k.name == "apply" && args.len() == 3 {
                dynamic_mfa(&args[0], &args[1], &args[2], refs);
            }
            refs.push(k.clone());
            mfa_args(args, refs, atoms);
            for x in args {
                collect_refs(x, refs, atoms)
            }
        }
        CE::DynCall(m, name, args) => {
            let key = match m.as_ref() {
                CE::Lit(Lit::Atom(module)) => FunKey::new(module, name, args.len()),
                _ => FunKey::dynamic(None, Some(name), args.len()),
            };
            refs.push(key);
            atoms.push(name.clone());
            atoms.push(format!("{}{}/{}", DYN_SITE, name, args.len()));
            collect_refs(m, refs, atoms);
            for x in args {
                collect_refs(x, refs, atoms)
            }
        }
        CE::Apply(f, args) => {
            collect_refs(f, refs, atoms);
            for x in args {
                collect_refs(x, refs, atoms)
            }
        }
        CE::Fn(l) => {
            for c in &l.clauses {
                collect_clause(c, refs, atoms);
            }
        }
        CE::FunRef(key, t) => match t {
            Target::Fun(k) => refs.push(k.clone()),
            Target::Dynamic => {
                refs.push(key.clone());
                atoms.push(key.name.clone());
            }
            _ => {}
        },
        CE::Match(p, e) => {
            collect_pat(p, refs, atoms);
            collect_refs(e, refs, atoms);
        }
        CE::Case(subs, cs, _) => {
            for s in subs {
                collect_refs(s, refs, atoms);
            }
            for c in cs {
                collect_clause(c, refs, atoms);
            }
        }
        CE::If(a, b, c) => {
            collect_refs(a, refs, atoms);
            collect_refs(b, refs, atoms);
            collect_refs(c, refs, atoms);
        }
        CE::Receive(cs, after) => {
            for c in cs {
                collect_clause(c, refs, atoms);
            }
            if let Some((t, b)) = after {
                collect_refs(t, refs, atoms);
                collect_refs(b, refs, atoms);
            }
        }
        CE::Try(t) => {
            collect_refs(&t.body, refs, atoms);
            for c in t.catches.iter().chain(t.else_clauses.iter()) {
                collect_clause(c, refs, atoms);
            }
            if let Some(a) = &t.after {
                collect_refs(a, refs, atoms);
            }
        }
    }
}

fn collect_clause(c: &Clause, refs: &mut Vec<FunKey>, atoms: &mut Vec<String>) {
    for p in &c.pats {
        collect_pat(p, refs, atoms);
    }
    if let Some(g) = &c.guard {
        collect_refs(g, refs, atoms);
    }
    collect_refs(&c.body, refs, atoms);
}

fn collect_pat(p: &Pat, refs: &mut Vec<FunKey>, atoms: &mut Vec<String>) {
    match p {
        Pat::Map(ps) => {
            for (k, v) in ps {
                collect_refs(k, refs, atoms);
                collect_pat(v, refs, atoms);
            }
        }
        Pat::Tuple(ps) => {
            for x in ps {
                collect_pat(x, refs, atoms)
            }
        }
        Pat::Cons(a, b) | Pat::Alias(a, b) => {
            collect_pat(a, refs, atoms);
            collect_pat(b, refs, atoms);
        }
        Pat::Atom(a) => collect_pat(a, refs, atoms),
        Pat::Bin(segs) => {
            for s in segs {
                collect_pat(&s.val, refs, atoms);
                if let Some(sz) = &s.size {
                    collect_refs(sz, refs, atoms);
                }
            }
        }
        _ => {}
    }
}
