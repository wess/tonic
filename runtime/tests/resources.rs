use std::collections::{HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tonic_core::heap::{Ctx, MIN_HEAP_WORDS};
use tonic_core::nif::{self, Env};
use tonic_core::sched::{ProcShared, ProcState};
use tonic_core::term::*;

fn context() -> Box<Ctx> {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        for (i, name) in tonic_shared::FIXED_ATOMS.iter().enumerate() {
            assert_eq!(tonic_core::atoms::intern(name), i as u64);
        }
    });
    Ctx::new(MIN_HEAP_WORDS)
}

struct Tracked(Arc<AtomicUsize>);

impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn shared(pid: u64) -> ProcShared {
    ProcShared {
        pid,
        inbox: parking_lot::Mutex::new(VecDeque::new()),
        notify: tokio::sync::Notify::new(),
        has_signal: AtomicBool::new(false),
        st: parking_lot::Mutex::new(ProcState {
            links: HashSet::new(),
            monitored_by: Vec::new(),
            trap_exit: false,
            alive: true,
            doomed: false,
            name: None,
            priority: 0,
            initial_call: (0, 0, 0),
            parent: None,
        }),
    }
}

fn collect_all(ctx: &mut Ctx) {
    // fill the old generation too, forcing a major after any minor collections
    for _ in 0..8 {
        ctx.gc(0);
        let n = ctx.old_words + ctx.heap_words + 64;
        ctx.reserve(n);
        let p = ctx.alloc_nogc(n);
        unsafe {
            *p = header(T_BINARY, (n - 1) as u64);
            *p.add(1) = 0;
        }
    }
    ctx.gc(0);
}

#[test]
fn gc_reclaims_unreachable_refs_and_preserves_promoted_live_refs() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut ctx = context();
    let r = ctx.managed_ref(Tracked(drops.clone()));
    ctx.push(r);
    collect_all(&mut ctx);
    assert!(tonic_core::resource::get::<Tracked>(ctx.get(0)).is_some());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    ctx.truncate(0);
    collect_all(&mut ctx);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(ctx);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn copied_messages_outlive_sender_and_release_after_last_receiver() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut sender = context();
    let r = sender.managed_ref(Tracked(drops.clone()));
    let message = sender.tuple(&[r, r]);
    let queued = sender.to_fragment(message);
    let forwarded = tonic_core::sched::clone_fragment(&queued);
    drop(sender);
    drop(queued);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let mut receiver = context();
    let message = receiver.from_fragment(&forwarded);
    receiver.push(message);
    drop(forwarded);
    collect_all(&mut receiver);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(receiver);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn ets_retains_resources_until_objects_are_deleted() {
    let process = shared(8101);
    let mut ctx = context();
    ctx.shared = &process;
    let name = atom(tonic_core::atoms::intern("resource_test"));
    let tab = tonic_core::ets::tn_ets_new(&mut *ctx, name, NIL_LIST);
    ctx.push(tab);
    let drops = Arc::new(AtomicUsize::new(0));
    let r = ctx.managed_ref(Tracked(drops.clone()));
    let object = ctx.tuple(&[small(1), r]);
    tonic_core::ets::tn_ets_insert(&mut *ctx, ctx.get(0), object);
    collect_all(&mut ctx);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let found = tonic_core::ets::tn_ets_lookup(&mut *ctx, ctx.get(0), small(1));
    assert!(tonic_core::resource::get::<Tracked>(tuple_get(head(found), 1)).is_some());
    tonic_core::ets::tn_ets_delete_table(&mut *ctx, ctx.get(0));
    collect_all(&mut ctx);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

unsafe extern "C" fn destroy(_env: *mut Env, obj: *mut c_void) {
    std::ptr::drop_in_place(obj as *mut Tracked);
}

#[test]
fn nif_destructor_runs_once_after_envs_copies_and_native_refs_release() {
    let drops = Arc::new(AtomicUsize::new(0));
    unsafe {
        let mut env = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
        let kind = nif::enif_open_resource_type(
            &mut env,
            std::ptr::null(),
            std::ptr::null(),
            Some(destroy),
            1,
            std::ptr::null_mut(),
        );
        let obj = nif::enif_alloc_resource(kind, std::mem::size_of::<Tracked>());
        (obj as *mut Tracked).write(Tracked(drops.clone()));
        let first = nif::enif_make_resource(&mut env, obj);
        let second = nif::enif_make_resource(&mut env, obj);
        let tuple = env.tuple(&[first, second]);
        let exported = env.export(tuple);
        nif::enif_keep_resource(obj);
        nif::enif_release_resource(obj);
        drop(env);
        let mut ctx = context();
        let copied = ctx.from_fragment(&exported);
        ctx.push(copied);
        drop(exported);
        collect_all(&mut ctx);
        let mut found = std::ptr::null_mut();
        assert_eq!(
            nif::enif_get_resource(
                std::ptr::null_mut(),
                tuple_get(ctx.get(0), 0),
                kind,
                &mut found
            ),
            1
        );
        assert_eq!(found, obj);
        let id = word(tuple_get(ctx.get(0), 0), 1);
        drop(ctx);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        let mut restored = context();
        let term = restored.ref_from_id(id);
        assert_eq!(nif::enif_get_resource(std::ptr::null_mut(), term, kind, &mut found), 1);
        assert_eq!(found, obj);
        drop(restored);
        nif::enif_release_resource(obj);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        drop(Box::from_raw(kind));
    }
}

#[test]
fn atomics_and_zlib_handles_survive_copies_and_close_is_shared() {
    let process = shared(8102);
    let mut ctx = context();
    ctx.shared = &process;
    let r = tonic_core::atomics::tn_atomics_new(&mut *ctx, small(2), atom(1));
    ctx.push(r);
    tonic_core::atomics::tn_atomics_put(&mut *ctx, ctx.get(0), small(1), small(42));
    let copy = ctx.to_fragment(ctx.get(0));
    ctx.truncate(0);
    collect_all(&mut ctx);
    let r = ctx.from_fragment(&copy);
    ctx.push(r);
    assert_eq!(
        tonic_core::atomics::tn_atomics_get(&mut *ctx, ctx.get(0), small(1)),
        small(42)
    );
    let stream = tonic_core::zlib::tn_zlib_open(&mut *ctx);
    ctx.push(stream);
    let copy = ctx.to_fragment(ctx.get(1));
    collect_all(&mut ctx);
    assert_eq!(
        tonic_core::zlib::tn_zlib_close(&mut *ctx, ctx.get(1)),
        atom(3)
    );
    let stream = ctx.from_fragment(&copy);
    assert_eq!(tonic_core::zlib::tn_zlib_close(&mut *ctx, stream), NONE);
}

#[test]
fn unclosed_streams_and_atomics_are_reclaimed_on_gc_and_process_teardown() {
    let process = shared(8103);
    let mut ctx = context();
    ctx.shared = &process;
    let array = tonic_core::atomics::tn_atomics_new(&mut *ctx, small(4096), atom(1));
    let weak_array = tonic_core::resource::downgrade(array).unwrap();
    let stream = tonic_core::zlib::tn_zlib_open(&mut *ctx);
    ctx.push(stream);
    assert_eq!(
        tonic_core::zlib::tn_zlib_deflate_init(
            &mut *ctx,
            ctx.get(0),
            small(-1),
            small(8),
            small(15),
            small(8),
            small(0)
        ),
        atom(3)
    );
    let weak_stream = tonic_core::resource::downgrade(ctx.get(0)).unwrap();
    ctx.truncate(0);
    collect_all(&mut ctx);
    assert!(weak_array.upgrade().is_none());
    assert!(weak_stream.upgrade().is_none());
    let array = tonic_core::atomics::tn_atomics_new(&mut *ctx, small(4096), atom(1));
    let weak_array = tonic_core::resource::downgrade(array).unwrap();
    ctx.push(array);
    let stream = tonic_core::zlib::tn_zlib_open(&mut *ctx);
    let weak_stream = tonic_core::resource::downgrade(stream).unwrap();
    drop(ctx);
    assert!(weak_array.upgrade().is_none());
    assert!(weak_stream.upgrade().is_none());
}

#[test]
fn deserialized_resources_retain_native_state_only_while_a_handle_is_alive() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut ctx = context();
    let original = ctx.managed_ref(Tracked(drops.clone()));
    let id = word(original, 1);
    ctx.push(original);
    let restored = ctx.ref_from_id(id);
    let message = ctx.to_fragment(restored);
    drop(ctx);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(message);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let mut ctx = context();
    let restored = ctx.ref_from_id(id);
    assert!(is_ref(restored));
    assert!(tonic_core::resource::get::<Tracked>(restored).is_none());
}
