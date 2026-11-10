#!/bin/sh
# usage: mk.sh NAME ARG   builds every variant of one benchmark and prints output hashes
# BUILDC names the compiler and RUSTC the Rust compiler (for example "rustup run 1.98.0 rustc"); BUILDLANG_STDLIB defaults to the repository's stdlib.
set -e
here="$(cd "$(dirname "$0")" && pwd)"
B="${BUILDC:-$here/../../compiler/target/release/buildc}"
export BUILDLANG_STDLIB="${BUILDLANG_STDLIB:-$here/../../stdlib}"
cd "$here/$1"
"$B" -o main.c main.bld > build.log 2>&1 || { tail -5 build.log; exit 1; }
gcc -O2 -w -o bl_O2.exe main.c -lm
gcc -O3 -w -o bl_O3.exe main.c -lm
gcc -O3 -march=native -w -o bl_O3n.exe main.c -lm
gcc -O2 -o c_O2.exe main_ref.c -lm
gcc -O3 -o c_O3.exe main_ref.c -lm
gcc -O3 -march=native -o c_O3n.exe main_ref.c -lm
${RUSTC:-rustc} -C opt-level=3 -o rust.exe main.rs
${RUSTC:-rustc} -C opt-level=3 -C target-cpu=native -C codegen-units=1 -C panic=abort -o rust_n.exe main.rs
for v in bl_O2 bl_O3 bl_O3n c_O2 c_O3 c_O3n rust rust_n; do
  printf "%s " $v; ./$v.exe $2 | tr -d '\015' | sha256sum | cut -c1-12
done
