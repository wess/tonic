//! The BEAM atom table at script start (Elixir 1.18 / OTP 27), dumped by
//! `tools/dump_atoms.exs`. Atom-keyed maps on the BEAM iterate (and print) in
//! atom-table order, so tonic gives each atom the index it would have there:
//! atoms that already exist when a script starts keep their BEAM index; atoms
//! the script introduces follow in the order the tokenizer meets them.

use std::collections::HashMap;
use std::sync::OnceLock;

static DATA: &str = include_str!("../data/beam_atoms.txt");

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn table() -> &'static HashMap<String, u32> {
    static T: OnceLock<HashMap<String, u32>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m = HashMap::new();
        for (i, l) in DATA.lines().enumerate() {
            m.entry(unescape(l)).or_insert(i as u32);
        }
        m
    })
}

/// Number of atoms that exist before a script's own atoms are created.
pub fn boot_count() -> u32 {
    DATA.lines().count() as u32
}

/// BEAM index of an atom that exists at boot.
pub fn boot_index(name: &str) -> Option<u32> {
    table().get(name).copied()
}

/// Assigns BEAM indices: boot atoms keep theirs; `script_order` (atoms in the
/// order the script creates them) come next; anything else after that.
pub struct Assigner {
    extra: HashMap<String, u32>,
    next: u32,
}

impl Assigner {
    pub fn new(script_order: &[String]) -> Assigner {
        let mut a = Assigner { extra: HashMap::new(), next: boot_count() };
        for s in script_order {
            a.index(s);
        }
        a
    }

    pub fn index(&mut self, name: &str) -> u32 {
        if let Some(i) = boot_index(name) {
            return i;
        }
        if let Some(&i) = self.extra.get(name) {
            return i;
        }
        let i = self.next;
        self.next += 1;
        self.extra.insert(name.to_string(), i);
        i
    }
}
