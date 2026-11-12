"""
Build Oracle -- Changepoint Detection Page

Detect structural breaks in time series with configurable penalties.
"""

from PyQt6.QtCore import QPointF, QRectF, Qt, QThread, pyqtSignal
from PyQt6.QtGui import QColor, QPainter, QPen
from PyQt6.QtWidgets import (
    QAbstractItemView,
    QComboBox,
    QGridLayout,
    QHeaderView,
    QLabel,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from build_oracle.gui.app import C, Card, Heading

# =============================================================================
# Changepoint Worker Thread
# =============================================================================


class ChangepointWorker(QThread):
    """Run changepoint detection in background thread."""

    finished = pyqtSignal(dict)

    def __init__(self, series, penalty, min_segment, parent=None):
        super().__init__(parent)
        self._series = series
        self._penalty = penalty
        self._min_segment = min_segment

    def run(self):
        import numpy as np

        arr = np.array(self._series, dtype=np.float64)

        try:
            from build_oracle.changepoint import pelt as pelt_detect

            cps = pelt_detect(arr, penalty=self._penalty, min_segment=self._min_segment)
        except ImportError:
            # Fallback: CUSUM-like detection
            cps = []
            mean_val = float(np.mean(arr))
            cumsum = 0.0
            threshold = float(np.std(arr)) * 3.0
            last_cp = 0
            for i in range(1, len(arr)):
                cumsum += arr[i] - mean_val
                if abs(cumsum) > threshold and (i - last_cp) >= self._min_segment:
                    cps.append(i)
                    cumsum = 0.0
                    last_cp = i
                    mean_val = float(np.mean(arr[i : min(i + 30, len(arr))]))

        # Compute stats for each changepoint
        cp_data = []
        for cp in cps:
            left_seg = arr[max(0, cp - 20) : cp]
            right_seg = arr[cp : min(len(arr), cp + 20)]
            left_mean = float(np.mean(left_seg)) if len(left_seg) > 0 else 0.0
            right_mean = float(np.mean(right_seg)) if len(right_seg) > 0 else 0.0
            shift = abs(right_mean - left_mean)
            confidence = min(1.0, shift / (float(np.std(arr)) + 1e-8))
            cp_data.append(
                {
                    "index": cp,
                    "confidence": confidence,
                    "left_mean": left_mean,
                    "right_mean": right_mean,
                }
            )

        # Compute segment means
        boundaries = [0] + cps + [len(arr)]
        segment_means = []
        for i in range(len(boundaries) - 1):
            seg = arr[boundaries[i] : boundaries[i + 1]]
            segment_means.append(float(np.mean(seg)))

        self.finished.emit(
            {
                "series": arr.tolist(),
                "changepoints": cp_data,
                "segment_boundaries": boundaries,
                "segment_means": segment_means,
            }
        )


# =============================================================================
# Changepoint Chart Widget
# =============================================================================


class ChangepointChart(QWidget):
    """Chart showing time series with vertical lines at changepoints."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumHeight(260)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

        self._series: list[float] = []
        self._changepoints: list[dict] = []
        self._segment_boundaries: list[int] = []
        self._segment_means: list[float] = []

    def set_data(self, series, changepoints, boundaries, means):
        self._series = series
        self._changepoints = changepoints
        self._segment_boundaries = boundaries
        self._segment_means = means
        self.update()

    def paintEvent(self, event):
        if not self._series:
            p = QPainter(self)
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Run detection to see results")
            p.end()
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        margin_l, margin_r, margin_t, margin_b = 55, 20, 20, 35
        plot_w = w - margin_l - margin_r
        plot_h = h - margin_t - margin_b

        n = len(self._series)
        y_min = min(self._series)
        y_max = max(self._series)
        y_range = y_max - y_min if y_max > y_min else 1.0
        y_min -= y_range * 0.05
        y_max += y_range * 0.05
        y_range = y_max - y_min

        def to_screen(idx, val):
            sx = margin_l + (idx / max(1, n - 1)) * plot_w
            sy = margin_t + (1 - (val - y_min) / y_range) * plot_h
            return QPointF(sx, sy)

        # Background
        p.fillRect(self.rect(), QColor(C.SURFACE))

        # Grid
        grid_pen = QPen(QColor(C.BORDER), 1, Qt.PenStyle.DotLine)
        p.setPen(grid_pen)
        for i in range(6):
            gy = margin_t + (i / 5.0) * plot_h
            p.drawLine(QPointF(margin_l, gy), QPointF(w - margin_r, gy))
            val = y_max - (i / 5.0) * y_range
            p.setPen(QColor(C.TEXT3))
            p.drawText(
                QRectF(0, gy - 8, margin_l - 6, 16),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                f"{val:.1f}",
            )
            p.setPen(grid_pen)

        # Segment mean lines (horizontal colored lines for each segment)
        for i in range(len(self._segment_boundaries) - 1):
            start = self._segment_boundaries[i]
            end = self._segment_boundaries[i + 1]
            mean_val = self._segment_means[i]

            mean_pen = QPen(QColor(C.YELLOW), 2, Qt.PenStyle.DashDotLine)
            p.setPen(mean_pen)
            pt_start = to_screen(start, mean_val)
            pt_end = to_screen(min(end, n - 1), mean_val)
            p.drawLine(pt_start, pt_end)

        # Time series line
        line_pen = QPen(QColor(C.ACCENT_TX), 1.5)
        line_pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        line_pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setPen(line_pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        step = max(1, n // 500)
        prev = to_screen(0, self._series[0])
        for i in range(step, n, step):
            cur = to_screen(i, self._series[i])
            p.drawLine(prev, cur)
            prev = cur

        # Changepoint vertical lines
        for cp in self._changepoints:
            idx = cp["index"]
            sx = margin_l + (idx / max(1, n - 1)) * plot_w

            # Vertical line
            cp_pen = QPen(QColor(C.RED), 2, Qt.PenStyle.DashLine)
            p.setPen(cp_pen)
            p.drawLine(QPointF(sx, margin_t), QPointF(sx, margin_t + plot_h))

            # Label
            p.setPen(QColor(C.RED))
            p.drawText(QRectF(sx - 15, margin_t - 16, 30, 14), Qt.AlignmentFlag.AlignCenter, str(idx))

        # X-axis
        p.setPen(QColor(C.TEXT2))
        p.drawText(QRectF(margin_l, h - 18, plot_w, 16), Qt.AlignmentFlag.AlignCenter, "Time Index")

        # Legend
        lx = margin_l + 10
        ly = margin_t + 10

        p.setPen(QPen(QColor(C.ACCENT_TX), 1.5))
        p.drawLine(QPointF(lx, ly + 6), QPointF(lx + 18, ly + 6))
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(lx + 22, ly + 10), "Series")

        p.setPen(QPen(QColor(C.RED), 2, Qt.PenStyle.DashLine))
        p.drawLine(QPointF(lx, ly + 22), QPointF(lx + 18, ly + 22))
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(lx + 22, ly + 26), "Changepoints")

        p.setPen(QPen(QColor(C.YELLOW), 2, Qt.PenStyle.DashDotLine))
        p.drawLine(QPointF(lx, ly + 38), QPointF(lx + 18, ly + 38))
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(lx + 22, ly + 42), "Segment Means")

        p.end()


# =============================================================================
# Changepoint Page
# =============================================================================


class ChangepointPage(QWidget):
    """Changepoint detection configuration, visualization, and results table."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._worker: ChangepointWorker | None = None

        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(16)

        layout.addWidget(Heading("Changepoint Detection"))

        subtitle = QLabel("Detect structural breaks and level shifts in time series")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Config Card ---
        config_card, config_layout = Card.with_layout(QGridLayout, margins=(20, 16, 20, 16), spacing=12)

        config_layout.addWidget(QLabel("Penalty:"), 0, 0)
        self._penalty_combo = QComboBox()
        self._penalty_combo.addItems(["BIC", "AIC", "MBIC"])
        config_layout.addWidget(self._penalty_combo, 0, 1)

        config_layout.addWidget(QLabel("Min Segment:"), 1, 0)
        self._min_seg_spin = QSpinBox()
        self._min_seg_spin.setRange(2, 100)
        self._min_seg_spin.setValue(10)
        self._min_seg_spin.setSuffix(" points")
        config_layout.addWidget(self._min_seg_spin, 1, 1)

        self._run_btn = QPushButton("Detect")
        self._run_btn.setProperty("primary", True)
        self._run_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._run_btn.setFixedHeight(42)
        self._run_btn.clicked.connect(self._run_detection)
        config_layout.addWidget(self._run_btn, 2, 0, 1, 2)

        layout.addWidget(config_card)

        # --- Chart ---
        layout.addWidget(Heading("Detection Chart", level=2))

        chart_card = Card()
        chart_layout = QVBoxLayout(chart_card)
        chart_layout.setContentsMargins(12, 12, 12, 12)

        self._chart = ChangepointChart()
        chart_layout.addWidget(self._chart)

        layout.addWidget(chart_card)

        # --- Results Table ---
        layout.addWidget(Heading("Results", level=2))

        table_card = Card()
        table_layout = QVBoxLayout(table_card)
        table_layout.setContentsMargins(12, 12, 12, 12)

        self._table = QTableWidget(0, 4)
        self._table.setHorizontalHeaderLabels(["Index", "Confidence", "Left Mean", "Right Mean"])
        self._table.horizontalHeader().setStretchLastSection(True)
        self._table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        self._table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self._table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self._table.setAlternatingRowColors(True)
        self._table.setMinimumHeight(120)
        self._table.setMaximumHeight(220)
        self._table.setStyleSheet(f"""
            QTableWidget {{
                background: {C.SURFACE};
                border: none;
                gridline-color: {C.BORDER};
                font-size: 12px;
            }}
            QTableWidget::item {{
                padding: 6px 8px;
            }}
            QTableWidget::item:selected {{
                background: {C.SURFACE2};
                color: {C.TEXT};
            }}
            QHeaderView::section {{
                background: {C.SURFACE2};
                border: none;
                border-bottom: 1px solid {C.BORDER};
                padding: 8px;
                font-weight: 600;
                font-size: 11px;
                color: {C.TEXT2};
            }}
        """)

        self._no_results_label = QLabel("Run detection to see results")
        self._no_results_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT3};")
        self._no_results_label.setAlignment(Qt.AlignmentFlag.AlignCenter)

        table_layout.addWidget(self._table)
        table_layout.addWidget(self._no_results_label)
        self._table.hide()

        layout.addWidget(table_card)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _run_detection(self):
        self._run_btn.setEnabled(False)
        self._run_btn.setText("Detecting...")

        from build_oracle.cli import generate_sample_series

        series = generate_sample_series(n=365, changepoints=3)

        penalty = self._penalty_combo.currentText().lower()
        min_seg = self._min_seg_spin.value()

        self._worker = ChangepointWorker(series, penalty, min_seg)
        self._worker.finished.connect(self._on_detection_done)
        self._worker.start()

    def _on_detection_done(self, result: dict):
        self._run_btn.setEnabled(True)
        self._run_btn.setText("Detect")

        # Update chart
        self._chart.set_data(
            result["series"],
            result["changepoints"],
            result["segment_boundaries"],
            result["segment_means"],
        )

        # Update table
        cps = result["changepoints"]
        if cps:
            self._no_results_label.hide()
            self._table.show()
            self._table.setRowCount(len(cps))
            for row, cp in enumerate(cps):
                self._table.setItem(row, 0, QTableWidgetItem(str(cp["index"])))
                self._table.setItem(row, 1, QTableWidgetItem(f"{cp['confidence']:.4f}"))
                self._table.setItem(row, 2, QTableWidgetItem(f"{cp['left_mean']:.4f}"))
                self._table.setItem(row, 3, QTableWidgetItem(f"{cp['right_mean']:.4f}"))
        else:
            self._table.hide()
            self._no_results_label.show()
            self._no_results_label.setText("No changepoints detected")

        if self._main_window:
            count = len(cps)
            self._main_window.show_toast(f"Detected {count} changepoint{'s' if count != 1 else ''}", "success")
