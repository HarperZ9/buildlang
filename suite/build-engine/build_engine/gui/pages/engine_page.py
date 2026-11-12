"""
Engine Control -- the control panel.

Configure symbols, models, risk parameters and run the adaptive engine
with live activity logging. Engine runs in a QThread to keep the GUI
responsive.
"""

import logging
import time

from PyQt6.QtCore import Qt, QThread, pyqtSignal
from PyQt6.QtGui import QFont, QTextCursor
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QSlider,
    QSpinBox,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from build_engine.gui.app import C, Card, Heading, StatusDot

logger = logging.getLogger(__name__)


class EngineWorker(QThread):
    """Runs the AdaptiveEngine loop in a background thread."""

    log_line = pyqtSignal(str)
    cycle_result = pyqtSignal(dict)
    error = pyqtSignal(str)
    finished_signal = pyqtSignal()

    def __init__(self, config_dict: dict, parent=None):
        super().__init__(parent)
        self._config_dict = config_dict
        self._stop_requested = False

    def run(self):
        try:
            from build_engine.adaptive_engine import AdaptiveEngine
            from build_engine.config import EngineConfig

            config = EngineConfig(
                symbols=self._config_dict["symbols"],
                models=self._config_dict["models"],
                paper_trading=self._config_dict["paper_trading"],
                risk_per_trade=self._config_dict["risk_per_trade"],
                forecast_horizon=self._config_dict["forecast_horizon"],
                min_confidence=self._config_dict["min_confidence"],
            )

            self.log_line.emit("Initializing engine...")
            self.log_line.emit(f"  Symbols : {config.symbols}")
            self.log_line.emit(f"  Models  : {config.models}")
            self.log_line.emit(f"  Paper   : {config.paper_trading}")
            self.log_line.emit(f"  Risk    : {config.risk_per_trade:.0%}")
            self.log_line.emit(f"  Horizon : {config.forecast_horizon}")
            self.log_line.emit("")

            engine = AdaptiveEngine(config)
            self._engine = engine

            self.log_line.emit("Engine started. Running cycles...")
            self.log_line.emit("")

            interval = self._config_dict.get("interval_seconds", 300)
            max_cycles = self._config_dict.get("max_cycles")

            engine.running = True
            while engine.running and not self._stop_requested:
                try:
                    t0 = time.time()
                    result = engine.run_cycle()
                    elapsed = time.time() - t0

                    self.log_line.emit(
                        f"[Cycle {result['cycle']}] "
                        f"Equity=${result['equity']:,.2f}  "
                        f"Actions={len(result['actions'])}  "
                        f"Accuracy={result['accuracy']:.0%}  "
                        f"({elapsed:.1f}s)"
                    )

                    for action in result["actions"]:
                        self.log_line.emit(f"  -> {action}")

                    self.cycle_result.emit(result)

                except Exception as exc:
                    self.log_line.emit(f"  [ERROR] {exc}")
                    self.error.emit(str(exc))

                if max_cycles and engine.cycle_count >= max_cycles:
                    self.log_line.emit(f"\nMax cycles ({max_cycles}) reached.")
                    break

                if not self._stop_requested:
                    # Sleep in short intervals so we can respond to stop
                    for _ in range(interval):
                        if self._stop_requested:
                            break
                        self.msleep(1000)

            status = engine.get_status()
            self.log_line.emit("")
            self.log_line.emit("=== Engine Stopped ===")
            self.log_line.emit(f"  Cycles    : {status['cycles']}")
            self.log_line.emit(f"  Equity    : ${status['equity']:,.2f}")
            self.log_line.emit(f"  Positions : {status['positions']}")
            self.log_line.emit(f"  Trades    : {status['trades']}")
            self.log_line.emit(f"  Accuracy  : {status['accuracy']:.0%}")

        except Exception as exc:
            self.log_line.emit(f"[FATAL] Engine failed: {exc}")
            self.error.emit(str(exc))

        self.finished_signal.emit()

    def request_stop(self):
        self._stop_requested = True
        if hasattr(self, "_engine"):
            self._engine.stop()


class EngineControlPage(QWidget):
    """The engine control panel."""

    def __init__(self, main_window=None, parent=None):
        super().__init__(parent)
        self._main_window = main_window
        self._engine_running = False
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

        layout.addWidget(Heading("Engine Control"))

        # Paper trading banner
        self._paper_banner = QFrame()
        self._paper_banner.setStyleSheet(
            f"background: {C.GREEN_BG}; border: 1px solid {C.GREEN_MUT}; border-radius: 10px; padding: 10px 16px;"
        )
        banner_lay = QHBoxLayout(self._paper_banner)
        banner_lay.setContentsMargins(12, 6, 12, 6)
        self._banner_dot = StatusDot(C.GREEN_MUT, 10)
        banner_lay.addWidget(self._banner_dot)
        self._banner_label = QLabel("Paper trading -- no real money at risk")
        self._banner_label.setStyleSheet(f"font-size: 12px; font-weight: 500; color: {C.TEXT};")
        banner_lay.addWidget(self._banner_label)
        banner_lay.addStretch()
        layout.addWidget(self._paper_banner)

        # Main content: Config + Controls
        content_row = QHBoxLayout()
        content_row.setSpacing(16)

        # Left: Configuration card
        card_config, config_lay = Card.with_layout()
        config_lay.addWidget(Heading("Configuration", level=2))

        grid = QGridLayout()
        grid.setSpacing(10)
        grid.setColumnMinimumWidth(0, 130)

        # Symbols
        grid.addWidget(QLabel("Symbols:"), 0, 0, Qt.AlignmentFlag.AlignRight)
        self._symbols_input = QLineEdit("AAPL,BTC-USD,ETH-USD")
        self._symbols_input.setPlaceholderText("Comma-separated symbols")
        grid.addWidget(self._symbols_input, 0, 1)

        # Models
        grid.addWidget(QLabel("Models:"), 1, 0, Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignTop)
        models_widget = QWidget()
        models_lay = QVBoxLayout(models_widget)
        models_lay.setContentsMargins(0, 0, 0, 0)
        models_lay.setSpacing(4)
        self._chk_arima = QCheckBox("ARIMA")
        self._chk_arima.setChecked(True)
        self._chk_prophet = QCheckBox("Prophet")
        self._chk_prophet.setChecked(True)
        self._chk_neural = QCheckBox("Neural Network")
        models_lay.addWidget(self._chk_arima)
        models_lay.addWidget(self._chk_prophet)
        models_lay.addWidget(self._chk_neural)
        grid.addWidget(models_widget, 1, 1)

        # Forecast horizon
        grid.addWidget(QLabel("Forecast Horizon:"), 2, 0, Qt.AlignmentFlag.AlignRight)
        self._horizon_spin = QSpinBox()
        self._horizon_spin.setRange(1, 30)
        self._horizon_spin.setValue(5)
        self._horizon_spin.setSuffix(" steps")
        grid.addWidget(self._horizon_spin, 2, 1)

        # Risk per trade
        grid.addWidget(QLabel("Risk Per Trade:"), 3, 0, Qt.AlignmentFlag.AlignRight)
        risk_widget = QWidget()
        risk_lay = QHBoxLayout(risk_widget)
        risk_lay.setContentsMargins(0, 0, 0, 0)
        risk_lay.setSpacing(8)
        self._risk_slider = QSlider(Qt.Orientation.Horizontal)
        self._risk_slider.setRange(1, 10)
        self._risk_slider.setValue(2)
        self._risk_slider.setTickPosition(QSlider.TickPosition.TicksBelow)
        self._risk_label = QLabel("2%")
        self._risk_label.setFixedWidth(35)
        self._risk_label.setStyleSheet(f"font-weight: 600; color: {C.ACCENT_TX};")
        self._risk_slider.valueChanged.connect(lambda v: self._risk_label.setText(f"{v}%"))
        risk_lay.addWidget(self._risk_slider)
        risk_lay.addWidget(self._risk_label)
        grid.addWidget(risk_widget, 3, 1)

        # Interval
        grid.addWidget(QLabel("Interval:"), 4, 0, Qt.AlignmentFlag.AlignRight)
        self._interval_combo = QComboBox()
        self._interval_combo.addItems(["1 min", "5 min", "15 min", "1 hour", "1 day"])
        self._interval_combo.setCurrentIndex(1)
        grid.addWidget(self._interval_combo, 4, 1)

        # Confidence threshold
        grid.addWidget(QLabel("Confidence:"), 5, 0, Qt.AlignmentFlag.AlignRight)
        conf_widget = QWidget()
        conf_lay = QHBoxLayout(conf_widget)
        conf_lay.setContentsMargins(0, 0, 0, 0)
        conf_lay.setSpacing(8)
        self._conf_slider = QSlider(Qt.Orientation.Horizontal)
        self._conf_slider.setRange(10, 90)
        self._conf_slider.setValue(30)
        self._conf_slider.setTickPosition(QSlider.TickPosition.TicksBelow)
        self._conf_label = QLabel("30%")
        self._conf_label.setFixedWidth(35)
        self._conf_label.setStyleSheet(f"font-weight: 600; color: {C.ACCENT_TX};")
        self._conf_slider.valueChanged.connect(lambda v: self._conf_label.setText(f"{v}%"))
        conf_lay.addWidget(self._conf_slider)
        conf_lay.addWidget(self._conf_label)
        grid.addWidget(conf_widget, 5, 1)

        # Paper/Live toggle
        grid.addWidget(QLabel("Mode:"), 6, 0, Qt.AlignmentFlag.AlignRight)
        self._chk_paper = QCheckBox("Paper Trading")
        self._chk_paper.setChecked(True)
        self._chk_paper.toggled.connect(self._update_paper_banner)
        grid.addWidget(self._chk_paper, 6, 1)

        config_lay.addLayout(grid)
        config_lay.addStretch()

        # Start/Stop button
        self._btn_start = QPushButton("Start Engine")
        self._btn_start.setFixedHeight(48)
        self._btn_start.setCursor(Qt.CursorShape.PointingHandCursor)
        self._btn_start.setStyleSheet(
            f"QPushButton {{ background: {C.GREEN}; border: none; border-radius: 12px; "
            f"color: white; font-size: 15px; font-weight: 700; }}"
            f"QPushButton:hover {{ background: #22c55e; }}"
        )
        self._btn_start.clicked.connect(self._toggle_engine)
        config_lay.addWidget(self._btn_start)

        card_config.setMinimumWidth(380)
        card_config.setMaximumWidth(450)
        content_row.addWidget(card_config)

        # Right: Live Activity Log
        card_log, log_lay = Card.with_layout()
        log_lay.addWidget(Heading("Live Activity", level=2))

        self._log_text = QTextEdit()
        self._log_text.setReadOnly(True)
        self._log_text.setFont(QFont("Cascadia Code", 10))
        self._log_text.setStyleSheet(
            f"QTextEdit {{ background: #faf5f0; border: 1px solid {C.BORDER}; border-radius: 8px; padding: 10px; }}"
        )
        self._log_text.setPlaceholderText("Engine activity will appear here...")
        log_lay.addWidget(self._log_text, stretch=1)

        # Clear log button
        btn_clear = QPushButton("Clear Log")
        btn_clear.setCursor(Qt.CursorShape.PointingHandCursor)
        btn_clear.clicked.connect(self._log_text.clear)
        log_lay.addWidget(btn_clear, alignment=Qt.AlignmentFlag.AlignRight)

        content_row.addWidget(card_log, stretch=1)
        layout.addLayout(content_row, stretch=1)

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _update_paper_banner(self, paper: bool):
        if paper:
            self._paper_banner.setStyleSheet(
                f"background: {C.GREEN_BG}; border: 1px solid {C.GREEN_MUT}; border-radius: 10px; padding: 10px 16px;"
            )
            self._banner_dot.set_color(C.GREEN_MUT)
            self._banner_label.setText("Paper trading -- no real money at risk")
        else:
            self._paper_banner.setStyleSheet(
                f"background: {C.RED_BG}; border: 1px solid {C.RED}; border-radius: 10px; padding: 10px 16px;"
            )
            self._banner_dot.set_color(C.RED)
            self._banner_label.setText("LIVE TRADING -- real money at risk")

    def _get_interval_seconds(self) -> int:
        intervals = {
            "1 min": 60,
            "5 min": 300,
            "15 min": 900,
            "1 hour": 3600,
            "1 day": 86400,
        }
        return intervals.get(self._interval_combo.currentText(), 300)

    def _get_config_dict(self) -> dict:
        symbols = [s.strip() for s in self._symbols_input.text().split(",") if s.strip()]
        models = []
        if self._chk_arima.isChecked():
            models.append("arima")
        if self._chk_prophet.isChecked():
            models.append("prophet")
        if self._chk_neural.isChecked():
            models.append("neural")

        return {
            "symbols": symbols or ["AAPL"],
            "models": models or ["arima"],
            "paper_trading": self._chk_paper.isChecked(),
            "risk_per_trade": self._risk_slider.value() / 100.0,
            "forecast_horizon": self._horizon_spin.value(),
            "min_confidence": self._conf_slider.value() / 100.0,
            "interval_seconds": self._get_interval_seconds(),
            "max_cycles": None,
        }

    def _toggle_engine(self):
        if self._engine_running:
            self.stop_engine()
        else:
            self.start_engine()

    def start_engine(self):
        if self._engine_running:
            return

        self._engine_running = True
        self._btn_start.setText("Stop Engine")
        self._btn_start.setStyleSheet(
            f"QPushButton {{ background: {C.RED}; border: none; border-radius: 12px; "
            f"color: white; font-size: 15px; font-weight: 700; }}"
            f"QPushButton:hover {{ background: #ef4444; }}"
        )

        # Disable config while running
        self._symbols_input.setEnabled(False)
        self._chk_arima.setEnabled(False)
        self._chk_prophet.setEnabled(False)
        self._chk_neural.setEnabled(False)
        self._horizon_spin.setEnabled(False)
        self._risk_slider.setEnabled(False)
        self._interval_combo.setEnabled(False)
        self._conf_slider.setEnabled(False)
        self._chk_paper.setEnabled(False)

        if self._main_window:
            self._main_window.set_engine_status(True)
            self._main_window.show_toast("Engine started", "success")

        config_dict = self._get_config_dict()
        self._worker = EngineWorker(config_dict)
        self._worker.log_line.connect(self._append_log)
        self._worker.cycle_result.connect(self._on_cycle_result)
        self._worker.error.connect(self._on_error)
        self._worker.finished_signal.connect(self._on_worker_finished)
        self._worker.start()

    def stop_engine(self):
        if not self._engine_running:
            return

        self._append_log("\nStop requested...")
        if self._worker:
            self._worker.request_stop()

    def _on_worker_finished(self):
        self._engine_running = False
        self._btn_start.setText("Start Engine")
        self._btn_start.setStyleSheet(
            f"QPushButton {{ background: {C.GREEN}; border: none; border-radius: 12px; "
            f"color: white; font-size: 15px; font-weight: 700; }}"
            f"QPushButton:hover {{ background: #22c55e; }}"
        )

        # Re-enable config
        self._symbols_input.setEnabled(True)
        self._chk_arima.setEnabled(True)
        self._chk_prophet.setEnabled(True)
        self._chk_neural.setEnabled(True)
        self._horizon_spin.setEnabled(True)
        self._risk_slider.setEnabled(True)
        self._interval_combo.setEnabled(True)
        self._conf_slider.setEnabled(True)
        self._chk_paper.setEnabled(True)

        if self._main_window:
            self._main_window.set_engine_status(False)
            self._main_window.show_toast("Engine stopped", "info")

        self._worker = None

    def _append_log(self, text: str):
        self._log_text.append(text)
        # Auto-scroll to bottom
        cursor = self._log_text.textCursor()
        cursor.movePosition(QTextCursor.MoveOperation.End)
        self._log_text.setTextCursor(cursor)

    def _on_cycle_result(self, result: dict):
        """Forward cycle result to dashboard and trading page."""
        if self._main_window:
            dashboard = self._main_window.stack.widget(0)
            if hasattr(dashboard, "update_from_engine"):
                dashboard.update_from_engine(result)

            trading = self._main_window.stack.widget(1)
            if hasattr(trading, "update_from_engine"):
                trading.update_from_engine(result)

    def _on_error(self, msg: str):
        if self._main_window:
            self._main_window.show_toast(f"Engine error: {msg}", "error")

    def refresh(self):
        pass
