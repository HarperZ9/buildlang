# Contributing

Build Oracle is a public time-series forecasting package. Contributions
should preserve numerical behavior, keep optional GUI dependencies optional,
and include focused tests for forecasting models, changepoint detection,
decomposition, feature engineering, or evaluation metrics.

## Local Setup

```powershell
python -m pip install -e ".[all]"
python -m pip install pytest ruff mypy
```

## Verification

For release-boundary and documentation changes:

```powershell
git diff --check
```

For core package changes:

```powershell
python -m pytest tests/test_arima.py tests/test_multivariate.py tests/test_ensemble.py -q
python -m pytest tests/test_streaming.py tests/test_changepoint.py tests/test_decompose.py -q
```

For broader changes, run the full test suite:

```powershell
python -m pytest tests/ -q
```

Do not commit `.env` files, saved model artifacts, proprietary series data,
credentials, or local-only build artifacts.
