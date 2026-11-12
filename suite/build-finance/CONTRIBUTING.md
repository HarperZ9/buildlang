# Contributing

Thanks for your interest in Build Finance.

## Ground rules for a money-adjacent library

- **Paper first.** Paper/simulation mode is the default. Any change that touches live
  broker execution must keep live trading behind an explicit opt-in and must not weaken
  that gate. See [SECURITY.md](SECURITY.md).
- **No secrets in code.** API keys and broker credentials are user-supplied at runtime and
  must never be committed, logged, or embedded.
- **Honest results.** Do not present backtest numbers as live performance. Slippage,
  commissions, latency, and partial fills differ; keep that distinction explicit in code
  and docs.

## Development setup

```bash
git clone https://github.com/HarperZ9/build-finance.git
cd build-finance
pip install -e ".[all,dev]"
```

## Before opening a PR

Run the local gates and keep them green:

```bash
ruff check .
mypy
pytest -q --cov=build_finance
```

- Match the existing style; `ruff format` is the formatter.
- Type-check clean (`mypy` must pass). New numeric code should be typed; heterogeneous or
  genuinely dynamic values may use `Any` where honest.
- Add or update tests for behavior you change; keep meaningful assertions.
- Keep the public API and README aligned with real behavior.
