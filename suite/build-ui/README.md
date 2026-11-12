<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/HarperZ9/build-ui/main/docs/art/hero-dark.svg">
  <img src="https://raw.githubusercontent.com/HarperZ9/build-ui/main/docs/art/hero-light.svg" alt="build-ui: Qt 6 theme and widgets that give every Build app one look. A fine lattice of lines bulges outward around a bright core, as if seen through a lens, inside a ring." width="100%">
</picture>

# build-ui

Qt 6 theme and widgets that give every Build app one look.

```
pip install "build-ui[pyside6]>=2,<3"
```

[![version: 2.0.0](https://img.shields.io/badge/version-2.0.0-e6e1d6?style=flat-square&labelColor=1a1712)](https://pypi.org/project/build-ui/)
[![CI](https://github.com/HarperZ9/build-ui/actions/workflows/ci.yml/badge.svg)](https://github.com/HarperZ9/build-ui/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-FSL--1.1--MIT-e6e1d6?style=flat-square&labelColor=1a1712)](https://github.com/HarperZ9/build-ui/blob/main/LICENSE)
![python 3.10+](https://img.shields.io/badge/python-3.10%2B-e6e1d6?style=flat-square&labelColor=1a1712)

> Binding-neutral Qt 6 theme and reusable widget library for the Build
> ecosystem - one consistent look across every Build application.

[Project Telos](https://harperz9.github.io) | [gather](https://github.com/HarperZ9/gather) | [crucible](https://github.com/HarperZ9/crucible) | [index](https://github.com/HarperZ9/index) | [forum](https://github.com/HarperZ9/forum) | [telos](https://github.com/HarperZ9/telos) | [emet](https://github.com/HarperZ9/emet) | [buildlang](https://github.com/HarperZ9/buildlang)

Build UI provides a single source of truth for color and Qt stylesheet
generation, plus a small set of pre-styled widgets, so every application in
the Build family (Calibrate Pro, Build Color, Build Finance, Build Oracle,
Build Engine) renders consistently instead of re-implementing chrome.

## Quick Start

Choose exactly one supported Qt 6 binding:

```bash
pip install "build-ui[pyside6]>=2,<3"
# or
pip install "build-ui[pyqt6]>=2,<3"
```

Plain `pip install build-ui` installs QtPy but no Qt binding. It is intended for
applications that already provide one supported binding. If more than one Qt
binding is installed, set `QT_API` to `pyside6` or `pyqt6` before the first
QtPy or Build UI import. One process cannot mix widget objects from two
bindings.

```python
from build_ui.theme import C, STYLE
from build_ui.widgets import Card, Heading, Sidebar

app.setStyleSheet(STYLE)

card, layout = Card.with_layout()
layout.addWidget(Heading("Dashboard", level=1))
```

## Features

- **Theme (`theme.py`)** - `C`, a class of hex color constants (background,
  surface, border, text, accent, semantic colors); `create_stylesheet(c=None)`,
  which renders a full Qt stylesheet from any color class; `STYLE`, the
  pre-rendered stylesheet for the default palette.
- **Widgets (`widgets.py`)** - `Card`, `StatusDot`, `Heading`, `Stat`,
  `NavButton`, `Sidebar`, `ToastNotification`: reusable `QWidget` subclasses
  that read their colors from `theme.C`.

## Python API

```python
from build_ui.theme import C, STYLE, create_stylesheet
from build_ui.widgets import Card, Heading, Stat, NavButton, Sidebar, StatusDot, ToastNotification

# Apply the shared stylesheet to a QApplication
app.setStyleSheet(STYLE)


# Build a themed variant by overriding constants
class DarkC(C):
    BG = "#161616"


dark_style = create_stylesheet(DarkC)

# Compose widgets
sidebar = Sidebar(["Dashboard", "Settings"], app_name="My App")
stat = Stat("Uptime", "99.9%")
toast = ToastNotification("Saved", level="success")
```

## Architecture

```text
build_ui/
  theme.py     Color constants (C) + create_stylesheet() Qt stylesheet generator
  widgets.py   Card, StatusDot, Heading, Stat, NavButton, Sidebar, ToastNotification
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for module responsibilities and design
decisions, [USAGE.md](USAGE.md) for a full worked walkthrough, and
[MIGRATING.md](MIGRATING.md) for the Build UI 1 to 2 installation change.

## Installation

```bash
pip install "build-ui[pyside6]>=2,<3"
pip install "build-ui[pyqt6]>=2,<3"
```

Install one line, not both. Build UI requires Python 3.10+, installs QtPy as its
core dependency, and selects either PySide6 or PyQt6 through an extra. Calibrate
Pro uses the PySide6 lane. Existing PyQt6 applications use the PyQt6 lane.

## License

Copyright (c) 2022-2026 Zain Dana Harper. All rights reserved. Build UI is
released under the FSL-1.1-MIT: the source is available so you can read it, run
it, and build on it, while competing commercial use is reserved to the
Licensor to fund continued development. See [LICENSE](LICENSE) and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

---

Built by **[Zain Dana Harper](https://harperz9.github.io)** in Seattle: evidence-first tools that leave a re-checkable artifact behind. The full workbench is at [Project Telos](https://harperz9.github.io).
