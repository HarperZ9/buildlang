# Architecture

Build Engine is a single-package adaptive prediction and paper-trading engine. Its
only required runtime dependencies are `numpy` and `scipy`; the engine's forecasting
and execution capability comes from two sibling packages, `build-oracle` (forecasting
models) and `build-finance` (broker, market data, and backtesting primitives), which
are imported lazily at the call sites that need them rather than required at import
time of the top-level package.

## Layers

```
build_engine/
  config.py                Central EngineConfig dataclass + live-mode acknowledgement constant
  model_trainer.py          Wraps build-oracle model fitting/prediction (ARIMA, Prophet, neural)
  prediction_strategy.py    Bridges oracle forecasts to build-finance Signal objects
  performance_tracker.py    Tracks per-model directional accuracy, computes ensemble weights
  adaptive_engine.py         Wires training, prediction, broker setup, execution, and feedback
  persistence.py            Saves/loads non-secret engine state and trade history (~/.build-engine/)
  alerts.py                 Regime-change detection (trend reversal, vol spike, accuracy drop, drawdown)
  cli.py                    Command-line entry point (`build-engine`)
  gui/                      Optional PyQt6 dashboard (thin adapter over the core; not required)
```

## Data flow

The feedback loop is the core design:

```
market data
  -> model_trainer   (fit/refresh forecasting models per symbol)
  -> prediction_strategy (ensemble forecasts -> directional trading signals)
  -> adaptive_engine  (execute signals through a build-finance broker)
  -> performance_tracker (score prediction accuracy against realized outcomes)
  -> model_trainer / prediction_strategy (recompute per-model weights for the next cycle)
```

`AdaptiveEngine.run_cycle()` performs one iteration of this loop for every configured
symbol; `run_loop()` repeats it on an interval until stopped or a cycle limit is
reached. Model weights are `accuracy ** 2`-derived, so a persistently more accurate
model gains more influence in the ensemble over successive cycles — this is the
"self-improving" property; it converges only as fast, and only as far, as the
directional accuracy of the underlying models allows, and it can also entrench a
temporarily lucky model, which is why `performance_tracker` uses a bounded
`accuracy_window` rather than all-time history.

## Design decisions

- **numpy/scipy core, lazy cross-package imports.** `build_engine` does not declare
  `build-oracle` or `build-finance` as installation dependencies; it expects them to be
  installed as sibling editable packages in a development workspace (`pip install -e
  ../build-oracle`, `pip install -e ../build-finance`). Imports of those packages are
  deferred into the functions/methods that need them, so importing `build_engine` alone
  never requires them, and unit tests can exercise config/persistence/alerts logic
  without either sibling installed.
- **Paper trading is the default, live trading is opt-in and gated.** `EngineConfig.
  paper_trading` defaults to `True`. Switching to live broker mode additionally
  requires an explicit acknowledgement string (`BUILD_ENGINE_LIVE_ACK` or
  `--live-ack`) matching `LIVE_TRADING_ACK`, plus Alpaca API credentials; missing
  either raises before any broker is constructed. See `SECURITY.md` for the full gate.
- **Flat, single-purpose modules.** Each file answers one question in the loop
  ("how do we train models?", "how do we turn a forecast into a signal?", "how do we
  score accuracy?"), which keeps each unit independently testable without a live
  broker or network connection.
- **Type-clean core, boundary-typed GUI.** The adaptive-engine core is fully type
  checked (`mypy` clean). The PyQt6 GUI is a thin adapter over an untyped Qt binding
  and is checked at its public boundary rather than strict-typed internally.
- **No secrets persisted.** `persistence.py` writes model weights, accuracy history,
  and trade logs to `~/.build-engine/`; it never writes broker API keys or the
  live-mode acknowledgement value to disk.

## Testing

The suite under `tests/` covers configuration defaults, model-trainer lifecycle,
prediction-strategy signal generation, performance-tracker weight computation,
persistence round-trips, and alert/regime-detection logic. Tests that require the
`build-oracle`/`build-finance` sibling packages to be installed are part of the same
suite (not excluded locally) since this repository's CI mocks the two sibling module
surfaces it imports. Run `pytest` for the full suite; `ruff check .` and `mypy` gate
style and types.
