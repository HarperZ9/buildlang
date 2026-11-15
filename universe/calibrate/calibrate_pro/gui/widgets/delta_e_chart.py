"""
Delta E Chart Widget

Displays Delta E (color difference) measurements:
- Bar chart of per-patch Delta E
- Threshold indicators (excellent, good, acceptable)
- Statistics summary
"""

from typing import Optional, List, Tuple
from dataclasses import dataclass
from enum import Enum, auto

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QFrame,
    QSizePolicy, QToolTip
)
from PyQt6.QtCore import Qt, QRectF, QPointF, pyqtSignal
from PyQt6.QtGui import (
    QPainter, QPen, QBrush, QColor, QPainterPath,
    QLinearGradient, QFont
)


# =============================================================================
# Delta E Classifications
# =============================================================================

class DeltaEQuality(Enum):
    """Quality classification based on Delta E value."""
    IMPERCEPTIBLE = auto()   # < 1.0
    EXCELLENT = auto()       # < 2.0
    GOOD = auto()            # < 3.0
    ACCEPTABLE = auto()      # < 5.0
    NOTICEABLE = auto()      # < 10.0
    POOR = auto()            # >= 10.0


def classify_delta_e(value: float) -> DeltaEQuality:
    """Classify a Delta E value."""
    if value < 1.0:
        return DeltaEQuality.IMPERCEPTIBLE
    elif value < 2.0:
        return DeltaEQuality.EXCELLENT
    elif value < 3.0:
        return DeltaEQuality.GOOD
    elif value < 5.0:
        return DeltaEQuality.ACCEPTABLE
    elif value < 10.0:
        return DeltaEQuality.NOTICEABLE
    else:
        return DeltaEQuality.POOR


def get_delta_e_color(value: float) -> QColor:
    """Get color for a Delta E value."""
    if value < 1.0:
        return QColor("#4caf50")  # Green
    elif value < 2.0:
        return QColor("#8bc34a")  # Light green
    elif value < 3.0:
        return QColor("#ffeb3b")  # Yellow
    elif value < 5.0:
        return QColor("#ff9800")  # Orange
    else:
        return QColor("#f44336")  # Red


@dataclass
class DeltaEMeasurement:
    """A single Delta E measurement."""
    label: str
    value: float
    target_color: Tuple[int, int, int] = (128, 128, 128)  # RGB
    measured_color: Tuple[int, int, int] = (128, 128, 128)


# =============================================================================
# Delta E Bar Chart
# =============================================================================

class DeltaEBarChart(QWidget):
    """Bar chart showing Delta E values for multiple patches."""

    bar_clicked = pyqtSignal(int, DeltaEMeasurement)  # index, measurement
    bar_hovered = pyqtSignal(int, DeltaEMeasurement)

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(400, 200)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setMouseTracking(True)

        # Data
        self.measurements: List[DeltaEMeasurement] = []

        # Display options
        self.show_grid = True
        self.show_thresholds = True
        self.show_labels = True
        self.show_average_line = True
        self.max_display_value = 10.0  # Auto-scale if exceeded

        # View settings
        self.margin_left = 50
        self.margin_right = 20
        self.margin_top = 30
        self.margin_bottom = 60
        self.bar_spacing = 2

        # Threshold lines
        self.thresholds = [
            (1.0, QColor("#4caf50"), "Imperceptible"),
            (2.0, QColor("#8bc34a"), "Excellent"),
            (3.0, QColor("#ff9800"), "Good"),
        ]

        # Hover state
        self._hover_index: int = -1

    def set_measurements(self, measurements: List[DeltaEMeasurement]):
        """Set the measurements to display."""
        self.measurements = measurements

        # Auto-scale if needed
        if measurements:
            max_val = max(m.value for m in measurements)
            self.max_display_value = max(10.0, max_val * 1.1)

        self.update()

    def add_measurement(self, label: str, value: float,
                       target_rgb: Tuple[int, int, int] = None,
                       measured_rgb: Tuple[int, int, int] = None):
        """Add a single measurement."""
        m = DeltaEMeasurement(
            label=label,
            value=value,
            target_color=target_rgb or (128, 128, 128),
            measured_color=measured_rgb or (128, 128, 128)
        )
        self.measurements.append(m)
        self.update()

    def clear(self):
        """Clear all measurements."""
        self.measurements.clear()
        self.update()

    def _get_bar_rect(self, index: int) -> QRectF:
        """Get the rectangle for a bar at given index."""
        if not self.measurements:
            return QRectF()

        plot_width = self.width() - self.margin_left - self.margin_right
        plot_height = self.height() - self.margin_top - self.margin_bottom

        bar_width = (plot_width - self.bar_spacing * (len(self.measurements) - 1)) / len(self.measurements)

        x = self.margin_left + index * (bar_width + self.bar_spacing)
        height = (self.measurements[index].value / self.max_display_value) * plot_height
        y = self.height() - self.margin_bottom - height

        return QRectF(x, y, bar_width, height)

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        # Background
        painter.fillRect(self.rect(), QColor("#1a1a1a"))

        if not self.measurements:
            # No data message
            painter.setPen(QColor("#808080"))
            painter.setFont(QFont("Segoe UI", 12))
            painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No measurements")
            return

        # Draw grid
        if self.show_grid:
            self._draw_grid(painter)

        # Draw threshold lines
        if self.show_thresholds:
            self._draw_thresholds(painter)

        # Draw bars
        self._draw_bars(painter)

        # Draw average line
        if self.show_average_line:
            self._draw_average(painter)

        # Draw axes
        self._draw_axes(painter)

    def _draw_grid(self, painter: QPainter):
        """Draw horizontal grid lines."""
        painter.setPen(QPen(QColor("#303030"), 1, Qt.PenStyle.DotLine))

        plot_height = self.height() - self.margin_top - self.margin_bottom

        for value in [2, 4, 6, 8, 10]:
            if value > self.max_display_value:
                break
            y = self.height() - self.margin_bottom - (value / self.max_display_value) * plot_height
            painter.drawLine(self.margin_left, int(y), self.width() - self.margin_right, int(y))

    def _draw_thresholds(self, painter: QPainter):
        """Draw threshold indicator lines."""
        plot_height = self.height() - self.margin_top - self.margin_bottom

        for value, color, label in self.thresholds:
            if value > self.max_display_value:
                continue

            y = self.height() - self.margin_bottom - (value / self.max_display_value) * plot_height

            painter.setPen(QPen(color, 1, Qt.PenStyle.DashLine))
            painter.drawLine(self.margin_left, int(y), self.width() - self.margin_right, int(y))

            # Label
            painter.setFont(QFont("Segoe UI", 8))
            painter.drawText(self.width() - self.margin_right + 5, int(y) + 4, f"ΔE {value}")

    def _draw_bars(self, painter: QPainter):
        """Draw the Delta E bars."""
        for i, measurement in enumerate(self.measurements):
            rect = self._get_bar_rect(i)
            color = get_delta_e_color(measurement.value)

            # Highlight hovered bar
            if i == self._hover_index:
                color = color.lighter(130)
                # Draw glow effect
                glow_color = QColor(color)
                glow_color.setAlpha(50)
                painter.fillRect(rect.adjusted(-2, -2, 2, 2), glow_color)

            # Bar fill with gradient
            gradient = QLinearGradient(rect.topLeft(), rect.bottomLeft())
            gradient.setColorAt(0, color.lighter(120))
            gradient.setColorAt(1, color)
            painter.fillRect(rect, gradient)

            # Bar outline
            painter.setPen(QPen(color.darker(120), 1))
            painter.drawRect(rect)

            # Value label on top
            if self.show_labels and rect.height() > 20:
                painter.setPen(QColor("#ffffff"))
                painter.setFont(QFont("Segoe UI", 8, QFont.Weight.Bold))
                text = f"{measurement.value:.2f}"
                text_rect = painter.fontMetrics().boundingRect(text)
                text_x = rect.center().x() - text_rect.width() / 2
                text_y = rect.top() - 5
                painter.drawText(int(text_x), int(text_y), text)

    def _draw_average(self, painter: QPainter):
        """Draw average Delta E line."""
        if not self.measurements:
            return

        avg = sum(m.value for m in self.measurements) / len(self.measurements)
        plot_height = self.height() - self.margin_top - self.margin_bottom
        y = self.height() - self.margin_bottom - (avg / self.max_display_value) * plot_height

        painter.setPen(QPen(QColor("#ffffff"), 2))
        painter.drawLine(self.margin_left, int(y), self.width() - self.margin_right, int(y))

        # Label
        painter.setFont(QFont("Segoe UI", 9, QFont.Weight.Bold))
        painter.drawText(self.margin_left + 5, int(y) - 5, f"Avg: {avg:.2f}")

    def _draw_axes(self, painter: QPainter):
        """Draw axes and labels."""
        # Y axis
        painter.setPen(QPen(QColor("#505050"), 2))
        painter.drawLine(
            self.margin_left, self.margin_top,
            self.margin_left, self.height() - self.margin_bottom
        )

        # X axis
        painter.drawLine(
            self.margin_left, self.height() - self.margin_bottom,
            self.width() - self.margin_right, self.height() - self.margin_bottom
        )

        # Y axis labels
        painter.setPen(QColor("#808080"))
        painter.setFont(QFont("Segoe UI", 9))

        plot_height = self.height() - self.margin_top - self.margin_bottom
        for value in range(0, int(self.max_display_value) + 1, 2):
            y = self.height() - self.margin_bottom - (value / self.max_display_value) * plot_height
            painter.drawText(5, int(y) + 4, f"{value}")

        # Y axis title
        painter.save()
        painter.translate(15, self.height() // 2)
        painter.rotate(-90)
        painter.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
        painter.drawText(0, 0, "Delta E (ΔE)")
        painter.restore()

        # X axis labels (patch labels)
        if self.show_labels:
            painter.setFont(QFont("Segoe UI", 8))
            for i, measurement in enumerate(self.measurements):
                rect = self._get_bar_rect(i)
                # Rotate labels if there are many
                if len(self.measurements) > 12:
                    painter.save()
                    painter.translate(rect.center().x(), self.height() - self.margin_bottom + 10)
                    painter.rotate(45)
                    painter.drawText(0, 0, measurement.label[:8])
                    painter.restore()
                else:
                    painter.drawText(
                        int(rect.center().x() - 15),
                        self.height() - self.margin_bottom + 15,
                        measurement.label[:6]
                    )

    def mouseMoveEvent(self, event):
        # Find bar under cursor
        old_hover = self._hover_index
        self._hover_index = -1

        for i in range(len(self.measurements)):
            rect = self._get_bar_rect(i)
            if rect.contains(event.pos().x(), event.pos().y()):
                self._hover_index = i
                self.bar_hovered.emit(i, self.measurements[i])

                # Show tooltip
                m = self.measurements[i]
                tooltip = (
                    f"<b>{m.label}</b><br>"
                    f"Delta E: {m.value:.3f}<br>"
                    f"Quality: {classify_delta_e(m.value).name.replace('_', ' ').title()}"
                )
                QToolTip.showText(event.globalPosition().toPoint(), tooltip, self)
                break

        if old_hover != self._hover_index:
            self.update()

    def leaveEvent(self, event):
        self._hover_index = -1
        self.update()

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton and self._hover_index >= 0:
            self.bar_clicked.emit(self._hover_index, self.measurements[self._hover_index])


# =============================================================================
# Delta E Statistics Panel
# =============================================================================

class DeltaEStatsPanel(QWidget):
    """Statistics summary for Delta E measurements."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self):
        layout = QHBoxLayout(self)
        layout.setContentsMargins(16, 8, 16, 8)
        layout.setSpacing(32)

        # Average
        self.avg_frame = self._create_stat_box("Average ΔE", "--")
        layout.addWidget(self.avg_frame)

        # Max
        self.max_frame = self._create_stat_box("Max ΔE", "--")
        layout.addWidget(self.max_frame)

        # Min
        self.min_frame = self._create_stat_box("Min ΔE", "--")
        layout.addWidget(self.min_frame)

        # 95th percentile
        self.p95_frame = self._create_stat_box("95th %ile", "--")
        layout.addWidget(self.p95_frame)

        # Passing
        self.pass_frame = self._create_stat_box("Passing", "--%")
        layout.addWidget(self.pass_frame)

        layout.addStretch()

    def _create_stat_box(self, label: str, value: str) -> QFrame:
        """Create a statistics display box."""
        frame = QFrame()
        frame.setStyleSheet("""
            QFrame {
                background-color: #2d2d2d;
                border: 1px solid #404040;
                border-radius: 8px;
                padding: 8px;
            }
        """)

        layout = QVBoxLayout(frame)
        layout.setContentsMargins(16, 8, 16, 8)
        layout.setSpacing(4)

        label_widget = QLabel(label)
        label_widget.setStyleSheet("color: #808080; font-size: 11px;")
        layout.addWidget(label_widget)

        value_widget = QLabel(value)
        value_widget.setStyleSheet("color: #e0e0e0; font-size: 18px; font-weight: 600;")
        value_widget.setObjectName("value")
        layout.addWidget(value_widget)

        return frame

    def update_stats(self, measurements: List[DeltaEMeasurement], threshold: float = 2.0):
        """Update statistics from measurements."""
        if not measurements:
            return

        values = [m.value for m in measurements]

        # Calculate stats
        avg = sum(values) / len(values)
        max_val = max(values)
        min_val = min(values)

        # 95th percentile
        sorted_vals = sorted(values)
        p95_idx = int(len(sorted_vals) * 0.95)
        p95 = sorted_vals[p95_idx] if p95_idx < len(sorted_vals) else sorted_vals[-1]

        # Passing rate (below threshold)
        passing = sum(1 for v in values if v < threshold) / len(values) * 100

        # Update displays
        self._update_stat(self.avg_frame, f"{avg:.2f}", avg)
        self._update_stat(self.max_frame, f"{max_val:.2f}", max_val)
        self._update_stat(self.min_frame, f"{min_val:.2f}", min_val)
        self._update_stat(self.p95_frame, f"{p95:.2f}", p95)

        pass_widget = self.pass_frame.findChild(QLabel, "value")
        if pass_widget:
            pass_widget.setText(f"{passing:.0f}%")
            color = "#4caf50" if passing >= 90 else "#ff9800" if passing >= 70 else "#f44336"
            pass_widget.setStyleSheet(f"color: {color}; font-size: 18px; font-weight: 600;")

    def _update_stat(self, frame: QFrame, text: str, value: float):
        """Update a stat box with color coding."""
        widget = frame.findChild(QLabel, "value")
        if widget:
            widget.setText(text)
            color = get_delta_e_color(value).name()
            widget.setStyleSheet(f"color: {color}; font-size: 18px; font-weight: 600;")
