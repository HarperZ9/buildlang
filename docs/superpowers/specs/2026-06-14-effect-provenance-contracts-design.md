# Effect Provenance Contracts Design

Date: 2026-06-14
Status: Design captured for review; implementation not started

## Purpose

BuildLang already makes ambient operational power visible as typed effects:
direct file, network, process, environment, clock, console, GPU, and FFI access
must appear in function signatures and check receipts. The next accountability
step is provenance: distinguish a function that directly touches an ambient
surface from a function that only propagates an effect because it calls another
effectful function.

This slice turns `buildc check --receipt --policy` from a flat allow/deny gate
into a practical operational contract system:

- direct ambient access can be restricted to narrow boundary functions;
- higher-level orchestration functions can propagate approved effects without
  receiving direct access to the underlying capability;
- receipts can show the route by which operational power entered a function;
- CI can reject code that uses the right effect in the wrong place.

## Existing Context

- `compiler/src/types/infer.rs` records direct capability sources in
  `capability_sources: BTreeMap<String, BTreeSet<String>>`.
- `TypeInfer::record_call_capability` and `record_macro_capability` add direct
  ambient effects such as `FileSystem`, `Console`, and `Network`.
- `compiler/src/types/check.rs` exposes `FunctionEffectSummary` with
  `declared_effects` and `observed_capabilities`.
- `compiler/src/main.rs` serializes those summaries into
  `buildlang-check-receipt/v1` and evaluates `buildlang-check-policy/v1`.
- `source_digest`, `input_digests`, and `input_graph_digest` already bind the
  receipt to the checked source graph.
- The next-generation assessment recommends this slice as the highest-leverage
  direction for Build as an operational accountability language.

## Problem

Current evidence is flat at the function boundary. If a receipt says:

```json
{
  "declared_effects": {
    "main": ["FileSystem"]
  }
}
```

policy tooling cannot tell whether `main` directly called `read_file` or only
called `load_config`, where `load_config` is the intended filesystem boundary.

That is too coarse for real adoption. Practical policies need to distinguish:

- direct source: `main -> read_file`;
- propagated source: `main -> load_config -> FileSystem`.

## Design

### Receipt Extension

Add a top-level `propagated_effects` object to
`buildlang-check-receipt/v1`:

```json
{
  "observed_capabilities": {
    "load_config": {
      "FileSystem": ["read_file"]
    }
  },
  "propagated_effects": {
    "main": {
      "FileSystem": ["load_config"]
    }
  }
}
```

Rules:

- `observed_capabilities` keeps its current meaning: direct ambient call or
  macro sources observed inside the function body.
- `propagated_effects` records local function calls that introduce declared
  effects into the caller body.
- Each propagated source value is a callee name, not a runtime helper.
- The field is additive and backward-compatible.
- Empty propagated maps should serialize as empty maps for each checked
  function, matching the current receipt style for capability maps.
- Values must be deterministic: function names sorted by receipt map order,
  effect names sorted by map order, and callee lists sorted.

### Type Checker Evidence

Extend `FunctionEffectSummary`:

```rust
pub struct FunctionEffectSummary {
    pub function: String,
    pub declared_effects: Vec<String>,
    pub observed_capabilities: BTreeMap<String, BTreeSet<String>>,
    pub propagated_effects: BTreeMap<String, BTreeSet<String>>,
}
```

`observed_capabilities` remains the direct ambient evidence. The new
`propagated_effects` map is collected by `TypeInfer` while inferring calls.

### Inference Integration

When `TypeInfer` sees a call expression with a simple name or simple path:

1. Keep current direct capability handling for runtime helpers and macros.
2. If the callee type is a function type with an effect row, add those effects
   to `current_effects` as it already does today.
3. Also record `effect_name -> callee_name` in a new propagated effect source
   ledger when the effect came from the callee function type.
4. Do not record propagation for direct ambient runtime helpers. Those stay
   only in `observed_capabilities`.
5. Do not record propagation when the callee name cannot be resolved to a
   stable source name.

The first implementation should be local and conservative. It does not need a
whole-program transitive call graph. If `main` calls `run`, and `run` calls
`load_config`, the first receipt can show:

```json
{
  "propagated_effects": {
    "main": {
      "FileSystem": ["run"]
    },
    "run": {
      "FileSystem": ["load_config"]
    }
  }
}
```

That is enough for CI and review tooling to follow the chain without making the
compiler solve global provenance in this slice.

### Policy Extension

Keep the existing schema string, `buildlang-check-policy/v1`, because these
fields are optional and additive:

```json
{
  "schema": "buildlang-check-policy/v1",
  "allowed_effects": ["Console", "FileSystem"],
  "denied_effects": ["Network", "Foreign"],
  "direct_effect_allowlist": {
    "FileSystem": ["load_config"],
    "Console": ["main", "report"]
  },
  "propagated_effect_allowlist": {
    "FileSystem": ["main", "run"]
  },
  "require_input_graph_digest": true
}
```

Policy behavior:

- `denied_effects` remains global and wins over every other rule.
- `allowed_effects`, when non-empty, applies to declared effects, direct
  observed capabilities, and propagated effects.
- `direct_effect_allowlist` applies only to `observed_capabilities`.
- `propagated_effect_allowlist` applies only to `propagated_effects`.
- If an allowlist map omits an effect, that effect has no extra allowlist
  restriction for that surface.
- If an allowlist map includes an effect with an empty function list, no
  function may use that effect on that surface.
- `require_input_graph_digest` requires the top-level receipt field to exist
  with `algorithm == "sha256"` and a 64-character lowercase hex digest.

### Policy Violations

Extend policy violation evidence with source details while keeping the current
fields stable:

```json
{
  "kind": "DirectEffectNotAllowed",
  "effect": "FileSystem",
  "function": "main",
  "surface": "observed_capabilities",
  "source": "read_file",
  "message": "policy does not allow direct effect `FileSystem` in `main`"
}
```

New violation kinds:

- `DirectEffectNotAllowed`: direct ambient capability found in a function not
  listed for that effect.
- `PropagatedEffectNotAllowed`: callee-propagated effect found in a function not
  listed for that effect.
- `MissingInputGraphDigest`: policy requires a SHA-256 input graph digest and
  the receipt evidence lacks one.

Existing violation kinds remain valid:

- `DeniedEffect`
- `DisallowedEffect`
- `MissingSourceDigest`

Sorting remains deterministic by function, effect, surface, source, kind, and
message.

## Data Flow

1. Parser and type collection run unchanged.
2. `TypeInfer` records direct capability sources exactly as today.
3. `TypeInfer` additionally records effectful local function calls as
   propagated effect sources.
4. `TypeChecker` copies both direct and propagated evidence into
   `FunctionEffectSummary`.
5. `run_check` carries the summaries into `CheckOutcome`.
6. `build_check_receipt` serializes `propagated_effects`.
7. `evaluate_check_policy` builds policy evidence from declared effects,
   observed capabilities, propagated effects, source digest, and input graph
   digest.
8. `cmd_check` preserves existing exit behavior: type/parse failure or policy
   failure exits nonzero.

## Error Handling

- Unknown policy fields remain tolerated for forward compatibility.
- Invalid allowlist shapes should fail JSON/schema parsing with a clear
  configuration diagnostic.
- `--receipt -` must keep stdout as parseable JSON; human policy diagnostics
  stay on stderr.
- Type-checking failures should still produce partial provenance evidence when
  inference reached the relevant calls.

## Testing Strategy

Implementation must be test-first.

Initial red tests:

- Type checker: direct `read_file` records `observed_capabilities.FileSystem`
  and no propagated source.
- Type checker: `main` calling `load_config`, where `load_config ~ FileSystem`,
  records `propagated_effects.main.FileSystem == ["load_config"]`.
- CLI receipt: `--receipt -` includes `propagated_effects` beside
  `observed_capabilities`.
- CLI policy: `direct_effect_allowlist.FileSystem == ["load_config"]` rejects
  `main` when `main` directly calls `read_file`.
- CLI policy: the same direct allowlist allows `main` to call `load_config`
  when `main` only propagates `FileSystem`.
- CLI policy: `propagated_effect_allowlist.FileSystem == []` rejects a caller
  that propagates `FileSystem`.
- CLI policy: `require_input_graph_digest` passes for current receipts and
  reports `MissingInputGraphDigest` in the unit evaluator fixture when the
  digest is absent or not SHA-256.

Verification for the implementation branch:

- `cargo test --manifest-path compiler/Cargo.toml provenance --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml capability --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --quiet`
- `RUSTFLAGS=-Dwarnings cargo test --manifest-path compiler/Cargo.toml --quiet`
- `python -m pytest -q tests/test_docs_landing_page.py`
- `git diff --check`
- diff-level secret scan before commit and push

## Non-Goals

- No OS sandboxing.
- No runtime enforcement.
- No signing or certificate chain.
- No network-fetched policies.
- No whole-program fixed-point call graph solver.
- No backend code generation changes.
- No package registry policy discovery.
- No schema version bump unless implementation discovers an unavoidable
  backward-compatibility break.

## Acceptance

The slice is acceptable when:

- existing `buildc check` behavior is unchanged without `--policy` or
  `--receipt`;
- receipts distinguish direct ambient capability sources from propagated
  effectful callee sources;
- direct and propagated policy rules can independently allow or reject the same
  effect in different functions;
- policy receipts include deterministic, structured violations with enough
  source detail for review tooling;
- source and input graph digest requirements are enforceable by policy;
- existing capability, receipt, policy, full compiler, warning-clean compiler,
  docs, diff hygiene, secret scan, CI, and Pages gates pass.
