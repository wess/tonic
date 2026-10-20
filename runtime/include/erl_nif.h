/*
 * erl_nif.h for tonic.
 *
 * NIF libraries are compiled from source and linked into the native
 * executable. Terms are tonic terms that live in memory owned by the
 * ErlNifEnv; the runtime copies arguments in and results out, so the
 * garbage collector never moves a term a NIF holds.
 */
#ifndef TONIC_ERL_NIF_H
#define TONIC_ERL_NIF_H

#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define ERL_NIF_MAJOR_VERSION 2
#define ERL_NIF_MINOR_VERSION 17
#define ERL_NIF_MIN_ERTS_VERSION "erts-14.0"

typedef uint64_t ERL_NIF_TERM;
typedef ERL_NIF_TERM ERL_NIF_UINT;
typedef int64_t ErlNifSInt64;
typedef uint64_t ErlNifUInt64;
typedef ErlNifSInt64 ErlNifTime;

typedef struct tonic_nif_env ErlNifEnv;
typedef struct tonic_nif_resource_type ErlNifResourceType;
typedef struct tonic_nif_mutex ErlNifMutex;
typedef struct tonic_nif_cond ErlNifCond;
typedef struct tonic_nif_rwlock ErlNifRWLock;

typedef struct {
    ERL_NIF_TERM pid;
} ErlNifPid;

typedef struct {
    size_t size;
    unsigned char* data;
    void* ref_bin;
    void* __spare__[2];
} ErlNifBinary;

typedef struct {
    ERL_NIF_TERM data[4];
} ErlNifMonitor;

typedef enum {
    ERL_NIF_SEC = 0,
    ERL_NIF_MSEC = 1,
    ERL_NIF_USEC = 2,
    ERL_NIF_NSEC = 3
} ErlNifTimeUnit;

typedef enum {
    ERL_NIF_LATIN1 = 1,
    ERL_NIF_UTF8 = 2
} ErlNifCharEncoding;

typedef enum {
    ERL_NIF_RT_CREATE = 1,
    ERL_NIF_RT_TAKEOVER = 2
} ErlNifResourceFlags;

typedef enum {
    ERL_NIF_TERM_TYPE_ATOM = 1,
    ERL_NIF_TERM_TYPE_BITSTRING = 2,
    ERL_NIF_TERM_TYPE_FLOAT = 3,
    ERL_NIF_TERM_TYPE_FUN = 4,
    ERL_NIF_TERM_TYPE_INTEGER = 5,
    ERL_NIF_TERM_TYPE_LIST = 6,
    ERL_NIF_TERM_TYPE_MAP = 7,
    ERL_NIF_TERM_TYPE_PID = 8,
    ERL_NIF_TERM_TYPE_PORT = 9,
    ERL_NIF_TERM_TYPE_REFERENCE = 10,
    ERL_NIF_TERM_TYPE_TUPLE = 11
} ErlNifTermType;

typedef void ErlNifResourceDtor(ErlNifEnv*, void*);
typedef void ErlNifResourceStop(ErlNifEnv*, void*, int, int);
typedef void ErlNifResourceDown(ErlNifEnv*, void*, ErlNifPid*, ErlNifMonitor*);

typedef struct {
    ErlNifResourceDtor* dtor;
    ErlNifResourceStop* stop;
    ErlNifResourceDown* down;
    int members;
    void* dyncall;
} ErlNifResourceTypeInit;

#define ERL_NIF_DIRTY_JOB_CPU_BOUND 1
#define ERL_NIF_DIRTY_JOB_IO_BOUND 2

typedef struct {
    const char* name;
    unsigned arity;
    ERL_NIF_TERM (*fptr)(ErlNifEnv* env, int argc, const ERL_NIF_TERM argv[]);
    unsigned flags;
} ErlNifFunc;

typedef struct {
    int major;
    int minor;
    const char* name;
    int num_of_funcs;
    ErlNifFunc* funcs;
    int (*load)(ErlNifEnv*, void** priv_data, ERL_NIF_TERM load_info);
    int (*reload)(ErlNifEnv*, void** priv_data, ERL_NIF_TERM load_info);
    int (*upgrade)(ErlNifEnv*, void** priv_data, void** old_priv_data, ERL_NIF_TERM load_info);
    void (*unload)(ErlNifEnv*, void* priv_data);
} ErlNifEntry;

/* Memory */
void* enif_alloc(size_t size);
void* enif_realloc(void* ptr, size_t size);
void enif_free(void* ptr);

/* Environments */
ErlNifEnv* enif_alloc_env(void);
void enif_free_env(ErlNifEnv* env);
void enif_clear_env(ErlNifEnv* env);
void* enif_priv_data(ErlNifEnv* env);

/* Exceptions */
ERL_NIF_TERM enif_make_badarg(ErlNifEnv* env);
ERL_NIF_TERM enif_raise_exception(ErlNifEnv* env, ERL_NIF_TERM reason);
int enif_has_pending_exception(ErlNifEnv* env, ERL_NIF_TERM* reason);

/* Construction */
ERL_NIF_TERM enif_make_atom(ErlNifEnv* env, const char* name);
ERL_NIF_TERM enif_make_atom_len(ErlNifEnv* env, const char* name, size_t len);
int enif_make_existing_atom(ErlNifEnv* env, const char* name, ERL_NIF_TERM* atom, ErlNifCharEncoding encoding);
ERL_NIF_TERM enif_make_int(ErlNifEnv* env, int i);
ERL_NIF_TERM enif_make_uint(ErlNifEnv* env, unsigned i);
ERL_NIF_TERM enif_make_long(ErlNifEnv* env, long i);
ERL_NIF_TERM enif_make_ulong(ErlNifEnv* env, unsigned long i);
ERL_NIF_TERM enif_make_int64(ErlNifEnv* env, ErlNifSInt64 i);
ERL_NIF_TERM enif_make_uint64(ErlNifEnv* env, ErlNifUInt64 i);
ERL_NIF_TERM enif_make_double(ErlNifEnv* env, double d);
ERL_NIF_TERM enif_make_string(ErlNifEnv* env, const char* string, ErlNifCharEncoding encoding);
ERL_NIF_TERM enif_make_string_len(ErlNifEnv* env, const char* string, size_t len, ErlNifCharEncoding encoding);
ERL_NIF_TERM enif_make_binary(ErlNifEnv* env, ErlNifBinary* bin);
unsigned char* enif_make_new_binary(ErlNifEnv* env, size_t size, ERL_NIF_TERM* termp);
ERL_NIF_TERM enif_make_sub_binary(ErlNifEnv* env, ERL_NIF_TERM bin_term, size_t pos, size_t size);
ERL_NIF_TERM enif_make_tuple_from_array(ErlNifEnv* env, const ERL_NIF_TERM arr[], unsigned cnt);
ERL_NIF_TERM enif_make_list_from_array(ErlNifEnv* env, const ERL_NIF_TERM arr[], unsigned cnt);
ERL_NIF_TERM enif_make_list_cell(ErlNifEnv* env, ERL_NIF_TERM car, ERL_NIF_TERM cdr);
ERL_NIF_TERM enif_make_ref(ErlNifEnv* env);
ERL_NIF_TERM enif_make_pid(ErlNifEnv* env, const ErlNifPid* pid);
ERL_NIF_TERM enif_make_resource(ErlNifEnv* env, void* obj);

/* Inspection */
int enif_is_atom(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_binary(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_list(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_tuple(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_map(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_number(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_pid(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_ref(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_port(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_empty_list(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_is_identical(ERL_NIF_TERM lhs, ERL_NIF_TERM rhs);
int enif_compare(ERL_NIF_TERM lhs, ERL_NIF_TERM rhs);
ErlNifTermType enif_term_type(ErlNifEnv* env, ERL_NIF_TERM term);
int enif_get_atom(ErlNifEnv* env, ERL_NIF_TERM atom, char* buf, unsigned size, ErlNifCharEncoding encoding);
int enif_get_atom_length(ErlNifEnv* env, ERL_NIF_TERM atom, unsigned* len, ErlNifCharEncoding encoding);
int enif_get_int(ErlNifEnv* env, ERL_NIF_TERM term, int* ip);
int enif_get_uint(ErlNifEnv* env, ERL_NIF_TERM term, unsigned* ip);
int enif_get_long(ErlNifEnv* env, ERL_NIF_TERM term, long* ip);
int enif_get_ulong(ErlNifEnv* env, ERL_NIF_TERM term, unsigned long* ip);
int enif_get_int64(ErlNifEnv* env, ERL_NIF_TERM term, ErlNifSInt64* ip);
int enif_get_uint64(ErlNifEnv* env, ERL_NIF_TERM term, ErlNifUInt64* ip);
int enif_get_double(ErlNifEnv* env, ERL_NIF_TERM term, double* dp);
int enif_get_string(ErlNifEnv* env, ERL_NIF_TERM list, char* buf, unsigned size, ErlNifCharEncoding encoding);
int enif_get_tuple(ErlNifEnv* env, ERL_NIF_TERM tpl, int* arity, const ERL_NIF_TERM** array);
int enif_get_list_cell(ErlNifEnv* env, ERL_NIF_TERM term, ERL_NIF_TERM* head, ERL_NIF_TERM* tail);
int enif_get_list_length(ErlNifEnv* env, ERL_NIF_TERM term, unsigned* len);
int enif_get_map_size(ErlNifEnv* env, ERL_NIF_TERM term, size_t* size);
int enif_get_map_value(ErlNifEnv* env, ERL_NIF_TERM map, ERL_NIF_TERM key, ERL_NIF_TERM* value);
int enif_inspect_binary(ErlNifEnv* env, ERL_NIF_TERM bin_term, ErlNifBinary* bin);
int enif_inspect_iolist_as_binary(ErlNifEnv* env, ERL_NIF_TERM term, ErlNifBinary* bin);

/* Binaries */
int enif_alloc_binary(size_t size, ErlNifBinary* bin);
int enif_realloc_binary(ErlNifBinary* bin, size_t size);
void enif_release_binary(ErlNifBinary* bin);

/* Resources */
ErlNifResourceType* enif_open_resource_type(ErlNifEnv* env, const char* module_str, const char* name, ErlNifResourceDtor* dtor, ErlNifResourceFlags flags, ErlNifResourceFlags* tried);
ErlNifResourceType* enif_init_resource_type(ErlNifEnv* env, const char* name_str, const ErlNifResourceTypeInit* init, ErlNifResourceFlags flags, ErlNifResourceFlags* tried);
void* enif_alloc_resource(ErlNifResourceType* type, size_t size);
void enif_release_resource(void* obj);
void enif_keep_resource(void* obj);
int enif_get_resource(ErlNifEnv* env, ERL_NIF_TERM term, ErlNifResourceType* type, void** objp);

/* Processes */
ErlNifPid* enif_self(ErlNifEnv* caller_env, ErlNifPid* pid);
int enif_get_local_pid(ErlNifEnv* env, ERL_NIF_TERM term, ErlNifPid* pid);
int enif_is_process_alive(ErlNifEnv* env, ErlNifPid* pid);
int enif_send(ErlNifEnv* caller_env, const ErlNifPid* to_pid, ErlNifEnv* msg_env, ERL_NIF_TERM msg);

/* Threads */
ErlNifMutex* enif_mutex_create(char* name);
void enif_mutex_destroy(ErlNifMutex* mtx);
int enif_mutex_trylock(ErlNifMutex* mtx);
void enif_mutex_lock(ErlNifMutex* mtx);
void enif_mutex_unlock(ErlNifMutex* mtx);
int enif_thread_type(void);
ErlNifTime enif_monotonic_time(ErlNifTimeUnit unit);

#define ERL_NIF_THR_UNDEFINED 0
#define ERL_NIF_THR_NORMAL_SCHEDULER 1
#define ERL_NIF_THR_DIRTY_CPU_SCHEDULER 2
#define ERL_NIF_THR_DIRTY_IO_SCHEDULER 3

#define enif_get_local_port(env, term, port) 0
#define enif_select(env, e, mode, obj, pid, ref) (-1)

static inline ERL_NIF_TERM enif_make_tuple(ErlNifEnv* env, unsigned cnt, ...) {
    ERL_NIF_TERM items[cnt > 0 ? cnt : 1];
    va_list ap;
    va_start(ap, cnt);
    for (unsigned i = 0; i < cnt; i++) items[i] = va_arg(ap, ERL_NIF_TERM);
    va_end(ap);
    return enif_make_tuple_from_array(env, items, cnt);
}

static inline ERL_NIF_TERM enif_make_list(ErlNifEnv* env, unsigned cnt, ...) {
    ERL_NIF_TERM items[cnt > 0 ? cnt : 1];
    va_list ap;
    va_start(ap, cnt);
    for (unsigned i = 0; i < cnt; i++) items[i] = va_arg(ap, ERL_NIF_TERM);
    va_end(ap);
    return enif_make_list_from_array(env, items, cnt);
}

#define enif_make_tuple1(env, a) enif_make_tuple(env, 1, a)
#define enif_make_tuple2(env, a, b) enif_make_tuple(env, 2, a, b)
#define enif_make_tuple3(env, a, b, c) enif_make_tuple(env, 3, a, b, c)
#define enif_make_tuple4(env, a, b, c, d) enif_make_tuple(env, 4, a, b, c, d)
#define enif_make_tuple5(env, a, b, c, d, e) enif_make_tuple(env, 5, a, b, c, d, e)
#define enif_make_tuple6(env, a, b, c, d, e, f) enif_make_tuple(env, 6, a, b, c, d, e, f)
#define enif_make_list1(env, a) enif_make_list(env, 1, a)
#define enif_make_list2(env, a, b) enif_make_list(env, 2, a, b)
#define enif_make_list3(env, a, b, c) enif_make_list(env, 3, a, b, c)
#define enif_make_list4(env, a, b, c, d) enif_make_list(env, 4, a, b, c, d)

/* Each library registers itself before main; the runtime binds it to its
 * module when the module's @on_load calls :erlang.load_nif/2. */
void tonic_nif_register(const ErlNifEntry* entry);

#define TONIC_NIF_STRING(x) #x
#define TONIC_NIF_NAME(x) TONIC_NIF_STRING(x)

#define ERL_NIF_INIT(NAME, FUNCS, LOAD, RELOAD, UPGRADE, UNLOAD)                     \
    static ErlNifEntry tonic_nif_entry = {                                          \
        ERL_NIF_MAJOR_VERSION, ERL_NIF_MINOR_VERSION, TONIC_NIF_NAME(NAME),          \
        sizeof(FUNCS) / sizeof(*FUNCS), FUNCS, LOAD, RELOAD, UPGRADE, UNLOAD};       \
    __attribute__((constructor)) static void tonic_nif_init(void) {                 \
        tonic_nif_register(&tonic_nif_entry);                                       \
    }

#ifdef __cplusplus
}
#endif

#endif
