"""
Gamma Curve Widget

Displays gamma/EOTF curves with:
- Target curve (sRGB, BT.1886, power law)
- Measured response
- Per-channel curves (R, G, B)
- Deviation indicators
"""

from typing import Optional, List, Tuple, Dict
from dataclasses import dataclass
import math

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QLabel, QCheckBox, QSizePolicy
from PyQt6.QtCore import Qt, QRectF, QPointF, pyqtSignal
from PyQt6.QtGui import (
    QPainter, QPen, QBrush, QColor, QPainterPath,
    QLinearGradient, QFont
)


# =============================================================================
# Gamma Functions
# =============================================================================

def srgb_eotf(x: float) -> float:
    """sRGB EOTF (display encoding to linear light)."""
    if x <= 0.04045:
        return x / 12.92
    return ((x + 0.055) / 1.055) ** 2.4


def srgb_oetf(y: float) -> float:
    """sRGB OETF (linear light to display encoding)."""
    if y <= 0.0031308:
        return 12.92 * y
    return 1.055 * (y ** (1 / 2.4)) - 0.055


def bt1886_eotf(x: float, gamma: float = 2.4, black: float = 0.0, white: float = 1.0) -> float:
    """BT.1886 EOTF."""
    a = (white ** (1 / gamma) - black ** (1 / gamma)) ** gamma
    b = black ** (1 / gamma) / (white ** (1 / gamma) - black ** (1 / gamma))
    return a * max(x + b, 0) ** gamma


def power_law_eotf(x: float, gamma: float = 2.2) -> float:
    """Simple power law gamma."""
    return x ** gamma


def l_star_eotf(x: float) -> float:
    """L* (CIELAB lightness) based EOTF."""
    if x <= 0.08:
        return x * 100 / 903.3
    return ((x + 0.16) / 1.16) ** 3


@dataclass
class CurveData:
    """Data for a gamma curve."""
    name: str
    color: QColor
    points: List[Tuple[float, float]]  # (input, output) normalized 0-1
    visible: bool = True


# =============================================================================
# Gamma Curve Widget
# =============================================================================

class GammaCurveWidget(QWidget):
    """Gamma/EOTF curve display widget."""

    point_hovered = pyqtSignal(float, float, float)  # input, target, measured

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(300, 250)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setMouseTracking(True)

        # Curve data
        self.target_curve: Optional[CurveData] = None
        self.measured_curves: Dict[str, CurveData] = {}

        # Display options
        self.show_grid = True
        self.show_target = True
        self.show_channels = True  # R, G, B individual
        self.show_grayscale = True  # Combined grayscale
        self.show_deviation = True
        self.log_scale = False

        # View settings
        self.margin = 50
        self.x_range = (0.0, 1.0)
        self.y_range = (0.0, 1.0)

        # Colors
        self.colors = {
            "target": QColor("#ff5722"),
            "grayscale": QColor("#ffffff"),
            "red": QColor("#ff4444"),
            "green": QColor("#44ff44"),
            "blue": QColor("#4444ff"),
            "grid": QColor("#303030"),
            "axis": QColor("#505050"),
        }

        # Hover state
        self._hover_x: Optional[float] = None

    def set_target_gamma(self, gamma_type: str, gamma_value: float = 2.2):
        """Set the target gamma curve."""
        points = []

        for i in range(101):
            x = i / 100.0
            if gamma_type == "sRGB":
                y = srgb_eotf(x)
            elif gamma_type == "BT.1886":
                y = bt1886_eotf(x, gamma_value)
            elif gamma_type == "power":
                y = power_law_eotf(x, gamma_value)
            elif gamma_type == "L*":
                y = l_star_eotf(x)
            else:
                y = power_law_eotf(x, gamma_value)

            points.append((x, y))

        self.target_curve = CurveData(
            name=f"{gamma_type} (γ={gamma_value:.1f})" if gamma_type == "power" else gamma_type,
            color=self.colors["target"],
            points=points
        )
        self.update()

    def set_measured_grayscale(self, points: List[Tuple[float, float]]):
        """Set measured grayscale response."""
        self.measured_curves["grayscale"] = CurveData(
            name="Measured",
            color=self.colors["grayscale"],
            points=points
        )
        self.update()

    def set_measured_channel(self, channel: str, points: List[Tuple[float, float]]):
        """Set measured response for a specific channel (R, G, B)."""
        color_map = {"R": "red", "G": "green", "B": "blue"}
        self.measured_curves[channel] = CurveData(
            name=channel,
            color=self.colors.get(color_map.get(channel, "grayscale")),
            points=points
        )
        self.update()

    def clear_measurements(self):
        """Clear all measured curves."""
        self.measured_curves.clear()
        self.update()

    def _value_to_pixel(self, x: float, y: float) -> QPointF:
        """Convert normalized values to pixel coordinates."""
        plot_width = self.width() - 2 * self.margin
        plot_height = self.height() - 2 * self.margin

        if self.log_scale and y > 0:
            y = math.log10(y * 100 + 1) / math.log10(101)

        px = self.margin + x * plot_width
        py = self.height() - self.margin - y * plot_height

        return QPointF(px, py)

    def _pixel_to_value(self, px: float, py: float) -> Tuple[float, float]:
        """Convert pixel coordinates to normalized values."""
        plot_width = self.width() - 2 * self.margin
        plot_height = self.height() - 2 * self.margin

        x = (px - self.margin) / plot_width
        y = (self.height() - self.margin - py) / plot_height

        return (max(0, min(1, x)), max(0, min(1, y)))

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Background
        painter.fillRect(self.rect(), QColor("#1a1a1a"))

        # Draw grid
        if self.show_grid:
            self._draw_grid(painter)

        # Draw target curve
        if self.show_target and self.target_curve:
            self._draw_curve(painter, self.target_curve, 3, Qt.PenStyle.DashLine)

        # Draw measured curves
        if self.show_grayscale and "grayscale" in self.measured_curves:
            self._draw_curve(painter, self.measured_curves["grayscale"], 2)

        if self.show_channels:
            for channel in ["R", "G", "B"]:
                if channel in self.measured_curves:
                    self._draw_curve(painter, self.measured_curves[channel], 1.5)

        # Draw deviation area
        if self.show_deviation and self.target_curve and "grayscale" in self.measured_curves:
            self._draw_deviation(painter)

        # Draw hover line
        if self._hover_x is not None:
            self._draw_hover(painter)

        # Draw axes and labels
        self._draw_axes(painter)

    def _draw_grid(self, painter: QPainter):
        """Draw coordinate grid."""
        painter.setPen(QPen(self.colors["grid"], 1, Qt.PenStyle.DotLine))

        # Grid lines at 10% intervals
        for i in range(1, 10):
            val = i / 10.0

            # Vertical
            p1 = self._value_to_pixel(val, 0)
            p2 = self._value_to_pixel(val, 1)
            painter.drawLine(p1, p2)

            # Horizontal
            p1 = self._value_to_pixel(0, val)
            p2 = self._value_to_pixel(1, val)
            painter.drawLine(p1, p2)

        # Reference line (y = x for gamma 1.0)
        painter.setPen(QPen(self.colors["axis"], 1, Qt.PenStyle.DashDotLine))
        p1 = self._value_to_pixel(0, 0)
        p2 = self._value_to_pixel(1, 1)
        painter.drawLine(p1, p2)

    def _draw_curve(self, painter: QPainter, curve: CurveData,
                    width: float = 2, style: Qt.PenStyle = Qt.PenStyle.SolidLine):
        """Draw a single curve."""
        if not curve.points or not curve.visible:
            return

        path = QPainterPath()
        first = True

        for x, y in curve.points:
            pt = self._value_to_pixel(x, y)
            if first:
                path.moveTo(pt)
                first = False
            else:
                path.lineTo(pt)

        painter.setPen(QPen(curve.color, width, style))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawPath(path)

    def _draw_deviation(self, painter: QPainter):
        """Draw deviation area between target and measured."""
        if not self.target_curve or "grayscale" not in self.measured_curves:
            return

        target_points = dict(self.target_curve.points)
        measured = self.measured_curves["grayscale"]

        # Create fill path
        path = QPainterPath()

        # Forward along measured curve
        first = True
        for x, y in measured.points:
            pt = self._value_to_pixel(x, y)
            if first:
                path.moveTo(pt)
                first = False
            else:
                path.lineTo(pt)

        # Backward along target curve
        for x, y in reversed(self.target_curve.points):
            pt = self._value_to_pixel(x, y)
            path.lineTo(pt)

        path.closeSubpath()

        # Fill with semi-transparent color based on deviation
        fill_color = QColor("#ff5722")
        fill_color.setAlpha(40)
        painter.fillPath(path, fill_color)

    def _draw_hover(self, painter: QPainter):
        """Draw hover indicator line."""
        if self._hover_x is None:
            return

        # Vertical line
        p1 = self._value_to_pixel(self._hover_x, 0)
        p2 = self._value_to_pixel(self._hover_x, 1)
        painter.setPen(QPen(QColor("#ffffff"), 1, Qt.PenStyle.DashLine))
        painter.drawLine(p1, p2)

        # Draw intersection points
        if self.target_curve:
            for x, y in self.target_curve.points:
                if abs(x - self._hover_x) < 0.01:
                    pt = self._value_to_pixel(x, y)
                    painter.setPen(QPen(self.colors["target"], 2))
                    painter.setBrush(self.colors["target"])
                    painter.drawEllipse(pt, 4, 4)
                    break

        if "grayscale" in self.measured_curves:
            for x, y in self.measured_curves["grayscale"].points:
                if abs(x - self._hover_x) < 0.01:
                    pt = self._value_to_pixel(x, y)
                    painter.setPen(QPen(self.colors["grayscale"], 2))
                    painter.setBrush(self.colors["grayscale"])
                    painter.drawEllipse(pt, 4, 4)
                    break

    def _draw_axes(self, painter: QPainter):
        """Draw axis lines and labels."""
        # Axis lines
        painter.setPen(QPen(self.colors["axis"], 2))

        # X axis
        p1 = self._value_to_pixel(0, 0)
        p2 = self._value_to_pixel(1, 0)
        painter.drawLine(p1, p2)

        # Y axis
        p1 = self._value_to_pixel(0, 0)
        p2 = self._value_to_pixel(0, 1)
        painter.drawLine(p1, p2)

        # Labels
        painter.setPen(QColor("#808080"))
        painter.setFont(QFont("Segoe UI", 9))

        # X axis labels
        for i in range(0, 11, 2):
            val = i / 10.0
            pt = self._value_to_pixel(val, 0)
            label = f"{int(val * 100)}%"
            painter.drawText(int(pt.x()) - 12, int(pt.y()) + 18, label)

        # Y axis labels
        for i in range(0, 11, 2):
            val = i / 10.0
            pt = self._value_to_pixel(0, val)
            label = f"{int(val * 100)}%"
            painter.drawText(int(pt.x()) - 35, int(pt.y()) + 5, label)

        # Axis titles
        painter.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        painter.drawText(self.width() // 2 - 40, self.height() - 8, "Input (Signal)")

        painter.save()
        painter.translate(12, self.height() // 2 + 40)
        painter.rotate(-90)
        painter.drawText(0, 0, "Output (Luminance)")
        painter.restore()

        # Legend
        if self.target_curve or self.measured_curves:
            self._draw_legend(painter)

    def _draw_legend(self, painter: QPainter):
        """Draw curve legend."""
        painter.setFont(QFont("Segoe UI", 9))
        x = self.width() - 120
        y = self.margin + 10

        if self.target_curve:
            painter.setPen(QPen(self.colors["target"], 2, Qt.PenStyle.DashLine))
            painter.drawLine(x, y, x + 20, y)
            painter.setPen(self.colors["target"])
            painter.drawText(x + 25, y + 4, self.target_curve.name)
            y += 18

        if "grayscale" in self.measured_curves and self.show_grayscale:
            painter.setPen(QPen(self.colors["grayscale"], 2))
            painter.drawLine(x, y, x + 20, y)
            painter.setPen(self.colors["grayscale"])
            painter.drawText(x + 25, y + 4, "Measured")
            y += 18

    def mouseMoveEvent(self, event):
        x, _ = self._pixel_to_value(event.pos().x(), event.pos().y())
        if 0 <= x <= 1:
            self._hover_x = x
            self.update()

            # Find values at this x position
            target_y = None
            measured_y = None

            if self.target_curve:
                for px, py in self.target_curve.points:
                    if abs(px - x) < 0.015:
                        target_y = py
                        break

            if "grayscale" in self.measured_curves:
                for px, py in self.measured_curves["grayscale"].points:
                    if abs(px - x) < 0.015:
                        measured_y = py
                        break

            if target_y is not None or measured_y is not None:
                self.point_hovered.emit(x, target_y or 0, measured_y or 0)

    def leaveEvent(self, event):
        self._hover_x = None
        self.update()


# =============================================================================
# Gamma Info Panel
# =============================================================================

class GammaInfoPanel(QWidget):
    """Information panel showing gamma statistics."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)

        # Target info
        self.target_label = QLabel("Target: sRGB")
        self.target_label.setStyleSheet("font-weight: 600;")
        layout.addWidget(self.target_label)

        # Measured gamma
        self.measured_label = QLabel("Measured γ: --")
        layout.addWidget(self.measured_label)

        # Average deviation
        self.deviation_label = QLabel("Avg. Deviation: --")
        layout.addWidget(self.deviation_label)

        # Max deviation
        self.max_dev_label = QLabel("Max Deviation: --")
        layout.addWidget(self.max_dev_label)

        # RGB balance
        self.balance_label = QLabel("RGB Balance: --")
        layout.addWidget(self.balance_label)

        layout.addStretch()

    def update_stats(self, target_gamma: float, measured_gamma: float,
                    avg_deviation: float, max_deviation: float,
                    rgb_balance: Tuple[float, float, float]):
        """Update the statistics display."""
        self.target_label.setText(f"Target: γ {target_gamma:.2f}")
        self.measured_label.setText(f"Measured γ: {measured_gamma:.2f}")

        # Color code deviation
        dev_color = "#4caf50" if avg_deviation < 0.02 else "#ff9800" if avg_deviation < 0.05 else "#f44336"
        self.deviation_label.setText(f"Avg. Deviation: {avg_deviation * 100:.1f}%")
        self.deviation_label.setStyleSheet(f"color: {dev_color};")

        max_color = "#4caf50" if max_deviation < 0.03 else "#ff9800" if max_deviation < 0.08 else "#f44336"
        self.max_dev_label.setText(f"Max Deviation: {max_deviation * 100:.1f}%")
        self.max_dev_label.setStyleSheet(f"color: {max_color};")

        r, g, b = rgb_balance
        self.balance_label.setText(f"RGB Balance: R{r:+.1f}% G{g:+.1f}% B{b:+.1f}%")
