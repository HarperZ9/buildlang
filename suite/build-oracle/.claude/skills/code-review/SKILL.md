---
name: Code Review
description: Comprehensive code review for Python projects
triggers:
  - review
  - audit
  - check code
---

# Code Review Skill

## 1. Security
- [ ] No hardcoded secrets or API keys
- [ ] Input validation on user data
- [ ] SQL injection prevention (parameterized queries)
- [ ] No pickle/eval on untrusted data
- [ ] Authentication on protected routes

## 2. Python Quality
- [ ] Type hints on all public functions
- [ ] No bare `except:` blocks
- [ ] No mutable default arguments
- [ ] Proper use of dataclasses/Pydantic
- [ ] No `global` statements

## 3. Error Handling
- [ ] Specific exception types caught
- [ ] Errors logged with context
- [ ] Resources properly closed (with/contextmanager)
- [ ] No swallowed exceptions

## 4. Performance
- [ ] No N+1 database queries
- [ ] Generators for large datasets
- [ ] No unnecessary copies of large data
- [ ] Proper use of caching where needed

## 5. Testing
- [ ] New code has corresponding tests
- [ ] Tests have explicit assertions
- [ ] Edge cases covered (None, empty, max)
- [ ] Parametrize used for multiple inputs
