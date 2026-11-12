# Contributing

Build Ecosystem is a public meta-package with no logic of its own. The most
common contribution is bumping a pinned member version in `pyproject.toml`
after that member ships a release.

## Local Setup

```powershell
python -m pip install -e .
python -m pip install pytest ruff
```

## Verification

```powershell
git diff --check
python -m pytest tests/ -q
```

The test suite uses `pytest.importorskip` per member, so it passes even when
sibling packages are not installed in your checkout.

Do not commit `.env` files, credentials, or local-only build artifacts.
