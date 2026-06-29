# Transpile-Preservation Criterion

Status: executable (Phase 1, brick 2)
Owner: BuildLang compiler (`buildlang` crate, `buildc` binary)
Schema referenced: `buildlang-semantic-corpus/v1`, `buildlang.mir/v0`

## The criterion

When the **same** BuildLang program is lowered to the MIR interlingua
(`buildlang.mir/v0`) and then emitted through two different backend
target languages, the **observable contract** that must be preserved is:

> **Byte-identical stdout and equal process exit status, independent of
> the target language.**

Concretely, for a program `P`:

```
run( compile_C(  lower_to_mir(P) ) ).stdout  ==  run( compile_Rust( lower_to_mir(P) ) ).stdout
run( compile_C(  lower_to_mir(P) ) ).exit    ==  run( compile_Rust( lower_to_mir(P) ) ).exit
```

The comparison is **byte-for-byte after a single CRLF->LF normalization**
(Windows console hosts emit `\r\n`; the normalization is applied
identically to both sides so it cannot hide a genuine divergence). The
test asserts that the **two backends agree with each other**, not merely
that each backend matches its own pre-recorded expected stdout. Agreeing
with a shared expectation is a weaker property than agreeing with each
other on a value neither side knew in advance of the run.

## Why this is the conservation pillar

The universal substrate claims that a program's meaning is carried by the
MIR, not by any one backend. If two backends lowered from the same MIR
could print different bytes, the MIR would not be a faithful interlingua
and the "receipts" would be vacuous. This criterion is the executable
witness that lowering conserves the observable behaviour.

## Honest scope (what is and is not cross-checked today)

- **C backend**: production-verified, covers **all** semantic-corpus
  programs (`verify_c_corpus_stdout` in `compiler/src/main.rs`).
- **Rust backend**: an **experimental executable subset**. Every program
  listed in `semantic-corpus/manifest.json` currently names a
  `generated_rust_runs_for_*` execution test, so the Rust-supported
  subset is presently **the whole manifest** (8 programs). The
  cross-target preservation harness can only assert C-vs-Rust agreement
  on programs the Rust backend can execute, so its scope is exactly that
  Rust-supported subset.
- Programs outside the Rust subset (none today, but expected as the
  corpus grows) are covered by the C path alone and are **not**
  cross-checked. That is a known, declared gap, not an oversight.

## Where the criterion is enforced

`compiler/tests/cli.rs`,
`transpile_preservation_c_and_rust_backends_agree_on_stdout`.

For each Rust-supported corpus program it:

1. Runs the **C path** end-to-end through the real `buildc run` binary
   (the same path `verify_c_corpus_stdout` exercises) and captures stdout
   + exit status.
2. Lowers the **same source** to the Rust backend via the public library
   API, compiles the emitted Rust with `rustc`, runs it, and captures
   stdout + exit status.
3. Asserts the two stdouts are byte-identical and the two exit statuses
   are equal.

The harness **skips cleanly** (does not fail) when no C compiler is
available (`buildc doctor`) or `rustc` is absent, and **runs** when both
are present.
