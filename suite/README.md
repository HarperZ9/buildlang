# The Build suite

Five Python products that used to live in their own repositories. Each came in with its full
history and keeps its own licence file, version and changelog.

| Package | What it does |
|---|---|
| `build-engine` | adaptive prediction engine for forecasting |
| `build-oracle` | time-series forecasting toolkit |
| `build-finance` | algorithmic finance toolkit for backtesting |
| `build-ui` | shared PyQt6 theme and widget toolkit |
| `build-ecosystem` | meta-package for the Build package graph |

Run one package's tests:

```
cd suite/build-engine
python -m pip install -e ".[test]"
python -m pytest -q
```

Licence: the suite packages are source-visible under FSL-1.1-MIT, as stated in each package's
`LICENSE`. The compiler and the rest of this repository keep the licence in the root `LICENSE`.
