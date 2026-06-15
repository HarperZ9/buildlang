# QuantaLang

[![crates.io](https://img.shields.io/crates/v/quantalang.svg)](https://crates.io/crates/quantalang)
[![docs.rs](https://img.shields.io/docsrs/quantalang)](https://docs.rs/quantalang)
[![VS Code Marketplace](https://img.shields.io/visual-studio-marketplace/v/HarperZ9.quantalang?label=VS%20Code)](https://marketplace.visualstudio.com/items?itemName=HarperZ9.quantalang)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**The Effects Language** - a Rust-built compiler for typed effects, systems
experiments, and shader-oriented code generation.

QuantaLang compiles `.quanta` source files to **C** as the primary verified
execution path, emits **HLSL** and **GLSL** for shader work, and keeps SPIR-V,
LLVM IR, WebAssembly, Rust source, x86-64, and ARM64 backends labeled as
experimental research surfaces.

**Landing page:** [harperz9.github.io/quantalang](https://harperz9.github.io/quantalang/)

## Install

From crates.io (recommended):

```bash
cargo install quantalang
# binary: quantac
```

Or build from source:

```bash
cd compiler
cargo build --release
```

Add `target/release/quantac` (or `target\release\quantac.exe` on Windows) to your PATH.

Verify your local toolchain:

```bash
quantac doctor
```

`doctor` reports the installed compiler version, C-backend readiness, stdlib and
local registry discovery, optional backend tools, and the current backend
maturity table.

## Editor support

Install the **[QuantaLang VS Code extension](https://marketplace.visualstudio.com/items?itemName=HarperZ9.quantalang)** - syntax highlighting, brackets, comment toggles. Grammar source: [HarperZ9/quantalang-tmLanguage](https://github.com/HarperZ9/quantalang-tmLanguage).

## Quick Start

Create `hello.quanta`:

```
fn main() {
    println!("Hello, World!");
}
```

Compile and run:

```bash
quantac run hello.quanta
```

The repository also carries tested quickstart examples:

```bash
quantac run examples/quickstart/hello.quanta
quantac run examples/quickstart/ledger.quanta
quantac run examples/quickstart/effects_greeting.quanta
quantac examples/quickstart/vignette_shader.quanta --target hlsl -o vignette_shader.hlsl
```

Or compile to C and build manually:

```bash
quantac hello.quanta -o hello.c
cc hello.c -o hello
./hello
```

## Shader Example

QuantaLang can compile shader code directly to HLSL or GLSL. Create `vignette.quanta`:

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
quantac vignette.quanta --target hlsl -o vignette.fx
```

Compile to GLSL (for OpenGL / Vulkan):

```bash
quantac vignette.quanta --target glsl -o vignette.glsl
```

## CLI Commands

| Command         | Description                          |
|-----------------|--------------------------------------|
| `quantac lex`   | Tokenize a file and print tokens     |
| `quantac parse` | Parse a file and print the AST       |
| `quantac check <file> [--receipt PATH|-] [--policy policy.json|--profile NAME]` | Type-check, optionally evaluate policy, and optionally emit a JSON accountability receipt |
| `quantac build` | Build a project                      |
| `quantac run`   | Compile and run a `.quanta` file     |
| `quantac doctor` | Diagnose local toolchain readiness  |
| `quantac policy list [--json]` / `quantac policy print <name>` / `quantac policy scaffold <receipt.json>` | List, emit, or scaffold check policy profiles |
| `quantac receipt verify <receipt.json> [--source PATH] [--expect-profile NAME] [--expect-policy-digest HEX] [--json]` | Re-check a saved accountability receipt against current source inputs and optional policy expectations |
| `quantac corpus verify [--root DIR] [--write]` | Verify semantic corpus receipts and C stdout; optionally refresh the C receipt |

## Capability Effects

`quantac check` now treats direct ambient runtime access as typed effects. A
function that calls helpers such as `read_file`, `write_file`, `tcp_connect`,
`process_exit`, `getenv`, `clock_ms`, Vulkan runtime helpers, known `quanta_*`
C runtime helper aliases, or an `extern` function/static must declare the
matching capability effect in its signature:

```quanta
fn load_config() ~ FileSystem {
    read_file("ops.toml");
}

extern "C" { fn touch(); }
extern "C" { static QUANTA_ERRNO: i32; }

fn call_foreign() ~ Foreign {
    touch();
    let code = QUANTA_ERRNO;
}
```

Known runtime C helper aliases declared through `extern` blocks are classified
by their actual capability instead of being flattened into generic FFI. For
example, `quanta_gfx_init` requires `Gpu`, `quanta_read_file` requires
`FileSystem`, and `quanta_tcp_connect` requires `Network`. Unknown extern
functions and foreign statics remain `Foreign`.

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
`let ops = if use_secret { secret } else { config }`, and tuple destructuring
of the same shape, merge branch origins such as `load_config` and `load_secret`
into `ops.loader` without requiring stale branch-local paths such as
`config.loader` or `secret.loader`;
struct-field shorthand with aggregate values such as `Outer { ops }`, including
direct destructuring of `Outer { ops }`, refreshes descendant paths the same way
without requiring stale `ops.loader` evidence;
stored enum-variant aggregate payloads such as `let slot =
Slot::Ready(replacement); match slot { Slot::Ready(ops) => ... }` refresh the
branch-local path the same way;
shadowing an aggregate with an opaque producer such as
`let ops = make_ops()` also clears the previous binding's descendant origins,
so old helpers do not survive under the reused name;
whole-struct assignment such as
`ops = defaults` refreshes member origins without leaking stale intermediate
paths such as `defaults.loader`; enum-variant payloads preserve their stored
callback sources when matched; immediate calls through
returned function values record sources such as `make_loader()`. `if` and
`match` expressions that select an
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
so stale sources do not survive `loader = load_secret`,
`ops.loader = load_secret`, `ops = defaults`, or
`loaders[0] = load_secret`.
Async blocks follow the same delayed-effect model for type checking: creating
`let task = async { read_file("ops.toml") };` is pure, while `task.await`
inherits `FileSystem` and records both the awaited source (`task`) and the
latent ambient origin (`task <- read_file`) as propagated evidence. If control
flow selects between async blocks with different capability effects, the
selected future keeps the union of those effects and their origins until it is
awaited, so an `if` or `match` selected task can record branch origins such as
`task <- read_file` and `task <- getenv`.

`quantac check --receipt` also binds each receipt to the checked source inputs
with SHA-256 digests plus compiler and language version metadata. The top-level
`source_digest` records the entry file, `input_digests` records every entry,
import, include, and module file that feeds the check pipeline, and
`input_graph_digest` gives CI a portable fingerprint for the exact source graph
that passed or failed the capability gate.
`quantac receipt verify receipt.json` re-runs the check input graph and confirms
the saved receipt still matches the current source bytes, compiler/language
identity, graph digest, file-backed policy digest, any recorded built-in profile
digest, and the replayed accountability surfaces (`declared_effects`,
`observed_capabilities`, `propagated_effects`, diagnostics, and policy
violations). Add `--json` to emit a
`quantalang-receipt-verification/v1` report for CI systems that need
machine-readable pass/fail checks instead of human text.
Use `--expect-profile ci-review` when CI must reject receipts that were not
accepted under the required built-in policy profile, including receipts whose
policy object was stripped after creation.
Use `--expect-policy-digest sha256:<hex>` when CI must reject receipts that were
not accepted under a specific file-backed or built-in policy digest.

`quantac check --policy <policy.json>` evaluates a portable
`quantalang-check-policy/v1` profile against declared effects and observed
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

`quantac policy list` shows built-in baseline profiles,
`quantac policy list --json` emits a machine-readable catalog with profile
digests, and
`quantac policy print <name> --output policy.json` writes one as normal policy
JSON. The initial profiles are `pure`, `console-only`, `offline`, and
`ci-review`, plus `strict-accountability` for gates that require exact
boundary/source allowlists before ambient IO is accepted.

For the common case, `quantac check app.quanta --profile ci-review --receipt -`
evaluates a built-in profile directly without first writing a policy file.
Receipts identify these gates with a `policy.source` such as `builtin:ci-review`.
Built-in profile receipts also include `policy.profile` and
`policy.profile_digest`, so CI can distinguish official profile identity from
an equivalent file-backed policy document.
Use `--profile strict-accountability` when CI should reject every ambient
capability boundary until a printed policy adds exact direct, propagated, and
source-level allowlists, with `allowed_effects` enforced as an explicit effect
inventory.
Use `quantac policy scaffold receipt.json --output policy.json` to turn an
accountability receipt into a strict, reviewable policy skeleton with observed
direct boundaries, ambient helper/macro/FFI sources, propagated callers, and
callee sources already filled in. Scaffolded policies also enable
`require_effect_allowlist`, including for receipts that currently have no
effects.
Use `--expect-profile-digest <hex>` with `--profile` to pin check-time CI to the
digest reported by `quantac policy list --json` or by a prior trusted receipt.
Use `quantac receipt verify --expect-profile <name>` to pin verification-time CI
to the required built-in profile identity, or
`--expect-policy-digest sha256:<hex>` to pin verification to an exact policy
document digest.

Receipts separate direct capability boundaries from callers that inherit those
effects. `observed_capabilities` records ambient helper, macro, and FFI access
inside a function, such as `read_file`, `tcp_connect`, `println!`, or `touch`.
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
aliases such as `replacement.loader`. Immediate invocation
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
`if` or `match` expression records the possible effectful branch targets, such
as `load_config` and `load_secret`. If the selected function is bound first,
for example `let loader = if ...`, a later `loader()` call records `loader`
plus the possible selected targets; an explicit cast to a typed effectful
callback keeps that same source set instead of collapsing it to the local alias.
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
slot, indexed entry, or whole aggregate rebinds that call-source evidence, so
policy receipts follow mutable callback slots instead of preserving stale
earlier sources.

Policy profiles can enforce that split:

```json
{
  "schema": "quantalang-check-policy/v1",
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
`quantac corpus verify` checks the manifest, C/Rust receipts, and real
C-backend stdout from `quantac run`. `quantac corpus verify --root <DIR>`
points verification at a copied corpus, while `--write` rewrites the C
execution receipt after C stdout passes and Rust receipt alignment is still
clean. It currently covers scalar functions, locals, arithmetic, printing,
simple branching, basic structs/arrays/references, tuple ownership reuse,
struct aggregate reuse, field assignment reuse, nested field reuse, and
dereference reuse; unsupported MIR returns a codegen error rather than silent
fallback.

## Status

**132/132 test programs compile.** Full pipeline: `.quanta` -> C99 -> MSVC -> native x86-64 executable. See [TEST_RESULTS.md](TEST_RESULTS.md) for outputs.

Programs cover: functions, recursion, structs, enums, closures, generics, traits, dynamic dispatch, algebraic effects, pattern matching, iterators, hashmaps, vector math, color science, and self-hosted compiler components.

The C backend is the primary target. HLSL/GLSL produce clean shader output. SPIR-V, LLVM, WASM, Rust, x86-64, and ARM64 backends are experimental.

## Design

See [DESIGN.md](DESIGN.md) for full architectural documentation including:
- Pipeline overview (lexer -> parser -> types -> MIR -> backends)
- Type system rationale: why bidirectional inference, why Pratt parsing, why setjmp/longjmp for effects
- MIR design: SSA with basic blocks, statement/terminator model
- Known limitations: borrow/lifetime checking is still early, Rust-target validation is subset-only, eager monomorphization, one-shot effects

## Code Quality

- **CI**: clippy (correctness) + rustfmt + `cargo test` on Linux and Windows
- **Warning gate**: local `RUSTFLAGS=-Dwarnings cargo test --manifest-path compiler/Cargo.toml --quiet` is clean as of 2026-06-14
- **Error handling**: Parser uses `expect()` with messages, lexer has 30+ error variants for recovery, pkg layer uses full `Result<T, E>` propagation
- **Codegen unwraps**: Intentional assertions on validated AST (documented policy in `codegen/mod.rs`)
- **Tests**: 837 passing, 0 failing, 10 ignored, 4 filtered in local `cargo test -- --skip spirv::tests::test_triangle --skip spirv::tests::test_write` from `compiler/` on 2026-06-14
  - Type inference: 54 tests (unification, bidirectional flow, effect inference, const generics)
  - Lexer: 51 tests (token types, spans, Unicode, edge cases, error recovery)
  - Parser: 85 tests (all expression/item/pattern forms, malformed programs)
  - CLI: binary-level smoke tests cover help output, `quantac doctor`, `quantac corpus verify`, `quantac receipt verify`, explicit corpus roots, C receipt writes against copied corpus fixtures, capability diagnostics, and the runnable quickstart examples
  - Codegen: tests across 9 backends, including C formatted-print lowering, Rust source emission, Rust executable smoke checks over the semantic corpus, and semantic-corpus manifest contract/receipt consistency/metadata guards (C backend has 24 end-to-end output verification tests)

## License

MIT License. See [LICENSE](LICENSE) for details.
