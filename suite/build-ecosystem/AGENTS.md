# AGENTS.md - Build Ecosystem

## Scope

This file applies to the `build-ecosystem` repository. Root workspace
instructions still apply; this repo is a public meta-package that installs
the full Build family in one command.

## Product Boundary

Build Ecosystem is an aggregator with no logic of its own. Keep the public
repo focused on version pinning of member packages and the cross-wiring smoke
tests that confirm the family installs and imports together.

Publishable surfaces:

- `build_ecosystem/` - the single-file meta-package.
- `tests/` - import and cross-wiring smoke tests.
- `README.md`, `docs/`, and `pyproject.toml` - package and product posture.

Keep local-only unless deliberately scrubbed:

- `.env`, `.env.*`, local settings, generated logs, and build artifacts.

## Editing Rules

- Do not add forecasting, color-science, trading, or calibration logic here
  — that belongs in the owning member repository.
- When a member ships a release the ecosystem should adopt, bump its pinned
  minimum version in `pyproject.toml` (both the base dependency and, if
  applicable, the `gui`/`all` extras).
- Keep `tests/test_ecosystem.py` using `pytest.importorskip` per member so
  the suite still passes in an isolated checkout.

## Verification

```powershell
git diff --check
python -m pytest tests/ -q
```

Before committing or pushing, scan changed files for credential-shaped
content and confirm `.env` remains ignored.
