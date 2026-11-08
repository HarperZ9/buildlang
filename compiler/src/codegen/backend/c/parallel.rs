//! The C side of `par_for` and `par_map` (M5): a fixed thread pool and one
//! wrapper per parallel body.
//!
//! Lowering turns each call into `build_par_for__<f>(n, ctx)` or
//! `build_par_map__<f>(xs)`. This file emits those wrappers and, once per
//! program that uses them, the pool. A wrapper allocates the result vector,
//! and the pool hands out contiguous index ranges; the body's result for index
//! `i` goes to slot `i`. No two indices share a slot, and the body is the same C
//! function in every mode, so the vector equals the serial one byte for byte.
//!
//! `BUILD_PAR_THREADS` sets the thread count (default: the logical processor
//! count, at most 256). `BUILD_PAR_THREADS=1` runs every body on the calling
//! thread, in index order. A call made while the pool is busy (a body that
//! itself calls `par_for`) runs serially on its own thread.

use std::fmt::Write as _;

use super::CBackend;
use crate::codegen::ir::{MirModule, MirTerminator, MirValue};
use crate::codegen::lower::parallel::{PAR_FOR_PREFIX, PAR_MAP_PREFIX, PAR_MAP_REF_PREFIX};

/// Which wrapper shape a call site needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParKind {
    For,
    Map,
    MapRef,
}

/// The pool. Plain mutex and condition variables (Win32 or pthreads); the
/// chunk counter is taken under the mutex, which costs nothing next to a chunk.
pub(super) const PAR_RUNTIME: &str = r#"
// --- M5 parallel runtime: a fixed thread pool for par_for / par_map ---
#ifndef _WIN32
#include <pthread.h>
#include <unistd.h>
#endif
typedef void (*BuildParBody)(void* env, int64_t lo, int64_t hi);
#ifdef _WIN32
static CRITICAL_SECTION bl_par_mu;
static CONDITION_VARIABLE bl_par_cv_work;
static CONDITION_VARIABLE bl_par_cv_done;
#define BL_PAR_LOCK() EnterCriticalSection(&bl_par_mu)
#define BL_PAR_UNLOCK() LeaveCriticalSection(&bl_par_mu)
#define BL_PAR_WAIT(cv) SleepConditionVariableCS(&(cv), &bl_par_mu, INFINITE)
#define BL_PAR_WAKE_ALL(cv) WakeAllConditionVariable(&(cv))
#else
static pthread_mutex_t bl_par_mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t bl_par_cv_work = PTHREAD_COND_INITIALIZER;
static pthread_cond_t bl_par_cv_done = PTHREAD_COND_INITIALIZER;
#define BL_PAR_LOCK() pthread_mutex_lock(&bl_par_mu)
#define BL_PAR_UNLOCK() pthread_mutex_unlock(&bl_par_mu)
#define BL_PAR_WAIT(cv) pthread_cond_wait(&(cv), &bl_par_mu)
#define BL_PAR_WAKE_ALL(cv) pthread_cond_broadcast(&(cv))
#endif
static int bl_par_threads = 0;
static int bl_par_busy = 0;
static unsigned long bl_par_gen = 0;
static int bl_par_pending = 0;
static BuildParBody bl_par_body = NULL;
static void* bl_par_env = NULL;
static int64_t bl_par_n = 0;
static int64_t bl_par_next = 0;
static int64_t bl_par_chunk = 1;

static void bl_par_drain(void) {
    for (;;) {
        BL_PAR_LOCK();
        if (bl_par_next >= bl_par_n) { BL_PAR_UNLOCK(); return; }
        int64_t lo = bl_par_next;
        int64_t hi = lo + bl_par_chunk;
        if (hi > bl_par_n) hi = bl_par_n;
        bl_par_next = hi;
        BuildParBody body = bl_par_body;
        void* env = bl_par_env;
        BL_PAR_UNLOCK();
        (*body)(env, lo, hi);
    }
}

static void bl_par_worker_loop(void) {
    unsigned long seen = 0;
    BL_PAR_LOCK();
    for (;;) {
        while (bl_par_gen == seen) BL_PAR_WAIT(bl_par_cv_work);
        seen = bl_par_gen;
        BL_PAR_UNLOCK();
        bl_par_drain();
        BL_PAR_LOCK();
        bl_par_pending--;
        if (bl_par_pending == 0) BL_PAR_WAKE_ALL(bl_par_cv_done);
    }
}

#ifdef _WIN32
static DWORD WINAPI bl_par_worker(LPVOID arg) { (void)arg; bl_par_worker_loop(); return 0; }
#else
static void* bl_par_worker(void* arg) { (void)arg; bl_par_worker_loop(); return NULL; }
#endif

static int bl_par_cpu_count(void) {
#ifdef _WIN32
    SYSTEM_INFO si; GetSystemInfo(&si); return (int)si.dwNumberOfProcessors;
#else
    long c = sysconf(_SC_NPROCESSORS_ONLN); return c > 0 ? (int)c : 1;
#endif
}

static void bl_par_init(void) {
    if (bl_par_threads != 0) return;
    int t = 0;
    const char* env = getenv("BUILD_PAR_THREADS");
    if (env && *env) t = atoi(env);
    if (t <= 0) t = bl_par_cpu_count();
    if (t > 256) t = 256;
#ifdef _WIN32
    InitializeCriticalSection(&bl_par_mu);
    InitializeConditionVariable(&bl_par_cv_work);
    InitializeConditionVariable(&bl_par_cv_done);
#endif
    for (int i = 1; i < t; i++) {
#ifdef _WIN32
        HANDLE h = CreateThread(NULL, 0, bl_par_worker, NULL, 0, NULL);
        if (h == NULL) { t = i; break; }
        CloseHandle(h);
#else
        pthread_t th;
        if (pthread_create(&th, NULL, bl_par_worker, NULL) != 0) { t = i; break; }
        pthread_detach(th);
#endif
    }
    bl_par_threads = t;
}

// Run body over [0, n) in contiguous chunks on the pool, and return when
// every chunk is done.
static void build_par_run(int64_t n, BuildParBody body, void* env) {
    if (n <= 0) return;
    bl_par_init();
    if (bl_par_threads <= 1 || n == 1) { (*body)(env, 0, n); return; }
    BL_PAR_LOCK();
    if (bl_par_busy) { BL_PAR_UNLOCK(); (*body)(env, 0, n); return; }
    bl_par_busy = 1;
    bl_par_body = body;
    bl_par_env = env;
    bl_par_n = n;
    bl_par_next = 0;
    bl_par_chunk = n / ((int64_t)bl_par_threads * 8);
    if (bl_par_chunk < 1) bl_par_chunk = 1;
    bl_par_pending = bl_par_threads - 1;
    bl_par_gen++;
    BL_PAR_WAKE_ALL(bl_par_cv_work);
    BL_PAR_UNLOCK();
    bl_par_drain();
    BL_PAR_LOCK();
    while (bl_par_pending > 0) BL_PAR_WAIT(bl_par_cv_done);
    bl_par_busy = 0;
    BL_PAR_UNLOCK();
}

// A vector of n zeroed elements, filled in place by a parallel wrapper.
static BuildVecHandle build_par_vec_alloc(int64_t n, size_t elem_size) {
    BuildVecHandle h;
    size_t count = n > 0 ? (size_t)n : 0;
    h.inner = (BuildVec*)malloc(sizeof(BuildVec));
    h.inner->ptr = calloc(count ? count : 1, elem_size);
    if (!h.inner->ptr) { fprintf(stderr, "par: out of memory\n"); exit(101); }
    h.inner->len = count;
    h.inner->cap = count;
    h.inner->elem_size = elem_size;
    return h;
}
"#;

impl CBackend {
    /// The wrapper names every call site in the module needs, sorted.
    fn par_sites(module: &MirModule) -> Vec<(String, ParKind, String)> {
        let mut sites = std::collections::BTreeMap::new();
        for func in &module.functions {
            for block in func.blocks.iter().flatten() {
                let Some(MirTerminator::Call {
                    func: MirValue::Function(name),
                    ..
                }) = &block.terminator
                else {
                    continue;
                };
                let found = [
                    (PAR_MAP_REF_PREFIX, ParKind::MapRef),
                    (PAR_MAP_PREFIX, ParKind::Map),
                    (PAR_FOR_PREFIX, ParKind::For),
                ]
                .into_iter()
                .find_map(|(prefix, kind)| {
                    name.strip_prefix(prefix)
                        .map(|body| (kind, body.to_string()))
                });
                if let Some((kind, body)) = found {
                    sites.insert(name.to_string(), (kind, body));
                }
            }
        }
        sites
            .into_iter()
            .map(|(wrapper, (kind, body))| (wrapper, kind, body))
            .collect()
    }

    /// Emit the pool and one wrapper per parallel body. Emits nothing for a
    /// program that uses neither `par_for` nor `par_map`.
    pub(super) fn generate_par_wrappers(&mut self, module: &MirModule) {
        let sites = Self::par_sites(module);
        if sites.is_empty() {
            return;
        }
        self.output.push_str(PAR_RUNTIME);
        for (wrapper, kind, body) in sites {
            let Some(f) = module.functions.iter().find(|f| f.name.as_ref() == body) else {
                // Lowering only emits a call for a body it found; keep the C
                // compile error explicit if that ever breaks.
                writeln!(self.output, "#error \"parallel body {body} not found\"").unwrap();
                continue;
            };
            let callee = self.value_to_c(&MirValue::Function(f.name.clone()), &[]);
            let ret_c = self.type_to_c(&f.sig.ret);
            let env = format!("{wrapper}_env");
            let tramp = format!("{wrapper}_body");
            match kind {
                ParKind::For => {
                    let ctx_c = self.type_to_c(&f.sig.params[0]);
                    let idx_c = self.type_to_c(&f.sig.params[1]);
                    writeln!(
                        self.output,
                        "typedef struct {{ {ctx_c} ctx; {ret_c}* out; }} {env};\n\
                         static void {tramp}(void* e_, int64_t lo, int64_t hi) {{\n    \
                         {env}* e = ({env}*)e_;\n    \
                         for (int64_t i = lo; i < hi; i++) e->out[i] = {callee}(e->ctx, ({idx_c})i);\n}}\n\
                         static BuildVecHandle {wrapper}(int64_t n, {ctx_c} ctx) {{\n    \
                         BuildVecHandle h = build_par_vec_alloc(n, sizeof({ret_c}));\n    \
                         {env} e; e.ctx = ctx; e.out = ({ret_c}*)h.inner->ptr;\n    \
                         build_par_run(n, {tramp}, &e);\n    return h;\n}}"
                    )
                    .unwrap();
                }
                ParKind::Map | ParKind::MapRef => {
                    let elem_c = self.type_to_c(&f.sig.params[0]);
                    let (param, deref) = if kind == ParKind::MapRef {
                        ("BuildVecHandle* xs", "(*xs)")
                    } else {
                        ("BuildVecHandle xs", "xs")
                    };
                    writeln!(
                        self.output,
                        "typedef struct {{ const {elem_c}* in; {ret_c}* out; }} {env};\n\
                         static void {tramp}(void* e_, int64_t lo, int64_t hi) {{\n    \
                         {env}* e = ({env}*)e_;\n    \
                         for (int64_t i = lo; i < hi; i++) e->out[i] = {callee}(e->in[i]);\n}}\n\
                         static BuildVecHandle {wrapper}({param}) {{\n    \
                         int64_t n = (int64_t){deref}.inner->len;\n    \
                         BuildVecHandle h = build_par_vec_alloc(n, sizeof({ret_c}));\n    \
                         {env} e; e.in = (const {elem_c}*){deref}.inner->ptr; e.out = ({ret_c}*)h.inner->ptr;\n    \
                         build_par_run(n, {tramp}, &e);\n    return h;\n}}"
                    )
                    .unwrap();
                }
            }
        }
        self.output.push('\n');
    }
}
