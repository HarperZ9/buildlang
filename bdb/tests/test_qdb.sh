#!/usr/bin/env bash
# qdb regression harness -- the full SQL surface of programs/db.bld in one run.
#
# Build qdb.exe first (see programs/build.bat), then: bash programs/tests/test_qdb.sh
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
  echo "SKIP: $QDB not found -- build it first (programs/build.bat), then rerun."
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
# NULL in INSERT stores a blank cell; IS NULL matches it, IS NOT NULL matches the rest.
$QDB "$DB" "CREATE TABLE nn (id INTEGER, name TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO nn VALUES (1,'alice'),(2,NULL),(3,'carol')" >/dev/null
chksel "is_null"       "SELECT id FROM nn WHERE name IS NULL" "2"
chksel "is_not_null"   "SELECT id FROM nn WHERE name IS NOT NULL ORDER BY id" "$(printf '1\n3')"
$QDB "$DB" "DROP TABLE nn" >/dev/null 2>&1

echo "== ORDER BY (multi-key) / LIMIT / OFFSET / DISTINCT =="
chksel "order_desc"    "SELECT name FROM emp ORDER BY salary DESC" "$(printf 'dave\nalice\nbob\ncarol\neve')"
chksel "order_2key"    "SELECT name FROM emp ORDER BY dept, salary DESC" "$(printf 'alice\nbob\neve\ndave\ncarol')"
chksel "limit"         "SELECT name FROM emp ORDER BY id LIMIT 2" "$(printf 'alice\nbob')"
chksel "limit_offset"  "SELECT name FROM emp ORDER BY id LIMIT 2 OFFSET 1" "$(printf 'bob\ncarol')"
chksel "distinct"      "SELECT DISTINCT dept FROM emp ORDER BY dept" "$(printf 'eng\nhr\nsales')"

echo "== aggregates / GROUP BY / HAVING =="
chksel "count_star"    "SELECT COUNT(*) FROM emp" "5"
chksel "sum"           "SELECT SUM(salary) FROM emp" "460"
chksel "min_max"       "SELECT MIN(salary) FROM emp" "70"
chkhdr "hdr_count"     "SELECT COUNT(*) FROM emp" "COUNT(*)"
chksel "group_count"   "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY dept" "$(printf 'eng   | 2\nhr    | 1\nsales | 2')"
chksel "group_desc"    "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY dept DESC" "$(printf 'sales | 2\nhr    | 1\neng   | 2')"
chksel "group_sum_ord" "SELECT dept, SUM(salary) FROM emp GROUP BY dept ORDER BY dept" "$(printf 'eng   | 190\nhr    | 70\nsales | 200')"
chksel "having"        "SELECT dept FROM emp GROUP BY dept HAVING COUNT(*) > 1 ORDER BY dept" "$(printf 'eng\nsales')"
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
# Fail closed: an ORDER BY term that names neither the grouped column, an
# aggregate output, nor a select alias has no defined ordering -- error rather
# than emit rows in arbitrary discovery order.
reseed; errchk "group_ord_unresolved_closed" "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY age" "grouped column, aggregate, or alias"

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

echo "== LEFT [OUTER] JOIN (null-extend unmatched left rows) =="
# Dedicated pair: la carries three tags, lb covers only x and z, so the 'y' row
# has no match. INNER drops it; LEFT keeps it with a NULL (blank) right column.
# These assertions select without ORDER BY, so rows render in left-table scan
# order; la is inserted 1,2,3 so that order is already sorted. ORDER BY inside a
# join is covered in the "ORDER BY inside JOIN" section below.
$QDB "$DB" "CREATE TABLE la (id INTEGER, tag TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO la VALUES (1,'x'),(2,'y'),(3,'z')" >/dev/null
$QDB "$DB" "CREATE TABLE lb (tag TEXT, note TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO lb VALUES ('x','has-x'),('z','has-z')" >/dev/null
chksel "join_inner_drop"   "SELECT la.id, lb.note FROM la JOIN lb ON la.tag = lb.tag" "$(printf '1  | has-x\n3  | has-z')"
chksel "left_join_null"    "SELECT la.id, lb.note FROM la LEFT JOIN lb ON la.tag = lb.tag" "$(printf '1  | has-x\n2  |\n3  | has-z')"
chksel "left_outer_join"   "SELECT la.id, lb.note FROM la LEFT OUTER JOIN lb ON la.tag = lb.tag" "$(printf '1  | has-x\n2  |\n3  | has-z')"
# WHERE on the null-extended right column: IS NULL selects exactly the unmatched
# left row; IS NOT NULL and equality keep matched rows only (null excluded).
chksel "left_join_isnull"  "SELECT la.id FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE lb.note IS NULL" "2"
chksel "left_join_notnull" "SELECT la.id FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE lb.note IS NOT NULL" "$(printf '1\n3')"
chksel "left_join_reqeq"   "SELECT la.id FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE lb.note = 'has-x'" "1"
# WHERE on the preserved (left) column filters left rows, null-extension intact.
chksel "left_join_leftwh"  "SELECT la.id, lb.note FROM la LEFT JOIN lb ON la.tag = lb.tag WHERE la.id > 1" "$(printf '2  |\n3  | has-z')"
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
$QDB "$DB" "DROP TABLE ua" >/dev/null 2>&1
$QDB "$DB" "DROP TABLE ub" >/dev/null 2>&1

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
# Cross-process transactions: each statement is its own process and
# auto-commit checkpoints, so a later ROLLBACK cannot undo an earlier
# INSERT. SQL-correct would drop the row; here it persists (count 6).
rm -f "$DB" "$DB".wal 2>/dev/null
$QDB "$DB" "CREATE TABLE t (id INTEGER)" >/dev/null
$QDB "$DB" "BEGIN" >/dev/null; $QDB "$DB" "INSERT INTO t VALUES (1)" >/dev/null; $QDB "$DB" "ROLLBACK" >/dev/null
xfail "xproc_rollback" "$($QDB "$DB" "SELECT COUNT(*) FROM t" | tr -d '\r' | tail -n +3 | tr -d ' ')" "0"

rm -f "$DB" "$DB".wal 2>/dev/null
echo
echo "QDB: pass=$pass fail=$fail xfail=$xfail xpass=$xpass"
if [ "$fail" -ne 0 ]; then exit 1; fi
exit 0
