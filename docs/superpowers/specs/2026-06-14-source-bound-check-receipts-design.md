# Source-Bound Check Receipt Design

Date: 2026-06-14
Status: Approved for implementation planning

## Purpose

`quantac check --receipt` now emits deterministic accountability JSON for typed
capability checks. The next hardening slice should bind each receipt to the
exact source bytes that were checked, so CI, release tooling, and downstream
policy consumers can distinguish "this file passed" from "some file at this path
passed."

This is an evidence-binding slice, not a policy engine or signing system. It
makes receipts stronger without changing language semantics or expanding the
capability vocabulary.

## Existing Context

- `compiler/src/main.rs` owns the `check` command, receipt rendering, and file
  reading.
- `CheckReceipt` currently records schema, compiler name, source path, status,
  token count, item count, declared effects, observed capabilities, and compact
  diagnostics.
- `compiler/src/lib.rs` exposes `VERSION` and `LANGUAGE_VERSION`.
- CLI receipt tests already exercise both stdout receipts and file receipts
  through the built `quantac` binary.
- Semantic-corpus receipts have backend execution evidence, but arbitrary source
  checks are the user-facing receipt surface for practical ops workflows.

## Design

Extend `quantalang-check-receipt/v1` with backward-compatible fields:

```json
{
  "schema": "quantalang-check-receipt/v1",
  "compiler": "quantac",
  "compiler_version": "1.0.0",
  "language_version": "1.0.0",
  "source": "path/to/file.quanta",
  "source_digest": {
    "algorithm": "sha256",
    "hex": "..."
  },
  "status": "passed"
}
```

Field rules:

- `compiler_version` uses `quantalang::VERSION`.
- `language_version` renders `quantalang::LANGUAGE_VERSION` as
  `major.minor.patch`.
- `source_digest.algorithm` is exactly `sha256`.
- `source_digest.hex` is lowercase hexadecimal SHA-256 over the exact bytes read
  from the source file before lexing.
- Digest computation is independent of path spelling, path separator style,
  timestamps, current working directory, and receipt output target.
- Passing and failing receipts both include source metadata.

The schema string remains `quantalang-check-receipt/v1` because added fields are
backward-compatible for JSON consumers that ignore unknown members. A future
schema bump should be reserved for removing or retyping existing fields.

## Data Flow

1. `run_check` reads source bytes from disk before constructing `SourceFile`.
2. A small digest helper computes SHA-256 over those bytes and returns lowercase
   hex.
3. `CheckOutcome` carries `compiler_version`, `language_version`, and
   `source_digest`.
4. `build_check_receipt` copies those values into `CheckReceipt`.
5. Human diagnostic behavior stays unchanged.
6. `--receipt -` still keeps stdout as parseable JSON and routes human output to
   stderr.

## Dependency Choice

Use the RustCrypto `sha2` crate for real SHA-256 instead of a placeholder hash
or platform-specific command shell call.

Reasons:

- deterministic across supported platforms;
- pure Rust and already common in Rust supply-chain tooling;
- testable directly inside CLI/unit tests;
- avoids confusing receipt evidence with non-cryptographic hashes already used
  internally for compiler IDs or maps.

## Testing Strategy

Implementation must be test-first.

Initial red tests:

- CLI test: `quantac check fixture.quanta --receipt -` includes
  `compiler_version`, `language_version`, `source_digest.algorithm == "sha256"`,
  and a 64-character lowercase hex digest.
- CLI test: two temporary files with identical contents but different paths
  produce identical `source_digest.hex`.
- CLI test: changing source contents changes `source_digest.hex`.
- CLI test: a failing capability check still writes source metadata to the
  receipt.
- Unit test: the digest helper returns the known SHA-256 for a fixed byte string
  such as `abc`.

Verification for the implementation branch:

- `cargo test --manifest-path compiler/Cargo.toml --test cli check_receipt -- --nocapture`
- `cargo test --manifest-path compiler/Cargo.toml source_digest --quiet`
- `cargo test --manifest-path compiler/Cargo.toml capability --quiet`
- `cargo test --manifest-path compiler/Cargo.toml --quiet`
- `RUSTFLAGS=-Dwarnings cargo test --manifest-path compiler/Cargo.toml --quiet`
- `python -m pytest -q tests/test_docs_landing_page.py` if public docs change
- `git diff --check`
- diff-level secret scan before commit

## Non-Goals

- No policy allow/deny CLI.
- No signature creation or verification.
- No certificate, key, or trust-store management.
- No build artifact digesting.
- No semantic-corpus receipt schema migration.
- No claim that a source digest proves runtime sandboxing.

## Acceptance

The slice is acceptable when:

- every check receipt includes compiler version, language version, and SHA-256
  source digest metadata;
- digest values are deterministic across receipt targets and source paths;
- changed source bytes produce changed digest values;
- failing receipts include the same source binding as passing receipts;
- stdout receipts remain parseable JSON;
- focused CLI tests, digest unit tests, capability tests, full compiler tests,
  warning-clean compiler tests, diff hygiene, and secret scanning pass.
