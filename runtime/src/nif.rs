//! Native implemented functions: the erl_nif API over tonic terms.
//!
//! NIF libraries are compiled from source and linked into the executable;
//! each registers its `ErlNifEntry` before `main`, and a module's `@on_load`
//! binds it through `:erlang.load_nif/2`. During a call every term the NIF
//! sees lives in memory owned by its `ErlNifEnv` (arguments are copied in,
//! new terms are built in an arena), so the moving collector never touches
//! them; the result is copied back into the calling process's heap.

use crate::bif_core::cx;
use crate::heap::{Ctx, Fragment};
use crate::term::*;
use parking_lot::lock_api::RawMutex as _;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CStr};
use std::sync::atomic::{AtomicUsize, Ordering};

type NifFn = unsafe extern "C" fn(*mut Env, c_int, *const Term) -> Term;
type LoadFn = unsafe extern "C" fn(*mut Env, *mut *mut c_void, Term) -> c_int;
type DtorFn = unsafe extern "C" fn(*mut Env, *mut c_void);

#[repr(C)]
pub struct NifFunc {
    name: *const c_char,
    arity: c_uint,
    fptr: NifFn,
    flags: c_uint,
}

#[repr(C)]
pub struct NifEntry {
    major: c_int,
    minor: c_int,
    name: *const c_char,
    num_of_funcs: c_int,
    funcs: *const NifFunc,
    load: Option<LoadFn>,
    reload: Option<LoadFn>,
    upgrade: Option<unsafe extern "C" fn(*mut Env, *mut *mut c_void, *mut *mut c_void, Term) -> c_int>,
    unload: Option<DtorFn>,
}

struct Library {
    entry: *const NifEntry,
    loaded: bool,
    priv_data: *mut c_void,
}
unsafe impl Send for Library {}

static LIBRARIES: parking_lot::Mutex<Vec<Library>> = parking_lot::const_mutex(Vec::new());

/// Called from each library's constructor (see erl_nif.h's ERL_NIF_INIT).
#[no_mangle]
pub unsafe extern "C" fn tonic_nif_register(entry: *const NifEntry) {
    LIBRARIES.lock().push(Library { entry, loaded: false, priv_data: std::ptr::null_mut() });
}

fn module_name(entry: *const NifEntry) -> &'static str {
    unsafe { CStr::from_ptr((*entry).name).to_str().unwrap_or("") }
}

// ---------------------------------------------------------------------------
// Environments
// ---------------------------------------------------------------------------

pub struct Env {
    ctx: *mut Ctx,
    fragments: Vec<Fragment>,
    chunks: Vec<Box<[u64]>>,
    used: usize,
    buffers: Vec<Vec<u8>>,
    exception: Option<Term>,
    priv_data: *mut c_void,
    offheap: Vec<Term>,
}

const CHUNK: usize = 1024;

impl Env {
    pub fn new(ctx: *mut Ctx, priv_data: *mut c_void) -> Env {
        Env { ctx, fragments: Vec::new(), chunks: Vec::new(), used: 0, buffers: Vec::new(), exception: None, priv_data, offheap: Vec::new() }
    }

    pub fn alloc(&mut self, words: usize) -> *mut u64 {
        let fits = self.chunks.last().map(|c| c.len() - self.used >= words).unwrap_or(false);
        if !fits {
            self.chunks.push(vec![0u64; words.max(CHUNK)].into_boxed_slice());
            self.used = 0;
        }
        let chunk = self.chunks.last_mut().unwrap();
        let p = unsafe { chunk.as_mut_ptr().add(self.used) };
        self.used += words;
        p
    }

    /// Whether `addr` is memory this environment owns.
    fn owns(&self, addr: usize) -> bool {
        let within = |start: *const u64, len: usize| {
            let lo = start as usize;
            addr >= lo && addr < lo + len * 8
        };
        self.chunks.iter().any(|c| within(c.as_ptr(), c.len()))
            || self.fragments.iter().any(|f| within(f.buf.as_ptr(), f.buf.len()))
    }

    /// Takes ownership of a fragment; returns its term.
    pub fn adopt(&mut self, fragment: Fragment) -> Term {
        let root = fragment.root;
        self.fragments.push(fragment);
        root
    }

    pub fn owns_address(&self, addr: usize) -> bool {
        self.owns(addr)
    }

    /// Copies a process term into this environment.
    fn import(&mut self, ctx: &Ctx, t: Term) -> Term {
        let fragment = ctx.to_fragment(t);
        let root = fragment.root;
        self.fragments.push(fragment);
        root
    }

    /// Copies a term of this environment into a standalone fragment.
    pub fn export(&self, t: Term) -> Fragment {
        let inside = |a: usize| self.owns(a);
        let words = crate::heap::copy_size_by(t, &inside);
        let mut buf: Vec<u64> = vec![0; words.max(1)];
        let mut free = buf.as_mut_ptr();
        let root = unsafe { crate::heap::copy_term_by(t, &inside, &mut free) };
        Fragment::new(buf, root, free)
    }

    fn clear(&mut self) {
        for t in self.offheap.drain(..) {
            unsafe { crate::resource::release(t) };
        }
        self.fragments.clear();
        self.chunks.clear();
        self.used = 0;
        self.buffers.clear();
        self.exception = None;
    }

    pub fn tuple(&mut self, items: &[Term]) -> Term {
        if items.is_empty() {
            return crate::heap::empty_tuple();
        }
        let p = self.alloc(1 + items.len());
        unsafe {
            *p = header(T_TUPLE, items.len() as u64);
            std::ptr::copy_nonoverlapping(items.as_ptr(), p.add(1), items.len());
        }
        p as Term
    }

    pub fn cons(&mut self, h: Term, t: Term) -> Term {
        let p = self.alloc(3);
        unsafe {
            *p = header(T_CONS, 2);
            *p.add(1) = h;
            *p.add(2) = t;
        }
        p as Term
    }

    fn binary(&mut self, len: usize) -> (Term, *mut u8) {
        let words = 2 + (len + 7) / 8;
        let p = self.alloc(words);
        unsafe {
            *p = header(T_BINARY, (words - 1) as u64);
            *p.add(1) = len as u64;
            (p as Term, p.add(2) as *mut u8)
        }
    }

    pub fn bytes(&mut self, bytes: &[u8]) -> Term {
        let (t, d) = self.binary(bytes.len());
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), d, bytes.len()) };
        t
    }

    pub fn integer(&mut self, v: i128) -> Term {
        if v >= SMALL_MIN as i128 && v <= SMALL_MAX as i128 {
            return small(v as i64);
        }
        let magnitude = v.unsigned_abs();
        let digits: Vec<u64> = if magnitude >> 64 == 0 {
            vec![magnitude as u64]
        } else {
            vec![magnitude as u64, (magnitude >> 64) as u64]
        };
        let p = self.alloc(2 + digits.len());
        unsafe {
            *p = header(T_BIGINT, (1 + digits.len()) as u64);
            *p.add(1) = (v < 0) as u64;
            for (i, d) in digits.iter().enumerate() {
                *p.add(2 + i) = *d;
            }
        }
        p as Term
    }

    fn float(&mut self, f: f64) -> Term {
        let p = self.alloc(2);
        unsafe {
            *p = header(T_FLOAT, 1);
            *p.add(1) = f.to_bits();
        }
        p as Term
    }

    fn charlist(&mut self, bytes: &[u8]) -> Term {
        let mut list = NIL_LIST;
        for b in bytes.iter().rev() {
            list = self.cons(small(*b as i64), list);
        }
        list
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        self.clear();
    }
}

unsafe fn env<'a>(e: *mut Env) -> &'a mut Env {
    &mut *e
}

fn integer_of(t: Term) -> Option<i128> {
    if is_small(t) {
        Some(small_val(t) as i128)
    } else if is_bigint(t) {
        use num_traits::ToPrimitive;
        crate::num::to_big(t).to_i128()
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Loading and calling (BIFs used by compiled stubs)
// ---------------------------------------------------------------------------

fn charlist_term(ctx: &mut Ctx, s: &str) -> Term {
    let mut items: Vec<Term> = s.bytes().map(|b| small(b as i64)).collect();
    ctx.list_from_vec(&mut items)
}

fn load_error(ctx: &mut Ctx, kind: &str, text: &str) -> Term {
    let text = charlist_term(ctx, text);
    let reason = ctx.tuple(&[atom(crate::atoms::intern(kind)), text]);
    ctx.tuple(&[atom(a::ERROR), reason])
}

/// `:erlang.load_nif/2` inside `module`: binds the library linked for it.
#[no_mangle]
pub extern "C" fn tn_nif_load(c: *mut Ctx, module: Term, _path: Term, info: Term) -> Term {
    let ctx = cx(c);
    if !is_atom(module) {
        return ctx.badarg("argument error");
    }
    let name = crate::atoms::name(atom_idx(module));
    let found = {
        let libraries = LIBRARIES.lock();
        libraries.iter().position(|l| module_name(l.entry) == name).map(|i| (i, libraries[i].loaded, libraries[i].entry))
    };
    let (index, loaded, entry) = match found {
        Some(found) => found,
        None => {
            return load_error(ctx, "load_failed", &format!("Failed to load NIF library: no native library for {} is linked into this executable", name.trim_start_matches("Elixir.")))
        }
    };
    if loaded {
        return atom(a::OK);
    }
    let mut priv_data: *mut c_void = std::ptr::null_mut();
    if let Some(load) = unsafe { (*entry).load } {
        let mut e = Env::new(c, std::ptr::null_mut());
        let info = e.import(ctx, info);
        let status = unsafe { load(&mut e, &mut priv_data, info) };
        if status != 0 {
            return load_error(ctx, "load", &format!("Library load-call unsuccessful ({}).", status));
        }
    }
    let mut libraries = LIBRARIES.lock();
    libraries[index].loaded = true;
    libraries[index].priv_data = priv_data;
    atom(a::OK)
}

/// A NIF stub of `module`: calls the native function, or fails with the
/// stub's own `:erlang.nif_error/1` reason when the library is not loaded.
#[no_mangle]
pub extern "C" fn tn_nif_call(c: *mut Ctx, module: Term, name: Term, args: Term, reason: Term) -> Term {
    let ctx = cx(c);
    let module_text = crate::atoms::name(atom_idx(module));
    let fun = crate::atoms::name(atom_idx(name));
    let mut argc = 0usize;
    let mut l = args;
    while is_cons(l) {
        argc += 1;
        l = tail(l);
    }
    let target = {
        let libraries = LIBRARIES.lock();
        libraries.iter().find(|l| l.loaded && module_name(l.entry) == module_text).and_then(|l| unsafe {
            let e = &*l.entry;
            (0..e.num_of_funcs as usize)
                .map(|i| &*e.funcs.add(i))
                .find(|f| f.arity as usize == argc && CStr::from_ptr(f.name).to_bytes() == fun.as_bytes())
                .map(|f| (f.fptr, l.priv_data))
        })
    };
    let (fptr, priv_data) = match target {
        Some(target) => target,
        None => return ctx.raise_error(reason),
    };
    let mut e = Env::new(c, priv_data);
    let mut argv = Vec::with_capacity(argc);
    let mut l = e.import(ctx, args);
    while is_cons(l) {
        argv.push(head(l));
        l = tail(l);
    }
    let result = unsafe { fptr(&mut e, argc as c_int, argv.as_ptr()) };
    if let Some(exception) = e.exception {
        let reason = ctx.copy_in_by(exception, &|a| e.owns(a));
        return ctx.raise_error(reason);
    }
    ctx.copy_in_by(result, &|a| e.owns(a))
}

// ---------------------------------------------------------------------------
// Memory and environments
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn enif_alloc(size: usize) -> *mut c_void {
    libc_malloc(size)
}

#[no_mangle]
pub unsafe extern "C" fn enif_realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    libc_realloc(ptr, size)
}

#[no_mangle]
pub unsafe extern "C" fn enif_free(ptr: *mut c_void) {
    libc_free(ptr)
}

extern "C" {
    #[link_name = "malloc"]
    fn libc_malloc(size: usize) -> *mut c_void;
    #[link_name = "realloc"]
    fn libc_realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    #[link_name = "free"]
    fn libc_free(ptr: *mut c_void);
}

#[no_mangle]
pub extern "C" fn enif_alloc_env() -> *mut Env {
    Box::into_raw(Box::new(Env::new(std::ptr::null_mut(), std::ptr::null_mut())))
}

#[no_mangle]
pub unsafe extern "C" fn enif_free_env(e: *mut Env) {
    if !e.is_null() {
        drop(Box::from_raw(e));
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_clear_env(e: *mut Env) {
    env(e).clear()
}

#[no_mangle]
pub unsafe extern "C" fn enif_priv_data(e: *mut Env) -> *mut c_void {
    env(e).priv_data
}

// ---------------------------------------------------------------------------
// Exceptions
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn enif_make_badarg(e: *mut Env) -> Term {
    env(e).exception = Some(atom(crate::atoms::intern("badarg")));
    NONE
}

#[no_mangle]
pub unsafe extern "C" fn enif_raise_exception(e: *mut Env, reason: Term) -> Term {
    env(e).exception = Some(reason);
    NONE
}

#[no_mangle]
pub unsafe extern "C" fn enif_has_pending_exception(e: *mut Env, reason: *mut Term) -> c_int {
    match env(e).exception {
        Some(r) => {
            if !reason.is_null() {
                *reason = r;
            }
            1
        }
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn enif_make_atom(_e: *mut Env, name: *const c_char) -> Term {
    atom(crate::atoms::intern(&CStr::from_ptr(name).to_string_lossy()))
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_atom_len(_e: *mut Env, name: *const c_char, len: usize) -> Term {
    let bytes = raw_slice(name as *const u8, len);
    atom(crate::atoms::intern(&String::from_utf8_lossy(bytes)))
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_existing_atom(_e: *mut Env, name: *const c_char, out: *mut Term, _enc: c_int) -> c_int {
    match crate::atoms::lookup(&CStr::from_ptr(name).to_string_lossy()) {
        Some(i) => {
            *out = atom(i);
            1
        }
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_int(e: *mut Env, i: c_int) -> Term {
    env(e).integer(i as i128)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_uint(e: *mut Env, i: c_uint) -> Term {
    env(e).integer(i as i128)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_long(e: *mut Env, i: c_long) -> Term {
    env(e).integer(i as i128)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_ulong(e: *mut Env, i: c_ulong) -> Term {
    env(e).integer(i as i128)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_int64(e: *mut Env, i: i64) -> Term {
    env(e).integer(i as i128)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_uint64(e: *mut Env, i: u64) -> Term {
    env(e).integer(i as i128)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_double(e: *mut Env, d: f64) -> Term {
    if !d.is_finite() {
        return enif_make_badarg(e);
    }
    env(e).float(d)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_string(e: *mut Env, s: *const c_char, _enc: c_int) -> Term {
    env(e).charlist(CStr::from_ptr(s).to_bytes())
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_string_len(e: *mut Env, s: *const c_char, len: usize, _enc: c_int) -> Term {
    env(e).charlist(raw_slice(s as *const u8, len))
}

/// Marks binaries allocated by enif_alloc_binary (their memory is ours).
const OWNED_BINARY: usize = 0x746f6e6963;

#[repr(C)]
pub struct NifBinary {
    size: usize,
    data: *mut u8,
    ref_bin: *mut c_void,
    spare: [*mut c_void; 2],
}

#[no_mangle]
pub unsafe extern "C" fn enif_alloc_binary(size: usize, bin: *mut NifBinary) -> c_int {
    let data = libc_malloc(size.max(1)) as *mut u8;
    if data.is_null() {
        return 0;
    }
    (*bin).size = size;
    (*bin).data = data;
    (*bin).ref_bin = OWNED_BINARY as *mut c_void;
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_realloc_binary(bin: *mut NifBinary, size: usize) -> c_int {
    if (*bin).ref_bin as usize != OWNED_BINARY {
        let mut fresh = NifBinary { size: 0, data: std::ptr::null_mut(), ref_bin: std::ptr::null_mut(), spare: [std::ptr::null_mut(); 2] };
        if enif_alloc_binary(size, &mut fresh) == 0 {
            return 0;
        }
        std::ptr::copy_nonoverlapping((*bin).data, fresh.data, (*bin).size.min(size));
        *bin = fresh;
        return 1;
    }
    let data = libc_realloc((*bin).data as *mut c_void, size.max(1)) as *mut u8;
    if data.is_null() {
        return 0;
    }
    (*bin).data = data;
    (*bin).size = size;
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_release_binary(bin: *mut NifBinary) {
    if (*bin).ref_bin as usize == OWNED_BINARY {
        libc_free((*bin).data as *mut c_void);
        (*bin).ref_bin = std::ptr::null_mut();
    }
    (*bin).data = std::ptr::null_mut();
    (*bin).size = 0;
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_binary(e: *mut Env, bin: *mut NifBinary) -> Term {
    let bytes = raw_slice((*bin).data, (*bin).size);
    let t = env(e).bytes(bytes);
    enif_release_binary(bin);
    t
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_new_binary(e: *mut Env, size: usize, termp: *mut Term) -> *mut u8 {
    let (t, data) = env(e).binary(size);
    *termp = t;
    data
}

/// A slice from C: NIFs pass NULL with a zero count, which
/// `slice::from_raw_parts` does not allow.
unsafe fn raw_slice<'a, T>(p: *const T, n: usize) -> &'a [T] {
    if n == 0 || p.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(p, n)
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_sub_binary(e: *mut Env, bin: Term, pos: usize, size: usize) -> Term {
    if !is_binary(bin) || pos + size > bin_len(bin) {
        return enif_make_badarg(e);
    }
    let bytes = bin_bytes(bin)[pos..pos + size].to_vec();
    env(e).bytes(&bytes)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_tuple_from_array(e: *mut Env, arr: *const Term, cnt: c_uint) -> Term {
    let items = raw_slice(arr, cnt as usize);
    env(e).tuple(items)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_list_from_array(e: *mut Env, arr: *const Term, cnt: c_uint) -> Term {
    let items = raw_slice(arr, cnt as usize);
    let mut list = NIL_LIST;
    for item in items.iter().rev() {
        list = env(e).cons(*item, list);
    }
    list
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_list_cell(e: *mut Env, car: Term, cdr: Term) -> Term {
    env(e).cons(car, cdr)
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_ref(e: *mut Env) -> Term {
    let p = env(e).alloc(2);
    *p = header(T_REF, 1);
    *p.add(1) = crate::sched::next_ref_id();
    p as Term
}

#[repr(C)]
pub struct NifPid {
    pid: Term,
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_pid(_e: *mut Env, pid: *const NifPid) -> Term {
    (*pid).pid
}

// ---------------------------------------------------------------------------
// Inspection
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn enif_is_atom(_e: *mut Env, t: Term) -> c_int {
    is_atom(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_binary(_e: *mut Env, t: Term) -> c_int {
    is_binary(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_list(_e: *mut Env, t: Term) -> c_int {
    is_list(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_tuple(_e: *mut Env, t: Term) -> c_int {
    is_tuple(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_map(_e: *mut Env, t: Term) -> c_int {
    is_map(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_number(_e: *mut Env, t: Term) -> c_int {
    is_number(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_pid(_e: *mut Env, t: Term) -> c_int {
    is_pid(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_ref(_e: *mut Env, t: Term) -> c_int {
    is_ref(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_port(_e: *mut Env, t: Term) -> c_int {
    is_port(t) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_empty_list(_e: *mut Env, t: Term) -> c_int {
    (t == NIL_LIST) as c_int
}

#[no_mangle]
pub extern "C" fn enif_is_identical(a: Term, b: Term) -> c_int {
    crate::cmp::eq_exact(a, b) as c_int
}

#[no_mangle]
pub extern "C" fn enif_compare(a: Term, b: Term) -> c_int {
    crate::cmp::compare(a, b) as c_int
}

#[no_mangle]
pub extern "C" fn enif_term_type(_e: *mut Env, t: Term) -> c_int {
    if is_atom(t) {
        1
    } else if is_bitstring(t) {
        2
    } else if is_float(t) {
        3
    } else if is_closure(t) {
        4
    } else if is_integer(t) {
        5
    } else if is_list(t) {
        6
    } else if is_map(t) {
        7
    } else if is_pid(t) {
        8
    } else if is_port(t) {
        9
    } else if is_ref(t) {
        10
    } else {
        11
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_atom(_e: *mut Env, t: Term, buf: *mut c_char, size: c_uint, _enc: c_int) -> c_int {
    if !is_atom(t) {
        return 0;
    }
    let name = crate::atoms::name(atom_idx(t)).as_bytes();
    if name.len() + 1 > size as usize {
        return 0;
    }
    std::ptr::copy_nonoverlapping(name.as_ptr(), buf as *mut u8, name.len());
    *buf.add(name.len()) = 0;
    (name.len() + 1) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_atom_length(_e: *mut Env, t: Term, len: *mut c_uint, _enc: c_int) -> c_int {
    if !is_atom(t) {
        return 0;
    }
    *len = crate::atoms::name(atom_idx(t)).len() as c_uint;
    1
}

macro_rules! get_integer {
    ($name:ident, $ty:ty) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(_e: *mut Env, t: Term, out: *mut $ty) -> c_int {
            match integer_of(t).and_then(|v| <$ty>::try_from(v).ok()) {
                Some(v) => {
                    *out = v;
                    1
                }
                None => 0,
            }
        }
    };
}

get_integer!(enif_get_int, c_int);
get_integer!(enif_get_uint, c_uint);
get_integer!(enif_get_long, c_long);
get_integer!(enif_get_ulong, c_ulong);
get_integer!(enif_get_int64, i64);
get_integer!(enif_get_uint64, u64);

#[no_mangle]
pub unsafe extern "C" fn enif_get_double(_e: *mut Env, t: Term, out: *mut f64) -> c_int {
    if !is_float(t) {
        return 0;
    }
    *out = float_val(t);
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_string(_e: *mut Env, list: Term, buf: *mut c_char, size: c_uint, _enc: c_int) -> c_int {
    if size == 0 {
        return 0;
    }
    let mut n = 0usize;
    let mut l = list;
    while is_cons(l) {
        let c = head(l);
        if !is_small(c) || !(0..=255).contains(&small_val(c)) {
            return 0;
        }
        if n + 1 >= size as usize {
            *buf.add(n) = 0;
            return -(size as c_int);
        }
        *buf.add(n) = small_val(c) as c_char;
        n += 1;
        l = tail(l);
    }
    if l != NIL_LIST {
        return 0;
    }
    *buf.add(n) = 0;
    (n + 1) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_tuple(_e: *mut Env, t: Term, arity: *mut c_int, array: *mut *const Term) -> c_int {
    if !is_tuple(t) {
        return 0;
    }
    *arity = tuple_size(t) as c_int;
    *array = ptr(t).add(1) as *const Term;
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_list_cell(_e: *mut Env, t: Term, h: *mut Term, tl: *mut Term) -> c_int {
    if !is_cons(t) {
        return 0;
    }
    *h = head(t);
    *tl = tail(t);
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_list_length(_e: *mut Env, t: Term, len: *mut c_uint) -> c_int {
    let mut n = 0;
    let mut l = t;
    while is_cons(l) {
        n += 1;
        l = tail(l);
    }
    if l != NIL_LIST {
        return 0;
    }
    *len = n;
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_map_size(_e: *mut Env, t: Term, size: *mut usize) -> c_int {
    if !is_map(t) {
        return 0;
    }
    *size = crate::map::size(t) as usize;
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_map_value(_e: *mut Env, map: Term, key: Term, value: *mut Term) -> c_int {
    if !is_map(map) {
        return 0;
    }
    match crate::map::find(map, key) {
        Some(v) => {
            *value = v;
            1
        }
        None => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_inspect_binary(_e: *mut Env, t: Term, bin: *mut NifBinary) -> c_int {
    if !is_binary(t) {
        return 0;
    }
    let bytes = bin_bytes(t);
    (*bin).size = bytes.len();
    (*bin).data = bytes.as_ptr() as *mut u8;
    (*bin).ref_bin = std::ptr::null_mut();
    1
}

pub fn flatten_iolist(t: Term, out: &mut Vec<u8>) -> bool {
    if is_binary(t) {
        out.extend_from_slice(bin_bytes(t));
        return true;
    }
    let mut l = t;
    while is_cons(l) {
        let h = head(l);
        if is_small(h) && (0..=255).contains(&small_val(h)) {
            out.push(small_val(h) as u8);
        } else if !(is_binary(h) || is_list(h)) || !flatten_iolist(h, out) {
            return false;
        }
        l = tail(l);
    }
    l == NIL_LIST || (is_binary(l) && flatten_iolist(l, out))
}

#[no_mangle]
pub unsafe extern "C" fn enif_inspect_iolist_as_binary(e: *mut Env, t: Term, bin: *mut NifBinary) -> c_int {
    let mut bytes = Vec::new();
    if !flatten_iolist(t, &mut bytes) {
        return 0;
    }
    let e = env(e);
    e.buffers.push(bytes);
    let stored = e.buffers.last().unwrap();
    (*bin).size = stored.len();
    (*bin).data = stored.as_ptr() as *mut u8;
    (*bin).ref_bin = std::ptr::null_mut();
    1
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

pub struct ResourceType {
    dtor: Option<DtorFn>,
}

#[repr(C)]
struct ResourceHeader {
    kind: *const ResourceType,
    refs: AtomicUsize,
    id: u64,
    size: usize,
}

const RESOURCE_HEADER: usize = std::mem::size_of::<ResourceHeader>();

// native keep_resource references can outlive every term wrapper
static RESOURCES: parking_lot::Mutex<Option<HashMap<u64, usize>>> = parking_lot::const_mutex(None);

fn resource_header(obj: *mut c_void) -> *mut ResourceHeader {
    unsafe { (obj as *mut u8).sub(RESOURCE_HEADER) as *mut ResourceHeader }
}

#[no_mangle]
pub unsafe extern "C" fn enif_open_resource_type(
    _e: *mut Env,
    _module: *const c_char,
    _name: *const c_char,
    dtor: Option<DtorFn>,
    flags: c_int,
    tried: *mut c_int,
) -> *mut ResourceType {
    if !tried.is_null() {
        *tried = flags;
    }
    Box::into_raw(Box::new(ResourceType { dtor }))
}

#[repr(C)]
pub struct ResourceTypeInit {
    dtor: Option<DtorFn>,
    stop: *mut c_void,
    down: *mut c_void,
    members: c_int,
    dyncall: *mut c_void,
}

#[no_mangle]
pub unsafe extern "C" fn enif_init_resource_type(
    e: *mut Env,
    name: *const c_char,
    init: *const ResourceTypeInit,
    flags: c_int,
    tried: *mut c_int,
) -> *mut ResourceType {
    enif_open_resource_type(e, std::ptr::null(), name, (*init).dtor, flags, tried)
}

#[no_mangle]
pub unsafe extern "C" fn enif_alloc_resource(kind: *mut ResourceType, size: usize) -> *mut c_void {
    let base = libc_malloc(RESOURCE_HEADER + size) as *mut ResourceHeader;
    if base.is_null() {
        return std::ptr::null_mut();
    }
    let id = crate::sched::next_ref_id();
    base.write(ResourceHeader { kind, refs: AtomicUsize::new(1), id, size });
    RESOURCES.lock().get_or_insert_with(HashMap::new).insert(id, base as usize);
    (base as *mut u8).add(RESOURCE_HEADER) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn enif_keep_resource(obj: *mut c_void) {
    (*resource_header(obj)).refs.fetch_add(1, Ordering::AcqRel);
}

#[no_mangle]
pub unsafe extern "C" fn enif_release_resource(obj: *mut c_void) {
    let h = resource_header(obj);
    let mut resources = RESOURCES.lock();
    if (*h).refs.fetch_sub(1, Ordering::AcqRel) != 1 {
        return;
    }
    resources.as_mut().unwrap().remove(&(*h).id);
    drop(resources);
    if let Some(dtor) = (*(*h).kind).dtor {
        let mut e = Env::new(std::ptr::null_mut(), std::ptr::null_mut());
        dtor(&mut e, obj);
    }
    libc_free(h as *mut c_void);
}

pub(crate) struct ResourceTerm {
    obj: usize,
}

pub(crate) fn resource_from_id(id: u64) -> Option<ResourceTerm> {
    let resources = RESOURCES.lock();
    let base = *resources.as_ref()?.get(&id)?;
    let obj = unsafe { (base as *mut u8).add(RESOURCE_HEADER) as *mut c_void };
    unsafe { enif_keep_resource(obj) };
    Some(ResourceTerm { obj: obj as usize })
}

impl Drop for ResourceTerm {
    fn drop(&mut self) {
        unsafe { enif_release_resource(self.obj as *mut c_void) };
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_make_resource(e: *mut Env, obj: *mut c_void) -> Term {
    let h = resource_header(obj);
    (*h).refs.fetch_add(1, Ordering::AcqRel);
    let e = env(e);
    let p = e.alloc(3);
    crate::resource::write(p, (*h).id, ResourceTerm { obj: obj as usize });
    e.offheap.push(p as Term);
    p as Term
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_resource(_e: *mut Env, t: Term, kind: *mut ResourceType, objp: *mut *mut c_void) -> c_int {
    let Some(resource) = crate::resource::get::<ResourceTerm>(t) else {
        return 0;
    };
    let obj = resource.obj as *mut c_void;
    if (*resource_header(obj)).kind != kind as *const ResourceType {
        return 0;
    }
    *objp = obj;
    1
}

// ---------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------

#[no_mangle]
pub unsafe extern "C" fn enif_self(e: *mut Env, out: *mut NifPid) -> *mut NifPid {
    let e = env(e);
    if e.ctx.is_null() {
        return std::ptr::null_mut();
    }
    (*out).pid = pid((*e.ctx).pid());
    out
}

#[no_mangle]
pub unsafe extern "C" fn enif_get_local_pid(_e: *mut Env, t: Term, out: *mut NifPid) -> c_int {
    if !is_pid(t) {
        return 0;
    }
    (*out).pid = t;
    1
}

#[no_mangle]
pub unsafe extern "C" fn enif_is_process_alive(_e: *mut Env, p: *mut NifPid) -> c_int {
    crate::sched::is_alive(pid_id((*p).pid)) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn enif_send(caller: *mut Env, to: *const NifPid, msg_env: *mut Env, msg: Term) -> c_int {
    let source = if msg_env.is_null() { caller } else { msg_env };
    if source.is_null() {
        return 0;
    }
    let target = pid_id((*to).pid);
    if !crate::sched::is_alive(target) {
        return 0;
    }
    let fragment = env(source).export(msg);
    crate::sched::send_fragment(target, fragment);
    if !msg_env.is_null() {
        env(msg_env).clear();
    }
    1
}

// ---------------------------------------------------------------------------
// Threads and time
// ---------------------------------------------------------------------------

pub struct NifMutex(parking_lot::RawMutex);

#[no_mangle]
pub extern "C" fn enif_mutex_create(_name: *mut c_char) -> *mut NifMutex {
    Box::into_raw(Box::new(NifMutex(parking_lot::RawMutex::INIT)))
}

#[no_mangle]
pub unsafe extern "C" fn enif_mutex_destroy(m: *mut NifMutex) {
    if !m.is_null() {
        drop(Box::from_raw(m));
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_mutex_lock(m: *mut NifMutex) {
    (*m).0.lock()
}

#[no_mangle]
pub unsafe extern "C" fn enif_mutex_trylock(m: *mut NifMutex) -> c_int {
    if (*m).0.try_lock() {
        0
    } else {
        16
    }
}

#[no_mangle]
pub unsafe extern "C" fn enif_mutex_unlock(m: *mut NifMutex) {
    (*m).0.unlock()
}

#[no_mangle]
pub extern "C" fn enif_thread_type() -> c_int {
    1
}

#[no_mangle]
pub extern "C" fn enif_monotonic_time(unit: c_int) -> i64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let nanos = START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as i64;
    match unit {
        0 => nanos / 1_000_000_000,
        1 => nanos / 1_000_000,
        2 => nanos / 1_000,
        _ => nanos,
    }
}
