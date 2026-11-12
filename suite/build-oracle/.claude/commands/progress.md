---
description: Show project progress — files, tests, recent activity
allowed-tools: Read, Bash(find:*), Bash(ls:*), Bash(wc:*), Bash(git log:*)
---

# Project Progress

## Check

```bash
echo "=== Source Files ==="
find . -name "*.py" -not -path "./.venv/*" -not -path "./__pycache__/*" | wc -l | xargs -I{} echo "Python: {} files"

echo ""
echo "=== Test Files ==="
find tests/ -name "test_*.py" 2>/dev/null | wc -l | xargs -I{} echo "Tests: {} files"

echo ""
echo "=== Recent Activity ==="
git log --oneline --since="7 days ago" 2>/dev/null | head -15

echo ""
echo "=== Test Results ==="
python -m pytest tests/ --tb=no -q 2>&1 | tail -5
```

## Output

| Area | Count | Status |
|------|-------|--------|
| Source | N files | ... |
| Tests | N files | ... |

### Next Actions
1. ...
2. ...
