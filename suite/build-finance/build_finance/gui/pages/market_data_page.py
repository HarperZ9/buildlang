"""
Build Finance -- Market Data Page

Watchlist table, quote lookup, data source indicator,
and auto-refresh with interval selector.  Wired to DataBridge.
"""

from __future__ import annotations

import logging

from PyQt6.QtCore import QPointF, QRectF, Qt, QThread, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QLinearGradient, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QFileDialog,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from build_finance.gui.app import C, Card, Heading, Stat

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Table style (shared)
# ---------------------------------------------------------------------------

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
# Price Chart Widget  (kept from original)
# ---------------------------------------------------------------------------


class PriceChartWidget(QWidget):
    """Simple QPainter line chart for price data."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(200)
        self.setMinimumWidth(400)
        self._prices: list[float] = []
        self._symbol = ""

    def set_data(self, prices: list[float], symbol: str = "") -> None:
        self._prices = prices or []
        self._symbol = symbol
        self.update()

    def paintEvent(self, event):
        if not self._prices or len(self._prices) < 2:
            p = QPainter(self)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(QColor(C.TEXT3))
            p.drawText(
                self.rect(),
                Qt.AlignmentFlag.AlignCenter,
                "No data \u2014 fetch market data first",
            )
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        margin_l, margin_r, margin_t, margin_b = 60, 16, 16, 28
        chart_w = w - margin_l - margin_r
        chart_h = h - margin_t - margin_b

        data = self._prices
        n = len(data)
        y_min, y_max = min(data), max(data)
        y_range = y_max - y_min if y_max != y_min else 1.0

        def to_x(i):
            return margin_l + (i / (n - 1)) * chart_w

        def to_y(v):
            return margin_t + chart_h - ((v - y_min) / y_range) * chart_h

        # Background
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.SURFACE2))
        p.drawRoundedRect(0, 0, w, h, 10, 10)

        # Grid
        grid_pen = QPen(QColor(C.BORDER), 1)
        grid_pen.setStyle(Qt.PenStyle.DotLine)
        p.setPen(grid_pen)
        for frac in [0.0, 0.25, 0.5, 0.75, 1.0]:
            y_val = y_min + frac * y_range
            y_px = to_y(y_val)
            p.drawLine(
                QPointF(margin_l, y_px),
                QPointF(w - margin_r, y_px),
            )
            p.setPen(QColor(C.TEXT3))
            p.drawText(
                QRectF(0, y_px - 8, margin_l - 6, 16),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                f"${y_val:,.2f}",
            )
            p.setPen(grid_pen)

        # Line path
        path = QPainterPath()
        path.moveTo(to_x(0), to_y(data[0]))
        for i in range(1, n):
            path.lineTo(to_x(i), to_y(data[i]))

        # Fill
        fill_path = QPainterPath(path)
        fill_path.lineTo(to_x(n - 1), margin_t + chart_h)
        fill_path.lineTo(to_x(0), margin_t + chart_h)
        fill_path.closeSubpath()

        up = data[-1] >= data[0]
        line_color_str = C.GREEN if up else C.RED
        gradient = QLinearGradient(0, margin_t, 0, margin_t + chart_h)
        gc = QColor(line_color_str)
        gc.setAlpha(35)
        gradient.setColorAt(0, gc)
        gc2 = QColor(line_color_str)
        gc2.setAlpha(5)
        gradient.setColorAt(1, gc2)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(gradient)
        p.drawPath(fill_path)

        line_pen = QPen(QColor(line_color_str), 2.0)
        line_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        line_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setPen(line_pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawPath(path)

        # Dots
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(line_color_str))
        p.drawEllipse(QPointF(to_x(0), to_y(data[0])), 3, 3)
        p.drawEllipse(QPointF(to_x(n - 1), to_y(data[-1])), 3, 3)

        # Title
        label = self._symbol if self._symbol else "Price"
        p.setPen(QColor(C.TEXT2))
        p.drawText(
            QRectF(margin_l, h - margin_b + 4, chart_w, 20),
            Qt.AlignmentFlag.AlignCenter,
            label,
        )
        p.end()


# ---------------------------------------------------------------------------
# Data Fetch Worker  (kept from original)
# ---------------------------------------------------------------------------


class FetchWorker(QThread):
    """Fetches market data in a background thread."""

    finished_signal = pyqtSignal(dict)
    error = pyqtSignal(str)

    def __init__(self, symbol: str, source: str):
        super().__init__()
        self._symbol = symbol
        self._source = source

    def run(self):
        try:
            # Try real data fetch
            try:
                from build_finance.market_data import fetch_yahoo

                candles = fetch_yahoo(self._symbol, period="1y")
                if candles:
                    closes = [c.close for c in candles]
                    last = candles[-1]
                    result = {
                        "symbol": self._symbol,
                        "source": self._source,
                        "prices": closes,
                        "candles": len(closes),
                        "open": last.open,
                        "high": last.high,
                        "low": last.low,
                        "close": last.close,
                        "volume": int(last.volume),
                    }
                    self.finished_signal.emit(result)
                    return
            except Exception:
                pass

            # Fallback: synthetic demo data
            import math  # noqa: F401
            import random
            import time as _time

            _time.sleep(0.4)
            random.seed(hash(self._symbol) % 2**32)
            base_price = random.uniform(50, 500)
            prices = [base_price]
            for _ in range(251):
                change = random.gauss(0.0003, 0.018)
                prices.append(prices[-1] * (1 + change))

            last_price = prices[-1]
            dr = last_price * random.uniform(0.01, 0.04)
            result = {
                "symbol": self._symbol,
                "source": self._source,
                "prices": prices,
                "candles": len(prices),
                "open": last_price - random.uniform(-dr, dr),
                "high": last_price + dr * 0.5,
                "low": last_price - dr * 0.5,
                "close": last_price,
                "volume": random.randint(1_000_000, 50_000_000),
            }
            self.finished_signal.emit(result)

        except Exception as e:
            logger.error("Fetch error: %s", e)
            self.error.emit(str(e))


# ---------------------------------------------------------------------------
# Market Data Page
# ---------------------------------------------------------------------------

_DEFAULT_WATCHLIST = [
    "AAPL",
    "MSFT",
    "GOOGL",
    "NVDA",
    "TSLA",
    "AMZN",
    "META",
    "JPM",
]

_REFRESH_INTERVALS = {
    "5s": 5_000,
    "15s": 15_000,
    "30s": 30_000,
    "60s": 60_000,
}


class MarketDataPage(QWidget):
    """Market data: watchlist, quote lookup, chart, auto-refresh."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._worker = None
        self._last_data = None
        self._bridge = None
        self._auto_timer = None
        self._build_ui()
        self._init_bridge()

    def _init_bridge(self) -> None:
        try:
            from build_finance.gui.data_bridge import DataBridge

            self._bridge = DataBridge(use_paper=True)
        except Exception as exc:
            logger.warning("DataBridge init failed: %s", exc)
            self._bridge = None
        self._load_watchlist()

    # -- UI build -----------------------------------------------------------

    def _build_ui(self) -> None:
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        layout.addWidget(Heading("Market Data"))

        subtitle = QLabel("Live quotes, watchlist, and historical data visualization")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Data Source Indicator ---
        src_card, src_layout = Card.with_layout(
            QHBoxLayout,
            margins=(24, 12, 24, 12),
        )
        src_layout.addWidget(QLabel("Data Source:"))
        self._source_label = QLabel("Initializing...")
        self._source_label.setStyleSheet(f"font-weight: 600; color: {C.ACCENT_TX};")
        src_layout.addWidget(self._source_label)
        src_layout.addStretch()

        # Auto-refresh controls
        self._auto_check = QCheckBox("Auto-Refresh")
        self._auto_check.setChecked(False)
        self._auto_check.stateChanged.connect(self._toggle_auto_refresh)
        src_layout.addWidget(self._auto_check)

        self._interval_combo = QComboBox()
        self._interval_combo.addItems(list(_REFRESH_INTERVALS.keys()))
        self._interval_combo.setCurrentText("15s")
        self._interval_combo.setFixedWidth(80)
        self._interval_combo.currentTextChanged.connect(self._update_refresh_interval)
        src_layout.addWidget(self._interval_combo)

        layout.addWidget(src_card)

        # --- Watchlist Card ---
        wl_card, wl_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        wl_layout.addWidget(Heading("Watchlist", level=2))

        self._watchlist_table = QTableWidget(0, 6)
        self._watchlist_table.setHorizontalHeaderLabels(
            [
                "Symbol",
                "Last Price",
                "Change",
                "Volume",
                "Bid",
                "Ask",
            ]
        )
        self._watchlist_table.setStyleSheet(_TABLE_STYLE)
        self._watchlist_table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._watchlist_table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self._watchlist_table.setAlternatingRowColors(True)
        self._watchlist_table.verticalHeader().setVisible(False)
        self._watchlist_table.setFixedHeight(260)

        hdr = self._watchlist_table.horizontalHeader()
        hdr.setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        wl_layout.addWidget(self._watchlist_table)

        layout.addWidget(wl_card)

        # --- Quote Lookup Card ---
        quote_card, quote_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )
        quote_layout.addWidget(Heading("Quote Lookup", level=2))

        fetch_row = QHBoxLayout()
        fetch_row.setSpacing(16)

        sym_col = QVBoxLayout()
        sym_label = QLabel("Symbol")
        sym_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        sym_col.addWidget(sym_label)
        self._symbol_input = QLineEdit("AAPL")
        self._symbol_input.setFixedWidth(140)
        self._symbol_input.setPlaceholderText("e.g. AAPL")
        self._symbol_input.returnPressed.connect(self._fetch_data)
        sym_col.addWidget(self._symbol_input)
        fetch_row.addLayout(sym_col)

        source_col = QVBoxLayout()
        source_label = QLabel("Source")
        source_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        source_col.addWidget(source_label)
        self._source_combo = QComboBox()
        self._source_combo.addItems(
            [
                "Yahoo Finance",
                "Alpaca",
                "CoinGecko",
                "CSV File",
            ]
        )
        self._source_combo.setFixedWidth(160)
        source_col.addWidget(self._source_combo)
        fetch_row.addLayout(source_col)

        fetch_row.addStretch()
        quote_layout.addLayout(fetch_row)

        # Fetch + Export buttons
        btn_row = QHBoxLayout()
        self._fetch_btn = QPushButton("Fetch")
        self._fetch_btn.setProperty("primary", True)
        self._fetch_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._fetch_btn.setFixedWidth(120)
        self._fetch_btn.clicked.connect(self._fetch_data)
        btn_row.addWidget(self._fetch_btn)

        self._save_btn = QPushButton("Save as CSV")
        self._save_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._save_btn.setFixedWidth(140)
        self._save_btn.clicked.connect(self._save_csv)
        self._save_btn.setEnabled(False)
        btn_row.addWidget(self._save_btn)

        btn_row.addStretch()
        quote_layout.addLayout(btn_row)

        layout.addWidget(quote_card)

        # --- Results Card (hidden until fetch) ---
        self._results_card, self._results_layout = Card.with_layout(
            QVBoxLayout,
            margins=(24, 20, 24, 20),
        )

        self._chart_heading = Heading("Price Chart", level=2)
        self._results_layout.addWidget(self._chart_heading)
        self._price_chart = PriceChartWidget()
        self._results_layout.addWidget(self._price_chart)

        self._results_layout.addWidget(Heading("Latest Data", level=3))
        stats_row = QHBoxLayout()
        self._stat_open = Stat("Open", "\u2014", C.TEXT)
        self._stat_high = Stat("High", "\u2014", C.GREEN)
        self._stat_low = Stat("Low", "\u2014", C.RED)
        self._stat_close = Stat("Close", "\u2014", C.TEXT)
        self._stat_volume = Stat("Volume", "\u2014", C.CYAN)
        self._stat_candles = Stat("Candles", "\u2014", C.TEXT2)
        for s in (
            self._stat_open,
            self._stat_high,
            self._stat_low,
            self._stat_close,
            self._stat_volume,
            self._stat_candles,
        ):
            stats_row.addWidget(s)
        stats_row.addStretch()
        self._results_layout.addLayout(stats_row)

        self._results_card.setVisible(False)
        layout.addWidget(self._results_card)

        layout.addStretch()
        scroll.setWidget(content)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    # -- watchlist ----------------------------------------------------------

    def _load_watchlist(self) -> None:
        """Populate the watchlist table via DataBridge."""
        if self._bridge is None:
            return

        self._source_label.setText(self._bridge.source_label)

        data = self._bridge.get_watchlist(_DEFAULT_WATCHLIST)
        self._watchlist_table.setRowCount(len(data))

        for row, item in enumerate(data):
            self._watchlist_table.setItem(
                row,
                0,
                QTableWidgetItem(item.get("symbol", "")),
            )

            last = item.get("last", 0)
            self._watchlist_table.setItem(
                row,
                1,
                QTableWidgetItem(f"${last:,.2f}"),
            )

            change = item.get("change", 0)
            pct = item.get("change_pct", 0)
            change_text = f"${change:+.2f} ({pct:+.2f}%)"
            change_item = QTableWidgetItem(change_text)
            change_item.setForeground(QColor(C.GREEN if change >= 0 else C.RED))
            self._watchlist_table.setItem(row, 2, change_item)

            vol = item.get("volume", 0)
            if vol >= 1_000_000:
                vol_str = f"{vol / 1_000_000:.1f}M"
            elif vol >= 1_000:
                vol_str = f"{vol / 1_000:.1f}K"
            else:
                vol_str = str(vol)
            self._watchlist_table.setItem(
                row,
                3,
                QTableWidgetItem(vol_str),
            )

            self._watchlist_table.setItem(
                row,
                4,
                QTableWidgetItem(f"${item.get('bid', 0):,.2f}"),
            )
            self._watchlist_table.setItem(
                row,
                5,
                QTableWidgetItem(f"${item.get('ask', 0):,.2f}"),
            )

    # -- auto-refresh -------------------------------------------------------

    def _toggle_auto_refresh(self, state: int) -> None:
        checked = state == Qt.CheckState.Checked.value
        if checked:
            interval_key = self._interval_combo.currentText()
            ms = _REFRESH_INTERVALS.get(interval_key, 15_000)
            if self._auto_timer is None:
                self._auto_timer = QTimer(self)
                self._auto_timer.timeout.connect(self._auto_refresh_tick)
            self._auto_timer.start(ms)
        else:
            if self._auto_timer:
                self._auto_timer.stop()

    def _update_refresh_interval(self, key: str) -> None:
        if self._auto_timer and self._auto_timer.isActive():
            ms = _REFRESH_INTERVALS.get(key, 15_000)
            self._auto_timer.setInterval(ms)

    def _auto_refresh_tick(self) -> None:
        if self._bridge:
            self._bridge.invalidate_cache()
        self._load_watchlist()

    # -- fetch / chart ------------------------------------------------------

    def _fetch_data(self) -> None:
        if self._worker and self._worker.isRunning():
            return

        symbol = self._symbol_input.text().strip().upper()
        if not symbol:
            symbol = "AAPL"
            self._symbol_input.setText(symbol)

        source = self._source_combo.currentText()

        self._fetch_btn.setEnabled(False)
        self._fetch_btn.setText("Fetching...")
        self._results_card.setVisible(False)

        self._worker = FetchWorker(symbol, source)
        self._worker.finished_signal.connect(self._on_fetched)
        self._worker.error.connect(self._on_error)
        self._worker.start()

    def _on_fetched(self, result: dict) -> None:
        self._fetch_btn.setEnabled(True)
        self._fetch_btn.setText("Fetch")
        self._save_btn.setEnabled(True)
        self._last_data = result

        symbol = result.get("symbol", "")
        prices = result.get("prices", [])

        self._chart_heading.setText(f"Price Chart \u2014 {symbol}")
        self._price_chart.set_data(prices, symbol)

        self._stat_open.set_value(
            f"${result.get('open', 0):.2f}",
            C.TEXT,
        )
        self._stat_high.set_value(
            f"${result.get('high', 0):.2f}",
            C.GREEN,
        )
        self._stat_low.set_value(
            f"${result.get('low', 0):.2f}",
            C.RED,
        )
        self._stat_close.set_value(
            f"${result.get('close', 0):.2f}",
            C.TEXT,
        )

        vol = result.get("volume", 0)
        if vol >= 1_000_000:
            vol_str = f"{vol / 1_000_000:.1f}M"
        elif vol >= 1_000:
            vol_str = f"{vol / 1_000:.1f}K"
        else:
            vol_str = str(vol)
        self._stat_volume.set_value(vol_str, C.CYAN)
        self._stat_candles.set_value(
            str(result.get("candles", 0)),
            C.TEXT2,
        )

        self._results_card.setVisible(True)

    def _on_error(self, message: str) -> None:
        self._fetch_btn.setEnabled(True)
        self._fetch_btn.setText("Fetch")
        logger.error("Market data error: %s", message)

    def _save_csv(self) -> None:
        if not self._last_data:
            return

        symbol = self._last_data.get("symbol", "data")
        path, _ = QFileDialog.getSaveFileName(
            self,
            "Save Market Data",
            f"{symbol}_data.csv",
            "CSV Files (*.csv);;All Files (*)",
        )
        if path:
            try:
                prices = self._last_data.get("prices", [])
                with open(path, "w") as f:
                    f.write("index,close\n")
                    for i, price in enumerate(prices):
                        f.write(f"{i},{price:.4f}\n")
                logger.info("Saved market data to %s", path)
            except Exception as e:
                logger.error("Save error: %s", e)

    # -- refresh (called by F5 in main window) ------------------------------

    def refresh(self) -> None:
        if self._bridge:
            self._bridge.invalidate_cache()
        self._load_watchlist()
