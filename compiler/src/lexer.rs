//! Elixir tokenizer.

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Lit(Vec<u8>),
    /// Interpolated source code and its starting line.
    Code(String, u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum T {
    Int(String),
    Float(f64),
    Atom(String),
    AtomInterp(Vec<Part>),
    Str(Vec<Part>),
    Charlist(Vec<Part>),
    Sigil {
        ch: char,
        parts: Vec<Part>,
        mods: String,
    },
    Ident(String),
    Alias(String),
    /// `key:` in keyword lists (also `"quoted key":`)
    KwKey(String),
    Op(&'static str),
    CapArg(u32),
    Do,
    End,
    Fn,
    Else,
    Catch,
    Rescue,
    After,
    Nl,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Tok {
    pub t: T,
    pub line: u32,
    /// whitespace (or newline) immediately before this token
    pub sp: bool,
    /// whitespace immediately after this token
    pub sp_after: bool,
}

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: u32,
    pub toks: Vec<Tok>,
    file: String,
}

const OPS: &[&str] = &[
    "===", "!==", "<<<", ">>>", "&&&", "|||", "^^^", "~~~", "<<~", "~>>", "<~>", "...", "+++",
    "---", "//", "==", "!=", "<=", ">=", "<-", "->", "=>", "=~", "|>", "<>", "++", "--", "..",
    "::", "\\\\", "&&", "||", "**", "<~", "~>", "<<", ">>", "+", "-", "*", "/", "<", ">", "=",
    "!", "^", "&", "|", ".", "@", "(", ")", "[", "]", "{", "}", ",", ";", "%",
];

pub type LexResult<T> = Result<T, String>;

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_lowercase() || (!c.is_ascii() && c.is_alphabetic() && !c.is_uppercase())
}
fn is_ident_cont(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str, file: &str, line: u32) -> Self {
        Lexer {
            src: src.as_bytes(),
            pos: 0,
            line,
            toks: Vec::new(),
            file: file.to_string(),
        }
    }

    fn err<T>(&self, msg: &str) -> LexResult<T> {
        Err(format!("{}:{}: syntax error: {}", self.file, self.line, msg))
    }

    fn peek(&self) -> u8 {
        *self.src.get(self.pos).unwrap_or(&0)
    }
    fn peek_at(&self, n: usize) -> u8 {
        *self.src.get(self.pos + n).unwrap_or(&0)
    }
    fn starts(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s.as_bytes())
    }
    fn cur_char(&self) -> char {
        // Decode just the next UTF-8 sequence (at most 4 bytes).
        let end = (self.pos + 4).min(self.src.len());
        let chunk = &self.src[self.pos..end];
        for n in (1..=chunk.len()).rev() {
            if let Ok(s) = std::str::from_utf8(&chunk[..n]) {
                return s.chars().next().unwrap_or('\0');
            }
        }
        '\0'
    }

    fn push(&mut self, t: T, line: u32, sp: bool) {
        if let Some(last) = self.toks.last_mut() {
            last.sp_after = sp;
        }
        self.toks.push(Tok {
            t,
            line,
            sp,
            sp_after: false,
        });
    }

    pub fn run(mut self) -> LexResult<Vec<Tok>> {
        let mut sp = true;
        loop {
            // whitespace
            let c = self.peek();
            if c == b' ' || c == b'\t' || c == b'\r' {
                self.pos += 1;
                sp = true;
                continue;
            }
            if c == b'\\' && self.peek_at(1) == b'\n' {
                self.pos += 2;
                self.line += 1;
                sp = true;
                continue;
            }
            if c == b'#' {
                while self.pos < self.src.len() && self.peek() != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            if c == b'\n' {
                let line = self.line;
                self.pos += 1;
                self.line += 1;
                if !matches!(self.toks.last().map(|t| &t.t), Some(T::Nl) | None) {
                    self.push(T::Nl, line, sp);
                }
                sp = true;
                continue;
            }
            if self.pos >= self.src.len() {
                let line = self.line;
                self.push(T::Nl, line, sp);
                self.push(T::Eof, line, true);
                break;
            }
            let line = self.line;
            self.token(line, sp)?;
            sp = false;
        }
        Ok(self.toks)
    }

    fn token(&mut self, line: u32, sp: bool) -> LexResult<()> {
        let c = self.peek();
        let ch = self.cur_char();
        // Numbers
        if c.is_ascii_digit() {
            let t = self.number()?;
            self.push(t, line, sp);
            return Ok(());
        }
        // Identifiers / keywords
        if is_ident_start(ch) {
            let start = self.pos;
            self.pos += ch.len_utf8();
            loop {
                let c2 = self.cur_char();
                if is_ident_cont(c2) {
                    self.pos += c2.len_utf8();
                } else {
                    break;
                }
            }
            if self.peek() == b'?' || (self.peek() == b'!' && self.peek_at(1) != b'=') {
                self.pos += 1;
            }
            let name = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
            // keyword key `name:` (followed by space/newline, not `::`)
            if self.peek() == b':' && self.peek_at(1) != b':' {
                let n1 = self.peek_at(1);
                if n1 == b' ' || n1 == b'\n' || n1 == b'\t' || n1 == b'\r' || n1 == 0 {
                    self.pos += 1;
                    self.push(T::KwKey(name), line, sp);
                    return Ok(());
                }
            }
            // Operator words used as function names: `def in(...)`, `x.not()`.
            let after_def = matches!(self.toks.last().map(|t| &t.t), Some(T::Ident(d)) if d == "def" || d == "defp" || d == "defmacro" || d == "defdelegate")
                || matches!(self.toks.last().map(|t| &t.t), Some(T::Op(".")));
            if after_def && matches!(name.as_str(), "in" | "and" | "or" | "not" | "when") {
                self.push(T::Ident(name), line, sp);
                return Ok(());
            }
            let t = match name.as_str() {
                "do" => T::Do,
                "end" => T::End,
                "fn" => T::Fn,
                "else" => T::Else,
                "catch" => T::Catch,
                "rescue" => T::Rescue,
                "after" => T::After,
                "true" | "false" | "nil" => T::Atom(name),
                "when" => T::Op("when"),
                "and" => T::Op("and"),
                "or" => T::Op("or"),
                "not" => T::Op("not"),
                "in" => T::Op("in"),
                _ => T::Ident(name),
            };
            self.push(t, line, sp);
            return Ok(());
        }
        if ch.is_uppercase() {
            let start = self.pos;
            self.pos += ch.len_utf8();
            loop {
                let c2 = self.cur_char();
                if is_ident_cont(c2) {
                    self.pos += c2.len_utf8();
                } else {
                    break;
                }
            }
            let name = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
            if self.peek() == b':' && self.peek_at(1) != b':' {
                let n1 = self.peek_at(1);
                if n1 == b' ' || n1 == b'\n' || n1 == b'\t' {
                    self.pos += 1;
                    self.push(T::KwKey(name), line, sp);
                    return Ok(());
                }
            }
            self.push(T::Alias(name), line, sp);
            return Ok(());
        }
        match c {
            b'"' => {
                let parts = if self.starts("\"\"\"") {
                    self.heredoc(b'"', true)?
                } else {
                    self.pos += 1;
                    self.string_body(b'"', true, false)?
                };
                // quoted keyword key
                if self.peek() == b':' && self.peek_at(1) != b':' {
                    let n1 = self.peek_at(1);
                    if n1 == b' ' || n1 == b'\n' || n1 == b'\t' {
                        if let [Part::Lit(b)] = parts.as_slice() {
                            self.pos += 1;
                            self.push(T::KwKey(String::from_utf8_lossy(b).into_owned()), line, sp);
                            return Ok(());
                        }
                        if parts.is_empty() {
                            self.pos += 1;
                            self.push(T::KwKey(String::new()), line, sp);
                            return Ok(());
                        }
                    }
                }
                self.push(T::Str(parts), line, sp);
                return Ok(());
            }
            b'\'' => {
                let parts = if self.starts("'''") {
                    self.heredoc(b'\'', true)?
                } else {
                    self.pos += 1;
                    self.string_body(b'\'', true, false)?
                };
                if self.peek() == b':' && self.peek_at(1) != b':' {
                    let n1 = self.peek_at(1);
                    if n1 == b' ' || n1 == b'\n' {
                        if let [Part::Lit(b)] = parts.as_slice() {
                            self.pos += 1;
                            self.push(T::KwKey(String::from_utf8_lossy(b).into_owned()), line, sp);
                            return Ok(());
                        }
                    }
                }
                self.push(T::Charlist(parts), line, sp);
                return Ok(());
            }
            b':' => {
                if self.peek_at(1) == b':' {
                    self.pos += 2;
                    self.push(T::Op("::"), line, sp);
                    return Ok(());
                }
                // atom
                self.pos += 1;
                let c1 = self.peek();
                let ch1 = self.cur_char();
                if c1 == b'"' || c1 == b'\'' {
                    self.pos += 1;
                    let parts = self.string_body(c1, true, false)?;
                    match parts.as_slice() {
                        [] => self.push(T::Atom(String::new()), line, sp),
                        [Part::Lit(b)] => {
                            self.push(T::Atom(String::from_utf8_lossy(b).into_owned()), line, sp)
                        }
                        _ => self.push(T::AtomInterp(parts), line, sp),
                    }
                    return Ok(());
                }
                if is_ident_start(ch1) || ch1.is_uppercase() {
                    let start = self.pos;
                    self.pos += ch1.len_utf8();
                    loop {
                        let c2 = self.cur_char();
                        if is_ident_cont(c2) || c2 == '@' || (c2 == '.' && ch1.is_uppercase() && {
                            let n = self.src.get(self.pos + 1).copied().unwrap_or(0) as char;
                            n.is_uppercase()
                        }) {
                            self.pos += c2.len_utf8();
                        } else {
                            break;
                        }
                    }
                    if self.peek() == b'?' || self.peek() == b'!' {
                        if self.peek_at(1) != b'=' || self.peek_at(2) == b'=' {
                            self.pos += 1;
                        }
                    }
                    let name = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                    self.push(T::Atom(name), line, sp);
                    return Ok(());
                }
                // operator atoms
                for op in [
                    "%{}", "{}", "<<>>", "...", "..//", "+++", "---", "<|>", "===", "!==", "<<<", ">>>", "&&&", "|||",
                    "^^^", "~~~", "<~>", "<<~", "~>>", "==", "!=", "<=", ">=", "&&", "||", "<>",
                    "++", "--", "|>", "=~", "..", "->", "<-", "=>", "**", "\\\\", "::", "<~",
                    "~>", "+", "-", "*", "/", "<", ">", "!", "^", "&", "|", "=", "@", ".", "%",
                ] {
                    if self.starts(op) {
                        self.pos += op.len();
                        self.push(T::Atom(op.to_string()), line, sp);
                        return Ok(());
                    }
                }
                return self.err("unexpected ':'");
            }
            b'?' => {
                // character literal
                self.pos += 1;
                let v: u32 = if self.peek() == b'\\' {
                    self.pos += 1;
                    let e = self.peek();
                    self.pos += 1;
                    match e {
                        b'n' => 10,
                        b't' => 9,
                        b'r' => 13,
                        b's' => 32,
                        b'0' => 0,
                        b'e' => 27,
                        b'a' => 7,
                        b'b' => 8,
                        b'f' => 12,
                        b'v' => 11,
                        b'd' => 127,
                        b'\\' => 92,
                        _ => {
                            self.pos -= 1;
                            let ch = self.cur_char();
                            self.pos += ch.len_utf8();
                            ch as u32
                        }
                    }
                } else {
                    let ch = self.cur_char();
                    if ch == '\n' {
                        self.line += 1;
                    }
                    self.pos += ch.len_utf8();
                    ch as u32
                };
                self.push(T::Int(v.to_string()), line, sp);
                return Ok(());
            }
            b'~' => {
                let n = self.peek_at(1);
                if n.is_ascii_alphabetic() {
                    return self.sigil(line, sp);
                }
            }
            b'&' => {
                if self.peek_at(1).is_ascii_digit() && self.peek_at(1) != b'0' {
                    self.pos += 1;
                    let start = self.pos;
                    while self.peek().is_ascii_digit() {
                        self.pos += 1;
                    }
                    let n: u32 = std::str::from_utf8(&self.src[start..self.pos])
                        .unwrap()
                        .parse()
                        .unwrap();
                    self.push(T::CapArg(n), line, sp);
                    return Ok(());
                }
            }
            b'%' => {
                if self.peek_at(1) == b':' && matches!(self.peek_at(2), b' ' | b'\n' | b'\t' | b'\r') {
                    self.pos += 2;
                    self.push(T::KwKey("%".into()), line, sp);
                    return Ok(());
                }
                if self.peek_at(1) == b'{'
                    && !(self.starts("%{}:") && matches!(self.peek_at(4), b' ' | b'\n' | b'\t' | b'\r'))
                {
                    self.pos += 2;
                    self.push(T::Op("%{"), line, sp);
                    return Ok(());
                }
            }
            _ => {}
        }
        // Operators used as keyword keys: `[+: 1, "::": 2, %{}: 3]`.
        for op in ["%{}", "<<>>", "..//", "...", "{}"].iter().chain(OPS.iter()) {
            if matches!(*op, "(" | ")" | "[" | "]" | "{" | "}" | "," | ";" | "%") {
                continue;
            }
            if self.starts(op)
                && self.peek_at(op.len()) == b':'
                && matches!(self.peek_at(op.len() + 1), b' ' | b'\n' | b'\t' | b'\r')
            {
                self.pos += op.len() + 1;
                self.push(T::KwKey(op.to_string()), line, sp);
                return Ok(());
            }
        }
        for op in OPS {
            if self.starts(op) {
                self.pos += op.len();
                self.push(T::Op(op), line, sp);
                return Ok(());
            }
        }
        self.err(&format!("unexpected character {:?}", ch))
    }

    fn number(&mut self) -> LexResult<T> {
        let start = self.pos;
        if self.peek() == b'0' && matches!(self.peek_at(1), b'x' | b'b' | b'o') {
            let base = match self.peek_at(1) {
                b'x' => 16,
                b'b' => 2,
                _ => 8,
            };
            self.pos += 2;
            let ds = self.pos;
            while self.peek().is_ascii_hexdigit() || self.peek() == b'_' {
                self.pos += 1;
            }
            let digits: String = std::str::from_utf8(&self.src[ds..self.pos])
                .unwrap()
                .chars()
                .filter(|c| *c != '_')
                .collect();
            let v = parse_big_radix(&digits, base).ok_or("bad number")?;
            return Ok(T::Int(v));
        }
        while self.peek().is_ascii_digit() || (self.peek() == b'_' && self.peek_at(1).is_ascii_digit()) {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.peek() == b'.' && self.peek_at(1).is_ascii_digit() {
            is_float = true;
            self.pos += 1;
            while self.peek().is_ascii_digit() || (self.peek() == b'_' && self.peek_at(1).is_ascii_digit()) {
                self.pos += 1;
            }
            if self.peek() == b'e' || self.peek() == b'E' {
                let save = self.pos;
                self.pos += 1;
                if self.peek() == b'-' || self.peek() == b'+' {
                    self.pos += 1;
                }
                if self.peek().is_ascii_digit() {
                    while self.peek().is_ascii_digit() {
                        self.pos += 1;
                    }
                } else {
                    self.pos = save;
                }
            }
        }
        let text: String = std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .chars()
            .filter(|c| *c != '_')
            .collect();
        if is_float {
            Ok(T::Float(text.parse().map_err(|_| "bad float")?))
        } else {
            Ok(T::Int(text.trim_start_matches('0').to_string()).normalize_int())
        }
    }

    fn read_escape(&mut self, out: &mut Vec<u8>) -> LexResult<()> {
        // after backslash
        let e = self.peek();
        self.pos += 1;
        match e {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b's' => out.push(b' '),
            b'0' => out.push(0),
            b'e' => out.push(27),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'd' => out.push(127),
            b'\n' => {
                self.line += 1;
            }
            b'x' => {
                if self.peek() == b'{' {
                    self.pos += 1;
                    let s = self.pos;
                    while self.peek() != b'}' {
                        self.pos += 1;
                    }
                    let h = std::str::from_utf8(&self.src[s..self.pos]).unwrap();
                    self.pos += 1;
                    let v = u32::from_str_radix(h, 16).map_err(|_| "bad \\x escape")?;
                    push_cp(out, v);
                } else {
                    let s = self.pos;
                    let mut n = 0;
                    while n < 2 && self.peek().is_ascii_hexdigit() {
                        self.pos += 1;
                        n += 1;
                    }
                    let h = std::str::from_utf8(&self.src[s..self.pos]).unwrap();
                    let v = u8::from_str_radix(h, 16).map_err(|_| "bad \\x escape")?;
                    out.push(v);
                }
            }
            b'u' => {
                let h = if self.peek() == b'{' {
                    self.pos += 1;
                    let s = self.pos;
                    while self.peek() != b'}' {
                        self.pos += 1;
                    }
                    let h = std::str::from_utf8(&self.src[s..self.pos]).unwrap().to_string();
                    self.pos += 1;
                    h
                } else {
                    let s = self.pos;
                    self.pos += 4;
                    std::str::from_utf8(&self.src[s..self.pos]).unwrap().to_string()
                };
                let v = u32::from_str_radix(&h, 16).map_err(|_| "bad \\u escape")?;
                push_cp(out, v);
            }
            _ => {
                // unknown escape: the char itself
                self.pos -= 1;
                let ch = self.cur_char();
                self.pos += ch.len_utf8();
                let mut b = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
            }
        }
        Ok(())
    }

    /// Scan `#{ ... }` starting after `#{`; returns source.
    fn interp_code(&mut self) -> LexResult<(String, u32)> {
        let line = self.line;
        let start = self.pos;
        let mut depth = 1;
        while self.pos < self.src.len() {
            let c = self.peek();
            match c {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let code = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
                        self.pos += 1;
                        return Ok((code, line));
                    }
                }
                b'\n' => self.line += 1,
                b'"' | b'\'' => {
                    // skip nested string
                    let q = c;
                    self.pos += 1;
                    while self.pos < self.src.len() && self.peek() != q {
                        if self.peek() == b'\\' {
                            self.pos += 1;
                        } else if self.peek() == b'#' && self.peek_at(1) == b'{' {
                            self.pos += 2;
                            self.interp_code()?;
                            continue;
                        } else if self.peek() == b'\n' {
                            self.line += 1;
                        }
                        self.pos += 1;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        self.err("unterminated interpolation")
    }

    /// Body of a quoted string after the opening delimiter.
    fn string_body(&mut self, close: u8, interp: bool, raw: bool) -> LexResult<Vec<Part>> {
        self.delimited(close, 0, interp, raw, false)
    }

    /// Generic delimited content reader. `open` is the nesting opener (0 if none).
    fn delimited(&mut self, close: u8, open: u8, interp: bool, raw: bool, keep_escapes: bool) -> LexResult<Vec<Part>> {
        let mut parts = Vec::new();
        let mut cur: Vec<u8> = Vec::new();
        let mut depth = 0;
        loop {
            if self.pos >= self.src.len() {
                if close == 0xff {
                    break;
                }
                return self.err("unterminated string");
            }
            let c = self.peek();
            if c == close && depth == 0 {
                self.pos += 1;
                break;
            }
            if open != 0 && c == open {
                depth += 1;
            } else if open != 0 && c == close {
                depth -= 1;
            }
            if c == b'\\' {
                if raw || keep_escapes {
                    // only the delimiter can be escaped
                    let n = self.peek_at(1);
                    if n == close || (open != 0 && n == open) {
                        if keep_escapes && close != b'/' {
                            cur.push(b'\\');
                        }
                        cur.push(n);
                        self.pos += 2;
                        continue;
                    }
                    if keep_escapes && n == b'#' && self.peek_at(2) == b'{' {
                        cur.extend_from_slice(b"#{");
                        self.pos += 3;
                        continue;
                    }
                    cur.push(b'\\');
                    self.pos += 1;
                    if keep_escapes && self.pos < self.src.len() {
                        cur.push(self.peek());
                        if self.peek() == b'\n' {
                            self.line += 1;
                        }
                        self.pos += 1;
                    }
                    continue;
                }
                self.pos += 1;
                self.read_escape(&mut cur)?;
                continue;
            }
            if interp && c == b'#' && self.peek_at(1) == b'{' {
                self.pos += 2;
                if !cur.is_empty() {
                    parts.push(Part::Lit(std::mem::take(&mut cur)));
                }
                let (code, line) = self.interp_code()?;
                parts.push(Part::Code(code, line));
                continue;
            }
            if c == b'\n' {
                self.line += 1;
            }
            cur.push(c);
            self.pos += 1;
        }
        if !cur.is_empty() {
            parts.push(Part::Lit(cur));
        }
        Ok(parts)
    }

    fn heredoc(&mut self, q: u8, interp: bool) -> LexResult<Vec<Part>> {
        self.heredoc_x(q, interp, false, false)
    }

    fn heredoc_x(&mut self, q: u8, interp: bool, raw: bool, keep_escapes: bool) -> LexResult<Vec<Part>> {
        self.pos += 3;
        // skip to end of line
        while self.peek() != b'\n' {
            if self.pos >= self.src.len() {
                return self.err("unterminated heredoc");
            }
            self.pos += 1;
        }
        self.pos += 1;
        self.line += 1;
        // collect raw lines until closing delimiter line
        let mut lines: Vec<(usize, usize, u32)> = Vec::new();
        let close = [q, q, q];
        let indent;
        loop {
            if self.pos >= self.src.len() {
                return self.err("unterminated heredoc");
            }
            let ls = self.pos;
            while self.pos < self.src.len() && self.peek() != b'\n' {
                self.pos += 1;
            }
            let le = self.pos;
            let text = &self.src[ls..le];
            let trimmed_start = text.iter().position(|&b| b != b' ' && b != b'\t').unwrap_or(text.len());
            if text[trimmed_start..].starts_with(&close) {
                indent = trimmed_start;
                self.pos = ls + trimmed_start + 3;
                break;
            }
            lines.push((ls, le, self.line));
            self.pos += 1;
            self.line += 1;
        }
        // Build content with indentation stripped, then process escapes/interp.
        let mut content: Vec<u8> = Vec::new();
        for (ls, le, _) in &lines {
            let text = &self.src[*ls..*le];
            let strip = text
                .iter()
                .take(indent)
                .take_while(|&&b| b == b' ' || b == b'\t')
                .count();
            content.extend_from_slice(&text[strip..]);
            content.push(b'\n');
        }
        let start_line = lines.first().map(|l| l.2).unwrap_or(self.line);
        let content = String::from_utf8_lossy(&content).into_owned();
        let mut sub = Lexer::new(&content, &self.file, start_line);
        sub.src = content.as_bytes();
        let parts = sub.delimited(0xff, 0, interp, raw, keep_escapes)?;
        Ok(parts)
    }

    fn sigil(&mut self, line: u32, sp: bool) -> LexResult<()> {
        self.pos += 1;
        let ch = self.peek() as char;
        self.pos += 1;
        // Multi-letter uppercase sigils (~HTML) - consume trailing uppercase letters
        let d = self.peek();
        let lower = ch.is_ascii_lowercase();
        let parts = if (d == b'"' || d == b'\'') && self.peek_at(1) == d && self.peek_at(2) == d {
            self.heredoc_x(d, lower, !lower, ch == 'r')?
        } else {
            let (open, close) = match d {
                b'(' => (b'(', b')'),
                b'[' => (b'[', b']'),
                b'{' => (b'{', b'}'),
                b'<' => (b'<', b'>'),
                b'"' | b'\'' | b'|' | b'/' => (0, d),
                _ => return self.err("invalid sigil delimiter"),
            };
            self.pos += 1;
            if ch == 'r' {
                self.delimited(close, open, true, false, true)?
            } else {
                self.delimited(close, open, lower, !lower, false)?
            }
        };
        let ms = self.pos;
        while self.peek().is_ascii_alphabetic() {
            self.pos += 1;
        }
        let mods = String::from_utf8_lossy(&self.src[ms..self.pos]).into_owned();
        self.push(T::Sigil { ch, parts, mods }, line, sp);
        Ok(())
    }
}

trait NormInt {
    fn normalize_int(self) -> T;
}
impl NormInt for T {
    fn normalize_int(self) -> T {
        match self {
            T::Int(s) if s.is_empty() => T::Int("0".into()),
            t => t,
        }
    }
}

fn push_cp(out: &mut Vec<u8>, v: u32) {
    if let Some(ch) = char::from_u32(v) {
        let mut b = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
    }
}

/// Convert digits in `base` to a decimal string (arbitrary size).
fn parse_big_radix(digits: &str, base: u32) -> Option<String> {
    if digits.is_empty() {
        return None;
    }
    // decimal digit vector, little endian
    let mut dec: Vec<u32> = vec![0];
    for c in digits.chars() {
        let d = c.to_digit(base)?;
        let mut carry = d;
        for x in dec.iter_mut() {
            let v = *x * base + carry;
            *x = v % 10;
            carry = v / 10;
        }
        while carry > 0 {
            dec.push(carry % 10);
            carry /= 10;
        }
    }
    while dec.len() > 1 && *dec.last().unwrap() == 0 {
        dec.pop();
    }
    Some(dec.iter().rev().map(|d| char::from_digit(*d, 10).unwrap()).collect())
}

pub fn tokenize(src: &str, file: &str, line: u32) -> LexResult<Vec<Tok>> {
    Lexer::new(src, file, line).run()
}
