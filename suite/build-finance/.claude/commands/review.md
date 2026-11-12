---
description: Review code for bugs, security issues, and best practices
allowed-tools: Read, Grep, Glob, Bash(git diff:*)
---

# Code Review

## Context
- Current diff: `git diff HEAD`
- Staged changes: `git diff --cached`
- Current branch: `git branch --show-current`

## Review Checklist (Python)

1. **Security** — No secrets in code, proper input validation, no SQL injection
2. **Type Hints** — Type annotations on all public functions, use Python 3.10+ syntax
3. **Error Handling** — No bare except, no swallowed errors, proper logging
4. **Performance** — No N+1 queries, proper use of generators, no memory leaks
5. **Testing** — New code has tests, tests have explicit assertions
6. **Imports** — Clean imports, no circular dependencies, no wildcard imports
7. **Quality Gates** — Files <= 300 lines, functions <= 50 lines

## Output Format

For each issue found:
- **File**: path/to/file.py:line
- **Severity**: CRITICAL | WARNING | INFO
- **Issue**: Description
- **Fix**: Suggested change
