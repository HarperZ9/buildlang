# Build Finance Enterprise Readiness

Build Finance is the algorithmic trading engine of the build/Project Telos family: a
dependency-light, deterministic toolkit for indicators, strategy signals, backtesting,
risk measurement, and portfolio optimization, with an optional paper/live broker bridge.
It is designed to be used alone as a library or CLI, and as a component other flagships
depend on.

## Risk posture (read this first)

Build Finance is money-adjacent software. Before any other capability claim:

- **API keys are user-supplied, never stored in code or logs.** Credentials are passed
  in by the caller at runtime and are the caller's responsibility to secure.
- **Paper trading is the default.** Live broker execution (Alpaca) requires an explicit,
  deliberate opt-in (`paper_trading=False` plus live credentials); nothing in the
  library escalates a session to live trading on its own.
- **Backtests do not guarantee live results.** Slippage, commission, latency, partial
  fills, and regime change are real gaps between a historical replay and a live market.
  Treat backtest output as a hypothesis, validate it in paper trading, and only then
  consider live capital, and even then, at your own risk.
- **The library custodies no funds.** All money and positions live at the broker; this
  library only sends instructions to, and reads state from, that broker's API.

See [SECURITY.md](../SECURITY.md) for the full statement, attack surface, and what does
and does not count as a security issue.

## Enterprise role

- Compute technical indicators and turn them into strategy signals with vectorized,
  reproducible numeric routines.
- Backtest strategies against historical data with slippage/commission/market-impact
  modeling, walk-forward optimization, and Monte Carlo trade-order resampling.
- Measure risk (Sharpe, Sortino, drawdown, VaR/CVaR, and more) and optimize portfolios
  (Mean-Variance, Black-Litterman, HRP, Risk Parity) with published-method correctness.
- Bridge the same strategy/sizing code to a broker (paper by default, live behind an
  explicit opt-in) so a validated backtest and a live run share the same logic path.

## Operator surface

- `build-finance` CLI for scriptable backtests, portfolio optimization, and indicator
  computation.
- The importable Python API (`build_finance.indicators`, `.strategies`, `.risk`,
  `.sizing`, `.backtest`, `.portfolio`, `.market_data`, `.broker`, `.autotrader`) for
  embedding in pipelines.
- An optional PyQt6 interface (`pip install ".[gui]"`) with a dashboard, backtest runner,
  auto-trader control panel, portfolio view, market data view, and settings page.

## Reproducibility and provenance

- Indicator, risk, and portfolio-optimization functions are pure functions of their
  inputs: the same input series yields the same output, which makes results reproducible
  and diffable across runs and machines.
- Backtests accept a fixed seed for synthetic data generation, so a reported backtest
  result can be regenerated from the same strategy, config, and data.
- Trade logs and equity curves are the durable artifacts of a backtest run; they can be
  re-derived from the same inputs and diffed run-to-run.

## Dependencies and boundary

- **Runtime core:** `numpy`, `pandas`, `scipy`. Network access is confined to
  `market_data.py` (Yahoo Finance, CoinGecko) and `broker.py` (Alpaca), and only runs
  when explicitly invoked. The indicator/strategy/risk/backtest/portfolio core performs
  no network access on its own.
- **Optional:** `matplotlib` (plotting), `ta-lib` (extra indicators), `PyQt6` (GUI). Each
  is isolated behind an extra so the core stays minimal.
- The GUI and CLI consume the core; they are never a dependency of it.

## Quality gates

- `ruff check .` (style), `mypy` (types: the numeric and trading core is type-clean; the
  GUI adapter is boundary-typed), and `pytest` with coverage run in CI on every push and
  pull request.
- No API token or broker credential is stored in the repository; broker keys are
  supplied by the caller at runtime only.

## Honest limits

- Trading strategies and risk metrics are reference-relative: correctness claims are
  against published formulae and are validated by the test suite. Report a deviation
  with its reference.
- A profitable backtest is not a profitable live strategy. Market data providers (Yahoo
  Finance, CoinGecko) and the broker (Alpaca) are third-party services with their own
  availability, rate limits, and data-quality characteristics that this library does not
  control.
- The optional GUI and plotting layers inherit the maturity and advisories of PyQt6 and
  matplotlib; the guarantees above describe the numpy/pandas/scipy-only core.
