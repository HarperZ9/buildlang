"""
Build Finance — Main Application

Professional algorithmic trading workbench with sidebar navigation,
page transitions, and the shared Calibrate Pro visual framework.
"""

import logging
import sys
from pathlib import Path

logger = logging.getLogger(__name__)

from build_ui.theme import STYLE, C
from build_ui.widgets import Card, Heading, Sidebar, Stat, StatusDot, ToastNotification
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

__all__ = [
    "APP_NAME",
    "APP_ORG",
    "APP_VERSION",
    "C",
    "Card",
    "Heading",
    "Stat",
    "StatusDot",
    "BuildFinanceWindow",
    "make_app_icon",
]

APP_NAME = "Build Finance"
APP_VERSION = "1.0.0"
APP_ORG = "Build Universe"


# =============================================================================
# Application Icon — candlestick chart
# =============================================================================


def make_app_icon() -> QIcon:
    """
    Create the application icon programmatically.

    A stylized candlestick / chart icon rendered at multiple
    sizes for crisp display at any DPI.
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

        # Background circle — soft cream
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor("#fdf9f5"))
        p.drawEllipse(QPointF(cx, cy), s * 0.45, s * 0.45)

        # Outer ring — accent pink
        ring_pen = QPen(QColor("#d4a0a0"), max(1.5, s * 0.04))
        p.setPen(ring_pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawEllipse(QPointF(cx, cy), s * 0.42, s * 0.42)

        # Draw candlesticks
        bar_width = max(2.0, s * 0.08)
        wick_width = max(1.0, s * 0.025)

        candles = [
            # (x_frac, top_wick, body_top, body_bot, bot_wick, color)
            (0.25, 0.25, 0.30, 0.55, 0.65, "#92ad7e"),  # green (up)
            (0.40, 0.20, 0.25, 0.50, 0.60, "#d08888"),  # red (down)
            (0.55, 0.30, 0.35, 0.60, 0.70, "#92ad7e"),  # green (up)
            (0.70, 0.15, 0.22, 0.48, 0.55, "#92ad7e"),  # green (up)
        ]

        for x_frac, tw, bt, bb, bw, color in candles:
            x = s * x_frac
            # Wick
            wick_pen = QPen(QColor(color), wick_width)
            wick_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
            p.setPen(wick_pen)
            p.drawLine(QPointF(x, s * tw), QPointF(x, s * bw))
            # Body
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(color))
            body_rect = QRectF(x - bar_width / 2, s * bt, bar_width, s * (bb - bt))
            p.drawRoundedRect(body_rect, 1.5, 1.5)

        # Trend line — soft accent
        trend_pen = QPen(QColor("#e0c87a"), max(1.0, s * 0.03))
        trend_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        p.setPen(trend_pen)
        p.drawLine(QPointF(s * 0.20, s * 0.65), QPointF(s * 0.80, s * 0.28))

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
    "Backtest",
    "Auto-Trader",
    "Portfolio",
    "Market Data",
    "Settings",
]

PAGE_SHORTCUTS = ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4", "Ctrl+5", "Ctrl+6"]

PAGE_MENU_NAMES = [
    "&Dashboard",
    "&Backtest",
    "&Auto-Trader",
    "&Portfolio",
    "&Market Data",
    "&Settings",
]


class BuildFinanceWindow(QMainWindow):
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
        file_menu.addAction(QAction("&Import Data...", self, shortcut="Ctrl+I", triggered=self._import_data))
        file_menu.addAction(QAction("&Export Results...", self, shortcut="Ctrl+E", triggered=self._export_results))
        file_menu.addSeparator()
        file_menu.addAction(QAction("E&xit", self, shortcut="Alt+F4", triggered=self.close))

        # View — page navigation shortcuts
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
            from build_finance.gui.pages.dashboard import DashboardPage

            self.stack.addWidget(DashboardPage())
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load DashboardPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Dashboard"))

        # Page 1: Backtest
        try:
            from build_finance.gui.pages.backtest_page import BacktestPage

            self.stack.addWidget(BacktestPage())
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load BacktestPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Backtest"))

        # Page 2: Auto-Trader
        try:
            from build_finance.gui.pages.autotrader_page import AutoTraderPage

            self.stack.addWidget(AutoTraderPage())
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load AutoTraderPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Auto-Trader"))

        # Page 3: Portfolio
        try:
            from build_finance.gui.pages.portfolio_page import PortfolioPage

            self.stack.addWidget(PortfolioPage())
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load PortfolioPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Portfolio"))

        # Page 4: Market Data
        try:
            from build_finance.gui.pages.market_data_page import MarketDataPage

            self.stack.addWidget(MarketDataPage())
        except (ImportError, AttributeError, TypeError) as e:
            logger.warning("Failed to load MarketDataPage: %s", e)
            self.stack.addWidget(PlaceholderPage("Market Data"))

        # Page 5: Settings
        try:
            from build_finance.gui.pages.settings_page import SettingsPage

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

    def _import_data(self):
        path, _ = QFileDialog.getOpenFileName(
            self, "Import Data", "", "Data Files (*.csv *.json *.xlsx *.parquet);;All Files (*)"
        )
        if path:
            self._status.setText(f"Imported: {Path(path).name}")
            self.show_toast(f"Imported {Path(path).name}", "success")

    def _export_results(self):
        path, _ = QFileDialog.getSaveFileName(
            self, "Export Results", "", "CSV Files (*.csv);;JSON Files (*.json);;All Files (*)"
        )
        if path:
            self._status.setText(f"Exported: {Path(path).name}")
            self.show_toast(f"Exported to {Path(path).name}", "success")

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
            f"<p>Professional algorithmic trading workbench for<br>"
            f"backtesting, auto-trading, and portfolio optimization.</p>"
            f"<p>Strategies: Momentum, Mean Reversion, Trend, Breakout</p>"
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
    from build_finance.gui import launch

    sys.exit(launch())
