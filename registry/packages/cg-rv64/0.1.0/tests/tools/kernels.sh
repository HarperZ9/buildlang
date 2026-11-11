#!/usr/bin/env bash
# The slice target of cg-rv64: the trace of a kernel core is the same
# bytes on the host and on the emulator.
#
#   kernels.sh PACKAGE SIZE SEED STEPS     one trace, any parameters
#   kernels.sh PACKAGE goldens             every committed golden trace
#
# PACKAGE is os-pfa or os-runq, version 0.1.0, read from the registry. No
# file of the package is edited: the function `trace` of its own
# src/main.bld is the entry function, and SIZE, SEED and STEPS reach it
# as entry arguments of cg-rv64 (the first argument, 0 or 1, selects the
# core or the reference model, as `trace` and `trace-ref` do on the host).
#
# Steps:
# 1. Copy the package from the registry and compare the SHA-256 of every
#    source file of the copy with the registry file.
# 2. Build the copy on the host with `buildc pkg build`.
# 3. Write the MIR of the joined source, target/pkg/work/main.bld, with
#    `buildc mir emit`, and compare it with the committed fixture
#    tests/mir/PACKAGE.mir.json byte for byte.
# 4. For the core and for the model: run the hosted program, run the
#    emulator through `cg-rv64 run`, and compare the bytes. With
#    `goldens`, compare both with the committed golden file as well.
# 5. Run `cg-rv64 run-check`, which runs the image twice and compares the
#    state hashes.
#
# Environment: BUILDC, CG, BUILDLANG_STDLIB, WORK, REGISTRY (the registry
# folder), MAXSTEPS (default 2000000000). Run from the package folder.
set -u
PKG=$1
MAX=${MAXSTEPS:-2000000000}
SRC=$REGISTRY/packages/$PKG/0.1.0
K=$WORK/kernel-$PKG
rm -rf "$K" && mkdir -p "$K" && cp -r "$SRC/." "$K/"
fail=0
for f in "$SRC"/src/*.bld; do
  a=$(sha256sum < "$f" | cut -d' ' -f1)
  b=$(sha256sum < "$K/src/$(basename "$f")" | cut -d' ' -f1)
  echo "source $(basename "$f") sha256 $a"
  [ "$a" = "$b" ] || { echo "SOURCE DIFFERS: $f"; fail=1; }
done
(cd "$K" && "$BUILDC" pkg build . --registry "$REGISTRY" > build.log 2>&1) || { echo "host build failed"; exit 2; }
"$BUILDC" mir emit "$K/target/pkg/work/main.bld" -o "$K/$PKG.mir.json" > /dev/null || { echo "mir emit failed"; exit 2; }
if cmp -s "$K/$PKG.mir.json" "tests/mir/$PKG.mir.json"; then
  echo "fixture tests/mir/$PKG.mir.json equals the MIR of the registry sources"
else
  echo "FIXTURE DIFFERS from the MIR of the registry sources"; fail=1
fi
one() { # size seed steps golden-or-empty
  for ref in 0 1; do
    cmd=trace; [ $ref = 1 ] && cmd=trace-ref
    "$K/target/pkg/$PKG" $cmd "$1" "$2" "$3" > "$K/host.txt" 2>&1; hrc=$?
    "$CG" run "tests/mir/$PKG.mir.json" trace "$MAX" $ref "$1" "$2" "$3" > "$K/emu.txt" 2> "$K/emu.err"; erc=$?
    verdict=equal
    cmp -s "$K/host.txt" "$K/emu.txt" && [ $hrc = $erc ] || { verdict=DIFFERENT; fail=1; }
    if [ -n "$4" ]; then cmp -s "$K/emu.txt" "$4" || { verdict="DIFFERENT from $4"; fail=1; }; fi
    echo "$PKG $cmd $1 $2 $3: host status $hrc, emulator status $erc, $(wc -c < "$K/emu.txt") bytes, $verdict"
    "$CG" run-check "tests/mir/$PKG.mir.json" trace "$MAX" $ref "$1" "$2" "$3" | tail -3 | tr '\n' ' '; echo
  done
}
if [ "$2" = goldens ]; then
  for g in "$SRC"/tests/golden-trace-*.txt; do
    set -- $(grep "$(basename "$g") trace " "$SRC/tests/cases.txt")
    one "$4" "$5" "$6" "$g"
  done
else
  one "$2" "$3" "$4" ""
fi
[ $fail = 0 ] && echo "result: all equal" || echo "result: FAILED"
exit $fail
