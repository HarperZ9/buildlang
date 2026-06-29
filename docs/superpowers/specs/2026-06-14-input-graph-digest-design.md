# Input Graph Digest Design

Date: 2026-06-14
Status: Approved by standing continue direction

## Purpose

`buildc check --receipt` now records an `input_digests` ledger for every entry,
import, include, and module file that feeds the check pipeline. That ledger is
inspectable, but policy tooling still has to walk an array to compare source
graphs. Add one portable graph fingerprint so CI can compare the entire checked
input set with a single value.

## Design

Add `input_graph_digest` to `buildlang-check-receipt/v1`:

```json
{
  "input_graph_digest": {
    "algorithm": "sha256",
    "hex": "..."
  }
}
```

Rules:

- `source_digest` remains the entry-file byte digest.
- `input_digests` remains the detailed ledger with roles, source paths, and
  per-file byte digests.
- `input_graph_digest` is a SHA-256 digest over sorted tuples of
  `role`, digest algorithm, and digest hex from `input_digests`.
- Absolute source paths are intentionally excluded from the graph digest so the
  same source graph produces the same fingerprint in different checkout roots.
- Duplicate inputs with the same role and content still affect the graph digest
  because their tuples appear once per ledger record.
- Changing any checked input changes the graph digest.

## Testing

- A receipt with `include!("shared.bld")` keeps the same entry
  `source_digest` when only the included file changes, but changes
  `input_graph_digest`.
- Two equivalent source graphs in different temp directories produce identical
  `input_graph_digest` values even though detailed source paths differ.
- Existing receipt, policy, capability, full compiler, warning-clean, docs, and
  hygiene gates remain green.

## Non-Goals

- No signing or trust-store behavior.
- No schema version bump; this is an additive receipt field.
- No policy-schema change in this slice.
- No change to import/include/module resolution behavior.
