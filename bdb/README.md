# BuildDB (qdb)

qdb is a SQL database engine written in BuildLang: one source file, `db.bld`, built by this
repository's `buildc` and tested in this repository's CI on every compiler change. BuildQL is the
SQL dialect qdb runs. The compiler side of BuildQL is `compiler/src/bql/` (`buildc bql export`,
`query`, `rename`, `verify`), which writes a compiled program into qdb's file format; the design is
`docs/design/BUILDDB-BUILDQL.md`. `programs/sql.bld` in Build Universe is a parser demo and does
not execute.

## Build and test

```bash
cargo build --release --manifest-path compiler/Cargo.toml
compiler/target/release/buildc bdb/db.bld --target c -o qdb.c
gcc -O2 -w -o bdb/qdb.exe qdb.c -lm        # add -lws2_32 on Windows
bash bdb/tests/test_qdb.sh                  # SQL regression harness
python3 bdb/tests/test_qdb_differential.py OLD_QDB bdb/qdb.exe
python3 bdb/tests/test_qdb_wal_recovery.py bdb/qdb.exe
```

```bash
bdb/qdb.exe app.db "CREATE TABLE users (id INTEGER, name TEXT, age INTEGER)"
bdb/qdb.exe app.db "INSERT INTO users VALUES (1, 'ada', 36), (2, 'lin', 41)"
bdb/qdb.exe app.db "SELECT name FROM users WHERE age > 40"
bdb/qdb.exe app.db        # interactive REPL
```

`test_qdb.sh` asserts rendered output over the whole SQL surface. More checks, each with a planted
failure it must catch, are listed with their results in `bdb/COMPLETE.md`:

```bash
python3 bdb/tests/test_qdb_crash.py bdb/qdb.exe          # stop at every durable write, recover
python3 bdb/tests/test_qdb_text.py bdb/qdb.exe           # 1,000 values with | \ line breaks
python3 bdb/tests/test_qdb_sqlite_oracle.py bdb/qdb.exe  # 2,000 generated queries vs SQLite
python3 bdb/tests/slt_run.py CORPUS --qdb bdb/qdb.exe    # sqllogictest (corpus not vendored)
```

## Use it as a library

The same source builds as a static library with a three-call C interface, so a host (the
compiler, a game, a test) opens a database once and runs many statements in one process:

```bash
python3 bdb/lib/make_lib.py build/qdblib          # db.bld without main(), plus lib/exports.bld
buildc build --emit staticlib build/qdblib        # target/debug/libmain.a, main.h, main.link
cc host.c -I build/qdblib/target/debug build/qdblib/target/debug/libmain.a -lm -o host
```

```c
DB* d = qdb_open((uint8_t*)"app.db");     /* load the .db, replay the WAL */
qdb_exec(d, (uint8_t*)"SELECT * FROM t"); /* prints what the CLI prints */
qdb_close(d);                             /* checkpoint, or HOLD an open BEGIN */
```

`bdb/tests/test_qdb_lib.py` runs the differential workloads through `bdb/lib/driver.c` and through
one CLI process per statement and requires the same output and the same final `.db`. Limit: a
statement the parser rejects ends the host process with exit code 1, as it ends the CLI.

## Where it came from

qdb lived in HarperZ9/build-universe as `programs/db.bld` until 2026-11-09, and moved here with its
history (`git log --follow bdb/db.bld`). PR numbers in commit messages from before the move are
written `HarperZ9/build-universe#N`. Its changelog before the move is `programs/CHANGELOG.md` in
that repository.

## Licence

The files in `bdb/` keep the licence they had in Build Universe, FSL-1.1-MIT (`bdb/LICENSE`). The
rest of this repository is AGPL-3.0-only.

## What it runs

qdb is about 7,000 lines of BuildLang. BuildQL is the SQL dialect it runs:

**SQL support:**
- CREATE TABLE, INSERT, SELECT, UPDATE, DELETE, DROP TABLE
- Column types INTEGER, REAL, and TEXT, with the common SQL aliases (INT, BIGINT, SMALLINT, FLOAT, DOUBLE, NUMERIC, and similar) recognized so a numeric column orders and right-aligns as a number
- ALTER TABLE ADD COLUMN
- UPDATE SET col = col OP literal, where OP is `+`, `-`, `*`, or `/` (arithmetic RHS, fail-closed on bad operand or division by zero)
- WHERE as a boolean-expression tree: AND/OR/NOT with parenthesized grouping, over comparisons, LIKE, IN, IS NULL/IS NOT NULL, BETWEEN/NOT BETWEEN
- A WHERE comparison uses one of `=`, `<`, `>`, `<=`, `>=`, `<>`, `!=`. Arithmetic inside a condition (`salary + 10 > 100`) and a doubled operator (`salary == 100`) fail closed with an error rather than returning zero rows silently
- A WHERE clause that names a column no target table has fails closed with `no such column` across SELECT, aggregates, GROUP BY, UPDATE, DELETE, and scalar subqueries. Before this a typo'd column returned an empty result, and IS NULL on it returned every row, both silent wrong answers; UPDATE and DELETE now leave the table untouched
- Scalar subqueries in WHERE clauses
- INNER and LEFT JOIN (nested-loop) with table aliases; LEFT JOIN fills unmatched right rows with NULL
- UNION and UNION ALL, with column-count and per-branch shape checks (fail-closed on mismatch)
- A complete statement must consume its whole input. A token left over after the parse, whether a clause qdb does not support or plain garbage after LIMIT, WHERE, or a table alias, fails closed with `unexpected trailing input near '<token>'` rather than being dropped. A query would otherwise return a plausible wrong answer and a mutation would apply a half-specified write; the check runs before the WAL, so a rejected INSERT, UPDATE, DELETE, CREATE, or DROP changes nothing, and it re-runs on each UNION branch so a trailing token after the final branch is caught too. A single bareword after a table name is still a legal alias, not trailing garbage
- GROUP BY with aggregates: COUNT(*), COUNT(col), COUNT(DISTINCT col), SUM, AVG, MIN, MAX
- MIN and MAX order a TEXT column lexicographically; over an empty result set an aggregate MIN or MAX yields SQL NULL
- HAVING on an aggregate, with the same operator fail-closed as WHERE: an arithmetic or doubled operator (`HAVING COUNT(*) + 0 > 1`, `HAVING COUNT(*) == 1`) errors rather than dropping every group silently
- An aggregate over a column no table has fails closed with `no such column`, across a plain SELECT, a GROUP BY, a HAVING term, and a scalar subquery. Before this SUM, AVG, and COUNT(col) returned 0, MIN and MAX returned blank, a grouped aggregate returned 0 per group, and a subquery fed 0 into the outer comparison so every row matched, each a silent wrong answer. COUNT(*) is exempt. An aggregate over a JOIN is a separate path: a scalar aggregate, a GROUP BY, a HAVING filter, and an ORDER BY all fold or sort over the joined rows (see the following bullets); COUNT(DISTINCT) over a grouped JOIN still fails closed (see Known limitations)
- A scalar aggregate (COUNT/SUM/AVG/MIN/MAX, with no GROUP BY) over a JOIN folds over the assembled join rows: COUNT(*) counts joined rows, COUNT(col) counts non-empty cells, and SUM/AVG/MIN/MAX read the referenced table's column on each matched pair. The fold honors WHERE and LEFT JOIN null-extension, and matches single-table aggregate semantics (numeric MIN/MAX coerce empty cells to 0, text MIN/MAX compare lexically, MIN/MAX over zero rows yield SQL NULL)
- A GROUP BY over a JOIN folds the same way per group. The group column resolves against either joined table, the distinct keys keep first-seen order, and each group's bucket folds its projection items with the scalar path's rules. A WHERE that empties a group drops it from the output. DISTINCT over a grouped JOIN, and COUNT(DISTINCT) over any JOIN, each fail closed with a specific message
- A HAVING over a grouped JOIN folds its aggregate over each group's bucket and keeps only the groups that pass, with the same COUNT/SUM/AVG/MIN/MAX rules as the projection fold. The HAVING aggregate is independent of the projected one, so `SELECT emp.dept, COUNT(*) ... HAVING MAX(emp.salary) >= 120` filters on the max while displaying the count. A bad HAVING operator is rejected at parse time, and a HAVING that filters every group leaves a header with zero rows rather than all groups or an error
- An ORDER BY over a grouped JOIN sorts the folded groups after the HAVING filter. The key names the grouped column, an aggregate term (`ORDER BY SUM(emp.salary)`), a projection alias, or a 1-based positional ordinal (`ORDER BY 2`); it resolves to an output column and a stable sort keeps equal keys in group-discovery order. COUNT sorts as an integer, SUM/AVG/MIN/MAX as a real, a plain key by its source type. A secondary key or an unresolvable name fails closed with an accurate message rather than a silent wrong order, and an out-of-range ordinal fails closed too
- A LIMIT and OFFSET over a grouped JOIN truncate the group rows after the ORDER BY sort, so `ORDER BY SUM(emp.salary) DESC LIMIT 3` returns the top three groups. LIMIT without ORDER BY keeps group-discovery order, OFFSET skips whole groups, HAVING runs before the truncation, and LIMIT 0 or an OFFSET past the last group renders the header with zero data rows
- DISTINCT, LIMIT/OFFSET
- ORDER BY a column name, a positional ordinal (ORDER BY 2, counting select columns from 1), or a projection alias (ORDER BY the AS name), with an optional secondary tie-break key. Single-table, JOIN, and UNION selects each resolve an ordinal or column key and fail closed on a key that names nothing, instead of leaving the rows unsorted: a JOIN key binds to whichever joined table owns the column, and a UNION key binds to an output-column position (only the first branch's names reach the merged headers, so an ordinal is the portable way to order a union)
- ORDER BY on grouped output also accepts the group column, an aggregate term (ORDER BY SUM(salary)), or a positional ordinal (ORDER BY 2), on both the single-table and JOIN grouped paths
- Column aliases (SELECT col AS alias)

**Storage and reliability:**
- Line-based file format (format 1): a `QDB 1 <txid>` header, cells escaped (`\\`, `\|`, `\n`, `\r`), so text with a pipe or a line break round-trips; a file without the header is read as before
- CREATE INDEX with sorted-array indexes and binary search lookup
- Write-Ahead Log (WAL) for crash recovery
- BEGIN/COMMIT/ROLLBACK transaction support, in one process or across several: a clean exit inside BEGIN leaves `HOLD` in the WAL and the next process resumes the transaction
- Interactive REPL with dot-commands (.tables, .schema, .wal)

**Architecture:** All state (parser, storage, indexes, WAL, REPL) lives in a
single `DB` struct due to the flat-struct constraint. The SQL parser is embedded in the same file.

**Known limitations:**
- DISTINCT inside an aggregate is supported only for COUNT (COUNT(DISTINCT col)); SUM(DISTINCT ...), AVG(DISTINCT ...), and COUNT(DISTINCT *) fail closed with an error rather than silently ignoring the keyword
- Grouped ORDER BY resolves the key to the group column, an aggregate output, a select alias, or a 1-based positional ordinal; a key that matches none of these fails closed with an error rather than sorting by an unselected column
- A non-grouped JOIN ORDER BY resolves a column name (in either joined table) or a positional ordinal, but not a projection alias; ordering a non-grouped join by an AS name fails closed rather than sorting by an unselected column. A grouped JOIN ORDER BY is the separate path described in Features, and it does resolve an alias
- A scalar aggregate, a GROUP BY, a HAVING, an ORDER BY, and a LIMIT/OFFSET over a grouped JOIN all fold, sort, or truncate over the joined rows (see Features above). What remains fail-closed over a JOIN: a secondary ORDER BY key over a grouped JOIN, DISTINCT over a grouped JOIN, and COUNT(DISTINCT ...) over any JOIN (`COUNT(DISTINCT) over a JOIN is not supported`). Each is tracked here rather than returning a silent wrong grouping, order, or count
- A process that ends inside BEGIN without a clean exit (a crash, or a statement rejected with exit code 1) drops the open transaction
- NULL and the empty string are stored alike, as an empty cell, so `IS NULL` matches `''` and `NOT (col = x)` keeps a NULL row; SQL's three-valued logic is not implemented
- No query optimizer; all scans are sequential unless an index matches exactly
- Writes: a bulk load is linear in the rows loaded (about 0.13 s per 1,000 rows, 15 to 28 s per 100,000, see ARCHITECTURE.md, Write Cost). Still superlinear: an INSERT followed by an indexed SELECT on every row, INSERT into a table that is not the last one created, UPDATE, DELETE, and ORDER BY (selection sort)
- During a REPL session, committed writes live in the append-only WAL and reach the .db at a checkpoint (when the WAL outgrows the .db, and at a clean exit). A crash loses nothing that was committed; the next start replays it
- File format is not binary-compatible with SQLite
