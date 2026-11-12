---
description: Create a git worktree + branch for an isolated task
argument-hint: <branch-name> [base-branch]
allowed-tools: Bash, Read
---

# Git Worktree

Create a new git worktree for isolated work. Main stays untouched.

**Arguments:** $ARGUMENTS

## Steps

1. Parse branch name (prefix with `task/` if no prefix given)
2. Check for uncommitted changes — warn if dirty
3. Create worktree: `git worktree add -b <branch> ../<repo>--<branch> <base>`
4. Report the directory path and next steps

## When Done

1. Review diff: `git diff main...HEAD`
2. Merge or open PR
3. Cleanup: `git worktree remove ../<dir>` then `git branch -d <branch>`
