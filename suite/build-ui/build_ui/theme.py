"""Build ecosystem shared color theme and stylesheet."""


class C:
    """Application color constants -- warm pastel light theme."""

    BG = "#fdf9f5"
    BG_ALT = "#f8f2ec"
    SURFACE = "#ffffff"
    SURFACE2 = "#faf5f0"
    BORDER = "#ede4da"
    BORDER_LT = "#e0d5ca"
    TEXT = "#443933"
    TEXT2 = "#907e73"
    TEXT3 = "#bfb0a4"
    ACCENT = "#d4a0a0"
    ACCENT_HI = "#deb0b0"
    ACCENT_TX = "#b07878"
    GREEN = "#92ad7e"
    GREEN_HI = "#a3be90"
    YELLOW = "#e0c87a"
    RED = "#d08888"
    CYAN = "#95b3ba"


def create_stylesheet(c=None):
    """Generate the ecosystem stylesheet using given color class (defaults to C)."""
    if c is None:
        c = C
    return f"""
* {{
    font-family: "Segoe UI Variable Display", "Segoe UI Variable", "Segoe UI", "SF Pro Rounded", "SF Pro Display", sans-serif;
    font-size: 13px;
    letter-spacing: 0.2px;
    color: {c.TEXT};
}}

QMainWindow {{
    background: {c.BG};
}}

QMenuBar {{
    background: {c.BG};
    border-bottom: 1px solid {c.BORDER};
    padding: 4px 0;
    font-size: 12px;
}}
QMenuBar::item {{
    padding: 6px 16px;
    border-radius: 8px;
    margin: 0 2px;
}}
QMenuBar::item:selected {{
    background: {c.SURFACE2};
}}

QMenu {{
    background: {c.SURFACE};
    border: 1px solid {c.BORDER};
    border-radius: 10px;
    padding: 6px;
}}
QMenu::item {{
    padding: 8px 30px 8px 16px;
    border-radius: 6px;
    margin: 1px 2px;
}}
QMenu::item:selected {{
    background: {c.ACCENT};
    color: white;
}}
QMenu::separator {{
    height: 1px;
    background: {c.BORDER};
    margin: 6px 10px;
}}

QStatusBar {{
    background: {c.BG};
    border-top: 1px solid {c.BORDER};
    font-size: 11px;
    color: {c.TEXT2};
    padding: 4px 12px;
}}

QToolBar {{
    background: {c.BG};
    border: none;
    spacing: 2px;
    padding: 4px 8px;
}}

QScrollArea {{
    border: none;
    background: transparent;
}}

QPushButton {{
    background: {c.SURFACE};
    border: 1px solid {c.BORDER};
    border-radius: 10px;
    padding: 9px 22px;
    font-weight: 500;
}}
QPushButton:hover {{
    background: {c.SURFACE2};
    border-color: {c.ACCENT};
}}
QPushButton:pressed {{
    background: {c.BORDER};
}}
QPushButton[primary="true"] {{
    background: {c.ACCENT};
    border: none;
    color: white;
    font-weight: 600;
    border-radius: 10px;
}}
QPushButton[primary="true"]:hover {{
    background: {c.ACCENT_HI};
}}

QGroupBox {{
    border: 1px solid {c.BORDER};
    border-radius: 12px;
    margin-top: 18px;
    padding-top: 22px;
    font-weight: 500;
}}
QGroupBox::title {{
    subcontrol-origin: margin;
    left: 18px;
    padding: 0 8px;
    color: {c.TEXT2};
}}

QProgressBar {{
    border: none;
    border-radius: 6px;
    background: {c.BORDER};
    height: 10px;
    text-align: center;
    font-size: 9px;
    color: {c.TEXT2};
}}
QProgressBar::chunk {{
    border-radius: 6px;
    background: {c.ACCENT};
}}

QComboBox {{
    background: {c.SURFACE};
    border: 1px solid {c.BORDER};
    border-radius: 8px;
    padding: 6px 12px;
}}
QComboBox:hover {{
    border-color: {c.ACCENT};
}}
QComboBox::drop-down {{
    border: none;
    width: 24px;
}}

QCheckBox {{
    spacing: 8px;
}}
QCheckBox::indicator {{
    width: 18px;
    height: 18px;
    border-radius: 5px;
    border: 1px solid {c.BORDER_LT};
    background: {c.SURFACE};
}}
QCheckBox::indicator:checked {{
    background: {c.ACCENT};
    border-color: {c.ACCENT};
}}

QSpinBox {{
    background: {c.SURFACE};
    border: 1px solid {c.BORDER};
    border-radius: 8px;
    padding: 6px 12px;
}}
QSpinBox:hover {{
    border-color: {c.ACCENT};
}}

QSlider::groove:horizontal {{
    border: none;
    height: 6px;
    background: {c.BORDER};
    border-radius: 3px;
}}
QSlider::handle:horizontal {{
    background: {c.ACCENT};
    border: none;
    width: 16px;
    height: 16px;
    margin: -5px 0;
    border-radius: 8px;
}}
QSlider::handle:horizontal:hover {{
    background: {c.ACCENT_HI};
}}
QSlider::sub-page:horizontal {{
    background: {c.ACCENT};
    border-radius: 3px;
}}

QLineEdit {{
    background: {c.SURFACE};
    border: 1px solid {c.BORDER};
    border-radius: 8px;
    padding: 6px 12px;
}}
QLineEdit:hover {{
    border-color: {c.ACCENT};
}}
QLineEdit:focus {{
    border-color: {c.ACCENT};
}}

QTextEdit, QPlainTextEdit {{
    background: {c.SURFACE};
    border: 1px solid {c.BORDER};
    border-radius: 8px;
    padding: 8px;
    font-family: "Cascadia Code", "Consolas", monospace;
    font-size: 11px;
}}

QLabel {{
    background: transparent;
}}

QToolTip {{
    background: {c.SURFACE};
    color: {c.TEXT};
    border: 1px solid {c.BORDER};
    border-radius: 6px;
    padding: 8px 12px;
    font-size: 11px;
}}
"""


STYLE = create_stylesheet()
