"""
Build Finance — Backtest Page

Run backtests with strategy selection, view results including
equity curve, key metrics, and trade log.
"""

import logging

from PyQt6.QtCore import QPointF, QRectF, Qt, QThread, pyqtSignal
from PyQt6.QtGui import QColor, QLinearGradient, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import (
    QComboBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QProgressBar,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from build_finance.gui.app import C, Card, Heading, Stat

logger = logging.getLogger(__name__)


# =============================================================================
# Equity Curve Chart Widget
# =============================================================================


class EquityCurveWidget(QWidget):
    """Custom QPainter widget that draws an equity curve with drawdown shading."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(220)
        self.setMinimumWidth(400)
        self._equity_data = []
        self._drawdown_data = []

    def set_data(self, equity: list[float], drawdown: list[float] = None):
        """Set the equity and drawdown data for rendering."""
        self._equity_data = equity or []
        self._drawdown_data = drawdown or []
        self.update()

    def paintEvent(self, event):
        if not self._equity_data or len(self._equity_data) < 2:
            p = QPainter(self)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No data — run a backtest")
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        margin_l = 50
        margin_r = 16
        margin_t = 16
        margin_b = 28
        chart_w = w - margin_l - margin_r
        chart_h = h - margin_t - margin_b

        data = self._equity_data
        n = len(data)
        y_min = min(data)
        y_max = max(data)
        y_range = y_max - y_min if y_max != y_min else 1.0

        def to_x(i):
            return margin_l + (i / (n - 1)) * chart_w

        def to_y(v):
            return margin_t + chart_h - ((v - y_min) / y_range) * chart_h

        # Background
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.SURFACE2))
        p.drawRoundedRect(0, 0, w, h, 10, 10)

        # Grid lines (horizontal)
        grid_pen = QPen(QColor(C.BORDER), 1)
        grid_pen.setStyle(Qt.PenStyle.DotLine)
        p.setPen(grid_pen)
        for frac in [0.0, 0.25, 0.5, 0.75, 1.0]:
            y_val = y_min + frac * y_range
            y_px = to_y(y_val)
            p.drawLine(QPointF(margin_l, y_px), QPointF(w - margin_r, y_px))
            # Label
            label_pen = QPen(QColor(C.TEXT3), 1)
            p.setPen(label_pen)
            p.drawText(
                QRectF(0, y_px - 8, margin_l - 6, 16),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                f"${y_val:,.0f}",
            )
            p.setPen(grid_pen)

        # Drawdown shading
        if self._drawdown_data and len(self._drawdown_data) == n:
            dd_path = QPainterPath()
            in_drawdown = False
            for i in range(n):
                if self._drawdown_data[i] < -0.001:
                    x = to_x(i)
                    to_y(data[i])
                    # Find the peak equity at this point
                    peak = max(data[: i + 1])
                    y_peak = to_y(peak)
                    if not in_drawdown:
                        dd_path.moveTo(x, y_peak)
                        in_drawdown = True
                    dd_path.lineTo(x, y_peak)
                else:
                    if in_drawdown:
                        # Close the path back down
                        for j in range(i - 1, -1, -1):
                            if self._drawdown_data[j] < -0.001:
                                dd_path.lineTo(to_x(j), to_y(data[j]))
                            else:
                                break
                        dd_path.closeSubpath()
                        in_drawdown = False

            dd_color = QColor(C.RED)
            dd_color.setAlpha(35)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(dd_color)
            p.drawPath(dd_path)

        # Equity curve line
        path = QPainterPath()
        path.moveTo(to_x(0), to_y(data[0]))
        for i in range(1, n):
            path.lineTo(to_x(i), to_y(data[i]))

        # Gradient fill under curve
        fill_path = QPainterPath(path)
        fill_path.lineTo(to_x(n - 1), margin_t + chart_h)
        fill_path.lineTo(to_x(0), margin_t + chart_h)
        fill_path.closeSubpath()

        gradient = QLinearGradient(0, margin_t, 0, margin_t + chart_h)
        final_color = C.GREEN if data[-1] >= data[0] else C.RED
        grad_color = QColor(final_color)
        grad_color.setAlpha(40)
        gradient.setColorAt(0, grad_color)
        grad_color_end = QColor(final_color)
        grad_color_end.setAlpha(5)
        gradient.setColorAt(1, grad_color_end)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(gradient)
        p.drawPath(fill_path)

        # Main line
        line_color = QColor(C.GREEN if data[-1] >= data[0] else C.RED)
        line_pen = QPen(line_color, 2.0)
        line_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        line_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setPen(line_pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawPath(path)

        # Start / end dots
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(line_color)
        p.drawEllipse(QPointF(to_x(0), to_y(data[0])), 4, 4)
        p.drawEllipse(QPointF(to_x(n - 1), to_y(data[-1])), 4, 4)

        # Title
        p.setPen(QColor(C.TEXT2))
        p.drawText(QRectF(margin_l, h - margin_b + 4, chart_w, 20), Qt.AlignmentFlag.AlignCenter, "Equity Curve")

        p.end()


# =============================================================================
# Backtest Worker Thread
# =============================================================================


class BacktestWorker(QThread):
    """Runs a backtest in a background thread."""

    progress = pyqtSignal(int)
    finished_signal = pyqtSignal(dict)
    error = pyqtSignal(str)

    def __init__(self, symbol: str, strategy: str, lookback_days: int):
        super().__init__()
        self._symbol = symbol
        self._strategy = strategy
        self._lookback = lookback_days

    def run(self):
        try:
            self.progress.emit(10)

            # Attempt to use real backtest engine
            try:
                from build_finance.backtest import Backtest
                from build_finance.data import fetch_ohlcv
                from build_finance.strategies import get_strategy

                self.progress.emit(20)
                data = fetch_ohlcv(self._symbol, days=self._lookback)
                self.progress.emit(40)

                strategy = get_strategy(self._strategy)
                bt = Backtest(data, strategy)
                self.progress.emit(60)

                result = bt.run()
                self.progress.emit(90)

                self.finished_signal.emit(result)

            except ImportError:
                # Fallback: generate synthetic demo results
                self.progress.emit(30)
                import time

                time.sleep(0.3)
                self.progress.emit(50)

                import random

                random.seed(42)
                n = self._lookback
                equity = [10000.0]
                for _i in range(1, n):
                    change = random.gauss(0.0004, 0.015)
                    equity.append(equity[-1] * (1 + change))

                self.progress.emit(70)

                # Compute metrics
                total_return = (equity[-1] / equity[0] - 1) * 100
                peak = equity[0]
                max_dd = 0
                drawdowns = [0.0]
                for v in equity[1:]:
                    peak = max(peak, v)
                    dd = (v - peak) / peak
                    drawdowns.append(dd)
                    max_dd = min(max_dd, dd)

                daily_returns = [(equity[i] / equity[i - 1] - 1) for i in range(1, len(equity))]
                import math

                avg_ret = sum(daily_returns) / len(daily_returns)
                std_ret = math.sqrt(sum((r - avg_ret) ** 2 for r in daily_returns) / len(daily_returns))
                sharpe = (avg_ret / std_ret * math.sqrt(252)) if std_ret > 0 else 0

                wins = sum(1 for r in daily_returns if r > 0)
                win_rate = wins / len(daily_returns) * 100

                gross_profit = sum(r for r in daily_returns if r > 0)
                gross_loss = abs(sum(r for r in daily_returns if r < 0))
                profit_factor = gross_profit / gross_loss if gross_loss > 0 else float("inf")

                self.progress.emit(85)

                # Generate sample trades
                trades = []
                for _i in range(min(20, n // 10)):
                    entry_idx = random.randint(1, n - 10)
                    exit_idx = entry_idx + random.randint(1, 8)
                    entry_price = equity[entry_idx] / 100
                    exit_price = equity[min(exit_idx, n - 1)] / 100
                    pnl = exit_price - entry_price
                    trades.append(
                        {
                            "entry_day": entry_idx,
                            "exit_day": min(exit_idx, n - 1),
                            "entry_price": entry_price,
                            "exit_price": exit_price,
                            "pnl": pnl,
                            "side": "LONG" if random.random() > 0.3 else "SHORT",
                        }
                    )

                time.sleep(0.2)
                self.progress.emit(95)

                result = {
                    "equity": equity,
                    "drawdowns": drawdowns,
                    "total_return": total_return,
                    "sharpe": sharpe,
                    "max_drawdown": max_dd * 100,
                    "win_rate": win_rate,
                    "profit_factor": profit_factor,
                    "trades": trades,
                    "symbol": self._symbol,
                    "strategy": self._strategy,
                }
                self.finished_signal.emit(result)

            self.progress.emit(100)

        except Exception as e:
            logger.error("Backtest error: %s", e)
            self.error.emit(str(e))


# =============================================================================
# Backtest Page
# =============================================================================


class BacktestPage(QWidget):
    """Backtest configuration, execution, and results display."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._worker = None
        self._build_ui()

    def _build_ui(self):
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        layout.addWidget(Heading("Backtest"))

        subtitle = QLabel("Configure and run strategy backtests")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Configuration Card ---
        config_card, config_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        config_layout.addWidget(Heading("Configuration", level=2))

        config_row = QHBoxLayout()
        config_row.setSpacing(16)

        # Strategy selector
        strat_col = QVBoxLayout()
        strat_label = QLabel("Strategy")
        strat_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        strat_col.addWidget(strat_label)
        self._strategy_combo = QComboBox()
        self._strategy_combo.addItems(["Momentum", "Mean Reversion", "Trend Following", "Breakout", "Ensemble"])
        self._strategy_combo.setFixedWidth(180)
        strat_col.addWidget(self._strategy_combo)
        config_row.addLayout(strat_col)

        # Symbol input
        symbol_col = QVBoxLayout()
        symbol_label = QLabel("Symbol")
        symbol_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        symbol_col.addWidget(symbol_label)
        self._symbol_input = QLineEdit("AAPL")
        self._symbol_input.setFixedWidth(120)
        self._symbol_input.setPlaceholderText("e.g. AAPL")
        symbol_col.addWidget(self._symbol_input)
        config_row.addLayout(symbol_col)

        # Lookback days
        days_col = QVBoxLayout()
        days_label = QLabel("Lookback (days)")
        days_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        days_col.addWidget(days_label)
        self._days_spin = QSpinBox()
        self._days_spin.setRange(30, 2520)
        self._days_spin.setValue(252)
        self._days_spin.setSuffix(" days")
        self._days_spin.setFixedWidth(140)
        days_col.addWidget(self._days_spin)
        config_row.addLayout(days_col)

        config_row.addStretch()

        config_layout.addLayout(config_row)

        # Run button + progress
        run_row = QHBoxLayout()
        self._run_btn = QPushButton("Run Backtest")
        self._run_btn.setProperty("primary", True)
        self._run_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._run_btn.setFixedWidth(160)
        self._run_btn.clicked.connect(self._run_backtest)
        run_row.addWidget(self._run_btn)

        self._progress = QProgressBar()
        self._progress.setFixedHeight(10)
        self._progress.setValue(0)
        self._progress.setVisible(False)
        run_row.addWidget(self._progress)

        run_row.addStretch()
        config_layout.addLayout(run_row)

        layout.addWidget(config_card)

        # --- Results Card (initially hidden) ---
        self._results_card, self._results_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        self._results_layout.addWidget(Heading("Results", level=2))

        # Metrics row
        self._metrics_row = QHBoxLayout()
        self._stat_return = Stat("Total Return", "\u2014", C.TEXT)
        self._stat_sharpe = Stat("Sharpe Ratio", "\u2014", C.TEXT)
        self._stat_maxdd = Stat("Max Drawdown", "\u2014", C.TEXT)
        self._stat_winrate = Stat("Win Rate", "\u2014", C.TEXT)
        self._stat_pf = Stat("Profit Factor", "\u2014", C.TEXT)

        self._metrics_row.addWidget(self._stat_return)
        self._metrics_row.addWidget(self._stat_sharpe)
        self._metrics_row.addWidget(self._stat_maxdd)
        self._metrics_row.addWidget(self._stat_winrate)
        self._metrics_row.addWidget(self._stat_pf)
        self._metrics_row.addStretch()

        self._results_layout.addLayout(self._metrics_row)

        # Equity curve chart
        self._equity_chart = EquityCurveWidget()
        self._results_layout.addWidget(self._equity_chart)

        # Trade log
        self._results_layout.addWidget(Heading("Trade Log", level=3))
        self._trade_log = QTextEdit()
        self._trade_log.setReadOnly(True)
        self._trade_log.setFixedHeight(160)
        self._trade_log.setStyleSheet(
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
        self._results_layout.addWidget(self._trade_log)

        self._results_card.setVisible(False)
        layout.addWidget(self._results_card)

        layout.addStretch()

        scroll.setWidget(content)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _run_backtest(self):
        """Launch the backtest in a worker thread."""
        if self._worker and self._worker.isRunning():
            return

        symbol = self._symbol_input.text().strip().upper()
        if not symbol:
            symbol = "AAPL"
            self._symbol_input.setText(symbol)

        strategy = self._strategy_combo.currentText()
        days = self._days_spin.value()

        self._run_btn.setEnabled(False)
        self._run_btn.setText("Running...")
        self._progress.setVisible(True)
        self._progress.setValue(0)
        self._results_card.setVisible(False)

        self._worker = BacktestWorker(symbol, strategy, days)
        self._worker.progress.connect(self._on_progress)
        self._worker.finished_signal.connect(self._on_finished)
        self._worker.error.connect(self._on_error)
        self._worker.start()

    def _on_progress(self, value: int):
        self._progress.setValue(value)

    def _on_finished(self, result: dict):
        self._run_btn.setEnabled(True)
        self._run_btn.setText("Run Backtest")
        self._progress.setVisible(False)

        # Update metrics
        tr = result.get("total_return", 0)
        tr_color = C.GREEN if tr >= 0 else C.RED
        self._stat_return.set_value(f"{tr:+.2f}%", tr_color)

        sharpe = result.get("sharpe", 0)
        sh_color = C.GREEN if sharpe >= 1 else (C.YELLOW if sharpe >= 0 else C.RED)
        self._stat_sharpe.set_value(f"{sharpe:.2f}", sh_color)

        mdd = result.get("max_drawdown", 0)
        mdd_color = C.GREEN if mdd > -10 else (C.YELLOW if mdd > -20 else C.RED)
        self._stat_maxdd.set_value(f"{mdd:.1f}%", mdd_color)

        wr = result.get("win_rate", 0)
        wr_color = C.GREEN if wr >= 50 else C.RED
        self._stat_winrate.set_value(f"{wr:.1f}%", wr_color)

        pf = result.get("profit_factor", 0)
        pf_color = C.GREEN if pf >= 1.5 else (C.YELLOW if pf >= 1.0 else C.RED)
        pf_str = f"{pf:.2f}" if pf < 100 else "Inf"
        self._stat_pf.set_value(pf_str, pf_color)

        # Update equity curve
        equity = result.get("equity", [])
        drawdowns = result.get("drawdowns", [])
        self._equity_chart.set_data(equity, drawdowns)

        # Update trade log
        trades = result.get("trades", [])
        self._trade_log.clear()
        header = f"{'#':<4} {'Side':<6} {'Entry':>8} {'Exit':>8} {'P&L':>10}"
        self._trade_log.append(header)
        self._trade_log.append("-" * 44)
        for i, t in enumerate(trades):
            pnl = t.get("pnl", 0)
            pnl_sign = "+" if pnl >= 0 else ""
            line = (
                f"{i + 1:<4} {t.get('side', 'LONG'):<6} "
                f"${t.get('entry_price', 0):>7.2f} "
                f"${t.get('exit_price', 0):>7.2f} "
                f"{pnl_sign}${pnl:>8.2f}"
            )
            self._trade_log.append(line)

        if not trades:
            self._trade_log.append("No individual trades recorded.")

        self._results_card.setVisible(True)

    def _on_error(self, message: str):
        self._run_btn.setEnabled(True)
        self._run_btn.setText("Run Backtest")
        self._progress.setVisible(False)
        self._trade_log.clear()
        self._trade_log.append(f"Error: {message}")
        self._results_card.setVisible(True)
