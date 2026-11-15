//! C ABI for the frame-state tracer, so a C or C++ D3D11 vtable hook can drive
//! the Rust core across an FFI boundary. View and resource ids are u64 (the
//! ID3D11 pointers in a live hook); id 0 is reserved to mean null / unbound.
//!
//! ViewKind codes: 0=Srv 1=Rtv 2=Dsv 3=DsvReadOnly 4=Uav.
//! Stage codes:    0=Vs  1=Ps  2=Cs  3=Gs 4=Hs 5=Ds.

use std::os::raw::{c_char, c_int};

use crate::{Claim, Event, FrameState, HazardKind, ResourceId, Snapshot, Stage, Verdict, ViewId, ViewKind};

fn view_opt(id: u64) -> Option<ViewId> {
    if id == 0 {
        None
    } else {
        Some(ViewId(id))
    }
}

fn kind_from(k: c_int) -> ViewKind {
    match k {
        1 => ViewKind::Rtv,
        2 => ViewKind::Dsv,
        3 => ViewKind::DsvReadOnly,
        4 => ViewKind::Uav,
        _ => ViewKind::Srv,
    }
}

fn stage_from(s: c_int) -> Stage {
    match s {
        1 => Stage::Ps,
        2 => Stage::Cs,
        3 => Stage::Gs,
        4 => Stage::Hs,
        5 => Stage::Ds,
        _ => Stage::Vs,
    }
}

unsafe fn ids(ptr: *const u64, n: usize) -> Vec<Option<ViewId>> {
    if ptr.is_null() || n == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(ptr, n)
            .iter()
            .map(|&id| view_opt(id))
            .collect()
    }
}

/// Create a new tracer. Free with ft_free.
#[no_mangle]
pub extern "C" fn ft_new() -> *mut FrameState {
    Box::into_raw(Box::new(FrameState::new()))
}

/// Free a tracer created by ft_new.
#[no_mangle]
pub unsafe extern "C" fn ft_free(state: *mut FrameState) {
    if !state.is_null() {
        drop(Box::from_raw(state));
    }
}

#[no_mangle]
pub unsafe extern "C" fn ft_register_view(state: *mut FrameState, view: u64, resource: u64, kind: c_int) {
    if let Some(s) = state.as_mut() {
        s.apply(Event::RegisterView {
            view: ViewId(view),
            resource: ResourceId(resource),
            kind: kind_from(kind),
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn ft_set_shader_resources(state: *mut FrameState, stage: c_int, start: u32, views: *const u64, n: usize) {
    if let Some(s) = state.as_mut() {
        s.apply(Event::SetShaderResources {
            stage: stage_from(stage),
            start_slot: start,
            views: ids(views, n),
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn ft_set_unordered_access_views(state: *mut FrameState, start: u32, views: *const u64, n: usize) {
    if let Some(s) = state.as_mut() {
        s.apply(Event::SetUnorderedAccessViews {
            start_slot: start,
            views: ids(views, n),
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn ft_set_render_targets(state: *mut FrameState, rtvs: *const u64, n: usize, dsv: u64) {
    if let Some(s) = state.as_mut() {
        s.apply(Event::SetRenderTargets {
            rtvs: ids(rtvs, n),
            dsv: view_opt(dsv),
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn ft_draw(state: *mut FrameState) {
    if let Some(s) = state.as_mut() {
        s.apply(Event::Draw);
    }
}

#[no_mangle]
pub unsafe extern "C" fn ft_dispatch(state: *mut FrameState) {
    if let Some(s) = state.as_mut() {
        s.apply(Event::Dispatch);
    }
}

/// Number of hazards in the current binding state.
#[no_mangle]
pub unsafe extern "C" fn ft_hazard_count(state: *const FrameState) -> usize {
    state.as_ref().map(|s| s.hazards().len()).unwrap_or(0)
}

/// Kind of the i-th current hazard: 0 = ReadWrite, 1 = WriteWrite, -1 = none.
#[no_mangle]
pub unsafe extern "C" fn ft_hazard_kind(state: *const FrameState, i: usize) -> c_int {
    if let Some(s) = state.as_ref() {
        if let Some(h) = s.hazards().get(i) {
            return match h.kind {
                HazardKind::ReadWrite => 0,
                HazardKind::WriteWrite => 1,
            };
        }
    }
    -1
}

/// Name of the i-th current hazard kind ("ReadWrite"/"WriteWrite"/"none").
/// Single source of truth for the kind encoding so C/C++ callers never hardcode it.
#[no_mangle]
pub unsafe extern "C" fn ft_hazard_kind_name(state: *const FrameState, i: usize) -> *const c_char {
    let s = match state.as_ref().and_then(|st| st.hazards().get(i).map(|h| h.kind)) {
        Some(HazardKind::ReadWrite) => c"ReadWrite",
        Some(HazardKind::WriteWrite) => c"WriteWrite",
        None => c"none",
    };
    s.as_ptr()
}

/// Resource id of the i-th current hazard, or 0 if out of range.
#[no_mangle]
pub unsafe extern "C" fn ft_hazard_resource(state: *const FrameState, i: usize) -> u64 {
    if let Some(s) = state.as_ref() {
        if let Some(h) = s.hazards().get(i) {
            return h.resource.0;
        }
    }
    0
}

/// Capture the current bind state as a snapshot (free with ft_snapshot_free).
#[no_mangle]
pub unsafe extern "C" fn ft_snapshot(state: *const FrameState) -> *mut Snapshot {
    match state.as_ref() {
        Some(s) => Box::into_raw(Box::new(s.snapshot())),
        None => std::ptr::null_mut(),
    }
}

/// Free a snapshot from ft_snapshot.
#[no_mangle]
pub unsafe extern "C" fn ft_snapshot_free(snap: *mut Snapshot) {
    if !snap.is_null() {
        drop(Box::from_raw(snap));
    }
}

/// Slots that differ between a SAVED and a RESTORED snapshot (0 = transparent).
#[no_mangle]
pub unsafe extern "C" fn ft_restore_leak_count(saved: *const Snapshot, restored: *const Snapshot) -> usize {
    match (saved.as_ref(), restored.as_ref()) {
        (Some(s), Some(r)) => s.diff_restore(r).len(),
        _ => 0,
    }
}

/// Write the first restore leak witness into buf (NUL-terminated); return bytes
/// written (excluding NUL), or 0 if there is no leak.
#[no_mangle]
pub unsafe extern "C" fn ft_restore_first_leak(
    saved: *const Snapshot,
    restored: *const Snapshot,
    buf: *mut c_char,
    len: usize,
) -> usize {
    if buf.is_null() || len == 0 {
        return 0;
    }
    let leaks = match (saved.as_ref(), restored.as_ref()) {
        (Some(s), Some(r)) => s.diff_restore(r),
        _ => Vec::new(),
    };
    let first = match leaks.first() {
        Some(l) => l,
        None => return 0,
    };
    let msg = format!("{}", first);
    let bytes = msg.as_bytes();
    let n = bytes.len().min(len - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
    *buf.add(n) = 0;
    n
}

/// Mark a frame boundary (Present); evaluates temporal invariants for the frame.
#[no_mangle]
pub unsafe extern "C" fn ft_present(state: *mut FrameState) {
    if let Some(s) = state.as_mut() {
        s.apply(Event::Present);
    }
}

/// Declare two resources (ID3D11Resource pointers as u64) as a history pair.
#[no_mangle]
pub unsafe extern "C" fn ft_declare_history_pair(state: *mut FrameState, a: u64, b: u64) {
    if let Some(s) = state.as_mut() {
        s.declare_history_pair(ResourceId(a), ResourceId(b));
    }
}

/// Number of temporal violations witnessed so far.
#[no_mangle]
pub unsafe extern "C" fn ft_temporal_violation_count(state: *const FrameState) -> usize {
    state.as_ref().map(|s| s.temporal_violations().len()).unwrap_or(0)
}

/// Write the i-th temporal violation witness into buf; return bytes written.
#[no_mangle]
pub unsafe extern "C" fn ft_temporal_witness(state: *const FrameState, i: usize, buf: *mut c_char, len: usize) -> usize {
    if buf.is_null() || len == 0 {
        return 0;
    }
    let s = match state.as_ref() {
        Some(s) => s,
        None => return 0,
    };
    let v = match s.temporal_violations().get(i) {
        Some(v) => v,
        None => return 0,
    };
    let msg = format!("{}", v);
    let bytes = msg.as_bytes();
    let n = bytes.len().min(len - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
    *buf.add(n) = 0;
    n
}

/// Adjudicate a claim string against observed state. Returns 0=Confirmed,
/// 1=Contradicted, 2=Unresolvable; writes the witness into buf (NUL-terminated).
#[no_mangle]
pub unsafe extern "C" fn ft_adjudicate(state: *const FrameState, claim: *const c_char, buf: *mut c_char, len: usize) -> c_int {
    let s = match state.as_ref() {
        Some(s) => s,
        None => return 2,
    };
    if claim.is_null() {
        return 2;
    }
    let claim_str = std::ffi::CStr::from_ptr(claim).to_string_lossy().into_owned();
    let verdict = s.adjudicate(&Claim::parse(&claim_str));
    let (code, witness) = match &verdict {
        Verdict::Confirmed => (0, String::new()),
        Verdict::Contradicted { observed } => (1, observed.clone()),
        Verdict::Unresolvable => (2, "not instrumented".to_string()),
    };
    if !buf.is_null() && len > 0 {
        let b = witness.as_bytes();
        let n = b.len().min(len - 1);
        std::ptr::copy_nonoverlapping(b.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    code
}
