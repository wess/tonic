//! Translate parsed Erlang into tonic's Elixir AST (E), preserving Erlang
//! semantics: no rebinding (already-bound variables in patterns are pinned),
//! variables bound in every branch of case/if/receive are exported, local
//! calls to undefined functions are auto-imported `erlang` BIFs, records are
//! tuples.

use crate::ast::{Clause, E, K};
use crate::erl_parse::{BinEl, Cl, Form, Q, X, XK};
use std::collections::{BTreeSet, HashMap, HashSet};

type Bound = BTreeSet<String>;
type R<T> = Result<T, String>;

pub struct Tr {
    pub module: String,
    file: String,
    records: HashMap<String, Vec<(String, Option<X>)>>,
    locals: HashSet<(String, usize)>,
    no_auto: HashSet<(String, usize)>,
    imports: HashMap<(String, usize), String>,
    tmp: usize,
}

fn atom(a: &str, l: u32) -> E {
    E::atom(a, l)
}
fn var(n: &str, l: u32) -> E {
    E::var(n, l)
}
fn int(v: i128, l: u32) -> E {
    E::new(K::Int(v.to_string()), l)
}
fn remote(m: &str, f: &str, args: Vec<E>, l: u32) -> E {
    E::new(
        K::Remote { recv: Box::new(atom(m, l)), name: f.to_string(), args, parens: true },
        l,
    )
}
fn kw(pairs: Vec<(&str, E)>, l: u32) -> E {
    E::list(pairs.into_iter().map(|(k, v)| E::tuple(vec![atom(k, l), v], l)).collect(), l)
}
fn block(mut es: Vec<E>, l: u32) -> E {
    if es.len() == 1 {
        es.pop().unwrap()
    } else {
        E::new(K::Block(es), l)
    }
}

/// Variables a pattern would bind (ignoring `_`).
fn pat_vars(x: &X, out: &mut Bound) {
    match &x.k {
        XK::Var(v) if v != "_" => {
            out.insert(v.clone());
        }
        XK::List(items, t) => {
            items.iter().for_each(|i| pat_vars(i, out));
            if let Some(t) = t {
                pat_vars(t, out);
            }
        }
        XK::Tuple(items) => items.iter().for_each(|i| pat_vars(i, out)),
        XK::Bin(els) => els.iter().for_each(|e| pat_vars(&e.val, out)),
        XK::Match(a, b) => {
            pat_vars(a, out);
            pat_vars(b, out);
        }
        XK::BinOp(op, a, b) if op == "++" => {
            pat_vars(a, out);
            pat_vars(b, out);
        }
        XK::Map(_, ps) => ps.iter().for_each(|(_, _, v)| pat_vars(v, out)),
        XK::Rec(_, _, fs) => fs.iter().for_each(|(_, v)| pat_vars(v, out)),
        _ => {}
    }
}

pub fn translate(forms: Vec<Form>, file: &str) -> R<E> {
    let mut module = String::new();
    let mut exports: HashSet<(String, usize)> = HashSet::new();
    let mut export_all = false;
    let mut records = HashMap::new();
    let mut locals = HashSet::new();
    let mut no_auto = HashSet::new();
    let mut imports = HashMap::new();
    let mut funs = Vec::new();
    for f in forms {
        match f {
            Form::Module(m) => module = m,
            Form::Export(fs) => exports.extend(fs),
            Form::ExportAll => export_all = true,
            Form::Record(n, fs) => {
                records.insert(n, fs);
            }
            Form::NoAutoImport(fs) => no_auto.extend(fs),
            Form::Import(m, fs) => {
                for fa in fs {
                    imports.insert(fa, m.clone());
                }
            }
            Form::Function(n, a, cls, l) => {
                locals.insert((n.clone(), a));
                funs.push((n, a, cls, l));
            }
            Form::Other => {}
        }
    }
    if module.is_empty() {
        return Err(format!("{}: missing -module", file));
    }
    let mut tr = Tr { module: module.clone(), file: file.to_string(), records, locals, no_auto, imports, tmp: 0 };
    let mut body = Vec::new();
    for (n, a, cls, _l) in funs {
        let public = export_all || exports.contains(&(n.clone(), a));
        for c in cls {
            body.push(tr.function_clause(&n, &c, public)?);
        }
    }
    let l = 1;
    Ok(E::call("defmodule", vec![atom(&module, l), kw(vec![("do", E::new(K::Block(body), l))], l)], l))
}

impl Tr {
    fn fresh(&mut self, base: &str) -> String {
        self.tmp += 1;
        format!("Tonic__{}{}", base, self.tmp)
    }

    fn err<Y>(&self, line: u32, m: &str) -> R<Y> {
        Err(format!("{}:{}: {}", self.file, line, m))
    }

    fn function_clause(&mut self, name: &str, c: &Cl, public: bool) -> R<E> {
        let l = c.line;
        let mut b = Bound::new();
        let mut pats = Vec::new();
        let before = b.clone();
        for p in &c.pats {
            pats.push(self.pat(p, &before, &mut b)?);
        }
        let head = E::new(K::Call { name: name.to_string(), args: pats, parens: true }, l);
        let head = match self.guard(&c.guard, &mut b.clone())? {
            Some(g) => E::bin("when", head, g, l),
            None => head,
        };
        let body = self.body(&c.body, &mut b)?;
        Ok(E::call(if public { "def" } else { "defp" }, vec![head, kw(vec![("do", body)], l)], l))
    }

    fn guard(&mut self, g: &[Vec<X>], b: &mut Bound) -> R<Option<E>> {
        if g.is_empty() {
            return Ok(None);
        }
        let mut alts = Vec::new();
        for conj in g {
            let mut acc: Option<E> = None;
            for t in conj {
                let e = self.expr(t, b)?;
                acc = Some(match acc {
                    None => e,
                    Some(a) => E::bin("and", a, e, t.line),
                });
            }
            alts.push(acc.unwrap());
        }
        let mut it = alts.into_iter().rev();
        let mut acc = it.next().unwrap();
        for a in it {
            let l = a.line;
            acc = E::bin("when", a, acc, l);
        }
        Ok(Some(acc))
    }

    fn body(&mut self, es: &[X], b: &mut Bound) -> R<E> {
        let l = es.first().map(|e| e.line).unwrap_or(1);
        let mut out = Vec::new();
        for e in es {
            out.push(self.expr(e, b)?);
        }
        Ok(E::new(K::Block(out), l))
    }

    // ------------------------------------------------------------- patterns

    /// Translate a pattern; `before` holds variables bound before the
    /// pattern (pinned), `b` receives the new bindings.
    fn pat(&mut self, x: &X, before: &Bound, b: &mut Bound) -> R<E> {
        let l = x.line;
        Ok(match &x.k {
            XK::Var(v) => {
                if v == "_" {
                    var("_", l)
                } else if before.contains(v) {
                    E::new(K::Un { op: "^".into(), e: Box::new(var(v, l)) }, l)
                } else {
                    b.insert(v.clone());
                    var(v, l)
                }
            }
            XK::Atom(a) => atom(a, l),
            XK::Int(v) => int(*v, l),
            XK::Float(f) => E::new(K::Float(*f), l),
            XK::Str(cps) => E::list(cps.iter().map(|c| int(*c as i128, l)).collect(), l),
            XK::List(items, tail) => {
                let mut its = Vec::new();
                for i in items {
                    its.push(self.pat(i, before, b)?);
                }
                let t = match tail {
                    Some(t) => Some(Box::new(self.pat(t, before, b)?)),
                    None => None,
                };
                E::new(K::List(its, t), l)
            }
            XK::Tuple(items) => {
                let mut its = Vec::new();
                for i in items {
                    its.push(self.pat(i, before, b)?);
                }
                E::tuple(its, l)
            }
            XK::Match(p1, p2) => {
                let a = self.pat(p1, before, b)?;
                let c = self.pat(p2, before, b)?;
                E::bin("=", a, c, l)
            }
            XK::BinOp(op, a, r) if op == "++" => {
                let left = match &a.k {
                    XK::Str(cps) => E::list(cps.iter().map(|c| int(*c as i128, l)).collect(), l),
                    _ => self.pat(a, before, b)?,
                };
                let right = self.pat(r, before, b)?;
                E::bin("++", left, right, l)
            }
            XK::Bin(els) => {
                let mut segs = Vec::new();
                for e in els {
                    segs.extend(self.bin_seg(e, true, before, b)?);
                }
                E::new(K::Bits(segs), l)
            }
            XK::Map(None, pairs) => {
                let mut ps = Vec::new();
                for (k, _, v) in pairs {
                    let ke = self.expr(k, &mut before.clone())?;
                    let ke = match ke.k {
                        K::Var(_) => E::new(K::Un { op: "^".into(), e: Box::new(ke.clone()) }, ke.line),
                        _ => ke,
                    };
                    ps.push((ke, self.pat(v, before, b)?));
                }
                E::new(K::Map(ps), l)
            }
            XK::Rec(None, name, fields) => {
                let defs = self.record(name, l)?;
                let mut its = vec![atom(name, l)];
                let wild = fields.iter().find(|(f, _)| f == "_").map(|(_, v)| v.clone());
                for (f, _) in &defs {
                    match fields.iter().find(|(g, _)| g == f) {
                        Some((_, v)) => its.push(self.pat(v, before, b)?),
                        None => match &wild {
                            Some(w) => its.push(self.pat(w, before, b)?),
                            None => its.push(var("_", l)),
                        },
                    }
                }
                E::tuple(its, l)
            }
            XK::RecIndex(r, f) => int(self.rec_index(r, f, l)? as i128, l),
            XK::UnOp(op, e) if op == "-" || op == "+" => {
                let inner = self.pat(e, before, b)?;
                E::new(K::Un { op: op.clone(), e: Box::new(inner) }, l)
            }
            // constant expressions in patterns (e.g. ?MACRO arithmetic)
            XK::BinOp(op, _, _) if matches!(op.as_str(), "+" | "-" | "*" | "bsl" | "bor" | "band") => {
                match const_eval(x) {
                    Some(v) => int(v, l),
                    None => return self.err(l, "illegal pattern"),
                }
            }
            _ => return self.err(l, &format!("illegal pattern {:?}", x.k)),
        })
    }

    fn record(&self, name: &str, l: u32) -> R<Vec<(String, Option<X>)>> {
        match self.records.get(name) {
            Some(d) => Ok(d.clone()),
            None => self.err(l, &format!("record {} undefined", name)),
        }
    }

    fn rec_index(&self, r: &str, f: &str, l: u32) -> R<usize> {
        let d = self.record(r, l)?;
        match d.iter().position(|(g, _)| g == f) {
            Some(i) => Ok(i + 2),
            None => self.err(l, &format!("field {} undefined in record {}", f, r)),
        }
    }

    /// One binary element -> Elixir segment(s).
    fn bin_seg(&mut self, el: &BinEl, is_pat: bool, before: &Bound, b: &mut Bound) -> R<Vec<E>> {
        let l = el.val.line;
        // String literal: bytes (latin1) unless a utf type is given.
        if let XK::Str(cps) = &el.val.k {
            let utf = el.tsl.iter().find(|(t, _)| t == "utf8" || t == "utf16" || t == "utf32");
            if let Some((t, _)) = utf {
                let s: String = cps.iter().filter_map(|c| char::from_u32(*c)).collect();
                let spec = self.seg_spec(&[(t.clone(), None)], None, l, before)?;
                return Ok(vec![E::bin("::", E::new(K::Str(s.into_bytes()), l), spec.unwrap(), l)]);
            }
            if el.size.is_none() && el.tsl.is_empty() && cps.iter().all(|c| *c < 256) {
                let bytes: Vec<u8> = cps.iter().map(|c| *c as u8).collect();
                if bytes.iter().all(|c| *c < 128) {
                    return Ok(vec![E::new(K::Str(bytes), l)]);
                }
                return Ok(bytes.iter().map(|c| int(*c as i128, l)).collect());
            }
            // each char as an element with the given size/types
            let mut out = Vec::new();
            for c in cps {
                let v = int(*c as i128, l);
                let spec = self.seg_spec(&el.tsl, el.size.as_ref(), l, before)?;
                out.push(match spec {
                    Some(s) => E::bin("::", v, s, l),
                    None => v,
                });
            }
            return Ok(out);
        }
        let v = if is_pat { self.pat(&el.val, before, b)? } else { self.expr(&el.val, b)? };
        let spec = self.seg_spec(&el.tsl, el.size.as_ref(), l, before)?;
        Ok(vec![match spec {
            Some(s) => E::bin("::", v, s, l),
            None => v,
        }])
    }

    fn seg_spec(&mut self, tsl: &[(String, Option<i128>)], size: Option<&X>, l: u32, before: &Bound) -> R<Option<E>> {
        let mut parts: Vec<E> = Vec::new();
        for (t, n) in tsl {
            let name = match t.as_str() {
                "bytes" => "binary",
                "bits" => "bitstring",
                x => x,
            };
            parts.push(match (name, n) {
                ("unit", Some(n)) => E::call("unit", vec![int(*n, l)], l),
                (x, _) => var(x, l),
            });
        }
        if let Some(s) = size {
            // sizes are expressions over already-bound variables
            let mut bb = before.clone();
            let se = self.expr(s, &mut bb)?;
            parts.push(E::call("size", vec![se], l));
        }
        let mut it = parts.into_iter();
        let mut acc = match it.next() {
            Some(p) => p,
            None => return Ok(None),
        };
        for p in it {
            acc = E::bin("-", acc, p, l);
        }
        Ok(Some(acc))
    }

    // ---------------------------------------------------------- expressions

    fn exprs(&mut self, xs: &[X], b: &mut Bound) -> R<Vec<E>> {
        // Sibling arguments don't see each other's bindings, but all
        // bindings are visible afterwards.
        let start = b.clone();
        let mut all = b.clone();
        let mut out = Vec::new();
        for x in xs {
            let mut bb = start.clone();
            out.push(self.expr(x, &mut bb)?);
            all.extend(bb);
        }
        *b = all;
        Ok(out)
    }

    pub fn expr(&mut self, x: &X, b: &mut Bound) -> R<E> {
        let l = x.line;
        Ok(match &x.k {
            XK::Var(v) => {
                if v == "_" {
                    return self.err(l, "variable '_' is unbound");
                }
                var(v, l)
            }
            XK::Atom(a) => atom(a, l),
            XK::Int(v) => int(*v, l),
            XK::Float(f) => E::new(K::Float(*f), l),
            XK::Str(cps) => E::list(cps.iter().map(|c| int(*c as i128, l)).collect(), l),
            XK::List(items, tail) => {
                let mut all: Vec<X> = items.clone();
                if let Some(t) = tail {
                    all.push((**t).clone());
                }
                let mut es = self.exprs(&all, b)?;
                let t = if tail.is_some() { Some(Box::new(es.pop().unwrap())) } else { None };
                E::new(K::List(es, t), l)
            }
            XK::Tuple(items) => E::tuple(self.exprs(items, b)?, l),
            XK::Bin(els) => {
                let mut segs = Vec::new();
                let start = b.clone();
                for e in els {
                    segs.extend(self.bin_seg(e, false, &start, b)?);
                }
                E::new(K::Bits(segs), l)
            }
            XK::Match(p, e) => {
                let rhs = self.expr(e, b)?;
                let before = b.clone();
                let lhs = self.pat(p, &before, b)?;
                E::bin("=", lhs, rhs, l)
            }
            XK::MaybeMatch(_, _) => return self.err(l, "?= outside maybe"),
            XK::BinOp(op, a, c) => {
                let es = self.exprs(&[(**a).clone(), (**c).clone()], b)?;
                let mut it = es.into_iter();
                let (ea, ec) = (it.next().unwrap(), it.next().unwrap());
                match op.as_str() {
                    "+" | "-" | "*" | "/" | "++" | "--" | "<" | ">" | ">=" | "==" => E::bin(op, ea, ec, l),
                    "=<" => E::bin("<=", ea, ec, l),
                    "/=" => E::bin("!=", ea, ec, l),
                    "=:=" => E::bin("===", ea, ec, l),
                    "=/=" => E::bin("!==", ea, ec, l),
                    "andalso" | "and" => E::bin("and", ea, ec, l),
                    "orelse" | "or" => E::bin("or", ea, ec, l),
                    "!" => remote("erlang", "send", vec![ea, ec], l),
                    "div" | "rem" | "band" | "bor" | "bxor" | "bsl" | "bsr" | "xor" => remote("erlang", op, vec![ea, ec], l),
                    _ => return self.err(l, &format!("unknown operator {}", op)),
                }
            }
            XK::UnOp(op, e) => {
                let inner = self.expr(e, b)?;
                match op.as_str() {
                    "-" | "+" | "not" => E::new(K::Un { op: op.clone(), e: Box::new(inner) }, l),
                    "bnot" => remote("erlang", "bnot", vec![inner], l),
                    _ => return self.err(l, "unknown unary operator"),
                }
            }
            XK::Call(f, args) => self.call(f, args, l, b)?,
            XK::Remote(_, _) => return self.err(l, "bad remote call"),
            XK::Case(e, cls) => {
                let scrut = self.expr(e, b)?;
                let (cls, exported) = self.cr_clauses(cls, b, None)?;
                let c = E::call("case", vec![scrut, kw(vec![("do", E::new(K::Clauses(cls), l))], l)], l);
                self.export(c, exported, b, l)
            }
            XK::If(cls) => {
                // case :ok do _ when G -> B; ...; _ -> error(if_clause) end
                let mut out = Vec::new();
                let mut sets: Vec<Bound> = Vec::new();
                for c in cls {
                    let mut bb = b.clone();
                    let g = self.guard(&c.guard, &mut bb.clone())?;
                    let body_es = self.body_list(&c.body, &mut bb)?;
                    sets.push(bb);
                    out.push((c.line, g, body_es));
                }
                let exported = intersect_new(&sets, b);
                let mut clauses = Vec::new();
                for (cl, g, body_es) in out {
                    let body = self.with_export_tuple(body_es, &exported, cl);
                    clauses.push(Clause { args: vec![var("_", cl)], guard: g, body, line: cl });
                }
                clauses.push(Clause {
                    args: vec![var("_", l)],
                    guard: None,
                    body: remote("erlang", "error", vec![atom("if_clause", l)], l),
                    line: l,
                });
                let c = E::call("case", vec![atom("ok", l), kw(vec![("do", E::new(K::Clauses(clauses), l))], l)], l);
                self.export(c, exported, b, l)
            }
            XK::Receive(cls, after) => {
                let (cls, mut exported, after_e) = if cls.is_empty() {
                    let (t, body) = after.as_ref().unwrap();
                    let te = self.expr(t, &mut b.clone())?;
                    let mut bb = b.clone();
                    let be = self.body(body, &mut bb)?;
                    (vec![], BTreeSet::new(), Some(vec![Clause { args: vec![te], guard: None, body: be, line: t.line }]))
                } else {
                    let after_pre = match after {
                        Some((t, body)) => {
                            let te = self.expr(t, &mut b.clone())?;
                            let mut bb = b.clone();
                            let be = self.body_list(body, &mut bb)?;
                            Some((te, be, bb, t.line))
                        }
                        None => None,
                    };
                    let extra = after_pre.as_ref().map(|(_, _, bb, _)| bb.clone());
                    let (cls, exported) = self.cr_clauses(cls, b, extra)?;
                    let after_e = after_pre.map(|(te, be, _, tl)| {
                        let body = self.with_export_tuple(be, &exported, tl);
                        vec![Clause { args: vec![te], guard: None, body, line: tl }]
                    });
                    (cls, exported, after_e)
                };
                if cls.is_empty() {
                    exported = BTreeSet::new();
                }
                let mut sections = vec![(
                    "do",
                    if cls.is_empty() { E::new(K::Block(vec![]), l) } else { E::new(K::Clauses(cls), l) },
                )];
                if let Some(a) = after_e {
                    sections.push(("after", E::new(K::Clauses(a), l)));
                }
                let c = E::call("receive", vec![kw(sections, l)], l);
                self.export(c, exported, b, l)
            }
            XK::Try(body, of, catches, after) => self.try_expr(body, of, catches, after, l, b)?,
            XK::Catch(e) => {
                let inner = self.expr(e, &mut b.clone())?;
                let v = self.fresh("v");
                let cls = vec![
                    Clause { args: vec![atom("throw", l), var(&v, l)], guard: None, body: var(&v, l), line: l },
                    Clause {
                        args: vec![atom("error", l), var(&v, l)],
                        guard: None,
                        body: E::tuple(vec![atom("EXIT", l), E::tuple(vec![var(&v, l), var("__STACKTRACE__", l)], l)], l),
                        line: l,
                    },
                    Clause {
                        args: vec![atom("exit", l), var(&v, l)],
                        guard: None,
                        body: E::tuple(vec![atom("EXIT", l), var(&v, l)], l),
                        line: l,
                    },
                ];
                E::call("try", vec![kw(vec![("do", inner), ("catch", E::new(K::Clauses(cls), l))], l)], l)
            }
            XK::Block(es) => {
                let v = self.body_list(es, b)?;
                block(v, l)
            }
            XK::Fun(name, cls) => self.fun(name.as_deref(), cls, l, b)?,
            XK::FunRef(m, f, a) => self.fun_ref(m.as_deref(), f, a, l, b)?,
            XK::LC(e, qs) => self.comprehension(qs, l, b, None, |s, bb| s.expr(e, bb))?,
            XK::BC(e, qs) => {
                let into = E::new(K::Str(vec![]), l);
                self.comprehension(qs, l, b, Some(into), |s, bb| {
                    let v = s.expr(e, bb)?;
                    Ok(match v.k {
                        K::Bits(_) => v,
                        _ => {
                            let ll = v.line;
                            E::new(K::Bits(vec![E::bin("::", v, var("bitstring", ll), ll)]), ll)
                        }
                    })
                })?
            }
            XK::MC(k, v, qs) => {
                let into = E::new(K::Map(vec![]), l);
                self.comprehension(qs, l, b, Some(into), |s, bb| {
                    let ke = s.expr(k, bb)?;
                    let ve = s.expr(v, bb)?;
                    Ok(E::tuple(vec![ke, ve], l))
                })?
            }
            XK::Map(None, pairs) => {
                let mut flat = Vec::new();
                for (k, _, v) in pairs {
                    flat.push(k.clone());
                    flat.push(v.clone());
                }
                let es = self.exprs(&flat, b)?;
                let mut ps = Vec::new();
                let mut it = es.into_iter();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    ps.push((k, v));
                }
                E::new(K::Map(ps), l)
            }
            XK::Map(Some(base), pairs) => {
                let mut acc = self.expr(base, b)?;
                let mut i = 0;
                while i < pairs.len() {
                    let exact = pairs[i].1;
                    let mut group = Vec::new();
                    while i < pairs.len() && pairs[i].1 == exact {
                        let k = self.expr(&pairs[i].0, b)?;
                        let v = self.expr(&pairs[i].2, b)?;
                        group.push((k, v));
                        i += 1;
                    }
                    acc = if exact {
                        E::new(K::MapUpd(Box::new(acc), group), l)
                    } else {
                        remote("maps", "merge", vec![acc, E::new(K::Map(group), l)], l)
                    };
                }
                acc
            }
            XK::Rec(None, name, fields) => {
                let defs = self.record(name, l)?;
                let wild = fields.iter().find(|(f, _)| f == "_").map(|(_, v)| v.clone());
                let mut vals = vec![atom(name, l)];
                for (f, def) in &defs {
                    let e = match fields.iter().find(|(g, _)| g == f) {
                        Some((_, v)) => self.expr(v, b)?,
                        None => match (&wild, def) {
                            (Some(w), _) => self.expr(w, b)?,
                            (None, Some(d)) => self.expr(d, &mut Bound::new())?,
                            (None, None) => atom("undefined", l),
                        },
                    };
                    vals.push(e);
                }
                E::tuple(vals, l)
            }
            XK::Rec(Some(base), name, fields) => {
                let be = self.expr(base, b)?;
                let t = self.fresh("rec");
                let mut acc = var(&t, l);
                for (f, v) in fields {
                    let i = self.rec_index(name, f, l)?;
                    let ve = self.expr(v, b)?;
                    acc = remote("erlang", "setelement", vec![int(i as i128, l), acc, ve], l);
                }
                let size = self.record(name, l)?.len() + 1;
                // badrecord check like the BEAM
                let check = E::call(
                    "case",
                    vec![
                        var(&t, l),
                        kw(
                            vec![(
                                "do",
                                E::new(
                                    K::Clauses(vec![
                                        Clause {
                                            args: vec![var("_", l)],
                                            guard: Some(E::bin(
                                                "and",
                                                remote("erlang", "is_tuple", vec![var(&t, l)], l),
                                                E::bin(
                                                    "and",
                                                    E::bin("==", remote("erlang", "tuple_size", vec![var(&t, l)], l), int(size as i128, l), l),
                                                    E::bin("===", remote("erlang", "element", vec![int(1, l), var(&t, l)], l), atom(name, l), l),
                                                    l,
                                                ),
                                                l,
                                            )),
                                            body: acc,
                                            line: l,
                                        },
                                        Clause {
                                            args: vec![var("_", l)],
                                            guard: None,
                                            body: remote("erlang", "error", vec![E::tuple(vec![atom("badrecord", l), var(&t, l)], l)], l),
                                            line: l,
                                        },
                                    ]),
                                    l,
                                ),
                            )],
                            l,
                        ),
                    ],
                    l,
                );
                block(vec![E::bin("=", var(&t, l), be, l), check], l)
            }
            XK::RecField(e, r, f) => {
                let ee = self.expr(e, b)?;
                let i = self.rec_index(r, f, l)?;
                remote("erlang", "element", vec![int(i as i128, l), ee], l)
            }
            XK::RecIndex(r, f) => int(self.rec_index(r, f, l)? as i128, l),
            XK::Maybe(body, els) => self.maybe(body, els, l, b)?,
        })
    }

    fn body_list(&mut self, es: &[X], b: &mut Bound) -> R<Vec<E>> {
        let mut out = Vec::new();
        for e in es {
            out.push(self.expr(e, b)?);
        }
        Ok(out)
    }

    /// Case/receive clauses; returns translated clauses and the exported
    /// variables (bound in every clause, plus `extra` from an after body).
    fn cr_clauses(&mut self, cls: &[Cl], b: &Bound, extra: Option<Bound>) -> R<(Vec<Clause>, Bound)> {
        let mut parts = Vec::new();
        let mut sets = Vec::new();
        for c in cls {
            let mut bb = b.clone();
            let p = self.pat(&c.pats[0], b, &mut bb)?;
            let g = self.guard(&c.guard, &mut bb.clone())?;
            let body = self.body_list(&c.body, &mut bb)?;
            sets.push(bb);
            parts.push((c.line, p, g, body));
        }
        if let Some(e) = extra {
            sets.push(e);
        }
        let exported = intersect_new(&sets, b);
        let mut out = Vec::new();
        for (cl, p, g, body) in parts {
            let body = self.with_export_tuple(body, &exported, cl);
            out.push(Clause { args: vec![p], guard: g, body, line: cl });
        }
        Ok((out, exported))
    }

    /// Make a clause body return `{value, V1, ..., Vn}` for exported vars.
    fn with_export_tuple(&mut self, mut body: Vec<E>, exported: &Bound, l: u32) -> E {
        if exported.is_empty() {
            return E::new(K::Block(body), l);
        }
        let last = body.pop().unwrap_or_else(|| atom("ok", l));
        let t = self.fresh("r");
        let ll = last.line;
        body.push(E::bin("=", var(&t, ll), last, ll));
        let mut items = vec![var(&t, ll)];
        items.extend(exported.iter().map(|v| var(v, ll)));
        body.push(E::tuple(items, ll));
        E::new(K::Block(body), l)
    }

    /// Bind exported variables from a construct returning export tuples.
    fn export(&mut self, construct: E, exported: Bound, b: &mut Bound, l: u32) -> E {
        if exported.is_empty() {
            return construct;
        }
        let t = self.fresh("x");
        let mut items = vec![var(&t, l)];
        items.extend(exported.iter().map(|v| var(v, l)));
        b.extend(exported.iter().cloned());
        block(vec![E::bin("=", E::tuple(items, l), construct, l), var(&t, l)], l)
    }

    fn call(&mut self, f: &X, args: &[X], l: u32, b: &mut Bound) -> R<E> {
        match &f.k {
            XK::Atom(name) => {
                let n = args.len();
                // compile-time record helpers
                if name == "record_info" && n == 2 {
                    if let (XK::Atom(what), XK::Atom(rec)) = (&args[0].k, &args[1].k) {
                        let d = self.record(rec, l)?;
                        return Ok(match what.as_str() {
                            "size" => int(d.len() as i128 + 1, l),
                            _ => E::list(d.iter().map(|(f, _)| atom(f, l)).collect(), l),
                        });
                    }
                }
                if name == "is_record" && (n == 2 || n == 3) && !self.locals.contains(&(name.clone(), n)) {
                    if let XK::Atom(rec) = &args[1].k {
                        let size = match self.records.get(rec) {
                            Some(d) => d.len() + 1,
                            None => match args.get(2).map(|a| &a.k) {
                                Some(XK::Int(s)) => *s as usize,
                                _ => return self.err(l, "is_record/2 of unknown record"),
                            },
                        };
                        let v = self.expr(&args[0], b)?;
                        return Ok(E::bin(
                            "and",
                            remote("erlang", "is_tuple", vec![v.clone()], l),
                            E::bin(
                                "and",
                                E::bin("==", remote("erlang", "tuple_size", vec![v.clone()], l), int(size as i128, l), l),
                                E::bin("===", remote("erlang", "element", vec![int(1, l), v], l), atom(rec, l), l),
                                l,
                            ),
                            l,
                        ));
                    }
                }
                let a = self.exprs(args, b)?;
                if self.locals.contains(&(name.clone(), n)) {
                    Ok(remote(&self.module.clone(), name, a, l))
                } else if let Some(m) = self.imports.get(&(name.clone(), n)) {
                    Ok(remote(&m.clone(), name, a, l))
                } else {
                    // auto-imported BIF
                    Ok(remote("erlang", name, a, l))
                }
            }
            XK::Remote(m, fname) => {
                let mut all = vec![(**m).clone(), (**fname).clone()];
                all.extend(args.iter().cloned());
                let mut es = self.exprs(&all, b)?;
                let rest = es.split_off(2);
                let fe = es.pop().unwrap();
                let me = es.pop().unwrap();
                match (&me.k, &fe.k) {
                    (K::Atom(m), K::Atom(fnm)) => Ok(remote(m, fnm, rest, l)),
                    (_, K::Atom(fnm)) => Ok(E::new(
                        K::Remote { recv: Box::new(me.clone()), name: fnm.clone(), args: rest, parens: true },
                        l,
                    )),
                    _ => Ok(remote("erlang", "apply", vec![me, fe, E::list(rest, l)], l)),
                }
            }
            _ => {
                let mut all = vec![f.clone()];
                all.extend(args.iter().cloned());
                let mut es = self.exprs(&all, b)?;
                let rest = es.split_off(1);
                let fe = es.pop().unwrap();
                Ok(E::new(K::AnonCall { f: Box::new(fe), args: rest }, l))
            }
        }
    }

    fn fun(&mut self, name: Option<&str>, cls: &[Cl], l: u32, b: &Bound) -> R<E> {
        let arity = cls[0].pats.len();
        let selfv = name.map(|_| self.fresh("self"));
        let mut out = Vec::new();
        for c in cls {
            // fun heads shadow outer variables
            let mut bb = b.clone();
            let empty = Bound::new();
            let mut pats = Vec::new();
            if let Some(s) = &selfv {
                pats.push(var(s, c.line));
            }
            let mut newb = Bound::new();
            for p in &c.pats {
                pats.push(self.pat(p, &empty, &mut newb)?);
            }
            for v in &newb {
                bb.insert(v.clone());
            }
            let g = self.guard(&c.guard, &mut bb.clone())?;
            let mut body = Vec::new();
            if let (Some(n), Some(s)) = (name, &selfv) {
                // Name = fun(A1..An) -> Self(Self, A1..An) end
                let ps: Vec<String> = (0..arity).map(|i| format!("Tonic__a{}", i)).collect();
                let mut cargs = vec![var(s, c.line)];
                cargs.extend(ps.iter().map(|p| var(p, c.line)));
                let rec = E::new(
                    K::Fn(vec![Clause {
                        args: ps.iter().map(|p| var(p, c.line)).collect(),
                        guard: None,
                        body: E::new(K::AnonCall { f: Box::new(var(s, c.line)), args: cargs }, c.line),
                        line: c.line,
                    }]),
                    c.line,
                );
                body.push(E::bin("=", var(n, c.line), rec, c.line));
                bb.insert(n.to_string());
            }
            body.extend(self.body_list(&c.body, &mut bb)?);
            out.push(Clause { args: pats, guard: g, body: E::new(K::Block(body), c.line), line: c.line });
        }
        let f = E::new(K::Fn(out), l);
        match &selfv {
            None => Ok(f),
            Some(s) => {
                let t = self.fresh("named");
                let ps: Vec<String> = (0..arity).map(|i| format!("Tonic__a{}", i)).collect();
                let mut cargs = vec![var(&t, l)];
                cargs.extend(ps.iter().map(|p| var(p, l)));
                let _ = s;
                let wrapper = E::new(
                    K::Fn(vec![Clause {
                        args: ps.iter().map(|p| var(p, l)).collect(),
                        guard: None,
                        body: E::new(K::AnonCall { f: Box::new(var(&t, l)), args: cargs }, l),
                        line: l,
                    }]),
                    l,
                );
                Ok(block(vec![E::bin("=", var(&t, l), f, l), wrapper], l))
            }
        }
    }

    fn fun_ref(&mut self, m: Option<&X>, f: &X, a: &X, l: u32, b: &mut Bound) -> R<E> {
        match (m.map(|m| &m.k), &f.k, &a.k) {
            (None, XK::Atom(n), XK::Int(ar)) => {
                let ar = *ar as usize;
                let md = if self.locals.contains(&(n.clone(), ar)) { self.module.clone() } else { "erlang".into() };
                Ok(capture(&md, n, ar, l))
            }
            (Some(XK::Atom(md)), XK::Atom(n), XK::Int(ar)) => Ok(capture(md, n, *ar as usize, l)),
            _ => {
                let me = match m {
                    Some(m) => self.expr(m, b)?,
                    None => atom(&self.module.clone(), l),
                };
                let fe = self.expr(f, b)?;
                let ae = self.expr(a, b)?;
                Ok(remote("erlang", "make_fun", vec![me, fe, ae], l))
            }
        }
    }

    fn comprehension(
        &mut self,
        qs: &[Q],
        l: u32,
        b: &Bound,
        into: Option<E>,
        body: impl FnOnce(&mut Tr, &mut Bound) -> R<E>,
    ) -> R<E> {
        let mut bb = b.clone();
        let mut args = Vec::new();
        for q in qs {
            match q {
                Q::Gen(p, src) => {
                    let se = self.expr(src, &mut bb.clone())?;
                    // generator patterns shadow outer variables
                    let mut nb = Bound::new();
                    let pe = self.pat(p, &Bound::new(), &mut nb)?;
                    bb.extend(nb);
                    args.push(E::bin("<-", pe, se, p.line));
                }
                Q::BGen(p, src) => {
                    let se = self.expr(src, &mut bb.clone())?;
                    let mut nb = Bound::new();
                    let els = match &p.k {
                        XK::Bin(els) => els.clone(),
                        _ => return self.err(p.line, "bad binary generator"),
                    };
                    let empty = Bound::new();
                    let mut segs = Vec::new();
                    for e in &els {
                        segs.extend(self.bin_seg(e, true, &empty, &mut nb)?);
                    }
                    let last = segs.pop().unwrap();
                    segs.push(E::bin("<-", last, se, p.line));
                    bb.extend(nb);
                    args.push(E::new(K::Bits(segs), p.line));
                }
                Q::MGen(k, v, src) => {
                    let se = self.expr(src, &mut bb.clone())?;
                    let mut nb = Bound::new();
                    let kp = self.pat(k, &Bound::new(), &mut nb)?;
                    let vp = self.pat(v, &Bound::new(), &mut nb)?;
                    bb.extend(nb);
                    args.push(E::bin("<-", E::tuple(vec![kp, vp], k.line), remote("maps", "to_list", vec![se], k.line), k.line));
                }
                Q::Filter(e) => {
                    let fe = self.expr(e, &mut bb.clone())?;
                    args.push(fe);
                }
            }
        }
        let be = body(self, &mut bb)?;
        let mut opts = Vec::new();
        if let Some(i) = into {
            opts.push(("into", i));
        }
        opts.push(("do", be));
        args.push(kw(opts, l));
        Ok(E::call("for", args, l))
    }

    fn try_expr(&mut self, body: &[X], of: &[Cl], catches: &[Cl], after: &[X], l: u32, b: &mut Bound) -> R<E> {
        let mut bb = b.clone();
        let mut body_es = self.body_list(body, &mut bb)?;
        let mut sections = Vec::new();
        // variables bound in the body are visible in the `of` clauses
        let body_new: Bound = bb.difference(b).cloned().collect();
        if !of.is_empty() && !body_new.is_empty() {
            body_es = vec![self.with_export_tuple(body_es, &body_new, l)];
        }
        sections.push(("do", E::new(K::Block(body_es), l)));
        if !of.is_empty() {
            let mut cls = Vec::new();
            for c in of {
                let mut cb = bb.clone();
                let p = self.pat(&c.pats[0], &bb, &mut cb)?;
                let p = if body_new.is_empty() {
                    p
                } else {
                    let mut items = vec![p];
                    items.extend(body_new.iter().map(|v| var(v, c.line)));
                    E::tuple(items, c.line)
                };
                let g = self.guard(&c.guard, &mut cb.clone())?;
                let be = self.body(&c.body, &mut cb)?;
                cls.push(Clause { args: vec![p], guard: g, body: be, line: c.line });
            }
            sections.push(("else", E::new(K::Clauses(cls), l)));
        }
        if !catches.is_empty() {
            let mut cls = Vec::new();
            for c in catches {
                let mut cb = b.clone();
                let mut args = Vec::new();
                for p in &c.pats[..2] {
                    args.push(self.pat(p, b, &mut cb)?);
                }
                let mut body = Vec::new();
                if let Some(st) = c.pats.get(2) {
                    let before = cb.clone();
                    let sp = self.pat(st, &before, &mut cb)?;
                    body.push(E::bin("=", sp, var("__STACKTRACE__", c.line), c.line));
                }
                let g = self.guard(&c.guard, &mut cb.clone())?;
                body.extend(self.body_list(&c.body, &mut cb)?);
                cls.push(Clause { args, guard: g, body: E::new(K::Block(body), c.line), line: c.line });
            }
            sections.push(("catch", E::new(K::Clauses(cls), l)));
        }
        if !after.is_empty() {
            let ae = self.body(after, &mut b.clone())?;
            sections.push(("after", ae));
        }
        Ok(E::call("try", vec![kw(sections, l)], l))
    }

    fn maybe(&mut self, body: &[X], els: &[Cl], l: u32, b: &mut Bound) -> R<E> {
        let mut bb = b.clone();
        let mut args = Vec::new();
        let n = body.len();
        for (i, e) in body.iter().enumerate() {
            match &e.k {
                XK::MaybeMatch(p, rhs) => {
                    let re = self.expr(rhs, &mut bb)?;
                    let before = bb.clone();
                    let pe = self.pat(p, &before, &mut bb)?;
                    if i + 1 == n {
                        // last: value is the matched value
                        let t = self.fresh("m");
                        args.push(E::bin("<-", E::bin("=", pe, var(&t, e.line), e.line), re, e.line));
                        args.push(kw(vec![("do", var(&t, e.line))], l));
                        return self.maybe_else(args, els, l, b);
                    }
                    args.push(E::bin("<-", pe, re, e.line));
                }
                _ => {
                    if i + 1 == n {
                        let v = self.expr(e, &mut bb)?;
                        args.push(kw(vec![("do", v)], l));
                        return self.maybe_else(args, els, l, b);
                    }
                    args.push(self.expr(e, &mut bb)?);
                }
            }
        }
        unreachable!()
    }

    fn maybe_else(&mut self, mut args: Vec<E>, els: &[Cl], l: u32, b: &Bound) -> R<E> {
        if !els.is_empty() {
            let (cls, _) = self.cr_clauses(els, b, None)?;
            // append else to the keyword list
            if let Some(K::List(items, _)) = args.last_mut().map(|a| &mut a.k) {
                items.push(E::tuple(vec![atom("else", l), E::new(K::Clauses(cls), l)], l));
            }
        }
        Ok(E::call("with", args, l))
    }
}

fn capture(m: &str, f: &str, arity: usize, l: u32) -> E {
    let r = E::new(K::Remote { recv: Box::new(atom(m, l)), name: f.to_string(), args: vec![], parens: false }, l);
    E::new(K::Capture(Box::new(E::bin("/", r, int(arity as i128, l), l))), l)
}

/// Variables newly bound (not in `b`) in every set.
fn intersect_new(sets: &[Bound], b: &Bound) -> Bound {
    let mut it = sets.iter();
    let first = match it.next() {
        Some(f) => f.difference(b).cloned().collect::<Bound>(),
        None => return Bound::new(),
    };
    it.fold(first, |acc, s| acc.intersection(s).cloned().collect())
}

fn const_eval(x: &X) -> Option<i128> {
    match &x.k {
        XK::Int(v) => Some(*v),
        XK::BinOp(op, a, b) => {
            let (a, b) = (const_eval(a)?, const_eval(b)?);
            Some(match op.as_str() {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "bsl" => a << b,
                "bor" => a | b,
                "band" => a & b,
                _ => return None,
            })
        }
        XK::UnOp(op, a) if op == "-" => Some(-const_eval(a)?),
        _ => None,
    }
}

/// Parse + translate an Erlang source file to a `defmodule` E.
pub fn compile_erl(src: &str, file: &str, includes: &dyn Fn(&str) -> Option<String>) -> R<E> {
    let mut pp = crate::erl_pp::Pp::new(file, includes);
    let toks = pp.run(src)?;
    let mut p = crate::erl_parse::Parser::new(toks, file);
    let forms = p.forms()?;
    translate(forms, file)
}
