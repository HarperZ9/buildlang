# Architecture

Build UI is a binding-neutral Qt 6 support library: a shared theme, a QtPy
selection facade, and a shared widget set. It has no business logic of its own.
It exists so every application in the Build family (Calibrate Pro, Build Color,
Build Finance, Build Oracle, Build Engine) renders with one consistent look
instead of five divergent ones.

## Layers

```text
build_ui/
  theme.py     Color constants (class C) + create_stylesheet() generator
  _qt.py       QtPy selection validation and binding-neutral Qt namespaces
  widgets.py   Reusable QWidget subclasses built on theme.py and _qt.py
  __init__.py  Public version surface
```

## theme.py

`C` is a plain class of hex-string color constants (background, surface, text,
accent, and semantic colors). `create_stylesheet(c=None)` renders a full Qt
stylesheet string from a color class, defaulting to `C`. `STYLE` is the
pre-rendered stylesheet for `C`, computed once at import time.

Passing a different class (subclassing `C` and overriding constants) into
`create_stylesheet` produces a themed variant without touching widget code.
Theme imports do not require a display or a Qt binding.

## _qt.py

The private facade imports Qt through QtPy, validates the selected API, and
exports binding-neutral `QtCore`, `QtGui`, and `QtWidgets` namespaces. Build UI
accepts PyQt6 or PySide6, rejects Qt 5 APIs, and fails closed when `QT_API`
requests a binding that is not installed.

The core package installs QtPy but no Qt binding. Binding packages remain
separate extras, and a process selects exactly one before its first QtPy or
Build UI widget import.

## widgets.py

Each widget is a thin `QWidget`/`QFrame`/`QLabel`/`QPushButton` subclass that
applies `theme.py` colors in its constructor: `Card`, `StatusDot`, `Heading`,
`Stat`, `NavButton`, `Sidebar`, `ToastNotification`. Widgets read colors
directly from `build_ui.theme.C`; consumers needing a different palette should
apply a process-wide stylesheet created with `create_stylesheet`.

`Sidebar` and `NavButton` cooperate: `Sidebar` owns the list of `NavButton`s and
emits `page_changed` when the active page changes. `ToastNotification` owns its
timer and property animations for auto-dismiss and slide/fade transitions.

## Design decisions

- **No state beyond the widget itself.** Widgets hold display state and emit Qt
  signals; they do not reach into application state.
- **QtPy source plus exactly one selected binding per process.** Build UI keeps
  its source binding-neutral while each application chooses PyQt6 or PySide6.
  Widget objects from two bindings must never be mixed in one process.
- **Stable public names.** `C`, `STYLE`, `create_stylesheet`, and all existing
  widget class names are preserved across the Build UI 2 migration.
- **Headless-safe theme, GUI-only widgets.** Color and stylesheet generation
  are pure string work; constructing a widget requires a running
  `QApplication`.

## Testing

The suite verifies theme behavior, the public widget boundary, ownership,
signals, animations, binding selection, and requested-but-absent binding
failure under both PyQt6 and PySide6. CI isolates each binding, derives mypy
arguments from QtPy, enforces at least 70% coverage, and verifies the built
wheel in binding-free and single-binding environments.
