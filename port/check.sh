#!/usr/bin/env bash
# The port tool's check. For every C program in corpus/c:
#   1. gcc builds and runs it; that output is the reference;
#   2. `port c` writes BuildLang, buildc builds and runs it;
#   3. the two outputs must be equal byte for byte.
# For every program in corpus/c/refused, `port` must exit 2, write nothing and name
# exactly the lines listed in the program's .lines file.
# Control: one ported program's output is compared with another program's reference
# and must differ, so the comparison itself can fail.
#
#   port/check.sh path/to/buildc
set -u
BUILDC="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

cp "$HERE"/*.bld "$WORK"/
(cd "$WORK" && "$BUILDC" build --release . > build.log 2>&1) || { cat "$WORK/build.log"; echo "port: the tool did not build"; exit 1; }
PORT="$WORK/target/release/main"

same=0; total=0
for c in "$HERE"/corpus/c/*.c; do
  n="$(basename "$c" .c)"; total=$((total + 1))
  gcc -O1 -o "$WORK/$n-c" "$c" -lm && "$WORK/$n-c" | tr -d '\r' > "$WORK/$n.want"
  if ! "$PORT" c "$c" "$WORK/$n.bld" > "$WORK/$n.port.log" 2>&1; then
    echo "DIFF  $n: the tool refused a program in the corpus"; cat "$WORK/$n.port.log"; fail=1; continue
  fi
  "$BUILDC" run "$WORK/$n.bld" 2> "$WORK/$n.err" | tr -d '\r' > "$WORK/$n.got"
  if [ -s "$WORK/$n.want" ] && cmp -s "$WORK/$n.want" "$WORK/$n.got"; then
    same=$((same + 1)); echo "same  $n ($(wc -l < "$WORK/$n.want" | tr -d ' ') lines)"
  else
    echo "DIFF  $n"; diff "$WORK/$n.want" "$WORK/$n.got" | head -10; head -5 "$WORK/$n.err"; fail=1
  fi
done

refused=0; rtotal=0
for c in "$HERE"/corpus/c/refused/*.c; do
  n="$(basename "$c" .c)"; rtotal=$((rtotal + 1))
  "$PORT" c "$c" "$WORK/r-$n.bld" > "$WORK/r-$n.log" 2>&1; rc=$?
  got="$(grep -o '^  line [0-9]*' "$WORK/r-$n.log" | tr -dc '0-9\n' | tr '\n' ' ' | sed 's/ $//')"
  want="$(tr -d '\r' < "${c%.c}.lines" | tr '\n' ' ' | sed 's/ $//')"
  if [ "$rc" = 2 ] && [ ! -e "$WORK/r-$n.bld" ] && [ "$got" = "$want" ]; then
    refused=$((refused + 1)); echo "refused  $n at lines $got"
  else
    echo "WRONG  $n: exit $rc, lines [$got], wanted exit 2 and lines [$want]"; fail=1
  fi
done

if cmp -s "$WORK/primes.got" "$WORK/numeric.want"; then echo "control: two different programs compared equal"; fail=1; else echo "control: a different program's reference differs: 1 of 1"; fi
echo "ported and identical to gcc: $same of $total; refused as recorded: $refused of $rtotal"
exit $fail
