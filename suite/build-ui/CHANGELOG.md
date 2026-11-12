# Changelog

All notable changes to Build UI are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] - 2026-07-10

### Changed

- Replaced direct Qt binding imports with QtPy while preserving the public
  theme and widget names.
- Changed installation from one implicit PyQt6 dependency to explicit
  `pyqt6` and `pyside6` extras. The core package now installs QtPy without a Qt
  binding.
- Added a documented `QT_API` selection and one-binding-per-process invariant.
- Added isolated PyQt6 and PySide6 CI lanes, a binding-free lane, behavior and
  mismatch probes, a 70% coverage gate, and clean-wheel candidate verification.
- Added the PySide6 installation lane that Calibrate Pro consumes after its own
  downstream GUI acceptance and candidate-hash check.

### Security

- Removed automatic publishing from the candidate workflow. Candidate builds
  are manual, read-only, hashed, and verified before any later release action.
- Declared the existing FSL license text with the valid PEP 639 custom
  identifier `LicenseRef-FSL-1.1-MIT`.

## [1.0.1] - 2026-06-30

### Added

- Flagship parity pass: mypy and coverage configuration, development extras,
  CI gates, root documentation, enterprise readiness notes, and brand assets.

### Fixed

- Type annotation on widgets (`color: str | None`) so mypy passes clean.

## [1.0.0]

### Added

- Shared PyQt6 dark theme and reusable widget library for consistent styling
  across Build applications.
