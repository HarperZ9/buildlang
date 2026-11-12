# AGENTS.md - Build Engine

## Scope

This file applies to the `build-engine` repository. Root workspace instructions
still apply; this repo is a Python adaptive prediction and paper-trading engine
that integrates `build-oracle` (forecasting) with `build-finance` (execution).

## Product Boundary

Build Engine is the feedback-loop layer: it turns forecasts into trading
signals, executes them through a broker abstraction, and scores prediction
accuracy to adjust model weights. Keep the repo focused on that loop —
configuration, model lifecycle management, signal generation, execution
wiring, accuracy tracking, persistence, CLI behavior, and the optional GUI
dashboard.

Publishable surfaces:

- `build_engine/` - package code.
- `tests/` - regression coverage for config, model training, prediction
  strategy, performance tracking, persistence, and alerts.
- `README.md`, `CHANGELOG.md`, `docs/`, and `pyproject.toml` - package and
  product posture.

Keep local-only unless deliberately scrubbed:

- `.env`, `.env.*`, local settings, generated logs, and build artifacts.
- `~/.build-engine/` saved state (model weights, trade history) — this is
  user-machine runtime state, not repository content.
- Broker API keys, the live-mode acknowledgement value, and any real account
  identifiers or trade confirmations.

## Money-Adjacent Editing Rules

This package can place trades (paper by default, live when explicitly
unlocked). Treat every change in `adaptive_engine.py`, `cli.py`, and
`config.py` as safety-relevant:

- **Never weaken the live-mode gate.** Live broker mode must continue to
  require both the acknowledgement string (`BUILD_ENGINE_LIVE_ACK` /
  `--live-ack` matching `LIVE_TRADING_ACK`) and real API credentials before a
  live broker is constructed. Do not add a code path that reaches a live
  broker without both checks.
- **Paper trading stays the default.** `EngineConfig.paper_trading` defaults
  to `True`; do not flip this default or make `--live` implicit.
- **No secrets in persisted state.** `persistence.py` must never serialize
  broker API keys, secrets, or the live-mode acknowledgement value.
- **Preserve numerical behavior with focused tests.** Model-weighting and
  signal-generation regressions are easy to make and hard to see by
  inspection alone; a change to accuracy scoring or weight computation needs
  a test that pins the expected numbers.
- **Keep optional GUI dependencies optional.** Core package tests should not
  require PyQt6, `build-oracle`, or `build-finance` to be installed to
  exercise `config.py`, `persistence.py`, and `alerts.py`.
- **Keep CLI examples aligned with `pyproject.toml` entry points and the
  actual `argparse` definitions in `cli.py`.**

## Verification

For documentation or release-boundary changes:

```powershell
git diff --check
```

For package behavior changes, run the focused core suite:

```powershell
python -m pytest tests/test_performance_tracker.py tests/test_persistence.py tests/test_alerts.py -q
python -m pytest tests/test_model_trainer.py tests/test_prediction_strategy.py tests/test_engine.py -q
```

Before committing or pushing, scan changed files for credential-shaped content
(API keys, broker tokens) and confirm `.env` remains ignored.
