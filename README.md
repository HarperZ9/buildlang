<p align="center">
  <img src="https://raw.githubusercontent.com/HarperZ9/buildlang/main/docs/brand/buildlang-hero.png" alt="BuildLang, the Effects Language compiler in the Build ecosystem">
</p>
<!-- Project mark: https://raw.githubusercontent.com/HarperZ9/buildlang/main/docs/brand/buildlang-mark.svg -->

# BuildLang

> The Effects Language: a Rust-built compiler for typed effects, systems experiments, and shader-oriented code generation.

[Build ecosystem](https://github.com/HarperZ9/build-universe) | [buildlang](https://github.com/HarperZ9/buildlang) | [build-universe](https://github.com/HarperZ9/build-universe) | [VS Code extension](https://github.com/HarperZ9/buildlang-vscode) | [grammar](https://github.com/HarperZ9/buildlang-tmLanguage)

[![license: fair-source](https://img.shields.io/badge/license-fair--source-blue.svg)](LICENSE)
![rust](https://img.shields.io/badge/rust-edition_2021-orange.svg)
[![crates.io](https://img.shields.io/badge/crates.io-buildlang-blue.svg)](https://crates.io/crates/buildlang)
![version](https://img.shields.io/badge/version-1.0.x-informational.svg)
[![CI](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/buildlang/actions/workflows/ci.yml)
[![part of: Build ecosystem](https://img.shields.io/badge/part_of-Build_ecosystem-00b3a4.svg)](https://github.com/HarperZ9/build-universe)

**The Effects Language** - a Rust-built compiler for typed effects, systems
experiments, and shader-oriented code generation.

BuildLang compiles `.bld` source files to **C** as the primary verified
execution path, emits **HLSL** and **GLSL** for shader work, and keeps SPIR-V,
LLVM IR, WebAssembly, Rust source, x86-64, and ARM64 backends labeled as
experimental research surfaces.

**Landing page:** [harperz9.github.io/buildlang](https://harperz9.github.io/buildlang/)

## Current status

- **Release:** BuildLang 1.0.x; compiler binary `buildc`; built with Rust (edition 2021). C is the production-grade verified backend; HLSL and GLSL ship for shader work; SPIR-V, LLVM IR, WebAssembly, Rust, x86-64, and ARM64 stay labeled experimental.
- **Type system:** Hindley-Milner inference with typed algebraic effects, plus an **experimental** opt-in `#[linear]` attribute toward **no-cloning** — a `#[linear]` struct/enum value should be moved/consumed at most once (the foundation shared by quantum qubits, on-chain no-double-spend, and resource handles). It conservatively rejects a large, regression-tested set of compositional escapes, but it is **not yet fully sound** (a few known-open classes remain; full soundness needs an affine/borrow checker on MIR). Borrows do not consume; ordinary types keep copy-like reuse. Honest scope, what's enforced, and what's open: [docs/LINEAR-TYPES.md](docs/LINEAR-TYPES.md); also `examples/linear/`, [CHANGELOG.md](CHANGELOG.md), `docs/QUANTUM-HOST.md`.
- **Operator surface:** the `buildc` CLI exposes `lex`, `parse`, `check` (with `--receipt` / `--policy`), `build`, `run`, `test`, `repl`, `fmt`, `pkg`, `watch`, `doctor`, `corpus`, `policy`, `receipt`, and an `lsp` subcommand that starts a bundled LSP server (completion, hover, diagnostics, go-to-definition, semantic tokens). The CLI and the LSP server are the two integration surfaces; accountability receipts (`buildlang-receipt-verification/v1`) carry SHA-256 source digests for re-checkable codegen.
- **Accountable scientific compute:** a second receipt family (`buildlang-scientific-runtime-receipt/v0`) seals a re-checkable proof that a numeric kernel's output series satisfies a stated invariant, verified by RE-RUNNING the program. Six invariants ship (energy-monotone, conservation, bounded, energy-identity, relation, conserved-band), each with a paired negative-fixture kernel; `buildc receipt export` emits witnessed measurement rows. Honest scope: it witnesses the observed series, not the model or any physical law. See [Accountable scientific compute](#accountable-scientific-compute) and [docs/SCIENTIFIC-RECEIPT.md](docs/SCIENTIFIC-RECEIPT.md).
- **Umbrella:** part of the operator's Build ecosystem alongside `build-universe`, the VS Code extension, and the TextMate grammar; standalone and not dependent on any single host.
- **Repository naming:** public product names are BuildLang, `buildc`, and `.bld`; the crate is [`buildlang`](https://crates.io/crates/buildlang) on crates.io and the repo is [`HarperZ9/buildlang`](https://github.com/HarperZ9/buildlang) on GitHub. The former `quantalang` crate is deprecated and points here.
- **Housekeeping:** ground-truth release evidence lives in `STATUS.md`; [CHANGELOG.md](CHANGELOG.md) tracks the current presentation pass under Unreleased.

## Install

From crates.io (installs the `buildc` binary):

```bash
cargo install buildlang
```

> Previously published as `quantalang`; that crate is deprecated and now points
> here. Use `buildlang` / `buildc`.

Or build from the repository source:

```bash
cd compiler
cargo build --release
```

Add `compiler/target/release/buildc` (or
`compiler\target\release\buildc.exe` on Windows) to your PATH.

Verify your local toolchain:

```bash
buildc doctor
```

`doctor` reports the installed compiler version, C-backend readiness, stdlib and
local registry discovery, optional backend tools, the current backend maturity
table, and the substrate receipt evidence posture for the semantic corpus.

## Editor support

VS Code extension sources live in `editors/vscode`: syntax highlighting,
brackets, comment toggles, file icons, and optional `buildc lsp` process
startup. LSP request dispatch is still partial; see
`compiler/src/lsp/STATUS.md`.

## Quick Start

Create `hello.bld`:

```
fn main() {
    println!("Hello, World!");
}
```

Compile and run:

```bash
buildc run hello.bld
```

The repository also carries tested quickstart examples:

```bash
buildc run examples/quickstart/hello.bld
buildc run examples/quickstart/ledger.bld
buildc run examples/quickstart/effects_greeting.bld
buildc examples/quickstart/vignette_shader.bld --target hlsl -o vignette_shader.hlsl
```

Or compile to C and build manually:

```bash
buildc hello.bld -o hello.c
cc hello.c -o hello
./hello
```

## Usage

See [USAGE.md](USAGE.md) for an install/build line, the full command and
backend reference, and worked examples (run, type-check with a policy receipt,
and shader output) with expected output. A runnable demo lives in
[examples/demo](examples/demo).

## For developers

The main implementation lives under `compiler/`. Use the targeted checks below
before changing public compiler behavior, receipts, corpus verification, or the
CLI surface:

```bash
cargo test --manifest-path compiler/Cargo.toml --bin buildc --quiet
cargo fmt --manifest-path compiler/Cargo.toml --check
cargo clippy --manifest-path compiler/Cargo.toml -- -D clippy::correctness -A clippy::complexity -A clippy::style -A clippy::pedantic -A clippy::perf
buildc doctor
buildc corpus verify
git diff --check
```

Keep `.bld` examples, `buildc` command docs, receipts, and semantic-corpus
evidence aligned. When behavior changes, update tests and public docs in the
same branch.

## Shader Example

BuildLang can compile shader code directly to HLSL or GLSL. Create `vignette.bld`:

```
fn vignette(uv_x: f64, uv_y: f64, strength: f64, softness: f64) -> f64 {
    let dx = uv_x - 0.5;
    let dy = uv_y - 0.5;
    let dist = sqrt(dx * dx + dy * dy);
    let vig = smoothstep(0.5, 0.5 * softness, dist);
    1.0 - strength * (1.0 - vig)
}

#[fragment]
fn PS_Vignette(uv: vec2) -> vec4 {
    let color = tex2d(uv);
    let vig = vignette(color.x, color.y, 0.5, 0.6);
    vec4(color.x * vig, color.y * vig, color.z * vig, 1.0)
}
```

Compile to HLSL (for ReShade / DirectX):

```bash
buildc vignette.bld --target hlsl -o vignette.fx
```

Compile to GLSL (for OpenGL / Vulkan):

```bash
buildc vignette.bld --target glsl -o vignette.glsl
```

## CLI Commands

| Command         | Description                          |
|-----------------|--------------------------------------|
| `buildc lex`   | Tokenize a file and print tokens     |
| `buildc parse` | Parse a file and print the AST       |
| `buildc check <file> [--receipt PATH|-] [--policy policy.json|--profile NAME]` | Type-check, optionally evaluate policy, and optionally emit a JSON accountability receipt |
| `buildc build` | Build a project                      |
| `buildc run`   | Compile and run a `.bld` file     |
| `buildc run <file> --emit-receipt <path> --invariant <NAME> [--columns N]` | Run a numeric kernel and seal a re-checkable **scientific-runtime** receipt over a stated invariant (see [Accountable scientific compute](#accountable-scientific-compute)) |
| `buildc receipt export <receipt.json>` | Re-verify a scientific-runtime receipt and emit witnessed measurement rows |
| `buildc doctor` | Diagnose local toolchain readiness  |
| `buildc policy list [--json]` / `buildc policy print <name>` / `buildc policy scaffold <receipt.json>` | List, emit, or scaffold check policy profiles |
| `buildc receipt verify <receipt.json> [--source PATH] [--expect-profile NAME] [--expect-policy-digest HEX] [--json]` | Re-check a saved accountability receipt against current source inputs and optional policy expectations |
| `buildc corpus verify [--root DIR] [--write]` | Verify semantic corpus receipts and C stdout; optionally refresh the C receipt |

## Accountable scientific compute

Beyond the capability (check) receipts described below, `buildc` emits a second,
independent receipt family for **numeric** programs: a **scientific-runtime
receipt** (`buildlang-scientific-runtime-receipt/v0`).

`buildc run --emit-receipt <path>` compiles and runs a `.bld` kernel, captures
its numeric stdout as a measurement series, checks a stated **invariant** over
that series, and seals a re-checkable JSON receipt. `buildc receipt verify`
RE-RUNS the program and re-derives the verdict, so drift, tamper, or a source
change fails with a typed `failure_class` and a verdict-gated exit code. A
verifier that cannot fail proves nothing, so every invariant ships a paired
negative-fixture kernel that must fail for the right reason. `buildc receipt
export` re-verifies and emits witnessed measurement rows for downstream
ingestion.

The invariant family (each a fixed, re-checked tolerance with a paired
positive/negative kernel):

| `--invariant` | checks | example kernel |
|---|---|---|
| `energy-monotone` | the series never increases beyond tolerance | heat-equation discrete energy (stable FTCS) |
| `conservation` | stays within roundoff of its initial value | a rotation preserving `r^2` |
| `bounded` | never exceeds its initial value (the discrete maximum principle) | an undamped oscillator's `x^2` |
| `energy-identity` | a quantitative per-step energy-balance residual held at roundoff | the FTCS discrete energy identity |
| `relation` (`--columns N`) | a row's columns agree (the VERIFIER compares them) | `sin(2t)` computed two ways |
| `conserved-band` | stays within a fixed error budget of its initial value (approximate conservation) | a symplectic leapfrog oscillator's energy |

Honest scope: the receipt witnesses that the compiled program's OBSERVED OUTPUT
SERIES satisfies (or expectedly violates) the invariant. It does **not** prove
the underlying model correct and does **not** claim a physical law (every
receipt carries a `NOT_A_NEW_PHYSICAL_LAW` label). Full field reference,
exit-code semantics, and the failure-class vocabulary:
[docs/SCIENTIFIC-RECEIPT.md](docs/SCIENTIFIC-RECEIPT.md).

## Capability Effects

`buildc check` now treats direct ambient runtime and compile-time access as
typed effects. A function that calls helpers such as `read_file`, `write_file`,
`tcp_connect`, `process_exit`, `getenv`, `clock_ms`, Vulkan runtime helpers,
known `build_*` C runtime helper aliases, compile-time include/environment
macros, or an `extern` function/static must declare the matching capability
effect in its signature:

```build
fn load_config() ~ FileSystem {
    read_file("ops.toml");
}

extern "C" { fn touch(); }
extern "C" { static BUILD_ERRNO: i32; }

fn call_foreign() ~ Foreign {
    touch();
    let code = BUILD_ERRNO;
}
```

Known runtime C helper aliases declared through `extern` blocks are classified
by their actual capability instead of being flattened into generic FFI. For
example, `build_gfx_init` requires `Gpu`, `build_read_file` requires
`FileSystem`, and `build_tcp_connect` requires `Network`. Unknown extern
functions and foreign statics remain `Foreign`.

Compile-time ambient macros are gated as capability access too:
`include!`, `include_str!`, and `include_bytes!` require `FileSystem`, while
`env!` and `option_env!` require `Environment`. Receipts record the exact macro
source, such as `include_str!` or `env!`, under `observed_capabilities`.
Macro argument token trees are scanned for ambient capability use as well, so
`println!(read_file("ops.toml"))` requires both `Console` and `FileSystem` and
records `println!` plus `read_file` in the receipt. The scan is backed by
`SourceId` provenance, so the same gate applies when a macro invocation lives
inside an external `mod` file. Unknown extern calls and foreign static reads
inside macro arguments are also surfaced as direct `Foreign` boundaries.

If the effect is missing, the checker reports the required capability and a
diagnostic note naming the ambient call or macro that triggered it. This is the
first security gate for practical ops/accountability use: file, network,
process, environment, clock, GPU, console helper/macro, and FFI surfaces are
represented in the language's effect vocabulary instead of remaining invisible
compiler side channels.
Qualified helper paths are covered too: `io::read_file()` is classified from
its `read_file` leaf and recorded as the exact source `io::read_file`.
First-class function types can carry capability effects as well: a parameter
written as `loader: fn() with FileSystem` forces callers of `loader()` to
declare or handle `FileSystem`, and `(fn() -> str) with FileSystem` supports
effectful callbacks that return data while keeping callback provenance in
receipts. Function effect rows are enforced during type unification, so an
effectful callback cannot be passed into a pure `fn(...)` slot and silently
erase `FileSystem`, `Network`, `Process`, `Foreign`, or any other declared
capability. Wrappers that receive an effectful callback argument keep
caller-side evidence as well, so `run(load_config)` records both `run` and
`load_config` as propagated `FileSystem` sources. Effectful inherent methods
and associated functions carry the same declared effects through call syntax, so
`config.load()` can require `FileSystem` and record `Config.load` as propagated
evidence, while `Config::load()` records `Config::load`. Dynamic dispatch
through `dyn Loader` also checks the trait method signature, so
`loader.load()` records `Loader.load` instead of bypassing the capability gate.
Ambient helpers used as values keep those effects too:
`let loader = read_file; loader("ops.toml");` requires `FileSystem` and records
`loader` as the propagated source. Closure literals capture their body effects
without performing them at definition time, so `let loader = |path: str|
read_file(path);` remains pure until `loader("ops.toml")` is called, and then
records `loader` as propagated `FileSystem` evidence; immediately invoked
anonymous closures record the synthetic source `<closure>`. Effectful function
values stored in structs, tuple structs, and enum variants, including
struct-like variants such as `Slot::Ready { loader: load_config }`, stay pure
until called and keep source evidence: `(ops.loader)("ops.toml")` records
`ops.loader`; tuple slots, tuple-struct fields, and indexed ops tables record
sources such as `loaders.0`, `slot.0`, and `loaders[0]`; repeated ops tables
such as `[load_config; 2]` retain `load_config` origins alongside indexed
access paths such as `loaders[1]`; struct updates such as
`Ops { ..defaults }` preserve inherited field origins such as `load_config`
alongside new access paths such as `ops.loader`; nested updates such as
`Outer { ..defaults }` carry descendant origins such as `load_config` to
access paths such as `outer.ops.loader`; destructuring that nested bundle with
`let Outer { ops } = outer`, or destructuring the update expression itself with
`let Outer { ops } = Outer { ..defaults }`, carries the same origin to
`ops.loader`; explicit update-field replacement such as
`Outer { ops: replacement, ..defaults }` also refreshes the destructured path
without leaking stale intermediate sources such as `replacement.loader`;
tuple-literal destructuring such as `let (ops,) = (replacement,)` applies the
same refresh rule, keeping `load_config` and `ops.loader` without requiring
stale `replacement.loader` evidence;
control-flow-selected aggregate bindings such as
`let ops = if use_secret { secret } else { config }`, `if let` selected
aggregate fields such as `Outer { ops: if let ... }`, and tuple destructuring
of the same shapes, merge branch origins such as `load_config` and
`load_secret` into `ops.loader` or `outer.ops.loader` without requiring stale
branch-local paths such as `config.loader` or `secret.loader`;
struct-field shorthand with aggregate values such as `Outer { ops }`, including
direct destructuring of `Outer { ops }`, refreshes descendant paths the same way
without requiring stale `ops.loader` evidence;
stored enum-variant aggregate payloads such as `let slot =
Slot::Ready(replacement); match slot { Slot::Ready(ops) => ... }` refresh the
branch-local path the same way;
shadowing an aggregate with an opaque producer such as
`let ops = make_ops()` also clears the previous binding's descendant origins,
so old helpers do not survive under the reused name, including when the new
binding lives in an inner block and is copied through another local;
whole-struct assignment such as
`ops = defaults` refreshes member origins without leaking stale intermediate
paths such as `defaults.loader`; enum-variant payloads preserve their stored
callback sources when matched; immediate calls through
returned function values record sources such as `make_loader()`. `if`,
`if let`, and `match` expressions that select an
effectful function value record every possible branch target, for example
`load_config` and `load_secret`; binding that selected function before calling
it records both the binding and the possible selected targets, even when the
selected value is explicitly cast to a typed effectful callback such as
`(fn() -> str) with FileSystem` or called through a reference/dereference pair
such as `let loader_ref = &loader; (*loader_ref)()`. A cast to a pure callback
type such as `as fn() -> str` is rejected when the source carries effects, so an
explicit cast cannot erase the capability row. Pipe application is checked as
real function application too, so `"ops.toml" |> load_config` requires
`FileSystem` and records `load_config` as propagated evidence. Ordinary binary
operators reject function values, so `load_config >> load_secret` cannot pretend
to compose callbacks while skipping the call-effect gate. Tuple, tuple-struct,
struct, enum-variant, and slice destructuring keep that source evidence too, so
`let (loader,) = (...)`, `let Slot(loader) = slot`,
`let Ops { loader } = ops`, `let Outer { ops } = outer`,
`let Slot::Ready(loader) = slot`, and `let Slot::Ready { loader } = slot`, and
`let [loader] = loaders` continue to record the selected callees as well as
`loader`, and nested destructuring keeps descendant origins such as
`load_config` for `ops.loader`; branch-local `if let` and
`while let` destructuring enforce the same declared effect gate. The `?`
operator is rejected on plain callback values, so `loader?()` cannot turn an
effectful callback into an untracked unknown call. The `.await` operator is
rejected on plain callback values too, so `loader.await` cannot launder a
selected effectful callback into a future output. Later
assignment to a callback variable or aggregate member refreshes that evidence,
including when a nested block mutates an outer callback alias or ops slot, so
stale sources do not survive `loader = load_secret`,
`ops.loader = load_secret`, `ops = defaults`, or
`loaders[0] = load_secret`. Conditional `if`, `if let`, `match`, explicit
`loop`/`break`, and zero-or-more loop assignments merge possible
post-control-flow sources, so
`if use_secret { loader = load_secret }` keeps both the original and assigned
callback origins in later receipts,
`if let Slot::Ready(v) = slot { loader = load_secret }` keeps the pre-branch
source too,
`match mode { 0 => { loader = load_secret } _ => { loader = load_backup } }`
keeps both arm-assigned origins,
`loop { if stop { break; }; loader = load_secret; break; }` keeps both break
exit origins, and `while reload { loader = load_secret }`,
`while let Slot::Ready(v) = slot { loader = load_secret }`, or
`for item in items { loader = load_secret }` keeps the pre-loop source too.
Async blocks follow the same delayed-effect model for type checking: creating
`let task = async { read_file("ops.toml") };` is pure, while `task.await`
inherits `FileSystem` and records both the awaited source (`task`) and the
latent ambient origin (`task <- read_file`) as propagated evidence. If control
flow selects between async blocks with different capability effects, the
selected future keeps the union of those effects and their origins until it is
awaited, so an `if` or `match` selected task can record branch origins such as
`task <- read_file` and `task <- getenv`.

`buildc check --receipt` also binds each receipt to the checked source inputs
with SHA-256 digests plus compiler and language version metadata. The top-level
`source_digest` records the entry file, `input_digests` records every entry,
import, include, and module file that feeds the check pipeline, and
`input_graph_digest` gives CI a portable fingerprint for the exact source graph
that passed or failed the capability gate.
`buildc receipt verify receipt.json` re-runs the check input graph and confirms
the saved receipt still matches the current source bytes, compiler/language
identity, graph digest, file-backed policy digest, any recorded built-in profile
digest, and the replayed accountability surfaces (`declared_effects`,
`observed_capabilities`, `propagated_effects`, diagnostics, and policy
violations). Add `--json` to emit a
`buildlang-receipt-verification/v1` report for CI systems that need
machine-readable pass/fail checks instead of human text.
Use `--expect-profile ci-review` when CI must reject receipts that were not
accepted under the required built-in policy profile, including receipts whose
policy object was stripped after creation.
Use `--expect-policy-digest sha256:<hex>` when CI must reject receipts that were
not accepted under a specific file-backed or built-in policy digest.

`buildc check --policy <policy.json>` evaluates a portable
`buildlang-check-policy/v1` profile against declared effects and observed
capabilities. Policy failures make the check fail even when type checking
passes, and receipts record the policy path, policy digest, status, and
structured violations.
Effect names in `allowed_effects`, `denied_effects`, and allowlist keys are
validated against built-in capability effects and the effects present in the
checked source graph, so misspelled policy gates fail as `UnknownPolicyEffect`
instead of silently weakening CI.
Set `require_effect_allowlist` when `allowed_effects` should be authoritative
even when it is empty. Strict profiles and scaffolded policies enable it so a
pure receipt stays pure: later declared, observed, or propagated effect drift
must be added to the policy deliberately.

`buildc policy list` shows built-in baseline profiles,
`buildc policy list --json` emits a machine-readable catalog with profile
digests, and
`buildc policy print <name> --output policy.json` writes one as normal policy
JSON. The initial profiles are `pure`, `console-only`, `offline`, and
`ci-review`, plus `strict-accountability` for gates that require exact
boundary/source allowlists before ambient IO is accepted.

For the common case, `buildc check app.bld --profile ci-review --receipt -`
evaluates a built-in profile directly without first writing a policy file.
Receipts identify these gates with a `policy.source` such as `builtin:ci-review`.
Built-in profile receipts also include `policy.profile` and
`policy.profile_digest`, so CI can distinguish official profile identity from
an equivalent file-backed policy document.
Use `--profile strict-accountability` when CI should reject every ambient
capability boundary until a printed policy adds exact direct, propagated, and
source-level allowlists, with `allowed_effects` enforced as an explicit effect
inventory.
Use `buildc policy scaffold receipt.json --output policy.json` to turn an
accountability receipt into a strict, reviewable policy skeleton with observed
direct boundaries, ambient helper/macro/FFI sources, propagated callers, and
callee sources already filled in, including compile-time file/environment
macros such as `include_str!` and `env!`. Scaffolded policies also enable
`require_effect_allowlist`, including for receipts that currently have no
effects.
Use `--expect-profile-digest <hex>` with `--profile` to pin check-time CI to the
digest reported by `buildc policy list --json` or by a prior trusted receipt.
Use `buildc receipt verify --expect-profile <name>` to pin verification-time CI
to the required built-in profile identity, or
`--expect-policy-digest sha256:<hex>` to pin verification to an exact policy
document digest.

Receipts separate direct capability boundaries from callers that inherit those
effects. `observed_capabilities` records ambient helper, macro, and FFI access
inside a function, such as `read_file`, `tcp_connect`, `include_str!`, `env!`,
`println!`, or `touch`.
`propagated_effects` records effectful callees that made a caller inherit a
typed effect. Raw unknown extern-block calls are direct `Foreign` boundaries;
foreign static reads are direct `Foreign` boundaries; known runtime helper
aliases declared in extern blocks are direct entries under their domain
capability such as `Gpu` or `FileSystem`; calls to local wrappers around those
extern functions are propagated dependencies. Qualified ambient helpers keep
their full source path, such as
`io::read_file`, so source allowlists can distinguish equivalent helper names
from different modules. This lets teams permit a small audited boundary
function while still proving which higher-level workflows depend on it.
Effectful inherent methods and associated functions are propagated dependencies
as well: calling `config.load()` where `Config.load` declares `~ FileSystem`
records `Config.load` under the caller's `propagated_effects`, and
`Config::load()` records `Config::load`. Effectful trait-object method calls
behave the same way: calling `loader.load()` through `dyn Loader` records
`Loader.load`, so dynamic dispatch remains visible to source allowlists.
Effectful callback parameters are also propagated sources, so a wrapper that
calls `loader: fn() with FileSystem` records `loader` as the inherited
`FileSystem` source. When a caller supplies an effectful callback to that
wrapper, receipts record the supplied callback source too, so `run(load_config)`
can be reviewed or allowlisted by both `run` and `load_config`. The same rule
covers effect-row compatibility itself: `fn run(loader: fn(str) -> str)` cannot
accept `read_file` because the pure callback boundary does not declare
`FileSystem`. Aliases of ambient helpers, such as
`let loader = read_file`; calling the alias inherits the helper's capability
effect instead of falling back to an untyped function value. Effectful closures
use the same function-value path: creating `|path: str| read_file(path)` does
not trigger `FileSystem`, but calling a bound closure records the alias as a
propagated source. Calling an anonymous closure immediately records `<closure>`
as the propagated source. Tuple-struct and enum-variant construction, including
struct-like enum variants, can store an effectful callback without adding
propagated receipt evidence until that callback is called. Calls through
effectful struct fields, tuple slots, tuple-struct fields, and indexed ops
tables record paths such as `ops.loader`, `loaders.0`, `slot.0`, and
`loaders[0]`, and repeated tables retain callback origins such as
`load_config` next to access paths such as `loaders[1]`; struct updates retain
inherited field origins next to new access paths such as `ops.loader`, so
source allowlists can constrain capability-bearing registries and ops tables;
nested updates and later destructuring retain descendant callback origins next
to paths such as `outer.ops.loader` and `ops.loader`; direct update-expression
destructuring preserves the same inherited origins, and explicit update-field
replacements or aggregate-literal destructuring refresh those paths without
requiring stale replacement aliases in source allowlists.
Whole-aggregate assignments refresh member origins too, so `ops = defaults`
does not leave stale `defaults.loader` evidence in later `ops.loader` calls.
Enum-variant payloads keep their stored callback sources when a match, `if let`,
or `while let` branch destructures them, and stored aggregate payloads refresh
branch-local paths such as `ops.loader` without requiring stale construction
aliases such as `replacement.loader`. Nested `if let` selected aggregate fields
also merge each possible field origin into paths such as `outer.ops.loader`.
Immediate invocation
of a returned effectful function records the factory call, such as
`make_loader()`.
Async blocks also delay capability effects at construction time: `async {
read_file("ops.toml") }` stores the effect and its ambient source on the future
value, and `task.await` records both the awaited source (`task`) and latent
origin (`task <- read_file`) under `propagated_effects`. Futures selected by
`if` or `match` merge their stored capability effects and source origins, so
awaiting a selected task requires every possible branch capability and leaves
receipt evidence for each possible origin.
Control-flow selectors keep reviewable evidence too: calling the result of an
`if`, `if let`, or `match` expression records the possible effectful branch
targets, such as `load_config` and `load_secret`. If the selected function is
bound first, for example `let loader = if ...` or `let loader = if let ...`,
a later `loader()` call records `loader` plus the possible selected targets; an
explicit cast to a typed effectful callback keeps that same source set instead
of collapsing it to the local alias.
References keep it too, so `(*loader_ref)()` records the selected branch targets,
`loader`, and `loader_ref`. `?` is limited to fallible values and is rejected on
plain callback values, so `loader?()` cannot erase the selected callback's effect
row. `.await` is limited to futures and is rejected on plain callback values, so
`loader.await` cannot erase the selected callback's latent effect row. Pure
function casts are checked against function effect rows, so
`as fn() -> str` cannot launder an effectful selected callback. Pipe expressions
such as `"ops.toml" |> load_config` use the same effect gate as
`load_config("ops.toml")`, so operator syntax cannot bypass propagated
capability evidence. Ordinary binary operators reject function values, so
`load_config >> load_secret` is a type error rather than fake composition. The
same source binding is preserved through
tuple, tuple-struct, struct, enum-variant, and slice destructuring, including
branch-local `if let` and `while let` patterns, so
`let (loader,) = (...)`, `let Slot(loader) = slot`, `let Ops { loader } = ops`,
`let Slot::Ready(loader) = slot`, `let Slot::Ready { loader } = slot`, and
`let [loader] = loaders` do not collapse a selected effectful function down to
only the local alias. Plain assignment to an identifier, struct field, tuple
slot, indexed entry, or whole aggregate rebinds that call-source evidence,
including across nested blocks that mutate an outer binding, so policy receipts
follow mutable callback slots instead of preserving stale earlier sources.
Assignments inside `if`, `if/else`, `match`, `while`, and `for` are merged
conservatively so receipts show every callback source that can reach the later
call.

Policy profiles can enforce that split:

```json
{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["FileSystem", "Network"],
  "direct_effect_allowlist": {
    "FileSystem": ["load_config"]
  },
  "direct_capability_source_allowlist": {
    "FileSystem": {
      "load_config": ["read_file"]
    }
  },
  "propagated_effect_allowlist": {
    "FileSystem": ["main"]
  },
  "propagated_effect_source_allowlist": {
    "FileSystem": {
      "main": ["load_config"]
    }
  },
  "require_source_digest": true,
  "require_input_graph_digest": true,
  "require_effect_allowlist": true,
  "require_provenance_allowlists": true,
  "require_source_allowlists": true,
  "require_allowlist_coverage": true
}
```

Set `require_effect_allowlist` when CI should reject any declared, observed, or
propagated effect not named in `allowed_effects`, including the case where that
list is intentionally empty.
Set `require_provenance_allowlists` when CI should require every direct
capability boundary and propagated capability caller to be explicitly named.
Use `direct_capability_source_allowlist` when an approved direct boundary must
also name the exact ambient helper, macro, or FFI source allowed inside that
function.
Use `propagated_effect_source_allowlist` when an approved caller may inherit an
effect only through specific effectful callees.
Set `require_source_allowlists` when CI should require exact source entries for
every approved direct capability boundary and propagated caller.
Set `require_allowlist_coverage` when CI should also reject stale direct or
propagated allowlist entries, including source-level direct capability and
propagated-effect entries, that are not matched by the current receipt evidence.

### Backend Selection

Use `--target` to select a code generation backend:

| Target   | Flag                          | Output  | Status       |
|----------|-------------------------------|---------|--------------|
| C        | `--target c` (default)        | `.c`    | Working      |
| HLSL     | `--target hlsl`               | `.hlsl` | Working      |
| GLSL     | `--target glsl`               | `.glsl` | Working      |
| SPIR-V   | `--target spirv`              | `.spv`  | Experimental |
| LLVM IR  | `--target llvm`               | `.ll`   | Experimental |
| WASM     | `--target wasm`               | `.wasm` | Experimental |
| Rust     | `--target rust` / `--target rs` | `.rs`   | Experimental |
| x86-64   | `--target x86-64`             | `.o`    | Experimental |
| ARM64    | `--target arm64`              | `.o`    | Experimental |

The Rust target emits source for a subset of MIR and is validated with
`rustc --emit=metadata` plus a small executable stdout smoke corpus. The
semantic corpus manifest now drives a Rust execution test, so corpus paths,
expected stdout, generated Rust, `rustc`, and executable behavior are checked
together; manifest contract, receipt consistency, and metadata tests keep the
corpus and Rust execution receipt aligned. The C backend now has a matching
semantic-corpus execution receipt for the same 8 programs, and
`buildc corpus verify` checks the manifest, C/Rust receipts, and real
C-backend stdout from `buildc run`. `buildc corpus verify --root <DIR>`
points verification at a copied corpus, while `--write` rewrites the C
execution receipt after C stdout passes and Rust receipt alignment is still
clean. It currently covers scalar functions, locals, arithmetic, printing,
simple branching, basic structs/arrays/references, tuple ownership reuse,
struct aggregate reuse, field assignment reuse, nested field reuse, and
dereference reuse; unsupported MIR returns a codegen error rather than silent
fallback.

`buildc corpus verify` also validates a Substrate Receipt
(`buildlang-substrate-receipt/v0`) for the same semantic corpus. This receipt
aggregates existing evidence across semantic, execution, memory, representation,
and command surfaces: C remains the production execution anchor, Rust remains an
experimental subset lane, and unverified GPU/native lanes must keep explicit
maturity and unsupported-behavior labels. The receipt is an evidence contract,
not a backend promotion claim.
The same verification path now validates a MIR Representation Receipt
(`buildlang-mir-representation-receipt/v0`) that recomputes per-program MIR
module counts, symbols, operation families, memory-surface flags, and
control-flow summaries from the real parse, type-check, and AST-to-MIR lowering
pipeline. This makes the representation claim inspectable without promoting any
experimental backend.
The substrate path also carries a checked
`buildlang-memory-layout-receipt/v0` artifact for the semantic corpus. It
recomputes per-program manifest memory tags, MIR-derived memory flags,
ownership-surface classification, layout-scope classification, source/input/MIR
digests, and explicit known gaps during `buildc corpus verify`. This is a
representation-level RAM/memory evidence receipt, not a byte-offset ABI layout
claim, allocator proof, async runtime memory proof, or full interprocedural
borrow proof.
The substrate path now also carries a checked
`buildlang-symbol-graph-receipt/v0` artifact that recomputes source/MIR/effect
symbol evidence during `buildc corpus verify` without claiming call graph, LSP
readiness, or package API completion.

### Native FFI: header-backed extern blocks

An `extern` block can name the C header that backs its declarations with a
`header "..."` clause, and the library to link with a `link "..."` clause. The
two clauses may appear in either order after the ABI. The C backend emits the
matching `#include` and trusts that header for the prototypes, types, and
macros instead of synthesizing its own declaration, and `buildc build` passes
the named library to the C compiler. This is how BuildLang integrates a
third-party C-ABI library natively and links it in one command, and through the
C ABI it reaches any language that exposes one, such as C, C++, Rust, and Zig:

```build
extern "C" link "sqlite3" header "<sqlite3.h>" {
    fn sqlite3_libversion() -> &str;
}

fn main() ~ Foreign {
    let version = sqlite3_libversion();
}
```

A header written in angle-bracket form (`"<sqlite3.h>"`) becomes
`#include <sqlite3.h>` for system and library headers; any other form
(`"mylib.h"`) becomes `#include "mylib.h"` for local headers. Headers are
emitted once each and in sorted order so generated C stays reproducible for
receipts. A `link "sqlite3"` clause adds the library to the C compiler
invocation (`-lsqlite3` for gcc/clang/cc, `sqlite3.lib` for MSVC) and records a
`// buildc-link: sqlite3` note in the emitted C so the requirement is visible
under `--emit c`. An extern block with no `header` clause keeps the existing
behavior: buildc synthesizes a prototype for non-standard functions and relies
on the standard includes for the C library. Foreign `static` declarations work
the same way: a `static` carries the block's `header`/`link` and is emitted as
an external reference (the header declares it, or buildc emits a bare
`extern <type> <name>;`), never a conflicting definition. A function may end
with a C-style `...` to declare it variadic, so `printf`-family functions work:

```build
extern "C" {
    fn printf(fmt: &str, ...) -> i32;
}

fn main() ~ Foreign {
    printf("%d and %d\n", 1, 2);
}
```

A variadic call may pass more arguments than there are fixed parameters (the
extra ones are unchecked, as in C), while a non-variadic call still requires an
exact argument count. Foreign calls still require the `Foreign` capability
effect, so native interop stays inside the same accountability gate as every
other ambient surface.

### Exporting BuildLang functions to C

Interop runs both directions. An `extern "C" fn` definition gives a BuildLang
function C linkage and a stable, unmangled symbol name, so it compiles to a
non-`static` C function that C, and any language that speaks the C ABI, can
call directly:

```build
extern "C" fn buildlang_square(n: i32) -> i32 {
    n * n
}
```

```c
// from C:
extern int buildlang_square(int n);
int r = buildlang_square(7);  // 49
```

Ordinary BuildLang functions stay internal (`static`) so a whole-program build
keeps a clean symbol table; only functions you explicitly mark `extern "C"` are
exported. `buildc build --emit header` writes a `main.h` declaring those
exports (with an include guard and a `#ifdef __cplusplus extern "C"` guard), so
C and C++ consumers can `#include` it instead of hand-writing prototypes.
Together with header-backed extern blocks, this closes the loop: BuildLang can
call into any C-ABI library and be called by any C-ABI consumer.

## Status

The current release-shaped proof is the Cargo baseline above: `cargo test`
from `compiler/` on 2026-07-02 produced lib 940, bin 135, cli 307, lexer
52, parser 88 passing tests (0 failing), with `buildc corpus verify` 8/8.
[TEST_RESULTS.md](TEST_RESULTS.md) is retained as a
historical C-backend output record, not the current release gate; the legacy
`buildc test` fixture runner now needs a Console-capability annotation pass
before it can be used as a public green-corpus claim again.

The broader fixture corpus covers functions, recursion, structs, enums,
closures, generics, traits, dynamic dispatch, algebraic effects, pattern
matching, iterators, hashmaps, vector math, color science, and historical
self-hosted compiler components. Treat that as a mixed regression/design corpus;
the current release-shaped proof is the Cargo baseline and 8-program semantic
corpus receipt path above.

The C backend is the primary target. HLSL/GLSL produce clean shader output. SPIR-V, LLVM, WASM, Rust, x86-64, and ARM64 backends are experimental.

## Design

See [DESIGN.md](DESIGN.md) for full architectural documentation including:
- Pipeline overview (lexer -> parser -> types -> MIR -> backends)
- Type system rationale: why bidirectional inference, why Pratt parsing, why setjmp/longjmp for effects
- MIR design: SSA with basic blocks, statement/terminator model
- Known limitations: borrow/lifetime checking is still early, Rust-target validation is subset-only, eager monomorphization, one-shot effects
- Wind-down/backend assessment: [COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md](docs/COMPILER_WIND_DOWN_ASSESSMENT_2026-06-15.md)

## Code Quality

- **CI**: clippy (correctness) + rustfmt + `cargo test` on Linux and Windows
- **Warning gate**: local `RUSTFLAGS=-Dwarnings cargo build --manifest-path compiler/Cargo.toml` is clean as of 2026-06-30; re-run before making a current warning-clean claim
- **Error handling**: Parser uses `expect()` with messages, lexer has 30+ error variants for recovery, pkg layer uses full `Result<T, E>` propagation
- **Codegen unwraps**: Intentional assertions on validated AST (documented policy in `codegen/mod.rs`)
- **Tests**: lib 940, bin 135, cli 307, lexer 52, parser 88 passing (0 failing) in local `cargo test` from `compiler/` on 2026-07-02
  - Library (940): type inference + effects + linear-type no-cloning, lexer/parser units, MIR + codegen across backends, the semantic-corpus receipt builders, and LSP dispatch
  - Lexer: 52 integration tests (token types, spans, Unicode, edge cases, error recovery)
  - Parser: 88 integration tests (all expression/item/pattern forms, malformed programs)
  - CLI (307): binary-level smoke tests over help/`doctor`/`corpus verify`/`receipt verify`, capability diagnostics, the scientific-runtime receipt round trips (six invariants, each with a positive and negative kernel), runnable quickstart examples, and end-to-end C-backend execution checks (including the Option<i64>, 64-bit-literal, and overflow-safe-arithmetic regressions)
  - Codegen: tests across the C/Rust/HLSL/GLSL/SPIR-V/LLVM/WASM/x86-64/ARM64 backends, with the C path verified end-to-end against the semantic corpus

## License

BuildLang Fair-Source License v1.0 — source-available, **not** open source: the
source is published so you can read it, run it, and build on it, while commercial
use that competes with the project is reserved. See [LICENSE](LICENSE) for the
full terms.
