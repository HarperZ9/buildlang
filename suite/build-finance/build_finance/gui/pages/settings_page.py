"""
Build Finance — Settings Page

Application configuration including broker credentials,
default parameters, and export settings.
"""

from PyQt6.QtCore import QSettings, Qt
from PyQt6.QtWidgets import (
    QComboBox,
    QFileDialog,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from build_finance.gui.app import APP_NAME, APP_ORG, APP_VERSION, C, Card, Heading


class SettingsPage(QWidget):
    """Application settings and configuration."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._settings = QSettings(APP_ORG, APP_NAME)
        self._build_ui()
        self._load_settings()

    def _build_ui(self):
        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        layout.addWidget(Heading("Settings"))

        subtitle = QLabel("Configure application preferences and broker connections")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Broker Configuration Card ---
        broker_card, broker_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        broker_layout.addWidget(Heading("Broker Configuration", level=2))

        broker_note = QLabel("Connect your broker for live trading and account data.")
        broker_note.setStyleSheet(f"font-size: 12px; color: {C.TEXT2};")
        broker_layout.addWidget(broker_note)

        # API Key
        key_row = QHBoxLayout()
        key_col = QVBoxLayout()
        key_label = QLabel("API Key")
        key_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        key_col.addWidget(key_label)
        self._api_key_input = QLineEdit()
        self._api_key_input.setEchoMode(QLineEdit.EchoMode.Password)
        self._api_key_input.setPlaceholderText("Enter API key...")
        self._api_key_input.setFixedWidth(360)
        key_col.addWidget(self._api_key_input)
        key_row.addLayout(key_col)
        key_row.addStretch()
        broker_layout.addLayout(key_row)

        # API Secret
        secret_row = QHBoxLayout()
        secret_col = QVBoxLayout()
        secret_label = QLabel("API Secret")
        secret_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        secret_col.addWidget(secret_label)
        self._api_secret_input = QLineEdit()
        self._api_secret_input.setEchoMode(QLineEdit.EchoMode.Password)
        self._api_secret_input.setPlaceholderText("Enter API secret...")
        self._api_secret_input.setFixedWidth(360)
        secret_col.addWidget(self._api_secret_input)
        secret_row.addLayout(secret_col)
        secret_row.addStretch()
        broker_layout.addLayout(secret_row)

        layout.addWidget(broker_card)

        # --- Default Parameters Card ---
        defaults_card, defaults_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        defaults_layout.addWidget(Heading("Default Parameters", level=2))

        defaults_row = QHBoxLayout()
        defaults_row.setSpacing(16)

        # Default strategy
        strat_col = QVBoxLayout()
        strat_label = QLabel("Default Strategy")
        strat_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        strat_col.addWidget(strat_label)
        self._default_strategy = QComboBox()
        self._default_strategy.addItems(["Momentum", "Mean Reversion", "Trend Following", "Breakout", "Ensemble"])
        self._default_strategy.setFixedWidth(180)
        strat_col.addWidget(self._default_strategy)
        defaults_row.addLayout(strat_col)

        # Default risk %
        risk_col = QVBoxLayout()
        risk_label = QLabel("Default Risk %")
        risk_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        risk_col.addWidget(risk_label)
        self._default_risk = QSpinBox()
        self._default_risk.setRange(1, 10)
        self._default_risk.setValue(2)
        self._default_risk.setSuffix("%")
        self._default_risk.setFixedWidth(100)
        risk_col.addWidget(self._default_risk)
        defaults_row.addLayout(risk_col)

        defaults_row.addStretch()
        defaults_layout.addLayout(defaults_row)

        layout.addWidget(defaults_card)

        # --- Export Settings Card ---
        export_card, export_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        export_layout.addWidget(Heading("Export Settings", level=2))

        export_row = QHBoxLayout()
        export_row.setSpacing(12)

        dir_col = QVBoxLayout()
        dir_label = QLabel("Export Directory")
        dir_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT2}; font-weight: 500;")
        dir_col.addWidget(dir_label)

        dir_input_row = QHBoxLayout()
        self._export_dir_input = QLineEdit()
        self._export_dir_input.setPlaceholderText("Select export directory...")
        self._export_dir_input.setFixedWidth(320)
        self._export_dir_input.setReadOnly(True)
        dir_input_row.addWidget(self._export_dir_input)

        browse_btn = QPushButton("Browse...")
        browse_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        browse_btn.setFixedWidth(100)
        browse_btn.clicked.connect(self._browse_export_dir)
        dir_input_row.addWidget(browse_btn)
        dir_input_row.addStretch()

        dir_col.addLayout(dir_input_row)
        export_row.addLayout(dir_col)

        export_row.addStretch()
        export_layout.addLayout(export_row)

        layout.addWidget(export_card)

        # --- Save Button ---
        save_row = QHBoxLayout()
        save_btn = QPushButton("Save Settings")
        save_btn.setProperty("primary", True)
        save_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        save_btn.setFixedWidth(160)
        save_btn.clicked.connect(self._save_settings)
        save_row.addWidget(save_btn)
        save_row.addStretch()
        layout.addLayout(save_row)

        # --- About Section ---
        about_card, about_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20))
        about_layout.addWidget(Heading("About", level=2))

        about_text = QLabel(
            f"<b>{APP_NAME}</b> v{APP_VERSION}<br><br>"
            f"Professional algorithmic trading workbench for<br>"
            f"backtesting, auto-trading, and portfolio optimization.<br><br>"
            f"<b>Strategies:</b> Momentum, Mean Reversion, Trend, Breakout, Ensemble<br>"
            f"<b>Data Sources:</b> Yahoo Finance, CoinGecko, CSV<br>"
            f"<b>Portfolio:</b> Mean-Variance, Risk Parity, Black-Litterman, HRP<br><br>"
            f"<span style='color: {C.TEXT3};'>&copy; 2022-2026 Zain Dana Harper</span>"
        )
        about_text.setStyleSheet(f"font-size: 12px; color: {C.TEXT}; line-height: 1.5;")
        about_text.setWordWrap(True)
        about_text.setTextFormat(Qt.TextFormat.RichText)
        about_layout.addWidget(about_text)

        layout.addWidget(about_card)

        layout.addStretch()

        scroll.setWidget(content)

        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _browse_export_dir(self):
        directory = QFileDialog.getExistingDirectory(self, "Select Export Directory")
        if directory:
            self._export_dir_input.setText(directory)

    def _save_settings(self):
        """Persist settings to QSettings."""
        self._settings.setValue("broker/api_key", self._api_key_input.text())
        self._settings.setValue("broker/api_secret", self._api_secret_input.text())
        self._settings.setValue("defaults/strategy", self._default_strategy.currentText())
        self._settings.setValue("defaults/risk_pct", self._default_risk.value())
        self._settings.setValue("export/directory", self._export_dir_input.text())

        # Show feedback via parent window toast if available
        widget = self.parent()
        while widget:
            if hasattr(widget, "show_toast"):
                widget.show_toast("Settings saved", "success")
                break
            widget = widget.parent() if hasattr(widget, "parent") else None

    def _load_settings(self):
        """Load persisted settings from QSettings."""
        api_key = self._settings.value("broker/api_key", "")
        if api_key:
            self._api_key_input.setText(api_key)

        api_secret = self._settings.value("broker/api_secret", "")
        if api_secret:
            self._api_secret_input.setText(api_secret)

        strategy = self._settings.value("defaults/strategy", "Momentum")
        idx = self._default_strategy.findText(strategy)
        if idx >= 0:
            self._default_strategy.setCurrentIndex(idx)

        risk = self._settings.value("defaults/risk_pct", 2, type=int)
        self._default_risk.setValue(risk)

        export_dir = self._settings.value("export/directory", "")
        if export_dir:
            self._export_dir_input.setText(export_dir)
