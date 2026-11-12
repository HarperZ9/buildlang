"""
Build Oracle -- Dashboard Page

Overview stats, quick actions, and library info.
"""

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from build_oracle.gui.app import C, Card, Heading, Stat


class DashboardPage(QWidget):
    """Main dashboard with stats, quick actions, and library info."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window

        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        # Header
        layout.addWidget(Heading("Dashboard"))

        subtitle = QLabel("Time series forecasting and anomaly detection workbench")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2}; margin-bottom: 8px;")
        layout.addWidget(subtitle)

        # --- Stats Row ---
        stats_card, stats_layout = Card.with_layout(QHBoxLayout, margins=(24, 20, 24, 20))

        self._stat_models = Stat("Models Available", "3", C.ACCENT_TX)
        stats_layout.addWidget(self._stat_models)

        self._stat_metrics = Stat("Evaluation Metrics", "7", C.GREEN)
        stats_layout.addWidget(self._stat_metrics)

        self._stat_features = Stat("Extractable Features", "20+", C.CYAN)
        stats_layout.addWidget(self._stat_features)

        self._stat_methods = Stat("Decomposition Methods", "2", C.YELLOW)
        stats_layout.addWidget(self._stat_methods)

        layout.addWidget(stats_card)

        # --- Quick Actions ---
        layout.addWidget(Heading("Quick Actions", level=2))

        actions_card, actions_layout = Card.with_layout(QGridLayout, margins=(20, 16, 20, 16), spacing=12)

        btn_forecast = QPushButton("Run Forecast")
        btn_forecast.setProperty("primary", True)
        btn_forecast.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_forecast.setFixedHeight(44)
        btn_forecast.clicked.connect(self._go_forecast)
        actions_layout.addWidget(btn_forecast, 0, 0)

        btn_decompose = QPushButton("Decompose Series")
        btn_decompose.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_decompose.setFixedHeight(44)
        btn_decompose.clicked.connect(self._go_decompose)
        actions_layout.addWidget(btn_decompose, 0, 1)

        btn_changepoints = QPushButton("Detect Changepoints")
        btn_changepoints.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_changepoints.setFixedHeight(44)
        btn_changepoints.clicked.connect(self._go_changepoints)
        actions_layout.addWidget(btn_changepoints, 0, 2)

        layout.addWidget(actions_card)

        # --- Model Library ---
        layout.addWidget(Heading("Model Library", level=2))

        models_card, models_layout = Card.with_layout(QVBoxLayout, margins=(20, 16, 20, 16), spacing=8)

        models = [
            (
                "ARIMA / SARIMA",
                "Autoregressive Integrated Moving Average with optional seasonal component",
                C.ACCENT_TX,
            ),
            ("Prophet-style", "Fourier-based seasonality + piecewise linear trend + changepoints", C.GREEN),
            ("Neural Network", "LSTM / Attention layers with automatic differentiation", C.CYAN),
        ]

        for name, desc, color in models:
            row = QHBoxLayout()
            row.setSpacing(12)

            dot = QLabel("\u25cf")
            dot.setStyleSheet(f"font-size: 14px; color: {color};")
            dot.setFixedWidth(18)
            row.addWidget(dot)

            info = QVBoxLayout()
            info.setSpacing(1)
            title = QLabel(name)
            title.setStyleSheet(f"font-size: 13px; font-weight: 600; color: {C.TEXT};")
            info.addWidget(title)
            detail = QLabel(desc)
            detail.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
            detail.setWordWrap(True)
            info.addWidget(detail)
            row.addLayout(info, stretch=1)

            models_layout.addLayout(row)

        layout.addWidget(models_card)

        # --- Capabilities ---
        layout.addWidget(Heading("Capabilities", level=2))

        caps_card, caps_layout = Card.with_layout(QGridLayout, margins=(20, 16, 20, 16), spacing=8)

        capabilities = [
            ("Metrics", "MAE, RMSE, MAPE, MASE, R2, SMAPE, Bias"),
            ("Features", "Statistical, temporal, frequency-domain extraction"),
            ("Decomposition", "Additive & multiplicative trend-seasonal-residual"),
            ("Changepoints", "PELT algorithm with BIC, AIC, MBIC penalties"),
            ("Optimization", "Bayesian optimization with Gaussian Processes"),
            ("Autodiff", "Dual-number automatic differentiation engine"),
        ]

        for i, (cap_name, cap_desc) in enumerate(capabilities):
            row_idx = i // 2
            col_idx = i % 2

            cap_layout = QVBoxLayout()
            cap_layout.setSpacing(2)
            name_label = QLabel(cap_name)
            name_label.setStyleSheet(f"font-size: 12px; font-weight: 600; color: {C.ACCENT_TX};")
            cap_layout.addWidget(name_label)
            desc_label = QLabel(cap_desc)
            desc_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
            desc_label.setWordWrap(True)
            cap_layout.addWidget(desc_label)

            caps_layout.addLayout(cap_layout, row_idx, col_idx)

        layout.addWidget(caps_card)

        # --- Library Info ---
        layout.addWidget(Heading("Library", level=3))

        info_label = QLabel(
            "Build Oracle v1.0.0  --  Pure NumPy/SciPy core with zero heavy dependencies.  Part of the Build Universe."
        )
        info_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
        info_label.setWordWrap(True)
        layout.addWidget(info_label)

        layout.addStretch()

        scroll.setWidget(container)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _go_forecast(self):
        if self._main_window:
            self._main_window._switch_page(1)
            self._main_window.sidebar._on_click(1)

    def _go_decompose(self):
        if self._main_window:
            self._main_window._switch_page(2)
            self._main_window.sidebar._on_click(2)

    def _go_changepoints(self):
        if self._main_window:
            self._main_window._switch_page(3)
            self._main_window.sidebar._on_click(3)
