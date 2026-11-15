"""
Color Swatch Widget

Displays color patches with:
- RGB/Lab/XYZ values
- Target vs measured comparison
- Delta E indicator
"""

from typing import Optional, Tuple
from dataclasses import dataclass
import math

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QFrame,
    QGridLayout, QSizePolicy
)
from PyQt6.QtCore import Qt, QRectF, pyqtSignal
from PyQt6.QtGui import (
    QPainter, QPen, QBrush, QColor, QFont, QLinearGradient
)


# =============================================================================
# Color Conversion Utilities
# =============================================================================

def rgb_to_xyz(r: int, g: int, b: int) -> Tuple[float, float, float]:
    """Convert sRGB to XYZ (D65)."""
    # Normalize to 0-1
    r = r / 255.0
    g = g / 255.0
    b = b / 255.0

    # Apply sRGB gamma
    r = ((r + 0.055) / 1.055) ** 2.4 if r > 0.04045 else r / 12.92
    g = ((g + 0.055) / 1.055) ** 2.4 if g > 0.04045 else g / 12.92
    b = ((b + 0.055) / 1.055) ** 2.4 if b > 0.04045 else b / 12.92

    # sRGB to XYZ matrix
    X = r * 0.4124564 + g * 0.3575761 + b * 0.1804375
    Y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750
    Z = r * 0.0193339 + g * 0.1191920 + b * 0.9503041

    return (X * 100, Y * 100, Z * 100)


def xyz_to_lab(X: float, Y: float, Z: float) -> Tuple[float, float, float]:
    """Convert XYZ to CIELAB (D65 white point)."""
    # D65 reference white
    Xn, Yn, Zn = 95.047, 100.000, 108.883

    def f(t):
        delta = 6 / 29
        if t > delta ** 3:
            return t ** (1/3)
        return t / (3 * delta ** 2) + 4 / 29

    fx = f(X / Xn)
    fy = f(Y / Yn)
    fz = f(Z / Zn)

    L = 116 * fy - 16
    a = 500 * (fx - fy)
    b = 200 * (fy - fz)

    return (L, a, b)


def rgb_to_lab(r: int, g: int, b: int) -> Tuple[float, float, float]:
    """Convert RGB to CIELAB."""
    X, Y, Z = rgb_to_xyz(r, g, b)
    return xyz_to_lab(X, Y, Z)


def delta_e_2000(lab1: Tuple[float, float, float],
                 lab2: Tuple[float, float, float]) -> float:
    """Calculate CIEDE2000 color difference."""
    L1, a1, b1 = lab1
    L2, a2, b2 = lab2

    # Mean L
    L_bar = (L1 + L2) / 2

    # C values
    C1 = math.sqrt(a1 ** 2 + b1 ** 2)
    C2 = math.sqrt(a2 ** 2 + b2 ** 2)
    C_bar = (C1 + C2) / 2

    # G factor
    G = 0.5 * (1 - math.sqrt(C_bar ** 7 / (C_bar ** 7 + 25 ** 7)))

    # a' values
    a1_prime = a1 * (1 + G)
    a2_prime = a2 * (1 + G)

    # C' values
    C1_prime = math.sqrt(a1_prime ** 2 + b1 ** 2)
    C2_prime = math.sqrt(a2_prime ** 2 + b2 ** 2)
    C_bar_prime = (C1_prime + C2_prime) / 2

    # h' values
    def h_prime(a, b):
        if a == 0 and b == 0:
            return 0
        h = math.degrees(math.atan2(b, a))
        return h + 360 if h < 0 else h

    h1_prime = h_prime(a1_prime, b1)
    h2_prime = h_prime(a2_prime, b2)

    # Delta h'
    if C1_prime * C2_prime == 0:
        delta_h_prime = 0
    elif abs(h2_prime - h1_prime) <= 180:
        delta_h_prime = h2_prime - h1_prime
    elif h2_prime - h1_prime > 180:
        delta_h_prime = h2_prime - h1_prime - 360
    else:
        delta_h_prime = h2_prime - h1_prime + 360

    # Delta H'
    delta_H_prime = 2 * math.sqrt(C1_prime * C2_prime) * math.sin(math.radians(delta_h_prime / 2))

    # H_bar_prime
    if C1_prime * C2_prime == 0:
        H_bar_prime = h1_prime + h2_prime
    elif abs(h1_prime - h2_prime) <= 180:
        H_bar_prime = (h1_prime + h2_prime) / 2
    elif h1_prime + h2_prime < 360:
        H_bar_prime = (h1_prime + h2_prime + 360) / 2
    else:
        H_bar_prime = (h1_prime + h2_prime - 360) / 2

    # T factor
    T = (1 - 0.17 * math.cos(math.radians(H_bar_prime - 30))
         + 0.24 * math.cos(math.radians(2 * H_bar_prime))
         + 0.32 * math.cos(math.radians(3 * H_bar_prime + 6))
         - 0.20 * math.cos(math.radians(4 * H_bar_prime - 63)))

    # Delta L', C', H'
    delta_L_prime = L2 - L1
    delta_C_prime = C2_prime - C1_prime

    # S_L, S_C, S_H
    S_L = 1 + (0.015 * (L_bar - 50) ** 2) / math.sqrt(20 + (L_bar - 50) ** 2)
    S_C = 1 + 0.045 * C_bar_prime
    S_H = 1 + 0.015 * C_bar_prime * T

    # R_T
    delta_theta = 30 * math.exp(-((H_bar_prime - 275) / 25) ** 2)
    R_C = 2 * math.sqrt(C_bar_prime ** 7 / (C_bar_prime ** 7 + 25 ** 7))
    R_T = -R_C * math.sin(math.radians(2 * delta_theta))

    # Final calculation
    kL = kC = kH = 1  # Unity for default
    delta_E = math.sqrt(
        (delta_L_prime / (kL * S_L)) ** 2 +
        (delta_C_prime / (kC * S_C)) ** 2 +
        (delta_H_prime / (kH * S_H)) ** 2 +
        R_T * (delta_C_prime / (kC * S_C)) * (delta_H_prime / (kH * S_H))
    )

    return delta_E


# =============================================================================
# Color Swatch Widget
# =============================================================================

class ColorSwatch(QWidget):
    """Single color swatch display."""

    clicked = pyqtSignal()

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(80, 80)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

        self._color = QColor(128, 128, 128)
        self._label = ""
        self._show_border = True
        self._selected = False

    @property
    def color(self) -> QColor:
        return self._color

    @color.setter
    def color(self, value: QColor):
        self._color = value
        self.update()

    def set_rgb(self, r: int, g: int, b: int):
        """Set color from RGB values."""
        self._color = QColor(r, g, b)
        self.update()

    @property
    def label(self) -> str:
        return self._label

    @label.setter
    def label(self, value: str):
        self._label = value
        self.update()

    @property
    def selected(self) -> bool:
        return self._selected

    @selected.setter
    def selected(self, value: bool):
        self._selected = value
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Main color fill
        rect = self.rect().adjusted(2, 2, -2, -2)
        painter.fillRect(rect, self._color)

        # Border
        if self._show_border:
            if self._selected:
                painter.setPen(QPen(QColor("#4a9eff"), 3))
            else:
                painter.setPen(QPen(QColor("#404040"), 1))
            painter.drawRect(rect)

        # Label
        if self._label:
            # Determine text color based on luminance
            lum = 0.299 * self._color.red() + 0.587 * self._color.green() + 0.114 * self._color.blue()
            text_color = QColor("#000000") if lum > 128 else QColor("#ffffff")

            painter.setPen(text_color)
            painter.setFont(QFont("Segoe UI", 8))
            painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, self._label)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit()


# =============================================================================
# Comparison Swatch Widget
# =============================================================================

class ComparisonSwatch(QWidget):
    """Side-by-side target and measured color comparison."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(120, 100)

        self._target_color = QColor(128, 128, 128)
        self._measured_color = QColor(128, 128, 128)
        self._label = ""
        self._delta_e: Optional[float] = None

    def set_colors(self, target: Tuple[int, int, int], measured: Tuple[int, int, int]):
        """Set target and measured colors."""
        self._target_color = QColor(*target)
        self._measured_color = QColor(*measured)

        # Calculate Delta E
        lab1 = rgb_to_lab(*target)
        lab2 = rgb_to_lab(*measured)
        self._delta_e = delta_e_2000(lab1, lab2)

        self.update()

    @property
    def label(self) -> str:
        return self._label

    @label.setter
    def label(self, value: str):
        self._label = value
        self.update()

    @property
    def delta_e(self) -> Optional[float]:
        return self._delta_e

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        rect = self.rect()
        half_width = rect.width() // 2

        # Target side (left)
        target_rect = QRectF(0, 0, half_width, rect.height() - 25)
        painter.fillRect(target_rect, self._target_color)

        # Measured side (right)
        measured_rect = QRectF(half_width, 0, half_width, rect.height() - 25)
        painter.fillRect(measured_rect, self._measured_color)

        # Divider line
        painter.setPen(QPen(QColor("#ffffff"), 1))
        painter.drawLine(int(half_width), 0, int(half_width), int(rect.height() - 25))

        # Labels
        painter.setFont(QFont("Segoe UI", 8))
        painter.setPen(QColor("#808080"))
        painter.drawText(5, int(rect.height() - 12), "Target")
        painter.drawText(int(half_width + 5), int(rect.height() - 12), "Measured")

        # Delta E indicator
        if self._delta_e is not None:
            # Color code
            if self._delta_e < 1.0:
                de_color = QColor("#4caf50")
            elif self._delta_e < 2.0:
                de_color = QColor("#8bc34a")
            elif self._delta_e < 3.0:
                de_color = QColor("#ff9800")
            else:
                de_color = QColor("#f44336")

            # Draw Delta E badge
            badge_x = rect.width() - 50
            painter.setPen(de_color)
            painter.setFont(QFont("Segoe UI", 9, QFont.Weight.Bold))
            painter.drawText(int(badge_x), int(rect.height() - 10), f"ΔE {self._delta_e:.2f}")


# =============================================================================
# Color Info Panel
# =============================================================================

class ColorInfoPanel(QWidget):
    """Detailed color information display."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Color swatch
        self.swatch = ColorSwatch()
        self.swatch.setFixedSize(100, 100)
        layout.addWidget(self.swatch, alignment=Qt.AlignmentFlag.AlignCenter)

        # Color values grid
        values_layout = QGridLayout()
        values_layout.setSpacing(4)

        self.value_labels = {}

        rows = [
            ("RGB", "rgb"),
            ("Hex", "hex"),
            ("Lab", "lab"),
            ("XYZ", "xyz"),
        ]

        for i, (label, key) in enumerate(rows):
            label_widget = QLabel(f"{label}:")
            label_widget.setStyleSheet("color: #808080; font-size: 11px;")
            value_widget = QLabel("--")
            value_widget.setStyleSheet("font-family: monospace;")
            self.value_labels[key] = value_widget
            values_layout.addWidget(label_widget, i, 0)
            values_layout.addWidget(value_widget, i, 1)

        layout.addLayout(values_layout)

    def set_color(self, r: int, g: int, b: int, label: str = ""):
        """Set the displayed color."""
        self.swatch.set_rgb(r, g, b)
        self.swatch.label = label

        # Update values
        self.value_labels["rgb"].setText(f"({r}, {g}, {b})")
        self.value_labels["hex"].setText(f"#{r:02X}{g:02X}{b:02X}")

        L, a, b_val = rgb_to_lab(r, g, b)
        self.value_labels["lab"].setText(f"({L:.1f}, {a:.1f}, {b_val:.1f})")

        X, Y, Z = rgb_to_xyz(r, g, b)
        self.value_labels["xyz"].setText(f"({X:.2f}, {Y:.2f}, {Z:.2f})")


# =============================================================================
# Color Grid Widget
# =============================================================================

class ColorGrid(QWidget):
    """Grid of color swatches (e.g., for ColorChecker display)."""

    swatch_clicked = pyqtSignal(int)  # Index of clicked swatch

    def __init__(self, cols: int = 6, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.cols = cols
        self.swatches: list[ColorSwatch] = []
        self._selected_index = -1

        self._layout = QGridLayout(self)
        self._layout.setSpacing(4)
        self._layout.setContentsMargins(0, 0, 0, 0)

    def set_colors(self, colors: list[Tuple[int, int, int]], labels: list[str] = None):
        """Set all colors in the grid."""
        # Clear existing
        for swatch in self.swatches:
            swatch.deleteLater()
        self.swatches.clear()

        # Create new swatches
        for i, (r, g, b) in enumerate(colors):
            swatch = ColorSwatch()
            swatch.set_rgb(r, g, b)
            if labels and i < len(labels):
                swatch.label = labels[i]

            swatch.clicked.connect(lambda idx=i: self._on_swatch_clicked(idx))

            row = i // self.cols
            col = i % self.cols
            self._layout.addWidget(swatch, row, col)
            self.swatches.append(swatch)

    def _on_swatch_clicked(self, index: int):
        """Handle swatch click."""
        # Update selection
        if self._selected_index >= 0:
            self.swatches[self._selected_index].selected = False

        self._selected_index = index
        self.swatches[index].selected = True
        self.swatch_clicked.emit(index)

    def get_color(self, index: int) -> Optional[Tuple[int, int, int]]:
        """Get color at index."""
        if 0 <= index < len(self.swatches):
            c = self.swatches[index].color
            return (c.red(), c.green(), c.blue())
        return None
