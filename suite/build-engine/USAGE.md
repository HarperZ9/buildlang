# Build Engine — Usage Guide

Build Engine is a Python adaptive prediction and paper-trading engine with a
command-line interface (`build-engine`) and an optional PyQt6 GUI dashboard. This
guide covers installation, the CLI commands, the Python API, and worked examples.

Command help blocks below were run against the package in this repository. The
`run`/`backtest` walkthroughs describe expected shapes since they depend on the
`build-oracle` and `build-finance` sibling packages, live or generated market data,
and non-deterministic timing; the CLI `--help` and `status` output are real runs.

## Install

```bash
# Build Engine depends on the build-oracle and build-finance sibling packages for
# forecasting models and execution primitives. Install all three from a workspace
# checkout:
pip install -e ../build-oracle
pip install -e ../build-finance
pip install -e .

# Or, with the dev/test extras for this repo alone:
pip install -e ".[dev]"
```

This installs the `build-engine` console script (entry point
`build_engine.cli:main`). Requires Python 3.10+.

Without installing, you can run the CLI directly from a checkout:

```bash
python -m build_engine.cli <command> ...
```

## CLI

```text
usage: build-engine [-h] [-v] {run,backtest,status,gui} ...

Self-improving prediction and trading engine

positional arguments:
  {run,backtest,status,gui}
    run                 Run the adaptive engine
    backtest            Backtest the prediction strategy
    status              Show engine status
    gui                 Launch the GUI

options:
  -h, --help            show this help message and exit
  -v, --verbose         Enable debug logging
```

| Command | Description |
|---------|-------------|
| `build-engine` | Launch the GUI (default when no command is given) |
| `build-engine gui` | Launch the GUI dashboard |
| `build-engine run` | Run the adaptive engine (paper by default) |
| `build-engine backtest` | Backtest the prediction strategy on generated sample data |
| `build-engine status` | Show engine status |

### `run` options

| Flag | Default | Meaning |
|------|---------|---------|
| `--symbols` | `AAPL,BTC-USD` | Comma-separated ticker symbols |
| `--models` | `arima,prophet` | Comma-separated oracle model names |
| `--paper` | on | Use paper trading (default) |
| `--live` | off | Use live trading — overrides `--paper`; see `SECURITY.md` for the required gate |
| `--live-ack` | `""` | Live-mode acknowledgement value; also read from `BUILD_ENGINE_LIVE_ACK` |
| `--cycles` | unlimited | Max cycles to run |
| `--interval` | `300` | Seconds between cycles |
| `--risk` | `0.02` | Risk per trade as a fraction of equity |
| `--max-positions` | `5` | Maximum simultaneous open positions |
| `--horizon` | `5` | Forecast horizon in steps |

### `backtest` options

| Flag | Default | Meaning |
|------|---------|---------|
| `--symbols` | `AAPL` | Comma-separated ticker symbols |
| `--models` | `arima,prophet` | Comma-separated oracle model names |
| `--days` | `252` | Number of trading days of generated sample data |
| `--capital` | `100000` | Initial capital |
| `--horizon` | `5` | Forecast horizon in steps |
| `--monte-carlo` | off | Run a 1000-simulation Monte Carlo pass after the backtest |

## Worked examples (CLI)

### 1. Check status

```bash
build-engine status
```

```text
Build Engine status
  No running engine detected.
  Use 'build-engine run' to start a new session.
```

### 2. Paper-trade a session

```bash
build-engine run --symbols AAPL,BTC-USD --paper --cycles 10
```

Prints the starting configuration, then a final status block with cycles completed,
equity, open positions, total trades, model accuracy, and per-model weights.

### 3. Backtest with Monte Carlo

```bash
build-engine backtest --symbols AAPL --days 252 --monte-carlo
```

Generates deterministic synthetic candles (fixed seed), runs the prediction strategy
through `build_finance.backtest.Backtester`, prints the backtest summary, then a
1000-simulation Monte Carlo pass over the resulting trades (median/5th/95th
percentile return, median drawdown).

### 4. Live mode (explicit opt-in — see `SECURITY.md`)

```powershell
$env:BUILD_ENGINE_LIVE_ACK="I_UNDERSTAND_LIVE_RISK"
$env:APCA_API_KEY_ID="<paper-or-live-key-id>"
$env:APCA_API_SECRET_KEY="<paper-or-live-secret>"
build-engine run --symbols AAPL --live --live-ack I_UNDERSTAND_LIVE_RISK --cycles 1
```

Without both the acknowledgement and the credentials, `AdaptiveEngine` raises
`ValueError` before constructing a live broker.

## Python API

```python
from build_engine.adaptive_engine import AdaptiveEngine
from build_engine.config import EngineConfig

config = EngineConfig(
    symbols=["AAPL", "BTC-USD"],
    models=["arima", "prophet"],
    paper_trading=True,
)

engine = AdaptiveEngine(config)
result = engine.run_cycle()
print(f"Equity: ${result['equity']:,.2f}")
print(f"Accuracy: {result['accuracy']:.0%}")
```

`EngineConfig` is a dataclass; see `build_engine/config.py` for the full field list
(trading, prediction, and feedback-loop knobs) and defaults.

`AdaptiveEngine.run_cycle()` returns a dict with keys `cycle`, `actions`, `equity`,
`cash`, `positions`, `model_weights`, and `accuracy`. `AdaptiveEngine.get_status()`
returns a comprehensive snapshot with keys `running`, `cycles`, `equity`,
`positions`, `trades`, `models`, `accuracy`, and `model_weights`.

### Persisting engine state

```python
from build_engine.persistence import EngineState

state = EngineState()  # defaults to ~/.build-engine/
state.save(engine)
```

`EngineState` writes `state.json` (weights, accuracy history, config snapshot,
timestamp) and `trades.jsonl` (append-only trade history). It never writes broker
credentials or the live-mode acknowledgement value.

### Alerts / regime detection

```python
from build_engine.alerts import AlertManager

alerts = AlertManager()
alerts.check_drawdown(equity_curve)
```

See `build_engine/alerts.py` for the full detector set: trend reversals, volatility
spikes, accuracy drops, weight shifts, and drawdowns, delivered via callback, JSONL
log, or an optional webhook.

## GUI

```bash
build-engine gui
```

The GUI requires PyQt6 and the `build-ui` shared theme/widget package. It provides a
Dashboard, Trading, Engine Control, Performance, Backtest, Market Data, and Settings
page (see `build_engine/gui/pages/`). If the GUI import fails, the CLI reports that
the GUI is unavailable rather than crashing.

## See also

- `README.md` — project overview and feature list.
- `SECURITY.md` — the paper/live trading boundary and what does not custody funds.
- `ARCHITECTURE.md` — the feedback-loop data flow and module layering.
