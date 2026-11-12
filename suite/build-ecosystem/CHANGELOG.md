# Changelog

All notable changes to Build Ecosystem are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2026-06-30

### Added
- Flagship parity pass: `[tool.mypy]` and `[tool.coverage]` configuration, `dev` extras,
  mypy in CI, OIDC trusted-publishing release workflow, full root doc set (ARCHITECTURE,
  SECURITY, AGENTS, AUTHORS, CONTRIBUTING, USAGE, ENTERPRISE-READINESS), and brand assets.

## [1.0.0]

### Added
- Meta-package aggregating the Build family — `build-color`, `build-finance`,
  `build-oracle`, `build-engine`, `build-ui`, and `calibrate-pro` — so the whole suite
  installs with a single `pip install build-ecosystem`. Optional `gui` and `all` extras
  pull the members' corresponding extras.
