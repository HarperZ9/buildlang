# Build Oracle Enterprise Readiness

Build Oracle is the forecasting engine of the build/Project Telos family: a
dependency-light, deterministic time-series library that turns forecasting
into reproducible, inspectable results. It is designed to be used alone as a
library or CLI, and as a component other flagships in the Build family
depend on.

## Enterprise role

- Fit and forecast with ARIMA, VAR (multivariate), Prophet-style additive
  decomposition, a NumPy-only neural forecaster, and a dynamically weighted
  ensemble of those models.
- Detect structural breaks (PELT changepoint detection), decompose series
  into trend/seasonal/residual, and engineer statistical/temporal/frequency
  features from raw series.
- Update forecasts incrementally as new observations arrive without a full
  model refit (`streaming.StreamForecaster`).
- Keep the numeric core free of heavy or network dependencies, so it installs
  and runs in constrained and offline environments.

## Operator surface

- `build-oracle` CLI for scriptable forecasting operations (`forecast`,
  `decompose`, `changepoints`, `features`, `gui`).
- The importable Python API (`build_oracle.arima`, `.var`, `.prophet`,
  `.neural`, `.ensemble`, `.streaming`, `.changepoint`, `.decompose`,
  `.features`, `.metrics`) for embedding in pipelines.
- An optional PyQt6 workbench (`pip install ".[gui]"`) for interactive
  inspection of forecasts, decompositions, and changepoints.

## Reproducibility and provenance

- Fitting and prediction are deterministic functions of their inputs (and,
  where randomness is used, such as neural weight initialization, an explicit
  seed), which makes results reproducible and diffable across runs and
  machines.
- Saved model artifacts (`--save` / `--load` in the `forecast` command) are
  the durable output; they can be re-generated from the same input series.
- When used inside Project Telos, forecasting operations and their outputs
  can be referenced by content hash rather than carrying raw series data
  through every context window.

## Dependencies and boundary

- **Runtime core:** `numpy` and `scipy` only. No network, no code evaluation.
- **Optional:** `pandas`, `scikit-learn`, `matplotlib` (analysis conveniences)
  and `PyQt6` (GUI). Each is isolated behind an extra so the core stays
  minimal.
- The GUI and CLI consume the core; they are never a dependency of it.

## Quality gates

- `ruff check .` (style), `mypy` (types: the numeric core is type-clean; the
  GUI adapter is boundary-typed per the overrides in `pyproject.toml`), and
  `pytest` run in CI on every push and pull request.

## Honest limits

- Forecasts are statistical estimates conditioned on the input series and
  each model's assumptions (stationarity for ARIMA, additive seasonality for
  the Prophet-style model, linear lag structure for VAR). None of them is a
  guarantee of future behavior; validate against a holdout set with the
  metrics in `metrics.py` before relying on a forecast.
- Changepoint detection and decomposition are diagnostic tools, not causal
  explanations: a detected changepoint marks a statistical shift in mean,
  not a confirmed real-world event.
- The optional pandas/scikit-learn/matplotlib/GUI layers inherit the
  maturity and advisories of those packages; the guarantees above describe
  the numpy/scipy-only core.
