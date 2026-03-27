# qdb Architecture

## Overview

`qdb` is a SQL database engine written in ~4,500 lines of QuantaLang (103 functions, 1 struct). It implements a substantial subset of SQL including DDL, DML, queries with JOIN/GROUP BY/ORDER BY/HAVING, sorted-array indexes with binary search, and transactions with WAL crash recovery. It persists data via a human-readable text file format and includes an interactive REPL.

## Why One File?

QuantaLang does not support multi-file compilation or `use` imports between `.quanta` files. All code must reside in a single file. Within the file, strict section boundaries with documentation headers maintain logical modularity. If QuantaLang adds multi-file support in the future, each section maps cleanly to a standalone module.

## Section Map

```
Section                          Lines         Functions   Purpose
-------------------------------  -----------   ---------   ----------------------------------
 0. PRIMITIVES                   ~81-97        6           Character classification (is_ws, is_digit, etc.)
 1. DATA MODEL                   ~99-325       3           struct DB (139 fields), db_new, string pool
 2. STORAGE                      ~327-595      2           File save/load (text format)
 3. WAL                          ~596-940      10          Write-ahead log, snapshots, transactions
 4. TOKENIZER                    ~953-1185     8           SQL lexical analysis
 5. PARSER                       ~1186-2113    18          Recursive-descent SQL parsing
 6. EXECUTOR                     ~2115-3640    26          Query execution engine
    6a. DDL                                    2             CREATE TABLE, DROP TABLE
    6b. DML                                    3             INSERT, UPDATE, DELETE
    6c. SELECT (basic)                         1             Single-table with WHERE/ORDER/LIMIT/DISTINCT
    6d. SELECT (aggregates)                    3             COUNT/SUM/AVG/MIN/MAX without GROUP BY
    6e. SELECT (GROUP BY)                      6             GROUP BY + HAVING
    6f. SELECT (JOIN)                          2             Nested-loop join
    6g. Subqueries                             2             Scalar subqueries in WHERE
    6h. ALTER TABLE                            1             ADD COLUMN
 7. HELPERS                      ~3643-4055    17          WHERE eval, LIKE, sorting, formatting, conversions
 8. INDEX                        ~4056-4326    5           Sorted-array indexes, binary search, CREATE INDEX
 9. DISPATCH                     ~4327-4389    2           exec_sql / exec_sql_no_wal routing
10. REPL                         ~4390-4489    3           Interactive shell, dot-commands
11. MAIN                         ~4490-end     1           Entry point, arg handling
```

## Data Model

### The DB Struct (139 fields)

All engine state lives in one flat struct. This is a QuantaLang constraint, not a design preference — the C codegen cannot reliably emit struct field assignments on local variables.

The struct fields are grouped into:

| Group | Fields | Purpose |
|-------|--------|---------|
| File | `db_path` | Path to the `.db` file |
| String Pool | `pool`, `ps`, `pl`, `pn` | Interned string buffer + start/length arrays |
| Tables | `tbl_names`, `tbl_count`, `tbl_col_starts`, `tbl_col_counts` | Table metadata |
| Columns | `col_names`, `col_types` | Flattened column schema |
| Rows | `cells`, `tbl_row_starts`, `tbl_row_counts` | Flattened cell data |
| Indexes | `idx_names`, `idx_tables`, `idx_col_indices`, `idx_keys`, `idx_rowids`, ... | Sorted-array index data |
| Parser | `input`, `pos`, `tk_type`, `tk_val`, `stmt_type`, `p_table`, `p_cols`, ... | Tokenizer + AST state |
| WHERE | `has_where`, `w_left`, `w_op`, `w_right`, `w_logic`, `w_in_vals1`, ... | Parsed WHERE conditions |
| WAL | `wal_path`, `in_transaction`, `txn_id`, `wal_buf` | WAL state |
| Snapshot | `snap_cells`, `snap_row_starts`, `snap_tbl_names`, ... | Rollback snapshot |

### String Pool Pattern

All strings are interned into a contiguous buffer (`d.pool`). `sp_add(text)` appends to the buffer and returns an integer ID. `sp_get(id)` retrieves text by ID. This avoids `Vec<String>` which has incomplete codegen support in QuantaLang.

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

Each `ROW` uses pipe-delimited values. The entire file is read/written atomically (no incremental I/O).

## WAL (Write-Ahead Log)

Append-only log at `<db_path>.wal`:

```
TXID 1
OP INSERT INTO users VALUES (3, 'Alice', 28)
OP INSERT INTO users VALUES (4, 'Bob', 32)
COMMIT
```

**Recovery**: On startup, `wal_replay()` scans for committed TXIDs and replays their OPs.

**Transactions**: `BEGIN` takes a snapshot. `COMMIT` writes the COMMIT marker and checkpoints. `ROLLBACK` restores from snapshot.

**Auto-commit**: Mutations outside explicit transactions are wrapped in single-statement transactions.

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
- **Maintenance**: Full rebuild after any mutation (INSERT/UPDATE/DELETE)
- **Sort algorithm**: Selection sort (O(n^2) but simple and correct)

## Design Tradeoffs

| Decision | Rationale |
|----------|-----------|
| Single flat struct | QuantaLang codegen limitation — no reliable local struct field assignment |
| String pool | `Vec<String>` codegen is incomplete |
| Text file format | Human-readable, simple parsing, adequate for target scale |
| Selection sort | O(n^2) but trivial to implement correctly without stdlib sort |
| Full array rebuilds | No in-place Vec mutation in QuantaLang — must reconstruct |
| No B-tree | Sorted arrays sufficient; B-tree in flat arrays not justified |
| Embedded parser state | No separate AST struct; parsed state lives in DB fields |

## Potential Future Modules

If QuantaLang adds multi-file imports, the sections map directly:

```
db_primitives.quanta   -- Section 0: char helpers
db_model.quanta        -- Section 1: struct DB, constructor, string pool
db_storage.quanta      -- Section 2: file I/O
db_wal.quanta          -- Section 3: WAL + transactions
db_tokenizer.quanta    -- Section 4: SQL lexer
db_parser.quanta       -- Section 5: SQL parser
db_executor.quanta     -- Section 6: query execution
db_helpers.quanta      -- Section 7: WHERE eval, sorting, formatting
db_index.quanta        -- Section 8: index operations
db_repl.quanta         -- Sections 9-11: dispatch, REPL, main
```
