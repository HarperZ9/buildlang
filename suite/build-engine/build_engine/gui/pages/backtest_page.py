"""
Backtest -- run backtests with the prediction strategy.

Configure strategy, symbol, and duration, then view results including
equity curve, trade log, and key performance metrics.
"""

import random

from PyQt6.QtCore import QPointF, QRectF, Qt, QThread, pyqtSignal
from PyQt6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QLinearGradient,
    QPainter,
    QPen,
    QPolygonF,
    QTextCursor,
)
from PyQt6.QtWidgets import (
    QComboBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from build_engine.gui.app import C, Card, Heading, Stat


class BacktestEquityChart(QWidget):
    """Equity curve chart for backtest results."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(220)
        self.setMinimumWidth(400)
        self._data = []
        self._benchmark = []

    def set_data(self, equity: list, benchmark: list = None):
        self._data = equity
        self._benchmark = benchmark or []
        self.update()

    def paintEvent(self, event):
        if len(self._data) < 2:
            p = QPainter(self)
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Run a backtest to see results")
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        pad_x = 55
        pad_y = 20
        chart_w = w - pad_x * 2
        chart_h = h - pad_y * 2

        data = self._data
        all_vals = list(data) + list(self._benchmark)
        n = len(data)
        min_v = min(all_vals) * 0.998
        max_v = max(all_vals) * 1.002
        val_range = max_v - min_v if max_v != min_v else 1.0

        def x_pos(i, total):
            return pad_x + (i / max(total - 1, 1)) * chart_w

        def y_pos(v):
            return pad_y + chart_h - ((v - min_v) / val_range) * chart_h

        # Grid lines
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

        # Benchmark line (if available)
        if len(self._benchmark) >= 2:
            bench_pen = QPen(QColor(C.TEXT3), 1, Qt.PenStyle.DashLine)
            p.setPen(bench_pen)
            bench_pts = [QPointF(x_pos(i, len(self._benchmark)), y_pos(v)) for i, v in enumerate(self._benchmark)]
            for i in range(len(bench_pts) - 1):
                p.drawLine(bench_pts[i], bench_pts[i + 1])

        # Strategy equity fill
        points = [QPointF(x_pos(i, n), y_pos(v)) for i, v in enumerate(data)]
        fill_poly = QPolygonF()
        fill_poly.append(QPointF(points[0].x(), pad_y + chart_h))
        for pt in points:
            fill_poly.append(pt)
        fill_poly.append(QPointF(points[-1].x(), pad_y + chart_h))

        final_return = (data[-1] - data[0]) / data[0]
        if final_return >= 0:
            fill_color = C.GREEN
        else:
            fill_color = C.RED

        grad = QLinearGradient(0, pad_y, 0, pad_y + chart_h)
        c1 = QColor(fill_color)
        c1.setAlpha(60)
        c2 = QColor(fill_color)
        c2.setAlpha(10)
        grad.setColorAt(0, c1)
        grad.setColorAt(1, c2)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QBrush(grad))
        p.drawPolygon(fill_poly)

        # Strategy line
        pen = QPen(QColor(fill_color), 2)
        p.setPen(pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        for i in range(len(points) - 1):
            p.drawLine(points[i], points[i + 1])

        # End dot
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(fill_color))
        p.drawEllipse(points[-1], 4, 4)

        # Legend
        p.setFont(QFont("Segoe UI", 9))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(fill_color))
        p.drawEllipse(QPointF(pad_x + 10, pad_y - 6), 4, 4)
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(pad_x + 18, pad_y - 2), "Strategy")

        if self._benchmark:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(C.TEXT3))
            p.drawEllipse(QPointF(pad_x + 90, pad_y - 6), 4, 4)
            p.setPen(QColor(C.TEXT3))
            p.drawText(QPointF(pad_x + 98, pad_y - 2), "Buy & Hold")

        p.end()


class BacktestWorker(QThread):
    """Run backtest in background thread."""

    log_line = pyqtSignal(str)
    result_ready = pyqtSignal(dict)
    error = pyqtSignal(str)

    def __init__(self, config: dict, parent=None):
        super().__init__(parent)
        self._config = config

    def run(self):
        try:
            self.log_line.emit("Starting backtest...")
            self.log_line.emit(f"  Strategy : {self._config['strategy']}")
            self.log_line.emit(f"  Symbol   : {self._config['symbol']}")
            self.log_line.emit(f"  Days     : {self._config['days']}")
            self.log_line.emit(f"  Capital  : ${self._config['capital']:,.0f}")
            self.log_line.emit("")

            try:
                from build_finance.backtest import (
                    BacktestConfig,
                    Backtester,
                    generate_sample_data,
                )

                from build_engine.config import EngineConfig
                from build_engine.prediction_strategy import PredictionStrategy

                engine_config = EngineConfig(
                    symbols=[self._config["symbol"]],
                    models=["arima", "prophet"],
                    forecast_horizon=5,
                    min_confidence=0.3,
                )

                strategy = PredictionStrategy(engine_config)

                candles = generate_sample_data(
                    symbol=self._config["symbol"],
                    days=self._config["days"],
                    start_price=100.0,
                    volatility=0.02,
                    seed=42,
                )

                bt_config = BacktestConfig(
                    initial_capital=self._config["capital"],
                    risk_per_trade=0.10,
                    max_positions=5,
                )

                bt = Backtester(bt_config)
                result = bt.run(strategy, {self._config["symbol"]: candles})

                # Build result dict
                equity_curve = list(result.equity_curve) if hasattr(result, "equity_curve") else []
                trades_log = []
                if hasattr(result, "trades"):
                    for t in result.trades[:50]:
                        trades_log.append(str(t))

                result_dict = {
                    "total_return": getattr(result, "total_return", 0),
                    "sharpe": getattr(result, "sharpe_ratio", 0),
                    "max_drawdown": getattr(result, "max_drawdown", 0),
                    "win_rate": getattr(result, "win_rate", 0),
                    "profit_factor": getattr(result, "profit_factor", 0),
                    "total_trades": getattr(result, "total_trades", 0),
                    "equity_curve": equity_curve,
                    "trades_log": trades_log,
                }

                self.log_line.emit("Backtest complete!")
                self.log_line.emit(f"  Total Return  : {result_dict['total_return']:.2%}")
                self.log_line.emit(f"  Sharpe Ratio  : {result_dict['sharpe']:.2f}")
                self.log_line.emit(f"  Max Drawdown  : {result_dict['max_drawdown']:.2%}")
                self.log_line.emit(f"  Win Rate      : {result_dict['win_rate']:.0%}")
                self.log_line.emit(f"  Profit Factor : {result_dict['profit_factor']:.2f}")
                self.log_line.emit(f"  Total Trades  : {result_dict['total_trades']}")

                self.result_ready.emit(result_dict)
                return

            except ImportError:
                self.log_line.emit("  build-finance not available, generating synthetic results...")

            # Synthetic backtest results
            capital = self._config["capital"]
            days = self._config["days"]
            equity = [capital]
            trades_log = []
            wins = 0
            losses = 0
            total_profit = 0
            total_loss = 0

            for d in range(1, days + 1):
                # Simulate daily return
                daily_return = random.gauss(0.0005, 0.015)
                capital *= 1 + daily_return
                equity.append(capital)

                # Simulate occasional trades
                if random.random() < 0.08:
                    side = random.choice(["BUY", "SELL"])
                    pnl = random.gauss(50, 200)
                    sym = self._config["symbol"]
                    price = 100 * (1 + random.gauss(0, 0.1))
                    trades_log.append(f"Day {d}: {side} {sym} @ ${price:.2f}  P&L: ${pnl:+.2f}")
                    if pnl > 0:
                        wins += 1
                        total_profit += pnl
                    else:
                        losses += 1
                        total_loss += abs(pnl)

            total_trades = wins + losses
            total_return = (equity[-1] - equity[0]) / equity[0]

            # Sharpe
            returns = [(equity[i] - equity[i - 1]) / equity[i - 1] for i in range(1, len(equity))]
            avg_ret = sum(returns) / len(returns) if returns else 0
            std_ret = (sum((r - avg_ret) ** 2 for r in returns) / max(len(returns) - 1, 1)) ** 0.5
            sharpe = (avg_ret / std_ret * (252**0.5)) if std_ret > 0 else 0

            # Max drawdown
            peak = equity[0]
            max_dd = 0
            for v in equity:
                peak = max(peak, v)
                dd = (peak - v) / peak
                max_dd = max(max_dd, dd)

            win_rate = wins / total_trades if total_trades > 0 else 0
            profit_factor = total_profit / total_loss if total_loss > 0 else 0

            # Benchmark (buy & hold)
            bench_start = self._config["capital"]
            benchmark = [bench_start]
            for _d in range(1, days + 1):
                bench_start *= 1 + random.gauss(0.0003, 0.012)
                benchmark.append(bench_start)

            result_dict = {
                "total_return": total_return,
                "sharpe": sharpe,
                "max_drawdown": max_dd,
                "win_rate": win_rate,
                "profit_factor": profit_factor,
                "total_trades": total_trades,
                "equity_curve": equity,
                "benchmark": benchmark,
                "trades_log": trades_log,
            }

            self.log_line.emit("")
            self.log_line.emit("Backtest complete!")
            self.log_line.emit(f"  Total Return  : {total_return:+.2%}")
            self.log_line.emit(f"  Sharpe Ratio  : {sharpe:.2f}")
            self.log_line.emit(f"  Max Drawdown  : {max_dd:.2%}")
            self.log_line.emit(f"  Win Rate      : {win_rate:.0%}")
            self.log_line.emit(f"  Profit Factor : {profit_factor:.2f}")
            self.log_line.emit(f"  Total Trades  : {total_trades}")

            self.result_ready.emit(result_dict)

        except Exception as exc:
            self.log_line.emit(f"[ERROR] {exc}")
            self.error.emit(str(exc))


class BacktestPage(QWidget):
    """Backtest configuration and results page."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._worker = None
        self._build_ui()

    def _build_ui(self):
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(18)

        layout.addWidget(Heading("Backtest"))

        # Config row
        card_config, config_lay = Card.with_layout(layout_cls=QHBoxLayout)

        # Strategy
        strat_col = QVBoxLayout()
        strat_col.setSpacing(4)
        strat_col.addWidget(QLabel("Strategy"))
        self._strategy_combo = QComboBox()
        self._strategy_combo.addItems(
            [
                "Prediction (ARIMA+Prophet)",
                "Momentum",
                "Mean Reversion",
                "Trend Following",
                "Ensemble (All Models)",
            ]
        )
        strat_col.addWidget(self._strategy_combo)
        config_lay.addLayout(strat_col)

        # Symbol
        sym_col = QVBoxLayout()
        sym_col.setSpacing(4)
        sym_col.addWidget(QLabel("Symbol"))
        self._symbol_input = QLineEdit("AAPL")
        self._symbol_input.setFixedWidth(120)
        sym_col.addWidget(self._symbol_input)
        config_lay.addLayout(sym_col)

        # Days
        days_col = QVBoxLayout()
        days_col.setSpacing(4)
        days_col.addWidget(QLabel("Days"))
        self._days_spin = QSpinBox()
        self._days_spin.setRange(30, 2520)
        self._days_spin.setValue(252)
        self._days_spin.setSuffix(" days")
        self._days_spin.setFixedWidth(120)
        days_col.addWidget(self._days_spin)
        config_lay.addLayout(days_col)

        # Capital
        cap_col = QVBoxLayout()
        cap_col.setSpacing(4)
        cap_col.addWidget(QLabel("Capital"))
        self._capital_spin = QSpinBox()
        self._capital_spin.setRange(1000, 10_000_000)
        self._capital_spin.setValue(100_000)
        self._capital_spin.setSingleStep(10000)
        self._capital_spin.setPrefix("$")
        self._capital_spin.setFixedWidth(140)
        cap_col.addWidget(self._capital_spin)
        config_lay.addLayout(cap_col)

        config_lay.addStretch()

        # Run button
        self._btn_run = QPushButton("Run Backtest")
        self._btn_run.setProperty("primary", True)
        self._btn_run.setFixedHeight(38)
        self._btn_run.setCursor(Qt.CursorShape.PointingHandCursor)
        self._btn_run.clicked.connect(self._run_backtest)
        config_lay.addWidget(self._btn_run)

        layout.addWidget(card_config)

        # Results stats
        card_stats, stats_lay = Card.with_layout(layout_cls=QHBoxLayout, spacing=24)
        self._stat_return = Stat("Total Return", "--", C.TEXT)
        self._stat_sharpe = Stat("Sharpe Ratio", "--", C.TEXT)
        self._stat_drawdown = Stat("Max Drawdown", "--", C.TEXT)
        self._stat_winrate = Stat("Win Rate", "--", C.TEXT)
        self._stat_pf = Stat("Profit Factor", "--", C.TEXT)
        stats_lay.addWidget(self._stat_return)
        stats_lay.addWidget(self._stat_sharpe)
        stats_lay.addWidget(self._stat_drawdown)
        stats_lay.addWidget(self._stat_winrate)
        stats_lay.addWidget(self._stat_pf)
        stats_lay.addStretch()
        layout.addWidget(card_stats)

        # Equity curve
        card_chart, chart_lay = Card.with_layout()
        chart_lay.addWidget(Heading("Equity Curve", level=2))
        self._equity_chart = BacktestEquityChart()
        chart_lay.addWidget(self._equity_chart)
        layout.addWidget(card_chart)

        # Trade log
        card_log, log_lay = Card.with_layout()
        log_lay.addWidget(Heading("Trade Log", level=2))

        self._trade_log = QTextEdit()
        self._trade_log.setReadOnly(True)
        self._trade_log.setFont(QFont("Cascadia Code", 10))
        self._trade_log.setFixedHeight(200)
        self._trade_log.setStyleSheet(
            f"QTextEdit {{ background: #faf5f0; border: 1px solid {C.BORDER}; border-radius: 8px; padding: 10px; }}"
        )
        self._trade_log.setPlaceholderText("Trade log will appear here after backtest...")
        log_lay.addWidget(self._trade_log)
        layout.addWidget(card_log)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _run_backtest(self):
        if self._worker and self._worker.isRunning():
            return

        self._btn_run.setEnabled(False)
        self._btn_run.setText("Running...")
        self._trade_log.clear()
        self._equity_chart.set_data([])

        config = {
            "strategy": self._strategy_combo.currentText(),
            "symbol": self._symbol_input.text().strip() or "AAPL",
            "days": self._days_spin.value(),
            "capital": self._capital_spin.value(),
        }

        self._worker = BacktestWorker(config)
        self._worker.log_line.connect(self._append_log)
        self._worker.result_ready.connect(self._on_result)
        self._worker.error.connect(self._on_error)
        self._worker.finished.connect(self._on_finished)
        self._worker.start()

    def _append_log(self, text: str):
        self._trade_log.append(text)
        cursor = self._trade_log.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        self._trade_log.setTextCursor(cursor)

    def _on_result(self, result: dict):
        # Update stats
        ret = result.get("total_return", 0)
        ret_color = C.GREEN if ret >= 0 else C.RED
        self._stat_return.set_value(f"{ret:+.2%}", ret_color)
        self._stat_sharpe.set_value(f"{result.get('sharpe', 0):.2f}", C.TEXT)

        dd = result.get("max_drawdown", 0)
        dd_color = C.GREEN if dd < 0.1 else C.YELLOW if dd < 0.2 else C.RED
        self._stat_drawdown.set_value(f"{dd:.2%}", dd_color)

        wr = result.get("win_rate", 0)
        wr_color = C.GREEN if wr >= 0.55 else C.YELLOW if wr >= 0.45 else C.RED
        self._stat_winrate.set_value(f"{wr:.0%}", wr_color)

        pf = result.get("profit_factor", 0)
        pf_color = C.GREEN if pf >= 1.5 else C.YELLOW if pf >= 1.0 else C.RED
        self._stat_pf.set_value(f"{pf:.2f}", pf_color)

        # Update equity chart
        equity = result.get("equity_curve", [])
        benchmark = result.get("benchmark", [])
        if equity:
            self._equity_chart.set_data(equity, benchmark)

        # Trade log
        trades = result.get("trades_log", [])
        if trades:
            self._trade_log.append("")
            self._trade_log.append("=== Trade Details ===")
            for t in trades:
                self._trade_log.append(t)

        if self._main_window:
            self._main_window.show_toast("Backtest complete", "success")

    def _on_error(self, msg: str):
        if self._main_window:
            self._main_window.show_toast(f"Backtest error: {msg}", "error")

    def _on_finished(self):
        self._btn_run.setEnabled(True)
        self._btn_run.setText("Run Backtest")
        self._worker = None

    def refresh(self):
        pass
