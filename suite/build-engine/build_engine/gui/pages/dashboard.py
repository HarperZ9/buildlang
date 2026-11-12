"""
Dashboard -- the command center.

Shows at a glance: engine status, account overview, equity curve,
recent trades, model accuracy, and quick actions.
"""

import random
from collections import deque

from PyQt6.QtCore import QPointF, QRectF, Qt, QTimer
from PyQt6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QLinearGradient,
    QPainter,
    QPen,
    QPolygonF,
)
from PyQt6.QtWidgets import (
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from build_engine.gui.app import C, Card, Heading, Stat, StatusDot


class EquityCurveWidget(QWidget):
    """Live equity curve with green fill and drawdown shading."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(200)
        self.setMinimumWidth(400)
        self._data = deque(maxlen=200)
        self._peak = 0.0

    def set_data(self, values: list):
        self._data.clear()
        self._data.extend(values)
        self._peak = max(values) if values else 0.0
        self.update()

    def add_point(self, value: float):
        self._data.append(value)
        self._peak = max(self._peak, value)
        self.update()

    def paintEvent(self, event):
        if len(self._data) < 2:
            p = QPainter(self)
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Waiting for data...")
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        pad_x = 50
        pad_y = 20
        chart_w = w - pad_x * 2
        chart_h = h - pad_y * 2

        data = list(self._data)
        n = len(data)
        min_v = min(data) * 0.998
        max_v = max(data) * 1.002
        val_range = max_v - min_v if max_v != min_v else 1.0

        def x_pos(i):
            return pad_x + (i / max(n - 1, 1)) * chart_w

        def y_pos(v):
            return pad_y + chart_h - ((v - min_v) / val_range) * chart_h

        # Grid lines and labels
        p.setPen(QPen(QColor(C.BORDER), 1))
        for frac in [0.0, 0.25, 0.5, 0.75, 1.0]:
            gy = pad_y + chart_h * (1.0 - frac)
            p.drawLine(QPointF(pad_x, gy), QPointF(w - pad_x, gy))
            val = min_v + val_range * frac
            p.setPen(QColor(C.TEXT3))
            p.setFont(QFont("Segoe UI", 8))
            p.drawText(
                QRectF(0, gy - 8, pad_x - 6, 16),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                f"${val:,.0f}",
            )
            p.setPen(QPen(QColor(C.BORDER), 1))

        # Drawdown shading -- areas below peak
        peak_so_far = data[0]
        dd_points_top = []
        dd_points_bot = []
        for i, v in enumerate(data):
            peak_so_far = max(peak_so_far, v)
            if v < peak_so_far:
                dd_points_top.append(QPointF(x_pos(i), y_pos(peak_so_far)))
                dd_points_bot.append(QPointF(x_pos(i), y_pos(v)))

        if dd_points_top:
            dd_color = QColor(C.RED)
            dd_color.setAlpha(25)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(dd_color)
            for j in range(len(dd_points_top)):
                t = dd_points_top[j]
                b = dd_points_bot[j]
                rect = QRectF(t.x() - 1, t.y(), 3, b.y() - t.y())
                p.drawRect(rect)

        # Equity line points
        points = [QPointF(x_pos(i), y_pos(v)) for i, v in enumerate(data)]

        # Green fill below line
        fill_poly = QPolygonF()
        fill_poly.append(QPointF(points[0].x(), pad_y + chart_h))
        for pt in points:
            fill_poly.append(pt)
        fill_poly.append(QPointF(points[-1].x(), pad_y + chart_h))

        grad = QLinearGradient(0, pad_y, 0, pad_y + chart_h)
        green_fill = QColor(C.GREEN)
        green_fill.setAlpha(40)
        green_top = QColor(C.GREEN)
        green_top.setAlpha(80)
        grad.setColorAt(0, green_top)
        grad.setColorAt(1, QColor(C.GREEN_BG))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QBrush(grad))
        p.drawPolygon(fill_poly)

        # Equity line
        pen = QPen(QColor(C.GREEN), 2)
        p.setPen(pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        for i in range(len(points) - 1):
            p.drawLine(points[i], points[i + 1])

        # Current value dot
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.GREEN))
        p.drawEllipse(points[-1], 4, 4)

        p.end()


class ModelAccuracyBars(QWidget):
    """Horizontal accuracy bars per model (like gamut bars)."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(100)
        self._models = {}  # {name: accuracy_pct}

    def set_data(self, models: dict):
        self._models = models
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        self.height()
        label_w = 70
        bar_x = label_w + 10
        bar_max_w = w - bar_x - 60
        bar_h = 16
        spacing = 8

        if not self._models:
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No model data")
            p.end()
            return

        y = 8
        for name, acc in self._models.items():
            # Label
            p.setPen(QColor(C.TEXT))
            p.setFont(QFont("Segoe UI", 10))
            p.drawText(
                QRectF(0, y, label_w, bar_h + 4),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                name.title(),
            )

            # Background track
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(C.BORDER))
            track_rect = QRectF(bar_x, y + 2, bar_max_w, bar_h)
            p.drawRoundedRect(track_rect, bar_h / 2, bar_h / 2)

            # Accuracy bar
            pct = max(0, min(acc / 100.0, 1.0))
            if acc >= 60:
                bar_color = C.GREEN
            elif acc >= 40:
                bar_color = C.YELLOW
            else:
                bar_color = C.RED
            p.setBrush(QColor(bar_color))
            fill_rect = QRectF(bar_x, y + 2, bar_max_w * pct, bar_h)
            p.drawRoundedRect(fill_rect, bar_h / 2, bar_h / 2)

            # Percentage label
            p.setPen(QColor(C.TEXT2))
            p.setFont(QFont("Segoe UI", 9))
            p.drawText(
                QRectF(bar_x + bar_max_w + 6, y, 50, bar_h + 4),
                Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                f"{acc:.0f}%",
            )

            y += bar_h + spacing

        p.end()


class DashboardPage(QWidget):
    """The command center -- everything at a glance."""

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

        # Title
        layout.addWidget(Heading("Dashboard"))

        # Row 1: Engine Status + Account
        row1 = QHBoxLayout()
        row1.setSpacing(16)

        # Engine Status card
        card_engine, engine_lay = Card.with_layout()
        engine_header = QHBoxLayout()
        engine_header.setSpacing(8)
        self._engine_dot = StatusDot(C.TEXT3, 12)
        engine_header.addWidget(self._engine_dot)
        self._engine_state_label = QLabel("Stopped")
        self._engine_state_label.setStyleSheet(f"font-size: 14px; font-weight: 600; color: {C.TEXT};")
        engine_header.addWidget(self._engine_state_label)
        engine_header.addStretch()
        engine_lay.addLayout(engine_header)

        engine_details = QGridLayout()
        engine_details.setSpacing(6)
        engine_details.addWidget(QLabel("Strategy:"), 0, 0)
        self._strategy_label = QLabel("Prediction (Ensemble)")
        self._strategy_label.setStyleSheet(f"color: {C.ACCENT_TX}; font-weight: 500;")
        engine_details.addWidget(self._strategy_label, 0, 1)
        engine_details.addWidget(QLabel("Models:"), 1, 0)
        self._models_label = QLabel("ARIMA, Prophet")
        self._models_label.setStyleSheet(f"color: {C.TEXT2};")
        engine_details.addWidget(self._models_label, 1, 1)
        engine_details.addWidget(QLabel("Cycles:"), 2, 0)
        self._cycles_label = QLabel("0")
        self._cycles_label.setStyleSheet(f"color: {C.TEXT2};")
        engine_details.addWidget(self._cycles_label, 2, 1)
        engine_lay.addLayout(engine_details)
        engine_lay.addStretch()

        card_engine.setMinimumWidth(280)
        row1.addWidget(card_engine)

        # Account card
        card_account, account_lay = Card.with_layout()
        account_lay.addWidget(Heading("Account", level=2))

        stats_row = QHBoxLayout()
        stats_row.setSpacing(20)
        self._equity_stat = Stat("Equity", "$100,000", C.TEXT)
        self._cash_stat = Stat("Cash", "$100,000", C.TEXT2)
        self._pnl_stat = Stat("Unrealized P&L", "$0.00", C.TEXT2)
        self._positions_stat = Stat("Positions", "0", C.TEXT2)
        stats_row.addWidget(self._equity_stat)
        stats_row.addWidget(self._cash_stat)
        stats_row.addWidget(self._pnl_stat)
        stats_row.addWidget(self._positions_stat)
        stats_row.addStretch()
        account_lay.addLayout(stats_row)
        account_lay.addStretch()

        row1.addWidget(card_account, stretch=1)
        layout.addLayout(row1)

        # Row 2: Equity Curve
        card_equity, equity_lay = Card.with_layout()
        equity_lay.addWidget(Heading("Equity Curve", level=2))
        self._equity_chart = EquityCurveWidget()
        equity_lay.addWidget(self._equity_chart)
        layout.addWidget(card_equity)

        # Row 3: Recent Trades + Model Accuracy
        row3 = QHBoxLayout()
        row3.setSpacing(16)

        # Recent Trades card
        card_trades, trades_lay = Card.with_layout()
        trades_lay.addWidget(Heading("Recent Trades", level=2))
        self._trades_container = QVBoxLayout()
        self._trades_container.setSpacing(4)
        trades_lay.addLayout(self._trades_container)

        # Placeholder rows
        self._trade_labels = []
        for _ in range(10):
            row_widget = QWidget()
            row_lay = QHBoxLayout(row_widget)
            row_lay.setContentsMargins(0, 2, 0, 2)
            row_lay.setSpacing(8)

            sym = QLabel("--")
            sym.setFixedWidth(70)
            sym.setStyleSheet(f"font-weight: 600; color: {C.TEXT};")
            side = QLabel("--")
            side.setFixedWidth(36)
            qty = QLabel("--")
            qty.setFixedWidth(40)
            qty.setStyleSheet(f"color: {C.TEXT2}; font-size: 11px;")
            price = QLabel("--")
            price.setFixedWidth(70)
            price.setStyleSheet(f"color: {C.TEXT2}; font-size: 11px;")
            pnl = QLabel("--")
            pnl.setFixedWidth(70)
            pnl.setAlignment(Qt.AlignmentFlag.AlignRight)

            row_lay.addWidget(sym)
            row_lay.addWidget(side)
            row_lay.addWidget(qty)
            row_lay.addWidget(price)
            row_lay.addStretch()
            row_lay.addWidget(pnl)

            self._trades_container.addWidget(row_widget)
            self._trade_labels.append((sym, side, qty, price, pnl))

        trades_lay.addStretch()
        card_trades.setMinimumWidth(360)
        row3.addWidget(card_trades, stretch=1)

        # Model Accuracy card
        card_acc, acc_lay = Card.with_layout()
        acc_lay.addWidget(Heading("Model Accuracy", level=2))
        self._accuracy_bars = ModelAccuracyBars()
        acc_lay.addWidget(self._accuracy_bars)
        acc_lay.addStretch()
        card_acc.setMinimumWidth(300)
        row3.addWidget(card_acc, stretch=1)

        layout.addLayout(row3)

        # Quick Actions
        card_actions, actions_lay = Card.with_layout(margins=(20, 12, 20, 12))
        actions_header = QHBoxLayout()
        actions_header.setSpacing(12)
        actions_header.addWidget(Heading("Quick Actions", level=3))
        actions_header.addStretch()

        btn_start = QPushButton("Start Engine")
        btn_start.setProperty("primary", True)
        btn_start.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_start.clicked.connect(self._quick_start_engine)
        actions_header.addWidget(btn_start)

        btn_backtest = QPushButton("Run Backtest")
        btn_backtest.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_backtest.clicked.connect(self._quick_run_backtest)
        actions_header.addWidget(btn_backtest)

        btn_fetch = QPushButton("Fetch Data")
        btn_fetch.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_fetch.clicked.connect(self._quick_fetch_data)
        actions_header.addWidget(btn_fetch)

        actions_lay.addLayout(actions_header)
        layout.addWidget(card_actions)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _load_demo_data(self):
        """Load demo data for initial display."""
        # Equity curve demo data
        base = 100_000
        equity_data = [base]
        for _i in range(99):
            change = random.gauss(0.001, 0.008)
            base = base * (1 + change)
            equity_data.append(base)
        self._equity_chart.set_data(equity_data)

        # Update account stats from last equity value
        final_equity = equity_data[-1]
        pnl = final_equity - 100_000
        pnl_color = C.GREEN if pnl >= 0 else C.RED
        self._equity_stat.set_value(f"${final_equity:,.0f}", C.TEXT)
        self._cash_stat.set_value(f"${final_equity * 0.6:,.0f}", C.TEXT2)
        self._pnl_stat.set_value(
            f"{'+' if pnl >= 0 else ''}${pnl:,.0f}",
            pnl_color,
        )
        self._positions_stat.set_value("3", C.TEXT2)

        # Model accuracy demo
        self._accuracy_bars.set_data(
            {
                "arima": 62,
                "prophet": 55,
                "neural": 48,
            }
        )

        # Demo trades
        demo_trades = [
            ("AAPL", "BUY", "50", "$178.23", "+$342"),
            ("BTC-USD", "SELL", "0.5", "$67,420", "-$128"),
            ("ETH-USD", "BUY", "3.0", "$3,456", "+$87"),
            ("AAPL", "SELL", "25", "$181.45", "+$215"),
            ("BTC-USD", "BUY", "0.2", "$66,890", "+$94"),
            ("ETH-USD", "SELL", "2.0", "$3,512", "+$112"),
            ("AAPL", "BUY", "30", "$176.90", "-$45"),
            ("BTC-USD", "SELL", "0.3", "$68,100", "+$203"),
            ("ETH-USD", "BUY", "1.5", "$3,390", "+$67"),
            ("AAPL", "SELL", "40", "$179.80", "+$156"),
        ]
        for i, (sym, side, qty, price, pnl) in enumerate(demo_trades):
            if i >= len(self._trade_labels):
                break
            labels = self._trade_labels[i]
            labels[0].setText(sym)
            side_color = C.GREEN if side == "BUY" else C.RED
            labels[1].setText(side)
            labels[1].setStyleSheet(f"color: {side_color}; font-weight: 600; font-size: 11px;")
            labels[2].setText(qty)
            labels[3].setText(price)
            pnl_color = C.GREEN if pnl.startswith("+") else C.RED
            labels[4].setText(pnl)
            labels[4].setStyleSheet(f"color: {pnl_color}; font-weight: 600; font-size: 12px;")

    def _refresh_data(self):
        """Refresh data from engine if available."""
        pass  # Will connect to live engine data

    def _quick_start_engine(self):
        if self._main_window:
            self._main_window._shortcut_switch_page(2)
            page = self._main_window.stack.widget(2)
            if hasattr(page, "start_engine"):
                page.start_engine()

    def _quick_run_backtest(self):
        if self._main_window:
            self._main_window._shortcut_switch_page(4)

    def _quick_fetch_data(self):
        if self._main_window:
            self._main_window._shortcut_switch_page(5)

    def update_from_engine(self, status: dict):
        """Update dashboard from engine status dict."""
        running = status.get("running", False)
        if running:
            self._engine_dot.set_color(C.GREEN)
            self._engine_state_label.setText("Running")
        else:
            self._engine_dot.set_color(C.TEXT3)
            self._engine_state_label.setText("Stopped")

        self._cycles_label.setText(str(status.get("cycles", 0)))

        equity = status.get("equity", 0)
        self._equity_stat.set_value(f"${equity:,.0f}", C.TEXT)
        self._equity_chart.add_point(equity)

        accuracy = status.get("accuracy", 0)
        weights = status.get("model_weights", {})
        if weights:
            acc_data = {}
            for model_name in weights:
                acc_data[model_name] = accuracy * 100
            self._accuracy_bars.set_data(acc_data)

    def refresh(self):
        self._refresh_data()
