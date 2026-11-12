# Security Policy

## Supported

Build Finance follows a rolling release. Until a 2.0 line exists, only the latest
release on the default branch is supported for fixes.

## Reporting a vulnerability

Report suspected vulnerabilities privately via GitHub Security Advisories: the
"Security" tab of this repository, then "Report a vulnerability". Do NOT open a public
issue for an unfixed vulnerability.

Please include the affected module and version, a minimal reproduction, and the impact.
The maintainer will acknowledge within a stated window and agree a disclosure date.

## Money-adjacent: read this before connecting a broker

Build Finance can place real trades through a real brokerage (currently Alpaca). This
section is the honest statement of what that does and does not mean.

- **API keys are user-supplied, never stored in code or logs.** The library never
  hardcodes, embeds, or bundles a broker API key or secret. Credentials are supplied by
  the caller at runtime (CLI flag, GUI settings field, or `BrokerConfig` in the Python
  API) and are not written to log output. Treat any key you enter as a secret: do not
  commit it, do not paste it into an issue, and store it the same way you would any
  other credential (environment variable or a secrets manager), not in source.
- **Paper trading is the default.** `BrokerConfig.paper_trading` defaults to `True`, and
  the documented setup path (see [docs/ALPACA_SETUP.md](docs/ALPACA_SETUP.md)) tells you
  to test with paper trading first. Paper mode uses real market data against a simulated
  account with fake money, so no funds move.
- **Live execution requires explicit opt-in.** Placing a real order against a real
  brokerage account requires the caller to deliberately set `paper_trading=False` (or
  the equivalent live flag) and to supply live-account credentials. Nothing in this
  library flips that switch on its own, and nothing auto-upgrades a paper session to
  live.
- **Backtests do not guarantee live results.** The backtesting engine models slippage,
  commission, and market impact, but a historical replay cannot fully capture live
  conditions: partial fills, order latency, liquidity gaps, data revisions, and
  regime change are all real risks a backtest does not fully price in. A strategy that
  performs well in `backtest.py` is not a promise of live performance. Treat backtest
  output as a hypothesis to validate in paper trading before ever considering live
  capital.
- **The library custodies no funds.** Build Finance never holds, transfers, or has
  custody of money. All funds and positions live at the broker (Alpaca or your paper
  account); this library only sends instructions to and reads state from that broker's
  API. Loss of funds from live trading is a trading-risk and account-security matter
  between you and your broker, not a defect this policy covers, but a vulnerability
  that could cause the library to place unintended orders, leak credentials, or
  misreport account state is squarely in scope below.

## Attack surface (the honest part)

- **No network by default.** The core numeric modules (indicators, strategies, risk,
  sizing, orderbook simulation, backtest, portfolio optimization) perform no network
  access. Network access is confined to `market_data.py` (Yahoo Finance, CoinGecko) and
  `broker.py` (Alpaca), and only runs when explicitly invoked.
- **No code evaluation.** Inputs are numbers, arrays, `Candle`/`Quote` records, and file
  paths; the library never `eval`s or executes input data.
- **File I/O is a real surface.** CSV import/export in `market_data.py` reads and writes
  files. Treat untrusted CSV input as untrusted: parsing is bounded to documented
  formats, but malformed files should raise, not corrupt state or execute anything.
- **Broker and market-data credentials are the highest-value surface.** A leaked Alpaca
  API key/secret can be used to place real trades on the associated account. Rotate keys
  immediately if you suspect exposure, and prefer paper keys during development.
- **Optional dependencies carry their own surface.** matplotlib, ta-lib, and PyQt6 are
  optional; when installed, their own advisories apply. The numpy/pandas/scipy-only core
  is unaffected by them.

## What does not count

- A malformed-file parse that raises a normal exception is expected behavior, not a
  vulnerability. A parse that reads out of bounds, hangs unboundedly, or corrupts memory
  in the pure-Python/numpy/pandas path is in scope.
- Financial loss from a strategy performing poorly, from live market conditions
  diverging from a backtest, or from normal brokerage risk (margin calls, slippage,
  outages at the broker) is a trading-risk matter, not a security vulnerability.
- Numerical inaccuracy relative to a published formula is a correctness issue (open a
  normal issue with the reference), not a security vulnerability.
