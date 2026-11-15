"""
HDR Calibration GUI

Provides a user interface for HDR display calibration using dwm_lut.
Generates HDR 3D LUTs with PQ EOTF for system-wide color correction.

Requires administrator privileges for DWM LUT injection.
"""

import sys
import os
import ctypes
from pathlib import Path
from typing import Optional, Tuple

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def is_admin() -> bool:
    """Check if running with administrator privileges."""
    try:
        return ctypes.windll.shell32.IsUserAnAdmin()
    except Exception:
        return False


def run_as_admin():
    """Re-launch the script with administrator privileges."""
    if is_admin():
        return True

    try:
        # Get the Python executable and script path
        script = os.path.abspath(sys.argv[0])
        params = ' '.join([f'"{arg}"' for arg in sys.argv[1:]])

        # Use ShellExecuteW to request elevation
        # verb="runas" triggers UAC prompt
        result = ctypes.windll.shell32.ShellExecuteW(
            None,           # hwnd
            "runas",        # lpOperation (run as admin)
            sys.executable, # lpFile (python.exe)
            f'"{script}" {params}',  # lpParameters
            None,           # lpDirectory
            1               # nShowCmd (SW_SHOWNORMAL)
        )

        # ShellExecuteW returns > 32 on success
        if result > 32:
            sys.exit(0)  # Exit the non-elevated instance
        else:
            return False
    except Exception as e:
        print(f"Failed to elevate: {e}")
        return False

try:
    from PyQt6.QtWidgets import (
        QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
        QLabel, QSlider, QPushButton, QComboBox, QGroupBox, QFormLayout,
        QSpinBox, QDoubleSpinBox, QCheckBox, QMessageBox, QTabWidget,
        QProgressBar, QTextEdit, QFrame, QSplitter
    )
    from PyQt6.QtCore import Qt, QTimer, pyqtSignal
    from PyQt6.QtGui import QFont, QColor, QPalette
except ImportError:
    from PyQt5.QtWidgets import (
        QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
        QLabel, QSlider, QPushButton, QComboBox, QGroupBox, QFormLayout,
        QSpinBox, QDoubleSpinBox, QCheckBox, QMessageBox, QTabWidget,
        QProgressBar, QTextEdit, QFrame, QSplitter
    )
    from PyQt5.QtCore import Qt, QTimer, pyqtSignal
    from PyQt5.QtGui import QFont, QColor, QPalette

import numpy as np

from calibrate_pro.lut_system.dwm_lut import (
    DwmLutController, LUTType, MonitorInfo,
    generate_hdr_calibration_lut, generate_sdr_calibration_lut,
    list_monitors, get_dwm_lut_directory, pq_eotf, pq_oetf
)


class GainSlider(QWidget):
    """Custom slider for RGB gain adjustments."""

    valueChanged = pyqtSignal(float)

    def __init__(self, label: str, min_val: float = 0.5, max_val: float = 1.5,
                 default: float = 1.0, parent=None):
        super().__init__(parent)
        self.min_val = min_val
        self.max_val = max_val
        self.steps = 200

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self.label = QLabel(label)
        self.label.setFixedWidth(80)
        layout.addWidget(self.label)

        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(0, self.steps)
        self.slider.setValue(self._value_to_slider(default))
        self.slider.valueChanged.connect(self._on_slider_change)
        layout.addWidget(self.slider, 1)

        self.value_label = QLabel(f"{default:.3f}")
        self.value_label.setFixedWidth(50)
        layout.addWidget(self.value_label)

    def _value_to_slider(self, value: float) -> int:
        normalized = (value - self.min_val) / (self.max_val - self.min_val)
        return int(normalized * self.steps)

    def _slider_to_value(self, slider_val: int) -> float:
        normalized = slider_val / self.steps
        return self.min_val + normalized * (self.max_val - self.min_val)

    def _on_slider_change(self, slider_val: int):
        value = self._slider_to_value(slider_val)
        self.value_label.setText(f"{value:.3f}")
        self.valueChanged.emit(value)

    def value(self) -> float:
        return self._slider_to_value(self.slider.value())

    def setValue(self, value: float):
        self.slider.setValue(self._value_to_slider(value))


class OffsetSlider(QWidget):
    """Custom slider for RGB offset adjustments."""

    valueChanged = pyqtSignal(float)

    def __init__(self, label: str, min_val: float = -0.1, max_val: float = 0.1,
                 default: float = 0.0, parent=None):
        super().__init__(parent)
        self.min_val = min_val
        self.max_val = max_val
        self.steps = 200

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self.label = QLabel(label)
        self.label.setFixedWidth(80)
        layout.addWidget(self.label)

        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(0, self.steps)
        self.slider.setValue(self._value_to_slider(default))
        self.slider.valueChanged.connect(self._on_slider_change)
        layout.addWidget(self.slider, 1)

        self.value_label = QLabel(f"{default:+.4f}")
        self.value_label.setFixedWidth(60)
        layout.addWidget(self.value_label)

    def _value_to_slider(self, value: float) -> int:
        normalized = (value - self.min_val) / (self.max_val - self.min_val)
        return int(normalized * self.steps)

    def _slider_to_value(self, slider_val: int) -> float:
        normalized = slider_val / self.steps
        return self.min_val + normalized * (self.max_val - self.min_val)

    def _on_slider_change(self, slider_val: int):
        value = self._slider_to_value(slider_val)
        self.value_label.setText(f"{value:+.4f}")
        self.valueChanged.emit(value)

    def value(self) -> float:
        return self._slider_to_value(self.slider.value())

    def setValue(self, value: float):
        self.slider.setValue(self._value_to_slider(value))


class HDRCalibrationWindow(QMainWindow):
    """Main HDR Calibration Window."""

    def __init__(self):
        super().__init__()
        self.controller = DwmLutController()
        self.current_monitor: Optional[MonitorInfo] = None
        self.live_update = False

        self.setWindowTitle("HDR Calibration - Calibrate Pro")
        self.setMinimumSize(800, 700)

        self._setup_ui()
        self._refresh_monitors()
        self._update_status()

        # Timer for live updates
        self.update_timer = QTimer()
        self.update_timer.timeout.connect(self._apply_lut)

        # Auto-start DwmLutGUI if we have admin rights
        if is_admin() and self.controller.is_available:
            QTimer.singleShot(500, self._auto_start_dwm_lut)

    def _setup_ui(self):
        """Set up the user interface."""
        central = QWidget()
        self.setCentralWidget(central)
        layout = QVBoxLayout(central)

        # Monitor selection
        monitor_group = QGroupBox("Monitor Selection")
        monitor_layout = QHBoxLayout(monitor_group)

        self.monitor_combo = QComboBox()
        self.monitor_combo.currentIndexChanged.connect(self._on_monitor_changed)
        monitor_layout.addWidget(QLabel("Monitor:"))
        monitor_layout.addWidget(self.monitor_combo, 1)

        refresh_btn = QPushButton("Refresh")
        refresh_btn.clicked.connect(self._refresh_monitors)
        monitor_layout.addWidget(refresh_btn)

        layout.addWidget(monitor_group)

        # Status panel
        status_group = QGroupBox("Status")
        status_layout = QFormLayout(status_group)

        self.status_admin = QLabel("Yes ✓" if is_admin() else "No")
        self.status_admin.setStyleSheet("color: green;" if is_admin() else "color: red;")
        self.status_dwm_lut = QLabel("Not running")
        self.status_hdr = QLabel("Unknown")
        self.status_lut = QLabel("None")

        status_layout.addRow("Administrator:", self.status_admin)
        status_layout.addRow("DwmLutGUI:", self.status_dwm_lut)
        status_layout.addRow("HDR Mode:", self.status_hdr)
        status_layout.addRow("Active LUT:", self.status_lut)

        layout.addWidget(status_group)

        # Tabs for SDR/HDR
        self.tabs = QTabWidget()

        # HDR Tab
        hdr_tab = QWidget()
        hdr_layout = QVBoxLayout(hdr_tab)

        # Peak luminance
        peak_group = QGroupBox("Display Capabilities")
        peak_layout = QFormLayout(peak_group)

        self.peak_luminance = QSpinBox()
        self.peak_luminance.setRange(100, 10000)
        self.peak_luminance.setValue(1000)
        self.peak_luminance.setSuffix(" nits")
        self.peak_luminance.valueChanged.connect(self._on_param_changed)
        peak_layout.addRow("Peak Luminance:", self.peak_luminance)

        self.sdr_white = QSpinBox()
        self.sdr_white.setRange(80, 500)
        self.sdr_white.setValue(203)
        self.sdr_white.setSuffix(" nits")
        self.sdr_white.valueChanged.connect(self._on_param_changed)
        peak_layout.addRow("SDR White Level:", self.sdr_white)

        hdr_layout.addWidget(peak_group)

        # RGB Gains
        gain_group = QGroupBox("RGB Gains (White Point Correction)")
        gain_layout = QVBoxLayout(gain_group)

        self.hdr_gain_r = GainSlider("Red Gain:", 0.5, 1.5, 1.0)
        self.hdr_gain_r.valueChanged.connect(self._on_param_changed)
        gain_layout.addWidget(self.hdr_gain_r)

        self.hdr_gain_g = GainSlider("Green Gain:", 0.5, 1.5, 1.0)
        self.hdr_gain_g.valueChanged.connect(self._on_param_changed)
        gain_layout.addWidget(self.hdr_gain_g)

        self.hdr_gain_b = GainSlider("Blue Gain:", 0.5, 1.5, 1.0)
        self.hdr_gain_b.valueChanged.connect(self._on_param_changed)
        gain_layout.addWidget(self.hdr_gain_b)

        hdr_layout.addWidget(gain_group)

        # RGB Offsets
        offset_group = QGroupBox("RGB Offsets (Black Level Correction)")
        offset_layout = QVBoxLayout(offset_group)

        self.hdr_offset_r = OffsetSlider("Red Offset:", -0.05, 0.05, 0.0)
        self.hdr_offset_r.valueChanged.connect(self._on_param_changed)
        offset_layout.addWidget(self.hdr_offset_r)

        self.hdr_offset_g = OffsetSlider("Green Offset:", -0.05, 0.05, 0.0)
        self.hdr_offset_g.valueChanged.connect(self._on_param_changed)
        offset_layout.addWidget(self.hdr_offset_g)

        self.hdr_offset_b = OffsetSlider("Blue Offset:", -0.05, 0.05, 0.0)
        self.hdr_offset_b.valueChanged.connect(self._on_param_changed)
        offset_layout.addWidget(self.hdr_offset_b)

        hdr_layout.addWidget(offset_group)

        # LUT Size
        lut_group = QGroupBox("LUT Settings")
        lut_layout = QFormLayout(lut_group)

        self.lut_size = QComboBox()
        self.lut_size.addItems(["17", "33", "65"])
        self.lut_size.setCurrentText("33")
        lut_layout.addRow("LUT Size:", self.lut_size)

        self.live_checkbox = QCheckBox("Live Update")
        self.live_checkbox.toggled.connect(self._on_live_toggle)
        lut_layout.addRow(self.live_checkbox)

        hdr_layout.addWidget(lut_group)
        hdr_layout.addStretch()

        self.tabs.addTab(hdr_tab, "HDR Calibration")

        # SDR Tab
        sdr_tab = QWidget()
        sdr_layout = QVBoxLayout(sdr_tab)

        # Gamma
        gamma_group = QGroupBox("Gamma Settings")
        gamma_layout = QFormLayout(gamma_group)

        self.target_gamma = QDoubleSpinBox()
        self.target_gamma.setRange(1.8, 2.8)
        self.target_gamma.setValue(2.2)
        self.target_gamma.setSingleStep(0.1)
        self.target_gamma.valueChanged.connect(self._on_param_changed)
        gamma_layout.addRow("Target Gamma:", self.target_gamma)

        sdr_layout.addWidget(gamma_group)

        # SDR RGB Gains
        sdr_gain_group = QGroupBox("RGB Gains")
        sdr_gain_layout = QVBoxLayout(sdr_gain_group)

        self.sdr_gain_r = GainSlider("Red Gain:", 0.5, 1.5, 1.0)
        self.sdr_gain_r.valueChanged.connect(self._on_param_changed)
        sdr_gain_layout.addWidget(self.sdr_gain_r)

        self.sdr_gain_g = GainSlider("Green Gain:", 0.5, 1.5, 1.0)
        self.sdr_gain_g.valueChanged.connect(self._on_param_changed)
        sdr_gain_layout.addWidget(self.sdr_gain_g)

        self.sdr_gain_b = GainSlider("Blue Gain:", 0.5, 1.5, 1.0)
        self.sdr_gain_b.valueChanged.connect(self._on_param_changed)
        sdr_gain_layout.addWidget(self.sdr_gain_b)

        sdr_layout.addWidget(sdr_gain_group)

        # SDR RGB Offsets
        sdr_offset_group = QGroupBox("RGB Offsets")
        sdr_offset_layout = QVBoxLayout(sdr_offset_group)

        self.sdr_offset_r = OffsetSlider("Red Offset:", -0.1, 0.1, 0.0)
        self.sdr_offset_r.valueChanged.connect(self._on_param_changed)
        sdr_offset_layout.addWidget(self.sdr_offset_r)

        self.sdr_offset_g = OffsetSlider("Green Offset:", -0.1, 0.1, 0.0)
        self.sdr_offset_g.valueChanged.connect(self._on_param_changed)
        sdr_offset_layout.addWidget(self.sdr_offset_g)

        self.sdr_offset_b = OffsetSlider("Blue Offset:", -0.1, 0.1, 0.0)
        self.sdr_offset_b.valueChanged.connect(self._on_param_changed)
        sdr_offset_layout.addWidget(self.sdr_offset_b)

        sdr_layout.addWidget(sdr_offset_group)
        sdr_layout.addStretch()

        self.tabs.addTab(sdr_tab, "SDR Calibration")

        layout.addWidget(self.tabs)

        # Buttons
        button_layout = QHBoxLayout()

        self.apply_btn = QPushButton("Apply LUT")
        self.apply_btn.clicked.connect(self._apply_lut)
        button_layout.addWidget(self.apply_btn)

        self.reset_btn = QPushButton("Reset to Identity")
        self.reset_btn.clicked.connect(self._reset_lut)
        button_layout.addWidget(self.reset_btn)

        self.remove_btn = QPushButton("Remove LUT")
        self.remove_btn.clicked.connect(self._remove_lut)
        button_layout.addWidget(self.remove_btn)

        button_layout.addStretch()

        self.start_dwm_btn = QPushButton("Start DwmLutGUI")
        self.start_dwm_btn.clicked.connect(self._start_dwm_lut)
        button_layout.addWidget(self.start_dwm_btn)

        layout.addLayout(button_layout)

        # Log output
        self.log = QTextEdit()
        self.log.setMaximumHeight(100)
        self.log.setReadOnly(True)
        layout.addWidget(self.log)

    def _log(self, message: str):
        """Add message to log."""
        self.log.append(message)
        self.log.verticalScrollBar().setValue(self.log.verticalScrollBar().maximum())

    def _refresh_monitors(self):
        """Refresh monitor list."""
        self.monitor_combo.clear()
        monitors = list_monitors()

        for m in monitors:
            hdr_str = " [HDR]" if m['is_hdr'] else ""
            primary_str = " (Primary)" if m['is_primary'] else ""
            self.monitor_combo.addItem(
                f"{m['friendly_name']}{primary_str}{hdr_str} - {m['size'][0]}x{m['size'][1]}",
                m
            )

        self._log(f"Found {len(monitors)} monitors")

    def _on_monitor_changed(self, index: int):
        """Handle monitor selection change."""
        if index >= 0:
            data = self.monitor_combo.itemData(index)
            monitors = self.controller.get_monitors()
            if data and 'index' in data:
                self.current_monitor = self.controller.get_monitor_by_index(data['index'])
                self._log(f"Selected: {data['friendly_name']}")

    def _update_status(self):
        """Update status panel."""
        # DwmLutGUI status
        if self.controller._is_dwm_lut_running():
            self.status_dwm_lut.setText("Running ✓")
            self.status_dwm_lut.setStyleSheet("color: green;")
        else:
            self.status_dwm_lut.setText("Not running")
            self.status_dwm_lut.setStyleSheet("color: red;")

        # HDR status
        if self.current_monitor:
            if self.current_monitor.is_hdr:
                self.status_hdr.setText("Enabled ✓")
                self.status_hdr.setStyleSheet("color: green;")
            else:
                self.status_hdr.setText("Disabled")
                self.status_hdr.setStyleSheet("color: orange;")

        # Active LUT status
        active = self.controller.get_active_luts()
        if active:
            lut_info = list(active.values())[0]
            self.status_lut.setText(f"{lut_info.lut_type.value.upper()} - {lut_info.lut_size}³")
        else:
            self.status_lut.setText("None")

    def _on_param_changed(self):
        """Handle parameter change."""
        if self.live_update:
            self.update_timer.stop()
            self.update_timer.start(200)  # Debounce: apply after 200ms

    def _on_live_toggle(self, checked: bool):
        """Handle live update toggle."""
        self.live_update = checked
        if checked:
            self._apply_lut()

    def _get_hdr_params(self) -> dict:
        """Get current HDR calibration parameters."""
        return {
            'rgb_gains': (
                self.hdr_gain_r.value(),
                self.hdr_gain_g.value(),
                self.hdr_gain_b.value()
            ),
            'rgb_offsets': (
                self.hdr_offset_r.value(),
                self.hdr_offset_g.value(),
                self.hdr_offset_b.value()
            ),
            'whitepoint': (1.0, 1.0, 1.0),
            'peak_luminance': float(self.peak_luminance.value()),
            'lut_size': int(self.lut_size.currentText())
        }

    def _get_sdr_params(self) -> dict:
        """Get current SDR calibration parameters."""
        return {
            'rgb_gains': (
                self.sdr_gain_r.value(),
                self.sdr_gain_g.value(),
                self.sdr_gain_b.value()
            ),
            'rgb_offsets': (
                self.sdr_offset_r.value(),
                self.sdr_offset_g.value(),
                self.sdr_offset_b.value()
            ),
            'whitepoint': (1.0, 1.0, 1.0),
            'target_gamma': self.target_gamma.value(),
            'lut_size': int(self.lut_size.currentText())
        }

    def _apply_lut(self):
        """Apply calibration LUT to selected monitor."""
        if not self.current_monitor:
            self._log("Error: No monitor selected")
            return

        try:
            # Determine if HDR or SDR based on tab
            is_hdr = self.tabs.currentIndex() == 0

            if is_hdr:
                params = self._get_hdr_params()
                lut = generate_hdr_calibration_lut(
                    size=params['lut_size'],
                    rgb_gains=params['rgb_gains'],
                    rgb_offsets=params['rgb_offsets'],
                    target_whitepoint=params['whitepoint'],
                    peak_luminance=params['peak_luminance']
                )
                lut_type = LUTType.HDR
                title = f"HDR Calibration - Peak {params['peak_luminance']} nits"
            else:
                params = self._get_sdr_params()
                lut = generate_sdr_calibration_lut(
                    size=params['lut_size'],
                    target_gamma=params['target_gamma'],
                    rgb_gains=params['rgb_gains'],
                    rgb_offsets=params['rgb_offsets'],
                    target_whitepoint=params['whitepoint']
                )
                lut_type = LUTType.SDR
                title = f"SDR Calibration - Gamma {params['target_gamma']}"

            # Apply LUT
            success = self.controller.load_lut(
                self.current_monitor,
                lut,
                lut_type,
                title
            )

            if success:
                self._log(f"Applied {lut_type.value.upper()} LUT: {params['lut_size']}³")
            else:
                self._log(f"Failed to apply LUT")

            self._update_status()

        except Exception as e:
            self._log(f"Error: {e}")

    def _reset_lut(self):
        """Reset to identity LUT."""
        if not self.current_monitor:
            self._log("Error: No monitor selected")
            return

        # Reset sliders
        self.hdr_gain_r.setValue(1.0)
        self.hdr_gain_g.setValue(1.0)
        self.hdr_gain_b.setValue(1.0)
        self.hdr_offset_r.setValue(0.0)
        self.hdr_offset_g.setValue(0.0)
        self.hdr_offset_b.setValue(0.0)

        self.sdr_gain_r.setValue(1.0)
        self.sdr_gain_g.setValue(1.0)
        self.sdr_gain_b.setValue(1.0)
        self.sdr_offset_r.setValue(0.0)
        self.sdr_offset_g.setValue(0.0)
        self.sdr_offset_b.setValue(0.0)

        self._apply_lut()
        self._log("Reset to identity LUT")

    def _remove_lut(self):
        """Remove LUT from monitor."""
        if not self.current_monitor:
            self._log("Error: No monitor selected")
            return

        try:
            is_hdr = self.tabs.currentIndex() == 0
            lut_type = LUTType.HDR if is_hdr else LUTType.SDR

            success = self.controller.unload_lut(self.current_monitor, lut_type)

            if success:
                self._log(f"Removed {lut_type.value.upper()} LUT")
            else:
                self._log(f"Failed to remove LUT")

            self._update_status()

        except Exception as e:
            self._log(f"Error: {e}")

    def _auto_start_dwm_lut(self):
        """Auto-start DwmLutGUI on launch."""
        if not self.controller._is_dwm_lut_running():
            try:
                self.controller.start_dwm_lut_gui()
                self._log("Auto-started DwmLutGUI")
                self._update_status()
            except Exception as e:
                self._log(f"Failed to auto-start DwmLutGUI: {e}")

    def _start_dwm_lut(self):
        """Start DwmLutGUI."""
        if not self.controller.is_available:
            QMessageBox.warning(
                self,
                "DwmLutGUI Not Found",
                "DwmLutGUI.exe was not found.\n\n"
                "Please download dwm_lut from:\n"
                "https://github.com/ledoge/dwm_lut/releases\n\n"
                "And extract it to the calibrate/dwm_lut folder."
            )
            return

        if self.controller._is_dwm_lut_running():
            self._log("DwmLutGUI is already running")
            return

        try:
            # Try to start
            self.controller.start_dwm_lut_gui()
            self._log("Started DwmLutGUI")
        except Exception as e:
            # Needs admin - show instructions
            QMessageBox.information(
                self,
                "Administrator Required",
                "DwmLutGUI requires administrator privileges.\n\n"
                f"Please manually run as Administrator:\n"
                f"{self.controller.dwm_lut_exe}\n\n"
                "After starting DwmLutGUI, click 'Apply LUT' to apply your calibration."
            )

        # Update status after delay
        QTimer.singleShot(2000, self._update_status)


def main():
    """Main entry point."""
    # Request administrator privileges if not already elevated
    if not is_admin():
        run_as_admin()
        return  # Exit if elevation was requested

    app = QApplication(sys.argv)

    # Set dark palette
    palette = QPalette()
    palette.setColor(QPalette.ColorRole.Window, QColor(53, 53, 53))
    palette.setColor(QPalette.ColorRole.WindowText, Qt.GlobalColor.white)
    palette.setColor(QPalette.ColorRole.Base, QColor(25, 25, 25))
    palette.setColor(QPalette.ColorRole.AlternateBase, QColor(53, 53, 53))
    palette.setColor(QPalette.ColorRole.ToolTipBase, Qt.GlobalColor.white)
    palette.setColor(QPalette.ColorRole.ToolTipText, Qt.GlobalColor.white)
    palette.setColor(QPalette.ColorRole.Text, Qt.GlobalColor.white)
    palette.setColor(QPalette.ColorRole.Button, QColor(53, 53, 53))
    palette.setColor(QPalette.ColorRole.ButtonText, Qt.GlobalColor.white)
    palette.setColor(QPalette.ColorRole.BrightText, Qt.GlobalColor.red)
    palette.setColor(QPalette.ColorRole.Link, QColor(42, 130, 218))
    palette.setColor(QPalette.ColorRole.Highlight, QColor(42, 130, 218))
    palette.setColor(QPalette.ColorRole.HighlightedText, Qt.GlobalColor.black)
    app.setPalette(palette)

    window = HDRCalibrationWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
