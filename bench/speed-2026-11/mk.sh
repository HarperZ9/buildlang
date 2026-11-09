#!/bin/sh
# usage: mk.sh NAME  (in perf/NAME: main.bld built by buildc, main.rs, main_ref.c)
set -e
cd "$(dirname "$0")/$1"
B=${BUILDC:-buildc}
$B build --release --keep-c . > build.log 2>&1
gcc -O2 -o ${1}_bl_O2.exe target/release/main.c -lm 2> cc.log
gcc -O3 -o ${1}_bl_O3.exe target/release/main.c -lm 2>> cc.log
gcc -O2 -o ${1}_c_O2.exe main_ref.c -lm
rustc -C opt-level=3 -o ${1}_rust.exe main.rs
for v in bl_O2 bl_O3 rust c_O2; do ./${1}_$v.exe $2 | sha256sum | cut -c1-16; done
