# Architecture

Build Finance is a single-package, dependency-light algorithmic trading toolkit. The
required runtime dependencies are `numpy`, `pandas`, and `scipy`; PyQt6, matplotlib, and
ta-lib are optional and gate specific features. The public API is a flat set of
focused modules under `build_finance/`, each owning one well-bounded area of trading
(data, indicators, strategy, risk, execution, optimization), plus a thin CLI and an
optional GUI.

## Layers

```
build_finance/
  data.py          Market data structures (Candle, Quote, Signal, Trade, Position) shared across the package
  indicators.py     10 vectorized technical indicators (SMA, EMA, RSI, MACD, Bollinger, ATR, Stochastic, VWAP, ADX, OBV)
  strategies.py     5 trading strategies (Momentum, Mean Reversion, Trend Following, Breakout, Ensemble)
  risk.py           14 risk metrics (Sharpe, Sortino, Max Drawdown, Calmar, VaR, CVaR, Beta, Alpha, ...)
  sizing.py         Position sizing methods (fixed, Kelly, ATR-based, percent-of-equity)
  orderbook.py      Order execution simulation (slippage, commission, market impact)
  backtest.py       Backtesting engine (event-driven simulation, walk-forward optimization, Monte Carlo)
  portfolio.py      Portfolio optimization (Mean-Variance/Markowitz, Black-Litterman, HRP, Risk Parity)
  market_data.py    Market data sources (Yahoo Finance, CoinGecko) and CSV import/export
  broker.py         Broker abstraction: paper trading (simulated) and Alpaca (paper + live)
  autotrader.py     Automated trading engine that wires strategy + broker + risk together
  cli.py            Command-line entry point (`build-finance`)
  gui/              Optional PyQt6 interface (thin adapter over the core; not required)
```

## Data flow

The core is functional wherever possible: pure numeric transforms operate on
`Candle`/`Quote` series and numpy arrays. A typical path is

```
market data (Candle series, via market_data or a broker)
  -> indicators (derive signals: SMA/RSI/MACD/...)
  -> strategies (turn indicators into buy/sell/hold Signal objects)
  -> sizing (turn a Signal into a position size)
  -> orderbook / broker (execute: simulate fills, slippage, commission -- or place a real order)
  -> backtest / autotrader (drive the loop: historical replay, or live polling)
  -> risk / portfolio (measure results: Sharpe, drawdown, optimal weights)
```

`backtest.py` replays historical `Candle` data through this same pipeline offline;
`autotrader.py` drives it live against a `broker.py` backend. Both consume the same
strategy and sizing code, so a strategy validated in backtest runs unchanged live. The
GUI and CLI are consumers of this core, never a dependency of it.

## Design decisions

- **numpy/pandas/scipy core.** Indicators, risk metrics, and portfolio optimization are
  vectorized numeric routines. matplotlib, ta-lib, and PyQt6 are optional and isolated
  behind extras so the core stays installable without a GUI toolkit or plotting stack.
- **Flat, single-purpose modules.** Each file answers one question ("what happened in the
  market?", "what should I do about it?", "how did that perform?"), which keeps the
  public surface legible and each unit independently testable.
- **Broker is an abstraction, not a hard dependency.** `broker.py` defines a paper-trading
  backend and an Alpaca backend behind the same interface. The rest of the package
  (backtest, autotrader, strategies) is broker-agnostic; swapping brokers does not
  change strategy or risk code. See [SECURITY.md](SECURITY.md) for the live-trading
  posture and credential handling.
- **Type-clean core, boundary-typed GUI.** The numeric and trading core is fully
  type-checked (`mypy` clean). The PyQt6 GUI is a thin adapter over an untyped Qt
  binding and is checked at its public boundary rather than strict-typed internally.
- **Backtests are estimates, not guarantees.** The backtesting engine models slippage,
  commission, and market impact, but a historical replay cannot capture every
  live-market effect (partial fills, latency, liquidity shocks). Treat backtest results
  as a lower bound on uncertainty, not a promise of live performance.

## Testing

The suite under `tests/` covers indicator correctness, strategy signal generation,
backtest engine behavior (including walk-forward and Monte Carlo paths), risk metric
correctness against known formulae, and market-data/CSV I/O round-trips. Run `pytest`
for the full suite; `ruff check .` and `mypy` gate style and types.
