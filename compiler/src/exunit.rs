//! ExUnit.Case support at collection time: `test`, `describe`, `setup`,
//! `setup_all` and tags become ordinary functions plus the
//! `__ex_unit__/0,1,2` functions real ExUnit's runner reads (as
//! ExUnit.Case.__before_compile__ and ExUnit.Callbacks.__callbacks__ do).

use crate::ast::*;
use crate::collect::*;
use std::collections::HashMap;

/// A setup/setup_all callback: a local function or `{module, function}`.
#[derive(Clone, Debug)]
pub enum Callback {
    Local(String),
    Remote(E, String),
}

#[derive(Clone, Debug, Default)]
pub struct ExUnitState {
    /// `use ExUnit.Case` options.
    pub opts: Option<E>,
    /// (name, line, counter) of the describe block being collected.
    pub describe: Option<(String, u32, usize)>,
    /// (function, describe, line, tags)
    pub tests: Vec<(String, Option<(String, u32)>, u32, Vec<(String, E)>)>,
    pub setup: Vec<Callback>,
    pub setup_all: Vec<Callback>,
    /// Describe blocks in order: (name, setup function if any).
    pub describes: Vec<(String, Option<String>)>,
    pub pending_tags: Vec<(String, E)>,
    pub module_tags: Vec<(String, E)>,
    pub describe_tags: Vec<(String, E)>,
}

fn tag_pairs(v: &E) -> Vec<(String, E)> {
    let line = v.line;
    match &v.k {
        K::Atom(a) => vec![(a.clone(), E::atom("true", line))],
        K::List(items, _) => items.iter().flat_map(tag_pairs).collect(),
        K::Tuple(items) if items.len() == 2 => match &items[0].k {
            K::Atom(a) => vec![(a.clone(), items[1].clone())],
            _ => vec![],
        },
        _ => match v.as_keyword() {
            Some(kw) => kw.into_iter().map(|(k, x)| (k, x.clone())).collect(),
            None => vec![],
        },
    }
}

fn str_e(s: &str, line: u32) -> E {
    E::new(K::Str(s.as_bytes().to_vec()), line)
}

/// Parses one line of Elixir source placed at `line`, replacing the
/// placeholder variables in `subs`.
fn tmpl(src: &str, subs: &HashMap<String, E>, line: u32) -> R<Vec<E>> {
    let text = format!("{}{}", "\n".repeat(line.saturating_sub(1) as usize), src);
    let es = crate::parser::parse_source(&text, "<ex_unit>")?;
    Ok(es
        .iter()
        .map(|e| {
            map_expr(e, &mut |x: &E| match &x.k {
                K::Var(n) => subs.get(n).cloned(),
                _ => None,
            })
        })
        .collect())
}

fn atom_src(a: &str) -> String {
    format!(":\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `%{k => v, ...}` source for tag pairs (later pairs win), with
/// placeholders registered in `subs`.
fn tags_src(pairs: &[(String, E)], subs: &mut HashMap<String, E>) -> String {
    let mut keys: Vec<String> = Vec::new();
    let mut vals: HashMap<String, E> = HashMap::new();
    for (k, v) in pairs {
        if !vals.contains_key(k) {
            keys.push(k.clone());
        }
        vals.insert(k.clone(), v.clone());
    }
    let mut parts = Vec::new();
    for k in keys {
        let ph = format!("tonic_exu_{}", subs.len());
        subs.insert(ph.clone(), vals.remove(&k).unwrap());
        parts.push(format!("{} => {}", atom_src(&k), ph));
    }
    format!("%{{{}}}", parts.join(", "))
}

fn callbacks_of(v: &E) -> Option<Vec<Callback>> {
    match &v.k {
        K::Atom(a) => Some(vec![Callback::Local(a.clone())]),
        K::Tuple(items) if items.len() == 2 => match &items[1].k {
            K::Atom(f) => Some(vec![Callback::Remote(items[0].clone(), f.clone())]),
            _ => None,
        },
        K::List(items, _) => {
            let mut out = Vec::new();
            for i in items {
                out.extend(callbacks_of(i)?);
            }
            Some(out)
        }
        _ => None,
    }
}

/// Source of a setup chain (as ExUnit.Callbacks.compile_setup/2).
fn setup_chain_src(cbs: &[Callback], kind: &str, subs: &mut HashMap<String, E>) -> String {
    if cbs.is_empty() {
        return "context".into();
    }
    let calls: Vec<String> = cbs
        .iter()
        .map(|c| {
            let call = match c {
                Callback::Local(f) => format!("{}(context)", f),
                Callback::Remote(m, f) => {
                    let ph = format!("tonic_exu_{}", subs.len());
                    subs.insert(ph.clone(), m.clone());
                    format!("{}.{}(context)", ph, atom_src(f))
                }
            };
            format!(
                "(result = ExUnit.Callbacks.__merge__(__MODULE__, :{}, context, {}); ExUnit.Callbacks.__noop__(); result)",
                kind, call
            )
        })
        .collect();
    let mut src = String::new();
    for (i, c) in calls.iter().enumerate() {
        if i + 1 < calls.len() {
            src.push_str(&format!("context = {}; ", c));
        } else {
            src.push_str(c);
        }
    }
    src
}

impl Collector {
    pub fn exunit_tag(&mut self, module: &str, kind: &str, v: &E) -> bool {
        let m = self.mods.get_mut(module).unwrap();
        let Some(st) = m.exunit.as_mut() else { return false };
        let pairs = tag_pairs(v);
        match kind {
            "tag" => st.pending_tags.extend(pairs),
            "moduletag" => st.module_tags.extend(pairs),
            "describetag" => st.describe_tags.extend(pairs),
            _ => return false,
        }
        true
    }

    fn exu(&mut self, module: &str) -> &mut ExUnitState {
        self.mods.get_mut(module).unwrap().exunit.as_mut().unwrap()
    }

    /// Handles test/describe/setup/setup_all; returns false if `name` is
    /// not one of them (or the module doesn't use ExUnit.Case).
    pub fn exunit_stmt(
        &mut self,
        module: &str,
        name: &str,
        args: &[E],
        env: &mut ModEnv,
        attrs: &mut HashMap<String, E>,
        line: u32,
    ) -> R<bool> {
        if self.mods.get(module).and_then(|m| m.exunit.as_ref()).is_none() {
            return Ok(false);
        }
        let vars: crate::ctime::Vars = attrs
            .iter()
            .filter_map(|(k, v)| k.strip_prefix("\u{0}var:").map(|nm| (nm.to_string(), v.clone())))
            .collect();
        let eval_str = |e: &E| -> Option<String> {
            match crate::ctime::eval(e, &vars, attrs)?.k {
                K::Str(b) => String::from_utf8(b).ok(),
                K::Atom(a) => Some(a),
                _ => None,
            }
        };
        match name {
            "test" if !args.is_empty() && args.len() <= 3 => {
                let tname = eval_str(&args[0])
                    .ok_or_else(|| format!("{}:{}: test names must be strings", env.file, line))?;
                let (ctx, contents) = match args.len() {
                    1 => (E::var("_", line), None),
                    2 => (E::var("_", line), Some(args[1].clone())),
                    _ => (args[1].clone(), Some(args[2].clone())),
                };
                // As ExUnit.Case.test/3: the body is followed by `:ok` (or
                // wrapped in `try` when it has rescue/catch/after clauses).
                let body = contents.map(|c| {
                    let only_do = c.as_keyword().map(|kw| kw.len() == 1).unwrap_or(false);
                    let main = if only_do {
                        c.kw_get("do").cloned().unwrap()
                    } else {
                        E::call("try", vec![c.clone()], line)
                    };
                    E::new(K::Block(vec![main, E::atom("ok", line)]), line)
                });
                let st = self.exu(module);
                let describe = st.describe.clone();
                let fname = match &describe {
                    Some((d, _, _)) => format!("test {} {}", d, tname),
                    None => format!("test {}", tname),
                };
                // tags ++ tag ++ describetag ++ moduletag, earlier wins.
                let mut tags = st.module_tags.clone();
                if describe.is_some() {
                    tags.extend(st.describe_tags.clone());
                }
                tags.extend(std::mem::take(&mut st.pending_tags));
                let body = match body {
                    Some(b) => b,
                    None => {
                        tags.push(("not_implemented".into(), E::atom("true", line)));
                        E::new(
                            K::Remote {
                                recv: Box::new(E::new(K::Alias("ExUnit.Assertions".into()), line)),
                                name: "flunk".into(),
                                args: vec![str_e("Not implemented", line)],
                                parens: true,
                            },
                            line,
                        )
                    }
                };
                if st.tests.iter().any(|t| t.0 == fname) {
                    return Err(format!(
                        "{}:{}: ** (ArgumentError) \"{}\" is already defined in {}",
                        env.file,
                        line,
                        fname,
                        module.trim_start_matches("Elixir.")
                    ));
                }
                st.tests.push((fname.clone(), describe.map(|(d, l, _)| (d, l)), line, tags));
                let head = E::call(&fname, vec![ctx], line);
                let def_args = vec![head, E::list(vec![E::tuple(vec![E::atom("do", line), body], line)], line)];
                self.add_def(module, "def", &def_args, env, attrs, line)?;
                Ok(true)
            }
            "describe" if args.len() == 2 => {
                let dname = eval_str(&args[0])
                    .ok_or_else(|| format!("{}:{}: describe name must be a string", env.file, line))?;
                let body = args[1].kw_get("do").cloned().unwrap_or(E::nil(line));
                let saved_setup = {
                    let st = self.exu(module);
                    if st.describe.is_some() {
                        return Err(format!("{}:{}: cannot call \"describe\" inside another \"describe\". See the documentation for ExUnit.Case.describe/2 on named setups and how to handle hierarchies", env.file, line));
                    }
                    if st.describes.iter().any(|(d, _)| *d == dname) {
                        return Err(format!(
                            "{}:{}: ** (ArgumentError) describe {:?} is already defined in {}",
                            env.file,
                            line,
                            dname,
                            module.trim_start_matches("Elixir.")
                        ));
                    }
                    let counter = st.describes.len();
                    st.describe = Some((dname.clone(), line, counter));
                    st.describe_tags.clear();
                    std::mem::take(&mut st.setup)
                };
                for s in block_stmts(&body) {
                    self.module_stmt_inner(module, s, env, attrs)?;
                }
                let (dsetup, counter) = {
                    let st = self.exu(module);
                    let counter = st.describe.as_ref().unwrap().2;
                    st.describe = None;
                    st.describe_tags.clear();
                    (std::mem::replace(&mut st.setup, saved_setup), counter)
                };
                let fname = if dsetup.is_empty() {
                    None
                } else {
                    let f = format!("__ex_unit_describe_{}", counter);
                    let mut subs = HashMap::new();
                    let chain = setup_chain_src(&dsetup, "setup", &mut subs);
                    let src = format!("defp {}(context), do: ({})", f, chain);
                    for d in tmpl(&src, &subs, line)? {
                        if let K::Call { args, .. } = &d.k {
                            self.add_def(module, "defp", args, env, attrs, line)?;
                        }
                    }
                    Some(f)
                };
                self.exu(module).describes.push((dname, fname));
                Ok(true)
            }
            "setup" | "setup_all" if !args.is_empty() && args.len() <= 2 => {
                let all = name == "setup_all";
                let block = args.last().unwrap().kw_get("do").cloned();
                let st = self.exu(module);
                if all && st.describe.is_some() {
                    return Err(format!(
                        "{}:{}: ** (RuntimeError) cannot invoke setup_all/1-2 inside describe as setup_all always applies to all tests in a module",
                        env.file, line
                    ));
                }
                match block {
                    Some(b) => {
                        let fname = if all {
                            format!("__ex_unit_setup_all_{}", st.setup_all.len())
                        } else {
                            match &st.describe {
                                Some((_, _, c)) => format!("__ex_unit_setup_{}_{}", c, st.setup.len()),
                                None => format!("__ex_unit_setup_{}", st.setup.len()),
                            }
                        };
                        if all {
                            st.setup_all.push(Callback::Local(fname.clone()));
                        } else {
                            st.setup.push(Callback::Local(fname.clone()));
                        }
                        let ctx = if args.len() == 2 { args[0].clone() } else { E::var("_", line) };
                        let def_args = vec![
                            E::call(&fname, vec![ctx], line),
                            E::list(vec![E::tuple(vec![E::atom("do", line), b], line)], line),
                        ];
                        self.add_def(module, "defp", &def_args, env, attrs, line)?;
                    }
                    None => {
                        let cbs = callbacks_of(&args[0]).ok_or_else(|| {
                            format!(
                                "{}:{}: ** (ArgumentError) setup/setup_all expect a callback as an atom, a {{module, function}} tuple or a list of callbacks",
                                env.file, line
                            )
                        })?;
                        if all {
                            st.setup_all.extend(cbs);
                        } else {
                            st.setup.extend(cbs);
                        }
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Defines `__ex_unit__/0,1,2` (ExUnit.Case.__before_compile__).
    pub fn exunit_finish(&mut self, module: &str, env: &ModEnv, attrs: &HashMap<String, E>) -> R<()> {
        let Some(st) = self.mods.get(module).and_then(|m| m.exunit.clone()) else { return Ok(()) };
        let line = self.mods[module].line;
        let mut subs: HashMap<String, E> = HashMap::new();
        let file_ph = "tonic_exu_file".to_string();
        subs.insert(file_ph.clone(), str_e(&env.file, line));
        let mut tests = Vec::new();
        for (fname, describe, tline, tags) in &st.tests {
            let (d, dl) = match describe {
                Some((d, l)) => {
                    let ph = format!("tonic_exu_{}", subs.len());
                    subs.insert(ph.clone(), str_e(d, line));
                    (ph, l.to_string())
                }
                None => ("nil".to_string(), "nil".to_string()),
            };
            let tsrc = tags_src(tags, &mut subs);
            tests.push(format!(
                "%ExUnit.Test{{name: {}, case: __MODULE__, module: __MODULE__, tags: Map.merge({}, %{{line: {}, file: Path.expand(tonic_exu_file), registered: %{{}}, describe: {}, describe_line: {}, test_type: :test}})}}",
                atom_src(fname),
                tsrc,
                tline,
                d,
                dl
            ));
        }
        let mtags = tags_src(&st.module_tags, &mut subs);
        let opt = |k: &str, subs: &mut HashMap<String, E>, default: &str| -> String {
            match st.opts.as_ref().and_then(|o| o.kw_get(k)) {
                Some(v) => {
                    let ph = format!("tonic_exu_{}", subs.len());
                    subs.insert(ph.clone(), v.clone());
                    ph
                }
                None => default.to_string(),
            }
        };
        let async_ = opt("async", &mut subs, "false");
        let group = opt("group", &mut subs, "nil");
        let param = opt("parameterize", &mut subs, "nil");
        let setup_chain = setup_chain_src(&st.setup, "setup", &mut subs);
        let describe_clauses: Vec<String> = st
            .describes
            .iter()
            .filter_map(|(d, f)| {
                f.as_ref().map(|f| {
                    let ph = format!("tonic_exu_{}", subs.len());
                    subs.insert(ph.clone(), str_e(d, line));
                    format!("{} -> {}(context)", ph, f)
                })
            })
            .collect();
        let setup_body = if describe_clauses.is_empty() {
            setup_chain
        } else {
            format!(
                "context = ({}); case Map.get(context, :describe, nil) do {}; _ -> context end",
                setup_chain,
                describe_clauses.join("; ")
            )
        };
        let all_chain = setup_chain_src(&st.setup_all, "setup_all", &mut subs);
        let src = format!(
            "def __ex_unit__(:setup, context), do: ({}); def __ex_unit__(:setup_all, context), do: ({}); def __ex_unit__, do: %ExUnit.TestModule{{file: Path.expand(tonic_exu_file), name: __MODULE__, setup_all?: {}, tags: {}, tests: [{}]}}; def __ex_unit__(:config), do: %{{async?: {}, group: {}, parameterize: {}}}",
            setup_body,
            all_chain,
            if st.setup_all.is_empty() { "false" } else { "true" },
            mtags,
            tests.join(", "),
            async_,
            group,
            param
        );
        for d in tmpl(&src, &subs, line)? {
            let d = match d.k {
                K::Block(ref es) => es.clone(),
                _ => vec![d],
            };
            for d in d {
                if let K::Call { name, args, .. } = &d.k {
                    self.add_def(module, name, args, env, attrs, line)?;
                }
            }
        }
        self.exunit_modules.push(module.to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ExUnit.Assertions macros, expanded by the compiler.

use crate::core::CE;
use crate::expand::{Cx, Expander};

const OPERATORS: &[&str] = &["==", "<", ">", "<=", ">=", "===", "=~", "!==", "!=", "in"];

fn remote(m: &str, f: &str, args: Vec<E>, line: u32) -> E {
    E::new(
        K::Remote { recv: Box::new(E::new(K::Alias(m.into()), line)), name: f.into(), args, parens: true },
        line,
    )
}

fn kw(pairs: Vec<(&str, E)>, line: u32) -> E {
    E::list(pairs.into_iter().map(|(k, v)| E::tuple(vec![E::atom(k, line), v], line)).collect(), line)
}

fn block(es: Vec<E>, line: u32) -> E {
    E::new(K::Block(es), line)
}

fn quoted(e: E, line: u32) -> E {
    E::call("quote", vec![kw(vec![("prune_metadata", E::atom("true", line)), ("do", e)], line)], line)
}

fn raise_assertion(fields: Vec<(&str, E)>, line: u32) -> E {
    E::call("raise", vec![E::new(K::Alias("ExUnit.AssertionError".into()), line), kw(fields, line)], line)
}

fn interp_inspect(prefix: &str, v: E, line: u32) -> E {
    E::new(
        K::Interp(vec![IPart::Lit(prefix.as_bytes().to_vec()), IPart::Expr(E::call("inspect", vec![v], line))]),
        line,
    )
}

fn is_literal(e: &E) -> bool {
    match &e.k {
        K::Int(_) | K::Float(_) | K::Atom(_) | K::Str(_) => true,
        K::List(items, None) => items.iter().all(is_literal),
        K::Tuple(items) => items.iter().all(is_literal),
        _ => false,
    }
}

impl Expander {
    pub fn exunit_assertion(&mut self, name: &str, args: &[E], line: u32, cx: &mut Cx) -> Option<crate::collect::R<CE>> {
        let imported = cx.env.imports.iter().any(|i| i.module == "Elixir.ExUnit.Assertions");
        if !imported {
            return None;
        }
        let e = match (name, args.len()) {
            ("assert", 1) => self.assert_expr(&args[0], false, line),
            ("refute", 1) => self.assert_expr(&args[0], true, line),
            ("assert_receive", 1..=3) | ("assert_received", 1..=2) => {
                let timeout = if name == "assert_received" {
                    E::new(K::Int("0".into()), line)
                } else {
                    args.get(1).cloned().unwrap_or(E::nil(line))
                };
                let msg = if name == "assert_received" { args.get(1) } else { args.get(2) };
                self.assert_receive_expr(name, &args[0], timeout, msg.cloned(), line)
            }
            ("refute_receive", 1..=3) | ("refute_received", 1..=2) => {
                let timeout = if name == "refute_received" {
                    E::new(K::Int("0".into()), line)
                } else {
                    args.get(1).cloned().unwrap_or(E::new(K::Int("100".into()), line))
                };
                let msg = if name == "refute_received" { args.get(1) } else { args.get(2) };
                // receive do pattern = actual -> flunk(...) after timeout -> false end
                let pat = &args[0];
                let actual = E::var("refute_receive__actual", line);
                let fail = match msg {
                    Some(m) => remote("ExUnit.Assertions", "flunk", vec![m.clone()], line),
                    None => remote(
                        "ExUnit.Assertions",
                        "flunk",
                        vec![E::new(
                            K::Interp(vec![
                                IPart::Lit(b"Unexpectedly received message ".to_vec()),
                                IPart::Expr(E::call("inspect", vec![actual.clone()], line)),
                                IPart::Lit(format!(" (which matched {})", pattern_text(pat)).into_bytes()),
                            ]),
                            line,
                        )],
                        line,
                    ),
                };
                let clause = Clause { args: vec![E::bin("=", pat.clone(), actual.clone(), line)], guard: None, body: fail, line };
                E::call(
                    "receive",
                    vec![kw(
                        vec![
                            ("do", E::new(K::Clauses(vec![clause]), line)),
                            (
                                "after",
                                E::new(
                                    K::Clauses(vec![Clause {
                                        args: vec![timeout],
                                        guard: None,
                                        body: E::atom("false", line),
                                        line,
                                    }]),
                                    line,
                                ),
                            ),
                        ],
                        line,
                    )],
                    line,
                )
            }
            ("catch_throw", 1) | ("catch_exit", 1) | ("catch_error", 1) => {
                let kind = &name[6..];
                // try do expr; flunk "Expected to catch kind, got nothing" catch kind, x -> x end
                let x = E::var("catch__value", line);
                let flunk = remote(
                    "ExUnit.Assertions",
                    "flunk",
                    vec![str_e(&format!("Expected to catch {}, got nothing", kind), line)],
                    line,
                );
                let body = block(vec![args[0].clone(), flunk], line);
                let catch = Clause {
                    args: vec![E::atom(kind, line), x.clone()],
                    guard: None,
                    body: x,
                    line,
                };
                E::call(
                    "try",
                    vec![kw(vec![("do", body), ("catch", E::new(K::Clauses(vec![catch]), line))], line)],
                    line,
                )
            }
            _ => return None,
        };
        Some(self.expr(&e, cx))
    }

    fn assert_expr(&mut self, a: &E, refute: bool, line: u32) -> E {
        let kind = if refute { "refute" } else { "assert" };
        let code = quoted(E::call(kind, vec![a.clone()], line), line);
        // Comparison operators: show left/right.
        if let K::Bin { op, l, r } = &a.k {
            if OPERATORS.contains(&op.as_str()) {
                let left = E::var("assert__left", line);
                let right = E::var("assert__right", line);
                let call = E::bin(op, left.clone(), right.clone(), line);
                let (call, eq_check, message) = if refute {
                    (
                        E::new(K::Un { op: "not".into(), e: Box::new(call) }, line),
                        ["<=", ">=", "===", "==", "=~"].contains(&op.as_str()),
                        format!("Refute with {} failed", op),
                    )
                } else {
                    (call, ["<", ">", "!==", "!="].contains(&op.as_str()), format!("Assertion with {} failed", op))
                };
                let context = if op == "===" || op == "!==" { "===" } else { "==" };
                let main = remote(
                    "ExUnit.Assertions",
                    "assert",
                    vec![
                        call,
                        kw(
                            vec![
                                ("left", left.clone()),
                                ("right", right.clone()),
                                ("expr", code.clone()),
                                ("message", str_e(&message, line)),
                                ("context", E::atom(context, line)),
                            ],
                            line,
                        ),
                    ],
                    line,
                );
                let body = if eq_check {
                    E::call(
                        "if",
                        vec![
                            E::bin("===", left.clone(), right.clone(), line),
                            kw(
                                vec![
                                    (
                                        "do",
                                        remote(
                                            "ExUnit.Assertions",
                                            "assert",
                                            vec![
                                                E::atom("false", line),
                                                kw(
                                                    vec![
                                                        ("left", left.clone()),
                                                        ("expr", code.clone()),
                                                        ("message", str_e(&format!("{}, both sides are exactly equal", message), line)),
                                                    ],
                                                    line,
                                                ),
                                            ],
                                            line,
                                        ),
                                    ),
                                    ("else", main),
                                ],
                                line,
                            ),
                        ],
                        line,
                    )
                } else {
                    main
                };
                let stmts = vec![
                    E::bin("=", left, (**l).clone(), line),
                    E::bin("=", right, (**r).clone(), line),
                    body,
                ];
                return if refute {
                    E::new(K::Un { op: "!".into(), e: Box::new(block(stmts, line)) }, line)
                } else {
                    block(stmts, line)
                };
            }
            if op == "=" && !refute {
                // assert pattern = expr
                let right = E::var("assert__right", line);
                let pat = (**l).clone();
                let check = E::call(
                    "if",
                    vec![
                        right.clone(),
                        kw(
                            vec![
                                ("do", E::atom("ok", line)),
                                (
                                    "else",
                                    raise_assertion(
                                        vec![("expr", code.clone()), ("message", interp_inspect("Expected truthy, got ", right.clone(), line))],
                                        line,
                                    ),
                                ),
                            ],
                            line,
                        ),
                    ],
                    line,
                );
                let fail = raise_assertion(
                    vec![
                        ("left", quoted(pat.clone(), line)),
                        ("right", right.clone()),
                        ("expr", code.clone()),
                        ("message", str_e("match (=) failed", line)),
                        ("context", E::tuple(vec![E::atom("match", line), E::list(vec![], line)], line)),
                    ],
                    line,
                );
                let case = E::call(
                    "case",
                    vec![
                        right.clone(),
                        kw(
                            vec![(
                                "do",
                                E::new(
                                    K::Clauses(vec![
                                        Clause { args: vec![pat.clone()], guard: None, body: check, line },
                                        Clause { args: vec![E::var("_", line)], guard: None, body: fail, line },
                                    ]),
                                    line,
                                ),
                            )],
                            line,
                        ),
                    ],
                    line,
                );
                return block(
                    vec![E::bin("=", right.clone(), (**r).clone(), line), case, E::bin("=", pat, right.clone(), line), right],
                    line,
                );
            }
        }
        // match?/2
        if let K::Call { name, args, .. } = &a.k {
            if name == "match?" && args.len() == 2 {
                let right = E::var("assert__right", line);
                let m = E::call("match?", vec![args[0].clone(), right.clone()], line);
                let msg = if refute { "match (match?) succeeded, but should have failed" } else { "match (match?) failed" };
                return block(
                    vec![
                        E::bin("=", right.clone(), args[1].clone(), line),
                        remote(
                            "ExUnit.Assertions",
                            kind,
                            vec![
                                m,
                                kw(
                                    vec![
                                        ("right", right),
                                        ("left", quoted(args[0].clone(), line)),
                                        ("expr", code),
                                        ("message", str_e(msg, line)),
                                        ("context", E::tuple(vec![E::atom("match", line), E::list(vec![], line)], line)),
                                    ],
                                    line,
                                ),
                            ],
                            line,
                        ),
                    ],
                    line,
                );
            }
        }
        // General: arguments of a (non-special) call are reported.
        let value = E::var("assert__value", line);
        let mut pre = Vec::new();
        let mut args_e = E::atom("ex_unit_no_meaningful_value", line);
        let mut target = a.clone();
        let call_args: Option<Vec<E>> = match &a.k {
            K::Call { name, args, .. }
                if !args.is_empty()
                    && !matches!(name.as_str(), "match?" | "is_nil" | "not" | "!" | "and" | "or" | "if" | "unless" | "case" | "cond" | "receive" | "try" | "with" | "for" | "fn" | "quote")
                    && !args.iter().all(is_literal) =>
            {
                Some(args.clone())
            }
            K::Remote { args, .. } if !args.is_empty() && !args.iter().all(is_literal) => Some(args.clone()),
            _ => None,
        };
        if let Some(cargs) = call_args {
            let vars: Vec<E> = (1..=cargs.len()).map(|i| E::var(&format!("assert__arg{}", i), line)).collect();
            for (v, x) in vars.iter().zip(cargs.iter()) {
                pre.push(E::bin("=", v.clone(), x.clone(), line));
            }
            target = match &a.k {
                K::Call { name, parens, .. } => E::new(K::Call { name: name.clone(), args: vars.clone(), parens: *parens }, line),
                K::Remote { recv, name, parens, .. } => {
                    E::new(K::Remote { recv: recv.clone(), name: name.clone(), args: vars.clone(), parens: *parens }, line)
                }
                _ => unreachable!(),
            };
            args_e = E::list(vars, line);
        }
        let message = if refute {
            interp_inspect("Expected false or nil, got ", value.clone(), line)
        } else {
            interp_inspect("Expected truthy, got ", value.clone(), line)
        };
        let fail = raise_assertion(vec![("args", args_e), ("expr", code), ("message", message)], line);
        let (then, els) = if refute { (fail, value.clone()) } else { (value.clone(), fail) };
        pre.push(E::call(
            "if",
            vec![E::bin("=", value.clone(), target, line), kw(vec![("do", then), ("else", els)], line)],
            line,
        ));
        block(pre, line)
    }

    fn assert_receive_expr(&mut self, name: &str, pattern: &E, timeout: E, msg: Option<E>, line: u32) -> E {
        let code = quoted(E::call(name, vec![pattern.clone()], line), line);
        let received = E::var("assert__received", line);
        let tvar = E::var("assert__timeout", line);
        let on_timeout = match msg {
            Some(m) => remote("ExUnit.Assertions", "flunk", vec![m], line),
            None => remote(
                "ExUnit.Assertions",
                "__timeout__",
                vec![
                    quoted(pattern.clone(), line),
                    code,
                    E::list(vec![], line),
                    E::new(
                        K::Fn(vec![
                            Clause {
                                args: vec![E::var("assert__msg", line)],
                                guard: None,
                                body: E::call("match?", vec![pattern.clone(), E::var("assert__msg", line)], line),
                                line,
                            },
                        ]),
                        line,
                    ),
                    tvar.clone(),
                ],
                line,
            ),
        };
        let (pat, guard) = match &pattern.k {
            K::Bin { op, l, r } if op == "when" => ((**l).clone(), Some((**r).clone())),
            _ => (pattern.clone(), None),
        };
        let clause = Clause {
            args: vec![E::bin("=", pat.clone(), received.clone(), line)],
            guard,
            body: received.clone(),
            line,
        };
        let recv = E::call(
            "receive",
            vec![kw(
                vec![
                    ("do", E::new(K::Clauses(vec![clause]), line)),
                    (
                        "after",
                        E::new(K::Clauses(vec![Clause { args: vec![tvar.clone()], guard: None, body: on_timeout, line }]), line),
                    ),
                ],
                line,
            )],
            line,
        );
        // Variables bound by the pattern are visible after the assertion.
        let rebind = E::bin("=", pat, received.clone(), line);
        block(
            vec![
                E::bin(
                    "=",
                    tvar,
                    remote("ExUnit.Assertions", "__timeout__", vec![timeout, E::atom("assert_receive_timeout", line)], line),
                    line,
                ),
                E::bin("=", received.clone(), recv, line),
                rebind,
                received,
            ],
            line,
        )
    }
}

fn pattern_text(e: &E) -> String {
    let _ = e;
    "the pattern".to_string()
}
