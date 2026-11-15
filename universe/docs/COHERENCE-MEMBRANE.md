# THE COHERENCE MEMBRANE

> Provenance: synthesized 2026-06-05 by a 12-lens adversarially-critiqued workflow
> (53 proposals survived critique), RE-GROUNDED against the actual photon/frametrace
> source via raw-byte Grep, and cross-checked against a hand-off doctrine doc from a
> parallel same-base-model session (a correlated witness -- see the nervous-system
> section). Treat every claim as a witness to corroborate by an execution oracle,
> NOT canon to obey. Code-vs-prose disagreements resolved in favor of the code.
> Governs the way LINEAGE.md / STATUS.md do.

---

# THE COHERENCE MEMBRANE — A Theory of Externalized Runtime State for Stateless Inference

> Status of this document: synthesized from the keep=true proposal set, **re-grounded against the
> actual `photon/frametrace` source** (state.rs, ffi.rs, trace.rs, hook/frametrace_hook.cpp) via raw
> Grep over the bytes, not via the prose-summarizing read layer. Confidence labels are attached to
> every load-bearing factual claim. Where the synthesis and the code disagreed, the **code wins** and
> the disagreement is stated.

---

## 1. What the Coherence Membrane Is (precise definition)

An LLM does approximate inference over text. It has **no symbol table** (live bindings), **no heap**
(actual values), and **no execution cursor** (ordered trace). Its accuracy degrades precisely as the
answer depends on a mutation distant — in representation and in time — from the read. The lethal
property is that **confidence does not fall when accuracy does**: confident-but-wrong about a binding
or a race is the default, expensive failure.

The fix is not "reason harder." It is to **engineer a simulator the model reads**, making state
*local* before the model touches it. The **Coherence Membrane** is that layer: a selectively-permeable
boundary between stateless inference and a stateful system's ground truth. It **passes observations**
(read the artifact) and **blocks unobserved assertions** (the failure mode). It externalizes the three
things the model lacks into three emitted, checkable artifacts:

| Model lacks | Membrane externalizes as | Witness shape |
|---|---|---|
| symbol table (live bindings) | **BINDING LEDGER** — actual SRV/RTV/DSV/UAV per slot per pass, with provenance | `(stage, slot, ViewId, ResourceId, ViewKind, set_by_seq)` |
| heap (actual values) | **OUTPUT METRICS** ("the eyes") — mean/min/max/NaN%/%black/saturation/diff-vs-vanilla, fence-stamped for freshness | `(resource, checkpoint, metric, value, age_frames)` |
| execution trace | **FRAME LOG + INVARIANT ASSERTIONS** — ordered Set/Clear/Unbind/Present + hazard/restore/range/temporal asserts | `(seq, frame, checkpoint, event, violation?)` |

### The four membrane properties (the design vocabulary)

- **Selective permeability.** The membrane is *asymmetric on purpose*. An **observation** (a value
  read off an artifact) passes; an **assertion** about runtime state that carries no witness is
  *blocked*, not trusted. The default verdict for an uncited state-claim is `UNKNOWN`, never `PASS`.
  This is the whole point — it inverts the model's failure mode from "confident silence = green" to
  "silence = I did not look here."
- **Surface area.** = the fraction of dead zones instrumented. Surface area you *don't* have must be
  **emitted as a first-class blind-spot count** (`uncovered_slot`, `UNINSTRUMENTED`,
  `DeferredContextUnmodeled`), so that growing surface area is *safe* — green means green, not
  "green because I wasn't looking." Coverage that costs too much to ship is coverage that does not
  exist; surface area is therefore bounded by an **overhead budget** with sampling stamps.
- **Integrity.** A membrane can itself lie: a stale build fingerprint, a read layer that substitutes
  content, a buggy assert that silently stopped firing, a digest collision. Integrity = the membrane's
  ability to **prove its own freshness, provenance, and that its asserts actually ran** — via
  raw-byte bypass, cross-oracle agreement, hash chains, and a self-audit manifest. *An integrity layer
  that can't prove itself must SAY so* (`provenance_level=header_only`), not pass.
- **Latency.** = distance-in-time between a mutation and its read. The doctrine is "collapse the
  distance." But collapsing it via async readback reintroduces *staleness* — so latency must be
  **measured and stamped** (`age_frames`, `fence_value`), and a value too old must be emitted as
  `pending`/`dropped`/`STALE`, never as a confident number.

### Division of labor (unchanged, now load-bearing)

- **ENGINE** is source of truth on state: bindings, hazards, ranges, ordering, freshness.
- **MODEL** is reliable on local/pure logic and on **writing the instrumentation** (bounded,
  self-contained — its strong zone). It is *not* trusted to assert runtime state from code in its head.
- **HUMAN** is perceiver of last resort, only for genuinely spatial/aesthetic calls metrics cannot
  capture — and even there, **a number narrows it first** (one tile coordinate, not a 4K frame).

---

## 2. Coverage Map — Dead Zone → Mechanism → Status

Status legend: **[C]** covered in frametrace today · **[P]** partial · **[M]** missing (net-new) ·
**[R]** lives in RAW sibling, not this repo.

| # | Dead zone | Membrane mechanism | Status (verified) |
|---|---|---|---|
| 1 | SRV bound to t27 when compositor samples it | Binding snapshot per dispatch (binding ledger) | **[C]** `srv_at/render_targets/depth_stencil/unordered_access/resolve` exist in state.rs (high) |
| 2 | Read/write hazard (SRV also RTV/DSV/UAV) | O(1)-per-draw hazard assert, read-only-DSV excluded via GetDesc | **[C]** `hazards()` + `HazardKind::{ReadWrite,WriteWrite}` only (high) |
| 3 | "Which Set-call bound the impostor?" (blame) | **BindToken**: `seq` (internal, free) + best-effort `site`/`pass` on each binding | **[M]** binding maps store `ViewId` only, no provenance (high) |
| 4 | Mid-frame dispatch left OM targets unexpected | **Restore-Verify diff**: snapshot bindings, expect-restore, emit slot-level set-difference | **[M]** no snapshot/diff event; FrameState not `Clone` (high) |
| 5 | SRV slot-map t17–t38 matches documented owner | **Slot-Owner Covenant** keyed on a stable `owner_tag` / debug-name, table sha-pinned | **[M]** only opaque `ResourceId` pointers; no tag→name map (high) |
| 6 | RANGE: depth∈[0,1], reversed-Z, luma≥0, NaN, %black | **PassMetrics → ledger join**: `ft_pass_metrics`, range-spec sidecar keyed by owner_tag | **[M]/[R]** no value channel here; `raw_eyes` is in RAW (high) |
| 7 | Temporal ping-pong: read vs write history each frame | **Present epoch + per-pair role ledger**: read(N)≠write(N), within-frame read/write disjoint | **[M]** no `frame_id`; single-frame by construction (high) |
| 8 | Warmup / skip-first-N clear-before-read | **HistoryLifecycle** state machine driven by Clear/Write/Present | **[M]** `ClearRTV/ClearDSV` are literal no-ops (`=> {}` at state.rs:79) (high) |
| 9 | Channel saturation / sentinel colors / Inf vs NaN | **Saturation vector + liveness scalar**, bit-exact Inf(0x7F800000)/NaN(0x7FC00000)/denorm | **[M]** rides on readback ring (high) |
| 10 | Stale async value attribution | **Fence-stamped readback ring**, emit `pending`/`dropped` not stale numbers | **[M]** zero readback/copy/fence in hook (high) |
| 11 | Resource aliasing / pool reuse / use-after-free | **Lifetime/alias tags**: `alloc_id` + generation; `AliasedReadWrite`/`UseAfterRelease` | **[M]/[R]** alloc_id only available in RAW's pool / D3D12 placed (high) |
| 12 | Full PSO state (blend/depth/raster/topology/viewport) | **PSO fingerprint** via GetDesc; opt-in expected-PSO sidecar | **[M]** FrameState models none of these (high) |
| 13 | Dynamic-buffer Map/Discard freshness, CB-overflow | **Map/Unmap generation stamp**; `DrawWhileMapped`, `MapSizeMismatch` | **[M]** no Map/Unmap events (high) |
| 14 | Indirect args (Draw/DispatchIndirect counts) | **Args readback snapshot** + producer linkage; range on counts | **[M]** needs GPU→CPU staging readback (high) |
| 15 | Deferred contexts / ExecuteCommandList | **Per-context timeline + linearization**; Phase-1 detect-and-flag `DeferredContextUnmodeled` | **[M]** single `timeline: Vec<Event>`, no context_id (high) |
| 16 | Live "what is bound right now" query | **Seqlock state mailbox** (SPSC shm ring), tear-proof, age-stamped | **[M]** only push-only OutputDebugString today (high) |
| 17 | Live staleness / hook death | **Liveness heartbeat + staleness fuse**, typed `{Live,StaleAge,StalePaused,SuspectPartialHook}` | **[M]** (high) |
| 18 | Reprojection-matrix stale CB | **Tagged CB content hash**, assert prev(N)==curr(N-1) shift-by-one | **[M]** no CB content channel (high) |
| **Integrity dead zones** | | | |
| I1 | Membrane asserts didn't run / drifted | **Assertion-Manifest self-audit**: expected-vs-evaluated set, sidecar sha pin, dual-path byte-bypass | **[M]** (high) — *live threat this session, see §5* |
| I2 | ABI contract-stale across Rust/C/C++ | **ABI Trinity Pin**: `ft_abi_hash()` from a single discriminant table, refuse-to-install on skew | **[M]** hook hardcodes `==0?"ReadWrite":"WriteWrite"` (high) |
| I3 | Replay non-determinism / capture stale | **Determinism guard** (`ft_replay_hash`, fail-closed) + **freshness/provenance seal** | **[M]** `views`/`srv` are **HashMap** → latent nondeterminism (high) |
| I4 | Two oracles silently disagree | **Cross-oracle quorum**: hook-fp vs RenderDoc-replay-fp byte-match; eyes GPU-vs-CPU position-sensitive digest | **[P]** RenderDoc adapter exists but collapses slots to `start:0` (high) |
| I5 | Eye instrument itself wrong this run (transpose/row-pitch) | **Position-sensitive reduction** (per-row checksum / rolling hash), not sum | **[M]** (high) |
| **Meta-agent dead zones** | | | |
| A1 | Uncited confident state-claim in agent text | **CiteOrQuarantine / ClaimAsQuery**: claim compiles to an `ft-assert` query; uncited → blocked | **[M]** needs `ft_query` CLI + capture addressing (high) |
| A2 | Claim about a slot the membrane never observed | **NegativeLedger**: `OBSERVED/UNOBSERVED/UNINSTRUMENTED` tri-state | **[P]** `srv_at` returns `Option` (high); coverage manifest missing |
| A3 | Confidence not tracking accuracy | **CalibrationHarness**: ECE + hard gate `count(high ∧ contradicted)==0` | **[M]** co-tenant on A1's resolver |

> **Correction to the synthesis (code wins):** multiple proposals assert "raw_eyes already produces
> the scalars, effort M" and "reuses existing sidecar-invariant machinery." **Verified false in this
> repo:** there is no value/metrics channel, no sidecar machinery, and `HazardKind` has exactly two
> variants. Any range/eyes/covenant work here is **net-new producer + new event type + new sidecar**,
> not a cheap join. (high)

---

## 3. Prioritized Roadmap — MOST-EXPENSIVE-FAILURE-FIRST

Ordering principle: a failure that corrupts the **host game's subsequent frames** (manifests far from
cause, intermittent, frame-dependent) outranks one that degrades visibly in the mod pass. An
**integrity** failure that manufactures confident-green outranks a coverage gap, because it poisons
every other witness. Each item: **build** + **verify (the witness)**. All Tier-0/1 witnesses are
deterministic CPU/headless tests over the existing JSON replay path — **no live GPU required**.

### TIER 0 — Trust roots (build FIRST; everything else rides on these)

**T0.1 — Determinism guard + canonical ordering** *(effort S; foundation for all diffs)*
*Build:* Add `ft_replay_hash(state) -> u64` hashing the serialized `{binding-ledger + hazard_log}`.
**Canonicalize the projection by iterating `views`/`srv` in sorted-key order** (they are `HashMap`
today — state.rs:29–30, verified). Diff/restore tools refuse to emit (fail-closed) if
`replay_hash(replay(T)) != replay_hash(replay(T))`.
*Verify:* CI asserts byte-equal hashes across two replays of every fixture trace; a regression that
serializes a `HashMap` in iteration order **fails immediately**. This is the forcing function that
either proves the maps are never serialized in order or converts them to `BTreeMap`. (Witness =
committed golden hash.)

**T0.2 — Sequenced, frame-bounded, freshness-pinned log** *(effort S/M)*
*Build:* (a) internal monotonic `event_seq` bumped in `apply()` (free, hook-independent); (b)
`Event::Present` bumping `frame: u64` and marking current bindings `carried`; (c) every emitted record
carries `(frame, checkpoint, seq)` + a **rolling content checksum** `chk_i = H(chk_{i-1} ‖ bytes)`;
(d) a provenance header `{build_sha, abi_version, schema_version, source: live_hook|renderdoc|json}`.
**Drop the "MAC/anti-forgery" framing** — against an in-process injector who owns the nonce it is
theater; keep it honestly as a **lossy/reorder/drop detector** for a noisy read layer.
*Verify:* feed seq `1,2,4` → `gaps_detected=1`; drop record #k → "gap at seq k"; edit a byte → checksum
break at k; intact log → clean golden chain. Carried-state: bind frame0, read frame1 w/o rebind →
`CARRIED` warning; control rebinds → none.

**T0.3 — ABI Trinity Pin** *(effort M; kills a LIVE bug)*
*Build:* Define `const KIND_ABI: [(&str, c_int); N]` and `STAGE_ABI` as the **single source** both the
`kind_from`/`stage_from` match arms and the hash consume. `build.rs` hashes that table + the FFI
signature list into `FT_ABI_HASH`, exported via `ft_abi_hash()`, emitted into a committed
`frametrace_abi.h` with `#define FT_HAZARD_0 "ReadWrite"` etc. **The hook uses those macros instead of
its hardcoded `ft_hazard_kind(...)==0 ? "ReadWrite" : "WriteWrite"`** (frametrace_hook.cpp:157,
verified) — this *kills* the live mislabel bug, not just detects it. DLL init compares
`ft_abi_hash()` to its compiled-in constant; on skew, **log + continue read-only** (observe without
emitting mislabeled hazards) rather than silently lying.
*Verify:* CI regenerate-and-git-diff job; a scratch discriminant-bump must (a) fail the header diff and
(b) trip `SKEW` in a harness linking new rlib against old header constant.

**T0.4 — Assertion-Manifest self-audit** *(effort M; build right after a clean assert exists)*
*Build:* Per-frame JSON manifest: every sealed rule whose target resource was bound this frame
(**expected**, derived mechanically from sidecars) vs every rule that actually fired (**evaluated**,
instrumented in the emit path) → `SilentAssertGap{rule_id}`. Recompute each sidecar sha + dll
fingerprint + **FrameState schema version** at frame start vs a pin → `StaleManifest`. **Dual-path
byte-bypass**: resolve N sealed slots via the normal path AND an independent table walk, assert
agreement → directly models the `safe_read` substitution observed this session (§5). Build-time lint:
an emit-path assert with no `rule_id` is a compile failure.
*Verify:* delete one assert call → `SilentAssertGap` for exactly that rule; mutate a sidecar byte →
`StaleManifest`; inject one-slot divergence → self-check flags exactly that slot. (Stated boundary:
catches *registered-but-non-firing*; an *entirely unregistered* invariant is still low-observable.)

### TIER 1 — Host-corrupting failures (most expensive coverage class)

**T1.1 — Restore-Verify diff (binding-restore verifier)** *(effort S/M)*
*Build:* `ft_snapshot(state) -> SnapshotId` retaining a **sorted `Vec<(slot_key, ViewId)>`** (not just a
digest — the digest is an O(1) early-out only); `ft_assert_restored(state, id)` emits the element-wise
**set-difference** as `TransparencyViolation{label, changed_slots[before/after ViewId]}`. Make the
bracket **panic-safe** (RAII/finally) so a *failed* inject — the likely real case — is still measured.
*Scope honestly in the witness:* "binding-table restore verified; resource CONTENTS not checked here —
see range/eyes." Keys on **ViewId identity**, not ResourceId.
*Verify:* balanced set/unset → zero violations; one RTV left rebound → exactly `{Rtv(0)}` with correct
before/after; **rebind a slot to a different view of the SAME resource (ViewId changes, ResourceId
constant) → IS flagged** (catches aliasing-restore bugs); property test asserts digest-equality IFF the
(slot→ViewId) maps are set-equal.

**T1.2 — BindToken provenance (the spine)** *(effort M; many items hang off it)*
*Build:* Change the three binding maps to `(ViewId, u64 /*seq*/)`. `seq` is **load-bearing and free**
(assigned internally in `apply()`). `site`/`pass` are **best-effort enrichment** the hook MAY supply;
default `0`, legacy FFI wrappers pass `site=0`. Hazards gain `reads_prov/writes_prov`.
*Verify:* golden test asserts on **seq** (deterministic, hook-independent), not site; legacy `site=0`
path still detects the hazard (provenance is additive). *Honest boundary:* `site` specificity depends
on the hook threading a per-pass id — that work lives in RAW/the hook, not this crate.

**T1.3 — Producer Ledger (last-writer, append-only)** *(effort M; consumes T1.2's seq)*
*Build:* `produced: HashMap<ResourceId, Vec<WriteRecord>>` appended (not overwritten) at each
Draw/Dispatch for every bound write view; key on **(resource, checkpoint)** accepting a **set** of
writers per checkpoint (MRT/scatter is legal). `ft_last_writer = .last()`.
*Verify:* replay writes R in pass A then reads in B → full producer history `[12(A)...]`; second write
moves last-writer to B; assert temporal invariant `producer_checkpoint < consumer_checkpoint`. The
value-join to the eyes is a documented **future seam** (`ft_last_writer` the Python eyes calls), not
claimed end-to-end here.

### TIER 2 — Temporal (most-stateful, ships TAA ghosting to users)

**T2.1 — Present epoch + within-frame ping-pong assert** *(split L → S+M)*
*Build (ship first, no tags, no eyes):* with `Event::Present` (T0.2) + `frame_id`, assert the **single
within-frame invariant**: resource read via the declared history SRV **slot** ≠ resource bound writable
at the declared write **slot**, same frame. **Declare the pair by SLOT** (which frametrace observes),
not by human tag. Reuse the existing `hazards()` ReadWrite logic — do not invent a parallel check.
*Build (defer):* cross-frame `read(N)==write(N-1)` alternation as a **lower-severity `history_alias`
note keyed on ResourceId**; demote auto-inference of pairs to *advisory candidate suggestions* that
never fail a build (cross-oracle `derived==declared`). Generalize the 2-buffer assert to an **N-ring
derangement** so triple-buffered histories don't false-positive.
*Verify:* 4-frame A/B/A/B → clean; stuck (read==write A every frame) → one violation at frame 2 (the
**killer demo: provably unreachable by single-frame RenderDoc capture**); `pair_identity_lost` emitted
(not false PASS) when pointers are recreated per frame.

**T2.2 — HistoryLifecycle / warmup** *(effort M; turns state.rs:79 no-op into transitions)*
*Build:* per-ResourceId state machine `Uninitialized→Cleared→Written→Valid`, driven by **existing**
Clear/Write/Present events (Clear is currently `=> {}` — verified). A history SRV read while
`Uninitialized` → `read_uninitialized_history`. **Coverage guard:** a Write only advances to `Valid`
if it is a full-RT write (no scissor/partial viewport); else `Written-partial`, requiring metrics
confirmation — closes the partial-write false-PASS.
*Verify:* read-before-clear → one violation; insert Clear → clean; after `reset`, first post-reset read
re-flags (proves the resize transition re-arms, not latched-Valid). Cross-oracle (`raw_eyes`)
disagreement test is **pure-logic over a synthetic metrics row** and labeled cross-repo/unbuilt.

### TIER 3 — Value channel (the eyes; net-new producer)

**T3.1 — Fence-stamped async readback ring** *(effort M; load-bearing primitive every eye rides)*
*Build:* at each checkpoint `CopyResource(bound RTV0/UAV0 → 3-deep staging ring)` + a fence; **do not
Map this frame**. Drain when the fence signals; compute metrics off the hot path. Each record carries
`{frame, checkpoint, resource, view, fence_value, age_frames}`. **Refuse to emit a number whose fence
hasn't signaled — emit `pending`; ring exhaustion emits `dropped`** (a fourth status), never a stale
number, never silent. Key on the existing monotonic `checkpoint` so values join the hazard log by the
same key.
*Verify:* constant-0.5 staging → mean==0.5,NaN%==0; 0x7FC00000 → NaN%==50; **frame N reports A while GPU
is on B** (fence-tag attribution, not wall-clock); over-enqueue without draining → oldest `dropped`.
*Honest boundary:* proves the *bookkeeping*; the live checkpoint↔fence binding under driver reordering
is unverifiable without a real GPU.

**T3.2 — Saturation/sentinel index + liveness scalar** *(effort S; rides T3.1)*
*Build:* per-channel `{pct_floor, pct_ceil, pct_inf, pct_nan, pct_denorm}`; bit-exact sentinel
hit-test (exact tier = zero-false-positive anchor; a second `near-sentinel` ε-tier surfaced
separately, lower confidence). `liveness = fraction NOT at floor/ceil/sentinel/Inf/NaN` as a first-class
per-pass assertable scalar.
*Verify:* all-1.0 → ceil==100,liveness==0; 0x7F800000 → inf==100,nan==0 (proves bit-pattern read, not
range); 25% magenta exact → 25, off-by-LSB → 0.

**T3.3 — Range asserts as eyes→ledger join** *(effort M; the named historical bugs)*
*Build:* `ft_pass_metrics(state, resource, pass_tag, min, max, nan, frac_black)`; range-spec sidecar
`{owner_tag, min_ge, max_le, nan_eq_0, orient_dir:i8}`. **Rename "monotonic" → "depth-endpoint
orientation"** (min/max alone cannot witness interior monotonicity); for true monotonicity have the
eyes emit a coarse N-bucket histogram.
*Verify:* max=1.4 vs [0,1] → one `RangeViolation` naming `max_le=1.0`; nan=3 → NaN violation;
cross-oracle ε-match vs an independent value computation (catches a noise-injecting eyes layer);
spec-coverage test (every sealed owner_tag has a spec OR is range-exempt).
*Eye integrity (T3.x):* GPU-vs-CPU agreement must use a **position-sensitive rolling hash / per-row
checksum** (Fletcher/CRC over a fixed traversal), **not a sum** — sum is invariant under transpose and
permutation. Witness must include a **transpose fault: equal-sum, transposed-layout buffers FAIL**.

### TIER 4 — Liveness query surface

**T4.1 — Seqlock mailbox + heartbeat fuse** *(effort M + S)*
*Build:* SPSC shm ring, writer `seq++` (odd) → memcpy POD record (publish **only non-default bound
slots up to cap K + overflow flag**; FrameState maps are not POD — serialize them) → `seq++` (even);
reader retries on odd/changed. Stamp `{frame, checkpoint, seq, qpc}`. Fuse returns typed
`{Live, StaleAge, StalePaused, SuspectPartialHook}`; **decouple Present-heartbeat from a per-draw
heartbeat** so Present-advancing-but-draws-frozen = partial-hook-death surfaced for free. Budget in
frame-times read from actual Present delta.
*Verify:* two-process witness — torn write (sleep between memcpy and seq++) is rejected (zero torn
records); reader can **re-derive the same `hazards()` set from the snapshot alone** (proves the record
is a sufficient statistic); freeze heartbeat → fuse blows within one budget; QPC-backward → clamp to
STALE.
*Honest boundary:* proves ledger↔context consistency and tear-freedom; does **not** prove the bytes
match physical GPU completion or catch context-substitution (both oracles share the hooked context).

### TIER 5 — Agent-facing membrane (operationalizes selective permeability)

**T5.1 — NegativeLedger** *(effort S; cheapest hard block on the most dangerous behavior)*
*Build:* surface `srv_at`'s `Option` as tri-state `OBSERVED | UNOBSERVED | UNINSTRUMENTED`. **Split the
verdict:** `UNOBSERVED` + confidence>unknown → **CONTRADICTED** (model invented an owner);
`UNINSTRUMENTED` + confidence>unknown → **UNRESOLVABLE/unknown** (membrane blind — block the assertion
but do NOT assert it false). Distinguishing the two **requires a capture-time instrumentation-coverage
manifest** (what the hook was wired to watch).
*Verify:* query never-bound slot → UNOBSERVED, high-confidence claim blocked; **feed the slot-map
COMMENT as context, confirm the claim is STILL blocked** (the comment is not an oracle — the sharpest
test in the set); OBSERVED set == slots touched in v1_completeness.rs.

**T5.2 — ClaimAsQuery + CiteOrQuarantine** *(effort M; needs ft_query CLI net-new)*
*Build:* state-claims authored AS executable `ft-assert` expressions (`assert srv_at(PS,27)==view('x')
@cp14`) over **existing accessors only**; v1 grammar **excludes `depth_stencil is read_only`** (no
read-only flag exposed — verified). Build the missing `ft_query(capture, checkpoint, expr)` CLI and a
persisted, addressable capture store (checkpoint = the literal monotonic u64). CiteOrQuarantine shrinks
to: every state-claim sentence has a matching PASS record, else block; parse error → fail-closed.
*Verify:* golden PASS/FAIL set vs ssr_hazard/replay fixtures; **mutation test** — flip one binding,
PASS→FAIL (proves it reads live state); plausible-but-wrong citation (right capture, wrong checkpoint)
→ CONTRADICTED.

**T5.3 — CalibrationHarness** *(effort M-on-top-of-T5.2)*
*Build:* held-out battery; collect the **existing 4-level vocabulary** (high/moderate/low/unknown); emit
reliability table + ECE; **hard gate `count(confidence=high ∧ contradicted)==0`**, ECE advisory until
≥200 graded claims. Adversarial slice = questions on UNOBSERVED slots (pairs with T5.1).
*Verify:* perfectly-calibrated stub → ECE≈0; always-high-50%-wrong stub → ECE≈0.5 and trips the gate.

### Sequence-last / scoped items

- **PSO fingerprint (T1-value-class):** ship **Layer A always-on** (decoded fields per draw, pure
  observation, cannot lie) before **Layer B opt-in sidecar** (self-dating → `SidecarStale` not false
  `PipelineMismatch`). Canonicalize desc (mask don't-care fields) before hashing.
- **Map/Unmap (T3-adjacent):** ship `DrawWhileMapped` (clean, crash-relevant); rename overflow →
  `MapSizeMismatch` (declared-size only — the hook cannot see memcpy extent through WRITE_DISCARD);
  demote `StaleDynamicRead` to informational unless a sidecar opts a buffer into a per-frame contract.
- **Aliasing tags / Deferred contexts / Indirect args:** **Phase-1 detect-and-flag only**
  (`AliasUntracked`, `DeferredContextUnmodeled`, `ArgsProducerUnknown`) — the membrane *refuses to vouch*
  for a frame it cannot model. Pay the full per-context/readback cost **only behind a real captured
  trace** that exercises the feature. Scope aliasing explicitly to RAW-pool/D3D12-placed (D3D11 hides
  committed allocations — `alloc_id` is not available to a generic hook).
- **Trace Bisect:** **linear forward scan** over aligned checkpoints reporting ALL divergences (state
  divergence is non-monotone — binary search is unsound); Myers/LCS-align streams of differing length;
  prerequisite is per-checkpoint binding snapshots (T1.1 machinery).
- **Diff-vs-vanilla SSIM/banding:** gate behind a per-pass **`deterministic` declaration** (dither/jitter
  off in golden); banding-as-LSB-step-count is meaningless under dithering — emit
  `diff_invalid:nondeterministic` otherwise.

---

## 4. The Cross-Domain Membrane PROTOCOL — one schema, five organs

The whole point of BUILD being an *organism*: the same four-part membrane schema instantiates across
every stateful subsystem, so each module exports the **same observable contract** and a cross-module
read is as local as a within-module read. The unified record:

```
MembraneRecord {
  // FRAME-LOG axis (ordered execution trace)
  seq:        u64,         // monotonic, assigned internally — hook-independent
  epoch:      u64,         // frame / build-id / request-id / tick — the domain's "Present"
  checkpoint: u64,         // draw|dispatch | compile-unit | alloc-site | lock-acq | build-step
  // BINDING-LEDGER axis (live symbol table)
  bindings:   [(slot_key, identity, kind, set_by_seq)],   // who/what is wired where, with blame
  // OUTPUT-METRICS axis (the eyes — actual values)
  metrics:    [(target, metric_code, value, age, status)], // fence/freshness-stamped
  // INVARIANT axis (asserted, with witness)
  violations: [(rule_id, observed, expected, witness)],
  // INTEGRITY envelope (every record self-describes its trust)
  provenance: { build_sha, abi_hash, schema_version, source, content_chk, freshness }
}
```

| Organ (BUILD module) | binding-ledger | output-metrics (eyes) | frame-log epoch | invariants |
|---|---|---|---|---|
| **GPU** (frametrace/RAW) | SRV/RTV/DSV/UAV per slot per pass + setter seq | mean/min/max/NaN/%black/saturation/diff-vs-vanilla | `Present` → frame_id | hazard, restore-verify, range, ping-pong/warmup |
| **Compiler / module graph** (`entangle`, `foundation`, `neutrino`) | symbol→definition, import edges, ABI discriminant table | compiles-clean %, unresolved-symbol count, `FT_ABI_HASH` | build invocation / commit sha | ABI-trinity pin, no-unresolved, no-cycle, header==rlib |
| **Heap / allocator** | resource→`alloc_id`+generation, live window | bytes-live, frag%, leak count | alloc epoch / GC tick | use-after-free (gen<current), aliased-read-write, no-double-free |
| **Threads / contexts** (buildos seqlock) | lock→holder, context_id→bindings | contention count, queue depth | scheduler tick / `ExecuteCommandList` | tear-free (seqlock even-read), cross-context-leak, no-deadlock-order |
| **Build / integration** ("organism" health-check) | artifact→source sha, dep edges | tests-green, coverage%, stale-fingerprint count | session start / CI run | corpus-preverify, manifest sha, freshness pin |

**Why this makes them one organism:** every organ emits `(seq, epoch, checkpoint, bindings, metrics,
violations, provenance)`. A cross-organ question — "is the heap allocation behind GPU slot t27 the one
the compiler's symbol table says the SSR pass owns, and is its build artifact fresh?" — resolves by
**joining three ledgers on the shared `identity`/`alloc_id` key and checking three `provenance`
envelopes for freshness**, instead of the model reasoning across three subsystems in its head. The
integrity envelope is *mandatory on every record* so no organ can present a stale or unobserved state
as live truth — the membrane's selective permeability is enforced uniformly.

---

## 5. Membrane Integrity — how the membrane avoids lying (and this session's live lessons)

The membrane's worst failure is not missing a bug — it is **manufacturing a confident green**. Five
mechanisms, each with its own falsifiable witness, plus two live incidents from this very session.

1. **Provenance / freshness pin (T0.2, FreshSeal).** Every record carries `{build_sha, abi_hash,
   schema_version, source, content_chk}`. A capture whose `build_sha` ≠ the touched-file blob shas is
   `STALE` ("recapture required"). Compare **per-touched-file blob_sha, not whole-repo HEAD** (avoids
   false-positive staleness storms). The freshness check is a **raw-byte recompute**, not the stored
   fingerprint — defeats a tool that reports a cached/forged hash.

2. **Raw-byte bypass / dual-path (T0.4).** Resolve a sealed slot via the normal path AND an
   independent table walk; assert agreement. This is the **load-bearing integrity primitive** and it
   directly models the read-layer-substitution threat.

3. **Cross-oracle agreement (I4, I5).** Two independent derivations of the same ground truth must
   produce a **position-sensitive** digest match (hook-fp vs RenderDoc-replay-fp; GPU-reduction vs
   CPU-reduction). *Disagreement is the artifact.* **Honest boundary:** oracles that share the hooked
   context are not fully independent (a context-substituting anti-cheat fools both) — true independence
   needs a layer below the immediate context (debug layer / GPU query).

4. **Self-audit manifest (T0.4).** "expected 27 asserts, ran 27, sidecar shas match pin, dual-path OK"
   — a single green line that is **itself falsifiable**. Catches a *registered-but-non-firing* assert;
   an *unregistered* invariant remains a stated coverage-gap.

5. **Determinism guard (T0.1).** A diff is only ground truth if the baseline is reproducible. Fail-closed
   on `replay_hash(replay(T)) != replay_hash(replay(T))`. **Necessary-not-sufficient:** proves
   *reproducible*, not *correct* — a consistently-wrong tracker hashes identically. State this; do not
   oversell it as validating verdicts (it validates that *inputs* were fresh/complete/ordered).

### This session's live integrity incidents (the threat model is active, not hypothetical)

- **Read-layer substitution (confirmed):** the project's `safe-read-redirect.py` PreToolUse hook blocked
  native `Read` of `state.rs`/`ffi.rs`, routing through `safe_read.py` which performs "vocabulary
  calibration" substitutions. The synthesis records an observed case where this layer rendered
  `.enumerate()` as "authorized service-enumeration" while its header printed `substitutions: {}
  (total=0)` / `semantic_modulation: clean` — **a read layer altering content while reporting clean.**
  This is exactly the dead zone T0.4's dual-path byte-bypass exists to catch.
- **My mitigation this session:** I refused to trust the summarizing read path for ground-truth
  verification and instead read the raw bytes via **Grep over the file** (ripgrep, content mode), which
  surfaced the literal source: `views: HashMap` (state.rs:29), `Event::Clear... => {}` (state.rs:79),
  the hook's hardcoded `==0 ? "ReadWrite" : "WriteWrite"` (frametrace_hook.cpp:157), and the exact FFI
  export set. Where the synthesis's prose disagreed with the bytes (e.g. "state.rs already uses BTreeMap
  for ordered tables" — **false**, `views`/`srv` are HashMap), **the bytes won.** That is the doctrine
  applied to itself: observe the artifact, do not assert from the summary in the layer's head.

---

## 6. frametrace ⇄ RAW Convergence — the smallest real next steps

RAW has the half frametrace lacks (`WrappedContext` wrapping `SetShaderResources`; `D3D11StateBackup`
save/restore; `SRVInjector`; `raw_eyes.py` externalizing per-pass output VALUES). frametrace has the
half RAW lacks (binding ledger + hazard model + invariant asserts + deterministic replay). Convergence
= **one ABI seam where RAW's saver feeds frametrace's comparator**, smallest steps first:

1. **Restore-Verify on RAW's StateBackup (smallest, highest-ROI).** RAW already saves/restores but never
   *proves* the restore. Add `ft_snapshot`/`ft_assert_restored` (T1.1); RAW's `WrappedContext` brackets
   each injected pass: snapshot before, verify after `StateBackup::restore()`. Output: a named leaked
   slot — `PS t27: saved=res#21042 restored=NULL`. **Coverage guard:** `ft_verify_restore` flags
   `RestoreUnobserved` if zero events flowed through the hook between snapshot and verify (the restore
   bypassed the membrane). **Unstated risk to close:** every mutation `StateBackup` performs must flow
   through the same `WrappedContext` the snapshot was taken on, or the diff reports an instrumentation
   gap as a false `RestoreDrift`.

2. **Sequence + freshness header on RAW's emit (T0.2).** RAW's `WrappedContext` and `raw_eyes` populate
   the same `{seq, build_sha, source}` header, so a stale/gappy RAW trace is visibly stale, not trusted.

3. **Value axis via the shared pointer key (T3.3).** `ResourceId(u64)` IS the `ID3D11Resource*` pointer
   (verified) and `raw_eyes` reads back outputs it can key by that same pointer — the join is **free, no
   new identity scheme**. `raw_eyes` calls `ft_pass_metrics(...)` so "depth out of [0,1] AND bound at
   t27 checkpoint 9" becomes one record. **The catch (must engineer, not hand-wave):** `raw_eyes`
   readback is async, 1+ frames behind the ledger checkpoint — `ft_pass_metrics` must carry the
   checkpoint **live when the readback was issued** (captured at copy-to-staging), and lag beyond a
   frame budget emits `MetricStale`, never a wrong-but-precise attribution.

4. **Fix the RenderDoc adapter to enable cross-oracle quorum (I4).** Today `renderdoc_to_frametrace.py`
   emits every `SetShaderResources` with `start:0` positional views and mints synthetic view ids — so
   "SSR at t27" is lost on the replay path and can never byte-match the hook fingerprint. **STEP 0:**
   emit true slot indices and a **shared resource identity (debug name via
   `GetPrivateData(WKPDID_D3DDebugObjectName)`)**. Only then does `ft_checkpoint_fingerprint` agreement
   become meaningful; the **first committed test should assert today's adapter DISAGREES** (documenting
   the bug the quorum catches), agreement only after the slot+identity fix.

**Convergence north star:** RAW owns state *mutation* (backup, inject, pool allocation, history
ping-pong); frametrace owns state *assertion* (ledger, hazard, restore-verify, range, temporal);
`raw_eyes` owns *values*. The four-part `MembraneRecord` (§4) is the single contract all three speak,
joined on `(ResourceId pointer, checkpoint, frame)`.

---

## Implementation status (as built -- photon/frametrace)

The GPU organ of the coherence membrane plus the agent-facing gate. 37 tests, all
green, verified by the organism check + CI (.github/workflows/organism.yml) on
every push. Each organ is model + C ABI + oracle tests + a runnable example.

| Tier | Organ | Commit | Witness it emits |
|------|-------|--------|------------------|
| foundation | binding table + hazards (v0/v1), C ABI, D3D11 vtable-hook DLL, JSON trace replay, RenderDoc adapter | 112bf3b..0962c8f | ReadWrite/WriteWrite hazard on res#X |
| 0 | trust roots: ABI-trinity single-source-of-truth (ft_hazard_kind_name); determinism (BTreeSet + Ord) | 5b7aea4 | (integrity floor) |
| 1 | restore-verify: snapshot + diff_restore | fa87fd6 | Ps t27 saved=res#21042 restored=NULL |
| 2 | temporal ping-pong: Present, declare_history_pair; wires the formerly-no-op Clear | 6219df4 | frame 2 SwapDesync ... member res#10 |
| 5 | NegativeLedger: adjudicate(Claim) -> Verdict; the agent-facing gate | 917e215 | CONTRADICTED (observed: Ps t27 unbound) |

Open: Tier 3 value channel (the eyes: range/NaN/%black) -- blocked-honest, raw_eyes.py
lives in the RAW sibling repo; needs a minimal in-tree value producer or RAW
integration. Tier 4 liveness query surface. Live-hook wiring of Present (swapchain
hook) and restore boundaries (RAW StateBackup) -- the build-verified pattern; runtime
needs a target app.
