<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/build-ecosystem/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/build-ecosystem/main/docs/art/hero-light.svg" alt="build-ecosystem: One install for the Build family: color, trading, forecasting, display. Clusters of small nodes, named color, finance, oracle, engine and calibrate, are wired to their neighbours and bundled through a bright core." width="100%">
</picture>

# build-ecosystem

One install for the Build family: color, trading, forecasting, display.

```
git clone https://github.com/HarperZ9/build-ecosystem.git
```

[![version: 1.0.0](https://img.shields.io/badge/version-1.0.0-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/build-ecosystem/releases/latest)
[![CI](https://github.com/HarperZ9/build-ecosystem/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/build-ecosystem/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-FSL--1.1--MIT-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/build-ecosystem/blob/main/LICENSE)
![python 3.10+](https://img.shields.io/badge/python-3.10%2B-e6e1d6?style=flat-square&labelColor=1a1712)

[Project Telos](https://harperz9.github.io) | [gather](https://github.com/HarperZ9/gather) | [crucible](https://github.com/HarperZ9/crucible) | [index](https://github.com/HarperZ9/index) | [forum](https://github.com/HarperZ9/forum) | [telos](https://github.com/HarperZ9/telos) | [emet](https://github.com/HarperZ9/emet) | [buildlang](https://github.com/HarperZ9/buildlang)

Complete Build ecosystem meta-package. It is the version-pinning hub for the Build family: color science, algorithmic trading, time-series forecasting, self-improving prediction, display calibration, and the shared UI layer.

## Installation

`build-ecosystem` is not published on PyPI yet, and one member dependency (`build-finance`) is also not available from PyPI. Until those registry entries exist, install this repository from source for metadata and smoke checks:

```bash
git clone https://github.com/HarperZ9/build-ecosystem.git
cd build-ecosystem
python -m pip install -e ".[test]" --no-deps
python -c "import build_ecosystem; print(build_ecosystem.__version__)"
```

The published member packages currently available on PyPI are `build-color`, `build-oracle`, `build-engine`, `calibrate-pro`, and `build-ui`. Install `build-finance` from its owning repository until it has a registry release. Do not use `pip install build-ecosystem` until the PyPI package exists.

## Included Packages

- **build-color** -- Professional color science library
- **build-finance** -- Algorithmic trading toolkit
- **build-oracle** -- Time series forecasting and anomaly detection
- **build-engine** -- Self-improving prediction and trading engine
- **calibrate-pro** -- Professional display calibration

## Repository

[https://github.com/HarperZ9/build-ecosystem](https://github.com/HarperZ9/build-ecosystem)

---

Built by **[Zain Dana Harper](https://harperz9.github.io)** in Seattle: evidence-first tools that leave a re-checkable artifact behind. The full workbench is at [Project Telos](https://harperz9.github.io).
