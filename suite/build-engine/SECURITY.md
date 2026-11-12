# Security Policy

## Supported

Build Engine follows a rolling release. Until a 2.0 line exists, only the latest release
on the default branch is supported for fixes.

## Reporting a vulnerability

Report suspected vulnerabilities privately via GitHub Security Advisories — the
"Security" tab of this repository, then "Report a vulnerability". Do NOT open a public
issue for an unfixed vulnerability, and never include live broker credentials, account
identifiers, or trade history in a report.

Please include the affected module and version, a minimal reproduction, and the impact.
The maintainer will acknowledge within a stated window and agree a disclosure date.

## Not financial advice

Build Engine's predictions, signals, and backtests are research and engineering
output, not investment, financial, legal, or tax advice. Model output can be wrong,
delayed, overfit, incomplete, or inappropriate for real capital allocation. Nothing
this software produces should be treated as a recommendation to buy, sell, or hold
any instrument.

## Paper trading is the default; live trading is explicit opt-in

`EngineConfig.paper_trading` defaults to `True`, and the CLI's `--paper` flag is the
default behavior. Live broker execution is a separate, deliberately-gated path
inherited from `build-finance`'s broker abstraction:

- Live mode requires `--live` (or `paper_trading=False`) **and** an explicit
  acknowledgement — `BUILD_ENGINE_LIVE_ACK=I_UNDERSTAND_LIVE_RISK` (or the matching
  `--live-ack` / `EngineConfig.live_trading_ack` value) — **and** real broker
  credentials (`APCA_API_KEY_ID` / `APCA_API_SECRET_KEY`, or the equivalent
  `EngineConfig` fields).
- If the acknowledgement is missing or does not match, or credentials are absent,
  `AdaptiveEngine` raises `ValueError` before a live broker object is constructed —
  no live order path is reachable without both gates satisfied.
- The acknowledgement string and broker credentials are read from the environment or
  passed explicitly per run; they are never written to `~/.build-engine/state.json` or
  `trades.jsonl` by `persistence.py`.

## Model weights adjust automatically

The ensemble weight of each forecasting model is recomputed every cycle from its
recent directional accuracy (`performance_tracker.py`, weight ≈ `accuracy ** 2` over
a bounded `accuracy_window`). This is an intentional, inspectable feedback mechanism,
not a hidden or externally-triggerable control surface: weights only move in response
to the engine's own recorded prediction outcomes, and are visible via
`AdaptiveEngine.get_status()` and the `status`/`run` CLI output.

## No funds are custodied

Build Engine does not hold, transmit, or custody funds. In paper mode it simulates an
account entirely in memory/on-disk state under `~/.build-engine/`. In live mode it is
a thin signal-and-order-submission client against the configured broker (Alpaca); all
custody, settlement, and account control remain with the broker.

## Attack surface (the honest part)

- **No network access in paper mode**, other than the optional market-data fetch
  (`build_finance.market_data.fetch_yahoo`) used to source candles; a failed or
  unreachable fetch falls back to cached history rather than raising.
- **Live mode makes real network calls to the configured broker.** Treat broker
  credentials as secrets: use environment variables, never commit them, and never
  paste them into an issue or PR.
- **File I/O is a real surface.** `persistence.py` reads and writes JSON/JSONL under
  `~/.build-engine/`. Malformed or truncated state files should raise or be skipped,
  not silently produce an inconsistent in-memory state.
- **Optional dependencies carry their own surface.** `build-oracle`, `build-finance`,
  and PyQt6 are separate packages with their own advisories; the `build_engine`
  package itself performs no `eval` and no dynamic code execution on its inputs.

## What does not count

- A malformed-state-file parse that raises a normal exception is expected behavior,
  not a vulnerability. A parse that hangs unboundedly or corrupts unrelated files is
  in scope.
- A losing paper or live trade, or a model prediction that turns out wrong, is a
  correctness/performance question, not a security vulnerability. A code path that
  reaches live order submission *without* satisfying both the acknowledgement gate
  and the credential check is a vulnerability — report it as one.
