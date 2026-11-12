"""
Build Finance — Portfolio Page

Portfolio optimization with multiple methods, weight visualization,
and performance statistics.
"""

import logging

from PyQt6.QtCore import QRectF, Qt, QThread, pyqtSignal
from PyQt6.QtGui import QColor, QFont, QPainter
from PyQt6.QtWidgets import (
    QComboBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from build_finance.gui.app import C, Card, Heading, Stat

logger = logging.getLogger(__name__)


# =============================================================================
# Weight Bar Chart Widget
# =============================================================================


class WeightBarWidget(QWidget):
    """Horizontal bar chart showing portfolio weight allocations."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumHeight(60)
        self._data = {}  # {symbol: weight}

    def set_data(self, weights: dict):
        """Set weight allocations. weights: {symbol: float 0-1}"""
        self._data = weights or {}
        self.setFixedHeight(max(60, len(self._data) * 36 + 20))
        self.update()

    def paintEvent(self, event):
        if not self._data:
            p = QPainter(self)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No data \u2014 run an optimization")
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        self.height()
        margin_l = 80
        margin_r = 60
        bar_h = 22
        spacing = 36
        chart_w = w - margin_l - margin_r

        colors = [C.ACCENT, C.GREEN, C.CYAN, C.YELLOW, C.RED, C.ACCENT_HI, C.GREEN_HI, C.ACCENT_TX]

        sorted_items = sorted(self._data.items(), key=lambda x: x[1], reverse=True)

        for i, (symbol, weight) in enumerate(sorted_items):
            y = 10 + i * spacing
            color = colors[i % len(colors)]

            # Symbol label
            p.setPen(QColor(C.TEXT))
            p.setFont(QFont("Segoe UI", 10, QFont.Weight.Medium))
            p.drawText(
                QRectF(0, y, margin_l - 8, bar_h), Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter, symbol
            )

            # Background bar
            bar_rect = QRectF(margin_l, y, chart_w, bar_h)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(C.BORDER))
            p.drawRoundedRect(bar_rect, 6, 6)

            # Filled bar
            fill_w = max(4, chart_w * weight)
            fill_rect = QRectF(margin_l, y, fill_w, bar_h)
            bar_color = QColor(color)
            bar_color.setAlpha(180)
            p.setBrush(bar_color)
            p.drawRoundedRect(fill_rect, 6, 6)

            # Percentage label
            p.setPen(QColor(C.TEXT2))
            p.setFont(QFont("Segoe UI", 9))
            pct_text = f"{weight * 100:.1f}%"
            p.drawText(
                QRectF(margin_l + chart_w + 8, y, margin_r - 8, bar_h),
                Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                pct_text,
            )

        p.end()


# =============================================================================
# Portfolio Optimization Worker
# =============================================================================


class PortfolioWorker(QThread):
    """Runs portfolio optimization in a background thread."""

    finished_signal = pyqtSignal(dict)
    error = pyqtSignal(str)

    def __init__(self, symbols: list[str], method: str):
        super().__init__()
        self._symbols = symbols
        self._method = method

    def run(self):
        try:
            # Try real optimizer first
            try:
                from build_finance.portfolio import optimize

                result = optimize(self._symbols, method=self._method)
                self.finished_signal.emit(result)
                return
            except ImportError:
                pass

            # Fallback: generate synthetic demo weights
            import math
            import random
            import time

            time.sleep(0.5)
            random.seed(hash(tuple(self._symbols)) % 2**32)

            n = len(self._symbols)

            if self._method == "Risk Parity":
                # Equal risk contribution approximation
                raw = [1.0 / n + random.gauss(0, 0.02) for _ in range(n)]
            elif self._method == "Black-Litterman":
                # Market-cap weighted bias
                raw = [random.expovariate(1.5) for _ in range(n)]
            elif self._method == "HRP":
                # Hierarchical risk parity
                raw = [random.uniform(0.05, 0.35) for _ in range(n)]
            else:
                # Mean-Variance (default)
                raw = [max(0.01, random.gauss(1.0 / n, 0.15)) for _ in range(n)]

            total = sum(raw)
            weights = {sym: w / total for sym, w in zip(self._symbols, raw)}

            # Compute portfolio stats
            exp_return = sum(random.gauss(0.08, 0.04) * w for w in weights.values())
            volatility = math.sqrt(sum((random.gauss(0.20, 0.05) * w) ** 2 for w in weights.values()))
            sharpe = exp_return / volatility if volatility > 0 else 0

            result = {
                "weights": weights,
                "expected_return": exp_return * 100,
                "volatility": volatility * 100,
                "sharpe": sharpe,
                "method": self._method,
            }
            self.finished_signal.emit(result)

        except Exception as e:
            logger.error("Portfolio optimization error: %s", e)
            self.error.emit(str(e))


# =============================================================================
# Portfolio Page
# =============================================================================


class PortfolioPage(QWidget):
    """Portfolio optimization configuration and results."""

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

        layout.addWidget(Heading("Portfolio Optimization"))

        subtitle = QLabel("Optimize asset allocation across multiple methods")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Configuration Card ---
        config_card, config_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        config_layout.addWidget(Heading("Configuration", level=2))

        config_row = QHBoxLayout()
        config_row.setSpacing(16)

        # Method selector
        method_col = QVBoxLayout()
        method_label = QLabel("Optimization Method")
        method_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        method_col.addWidget(method_label)
        self._method_combo = QComboBox()
        self._method_combo.addItems(["Mean-Variance", "Risk Parity", "Black-Litterman", "HRP"])
        self._method_combo.setFixedWidth(200)
        method_col.addWidget(self._method_combo)
        config_row.addLayout(method_col)

        # Symbols input
        sym_col = QVBoxLayout()
        sym_label = QLabel("Symbols (comma-separated)")
        sym_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        sym_col.addWidget(sym_label)
        self._symbols_input = QLineEdit("AAPL,GOOGL,MSFT,AMZN,TSLA")
        self._symbols_input.setFixedWidth(320)
        self._symbols_input.setPlaceholderText("e.g. AAPL,GOOGL,MSFT")
        sym_col.addWidget(self._symbols_input)
        config_row.addLayout(sym_col)

        config_row.addStretch()
        config_layout.addLayout(config_row)

        # Optimize button
        btn_row = QHBoxLayout()
        self._opt_btn = QPushButton("Optimize")
        self._opt_btn.setProperty("primary", True)
        self._opt_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._opt_btn.setFixedWidth(160)
        self._opt_btn.clicked.connect(self._run_optimization)
        btn_row.addWidget(self._opt_btn)
        btn_row.addStretch()
        config_layout.addLayout(btn_row)

        layout.addWidget(config_card)

        # --- Results Card (initially hidden) ---
        self._results_card, self._results_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        self._results_layout.addWidget(Heading("Optimal Allocation", level=2))

        # Weight bars
        self._weight_bars = WeightBarWidget()
        self._results_layout.addWidget(self._weight_bars)

        # Stats row
        self._results_layout.addWidget(Heading("Portfolio Statistics", level=3))
        stats_row = QHBoxLayout()
        self._stat_return = Stat("Expected Return", "\u2014", C.TEXT)
        self._stat_vol = Stat("Volatility", "\u2014", C.TEXT)
        self._stat_sharpe = Stat("Sharpe Ratio", "\u2014", C.TEXT)
        self._stat_method = Stat("Method", "\u2014", C.TEXT2)

        stats_row.addWidget(self._stat_return)
        stats_row.addWidget(self._stat_vol)
        stats_row.addWidget(self._stat_sharpe)
        stats_row.addWidget(self._stat_method)
        stats_row.addStretch()
        self._results_layout.addLayout(stats_row)

        self._results_card.setVisible(False)
        layout.addWidget(self._results_card)

        layout.addStretch()

        scroll.setWidget(content)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _run_optimization(self):
        """Launch portfolio optimization in a worker thread."""
        if self._worker and self._worker.isRunning():
            return

        symbols_text = self._symbols_input.text().strip()
        if not symbols_text:
            symbols_text = "AAPL,GOOGL,MSFT"
            self._symbols_input.setText(symbols_text)

        symbols = [s.strip().upper() for s in symbols_text.split(",") if s.strip()]
        method = self._method_combo.currentText()

        self._opt_btn.setEnabled(False)
        self._opt_btn.setText("Optimizing...")
        self._results_card.setVisible(False)

        self._worker = PortfolioWorker(symbols, method)
        self._worker.finished_signal.connect(self._on_finished)
        self._worker.error.connect(self._on_error)
        self._worker.start()

    def _on_finished(self, result: dict):
        self._opt_btn.setEnabled(True)
        self._opt_btn.setText("Optimize")

        # Update weight bars
        weights = result.get("weights", {})
        self._weight_bars.set_data(weights)

        # Update stats
        exp_ret = result.get("expected_return", 0)
        ret_color = C.GREEN if exp_ret >= 0 else C.RED
        self._stat_return.set_value(f"{exp_ret:.1f}%", ret_color)

        vol = result.get("volatility", 0)
        vol_color = C.GREEN if vol < 20 else (C.YELLOW if vol < 30 else C.RED)
        self._stat_vol.set_value(f"{vol:.1f}%", vol_color)

        sharpe = result.get("sharpe", 0)
        sh_color = C.GREEN if sharpe >= 1 else (C.YELLOW if sharpe >= 0 else C.RED)
        self._stat_sharpe.set_value(f"{sharpe:.2f}", sh_color)

        method = result.get("method", "")
        self._stat_method.set_value(method, C.ACCENT_TX)

        self._results_card.setVisible(True)

    def _on_error(self, message: str):
        self._opt_btn.setEnabled(True)
        self._opt_btn.setText("Optimize")
        logger.error("Portfolio error: %s", message)
