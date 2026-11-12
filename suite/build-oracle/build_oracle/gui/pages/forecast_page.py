"""
Build Oracle -- Forecast Page

Data source selection, model configuration, forecast execution,
results display with QPainter chart.
"""

from PyQt6.QtCore import QPointF, QRectF, Qt, QThread, pyqtSignal
from PyQt6.QtGui import QColor, QPainter, QPen, QPolygonF
from PyQt6.QtWidgets import (
    QComboBox,
    QFileDialog,
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
# Forecast Worker Thread
# =============================================================================


class ForecastWorker(QThread):
    """Run forecast in background thread."""

    finished = pyqtSignal(dict)

    def __init__(self, series, model_name, horizon, parent=None):
        super().__init__(parent)
        self._series = series
        self._model_name = model_name
        self._horizon = horizon

    def run(self):
        import numpy as np

        arr = np.array(self._series, dtype=np.float64)
        train = arr[: -self._horizon] if self._horizon < len(arr) else arr
        actual = arr[-self._horizon :] if self._horizon < len(arr) else np.array([])

        if self._model_name == "arima":
            try:
                from build_oracle.arima import ARIMA

                model = ARIMA(p=2, d=1, q=2)
                model.fit(train)
                forecast = model.predict(self._horizon)
            except ImportError:
                x = np.arange(len(train))
                slope = np.polyfit(x, train, 1)[0]
                last_val = train[-1]
                forecast = np.array([last_val + slope * (i + 1) for i in range(self._horizon)])
        else:  # prophet
            try:
                from build_oracle.prophet import Prophet

                model = Prophet()
                model.fit(train)
                forecast = model.predict(self._horizon)
            except ImportError:
                period = 7
                forecast = np.array([train[-(period - i % period)] for i in range(self._horizon)])

        forecast = np.asarray(forecast, dtype=np.float64)[: self._horizon]

        # Confidence interval (simple +/- 1.96 * residual std)
        if len(actual) > 0 and len(forecast) >= len(actual):
            errors = actual - forecast[: len(actual)]
            mae = float(np.mean(np.abs(errors)))
            rmse = float(np.sqrt(np.mean(errors**2)))
            nonzero = actual[actual != 0]
            if len(nonzero) > 0:
                mape = float(np.mean(np.abs(errors[actual != 0] / nonzero)) * 100)
            else:
                mape = 0.0
            residual_std = float(np.std(errors))
        else:
            mae = 0.0
            rmse = 0.0
            mape = 0.0
            residual_std = float(np.std(train)) * 0.1

        ci_width = 1.96 * max(residual_std, 0.01)
        upper = forecast + ci_width
        lower = forecast - ci_width

        result = {
            "train": train.tolist(),
            "forecast": forecast.tolist(),
            "upper": upper.tolist(),
            "lower": lower.tolist(),
            "mae": mae,
            "rmse": rmse,
            "mape": mape,
        }
        self.finished.emit(result)


# =============================================================================
# Forecast Chart Widget
# =============================================================================


class ForecastChart(QWidget):
    """Custom QPainter chart showing historical + forecast data."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumHeight(280)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        self._train: list[float] = []
        self._forecast: list[float] = []
        self._upper: list[float] = []
        self._lower: list[float] = []

    def set_data(self, train, forecast, upper, lower):
        self._train = train
        self._forecast = forecast
        self._upper = upper
        self._lower = lower
        self.update()

    def paintEvent(self, event):
        if not self._train:
            p = QPainter(self)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Run a forecast to see results")
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        margin_l, margin_r, margin_t, margin_b = 55, 20, 20, 35
        plot_w = w - margin_l - margin_r
        plot_h = h - margin_t - margin_b

        # Combine all values for y-axis range
        all_vals = self._train + self._forecast + self._upper + self._lower
        y_min = min(all_vals)
        y_max = max(all_vals)
        y_range = y_max - y_min if y_max > y_min else 1.0
        y_min -= y_range * 0.05
        y_max += y_range * 0.05
        y_range = y_max - y_min

        total_pts = len(self._train) + len(self._forecast)
        if total_pts < 2:
            p.end()
            return

        def to_screen(idx, val):
            sx = margin_l + (idx / (total_pts - 1)) * plot_w
            sy = margin_t + (1 - (val - y_min) / y_range) * plot_h
            return QPointF(sx, sy)

        # Background
        p.fillRect(self.rect(), QColor(C.SURFACE))

        # Grid lines
        grid_pen = QPen(QColor(C.BORDER), 1)
        grid_pen.setStyle(Qt.PenStyle.DotLine)
        p.setPen(grid_pen)
        num_grid = 5
        for i in range(num_grid + 1):
            gy = margin_t + (i / num_grid) * plot_h
            p.drawLine(QPointF(margin_l, gy), QPointF(w - margin_r, gy))
            # Y-axis label
            val = y_max - (i / num_grid) * y_range
            p.setPen(QColor(C.TEXT3))
            p.drawText(
                QRectF(0, gy - 8, margin_l - 6, 16),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                f"{val:.1f}",
            )
            p.setPen(grid_pen)

        # X-axis labels
        p.setPen(QColor(C.TEXT3))
        x_labels = [0, total_pts // 4, total_pts // 2, 3 * total_pts // 4, total_pts - 1]
        for idx in x_labels:
            sx = margin_l + (idx / (total_pts - 1)) * plot_w
            p.drawText(QRectF(sx - 20, h - margin_b + 5, 40, 20), Qt.AlignmentFlag.AlignCenter, str(idx))

        # Axis labels
        p.setPen(QColor(C.TEXT2))
        p.drawText(QRectF(margin_l, h - 18, plot_w, 16), Qt.AlignmentFlag.AlignCenter, "Time Index")

        # Vertical line at forecast boundary
        boundary_x = margin_l + (len(self._train) / (total_pts - 1)) * plot_w
        boundary_pen = QPen(QColor(C.TEXT3), 1, Qt.PenStyle.DashLine)
        p.setPen(boundary_pen)
        p.drawLine(QPointF(boundary_x, margin_t), QPointF(boundary_x, margin_t + plot_h))

        # Confidence interval (shaded area)
        if self._upper and self._lower:
            ci_polygon = QPolygonF()
            start_idx = len(self._train)
            for i in range(len(self._upper)):
                ci_polygon.append(to_screen(start_idx + i, self._upper[i]))
            for i in range(len(self._lower) - 1, -1, -1):
                ci_polygon.append(to_screen(start_idx + i, self._lower[i]))

            ci_color = QColor(C.YELLOW)
            ci_color.setAlpha(40)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(ci_color)
            p.drawPolygon(ci_polygon)

        # Historical data -- solid line
        hist_pen = QPen(QColor(C.ACCENT_TX), 2)
        hist_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        hist_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setPen(hist_pen)
        p.setBrush(Qt.BrushStyle.NoBrush)

        # Draw only a tail of the history for clarity
        display_start = max(0, len(self._train) - 120)
        for i in range(display_start, len(self._train) - 1):
            p.drawLine(to_screen(i, self._train[i]), to_screen(i + 1, self._train[i + 1]))

        # Forecast -- dashed accent line
        fc_pen = QPen(QColor(C.GREEN), 2, Qt.PenStyle.DashLine)
        fc_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        p.setPen(fc_pen)
        start_idx = len(self._train)

        # Connect last historical point to first forecast point
        if self._train and self._forecast:
            p.drawLine(to_screen(len(self._train) - 1, self._train[-1]), to_screen(start_idx, self._forecast[0]))

        for i in range(len(self._forecast) - 1):
            p.drawLine(to_screen(start_idx + i, self._forecast[i]), to_screen(start_idx + i + 1, self._forecast[i + 1]))

        # Legend
        legend_x = margin_l + 10
        legend_y = margin_t + 10

        p.setPen(QPen(QColor(C.ACCENT_TX), 2))
        p.drawLine(QPointF(legend_x, legend_y + 6), QPointF(legend_x + 20, legend_y + 6))
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(legend_x + 25, legend_y + 10), "Historical")

        p.setPen(QPen(QColor(C.GREEN), 2, Qt.PenStyle.DashLine))
        p.drawLine(QPointF(legend_x, legend_y + 22), QPointF(legend_x + 20, legend_y + 22))
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(legend_x + 25, legend_y + 26), "Forecast")

        ci_swatch = QColor(C.YELLOW)
        ci_swatch.setAlpha(80)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(ci_swatch)
        p.drawRect(QRectF(legend_x, legend_y + 34, 20, 10))
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(legend_x + 25, legend_y + 42), "95% CI")

        p.end()


# =============================================================================
# Forecast Page
# =============================================================================


class ForecastPage(QWidget):
    """Forecast configuration, execution, and visualization."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._worker: ForecastWorker | None = None
        self._csv_path: str | None = None

        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(16)

        layout.addWidget(Heading("Forecast"))

        subtitle = QLabel("Configure and run time series forecasts")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Configuration Card ---
        config_card, config_layout = Card.with_layout(QGridLayout, margins=(20, 16, 20, 16), spacing=12)

        # Data source
        config_layout.addWidget(QLabel("Data Source:"), 0, 0)
        self._data_combo = QComboBox()
        self._data_combo.addItems(["Sample (365 pts)", "Load CSV..."])
        self._data_combo.currentIndexChanged.connect(self._on_data_changed)
        config_layout.addWidget(self._data_combo, 0, 1)

        self._data_label = QLabel("Using sample data")
        self._data_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
        config_layout.addWidget(self._data_label, 0, 2)

        # Model
        config_layout.addWidget(QLabel("Model:"), 1, 0)
        self._model_combo = QComboBox()
        self._model_combo.addItems(["ARIMA", "Prophet"])
        config_layout.addWidget(self._model_combo, 1, 1)

        # Horizon
        config_layout.addWidget(QLabel("Horizon:"), 2, 0)
        self._horizon_spin = QSpinBox()
        self._horizon_spin.setRange(1, 365)
        self._horizon_spin.setValue(30)
        self._horizon_spin.setSuffix(" steps")
        config_layout.addWidget(self._horizon_spin, 2, 1)

        # Run button
        self._run_btn = QPushButton("Forecast")
        self._run_btn.setProperty("primary", True)
        self._run_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._run_btn.setFixedHeight(42)
        self._run_btn.clicked.connect(self._run_forecast)
        config_layout.addWidget(self._run_btn, 3, 0, 1, 3)

        layout.addWidget(config_card)

        # --- Results Stats ---
        layout.addWidget(Heading("Results", level=2))

        results_card, results_layout = Card.with_layout(QHBoxLayout, margins=(24, 16, 24, 16))

        self._stat_mae = Stat("MAE", "--", C.ACCENT_TX)
        results_layout.addWidget(self._stat_mae)

        self._stat_rmse = Stat("RMSE", "--", C.GREEN)
        results_layout.addWidget(self._stat_rmse)

        self._stat_mape = Stat("MAPE", "--", C.CYAN)
        results_layout.addWidget(self._stat_mape)

        layout.addWidget(results_card)

        # --- Chart ---
        layout.addWidget(Heading("Forecast Chart", level=2))

        chart_card = Card()
        chart_layout = QVBoxLayout(chart_card)
        chart_layout.setContentsMargins(12, 12, 12, 12)

        self._chart = ForecastChart()
        chart_layout.addWidget(self._chart)

        layout.addWidget(chart_card)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _on_data_changed(self, index):
        if index == 1:
            path, _ = QFileDialog.getOpenFileName(self, "Load CSV Data", "", "CSV Files (*.csv);;All Files (*)")
            if path:
                self._csv_path = path
                from pathlib import Path

                self._data_label.setText(f"File: {Path(path).name}")
            else:
                self._data_combo.setCurrentIndex(0)
                self._data_label.setText("Using sample data")
        else:
            self._csv_path = None
            self._data_label.setText("Using sample data")

    def _run_forecast(self):
        self._run_btn.setEnabled(False)
        self._run_btn.setText("Running...")

        # Get data
        if self._csv_path:
            try:
                import numpy as np

                data = np.loadtxt(self._csv_path, delimiter=",", usecols=-1, skiprows=1)
                series = data.tolist()
            except Exception as e:
                self._run_btn.setEnabled(True)
                self._run_btn.setText("Forecast")
                if self._main_window:
                    self._main_window.show_toast(f"CSV load error: {e}", "warning")
                return
        else:
            from build_oracle.cli import generate_sample_series

            series = generate_sample_series(n=365)

        model_name = self._model_combo.currentText().lower()
        horizon = self._horizon_spin.value()

        self._worker = ForecastWorker(series, model_name, horizon)
        self._worker.finished.connect(self._on_forecast_done)
        self._worker.start()

    def _on_forecast_done(self, result: dict):
        self._run_btn.setEnabled(True)
        self._run_btn.setText("Forecast")

        # Update stats
        self._stat_mae.set_value(f"{result['mae']:.4f}")
        self._stat_rmse.set_value(f"{result['rmse']:.4f}")
        self._stat_mape.set_value(f"{result['mape']:.1f}%")

        # Update chart
        self._chart.set_data(
            result["train"],
            result["forecast"],
            result["upper"],
            result["lower"],
        )

        if self._main_window:
            self._main_window.show_toast("Forecast complete", "success")
