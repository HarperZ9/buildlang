# Language migration: other languages into BuildLang

Status: the first frontend slice exists (2026-11-12): `port/` reads a C subset and is checked against gcc (`port/README.md`). The rest of this document is design. Written 2026-11-02.

## Goal

Take a program written in another language, produce a BuildLang program and a native
binary that behave the same, and say exactly what could not be carried over. Behaviour
is shown by running both, never assumed from the translation.

## Prior art to reuse

- The semantic corpus already checks that the C and Rust backends print the same output
  for the same program (transpile preservation). Migration uses the same harness with
  the source language's own toolchain as the reference.
- C ABI export and static libraries (1.9.0) let a migrated module replace one file of a
  C or C++ program at a time.
- BuildDB tables (`BUILDDB-BUILDQL.md`) hold the frontend's output and its refusals.

## Order of frontends

Ranked by value over difficulty. Each later frontend reuses the earlier ones' MIR
lowering and test harness.

| Order | Source | Why here | Main difficulty | Reference toolchain |
|---|---|---|---|---|
| 1 | C subset (C99 without the preprocessor's dark corners, no `goto` into blocks, no unions at first) | Closest semantics to the C backend; the round trip C to BuildLang to C is testable at once | Pointer arithmetic, undefined behaviour, macros | gcc and clang with `-fsanitize=undefined` |
| 2 | Python subset (functions, classes without metaclasses, lists, dicts, str, int, float) | Largest user base; P6 already plans interop | Dynamic types need inference or annotations; big-integer `int` | CPython |
| 3 | JavaScript and TypeScript subset | Tools and front ends; TypeScript types help | Prototype chains, `this`, async | Node |
| 4 | Rust subset | Close types and ownership | Traits, lifetimes, macros | rustc |
| 5 | Lean (definitions and theorems) | BuildTeX's proof half (V2) | Dependent types beyond BuildLang's type system | Lean's kernel |
| 6 | `.tex` subset | BuildTeX's document half | TeX macros are a programming language | pdfTeX or LuaTeX output compared through typeset |

## Pipeline per frontend

1. **Parse** with the language's own grammar. A parser library carries its licence; see
   the licence decision in ROADMAP_VISION section 8.
2. **Lower** each construct to BuildLang AST, then through the normal checker and MIR.
   A construct with no lowering writes an `untranslated` row with its span and reason.
3. **Map effects.** I/O calls map to capability effects (a Python `open` becomes
   `FileSystem`, a socket becomes `Network`), so the migrated program's authority is
   explicit and the BuildQL query `SELECT * FROM effects` shows it.
4. **Differential test.** Run the source and the migrated binary on the same inputs:
   the source's own tests where they exist, generated inputs otherwise. Compare stdout,
   exit code and written files. Floating-point output is compared exactly unless the
   migration declares a ULP bound and the reason.
5. **Emit** native code through the C backend and a receipt listing inputs, comparisons
   and refusals.

## What counts as success

For a corpus of programs per language, published with each frontend:

- the share that migrate with zero `untranslated` rows;
- of those, the share whose outputs match on every input;
- every refusal, grouped by construct.

A migrated program whose output differs on any input is a failure of the frontend and is
reported. It does not ship. A frontend is released when its corpus result is published,
not when it reaches a fixed percentage.

## Undefined behaviour and semantics gaps

C programs with undefined behaviour have no single meaning to preserve. The C frontend
runs the source under UBSan first; a program that triggers UBSan on its test inputs is
reported as such and not counted as a mismatch. Python's unbounded `int` maps to `i64`
with a checked overflow trap until a big-integer type exists, and the receipt names the
trap as a semantic difference.

## First milestone (H6)

A C-subset frontend that handles the semantic corpus programs written in C, plus a set of
small numeric and string programs. Acceptance: every migrated program's output equals
gcc's on its inputs, and the refusal report lists every construct outside the subset.
