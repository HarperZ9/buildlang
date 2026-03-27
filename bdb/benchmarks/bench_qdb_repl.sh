#!/usr/bin/env bash
# In-process qdb benchmark — all queries in one REPL session
# Compares against the per-process bench_qdb.sh to show startup overhead savings.
set -e

QDB="$(dirname "$0")/../qdb.exe"
if [ ! -f "$QDB" ]; then
    QDB="$(dirname "$0")/../qdb"
fi

if [ ! -f "$QDB" ]; then
    echo "ERROR: qdb binary not found at $QDB"
    exit 1
fi

DB="/tmp/bench_qdb_repl_$$.db"
trap "rm -f $DB /tmp/bench_script_$$.sql" EXIT

echo "=== qdb In-Process Benchmark ==="
echo "All queries run in a single REPL session (no per-query startup)."
echo "Date: $(date)"
echo ""

# Generate the SQL script
cat > /tmp/bench_script_$$.sql << 'SQLEOF'
CREATE TABLE users (id INTEGER, name TEXT, age INTEGER, city TEXT);
SQLEOF

# Add 1000 INSERT statements
for i in $(seq 1 1000); do
    echo "INSERT INTO users VALUES ($i, 'user_$i', $((20 + i % 50)), 'city_$((i % 10))');" >> /tmp/bench_script_$$.sql
done

# Add query benchmarks
cat >> /tmp/bench_script_$$.sql << 'SQLEOF'
SELECT COUNT(*) FROM users;
SELECT * FROM users WHERE age > 60 LIMIT 10;
SELECT * FROM users WHERE city = 'city_5' LIMIT 10;
SELECT * FROM users ORDER BY age DESC LIMIT 10;
.quit
SQLEOF

STMT_COUNT=$(wc -l < /tmp/bench_script_$$.sql)
echo "Piping $STMT_COUNT statements through stdin to single qdb session..."
echo ""

# Time the entire session
START=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))" 2>/dev/null || python -c "import time; print(int(time.time()*1e9))")

cat /tmp/bench_script_$$.sql | $QDB $DB >/dev/null 2>&1

END=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))" 2>/dev/null || python -c "import time; print(int(time.time()*1e9))")
MS=$(( (END - START) / 1000000 ))

echo "Total time for 1,000 INSERTs + 4 queries: ${MS}ms"
if [ "$STMT_COUNT" -gt 0 ]; then
    echo "Average per-statement: $((MS / STMT_COUNT))ms"
fi
echo ""

# Verify data
ROW_COUNT=$($QDB $DB "SELECT COUNT(*) FROM users" 2>/dev/null | grep -oE '[0-9]+' | head -1)
echo "Rows in table: $ROW_COUNT"
echo ""

# Run per-process comparison: 4 individual SELECT queries
echo "--- Per-process comparison (4 individual queries) ---"
START2=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))" 2>/dev/null || python -c "import time; print(int(time.time()*1e9))")

$QDB $DB "SELECT COUNT(*) FROM users" >/dev/null 2>&1
$QDB $DB "SELECT * FROM users WHERE age > 60 LIMIT 10" >/dev/null 2>&1
$QDB $DB "SELECT * FROM users WHERE city = 'city_5' LIMIT 10" >/dev/null 2>&1
$QDB $DB "SELECT * FROM users ORDER BY age DESC LIMIT 10" >/dev/null 2>&1

END2=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))" 2>/dev/null || python -c "import time; print(int(time.time()*1e9))")
MS2=$(( (END2 - START2) / 1000000 ))

echo "4 per-process queries: ${MS2}ms (avg $((MS2 / 4))ms/query)"
echo ""
echo "=== Benchmark Complete ==="
