# AGENTS.md - Build Finance

## Scope

This file applies to the `build-finance` repository. Root workspace instructions
still apply; this repo is a public Python algorithmic trading toolkit and a
dependency surface for other Telos tooling.

## Product Boundary

Build Finance is a reusable trading toolkit. Keep the public repo focused on
deterministic and well-tested market analysis: technical indicators, trading
strategies, risk metrics, position sizing, backtesting, portfolio optimization,
market data access, broker execution, CLI behavior, and optional GUI surfaces.

Publishable surfaces:

- `build_finance/` - package code.
- `tests/` - regression coverage for indicators, strategies, risk metrics,
  sizing, order book simulation, backtesting, portfolio optimization, market
  data, and broker behavior.
- `README.md`, `CHANGELOG.md`, `docs/`, and `pyproject.toml` - package and
  product posture.

Keep local-only unless deliberately scrubbed:

- `.env`, `.env.*`, local settings, generated logs, and build artifacts.
- Broker API keys and secrets (Alpaca or any other broker), account
  statements, real trade history, and any customer- or account-identifying
  data. See [SECURITY.md](SECURITY.md) for the credential-handling posture.

## Editing Rules

- Preserve numerical behavior with focused tests; indicator, risk, and
  backtest math regressions are easy to make and hard to see by inspection
  alone.
- Keep optional GUI dependencies optional. Core package tests should not
  require PyQt6.
- Keep CLI examples aligned with `pyproject.toml` entry points.
- Never hardcode or log broker API keys or secrets. Paper trading is the
  default; anything that touches live order placement must keep an explicit
  opt-in gate and must not weaken it silently.
- When adding a new indicator, strategy, or risk metric, document the source
  formula and add at least one known-value or invariant test.

## Verification

For documentation or release-boundary changes:

```powershell
git diff --check
```

For package behavior changes, run the focused core suite:

```powershell
python -m pytest tests/test_indicators.py tests/test_backtest.py -q
python -m pytest tests/test_risk.py tests/test_market_data.py tests/test_data_bridge.py -q
```

Before committing or pushing, scan changed files for credential-shaped content
(API keys, secrets) and confirm `.env` remains ignored.
