# Build Ecosystem Enterprise Readiness

Build Ecosystem is the one-command install surface for the full Build
family. It carries no logic of its own; its enterprise value is reducing
"install six packages at compatible versions" to `pip install build-ecosystem`.

## Enterprise role

- Pin compatible minimum versions of build-color, build-finance,
  build-oracle, build-engine, calibrate-pro, and build-ui in one
  `pyproject.toml`, so a single install brings up the whole family at once.
- Provide `gui` and `all` extras that forward to each member's own extras,
  so GUI or full-optional installs stay a one-line command.

## Operator surface

- `pip install build-ecosystem` — core dependencies of every member.
- `pip install "build-ecosystem[gui]"` — every member's GUI extra.
- `pip install "build-ecosystem[all]"` — every member's full optional extra.
- No CLI, no importable API beyond `build_ecosystem.__version__`.

## Dependencies and boundary

- **Runtime:** the six pinned member packages. No dependencies beyond them.
- Each member's own dependency and security surface (numpy/scipy, PyQt6,
  network access for exchange APIs in build-finance, etc.) applies once
  installed; see that member's own `docs/ENTERPRISE-READINESS.md`.

## Quality gates

- `ruff check .` and `pytest` run in CI. The test suite uses
  `pytest.importorskip` per member so it passes in an isolated checkout and
  verifies real cross-package imports when the full family is installed.

## Honest limits

- This package does not validate compatibility beyond version pins — it
  does not run the members' own test suites. Compatibility guarantees are
  only as strong as each member's own release testing.
- All functional, security, and correctness guarantees live in the member
  repositories, not here.
