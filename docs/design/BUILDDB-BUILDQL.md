# BuildDB and BuildQL: the program as a database

Status: design, with the first slice built (`compiler/src/bql/`, `buildc bql`). Written
2026-11-02.

## What already exists

BuildDB is qdb, the SQL engine written in BuildLang (`programs/db.bld` in the Build
Universe repository). BuildQL is the SQL dialect qdb executes. qdb has DDL and DML, WHERE
trees, GROUP BY with HAVING, the five aggregates and COUNT DISTINCT, INNER and LEFT JOIN
with grouping, UNION, DISTINCT, ORDER BY, LIMIT and OFFSET, sorted-array indexes, a
write-ahead log with snapshots and transactions, and a REPL. Its regression gate passed
214 checks with one expected failure when rebuilt with buildc 1.10.0 on 2026-11-02.
`programs/sql.bld` is a parser demo that does not execute.

This design does not start a second database. It makes the compiler write into qdb's
format, extends the dialect's reach to compiler data, and adds transforms.

## The tables

A compiled program becomes these tables. Version 0 (shipped) has the first four.

| Table | Columns | Source |
|---|---|---|
| `functions` | name, kind (`defined` or `extern`), is_public, span_start, span_end, shader_stage | MIR functions, AST spans |
| `effects` | function, effect, how (`declared`, `observed`, `propagated`), via | type checker effect summaries |
| `calls` | caller, callee | MIR `Call` terminators; `<indirect>` for calls through a value |
| `items` | name, kind | AST top-level items other than functions |
| `refs` (v1) | symbol, span_start, span_end, role | resolved references from the type checker |
| `types` (v1) | name, kind, size, align | MIR type definitions and the memory-layout receipt |
| `blocks`, `stmts` (v1) | function, block, index, op | MIR bodies, for queries such as "every function that stores to a global" |
| `untranslated` (migration) | source_file, span, construct, reason | a migration frontend's refusals |
| `profile` (imports) | tool, key, value, source_sha256 | imported editor and formatter settings |
| `topology` (hardware, H7) | to be designed | a machine probe |

Rows are sorted, so the JSON form and its SHA-256 digest are deterministic. The schema tag
is `buildlang.bdb/v0`.

## Queries

`buildc bql export prog.bld -o prog.qdb` writes qdb's checkpoint format, and qdb then runs
its whole dialect:

```
qdb prog.qdb "SELECT DISTINCT function FROM effects WHERE effect = 'Network'"
qdb prog.qdb "SELECT caller, COUNT(*) FROM calls GROUP BY caller"
```

The compiler also needs answers in process, inside tests and transforms, without a qdb
binary. `buildc bql query` runs the SELECT, WHERE (`=`, `!=`, `IS NULL`, AND, OR,
brackets), ORDER BY and LIMIT subset over one table, with qdb's NULL rules, and refuses
everything else with a message that names qdb. A differential test runs the same queries
through both and compares the rows when `QDB_BIN` points at a qdb binary. The subset stays
small on purpose: when qdb is built by buildc in CI (milestone H2), the compiler can call
the same engine and the subset becomes a fast path with the differential test as its
guard.

## Transforms

A transform rewrites the program and proves it kept what it claims to keep. Each one writes
a receipt (`buildlang.bql-transform-receipt/v0`) with the digests of the source, MIR and
store before and after, the edited spans and the name of the check that passed.

The first transform, `buildc bql rename FILE OLD NEW`, renames a top-level function:

1. Load the program and confirm OLD is a defined top-level function other than `main`.
2. Confirm NEW lexes as one plain identifier and appears nowhere in the program, so the
   rename cannot capture an existing name.
3. Replace every identifier token spelled OLD. Strings and comments are not tokens, so
   they are left alone.
4. Lower the result again. Its MIR must equal the original MIR with only the function name
   substituted, and its effect summaries must match the same way. A local, field or method
   that shared the name changes MIR elsewhere, so the rename is refused.

Planned transforms, each with its own check: extract function, inline function, change an
effect row (checked by re-running the effect checker and diffing the `effects` table), and
the migration lowerings below. In BuildQL they become statements such as
`TRANSFORM rename_function('add_month', 'accrue')`, which qdb's dispatcher forwards to the
compiler.

## Migration at the click of a button

`buildc migrate SRC --from LANG` is one command, and an editor command calls the same
entry point:

1. A frontend parses SRC and lowers what it can to BuildLang MIR. It writes every construct
   it cannot translate into `untranslated`, and never drops one.
2. The compiler writes the store, and BuildQL checks run over it: no `untranslated` rows,
   no `Foreign` effect the user did not allow.
3. Differential tests run the original program with its own toolchain and the migrated
   build on the same inputs, and compare outputs.
4. The C backend emits native code. The receipt lists the inputs, the comparisons and
   every refusal.

The migration design is in `LANGUAGE-MIGRATION.md`.

## Profile and personalisation imports

The same store takes settings from other tools: VS Code `settings.json`, `.editorconfig`,
`rustfmt.toml`, `.clang-format`, Black and Prettier settings. Each importer writes
`profile` rows with the source file's digest, and `buildc fmt` and the LSP read them. A
setting with no BuildLang equivalent goes into `untranslated` with the reason, the same
as a language construct.

## Where qdb and the new direction disagree, and the proposal

| Point | qdb today | The new direction needs | Proposal |
|---|---|---|---|
| Home | Build Universe, newest work unpublished | built and tested with the compiler | Move to this repo as `bdb/` after the author decides (ROADMAP_VISION section 8) |
| File format | text with no escaping | arbitrary symbol and path text | Escaped text v1, or BDF as the storage encoding |
| Process model | one process per statement; cross-process ROLLBACK is a known limit | many queries per compile | A library entry point the compiler links through the C ABI |
| Sorting | selection sort, O(n^2) | whole-program stores of 10^5 rows | Replace with merge sort once a measured store shows the cost |
| Transforms | UPDATE on rows | checked rewrites of programs | Transform statements dispatched to the compiler, with receipts |

## Known limits of version 0

Single-file programs only; a program with `use`, `mod name;` or an include is refused. No
`refs` table yet, so the rename relies on the MIR check, not on resolved references.
The call graph lists runtime calls under their lowered names (`println!` appears as
`printf`).
