# Changelog

## v1.1.0 (2026-08-22)

### Features
- **Deterministic offline paper kernel** (`build_finance/live_paper`): admission, grouping, causal feature snapshots, fusion, fail-closed risk authority, mandatory exits, sizing, and causally simulated terminal fills
- **Hash-chained paper accounting** with reconciliation invariants and canonical frozen-event comparison
- **Model-inference abstain boundary**: model signals are disabled by design; the kernel runs evidence-only
- **Crypto replay engine** (`build_finance/crypto_replay`): deterministic replay of historical crypto fills with schema-locked contracts
- **Offline artifact gate**: paper-core wheel/sdist are built from a reviewed source allowlist; the gate verifies archive closure, RECORD digests, AST import closure (no network, subprocess, or environment access), network-denied test execution, ruff, mypy, and git hygiene
- **Operator authorization records**: publication stays `BLOCKED` unless an authorization node bound to the sealed implementation SHA is recorded at capture time

### Boundary
- Paper-only. No live order placement, no external data, no model inference.
- No profitability claim is made or implied.

### Tests
- Full prescribed G2 gate green on the release candidate

## v1.0.0 (2026-03-22)

### Features
- **10 technical indicators**: SMA, EMA, MACD, RSI, Bollinger Bands, Stochastic, ATR, OBV, VWAP, ADX
- **5 trading strategies**: momentum, mean reversion, breakout, pairs, multi-factor
- **Event-driven backtesting** with slippage, commission, and market impact simulation
- **Order book simulation** with limit/market/stop orders
- **Portfolio optimization**: Mean-Variance, Black-Litterman, HRP, Risk Parity
- **14 risk metrics**: Sharpe, Sortino, Calmar, VaR, CVaR, max drawdown, beta, alpha, information ratio, profit factor, win rate, volatility
- **Monte Carlo simulation** for risk analysis
- **Alpaca live trading** integration
- **PyQt6 GUI**: backtesting, portfolio optimization, market data, autotrader pages
- **CLI**: backtest, analyze, optimize, indicators commands

### Tests
- 114 tests passing
