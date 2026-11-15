#!/usr/bin/env bash
# Best-effort demo — these checks were run by the author against the current
# tree and passed; treat behavior on other environments as best-effort.
#
# Runs the genuine validation surface this repository uses (the same checks as
# .github/workflows/ci.yml). This package has no compiler/CLI of its own; the
# only runnable surface is JSON validation + whitespace check. Requires Python 3
# and git on PATH. Run from the repository root:
#
#   bash examples/validate_demo.sh
#
# The grammar maps the demo source examples/demo.bld to TextMate scopes; to
# SEE highlighting you must open that file in an editor with the grammar
# installed (see USAGE.md). This script only validates the package files.

set -euo pipefail

echo "==> Validating grammars/buildlang.tmLanguage.json"
python -m json.tool grammars/buildlang.tmLanguage.json > /dev/null
echo "    OK (well-formed JSON)"

echo "==> Validating language-configuration.json"
python -m json.tool language-configuration.json > /dev/null
echo "    OK (well-formed JSON)"

echo "==> Reporting grammar identity"
python - <<'PY'
import json
with open("grammars/buildlang.tmLanguage.json", encoding="utf-8") as fh:
    g = json.load(fh)
print(f"    name      = {g['name']}")
print(f"    scopeName = {g['scopeName']}")
print(f"    fileTypes = {g['fileTypes']}")
PY

echo "==> Checking whitespace (git diff --check)"
git diff --check
echo "    OK (no whitespace/conflict-marker issues)"

echo "All checks passed."
