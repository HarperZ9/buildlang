#!/usr/bin/env bash
# Runs `buildc check` on every .bld file under universe/ and compares the result with
# universe/CHECK-FAILS.txt, the list of files the compiler does not accept yet.
#
#   bash universe/check.sh <path to buildc>           compare with the list, exit 1 on a difference
#   bash universe/check.sh <path to buildc> --write   rewrite the list from this run
#
# A file fails at stage `lex` (no token stream), `parse` (the parser reports errors) or
# `types` (it parses and the type and effect check refuses it). The run fails when a file
# fails and is not listed, when a listed file passes, or when a listed file fails at another
# stage. A fix to the compiler or to a module therefore has to remove the line.

set -u
export LC_ALL=C

if [ $# -lt 1 ]; then
  echo "usage: bash universe/check.sh <buildc> [--write]" >&2
  exit 2
fi
BUILDC=$1
MODE=${2:-compare}
case "$BUILDC" in
  /*|[A-Za-z]:*) ;;
  *) BUILDC="$PWD/$BUILDC" ;;
esac
HERE=$(cd "$(dirname "$0")" && pwd)
LIST="$HERE/CHECK-FAILS.txt"
cd "$HERE" || exit 2

if ! "$BUILDC" --version >/dev/null 2>&1; then
  echo "cannot run $BUILDC" >&2
  exit 2
fi

observed=$(mktemp)
total=0
passed=0
for f in $(find . -name '*.bld' -type f | sed 's#^\./##' | sort); do
  total=$((total + 1))
  out=$(timeout 120 "$BUILDC" check "$f" 2>&1)
  rc=$?
  if [ $rc -eq 0 ]; then
    passed=$((passed + 1))
    continue
  fi
  if [ $rc -eq 124 ]; then
    stage=timeout
  elif printf '%s\n' "$out" | grep -q '^Parsing\.\.\. OK'; then
    stage=types
  elif printf '%s\n' "$out" | grep -q '^Lexing\.\.\. OK'; then
    stage=parse
  else
    stage=lex
  fi
  echo "$f $stage" >> "$observed"
done
failed=$((total - passed))

echo "$("$BUILDC" --version): $total files, $passed pass, $failed fail"
echo "  lex: $(grep -c ' lex$' "$observed")  parse: $(grep -c ' parse$' "$observed")  types: $(grep -c ' types$' "$observed")  timeout: $(grep -c ' timeout$' "$observed")"

if [ "$total" -lt 200 ]; then
  echo "FAIL: found $total .bld files, expected more than 200. The tree is incomplete." >&2
  rm -f "$observed"
  exit 1
fi

if [ "$MODE" = "--write" ]; then
  grep '^#' "$LIST" > "$LIST.new" 2>/dev/null
  cat "$observed" >> "$LIST.new"
  mv "$LIST.new" "$LIST"
  rm -f "$observed"
  echo "wrote $LIST"
  exit 0
fi

listed=$(mktemp)
grep -v '^#' "$LIST" | grep -v '^[[:space:]]*$' | sort > "$listed"
sort -o "$observed" "$observed"

unlisted=$(comm -23 "$observed" "$listed")
stale=$(comm -13 "$observed" "$listed")
rm -f "$observed" "$listed"

status=0
if [ -n "$unlisted" ]; then
  echo
  echo "FAIL: these files fail and CHECK-FAILS.txt does not say so (file, stage):"
  printf '%s\n' "$unlisted" | sed 's/^/  + /'
  status=1
fi
if [ -n "$stale" ]; then
  echo
  echo "FAIL: CHECK-FAILS.txt lists these lines and they are no longer true. Remove or correct them:"
  printf '%s\n' "$stale" | sed 's/^/  - /'
  status=1
fi
if [ $status -eq 0 ]; then
  echo "OK: the failing files are exactly the ones CHECK-FAILS.txt lists."
fi
exit $status
