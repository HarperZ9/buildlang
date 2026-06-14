# Check Receipt Design

Date: 2026-06-14
Status: Approved for implementation planning

## Purpose

`quantac check` should produce a machine-readable accountability artifact for
ordinary source files. The compiler already enforces capability effects such as
`Console`, `FileSystem`, `Network`, `Process`, `Environment`, `Clock`,
`Foreign`, and `Gpu`; this slice exposes the same evidence as deterministic
JSON so CI, release tooling, reviewers, and downstream products do not need to
scrape human diagnostics.

The receipt is not a signature system yet. It is the stable evidence carrier
that later signing, policy profiles, build attestations, and organization-level
gates can consume.

## Existing Context

- `compiler/src/main.rs` owns the `check` command and already runs lexing,
  parsing, module resolution, and type checking.
- `compiler/src/types/check.rs` compares function body effects against declared
  effects and creates capability-aware diagnostics.
- `compiler/src/types/infer.rs` tracks capability sources as
  `effect -> triggering call or macro names`.
- Semantic-corpus receipts already record capability metadata for curated
  backend execution evidence, but they do not cover arbitrary user source.
- CLI tests in `compiler/tests/cli.rs` already exercise capability diagnostics
  through the built `quantac` binary.

## Command Surface

Extend `quantac check` with:

```text
quantac check <FILE> --receipt <PATH>
quantac check <FILE> --receipt -
```

Behavior:

- Without `--receipt`, keep the current human output unchanged.
- With `--receipt <PATH>`, write the JSON receipt to that path and keep human
  diagnostics on the existing stdout/stderr channels.
- With `--receipt -`, write the JSON receipt to stdout. In that mode, route
  progress and success text to stderr so stdout remains parseable JSON.
- The exit code remains semantic: `0` when parse/type checks pass, `1` when
  they fail or when receipt writing fails.

## Receipt Shape

The first schema is `quantalang-check-receipt/v1`.

Required fields:

```json
{
  "schema": "quantalang-check-receipt/v1",
  "compiler": "quantac",
  "source": "path/to/file.quanta",
  "status": "passed",
  "items": 1,
  "tokens": 12,
  "declared_effects": {
    "main": ["Console"]
  },
  "observed_capabilities": {
    "main": {
      "Console": ["println!"]
    }
  },
  "diagnostics": []
}
```

When checking fails, `status` is `"failed"` and `diagnostics` contains compact
entries with:

- `stage`: `parse` or `type`;
- `kind`: a stable diagnostic class such as `UnhandledEffect`;
- `message`: the existing display string;
- `help`: optional existing help text;
- `notes`: existing diagnostic notes.

The first implementation intentionally omits source hashing and cryptographic
signatures. Those belong in a later attestation slice once the schema is
consumed by at least one CI/tooling path.

## Data Flow

1. `cmd_check` parses the new receipt option.
2. The check pipeline builds a `CheckOutcome` containing token count, item
   count, parse errors, type errors, declared function effects, and observed
   capability sources.
3. Human rendering consumes `CheckOutcome` to preserve the current terminal
   behavior.
4. Receipt rendering serializes `CheckOutcome` into the v1 JSON schema.
5. CLI tests verify both passing and failing receipts through the built binary.

## Type Checker Exposure

The checker should expose check evidence without making CLI code re-walk the
AST after type checking. The first narrow API can be:

- a per-function summary with function name, declared effects, and observed
  capability sources;
- a stable getter on `TypeChecker` for the summaries collected during
  `check_module`.

This keeps accountability data owned by the type/effect layer and keeps
`main.rs` as a renderer/orchestrator.

## Testing Strategy

Implementation must be test-first.

Initial red tests:

- CLI test: a passing file with `fn main() ~ Console { println!("ok"); }` and
  `--receipt -` exits `0`, writes valid JSON to stdout, and records
  `declared_effects.main == ["Console"]` plus
  `observed_capabilities.main.Console == ["println!"]`.
- CLI test: a failing file with `fn main() { read_file("ops.txt"); }` and
  `--receipt <tempfile>` exits `1`, writes a valid receipt, records
  `status == "failed"`, records `FileSystem -> ["read_file"]`, and includes a
  type diagnostic mentioning `UnhandledEffect`.
- Type checker test: function summaries are cleared between modules and do not
  leak evidence across runs.

Verification for the implementation branch:

- `cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml capability --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --quiet`
- `RUSTFLAGS=-Dwarnings cargo test --manifest-path compiler/Cargo.toml --quiet`
- `python -m pytest -q tests/test_docs_landing_page.py` if public docs change

## Non-Goals

- No new language syntax.
- No policy allow/deny engine.
- No source hashing or cryptographic signing in this slice.
- No backend build receipt replacement.
- No change to semantic-corpus receipt schema beyond sharing vocabulary.
- No claim that receipt emission proves runtime sandboxing.

## Acceptance

The slice is acceptable when:

- `quantac check` works exactly as before without `--receipt`;
- `--receipt -` emits parseable JSON to stdout;
- `--receipt <PATH>` writes parseable JSON to the requested path;
- passing receipts include declared and observed capability evidence;
- failing receipts include capability evidence plus diagnostics;
- capability evidence comes from the type/effect checker, not ad hoc CLI string
  scanning;
- focused CLI tests, capability tests, full compiler tests, warning-clean
  compiler tests, diff hygiene, and secret scanning pass.
