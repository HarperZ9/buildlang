# Changelog

All notable changes to the BuildLang program suite.

## [Unreleased]

### qdb: Loading N Rows Costs O(N), Not O(N^2) (2026-11-02)

**A bulk load is now linear in the rows loaded, and 70 to 170 times faster
at 1,000 rows.** Before, every INSERT paid for everything already in the
database, in five places. `sp_add` grew the string pool by concatenation, which
copies the whole pool. `wal_write` read the whole WAL and wrote it back for each
line. Every auto-commit statement rewrote the whole .db. `exec_insert` copied the
whole cells array. And `update_indexes_for_table` re-sorted every index on the
table with a selection sort, so an indexed load was closer to O(N^3).

- The string pool is a `Vec<String>`; `sp_add` and `sp_get` are O(1).
- The WAL is append-only (`append_file`). An auto-commit statement appends its
  `TXID / OP / COMMIT` record in one write. Replay is one linear pass.
- The .db is rewritten at a checkpoint: when the WAL outgrows the .db by 64 KB,
  and at a clean exit, never inside an open BEGIN. Save cost is amortized
  against WAL growth.
- `db_save` builds its output in 64 KB chunks joined once; `db_load`,
  `wal_replay` and `.wal` split their file once instead of slicing the rest of
  the file per line.
- INSERT into the last table appends cells in place.
- Indexes refresh lazily on the next index read. Appended rows are sorted alone
  and merged in; UPDATE, DELETE, ALTER and ROLLBACK trigger one full rebuild.
  Index sorts use a stable merge sort whose tie order matches the old
  selection sort exactly.

Crash safety is at least as good as before. A statement is durable once its
COMMIT line is in the WAL, the same point the engine relied on before. The old
code truncated and rewrote both the WAL and the .db on every statement, so a
crash during either could lose committed data; an append can tear only the last
record, and a torn record has no COMMIT and is skipped. No write calls fsync,
before or after.

Trade-off: an index read right after writes pays for folding them in, so a
workload that alternates INSERT with indexed SELECT is still superlinear
(k = 1.52 over N = 1k..16k), though 170 times faster at 1k. The .db file now
lags the WAL during a long REPL session; it is current again at every clean
exit, and `.wal` reports the pending records.

Measured with the new `benchmarks/bench_qdb_load.py` (k is the log-log slope of
time against N; Windows 11, buildc 1.4.0, gcc 13.2 -O2, one run per size):

| Scenario | Before: time at N | Before: k | After: time at N | After: k |
|----------|-------------------|-----------|------------------|----------|
| autocommit | 15.0 s at 1k, 97.8 s at 2k | 1.78 (N=500..2k) | 0.13 s at 1k, 27.6 s at 100k | 1.16 (N=1k..100k) |
| txn | 10.0 s at 1k, 40.6 s at 4k | 1.01 (N=1k..4k) | 0.14 s at 1k, 18.5 s at 100k | 1.07 (N=1k..100k) |
| txn_indexed | 21.3 s at 1k | 1.94 (N=250..1k) | 0.14 s at 1k, 15.0 s at 100k | 1.02 (N=1k..100k) |
| interleaved | 32.7 s at 1k | 1.61 (N=250..1k) | 0.20 s at 1k, 13.3 s at 16k | 1.52 (N=1k..16k) |

Verified:
- `tests/test_qdb.sh`: 214 pass, 0 fail, 1 documented xfail, the same as before.
- `tests/test_qdb_differential.py` (new): 8 seeded random workloads (indexes,
  multi-row INSERT into either of two tables, UPDATE, DELETE, COMMIT and
  ROLLBACK, indexed and range SELECTs, aggregates, GROUP BY), each run as one
  REPL session and as one process per statement, through the build before this
  change and the build after it: stdout and final .db byte-identical, 16 of 16.
  A mutant that flips the merge sort's tie order fails 6 of 6 checks.
- `tests/test_qdb_wal_recovery.py` (new): a WAL left by an abnormal exit, cut at
  every byte offset (315 cuts) and at 202 offsets of a 98 KB WAL whose prefix
  was already checkpointed into the .db; recovered rows match the rule "a
  transaction is applied if and only if its COMMIT survives", 0 failures, and
  the old build recovers the small case's 315 cuts identically. A mutant that
  also replays the open transaction fails 399 checks.

### qdb: A Statement Must Consume Its Whole Input (2026-09-05)

**A token left over after a complete parse now fails closed instead of being
silently dropped.** Before this, `SELECT name FROM emp WHERE salary > 80 EXTRA`
ran the query and discarded `EXTRA`, returning a plausible answer to a
malformed request; `DELETE FROM emp WHERE id = 1 EXTRA` deleted the row and
discarded the tail, applying a half-specified write. A parser that stops early
and keeps going is the silent-wrong-answer failure a query engine must never
have. Every statement kind now checks that the cursor sits at end of input, at a
lone trailing `;`, or (for SELECT) at a UNION keyword, and otherwise prints
`unexpected trailing input near '<token>'` and exits non-zero.
- The SELECT check lives in `parse_select`, after the OFFSET clause. It tolerates
  a trailing `;` and a following UNION keyword; anything else is a clause qdb does
  not parse or plain garbage. Because `exec_select_union` re-enters `parse_select`
  per branch, the same check guards the tail after the final UNION branch, closing
  that path for free.
- Mutations get a backstop in `exec_sql` after the parse and before the WAL write,
  so a rejected INSERT, UPDATE, DELETE, CREATE, or DROP mutates nothing. SELECT is
  exempt there because its own check already ran inside `parse_select`.
- `parse_order_by` now consumes an aggregate secondary key in full
  (`ORDER BY x, SUM(col)`), parentheses included, so a well-formed grouped-JOIN
  secondary sort no longer leaves a dangling `(` to trip the new end-of-input check.
- A single bareword after a table name stays a legal alias (`SELECT id FROM emp e`),
  so the guard rejects a second trailing bareword, not the first.
- Gate: 214/0/1/0 (up from 204), db.bld 6,861 -> 6,913. Ten controls added: seven
  errchk cases (SELECT WHERE/ORDER BY/LIMIT, DELETE, INSERT, UPDATE, CREATE, each
  asserting the error text and the table left intact), one after-alias case, one
  UNION final-branch case, and a positive companion proving a single alias still
  renders. Rebuilt from the pre-fix HEAD, the nine trailing-garbage controls fail
  (205/9), so each earns its place.

### qdb: A Positional Ordinal Orders Grouped Output on Both Paths (2026-09-05)

**`ORDER BY 2` now orders a grouped result by its second output column, on both the
single-table grouped path and the grouped JOIN.** Before this a bare integer key
was read as a column name, matched nothing, and failed closed with "not a grouped
column, aggregate, or alias". The non-grouped and UNION paths already resolved
ordinals, so `ORDER BY 2` worked everywhere except a grouped result; this closes
that gap and brings the ordinal surface to parity across all four ORDER BY paths.
- Both grouped sort blocks (`exec_select` single-table and `exec_select_join`) gain
  one branch ahead of the name and alias resolution: a key that `is_int_str`
  parses is a 1-based ordinal into the `ncols` output columns. In range it sets the
  output column directly and takes the sort type from that column's own aggregate
  func, so `ORDER BY 2` sorts a SUM numerically while `ORDER BY 1` sorts a text
  group column as text. Out of range (position 0, or past the last column) fails
  closed with "ORDER BY position N is out of range".
- A secondary ORDER BY key over a grouped JOIN still fails closed (an honest
  partial, candidate next fork); an unresolvable name still fails closed with its
  own message. LIMIT/OFFSET compose after the ordinal sort unchanged.
- Gate: 204/0/1/0 (up from 197), db.bld 6,833 -> 6,861. Seven assertions added
  across the two paths; a false-success control built from the pre-fix HEAD fails
  exactly the eight ordinal assertions (196/8), so each earns its place.

### qdb: LIMIT and OFFSET Over a Grouped JOIN Truncate the Groups (2026-09-05)

**LIMIT and OFFSET over a grouped JOIN now truncate the group rows instead of
failing closed.** The ORDER BY entry below still left LIMIT over a grouped JOIN
fail-closed. This completes the top-n idiom: `ORDER BY SUM(emp.salary) DESC LIMIT
3` returns the three highest-total groups. DISTINCT over a grouped JOIN and
COUNT(DISTINCT) over any JOIN still fail closed.
- After `exec_select_join` folds each group's row, applies the HAVING filter, and
  runs the ORDER BY sort, it truncates the group rows with the same loop the
  single-table grouped path uses: skip OFFSET whole group rows, then take up to
  LIMIT, each row being `ncols` pool ids. The count `gkc` and the `grows` pool are
  rewritten to the surviving rows, so every downstream width and render loop is
  unchanged.
- LIMIT runs after the sort, so it selects the top-n of the ordered groups; LIMIT
  with no ORDER BY keeps group-discovery order. OFFSET skips whole groups, HAVING
  runs before the truncation, and LIMIT 0 or an OFFSET past the last group leaves a
  header with zero data rows rather than all groups or an error.
- Tests added: `join_group_limit_topn` / `_desc_topn` (ORDER BY an aggregate then
  LIMIT, both directions), `_offset` (LIMIT with OFFSET), `_noorder` (LIMIT keeps
  discovery order), `_having` (HAVING then ORDER BY then LIMIT), `_zero_hdr` (LIMIT
  0 renders the header), and `_zero_rows` (LIMIT 0 renders no data rows). The six
  value and header assertions fail on the pre-fix binary, which answered every
  LIMIT/OFFSET over a grouped JOIN with `LIMIT over a grouped JOIN is not
  supported` (control run: 6 fail); `_zero_rows` passes on both binaries by design,
  since the fail-closed message also renders no data rows. The qdb gate is now 197
  pass, 0 fail, 1 XFAIL, 0 XPASS.

### qdb: ORDER BY Over a Grouped JOIN Sorts the Groups (2026-09-05)

**ORDER BY over a grouped JOIN now sorts the folded groups instead of failing
closed.** The HAVING entry below still left ORDER BY over a grouped JOIN
fail-closed. This sorts the grouped output. A secondary key, a positional
ordinal, an unresolvable name, DISTINCT, and LIMIT over a grouped JOIN still fail
closed, as does COUNT(DISTINCT) over any JOIN.
- After `exec_select_join` folds each group's row and applies the HAVING filter,
  it resolves the ORDER BY key to an output column: a same-kind match against the
  projection's aggregate functions and argument columns, then a projection-alias
  match. A key that resolves to none fails closed with `ORDER BY term '...' is not
  a grouped column, aggregate, or alias`, and a secondary key fails closed with
  its own message.
- The sort type follows the resolved column. COUNT sorts as an integer,
  SUM/AVG/MIN/MAX as a real, and a plain grouped column by its declared source
  type, so a text key compares lexically. A stable selection sort over the folded
  group rows keeps equal keys in group-discovery order, so `ORDER BY COUNT(*) ASC`
  with a tie holds the discovery order of the tied groups.
- The sort runs on the already-folded, already-HAVING-filtered group rows, so
  WHERE, HAVING, and the empty-result path are untouched: an ORDER BY over a
  result that HAVING emptied still renders a header and zero rows.
- Tests added: `join_group_order_agg_asc` / `_agg_desc` (order by an aggregate,
  both directions), `_col_desc` (order by the grouped column), `_real_max` (a
  real-typed aggregate key), `_alias_asc` (order by a projection alias), and the
  fail-closed trio `_secondary_closed`, `_ordinal_closed`, `_unresolv_closed`. All
  eight fail on the pre-fix binary, which answered every ORDER BY over a grouped
  JOIN with the blanket `ORDER BY over a grouped JOIN is not supported` (control
  run: 8 fail). The former `join_group_order_closed` fail-closed assertion is
  replaced by these. The qdb gate is now 190 pass, 0 fail, 1 XFAIL, 0 XPASS.

### qdb: HAVING Over a Grouped JOIN Filters Groups (2026-09-05)

**HAVING over a grouped JOIN now folds its aggregate per group and keeps only the
groups that pass, instead of failing closed.** The entry below folded a grouped
aggregate over a JOIN but kept `HAVING` fail-closed. This filters the grouped
output. ORDER BY, a bare GROUP BY with no aggregate, and COUNT(DISTINCT) over a
JOIN still fail closed.
- After `exec_select_join` enumerates the distinct group keys, it resolves the
  HAVING aggregate's column against table1 then table2 (COUNT(*) needs none,
  failing closed with `HAVING column '...' not found in either table` when
  neither owns it), folds the HAVING aggregate over each key's bucket with
  `join_having_agg`, and keeps only the keys whose value passes `eval_where`.
  Trimming the group-key list leaves every downstream fold, width, and render loop
  unchanged.
- `join_having_agg` mirrors the projection fold: COUNT(*) counts bucket pairs,
  COUNT(col) counts non-empty cells, SUM/AVG/MIN/MAX read the referenced column,
  and a text-typed MIN/MAX compares lexically. The HAVING aggregate is independent
  of the projected one, so `SELECT emp.dept, COUNT(*) ... HAVING MAX(emp.salary)
  >= 120` filters on the max while displaying the count.
- A bad HAVING operator is still rejected at parse time, so the executor never
  sees a stray operator. When every group fails the test the result is an
  empty-but-well-formed table: header, dashed separator, zero rows, exit 0.
- Tests added: `join_group_having_sum`, `join_group_having_cnt`,
  `join_group_having_maxcross` (HAVING aggregate differs from the projected one),
  `join_group_having_text` (TEXT-typed HAVING), and `join_group_having_emptyhdr` /
  `join_group_having_emptyrows` (all groups filtered leaves a header and zero
  rows). The first five fail on the pre-fix binary; the empty-rows guard passes on
  both because the fail-closed message also renders zero rows (control run: 5
  fail). The former `join_group_having_closed` fail-closed assertion is replaced by
  these value assertions. The qdb gate is now 183 pass, 0 fail, 1 XFAIL, 0 XPASS.

### qdb: GROUP BY Over a JOIN Folds Per Group (2026-09-05)

**A grouped aggregate over a JOIN now computes per group instead of failing
closed.** The entry below folded a scalar aggregate over the joined rows but kept
`GROUP BY` over a JOIN fail-closed. This turns the grouped case into a correct
result. HAVING, ORDER BY, and COUNT(DISTINCT) over a grouped JOIN still fail
closed.
- `exec_select_join` resolves the single group column against table1 then
  table2, failing closed with `GROUP BY column '...' not found in either table`
  when neither owns it. It assembles the matched pairs once, honoring WHERE and
  LEFT JOIN null-extension, then enumerates the distinct group keys in first-seen
  order.
- Per group key it folds each projection item over that key's bucket, reusing the
  scalar path's fold. COUNT(*) counts the bucket's pairs, COUNT(col) counts the
  bucket's non-empty cells, SUM/AVG/MIN/MAX read the referenced column, a plain
  column returns the bucket's first value, and a text-typed MIN/MAX compares
  lexically. A WHERE that empties a group drops that group from the output.
- The render mirrors the single-table grouped render: an alias-aware header, a
  dashed separator, and one padded line per group.
- The clauses that the grouped fold does not yet handle each fail closed with a
  specific message: `HAVING over a grouped JOIN is not supported`, `ORDER BY over
  a grouped JOIN is not supported`, `DISTINCT over a grouped JOIN is not
  supported`, `LIMIT over a grouped JOIN is not supported`, and
  `COUNT(DISTINCT) over a JOIN is not supported`. Each is an honest partial,
  never a silent wrong grouping.
- Tests added: `join_group_sum`, `join_group_cnt`, `join_group_minmax`,
  `join_group_avg`, `join_group_where`, and `left_join_group_cnt`,
  `left_join_group_cntnote` assert the folded per-group values; each is a
  fail-closed error on the pre-fix binary (control run: 7 fail). `join_group_sum_closed`
  and `join_group_cnt_closed` become the value assertions above; new
  `join_group_having_closed`, `join_group_order_closed`, and `join_group_noagg_closed`
  hold the remaining fail-closed cases. The qdb gate is now 178 pass, 0 fail,
  1 XFAIL, 0 XPASS.

### qdb: A Scalar Aggregate Over a JOIN Folds the Joined Rows (2026-09-05)

**A scalar aggregate over a JOIN with no GROUP BY now computes over the joined
rows instead of failing closed.** The entry below made this case fail closed to
stop a silent wrong answer. This turns the same case into a correct result;
GROUP BY and COUNT(DISTINCT) over a JOIN stay fail-closed.
- `exec_select_join` resolves both tables' columns first, then detects a real
  aggregate by scanning `p_agg_funcs` for a non-zero entry (COUNT, SUM, AVG,
  MIN, MAX), not by testing `p_agg_count`, which counts plain columns too.
- For an aggregate query with no GROUP BY it assembles the matched pairs once,
  honoring WHERE and LEFT JOIN null-extension, then folds each projection item.
  COUNT(*) counts joined rows, COUNT(col) counts non-empty cells, and SUM, AVG,
  MIN, MAX read the referenced table's column on each pair. The fold mirrors the
  single-table aggregate: numeric MIN/MAX coerce empty cells to 0, a text-typed
  MIN/MAX compares lexically, and MIN/MAX over zero rows yield SQL NULL. A LEFT
  JOIN's unmatched right cells read as blank, so COUNT(right col) skips them.
- GROUP BY over a JOIN fails closed with `GROUP BY over a JOIN is not
  supported`. COUNT(DISTINCT ...) over a JOIN fails closed with `COUNT(DISTINCT)
  over a JOIN is not supported`. Both are honest partials, never a silent wrong
  grouping or count.
- Tests added: `join_agg_count`, `join_agg_sum`, `join_agg_min`, `join_agg_max`,
  `join_agg_avg`, `join_agg_countcol`, `join_agg_where_sum`, `join_agg_where_cnt`,
  `join_agg_min_text`, `join_agg_max_text`, and `left_join_agg_cnt`,
  `left_join_agg_cntnote`, `left_join_agg_sum` assert the folded values; each is
  a fail-closed error on the pre-fix binary (control run: 13 fail). A new
  `join_distinct_closed` joins `join_group_sum_closed` and `join_group_cnt_closed`
  to hold the fail-closed cases. The qdb gate is now 170 pass, 0 fail, 1 XFAIL,
  0 XPASS.

### qdb: An Aggregate or GROUP BY Over a JOIN Fails Closed (2026-09-05)

**An aggregate or a GROUP BY over a JOIN returned a silent wrong answer.** The
join executor projects one output row per joined row and applies neither
aggregation nor grouping, but it accepted queries that asked for both.
- `SUM(emp.salary)` and `MIN(emp.salary)` over a join returned the per-row
  column values with exit 0. A grouped join (`GROUP BY dep.loc` with a `SUM`)
  returned the ungrouped rows. `COUNT(*)` and a grouped `COUNT(*)` tried to
  resolve `"*"` as a column and reported `column '*' not found in either table`,
  a misleading message for an unsupported operation.
- `exec_select` dispatches any query carrying a JOIN to `exec_select_join`
  before the aggregate and GROUP BY paths run, so the Fork above never reached
  these. `exec_select_join` now rejects them up front with one accurate message,
  `aggregates and GROUP BY over a JOIN are not supported`.
- The guard detects a real aggregate by scanning `p_agg_funcs` for a non-zero
  entry, not by testing `p_agg_count`. `p_agg_count` counts every projection
  item including plain columns, so a count-based test would reject every plain
  join; a two-column dotted projection over a join still returns rows.
- Computing the aggregate over the assembled join rows is a larger change,
  recorded as a known limitation rather than attempted.
- Tests added: `join_agg_count_closed`, `join_agg_sum_closed`,
  `join_agg_min_closed`, `join_group_sum_closed`, `join_group_cnt_closed`, and a
  `join_multicol` non-regression proving a plain two-column join still returns
  rows. Each fail-closed control is silent-wrong on the pre-fix binary and errors
  after; `join_multicol` passes on both. The qdb gate is now 159 pass, 0 fail,
  1 XFAIL, 0 XPASS.

### qdb: An Aggregate Over an Unknown Column Fails Closed (2026-09-05)

**An aggregate whose argument named a column no table has returned a silent
wrong answer instead of an error.** The WHERE and HAVING fixes below guard the
condition columns; this guards the aggregate's own argument column.
- `resolve_col` returns -1 for a name no column matches, and every aggregate
  compute site then read zero cells. `SUM(badcol)`, `AVG(badcol)`, and
  `COUNT(badcol)` returned 0. `MIN(badcol)` and `MAX(badcol)` returned blank. A
  grouped `SUM(badcol)` returned 0 for each group. `HAVING SUM(badcol) > 0`
  dropped every group. A scalar subquery `(SELECT AVG(badcol) FROM emp)` yielded
  0, so `salary > 0` matched every row. Each ran with exit 0 and no diagnostic.
- One helper, `agg_cols_ok`, validates each projection aggregate argument and
  the HAVING aggregate argument once, before the aggregate dispatch. It is
  guarded at three entry points: `exec_select` and `exec_select_group` return
  early, and `exec_subquery_scalar` exits so the outer comparison never sees a
  fabricated 0.
- `COUNT(*)` (argument `"*"`) and the `DISTINCT` flag are skipped. A plain
  SELECT column keeps its own existing `column '<name>' not found` path, so the
  helper only inspects aggregate arguments. An aggregate or GROUP BY over a JOIN
  is a separate case, closed by the entry below.
- Tests added: `agg_unknown_sum`, `agg_unknown_avg`, `agg_unknown_min`,
  `agg_unknown_count`, `agg_unknown_group`, `agg_unknown_having_sum`,
  `agg_unknown_having_count`, `agg_unknown_subquery`. Each is silent-wrong on
  the pre-fix binary and errors after. The qdb gate is now 153 pass, 0 fail,
  1 XFAIL, 0 XPASS.

### qdb: HAVING Fails Closed on a Bad Operator (2026-09-05)

**A HAVING clause with an arithmetic or doubled operator dropped every group
and returned an empty result with exit 0: a silent wrong answer.** This is the
HAVING side of the WHERE fix below.
- `parse_having` stored `op_encode(op)` in `having_op`, and `op_encode` returns
  -1 for any token that is not one of `=`, `<`, `>`, `<=`, `>=`, `<>`, `!=`.
  `eval_where` treats -1 as always-false, so `HAVING COUNT(*) + 0 > 1` and
  `HAVING SUM(salary) % 2 = 0` matched no group. `HAVING COUNT(*) == 1` was a
  second path: `=` is one token, so the first parsed as a valid op and the
  second landed in the value position, comparing the aggregate to the symbol
  `"="`, which it never equals.
- Two guards in `parse_having`, mirroring Fork 12's WHERE guards. The first
  rejects an operator `op_encode` does not recognize. The second rejects an
  operator token (type 4) in the value position, catching `==` and `><`.
- A plain-column HAVING (`HAVING dept = 'eng'`) already failed closed earlier,
  in `expect_punct`, because the token after the column is not `(`.
- Tests added: `having_arith_plus_closed`, `having_arith_pct_closed`,
  `having_double_eq_closed`, `having_gtlt_closed`. Each fails on the pre-fix
  binary and passes after. The qdb gate is now 145 pass, 0 fail, 1 XFAIL,
  0 XPASS.

### qdb: WHERE Fails Closed on an Unknown Column (2026-09-05)

**A WHERE clause that named a column no table has returned a silent wrong
answer instead of an error.** A leaf condition stores its left side as a column
ref resolved at eval time, and `resolve_col` returns -1 for a name no column
matches.
- `eval_cond_row` scored that unresolved leaf as false, so `WHERE nosuchcol = 5`
  matched nothing and returned an empty result with exit 0. The IS NULL branch
  scored it true instead, so `WHERE nosuchcol IS NULL` returned every row. Both
  dropped the real intent with no diagnostic, on SELECT, aggregates, GROUP BY,
  UPDATE, DELETE, and scalar subqueries.
- One helper, `where_cols_ok`, validates every WHERE leaf column once before the
  row scan and prints `Error: no such column: <name>`. It reuses `resolve_col`,
  so a dotted `table.col` or an alias ref resolves exactly as it does during
  evaluation, and it runs independently of row count, so an empty table is
  covered too.
- Guards at six single-table entry points call it: `exec_select` (one guard
  covers both its index and full scans and the no-group aggregate path it
  calls), `exec_select_group`, `exec_update`, `exec_delete`, and
  `exec_subquery_scalar`. The five void paths return before any work; the
  subquery, which returns a String and cannot abort just itself, exits the
  statement. UPDATE and DELETE validate before mutating, so a bad column leaves
  the table untouched. The JOIN path uses a separate evaluator and is unaffected.
- Tests added: `where_unknown_select`, `where_unknown_agg`,
  `where_unknown_group`, `where_unknown_update`, `where_unknown_delete`,
  `where_unknown_isnull`, `where_unknown_dotted`, `where_unknown_subquery`. Each
  fails on the pre-fix binary and passes after the guards. The qdb gate is now
  141 pass, 0 fail, 1 XFAIL, 0 XPASS.

### qdb: WHERE Fails Closed on Arithmetic and Doubled Operators (2026-09-05)

**A WHERE condition that used an arithmetic or doubled operator returned zero
rows with exit 0 instead of an error: a silent wrong answer.** The leaf parser
built a condition even when the operator token was not a comparison.
- `op_encode` returns -1 for any token that is not one of `=`, `<`, `>`, `<=`,
  `>=`, `<>`, `!=`. The parser stored that -1 in the leaf, and `eval_where`
  treats -1 as always-false, so `WHERE salary + 10 > 100` and `WHERE salary % 2
  = 0` matched nothing and dropped the real comparison with no diagnostic.
- Two guards in `parse_condition` now fail closed. The first rejects an
  operator that `op_encode` does not recognize, which covers arithmetic in a
  condition and stray symbols like `%`. The second rejects an operator token in
  the value position (token type 4), which catches the C-style `==` (two `=`
  tokens, where the second would be read as the string value `"="`) and `><`.
- A negative literal is a single number token, so `col > -5` is unaffected, and
  a quoted string that looks like an operator (`sym = '<'`) is a string token, so
  it still compares as a value.
- Tests added: `where_arith_plus_closed`, `where_arith_pct_closed`,
  `where_double_eq_closed`, `where_gtlt_closed`. Each fails on the pre-fix binary
  and passes after the guards.

### qdb: Empty-Set MIN and MAX Return SQL NULL (2026-09-05)

**An aggregate MIN or MAX over an empty result set rendered a float sentinel as
a plausible number instead of NULL: a silent wrong answer.** A non-grouped
aggregate returns one row even when the WHERE matches nothing, and MIN/MAX seed
their accumulators to `999999999` / `-999999999`.
- Over zero rows `float_to_str` rendered those seeds as `12157520.92` /
  `-12157520.92`, so `SELECT MIN(salary) FROM emp WHERE id > 999` handed back a
  number for what is really NULL.
- Fix at all three aggregate sites: when the match count is zero, MIN and MAX
  return the empty string, which renders as SQL NULL and mirrors the text path.
  COUNT stays 0; SUM and AVG stay 0 (a kept divergence from SQLite's NULL, where
  0 is a real answer).
- Tests added: `empty_min_num`, `empty_max_num` (the two controls, which fail on
  the prior binary), plus `empty_count`, `empty_sum`, `empty_avg`.

### qdb: MIN and MAX on a TEXT Column Order Lexicographically (2026-09-05)

**MIN and MAX on a TEXT column returned 0 instead of the extreme string: a
silent wrong answer.** Every cell went through `parse_float`, which coerces text
like `alice` to 0.0, so with the numeric sentinels every text extreme collapsed
to 0.
- Fix at all three aggregate sites: read the column type code and, for TEXT,
  track the extreme cell through `compare_vals` (which already orders TEXT
  lexicographically) and return that string. The numeric path is unchanged; SUM
  and AVG on text stay 0, matching SQLite coercion of non-numeric text.
- Tests added: `min_name`, `max_name`, `min_dept`, `max_dept`,
  `group_min_name`, `group_max_name`, `subq_min_name`, `subq_max_name`, plus
  `max_salary` as a numeric-path guard.

### qdb: JOIN and UNION ORDER BY Resolve Ordinals and Fail Closed (2026-09-05)

**The JOIN and UNION sort paths now resolve a positional ordinal and fail closed
on an unknown key, matching the single-table path.** This closes the follow-up
the entry below left open.
- The single-table ORDER BY was already fail-closed and ordinal-aware, but the
  JOIN and UNION sort paths still resolved a key only by name and silently
  returned scan order for an ordinal (`ORDER BY 1`), an out-of-range position, or
  an unknown name. A union has no other portable key: only the first branch's
  column names reach the merged headers, so `ORDER BY 2` was the intended way to
  order it and it did nothing.
- UNION: a new helper, `resolve_union_order_key`, maps the key to an output-column
  position. A bare integer is a 1-based ordinal into the merged columns; anything
  else matches a merged header name (which reflects a first-branch `AS` alias).
  Both the primary and the secondary key route through it, and a position outside
  `1..ncol` or an unmatched name fails closed.
- JOIN: the resolution preamble now takes an ordinal as the Nth projected column
  and maps it back to whichever source table owns that column, reusing the
  existing `sel_table`/`sel_col` projection arrays, so ordinal ordering shares the
  named-key path's numeric comparison, stable tie-break, and LEFT JOIN
  null-extension. A name still resolves against either joined table.
- Honest carve-out: a JOIN ORDER BY resolves a source-table column name or an
  ordinal, but not a projection alias (`... AS k ... ORDER BY k`), which fails
  closed rather than sorting by an unselected column. Single-table and UNION
  selects do resolve an alias. This is safe (fail-closed, never a wrong order) and
  is documented as a known limitation, not silently ignored.
- Both read-only paths (`exec_select_join` is `~ Console`, no Process effect) fail
  closed with an error message and a bare return that leaves the table intact,
  rather than a process exit, so the effect signatures are unchanged.
- Tests added: `join_order_ordinal_left`, `join_order_ordinal_sec`,
  `union_order_ordinal_desc`, `union_order_ordinal_two`, `union_order_ordinal_name`,
  and four fail-closed cases (`join_order_ordinal_oor_closed`,
  `join_order_unknown_closed`, `union_order_ordinal_oor_closed`,
  `union_order_unknown_closed`).
- db.bld: 5,946 lines. Gate: pass=115 fail=0 xfail=1 xpass=0 (the one XFAIL is
  cross-process ROLLBACK, a documented process-model null).

### qdb: ORDER BY Resolves Ordinals and Aliases, Fails Closed on Unknown Keys (2026-09-05)

**The non-grouped single-table ORDER BY now resolves a positional ordinal and a
projection alias, and fails closed when a key names no column.**
- The plain SELECT sort path resolved the ORDER BY key only against real table
  columns. A key that did not match, a positional ordinal (`ORDER BY 2`, standard
  SQL-92), an unknown name (`ORDER BY nonesuch`), or a projection alias on a plain
  select (`SELECT salary AS s ... ORDER BY s`), silently returned discovery order
  with no error. A secondary tie-break key (`ORDER BY dept, s`) failed the same
  silent way. The grouped path already failed closed on an unresolved key; the
  non-grouped path did not, so the two disagreed.
- A single helper, `resolve_order_key`, now maps a key to a table column index by
  three forms: a column name (bare or dotted), a positional ordinal (the Nth
  select column, or the Nth table column under `SELECT *`), and a projection
  alias whose underlying select item is a plain column. Both the primary and the
  secondary key route through it, and an unresolved key prints an error and exits
  non-zero instead of leaving the rows in discovery order.
- An ordinal or alias that points at an aggregate select item does not resolve on
  this path (it never sorts by an aggregate value), so such a key fails closed.
- Tests added: `order_ordinal_proj`, `order_alias_plain`, `order_secondary_ord`,
  `order_secondary_alias`, `order_ordinal_star`, and four fail-closed cases
  (`order_unknown_closed`, `order_ordinal_oor_closed`, `order_ordinal_zero_closed`,
  `order_secondary_closed`).
- Scope: the JOIN and UNION sort paths still ignore an ordinal or unknown key the
  same silent way. They resolve differently (two tables for a join, output-column
  position for a union) and are left as separate follow-ups, not folded in here.
- db.bld: 5,892 lines. Gate: pass=106 fail=0 xfail=1 xpass=0 (the one XFAIL is
  cross-process ROLLBACK, a documented process-model null).

### qdb: Numeric Type Aliases Sort Numerically (2026-09-05)

**A column declared with a common SQL integer or real alias now orders and
right-aligns as a number, matching INTEGER/REAL.**
- The engine recognized only the exact tokens `INTEGER` and `REAL` as numeric.
  Every other spelling (`INT`, `BIGINT`, `SMALLINT`, `TINYINT`, `MEDIUMINT`,
  `FLOAT`, `DOUBLE`, `NUMERIC`, `DECIMAL`) fell through to the TEXT storage code,
  so a column such as `salary INT` compared lexicographically. `ORDER BY salary
  DESC` returned `90, 80, 70, 120, 100` instead of `120, 100, 90, 80, 70`: a
  silent wrong answer, plus left-justified output for what is really a number.
- A single classifier, `type_code_for(type_up)`, now maps a declared type name to
  its storage code (0 INTEGER, 1 REAL, 2 TEXT) and recognizes the aliases. It
  replaces the three inline `INTEGER`/`REAL` checks at the schema-load, CREATE
  TABLE, and ALTER TABLE ADD COLUMN sites, so all three agree by construction.
- WHERE was already correct: `eval_where` auto-detects a numeric operand at
  comparison time regardless of the declared type, so the blast radius was
  ORDER BY and column alignment only.
- Tests added: `alias_int_order_desc`, `alias_int_order_asc`,
  `alias_bigint_order_asc`, `alias_float_order_desc`.
- db.bld: 5,832 lines. Gate: pass=97 fail=0 xfail=1 xpass=0 (the one XFAIL is
  cross-process ROLLBACK, a documented process-model null).

**README reconciled.** The qdb line count was corrected to 5,832 in both the
program table and the section prose (the table row still read 4,940 and the prose
5,705), and a SQL-support bullet now records that integer and real type aliases
are recognized.

### qdb: COUNT(DISTINCT col) + Fail-Closed DISTINCT Guards (2026-09-05)

**COUNT(DISTINCT col) now counts distinct non-NULL values, grouped and non-grouped.**
- `SELECT COUNT(DISTINCT dept) FROM emp` and
  `SELECT dept, COUNT(DISTINCT salary) FROM emp GROUP BY dept` previously had no
  path: the parser accepted only a bare column or `*` inside an aggregate, so
  DISTINCT was a syntax error. A fourth per-projection array, `p_agg_distinct`,
  now runs parallel to `p_agg_funcs`/`p_agg_cols`/`p_aliases` and carries a
  1 for `COUNT(DISTINCT col)`. Both the non-grouped path (`compute_single_agg`)
  and the grouped path (a new `group_count_col_distinct`) linear-scan the match
  set and dedup by pooled string value before counting.
- The distinct count is proven independent of the row count by a test that
  inserts a duplicate salary within a group, so distinct (2) diverges from the
  group's row count (3).
- The output header renders `COUNT(DISTINCT col)`; a projection alias still
  overrides it, and grouped ORDER BY resolves against that alias.
- DISTINCT outside COUNT fails closed: `SUM(DISTINCT ...)`/`AVG(DISTINCT ...)`
  report "DISTINCT is only supported inside COUNT" and `COUNT(DISTINCT *)`
  reports "COUNT(DISTINCT *) is not supported; use COUNT(*)", both via
  process_exit(1) with the table left intact, rather than silently dropping the
  keyword.
- Tests added: `count_distinct_dept_nogroup`, `count_distinct_grouped`,
  `count_distinct_alias_order`, `count_distinct_sum_closed`,
  `count_distinct_star_closed`.
- db.bld: 5,810 lines. Gate: pass=93 fail=0 xfail=1 xpass=0 (the one XFAIL is
  cross-process ROLLBACK, a documented process-model null).

**README reconciled.** The aggregate list now includes COUNT(DISTINCT col), a
known-limitation note records that DISTINCT is COUNT-only with the other forms
failing closed, and a stale Known-Issue claiming grouped ORDER BY sorts by the
group column only (already false since the prior entry) was removed.

### qdb: Grouped ORDER BY by Alias + Fail-Closed Resolution (2026-09-05)

**Grouped ORDER BY now resolves a projection alias.**
- `SELECT dept, SUM(salary) AS s FROM emp GROUP BY dept ORDER BY s DESC`
  previously left rows in group-discovery order (a silent wrong answer). The
  grouped-sort path in `exec_select_group` now resolves the order key in two
  steps: an aggregate/plain match by func + arg, then a projection-alias match
  against `p_aliases`. The sort type is derived from the matched output column,
  not the parsed key.
- An order key that matches none of the grouped column, an aggregate output, or
  a select alias now fails closed with an error instead of emitting rows in
  arbitrary order.
- Tests added: `group_ord_alias_sum_desc`, `group_ord_alias_sum_asc`,
  `group_ord_alias_cnt_desc`, `group_ord_unresolved_closed`.
- db.bld: 5,705 lines. Gate: pass=88 fail=0 xfail=1 xpass=0 (the one XFAIL is
  cross-process ROLLBACK, a documented process-model null).

**README reconciled to the current engine.** The qdb section now documents
capabilities the engine already shipped: INNER/LEFT JOIN, UNION and UNION ALL,
and the boolean-expression-tree WHERE parser (parenthesized grouping and NOT
BETWEEN evaluate exactly, no longer flat DNF). Removed the stale limitation
claiming grouped ORDER BY on an aggregate term leaves rows unsorted, and
narrowed the XFAIL-divergence note to cross-process ROLLBACK only.

### Documentation Site + Build Scripts (2026-03-27)

**GitHub Pages site created (docs/index.html):**
- Single-page documentation at harperz9.github.io/build-universe
- Stats, code examples, program table, architecture overview, quality badges
- All numbers verified from actual source (30,132 lines, 62 programs, etc.)
- Enable in repo Settings > Pages > Deploy from branch `main`, folder `/docs`

**Build scripts created:**
- `build_all.bat` - compile all programs via buildc + MSVC (run from cmd.exe)
- `build_all_self.bat` - compile all via self-hosted qcodegen + MSVC

### Closure Codegen + In-Process Benchmarks (2026-03-27)

**Closure/lambda codegen - THE LAST MAJOR GAP CLOSED:**
- `|x| x * 2` emits static C function `__lambda_N`
- `.map(|x| expr)` desugars to C for-loop with qv_push
- `.filter(|x| expr)` desugars to conditional qv_push
- `.fold(init, |acc, x| expr)` desugars to accumulator loop
- `fn(T) -> T` types emit C typedef function pointers
- codegen.bld: 2,294 → 2,490 lines (+196)

**All major codegen features now implemented:**
Functions, let, if (ternary + blocks), while, return, structs, impl methods,
Vec, include!(), 30+ builtins, closures, iterator desugaring, self-compilation.

**In-process qdb benchmark:**
- REPL mode: 1,006 statements in ~12,000ms = 11ms/statement
- Per-process mode: 49ms/query (dominated by startup)
- REPL eliminates 75% of per-query overhead

### Multi-Statement If-Blocks + Full Audit (2026-03-27)

**If-expression blocks with let bindings - FIXED:**
- Single-expression branches: still emit compact C ternary
- Multi-statement branches: emit `if (cond) { stmts; target = last; } else { ... }`
- 5 new functions: is_multi_stmt_block, if_has_multi_stmt, codegen_if_block_assign,
  codegen_if_branch_body, infer_if_block_type
- codegen.bld: 2,081 → 2,294 lines (+213)

**Full re-audit: 62/62 programs produce structurally valid C**
- All have `#include`, `int main`, >20 lines
- 0 programs fail codegen
- Largest: db (4,579 lines C), codegen (3,565), qc (3,258)

### include!() Preprocessing + Builtins + Self-Compilation (2026-03-27)

**include!() preprocessing added to qcodegen:**
- Scans source for `include!("path")`, reads referenced files, splices inline
- Double-inclusion guard (pipe-delimited path tracking)
- Path resolution relative to source file directory
- All 16 stdlib-using programs now generate correct C
- **Self-compilation works:** qcodegen processes its own source (2,081 lines)
  including `include!("stdlib/chars.bld")` and `include!("stdlib/tokenizer.bld")`
  → produces 3,352 lines of C

**15 missing builtins mapped:**
- Functions: time_unix, clock_ms, getenv, to_string_i32, to_string_f64
- String methods: ends_with, trim, split, split_whitespace, parse_float,
  compare, to_lowercase, to_uppercase, replace
- Runtime helpers added: qc_itoa, qc_ftoa, qc_clock_ms, qc_ends_with,
  qc_trim, qc_to_lower, qc_to_upper, qc_replace

**getenv return type fixed:** now inferred as `const char*` (was `int64_t`)

codegen.bld: 1,835 → 2,081 lines (+246)

### Impl Method Dispatch + E2E Audit (2026-03-27)

**Impl/method dispatch added to qcodegen:**
- `impl Type { fn method(&self) }` → `Type_method(Type* self)` (mangled names)
- `self.field` → `self->field` (pointer deref inside methods)
- `obj.method(args)` → `Type_method(&obj, args)` (dispatch with address-of)
- `Type::new(...)` recognized as constructor returning `Type`
- codegen.bld: 1,594 → 1,835 lines (+241 lines for impl support)

**End-to-end audit (qcodegen → MSVC → run):**
- Previously proven: 6 programs (test_hello, yes, echo, basename, dirname, seq)
- New proven: tee, + 2 impl test programs = **9 programs total**
- Remaining gaps: include!() not preprocessed by qcodegen, if-expression blocks
  with let bindings, missing builtin mappings (time_unix, split, split_whitespace)

### Self-Hosted Compiler: 62/62 Programs Compile (2026-03-27)

**qcodegen now compiles ALL 62 programs to C.**
- Added Vec codegen: `vec![]` → `qv_new()`, `vec_push`/`vec_get`/`vec_len` → `qv_push`/`qv_get`/`qv_len` with embedded QVec runtime
- Fixed ternary type inference for string branches
- codegen.bld: 1,517 → 1,594 lines

**Audit results:**
- 62/62 programs produce valid C output
- Largest: db.bld → 4,429 lines of C
- Self-compilation: codegen.bld → 1,773 lines C, qc.bld → 2,228 lines C
- Smallest: test_hello → 36 lines C

**Note:** "compiles to C" means syntactically valid C is generated. Some programs
may have runtime issues (wrong types, missing methods) that prevent the C from
compiling with gcc/MSVC. The proven end-to-end programs (C compiles AND runs
correctly) are: test_hello, yes, echo, basename, dirname, seq, + struct test.

### Struct Codegen in Self-Hosted Compiler (2026-03-27)

**qcodegen now handles structs:**
- Struct typedef emission: `struct Point { x: i64 }` → `typedef struct { int64_t x; } Point;`
- Struct literal construction: `Point { x: 3 }` → `(Point){ .x = 3 }`
- Field access: `p.x` → `p.x`
- Field assignment: `q.x = 10` → `q.x = 10`
- Struct parameters in functions: `fn foo(p: Point)` → `int64_t foo(Point p)`

**New programs compilable by self-hosted compiler:**
- basename.bld (232 lines C), dirname.bld (235), seq.bld (349)
- Any program using simple structs now works

**Programs proven end-to-end with qcodegen: 6**
(test_hello, yes, echo, + struct test, basename, dirname)

**4 more programs migrated to stdlib/lines.bld:**
- awk (-18), sed (-13), cut (-12), paste (-30) = **-73 lines**

### Self-Hosted Compiler Proven End-to-End (2026-03-27)

**qcodegen compiles real programs to working native binaries:**
- `yes.bld` (81 lines) → 156 lines of C → compiles → runs correctly
  (tested: `yes -n 5`, `yes hello -n 3`, `yes --help`)
- `echo.bld` (131 lines) → 195 lines of C → compiles → runs correctly
  (tested: `echo hello world`, `echo -n hello`, `echo --help`)
- `test_hello.bld` → compiles → `3628800`, `5050` (correct)

**7 codegen fixes applied:**
1. Variable type tracking (string vs int for correct format specifiers)
2. Builtin function translation (args_count→argc, args_get→argv[i])
3. String method translation (.starts_with, .len, .substring, .parse_int, etc.)
4. String comparison (== → strcmp)
5. Format specifier detection (%s for strings, %lld for ints)
6. String concatenation (+ → qc_strcat)
7. Embedded C runtime with all qc_* helpers

**Remaining codegen gaps (honest):**
- Structs, if-expressions, Vec, trait methods not yet handled
- Programs using these features still need the Rust compiler

**2 more programs migrated to stdlib/lines.bld:**
- grep.bld (-16 lines), rev.bld (-15 lines)
- sort, uniq, wc couldn't migrate (architecture mismatch, already clean)

**Artifact cleanup:**
- Removed stale .bat build scripts
- Added 3 while-loop test programs to git
- Updated .gitignore for generated .c files
- Working tree: zero dirty files

### Principal-Grade Polish (2026-03-27)

**Compiler DESIGN.md (436 lines):**
- Full pipeline documentation: preprocessor → lexer → parser → type checker → MIR → backends
- Every module described with verified line counts
- Internal architecture: two-pass type checking, SSA basic blocks, closure capture,
  generic monomorphization, iterator desugaring, vtable generation
- Key design decisions with rationale (why MIR, why C backend, why flat structs)
- Complete source file index

**qdb benchmark suite:**
- `benchmarks/bench_qdb.sh`: INSERT (100/1000 rows), SELECT, WHERE, COUNT,
  ORDER BY, GROUP BY, JOIN - all timed
- Results: 34-87ms per query on 1,000 rows (dominated by process startup)
- Honest documentation of limitations (no in-process mode, per-invocation overhead)

**numpy warnings - FIXED:**
- build-color: 13 RuntimeWarnings → 0 (np.errstate wraps in gamut.py,
  spaces.py, tonemap.py)
- 457 tests pass with zero warnings

**5 more programs migrated to stdlib/lines.bld:**
- expand (-6), unexpand (-6), fold (-6), nl (-10), tac (-77)
- Total: 105 lines removed
- tac.bld: eliminated entire Tac struct, replaced with lr_new/lr_get

### Shared Tokenizer Extraction - Major Deduplication (2026-03-27)

**stdlib/tokenizer.bld created (430 lines):**
- All 48 TK_* token type constants
- Tok struct, tok_new constructor
- Full tokenizer: tokenize(), read_string(), read_number(), read_ident_or_keyword()
- Token access: tok_text(), tok_emit(), peek_tok(), advance_tok()
- Helper functions: is_keyword(), skip_whitespace(), skip_line_comment()

**Self-hosting tools migrated (2,118 lines eliminated):**

| File | Before | After | Saved |
|------|--------|-------|-------|
| tok.bld | 916 | 289 | 627 |
| parse.bld | 1,420 | 1,050 | 370 |
| check.bld | 1,990 | 1,628 | 362 |
| codegen.bld | 1,470 | 1,093 | 377 |
| qc.bld | 2,280 | 1,898 | 382 |
| **Total** | **8,076** | **5,958** | **2,118** |

**String pool migration - partially blocked:**
- csv2json, paste, patch annotated with stdlib pattern reference
- Cannot directly include stdlib/string_pool.bld due to function name
  collisions (each program's sp_add takes a different struct type)
- Blocked by: compiler lacks function overloading or module-scoped names
- Documented in program comments for future migration

### Stdlib Migration - First Real Deduplication (2026-03-27)

**Self-hosting tools migrated to shared stdlib:**
- tok.bld: -60 lines (is_alpha/is_digit/etc. replaced with include)
- parse.bld: -45 lines
- check.bld: -45 lines
- codegen.bld: -45 lines
- qc.bld: -45 lines
- Total: **240 lines of duplication eliminated** from 5 files
- Each file now starts with `include!("stdlib/chars.bld");`

**Stdlib expanded (3 new modules):**
- `stdlib/lines.bld` - LineReader struct (lr_new, lr_get) for splitting
  strings into lines. Used by 35+ programs.
- `stdlib/string_pool.bld` - StringPool struct (sp_new, sp_add, sp_get)
  for storing string arrays with Vec<i32>. Used by 10+ programs.
- `stdlib/chars.bld` - added is_bin_digit, is_oct_digit (needed by tokenizers)

**Stdlib inventory:**
```
stdlib/chars.bld        - is_digit, is_alpha, is_alnum, is_whitespace,
                             is_hex_digit, is_bin_digit, is_oct_digit
stdlib/string_utils.bld - trim_left, starts_with_alpha
stdlib/lines.bld        - LineReader (split string into lines)
stdlib/string_pool.bld  - StringPool (string array via Vec<i32>)
```

### Multi-File Includes + Refactoring (2026-03-27)

**`include!("path")` preprocessor - NEW COMPILER FEATURE:**
- Textual file inclusion: `include!("stdlib/chars.bld");` splices referenced
  file contents at the directive site, like C's `#include`
- Double-inclusion guard: each file included at most once (canonical path tracking)
- Recursion depth limit: 10 levels max with clear error on overflow
- Error on missing files: prints resolved path, exits with code 1
- Wired into all 6 compiler commands (lex, parse, check, build, run, compile)
- Unblocks: stdlib extraction, eliminating 4,000 lines of duplicated code

**Standard library started:**
- `stdlib/chars.bld` - `is_digit`, `is_alpha`, `is_alnum`, `is_whitespace`, `is_hex_digit`
- `stdlib/string_utils.bld` - `trim_left`, `starts_with_alpha` (includes chars.bld)
- Both verified: compiles, correct output, nested inclusion works

**Setter workaround removal:**
- Refactored awk.bld (-3 lines), make.bld (-14 lines), sed.bld (-13 lines)
- Removed 7 trivial setter functions, replaced with direct `s.field = value;`
- Total: 30 lines removed. Proves the struct field fix has real impact.
- All 96 program tests pass after refactoring.

### Compiler Bug Fixes (2026-03-27)

**Struct field assignment on local variables - FIXED:**
- Root cause: `lower_assign()` only handled field assignment through pointers
  (`obj->field = val`), not on local struct values (`obj.field = val`).
  Assignments on locals were silently dropped - no MIR instruction emitted.
- Fix: Added `MirStmtKind::FieldAssign` to IR, builder, lowerer, and all 7
  backends (C, LLVM, WASM, SPIRV, ARM64, x86_64). C backend now emits
  `base.field = value;` for locals.
- Impact: Eliminates the `&mut` workaround pattern from ALL 60 programs.
  Programs can now assign struct fields directly in any function scope.
- Test: `125_local_struct_assign.bld` - verifies `p.x = 10;` emits correctly.

**String literal method calls - VERIFIED WORKING:**
- Previously reported as broken (`let s = ""; s.char_at(0)` fails).
- After investigation: compiles correctly in isolation. The issue occurs only
  when mixing string literal returns with parameter method calls in the same
  function - a specific codegen edge case, not a general type inference bug.
- Status: Documented as edge-case workaround, not a blocking issue.

**Sequential while-loop codegen - VERIFIED FIXED:**
- Previously reported as dropped second while loop.
- The var_map save/restore fix (applied earlier) resolved this completely.
- Verified: 3 sequential while loops generate correct C with proper basic blocks.

### Quality Overhaul (2026-03-27)

**Documentation:**
- Added `README.md` (192 lines) - architecture decisions, full program table,
  self-hosting chain, qdb feature set, build instructions, known issues
- Added `ARCHITECTURE.md` (176 lines) - qdb module map, data model, file format
  spec, WAL protocol, query execution flow, design tradeoffs
- Added section banners and function documentation to db.bld (15+ functions)

**Testing:**
- Added `tests/run_tests.sh` - automated test suite
- 96 tests pass, 0 failures, 8 skips (interactive/system-dependent programs)
- Coverage: 49 of 57 executables tested

**Consistency Fixes:**
- Fixed qjq hanging on no input (added stdin_is_pipe check)
- Added `--help` to qdb, qjq, qsql (was missing - now 100% coverage)
- All 60 programs respond to `--help` or `-h`

**Code Organization:**
- db.bld reorganized into 12 labeled sections with architecture comments
- No logic changes - pure documentation and structural clarity

### Compiler Cleanup (2026-03-26)

- Reduced compiler warnings from 2,031 to 0
- Split lower.rs (7,967 lines) into 4 modules:
  - `mod.rs` (1,609) - struct, constructors, item lowering
  - `expr.rs` (3,627) - expression and statement lowering
  - `types.rs` (777) - type lowering, const eval
  - `macros.rs` (2,028) - closures, builtins, iterators
- Added `programs/.gitignore` for build artifacts (*.c, *.exe, *.obj)
- Removed stale `db.exe` duplicate and misnamed `qdiff` C file
- Removed stray `claude-mastery-project.conf` from calibrate-pro and aurora

## [1.0.0] - 2026-03-24 to 2026-03-27

### Programs (60 total)

**Self-Hosting Compiler Chain (8 programs, 8,315 lines):**
- `qtok` - tokenizer (975 lines)
- `qparse` - recursive descent parser (1,464 lines)
- `qcheck` - type checker with scope tracking (2,040 lines)
- `qcodegen` - C code generator (1,513 lines)
- `qc` - unified self-hosted compiler (2,323 lines)
- `qfmt` - code formatter (462 lines)
- `qlint` - source linter with 8 checks (452 lines)
- `qjson` - JSON parser/pretty-printer (417 lines)

**Database Engine (3 programs, 5,634 lines):**
- `qdb` - SQL database with JOIN, GROUP BY, indexes, WAL, transactions (4,232 lines)
- `qkv` - persistent key-value store (533 lines)
- `qsql` - standalone SQL parser (869 lines)

**Text Processing (14 programs):**
- `qawk`, `qcut`, `qexpand`, `qfold`, `qgrep`, `qht`, `qnl`, `qpaste`,
  `qsed`, `qsort`, `qtr`, `qunexpand`, `quniq`, `qwc`

**Data Tools (6 programs):**
- `qbase64`, `qcalc`, `qcsv`, `qhex`, `qjq`, `qprintf`

**File Tools (9 programs):**
- `qcmp`, `qcomm`, `qdiff`, `qfind`, `qjoin`, `qloc`,
  `qpatch`, `qstrings`, `qwatch`

**System/Shell (10 programs):**
- `qbasename`, `qdate`, `qdirname`, `qecho`, `qenv`, `qrealpath`,
  `qsleep`, `qtest`, `qwhich`, `qxargs`

**Utilities (7 programs):**
- `qhttp`, `qmake`, `qmd`, `qrev`, `qseq`, `qtac`, `qtee`, `qyes`

### Compiler (buildc)

- 507 tests, 0 warnings
- 96 runtime builtins
- Backends: C (primary), LLVM, WASM, SPIRV, ARM64, x86_64 (varying completeness)
- Features: structs, enums, match, traits, generics, closures, iterators,
  tuples, Vec, HashMap, inline modules, struct constants, &self/&mut self,
  25+ float intrinsics, 11 string methods, argv/stdin, file/dir I/O,
  TCP sockets, binary I/O, Range.step_by, clock/time

### Spectrum Library

- 7,129 lines of BuildLang color science compiles to 35,241 lines of C
- Zero parse errors, zero type errors
- Covers: RGB/XYZ/Lab/Oklab/JzAzBz/ICtCp conversions, CIEDE2000,
  tonemapping, chromatic adaptation, gamut mapping, ICC profiles

## Known Issues and Technical Debt

### Critical (blocking portfolio quality)
1. **No inter-program code reuse** - tokenizer duplicated across 5 self-hosting
   tools (~4,000 lines). Blocked by: compiler lacks multi-file compilation.
   Intended fix: add `use` import support to buildc.

### Significant
2. **Struct field assignment on locals** - compiler codegen bug. Programs work
   around it with `&mut` reference parameters. Fix requires C backend changes.
3. **String type inference** - `let x = ""` infers `&'static str`, blocking
   method calls. Workaround: use `str.substring(0, 0)` instead of `""`.
4. **Sequential while-loop codegen** - second while loop in same scope can be
   silently dropped. Workaround: use single-loop patterns or separate functions.
5. **Vec<String> incomplete** - only Vec<i32>/Vec<f64> have full codegen.
   Programs use string pool pattern instead.

### Minor
6. **GROUP BY rendering** - some aggregate combinations produce incorrect values
7. **qc subset** - self-hosted compiler handles simple programs only
8. **13 numpy warnings** in build-color (harmless edge cases)
9. **8 compiler integration tests skipped** (not blocking, test infra issue)
