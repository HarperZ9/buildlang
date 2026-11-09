# BuildDB (qdb) changelog

## Unreleased

- 2026-11-09: qdb moved from HarperZ9/build-universe (`programs/db.bld`) to `bdb/` with its
  history. CI now builds it with this repository's `buildc` on every change to `bdb/` or
  `compiler/` (`.github/workflows/bdb.yml`), and the compiler's in-process BuildQL subset is checked against
  that binary (`QDB_BIN`). No engine change in the move. Entries before the
  move are in Build Universe's `programs/CHANGELOG.md`.
