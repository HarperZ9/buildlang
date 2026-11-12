# Build UI 2 Qt Binding Bridge Design

**Date:** 2026-07-10
**Target release:** 2.0.0
**Status:** Approved for Calibrate Pro packaging dependency implementation

## Objective

Make Build UI usable by both the existing PyQt6 Build applications and the
PySide6-only Calibrate Pro release without loading two Qt bindings into one
process. Preserve the public theme/widget API while moving binding selection to
QtPy and explicit installation extras.

## Verified Starting Point

- PyPI currently serves Build UI 1.0.1 with a required `PyQt6>=6.5,<7`
  dependency.
- Build Color, Build Finance, Build Oracle, Build Engine, and Calibrate Pro all
  import Build UI while their GUI sources currently import PyQt6 directly.
- Several consumers declare unbounded `build-ui>=1.0.0`; publishing a
  PySide6-only minor release would resolve into a PyQt application and produce
  incompatible `QObject`/widget types.
- QtPy 2.4.3 is an MIT-licensed binding abstraction supporting PyQt6 and
  PySide6. It exposes `Signal`, `Slot`, and `Property` consistently.
- Disposable offscreen probes passed `Signal(dict)` and `QWidget` construction
  through QtPy 2.4.3 with both PyQt6 6.11.0 and PySide6 6.11.1.
- The existing Build UI suite passes 17 tests, Ruff, and Mypy, but only proves
  the PyQt6 import path and mostly tests class importability rather than widget
  behavior.

## Decision

Release Build UI 2.0.0 as a binding-neutral library using QtPy. The major
version reflects the changed installation contract: the core wheel depends on
QtPy but does not silently choose or install a Qt binding. Users install one of
the explicit extras:

```text
build-ui[pyqt6]>=2,<3
build-ui[pyside6]>=2,<3
```

Calibrate Pro installs the `pyside6` extra and sets `QT_API=pyside6` before the
first QtPy/Build UI import. Existing PyQt applications already declaring PyQt6
remain compatible; their dependency declarations should move to the explicit
`pyqt6` extra as they next release.

This avoids a forced simultaneous migration of every Build application while
ensuring Calibrate Pro's frozen graph contains only PySide6.

## Requirements

- **B1 — Stable public API:** Preserve `C`, `STYLE`, `create_stylesheet`,
  `Card`, `StatusDot`, `Heading`, `Stat`, `NavButton`, `Sidebar`, and
  `ToastNotification` at their current import paths.
- **B2 — QtPy source boundary:** `build_ui.widgets` imports Qt classes and
  `Signal` through the guarded `build_ui._qt` QtPy facade. Package source
  contains no direct PyQt5, PyQt6, PySide2, or PySide6 import.
- **B3 — Explicit binding extras:** Core dependency is
  `QtPy>=2.4.3,<3`. Extras provide `PyQt6>=6.5,<7` and
  `PySide6>=6.11.1,<7` independently. The wheel never installs both bindings.
- **B4 — Deterministic selection:** Build UI snapshots the caller's `QT_API`
  before importing QtPy, never mutates it, and fails closed if QtPy selects a
  different binding than the explicit request. Consumers with multiple
  installed bindings must set it before importing QtPy/Build UI.
- **B5 — Canonical version and license payload:** `build_ui.__version__` is
  `2.0.0` and setuptools
  reads it dynamically. No executable build, CI, release, or packaging surface
  defines another version value. Documentation may record the released version;
  the README badge reads published package metadata rather than a hardcoded
  badge value. Setuptools 77 or newer ships `LICENSE` and
  `THIRD_PARTY_NOTICES.md` as PEP 639 license files under the SPDX expression
  `FSL-1.1-MIT`.
- **B6 — Behavioral parity:** Card construction/layout, Sidebar signal
  emission and exactly-one-active navigation, StatusDot painting/mutation,
  Heading/Stat mutation, NavButton selection, and Toast construction/timer and
  animation setup pass under both bindings with an offscreen `QApplication`.
- **B7 — Process isolation in tests:** PyQt6 and PySide6 compatibility probes
  execute in separate subprocesses or clean environments because a process may
  not switch Qt bindings after QtPy imports.
- **B8 — Source hygiene:** Repository scans reject direct Qt-binding imports and
  legacy `pyqtSignal`, `pyqtSlot`, or `pyqtProperty` tokens in package source.
- **B9 — Wheel verification:** Clean PyQt6 and PySide6 virtual environments each
  install the built wheel with exactly one binding extra, verify metadata, set
  `QT_API`, prove the other binding is absent, import every public name, and
  construct representative widgets offscreen. The verifier runs with a working
  directory outside the repository and rejects imports resolved from the source
  checkout. It parses requirements and markers structurally with `packaging`,
  verifies the installed QtPy version, and proves both license files exist.
  `packaging` is installed only in verification environments.
- **B10 — Consumer transition:** Publish `MIGRATING.md` for existing PyQt6
  consumers and the Calibrate Pro PySide6 consumer. Record that direct
  `pip install build-ui` installs no binding and is insufficient for widget
  construction.
- **B11 — Calibrate frozen closure:** The Calibrate release installs
  `build-ui[pyside6]>=2,<3`, contains PySide6/QtPy only, and its frozen doctor
  proves `qtpy.API_NAME == "PySide6"`.
- **B12 — Truthful compatibility:** Build UI 2 does not claim every downstream
  application is migrated. PyQt compatibility is proven at Build UI's public
  boundary; each consumer retains its own release acceptance tests.
- **B13 — Qt 6 fail-closed guard:** `build_ui._qt` converts a missing binding to
  an actionable extras error and rejects QtPy selections other than PyQt6 or
  PySide6. It also rejects a supported binding that differs from the explicit
  request, so Build UI 2 does not silently run through PyQt5, PySide2, or
  QtPy's supported-binding fallback.
- **B14 — Branch-bearing execution:** After explicit user approval,
  implementation runs only in `C:\dev\worktrees\build-ui-2-qt-bridge` on
  `feat/build-ui-2-qt-bridge`, from a coordinator-recorded reviewed plan tip
  that descends from `f842789`; detached HEAD execution is rejected.
- **B15 — Non-publishing candidate:** The candidate workflow contains no PyPI
  credentials, trusted-publishing permission, or publish job. Publication is a
  later reviewed change, gated on Calibrate Pro accepting the exact candidate
  wheel hash.

## Package Metadata

The intended dependency shape is:

```toml
[build-system]
requires = ["setuptools>=77.0.0"]
build-backend = "setuptools.build_meta"

[project]
dynamic = ["version"]
license = "FSL-1.1-MIT"
license-files = ["LICENSE", "THIRD_PARTY_NOTICES.md"]
dependencies = ["QtPy>=2.4.3,<3"]

[project.optional-dependencies]
pyqt6 = ["PyQt6>=6.5,<7"]
pyside6 = ["PySide6>=6.11.1,<7"]
test = ["pytest>=8.0", "pytest-cov>=5", "tomli>=2,<3"]
dev = [
  "pytest>=8.0",
  "pytest-cov>=5",
  "tomli>=2,<3",
  "ruff>=0.6",
  "mypy>=1.10",
  "build>=1.2",
]

[tool.setuptools.dynamic]
version = {attr = "build_ui.__version__"}
```

CI installs the binding separately; the `test` and `dev` extras deliberately do
not pull both bindings into the same environment.

## Runtime Contract

Consumers select a binding before the first Qt import:

```python
import os

os.environ.setdefault("QT_API", "pyside6")

from build_ui.widgets import Card
```

Library code uses:

```python
from build_ui._qt import QtCore, QtGui, QtWidgets

Signal = QtCore.Signal
```

`Sidebar.page_changed = Signal(int)` retains the observable signal contract.
No public API exposes a binding-specific signal type.

`build_ui._qt` reads and validates `QT_API` before importing QtPy but never
mutates it. It translates a missing binding to an error naming
`build-ui[pyqt6]` and `build-ui[pyside6]`, rejects APIs other than PyQt6 and
PySide6, and rejects a supported API that differs from the explicit request
captured before QtPy's import-time fallback.

## CI and Release Matrix

1. Pure theme and metadata tests across Python 3.10–3.13.
2. PyQt6 compatibility on Linux and Windows, Python 3.12, offscreen.
3. PySide6 compatibility on Linux and Windows, Python 3.12, offscreen.
4. A no-binding environment proves theme-only importability and the actionable
   widget error.
5. Each one-binding environment requests the absent opposite binding in a fresh
   subprocess and proves Build UI fails closed.
6. Ruff, format, and Mypy run with QtPy's selected-binding Mypy arguments;
   behavioral coverage must be at least 70% without omissions or exclusions.
7. Wheel metadata inspection structurally proves QtPy core, independent binding
   extras, exact markers, and both PEP 639 license files.
8. Two clean wheel smoke environments prove exactly one Qt binding each from a
   working directory outside the source repository.
9. The candidate workflow builds and verifies without a publication surface.
10. Calibrate Pro consumes the local candidate wheel using the PySide6 extra
    and records its SHA-256 before publication is separately approved.

## Documentation and Compatibility

README and USAGE lead with explicit installs:

```bash
pip install "build-ui[pyside6]>=2,<3"
# or
pip install "build-ui[pyqt6]>=2,<3"
```

The migration guide states:

- PyQt6 applications install the `pyqt6` extra or keep their own PyQt6
  dependency and set `QT_API=pyqt6` if multiple bindings exist.
- PySide6 applications install the `pyside6` extra and set
  `QT_API=pyside6` before importing Build UI.
- A single process must not mix widget objects from two Qt bindings.
- Build UI 2 preserves public class/function names but changes the dependency
  installation contract, hence the major version.
- Existing downstream Build applications retain their own acceptance gates.
  Build Engine's undeclared GUI dependency and Build Ecosystem's aggregate
  PyQt6/PySide6 extras must be corrected before either claims Build UI 2 or
  single-binding compatibility.

The binding-specific hero copy is replaced with a binding-neutral SVG. The
stale PNG is removed rather than retained as a second hand-maintained source.

## Verification Strategy

1. Metadata tests fail on the current PyQt6-only dependency and static version.
2. Source-token tests fail on direct PyQt6 imports and `pyqtSignal`.
3. Subprocess binding probes fail before QtPy migration and pass afterward under
   both bindings.
4. A no-binding probe gets an actionable extras error; a forced Qt5 selection is
   rejected before widget construction; and an explicit request for an absent
   supported binding fails instead of falling back to the installed one.
5. Behavioral widget tests assert painting, emitted values, exclusive selection,
   timer state, and animation setup, not only class importability.
6. Wheel smoke tests install from `dist/`, run outside the repository, and prove
   their imports resolve from the clean virtual environment.
7. A Calibrate integration probe imports its active window with
   `QT_API=pyside6` and scans the environment/artifact for PyQt distributions.

## Delivery Sequence

1. After user approval, create and verify the branch-bearing centralized
   worktree from the coordinator-recorded reviewed plan tip.
2. Add metadata tests, move version metadata to one source, define binding
   extras, and ship the PEP 639 license payload.
3. Add source-boundary tests, the exact-request Qt6 guard, and migrate imports
   and signal declarations through QtPy.
4. Add binding-isolated behavioral and opposite-request tests, including the
   test-first Sidebar exclusive-selection correction.
5. Update CI and clean-wheel verification; replace publishing automation with
   a non-publishing candidate workflow.
6. Update README, USAGE, architecture, security, contributor/operator docs,
   `MIGRATING.md`, third-party notices, changelog, and binding-neutral assets.
7. Build the candidate wheel and consume it from Calibrate Pro's PySide6
   integration acceptance lane.
8. Separately design and approve publication only after both binding lanes and
   Calibrate integration pass for the exact candidate hash.

## Success Criteria

- [ ] The same Build UI wheel passes public widget behavior under PyQt6 and
  PySide6 in isolated processes.
- [ ] Package source has no direct binding import or legacy signal alias.
- [ ] Missing bindings and Qt5 selections fail with actionable, deterministic
  errors, and an explicit supported request never falls back to the other
  supported binding.
- [ ] Core metadata installs QtPy and exposes independent binding extras without
  installing both bindings.
- [ ] Existing public names and signal behavior remain stable.
- [ ] Behavioral coverage is at least 70% in both binding lanes and Mypy passes
  with QtPy's binding-specific arguments.
- [ ] Calibrate Pro resolves Build UI through the PySide6 extra and freezes no
  PyQt package.
- [ ] Documentation and release metadata accurately describe the new install
  contract and version.
- [ ] The installed wheel contains `LICENSE` and `THIRD_PARTY_NOTICES.md`, and
  requirement markers are compared structurally.
- [ ] Candidate CI has no publish job or trusted-publishing permission;
  publication remains blocked on Calibrate's exact-wheel evidence.
- [ ] Implementation runs on `feat/build-ui-2-qt-bridge` in the approved
  centralized worktree, never on a detached HEAD or normal `main` checkout.
- [ ] Downstream compatibility claims remain gated on each consumer's own tests;
  mixed-binding Build Ecosystem aggregates are not represented as proven.

## Approval Record

The operator previously approved the Calibrate Pro self-contained dependency
and PySide6 packaging lane. This bridge is the dependency-safe implementation
of that approval; it avoids breaking the other mapped PyQt6 consumers.
