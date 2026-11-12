# Build Engine Enterprise Readiness

Build Engine is the adaptive forecasting-to-execution bridge of the build/Project
Telos family: it wires `build-oracle` forecasting models to `build-finance`
paper/live broker execution through an accuracy-weighted feedback loop. It is
designed to be used alone as a library or CLI, and as a component other
flagships depend on.

## Risk posture (read this first)

Build Engine is money-adjacent software. Before any other capability claim:

- **Not financial advice.** Predictions, signals, and backtests are research and
  engineering output. Model output can be wrong, delayed, overfit, incomplete,
  or inappropriate for real capital allocation.
- **Paper trading is the default.** `EngineConfig.paper_trading` defaults to
  `True`, and the CLI's `--paper` flag is the default behavior.
- **Live broker execution is explicit opt-in and gated.** Live mode requires
  `--live` (or `paper_trading=False`) **and** a matching
  `BUILD_ENGINE_LIVE_ACK=I_UNDERSTAND_LIVE_RISK` acknowledgement **and** real
  Alpaca credentials. Missing any one of the three raises `ValueError` before a
  live broker object is constructed.
- **Model weights adjust automatically, but only from the engine's own recorded
  outcomes.** Ensemble weighting (`performance_tracker.py`) is not an externally
  triggerable control surface — see [SECURITY.md](../SECURITY.md).
- **No funds are custodied.** Paper mode simulates an account in local state
  under `~/.build-engine/`. In live mode, all custody and settlement remain
  with the configured broker (Alpaca).

See [SECURITY.md](../SECURITY.md) for the full statement, attack surface, and
what does and does not count as a security issue.

## Enterprise role

- Train and refresh forecasting models per symbol through `build-oracle`, then
  turn ensembled forecasts into directional trading signals.
- Execute those signals through the same `build-finance` broker abstraction
  used for backtesting, so a validated paper run and a live run share one code
  path.
- Score prediction accuracy against realized outcomes every cycle and recompute
  per-model ensemble weights, so persistently more accurate models gain more
  influence over time — a bounded, inspectable feedback loop, not a black box.
- Detect regime changes (trend reversal, volatility spike, accuracy drop,
  drawdown) and surface them via callback, JSONL log, or optional webhook.

## Operator surface

- `build-engine` CLI for scriptable runs, backtests, and status checks
  (`run`, `backtest`, `status`, `gui`).
- The importable Python API (`build_engine.adaptive_engine`, `.config`,
  `.model_trainer`, `.prediction_strategy`, `.performance_tracker`,
  `.persistence`, `.alerts`) for embedding in pipelines.
- An optional PyQt6 dashboard for interactive monitoring (thin adapter over the
  core; requires the sibling `build-ui` package).

## Reproducibility and provenance

- `AdaptiveEngine.run_cycle()` and `backtest` accept fixed seeds for generated
  sample data, so a reported run can be regenerated from the same config.
- `AdaptiveEngine.get_status()` and `persistence.EngineState` are the durable
  snapshots of engine state (weights, accuracy history, trade log) and can be
  re-derived and diffed run-to-run.
- Model-weight evolution is deterministic given the same accuracy history: the
  weighting function (`accuracy ** 2` over a bounded window) is documented and
  testable in isolation from any live broker or network connection.

## Dependencies and boundary

- **Runtime core:** `numpy`, `scipy`. `build-oracle` and `build-finance` are
  sibling packages imported lazily at the call sites that need them, not
  installation dependencies of `build_engine` itself — importing the package
  alone requires neither.
- **Network access** is confined to the optional market-data fetch
  (`build_finance.market_data.fetch_yahoo`) and, in live mode only, real calls
  to the configured broker (Alpaca). Paper mode makes no live-broker network
  calls.
- **Optional:** PyQt6 (GUI), `build-ui` (shared GUI theme/widgets). Each is
  isolated so the core stays installable without a GUI toolkit.
- The GUI and CLI consume the core; they are never a dependency of it.

## Quality gates

- `ruff check .` (style), `mypy` (types — the adaptive-engine core is
  type-clean; the GUI adapter is boundary-typed), and `pytest` run in CI on
  every push and pull request, on Ubuntu and Windows.
- CI mocks the `build-oracle`/`build-finance` sibling module surfaces so the
  test suite runs without either package installed as an editable dependency.
- No broker API key or the live-mode acknowledgement value is ever written to
  `~/.build-engine/state.json` or `trades.jsonl` by `persistence.py`.

## Honest limits

- A profitable paper run or backtest is not a promise of live performance —
  see `build-finance`'s own honest limits on slippage, commission, latency, and
  regime change, which this engine inherits when it executes through that
  broker abstraction.
- Forecast accuracy is bounded by the underlying `build-oracle` models; the
  feedback loop converges only as fast, and only as far, as those models'
  actual directional accuracy allows, and can temporarily entrench a lucky
  model within the bounded accuracy window.
- The optional GUI layer inherits the maturity and advisories of PyQt6 and the
  sibling `build-ui` package; the guarantees above describe the numpy/scipy-only
  core.
