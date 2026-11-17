#!/usr/bin/env bash
# Every skipped test names the issue that tracks it.
#
# A skip is a `--skip <name>` in a script or workflow under .github/, or an
# `#[ignore]` attribute in the compiler's Rust sources. Each one passes only
# when its own line or the line above it links an issue in this repository:
# `HarperZ9/buildlang#123` or `https://github.com/HarperZ9/buildlang/issues/123`.
# Anything else fails the job, so a test cannot leave the run without a trail.
#
# Usage: check-skips.sh [repo-root]   (default: the current directory)
#        check-skips.sh --self-test   (planted skips: the check must refuse
#                                      the unlinked ones and accept the linked)
set -uo pipefail

if [ "${1:-}" = "--self-test" ]; then
  me="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/bad/.github" "$tmp/bad/compiler" "$tmp/good/.github" "$tmp/good/compiler"
  printf 'cargo test -- --skip some::test\n' > "$tmp/bad/.github/run.sh"
  printf '#[test]\n#[ignore]\nfn t() {}\n' > "$tmp/bad/compiler/a.rs"
  printf '# HarperZ9/buildlang#1\ncargo test -- --skip some::test\n' > "$tmp/good/.github/run.sh"
  printf '#[test]\n#[ignore = "https://github.com/HarperZ9/buildlang/issues/1"]\nfn t() {}\n' > "$tmp/good/compiler/a.rs"
  bad_out=$(bash "$me" "$tmp/bad" 2>&1); bad=$?
  bash "$me" "$tmp/good" > /dev/null 2>&1; good=$?
  refused=$(printf '%s\n' "$bad_out" | grep -c 'skip without an issue link')
  echo "planted unlinked skips: exit $bad, $refused of 2 refused; linked skips: exit $good"
  [ "$bad" -ne 0 ] && [ "$refused" -eq 2 ] && [ "$good" -eq 0 ]
  exit
fi

root="${1:-.}"
cd "$root" || { echo "::error::no such directory: $root"; exit 2; }

self=".github/check-skips.sh"
issue_re='HarperZ9/buildlang#[0-9]+|github[.]com/HarperZ9/buildlang/issues/[0-9]+'

files=()
if [ -d .github ]; then
  while IFS= read -r f; do
    [ "$f" = "$self" ] && continue
    files+=("$f")
  done < <(find .github -type f \( -name '*.sh' -o -name '*.yml' -o -name '*.yaml' \) | sed 's|^\./||' | sort)
fi
if [ -d compiler ]; then
  while IFS= read -r f; do
    files+=("$f")
  done < <(find compiler -type f -name '*.rs' -not -path '*/target/*' | sed 's|^\./||' | sort)
fi

found=0
unlinked=0
for f in "${files[@]}"; do
  case "$f" in
    *.rs) skip_re='#[[]ignore' ;;
    *) skip_re='(^|[[:space:]])--skip([[:space:]=]|$)' ;;
  esac
  out=$(SKIP_RE="$skip_re" ISSUE_RE="$issue_re" FILE="$f" awk '
    BEGIN { skip_re = ENVIRON["SKIP_RE"]; issue_re = ENVIRON["ISSUE_RE"]; file = ENVIRON["FILE"] }
    { sub(/\r$/, "") }
    $0 ~ skip_re {
      found++
      if ($0 !~ issue_re && prev !~ issue_re) {
        printf "%s:%d: skip without an issue link: %s\n", file, NR, $0
        bad++
      }
    }
    { prev = $0 }
    END { printf "COUNTS %d %d\n", found + 0, bad + 0 }
  ' "$f") || { echo "::error::could not scan $f"; exit 2; }
  counts=$(printf '%s\n' "$out" | tail -n 1)
  printf '%s\n' "$out" | sed '$d' | sed 's/^/::error::/'
  set -- $counts
  found=$((found + $2))
  unlinked=$((unlinked + $3))
done

echo "skips found: $found; without an issue link: $unlinked (checked ${#files[@]} files)"
[ "$unlinked" -eq 0 ]
