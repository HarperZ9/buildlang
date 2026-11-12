# Build UI - Usage Guide

Build UI is a binding-neutral Qt 6 theme and widget library for the Build
ecosystem. It has no CLI or GUI of its own - it is consumed as a library by
other Build applications.

## Install

Choose one supported binding:

```bash
pip install "build-ui[pyside6]>=2,<3"
# or
pip install "build-ui[pyqt6]>=2,<3"
```

Install exactly one binding extra. Plain `pip install build-ui` installs QtPy but no Qt binding,
which is useful when an application already supplies PySide6 or PyQt6. If both
bindings are present in an environment, set
`QT_API=pyside6` or `QT_API=pyqt6` before importing QtPy or Build UI. Do not
change `QT_API` after import, and do not mix objects from two bindings in one
process.

## Theme

```python
from build_ui.theme import C, STYLE, create_stylesheet

# Apply the shared stylesheet to a running QApplication
app.setStyleSheet(STYLE)

# C exposes color constants used throughout the stylesheet and widgets
print(C.ACCENT)  # '#d4a0a0'
print(C.BG)  # '#fdf9f5'


# Generate a themed variant by subclassing C and overriding constants
class DarkC(C):
    BG = "#161616"
    TEXT = "#f0f0f0"


dark_style = create_stylesheet(DarkC)
```

## Widgets

```python
from build_ui.widgets import Card, Heading, Stat, NavButton, Sidebar, StatusDot, ToastNotification

# Card: a styled surface container
card, layout = Card.with_layout()
layout.addWidget(Heading("Section title", level=1))

# Stat: a compact value + label display
stat = Stat("Uptime", "99.9%")
stat.set_value("100%", color="#38d39f")

# StatusDot: small colored indicator
dot = StatusDot(color="#92ad7e", size=12)
dot.set_color("#d08888")

# Sidebar: left navigation with pages
sidebar = Sidebar(["Dashboard", "Settings"], app_name="My App", app_version="1.0.0")
sidebar.page_changed.connect(lambda idx: print(f"switched to page {idx}"))

# ToastNotification: transient message
toast = ToastNotification("Saved successfully", level="success", parent=main_window)
toast.slide_in()
```

All widgets require a running `QApplication` to construct (standard Qt
behavior); the theme module can be imported and used with no display present.

## See also

- `README.md` - project overview.
- `ARCHITECTURE.md` - module responsibilities and design decisions.
- `MIGRATING.md` - Build UI 1 to 2 binding and environment guidance.
