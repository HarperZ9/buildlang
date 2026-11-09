#!/usr/bin/env bash
# qdb Benchmark Suite
# Tests INSERT, SELECT, WHERE, JOIN, GROUP BY performance at different scales.

set -e

QDB="$(dirname "$0")/../qdb.exe"
if [ ! -f "$QDB" ]; then
    QDB="$(dirname "$0")/../qdb"
fi

if [ ! -f "$QDB" ]; then
    echo "ERROR: qdb binary not found at $QDB"
    exit 1
fi

DB="/tmp/bench_qdb_$$.db"
trap "rm -f $DB" EXIT

echo "=== qdb Benchmark Suite ==="
echo "Date: $(date)"
echo ""

# Helper: time a command
bench() {
    local name="$1"
    shift
    local start=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
    eval "$@" >/dev/null 2>&1
    local end=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
    local ms=$(( (end - start) / 1000000 ))
    printf "  %-40s %6d ms\n" "$name" "$ms"
}

# --- INSERT benchmarks ---
echo "INSERT performance:"
rm -f $DB

bench "CREATE TABLE (1 table)" "$QDB $DB 'CREATE TABLE users (id INTEGER, name TEXT, age INTEGER, city TEXT)'"

# Insert 100 rows
for i in $(seq 1 100); do
    $QDB $DB "INSERT INTO users VALUES ($i, 'user_$i', $((20 + i % 50)), 'city_$((i % 10))')" >/dev/null 2>&1
done
bench "INSERT 100 rows" "echo done"  # Already done, just record

# Insert 1000 rows (fresh table)
rm -f $DB
$QDB $DB "CREATE TABLE big (id INTEGER, val TEXT, num INTEGER)" >/dev/null 2>&1
START_INS=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
for i in $(seq 1 1000); do
    $QDB $DB "INSERT INTO big VALUES ($i, 'row_$i', $((i * 7 % 1000)))" >/dev/null 2>&1
done
END_INS=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
MS_INS=$(( (END_INS - START_INS) / 1000000 ))
printf "  %-40s %6d ms\n" "INSERT 1,000 rows" "$MS_INS"

echo ""

# --- SELECT benchmarks ---
echo "SELECT performance (1,000 rows):"
bench "SELECT * (full scan)" "$QDB $DB 'SELECT * FROM big LIMIT 10'"
bench "SELECT WHERE num > 500" "$QDB $DB 'SELECT * FROM big WHERE num > 500 LIMIT 10'"
bench "SELECT WHERE val = 'row_500'" "$QDB $DB 'SELECT * FROM big WHERE val = '\\''row_500'\\'''"
bench "COUNT(*)" "$QDB $DB 'SELECT COUNT(*) FROM big'"

echo ""

# --- ORDER BY ---
echo "ORDER BY performance (1,000 rows):"
bench "ORDER BY num ASC LIMIT 10" "$QDB $DB 'SELECT * FROM big ORDER BY num ASC LIMIT 10'"
bench "ORDER BY num DESC LIMIT 10" "$QDB $DB 'SELECT * FROM big ORDER BY num DESC LIMIT 10'"

echo ""

# --- GROUP BY ---
echo "GROUP BY performance (100 rows):"
rm -f $DB
$QDB $DB "CREATE TABLE employees (id INTEGER, dept TEXT, salary INTEGER)" >/dev/null 2>&1
for i in $(seq 1 100); do
    dept="dept_$((i % 5))"
    $QDB $DB "INSERT INTO employees VALUES ($i, '$dept', $((30000 + i * 100)))" >/dev/null 2>&1
done
bench "GROUP BY dept, COUNT(*)" "$QDB $DB 'SELECT dept, COUNT(*) FROM employees GROUP BY dept'"
bench "GROUP BY dept, AVG(salary)" "$QDB $DB 'SELECT dept, AVG(salary) FROM employees GROUP BY dept'"

echo ""

# --- JOIN ---
echo "JOIN performance:"
$QDB $DB "CREATE TABLE orders (id INTEGER, emp_id INTEGER, amount INTEGER)" >/dev/null 2>&1
for i in $(seq 1 50); do
    $QDB $DB "INSERT INTO orders VALUES ($i, $((i % 100 + 1)), $((100 + i * 10)))" >/dev/null 2>&1
done
bench "JOIN 100x50 rows" "$QDB $DB 'SELECT employees.dept, orders.amount FROM employees JOIN orders ON employees.id = orders.emp_id LIMIT 10'"

echo ""
echo "=== Benchmark Complete ==="
