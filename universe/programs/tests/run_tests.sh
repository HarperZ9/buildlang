#!/usr/bin/env bash
# BuildLang Program Test Suite
# Tests all 60 compiled programs in the BUILD-UNIVERSE/programs/ directory.
# Run: bash programs/tests/run_tests.sh

set -o pipefail

PASS=0
FAIL=0
SKIP=0
PROGRAMS_DIR="$(cd "$(dirname "$0")/.." && pwd)"

pass() { echo "  PASS: $1"; PASS=$((PASS+1)); }
fail() { echo "  FAIL: $1 -- expected: $2, got: $3"; FAIL=$((FAIL+1)); }
skip() { echo "  SKIP: $1 -- $2"; SKIP=$((SKIP+1)); }

# Helper: run program, check output contains expected string
check_output() {
    local name="$1" cmd="$2" expected="$3"
    local actual
    actual=$(eval "$cmd" 2>&1)
    if echo "$actual" | grep -qF -- "$expected"; then
        pass "$name"
    else
        fail "$name" "$expected" "$(echo "$actual" | head -1)"
    fi
}

# Helper: run program, check exit code
check_exit() {
    local name="$1" cmd="$2" expected_exit="$3"
    eval "$cmd" >/dev/null 2>&1
    local actual_exit=$?
    if [ "$actual_exit" -eq "$expected_exit" ]; then
        pass "$name"
    else
        fail "$name" "exit $expected_exit" "exit $actual_exit"
    fi
}

# Helper: run program, check output exactly equals expected (trimmed)
check_exact() {
    local name="$1" cmd="$2" expected="$3"
    local actual
    actual=$(eval "$cmd" 2>&1 | tr -d '\r')
    if [ "$actual" = "$expected" ]; then
        pass "$name"
    else
        fail "$name" "$expected" "$(echo "$actual" | head -1)"
    fi
}

echo "=== BuildLang Program Test Suite ==="
echo "Programs dir: $PROGRAMS_DIR"
echo ""

# =========================================================================
# 1. qwc — word/line/byte counter
# =========================================================================
echo "qwc:"
check_output "count words+lines+bytes" "echo 'hello world' | $PROGRAMS_DIR/qwc.exe" "1 2 12"
check_exact  "count lines only"        "printf 'a\nb\nc\n' | $PROGRAMS_DIR/qwc.exe -l" "3"
check_output "help flag"               "$PROGRAMS_DIR/qwc.exe --help" "Usage:"
check_exact  "word count only"         "echo 'one two three' | $PROGRAMS_DIR/qwc.exe -w" "3"
check_exact  "byte count only"         "echo 'hi' | $PROGRAMS_DIR/qwc.exe -c" "3"

# =========================================================================
# 2. qgrep — pattern matcher
# =========================================================================
echo "qgrep:"
check_output "match found"       "printf 'hello\nworld\nhello again\n' | $PROGRAMS_DIR/qgrep.exe hello" "hello"
check_exact  "line count"        "printf 'a\nb\na\n' | $PROGRAMS_DIR/qgrep.exe -c a" "2"
check_exit   "no match exits 1"  "echo 'hello' | $PROGRAMS_DIR/qgrep.exe xyz" 1
check_output "case insensitive"  "echo 'Hello' | $PROGRAMS_DIR/qgrep.exe -i hello" "Hello"

# =========================================================================
# 3. qjq — JSON processor
# =========================================================================
echo "qjq:"
check_output "field access"  "echo '{\"name\":\"Zain\"}' | $PROGRAMS_DIR/qjq.exe '.name'" '"Zain"'
check_output "array length"  "echo '[1,2,3]' | $PROGRAMS_DIR/qjq.exe 'length'" "3"
check_output "keys"          "echo '{\"a\":1,\"b\":2}' | $PROGRAMS_DIR/qjq.exe 'keys'" '"a"'
check_output "pipe"          "echo '{\"a\":{\"b\":42}}' | $PROGRAMS_DIR/qjq.exe '.a | .b'" "42"

# =========================================================================
# 4. qcsv — CSV to JSON converter
# =========================================================================
echo "qcsv:"
check_output "basic csv"     "printf 'name,age\nZain,25\n' | $PROGRAMS_DIR/qcsv.exe" "Zain"
check_output "numeric detect" "printf 'n,v\na,42\n' | $PROGRAMS_DIR/qcsv.exe" "42"

# =========================================================================
# 5. qsort — line sorter
# =========================================================================
echo "qsort:"
check_output "alphabetical"  "printf 'c\na\nb\n' | $PROGRAMS_DIR/qsort.exe" "a"
check_output "reverse first"  "printf 'a\nb\nc\n' | $PROGRAMS_DIR/qsort.exe -r | head -1" "c"
check_exact  "unique"        "printf 'a\na\nb\n' | $PROGRAMS_DIR/qsort.exe -u" "$(printf 'a\nb')"

# =========================================================================
# 6. quniq — duplicate filter
# =========================================================================
echo "quniq:"
check_exact  "dedup"  "printf 'a\na\nb\nb\nc\n' | $PROGRAMS_DIR/quniq.exe" "$(printf 'a\nb\nc')"
check_output "count"  "printf 'a\na\nb\n' | $PROGRAMS_DIR/quniq.exe -c" "2 a"

# =========================================================================
# 7. qcalc — expression evaluator
# =========================================================================
echo "qcalc:"
check_exact  "addition"     "$PROGRAMS_DIR/qcalc.exe '2 + 3'" "5"
check_exact  "precedence"   "$PROGRAMS_DIR/qcalc.exe '2 + 3 * 4'" "14"
check_exact  "parens"       "$PROGRAMS_DIR/qcalc.exe '(2 + 3) * 4'" "20"
check_exact  "sqrt"         "$PROGRAMS_DIR/qcalc.exe 'sqrt(144)'" "12"
check_output "div by zero"  "$PROGRAMS_DIR/qcalc.exe '1/0'" "Division by zero"

# =========================================================================
# 8. qbase64 — base64 encoder/decoder
# =========================================================================
echo "qbase64:"
check_exact  "encode"  "echo -n 'hello' | $PROGRAMS_DIR/qbase64.exe -e" "aGVsbG8="
check_exact  "decode"  "echo -n 'aGVsbG8=' | $PROGRAMS_DIR/qbase64.exe -d" "hello"

# =========================================================================
# 9. qdiff — file differ
# =========================================================================
echo "qdiff:"
check_exit   "identical files" "printf 'a\n' > /tmp/qd1.txt; cp /tmp/qd1.txt /tmp/qd2.txt; $PROGRAMS_DIR/qdiff.exe /tmp/qd1.txt /tmp/qd2.txt" 0
check_output "different files" "printf 'old\n' > /tmp/qd1.txt; printf 'new\n' > /tmp/qd2.txt; $PROGRAMS_DIR/qdiff.exe /tmp/qd1.txt /tmp/qd2.txt" "-old"

# =========================================================================
# 10. qsed — stream editor
# =========================================================================
echo "qsed:"
check_exact  "substitute"  "echo 'hello world' | $PROGRAMS_DIR/qsed.exe 's/world/earth/'" "hello earth"
check_output "delete line"  "printf 'a\nb\nc\n' | $PROGRAMS_DIR/qsed.exe '2d'" "a"

# =========================================================================
# 11. qmd — markdown to HTML
# =========================================================================
echo "qmd:"
check_output "heading"  "echo '# Hello' | $PROGRAMS_DIR/qmd.exe -f" "<h1>Hello</h1>"
check_output "bold"     "echo '**bold**' | $PROGRAMS_DIR/qmd.exe -f" "<strong>bold</strong>"
check_output "list"     "echo '- item' | $PROGRAMS_DIR/qmd.exe -f" "<li>item</li>"

# =========================================================================
# 12. qdb — SQLite-like database
# =========================================================================
echo "qdb: moved to HarperZ9/buildlang bdb/ (programs/QDB.md); its tests run there"

# =========================================================================
# 13. qtr — character translator
# =========================================================================
echo "qtr:"
check_exact  "uppercase"  "echo 'hello' | $PROGRAMS_DIR/qtr.exe 'a-z' 'A-Z'" "HELLO"
check_exact  "delete"     "echo 'h3ll0' | $PROGRAMS_DIR/qtr.exe -d '0-9'" "hll"

# =========================================================================
# 14. qcut — column cutter
# =========================================================================
echo "qcut:"
check_exact  "field 2"     "echo 'a,b,c' | $PROGRAMS_DIR/qcut.exe -d, -f2" "b"
check_exact  "fields 1,3"  "echo 'a,b,c' | $PROGRAMS_DIR/qcut.exe -d, -f1,3" "a,c"

# =========================================================================
# 15. qhex — hexdump
# =========================================================================
echo "qhex:"
check_output "hex output"  "echo -n 'AB' | $PROGRAMS_DIR/qhex.exe" "4142"

# =========================================================================
# 16. qfind — file finder
# =========================================================================
echo "qfind:"
check_exit   "help"            "$PROGRAMS_DIR/qfind.exe --help" 0
check_output "find build"     "$PROGRAMS_DIR/qfind.exe $PROGRAMS_DIR -name '*.bld' -type f" ".bld"

# =========================================================================
# 17. qloc — lines of code counter
# =========================================================================
echo "qloc:"
check_output "count loc"  "$PROGRAMS_DIR/qloc.exe $PROGRAMS_DIR" "BuildLang"

# =========================================================================
# 18. qfmt — code formatter
# =========================================================================
echo "qfmt:"
check_exit   "check formatted"  "$PROGRAMS_DIR/qfmt.exe --check $PROGRAMS_DIR/wc.bld" 0

# =========================================================================
# 19. qlint — code linter
# =========================================================================
echo "qlint:"
check_output "lint wc"  "$PROGRAMS_DIR/qlint.exe $PROGRAMS_DIR/wc.bld 2>&1" "warning"

# =========================================================================
# 20. qcmp — byte comparator
# =========================================================================
echo "qcmp:"
check_exit   "identical"  "printf 'x' > /tmp/qc1; cp /tmp/qc1 /tmp/qc2; $PROGRAMS_DIR/qcmp.exe /tmp/qc1 /tmp/qc2" 0
check_exit   "different"  "printf 'x' > /tmp/qc1; printf 'y' > /tmp/qc2; $PROGRAMS_DIR/qcmp.exe /tmp/qc1 /tmp/qc2" 1

# =========================================================================
# 21. qseq — sequence generator
# =========================================================================
echo "qseq:"
check_output "1 to 3"       "$PROGRAMS_DIR/qseq.exe 1 3" "1"
check_exact  "full sequence" "$PROGRAMS_DIR/qseq.exe 1 3" "$(printf '1\n2\n3')"

# =========================================================================
# 22. qdate — date printer
# =========================================================================
echo "qdate:"
check_output "has year"  "$PROGRAMS_DIR/qdate.exe" "202"

# =========================================================================
# 23. qecho — echo
# =========================================================================
echo "qecho:"
check_exact  "basic"          "$PROGRAMS_DIR/qecho.exe hello world" "hello world"
check_exact  "single arg"     "$PROGRAMS_DIR/qecho.exe test" "test"

# =========================================================================
# 24. qtest — conditional evaluator
# =========================================================================
echo "qtest:"
check_exit  "file exists"   "$PROGRAMS_DIR/qtest.exe -f $PROGRAMS_DIR/wc.bld" 0
check_exit  "file missing"  "$PROGRAMS_DIR/qtest.exe -f /nonexistent" 1
check_exit  "eq true"       "$PROGRAMS_DIR/qtest.exe 5 -eq 5" 0
check_exit  "gt true"       "$PROGRAMS_DIR/qtest.exe 5 -gt 3" 0
check_exit  "gt false"      "$PROGRAMS_DIR/qtest.exe 3 -gt 5" 1

# =========================================================================
# 25. qbasename — path basename
# =========================================================================
echo "qbasename:"
check_exact  "basic"      "$PROGRAMS_DIR/qbasename.exe /foo/bar/baz.txt" "baz.txt"

# =========================================================================
# 26. qdirname — path dirname
# =========================================================================
echo "qdirname:"
check_output "basic"  "$PROGRAMS_DIR/qdirname.exe /foo/bar/baz.txt" "bar"

# =========================================================================
# 27. qrev — line reverser
# =========================================================================
echo "qrev:"
check_exact  "reverse"  "echo 'hello' | $PROGRAMS_DIR/qrev.exe" "olleh"

# =========================================================================
# 28. qtac — reverse line order
# =========================================================================
echo "qtac:"
check_output "reverse order"  "printf 'a\nb\nc\n' | $PROGRAMS_DIR/qtac.exe | head -1" "c"

# =========================================================================
# 29. qnl — number lines
# =========================================================================
echo "qnl:"
check_output "numbered"  "printf 'a\nb\n' | $PROGRAMS_DIR/qnl.exe" "1"
check_output "has tab"   "printf 'a\nb\n' | $PROGRAMS_DIR/qnl.exe" "a"

# =========================================================================
# 30. qprintf — formatted printing
# =========================================================================
echo "qprintf:"
check_output "format string"  "$PROGRAMS_DIR/qprintf.exe '%s has %d items' hello 5" "hello has 5 items"

# =========================================================================
# 31. qexpand — tabs to spaces
# =========================================================================
echo "qexpand:"
check_output "expand tabs"  "printf 'a\tb\n' | $PROGRAMS_DIR/qexpand.exe" "a"

# =========================================================================
# 32. qunexpand — spaces to tabs
# =========================================================================
echo "qunexpand:"
check_exit  "runs"  "printf '    a\n' | $PROGRAMS_DIR/qunexpand.exe" 0

# =========================================================================
# 33. qfold — line wrapper
# =========================================================================
echo "qfold:"
check_output "wrap lines"  "echo 'hello world this is a long line' | $PROGRAMS_DIR/qfold.exe -w 15" "hello world thi"

# =========================================================================
# 34. qcomm — common line finder
# =========================================================================
echo "qcomm:"
check_output "common lines"  "printf 'a\nb\n' > /tmp/qcomm1.txt; printf 'b\nc\n' > /tmp/qcomm2.txt; $PROGRAMS_DIR/qcomm.exe /tmp/qcomm1.txt /tmp/qcomm2.txt" "b"

# =========================================================================
# 35. qjoin — file joiner
# =========================================================================
echo "qjoin:"
check_output "join files"  "printf '1 a\n2 b\n' > /tmp/qj1.txt; printf '1 x\n3 y\n' > /tmp/qj2.txt; $PROGRAMS_DIR/qjoin.exe /tmp/qj1.txt /tmp/qj2.txt" "1 a x"

# =========================================================================
# 36. qpaste — column merger
# =========================================================================
echo "qpaste:"
check_output "merge cols"  "printf 'a\nb\n' > /tmp/qp1.txt; printf '1\n2\n' > /tmp/qp2.txt; $PROGRAMS_DIR/qpaste.exe /tmp/qp1.txt /tmp/qp2.txt" "a"

# =========================================================================
# 37. qenv — environment printer
# =========================================================================
echo "qenv:"
check_output "has PATH"  "$PROGRAMS_DIR/qenv.exe" "PATH="

# =========================================================================
# 38. qawk — text processor
# =========================================================================
echo "qawk:"
check_exact  "field 1"      "echo 'hello world' | $PROGRAMS_DIR/qawk.exe '{print \$1}'" "hello"
check_exact  "field 2"      "echo 'hello world' | $PROGRAMS_DIR/qawk.exe '{print \$2}'" "world"

# =========================================================================
# 39. qkv — key-value store
# =========================================================================
echo "qkv:"
rm -f /tmp/test_kv.db
check_exit   "set key"    "$PROGRAMS_DIR/qkv.exe --db /tmp/test_kv.db set name Zain" 0
check_exact  "get key"    "$PROGRAMS_DIR/qkv.exe --db /tmp/test_kv.db get name" "Zain"
check_exact  "count"      "$PROGRAMS_DIR/qkv.exe --db /tmp/test_kv.db count" "1"
check_exit   "del key"    "$PROGRAMS_DIR/qkv.exe --db /tmp/test_kv.db del name" 0
check_exact  "count zero" "$PROGRAMS_DIR/qkv.exe --db /tmp/test_kv.db count" "0"
rm -f /tmp/test_kv.db

# =========================================================================
# 40. qsum — file checksum
# =========================================================================
echo "qsum:"
check_output "checksum"  "printf 'x' > /tmp/qsum_test; $PROGRAMS_DIR/qsum.exe /tmp/qsum_test" "qsum_test"

# =========================================================================
# 41. qtok — tokenizer (self-hosting step 1)
# =========================================================================
echo "qtok:"
check_output "tokenize let"    "echo 'let x = 42;' | $PROGRAMS_DIR/qtok.exe" "KEYWORD"
check_output "tokenize ident"  "echo 'let x = 42;' | $PROGRAMS_DIR/qtok.exe" "IDENT"
check_output "tokenize int"    "echo 'let x = 42;' | $PROGRAMS_DIR/qtok.exe" "INT_LIT"

# =========================================================================
# 42. qparse — parser (self-hosting step 2)
# =========================================================================
echo "qparse:"
check_output "parse fn"   "echo 'fn main() { let x = 42; }' | $PROGRAMS_DIR/qparse.exe" "fn main"
check_output "parse let"  "echo 'fn main() { let x = 42; }' | $PROGRAMS_DIR/qparse.exe" "let x"

# =========================================================================
# 43. qcheck — type checker (self-hosting step 3)
# =========================================================================
echo "qcheck:"
check_output "check wc"  "$PROGRAMS_DIR/qcheck.exe $PROGRAMS_DIR/wc.bld 2>&1" "warning"

# =========================================================================
# 44. qcodegen — C code generator (self-hosting step 4)
# =========================================================================
echo "qcodegen:"
if [ -f "$PROGRAMS_DIR/qcodegen.exe" ]; then
    check_output "help"  "$PROGRAMS_DIR/qcodegen.exe --help" "Usage:"
else
    skip "help" "qcodegen.exe not linked (needs manual MSVC)"
fi

# =========================================================================
# 45. qyes — infinite yes
# =========================================================================
echo "qyes:"
check_exact  "default y"      "$PROGRAMS_DIR/qyes.exe | head -3" "$(printf 'y\ny\ny')"
check_exact  "custom string"  "$PROGRAMS_DIR/qyes.exe hello | head -2" "$(printf 'hello\nhello')"

# =========================================================================
# 46. qsql — SQL parser
# =========================================================================
echo "qsql:"
check_output "parse CREATE"  "$PROGRAMS_DIR/qsql.exe 'CREATE TABLE users (name TEXT, age INTEGER)'" "Parsed CREATE TABLE"
check_output "parse SELECT"  "$PROGRAMS_DIR/qsql.exe 'SELECT name FROM users WHERE age > 25'" "Parsed SELECT"
check_output "parse INSERT"  "$PROGRAMS_DIR/qsql.exe \"INSERT INTO users VALUES ('Zain', 25)\"" "Parsed INSERT"

# =========================================================================
# 47. qstrings — printable string extractor
# =========================================================================
echo "qstrings:"
check_output "help"  "$PROGRAMS_DIR/qstrings.exe --help" "extract printable"

# =========================================================================
# 48. qtee — tee splitter
# =========================================================================
echo "qtee:"
rm -f /tmp/qtee_out.txt
check_output "tee output"  "echo 'hello' | $PROGRAMS_DIR/qtee.exe /tmp/qtee_out.txt" "hello"
rm -f /tmp/qtee_out.txt

# =========================================================================
# 49. qxargs — argument builder
# =========================================================================
echo "qxargs:"
check_output "basic xargs"  "printf 'a\nb\nc\n' | $PROGRAMS_DIR/qxargs.exe echo" "a b c"

# =========================================================================
# 50-55. Interactive / long-running programs (skip)
# =========================================================================
echo ""
echo "--- Skipped (interactive/long-running) ---"
skip "qhttp"    "HTTP server — needs port binding"
skip "qht"      "HTTP tool — needs network"
skip "qwatch"   "file watcher — long-running"
skip "qsleep"   "sleep — blocking"
skip "qwhich"   "which — needs PATH lookup, system-dependent"
skip "qmake"    "make — needs Makefile context"
skip "qpatch"   "patch — needs diff context files"
skip "qjson"    "json — duplicate of qjq, stdin-blocking"

echo ""
echo "=== RESULTS ==="
echo "PASS: $PASS"
echo "FAIL: $FAIL"
echo "SKIP: $SKIP"
echo "TOTAL: $((PASS+FAIL+SKIP))"

# Cleanup temp files
rm -f /tmp/qd1.txt /tmp/qd2.txt /tmp/qc1 /tmp/qc2
rm -f /tmp/qcomm1.txt /tmp/qcomm2.txt /tmp/qj1.txt /tmp/qj2.txt
rm -f /tmp/qp1.txt /tmp/qp2.txt /tmp/qsum_test

if [ "$FAIL" -eq 0 ]; then
    echo "ALL TESTS PASSED"
    exit 0
else
    echo "$FAIL FAILURES -- fix before committing"
    exit 1
fi
