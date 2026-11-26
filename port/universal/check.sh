#!/usr/bin/env bash
# Universal Hourglass Lifter Verification Harness (port/universal/check.sh)
# Builds all 3 pure-BuildLang lifters (wasm_lift.bld, asm_riscv_lift.bld, multi_lang_lift.bld),
# lifts every corpus fixture across wat, riscv, js, rs, zig, go, rb, and spvasm, compiles and
# executes each lifted .bld binary via buildc run, verifies byte-for-byte output parity against
# golden references (and live node for JS when available), verifies exact refusal line numbers,
# and runs a planted-mismatch control that MUST fail.
#
#   port/universal/check.sh path/to/buildc
set -u
BUILDC="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

build_lifter() {
  local src="$1"
  local name="$2"
  local dir="$WORK/build_$name"
  mkdir -p "$dir"
  cp "$src" "$dir/main.bld"
  if ! "$BUILDC" check "$dir/main.bld" > "$WORK/$name.check.log" 2>&1; then
    echo "FAIL  buildc check $name"; cat "$WORK/$name.check.log"; exit 1
  fi
  if ! "$BUILDC" lint "$dir/main.bld" > "$WORK/$name.lint.log" 2>&1; then
    echo "FAIL  buildc lint $name"; cat "$WORK/$name.lint.log"; exit 1
  fi
  if ! (cd "$dir" && "$BUILDC" build --release . > "$WORK/$name.build.log" 2>&1); then
    echo "FAIL  buildc build $name"; cat "$WORK/$name.build.log"; exit 1
  fi
  if [ -x "$dir/target/release/main.exe" ]; then
    echo "$dir/target/release/main.exe"
  else
    echo "$dir/target/release/main"
  fi
}

WASM_LIFT="$(build_lifter "$HERE/wasm_lift.bld" wasm_lift)"
RISCV_LIFT="$(build_lifter "$HERE/asm_riscv_lift.bld" asm_riscv_lift)"
MULTI_LIFT="$(build_lifter "$HERE/multi_lang_lift.bld" multi_lang_lift)"

same=0; total=0
refused=0; rtotal=0

run_accept() {
  local label="$1"
  local src="$2"
  local want_file="$3"
  shift 3
  total=$((total + 1))
  local out_bld="$WORK/${label}.bld"
  local got_file="$WORK/${label}.got"
  local want_norm="$WORK/${label}.want"
  tr -d '\r' < "$want_file" > "$want_norm"
  if ! "$@" "$src" "$out_bld" > "$WORK/${label}.lift.log" 2>&1; then
    echo "DIFF  $label: lifter refused accepted corpus file"; cat "$WORK/${label}.lift.log"; fail=1; return
  fi
  "$BUILDC" run "$out_bld" 2> "$WORK/${label}.err" | tr -d '\r' > "$got_file"
  if [ -s "$want_norm" ] && cmp -s "$want_norm" "$got_file"; then
    same=$((same + 1)); echo "same  $label ($(wc -l < "$want_norm" | tr -d ' ') lines)"
  else
    echo "DIFF  $label"; diff "$want_norm" "$got_file" | head -10; head -5 "$WORK/${label}.err"; fail=1
  fi
}

run_refuse() {
  local label="$1"
  local src="$2"
  local lines_file="$3"
  shift 3
  rtotal=$((rtotal + 1))
  local out_bld="$WORK/r_${label}.bld"
  "$@" "$src" "$out_bld" > "$WORK/r_${label}.log" 2>&1
  local rc=$?
  local got
  got="$(grep -o '^  line [0-9]*' "$WORK/r_${label}.log" | tr -dc '0-9\n' | tr '\n' ' ' | sed 's/ $//')"
  local want
  want="$(tr -d '\r' < "$lines_file" | tr '\n' ' ' | sed 's/ $//')"
  if [ "$rc" = 2 ] && [ ! -e "$out_bld" ] && [ "$got" = "$want" ]; then
    refused=$((refused + 1)); echo "refused  $label at lines $got"
  else
    echo "WRONG  $label: exit $rc, lines [$got], wanted exit 2 and lines [$want]"
    cat "$WORK/r_${label}.log"
    fail=1
  fi
}

# 1. WAT corpus
for f in "$HERE"/corpus/wat/*.wat; do
  n="$(basename "$f" .wat)"
  run_accept "wat_$n" "$f" "${f%.wat}.want" "$WASM_LIFT"
done
for f in "$HERE"/corpus/wat/refused/*.wat; do
  n="$(basename "$f" .wat)"
  run_refuse "wat_$n" "$f" "${f%.wat}.lines" "$WASM_LIFT"
done

# 2. RISC-V corpus
for f in "$HERE"/corpus/riscv/*.s; do
  n="$(basename "$f" .s)"
  run_accept "riscv_$n" "$f" "${f%.s}.want" "$RISCV_LIFT"
done
for f in "$HERE"/corpus/riscv/refused/*.s; do
  n="$(basename "$f" .s)"
  run_refuse "riscv_$n" "$f" "${f%.s}.lines" "$RISCV_LIFT"
done

# 3. Multi-language corpus (js, rs, zig, go, rb, spvasm)
for spec in "js:js" "rs:rs" "zig:zig" "go:go" "rb:rb" "spvasm:spvasm"; do
  lang="${spec%%:*}"
  ext="${spec##*:}"
  for f in "$HERE/corpus/$lang"/*."$ext"; do
    n="$(basename "$f" ".$ext")"
    run_accept "${lang}_$n" "$f" "${f%."$ext"}.want" "$MULTI_LIFT" "$lang"
    if [ "$lang" = "js" ] && command -v node >/dev/null 2>&1; then
      node "$f" | tr -d '\r' > "$WORK/node_${n}.want"
      if cmp -s "$WORK/node_${n}.want" "$WORK/${lang}_$n.got"; then
        echo "node-diff  js_$n matches live node byte-for-byte"
      else
        echo "DIFF  js_$n vs live node"; fail=1
      fi
    fi
  done
  for f in "$HERE/corpus/$lang/refused"/*."$ext"; do
    n="$(basename "$f" ".$ext")"
    run_refuse "${lang}_$n" "$f" "${f%."$ext"}.lines" "$MULTI_LIFT" "$lang"
  done
done

# 4. Planted-mismatch control (MUST detect difference between distinct lifted programs)
if cmp -s "$WORK/wat_arith_calls.got" "$WORK/wat_fib_iter.want"; then
  echo "control: two distinct lifted programs compared equal"
  fail=1
else
  echo "control: planted mismatch detected: 1 of 1"
fi

echo "universal lifters identical to reference: $same of $total; refused as recorded: $refused of $rtotal"
exit $fail
