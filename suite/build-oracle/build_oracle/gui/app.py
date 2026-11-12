"""
Build Oracle -- Main Application

Professional time series forecasting workbench with sidebar navigation,
page transitions, and the shared Calibrate Pro visual framework.
"""

import logging
import sys
from pathlib import Path

logger = logging.getLogger(__name__)

from build_ui.theme import STYLE, C
from build_ui.widgets import Heading, Sidebar, ToastNotification
from PyQt6.QtCore import (
    QEasingCurve,
    QPointF,
    QPropertyAnimation,
    QSettings,
    Qt,
)
from PyQt6.QtGui import (
    QAction,
    QColor,
    QIcon,
    QKeySequence,
    QPainter,
    QPen,
    QPixmap,
    QShortcut,
)
from PyQt6.QtWidgets import (
    QFileDialog,
    QGraphicsOpacityEffect,
    QHBoxLayout,
    QLabel,
    QMainWindow,
    QMessageBox,
    QStackedWidget,
    QVBoxLayout,
    QWidget,
)

APP_NAME = "Build Oracle"
APP_VERSION = "1.0.0"
APP_ORG = "Build Universe"


# =============================================================================
# Application Icon -- crystal ball with prediction line
# =============================================================================


def make_app_icon() -> QIcon:
    """
    Create the application icon programmatically.

    A stylized crystal ball with a rising prediction line rendered
    at multiple sizes for crisp display at any DPI.
    """
    icon = QIcon()
    for size in [16, 24, 32, 48, 64, 128, 256]:
        pm = QPixmap(size, size)
        pm.fill(QColor(0, 0, 0, 0))

        p = QPainter(pm)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        s = size
        cx = s * 0.5
        cy = s * 0.5

        # Crystal ball -- outer circle
        radius = s * 0.36
        ring_width = max(2.0, s * 0.06)

        # Soft gradient ring
        pen = QPen(QColor("#d4a0a0"), ring_width)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        p.setPen(pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawEllipse(QPointF(cx, cy), radius, radius)

        # Inner fill -- soft cream
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor("#fdf9f5"))
        inner_r = radius - ring_width * 0.5
        p.drawEllipse(QPointF(cx, cy), inner_r, inner_r)

        # Prediction line inside the ball -- rising trend
        line_pen = QPen(QColor("#92ad7e"), max(1.5, s * 0.04))
        line_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        line_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setPen(line_pen)

        # Wavy rising line
        import math

        points = []
        num_pts = 8
        for i in range(num_pts):
            t = i / (num_pts - 1)
            px = cx - inner_r * 0.6 + t * inner_r * 1.2
            py = cy + inner_r * 0.3 - t * inner_r * 0.5 + math.sin(t * math.pi * 2) * inner_r * 0.12
            points.append(QPointF(px, py))

        for i in range(len(points) - 1):
            p.drawLine(points[i], points[i + 1])

        # Dashed forecast extension
        dash_pen = QPen(QColor("#e0c87a"), max(1.0, s * 0.03))
        dash_pen.setStyle(Qt.PenStyle.DashLine)
        dash_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        p.setPen(dash_pen)
        last = points[-1]
        forecast_end = QPointF(cx + inner_r * 0.7, cy - inner_r * 0.4)
        p.drawLine(last, forecast_end)

        # Small dot at forecast tip
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor("#e0c87a"))
        dot_r = max(1.5, s * 0.035)
        p.drawEllipse(forecast_end, dot_r, dot_r)

        # Base of crystal ball
        base_pen = QPen(QColor("#bfb0a4"), max(1.5, s * 0.04))
        p.setPen(base_pen)
        base_y = cy + radius + ring_width * 0.3
        p.drawLine(
            QPointF(cx - radius * 0.5, base_y),
            QPointF(cx + radius * 0.5, base_y),
        )

        p.end()
        icon.addPixmap(pm)

    return icon


# =============================================================================
# Placeholder Page (fallback for unbuilt pages)
# =============================================================================


class PlaceholderPage(QWidget):
    def __init__(self, title: str, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.addWidget(Heading(title))

        desc = QLabel("This page is under construction.")
        desc.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(desc)

        layout.addStretch()


# =============================================================================
# Main Window
# =============================================================================

PAGE_NAMES = [
    "Dashboard",
    "Forecast",
    "Decompose",
    "Changepoints",
    "Settings",
]

PAGE_SHORTCUTS = ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4", "Ctrl+5"]

PAGE_MENU_NAMES = [
    "&Dashboard",
    "&Forecast",
    "D&ecompose",
    "&Changepoints",
    "&Settings",
]


class BuildOracleWindow(QMainWindow):
    """Main application window."""

    def __init__(self):
        super().__init__()
        self.settings = QSettings(APP_ORG, APP_NAME)
        self.setWindowTitle(f"{APP_NAME} v{APP_VERSION}")
        self.setMinimumSize(1000, 700)
        self.resize(1200, 800)
        self.setStyleSheet(STYLE)
        self._app_icon = make_app_icon()
        self.setWindowIcon(self._app_icon)

        self._build_menubar()
        self._build_central()
        self._build_statusbar()
        self._setup_shortcuts()
        self._restore_geometry()

    # --- Keyboard Shortcuts ---

    def _setup_shortcuts(self):
        """Register keyboard shortcuts not already attached to menu actions."""
        sc_escape = QShortcut(QKeySequence("Escape"), self)
        sc_escape.activated.connect(self.close)

    def _shortcut_switch_page(self, index: int):
        """Switch to a page by index and update sidebar."""
        self._switch_page(index)
        self.sidebar._on_click(index)

    # --- Menu Bar ---

    def _build_menubar(self):
        mb = self.menuBar()

        # File
        file_menu = mb.addMenu("&File")
        file_menu.addAction(QAction("&Load CSV...", self, shortcut="Ctrl+O", triggered=self._load_csv))
        file_menu.addSeparator()
        file_menu.addAction(QAction("E&xit", self, shortcut="Alt+F4", triggered=self.close))

        # View -- page navigation shortcuts
        view = mb.addMenu("&View")
        for i, (name, sc) in enumerate(zip(PAGE_MENU_NAMES, PAGE_SHORTCUTS)):
            act = QAction(name, self)
            act.setShortcut(QKeySequence(sc))
            act.triggered.connect(lambda checked, idx=i: self._shortcut_switch_page(idx))
            view.addAction(act)
        view.addSeparator()
        view.addAction(QAction("&Refresh", self, shortcut="F5", triggered=self._refresh_current))

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
        self.sidebar = Sidebar(PAGE_NAMES, app_name=APP_NAME, app_version=APP_VERSION)
        self.sidebar.page_changed.connect(self._switch_page)
        main_layout.addWidget(self.sidebar)

        # Page stack
        self.stack = QStackedWidget()
        self.stack.setStyleSheet(f"background: {C.BG};")

        # Page 0: Dashboard
        try:
            from build_oracle.gui.pages.dashboard import DashboardPage

            self.stack.addWidget(DashboardPage(main_window=self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load DashboardPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Dashboard"))

        # Page 1: Forecast
        try:
            from build_oracle.gui.pages.forecast_page import ForecastPage

            self.stack.addWidget(ForecastPage(main_window=self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load ForecastPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Forecast"))

        # Page 2: Decompose
        try:
            from build_oracle.gui.pages.decompose_page import DecomposePage

            self.stack.addWidget(DecomposePage(main_window=self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load DecomposePage: %s", e)
            self.stack.addWidget(PlaceholderPage("Decompose"))

        # Page 3: Changepoints
        try:
            from build_oracle.gui.pages.changepoint_page import ChangepointPage

            self.stack.addWidget(ChangepointPage(main_window=self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load ChangepointPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Changepoints"))

        # Page 4: Settings
        try:
            from build_oracle.gui.pages.settings_page import SettingsPage

            self.stack.addWidget(SettingsPage())
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load SettingsPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Settings"))

        main_layout.addWidget(self.stack, stretch=1)
        self.setCentralWidget(central)

    # --- Status Bar ---

    def _build_statusbar(self):
        sb = self.statusBar()
        self._status = QLabel("Ready")
        sb.addWidget(self._status, 1)

    # --- Page Switching ---

    def _switch_page(self, index: int):
        """Switch page with a subtle opacity fade transition."""
        if index == self.stack.currentIndex():
            return
        target = self.stack.widget(index)
        if target:
            try:
                effect = QGraphicsOpacityEffect(target)
                target.setGraphicsEffect(effect)
                effect.setOpacity(0.3)
                self.stack.setCurrentIndex(index)

                anim = QPropertyAnimation(effect, b"opacity")
                anim.setDuration(150)
                anim.setStartValue(0.3)
                anim.setEndValue(1.0)
                anim.setEasingCurve(QEasingCurve.Type.OutCubic)
                anim.finished.connect(lambda: target.setGraphicsEffect(None))
                self._page_anim = anim  # prevent GC
                anim.start()
            except (AttributeError, RuntimeError):
                self.stack.setCurrentIndex(index)
        else:
            self.stack.setCurrentIndex(index)

    # --- Toast ---

    def show_toast(self, message: str, level: str = "info"):
        """Show a toast notification in the bottom-right corner."""
        toast = ToastNotification(message, level, parent=self)
        margin = 16
        x = self.width() - toast.width() - margin
        y = self.height() - toast.height() - margin
        toast.move(x, y)
        toast.slide_in()

    # --- Actions ---

    def _load_csv(self):
        path, _ = QFileDialog.getOpenFileName(self, "Load CSV Data", "", "CSV Files (*.csv);;All Files (*)")
        if path:
            self._status.setText(f"Loaded: {Path(path).name}")
            self.show_toast(f"Loaded {Path(path).name}", "success")

    def _refresh_current(self):
        page = self.stack.currentWidget()
        if hasattr(page, "refresh"):
            page.refresh()
        self._status.setText("Refreshed")

    def _about(self):
        QMessageBox.about(
            self,
            f"About {APP_NAME}",
            f"<h2>{APP_NAME}</h2>"
            f"<p>Version {APP_VERSION}</p>"
            f"<p>Professional time series forecasting workbench for<br>"
            f"ARIMA, Prophet-style models, changepoint detection,<br>"
            f"and series decomposition.</p>"
            f"<p>Models: ARIMA, Prophet, Neural Networks</p>"
            f"<p>&copy; 2022-2026 Zain Dana Harper</p>",
        )

    # --- Geometry Persistence ---

    def _restore_geometry(self):
        geo = self.settings.value("window/geometry")
        if geo:
            self.restoreGeometry(geo)

    def closeEvent(self, event):
        self.settings.setValue("window/geometry", self.saveGeometry())
        event.accept()


# =============================================================================
# Entry Point
# =============================================================================

if __name__ == "__main__":
    from build_oracle.gui import launch

    sys.exit(launch())
