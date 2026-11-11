#!/usr/bin/env bash
# Differential fuzz of cg-rv64: for every seed of a range, fuzzgen writes
# a BuildLang program; the program is run on the host and through
# cg-rv64, asm-rv64 and emu-rv64 with arguments 0, 1 and 2, and the two
# outcomes are compared by the rule of tests/tools/diff.sh.
#
#   fuzz.sh [-v V] [-x] FROM TO
#
# -v V  use planted defect V of the generator (0 is the generator)
# -x    stop at the first difference
#
# A seed whose program the hosted compiler or the generator does not
# accept is counted and skipped; the counts are printed, and a refusal by
# the generator names the construct in WORK/fuzz/SEED.refused.
#
# Environment: BUILDC, CG, BUILDLANG_STDLIB, WORK, MAXSTEPS, as diff.sh.
# Run from the package folder.
set -u
V=0
STOP=0
while [ "${1:-}" = "-v" ] || [ "${1:-}" = "-x" ]; do
  if [ "$1" = "-v" ]; then V=$2; shift 2; else STOP=1; shift; fi
done
FROM=$1
TO=$2
MAX=${MAXSTEPS:-200000000}
F=$WORK/fuzz
mkdir -p "$F"
if [ ! -f "$F/fuzzgen.exe" ]; then
  cat tests/tools/fuzzgen.bld tests/tools/fuzzgen2.bld > "$F/fuzzgen.bld"
  "$BUILDC" -o "$F/fuzzgen.exe" --stdio-mode portable-lf "$F/fuzzgen.bld" > "$F/fuzzgen.build" 2>&1 \
    || { echo "fuzzgen build failed"; exit 2; }
fi
total=0; bad=0; nohost=0; refused=0; traps=0; seeds=0
seed=$FROM
while [ "$seed" -le "$TO" ]; do
  p=$F/$seed
  if [ ! -f "$p.done" ]; then
    "$F/fuzzgen.exe" "$seed" > "$p.bld"
    if "$BUILDC" -o "$p.exe" --stdio-mode portable-lf "$p.bld" > "$p.build" 2>&1 \
       && "$BUILDC" mir emit "$p.bld" -o "$p.mir.json" > /dev/null 2>&1; then
      for k in 0 1 2; do "$p.exe" $k > "$p.$k.host" 2>&1; echo $? > "$p.$k.host.rc"; done
      echo ok > "$p.done"
    else
      echo nohost > "$p.done"
    fi
  fi
  if [ "$(cat "$p.done")" = nohost ]; then
    nohost=$((nohost + 1))
  else
    seeds=$((seeds + 1))
    for k in 0 1 2; do
      e=$p.$k.v$V.emu
      "$CG" runv "$V" "$p.mir.json" run "$MAX" "$k" > "$e" 2> "$e.err"
      rc=$?
      if [ "$rc" = 1 ] && grep -q '^refused: ' "$e"; then
        refused=$((refused + 1)); cp "$e" "$p.refused"; break
      fi
      total=$((total + 1))
      [ "$(cat "$p.$k.host.rc")" = 101 ] && traps=$((traps + 1))
      if [ "$rc" != "$(cat "$p.$k.host.rc")" ] || ! cmp -s "$p.$k.host" "$e"; then
        bad=$((bad + 1))
        echo "DIFFERENT: seed $seed argument $k: host status $(cat "$p.$k.host.rc"), emulator status $rc, comparison $total"
        if [ "$STOP" = 1 ]; then echo "result: variant $V refused after $total comparisons, at seed $seed"; exit 1; fi
      fi
    done
  fi
  seed=$((seed + 1))
done
echo "result: variant $V, seeds $FROM to $TO: $seeds programs, $total comparisons, $bad different, $traps ended in a trap, $nohost not accepted by the hosted compiler, $refused refused by the generator"
[ "$bad" = 0 ]
