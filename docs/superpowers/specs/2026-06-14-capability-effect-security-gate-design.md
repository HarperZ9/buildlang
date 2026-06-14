# Capability / Effect Security Gate Design

Date: 2026-06-14
Status: Approved for implementation planning

## Purpose

QuantaLang should make operational power visible at the language boundary.
The first security slice turns ambient runtime access into typed effects:
programs that touch files, network sockets, process control, environment
variables, clocks, console output, or foreign functions must declare those
capabilities in function signatures or handle them explicitly.

This positions `quantac` as an accountability compiler: source code does not
silently gain access to outside-world surfaces merely because a runtime helper
exists.

## Existing Context

The compiler already has the pieces needed for a first gate:

- `compiler/src/types/effects.rs` defines effect rows and built-in effect
  machinery.
- `compiler/src/parser/item.rs` parses `fn name(...) ~ EffectA + EffectB`
  annotations.
- `compiler/src/types/infer.rs` accumulates callee effects and explicit
  `perform Effect.operation(...)` effects.
- `compiler/src/types/check.rs` compares body effects against declared function
  effects and emits diagnostics for unhandled or undeclared effects.
- `compiler/src/codegen/runtime.rs` exposes runtime helpers for file IO,
  stdin/CLI, process, directory traversal, TCP, environment, clocks, Vulkan,
  and C-level foreign integration.
- `semantic-corpus/manifest.json` and `semantic-corpus/receipts/` already act
  as behavior evidence carriers.

The missing layer is a compiler-owned map from ambient runtime calls and foreign
items to required effects.

## Design

### Capability Effects

The first capability vocabulary is intentionally small and concrete:

| Effect | Surfaces |
|---|---|
| `Console` | `println!`, `print`, terminal stdout/stderr helpers |
| `FileSystem` | `read_file`, `write_file`, `file_exists`, `read_bytes`, `write_bytes`, `append_file`, `list_dir`, `is_dir`, `file_size` |
| `Network` | `tcp_connect`, `tcp_send`, `tcp_recv`, `tcp_close` |
| `Process` | `exit`, `process_exit` |
| `Environment` | `getenv`, CLI argument access if promoted beyond normal test harness use |
| `Clock` | `clock_ms`, `time_unix` |
| `Foreign` | `extern` functions, foreign statics, and direct FFI declarations |
| `Gpu` | Vulkan/runtime GPU helpers, staged after the first IO/process/network slice |

These names are compiler-facing and public. They should remain stable once
released because they become part of the language's accountability contract.

### Capability Registry

Add a compiler-owned registry beside the type/effect layer. It should answer:

- Given a callable name, which capability effect does it require?
- Given an `extern` block or foreign item, which capability effect does it
  require?
- Is a callable pure, runtime-local, or outside-world touching?
- What diagnostic label should be shown to the user?

The first implementation can be a static table. It should not depend on codegen
backend internals. Codegen may use the same names, but the type/effect checker
owns the accountability decision.

### Inference Integration

When `TypeInfer::infer_call` sees a callee whose identifier is present in the
capability registry, it adds that capability effect to `current_effects` before
returning the call type.

When the checker collects foreign functions from an `extern` block, their
function type should carry the `Foreign` effect. If finer attribution is needed
later, `Foreign` can be joined with `FileSystem`, `Network`, or `Process`
through attributes, but the first slice treats all foreign calls as `Foreign`.

Macro handling starts with `println!` because it is the public quickstart path.
The first implementation may classify `println!` as `Console` during macro
inference/lowering. If implementation complexity is high, `println!` can be
documented as the second test in the same design while file/process/network
calls land first.

### Diagnostics

The existing undeclared-effect diagnostics should become capability-aware.

Required behavior:

- `fn main() { read_file("x"); }` fails with `FileSystem` required.
- `fn main() ~ FileSystem { read_file("x"); }` passes.
- `fn main() ~ Network { read_file("x"); }` fails and reports the missing
  `FileSystem` effect.
- `extern "C" { fn puts(msg: &str); } fn main() { puts("x"); }` fails with
  `Foreign` required.

Diagnostics should include:

- the ambient call or foreign item name;
- the required capability effect;
- the function whose signature is missing the effect;
- a concrete signature fix using existing `~ Effect` syntax.

### Receipts

Extend semantic-corpus receipts after compiler enforcement is working.

The receipt extension should record, per program:

- `declared_effects`: effects appearing in the function signatures relevant to
  execution;
- `observed_capabilities`: capability effects inferred from ambient calls and
  explicit `perform`;
- `capability_gate`: `passed`;
- `capability_gate_test`: the cargo or CLI test proving the gate.

The receipt schema can accept these fields as extra JSON during the first slice.
Existing receipt verification should tolerate the fields and later require them
for programs that declare operational capabilities.

## Data Flow

1. Parser builds existing AST nodes for function effects, calls, macros,
   `perform`, and extern blocks.
2. Type checker registers user-defined effects and built-in capability effects.
3. Type inference inspects calls/macros/foreign declarations against the
   capability registry.
4. Inference accumulates capability effects into the current function body row.
5. Function checking compares observed body effects against declared effects.
6. Diagnostics reject undeclared capabilities before code generation.
7. Corpus verification records declared and observed capabilities in receipts.

## Testing Strategy

Implementation must be test-first.

Initial red tests:

- Unit test: capability registry maps `read_file` to `FileSystem`, `tcp_connect`
  to `Network`, `process_exit` to `Process`, `getenv` to `Environment`, and
  extern calls to `Foreign`.
- Type checker test: undeclared `read_file` produces an undeclared effect.
- Type checker test: `~ FileSystem` permits `read_file`.
- Type checker test: wrong declared effect does not permit the call.
- Parser/type checker test: `extern` call requires `Foreign`.
- CLI smoke test: `quantac check` reports a capability diagnostic on a fixture.
- Receipt test: corpus capability metadata is preserved once receipt fields are
  introduced.

Verification for the implementation branch:

- `cargo test --manifest-path compiler/Cargo.toml capability --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --test cli capability --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --quiet`
- `RUSTFLAGS=-Dwarnings cargo test --manifest-path compiler/Cargo.toml --quiet`
- `python -m pytest -q tests/test_docs_landing_page.py` if public docs change

## Rollout

Phase 1:

- Add capability registry.
- Register built-in capability effects.
- Gate direct runtime calls for file, network, process, environment, and clock.
- Add diagnostics and tests.

Phase 2:

- Add `println!` / console macro classification.
- Annotate quickstart and semantic corpus programs that intentionally use
  console output.
- Extend receipts with capability metadata.

Phase 3:

- Gate `extern` and FFI surfaces with `Foreign`.
- Add negative tests for foreign calls without `~ Foreign`.
- Document FFI as an explicit accountability boundary.

Phase 4:

- Add `Gpu` capability classification for Vulkan/runtime GPU helpers.
- Connect GPU/resource effects to later resource-lifetime work.

## Non-Goals

- No OS sandboxing.
- No process isolation.
- No cryptographic signing.
- No policy engine.
- No broad standard-library migration.
- No syntax redesign beyond using existing `~ EffectA + EffectB` annotations.
- No claim that experimental backends enforce every capability until tests prove
  that backend path.

## Acceptance

The first implementation plan is acceptable when:

- ambient file/process/network/environment/clock calls cannot compile inside a
  pure function;
- the same calls compile when the correct effect is declared;
- wrong effect declarations fail;
- foreign calls require `Foreign`;
- diagnostics name the capability and the triggering call;
- existing compiler tests still pass;
- public docs describe the security gate as an implemented compiler feature only
  after the tests are green.
