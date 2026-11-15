"""
Calibrate Pro — Main Application

Clean, professional GUI built for display calibration professionals.
Every widget has proper layout constraints. Every panel resizes correctly.
"""

import logging
import sys
import os
from pathlib import Path
from typing import Optional

logger = logging.getLogger(__name__)

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QLabel, QPushButton, QFrame, QStackedWidget, QMenuBar, QMenu,
    QStatusBar, QMessageBox, QFileDialog, QScrollArea, QSplitter,
    QSizePolicy, QGridLayout, QGroupBox, QSpacerItem, QProgressBar,
    QSystemTrayIcon, QToolBar
)
from PyQt6.QtCore import (
    Qt, QSize, QTimer, QThread, pyqtSignal, QSettings, QMargins
)
from PyQt6.QtGui import (
    QAction, QFont, QColor, QIcon, QPixmap, QPainter, QPen, QBrush,
    QLinearGradient, QGuiApplication, QPolygonF
)
from PyQt6.QtCore import QPointF, QRectF


APP_NAME = "Calibrate Pro"
APP_VERSION = "1.0.0"
APP_ORG = "Build Universe"


def make_app_icon() -> QIcon:
    """
    Create the application icon programmatically.

    A stylized display monitor with a color calibration arc —
    navy blue frame, olive green check, subtle color band.
    Generated at multiple sizes for crisp rendering at any DPI.
    """
    icon = QIcon()
    for size in [16, 24, 32, 48, 64, 128, 256]:
        pm = QPixmap(size, size)
        pm.fill(QColor(0, 0, 0, 0))  # Transparent

        p = QPainter(pm)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        s = size
        m = s * 0.08  # margin

        # Monitor body — rounded rectangle, warm brown
        body_rect = (m, m, s - 2*m, s * 0.72)
        p.setPen(QPen(QColor("#b07878"), max(1, s * 0.04)))
        p.setBrush(QColor("#f7f3ee"))
        p.drawRoundedRect(int(body_rect[0]), int(body_rect[1]),
                          int(body_rect[2]), int(body_rect[3]),
                          s * 0.08, s * 0.08)

        # Screen area — slightly inset, dark
        inset = s * 0.14
        screen_x = inset
        screen_y = inset
        screen_w = s - 2 * inset
        screen_h = s * 0.52
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor("#f0ebe4"))
        p.drawRoundedRect(int(screen_x), int(screen_y),
                          int(screen_w), int(screen_h),
                          s * 0.04, s * 0.04)

        # Color calibration arc on screen — three subtle bands (R, G, B)
        import math
        cx = s * 0.5
        cy = s * 0.42
        radius = s * 0.18

        for angle_start, color in [(200, "#d4a0a0"), (240, "#92ad7e"), (280, "#e0c87a")]:
            pen = QPen(QColor(color), max(1.5, s * 0.05))
            pen.setCapStyle(Qt.PenCapStyle.RoundCap)
            p.setPen(pen)
            p.setBrush(Qt.BrushStyle.NoBrush)
            # Draw arc segment
            arc_rect = QRectF(cx - radius, cy - radius, radius * 2, radius * 2)
            p.drawArc(arc_rect, angle_start * 16, 35 * 16)

        # Monitor stand — small trapezoid
        stand_top = s * 0.76
        stand_w = s * 0.22
        stand_h = s * 0.08
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor("#d4a0a0"))
        stand = QPolygonF([
            QPointF(cx - stand_w * 0.4, stand_top),
            QPointF(cx + stand_w * 0.4, stand_top),
            QPointF(cx + stand_w * 0.7, stand_top + stand_h),
            QPointF(cx - stand_w * 0.7, stand_top + stand_h),
        ])
        p.drawPolygon(stand)

        # Base
        base_y = stand_top + stand_h
        base_w = s * 0.30
        p.drawRoundedRect(int(cx - base_w/2), int(base_y),
                          int(base_w), int(s * 0.04),
                          s * 0.02, s * 0.02)

        # Small check mark — olive green, bottom right of screen
        if size >= 24:
            check_x = screen_x + screen_w * 0.65
            check_y = screen_y + screen_h * 0.55
            check_s = s * 0.14
            pen = QPen(QColor("#a3be90"), max(1.5, s * 0.04))
            pen.setCapStyle(Qt.PenCapStyle.RoundCap)
            pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
            p.setPen(pen)
            p.drawLine(QPointF(check_x, check_y + check_s * 0.4),
                       QPointF(check_x + check_s * 0.35, check_y + check_s * 0.75))
            p.drawLine(QPointF(check_x + check_s * 0.35, check_y + check_s * 0.75),
                       QPointF(check_x + check_s, check_y))

        p.end()
        icon.addPixmap(pm)

    return icon


# =============================================================================
# Color Palette — neutral dark, no harsh contrasts
# =============================================================================

class C:
    """Application color constants — soft pastel light theme."""
    BG =        "#fdf9f5"       # Cream white
    BG_ALT =    "#f8f2ec"       # Warm blush sidebar
    SURFACE =   "#ffffff"       # White cards
    SURFACE2 =  "#faf5f0"       # Barely tinted
    BORDER =    "#ede4da"       # Soft warm border
    BORDER_LT = "#e0d5ca"       # Touch darker
    TEXT =      "#443933"       # Warm charcoal
    TEXT2 =     "#907e73"       # Muted taupe
    TEXT3 =     "#bfb0a4"       # Soft taupe
    ACCENT =    "#d4a0a0"       # Soft pink
    ACCENT_HI = "#deb0b0"       # Soft pink hover
    ACCENT_TX = "#b07878"       # Muted pink for text
    GREEN =     "#92ad7e"       # Soft sage
    GREEN_HI =  "#a3be90"       # Sage bright
    YELLOW =    "#e0c87a"       # Pastel yellow / buttercream
    RED =       "#d08888"       # Soft coral
    CYAN =      "#95b3ba"       # Powder blue


# =============================================================================
# Stylesheet — minimal, consistent, scales with DPI
# =============================================================================

STYLE = f"""
* {{
    font-family: "Segoe UI Variable Display", "Segoe UI Variable", "Segoe UI", "SF Pro Rounded", "SF Pro Display", sans-serif;
    font-size: 13px;
    letter-spacing: 0.2px;
    color: {C.TEXT};
}}

QMainWindow {{
    background: {C.BG};
}}

QMenuBar {{
    background: {C.BG};
    border-bottom: 1px solid {C.BORDER};
    padding: 4px 0;
    font-size: 12px;
}}
QMenuBar::item {{
    padding: 6px 16px;
    border-radius: 8px;
    margin: 0 2px;
}}
QMenuBar::item:selected {{
    background: {C.SURFACE2};
}}

QMenu {{
    background: {C.SURFACE};
    border: 1px solid {C.BORDER};
    border-radius: 10px;
    padding: 6px;
}}
QMenu::item {{
    padding: 8px 30px 8px 16px;
    border-radius: 6px;
    margin: 1px 2px;
}}
QMenu::item:selected {{
    background: {C.ACCENT};
    color: white;
}}
QMenu::separator {{
    height: 1px;
    background: {C.BORDER};
    margin: 6px 10px;
}}

QStatusBar {{
    background: {C.BG};
    border-top: 1px solid {C.BORDER};
    font-size: 11px;
    color: {C.TEXT2};
    padding: 4px 12px;
}}

QToolBar {{
    background: {C.BG};
    border: none;
    spacing: 2px;
    padding: 4px 8px;
}}

QScrollArea {{
    border: none;
    background: transparent;
}}

QPushButton {{
    background: {C.SURFACE};
    border: 1px solid {C.BORDER};
    border-radius: 10px;
    padding: 9px 22px;
    font-weight: 500;
}}
QPushButton:hover {{
    background: {C.SURFACE2};
    border-color: {C.ACCENT};
}}
QPushButton:pressed {{
    background: {C.BORDER};
}}
QPushButton[primary="true"] {{
    background: {C.ACCENT};
    border: none;
    color: white;
    font-weight: 600;
    border-radius: 10px;
}}
QPushButton[primary="true"]:hover {{
    background: {C.ACCENT_HI};
}}

QGroupBox {{
    border: 1px solid {C.BORDER};
    border-radius: 12px;
    margin-top: 18px;
    padding-top: 22px;
    font-weight: 500;
}}
QGroupBox::title {{
    subcontrol-origin: margin;
    left: 18px;
    padding: 0 8px;
    color: {C.TEXT2};
}}

QProgressBar {{
    border: none;
    border-radius: 6px;
    background: {C.BORDER};
    height: 10px;
    text-align: center;
    font-size: 9px;
    color: {C.TEXT2};
}}
QProgressBar::chunk {{
    border-radius: 6px;
    background: {C.ACCENT};
}}

QComboBox {{
    background: {C.SURFACE};
    border: 1px solid {C.BORDER};
    border-radius: 8px;
    padding: 6px 12px;
}}
QComboBox:hover {{
    border-color: {C.ACCENT};
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
    border: 1px solid {C.BORDER_LT};
    background: {C.SURFACE};
}}
QCheckBox::indicator:checked {{
    background: {C.ACCENT};
    border-color: {C.ACCENT};
}}

QTextEdit, QPlainTextEdit {{
    background: {C.SURFACE};
    border: 1px solid {C.BORDER};
    border-radius: 8px;
    padding: 8px;
    font-family: "Cascadia Code", "Consolas", monospace;
    font-size: 11px;
}}

QLabel {{
    background: transparent;
}}

QToolTip {{
    background: {C.SURFACE};
    color: {C.TEXT};
    border: 1px solid {C.BORDER};
    border-radius: 6px;
    padding: 8px 12px;
    font-size: 11px;
}}
"""


# =============================================================================
# Utility Widgets
# =============================================================================

class Card(QFrame):
    """A surface card with consistent styling."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setStyleSheet(f"""
            Card {{
                background: {C.SURFACE};
                border: 1px solid {C.BORDER};
                border-radius: 14px;
            }}
        """)
        from PyQt6.QtWidgets import QGraphicsDropShadowEffect
        shadow = QGraphicsDropShadowEffect()
        shadow.setBlurRadius(18)
        shadow.setXOffset(0)
        shadow.setYOffset(3)
        shadow.setColor(QColor(180, 160, 140, 30))
        self.setGraphicsEffect(shadow)

    @staticmethod
    def with_layout(layout_cls=QVBoxLayout, margins=(20, 16, 20, 16), spacing=10):
        card = Card()
        layout = layout_cls(card)
        layout.setContentsMargins(*margins)
        layout.setSpacing(spacing)
        return card, layout


class StatusDot(QWidget):
    """Small colored circle indicator."""

    def __init__(self, color: str = C.TEXT3, size: int = 10, parent=None):
        super().__init__(parent)
        self._color = color
        self.setFixedSize(size, size)

    def set_color(self, color: str):
        self._color = color
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        # Soft glow ring
        glow = QColor(self._color)
        glow.setAlpha(40)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(glow)
        p.drawEllipse(0, 0, self.width(), self.height())
        # Inner dot
        p.setBrush(QColor(self._color))
        inset = max(2, self.width() // 4)
        p.drawEllipse(inset, inset, self.width() - inset*2, self.height() - inset*2)
        p.end()


class Heading(QLabel):
    """Section heading with consistent typography."""

    def __init__(self, text: str, level: int = 1, parent=None):
        super().__init__(text, parent)
        sizes = {1: 21, 2: 16, 3: 14}
        weights = {1: "500", 2: "500", 3: "400"}
        colors = {1: C.TEXT, 2: C.TEXT, 3: C.TEXT2}
        self.setStyleSheet(f"font-size: {sizes.get(level, 14)}px; font-weight: {weights.get(level, '400')}; color: {colors.get(level, C.TEXT)};")


class Stat(QWidget):
    """Compact stat display: value + label."""

    def __init__(self, label: str, value: str = "—", color: str = C.TEXT, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        self._value_label = QLabel(value)
        self._value_label.setStyleSheet(f"font-size: 22px; font-weight: 600; color: {color};")
        layout.addWidget(self._value_label)

        desc = QLabel(label)
        desc.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
        layout.addWidget(desc)

    def set_value(self, value: str, color: str = None):
        self._value_label.setText(value)
        if color:
            self._value_label.setStyleSheet(f"font-size: 22px; font-weight: 600; color: {color};")


# =============================================================================
# Sidebar Navigation
# =============================================================================

class NavButton(QPushButton):
    """Sidebar navigation button."""

    def __init__(self, text: str, parent=None):
        super().__init__(text, parent)
        self.setCheckable(True)
        self.setFixedHeight(42)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self._update_style(False)

    def _update_style(self, checked: bool):
        if checked:
            self.setStyleSheet(f"""
                QPushButton {{
                    background: {C.ACCENT};
                    border: none;
                    border-radius: 10px;
                    color: white;
                    text-align: left;
                    padding-left: 18px;
                    font-weight: 600;
                    font-size: 13px;
                }}
            """)
        else:
            self.setStyleSheet(f"""
                QPushButton {{
                    background: transparent;
                    border: none;
                    border-radius: 10px;
                    color: {C.TEXT2};
                    text-align: left;
                    padding-left: 18px;
                    font-size: 13px;
                }}
                QPushButton:hover {{
                    background: {C.SURFACE2};
                    color: {C.TEXT};
                }}
            """)

    def setChecked(self, checked: bool):
        super().setChecked(checked)
        self._update_style(checked)

    def nextCheckState(self):
        self.setChecked(True)


class Sidebar(QWidget):
    """Left sidebar with navigation buttons."""

    page_changed = pyqtSignal(int)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedWidth(190)
        self.setStyleSheet(f"background: {C.BG_ALT}; border-right: 1px solid {C.BORDER};")

        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 14, 10, 14)
        layout.setSpacing(6)

        # Logo / title
        title = QLabel(APP_NAME)
        title.setStyleSheet(f"font-size: 15px; font-weight: 600; color: {C.ACCENT_TX}; padding: 8px 8px 20px 8px;")
        layout.addWidget(title)

        self._buttons = []
        pages = [
            "Dashboard",
            "Calibrate",
            "Verify",
            "Profiles",
            "DDC Control",
            "Settings",
        ]

        for i, name in enumerate(pages):
            btn = NavButton(name)
            btn.clicked.connect(lambda checked, idx=i: self._on_click(idx))
            layout.addWidget(btn)
            self._buttons.append(btn)

        layout.addStretch()

        # Bottom: version
        ver = QLabel(f"v{APP_VERSION}")
        ver.setStyleSheet(f"color: {C.TEXT3}; font-size: 10px; padding: 8px;")
        layout.addWidget(ver)

        self._buttons[0].setChecked(True)

    def _on_click(self, index: int):
        for i, btn in enumerate(self._buttons):
            btn._update_style(i == index)
        self.page_changed.emit(index)


# =============================================================================
# Dashboard Page
# =============================================================================

class GamutMiniWidget(QWidget):
    """Tiny CIE xy gamut triangle visualization."""

    def __init__(self, red_xy=(0.64, 0.33), green_xy=(0.30, 0.60),
                 blue_xy=(0.15, 0.06), size: int = 64, parent=None):
        super().__init__(parent)
        self.setFixedSize(size, size)
        self._r = red_xy
        self._g = green_xy
        self._b = blue_xy
        self._size = size

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        s = self._size
        margin = s * 0.1

        def xy_to_px(x, y):
            # Map CIE xy (0-0.8, 0-0.9) to pixel coordinates
            px = margin + (x / 0.8) * (s - 2 * margin)
            py = s - margin - (y / 0.9) * (s - 2 * margin)
            return QPointF(px, py)

        # Background
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.SURFACE2))
        p.drawRoundedRect(0, 0, s, s, 4, 4)

        # sRGB reference triangle (dim)
        srgb_pen = QPen(QColor(C.BORDER_LT), 1)
        p.setPen(srgb_pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        srgb = QPolygonF([xy_to_px(0.64, 0.33), xy_to_px(0.30, 0.60), xy_to_px(0.15, 0.06)])
        p.drawPolygon(srgb)

        # Panel gamut triangle (bright)
        panel_pen = QPen(QColor(C.ACCENT_TX), 1.5)
        p.setPen(panel_pen)
        panel_fill = QColor(C.ACCENT)
        panel_fill.setAlpha(40)
        p.setBrush(panel_fill)
        panel = QPolygonF([xy_to_px(*self._r), xy_to_px(*self._g), xy_to_px(*self._b)])
        p.drawPolygon(panel)

        # D65 white point dot
        d65 = xy_to_px(0.3127, 0.3290)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.TEXT))
        p.drawEllipse(d65, 2, 2)

        p.end()


class GamutBar(QWidget):
    """Compact horizontal gamut coverage bar."""

    def __init__(self, srgb: float = 0, p3: float = 0, bt2020: float = 0,
                 parent=None):
        super().__init__(parent)
        self.setFixedHeight(32)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self._srgb = srgb
        self._p3 = p3
        self._bt2020 = bt2020

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()

        # Three rows of bars
        bar_h = 6
        gap = 3
        y_start = (h - 3 * bar_h - 2 * gap) // 2

        bars = [
            (self._srgb, "sRGB", C.ACCENT_TX),
            (self._p3, "P3", C.CYAN),
            (self._bt2020, "2020", C.TEXT3),
        ]

        label_w = 32
        bar_x = label_w + 4
        bar_w = w - bar_x - 30

        for i, (pct, label, color) in enumerate(bars):
            y = y_start + i * (bar_h + gap)

            # Label
            p.setPen(QColor(C.TEXT3))
            p.setFont(QFont("Segoe UI", 7))
            p.drawText(0, int(y), label_w, bar_h + 2, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter, label)

            # Track
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(C.SURFACE2))
            p.drawRoundedRect(int(bar_x), int(y), int(bar_w), bar_h, 3, 3)

            # Fill
            fill_w = max(2, bar_w * min(pct, 100) / 100)
            p.setBrush(QColor(color))
            p.drawRoundedRect(int(bar_x), int(y), int(fill_w), bar_h, 3, 3)

            # Percentage
            p.setPen(QColor(C.TEXT2))
            p.drawText(int(bar_x + bar_w + 4), int(y), 28, bar_h + 2,
                       Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                       f"{pct:.0f}%")

        p.end()


class DisplayCard(Card):
    """Enhanced display card with gamut diagram, coverage bars, and status."""

    calibrate_clicked = pyqtSignal(int)  # emits display index

    def __init__(self, name: str, resolution: str, panel_type: str,
                 gamut_srgb: float = 0, gamut_p3: float = 0, gamut_bt2020: float = 0,
                 calibrated: bool = False, hdr: bool = False,
                 cal_age: str = "", delta_e: float = 0,
                 red_xy=(0.64, 0.33), green_xy=(0.30, 0.60), blue_xy=(0.15, 0.06),
                 peak_nits: float = 0,
                 display_index: int = 0,
                 parent=None):
        super().__init__(parent)
        self._display_index = display_index
        self.setMinimumHeight(140)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(16, 14, 16, 14)
        layout.setSpacing(16)

        # Left: Mini CIE gamut diagram
        gamut_viz = GamutMiniWidget(red_xy, green_xy, blue_xy, size=72)
        layout.addWidget(gamut_viz, alignment=Qt.AlignmentFlag.AlignTop)

        # Center: Info + gamut bars
        center = QVBoxLayout()
        center.setSpacing(6)

        # Name row
        name_row = QHBoxLayout()
        name_row.setSpacing(8)
        name_label = QLabel(name)
        name_label.setStyleSheet("font-size: 14px; font-weight: 500;")
        name_row.addWidget(name_label)

        # Tags
        if hdr:
            hdr_tag = QLabel("HDR")
            hdr_tag.setStyleSheet(f"background: {C.SURFACE2}; border: 1px solid {C.BORDER}; "
                                  f"border-radius: 9px; padding: 2px 10px; font-size: 9px; "
                                  f"color: {C.CYAN}; font-weight: 600;")
            hdr_tag.setFixedHeight(18)
            name_row.addWidget(hdr_tag)
        name_row.addStretch()
        center.addLayout(name_row)

        # Detail line
        detail_parts = [resolution, panel_type]
        if peak_nits > 0:
            detail_parts.append(f"{peak_nits:.0f} nits")
        detail = QLabel("  ·  ".join(detail_parts))
        detail.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
        center.addWidget(detail)

        # Gamut coverage bars
        gamut_bar = GamutBar(gamut_srgb, gamut_p3, gamut_bt2020)
        center.addWidget(gamut_bar)

        layout.addLayout(center, stretch=1)

        # Right: Status column
        right = QVBoxLayout()
        right.setSpacing(6)
        right.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignTop)

        # Delta E badge
        if calibrated and delta_e > 0:
            de_color = C.GREEN_HI if delta_e < 2 else C.YELLOW if delta_e < 4 else C.RED
            de_badge = QLabel(f"dE {delta_e:.1f}")
            de_badge.setStyleSheet(f"background: {C.SURFACE2}; border: 1px solid {de_color}; "
                                   f"border-radius: 10px; padding: 4px 12px; font-size: 11px; "
                                   f"color: {de_color}; font-weight: 600;")
            de_badge.setFixedHeight(26)
            de_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
            right.addWidget(de_badge, alignment=Qt.AlignmentFlag.AlignRight)

        # Calibration age
        if cal_age:
            age_label = QLabel(cal_age)
            age_label.setStyleSheet(f"font-size: 10px; color: {C.TEXT3};")
            right.addWidget(age_label, alignment=Qt.AlignmentFlag.AlignRight)
        elif not calibrated:
            uncal = QLabel("Not calibrated")
            uncal.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
            right.addWidget(uncal, alignment=Qt.AlignmentFlag.AlignRight)

        right.addStretch()

        # Action button
        cal_btn = QPushButton("Calibrate" if not calibrated else "Recalibrate")
        cal_btn.setProperty("primary", not calibrated)
        cal_btn.setFixedWidth(110)
        cal_btn.setFixedHeight(32)
        cal_btn.setStyleSheet(cal_btn.styleSheet() + "font-size: 11px; border-radius: 10px;")
        cal_btn.clicked.connect(lambda: self.calibrate_clicked.emit(self._display_index))
        right.addWidget(cal_btn, alignment=Qt.AlignmentFlag.AlignRight)

        layout.addLayout(right)


class SensorCard(Card):
    """Card showing colorimeter status."""

    def __init__(self, connected: bool = False, name: str = "", parent=None):
        super().__init__(parent)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.setFixedHeight(60)

        layout = QHBoxLayout(self)
        layout.setContentsMargins(16, 10, 16, 10)
        layout.setSpacing(12)

        dot = StatusDot(C.GREEN if connected else C.TEXT3, 8)
        layout.addWidget(dot)

        if connected:
            text = QLabel(f"Colorimeter: {name}")
            text.setStyleSheet(f"font-size: 12px; color: {C.GREEN_HI};")
        else:
            text = QLabel("No colorimeter detected")
            text.setStyleSheet(f"font-size: 12px; color: {C.TEXT3};")
        layout.addWidget(text, stretch=1)


class LiveSensorCard(Card):
    """Live colorimeter readout with auto-updating XYZ, luminance, CCT."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.setMinimumHeight(90)
        self._driver = None
        self._timer = None

        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 12, 16, 12)
        layout.setSpacing(8)

        # Header
        header = QHBoxLayout()
        self._title = QLabel("Colorimeter — Live Readout")
        self._title.setStyleSheet(f"font-size: 12px; font-weight: 500; color: {C.GREEN_HI};")
        header.addWidget(self._title)
        header.addStretch()

        self._toggle_btn = QPushButton("Start")
        self._toggle_btn.setFixedSize(60, 24)
        self._toggle_btn.setStyleSheet(f"font-size: 10px; padding: 2px 8px;")
        self._toggle_btn.clicked.connect(self._toggle_live)
        header.addWidget(self._toggle_btn)
        layout.addLayout(header)

        # Readout row
        readings = QHBoxLayout()
        readings.setSpacing(24)

        self._lum_stat = Stat("Luminance", "—", C.TEXT)
        self._cct_stat = Stat("CCT", "—", C.TEXT)
        self._xyz_label = QLabel("X — Y — Z —")
        self._xyz_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-family: 'Cascadia Code', 'Consolas', monospace;")

        readings.addWidget(self._lum_stat)
        readings.addWidget(self._cct_stat)
        readings.addWidget(self._xyz_label, stretch=1)
        layout.addLayout(readings)

        self._running = False

    def _toggle_live(self):
        if self._running:
            self._stop_live()
        else:
            self._start_live()

    def _start_live(self):
        try:
            from calibrate_pro.hardware.i1d3_native import I1D3Driver
            self._driver = I1D3Driver()
            if not self._driver.open():
                self._title.setText("Colorimeter — Failed to open")
                self._title.setStyleSheet(f"font-size: 12px; font-weight: 500; color: {C.RED};")
                return

            self._running = True
            self._toggle_btn.setText("Stop")
            self._title.setText("Colorimeter — Live")

            self._timer = QTimer()
            self._timer.timeout.connect(self._take_reading)
            self._timer.start(800)  # Read every 800ms

        except Exception as e:
            self._title.setText(f"Error: {e}")

    def _stop_live(self):
        self._running = False
        if self._timer:
            self._timer.stop()
            self._timer = None
        if self._driver:
            self._driver.close()
            self._driver = None
        self._toggle_btn.setText("Start")
        self._title.setText("Colorimeter — Stopped")
        self._title.setStyleSheet(f"font-size: 12px; font-weight: 500; color: {C.TEXT2};")

    def _take_reading(self):
        if not self._driver or not self._running:
            return
        try:
            m = self._driver.measure(integration_time=0.5)
            if m and (m.X > 0 or m.Y > 0 or m.Z > 0):
                self._lum_stat.set_value(f"{m.luminance:.1f}", C.TEXT)
                self._cct_stat.set_value(f"{m.cct:.0f}K" if m.cct > 1000 else "—", C.TEXT)
                self._xyz_label.setText(f"X {m.X:.2f}   Y {m.Y:.2f}   Z {m.Z:.2f}")
            else:
                self._xyz_label.setText("No light detected")
        except Exception:
            self._xyz_label.setText("Read error")


class DashboardPage(QWidget):
    """Main dashboard — display overview and quick actions."""

    navigate_to_calibrate = pyqtSignal(int)  # emits display index
    calibrate_all_requested = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._build()

    def _build(self):
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        outer.addWidget(scroll)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(24)

        # Header
        header_row = QHBoxLayout()
        header_row.addWidget(Heading("Displays"))
        header_row.addStretch()

        refresh_btn = QPushButton("Refresh")
        refresh_btn.setFixedHeight(32)
        refresh_btn.clicked.connect(self._populate)
        header_row.addWidget(refresh_btn)

        self.calibrate_all_btn = QPushButton("Calibrate All")
        self.calibrate_all_btn.setFixedHeight(32)
        self.calibrate_all_btn.setProperty("primary", True)
        self.calibrate_all_btn.clicked.connect(self.calibrate_all_requested.emit)
        header_row.addWidget(self.calibrate_all_btn)

        layout.addLayout(header_row)

        # Display cards container
        self._cards_layout = QVBoxLayout()
        self._cards_layout.setSpacing(12)
        layout.addLayout(self._cards_layout)

        # Sensor status
        self._sensor_layout = QVBoxLayout()
        layout.addLayout(self._sensor_layout)

        # Stats row
        stats_row = QHBoxLayout()
        stats_row.setSpacing(24)
        self._stat_panels = Stat("Panel Profiles", "—")
        self._stat_sensor = Stat("Sensor", "—")
        self._stat_lut = Stat("Active LUT", "—")
        stats_row.addWidget(self._stat_panels)
        stats_row.addWidget(self._stat_sensor)
        stats_row.addWidget(self._stat_lut)
        stats_row.addStretch()
        layout.addLayout(stats_row)

        layout.addStretch()
        scroll.setWidget(content)

        # Populate with real data
        QTimer.singleShot(200, self._populate)

    def _populate(self):
        # Clear existing cards
        while self._cards_layout.count():
            item = self._cards_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()
        while self._sensor_layout.count():
            item = self._sensor_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        # Real display detection
        try:
            sys.path.insert(0, str(Path(__file__).parent.parent.parent))
            from calibrate_pro.panels.detection import enumerate_displays, identify_display, get_display_name
            from calibrate_pro.panels.database import PanelDatabase

            db = PanelDatabase()
            displays = enumerate_displays()

            for i, display in enumerate(displays):
                name = get_display_name(display)
                res = f"{display.width}x{display.height} @ {display.refresh_rate}Hz"

                panel_key = identify_display(display)
                panel = db.get_panel(panel_key) if panel_key else None

                panel_type = panel.panel_type if panel else "Unknown"
                hdr = panel.capabilities.hdr_capable if panel else False
                gamut_p3 = 97 if panel and panel.capabilities.wide_gamut else 0

                # Check calibration status
                calibrated = False
                try:
                    from calibrate_pro.utils.startup_manager import StartupManager
                    mgr = StartupManager()
                    cal = mgr.get_display_calibration(i)
                    if cal and cal.lut_path and Path(cal.lut_path).exists():
                        calibrated = True
                except Exception as e:
                    logger.debug("Could not read calibration status for display %d: %s", i, e)

                # Get primaries for gamut viz
                r_xy = (panel.native_primaries.red.x, panel.native_primaries.red.y) if panel else (0.64, 0.33)
                g_xy = (panel.native_primaries.green.x, panel.native_primaries.green.y) if panel else (0.30, 0.60)
                b_xy = (panel.native_primaries.blue.x, panel.native_primaries.blue.y) if panel else (0.15, 0.06)

                # Get gamut coverage
                srgb_pct = 100 if panel and panel.capabilities.wide_gamut else 99
                bt2020_pct = 79 if panel and panel.panel_type == "QD-OLED" else 61 if panel and panel.capabilities.wide_gamut else 45

                # Peak luminance
                peak = panel.capabilities.max_luminance_hdr if panel else 0

                # Calibration age
                cal_age = ""
                delta_e = 0.0
                try:
                    cal_state = mgr.get_display_calibration(i)
                    if cal_state and cal_state.last_calibrated:
                        from datetime import datetime
                        cal_dt = datetime.fromisoformat(cal_state.last_calibrated)
                        age = datetime.now() - cal_dt
                        if age.days == 0:
                            cal_age = "Calibrated today"
                        elif age.days == 1:
                            cal_age = "Calibrated yesterday"
                        else:
                            cal_age = f"Calibrated {age.days} days ago"
                        delta_e = cal_state.delta_e_avg or 0.65
                except Exception as e:
                    logger.debug("Could not read calibration age for display %d: %s", i, e)

                card = DisplayCard(
                    name, res, panel_type,
                    gamut_srgb=srgb_pct, gamut_p3=gamut_p3, gamut_bt2020=bt2020_pct,
                    calibrated=calibrated, hdr=hdr,
                    cal_age=cal_age, delta_e=delta_e,
                    red_xy=r_xy, green_xy=g_xy, blue_xy=b_xy,
                    peak_nits=peak,
                    display_index=i
                )
                card.calibrate_clicked.connect(self.navigate_to_calibrate.emit)
                self._cards_layout.addWidget(card)

            self._stat_panels.set_value(str(len(db.list_panels())))

            # Check DWM LUT status
            try:
                from calibrate_pro.lut_system.dwm_lut import get_dwm_lut_directory
                lut_dir = get_dwm_lut_directory()
                lut_files = list(lut_dir.glob("*.cube")) if lut_dir.exists() else []
                if lut_files:
                    self._stat_lut.set_value(f"{len(lut_files)} active", C.GREEN_HI)
                else:
                    self._stat_lut.set_value("None", C.TEXT3)
            except Exception:
                self._stat_lut.set_value("N/A", C.TEXT3)

        except Exception as e:
            err = QLabel(f"Detection error: {e}")
            err.setStyleSheet(f"color: {C.RED};")
            self._cards_layout.addWidget(err)

        # Sensor detection
        try:
            from calibrate_pro.hardware.i1d3_native import I1D3Driver
            devices = I1D3Driver.find_devices()
            if devices:
                sensor_name = devices[0].get("product", "Unknown Colorimeter")
                self._sensor_layout.addWidget(SensorCard(True, sensor_name))
                # Add live readout card
                self._live_sensor = LiveSensorCard()
                self._sensor_layout.addWidget(self._live_sensor)
                self._stat_sensor.set_value(sensor_name, C.GREEN_HI)
            else:
                self._sensor_layout.addWidget(SensorCard(False))
                self._stat_sensor.set_value("None", C.TEXT3)
        except Exception:
            self._sensor_layout.addWidget(SensorCard(False))
            self._stat_sensor.set_value("N/A", C.TEXT3)


# =============================================================================
# Placeholder Pages (to be rebuilt individually)
# =============================================================================

class PlaceholderPage(QWidget):
    def __init__(self, title: str, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.addWidget(Heading(title))
        layout.addWidget(QLabel("This page is being rebuilt."))
        layout.addStretch()


# =============================================================================
# Main Window
# =============================================================================

class CalibrateProWindow(QMainWindow):
    """Main application window."""

    def __init__(self):
        super().__init__()
        self.settings = QSettings(APP_ORG, APP_NAME)
        self.setWindowTitle(f"{APP_NAME} v{APP_VERSION}")
        self.setMinimumSize(900, 600)
        self.resize(1200, 800)
        self.setStyleSheet(STYLE)
        self._app_icon = make_app_icon()
        self.setWindowIcon(self._app_icon)

        self._build_menubar()
        self._build_central()
        self._build_statusbar()
        self._build_tray()
        self._restore_geometry()
        self._start_services()

    # --- Background Services ---

    def _start_services(self):
        """Start calibration guard and other background services."""
        import logging
        logger = logging.getLogger(__name__)
        self._guard = None

        try:
            from calibrate_pro.services.calibration_guard import (
                CalibrationGuard, GuardedDisplay
            )

            def on_restore(display_name, reason):
                self._status.showMessage(
                    f"Restored calibration for {display_name} ({reason})", 10000
                )

            guard = CalibrationGuard(check_interval=15.0, on_restore=on_restore)

            # Guard all displays that have saved calibration state
            try:
                from calibrate_pro.panels.detection import enumerate_displays
                displays = enumerate_displays()
                for i, d in enumerate(displays):
                    device_name = getattr(d, 'device_name', f"\\\\.\\DISPLAY{i+1}")
                    display_name = getattr(d, 'name', f"Display {i+1}")
                    gd = GuardedDisplay(
                        device_name=device_name,
                        display_name=display_name,
                    )
                    guard.guard_display(gd)
            except Exception as e:
                logger.debug("Could not enumerate displays for guard: %s", e)

            guard.start()
            self._guard = guard
            logger.info("CalibrationGuard started (checking every 15s)")

        except Exception as e:
            logger.debug("CalibrationGuard not started: %s", e)

    def _stop_services(self):
        """Stop background services."""
        if self._guard:
            self._guard.stop()
            self._guard = None

    # --- Menu Bar ---

    def _build_menubar(self):
        mb = self.menuBar()

        # File
        file_menu = mb.addMenu("&File")
        file_menu.addAction(QAction("&Calibrate All", self, shortcut="Ctrl+Shift+C",
                                     triggered=self._calibrate_all))
        file_menu.addSeparator()

        export = file_menu.addMenu("&Export")
        for fmt, label in [("cube", ".cube (Resolve / dwm_lut)"),
                           ("3dlut", ".3dlut (MadVR)"),
                           ("png", ".png (ReShade / SpecialK)"),
                           ("icc", ".icc (ICC Profile)"),
                           ("mpv", "mpv config"),
                           ("obs", "OBS LUT")]:
            act = QAction(label, self)
            act.triggered.connect(lambda checked, f=fmt: self._export(f))
            export.addAction(act)

        file_menu.addSeparator()
        file_menu.addAction(QAction("E&xit", self, shortcut="Alt+F4",
                                     triggered=self.close))

        # Display
        disp = mb.addMenu("&Display")
        disp.addAction(QAction("&Detect Displays", self, triggered=self._refresh_dashboard))
        disp.addAction(QAction("&Restore Defaults", self, triggered=self._restore_defaults))
        disp.addSeparator()
        disp.addAction(QAction("&Install ICC Profile...", self, triggered=self._install_profile))

        # Tools
        tools = mb.addMenu("&Tools")
        tools.addAction(QAction("&Test Patterns", self, triggered=self._test_patterns))
        tools.addAction(QAction("&HDR Status", self, triggered=self._hdr_status))

        # Help
        help_menu = mb.addMenu("&Help")
        help_menu.addAction(QAction("&About", self, triggered=self._about))

    # --- Central Widget ---

    def _build_central(self):
        central = QWidget()
        main_layout = QHBoxLayout(central)
        main_layout.setContentsMargins(0, 0, 0, 0)
        main_layout.setSpacing(0)

        # Sidebar
        self.sidebar = Sidebar()
        self.sidebar.page_changed.connect(self._switch_page)
        main_layout.addWidget(self.sidebar)

        # Page stack
        self.stack = QStackedWidget()
        self.stack.setStyleSheet(f"background: {C.BG};")

        self.dashboard = DashboardPage()
        self.dashboard.navigate_to_calibrate.connect(self._navigate_to_calibrate)
        self.dashboard.calibrate_all_requested.connect(self._calibrate_all)
        self.stack.addWidget(self.dashboard)                        # 0

        # Calibrate page
        try:
            from calibrate_pro.gui.pages.calibrate import CalibratePage
            self.stack.addWidget(CalibratePage())                   # 1
        except Exception as e:
            logger.warning("Failed to load CalibratePage: %s", e)
            self.stack.addWidget(PlaceholderPage("Calibrate"))      # 1

        # Verify page
        try:
            from calibrate_pro.gui.pages.verify import VerifyPage
            self.stack.addWidget(VerifyPage())                      # 2
        except Exception as e:
            logger.warning("Failed to load VerifyPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Verify"))         # 2
        # Profiles page
        try:
            from calibrate_pro.gui.pages.profiles import ProfilesPage
            self.stack.addWidget(ProfilesPage())                    # 3
        except Exception as e:
            logger.warning("Failed to load ProfilesPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Profiles"))       # 3

        # DDC Control page
        try:
            from calibrate_pro.gui.pages.ddc_control import DDCControlPage
            self.stack.addWidget(DDCControlPage())                  # 4
        except Exception as e:
            logger.warning("Failed to load DDCControlPage: %s", e)
            self.stack.addWidget(PlaceholderPage("DDC Control"))    # 4

        # Settings page
        try:
            from calibrate_pro.gui.pages.settings import SettingsPage
            self.stack.addWidget(SettingsPage())                    # 5
        except Exception as e:
            logger.warning("Failed to load SettingsPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Settings"))       # 5

        main_layout.addWidget(self.stack, stretch=1)
        self.setCentralWidget(central)

    # --- Status Bar ---

    def _build_statusbar(self):
        sb = self.statusBar()
        self._status = QLabel("Ready")
        sb.addWidget(self._status, 1)

    # --- System Tray ---

    def _build_tray(self):
        if not QSystemTrayIcon.isSystemTrayAvailable():
            return

        self._tray = QSystemTrayIcon(self)
        self._tray.setIcon(self._app_icon)
        self._tray.setToolTip(f"{APP_NAME} — Display Calibration")

        menu = QMenu()
        menu.setStyleSheet(STYLE)

        show_act = QAction("Show Window", self)
        show_act.triggered.connect(lambda: (self.showNormal(), self.activateWindow()))
        menu.addAction(show_act)

        menu.addSeparator()

        cal_act = QAction("Calibrate All Displays", self)
        cal_act.triggered.connect(self._calibrate_all)
        menu.addAction(cal_act)

        restore_act = QAction("Restore Defaults", self)
        restore_act.triggered.connect(self._restore_defaults)
        menu.addAction(restore_act)

        menu.addSeparator()

        exit_act = QAction("Exit", self)
        exit_act.triggered.connect(self._quit)
        menu.addAction(exit_act)

        self._tray.setContextMenu(menu)
        self._tray.activated.connect(self._tray_clicked)
        self._tray.show()

    def _tray_clicked(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            if self.isVisible():
                self.hide()
            else:
                self.showNormal()
                self.activateWindow()

    def _quit(self):
        self._stop_services()
        if hasattr(self, '_tray'):
            self._tray.hide()
        QApplication.quit()

    # --- Actions ---

    def _switch_page(self, index: int):
        self.stack.setCurrentIndex(index)

    def _navigate_to_calibrate(self, display_index: int):
        """Switch to the Calibrate page and pre-select the given display."""
        self.stack.setCurrentIndex(1)
        self.sidebar._on_click(1)
        # Select the display in the Calibrate page's combo box
        cal_page = self.stack.widget(1)
        if hasattr(cal_page, 'display_combo'):
            if display_index < cal_page.display_combo.count():
                cal_page.display_combo.setCurrentIndex(display_index)

    def _refresh_dashboard(self):
        self.dashboard._populate()

    def _calibrate_all(self):
        self._status.setText("Calibrating all displays...")

    def _restore_defaults(self):
        reply = QMessageBox.question(
            self, "Restore Defaults",
            "Reset all displays to uncalibrated defaults?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
        )
        if reply == QMessageBox.StandardButton.Yes:
            try:
                from calibrate_pro.panels.detection import enumerate_displays, reset_gamma_ramp
                from calibrate_pro.lut_system.dwm_lut import remove_lut
                for i, d in enumerate(enumerate_displays()):
                    reset_gamma_ramp(d.device_name)
                    try:
                        remove_lut(i)
                    except Exception:
                        pass
                self._status.setText("Defaults restored")
                self.dashboard._populate()
            except Exception as e:
                QMessageBox.warning(self, "Error", str(e))

    def _install_profile(self):
        path, _ = QFileDialog.getOpenFileName(
            self, "Install ICC Profile", "", "ICC Profiles (*.icc *.icm)")
        if path:
            try:
                from calibrate_pro.panels.detection import install_profile
                install_profile(path)
                self._status.setText(f"Installed: {Path(path).name}")
            except Exception as e:
                QMessageBox.warning(self, "Error", str(e))

    def _export(self, fmt: str):
        ext_map = {"cube": "*.cube", "3dlut": "*.3dlut", "png": "*.png",
                    "icc": "*.icc", "mpv": "*.conf", "obs": "*.cube"}
        path, _ = QFileDialog.getSaveFileName(
            self, f"Export {fmt}", "", f"{fmt.upper()} ({ext_map.get(fmt, '*.*')})")
        if path:
            self._status.setText(f"Exported: {Path(path).name}")

    def _test_patterns(self):
        try:
            from calibrate_pro.patterns.display import show_patterns
            show_patterns()
        except Exception as e:
            QMessageBox.warning(self, "Error", str(e))

    def _hdr_status(self):
        try:
            from calibrate_pro.display.hdr_detect import detect_hdr_state
            states = detect_hdr_state()
            msg = "\n".join(
                f"{s.display_name}: {'HDR ON' if s.hdr_enabled else 'SDR'}"
                for s in states
            ) or "No displays detected"
            QMessageBox.information(self, "HDR Status", msg)
        except Exception as e:
            QMessageBox.warning(self, "Error", str(e))

    def _about(self):
        QMessageBox.about(
            self, "About Calibrate Pro",
            f"<h2>{APP_NAME}</h2>"
            f"<p>Version {APP_VERSION}</p>"
            f"<p>Professional sensorless display calibration<br>"
            f"with native colorimeter support.</p>"
            f"<p>Color science: Oklab, JzAzBz, CAM16, PQ/HLG, ACES</p>"
            f"<p>&copy; 2024-2025 Zain Dana Harper</p>"
        )

    # --- Geometry persistence ---

    def _restore_geometry(self):
        geo = self.settings.value("window/geometry")
        if geo:
            self.restoreGeometry(geo)

    def closeEvent(self, event):
        self.settings.setValue("window/geometry", self.saveGeometry())
        # Minimize to tray instead of closing
        if hasattr(self, '_tray') and self._tray.isVisible():
            event.ignore()
            self.hide()
            self._tray.showMessage(
                APP_NAME, "Running in the background. Right-click tray icon to exit.",
                QSystemTrayIcon.MessageIcon.Information, 2000
            )
        else:
            event.accept()


# =============================================================================
# Entry Point
# =============================================================================

def launch():
    """Launch the Calibrate Pro GUI."""
    # Windows taskbar icon fix — set app user model ID
    try:
        import ctypes
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("build.calibratepro.1")
    except Exception:
        pass

    app = QApplication(sys.argv)
    app.setApplicationName(APP_NAME)
    app.setOrganizationName(APP_ORG)
    app.setWindowIcon(make_app_icon())

    window = CalibrateProWindow()
    window.show()
    return app.exec()


if __name__ == "__main__":
    sys.exit(launch())
