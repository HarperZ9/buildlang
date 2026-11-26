#!/usr/bin/env bash
# The port tool's differential check across all three frontends:
#   1. C subset (`port c`): byte-for-byte stdout equality with `gcc -O1 -lm` + refusals.
#   2. Python subset (`port py`): byte-for-byte stdout equality with `python3` / `python` + refusals.
#   3. Controlled Plain-English (`port plain`): byte-for-byte stdout equality with `.want` + refusals.
# Planted controls verify that comparing two different programs' outputs fails the check.
#
#   port/check.sh path/to/buildc
set -u
BUILDC="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

PYTHON_BIN=""
if command -v python3 >/dev/null 2>&1; then
  PYTHON_BIN="python3"
elif command -v python >/dev/null 2>&1; then
  PYTHON_BIN="python"
else
  echo "port: python3/python not found on PATH"; exit 1
fi

cp "$HERE"/*.bld "$WORK"/
(cd "$WORK" && "$BUILDC" build --release . > build.log 2>&1) || { cat "$WORK/build.log"; echo "port: the tool did not build"; exit 1; }
PORT="$WORK/target/release/main"
if [ -x "$WORK/target/release/main.exe" ]; then PORT="$WORK/target/release/main.exe"; fi

# --- Frontend 1: C subset vs gcc ---
c_same=0; c_total=0
for c in "$HERE"/corpus/c/*.c; do
  n="$(basename "$c" .c)"; c_total=$((c_total + 1))
  gcc -O1 -o "$WORK/c-$n" "$c" -lm && "$WORK/c-$n" | tr -d '\r' > "$WORK/c-$n.want"
  if ! "$PORT" c "$c" "$WORK/c-$n.bld" > "$WORK/c-$n.port.log" 2>&1; then
    echo "DIFF  c/$n: the tool refused a program in the corpus"; cat "$WORK/c-$n.port.log"; fail=1; continue
  fi
  "$BUILDC" run "$WORK/c-$n.bld" 2> "$WORK/c-$n.err" | tr -d '\r' > "$WORK/c-$n.got"
  if [ -s "$WORK/c-$n.want" ] && cmp -s "$WORK/c-$n.want" "$WORK/c-$n.got"; then
    c_same=$((c_same + 1)); echo "same  c/$n ($(wc -l < "$WORK/c-$n.want" | tr -d ' ') lines)"
  else
    echo "DIFF  c/$n"; diff "$WORK/c-$n.want" "$WORK/c-$n.got" | head -10; head -5 "$WORK/c-$n.err"; fail=1
  fi
done

c_ref=0; c_rtotal=0
for c in "$HERE"/corpus/c/refused/*.c; do
  n="$(basename "$c" .c)"; c_rtotal=$((c_rtotal + 1))
  "$PORT" c "$c" "$WORK/rc-$n.bld" > "$WORK/rc-$n.log" 2>&1; rc=$?
  got="$(grep -o '^  line [0-9]*' "$WORK/rc-$n.log" | tr -dc '0-9\n' | tr '\n' ' ' | sed 's/ $//')"
  want="$(tr -d '\r' < "${c%.c}.lines" | tr '\n' ' ' | sed 's/ $//')"
  if [ "$rc" = 2 ] && [ ! -e "$WORK/rc-$n.bld" ] && [ "$got" = "$want" ]; then
    c_ref=$((c_ref + 1)); echo "refused  c/$n at lines $got"
  else
    echo "WRONG  c/$n: exit $rc, lines [$got], wanted exit 2 and lines [$want]"; fail=1
  fi
done

# --- Frontend 2: Python subset vs CPython ---
py_same=0; py_total=0
for p in "$HERE"/corpus/py/*.py; do
  n="$(basename "$p" .py)"; py_total=$((py_total + 1))
  "$PYTHON_BIN" "$p" | tr -d '\r' > "$WORK/py-$n.want"
  if ! "$PORT" py "$p" "$WORK/py-$n.bld" > "$WORK/py-$n.port.log" 2>&1; then
    echo "DIFF  py/$n: the tool refused a program in the corpus"; cat "$WORK/py-$n.port.log"; fail=1; continue
  fi
  "$BUILDC" run "$WORK/py-$n.bld" 2> "$WORK/py-$n.err" | tr -d '\r' > "$WORK/py-$n.got"
  if [ -s "$WORK/py-$n.want" ] && cmp -s "$WORK/py-$n.want" "$WORK/py-$n.got"; then
    py_same=$((py_same + 1)); echo "same  py/$n ($(wc -l < "$WORK/py-$n.want" | tr -d ' ') lines)"
  else
    echo "DIFF  py/$n"; diff "$WORK/py-$n.want" "$WORK/py-$n.got" | head -10; head -5 "$WORK/py-$n.err"; fail=1
  fi
done

py_ref=0; py_rtotal=0
for p in "$HERE"/corpus/py/refused/*.py; do
  n="$(basename "$p" .py)"; py_rtotal=$((py_rtotal + 1))
  "$PORT" py "$p" "$WORK/rpy-$n.bld" > "$WORK/rpy-$n.log" 2>&1; rc=$?
  got="$(grep -o '^  line [0-9]*' "$WORK/rpy-$n.log" | tr -dc '0-9\n' | tr '\n' ' ' | sed 's/ $//')"
  want="$(tr -d '\r' < "${p%.py}.lines" | tr '\n' ' ' | sed 's/ $//')"
  if [ "$rc" = 2 ] && [ ! -e "$WORK/rpy-$n.bld" ] && [ "$got" = "$want" ]; then
    py_ref=$((py_ref + 1)); echo "refused  py/$n at lines $got"
  else
    echo "WRONG  py/$n: exit $rc, lines [$got], wanted exit 2 and lines [$want]"; cat "$WORK/rpy-$n.log"; fail=1
  fi
done

# --- Frontend 3: Controlled Plain-English vs golden .want ---
pl_same=0; pl_total=0
for p in "$HERE"/corpus/plain/*.plain; do
  n="$(basename "$p" .plain)"; pl_total=$((pl_total + 1))
  tr -d '\r' < "${p%.plain}.want" > "$WORK/pl-$n.want"
  if ! "$PORT" plain "$p" "$WORK/pl-$n.bld" > "$WORK/pl-$n.port.log" 2>&1; then
    echo "DIFF  plain/$n: the tool refused a program in the corpus"; cat "$WORK/pl-$n.port.log"; fail=1; continue
  fi
  if ! grep -q '^// MIRROR: Active safety traps:' "$WORK/pl-$n.bld"; then
    echo "DIFF  plain/$n: missing Plain-English MIRROR summary header"; fail=1; continue
  fi
  "$BUILDC" run "$WORK/pl-$n.bld" 2> "$WORK/pl-$n.err" | tr -d '\r' > "$WORK/pl-$n.got"
  if [ -s "$WORK/pl-$n.want" ] && cmp -s "$WORK/pl-$n.want" "$WORK/pl-$n.got"; then
    pl_same=$((pl_same + 1)); echo "same  plain/$n ($(wc -l < "$WORK/pl-$n.want" | tr -d ' ') lines)"
  else
    echo "DIFF  plain/$n"; diff "$WORK/pl-$n.want" "$WORK/pl-$n.got" | head -10; head -5 "$WORK/pl-$n.err"; fail=1
  fi
done

pl_ref=0; pl_rtotal=0
for p in "$HERE"/corpus/plain/refused/*.plain; do
  n="$(basename "$p" .plain)"; pl_rtotal=$((pl_rtotal + 1))
  "$PORT" plain "$p" "$WORK/rpl-$n.bld" > "$WORK/rpl-$n.log" 2>&1; rc=$?
  got="$(grep -o '^  line [0-9]*' "$WORK/rpl-$n.log" | tr -dc '0-9\n' | tr '\n' ' ' | sed 's/ $//')"
  want="$(tr -d '\r' < "${p%.plain}.lines" | tr '\n' ' ' | sed 's/ $//')"
  if [ "$rc" = 2 ] && [ ! -e "$WORK/rpl-$n.bld" ] && [ "$got" = "$want" ]; then
    pl_ref=$((pl_ref + 1)); echo "refused  plain/$n at lines $got"
  else
    echo "WRONG  plain/$n: exit $rc, lines [$got], wanted exit 2 and lines [$want]"; cat "$WORK/rpl-$n.log"; fail=1
  fi
done

# --- Planted mismatch controls (one per frontend) ---
ctrl_ok=0
if cmp -s "$WORK/c-primes.got" "$WORK/c-numeric.want"; then echo "control c: two different programs compared equal"; fail=1; else ctrl_ok=$((ctrl_ok + 1)); fi
if cmp -s "$WORK/py-primes.got" "$WORK/py-numeric.want"; then echo "control py: two different programs compared equal"; fail=1; else ctrl_ok=$((ctrl_ok + 1)); fi
if cmp -s "$WORK/pl-arithmetic.got" "$WORK/pl-kinematics_budget.want"; then echo "control plain: two different programs compared equal"; fail=1; else ctrl_ok=$((ctrl_ok + 1)); fi
echo "controls: planted mismatch checks passed: $ctrl_ok of 3"
echo "C: $c_same/$c_total identical to gcc, $c_ref/$c_rtotal refused; Python: $py_same/$py_total identical to python, $py_ref/$py_rtotal refused; Plain-English: $pl_same/$pl_total identical to golden, $pl_ref/$pl_rtotal refused"
exit $fail
