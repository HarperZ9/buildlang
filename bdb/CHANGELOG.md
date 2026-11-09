# BuildDB (qdb) changelog

## Unreleased

- 2026-11-09: a damaged transaction id in the WAL no longer stops qdb. A WAL line such as
  `TXID 25O...` (one damaged byte) overflowed the number parser and the process ended with exit
  101; replay and the `.db` header now read the id with `txid_text` and skip a bad one. Found by
  the new fuzzer `bdb/tests/fuzz_qdb_wal.py` (seed 20261109, 2,000 single damages to a 2,560-byte
  WAL: 5 stops before the fix, 0 after; a planted replay that accepts any COMMIT hash fails 329
  cases). It runs in CI. Open: the `.db` file has no checksum, so of 500 damaged `.db` files 7
  stop qdb and 438 change an answer with no error; the fuzzer counts these and does not gate them.

- 2026-11-09: the REPL ends at end of input. A script piped to `qdb` with no `.quit` used to loop
  forever on the prompt; it now leaves the way `.quit` does (exit 0, the WAL folded into the `.db`,
  `HOLD` inside an open BEGIN). New test `bdb/tests/test_qdb_eof.py`, in CI.

- 2026-11-09: memory follows the data. A checkpoint and a COMMIT build their text with one join,
  the parser reuses its lists, the tokenizer reads bytes, and a single-row constraint check no
  longer builds a map. Through `bdb/lib`, loading 8,000 rows peaks at 9.7 MB where it took 953.8 MB;
  100,000 rows peak at 89.1 MB (SQLite 3.54.1: 2.0 MB, so 44.6 times; the 10 times target is
  missed). Output, `.db` and WAL bytes are unchanged. New test `bdb/tests/test_qdb_memory.py`
  bounds memory growth per row and per statement and must catch a planted leak; it runs in CI.

- 2026-11-09: `bdb/lib` has prepared statements and a row API (`qdb_prepare`, `qdb_bind_*`,
  `qdb_step`, `qdb_column_*`, `qdb_reset`, `qdb_finalize`). Point selects on an indexed column and
  single-equality updates skip the parser; numeric index searches compare cached numbers. With
  both engines prepared (10,000 rows, Windows, median of 3): select 1.63 times SQLite's time,
  update 0.97 times, mixed 0.92 times. New test `bdb/tests/test_qdb_rows.py`.

- 2026-11-09: `REPLACE INTO` and `INSERT OR REPLACE INTO` run with SQLite's rule: a new row removes
  the rows that share any of its UNIQUE or PRIMARY KEY keys. sqllogictest 9,181 to 9,185 of 9,202.

- 2026-11-09: DISTINCT works in SUM, AVG, MIN, MAX and TOTAL, with or without GROUP BY; `total()`,
  `group_concat(x [, sep])`, `NOT INDEXED` / `INDEXED BY` and the postfix tests `x NOT NULL`,
  `ISNULL`, `NOTNULL` run. sqllogictest 9,143 to 9,181 of 9,202.

- 2026-11-09: `INSERT INTO t [(cols)] SELECT ...` runs; the selected rows pass the same column and
  constraint checks as VALUES rows. sqllogictest 9,087 to 9,143 of 9,202, wrong answers 65 to 9.

- 2026-11-09: an explicit transaction writes its WAL lines once, with its COMMIT record (or with
  HOLD at a clean exit inside BEGIN), instead of one file append per statement. The WAL bytes are
  the same. Through `bdb/lib`: updates at 7.5 times SQLite's time (102.9 before), mixed at 8.3
  times (51.6 before).

- 2026-11-09: UPDATE and DELETE change rows in place. An UPDATE writes only the changed cells and
  moves their entries in the indexes on the changed column; a DELETE compacts the table in place
  and renumbers its indexes without a sort; a WHERE of one equality on an indexed column finds its
  rows through the index. Indexes no longer go stale after either. Through `bdb/lib` against
  SQLite: updates 2,472 to 102.9 times SQLite's time, mixed 2,185 to 51.6 times. New test
  `bdb/tests/test_qdb_mutations.py` checks indexed tables against SQLite after every kind of write.

- 2026-11-09: `SELECT 1 IN (...)`, `WHERE 1 IN (...)` and SQLite's `x IN t` run; `NULL IN ()` is
  false; `IN (SELECT a, b ...)` is an error. sqllogictest 8,921 to 9,087 of 9,202.

- 2026-11-09: first throughput number through `bdb/lib` against SQLite 3.49.1 on the same machine
  (`bdb/benchmarks/bench_qdb_throughput.py`): point selects at 10.0 times SQLite's time, updates
  at 2,472 times (about 7.6 ms per UPDATE on 10,000 rows).

- 2026-11-09: PRIMARY KEY and UNIQUE checks go through a key map instead of a scan of the table, so
  a keyed bulk load is linear: 8,000 rows in one transaction 44.1 s to 2.72 s, and an UPDATE of a
  key column checks the table in one pass. NULL keys stay distinct and `1` still equals `1.0`.

- 2026-11-09: receipts hash with the runtime's `String.sha256()` and print as one string: their
  cost falls from 44 to 13.0 percent of query time on Linux and from 347 to 5.7 percent on
  Windows (target under 5). Receipt bytes unchanged.

- 2026-11-09: history, `AS OF` and receipts. Run qdb once with `QDB_HISTORY=1` and the database
  keeps every committed transaction in `<db>.hist`; `SELECT ... AS OF <txid>` answers from the
  state at that transaction. With `QDB_RECEIPTS=1` each result is followed by
  `RECEIPT <txid> <state> <query> <result>` (SHA-256 of each), which a second process reproduces
  with `AS OF`: 1,000 of 1,000 (`bdb/tests/test_qdb_receipts.py`). Receipts cost 44 percent of
  query time on Linux, against a target under 5. A mutation that fails no longer reaches the WAL.

- 2026-11-09: seeded power cuts (`bdb/tests/test_qdb_faults.py`). The harness learns every durable
  write of the crash workload and, from a seed, drops, tears and reorders the writes no flush had
  covered. It found a torn transaction replayed as committed (2 of 10,000 seeds, the recovering
  process exited 1). A `COMMIT` record now carries an FNV-1a hash of its `TXID` and `OP` lines, and
  replay applies only a transaction whose hash matches: 0 of 10,000 after. The recovered files hash
  the same on Windows and Linux for 10,000 of 10,000 seeds, checked in CI against the Windows digest.

- 2026-11-08: crash-safe checkpoints, transactions across processes, escaped text (file format 1).
  A crash between the `.db` rewrite and the WAL clear no longer replays the WAL a second time
  (duplicate rows), and a torn `.db` rewrite no longer loses rows: the checkpoint writes
  `<db>.ckpt` first, and the `.db` header `QDB 1 <txid>` tells replay what the file already holds.
  `bdb/tests/test_qdb_crash.py`: 24 violations of 104 crash points before, 0 of 170 after.
  A clean exit inside BEGIN leaves `HOLD` in the WAL and the next process resumes the
  transaction, so the documented null `xproc_rollback` is closed (test_qdb.sh 222 pass, 0 fail,
  0 xfail). Cells escape `\\`, `\|`, line feed and carriage return, and a WAL statement with a
  line break is written as `OPX`; 1,000 values with those characters round-trip through the `.db`
  and through the WAL (`bdb/tests/test_qdb_text.py`; 142 of 1,000 survived the `.db` before, 0 the
  WAL). A file without the header (format 0) is read as before.

- 2026-11-09: qdb moved from HarperZ9/build-universe (`programs/db.bld`) to `bdb/` with its
  history. CI now builds it with this repository's `buildc` on every change to `bdb/` or
  `compiler/` (`.github/workflows/bdb.yml`), and the compiler's in-process BuildQL subset is checked against
  that binary (`QDB_BIN`). No engine change in the move. Entries before the
  move are in Build Universe's `programs/CHANGELOG.md`.
