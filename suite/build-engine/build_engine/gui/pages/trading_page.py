"""
Trading -- live trading view.

Shows current position status, P&L breakdown, recent trade history,
and buy/sell signal indicators powered by the adaptive engine.
"""

from PyQt6.QtCore import QPointF, QRectF, Qt, QTimer
from PyQt6.QtGui import (
    QColor,
    QFont,
    QPainter,
)
from PyQt6.QtWidgets import (
    QGridLayout,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QPushButton,
    QScrollArea,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from build_engine.gui.app import C, Card, Heading, Stat

# Signal Indicator Widget


class SignalIndicator(QWidget):
    """Visual buy/sell/flat signal gauge."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(80)
        self.setMinimumWidth(200)
        self._signal = "flat"  # "buy", "sell", "flat"
        self._strength = 0.0  # 0.0 .. 1.0

    def set_signal(self, signal: str, strength: float = 0.0):
        self._signal = signal
        self._strength = max(0.0, min(1.0, strength))
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        cx = w / 2
        cy = h / 2

        # Background rounded rect
        bg = QColor(C.SURFACE2)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(bg)
        p.drawRoundedRect(QRectF(0, 0, w, h), 12, 12)

        # Signal colors
        colors = {
            "buy": QColor(C.GREEN),
            "sell": QColor(C.RED),
            "flat": QColor(C.TEXT3),
        }
        color = colors.get(self._signal, QColor(C.TEXT3))

        # Big circle indicator
        radius = min(w, h) * 0.28
        glow = QColor(color)
        glow.setAlpha(int(60 * self._strength) if self._signal != "flat" else 20)
        p.setBrush(glow)
        p.drawEllipse(QPointF(cx - 40, cy), radius + 6, radius + 6)

        p.setBrush(color)
        p.drawEllipse(QPointF(cx - 40, cy), radius, radius)

        # Label
        p.setPen(QColor(C.TEXT))
        p.setFont(QFont("Segoe UI", 14, QFont.Weight.Bold))
        label = self._signal.upper()
        p.drawText(
            QRectF(cx - 10, cy - 16, w / 2, 32),
            Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
            label,
        )

        # Strength bar
        bar_x = cx + 10
        bar_y = cy + 12
        bar_w = w * 0.35
        bar_h = 8

        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.BORDER))
        p.drawRoundedRect(QRectF(bar_x, bar_y, bar_w, bar_h), 4, 4)

        if self._strength > 0:
            p.setBrush(color)
            p.drawRoundedRect(
                QRectF(bar_x, bar_y, bar_w * self._strength, bar_h),
                4,
                4,
            )

        # Strength text
        p.setPen(QColor(C.TEXT2))
        p.setFont(QFont("Segoe UI", 9))
        p.drawText(
            QRectF(bar_x, bar_y - 16, bar_w, 14),
            Qt.AlignmentFlag.AlignLeft,
            f"Strength: {self._strength:.0%}",
        )

        p.end()


# P&L Breakdown Widget


class PnlBreakdown(QWidget):
    """Colored P&L bar showing profit/loss distribution."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(36)
        self.setMinimumWidth(200)
        self._wins = 0.0
        self._losses = 0.0

    def set_data(self, wins: float, losses: float):
        self._wins = abs(wins)
        self._losses = abs(losses)
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        total = self._wins + self._losses

        # Background track
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.BORDER))
        p.drawRoundedRect(QRectF(0, 8, w, h - 16), 6, 6)

        if total > 0:
            win_frac = self._wins / total
            # Green portion
            p.setBrush(QColor(C.GREEN))
            p.drawRoundedRect(QRectF(0, 8, w * win_frac, h - 16), 6, 6)

            # Red portion (overlap handled by draw order)
            p.setBrush(QColor(C.RED))
            p.drawRoundedRect(
                QRectF(w * win_frac, 8, w * (1 - win_frac), h - 16),
                6,
                6,
            )

        p.end()


# Trading Page


class TradingPage(QWidget):
    """Live trading view with position, P&L, history, and signals."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._build_ui()
        self._load_demo_data()

        # Refresh timer
        self._timer = QTimer(self)
        self._timer.timeout.connect(self._refresh_data)
        self._timer.start(5000)

    def _build_ui(self):
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(18)

        layout.addWidget(Heading("Trading"))

        # Row 1: Position Status + Signal Indicator
        row1 = QHBoxLayout()
        row1.setSpacing(16)

        # Position Status card
        card_pos, pos_lay = Card.with_layout()
        pos_lay.addWidget(Heading("Current Position", level=2))

        pos_grid = QGridLayout()
        pos_grid.setSpacing(10)

        pos_grid.addWidget(QLabel("Status:"), 0, 0, Qt.AlignmentFlag.AlignRight)
        self._position_label = QLabel("FLAT")
        self._position_label.setStyleSheet(f"font-size: 16px; font-weight: 700; color: {C.TEXT3};")
        pos_grid.addWidget(self._position_label, 0, 1)

        pos_grid.addWidget(QLabel("Symbol:"), 1, 0, Qt.AlignmentFlag.AlignRight)
        self._pos_symbol = QLabel("--")
        self._pos_symbol.setStyleSheet(f"font-weight: 600; color: {C.TEXT};")
        pos_grid.addWidget(self._pos_symbol, 1, 1)

        pos_grid.addWidget(QLabel("Entry Price:"), 2, 0, Qt.AlignmentFlag.AlignRight)
        self._pos_entry = QLabel("--")
        self._pos_entry.setStyleSheet(f"color: {C.TEXT2};")
        pos_grid.addWidget(self._pos_entry, 2, 1)

        pos_grid.addWidget(QLabel("Quantity:"), 3, 0, Qt.AlignmentFlag.AlignRight)
        self._pos_qty = QLabel("--")
        self._pos_qty.setStyleSheet(f"color: {C.TEXT2};")
        pos_grid.addWidget(self._pos_qty, 3, 1)

        pos_grid.addWidget(QLabel("Unrealized P&L:"), 4, 0, Qt.AlignmentFlag.AlignRight)
        self._pos_pnl = QLabel("$0.00")
        self._pos_pnl.setStyleSheet(f"font-size: 14px; font-weight: 600; color: {C.TEXT2};")
        pos_grid.addWidget(self._pos_pnl, 4, 1)

        pos_lay.addLayout(pos_grid)
        pos_lay.addStretch()
        card_pos.setMinimumWidth(280)
        row1.addWidget(card_pos)

        # Signal Indicator card
        card_signal, signal_lay = Card.with_layout()
        signal_lay.addWidget(Heading("Signal", level=2))
        self._signal_indicator = SignalIndicator()
        signal_lay.addWidget(self._signal_indicator)

        self._signal_details = QLabel("No active signal")
        self._signal_details.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; padding: 6px 0;")
        self._signal_details.setWordWrap(True)
        signal_lay.addWidget(self._signal_details)
        signal_lay.addStretch()
        card_signal.setMinimumWidth(280)
        row1.addWidget(card_signal)

        # P&L Summary card
        card_pnl, pnl_lay = Card.with_layout()
        pnl_lay.addWidget(Heading("Profit & Loss", level=2))

        pnl_stats = QGridLayout()
        pnl_stats.setSpacing(10)

        self._pnl_total = Stat("Total P&L", "$0.00", C.TEXT)
        self._pnl_today = Stat("Today", "$0.00", C.TEXT2)
        self._pnl_week = Stat("This Week", "$0.00", C.TEXT2)
        self._pnl_win_rate = Stat("Win Rate", "0%", C.TEXT2)

        pnl_stats.addWidget(self._pnl_total, 0, 0)
        pnl_stats.addWidget(self._pnl_today, 0, 1)
        pnl_stats.addWidget(self._pnl_week, 1, 0)
        pnl_stats.addWidget(self._pnl_win_rate, 1, 1)
        pnl_lay.addLayout(pnl_stats)

        pnl_lay.addWidget(QLabel("Win/Loss Distribution:"))
        self._pnl_bar = PnlBreakdown()
        pnl_lay.addWidget(self._pnl_bar)

        pnl_lay.addStretch()
        row1.addWidget(card_pnl, stretch=1)

        layout.addLayout(row1)

        # Row 2: Trade History Table
        card_history, history_lay = Card.with_layout()
        history_lay.addWidget(Heading("Trade History", level=2))

        self._trade_table = QTableWidget(0, 7)
        self._trade_table.setHorizontalHeaderLabels(
            [
                "Time",
                "Symbol",
                "Direction",
                "Qty",
                "Entry",
                "Exit",
                "P&L",
            ]
        )
        self._trade_table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        self._trade_table.verticalHeader().setVisible(False)
        self._trade_table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._trade_table.setSelectionMode(QTableWidget.SelectionMode.SingleSelection)
        self._trade_table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self._trade_table.setMinimumHeight(320)
        self._trade_table.setStyleSheet(
            f"QTableWidget {{ background: {C.SURFACE}; "
            f"border: 1px solid {C.BORDER}; border-radius: 8px; "
            f"gridline-color: {C.BORDER}; }}"
            f"QTableWidget::item {{ padding: 6px 10px; }}"
            f"QHeaderView::section {{ background: {C.SURFACE2}; "
            f"border: none; border-bottom: 1px solid {C.BORDER}; "
            f"padding: 8px; font-weight: 600; color: {C.TEXT2}; "
            f"font-size: 11px; }}"
        )
        history_lay.addWidget(self._trade_table)

        # Controls row
        ctrl_row = QHBoxLayout()
        ctrl_row.setSpacing(8)

        self._trade_count_label = QLabel("0 trades")
        self._trade_count_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
        ctrl_row.addWidget(self._trade_count_label)
        ctrl_row.addStretch()

        btn_refresh = QPushButton("Refresh")
        btn_refresh.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_refresh.clicked.connect(self.refresh)
        ctrl_row.addWidget(btn_refresh)

        history_lay.addLayout(ctrl_row)
        layout.addWidget(card_history)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    # Demo data

    def _load_demo_data(self):
        """Populate the page with demo data for initial display."""
        # Position
        self._set_position("long", "AAPL", "$178.23", "50", "+$342.00")

        # Signal
        self._signal_indicator.set_signal("buy", 0.72)
        self._signal_details.setText(
            "Ensemble prediction: +1.4% over 5 steps. ARIMA (up), Prophet (up), Neural (flat)."
        )

        # P&L
        self._pnl_total.set_value("+$2,847", C.GREEN)
        self._pnl_today.set_value("+$342", C.GREEN)
        self._pnl_week.set_value("+$1,205", C.GREEN)
        self._pnl_win_rate.set_value("61%", C.GREEN)
        self._pnl_bar.set_data(2847, 1823)

        # Trade history
        demo_trades = [
            ("03/24 14:30", "AAPL", "BUY", "50", "$178.23", "--", "+$342"),
            ("03/24 11:15", "BTC-USD", "SELL", "0.3", "$68,100", "$67,897", "-$61"),
            ("03/24 09:45", "ETH-USD", "BUY", "3.0", "$3,456", "$3,512", "+$168"),
            ("03/23 15:50", "AAPL", "SELL", "25", "$181.45", "$182.10", "+$16"),
            ("03/23 14:20", "BTC-USD", "BUY", "0.5", "$67,420", "$68,100", "+$340"),
            ("03/23 10:30", "AAPL", "BUY", "30", "$176.90", "$178.23", "+$40"),
            ("03/22 15:45", "ETH-USD", "SELL", "2.0", "$3,512", "$3,390", "+$244"),
            ("03/22 11:00", "BTC-USD", "SELL", "0.2", "$66,890", "$67,420", "-$106"),
            ("03/22 09:30", "AAPL", "BUY", "40", "$179.80", "$176.90", "-$116"),
            ("03/21 14:00", "ETH-USD", "BUY", "1.5", "$3,390", "$3,456", "+$99"),
        ]
        self._populate_trade_table(demo_trades)

    def _set_position(
        self,
        status: str,
        symbol: str,
        entry: str,
        qty: str,
        pnl: str,
    ):
        """Update position display."""
        status_upper = status.upper()
        if status_upper == "LONG":
            color = C.GREEN
        elif status_upper == "SHORT":
            color = C.RED
        else:
            color = C.TEXT3

        self._position_label.setText(status_upper)
        self._position_label.setStyleSheet(f"font-size: 16px; font-weight: 700; color: {color};")
        self._pos_symbol.setText(symbol)
        self._pos_entry.setText(entry)
        self._pos_qty.setText(qty)

        pnl_color = C.GREEN if pnl.startswith("+") else C.RED
        self._pos_pnl.setText(pnl)
        self._pos_pnl.setStyleSheet(f"font-size: 14px; font-weight: 600; color: {pnl_color};")

    def _populate_trade_table(self, trades: list):
        """Fill the trade history table from a list of tuples."""
        self._trade_table.setRowCount(len(trades))

        for row, trade in enumerate(trades):
            for col, val in enumerate(trade):
                item = QTableWidgetItem(val)
                item.setTextAlignment(Qt.AlignmentFlag.AlignCenter)

                # Color the Direction column
                if col == 2:
                    color = C.GREEN if val == "BUY" else C.RED
                    item.setForeground(QColor(color))
                    item.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))

                # Color the P&L column
                if col == 6:
                    color = C.GREEN if val.startswith("+") else C.RED
                    item.setForeground(QColor(color))
                    item.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))

                self._trade_table.setItem(row, col, item)

        self._trade_count_label.setText(f"{len(trades)} trades")

    # Live data integration

    def update_from_engine(self, status: dict):
        """Update trading page from engine status dict."""
        # Position info
        positions = status.get("positions", 0)
        if positions > 0:
            self._position_label.setText("ACTIVE")
            self._position_label.setStyleSheet(f"font-size: 16px; font-weight: 700; color: {C.GREEN};")
        else:
            self._position_label.setText("FLAT")
            self._position_label.setStyleSheet(f"font-size: 16px; font-weight: 700; color: {C.TEXT3};")

    def update_signal(self, direction: str, strength: float, details: str):
        """Update the signal indicator from engine predictions."""
        self._signal_indicator.set_signal(direction, strength)
        self._signal_details.setText(details)

    def add_trade(self, trade: dict):
        """Insert a trade at the top of the history table."""
        self._trade_table.rowCount()
        self._trade_table.insertRow(0)

        cols = [
            trade.get("time", "--"),
            trade.get("symbol", "--"),
            trade.get("direction", "--"),
            trade.get("qty", "--"),
            trade.get("entry", "--"),
            trade.get("exit", "--"),
            trade.get("pnl", "--"),
        ]

        for col, val in enumerate(cols):
            item = QTableWidgetItem(str(val))
            item.setTextAlignment(Qt.AlignmentFlag.AlignCenter)

            if col == 2:
                color = C.GREEN if val == "BUY" else C.RED
                item.setForeground(QColor(color))
                item.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
            if col == 6:
                s = str(val)
                color = C.GREEN if s.startswith("+") else C.RED
                item.setForeground(QColor(color))
                item.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))

            self._trade_table.setItem(0, col, item)

        # Cap at 50 rows
        while self._trade_table.rowCount() > 50:
            self._trade_table.removeRow(self._trade_table.rowCount() - 1)

        self._trade_count_label.setText(f"{self._trade_table.rowCount()} trades")

    def _refresh_data(self):
        """Refresh from live engine data if available."""
        pass

    def refresh(self):
        self._refresh_data()
