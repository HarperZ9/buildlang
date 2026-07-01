# Design: Linear-on-MIR 2a — Groundwork (linearity annotations + span side-table)

Status: draft for execution (2026-07-01), branch `feat/linear-mir-checker`. First of four
specs (2a-2d) for the MIR affine/borrow checker that closes the five open `#[linear]`
escape classes and makes linear checking sound AND precise. Governed by
`docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-30.md`. Builds on the `codegen::analysis`
substrate shipped in the increment-4 brick.

## Summary

Thread the two facts a MIR-phase linear checker needs but MIR does not currently carry:
(1) **which locals are `#[linear]`**, via the existing `MirLocal.annotations` channel, and
(2) **source spans** for the statements/terminators the checker will point diagnostics at,
via an in-memory span side-table on `MirFunction`. This sub-brick ships ONLY the
groundwork with golden tests; no checker yet (that is 2b).

## Motivation

The linear checker (2b) runs post-lowering on MIR. Two gaps block it:
- **Linearity is invisible at MIR.** `MirLocal.ty` is `MirType::Struct(name)` where `name`
  is module-prefixed / monomorphization-mangled (`lower/mod.rs:740-757, 1110-1121`), while
  `TypeContext` stores bare source names. Mapping back is fragile. But `#[linear]` is fully
  known at lowering time (`mark_linear` runs during type-check collection,
  `check.rs:417-421, 478-481`; `is_linear_def(def_id)` at `context.rs:385-387`).
- **Spans are absent from MIR.** `MirStmt` is `{ kind }` only (`ir.rs:561-565`); terminators
  and locals carry none. But every AST node the lowerer processes has `.span`
  (`ast/expr.rs:18-22`, etc.), and `Span { start, end, source_id }` is `Copy`
  (`lexer/span.rs:137-145`). The data is in hand at lowering; it is simply dropped.

There is a proven precedent for bridging type-system facts to MIR: `MirLocal.annotations:
Vec<Arc<str>>` (`ir.rs:501-503`) already carries `"ColorSpace:Linear"` / `"Precision:Half"`
tags via `extract_type_annotations` (`lower/types.rs:133-153`) and `set_param_annotations`
(`lower/mod.rs:1668-1677`, `builder.rs:158-163`) — but only for parameters.

## Architecture

Two additive changes at lowering; nothing consumes them yet.

1. **Linearity annotation.** Stamp the tag `"linear"` onto `MirLocal.annotations` for every
   local (parameter, `let`-binding, and compiler temp) whose resolved type is a `#[linear]`
   ADT. Computed at lowering where `ctx: &TypeContext` is live (`lower/mod.rs:31-33`) and the
   un-mangled type is known, using the existing `is_linear_def` / `linear_def_of` predicates.
   After this, "is this MIR local linear?" is a pure function of MIR: check for the `"linear"`
   annotation. No `TypeContext` and no name-unmangling needed by the 2b checker.

2. **Span side-table.** Add to `MirFunction` an in-memory-only span map:
   ```rust
   /// Source spans for statements/terminators, keyed by (block position, index).
   /// In-memory only: populated at lowering, consumed by the MIR linear checker
   /// immediately after, and NOT serialized (the linear check runs before any MIR
   /// serialization, so the serializable-MIR contract is untouched).
   #[serde(skip, default)]
   pub spans: MirSpanTable,
   ```
   where `MirSpanTable` holds `stmt: HashMap<(u32 /*block id*/, usize /*stmt idx*/), Span>`
   and `terminator: HashMap<u32 /*block id*/, Span>`. Populated in the lowering paths that
   emit statements/terminators (they already receive the span-carrying AST node). `Span`
   gains `Serialize`/`Deserialize` derives ONLY if needed to satisfy `MirSpanTable`'s derives;
   because the field is `#[serde(skip)]`, the simplest path is for `MirSpanTable` itself to
   not require `Span: Serialize` (store spans but skip the whole table). Prefer: derive
   `Default` on `MirSpanTable`, skip it in `MirFunction`'s serde, and do not add serde to
   `Span` at all.

## Data flow

`lower_module` / `lower_fn` build each `MirFunction`. At each local creation, if
`linear_def_of(ty).is_some()`, push `"linear"` into that local's `annotations`. At each
statement/terminator emission, record the source `Span` into the function's `spans` table
keyed by the block id + statement index (or block id for the terminator). Both are pure
additions; existing behavior is unchanged.

## Error handling

None (no user-facing behavior). If a local's type cannot be resolved to an ADT (generic
`Param`, infer var), it is simply not tagged `"linear"` — the 2b checker treats an untagged
local as non-linear, which for THIS sub-brick is fine because 2b handles generics via
monomorphized MIR (the `Param` is already substituted by the time a concrete linear local
exists). Spans that are unavailable for a synthesized statement are simply absent from the
table; 2b falls back to the enclosing function's span for such a site (rare).

## Testing

- **Unit (annotations):** lower a small program with a `#[linear]` struct used in a `let`,
  a temp (e.g. a function-call result), and a parameter; assert every corresponding
  `MirLocal.annotations` contains `"linear"`, and that non-linear locals do not.
- **Unit (spans):** lower a program; assert `func.spans.stmt` / `func.spans.terminator`
  contain entries for known statements/terminators whose `Span` matches the source text
  range (compare against `source[span.start..span.end]`).
- **Regression:** full `cargo test` green; `RUSTFLAGS=-Dwarnings cargo build` clean;
  `buildc corpus verify` 8/8 (annotations/spans are additive, must not change codegen).
- **Serialization unchanged:** if any MIR round-trip/golden test exists, confirm the
  `#[serde(skip)]` span table leaves serialized MIR byte-identical.

## File touch-points (verify at implementation time)

- `compiler/src/codegen/ir.rs`: add `spans: MirSpanTable` (`#[serde(skip, default)]`) to
  `MirFunction`; define `MirSpanTable`.
- `compiler/src/codegen/lower/mod.rs`, `lower/types.rs`, `lower/expr.rs`, `lower/stmt.rs`:
  stamp `"linear"` annotations on locals; record spans on statement/terminator emission.
- `compiler/src/codegen/builder.rs`: any builder helper that creates locals/statements may
  need a span parameter or a post-hoc setter (prefer a setter to keep call sites small).
- Tests: `compiler/src/codegen/lower/` unit tests (or a new `lower/tests.rs`).

## Risks

- **Builder-API churn.** Recording spans at every emission may touch many `builder` call
  sites. Mitigation: give `MirBuilder` a `current_span` cursor set by `lower_expr`/`lower_stmt`
  before emitting, so emission helpers read it without a new parameter everywhere.
- **Temp locals without a clean type.** Some temps are synthesized; tag conservatively (only
  when the type resolves to a linear def). Under-tagging a temp is safe for 2b (it just
  won't be checked as linear at that temp; the owning binding still is).

## Dependency

None beyond the shipped `codegen::analysis` substrate. 2b/2c/2d depend on this.
