# Agentic Language Directive - 2026-10-09

Status: in force. This is the named source of truth for the project's identity and
direction. Where any in-tree document disagrees with this directive, this directive
governs until a later dated successor replaces it.

## Decision

The author approved, on 2026-10-09, the thesis below and milestone M1 as the first
implementation slice, and asked for version 1.5.0 to be published to crates.io.

## Supersession Notice

This directive supersedes `docs/UNIVERSAL_SUBSTRATE_DIRECTIVE_2026-06-30.md` ("the honest
scientific language"). That file stays in the tree with a banner marking it superseded.
The work it started is not withdrawn: the scientific-runtime receipts, units of measure,
checked arithmetic and the C-path hardening all remain part of BuildLang and keep their
tests. What changes is the order of work and the reason for it.

## Identity (2026-10-09)

BuildLang is the language for agent code whose authority must be bounded and whose
actions must be provable.

- The effect row in a function's type says what the code may do (`~ FileSystem`,
  `~ Network`, `~ Model`).
- A receipt says what the code did, and a third party re-derives it.
- The compiler keeps the two consistent, so a receipt can only record effects the type
  allowed.

This serves two kinds of user. An agent writing code gets a checker that bounds what the
code can touch and tells it, precisely, what to fix. A person running an agent gets a
record of its actions that does not depend on trusting the agent.

Boundary: this does not claim BuildLang code is correct, secure or safe to run
unattended. It claims that its authority is declared, checked and evidenced. A person
still decides what authority an agent gets; receipts make that decision checkable.

## Why this order

A small probe on 2026-10-09 asked one frontier model to write ten small programs in
Python, Go, Rust and BuildLang, one sample each, under a preregistration. Python, Go and
Rust scored 10 of 10. BuildLang scored 4 of 10 from a short description and 7 of 10 with
the specification. Eight of the nine BuildLang failures passed `buildc check` and then
failed inside the generated C. None was a missing effect declaration. One program passed
every check and crashed at run time.

This is exploratory (n=10, one sample, one model) and supports no general claim. It does
say where to start: an agent cannot rely on a passing check today, and until it can, the
effect types are a promise the toolchain does not keep.

## Milestone M1: the checker is the truth

Every exit criterion is a test a third party can run.

1. Zero programs where `buildc check` succeeds and C compilation fails, across
   `tests/programs`, `examples`, `semantic-corpus`, the probe programs, and a generated
   set of at least 200 variants drawn from the probe's failure classes.
2. `buildc check --error-format=json` output validates against a committed JSON Schema.
   Every type error has a stable code and a test.
3. Every stored fix suggestion appears in both human and JSON output.
4. Every fenced BuildLang code block in `SPECIFICATION.md`, `docs/EFFECTS_GUIDE.md` and
   `README.md` is extracted and checked in CI.
5. A fresh preregistered rerun of the probe scores at least 9 of 10 for BuildLang with
   the documentation, with zero failures that pass `check`. A shortfall is reported as a
   shortfall.

## After M1 (sequenced, not dated)

- M2: agent-facing toolchain. An MCP server for the compiler, an agent skill file and
  `llms.txt`, LSP parity with the JSON diagnostics.
- M3: typed model and tool effects. `model<T>` whose output schema is the type T; a
  `Tool` effect exported as MCP tool definitions; receipts emitted by compiled programs.
- M4: receipts bound to effect rows at run time; a recorded effect outside the declared
  row fails verification.
- M5: durable execution with replay, and capability attenuation with every use recorded.
- M6: a public, preregistered benchmark across languages and models, with raw outputs.

Deferred: native and GPU backends beyond C, the in-process JIT, self-hosting, and a sound
linear-type checker. None of them is withdrawn; none of them moves this thesis.

## Honest Baseline (2026-10-09)

`STATUS.md` states current verified capability. In short: the C path is the only backend
verified end to end; capability effects are enforced at compile time over ten
capabilities; check and scientific receipts are re-derivable; compiled programs do not
yet free heap memory; diagnostics are human-readable text only.

## Source Of Truth

The dated directive is the posture. `STATUS.md` states verified capability. Design specs
under `docs/superpowers/specs/` describe the bricks. Roadmap documents describe ambition.
None of them overrides this directive. The next dated directive supersedes this one.
