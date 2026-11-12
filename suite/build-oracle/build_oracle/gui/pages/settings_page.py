"""
Build Oracle -- Settings Page

Default configuration, export directory, and about section.
"""

from PyQt6.QtCore import QSettings, Qt
from PyQt6.QtWidgets import (
    QComboBox,
    QFileDialog,
    QGridLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

from build_oracle.gui.app import APP_NAME, APP_ORG, APP_VERSION, C, Card, Heading


class SettingsPage(QWidget):
    """Application settings and about information."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._settings = QSettings(APP_ORG, APP_NAME)

        scroll = QScrollArea(self)
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QScrollArea.Shape.NoFrame)

        container = QWidget()
        layout = QVBoxLayout(container)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        layout.addWidget(Heading("Settings"))

        subtitle = QLabel("Configure default behavior and preferences")
        subtitle.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        layout.addWidget(subtitle)

        # --- Defaults Card ---
        layout.addWidget(Heading("Defaults", level=2))

        defaults_card, defaults_layout = Card.with_layout(QGridLayout, margins=(20, 16, 20, 16), spacing=12)

        defaults_layout.addWidget(QLabel("Default Model:"), 0, 0)
        self._model_combo = QComboBox()
        self._model_combo.addItems(["ARIMA", "Prophet"])
        saved_model = self._settings.value("defaults/model", "ARIMA")
        idx = self._model_combo.findText(saved_model)
        if idx >= 0:
            self._model_combo.setCurrentIndex(idx)
        self._model_combo.currentTextChanged.connect(lambda t: self._settings.setValue("defaults/model", t))
        defaults_layout.addWidget(self._model_combo, 0, 1)

        defaults_layout.addWidget(QLabel("Default Horizon:"), 1, 0)
        self._horizon_spin = QSpinBox()
        self._horizon_spin.setRange(1, 365)
        self._horizon_spin.setValue(int(self._settings.value("defaults/horizon", 30)))
        self._horizon_spin.setSuffix(" steps")
        self._horizon_spin.valueChanged.connect(lambda v: self._settings.setValue("defaults/horizon", v))
        defaults_layout.addWidget(self._horizon_spin, 1, 1)

        layout.addWidget(defaults_card)

        # --- Export Card ---
        layout.addWidget(Heading("Export", level=2))

        export_card, export_layout = Card.with_layout(QGridLayout, margins=(20, 16, 20, 16), spacing=12)

        export_layout.addWidget(QLabel("Export Directory:"), 0, 0)

        self._export_label = QLabel(self._settings.value("export/directory", "Not set"))
        self._export_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT2};")
        self._export_label.setWordWrap(True)
        export_layout.addWidget(self._export_label, 0, 1)

        browse_btn = QPushButton("Browse...")
        browse_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        browse_btn.clicked.connect(self._browse_export)
        export_layout.addWidget(browse_btn, 0, 2)

        layout.addWidget(export_card)

        # --- About Card ---
        layout.addWidget(Heading("About", level=2))

        about_card, about_layout = Card.with_layout(QVBoxLayout, margins=(24, 20, 24, 20), spacing=8)

        app_title = QLabel(APP_NAME)
        app_title.setStyleSheet(f"font-size: 18px; font-weight: 600; color: {C.ACCENT_TX};")
        about_layout.addWidget(app_title)

        version_label = QLabel(f"Version {APP_VERSION}")
        version_label.setStyleSheet(f"font-size: 13px; color: {C.TEXT2};")
        about_layout.addWidget(version_label)

        desc = QLabel(
            "Professional time series forecasting workbench.\n"
            "ARIMA, Prophet-style models, changepoint detection,\n"
            "decomposition, and feature extraction.\n\n"
            "Pure NumPy/SciPy core with zero heavy dependencies.\n"
            "Part of the Build Universe."
        )
        desc.setStyleSheet(f"font-size: 12px; color: {C.TEXT2}; line-height: 1.5;")
        desc.setWordWrap(True)
        about_layout.addWidget(desc)

        sep = QLabel("")
        sep.setFixedHeight(1)
        sep.setStyleSheet(f"background: {C.BORDER};")
        about_layout.addWidget(sep)

        copyright_label = QLabel("2022-2026 Zain Dana Harper")
        copyright_label.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
        about_layout.addWidget(copyright_label)

        layout.addWidget(about_card)

        layout.addStretch()

        scroll.setWidget(container)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.addWidget(scroll)

    def _browse_export(self):
        directory = QFileDialog.getExistingDirectory(
            self,
            "Select Export Directory",
            self._settings.value("export/directory", ""),
        )
        if directory:
            self._settings.setValue("export/directory", directory)
            self._export_label.setText(directory)
