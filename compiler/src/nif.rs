//! Modules backed by NIF libraries linked into the program.
//!
//! A module's `@on_load` callback runs when the program starts. Inside such a
//! module, `:erlang.load_nif/2` binds the library registered for the module,
//! and functions whose body is `:erlang.nif_error/1` become calls into it.

use crate::ast::{E, K};
use crate::collect::{map_expr, Collector};

/// `Tonic.Internal.on_load(module, callback)`, run where the module is defined.
pub fn on_load_call(module: &str, callback: &str, line: u32) -> E {
    E::new(
        K::Remote {
            recv: Box::new(E::atom("Elixir.Tonic.Internal", line)),
            name: "on_load".into(),
            args: vec![E::atom(module, line), E::atom(callback, line)],
            parens: true,
        },
        line,
    )
}

fn erlang_call<'a>(e: &'a E, name: &str, arity: usize) -> Option<&'a [E]> {
    match &e.k {
        K::Remote { recv, name: n, args, .. }
            if n == name && args.len() == arity && matches!(&recv.k, K::Atom(m) if m == "erlang") =>
        {
            Some(args)
        }
        _ => None,
    }
}

fn tonic_call(name: &str, args: Vec<E>, line: u32) -> E {
    E::new(
        K::Remote { recv: Box::new(E::atom("tonic", line)), name: name.into(), args, parens: true },
        line,
    )
}

impl Collector {
    pub fn bind_nifs(&mut self) {
        for module in self.on_load.clone() {
            let Some(m) = self.mods.get_mut(&module) else { continue };
            for def in m.defs.iter_mut().filter(|d| !d.is_macro) {
                let Some(body) = &def.body else { continue };
                let line = def.line;
                if let Some(reason) = erlang_call(body, "nif_error", 1).map(|a| a[0].clone()) {
                    let params: Vec<E> = (0..def.arity).map(|i| E::var(&format!("tonic__nif{}", i), line)).collect();
                    def.body = Some(tonic_call(
                        "nif_call",
                        vec![
                            E::atom(&module, line),
                            E::atom(&def.name, line),
                            E::new(K::List(params.clone(), None), line),
                            reason,
                        ],
                        line,
                    ));
                    def.args = params;
                    def.guard = None;
                    continue;
                }
                def.body = Some(map_expr(body, &mut |e| {
                    erlang_call(e, "load_nif", 2).map(|args| {
                        tonic_call("nif_load", vec![E::atom(&module, e.line), args[0].clone(), args[1].clone()], e.line)
                    })
                }));
            }
        }
    }
}
