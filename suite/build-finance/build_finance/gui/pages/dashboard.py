"""
Build Finance -- Dashboard Page

Account overview, open positions table, portfolio allocation stats,
and quick actions -- all wired to live Alpaca data via DataBridge.
"""

from __future__ import annotations

import logging

from PyQt6.QtCore import Qt, QTimer
from PyQt6.QtWidgets import (
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QTableWidget,
    QTableWidgetItem,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from build_finance.gui.app import C, Card, Heading, Stat, StatusDot

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _fmt_currency(value: float) -> str:
    """Format a float as $X,XXX.XX."""
    return f"${value:,.2f}"


def _fmt_pnl(value: float) -> str:
    """Format P&L with sign and color hint."""
    return f"${value:+,.2f}"


def _pnl_color(value: float) -> str:
    return C.GREEN if value >= 0 else C.RED


_TABLE_STYLE = (
    f"QTableWidget {{"
    f"  background: {C.SURFACE2};"
    f"  border: 1px solid {C.BORDER};"
    f"  border-radius: 8px;"
    f"  gridline-color: {C.BORDER};"
    f"  font-size: 12px;"
    f"  color: {C.TEXT};"
    f"}}"
    f"QTableWidget::item {{"
    f"  padding: 4px 8px;"
    f"}}"
    f"QHeaderView::section {{"
    f"  background: {C.SURFACE};"
    f"  color: {C.TEXT2};"
    f"  font-weight: 600;"
    f"  font-size: 11px;"
    f"  border: none;"
    f"  border-bottom: 1px solid {C.BORDER};"
    f"  padding: 6px 8px;"
    f"}}"
)


# ---------------------------------------------------------------------------
# Dashboard Page
# ---------------------------------------------------------------------------


class DashboardPage(QWidget):
    """Main dashboard: account overview, positions, allocation, actions."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._bridge = None
        self._refresh_timer = None
        self._build_ui()
        self._init_bridge()

    # -- data bridge --------------------------------------------------------

    def _init_bridge(self) -> None:
        """Create the data bridge and start periodic refresh."""
        try:
            from build_finance.gui.data_bridge import DataBridge

            self._bridge = DataBridge(use_paper=True)
        except Exception as exc:
            logger.warning("DataBridge init failed: %s", exc)
            self._bridge = None

        self._load_data()

        # Auto-refresh every 15 seconds
        self._refresh_timer = QTimer(self)
        self._refresh_timer.timeout.connect(self._load_data)
        self._refresh_timer.start(15_000)

    # -- UI build -----------------------------------------------------------

    def _build_ui(self) -> None:
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        # Page heading
        layout.addWidget(Heading("Dashboard"))

        subtitle = QLabel("Algorithmic trading workbench overview")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Account Overview Card ---
        acct_card, acct_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )

        acct_header = QHBoxLayout()
        acct_header.addWidget(Heading("Account Overview", level=2))

        self._acct_dot = StatusDot(C.TEXT3, 12)
        acct_header.addWidget(self._acct_dot)

        self._acct_status_label = QLabel("Connecting...")
        self._acct_status_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
        acct_header.addWidget(self._acct_status_label)
        acct_header.addStretch()
        acct_layout.addLayout(acct_header)

        # Account stats row
        acct_stats = QHBoxLayout()
        self._stat_equity = Stat("Equity", "\u2014", C.TEXT)
        self._stat_buying_power = Stat("Buying Power", "\u2014", C.TEXT)
        self._stat_cash = Stat("Cash", "\u2014", C.TEXT)
        self._stat_portfolio = Stat("Portfolio Value", "\u2014", C.TEXT)

        acct_stats.addWidget(self._stat_equity)
        acct_stats.addWidget(self._stat_buying_power)
        acct_stats.addWidget(self._stat_cash)
        acct_stats.addWidget(self._stat_portfolio)
        acct_stats.addStretch()
        acct_layout.addLayout(acct_stats)

        layout.addWidget(acct_card)

        # --- Open Positions Table ---
        pos_card, pos_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        pos_layout.addWidget(Heading("Open Positions", level=2))

        self._positions_table = QTableWidget(0, 6)
        self._positions_table.setHorizontalHeaderLabels(
            [
                "Symbol",
                "Qty",
                "Avg Entry",
                "Current",
                "Unrealized P&L",
                "%",
            ]
        )
        self._positions_table.setStyleSheet(_TABLE_STYLE)
        self._positions_table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._positions_table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self._positions_table.setAlternatingRowColors(True)
        self._positions_table.verticalHeader().setVisible(False)
        self._positions_table.setFixedHeight(180)

        hdr = self._positions_table.horizontalHeader()
        hdr.setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        pos_layout.addWidget(self._positions_table)

        layout.addWidget(pos_card)

        # --- Portfolio Allocation Stats ---
        alloc_card, alloc_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        alloc_layout.addWidget(Heading("Portfolio Allocation", level=2))

        self._alloc_stats_row = QHBoxLayout()
        self._alloc_labels: list[Stat] = []
        # Populated dynamically by _load_data
        alloc_layout.addLayout(self._alloc_stats_row)

        self._alloc_placeholder = QLabel("Allocation data will appear when positions are loaded.")
        self._alloc_placeholder.setStyleSheet(f"font-size: 12px; color: {C.TEXT3};")
        alloc_layout.addWidget(self._alloc_placeholder)

        layout.addWidget(alloc_card)

        # --- Quick Actions Card ---
        actions_card, actions_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        actions_layout.addWidget(Heading("Quick Actions", level=2))

        actions_row = QHBoxLayout()
        actions_row.setSpacing(12)

        btn_refresh = QPushButton("Refresh")
        btn_refresh.setProperty("primary", True)
        btn_refresh.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_refresh.setToolTip("Refresh all dashboard data (F5)")
        btn_refresh.clicked.connect(self.refresh)
        actions_row.addWidget(btn_refresh)

        btn_flatten = QPushButton("Flatten All")
        btn_flatten.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_flatten.setToolTip("Close all open positions")
        btn_flatten.clicked.connect(self._flatten_all)
        actions_row.addWidget(btn_flatten)

        btn_stop = QPushButton("Emergency Stop")
        btn_stop.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_stop.setToolTip("Immediately stop all trading activity")
        btn_stop.setStyleSheet(
            f"QPushButton {{"
            f"  background: {C.RED}; color: white;"
            f"  border: none; border-radius: 8px;"
            f"  font-weight: 600; padding: 6px 16px;"
            f"}}"
            f"QPushButton:hover {{ background: #c07070; }}"
        )
        btn_stop.clicked.connect(self._emergency_stop)
        actions_row.addWidget(btn_stop)

        actions_row.addStretch()
        actions_layout.addLayout(actions_row)

        layout.addWidget(actions_card)

        # --- Recent Activity Card ---
        activity_card, activity_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        activity_layout.addWidget(Heading("Recent Activity", level=2))

        self._activity_log = QTextEdit()
        self._activity_log.setReadOnly(True)
        self._activity_log.setFixedHeight(160)
        self._activity_log.setStyleSheet(
            f"QTextEdit {{"
            f"  background: {C.SURFACE2};"
            f"  border: 1px solid {C.BORDER};"
            f"  border-radius: 8px;"
            f"  padding: 10px;"
            f"  font-family: 'Cascadia Code', 'Consolas', monospace;"
            f"  font-size: 11px;"
            f"  color: {C.TEXT};"
            f"}}"
        )
        self._activity_log.setPlaceholderText("No recent trades. Start the auto-trader to see activity.")
        activity_layout.addWidget(self._activity_log)

        layout.addWidget(activity_card)

        layout.addStretch()
        scroll.setWidget(content)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    # -- data loading -------------------------------------------------------

    def _load_data(self) -> None:
        """Pull data from the bridge and update all widgets."""
        if self._bridge is None:
            return

        # Account
        try:
            acct = self._bridge.get_account()
            source = self._bridge.source_label
            is_demo = self._bridge.is_demo

            dot_color = C.YELLOW if is_demo else C.GREEN
            self._acct_dot.set_color(dot_color)
            self._acct_status_label.setText(f"Connected ({source})")
            self._acct_status_label.setStyleSheet(f"font-size: 11px; color: {dot_color};")

            self._stat_equity.set_value(
                _fmt_currency(acct.equity),
                C.TEXT,
            )
            self._stat_buying_power.set_value(
                _fmt_currency(acct.buying_power),
                C.TEXT,
            )
            self._stat_cash.set_value(
                _fmt_currency(acct.cash),
                C.TEXT,
            )
            self._stat_portfolio.set_value(
                _fmt_currency(acct.portfolio_value),
                C.TEXT,
            )
        except Exception as exc:
            logger.warning("Dashboard account load error: %s", exc)

        # Positions
        try:
            positions = self._bridge.get_positions()
            self._update_positions_table(positions)
            self._update_allocation(positions)
        except Exception as exc:
            logger.warning("Dashboard positions load error: %s", exc)

    def _update_positions_table(self, positions) -> None:
        """Populate the positions table from a list of PositionInfo."""
        self._positions_table.setRowCount(len(positions))
        for row, pos in enumerate(positions):
            self._positions_table.setItem(
                row,
                0,
                QTableWidgetItem(pos.symbol),
            )
            self._positions_table.setItem(
                row,
                1,
                QTableWidgetItem(f"{pos.qty:.0f}"),
            )
            self._positions_table.setItem(
                row,
                2,
                QTableWidgetItem(_fmt_currency(pos.avg_entry)),
            )
            self._positions_table.setItem(
                row,
                3,
                QTableWidgetItem(_fmt_currency(pos.current_price)),
            )

            pnl_item = QTableWidgetItem(_fmt_pnl(pos.unrealized_pnl))
            pnl_item.setForeground(
                __import__("PyQt6.QtGui", fromlist=["QColor"]).QColor(_pnl_color(pos.unrealized_pnl))
            )
            self._positions_table.setItem(row, 4, pnl_item)

            pct_item = QTableWidgetItem(f"{pos.pnl_percent:+.2f}%")
            pct_item.setForeground(__import__("PyQt6.QtGui", fromlist=["QColor"]).QColor(_pnl_color(pos.pnl_percent)))
            self._positions_table.setItem(row, 5, pct_item)

    def _update_allocation(self, positions) -> None:
        """Update portfolio allocation stats from position data."""
        # Clear old widgets
        for stat in self._alloc_labels:
            stat.setParent(None)
        self._alloc_labels.clear()

        if not positions:
            self._alloc_placeholder.setVisible(True)
            return
        self._alloc_placeholder.setVisible(False)

        total_value = sum(pos.current_price * pos.qty for pos in positions)
        if total_value <= 0:
            return

        colors = [C.ACCENT_TX, C.GREEN, C.CYAN, C.YELLOW, C.RED]
        for i, pos in enumerate(positions):
            value = pos.current_price * pos.qty
            pct = value / total_value * 100
            color = colors[i % len(colors)]
            stat = Stat(
                pos.symbol,
                f"{pct:.1f}% (${value:,.0f})",
                color,
            )
            self._alloc_stats_row.addWidget(stat)
            self._alloc_labels.append(stat)

        self._alloc_stats_row.addStretch()

    # -- actions ------------------------------------------------------------

    def _flatten_all(self) -> None:
        """Prompt, then close all positions."""
        reply = QMessageBox.question(
            self,
            "Flatten All Positions",
            "Are you sure you want to close ALL open positions?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            QMessageBox.StandardButton.No,
        )
        if reply != QMessageBox.StandardButton.Yes:
            return

        if self._bridge:
            results = self._bridge.flatten_all()
            count = len(results)
            self.add_activity(f"Flatten all: {count} position(s) closed.")
            self._bridge.invalidate_cache()
            self._load_data()

    def _emergency_stop(self) -> None:
        """Emergency stop: flatten + stop auto-trader."""
        reply = QMessageBox.warning(
            self,
            "Emergency Stop",
            "This will close ALL positions and stop the auto-trader.\nContinue?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            QMessageBox.StandardButton.No,
        )
        if reply != QMessageBox.StandardButton.Yes:
            return

        if self._bridge:
            self._bridge.flatten_all()
        self.add_activity("EMERGENCY STOP activated.")
        self._load_data()

    def add_activity(self, message: str) -> None:
        """Append a timestamped message to the activity log."""
        from datetime import datetime as dt

        ts = dt.now().strftime("%H:%M:%S")
        self._activity_log.append(f"[{ts}] {message}")

    # -- navigation helpers -------------------------------------------------

    def _find_main_window(self):
        widget = self.parent()
        while widget:
            if hasattr(widget, "sidebar"):
                return widget
            widget = widget.parent() if hasattr(widget, "parent") else None
        return None

    def _navigate_to(self, index: int) -> None:
        main = self._find_main_window()
        if main:
            main._switch_page(index)
            main.sidebar._on_click(index)

    # -- refresh ------------------------------------------------------------

    def refresh(self) -> None:
        """Refresh all dashboard data."""
        if self._bridge:
            self._bridge.invalidate_cache()
        self._load_data()
