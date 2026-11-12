# Build UI 2 Qt Binding Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Release Build UI 2.0.0 as one QtPy-backed wheel that preserves the public widget API under isolated PyQt6 and PySide6 processes and lets Calibrate Pro freeze a PySide6-only dependency graph.

**Architecture:** Replace direct binding imports in `build_ui.widgets` with QtPy and expose independent `pyqt6` and `pyside6` installation extras. Keep binding selection caller-owned through `QT_API`, validate real widget behavior under each binding, and prove the built wheel rather than only the source checkout.

**Tech Stack:** Python 3.10–3.13, QtPy 2.4.3, PyQt6 6.x, PySide6 6.11.1, setuptools, pytest, Ruff, Mypy, GitHub Actions.

## Global Constraints

- Target version is exactly `2.0.0` and comes only from `build_ui.__version__`.
- Preserve `C`, `STYLE`, `create_stylesheet`, `Card`, `StatusDot`, `Heading`, `Stat`, `NavButton`, `Sidebar`, and `ToastNotification` at their existing import paths.
- Package source imports Qt only through QtPy and contains no direct PyQt5, PyQt6, PySide2, or PySide6 import.
- Core dependency is `QtPy>=2.4.3,<3`; extras are `pyqt6 = ["PyQt6>=6.5,<7"]` and `pyside6 = ["PySide6>=6.11.1,<7"]`.
- Build UI snapshots an explicit `QT_API` before importing QtPy, never mutates
  it, and fails closed if QtPy selects a different supported binding.
- Tests for different bindings run in isolated processes/environments and never switch a loaded Qt binding in-process.
- Calibrate Pro consumes `build-ui[pyside6]>=2,<3`; its frozen artifact must contain no PyQt distribution.
- The candidate workflow must have no publish job, PyPI credential, or
  trusted-publishing permission. Publication is a later reviewed change after
  Calibrate accepts the exact candidate wheel hash.
- Implementation must occur, after explicit user approval, on
  `feat/build-ui-2-qt-bridge` in
  `C:\dev\worktrees\build-ui-2-qt-bridge`, created from a
  coordinator-recorded reviewed plan tip that descends from `f842789`. Never
  execute on detached HEAD or in the normal `main` checkout.
- Build metadata uses setuptools `>=77.0.0`, SPDX `FSL-1.1-MIT`, and ships
  `LICENSE` plus `THIRD_PARTY_NOTICES.md` as PEP 639 license files.
- Follow strict red-green-refactor: every production behavior change starts with a test observed failing for the intended reason.

---

## File Responsibility Map

- `build_ui/__init__.py` — authoritative `2.0.0` version.
- `build_ui/_qt.py` — QtPy import, actionable no-binding error, and Qt6-only guard.
- `build_ui/widgets.py` — binding-neutral widgets imported through QtPy.
- `pyproject.toml` — dynamic version, QtPy core dependency, independent binding extras.
- `tests/test_packaging_contract.py` — version/dependency/source-token contract.
- `tests/test_widget_behavior.py` — active-binding widget behavior.
- `tests/qt_binding_probe.py` — subprocess/wheel probe that emits one JSON receipt.
- `tests/qt_selection_mismatch_probe.py` — proves an explicit opposite-binding
  request fails closed in each one-binding environment.
- `scripts/verify_wheel.py` — installed-wheel verifier using verification-only
  `packaging`, run outside the repository.
- `.github/workflows/ci.yml` — source matrix for PyQt6 and PySide6.
- `.github/workflows/release.yml` — non-publishing wheel/sdist candidate plus
  two clean binding smokes.
- `README.md`, `USAGE.md`, `MIGRATING.md`, `ARCHITECTURE.md`, `SECURITY.md`,
  `CONTRIBUTING.md`, `CHANGELOG.md`, `AGENTS.md`, and
  `docs/ENTERPRISE-READINESS.md` — install,
  licensing, and compatibility contract.
- `THIRD_PARTY_NOTICES.md` — shipped third-party license notice.
- `docs/brand/build-ui-hero.svg`, `docs/brand/README.md`, and removal of
  `docs/brand/build-ui-hero.png` — one binding-neutral brand source.

---

### Task 0: Create and verify the branch-bearing execution worktree

**Files:**
- No repository files change in this task.

**Interfaces:**
- Consumes: explicit user approval and `BUILD_UI_REVIEWED_PLAN_TIP`, supplied by
  the coordinator after reviewing the committed design and plan.
- Produces: `C:\dev\worktrees\build-ui-2-qt-bridge` on
  `feat/build-ui-2-qt-bridge` at that exact reviewed commit.

- [ ] **Step 1: Fail closed unless the reviewed tip and normal checkout are exact**

Run from PowerShell only after the user approves worktree creation:

```powershell
$sourceRoot = "C:\dev\public\build-ui"
$worktreeRoot = "C:\dev\worktrees"
$worktree = Join-Path $worktreeRoot "build-ui-2-qt-bridge"
$featureBranch = "feat/build-ui-2-qt-bridge"

if ([string]::IsNullOrWhiteSpace($env:BUILD_UI_REVIEWED_PLAN_TIP)) {
    throw "Coordinator must set BUILD_UI_REVIEWED_PLAN_TIP to the reviewed plan commit"
}
$tipExpression = "$($env:BUILD_UI_REVIEWED_PLAN_TIP)^{commit}"
$reviewedPlanTip = (& git -C $sourceRoot rev-parse $tipExpression).Trim()
if ($LASTEXITCODE -ne 0) { throw "Reviewed plan tip is not a commit" }
$currentTip = (& git -C $sourceRoot rev-parse HEAD).Trim()
if ($currentTip -ne $reviewedPlanTip) { throw "Normal checkout moved after plan review" }
& git -C $sourceRoot merge-base --is-ancestor f842789 $reviewedPlanTip
if ($LASTEXITCODE -ne 0) { throw "Reviewed tip does not descend from f842789" }
if ((& git -C $sourceRoot branch --show-current).Trim() -ne "main") {
    throw "Reviewed source checkout is not main"
}
$dirty = @(& git -C $sourceRoot status --porcelain)
if ($dirty.Count -ne 0) {
    throw "Reviewed source checkout is dirty"
}
if (Test-Path -LiteralPath $worktree) { throw "Central worktree path already exists" }
$existingBranch = @(& git -C $sourceRoot branch --list $featureBranch)
if ($existingBranch.Count -ne 0) {
    throw "Feature branch already exists; review it explicitly"
}
```

Expected: all checks exit without output. The coordinator records the reviewed
tip at handoff time; this plan deliberately does not hardcode its own future
commit hash.

- [ ] **Step 2: Create a named branch in the centralized worktree**

```powershell
New-Item -ItemType Directory -Force -Path $worktreeRoot | Out-Null
git -C $sourceRoot worktree add -b $featureBranch $worktree $reviewedPlanTip
if ($LASTEXITCODE -ne 0) { throw "git worktree add failed" }
if ((& git -C $worktree branch --show-current).Trim() -ne $featureBranch) {
    throw "Worktree is detached or on the wrong branch"
}
if ((& git -C $worktree rev-parse HEAD).Trim() -ne $reviewedPlanTip) {
    throw "Worktree was created from the wrong commit"
}
```

Expected: Git reports a new worktree and the two assertions pass.

- [ ] **Step 3: Establish the clean baseline inside the worktree**

```powershell
Set-Location -LiteralPath $worktree
$env:QT_QPA_PLATFORM = "offscreen"
python -m pip install -e ".[dev]"
python -m pytest -q
python -m ruff check .
python -m ruff format --check .
python -m mypy build_ui
git status --short --branch
```

Expected: 17 tests pass; Ruff, format, and Mypy exit 0; status names
`feat/build-ui-2-qt-bridge` and is clean. Stop instead of implementing if any
baseline command fails.

---

### Task 1: Lock the Build UI 2 metadata and source contract

**Files:**
- Create: `tests/test_packaging_contract.py`
- Create: `THIRD_PARTY_NOTICES.md`
- Modify: `build_ui/__init__.py`
- Modify: `pyproject.toml`
- Modify: `tests/test_theme.py:127-130`

**Interfaces:**
- Consumes: the existing `build_ui.__version__` attribute.
- Produces: dynamic wheel version `2.0.0`, `pyqt6`/`pyside6` extras, and a reusable source-hygiene test used by later tasks.

- [ ] **Step 1: Write failing metadata and source-boundary tests**

Create `tests/test_packaging_contract.py`:

```python
from __future__ import annotations

import pathlib

import tomli
from packaging.licenses import canonicalize_license_expression

import build_ui

ROOT = pathlib.Path(__file__).resolve().parents[1]


def _project() -> dict:
    return tomli.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))


def test_version_is_dynamic_and_authoritative() -> None:
    data = _project()
    assert build_ui.__version__ == "2.0.0"
    assert data["project"]["dynamic"] == ["version"]
    assert "version" not in data["project"]
    assert data["tool"]["setuptools"]["dynamic"]["version"] == {"attr": "build_ui.__version__"}


def test_pep639_license_payload_is_declared() -> None:
    data = _project()
    assert data["build-system"]["requires"] == ["setuptools>=77.0.0"]
    license_expression = data["project"]["license"]
    assert license_expression == "LicenseRef-FSL-1.1-MIT"
    assert canonicalize_license_expression(license_expression) == license_expression
    assert data["project"]["license-files"] == [
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
    ]
    notice = (ROOT / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    for token in ("QtPy", "MIT", "PyQt6", "GPL", "PySide6", "LGPLv3"):
        assert token in notice


def test_qtpy_core_and_binding_extras_are_disjoint() -> None:
    project = _project()["project"]
    assert project["dependencies"] == ["QtPy>=2.4.3,<3"]
    extras = project["optional-dependencies"]
    assert extras["pyqt6"] == ["PyQt6>=6.5,<7"]
    assert extras["pyside6"] == ["PySide6>=6.11.1,<7"]
    assert not any("PyQt" in item or "PySide" in item for item in project["dependencies"])
    for name, requirements in extras.items():
        has_pyqt = any("PyQt" in item for item in requirements)
        has_pyside = any("PySide" in item for item in requirements)
        assert not (has_pyqt and has_pyside)
        if name in {"test", "dev"}:
            assert not has_pyqt and not has_pyside
```

- [ ] **Step 2: Run the tests and verify the intended RED state**

Run:

```powershell
python -m pip install "tomli>=2,<3"
python -m pytest tests/test_packaging_contract.py -q
```

Expected: failures showing version `1.0.1`, a literal project version,
`PyQt6` as the core dependency, missing binding extras, setuptools below 77,
and the missing shipped third-party notice.

- [ ] **Step 3: Make version and dependency metadata minimal and canonical**

Change `build_ui/__init__.py` to retain its docstring and public usage example,
then set:

```python
__version__ = "2.0.0"
```

Replace the relevant `pyproject.toml` fields with:

```toml
[build-system]
requires = ["setuptools>=77.0.0"]
build-backend = "setuptools.build_meta"

[project]
name = "build-ui"
dynamic = ["version"]
description = "Binding-neutral Qt theme and widgets for the Build ecosystem"
readme = "README.md"
license = "FSL-1.1-MIT"
license-files = ["LICENSE", "THIRD_PARTY_NOTICES.md"]
authors = [
    { name = "Zain Dana Harper" },
]
requires-python = ">=3.10"
dependencies = [
    "QtPy>=2.4.3,<3",
]

[project.optional-dependencies]
pyqt6 = ["PyQt6>=6.5,<7"]
pyside6 = ["PySide6>=6.11.1,<7"]
test = ["pytest>=8.0", "pytest-cov>=5", "tomli>=2,<3"]
dev = ["pytest>=8.0", "pytest-cov>=5", "tomli>=2,<3", "ruff>=0.6", "mypy>=1.10", "build>=1.2"]

[tool.setuptools.dynamic]
version = {attr = "build_ui.__version__"}
```

Create `THIRD_PARTY_NOTICES.md` with this complete payload:

```markdown
# Third-Party Notices

Build UI is licensed under FSL-1.1-MIT. That license does not relicense its
dependencies.

- QtPy 2.4.3 is available under the MIT License.
- PyQt6 is available under GPL/commercial terms.
- PySide6 and the relevant Qt modules are available under
  LGPLv3/GPLv3/commercial terms.

Binding packages are selected and installed independently through Build UI's
extras. Their distributed license files and upstream terms govern their use and
redistribution.
```

Replace the hardcoded version assertion in `tests/test_theme.py` with:

```python
def test_version(self):
    import importlib.metadata

    import build_ui

    assert build_ui.__version__ == importlib.metadata.version("build-ui")
```

Do not change `widgets.py` yet; Task 2 introduces its source-boundary test before
changing the import path.

- [ ] **Step 4: Run the narrow tests GREEN**

Run:

```powershell
python -m pip install -e ".[pyqt6,test]"
python -m pytest tests/test_packaging_contract.py -q
python -m pytest tests/test_theme.py -q
```

Expected: installation succeeds and both test commands pass.

- [ ] **Step 5: Commit the independently reviewable metadata change**

```powershell
git add build_ui/__init__.py pyproject.toml THIRD_PARTY_NOTICES.md tests/test_packaging_contract.py tests/test_theme.py
git commit -m "build: define Build UI 2 binding extras"
```

---

### Task 2: Migrate widget imports and signals through QtPy

**Files:**
- Create: `build_ui/_qt.py`
- Modify: `build_ui/widgets.py:3-22`
- Modify: `build_ui/widgets.py:174`
- Test: `tests/test_packaging_contract.py`

**Interfaces:**
- Consumes: QtPy selected by caller `QT_API` and the public widget names.
- Produces: binding-neutral `Signal(int)` and widget classes backed by the selected binding.

- [ ] **Step 1: Add failing facade, source-boundary, and legacy-signal tests**

First make the test module's imports Ruff-clean:

```python
import pathlib

import pytest
import tomli

import build_ui
```

Then append to `tests/test_packaging_contract.py`:

```python
def test_package_source_has_no_direct_qt_binding_imports() -> None:
    forbidden = ("from PyQt", "import PyQt", "from PySide", "import PySide")
    for path in sorted((ROOT / "build_ui").rglob("*.py")):
        text = path.read_text(encoding="utf-8")
        assert not any(token in text for token in forbidden), path


def test_package_source_uses_binding_neutral_signal_names() -> None:
    text = (ROOT / "build_ui" / "widgets.py").read_text(encoding="utf-8")
    assert "pyqtSignal" not in text
    assert "pyqtSlot" not in text
    assert "pyqtProperty" not in text
    assert "from build_ui._qt import" in text
    assert "Signal" in text


def test_qt_facade_rejects_unsupported_and_fallback_selection() -> None:
    from build_ui._qt import require_supported_api

    with pytest.raises(ImportError, match="requires PyQt6 or PySide6"):
        require_supported_api("PyQt5", None)
    with pytest.raises(ImportError, match="requires PyQt6 or PySide6"):
        require_supported_api("PySide2", None)
    with pytest.raises(ImportError, match="requested PyQt6.*selected PySide6"):
        require_supported_api("PySide6", "PyQt6")


def test_qt_facade_reads_but_never_mutates_qt_api() -> None:
    source = (ROOT / "build_ui" / "_qt.py").read_text(encoding="utf-8")
    assert 'os.environ.get("QT_API")' in source
    forbidden = (
        'os.environ["QT_API"] =',
        "os.environ['QT_API'] =",
        "os.environ.setdefault",
        "os.environ.update",
        "os.environ.pop",
        "del os.environ",
        "os.putenv",
    )
    assert not any(token in source for token in forbidden)
```

- [ ] **Step 2: Verify RED before changing production imports**

```powershell
python -m pytest tests/test_packaging_contract.py -q
```

Expected: failures because `build_ui._qt` does not exist and `widgets.py`
contains direct PyQt6 imports and `pyqtSignal`.

- [ ] **Step 3: Add the actionable Qt 6 selection guard**

Create `build_ui/_qt.py`:

```python
from __future__ import annotations

import os

SUPPORTED_REQUESTS = {"pyqt6": "PyQt6", "pyside6": "PySide6"}


def requested_api_name(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.lower()
    if normalized not in SUPPORTED_REQUESTS:
        raise ImportError(f"Build UI 2 requires QT_API=pyqt6 or QT_API=pyside6; got {value!r}.")
    return SUPPORTED_REQUESTS[normalized]


REQUESTED_API_NAME = requested_api_name(os.environ.get("QT_API"))

try:
    import qtpy
except ImportError as exc:
    if type(exc).__name__ == "QtBindingsNotFoundError":
        raise ImportError(
            'Build UI widgets require one Qt 6 binding; install "build-ui[pyqt6]>=2,<3" or "build-ui[pyside6]>=2,<3".'
        ) from exc
    raise


SUPPORTED_APIS = frozenset({"PyQt6", "PySide6"})


def require_supported_api(
    api_name: str,
    requested_name: str | None,
) -> str:
    if api_name not in SUPPORTED_APIS:
        raise ImportError(
            f"Build UI 2 requires PyQt6 or PySide6; QtPy selected {api_name!r}. "
            "Set QT_API=pyqt6 or QT_API=pyside6 before importing Build UI."
        )
    if requested_name is not None and api_name != requested_name:
        raise ImportError(
            f"Build UI explicitly requested {requested_name}, but QtPy selected "
            f"{api_name}. Install the requested Build UI binding extra and import "
            "Build UI before any other Qt binding."
        )
    return api_name


QT_API_NAME = require_supported_api(qtpy.API_NAME, REQUESTED_API_NAME)

from qtpy import QtCore, QtGui, QtWidgets

__all__ = [
    "QT_API_NAME",
    "QtCore",
    "QtGui",
    "QtWidgets",
    "require_supported_api",
    "requested_api_name",
]
```

The module snapshots `QT_API` before importing QtPy but must never assign,
delete, update, or call `setdefault` on the environment.

- [ ] **Step 4: Replace direct widget imports with the guarded QtPy modules**

Use this import block in `build_ui/widgets.py`:

```python
from build_ui._qt import QtCore, QtGui, QtWidgets

QEasingCurve = QtCore.QEasingCurve
QPoint = QtCore.QPoint
QPropertyAnimation = QtCore.QPropertyAnimation
Qt = QtCore.Qt
QTimer = QtCore.QTimer
Signal = QtCore.Signal
QColor = QtGui.QColor
QPainter = QtGui.QPainter
QFrame = QtWidgets.QFrame
QGraphicsDropShadowEffect = QtWidgets.QGraphicsDropShadowEffect
QGraphicsOpacityEffect = QtWidgets.QGraphicsOpacityEffect
QHBoxLayout = QtWidgets.QHBoxLayout
QLabel = QtWidgets.QLabel
QPushButton = QtWidgets.QPushButton
QSizePolicy = QtWidgets.QSizePolicy
QVBoxLayout = QtWidgets.QVBoxLayout
QWidget = QtWidgets.QWidget
```

Change the Sidebar signal declaration to:

```python
page_changed = Signal(int)
```

Do not set `QT_API` in library code and do not add binding-specific fallbacks.

- [ ] **Step 5: Run metadata/source and existing regression tests GREEN**

With the current environment's selected binding explicitly declared:

```powershell
$env:QT_API = "pyqt6"
$env:QT_QPA_PLATFORM = "offscreen"
python -m pytest tests/test_packaging_contract.py tests/test_theme.py -q
python -m ruff check build_ui tests
$qtpyMypyArgs = (& python -m qtpy mypy-args) -split " "
python -m mypy @qtpyMypyArgs build_ui
```

Expected: all commands exit 0; the complete pre-behavior suite remains green,
and Mypy analyzes the PyQt6 branch selected by QtPy.

- [ ] **Step 6: Commit the binding-neutral source migration**

```powershell
git add build_ui/_qt.py build_ui/widgets.py tests/test_packaging_contract.py
git commit -m "refactor: route Build UI widgets through QtPy"
```

---

### Task 3: Prove real widget behavior under each selected binding

**Files:**
- Create: `tests/test_widget_behavior.py`
- Create: `tests/qt_binding_probe.py`
- Create: `tests/qt_selection_mismatch_probe.py`
- Modify: `tests/test_theme.py`

**Interfaces:**
- Consumes: `QT_API`, `QT_QPA_PLATFORM=offscreen`, and Build UI public classes.
- Produces: a JSON binding receipt and behavioral tests reusable in CI and wheel smoke environments.

- [ ] **Step 1: Write failing behavior tests against the desired cross-binding contract**

Create `tests/test_widget_behavior.py`:

```python
from __future__ import annotations

import gc
import os

import pytest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from qtpy.QtGui import QImage
from qtpy.QtWidgets import QApplication, QVBoxLayout

from build_ui.widgets import (
    Card,
    Heading,
    NavButton,
    Sidebar,
    Stat,
    StatusDot,
    ToastNotification,
)


@pytest.fixture(scope="session")
def app() -> QApplication:
    return QApplication.instance() or QApplication([])


def test_card_with_layout_returns_attached_layout(app: QApplication) -> None:
    card, layout = Card.with_layout(QVBoxLayout, margins=(1, 2, 3, 4), spacing=5)
    gc.collect()
    assert card.layout() is layout
    margins = layout.contentsMargins()
    assert (margins.left(), margins.top(), margins.right(), margins.bottom()) == (
        1,
        2,
        3,
        4,
    )
    assert layout.spacing() == 5
    assert card.graphicsEffect() is not None


def test_status_dot_mutates_and_paints(app: QApplication) -> None:
    dot = StatusDot(size=12)
    dot.set_color("#112233")
    dot.show()
    app.processEvents()
    image = QImage(dot.size(), QImage.Format.Format_ARGB32)
    image.fill(0)
    dot.render(image)
    assert dot._color == "#112233"
    assert any(image.pixelColor(x, y).alpha() > 0 for x in range(image.width()) for y in range(image.height()))
    dot.close()


def test_sidebar_emits_selected_page(app: QApplication) -> None:
    sidebar = Sidebar(["One", "Two"], app_name="Probe", app_version="2.0.0")
    seen: list[int] = []
    sidebar.page_changed.connect(seen.append)
    sidebar._buttons[1].click()
    app.processEvents()
    assert seen == [1]
    assert [button.isChecked() for button in sidebar._buttons] == [False, True]


def test_text_and_selection_widgets_preserve_behavior(app: QApplication) -> None:
    heading = Heading("Section", level=2)
    assert heading.text() == "Section"
    assert "font-size: 16px" in heading.styleSheet()
    stat = Stat("Value", "1")
    stat.set_value("2", color="#112233")
    assert stat._value_label.text() == "2"
    assert "#112233" in stat._value_label.styleSheet()
    button = NavButton("Page")
    assert not button.isChecked()
    button.nextCheckState()
    assert button.isChecked()


def test_toast_timer_and_animations_are_owned(app: QApplication) -> None:
    toast = ToastNotification("Saved", level="success")
    try:
        gc.collect()
        assert toast.graphicsEffect() is not None
        assert toast._auto_timer.isSingleShot()
        assert toast._auto_timer.isActive()
        assert toast._auto_timer.interval() == 3000
        final_position = toast.pos()
        toast.slide_in()
        assert toast._slide_anim.duration() == 250
        assert toast._slide_anim.endValue() == final_position
        toast._fade_out()
        assert toast._fade_anim.duration() == 300
        assert toast._fade_anim.endValue() == 0.0
    finally:
        toast._auto_timer.stop()
        toast.hide()
        toast.deleteLater()
        app.processEvents()
```

Create `tests/qt_binding_probe.py`:

```python
from __future__ import annotations

import json
import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import qtpy
from qtpy.QtWidgets import QApplication

from build_ui import __version__
from build_ui.theme import STYLE, C, create_stylesheet
from build_ui.widgets import (
    Card,
    Heading,
    NavButton,
    Sidebar,
    Stat,
    StatusDot,
    ToastNotification,
)


def main() -> int:
    app = QApplication.instance() or QApplication([])
    card, layout = Card.with_layout()
    sidebar = Sidebar(["One", "Two"], app_version=__version__)
    seen: list[int] = []
    sidebar.page_changed.connect(seen.append)
    sidebar._buttons[1].click()
    app.processEvents()
    public_names = (
        C,
        STYLE,
        create_stylesheet,
        Card,
        StatusDot,
        Heading,
        Stat,
        NavButton,
        Sidebar,
        ToastNotification,
    )
    payload = {
        "api": qtpy.API_NAME,
        "qt_version": qtpy.QT_VERSION,
        "build_ui_version": __version__,
        "card": type(card).__name__,
        "card_base_module": type(card).__mro__[1].__module__,
        "layout": type(layout).__name__,
        "signal_roundtrip": seen == [1],
        "exclusive_sidebar": [button.isChecked() for button in sidebar._buttons] == [False, True],
        "public_api": all(public_names),
    }
    print(json.dumps(payload, sort_keys=True))
    passed = all(payload[name] for name in ("signal_roundtrip", "exclusive_sidebar", "public_api"))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
```

Create `tests/qt_selection_mismatch_probe.py`:

```python
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import tempfile

MODULE_BY_API = {"pyqt6": "PyQt6", "pyside6": "PySide6"}
OPPOSITE_API = {"pyqt6": "pyside6", "pyside6": "pyqt6"}
CHILD = """
import json

try:
    import build_ui.widgets
except ImportError as exc:
    print(json.dumps({"failed_closed": True, "error": str(exc)}, sort_keys=True))
    raise SystemExit(0)
print(json.dumps({"failed_closed": False}, sort_keys=True))
raise SystemExit(1)
"""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--installed-api", choices=tuple(MODULE_BY_API), required=True)
    args = parser.parse_args()
    installed_module = MODULE_BY_API[args.installed_api]
    absent_module = MODULE_BY_API[OPPOSITE_API[args.installed_api]]
    assert importlib.util.find_spec(installed_module) is not None
    assert importlib.util.find_spec(absent_module) is None
    requested_api = OPPOSITE_API[args.installed_api]
    env = os.environ.copy()
    env["QT_API"] = requested_api
    env["QT_QPA_PLATFORM"] = "offscreen"
    completed = subprocess.run(
        [sys.executable, "-c", CHILD],
        cwd=tempfile.gettempdir(),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode == 0, (completed.stdout, completed.stderr)
    payload = json.loads(completed.stdout.strip().splitlines()[-1])
    assert payload["failed_closed"] is True
    print(
        json.dumps(
            {
                "installed_api": args.installed_api,
                "requested_api": requested_api,
                "failed_closed": True,
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Run behavior tests and observe the migration-specific failure**

```powershell
$env:QT_API = "pyqt6"
$env:QT_QPA_PLATFORM = "offscreen"
python -m pytest tests/test_widget_behavior.py -q
```

Expected: the Sidebar test fails because the existing `_on_click` restyles the
buttons without clearing the first button's checked state. The other behavior
tests pass.

- [ ] **Step 3: Make Sidebar selection state deterministic**

Replace `Sidebar._on_click` with:

```python
def _on_click(self, index: int):
    for i, btn in enumerate(self._buttons):
        btn.setChecked(i == index)
    self.page_changed.emit(index)
```

This uses the public `setChecked` override so visual and logical state agree.

- [ ] **Step 4: Verify the PyQt6 lane GREEN in a clean environment**

```powershell
$pyqtVenv = Join-Path $env:TEMP ("build-ui-2-pyqt6-" + [guid]::NewGuid())
py -3.12 -m venv $pyqtVenv
$pyqtPython = Join-Path $pyqtVenv "Scripts\python.exe"
& $pyqtPython -m pip install --upgrade pip
& $pyqtPython -m pip install -e ".[dev,pyqt6]"
$env:QT_API = "pyqt6"
$env:QT_QPA_PLATFORM = "offscreen"
& $pyqtPython -m pytest tests/test_widget_behavior.py tests/test_theme.py tests/test_packaging_contract.py -q
& $pyqtPython tests/qt_binding_probe.py
& $pyqtPython tests/qt_selection_mismatch_probe.py --installed-api pyqt6
& $pyqtPython -c "import importlib.util; assert importlib.util.find_spec('PySide6') is None"
```

Expected probe fields include `"api": "PyQt6"`, version `2.0.0`, and
`"signal_roundtrip": true`; the other-binding assertion exits 0.

- [ ] **Step 5: Run the PySide6 lane and observe the ownership-specific failure**

```powershell
$pysideVenv = Join-Path $env:TEMP ("build-ui-2-pyside6-" + [guid]::NewGuid())
py -3.12 -m venv $pysideVenv
$pysidePython = Join-Path $pysideVenv "Scripts\python.exe"
& $pysidePython -m pip install --upgrade pip
& $pysidePython -m pip install -e ".[dev,pyside6]"
$env:QT_API = "pyside6"
$env:QT_QPA_PLATFORM = "offscreen"
& $pysidePython -m pytest tests/test_widget_behavior.py tests/test_theme.py tests/test_packaging_contract.py -q
```

Expected: the Card and Toast graphics-effect assertions fail because an
unparented `QGraphicsDropShadowEffect` wrapper can be collected under PySide6.
This is a binding-lifecycle difference, not a visual-contract difference.

- [ ] **Step 6: Give both initial shadow effects explicit widget ownership**

In `Card.__init__`, construct the initial shadow as:

```python
shadow = QGraphicsDropShadowEffect(self)
```

Make the same constructor change in `ToastNotification.__init__`. Do not keep a
Python-side shadow reference: `_fade_out()` deliberately replaces the Toast
shadow with a parented opacity effect, and a retained wrapper would become
stale after replacement.

Re-run the complete PySide6 lane:

```powershell
& $pysidePython -m pytest tests/test_widget_behavior.py tests/test_theme.py tests/test_packaging_contract.py -q
& $pysidePython tests/qt_binding_probe.py
& $pysidePython tests/qt_selection_mismatch_probe.py --installed-api pyside6
& $pysidePython -c "import importlib.util; assert importlib.util.find_spec('PyQt6') is None"
```

Expected probe fields include `"api": "PySide6"` and
`"signal_roundtrip": true`; the other-binding assertion exits 0.

Then re-run the complete PyQt6 lane from Step 4 to prove the explicit parent is
binding-neutral.

- [ ] **Step 7: Commit behavioral parity**

```powershell
git add build_ui/widgets.py tests/test_widget_behavior.py tests/qt_binding_probe.py tests/qt_selection_mismatch_probe.py docs/superpowers/plans/2026-07-10-build-ui-2-qt-bridge.md
git commit -m "test: prove Build UI behavior across Qt bindings"
```

---

### Task 4: Add dual-binding CI and clean-wheel verification

**Files:**
- Create: `scripts/verify_wheel.py`
- Create: `tests/test_release_contract.py`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `pyproject.toml`
- Modify: `tests/test_packaging_contract.py`

**Interfaces:**
- Consumes: a wheel path, selected `QT_API`, and the JSON probe from Task 3.
- Produces: source and built-wheel evidence for PyQt6 and PySide6.

The candidate gate must also accept the emitted PEP 639 metadata. If `twine
check` rejects the FSL short name because it is not in the SPDX license list,
declare the same bundled license as `LicenseRef-FSL-1.1-MIT`; this changes the
metadata identifier, not the license text or terms.

- [ ] **Step 1: Write failing workflow and verifier contract tests**

Create `tests/test_release_contract.py`:

```python
from __future__ import annotations

import ast
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]


def _subprocess_argv(text: str, method: str) -> list[str]:
    match = re.search(r'python -c "(?P<source>[^"]+)"', text, re.DOTALL)
    assert match is not None, "CI must invoke the type checker through python -c"
    source = " ".join(line.strip() for line in match.group("source").splitlines())
    tree = ast.parse(source)
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        if not (
            isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == "subprocess"
            and node.func.attr == method
        ):
            continue
        assert node.args and isinstance(node.args[0], ast.List)
        argv: list[str] = []
        for item in node.args[0].elts:
            if (
                isinstance(item, ast.Attribute)
                and isinstance(item.value, ast.Name)
                and item.value.id == "sys"
                and item.attr == "executable"
            ):
                argv.append("sys.executable")
            elif isinstance(item, ast.Constant) and isinstance(item.value, str):
                argv.append(item.value)
            elif isinstance(item, ast.Starred) and isinstance(item.value, ast.Name):
                argv.append(f"*{item.value.id}")
            else:
                raise AssertionError(ast.dump(item))
        return argv
    raise AssertionError(f"subprocess.{method} call not found")


def test_ci_runs_both_qt_binding_lanes() -> None:
    text = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    assert "pyqt6" in text
    assert "pyside6" in text
    assert "QT_API" in text
    assert "QT_QPA_PLATFORM" in text
    assert "tests/qt_binding_probe.py" in text
    assert "tests/qt_selection_mismatch_probe.py" in text
    assert "--cov-fail-under=70" in text
    assert _subprocess_argv(text, "check_output") == [
        "sys.executable",
        "-m",
        "qtpy",
        "mypy-args",
    ]
    assert _subprocess_argv(text, "call") == [
        "sys.executable",
        "-m",
        "mypy",
        "*flags",
        "build_ui",
    ]
    assert "--expect-no-binding" in text


def test_release_verifies_built_wheel_with_both_extras() -> None:
    text = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    assert "scripts/verify_wheel.py" in text
    assert "pyqt6" in text
    assert "pyside6" in text
    assert "--expect-no-binding" in text
    assert 'cd "$RUNNER_TEMP"' in text
    assert "--source-root" in text
    assert text.count("pip check") >= 3
    assert "packaging>=24,<26" in text
    assert "tests/qt_selection_mismatch_probe.py" in text
    assert "pypa/gh-action-pypi-publish" not in text
    assert "id-token: write" not in text
    assert "\n  publish:" not in text


def test_wheel_verifier_exists() -> None:
    assert (ROOT / "scripts" / "verify_wheel.py").is_file()
    assert (ROOT / "tests" / "qt_selection_mismatch_probe.py").is_file()
```

- [ ] **Step 2: Verify RED against the current workflows**

```powershell
python -m pytest tests/test_release_contract.py -q
```

Expected: failures for missing binding matrix, probe, and verifier.

- [ ] **Step 3: Add the built-wheel verifier**

Create `scripts/verify_wheel.py`:

```python
from __future__ import annotations

import argparse
import importlib
import importlib.metadata
import importlib.util
import json
import os
from pathlib import Path

from packaging.requirements import Requirement
from packaging.specifiers import SpecifierSet
from packaging.utils import canonicalize_name
from packaging.version import Version

EXPECTED_ENV = {"PyQt6": "pyqt6", "PySide6": "pyside6"}


def _requirements() -> dict[str, list[Requirement]]:
    result: dict[str, list[Requirement]] = {}
    for value in importlib.metadata.requires("build-ui") or []:
        requirement = Requirement(value)
        name = canonicalize_name(requirement.name)
        result.setdefault(name, []).append(requirement)
    return result


def _one_requirement(
    requirements: dict[str, list[Requirement]],
    name: str,
) -> Requirement:
    matches = requirements[name]
    assert len(matches) == 1, (name, matches)
    return matches[0]


def _assert_metadata() -> None:
    assert importlib.metadata.version("build-ui") == "2.0.0"
    requirements = _requirements()
    qtpy_requirement = _one_requirement(requirements, "qtpy")
    pyqt_requirement = _one_requirement(requirements, "pyqt6")
    pyside_requirement = _one_requirement(requirements, "pyside6")
    assert qtpy_requirement.specifier == SpecifierSet(">=2.4.3,<3")
    assert qtpy_requirement.marker is None
    assert pyqt_requirement.specifier == SpecifierSet(">=6.5,<7")
    assert str(pyqt_requirement.marker) == 'extra == "pyqt6"'
    assert pyside_requirement.specifier == SpecifierSet(">=6.11.1,<7")
    assert str(pyside_requirement.marker) == 'extra == "pyside6"'
    assert Version(importlib.metadata.version("QtPy")) in qtpy_requirement.specifier
    distribution = importlib.metadata.distribution("build-ui")
    license_names = {
        Path(str(path)).name
        for path in distribution.files or ()
        if ".dist-info/licenses/" in str(path).replace("\\", "/")
    }
    assert {"LICENSE", "THIRD_PARTY_NOTICES.md"} <= license_names


def _assert_installed_origin(source_root: Path | None) -> None:
    import build_ui

    if source_root is None:
        return
    origin = Path(build_ui.__file__).resolve()
    try:
        origin.relative_to(source_root.resolve())
    except ValueError:
        return
    raise AssertionError(f"build_ui resolved from source checkout: {origin}")


def _verify_no_binding(source_root: Path | None) -> dict[str, object]:
    _assert_installed_origin(source_root)
    assert importlib.util.find_spec("PyQt6") is None
    assert importlib.util.find_spec("PySide6") is None
    from build_ui.theme import STYLE

    assert STYLE
    try:
        importlib.import_module("build_ui.widgets")
    except ImportError as exc:
        message = str(exc)
        assert "build-ui[pyqt6]>=2,<3" in message
        assert "build-ui[pyside6]>=2,<3" in message
    else:
        raise AssertionError("build_ui.widgets imported without a Qt binding")
    return {"api": None, "theme_import": True, "widget_error": True}


def _verify_binding(expected_api: str, source_root: Path | None) -> dict[str, object]:
    expected_env = EXPECTED_ENV[expected_api]
    assert os.environ.get("QT_API") == expected_env
    other_api = "PySide6" if expected_api == "PyQt6" else "PyQt6"
    assert importlib.util.find_spec(expected_api) is not None
    assert importlib.util.find_spec(other_api) is None
    assert os.environ.get("QT_QPA_PLATFORM") == "offscreen"

    import qtpy
    from qtpy.QtWidgets import QApplication

    import build_ui
    from build_ui.theme import STYLE, C, create_stylesheet
    from build_ui.widgets import (
        Card,
        Heading,
        NavButton,
        Sidebar,
        Stat,
        StatusDot,
        ToastNotification,
    )

    _assert_installed_origin(source_root)
    assert expected_api == qtpy.API_NAME
    assert build_ui.__version__ == "2.0.0"
    assert all(
        (
            C,
            STYLE,
            create_stylesheet,
            Card,
            StatusDot,
            Heading,
            Stat,
            NavButton,
            Sidebar,
            ToastNotification,
        )
    )
    app = QApplication.instance() or QApplication([])
    card, layout = Card.with_layout()
    sidebar = Sidebar(["One", "Two"], app_version=build_ui.__version__)
    seen: list[int] = []
    sidebar.page_changed.connect(seen.append)
    sidebar._buttons[1].click()
    app.processEvents()
    checked = [button.isChecked() for button in sidebar._buttons]
    assert type(card).__mro__[1].__module__.startswith(expected_api)
    assert layout.parent() is card
    assert seen == [1]
    assert checked == [False, True]
    return {
        "api": qtpy.API_NAME,
        "build_ui_version": build_ui.__version__,
        "exclusive_sidebar": True,
        "other_binding_absent": True,
        "signal_roundtrip": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--expected-api", choices=("PyQt6", "PySide6"))
    mode.add_argument("--expect-no-binding", action="store_true")
    parser.add_argument("--source-root", type=Path)
    args = parser.parse_args()
    _assert_metadata()
    payload = (
        _verify_no_binding(args.source_root)
        if args.expect_no_binding
        else _verify_binding(args.expected_api, args.source_root)
    )
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

The verifier uses `packaging>=24,<26` for structural requirement and marker
comparison. Install it only in verification environments; do not add it to
Build UI's runtime or optional dependency metadata. Each clean environment runs
`pip check` before invoking the verifier.

- [ ] **Step 4: Replace CI with the exact binding-isolated workflow**

Replace `.github/workflows/ci.yml` with:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: true

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - run: python -m pip install ruff
      - run: python -m ruff check .
      - run: python -m ruff format --check .
      - run: git diff --check
      - name: Reject private-key material
        shell: bash
        run: |
          if git grep -n -I -E 'BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY'; then
            exit 1
          fi

  pure:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        python-version: ["3.10", "3.11", "3.12", "3.13"]
    steps:
      - uses: actions/checkout@v5
      - uses: actions/setup-python@v5
        with:
          python-version: ${{ matrix.python-version }}
      - run: python -m pip install -e ".[test]" "packaging>=24,<26"
      - name: Test binding-free surfaces
        run: |
          python -m pytest -q \
            tests/test_theme.py::TestColorConstants \
            tests/test_theme.py::TestStylesheet \
            tests/test_packaging_contract.py::test_version_is_dynamic_and_authoritative \
            tests/test_packaging_contract.py::test_qtpy_core_and_binding_extras_are_disjoint \
            tests/test_packaging_contract.py::test_package_source_has_no_direct_qt_binding_imports \
            tests/test_packaging_contract.py::test_package_source_uses_binding_neutral_signal_names
          python scripts/verify_wheel.py --expect-no-binding
          python -m pip check

  compatibility:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
        qt-api: [pyqt6, pyside6]
    env:
      QT_API: ${{ matrix.qt-api }}
      QT_QPA_PLATFORM: offscreen
    steps:
      - uses: actions/checkout@v5
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - name: Install Linux Qt runtime libraries
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libegl1 libxkbcommon0
      - run: python -m pip install -e ".[test,${{ matrix.qt-api }}]"
      - run: python -m pytest tests -q --cov=build_ui --cov-report=term-missing --cov-fail-under=70
      - run: python tests/qt_binding_probe.py
      - run: python tests/qt_selection_mismatch_probe.py --installed-api ${{ matrix.qt-api }}
      - run: python -m pip check

  typecheck:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        qt-api: [pyqt6, pyside6]
    env:
      QT_API: ${{ matrix.qt-api }}
    steps:
      - uses: actions/checkout@v5
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - run: python -m pip install -e ".[dev,${{ matrix.qt-api }}]"
      - name: Type check selected QtPy branch
        run: >-
          python -c "import subprocess, sys;
          flags=subprocess.check_output([sys.executable, '-m', 'qtpy', 'mypy-args'], text=True).split();
          raise SystemExit(subprocess.call([sys.executable, '-m', 'mypy', *flags, 'build_ui']))"
```

- [ ] **Step 5: Replace publishing automation with a non-publishing candidate workflow**

Replace `.github/workflows/release.yml` completely. This workflow deliberately
has one candidate job, read-only permissions, no release trigger, no PyPI
permission, and no publish job:

```yaml
name: Candidate

on:
  workflow_dispatch:

permissions:
  contents: read

env:
  FORCE_JAVASCRIPT_ACTIONS_TO_NODE24: true

jobs:
  candidate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: actions/setup-python@v6
        with:
          python-version: "3.12"
      - name: Install build verification tools
        run: python -m pip install --upgrade build twine
      - name: Install Linux Qt runtime libraries
        run: |
          sudo apt-get update
          sudo apt-get install -y libegl1 libxkbcommon0
      - name: Build sdist and wheel
        run: python -m build
      - name: Check distributions
        run: python -m twine check dist/*
      - name: Resolve candidate wheel
        shell: bash
        run: |
          wheel=$(python -c "from pathlib import Path; wheels=list(Path('dist').glob('*.whl')); assert len(wheels)==1; print(wheels[0].resolve())")
          uri=$(python -c "from pathlib import Path; print(Path('$wheel').as_uri())")
          echo "WHEEL=$wheel" >> "$GITHUB_ENV"
          echo "WHEEL_URI=$uri" >> "$GITHUB_ENV"
      - name: Verify core wheel without a binding
        shell: bash
        run: |
          python -m venv "$RUNNER_TEMP/core"
          "$RUNNER_TEMP/core/bin/pip" install "packaging>=24,<26" "build-ui @ $WHEEL_URI"
          "$RUNNER_TEMP/core/bin/python" -m pip check
          cd "$RUNNER_TEMP"
          "$RUNNER_TEMP/core/bin/python" "$GITHUB_WORKSPACE/scripts/verify_wheel.py" \
            --expect-no-binding --source-root "$GITHUB_WORKSPACE"
      - name: Verify wheel with PyQt6 only
        shell: bash
        env:
          QT_API: pyqt6
          QT_QPA_PLATFORM: offscreen
        run: |
          python -m venv "$RUNNER_TEMP/pyqt6"
          "$RUNNER_TEMP/pyqt6/bin/pip" install "packaging>=24,<26" "build-ui[pyqt6] @ $WHEEL_URI"
          "$RUNNER_TEMP/pyqt6/bin/python" -m pip check
          cd "$RUNNER_TEMP"
          "$RUNNER_TEMP/pyqt6/bin/python" "$GITHUB_WORKSPACE/scripts/verify_wheel.py" \
            --expected-api PyQt6 --source-root "$GITHUB_WORKSPACE"
          "$RUNNER_TEMP/pyqt6/bin/python" "$GITHUB_WORKSPACE/tests/qt_selection_mismatch_probe.py" \
            --installed-api pyqt6
      - name: Verify wheel with PySide6 only
        shell: bash
        env:
          QT_API: pyside6
          QT_QPA_PLATFORM: offscreen
        run: |
          python -m venv "$RUNNER_TEMP/pyside6"
          "$RUNNER_TEMP/pyside6/bin/pip" install "packaging>=24,<26" "build-ui[pyside6] @ $WHEEL_URI"
          "$RUNNER_TEMP/pyside6/bin/python" -m pip check
          cd "$RUNNER_TEMP"
          "$RUNNER_TEMP/pyside6/bin/python" "$GITHUB_WORKSPACE/scripts/verify_wheel.py" \
            --expected-api PySide6 --source-root "$GITHUB_WORKSPACE"
          "$RUNNER_TEMP/pyside6/bin/python" "$GITHUB_WORKSPACE/tests/qt_selection_mismatch_probe.py" \
            --installed-api pyside6
      - name: Record candidate hashes
        run: sha256sum dist/* > dist/SHA256SUMS.txt
      - uses: actions/upload-artifact@v7
        with:
          name: dist
          path: dist/
```

- [ ] **Step 6: Run source release-contract tests GREEN**

```powershell
python -m pytest tests/test_release_contract.py tests/test_packaging_contract.py -q
python -m build
```

Expected: all tests and build exit 0.

- [ ] **Step 7: Commit CI and wheel evidence**

```powershell
git add scripts/verify_wheel.py tests/test_release_contract.py .github/workflows/ci.yml .github/workflows/release.yml
git commit -m "ci: verify Build UI wheel with PyQt6 and PySide6"
```

---

### Task 5: Document the migration contract and run acceptance

**Files:**
- Modify: `README.md`
- Modify: `USAGE.md`
- Modify: `ARCHITECTURE.md`
- Create: `MIGRATING.md`
- Modify: `SECURITY.md`
- Modify: `CONTRIBUTING.md`
- Modify: `CHANGELOG.md`
- Modify: `AGENTS.md`
- Modify: `docs/ENTERPRISE-READINESS.md`
- Modify: `docs/brand/build-ui-hero.svg`
- Modify: `docs/brand/README.md`
- Delete: `docs/brand/build-ui-hero.png`
- Modify: `tests/test_release_contract.py`

**Interfaces:**
- Consumes: verified Build UI 2 metadata and both binding lanes.
- Produces: truthful install/migration guidance and the candidate wheel Calibrate consumes.

- [ ] **Step 1: Add failing documentation assertions**

Append to `tests/test_release_contract.py`:

```python
def test_docs_publish_explicit_binding_install_contract() -> None:
    texts = "\n".join(
        (ROOT / name).read_text(encoding="utf-8")
        for name in (
            "README.md",
            "USAGE.md",
            "MIGRATING.md",
            "ARCHITECTURE.md",
            "SECURITY.md",
            "CONTRIBUTING.md",
            "AGENTS.md",
            "THIRD_PARTY_NOTICES.md",
        )
    )
    assert "build-ui[pyside6]" in texts
    assert "build-ui[pyqt6]" in texts
    assert "QT_API" in texts
    assert "QtPy" in texts
    assert "PyQt6 only" not in texts
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    usage = (ROOT / "USAGE.md").read_text(encoding="utf-8")
    for public_doc in (readme, usage):
        assert "build-ui[pyside6]" in public_doc
        assert "build-ui[pyqt6]" in public_doc
        assert "installs QtPy but no Qt binding" in public_doc
    notice = (ROOT / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    for token in ("QtPy", "MIT", "PyQt6", "GPL", "PySide6", "LGPLv3"):
        assert token in notice
    assert "does not relicense" in notice
    assert "PyQt6 only" not in (ROOT / "docs" / "ENTERPRISE-READINESS.md").read_text(encoding="utf-8")
    assert 'src="docs/brand/build-ui-hero.svg"' in readme
    assert "PyQt6 theme" not in (ROOT / "docs/brand/build-ui-hero.svg").read_text(encoding="utf-8")
    assert not (ROOT / "docs/brand/build-ui-hero.png").exists()
```

- [ ] **Step 2: Verify RED against current PyQt-only documentation**

```powershell
python -m pytest tests/test_release_contract.py::test_docs_publish_explicit_binding_install_contract -q
```

Expected: missing explicit extras/QtPy/QT_API and existing PyQt-only wording.

- [ ] **Step 3: Update the public documentation contract**

Document these exact installation paths:

```bash
pip install "build-ui[pyside6]>=2,<3"
pip install "build-ui[pyqt6]>=2,<3"
```

State that plain `pip install build-ui` installs QtPy but no Qt binding; callers
with more than one binding set `QT_API` before import; one process cannot mix
widget objects from two bindings. Preserve all public names and examples.

Add a `2.0.0` changelog entry covering the major installation-contract change,
QtPy migration, binding matrix, and Calibrate dependency. Update AGENTS rules
from “PyQt6 only” to “QtPy source plus exactly one selected binding per process.”

Create `MIGRATING.md` with these required sections and commands:

```markdown
# Migrating to Build UI 2

Build UI 2 preserves the public theme and widget names but changes the Qt
installation contract. The core package installs QtPy and no Qt binding.

## PyQt6 applications

Install `pip install "build-ui[pyqt6]>=2,<3"`. If more than one Qt binding is
installed, set `QT_API=pyqt6` before the first QtPy or Build UI import.

## PySide6 applications

Install `pip install "build-ui[pyside6]>=2,<3"`. Set `QT_API=pyside6` before the
first QtPy or Build UI import. Calibrate Pro uses this lane.

## Process invariant

Do not mix PyQt6 and PySide6 objects in one process and do not change `QT_API`
after importing QtPy. Build UI rejects Qt 5 bindings.

## Downstream acceptance

Build UI proves its public boundary under both bindings. Each consuming
application must run its own GUI acceptance tests before updating its version
constraint. Build Engine must declare its GUI dependencies, and Build
Ecosystem must not aggregate PyQt6-only and PySide6-only applications as one
proven single-binding environment.
```

Retain the shipped `THIRD_PARTY_NOTICES.md` from Task 1 verbatim; documentation
may link to it but must not duplicate or weaken its dependency-license terms.

Change the README image source to `docs/brand/build-ui-hero.svg`; replace the
SVG's `PyQt6 theme` wording and accessible label with `binding-neutral Qt 6
theme`; update `docs/brand/README.md`; delete the stale PNG. Replace the
hardcoded version badge with `https://img.shields.io/pypi/v/build-ui`.

- [ ] **Step 4: Run the complete fresh acceptance sequence**

```powershell
$env:QT_API = "pyqt6"
$env:QT_QPA_PLATFORM = "offscreen"
python -m pytest -q
python -m ruff check .
python -m ruff format --check .
$qtpyMypyArgs = (& python -m qtpy mypy-args) -split " "
python -m mypy @qtpyMypyArgs
python -m pytest tests -q --cov=build_ui --cov-report=term-missing --cov-fail-under=70
python -m build
git diff --check
```

Then run both clean built-wheel smokes from Task 4. Record exact wheel path and
SHA-256 for the Calibrate integration task:

```powershell
$wheel = Get-ChildItem -LiteralPath dist -Filter "*.whl"
if ($wheel.Count -ne 1) { throw "Expected exactly one candidate wheel" }
Get-FileHash -LiteralPath $wheel.FullName -Algorithm SHA256
```

Do not publish yet.

- [ ] **Step 5: Commit the documentation and candidate evidence**

```powershell
git add README.md USAGE.md MIGRATING.md ARCHITECTURE.md SECURITY.md CONTRIBUTING.md CHANGELOG.md AGENTS.md docs/ENTERPRISE-READINESS.md docs/brand/build-ui-hero.svg docs/brand/README.md docs/brand/build-ui-hero.png tests/test_release_contract.py
git commit -m "docs: publish Build UI 2 binding migration"
```

---

## Plan Self-Review

- B1 and B6 map to Tasks 2–3.
- B2, B4, and B8 map to Task 2.
- B3 and B5 map to Task 1.
- B7 maps to Tasks 3–4.
- B9 maps to Task 4.
- B10 and B12 map to Task 5.
- B13 maps to Tasks 2 and 4.
- B14 maps to Task 0.
- B15 maps to Task 4; the candidate workflow has no publication capability.
- B11 is completed by the downstream Calibrate Pro plan and is a release gate
  before Build UI 2 publication.
- No task installs both binding extras into one test environment.
- No task changes another Build application. Build UI's PyQt6 boundary is
  tested, while each downstream application retains its own acceptance gate.
- The plan removes the existing publication job. Publication requires a later
  reviewed workflow change after Calibrate records the candidate wheel hash.
