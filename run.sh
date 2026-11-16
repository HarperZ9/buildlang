#!/usr/bin/env bash
# Exploration only: split all.txt into one program a probe, build each with the given buildc,
# and class the result as PASS, REJECTED (check refuses), BUILD-FAIL (check passes, build fails),
# CRASH or WRONG (runs, prints something else).
BUILDC="$1"; OUT="$2"; HERE="$(cd "$(dirname "$0")" && pwd)"
rm -rf "$OUT"; mkdir -p "$OUT"
awk -v out="$OUT" '/^=== /{name=$2; sub(/^=== [^ ]+ :: /,""); dir=out "/" name; system("mkdir -p " dir); print > (dir "/expect.txt"); close(dir "/expect.txt"); f=dir "/main.bld"; next} {print > f}' "$HERE/all.txt"
for d in "$OUT"/*/; do
  n=$(basename "$d"); want=$(tr -d '\r' < "$d/expect.txt")
  if ! "$BUILDC" check "$d/main.bld" > "$d/check.log" 2>&1; then
    echo "REJECTED   $n :: $(grep -v '^\s*$' "$d/check.log" | grep -viE '^\[|^help|^note|^  ' | head -2 | tr '\n' ' ' | cut -c1-150)"; continue; fi
  if ! (cd "$d" && "$BUILDC" build --release . > build.log 2>&1) || [ ! -f "$d/target/release/main.exe" ]; then
    echo "BUILD-FAIL $n :: $(grep -iE 'error' "$d/build.log" | head -2 | tr '\n' ' ' | cut -c1-150)"; continue; fi
  got=$(cd "$d" && timeout 10 ./target/release/main.exe 2>&1 | tr -d '\r' | tr '\n' ' ' | sed 's/ *$//'); rc=$?
  if [ "$got" = "$want" ]; then echo "PASS       $n"
  elif [ -z "$got" ]; then echo "CRASH      $n :: no output"
  else echo "WRONG      $n :: want [$want] got [$(echo "$got" | cut -c1-80)]"; fi
done
