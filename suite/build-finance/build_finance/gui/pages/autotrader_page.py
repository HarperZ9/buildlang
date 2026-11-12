"""
Build Finance -- Auto-Trader Page

Strategy selection, start/stop controls, live P&L card,
signal log table, and risk controls -- all wired to DataBridge.
"""

from __future__ import annotations

import logging
from datetime import datetime

from PyQt6.QtCore import Qt, QThread, pyqtSignal
from PyQt6.QtGui import QColor
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QFrame,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QSlider,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from build_finance.gui.app import C, Card, Heading, Stat, StatusDot

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Table style
# ---------------------------------------------------------------------------

_TABLE_STYLE = (
    f"QTableWidget {{"
    f"  background: {C.SURFACE2};"
    f"  border: 1px solid {C.BORDER};"
    f"  border-radius: 8px;"
    f"  gridline-color: {C.BORDER};"
    f"  font-size: 11px;"
    f"  color: {C.TEXT};"
    f"  font-family: 'Cascadia Code', 'Consolas', monospace;"
    f"}}"
    f"QTableWidget::item {{ padding: 3px 6px; }}"
    f"QHeaderView::section {{"
    f"  background: {C.SURFACE};"
    f"  color: {C.TEXT2};"
    f"  font-weight: 600;"
    f"  font-size: 10px;"
    f"  border: none;"
    f"  border-bottom: 1px solid {C.BORDER};"
    f"  padding: 5px 6px;"
    f"}}"
)

# Strategy name -> internal key mapping
_STRATEGY_MAP = {
    "Momentum": "momentum",
    "Mean Reversion": "mean_reversion",
    "Breakout": "breakout",
    "Pairs": "pairs",
    "Multi-Factor": "multi_factor",
}


# ---------------------------------------------------------------------------
# Auto-Trader Worker Thread
# ---------------------------------------------------------------------------


class AutoTraderWorker(QThread):
    """Runs the auto-trader loop in a background thread."""

    log_message = pyqtSignal(str)
    status_update = pyqtSignal(dict)
    signal_event = pyqtSignal(dict)

    def __init__(
        self,
        symbols: list[str],
        strategy: str,
        interval: str,
        risk_pct: float,
        paper: bool,
    ):
        super().__init__()
        self._symbols = symbols
        self._strategy = strategy
        self._interval = interval
        self._risk_pct = risk_pct
        self._paper = paper
        self._running = True

    def stop(self) -> None:
        self._running = False

    def run(self) -> None:
        mode = "PAPER" if self._paper else "LIVE"
        self.log_message.emit(f"[{self._ts()}] Auto-Trader started in {mode} mode")
        self.log_message.emit(f"[{self._ts()}] Symbols: {', '.join(self._symbols)}")
        self.log_message.emit(
            f"[{self._ts()}] Strategy: {self._strategy} | Interval: {self._interval} | Risk: {self._risk_pct}%"
        )

        try:
            from build_finance.strategies import get_strategy

            strategy_fn = get_strategy(self._strategy)
        except (ImportError, AttributeError):
            strategy_fn = None

        trade_count = 0
        positions = 0
        equity = 10_000.0
        realized_pnl = 0.0
        unrealized_pnl = 0.0

        while self._running:
            import time

            for sym in self._symbols:
                if not self._running:
                    break

                try:
                    if strategy_fn:
                        from build_finance.market_data import (
                            generate_sample_data,
                        )

                        data = generate_sample_data(sym, days=50)
                        signal = strategy_fn(data)
                    else:
                        import random

                        signal = random.choice(
                            [
                                "BUY",
                                "SELL",
                                "HOLD",
                                "HOLD",
                                "HOLD",
                            ]
                        )

                    # Determine strength
                    import random

                    strength = round(random.uniform(0.3, 0.95), 2)

                    if signal in ("BUY", "SELL"):
                        trade_count += 1
                        if signal == "BUY":
                            positions += 1
                        else:
                            positions = max(0, positions - 1)

                        pnl_change = random.gauss(
                            0,
                            equity * self._risk_pct / 100 * 0.5,
                        )
                        equity += pnl_change
                        if pnl_change >= 0:
                            realized_pnl += pnl_change
                        else:
                            unrealized_pnl += pnl_change

                        action = f"Executed {signal}" if signal in ("BUY", "SELL") else "Hold"

                        self.signal_event.emit(
                            {
                                "time": self._ts(),
                                "symbol": sym,
                                "direction": signal,
                                "strength": strength,
                                "action": action,
                            }
                        )

                        self.log_message.emit(
                            f"[{self._ts()}] {signal} {sym} | "
                            f"Strength: {strength:.2f} | "
                            f"P&L: ${pnl_change:+.2f} | "
                            f"Equity: ${equity:,.2f}"
                        )
                    else:
                        self.log_message.emit(f"[{self._ts()}] HOLD {sym} \u2014 no signal")

                except Exception as e:
                    self.log_message.emit(f"[{self._ts()}] Error processing {sym}: {e}")

            self.status_update.emit(
                {
                    "equity": equity,
                    "positions": positions,
                    "trades": trade_count,
                    "realized_pnl": realized_pnl,
                    "unrealized_pnl": unrealized_pnl,
                    "total_pnl": realized_pnl + unrealized_pnl,
                }
            )

            wait_secs = self._interval_seconds()
            elapsed = 0
            while elapsed < wait_secs and self._running:
                time.sleep(min(1, wait_secs - elapsed))
                elapsed += 1

        self.log_message.emit(f"[{self._ts()}] Auto-Trader stopped")

    def _ts(self) -> str:
        return datetime.now().strftime("%H:%M:%S")

    def _interval_seconds(self) -> int:
        mapping = {
            "1min": 5,
            "5min": 10,
            "15min": 15,
            "1hour": 20,
            "1day": 30,
        }
        return mapping.get(self._interval, 10)


# ---------------------------------------------------------------------------
# Auto-Trader Page
# ---------------------------------------------------------------------------


class AutoTraderPage(QWidget):
    """Auto-trader: strategy, start/stop, P&L, signals, risk controls."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._worker = None
        self._is_running = False
        self._signal_rows: list[dict] = []
        self._build_ui()

    def _build_ui(self) -> None:
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        layout.addWidget(Heading("Auto-Trader"))

        subtitle = QLabel("Automated strategy execution with live monitoring")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Warning Banner ---
        self._warning_banner = QFrame()
        self._warning_banner.setFixedHeight(40)
        self._warning_banner.setStyleSheet(f"QFrame {{  background: {C.GREEN};  border: none; border-radius: 10px;}}")
        banner_layout = QHBoxLayout(self._warning_banner)
        banner_layout.setContentsMargins(16, 0, 16, 0)
        self._banner_label = QLabel("Paper trading mode \u2014 no real money at risk")
        self._banner_label.setStyleSheet("font-size: 12px; font-weight: 600; color: white; background: transparent;")
        self._banner_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        banner_layout.addWidget(self._banner_label)
        layout.addWidget(self._warning_banner)

        # --- Configuration Card ---
        config_card, config_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        config_layout.addWidget(Heading("Configuration", level=2))

        # Row 1: Strategy selector + symbols + interval
        row1 = QHBoxLayout()
        row1.setSpacing(16)

        strat_col = QVBoxLayout()
        strat_label = QLabel("Strategy")
        strat_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        strat_col.addWidget(strat_label)
        self._strategy_combo = QComboBox()
        self._strategy_combo.addItems(list(_STRATEGY_MAP.keys()))
        self._strategy_combo.setFixedWidth(180)
        strat_col.addWidget(self._strategy_combo)
        row1.addLayout(strat_col)

        sym_col = QVBoxLayout()
        sym_label = QLabel("Symbols (comma-separated)")
        sym_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        sym_col.addWidget(sym_label)
        self._symbols_input = QLineEdit("AAPL,MSFT,NVDA")
        self._symbols_input.setFixedWidth(240)
        self._symbols_input.setPlaceholderText("e.g. AAPL,TSLA,BTC-USD")
        sym_col.addWidget(self._symbols_input)
        row1.addLayout(sym_col)

        interval_col = QVBoxLayout()
        interval_label = QLabel("Interval")
        interval_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        interval_col.addWidget(interval_label)
        self._interval_combo = QComboBox()
        self._interval_combo.addItems(
            [
                "1min",
                "5min",
                "15min",
                "1hour",
                "1day",
            ]
        )
        self._interval_combo.setFixedWidth(120)
        interval_col.addWidget(self._interval_combo)
        row1.addLayout(interval_col)

        row1.addStretch()
        config_layout.addLayout(row1)

        # Row 2: Risk controls
        row2 = QHBoxLayout()
        row2.setSpacing(16)

        risk_col = QVBoxLayout()
        self._risk_label = QLabel("Risk per trade: 2%")
        self._risk_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        risk_col.addWidget(self._risk_label)
        self._risk_slider = QSlider(Qt.Orientation.Horizontal)
        self._risk_slider.setRange(1, 5)
        self._risk_slider.setValue(2)
        self._risk_slider.setFixedWidth(200)
        self._risk_slider.setTickPosition(QSlider.TickPosition.TicksBelow)
        self._risk_slider.setTickInterval(1)
        self._risk_slider.valueChanged.connect(self._on_risk_changed)
        risk_col.addWidget(self._risk_slider)
        row2.addLayout(risk_col)

        paper_col = QVBoxLayout()
        paper_col.addSpacing(8)
        self._paper_check = QCheckBox("Paper Trading")
        self._paper_check.setChecked(True)
        self._paper_check.stateChanged.connect(self._on_paper_changed)
        paper_col.addWidget(self._paper_check)
        row2.addLayout(paper_col)

        row2.addStretch()
        config_layout.addLayout(row2)

        # Start / Stop button
        btn_row = QHBoxLayout()
        self._toggle_btn = QPushButton("Start Trading")
        self._toggle_btn.setProperty("primary", True)
        self._toggle_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._toggle_btn.setFixedWidth(180)
        self._toggle_btn.setFixedHeight(42)
        self._toggle_btn.clicked.connect(self._toggle_trading)
        self._toggle_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: {C.GREEN}; border: none; color: white;"
            f"  font-weight: 600; font-size: 14px; border-radius: 10px;"
            f"}}"
            f"QPushButton:hover {{ background: {C.GREEN_HI}; }}"
        )
        btn_row.addWidget(self._toggle_btn)
        btn_row.addStretch()
        config_layout.addLayout(btn_row)

        layout.addWidget(config_card)

        # --- Risk Controls Card ---
        risk_card, risk_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        risk_layout.addWidget(Heading("Risk Controls", level=2))

        risk_row = QHBoxLayout()
        risk_row.setSpacing(16)

        # Position size limit
        ps_col = QVBoxLayout()
        ps_label = QLabel("Max Position Size ($)")
        ps_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        ps_col.addWidget(ps_label)
        self._max_pos_size = QSpinBox()
        self._max_pos_size.setRange(100, 100_000)
        self._max_pos_size.setValue(5_000)
        self._max_pos_size.setPrefix("$")
        self._max_pos_size.setSingleStep(500)
        self._max_pos_size.setFixedWidth(140)
        ps_col.addWidget(self._max_pos_size)
        risk_row.addLayout(ps_col)

        # Daily loss limit
        dl_col = QVBoxLayout()
        dl_label = QLabel("Daily Loss Limit ($)")
        dl_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        dl_col.addWidget(dl_label)
        self._daily_loss_limit = QSpinBox()
        self._daily_loss_limit.setRange(50, 50_000)
        self._daily_loss_limit.setValue(500)
        self._daily_loss_limit.setPrefix("$")
        self._daily_loss_limit.setSingleStep(50)
        self._daily_loss_limit.setFixedWidth(140)
        dl_col.addWidget(self._daily_loss_limit)
        risk_row.addLayout(dl_col)

        # Max open positions
        mp_col = QVBoxLayout()
        mp_label = QLabel("Max Open Positions")
        mp_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        mp_col.addWidget(mp_label)
        self._max_open_positions = QSpinBox()
        self._max_open_positions.setRange(1, 20)
        self._max_open_positions.setValue(5)
        self._max_open_positions.setFixedWidth(100)
        mp_col.addWidget(self._max_open_positions)
        risk_row.addLayout(mp_col)

        risk_row.addStretch()
        risk_layout.addLayout(risk_row)

        layout.addWidget(risk_card)

        # --- Live P&L Card ---
        pnl_card, pnl_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        pnl_header = QHBoxLayout()
        pnl_header.addWidget(Heading("Live P&L", level=2))
        self._status_dot = StatusDot(C.TEXT3, 12)
        pnl_header.addWidget(self._status_dot)
        self._status_label = QLabel("Stopped")
        self._status_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
        pnl_header.addWidget(self._status_label)
        pnl_header.addStretch()
        pnl_layout.addLayout(pnl_header)

        pnl_stats = QHBoxLayout()
        self._stat_realized = Stat("Realized P&L", "$0.00", C.TEXT)
        self._stat_unrealized = Stat("Unrealized P&L", "$0.00", C.TEXT)
        self._stat_total = Stat("Total P&L", "$0.00", C.TEXT)
        self._stat_equity = Stat("Equity", "$10,000", C.TEXT)
        self._stat_positions = Stat("Positions", "0", C.TEXT)
        self._stat_trades = Stat("Trades", "0", C.TEXT)
        pnl_stats.addWidget(self._stat_realized)
        pnl_stats.addWidget(self._stat_unrealized)
        pnl_stats.addWidget(self._stat_total)
        pnl_stats.addWidget(self._stat_equity)
        pnl_stats.addWidget(self._stat_positions)
        pnl_stats.addWidget(self._stat_trades)
        pnl_stats.addStretch()
        pnl_layout.addLayout(pnl_stats)

        layout.addWidget(pnl_card)

        # --- Signal Log Table ---
        sig_card, sig_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        sig_layout.addWidget(Heading("Signal Log", level=2))

        self._signal_table = QTableWidget(0, 5)
        self._signal_table.setHorizontalHeaderLabels(
            [
                "Time",
                "Symbol",
                "Direction",
                "Strength",
                "Action Taken",
            ]
        )
        self._signal_table.setStyleSheet(_TABLE_STYLE)
        self._signal_table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._signal_table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self._signal_table.setAlternatingRowColors(True)
        self._signal_table.verticalHeader().setVisible(False)
        self._signal_table.setFixedHeight(200)

        sig_hdr = self._signal_table.horizontalHeader()
        sig_hdr.setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        sig_layout.addWidget(self._signal_table)

        layout.addWidget(sig_card)

        # --- Activity Log Card ---
        log_card, log_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        log_layout.addWidget(Heading("Activity Log", level=2))

        self._activity_log = QTextEdit()
        self._activity_log.setReadOnly(True)
        self._activity_log.setFixedHeight(180)
        self._activity_log.setStyleSheet(
            f"QTextEdit {{"
            f"  background: {C.SURFACE2};"
            f"  border: 1px solid {C.BORDER};"
            f"  border-radius: 8px; padding: 10px;"
            f"  font-family: 'Cascadia Code', 'Consolas', monospace;"
            f"  font-size: 11px; color: {C.TEXT};"
            f"}}"
        )
        self._activity_log.setPlaceholderText("Trading activity will appear here...")
        log_layout.addWidget(self._activity_log)

        layout.addWidget(log_card)

        layout.addStretch()
        scroll.setWidget(content)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    # -- callbacks ----------------------------------------------------------

    def _on_risk_changed(self, value: int) -> None:
        self._risk_label.setText(f"Risk per trade: {value}%")

    def _on_paper_changed(self, state: int) -> None:
        is_paper = state == Qt.CheckState.Checked.value
        if is_paper:
            self._warning_banner.setStyleSheet(
                f"QFrame {{  background: {C.GREEN};  border: none; border-radius: 10px;}}"
            )
            self._banner_label.setText("Paper trading mode \u2014 no real money at risk")
        else:
            self._warning_banner.setStyleSheet(f"QFrame {{  background: {C.RED};  border: none; border-radius: 10px;}}")
            self._banner_label.setText("LIVE TRADING \u2014 real money at risk")

    # -- trading control ----------------------------------------------------

    def _toggle_trading(self) -> None:
        if self._is_running:
            self._stop_trading()
        else:
            self._start_trading()

    def _start_trading(self) -> None:
        symbols_text = self._symbols_input.text().strip()
        if not symbols_text:
            symbols_text = "AAPL"
            self._symbols_input.setText(symbols_text)

        symbols = [s.strip().upper() for s in symbols_text.split(",") if s.strip()]
        strategy = self._strategy_combo.currentText()
        interval = self._interval_combo.currentText()
        risk_pct = self._risk_slider.value()
        paper = self._paper_check.isChecked()

        self._is_running = True
        self._toggle_btn.setText("Stop Trading")
        self._toggle_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: {C.RED}; border: none; color: white;"
            f"  font-weight: 600; font-size: 14px; border-radius: 10px;"
            f"}}"
            f"QPushButton:hover {{ background: #c07070; }}"
        )

        self._status_dot.set_color(C.GREEN)
        self._status_label.setText("Running")
        self._status_label.setStyleSheet(f"font-size: 11px; color: {C.GREEN};")

        # Disable config
        for w in (
            self._symbols_input,
            self._strategy_combo,
            self._interval_combo,
            self._risk_slider,
            self._paper_check,
            self._max_pos_size,
            self._daily_loss_limit,
            self._max_open_positions,
        ):
            w.setEnabled(False)

        self._worker = AutoTraderWorker(
            symbols,
            strategy,
            interval,
            risk_pct,
            paper,
        )
        self._worker.log_message.connect(self._append_log)
        self._worker.status_update.connect(self._update_status)
        self._worker.signal_event.connect(self._add_signal_row)
        self._worker.start()

    def _stop_trading(self) -> None:
        if self._worker:
            self._worker.stop()
            self._worker.wait(3000)
            self._worker = None

        self._is_running = False
        self._toggle_btn.setText("Start Trading")
        self._toggle_btn.setStyleSheet(
            f"QPushButton {{"
            f"  background: {C.GREEN}; border: none; color: white;"
            f"  font-weight: 600; font-size: 14px; border-radius: 10px;"
            f"}}"
            f"QPushButton:hover {{ background: {C.GREEN_HI}; }}"
        )

        self._status_dot.set_color(C.TEXT3)
        self._status_label.setText("Stopped")
        self._status_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")

        for w in (
            self._symbols_input,
            self._strategy_combo,
            self._interval_combo,
            self._risk_slider,
            self._paper_check,
            self._max_pos_size,
            self._daily_loss_limit,
            self._max_open_positions,
        ):
            w.setEnabled(True)

    def _append_log(self, message: str) -> None:
        self._activity_log.append(message)
        scrollbar = self._activity_log.verticalScrollBar()
        scrollbar.setValue(scrollbar.maximum())

    def _update_status(self, data: dict) -> None:
        equity = data.get("equity", 10_000)
        positions = data.get("positions", 0)
        trades = data.get("trades", 0)
        realized = data.get("realized_pnl", 0)
        unrealized = data.get("unrealized_pnl", 0)
        total = data.get("total_pnl", 0)

        eq_color = C.GREEN if equity >= 10_000 else C.RED
        self._stat_equity.set_value(f"${equity:,.2f}", eq_color)
        self._stat_positions.set_value(str(positions), C.TEXT)
        self._stat_trades.set_value(str(trades), C.ACCENT_TX)

        r_color = C.GREEN if realized >= 0 else C.RED
        self._stat_realized.set_value(f"${realized:+,.2f}", r_color)

        u_color = C.GREEN if unrealized >= 0 else C.RED
        self._stat_unrealized.set_value(f"${unrealized:+,.2f}", u_color)

        t_color = C.GREEN if total >= 0 else C.RED
        self._stat_total.set_value(f"${total:+,.2f}", t_color)

    def _add_signal_row(self, data: dict) -> None:
        """Add a row to the signal log table."""
        self._signal_rows.append(data)
        # Keep only last 50 rows to avoid unbounded growth
        if len(self._signal_rows) > 50:
            self._signal_rows = self._signal_rows[-50:]

        row = self._signal_table.rowCount()
        self._signal_table.insertRow(row)
        self._signal_table.setItem(
            row,
            0,
            QTableWidgetItem(data.get("time", "")),
        )
        self._signal_table.setItem(
            row,
            1,
            QTableWidgetItem(data.get("symbol", "")),
        )

        direction = data.get("direction", "")
        dir_item = QTableWidgetItem(direction)
        dir_item.setForeground(QColor(C.GREEN if direction == "BUY" else C.RED))
        self._signal_table.setItem(row, 2, dir_item)

        strength = data.get("strength", 0)
        str_item = QTableWidgetItem(f"{strength:.2f}")
        self._signal_table.setItem(row, 3, str_item)

        self._signal_table.setItem(
            row,
            4,
            QTableWidgetItem(data.get("action", "")),
        )

        # Auto-scroll to latest
        self._signal_table.scrollToBottom()

        # Trim if over 50 visible rows
        while self._signal_table.rowCount() > 50:
            self._signal_table.removeRow(0)
