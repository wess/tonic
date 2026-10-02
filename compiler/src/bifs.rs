//! Table of functions implemented natively by the runtime.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub struct Bif {
    pub sym: &'static str,
    /// Allowed in guards.
    pub guard: bool,
    /// Cannot raise (no exception check needed).
    pub nofail: bool,
}

// (module, name, arity, symbol, flags) — flags: 'G' guard-safe, 'N' never fails
const TABLE: &[(&str, &str, usize, &str, &str)] = &[
    // arithmetic / comparison
    ("erlang", "+", 2, "tn_add", "G"),
    ("erlang", "-", 2, "tn_sub", "G"),
    ("erlang", "*", 2, "tn_mul", "G"),
    ("erlang", "/", 2, "tn_fdiv", "G"),
    ("erlang", "-", 1, "tn_neg", "G"),
    ("erlang", "+", 1, "tn_pos", "G"),
    ("erlang", "div", 2, "tn_div", "G"),
    ("erlang", "rem", 2, "tn_rem", "G"),
    ("erlang", "abs", 1, "tn_abs", "G"),
    ("erlang", "==", 2, "tn_eq", "GN"),
    ("erlang", "/=", 2, "tn_ne", "GN"),
    ("erlang", "=:=", 2, "tn_eqx", "GN"),
    ("erlang", "=/=", 2, "tn_nex", "GN"),
    ("erlang", "<", 2, "tn_lt", "GN"),
    ("erlang", ">", 2, "tn_gt", "GN"),
    ("erlang", "=<", 2, "tn_le", "GN"),
    ("erlang", ">=", 2, "tn_ge", "GN"),
    ("erlang", "not", 1, "tn_not", "G"),
    ("erlang", "band", 2, "tn_band", "G"),
    ("erlang", "bor", 2, "tn_bor", "G"),
    ("erlang", "bxor", 2, "tn_bxor", "G"),
    ("erlang", "bsl", 2, "tn_bsl", "G"),
    ("erlang", "bsr", 2, "tn_bsr", "G"),
    ("erlang", "bnot", 1, "tn_bnot", "G"),
    ("erlang", "max", 2, "tn_max", "GN"),
    ("erlang", "min", 2, "tn_min", "GN"),
    // type tests
    ("erlang", "is_atom", 1, "tn_is_atom", "GN"),
    ("erlang", "is_binary", 1, "tn_is_binary", "GN"),
    ("erlang", "is_bitstring", 1, "tn_is_bitstring", "GN"),
    ("erlang", "is_boolean", 1, "tn_is_boolean", "GN"),
    ("erlang", "is_float", 1, "tn_is_float", "GN"),
    ("erlang", "is_function", 1, "tn_is_function", "GN"),
    ("erlang", "is_function", 2, "tn_is_function2", "G"),
    ("erlang", "is_integer", 1, "tn_is_integer", "GN"),
    ("erlang", "is_list", 1, "tn_is_list", "GN"),
    ("erlang", "is_map", 1, "tn_is_map", "GN"),
    ("erlang", "is_number", 1, "tn_is_number", "GN"),
    ("erlang", "is_pid", 1, "tn_is_pid", "GN"),
    ("erlang", "is_port", 1, "tn_is_port", "GN"),
    ("erlang", "is_reference", 1, "tn_is_reference", "GN"),
    ("erlang", "is_tuple", 1, "tn_is_tuple", "GN"),
    ("erlang", "is_map_key", 2, "tn_is_map_key_erl", "G"),
    ("erlang", "map_get", 2, "tn_map_get", "G"),
    // data
    ("erlang", "hd", 1, "tn_hd", "G"),
    ("erlang", "tl", 1, "tn_tl", "G"),
    ("erlang", "length", 1, "tn_length", "G"),
    ("erlang", "element", 2, "tn_element", "G"),
    ("erlang", "setelement", 3, "tn_setelement", ""),
    ("erlang", "tuple_size", 1, "tn_tuple_size", "G"),
    ("erlang", "map_size", 1, "tn_map_size", "G"),
    ("erlang", "byte_size", 1, "tn_byte_size", "G"),
    ("erlang", "bit_size", 1, "tn_bit_size", "G"),
    ("erlang", "binary_part", 3, "tn_binary_part", "G"),
    ("erlang", "trunc", 1, "tn_trunc", "G"),
    ("erlang", "round", 1, "tn_round", "G"),
    ("erlang", "ceil", 1, "tn_ceil", "G"),
    ("erlang", "floor", 1, "tn_floor", "G"),
    ("erlang", "float", 1, "tn_to_float", "G"),
    ("erlang", "self", 0, "tn_self", "GN"),
    ("erlang", "node", 0, "tn_node", "GN"),
    ("erlang", "node", 1, "tn_node1", "G"),
    ("erlang", "make_ref", 0, "tn_make_ref", ""),
    ("erlang", "tuple_to_list", 1, "tn_tuple_to_list", ""),
    ("erlang", "list_to_tuple", 1, "tn_list_to_tuple", ""),
    ("erlang", "make_tuple", 2, "tn_make_tuple", ""),
    ("erlang", "append_element", 2, "tn_tuple_append", ""),
    ("erlang", "insert_element", 3, "tn_insert_element_erl", ""),
    ("erlang", "delete_element", 2, "tn_delete_element_erl", ""),
    ("erlang", "atom_to_binary", 1, "tn_atom_to_binary", ""),
    ("erlang", "binary_to_atom", 1, "tn_binary_to_atom", ""),
    ("erlang", "binary_to_existing_atom", 1, "tn_binary_to_existing_atom", ""),
    ("erlang", "iolist_to_binary", 1, "tn_iodata_to_binary", ""),
    ("erlang", "list_to_binary", 1, "tn_iodata_to_binary", ""),
    ("erlang", "iolist_size", 1, "tn_iodata_length", ""),
    ("erlang", "binary_to_list", 1, "tn_bin_to_list", ""),
    ("erlang", "float_to_binary", 1, "tn_float_to_binary", ""),
    ("erlang", "binary_to_float", 1, "tn_binary_to_float", ""),
    ("erlang", "phash2", 1, "tn_phash2_1", ""),
    ("erlang", "phash2", 2, "tn_phash2", ""),
    ("erlang", "++", 2, "tn_append", ""),
    ("erlang", "--", 2, "tn_subtract", ""),
    ("erlang", "throw", 1, "tn_throw", ""),
    ("erlang", "exit", 1, "tn_exit", ""),
    ("erlang", "exit", 2, "tn_exit2", ""),
    ("erlang", "error", 1, "tn_error", ""),
    ("erlang", "error", 2, "tn_error2", ""),
    ("erlang", "error", 3, "tn_error3", ""),
    ("erlang", "raise", 3, "tn_raise3", ""),
    ("erlang", "apply", 2, "tn_apply_fun", ""),
    ("erlang", "apply", 3, "tn_apply_mfa", ""),
    ("erlang", "send", 2, "tn_send", ""),
    ("erlang", "spawn", 1, "tn_spawn", ""),
    ("erlang", "spawn_link", 1, "tn_spawn_link", ""),
    ("erlang", "spawn_monitor", 1, "tn_spawn_monitor", ""),
    ("erlang", "link", 1, "tn_link", ""),
    ("erlang", "unlink", 1, "tn_unlink", ""),
    ("erlang", "process_flag", 2, "tn_process_flag", ""),
    ("erlang", "register", 2, "tn_register_erl", ""),
    ("erlang", "unregister", 1, "tn_unregister", ""),
    ("erlang", "whereis", 1, "tn_whereis_erl", ""),
    ("erlang", "registered", 0, "tn_registered", ""),
    ("erlang", "is_process_alive", 1, "tn_is_alive", ""),
    ("erlang", "send_after", 3, "tn_send_after_erl", ""),
    ("erlang", "cancel_timer", 1, "tn_cancel_timer", ""),
    ("erlang", "get", 1, "tn_dict_get1", ""),
    ("erlang", "put", 2, "tn_dict_put", ""),
    ("erlang", "erase", 1, "tn_dict_delete", ""),
    ("erlang", "get", 0, "tn_dict_all", ""),
    ("erlang", "monotonic_time", 1, "tn_monotonic_time", ""),
    ("erlang", "system_time", 1, "tn_system_time", ""),
    ("erlang", "garbage_collect", 0, "tn_gc", ""),
    ("erlang", "function_exported", 3, "tn_function_exported", ""),
    ("erlang", "integer_to_binary", 1, "tn_integer_to_binary10", ""),
    ("erlang", "integer_to_binary", 2, "tn_integer_to_binary", ""),
    ("erlang", "binary_to_integer", 1, "tn_binary_to_integer10", ""),
    ("erlang", "binary_to_integer", 2, "tn_binary_to_integer", ""),
    ("erlang", "integer_to_list", 1, "tn_integer_to_list10", ""),
    ("erlang", "integer_to_list", 2, "tn_integer_to_list", ""),
    ("erlang", "atom_to_list", 1, "tn_atom_to_list", ""),
    ("erlang", "list_to_atom", 1, "tn_list_to_atom", ""),
    ("erlang", "list_to_integer", 1, "tn_list_to_integer", ""),
    ("erlang", "demonitor", 1, "tn_demonitor", ""),
    // lists / maps / binary
    ("lists", "reverse", 1, "tn_reverse", ""),
    ("lists", "reverse", 2, "tn_reverse2", ""),
    ("lists", "member", 2, "tn_lists_member", ""),
    ("lists", "keyfind", 3, "tn_keyfind", ""),
    ("lists", "sort", 1, "tn_sort", ""),
    ("lists", "last", 1, "tn_list_last", ""),
    ("lists", "seq", 3, "tn_seq", ""),
    ("maps", "get", 2, "tn_map_get_erl", ""),
    ("maps", "get", 3, "tn_map_get3_erl", ""),
    ("maps", "find", 2, "tn_map_fetch_erl", ""),
    ("maps", "put", 3, "tn_map_put_erl", ""),
    ("maps", "remove", 2, "tn_map_remove_erl", ""),
    ("maps", "merge", 2, "tn_map_merge", ""),
    ("maps", "keys", 1, "tn_map_keys", ""),
    ("maps", "values", 1, "tn_map_values", ""),
    ("maps", "to_list", 1, "tn_map_to_list", ""),
    ("maps", "from_list", 1, "tn_map_from_list", ""),
    ("maps", "is_key", 2, "tn_is_map_key_erl", ""),
    ("maps", "size", 1, "tn_map_size", ""),
    ("binary", "part", 3, "tn_binary_part", ""),
    ("binary", "at", 2, "tn_binary_at", ""),
    ("binary", "copy", 2, "tn_binary_copy", ""),
    ("binary", "bin_to_list", 1, "tn_bin_to_list", ""),
    ("binary", "match", 2, "tn_binary_match", ""),
    ("binary", "matches", 2, "tn_binary_matches", ""),
    ("math", "sqrt", 1, "tn_math_sqrt", ""),
    ("math", "pow", 2, "tn_math_pow", ""),
    ("math", "pi", 0, "tn_math_pi", "N"),
    // Kernel (auto-imported)
    ("Elixir.Kernel", "+", 2, "tn_add", "G"),
    ("Elixir.Kernel", "-", 2, "tn_sub", "G"),
    ("Elixir.Kernel", "*", 2, "tn_mul", "G"),
    ("Elixir.Kernel", "/", 2, "tn_fdiv", "G"),
    ("Elixir.Kernel", "-", 1, "tn_neg", "G"),
    ("Elixir.Kernel", "+", 1, "tn_pos", "G"),
    ("Elixir.Kernel", "==", 2, "tn_eq", "GN"),
    ("Elixir.Kernel", "!=", 2, "tn_ne", "GN"),
    ("Elixir.Kernel", "===", 2, "tn_eqx", "GN"),
    ("Elixir.Kernel", "!==", 2, "tn_nex", "GN"),
    ("Elixir.Kernel", "<", 2, "tn_lt", "GN"),
    ("Elixir.Kernel", ">", 2, "tn_gt", "GN"),
    ("Elixir.Kernel", "<=", 2, "tn_le", "GN"),
    ("Elixir.Kernel", ">=", 2, "tn_ge", "GN"),
    ("Elixir.Kernel", "++", 2, "tn_append", ""),
    ("Elixir.Kernel", "--", 2, "tn_subtract", ""),
    ("Elixir.Kernel", "<>", 2, "tn_bin_concat", ""),
    ("Elixir.Kernel", "abs", 1, "tn_abs", "G"),
    ("Elixir.Kernel", "div", 2, "tn_div", "G"),
    ("Elixir.Kernel", "rem", 2, "tn_rem", "G"),
    ("Elixir.Kernel", "hd", 1, "tn_hd", "G"),
    ("Elixir.Kernel", "tl", 1, "tn_tl", "G"),
    ("Elixir.Kernel", "length", 1, "tn_length", "G"),
    ("Elixir.Kernel", "elem", 2, "tn_elem", "G"),
    ("Elixir.Kernel", "put_elem", 3, "tn_put_elem", ""),
    ("Elixir.Kernel", "tuple_size", 1, "tn_tuple_size", "G"),
    ("Elixir.Kernel", "map_size", 1, "tn_map_size", "G"),
    ("Elixir.Kernel", "byte_size", 1, "tn_byte_size", "G"),
    ("Elixir.Kernel", "bit_size", 1, "tn_bit_size", "G"),
    ("Elixir.Kernel", "binary_part", 3, "tn_binary_part", "G"),
    ("Elixir.Kernel", "trunc", 1, "tn_trunc", "G"),
    ("Elixir.Kernel", "round", 1, "tn_round", "G"),
    ("Elixir.Kernel", "ceil", 1, "tn_ceil", "G"),
    ("Elixir.Kernel", "floor", 1, "tn_floor", "G"),
    ("Elixir.Kernel", "max", 2, "tn_max", "GN"),
    ("Elixir.Kernel", "min", 2, "tn_min", "GN"),
    ("Elixir.Kernel", "self", 0, "tn_self", "GN"),
    ("Elixir.Kernel", "node", 0, "tn_node", "GN"),
    ("Elixir.Kernel", "node", 1, "tn_node1", "G"),
    ("Elixir.Kernel", "make_ref", 0, "tn_make_ref", ""),
    ("Elixir.Kernel", "is_atom", 1, "tn_is_atom", "GN"),
    ("Elixir.Kernel", "is_binary", 1, "tn_is_binary", "GN"),
    ("Elixir.Kernel", "is_bitstring", 1, "tn_is_bitstring", "GN"),
    ("Elixir.Kernel", "is_boolean", 1, "tn_is_boolean", "GN"),
    ("Elixir.Kernel", "is_float", 1, "tn_is_float", "GN"),
    ("Elixir.Kernel", "is_function", 1, "tn_is_function", "GN"),
    ("Elixir.Kernel", "is_function", 2, "tn_is_function2", "G"),
    ("Elixir.Kernel", "is_integer", 1, "tn_is_integer", "GN"),
    ("Elixir.Kernel", "is_list", 1, "tn_is_list", "GN"),
    ("Elixir.Kernel", "is_map", 1, "tn_is_map", "GN"),
    ("Elixir.Kernel", "is_number", 1, "tn_is_number", "GN"),
    ("Elixir.Kernel", "is_pid", 1, "tn_is_pid", "GN"),
    ("Elixir.Kernel", "is_port", 1, "tn_is_port", "GN"),
    ("Elixir.Kernel", "is_reference", 1, "tn_is_reference", "GN"),
    ("Elixir.Kernel", "is_tuple", 1, "tn_is_tuple", "GN"),
    ("Elixir.Kernel", "is_map_key", 2, "tn_is_map_key", "G"),
    ("Elixir.Kernel", "is_struct", 1, "tn_is_struct", "GN"),
    ("Elixir.Kernel", "is_struct", 2, "tn_is_struct2", "G"),
    ("Elixir.Kernel", "is_non_struct_map", 1, "tn_is_non_struct_map", "GN"),
    ("Elixir.Kernel", "is_exception", 1, "tn_is_exception", "GN"),
    ("Elixir.Kernel", "is_exception", 2, "tn_is_exception2", "G"),
    ("Elixir.Kernel", "send", 2, "tn_send", ""),
    ("Elixir.Kernel", "spawn", 1, "tn_spawn", ""),
    ("Elixir.Kernel", "spawn_link", 1, "tn_spawn_link", ""),
    ("Elixir.Kernel", "spawn_monitor", 1, "tn_spawn_monitor", ""),
    ("Elixir.Kernel", "throw", 1, "tn_throw", ""),
    ("Elixir.Kernel", "exit", 1, "tn_exit", ""),
    ("Elixir.Kernel", "apply", 2, "tn_apply_fun", ""),
    ("Elixir.Kernel", "apply", 3, "tn_apply_mfa", ""),
    ("Elixir.Kernel", "function_exported?", 3, "tn_function_exported", ""),
    ("Elixir.Kernel", "not", 1, "tn_not", "G"),
];

pub fn table() -> &'static HashMap<(String, String, usize), Bif> {
    static T: OnceLock<HashMap<(String, String, usize), Bif>> = OnceLock::new();
    T.get_or_init(|| {
        let mut m = HashMap::new();
        for &(md, n, a, sym, flags) in TABLE {
            m.insert(
                (md.to_string(), n.to_string(), a),
                Bif {
                    sym,
                    guard: flags.contains('G'),
                    nofail: flags.contains('N'),
                },
            );
        }
        m
    })
}

/// All (module, name, arity, symbol) BIF entries.
pub fn entries() -> impl Iterator<Item = (&'static str, &'static str, usize, &'static str)> {
    TABLE.iter().map(|(m, n, a, s, _)| (*m, *n, *a, *s))
}

pub fn lookup(module: &str, name: &str, arity: usize) -> Option<Bif> {
    table()
        .get(&(module.to_string(), name.to_string(), arity))
        .copied()
}

/// Runtime `tn_*` functions discovered from the runtime sources, with their
/// arity (excluding ctx). Used for `:tonic.name(...)` calls from the prelude.
pub fn runtime_syms() -> &'static HashMap<String, usize> {
    static S: OnceLock<HashMap<String, usize>> = OnceLock::new();
    S.get_or_init(|| {
        let srcs = [
            include_str!("../../runtime/src/bif_core.rs"),
            include_str!("../../runtime/src/bif_bin.rs"),
            include_str!("../../runtime/src/bif_proc.rs"),
            include_str!("../../runtime/src/bif_misc.rs"),
            include_str!("../../runtime/src/bif_extra.rs"),
            include_str!("../../runtime/src/bif_more.rs"),
            include_str!("../../runtime/src/ets.rs"),
            include_str!("../../runtime/src/bif_file.rs"),
            include_str!("../../runtime/src/re.rs"),
            include_str!("../../runtime/src/ustr.rs"),
        ];
        let mut m = HashMap::new();
        for src in srcs {
            for (mac, ar) in [("arith2!(", 2usize), ("bitop!(", 2), ("typetest!(", 1)] {
                let mut rest = src;
                while let Some(i) = rest.find(mac) {
                    rest = &rest[i + mac.len()..];
                    let end = rest.find(',').unwrap_or(0);
                    let name = rest[..end].trim().to_string();
                    if name.starts_with("tn_") {
                        m.insert(name, ar);
                    }
                }
            }
            let mut rest = src;
            while let Some(i) = rest.find("pub extern \"C\" fn tn_") {
                rest = &rest[i + "pub extern \"C\" fn ".len()..];
                let name_end = rest.find('(').unwrap();
                let name = rest[..name_end].to_string();
                let close = rest.find(')').unwrap();
                let params = &rest[name_end + 1..close];
                let n = params.split(',').filter(|p| !p.trim().is_empty()).count();
                // Only term-only signatures are callable from Elixir code.
                if params.contains("*const") || params.contains(": u64") {
                    rest = &rest[close..];
                    continue;
                }
                m.insert(name, n.saturating_sub(1));
                rest = &rest[close..];
            }
        }
        m
    })
}

/// Guard-safe runtime symbols (for `:tonic.*` calls in guards).
pub fn guard_sym(sym: &str) -> bool {
    table().values().any(|b| b.sym == sym && b.guard)
}

/// The Erlang function a runtime symbol implements (for the BEAM-style
/// `{:erlang, :fun, args, ...}` top stack frame on failure).
pub fn erl_origin(sym: &str) -> Option<(&'static str, &'static str)> {
    // Kernel.elem/2 is :erlang.element/2 (tn_bif_frame reorders the args).
    if sym == "tn_elem" {
        return Some(("erlang", "element"));
    }
    TABLE
        .iter()
        .find(|(m, n, _, s, _)| {
            *s == sym
                && !m.starts_with("Elixir.")
                && *m != "tonic"
                && !matches!(*n, "error" | "exit" | "throw" | "raise" | "nif_error" | "halt" | "apply")
        })
        .map(|(m, n, _, _, _)| (*m, *n))
}

pub fn nofail_sym(sym: &str) -> bool {
    table().values().any(|b| b.sym == sym && b.nofail)
        || matches!(
            sym,
            "tn_exc_module" | "tn_map_find" | "tn_bs_size" | "tn_bs_int" | "tn_bs_bin" | "tn_bs_rest" | "tn_bs_utf8"
                | "tn_bs_lit" | "tn_bs_float" | "tn_impl_for" | "tn_builtin_type"
                | "tn_str_printable" | "tn_list_ascii_printable" | "tn_whereis" | "tn_self"
        )
}
