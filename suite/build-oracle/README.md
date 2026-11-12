<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/build-oracle/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/build-oracle/main/docs/art/hero-light.svg" alt="build-oracle: Python time-series forecasting with ARIMA and changepoint detection. 6 wavering traces run from the left and narrow into a bright core over a row of tick marks." width="100%">
</picture>

# build-oracle

Python time-series forecasting with ARIMA and changepoint detection.

```
pip install .
```

[![version: 1.0.0](https://img.shields.io/badge/version-1.0.0-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/build-oracle/releases/latest)
[![CI](https://github.com/HarperZ9/build-oracle/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/build-oracle/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-FSL--1.1--MIT-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/build-oracle/blob/main/LICENSE)
![python 3.10+](https://img.shields.io/badge/python-3.10%2B-e6e1d6?style=flat-square&labelColor=1a1712)

[Project Telos](https://harperz9.github.io) | [gather](https://github.com/HarperZ9/gather) | [crucible](https://github.com/HarperZ9/crucible) | [index](https://github.com/HarperZ9/index) | [forum](https://github.com/HarperZ9/forum) | [telos](https://github.com/HarperZ9/telos) | [emet](https://github.com/HarperZ9/emet) | [buildlang](https://github.com/HarperZ9/buildlang)

Time series forecasting and anomaly detection toolkit.

## Features

- **ARIMA**: Auto-regressive integrated moving average with automatic order selection
- **Prophet-style**: Exponential smoothing with trend, seasonality, and holiday decomposition
- **Neural Networks**: Feedforward and recurrent architectures for non-linear forecasting
- **Changepoint Detection**: BIC/AIC penalty-based structural break identification
- **Decomposition**: Seasonal-trend decomposition (STL-style) with configurable period
- **Feature Engineering**: Lag features, rolling statistics, Fourier terms

## Installation

```bash
# Core (numpy + scipy only)
pip install .

# With all optional dependencies
pip install ".[all]"
```

## Quick Start

### CLI

```bash
# Forecast with ARIMA (built-in sample data)
build-oracle forecast --data sample --model arima --horizon 30

# Decompose a time series
build-oracle decompose --data sample --period 7

# Detect changepoints
build-oracle changepoints --data sample --penalty bic

# Extract features
build-oracle features --data sample

# Launch GUI
build-oracle gui
```

### Python API

```python
from build_oracle.arima import ARIMAModel

model = ARIMAModel(order=(2, 1, 1))
model.fit(training_data)
forecast = model.predict(horizon=30)
```

## Supported Models

| Model | Use Case |
|---|---|
| ARIMA | Stationary/near-stationary univariate series |
| Prophet-style | Series with strong seasonality and holidays |
| Neural Net | Complex non-linear patterns |
| Changepoint | Detecting regime shifts in data |

## Requirements

- Python >= 3.10
- numpy >= 1.24
- scipy >= 1.10
- Optional: pandas, scikit-learn, matplotlib, PyQt6

## License

Build Oracle is released under the FSL-1.1-MIT
(see [LICENSE](LICENSE)). The source is available: you may read, run, modify,
and build on it for any purpose other than a competing commercial use.
Commercial use that competes with the project is reserved to the Licensor and
requires a separate commercial license.

---

Built by **[Zain Dana Harper](https://harperz9.github.io)** in Seattle: evidence-first tools that leave a re-checkable artifact behind. The full workbench is at [Project Telos](https://harperz9.github.io).
