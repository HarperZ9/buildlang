# Symbol Graph Receipts v0 Design

Date: 2026-06-18
Status: Approved direction; written spec pending user review

## Purpose

QuantaLang now has checked substrate evidence for execution, MIR
representation, and memory/RAM surfaces. The remaining gap in the near-term
native-language thesis is symbols: a shared surface where source names, MIR
names, callable identity, type identity, and effect/capability identity can be
read by humans and verified by machines.

The MIR representation receipt already records per-program function, type,
global, and external arrays. That proves raw MIR symbol inventory, but it does
not connect those names to source-level declarations, source spans, effect
summaries, exported identity, or cross-surface digest evidence. LSP providers
also parse symbols, definitions, and hover text, but those are editor helpers,
not checked corpus evidence.

Symbol Graph Receipts v0 creates a dedicated checked artifact for the semantic
corpus. It records deterministic per-program source symbols, MIR symbols, and
basic edges between declarations, lowered symbols, type definitions, and effect
surfaces. `quantac corpus verify` will recompute the graph and reject stale or
inflated symbol claims.

This slice is intentionally an evidence contract, not a full name resolver,
package graph, IDE index, or semantic database. It makes the current symbol
surface inspectable while leaving deeper resolution and editor readiness for
later receipts.

## Current Evidence

- `semantic-corpus/manifest.json` names the stable eight-program corpus used by
  existing execution and substrate receipts.
- The MIR representation receipt records source, input graph, and MIR digests
  for each corpus program, plus per-program MIR symbols:
  `functions`, `types`, `globals`, and `externals`.
- The memory layout receipt reuses the same source/input/MIR digest chain and
  proves that specialized receipt families can share representation anchors.
- `TypeChecker::function_effect_summaries()` already exposes checked function
  effect summaries with declared effects, observed capabilities, and propagated
  effect sources.
- `ast::Item::name()` exposes source-level item names for functions, structs,
  enums, traits, type aliases, consts, statics, modules, macros, and effects.
- `ast::ItemKind` carries source-level details for fields, variants, impl
  items, trait items, use declarations, extern blocks, and effect declarations.
- LSP symbol, definition, and hover providers exist, but they use line-oriented
  local document parsing and are not yet the authoritative compiler symbol
  graph.
- `quantac corpus verify` already validates the manifest, C/Rust execution
  receipts, substrate receipt, MIR representation receipt, memory layout
  receipt, and real C stdout.

## Alternatives Considered

### Approach A: Extend the MIR Representation Receipt

Add source symbols and effect summaries to
`quantalang-mir-representation-receipt/v0`.

Tradeoff: fewer files and less verifier wiring, but the MIR receipt would become
too broad. MIR representation answers "what was lowered"; symbol graph evidence
answers "what names and semantic identities are shared across source, MIR, and
effects." They should be independently versioned.

### Approach B: Turn LSP Providers Into the Symbol Authority

Use `compiler/src/lsp/symbols.rs`, `definition.rs`, and `hover.rs` as the
receipt source of truth.

Tradeoff: this pulls symbol receipts toward editor behavior before the LSP
server has full request dispatch. The current LSP providers are useful evidence
of intent, but the receipt should use the parser, AST, type checker, and MIR
lowering path that `quantac` already verifies.

### Approach C: Add a Separate Symbol Graph Receipt

Create `quantalang-symbol-graph-receipt/v0`, reference it from the substrate
receipt, and verify it during `quantac corpus verify`.

Tradeoff: one additional artifact and verifier path. Benefit: the symbol graph
contract is focused, can bridge source/MIR/effect evidence, and can later grow
into package indexing, LSP readiness, call graph, import graph, and public API
receipts without inflating representation or memory receipts.

Recommendation: Approach C.

## Architecture

The first implementation adds a checked receipt at
`semantic-corpus/receipts/symbol-graph-2026-06-18.json` and a verifier that
recomputes the same evidence from `semantic-corpus/manifest.json`.

The verifier should be deterministic and structural:

1. Read the semantic corpus manifest.
2. For each manifest program, validate the program path under the corpus root.
3. Read and text-normalize source inputs for stable digest behavior across line
   endings.
4. Parse, import-resolve, include-preprocess, module-resolve, type-check, and
   lower the program to MIR using the same compiler pipeline used by MIR and
   memory receipts.
5. Collect source symbols from the AST.
6. Collect checked effect/capability summaries from the type checker.
7. Collect MIR symbols and module counts from the lowered `MirModule`.
8. Build deterministic source-to-MIR and source-to-effect edges where names can
   be matched without inventing unresolved semantics.
9. Compute a per-program symbol graph digest from the normalized graph.
10. Compare the recomputed receipt with the checked-in receipt.
11. Reject schema drift, path escape, source/input/MIR digest drift,
    program-set drift, symbol drift, edge drift, and summary drift.

The substrate receipt should add a `symbol_surface` block:

```json
"symbol_surface": {
  "source": "AST",
  "representation": "MIR",
  "effect_anchor": "quantalang-check-receipt/v1",
  "symbol_receipt": "receipts/symbol-graph-2026-06-18.json",
  "known_gaps": [
    "full package graph resolution",
    "full LSP request dispatch",
    "cross-package public API index"
  ]
}
```

The substrate verifier should require the receipt path to exist, stay under the
corpus root, and point at the canonical v0 symbol graph receipt.

## Receipt Shape

The schema should be additive and stable:

```json
{
  "schema": "quantalang-symbol-graph-receipt/v0",
  "receipt_id": "symbol-graph-semantic-corpus-2026-06-18",
  "created_at": "2026-06-18",
  "compiler": "quantac",
  "language": "quantalang",
  "source_set": {
    "kind": "semantic-corpus",
    "manifest": "manifest.json",
    "program_count": 8
  },
  "symbol_model": {
    "source": "AST",
    "representation": "MIR",
    "semantic_anchor": "type-checker function effect summaries",
    "lowering_pipeline": "parse -> type-check -> ast-to-mir",
    "representation_anchor": "receipts/mir-representation-2026-06-18.json",
    "memory_anchor": "receipts/memory-layout-2026-06-18.json"
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
      "mir_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "symbol_graph_digest": {
        "algorithm": "sha256",
        "hex": "64 lowercase hex characters"
      },
      "source_symbols": [
        {
          "id": "source:function:choose",
          "kind": "function",
          "name": "choose",
          "visibility": "private",
          "span": {
            "start": 0,
            "end": 0
          },
          "signature": {
            "parameters": 1,
            "has_return": true,
            "is_async": false,
            "is_unsafe": false,
            "declared_effects": []
          }
        }
      ],
      "mir_symbols": {
        "functions": ["choose", "main"],
        "types": ["quanta_vec2", "quanta_vec3", "quanta_vec4"],
        "globals": [],
        "externals": []
      },
      "effect_symbols": [
        {
          "function": "main",
          "declared_effects": ["Console"],
          "observed_capabilities": ["Console"],
          "propagated_effect_sources": []
        }
      ],
      "edges": [
        {
          "kind": "source_to_mir_function",
          "from": "source:function:choose",
          "to": "mir:function:choose"
        },
        {
          "kind": "source_to_effect_summary",
          "from": "source:function:main",
          "to": "effect:function:main"
        }
      ],
      "known_gaps": [
        "call graph is not claimed",
        "external package resolution is not claimed"
      ]
    }
  ],
  "summary": {
    "program_count": 8,
    "source_symbol_kinds": ["function", "struct"],
    "mir_symbol_kinds": ["function", "type"],
    "effect_names": ["Console"],
    "edge_kinds": [
      "source_to_effect_summary",
      "source_to_mir_function",
      "source_to_mir_type"
    ],
    "known_gaps": [
      "full package graph resolution",
      "full LSP request dispatch",
      "cross-package public API index"
    ]
  }
}
```

The example values are illustrative. The implementation must compute actual
digests, symbols, effect arrays, edges, spans, and summaries from the current
semantic corpus.

## Symbol Collection Rules

Arrays must be sorted and deduplicated. Paths must be relative to the semantic
corpus root and must reject absolute paths, rooted paths, Windows drive or UNC
forms, and `..` components.

Source symbols should start with top-level declarations:

- `function`
- `struct`
- `enum`
- `trait`
- `type_alias`
- `const`
- `static`
- `module`
- `macro`
- `effect`

The first implementation should also record nested declaration identity when it
is structurally available without a resolver:

- struct fields;
- enum variants;
- trait methods, associated types, and associated consts;
- impl methods, associated types, and associated consts;
- extern function/static declarations.

Source symbol IDs must be deterministic and local to a program. Use explicit
prefixes such as `source:function:main`, `source:struct:Point`,
`source:struct:Point.field:x`, `source:enum:Result.variant:Ok`,
`source:trait:Display.method:fmt`, and `source:extern:function:puts`.

MIR symbols should initially mirror the MIR representation receipt:

- `mir:function:<name>`
- `mir:type:<name>`
- `mir:global:<name>`
- `mir:external:<name>`

Effect symbols should be derived from checked `FunctionEffectSummary` values:

- declared effect names;
- observed capability names;
- propagated effect source names when present.

## Edge Rules

Edges must only claim relationships the current compiler evidence proves.

The first implementation may claim:

- `source_to_mir_function` when a source function name exactly matches a MIR
  function name.
- `source_to_mir_type` when a source struct, enum, or type alias name exactly
  matches a MIR type name.
- `source_to_mir_external` when an extern source declaration exactly matches a
  MIR external name.
- `source_to_effect_summary` when a source function name exactly matches a
  checked function effect summary.

The first implementation must not claim:

- full call graph;
- overload or trait method resolution beyond exact names;
- import graph completion;
- package-level public API graph;
- LSP readiness;
- cross-file definition navigation;
- global incremental index readiness.

When a source symbol has no MIR or effect edge, that is acceptable. The receipt
should keep the source symbol and omit the unproven edge.

## Data Flow

The symbol graph receipt fits into the existing evidence chain:

1. `semantic-corpus/manifest.json` names stable source programs.
2. The MIR representation receipt proves source/input/MIR digests and raw MIR
   symbol inventory.
3. The memory layout receipt proves memory-oriented interpretation over the
   same digest chain.
4. The substrate receipt aggregates semantic, execution, memory,
   representation, symbol, and evidence posture.
5. The symbol graph receipt proves the detailed source/MIR/effect symbol
   bridge behind `substrate.symbol_surface`.
6. `quantac corpus verify` validates all layers together.

Successful `quantac corpus verify` output should include:

```text
symbol graph receipt: ok
```

## Validation Rules

The verifier should reject a symbol graph receipt when:

- `schema` is not `quantalang-symbol-graph-receipt/v0`.
- `compiler` is not `quantac`.
- `language` is not `quantalang`.
- `source_set.kind` is not `semantic-corpus`.
- `source_set.manifest` does not point to the corpus `manifest.json`.
- `source_set.program_count` does not match the manifest.
- The receipt has missing, duplicate, extra, or out-of-order program IDs.
- Any program path is absolute, escapes the corpus root, does not exist, or
  differs from the manifest path for that program ID.
- Any source digest, input graph digest, MIR digest, or symbol graph digest
  does not match the recomputed value.
- Any source symbol, MIR symbol, effect symbol, edge, known gap, or summary
  value differs from the recomputed value.
- The summary does not match the union of per-program symbol evidence.
- The substrate receipt references a missing or escaping symbol receipt path.
- The receipt claims call graph, LSP readiness, package API graph, or
  cross-package resolution in v0.

Diagnostics should name the exact surface where possible, for example:

- `symbol graph receipt has unsupported schema 'x'`
- `symbol graph program scalar_branch source_digest mismatch`
- `symbol graph program scalar_branch source_symbols drift`
- `symbol graph program scalar_branch edges drift`
- `symbol graph summary.effect_names drift`
- `substrate symbol_surface.symbol_receipt path not found: ...`

## Error Handling

The verifier should follow the current corpus/substrate style: print one
actionable diagnostic to stderr and exit nonzero. Internal helpers may return
`Result<T, String>` for quiet unit tests, with thin CLI wrappers converting
messages to stderr and `Err(1)`.

JSON parsing, source reading, path validation, parse errors, type-check errors,
and MIR lowering errors should identify the program ID or path being processed.

## Testing

Implementation should be test-first:

- CLI test: valid symbol graph receipt passes and `quantac corpus verify`
  prints `symbol graph receipt: ok`.
- CLI test: wrong schema fails with an unsupported schema diagnostic.
- CLI test: program count drift fails.
- CLI test: symbol receipt path escape fails.
- CLI test: source digest drift fails.
- CLI test: source symbol drift fails.
- CLI test: MIR symbol drift fails.
- CLI test: edge drift fails.
- CLI test: v0 rejects call-graph or LSP-readiness overclaiming.
- Unit test: source symbols are sorted and deduplicated.
- Unit test: source-to-MIR edges are emitted only for exact supported matches.
- Unit test: effect names are sorted and deduplicated from function summaries.
- Existing CLI corpus, substrate, MIR representation, and memory layout tests
  continue to pass.
- Formatting gate: `cargo fmt --manifest-path compiler/Cargo.toml -- --check`.
- Diff gate: `git diff --check`.

The implementation plan should keep test slices narrow:

```text
cargo test --manifest-path compiler/Cargo.toml --test cli symbol_graph -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --bin quantac symbol_graph --quiet
cargo test --manifest-path compiler/Cargo.toml --bin quantac mir_representation --quiet
```

Exact test names may differ if the existing test organization makes a different
slice clearer.

## Non-Goals

- No full call graph in v0.
- No package-level public API graph.
- No external registry or package resolution claim.
- No LSP request-dispatch readiness claim.
- No cross-file editor navigation claim.
- No general public symbol dump command.
- No receipt writer in the first implementation.
- No backend productionization.
- No SPIR-V, LLVM, WASM, x86-64, or ARM64 execution claim.
- No self-hosted compiler execution claim.
- No cryptographic signing layer.

## Acceptance Criteria

This design is implemented when:

- `semantic-corpus/receipts/symbol-graph-2026-06-18.json` exists and uses
  `quantalang-symbol-graph-receipt/v0`.
- The substrate receipt references it through `symbol_surface.symbol_receipt`.
- `quantac corpus verify` recomputes and validates the symbol graph receipt.
- Successful corpus verification prints `symbol graph receipt: ok`.
- Invalid fixtures cover schema drift, program count drift, path escape, source
  digest drift, source-symbol drift, MIR-symbol drift, edge drift, and v0
  overclaiming.
- README or STATUS documents the receipt as source/MIR/effect symbol evidence,
  not call graph, LSP readiness, or package API proof.
- Narrow implementation test slices and formatting checks pass.

## Future Extensions

Later slices can extend this receipt family into stronger native symbol proof:

- exact source-to-HIR or source-to-MIR definition IDs once the compiler has a
  stable name-resolution graph;
- call graph receipts;
- import and module graph receipts;
- package public API receipts;
- LSP readiness receipts that exercise request dispatch for document symbols,
  hover, definition, references, diagnostics, and code actions;
- cross-backend symbol maps for C, Rust, LLVM, native assembly, WASM, and GPU
  lanes;
- incremental symbol graph receipts for watch and responsiveness surfaces.

The invariant is that QuantaLang's shared human/machine language should grow
through checked symbol evidence, not through editor helper claims or broad prose
assertions.
