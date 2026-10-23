# BuildLang strategy, October 2026: surpass targets and parity

Companion to `docs/STRATEGY-2026-10.md` (milestones M3 to M6 and the pilot). Section numbering follows that file.

## 7. Surpass targets and parity (author, 2026-10-23)

The author, verbatim: "We want BuildLang to beat and surpass both of these". The two are Anthropic and
Theorem's Claude-written Rocq proofs over RISC-V binaries (239 OSS-Fuzz entry points in 123 libraries,
per anthropic.com/research/formal-verification as read on 2026-10-23), and Bend 2 with Taelin's
roadmap. Then: "Feature parity in where the competitors excel as well." The source of record is the
mission-control note `2026-10-23-buildlang/SURPASS-TARGETS.md`.

The milestones below join M3 to M6. The order puts what the game rebuild needs first (C ABI, shaders,
deterministic numerics, parallelism), then verification, then the remaining parity rows. Each one has
a check that can fail. A milestone that matches a competitor's feature claims parity only after a
dated comparison on a shared benchmark, and a lead needs the same.

### Game-rebuild parity first

**P1. C import and cross-compilation (Zig's strength).** Import a C header directly (functions,
structs, enums, constants) and cross-compile to Windows, Linux and macOS from one host. This extends
the M4 C ABI export item. Check: the raw-native C header the pilot links against imports with no
hand-written `extern` block, and binaries built on one host for all three targets run their test
programs in CI on all three. Fails if any target's binary is missing or any test differs.

**P2. Shader modules with generics (Slang's strength).** Shader modules, generics and imports,
emitting HLSL, GLSL, WGSL and SPIR-V. Check: the raw-native style passes rebuilt as BuildLang shader
modules match their existing image goldens within each golden's stated bound. Fails on any pass
outside its bound or any target that does not compile in the vendor validator (DXC, glslang, Naga,
spirv-val).

**P3. Deterministic numerics and dispatch (Julia's strength).** The DETMATH rules from M4 as a
library, generic dispatch over numeric types, and the standard numeric kernels (dot, GEMM, FFT,
stencil). Check: the DETMATH goldens match bit for bit, and each kernel's median time over at least
10 runs is reported beside Julia's on the same machine and date. Fails on any golden bit. Parity is
claimed only where the BuildLang median is within 1.25x of Julia's.

**P4. GPU execution and automatic parallelism (Bend 2's strength).** M5 and M6 extended to CUDA,
Metal and Vulkan. Check: the same program runs on CPU and GPU with byte-identical integer output and
float output within a declared ULP bound, and Bend 2's own published examples, ported, are timed
against Bend 2 on the same machine. Fails on any output difference. Bend's numbers are vendor numbers
until reproduced here.

### Verification: the surpass targets

**V1. Safety by construction, with certificates.** Define the crash classes Anthropic's proofs cover
(out-of-bounds access, null dereference, division by zero, stack and heap exhaustion under a bound,
integer overflow where defined as a trap). For each class, either the type and effect system rules it
out or the backend inserts a defined check, and the receipt carries a certificate naming the class and
the rule that discharged it. Check: a corpus of programs seeded with each crash class (the
false-accept corpus pattern) where every seeded crash is rejected at check time or caught by an
inserted check at run time, plus a fuzzing run of the compiled binaries with zero unexplained crashes.
Fails on any seeded crash that reaches undefined behavior.

**V2. Contracts and refinement types through SMT, exported to Rocq or Lean.** `requires`, `ensures`
and refinement types (`i64 where self >= 0`) in the source, discharged by an SMT solver (Z3 or cvc5)
at check time. An obligation the solver cannot close is exported as a Rocq or Lean goal. Check: a
contract suite that includes Bend 2's published `LAWS.bend` examples, ported, plus false contracts
that must be refused, and every exported goal type-checks in the target prover. Fails if any false
contract is accepted, or any export does not load. Spec size per verified function is reported beside
Anthropic's published proof sizes.

**V3. Reward-hacking guard.** Before any proof attempt, the spec and the program are hashed into the
receipt. A proof attached to a program whose hash differs, or a patch that changes behavior on any
test in the frozen suite, is rejected unless the receipt carries a reviewer's signature over the new
hash. Check: an adversarial set of agent "fixes" that make a spec trivially true (delete the body,
narrow the input domain, change the spec) where every one is rejected without a signature. Fails on
any accepted hack.

**V4. Translation validation for compiled output.** Per-function certificates that the C (and later
native) backend output matches the source semantics, so verification holds on the binary users run
(x86 and ARM, glibc and MSVC), which closes the model gap Anthropic names for RISC-V and musl. Check:
certificates for every function in the semantic corpus and the pilot, with coverage reported, and a
mutation run where every injected miscompilation is caught. Fails on any missed mutation.

**V5. E-graph substitution with equivalence certificates.** Equality saturation rewrites algorithms
into faster target-specific forms, and each rewrite carries an equivalence certificate in the receipt
that a separate checker re-derives. This answers Taelin's "provably equivalent" substitution plan.
Check: a benchmark of rewrites with outputs equal to the unrewritten program on a frozen input set,
every certificate re-checked by the independent checker, and at least one forged certificate refused.
Fails on any output difference or accepted forgery.

**V6. Differentiable types (BendTorch's strength).** Gradients derived from effect-typed code and a
tensor library. Check: MNIST and a small transformer trained in BuildLang match a PyTorch twin
(same initialization and data order) within a stated loss tolerance per step, on CPU and on the M6 GPU
path. Fails if any step leaves the tolerance.

**V7. Verification inside the developer loop (Anthropic's skill).** The BuildLang skill, `buildc mcp`
and the language server run contract checks and show certificates in the editor. Check: the M3
held-out probe includes verification tasks, scored like the rest. Fails below the M3 bar.

**V8. Shared verification benchmark.** A benchmark of entry points comparable to the OSS-Fuzz set,
measured as tokens per verified function, coverage, and wall time, reported beside Anthropic's
published figures with the date of each. Check: the run is reproducible from its receipt by a third
party. No superiority claim until this has run.

### Specs, the hard part

The replies to Anthropic's post on X (2026-10-22, screenshots from the author) agree that writing the
spec is harder than proving it: "a proof only covers the spec, so specs become the new tests". The
milestones below make the spec BuildLang's sharpest wedge. They land with V2.

**S1. Specs in the same language as the code, kept in sync.** Contracts, laws and properties are
BuildLang declarations beside the code they describe, and a code change that breaks a spec fails
`buildc check`. Check: a change set that edits implementations under existing specs, where every
spec-breaking edit fails check with a BL code and every spec-preserving edit passes. Fails on either
kind of error.

**S2. Spec quality tools.** A spec is tested like code. It must accept a set of known-good programs,
reject known-bad ones, and reject mutants of the implementation (mutation testing against the spec).
`buildc spec explain` renders the spec in plain words beside the code for review. Check: for each spec
in the property library, every mutant in its mutation set fails the spec. A spec that lets a mutant
through is reported as weak and fails the run.

**S3. A property library.** Reusable properties for round-trip (`decode(encode(x)) == x`),
idempotence, no data races, crash consistency and authorization, plus game properties: deterministic
replay and a frame-budget bound. Check: each property is applied to a correct implementation and to a
seeded wrong one, and the wrong one fails. Fails on any wrong implementation that passes.

**S4. Abstract interpretation built in.** Interval, sign and nullness domains, with reduced products
that an agent may propose and the compiler checks for soundness before use. Check: the analysis
proves the bounds in a corpus of array-indexing programs and refuses a seeded unsound domain product.
Fails on any accepted unsound product or any false "safe" verdict on a seeded out-of-bounds access.

### Remaining parity rows

**P5. Proof laws beside the code (Bend 2).** Delivered by V2's contracts.

**P6. Python interop (Mojo).** Call a Python library from BuildLang and be called from Python, with
types round-tripped. Check: import a pure Python library and a C-extension library (NumPy), and
round-trip ints, floats, strings, lists and arrays. Fails on any round-trip difference.

**P7. Packages and a registry (Rust).** A package manager with a lockfile and reproducible builds.
Check: publish, resolve and build a three-package graph offline from the lockfile, twice, with
identical output hashes. Fails on any hash difference or network access.

**P8. Supervised actors (Elixir).** Actors with supervision trees built on the effect system.
Check: crash-and-restart tests (one-for-one and one-for-all) where the supervisor restarts the right
children within a bounded number of attempts and the actor's message order is preserved. Fails on any
lost or reordered message.

### Order after #83

1. M4 pilot gaps, with P1 (C import and cross-compilation) and P3's DETMATH library.
2. P2 shader modules, then M5 and P4 parallelism and GPU, with M6.
3. V2 contracts with S1 to S3, and the V3 guard, then V1 certificates and S4.
4. V5 e-graph substitution, V4 translation validation, V6 differentiable types.
5. V7 in the developer loop and M3's next probe, then V8 the shared benchmark.
6. P6 to P8.

The game and engine stay first; BuildLang runs in parallel.

Does not prove: building any of these features does not beat a competitor. Only a measured, dated
comparison on a shared benchmark can support a parity or lead claim. Taelin's unreleased work is
unknown, and nothing here is claimed against it.

