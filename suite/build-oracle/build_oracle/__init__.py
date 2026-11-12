"""
Build Oracle — Time Series Forecasting & Anomaly Detection

Modules:
    tensor       - N-dimensional tensor operations
    arima        - ARIMA/SARIMA models
    prophet      - Prophet-style decomposition (Fourier + trend + seasonality)
    ensemble     - Dynamic multi-model ensemble (ARIMA + Prophet + Neural)
    streaming    - Incremental/streaming forecast updates without full refit
    changepoint  - PELT changepoint detection
    decompose    - Time series decomposition (classical, STL-like)
    features     - Feature engineering (statistical, temporal, frequency)
    neural       - Neural network layers (Linear, LSTM, Attention)
    autodiff     - Automatic differentiation (dual numbers)
    metrics      - Forecast evaluation (MAE, RMSE, MAPE, MASE)
    optimize     - Bayesian optimization with Gaussian Processes
"""

__version__ = "1.0.1"
