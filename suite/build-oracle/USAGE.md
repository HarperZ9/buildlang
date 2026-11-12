# Build Oracle Usage Guide

Build Oracle is a Python time-series forecasting library with a
command-line interface (`build-oracle`) and an optional PyQt6 GUI. This
guide covers installation, the CLI commands, the Python API, and worked
examples with their actual output.

All command and API examples below were run against the package in this
repository. Output blocks reflect real runs unless explicitly marked
*illustrative*.

## Install

```bash
# Core library + CLI (numpy + scipy)
pip install .

# Everything, including pandas/scikit-learn/matplotlib/PyQt6
pip install ".[all]"

# Just the GUI extras
pip install ".[gui]"
```

This installs the `build-oracle` console script (entry point
`build_oracle.cli:main`). Requires Python 3.10+.

Without installing, you can run the CLI directly from a checkout:

```bash
python -m build_oracle.cli <command> ...
```

## CLI

```text
build-oracle <command> [options]
```

| Command | Description |
|---------|-------------|
| `build-oracle` | Launch the GUI (default when no command is given) |
| `build-oracle gui` | Launch the GUI workbench |
| `build-oracle forecast --data sample --model arima --horizon 30` | Fit a model and forecast |
| `build-oracle decompose --data sample --period 7` | Trend/seasonal/residual decomposition |
| `build-oracle changepoints --data sample --penalty bic` | PELT changepoint detection |
| `build-oracle features --data sample` | Statistical/temporal/frequency feature extraction |

Option vocabularies (from the source):

- `forecast --model`: `arima`, `prophet`, `ensemble` (default `arima`); `--save PATH` /
  `--load PATH` persist or reuse a fitted model; `--multivariate` runs a VAR model on a
  2-column sample series instead
- `decompose --model`: `additive`, `multiplicative` (default `additive`)
- `changepoints --penalty`: `bic` (default), `aic`, `mbic`
- `--data` currently accepts `sample`, which generates a synthetic series with trend,
  seasonality, noise, and injected changepoints (see `generate_sample_series` in `cli.py`)

## Worked examples (CLI)

### 1. Forecast with ARIMA

```bash
build-oracle forecast --data sample --model arima --horizon 5
```

```text
  Build Oracle v1.0.0
  ========================================

  Data source : sample (365 points)
  Model       : ARIMA
  Horizon     : 5 steps

  --- Metrics ---
  MAE  : 0.4888
  RMSE : 0.5268
  MAPE : 4.56%

  --- Forecast (first 5 / last 5) ---
    t+  1: 12.4352
    t+  2: 11.3181
    t+  3: 10.7694
    t+  4: 11.1206
    t+  5: 11.9671
```

### 2. Decompose the sample series

```bash
build-oracle decompose --data sample --period 7
```

```text
  Data source : sample (365 points)
  Period      : 7
  Model       : additive

  --- Component Strengths ---
  Trend      : 0.9428
  Seasonal   : 0.9860

  Trend range    : [9.32, 11.65]
  Seasonal range : [-1.47, 1.50]
  Residual std   : 0.1280
```

### 3. Detect changepoints

```bash
build-oracle changepoints --data sample --penalty bic
```

```text
  Data source : sample (365 points)
  Penalty     : BIC

  Detected 4 changepoint(s):

     Index  Confidence   Left Mean  Right Mean
     -----  ----------   ---------  ----------
        56      0.3885     10.2641     10.7930
       108      0.8581     10.9175      9.7494
       259      1.0000      9.9124     11.3288
       322      0.4566     11.3532     11.9747
```

### 4. Extract features

```bash
build-oracle features --data sample
```

```text
  --- Statistical Features ---
  Mean            : 10.5683
  Std Dev         : 1.1985
  Min             : 7.8838
  Max             : 13.2126
  Skewness        : 0.0120
  Kurtosis        : -0.9680

  --- Temporal Features ---
  Trend slope     : 0.001075
  Mean |diff|     : 0.8520
  Zero-cross rate : 0.2857
  Autocorr lag-1  : 0.6847
  Autocorr lag-7  : 0.8844

  --- Frequency Features ---
  Dominant freq   : 52 (period ~ 7)
  Spectral energy : 95678.47
  Spectral entropy: 1.7805
```

## Python API

```python
import numpy as np
from build_oracle.arima import ARIMA
from build_oracle.changepoint import pelt
from build_oracle.metrics import mae, rmse

# Fit and forecast with ARIMA(p=2, d=1, q=1)
series = 10 + 0.02 * np.arange(200) + 2 * np.sin(2 * np.pi * np.arange(200) / 12)
model = ARIMA(p=2, d=1, q=1)
model.fit(series)
forecast = model.predict(horizon=5)
# -> array([13.3552, 12.964 , 12.8416, 12.7322, 12.6935])

# PELT changepoint detection
changepoints = pelt(series, penalty="bic")
# -> list[int] of 0-based indices marking the start of each new segment

# Evaluation metrics
score = mae([1, 2, 3], [1.1, 2.2, 2.7])
# -> 0.2
```

`ARIMA(p, d, q)` takes AR order, differencing order, and MA order.
`auto_arima(series, max_p=5, max_d=2, max_q=5)` grid-searches over that space
by AIC and returns the fitted best model. `VAR(p)` fits a p-lag vector
autoregression on a 2-D `(n_obs, n_series)` array. `Prophet` and
`SimpleForecaster` (neural) follow the same `fit(series)` / `predict(horizon)`
shape. `EnsembleForecaster` wraps any combination of these behind
`EnsembleConfig` and combines their predictions with accuracy-based weights.
`metrics.py` exposes `mae`, `mse`, `rmse`, `mape`, `smape`, `mase`, and
`r_squared`, all with the signature `(actual, predicted) -> float`.

### Streaming updates

```python
from build_oracle.streaming import StreamConfig, StreamForecaster

config = StreamConfig()  # selects the base model and window size
stream = StreamForecaster(config)
# stream.update(new_point) -> StreamUpdate with the incremental prediction
```

`StreamForecaster` avoids a full refit on every new observation: it applies a
model-specific incremental update (state update for ARIMA, trend-only refit
for Prophet, online gradient step for the neural model, or reweighting for an
ensemble) and returns a `StreamUpdate` per call.

## GUI

```bash
build-oracle gui
```

The GUI requires the `gui` (or `all`) extra so that PyQt6 is available. It
provides a Dashboard, Forecast page, Decompose page, Changepoint page, and a
Settings page (see `build_oracle/gui/pages/`).

## See also

- `README.md`: project overview and feature list.
- `ARCHITECTURE.md`: module layout and data flow.
