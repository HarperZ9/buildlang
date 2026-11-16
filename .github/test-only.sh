#!/usr/bin/env bash
# Run the compiler tests (from compiler/): all of them, or one test binary when
# the commit under test asks for it with a line `test-only: <name>` in its
# message. A revert check uses that to read one binary in minutes.
#
# A narrowed run is never green: it exits 1 after the tests, whatever they
# did, so a commit that asks for one binary cannot satisfy a merge. A push to
# main never narrows.
set -uo pipefail

only=""
if [ "${IS_MAIN:-false}" != "true" ] && [ -n "${HEAD_SHA:-}" ]; then
  git fetch -q --depth=1 origin "$HEAD_SHA" 2>/dev/null || true
  msg=$(git log -1 --format=%B "$HEAD_SHA" 2>/dev/null || true)
  only=$(printf '%s\n' "$msg" | sed -n 's/^test-only: *\([A-Za-z0-9_]*\) *$/\1/p' | head -n 1)
fi

if [ -z "$only" ]; then
  exec cargo test -- --skip spirv::tests::test_triangle --skip spirv::tests::test_write
fi

if [ ! -f "tests/$only.rs" ]; then
  echo "::error::the commit asks for tests/$only.rs, which does not exist"
  exit 1
fi
echo "::warning::the commit asks for tests/$only.rs only; this run is a revert check and cannot pass"
cargo test --no-fail-fast --test "$only"
status=$?
echo "narrowed run of tests/$only.rs: cargo test exited $status; the job fails so this commit cannot be merged"
exit 1
