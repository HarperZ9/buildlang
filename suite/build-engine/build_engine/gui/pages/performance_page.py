"""
Performance -- the feedback loop visibility.

Shows model comparison table, prediction-vs-actual scatter chart,
accuracy over time, and weight evolution.
"""

import random

from PyQt6.QtCore import QPointF, QRectF, Qt
from PyQt6.QtGui import (
    QColor,
    QFont,
    QPainter,
    QPen,
)
from PyQt6.QtWidgets import (
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QScrollArea,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from build_engine.gui.app import C, Card, Heading, Stat, StatusDot


class PredictionScatterChart(QWidget):
    """Scatter plot: predicted direction vs actual direction.

    Points in Q1 (both positive) and Q3 (both negative) are correct
    predictions (green). Points in Q2/Q4 are wrong (red).
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(220)
        self.setMinimumWidth(350)
        self._points = []  # list of (predicted, actual)

    def set_data(self, points: list):
        self._points = points
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        pad = 50
        chart_w = w - pad * 2
        chart_h = h - pad * 2
        cx = pad + chart_w / 2
        cy = pad + chart_h / 2

        if not self._points:
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No prediction data")
            p.end()
            return

        # Find range
        all_vals = [abs(v) for pt in self._points for v in pt]
        max_val = max(all_vals) if all_vals else 1.0
        max_val = max(max_val, 0.01)

        # Background quadrants
        # Q1 (top-right) and Q3 (bottom-left) = correct
        q_correct = QColor(C.GREEN_BG)
        q_wrong = QColor(C.RED_BG)

        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(q_correct)
        p.drawRect(QRectF(cx, pad, chart_w / 2, chart_h / 2))  # Q1
        p.drawRect(QRectF(pad, cy, chart_w / 2, chart_h / 2))  # Q3
        p.setBrush(q_wrong)
        p.drawRect(QRectF(pad, pad, chart_w / 2, chart_h / 2))  # Q2
        p.drawRect(QRectF(cx, cy, chart_w / 2, chart_h / 2))  # Q4

        # Axes
        axis_pen = QPen(QColor(C.BORDER_LT), 1)
        p.setPen(axis_pen)
        p.drawLine(QPointF(pad, cy), QPointF(w - pad, cy))  # horizontal
        p.drawLine(QPointF(cx, pad), QPointF(cx, h - pad))  # vertical

        # Axis labels
        p.setPen(QColor(C.TEXT2))
        p.setFont(QFont("Segoe UI", 9))
        p.drawText(QRectF(cx - 50, h - pad + 4, 100, 20), Qt.AlignmentFlag.AlignCenter, "Predicted")
        p.save()
        p.translate(14, cy)
        p.rotate(-90)
        p.drawText(QRectF(-40, -8, 80, 16), Qt.AlignmentFlag.AlignCenter, "Actual")
        p.restore()

        # Plot points
        for pred, actual in self._points:
            px = cx + (pred / max_val) * (chart_w / 2)
            py = cy - (actual / max_val) * (chart_h / 2)

            # Correct if same sign
            correct = (pred > 0 and actual > 0) or (pred < 0 and actual < 0)
            color = QColor(C.GREEN) if correct else QColor(C.RED)
            color.setAlpha(160)

            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(color)
            p.drawEllipse(QPointF(px, py), 4, 4)

        # Legend
        p.setFont(QFont("Segoe UI", 8))
        legend_y = pad - 8
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.GREEN))
        p.drawEllipse(QPointF(pad + 4, legend_y), 4, 4)
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(pad + 12, legend_y + 4), "Correct")

        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(C.RED))
        p.drawEllipse(QPointF(pad + 72, legend_y), 4, 4)
        p.setPen(QColor(C.TEXT2))
        p.drawText(QPointF(pad + 80, legend_y + 4), "Wrong")

        p.end()


class AccuracyTimeChart(QWidget):
    """Rolling accuracy line per model over time."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(170)
        self.setMinimumWidth(350)
        self._series = {}  # model_name -> list of accuracy values

    def set_data(self, series: dict):
        self._series = series
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        pad_x = 50
        pad_y = 20
        chart_w = w - pad_x * 2
        chart_h = h - pad_y * 2

        if not self._series:
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No accuracy history")
            p.end()
            return

        # Grid
        p.setPen(QPen(QColor(C.BORDER), 1))
        for frac in [0.0, 0.25, 0.5, 0.75, 1.0]:
            gy = pad_y + chart_h * (1.0 - frac)
            p.drawLine(QPointF(pad_x, gy), QPointF(w - pad_x, gy))
            p.setPen(QColor(C.TEXT3))
            p.setFont(QFont("Segoe UI", 8))
            p.drawText(
                QRectF(0, gy - 8, pad_x - 6, 16),
                Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter,
                f"{frac * 100:.0f}%",
            )
            p.setPen(QPen(QColor(C.BORDER), 1))

        # 50% reference line
        ref_y = pad_y + chart_h * 0.5
        ref_pen = QPen(QColor(C.YELLOW), 1, Qt.PenStyle.DashLine)
        p.setPen(ref_pen)
        p.drawLine(QPointF(pad_x, ref_y), QPointF(w - pad_x, ref_y))

        # Model colors
        model_colors = {
            "arima": C.CYAN,
            "prophet": C.ACCENT,
            "neural": C.GREEN,
        }

        max(len(v) for v in self._series.values()) if self._series else 0

        # Draw lines
        legend_x = pad_x + 10
        for model_name, values in self._series.items():
            if len(values) < 2:
                continue

            n = len(values)
            color = model_colors.get(model_name, C.TEXT2)
            pen = QPen(QColor(color), 2)
            p.setPen(pen)

            points = []
            for i, v in enumerate(values):
                px = pad_x + (i / max(n - 1, 1)) * chart_w
                py = pad_y + chart_h - (v / 100.0) * chart_h
                points.append(QPointF(px, py))

            for i in range(len(points) - 1):
                p.drawLine(points[i], points[i + 1])

            # Legend entry
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(color))
            p.drawEllipse(QPointF(legend_x, pad_y - 6), 4, 4)
            p.setPen(QColor(C.TEXT2))
            p.setFont(QFont("Segoe UI", 9))
            p.drawText(QPointF(legend_x + 8, pad_y - 2), model_name.title())
            legend_x += 80

        p.end()


class WeightEvolutionChart(QWidget):
    """Stacked area showing how model weights change over time."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedHeight(150)
        self.setMinimumWidth(350)
        self._series = {}  # model_name -> list of weight values

    def set_data(self, series: dict):
        self._series = series
        self.update()

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)

        w = self.width()
        h = self.height()
        pad_x = 50
        pad_y = 20
        chart_w = w - pad_x * 2
        chart_h = h - pad_y * 2

        if not self._series:
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "No weight history")
            p.end()
            return

        model_colors = {
            "arima": C.CYAN_MUT,
            "prophet": C.ACCENT,
            "neural": C.GREEN_MUT,
        }

        max_len = max(len(v) for v in self._series.values()) if self._series else 0
        if max_len < 2:
            p.setPen(QColor(C.TEXT3))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, "Collecting data...")
            p.end()
            return

        model_names = list(self._series.keys())
        n = max_len

        # Normalize weights at each time step
        for i in range(n):
            total = 0
            for name in model_names:
                vals = self._series[name]
                total += vals[i] if i < len(vals) else 0
            if total == 0:
                total = 1

        # Draw bars for each time step
        bar_w = max(2, chart_w / n)
        for i in range(n):
            x = pad_x + (i / max(n - 1, 1)) * (chart_w - bar_w)
            total = sum(self._series[name][i] if i < len(self._series[name]) else 0 for name in model_names)
            if total == 0:
                total = 1

            cum_y = 0
            for name in model_names:
                vals = self._series[name]
                v = vals[i] if i < len(vals) else 0
                frac = v / total
                seg_h = frac * chart_h

                color = QColor(model_colors.get(name, C.TEXT3))
                color.setAlpha(120)
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(color)
                p.drawRect(QRectF(x, pad_y + chart_h - cum_y - seg_h, bar_w, seg_h))
                cum_y += seg_h

        # Legend
        legend_x = pad_x + 10
        for name in model_names:
            color = model_colors.get(name, C.TEXT3)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(color))
            p.drawRoundedRect(QRectF(legend_x, pad_y - 10, 10, 10), 2, 2)
            p.setPen(QColor(C.TEXT2))
            p.setFont(QFont("Segoe UI", 9))
            p.drawText(QPointF(legend_x + 14, pad_y - 2), name.title())
            legend_x += 80

        p.end()


class PerformancePage(QWidget):
    """The feedback loop visibility page."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._build_ui()
        self._load_demo_data()

    def _build_ui(self):
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(18)

        layout.addWidget(Heading("Performance"))

        # Row 1: Model Comparison Table
        card_table, table_lay = Card.with_layout()
        table_lay.addWidget(Heading("Model Comparison", level=2))

        self._model_table = QTableWidget(3, 4)
        self._model_table.setHorizontalHeaderLabels(["Model", "Accuracy", "Predictions", "Weight"])
        self._model_table.horizontalHeader().setSectionResizeMode(QHeaderView.ResizeMode.Stretch)
        self._model_table.verticalHeader().setVisible(False)
        self._model_table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._model_table.setSelectionMode(QTableWidget.SelectionMode.NoSelection)
        self._model_table.setFixedHeight(140)
        self._model_table.setStyleSheet(
            f"QTableWidget {{ background: {C.SURFACE}; border: 1px solid {C.BORDER}; "
            f"border-radius: 8px; gridline-color: {C.BORDER}; }}"
            f"QTableWidget::item {{ padding: 6px 12px; }}"
            f"QHeaderView::section {{ background: {C.SURFACE2}; border: none; "
            f"border-bottom: 1px solid {C.BORDER}; padding: 8px; font-weight: 600; "
            f"color: {C.TEXT2}; font-size: 11px; }}"
        )
        table_lay.addWidget(self._model_table)
        layout.addWidget(card_table)

        # Row 2: Prediction vs Actual + Accuracy Over Time
        row2 = QHBoxLayout()
        row2.setSpacing(16)

        card_scatter, scatter_lay = Card.with_layout()
        scatter_lay.addWidget(Heading("Prediction vs Actual", level=2))
        self._scatter_chart = PredictionScatterChart()
        scatter_lay.addWidget(self._scatter_chart)
        scatter_lay.addStretch()
        row2.addWidget(card_scatter, stretch=1)

        card_acc_time, acc_time_lay = Card.with_layout()
        acc_time_lay.addWidget(Heading("Accuracy Over Time", level=2))
        self._accuracy_time_chart = AccuracyTimeChart()
        acc_time_lay.addWidget(self._accuracy_time_chart)
        acc_time_lay.addStretch()
        row2.addWidget(card_acc_time, stretch=1)

        layout.addLayout(row2)

        # Row 3: Weight Evolution
        card_weights, weights_lay = Card.with_layout()
        weights_lay.addWidget(Heading("Weight Evolution", level=2))
        self._weight_chart = WeightEvolutionChart()
        weights_lay.addWidget(self._weight_chart)
        layout.addWidget(card_weights)

        # Overall stats row
        card_overall, overall_lay = Card.with_layout(layout_cls=QHBoxLayout)
        self._stat_accuracy = Stat("Overall Accuracy", "57%", C.GREEN)
        self._stat_total = Stat("Total Predictions", "246", C.TEXT)
        self._stat_correct = Stat("Correct", "140", C.GREEN)
        self._stat_wrong = Stat("Wrong", "106", C.RED)
        overall_lay.addWidget(self._stat_accuracy)
        overall_lay.addWidget(self._stat_total)
        overall_lay.addWidget(self._stat_correct)
        overall_lay.addWidget(self._stat_wrong)
        overall_lay.addStretch()
        layout.addWidget(card_overall)

        # Row 4: Risk Metrics
        card_risk, risk_lay = Card.with_layout()
        risk_lay.addWidget(Heading("Risk Metrics", level=2))

        risk_row = QHBoxLayout()
        risk_row.setSpacing(20)
        self._stat_sharpe = Stat("Sharpe Ratio", "1.24", C.GREEN)
        self._stat_sortino = Stat("Sortino Ratio", "1.87", C.GREEN)
        self._stat_drawdown = Stat("Max Drawdown", "-8.3%", C.RED)
        self._stat_win_rate = Stat("Win Rate", "61%", C.GREEN)
        risk_row.addWidget(self._stat_sharpe)
        risk_row.addWidget(self._stat_sortino)
        risk_row.addWidget(self._stat_drawdown)
        risk_row.addWidget(self._stat_win_rate)
        risk_row.addStretch()
        risk_lay.addLayout(risk_row)
        layout.addWidget(card_risk)

        # Row 5: Regime Change Alerts
        card_regime, regime_lay = Card.with_layout()
        regime_lay.addWidget(Heading("Regime Change Alerts", level=2))

        self._regime_container = QVBoxLayout()
        self._regime_container.setSpacing(6)
        regime_lay.addLayout(self._regime_container)

        self._regime_labels = []
        for _ in range(5):
            row_widget = QWidget()
            row_lay = QHBoxLayout(row_widget)
            row_lay.setContentsMargins(0, 2, 0, 2)
            row_lay.setSpacing(8)

            dot = StatusDot(C.TEXT3, 8)
            row_lay.addWidget(dot)

            ts_label = QLabel("--")
            ts_label.setFixedWidth(120)
            ts_label.setStyleSheet(f"color: {C.TEXT2}; font-size: 11px;")
            row_lay.addWidget(ts_label)

            msg_label = QLabel("--")
            msg_label.setStyleSheet(f"color: {C.TEXT}; font-size: 12px;")
            row_lay.addWidget(msg_label)
            row_lay.addStretch()

            self._regime_container.addWidget(row_widget)
            self._regime_labels.append((dot, ts_label, msg_label))

        regime_lay.addStretch()
        layout.addWidget(card_regime)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _load_demo_data(self):
        """Load demo data for display."""
        # Model comparison table
        models_data = [
            ("ARIMA", "62%", "98", "0.38"),
            ("Prophet", "55%", "87", "0.30"),
            ("Neural", "48%", "61", "0.23"),
        ]
        for row, (name, acc, preds, weight) in enumerate(models_data):
            self._model_table.setItem(row, 0, QTableWidgetItem(name))

            acc_item = QTableWidgetItem(acc)
            acc_val = int(acc.replace("%", ""))
            if acc_val >= 60:
                acc_item.setForeground(QColor(C.GREEN))
            elif acc_val >= 40:
                acc_item.setForeground(QColor(C.YELLOW))
            else:
                acc_item.setForeground(QColor(C.RED))
            acc_item.setFont(QFont("Segoe UI", 11, QFont.Weight.Bold))
            self._model_table.setItem(row, 1, acc_item)

            self._model_table.setItem(row, 2, QTableWidgetItem(preds))
            self._model_table.setItem(row, 3, QTableWidgetItem(weight))

        # Scatter chart demo
        scatter_pts = []
        for _ in range(80):
            pred = random.gauss(0, 0.02)
            # Bias toward correct predictions
            noise = random.gauss(0, 0.015)
            actual = pred * random.choice([0.8, 1.2, -0.3, 0.9, 1.1]) + noise
            scatter_pts.append((pred, actual))
        self._scatter_chart.set_data(scatter_pts)

        # Accuracy over time demo
        def gen_accuracy_series(base, n=30):
            series = []
            v = base
            for _ in range(n):
                v += random.gauss(0, 3)
                v = max(20, min(90, v))
                series.append(v)
            return series

        self._accuracy_time_chart.set_data(
            {
                "arima": gen_accuracy_series(62),
                "prophet": gen_accuracy_series(55),
                "neural": gen_accuracy_series(48),
            }
        )

        # Weight evolution demo
        def gen_weight_series(base, n=30):
            series = []
            v = base
            for _ in range(n):
                v += random.gauss(0, 0.03)
                v = max(0.05, min(1.0, v))
                series.append(v)
            return series

        self._weight_chart.set_data(
            {
                "arima": gen_weight_series(0.38),
                "prophet": gen_weight_series(0.30),
                "neural": gen_weight_series(0.23),
            }
        )

        # Regime change alerts demo
        demo_alerts = [
            (C.RED, "03/24 09:30", "Volatility spike detected -- VIX +23%"),
            (C.YELLOW, "03/23 14:15", "Model weight shift: Neural surpassed Prophet"),
            (C.CYAN, "03/22 10:00", "Trend reversal signal on BTC-USD (bear -> bull)"),
            (C.YELLOW, "03/21 16:30", "Correlation breakdown: AAPL / SPY divergence"),
            (C.GREEN, "03/20 11:45", "Low-volatility regime entered -- reducing position sizes"),
        ]
        for i, (color, ts, msg) in enumerate(demo_alerts):
            if i >= len(self._regime_labels):
                break
            dot, ts_label, msg_label = self._regime_labels[i]
            dot.set_color(color)
            ts_label.setText(ts)
            msg_label.setText(msg)

    def update_from_tracker(self, stats: dict, weights: dict):
        """Update from live PerformanceTracker data."""
        row = 0
        for name, info in stats.items():
            if row >= 3:
                break
            self._model_table.setItem(row, 0, QTableWidgetItem(name.title()))
            acc = info.get("accuracy", 0)
            acc_item = QTableWidgetItem(f"{acc:.0%}")
            if acc >= 0.6:
                acc_item.setForeground(QColor(C.GREEN))
            elif acc >= 0.4:
                acc_item.setForeground(QColor(C.YELLOW))
            else:
                acc_item.setForeground(QColor(C.RED))
            acc_item.setFont(QFont("Segoe UI", 11, QFont.Weight.Bold))
            self._model_table.setItem(row, 1, acc_item)
            self._model_table.setItem(row, 2, QTableWidgetItem(str(info.get("total", 0))))
            w = weights.get(name, 0)
            self._model_table.setItem(row, 3, QTableWidgetItem(f"{w:.2f}"))
            row += 1

    def refresh(self):
        pass
