#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# qdb regression harness -- the full SQL surface of bdb/db.bld in one run.
#
# Build bdb/qdb.exe first (see bdb/README.md), then: bash bdb/tests/test_qdb.sh
# Every qdb invocation is a separate process: main() loads the db, replays the
# WAL, runs one statement, and auto-commit checkpoints back to disk. Mutations
# therefore persist across calls, which is why each group re-seeds a pristine
# table before it mutates.
#
# Sections assert real rendered output. The XFAIL block records behaviors that
# diverge from full SQL by design (documented honest nulls); an XFAIL that
# starts passing is reported as XPASS so this file can be tightened, but it
# never fails the run.
set -o pipefail
cd "$(dirname "$0")"
PROGRAMS_DIR="$(cd .. && pwd)"
QDB="$PROGRAMS_DIR/qdb.exe"
DB=/tmp/qdb_regress.db
pass=0; fail=0; xfail=0; xpass=0

if [ ! -x "$QDB" ] && [ ! -f "$QDB" ]; then
  echo "SKIP: $QDB not found -- build it first (see bdb/README.md), then rerun."
  exit 0
fi

reseed() {
  rm -f "$DB" "$DB".wal 2>/dev/null
  $QDB "$DB" "CREATE TABLE emp (id INTEGER, name TEXT, dept TEXT, salary INTEGER, age INTEGER)" >/dev/null
  $QDB "$DB" "INSERT INTO emp VALUES (1,'alice','eng',100,30),(2,'bob','eng',90,45),(3,'carol','sales',80,35),(4,'dave','sales',120,50),(5,'eve','hr',70,28)" >/dev/null
}
# rows of a SELECT with header+separator stripped, each row trimmed both ends
rows() { $QDB "$DB" "$1" | tr -d '\r' | tail -n +3 \
    | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' | sed -e '/^$/d'; }
# full raw output (header included), CR stripped -- for header-label checks
raw()  { $QDB "$DB" "$1" | tr -d '\r'; }

chk() { # name  got  want
  local want; want="$(printf '%s' "$3")"
  if [ "$2" = "$want" ]; then echo "PASS  $1"; pass=$((pass+1))
  else echo "FAIL  $1"; echo "   want: [$want]"; echo "   got:  [$2]"; fail=$((fail+1)); fi
}
chksel() { chk "$1" "$(rows "$2")" "$3"; }              # assert SELECT rows
chkhdr() { chk "$1" "$(raw "$2" | head -1 | sed 's/[[:space:]]*$//')" "$3"; }  # assert header line
errchk() { # name  sql  needle  -- error printed AND emp table left intact
  local out base now
  out="$($QDB "$DB" "$2" 2>&1 | tr -d '\r')"
  base="$(printf '1\n2\n3\n4\n5')"; now="$(rows 'SELECT id FROM emp ORDER BY id')"
  if printf '%s' "$out" | grep -qi "$3" && [ "$now" = "$base" ]; then
    echo "PASS  $1"; pass=$((pass+1))
  else echo "FAIL  $1"; echo "   out: [$out]  rows-intact: $([ "$now" = "$base" ] && echo yes || echo NO)"; fail=$((fail+1)); fi
}
xfail() { # name  got  full-sql-correct-want  -- divergence expected; xpass if it matches
  local want; want="$(printf '%s' "$3")"
  if [ "$2" = "$want" ]; then echo "XPASS $1 (null closed -- tighten harness)"; xpass=$((xpass+1))
  else echo "xfail $1 (documented null)"; xfail=$((xfail+1)); fi
}

echo "== DDL + projection =="
reseed
chksel "project_one"   "SELECT name FROM emp WHERE id = 3" "carol"
chksel "project_all"   "SELECT id FROM emp ORDER BY id" "$(printf '1\n2\n3\n4\n5')"
chkhdr "hdr_star"      "SELECT * FROM emp LIMIT 0" "id | name | dept | salary | age"

echo "== WHERE: comparisons, DNF, NOT, IN, LIKE, BETWEEN, IS NULL =="
chksel "eq"            "SELECT name FROM emp WHERE dept = 'hr'" "eve"
chksel "lt"            "SELECT name FROM emp WHERE salary < 90 ORDER BY id" "$(printf 'carol\neve')"
chksel "ne"            "SELECT name FROM emp WHERE dept <> 'eng' ORDER BY id" "$(printf 'carol\ndave\neve')"
chksel "ne_bang"       "SELECT name FROM emp WHERE dept != 'eng' ORDER BY id" "$(printf 'carol\ndave\neve')"
chksel "ge"            "SELECT name FROM emp WHERE salary >= 100 ORDER BY id" "$(printf 'alice\ndave')"
chksel "le"            "SELECT name FROM emp WHERE salary <= 80 ORDER BY id" "$(printf 'carol\neve')"
chksel "and3"          "SELECT name FROM emp WHERE dept = 'eng' AND salary > 85 AND age < 40" "alice"
chksel "or3"           "SELECT name FROM emp WHERE dept='eng' OR dept='hr' OR dept='sales' ORDER BY id" "$(printf 'alice\nbob\ncarol\ndave\neve')"
chksel "and_or_prec"   "SELECT name FROM emp WHERE dept='eng' AND salary>95 OR dept='hr' ORDER BY id" "$(printf 'alice\neve')"
chksel "not_eq"        "SELECT name FROM emp WHERE NOT dept = 'eng' ORDER BY id" "$(printf 'carol\ndave\neve')"
chksel "in_list"       "SELECT name FROM emp WHERE dept IN ('eng','hr') ORDER BY id" "$(printf 'alice\nbob\neve')"
chksel "not_in"        "SELECT name FROM emp WHERE dept NOT IN ('eng','hr') ORDER BY id" "$(printf 'carol\ndave')"
chksel "like_prefix"   "SELECT name FROM emp WHERE name LIKE 'a%'" "alice"
chksel "not_like"      "SELECT name FROM emp WHERE name NOT LIKE 'a%' ORDER BY id" "$(printf 'bob\ncarol\ndave\neve')"
chksel "between"       "SELECT name FROM emp WHERE salary BETWEEN 80 AND 100 ORDER BY id" "$(printf 'alice\nbob\ncarol')"
chksel "between_and"   "SELECT name FROM emp WHERE salary BETWEEN 80 AND 120 AND dept='sales' ORDER BY id" "$(printf 'carol\ndave')"
chksel "paren_group"   "SELECT name FROM emp WHERE (dept='eng' OR dept='hr') AND salary > 85 ORDER BY id" "$(printf 'alice\nbob')"
chksel "not_between"   "SELECT name FROM emp WHERE salary NOT BETWEEN 80 AND 120 ORDER BY id" "eve"
chksel "not_paren"     "SELECT name FROM emp WHERE NOT (dept='eng' OR dept='hr') ORDER BY id" "$(printf 'carol\ndave')"
# IS NULL / IS NOT NULL need a null cell (emp has none), so use a dedicated table.
# NULL in INSERT stores SQL NULL, apart from ''; IS NULL matches only NULL.
$QDB "$DB" "CREATE TABLE nn (id INTEGER, name TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO nn VALUES (1,'alice'),(2,NULL),(3,'carol'),(4,'')" >/dev/null
chksel "is_null"       "SELECT id FROM nn WHERE name IS NULL" "2"
chksel "is_not_null"   "SELECT id FROM nn WHERE name IS NOT NULL ORDER BY id" "$(printf '1\n3\n4')"
chksel "empty_not_null" "SELECT id FROM nn WHERE name = ''" "4"
# Three-valued logic: a comparison with NULL is unknown, and NOT unknown is unknown.
chksel "not_eq_null"   "SELECT id FROM nn WHERE NOT name = 'alice' ORDER BY id" "$(printf '3\n4')"
chksel "ne_null"       "SELECT id FROM nn WHERE name != 'alice' ORDER BY id" "$(printf '3\n4')"
chksel "not_in_null"   "SELECT id FROM nn WHERE name NOT IN ('carol') ORDER BY id" "$(printf '1\n4')"
chksel "or_unknown"    "SELECT id FROM nn WHERE name = 'x' OR id = 2" "2"
chksel "null_renders"  "SELECT name FROM nn WHERE id = 2" "NULL"
chksel "count_col_empty" "SELECT COUNT(name) FROM nn" "3"
chksel "min_text_empty" "SELECT MIN(name) FROM nn" ""
chksel "order_null_first" "SELECT id FROM nn ORDER BY name" "$(printf '2\n4\n1\n3')"
chksel "order_null_last_desc" "SELECT id FROM nn ORDER BY name DESC" "$(printf '3\n1\n4\n2')"
$QDB "$DB" "UPDATE nn SET name = NULL WHERE id = 1" >/dev/null
chksel "update_set_null" "SELECT id FROM nn WHERE name IS NULL ORDER BY id" "$(printf '1\n2')"
chk   "reserved_null_text" "$($QDB "$DB" "$(printf "INSERT INTO nn VALUES (9, '\001')")" 2>&1 | tr -d '\r' | grep -ci 'reserved')" "1"
$QDB "$DB" "DROP TABLE nn" >/dev/null 2>&1

echo "== WHERE operator fail-closed (arithmetic / doubled / stray) =="
# op_encode returns -1 for any token that is not a comparison operator. Before
# the parse_condition guards, a leaf built with op -1 evaluated to always-false,
# so these queries returned zero rows with exit 0: a silent wrong answer. Each
# must now print a diagnostic, leave emp intact, and (checked below) exit nonzero.
# Arithmetic in WHERE is read by the expression path since P11 and answers.
reseed; chksel "where_arith_plus"  "SELECT id FROM emp WHERE salary + 10 > 100 ORDER BY id" "$(printf '1\n4')"
reseed; chksel "where_arith_pct"   "SELECT id FROM emp WHERE salary % 3 = 0 ORDER BY id"    "$(printf '2\n4')"
reseed; errchk "where_double_eq_closed"  "SELECT id FROM emp WHERE salary == 100"     "expected a value after operator"
reseed; errchk "where_gtlt_closed"       "SELECT id FROM emp WHERE salary >< 100"     "expected a value after operator"

echo "== WHERE unknown-column fail-closed (every single-table path) =="
# A WHERE leaf stores its left side as a column ref resolved at eval time.
# resolve_col returns -1 for a name no column has, and eval_cond_row scored that
# leaf false, so a typo'd column returned an empty result (and IS NULL on it
# returned every row): silent wrong answers on SELECT, GROUP BY, UPDATE, DELETE,
# and scalar subqueries. Each must now print "no such column" and touch no row.
reseed; errchk "where_unknown_select"   "SELECT id FROM emp WHERE nosuchcol = 5"                 "no such column"
reseed; errchk "where_unknown_agg"      "SELECT COUNT(*) FROM emp WHERE nosuchcol = 5"           "no such column"
reseed; errchk "where_unknown_group"    "SELECT dept, COUNT(*) FROM emp WHERE nosuchcol = 5 GROUP BY dept" "no such column"
reseed; errchk "where_unknown_update"   "UPDATE emp SET salary = 0 WHERE nosuchcol = 5"          "no such column"
reseed; errchk "where_unknown_delete"   "DELETE FROM emp WHERE nosuchcol = 5"                    "no such column"
reseed; errchk "where_unknown_isnull"   "SELECT id FROM emp WHERE nosuchcol IS NULL"             "no such column"
reseed; errchk "where_unknown_dotted"   "SELECT id FROM emp WHERE emp.nosuchcol = 5"             "no such column"
reseed; errchk "where_unknown_subquery" "SELECT id FROM emp WHERE salary > (SELECT MAX(salary) FROM emp WHERE nosuchcol = 1)" "no such column"

echo "== ORDER BY (multi-key) / LIMIT / OFFSET / DISTINCT =="
chksel "order_desc"    "SELECT name FROM emp ORDER BY salary DESC" "$(printf 'dave\nalice\nbob\ncarol\neve')"
chksel "order_2key"    "SELECT name FROM emp ORDER BY dept, salary DESC" "$(printf 'alice\nbob\neve\ndave\ncarol')"
chksel "limit"         "SELECT name FROM emp ORDER BY id LIMIT 2" "$(printf 'alice\nbob')"
chksel "limit_offset"  "SELECT name FROM emp ORDER BY id LIMIT 2 OFFSET 1" "$(printf 'bob\ncarol')"
chksel "distinct"      "SELECT DISTINCT dept FROM emp ORDER BY dept" "$(printf 'eng\nhr\nsales')"

echo "== numeric type aliases sort numerically (not lexicographic) =="
# A column declared with a SQL integer/real ALIAS (INT, BIGINT, FLOAT, ...) must
# order and right-justify as a number, exactly like INTEGER/REAL. The regression:
# only "INTEGER"/"REAL" were recognized, so an INT column sorted lexicographically
# -- "90" > "120" as text -- a silent wrong answer on ORDER BY. Uses dedicated
# tables so emp (declared INTEGER) stays pristine for the aggregate section.
$QDB "$DB" "CREATE TABLE tal_i (v INT)" >/dev/null
$QDB "$DB" "INSERT INTO tal_i VALUES (100),(90),(80),(120),(70)" >/dev/null
# Lexicographic DESC would give 90,80,70,120,100; numeric DESC is 120,100,90,80,70.
chksel "alias_int_order_desc"  "SELECT v FROM tal_i ORDER BY v DESC" "$(printf '120\n100\n90\n80\n70')"
chksel "alias_int_order_asc"   "SELECT v FROM tal_i ORDER BY v"      "$(printf '70\n80\n90\n100\n120')"
$QDB "$DB" "DROP TABLE tal_i" >/dev/null 2>&1
$QDB "$DB" "CREATE TABLE tal_b (v BIGINT)" >/dev/null
$QDB "$DB" "INSERT INTO tal_b VALUES (5),(100),(20)" >/dev/null
# Lexicographic ASC would give 100,20,5; numeric ASC is 5,20,100.
chksel "alias_bigint_order_asc" "SELECT v FROM tal_b ORDER BY v" "$(printf '5\n20\n100')"
$QDB "$DB" "DROP TABLE tal_b" >/dev/null 2>&1
$QDB "$DB" "CREATE TABLE tal_f (x FLOAT)" >/dev/null
$QDB "$DB" "INSERT INTO tal_f VALUES (2.5),(10.1),(1.9)" >/dev/null
# Lexicographic DESC would give 2.5,10.1,1.9; numeric DESC is 10.1,2.5,1.9.
chksel "alias_float_order_desc" "SELECT x FROM tal_f ORDER BY x DESC" "$(printf '10.1\n2.5\n1.9')"
$QDB "$DB" "DROP TABLE tal_f" >/dev/null 2>&1

echo "== ORDER BY key resolution: ordinal / alias / fail-closed (non-grouped) =="
# The non-grouped single-table ORDER BY path used to silently no-sort (return
# discovery order) whenever the key was not a real table column. It now resolves
# a positional ordinal (SQL-92) and a projection alias, and FAILS CLOSED on a key
# that resolves to nothing -- matching the grouped path's discipline. Without the
# fix each of these returned insert order (100,90,80,120,70) with no error.
# Positional ordinal into the projected list: position 1 = the projected column.
chksel "order_ordinal_proj"    "SELECT salary FROM emp ORDER BY 1 DESC"    "$(printf '120\n100\n90\n80\n70')"
# Projection alias on a plain (non-grouped) SELECT.
chksel "order_alias_plain"     "SELECT salary AS s FROM emp ORDER BY s DESC" "$(printf '120\n100\n90\n80\n70')"
# Secondary key by ordinal and by alias: dept ASC, then salary DESC within dept.
chksel "order_secondary_ord"   "SELECT dept, salary FROM emp ORDER BY dept, 2 DESC"      "$(printf 'eng   |    100\neng   |     90\nhr    |     70\nsales |    120\nsales |     80')"
chksel "order_secondary_alias" "SELECT dept, salary AS s FROM emp ORDER BY dept, s DESC" "$(printf 'eng   | 100\neng   |  90\nhr    |  70\nsales | 120\nsales |  80')"
# Positional ordinal against SELECT * maps onto table columns (col 1 = v).
$QDB "$DB" "CREATE TABLE o8 (v INTEGER)" >/dev/null
$QDB "$DB" "INSERT INTO o8 VALUES (100),(90),(80),(120),(70)" >/dev/null
chksel "order_ordinal_star"    "SELECT * FROM o8 ORDER BY 1 DESC" "$(printf '120\n100\n90\n80\n70')"
$QDB "$DB" "DROP TABLE o8" >/dev/null 2>&1
# Fail closed: unknown column, out-of-range ordinal, ordinal 0, unresolved secondary.
reseed; errchk "order_unknown_closed"      "SELECT name FROM emp ORDER BY nonesuch"             "must name"
reseed; errchk "order_ordinal_oor_closed"  "SELECT name, salary FROM emp ORDER BY 9"            "must name"
reseed; errchk "order_ordinal_zero_closed" "SELECT name FROM emp ORDER BY 0"                    "must name"
reseed; errchk "order_secondary_closed"    "SELECT name, salary FROM emp ORDER BY salary, nope" "must name"

echo "== aggregates / GROUP BY / HAVING =="
chksel "count_star"    "SELECT COUNT(*) FROM emp" "5"
chksel "sum"           "SELECT SUM(salary) FROM emp" "460"
chksel "min_max"       "SELECT MIN(salary) FROM emp" "70"
chksel "max_salary"    "SELECT MAX(salary) FROM emp" "120"
# MIN/MAX on a TEXT column order lexicographically (compare_vals), not by
# parse_float -- which coerces text to 0 and used to collapse every text
# extreme to 0. SUM/AVG on text stay numeric (0), matching SQLite coercion.
chksel "min_name"      "SELECT MIN(name) FROM emp" "alice"
chksel "max_name"      "SELECT MAX(name) FROM emp" "eve"
chksel "min_dept"      "SELECT MIN(dept) FROM emp" "eng"
chksel "max_dept"      "SELECT MAX(dept) FROM emp" "sales"
chksel "group_min_name" "SELECT dept, MIN(name) FROM emp GROUP BY dept ORDER BY dept" "$(printf 'eng   | alice\nhr    | eve\nsales | carol')"
chksel "group_max_name" "SELECT dept, MAX(name) FROM emp GROUP BY dept ORDER BY dept" "$(printf 'eng   | bob\nhr    | eve\nsales | dave')"
chksel "subq_min_name" "SELECT id FROM emp WHERE name = (SELECT MIN(name) FROM emp)" "1"
chksel "subq_max_name" "SELECT id FROM emp WHERE name = (SELECT MAX(name) FROM emp)" "5"
# Empty match set. An aggregate with no GROUP BY still returns exactly one row.
# MIN/MAX over zero rows is SQL NULL; before, the internal float sentinel
# leaked out as 12157520.92 / -12157520.92 (a silent wrong answer). COUNT stays 0
# (SQLite-correct); SUM/AVG stay 0 here (a kept divergence from SQLite's NULL, and
# the AVG path returns 0 rather than dividing by a zero count).
chksel "empty_min_num"  "SELECT MIN(salary) FROM emp WHERE id > 999" "NULL"
chksel "empty_max_num"  "SELECT MAX(salary) FROM emp WHERE id > 999" "NULL"
chksel "empty_count"    "SELECT COUNT(*) FROM emp WHERE id > 999" "0"
# SUM and AVG over zero rows are NULL too, as in SQL (they were 0 before P7).
chksel "empty_sum"      "SELECT SUM(salary) FROM emp WHERE id > 999" "NULL"
chksel "empty_avg"      "SELECT AVG(salary) FROM emp WHERE id > 999" "NULL"
# AVG divides by the count as a float and keeps every digit (P8): before, the
# divide was an integer division and AVG(age) printed 37.
chksel "avg_fraction"   "SELECT AVG(age) FROM emp" "37.6"
chksel "avg_group_fraction" "SELECT dept, AVG(age) FROM emp GROUP BY dept ORDER BY dept" "$(printf 'eng   | 37.5
hr    | 28
sales | 42.5')"
chksel "avg_having_fraction" "SELECT dept FROM emp GROUP BY dept HAVING AVG(age) > 37 ORDER BY dept" "$(printf 'eng
sales')"
chkhdr "hdr_count"     "SELECT COUNT(*) FROM emp" "COUNT(*)"
chksel "group_count"   "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY dept" "$(printf 'eng   | 2\nhr    | 1\nsales | 2')"
chksel "group_desc"    "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY dept DESC" "$(printf 'sales | 2\nhr    | 1\neng   | 2')"
chksel "group_sum_ord" "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY dept" "$(printf 'eng   | 190\nhr    | 70\nsales | 200')"
chksel "having"        "SELECT dept FROM emp GROUP BY dept HAVING COUNT(*) > 1 ORDER BY dept" "$(printf 'eng\nsales')"
# HAVING operator fail-closed. parse_having stores op_encode(op) in having_op, and
# eval_where treats -1 as always-false, so an arithmetic or stray operator dropped
# every group and a doubled operator compared the aggregate to a symbol: each
# returned an empty result with exit 0, a silent wrong answer (the WHERE-side of
# this was Fork 12). Guard 1 rejects an unrecognized operator, guard 2 rejects an
# operator token in the value position (== and ><).
reseed; errchk "having_arith_plus_closed" "SELECT dept, COUNT(*) FROM emp GROUP BY dept HAVING COUNT(*) + 0 > 1"   "unsupported operator in HAVING"
reseed; errchk "having_arith_pct_closed"  "SELECT dept, SUM(salary) FROM emp GROUP BY dept HAVING SUM(salary) % 2 = 0" "unsupported operator in HAVING"
reseed; errchk "having_double_eq_closed"  "SELECT dept, COUNT(*) FROM emp GROUP BY dept HAVING COUNT(*) == 1"      "expected a value after operator"
reseed; errchk "having_gtlt_closed"       "SELECT dept, COUNT(*) FROM emp GROUP BY dept HAVING COUNT(*) >< 1"      "expected a value after operator"
# Aggregate over an unknown column, fail closed. resolve_col returns -1 for a name
# no column matches, and each aggregate compute site then read zero cells. SUM, AVG
# and COUNT(col) returned 0, MIN and MAX returned blank, a grouped aggregate
# returned 0 per group, a HAVING aggregate dropped every group, and a scalar
# subquery fed 0 into the outer comparison. Every one ran with exit 0, a silent
# wrong answer. agg_cols_ok validates each projection aggregate argument and the
# HAVING aggregate argument before dispatch, guarded in exec_select,
# exec_select_group and exec_subquery_scalar. COUNT(*) and the DISTINCT flag are
# skipped, and a plain SELECT column keeps its own existing not-found path. A
# scalar aggregate over a JOIN, and GROUP BY over a JOIN, now fold over the joined
# rows (JOIN section below); HAVING/ORDER BY over a grouped JOIN and COUNT(DISTINCT)
# over a JOIN still fail closed there.
reseed; errchk "agg_unknown_sum"          "SELECT SUM(badcol) FROM emp"                                           "no such column"
reseed; errchk "agg_unknown_avg"          "SELECT AVG(badcol) FROM emp"                                           "no such column"
reseed; errchk "agg_unknown_min"          "SELECT MIN(badcol) FROM emp"                                           "no such column"
reseed; errchk "agg_unknown_count"        "SELECT COUNT(badcol) FROM emp"                                         "no such column"
reseed; errchk "agg_unknown_group"        "SELECT dept, SUM(badcol) FROM emp GROUP BY dept"                       "no such column"
reseed; errchk "agg_unknown_having_sum"   "SELECT dept, COUNT(*) FROM emp GROUP BY dept HAVING SUM(badcol) > 0"   "no such column"
reseed; errchk "agg_unknown_having_count" "SELECT dept, COUNT(*) FROM emp GROUP BY dept HAVING COUNT(badcol) > 0" "no such column"
reseed; errchk "agg_unknown_subquery"     "SELECT name FROM emp WHERE salary > (SELECT AVG(badcol) FROM emp)"     "no such column"
# ORDER BY an aggregate term. The order key matches the aggregate output column
# by func + arg and sorts numerically. Ties keep group-discovery order (eng
# before sales). SUM proves the numeric path (lexical "200" < "70" would be wrong).
chksel "group_ord_agg"      "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY COUNT(*)" "$(printf 'hr    | 1\neng   | 2\nsales | 2')"
chksel "group_ord_cnt_desc" "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY COUNT(*) DESC" "$(printf 'eng   | 2\nsales | 2\nhr    | 1')"
chksel "group_ord_sum_desc" "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY SUM(salary) DESC" "$(printf 'sales | 200\neng   | 190\nhr    | 70')"
# ORDER BY a projection ALIAS of an aggregate. `s`/`c` are bare identifiers, so
# they parse as plain keys; resolution falls through to the alias binding and
# sorts by the aliased aggregate column (previously left in discovery order).
chksel "group_ord_alias_sum_desc" "SELECT dept, SUM(salary) AS s FROM emp GROUP BY dept ORDER BY s DESC" "$(printf 'sales | 200\neng   | 190\nhr    | 70')"
chksel "group_ord_alias_sum_asc"  "SELECT dept, SUM(salary) AS s FROM emp GROUP BY dept ORDER BY s"      "$(printf 'hr    | 70\neng   | 190\nsales | 200')"
chksel "group_ord_alias_cnt_desc" "SELECT dept, COUNT(*) AS c FROM emp GROUP BY dept ORDER BY c DESC"    "$(printf 'eng   | 2\nsales | 2\nhr    | 1')"
# ORDER BY a positional ordinal over a grouped result: a bare integer is a
# 1-based position into the output columns (ORDER BY 2 = the aggregate), the
# same rule the non-grouped and UNION paths use. The matched column's own func
# drives the sort type, so ORDER BY 2 sorts the SUM numerically while ORDER BY 1
# sorts dept as text. The pre-fix binary read "2" as a column name and failed
# closed, so each positive answer separates the fixed engine from the guard.
chksel "group_ord_pos2_sum_asc"  "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY 2"      "$(printf 'hr    | 70\neng   | 190\nsales | 200')"
chksel "group_ord_pos2_sum_desc" "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY 2 DESC" "$(printf 'sales | 200\neng   | 190\nhr    | 70')"
chksel "group_ord_pos1_txt_desc" "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY 1 DESC" "$(printf 'sales | 200\nhr    | 70\neng   | 190')"
reseed; errchk "group_ord_pos_oor_closed"  "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY 3" "out of range"
reseed; errchk "group_ord_pos_zero_closed" "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY 0" "out of range"
# Fail closed: an ORDER BY term that names neither the grouped column, an
# aggregate output, nor a select alias has no defined ordering -- error rather
# than emit rows in arbitrary discovery order.
reseed; errchk "group_ord_unresolved_closed" "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY age" "grouped column, aggregate, or alias"

echo "== COUNT(DISTINCT) =="
reseed
# Non-grouped: 3 distinct depts among 5 rows -- distinct (3) diverges from a
# plain COUNT(dept) of 5, proving dedup on the no-group aggregate path.
chksel "count_distinct_dept_nogroup" "SELECT COUNT(DISTINCT dept) FROM emp" "3"
# Grouped: add a duplicate salary in eng (frank, 100) so the distinct count (2)
# diverges from the eng row count (3), proving per-group dedup. ORDER BY dept
# gives a stable listing. Reseeded afterward so later tests see the base rows.
"$QDB" "$DB" "INSERT INTO emp VALUES (6,'frank','eng',100,60)" >/dev/null 2>&1
chksel "count_distinct_grouped"     "SELECT dept, COUNT(DISTINCT salary) FROM emp GROUP BY dept ORDER BY dept"          "$(printf 'eng   | 2\nhr    | 1\nsales | 2')"
# ORDER BY the distinct-count alias, DESC; eng/sales tie at 2 -> discovery order.
chksel "count_distinct_alias_order" "SELECT dept, COUNT(DISTINCT salary) AS ds FROM emp GROUP BY dept ORDER BY ds DESC" "$(printf 'eng   | 2\nsales | 2\nhr    | 1')"
reseed
# Fail closed: DISTINCT outside COUNT, and COUNT(DISTINCT *), are rejected rather
# than silently ignored; the table stays intact.
errchk "count_distinct_sum_closed"  "SELECT SUM(DISTINCT salary) FROM emp" "DISTINCT is only supported inside COUNT"
errchk "count_distinct_star_closed" "SELECT COUNT(DISTINCT *) FROM emp"    "is not supported; use COUNT"

echo "== subqueries (scalar + IN) =="
reseed
# Scalar subquery in WHERE: AVG(salary)=460/5=92, so salary>92 -> alice(100),dave(120).
chksel "subq_scalar"   "SELECT name FROM emp WHERE salary > (SELECT AVG(salary) FROM emp) ORDER BY id" "$(printf 'alice\ndave')"
# IN-subquery: depts of rows with age>40 are {eng(bob),sales(dave)}; names there are all but eve.
chksel "subq_in"       "SELECT name FROM emp WHERE dept IN (SELECT dept FROM emp WHERE age > 40) ORDER BY id" "$(printf 'alice\nbob\ncarol\ndave')"

echo "== UPDATE: literal, arithmetic (+ - * /), fail-closed =="
reseed; $QDB "$DB" "UPDATE emp SET salary = 500 WHERE name='alice'" >/dev/null
chksel "upd_literal"   "SELECT salary FROM emp WHERE name='alice'" "500"
reseed; $QDB "$DB" "UPDATE emp SET salary = salary + 5 WHERE name='bob'" >/dev/null
chksel "upd_plus"      "SELECT salary FROM emp WHERE name='bob'" "95"
reseed; $QDB "$DB" "UPDATE emp SET salary = salary * 2 WHERE name='carol'" >/dev/null
chksel "upd_star"      "SELECT salary FROM emp WHERE name='carol'" "160"
chksel "upd_star_safe" "SELECT salary FROM emp WHERE name='alice'" "100"
reseed; $QDB "$DB" "UPDATE emp SET salary = salary / 4 WHERE id=1" >/dev/null
chksel "upd_slash"     "SELECT salary FROM emp WHERE id=1" "25"
reseed; errchk "upd_divzero"  "UPDATE emp SET salary = salary / 0 WHERE id=1" "division by zero"
reseed; errchk "upd_badop"    "UPDATE emp SET salary = salary + 'x' WHERE id=1" "operand must be an integer"
reseed; errchk "upd_text"     "UPDATE emp SET name = name + 1 WHERE id=1" "non-integer value"
reseed; errchk "upd_stray"    "UPDATE emp SET salary = salary 2 WHERE name='carol'" "unexpected token"

echo "== DELETE =="
reseed; $QDB "$DB" "DELETE FROM emp WHERE dept='sales'" >/dev/null
chksel "delete"        "SELECT name FROM emp ORDER BY id" "$(printf 'alice\nbob\neve')"

echo "== INDEX (equality fast-path returns same rows) =="
reseed; $QDB "$DB" "CREATE INDEX ix ON emp (salary)" >/dev/null
chksel "index_eq"      "SELECT name FROM emp WHERE salary = 100" "alice"
chksel "index_gt"      "SELECT name FROM emp WHERE salary > 100" "dave"
# NOT over an indexed-column equality must bypass the fast-path (wn_count>1) and
# full-scan: the answer is the complement, not the indexed match.
chksel "not_index"     "SELECT name FROM emp WHERE NOT salary = 100 ORDER BY id" "$(printf 'bob\ncarol\ndave\neve')"

echo "== JOIN =="
reseed
$QDB "$DB" "CREATE TABLE dep (dname TEXT, loc TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO dep VALUES ('eng','b1'),('sales','b2'),('hr','b3')" >/dev/null
chksel "join"          "SELECT emp.name FROM emp JOIN dep ON emp.dept = dep.dname WHERE dep.loc='b1' ORDER BY emp.name" "$(printf 'alice\nbob')"
# A two-column dotted projection still returns one row per joined row. This is the
# exact shape the aggregate-over-join guard must not reject: p_agg_count is 2 here
# (both plain columns), so a guard keyed on p_agg_count alone would wrongly fire.
chksel "join_multicol"  "SELECT emp.name, dep.loc FROM emp JOIN dep ON emp.dept = dep.dname WHERE dep.loc='b1' ORDER BY emp.name" "$(printf 'alice | b1\nbob   | b1')"
# A scalar aggregate over a join with no GROUP BY folds over the assembled pairs:
# COUNT(*) counts joined rows, SUM/AVG/MIN/MAX read the referenced table's column
# on each pair, and COUNT(col) counts non-empty cells. The guard scans p_agg_funcs
# per projection item, so join_multicol above (two plain columns, p_agg_count 2) is
# not mistaken for an aggregate. The emp/dep join matches all five emp rows.
chksel "join_agg_count"     "SELECT COUNT(*) FROM emp JOIN dep ON emp.dept = dep.dname" "5"
chksel "join_agg_sum"       "SELECT SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname" "460"
chksel "join_agg_min"       "SELECT MIN(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname" "70"
chksel "join_agg_max"       "SELECT MAX(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname" "120"
chksel "join_agg_avg"       "SELECT AVG(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname" "92"
chksel "join_agg_countcol"  "SELECT COUNT(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname" "5"
# The fold honors WHERE: loc='b1' keeps only the two eng rows before aggregating.
chksel "join_agg_where_sum" "SELECT SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname WHERE dep.loc='b1'" "190"
chksel "join_agg_where_cnt" "SELECT COUNT(*) FROM emp JOIN dep ON emp.dept = dep.dname WHERE dep.loc='b1'" "2"
# Text-typed MIN/MAX over a join compare lexically, matching single-table MIN/MAX.
chksel "join_agg_min_text"  "SELECT MIN(emp.name) FROM emp JOIN dep ON emp.dept = dep.dname" "alice"
chksel "join_agg_max_text"  "SELECT MAX(emp.name) FROM emp JOIN dep ON emp.dept = dep.dname" "eve"
# GROUP BY over a join buckets the assembled pairs by the single group column and
# folds each aggregate per bucket: one row per group, in left-scan discovery order.
# The group key resolves against either table (dep.loc and dep.dname on the right,
# emp.dept on the left). WHERE filters pairs before bucketing, so a group whose rows
# all fail the predicate drops out (b3/hr below, whose one row has salary 70).
chksel "join_group_sum"    "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc" "$(printf 'b1      | 190\nb2      | 200\nb3      | 70')"
chksel "join_group_cnt"    "SELECT dep.dname, COUNT(*) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.dname" "$(printf 'eng       | 2\nsales     | 2\nhr        | 1')"
chksel "join_group_minmax" "SELECT emp.dept, MIN(emp.salary), MAX(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY emp.dept" "$(printf 'eng      | 90              | 100\nsales    | 80              | 120\nhr       | 70              | 70')"
chksel "join_group_avg"    "SELECT dep.loc, AVG(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc" "$(printf 'b1      | 95\nb2      | 100\nb3      | 70')"
chksel "join_group_where"  "SELECT dep.loc, COUNT(*) FROM emp JOIN dep ON emp.dept = dep.dname WHERE emp.salary > 75 GROUP BY dep.loc" "$(printf 'b1      | 2\nb2      | 2')"
# HAVING over a grouped join folds the HAVING aggregate over each group's bucket
# and keeps only groups that pass. The HAVING aggregate is independent of the
# projected one: maxcross projects COUNT(*) but filters on MAX(salary), and text
# filters on a TEXT-typed MIN. When every group fails the test the result is an
# empty-but-well-formed table -- header, zero rows, exit 0 -- never all groups and
# never an error.
chksel "join_group_having_sum"      "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc HAVING SUM(emp.salary) > 100" "$(printf 'b1      | 190\nb2      | 200')"
chksel "join_group_having_cnt"      "SELECT dep.dname, COUNT(*) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.dname HAVING COUNT(*) > 1" "$(printf 'eng       | 2\nsales     | 2')"
chksel "join_group_having_maxcross" "SELECT emp.dept, COUNT(*) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY emp.dept HAVING MAX(emp.salary) >= 120" "sales    | 2"
chksel "join_group_having_text"     "SELECT emp.dept, COUNT(*) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY emp.dept HAVING MIN(emp.name) = 'carol'" "sales    | 2"
chkhdr "join_group_having_emptyhdr" "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc HAVING SUM(emp.salary) > 999" "dep.loc | SUM(emp.salary)"
chk    "join_group_having_emptyrows" "$(rows "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc HAVING SUM(emp.salary) > 999" | grep -c .)" "0"
# ORDER BY over a grouped join sorts the folded groups. The key names the
# grouped column, an aggregate term, or a projection alias; it resolves to an
# output column, and a stable sort keeps equal keys in group-discovery order.
# COUNT sorts as an integer, SUM/AVG/MIN/MAX as a real, a plain key by its
# source type. WHERE and HAVING run first, so the sort orders exactly the rows
# that survive both.
chksel "join_group_order_agg_asc"   "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY SUM(emp.salary)"      "$(printf 'b3      | 70\nb1      | 190\nb2      | 200')"
chksel "join_group_order_agg_desc"  "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY SUM(emp.salary) DESC" "$(printf 'b2      | 200\nb1      | 190\nb3      | 70')"
chksel "join_group_order_col_desc"  "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY dep.loc DESC"         "$(printf 'b3      | 70\nb2      | 200\nb1      | 190')"
chksel "join_group_order_real_max"  "SELECT emp.dept, MAX(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY emp.dept ORDER BY MAX(emp.salary)"    "$(printf 'hr       | 70\neng      | 100\nsales    | 120')"
# ORDER BY an alias resolves against the projection alias pool; ASC flips the
# discovery order (eng, sales, hr) to put the single-row hr group first, and the
# eng/sales tie holds its discovery order.
chksel "join_group_order_alias_asc" "SELECT dep.dname, COUNT(*) AS n FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.dname ORDER BY n ASC"               "$(printf 'hr        | 1\neng       | 2\nsales     | 2')"
# A positional ordinal resolves to an output column (ORDER BY 2 = the SUM),
# sorting by the matched column's own type: numeric for the aggregate, so
# ORDER BY 2 walks 70, 190, 200 and ORDER BY 2 DESC reverses it. The pre-fix
# binary read "2" as a column name and failed closed, so each ordered answer
# separates the fixed engine from the guard it replaced.
chksel "join_group_order_ord2_asc"  "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY 2"      "$(printf 'b3      | 70\nb1      | 190\nb2      | 200')"
chksel "join_group_order_ord2_desc" "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY 2 DESC" "$(printf 'b2      | 200\nb1      | 190\nb3      | 70')"
# A secondary key still fails closed (honest partial, candidate next fork); an
# out-of-range ordinal and an unresolvable name fail closed with accurate,
# distinct messages -- never a silent wrong order.
chk "join_group_order_secondary_closed" "$($QDB "$DB" "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY dep.loc, SUM(emp.salary)" 2>&1 | tr -d '\r' | grep -ci 'secondary')" "1"
chk "join_group_order_ord_oor_closed"   "$($QDB "$DB" "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY 9" 2>&1 | tr -d '\r' | grep -ci 'out of range')" "1"
chk "join_group_order_unresolv_closed"  "$($QDB "$DB" "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY emp.name" 2>&1 | tr -d '\r' | grep -ci 'not a grouped column')" "1"
# LIMIT / OFFSET over a grouped join truncate the group rows. Applied after the
# ORDER BY sort, so "ORDER BY ... LIMIT n" is the top-n idiom; LIMIT with no
# ORDER BY keeps group-discovery order (b1, b2, b3). OFFSET skips whole groups,
# HAVING runs before LIMIT, and LIMIT 0 or an OFFSET past the last group renders
# the header with zero data rows. The pre-fix binary answered every LIMIT/OFFSET
# query with "LIMIT over a grouped JOIN is not supported", so each value and
# header assertion separates the fixed engine from the guard it replaced.
chksel "join_group_limit_topn"      "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY SUM(emp.salary) LIMIT 2"          "$(printf 'b3      | 70\nb1      | 190')"
chksel "join_group_limit_desc_topn" "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY SUM(emp.salary) DESC LIMIT 2"     "$(printf 'b2      | 200\nb1      | 190')"
chksel "join_group_limit_offset"    "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc ORDER BY SUM(emp.salary) LIMIT 1 OFFSET 1" "b1      | 190"
chksel "join_group_limit_noorder"   "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc LIMIT 2"                                    "$(printf 'b1      | 190\nb2      | 200')"
chksel "join_group_limit_having"    "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc HAVING SUM(emp.salary) > 100 ORDER BY SUM(emp.salary) LIMIT 1" "b1      | 190"
chkhdr "join_group_limit_zero_hdr"  "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc LIMIT 0" "dep.loc | SUM(emp.salary)"
chk    "join_group_limit_zero_rows" "$(rows "SELECT dep.loc, SUM(emp.salary) FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc LIMIT 0" | grep -c .)" "0"
# A bare GROUP BY with no aggregate and COUNT(DISTINCT) over a join stay
# fail-closed: each is an honest partial with one accurate message.
chk "join_group_noagg_closed"  "$($QDB "$DB" "SELECT dep.loc FROM emp JOIN dep ON emp.dept = dep.dname GROUP BY dep.loc" 2>&1 | tr -d '\r' | grep -ci 'not supported')" "1"
chk "join_distinct_closed"     "$($QDB "$DB" "SELECT COUNT(DISTINCT emp.dept) FROM emp JOIN dep ON emp.dept = dep.dname" 2>&1 | tr -d '\r' | grep -ci 'not supported')" "1"

echo "== LEFT [OUTER] JOIN (null-extend unmatched left rows) =="
# Dedicated pair: la carries three tags, lb covers only x and z, so the 'y' row
# has no match. INNER drops it; LEFT keeps it with a NULL right column.
# These assertions select without ORDER BY, so rows render in left-table scan
# order; la is inserted 1,2,3 so that order is already sorted. ORDER BY inside a
# join is covered in the "ORDER BY inside JOIN" section below.
$QDB "$DB" "CREATE TABLE la (id INTEGER, tag TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO la VALUES (1,'x'),(2,'y'),(3,'z')" >/dev/null
$QDB "$DB" "CREATE TABLE lb (tag TEXT, note TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO lb VALUES ('x','has-x'),('z','has-z')" >/dev/null
chksel "join_inner_drop"   "SELECT la.id, lb.note FROM la JOIN lb ON la.tag = lb.tag" "$(printf '1  | has-x\n3  | has-z')"
chksel "left_join_null"    "SELECT la.id, lb.note FROM la LEFT JOIN lb ON la.tag = lb.tag" "$(printf '1  | has-x\n2  | NULL\n3  | has-z')"
chksel "left_outer_join"   "SELECT la.id, lb.note FROM la LEFT OUTER JOIN lb ON la.tag = lb.tag" "$(printf '1  | has-x\n2  | NULL\n3  | has-z')"
# WHERE on the null-extended right column: IS NULL selects exactly the unmatched
# left row; IS NOT NULL and equality keep matched rows only (null excluded).
chksel "left_join_isnull"  "SELECT la.id FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE lb.note IS NULL" "2"
chksel "left_join_notnull" "SELECT la.id FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE lb.note IS NOT NULL" "$(printf '1\n3')"
chksel "left_join_reqeq"   "SELECT la.id FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE lb.note = 'has-x'" "1"
# WHERE on the preserved (left) column filters left rows, null-extension intact.
chksel "left_join_leftwh"  "SELECT la.id, lb.note FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE la.id > 1" "$(printf '2  | NULL\n3  | has-z')"
# Scalar aggregate over a LEFT JOIN folds the null-extended row in: COUNT(*) counts
# all three left rows, but COUNT(right col) skips the unmatched row's blank cell.
chksel "left_join_agg_cnt"     "SELECT COUNT(*) FROM la LEFT JOIN lb ON la.tag = lb.tag" "3"
chksel "left_join_agg_cntnote" "SELECT COUNT(lb.note) FROM la LEFT JOIN lb ON la.tag = lb.tag" "2"
chksel "left_join_agg_sum"     "SELECT SUM(la.id) FROM la LEFT JOIN lb ON la.tag = lb.tag" "6"
# GROUP BY over a LEFT JOIN preserves the null-extended row in its own bucket: every
# tag is its own group, so COUNT(*) counts the unmatched 'y' row (1) while COUNT of the
# right column skips its blank cell (0), exactly as the ungrouped fold does.
chksel "left_join_group_cnt"     "SELECT la.tag, COUNT(*) FROM la LEFT JOIN lb ON la.tag = lb.tag GROUP BY la.tag" "$(printf 'x      | 1\ny      | 1\nz      | 1')"
chksel "left_join_group_cntnote" "SELECT la.tag, COUNT(lb.note) FROM la LEFT JOIN lb ON la.tag = lb.tag GROUP BY la.tag" "$(printf 'x      | 1\ny      | 0\nz      | 1')"
# RIGHT / FULL / CROSS are recognized but unimplemented: fail closed with a clear
# error, never a silent inner join.
chk "right_join_closed" "$($QDB "$DB" "SELECT la.id FROM la RIGHT JOIN lb ON la.tag = lb.tag" 2>&1 | tr -d '\r' | grep -ci 'not supported')" "1"
chk "full_join_closed"  "$($QDB "$DB" "SELECT la.id FROM la FULL JOIN lb ON la.tag = lb.tag" 2>&1 | tr -d '\r' | grep -ci 'not supported')" "1"
chk "cross_join_closed" "$($QDB "$DB" "SELECT la.id FROM la CROSS JOIN lb ON la.tag = lb.tag" 2>&1 | tr -d '\r' | grep -ci 'not supported')" "1"
$QDB "$DB" "DROP TABLE la" >/dev/null 2>&1
$QDB "$DB" "DROP TABLE lb" >/dev/null 2>&1

echo "== ORDER BY inside JOIN =="
# The join executor emits in left-table scan order, then applies ORDER BY. la_o is
# inserted out of order (3,1,2) and carries an unmatched 'z' row for the LEFT-JOIN
# null-ordering case; lb_o carries a weight column w so a key can resolve against
# the RIGHT table too. The sort is stable, so ties keep scan order.
$QDB "$DB" "CREATE TABLE la_o (id INTEGER, tag TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO la_o VALUES (3,'x'),(1,'y'),(2,'x'),(4,'z')" >/dev/null
$QDB "$DB" "CREATE TABLE lb_o (tag TEXT, w INTEGER)" >/dev/null
$QDB "$DB" "INSERT INTO lb_o VALUES ('x',20),('y',10)" >/dev/null
# Left-table key, ascending then descending: reorders the scan-order rows.
chksel "join_order_asc"   "SELECT la_o.id FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY la_o.id" "$(printf '1\n2\n3')"
chksel "join_order_desc"  "SELECT la_o.id FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY la_o.id DESC" "$(printf '3\n2\n1')"
# Right-table key: w=10 (id1) sorts before the two w=20 rows, which tie and keep
# scan order (id3 before id2) -- proves right-side resolution and stability.
chksel "join_order_rightkey" "SELECT la_o.id FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY lb_o.w" "$(printf '1\n3\n2')"
# Secondary key breaks the w=20 tie by la_o.id ascending -> 2 before 3.
chksel "join_order_secondary" "SELECT la_o.id FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY lb_o.w, la_o.id" "$(printf '1\n2\n3')"
# LEFT JOIN: the unmatched 'z' row null-extends lb_o.w to NULL, which sorts first
# (NULL-first), then w=10 (id1), then the stable w=20 pair (id3, id2).
chksel "join_order_leftnull" "SELECT la_o.id FROM la_o LEFT JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY lb_o.w" "$(printf '4\n1\n3\n2')"
# ORDER BY a positional ordinal: 1 -> la_o.id (left, numeric), 2 -> lb_o.w (right,
# numeric). The ordinal maps back to its source column, so numeric ordering and
# the stable tie-break match the named-key path. An out-of-range or unknown key
# fails closed rather than silently returning scan order.
chksel "join_order_ordinal_left" "SELECT la_o.id, lb_o.w FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY 1 DESC" "$(printf '3  | 20\n2  | 20\n1  | 10')"
chksel "join_order_ordinal_sec"  "SELECT la_o.id, lb_o.w FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY 2, 1"    "$(printf '1  | 10\n2  | 20\n3  | 20')"
chk "join_order_ordinal_oor_closed" "$($QDB "$DB" "SELECT la_o.id FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY 9" 2>&1 | tr -d '\r' | grep -ci 'select position')" "1"
chk "join_order_unknown_closed"     "$($QDB "$DB" "SELECT la_o.id FROM la_o JOIN lb_o ON la_o.tag = lb_o.tag ORDER BY nope" 2>&1 | tr -d '\r' | grep -ci 'select position')" "1"
$QDB "$DB" "DROP TABLE la_o" >/dev/null 2>&1
$QDB "$DB" "DROP TABLE lb_o" >/dev/null 2>&1

echo "== UNION / UNION ALL =="
# Two tables that overlap on (3,carol): plain UNION dedups the shared row, UNION
# ALL keeps it. Branches are full SELECTs, so each may carry its own WHERE. A
# trailing ORDER BY / LIMIT binds to the merged set and must sit after the final
# branch. Aggregate, group, and join branches fail closed rather than merge a
# shape the union path does not evaluate.
$QDB "$DB" "CREATE TABLE ua (id INTEGER, name TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO ua VALUES (1,'alice'),(2,'bob'),(3,'carol')" >/dev/null
$QDB "$DB" "CREATE TABLE ub (id INTEGER, name TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO ub VALUES (3,'carol'),(4,'dave'),(5,'eve')" >/dev/null
# Plain UNION removes the duplicate id=3; UNION ALL preserves it.
chksel "union_dedup"       "SELECT id FROM ua UNION SELECT id FROM ub ORDER BY id" "$(printf '1\n2\n3\n4\n5')"
chksel "union_all_dups"    "SELECT id FROM ua UNION ALL SELECT id FROM ub ORDER BY id" "$(printf '1\n2\n3\n3\n4\n5')"
# Trailing ORDER BY on the merged set, descending, proves the sort is applied
# after the merge rather than per branch.
chksel "union_order_desc"  "SELECT id FROM ua UNION SELECT id FROM ub ORDER BY id DESC" "$(printf '5\n4\n3\n2\n1')"
# Text column dedups on the shared 'carol' the same way an integer column does.
chksel "union_text_dedup"  "SELECT name FROM ua UNION SELECT name FROM ub ORDER BY name" "$(printf 'alice\nbob\ncarol\ndave\neve')"
# Per-branch WHERE: id<3 from ua and id>4 from ub, two projected columns.
chksel "union_twocol_where" "SELECT id, name FROM ua WHERE id < 3 UNION SELECT id, name FROM ub WHERE id > 4" "$(printf '1 | alice\n2 | bob\n5 | eve')"
# Three-way chain still collapses to one deduped set.
chksel "union_three"       "SELECT id FROM ua UNION SELECT id FROM ub UNION SELECT id FROM ua ORDER BY id" "$(printf '1\n2\n3\n4\n5')"
# WHERE on both branches then dedup across the survivors.
chksel "union_where_both"  "SELECT id FROM ua WHERE id >= 2 UNION SELECT id FROM ub WHERE id <= 4 ORDER BY id" "$(printf '2\n3\n4')"
# Fail closed, never silently merge, on shapes the union path does not evaluate:
# mismatched column counts, an aggregate branch, an ORDER BY / LIMIT that is not
# after the final branch, and LIMIT / OFFSET on the union itself.
chk "union_colcount_closed" "$($QDB "$DB" "SELECT id FROM ua UNION SELECT id, name FROM ub" 2>&1 | tr -d '\r' | grep -ci 'different column counts')" "1"
chk "union_agg_closed"      "$($QDB "$DB" "SELECT COUNT(*) FROM ua UNION SELECT id FROM ub" 2>&1 | tr -d '\r' | grep -ci 'aggregate')" "1"
chk "union_nonfinal_closed" "$($QDB "$DB" "SELECT id FROM ua ORDER BY id UNION SELECT id FROM ub" 2>&1 | tr -d '\r' | grep -ci 'final UNION branch')" "1"
chk "union_limit_closed"    "$($QDB "$DB" "SELECT id FROM ua UNION SELECT id FROM ub LIMIT 2" 2>&1 | tr -d '\r' | grep -ci 'not supported on a UNION')" "1"
# ORDER BY a positional ordinal binds to an output column -- the canonical way to
# order a union, since only the first branch's names reach the merged headers. 1
# -> id, 2 -> name. An out-of-range or unknown key fails closed rather than
# silently returning merge order.
chksel "union_order_ordinal_desc" "SELECT id FROM ua UNION SELECT id FROM ub ORDER BY 1 DESC" "$(printf '5\n4\n3\n2\n1')"
chksel "union_order_ordinal_two"  "SELECT id, name FROM ua UNION SELECT id, name FROM ub ORDER BY 1" "$(printf '1 | alice\n2 | bob\n3 | carol\n4 | dave\n5 | eve')"
chksel "union_order_ordinal_name" "SELECT id, name FROM ua UNION ALL SELECT id, name FROM ub ORDER BY 2" "$(printf '1 | alice\n2 | bob\n3 | carol\n3 | carol\n4 | dave\n5 | eve')"
chk "union_order_ordinal_oor_closed" "$($QDB "$DB" "SELECT id FROM ua UNION SELECT id FROM ub ORDER BY 5" 2>&1 | tr -d '\r' | grep -ci 'select position')" "1"
chk "union_order_unknown_closed"     "$($QDB "$DB" "SELECT id FROM ua UNION SELECT id FROM ub ORDER BY nope" 2>&1 | tr -d '\r' | grep -ci 'select position')" "1"
$QDB "$DB" "DROP TABLE ua" >/dev/null 2>&1
$QDB "$DB" "DROP TABLE ub" >/dev/null 2>&1

echo "== trailing input fails closed (no silently dropped tokens) =="
# A finished parse must consume the whole statement. A leftover token is a clause
# qdb does not parse or plain garbage; running anyway would silently drop it -- a
# wrong answer for a query, a half-specified write for a mutation. Every statement
# kind fails closed with the store untouched. These are false-success controls for
# the end-of-input guard: build qdb without it and each SELECT below returns a
# plausible answer while each mutation applies its partial write, so every line
# here fails on the unpatched parser (the needle is never printed; the mutation
# cases also leave emp changed).
# --- query side: garbage after a clause is rejected, never dropped ---
reseed; errchk "trail_where"  "SELECT name FROM emp WHERE salary > 80 EXTRA"  "trailing input"
reseed; errchk "trail_order"  "SELECT name FROM emp ORDER BY salary WOOPS"    "trailing input"
reseed; errchk "trail_limit"  "SELECT name FROM emp LIMIT 2 tail"             "trailing input"
# A table name takes exactly one optional alias. The first bareword after emp is
# that alias; a second bareword is garbage and must be rejected -- this guards the
# alias slot from swallowing arbitrary trailing words.
reseed; errchk "trail_after_alias" "SELECT id FROM emp e2 junk ORDER BY id"   "trailing input"
# Companion positive: a single trailing bareword IS a legal alias, so this well
# formed query must still render rather than over-reject.
reseed; chksel "alias_single_ok"   "SELECT id FROM emp e2 ORDER BY id"        "$(printf '1\n2\n3\n4\n5')"
# --- mutation side: the partial write is refused before the WAL, table intact ---
reseed; errchk "trail_delete" "DELETE FROM emp WHERE id = 1 EXTRA"            "trailing input"
reseed; errchk "trail_insert" "INSERT INTO emp VALUES (9,'z','x',1,1) OOPS"   "trailing input"
reseed; errchk "trail_update" "UPDATE emp SET salary = 0 WHERE id = 1 OOPS"   "trailing input"
reseed; errchk "trail_create" "CREATE TABLE ct (id INTEGER) OOPS"             "trailing input"
# A trailing token after the final UNION branch fails closed too: exec_select_union
# re-enters the same end-of-input check per branch, so the final branch's tail is
# guarded there. Garbage right after the table name would bind as that branch's
# alias, so the control puts it after ORDER BY, which cannot take an alias.
rm -f "$DB" "$DB".wal 2>/dev/null
$QDB "$DB" "CREATE TABLE ua (id INTEGER, name TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO ua VALUES (1,'alice'),(2,'bob')" >/dev/null
$QDB "$DB" "CREATE TABLE ub (id INTEGER, name TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO ub VALUES (3,'carol')" >/dev/null
chk "trail_union_final" "$($QDB "$DB" "SELECT id FROM ua UNION SELECT id FROM ub ORDER BY 1 JUNK" 2>&1 | tr -d '\r' | grep -ci 'trailing input')" "1"
$QDB "$DB" "DROP TABLE ua" >/dev/null 2>&1
$QDB "$DB" "DROP TABLE ub" >/dev/null 2>&1

echo "== transactions across processes =="
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE t (id INTEGER)" >/dev/null
$QDB "$DB" "BEGIN" >/dev/null; $QDB "$DB" "INSERT INTO t VALUES (1)" >/dev/null; $QDB "$DB" "ROLLBACK" >/dev/null
chk "xproc_rollback" "$($QDB "$DB" "SELECT COUNT(*) FROM t" | tr -d '\r' | tail -n +3 | tr -d ' ')" "0"
$QDB "$DB" "BEGIN" >/dev/null; $QDB "$DB" "INSERT INTO t VALUES (2)" >/dev/null; $QDB "$DB" "COMMIT" >/dev/null
chk "xproc_commit" "$($QDB "$DB" "SELECT id FROM t" | tr -d '\r' | tail -n +3 | tr -d ' ')" "2"
# A transaction is visible to the process that resumes it before COMMIT.
$QDB "$DB" "BEGIN" >/dev/null; $QDB "$DB" "INSERT INTO t VALUES (3)" >/dev/null
chk "xproc_resumed_sees_own_rows" "$($QDB "$DB" "SELECT COUNT(*) FROM t" | tr -d '\r' | tail -n +3 | tr -d ' ')" "2"
# A process rejected with exit code 1 inside the transaction leaves no HOLD,
# so the open transaction is dropped, as a crash would drop it.
$QDB "$DB" "NOT_A_STATEMENT" >/dev/null 2>&1
chk "xproc_unclean_exit_drops" "$($QDB "$DB" "SELECT COUNT(*) FROM t" | tr -d '\r' | tail -n +3 | tr -d ' ')" "1"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null

echo "== text with separators and line breaks round-trips =="
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE s (id INTEGER, v TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO s VALUES (1, 'x|y'), (2, 'back\\slash'), (3, 'a\\|b')" >/dev/null
chksel "text_pipe"       "SELECT v FROM s WHERE id = 1" "x|y"
chksel "text_backslash"  "SELECT v FROM s WHERE id = 2" 'back\slash'
chksel "text_both"       "SELECT v FROM s WHERE id = 3" 'a\|b'
chk   "text_format_header" "$(head -1 "$DB" | tr -d '\r' | cut -d' ' -f1-2)" "QDB 2"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null

echo "== NULL and '' through the file format =="
# Format 2 writes NULL as \N and '' as an empty cell; both survive a checkpoint
# (the .db written at exit) and a WAL replay (a crash before the checkpoint).
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE z (id INTEGER, v TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO z VALUES (1, NULL), (2, ''), (3, '\\N')" >/dev/null
chksel "fmt2_null_ckpt"  "SELECT id FROM z WHERE v IS NULL" "1"
chksel "fmt2_empty_ckpt" "SELECT id FROM z WHERE v = ''" "2"
chksel "fmt2_backslash_n" "SELECT v FROM z WHERE id = 3" '\N'
chk   "fmt2_cells" "$(grep '^ROW' "$DB" | tr -d '\r' | tr '\n' ' ')" 'ROW 1|\N ROW 2| ROW 3|\\N '
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE z (id INTEGER, v TEXT)" >/dev/null
QDB_CRASH_AT=2 $QDB "$DB" "INSERT INTO z VALUES (1, NULL), (2, '')" >/dev/null 2>&1
chksel "fmt2_null_wal"   "SELECT id FROM z WHERE v IS NULL" "1"
chksel "fmt2_empty_wal"  "SELECT id FROM z WHERE v = ''" "2"
# A format 1 file had one empty cell for NULL and ''; it loads as NULL.
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
printf 'QDB 1 0\nTABLE z\nSCHEMA id:INTEGER v:TEXT\nROW 1|\nROW 2|b\nEND\n' > "$DB"
chksel "fmt1_empty_is_null" "SELECT id FROM z WHERE v IS NULL" "1"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null

echo "== column type arguments and literal-first comparisons (P9) =="
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE ta (a INTEGER, x VARCHAR(30), y DECIMAL(10, 2))" >/dev/null
$QDB "$DB" "INSERT INTO ta VALUES (1,'p',2.5),(5,'q',3),(9,'r',4)" >/dev/null
chksel "type_args"        "SELECT x FROM ta WHERE a = 5" "q"
chksel "lit_first_lt"     "SELECT a FROM ta WHERE 5 < a" "9"
chksel "lit_first_ge"     "SELECT a FROM ta WHERE 5 >= a ORDER BY a" "$(printf '1
5')"
chksel "lit_first_eq_and" "SELECT a FROM ta WHERE 'q' = x AND 1 < a" "5"
chk   "lit_both_sides"   "$($QDB "$DB" "SELECT a FROM ta WHERE 1 = 1" 2>&1 | tr -d '
' | grep -ci 'column on one side')" "1"
chk   "type_arg_not_num" "$($QDB "$DB" "CREATE TABLE tb (a VARCHAR(x))" 2>&1 | tr -d '
' | grep -ci 'expected a number')" "1"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null

echo "== INTERSECT and EXCEPT, left to right (P10) =="
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE cp (a INTEGER, s TEXT)" >/dev/null
$QDB "$DB" "CREATE TABLE cq (a INTEGER, s TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO cp VALUES (1,'x'),(2,'y'),(2,'y'),(3,'z'),(4,NULL)" >/dev/null
$QDB "$DB" "INSERT INTO cq VALUES (2,'y'),(3,'w'),(4,NULL),(5,'v')" >/dev/null
chksel "intersect"        "SELECT a FROM cp INTERSECT SELECT a FROM cq ORDER BY 1" "$(printf '2
3
4')"
chksel "except_null_eq"   "SELECT a, s FROM cp EXCEPT SELECT a, s FROM cq ORDER BY 1" "$(printf '1 | x
3 | z')"
chksel "union_then_except" "SELECT a FROM cp UNION SELECT a FROM cq EXCEPT SELECT a FROM cq WHERE a > 3 ORDER BY 1" "$(printf '1
2
3')"
chksel "union_then_all"   "SELECT a FROM cp WHERE a < 3 UNION SELECT a FROM cp WHERE a < 3 UNION ALL SELECT a FROM cq WHERE a = 2 ORDER BY 1" "$(printf '1
2
2')"
chk   "intersect_all_closed" "$($QDB "$DB" "SELECT a FROM cp INTERSECT ALL SELECT a FROM cq" 2>&1 | tr -d '
' | grep -ci 'not supported')" "1"
$QDB "$DB" "CREATE TABLE cr (u TEXT, v TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO cr VALUES ('a|b','c'),('a','b|c')" >/dev/null
chksel "union_key_pipe"   "SELECT COUNT(*) FROM cr" "2"
chk   "union_key_pipe_rows" "$(rows "SELECT u, v FROM cr UNION SELECT u, v FROM cr" | wc -l | tr -d ' ')" "2"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null

echo "== expressions in the select list and WHERE (P11) =="
# xrows: the rows with runs of spaces squeezed, for multi-column answers.
xrows() { rows "$1" | tr -s ' '; }
reseed
chk    "x_arith"        "$(xrows "SELECT id, salary * 2 + age FROM emp WHERE id = 2")" "2 | 225"
chk    "x_int_div"      "$(xrows "SELECT salary / 7, salary % 7 FROM emp WHERE id = 1")" "14 | 2"
chksel "x_real_div"     "SELECT salary / 8.0 FROM emp WHERE id = 1" "12.5"
chksel "x_div_zero"     "SELECT salary / 0 FROM emp WHERE id = 1" "NULL"
chksel "x_col_col"      "SELECT id FROM emp WHERE salary > age * 3 ORDER BY id" "1"
chksel "x_minus_lex"    "SELECT salary-age FROM emp WHERE id = 5" "42"
chksel "x_case"         "SELECT CASE WHEN salary >= 100 THEN 'hi' WHEN salary >= 80 THEN 'mid' ELSE 'lo' END FROM emp ORDER BY id" "$(printf 'hi\nmid\nmid\nhi\nlo')"
chksel "x_case_simple"  "SELECT CASE dept WHEN 'eng' THEN 1 WHEN 'hr' THEN 2 END FROM emp ORDER BY id" "$(printf '1\n1\nNULL\nNULL\n2')"
chk    "x_abs_coalesce" "$(xrows "SELECT abs(age - 40), coalesce(NULL, name) FROM emp WHERE id = 1")" "10 | alice"
chk    "x_corr_subq"    "$(xrows "SELECT id, (SELECT count(*) FROM emp AS x WHERE x.salary > emp.salary) FROM emp ORDER BY 2, 1")" "$(printf '4 | 0\n1 | 1\n2 | 2\n3 | 3\n5 | 4')"
chksel "x_exists"       "SELECT id FROM emp WHERE EXISTS (SELECT 1 FROM emp AS x WHERE x.age > emp.age + 10) ORDER BY id" "$(printf '1\n3\n5')"
chk    "x_agg_expr"     "$(xrows "SELECT count(*), sum(salary) / count(*), avg(age) FROM emp WHERE salary > age * 2")" "4 | 92 | 35.75"
chk    "x_order_keys"   "$(xrows "SELECT dept, age, id FROM emp ORDER BY 1, 2 DESC, 3")" "$(printf 'eng | 45 | 2\neng | 30 | 1\nhr | 28 | 5\nsales | 50 | 4\nsales | 35 | 3')"
chksel "x_between_expr" "SELECT id FROM emp WHERE salary BETWEEN age * 2 AND age * 3 ORDER BY id" "$(printf '2\n3\n4\n5')"
chksel "x_three_valued" "SELECT id FROM emp WHERE NOT (CASE WHEN id = 1 THEN NULL ELSE 1 END = 1) ORDER BY id" ""
chk    "x_no_function"  "$($QDB "$DB" "SELECT nosuch(id) FROM emp" 2>&1 | tr -d '\r' | grep -ci 'no such function')" "1"
chk    "x_no_column"    "$($QDB "$DB" "SELECT id + nosuch FROM emp" 2>&1 | tr -d '\r' | grep -ci 'no such column')" "1"

echo "== comma-separated FROM lists (P11, ordered by P12) =="
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE j1 (a INTEGER, b INTEGER)" >/dev/null
$QDB "$DB" "CREATE TABLE j2 (a INTEGER, b INTEGER)" >/dev/null
$QDB "$DB" "CREATE TABLE j3 (a INTEGER, b INTEGER)" >/dev/null
$QDB "$DB" "INSERT INTO j1 VALUES (1,10),(2,20),(3,30)" >/dev/null
$QDB "$DB" "INSERT INTO j2 VALUES (10,1),(20,3),(30,3)" >/dev/null
$QDB "$DB" "INSERT INTO j3 VALUES (1,100),(3,300)" >/dev/null
chk "comma_join3" "$(xrows "SELECT j1.a, j3.b FROM j1, j2, j3 WHERE j3.a = 3 AND j2.b = j3.a AND j1.b = j2.a ORDER BY 1")" "$(printf '2 | 300\n3 | 300')"
chk "comma_join_cross" "$(xrows "SELECT count(*) FROM j1, j3")" "6"
chk "comma_join_alias" "$(xrows "SELECT x.a, y.a FROM j1 AS x, j1 AS y WHERE x.a + 1 = y.a ORDER BY 1")" "$(printf '1 | 2\n2 | 3')"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null

echo "== PRIMARY KEY, UNIQUE and NOT NULL (P13) =="
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
$QDB "$DB" "CREATE TABLE ku (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL, a INTEGER, b INTEGER, UNIQUE (a, b))" >/dev/null
$QDB "$DB" "INSERT INTO ku VALUES (1,'x',1,1),(NULL,'y',1,2)" >/dev/null
chksel "ipk_autonumber"  "SELECT id FROM ku WHERE name = 'y'" "2"
errk() { chk "$1" "$($QDB "$DB" "$2" 2>&1 | tr -d '\r' | grep -ci "$3")" "1"; }
errk "pk_dup"            "INSERT INTO ku VALUES (1,'z',3,3)"          "UNIQUE constraint failed: ku.id"
errk "unique_dup"        "INSERT INTO ku VALUES (5,'x',3,3)"          "UNIQUE constraint failed: ku.name"
errk "not_null"          "INSERT INTO ku VALUES (5,NULL,3,3)"         "NOT NULL constraint failed: ku.name"
errk "unique_pair"       "INSERT INTO ku VALUES (5,'w',1,1)"          "UNIQUE constraint failed: ku.a, ku.b"
errk "dup_in_batch"      "INSERT INTO ku VALUES (7,'p',7,7),(8,'p',8,8)" "UNIQUE constraint failed: ku.name"
errk "update_dup"        "UPDATE ku SET name = 'x' WHERE id = 2"      "UNIQUE constraint failed: ku.name"
chksel "rejected_atomic" "SELECT count(*) FROM ku" "2"
chksel "update_kept"     "SELECT name FROM ku WHERE id = 2" "y"
$QDB "$DB" "INSERT INTO ku VALUES (5,'w',NULL,1),(6,'v',NULL,1)" >/dev/null
chksel "unique_nulls_distinct" "SELECT count(*) FROM ku" "4"
chk   "con_in_file"      "$(grep -c '^UNIQUE a,b\|^PRIMARY KEY id\|^NOT NULL name' "$DB")" "3"
errk "con_after_reload"  "INSERT INTO ku VALUES (9,'w',9,9)"          "UNIQUE constraint failed: ku.name"
$QDB "$DB" "BEGIN" >/dev/null; $QDB "$DB" "CREATE TABLE kv (k INTEGER PRIMARY KEY)" >/dev/null; $QDB "$DB" "ROLLBACK" >/dev/null
$QDB "$DB" "CREATE TABLE kv (k INTEGER)" >/dev/null
$QDB "$DB" "INSERT INTO kv VALUES (1),(1)" >/dev/null
chksel "rollback_drops_con" "SELECT count(*) FROM kv" "2"
# P17: the key map in one process. A DELETE and an UPDATE must free their keys,
# and 1.0 must collide with 1 in an INTEGER PRIMARY KEY column.
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null
km=$(printf "CREATE TABLE km (id INTEGER PRIMARY KEY, name TEXT UNIQUE)\nINSERT INTO km VALUES (1,'a')\nINSERT INTO km VALUES (2,'b')\nDELETE FROM km WHERE id = 1\nINSERT INTO km VALUES (1,'a')\nINSERT INTO km VALUES (1.0,'c')\nUPDATE km SET name = 'z' WHERE id = 2\nINSERT INTO km VALUES (3,'b')\nSELECT count(*) FROM km\n.quit\n" | $QDB "$DB" | tr -d '\r')
chk "map_numeric_key"    "$(echo "$km" | grep -c 'UNIQUE constraint failed: km.id')" "1"
chk "map_freed_keys"     "$(echo "$km" | grep -c 'constraint failed')" "1"
chk "map_count"          "$(echo "$km" | grep -c '^ *3 *$')" "1"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null

echo "== INSERT ... SELECT (P22) =="
reseed
$QDB "$DB" "CREATE TABLE cp (id INTEGER PRIMARY KEY, name TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO cp SELECT id, name FROM emp WHERE dept = 'eng'" >/dev/null
chksel "insel_rows"      "SELECT id, name FROM cp ORDER BY id" "$(printf '1 | alice
2 | bob')"
$QDB "$DB" "INSERT INTO cp (name, id) SELECT name, id + 10 FROM emp WHERE id > 3 ORDER BY id" >/dev/null
chksel "insel_col_list"  "SELECT id, name FROM cp WHERE id > 10 ORDER BY id" "$(printf '14 | dave
15 | eve')"
$QDB "$DB" "INSERT INTO cp SELECT id, name FROM emp WHERE id > 99" >/dev/null
chksel "insel_zero_rows" "SELECT count(*) FROM cp" "4"
errk "insel_constraint"  "INSERT INTO cp SELECT 1, name FROM emp WHERE id = 3" "UNIQUE constraint failed: cp.id"
$QDB "$DB" "INSERT INTO cp SELECT id + 50, name FROM cp" >/dev/null
chksel "insel_self"      "SELECT count(*) FROM cp" "8"
errk "insel_batch_dup"   "INSERT INTO cp SELECT 400 + id / 2, name FROM emp WHERE id < 4" "UNIQUE constraint failed: cp.id"
chksel "insel_atomic"    "SELECT count(*) FROM cp" "8"
errk "insel_width"       "INSERT INTO cp SELECT id FROM emp" "has 2 columns but 1 values"
rm -f "$DB" "$DB".wal "$DB".ckpt 2>/dev/null


echo "== ALTER / DROP =="
reseed; $QDB "$DB" "ALTER TABLE emp ADD COLUMN active INTEGER" >/dev/null
chkhdr "alter_add"     "SELECT * FROM emp LIMIT 0" "id | name | dept | salary | age | active"
$QDB "$DB" "DROP TABLE dep" >/dev/null 2>&1
chk   "drop"           "$($QDB "$DB" 'SELECT * FROM dep' 2>&1 | tr -d '\r' | grep -ci 'not found')" "1"

echo "== documented honest nulls (XFAIL: divergence is by design) =="
reseed
# join_order is no longer a null: the join executor now applies ORDER BY after
# assembling result rows (left key asc/desc, right key, secondary key, NULL-first
# under LEFT JOIN). It moved up to chksel assertions in the "ORDER BY inside JOIN"
# section.
# paren_group and not_between are no longer nulls: the WHERE parser now builds a
# boolean-expression tree (OR<AND<NOT<primary, with '(' opening a subexpression),
# so parenthesized grouping and NOT BETWEEN evaluate exactly. Both moved up to
# chksel assertions in the WHERE section.
# group_ord_agg is no longer a null: parse_order_by now captures COUNT(*)/SUM(col)
# as an order key and the grouped-sort path matches it against the aggregate
# output column and compares numerically. It moved up to chksel assertions in the
# aggregates section (group_ord_agg, group_ord_cnt_desc, group_ord_sum_desc).
# Cross-process transactions are no longer a null: a clean exit inside BEGIN
# appends HOLD to the WAL and the next process resumes the transaction, so a
# later ROLLBACK undoes an earlier INSERT and a later COMMIT keeps it. They
# moved up to the "transactions across processes" section.
rm -f "$DB" "$DB".wal 2>/dev/null
echo
echo "QDB: pass=$pass fail=$fail xfail=$xfail xpass=$xpass"
if [ "$fail" -ne 0 ]; then exit 1; fi
exit 0
