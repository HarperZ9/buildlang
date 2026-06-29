# MIR Representation Receipts v0 Design

Date: 2026-06-18
Status: Approved direction; written spec pending user review

## Purpose

BuildLang already has a checked substrate receipt that says the semantic corpus
uses `MIR` as its representation surface. That is directionally correct, but it
does not prove which MIR operations, statement families, terminators, types,
symbols, or memory-relevant projections are actually produced for the corpus.

MIR Representation Receipts v0 turns that implicit representation claim into a
machine-readable evidence artifact. The receipt records a deterministic
per-program inventory produced from the same parse, type-check, and AST-to-MIR
lowering path used by real compilation. `buildc corpus verify` then recomputes
the inventory and rejects stale or inflated representation claims.

This moves the project toward the long-term native-language thesis by making
semantics, symbols, memory surfaces, and representations visible to humans and
machines through the same evidence layer. It does not promote any experimental
backend and does not claim GPU/native runtime maturity.

## Current Evidence

- `buildc corpus verify` already validates the semantic corpus manifest, C and
  Rust execution receipts, C stdout behavior, and the substrate receipt.
- The substrate receipt already has a `representation_surface` block with
  `ir: "MIR"`, a fallback policy, and a backend maturity descriptor.
- `CodeGenerator::mir()` exposes the lowered `MirModule` after generation.
- `MirLowerer` can lower an AST module to a `MirModule` using the same source
  text path used by code generation.
- `MirModule` contains functions, type definitions, globals, strings,
  externals, vtables, trait method signatures, shader uniforms, locals, blocks,
  statements, rvalues, places, projections, terminators, and type information.
- The semantic corpus manifest currently names eight stable program vectors
  with expected stdout and surface tags.

## Alternatives Considered

### Approach A: Add a `buildc mir dump` Command First

Expose a general MIR dump command and later build receipts on top.

Tradeoff: useful for debugging, but it creates another public CLI surface before
the evidence contract is clear. It also risks optimizing for human text output
instead of deterministic receipt verification.

### Approach B: Extend the Substrate Receipt Directly

Add MIR operation arrays directly inside
`buildlang-substrate-receipt/v0`.

Tradeoff: smaller file count, but it would make the substrate receipt too large
and harder to evolve. Representation inventories should be independently
versioned because they will change more often than backend maturity labels.

### Approach C: Add a Separate MIR Representation Receipt

Create `buildlang-mir-representation-receipt/v0`, reference it from the
substrate receipt, and verify it during `buildc corpus verify`.

Tradeoff: one additional artifact and verifier path. Benefit: the
representation contract is focused, independently testable, and can later grow
into backend unsupported-MIR manifests, symbol graph receipts, layout receipts,
and GPU validation receipts.

Recommendation: Approach C.

## Architecture

The first implementation adds a checked-in representation receipt at
`semantic-corpus/receipts/mir-representation-2026-06-18.json` and a verifier
that recomputes the same inventory from `semantic-corpus/manifest.json`.

The verifier should be deterministic and structural:

1. Read the semantic corpus manifest.
2. For each manifest program, resolve the program path under the corpus root.
3. Read and digest the source file.
4. Parse, type-check, and lower the program to MIR using the existing compiler
   pipeline.
5. Summarize the resulting `MirModule` into a stable receipt DTO.
6. Compare the recomputed receipt with the checked-in receipt.
7. Reject schema drift, path escape, source drift, program-set drift, and MIR
   inventory drift.

The receipt is an inventory, not a pretty-printer. It records counts and sorted
families, not full MIR text. That keeps it stable enough for review while still
catching meaningful representation drift.

## Receipt Shape

The schema should be additive and stable:

```json
{
  "schema": "buildlang-mir-representation-receipt/v0",
  "receipt_id": "mir-representation-semantic-corpus-2026-06-18",
  "created_at": "2026-06-18",
  "compiler": "buildc",
  "language": "buildlang",
  "source_set": {
    "kind": "semantic-corpus",
    "manifest": "manifest.json",
    "program_count": 8
  },
  "ir": {
    "name": "MIR",
    "version": "v0",
    "lowering_pipeline": "parse -> type-check -> ast-to-mir"
  },
  "programs": [
    {
      "id": "scalar_branch",
      "path": "programs/scalar_branch.bld",
      "source_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "module": {
        "function_count": 2,
        "defined_function_count": 2,
        "declaration_count": 0,
        "type_count": 3,
        "global_count": 0,
        "string_count": 0,
        "external_count": 0,
        "vtable_count": 0,
        "uniform_count": 0
      },
      "symbols": {
        "functions": ["main"],
        "types": ["build_vec2", "build_vec3", "build_vec4"],
        "globals": [],
        "externals": []
      },
      "operations": {
        "statements": ["Assign"],
        "rvalues": ["BinaryOp", "Use"],
        "terminators": ["If", "Return"],
        "binary_ops": ["Add", "Gt"],
        "unary_ops": [],
        "casts": [],
        "aggregate_kinds": [],
        "place_projections": []
      },
      "memory_surfaces": {
        "references": false,
        "mutable_references": false,
        "deref_reads": false,
        "deref_writes": false,
        "field_reads": false,
        "field_writes": false,
        "index_reads": false,
        "aggregate_values": false
      },
      "control_flow": {
        "block_count": 3,
        "branching": true,
        "switching": false,
        "calls": true,
        "loops": false,
        "unreachable": false
      }
    }
  ],
  "summary": {
    "program_count": 8,
    "statement_families": ["Assign", "DerefAssign", "FieldAssign"],
    "rvalue_families": ["Aggregate", "BinaryOp", "Deref", "FieldAccess", "Ref", "Use"],
    "terminator_families": ["Call", "Goto", "If", "Return"],
    "memory_surfaces": [
      "aggregate_values",
      "deref_reads",
      "field_reads",
      "field_writes",
      "mutable_references",
      "references"
    ]
  }
}
```

The example values are illustrative. The implementation must generate actual
counts, digests, and operation families from the current semantic corpus.

## Inventory Rules

The receipt should use stable family names derived from MIR enum variants:

- Statement families:
  - `Assign`
  - `DerefAssign`
  - `FieldDerefAssign`
  - `FieldAssign`
  - `StorageLive`
  - `StorageDead`
  - `Nop`
- RValue families:
  - `Use`
  - `BinaryOp`
  - `UnaryOp`
  - `Ref`
  - `AddressOf`
  - `Cast`
  - `Aggregate`
  - `Repeat`
  - `Discriminant`
  - `Len`
  - `NullaryOp`
  - `FieldAccess`
  - `VariantField`
  - `IndexAccess`
  - `Deref`
  - `TextureSample`
- Terminator families:
  - `Goto`
  - `If`
  - `Switch`
  - `Call`
  - `Return`
  - `Unreachable`
  - `Abort`
  - `Drop`
  - `Assert`
- Place projection families:
  - `Deref`
  - `Field`
  - `Index`
  - `ConstantIndex`
  - `Subslice`
  - `Downcast`

Arrays in the receipt must be sorted and deduplicated. Counts must be numeric
and derived from the lowered MIR. Source paths must be relative to the semantic
corpus root and must not contain `..` or absolute path components.

## Data Flow

The representation receipt fits into the existing evidence chain:

1. `semantic-corpus/manifest.json` names stable programs and expected stdout.
2. The C execution receipt proves current production-anchor stdout behavior.
3. The Rust execution receipt proves a subset generated-artifact execution
   lane.
4. The substrate receipt aggregates semantic, execution, memory,
   representation, and evidence posture.
5. The MIR representation receipt proves the concrete representation inventory
   behind `representation_surface.ir = "MIR"`.
6. `buildc corpus verify` validates all five layers together.

The substrate receipt should add:

```json
"representation_receipt": "receipts/mir-representation-2026-06-18.json"
```

under `representation_surface`. The substrate verifier should require that path
to exist and remain under the corpus root.

## Validation Rules

The verifier should reject a MIR representation receipt when:

- `schema` is not `buildlang-mir-representation-receipt/v0`.
- `compiler` is not `buildc`.
- `language` is not `buildlang`.
- `source_set.kind` is not `semantic-corpus`.
- `source_set.manifest` does not point to the corpus `manifest.json`.
- `source_set.program_count` does not match the manifest.
- The receipt has missing, duplicate, extra, or out-of-order program IDs.
- Any program path is absolute, escapes the corpus root, does not exist, or
  differs from the manifest path for that program ID.
- Any source digest does not match the current source bytes.
- Any recomputed module count, symbol array, operation family, memory surface,
  or control-flow value differs from the receipt.
- The receipt summary does not match the union of per-program inventories.
- The substrate receipt references a missing or escaping representation receipt
  path.

Diagnostics should name the exact surface where possible, for example:

- `mir representation receipt has unsupported schema 'x'`
- `mir representation program scalar_branch source_digest mismatch`
- `mir representation program deref_reuse operations.rvalues drift`
- `substrate representation_surface.representation_receipt path not found: ...`

## CLI Shape

The first implementation should extend `buildc corpus verify`. Successful
output should include:

```text
mir representation receipt: ok
```

The implementation should not add a receipt writer in the first slice. The
checked-in receipt is generated once during development and then verified by
tests. A future `--write` extension can refresh representation receipts after
the verifier is stable.

## Error Handling

The verifier should follow the current corpus/substrate style: print one
actionable diagnostic to stderr and exit nonzero. Internal helpers may return
`Result<T, String>` for quiet unit tests, with thin CLI wrappers converting
messages to stderr and `Err(1)`.

JSON parsing, source reading, path validation, parse errors, type-check errors,
and MIR lowering errors should all identify the program ID or path being
processed.

## Testing

Implementation should be test-first:

- CLI test: valid MIR representation receipt passes and `buildc corpus verify`
  prints `mir representation receipt: ok`.
- CLI test: wrong schema fails with an unsupported schema diagnostic.
- CLI test: program count drift fails.
- CLI test: path escape fails.
- CLI test: source digest drift fails.
- CLI test: operation-family drift fails for a changed rvalue, statement, or
  terminator family.
- Unit test: inventory summarization uses sorted, deduplicated family arrays.
- Unit test: memory-surface flags are derived from MIR statement and rvalue
  families, not from manifest prose tags.
- Existing CLI corpus/substrate tests continue to pass.
- Formatting gate: `cargo fmt --manifest-path compiler/Cargo.toml -- --check`.
- Diff gate: `git diff --check`.

The implementation plan should keep test slices narrow:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli mir_representation -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml mir_representation --quiet
```

Exact test names may differ if the existing test file organization makes a
different slice clearer.

## Non-Goals

- No backend productionization.
- No new SPIR-V, LLVM, WASM, x86-64, or ARM64 execution claim.
- No general public `buildc mir dump` command in this slice.
- No receipt writer in the first implementation.
- No full MIR pretty-printer.
- No full memory-layout ABI proof.
- No symbol graph for package boundaries.
- No self-hosted compiler execution claim.
- No cryptographic signing layer.

## Acceptance Criteria

This design is implemented when:

- `semantic-corpus/receipts/mir-representation-2026-06-18.json` exists and uses
  `buildlang-mir-representation-receipt/v0`.
- The receipt records per-program source digests, module counts, symbols,
  operation families, memory-surface flags, control-flow summaries, and corpus
  summary unions.
- `buildc corpus verify` recomputes the representation inventory from the real
  parse, type-check, and MIR lowering pipeline.
- The verifier rejects at least four invalid receipt fixtures or copied-corpus
  mutations with actionable diagnostics.
- The substrate receipt references the representation receipt and validates the
  path.
- README or STATUS explains MIR Representation Receipts as evidence for the
  representation surface, not as backend promotion.
- Focused Rust and CLI test slices pass.
- `cargo fmt --manifest-path compiler/Cargo.toml -- --check` and
  `git diff --check` pass before the implementation is marked complete.

## Future Extensions

Later slices can add:

- a controlled `buildc corpus verify --write-representation` refresh mode;
- backend-specific unsupported-MIR manifests derived from the same inventory;
- symbol graph receipts for public APIs, packages, traits, and module
  boundaries;
- memory-layout receipts for structs, enums, arrays, references, and FFI ABI
  surfaces;
- SPIR-V validation receipts once `spirv-val` or a Vulkan host path is available;
- latency and responsiveness receipts for watch, LSP, and incremental compile
  surfaces.

The invariant is that representation claims must be bound to current compiler
evidence, not broad prose.
