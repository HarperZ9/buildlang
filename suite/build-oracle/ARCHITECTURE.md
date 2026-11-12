# Architecture

Build Oracle is a single-package, dependency-light time-series forecasting
workbench. The only required runtime dependencies are `numpy` and `scipy`;
pandas, scikit-learn, matplotlib, and PyQt6 are optional and gate specific
features. The public API is a flat set of focused modules under
`build_oracle/`, each owning one well-bounded area of forecasting, plus a
thin CLI and an optional GUI.

## Layers

```
build_oracle/
  arima.py          ARIMA model: Yule-Walker/Levinson-Durbin AR fitting, residual-based MA
                     estimation, auto_arima grid search
  var.py            VAR(p): vector autoregression for multivariate series, OLS on the
                     stacked lag matrix
  prophet.py         Prophet-style additive model: piecewise-linear trend with automatic
                     changepoints + Fourier-series seasonality
  neural.py          NumPy-only neural network layers (Linear, LayerNorm, ReLU/Sigmoid/Tanh,
                     LSTMCell) and a lightweight MLP forecaster (SimpleForecaster)
  ensemble.py        EnsembleForecaster: dynamic multi-model combination (ARIMA + Prophet +
                     Neural) with accuracy-based weighting
  streaming.py       StreamForecaster: incremental forecast updates without a full refit
                     (per-model update strategies for ARIMA, Prophet, Neural, Ensemble)
  changepoint.py     PELT (Pruned Exact Linear Time) changepoint detection with BIC/AIC/mBIC
                     penalties
  decompose.py       Classical time-series decomposition (trend/seasonal/residual) via
                     centered moving averages and period averaging
  features.py        Feature engineering: statistical, temporal, rolling, and lag features;
                     differencing, log, and Box-Cox transforms
  metrics.py         Forecast evaluation metrics (MAE, MSE, RMSE, MAPE, SMAPE, MASE, R^2)
  autodiff.py        Forward-mode automatic differentiation via dual numbers (Dual class)
  cli.py             Command-line entry point (`build-oracle`)
  gui/               Optional PyQt6 workbench (thin adapter over the core; not required)
```

## Model registry

There is no single abstract base class uniting the forecasters. ARIMA,
`Prophet`, `SimpleForecaster` (neural), and `VAR` each expose a `fit`/`predict`
pair with a model-specific signature, and `EnsembleForecaster` treats them
heterogeneously through a narrow structural `Protocol` (`_SubModel` in
`ensemble.py`) that only pins down the common calling shape it actually uses.
This keeps each model's interface honest to what it needs (VAR predicts a
matrix of K series, ARIMA and Prophet predict a single series) instead of
forcing a lowest-common-denominator abstraction.

## Data flow

The library is functional at its core: a series flows in as a 1-D (or, for
VAR, 2-D) numpy array, is optionally reshaped by `features.py` or
`decompose.py`, fitted by a model module, and produces a forecast plus
diagnostics.

```
raw series (list / numpy array)
  -> features / decompose (optional: engineer inputs, inspect structure)
  -> model.fit(...)                  (arima / var / prophet / neural / ensemble)
  -> model.predict(horizon)          -> forecast array
  -> metrics (mae / rmse / mape / ...)  (compare forecast to holdout)
  -> streaming.StreamForecaster       (optional: incremental updates as new points arrive)
```

Each module takes and returns numpy arrays (or small dataclasses wrapping
them) in documented shapes, so modules compose without shared mutable state.
`changepoint.py` and `decompose.py` are read-only diagnostics over a series, and
they do not feed back into model fitting automatically; callers wire that up
explicitly (e.g. segmenting a series at a detected changepoint before
fitting). The GUI and CLI are consumers of this core, never a dependency of
it.

## The streaming engine

`streaming.StreamForecaster` exists because refitting ARIMA, Prophet, or a
neural net from scratch on every new observation is wasteful for live data.
Instead, `StreamConfig` selects a base model type and `StreamForecaster`
applies a model-specific incremental update on each new point: a
Kalman-filter-style state update for ARIMA, a trend-only refit (seasonality
held fixed) for Prophet, an online gradient step for the neural model, and
accuracy-based reweighting for an ensemble. Each update returns a
`StreamUpdate` recording the new prediction and any drift/error signal, so a
caller can observe convergence and reactivity without re-running `fit` on the
full history.

## Design decisions

- **numpy/scipy core.** Everything needed for the forecasting math
  (autoregression, Fourier features, changepoint search, evaluation metrics)
  is expressed with numpy and scipy. pandas, scikit-learn, matplotlib, and
  PyQt6 are optional and isolated behind extras so the core stays installable
  and importable anywhere.
- **Flat, single-purpose modules.** Each file answers one question ("how do
  I fit an ARIMA model?", "where did the series structurally change?",
  "how good was this forecast?"), which keeps the public surface legible and
  each unit independently testable.
- **Heterogeneous model registry via Protocol, not inheritance.** Forecasters
  differ enough in shape (univariate vs. multivariate, batch vs. streaming)
  that a shared base class would either be too thin to help or too wide to
  respect. `ensemble.py` and `streaming.py` depend on the minimal structural
  interface they actually call.
- **Type-clean core, boundary-typed GUI.** The numeric core is fully
  type-checked (`mypy` clean). The PyQt6 GUI is a thin adapter over an
  untyped Qt binding and is checked at its public boundary rather than
  strict-typed internally (see the `mypy` overrides in `pyproject.toml`).
- **Deterministic and offline.** Fitting and prediction are pure functions of
  their inputs (and, where used, an explicit random seed). The library
  performs no network access and no code evaluation.

## Testing

The suite under `tests/` covers each model (`test_arima.py`,
`test_multivariate.py` for VAR, `test_neural.py`, `test_ensemble.py`,
`test_streaming.py`), the diagnostics (`test_changepoint.py`,
`test_decompose.py`), evaluation (`test_metrics.py`), and save/load behavior
(`test_persistence.py`). Run `pytest` for the full suite; `ruff check .` and
`mypy` gate style and types.
