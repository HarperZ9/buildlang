# AGENTS.md - Build UI

## Scope

This file applies to the `build-ui` repository. Root workspace instructions
still apply. This repository is a binding-neutral Qt 6 theme and widget library
consumed by Calibrate Pro, Build Color, Build Finance, Build Oracle, and Build
Engine.

## Product boundary

Build UI provides a shared visual theme (`theme.py`), a private QtPy selection
facade (`_qt.py`), and reusable widgets (`widgets.py`). It has no application
logic, network access, or application file I/O.

Publishable surfaces:

- `build_ui/` - package code.
- `tests/` - binding, behavior, packaging, and release regression coverage.
- `README.md`, `USAGE.md`, `MIGRATING.md`, `ARCHITECTURE.md`, and
  `pyproject.toml` - package and product posture.

Keep local-only unless deliberately scrubbed:

- `.env`, `.env.*`, local settings, generated logs, build artifacts, and
  unpublished candidate receipts.

## Editing rules

- Keep QtPy as the core runtime dependency. Keep PyQt6 and PySide6 in separate
  optional extras; do not make either binding a core dependency.
- QtPy source plus exactly one selected binding per process. Never import
  PyQt6 or PySide6 directly from package code, mix binding objects, or mutate
  `QT_API` after QtPy import.
- Do not add application-specific logic; it belongs in the consuming app.
- New widgets must use `build_ui.theme.C`, not hardcoded colors.
- Preserve `C`, `STYLE`, `create_stylesheet`, and all existing widget class
  names because downstream repositories import them directly.
- New widgets need importability and behavior tests in both binding lanes.

## Verification

Documentation changes must run the release-contract tests and
`git diff --check`. Package changes must run pytest with the 70% coverage gate,
Ruff check and format check, QtPy-derived mypy flags, the JSON binding probe,
and the requested-but-absent binding probe under PyQt6 and PySide6.

Before committing or pushing, scan changed files for credential-shaped content,
confirm `.env` remains ignored, and do not publish without a later explicit
release approval after downstream acceptance.
