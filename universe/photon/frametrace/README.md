# photon-frametrace

A symbol table for the D3D11 immediate-context frame.

The D3D11 immediate context is a giant implicit mutable state machine. Whether a
resource is bound as a shader-resource view (read) at the same moment it is also
bound as a render target, depth-stencil, or unordered-access view (write) is an
emergent property of every Set/Clear/Unbind that ran before the draw. That is
exactly the kind of long-range mutable state an LLM cannot track in its head, and
neither can a human without tooling, which is why RenderDoc and PIX exist.

This crate is that tooling as a small, tested state machine. Feed it the event
stream (from a D3D11 vtable hook, or a RenderDoc/PIX/ETW trace), then query
ground truth instead of reasoning about it:

- what is bound to PS t27 right now (srv_at)
- the live render targets, depth-stencil, and UAVs
- which resources are simultaneously read and written at this draw (hazards)

## Status

v1 core plus tests (13 tests, all passing under cargo). The Rust core exposes a
surface a thin C ABI shim can call; the C++ D3D11 vtable hook that feeds it is
the next step (MSVC is now available locally).

Hazard model (v1): a resource bound as an SRV (read) and also as RTV/DSV/UAV
(write) is a ReadWrite hazard; a resource bound through two or more distinct
write views with no reader (e.g. RTV plus UAV) is a WriteWrite hazard. A
read-only DSV is correctly treated as neither read nor write. Each draw/dispatch
snapshots its hazards into a per-frame log (hazard_log / hazards_at).

## Run

    cargo test
    cargo run --example ssr_hazard

## Why it exists

An LLM has no symbol table and no heap; it approximates inference over text, so
it breaks exactly where state is mutated far from where it is read. The fix is
not to make it reason harder about state but to make it observe ground truth.
This crate is the observable substrate for the D3D11 frame.

## C ABI (for the D3D11 hook)

include/frametrace.h declares an extern "C" surface (ft_new, ft_register_view,
ft_set_shader_resources, ft_set_render_targets, ft_draw, ft_hazard_count, ...)
so a C or C++ D3D11 vtable hook can drive the Rust core across FFI. View and
resource ids are the ID3D11 pointers cast to uint64; id 0 means null/unbound.

Build the staticlib, then the C demo (Windows + MSVC):

    cargo build
    cmd /c c_demo\build.bat
    c_demo\demo.exe

c_demo/demo.c reproduces the SSR hazard through the C ABI and is verified to
compile (MSVC) and run, linking target/debug/photon_frametrace.lib. The required
native libs (kernel32 ntdll userenv ws2_32 dbghelp + msvcrt) come from
cargo rustc --lib -- --print native-static-libs.

Next: an ID3D11DeviceContext vtable hook (MinHook) that emits these events from a
live process. That step needs a running D3D11 app to exercise, so it is built
here but its runtime is validated against a target, not in CI.

## Trace ingestion (offline capture analysis)

The trace feature (on by default) ingests a normalized JSON trace -- what a
RenderDoc / PIX / ETW adapter emits -- and replays it into a FrameState:

    cargo run --example replay_trace -- tests/data/ssr_trace.json

A trace is a JSON array of op-tagged events: register_view, set_shader_resources,
set_render_targets, set_uav, clear_rtv, clear_dsv, draw, dispatch. Ids are u64
(0 means null/unbound). FrameState::replay_json applies the whole trace; then
query hazard_log / hazards_at for the per-draw verdict. For a dependency-free
core, build with --no-default-features (drops serde and trace ingestion).

## D3D11 hook (live capture) -- build-verified

hook/frametrace_hook.cpp is a proxy / injected DLL that hooks the shared
ID3D11DeviceContext vtable (pointer swap, no MinHook needed) and feeds every
Set/Draw/Dispatch to the C ABI, surfacing live hazards via OutputDebugString.

    cargo build
    cmd /c hook\build.bat     (produces hook/frametrace_hook.dll)

BUILD-VERIFIED ONLY: it compiles and links against d3d11.lib and the frametrace
staticlib, but its runtime is validated by injecting it into a live D3D11 process
(or renaming it to a proxy d3d11.dll), not in CI. View-to-resource mapping uses
GetResource(). GS/HS/DS SRV stages and OMSetRenderTargetsAndUnorderedAccessViews
are one-line additions following the same slot pattern.

## RenderDoc adapter (offline trace from a capture)

adapters/renderdoc_to_frametrace.py converts a RenderDoc .rdc capture into the
JSON trace format, for replay without a live process:

    python adapters/renderdoc_to_frametrace.py capture.rdc > trace.json
    cargo run --example replay_trace -- trace.json

It walks every draw/dispatch, reads the bound D3D11 views per stage, and assigns
a distinct view id per (resource, kind) so a resource used as both SRV and RTV
keeps its hazard. UNVERIFIED HERE: it targets the documented RenderDoc Python API
but has not been run against a live capture in this environment (no RenderDoc);
syntax is checked, API field names should be confirmed against your version.

Hardened hook coverage: PS/VS/CS/GS/HS/DS SetShaderResources, CSSetUnorderedAccessViews,
OMSetRenderTargets, OMSetRenderTargetsAndUnorderedAccessViews (with KEEP guards),
Draw/DrawIndexed/DrawInstanced/DrawIndexedInstanced/DrawAuto, Dispatch. Read-only
DSV is detected via GetDesc (D3D11_DSV_READ_ONLY_DEPTH) and excluded from hazards.

## Restore-verify (Tier-1: is an effect transparent to the host?)

An effect (or any mid-frame dispatch) saves the context state, does its work,
and restores. The invariant: restore is transparent -- after it, every slot
holds what the game set. A forgotten slot (the classic "left SSR bound to t27")
runs the game with state it never set: host corruption, and the most expensive
failure because the game did not cause it.

    let saved = fs.snapshot();      // before the effect
    ... effect binds/draws/restores ...
    let restored = fs.snapshot();   // after the restore
    for leak in saved.diff_restore(&restored) {
        // e.g. "Ps t27 saved=res#21042 (view#10) restored=NULL"
    }

cargo run --example restore_verify demonstrates it. C ABI: ft_snapshot /
ft_restore_leak_count / ft_restore_first_leak -- a RAW effect calls these around
its D3D11StateBackup save/restore so non-transparency becomes a logged witness,
not a guess. (RestoreUnobserved -- a restore via a path the membrane does not
hook -- is the hook-coverage half, flagged where the boundary is wired.)
## Temporal ping-pong (Tier-2: the most-stateful, user-shipping failure)

History-buffer effects (TAA, temporal accumulation, reprojection) read frame
N-1 and write frame N, alternating two buffers. Declare the pair and call
ft_present per frame; three invariants are checked at each frame boundary:

- within-frame feedback -- a buffer read AND written the same frame (aliasing);
- swap-desync -- the read buffer did not alternate from last frame: the ping-pong
  is stuck (stale history -> ghosting). PROVABLY unreachable by a single-frame
  capture, because it is a frame-N-vs-N-1 property;
- uninitialized-read -- a buffer read before any write/clear (warmup bug); this
  is where Clear stops being a no-op and becomes a warmup witness.

    fs.declare_history_pair(res_a, res_b);
    ... per frame: read one, write the other, Event::Present ...
    for v in fs.temporal_violations() { /* "frame 2 SwapDesync ... member res#10" */ }

cargo run --example temporal_stuck demonstrates the stuck buffer firing on
frame 2. C ABI: ft_declare_history_pair / ft_present / ft_temporal_violation_count
/ ft_temporal_witness. JSON traces carry a "present" op for frame boundaries.
## NegativeLedger (Tier-5: the agent-facing gate -- instrument the model)

The deepest organ: it instruments the one stateful system the others do not --
the model's own confidence. A claim about runtime state is adjudicated against
what the membrane OBSERVED, returning one of three verdicts:

- CONFIRMED -- observed exactly the claim; safe to assert;
- CONTRADICTED -- instrumented for this state but did not observe the claim; the
  confident claim is BLOCKED with a witness (the membrane is authoritative, and
  prose context never overrides the live observation);
- UNRESOLVABLE -- not instrumented; assert neither true nor false.

This converts the core failure (confidence does not track accuracy) into a gate:
nothing about runtime state leaves as fact unless the membrane witnessed it.

    let v = fs.adjudicate(&Claim::parse("srv ps 27 21042"));
    if v.may_assert() { /* CONFIRMED */ } else { /* blocked: v carries the witness */ }

cargo run --example adjudicate replays the SSR capture (which UNBINDS t27) and
shows a confident "t27 is the SSR resource" claim being CONTRADICTED by the live
state -- the sharpest test, on a real trace. C ABI: ft_adjudicate.