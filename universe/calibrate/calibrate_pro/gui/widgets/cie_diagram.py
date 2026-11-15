"""
CIE Chromaticity Diagram Widget

Displays the CIE 1931 xy chromaticity diagram with:
- Spectral locus
- Gamut triangles (sRGB, P3, BT.2020, etc.)
- Measured points
- Blackbody locus (Planckian)
"""

from typing import Optional, List, Tuple, Dict
from dataclasses import dataclass
import math

from PyQt6.QtWidgets import QWidget, QVBoxLayout, QSizePolicy
from PyQt6.QtCore import Qt, QRectF, QPointF, pyqtSignal
from PyQt6.QtGui import (
    QPainter, QPen, QBrush, QColor, QPainterPath,
    QLinearGradient, QPolygonF, QFont, QImage
)


# =============================================================================
# Color Data
# =============================================================================

# CIE 1931 spectral locus (wavelength -> xy)
SPECTRAL_LOCUS = [
    (380, 0.1741, 0.0050), (385, 0.1740, 0.0050), (390, 0.1738, 0.0049),
    (395, 0.1736, 0.0049), (400, 0.1733, 0.0048), (405, 0.1730, 0.0048),
    (410, 0.1726, 0.0048), (415, 0.1721, 0.0048), (420, 0.1714, 0.0051),
    (425, 0.1703, 0.0058), (430, 0.1689, 0.0069), (435, 0.1669, 0.0086),
    (440, 0.1644, 0.0109), (445, 0.1611, 0.0138), (450, 0.1566, 0.0177),
    (455, 0.1510, 0.0227), (460, 0.1440, 0.0297), (465, 0.1355, 0.0399),
    (470, 0.1241, 0.0578), (475, 0.1096, 0.0868), (480, 0.0913, 0.1327),
    (485, 0.0687, 0.2007), (490, 0.0454, 0.2950), (495, 0.0235, 0.4127),
    (500, 0.0082, 0.5384), (505, 0.0039, 0.6548), (510, 0.0139, 0.7502),
    (515, 0.0389, 0.8120), (520, 0.0743, 0.8338), (525, 0.1142, 0.8262),
    (530, 0.1547, 0.8059), (535, 0.1929, 0.7816), (540, 0.2296, 0.7543),
    (545, 0.2658, 0.7243), (550, 0.3016, 0.6923), (555, 0.3373, 0.6589),
    (560, 0.3731, 0.6245), (565, 0.4087, 0.5896), (570, 0.4441, 0.5547),
    (575, 0.4788, 0.5202), (580, 0.5125, 0.4866), (585, 0.5448, 0.4544),
    (590, 0.5752, 0.4242), (595, 0.6029, 0.3965), (600, 0.6270, 0.3725),
    (605, 0.6482, 0.3514), (610, 0.6658, 0.3340), (615, 0.6801, 0.3197),
    (620, 0.6915, 0.3083), (625, 0.7006, 0.2993), (630, 0.7079, 0.2920),
    (635, 0.7140, 0.2859), (640, 0.7190, 0.2809), (645, 0.7230, 0.2770),
    (650, 0.7260, 0.2740), (655, 0.7283, 0.2717), (660, 0.7300, 0.2700),
    (665, 0.7311, 0.2689), (670, 0.7320, 0.2680), (675, 0.7327, 0.2673),
    (680, 0.7334, 0.2666), (685, 0.7340, 0.2660), (690, 0.7344, 0.2656),
    (695, 0.7346, 0.2654), (700, 0.7347, 0.2653), (705, 0.7347, 0.2653),
    (710, 0.7347, 0.2653), (715, 0.7347, 0.2653), (720, 0.7347, 0.2653),
    (725, 0.7347, 0.2653), (730, 0.7347, 0.2653), (735, 0.7347, 0.2653),
    (740, 0.7347, 0.2653), (745, 0.7347, 0.2653), (750, 0.7347, 0.2653),
    (755, 0.7347, 0.2653), (760, 0.7347, 0.2653), (765, 0.7347, 0.2653),
    (770, 0.7347, 0.2653), (775, 0.7347, 0.2653), (780, 0.7347, 0.2653),
]

# Standard illuminant white points
WHITE_POINTS = {
    "D50": (0.3457, 0.3585),
    "D55": (0.3324, 0.3474),
    "D65": (0.3127, 0.3290),
    "D75": (0.2990, 0.3149),
    "E": (0.3333, 0.3333),
    "A": (0.4476, 0.4074),
}

# Standard color gamuts (RGB primaries as xy coordinates)
GAMUTS = {
    "sRGB": {
        "R": (0.640, 0.330),
        "G": (0.300, 0.600),
        "B": (0.150, 0.060),
        "W": (0.3127, 0.3290),
        "color": "#4a9eff",
    },
    "DCI-P3": {
        "R": (0.680, 0.320),
        "G": (0.265, 0.690),
        "B": (0.150, 0.060),
        "W": (0.3140, 0.3510),
        "color": "#ff9800",
    },
    "Display P3": {
        "R": (0.680, 0.320),
        "G": (0.265, 0.690),
        "B": (0.150, 0.060),
        "W": (0.3127, 0.3290),
        "color": "#ff5722",
    },
    "BT.2020": {
        "R": (0.708, 0.292),
        "G": (0.170, 0.797),
        "B": (0.131, 0.046),
        "W": (0.3127, 0.3290),
        "color": "#4caf50",
    },
    "Adobe RGB": {
        "R": (0.640, 0.330),
        "G": (0.210, 0.710),
        "B": (0.150, 0.060),
        "W": (0.3127, 0.3290),
        "color": "#9c27b0",
    },
}


@dataclass
class MeasuredPoint:
    """A measured chromaticity point."""
    x: float
    y: float
    label: str = ""
    color: QColor = None
    is_target: bool = False


# =============================================================================
# CIE Diagram Widget
# =============================================================================

class CIEDiagramWidget(QWidget):
    """CIE 1931 xy Chromaticity Diagram."""

    point_clicked = pyqtSignal(float, float)  # xy coordinates

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(300, 300)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        # Display options
        self.show_spectral_locus = True
        self.show_planckian_locus = True
        self.show_grid = True
        self.show_labels = True
        self.background_mode = "gradient"  # "gradient", "solid", "image"

        # Gamuts to display
        self.visible_gamuts: List[str] = ["sRGB"]
        self.measured_gamut: Optional[Dict[str, Tuple[float, float]]] = None

        # Points to display
        self.target_points: List[MeasuredPoint] = []
        self.measured_points: List[MeasuredPoint] = []

        # View settings
        self.margin = 40
        self.x_range = (0.0, 0.8)
        self.y_range = (0.0, 0.9)

        # Pre-render background
        self._background_image: Optional[QImage] = None

    def set_gamuts(self, gamuts: List[str]):
        """Set which gamuts to display."""
        self.visible_gamuts = gamuts
        self.update()

    def set_measured_gamut(self, primaries: Optional[Dict[str, Tuple[float, float]]]):
        """Set measured display gamut."""
        self.measured_gamut = primaries
        self.update()

    def add_target_point(self, x: float, y: float, label: str = "", color: QColor = None):
        """Add a target point."""
        self.target_points.append(MeasuredPoint(x, y, label, color or QColor("#ff5722"), True))
        self.update()

    def add_measured_point(self, x: float, y: float, label: str = "", color: QColor = None):
        """Add a measured point."""
        self.measured_points.append(MeasuredPoint(x, y, label, color or QColor("#4a9eff"), False))
        self.update()

    def clear_points(self):
        """Clear all points."""
        self.target_points.clear()
        self.measured_points.clear()
        self.update()

    def _xy_to_pixel(self, x: float, y: float) -> QPointF:
        """Convert xy chromaticity to pixel coordinates."""
        plot_width = self.width() - 2 * self.margin
        plot_height = self.height() - 2 * self.margin

        px = self.margin + (x - self.x_range[0]) / (self.x_range[1] - self.x_range[0]) * plot_width
        py = self.height() - self.margin - (y - self.y_range[0]) / (self.y_range[1] - self.y_range[0]) * plot_height

        return QPointF(px, py)

    def _pixel_to_xy(self, px: float, py: float) -> Tuple[float, float]:
        """Convert pixel coordinates to xy chromaticity."""
        plot_width = self.width() - 2 * self.margin
        plot_height = self.height() - 2 * self.margin

        x = self.x_range[0] + (px - self.margin) / plot_width * (self.x_range[1] - self.x_range[0])
        y = self.y_range[0] + (self.height() - self.margin - py) / plot_height * (self.y_range[1] - self.y_range[0])

        return (x, y)

    def _render_background(self) -> QImage:
        """Render the CIE background with spectral colors."""
        size = min(self.width(), self.height())
        image = QImage(self.width(), self.height(), QImage.Format.Format_RGB32)
        image.fill(QColor("#1a1a1a"))

        # For a proper CIE background, we'd need to convert xy to RGB
        # This is a simplified gradient version
        painter = QPainter(image)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Create the spectral locus path
        path = QPainterPath()
        first = True
        for wl, x, y in SPECTRAL_LOCUS:
            pt = self._xy_to_pixel(x, y)
            if first:
                path.moveTo(pt)
                first = False
            else:
                path.lineTo(pt)

        # Close the path along the purple line
        path.lineTo(self._xy_to_pixel(SPECTRAL_LOCUS[0][1], SPECTRAL_LOCUS[0][2]))

        # Fill with a gradient approximation
        gradient = QLinearGradient(
            self._xy_to_pixel(0.15, 0.06),
            self._xy_to_pixel(0.35, 0.35)
        )
        gradient.setColorAt(0.0, QColor(80, 0, 120, 100))
        gradient.setColorAt(0.2, QColor(0, 0, 180, 100))
        gradient.setColorAt(0.4, QColor(0, 150, 100, 100))
        gradient.setColorAt(0.6, QColor(100, 180, 0, 100))
        gradient.setColorAt(0.8, QColor(200, 150, 0, 100))
        gradient.setColorAt(1.0, QColor(200, 50, 0, 100))

        painter.fillPath(path, QBrush(gradient))
        painter.end()

        return image

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Background
        painter.fillRect(self.rect(), QColor("#1a1a1a"))

        # Draw grid
        if self.show_grid:
            self._draw_grid(painter)

        # Draw spectral locus
        if self.show_spectral_locus:
            self._draw_spectral_locus(painter)

        # Draw Planckian locus
        if self.show_planckian_locus:
            self._draw_planckian_locus(painter)

        # Draw gamuts
        for gamut_name in self.visible_gamuts:
            if gamut_name in GAMUTS:
                self._draw_gamut(painter, gamut_name, GAMUTS[gamut_name])

        # Draw measured gamut
        if self.measured_gamut:
            self._draw_measured_gamut(painter)

        # Draw white points
        self._draw_white_points(painter)

        # Draw points
        for point in self.target_points:
            self._draw_point(painter, point)
        for point in self.measured_points:
            self._draw_point(painter, point)

        # Draw axes labels
        if self.show_labels:
            self._draw_labels(painter)

    def _draw_grid(self, painter: QPainter):
        """Draw coordinate grid."""
        painter.setPen(QPen(QColor("#303030"), 1, Qt.PenStyle.DotLine))

        # Vertical lines
        for x in [i * 0.1 for i in range(9)]:
            p1 = self._xy_to_pixel(x, self.y_range[0])
            p2 = self._xy_to_pixel(x, self.y_range[1])
            painter.drawLine(p1, p2)

        # Horizontal lines
        for y in [i * 0.1 for i in range(10)]:
            p1 = self._xy_to_pixel(self.x_range[0], y)
            p2 = self._xy_to_pixel(self.x_range[1], y)
            painter.drawLine(p1, p2)

    def _draw_spectral_locus(self, painter: QPainter):
        """Draw the spectral locus curve."""
        path = QPainterPath()
        first = True

        for wl, x, y in SPECTRAL_LOCUS:
            pt = self._xy_to_pixel(x, y)
            if first:
                path.moveTo(pt)
                first = False
            else:
                path.lineTo(pt)

        # Close with purple line
        path.lineTo(self._xy_to_pixel(SPECTRAL_LOCUS[0][1], SPECTRAL_LOCUS[0][2]))

        painter.setPen(QPen(QColor("#808080"), 2))
        painter.drawPath(path)

        # Wavelength labels
        if self.show_labels:
            painter.setPen(QColor("#606060"))
            painter.setFont(QFont("Segoe UI", 7))
            for wl, x, y in SPECTRAL_LOCUS[::10]:  # Every 50nm
                pt = self._xy_to_pixel(x, y)
                painter.drawText(int(pt.x()) + 3, int(pt.y()) - 3, f"{wl}")

    def _draw_planckian_locus(self, painter: QPainter):
        """Draw the Planckian (blackbody) locus."""
        path = QPainterPath()
        first = True

        # Calculate Planckian locus points
        for T in range(1000, 25001, 100):
            # CIE Daylight approximation for xy coordinates
            if T < 4000:
                x = -0.2661239e9 / (T ** 3) - 0.2343589e6 / (T ** 2) + 0.8776956e3 / T + 0.179910
            elif T < 7000:
                x = -4.6070e9 / (T ** 3) + 2.9678e6 / (T ** 2) + 0.09911e3 / T + 0.244063
            else:
                x = -2.0064e9 / (T ** 3) + 1.9018e6 / (T ** 2) - 0.24748e3 / T + 0.237040

            y = -3.0 * x ** 2 + 2.87 * x - 0.275

            if 0 < x < 0.8 and 0 < y < 0.9:
                pt = self._xy_to_pixel(x, y)
                if first:
                    path.moveTo(pt)
                    first = False
                else:
                    path.lineTo(pt)

        painter.setPen(QPen(QColor("#804000"), 2, Qt.PenStyle.DashLine))
        painter.drawPath(path)

    def _draw_gamut(self, painter: QPainter, name: str, gamut: dict):
        """Draw a color gamut triangle."""
        r = self._xy_to_pixel(*gamut["R"])
        g = self._xy_to_pixel(*gamut["G"])
        b = self._xy_to_pixel(*gamut["B"])

        triangle = QPolygonF([r, g, b])

        # Fill with transparent color
        color = QColor(gamut.get("color", "#4a9eff"))
        fill_color = QColor(color)
        fill_color.setAlpha(30)
        painter.setBrush(QBrush(fill_color))

        # Outline
        painter.setPen(QPen(color, 2))
        painter.drawPolygon(triangle)

        # Labels at primaries
        if self.show_labels:
            painter.setPen(color)
            painter.setFont(QFont("Segoe UI", 8, QFont.Weight.Bold))
            painter.drawText(int(r.x()) + 5, int(r.y()), "R")
            painter.drawText(int(g.x()) - 10, int(g.y()) - 5, "G")
            painter.drawText(int(b.x()) - 5, int(b.y()) + 15, "B")

    def _draw_measured_gamut(self, painter: QPainter):
        """Draw the measured display gamut."""
        if not self.measured_gamut:
            return

        r = self._xy_to_pixel(*self.measured_gamut.get("R", (0.64, 0.33)))
        g = self._xy_to_pixel(*self.measured_gamut.get("G", (0.30, 0.60)))
        b = self._xy_to_pixel(*self.measured_gamut.get("B", (0.15, 0.06)))

        triangle = QPolygonF([r, g, b])

        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.setPen(QPen(QColor("#ffffff"), 3))
        painter.drawPolygon(triangle)

        # Draw with dashed line for visibility
        painter.setPen(QPen(QColor("#00ffff"), 2, Qt.PenStyle.DashLine))
        painter.drawPolygon(triangle)

    def _draw_white_points(self, painter: QPainter):
        """Draw standard illuminant white points."""
        painter.setFont(QFont("Segoe UI", 8))

        for name, (x, y) in WHITE_POINTS.items():
            if name not in ["D50", "D65", "E"]:
                continue

            pt = self._xy_to_pixel(x, y)

            # Small cross
            painter.setPen(QPen(QColor("#ffff00"), 1))
            size = 4
            painter.drawLine(int(pt.x()) - size, int(pt.y()), int(pt.x()) + size, int(pt.y()))
            painter.drawLine(int(pt.x()), int(pt.y()) - size, int(pt.x()), int(pt.y()) + size)

            if self.show_labels:
                painter.drawText(int(pt.x()) + 6, int(pt.y()) + 4, name)

    def _draw_point(self, painter: QPainter, point: MeasuredPoint):
        """Draw a measured point."""
        pt = self._xy_to_pixel(point.x, point.y)
        color = point.color or QColor("#4a9eff")

        if point.is_target:
            # Target: hollow circle
            painter.setPen(QPen(color, 2))
            painter.setBrush(Qt.BrushStyle.NoBrush)
            painter.drawEllipse(pt, 6, 6)
        else:
            # Measured: filled circle
            painter.setPen(QPen(color.darker(120), 1))
            painter.setBrush(QBrush(color))
            painter.drawEllipse(pt, 5, 5)

        # Label
        if point.label and self.show_labels:
            painter.setPen(color)
            painter.setFont(QFont("Segoe UI", 8))
            painter.drawText(int(pt.x()) + 8, int(pt.y()) + 4, point.label)

    def _draw_labels(self, painter: QPainter):
        """Draw axis labels."""
        painter.setPen(QColor("#808080"))
        painter.setFont(QFont("Segoe UI", 9))

        # X axis
        for x in [0.0, 0.2, 0.4, 0.6, 0.8]:
            pt = self._xy_to_pixel(x, 0)
            painter.drawText(int(pt.x()) - 10, int(pt.y()) + 15, f"{x:.1f}")

        # Y axis
        for y in [0.0, 0.2, 0.4, 0.6, 0.8]:
            pt = self._xy_to_pixel(0, y)
            painter.drawText(int(pt.x()) - 30, int(pt.y()) + 5, f"{y:.1f}")

        # Axis names
        painter.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        painter.drawText(self.width() // 2 - 5, self.height() - 5, "x")
        painter.save()
        painter.translate(15, self.height() // 2)
        painter.rotate(-90)
        painter.drawText(0, 0, "y")
        painter.restore()

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            x, y = self._pixel_to_xy(event.pos().x(), event.pos().y())
            if self.x_range[0] <= x <= self.x_range[1] and self.y_range[0] <= y <= self.y_range[1]:
                self.point_clicked.emit(x, y)

    def resizeEvent(self, event):
        # Invalidate cached background
        self._background_image = None
        super().resizeEvent(event)
