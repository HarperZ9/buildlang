# qdb Architecture

## Overview

`qdb` is a SQL database engine written in ~4,940 lines of BuildLang (109 functions, 1 struct). It implements a substantial subset of SQL including DDL, DML, queries with JOIN/GROUP BY/ORDER BY/HAVING, sorted-array indexes with binary search, and transactions with WAL crash recovery. It persists data via a human-readable text file format and includes an interactive REPL.

## Why One File?

BuildLang does not support multi-file compilation or `use` imports between `.bld` files. All code must reside in a single file. Within the file, strict section boundaries with documentation headers maintain logical modularity. If BuildLang adds multi-file support in the future, each section maps cleanly to a standalone module.

## Section Map

```
Section                          Lines         Functions   Purpose
-------------------------------  -----------   ---------   ----------------------------------
 0. PRIMITIVES                   ~81-98        6           Character classification (is_ws, is_digit, etc.)
 1. DATA MODEL                   ~99-345       3           struct DB (105 fields), db_new, string pool
 2. STORAGE                      ~346-614      2           File save/load (text format)
 3. WAL                          ~615-971      11          Write-ahead log, snapshots, transactions
 4. TOKENIZER                    ~972-1205     8           SQL lexical analysis
 5. PARSER                       ~1206-2215    24          Recursive-descent SQL parsing (incl. DNF WHERE)
 6. EXECUTOR                     ~2216-4014    26          Query execution engine
    6a. DDL                                    2             CREATE TABLE, DROP TABLE
    6b. DML                                    3             INSERT, UPDATE (arithmetic RHS), DELETE
    6c. SELECT (basic)                         1             Single-table with WHERE/ORDER/LIMIT/DISTINCT
    6d. SELECT (aggregates)                    3             COUNT/SUM/AVG/MIN/MAX without GROUP BY
    6e. SELECT (GROUP BY)                      6             GROUP BY + HAVING + ORDER BY on group column
    6f. SELECT (JOIN)                          2             Nested-loop join
    6g. Subqueries                             2             Scalar subqueries in WHERE
    6h. ALTER TABLE                            1             ADD COLUMN
 7. HELPERS                      ~4015-4436    18          WHERE eval, LIKE, sorting, formatting, conversions
 8. INDEX                        ~4437-4707    5           Sorted-array indexes, binary search, CREATE INDEX
 9. DISPATCH                     ~4708-4770    2           exec_sql / exec_sql_no_wal routing
10. REPL                         ~4771-4871    3           Interactive shell, dot-commands
11. MAIN                         ~4872-end     1           Entry point, arg handling
```

## Data Model

### The DB Struct (105 fields)

All engine state lives in one flat struct threaded by `&mut` reference. buildc 1.2.0 lowers local struct field assignment (`p.x = 10`) correctly, so this single-struct layout is a retained design choice, not a compiler constraint. The flattened parallel-array layout inside it is still required by the `Vec<String>` limitation (see below).

The struct fields are grouped into:

| Group | Fields | Purpose |
|-------|--------|---------|
| File | `db_path` | Path to the `.db` file |
| String Pool | `pool`, `pn` | Interned strings (`Vec<String>`) and the next id |
| Tables | `tbl_names`, `tbl_count`, `tbl_col_starts`, `tbl_col_counts` | Table metadata |
| Columns | `col_names`, `col_types` | Flattened column schema |
| Rows | `cells`, `tbl_row_starts`, `tbl_row_counts` | Flattened cell data |
| Indexes | `idx_names`, `idx_tables`, `idx_col_indices`, `idx_keys`, `idx_rowids`, ... | Sorted-array index data |
| Parser | `input`, `pos`, `tk_type`, `tk_val`, `stmt_type`, `p_table`, `p_cols`, ... | Tokenizer + AST state |
| WHERE | `has_where`, `w_left`, `w_op`, `w_right`, `w_logic`, `w_in_vals1`, ... | Parsed WHERE conditions |
| WAL | `wal_path`, `in_transaction`, `txn_id`, `wal_bytes`, `db_bytes` | WAL state and checkpoint policy inputs |
| Index upkeep | `idx_stale` | Set when a mutation moved or rewrote rows; the next index read rebuilds |
| Snapshot | `snap_cells`, `snap_row_starts`, `snap_tbl_names`, ... | Rollback snapshot |

### String Pool Pattern

All strings are interned in `d.pool`, a `Vec<String>`. `sp_add(text)` pushes the text and returns its integer ID; `sp_get(id)` returns it. Both are O(1).

Until 2026-11-02 the pool was one contiguous `String` grown by concatenation, with parallel start and length arrays. The C backend's concatenation allocates a new buffer and copies both sides, so every `sp_add` copied the whole pool: loading N rows cost O(N^2) time and memory. One caveat for `Vec<String>`: create it from a typed local (`let v: Vec<String> = vec![];`). A bare `vec![]` inside a struct literal is built as a `Vec<i32>`, and pushing strings into it corrupts memory.

Pool IDs are used everywhere: table names, column names, cell values, parsed tokens, index keys.

### Parallel Arrays

Tables, columns, rows, and indexes all use parallel arrays indexed by integer. For example, table `i` has:
- Name: `tbl_names[i]` (pool ID)
- Columns start at: `tbl_col_starts[i]` in the `col_names`/`col_types` arrays
- Column count: `tbl_col_counts[i]`
- Rows start at: `tbl_row_starts[i]` in the `cells` array
- Row count: `tbl_row_counts[i]`

Cell data is flattened: row `r` of table `t` has cells at `cells[row_start + r * col_count + c]`.

## File Format

Human-readable, line-oriented text:

```
TABLE users
SCHEMA id:INTEGER name:TEXT age:INTEGER
ROW 1|Zain|25
ROW 2|Harper|30
END

INDEX idx_age ON users (age)
```

Each `ROW` uses pipe-delimited values. The entire file is read or written in one call (no incremental I/O). A write truncates and refills the file, so it is not atomic; the WAL is what makes committed data survive a crash (below).

## WAL (Write-Ahead Log)

Append-only log at `<db_path>.wal`:

```
TXID 1
OP INSERT INTO users VALUES (3, 'Alice', 28)
OP INSERT INTO users VALUES (4, 'Bob', 32)
COMMIT
```

Every write opens the file in append mode (`append_file`); nothing already written is rewritten.

**Recovery**: On startup, `wal_replay()` reads the WAL in one pass. A `TXID` line opens a transaction, and a `COMMIT` line replays that transaction's `OP` lines. A transaction without a `COMMIT` (a crash, a torn final record, a ROLLBACK, a process that exited inside BEGIN) is skipped. Replay then checkpoints and clears the WAL.

**Transactions**: `BEGIN` takes a snapshot and appends `TXID`. `COMMIT` appends the marker. `ROLLBACK` restores from snapshot and checkpoints. A clean exit inside `BEGIN` appends `HOLD <txid>`; the next process replays the committed records, snapshots, applies the held transaction's statements, appends `RESUME <txid>` and carries on inside the transaction, so `BEGIN`, the statements and `COMMIT` or `ROLLBACK` can each be their own process. An exit that is not clean leaves no `HOLD` at the end of the WAL, and the transaction is dropped.

**Auto-commit**: A mutation outside an explicit transaction appends one `TXID / OP / COMMIT` record in a single write.

**Checkpoints**: The .db is rewritten when the WAL holds more bytes than the .db plus 64 KB, never inside an open transaction, and at a clean exit (after a single statement, or at `.quit`). Each checkpoint costs at most the bytes appended since the last one, so saving a load of N rows costs O(N) in total.

**Durability**: A statement is durable once its `COMMIT` line is in the WAL, the same point as before. The old code rewrote the whole WAL for every line and the whole .db after every statement; both truncate a file, so a crash during either could lose committed data. An append can only tear the last record, and a torn record has no `COMMIT`, so it is skipped. No write calls fsync. `bdb/tests/test_qdb_wal_recovery.py` truncates a crash-left WAL at every byte offset and checks the recovered rows against this rule.

**Checkpoint safety**: the `.db` rewrite truncates the file, so a checkpoint writes the new state to `<db>.ckpt` first, ending in `QDB CHECKPOINT COMPLETE`, then the `.db`, then clears the WAL and empties the `.ckpt`. At load a complete `.ckpt` wins over the `.db`. The `.db` starts with `QDB 1 <txid>`, the last transaction it holds, and replay skips WAL transactions at or below it, so a crash between the rewrite and the WAL clear does not apply them twice. `bdb/tests/test_qdb_crash.py` stops each statement of a workload at every durable write, before it and halfway through it, and checks the recovered committed state (170 crash points, 0 violations on 2026-11-08; 24 violations of 104 before this design).

## Query Execution Flow

```
SQL string
  |
  v
reset_parser()  -- clear all parser state
  |
  v
parse_sql()     -- tokenize + parse into d.p_* fields
  |
  v
dispatch by d.stmt_type:
  0 -> exec_select()
         |-> has_join?      -> exec_select_join()    [nested loop]
         |-> has_group_by?  -> exec_select_group()   [grouping + HAVING]
         |-> has aggregates?-> exec_select_agg_no_group()
         |-> basic scan     -> filter, sort, limit, distinct, print
  1 -> exec_insert()        [add row to cells array]
  2 -> exec_create()        [add table metadata]
  3 -> exec_update()        [rebuild cells with new values]
  4 -> exec_delete()        [rebuild cells, skip deleted rows]
  5 -> exec_drop()          [remove table + adjust arrays]
  6 -> exec_create_index()  [register + rebuild sorted pairs]
  7 -> wal_begin()
  8 -> wal_commit()
  9 -> wal_rollback()
 10 -> exec_alter_table()   [add column, pad existing rows]
```

## Index Implementation

Sorted-array indexes with binary search:

- Each index stores sorted `(key, rowid)` pairs in flattened parallel arrays
- **Equality lookup**: Binary search to find key, scan left/right for duplicates
- **Range queries**: Linear scan of sorted array (could be optimized)
- **Maintenance**: Lazy, on the next index read (`refresh_index`). Rows appended by INSERT carry the highest row ids, so they are sorted alone and merged in, O(n + k log k). UPDATE, DELETE, ALTER and ROLLBACK set `idx_stale`, and the next read rebuilds every index once.
- **Sort algorithm**: Stable bottom-up merge sort (`sort_positions`), O(n log n). Ties keep row-id order, which is the order the earlier selection sort produced.

## Write Cost

Loading N rows used to cost at least O(N^2), from five places: the string pool
copied itself on every intern; the WAL was read and rewritten for every line;
the .db was rewritten after every auto-commit statement; INSERT copied the whole
cells array; and every INSERT re-sorted each index on the table with a selection
sort. With an index, the load was closer to O(N^3). Each path now costs
O(statement) or is amortized (see String Pool, WAL and Index Implementation
above).

Measured with `bdb/benchmarks/bench_qdb_load.py`: one REPL process per run,
a fresh database, rows inserted in a fixed scrambled order, a row-count check
after each run. k is the least-squares slope of log(time) against log(N).
Windows 11, buildc 1.4.0, gcc 13.2 -O2, one run per size, so treat single
timings as rough to about 2x; the old-build runs shared the machine with other
work. The before column stops where one run passed a few minutes.

| Scenario | Before: time at N | Before: k | After: time at N | After: k |
|----------|-------------------|-----------|------------------|----------|
| autocommit | 15.0 s at 1k, 97.8 s at 2k | 1.78 (N=500..2k) | 0.13 s at 1k, 27.6 s at 100k | 1.16 (N=1k..100k) |
| txn | 10.0 s at 1k, 40.6 s at 4k | 1.01 (N=1k..4k) | 0.14 s at 1k, 18.5 s at 100k | 1.07 (N=1k..100k) |
| txn_indexed | 21.3 s at 1k | 1.94 (N=250..1k) | 0.14 s at 1k, 15.0 s at 100k | 1.02 (N=1k..100k) |
| interleaved | 32.7 s at 1k | 1.61 (N=250..1k) | 0.20 s at 1k, 13.3 s at 16k | 1.52 (N=1k..16k) |

`autocommit` is N single-row INSERTs, `txn` wraps them in BEGIN/COMMIT,
`txn_indexed` adds two indexes and one indexed SELECT after COMMIT (so lazy
index work is inside the timing), and `interleaved` follows every INSERT with an
indexed SELECT. Before the change the `txn` run looks linear only because about
10 ms per statement of file rewriting hides the quadratic term below N=4,000.

What is still superlinear: `interleaved` (each read folds one new row into a
flattened index array, an O(n) splice, so the pair costs O(n)); INSERT into a
table that is not the last one created (the cells array is rebuilt, O(all
cells)); UPDATE and DELETE (they rebuild arrays, as before); and ORDER BY
(selection sort). The remaining per-row cost after the change, about 130 to
280 us, is mostly one `append_file` open and close per statement.

## Design Tradeoffs

| Decision | Rationale |
|----------|-----------|
| Single flat struct | Retained pattern; buildc 1.2.0 now supports local struct field assignment |
| String pool in a `Vec<String>` | O(1) intern and lookup; ids keep cells as `Vec<i32>` |
| Text file format | Human-readable, simple parsing, adequate for target scale |
| Selection sort for UNION, grouped and JOIN ORDER BY | O(n^2); single-table ORDER BY (sort_rows) and index builds use a stable merge sort, measured in benchmarks/bench_qdb_sort.py |
| Append-only WAL, deferred checkpoint | A write costs O(statement); the .db rewrite is amortized against WAL growth |
| Full array rebuilds | Most mutations rebuild their arrays; INSERT into the last table appends in place (`vec_push` and index assignment work on the runtime's heap vectors) |
| No B-tree | Sorted arrays sufficient; B-tree in flat arrays not justified |
| Embedded parser state | No separate AST struct; parsed state lives in DB fields |

## Potential Future Modules

If BuildLang adds multi-file imports, the sections map directly:

```
db_primitives.bld   -- Section 0: char helpers
db_model.bld        -- Section 1: struct DB, constructor, string pool
db_storage.bld      -- Section 2: file I/O
db_wal.bld          -- Section 3: WAL + transactions
db_tokenizer.bld    -- Section 4: SQL lexer
db_parser.bld       -- Section 5: SQL parser
db_executor.bld     -- Section 6: query execution
db_helpers.bld      -- Section 7: WHERE eval, sorting, formatting
db_index.bld        -- Section 8: index operations
db_repl.bld         -- Sections 9-11: dispatch, REPL, main
```
