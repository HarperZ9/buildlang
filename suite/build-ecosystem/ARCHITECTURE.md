# Architecture

Build Ecosystem is an umbrella meta-package. It contains no forecasting,
color-science, trading, or calibration logic of its own — `build_ecosystem/`
is a single module whose job is to declare, at a pinned version range, the
full Build family as one installable unit.

```
build_ecosystem/
  __init__.py     __version__ only; no logic
```

## Members

- **build-color** — color-science library (spaces, tone mapping, appearance
  models, Delta E metrics, chromatic adaptation, spectral utilities, ICC/LUT).
- **build-finance** — algorithmic trading toolkit (indicators, strategies,
  backtesting).
- **build-oracle** — time-series forecasting and anomaly detection (ARIMA,
  VAR, Prophet-style, neural, ensemble, changepoint detection).
- **build-engine** — self-improving prediction and trading engine that
  composes build-oracle and build-finance (model training, adaptive
  prediction strategy).
- **calibrate-pro** — professional display calibration (ICC profile
  generation, DDC-CI monitor control, using build-color for the underlying
  color math).
- **build-ui** — shared PyQt6 theme and widget layer used by the optional GUI
  extras of the other members.

## Data flow

There is no data flow inside this package. `pyproject.toml` pins each
member's minimum version (`>=1.0.0`) as a `dependencies` entry, and the `gui`
and `all` extras forward to the corresponding extra on each member. Installing
`build-ecosystem` installs the whole family; the members remain independent
packages with their own public APIs, tests, and release cadence.

## Design decisions

- **Aggregator, not a facade.** `build_ecosystem` does not re-export or wrap
  the members' APIs — importing `build_color`, `build_finance`, etc. directly
  is still the intended usage. The package exists purely to make "install
  everything" a single `pip install build-ecosystem` instead of six.
- **Version pinning is the whole contract.** The meta-package's only ongoing
  maintenance is bumping the pinned minimum version of a member when that
  member ships a release the ecosystem should adopt.
- **Tests are cross-wiring smoke checks.** `tests/test_ecosystem.py` uses
  `pytest.importorskip` per member so the suite passes in an isolated
  checkout (where sibling repos may not be installed) and asserts real
  cross-package imports when the full family is installed.

## Testing

`pytest` runs `tests/test_ecosystem.py`, which skips (rather than fails) any
member that is not installed, and otherwise verifies each member imports and
that a couple of cross-package call paths (calibrate-pro using build-color,
build-engine using build-oracle and build-finance) resolve correctly.
