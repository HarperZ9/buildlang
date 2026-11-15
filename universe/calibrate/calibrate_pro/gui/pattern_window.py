"""
Pattern Window - Fullscreen test pattern display

Provides fullscreen test patterns for calibration:
- Color patches (RGB, CMYK, grayscale)
- Gradient ramps
- Geometry patterns
- SMPTE/EBU color bars
- Resolution/sharpness patterns
"""

from typing import Optional, List, Tuple, Callable
from dataclasses import dataclass
from enum import Enum, auto
import math

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QFrame, QApplication, QGraphicsView, QGraphicsScene,
    QStackedWidget, QToolButton, QSizePolicy
)
from PyQt6.QtCore import (
    Qt, QSize, QRect, QRectF, QTimer, pyqtSignal, QPointF
)
from PyQt6.QtGui import (
    QColor, QPainter, QPen, QBrush, QFont, QScreen, QGuiApplication,
    QLinearGradient, QRadialGradient, QConicalGradient, QPainterPath,
    QPixmap, QImage, QKeySequence, QShortcut
)


# =============================================================================
# Pattern Types
# =============================================================================

class PatternType(Enum):
    """Available test pattern types."""
    SOLID_COLOR = auto()
    GRAYSCALE_RAMP = auto()
    RGB_PRIMARIES = auto()
    SMPTE_BARS = auto()
    EBU_BARS = auto()
    COLORCHECKER = auto()
    GRADIENT_HORIZONTAL = auto()
    GRADIENT_VERTICAL = auto()
    GRID = auto()
    CROSSHATCH = auto()
    ZONE_PLATE = auto()
    RESOLUTION = auto()
    FOCUS = auto()
    GEOMETRY = auto()
    WHITE_POINT = auto()
    BLACK_LEVEL = auto()
    GAMMA_RAMP = auto()
    PLUGE = auto()


@dataclass
class PatternConfig:
    """Configuration for a test pattern."""
    pattern_type: PatternType
    color: Tuple[int, int, int] = (128, 128, 128)  # For solid color
    steps: int = 21  # For ramps
    patch_size: int = 100  # For patches
    show_labels: bool = False
    background: Tuple[int, int, int] = (0, 0, 0)


# =============================================================================
# Pattern Renderer
# =============================================================================

class PatternRenderer:
    """Renders various test patterns."""

    # Standard color values
    SMPTE_COLORS = [
        (192, 192, 192),  # 75% White
        (192, 192, 0),    # 75% Yellow
        (0, 192, 192),    # 75% Cyan
        (0, 192, 0),      # 75% Green
        (192, 0, 192),    # 75% Magenta
        (192, 0, 0),      # 75% Red
        (0, 0, 192),      # 75% Blue
    ]

    EBU_COLORS = [
        (255, 255, 255),  # White
        (255, 255, 0),    # Yellow
        (0, 255, 255),    # Cyan
        (0, 255, 0),      # Green
        (255, 0, 255),    # Magenta
        (255, 0, 0),      # Red
        (0, 0, 255),      # Blue
        (0, 0, 0),        # Black
    ]

    # ColorChecker 24-patch values (sRGB approximations)
    COLORCHECKER = [
        (115, 82, 68),    # Dark skin
        (194, 150, 130),  # Light skin
        (98, 122, 157),   # Blue sky
        (87, 108, 67),    # Foliage
        (133, 128, 177),  # Blue flower
        (103, 189, 170),  # Bluish green
        (214, 126, 44),   # Orange
        (80, 91, 166),    # Purplish blue
        (193, 90, 99),    # Moderate red
        (94, 60, 108),    # Purple
        (157, 188, 64),   # Yellow green
        (224, 163, 46),   # Orange yellow
        (56, 61, 150),    # Blue
        (70, 148, 73),    # Green
        (175, 54, 60),    # Red
        (231, 199, 31),   # Yellow
        (187, 86, 149),   # Magenta
        (8, 133, 161),    # Cyan
        (243, 243, 242),  # White
        (200, 200, 200),  # Neutral 8
        (160, 160, 160),  # Neutral 6.5
        (122, 122, 121),  # Neutral 5
        (85, 85, 85),     # Neutral 3.5
        (52, 52, 52),     # Black
    ]

    @classmethod
    def render(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render a pattern to the painter."""
        method_map = {
            PatternType.SOLID_COLOR: cls._render_solid,
            PatternType.GRAYSCALE_RAMP: cls._render_grayscale_ramp,
            PatternType.RGB_PRIMARIES: cls._render_rgb_primaries,
            PatternType.SMPTE_BARS: cls._render_smpte_bars,
            PatternType.EBU_BARS: cls._render_ebu_bars,
            PatternType.COLORCHECKER: cls._render_colorchecker,
            PatternType.GRADIENT_HORIZONTAL: cls._render_gradient_horizontal,
            PatternType.GRADIENT_VERTICAL: cls._render_gradient_vertical,
            PatternType.GRID: cls._render_grid,
            PatternType.CROSSHATCH: cls._render_crosshatch,
            PatternType.ZONE_PLATE: cls._render_zone_plate,
            PatternType.PLUGE: cls._render_pluge,
            PatternType.GEOMETRY: cls._render_geometry,
            PatternType.WHITE_POINT: cls._render_white_point,
            PatternType.BLACK_LEVEL: cls._render_black_level,
            PatternType.GAMMA_RAMP: cls._render_gamma_ramp,
        }

        method = method_map.get(config.pattern_type, cls._render_solid)
        method(painter, rect, config)

    @classmethod
    def _render_solid(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render solid color pattern."""
        color = QColor(*config.color)
        painter.fillRect(rect, color)

    @classmethod
    def _render_grayscale_ramp(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render grayscale step ramp."""
        steps = config.steps
        step_width = rect.width() // steps

        for i in range(steps):
            gray = int((i / (steps - 1)) * 255) if steps > 1 else 128
            color = QColor(gray, gray, gray)
            x = rect.x() + i * step_width
            w = step_width if i < steps - 1 else rect.width() - i * step_width
            painter.fillRect(x, rect.y(), w, rect.height(), color)

            if config.show_labels:
                painter.setPen(QColor(255, 255, 255) if gray < 128 else QColor(0, 0, 0))
                painter.drawText(x + 5, rect.y() + 20, f"{gray}")

    @classmethod
    def _render_rgb_primaries(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render RGB primary colors with secondaries."""
        colors = [
            (255, 0, 0),    # Red
            (0, 255, 0),    # Green
            (0, 0, 255),    # Blue
            (255, 255, 0),  # Yellow
            (0, 255, 255),  # Cyan
            (255, 0, 255),  # Magenta
            (255, 255, 255),  # White
            (0, 0, 0),      # Black
        ]

        cols = 4
        rows = 2
        w = rect.width() // cols
        h = rect.height() // rows

        for i, (r, g, b) in enumerate(colors):
            row = i // cols
            col = i % cols
            x = rect.x() + col * w
            y = rect.y() + row * h
            painter.fillRect(x, y, w, h, QColor(r, g, b))

    @classmethod
    def _render_smpte_bars(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render SMPTE color bars."""
        # Top 2/3: Main color bars
        bar_width = rect.width() // 7
        bar_height = int(rect.height() * 0.67)

        for i, (r, g, b) in enumerate(cls.SMPTE_COLORS):
            x = rect.x() + i * bar_width
            w = bar_width if i < 6 else rect.width() - 6 * bar_width
            painter.fillRect(x, rect.y(), w, bar_height, QColor(r, g, b))

        # Bottom 1/3: PLUGE and ramp
        bottom_y = rect.y() + bar_height
        bottom_h = rect.height() - bar_height

        # Mini bars section
        mini_colors = [
            (0, 0, 192), (0, 0, 0), (192, 0, 192), (0, 0, 0),
            (0, 192, 192), (0, 0, 0), (192, 192, 192)
        ]
        for i, (r, g, b) in enumerate(mini_colors):
            x = rect.x() + i * bar_width
            w = bar_width if i < 6 else rect.width() - 6 * bar_width
            painter.fillRect(x, bottom_y, w, bottom_h // 2, QColor(r, g, b))

        # PLUGE section
        pluge_y = bottom_y + bottom_h // 2
        pluge_h = bottom_h // 2

        # -4% black, 0% black, +4% black pattern
        pluge_values = [(3, 3, 3), (0, 0, 0), (11, 11, 11), (0, 0, 0)] * 2
        pluge_width = rect.width() // 8
        for i, (r, g, b) in enumerate(pluge_values[:7]):
            x = rect.x() + i * pluge_width
            painter.fillRect(x, pluge_y, pluge_width, pluge_h, QColor(r, g, b))

    @classmethod
    def _render_ebu_bars(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render EBU color bars (100% saturation)."""
        bar_width = rect.width() // 8

        for i, (r, g, b) in enumerate(cls.EBU_COLORS):
            x = rect.x() + i * bar_width
            w = bar_width if i < 7 else rect.width() - 7 * bar_width
            painter.fillRect(x, rect.y(), w, rect.height(), QColor(r, g, b))

    @classmethod
    def _render_colorchecker(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render ColorChecker 24-patch layout."""
        cols = 6
        rows = 4
        padding = 10

        available_width = rect.width() - padding * 2
        available_height = rect.height() - padding * 2

        patch_w = available_width // cols
        patch_h = available_height // rows

        # Background
        painter.fillRect(rect, QColor(80, 80, 80))

        for i, (r, g, b) in enumerate(cls.COLORCHECKER):
            row = i // cols
            col = i % cols
            x = rect.x() + padding + col * patch_w + 2
            y = rect.y() + padding + row * patch_h + 2
            painter.fillRect(x, y, patch_w - 4, patch_h - 4, QColor(r, g, b))

    @classmethod
    def _render_gradient_horizontal(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render horizontal gradient (black to white)."""
        gradient = QLinearGradient(rect.x(), 0, rect.right(), 0)
        gradient.setColorAt(0, QColor(0, 0, 0))
        gradient.setColorAt(1, QColor(255, 255, 255))
        painter.fillRect(rect, gradient)

    @classmethod
    def _render_gradient_vertical(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render vertical gradient (black to white)."""
        gradient = QLinearGradient(0, rect.y(), 0, rect.bottom())
        gradient.setColorAt(0, QColor(0, 0, 0))
        gradient.setColorAt(1, QColor(255, 255, 255))
        painter.fillRect(rect, gradient)

    @classmethod
    def _render_grid(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render grid pattern."""
        # Black background
        painter.fillRect(rect, QColor(0, 0, 0))

        # White grid lines
        painter.setPen(QPen(QColor(255, 255, 255), 1))

        spacing = 50
        for x in range(rect.x(), rect.right(), spacing):
            painter.drawLine(x, rect.y(), x, rect.bottom())
        for y in range(rect.y(), rect.bottom(), spacing):
            painter.drawLine(rect.x(), y, rect.right(), y)

        # Red center cross
        painter.setPen(QPen(QColor(255, 0, 0), 2))
        cx = rect.center().x()
        cy = rect.center().y()
        painter.drawLine(cx, rect.y(), cx, rect.bottom())
        painter.drawLine(rect.x(), cy, rect.right(), cy)

    @classmethod
    def _render_crosshatch(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render crosshatch pattern for geometry check."""
        painter.fillRect(rect, QColor(128, 128, 128))

        painter.setPen(QPen(QColor(255, 255, 255), 1))

        # Diagonal lines
        spacing = 30
        for i in range(-rect.height(), rect.width() + rect.height(), spacing):
            painter.drawLine(rect.x() + i, rect.y(), rect.x() + i + rect.height(), rect.bottom())
            painter.drawLine(rect.x() + i, rect.bottom(), rect.x() + i + rect.height(), rect.y())

    @classmethod
    def _render_zone_plate(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render zone plate pattern for focus/resolution check."""
        cx = rect.width() // 2
        cy = rect.height() // 2
        max_r = min(cx, cy)

        # Create image for pixel-level control
        image = QImage(rect.width(), rect.height(), QImage.Format.Format_RGB32)

        for y in range(rect.height()):
            for x in range(rect.width()):
                dx = x - cx
                dy = y - cy
                r_sq = dx * dx + dy * dy
                # Zone plate formula
                val = int((math.sin(r_sq * 0.005) + 1) * 127.5)
                image.setPixelColor(x, y, QColor(val, val, val))

        painter.drawImage(rect, image)

    @classmethod
    def _render_pluge(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render PLUGE (Picture Line-Up Generation Equipment) pattern."""
        # Background at 7.5% (video black)
        painter.fillRect(rect, QColor(19, 19, 19))

        # Center section with PLUGE bars
        center_w = rect.width() // 3
        center_x = rect.x() + (rect.width() - center_w) // 2

        # -4% (below black)
        painter.fillRect(center_x, rect.y(), center_w // 5, rect.height(), QColor(3, 3, 3))

        # 0% (black)
        painter.fillRect(center_x + center_w // 5, rect.y(), center_w // 5, rect.height(),
                        QColor(0, 0, 0))

        # +4% (just above black)
        painter.fillRect(center_x + 2 * center_w // 5, rect.y(), center_w // 5, rect.height(),
                        QColor(11, 11, 11))

        # 7.5% (background reference)
        painter.fillRect(center_x + 3 * center_w // 5, rect.y(), center_w // 5, rect.height(),
                        QColor(19, 19, 19))

        # 11.5% (above background)
        painter.fillRect(center_x + 4 * center_w // 5, rect.y(), center_w // 5, rect.height(),
                        QColor(29, 29, 29))

        # Labels
        if config.show_labels:
            painter.setPen(QColor(255, 255, 255))
            labels = ["-4%", "0%", "+4%", "7.5%", "11.5%"]
            for i, label in enumerate(labels):
                x = center_x + i * center_w // 5 + 5
                painter.drawText(x, rect.y() + 20, label)

    @classmethod
    def _render_geometry(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render geometry test pattern."""
        painter.fillRect(rect, QColor(0, 0, 0))

        # White border
        painter.setPen(QPen(QColor(255, 255, 255), 2))
        painter.drawRect(rect.x() + 10, rect.y() + 10, rect.width() - 20, rect.height() - 20)

        # Center circles
        cx = rect.center().x()
        cy = rect.center().y()
        painter.setPen(QPen(QColor(255, 255, 255), 1))

        for r in range(50, min(cx, cy), 50):
            painter.drawEllipse(QPointF(cx, cy), r, r)

        # Cross
        painter.drawLine(cx, rect.y(), cx, rect.bottom())
        painter.drawLine(rect.x(), cy, rect.right(), cy)

        # Corner circles
        corner_r = 30
        corners = [
            (rect.x() + corner_r + 10, rect.y() + corner_r + 10),
            (rect.right() - corner_r - 10, rect.y() + corner_r + 10),
            (rect.x() + corner_r + 10, rect.bottom() - corner_r - 10),
            (rect.right() - corner_r - 10, rect.bottom() - corner_r - 10),
        ]
        for (x, y) in corners:
            painter.drawEllipse(QPointF(x, y), corner_r, corner_r)

    @classmethod
    def _render_white_point(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render white point reference pattern."""
        # Gray background
        painter.fillRect(rect, QColor(128, 128, 128))

        # White center patch
        patch_size = min(rect.width(), rect.height()) // 3
        cx = rect.center().x()
        cy = rect.center().y()
        painter.fillRect(cx - patch_size // 2, cy - patch_size // 2,
                        patch_size, patch_size, QColor(*config.color))

    @classmethod
    def _render_black_level(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render black level test pattern."""
        # Full black background
        painter.fillRect(rect, QColor(0, 0, 0))

        # Near-black patches at various levels
        patch_h = rect.height() // 10
        levels = [0, 1, 2, 3, 4, 5, 8, 12, 16, 20]

        for i, level in enumerate(levels):
            y = rect.y() + i * patch_h
            color = QColor(level, level, level)
            painter.fillRect(rect.x(), y, rect.width(), patch_h, color)

            if config.show_labels:
                painter.setPen(QColor(255, 255, 255))
                painter.drawText(rect.x() + 10, y + patch_h - 5, f"Level {level}")

    @classmethod
    def _render_gamma_ramp(cls, painter: QPainter, rect: QRect, config: PatternConfig):
        """Render gamma verification ramp."""
        # Top half: Solid gray patches
        # Bottom half: Dithered pattern that should match at correct gamma

        steps = 11
        step_width = rect.width() // steps
        half_height = rect.height() // 2

        for i in range(steps):
            gray = int((i / (steps - 1)) * 255)
            x = rect.x() + i * step_width
            w = step_width if i < steps - 1 else rect.width() - (steps - 1) * step_width

            # Top: solid gray
            painter.fillRect(x, rect.y(), w, half_height, QColor(gray, gray, gray))

            # Bottom: dithered pattern (checkerboard of black and white)
            # that should appear as the same gray at correct gamma
            target_luminance = gray / 255.0
            # At gamma 2.2, dither pattern of 0 and 1 appears as ~0.5^(1/2.2) = 0.73
            # For i-th step, we need black and a computed white

            for py in range(rect.y() + half_height, rect.bottom(), 2):
                for px in range(x, x + w, 2):
                    if (px + py) % 4 < 2:
                        painter.fillRect(px, py, 2, 2, QColor(0, 0, 0))
                    else:
                        painter.fillRect(px, py, 2, 2, QColor(255, 255, 255))


# =============================================================================
# Pattern Window Widget
# =============================================================================

class PatternCanvas(QWidget):
    """Canvas widget that displays test patterns."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.config = PatternConfig(PatternType.SOLID_COLOR)
        self.setMouseTracking(True)

    def set_pattern(self, config: PatternConfig):
        """Set the pattern to display."""
        self.config = config
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        PatternRenderer.render(painter, self.rect(), self.config)


class PatternWindow(QWidget):
    """Fullscreen window for displaying test patterns."""

    pattern_changed = pyqtSignal(PatternConfig)
    closed = pyqtSignal()

    def __init__(self, screen: Optional[QScreen] = None, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.target_screen = screen
        self._show_controls = True
        self._current_config = PatternConfig(PatternType.SOLID_COLOR)
        self._setup_ui()
        self._setup_shortcuts()

    def _setup_ui(self):
        self.setWindowTitle("Test Patterns")
        self.setStyleSheet("background-color: black;")

        # Main layout
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Pattern canvas
        self.canvas = PatternCanvas()
        layout.addWidget(self.canvas, 1)

        # Control bar (toggleable)
        self.control_bar = QFrame()
        self.control_bar.setStyleSheet("""
            QFrame {
                background-color: rgba(30, 30, 30, 200);
                border-top: 1px solid #404040;
            }
            QPushButton {
                background-color: #383838;
                border: 1px solid #505050;
                border-radius: 4px;
                padding: 8px 16px;
                color: #e0e0e0;
                min-width: 80px;
            }
            QPushButton:hover {
                background-color: #4a9eff;
                border-color: #4a9eff;
            }
            QPushButton:pressed {
                background-color: #3a8eef;
            }
            QLabel {
                color: #e0e0e0;
            }
        """)
        control_layout = QHBoxLayout(self.control_bar)
        control_layout.setContentsMargins(16, 8, 16, 8)

        # Pattern type buttons
        pattern_buttons = [
            ("Solid", PatternType.SOLID_COLOR),
            ("Grayscale", PatternType.GRAYSCALE_RAMP),
            ("RGB", PatternType.RGB_PRIMARIES),
            ("SMPTE", PatternType.SMPTE_BARS),
            ("EBU", PatternType.EBU_BARS),
            ("ColorChecker", PatternType.COLORCHECKER),
            ("PLUGE", PatternType.PLUGE),
            ("Gradient", PatternType.GRADIENT_HORIZONTAL),
            ("Grid", PatternType.GRID),
            ("Geometry", PatternType.GEOMETRY),
        ]

        for name, pattern_type in pattern_buttons:
            btn = QPushButton(name)
            btn.clicked.connect(lambda checked, pt=pattern_type: self._set_pattern_type(pt))
            control_layout.addWidget(btn)

        control_layout.addStretch()

        # Info label
        self.info_label = QLabel("Press H to toggle controls, ESC to exit")
        self.info_label.setStyleSheet("color: #808080;")
        control_layout.addWidget(self.info_label)

        # Close button
        close_btn = QPushButton("Close (ESC)")
        close_btn.clicked.connect(self.close)
        control_layout.addWidget(close_btn)

        layout.addWidget(self.control_bar)

    def _setup_shortcuts(self):
        """Setup keyboard shortcuts."""
        # ESC to close
        QShortcut(QKeySequence(Qt.Key.Key_Escape), self, self.close)

        # H to toggle controls
        QShortcut(QKeySequence(Qt.Key.Key_H), self, self._toggle_controls)

        # F for fullscreen toggle
        QShortcut(QKeySequence(Qt.Key.Key_F), self, self._toggle_fullscreen)

        # Number keys for quick patterns
        patterns = [
            PatternType.SOLID_COLOR,
            PatternType.GRAYSCALE_RAMP,
            PatternType.RGB_PRIMARIES,
            PatternType.SMPTE_BARS,
            PatternType.EBU_BARS,
            PatternType.COLORCHECKER,
            PatternType.PLUGE,
            PatternType.GRADIENT_HORIZONTAL,
            PatternType.GRID,
        ]
        for i, pattern in enumerate(patterns):
            key = getattr(Qt.Key, f"Key_{i + 1}")
            QShortcut(QKeySequence(key), self, lambda p=pattern: self._set_pattern_type(p))

        # Arrow keys for grayscale level
        QShortcut(QKeySequence(Qt.Key.Key_Left), self, lambda: self._adjust_color(-10))
        QShortcut(QKeySequence(Qt.Key.Key_Right), self, lambda: self._adjust_color(10))

        # RGB keys
        QShortcut(QKeySequence(Qt.Key.Key_R), self, lambda: self._set_solid_color(255, 0, 0))
        QShortcut(QKeySequence(Qt.Key.Key_G), self, lambda: self._set_solid_color(0, 255, 0))
        QShortcut(QKeySequence(Qt.Key.Key_B), self, lambda: self._set_solid_color(0, 0, 255))
        QShortcut(QKeySequence(Qt.Key.Key_W), self, lambda: self._set_solid_color(255, 255, 255))
        QShortcut(QKeySequence(Qt.Key.Key_K), self, lambda: self._set_solid_color(0, 0, 0))

    def _toggle_controls(self):
        """Toggle control bar visibility."""
        self._show_controls = not self._show_controls
        self.control_bar.setVisible(self._show_controls)

    def _toggle_fullscreen(self):
        """Toggle fullscreen mode."""
        if self.isFullScreen():
            self.showNormal()
        else:
            self.show_fullscreen()

    def _set_pattern_type(self, pattern_type: PatternType):
        """Set the current pattern type."""
        self._current_config.pattern_type = pattern_type
        self.canvas.set_pattern(self._current_config)
        self.pattern_changed.emit(self._current_config)

    def _set_solid_color(self, r: int, g: int, b: int):
        """Set solid color."""
        self._current_config.pattern_type = PatternType.SOLID_COLOR
        self._current_config.color = (r, g, b)
        self.canvas.set_pattern(self._current_config)
        self.pattern_changed.emit(self._current_config)

    def _adjust_color(self, delta: int):
        """Adjust solid color by delta."""
        if self._current_config.pattern_type == PatternType.SOLID_COLOR:
            r, g, b = self._current_config.color
            new_val = max(0, min(255, r + delta))
            self._current_config.color = (new_val, new_val, new_val)
            self.canvas.set_pattern(self._current_config)
            self.info_label.setText(f"Gray level: {new_val}")

    def show_fullscreen(self, screen: Optional[QScreen] = None):
        """Show window fullscreen on specified screen."""
        target = screen or self.target_screen or QGuiApplication.primaryScreen()

        if target:
            self.setGeometry(target.geometry())
            self.showFullScreen()
        else:
            self.showFullScreen()

    def show_pattern(self, config: PatternConfig):
        """Show specific pattern configuration."""
        self._current_config = config
        self.canvas.set_pattern(config)

    def closeEvent(self, event):
        self.closed.emit()
        super().closeEvent(event)


# =============================================================================
# Pattern Sequence Player
# =============================================================================

class PatternSequencer:
    """Plays sequences of patterns for automated calibration."""

    def __init__(self, window: PatternWindow):
        self.window = window
        self.sequence: List[PatternConfig] = []
        self.current_index = 0
        self.timer = QTimer()
        self.timer.timeout.connect(self._next_pattern)
        self.on_pattern_shown: Optional[Callable[[int, PatternConfig], None]] = None
        self.on_sequence_complete: Optional[Callable[[], None]] = None

    def set_sequence(self, patterns: List[PatternConfig]):
        """Set the pattern sequence."""
        self.sequence = patterns
        self.current_index = 0

    def start(self, interval_ms: int = 1000):
        """Start playing the sequence."""
        if self.sequence:
            self.current_index = 0
            self._show_current()
            if len(self.sequence) > 1:
                self.timer.start(interval_ms)

    def stop(self):
        """Stop the sequence."""
        self.timer.stop()

    def next(self):
        """Advance to next pattern manually."""
        self._next_pattern()

    def _show_current(self):
        """Show current pattern."""
        if 0 <= self.current_index < len(self.sequence):
            config = self.sequence[self.current_index]
            self.window.show_pattern(config)
            if self.on_pattern_shown:
                self.on_pattern_shown(self.current_index, config)

    def _next_pattern(self):
        """Advance to next pattern."""
        self.current_index += 1
        if self.current_index >= len(self.sequence):
            self.timer.stop()
            if self.on_sequence_complete:
                self.on_sequence_complete()
        else:
            self._show_current()

    @staticmethod
    def create_grayscale_sequence(steps: int = 21) -> List[PatternConfig]:
        """Create a grayscale measurement sequence."""
        sequence = []
        for i in range(steps):
            gray = int((i / (steps - 1)) * 255) if steps > 1 else 128
            config = PatternConfig(
                pattern_type=PatternType.SOLID_COLOR,
                color=(gray, gray, gray)
            )
            sequence.append(config)
        return sequence

    @staticmethod
    def create_colorchecker_sequence() -> List[PatternConfig]:
        """Create ColorChecker measurement sequence."""
        sequence = []
        for r, g, b in PatternRenderer.COLORCHECKER:
            config = PatternConfig(
                pattern_type=PatternType.SOLID_COLOR,
                color=(r, g, b)
            )
            sequence.append(config)
        return sequence

    @staticmethod
    def create_primary_sequence() -> List[PatternConfig]:
        """Create primary/secondary color sequence."""
        colors = [
            (255, 255, 255),  # White
            (255, 0, 0),      # Red
            (0, 255, 0),      # Green
            (0, 0, 255),      # Blue
            (255, 255, 0),    # Yellow
            (0, 255, 255),    # Cyan
            (255, 0, 255),    # Magenta
            (0, 0, 0),        # Black
        ]
        return [PatternConfig(PatternType.SOLID_COLOR, color=c) for c in colors]
