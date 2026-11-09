# What finished means for BuildDB and BuildQL

Written 2026-11-08. BuildDB is qdb, the SQL engine in `programs/db.bld`. BuildQL is the SQL
dialect qdb runs, plus the compiler side in HarperZ9/buildlang (`compiler/src/bql/`, `buildc
bql export|query|rename|verify`), which writes a compiled program into qdb's file format. The
design is `docs/design/BUILDDB-BUILDQL.md` in buildlang. This file defines when the pair is
finished, orders the work into stages, and gives every row a check that can fail.

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

## Stage 1: a correct, durable single-file engine

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 1a Regression gate | `programs/tests/test_qdb.sh` reports any FAIL | PASS | 214 pass, 0 fail, 1 xfail on 4a296c1 with buildc 1.11.0 (buildlang 3c7cc54), 2026-11-08 |
| 1b SQLite differential | any of the P2 queries disagrees | NOT BUILT | harness `test_qdb_sqlite_oracle.py` to be written |
| 1c Crash recovery | any P3 crash point breaks the rule | NOT BUILT | fault hook and `test_qdb_crash.py` to be written; the existing `test_qdb_wal_recovery.py` covers torn WAL tails only |
| 1d Cross-process transaction | either P4 case misses | FAIL | documented null `xproc_rollback` |
| 1e Text round trip | any P5 value changes | FAIL | `'x|y'` reads back as `x`; no escaping in the file or WAL format |
| 1f NULL apart from '' | `INSERT (NULL)` and `INSERT ('')` are indistinguishable by `IS NULL` | FAIL | both store an empty cell |

## Stage 2: SQL coverage against the outside corpus

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 2a sqllogictest queries | any P1 query record does not pass | NOT BUILT | runner `slt_run.py` to be written |
| 2b Expressions in the select list | `SELECT a+1, a*b-c FROM t` refused or wrong | FAIL | refused: "expected keyword 'FROM'" |
| 2c CASE, scalar functions (abs, coalesce) | refused or wrong | NOT BUILT | |
| 2d Correlated subqueries, EXISTS | refused or wrong | NOT BUILT | |

## Stage 3: scale

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 3a Bulk load is linear | `bench_qdb_load.py` log-log slope k above 1.3 for autocommit or transactional indexed load | PASS | #35 (d3a71d8): autocommit k 1.16, transactional indexed k 1.02. Limit stated there: interleaved INSERT plus indexed SELECT stays superlinear (k 1.52) |
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
| 5a Export, query, verify | `cargo test bql` in buildlang fails | PASS (buildlang main) | `compiler/src/bql/tests.rs`, `drift_tests.rs` (35 of 35 sampled renames agree, 2026-11-04) |
| 5b `refs` table | resolved references are missing from the store, or a planted wrong span is not caught by `bql verify` | NOT BUILT | |
| 5c Multi-file programs | a program with `mod name;` or `use` is refused by `bql export` | FAIL | refused by version 0 |
| 5d Symbol text with `|` or line breaks | `bql export` refuses a cell | FAIL | refused because qdb's format has no escape; closes with 1e |
| 5e Transforms beyond rename | extract, inline or effect-row transforms missing | NOT BUILT | |
| 5f qdb in the compiler's CI | qdb is not built and tested beside buildc (design milestone H2) | NOT BUILT | waits on the home decision below |

## Open question: where qdb lives

The design proposes moving qdb into the buildlang repository as `bdb/` "after the author decides".
It has not moved. The tradeoff: in buildlang it would be built and tested by every compiler change
(a codegen regression would fail the compiler's own CI, and `bql` could link it), at the cost of
a larger compiler repo and CI, and of separating qdb from the Build Universe programs that share
its history. Kept here, it stays pinned to a released buildc, and compiler regressions surface
only when the pin moves.
