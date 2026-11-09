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

**P18. IN with a literal on the left, and IN of a table (item 4), written 2026-11-09 06:50 PT before
the change.** The largest groups of P1 rejections are `evidence/in1.test` (183, "expected keyword
'FROM', got 'IN'": `SELECT 1 IN (...)` with no FROM) and `evidence/in2.test` (45, "expected a
comparison after the literal"): `WHERE 1 IN (...)`. The expression path (P11) already reads IN
with a list, an empty list and a subquery, with SQL's NULL rules; the older path is chosen for
these statements. The change routes a SELECT to the expression path when IN or NOT IN follows a
literal or NULL, or appears in the select list, and reads `x IN t` (SQLite's table form) as
`x IN (SELECT * FROM t)`. Measured on P1. Predictions: `in1.test` from 4 to at least 150 of 187
query records; `in2.test` from 0 to at least 35 of 45; every other file unchanged; total from
8,921 to between 9,100 and 9,160. Planted check: a build without the table form loses the
`IN t` records of `in1.test` (about 90). Guards: test_qdb.sh no FAIL, P2 2,000, statements not
below 1,931.
**P19. Throughput through bdb/lib against SQLite (item 5), written 2026-11-09 07:45 PT before any
run.** A new `bdb/benchmarks/bench_qdb_throughput.py` builds the library in release mode and
`bdb/benchmarks/throughput.c` twice: linked with `libmain.a`, and linked with SQLite (on Windows
the `sqlite3.dll` that ships with Python 3.12, SQLite 3.49.1; see A11). Each run makes `t (id INTEGER
PRIMARY KEY, v INTEGER)` with 10,000 rows (qdb also gets `CREATE INDEX t_id ON t (id)`, its only
way to look a key up), then times 2,000 statements inside one transaction, passed as SQL text to
both: `select` (`SELECT v FROM t WHERE id = k`), `update` (`UPDATE t SET v = v + 1 WHERE id = k`)
and `mixed` (alternating). qdb prints each result to stdout, sent to the null device; SQLite's
callback formats each row into a buffer. Median of 3 runs per engine and mode, one core, the same
machine. The target is qdb within 10 times SQLite's time. Predictions: `select` 10 to 40 times;
`update` and `mixed` 100 times or more, since an UPDATE marks every index stale and the next
indexed read rebuilds it. Whatever the ratio is, it is reported. No planted check: this is a
measurement, and its guard is that both engines end with the same table (checked by the benchmark
counting rows with `v` changed).

**P20. UPDATE and DELETE in place (item 1 of the 2026-11-09 continuation), written 2026-11-09 before
the change.** Today an UPDATE copies the whole cell array into a new one, evaluates WHERE on every
row of the table, and marks every index stale; a DELETE copies the array and marks every index
stale; the next indexed read rebuilds each index with a sort. The change: an UPDATE or DELETE whose
WHERE is one equality on an indexed column takes its candidate rows from the index (each candidate
is still checked against the WHERE), computes every new value and checks constraints before it
writes, then writes the changed cells into the array in place. Each index on a changed column
moves the changed entries to their new sorted place (key, then row id, the order a rebuild gives);
other indexes are untouched. A DELETE compacts the cell array in place and renumbers the row ids
of every index on the table in one integer pass, with no sort. The key map of P17 stays valid
across an UPDATE that changes no constrained column. Measured on the P19 benchmark, unchanged
(Windows, 10,000 rows, 2,000 statements, median of 3). Predictions: `update` from 2,472 times
SQLite's time to between 5 and 25 times; `mixed` to between 5 and 25 times; `select` unchanged
within noise (8 to 14 times). Target: update and mixed within 10 times. Planted checks: a new
`bdb/tests/test_qdb_mutations.py` runs seeded scripts of INSERT, UPDATE and DELETE on three
indexed tables (a DELETE in the first table moves the later tables' cells), with indexed equality
and range reads in between, through one qdb process and through Python's sqlite3, and compares
every read as a multiset of rows. It must fail on (a) a build that skips the index move when an
indexed column changes, and (b) a build whose DELETE does not renumber index row ids; and its
harness must count a disagreement when one qdb row is altered. Guards: test_qdb.sh no FAIL, crash
test 194 points 0 violations, seeded power cuts 0 of 10,000 with the P15 digest unchanged (the
.db and WAL bytes must not change), P2 2,000 of 2,000, text 2,000 of 2,000, WAL recovery, library
and reference differential unchanged.

**P21. A transaction's WAL lines held until COMMIT, written 2026-11-09 after the P20 results, before
the change.** P20 left about 330 microseconds per statement inside a transaction in the WAL append
(one file open, write and close per OP line). The change: BEGIN's TXID line and each statement's
OP line are held in memory and appended in one write together with the COMMIT record, or with
HOLD at a clean exit inside BEGIN; ROLLBACK discards them; RESUME is still written at once. The
WAL bytes after a clean COMMIT or HOLD are the same as before, line for line; only the number of
writes changes. Nothing in an open transaction was durable before its COMMIT, and replay drops a
transaction without one, so a crash loses nothing it could have kept. Measured on the P19
benchmark unchanged. Predictions: `update` 4 to 12 times SQLite's time; `mixed` 5 to 12 times;
`select` 8 to 14 times. Target: update and mixed within 10 times. The crash test has fewer crash
points than 194 (the OP writes inside transactions go) and 0 violations. The seeded power cut
digest changes, since the write sequence changes; the new digest must be equal on Windows and
Linux, with 0 violations in 10,000 seeds, and replaces the CI value with this reason. Planted
checks: a build that writes the held lines at COMMIT but not at HOLD fails the cross-process
transaction checks of test_qdb.sh; a build that drops the held lines and writes only the COMMIT
record shows violations in the crash test. Guards as in P20, and test_qdb_mutations.py.

**P22. `INSERT ... SELECT` (item 2), written 2026-11-09 before the change.** qdb refuses `INSERT INTO t
SELECT ...` ("expected keyword 'VALUES'"), so the 7 statement records of that form in the corpus fail
and the tables they fill stay short: most of the 65 wrong P1 answers after P18 read such a table
(A10). The change: after the table name and optional column list, a SELECT is parsed by the
expression path (P11) and run before the insert; its rows go through the same checks as VALUES
rows (column count, constraints, the key map). A SELECT of zero rows inserts nothing. Measured on
P1. Predictions: statements from 1,931 to at least 1,938; query records from 9,087 to between
9,120 and 9,145; wrong answers from 65 to under 20; no file loses a record. Planted check: a build
that drops the last row of the SELECT keeps most of those records wrong (at least 30 fewer passes
than the change). Guards as in P21, and new test_qdb.sh checks: rows copied, a column list, a
SELECT of zero rows, a SELECT from the table itself, a constraint failure that inserts nothing.

**P23. DISTINCT in every aggregate, TOTAL, group_concat and the refused aggregate forms (item 3),
written 2026-11-09 before the full run.** After P22 the P1 rejections in
`evidence/slt_lang_aggfunc.test` are: DISTINCT inside SUM, AVG, MIN, MAX or TOTAL (19, "DISTINCT
is only supported inside COUNT"); `total(...)` and `group_concat(...)`, which the older path does
not read (11 "expected keyword" and 9 "unexpected trailing input", some with `NOT INDEXED`); and
`x NOT NULL` as a postfix test (4). The change: the older path accepts DISTINCT in every aggregate
and feeds SUM, AVG, MIN and MAX the rows that first carry each distinct value (values compare as
values, so 2 and 2.0 are one value; COUNT(DISTINCT) keeps its text comparison); a SELECT without
GROUP BY that uses TOTAL, group_concat, DISTINCT in a non-COUNT aggregate, an index hint (`NOT
INDEXED`, `INDEXED BY i`) or `NOT NULL`, `ISNULL`, `NOTNULL` as a postfix test goes to the
expression path, which gains group_concat (with an optional separator; DISTINCT with a separator
is an error, as in SQLite), the index hints (read and ignored) and the postfix tests. The
expression path's DISTINCT also compares values. Disclosed: during development the aggfunc file
alone was run on the change (61 of 67 query records, from 23), before this entry. Measured on P1.
Predictions: total from 9,143 to between 9,175 and 9,185; `slt_lang_aggfunc.test` to at least 60
of 67; no other file changes; statements not below 1,938. Planted check: a build whose DISTINCT is
ignored in SUM, AVG and TOTAL loses at least 8 records of the aggfunc file. Guards as in P22.

**P24. REPLACE (item 5), written 2026-11-09 before the full run.** `REPLACE INTO t ...` and `INSERT OR
REPLACE INTO t ...` are refused, so `evidence/slt_lang_replace.test` has 4 wrong answers and 4
statements fail. The change follows SQLite: each new row removes every row whose UNIQUE or PRIMARY
KEY key equals its own (NULL keys never clash), then goes in; within one statement a later row
replaces an earlier one; a NOT NULL failure is found before anything is removed, so a failing
REPLACE changes nothing. Removal uses the in-place DELETE of P20. Disclosed: during development the
replace file alone was run on the change (6 of 6 queries, 8 of 8 statements) and one script was
compared with Python's sqlite3, before this entry. Measured on P1. Predictions: total 9,181 to
9,185; statements 1,938 to 1,942; no other file changes. Planted check: a build whose REPLACE
removes nothing fails the replace file (at least 2 of its 6 queries wrong or rejected) and a new
test_qdb.sh check. Guards as in P23.

**P25. Prepared statements and a row API in bdb/lib (speed item 1), written 2026-11-09 before the
measured runs below.** The P19 benchmark passes SQL text per statement and qdb prints each result,
so it times qdb's parser and printer more than its engine: a probe on this machine put the parse of
`SELECT v FROM t WHERE id = k` at about 22 microseconds of the 31. The change adds to bdb/lib
`qdb_prepare` (`?` placeholders), `qdb_bind_int`/`_int64`/`_text`/`_null`, `qdb_step` (100 a row,
101 done, as SQLite), `qdb_column_count`/`_name`/`_text`/`_int64`, `qdb_reset` and `qdb_finalize`.
Results come back as rows, nothing is printed. A statement is planned once: a point select on an
indexed column (`SELECT cols FROM t WHERE c = ?`) reads the index directly; an UPDATE with one
equality WHERE sets the parse fields from the plan and runs exec_update, or, when the WHERE column
is an indexed number column and no index is on the SET column, a direct path that computes the same
rows and values and then shares exec_update's second half (`update_apply`) and the WAL line;
anything else, and any statement with a text or NULL parameter, writes the parameters in as SQL
literals and runs exec_sql with its rows captured. A schema change re-plans at the next step. Two
engine changes serve the planned paths: each pool entry caches its number and kind on first use
(`sp_kind`), read at index build for numeric index keys, so an equality search on a numeric index
compares floats instead of parsing strings; and the fast paths reuse their buffers between steps.
The text path and the CLI keep their output and WAL bytes. Disclosed: the design was explored on
this machine before this entry, with single runs: prepared, both engines prepared, qdb's time was
1.14, 1.6 and 1.2 times SQLite's (select, update, mixed); with text per statement it was 12.4, 7.4
and 6.1 before the change. Measurement: P19's harness gains `--apis text,prep`. API prep prepares
both statements once on both engines (SQLite through `sqlite3_prepare_v2`, `bind_int64`, `step`,
`column_text` copied to a buffer) and runs one untimed select first, since qdb folds the loaded rows
into its index at the first indexed read where SQLite built its key B-tree during the untimed load.
Median of 3, Windows, one core, 10,000 rows, 2,000 statements in one transaction. Predictions: API
text unchanged within noise (select 10 to 15 times SQLite's time, update and mixed 5 to 10 times);
API prep select 0.8 to 1.6 times, update 1.2 to 2.5 times, mixed 1.0 to 2.0 times. The author's
target is below 1 on all three; a miss is reported as one. Correctness: a new
`bdb/tests/test_qdb_rows.py` runs the differential workloads and a keyed workload (an indexed
INTEGER key holding integers, reals, text and NULL; updates by `+ - * =` inside and outside
transactions; an index added on the SET column halfway) twice through the row API, once with every
integer literal of a SELECT or UPDATE bound as a parameter and once as literal text; rows, column
names, the .db and the .wal must be identical, and the literal run's rows must equal the CLI's
output cell by cell. Planted check: a library whose numeric index search also accepts larger keys
must show differences. Guards: test_qdb.sh, crash test (176 points, 0 violations), seeded power
cuts (0 of 10,000, digest 77a49282... unchanged, since no WAL or .db bytes change), P2 2,000 of
2,000, text round trip, WAL recovery, reference differential, library, receipts, mutations.

**P26. Settling the P25 comparison (speed item 1), written 2026-11-09 before the runs below.** P25's
prepared update and mixed ratios (0.97, 0.92) sat inside the machine's run-to-run spread, and each
timed region was short: 2,000 statements take 1 to 3 ms, near the timer's and the scheduler's
grain. No engine change. The harness gains paired runs (qdb and SQLite alternate, one pair per
repetition, so a slow spell lands on both), the median of the per-pair ratios, and a
distribution-free interval for that median (order statistics; with 11 pairs, the 3rd and 9th
smallest ratios, exact coverage 93.5 percent). Two runs, prep API, 10,000 rows, 11 pairs each, at a
moment with no cargo, rustc or render job and at least 8 GB free: run A repeats P25's 2,000
statements; run B uses 200,000 statements so each timed region lasts about 0.1 to 0.3 s. Claim rule:
qdb is called faster than SQLite on a mode only when the interval's upper end is below 1.0; slower
only when its lower end is above 1.0; otherwise unsettled. Predictions (written before any paired
run): select slower in both runs (B median 1.4 to 1.9); update and mixed unsettled in run A; in run
B update 0.8 to 1.3 and mixed 0.8 to 1.2, with at most one of them settled faster. Planted check:
`--aa` puts SQLite in qdb's place; its intervals must contain 1.0 on all three modes (run B
settings, 7 pairs). A planted interval that excludes 1.0 means the interval method or the machine
cannot support a claim, and no claim is made. Guards: the full set, on the unchanged engine.

**P27. Typed key array in each index (speed item 2, first step), written 2026-11-09 before the
measured runs below.** An index holds pool ids of its keys (`idx_keys`); an equality search on a
numeric index reads each probed key's number from `pool_num` at that id, and since pool ids of keys
are spread over the whole pool, most of the 14 probes of a 10,000-key search miss the cache (P25's
profile). The change adds `idx_num`, an f64 array parallel to `idx_keys` holding each entry's key
number (NULL as minus infinity, which sorts first and equals no number), and the numeric search
runs over it alone. `ix_move` (UPDATE of an indexed column) keeps it current; every other write of
the index arrays (splice_index, which every rebuild and merge uses, DELETE's renumbering, ROLLBACK's
restore) clears a flag and the next numeric search rebuilds it from the pool cache in one pass. No
.db, WAL or output byte changes, so the power-cut digest stays 77a49282.... Measurement: P26's run
B settings (prep API, 10,000 rows, 200,000 statements, 11 pairs) and the same at 100,000 rows.
Predictions: select ratio falls by 10 to 35 percent against P26 run B; update and mixed fall by 5 to
25 percent; at 100,000 rows the select gain is at least as large as at 10,000. Planted check: a
build whose `ix_move` leaves `idx_num` unchanged must fail test_qdb_rows.py (the keyed workload
updates an indexed column and then searches it). Guards: the full set.

**P28. Update path costs with no byte change (speed item 2), written 2026-11-09 before the measured
runs below.** After P27 a prepared UPDATE takes 2.3 to 2.4 times SQLite's time. Three per-statement
costs in it, none of which changes a byte on disk: (1) the WAL line's FNV hash takes a 64-bit
remainder by 2^32 per byte; the value is non-negative and below 2^56, so a mask by 2^32 - 1 gives the
same number without a division; (2) every changed cell gets a new string and pool entry
(`sp_add(to_string_i32(r))`); integers 0 to 65,535 written by the fast UPDATE now share one pool
entry per value through a cache (`sp_int`), which is safe because no pool entry is changed after
`sp_add`; (3) `update_apply` allocates its old-id vector per call; it now reuses one. Measurement:
P27's runs (prep API, 200,000 statements, 11 pairs, 10,000 and 100,000 rows) against the P27 tip.
Predictions: update falls 10 to 30 percent and mixed 5 to 20 percent at both sizes; select changes
by less than 10 percent. Planted checks: a build whose FNV mask is 2^32 - 2 must change the
power-cut digest (the fault test's expected-digest check fails); a build whose `sp_int` returns the
entry of r - 1 must fail test_qdb_rows.py. Guards: the full set; digest 77a49282... unchanged.

**P29. Memory that follows the data (speed item 3), written 2026-11-09 before the measured runs
below.** The runtime frees no string and no list on its own, so every temporary a statement makes
stays until the process ends. Found while reading the hot paths: `db_text` and `concat_parts` grew
one string by repeated `+` (each step a new copy of everything so far); `reset_parser` gave every
statement about 48 new lists; the tokenizer made one string per character; `con_check_insert` built
a map for a single row; `qdb_from_c` grew the statement text a byte at a time. Changes, none of
which may change a byte of output, of the `.db` file or of the WAL: one `join` per checkpoint and
per COMMIT (`Vec<String>.join`, buildlang #224); parser lists emptied in place outside a subquery
(`reset_fresh` keeps new lists where a subquery saves the old ones); a byte tokenizer with one
`substring` per token; a one-row constraint probe that reuses the keys it built; constraint keys
from cached pieces; row counts written in place; the statement text from the host in one
allocation; an unescaped WAL line held as three pieces instead of two copies; `exec_select` frees
its four work lists. Exploratory numbers already seen (scratch builds, not the committed tree):
through `bdb/lib`, load of 8,000 rows peaked at 0.888 GB before; after, 10.6, 45.6 and 89.1 MB at
10,000, 50,000 and 100,000 rows, against 0.9, 1.3 and 2.0 MB for SQLite 3.54.1. Measurement:
`throughput` mixed, prepared API, 1,000 statements after the load, peak commit and peak working set
of the process read by the watchdog (PeakPagefileUsage, PeakWorkingSetSize), median of 3 runs, rows
10,000, 50,000 and 100,000, SQLite 3.54.1 (the current release; earlier sections used 3.49.1). The
build before this change runs at 1,000, 2,000, 4,000 and 8,000 rows only, under a 6 GB cap, because
its growth is faster than linear. Predictions: (1) qdb's peak commit is linear in rows: bytes per
row from 10,000 to 50,000 and from 50,000 to 100,000 differ by less than 15 percent; (2) the target
is peak commit within 10 times SQLite's at 100,000 rows, and from the exploratory runs I expect it
MISSED at about 45 times; (3) speed, P28's runs against SQLite 3.54.1 (prepared, 200,000
statements, 11 pairs, 10,000 and 100,000 rows): no ratio rises more than 10 percent above P28's
published value (0.56, 1.32, 1.00 at 10,000; 1.13, 1.74, 1.34 at 100,000), with the caveat that the
SQLite release differs. Planted checks: a build whose `con_key` joins with `;` in place of `|`
between the constraint number and the length must fail `test_qdb.sh`; a build that drops the
newline piece of a held WAL line must fail `test_qdb_wal_recovery.py` or change the power-cut
digest; a build that empties parser lists in place inside a subquery too (`reset_fresh` never set)
must fail `test_qdb.sh`. Guards: the full set, digest 77a49282... unchanged, sqllogictest 9,185 of
9,202, SQLite differential 2,000 of 2,000, and `cmp` of output and `.db` against the build before.

**P30. A memory growth test with a planted leak (speed item 3), written 2026-11-09 before the
measured runs below.** `bdb/tests/test_qdb_memory.py` runs the CLI on three workloads at 20,000 and
80,000 statements and takes the slope of peak memory (PeakPagefileUsage on Windows, `ru_maxrss` on
POSIX) in bytes per statement: load (single-row INSERTs in one transaction), select (point SELECTs
on 5,000 rows), update (UPDATEs, 1,000 to a transaction). Exploratory slopes on Windows for the
P29 scratch build: 989, 1,207 and 1,045. Bounds: 1.5 times the slope measured on the committed
tree on Windows, rounded up to the next 100; they are regression limits, not targets. The slope on
the CI runner (Linux, another allocator) is unknown until it runs; if it exceeds a bound the result
is logged as an amendment and that bound is reset to 1.5 times the Linux slope. Planted check, run
in CI on every push: a build whose `reset_parser` always takes new lists (`reset_fresh` forced
true) must exceed the select bound, or the test fails. Exploratory: that plant measured 4,947 bytes
per statement when the unplanted build measured 3,162. What the test does not show: the text path
still leaves about 1 KB per statement, which the P29 changes do not remove.

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
- **A10 (2026-11-09 07:20 PT, after the first P18 run, before the second).** First run: 9,084 of
  9,202 query records (prediction 9,100 to 9,160: missed), `in1.test` 123 of 187 (prediction at
  least 150: missed), `in2.test` 44 of 45 (held), and statements 1,927 of 1,993, below the 1,931
  guard. Causes: on the expression path `NULL IN ()` was NULL where SQL gives false (an empty set
  holds no match even for NULL); and `IN (SELECT 1, 2)` with two columns was accepted where SQLite
  refuses it (the four `statement error` records of `in2.test`). Both are fixed in P18 before the
  second run. Most of the 58 wrong `in1.test` records read tables filled by `INSERT INTO t SELECT
  ...`, which qdb does not run (7 statement records), so they stay wrong. The predictions stand
  for the second run.
- **A11 (2026-11-09 08:15 PT, before any P19 measurement).** Linking `throughput.c` with the
  `sqlite3.dll` that ships with Python crashed at the first call (a null function pointer inside
  the DLL, under MinGW, with direct linking and with an import library from `dlltool`). The SQLite
  twin is compiled instead from the SQLite 3.49.1 amalgamation (`sqlite3.c`, public domain, from
  sqlite.org; kept outside the repository with its SHA-256), with default options and -O2, the
  same version as before. Nothing else in P19 changes.

**A11 (2026-11-09, P27).** The keyed workload of `test_qdb_rows.py` gains a bound
`SELECT id, v FROM t WHERE v = k` right after each `UPDATE t SET v = k` once the index on v exists,
because P27's preregistered planted check (a stale `idx_num` after `ix_move`) was not caught
without it. Written after that miss, before the after-measurements.

**A12 (2026-11-09, P28).** A change that claims no byte change on disk is also checked by
`test_qdb_wal_recovery.py --old <previous build>`: WAL files written by the build before the
change must recover under the new one. Added because P28's planted FNV change was invisible to the
power-cut digest. Written after that miss, before the after-measurements.

**A13 (2026-11-09, P29).** P29's planted `con_key` build (`;` for `|` between the constraint number
and the length) was MISSED by every guard: the key is never stored, it is only compared with keys
the same function made, so a consistent change of separator is an equivalent program and no test
can tell it apart. Replacement plant, chosen after that miss: `con_key` leaves out the cell body.
`test_qdb.sh` then fails 24 of 314 (caught). The miss stays in the P29 results.

**A14 (2026-11-09, P29).** The speed script also ran the build before the change at 100,000 rows,
against the preregistered limit of 8,000 rows for that build. It needs more than 7.6 GB there; the
run was stopped when free memory fell under 4 GB, so no same-session pair exists at 100,000 rows.
Two runs were added after the fact and are exploratory: the new build against SQLite 3.49.1 at
100,000 rows, and an A/A run at 10,000 rows.

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
  a key column); a constrained bulk load is quadratic until keys get an index. Closed by P17. DEFAULT, CHECK and
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

## After P17: constraint checks through a key map (2026-11-09, feat/bdb-con-index)

- **Keyed bulk load** (`bench_qdb_load.py txn_keyed`, Windows, one transaction): 8,000 rows 44.1 s
  before, 2.72 s after (16 times faster); exponent 1.88 before, 0.92 after. With one UPDATE of
  the UNIQUE column at the end (`txn_keyed_update`): 49.6 s to 3.03 s, exponent 1.93 to 1.03.
  Predictions (before at least 1.7, after at most 1.3, at least 5 times faster at 8,000) held.
- **Planted checks:** a build whose map is never marked stale fails `map_numeric_key`,
  `map_freed_keys` and `map_count`; a build that compares key cells as text fails the same three.
  Both caught.
- **Guards:** test_qdb.sh 293 pass, 0 fail (three checks added); P1 8,921 of 9,202 queries and
  1,931 of 1,993 statements, unchanged; P2 2,000 of 2,000; crash 194 points, 0 violations; P15
  digest unchanged; P16 1,000 of 1,000; text 0 changed; WAL 0; library 0; reference differential
  16 of 16. Load with no constraint (`txn`, 2,000 and 8,000 rows): exponent 0.97, 0.30 ms per row.
- Limits: the map lives in one process and is rebuilt (one pass over the constrained tables) by
  the first INSERT of each process and after any other mutation, so a single-statement INSERT per
  process still costs a pass over its table; a run of INSERTs in one process or one library handle
  pays it once.

## After P18: IN with a literal on the left, IN of a table (2026-11-09, feat/bdb-in)

- **P1 sqllogictest:** 9,087 of 9,202 query records (98.75 percent), up from 8,921; 65 wrong, 50
  rejected; statements 1,931 of 1,993 (unchanged). `evidence/in1.test` 4 to 125 of 187,
  `evidence/in2.test` 0 to 45 of 45, every other file unchanged. Predictions: total 9,100 to 9,160
  missed by 13; `in1.test` at least 150 missed (56 records stay wrong because their tables are
  filled by `INSERT INTO t SELECT ...`, which qdb does not run); `in2.test` held. The first run
  (A10) gave 9,084 with statements at 1,927; its two faults were fixed before this run.
- **Planted check:** the build without the table form passes 81 of 187 `in1.test` records (76
  rejected), 44 fewer than with it: caught, below the estimate of about 90 because many of those
  records are wrong either way for the reason above.
- **Guards:** test_qdb.sh 293 pass, 0 fail; P2 2,000 of 2,000; statements 1,931.
- What is left in P1, by count: 65 wrong (56 in `in1.test` from `INSERT ... SELECT`, the rest in
  the REPLACE, UPDATE and aggregate files), 19 `DISTINCT` inside SUM, AVG, MIN, MAX or TOTAL, 11
  aggregate forms the parser refuses, 9 trailing input, then views (2), `group_concat` (1); and 62
  statements (triggers 20, views 10, DROP forms 9, `INSERT ... SELECT` 7, REPLACE 4, REINDEX 1).
## After P19: throughput through bdb/lib against SQLite (2026-11-09, feat/bdb-throughput)

Windows, one core, `bench_qdb_throughput.py`, 10,000 rows, 2,000 statements in one transaction,
median of 3; SQLite 3.49.1 compiled from the amalgamation (A11).

| Mode | qdb (statements per second) | SQLite | qdb's time against SQLite's |
|---|---|---|---|
| select (`WHERE id = k`) | 36,368 | 362,450 | 10.0 times |
| update (`SET v = v + 1 WHERE id = k`) | 131 | 323,363 | 2,472 times |
| mixed | 130 | 283,688 | 2,185 times |

- Both engines end in the same state (SUM(v) 499,600 after 1,000 rows and 200 mixed statements).
- Predictions (select 10 to 40 times; update and mixed 100 times or more) held. The target (within
  10 times) is met by point selects at the boundary and missed by updates by a factor of about
  250: one UPDATE takes about 7.6 ms on a 10,000-row table, where SQLite takes 3 microseconds.
  The UPDATE path scans and rewrites the table's cells and marks every index stale, so the cost
  grows with the table, not with the rows changed. That is the next throughput target.
- qdb's select number includes printing each result to the null device (qdb has no row API);
  SQLite's callback formats the row into a buffer.

## After P20: UPDATE and DELETE in place (2026-11-09, feat/bdb-inplace)

Windows, one core, the P19 benchmark unchanged (10,000 rows, 2,000 statements in one transaction,
median of 3, SQLite 3.49.1 from the amalgamation).

| Mode | qdb (statements per second) | SQLite | qdb's time against SQLite's | Before (P19) |
|---|---|---|---|---|
| select | 30,196 | 364,432 | 12.1 times | 10.0 times |
| update | 2,939 | 302,435 | 102.9 times | 2,472 times |
| mixed | 5,362 | 276,702 | 51.6 times | 2,185 times |

- Both engines end in the same state (SUM(v) 499,600).
- Predictions: `select` 8 to 14 times held. `update` and `mixed` 5 to 25 times MISSED: update fell
  24 times (7.6 ms to 340 microseconds per statement) and stays 103 times SQLite's. The target
  (within 10 times) is missed.
- Cause of the miss, found after the run: inside an explicit transaction each statement still
  appends its `OP` line to the WAL with its own file open, write and close, which costs about 330
  microseconds on this Windows machine. An exploratory build with that append removed (not a
  candidate; it loses the transaction's lines) ran update at 6.7 times and select at 11.5 times
  SQLite's time (1 run each). Holding a transaction's lines in memory until its COMMIT is P21.
- Planted checks: `test_qdb_mutations.py` (20 seeds, 2,143 compared answers) finds 0 disagreements
  with SQLite; a build that skips the index move finds 661; a build whose DELETE keeps the old row
  ids in its indexes exits on an out-of-range read in the first seed (caught); an altered qdb row
  is counted. The P19 build also passes it (it was slow, not wrong).
- Guards: test_qdb.sh 293 pass, 0 fail; crash test 194 points, 0 violations; seeded power cuts 0
  of 10,000 with the P15 digest unchanged (cd3ccf9f...); P2 2,000 of 2,000; text 0 changed; WAL
  recovery 0 failures; reference differential 16 of 16; library 0 failures; receipts reproduced.

## After P21: a transaction's WAL lines held until COMMIT (2026-11-09, feat/bdb-walbuf)

Windows, one core, the P19 benchmark unchanged (10,000 rows, 2,000 statements in one transaction,
median of 3).

| Mode | qdb (statements per second) | SQLite | qdb's time against SQLite's | P20 | P19 |
|---|---|---|---|---|---|
| select | 28,319 | 365,764 | 12.9 times | 12.1 | 10.0 |
| update | 37,895 | 285,796 | 7.5 times | 102.9 | 2,472 |
| mixed | 32,515 | 270,234 | 8.3 times | 51.6 | 2,185 |

- Both engines end in the same state (SUM(v) 499,600).
- Predictions held: update 7.5 times (4 to 12), mixed 8.3 times (5 to 12), select 12.9 times (8 to
  14). Target met for update and mixed (within 10 times). Point selects stay above 10 times; their
  cost includes printing each result, since qdb has no row API.
- Crash test: 176 crash points (194 before; the OP writes inside transactions are gone), 0
  violations. Seeded power cuts: 0 of 10,000, new digest 77a49282bfb71d4786b2a8ac6be890f1e5952b2e6dc960b26033ccbdaa24c47f,
  equal on Windows and Linux (WSL); it replaces the CI value, since the write sequence changed.
  The flush-ignoring plant still shows violations (627 of 2,000 seeds).
- Planted checks: a build that writes HOLD without the held lines fails 5 checks of test_qdb.sh
  (the four `xproc_*` checks and `rollback_drops_con`); a build that writes only the COMMIT record
  shows 8 violations in the crash test.
- Guards: test_qdb.sh 293 pass, 0 fail; P2 2,000 of 2,000; text 0 changed; WAL recovery 0
  failures; reference differential 16 of 16; library 0 failures; receipts reproduced;
  test_qdb_mutations.py 0 disagreements.

## After P22: INSERT ... SELECT (2026-11-09, feat/bdb-insert-select)

| Measure | P18 | P22 | Prediction |
|---|---|---|---|
| P1 query records passing | 9,087 of 9,202 | 9,143 (99.36 percent) | 9,120 to 9,145: held |
| wrong answers | 65 | 9 | under 20: held |
| statements as expected | 1,931 of 1,993 | 1,938 | at least 1,938: held |

- `in1.test` 125 to 181 of 187 (6 rejected); no file lost a record.
- Planted check MISSED its bar: the build that drops the last row of each SELECT passes 9,127,
  16 fewer than the change (bar: at least 30 fewer). It does separate (16 more wrong answers,
  mostly in `in1.test`), but by less than predicted: many corpus queries read tables whose last
  copied row does not change their answer.
- INSERT ... SELECT takes the queries the expression path reads; a compound SELECT (UNION) or
  GROUP BY in the inserted query is refused ("unexpected trailing input").
- test_qdb.sh 301 pass, 0 fail (8 new checks: rows, column list, zero rows, a table copied into
  itself, a key clash with the table, a key clash inside the batch, nothing inserted on failure,
  column count). Guards: crash 176 points, 0 violations; power cuts 0 of 10,000, digest
  77a49282... unchanged; P2 2,000 of 2,000; text 0 changed; WAL recovery 0; reference differential
  16 of 16; library 0 failures; receipts reproduced; test_qdb_mutations.py 0 disagreements.

## After P23: DISTINCT in every aggregate, TOTAL, group_concat (2026-11-09, feat/bdb-agg-distinct)

| Measure | P22 | P23 | Prediction |
|---|---|---|---|
| P1 query records passing | 9,143 of 9,202 | 9,181 (99.77 percent) | 9,175 to 9,185: held |
| `slt_lang_aggfunc.test` | 23 of 67 | 61 | at least 60: held |
| rejected | 50 | 8 | |
| wrong | 9 | 13 | |
| statements as expected | 1,938 | 1,938 | not below 1,938: held |

- No other file changed. The 4 more wrong answers are the integer-overflow records of A1 (sum and
  total over 1<<63, which SQLite itself does not reproduce), now answered instead of refused.
  The other 2 wrong aggfunc records read `min(y)` where y holds the text 'NULL'; the runner
  reads a printed NULL as SQL NULL (A5), so qdb's 'NULL' text and SQL NULL look alike to it.
- Planted check MISSED its bar: the build that ignores DISTINCT in SUM, AVG and TOTAL passes 55
  aggfunc records, 6 fewer (bar: at least 8 fewer). It separates, by less than predicted.
- Remaining P1 rejections: 6 views (`unexpected trailing input`), 2 dropped tables read later.
- test_qdb.sh 308 pass, 0 fail (the fail-closed DISTINCT check became 8 positive checks, each
  matched against Python's sqlite3 first). Guards: crash 176 points, 0 violations; power cuts 0 of
  10,000, digest unchanged; P2 2,000 of 2,000; text 0 changed; WAL recovery 0; reference
  differential 16 of 16; library 0 failures; receipts reproduced; test_qdb_mutations.py 0.

## After P24: REPLACE (2026-11-09, feat/bdb-replace)

| Measure | P23 | P24 | Prediction |
|---|---|---|---|
| P1 query records passing | 9,181 of 9,202 | 9,185 (99.82 percent) | 9,185: held |
| `slt_lang_replace.test` | 2 of 6, statements 4 of 8 | 6 of 6, statements 8 of 8 | |
| statements as expected | 1,938 | 1,942 | 1,942: held |

- No other file changed. Planted check caught: a build whose REPLACE removes nothing answers 2 of
  the replace file's 6 queries wrong, fails 2 of its statements, and fails 4 of the 6 new
  test_qdb.sh checks.
- test_qdb.sh 314 pass, 0 fail. Guards: crash 176 points, 0 violations; power cuts 0 of 10,000,
  digest unchanged; P2 2,000 of 2,000; text 0 changed; WAL recovery 0; reference differential 16
  of 16; library 0 failures; receipts reproduced; test_qdb_mutations.py 0 disagreements. The
  receipts overhead that run printed (-27.7 percent, one repetition while sqllogictest ran beside
  it) shows that single-repetition overhead figures on a loaded machine carry no information.

## After P25: prepared statements and a row API in bdb/lib (2026-11-09, feat/bdb-rowapi)

Windows, one core, `bench_qdb_throughput.py --apis text,prep`, 10,000 rows, 2,000 statements in one
transaction, median of 3; SQLite 3.49.1 from the amalgamation. Another agent's compiler builds ran
on the same machine during the run.

| API, mode | qdb (statements per second) | SQLite | qdb's time against SQLite's | Before |
|---|---|---|---|---|
| text, select | 21,751 | 305,764 | 14.06 times | 12.9 (P21) |
| text, update | 37,284 | 275,406 | 7.39 times | 7.5 |
| text, mixed | 28,575 | 237,051 | 8.30 times | 8.3 |
| prep, select | 1,607,717 | 2,624,672 | 1.63 times | none (no API) |
| prep, update | 753,012 | 728,863 | 0.97 times | none |
| prep, mixed | 1,044,386 | 959,233 | 0.92 times | none |

- Both engines end in the same state under both APIs (SUM(v) 499,600).
- Predictions: text held (select 14.06 within 10 to 15; update and mixed within 5 to 10). Prep
  select MISSED (1.63 against 0.8 to 1.6, the slow side). Prep update (0.97 against 1.2 to 2.5)
  and mixed (0.92 against 1.0 to 2.0) also fell outside their ranges, on the fast side. Target
  (below 1): met by update and mixed in this run, missed by select. The margins are inside this
  machine's run-to-run spread: in exploratory single runs SQLite's prepared update ran between
  0.73 and 1.43 million per second, so "faster than SQLite" on update and mixed is not settled by
  one median of 3.
- Where qdb's time goes (exploratory, cycle counts on the prepared update): the WAL line (one
  logical OP line, hashed and held, about a third), the index search (pool ids of keys spread over
  the string pool, so most probes miss the cache), and one new pool string per changed cell. These
  are the targets of the storage work (speed item 2): typed key arrays in the index, and cells that
  are updated in place instead of appended to the pool.
- Correctness: `test_qdb_rows.py`, seeds 1 to 3, differential and keyed workloads (1,029
  statements, 540 bound parameters): bound against literal 0 differences, .db and .wal identical,
  literal rows against the CLI 0 differences. Planted (index search accepts larger keys): 186
  differences, caught.
- Guards: test_qdb.sh 314 pass, 0 fail; crash test 176 points, 0 violations; seeded power cuts 0
  of 10,000, digest 77a49282... unchanged; P2 2,000 of 2,000; text 0 changed; WAL recovery 0
  failures; reference differential 16 of 16; library 0 failures; receipts reproduced; mutations 0
  disagreements.

## After P26: the prepared comparison settled (2026-11-09, feat/bdb-settle)

Windows, one core per process, prep API, 10,000 rows, qdb and SQLite 3.49.1 alternating, 11 pairs
per mode (planted check 7). Run at 10:28 to 10:33 PT with no cargo, rustc or render job and 10.7 GB
free; a GPU scoring job held one of the 24 cores. Interval: order statistics of the per-pair ratio
(qdb's time over SQLite's), exact coverage 93.5 percent for 11 pairs, 98.4 percent for 7.

| Run | Mode | qdb (statements per second) | SQLite | Paired median ratio | Interval | Verdict |
|---|---|---|---|---|---|---|
| A (2,000 statements) | select | 2,118,644 | 2,538,071 | 1.23 | 1.18 to 1.29 | slower |
| A | update | 852,152 | 1,040,583 | 1.21 | 1.13 to 1.33 | slower |
| A | mixed | 1,094,092 | 997,506 | 0.86 | 0.76 to 0.95 | faster |
| B (200,000 statements) | select | 2,899,223 | 2,584,013 | 0.89 | 0.84 to 1.08 | unsettled |
| B | update | 1,004,394 | 2,512,153 | 2.51 | 2.43 to 2.60 | slower |
| B | mixed | 1,190,221 | 2,465,878 | 2.05 | 1.99 to 2.08 | slower |
| planted (SQLite against SQLite, B settings) | select | | | 0.99 | 0.88 to 1.02 | brackets 1 |
| planted | update | | | 0.99 | 0.98 to 1.02 | brackets 1 |
| planted | mixed | | | 1.00 | 0.96 to 1.06 | brackets 1 |

- The claim is settled against qdb: on long runs SQLite's prepared UPDATE runs at about 2.5 million
  per second, and qdb's at 1.0 million, so qdb takes 2.5 times SQLite's time on update and 2.05
  times on mixed. P25's update and mixed ratios below 1 came from short timed regions where SQLite
  had not yet reached its steady rate (about 1.0 million per second over the first 2,000
  statements, 2.5 million over 200,000). qdb is not faster than SQLite on any mode of the long run;
  point select is level within the interval.
- Run A's mixed result (faster) does not carry to run B and is not claimed as a general result.
- Predictions: select slower in A held; select in B MISSED (0.89, unsettled, predicted 1.4 to 1.9).
  Update and mixed unsettled in A MISSED (update settled slower, mixed settled faster). Update in B
  (0.8 to 1.3) and mixed in B (0.8 to 1.2) MISSED, both on the slow side.
- Planted check held: all three A/A intervals contain 1.0, so the method separates; the measured
  gaps are larger than the A/A spread.
- From here the speed work is measured on run B's settings; run A stays only for continuity with
  P25.
- Guards on the unchanged engine (main a5fb755): test_qdb.sh 314 pass, 0 fail; rows 0 differences
  (planted 186 caught); crash 176 points, 0 violations; power cuts 0 of 10,000, digest 77a49282...
  equal; P2 2,000 of 2,000; text 0 changed; WAL recovery 0; reference differential 16 of 16;
  library 0 failures; receipts reproduced; mutations 0 disagreements.

## After P27: typed key array in each index (2026-11-09, feat/bdb-typedkeys)

P26's harness, prep API, 200,000 statements, 11 pairs per mode, qdb before (P26 tip 2a4726b) and
after, each against SQLite 3.49.1 in its own paired run. Run 11:06 to 11:22 PT, no cargo, rustc or
render job, 12.8 GB free. Paired median of qdb's time over SQLite's, with the 93.5 percent interval.

| Rows | Mode | Before | After | Change |
|---|---|---|---|---|
| 10,000 | select | 1.03 (0.93 to 1.23) | 0.82 (0.77 to 0.89) | -20% |
| 10,000 | update | 2.55 (2.50 to 2.62) | 2.34 (2.33 to 2.41) | -8% |
| 10,000 | mixed | 1.87 (1.84 to 1.93) | 1.76 (1.67 to 1.82) | -6% |
| 100,000 | select | 1.59 (1.50 to 2.10) | 1.17 (1.02 to 1.25) | -26% |
| 100,000 | update | 2.98 (2.80 to 3.21) | 2.40 (2.35 to 2.47) | -19% |
| 100,000 | mixed | 2.16 (2.02 to 2.23) | 1.78 (1.63 to 2.02) | -18% |

- qdb's prepared point select on 10,000 rows is now faster than SQLite's by the P26 rule (interval
  0.77 to 0.89, all below 1): 3.14 million per second against 2.56 million. On 100,000 rows it is
  still slower (1.02 to 1.25). Update and mixed stay slower at both sizes.
- Predictions held: select fell 20 and 26 percent (10 to 35); update and mixed fell 8, 6, 19 and 18
  percent (5 to 25); the 100,000-row select gain is the larger.
- Planted check as preregistered MISSED at first: with `ix_move` leaving `idx_num` stale, the rows
  test still passed, because its keyed workload never searched the moved key before something
  rebuilt the array. Amendment A11 (below) adds a bound search of the new value right after each
  `UPDATE t SET v = k` in the second half of the keyed workload. With it the planted build fails
  (seeds 1 to 3: 1, 3 and 1 differences) and the real build passes. The rows test's own planted
  anchor moved to the new search loop (188 differences, caught).
- Guards: test_qdb.sh 314 pass, 0 fail; rows 0 differences; crash 176 points, 0 violations; power
  cuts 0 of 10,000, digest 77a49282... equal (no byte change); P2 2,000 of 2,000; text 0 changed; WAL
  recovery 0; reference differential 16 of 16; library 0 failures; receipts reproduced; mutations
  0 disagreements.

## After P29 and P30: memory that follows the data (2026-11-09, feat/bdb-memory)

Run 13:36 to 14:20 PT on Windows, with other jobs on the machine (about a third of the CPU in
use). Peak commit (PeakPagefileUsage) and peak working set of one `throughput` process: mixed,
prepared API, the load and then 1,000 statements, median of 3 runs. "Before" is the P28 engine
(main at f287cf1); SQLite is 3.54.1.

| Rows | Before, commit | After, commit | After, working set | SQLite, commit | SQLite, working set | After over SQLite, commit |
|---|---|---|---|---|---|---|
| 1,000 | 22.1 MB | 2.1 MB | 6.1 MB | 0.77 MB | 5.4 MB | 2.8 |
| 2,000 | 116.8 MB | 3.4 MB | 7.0 MB | 0.77 MB | 5.4 MB | 4.4 |
| 4,000 | 437.6 MB | 5.4 MB | 8.8 MB | 0.77 MB | 5.4 MB | 7.0 |
| 8,000 | 953.8 MB | 9.7 MB | 12.1 MB | 0.78 MB | 5.4 MB | 12.3 |
| 10,000 | not run | 10.2 MB | 13.0 MB | 0.88 MB | 5.4 MB | 11.6 |
| 50,000 | not run | 45.6 MB | 39.0 MB | 1.31 MB | 5.6 MB | 34.8 |
| 100,000 | not run | 89.1 MB | 71.9 MB | 2.00 MB | 6.1 MB | 44.6 |

- At 8,000 rows the peak fell from 953.8 MB to 9.7 MB (99 times less). The three runs of a cell
  differ by under 5 percent.
- Prediction 1 (linear in rows): 883 bytes per row from 10,000 to 50,000 and 871 from 50,000 to
  100,000, 1.4 percent apart (bound 15): held.
- Prediction 2 (within 10 times SQLite at 100,000 rows): MISSED, 44.6 times on commit (expected
  about 45) and 11.7 times on working set. qdb keeps every row in memory as text cells plus the
  string pool and index arrays, about 880 bytes per row here; SQLite keeps a page cache and reads
  the rest from the file. Closing this needs the storage change (typed cells, pages on disk), which
  is the next decision, and it is open.
- CLI, same statements as a script (output and `.db` byte-equal before and after): 2,000 inserts
  with no transaction 75.6 MB to 4.4 MB; 8,000 in one transaction 486 MB to 9.4 MB; 8,000 plus
  1,000 mixed 950 MB to 12.4 MB.

Speed, prediction 3: prepared API, 200,000 statements, 11 pairs, paired median of qdb's time over
SQLite's with a 93.5 percent interval. The process ran at below-normal priority on a loaded
machine; pairing holds the two engines to the same conditions within a pair.

| Rows | Mode | P28 published (SQLite 3.49.1) | Before, this session (3.54.1) | After (3.54.1) | After (3.49.1, exploratory) |
|---|---|---|---|---|---|
| 10,000 | select | 0.56 | 0.56 (0.55 to 0.58) | 0.51 (0.50 to 0.54) | not run |
| 10,000 | update | 1.32 | 1.19 (1.16 to 1.22) | 1.19 (1.17 to 1.21) | not run |
| 10,000 | mixed | 1.00 | 1.01 (0.88 to 1.07) | 0.86 (0.85 to 0.91) | not run |
| 100,000 | select | 1.13 | not run (A14) | 1.39 (1.34 to 1.48) | 1.24 (1.13 to 1.37) |
| 100,000 | update | 1.74 | not run (A14) | 1.94 (1.83 to 2.02) | 1.96 (1.53 to 2.13) |
| 100,000 | mixed | 1.34 | not run (A14) | 1.81 (1.67 to 1.97) | 1.55 (1.45 to 1.68) |

- 10,000 rows: no ratio rose (held). Against the same-session before build, select is faster
  (intervals apart), mixed is lower with overlapping intervals (0.85 to 0.91 against 0.88 to 1.07),
  and update is level. Select and mixed on 10,000 rows are faster than SQLite 3.54.1; update is
  slower.
- 100,000 rows: MISSED on all three (23, 11 and 35 percent above P28's values). The cause is not
  settled. qdb's own rate is higher than in P28's session (select 2.32 million per second against
  1.86, update 1.64 against 1.30, mixed 1.70 against 1.58), and SQLite's rate is higher by more
  (select 3.35 million per second, where P28's ratio implies about 2.1). SQLite 3.49.1 in this
  session also runs at 3.5 million, so the release does not explain it; the session differs.
  Whether P29 itself cost speed at 100,000 rows is unknown, because the build before cannot run
  there inside the memory limit (A14). Everything at 100,000 rows is slower than SQLite.
- A/A planted check (SQLite in both places, 10,000 rows): 1.02 (0.95 to 1.10), 1.00 (0.98 to
  1.04), 0.95 (0.92 to 1.01); each interval holds 1.
- Planted checks: parser lists emptied in place inside a subquery too: `test_qdb.sh` 313 pass, 1
  fail (`subq_in`), caught. Held WAL line without its newline piece: WAL recovery 1 failure with
  either writer, 242 violations in 10,000 power cuts, digest 040128cf..., `test_qdb.sh` 3 fail,
  caught. `con_key` with `;`: MISSED (314 pass; A13); the replacement plant fails 24.
- Guards: `test_qdb.sh` 314 pass, 0 fail; rows 0 differences; crash 176 points, 0 violations; power
  cuts 0 of 10,000, digest 77a49282... equal; P2 2,000 of 2,000; text 0 changed; WAL recovery 0 (old
  reference writer) and 0 (P28 build writer); reference differential 16 of 16; library 0 failures;
  receipts reproduced; mutations 0 disagreements; sqllogictest 9,185 of 9,202.

P30, `bdb/tests/test_qdb_memory.py` on the committed tree, Windows, 3 runs (slope of peak commit
between 20,000 and 80,000 statements):

| Workload | Slope, bytes per statement (3 runs) | Bound (1.5 times the median, next 100) |
|---|---|---|
| load | 990.3, 988.3, 987.8 | 1,500 |
| select | 1,206.8, 1,206.3, 1,206.7 | 1,900 |
| update | 1,039.4, 1,044.8, 1,046.1 | 1,600 |

- Planted build (`reset_fresh` forced true): 4,434.8, 4,434.0 and 4,437.4 bytes per statement on
  select, above the 1,900 bound in all 3 runs: caught. The test and the plant run in CI.
- The CLI still keeps about 1.2 KB per SELECT and 1.0 KB per UPDATE for the life of the process
  (statement text, tokens and result text that the runtime never frees), so a long-lived process
  grows by that much per statement. The bounds catch a regression from here. Removing the growth
  itself is open work.

## After P28: update path costs (2026-11-09, feat/bdb-updpath)

P27's runs, against the P27 merge (c6397aa) as "before". Run 11:46 to 12:02 PT, no cargo, rustc or
render job. Paired median of qdb's time over SQLite 3.49.1's, 93.5 percent interval; qdb's rate in
statements per second.

| Rows | Mode | Before | After | Change | qdb before | qdb after |
|---|---|---|---|---|---|---|
| 10,000 | select | 0.55 (0.55 to 0.60) | 0.56 (0.55 to 0.57) | +2% | 4,361,004 | 4,667,554 |
| 10,000 | update | 2.25 (2.18 to 2.39) | 1.32 (1.28 to 1.40) | -41% | 1,084,875 | 1,927,358 |
| 10,000 | mixed | 1.48 (1.42 to 1.65) | 1.00 (0.96 to 1.05) | -32% | 1,563,172 | 2,541,587 |
| 100,000 | select | 1.14 (1.08 to 1.23) | 1.13 (1.09 to 1.33) | -1% | 1,963,653 | 1,864,976 |
| 100,000 | update | 2.49 (2.36 to 2.64) | 1.74 (1.59 to 1.95) | -30% | 828,892 | 1,297,497 |
| 100,000 | mixed | 1.91 (1.83 to 2.03) | 1.34 (1.25 to 1.64) | -30% | 1,065,042 | 1,582,917 |

- Point select on 10,000 rows stays faster than SQLite (0.55 to 0.57); mixed on 10,000 rows is level
  (0.96 to 1.05, unsettled); update at both sizes and everything at 100,000 rows stays slower.
- Predictions: update fell 41 and 30 percent (10 to 30: MISSED at 10,000 on the fast side, held at
  100,000); mixed fell 32 and 30 percent (5 to 20: MISSED twice, fast side); select moved under 10
  percent (held).
- Spread between sessions: the same engine (P27) measured 0.82 on 10,000-row select in P27's run
  and 0.55 here, with qdb at 3.14 and 4.36 million per second. Pairing removes SQLite's drift within
  a run; it does not remove a change in qdb's own rate between processes or sessions (memory layout
  of the pool is the leading guess, not tested). Before/after comparisons are made only within one
  session, as here.
- Planted checks: the 2^32 - 2 FNV mask did NOT change the power-cut digest (MISSED as
  preregistered): writer and reader in that test are the same build, so a self-consistent hash
  change is invisible to it. Amendment A12: the WAL recovery test also runs with the previous build
  as the writer; the planted build then fails 223 recovery checks and the real build 0. The `sp_int`
  build that returns r - 1's entry fails the rows test (32 differences over two seeds, and .db/.wal
  differ), caught.
- Guards: test_qdb.sh 314 pass, 0 fail; rows 0 differences; crash 176 points, 0 violations; power
  cuts 0 of 10,000, digest 77a49282... equal; P2 2,000 of 2,000; text 0 changed; WAL recovery 0 (old
  reference writer) and 0 (P27 build writer); reference differential 16 of 16; library 0 failures;
  receipts reproduced; mutations 0 disagreements.

## Stage 1: a correct, durable single-file engine

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 1a Regression gate | `bdb/tests/test_qdb.sh` reports any FAIL | PASS | 290 pass, 0 fail after P13; 277 after P12; 274 after P11; 257 after P10; 250 after P9; 244 after P8; 241 after P7; 222 after the completion changes (214/0/1 at the move) |
| 1b SQLite differential | any of the P2 queries disagrees, with numbers compared as numbers | PASS | 2,000 of 2,000 after P8, a CI gate; 1,944 after P7; 1,251 at the move (519 null_vs_empty, 230 rows, 46 number_text) |
| 1c Crash recovery | any P3 crash point breaks the rule, or any P15 seed does | PASS | P21: 0 of 10,000 seeds and 0 of 176 crash points after transactions write once at COMMIT. P15: 0 violations in 10,000 seeded power cuts (dropped, torn and reordered unflushed writes) after P15b, 2 before; hashes equal on Windows and Linux for 10,000 of 10,000. Process stops: 0 violations of 194 after P14; 0 of 170 with `.ckpt`; 24 of 104 at the move |
| 1d Cross-process transaction | either P4 case misses | PASS | four checks in test_qdb.sh (`xproc_*`), differential process mode equals REPL mode on 8 seeds |
| 1e Text round trip | any P5 value changes | PASS | 2,000 of 2,000 (`test_qdb_text.py`); format 0 files still load |
| 1f NULL apart from '' | `INSERT (NULL)` and `INSERT ('')` are indistinguishable by `IS NULL`, or `NOT (NULL = x)` is true | PASS | P7: NULL is its own value with three-valued logic; 19 checks in test_qdb.sh; P2 null_vs_empty 519 to 0. Before: both stored an empty cell |
| 1h History, AS OF and receipts | a second process with `AS OF` fails to reproduce any of the P16 results, or receipts cost 5 percent of query time or more | FAIL | P16b: 1,000 of 1,000 reproduced on Windows and Linux; overhead 13.0 percent (Linux) and 5.7 percent (Windows) with the runtime SHA-256; P16: 44 and 347 percent with SHA-256 written in BuildLang |
| 1g JOIN gaps that fail closed | a secondary ORDER BY key over a grouped JOIN, DISTINCT over a grouped JOIN, or COUNT(DISTINCT) over any JOIN is refused | FAIL | each refused with a specific message (`bdb/README.md`, Known limitations) |

## Stage 2: SQL coverage against the outside corpus

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 2a sqllogictest queries | any P1 query record does not pass | FAIL | 9,185 of 9,202 (99.82 percent) after P24; 9,181 (99.77 percent) after P23; 9,143 (99.36 percent) after P22; 9,087 (98.75 percent) after P18; 8,921 (96.95 percent) after P12; 8,186 after P11 (`select5` not run); 1,084 after P10; 32 at the move; `bdb/tests/slt_run.py` |
| 2b Expressions in the select list | `SELECT a+1, a*b-c FROM t` refused or wrong | PASS | P11: arithmetic, CASE, functions, correlated subqueries; `select1` to `select3` 5,320 of 5,320 |
| 2c Column types with arguments and constraints | `VARCHAR(30)`, `INTEGER PRIMARY KEY`, `UNIQUE` refused | PASS | P9 type arguments; P13 PRIMARY KEY, UNIQUE and NOT NULL enforced on INSERT and UPDATE, kept in the file and through ROLLBACK; statements 1,931 of 1,993 |
| 2d CASE, scalar functions (abs, coalesce) | refused or wrong | PASS | P11: searched and simple CASE, `abs`, `coalesce`, `ifnull`, `nullif` |
| 2e Correlated subqueries, EXISTS | refused or wrong | PASS | P11: scalar, EXISTS and IN subqueries naming outer columns (`x_corr_subq`, `x_exists`) |

## Stage 3: scale

| Row | Check (fails if) | Status | Evidence |
|---|---|---|---|
| 3a Bulk load is linear | `bench_qdb_load.py` log-log slope k above 1.3 for autocommit or transactional indexed load | PASS | HarperZ9/build-universe#35: autocommit k 1.16, transactional indexed k 1.02. Limit stated there: interleaved INSERT plus indexed SELECT stays superlinear (k 1.52). Not re-run after the checkpoint change, which writes the state twice per checkpoint |
| 3b ORDER BY is n log n | P6 ratio 30k/10k above 6 | PASS (single-table) | 3.29 after, 9.51 before (`bench_qdb_sort.py`). The UNION, grouped, JOIN and grouped-JOIN sorts are still selection sorts and were not measured |
| 3d Throughput through the library | a point select or update loop through `bdb/lib` takes more than 10 times SQLite's time on the same machine | FAIL | P21: update 7.5 times and mixed 8.3 times (within 10), point select 12.9 times (above 10); P20: 12.1, 102.9, 51.6; P19: 10.0, 2,472, 2,185 (10,000 rows, Windows) |
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
