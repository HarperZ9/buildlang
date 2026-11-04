# BuildLang vision: the language, its database and the machine under it

Written 2026-11-02. This file replaces the 2026-06 graphics vision, which is kept in git
history and summarised in section 9. Milestones M3 to M6, P1 to P8, V1 to V8 and S1 to S4
in `docs/STRATEGY-2026-10.md` and `docs/STRATEGY-2026-10-SURPASS.md` still stand. This
plan adds the parts the author named on 2026-11-02 and orders all of them.

The author, 2026-11-02:

> "We are going to make BuildLang the best language in the world, and make it very simple
> to automatically migrate everything at the click of a button using BuildDB/BuildQL
> (BDB/BQL), and auto migration of languages to native running programs, self compiling,
> hardware level control (machine), low<->high heirarchy and structure with elegant
> design. Also connecting the relation of the RAM, CPU, GPU, and Motherboard to better
> interoperate, and build new architectures that improve the next horizon of computing."

Earlier the same day: "Being able to smoothly transform, and migrate existing languages to
ours is also valuable, as ease of platform migration and profile/personalization imports.
Also the machine language, kernels, and machine learning work." And: "our tools should be
interoperable, native, and interfacing very smoothly ~ care to design, heirarchy,
structure, and both small scale, large scale, and architecture intent with our compiler,
language, and programs." And: "BuildTex meaning we replace Lean and LaTeX ~ no need for it
in BuildLang".

Earlier statements of the same intent: "continue updating, improving, and extending
BuildC/BuildLang, and BuildQL, and BuildDB" (2026-09-05); "Do not stop until
BuildLang/BuildC, and BuildDB/BuildQL are complete public-ready flagship, tnerprise
tools." (2026-09-06); and a request to compare BuildDB/BuildQL with SQLite, PostgreSQL,
MySQL, DuckDB, CockroachDB, TiDB, Turso and ClickHouse (2026-09-10).

## 1. Prior art

Every row below existed before this plan. Nothing in this file claims a capability as new
when a row here already has it.

| Piece | Where | State | Evidence |
|---|---|---|---|
| **qdb, the BuildDB engine** | Build Universe repo, `programs/db.bld` | Built and tested. The published main branch holds the 4,559-line version. A newer line of 39 commits (34 of them to `db.bld`, which grows to 6,913 lines) sits on unpublished branches the author holds; it is not merged or released. | Rebuilt on 2026-11-02 from the newest branch with buildc 1.10.0 from this repo's main and gcc: the qdb regression gate printed `pass=214 fail=0 xfail=1 xpass=0`. The one expected failure is a cross-process ROLLBACK, a known design limit. |
| What qdb runs | same file | Built and tested | CREATE, INSERT, SELECT, UPDATE, DELETE, DROP, ALTER TABLE ADD COLUMN; WHERE trees with LIKE, IN, BETWEEN, IS NULL and scalar subqueries; GROUP BY, HAVING, COUNT, COUNT DISTINCT, SUM, AVG, MIN, MAX; INNER and LEFT JOIN, including grouped joins; UNION and UNION ALL; DISTINCT; ORDER BY with aliases and ordinals; LIMIT and OFFSET; sorted-array indexes; a write-ahead log, snapshots and BEGIN/COMMIT/ROLLBACK; a REPL. |
| **BuildQL, the query language** | qdb's SQL dialect; `programs/sql.bld` is a standalone parser demo | Built (dialect); demo only (sql.bld) | sql.bld parses and prints, and never executes. The dialect of record is the one qdb executes. |
| qkv, qjq, qcsv | Build Universe `programs/` | Built | A key-value store, a JSON query tool and a CSV converter, the earlier steps of the same line. |
| `sql!{ ... }` blocks | this repo, lexer and parser | Lexed, then dropped | The lexer recognises `sql!`, `regex!`, `math!` and others, and the parser keeps the name with empty contents (`parser/expr.rs`). Nothing executes a query. |
| MIR interlingua | `compiler/src/codegen/ir.rs`, `buildc mir emit/load` | Merged and released | Versioned `buildlang.mir/v1` JSON with lossless round trip. |
| Build Data Format (BDF) | `compiler/src/bdf.rs`, `buildc bdf` | Merged and released | Deterministic, content-hashed binary and JSON forms. The name sits close to BDB; see section 8. |
| Symbol and module graph receipts | `compiler/src/symbol_graph*`, `module_graph*` | Merged | Per-program source symbols, MIR symbols and effect symbols over the semantic corpus. They state "call graph is not claimed". |
| Effect summaries | `compiler/src/types/check.rs` (`FunctionEffectSummary`) | Merged and released | Declared, observed and propagated capability effects per function. |
| Check receipts and policies | `buildc receipt`, `buildc policy` | Merged and released | Source-bound receipts that a third party re-checks. |
| LSP rename | `compiler/src/lsp/server.rs` | Merged | Text search on word boundaries with no compiler check. It edits matches inside strings and comments, and it does not treat `_` as part of a word, so renaming `add` also edits the `add` inside `my_add`. |
| Transpile preservation | semantic corpus, C and Rust backends | Merged | Executable agreement between the C and Rust backends on the corpus: the pattern section 4 reuses for migration. |
| Self-hosted compiler tree | `future/self-hosted-compiler/` | Not compilable | 251,590 lines of BuildLang that the current compiler cannot build (`ARCHITECTURE.md`). |
| MIR to Lean experiment | a 2026-09-16 scratch experiment, unpublished | Exploratory | Lowered one closed integer program's MIR to Lean and checked it with `lean --trust=0`. |
| typeset | PR #115, branch `feat/typeset-field` | Open PR | A document layer in BuildLang, Knuth-Plass line breaking in BuildLang and a layout engine. Section 7 cross-references it. |
| GPU path | `compiler/src/gpu/`, `examples/gpu/` | Merged; experimental | Compute SPIR-V with a CPU C reference, an optional Vulkan host behind `--features gpu`, and saxpy, reduce, matmul and stencil examples. |
| buildos | Build Universe `buildos/` | Built for QEMU; real-hardware boot unknown | A Rust `no_std` x86-64 kernel with ACPI, PCI, NUMA-aware memory, SMP scheduling, AHCI and NVMe. Its `lib.bld` describes the design in BuildLang. |
| superframe kernels and joules | superframe PR #2 (`packages/kernels`, `packages/joules`) | Open PR | A kernel contract with bit-identical CPU and CUDA modes, planted bad kernels, receipts and a "BuildLang port note" per kernel. The joules meter reads RAPL and NVML. |
| Editor support | `buildlang-vscode`, `buildlang-tmLanguage` repos | Released | Syntax and editor support. |

## 2. What exists today

- **Pipeline.** Lexer, parser, Hindley-Milner type checker with capability effects, MIR,
  then C99 and the system C compiler (`ARCHITECTURE.md`). C is the product path. HLSL and
  GLSL are supported shader output. LLVM, WASM, SPIR-V, x86-64, ARM64 and Rust are
  experimental and fail loudly on unsupported MIR (`compiler/src/codegen/backend/STATUS.md`).
- **Effects.** Ten built-in capabilities (`Console`, `FileSystem`, `Network`, `Process`,
  `Environment`, `Clock`, `Foreign`, `Gpu`, `Random`, `Model`) in
  `compiler/src/types/capabilities.rs`, user-defined algebraic effects with handlers, and
  receipts over them. Handlers are one-shot and do not resume (`docs/EFFECTS_GUIDE.md`).
- **Memory.** Compiled programs do not free heap memory by default. Drop insertion for
  strings ships behind `BUILDLANG_EXPERIMENTAL_FREE`, ASan-clean on its fixtures; vectors
  and maps are not freed (`STATUS.md`, `docs/MEMORY-PILLAR-DESIGN.md`). Linear types are
  opt-in. Bounds checks and division traps are inserted.
- **Self-hosting.** None. The largest BuildLang program this compiler builds is qdb.
- **Migration and import.** No source-language frontend exists. C header import (P1) is
  planned, not built. C ABI export and static libraries exist since 1.9.0.
- **Measured performance.** No matched comparison with any competitor exists. The only
  compile-time table is from buildc 1.0.0 on 2026-03-28 (`TEST_RESULTS.md`).

## 3. "Best language" as targets

Each row names the language to beat in that area, where BuildLang stands today, the
target and the benchmark that would show it. A lead is claimed only after the benchmark
runs on a dated release with its receipt. Where BuildLang is behind, the row says so.

| Area | Compare with | Today | Target | Benchmark that decides it |
|---|---|---|---|---|
| Compile time | Zig, Go, Rust (`cargo check`) | Unmeasured. Each build also pays for a C compiler run. | Check plus native build of a 10k-line program within 1.5x of Go's `go build` on the same machine | A shared suite of generated and real programs, clean and incremental, 10 runs each, median with a 95% interval |
| Run time | C++, Rust, Zig, Julia | Unmeasured; C output is unoptimised MIR-to-C | Within 1.25x of Rust on the kernel set; Julia's numeric kernels as in P3 | The P3 kernel set plus qdb workloads, each with an idiomatic twin, matched outputs and published raw times |
| Memory safety | Rust | **Behind.** No default reclamation; no borrow checker | Default-on drop insertion for every heap type, then the V1 crash-class certificates | ASan and leak checks over the corpus and qdb's gate; the V1 seeded-crash corpus |
| Effect tracking | Koka, OCaml 5, Zig's explicit I/O | Capability effects in the type, receipts a third party re-checks, one-shot handlers | Multi-shot handlers, effect-polymorphic library code, effect rows on hardware access (section 5) | A shared effect-program suite, including generators and async, run in each language; misuse cases that must be refused |
| Ergonomics for agents | Python, Mojo, Bend 2 | Held-out probe on 1.6.0: 20/27 and 17/27 (`bench/heldout-2026-10-15`) | M3: 25/30 for two generators with zero check-pass-but-fail | M3's sealed held-out probe |
| Tooling | Rust (cargo, rust-analyzer), Go | CLI, LSP with partial dispatch, formatter, MCP server; no live registry | P7 registry; an LSP whose rename and references come from the compiler | Editor task suite (rename, find references, quick fix) timed and scored; P7's offline rebuild check |
| GPU | Mojo, CUDA C++, Slang | Experimental SPIR-V compute with a CPU reference; HLSL and GLSL output | M6 device runs, then the superframe kernels contract on CPU and GPU | `kernels check` from superframe on BuildLang-built kernels; M6's golden suite |
| Proofs | Lean 4, Rocq, Bend 2 laws | None in the compiler; one exploratory MIR-to-Lean run | V2 contracts discharged by SMT, with goals exported to Lean | V2's contract suite including refusals of false contracts; V8's shared benchmark |
| Documents | LaTeX, Typst | typeset in PR #115 | A BuildLang document compiles to PDF, SVG and HTML from one source and matches LaTeX's line breaks on a reference set | Paragraph-break agreement with TeX on a reference corpus; build time against Typst on the same documents |
| Migration | c2rust, Mojo's Python import, transpilers | None | `buildc migrate` for a C subset, then Python, each with differential tests and a refusal report | Section 4's corpus: share of programs migrated, share whose outputs match, every refusal listed |

## 4. Subsystems

Two subsystems have design documents in this change. The others are listed with their
milestone in section 6 and get their own design documents in later changes.

- **BuildDB and BuildQL** (`docs/design/BUILDDB-BUILDQL.md`). BuildDB is qdb grown up and
  BuildQL is qdb's dialect extended to the compiler's IR. The compiler writes a program as
  BuildDB tables, qdb or an in-process subset queries them, and transforms run with
  receipts. "Migrate at the click of a button" is one command, `buildc migrate`, that runs
  a frontend, lowers to MIR, writes the tables, checks equivalence by differential tests
  and emits native code. The same store takes profile and personalisation imports.
- **Language migration** (`docs/design/LANGUAGE-MIGRATION.md`). One frontend per source
  language in this order: a C subset, Python, JavaScript and TypeScript, Rust, Lean, then
  `.tex` into typeset. Differential testing checks every migrated program, and every
  construct a frontend cannot translate is reported.
- **Self-compiling** (H8, H11). Grow a small self-hosted compiler one component at a time,
  each checked against the Rust compiler's output on the same files, and accept the
  bootstrap when stage-2 and stage-3 emit byte-identical C.
- **Low to high, hardware model, tensors and kernels** (H7, H9, H12). Design documents to
  follow. They build on the effect system, the existing GPU path and the superframe
  kernels and joules packages listed in section 1.

## 5. The first slice, shipped with this plan

`buildc bql` (`compiler/src/bql/`). It loads one compiled program into four tables built
from compiler data: `functions` and `calls` from MIR, `effects` from the type checker's
summaries, and `items` from the AST. It can then:

- export the tables in qdb's checkpoint format, so qdb runs its whole dialect over a
  program (`buildc bql export prog.bld -o prog.qdb`, then
  `qdb prog.qdb "SELECT effect, COUNT(*) FROM effects GROUP BY effect"`);
- run the SELECT, WHERE, ORDER BY and LIMIT subset of the same dialect in process, and
  refuse the rest with a message that names qdb (`buildc bql query`);
- rename a top-level function with every reference, re-lower the result and check that
  MIR equals the original with only the name changed, writing a receipt with both digests
  (`buildc bql rename`). Where the old name is also a local, field or method, MIR changes
  beyond the name and the rename is refused.

Why this slice: the call graph and the effect facts already exist inside the compiler but
had no queryable form, and qdb already exists as the engine. Joining the two costs little,
and the migration frontends, the hardware placement and the LSP all write to the same
tables later. The checked rename is also the first transform with a receipt, and it fixes
the class of error the LSP's text rename makes.

Tests: 12 library tests over real example programs and a networked fixture, 4 CLI tests
(including a renamed program that builds and prints the same output as the original), and
a differential test that runs the same queries through a qdb binary when `QDB_BIN` is set.
On 2026-11-02 that differential test passed against qdb built from its newest branch.

## 6. Sequencing

Each milestone ships something usable and has an acceptance test that can fail. The game
and engine come first, as `docs/STRATEGY-2026-10.md` sets out; this order runs beside them.

| # | Milestone | Ships | Accepted when |
|---|---|---|---|
| H1 | BQL core (this PR) | `buildc bql export, query, rename` | The tests above pass in CI; the qdb differential passes where qdb is built |
| H2 | qdb in CI | qdb built by buildc in this repo's CI (after the author decides its home) | qdb's 214-check gate runs green on Linux and Windows on every PR |
| H3 | Whole-program store | Multi-file programs, types, globals, spans for every reference | The store for qdb itself loads, and `SELECT` over it matches a hand count of its functions and calls |
| H4 | Compiler-backed LSP | Rename and references from the store | An editor task suite in which every rename is MIR-checked and the old text rename's false matches are gone |
| H5 | Memory by default | Drop insertion on by default for strings, vectors and maps | ASan and leak checks clean over the corpus and qdb's gate |
| H6 | C-subset migration | `buildc migrate --from c` with differential tests | Section 4's C corpus: outputs match on every migrated program; every refusal is listed |
| H7 | Hardware topology | `buildc hw probe` writes the machine's topology as BuildDB rows, each number graded | Values match the OS's own reports on the test machines; vendor figures carry their grade |
| H8 | Self-hosted lexer | A BuildLang lexer built by buildc | Its token stream equals the Rust lexer's on every `.bld` file in the repo |
| H9 | Kernels port | `blend` and `saxpy` written in BuildLang under the superframe kernels contract | `kernels check` passes, planted-bad controls fail, joules per result reported |
| H10 | Python-subset migration | `buildc migrate --from python` | The Python corpus as in H6, with CPython as the reference |
| H11 | Self-hosted front end | Lexer, parser and type checker in BuildLang | Stage-2 and stage-3 C output byte-identical on the front end |
| H12 | Placement experiments | Section 4 of the hardware design | Each experiment's preregistered pass or fail is published, including failures |

## 7. BuildTeX

BuildTeX is the author's name for documents and proofs written in BuildLang in place of
LaTeX and Lean. The document half is the typeset work in PR #115: a document layer in
BuildLang, Knuth-Plass line breaking in BuildLang and a layout engine with themes as data.
This plan does not redesign it. The proof half is milestone V2 (contracts discharged by
SMT, goals exported to Lean or Rocq), with the MIR-to-Lean experiment as its seed; a Lean
replacement means V2's checker carries the proofs that Lean would. Where they touch this
plan: a document's IR (`buildlang.doc/v0`) becomes BuildDB tables like a program's, and the
`.tex` migration frontend (section 4) imports a LaTeX subset into it.

## 8. Decisions for the author

1. **Home for qdb.** Move `programs/db.bld` into this repo as `bdb/` so CI builds it with
   the compiler, or keep it in Build Universe and pin a revision. Its newest 39 commits are
   on unpublished branches; publishing them comes first either way.
2. **The name.** The author said "BDB, not qdb" on 2026-10-29. A rename of the engine, its
   binary and its docs needs a written checklist. BDF (Build Data Format) and BDB are one
   letter apart; keep both or rename one.
3. **qdb's file format.** It has no escape for `|` or a line break, so the export refuses
   such cells. Choose an escaped text format v1 or BDF as the storage encoding.
4. **First migration language.** This plan proposes a C subset first (closest semantics,
   existing C ABI and corpus) and Python second (largest user base).
5. **Licence for frontends.** The compiler is AGPL-3.0-only. A frontend that parses a
   language with an existing library (for example a Python or TypeScript parser) inherits
   that library's terms, and frontends may want a more permissive licence so other tools
   embed them.
6. **Hardware for experiments.** The NUMA and multi-GPU experiments need a two-socket or
   multi-GPU machine that is not available here.

## 9. Where BuildLang is behind

Plainly: memory reclamation (Rust frees by default; BuildLang does not), measured
performance (no matched benchmark exists in any area), self-hosting (Rust, Zig, Go and Lean
build themselves), proofs (Lean and Rocq are mature; BuildLang has none in the compiler),
GPU (Mojo and CUDA run on devices in production; BuildLang's device path is experimental),
packages (Cargo has a registry; BuildLang has none live), documents (LaTeX and Typst are
released; typeset is an open PR), and migration (c2rust and Mojo's Python interop exist;
BuildLang has no frontend). The effect system with re-checkable receipts is the area with
no direct equivalent in the compared languages, and that is still a claim to test with the
effect-program suite in section 3.

### The earlier graphics vision

The 2026-06 version of this file set three pillars: one source compiled to CPU and GPU with
identical results, colour-space and precision types, and a render-graph language. The
first is now milestone M6 and the GPU row above. The second and third remain design
proposals; the field type in PR #115's `docs/FIELD.md` carries units, uncertainty and
representation, which covers the colour-space idea in a more general form.
