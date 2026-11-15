# qdb has moved

qdb, the SQL engine written in BuildLang (BuildDB), now lives in HarperZ9/buildlang as `bdb/`:
https://github.com/HarperZ9/buildlang/tree/main/bdb

It moved on 2026-11-09 with its history, so `git log --follow bdb/db.bld` in buildlang shows every
commit it had here. There it is built by the compiler of each buildlang checkout and its tests run
in that repository's CI (`.github/workflows/bdb.yml`). The move landed in HarperZ9/buildlang#163,
merge commit fd78429912beda394bad2498a552a0b3cfbdeb39.

The last commit of this repository that holds the source is 4a296c1: `programs/db.bld`, its
architecture notes (`programs/ARCHITECTURE.md`), its tests (`programs/tests/test_qdb*.{sh,py}`),
its benchmarks (`programs/benchmarks/bench_qdb*`) and its workflow (`.github/workflows/qdb.yml`).
Its changelog entries before the move stay in `programs/CHANGELOG.md`.

In buildlang the files are AGPL-3.0-only, like the rest of that repository (the author's decision
of 2026-11-09). The copies in this repository's history keep the FSL-1.1-MIT licence they were
published under here.
