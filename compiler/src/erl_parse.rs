//! Erlang parser: forms and expressions (after preprocessing).

use crate::erl_lex::{Tok, T};

#[derive(Clone, Debug)]
pub struct X {
    pub k: XK,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum XK {
    Var(String),
    Atom(String),
    Int(i128),
    Float(f64),
    Str(Vec<u32>),
    List(Vec<X>, Option<Box<X>>),
    Tuple(Vec<X>),
    Bin(Vec<BinEl>),
    BinOp(String, Box<X>, Box<X>),
    UnOp(String, Box<X>),
    Match(Box<X>, Box<X>),
    MaybeMatch(Box<X>, Box<X>),
    Call(Box<X>, Vec<X>),
    Remote(Box<X>, Box<X>),
    Case(Box<X>, Vec<Cl>),
    If(Vec<Cl>),
    Receive(Vec<Cl>, Option<(Box<X>, Vec<X>)>),
    Try(Vec<X>, Vec<Cl>, Vec<Cl>, Vec<X>),
    Catch(Box<X>),
    Block(Vec<X>),
    Fun(Option<String>, Vec<Cl>),
    FunRef(Option<Box<X>>, Box<X>, Box<X>),
    LC(Box<X>, Vec<Q>),
    BC(Box<X>, Vec<Q>),
    MC(Box<X>, Box<X>, Vec<Q>),
    Map(Option<Box<X>>, Vec<(X, bool, X)>),
    Rec(Option<Box<X>>, String, Vec<(String, X)>),
    RecField(Box<X>, String, String),
    RecIndex(String, String),
    Maybe(Vec<X>, Vec<Cl>),
}

#[derive(Clone, Debug)]
pub struct Cl {
    pub pats: Vec<X>,
    /// Guard sequence: alternatives (`;`) of conjunctions (`,`).
    pub guard: Vec<Vec<X>>,
    pub body: Vec<X>,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum Q {
    Gen(X, X),
    BGen(X, X),
    MGen(X, X, X),
    Filter(X),
}

#[derive(Clone, Debug)]
pub struct BinEl {
    pub val: X,
    pub size: Option<X>,
    pub tsl: Vec<(String, Option<i128>)>,
}

#[derive(Clone, Debug)]
pub enum Form {
    Module(String),
    Export(Vec<(String, usize)>),
    Record(String, Vec<(String, Option<X>)>),
    Function(String, usize, Vec<Cl>, u32),
    ExportAll,
    NoAutoImport(Vec<(String, usize)>),
    Import(String, Vec<(String, usize)>),
    Other,
}

pub struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    file: String,
}

type R<T> = Result<T, String>;

fn x(k: XK, line: u32) -> X {
    X { k, line }
}

impl Parser {
    pub fn new(toks: Vec<Tok>, file: &str) -> Parser {
        Parser { toks, pos: 0, file: file.to_string() }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }
    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)]
    }
    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        if self.pos < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn is(&self, s: &str) -> bool {
        matches!(&self.peek().t, T::P(p) if p == s)
    }
    fn is_at(&self, n: usize, s: &str) -> bool {
        matches!(&self.peek_at(n).t, T::P(p) if p == s)
    }
    fn err<Y>(&self, m: &str) -> R<Y> {
        let t = self.peek();
        Err(format!("{}:{}: syntax error: {} (near {:?})", self.file, t.line, m, t.t))
    }
    fn expect(&mut self, s: &str) -> R<()> {
        if self.is(s) {
            self.pos += 1;
            Ok(())
        } else {
            self.err(&format!("expected '{}'", s))
        }
    }
    fn line(&self) -> u32 {
        self.peek().line
    }

    // ---------------------------------------------------------------- forms

    pub fn forms(&mut self) -> R<Vec<Form>> {
        let mut out = Vec::new();
        while !matches!(self.peek().t, T::Eof) {
            out.push(self.form()?);
        }
        Ok(out)
    }

    fn skip_form(&mut self) {
        while !matches!(self.peek().t, T::Dot | T::Eof) {
            self.pos += 1;
        }
        if matches!(self.peek().t, T::Dot) {
            self.pos += 1;
        }
    }

    fn form(&mut self) -> R<Form> {
        if self.is("-") {
            let name = match &self.peek_at(1).t {
                T::Atom(a) => a.clone(),
                _ => {
                    self.skip_form();
                    return Ok(Form::Other);
                }
            };
            match name.as_str() {
                "module" => {
                    self.pos += 3;
                    let m = match self.next().t {
                        T::Atom(a) => a,
                        _ => return self.err("bad module"),
                    };
                    self.skip_form();
                    Ok(Form::Module(m))
                }
                "export" => {
                    self.pos += 3;
                    let fs = self.fa_list()?;
                    self.skip_form();
                    Ok(Form::Export(fs))
                }
                "record" => {
                    self.pos += 3;
                    let rname = match self.next().t {
                        T::Atom(a) => a,
                        _ => return self.err("bad record name"),
                    };
                    self.expect(",")?;
                    self.expect("{")?;
                    let mut fields = Vec::new();
                    while !self.is("}") {
                        let f = match self.next().t {
                            T::Atom(a) => a,
                            _ => return self.err("bad record field"),
                        };
                        let mut def = None;
                        if self.is("=") {
                            self.pos += 1;
                            def = Some(self.expr()?);
                        }
                        if self.is("::") {
                            // skip type up to top-level ',' or '}'
                            let mut depth = 0i32;
                            loop {
                                match &self.peek().t {
                                    T::P(p) if (p == "," || p == "}") && depth == 0 => break,
                                    T::P(p) if p == "(" || p == "[" || p == "{" || p == "<<" => depth += 1,
                                    T::P(p) if p == ")" || p == "]" || p == "}" || p == ">>" => depth -= 1,
                                    T::Eof | T::Dot => break,
                                    _ => {}
                                }
                                self.pos += 1;
                            }
                        }
                        fields.push((f, def));
                        if self.is(",") {
                            self.pos += 1;
                        }
                    }
                    self.skip_form();
                    Ok(Form::Record(rname, fields))
                }
                "import" => {
                    let st = self.pos;
                    self.skip_form();
                    let toks = &self.toks[st..self.pos];
                    let m = toks
                        .iter()
                        .filter_map(|t| match &t.t {
                            T::Atom(a) => Some(a.clone()),
                            _ => None,
                        })
                        .find(|a| a != "import");
                    let mut fas = Vec::new();
                    let mut j = 0;
                    while j + 2 < toks.len() {
                        if let (T::Atom(f), T::P(sl), T::Int(n)) = (&toks[j].t, &toks[j + 1].t, &toks[j + 2].t) {
                            if sl == "/" {
                                fas.push((f.clone(), *n as usize));
                            }
                        }
                        j += 1;
                    }
                    match m {
                        Some(m) => Ok(Form::Import(m, fas)),
                        None => Ok(Form::Other),
                    }
                }
                "compile" => {
                    // look for export_all / no_auto_import
                    let st = self.pos;
                    self.skip_form();
                    let toks = &self.toks[st..self.pos];
                    if toks.iter().any(|t| matches!(&t.t, T::Atom(a) if a == "export_all")) {
                        return Ok(Form::ExportAll);
                    }
                    if let Some(i) = toks.iter().position(|t| matches!(&t.t, T::Atom(a) if a == "no_auto_import")) {
                        let mut fas = Vec::new();
                        let mut j = i;
                        while j + 2 < toks.len() {
                            if let (T::Atom(f), T::P(sl), T::Int(n)) = (&toks[j].t, &toks[j + 1].t, &toks[j + 2].t) {
                                if sl == "/" {
                                    fas.push((f.clone(), *n as usize));
                                }
                            }
                            j += 1;
                        }
                        return Ok(Form::NoAutoImport(fas));
                    }
                    Ok(Form::Other)
                }
                _ => {
                    self.skip_form();
                    Ok(Form::Other)
                }
            }
        } else if let T::Atom(name) = self.peek().t.clone() {
            let line = self.line();
            let mut clauses = Vec::new();
            loop {
                match self.next().t {
                    T::Atom(a) if a == name => {}
                    _ => return self.err("expected function clause"),
                }
                let cl = self.fun_clause()?;
                clauses.push(cl);
                if self.is(";") {
                    self.pos += 1;
                    continue;
                }
                break;
            }
            if !matches!(self.peek().t, T::Dot) {
                return self.err("expected '.' after function");
            }
            self.pos += 1;
            let arity = clauses[0].pats.len();
            Ok(Form::Function(name, arity, clauses, line))
        } else {
            self.err("expected form")
        }
    }

    fn fa_list(&mut self) -> R<Vec<(String, usize)>> {
        let mut out = Vec::new();
        self.expect("[")?;
        while !self.is("]") {
            let f = match self.next().t {
                T::Atom(a) => a,
                _ => return self.err("bad export"),
            };
            self.expect("/")?;
            let n = match self.next().t {
                T::Int(n) => n as usize,
                _ => return self.err("bad arity"),
            };
            out.push((f, n));
            if self.is(",") {
                self.pos += 1;
            }
        }
        self.pos += 1;
        Ok(out)
    }

    /// `(Pats) [when Guard] -> Body`
    fn fun_clause(&mut self) -> R<Cl> {
        let line = self.line();
        self.expect("(")?;
        let pats = self.exprs_until(")")?;
        self.expect(")")?;
        let guard = self.opt_guard()?;
        self.expect("->")?;
        let body = self.body()?;
        Ok(Cl { pats, guard, body, line })
    }

    fn exprs_until(&mut self, close: &str) -> R<Vec<X>> {
        let mut v = Vec::new();
        if self.is(close) {
            return Ok(v);
        }
        loop {
            v.push(self.expr()?);
            if self.is(",") {
                self.pos += 1;
                continue;
            }
            break;
        }
        Ok(v)
    }

    fn opt_guard(&mut self) -> R<Vec<Vec<X>>> {
        if !self.is("when") {
            return Ok(vec![]);
        }
        self.pos += 1;
        let mut alts = Vec::new();
        loop {
            let mut conj = vec![self.expr()?];
            while self.is(",") {
                self.pos += 1;
                conj.push(self.expr()?);
            }
            alts.push(conj);
            if self.is(";") {
                self.pos += 1;
                continue;
            }
            break;
        }
        Ok(alts)
    }

    fn body(&mut self) -> R<Vec<X>> {
        let mut v = vec![self.expr()?];
        while self.is(",") {
            self.pos += 1;
            v.push(self.expr()?);
        }
        Ok(v)
    }

    /// `Pat [when Guard] -> Body` clauses separated by ';'
    fn cr_clauses(&mut self) -> R<Vec<Cl>> {
        let mut cls = Vec::new();
        loop {
            let line = self.line();
            let pat = self.expr()?;
            let guard = self.opt_guard()?;
            self.expect("->")?;
            let body = self.body()?;
            cls.push(Cl { pats: vec![pat], guard, body, line });
            if self.is(";") {
                self.pos += 1;
                continue;
            }
            break;
        }
        Ok(cls)
    }

    // ---------------------------------------------------------- expressions

    pub fn expr(&mut self) -> R<X> {
        if self.is("catch") {
            let line = self.line();
            self.pos += 1;
            let e = self.expr()?;
            return Ok(x(XK::Catch(Box::new(e)), line));
        }
        self.expr100()
    }

    fn expr100(&mut self) -> R<X> {
        let l = self.expr150()?;
        let line = self.line();
        if self.is("=") {
            self.pos += 1;
            let r = self.expr100()?;
            return Ok(x(XK::Match(Box::new(l), Box::new(r)), line));
        }
        if self.is("!") {
            self.pos += 1;
            let r = self.expr100()?;
            return Ok(x(XK::BinOp("!".into(), Box::new(l), Box::new(r)), line));
        }
        if self.is("?=") {
            self.pos += 1;
            let r = self.expr100()?;
            return Ok(x(XK::MaybeMatch(Box::new(l), Box::new(r)), line));
        }
        Ok(l)
    }

    fn expr150(&mut self) -> R<X> {
        let l = self.expr160()?;
        if self.is("orelse") {
            let line = self.line();
            self.pos += 1;
            let r = self.expr150()?;
            return Ok(x(XK::BinOp("orelse".into(), Box::new(l), Box::new(r)), line));
        }
        Ok(l)
    }

    fn expr160(&mut self) -> R<X> {
        let l = self.expr200()?;
        if self.is("andalso") {
            let line = self.line();
            self.pos += 1;
            let r = self.expr160()?;
            return Ok(x(XK::BinOp("andalso".into(), Box::new(l), Box::new(r)), line));
        }
        Ok(l)
    }

    fn expr200(&mut self) -> R<X> {
        let l = self.expr300()?;
        for op in ["==", "/=", "=<", "<", ">=", ">", "=:=", "=/="] {
            if self.is(op) {
                let line = self.line();
                self.pos += 1;
                let r = self.expr300()?;
                return Ok(x(XK::BinOp(op.into(), Box::new(l), Box::new(r)), line));
            }
        }
        Ok(l)
    }

    fn expr300(&mut self) -> R<X> {
        let l = self.expr400()?;
        for op in ["++", "--"] {
            if self.is(op) {
                let line = self.line();
                self.pos += 1;
                let r = self.expr300()?;
                return Ok(x(XK::BinOp(op.into(), Box::new(l), Box::new(r)), line));
            }
        }
        Ok(l)
    }

    fn expr400(&mut self) -> R<X> {
        let mut l = self.expr500()?;
        loop {
            let op = match &self.peek().t {
                T::P(p) if matches!(p.as_str(), "+" | "-" | "bor" | "bxor" | "bsl" | "bsr" | "or" | "xor") => p.clone(),
                _ => break,
            };
            let line = self.line();
            self.pos += 1;
            let r = self.expr500()?;
            l = x(XK::BinOp(op, Box::new(l), Box::new(r)), line);
        }
        Ok(l)
    }

    fn expr500(&mut self) -> R<X> {
        let mut l = self.expr600()?;
        loop {
            let op = match &self.peek().t {
                T::P(p) if matches!(p.as_str(), "*" | "/" | "div" | "rem" | "band" | "and") => p.clone(),
                _ => break,
            };
            let line = self.line();
            self.pos += 1;
            let r = self.expr600()?;
            l = x(XK::BinOp(op, Box::new(l), Box::new(r)), line);
        }
        Ok(l)
    }

    fn expr600(&mut self) -> R<X> {
        let op = match &self.peek().t {
            T::P(p) if matches!(p.as_str(), "+" | "-" | "bnot" | "not") => Some(p.clone()),
            _ => None,
        };
        if let Some(op) = op {
            let line = self.line();
            self.pos += 1;
            let e = self.expr600()?;
            // fold negative literals
            if op == "-" {
                match e.k {
                    XK::Int(v) => return Ok(x(XK::Int(-v), line)),
                    XK::Float(v) => return Ok(x(XK::Float(-v), line)),
                    _ => {}
                }
            }
            if op == "+" {
                if matches!(e.k, XK::Int(_) | XK::Float(_)) {
                    return Ok(e);
                }
            }
            return Ok(x(XK::UnOp(op, Box::new(e)), line));
        }
        self.expr700()
    }

    fn expr700(&mut self) -> R<X> {
        let mut e = self.expr800()?;
        loop {
            if self.is("(") {
                let line = self.line();
                self.pos += 1;
                let args = self.exprs_until(")")?;
                self.expect(")")?;
                e = x(XK::Call(Box::new(e), args), line);
                continue;
            }
            if self.is("#") {
                e = self.record_or_map_suffix(Some(e))?;
                continue;
            }
            break;
        }
        Ok(e)
    }

    fn expr800(&mut self) -> R<X> {
        let e = self.exprmax()?;
        if self.is(":") {
            let line = self.line();
            self.pos += 1;
            let f = self.exprmax()?;
            return Ok(x(XK::Remote(Box::new(e), Box::new(f)), line));
        }
        Ok(e)
    }

    /// `#{...}`, `#rec{...}`, `#rec.field` with optional base expression.
    fn record_or_map_suffix(&mut self, base: Option<X>) -> R<X> {
        let line = self.line();
        self.expect("#")?;
        if self.is("{") {
            self.pos += 1;
            let mut pairs = Vec::new();
            while !self.is("}") {
                let k = self.expr()?;
                let exact = if self.is(":=") {
                    true
                } else if self.is("=>") {
                    false
                } else {
                    return self.err("expected => or :=");
                };
                self.pos += 1;
                let v = self.expr()?;
                pairs.push((k, exact, v));
                if self.is(",") {
                    self.pos += 1;
                }
            }
            self.pos += 1;
            // map comprehension: #{K => V || Q}
            return Ok(x(XK::Map(base.map(Box::new), pairs), line));
        }
        let rname = match self.next().t {
            T::Atom(a) => a,
            _ => return self.err("bad record"),
        };
        if self.is(".") {
            self.pos += 1;
            let f = match self.next().t {
                T::Atom(a) => a,
                _ => return self.err("bad record field"),
            };
            return Ok(match base {
                Some(b) => x(XK::RecField(Box::new(b), rname, f), line),
                None => x(XK::RecIndex(rname, f), line),
            });
        }
        self.expect("{")?;
        let mut fields = Vec::new();
        while !self.is("}") {
            let f = match self.next().t {
                T::Atom(a) => a,
                T::Var(v) if v == "_" => "_".into(),
                _ => return self.err("bad record field"),
            };
            self.expect("=")?;
            let v = self.expr()?;
            fields.push((f, v));
            if self.is(",") {
                self.pos += 1;
            }
        }
        self.pos += 1;
        Ok(x(XK::Rec(base.map(Box::new), rname, fields), line))
    }

    fn exprmax(&mut self) -> R<X> {
        let t = self.peek().clone();
        let line = t.line;
        match t.t {
            T::Var(v) => {
                self.pos += 1;
                Ok(x(XK::Var(v), line))
            }
            T::Atom(a) => {
                self.pos += 1;
                Ok(x(XK::Atom(a), line))
            }
            T::Int(n) => {
                self.pos += 1;
                Ok(x(XK::Int(n), line))
            }
            T::Char(c) => {
                self.pos += 1;
                Ok(x(XK::Int(c as i128), line))
            }
            T::Float(f) => {
                self.pos += 1;
                Ok(x(XK::Float(f), line))
            }
            T::Str(s) => {
                self.pos += 1;
                let mut s = s;
                while let T::Str(more) = &self.peek().t {
                    s.extend(more.iter());
                    self.pos += 1;
                }
                Ok(x(XK::Str(s), line))
            }
            T::P(p) => match p.as_str() {
                "(" => {
                    self.pos += 1;
                    let e = self.expr()?;
                    self.expect(")")?;
                    Ok(e)
                }
                "[" => self.list(),
                "{" => {
                    self.pos += 1;
                    let es = self.exprs_until("}")?;
                    self.expect("}")?;
                    Ok(x(XK::Tuple(es), line))
                }
                "<<" => self.binary(),
                "#" => self.record_or_map_suffix(None),
                "begin" => {
                    self.pos += 1;
                    let b = self.body()?;
                    self.expect("end")?;
                    Ok(x(XK::Block(b), line))
                }
                "if" => {
                    self.pos += 1;
                    let mut cls = Vec::new();
                    loop {
                        let l = self.line();
                        let mut alts = Vec::new();
                        loop {
                            let mut conj = vec![self.expr()?];
                            while self.is(",") {
                                self.pos += 1;
                                conj.push(self.expr()?);
                            }
                            alts.push(conj);
                            if self.is(";") {
                                self.pos += 1;
                                continue;
                            }
                            break;
                        }
                        self.expect("->")?;
                        let body = self.body()?;
                        cls.push(Cl { pats: vec![], guard: alts, body, line: l });
                        if self.is(";") {
                            self.pos += 1;
                            continue;
                        }
                        break;
                    }
                    self.expect("end")?;
                    Ok(x(XK::If(cls), line))
                }
                "case" => {
                    self.pos += 1;
                    let e = self.expr()?;
                    self.expect("of")?;
                    let cls = self.cr_clauses()?;
                    self.expect("end")?;
                    Ok(x(XK::Case(Box::new(e), cls), line))
                }
                "receive" => {
                    self.pos += 1;
                    let cls = if self.is("after") { vec![] } else { self.cr_clauses()? };
                    let mut after = None;
                    if self.is("after") {
                        self.pos += 1;
                        let t = self.expr()?;
                        self.expect("->")?;
                        let b = self.body()?;
                        after = Some((Box::new(t), b));
                    }
                    self.expect("end")?;
                    Ok(x(XK::Receive(cls, after), line))
                }
                "try" => {
                    self.pos += 1;
                    let body = self.body()?;
                    let mut of = Vec::new();
                    if self.is("of") {
                        self.pos += 1;
                        of = self.cr_clauses()?;
                    }
                    let mut catches = Vec::new();
                    if self.is("catch") {
                        self.pos += 1;
                        loop {
                            let l = self.line();
                            let pats = self.catch_pattern()?;
                            let guard = self.opt_guard()?;
                            self.expect("->")?;
                            let b = self.body()?;
                            catches.push(Cl { pats, guard, body: b, line: l });
                            if self.is(";") {
                                self.pos += 1;
                                continue;
                            }
                            break;
                        }
                    }
                    let mut after = Vec::new();
                    if self.is("after") {
                        self.pos += 1;
                        after = self.body()?;
                    }
                    self.expect("end")?;
                    Ok(x(XK::Try(body, of, catches, after), line))
                }
                "maybe" => {
                    self.pos += 1;
                    let body = self.body()?;
                    let mut els = Vec::new();
                    if self.is("else") {
                        self.pos += 1;
                        els = self.cr_clauses()?;
                    }
                    self.expect("end")?;
                    Ok(x(XK::Maybe(body, els), line))
                }
                "fun" => self.fun_expr(),
                _ => self.err("unexpected token"),
            },
            _ => self.err("unexpected token"),
        }
    }

    /// `Class:Reason:Stack`, `Class:Reason` or `Reason` -> [class, reason, stack?]
    fn catch_pattern(&mut self) -> R<Vec<X>> {
        let line = self.line();
        // Parse the first part without remote-call interpretation.
        let a = self.expr150_pat()?;
        if self.is(":") {
            self.pos += 1;
            let b = self.expr150_pat()?;
            if self.is(":") {
                self.pos += 1;
                let c = self.expr150_pat()?;
                return Ok(vec![a, b, c]);
            }
            return Ok(vec![a, b]);
        }
        Ok(vec![x(XK::Atom("throw".into()), line), a])
    }

    /// A pattern in a catch clause: like expr100 but `:` is not a remote call.
    fn expr150_pat(&mut self) -> R<X> {
        let l = self.exprmax_chain()?;
        if self.is("=") {
            let line = self.line();
            self.pos += 1;
            let r = self.expr150_pat()?;
            return Ok(x(XK::Match(Box::new(l), Box::new(r)), line));
        }
        Ok(l)
    }

    fn exprmax_chain(&mut self) -> R<X> {
        let mut e = self.exprmax()?;
        while self.is("#") {
            e = self.record_or_map_suffix(Some(e))?;
        }
        Ok(e)
    }

    fn fun_expr(&mut self) -> R<X> {
        let line = self.line();
        self.expect("fun")?;
        // fun Name/Arity, fun M:F/A
        match (&self.peek().t, &self.peek_at(1).t) {
            (T::Atom(_), T::P(p)) | (T::Var(_), T::P(p)) if p == "/" || p == ":" => {
                let a = self.exprmax()?;
                if self.is(":") {
                    self.pos += 1;
                    let f = self.exprmax()?;
                    self.expect("/")?;
                    let n = self.exprmax()?;
                    return Ok(x(XK::FunRef(Some(Box::new(a)), Box::new(f), Box::new(n)), line));
                }
                self.expect("/")?;
                let n = self.exprmax()?;
                return Ok(x(XK::FunRef(None, Box::new(a), Box::new(n)), line));
            }
            _ => {}
        }
        // named fun
        let mut name = None;
        if let T::Var(v) = &self.peek().t {
            if self.is_at(1, "(") {
                name = Some(v.clone());
            }
        }
        let mut cls = Vec::new();
        loop {
            if name.is_some() {
                self.pos += 1; // the name
            }
            cls.push(self.fun_clause()?);
            if self.is(";") {
                self.pos += 1;
                continue;
            }
            break;
        }
        self.expect("end")?;
        Ok(x(XK::Fun(name, cls), line))
    }

    fn list(&mut self) -> R<X> {
        let line = self.line();
        self.expect("[")?;
        if self.is("]") {
            self.pos += 1;
            return Ok(x(XK::List(vec![], None), line));
        }
        let first = self.expr()?;
        if self.is("||") {
            self.pos += 1;
            let qs = self.qualifiers()?;
            self.expect("]")?;
            return Ok(x(XK::LC(Box::new(first), qs), line));
        }
        let mut items = vec![first];
        let mut tail = None;
        loop {
            if self.is(",") {
                self.pos += 1;
                items.push(self.expr()?);
                continue;
            }
            if self.is("|") {
                self.pos += 1;
                tail = Some(Box::new(self.expr()?));
            }
            break;
        }
        self.expect("]")?;
        Ok(x(XK::List(items, tail), line))
    }

    fn qualifiers(&mut self) -> R<Vec<Q>> {
        let mut qs = Vec::new();
        loop {
            let e = self.expr()?;
            if self.is("<-") || self.is("<=") || self.is("<:-") || self.is("<:=") {
                let bin = self.is("<=") || self.is("<:=");
                self.pos += 1;
                let src = self.expr()?;
                // map generator: K := V <- M
                if let XK::BinOp(op, k, v) = &e.k {
                    if op == ":=" {
                        qs.push(Q::MGen((**k).clone(), (**v).clone(), src));
                        if self.is(",") {
                            self.pos += 1;
                            continue;
                        }
                        break;
                    }
                }
                qs.push(if bin { Q::BGen(e, src) } else { Q::Gen(e, src) });
            } else if self.is(":=") {
                // map generator `K := V <- Map`
                self.pos += 1;
                let v = self.expr()?;
                self.expect("<-")?;
                let src = self.expr()?;
                qs.push(Q::MGen(e, v, src));
            } else {
                qs.push(Q::Filter(e));
            }
            if self.is(",") {
                self.pos += 1;
                continue;
            }
            break;
        }
        Ok(qs)
    }

    fn binary(&mut self) -> R<X> {
        let line = self.line();
        self.expect("<<")?;
        if self.is(">>") {
            self.pos += 1;
            return Ok(x(XK::Bin(vec![]), line));
        }
        let first = self.bin_el()?;
        if self.is("||") {
            self.pos += 1;
            let qs = self.qualifiers()?;
            self.expect(">>")?;
            let v = if first.size.is_none() && first.tsl.is_empty() {
                first.val
            } else {
                x(XK::Bin(vec![first]), line)
            };
            return Ok(x(XK::BC(Box::new(v), qs), line));
        }
        let mut els = vec![first];
        while self.is(",") {
            self.pos += 1;
            els.push(self.bin_el()?);
        }
        self.expect(">>")?;
        Ok(x(XK::Bin(els), line))
    }

    fn bin_el(&mut self) -> R<BinEl> {
        let line = self.line();
        let val = match &self.peek().t {
            T::P(p) if matches!(p.as_str(), "-" | "+" | "bnot" | "not") => {
                let op = p.clone();
                self.pos += 1;
                let e = self.exprmax()?;
                match (op.as_str(), &e.k) {
                    ("-", XK::Int(v)) => x(XK::Int(-v), line),
                    ("-", XK::Float(v)) => x(XK::Float(-v), line),
                    _ => x(XK::UnOp(op, Box::new(e)), line),
                }
            }
            _ => self.exprmax()?,
        };
        let mut size = None;
        if self.is(":") {
            self.pos += 1;
            size = Some(self.exprmax()?);
        }
        let mut tsl = Vec::new();
        if self.is("/") {
            self.pos += 1;
            loop {
                let t = match self.next().t {
                    T::Atom(a) => a,
                    _ => return self.err("bad binary type"),
                };
                let mut n = None;
                if self.is(":") {
                    self.pos += 1;
                    if let T::Int(v) = self.next().t {
                        n = Some(v);
                    }
                }
                tsl.push((t, n));
                if self.is("-") {
                    self.pos += 1;
                    continue;
                }
                break;
            }
        }
        Ok(BinEl { val, size, tsl })
    }
}
