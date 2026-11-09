# What finished means for BuildDB and BuildQL

Written 2026-11-08 in Build Universe and moved here with qdb on 2026-11-09. This file defines
when the pair is finished, orders the work into stages, and gives every row a check that can fail.

## What the names mean

- **BuildDB** is qdb, the SQL engine in `bdb/db.bld`: storage, the write-ahead log, indexes, the
  executor and the REPL.
- **BuildQL** is the SQL dialect qdb executes. The parser inside `bdb/db.bld` is its definition;
  the list of what it accepts is in `bdb/README.md`. `programs/sql.bld` in Build Universe is a
  separate parser demo that prints and never executes, and it is not BuildQL. The compiler side of
  BuildQL is `compiler/src/bql/` (`buildc bql export|query|rename|verify`): it writes a compiled
  program into qdb's file format and runs a small in-process subset of the dialect. The design is
  `docs/design/BUILDDB-BUILDQL.md`.

## The goal

The author, 2026-11-09: "we should be aiming to complete BuildLang and BuildC first; as well as
BuildDB and BuildQL." Earlier: "for the long shot and work on the aim for the longer term goal and
work our way to it." So the definition is staged, and the pair is finished when every row of every
stage says PASS.

## How a row is scored

- **PASS**: the row's check ran and met its bar. The evidence names the compiler (buildc version or
  buildlang commit), the qdb commit and the command.
- **FAIL**: the check ran and missed its bar. The evidence says by how much.
- **NOT BUILT**: the feature or its check does not exist yet. Parts that exist are listed; a part is
  not a pass.

Coverage is measured against outside references, never only against our own tests:

- **sqllogictest**, D. Richard Hipp's SQL conformance corpus (licence: the author's choice of GPL,
  BSD, MIT or CC0; no attribution required). Route: the Fossil site answered with a bot check on
  2026-11-08, so the files came from the public GitHub mirror `gregrahn/sqllogictest` at commit
  c93ac416f05635ff5cc490009add936d9753d01d. The corpus is not vendored; the runner takes its path.
  The selected denominator is fixed below before any run.
- **SQLite as a differential oracle**: generated workloads inside qdb's dialect run through qdb and
  through Python's `sqlite3`, and the rendered rows are compared.

## Preregistration (written 2026-11-08, before any run below)

Bounds, predictions and planted checks for every measured row. An amendment is added here, dated,
before the run it governs. Results go in the stage tables; a result that misses its prediction is
kept.

**P1. sqllogictest coverage.** Denominator: every `query` record in `test/select1.test` through
`select5.test` and `test/evidence/*.test` (12 files) at the commit above, after the corpus's own
conditionals are applied for the engine name `sqlite` (`skipif sqlite` skipped, `onlyif X` for
X other than `sqlite` skipped), since the expected results are SQLite's. Records counted: 9,202
query records before conditionals. A query passes when qdb accepts it and its rendered values match
the expected values after the corpus's sort mode and hash rule. Every other query is either
REJECTED (qdb prints an error or exits non-zero) or WRONG (qdb answers with other values). Statement
records are reported separately. Prediction: under 5 percent of query records pass. Most of the
corpus uses arithmetic and CASE expressions in the select list, which qdb's parser refuses
(`SELECT a+1` fails with "expected keyword 'FROM'"). Planted check: the runner must score a qdb
answer it is fed as wrong when one value is altered, and must score SQLite itself at 100 percent of
the records it runs (the runner run with `--engine sqlite`), or the runner is broken.

**P2. SQLite differential.** Seeds 1 to 10, each a generated workload of two tables, 60 inserted
rows and 200 SELECTs inside the dialect qdb documents (WHERE trees with AND, OR, NOT, IN, LIKE,
BETWEEN, IS NULL; ORDER BY on a total order; LIMIT and OFFSET; DISTINCT; COUNT, SUM, MIN, MAX,
AVG; GROUP BY and HAVING; INNER and LEFT JOIN). Denominator: 2,000 queries. Prediction: at least
95 percent agree. Expected disagreement classes: NULL against the empty string (qdb stores both
as an empty cell), and number rendering (REAL and AVG). Planted check: one generated query has its
qdb answer altered by the harness and must be counted as a disagreement.

**P3. Crash recovery.** A fault-injection hook (`QDB_CRASH_AT=k`, `QDB_CRASH_MODE=before|torn`,
read once at startup) stops the process at its k-th durable write, either before the write or after
writing half of it. The crash workload runs every statement as its own process. For each statement
and each of its write points, in both modes, the harness crashes it, recovers on a copy of the files
and compares the committed state with the reference states. Rule: the recovered committed state is
the state before the statement or the state after it, and it is the state after it when the
statement's COMMIT record was fully written before the crash. Predictions on the engine at 4a296c1:
(a) a crash between the checkpoint's `.db` rewrite and the WAL clear replays the WAL twice and
duplicates rows; (b) a torn `.db` rewrite loses every row checkpointed before it. Planted check: a
recovery comparison fed a state with one row removed must report a violation.

**P4. Cross-process transaction.** `BEGIN`, `INSERT` and `ROLLBACK` as three processes leave the
table empty, and the same three with `COMMIT` leave one row. Prediction on 4a296c1: the ROLLBACK
case leaves one row (the documented null `xproc_rollback`).

**P5. Text round trip.** 1,000 generated text values with `|`, `\`, line feeds, carriage returns,
quotes and leading or trailing spaces, inserted, checkpointed and reloaded, and separately
recovered from the WAL. Prediction on 4a296c1: every value holding `|` or a line break comes back
changed. `INSERT ... VALUES (1,'x|y')` reads back as `x` today.

**P6. ORDER BY cost.** `SELECT * FROM t ORDER BY v` over 1,000, 10,000 and 30,000 rows, wall time
of the query process minus the time of `SELECT COUNT(*) FROM t` on the same file. Prediction on
4a296c1: the ratio between 30,000 and 10,000 rows is above 6, the quadratic signature (n log n
gives about 3.3). The selection sort is replaced only if this measurement shows the cost. The
replacement must leave the output of every differential seed byte-identical, since the current
sort is stable.

### Amendments

- **A1 (2026-11-08 17:45, before the first qdb run of P1).** The SQLite self-check scored 9,198 of
  9,202 query records with SQLite 3.49.1 (Python's `sqlite3`). The four misses are the
  integer-overflow records in `evidence/slt_lang_aggfunc.test` at lines 479 to 499: `sum(x)` over
  `1<<63` twice, which modern SQLite rejects with an overflow error where the script records an
  empty result, and three `total` and `sum(DISTINCT)` results whose expected text keeps only 17
  significant digits (`-18446744073709550000.000`) where `%.3f` prints every digit. The runner's
  bar becomes: SQLite passes every record except those four. They stay in qdb's denominator. One
  runner fix came from the same self-check: a statement record may hold several statements, one
  per line ending in `;` (line 476 of the same file), and the runner now runs each.

- **A2 (2026-11-08 18:10, after the first P2 run).** The first P2 run dropped any qdb result row
  whose cells were all empty, because the parser skipped lines that were blank after trimming; a
  one-row `MIN(score)` over zero matches was read as zero rows. The parser now skips only empty
  lines. Both results are kept: the first run gave 1,193 of 2,000 as text and 1,239 with numbers
  compared as numbers; the corrected run is in row 1b. The P1 runner had the same line and has the
  same fix; P1 results below are from after it.

## Baseline (qdb at the move, 2026-11-08, buildc 1.11.0)

These are the runs on the engine as it arrived (build-universe 4a296c1, here d39dfc1), with only
the fault hook added. Later rows record the runs after each change.

- **P1 sqllogictest:** 32 of 9,202 query records pass (0.35 percent), 60 wrong, 9,110 rejected;
  statements 134 of 1,993 as expected; 5 records skipped by conditionals. SQLite self-check: 9,198
  of 9,202 (the four A1 records). Planted check caught. Prediction (under 5 percent) held. The run
  before the A2 parser fix gave 18 passes and 74 wrong; the 14 it missed are rows of all-NULL cells. The
  rejections, by first error: 6,978 "expected keyword" (mostly an expression in the select list),
  1,385 "unexpected trailing input", 581 "table not found" (setup refused: `VARCHAR(30)`,
  `PRIMARY KEY`, `UNIQUE`, views), 58 arithmetic in WHERE, 53 other parse, 19 DISTINCT outside
  COUNT, 36 "no such column" on a literal in the select list.
- **P2 SQLite differential:** 2,000 queries; 1,205 agree as text (60.2 percent), 1,251 with numbers
  compared as numbers (62.5 percent). Prediction (at least 95 percent) missed. Classes: 519
  null_vs_empty, 46 number_text, 230 rows. The examples of the rows class that were inspected are NULL semantics too:
  `NOT name = 'cy'` keeps a NULL name in qdb (SQL's three-valued logic drops it), and
  `COUNT(name)` skips `''` in qdb (SQL counts it). Number_text is REAL rendering: qdb prints `81`
  where SQLite prints `81.0`. Planted check caught.
- **P3 crash recovery:** 18 steps, 104 crash points, 24 violations. Both predictions held:
  (a) a crash after the `.db` rewrite and before the WAL clear replays the WAL a second time and
  duplicates rows (11 of 16 such points fail; the 5 that pass replay statements that change
  nothing twice); (b) a torn `.db` rewrite loses rows checkpointed earlier (13 of 16 points fail).
  Every WAL append, torn or not, recovers by the rule (56 of 56). Planted check caught.
- **P4:** the ROLLBACK case leaves one row (xfail `xproc_rollback`).

## Stage 1: a correct, durable single-file engine

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 1a Regression gate | `bdb/tests/test_qdb.sh` reports any FAIL | PASS | 214 pass, 0 fail, 1 xfail at the move with buildc 1.11.0 (buildlang 3c7cc54), 2026-11-08 |
| 1b SQLite differential | any of the P2 queries disagrees, with numbers compared as numbers | FAIL | baseline 1,251 of 2,000 (62.5 percent); `bdb/tests/test_qdb_sqlite_oracle.py` |
| 1c Crash recovery | any P3 crash point breaks the rule | FAIL | baseline 24 violations of 104 points; `bdb/tests/test_qdb_crash.py` |
| 1d Cross-process transaction | either P4 case misses | FAIL | documented null `xproc_rollback` |
| 1e Text round trip | any P5 value changes | FAIL | `'x|y'` reads back as `x`; no escaping in the file or WAL format |
| 1f NULL apart from '' | `INSERT (NULL)` and `INSERT ('')` are indistinguishable by `IS NULL`, or `NOT (NULL = x)` is true | FAIL | both store an empty cell; 519 P2 disagreements are this class directly, and the inspected examples of the 230 in the rows class are NULL semantics too |
| 1g JOIN gaps that fail closed | a secondary ORDER BY key over a grouped JOIN, DISTINCT over a grouped JOIN, or COUNT(DISTINCT) over any JOIN is refused | FAIL | each refused with a specific message (`bdb/README.md`, Known limitations) |

## Stage 2: SQL coverage against the outside corpus

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 2a sqllogictest queries | any P1 query record does not pass | FAIL | baseline 32 of 9,202 (0.35 percent); `bdb/tests/slt_run.py` |
| 2b Expressions in the select list | `SELECT a+1, a*b-c FROM t` refused or wrong | FAIL | refused: "expected keyword 'FROM'"; the largest single cause of P1 rejections |
| 2c Column types with arguments and constraints | `VARCHAR(30)`, `INTEGER PRIMARY KEY`, `UNIQUE` refused | FAIL | 581 P1 rejections follow from refused setup |
| 2d CASE, scalar functions (abs, coalesce) | refused or wrong | NOT BUILT | |
| 2e Correlated subqueries, EXISTS | refused or wrong | NOT BUILT | |

## Stage 3: scale

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 3a Bulk load is linear | `bench_qdb_load.py` log-log slope k above 1.3 for autocommit or transactional indexed load | PASS | HarperZ9/build-universe#35: autocommit k 1.16, transactional indexed k 1.02. Limit stated there: interleaved INSERT plus indexed SELECT stays superlinear (k 1.52) |
| 3b ORDER BY is n log n | P6 ratio 30k/10k above 6 | NOT BUILT | measurement pending |
| 3c Whole-program store of 10^5 rows | a `bql export` store of 100,000 rows loads and answers a GROUP BY in under 10 s | NOT BUILT | |

## Stage 4: an embeddable engine

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 4a Library entry point | a C program linked against a static library built from qdb's sources runs N statements on one open database in one process, and its output differs from the CLI's on the same statements | NOT BUILT | the C ABI (`extern "C"`, `buildc build --emit staticlib`) exists in buildlang 1.11.0 |
| 4b Many queries per open | the compiler side runs more than one query without a process per statement | NOT BUILT | depends on 4a |

## Stage 5: BuildQL over programs

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 5a Export, query, verify | `cargo test bql` fails | PASS (3c7cc54) | 15 bql tests pass, `in_process_subset_matches_qdb` included with `QDB_BIN` set; 35 of 35 sampled renames agree (2026-11-04) |
| 5b `refs` table | resolved references are missing from the store, or a planted wrong span is not caught by `bql verify` | NOT BUILT | |
| 5c Multi-file programs | a program with `mod name;` or `use` is refused by `bql export` | FAIL | refused by version 0 |
| 5d Symbol text with `|` or line breaks | `bql export` refuses a cell | FAIL | refused because qdb's format has no escape; closes with 1e |
| 5e Transforms beyond rename | extract, inline or effect-row transforms missing | NOT BUILT | |
| 5f qdb in the compiler's CI | qdb is not built and tested beside buildc (design milestone H2) | PASS (on merge of the move) | `.github/workflows/bdb.yml` builds qdb with the checkout's buildc and runs every gate above that is in CI |

## Stage 6: migration at the click of a button

The author, 2026-11-02: "automatically migrate everything at the click of a button using
BuildDB/BuildQL." The design is "Migration at the click of a button" in
`docs/design/BUILDDB-BUILDQL.md` and `LANGUAGE-MIGRATION.md`. This stage waits on stages 1 to 5;
nothing in it is built ahead of the core gaps.

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 6a One command | `buildc migrate SRC --from LANG` does not exist, or exits 0 on a source it did not fully translate | NOT BUILT | |
| 6b No silent drop | a construct the frontend cannot translate is missing from the `untranslated` table (checked by a planted untranslatable construct per frontend) | NOT BUILT | |
| 6c Behaviour kept | the migrated build and the original, run on the same inputs, differ on any output in the frontend's differential corpus | NOT BUILT | |
| 6d One button | the editor command calls anything other than the same entry point, or hides the receipt | NOT BUILT | |

## Where qdb lives

Decided by the author on 2026-11-09: "Move into BuildLang". qdb is `bdb/` in this repository, with
its history, and `.github/workflows/bdb.yml` builds it with the compiler of each checkout, so a
codegen regression that breaks qdb fails the compiler's own PR. Open: `bdb/` keeps Build
Universe's FSL-1.1-MIT licence (`bdb/LICENSE`, `NOTICE`); relicensing it to AGPL-3.0-only is the
author's call.
