# The Build suite

Five Python products that used to live in their own repositories. Each came in with its full
history and keeps its own licence file, version and changelog.

| Package | What it does |
|---|---|
| `build-engine` | adaptive prediction engine for forecasting, plus `capital-replay`, an offline paper supervisor that replays a local fixture through the build-finance paper core with models disabled |
| `build-oracle` | time-series forecasting toolkit |
| `build-finance` | algorithmic finance toolkit for backtesting, plus `live_paper`, a deterministic in-memory paper kernel (accounting, risk, simulated fills, reconciliation) that uses no network and places no orders |
| `build-ui` | shared PyQt6 theme and widget toolkit |
| `build-ecosystem` | meta-package for the Build package graph |

Run one package's tests:

```
cd suite/build-engine
python -m pip install -e ".[test]"
python -m pytest -q
```

The paper kernel and the supervisor are paper only. Their status file,
`build-finance/docs/live-paper/promotion-status.json`, records live readiness and publication as
blocked, model inference as disabled, and no profitability claim. The kernel tests also run with
sockets denied:

```
cd suite/build-finance
python -m pytest tests/live_paper -q
python scripts/run_network_denied.py --pytest tests/live_paper -q
```

Licence: the suite packages are source-visible under FSL-1.1-MIT, as stated in each package's
`LICENSE`. The compiler and the rest of this repository keep the licence in the root `LICENSE`.
