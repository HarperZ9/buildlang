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

**P7. NULL apart from the empty string (row 1f), written 2026-11-08 21:19 PT before the change.** qdb stores SQL
NULL as its own value, distinct from `''`: `IS NULL` matches only NULL; a comparison, LIKE, IN or
BETWEEN with a NULL operand is unknown, NOT of unknown is unknown, and WHERE and HAVING keep a
row only when the condition is true (SQL's three-valued logic); COUNT(col), SUM, AVG, MIN and MAX
skip NULL, and SUM, AVG, MIN and MAX over no non-NULL value are NULL; ORDER BY puts NULL first
ascending and last descending, as SQLite does; a LEFT JOIN null-extended column and a column left
out of an INSERT or added by ALTER TABLE are NULL; query output renders NULL as `NULL` and `''` as an
empty cell. The file format becomes 2: a NULL cell is written `\N`, an empty cell is `''`; a
format 0 or 1 file loads its empty cells as NULL, which is what they meant there.
Measured on the P2 harness unchanged (seeds 1 to 10, 2,000 queries), against the baseline 1,251
of 2,000 re-run on this branch before the change. Prediction: agreement with numbers compared as
numbers rises to between 1,800 and 1,960; null_vs_empty falls from 519 to 0; the disagreements
left are mostly AVG, which qdb renders with two decimals (`float_to_str`), a separate gap. The P2
bar (every query agrees) stays FAIL if any remain. Guards, each must hold after the change:
test_qdb.sh has no FAIL (assertions that encoded NULL as a blank cell are rewritten to the SQL
answer and listed in the commit); test_qdb_text.py 2,000 of 2,000; test_qdb_crash.py 0
violations; a format 1 file with an empty cell loads it as NULL; a format 2 file keeps NULL and
`''` apart through a checkpoint and through WAL replay. Planted check: a build whose `IS NULL`
also matches `''` must lose at least 20 agreements on P2. P1 (sqllogictest) is re-run and
reported; no prediction beyond "not lower than 32".

**P8. AVG and aggregate number rendering, written 2026-11-08 21:36 PT before the change.** Found by the P7
run: every one of the 56 P2 disagreements left is AVG. Two faults. `sum_val / cnt` divides a
float by an `i32` count, and the C backend lowers it to `bl_idiv_i32`, an integer division, so
AVG(60.3) over one row prints 60; and `float_to_str` rounds every aggregate to two decimals,
so AVG(22.1, 22.1, 22.15) could never print SQLite's digits. Change: divide by the count as
`f64`, and render SUM, AVG, MIN and MAX of numbers with the runtime's shortest round-trip
digits (`to_string_f64`); integral values keep printing without a decimal point (`81`, where
SQLite prints `81.0`, the number_text class). Measured on the P2 harness unchanged against the
P7 result 1,944 of 2,000. Prediction: between 1,990 and 2,000 agree with numbers compared as
numbers, and 0 AVG disagreements; SQLite 3.49 sums with Kahan-Babuska compensation and qdb
sums plainly, so last-digit differences are possible and land in number_text, not rows.
Guards: test_qdb.sh has no FAIL (assertions that encoded two-decimal rounding are rewritten and
listed); differential against the reference engine with `--no-null` (its AVG and SUM print two
decimals, so any check that differs only there is reported, not hidden); text, crash, WAL and
library tests unchanged. Planted check: a build that keeps the integer division must lose at
least 40 agreements on P2. The compiler fault (float divided by `i32` lowered as integer
division) is filed as a BuildLang issue; qdb works around it with an explicit cast.

**P9. sqllogictest setup and literal-first comparisons (rows 2a, 2c), written 2026-11-08 21:47 PT before
the change.** Two parser gaps block whole sqllogictest files. `CREATE TABLE t1(..., x1
VARCHAR(30))` is refused at the `(` after the type, so `select4.test` and `select5.test` lose
their tables and every later record (1,729 statements and 3,564 queries); and a comparison with
the literal first, `WHERE 272=e3`, is refused because a condition must start with a column.
Change: a column type may carry arguments, `(n)` or `(p, s)`, which are read and dropped (the
type keeps its storage class); a comparison `literal op column` is read as `column op' literal`
with `<` and `>`, `<=` and `>=` swapped. A literal on both sides stays refused. Measured on P1
unchanged against the P8 result. Prediction: statements as expected rise from 134 to at least
1,800 of 1,993; query records passing rise from 33 to between 45 and 300, since most of the
unblocked queries still use INTERSECT, EXCEPT, expressions in the select list or more than two
tables in FROM, which qdb refuses. Guards: test_qdb.sh no FAIL; P2 2,000 of 2,000; text, crash,
WAL, library and reference-differential tests unchanged. Planted check: a build that reads
`5 < a` as `a < 5` (no swap) fails the new test_qdb.sh assertions.

**P10. INTERSECT and EXCEPT (row 2a), written 2026-11-08 21:58 PT before the change.** Found by the P9 run:
878 of the 2,832 query records in `select4.test` combine single-table branches with INTERSECT or
EXCEPT, which qdb refuses (`expected keyword`). Change: a compound SELECT takes UNION, UNION ALL,
INTERSECT and EXCEPT, evaluated left to right with equal precedence, as SQLite does; INTERSECT
and EXCEPT return distinct rows. Rows compare by every cell, NULL equal to NULL, as SQLite's
compound operators do. The row key that UNION used joined cells with `|`, so `('a|b','c')` and
`('a','b|c')` were one row; the key now uses a separator no cell holds. Measured on P1 unchanged
against the P9 result 199 of 9,202. Prediction: between 500 and 1,100 query records pass; most
of the 878 have no ORDER BY and are scored by the corpus's own sort mode. Guards: test_qdb.sh no
FAIL; P2 2,000 of 2,000; text, crash, WAL, library and reference-differential tests unchanged.
Planted check: a build that evaluates EXCEPT as INTERSECT fails the new test_qdb.sh assertions
and passes fewer P1 records.

**P11. Expressions in the select list and WHERE (row 2b), written 2026-11-09 00:27 PT before the
change.** After P10 the largest rejection class is the expression: 949 of the 1,000 queries in
`select1.test` put arithmetic in the select list, and most also use CASE, `abs`, a column
compared with a column, a correlated scalar subquery or EXISTS. A column compared with a column
(`WHERE c > d`) is worse than refused today: the condition parser reads `d` as the text 'd'.
Change: a second SELECT path parses an expression tree (arithmetic with SQLite's integer and
real rules, comparisons, AND, OR, NOT with three-valued logic, IS NULL, BETWEEN, IN, LIKE,
searched and simple CASE, `abs`, `coalesce`, `ifnull`, `nullif`, aggregates without GROUP BY,
scalar and EXISTS and IN subqueries that may name the outer query's columns, ORDER BY over any
number of keys, DISTINCT, LIMIT and OFFSET), and runs it over one or more FROM tables. A token
scan picks the path before either parser runs: the new path takes a SELECT that uses one of these
forms and none of GROUP BY, HAVING, JOIN or a compound operator; every other SELECT keeps the
older path and its answer. Measured on P1 unchanged against the P10 result 1,084 of 9,202.
Prediction: between 2,500 and 6,000 query records pass, the bulk from `select1` to `select3`;
wrong answers stay under 2 percent of the records the new path answers. Guards: test_qdb.sh no
FAIL; P2 2,000 of 2,000 (its generator uses only forms the older path keeps, so P2 also checks
that the path choice leaves them alone); text, crash, WAL, library and reference-differential
tests unchanged. Planted check: a build whose integer division returns a real (no truncation)
passes fewer P1 records and fails a new test_qdb.sh assertion.

**P12. Join order for comma-separated FROM lists, written 2026-11-09 01:15 PT before the change.**
Found by the P11 run (A6): `select5.test` joins up to 64 tables, and looping in FROM order builds
cross products of tables no conjunct has linked yet. Change: before the loop, the tables are
ordered greedily. Each step takes the unplaced table with an equality conjunct to a placed table,
then one with a conjunct on itself alone (`a29 = 6`), then the first in FROM order; each conjunct
is checked at the step where its last table is bound. The result rows and their order do not
change: ORDER BY sorts after, and a query without ORDER BY is compared by the corpus's sort mode.
Measured on P1 unchanged, all 17 files, against P11 (16 files, `select5.test` at 0). Prediction:
`select5.test` passes at least 650 of its 732 queries; no query over 60 s; the other 16 files keep
their P11 counts exactly. Guards: test_qdb.sh no FAIL, P2 2,000 of 2,000, the other tests
unchanged. Planted check: a build that keeps FROM order times out on `select5.test` (fewer than
100 passes inside the 60 s limit on its first 100 queries).

**P13. PRIMARY KEY, UNIQUE and NOT NULL enforced (row 2c), written 2026-11-09 01:59 PT before the change.**
`PRIMARY KEY` is parsed and dropped, so `INSERT INTO t VALUES (1), (1)` into a key column keeps
both rows, and `UNIQUE` is refused. Change: column constraints `PRIMARY KEY`, `UNIQUE` and
`NOT NULL`, and table constraints `PRIMARY KEY (a, b)` and `UNIQUE (a, b)`, are stored with the
table, written to the file as `PRIMARY KEY a,b`, `UNIQUE a,b` and `NOT NULL a` lines after
SCHEMA (an older reader skips them), restored by ROLLBACK, and checked by INSERT and UPDATE
before any cell changes: a duplicate key prints `Error: UNIQUE constraint failed: t.a` and the
statement changes nothing. NULLs in a UNIQUE key are distinct, as in SQLite; a NULL inserted
into a single `INTEGER PRIMARY KEY` column takes the largest key plus one, as SQLite's rowid
alias does. Measured on P1 unchanged against P12. Prediction: query records unchanged at 8,921
(no query in the corpus depends on a rejected duplicate); statements as expected rise from 1,918
by 0 to 30, from `statement error` records that expect a constraint failure. Guards: test_qdb.sh
no FAIL; P2 2,000 of 2,000; text, crash, WAL, library and reference differential unchanged; bulk
load slope (row 3a) is unchanged for a table with no constraint. Planted check: a build that
skips the check on UPDATE fails a new test_qdb.sh assertion.
**P14. Atomic replace and fsync (scoreboard target 2, issue #166), written 2026-11-09 03:14 PT before the
change.** The checkpoint wrote the whole state twice (`<db>.ckpt` with a completion line, then
`<db>`), and nothing was flushed, so a power cut could lose committed rows. With the new
`rename_file` and `fsync_file` builtins, a checkpoint writes `<db>.tmp`, flushes it and renames
it over `<db>`; a commit flushes the WAL after appending it. Both the flush and the rename are
crash points of the fault hook, so `test_qdb_crash.py` stops the process before each. A complete
`.ckpt` left by an older build still wins at load and is cleared at the next checkpoint.
`QDB_SYNC=off` skips the flushes (the rename stays). Measured on `test_qdb_crash.py` unchanged:
prediction 0 violations, at a different number of crash points (each statement gains a WAL
flush; each checkpoint swaps two `.ckpt` writes for a flush and a rename), between 150 and 260.
Bytes written per checkpoint fall by half (one copy of the state instead of two), measured with
`QDB_TRACE_WRITES=1` on the crash workload. The cost of flushing: `bench_qdb_load.py` at 1,000,
4,000 and 16,000 rows, reported with and without `QDB_SYNC=off`; no bound is set, the number is
reported. Guards: test_qdb.sh no FAIL, P1 8,921, P2 2,000, text, WAL, library and reference
differential unchanged. Planted check: a build that renames before the `.tmp` write completes
(rename, then write) shows violations in the crash test. Power loss itself (dropped or reordered
unflushed writes) is not simulated here; that is the seeded-fault work that follows.
**P15. Seeded power cuts (scoreboard target 2), written 2026-11-09 04:10 PT before any run.** A new
harness, `bdb/tests/test_qdb_faults.py`, simulates a power cut in the operating system's file cache,
seeded, with no change to the engine. It learns each step of the `test_qdb_crash.py` workload (18
steps) by stopping the step at every durable operation (`QDB_CRASH_AT=k`) and comparing the files
before and after: a whole-file write, an append at an offset, a flush of one file, or a rename. A
seed then picks a step, a cut point c in 0..n (operations 1..c were issued; c = n is a cut after the
process exited and before the cache was written back), and for every issued write that no flush of
its file follows before the cut: dropped (probability 1/2) or kept, and a kept write is torn with
probability 1/4 (a seeded prefix of its bytes lands). Kept writes land in issue order at their
recorded offsets; a range left by a dropped earlier append is zero-filled, which is the outcome of a
later write reaching the device before an earlier one (reordering). Flushed writes always land. A
completed rename always lands (the runtime's `rename_file` flushes the directory on Linux and
writes through on Windows). Each step starts from a fully written-back image of the state before it,
so a cut never loses an earlier step's unflushed writes; that is a stated limit of this model. One
qdb process then opens the image, issues ROLLBACK, reads both tables and quits. Rule: the process
exits 0, and the state read equals the state after the step if a commit-tagged write (`wal_auto`,
`wal_commit`) was followed by a `wal_sync` before the cut (the commit was acknowledged as durable),
and equals the state before or after the step otherwise. Determinism: the sha256 of every file left
after recovery (`.db`, `.wal`, `.tmp`, `.ckpt`), per seed, and one digest over seeds 1 to 10,000.
The same generated `qdb.c` is compiled on Windows (gcc, MinGW) and on Linux (WSL Ubuntu 24.04,
gcc), and CI on ubuntu-latest checks the digest against the one committed from the Windows run.
Predictions: 0 violations of 10,000 seeds; the per-seed hashes equal on Windows and Linux for 10,000
of 10,000. Planted checks: (a) with `--plant-ignore-flush` every unflushed and flushed write alike
may be dropped, and the harness must report violations (acknowledged commits lost), or it cannot see
a lost commit; (b) one per-seed hash altered in the Linux manifest must be reported as a difference.
Cost (seeds per second on each platform) is reported; no bound.
**P16. History, `AS OF` and receipts (scoreboard target 1), written 2026-11-09 05:20 PT before the
change.** A database keeps an append-only history when `<db>.hist` exists. The first process run
with `QDB_HISTORY=1` creates it: a `BASE <txid>` line, and when the database already holds tables,
a copy of its state as `<db>.hist.base`. Every transaction that commits after that is appended to
the history as the same `TXID`, OP and `COMMIT <hash>` record the WAL holds, after the WAL flush; a
checkpoint flushes the history before it replaces the `.db`, and WAL replay appends the
transactions it recovers, so a record lost from the history's cache comes back from the WAL. The
history reader keeps the first record per txid whose hash matches and skips the rest.
`SELECT ... AS OF <txid>` rebuilds the state at that txid from the base and the history in a
separate in-memory database and runs the SELECT there. With `QDB_RECEIPTS=1` every SELECT result
is followed by `RECEIPT <txid> <state> <query> <result>`: the txid the state reflects, and SHA-256
of the state (the `.db` text without its header line), of the query (without the `AS OF` suffix)
and of the printed result lines. A mutation that fails (constraint, missing table) no longer
reaches the WAL, so neither replay nor history runs it again. Benchmark: a new
`bdb/tests/test_qdb_receipts.py` on the P2 workload, seeds 1 to 5 (1,000 SELECTs), with a seeded
mutation (UPDATE, DELETE or INSERT on `a` or `b`) after every 10 queries. Process A runs each seed
in one REPL with history and receipts on; process B, started after A exits, runs every query as
`<query> AS OF <txid from A's receipt>`. A query is reproduced when B's receipt line and result
lines equal A's. Prediction: 1,000 of 1,000. Overhead: the same 1,000 queries against each seed's
final state in one REPL per seed, receipts off and on, median of 5 runs; the bar is under 5
percent, and the prediction is 2 to 15 percent (SHA-256 is written in BuildLang and hashes every
result byte). The cost of `AS OF` (B's time against A's) and of history on a write load
(`bench_qdb_load.py` with and without `QDB_HISTORY=1`) is reported with no bound. Planted checks:
(a) the harness alters one of A's receipts and must count it as not reproduced; (b) one UPDATE in
the history file gets another value and a recomputed hash (a history rewritten consistently), and
B must then fail to reproduce every receipt at or after that txid that reads the changed row,
with the count reported. Guards: test_qdb.sh no FAIL; crash test 0 violations; P15 0 violations
with the digest unchanged (history is off by default); P2 2,000; text, WAL, library and reference
differential unchanged. Limits stated in advance: the crash and power-cut tests do not run with
history on; `AS OF` replays the history from the base on every query.

**P17. Constraint checks through a key map (scoreboard target, item 3), written 2026-11-09 06:05 PT
before the change.** P13 checks a new row against every row of its table and an UPDATE of a key
column compares every pair of rows, so a keyed bulk load is quadratic and one key UPDATE is
quadratic in the table. The change keeps one hash map from canonical key text (constraint number,
then each key cell as a number in shortest form or as text, length-prefixed) to presence, plus the
largest key of each `INTEGER PRIMARY KEY` for numbering NULLs. INSERT checks and extends it; any
other mutation, ROLLBACK and a new constraint mark it stale, and the next INSERT rebuilds it in one
pass. An UPDATE of a key column checks the table through a fresh map in one pass. NULL keys stay
distinct, and two keys equal as numbers (`1` and `1.0`) stay equal, as in P13. Benchmark: two new
scenarios in `bench_qdb_load.py`: `txn_keyed` loads N rows into `(id INTEGER PRIMARY KEY, name
TEXT UNIQUE, age INTEGER)` inside one transaction; `txn_keyed_update` does the same and then one
UPDATE of the `name` column. N = 1,000, 2,000, 4,000 and 8,000, before (main ee630a1) and after.
Predictions: before, exponent k of 1.7 or more for both; after, k at most 1.3 for both, and 8,000
rows at least 5 times faster than before. Guards: test_qdb.sh no FAIL (P13's thirteen constraint
checks included); P1 statements 1,931 of 1,993 unchanged and queries 8,921; P2 2,000; crash 0; P15
digest unchanged; text, WAL, library and reference differential unchanged. Planted checks: a build
whose map is never marked stale fails a new test_qdb.sh check (DELETE a keyed row, insert its key
again: must succeed); a build that compares key cells as text fails another (`1.0` after `1` in an
INTEGER PRIMARY KEY column: must fail).

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

- **A3 (2026-11-08 18:40, after the first P5 runs).** Two harness faults, both in
  `test_qdb_text.py`, were fixed before any result was recorded: the Windows C runtime writes each
  line feed of qdb's output as CR LF, which the harness now undoes (the generated values hold no CR
  LF pair, so the translation is exact); and the WAL case had sent the rows in 50-row chunks, where
  the second process's own startup replay is a checkpoint that the crash point then hit. The WAL
  case now sends one INSERT of all 1,000 rows and stops at its second durable write. The comparison
  was also tightened to exact text (an earlier draft ignored trailing spaces).
- **A4 (2026-11-08 18:55, before the P6 run).** The first P6 build of the 10,000-row table loaded
  nothing: qdb's REPL read a 6 KB INSERT in 4,095-byte pieces, because the runtime's `read_line`
  made one `fgets` call into a 4 KB buffer. That is a compiler runtime fault, fixed in this branch
  (`compiler/tests/read_line_long.rs`); P6 ran on the fixed runtime.
- **A5 (2026-11-08 21:35, before the P1 run after P7).** Since P7 qdb prints NULL as `NULL` and `''` as an
  empty cell. `slt_run.py` read every empty qdb cell as NULL, which was right for the engine before
  P7 and wrong after it. It now reads `NULL` as NULL and an empty cell as `''`; `--empty-is-null`
  keeps the old reading for an engine from before P7, and the baseline is re-run with it. The
  differential against the pre-rewrite reference engine (`test_qdb_differential.py`) gains
  `--no-null`, a workload with no NULL and no `''`, because that engine stored both as one empty
  cell; with NULL data it fails 8 of 16 checks by design, and without it 16 of 16 pass.
- **A6 (2026-11-09 01:15, during the P11 run, before its total).** The P11 run reached `select5.test`, whose
  queries join 4 to 64 tables in a comma-separated FROM list. The new path loops over the tables
  in FROM order and checks each conjunct once its tables are bound, so a FROM list that opens with
  unconnected tables builds their cross product: three sampled queries took 0.07 s, 0.25 s and
  over 120 s (stopped). At the runner's 60 s limit per record the file would take hours. The run
  was stopped in `select5.test`; P11 is reported for the other 16 files with `select5.test` counted
  at its P10 score (0 queries pass), and the planted check runs on the same 16 files. Ordering
  the joins is P12, which measures all 17 files again.
- **A7 (2026-11-09 04:40, after the first P15 run, before any change to the engine).** The P15 run
  on main 3aa051c found 2 violations in 10,000 seeds (9,351 and 9,570), the same on Windows and
  Linux. Both cut the explicit transaction step before its `wal_sync`: one OP record torn, a later
  record kept beyond a zero-filled hole, and the COMMIT record torn to `COMMIT` with no line break.
  Replay took the prefix as a commit and ran the torn OP; the recovering process exited 1. The
  commit was never acknowledged, so the rule allowed the state before or after, and neither came
  out. The fix, P15b: a commit record becomes `COMMIT <h>`, where h is the FNV-1a 32-bit hash, in
  8 hex digits, of the transaction's `TXID` line and each of its OP and OPX lines, each with its
  line break. Replay applies a transaction only when h matches; a bare `COMMIT` from an older
  build counts only when its line break survived. Prediction: 0 violations in seeds 1 to 10,000,
  hashes still equal on Windows and Linux. Planted check: a build that accepts any line starting
  with `COMMIT` (the old rule) shows the violations again on seeds 1 to 10,000. Guards: test_qdb.sh
  no FAIL, crash test 0 violations, P2 2,000, text, library and reference differential unchanged.
  `test_qdb_wal_recovery.py` truncates the WAL at every byte; its reference model now counts a
  COMMIT line only when the whole record survived, and its comparison with the reference engine
  skips offsets that cut inside a COMMIT record, where the reference engine accepts the torn
  prefix (the behaviour this fix removes).
- **A8 (2026-11-09 05:30 PT, after the P16 results, before P16b runs).** P16 missed its overhead bar:
  receipts cost 44 percent of query time on Linux and 347 percent on Windows, with SHA-256 written
  in BuildLang. P16b swaps that function for the runtime's `String.sha256()` (buildlang #192, a C
  implementation) and changes nothing else; the receipt bytes are the same SHA-256 digests.
  Measured on the P16 benchmark unchanged. Predictions: 1,000 of 1,000 still reproduced, receipt
  lines byte-identical to the P16 build on the same workload; overhead 3 to 12 percent on Linux
  (a small query costs about 50 microseconds and its result is a few hundred bytes); Windows
  reported, no prediction (its output path costs per byte). Planted checks as in P16. Guards:
  test_qdb.sh no FAIL, P15 digest unchanged, P2 2,000.
- **A9 (2026-11-09 05:50 PT, during P16b exploration, before its full runs).** With the runtime
  SHA-256 alone, a 2-seed pilot on Windows still showed 256 percent. A build that printed a fixed
  receipt and hashed nothing showed 78 percent, so the cost was the print: the receipt was one
  `println!` with four arguments, which the C backend lowers to one `printf` with four `%s`
  conversions, and on Windows (MinGW, unbuffered stdout) that costs several writes. P16b now builds
  the receipt line as one string and prints it with one argument; the bytes printed are the same.
  The 2-seed pilot of that build gave 3.5 percent on Windows. The full P16b runs (5 seeds, 5
  repetitions, Windows and Linux) cover both changes; the A8 predictions stand.

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

## After the changes (2026-11-08, buildlang branch feat/bdb-completion)

Built with buildc from the same branch (1.11.0 plus the `read_line` fix), gcc 13.2, Windows 11.

- **P3 crash recovery:** 0 violations of 170 crash points (18 steps; more points than the baseline
  because a checkpoint now has four writes). Mutants: replay without the txid skip, 30
  violations; the `.ckpt` ignored at load, 14.
- **P4:** BEGIN, INSERT and ROLLBACK as three processes leave the table empty; with COMMIT, one row.
  The differential test's process mode now equals the same build's REPL run on all 8 seeds; the
  build before the change fails it on 3 of 3 seeds tried.
- **P5 text:** 1,000 of 1,000 values kept through the `.db` and 1,000 of 1,000 through the WAL
  alone. Before: 142 of 1,000 and 0 of 1,000. A mutant that writes cells unescaped changes 1,760
  of 2,000.
- **P6 ORDER BY:** before, net 0.027 s, 2.98 s and 28.3 s at 1k, 10k and 30k rows (30k/10k ratio
  9.51, slope 2.05; prediction above 6 held). After the merge sort: 0.015 s, 0.048 s and 0.296 s
  at 10k, 30k and 100k (30k/10k ratio 3.29; slope 1.08, then 1.51 to 100k, cause not measured).
- **P1 and P2** are unchanged by these changes (32 of 9,202; 1,251 of 2,000): none of them touched
  the dialect or NULL handling.

## After P7: NULL apart from '' (2026-11-08, buildlang branch feat/bdb-null)

Same compiler build (1.11.0 at the rebased completion branch, plus the `read_line` fix), the P2
harness unchanged. Before: 1,251 of 2,000 on this branch, re-run before the change.

- **P2 SQLite differential:** 1,944 of 2,000 agree with numbers compared as numbers (97.2
  percent; 1,880 as text). Classes: null_vs_empty 519 to 0; rows 230 to 56; number_text 46 to 64.
  Prediction (1,800 to 1,960) held. Every one of the 56 left is AVG: qdb divided AVG as integers
  and printed two decimals, which P8 takes up. Planted check: a build whose `IS NULL` also
  matches `''` scores 1,829 (115 fewer; the bar was 20).
- **P1 sqllogictest** (A5): 33 of 9,202 query records pass, 59 wrong, 9,110 rejected (before,
  re-run with `--empty-is-null`: 32, 60, 9,110). Statements 134 of 1,993 in both.
- **Guards:** test_qdb.sh 241 pass, 0 fail (8 assertions rewritten to the SQL answer, 19 added);
  text round trip 0 values changed; crash 0 violations of 170; WAL recovery 0 failures; the C
  library 0 failures; bql's in-process check against qdb passes. The differential against the
  reference engine fails 8 of 16 with NULL data, by design, and passes 16 of 16 with `--no-null`.

## After P8: AVG and aggregate digits (2026-11-08, buildlang branch feat/bdb-avg)

- **P2 SQLite differential:** 2,000 of 2,000 agree with numbers compared as numbers (1,873 as
  text, 127 number_text: qdb prints an integral value as `81` where SQLite prints `81.0`, and a
  few sums differ in the last digit). Before: 1,944 (56 rows, all AVG). Prediction (1,990 to
  2,000, no AVG left) held. Planted check: a build that keeps the integer division scores
  1,944 (56 fewer; the bar was 40). The CI step becomes a gate.
- **P1 sqllogictest:** 33 of 9,202, unchanged.
- **Guards:** test_qdb.sh 244 pass, 0 fail (no assertion rewritten; three AVG checks added, which
  fail on the build before); text 0 changed; crash 0 of 170; WAL 0; library 0. The differential
  against the reference engine differs on 8 of 16 checks with SUM and AVG asked, as the
  preregistration said it would (that engine divides AVG as integers and rounds to two
  decimals), and passes 16 of 16 with `--no-null --no-float-agg`, which CI now runs.

## After P9: sqllogictest setup and literal-first comparisons (2026-11-08, feat/bdb-slt-setup)

- **P1 sqllogictest:** 199 of 9,202 query records pass (2.16 percent), 75 wrong, 8,928 rejected;
  statements 1,918 of 1,993 as expected. Before (P8): 33, 59, 9,110; statements 134. Prediction
  (statements at least 1,800; queries 45 to 300) held. By file: `select4.test` 0 to 133 queries
  and 0 to 1,019 statements; `select5.test` 0 to 704 statements, its 732 queries still rejected
  (comma-separated FROM lists of several tables, refused as trailing input). The rest of the gain is in `evidence/`. The largest rejection
  classes left are expressions in the select list (row 2b), INTERSECT and EXCEPT, and comma-separated FROM lists
  of more than two tables.
- **P2:** 2,000 of 2,000, unchanged.
- **Guards:** test_qdb.sh 250 pass, 0 fail (six checks added; a build that does not mirror the
  operator fails three); text 0 changed; crash 0 of 170; WAL 0; library 0; reference differential
  16 of 16 with `--no-null --no-float-agg`.

## After P10: INTERSECT and EXCEPT (2026-11-08, feat/bdb-compound)

- **P1 sqllogictest:** 1,084 of 9,202 query records pass (11.78 percent), 68 wrong, 8,050
  rejected; statements 1,918 of 1,993. Before (P9): 199, 75, 8,928. Prediction (500 to 1,100)
  held. `select4.test` passes 1,018 of its 2,832 queries with 0 wrong; the rest are rejected,
  mostly comma-separated FROM lists. Planted check: a build that evaluates EXCEPT as INTERSECT
  passes 304 (780 fewer) and answers 848 wrong.
- **P2:** 2,000 of 2,000, unchanged.
- **Guards:** test_qdb.sh 257 pass, 0 fail (seven checks added; the build before fails six of
  them, among them the `|` row-key collision); text 0 changed; crash 0 of 170; WAL 0; library 0;
  reference differential 16 of 16 with `--no-null --no-float-agg`.

## After P11: expressions (2026-11-09, feat/bdb-expr)

- **P1 sqllogictest** (A6: 16 files run, `select5.test` counted at its P10 score of 0): 8,186 of
  9,202 query records pass (88.96 percent), 9 wrong, 1,007 rejected (275 in the 16 files run plus
  `select5.test`'s 732). Before (P10): 1,084, 68, 8,050. `select1` to `select3` pass 5,320 of
  5,320; `select4` 2,829 of 2,832 (2,830 in an earlier run of the same build: one record sits near
  the 60 s limit). Statements 1,214 of 1,289 in the 16 files. Prediction (2,500 to 6,000) missed
  on the high side. Wrong answers are 9 of the 8,195 answered (0.11 percent; bar 2 percent): two in
  `slt_lang_aggfunc` (MIN over mixed text and numbers; `sum` overflow), seven in `slt_lang_replace`
  and `slt_lang_update` (statements qdb does not run: REPLACE, UPDATE forms), none on the new path.
- **Planted check, split result.** The build whose integer division returns a real fails two new
  test_qdb.sh checks (`x_int_div`, `x_agg_expr`): caught. On P1 it passes 8,187, one more than the
  real build: not caught. The runner prints an `I` column as `int(float(v))`, which truncates a
  real quotient to the integer one, so P1 cannot see this fault; the one-record difference is the
  record near the time limit. Kept as a null for P1's power over integer arithmetic.
- **P2:** 2,000 of 2,000, unchanged: none of its queries takes the new path.
- **Guards:** test_qdb.sh 274 pass, 0 fail (`where_arith_plus_closed` and `where_arith_pct_closed`
  asserted a refusal and now assert answers; 17 checks added); text 0 changed; crash 0 of 170;
  WAL 0; library 0; reference differential 16 of 16 with `--no-null --no-float-agg`.

## After P12: join order (2026-11-09, feat/bdb-joinorder)

- **P1 sqllogictest, all 17 files:** 8,921 of 9,202 query records pass (96.95 percent), 9 wrong,
  272 rejected; statements 1,918 of 1,993; the whole run takes 269 s. `select5.test` passes 732 of
  732 (P11: not run, counted 0); `select4.test` 2,832 of 2,832 (P11: 2,829, three records past the
  60 s limit). Prediction: `select5` at least 650, held; no query over 60 s, held; the other files
  unchanged, missed by those three `select4` records, which no longer time out.
- **Planted check, as preregistered: not separating.** On the first 100 `select5.test` queries
  the build that keeps FROM order also finishes all 100 inside 60 s; those queries join 4 to 12
  tables. Kept as a null for the check as written. A second look, not preregistered: every 7th
  query from 300 to 440 (21 queries, 28 to 40 tables) with a 30 s limit: FROM order finishes 0 of
  21 in 649 s, the ordered build 21 of 21 in 1 s.
- **P2:** 2,000 of 2,000. **Guards:** test_qdb.sh 277 pass, 0 fail (three comma-join checks added);
  text 0 changed; crash 0 of 170; WAL 0; reference differential 16 of 16.
- What is left in P1: 272 rejected and 9 wrong, all in `evidence/`: `IN ()` and literal-first
  `IN` forms, `DISTINCT` inside SUM and AVG, triggers, views, REINDEX, REPLACE and UPDATE forms
  qdb does not run.

## After P13: constraints (2026-11-09, feat/bdb-constraints)

- **P1 sqllogictest:** query records 8,921 of 9,202, unchanged (prediction held); statements as
  expected 1,918 to 1,931 of 1,993 (+13, all in `evidence/in1.test`, which creates tables with
  `UNIQUE` and `PRIMARY KEY`; prediction 0 to 30 held).
- **Planted check:** a build that skips the UPDATE check fails `update_dup` and `update_kept`:
  caught.
- **P2:** 2,000 of 2,000. **Guards:** test_qdb.sh 290 pass, 0 fail (thirteen checks added); text 0
  changed; crash 0 of 170; WAL 0; library 0; reference differential 16 of 16. Bulk load with no
  constraint (`bench_qdb_load.py`, 1,000 to 16,000 rows): exponent 1.00 and 1.05 before, 0.96 and
  1.02 after.
- Limits: the duplicate check scans the table (O(rows) per inserted row, O(rows^2) for an UPDATE of
  a key column); a constrained bulk load is quadratic until keys get an index. DEFAULT, CHECK and
  FOREIGN KEY are not read.

## After P14: atomic replace and fsync (2026-11-09, feat/bdb-atomic)

- **Crash test:** 194 crash points, 0 violations (170 and 0 before). Prediction (150 to 260
  points, 0 violations) held. Planted check: a build that renames first and then rewrites the
  live `.db` shows 14 violations at 166 points: caught.
- **Bytes per checkpoint:** a 500-row checkpoint wrote 17,674 bytes (`.ckpt` 8,849 plus `.db`
  8,825) and now writes 8,825 (`.tmp`), half as predicted.
- **The cost of flushing** (`bench_qdb_load.py`, 16,000 rows): one process per autocommit INSERT
  goes from 0.43 ms to 4.24 ms per row with the WAL flushed at each commit (exponent 1.00), and
  0.40 ms with `QDB_SYNC=off`; inside a transaction 0.33 to 0.35 ms either way. P1 takes 463 s
  instead of 269 s for the same 8,921 passes.
- **Guards:** test_qdb.sh no FAIL; P1 8,921 of 9,202; P2 2,000 of 2,000; text 0 changed; WAL 0;
  library 0; reference differential 16 of 16.
- Not covered: a power cut can drop or reorder writes that were never flushed; this run stops
  processes only. The seeded-fault simulation is next.

## After P15: seeded power cuts (2026-11-09, feat/bdb-faults)

- **First run, main 3aa051c (no engine change):** 10,000 seeds, 2 violations (seeds 9,351 and
  9,570; amendment A7), 38 distinct recovered images. The per-seed hashes are equal on Windows
  (MinGW gcc) and Linux (WSL Ubuntu 24.04, gcc) for 10,000 of 10,000, both from one generated
  `qdb.c`; digest `913f72f4...`. Prediction 0 violations: missed. Prediction equal hashes: held.
- **After P15b (the COMMIT record carries a hash):** 0 violations in 10,000 seeds, 35 distinct
  recovered images; Windows and Linux equal for 10,000 of 10,000, digest `cd3ccf9f...`, now a CI
  gate on ubuntu-latest. 97 durable operations learned over the 18 steps. Speed: 78 seeds per
  second on Windows, 232 on Linux (2 workers each).
- **Planted checks:** flushes ignored, 79 violations in 300 seeds: caught. One altered hash in
  the compared manifest: reported. The build that accepts any line starting with `COMMIT`: 1
  violation in 10,000 (seed 9,570): caught, at a lower count than the 2 of the first run.
- **Guards:** test_qdb.sh 290 pass, 0 fail; crash test 194 points, 0 violations; WAL truncation
  0 failures with the reference engine compared (no offset needed the COMMIT skip); text 0
  changed; library 0 failures; reference differential 16 of 16; P2 2,000 of 2,000.
- Limits: each step starts from a fully written-back image, so a cut never reaches back into an
  earlier step's unflushed writes; a lost range reads as zeros, never as stale bytes; the model
  treats a completed rename as durable. 10,000 seeds cover 35 to 38 distinct outcomes of one
  18-step workload, so the count of seeds overstates the variety.

## After P16b: receipts on the runtime SHA-256, printed as one string (2026-11-09)

- **Overhead of receipts: 13.0 percent on Linux, 5.7 percent on Windows** (0.050 s off, 0.057 s
  on; 0.202 s off, 0.214 s on; 1,000 queries, median of 5 per seed). P16: 44 and 347 percent.
  The bar (under 5 percent) is still missed on both; the A8 prediction for Linux (3 to 12
  percent) missed by 1 point. What is left is about 7 microseconds per query on Linux against a
  query of about 50: two SHA-256 passes (the result, a few hundred bytes, and the query text), the
  captured lines joined into one string, and the receipt line itself.
- **Reproduction:** 1,000 of 1,000 on Windows and Linux; receipt lines byte-identical to the P16
  build on the same workload (sha256 of process A's output equal). Planted checks (a) and (b)
  caught as in P16. **Guards:** test_qdb.sh 290 pass, 0 fail; P15 digest unchanged; P2 2,000 of
  2,000.
- Found on the way (A9): a `println!` with several arguments is one `printf` with several `%s`
  conversions, and on Windows with stdout unbuffered that cost about 0.3 ms per receipt line.
  Every multi-argument print in a BuildLang program pays it.

## After P16: history, AS OF and receipts (2026-11-09, feat/bdb-history)

- **Reproduction:** 1,000 of 1,000 results reproduced by a second process with `AS OF` (P2 seeds 1
  to 5, 200 queries each, a mutation after every 10 queries, so 21 states per seed), on Windows
  and on Linux (WSL Ubuntu 24.04). Prediction held.
- **Overhead of receipts: 44 percent on Linux, 347 percent on Windows. The bar (under 5 percent)
  and the prediction (2 to 15 percent) both missed.** Linux: 0.049 s for the 1,000 queries with
  receipts off, 0.071 s on (median of 5 per seed). Windows: 0.209 s off, 0.933 s on. These
  queries are small (about 50 microseconds each on Linux), and SHA-256 written in BuildLang costs
  about 6 microseconds per 64-byte block (every add is a checked call, every block allocates), so
  hashing a result of a few hundred bytes costs more than the query. The next step is a SHA-256
  builtin in the BuildLang runtime, measured again as P16b.
- **Cost of AS OF:** process B, which rebuilds the state from the history for every query, took
  0.51 s on Linux against 1.38 s for process A (which also ran the 62 setup statements and 100
  mutations, each flushed); 1.94 s against 2.61 s on Windows.
- **Cost of history on writes** (`bench_qdb_load.py`, Windows, WAL flushed): autocommit 3.97 to
  4.14 ms per row at 16,000 rows (4 percent), one transaction 0.72 to 0.75 ms per row. A first
  build kept the open transaction's text as one growing string, which made a 16,000-row
  transaction quadratic (0.72 ms per row against 0.34 on main, with history off); it now keeps
  parts, and the same load takes 0.338 ms per row (main 0.335).
- **Planted checks:** (a) one altered receipt: counted as not reproduced, caught. (b) history
  rewritten consistently at txid 63 (one UPDATE of row 13 with another value and a recomputed
  hash): 190 of 190 later receipts not reproduced and none before, caught. Every later receipt
  fails, more than the queries that read row 13, because the state hash covers the whole state.
- **Guards:** test_qdb.sh 290 pass, 0 fail; crash test 194 points, 0 violations; P15 0 of 10,000
  with the digest unchanged (Windows and Linux); P2 2,000 of 2,000; text 0 changed; WAL 0
  failures with the reference engine; library 0 failures; reference differential 16 of 16.
- Limits: the crash and power-cut tests do not run with history on; `AS OF` replays the history
  from the base on every query; a receipt covers the state, the query text and the printed result,
  and says nothing about who ran it.

## Stage 1: a correct, durable single-file engine

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 1a Regression gate | `bdb/tests/test_qdb.sh` reports any FAIL | PASS | 290 pass, 0 fail after P13; 277 after P12; 274 after P11; 257 after P10; 250 after P9; 244 after P8; 241 after P7; 222 after the completion changes (214/0/1 at the move) |
| 1b SQLite differential | any of the P2 queries disagrees, with numbers compared as numbers | PASS | 2,000 of 2,000 after P8, a CI gate; 1,944 after P7; 1,251 at the move (519 null_vs_empty, 230 rows, 46 number_text) |
| 1c Crash recovery | any P3 crash point breaks the rule, or any P15 seed does | PASS | P15: 0 violations in 10,000 seeded power cuts (dropped, torn and reordered unflushed writes) after P15b, 2 before; hashes equal on Windows and Linux for 10,000 of 10,000. Process stops: 0 violations of 194 after P14; 0 of 170 with `.ckpt`; 24 of 104 at the move |
| 1d Cross-process transaction | either P4 case misses | PASS | four checks in test_qdb.sh (`xproc_*`), differential process mode equals REPL mode on 8 seeds |
| 1e Text round trip | any P5 value changes | PASS | 2,000 of 2,000 (`test_qdb_text.py`); format 0 files still load |
| 1f NULL apart from '' | `INSERT (NULL)` and `INSERT ('')` are indistinguishable by `IS NULL`, or `NOT (NULL = x)` is true | PASS | P7: NULL is its own value with three-valued logic; 19 checks in test_qdb.sh; P2 null_vs_empty 519 to 0. Before: both stored an empty cell |
| 1h History, AS OF and receipts | a second process with `AS OF` fails to reproduce any of the P16 results, or receipts cost 5 percent of query time or more | FAIL | P16b: 1,000 of 1,000 reproduced on Windows and Linux; overhead 13.0 percent (Linux) and 5.7 percent (Windows) with the runtime SHA-256; P16: 44 and 347 percent with SHA-256 written in BuildLang |
| 1g JOIN gaps that fail closed | a secondary ORDER BY key over a grouped JOIN, DISTINCT over a grouped JOIN, or COUNT(DISTINCT) over any JOIN is refused | FAIL | each refused with a specific message (`bdb/README.md`, Known limitations) |

## Stage 2: SQL coverage against the outside corpus

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 2a sqllogictest queries | any P1 query record does not pass | FAIL | 8,921 of 9,202 (96.95 percent) after P12; 8,186 after P11 (`select5` not run); 1,084 after P10; 32 at the move; `bdb/tests/slt_run.py` |
| 2b Expressions in the select list | `SELECT a+1, a*b-c FROM t` refused or wrong | PASS | P11: arithmetic, CASE, functions, correlated subqueries; `select1` to `select3` 5,320 of 5,320 |
| 2c Column types with arguments and constraints | `VARCHAR(30)`, `INTEGER PRIMARY KEY`, `UNIQUE` refused | PASS | P9 type arguments; P13 PRIMARY KEY, UNIQUE and NOT NULL enforced on INSERT and UPDATE, kept in the file and through ROLLBACK; statements 1,931 of 1,993 |
| 2d CASE, scalar functions (abs, coalesce) | refused or wrong | PASS | P11: searched and simple CASE, `abs`, `coalesce`, `ifnull`, `nullif` |
| 2e Correlated subqueries, EXISTS | refused or wrong | PASS | P11: scalar, EXISTS and IN subqueries naming outer columns (`x_corr_subq`, `x_exists`) |

## Stage 3: scale

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 3a Bulk load is linear | `bench_qdb_load.py` log-log slope k above 1.3 for autocommit or transactional indexed load | PASS | HarperZ9/build-universe#35: autocommit k 1.16, transactional indexed k 1.02. Limit stated there: interleaved INSERT plus indexed SELECT stays superlinear (k 1.52). Not re-run after the checkpoint change, which writes the state twice per checkpoint |
| 3b ORDER BY is n log n | P6 ratio 30k/10k above 6 | PASS (single-table) | 3.29 after, 9.51 before (`bench_qdb_sort.py`). The UNION, grouped, JOIN and grouped-JOIN sorts are still selection sorts and were not measured |
| 3c Whole-program store of 10^5 rows | a `bql export` store of 100,000 rows loads and answers a GROUP BY in under 10 s | NOT BUILT | |

## Stage 4: an embeddable engine

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 4a Library entry point | a C program linked against a static library built from qdb's sources runs N statements on one open database in one process, and its output differs from the CLI's on the same statements | PASS | `bdb/lib` (qdb_open, qdb_exec, qdb_close); `test_qdb_lib.py`: 3 seeds of 129 statements, stdout and `.db` byte-identical to one CLI process per statement; dropping one statement is caught. Limit: a parser rejection exits the host with code 1 |
| 4b Many queries per open | the compiler side runs more than one query without a process per statement | NOT BUILT | the library exists (4a); `buildc bql` does not link it yet. On the test workloads one process took 0.04 to 0.13 s against about 1 s for one process per statement |

## Stage 5: BuildQL over programs

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 5a Export, query, verify | `cargo test bql` fails | PASS (branch) | 19 bql tests pass, `in_process_subset_matches_qdb` included with `QDB_BIN` set; 35 of 35 sampled renames agree |
| 5b `refs` table | resolved references are missing from the store, or a planted wrong span is not caught by `bql verify` | PASS (values and calls) | store schema `buildlang.bdb/v1`. For every function in `tests/programs` whose rename is accepted, refs equal its name tokens less the definition: 290 of 290 (139 programs); a checker that drops call references gives 19 of 289. A shifted or dropped ref span is drift. Not covered: type names, method calls, multiple dispatch |
| 5c Multi-file programs | a program with `mod name;` or `use` is refused by `bql export` | FAIL | refused. Module resolution lives in the binary (`main.rs`), and spans would need a file column |
| 5d Symbol text with `|` or line breaks | `bql export` refuses a cell | PASS | export writes format 1 and escapes; `export_escapes_a_pipe_and_a_line_break` |
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
codegen regression that breaks qdb fails the compiler's own PR. Licence, decided by the author on
2026-11-09 ("for the license, I think AGPL is good."): `bdb/` is AGPL-3.0-only like the rest of
the repository (`bdb/LICENSE`, `NOTICE`).
