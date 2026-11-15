"""
Measurement View - Live calibration measurement display

Displays real-time calibration measurements:
- Current patch color
- Target vs measured values
- Delta E in real-time
- Measurement history
"""

from typing import Optional, List, Tuple, Dict
from dataclasses import dataclass, field
from datetime import datetime

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QFrame,
    QGridLayout, QProgressBar, QScrollArea, QSizePolicy,
    QTableWidget, QTableWidgetItem, QHeaderView
)
from PyQt6.QtCore import Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QFont, QPainter, QBrush, QPen


# =============================================================================
# Theme Colors
# =============================================================================

COLORS = {
    "background": "#1a1a1a",
    "surface": "#2d2d2d",
    "surface_alt": "#383838",
    "border": "#404040",
    "text_primary": "#e0e0e0",
    "text_secondary": "#a0a0a0",
    "accent": "#4a9eff",
    "success": "#4caf50",
    "warning": "#ff9800",
    "error": "#f44336",
}


# =============================================================================
# Measurement Data
# =============================================================================

@dataclass
class Measurement:
    """A single calibration measurement."""
    index: int
    timestamp: datetime = field(default_factory=datetime.now)

    # Target values
    target_rgb: Tuple[int, int, int] = (128, 128, 128)
    target_xyz: Tuple[float, float, float] = (0.0, 0.0, 0.0)
    target_lab: Tuple[float, float, float] = (50.0, 0.0, 0.0)

    # Measured values
    measured_xyz: Tuple[float, float, float] = (0.0, 0.0, 0.0)
    measured_lab: Tuple[float, float, float] = (50.0, 0.0, 0.0)
    measured_xy: Tuple[float, float] = (0.3127, 0.3290)

    # Delta E
    delta_e: float = 0.0
    delta_l: float = 0.0
    delta_c: float = 0.0
    delta_h: float = 0.0


# =============================================================================
# Live Color Patch Display
# =============================================================================

class ColorPatchDisplay(QFrame):
    """Large color patch showing current target and measured colors."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(200, 200)
        self.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
            }}
        """)

        self._target_color = QColor(128, 128, 128)
        self._measured_color = QColor(128, 128, 128)
        self._show_split = True

    def set_target(self, r: int, g: int, b: int):
        """Set target color."""
        self._target_color = QColor(r, g, b)
        self.update()

    def set_measured(self, r: int, g: int, b: int):
        """Set measured color."""
        self._measured_color = QColor(r, g, b)
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        rect = self.rect().adjusted(2, 2, -2, -2)
        half_width = rect.width() // 2

        if self._show_split:
            # Left: Target
            painter.fillRect(rect.x(), rect.y(), half_width, rect.height(), self._target_color)

            # Right: Measured
            painter.fillRect(rect.x() + half_width, rect.y(), half_width, rect.height(), self._measured_color)

            # Divider
            painter.setPen(QPen(QColor("#ffffff"), 2))
            painter.drawLine(rect.x() + half_width, rect.y(), rect.x() + half_width, rect.bottom())

            # Labels
            painter.setFont(QFont("Segoe UI", 10))
            painter.setPen(QColor("#ffffff" if self._target_color.lightness() < 128 else "#000000"))
            painter.drawText(rect.x() + 10, rect.y() + 25, "Target")

            painter.setPen(QColor("#ffffff" if self._measured_color.lightness() < 128 else "#000000"))
            painter.drawText(rect.x() + half_width + 10, rect.y() + 25, "Measured")
        else:
            painter.fillRect(rect, self._measured_color)


# =============================================================================
# Delta E Display
# =============================================================================

class DeltaEDisplay(QFrame):
    """Large Delta E value display with color coding."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setMinimumSize(150, 100)
        self.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
            }}
        """)

        self._delta_e = 0.0
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)

        self.label = QLabel("ΔE")
        self.label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 14px;")
        layout.addWidget(self.label)

        self.value_label = QLabel("--")
        self.value_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.value_label.setStyleSheet("font-size: 36px; font-weight: bold;")
        layout.addWidget(self.value_label)

        self.quality_label = QLabel("")
        self.quality_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.quality_label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 12px;")
        layout.addWidget(self.quality_label)

    def set_value(self, delta_e: float):
        """Set Delta E value with color coding."""
        self._delta_e = delta_e

        # Color code
        if delta_e < 1.0:
            color = COLORS['success']
            quality = "Imperceptible"
        elif delta_e < 2.0:
            color = "#8bc34a"
            quality = "Excellent"
        elif delta_e < 3.0:
            color = COLORS['warning']
            quality = "Good"
        elif delta_e < 5.0:
            color = "#ff5722"
            quality = "Acceptable"
        else:
            color = COLORS['error']
            quality = "Poor"

        self.value_label.setText(f"{delta_e:.3f}")
        self.value_label.setStyleSheet(f"font-size: 36px; font-weight: bold; color: {color};")
        self.quality_label.setText(quality)
        self.quality_label.setStyleSheet(f"color: {color}; font-size: 12px;")


# =============================================================================
# Values Panel
# =============================================================================

class ValuesPanel(QFrame):
    """Display panel for color values (XYZ, Lab, xy)."""

    def __init__(self, title: str = "Values", parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
                padding: 12px;
            }}
        """)

        self._setup_ui(title)

    def _setup_ui(self, title: str):
        layout = QVBoxLayout(self)
        layout.setSpacing(8)

        # Title
        title_label = QLabel(title)
        title_label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-weight: 600;")
        layout.addWidget(title_label)

        # Values grid
        self.grid = QGridLayout()
        self.grid.setSpacing(4)

        self.value_labels = {}

        rows = [
            ("X", "x"),
            ("Y", "y"),
            ("Z", "z"),
            ("L*", "l"),
            ("a*", "a"),
            ("b*", "b"),
        ]

        for i, (label, key) in enumerate(rows):
            label_widget = QLabel(f"{label}:")
            label_widget.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 11px;")

            value_widget = QLabel("--")
            value_widget.setStyleSheet("font-family: monospace; font-size: 12px;")
            value_widget.setAlignment(Qt.AlignmentFlag.AlignRight)

            self.value_labels[key] = value_widget

            self.grid.addWidget(label_widget, i, 0)
            self.grid.addWidget(value_widget, i, 1)

        layout.addLayout(self.grid)

    def set_xyz(self, X: float, Y: float, Z: float):
        """Set XYZ values."""
        self.value_labels["x"].setText(f"{X:.4f}")
        self.value_labels["y"].setText(f"{Y:.4f}")
        self.value_labels["z"].setText(f"{Z:.4f}")

    def set_lab(self, L: float, a: float, b: float):
        """Set Lab values."""
        self.value_labels["l"].setText(f"{L:.2f}")
        self.value_labels["a"].setText(f"{a:+.2f}")
        self.value_labels["b"].setText(f"{b:+.2f}")


# =============================================================================
# Measurement History Table
# =============================================================================

class MeasurementHistoryTable(QWidget):
    """Table showing measurement history."""

    row_selected = pyqtSignal(int)

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self.table = QTableWidget()
        self.table.setColumnCount(5)
        self.table.setHorizontalHeaderLabels(["#", "RGB", "XYZ", "Lab", "ΔE"])
        self.table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self.table.setStyleSheet(f"""
            QTableWidget {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                gridline-color: {COLORS['border']};
            }}
            QTableWidget::item {{
                padding: 4px;
            }}
            QTableWidget::item:selected {{
                background-color: {COLORS['accent']};
            }}
            QHeaderView::section {{
                background-color: {COLORS['surface_alt']};
                border: none;
                border-bottom: 1px solid {COLORS['border']};
                padding: 8px;
            }}
        """)

        self.table.itemSelectionChanged.connect(self._on_selection_changed)
        layout.addWidget(self.table)

    def add_measurement(self, measurement: Measurement):
        """Add a measurement to the table."""
        row = self.table.rowCount()
        self.table.insertRow(row)

        # Index
        self.table.setItem(row, 0, QTableWidgetItem(str(measurement.index + 1)))

        # RGB
        r, g, b = measurement.target_rgb
        rgb_item = QTableWidgetItem(f"({r}, {g}, {b})")
        rgb_item.setBackground(QBrush(QColor(r, g, b)))
        rgb_item.setForeground(QBrush(QColor("#fff" if (r + g + b) / 3 < 128 else "#000")))
        self.table.setItem(row, 1, rgb_item)

        # XYZ
        X, Y, Z = measurement.measured_xyz
        self.table.setItem(row, 2, QTableWidgetItem(f"{X:.2f}, {Y:.2f}, {Z:.2f}"))

        # Lab
        L, a, b = measurement.measured_lab
        self.table.setItem(row, 3, QTableWidgetItem(f"{L:.1f}, {a:+.1f}, {b:+.1f}"))

        # Delta E
        de = measurement.delta_e
        de_item = QTableWidgetItem(f"{de:.3f}")

        if de < 1.0:
            de_item.setForeground(QBrush(QColor(COLORS['success'])))
        elif de < 3.0:
            de_item.setForeground(QBrush(QColor(COLORS['warning'])))
        else:
            de_item.setForeground(QBrush(QColor(COLORS['error'])))

        self.table.setItem(row, 4, de_item)

        # Scroll to new row
        self.table.scrollToBottom()

    def clear(self):
        """Clear all measurements."""
        self.table.setRowCount(0)

    def _on_selection_changed(self):
        """Handle row selection."""
        rows = self.table.selectedIndexes()
        if rows:
            self.row_selected.emit(rows[0].row())


# =============================================================================
# Main Measurement View
# =============================================================================

class MeasurementView(QWidget):
    """Complete measurement view for calibration."""

    measurement_complete = pyqtSignal(Measurement)

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.measurements: List[Measurement] = []
        self._current_index = 0
        self._setup_ui()

    def _setup_ui(self):
        layout = QHBoxLayout(self)
        layout.setSpacing(16)

        # Left panel: Current measurement
        left_panel = QVBoxLayout()

        # Color patch
        self.color_patch = ColorPatchDisplay()
        left_panel.addWidget(self.color_patch)

        # Delta E display
        self.delta_e_display = DeltaEDisplay()
        left_panel.addWidget(self.delta_e_display)

        # Progress
        progress_frame = QFrame()
        progress_frame.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
                padding: 12px;
            }}
        """)
        progress_layout = QVBoxLayout(progress_frame)

        self.progress_label = QLabel("Measurement 0 of 0")
        self.progress_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        progress_layout.addWidget(self.progress_label)

        self.progress_bar = QProgressBar()
        self.progress_bar.setStyleSheet(f"""
            QProgressBar {{
                background-color: {COLORS['background']};
                border: none;
                border-radius: 4px;
                height: 8px;
            }}
            QProgressBar::chunk {{
                background-color: {COLORS['accent']};
                border-radius: 4px;
            }}
        """)
        progress_layout.addWidget(self.progress_bar)

        left_panel.addWidget(progress_frame)
        left_panel.addStretch()

        layout.addLayout(left_panel, 1)

        # Middle panel: Values
        values_panel = QVBoxLayout()

        self.target_values = ValuesPanel("Target")
        values_panel.addWidget(self.target_values)

        self.measured_values = ValuesPanel("Measured")
        values_panel.addWidget(self.measured_values)

        values_panel.addStretch()

        layout.addLayout(values_panel, 1)

        # Right panel: History
        history_panel = QVBoxLayout()

        history_label = QLabel("Measurement History")
        history_label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-weight: 600;")
        history_panel.addWidget(history_label)

        self.history_table = MeasurementHistoryTable()
        history_panel.addWidget(self.history_table)

        # Statistics
        stats_frame = QFrame()
        stats_frame.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
                padding: 12px;
            }}
        """)
        stats_layout = QGridLayout(stats_frame)

        self.avg_label = QLabel("Avg ΔE: --")
        stats_layout.addWidget(self.avg_label, 0, 0)

        self.max_label = QLabel("Max ΔE: --")
        stats_layout.addWidget(self.max_label, 0, 1)

        self.min_label = QLabel("Min ΔE: --")
        stats_layout.addWidget(self.min_label, 1, 0)

        self.p95_label = QLabel("95th %: --")
        stats_layout.addWidget(self.p95_label, 1, 1)

        history_panel.addWidget(stats_frame)

        layout.addLayout(history_panel, 2)

    def start_measurement_sequence(self, total_patches: int):
        """Start a new measurement sequence."""
        self.measurements.clear()
        self.history_table.clear()
        self._current_index = 0
        self.progress_bar.setMaximum(total_patches)
        self.progress_bar.setValue(0)
        self._update_progress_label(total_patches)

    def set_target(self, rgb: Tuple[int, int, int], xyz: Tuple[float, float, float],
                   lab: Tuple[float, float, float]):
        """Set current target values."""
        self.color_patch.set_target(*rgb)
        self.target_values.set_xyz(*xyz)
        self.target_values.set_lab(*lab)

    def set_measured(self, xyz: Tuple[float, float, float], lab: Tuple[float, float, float],
                     delta_e: float):
        """Set measured values and Delta E."""
        # Convert XYZ to RGB for display (simplified)
        r = int(min(255, max(0, xyz[0] * 2.55)))
        g = int(min(255, max(0, xyz[1] * 2.55)))
        b = int(min(255, max(0, xyz[2] * 2.55)))

        self.color_patch.set_measured(r, g, b)
        self.measured_values.set_xyz(*xyz)
        self.measured_values.set_lab(*lab)
        self.delta_e_display.set_value(delta_e)

    def record_measurement(self, measurement: Measurement):
        """Record a completed measurement."""
        self.measurements.append(measurement)
        self.history_table.add_measurement(measurement)
        self._current_index += 1
        self.progress_bar.setValue(self._current_index)
        self._update_progress_label(self.progress_bar.maximum())
        self._update_statistics()
        self.measurement_complete.emit(measurement)

    def _update_progress_label(self, total: int):
        """Update progress label."""
        self.progress_label.setText(f"Measurement {self._current_index} of {total}")

    def _update_statistics(self):
        """Update statistics display."""
        if not self.measurements:
            return

        values = [m.delta_e for m in self.measurements]

        avg = sum(values) / len(values)
        max_val = max(values)
        min_val = min(values)

        sorted_vals = sorted(values)
        p95_idx = int(len(sorted_vals) * 0.95)
        p95 = sorted_vals[p95_idx] if p95_idx < len(sorted_vals) else sorted_vals[-1]

        self.avg_label.setText(f"Avg ΔE: {avg:.3f}")
        self.max_label.setText(f"Max ΔE: {max_val:.3f}")
        self.min_label.setText(f"Min ΔE: {min_val:.3f}")
        self.p95_label.setText(f"95th %: {p95:.3f}")
