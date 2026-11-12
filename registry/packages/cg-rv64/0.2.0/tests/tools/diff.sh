#!/usr/bin/env bash
# Differential check of cg-rv64: every case of a list is run on the host
# (buildc with the C backend) and through cg-rv64, asm-rv64 and emu-rv64,
# and the two outcomes are compared.
#
#   diff.sh [-v V] [-x] LIST DIR
#
# LIST holds lines `PROGRAM ENTRY FROM TO`: the program DIR/PROGRAM.bld is
# run once for every k from FROM to TO, on the host as `PROGRAM k` and on
# the emulator with entry function ENTRY and argument k.
# -v V  use planted defect V of the generator (0 is the generator)
# -x    stop at the first difference
#
# Rule of equality: the bytes the emulated program sent to the serial
# device equal the standard output of the hosted program followed by its
# standard error, and the halt code equals the hosted exit status. A
# hosted program writes both streams unbuffered, so one pipe holds them
# in the order they were written.
#
# Environment: BUILDC (the compiler), CG (the cg-rv64 program),
# BUILDLANG_STDLIB, WORK (a folder for files; host results are kept there
# and reused), MAXSTEPS (default 200000000).
set -u
V=0
STOP=0
while [ "${1:-}" = "-v" ] || [ "${1:-}" = "-x" ]; do
  if [ "$1" = "-v" ]; then V=$2; shift 2; else STOP=1; shift; fi
done
LIST=$1
DIR=$2
MAX=${MAXSTEPS:-200000000}
mkdir -p "$WORK"
total=0
bad=0
while read -r prog entry from to; do
  case "$prog" in ''|'#'*) continue ;; esac
  src=$DIR/$prog.bld
  if [ ! -f "$WORK/$prog.exe" ]; then
    "$BUILDC" -o "$WORK/$prog.exe" --stdio-mode portable-lf "$src" > "$WORK/$prog.build" 2>&1 \
      || { echo "host build failed: $prog"; exit 2; }
    "$BUILDC" mir emit "$src" -o "$WORK/$prog.mir.json" > /dev/null || { echo "mir emit failed: $prog"; exit 2; }
  fi
  k=$from
  while [ "$k" -le "$to" ]; do
    h=$WORK/$prog.$k.host
    if [ ! -f "$h.rc" ]; then
      "$WORK/$prog.exe" "$k" > "$h" 2>&1
      echo $? > "$h.rc"
    fi
    e=$WORK/$prog.$k.v$V.emu
    "$CG" runv "$V" "$WORK/$prog.mir.json" "$entry" "$MAX" "$k" > "$e" 2> "$e.err"
    rc=$?
    total=$((total + 1))
    if [ "$rc" != "$(cat "$h.rc")" ] || ! cmp -s "$h" "$e"; then
      bad=$((bad + 1))
      echo "DIFFERENT: $prog $entry $k: host status $(cat "$h.rc"), emulator status $rc, comparison $total"
      if [ "$STOP" = 1 ]; then echo "result: variant $V refused after $total comparisons"; exit 1; fi
    fi
    k=$((k + 1))
  done
done < "$LIST"
echo "result: variant $V, $total comparisons, $bad different"
[ "$bad" = 0 ]
