"""
Display Selector - Multi-monitor selection widget

Provides visual display selection with monitor preview,
EDID information, and calibration status indicators.
"""

from typing import Optional, List, Dict, Any, Tuple
from dataclasses import dataclass, field
from enum import Enum, auto

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QFrame,
    QPushButton, QGridLayout, QScrollArea, QSizePolicy,
    QGraphicsView, QGraphicsScene, QGraphicsRectItem,
    QGraphicsTextItem, QToolTip, QMenu
)
from PyQt6.QtCore import (
    Qt, QRectF, QPointF, QSize, pyqtSignal, QTimer
)
from PyQt6.QtGui import (
    QColor, QPainter, QPen, QBrush, QFont, QCursor,
    QGuiApplication, QScreen, QPainterPath
)


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
# Display Information
# =============================================================================

class DisplayTechnology(Enum):
    """Display panel technology."""
    UNKNOWN = "Unknown"
    LCD_TN = "LCD (TN)"
    LCD_IPS = "LCD (IPS)"
    LCD_VA = "LCD (VA)"
    OLED_WOLED = "OLED (WOLED)"
    OLED_QDOLED = "OLED (QD-OLED)"
    MINI_LED = "Mini-LED"
    MICRO_LED = "Micro-LED"


class CalibrationStatus(Enum):
    """Display calibration status."""
    UNCALIBRATED = auto()
    CALIBRATED = auto()
    NEEDS_RECALIBRATION = auto()
    CALIBRATING = auto()


@dataclass
class DisplayInfo:
    """Complete display information."""
    # Identification
    id: int = 0
    name: str = ""
    manufacturer: str = ""
    model: str = ""
    serial: str = ""

    # Physical
    size_inches: float = 0.0
    aspect_ratio: str = ""
    technology: DisplayTechnology = DisplayTechnology.UNKNOWN

    # Resolution and refresh
    width: int = 0
    height: int = 0
    refresh_rate: float = 0.0
    bit_depth: int = 8

    # Position (for multi-monitor)
    x: int = 0
    y: int = 0
    is_primary: bool = False

    # HDR
    hdr_supported: bool = False
    hdr_enabled: bool = False
    max_luminance: float = 0.0
    min_luminance: float = 0.0

    # Calibration
    calibration_status: CalibrationStatus = CalibrationStatus.UNCALIBRATED
    current_profile: str = ""
    last_calibrated: str = ""
    delta_e: float = 0.0

    @property
    def resolution(self) -> str:
        return f"{self.width}x{self.height}"

    @property
    def display_name(self) -> str:
        if self.manufacturer and self.model:
            return f"{self.manufacturer} {self.model}"
        return self.name or f"Display {self.id + 1}"


# =============================================================================
# Display Monitor Widget
# =============================================================================

class DisplayMonitorWidget(QFrame):
    """Visual representation of a single display."""

    clicked = pyqtSignal(int)  # Emits display ID
    double_clicked = pyqtSignal(int)

    def __init__(self, display_info: DisplayInfo, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.display_info = display_info
        self._selected = False
        self._hovered = False
        self._setup_ui()

    def _setup_ui(self):
        self.setMinimumSize(180, 120)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._update_style()

        layout = QVBoxLayout(self)
        layout.setContentsMargins(12, 12, 12, 12)

        # Display number badge
        badge_layout = QHBoxLayout()

        self.number_badge = QLabel(str(self.display_info.id + 1))
        self.number_badge.setFixedSize(24, 24)
        self.number_badge.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.number_badge.setStyleSheet(f"""
            background-color: {COLORS['accent']};
            color: white;
            font-weight: 600;
            border-radius: 12px;
        """)
        badge_layout.addWidget(self.number_badge)

        # Primary indicator
        if self.display_info.is_primary:
            primary_label = QLabel("Primary")
            primary_label.setStyleSheet(f"""
                color: {COLORS['success']};
                font-size: 10px;
                font-weight: 600;
            """)
            badge_layout.addWidget(primary_label)

        badge_layout.addStretch()

        # Status indicator
        self.status_indicator = QLabel()
        self._update_status_indicator()
        badge_layout.addWidget(self.status_indicator)

        layout.addLayout(badge_layout)

        # Display name
        self.name_label = QLabel(self.display_info.display_name)
        self.name_label.setStyleSheet(f"""
            font-weight: 600;
            font-size: 13px;
            color: {COLORS['text_primary']};
        """)
        self.name_label.setWordWrap(True)
        layout.addWidget(self.name_label)

        # Resolution
        self.resolution_label = QLabel(
            f"{self.display_info.resolution} @ {self.display_info.refresh_rate:.0f}Hz"
        )
        self.resolution_label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 11px;")
        layout.addWidget(self.resolution_label)

        # Technology and bit depth
        tech_text = f"{self.display_info.technology.value}"
        if self.display_info.bit_depth > 8:
            tech_text += f" • {self.display_info.bit_depth}-bit"
        if self.display_info.hdr_enabled:
            tech_text += " • HDR"

        self.tech_label = QLabel(tech_text)
        self.tech_label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 10px;")
        layout.addWidget(self.tech_label)

        layout.addStretch()

    def _update_status_indicator(self):
        """Update the calibration status indicator."""
        status = self.display_info.calibration_status
        if status == CalibrationStatus.CALIBRATED:
            text = "✓"
            color = COLORS['success']
            tooltip = f"Calibrated (ΔE: {self.display_info.delta_e:.2f})"
        elif status == CalibrationStatus.NEEDS_RECALIBRATION:
            text = "!"
            color = COLORS['warning']
            tooltip = "Needs recalibration"
        elif status == CalibrationStatus.CALIBRATING:
            text = "◐"
            color = COLORS['accent']
            tooltip = "Calibrating..."
        else:
            text = "○"
            color = COLORS['text_disabled']
            tooltip = "Not calibrated"

        self.status_indicator.setText(text)
        self.status_indicator.setStyleSheet(f"color: {color}; font-size: 16px;")
        self.status_indicator.setToolTip(tooltip)

    def _update_style(self):
        """Update widget style based on state."""
        if self._selected:
            border_color = COLORS['accent']
            bg_color = COLORS['surface_alt']
        elif self._hovered:
            border_color = COLORS['accent']
            bg_color = COLORS['surface']
        else:
            border_color = COLORS['border']
            bg_color = COLORS['surface']

        self.setStyleSheet(f"""
            DisplayMonitorWidget {{
                background-color: {bg_color};
                border: 2px solid {border_color};
                border-radius: 12px;
            }}
        """)

    @property
    def selected(self) -> bool:
        return self._selected

    @selected.setter
    def selected(self, value: bool):
        self._selected = value
        self._update_style()

    def enterEvent(self, event):
        self._hovered = True
        self._update_style()
        super().enterEvent(event)

    def leaveEvent(self, event):
        self._hovered = False
        self._update_style()
        super().leaveEvent(event)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.clicked.emit(self.display_info.id)
        super().mousePressEvent(event)

    def mouseDoubleClickEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.double_clicked.emit(self.display_info.id)
        super().mouseDoubleClickEvent(event)


# =============================================================================
# Visual Layout Preview
# =============================================================================

class DisplayLayoutPreview(QGraphicsView):
    """Visual preview of display arrangement."""

    display_selected = pyqtSignal(int)

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()
        self.displays: List[DisplayInfo] = []
        self.display_rects: Dict[int, QGraphicsRectItem] = {}
        self._selected_id = -1

    def _setup_ui(self):
        self.scene = QGraphicsScene(self)
        self.setScene(self.scene)

        self.setRenderHint(QPainter.RenderHint.Antialiasing)
        self.setViewportUpdateMode(QGraphicsView.ViewportUpdateMode.FullViewportUpdate)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setStyleSheet(f"""
            QGraphicsView {{
                background-color: {COLORS['background']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
            }}
        """)

        self.setMinimumHeight(150)

    def set_displays(self, displays: List[DisplayInfo]):
        """Set the displays to visualize."""
        self.displays = displays
        self._rebuild_scene()

    def _rebuild_scene(self):
        """Rebuild the scene with current displays."""
        self.scene.clear()
        self.display_rects.clear()

        if not self.displays:
            return

        # Find bounds
        min_x = min(d.x for d in self.displays)
        min_y = min(d.y for d in self.displays)
        max_x = max(d.x + d.width for d in self.displays)
        max_y = max(d.y + d.height for d in self.displays)

        total_width = max_x - min_x
        total_height = max_y - min_y

        # Calculate scale to fit
        view_width = self.width() - 40
        view_height = self.height() - 40
        scale = min(view_width / total_width, view_height / total_height) if total_width > 0 else 1

        # Draw displays
        for display in self.displays:
            x = (display.x - min_x) * scale + 20
            y = (display.y - min_y) * scale + 20
            w = display.width * scale
            h = display.height * scale

            # Create rectangle
            rect = QGraphicsRectItem(x, y, w, h)
            rect.setPen(QPen(QColor(COLORS['border']), 2))
            rect.setBrush(QBrush(QColor(COLORS['surface'])))
            rect.setData(0, display.id)
            self.scene.addItem(rect)
            self.display_rects[display.id] = rect

            # Add label
            label = QGraphicsTextItem(str(display.id + 1))
            label.setDefaultTextColor(QColor(COLORS['text_primary']))
            font = QFont("Segoe UI", 16, QFont.Weight.Bold)
            label.setFont(font)
            label.setPos(x + w/2 - 8, y + h/2 - 12)
            self.scene.addItem(label)

            # Add resolution text
            res_label = QGraphicsTextItem(display.resolution)
            res_label.setDefaultTextColor(QColor(COLORS['text_secondary']))
            res_font = QFont("Segoe UI", 8)
            res_label.setFont(res_font)
            res_label.setPos(x + 4, y + h - 16)
            self.scene.addItem(res_label)

        # Update scene rect
        self.scene.setSceneRect(0, 0, view_width + 40, view_height + 40)

    def select_display(self, display_id: int):
        """Select a display by ID."""
        self._selected_id = display_id
        for did, rect in self.display_rects.items():
            if did == display_id:
                rect.setPen(QPen(QColor(COLORS['accent']), 3))
                rect.setBrush(QBrush(QColor(COLORS['surface_alt'])))
            else:
                rect.setPen(QPen(QColor(COLORS['border']), 2))
                rect.setBrush(QBrush(QColor(COLORS['surface'])))

    def mousePressEvent(self, event):
        pos = self.mapToScene(event.pos())
        items = self.scene.items(pos)

        for item in items:
            if isinstance(item, QGraphicsRectItem):
                display_id = item.data(0)
                if display_id is not None:
                    self.select_display(display_id)
                    self.display_selected.emit(display_id)
                    break

        super().mousePressEvent(event)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._rebuild_scene()
        if self._selected_id >= 0:
            self.select_display(self._selected_id)


# =============================================================================
# Display Selector Widget
# =============================================================================

class DisplaySelector(QWidget):
    """Complete display selector with list and visual preview."""

    display_selected = pyqtSignal(DisplayInfo)
    display_double_clicked = pyqtSignal(DisplayInfo)

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.displays: List[DisplayInfo] = []
        self.display_widgets: Dict[int, DisplayMonitorWidget] = {}
        self._selected_id = -1
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(16)

        # Visual layout preview
        self.layout_preview = DisplayLayoutPreview()
        self.layout_preview.display_selected.connect(self._on_preview_selection)
        layout.addWidget(self.layout_preview)

        # Display cards scroll area
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setStyleSheet(f"""
            QScrollArea {{
                border: none;
                background-color: transparent;
            }}
        """)

        self.cards_container = QWidget()
        self.cards_layout = QHBoxLayout(self.cards_container)
        self.cards_layout.setContentsMargins(0, 0, 0, 0)
        self.cards_layout.setSpacing(12)
        self.cards_layout.addStretch()

        scroll.setWidget(self.cards_container)
        layout.addWidget(scroll)

        # Refresh button
        button_layout = QHBoxLayout()
        button_layout.addStretch()

        refresh_btn = QPushButton("Refresh Displays")
        refresh_btn.clicked.connect(self.refresh_displays)
        button_layout.addWidget(refresh_btn)

        layout.addLayout(button_layout)

        # Initial refresh
        QTimer.singleShot(100, self.refresh_displays)

    def refresh_displays(self):
        """Detect and refresh the display list."""
        # Clear existing widgets
        for widget in self.display_widgets.values():
            widget.deleteLater()
        self.display_widgets.clear()
        self.displays.clear()

        # Get displays from Qt
        screens = QGuiApplication.screens()

        for i, screen in enumerate(screens):
            geometry = screen.geometry()

            # Create display info
            display = DisplayInfo(
                id=i,
                name=screen.name(),
                manufacturer=screen.manufacturer() or "Unknown",
                model=screen.model() or screen.name(),
                width=geometry.width(),
                height=geometry.height(),
                x=geometry.x(),
                y=geometry.y(),
                refresh_rate=screen.refreshRate(),
                bit_depth=screen.depth(),
                is_primary=(screen == QGuiApplication.primaryScreen()),
            )

            # Try to determine aspect ratio
            from math import gcd
            g = gcd(display.width, display.height)
            display.aspect_ratio = f"{display.width // g}:{display.height // g}"

            self.displays.append(display)

            # Create widget
            widget = DisplayMonitorWidget(display)
            widget.clicked.connect(self._on_display_clicked)
            widget.double_clicked.connect(self._on_display_double_clicked)
            self.display_widgets[i] = widget

            # Insert before stretch
            self.cards_layout.insertWidget(self.cards_layout.count() - 1, widget)

        # Update preview
        self.layout_preview.set_displays(self.displays)

        # Select first display by default
        if self.displays:
            self._select_display(0)

    def _on_display_clicked(self, display_id: int):
        """Handle display card click."""
        self._select_display(display_id)

    def _on_display_double_clicked(self, display_id: int):
        """Handle display card double-click."""
        for display in self.displays:
            if display.id == display_id:
                self.display_double_clicked.emit(display)
                break

    def _on_preview_selection(self, display_id: int):
        """Handle selection from preview."""
        self._select_display(display_id)

    def _select_display(self, display_id: int):
        """Select a display."""
        self._selected_id = display_id

        # Update widgets
        for did, widget in self.display_widgets.items():
            widget.selected = (did == display_id)

        # Update preview
        self.layout_preview.select_display(display_id)

        # Emit signal
        for display in self.displays:
            if display.id == display_id:
                self.display_selected.emit(display)
                break

    @property
    def selected_display(self) -> Optional[DisplayInfo]:
        """Get currently selected display."""
        for display in self.displays:
            if display.id == self._selected_id:
                return display
        return None

    def select_by_id(self, display_id: int):
        """Programmatically select a display by ID."""
        if display_id in self.display_widgets:
            self._select_display(display_id)


# =============================================================================
# Quick Display Info Panel
# =============================================================================

class DisplayInfoPanel(QWidget):
    """Detailed information panel for selected display."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(16, 16, 16, 16)

        # Header
        self.name_label = QLabel("No display selected")
        self.name_label.setStyleSheet(f"font-size: 16px; font-weight: 600;")
        layout.addWidget(self.name_label)

        # Info grid
        self.info_grid = QGridLayout()
        self.info_grid.setSpacing(8)

        self.info_labels = {}
        info_items = [
            ("Resolution:", "resolution"),
            ("Refresh Rate:", "refresh"),
            ("Bit Depth:", "bit_depth"),
            ("Technology:", "technology"),
            ("Aspect Ratio:", "aspect"),
            ("HDR:", "hdr"),
            ("Profile:", "profile"),
            ("Calibration:", "calibration"),
            ("Delta E:", "delta_e"),
        ]

        for row, (label_text, key) in enumerate(info_items):
            label = QLabel(label_text)
            label.setStyleSheet(f"color: {COLORS['text_secondary']};")
            value = QLabel("--")
            self.info_labels[key] = value
            self.info_grid.addWidget(label, row, 0)
            self.info_grid.addWidget(value, row, 1)

        layout.addLayout(self.info_grid)
        layout.addStretch()

    def set_display(self, display: Optional[DisplayInfo]):
        """Update panel with display information."""
        if display is None:
            self.name_label.setText("No display selected")
            for label in self.info_labels.values():
                label.setText("--")
            return

        self.name_label.setText(display.display_name)

        self.info_labels["resolution"].setText(display.resolution)
        self.info_labels["refresh"].setText(f"{display.refresh_rate:.0f} Hz")
        self.info_labels["bit_depth"].setText(f"{display.bit_depth}-bit")
        self.info_labels["technology"].setText(display.technology.value)
        self.info_labels["aspect"].setText(display.aspect_ratio)

        if display.hdr_supported:
            hdr_text = "Enabled" if display.hdr_enabled else "Supported"
            self.info_labels["hdr"].setText(hdr_text)
            self.info_labels["hdr"].setStyleSheet(f"color: {COLORS['success']};")
        else:
            self.info_labels["hdr"].setText("Not supported")
            self.info_labels["hdr"].setStyleSheet("")

        self.info_labels["profile"].setText(display.current_profile or "None")

        status = display.calibration_status
        if status == CalibrationStatus.CALIBRATED:
            self.info_labels["calibration"].setText("Calibrated")
            self.info_labels["calibration"].setStyleSheet(f"color: {COLORS['success']};")
            self.info_labels["delta_e"].setText(f"{display.delta_e:.2f}")
        elif status == CalibrationStatus.NEEDS_RECALIBRATION:
            self.info_labels["calibration"].setText("Needs recalibration")
            self.info_labels["calibration"].setStyleSheet(f"color: {COLORS['warning']};")
            self.info_labels["delta_e"].setText("--")
        else:
            self.info_labels["calibration"].setText("Not calibrated")
            self.info_labels["calibration"].setStyleSheet("")
            self.info_labels["delta_e"].setText("--")
