---
description: Smart commit with conventional commit message
allowed-tools: Bash(git add:*), Bash(git status:*), Bash(git commit:*), Bash(git diff:*)
argument-hint: [optional commit message override]
---

# Smart Commit

## Context
- Current git status: `git status --short`
- Current diff: `git diff HEAD --stat`
- Current branch: `git branch --show-current`
- Recent commits: `git log --oneline -5`

## Task

Review the staged changes and create a commit.

### Rules
1. Use **conventional commit** format: `type(scope): description`
   - Types: feat, fix, docs, refactor, test, chore, perf
2. Description should be concise (max 72 chars)
3. If changes span multiple concerns, suggest splitting
4. NEVER commit .env files or secrets

### If message provided
Use: $ARGUMENTS

### If no message provided
Generate an appropriate commit message based on the diff.
