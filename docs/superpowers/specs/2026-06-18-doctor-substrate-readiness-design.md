# Doctor Substrate Readiness Design

Date: 2026-06-18
Status: Draft design pending user review

## Purpose

QuantaLang now has a checked-in `quantalang-substrate-receipt/v0` artifact and
`quantac corpus verify` validates it as a hard evidence gate. That is useful for
CI and release checks, but the evidence is still hidden from the command users
run when they want to understand local readiness: `quantac doctor`.

Doctor Substrate Readiness makes the substrate receipt visible at the adoption
boundary. The goal is to let humans and tooling see, in one diagnostic surface,
which native substrate lanes are proven, partial, experimental, or unverified.
This keeps forward motion toward a shared human/machine language surface without
inflating backend maturity claims.

## Current Evidence

- `semantic-corpus/receipts/substrate-semantic-corpus-2026-06-18.json` records
  semantic, execution, memory, representation, and command evidence for the
  current semantic corpus.
- `quantac corpus verify` reads the receipt, validates its schema, checks the
  corpus program count, verifies backend maturity constraints, and rejects
  missing evidence for production claims.
- `quantac doctor` already reports local toolchain readiness, optional backend
  tools, and backend maturity labels.
- `quantac doctor` does not yet report whether the substrate receipt exists,
  whether it verifies, or what evidence posture it contains.

## Alternatives Considered

### Approach A: Add a New `quantac substrate status` Command

This would give substrate evidence its own command and keep `doctor` compact.

Tradeoff: it creates another entry point before the substrate surface is broad
enough to justify one. Users checking local readiness would still need to know
that the extra command exists.

### Approach B: Promote Substrate Verification Failures Into Doctor Exit Codes

This would make `doctor` fail when the substrate receipt is missing or invalid.

Tradeoff: `doctor` is currently diagnostic-only and succeeds even when optional
tools are absent. Turning substrate drift into a doctor failure would make
`doctor` behave like a release gate, duplicating `quantac corpus verify`.

### Approach C: Add a Diagnostic `Substrate evidence:` Section to Doctor

This reuses the existing substrate receipt and verifier, prints compact evidence
status in `quantac doctor`, and keeps hard failure semantics in
`quantac corpus verify`.

Tradeoff: it adds more doctor output, but it puts the project thesis in the
place users already inspect for readiness.

Recommendation: Approach C.

## User-Facing Output

When the receipt exists and verifies, `quantac doctor` should print a section
like:

```text
Substrate evidence:
  receipt   ok       quantalang-substrate-receipt/v0
  corpus    ok       8 semantic program(s)
  c         anchor   production execution evidence
  rust      subset   experimental executable subset
  spirv     unverified explicit unsupported-MIR posture
  memory    partial  6 verified surface(s), 3 known gap(s)
  repr      MIR      fallback policy recorded
```

The exact spacing can follow the current doctor style. The field names should be
stable enough for smoke tests and simple log parsers.

If the receipt is missing, doctor should still exit successfully and print:

```text
Substrate evidence:
  receipt   missing  run quantac corpus verify from a repository checkout
```

If the receipt exists but fails verification, doctor should still exit
successfully and print:

```text
Substrate evidence:
  receipt   invalid  run quantac corpus verify for details
```

The detailed error remains owned by `quantac corpus verify`.

## Architecture

Add a small diagnostic helper near the existing corpus verification helpers in
`compiler/src/main.rs`.

The helper should:

1. Locate the semantic corpus root using the existing
   `find_semantic_corpus_root()` helper.
2. Read `manifest.json` as `SemanticCorpusManifest`.
3. Read `receipts/substrate-semantic-corpus-2026-06-18.json` as
   `SubstrateReceipt`.
4. Call `verify_substrate_receipt(&corpus_root, &receipt, &manifest)`.
5. Convert success or failure into doctor output rows.

Because `verify_substrate_receipt` currently writes validation failures to
stderr, the doctor helper should avoid leaking verifier diagnostics to stderr.
The implementation can either:

- add a quiet validation wrapper that returns a typed status without printing;
  or
- split the verifier into a pure `validate_*` helper and a printing wrapper used
  by `corpus verify`.

The recommended implementation is the second option. It improves the existing
boundary: hard verification can print actionable errors, while diagnostic
surfaces can reuse the same validation without changing exit behavior.

## Data Flow

`quantac doctor` should keep its existing sections:

1. Header and version.
2. C backend readiness.
3. Standard library and registry discovery.
4. Optional tool probes.
5. Backend maturity.
6. New substrate evidence section.
7. Practical C-backend readiness summary.

The substrate section should be derived from the receipt content, not hardcoded
from README or STATUS prose. At minimum:

- schema comes from `receipt.schema`;
- corpus count comes from `manifest.programs.len()`;
- C anchor row comes from `execution_surface.c.maturity`;
- Rust subset row comes from `execution_surface.rust.maturity`;
- SPIR-V unverified row comes from `execution_surface.spirv.status` or
  `unsupported_mir_policy`;
- memory counts come from `memory_surface.verified_surfaces.len()` and
  `memory_surface.known_gaps.len()`;
- representation row comes from `representation_surface.ir` and
  `fallback_policy`.

## Error Handling

`quantac doctor` remains diagnostic-only:

- missing corpus root: print `receipt missing`;
- missing manifest: print `receipt missing`;
- missing substrate receipt: print `receipt missing`;
- JSON parse or schema error: print `receipt invalid`;
- substrate validation error: print `receipt invalid`;
- all successful: print the full compact evidence summary.

`quantac corpus verify` remains the command that exits nonzero and prints field
level validation errors for substrate drift.

## Testing

Use CLI tests in `compiler/tests/cli.rs` because the value is user-facing doctor
output.

Test coverage should include:

- `doctor_reports_adoption_readiness_summary` expects `Substrate evidence:`.
- The same test, or a focused new test, expects `receipt   ok`,
  `quantalang-substrate-receipt/v0`, `corpus    ok`, `c         anchor`,
  `rust      subset`, `spirv     unverified`, `memory    partial`, and
  `repr      MIR`.
- Existing substrate and corpus verification tests remain the hard evidence
  gates for invalid receipts.

Focused verification commands:

```powershell
cargo fmt --manifest-path compiler/Cargo.toml -- --check
cargo test --manifest-path compiler/Cargo.toml --test cli doctor -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli corpus_verify -- --nocapture
cargo test --manifest-path compiler/Cargo.toml --test cli substrate -- --nocapture
```

## Non-Goals

- No new substrate schema version.
- No new backend production claim.
- No SPIR-V, LLVM, WASM, x86-64, or ARM64 execution proof.
- No generated substrate receipt writer.
- No new `quantac substrate` command.
- No change to `quantac doctor` exit semantics.
- No broad README/STATUS rewrite.

## Acceptance Criteria

This design is implemented when:

- `quantac doctor` prints a `Substrate evidence:` section.
- A valid repository checkout reports the substrate receipt as `ok`.
- Doctor output summarizes the current corpus count, C anchor, Rust subset lane,
  SPIR-V unverified posture, memory verified/gap counts, and MIR fallback
  posture from the receipt.
- Missing or invalid substrate evidence is reported on stdout without making
  `doctor` fail.
- `quantac corpus verify` remains the hard failing gate for substrate drift.
- Focused CLI tests pass.

## Future Extensions

Later slices can make this section richer without changing its purpose:

- JSON output for `quantac doctor --json`;
- per-program MIR operation coverage;
- symbol graph evidence;
- GPU validator evidence when SPIR-V has `spirv-val` or Vulkan-host receipts;
- latency and responsiveness receipts for watch, LSP, and incremental compile
  surfaces.

The invariant is that doctor should explain current evidence posture while
release gates prove it.
