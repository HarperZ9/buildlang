# Check Policy Profiles Design

Date: 2026-06-14
Status: Design approved; pending implementation planning

## Purpose

`quantac check --receipt` now produces source-bound accountability evidence:
compiler version, language version, source digest, declared effects, observed
capability sources, and diagnostics. The next step is a portable policy profile
that lets CI and review tooling turn that evidence into a deterministic
pass/fail decision.

This slice makes QuantaLang more useful as an ops accountability language: code
does not merely reveal operational capabilities; teams can enforce which effects
are acceptable for a given lane.

## Existing Context

- `compiler/src/main.rs` owns `quantac check`, source digesting, receipt
  rendering, and CLI exit behavior.
- `CheckReceipt` already records source-bound check evidence under
  `quantalang-check-receipt/v1`.
- `FunctionEffectSummary` exposes declared effects and observed capability
  sources from the type/effect checker.
- CLI tests in `compiler/tests/cli.rs` already exercise passing/failing
  receipts through the built `quantac` binary.
- Public docs describe capability effects and source-bound receipts.

## Command Surface

Extend `quantac check` with an optional policy file:

```text
quantac check <FILE> --policy <POLICY.json>
quantac check <FILE> --policy <POLICY.json> --receipt <PATH>
quantac check <FILE> --policy <POLICY.json> --receipt -
```

Behavior:

- Without `--policy`, current check and receipt behavior stays unchanged.
- With `--policy`, `quantac` reads and validates the policy profile, runs the
  normal check pipeline, evaluates the policy against the check outcome, and
  exits nonzero if type/parse diagnostics or policy violations exist.
- If `--receipt` is also present, the receipt includes the policy evidence and
  decision.
- If policy file reading or JSON/schema validation fails, `quantac` exits
  nonzero with a clear configuration error. No receipt is required for invalid
  policy configuration in this first slice.

## Policy Schema

The first schema is `quantalang-check-policy/v1`:

```json
{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["Console"],
  "denied_effects": ["FileSystem", "Network", "Process", "Foreign"],
  "require_source_digest": true
}
```

Field rules:

- `schema` is required and must be exactly `quantalang-check-policy/v1`.
- `allowed_effects` is optional. When present and non-empty, every declared
  effect and observed capability effect must be listed.
- `denied_effects` is optional. Any declared effect or observed capability
  effect listed here is a violation.
- `denied_effects` wins over `allowed_effects`.
- `require_source_digest` is optional and defaults to `false`. When true, the
  receipt source digest must be present and use `sha256`.
- Unknown policy fields are tolerated for forward compatibility.

The first implementation evaluates effect names surfaced by the existing check
outcome: declared function effects and observed capability effect names. This
means both operational capability declarations and user-defined effect
declarations can be governed by policy. Future schemas can split capability-only
policy from general effect policy if real users need that distinction.

## Receipt Extension

When policy evaluation runs, extend `quantalang-check-receipt/v1` with an
optional `policy` object:

```json
{
  "policy": {
    "schema": "quantalang-check-policy/v1",
    "source": "policy/console-only.json",
    "source_digest": {
      "algorithm": "sha256",
      "hex": "..."
    },
    "status": "failed",
    "violations": [
      {
        "kind": "DeniedEffect",
        "effect": "FileSystem",
        "function": "main",
        "surface": "observed_capabilities",
        "message": "policy denies effect `FileSystem`"
      }
    ]
  }
}
```

Receipt rules:

- `policy.source_digest` is SHA-256 over the exact policy file bytes.
- `policy.status` is `passed` when no violations exist and `failed` otherwise.
- Top-level receipt `status` is `failed` if parse/type diagnostics exist or
  policy status is `failed`.
- `violations` is deterministic: sorted by function, effect, surface, and kind.
- `surface` is `declared_effects`, `observed_capabilities`, or
  `source_digest`.

## Policy Evaluation

Build an effect evidence set from each `FunctionEffectSummary`:

- declared effects come from `summary.declared_effects`;
- observed capabilities come from `summary.observed_capabilities.keys()`;
- each evidence item records the function name and source surface.

Then apply rules:

1. If `require_source_digest` is true and the current check outcome lacks a
   `sha256` source digest, emit `MissingSourceDigest`.
2. For every evidence item whose effect appears in `denied_effects`, emit
   `DeniedEffect`.
3. If `allowed_effects` is present and non-empty, emit `DisallowedEffect` for
   every evidence item whose effect is absent from the allow-list.
4. Sort and deduplicate violations before receipt rendering.

Policy evaluation should still run when type checking fails, because a failing
source file can still provide useful effect evidence and receipt context.

## Data Flow

1. `cmd_check` accepts a new `policy: Option<PathBuf>`.
2. If a policy path is provided, `load_check_policy` reads the policy bytes,
   computes the policy SHA-256 digest, parses JSON, and validates the schema.
3. `run_check` produces the existing source-bound `CheckOutcome`.
4. `evaluate_check_policy` compares the policy against `CheckOutcome`.
5. `build_check_receipt` includes policy receipt data when available.
6. `cmd_check` exits with failure if parse errors, type errors, or policy
   violations exist.

## Error Handling

- Missing policy file: print `Error reading policy '<path>': <error>` and exit
  `1`.
- Invalid JSON: print `Error parsing policy '<path>': <error>` and exit `1`.
- Unsupported schema: print `Unsupported check policy schema '<schema>'` and
  exit `1`.
- Policy violation: print a compact human diagnostic after type diagnostics,
  for example `Policy violation: policy denies effect FileSystem in main`.

When `--receipt -` is used, human policy diagnostics go to stderr so stdout
remains parseable JSON.

## Testing Strategy

Implementation must be test-first.

Initial red tests:

- CLI: `--policy console-only.json --receipt -` passes for
  `fn main() ~ Console { println!("ok"); }` and records `policy.status ==
  "passed"` plus a 64-character policy digest.
- CLI: a policy with `denied_effects: ["FileSystem"]` fails a source file that
  declares and uses `FileSystem`, even though type checking passes.
- CLI: a policy with `allowed_effects: ["Console"]` fails a source file that
  declares and uses `FileSystem`.
- CLI: a policy violation receipt has top-level `status == "failed"` and a
  deterministic violation with kind, effect, function, surface, and message.
- CLI: invalid policy schema exits nonzero and reports the unsupported schema.
- Unit test: policy evaluation deduplicates declared/observed duplicates and
  sorts violations deterministically.

Verification for the implementation branch:

- `cargo test --manifest-path compiler/Cargo.toml --test cli check_policy -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml check_policy --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml capability --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --quiet`
- `RUSTFLAGS=-Dwarnings cargo test --manifest-path compiler/Cargo.toml --quiet`
- `python -m pytest -q tests/test_docs_landing_page.py` if public docs change
- `git diff --check`
- diff-level secret scan before commit

## Non-Goals

- No signing or signature verification.
- No certificate, key, or trust-store management.
- No network policy fetching.
- No package-level policy discovery.
- No backend-specific runtime sandboxing.
- No policy language beyond the v1 JSON fields above.
- No semantic-corpus receipt schema migration in this slice.

## Acceptance

The slice is acceptable when:

- `quantac check` behavior is unchanged without `--policy`;
- valid policy profiles can allow, deny, or restrict effects deterministically;
- policy failures make `quantac check` exit nonzero even when type checking
  passes;
- policy receipts record policy schema, path, SHA-256 digest, status, and
  structured violations;
- `--receipt -` remains parseable JSON on stdout;
- invalid policies fail with clear configuration diagnostics;
- focused policy tests, receipt regression tests, capability tests, full
  compiler tests, warning-clean compiler tests, diff hygiene, and secret
  scanning pass.
