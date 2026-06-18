# Module Graph Receipts v0 Design

Date: 2026-06-18
Status: Approved direction; written spec for implementation

## Purpose

QuantaLang now has checked evidence for execution, MIR representation, memory
layout, and source/MIR/effect symbols. The next gap is cross-file identity.
The compiler already lets imports, `include!` files, and resolved `mod` files
affect the input graph digest, but that digest is intentionally opaque. It
proves input bytes changed without preserving the resolver decisions as a
human- and tool-readable graph.

Module Graph Receipts v0 adds a checked semantic-corpus artifact that records
the source input graph behind each corpus program. It should show which entry
file was compiled, which additional source inputs were resolved, what role each
input played, and which deterministic edges tie the entry program to imports,
includes, and modules.

This is an evidence layer, not a full package system. It moves QuantaLang
toward a native shared language surface by making file and module identity
inspectable while leaving public API indexing, full name resolution, LSP
navigation, and package registry semantics for later slices.

## Current Evidence

- `InputDigestLedger` records `entry`, `include`, `import`, and `module`
  inputs with source digests.
- `input_graph_digest` is path-portable and based on source role plus content
  digest, not absolute machine paths.
- `resolve_imports_recording_inputs` records registry package `src/lib.quanta`
  files for bare `use <name>;` and `// import <name>` forms.
- `preprocess_includes_recording_inputs` records files used by source
  `include!("...")` preprocessing.
- `resolve_modules_recording_inputs` records files resolved from `mod foo;`
  declarations, including `foo.quanta`, `foo/mod.quanta`, and stdlib fallback.
- Existing CLI tests already exercise include, registry import, and module
  input digest behavior using temporary fixtures.
- The canonical semantic corpus currently contains eight single-file programs.
  The v0 receipt must describe that truth rather than invent a richer module
  graph.

## Alternatives Considered

### Approach A: Keep Only `input_graph_digest`

Continue relying on the existing digest-only evidence.

Tradeoff: no new artifact and no new verifier. The weakness is that humans,
LSP tooling, and future package tooling cannot inspect what the digest means.
It is too opaque for the native-language surface.

### Approach B: Fold Module Details Into Symbol Graph Receipts

Extend `symbol-graph-2026-06-18.json` with file and module nodes.

Tradeoff: fewer files, but symbol graph receipts should stay focused on
source/MIR/effect identity. Source input topology is a separate contract and
will later feed package graphs, LSP navigation, and incremental compilation.

### Approach C: Add a Dedicated Module Graph Receipt

Create `quantalang-module-graph-receipt/v0`, reference it from the substrate
receipt, and verify it in `quantac corpus verify`.

Tradeoff: one more receipt and verifier path. Benefit: the receipt has one
clear responsibility and can later grow into package and editor graph evidence
without inflating representation or symbol receipts.

Recommendation: Approach C.

## Architecture

The first implementation adds:

- `semantic-corpus/receipts/module-graph-2026-06-18.json`
- a focused `compiler/src/module_graph.rs` receipt builder and verifier
- a `module_surface` block in the substrate receipt
- `quantac corpus verify` integration
- targeted CLI tests for successful verification and receipt drift

The builder should reuse the existing lowering path where practical so module
graph evidence stays aligned with check receipts, MIR representation receipts,
memory layout receipts, and symbol graph receipts. The receipt should be
deterministic: sorted programs, sorted inputs, sorted edges, stable relative
paths, and stable source digests.

The canonical corpus receipt will initially contain only `entry` nodes because
the corpus programs are single-file. Tests must still exercise multi-input
graphs with temporary fixtures or tampered receipts so the verifier proves the
contract before the canonical corpus grows.

## Receipt Shape

The schema is:

```json
{
  "schema": "quantalang-module-graph-receipt/v0",
  "receipt_id": "module-graph-semantic-corpus-2026-06-18",
  "created_at": "2026-06-18",
  "compiler": "quantac",
  "language": "quantalang",
  "source_set": {
    "kind": "semantic-corpus",
    "manifest": "manifest.json",
    "program_count": 8
  },
  "module_model": {
    "resolver": "quantac source input resolver",
    "input_roles": ["entry", "include", "import", "module"],
    "digest_anchor": "quantalang-check-receipt/v1 input_graph_digest",
    "symbol_anchor": "receipts/symbol-graph-2026-06-18.json"
  },
  "programs": [
    {
      "id": "scalar_branch",
      "path": "programs/scalar_branch.quanta",
      "source_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "input_graph_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "module_graph_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "inputs": [
        {
          "id": "input:entry:programs/scalar_branch.quanta",
          "role": "entry",
          "path": "programs/scalar_branch.quanta",
          "source_digest": {
            "algorithm": "sha256",
            "hex": "64 lowercase hex characters"
          }
        }
      ],
      "edges": [
        {
          "kind": "program_entry",
          "from": "program:scalar_branch",
          "to": "input:entry:programs/scalar_branch.quanta"
        }
      ],
      "known_gaps": [
        "full name resolution is not claimed",
        "package public API graph is not claimed"
      ]
    }
  ],
  "summary": {
    "program_count": 8,
    "input_count": 8,
    "input_roles": ["entry"],
    "edge_kinds": ["program_entry"],
    "known_gaps": [
      "cross-package public API index",
      "full LSP navigation readiness",
      "package registry dependency graph"
    ]
  }
}
```

The example digest values are illustrative. The implementation must compute
the actual digests and graph contents from the current semantic corpus.

## Graph Rules

Inputs:

- Every program has one `entry` input matching the manifest path.
- `include` inputs represent files pulled in by `include!("...")`.
- `import` inputs represent registry package `src/lib.quanta` files resolved
  by bare `use <name>;` or `// import <name>`.
- `module` inputs represent files resolved by `mod foo;`.
- Input paths in the canonical semantic corpus receipt must be relative to the
  corpus root. If an input is outside the corpus root in a temporary check
  receipt scenario, the semantic-corpus module graph verifier must reject it
  rather than record an escaping path.

Edges:

- `program_entry`: `program:<id>` to the entry input.
- `program_include`: `program:<id>` to an include input.
- `program_import`: `program:<id>` to an import input.
- `program_module`: `program:<id>` to a module input.

The v0 receipt does not need to record source spans, module nesting depth, or
import names. That can be added after the resolver exposes structured events
instead of only the digest ledger.

## Validation Rules

The verifier should reject a module graph receipt when:

- `schema` is not `quantalang-module-graph-receipt/v0`.
- `compiler` is not `quantac`.
- `language` is not `quantalang`.
- `source_set.kind` is not `semantic-corpus`.
- `source_set.manifest` is not `manifest.json`.
- `source_set.program_count` differs from the manifest.
- Program IDs or paths differ from the manifest.
- Any receipt path is absolute, rooted, drive-prefixed, contains `..`, escapes
  the corpus root, or does not exist.
- Any source digest, input graph digest, or module graph digest differs from
  recomputed evidence.
- Any input list, edge list, program known gaps, module model, or summary value
  differs from recomputed evidence.
- The receipt claims unsupported v0 surfaces such as package public API graph,
  LSP readiness, full name resolution, call graph, or cross-package dependency
  completion.

Diagnostics should name the failing surface, for example:

- `module graph receipt has unsupported schema 'x'`
- `module graph program scalar_branch input_graph_digest mismatch`
- `module graph program scalar_branch inputs drift`
- `module graph program scalar_branch edges drift`
- `module graph summary drift`
- `substrate module_surface.module_receipt path not found: ...`

## Substrate Integration

The substrate receipt should add:

```json
"module_surface": {
  "resolver": "quantac source input resolver",
  "digest_anchor": "quantalang-check-receipt/v1 input_graph_digest",
  "module_receipt": "receipts/module-graph-2026-06-18.json",
  "known_gaps": [
    "cross-package public API index",
    "full LSP navigation readiness",
    "package registry dependency graph"
  ]
}
```

The substrate verifier should require `module_surface.module_receipt` to point
to the canonical module graph receipt under `semantic-corpus/receipts/`.

## Testing

Implementation should be test-first:

- CLI test: valid module graph receipt passes and `quantac corpus verify`
  prints `module graph receipt: ok`.
- CLI test: wrong schema fails with an unsupported schema diagnostic.
- CLI test: program count drift fails.
- CLI test: program path escape fails.
- CLI test: source digest drift fails.
- CLI test: input graph digest drift fails.
- CLI test: input list drift fails.
- CLI test: edge drift fails.
- CLI test: summary drift fails.
- CLI test: substrate rejects a missing or escaping module receipt path.
- Unit test: module graph summaries sort and deduplicate input roles and edge
  kinds.
- Existing corpus, substrate, MIR representation, memory layout, and symbol
  graph slices continue to pass.

Targeted verification commands:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli module_graph -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --bin quantac module_graph --quiet
cargo fmt --manifest-path compiler/Cargo.toml -- --check
git diff --check
```

## Non-Goals

- No full name resolver claim.
- No call graph claim.
- No package public API index.
- No package registry dependency graph.
- No LSP request dispatch or navigation readiness claim.
- No incremental compilation graph.
- No backend productionization.
- No SPIR-V, LLVM, WASM, x86-64, ARM64, or GPU execution claim.
- No cryptographic signing layer.

## Acceptance Criteria

This design is implemented when:

- `semantic-corpus/receipts/module-graph-2026-06-18.json` exists and uses
  `quantalang-module-graph-receipt/v0`.
- The substrate receipt references it through `module_surface.module_receipt`.
- `quantac corpus verify` recomputes and validates the module graph receipt.
- Successful corpus verification prints `module graph receipt: ok`.
- Invalid fixtures cover schema drift, program count drift, path escape,
  digest drift, input drift, edge drift, summary drift, and substrate reference
  drift.
- The receipt honestly describes the current semantic corpus as single-file
  entry graphs until module-bearing programs are intentionally added.
- Narrow implementation test slices and formatting checks pass.

## Future Extensions

Later slices can extend this receipt family into stronger native module proof:

- structured resolver events with import names and module nesting;
- package public API receipts;
- package registry dependency graph receipts;
- LSP definition and references readiness receipts;
- incremental module graph receipts for watch-mode responsiveness;
- cross-backend source map receipts for generated C, Rust, LLVM, WASM, and GPU
  lanes.

The invariant is that module and package claims must be checked evidence, not
unverified prose. The compiler should only claim the graph it can recompute.
