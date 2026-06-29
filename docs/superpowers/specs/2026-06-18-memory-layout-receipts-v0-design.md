# Memory/RAM Layout Receipts v0 Design

Date: 2026-06-18
Status: Approved direction; written spec pending user review

## Purpose

The substrate receipt now states a `memory_surface` with a Rust-inspired
ownership model, verified surfaces, and known gaps. That is useful, but it is
still a static aggregate. It does not prove which semantic corpus programs
actually exercise references, mutable references, dereferences, field writes,
index reads, aggregate values, by-value calls, or ownership reuse in the current
compiler pipeline.

Memory/RAM Layout Receipts v0 turns that memory claim into a checked artifact.
The receipt records deterministic per-program memory evidence from the semantic
corpus and lowered MIR, then `buildc corpus verify` recomputes it and rejects
stale or inflated memory claims.

The name "layout" is deliberately scoped for v0: this receipt proves observed
memory and RAM-shaping surfaces in the compiler representation. It does not
claim a complete byte-offset ABI layout, allocator contract, async runtime
memory model, or full interprocedural borrow proof. Those are later receipt
families once the evidence exists.

## Current Evidence

- `semantic-corpus/manifest.json` names eight stable programs with surface tags
  such as `mutable-reference`, `immutable-reference`, `struct-fields`,
  `fixed-array`, `tuple-aggregate`, `by-value-call`, `ownership-reuse`,
  `field-assignment`, `mutable-struct`, `nested-field-access`, and
  `dereference`.
- The C execution receipt proves current production-anchor stdout behavior for
  the same corpus.
- The Rust execution receipt proves an experimental subset lane that includes
  references, structs, arrays, tuple ownership reuse, field assignment reuse,
  nested field reuse, and dereference reuse.
- The substrate receipt already lists `memory_surface.verified_surfaces`, but
  the verifier only checks that the arrays are non-empty.
- The MIR representation receipt already recomputes per-program MIR digests and
  memory-surface flags such as `references`, `mutable_references`,
  `deref_reads`, `deref_writes`, `field_reads`, `field_writes`, `index_reads`,
  and `aggregate_values`.
- `buildc corpus verify` already validates the manifest, C/Rust execution
  receipts, substrate receipt, and MIR representation receipt.

## Alternatives Considered

### Approach A: Extend the Substrate Receipt Directly

Add computed memory arrays directly under `memory_surface`.

Tradeoff: fewer files, but it makes the substrate receipt do detailed
per-program analysis. The substrate receipt should remain the aggregate
contract, while detailed evidence lives in focused receipts.

### Approach B: Reuse the MIR Representation Receipt

Treat `memory_surfaces` in the MIR representation receipt as sufficient memory
evidence.

Tradeoff: this avoids duplicate computation, but MIR representation and memory
proof answer different questions. The MIR receipt proves representation
inventory. The memory receipt should connect manifest intent, MIR memory
signals, ownership-oriented surfaces, and explicit memory gaps into a dedicated
RAM contract.

### Approach C: Add a Separate Memory/RAM Layout Receipt

Create `buildlang-memory-layout-receipt/v0`, reference it from
`substrate.memory_surface`, and verify it during `buildc corpus verify`.

Tradeoff: one additional artifact and verifier path. Benefit: the memory
contract is independently versioned, can reuse MIR representation digest
helpers, and can later grow toward byte layout, ABI, allocator, borrow proof,
and runtime memory receipts without changing the substrate schema every time.

Recommendation: Approach C.

## Architecture

The first implementation adds a checked-in receipt at
`semantic-corpus/receipts/memory-layout-2026-06-18.json` and a verifier that
recomputes the same evidence from `semantic-corpus/manifest.json`.

The verifier should be deterministic and structural:

1. Read the semantic corpus manifest.
2. For each manifest program, validate the program path under the corpus root.
3. Read and text-normalize the source input for stable digest behavior across
   line endings.
4. Parse, type-check, and lower the program to MIR using the same pipeline used
   by the MIR representation receipt.
5. Collect manifest memory tags and MIR-derived memory surfaces.
6. Classify each program into ownership, reference, aggregate, projection, and
   layout-scope buckets.
7. Compute a stable per-program memory evidence digest.
8. Compare the recomputed receipt with the checked-in receipt.
9. Reject schema drift, path escape, source/input drift, program-set drift,
   memory-surface drift, and summary drift.

The substrate receipt should add a `memory_receipt` field under
`memory_surface`:

```json
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
  ],
  "memory_receipt": "receipts/memory-layout-2026-06-18.json"
}
```

The substrate verifier should require that path to exist, stay under the corpus
root, and point at the canonical v0 memory receipt.

## Receipt Shape

The schema should be additive and explicit about what is proven:

```json
{
  "schema": "buildlang-memory-layout-receipt/v0",
  "receipt_id": "memory-layout-semantic-corpus-2026-06-18",
  "created_at": "2026-06-18",
  "compiler": "buildc",
  "language": "buildlang",
  "source_set": {
    "kind": "semantic-corpus",
    "manifest": "manifest.json",
    "program_count": 8
  },
  "memory_model": {
    "ownership_model": "rust-inspired",
    "scope": "semantic-corpus-mir-memory-surface",
    "layout_claim": "representation-level memory surface, not byte-offset ABI layout",
    "lowering_pipeline": "parse -> type-check -> ast-to-mir",
    "execution_anchor": "receipts/c-execution-2026-06-13.json",
    "representation_anchor": "receipts/mir-representation-2026-06-18.json"
  },
  "programs": [
    {
      "id": "references_mutation",
      "path": "programs/references_mutation.bld",
      "source_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "input_graph_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "mir_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "memory_evidence_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "manifest_surfaces": [
        "immutable-reference",
        "mutable-reference",
        "stdout"
      ],
      "observed_memory_surfaces": {
        "references": true,
        "mutable_references": true,
        "deref_reads": false,
        "deref_writes": false,
        "field_reads": false,
        "field_writes": false,
        "index_reads": false,
        "aggregate_values": false
      },
      "ownership_surfaces": {
        "by_value_call": false,
        "ownership_reuse": false,
        "mutable_struct": false,
        "reference_mutation": true
      },
      "layout_surfaces": {
        "struct_fields": false,
        "tuple_aggregate": false,
        "fixed_array": false,
        "nested_field_access": false,
        "dereference": false,
        "field_assignment": false
      },
      "proof_status": {
        "representation_level": "verified",
        "execution_level": "c-stdout-verified",
        "byte_layout": "not-claimed",
        "full_borrow_proof": "not-claimed"
      }
    }
  ],
  "summary": {
    "program_count": 8,
    "manifest_memory_surfaces": [
      "by-value-call",
      "dereference",
      "field-assignment",
      "fixed-array",
      "immutable-reference",
      "mutable-reference",
      "mutable-struct",
      "nested-field-access",
      "ownership-reuse",
      "struct-fields",
      "tuple-aggregate"
    ],
    "observed_memory_surfaces": [
      "aggregate_values",
      "deref_reads",
      "field_reads",
      "field_writes",
      "mutable_references",
      "references"
    ],
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
  }
}
```

The example values are illustrative. The implementation must generate real
digests and arrays from the current semantic corpus.

## Classification Rules

Arrays must be sorted and deduplicated. Paths must be relative to the semantic
corpus root and must reject absolute paths, rooted paths, Windows drive or UNC
forms, and `..` components.

Manifest surface tags map into memory evidence as follows:

- `mutable-reference` and `immutable-reference` set reference-oriented manifest
  surfaces.
- `by-value-call` and `ownership-reuse` set ownership transfer and reuse
  surfaces.
- `struct-fields`, `tuple-aggregate`, `fixed-array`, `nested-field-access`,
  `field-assignment`, `mutable-struct`, and `dereference` set layout-scope
  surfaces.

MIR-derived flags must come from lowered MIR, not manifest prose. The initial
surface names should match the MIR representation receipt:

- `references`
- `mutable_references`
- `deref_reads`
- `deref_writes`
- `field_reads`
- `field_writes`
- `index_reads`
- `aggregate_values`

`memory_evidence_digest` should be computed from a stable JSON-like normalized
projection of the program's manifest surfaces, MIR memory surfaces, ownership
surfaces, layout surfaces, proof status, source digest, input graph digest, and
MIR digest. It should not include filesystem absolute paths.

## Data Flow

The memory receipt fits into the existing evidence chain:

1. `semantic-corpus/manifest.json` names the memory and ownership vectors.
2. C and Rust execution receipts prove stdout behavior for current execution
   lanes.
3. The MIR representation receipt proves source-bound MIR and memory flag
   inventory.
4. The substrate receipt aggregates semantic, execution, memory,
   representation, and evidence posture.
5. The memory receipt proves the detailed RAM/memory evidence behind
   `substrate.memory_surface`.
6. `buildc corpus verify` validates all layers together.

Successful `buildc corpus verify` output should include:

```text
memory layout receipt: ok
```

## Validation Rules

The verifier should reject a memory layout receipt when:

- `schema` is not `buildlang-memory-layout-receipt/v0`.
- `compiler` is not `buildc`.
- `language` is not `buildlang`.
- `source_set.kind` is not `semantic-corpus`.
- `source_set.manifest` does not point to the corpus `manifest.json`.
- `source_set.program_count` does not match the manifest.
- The receipt has missing, duplicate, extra, or out-of-order program IDs.
- Any program path is absolute, escapes the corpus root, does not exist, or
  differs from the manifest path for that program ID.
- Any source digest, input graph digest, MIR digest, or memory evidence digest
  does not match the recomputed value.
- Any manifest surface, observed memory surface, ownership surface,
  layout-surface, or proof-status value differs from the recomputed value.
- The summary does not match the union of per-program evidence.
- The substrate receipt references a missing or escaping memory receipt path.
- The receipt claims byte-level ABI layout or full borrow proof in v0.

Diagnostics should name the exact surface where possible, for example:

- `memory layout receipt has unsupported schema 'x'`
- `memory layout program references_mutation source_digest mismatch`
- `memory layout program deref_reuse observed_memory_surfaces drift`
- `memory layout summary.known_gaps drift`
- `substrate memory_surface.memory_receipt path not found: ...`

## Error Handling

The verifier should follow the current corpus/substrate style: print one
actionable diagnostic to stderr and exit nonzero. Internal helpers may return
`Result<T, String>` for quiet unit tests, with thin CLI wrappers converting
messages to stderr and `Err(1)`.

JSON parsing, source reading, path validation, parse errors, type-check errors,
and MIR lowering errors should identify the program ID or path being processed.

## Testing

Implementation should be test-first:

- CLI test: valid memory layout receipt passes and `buildc corpus verify`
  prints `memory layout receipt: ok`.
- CLI test: wrong schema fails with an unsupported schema diagnostic.
- CLI test: program count drift fails.
- CLI test: memory receipt path escape fails.
- CLI test: source digest drift fails.
- CLI test: observed memory surface drift fails.
- CLI test: summary known-gap drift fails.
- CLI test: v0 rejects byte-level ABI or full-borrow-proof overclaiming.
- Unit test: manifest memory surfaces are sorted and deduplicated.
- Unit test: MIR memory surfaces are derived from MIR, not manifest tags.
- Existing CLI corpus, substrate, and MIR representation tests continue to
  pass.
- Formatting gate: `cargo fmt --manifest-path compiler/Cargo.toml -- --check`.
- Diff gate: `git diff --check`.

The implementation plan should keep test slices narrow:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli memory_layout -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml memory_layout --quiet
cargo test --manifest-path compiler/Cargo.toml mir_representation --quiet
```

Exact test names may differ if the existing test organization makes a
different slice clearer.

## Non-Goals

- No byte-offset struct, enum, tuple, array, or ABI layout proof in v0.
- No allocator or heap ownership runtime.
- No async runtime memory execution proof.
- No full interprocedural borrow proof.
- No backend productionization.
- No SPIR-V, LLVM, WASM, x86-64, or ARM64 execution claim.
- No general public memory dump command.
- No receipt writer in the first implementation.
- No self-hosted compiler execution claim.
- No cryptographic signing layer.

## Acceptance Criteria

This design is implemented when:

- `semantic-corpus/receipts/memory-layout-2026-06-18.json` exists and uses
  `buildlang-memory-layout-receipt/v0`.
- The substrate receipt references it through
  `memory_surface.memory_receipt`.
- `buildc corpus verify` recomputes and validates the memory layout receipt.
- Successful corpus verification prints `memory layout receipt: ok`.
- Invalid fixtures cover schema drift, program count drift, path escape, source
  digest drift, observed-memory-surface drift, summary known-gap drift, and v0
  overclaiming.
- README or STATUS documents the receipt as a memory/RAM evidence layer, not a
  byte-level ABI or full borrow-proof claim.
- Narrow implementation test slices and formatting checks pass.

## Future Extensions

Later slices can extend this receipt family into stronger native memory proof:

- byte-offset ABI layout for structs, enums, tuples, arrays, and references;
- backend-specific layout comparison between MIR, C, Rust, LLVM, and native
  assembly lanes;
- allocator, heap, stack, and lifetime region receipts;
- interprocedural borrow and aliasing proof receipts;
- async runtime memory receipts once async execution is linked;
- GPU buffer and address-space layout receipts for SPIR-V/HLSL/GLSL;
- responsiveness receipts for memory behavior in watch, LSP, and incremental
  compilation paths.

The invariant is that BuildLang's native memory thesis should grow through
checked, scoped evidence instead of broad prose claims.
