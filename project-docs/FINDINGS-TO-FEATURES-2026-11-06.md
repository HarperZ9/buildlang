# Findings to features, 2026-11-06

This file maps findings onto BuildLang features. The findings come from the
saved-materials reading of 2026-11-02 to 2026-11-05 and from our own work. Each
row names its source, the feature, a priority, the test that would show the
feature works, and its state on this date.

**Sources.** "Row N" is a row of the saves map: `2026-11-01-new-saves/MAP.md`
in the mission-control workspace, with per-batch copies in
`2026-11-02-saves-reading/map-rows-batch*.md`. "Hnn" and "Qnn" are queue items
in `2026-10-31-wavefunction-lens/QUEUE.md`. Facts the map marks as unverified
stay marked here. "Own" rows come from BuildLang's own probes and from the
projects that use it.

**Priority.**
- P1 closes a hole where a program checks and then fails, or checks and
  computes the wrong thing.
- P2 makes an existing receipt or check stronger in a way a finding asks for.
- P3 is new ground.

## What already exists

Several candidates in the brief for this work were already in the language,
so they are not new features here:

- **DETMATH.** Every generated C file forbids contraction into fused
  multiply-add. `exp`, `log`, `pow`, `sin` and `cos` call portable fdlibm
  versions, with golden-bit tests in `compiler/tests/detmath.rs`.
- **Run receipts with input and output hashes.** `buildc run --emit-receipt`
  seals these digests: the source, the module graph, the toolchain, the
  executable and the raw stdout. It also seals a `determinism` block, derived
  from the capabilities the program uses (`docs/SCIENTIFIC-RECEIPT.md`).
  `receipt verify` re-runs the program and compares; `--self-test` shows that
  the verifier can fail.
- **Seeded randomness.** An unseeded `random_f64()` aborts, and the seed is
  sealed in the receipt.
- **Cross-checks between backends.** Receipts can compare the C backend with
  the Rust lane. A GPU readback is checked against the C loop within a
  tolerance.
- **Unit annotations.** `f64<m/s>` has been in the checker since slice one
  (2026-07-29), and the canonical unit is sealed in the receipt.
- **Diagnostics.** Every error has a stable BL code, and some carry
  machine-applicable `edits`.

## Map

| # | Source | Finding | Feature | Priority | Test that shows it works | State |
|---|---|---|---|---|---|---|
| 1 | Own: #124, #127 | #124 and #127 fixed four holes where a program checked and then failed in C: maths builtins typed i32 in the lowering, writes through a read-only slice, `&[T; N]` emitted as `double[4]*`, and reassigning an immutable `let` | Hunt the same class: a probe per builtin family, where every accepted form must build | P1 | A sweep test that checks, builds and runs every accepted form and compares values | f64 methods done in #128, which found row 2. Free maths builtins done in #130 |
| 2 | Own: probe of 2026-11-06 | `x.log(base)` checked, then the C compiler refused `build_det_log(x, base)` | Lower to `build_det_log(x) / build_det_log(base)` | P1 | `compiler/tests/float_methods_compile.rs`: 35 f64 method calls build and match Rust within 1e-12 | Shipped in #128 |
| 3 | Own: probe of 2026-11-06 | Free maths builtins (`sin`, `sqrt`, `pow`, `clamp` and the rest) had no signature, so `sin(true)` and `pow(2.0)` checked and then failed in C | Check the argument count and argument kinds of each builtin | P1 | `tests/reject/math_builtin_bool_arg.bld` and `math_builtin_wrong_arity.bld` | Shipped in #130 |
| 4 | Q29; row 846 (Zed: units in types); our physics lanes | Physics lanes mix quantities. Slice one refused every root and power on a unit, and two methods gave the wrong unit: `t.recip()` on seconds stayed seconds, and `d.sin()` on metres checked | Roots and powers lift the unit when the exponent is an integer literal. `recip` inverts the unit. Trigonometric, exponential and logarithmic operations need a dimensionless value. Binary methods check the units of their arguments | P1 | `compiler/tests/units_powers.rs`: a pendulum program checks, and its C is byte-identical to its unannotated twin. One wrong annotation fails with BL0601. Reject programs cover `sin` of a length and a power that inverts | Shipped in #130 |
| 5 | Rows 1299, 1266; Q29 | Constants travel without a source or an uncertainty: a Strouhal number and a Kármán spacing in a caption, an energy budget with no source | `#[measured(source = "...", uncertainty = ...)] const`. The checker refuses one without a source (BL0605). The check receipt lists each one, and `receipt verify` re-derives the list | P2 | `compiler/tests/measured_constants.rs`: the receipt lists G with CODATA 2018 and its uncertainty. An edited source fails verification. The emitted C is unchanged | #131 |
| 6 | Q29; our colour and audio work | Dimensions cannot see scale. `ms` against `s`, `kHz` against `Hz`, and `Hz` against `rad/s` (both s^-1) all pass | Scaled units (`f64<ms>`, `f64<kHz>`) with explicit conversion, and an angle kind that keeps `rad/s` apart from `Hz` | P1 | Adding `f64<ms>` to `f64<s>` is refused. A conversion carries its factor, which the receipt seals. `2 pi f` is the only way from `Hz` to `rad/s` | Not started |
| 7 | Weak mode, `docs/DIMENSIONAL-ANALYSIS.md` | A unit bug can hide in an unannotated intermediate binding | Dimension variables: an unannotated literal infers its dimension | P2 | `let p = 1.0; d + p` with `d: f64<m>` infers `p: f64<m>`; `let p: f64 = t; d + p` is refused | Not started |
| 8 | Row 473 (arXiv 2610.31692); H74, H148 | Changing only the batch layout changed nearly every chaotic float64 run. With fixed shapes, 448 of 448 runs were bitwise identical. The source files disagree on the count: NOTES.md says 173 of 192, the map 190 of 192. Check the paper before quoting either | A layout hash in the run receipt, and a `#[compute]` reduction order fixed by the source, so a GPU run is either bit-identical or names why it is not | P2 | Two runs with the same layout seal the same stdout digest. A planted layout change flips both the hash and the verdict | Not started |
| 9 | Rows 562, 919, 1167, 1273; COMBINATIONS-batch22 item 2 | Rounding to bf16 discarded most RL updates, and bf16 cannot tell 999, 1000 and 1001 apart. Row 562 puts it as "the checkpoint is not the model" | A narrowing cast names its rounding mode (`as f32 round(nearest)`, `stochastic(seed)`). The receipt binds the artefact that ran, not the master copy | P2 | A narrowing without a mode warns. A seeded stochastic cast is bit-reproducible. The receipt digest is over the narrowed bytes | Not started |
| 10 | Row 1021 (Silmaril); CROSS-EXAMINATION-batch25, section 1 | A proof must state its main boundary (IO, runtime, host), and budget exhaustion counts as "inconclusive" | Check receipts gain the `scope` list that scientific receipts already carry | P2 | Every check receipt carries `scope`, and a policy can require a named entry | Not started |
| 11 | Rows 934, 1148, 1059 (PTX in Lean; LARK) | Kernel proofs model floats as reals and assume there is no undefined behaviour. Race freedom and termination are separate obligations | `#[compute]` kernels get a race-freedom check on shared writes and a termination bound, reported apart from numerical claims | P3 | A kernel where two invocations write one index is refused. The receipt lists race freedom as checked and rounding as not checked | Not started |
| 12 | Row 934 (non-vacuity witness); row 1020 (obend: "words = 20 would pass") | A for-all gate can pass vacuously. A refusal helps most when it names the value that would pass | Diagnostics name the nearest passing value where one exists: a unit mismatch names the conversion, an arity error names the call shape | P3 | JSON diagnostics for BL0601 carry an `edits` entry that makes the program check | Partly done: edits exist for effects and some codes |
| 13 | Rows 466, 1056, 962 (qwenGPU and QwenOS replay each launch on an interpreter) | A reference interpreter catches drift between backends | A MIR interpreter as a third lane for `--cross-backend` | P3 | One kernel's stdout digest agrees across C, Rust and the MIR interpreter | Not started |
| 14 | Row 973 (Verus); rows 1300, 1146, 1147 | Trusted escapes such as `assume` and `external_body` must be visible. Drift between a spec and its code fails no build | The check receipt lists every `unsafe`, `extern "C"` and `#[trusted]` site, plus a hash of each doc-comment contract | P3 | The receipt lists every trusted site. Weakening a contract changes its hash, and a pinned policy fails | Partly done: capability provenance lists ambient sources |
| 15 | Own: a probe of 2026-11-06 | The canonical unit string `m^3/kg*s^2` reads as m^3 s^2 / kg, though it means m^3 / (kg s^2), and the unit grammar rejects parentheses | Group the denominator in the canonical form (`m^3/(kg*s^2)`), and accept parentheses in the unit grammar | P2 | `canonicalize_unit("m^3*kg^-1*s^-2")` returns an unambiguous string, and that string parses back to the same dimension | Not started |
| 16 | Own: CI on #128 | `run_keeps_a_crashed_programs_exit_code` expects 0xC00000FD on Windows. One CI run got 0xC0000005 (an access violation) from the same deep recursion | Accept either code, or make the overflow deterministic with a guard page | P2 | The test passes across 20 Windows CI runs | Not started; a rerun passed |
| 17 | Own: the DAW session | A DSP inner loop could not vectorise because of per-index bounds checks | `get_unchecked`, `set_unchecked` and vectorised release builds | P1 | `compiler/tests/unchecked_slices.rs` | Shipped in #129 by the DAW session |
| 18 | Own: the DAW session, after #129 | Programs hoist bounds checks by hand. gcc removes a check only when the loop is bounded by the length of the same slice it indexes | A compiler pass that proves the index range once before the loop, for loops bounded by anything else (a parameter, another slice's length), so safe code vectorises without `get_unchecked` | P1 | The DAW's `daw_bank` loops written with plain indexing emit no per-iteration check in the C and match the hand-hoisted timing within noise; an out-of-range bound still traps before the loop | Not started |
| 19 | Own: the DAW session, CHANGELOG | An `extern "C" fn` inside a module gets the module prefix in its C name (`dsp_daw_modal_coeffs`), so a host that links against the declared name fails | Keep the plain C name for `extern "C"` items in any module | P1 | A module-level `extern "C" fn` exports its declared name in `--emit staticlib`, and a C host links against it | Not started |

## Order of work after 2026-11-06

1. The module prefix on `extern "C"` names (row 19). A host cannot link
   against a module's exports today.
2. Automatic bounds-check hoisting (row 18), measured on `daw_bank`.
3. Scaled units and an angle kind (row 6). The physics, audio and colour
   lanes need `ms`, `kHz` and `rad/s` more than any other unit feature.
4. Dimension variables (row 7), which closes the weak-mode gap.
5. A builtin sweep for the remaining families: string, Vec and HashMap
   builtins, the way #128 swept the f64 methods (row 1).
6. An unambiguous canonical unit string (row 15).
7. A layout hash and a fixed reduction order for `#[compute]` (row 8).
8. A rounding mode on narrowing casts (row 9).
9. `scope` in check receipts (row 10), then the kernel obligations (row 11).

## Verified

- Rows 2 and 3: the probes ran on `main` at d529540 with `buildc check` and
  `buildc run`. Each hole reproduced before its fix. `float_methods_compile.rs`
  failed on `x.log(3.0)` without the fix.
- Row 4: the slice-one tests that asserted the old refusals were rewritten to
  assert the lifted dimensions. On the branch, the full suite passed 1,961
  tests with 0 failures.
- The "What already exists" list: checked against `buildc run --help`,
  `docs/SCIENTIFIC-RECEIPT.md`, `compiler/tests/detmath.rs` and
  `docs/DIMENSIONAL-ANALYSIS.md` on the same date.
- Saves facts: copied from the map with its own verification marks. This
  file does not check the underlying papers.
