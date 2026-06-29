# Input-Bound Check Receipts Design

Date: 2026-06-14
Status: Approved by standing continue direction

## Purpose

`buildc check --receipt` currently records a SHA-256 digest for the entry
source file. That is useful, but the checker also resolves registry imports,
`include!("...")` directives, and `mod foo;` files before type checking. A
receipt can therefore remain unchanged when a transitive checked input changes.

This slice binds check receipts to the full source input set used by the check
pipeline. The existing `source_digest` stays as the entry-file digest for
backward compatibility, and a new `input_digests` ledger records every source
file whose bytes feed the check.

## Receipt Shape

Add an `input_digests` array to `buildlang-check-receipt/v1`:

```json
{
  "input_digests": [
    {
      "role": "entry",
      "source": "app.bld",
      "digest": {
        "algorithm": "sha256",
        "hex": "..."
      }
    },
    {
      "role": "include",
      "source": "shared.bld",
      "digest": {
        "algorithm": "sha256",
        "hex": "..."
      }
    }
  ]
}
```

Rules:

- `source_digest` remains the SHA-256 digest of the entry file bytes.
- `input_digests` includes the entry file and every file read by import,
  include, or module resolution during `buildc check`.
- Each digest is over exact file bytes before UTF-8 conversion or parsing.
- Duplicate canonical paths are recorded once, using the first role observed.
- Output is deterministic: sorted by role and source path before serialization.
- If an input file cannot be read as UTF-8, the command fails as it does today.

## Data Flow

1. `run_check` creates a per-check `InputDigestLedger`.
2. Reading the entry file records role `entry`.
3. `resolve_imports`, `preprocess_includes`, and `resolve_modules` receive the
   ledger and record file bytes before transforming source.
4. `CheckOutcome` carries the ledger records.
5. `build_check_receipt` serializes the sorted records as `input_digests`.

## Non-Goals

- No signing or trust-store management.
- No build artifact digesting.
- No schema version bump; this is an additive field.
- No semantic change to include/import/module resolution.

## Acceptance

- A check receipt for an included source records both entry and included file.
- Changing the included file changes that file's recorded digest without
  changing the entry `source_digest`.
- Existing source digest tests still pass.
- Policy receipt behavior is unchanged except for the stronger input ledger.
- Focused receipt tests, capability/policy regressions, full compiler tests,
  warning-clean tests, docs checks, diff hygiene, and secret scanning pass.
