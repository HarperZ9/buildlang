"""
Calibrate Pro — Settings Page

Application settings: general preferences, calibration defaults,
file paths, and about information.
"""

import shutil
from pathlib import Path
from typing import Optional

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QFrame, QScrollArea, QSizePolicy, QCheckBox, QComboBox,
    QLineEdit, QFileDialog, QFormLayout,
)
from PyQt6.QtCore import Qt, QSettings

from calibrate_pro.gui.app import C, Card, Heading, Stat


# =============================================================================
# Constants
# =============================================================================

APP_ORG = "Build Universe"
APP_NAME = "Calibrate Pro"
APP_VERSION = "1.0.0"

DEFAULT_OUTPUT_DIR = str(
    Path.home() / "Documents" / "Calibrate Pro" / "Calibrations"
)


# =============================================================================
# Helpers
# =============================================================================

def _detect_argyll_path() -> str:
    """Try to find ArgyllCMS on the system PATH."""
    argyll = shutil.which("spotread") or shutil.which("dispcal")
    if argyll:
        return str(Path(argyll).parent)
    # Common install locations on Windows
    for candidate in [
        Path("C:/Program Files/ArgyllCMS/bin"),
        Path("C:/ArgyllCMS/bin"),
        Path.home() / "ArgyllCMS" / "bin",
    ]:
        if candidate.exists():
            return str(candidate)
    return ""


def _make_section_heading(text: str) -> QLabel:
    """Create a styled section heading label."""
    label = QLabel(text)
    label.setStyleSheet(
        f"font-size: 14px; font-weight: 500; color: {C.ACCENT_TX}; "
        f"padding-top: 6px;"
    )
    return label


def _make_browse_row(
    settings: QSettings,
    key: str,
    default: str,
    dialog_title: str,
    is_directory: bool = True,
):
    """Create a text field + browse button row, wired to QSettings."""
    row = QHBoxLayout()
    row.setSpacing(8)

    field = QLineEdit()
    field.setText(settings.value(key, default))
    field.setStyleSheet(
        f"QLineEdit {{ background: {C.SURFACE}; border: 1px solid {C.BORDER}; "
        f"border-radius: 8px; padding: 7px 12px; font-size: 12px; }}"
        f"QLineEdit:focus {{ border-color: {C.ACCENT}; }}"
    )
    row.addWidget(field, stretch=1)

    browse = QPushButton("Browse")
    browse.setFixedHeight(32)
    browse.setFixedWidth(80)
    browse.setStyleSheet(
        f"QPushButton {{ background: {C.SURFACE}; border: 1px solid {C.BORDER}; "
        f"border-radius: 10px; font-size: 11px; padding: 4px 12px; }}"
        f"QPushButton:hover {{ border-color: {C.ACCENT}; background: {C.SURFACE2}; }}"
    )

    def _browse():
        if is_directory:
            path = QFileDialog.getExistingDirectory(None, dialog_title, field.text())
        else:
            path, _ = QFileDialog.getOpenFileName(None, dialog_title, field.text())
        if path:
            field.setText(path)
            settings.setValue(key, path)

    browse.clicked.connect(_browse)
    row.addWidget(browse)

    # Save on edit
    field.editingFinished.connect(lambda: settings.setValue(key, field.text()))

    return row, field


# =============================================================================
# Settings Page
# =============================================================================

class SettingsPage(QWidget):
    """Application settings page."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._settings = QSettings(APP_ORG, APP_NAME)
        self._build()

    def _build(self):
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        outer.addWidget(scroll)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(32, 28, 32, 28)
        layout.setSpacing(20)

        # --- Page heading ---
        layout.addWidget(Heading("Settings"))

        # =================================================================
        # General section
        # =================================================================
        layout.addWidget(_make_section_heading("General"))

        general_card, general_layout = Card.with_layout(spacing=14)

        form_general = QFormLayout()
        form_general.setSpacing(14)
        form_general.setLabelAlignment(Qt.AlignmentFlag.AlignRight)
        form_general.setFormAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)

        # Start with Windows
        self._startup_cb = QCheckBox("Start with Windows")
        self._startup_cb.setToolTip(
            "Launch Calibrate Pro at login to maintain calibration.\n"
            "Reapplies your LUT and ICC profile automatically."
        )
        self._startup_cb.setChecked(
            self._settings.value("general/start_with_windows", False, type=bool)
        )
        self._startup_cb.toggled.connect(
            lambda v: self._settings.setValue("general/start_with_windows", v)
        )
        form_general.addRow("", self._startup_cb)

        # Minimize to tray
        self._tray_cb = QCheckBox("Minimize to tray on close")
        self._tray_cb.setToolTip(
            "Keep Calibrate Pro running in the system tray when closed.\n"
            "The calibration guard continues protecting your settings."
        )
        self._tray_cb.setChecked(
            self._settings.value("general/minimize_to_tray", True, type=bool)
        )
        self._tray_cb.toggled.connect(
            lambda v: self._settings.setValue("general/minimize_to_tray", v)
        )
        form_general.addRow("", self._tray_cb)

        # Default calibration target
        target_label = QLabel("Default target")
        target_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT};")
        self._target_combo = QComboBox()
        self._target_combo.addItems(["Native", "sRGB", "Display P3"])
        saved_target = self._settings.value("general/default_target", "sRGB")
        idx = self._target_combo.findText(saved_target)
        if idx >= 0:
            self._target_combo.setCurrentIndex(idx)
        self._target_combo.currentTextChanged.connect(
            lambda t: self._settings.setValue("general/default_target", t)
        )
        form_general.addRow(target_label, self._target_combo)

        general_layout.addLayout(form_general)
        layout.addWidget(general_card)

        # =================================================================
        # Calibration section
        # =================================================================
        layout.addWidget(_make_section_heading("Calibration"))

        cal_card, cal_layout = Card.with_layout(spacing=14)

        form_cal = QFormLayout()
        form_cal.setSpacing(14)
        form_cal.setLabelAlignment(Qt.AlignmentFlag.AlignRight)
        form_cal.setFormAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)

        # LUT size
        lut_label = QLabel("LUT size")
        lut_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT};")
        lut_label.setToolTip(
            "3D LUT grid resolution. Higher = more accurate, larger file.\n"
            "17: Fast, ~15 KB (preview/testing)\n"
            "33: Standard, ~108 KB (recommended)\n"
            "65: Maximum, ~823 KB (professional)"
        )
        self._lut_combo = QComboBox()
        self._lut_combo.addItems(["17", "33", "65"])
        saved_lut = self._settings.value("calibration/lut_size", "33")
        idx = self._lut_combo.findText(saved_lut)
        if idx >= 0:
            self._lut_combo.setCurrentIndex(idx)
        self._lut_combo.currentTextChanged.connect(
            lambda t: self._settings.setValue("calibration/lut_size", t)
        )
        form_cal.addRow(lut_label, self._lut_combo)

        # OLED compensation
        self._oled_cb = QCheckBox("OLED compensation")
        self._oled_cb.setToolTip(
            "Enable ABL (auto brightness limiting) and near-black\n"
            "compensation for OLED and QD-OLED panels.\n"
            "Auto-enabled for detected OLED displays."
        )
        self._oled_cb.setChecked(
            self._settings.value("calibration/oled_compensation", False, type=bool)
        )
        self._oled_cb.toggled.connect(
            lambda v: self._settings.setValue("calibration/oled_compensation", v)
        )
        form_cal.addRow("", self._oled_cb)

        # HDR mode
        self._hdr_cb = QCheckBox("HDR mode")
        self._hdr_cb.setToolTip(
            "Generate PQ (ST.2084) encoded LUT for HDR content.\n"
            "Uses JzAzBz perceptual space and BT.2390 EETF\n"
            "for tone mapping. Requires HDR enabled in Windows."
        )
        self._hdr_cb.setChecked(
            self._settings.value("calibration/hdr_mode", False, type=bool)
        )
        self._hdr_cb.toggled.connect(
            lambda v: self._settings.setValue("calibration/hdr_mode", v)
        )
        form_cal.addRow("", self._hdr_cb)

        cal_layout.addLayout(form_cal)
        layout.addWidget(cal_card)

        # =================================================================
        # Paths section
        # =================================================================
        layout.addWidget(_make_section_heading("Paths"))

        paths_card, paths_layout = Card.with_layout(spacing=14)

        form_paths = QFormLayout()
        form_paths.setSpacing(14)
        form_paths.setLabelAlignment(Qt.AlignmentFlag.AlignRight)
        form_paths.setFormAlignment(Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop)

        # Output directory
        out_label = QLabel("Output directory")
        out_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT};")
        out_row, self._output_field = _make_browse_row(
            self._settings,
            "paths/output_dir",
            DEFAULT_OUTPUT_DIR,
            "Select Output Directory",
            is_directory=True,
        )
        out_container = QWidget()
        out_container.setLayout(out_row)
        form_paths.addRow(out_label, out_container)

        # ArgyllCMS path
        argyll_label = QLabel("ArgyllCMS path")
        argyll_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT};")
        detected = _detect_argyll_path()
        argyll_row, self._argyll_field = _make_browse_row(
            self._settings,
            "paths/argyll_path",
            detected,
            "Select ArgyllCMS Directory",
            is_directory=True,
        )
        argyll_container = QWidget()
        argyll_container.setLayout(argyll_row)
        form_paths.addRow(argyll_label, argyll_container)

        if detected:
            detected_label = QLabel(f"Detected: {detected}")
            detected_label.setStyleSheet(f"font-size: 10px; color: {C.GREEN};")
            form_paths.addRow("", detected_label)
        else:
            not_found_label = QLabel("ArgyllCMS not found on PATH")
            not_found_label.setStyleSheet(f"font-size: 10px; color: {C.TEXT3};")
            form_paths.addRow("", not_found_label)

        paths_layout.addLayout(form_paths)
        layout.addWidget(paths_card)

        # =================================================================
        # About section
        # =================================================================
        layout.addWidget(_make_section_heading("About"))

        about_card, about_layout = Card.with_layout(spacing=10)

        version_label = QLabel(f"{APP_NAME}  v{APP_VERSION}")
        version_label.setStyleSheet(
            f"font-size: 15px; font-weight: 600; color: {C.TEXT};"
        )
        about_layout.addWidget(version_label)

        subtitle = QLabel("Professional display calibration for Windows")
        subtitle.setStyleSheet(f"font-size: 12px; color: {C.ACCENT_TX}; font-style: italic;")
        about_layout.addWidget(subtitle)

        # Separator
        sep = QFrame()
        sep.setFixedHeight(1)
        sep.setStyleSheet(f"background: {C.BORDER};")
        about_layout.addWidget(sep)

        details = [
            ("Color Science", "Oklab, JzAzBz, CAM16-UCS, PQ/HLG, ACES"),
            ("Gamut Mapping", "Oklab perceptual (SDR), JzCzhz (HDR)"),
            ("Native Sensor", "i1Display3 family via USB HID"),
            ("Spectral Correction", "CCMX for QD-OLED / WOLED"),
            ("Panel Database", "39 characterized displays"),
        ]
        for label, value in details:
            row = QHBoxLayout()
            row.setSpacing(8)
            lbl = QLabel(label)
            lbl.setStyleSheet(f"font-size: 11px; color: {C.TEXT3}; min-width: 120px;")
            row.addWidget(lbl)
            val = QLabel(value)
            val.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
            row.addWidget(val, stretch=1)
            about_layout.addLayout(row)

        # Separator
        sep2 = QFrame()
        sep2.setFixedHeight(1)
        sep2.setStyleSheet(f"background: {C.BORDER};")
        about_layout.addWidget(sep2)

        author_label = QLabel("Zain Dana Harper")
        author_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT};")
        about_layout.addWidget(author_label)

        copyright_label = QLabel("Copyright 2024-2026 Build Universe. All rights reserved.")
        copyright_label.setStyleSheet(f"font-size: 10px; color: {C.TEXT3};")
        about_layout.addWidget(copyright_label)

        layout.addWidget(about_card)

        # Bottom spacer
        layout.addStretch()
        scroll.setWidget(content)
