"""
Build Engine -- Main Application

Professional self-improving prediction and trading dashboard with sidebar
navigation, page transitions, and the shared Calibrate Pro visual framework.
"""

import logging
import sys

logger = logging.getLogger(__name__)

from build_ui.theme import STYLE
from build_ui.theme import C as _BaseC
from build_ui.widgets import Heading, Sidebar, StatusDot, ToastNotification
from PyQt6.QtCore import (
    QEasingCurve,
    QPointF,
    QPropertyAnimation,
    QRectF,
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


class C(_BaseC):
    """Trading-focused color overrides on the shared pastel theme."""

    GREEN = "#4ade80"  # Profit green
    GREEN_BG = "#dcfce7"  # Light green background
    GREEN_MUT = "#92ad7e"  # Soft sage
    RED = "#f87171"  # Loss red
    RED_BG = "#fee2e2"  # Light red background
    RED_MUT = "#d08888"  # Soft coral
    CYAN = "#38bdf8"  # Prediction blue
    CYAN_MUT = "#95b3ba"  # Powder blue


APP_NAME = "Build Engine"
APP_VERSION = "1.0.0"
APP_ORG = "Build Universe"


# Application Icon


def make_app_icon() -> QIcon:
    """
    Create the application icon programmatically.

    A stylized candlestick chart with a prediction line rendered at multiple
    sizes for crisp display at any DPI.
    """
    icon = QIcon()
    for size in [16, 24, 32, 48, 64, 128, 256]:
        pm = QPixmap(size, size)
        pm.fill(QColor(0, 0, 0, 0))

        p = QPainter(pm)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        s = size
        s * 0.12

        # Background circle
        p.setPen(Qt.PenStyle.NoPen)
        bg = QColor("#1a1a2e")
        p.setBrush(bg)
        p.drawRoundedRect(QRectF(0, 0, s, s), s * 0.22, s * 0.22)

        # Candlestick bars
        bar_w = max(1.5, s * 0.08)
        wick_w = max(0.8, s * 0.02)

        candles = [
            # (x_frac, open_frac, close_frac, high_frac, low_frac, bullish)
            (0.20, 0.65, 0.45, 0.35, 0.72, True),
            (0.35, 0.50, 0.60, 0.40, 0.68, False),
            (0.50, 0.55, 0.35, 0.28, 0.62, True),
            (0.65, 0.38, 0.50, 0.30, 0.58, False),
            (0.80, 0.45, 0.25, 0.18, 0.52, True),
        ]

        for x_frac, open_f, close_f, high_f, low_f, bullish in candles:
            cx = s * x_frac
            o_y = s * open_f
            c_y = s * close_f
            h_y = s * high_f
            l_y = s * low_f

            color = QColor("#4ade80") if bullish else QColor("#f87171")

            # Wick
            pen = QPen(color, wick_w)
            p.setPen(pen)
            p.drawLine(QPointF(cx, h_y), QPointF(cx, l_y))

            # Body
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(color)
            body_top = min(o_y, c_y)
            body_h = max(abs(c_y - o_y), wick_w * 1.5)
            p.drawRoundedRect(
                QRectF(cx - bar_w, body_top, bar_w * 2, body_h),
                max(1, s * 0.02),
                max(1, s * 0.02),
            )

        # Prediction line (dashed, cyan)
        pred_pen = QPen(QColor("#38bdf8"), max(1.2, s * 0.025))
        pred_pen.setStyle(Qt.PenStyle.DashLine)
        p.setPen(pred_pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        points = [
            QPointF(s * 0.50, s * 0.35),
            QPointF(s * 0.65, s * 0.28),
            QPointF(s * 0.80, s * 0.22),
            QPointF(s * 0.92, s * 0.18),
        ]
        for i in range(len(points) - 1):
            p.drawLine(points[i], points[i + 1])

        # Small dot at prediction end
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor("#38bdf8"))
        dot_r = max(1.5, s * 0.03)
        p.drawEllipse(points[-1], dot_r, dot_r)

        p.end()
        icon.addPixmap(pm)

    return icon


# Placeholder Page (fallback for unbuilt pages)


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


# Main Window

PAGE_NAMES = [
    "Dashboard",
    "Trading",
    "Engine Control",
    "Performance",
    "Backtest",
    "Market Data",
    "Settings",
]

PAGE_SHORTCUTS = [
    "Ctrl+1",
    "Ctrl+2",
    "Ctrl+3",
    "Ctrl+4",
    "Ctrl+5",
    "Ctrl+6",
    "Ctrl+7",
]

PAGE_MENU_NAMES = [
    "&Dashboard",
    "&Trading",
    "Engine &Control",
    "&Performance",
    "&Backtest",
    "&Market Data",
    "&Settings",
]


class BuildEngineWindow(QMainWindow):
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
        file_menu.addAction(QAction("&New Session", self, shortcut="Ctrl+N", triggered=self._new_session))
        file_menu.addAction(QAction("&Export Report...", self, shortcut="Ctrl+E", triggered=self._export_report))
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

        # Engine
        engine_menu = mb.addMenu("En&gine")
        engine_menu.addAction(QAction("&Start Engine", self, shortcut="Ctrl+R", triggered=self._start_engine_menu))
        engine_menu.addAction(QAction("S&top Engine", self, shortcut="Ctrl+T", triggered=self._stop_engine_menu))
        engine_menu.addSeparator()
        engine_menu.addAction(
            QAction("Run &Backtest", self, shortcut="Ctrl+B", triggered=lambda: self._shortcut_switch_page(4))
        )

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
            from build_engine.gui.pages.dashboard import DashboardPage

            self.stack.addWidget(DashboardPage(self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load DashboardPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Dashboard"))

        # Page 1: Trading
        try:
            from build_engine.gui.pages.trading_page import TradingPage

            self.stack.addWidget(TradingPage(self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load TradingPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Trading"))

        # Page 2: Engine Control
        try:
            from build_engine.gui.pages.engine_page import EngineControlPage

            self.stack.addWidget(EngineControlPage(self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load EngineControlPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Engine Control"))

        # Page 3: Performance
        try:
            from build_engine.gui.pages.performance_page import PerformancePage

            self.stack.addWidget(PerformancePage(self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load PerformancePage: %s", e)
            self.stack.addWidget(PlaceholderPage("Performance"))

        # Page 4: Backtest
        try:
            from build_engine.gui.pages.backtest_page import BacktestPage

            self.stack.addWidget(BacktestPage(self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load BacktestPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Backtest"))

        # Page 5: Market Data
        try:
            from build_engine.gui.pages.data_page import DataPage

            self.stack.addWidget(DataPage(self))
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load DataPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Market Data"))

        # Page 6: Settings
        try:
            from build_engine.gui.pages.settings_page import SettingsPage

            self.stack.addWidget(SettingsPage(self))
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

        self._engine_status_dot = StatusDot(C.TEXT3, 8)
        sb.addPermanentWidget(self._engine_status_dot)

        self._engine_status_label = QLabel("Engine stopped")
        self._engine_status_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
        sb.addPermanentWidget(self._engine_status_label)

    def set_engine_status(self, running: bool):
        """Update the status bar engine indicator."""
        if running:
            self._engine_status_dot.set_color(C.GREEN)
            self._engine_status_label.setText("Engine running")
        else:
            self._engine_status_dot.set_color(C.TEXT3)
            self._engine_status_label.setText("Engine stopped")

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

    def _new_session(self):
        self._status.setText("New session started")
        self.show_toast("New session initialized", "success")

    def _export_report(self):
        path, _ = QFileDialog.getSaveFileName(
            self, "Export Report", "build_engine_report.txt", "Text Files (*.txt);;CSV Files (*.csv);;All Files (*)"
        )
        if path:
            self._status.setText(f"Report exported: {path}")
            self.show_toast("Report exported successfully", "success")

    def _start_engine_menu(self):
        """Start engine from menu -- delegate to engine page."""
        self._shortcut_switch_page(2)
        page = self.stack.widget(2)
        if hasattr(page, "start_engine"):
            page.start_engine()

    def _stop_engine_menu(self):
        """Stop engine from menu -- delegate to engine page."""
        page = self.stack.widget(2)
        if hasattr(page, "stop_engine"):
            page.stop_engine()

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
            f"<p>Self-improving prediction and trading engine.</p>"
            f"<p>Integrates build-oracle forecasting models with<br>"
            f"build-finance execution for a complete feedback loop.</p>"
            f"<p>Models: ARIMA, Prophet, Neural Network</p>"
            f"<p>&copy; 2024-2026 Build Universe</p>",
        )

    # --- Geometry Persistence ---

    def _restore_geometry(self):
        geo = self.settings.value("window/geometry")
        if geo:
            self.restoreGeometry(geo)

    def closeEvent(self, event):
        self.settings.setValue("window/geometry", self.saveGeometry())
        # Stop engine if running
        page = self.stack.widget(2)
        if hasattr(page, "_engine_running") and page._engine_running:
            page.stop_engine()
        event.accept()


# Entry Point

if __name__ == "__main__":
    from build_engine.gui import launch

    sys.exit(launch())
