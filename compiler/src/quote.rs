//! `quote`: builds the Elixir AST of an expression at runtime, with
//! `unquote`/`unquote_splicing` evaluated in the surrounding scope.

use crate::ast::*;
use crate::collect::R;
use crate::core::*;
use crate::expand::{Cx, Expander};
use std::collections::HashMap;

/// Kernel's exported functions and macros: name -> arities.
pub fn kernel_exports() -> &'static HashMap<String, Vec<usize>> {
    static T: std::sync::OnceLock<HashMap<String, Vec<usize>>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut m: HashMap<String, Vec<usize>> = HashMap::new();
        for l in include_str!("../data/kernel_exports.txt").lines() {
            if let Some((n, a)) = l.rsplit_once(' ') {
                if let Ok(a) = a.parse() {
                    m.entry(n.to_string()).or_default().push(a);
                }
            }
        }
        for v in m.values_mut() {
            v.sort();
            v.dedup();
        }
        m
    })
}

const SPECIAL: &[&str] = &[
    "=",
    "|",
    "::",
    "when",
    "<-",
    "\\\\",
    "->",
    ".",
    "^",
    "%{}",
    "{}",
    "<<>>",
    "%",
    "&",
    "__block__",
    "__aliases__",
    "case",
    "cond",
    "fn",
    "receive",
    "try",
    "for",
    "with",
    "quote",
    "unquote",
    "unquote_splicing",
    "import",
    "alias",
    "require",
    "super",
    "__MODULE__",
    "__CALLER__",
    "__ENV__",
    "__DIR__",
    "__STACKTRACE__",
];

fn list(items: Vec<CE>) -> CE {
    let mut l = CE::Lit(Lit::Nil);
    for it in items.into_iter().rev() {
        l = CE::Cons(Box::new(it), Box::new(l));
    }
    l
}

fn kw(pairs: Vec<(&str, CE)>) -> CE {
    list(
        pairs
            .into_iter()
            .map(|(k, v)| CE::Tuple(vec![CE::atom(k), v]))
            .collect(),
    )
}

fn triple(name: CE, meta: CE, args: CE) -> CE {
    CE::Tuple(vec![name, meta, args])
}

/// One element of an argument list: a plain item or a spliced list.
enum Piece {
    One(CE),
    Splice(CE),
}

impl Expander {
    pub fn quote(&mut self, body: &E, opts: Option<&E>, cx: &mut Cx) -> R<CE> {
        let ctx = match cx.module() {
            "" | "Elixir.Tonic.Script" => "Elixir".to_string(),
            m => m.to_string(),
        };
        let mut q = Quoter {
            ctx,
            unquote: true,
            prune: false,
        };
        if let Some(o) = opts {
            if let Some(u) = o.kw_get("prune_metadata") {
                if matches!(u.k, K::Atom(ref a) if a == "true") {
                    q.prune = true;
                }
            }
            if let Some(u) = o.kw_get("unquote") {
                if matches!(u.k, K::Atom(ref a) if a == "false") {
                    q.unquote = false;
                }
            }
            if let Some(bq) = o.kw_get("bind_quoted") {
                q.unquote = false;
                let mut exprs = Vec::new();
                if let K::List(items, _) = &bq.k {
                    for it in items {
                        if let K::Tuple(kv) = &it.k {
                            if let K::Atom(name) = &kv[0].k {
                                let v = self.expr(&kv[1], cx)?;
                                exprs.push(triple(
                                    CE::atom("="),
                                    CE::Lit(Lit::Nil),
                                    list(vec![
                                        triple(CE::atom(name), CE::Lit(Lit::Nil), CE::atom(&q.ctx)),
                                        v,
                                    ]),
                                ));
                            }
                        }
                    }
                }
                let b = q.q(&mut ExUnq { ex: self, cx }, body)?;
                exprs.push(b);
                return Ok(triple(
                    CE::atom("__block__"),
                    CE::Lit(Lit::Nil),
                    list(exprs),
                ));
            }
        }
        q.q(&mut ExUnq { ex: self, cx }, body)
    }
}

impl Expander {
    /// Quoted form of a macro call argument (caller code: nil context).
    pub fn quote_macro_arg(&mut self, e: &E, _cx: &mut Cx) -> R<CE> {
        quote_literal(e)
    }
}

/// Evaluates `unquote` fragments while quoting.
pub trait Unq {
    fn unquote(&mut self, e: &E) -> R<CE>;
}

struct ExUnq<'a> {
    ex: &'a mut Expander,
    cx: &'a mut Cx,
}

impl Unq for ExUnq<'_> {
    fn unquote(&mut self, e: &E) -> R<CE> {
        self.ex.expr(e, self.cx)
    }
}

struct NoUnq;

impl Unq for NoUnq {
    fn unquote(&mut self, _e: &E) -> R<CE> {
        Err("unquote called outside quote".into())
    }
}

/// Quoted form (no metadata, nil variable context) of source code, as a
/// literal term: macro call arguments.
pub fn quote_literal(e: &E) -> R<CE> {
    let mut q = Quoter {
        ctx: "nil".into(),
        unquote: false,
        prune: true,
    };
    q.q(&mut NoUnq, e)
}

struct Quoter {
    ctx: String,
    unquote: bool,
    /// `prune_metadata: true`: no metadata at all.
    prune: bool,
}

impl Quoter {
    fn meta_for(&self, name: &str, arity: usize) -> CE {
        if self.prune || SPECIAL.contains(&name) {
            return CE::Lit(Lit::Nil);
        }
        match kernel_exports().get(name) {
            Some(ars) if ars.contains(&arity) => {
                let imports = list(
                    ars.iter()
                        .map(|a| CE::Tuple(vec![CE::int(*a as i64), CE::atom("Elixir.Kernel")]))
                        .collect(),
                );
                kw(vec![("context", CE::atom(&self.ctx)), ("imports", imports)])
            }
            _ => CE::Lit(Lit::Nil),
        }
    }

    fn call(&self, name: &str, args: Vec<CE>) -> CE {
        let meta = self.meta_for(name, args.len());
        triple(CE::atom(name), meta, list(args))
    }

    fn pieces_to_list(&self, pieces: Vec<Piece>) -> CE {
        // Build from the end so splices append.
        let mut acc = CE::Lit(Lit::Nil);
        for p in pieces.into_iter().rev() {
            acc = match p {
                Piece::One(x) => CE::Cons(Box::new(x), Box::new(acc)),
                Piece::Splice(l) => CE::bif("tn_append", vec![l, acc]),
            };
        }
        acc
    }

    fn args(&mut self, u: &mut dyn Unq, args: &[E]) -> R<Vec<Piece>> {
        let mut out = Vec::new();
        for a in args {
            if self.unquote {
                if let K::Call { name, args: ua, .. } = &a.k {
                    if name == "unquote_splicing" && ua.len() == 1 {
                        out.push(Piece::Splice(u.unquote(&ua[0])?));
                        continue;
                    }
                }
            }
            out.push(Piece::One(self.q(u, a)?));
        }
        Ok(out)
    }

    fn arg_list(&mut self, u: &mut dyn Unq, args: &[E]) -> R<CE> {
        let p = self.args(u, args)?;
        Ok(self.pieces_to_list(p))
    }

    fn map_pairs(&mut self, u: &mut dyn Unq, pairs: &[(E, E)]) -> R<CE> {
        let entries: Vec<E> = pairs.iter().map(|(key, value)| {
            if matches!(&value.k, K::ParenArgs(items) if items.is_empty()) && matches!(&key.k, K::Call { name, args, .. } if name == "unquote_splicing" && args.len() == 1) {
                key.clone()
            } else {
                E::tuple(vec![key.clone(), value.clone()], key.line)
            }
        }).collect();
        self.arg_list(u, &entries)
    }

    fn aliases(&self, a: &str) -> CE {
        let segs: Vec<CE> = a.split('.').map(CE::atom).collect();
        let meta = if self.prune {
            CE::Lit(Lit::Nil)
        } else {
            kw(vec![("alias", CE::atom("false"))])
        };
        triple(CE::atom("__aliases__"), meta, list(segs))
    }

    fn clause(&mut self, u: &mut dyn Unq, c: &crate::ast::Clause) -> R<CE> {
        let mut heads = self.args(u, &c.args)?;
        let head = match &c.guard {
            Some(g) => {
                heads.push(Piece::One(self.q(u, g)?));
                let l = self.pieces_to_list(heads);
                list(vec![triple(CE::atom("when"), CE::Lit(Lit::Nil), l)])
            }
            None => self.pieces_to_list(heads),
        };
        let body = self.q(u, &c.body)?;
        Ok(triple(
            CE::atom("->"),
            CE::Lit(Lit::Nil),
            list(vec![head, body]),
        ))
    }

    fn interp(&mut self, u: &mut dyn Unq, parts: &[IPart]) -> R<Vec<CE>> {
        let mut segs = Vec::new();
        for p in parts {
            match p {
                IPart::Lit(b) => segs.push(CE::Lit(Lit::Bin(b.clone()))),
                IPart::Expr(x) => {
                    let v = self.q(u, x)?;
                    let dot = triple(
                        CE::atom("."),
                        CE::Lit(Lit::Nil),
                        list(vec![CE::atom("Elixir.Kernel"), CE::atom("to_string")]),
                    );
                    let conv = triple(
                        dot,
                        kw(vec![("from_interpolation", CE::atom("true"))]),
                        list(vec![v]),
                    );
                    let ty = triple(CE::atom("binary"), CE::Lit(Lit::Nil), CE::atom(&self.ctx));
                    segs.push(triple(
                        CE::atom("::"),
                        CE::Lit(Lit::Nil),
                        list(vec![conv, ty]),
                    ));
                }
            }
        }
        Ok(segs)
    }

    fn q(&mut self, u: &mut dyn Unq, e: &E) -> R<CE> {
        let nil = || CE::Lit(Lit::Nil);
        Ok(match &e.k {
            K::Int(s) => match s.parse::<i64>() {
                Ok(i) => CE::int(i),
                Err(_) => CE::Lit(Lit::Big(s.clone())),
            },
            K::Float(f) => CE::Lit(Lit::Float(*f)),
            K::Atom(a) => CE::atom(a),
            K::Str(b) => CE::Lit(Lit::Bin(b.clone())),
            K::Interp(parts) => {
                let segs = self.interp(u, parts)?;
                triple(CE::atom("<<>>"), nil(), list(segs))
            }
            K::Charlist(parts) => {
                if parts.iter().all(|p| matches!(p, IPart::Lit(_))) {
                    let mut bytes = Vec::new();
                    for p in parts {
                        if let IPart::Lit(b) = p {
                            bytes.extend_from_slice(b);
                        }
                    }
                    let s = String::from_utf8_lossy(&bytes).into_owned();
                    list(s.chars().map(|c| CE::int(c as i64)).collect())
                } else {
                    let segs = self.interp(u, parts)?;
                    let bin = triple(CE::atom("<<>>"), nil(), list(segs));
                    let dot = triple(
                        CE::atom("."),
                        nil(),
                        list(vec![CE::atom("Elixir.List"), CE::atom("to_charlist")]),
                    );
                    triple(dot, nil(), list(vec![bin]))
                }
            }
            K::AtomInterp(parts) => {
                let segs = self.interp(u, parts)?;
                let bin = triple(CE::atom("<<>>"), nil(), list(segs));
                let dot = triple(
                    CE::atom("."),
                    nil(),
                    list(vec![CE::atom("erlang"), CE::atom("binary_to_atom")]),
                );
                triple(dot, nil(), list(vec![bin, CE::atom("utf8")]))
            }
            K::Var(v) => triple(CE::atom(v), nil(), CE::atom(&self.ctx)),
            K::Alias(a) => self.aliases(a),
            K::Call { name, args, parens } => {
                if self.unquote && name == "unquote" && args.len() == 1 {
                    return u.unquote(&args[0]);
                }
                if name == "__block__" {
                    let a = self.arg_list(u, args)?;
                    return Ok(triple(CE::atom("__block__"), nil(), a));
                }
                let a = self.arg_list(u, args)?;
                let n = args.len();
                let meta = if !parens
                    && n == 0
                    && !SPECIAL.contains(&name.as_str())
                    && kernel_exports()
                        .get(name.as_str())
                        .map(|v| !v.contains(&0))
                        .unwrap_or(true)
                {
                    self.meta_for(name, n)
                } else {
                    self.meta_for(name, n)
                };
                triple(CE::atom(name), meta, a)
            }
            K::Remote {
                recv,
                name,
                args,
                parens,
            } => {
                let r = match &recv.k {
                    K::Atom(m) => CE::atom(m),
                    _ => self.q(u, recv)?,
                };
                let dot = triple(CE::atom("."), nil(), list(vec![r, CE::atom(name)]));
                let meta = if !parens && args.is_empty() {
                    kw(vec![("no_parens", CE::atom("true"))])
                } else {
                    nil()
                };
                let a = self.arg_list(u, args)?;
                triple(dot, meta, a)
            }
            K::AnonCall { f, args } => {
                let fq = self.q(u, f)?;
                let dot = triple(CE::atom("."), nil(), list(vec![fq]));
                let a = self.arg_list(u, args)?;
                triple(dot, nil(), a)
            }
            K::Access { e: x, key } => {
                let dot = triple(
                    CE::atom("."),
                    nil(),
                    list(vec![CE::atom("Elixir.Access"), CE::atom("get")]),
                );
                let a = vec![self.q(u, x)?, self.q(u, key)?];
                triple(dot, nil(), list(a))
            }
            K::Bin { op, l, r } if op == ".." => {
                // a..b//c
                if let K::Bin {
                    op: o2,
                    l: rl,
                    r: rr,
                } = &r.k
                {
                    if o2 == "//" {
                        let a = vec![self.q(u, l)?, self.q(u, rl)?, self.q(u, rr)?];
                        return Ok(self.call("..//", a));
                    }
                }
                let a = vec![self.q(u, l)?, self.q(u, r)?];
                self.call("..", a)
            }
            K::Bin { op, l, r } if op == "not in" => {
                let a = vec![self.q(u, l)?, self.q(u, r)?];
                let inner = self.call("in", a);
                self.call("not", vec![inner])
            }
            K::Bin { op, l, r } => {
                let a = vec![self.q(u, l)?, self.q(u, r)?];
                self.call(op, a)
            }
            K::Un { op, e: x } => {
                if op == "-" {
                    if let K::Int(s) = &x.k {
                        if let Ok(i) = s.parse::<i64>() {
                            return Ok(CE::int(-i));
                        }
                    }
                    if let K::Float(f) = &x.k {
                        return Ok(CE::Lit(Lit::Float(-f)));
                    }
                }
                if op == "@" {
                    if let K::Call { name, args, .. } = &x.k {
                        let a = self.arg_list(u, args)?;
                        let inner = triple(
                            CE::atom(name),
                            kw(vec![("context", CE::atom(&self.ctx))]),
                            if args.is_empty() {
                                CE::atom(&self.ctx)
                            } else {
                                a
                            },
                        );
                        return Ok(self.call("@", vec![inner]));
                    }
                }
                let a = vec![self.q(u, x)?];
                self.call(op, a)
            }
            K::Tuple(items) if items.len() == 2 => {
                let a = self.q(u, &items[0])?;
                let b = self.q(u, &items[1])?;
                CE::Tuple(vec![a, b])
            }
            K::Tuple(items) => {
                let a = self.arg_list(u, items)?;
                triple(CE::atom("{}"), nil(), a)
            }
            K::List(items, tail) => {
                let mut pieces = self.args(u, items)?;
                if let Some(t) = tail {
                    // [a, b | t] is [a, {:|, [], [b, t]}]
                    let last = match pieces.pop() {
                        Some(Piece::One(x)) => x,
                        _ => CE::Lit(Lit::Nil),
                    };
                    let tq = self.q(u, t)?;
                    pieces.push(Piece::One(triple(
                        CE::atom("|"),
                        nil(),
                        list(vec![last, tq]),
                    )));
                }
                self.pieces_to_list(pieces)
            }
            K::Map(pairs) => {
                let ps = self.map_pairs(u, pairs)?;
                triple(CE::atom("%{}"), nil(), ps)
            }
            K::MapUpd(base, pairs) => {
                let ps = self.map_pairs(u, pairs)?;
                let b = self.q(u, base)?;
                let bar = triple(CE::atom("|"), nil(), list(vec![b, ps]));
                triple(CE::atom("%{}"), nil(), list(vec![bar]))
            }
            K::Struct { name, base, pairs } => {
                let ps = self.map_pairs(u, pairs)?;
                let inner = match base {
                    Some(b) => {
                        let bq = self.q(u, b)?;
                        list(vec![triple(CE::atom("|"), nil(), list(vec![bq, ps]))])
                    }
                    None => ps,
                };
                let n = self.q(u, name)?;
                triple(
                    CE::atom("%"),
                    nil(),
                    list(vec![n, triple(CE::atom("%{}"), nil(), inner)]),
                )
            }
            K::Bits(segs) => {
                let a = self.arg_list(u, segs)?;
                triple(CE::atom("<<>>"), nil(), a)
            }
            K::Fn(clauses) => {
                let mut cs = Vec::new();
                for c in clauses {
                    cs.push(self.clause(u, c)?);
                }
                triple(CE::atom("fn"), nil(), list(cs))
            }
            K::Block(es) if es.len() == 1 => self.q(u, &es[0])?,
            K::Block(es) => {
                let a = self.arg_list(u, es)?;
                triple(CE::atom("__block__"), nil(), a)
            }
            K::Clauses(cs) => {
                let mut out = Vec::new();
                for c in cs {
                    out.push(self.clause(u, c)?);
                }
                list(out)
            }
            K::Capture(x) => {
                let a = vec![self.q(u, x)?];
                triple(CE::atom("&"), nil(), list(a))
            }
            K::CapArg(n) => triple(CE::atom("&"), nil(), list(vec![CE::int(*n as i64)])),
            K::Attr(n) => {
                let inner = triple(
                    CE::atom(n),
                    kw(vec![("context", CE::atom(&self.ctx))]),
                    CE::atom(&self.ctx),
                );
                self.call("@", vec![inner])
            }
            K::Sigil { ch, parts, mods } => {
                let parts = if matches!(ch, 'c' | 's' | 'w') {
                    parts.iter().map(|part| match part {
                        IPart::Lit(bytes) => {
                            let mut escaped = Vec::new();
                            for byte in bytes {
                                match byte {
                                    b'\\' => escaped.extend_from_slice(b"\\\\"),
                                    b'\n' => escaped.extend_from_slice(b"\\n"),
                                    b'\r' => escaped.extend_from_slice(b"\\r"),
                                    b'\t' => escaped.extend_from_slice(b"\\t"),
                                    8 => escaped.extend_from_slice(b"\\b"),
                                    12 => escaped.extend_from_slice(b"\\f"),
                                    _ => escaped.push(*byte),
                                }
                            }
                            IPart::Lit(escaped)
                        }
                        other => other.clone(),
                    }).collect::<Vec<_>>()
                } else { parts.clone() };
                let segs = self.interp(u, &parts)?;
                let bin = triple(CE::atom("<<>>"), nil(), list(segs));
                let m = list(mods.chars().map(|c| CE::int(c as i64)).collect());
                let name = format!("sigil_{}", ch);
                let mut meta_items = vec![("delimiter", CE::Lit(Lit::Bin(b"/".to_vec())))];
                if let Some(ars) = kernel_exports().get(&name) {
                    if ars.contains(&2) {
                        meta_items.push(("context", CE::atom(&self.ctx)));
                        meta_items.push((
                            "imports",
                            list(vec![CE::Tuple(vec![CE::int(2), CE::atom("Elixir.Kernel")])]),
                        ));
                    }
                }
                triple(CE::atom(&name), kw(meta_items), list(vec![bin, m]))
            }
            K::ParenArgs(items) => {
                let a = self.arg_list(u, items)?;
                triple(CE::atom("__block__"), nil(), a)
            }
            K::MultiAlias(base, items) => {
                let b = self.q(u, base)?;
                let dot = triple(CE::atom("."), nil(), list(vec![b, CE::atom("{}")]));
                let a = self.arg_list(u, items)?;
                triple(dot, nil(), a)
            }
        })
    }
}
