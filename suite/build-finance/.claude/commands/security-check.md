---
description: Scan project for security issues
allowed-tools: Read, Grep, Glob, Bash(git:*), Bash(grep:*)
---

# Security Check

## Checks

### 1. Secrets in Code
```bash
git grep -n -E '(api[_-]?key|secret[_-]?key|password|token)\s*[:=]\s*["\x27][A-Za-z0-9+/=_-]{8,}' -- ':!*.lock' 2>/dev/null || echo "No secrets found"
git grep -n 'AKIA[0-9A-Z]\{16\}' 2>/dev/null || echo "No AWS keys found"
```

### 2. .gitignore Coverage
Verify: `.env`, `.env.*`, `__pycache__/`, `*.pyc`, `dist/`, `build/`, `.egg-info/`, `CLAUDE.local.md`

### 3. Sensitive Files
```bash
for f in .env .env.local secrets.json credentials.json id_rsa .pypirc; do
  git ls-files --error-unmatch "$f" 2>/dev/null && echo "WARNING: $f is tracked!"
done
```

### 4. Dependency Audit
```bash
pip-audit 2>/dev/null || echo "pip-audit not installed"
```

## Output
| Check | Status | Details |
|-------|--------|---------|
| Secrets | ... | ... |
| .gitignore | ... | ... |
| Sensitive files | ... | ... |
| Dependencies | ... | ... |
