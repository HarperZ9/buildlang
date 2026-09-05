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

echo "== JOIN =="
reseed
$QDB "$DB" "CREATE TABLE dep (dname TEXT, loc TEXT)" >/dev/null
$QDB "$DB" "INSERT INTO dep VALUES ('eng','b1'),('sales','b2'),('hr','b3')" >/dev/null
chksel "join"          "SELECT emp.name FROM emp JOIN dep ON emp.dept = dep.dname WHERE dep.loc='b1' ORDER BY emp.name" "$(printf 'alice\nbob')"

echo "== ALTER / DROP =="
reseed; $QDB "$DB" "ALTER TABLE emp ADD COLUMN active INTEGER" >/dev/null
chkhdr "alter_add"     "SELECT * FROM emp LIMIT 0" "id | name | dept | salary | age | active"
$QDB "$DB" "DROP TABLE dep" >/dev/null 2>&1
chk   "drop"           "$($QDB "$DB" 'SELECT * FROM dep' 2>&1 | tr -d '\r' | grep -ci 'not found')" "1"

echo "== documented honest nulls (XFAIL: divergence is by design) =="
reseed
# Parenthesized WHERE grouping: the flat DNF parser cannot express
# (a OR b) AND c. SQL-correct here is alice,bob; the engine returns empty.
xfail "paren_group" "$(rows "SELECT name FROM emp WHERE (dept='eng' OR dept='hr') AND salary > 85")" "$(printf 'alice\nbob')"
# NOT BETWEEN needs an OR across groups the flat parser cannot emit.
# SQL-correct is eve(70); the engine returns empty.
xfail "not_between" "$(rows "SELECT name FROM emp WHERE salary NOT BETWEEN 80 AND 120")" "eve"
# ORDER BY an aggregate term on a grouped query: parse_order_by cannot capture
# COUNT(*)/SUM(col) as an order key, so grouped ORDER BY resolves only a plain
# output column. SQL-correct ascending by count is hr(1),eng(2),sales(2); the
# engine leaves the rows in group-discovery order (eng,sales,hr).
xfail "group_ord_agg" "$(rows "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY COUNT(*)")" "$(printf 'hr    | 1\neng   | 2\nsales | 2')"
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
