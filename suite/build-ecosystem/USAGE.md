# Build Ecosystem — Usage Guide

Build Ecosystem is the meta-package for the Build family. It has no CLI and no API of its own beyond a version string.

## Install

The `build-ecosystem` distribution is not on PyPI yet. Use a source checkout for development and smoke checks:

```bash
git clone https://github.com/HarperZ9/build-ecosystem.git
cd build-ecosystem
python -m pip install -e ".[test]" --no-deps
python -c "import build_ecosystem; print(build_ecosystem.__version__)"
```

Requires Python 3.10+. The full one-command registry install is held until `build-ecosystem` and every required member package are published. At this revision, `build-finance` is the missing required PyPI dependency; install it from its owning repository for local ecosystem testing.

## What gets installed

- `build-color` — color-science library
- `build-finance` — algorithmic trading toolkit
- `build-oracle` — time-series forecasting and anomaly detection
- `build-engine` — self-improving prediction and trading engine
- `calibrate-pro` — professional display calibration
- `build-ui` — shared PyQt6 theme/widget layer (via the `gui`/`all` extras)

## Usage

Import each member package directly — `build_ecosystem` does not re-export
their APIs:

```python
import build_color
import build_finance
import build_oracle
import build_engine
import calibrate_pro

print(build_color.__version__)
```

See each member's own README and USAGE.md for its CLI and Python API.

## Troubleshooting

- `No matching distribution found for build-ecosystem`: expected until the PyPI project is published. Use the source checkout path above.
- `No matching distribution found for build-finance`: expected in a full dependency install at this revision. Install `build-finance` from its repository or run this meta-package with `--no-deps` for metadata checks.
- The test suite uses `pytest.importorskip` for member packages, so an isolated checkout can still verify the meta-package version without installing the full family.

## See also

- `README.md` — project overview.
- `ARCHITECTURE.md` — the umbrella structure and version-pinning contract.
