"""
Professional Display Calibration GUI

Comprehensive calibration interface combining:
- DDC/CI hardware control (brightness, contrast, RGB gains/offsets, gamma)
- System-wide 3D LUT via dwm_lut (HDR and SDR)
- ICC profile generation
- Scientifically accurate color transformations

This application requires Administrator privileges for:
- DDC/CI monitor access
- dwm_lut DWM injection
- Writing to system LUT directory

Designed to match or exceed Light Illusion ColourSpace and DisplayCAL.
"""

import sys
import os
import ctypes
from pathlib import Path
from typing import Optional, Tuple, Dict, List
from dataclasses import dataclass
import json
import time

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def is_admin() -> bool:
    """Check if running with administrator privileges."""
    try:
        return ctypes.windll.shell32.IsUserAnAdmin()
    except Exception:
        return False


def run_as_admin():
    """Re-launch with administrator privileges."""
    if is_admin():
        return True
    try:
        script = os.path.abspath(sys.argv[0])
        params = ' '.join([f'"{arg}"' for arg in sys.argv[1:]])
        result = ctypes.windll.shell32.ShellExecuteW(
            None, "runas", sys.executable,
            f'"{script}" {params}', None, 1
        )
        if result > 32:
            sys.exit(0)
        return False
    except Exception:
        return False


try:
    from PyQt6.QtWidgets import (
        QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
        QLabel, QSlider, QPushButton, QComboBox, QGroupBox, QFormLayout,
        QSpinBox, QDoubleSpinBox, QCheckBox, QMessageBox, QTabWidget,
        QProgressBar, QTextEdit, QFrame, QSplitter, QScrollArea,
        QGridLayout, QSizePolicy, QDialog, QDialogButtonBox, QLineEdit
    )
    from PyQt6.QtCore import Qt, QTimer, pyqtSignal, QThread
    from PyQt6.QtGui import QFont, QColor, QPalette, QPainter, QBrush, QPen
except ImportError:
    from PyQt5.QtWidgets import (
        QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
        QLabel, QSlider, QPushButton, QComboBox, QGroupBox, QFormLayout,
        QSpinBox, QDoubleSpinBox, QCheckBox, QMessageBox, QTabWidget,
        QProgressBar, QTextEdit, QFrame, QSplitter, QScrollArea,
        QGridLayout, QSizePolicy, QDialog, QDialogButtonBox, QLineEdit
    )
    from PyQt5.QtCore import Qt, QTimer, pyqtSignal, QThread
    from PyQt5.QtGui import QFont, QColor, QPalette, QPainter, QBrush, QPen

import numpy as np

# Import calibration modules
from calibrate_pro.hardware.ddc_ci import DDCCIController, VCPCode, MonitorSettings
from calibrate_pro.lut_system.dwm_lut import (
    DwmLutController, LUTType, MonitorInfo,
    generate_hdr_calibration_lut, generate_sdr_calibration_lut,
    pq_eotf, pq_oetf, get_dwm_lut_directory
)
from calibrate_pro.core.color_math import (
    D65_WHITE, D50_WHITE, delta_e_2000, xyz_to_lab, srgb_to_xyz,
    cct_to_xy, xy_to_cct, bradford_adapt
)


# =============================================================================
# Custom Slider Widgets
# =============================================================================

class LabeledSlider(QWidget):
    """Slider with label and value display."""

    valueChanged = pyqtSignal(int)

    def __init__(self, label: str, min_val: int, max_val: int,
                 default: int, suffix: str = "", parent=None):
        super().__init__(parent)
        self.suffix = suffix

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 2, 0, 2)

        self.label = QLabel(label)
        self.label.setFixedWidth(100)
        layout.addWidget(self.label)

        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(min_val, max_val)
        self.slider.setValue(default)
        self.slider.valueChanged.connect(self._on_change)
        layout.addWidget(self.slider, 1)

        self.value_label = QLabel(f"{default}{suffix}")
        self.value_label.setFixedWidth(60)
        self.value_label.setAlignment(Qt.AlignmentFlag.AlignRight)
        layout.addWidget(self.value_label)

    def _on_change(self, val: int):
        self.value_label.setText(f"{val}{self.suffix}")
        self.valueChanged.emit(val)

    def value(self) -> int:
        return self.slider.value()

    def setValue(self, val: int):
        self.slider.setValue(val)


class FloatSlider(QWidget):
    """Slider for floating point values."""

    valueChanged = pyqtSignal(float)

    def __init__(self, label: str, min_val: float, max_val: float,
                 default: float, decimals: int = 3, parent=None):
        super().__init__(parent)
        self.min_val = min_val
        self.max_val = max_val
        self.decimals = decimals
        self.steps = 1000

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 2, 0, 2)

        self.label = QLabel(label)
        self.label.setFixedWidth(100)
        layout.addWidget(self.label)

        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(0, self.steps)
        self.slider.setValue(self._val_to_slider(default))
        self.slider.valueChanged.connect(self._on_change)
        layout.addWidget(self.slider, 1)

        self.value_label = QLabel(f"{default:.{decimals}f}")
        self.value_label.setFixedWidth(60)
        self.value_label.setAlignment(Qt.AlignmentFlag.AlignRight)
        layout.addWidget(self.value_label)

    def _val_to_slider(self, val: float) -> int:
        norm = (val - self.min_val) / (self.max_val - self.min_val)
        return int(norm * self.steps)

    def _slider_to_val(self, s: int) -> float:
        return self.min_val + (s / self.steps) * (self.max_val - self.min_val)

    def _on_change(self, s: int):
        val = self._slider_to_val(s)
        self.value_label.setText(f"{val:.{self.decimals}f}")
        self.valueChanged.emit(val)

    def value(self) -> float:
        return self._slider_to_val(self.slider.value())

    def setValue(self, val: float):
        self.slider.setValue(self._val_to_slider(val))


# =============================================================================
# Color Swatch Widget
# =============================================================================

class ColorSwatch(QWidget):
    """Display a color swatch."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.color = QColor(128, 128, 128)
        self.setMinimumSize(60, 60)
        self.setMaximumSize(60, 60)

    def setColor(self, r: int, g: int, b: int):
        self.color = QColor(r, g, b)
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setBrush(QBrush(self.color))
        painter.setPen(QPen(QColor(60, 60, 60), 2))
        painter.drawRoundedRect(2, 2, self.width()-4, self.height()-4, 5, 5)


# =============================================================================
# Main Calibration Window
# =============================================================================

class ProfessionalCalibrationWindow(QMainWindow):
    """Main Professional Calibration Window."""

    def __init__(self):
        super().__init__()

        # Initialize controllers
        self.ddc_controller = DDCCIController()
        self.lut_controller = DwmLutController()

        self.current_ddc_monitor = None
        self.current_lut_monitor = None
        self.live_ddc = False
        self.live_lut = False

        # Calibration state
        self.calibration_data = {
            'brightness': 50,
            'contrast': 80,
            'rgb_gains': [100, 100, 100],
            'rgb_offsets': [50, 50, 50],
            'gamma': 22,
            'lut_gains': [1.0, 1.0, 1.0],
            'lut_offsets': [0.0, 0.0, 0.0],
            'lut_gamma': 2.2,
            'peak_luminance': 1000,
        }

        self.setWindowTitle("Calibrate Pro - Professional Display Calibration")
        self.setMinimumSize(1200, 900)

        self._setup_ui()
        self._refresh_monitors()
        self._update_status()

        # Auto-start DwmLutGUI
        if is_admin() and self.lut_controller.is_available:
            QTimer.singleShot(500, self._auto_start_dwm)

        # Status update timer
        self.status_timer = QTimer()
        self.status_timer.timeout.connect(self._update_status)
        self.status_timer.start(2000)

    def _setup_ui(self):
        """Set up the user interface."""
        central = QWidget()
        self.setCentralWidget(central)
        main_layout = QHBoxLayout(central)

        # Left panel - Monitor & Status
        left_panel = QWidget()
        left_panel.setFixedWidth(350)
        left_layout = QVBoxLayout(left_panel)

        # Monitor Selection
        monitor_group = QGroupBox("Monitor Selection")
        monitor_layout = QVBoxLayout(monitor_group)

        self.monitor_combo = QComboBox()
        self.monitor_combo.currentIndexChanged.connect(self._on_monitor_changed)
        monitor_layout.addWidget(self.monitor_combo)

        refresh_btn = QPushButton("Refresh Monitors")
        refresh_btn.clicked.connect(self._refresh_monitors)
        monitor_layout.addWidget(refresh_btn)

        left_layout.addWidget(monitor_group)

        # System Status
        status_group = QGroupBox("System Status")
        status_layout = QFormLayout(status_group)

        self.status_admin = QLabel("Yes ✓" if is_admin() else "No ✗")
        self.status_admin.setStyleSheet("color: green; font-weight: bold;" if is_admin()
                                         else "color: red; font-weight: bold;")
        status_layout.addRow("Administrator:", self.status_admin)

        self.status_dwm = QLabel("Checking...")
        status_layout.addRow("DwmLutGUI:", self.status_dwm)

        self.status_ddc = QLabel("Checking...")
        status_layout.addRow("DDC/CI:", self.status_ddc)

        self.status_hdr = QLabel("Unknown")
        status_layout.addRow("HDR Mode:", self.status_hdr)

        left_layout.addWidget(status_group)

        # Monitor Capabilities
        caps_group = QGroupBox("Monitor Capabilities")
        caps_layout = QFormLayout(caps_group)

        self.cap_brightness = QLabel("--")
        self.cap_contrast = QLabel("--")
        self.cap_rgb_gain = QLabel("--")
        self.cap_rgb_offset = QLabel("--")
        self.cap_gamma = QLabel("--")

        caps_layout.addRow("Brightness:", self.cap_brightness)
        caps_layout.addRow("Contrast:", self.cap_contrast)
        caps_layout.addRow("RGB Gain:", self.cap_rgb_gain)
        caps_layout.addRow("RGB Offset:", self.cap_rgb_offset)
        caps_layout.addRow("Gamma:", self.cap_gamma)

        left_layout.addWidget(caps_group)

        # Current Settings Display
        current_group = QGroupBox("Current Hardware Settings")
        current_layout = QFormLayout(current_group)

        self.cur_brightness = QLabel("--")
        self.cur_contrast = QLabel("--")
        self.cur_rgb = QLabel("--")
        self.cur_offset = QLabel("--")

        current_layout.addRow("Brightness:", self.cur_brightness)
        current_layout.addRow("Contrast:", self.cur_contrast)
        current_layout.addRow("RGB Gains:", self.cur_rgb)
        current_layout.addRow("RGB Offsets:", self.cur_offset)

        read_btn = QPushButton("Read Current Settings")
        read_btn.clicked.connect(self._read_current_settings)
        current_layout.addRow(read_btn)

        left_layout.addWidget(current_group)

        # Color Preview
        preview_group = QGroupBox("Color Preview")
        preview_layout = QHBoxLayout(preview_group)

        self.white_swatch = ColorSwatch()
        self.white_swatch.setColor(255, 255, 255)
        preview_layout.addWidget(QLabel("White:"))
        preview_layout.addWidget(self.white_swatch)

        self.gray_swatch = ColorSwatch()
        self.gray_swatch.setColor(128, 128, 128)
        preview_layout.addWidget(QLabel("Gray:"))
        preview_layout.addWidget(self.gray_swatch)

        self.black_swatch = ColorSwatch()
        self.black_swatch.setColor(0, 0, 0)
        preview_layout.addWidget(QLabel("Black:"))
        preview_layout.addWidget(self.black_swatch)

        left_layout.addWidget(preview_group)
        left_layout.addStretch()

        main_layout.addWidget(left_panel)

        # Right panel - Calibration Controls
        right_panel = QWidget()
        right_layout = QVBoxLayout(right_panel)

        # Tabs for different control types
        self.tabs = QTabWidget()

        # Tab 1: Hardware Controls (DDC/CI)
        hw_tab = self._create_hardware_tab()
        self.tabs.addTab(hw_tab, "Hardware (DDC/CI)")

        # Tab 2: SDR LUT Controls
        sdr_tab = self._create_sdr_lut_tab()
        self.tabs.addTab(sdr_tab, "SDR 3D LUT")

        # Tab 3: HDR LUT Controls
        hdr_tab = self._create_hdr_lut_tab()
        self.tabs.addTab(hdr_tab, "HDR 3D LUT")

        # Tab 4: Presets
        presets_tab = self._create_presets_tab()
        self.tabs.addTab(presets_tab, "Presets")

        right_layout.addWidget(self.tabs)

        # Log output
        log_group = QGroupBox("Activity Log")
        log_layout = QVBoxLayout(log_group)
        self.log = QTextEdit()
        self.log.setMaximumHeight(120)
        self.log.setReadOnly(True)
        log_layout.addWidget(self.log)
        right_layout.addWidget(log_group)

        main_layout.addWidget(right_panel, 1)

    def _create_hardware_tab(self) -> QWidget:
        """Create hardware (DDC/CI) control tab."""
        tab = QWidget()
        layout = QVBoxLayout(tab)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll_content = QWidget()
        scroll_layout = QVBoxLayout(scroll_content)

        # Luminance Controls
        lum_group = QGroupBox("Luminance")
        lum_layout = QVBoxLayout(lum_group)

        self.hw_brightness = LabeledSlider("Brightness:", 0, 100, 50)
        self.hw_brightness.valueChanged.connect(lambda v: self._on_hw_change('brightness', v))
        lum_layout.addWidget(self.hw_brightness)

        self.hw_contrast = LabeledSlider("Contrast:", 0, 100, 80)
        self.hw_contrast.valueChanged.connect(lambda v: self._on_hw_change('contrast', v))
        lum_layout.addWidget(self.hw_contrast)

        scroll_layout.addWidget(lum_group)

        # RGB Gains (White Point)
        gain_group = QGroupBox("RGB Gains (White Point Adjustment)")
        gain_layout = QVBoxLayout(gain_group)

        gain_info = QLabel("Adjust RGB gains to correct white point color temperature.\n"
                          "Higher values = more of that color in highlights.")
        gain_info.setStyleSheet("color: #888; font-size: 11px;")
        gain_layout.addWidget(gain_info)

        self.hw_red_gain = LabeledSlider("Red Gain:", 0, 100, 100)
        self.hw_red_gain.valueChanged.connect(lambda v: self._on_hw_change('red_gain', v))
        gain_layout.addWidget(self.hw_red_gain)

        self.hw_green_gain = LabeledSlider("Green Gain:", 0, 100, 100)
        self.hw_green_gain.valueChanged.connect(lambda v: self._on_hw_change('green_gain', v))
        gain_layout.addWidget(self.hw_green_gain)

        self.hw_blue_gain = LabeledSlider("Blue Gain:", 0, 100, 100)
        self.hw_blue_gain.valueChanged.connect(lambda v: self._on_hw_change('blue_gain', v))
        gain_layout.addWidget(self.hw_blue_gain)

        scroll_layout.addWidget(gain_group)

        # RGB Offsets (Black Point)
        offset_group = QGroupBox("RGB Offsets (Black Point Adjustment)")
        offset_layout = QVBoxLayout(offset_group)

        offset_info = QLabel("Adjust RGB offsets to correct black point and shadow tint.\n"
                            "Higher values = more of that color in shadows.")
        offset_info.setStyleSheet("color: #888; font-size: 11px;")
        offset_layout.addWidget(offset_info)

        self.hw_red_offset = LabeledSlider("Red Offset:", 0, 100, 50)
        self.hw_red_offset.valueChanged.connect(lambda v: self._on_hw_change('red_offset', v))
        offset_layout.addWidget(self.hw_red_offset)

        self.hw_green_offset = LabeledSlider("Green Offset:", 0, 100, 50)
        self.hw_green_offset.valueChanged.connect(lambda v: self._on_hw_change('green_offset', v))
        offset_layout.addWidget(self.hw_green_offset)

        self.hw_blue_offset = LabeledSlider("Blue Offset:", 0, 100, 50)
        self.hw_blue_offset.valueChanged.connect(lambda v: self._on_hw_change('blue_offset', v))
        offset_layout.addWidget(self.hw_blue_offset)

        scroll_layout.addWidget(offset_group)

        # Gamma
        gamma_group = QGroupBox("Gamma")
        gamma_layout = QVBoxLayout(gamma_group)

        gamma_info = QLabel("Select display gamma curve. Common values:\n"
                           "• 22 = 2.2 (sRGB standard)\n"
                           "• 24 = 2.4 (BT.1886 broadcast)")
        gamma_info.setStyleSheet("color: #888; font-size: 11px;")
        gamma_layout.addWidget(gamma_info)

        self.hw_gamma = LabeledSlider("Gamma:", 18, 28, 22)
        self.hw_gamma.valueChanged.connect(lambda v: self._on_hw_change('gamma', v))
        gamma_layout.addWidget(self.hw_gamma)

        scroll_layout.addWidget(gamma_group)
        scroll_layout.addStretch()

        scroll.setWidget(scroll_content)
        layout.addWidget(scroll)

        # Buttons
        btn_layout = QHBoxLayout()

        self.hw_live_check = QCheckBox("Live Update")
        self.hw_live_check.toggled.connect(lambda c: setattr(self, 'live_ddc', c))
        btn_layout.addWidget(self.hw_live_check)

        apply_btn = QPushButton("Apply Hardware Settings")
        apply_btn.clicked.connect(self._apply_hardware_settings)
        btn_layout.addWidget(apply_btn)

        reset_btn = QPushButton("Reset to Defaults")
        reset_btn.clicked.connect(self._reset_hardware_settings)
        btn_layout.addWidget(reset_btn)

        layout.addLayout(btn_layout)

        return tab

    def _create_sdr_lut_tab(self) -> QWidget:
        """Create SDR 3D LUT control tab."""
        tab = QWidget()
        layout = QVBoxLayout(tab)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll_content = QWidget()
        scroll_layout = QVBoxLayout(scroll_content)

        info = QLabel("SDR 3D LUT provides software-level color correction.\n"
                     "Applied via DWM for system-wide effect on all applications.")
        info.setStyleSheet("color: #888;")
        scroll_layout.addWidget(info)

        # Gamma
        gamma_group = QGroupBox("Target Gamma")
        gamma_layout = QVBoxLayout(gamma_group)

        self.sdr_gamma = FloatSlider("Gamma:", 1.8, 2.8, 2.2, 2)
        gamma_layout.addWidget(self.sdr_gamma)

        scroll_layout.addWidget(gamma_group)

        # RGB Gains
        gain_group = QGroupBox("RGB Gains")
        gain_layout = QVBoxLayout(gain_group)

        self.sdr_r_gain = FloatSlider("Red:", 0.5, 1.5, 1.0, 3)
        gain_layout.addWidget(self.sdr_r_gain)

        self.sdr_g_gain = FloatSlider("Green:", 0.5, 1.5, 1.0, 3)
        gain_layout.addWidget(self.sdr_g_gain)

        self.sdr_b_gain = FloatSlider("Blue:", 0.5, 1.5, 1.0, 3)
        gain_layout.addWidget(self.sdr_b_gain)

        scroll_layout.addWidget(gain_group)

        # RGB Offsets
        offset_group = QGroupBox("RGB Offsets")
        offset_layout = QVBoxLayout(offset_group)

        self.sdr_r_offset = FloatSlider("Red:", -0.1, 0.1, 0.0, 4)
        offset_layout.addWidget(self.sdr_r_offset)

        self.sdr_g_offset = FloatSlider("Green:", -0.1, 0.1, 0.0, 4)
        offset_layout.addWidget(self.sdr_g_offset)

        self.sdr_b_offset = FloatSlider("Blue:", -0.1, 0.1, 0.0, 4)
        offset_layout.addWidget(self.sdr_b_offset)

        scroll_layout.addWidget(offset_group)

        # LUT Settings
        lut_group = QGroupBox("LUT Settings")
        lut_layout = QFormLayout(lut_group)

        self.sdr_lut_size = QComboBox()
        self.sdr_lut_size.addItems(["17³ (Fast)", "33³ (Standard)", "65³ (High Quality)"])
        self.sdr_lut_size.setCurrentIndex(1)
        lut_layout.addRow("LUT Size:", self.sdr_lut_size)

        scroll_layout.addWidget(lut_group)
        scroll_layout.addStretch()

        scroll.setWidget(scroll_content)
        layout.addWidget(scroll)

        # Buttons
        btn_layout = QHBoxLayout()

        self.sdr_live_check = QCheckBox("Live Update")
        btn_layout.addWidget(self.sdr_live_check)

        apply_btn = QPushButton("Apply SDR LUT")
        apply_btn.clicked.connect(self._apply_sdr_lut)
        btn_layout.addWidget(apply_btn)

        remove_btn = QPushButton("Remove LUT")
        remove_btn.clicked.connect(lambda: self._remove_lut(LUTType.SDR))
        btn_layout.addWidget(remove_btn)

        layout.addLayout(btn_layout)

        return tab

    def _create_hdr_lut_tab(self) -> QWidget:
        """Create HDR 3D LUT control tab."""
        tab = QWidget()
        layout = QVBoxLayout(tab)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll_content = QWidget()
        scroll_layout = QVBoxLayout(scroll_content)

        info = QLabel("HDR 3D LUT for HDR10/PQ content calibration.\n"
                     "Uses ST.2084 PQ EOTF for accurate HDR color correction.")
        info.setStyleSheet("color: #888;")
        scroll_layout.addWidget(info)

        # Display Capabilities
        display_group = QGroupBox("Display Capabilities")
        display_layout = QFormLayout(display_group)

        self.hdr_peak = QSpinBox()
        self.hdr_peak.setRange(400, 10000)
        self.hdr_peak.setValue(1000)
        self.hdr_peak.setSuffix(" nits")
        display_layout.addRow("Peak Luminance:", self.hdr_peak)

        self.hdr_sdr_white = QSpinBox()
        self.hdr_sdr_white.setRange(80, 500)
        self.hdr_sdr_white.setValue(203)
        self.hdr_sdr_white.setSuffix(" nits")
        display_layout.addRow("SDR White Level:", self.hdr_sdr_white)

        scroll_layout.addWidget(display_group)

        # RGB Gains
        gain_group = QGroupBox("RGB Gains")
        gain_layout = QVBoxLayout(gain_group)

        self.hdr_r_gain = FloatSlider("Red:", 0.5, 1.5, 1.0, 3)
        gain_layout.addWidget(self.hdr_r_gain)

        self.hdr_g_gain = FloatSlider("Green:", 0.5, 1.5, 1.0, 3)
        gain_layout.addWidget(self.hdr_g_gain)

        self.hdr_b_gain = FloatSlider("Blue:", 0.5, 1.5, 1.0, 3)
        gain_layout.addWidget(self.hdr_b_gain)

        scroll_layout.addWidget(gain_group)

        # RGB Offsets
        offset_group = QGroupBox("RGB Offsets")
        offset_layout = QVBoxLayout(offset_group)

        self.hdr_r_offset = FloatSlider("Red:", -0.05, 0.05, 0.0, 4)
        offset_layout.addWidget(self.hdr_r_offset)

        self.hdr_g_offset = FloatSlider("Green:", -0.05, 0.05, 0.0, 4)
        offset_layout.addWidget(self.hdr_g_offset)

        self.hdr_b_offset = FloatSlider("Blue:", -0.05, 0.05, 0.0, 4)
        offset_layout.addWidget(self.hdr_b_offset)

        scroll_layout.addWidget(offset_group)

        # LUT Settings
        lut_group = QGroupBox("LUT Settings")
        lut_layout = QFormLayout(lut_group)

        self.hdr_lut_size = QComboBox()
        self.hdr_lut_size.addItems(["17³ (Fast)", "33³ (Standard)", "65³ (High Quality)"])
        self.hdr_lut_size.setCurrentIndex(1)
        lut_layout.addRow("LUT Size:", self.hdr_lut_size)

        scroll_layout.addWidget(lut_group)
        scroll_layout.addStretch()

        scroll.setWidget(scroll_content)
        layout.addWidget(scroll)

        # Buttons
        btn_layout = QHBoxLayout()

        self.hdr_live_check = QCheckBox("Live Update")
        btn_layout.addWidget(self.hdr_live_check)

        apply_btn = QPushButton("Apply HDR LUT")
        apply_btn.clicked.connect(self._apply_hdr_lut)
        btn_layout.addWidget(apply_btn)

        remove_btn = QPushButton("Remove LUT")
        remove_btn.clicked.connect(lambda: self._remove_lut(LUTType.HDR))
        btn_layout.addWidget(remove_btn)

        layout.addLayout(btn_layout)

        return tab

    def _create_presets_tab(self) -> QWidget:
        """Create presets tab."""
        tab = QWidget()
        layout = QVBoxLayout(tab)

        info = QLabel("Apply pre-configured calibration presets for common targets.")
        info.setStyleSheet("color: #888;")
        layout.addWidget(info)

        # Standard Presets
        std_group = QGroupBox("Standard Presets")
        std_layout = QVBoxLayout(std_group)

        presets = [
            ("D65 sRGB (Gamma 2.2)", "Standard web/photo preset"),
            ("D65 BT.1886 (Gamma 2.4)", "Broadcast reference preset"),
            ("D65 Linear", "Linear gamma for compositing"),
            ("DCI-P3 D65", "Wide gamut cinema preset"),
            ("Native (Identity)", "No correction, monitor native"),
        ]

        for name, desc in presets:
            btn = QPushButton(name)
            btn.setToolTip(desc)
            btn.clicked.connect(lambda c, n=name: self._apply_preset(n))
            std_layout.addWidget(btn)

        layout.addWidget(std_group)

        # Custom Presets
        custom_group = QGroupBox("Custom Presets")
        custom_layout = QVBoxLayout(custom_group)

        save_btn = QPushButton("Save Current as Preset...")
        save_btn.clicked.connect(self._save_preset)
        custom_layout.addWidget(save_btn)

        load_btn = QPushButton("Load Custom Preset...")
        load_btn.clicked.connect(self._load_preset)
        custom_layout.addWidget(load_btn)

        layout.addWidget(custom_group)

        # Factory Reset
        reset_group = QGroupBox("Factory Reset")
        reset_layout = QVBoxLayout(reset_group)

        factory_btn = QPushButton("Reset Monitor to Factory Defaults")
        factory_btn.setStyleSheet("background-color: #8B0000;")
        factory_btn.clicked.connect(self._factory_reset)
        reset_layout.addWidget(factory_btn)

        layout.addWidget(reset_group)
        layout.addStretch()

        return tab

    # =========================================================================
    # Monitor Management
    # =========================================================================

    def _log(self, msg: str):
        """Add message to log."""
        self.log.append(f"[{time.strftime('%H:%M:%S')}] {msg}")
        self.log.verticalScrollBar().setValue(self.log.verticalScrollBar().maximum())

    def _refresh_monitors(self):
        """Refresh monitor list."""
        self.monitor_combo.clear()

        # Get DDC monitors
        ddc_monitors = self.ddc_controller.enumerate_monitors()

        # Get LUT monitors
        lut_monitors = self.lut_controller.get_monitors()

        # Combine and display
        for i, lut_mon in enumerate(lut_monitors):
            hdr_str = " [HDR]" if lut_mon.is_hdr else ""
            primary_str = " (Primary)" if lut_mon.is_primary else ""

            # Find matching DDC monitor
            ddc_mon = ddc_monitors[i] if i < len(ddc_monitors) else None
            ddc_str = " [DDC/CI]" if ddc_mon else ""

            self.monitor_combo.addItem(
                f"{lut_mon.friendly_name}{primary_str}{hdr_str}{ddc_str}",
                {'lut': lut_mon, 'ddc': ddc_mon, 'index': i}
            )

        self._log(f"Found {len(lut_monitors)} monitor(s)")

    def _on_monitor_changed(self, index: int):
        """Handle monitor selection change."""
        if index >= 0:
            data = self.monitor_combo.itemData(index)
            if data:
                self.current_lut_monitor = data.get('lut')
                self.current_ddc_monitor = data.get('ddc')
                self._update_capabilities()
                self._read_current_settings()

    def _update_capabilities(self):
        """Update capability display for selected monitor."""
        if self.current_ddc_monitor:
            caps = self.current_ddc_monitor.get('capabilities')
            if caps:
                self.cap_brightness.setText("✓" if 0x10 in caps.supported_vcp_codes else "✗")
                self.cap_contrast.setText("✓" if 0x12 in caps.supported_vcp_codes else "✗")
                self.cap_rgb_gain.setText("✓" if caps.has_rgb_gain else "✗")
                self.cap_rgb_offset.setText("✓" if caps.has_rgb_black_level else "✗")
                self.cap_gamma.setText("✓" if 0xF2 in caps.supported_vcp_codes or 0x72 in caps.supported_vcp_codes else "✗")
                return

        self.cap_brightness.setText("--")
        self.cap_contrast.setText("--")
        self.cap_rgb_gain.setText("--")
        self.cap_rgb_offset.setText("--")
        self.cap_gamma.setText("--")

    def _update_status(self):
        """Update status display."""
        # DwmLutGUI status
        if self.lut_controller._is_dwm_lut_running():
            self.status_dwm.setText("Running ✓")
            self.status_dwm.setStyleSheet("color: green; font-weight: bold;")
        else:
            self.status_dwm.setText("Not running ✗")
            self.status_dwm.setStyleSheet("color: red;")

        # DDC/CI status
        if self.current_ddc_monitor:
            self.status_ddc.setText("Connected ✓")
            self.status_ddc.setStyleSheet("color: green;")
        else:
            self.status_ddc.setText("Not available")
            self.status_ddc.setStyleSheet("color: orange;")

        # HDR status
        if self.current_lut_monitor and self.current_lut_monitor.is_hdr:
            self.status_hdr.setText("Enabled ✓")
            self.status_hdr.setStyleSheet("color: green;")
        else:
            self.status_hdr.setText("Disabled")
            self.status_hdr.setStyleSheet("color: gray;")

    def _read_current_settings(self):
        """Read current hardware settings from monitor."""
        if not self.current_ddc_monitor:
            return

        try:
            settings = self.ddc_controller.get_settings(self.current_ddc_monitor)

            self.cur_brightness.setText(str(settings.brightness))
            self.cur_contrast.setText(str(settings.contrast))
            self.cur_rgb.setText(f"R:{settings.red_gain} G:{settings.green_gain} B:{settings.blue_gain}")
            self.cur_offset.setText(f"R:{settings.red_black_level} G:{settings.green_black_level} B:{settings.blue_black_level}")

            # Update sliders
            self.hw_brightness.setValue(settings.brightness)
            self.hw_contrast.setValue(settings.contrast)
            self.hw_red_gain.setValue(settings.red_gain)
            self.hw_green_gain.setValue(settings.green_gain)
            self.hw_blue_gain.setValue(settings.blue_gain)
            self.hw_red_offset.setValue(settings.red_black_level)
            self.hw_green_offset.setValue(settings.green_black_level)
            self.hw_blue_offset.setValue(settings.blue_black_level)

            self._log("Read current hardware settings")

        except Exception as e:
            self._log(f"Error reading settings: {e}")

    def _auto_start_dwm(self):
        """Auto-start DwmLutGUI."""
        if not self.lut_controller._is_dwm_lut_running():
            try:
                self.lut_controller.start_dwm_lut_gui()
                self._log("Auto-started DwmLutGUI")
            except Exception as e:
                self._log(f"Could not start DwmLutGUI: {e}")

    # =========================================================================
    # Hardware Control
    # =========================================================================

    def _on_hw_change(self, setting: str, value: int):
        """Handle hardware setting change."""
        if self.live_ddc and self.current_ddc_monitor:
            self._apply_single_hw_setting(setting, value)

    def _apply_single_hw_setting(self, setting: str, value: int):
        """Apply a single hardware setting."""
        if not self.current_ddc_monitor:
            return

        code_map = {
            'brightness': VCPCode.BRIGHTNESS,
            'contrast': VCPCode.CONTRAST,
            'red_gain': VCPCode.RED_GAIN,
            'green_gain': VCPCode.GREEN_GAIN,
            'blue_gain': VCPCode.BLUE_GAIN,
            'red_offset': VCPCode.RED_BLACK_LEVEL,
            'green_offset': VCPCode.GREEN_BLACK_LEVEL,
            'blue_offset': VCPCode.BLUE_BLACK_LEVEL,
            'gamma': 0xF2,  # Manufacturer-specific gamma
        }

        if setting in code_map:
            try:
                self.ddc_controller.set_vcp(self.current_ddc_monitor, code_map[setting], value)
            except Exception as e:
                self._log(f"DDC error: {e}")

    def _apply_hardware_settings(self):
        """Apply all hardware settings."""
        if not self.current_ddc_monitor:
            self._log("No DDC/CI monitor selected")
            return

        try:
            # Brightness and Contrast
            self.ddc_controller.set_vcp(self.current_ddc_monitor, VCPCode.BRIGHTNESS,
                                        self.hw_brightness.value())
            self.ddc_controller.set_vcp(self.current_ddc_monitor, VCPCode.CONTRAST,
                                        self.hw_contrast.value())

            # RGB Gains
            self.ddc_controller.set_rgb_gain(
                self.current_ddc_monitor,
                self.hw_red_gain.value(),
                self.hw_green_gain.value(),
                self.hw_blue_gain.value()
            )

            # RGB Offsets
            self.ddc_controller.set_rgb_black_level(
                self.current_ddc_monitor,
                self.hw_red_offset.value(),
                self.hw_green_offset.value(),
                self.hw_blue_offset.value()
            )

            # Gamma (try both common codes)
            try:
                self.ddc_controller.set_vcp(self.current_ddc_monitor, 0xF2, self.hw_gamma.value())
            except:
                try:
                    self.ddc_controller.set_vcp(self.current_ddc_monitor, 0x72, self.hw_gamma.value())
                except:
                    pass

            self._log("Applied hardware settings via DDC/CI")
            self._read_current_settings()

        except Exception as e:
            self._log(f"Error applying hardware settings: {e}")

    def _reset_hardware_settings(self):
        """Reset hardware to default values."""
        self.hw_brightness.setValue(50)
        self.hw_contrast.setValue(80)
        self.hw_red_gain.setValue(100)
        self.hw_green_gain.setValue(100)
        self.hw_blue_gain.setValue(100)
        self.hw_red_offset.setValue(50)
        self.hw_green_offset.setValue(50)
        self.hw_blue_offset.setValue(50)
        self.hw_gamma.setValue(22)

        self._apply_hardware_settings()

    # =========================================================================
    # LUT Control
    # =========================================================================

    def _get_lut_size(self, combo: QComboBox) -> int:
        """Get LUT size from combo box."""
        sizes = [17, 33, 65]
        return sizes[combo.currentIndex()]

    def _apply_sdr_lut(self):
        """Apply SDR 3D LUT."""
        if not self.current_lut_monitor:
            self._log("No monitor selected")
            return

        try:
            size = self._get_lut_size(self.sdr_lut_size)

            lut = generate_sdr_calibration_lut(
                size=size,
                target_gamma=self.sdr_gamma.value(),
                rgb_gains=(
                    self.sdr_r_gain.value(),
                    self.sdr_g_gain.value(),
                    self.sdr_b_gain.value()
                ),
                rgb_offsets=(
                    self.sdr_r_offset.value(),
                    self.sdr_g_offset.value(),
                    self.sdr_b_offset.value()
                )
            )

            success = self.lut_controller.load_lut(
                self.current_lut_monitor, lut, LUTType.SDR,
                "SDR Calibration LUT"
            )

            if success:
                self._log(f"Applied SDR LUT ({size}³)")
            else:
                self._log("Failed to apply SDR LUT")

        except Exception as e:
            self._log(f"Error: {e}")

    def _apply_hdr_lut(self):
        """Apply HDR 3D LUT."""
        if not self.current_lut_monitor:
            self._log("No monitor selected")
            return

        try:
            size = self._get_lut_size(self.hdr_lut_size)

            lut = generate_hdr_calibration_lut(
                size=size,
                rgb_gains=(
                    self.hdr_r_gain.value(),
                    self.hdr_g_gain.value(),
                    self.hdr_b_gain.value()
                ),
                rgb_offsets=(
                    self.hdr_r_offset.value(),
                    self.hdr_g_offset.value(),
                    self.hdr_b_offset.value()
                ),
                peak_luminance=float(self.hdr_peak.value())
            )

            success = self.lut_controller.load_lut(
                self.current_lut_monitor, lut, LUTType.HDR,
                f"HDR Calibration LUT - Peak {self.hdr_peak.value()} nits"
            )

            if success:
                self._log(f"Applied HDR LUT ({size}³)")
            else:
                self._log("Failed to apply HDR LUT")

        except Exception as e:
            self._log(f"Error: {e}")

    def _remove_lut(self, lut_type: LUTType):
        """Remove LUT from monitor."""
        if not self.current_lut_monitor:
            return

        try:
            self.lut_controller.unload_lut(self.current_lut_monitor, lut_type)
            self._log(f"Removed {lut_type.value.upper()} LUT")
        except Exception as e:
            self._log(f"Error: {e}")

    # =========================================================================
    # Presets
    # =========================================================================

    def _apply_preset(self, name: str):
        """Apply a calibration preset."""
        presets = {
            "D65 sRGB (Gamma 2.2)": {
                'hw': {'brightness': 50, 'contrast': 80, 'rgb_gains': (100, 100, 100),
                       'rgb_offsets': (50, 50, 50), 'gamma': 22},
                'sdr': {'gamma': 2.2, 'gains': (1.0, 1.0, 1.0), 'offsets': (0.0, 0.0, 0.0)}
            },
            "D65 BT.1886 (Gamma 2.4)": {
                'hw': {'brightness': 50, 'contrast': 80, 'rgb_gains': (100, 100, 100),
                       'rgb_offsets': (50, 50, 50), 'gamma': 24},
                'sdr': {'gamma': 2.4, 'gains': (1.0, 1.0, 1.0), 'offsets': (0.0, 0.0, 0.0)}
            },
            "D65 Linear": {
                'hw': {'brightness': 50, 'contrast': 80, 'rgb_gains': (100, 100, 100),
                       'rgb_offsets': (50, 50, 50), 'gamma': 10},
                'sdr': {'gamma': 1.0, 'gains': (1.0, 1.0, 1.0), 'offsets': (0.0, 0.0, 0.0)}
            },
            "Native (Identity)": {
                'hw': {'brightness': 50, 'contrast': 80, 'rgb_gains': (100, 100, 100),
                       'rgb_offsets': (50, 50, 50), 'gamma': 22},
                'sdr': {'gamma': 2.2, 'gains': (1.0, 1.0, 1.0), 'offsets': (0.0, 0.0, 0.0)}
            },
        }

        if name in presets:
            preset = presets[name]

            # Apply hardware settings
            hw = preset.get('hw', {})
            self.hw_brightness.setValue(hw.get('brightness', 50))
            self.hw_contrast.setValue(hw.get('contrast', 80))
            gains = hw.get('rgb_gains', (100, 100, 100))
            self.hw_red_gain.setValue(gains[0])
            self.hw_green_gain.setValue(gains[1])
            self.hw_blue_gain.setValue(gains[2])
            offsets = hw.get('rgb_offsets', (50, 50, 50))
            self.hw_red_offset.setValue(offsets[0])
            self.hw_green_offset.setValue(offsets[1])
            self.hw_blue_offset.setValue(offsets[2])
            self.hw_gamma.setValue(hw.get('gamma', 22))

            # Apply SDR LUT settings
            sdr = preset.get('sdr', {})
            self.sdr_gamma.setValue(sdr.get('gamma', 2.2))
            sdr_gains = sdr.get('gains', (1.0, 1.0, 1.0))
            self.sdr_r_gain.setValue(sdr_gains[0])
            self.sdr_g_gain.setValue(sdr_gains[1])
            self.sdr_b_gain.setValue(sdr_gains[2])
            sdr_offsets = sdr.get('offsets', (0.0, 0.0, 0.0))
            self.sdr_r_offset.setValue(sdr_offsets[0])
            self.sdr_g_offset.setValue(sdr_offsets[1])
            self.sdr_b_offset.setValue(sdr_offsets[2])

            self._apply_hardware_settings()
            self._apply_sdr_lut()

            self._log(f"Applied preset: {name}")

    def _save_preset(self):
        """Save current settings as preset."""
        # TODO: Implement preset saving dialog
        self._log("Save preset: Not yet implemented")

    def _load_preset(self):
        """Load custom preset."""
        # TODO: Implement preset loading dialog
        self._log("Load preset: Not yet implemented")

    def _factory_reset(self):
        """Reset monitor to factory defaults."""
        reply = QMessageBox.question(
            self, "Factory Reset",
            "This will reset the monitor to factory defaults.\nContinue?",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
        )

        if reply == QMessageBox.StandardButton.Yes and self.current_ddc_monitor:
            try:
                self.ddc_controller.set_vcp(self.current_ddc_monitor, VCPCode.RESTORE_FACTORY_DEFAULTS, 1)
                self._log("Factory reset sent")
                QTimer.singleShot(2000, self._read_current_settings)
            except Exception as e:
                self._log(f"Factory reset failed: {e}")


def main():
    """Main entry point."""
    # Request admin privileges
    if not is_admin():
        run_as_admin()
        return

    app = QApplication(sys.argv)

    # Dark theme
    palette = QPalette()
    palette.setColor(QPalette.ColorRole.Window, QColor(45, 45, 45))
    palette.setColor(QPalette.ColorRole.WindowText, QColor(220, 220, 220))
    palette.setColor(QPalette.ColorRole.Base, QColor(30, 30, 30))
    palette.setColor(QPalette.ColorRole.AlternateBase, QColor(45, 45, 45))
    palette.setColor(QPalette.ColorRole.Text, QColor(220, 220, 220))
    palette.setColor(QPalette.ColorRole.Button, QColor(55, 55, 55))
    palette.setColor(QPalette.ColorRole.ButtonText, QColor(220, 220, 220))
    palette.setColor(QPalette.ColorRole.Highlight, QColor(42, 130, 218))
    palette.setColor(QPalette.ColorRole.HighlightedText, QColor(0, 0, 0))
    app.setPalette(palette)

    app.setStyle("Fusion")

    window = ProfessionalCalibrationWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
