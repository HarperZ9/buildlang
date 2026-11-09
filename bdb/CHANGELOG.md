# BuildDB (qdb) changelog

## Unreleased

- 2026-11-09: `SELECT 1 IN (...)`, `WHERE 1 IN (...)` and SQLite's `x IN t` run; `NULL IN ()` is
  false; `IN (SELECT a, b ...)` is an error. sqllogictest 8,921 to 9,087 of 9,202.

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
