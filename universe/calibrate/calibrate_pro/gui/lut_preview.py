"""
LUT Preview Widget - 3D LUT Visualization

Provides interactive 3D visualization of color lookup tables:
- Rotatable 3D cube view
- Slice views (R, G, B planes)
- Before/After comparison
- LUT statistics
"""

from typing import Optional, List, Tuple
from dataclasses import dataclass
import math
import numpy as np

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QFrame, QSlider, QComboBox, QGroupBox, QGridLayout,
    QSizePolicy, QStackedWidget, QTabWidget
)
from PyQt6.QtCore import Qt, QPointF, QTimer, pyqtSignal
from PyQt6.QtGui import (
    QPainter, QPen, QBrush, QColor, QPainterPath,
    QImage, QPixmap, QLinearGradient, QTransform, QFont
)


# =============================================================================
# Theme Colors
# =============================================================================

COLORS = {
    "background": "#1a1a1a",
    "surface": "#2d2d2d",
    "border": "#404040",
    "text_primary": "#e0e0e0",
    "text_secondary": "#808080",
    "accent": "#4a9eff",
    "red": "#ff4444",
    "green": "#44ff44",
    "blue": "#4444ff",
}


# =============================================================================
# 3D LUT Data Structure
# =============================================================================

@dataclass
class LUT3D:
    """3D Color Lookup Table."""
    size: int  # Cube size (e.g., 17, 33, 65)
    data: np.ndarray  # Shape: (size, size, size, 3) for RGB

    @classmethod
    def identity(cls, size: int = 33) -> 'LUT3D':
        """Create an identity LUT (no color change)."""
        data = np.zeros((size, size, size, 3), dtype=np.float32)
        for r in range(size):
            for g in range(size):
                for b in range(size):
                    data[r, g, b] = [r / (size - 1), g / (size - 1), b / (size - 1)]
        return cls(size=size, data=data)

    @classmethod
    def from_cube_file(cls, path: str) -> 'LUT3D':
        """Load from .cube file."""
        with open(path, 'r') as f:
            lines = f.readlines()

        size = 0
        data_lines = []

        for line in lines:
            line = line.strip()
            if line.startswith('#') or not line:
                continue
            if line.startswith('LUT_3D_SIZE'):
                size = int(line.split()[1])
            elif line.startswith('TITLE') or line.startswith('DOMAIN'):
                continue
            else:
                # Data line
                parts = line.split()
                if len(parts) >= 3:
                    data_lines.append([float(x) for x in parts[:3]])

        if size == 0 or not data_lines:
            raise ValueError("Invalid .cube file")

        data = np.array(data_lines, dtype=np.float32).reshape((size, size, size, 3))
        return cls(size=size, data=data)

    def lookup(self, r: float, g: float, b: float) -> Tuple[float, float, float]:
        """Look up color with trilinear interpolation."""
        # Scale to LUT indices
        r_idx = r * (self.size - 1)
        g_idx = g * (self.size - 1)
        b_idx = b * (self.size - 1)

        # Get integer indices
        r0, r1 = int(r_idx), min(int(r_idx) + 1, self.size - 1)
        g0, g1 = int(g_idx), min(int(g_idx) + 1, self.size - 1)
        b0, b1 = int(b_idx), min(int(b_idx) + 1, self.size - 1)

        # Get fractional parts
        rf = r_idx - r0
        gf = g_idx - g0
        bf = b_idx - b0

        # Trilinear interpolation
        c000 = self.data[r0, g0, b0]
        c001 = self.data[r0, g0, b1]
        c010 = self.data[r0, g1, b0]
        c011 = self.data[r0, g1, b1]
        c100 = self.data[r1, g0, b0]
        c101 = self.data[r1, g0, b1]
        c110 = self.data[r1, g1, b0]
        c111 = self.data[r1, g1, b1]

        c00 = c000 * (1 - rf) + c100 * rf
        c01 = c001 * (1 - rf) + c101 * rf
        c10 = c010 * (1 - rf) + c110 * rf
        c11 = c011 * (1 - rf) + c111 * rf

        c0 = c00 * (1 - gf) + c10 * gf
        c1 = c01 * (1 - gf) + c11 * gf

        c = c0 * (1 - bf) + c1 * bf

        return tuple(np.clip(c, 0, 1))

    def get_deviation_stats(self) -> dict:
        """Calculate deviation from identity LUT."""
        identity = LUT3D.identity(self.size)

        diffs = np.abs(self.data - identity.data)
        max_diff = np.max(diffs)
        avg_diff = np.mean(diffs)

        # Per-channel stats
        r_diff = np.mean(np.abs(self.data[:, :, :, 0] - identity.data[:, :, :, 0]))
        g_diff = np.mean(np.abs(self.data[:, :, :, 1] - identity.data[:, :, :, 1]))
        b_diff = np.mean(np.abs(self.data[:, :, :, 2] - identity.data[:, :, :, 2]))

        return {
            'max_deviation': max_diff,
            'avg_deviation': avg_diff,
            'r_deviation': r_diff,
            'g_deviation': g_diff,
            'b_deviation': b_diff,
        }


# =============================================================================
# 3D Cube Preview Widget
# =============================================================================

class LUTCubeView(QWidget):
    """3D rotating cube view of LUT."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(300, 300)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setMouseTracking(True)

        # LUT data
        self.lut: Optional[LUT3D] = None

        # View parameters
        self.rotation_x = 30  # degrees
        self.rotation_y = -30
        self.zoom = 1.0
        self.show_wireframe = True
        self.show_points = True
        self.point_density = 5  # Show every Nth point

        # Drag state
        self._dragging = False
        self._last_pos = None

        # Animation
        self.auto_rotate = False
        self._timer = QTimer(self)
        self._timer.timeout.connect(self._animate)

    def set_lut(self, lut: LUT3D):
        """Set the LUT to display."""
        self.lut = lut
        self.update()

    def start_rotation(self):
        """Start auto-rotation."""
        self.auto_rotate = True
        self._timer.start(50)

    def stop_rotation(self):
        """Stop auto-rotation."""
        self.auto_rotate = False
        self._timer.stop()

    def _animate(self):
        """Animation tick."""
        self.rotation_y = (self.rotation_y + 1) % 360
        self.update()

    def _project_point(self, x: float, y: float, z: float) -> QPointF:
        """Project 3D point to 2D using perspective projection."""
        # Center and scale
        cx = self.width() / 2
        cy = self.height() / 2
        scale = min(self.width(), self.height()) * 0.35 * self.zoom

        # Rotate around Y axis
        angle_y = math.radians(self.rotation_y)
        x2 = x * math.cos(angle_y) - z * math.sin(angle_y)
        z2 = x * math.sin(angle_y) + z * math.cos(angle_y)

        # Rotate around X axis
        angle_x = math.radians(self.rotation_x)
        y2 = y * math.cos(angle_x) - z2 * math.sin(angle_x)
        z3 = y * math.sin(angle_x) + z2 * math.cos(angle_x)

        # Simple perspective
        perspective = 4
        factor = perspective / (perspective + z3 + 1)

        px = cx + x2 * scale * factor
        py = cy - y2 * scale * factor

        return QPointF(px, py)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Background
        painter.fillRect(self.rect(), QColor(COLORS['background']))

        if self.lut is None:
            painter.setPen(QColor(COLORS['text_secondary']))
            painter.setFont(QFont("Segoe UI", 12))
            painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No LUT loaded")
            return

        # Draw wireframe cube
        if self.show_wireframe:
            self._draw_wireframe(painter)

        # Draw LUT points
        if self.show_points:
            self._draw_lut_points(painter)

        # Draw axes labels
        self._draw_axes_labels(painter)

    def _draw_wireframe(self, painter: QPainter):
        """Draw cube wireframe."""
        painter.setPen(QPen(QColor(COLORS['border']), 1))

        # Cube vertices (-1 to 1 normalized)
        vertices = [
            (-1, -1, -1), (1, -1, -1), (1, 1, -1), (-1, 1, -1),
            (-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1),
        ]

        # Project vertices
        projected = [self._project_point(*v) for v in vertices]

        # Draw edges
        edges = [
            (0, 1), (1, 2), (2, 3), (3, 0),  # Front
            (4, 5), (5, 6), (6, 7), (7, 4),  # Back
            (0, 4), (1, 5), (2, 6), (3, 7),  # Connecting
        ]

        for i, j in edges:
            painter.drawLine(projected[i], projected[j])

    def _draw_lut_points(self, painter: QPainter):
        """Draw LUT color points."""
        if not self.lut:
            return

        size = self.lut.size
        step = max(1, size // self.point_density)

        # Collect points with depth for sorting
        points = []

        for r in range(0, size, step):
            for g in range(0, size, step):
                for b in range(0, size, step):
                    # Original position (normalized -1 to 1)
                    x = (r / (size - 1)) * 2 - 1
                    y = (g / (size - 1)) * 2 - 1
                    z = (b / (size - 1)) * 2 - 1

                    # Get LUT output color
                    out_r, out_g, out_b = self.lut.data[r, g, b]

                    # Calculate depth for sorting
                    angle_y = math.radians(self.rotation_y)
                    angle_x = math.radians(self.rotation_x)
                    z2 = x * math.sin(angle_y) + z * math.cos(angle_y)
                    z3 = y * math.sin(angle_x) + z2 * math.cos(angle_x)

                    points.append((z3, x, y, z, out_r, out_g, out_b))

        # Sort by depth (back to front)
        points.sort(key=lambda p: p[0])

        # Draw points
        for depth, x, y, z, out_r, out_g, out_b in points:
            pos = self._project_point(x, y, z)

            # Color from LUT output
            color = QColor(
                int(out_r * 255),
                int(out_g * 255),
                int(out_b * 255)
            )

            # Size based on depth
            size = max(2, int(4 * (1 + depth) / 2))

            painter.setPen(Qt.PenStyle.NoPen)
            painter.setBrush(QBrush(color))
            painter.drawEllipse(pos, size, size)

    def _draw_axes_labels(self, painter: QPainter):
        """Draw RGB axis labels."""
        painter.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))

        # R axis (pointing right)
        r_pos = self._project_point(1.2, 0, 0)
        painter.setPen(QColor(COLORS['red']))
        painter.drawText(r_pos, "R")

        # G axis (pointing up)
        g_pos = self._project_point(0, 1.2, 0)
        painter.setPen(QColor(COLORS['green']))
        painter.drawText(g_pos, "G")

        # B axis (pointing forward)
        b_pos = self._project_point(0, 0, 1.2)
        painter.setPen(QColor(COLORS['blue']))
        painter.drawText(b_pos, "B")

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._dragging = True
            self._last_pos = event.pos()
            self.stop_rotation()

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._dragging = False

    def mouseMoveEvent(self, event):
        if self._dragging and self._last_pos:
            delta = event.pos() - self._last_pos
            self.rotation_y += delta.x() * 0.5
            self.rotation_x += delta.y() * 0.5
            self.rotation_x = max(-89, min(89, self.rotation_x))
            self._last_pos = event.pos()
            self.update()

    def wheelEvent(self, event):
        delta = event.angleDelta().y() / 120
        self.zoom = max(0.5, min(3.0, self.zoom + delta * 0.1))
        self.update()


# =============================================================================
# Slice View Widget
# =============================================================================

class LUTSliceView(QWidget):
    """2D slice view of LUT (R, G, or B plane)."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(200, 200)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        # LUT data
        self.lut: Optional[LUT3D] = None

        # Slice settings
        self.slice_axis = 'B'  # R, G, or B
        self.slice_position = 0.5  # 0.0 to 1.0

        # Pre-rendered image
        self._slice_image: Optional[QImage] = None

    def set_lut(self, lut: LUT3D):
        """Set the LUT to display."""
        self.lut = lut
        self._render_slice()
        self.update()

    def set_slice(self, axis: str, position: float):
        """Set slice axis and position."""
        self.slice_axis = axis
        self.slice_position = max(0.0, min(1.0, position))
        self._render_slice()
        self.update()

    def _render_slice(self):
        """Render the current slice to an image."""
        if not self.lut:
            self._slice_image = None
            return

        size = 256  # Output image size
        image = QImage(size, size, QImage.Format.Format_RGB32)

        lut_size = self.lut.size
        slice_idx = int(self.slice_position * (lut_size - 1))

        for y in range(size):
            for x in range(size):
                # Map to LUT coordinates
                u = x / (size - 1)
                v = 1.0 - y / (size - 1)  # Flip Y

                # Get LUT indices based on slice axis
                if self.slice_axis == 'R':
                    r_idx = slice_idx
                    g_idx = int(v * (lut_size - 1))
                    b_idx = int(u * (lut_size - 1))
                elif self.slice_axis == 'G':
                    r_idx = int(v * (lut_size - 1))
                    g_idx = slice_idx
                    b_idx = int(u * (lut_size - 1))
                else:  # B
                    r_idx = int(v * (lut_size - 1))
                    g_idx = int(u * (lut_size - 1))
                    b_idx = slice_idx

                # Get color from LUT
                r, g, b = self.lut.data[
                    min(r_idx, lut_size - 1),
                    min(g_idx, lut_size - 1),
                    min(b_idx, lut_size - 1)
                ]

                color = QColor(
                    int(np.clip(r, 0, 1) * 255),
                    int(np.clip(g, 0, 1) * 255),
                    int(np.clip(b, 0, 1) * 255)
                )
                image.setPixelColor(x, y, color)

        self._slice_image = image

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)

        # Background
        painter.fillRect(self.rect(), QColor(COLORS['background']))

        if self._slice_image:
            # Scale image to widget size
            scaled = self._slice_image.scaled(
                self.width(), self.height(),
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation
            )

            # Center the image
            x = (self.width() - scaled.width()) // 2
            y = (self.height() - scaled.height()) // 2
            painter.drawImage(x, y, scaled)

            # Draw border
            painter.setPen(QPen(QColor(COLORS['border']), 1))
            painter.drawRect(x, y, scaled.width() - 1, scaled.height() - 1)
        else:
            painter.setPen(QColor(COLORS['text_secondary']))
            painter.setFont(QFont("Segoe UI", 10))
            painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No LUT loaded")

        # Draw axis labels
        if self._slice_image:
            painter.setFont(QFont("Segoe UI", 9))

            if self.slice_axis == 'R':
                h_label, v_label = 'B', 'G'
            elif self.slice_axis == 'G':
                h_label, v_label = 'B', 'R'
            else:
                h_label, v_label = 'G', 'R'

            # Horizontal axis
            painter.setPen(QColor(COLORS['text_secondary']))
            painter.drawText(self.width() // 2 - 5, self.height() - 5, h_label)

            # Vertical axis
            painter.save()
            painter.translate(10, self.height() // 2)
            painter.rotate(-90)
            painter.drawText(0, 0, v_label)
            painter.restore()


# =============================================================================
# Before/After Comparison
# =============================================================================

class BeforeAfterView(QWidget):
    """Side-by-side or split comparison view."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(400, 200)
        self.setMouseTracking(True)

        # Source image
        self._source_image: Optional[QImage] = None
        self._processed_image: Optional[QImage] = None

        # View mode
        self.split_position = 0.5  # For split view
        self._dragging = False

    def set_images(self, source: QImage, processed: QImage):
        """Set source and processed images."""
        self._source_image = source
        self._processed_image = processed
        self.update()

    def set_source_from_gradient(self):
        """Create a test gradient image."""
        size = 256
        image = QImage(size, size, QImage.Format.Format_RGB32)

        for y in range(size):
            for x in range(size):
                r = int((x / (size - 1)) * 255)
                g = int((y / (size - 1)) * 255)
                b = 128
                image.setPixelColor(x, y, QColor(r, g, b))

        self._source_image = image
        self.update()

    def apply_lut(self, lut: LUT3D):
        """Apply LUT to source image."""
        if not self._source_image:
            return

        size = self._source_image.width()
        processed = QImage(size, size, QImage.Format.Format_RGB32)

        for y in range(size):
            for x in range(size):
                color = self._source_image.pixelColor(x, y)
                r = color.red() / 255.0
                g = color.green() / 255.0
                b = color.blue() / 255.0

                out_r, out_g, out_b = lut.lookup(r, g, b)

                processed.setPixelColor(x, y, QColor(
                    int(out_r * 255),
                    int(out_g * 255),
                    int(out_b * 255)
                ))

        self._processed_image = processed
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)

        # Background
        painter.fillRect(self.rect(), QColor(COLORS['background']))

        if not self._source_image or not self._processed_image:
            painter.setPen(QColor(COLORS['text_secondary']))
            painter.setFont(QFont("Segoe UI", 10))
            painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Load image to compare")
            return

        # Calculate split position
        split_x = int(self.width() * self.split_position)

        # Scale images to widget size
        source_scaled = self._source_image.scaled(
            self.width(), self.height(),
            Qt.AspectRatioMode.IgnoreAspectRatio,
            Qt.TransformationMode.SmoothTransformation
        )
        processed_scaled = self._processed_image.scaled(
            self.width(), self.height(),
            Qt.AspectRatioMode.IgnoreAspectRatio,
            Qt.TransformationMode.SmoothTransformation
        )

        # Draw left side (original)
        painter.setClipRect(0, 0, split_x, self.height())
        painter.drawImage(0, 0, source_scaled)

        # Draw right side (processed)
        painter.setClipRect(split_x, 0, self.width() - split_x, self.height())
        painter.drawImage(0, 0, processed_scaled)

        # Reset clip
        painter.setClipping(False)

        # Draw split line
        painter.setPen(QPen(QColor("#ffffff"), 2))
        painter.drawLine(split_x, 0, split_x, self.height())

        # Labels
        painter.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        painter.drawText(10, 20, "Before")
        painter.drawText(self.width() - 50, 20, "After")

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self._dragging = True
            self.split_position = event.pos().x() / self.width()
            self.update()

    def mouseReleaseEvent(self, event):
        self._dragging = False

    def mouseMoveEvent(self, event):
        if self._dragging:
            self.split_position = max(0.1, min(0.9, event.pos().x() / self.width()))
            self.update()


# =============================================================================
# Main LUT Preview Widget
# =============================================================================

class LUTPreviewWidget(QWidget):
    """Complete LUT preview with multiple views."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()

        # Current LUT
        self.lut: Optional[LUT3D] = None

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # View tabs
        self.tabs = QTabWidget()
        self.tabs.setStyleSheet(f"""
            QTabWidget::pane {{
                border: 1px solid {COLORS['border']};
                background-color: {COLORS['background']};
            }}
            QTabBar::tab {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                padding: 8px 16px;
                margin-right: 2px;
            }}
            QTabBar::tab:selected {{
                background-color: {COLORS['accent']};
                color: white;
            }}
        """)

        # 3D Cube view
        self.cube_view = LUTCubeView()
        self.tabs.addTab(self.cube_view, "3D View")

        # Slice view container
        slice_widget = QWidget()
        slice_layout = QVBoxLayout(slice_widget)

        # Slice controls
        controls_layout = QHBoxLayout()
        controls_layout.addWidget(QLabel("Axis:"))

        self.axis_combo = QComboBox()
        self.axis_combo.addItems(["R", "G", "B"])
        self.axis_combo.setCurrentText("B")
        self.axis_combo.currentTextChanged.connect(self._update_slice)
        controls_layout.addWidget(self.axis_combo)

        controls_layout.addWidget(QLabel("Position:"))

        self.slice_slider = QSlider(Qt.Orientation.Horizontal)
        self.slice_slider.setRange(0, 100)
        self.slice_slider.setValue(50)
        self.slice_slider.valueChanged.connect(self._update_slice)
        controls_layout.addWidget(self.slice_slider)

        self.slice_label = QLabel("50%")
        controls_layout.addWidget(self.slice_label)

        slice_layout.addLayout(controls_layout)

        self.slice_view = LUTSliceView()
        slice_layout.addWidget(self.slice_view)

        self.tabs.addTab(slice_widget, "Slice View")

        # Before/After comparison
        self.comparison_view = BeforeAfterView()
        self.tabs.addTab(self.comparison_view, "Compare")

        layout.addWidget(self.tabs)

        # Stats panel
        self.stats_panel = QFrame()
        self.stats_panel.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                padding: 8px;
            }}
        """)
        stats_layout = QHBoxLayout(self.stats_panel)

        self.size_label = QLabel("Size: --")
        stats_layout.addWidget(self.size_label)

        self.deviation_label = QLabel("Avg Deviation: --")
        stats_layout.addWidget(self.deviation_label)

        self.max_dev_label = QLabel("Max Deviation: --")
        stats_layout.addWidget(self.max_dev_label)

        stats_layout.addStretch()

        layout.addWidget(self.stats_panel)

    def load_lut(self, lut: LUT3D):
        """Load a LUT for preview."""
        self.lut = lut

        # Update views
        self.cube_view.set_lut(lut)
        self.slice_view.set_lut(lut)

        # Update comparison
        self.comparison_view.set_source_from_gradient()
        self.comparison_view.apply_lut(lut)

        # Update stats
        self.size_label.setText(f"Size: {lut.size}³")

        stats = lut.get_deviation_stats()
        self.deviation_label.setText(f"Avg: {stats['avg_deviation'] * 100:.2f}%")
        self.max_dev_label.setText(f"Max: {stats['max_deviation'] * 100:.2f}%")

        self._update_slice()

    def load_identity(self, size: int = 33):
        """Load an identity LUT."""
        self.load_lut(LUT3D.identity(size))

    def _update_slice(self):
        """Update slice view from controls."""
        if self.lut:
            axis = self.axis_combo.currentText()
            position = self.slice_slider.value() / 100.0
            self.slice_label.setText(f"{int(position * 100)}%")
            self.slice_view.set_slice(axis, position)
