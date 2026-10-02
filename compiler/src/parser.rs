//! Elixir parser (Pratt expression parser plus Elixir's call/do-block rules).

use crate::ast::*;
use crate::lexer::{tokenize, Part, Tok, T};

pub type PResult<T> = Result<T, String>;

pub struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    file: String,
    nl_ign: Vec<bool>,
    paren_stab: bool,
}

fn infix_bp(op: &str) -> Option<(u8, u8)> {
    Some(match op {
        "<-" | "\\\\" => (10, 11),
        "when" => (20, 19),
        "::" => (30, 29),
        "|" => (40, 39),
        "=>" => (50, 49),
        "=" => (70, 69),
        "||" | "|||" | "or" => (80, 81),
        "&&" | "&&&" | "and" => (90, 91),
        "==" | "!=" | "=~" | "===" | "!==" => (100, 101),
        "<" | ">" | "<=" | ">=" => (110, 111),
        "|>" | "<<<" | ">>>" | "<<~" | "~>>" | "<~" | "~>" | "<~>" | "^^^" => (120, 121),
        "in" | "not in" => (130, 131),
        "++" | "--" | "+++" | "---" | ".." | "<>" => (140, 139),
        "//" => (140, 139),
        "+" | "-" => (150, 151),
        "*" | "/" => (160, 161),
        "**" => (170, 171),
        _ => return None,
    })
}

const UNARY_BP: u8 = 180;
const CAPTURE_BP: u8 = 61;

/// Operators that continue an expression when they start the next line.
fn continues_line(t: &T) -> bool {
    matches!(
        t,
        T::Op("|>")
            | T::Op("when")
            | T::Op("and")
            | T::Op("or")
            | T::Op("&&")
            | T::Op("||")
            | T::Op("++")
            | T::Op("<>")
            | T::Op("..")
            | T::Op("//")
            | T::Op("|")
            | T::Op("::")
            | T::Op("=>")
    )
}

impl Parser {
    pub fn new(toks: Vec<Tok>, file: &str) -> Parser {
        Parser {
            toks,
            pos: 0,
            file: file.to_string(),
            nl_ign: vec![false],
            paren_stab: false,
        }
    }

    fn err<X>(&self, msg: &str) -> PResult<X> {
        let t = &self.toks[self.pos.min(self.toks.len() - 1)];
        Err(format!(
            "{}:{}: syntax error: {} (near {})",
            self.file,
            t.line,
            msg,
            describe(&t.t)
        ))
    }

    fn ignoring(&self) -> bool {
        *self.nl_ign.last().unwrap()
    }

    fn peek(&mut self) -> &Tok {
        if self.ignoring() {
            while matches!(self.toks[self.pos].t, T::Nl) {
                self.pos += 1;
            }
        }
        &self.toks[self.pos]
    }

    fn peek_raw(&self) -> &Tok {
        &self.toks[self.pos]
    }

    fn peek_t(&mut self) -> T {
        self.peek().t.clone()
    }

    fn next(&mut self) -> Tok {
        self.peek();
        let t = self.toks[self.pos].clone();
        if !matches!(t.t, T::Eof) {
            self.pos += 1;
        }
        t
    }

    fn at_op(&mut self, op: &str) -> bool {
        matches!(&self.peek().t, T::Op(o) if *o == op)
    }

    fn expect_op(&mut self, op: &str) -> PResult<()> {
        if self.at_op(op) {
            self.next();
            Ok(())
        } else {
            self.err(&format!("expected '{}'", op))
        }
    }

    fn skip_nl(&mut self) {
        while matches!(self.toks[self.pos].t, T::Nl) {
            self.pos += 1;
        }
    }

    fn skip_nl_semi(&mut self) {
        while matches!(self.toks[self.pos].t, T::Nl | T::Op(";")) {
            self.pos += 1;
        }
    }

    fn line(&mut self) -> u32 {
        self.peek().line
    }

    // ------------------------------------------------------------------
    // Top level
    // ------------------------------------------------------------------

    pub fn parse_file(&mut self) -> PResult<Vec<E>> {
        let mut out = Vec::new();
        loop {
            self.skip_nl_semi();
            if matches!(self.peek_raw().t, T::Eof) {
                break;
            }
            let e = self.parse_expr(0, false)?;
            out.push(e);
            match &self.peek_raw().t {
                T::Nl | T::Op(";") | T::Eof => {}
                _ => return self.err("unexpected token after expression"),
            }
        }
        Ok(out)
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    pub fn parse_expr(&mut self, min_bp: u8, nd: bool) -> PResult<E> {
        let mut lhs = self.parse_prefix(nd)?;
        loop {
            // Continuation across newlines for leading pipe-like operators.
            if matches!(self.toks[self.pos].t, T::Nl) && !self.ignoring() {
                let mut k = self.pos;
                while matches!(self.toks[k].t, T::Nl) {
                    k += 1;
                }
                if continues_line(&self.toks[k].t) {
                    self.pos = k;
                } else {
                    break;
                }
            }
            let t = self.peek().clone();
            let (op, two): (&str, bool) = match &t.t {
                T::Op("not") => {
                    // `not in`
                    let mut k = self.pos + 1;
                    while matches!(self.toks[k].t, T::Nl) && self.ignoring() {
                        k += 1;
                    }
                    if matches!(self.toks[k].t, T::Op("in")) {
                        ("not in", true)
                    } else {
                        break;
                    }
                }
                T::Op(o) => (*o, false),
                _ => break,
            };
            let (lbp, rbp) = match infix_bp(op) {
                Some(b) => b,
                None => break,
            };
            if lbp < min_bp {
                break;
            }
            self.next();
            if two {
                self.next();
            }
            self.skip_nl();
            let line = t.line;
            let rhs = self.parse_expr(rbp, nd)?;
            lhs = E::bin(op, lhs, rhs, line);
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self, nd: bool) -> PResult<E> {
        let t = self.next();
        let line = t.line;
        let e = match t.t {
            T::Int(s) => E::new(K::Int(s), line),
            T::Float(f) => E::new(K::Float(f), line),
            T::Atom(a) => E::new(K::Atom(a), line),
            T::AtomInterp(parts) => E::new(K::AtomInterp(self.interp_parts(parts)?), line),
            T::Str(parts) => {
                let ps = self.interp_parts(parts)?;
                str_or_interp(ps, line)
            }
            T::Charlist(parts) => E::new(K::Charlist(self.interp_parts(parts)?), line),
            T::Sigil { ch, parts, mods } => {
                let ps = self.interp_parts(parts)?;
                E::new(
                    K::Sigil {
                        ch,
                        parts: ps,
                        mods,
                    },
                    line,
                )
            }
            T::CapArg(n) => E::new(K::CapArg(n), line),
            T::Ident(name) => self.ident_expr(name, line, nd)?,
            T::Alias(name) => E::new(K::Alias(name), line),
            T::Fn => self.parse_fn(line)?,
            T::Op("(") => self.parse_paren(line)?,
            T::Op("[") => self.parse_list(line)?,
            T::Op("{") => self.parse_tuple(line)?,
            T::Op("%{") => self.parse_map_body(line, None)?,
            T::Op("%") => self.parse_struct(line)?,
            T::Op("<<") => self.parse_bits(line)?,
            T::Op("&") => {
                self.skip_nl();
                // &+/2, &>=/2 ... operator captures
                if let T::Op(op) = self.peek_raw().t.clone() {
                    if (infix_bp(op).is_some() || matches!(op, "!" | "^" | "~~~" | "@"))
                        && matches!(self.toks.get(self.pos + 1).map(|t| &t.t), Some(T::Op("/")))
                    {
                        if let Some(T::Int(n)) = self.toks.get(self.pos + 2).map(|t| t.t.clone()) {
                            self.pos += 3;
                            let inner =
                                E::bin("/", E::var(op, line), E::new(K::Int(n), line), line);
                            return Ok(E::new(K::Capture(Box::new(inner)), line));
                        }
                    }
                }
                let e = self.parse_expr(CAPTURE_BP, nd)?;
                E::new(K::Capture(Box::new(e)), line)
            }
            T::Op("@") => {
                let t2 = self.next();
                match t2.t {
                    T::Ident(name) => {
                        let e = self.ident_expr(name.clone(), t2.line, nd)?;
                        match e.k {
                            K::Var(n) => E::new(K::Attr(n), line),
                            _ => E::new(
                                K::Un {
                                    op: "@".into(),
                                    e: Box::new(e),
                                },
                                line,
                            ),
                        }
                    }
                    T::Alias(a) => E::new(
                        K::Un {
                            op: "@".into(),
                            e: Box::new(E::new(K::Alias(a), t2.line)),
                        },
                        line,
                    ),
                    _ => return self.err("expected attribute name after @"),
                }
            }
            T::Op(op @ ("-" | "+")) => {
                let nt = self.peek_raw().clone();
                if !nt.sp {
                    if let T::Int(s) = &nt.t {
                        self.next();
                        let v = if op == "-" && s != "0" {
                            format!("-{}", s)
                        } else {
                            s.clone()
                        };
                        let lit = E::new(K::Int(v), line);
                        return self.postfix(lit, nd);
                    }
                    if let T::Float(f) = &nt.t {
                        let f = *f;
                        self.next();
                        let lit = E::new(K::Float(if op == "-" { -f } else { f }), line);
                        return self.postfix(lit, nd);
                    }
                }
                let e = self.parse_expr(UNARY_BP, nd)?;
                E::new(
                    K::Un {
                        op: op.into(),
                        e: Box::new(e),
                    },
                    line,
                )
            }
            T::Op(op @ ("!" | "^" | "~~~" | "not")) => {
                let e = self.parse_expr(UNARY_BP, nd)?;
                E::new(
                    K::Un {
                        op: op.into(),
                        e: Box::new(e),
                    },
                    line,
                )
            }
            T::Op("...") => E::new(K::Var("...".into()), line),
            T::Op("..") => {
                // Nullary `..` is the full range 0..-1//1.
                let i = |v: &str| E::new(K::Int(v.into()), line);
                E::bin("..", i("0"), E::bin("//", i("-1"), i("1"), line), line)
            }
            T::KwKey(k) => {
                // keyword list without brackets as an expression (e.g. `do: x` in
                // a position we didn't anticipate)
                self.pos -= 1;
                let _ = k;
                let kw = self.parse_kw_list(0, nd)?;
                return Ok(kw);
            }
            other => {
                self.pos -= 1;
                return Err(format!(
                    "{}:{}: syntax error: unexpected {}",
                    self.file,
                    line,
                    describe(&other)
                ));
            }
        };
        self.postfix(e, nd)
    }

    fn postfix(&mut self, mut e: E, nd: bool) -> PResult<E> {
        loop {
            let t = self.peek_raw().clone();
            match &t.t {
                T::Op(".") => {
                    self.pos += 1;
                    let n = self.next();
                    match n.t {
                        T::Ident(name) => {
                            e = self.remote_call(e, name, n.line, nd)?;
                        }
                        T::Alias(a) => {
                            e = match e.k {
                                K::Alias(base) => {
                                    E::new(K::Alias(format!("{}.{}", base, a)), n.line)
                                }
                                _ => E::new(
                                    K::Bin {
                                        op: ".alias".into(),
                                        l: Box::new(e),
                                        r: Box::new(E::new(K::Alias(a), n.line)),
                                    },
                                    n.line,
                                ),
                            };
                        }
                        T::Op("(") => {
                            let args = self.paren_args()?;
                            e = E::new(
                                K::AnonCall {
                                    f: Box::new(e),
                                    args,
                                },
                                n.line,
                            );
                        }
                        T::Op("{") => {
                            let items = self.items_until("}")?;
                            e = E::new(K::MultiAlias(Box::new(e), items), n.line);
                        }
                        T::Atom(a) if a == "true" || a == "false" || a == "nil" => {
                            e = self.remote_call(e, a, n.line, nd)?;
                        }
                        T::Do | T::End | T::Fn | T::Else | T::Catch | T::Rescue | T::After => {
                            let name = describe(&n.t);
                            e = self.remote_call(
                                e,
                                name.trim_matches('\'').to_string(),
                                n.line,
                                nd,
                            )?;
                        }
                        T::Op(op)
                            if infix_bp(op).is_some() || matches!(op, "!" | "@" | "&" | "^") =>
                        {
                            e = self.remote_call(e, op.to_string(), n.line, nd)?;
                        }
                        T::Str(parts) => {
                            let name = match parts.as_slice() {
                                [Part::Lit(b)] => String::from_utf8_lossy(b).into_owned(),
                                _ => return self.err("invalid quoted call"),
                            };
                            e = self.remote_call(e, name, n.line, nd)?;
                        }
                        _ => return self.err("unexpected token after '.'"),
                    }
                }
                T::Op("(")
                    if !t.sp
                        && matches!(&e.k, K::Call { name, parens: true, .. } if name == "unquote") =>
                {
                    // `unquote(name)(args)` in generated definitions
                    self.pos += 1;
                    let mut args = vec![e.clone()];
                    args.extend(self.paren_args()?);
                    e = E::new(
                        K::Call {
                            name: "__unquote_call__".into(),
                            args,
                            parens: true,
                        },
                        t.line,
                    );
                }
                T::Op("[") if !t.sp => {
                    self.pos += 1;
                    self.nl_ign.push(true);
                    let key = self.parse_expr(0, false)?;
                    let r = self.expect_op("]");
                    self.nl_ign.pop();
                    r?;
                    e = E::new(
                        K::Access {
                            e: Box::new(e),
                            key: Box::new(key),
                        },
                        t.line,
                    );
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn remote_call(&mut self, recv: E, name: String, line: u32, nd: bool) -> PResult<E> {
        let t = self.peek_raw().clone();
        let (mut args, parens) = if matches!(t.t, T::Op("(")) && !t.sp {
            self.pos += 1;
            (self.paren_args()?, true)
        } else if self.can_start_arg(&t) {
            (self.noparen_args()?, false)
        } else {
            (vec![], false)
        };
        if !nd && matches!(self.peek_raw().t, T::Do) {
            self.next();
            args.push(self.parse_do_block()?);
        }
        Ok(E::new(
            K::Remote {
                recv: Box::new(recv),
                name,
                args,
                parens,
            },
            line,
        ))
    }

    fn ident_expr(&mut self, name: String, line: u32, nd: bool) -> PResult<E> {
        let t = self.peek_raw().clone();
        if matches!(t.t, T::Op("(")) && !t.sp {
            self.pos += 1;
            let mut args = self.paren_args()?;
            if !nd && matches!(self.peek_raw().t, T::Do) {
                self.next();
                args.push(self.parse_do_block()?);
            }
            return Ok(E::new(
                K::Call {
                    name,
                    args,
                    parens: true,
                },
                line,
            ));
        }
        if matches!(t.t, T::Do) {
            if nd {
                return Ok(E::new(K::Var(name), line));
            }
            self.next();
            let blk = self.parse_do_block()?;
            return Ok(E::new(
                K::Call {
                    name,
                    args: vec![blk],
                    parens: false,
                },
                line,
            ));
        }
        if self.can_start_arg(&t) {
            let mut args = self.noparen_args()?;
            if !nd && matches!(self.peek_raw().t, T::Do) {
                self.next();
                args.push(self.parse_do_block()?);
            }
            return Ok(E::new(
                K::Call {
                    name,
                    args,
                    parens: false,
                },
                line,
            ));
        }
        Ok(E::new(K::Var(name), line))
    }

    fn can_start_arg(&self, t: &Tok) -> bool {
        if !t.sp {
            return false;
        }
        match &t.t {
            T::Int(_)
            | T::Float(_)
            | T::Atom(_)
            | T::AtomInterp(_)
            | T::Str(_)
            | T::Charlist(_)
            | T::Sigil { .. }
            | T::Ident(_)
            | T::Alias(_)
            | T::KwKey(_)
            | T::CapArg(_)
            | T::Fn => true,
            T::Op("[" | "{" | "%{" | "%" | "<<" | "@" | "&" | "!" | "^" | "(" | "~~~") => true,
            T::Op("not") => {
                // `x not in y` is the binary operator
                let mut k = self.pos + 1;
                while matches!(self.toks.get(k).map(|x| &x.t), Some(T::Nl)) {
                    k += 1;
                }
                !matches!(self.toks.get(k).map(|x| &x.t), Some(T::Op("in")))
            }
            T::Op("-" | "+") => {
                // unary only if no space after the sign
                !t.sp_after
                    && !matches!(
                        self.toks.get(self.pos + 1).map(|x| &x.t),
                        Some(T::Nl) | Some(T::Eof)
                    )
            }
            _ => false,
        }
    }

    fn noparen_args(&mut self) -> PResult<Vec<E>> {
        let mut args = Vec::new();
        loop {
            if matches!(self.peek_raw().t, T::KwKey(_)) {
                args.push(self.parse_kw_list(0, true)?);
                break;
            }
            let e = self.parse_expr(0, true)?;
            args.push(e);
            if matches!(self.peek_raw().t, T::Op(",")) {
                self.pos += 1;
                self.skip_nl();
                continue;
            }
            break;
        }
        Ok(args)
    }

    /// After `(`: comma separated args with optional trailing keyword list.
    fn paren_args(&mut self) -> PResult<Vec<E>> {
        self.nl_ign.push(true);
        let r = self.paren_args_inner();
        self.nl_ign.pop();
        r
    }

    fn paren_args_inner(&mut self) -> PResult<Vec<E>> {
        let mut args = Vec::new();
        if self.at_op(")") {
            self.next();
            return Ok(args);
        }
        loop {
            if matches!(self.peek().t, T::KwKey(_)) {
                args.push(self.parse_kw_list(0, false)?);
                if self.at_op(",") {
                    self.next();
                }
                self.expect_op(")")?;
                break;
            }
            let e = self.parse_expr(0, false)?;
            args.push(e);
            if self.at_op(",") {
                self.next();
                if self.at_op(")") {
                    self.next();
                    break;
                }
                continue;
            }
            self.expect_op(")")?;
            break;
        }
        Ok(args)
    }

    /// `k: v, k2: v2` → list of 2-tuples.
    fn parse_kw_list(&mut self, bp: u8, nd: bool) -> PResult<E> {
        let line = self.line();
        let mut items = Vec::new();
        loop {
            let t = self.next();
            let key = match t.t {
                T::KwKey(k) => k,
                _ => return self.err("expected keyword key"),
            };
            self.skip_nl();
            let v = self.parse_expr(bp, nd)?;
            items.push(E::tuple(vec![E::atom(&key, t.line), v], t.line));
            // continue if `, key:`
            let save = self.pos;
            if matches!(self.peek().t, T::Op(",")) {
                self.next();
                self.skip_nl();
                if matches!(self.peek().t, T::KwKey(_)) {
                    continue;
                }
            }
            self.pos = save;
            break;
        }
        Ok(E::new(K::List(items, None), line))
    }

    fn parse_do_block(&mut self) -> PResult<E> {
        // `do` already consumed
        let line = self.line();
        let mut sections = Vec::new();
        let body = self.parse_stab_body()?;
        sections.push(E::tuple(vec![E::atom("do", line), body], line));
        loop {
            let t = self.next();
            let key = match t.t {
                T::End => break,
                T::Else => "else",
                T::Rescue => "rescue",
                T::Catch => "catch",
                T::After => "after",
                _ => {
                    self.pos -= 1;
                    return self.err("expected 'end'");
                }
            };
            let body = self.parse_stab_body()?;
            sections.push(E::tuple(vec![E::atom(key, t.line), body], t.line));
        }
        Ok(E::new(K::List(sections, None), line))
    }

    fn is_block_term(t: &T) -> bool {
        matches!(
            t,
            T::End | T::Else | T::Rescue | T::Catch | T::After | T::Eof
        )
    }

    /// Body of a do-block section or fn: either a block of expressions or a
    /// list of `head -> body` clauses.
    fn parse_stab_body(&mut self) -> PResult<E> {
        self.nl_ign.push(false);
        let r = self.parse_stab_body_inner();
        self.nl_ign.pop();
        r
    }

    fn parse_stab_body_inner(&mut self) -> PResult<E> {
        let line = self.line();
        let mut clauses: Vec<Clause> = Vec::new();
        let mut body: Vec<E> = Vec::new();
        let mut head: Option<(Vec<E>, u32)> = None;
        let mut pre_body = false;
        loop {
            self.skip_nl_semi();
            let t = self.peek_raw().clone();
            if Self::is_block_term(&t.t) || (self.paren_stab && matches!(t.t, T::Op(")"))) {
                break;
            }
            if matches!(t.t, T::Op("->")) {
                self.pos += 1;
                self.flush_clause(&mut head, &mut body, &mut clauses, pre_body)?;
                head = Some((vec![], t.line));
                continue;
            }
            let e = self.parse_expr(0, false)?;
            let mut exprs = vec![e];
            if matches!(self.peek_raw().t, T::Op(",")) {
                while matches!(self.peek_raw().t, T::Op(",")) {
                    self.pos += 1;
                    self.skip_nl();
                    exprs.push(self.parse_expr(0, false)?);
                }
                if !matches!(self.peek_raw().t, T::Op("->")) {
                    return self.err("unexpected ','");
                }
            }
            if matches!(self.peek_raw().t, T::Op("->")) {
                self.pos += 1;
                if head.is_none() && !body.is_empty() {
                    pre_body = true;
                }
                self.flush_clause(&mut head, &mut body, &mut clauses, pre_body)?;
                head = Some((exprs, t.line));
                continue;
            }
            body.push(exprs.pop().unwrap());
            match &self.peek_raw().t {
                T::Nl | T::Op(";") => {}
                x if Self::is_block_term(x) => {}
                T::Op(")") if self.paren_stab => {}
                _ => return self.err("unexpected token"),
            }
        }
        if head.is_some() {
            self.flush_clause(&mut head, &mut body, &mut clauses, pre_body)?;
        }
        if clauses.is_empty() {
            Ok(E::new(K::Block(body), line))
        } else {
            Ok(E::new(K::Clauses(clauses), line))
        }
    }

    fn flush_clause(
        &mut self,
        head: &mut Option<(Vec<E>, u32)>,
        body: &mut Vec<E>,
        clauses: &mut Vec<Clause>,
        pre_body: bool,
    ) -> PResult<()> {
        if pre_body && clauses.is_empty() && head.is_none() && !body.is_empty() {
            return self.err("unexpected expression before clause");
        }
        if let Some((args, line)) = head.take() {
            let (args, guard) = split_head(args);
            let b = std::mem::take(body);
            let bline = b.first().map(|e| e.line).unwrap_or(line);
            clauses.push(Clause {
                args,
                guard,
                body: E::new(K::Block(b), bline),
                line,
            });
        }
        Ok(())
    }

    fn parse_fn(&mut self, line: u32) -> PResult<E> {
        let body = self.parse_stab_body()?;
        match self.next().t {
            T::End => {}
            _ => return self.err("expected 'end' to close fn"),
        }
        match body.k {
            K::Clauses(cs) => Ok(E::new(K::Fn(cs), line)),
            _ => Err(format!(
                "{}:{}: syntax error: expected clauses in fn",
                self.file, line
            )),
        }
    }

    /// Whether the parenthesised group starting at the current token holds
    /// `->` clauses at its top level, as in `do: (x -> x)`.
    fn paren_has_stab(&self) -> bool {
        let mut depth = 0i32;
        let mut k = self.pos;
        loop {
            match &self.toks[k].t {
                T::Eof => return false,
                T::Op("(" | "[" | "{" | "%{" | "<<") | T::Fn | T::Do => depth += 1,
                T::Op(")" | "]" | "}" | ">>") | T::End => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                T::Op("->") if depth == 0 => return true,
                _ => {}
            }
            k += 1;
        }
    }

    fn parse_paren(&mut self, line: u32) -> PResult<E> {
        if self.paren_has_stab() {
            let saved = self.paren_stab;
            self.paren_stab = true;
            let r = self.parse_stab_body();
            self.paren_stab = saved;
            let e = r?;
            self.skip_nl();
            self.expect_op(")")?;
            return Ok(e);
        }
        // Newlines inside parentheses separate expressions of a block.
        self.nl_ign.push(false);
        let r = (|| -> PResult<E> {
            self.skip_nl_semi();
            if self.at_op(")") {
                self.next();
                return Ok(E::new(K::Block(vec![]), line));
            }
            let first = self.parse_expr(0, false)?;
            self.skip_nl();
            if self.at_op(",") {
                let mut items = vec![first];
                while self.at_op(",") {
                    self.next();
                    self.skip_nl();
                    items.push(self.parse_expr(0, false)?);
                    self.skip_nl();
                }
                self.expect_op(")")?;
                return Ok(E::new(K::ParenArgs(items), line));
            }
            let mut exprs = vec![first];
            loop {
                self.skip_nl_semi();
                if self.at_op(")") {
                    break;
                }
                exprs.push(self.parse_expr(0, false)?);
                if !matches!(self.peek_raw().t, T::Nl | T::Op(";") | T::Op(")")) {
                    return self.err("expected ')'");
                }
            }
            self.expect_op(")")?;
            if exprs.len() == 1 {
                Ok(exprs.pop().unwrap())
            } else {
                Ok(E::new(K::Block(exprs), line))
            }
        })();
        self.nl_ign.pop();
        let e = r?;
        Ok(e)
    }

    fn items_until(&mut self, close: &str) -> PResult<Vec<E>> {
        self.nl_ign.push(true);
        let r = (|| -> PResult<Vec<E>> {
            let mut items = Vec::new();
            loop {
                if self.at_op(close) {
                    self.next();
                    break;
                }
                if matches!(self.peek().t, T::KwKey(_)) {
                    items.push(self.parse_kw_list(0, false)?);
                    if self.at_op(",") {
                        self.next();
                    }
                    self.expect_op(close)?;
                    break;
                }
                items.push(self.parse_expr(0, false)?);
                if self.at_op(",") {
                    self.next();
                    continue;
                }
                self.expect_op(close)?;
                break;
            }
            Ok(items)
        })();
        self.nl_ign.pop();
        r
    }

    fn parse_list(&mut self, line: u32) -> PResult<E> {
        self.nl_ign.push(true);
        let r = (|| -> PResult<E> {
            let mut items = Vec::new();
            let mut tail = None;
            loop {
                if self.at_op("]") {
                    self.next();
                    break;
                }
                if matches!(self.peek().t, T::KwKey(_)) {
                    let kw = self.parse_kw_list(0, false)?;
                    if let K::List(kitems, _) = kw.k {
                        items.extend(kitems);
                    }
                    if self.at_op(",") {
                        self.next();
                    }
                    self.expect_op("]")?;
                    break;
                }
                let mut e = self.parse_expr(41, false)?;
                while self.at_op("::") {
                    let l = self.peek().line;
                    self.next();
                    self.skip_nl();
                    let rhs = self.parse_expr(29, false)?;
                    e = E::bin("::", e, rhs, l);
                }
                while self.at_op("<-") || self.at_op("\\\\") {
                    let op = if self.at_op("<-") { "<-" } else { "\\\\" };
                    let l = self.peek().line;
                    self.next();
                    self.skip_nl();
                    let rhs = self.parse_expr(41, false)?;
                    e = E::bin(op, e, rhs, l);
                }
                items.push(e);
                if self.at_op("|") {
                    self.next();
                    tail = Some(Box::new(self.parse_expr(0, false)?));
                    self.expect_op("]")?;
                    break;
                }
                if self.at_op(",") {
                    self.next();
                    continue;
                }
                self.expect_op("]")?;
                break;
            }
            Ok(E::new(K::List(items, tail), line))
        })();
        self.nl_ign.pop();
        r
    }

    fn parse_tuple(&mut self, line: u32) -> PResult<E> {
        let items = self.items_until("}")?;
        Ok(E::new(K::Tuple(items), line))
    }

    fn parse_map_body(&mut self, line: u32, name: Option<E>) -> PResult<E> {
        self.nl_ign.push(true);
        let r = self.parse_map_inner(line, name);
        self.nl_ign.pop();
        r
    }

    fn parse_map_inner(&mut self, line: u32, name: Option<E>) -> PResult<E> {
        let mut pairs = Vec::new();
        let mut base = None;
        if !self.at_op("}") && !matches!(self.peek().t, T::KwKey(_)) {
            let first = self.parse_expr(51, false)?;
            if self.at_op("|") {
                self.next();
                base = Some(Box::new(first));
            } else {
                let v = if matches!(&first.k, K::Call { name, args, .. } if name == "unquote_splicing" && args.len() == 1)
                    && !self.at_op("=>")
                {
                    E::new(K::ParenArgs(Vec::new()), first.line)
                } else {
                    self.expect_op("=>")?;
                    self.parse_expr(0, false)?
                };
                pairs.push((first, v));
                if self.at_op(",") {
                    self.next();
                }
            }
        }
        loop {
            if self.at_op("}") {
                self.next();
                break;
            }
            if let T::KwKey(k) = self.peek_t() {
                let t = self.next();
                let v = self.parse_expr(0, false)?;
                pairs.push((E::atom(&k, t.line), v));
            } else {
                let k = self.parse_expr(51, false)?;
                let v = if matches!(&k.k, K::Call { name, args, .. } if name == "unquote_splicing" && args.len() == 1)
                    && !self.at_op("=>")
                {
                    E::new(K::ParenArgs(Vec::new()), k.line)
                } else {
                    self.expect_op("=>")?;
                    self.parse_expr(0, false)?
                };
                pairs.push((k, v));
            }
            if self.at_op(",") {
                self.next();
                continue;
            }
            self.expect_op("}")?;
            break;
        }
        Ok(match name {
            Some(n) => E::new(
                K::Struct {
                    name: Box::new(n),
                    base,
                    pairs,
                },
                line,
            ),
            None => match base {
                Some(b) => E::new(K::MapUpd(b, pairs), line),
                None => E::new(K::Map(pairs), line),
            },
        })
    }

    fn parse_struct(&mut self, line: u32) -> PResult<E> {
        let t = self.next();
        let mut name = match t.t {
            T::Alias(a) => E::new(K::Alias(a), t.line),
            T::Ident(n) if matches!(self.peek_raw().t, T::Op("(")) && !self.peek_raw().sp => {
                self.pos += 1;
                let args = self.paren_args()?;
                E::call(&n, args, t.line)
            }
            T::Atom(a) => E::new(K::Atom(a), t.line),
            T::Ident(n) => E::new(K::Var(n), t.line),
            T::Op("@") => match self.next().t {
                T::Ident(n) => E::new(K::Attr(n), t.line),
                _ => return self.err("bad struct name"),
            },
            T::Op("^") => match self.next().t {
                T::Ident(n) => E::new(
                    K::Un {
                        op: "^".into(),
                        e: Box::new(E::new(K::Var(n), t.line)),
                    },
                    t.line,
                ),
                _ => return self.err("bad struct name"),
            },
            _ => return self.err("expected struct name after %"),
        };
        while matches!(self.peek_raw().t, T::Op(".")) {
            self.pos += 1;
            match self.next().t {
                T::Alias(a) => {
                    name = match name.k {
                        K::Alias(b) => E::new(K::Alias(format!("{}.{}", b, a)), t.line),
                        _ => E::new(
                            K::Bin {
                                op: ".alias".into(),
                                l: Box::new(name),
                                r: Box::new(E::new(K::Alias(a), t.line)),
                            },
                            t.line,
                        ),
                    }
                }
                _ => return self.err("bad struct name"),
            }
        }
        self.expect_op("{")?;
        self.parse_map_body(line, Some(name))
    }

    fn parse_bits(&mut self, line: u32) -> PResult<E> {
        self.nl_ign.push(true);
        let r = (|| -> PResult<E> {
            let mut segs = Vec::new();
            loop {
                if self.at_op(">>") {
                    self.next();
                    break;
                }
                segs.push(self.parse_expr(10, false)?);
                if self.at_op(",") {
                    self.next();
                    continue;
                }
                self.expect_op(">>")?;
                break;
            }
            Ok(E::new(K::Bits(segs), line))
        })();
        self.nl_ign.pop();
        r
    }

    fn interp_parts(&mut self, parts: Vec<Part>) -> PResult<Vec<IPart>> {
        let mut out = Vec::new();
        for p in parts {
            match p {
                Part::Lit(b) => out.push(IPart::Lit(b)),
                Part::Code(src, line) => {
                    let toks = tokenize(&src, &self.file, line)?;
                    let mut sub = Parser::new(toks, &self.file);
                    let es = sub.parse_file()?;
                    let e = match es.len() {
                        0 => E::nil(line),
                        1 => es.into_iter().next().unwrap(),
                        _ => E::new(K::Block(es), line),
                    };
                    out.push(IPart::Expr(e));
                }
            }
        }
        Ok(out)
    }
}

fn str_or_interp(parts: Vec<IPart>, line: u32) -> E {
    if parts.iter().all(|p| matches!(p, IPart::Lit(_))) {
        let mut b = Vec::new();
        for p in parts {
            if let IPart::Lit(x) = p {
                b.extend(x);
            }
        }
        E::new(K::Str(b), line)
    } else {
        E::new(K::Interp(parts), line)
    }
}

/// Split `a, b when guard` into args and guard.
fn split_head(mut args: Vec<E>) -> (Vec<E>, Option<E>) {
    if args.len() == 1 {
        if let K::ParenArgs(_) = &args[0].k {
            if let K::ParenArgs(inner) = args.pop().unwrap().k {
                return (inner, None);
            }
        }
    }
    if let Some(last) = args.pop() {
        if let K::Bin { op, l, r } = last.k.clone() {
            if op == "when" {
                let guard = flatten_when(*r);
                match l.k {
                    K::ParenArgs(inner) if args.is_empty() => return (inner, Some(guard)),
                    _ => {
                        args.push(*l);
                        return (args, Some(guard));
                    }
                }
            }
        }
        args.push(last);
    }
    (args, None)
}

/// `a when b` inside a guard means `a or b`.
fn flatten_when(e: E) -> E {
    if let K::Bin { op, l, r } = &e.k {
        if op == "when" {
            let line = e.line;
            return E::bin("or", (**l).clone(), flatten_when((**r).clone()), line);
        }
    }
    e
}

pub fn describe(t: &T) -> String {
    match t {
        T::Int(s) => s.clone(),
        T::Float(f) => f.to_string(),
        T::Atom(a) => format!(":{}", a),
        T::AtomInterp(_) => "atom".into(),
        T::Str(_) => "string".into(),
        T::Charlist(_) => "charlist".into(),
        T::Sigil { ch, .. } => format!("~{}", ch),
        T::Ident(n) => n.clone(),
        T::Alias(n) => n.clone(),
        T::KwKey(k) => format!("{}:", k),
        T::Op(o) => format!("'{}'", o),
        T::CapArg(n) => format!("&{}", n),
        T::Do => "do".into(),
        T::End => "end".into(),
        T::Fn => "fn".into(),
        T::Else => "else".into(),
        T::Catch => "catch".into(),
        T::Rescue => "rescue".into(),
        T::After => "after".into(),
        T::Nl => "newline".into(),
        T::Eof => "end of file".into(),
    }
}

pub fn parse_source(src: &str, file: &str) -> PResult<Vec<E>> {
    let toks = tokenize(src, file, 1)?;
    let mut p = Parser::new(toks, file);
    p.parse_file()
}
