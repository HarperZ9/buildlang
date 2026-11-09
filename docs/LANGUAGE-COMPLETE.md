# What finished means for BuildLang

Written 2026-11-08. This file defines when BuildLang is finished and records how far it is. It
replaces no milestone: it orders the ones in `docs/STRATEGY-2026-10.md` (M3 to M6),
`docs/STRATEGY-2026-10-SURPASS.md` (parity P1 to P8, verification V1 to V8, specs S1 to S4) and the
author's later direction into stages, and gives every row a check that can fail.

## The goal

The author's words set the target, and finished means all of them:

- 2026-10-09: "push it far into the next level to exist as *the* agentic programming language."
- 2026-10-23: "a direct competitor to Bend as a next generation AI and agent first creative
  language", and "We want BuildLang to beat and surpass both of these" (Anthropic and Theorem's
  binary verification, and Bend 2 with its roadmap), and "Feature parity in where the competitors
  excel as well."
- 2026-10-31: "a programming language that crosses all existing physics and information and
  mathematics and literary and design and rendering and hardware and software and digital and
  analog boundaries", with output "through C, through memory languages ... to the lowest of the
  low and the highest of the high levels", and "interoperability and full fluid compatibility
  between these layers".
- 2026-11-08 (relayed): "for the long shot and work on the aim for the longer term goal and work
  our way to it."

So the definition is staged. Stage 1 is the language core a game can be built on and an agent can
write; the last stage is the multi-level language of the 2026-10-31 vision. BuildLang is finished
when every row of every stage says PASS.

## How a row is scored

- **PASS**: the row's check ran and met its bar. For anything shipped in the `buildlang` crate the
  check ran against a release installed from crates.io (`cargo install buildlang --version X
  --locked`, `BUILDLANG_STDLIB` unset); the release is named. For work outside the crate (BuildTeX
  in `typeset/`) the check is the CI job on `main`, named as such.
- **FAIL**: the check ran and missed its bar. The evidence says by how much.
- **NOT BUILT**: the feature or its check does not exist yet. Parts that exist are listed in the
  evidence column; a part is not a pass.

A row never moves to PASS on a claim, a design document or a branch. A competitor row (parity,
surpass) also needs a dated comparison on a shared benchmark before any parity or lead claim.

## Stage 1: the game-rebuild core (met on 1.11.0)

M4 from `docs/STRATEGY-2026-10.md`: the gaps the Low Water rebuild needs. Its goldens are the
pilot's own.

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| M4a DETMATH | every row of the Low Water `design/DETMATH.md` golden table reproduced bit for bit by BuildLang `dsin dcos datan datan2 dasin dacos dexp dhypot` (any differing bit) | PASS (1.11.0) | 12 of 12 goldens on 1.11.0 (first on 1.9.0, #96). The 12 goldens cannot see a one-ulp error in the sine's S1, so a sweep was added on 2026-11-08: raw-native `tests/data/action/detmath-sweep.json` (8 functions x 1,001 inputs, recorded from the JS) gives 8 of 8 equal hashes with 1.10.0 and 1.11.0; a planted one-ulp S1 changes the dsin and dcos hashes |
| M4b Move data | a BuildLang loader and expander for `combat/data/moves.json` equals `combat/tests/moves-expanded.golden.json` byte for byte (any differing byte) | PASS (1.11.0) | 168,234 bytes, identical, pilot built with 1.11.0 (and earlier with 1.9.0 and 1.10.0) |
| M4c Traces | a BuildLang combat step writes `lw-trace/1` records whose `trace-manifest` equals the JS corpus manifest (any differing hash) | PASS (1.11.0) | see "M4c in detail" below |
| M4d C ABI | the C++ host links a BuildLang static library through a plain C header and passes the same tests as the C++ module it replaces (any test differs) | PASS (1.11.0) | raw-native ab0e692 `tests/test_lowwater_c.cpp` linked against `examples/lowwater-rules` built by crates.io 1.11.0 through `rules_c.h`: OK, 16 of 16 checks, as against the C++ module (OK, 16 of 16). A planted label threshold (Easy at 9) fails 1 check. Limit: the story expression language is not ported; records that need it are refused at create |

### M4c in detail

The 40-fight corpus is pinned to Low Water `origin/dev` 8b49bdd, regenerated on 2026-11-07 with `LIGHT_HS` unset (`node combat/tools/trace.mjs <dir> 40
900`, then `tools/trace-manifest.mjs`): 40 traces, 2,944,071 bytes, manifest sha256
`657983012666366802a742e05bee58d0593e49c162fbb473102ccbe3aaae9f19`; a second JS run gave the same
manifest, and the bytes equal the untracked 2026-10-22 copy and the 2026-10-23 pilot copy. DETMATH section 4 records 2,845,097 bytes for a 2026-10-23 corpus; no run here reproduces that number, and its source is unknown. The BuildLang port, built by `buildc` 1.10.0 and again by 1.11.0 from crates.io, wrote 40 traces whose manifest
has the same sha256: cbegin 40, cin 20,290, cev 5,350, cstate 571 and cend 40 lines, all identical.
Two planted faults were caught: a heavy-hit force of 3.0 instead of 2.0 changed the hash of every
trace written before the run stopped, and one flipped bit in one trace changed the manifest.

Limits, stated beside the result: the port reads three things from a JS export instead of computing
them (the field's terms at each refresh, the clock at each hash, the footing grip table), so the
field's evolution and the clock are not ported. The port and the game data live with the private
game, not in this repository.

## Stage 2: an agent writes it (met on 1.11.0)

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| M3 Held-out pass rate | a new held-out probe of at least 30 tasks (at least 6 game-shaped), sealed by hash and pushed before any generator sees it, graded with the `bench/heldout-2026-10-15` harness against the current crates.io release, two generators: pass@1 at least 25/30 each (scaled: 29/34) and zero check-pass-but-fail (either generator below the bar, or any program that passes `buildc check` and then fails to build, crashes or prints the wrong output) | PASS (1.11.0) | bench5 (`bench/heldout-2026-11-08`), 34 tasks with 8 game-shaped, sealed in 3bd69e9 before generation: sample A 34 of 34, sample B 34 of 34, check-pass-but-fail 0 in both. Limits: the two samples are two draws of one model, and the task writer also wrote the 1.11.0 compiler changes (`RESULTS.md`) |

## Stage 3: parallel by default (shipped in 1.11.0; scored on its crates.io build)

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| M5 Deterministic parallelism | n-body step, CPU per-pixel shading and the 40-fight corpus run fight by fight, at least 10 timed runs each with median and 95% interval: at least 4x on 8 cores for n-body and pixels, byte-identical output to the serial run everywhere (any output difference, or a lower interval bound below 4x) | PENDING 1.11.0 | `par_for` and `par_map` merged in #136 and ship in 1.11.0. The run of record used the branch compiler (`bench/parallel-2026-11`): kernel speed-ups n-body 5.34x [5.07, 5.63], shading 5.87x [5.57, 6.05], 40-fight corpus 5.09x [4.94, 5.37], byte-identical in 30 of 30 runs each. The bar holds on that run; it counts once rerun against 1.11.0 from crates.io |

## Stage 4: a production GPU path (target 1.13.0)

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| M6 Device-run kernels | `#[compute]` kernels emitted as WGSL and HLSL, dispatched on Windows (D3D12 or WebGPU) and Linux (Vulkan); a device run of every golden kernel in CI on a GPU machine, integers exact and floats within a declared ULP bound, with a receipt naming device, driver and kernel hash (a kernel with no device run, or a result outside its bound) | NOT BUILT | HLSL and GLSL emit exist; WGSL emit does not; the 12 default-feature GPU tests do not run on a device |

## Stage 5: the language is frozen (target 2.0.0)

2.0.0 ships when stages 1 to 4 pass and the specification is frozen. From then on a program that
checks under 2.0.0 checks under every 2.x release.

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| F1 Specification current | every construct the 2.0.0 checker accepts has a section in `SPECIFICATION.md`, and every section has at least one checked block (`build` with no `parse-only` or `lex-only`) that passes in `compiler/tests/doc_blocks.rs` (any construct or section without one) | FAIL | measured 2026-11-08: 16 of 40 subsections have no checked passing block: 1.1, 1.3, 1.4 (lex-only), 1.5, 1.6, 2.1, 2.2, 2.6, 3.2, 4.5 (parse-only), 5.4, 7.1, 7.2, 7.3, 8.1, 8.2. The construct inventory against the checker does not exist yet |
| F2 Stability promise | a frozen-language test set (every checked block of 2.0.0) runs in CI against each later release (any 2.x release refuses a block 2.0.0 accepted) | NOT BUILT | the doc blocks run against the current compiler only |
| F3 The checker is the truth | the M1 mutation corpus, the `tests/reject` set and `check_implies_compile` pass on the release, and the latest held-out probe has zero check-pass-but-fail | NOT BUILT as a release gate | the three test sets pass on `main`; the probe count comes from M3 |

## Stage 6: verification inside the language (target 2.1.0 to 2.3.0)

The surpass targets against Anthropic and Theorem's binary verification. The spec, not the proof, is
the wedge (`docs/STRATEGY-2026-10-SURPASS.md`).

| Row | Check (fails if) | Release | Status | Evidence |
|---|---|---|---|---|
| V1 Safety by construction, with certificates | a corpus seeded with each named crash class (out of bounds, null, division by zero, stack and heap exhaustion under a bound, defined integer traps): every seed refused at check or caught by an inserted check, with a certificate naming the class and rule; a fuzzing run with zero unexplained crashes (any seed reaches undefined behaviour) | 2.1.0 | NOT BUILT | inserted bounds checks exist (exit 101); no certificates, no seeded corpus |
| V3 Reward-hacking guard | adversarial agent "fixes" that make a spec trivially true are all rejected without a reviewer signature (any accepted) | 2.1.0 | NOT BUILT | check receipts bind source hashes; no spec or proof binding |
| S1 Specs in the same language | spec-breaking edits fail `buildc check` with a BL code, spec-preserving edits pass (either error) | 2.1.0 | NOT BUILT | |
| S2 Spec quality tools | every mutant in each spec's mutation set fails the spec; `buildc spec explain` exists (a weak spec) | 2.1.0 | NOT BUILT | |
| S3 Property library | round-trip, idempotence, no data races, crash consistency, authorisation, deterministic replay, frame budget: each applied to a right and a seeded wrong implementation (a wrong one passes) | 2.1.0 | NOT BUILT | |
| V2 Contracts through SMT, exported to Rocq or Lean (P5 laws) | a contract suite including Bend 2's published `LAWS.bend` examples, ported, plus false contracts; every export loads in its prover (a false contract accepted, or an export that does not load) | 2.2.0 | NOT BUILT | parts in 1.12.0: `buildc verify` (laws and contracts, #138) with a bit-precise SAT solver inside `buildc` (#186): 24 false claims in `tests/verify/smt_false.bld`, 0 accepted, 23 refuted; Bend's numeric laws ported (`tests/verify/bend_laws.bld`). No Rocq or Lean export yet (author ruling 2026-11-02: no Lean) |
| S4 Abstract interpretation | interval, sign and nullness domains prove the bounds in an indexing corpus and refuse a seeded unsound product (an unsound product accepted, or a false "safe") | 2.2.0 | NOT BUILT | |
| V4 Translation validation | per-function certificates that the C output matches the source for the semantic corpus and the pilot, and a mutation run where every injected miscompilation is caught (any missed) | 2.3.0 | NOT BUILT | parts in 1.12.0: `buildc tv` certifies 159 of 435 functions in `tests/programs` and 58 of 139 in `stdlib` (integers, bool, floats, constants); planted `add-to-sub` and `fadd-to-fsub` miscompilations caught in every affected function (`docs/TRANSLATION-VALIDATION.md`). Not yet: structs, strings, effects, the pilot |
| V5 E-graph substitution with certificates | rewrites equal to the unrewritten program on a frozen input set, every certificate re-checked independently, a forged one refused (any difference or accepted forgery) | 2.3.0 | NOT BUILT | |

## Stage 7: parity where competitors excel (target 2.4.0 to 2.8.0)

Every row of the parity table in `SURPASS-TARGETS.md` and `docs/STRATEGY-2026-10-SURPASS.md`.

| Row | Competitor's strength | Check (fails if) | Release | Status | Evidence |
|---|---|---|---|---|---|
| P1 | Zig: C import, cross-compilation | the raw-native header the pilot links imports with no hand-written `extern` block, and binaries built on one host for Windows, Linux and macOS run their tests in CI on all three (a missing target or differing test) | 2.4.0 | NOT BUILT | `extern "C"` blocks are hand-written; `extern "C" source` compiles a C file into the program |
| P2 | Slang: shader modules with generics | the raw-native style passes as BuildLang shader modules match their image goldens within bound; every target passes DXC, glslang, Naga, spirv-val (a pass out of bound or a target that fails validation) | 2.4.0 | NOT BUILT | HLSL and GLSL emit; no modules, generics or WGSL |
| P3 | Julia: numerics and dispatch | DETMATH goldens bit for bit, and dot, GEMM, FFT and stencil medians over 10 runs beside Julia's on the same machine and date; parity only within 1.25x (any golden bit; parity claimed outside 1.25x) | 2.4.0 | NOT BUILT | DETMATH library shipped (M4a); multiple dispatch exists (`docs/MULTIPLE-DISPATCH.md`); no Julia comparison run |
| P4 | Bend 2: GPU and automatic parallelism | the same program on CPU and GPU, integers identical and floats within ULP bound; Bend 2's published examples ported and timed against Bend 2 on one machine (any output difference) | 2.5.0 | NOT BUILT | needs M5 and M6 |
| P6 | Mojo: Python interop | import a pure Python library and NumPy, and round-trip ints, floats, strings, lists and arrays both ways (any round-trip difference) | 2.6.0 | NOT BUILT | |
| P7 | Rust: packages and a registry | publish, resolve and build a three-package graph offline from the lockfile twice with identical output hashes (any hash difference or network access) | 2.6.0 | PENDING 1.11.0 | `buildc pkg` merged in #137 and ships in 1.11.0: two clean registry copies built offline from one lock give the same C artifact hash (`compiler/tests/pkg_flow.rs`), on the branch compiler. Still to run on the crates.io build: the three-package graph twice, with network access checked |
| P8 | Elixir: supervised actors | one-for-one and one-for-all crash-and-restart tests restart the right children within a bounded number of attempts with message order kept (any lost or reordered message) | 2.6.0 | NOT BUILT | |
| V6 | BendTorch: differentiable types | MNIST and a small transformer trained in BuildLang match a PyTorch twin within a stated loss tolerance per step, on CPU and the M6 GPU path (any step out of tolerance) | 2.7.0 | NOT BUILT | |
| V7 | Anthropic's skill: verification in the developer loop | the held-out probe includes verification tasks scored like the rest, at the M3 bar (below the bar) | 2.7.0 | NOT BUILT | `buildc mcp`, the language server and `skills/buildlang` exist; no verification tasks |
| V8 | Anthropic and Theorem: verification at scale | a shared benchmark comparable to their OSS-Fuzz set (tokens per verified function, coverage, wall time) reproducible by a third party from its receipt (no reproduction) | 2.8.0 | NOT BUILT | no superiority claim before this runs |

## Stage 8: one language from proof to plasma (target 3.0.0)

The 2026-10-31 vision. One program lowers through levels, every lowering is checked, and every
value carries what it means.

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| L1 The field core type | each value carries a unit, an uncertainty, a provenance and a representation (analog or digital, with sample rate or grain); the same operators (FFT, convolution, compression, render) apply to text, image, mesh and volume fields; a converter declares the bits it loses, and a lossless one round-trips bit for bit (a converter that loses bits it did not declare, or an operator that refuses a field kind) | NOT BUILT | units checked at compile time (`docs/DIMENSIONAL-ANALYSIS.md`, slices one and two); measured constants with source and uncertainty labels proposed in #131; `docs/FIELD.md` is the spec, with its reference implementation outside BuildLang |
| L2 Multi-level lowering | one program lowers through proof, spec and paper, algorithm, schedule, C and LLVM, SPIR-V and GPU, RTL (Verilog or Chisel) and an analog netlist (SPICE); each level is a named dialect with a printer and a checker (a level that cannot be printed or checked) | NOT BUILT | C is the product backend; LLVM IR, SPIR-V, WASM, Rust, x86-64 and ARM64 emit are experimental; no RTL or netlist |
| L3 Translation validation at every lowering | every lowering in L2 carries a certificate a separate checker re-derives, and a mutation run catches every injected fault at every level (any missed) | NOT BUILT | builds on V4 |
| L4 Differential backends | the C and GPU outputs of one program are bit-identical, or differ only within a bound the program declares (any difference outside the bound) | NOT BUILT | builds on M6 |
| L5 BuildTeX in place of LaTeX | the author's CV and a paper build from BuildLang sources with no TeX installed, and the PDFs pass the publish-metadata check (a TeX dependency, or a metadata finding) | PASS on `main` CI | `typeset/` with the `typeset` CI job; PDFs carry no author, creator or clock date (CHANGELOG, BuildTeX publish metadata) |
| L6 Proofs in place of Lean | the paper's proofs are checked by a kernel written in BuildLang, and the kernel refuses each planted fault (an accepted fault) | NOT BUILT as stated | mmk checks Metamath databases and refuses four planted faults and a PTX litmus fault; no paper proof is checked by it yet |
| L7 One instrument end to end | one instrument in a single language: law, circuit, firmware, measurement, receipt, rendered spec and blueprint, and the paper, with every stage checked (any stage outside the language or unchecked) | NOT BUILT | |

## Release plan

| Release | Ships when | Rows |
|---|---|---|
| 1.11.0 | released 2026-11-08 | M4 and M3 met; M5 and P7 shipped, to be scored on this build |
| 1.12.0 | M5 and P7 pass on a crates.io build | M5, P7 |
| 1.13.0 | M6 passes | M6 |
| 2.0.0 | stages 1 to 4 pass and F1 to F3 pass | the frozen language |
| 2.1.0 to 2.3.0 | each verification row passes | V1, V3, S1 to S3; V2, S4; V4, V5 |
| 2.4.0 to 2.8.0 | each parity row passes, with its dated comparison | P1 to P4, P6 to P8, V6 to V8 |
| 3.0.0 | stage 8 passes | L1 to L7 |

The order follows the strategy: the game rebuild first, then the agent bar, then parallelism and the
GPU, then the frozen language, then verification, parity and the multi-level language. A later stage
may start before an earlier one passes; it cannot be scored PASS on a release that fails an earlier
stage's row it depends on.

## Verified

- M4a to M4d: rerun on 2026-11-08 against `buildc` 1.11.0 installed from crates.io with
  `BUILDLANG_STDLIB` unset; every planted fault was caught.
- M3: bench5 graded on 2026-11-09 against the same install; grader controls 6 of 6 as predicted.
- F1: the subsection count was taken by a script over `SPECIFICATION.md` on 2026-11-08.
- Every NOT BUILT row was checked by a search of `compiler/src` and `docs/` on 2026-11-08 for the
  feature it names; "exists" notes in the evidence column come from that search.
