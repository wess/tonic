//! Erlang tokenizer (for compiling OTP's own .erl sources).

#[derive(Clone, Debug, PartialEq)]
pub enum T {
    Atom(String),
    Var(String),
    Int(i128),
    Float(f64),
    Char(u32),
    /// String literal, as Unicode codepoints.
    Str(Vec<u32>),
    /// Punctuation or keyword.
    P(String),
    /// `.` ending a form.
    Dot,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Tok {
    pub t: T,
    pub line: u32,
}

const KEYWORDS: &[&str] = &[
    "after", "and", "andalso", "band", "begin", "bnot", "bor", "bsl", "bsr", "bxor", "case", "catch", "cond", "div",
    "end", "fun", "if", "let", "not", "of", "or", "orelse", "receive", "rem", "try", "when", "xor", "maybe", "else",
];

// Longest first.
const PUNCT: &[&str] = &[
    "=:=", "=/=", "...", "<<", ">>", "<-", "<=", "=>", ":=", "->", "||", "++", "--", "==", "/=", "=<", ">=", "::", "..",
    "?=", "??", "(", ")", "{", "}", "[", "]", ",", ";", ":", "|", "!", "=", "<", ">", "+", "-", "*", "/", "#", "?", ".",
];

pub fn tokenize(src: &str, file: &str) -> Result<Vec<Tok>, String> {
    let s: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut out: Vec<Tok> = Vec::new();
    let err = |line: u32, m: &str| Err(format!("{}:{}: {}", file, line, m));
    while i < s.len() {
        let c = s[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '%' {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
            continue;
        }
        let tl = line;
        // numbers
        if c.is_ascii_digit() {
            let st = i;
            while i < s.len() && (s[i].is_ascii_digit() || s[i] == '_') {
                i += 1;
            }
            if i < s.len() && s[i] == '#' {
                let base: u32 = s[st..i].iter().filter(|c| **c != '_').collect::<String>().parse().unwrap_or(10);
                i += 1;
                let ds = i;
                while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == '_') {
                    i += 1;
                }
                let digits: String = s[ds..i].iter().filter(|c| **c != '_').collect();
                let v = i128::from_str_radix(&digits, base).map_err(|_| format!("{}:{}: bad number", file, tl))?;
                out.push(Tok { t: T::Int(v), line: tl });
                continue;
            }
            if i + 1 < s.len() && s[i] == '.' && s[i + 1].is_ascii_digit() {
                i += 1;
                while i < s.len() && (s[i].is_ascii_digit() || s[i] == '_') {
                    i += 1;
                }
                if i < s.len() && (s[i] == 'e' || s[i] == 'E') {
                    let save = i;
                    i += 1;
                    if i < s.len() && (s[i] == '-' || s[i] == '+') {
                        i += 1;
                    }
                    if i < s.len() && s[i].is_ascii_digit() {
                        while i < s.len() && s[i].is_ascii_digit() {
                            i += 1;
                        }
                    } else {
                        i = save;
                    }
                }
                let txt: String = s[st..i].iter().filter(|c| **c != '_').collect();
                out.push(Tok { t: T::Float(txt.parse().map_err(|_| format!("{}:{}: bad float", file, tl))?), line: tl });
                continue;
            }
            let txt: String = s[st..i].iter().filter(|c| **c != '_').collect();
            out.push(Tok { t: T::Int(txt.parse().map_err(|_| format!("{}:{}: bad integer", file, tl))?), line: tl });
            continue;
        }
        if c == '$' {
            i += 1;
            if i >= s.len() {
                return err(tl, "bad char");
            }
            let (ch, ni) = if s[i] == '\\' { escape(&s, i + 1)? } else { (s[i] as u32, i + 1) };
            if s[i] == '\n' {
                line += 1;
            }
            i = ni;
            out.push(Tok { t: T::Char(ch), line: tl });
            continue;
        }
        if c.is_lowercase() || (c.is_alphabetic() && !c.is_uppercase() && c != '_') {
            let st = i;
            while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_' || s[i] == '@') {
                i += 1;
            }
            let w: String = s[st..i].iter().collect();
            if KEYWORDS.contains(&w.as_str()) {
                out.push(Tok { t: T::P(w), line: tl });
            } else {
                out.push(Tok { t: T::Atom(w), line: tl });
            }
            continue;
        }
        if c.is_uppercase() || c == '_' {
            let st = i;
            while i < s.len() && (s[i].is_alphanumeric() || s[i] == '_' || s[i] == '@') {
                i += 1;
            }
            out.push(Tok { t: T::Var(s[st..i].iter().collect()), line: tl });
            continue;
        }
        if c == '\'' {
            i += 1;
            let mut a = String::new();
            while i < s.len() && s[i] != '\'' {
                if s[i] == '\\' {
                    let (ch, ni) = escape(&s, i + 1)?;
                    a.push(char::from_u32(ch).unwrap_or('?'));
                    i = ni;
                } else {
                    if s[i] == '\n' {
                        line += 1;
                    }
                    a.push(s[i]);
                    i += 1;
                }
            }
            i += 1;
            out.push(Tok { t: T::Atom(a), line: tl });
            continue;
        }
        if c == '"' {
            // triple-quoted string (OTP 27)
            if i + 2 < s.len() && s[i + 1] == '"' && s[i + 2] == '"' {
                let mut j = i + 3;
                while j < s.len() && s[j] != '\n' {
                    j += 1;
                }
                // find closing line: optional whitespace then """
                let mut k = j;
                let body_start = j + 1;
                let mut end_pos = None;
                while k < s.len() {
                    if s[k] == '\n' {
                        let mut m = k + 1;
                        while m < s.len() && (s[m] == ' ' || s[m] == '\t') {
                            m += 1;
                        }
                        if m + 2 < s.len() + 0 && m + 2 <= s.len() - 1 && s[m] == '"' && s[m + 1] == '"' && s[m + 2] == '"' {
                            end_pos = Some((k, m - (k + 1), m + 3));
                            break;
                        }
                    }
                    k += 1;
                }
                let (body_end, indent, after) = match end_pos {
                    Some(x) => x,
                    None => return err(tl, "unterminated triple-quoted string"),
                };
                let body: String = if body_start <= body_end { s[body_start..body_end].iter().collect() } else { String::new() };
                let mut cps = Vec::new();
                for (n, l) in body.split('\n').enumerate() {
                    if n > 0 {
                        cps.push('\n' as u32);
                    }
                    let l: String = l.chars().skip(indent).collect();
                    cps.extend(l.chars().map(|c| c as u32));
                }
                for ch in &s[i..after] {
                    if *ch == '\n' {
                        line += 1;
                    }
                }
                i = after;
                out.push(Tok { t: T::Str(cps), line: tl });
                continue;
            }
            i += 1;
            let mut cps = Vec::new();
            while i < s.len() && s[i] != '"' {
                if s[i] == '\\' {
                    let (ch, ni) = escape(&s, i + 1)?;
                    cps.push(ch);
                    i = ni;
                } else {
                    if s[i] == '\n' {
                        line += 1;
                    }
                    cps.push(s[i] as u32);
                    i += 1;
                }
            }
            i += 1;
            out.push(Tok { t: T::Str(cps), line: tl });
            continue;
        }
        if c == '~' {
            // sigil (OTP 27): ~"..." ~s"..." ~b"..." ~B[...] etc.
            let mut j = i + 1;
            let mut verbatim = false;
            if j < s.len() && s[j].is_ascii_alphabetic() {
                verbatim = s[j].is_ascii_uppercase();
                j += 1;
            }
            if j < s.len() && s[j] == '"' && j + 2 < s.len() && s[j + 1] == '"' && s[j + 2] == '"' {
                // ~"""...""" : reuse triple-quote lexing by recursion on the rest
                let rest: String = s[j..].iter().collect();
                let sub = tokenize(&rest, file)?;
                if let Some(Tok { t: T::Str(cps), .. }) = sub.first() {
                    // advance past the closing """ by counting chars consumed
                    let mut k = j + 3;
                    let mut found = false;
                    while k + 2 < s.len() {
                        if s[k] == '\n' {
                            let mut m = k + 1;
                            while m < s.len() && (s[m] == ' ' || s[m] == '\t') {
                                m += 1;
                            }
                            if m + 2 < s.len() && s[m] == '"' && s[m + 1] == '"' && s[m + 2] == '"' {
                                for ch in &s[i..m + 3] {
                                    if *ch == '\n' {
                                        line += 1;
                                    }
                                }
                                i = m + 3;
                                found = true;
                                break;
                            }
                        }
                        k += 1;
                    }
                    if !found {
                        return err(tl, "unterminated sigil");
                    }
                    out.push(Tok { t: T::Str(cps.clone()), line: tl });
                    continue;
                }
            }
            if j < s.len() {
                let open = s[j];
                let close = match open {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    '<' => '>',
                    c => c,
                };
                j += 1;
                let mut cps = Vec::new();
                while j < s.len() && s[j] != close {
                    if s[j] == '\\' && !verbatim {
                        let (ch, nj) = escape(&s, j + 1)?;
                        cps.push(ch);
                        j = nj;
                    } else {
                        if s[j] == '\n' {
                            line += 1;
                        }
                        cps.push(s[j] as u32);
                        j += 1;
                    }
                }
                i = j + 1;
                out.push(Tok { t: T::Str(cps), line: tl });
                continue;
            }
        }
        // punctuation
        let mut matched = false;
        for p in PUNCT {
            let pc: Vec<char> = p.chars().collect();
            if i + pc.len() <= s.len() && s[i..i + pc.len()] == pc[..] {
                if *p == "." {
                    // form end: followed by whitespace, % or EOF
                    let nx = s.get(i + 1).copied();
                    if nx.is_none() || nx.map(|c| c.is_whitespace() || c == '%').unwrap_or(false) {
                        out.push(Tok { t: T::Dot, line: tl });
                        i += 1;
                        matched = true;
                        break;
                    }
                }
                out.push(Tok { t: T::P(p.to_string()), line: tl });
                i += pc.len();
                matched = true;
                break;
            }
        }
        if !matched {
            return err(tl, &format!("unexpected character {:?}", c));
        }
    }
    out.push(Tok { t: T::Eof, line });
    Ok(out)
}

/// Escape sequence after a backslash at `i`; returns (codepoint, next index).
fn escape(s: &[char], i: usize) -> Result<(u32, usize), String> {
    if i >= s.len() {
        return Err("bad escape".into());
    }
    let c = s[i];
    Ok(match c {
        'n' => (10, i + 1),
        't' => (9, i + 1),
        'r' => (13, i + 1),
        's' => (32, i + 1),
        'e' => (27, i + 1),
        'd' => (127, i + 1),
        'b' => (8, i + 1),
        'f' => (12, i + 1),
        'v' => (11, i + 1),
        '0'..='7' => {
            let mut j = i;
            let mut v = 0u32;
            while j < s.len() && j < i + 3 && ('0'..='7').contains(&s[j]) {
                v = v * 8 + (s[j] as u32 - '0' as u32);
                j += 1;
            }
            (v, j)
        }
        'x' => {
            if i + 1 < s.len() && s[i + 1] == '{' {
                let mut j = i + 2;
                let mut h = String::new();
                while j < s.len() && s[j] != '}' {
                    h.push(s[j]);
                    j += 1;
                }
                (u32::from_str_radix(&h, 16).unwrap_or(0), j + 1)
            } else {
                let h: String = s[i + 1..(i + 3).min(s.len())].iter().collect();
                (u32::from_str_radix(&h, 16).unwrap_or(0), i + 3)
            }
        }
        '^' => {
            let ch = s.get(i + 1).copied().unwrap_or('@') as u32;
            (ch & 31, i + 2)
        }
        c => (c as u32, i + 1),
    })
}
