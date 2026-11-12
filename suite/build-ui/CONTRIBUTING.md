# Contributing

Build UI is a shared, binding-neutral Qt 6 theme and widget library for the
Build ecosystem. Keep the package dependency-light, theme widgets through
`build_ui.theme.C` rather than hardcoded colors, and include tests.

## Local setup

Choose one binding lane per environment:

```powershell
python -m pip install -e ".[dev,pyqt6]"
$env:QT_API = "pyqt6"
```

or:

```powershell
python -m pip install -e ".[dev,pyside6]"
$env:QT_API = "pyside6"
```

Do not install both extras into one test environment. Set
`QT_QPA_PLATFORM=offscreen` for automated widget tests.

## Verification

For documentation changes:

```powershell
python -m pytest tests/test_release_contract.py -q
git diff --check
```

For package changes, run the complete suite and both probes in separate,
binding-isolated environments. The requested-but-absent probe verifies that
QtPy cannot silently fall back to the other supported binding.

PyQt6 lane:

```powershell
py -3.12 -m venv .venv-pyqt6
$python = Resolve-Path .venv-pyqt6\Scripts\python.exe
& $python -m pip install -e ".[dev,pyqt6]"
$env:QT_API = "pyqt6"
$env:QT_QPA_PLATFORM = "offscreen"
& $python -m pytest tests -q --cov=build_ui --cov-report=term-missing --cov-fail-under=70
& $python -m ruff check .
& $python -m ruff format --check .
$qtpyMypyArgs = (& $python -m qtpy mypy-args) -split " "
& $python -m mypy @qtpyMypyArgs build_ui
& $python tests/qt_binding_probe.py
& $python tests/qt_selection_mismatch_probe.py --installed-api pyqt6
```

PySide6 lane:

```powershell
py -3.12 -m venv .venv-pyside6
$python = Resolve-Path .venv-pyside6\Scripts\python.exe
& $python -m pip install -e ".[dev,pyside6]"
$env:QT_API = "pyside6"
$env:QT_QPA_PLATFORM = "offscreen"
& $python -m pytest tests -q --cov=build_ui --cov-report=term-missing --cov-fail-under=70
& $python -m ruff check .
& $python -m ruff format --check .
$qtpyMypyArgs = (& $python -m qtpy mypy-args) -split " "
& $python -m mypy @qtpyMypyArgs build_ui
& $python tests/qt_binding_probe.py
& $python tests/qt_selection_mismatch_probe.py --installed-api pyside6
```

Do not commit `.env` files, credentials, generated distributions, or local-only
build artifacts. Publication requires a separate reviewed release decision.
