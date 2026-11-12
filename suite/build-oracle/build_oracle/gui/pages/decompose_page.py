"""
Build Oracle -- Decompose Page

Time series decomposition with trend, seasonal, and residual charts.
"""

from PyQt6.QtCore import QPointF, QRectF, Qt, QThread, pyqtSignal
from PyQt6.QtGui import QColor, QPainter, QPen
from PyQt6.QtWidgets import (
    QComboBox,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from build_oracle.gui.app import C, Card, Heading, Stat

# =============================================================================
# Decompose Worker Thread
# =============================================================================


class DecomposeWorker(QThread):
    """Run decomposition in background thread."""

    finished = pyqtSignal(dict)

    def __init__(self, series, period, model, parent=None):
        super().__init__(parent)
        self._series = series
        self._period = period
        self._model = model

    def run(self):
        import numpy as np

        arr = np.array(self._series, dtype=np.float64)
        period = self._period

        try:
            from build_oracle.decompose import classical_decompose

            result = classical_decompose(arr, period=period, model=self._model)
            trend = result["trend"]
            seasonal = result["seasonal"]
            residual = result["residual"]
        except (ImportError, KeyError):
            # Fallback: simple moving-average decomposition
            kernel = np.ones(period) / period
            trend = np.convolve(arr, kernel, mode="same")
            detrended = arr - trend
            seasonal = np.zeros_like(arr)
            for i in range(period):
                indices = list(range(i, len(arr), period))
                mean_val = np.mean(detrended[indices])
                for idx in indices:
                    seasonal[idx] = mean_val
            residual = arr - trend - seasonal

        # Replace NaN with interpolated values for display
        trend_display = np.copy(trend)
        residual_display = np.copy(residual)
        nan_mask = np.isnan(trend_display)
        if np.any(nan_mask) and not np.all(nan_mask):
            valid = np.where(~nan_mask)[0]
            trend_display[nan_mask] = np.interp(np.where(nan_mask)[0], valid, trend_display[valid])
            residual_display = arr - trend_display - seasonal

        # Strength measures (using clean data)
        mask = ~(np.isnan(trend) | np.isnan(seasonal) | np.isnan(residual))
        if np.any(mask):
            var_remainder = float(np.var(residual[mask]))
            var_detrended = float(np.var((arr - trend)[mask]))
            var_deseasoned = float(np.var((arr - seasonal)[mask]))
        else:
            var_remainder = var_detrended = var_deseasoned = 0

        trend_strength = max(0, 1 - var_remainder / var_deseasoned) if var_deseasoned > 0 else 0
        seasonal_strength = max(0, 1 - var_remainder / var_detrended) if var_detrended > 0 else 0

        self.finished.emit(
            {
                "trend": trend_display.tolist(),
                "seasonal": seasonal.tolist(),
                "residual": residual_display.tolist(),
                "trend_strength": trend_strength,
                "seasonal_strength": seasonal_strength,
            }
        )


# =============================================================================
# Mini Line Chart Widget
# =============================================================================


class MiniChart(QWidget):
    """Small chart for a single component (trend, seasonal, or residual)."""

    def __init__(self, title: str, color: str, scatter: bool = False, parent=None):
        super().__init__(parent)
        self._title = title
        self._color = color
        self._scatter = scatter
        self._data: list[float] = []
        self.setFixedHeight(120)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)

    def set_data(self, data: list[float]):
        self._data = data
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        margin_l, margin_r, margin_t, margin_b = 50, 15, 22, 10
        plot_w = w - margin_l - margin_r
        plot_h = h - margin_t - margin_b

        # Background
        p.fillRect(self.rect(), QColor(C.SURFACE))

        # Title
        p.setPen(QColor(C.TEXT2))
        p.drawText(
            QRectF(margin_l, 2, plot_w, 18), Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, self._title
        )

        if not self._data:
            p.setPen(QColor(C.TEXT3))
            p.drawText(QRectF(margin_l, margin_t, plot_w, plot_h), Qt.AlignmentFlag.AlignCenter, "No data")
            p.end()
            return

        n = len(self._data)
        y_min = min(self._data)
        y_max = max(self._data)
        y_range = y_max - y_min if y_max > y_min else 1.0
        y_min -= y_range * 0.05
        y_max += y_range * 0.05
        y_range = y_max - y_min

        def to_screen(idx, val):
            sx = margin_l + (idx / max(1, n - 1)) * plot_w
            sy = margin_t + (1 - (val - y_min) / y_range) * plot_h
            return QPointF(sx, sy)

        # Grid
        grid_pen = QPen(QColor(C.BORDER), 1, Qt.PenStyle.DotLine)
        p.setPen(grid_pen)
        for i in range(3):
            gy = margin_t + (i / 2.0) * plot_h
            p.drawLine(QPointF(margin_l, gy), QPointF(w - margin_r, gy))

        # Y-axis labels
        p.setPen(QColor(C.TEXT3))
        for i in range(3):
            gy = margin_t + (i / 2.0) * plot_h
            val = y_max - (i / 2.0) * y_range
            p.drawText(
                QRectF(0, gy - 7, margin_l - 5, 14),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                f"{val:.1f}",
            )

        if self._scatter:
            # Residual scatter
            p.setPen(Qt.PenStyle.NoPen)
            dot_color = QColor(self._color)
            dot_color.setAlpha(120)
            p.setBrush(dot_color)
            step = max(1, n // 200)
            for i in range(0, n, step):
                pt = to_screen(i, self._data[i])
                p.drawEllipse(pt, 2, 2)
            # Zero line
            zero_pen = QPen(QColor(C.TEXT3), 1, Qt.PenStyle.DashLine)
            p.setPen(zero_pen)
            if y_min <= 0 <= y_max:
                zy = margin_t + (1 - (0 - y_min) / y_range) * plot_h
                p.drawLine(QPointF(margin_l, zy), QPointF(w - margin_r, zy))
        else:
            # Line chart
            line_pen = QPen(QColor(self._color), 2)
            line_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
            line_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
            p.setPen(line_pen)
            p.setBrush(Qt.BrushStyle.NoBrush)
            step = max(1, n // 400)
            prev = to_screen(0, self._data[0])
            for i in range(step, n, step):
                cur = to_screen(i, self._data[i])
                p.drawLine(prev, cur)
                prev = cur

        p.end()


# =============================================================================
# Decompose Page
# =============================================================================


class DecomposePage(QWidget):
    """Time series decomposition configuration and visualization."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._worker: DecomposeWorker | None = None

        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(16)

        layout.addWidget(Heading("Decompose"))

        subtitle = QLabel("Break down a time series into trend, seasonal, and residual components")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Config Card ---
        config_card, config_layout = Card.with_layout(QGridLayout, margins=(20, 16, 20, 16), spacing=12)

        config_layout.addWidget(QLabel("Period:"), 0, 0)
        self._period_spin = QSpinBox()
        self._period_spin.setRange(2, 365)
        self._period_spin.setValue(7)
        self._period_spin.setSuffix(" (weekly)")
        config_layout.addWidget(self._period_spin, 0, 1)

        config_layout.addWidget(QLabel("Model:"), 1, 0)
        self._model_combo = QComboBox()
        self._model_combo.addItems(["additive", "multiplicative"])
        config_layout.addWidget(self._model_combo, 1, 1)

        self._run_btn = QPushButton("Decompose")
        self._run_btn.setProperty("primary", True)
        self._run_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._run_btn.setFixedHeight(42)
        self._run_btn.clicked.connect(self._run_decompose)
        config_layout.addWidget(self._run_btn, 2, 0, 1, 2)

        layout.addWidget(config_card)

        # --- Strength Stats ---
        layout.addWidget(Heading("Component Strengths", level=2))

        stats_card, stats_layout = Card.with_layout(QHBoxLayout, margins=(24, 16, 24, 16))

        self._stat_trend = Stat("Trend Strength", "--", C.ACCENT_TX)
        stats_layout.addWidget(self._stat_trend)

        self._stat_seasonal = Stat("Seasonal Strength", "--", C.GREEN)
        stats_layout.addWidget(self._stat_seasonal)

        layout.addWidget(stats_card)

        # --- Charts ---
        layout.addWidget(Heading("Components", level=2))

        charts_card = Card()
        charts_layout = QVBoxLayout(charts_card)
        charts_layout.setContentsMargins(12, 12, 12, 12)
        charts_layout.setSpacing(4)

        self._trend_chart = MiniChart("Trend", C.ACCENT_TX)
        charts_layout.addWidget(self._trend_chart)

        self._seasonal_chart = MiniChart("Seasonal", C.GREEN)
        charts_layout.addWidget(self._seasonal_chart)

        self._residual_chart = MiniChart("Residual", C.CYAN, scatter=True)
        charts_layout.addWidget(self._residual_chart)

        layout.addWidget(charts_card)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _run_decompose(self):
        self._run_btn.setEnabled(False)
        self._run_btn.setText("Decomposing...")

        from build_oracle.cli import generate_sample_series

        series = generate_sample_series(n=365)

        period = self._period_spin.value()
        model = self._model_combo.currentText()

        self._worker = DecomposeWorker(series, period, model)
        self._worker.finished.connect(self._on_decompose_done)
        self._worker.start()

    def _on_decompose_done(self, result: dict):
        self._run_btn.setEnabled(True)
        self._run_btn.setText("Decompose")

        self._stat_trend.set_value(f"{result['trend_strength']:.2f}")
        self._stat_seasonal.set_value(f"{result['seasonal_strength']:.2f}")

        self._trend_chart.set_data(result["trend"])
        self._seasonal_chart.set_data(result["seasonal"])
        self._residual_chart.set_data(result["residual"])

        if self._main_window:
            self._main_window.show_toast("Decomposition complete", "success")
