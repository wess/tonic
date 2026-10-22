use tonic_core::sched;
use tonic_core::term::atom;

#[test]
fn export_metadata_is_available_without_a_callable_pointer() {
    for name in tonic_shared::FIXED_ATOMS {
        tonic_core::atoms::intern(name);
    }
    let module = atom(tonic_core::atoms::intern("Elixir.ExportMetadata"));
    let pruned = atom(tonic_core::atoms::intern("pruned"));
    let retained = atom(tonic_core::atoms::intern("retained"));
    sched::register_funs(&[(module, pruned, 3, 0), (module, retained, 1, 1)]);
    assert!(sched::module_exists(module));
    assert!(sched::function_exported(module, pruned, 3));
    assert!(!sched::function_exported(module, pruned, 2));
    assert_eq!(sched::lookup_fun(module, pruned, 3), None);
    assert_eq!(sched::lookup_fun(module, retained, 1), Some(1));
    sched::register_funs(&[(module, retained, 1, 0)]);
    assert_eq!(sched::lookup_fun(module, retained, 1), Some(1));
}
