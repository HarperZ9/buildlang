---
description: Refactor a file following project best practices
argument-hint: <file-path> [--dry-run]
allowed-tools: Read, Write, Edit, Grep, Glob, Bash
---

# Refactor

**Target:** $ARGUMENTS

If `--dry-run` is passed, report what WOULD change without modifying files.

## Step 0 — Read Before Touching
Read the target file fully and understand its imports/dependents.

## Step 1 — Audit

### 1A. File Size
- **> 300 lines = MUST split.** Identify logical sections.

### 1B. Function Size
- **> 50 lines = MUST extract.** Each "thing" becomes its own function.

### 1C. Python Quality
- Type hints on all public functions (Python 3.10+ union syntax)
- No bare `except:` — always catch specific exceptions
- No mutable default arguments
- Use dataclasses or Pydantic models for structured data

### 1D. Import Hygiene
- No circular imports
- No wildcard imports (`from x import *`)
- Standard ordering: stdlib, third-party, local

### 1E. Error Handling
- No swallowed errors
- Errors must be logged with context
- Use custom exception classes where appropriate

### 1F. Dead Code
- Unused functions/variables — remove
- Commented-out code — remove (use git history)

## Step 2 — Plan
Present the plan and WAIT for approval.

## Step 3 — Execute
Make changes in order: create new files, move code, update imports, verify.

## Step 4 — Verify
```bash
python -m pytest tests/ -x -q
ruff check .
```
