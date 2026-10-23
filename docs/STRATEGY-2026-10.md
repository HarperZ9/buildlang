# BuildLang strategy, October 2026

Written 2026-10-22; section 7 added after the 1.8.0 release, which closed #83. Baseline at
writing: release 1.7.0 (crates.io and GitHub Latest), C backend as the
product path, HLSL and GLSL emit, other backends experimental, held-out agent probe on 1.6.0
at 20/27 for generator A and 17/27 for generator B.

## 1. The direction and what it reverses

The author, 2026-10-23 (verbatim, as relayed): "BuildLang can be a direct competitor to Bend as a
next generation AI and agent first creative language - it is built for AI in the loop programming,
and since AI is taking over software design entirely; we should focus on building BuildLang to fill
that niche and become a monolithic power language." Then: "Lets work on the game and engine first,
and BuildLang perhaps in parallel."

This reopens the lane that `docs/COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md` froze. The
2026-06-29 directive had already lifted the freeze for the C path, effects and self-hosting, and it
kept the freeze on native and GPU backend productionization. The 2026-10-23 direction lifts that
remaining freeze for the work this document names: a parallel-by-default story and a production GPU
path. Maturity labels stay honest. A backend is experimental until its milestone check below passes.

The game comes first. Low Water and raw-native are built in C++, rebuild-ready for BuildLang
(language-neutral trace, math and data specs; a plain C ABI game layer; WGSL shaders as the source of
truth). Nothing in this plan sits on the game's critical path.

## 2. The field, as of 2026-10-22

Sources are listed at the end. Vendor numbers are vendor claims; none was reproduced here. UNKNOWN
means no current primary source was found in this pass.

| Language | Current release (confidence) | What it does best | Where an agent-first creative language can beat it |
|---|---|---|---|
| Bend 2 (Higher Order Company) | Announced 2026-09-17; repo moved to bendlang/bend; no tagged release on GitHub (moderate) | Proof-gated agent code: humans write `LAWS.bend`, the agent writes proofs, the compiler refuses code that breaks a law. Targets C, Metal, CUDA, JS. Automatic divide-and-conquer parallelism | Bend states its own gaps: no Windows, no ABI promise, whole programs only (no linkable library), thin base library, verbose code, "parallelism requires balanced calls", compiler "99% AI-written and not yet fully audited". BuildLang can win on a C ABI library path, Windows, game and shader output, and bounded authority (capability effects plus receipts) where proofs are too costly |
| Bend 1 / HVM2 | Bend 1 (May 2024) programs do not load in Bend 2; HVM is gone from the Bend 2 runtime (moderate) | Interaction-net runtime with automatic parallelism on CPU and GPU | Superseded by its own authors |
| Mojo (Modular, acquired by Qualcomm) | 1.0 on 2026-08-11, compiler open sourced under Apache 2.0 the next week; a 1.1 date is unverified (high for 1.0) | Python syntax with systems performance; GPU kernels for AI workloads | No capability effects or receipts; an agent's authority is not in the type. Games and shaders are not its target |
| Rust | 1.96 confirmed (May 2026); the current October version is UNKNOWN | Memory safety without GC, the largest systems ecosystem, cargo | Agents hit borrow-checker friction; effects are not tracked; shader output needs a separate stack |
| Zig | 0.16.0, 2026-04-14 (high) | Explicit allocation, comptime, C interop, cross-compiling toolchain; 0.16 makes I/O an explicit parameter | Pre-1.0 churn (0.16 broke APIs widely); no effect system beyond the explicit I/O parameter |
| Odin | UNKNOWN | Data-oriented game code, simple syntax, vendor graphics bindings | UNKNOWN until checked; the likely gap is the same as Zig's: no checked authority |
| Jai | Closed beta, 0.2.026 on 2026-02-21 (moderate) | Compile-time metaprogramming for games | Not public, so agents cannot be trained or evaluated on it |
| Julia | Stable line 1.12.x to 1.13.x; exact current version conflicts across sources (low) | Multiple dispatch, scientific computing, `--trim` for smaller executables | JIT start-up and GC pauses suit games poorly |
| Elixir | 1.20.0 on 2026-06-03, whole-program gradual type inference with no annotations (high) | Concurrency on the BEAM, fault tolerance | Recursive and parametric types are listed as future work; not a rendering language |
| Slang (Khronos) | No 2026 release identified; Khronos reports more than 5,400 stars and cites it as the preferred shading language in the LunarG 2026 survey (moderate) | Modular shaders, many targets, automatic differentiation | Shader-only; no CPU host language or effects |
| Metal Shading Language | UNKNOWN for the 2026 version | First-party Apple GPU path | Apple only |
| Agent-first research languages (Pel, Quasar) | Papers from 2025; no adopted 2026 agent-first language was found (moderate) | Grammars small enough for reliable LLM generation; Pel parallelizes independent operations | Research proposals with no compiler to native code, games or shaders |

Two conclusions follow. First, Bend 2 has moved into BuildLang's own thesis: "the checker is the
truth" for agent code. Bend answers with proofs and BuildLang answers with capability effects,
receipts and a checker that refuses anything the C backend cannot compile. The two can coexist,
since proofs cost effort per function and effects cost nothing to write. Second, nobody else ships
an agent-first language that also targets games and shaders. That niche is open. No claim here says
BuildLang leads; the milestones below set out how a lead would be measured.

## 3. Thesis

BuildLang is the language an agent writes in a loop with a checker it can trust. The type says what
code may do (capability effects). The receipt says what it did. A third party re-derives the receipt.
The checker refuses every program the backend cannot compile, so a pass at `buildc check` means the
program builds.

The extension: the same loop for creative work. A game system, a renderer pass or a shader written
by an agent carries the same guarantees: bounded authority, deterministic replay, and a golden the
output must match. Determinism is part of the type story. A function with no effects in its row is
pure, so it can run in parallel and replay bit for bit.

## 4. Milestones, each with a check that fails

Each milestone names a check, a bar and what fails it. A milestone counts as met only when its check
ran against a release on crates.io.

### M3. Held-out agent pass rate

- Check: a new held-out probe of at least 30 tasks written after this date, sealed by hash and pushed
  before any generator sees them, graded with the `bench/heldout-2026-10-15` harness against the
  current crates.io release. At least 6 tasks are game-shaped (fixed-step simulation, data table
  load, trace writing).
- Bar: pass@1 of at least 25/30 for each of two independent generators, and zero
  check-pass-but-fail programs.
- Fails if either generator is below 25/30, or if any program passes `buildc check` and then fails
  to build, crashes or prints the wrong output.

### M4. The language gaps a game rebuild needs

The pilot (section 5) lists them. The check is the pilot's own goldens:

- DETMATH golden table: every row of the golden table in the Low Water `design/DETMATH.md`
  reproduced bit for bit (hex compare) by a BuildLang implementation of `dsin dcos datan datan2
  dasin dacos dexp dhypot`. Fails on any differing bit.
- Move data: a BuildLang loader and expander for `combat/data/moves.json` whose output equals
  `combat/tests/moves-expanded.golden.json` byte for byte. Fails on any differing byte.
- Traces: a BuildLang combat step that writes `lw-trace/1` records reproducing the 40-fight corpus
  manifest. Fails on any differing manifest hash.
- C ABI: the C++ host links a BuildLang-built static library through a plain C header and passes the
  same tests as the C++ module it replaces. Fails if any test differs.

### M5. Parallel by default, the answer to Bend

- Design: a function whose effect row is empty and whose arguments are owned or immutable can run in
  parallel without a lock. `par_map` and `par_for` over such functions lower to a thread pool on the
  C backend. Results are combined in index order, so output is identical to the serial run. An effect
  row that names `Console`, `FileSystem` or any mutable capability is rejected inside a parallel body,
  with a BL code and an edit that hoists the effect out.
- Check: a benchmark set (n-body step, per-pixel shading on the CPU, the pilot's 40-fight corpus
  run fight by fight) with at least 10 timed runs each, reporting median and a 95% interval.
- Bar: at least 4x speed-up on 8 cores for the n-body and pixel kernels, and byte-identical output to
  the serial run on every benchmark.
- Fails on any output difference, or if the lower bound of the interval is below 4x.
- Scope note: Bend parallelizes recursion with no annotation. BuildLang will not claim parity with
  that. The claim it can test is deterministic parallelism whose safety the checker proves from the
  effect row.

### M6. A production GPU path

- Design: `#[compute]` kernels emitted as WGSL (the source of truth on the Low Water side) and HLSL,
  dispatched through a small host runtime on Windows (D3D12 or WebGPU through a native wrapper) and
  Linux (Vulkan).
- Check: a device run of each kernel in a golden suite (integer kernels compared exactly, float
  kernels against a CPU reference within a declared ULP bound), run in CI on a machine with a GPU, and
  a receipt that records device, driver and kernel hash.
- Fails if a kernel has no device run, or any result falls outside its bound.
- Until M6 passes, the GPU path stays labeled experimental. The 12 default-feature GPU tests do not
  establish device behavior.

### Order

M4 comes first, because the pilot's features (section 5) are the same gaps M3's game-shaped tasks
will hit. Then M3, M5 and M6. Issue #83 (Box and recursive types) is the first M4 item and ships in
1.8.0.

## 5. The pilot: the Low Water combat sim, later

The port is not started. It waits on the C++ port's parity test (P3) and on the features below. The
features, in the order they land:

1. Recursive data through `Box<T>` (#83). Needed for the trace writer's nested records and any tree
   the sim builds.
2. IEEE 754 binary64 rules the DETMATH spec states: no contraction (`-ffp-contract=off` in every C
   compile the backend drives, checked by a test that greps the compile line), no fast-math, no
   reassociation in lowering, and `sqrt`, `floor`, `abs`, `min`, `max` lowered to the correctly
   rounded C calls. A test pins the bit pattern of each DETMATH golden.
3. Bit-exact float to shortest-string formatting (the `JSON.stringify` rule: shortest round-trip
   form, `-0` written `0`, non-finite written `null`). The current `{}` formatting is not specified
   to this rule; status UNKNOWN until tested against the trace goldens.
4. 32-bit wrapping integer arithmetic with an explicit operator or method (`hashStr` uses xor and a
   wrapping multiply), plus `fmod` with the trace format's zero-divisor rule.
5. A JSON reader for `moves.json`: objects, arrays, numbers and strings, into typed structs, with a
   checked error for a missing key. The reader is a library on the effect system's `FileSystem`
   capability.
6. A JSON-lines writer with a fixed key order and sorted nested keys, byte-stable across runs.
7. `HashMap<str, T>` for any element type `T`. The C backend's map is specialized to a few element
   types today.
8. A C ABI export surface: `#[export] extern "C" fn`, `#[repr(C)]` structs, a generated C header,
   and a static library output. BuildLang can call C today; the pilot needs the other direction.
9. Multi-shot or resumable handlers are not required by the sim. Handlers stay one-shot for the pilot.

Status 2026-10-23: items 2 to 8 shipped in 1.9.0 (#95, #96, #97, #101, #102, #103). The pilot has
started outside this repository. Against the JS sim's 40-fight corpus, BuildLang reads and rewrites
every trace record byte for byte, reproduces the 40 `cbegin` records, gives the same world hash as
JS for all 611 `cstate` and `cend` states (given JS's state text), and expands `moves.json` to the
168,234-byte golden exactly. The combat step itself (the `cin`, `cev`, `cstate` and `cend`
records) is not ported yet, so the trace check of M4 is not met.

## 6. What this document does not prove

It records a direction, a field survey and checks. It does not show BuildLang ahead of any language
listed, and no vendor benchmark above was reproduced. The research table's UNKNOWN cells need a
primary source before any public comparison uses them.

## 7. Surpass targets and parity (author, 2026-10-23)

The author's targets ("We want BuildLang to beat and surpass both of these" and "Feature parity in
where the competitors excel as well") are milestones in `docs/STRATEGY-2026-10-SURPASS.md`: parity
rows P1 to P8, verification targets V1 to V8 and spec milestones S1 to S4, each with a check that can
fail, ordered by what the game rebuild needs first.

## Sources (fetched 2026-10-22)

- Bend 2 repository README: https://github.com/bendlang/bend
- Bend 2 review with measurements, 2026-09-19: https://akitaonrails.com/en/2026/09/19/new-ai-language-just-released-bend-2/
- HVM2: https://github.com/HigherOrderCO/HVM2
- Mojo 1.0: https://modular.com/blog/modular-26-5-mojo-1-0-is-here and https://www.phoronix.com/news/Modular-Mojo-Open-Source
- Zig 0.16: https://ziggit.dev/tag/zig-0-16
- Jai: https://en.wikipedia.org/wiki/Jai_(programming_language)
- Julia 1.12: https://julialang.org/blog/2025/10/julia-1.12-highlights/index.html
- Elixir 1.20: https://elixir-lang.org/blog/2026/06/03/elixir-v1-20-0-released
- Slang: https://www.khronos.org/blog/khronos-at-siggraph-2026
- Rust releases: https://blog.rust-lang.org/releases
- Pel: https://arxiv.org/abs/2505.13453
- Anthropic formal verification (read 2026-10-23 per the targets note): https://www.anthropic.com/research/formal-verification
