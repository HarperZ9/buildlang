# Build Finance Usage Guide

Build Finance is a Python algorithmic trading toolkit with a command-line
interface (`build-finance`) and an optional PyQt6 GUI. This guide covers
installation, the CLI commands, the Python API, and worked examples.

Command help blocks and CLI output below were run against the package in this
repository. Live-broker and network-backed examples (Alpaca, Yahoo Finance,
CoinGecko) describe expected shapes rather than captured output, since they
depend on external services and credentials; see
[docs/ALPACA_SETUP.md](docs/ALPACA_SETUP.md) for the live setup path.

## Install

### GitHub release wheel

Build Finance 1.0.1 is published as GitHub release assets. PyPI publication is
not documented for 1.0.1, so install from the checked release wheel unless you
are working from source.

```powershell
& {
  $ErrorActionPreference = 'Stop'
  $wheel = 'build_finance-1.0.1-py3-none-any.whl'
  $expected = '6597fbbbc13d26cb63a1c123a929a21e512fefb19d156c9d2786364108b3b515'
  Invoke-WebRequest `
    -Uri "https://github.com/HarperZ9/build-finance/releases/download/v1.0.1/$wheel" `
    -OutFile $wheel
  $actual = (Get-FileHash ".\$wheel" -Algorithm SHA256).Hash.ToLower()
  if ($actual -ne $expected) {
    throw "SHA-256 mismatch for $wheel. Expected $expected, got $actual."
  }
  python -m pip install ".\$wheel"
  build-finance --help
}
```

In other shells, download the wheel, compare its SHA-256 to the expected value
above, and install it only if the digest matches.

Release assets:

- Wheel:
  <https://github.com/HarperZ9/build-finance/releases/download/v1.0.1/build_finance-1.0.1-py3-none-any.whl>
- sdist:
  <https://github.com/HarperZ9/build-finance/releases/download/v1.0.1/build_finance-1.0.1.tar.gz>
- Checksums:
  <https://github.com/HarperZ9/build-finance/releases/download/v1.0.1/SHA256SUMS.txt>

Expected SHA-256 values:

| Asset | SHA-256 |
| --- | --- |
| `build_finance-1.0.1-py3-none-any.whl` | `6597fbbbc13d26cb63a1c123a929a21e512fefb19d156c9d2786364108b3b515` |
| `build_finance-1.0.1.tar.gz` | `95f0212fa40541bae5fbf57cfd57debe2ae0b8d321dcb5399a4b9bb1d90bd468` |

### Source checkout

Clone the repository before using editable installs:

```bash
git clone https://github.com/HarperZ9/build-finance.git
cd build-finance
git checkout v1.0.1

# Core library + CLI (numpy, pandas, scipy)
python -m pip install -e .

# Optional GUI extras from source (+ PyQt6 and build-ui)
python -m pip install -e ".[gui]"
```

Both install routes expose the `build-finance` console script (entry point
`build_finance.cli:main`). Requires Python 3.10+.

Without installing, you can run the CLI directly from a source checkout:

```bash
python -m build_finance.cli <command> ...
```

The install and help commands do not place broker orders. Paper trading is the
default; anything that reaches a live broker requires credentials and explicit
live configuration by the caller.

## CLI

```text
usage: build-finance [-h] {backtest,analyze,optimize,indicators,gui} ...

Algorithmic trading toolkit -- strategies, backtesting, portfolio optimization

positional arguments:
  {backtest,analyze,optimize,indicators,gui}
    backtest            Run a backtest
    analyze             Analyze trade log CSV
    optimize            Portfolio optimization
    indicators          Compute technical indicators
    gui                 Launch GUI (default)

options:
  -h, --help            show this help message and exit
```

| Command | Description |
|---------|-------------|
| `build-finance` | Launch the GUI (default when no command is given) |
| `build-finance backtest` | Run a backtest on sample or CSV candle data |
| `build-finance analyze` | Analyze a completed-trades CSV |
| `build-finance optimize` | Portfolio optimization over a returns CSV |
| `build-finance indicators` | Compute technical indicators on sample data |
| `build-finance gui` | Launch the GUI explicitly |

### `backtest` options

| Flag | Default | Meaning |
|------|---------|---------|
| `--strategy` | `momentum` | Strategy name (`momentum`, `mean_reversion`) |
| `--data` | `sample` | `sample` or a path to a candle CSV |
| `--symbol` | `AAPL` | Symbol used when generating sample data |
| `--days` | `252` | Trading days of generated sample data |
| `--capital` | `100000` | Initial capital |
| `--monte-carlo` | off | Run a 1000-simulation Monte Carlo pass over the resulting trades |

### `analyze` options

| Flag | Default | Meaning |
|------|---------|---------|
| `--file` | required | Path to a trades CSV (`symbol,side,entry_price,exit_price,quantity,entry_time,exit_time,commission,pnl`) |

### `optimize` options

| Flag | Default | Meaning |
|------|---------|---------|
| `--file` | required | Path to a returns CSV (rows = dates, columns = assets) |
| `--method` | `max_sharpe` | `max_sharpe`, `min_variance`, `max_return`, `risk_parity`, `hrp` |
| `--risk-free` | `0.02` | Risk-free rate used in Sharpe/optimization math |

### `indicators` options

| Flag | Default | Meaning |
|------|---------|---------|
| `--symbol` | `AAPL` | Symbol used when generating sample data |
| `--indicator` | `rsi,sma` | Comma-separated: `rsi`, `sma`, `ema`, `macd` |

## Worked examples (CLI)

### 1. Compute indicators on generated sample data

```bash
build-finance indicators --symbol AAPL --indicator rsi,sma
```

```text
RSI(14) last 5: [71.35, 66.93, 73.62, 66.4, 69.2]
SMA(14) last 5: [136.35, 136.95, 137.94, 138.64, 139.53]
```

### 2. Backtest a strategy on sample data

```bash
build-finance backtest --strategy momentum --days 100
```

```text
=== Backtest Results ===
Initial capital : $  100,000.00
Final equity    : $  100,000.00
Total return    :        0.00%
...
Trades          :            0
```

The CLI's built-in `momentum` demo strategy is a simple fast/slow moving-average
crossover (10/30) and is intentionally minimal. On a given seeded sample it may
produce zero crossovers, as above. It exists to exercise the `Backtester` wiring
end to end; the `build_finance.strategies` module has the five full strategies
(Momentum, Mean Reversion, Trend Following, Breakout, Ensemble) used by the
Python API and GUI.

### 3. Analyze a trade log

```bash
build-finance analyze --file trades.csv
```

Prints trade count, total/average P&L, win rate, average win/loss, and best/worst
trade computed from the CSV's `pnl` column.

### 4. Optimize a portfolio

```bash
build-finance optimize --file returns.csv --method max_sharpe
```

Prints expected return, volatility, Sharpe ratio, and per-asset weights for the
selected method (`max_sharpe`, `min_variance`, `max_return` via
`mean_variance_optimize`; `risk_parity` via `risk_parity_weights`; `hrp` via
`hierarchical_risk_parity`).

## Python API

```python
from build_finance.indicators import rsi, macd
from build_finance.strategies import MomentumStrategy
from build_finance.backtest import BacktestConfig, Backtester, generate_sample_data
from build_finance.risk import sharpe_ratio, max_drawdown
from build_finance.portfolio import mean_variance_optimize, portfolio_stats

candles = generate_sample_data(symbol="AAPL", days=252, seed=42)

strategy = MomentumStrategy()
cfg = BacktestConfig(initial_capital=100_000.0)
result = Backtester(cfg).run(strategy, {"AAPL": candles})
print(result.summary())
```

### Paper and live broker access

```python
from build_finance.broker import PaperBroker, AlpacaBroker, BrokerConfig

# Paper trading -- no credentials, no network, no real money.
paper = PaperBroker(BrokerConfig(name="paper", paper_trading=True))

# Alpaca -- paper by default; requires API key/secret supplied by the caller.
# See docs/ALPACA_SETUP.md. Live execution requires paper_trading=False
# explicitly; nothing in this library flips that on its own.
alpaca = AlpacaBroker(
    BrokerConfig(
        name="alpaca",
        api_key="<your-key>",
        api_secret="<your-secret>",
        paper_trading=True,
    )
)
```

See [SECURITY.md](SECURITY.md) for the full credential-handling and
paper/live-trading posture before connecting a real broker account.

### Automated trading loop

```python
from build_finance.autotrader import AutoTrader, AutoTraderConfig

trader = AutoTrader(AutoTraderConfig(symbols=["AAPL", "BTC-USD"]))
# trader.run(...) drives strategy -> sizing -> broker on an interval;
# see build_finance/autotrader.py for the full configuration surface.
```

## GUI

```bash
build-finance gui
```

The GUI requires PyQt6. From a source checkout, install the optional GUI extras
with `python -m pip install -e ".[gui]"`. The GUI provides a Dashboard,
Backtest runner, Auto-Trader control panel, Portfolio view, Market Data view,
and Settings page (broker API keys, default parameters). If the GUI import
fails, the CLI reports that the GUI is unavailable rather than crashing.

## See also

- `README.md`: project overview and feature list.
- `SECURITY.md`: the paper/live trading boundary, credential handling, and
  what does not custody funds.
- `ARCHITECTURE.md`: module layering and the strategy/sizing/broker data flow.
- `docs/ALPACA_SETUP.md`: step-by-step Alpaca paper (and live) account setup.
- `docs/ENTERPRISE-READINESS.md`: risk posture, reproducibility, and quality gates.
