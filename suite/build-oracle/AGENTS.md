# AGENTS.md - Build Oracle

## Scope

This file applies to the `build-oracle` repository. Root workspace
instructions still apply; this repo is a public Python time-series
forecasting library and a dependency surface for the Build family.

## Product Boundary

Build Oracle is a reusable forecasting package. Keep the public repo focused
on deterministic forecasting models, changepoint detection, decomposition,
feature engineering, evaluation metrics, streaming updates, CLI behavior, and
optional GUI workbench surfaces.

Publishable surfaces:

- `build_oracle/` - package code.
- `tests/` - regression coverage for ARIMA, VAR, neural, ensemble, streaming,
  changepoint, decomposition, metrics, and persistence.
- `README.md`, `CHANGELOG.md`, `docs/`, and `pyproject.toml` - package and
  product posture.

Keep local-only unless deliberately scrubbed:

- `.env`, `.env.*`, local settings, generated logs, and build artifacts.
- Saved model files, generated forecast/report output, and any customer or
  proprietary series data used for local testing.

## Editing Rules

- Preserve numerical behavior with focused tests; forecasting math
  regressions are easy to make and hard to see by inspection alone.
- Keep optional GUI dependencies optional. Core package tests should not
  require PyQt6.
- Keep CLI examples aligned with `pyproject.toml` entry points and the
  `argparse` subcommands in `cli.py`.
- When adding a new model, metric, or feature transform, document the
  method and add at least one roundtrip, invariant, or known-value test.
- Never claim a forecast is a guarantee; keep language in docs and CLI output
  consistent with "estimate," not "prediction of fact."

## Verification

For documentation or release-boundary changes:

```powershell
git diff --check
```

For package behavior changes, run the focused core suite:

```powershell
python -m pytest tests/test_arima.py tests/test_multivariate.py tests/test_ensemble.py -q
python -m pytest tests/test_streaming.py tests/test_changepoint.py tests/test_decompose.py -q
```

Before committing or pushing, scan changed files for credential-shaped
content and confirm `.env` remains ignored.
