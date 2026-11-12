"""
Market Data -- data management page.

Fetch market data from multiple sources, visualize price charts,
inspect data statistics, and export to CSV.
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
)
from PyQt6.QtWidgets import (
    QComboBox,
    QFileDialog,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from build_engine.gui.app import C, Card, Heading


class PriceChartWidget(QWidget):
    """Line chart for OHLCV price data."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(220)
        self.setMinimumWidth(400)
        self._closes = []
        self._highs = []
        self._lows = []
        self._symbol = ""

    def set_data(self, closes: list, highs: list = None, lows: list = None, symbol: str = ""):
        self._closes = closes
        self._highs = highs or []
        self._lows = lows or []
        self._symbol = symbol
        self.update()

    def paintEvent(self, event):
        if len(self._closes) < 2:
            p = QPainter(self)
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Fetch data to see price chart")
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        pad_x = 55
        pad_y = 25
        chart_w = w - pad_x * 2
        chart_h = h - pad_y * 2

        data = self._closes
        all_vals = list(data)
        if self._highs:
            all_vals += self._highs
        if self._lows:
            all_vals += self._lows
        n = len(data)
        min_v = min(all_vals) * 0.998
        max_v = max(all_vals) * 1.002
        val_range = max_v - min_v if max_v != min_v else 1.0

        def x_pos(i):
            return pad_x + (i / max(n - 1, 1)) * chart_w

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
                f"${val:,.2f}" if val < 1000 else f"${val:,.0f}",
            )
            p.setPen(QPen(QColor(C.BORDER), 1))

        # High/Low band if available
        if self._highs and self._lows and len(self._highs) == n:
            band_poly = QPolygonF()
            for i in range(n):
                band_poly.append(QPointF(x_pos(i), y_pos(self._highs[i])))
            for i in range(n - 1, -1, -1):
                band_poly.append(QPointF(x_pos(i), y_pos(self._lows[i])))

            band_color = QColor(C.CYAN)
            band_color.setAlpha(20)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(band_color)
            p.drawPolygon(band_poly)

        # Close line
        points = [QPointF(x_pos(i), y_pos(v)) for i, v in enumerate(data)]

        # Fill
        fill_poly = QPolygonF()
        fill_poly.append(QPointF(points[0].x(), pad_y + chart_h))
        for pt in points:
            fill_poly.append(pt)
        fill_poly.append(QPointF(points[-1].x(), pad_y + chart_h))

        grad = QLinearGradient(0, pad_y, 0, pad_y + chart_h)
        c1 = QColor(C.CYAN)
        c1.setAlpha(50)
        c2 = QColor(C.CYAN)
        c2.setAlpha(5)
        grad.setColorAt(0, c1)
        grad.setColorAt(1, c2)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QBrush(grad))
        p.drawPolygon(fill_poly)

        # Line
        pen = QPen(QColor(C.CYAN), 2)
        p.setPen(pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        for i in range(len(points) - 1):
            p.drawLine(points[i], points[i + 1])

        # Current price
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.CYAN))
        p.drawEllipse(points[-1], 4, 4)

        # Symbol label
        if self._symbol:
            p.setPen(QColor(C.TEXT))
            p.setFont(QFont("Segoe UI", 12, QFont.Weight.Bold))
            p.drawText(QPointF(pad_x + 8, pad_y + 16), self._symbol)

            # Current price text
            p.setFont(QFont("Segoe UI", 10))
            p.setPen(QColor(C.CYAN))
            p.drawText(QPointF(pad_x + 8, pad_y + 32), f"${data[-1]:,.2f}")

        p.end()


class FetchWorker(QThread):
    """Fetch market data in a background thread."""

    data_ready = pyqtSignal(dict)
    error = pyqtSignal(str)
    status = pyqtSignal(str)

    def __init__(self, symbol: str, source: str, parent=None):
        super().__init__(parent)
        self._symbol = symbol
        self._source = source

    def run(self):
        self.status.emit(f"Fetching {self._symbol} from {self._source}...")

        try:
            if self._source == "Yahoo Finance":
                try:
                    from build_finance.market_data import fetch_yahoo

                    candles = fetch_yahoo(self._symbol, period="6mo", interval="1d")
                    if candles:
                        closes = [c.close for c in candles]
                        highs = [c.high for c in candles]
                        lows = [c.low for c in candles]
                        volumes = [getattr(c, "volume", 0) for c in candles]

                        self.data_ready.emit(
                            {
                                "symbol": self._symbol,
                                "closes": closes,
                                "highs": highs,
                                "lows": lows,
                                "volumes": volumes,
                                "candle_count": len(candles),
                                "source": self._source,
                            }
                        )
                        return
                except ImportError:
                    pass

            # Synthetic fallback
            self.status.emit(f"Generating synthetic data for {self._symbol}...")
            n = 180
            price = 100 + random.random() * 200
            closes = []
            highs = []
            lows = []
            volumes = []
            for _ in range(n):
                change = random.gauss(0.0003, 0.018)
                price *= 1 + change
                h = price * (1 + abs(random.gauss(0, 0.008)))
                lo = price * (1 - abs(random.gauss(0, 0.008)))
                vol = int(random.gauss(5_000_000, 2_000_000))
                closes.append(price)
                highs.append(h)
                lows.append(lo)
                volumes.append(max(vol, 100_000))

            self.data_ready.emit(
                {
                    "symbol": self._symbol,
                    "closes": closes,
                    "highs": highs,
                    "lows": lows,
                    "volumes": volumes,
                    "candle_count": n,
                    "source": f"{self._source} (synthetic)",
                }
            )

        except Exception as exc:
            self.error.emit(str(exc))


class DataPage(QWidget):
    """Market data management page."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._worker = None
        self._current_data = {}
        self._recent_symbols = []
        self._build_ui()

    def _build_ui(self):
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(18)

        layout.addWidget(Heading("Market Data"))

        # Fetch controls
        card_fetch, fetch_lay = Card.with_layout(layout_cls=QHBoxLayout)

        sym_col = QVBoxLayout()
        sym_col.setSpacing(4)
        sym_col.addWidget(QLabel("Symbol"))
        self._symbol_input = QLineEdit("AAPL")
        self._symbol_input.setFixedWidth(140)
        self._symbol_input.returnPressed.connect(self._fetch_data)
        sym_col.addWidget(self._symbol_input)
        fetch_lay.addLayout(sym_col)

        src_col = QVBoxLayout()
        src_col.setSpacing(4)
        src_col.addWidget(QLabel("Source"))
        self._source_combo = QComboBox()
        self._source_combo.addItems(["Yahoo Finance", "CoinGecko", "CSV File"])
        self._source_combo.setFixedWidth(140)
        src_col.addWidget(self._source_combo)
        fetch_lay.addLayout(src_col)

        fetch_lay.addStretch()

        self._btn_fetch = QPushButton("Fetch")
        self._btn_fetch.setProperty("primary", True)
        self._btn_fetch.setFixedHeight(38)
        self._btn_fetch.setCursor(Qt.CursorShape.PointingHandCursor)
        self._btn_fetch.clicked.connect(self._fetch_data)
        fetch_lay.addWidget(self._btn_fetch)

        self._btn_save = QPushButton("Save CSV")
        self._btn_save.setFixedHeight(38)
        self._btn_save.setCursor(Qt.CursorShape.PointingHandCursor)
        self._btn_save.clicked.connect(self._save_csv)
        self._btn_save.setEnabled(False)
        fetch_lay.addWidget(self._btn_save)

        layout.addWidget(card_fetch)

        # Price chart
        card_chart, chart_lay = Card.with_layout()
        chart_lay.addWidget(Heading("Price Chart", level=2))
        self._price_chart = PriceChartWidget()
        chart_lay.addWidget(self._price_chart)
        layout.addWidget(card_chart)

        # Data info row
        row_info = QHBoxLayout()
        row_info.setSpacing(16)

        # Stats card
        card_stats, stats_lay = Card.with_layout()
        stats_lay.addWidget(Heading("Data Info", level=2))

        stats_grid = QGridLayout()
        stats_grid.setSpacing(8)
        stats_grid.setColumnMinimumWidth(0, 100)

        self._info_labels = {}
        info_items = [
            ("Source", "--"),
            ("Candles", "--"),
            ("Date Range", "--"),
            ("Latest Close", "--"),
            ("High", "--"),
            ("Low", "--"),
            ("Avg Volume", "--"),
            ("Volatility", "--"),
        ]
        for row, (label, default) in enumerate(info_items):
            key_label = QLabel(f"{label}:")
            key_label.setStyleSheet(f"color: {C.TEXT2}; font-size: 12px;")
            val_label = QLabel(default)
            val_label.setStyleSheet(f"color: {C.TEXT}; font-weight: 500; font-size: 12px;")
            stats_grid.addWidget(key_label, row, 0)
            stats_grid.addWidget(val_label, row, 1)
            self._info_labels[label] = val_label

        stats_lay.addLayout(stats_grid)
        stats_lay.addStretch()
        row_info.addWidget(card_stats, stretch=1)

        # Recent symbols card
        card_recent, recent_lay = Card.with_layout()
        recent_lay.addWidget(Heading("Recent Symbols", level=2))

        self._recent_container = QVBoxLayout()
        self._recent_container.setSpacing(4)

        # Initial placeholder
        placeholder = QLabel("No symbols fetched yet")
        placeholder.setStyleSheet(f"color: {C.TEXT3}; font-size: 12px; padding: 8px;")
        self._recent_container.addWidget(placeholder)

        recent_lay.addLayout(self._recent_container)
        recent_lay.addStretch()
        row_info.addWidget(card_recent, stretch=1)

        layout.addLayout(row_info)
        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _fetch_data(self):
        symbol = self._symbol_input.text().strip().upper()
        if not symbol:
            return

        if self._worker and self._worker.isRunning():
            return

        self._btn_fetch.setEnabled(False)
        self._btn_fetch.setText("Fetching...")

        source = self._source_combo.currentText()
        self._worker = FetchWorker(symbol, source)
        self._worker.data_ready.connect(self._on_data_ready)
        self._worker.error.connect(self._on_error)
        self._worker.status.connect(self._on_status)
        self._worker.finished.connect(self._on_finished)
        self._worker.start()

    def _on_data_ready(self, data: dict):
        self._current_data = data
        symbol = data["symbol"]
        closes = data["closes"]
        highs = data.get("highs", [])
        lows = data.get("lows", [])
        volumes = data.get("volumes", [])

        # Update chart
        self._price_chart.set_data(closes, highs, lows, symbol)

        # Update info
        self._info_labels["Source"].setText(data.get("source", "--"))
        self._info_labels["Candles"].setText(str(data.get("candle_count", len(closes))))
        self._info_labels["Date Range"].setText(f"{len(closes)} trading days")
        self._info_labels["Latest Close"].setText(f"${closes[-1]:,.2f}")

        if highs:
            self._info_labels["High"].setText(f"${max(highs):,.2f}")
        if lows:
            self._info_labels["Low"].setText(f"${min(lows):,.2f}")
        if volumes:
            avg_vol = sum(volumes) / len(volumes)
            self._info_labels["Avg Volume"].setText(f"{avg_vol:,.0f}")

        # Volatility
        if len(closes) > 1:
            returns = [(closes[i] - closes[i - 1]) / closes[i - 1] for i in range(1, len(closes))]
            avg_ret = sum(returns) / len(returns)
            variance = sum((r - avg_ret) ** 2 for r in returns) / (len(returns) - 1)
            vol = variance**0.5 * (252**0.5) * 100
            self._info_labels["Volatility"].setText(f"{vol:.1f}%")

        self._btn_save.setEnabled(True)

        # Add to recent
        if symbol not in self._recent_symbols:
            self._recent_symbols.insert(0, symbol)
            if len(self._recent_symbols) > 10:
                self._recent_symbols.pop()
            self._update_recent_list()

        if self._main_window:
            self._main_window.show_toast(f"Loaded {symbol}: {len(closes)} candles", "success")

    def _update_recent_list(self):
        # Clear existing
        while self._recent_container.count():
            item = self._recent_container.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        for sym in self._recent_symbols:
            btn = QPushButton(sym)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.setStyleSheet(
                f"QPushButton {{ background: {C.SURFACE2}; border: 1px solid {C.BORDER}; "
                f"border-radius: 8px; padding: 6px 14px; text-align: left; "
                f"font-weight: 500; }}"
                f"QPushButton:hover {{ border-color: {C.ACCENT}; }}"
            )
            btn.clicked.connect(lambda checked, s=sym: self._load_symbol(s))
            self._recent_container.addWidget(btn)

    def _load_symbol(self, symbol: str):
        self._symbol_input.setText(symbol)
        self._fetch_data()

    def _on_error(self, msg: str):
        if self._main_window:
            self._main_window.show_toast(f"Fetch error: {msg}", "error")

    def _on_status(self, msg: str):
        if self._main_window:
            self._main_window._status.setText(msg)

    def _on_finished(self):
        self._btn_fetch.setEnabled(True)
        self._btn_fetch.setText("Fetch")
        self._worker = None

    def _save_csv(self):
        if not self._current_data:
            return

        symbol = self._current_data.get("symbol", "data")
        path, _ = QFileDialog.getSaveFileName(
            self, "Save CSV", f"{symbol}_data.csv", "CSV Files (*.csv);;All Files (*)"
        )
        if not path:
            return

        try:
            closes = self._current_data.get("closes", [])
            highs = self._current_data.get("highs", [])
            lows = self._current_data.get("lows", [])
            volumes = self._current_data.get("volumes", [])

            with open(path, "w") as f:
                f.write("index,close,high,low,volume\n")
                for i in range(len(closes)):
                    h = highs[i] if i < len(highs) else ""
                    lo = lows[i] if i < len(lows) else ""
                    v = volumes[i] if i < len(volumes) else ""
                    f.write(f"{i},{closes[i]:.4f},{h},{lo},{v}\n")

            if self._main_window:
                self._main_window.show_toast(f"Saved {symbol} to CSV", "success")

        except Exception as exc:
            if self._main_window:
                self._main_window.show_toast(f"Save error: {exc}", "error")

    def refresh(self):
        if self._current_data:
            self._fetch_data()
