# Substrate Receipts v0 Design

Date: 2026-06-18
Status: Approved design slice; implementation plan pending user review

## Purpose

BuildLang and `buildc` already expose several pieces of the long-term native
language thesis: a Rust compiler, a C execution anchor, typed capability
effects, source-bound check receipts, input graph digests, semantic corpus
receipts, and experimental CPU/GPU/backend surfaces. Those pieces are useful,
but they are still reported through separate artifacts.

Substrate Receipts v0 creates one machine-readable evidence layer that describes
what a BuildLang program touches, represents, lowers to, and proves. The goal is
to make CPU, GPU, memory, semantics, symbols, representations, backend maturity,
and verification evidence visible to humans and machines through the same
contract.

The first slice is intentionally an evidence aggregation layer. It does not
promote any experimental backend, add runtime mediation, or claim full native
self-hosting. It gives the project a native substrate vocabulary that future
compiler, GPU, memory, and self-hosting work can extend without rewriting public
claims each time.

## Current Evidence

- `buildc check --receipt` emits deterministic accountability receipts with
  declared effects, observed capability sources, propagated effects, source
  digest metadata, transitive input digests, and an input graph digest.
- `buildc receipt verify` re-checks saved accountability receipts against the
  current source graph and optional policy or profile expectations.
- `buildc corpus verify` validates the semantic corpus manifest, C execution
  receipt, Rust execution receipt, and current C-backend stdout.
- The C backend is the product execution anchor. It is the only backend with the
  current production execution claim.
- The Rust backend is an experimental subset validation lane with metadata and
  generated-executable stdout tests over selected semantic corpus programs.
- HLSL/GLSL shader output, LLVM, WASM, SPIR-V, x86-64, and ARM64 are selectable
  or preserved research surfaces with explicit maturity limits.
- `compiler/src/codegen/backend/STATUS.md`, `STATUS.md`, `README.md`, and the
  semantic corpus receipts already contain most of the evidence needed for a
  first substrate receipt.

## Alternatives Considered

### Approach A: Backend Promotion

Pick one experimental backend, such as SPIR-V, LLVM, or x86-64, and push it
toward stronger execution proof.

Tradeoff: this may produce a concrete demo, but it strengthens only one output
lane. It does not establish a common substrate contract across CPU, GPU, memory,
semantic, and representation surfaces.

### Approach B: More Accountability Policy Rules

Continue deepening `buildc check` policy profiles and effect provenance.

Tradeoff: this preserves the strongest current differentiator, but it remains
mostly semantic and policy-oriented. It does not bind backend and representation
maturity into the same artifact.

### Approach C: Substrate Receipts v0

Add a new schema and verifier that aggregate existing check receipts, corpus
receipts, backend maturity descriptors, MIR representation coverage, semantic
surfaces, memory surfaces, and command evidence.

Tradeoff: this is less visually impressive than backend promotion, but it gives
the language a durable contract for native substrate work. It lets humans and
machines inspect the same evidence before deciding whether a surface is proven,
experimental, partial, or aspirational.

Recommendation: Approach C.

## Architecture

Substrate Receipts v0 adds a new public artifact family under
`semantic-corpus/receipts/` and a verifier path in `buildc corpus verify` or a
future `buildc substrate verify` command. The first implementation should use a
checked-in sample receipt and tests before adding any generated receipt writer.

The receipt has these top-level surfaces:

- `semantic_surface`: source graph, declared effects, observed capabilities,
  propagated effects, type summary scope, and symbol summary scope.
- `execution_surface`: known backend lanes, target names, output format,
  maturity, evidence class, and execution or validation commands.
- `memory_surface`: ownership, borrow, reference, heap, aggregate, and async
  accountability surfaces observed or explicitly out of scope.
- `representation_surface`: MIR operation families used by the corpus or
  program set, backend coverage posture, unsupported behavior, and fallback
  policy.
- `evidence_surface`: exact commands, expected outcomes, receipt paths, schema
  versions, test names, and maturity labels that justify each claim.

The first slice should not require a full compiler analysis pass for every
program. It can bind existing evidence:

- source/input evidence from `buildlang-check-receipt/v1`;
- C/Rust corpus evidence from semantic corpus execution receipts;
- backend maturity from a checked-in descriptor derived from
  `compiler/src/codegen/backend/STATUS.md`;
- semantic corpus program list from `semantic-corpus/manifest.json`.

## Receipt Shape

The schema should be additive and stable:

```json
{
  "schema": "buildlang-substrate-receipt/v0",
  "receipt_id": "substrate-semantic-corpus-2026-06-18",
  "created_at": "2026-06-18",
  "compiler": "buildc",
  "language": "buildlang",
  "source_set": {
    "kind": "semantic-corpus",
    "manifest": "semantic-corpus/manifest.json",
    "program_count": 8
  },
  "semantic_surface": {
    "check_receipt_schema": "buildlang-check-receipt/v1",
    "requires_source_digest": true,
    "requires_input_graph_digest": true,
    "effect_surfaces": [
      "declared_effects",
      "observed_capabilities",
      "propagated_effects"
    ]
  },
  "execution_surface": {
    "c": {
      "target": "c",
      "maturity": "production-anchor",
      "evidence_class": "native-executable-stdout",
      "receipt": "semantic-corpus/receipts/c-execution-2026-06-13.json"
    },
    "rust": {
      "target": "rust",
      "maturity": "experimental-subset",
      "evidence_class": "generated-artifact-execution",
      "receipt": "semantic-corpus/receipts/rust-execution-2026-06-13.json"
    }
  },
  "memory_surface": {
    "ownership_model": "rust-inspired",
    "verified_surfaces": [
      "references_mutation",
      "tuple_ownership_reuse",
      "struct_aggregate_reuse",
      "field_assignment_reuse",
      "nested_field_reuse",
      "deref_reuse"
    ],
    "known_gaps": [
      "full interprocedural borrow proof",
      "self-hosted stdlib execution",
      "runtime-linked async execution"
    ]
  },
  "representation_surface": {
    "ir": "MIR",
    "fallback_policy": "unsupported or partial targets must not claim production maturity",
    "backend_maturity_descriptor": "compiler/src/codegen/backend/STATUS.md"
  },
  "evidence_surface": {
    "commands": [
      "cargo test --manifest-path compiler/Cargo.toml semantic_corpus_manifest --quiet",
      "cargo test --manifest-path compiler/Cargo.toml semantic_corpus_receipt --quiet",
      "cargo test --manifest-path compiler/Cargo.toml generated_rust_runs --quiet"
    ]
  }
}
```

## Data Flow

1. The semantic corpus manifest names the stable source vectors.
2. Existing C and Rust execution receipts prove executable stdout behavior for
   supported lanes.
3. Existing check receipts prove source graph, effect, capability, and policy
   evidence for individual checked programs.
4. The substrate receipt aggregates those facts and attaches maturity labels.
5. The verifier rejects claims that do not have supporting evidence. Examples:
   a backend marked `production-anchor` without an execution receipt; an
   `input_graph_digest` requirement without a check receipt schema that supports
   it; or an experimental backend with missing unsupported-MIR posture.

## Validation Rules

The first verifier should enforce structural truth rather than deep semantic
analysis:

- `schema` must be `buildlang-substrate-receipt/v0`.
- `source_set.manifest` must exist and parse as
  `buildlang-semantic-corpus/v1`.
- `source_set.program_count` must match the manifest program count.
- Every execution surface must include `target`, `maturity`, `evidence_class`,
  and either a `receipt` path or a clear `status` of `unverified`.
- `production-anchor` requires an execution receipt path and a validator chain
  that includes real executable stdout or equivalent runtime proof.
- `experimental-subset` requires either executable subset evidence or metadata
  evidence plus a known unsupported-MIR posture.
- `representation_surface.fallback_policy` must be present.
- `memory_surface.known_gaps` must be present even when verified memory surfaces
  exist.
- `evidence_surface.commands` must not be empty.

These checks keep the receipt honest. The receipt can say a surface is partial,
unverified, or aspirational, but it cannot silently imply proof.

## CLI Shape

The first implementation can choose the smallest local fit:

Option 1: extend `buildc corpus verify` to validate
`semantic-corpus/receipts/substrate-*.json` after existing manifest and C/Rust
receipt checks.

Option 2: add a focused `buildc substrate verify <receipt.json>` command once
the schema stabilizes.

The recommended first implementation is Option 1 because it reuses the existing
semantic corpus verification entry point and keeps the slice small.

## Error Handling

Verifier diagnostics should name the exact receipt path, JSON pointer or field
name, expected value, and observed value where possible. Examples:

- `execution_surface.c.maturity is production-anchor but receipt is missing`
- `source_set.program_count is 7 but manifest contains 8 programs`
- `execution_surface.spirv claims production-anchor without runtime evidence`

The CLI should exit nonzero on schema, file, maturity, or evidence mismatch.

## Testing

Implementation should be test-first:

- Unit or CLI test: valid substrate receipt passes.
- Unit or CLI test: wrong schema fails.
- Unit or CLI test: manifest program count mismatch fails.
- Unit or CLI test: production backend without receipt fails.
- Unit or CLI test: empty evidence commands fail.
- Unit or CLI test: experimental backend may pass only when maturity is explicit
  and unsupported behavior is documented.
- Docs gate: `cargo fmt --manifest-path compiler/Cargo.toml -- --check`.
- Diff gate: `git diff --check`.

## Non-Goals

- No backend productionization in this slice.
- No full SPIR-V, LLVM, WASM, x86-64, or ARM64 execution proof.
- No replacement for `buildlang-check-receipt/v1`.
- No replacement for semantic corpus C or Rust execution receipts.
- No cryptographic signing layer.
- No package registry trust model.
- No full self-hosted compiler execution claim.
- No runtime sandbox or OS enforcement.

## Acceptance Criteria

This design is implemented when:

- A `buildlang-substrate-receipt/v0` sample receipt exists for the current
  semantic corpus.
- The verifier checks schema, source set, manifest count, backend maturity,
  memory surface gaps, representation fallback policy, and evidence commands.
- At least one valid fixture passes and at least four invalid fixtures fail with
  actionable diagnostics.
- README or STATUS documents Substrate Receipts as an evidence aggregation layer,
  not a backend promotion claim.
- Existing semantic corpus receipt tests still pass.
- Docs-only gates pass before implementation planning is marked complete.

## Future Extensions

Substrate Receipts v0 is a base layer. Later slices can add:

- per-program MIR operation inventories;
- per-backend unsupported-MIR manifests;
- GPU validation receipts using `spirv-val`, HLSL/GLSL compile probes, or Vulkan
  host evidence;
- memory-layout receipts for structs, enums, arrays, and ABI surfaces;
- symbol graph receipts for public APIs and package boundaries;
- latency and responsiveness receipts for watch, LSP, and incremental compile
  surfaces;
- self-hosting readiness receipts for Build-written compiler components.

The important invariant is that the native substrate grows through receipts that
bind claims to evidence, not through broader prose claims.
