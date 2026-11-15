"""
Real-time Calibration Test Application

Uses Windows Gamma Ramp API for reliable display control.
"""

import sys
import ctypes
from ctypes import wintypes, Structure, POINTER, byref
from typing import Optional, List, Tuple
import time

from PyQt6.QtWidgets import (
    QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QGridLayout, QLabel, QSlider, QPushButton, QGroupBox, QFrame,
    QComboBox, QProgressBar, QSplitter, QTabWidget
)
from PyQt6.QtCore import Qt, QTimer, pyqtSignal, QThread
from PyQt6.QtGui import QColor, QPainter, QFont, QPen, QBrush, QLinearGradient


# ============================================================================
# Windows Display & Gamma Controller
# ============================================================================

class DISPLAY_DEVICE(Structure):
    _fields_ = [
        ('cb', wintypes.DWORD),
        ('DeviceName', wintypes.WCHAR * 32),
        ('DeviceString', wintypes.WCHAR * 128),
        ('StateFlags', wintypes.DWORD),
        ('DeviceID', wintypes.WCHAR * 128),
        ('DeviceKey', wintypes.WCHAR * 128),
    ]


class GAMMA_RAMP(Structure):
    _fields_ = [
        ('Red', ctypes.c_ushort * 256),
        ('Green', ctypes.c_ushort * 256),
        ('Blue', ctypes.c_ushort * 256),
    ]


class GammaController:
    """Controls display gamma ramps via Windows GDI API."""

    DISPLAY_DEVICE_ACTIVE = 0x00000001
    DISPLAY_DEVICE_PRIMARY = 0x00000004

    def __init__(self):
        self.gdi32 = ctypes.windll.gdi32
        self.user32 = ctypes.windll.user32
        self.displays = []
        self.original_ramps = {}
        self._enumerate_displays()

    def _enumerate_displays(self):
        """Enumerate active display devices."""
        self.displays = []
        dd = DISPLAY_DEVICE()
        dd.cb = ctypes.sizeof(DISPLAY_DEVICE)

        i = 0
        while self.user32.EnumDisplayDevicesW(None, i, byref(dd), 0):
            if dd.StateFlags & self.DISPLAY_DEVICE_ACTIVE:
                is_primary = bool(dd.StateFlags & self.DISPLAY_DEVICE_PRIMARY)
                self.displays.append({
                    'name': dd.DeviceName,
                    'string': dd.DeviceString,
                    'primary': is_primary,
                    'index': len(self.displays)
                })
                # Store original gamma ramp
                self._store_original_ramp(dd.DeviceName)
            i += 1

    def _store_original_ramp(self, display_name: str):
        """Store the original gamma ramp for a display."""
        hdc = self.gdi32.CreateDCW(display_name, None, None, None)
        if hdc:
            ramp = GAMMA_RAMP()
            if self.gdi32.GetDeviceGammaRamp(hdc, byref(ramp)):
                # Deep copy the ramp values
                stored = GAMMA_RAMP()
                for i in range(256):
                    stored.Red[i] = ramp.Red[i]
                    stored.Green[i] = ramp.Green[i]
                    stored.Blue[i] = ramp.Blue[i]
                self.original_ramps[display_name] = stored
            self.gdi32.DeleteDC(hdc)

    def get_gamma_ramp(self, display_idx: int) -> Optional[GAMMA_RAMP]:
        """Get current gamma ramp for a display."""
        if display_idx >= len(self.displays):
            return None

        display_name = self.displays[display_idx]['name']
        hdc = self.gdi32.CreateDCW(display_name, None, None, None)
        if not hdc:
            return None

        ramp = GAMMA_RAMP()
        result = self.gdi32.GetDeviceGammaRamp(hdc, byref(ramp))
        self.gdi32.DeleteDC(hdc)

        return ramp if result else None

    def set_gamma_ramp(self, display_idx: int, ramp: GAMMA_RAMP) -> bool:
        """Set gamma ramp for a display."""
        if display_idx >= len(self.displays):
            return False

        display_name = self.displays[display_idx]['name']
        hdc = self.gdi32.CreateDCW(display_name, None, None, None)
        if not hdc:
            return False

        result = self.gdi32.SetDeviceGammaRamp(hdc, byref(ramp))
        self.gdi32.DeleteDC(hdc)
        return bool(result)

    def create_calibration_ramp(self, brightness: int = 100, contrast: int = 100,
                                  r_gain: int = 100, g_gain: int = 100, b_gain: int = 100,
                                  r_offset: int = 0, g_offset: int = 0, b_offset: int = 0,
                                  gamma: float = 2.2) -> GAMMA_RAMP:
        """Create a calibrated gamma ramp with all adjustments."""
        ramp = GAMMA_RAMP()

        # Normalize values
        brightness_factor = brightness / 100.0
        contrast_factor = contrast / 100.0
        gains = [r_gain / 100.0, g_gain / 100.0, b_gain / 100.0]
        offsets = [r_offset / 100.0, g_offset / 100.0, b_offset / 100.0]

        channels = [ramp.Red, ramp.Green, ramp.Blue]

        for ch_idx, channel in enumerate(channels):
            gain = gains[ch_idx]
            offset = offsets[ch_idx]

            for i in range(256):
                # Normalize input to 0-1
                x = i / 255.0

                # Apply gamma
                x = pow(x, 1.0 / gamma)

                # Apply contrast (pivot at 0.5)
                x = (x - 0.5) * contrast_factor + 0.5

                # Apply brightness
                x = x * brightness_factor

                # Apply per-channel gain
                x = x * gain

                # Apply per-channel offset
                x = x + offset * 0.5  # Offset scaled to reasonable range

                # Clamp to 0-1
                x = max(0.0, min(1.0, x))

                # Convert to 16-bit value
                channel[i] = int(x * 65535)

        return ramp

    def apply_calibration(self, display_idx: int, brightness: int = 100, contrast: int = 100,
                          r_gain: int = 100, g_gain: int = 100, b_gain: int = 100,
                          r_offset: int = 0, g_offset: int = 0, b_offset: int = 0,
                          gamma: float = 2.2) -> bool:
        """Apply calibration settings to a display."""
        ramp = self.create_calibration_ramp(
            brightness, contrast, r_gain, g_gain, b_gain,
            r_offset, g_offset, b_offset, gamma
        )
        return self.set_gamma_ramp(display_idx, ramp)

    def reset_display(self, display_idx: int) -> bool:
        """Reset display to original gamma ramp."""
        if display_idx >= len(self.displays):
            return False

        display_name = self.displays[display_idx]['name']
        if display_name in self.original_ramps:
            return self.set_gamma_ramp(display_idx, self.original_ramps[display_name])

        # If no original stored, create linear ramp
        ramp = self.create_calibration_ramp(100, 100, 100, 100, 100, 0, 0, 0, 2.2)
        return self.set_gamma_ramp(display_idx, ramp)


# ============================================================================
# Color Widgets
# ============================================================================

class ColorSwatch(QFrame):
    """A colored rectangle widget."""

    def __init__(self, color: QColor, label: str = "", parent=None):
        super().__init__(parent)
        self.color = color
        self.label = label
        self.setMinimumSize(80, 80)
        self.setFrameStyle(QFrame.Shape.Box | QFrame.Shadow.Plain)

    def set_color(self, color: QColor):
        self.color = color
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.fillRect(self.rect(), self.color)

        if self.label:
            painter.setPen(Qt.GlobalColor.white if self.color.lightness() < 128 else Qt.GlobalColor.black)
            painter.setFont(QFont("Segoe UI", 10, QFont.Weight.Bold))
            painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter, self.label)

        painter.end()


class GrayRamp(QFrame):
    """Grayscale ramp from black to white."""

    def __init__(self, steps: int = 21, parent=None):
        super().__init__(parent)
        self.steps = steps
        self.gamma = 2.2
        self.setMinimumHeight(60)
        self.setFrameStyle(QFrame.Shape.Box | QFrame.Shadow.Plain)

    def set_gamma(self, gamma: float):
        self.gamma = gamma
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        width = self.width()
        height = self.height()
        step_width = width / self.steps

        for i in range(self.steps):
            linear = i / (self.steps - 1)
            value = int(255 * (linear ** (1/self.gamma)))
            color = QColor(value, value, value)

            x = int(i * step_width)
            w = int((i + 1) * step_width) - x
            painter.fillRect(x, 0, w, height, color)

        painter.end()


class ColorGradient(QFrame):
    """RGB gradient bar."""

    def __init__(self, channel: str = 'R', parent=None):
        super().__init__(parent)
        self.channel = channel
        self.gain = 100
        self.setMinimumHeight(30)
        self.setFrameStyle(QFrame.Shape.Box | QFrame.Shadow.Plain)

    def set_gain(self, gain: int):
        self.gain = gain
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        width = self.width()
        height = self.height()

        gradient = QLinearGradient(0, 0, width, 0)

        gain_factor = self.gain / 100.0

        if self.channel == 'R':
            gradient.setColorAt(0, QColor(0, 0, 0))
            gradient.setColorAt(1, QColor(int(255 * gain_factor), 0, 0))
        elif self.channel == 'G':
            gradient.setColorAt(0, QColor(0, 0, 0))
            gradient.setColorAt(1, QColor(0, int(255 * gain_factor), 0))
        elif self.channel == 'B':
            gradient.setColorAt(0, QColor(0, 0, 0))
            gradient.setColorAt(1, QColor(0, 0, int(255 * gain_factor)))

        painter.fillRect(self.rect(), gradient)
        painter.end()


class WhitePointIndicator(QFrame):
    """Shows current white point based on RGB gains."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.r_gain = 100
        self.g_gain = 100
        self.b_gain = 100
        self.setMinimumSize(150, 150)
        self.setFrameStyle(QFrame.Shape.Box | QFrame.Shadow.Plain)

    def set_gains(self, r: int, g: int, b: int):
        self.r_gain = r
        self.g_gain = g
        self.b_gain = b
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)

        r = int(255 * self.r_gain / 100)
        g = int(255 * self.g_gain / 100)
        b = int(255 * self.b_gain / 100)

        painter.fillRect(self.rect(), QColor(r, g, b))

        text_color = Qt.GlobalColor.black if (r + g + b) / 3 > 128 else Qt.GlobalColor.white
        painter.setPen(text_color)
        painter.setFont(QFont("Segoe UI", 9))
        painter.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter,
                        f"WHITE\nR:{self.r_gain} G:{self.g_gain} B:{self.b_gain}")

        painter.end()


class ColorChecker(QFrame):
    """Mini ColorChecker-style grid."""

    COLORS = [
        (115, 82, 68), (194, 150, 130), (98, 122, 157), (87, 108, 67),
        (133, 128, 177), (103, 189, 170),
        (214, 126, 44), (80, 91, 166), (193, 90, 99), (94, 60, 108),
        (157, 188, 64), (224, 163, 46),
        (56, 61, 150), (70, 148, 73), (175, 54, 60), (231, 199, 31),
        (187, 86, 149), (8, 133, 161),
        (243, 243, 242), (200, 200, 200), (160, 160, 160), (122, 122, 121),
        (85, 85, 85), (52, 52, 52),
    ]

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumSize(240, 160)
        self.setFrameStyle(QFrame.Shape.Box | QFrame.Shadow.Plain)

    def paintEvent(self, event):
        painter = QPainter(self)

        cols = 6
        rows = 4
        cell_w = self.width() / cols
        cell_h = self.height() / rows

        for i, (r, g, b) in enumerate(self.COLORS):
            row = i // cols
            col = i % cols

            x = int(col * cell_w)
            y = int(row * cell_h)
            w = int((col + 1) * cell_w) - x
            h = int((row + 1) * cell_h) - y

            painter.fillRect(x, y, w, h, QColor(r, g, b))

        painter.end()


# ============================================================================
# Main Application Window
# ============================================================================

class CalibrationTestWindow(QMainWindow):
    """Main calibration test window using Windows Gamma Ramp API."""

    def __init__(self):
        super().__init__()
        self.setWindowTitle("CalibratePro - Real-Time Calibration Test")
        self.setMinimumSize(1000, 700)
        self.setStyleSheet("""
            QMainWindow { background-color: #1e1e1e; }
            QLabel { color: #e0e0e0; }
            QGroupBox {
                color: #e0e0e0;
                border: 1px solid #444;
                border-radius: 4px;
                margin-top: 10px;
                padding-top: 10px;
            }
            QGroupBox::title {
                subcontrol-origin: margin;
                left: 10px;
                padding: 0 5px;
            }
            QSlider::groove:horizontal {
                height: 8px;
                background: #333;
                border-radius: 4px;
            }
            QSlider::handle:horizontal {
                background: #3d5a80;
                width: 18px;
                margin: -5px 0;
                border-radius: 9px;
            }
            QSlider::handle:horizontal:hover {
                background: #4a6fa5;
            }
            QPushButton {
                background-color: #3d5a80;
                color: white;
                border: none;
                padding: 8px 16px;
                border-radius: 4px;
                font-weight: bold;
            }
            QPushButton:hover { background-color: #4a6fa5; }
            QPushButton:pressed { background-color: #2c4a6e; }
            QComboBox {
                background-color: #333;
                color: #e0e0e0;
                border: 1px solid #444;
                padding: 5px;
                border-radius: 4px;
            }
        """)

        self.gamma_ctrl = GammaController()
        self.current_display = 0

        # Current calibration values
        self.brightness = 100
        self.contrast = 100
        self.r_gain = 100
        self.g_gain = 100
        self.b_gain = 100
        self.r_offset = 0
        self.g_offset = 0
        self.b_offset = 0
        self.gamma = 2.2

        self.setup_ui()

    def setup_ui(self):
        central = QWidget()
        self.setCentralWidget(central)
        layout = QHBoxLayout(central)
        layout.setSpacing(15)
        layout.setContentsMargins(15, 15, 15, 15)

        # Left panel - Controls
        left_panel = QVBoxLayout()

        # Monitor selection
        monitor_group = QGroupBox("Display Selection")
        monitor_layout = QVBoxLayout(monitor_group)

        self.monitor_combo = QComboBox()
        for disp in self.gamma_ctrl.displays:
            primary = " (Primary)" if disp['primary'] else ""
            self.monitor_combo.addItem(f"{disp['name']} - {disp['string']}{primary}")
        self.monitor_combo.currentIndexChanged.connect(self.on_display_changed)
        monitor_layout.addWidget(self.monitor_combo)

        self.status_label = QLabel("Using Windows Gamma Ramp API")
        self.status_label.setStyleSheet("color: #4CAF50;")
        monitor_layout.addWidget(self.status_label)

        left_panel.addWidget(monitor_group)

        # Basic Controls
        basic_group = QGroupBox("Basic Controls")
        basic_layout = QGridLayout(basic_group)

        # Brightness
        basic_layout.addWidget(QLabel("Brightness:"), 0, 0)
        self.brightness_slider = QSlider(Qt.Orientation.Horizontal)
        self.brightness_slider.setRange(10, 100)
        self.brightness_slider.setValue(100)
        self.brightness_slider.valueChanged.connect(self.on_brightness_changed)
        basic_layout.addWidget(self.brightness_slider, 0, 1)
        self.brightness_label = QLabel("100%")
        basic_layout.addWidget(self.brightness_label, 0, 2)

        # Contrast
        basic_layout.addWidget(QLabel("Contrast:"), 1, 0)
        self.contrast_slider = QSlider(Qt.Orientation.Horizontal)
        self.contrast_slider.setRange(50, 150)
        self.contrast_slider.setValue(100)
        self.contrast_slider.valueChanged.connect(self.on_contrast_changed)
        basic_layout.addWidget(self.contrast_slider, 1, 1)
        self.contrast_label = QLabel("100%")
        basic_layout.addWidget(self.contrast_label, 1, 2)

        # Gamma
        basic_layout.addWidget(QLabel("Gamma:"), 2, 0)
        self.gamma_slider = QSlider(Qt.Orientation.Horizontal)
        self.gamma_slider.setRange(10, 30)  # 1.0 to 3.0
        self.gamma_slider.setValue(22)  # 2.2
        self.gamma_slider.valueChanged.connect(self.on_gamma_changed)
        basic_layout.addWidget(self.gamma_slider, 2, 1)
        self.gamma_label = QLabel("2.2")
        basic_layout.addWidget(self.gamma_label, 2, 2)

        left_panel.addWidget(basic_group)

        # RGB Gain Controls
        gain_group = QGroupBox("RGB Gain (White Balance)")
        gain_layout = QGridLayout(gain_group)

        # Red Gain
        gain_layout.addWidget(QLabel("Red:"), 0, 0)
        self.red_slider = QSlider(Qt.Orientation.Horizontal)
        self.red_slider.setRange(50, 100)
        self.red_slider.setValue(100)
        self.red_slider.valueChanged.connect(self.on_red_gain_changed)
        gain_layout.addWidget(self.red_slider, 0, 1)
        self.red_label = QLabel("100%")
        gain_layout.addWidget(self.red_label, 0, 2)

        # Green Gain
        gain_layout.addWidget(QLabel("Green:"), 1, 0)
        self.green_slider = QSlider(Qt.Orientation.Horizontal)
        self.green_slider.setRange(50, 100)
        self.green_slider.setValue(100)
        self.green_slider.valueChanged.connect(self.on_green_gain_changed)
        gain_layout.addWidget(self.green_slider, 1, 1)
        self.green_label = QLabel("100%")
        gain_layout.addWidget(self.green_label, 1, 2)

        # Blue Gain
        gain_layout.addWidget(QLabel("Blue:"), 2, 0)
        self.blue_slider = QSlider(Qt.Orientation.Horizontal)
        self.blue_slider.setRange(50, 100)
        self.blue_slider.setValue(100)
        self.blue_slider.valueChanged.connect(self.on_blue_gain_changed)
        gain_layout.addWidget(self.blue_slider, 2, 1)
        self.blue_label = QLabel("100%")
        gain_layout.addWidget(self.blue_label, 2, 2)

        left_panel.addWidget(gain_group)

        # RGB Offset Controls
        offset_group = QGroupBox("RGB Offset (Black Level)")
        offset_layout = QGridLayout(offset_group)

        # Red Offset
        offset_layout.addWidget(QLabel("Red:"), 0, 0)
        self.red_offset_slider = QSlider(Qt.Orientation.Horizontal)
        self.red_offset_slider.setRange(-20, 20)
        self.red_offset_slider.setValue(0)
        self.red_offset_slider.valueChanged.connect(self.on_red_offset_changed)
        offset_layout.addWidget(self.red_offset_slider, 0, 1)
        self.red_offset_label = QLabel("0")
        offset_layout.addWidget(self.red_offset_label, 0, 2)

        # Green Offset
        offset_layout.addWidget(QLabel("Green:"), 1, 0)
        self.green_offset_slider = QSlider(Qt.Orientation.Horizontal)
        self.green_offset_slider.setRange(-20, 20)
        self.green_offset_slider.setValue(0)
        self.green_offset_slider.valueChanged.connect(self.on_green_offset_changed)
        offset_layout.addWidget(self.green_offset_slider, 1, 1)
        self.green_offset_label = QLabel("0")
        offset_layout.addWidget(self.green_offset_label, 1, 2)

        # Blue Offset
        offset_layout.addWidget(QLabel("Blue:"), 2, 0)
        self.blue_offset_slider = QSlider(Qt.Orientation.Horizontal)
        self.blue_offset_slider.setRange(-20, 20)
        self.blue_offset_slider.setValue(0)
        self.blue_offset_slider.valueChanged.connect(self.on_blue_offset_changed)
        offset_layout.addWidget(self.blue_offset_slider, 2, 1)
        self.blue_offset_label = QLabel("0")
        offset_layout.addWidget(self.blue_offset_label, 2, 2)

        left_panel.addWidget(offset_group)

        # Preset buttons
        preset_group = QGroupBox("Quick Presets")
        preset_layout = QVBoxLayout(preset_group)

        # White point presets
        wp_label = QLabel("White Point:")
        wp_label.setStyleSheet("font-weight: bold;")
        preset_layout.addWidget(wp_label)

        preset_btn_layout1 = QHBoxLayout()

        self.preset_d65_btn = QPushButton("D65 (6500K)")
        self.preset_d65_btn.clicked.connect(lambda: self.apply_preset('D65'))
        preset_btn_layout1.addWidget(self.preset_d65_btn)

        self.preset_d55_btn = QPushButton("D55 (5500K)")
        self.preset_d55_btn.clicked.connect(lambda: self.apply_preset('D55'))
        preset_btn_layout1.addWidget(self.preset_d55_btn)

        self.preset_d50_btn = QPushButton("D50 (5000K)")
        self.preset_d50_btn.clicked.connect(lambda: self.apply_preset('D50'))
        preset_btn_layout1.addWidget(self.preset_d50_btn)

        preset_layout.addLayout(preset_btn_layout1)

        # Gamma presets
        gamma_label = QLabel("Gamma:")
        gamma_label.setStyleSheet("font-weight: bold;")
        preset_layout.addWidget(gamma_label)

        preset_btn_layout2 = QHBoxLayout()

        self.preset_srgb_btn = QPushButton("sRGB (2.2)")
        self.preset_srgb_btn.clicked.connect(lambda: self.apply_preset('sRGB'))
        preset_btn_layout2.addWidget(self.preset_srgb_btn)

        self.preset_bt1886_btn = QPushButton("BT.1886 (2.4)")
        self.preset_bt1886_btn.clicked.connect(lambda: self.apply_preset('BT1886'))
        preset_btn_layout2.addWidget(self.preset_bt1886_btn)

        self.preset_linear_btn = QPushButton("Linear (1.0)")
        self.preset_linear_btn.clicked.connect(lambda: self.apply_preset('Linear'))
        preset_btn_layout2.addWidget(self.preset_linear_btn)

        preset_layout.addLayout(preset_btn_layout2)

        left_panel.addWidget(preset_group)

        # Reset button
        self.reset_btn = QPushButton("Reset to Default")
        self.reset_btn.clicked.connect(self.reset_calibration)
        self.reset_btn.setStyleSheet("""
            QPushButton {
                background-color: #c62828;
                padding: 12px;
            }
            QPushButton:hover { background-color: #d32f2f; }
        """)
        left_panel.addWidget(self.reset_btn)

        left_panel.addStretch()

        layout.addLayout(left_panel, 1)

        # Right panel - Color Charts
        right_panel = QVBoxLayout()

        # Primary colors
        primaries_group = QGroupBox("Primary Colors")
        primaries_layout = QHBoxLayout(primaries_group)

        self.red_swatch = ColorSwatch(QColor(255, 0, 0), "RED")
        self.green_swatch = ColorSwatch(QColor(0, 255, 0), "GREEN")
        self.blue_swatch = ColorSwatch(QColor(0, 0, 255), "BLUE")
        self.white_swatch = ColorSwatch(QColor(255, 255, 255), "WHITE")
        self.black_swatch = ColorSwatch(QColor(0, 0, 0), "BLACK")

        primaries_layout.addWidget(self.red_swatch)
        primaries_layout.addWidget(self.green_swatch)
        primaries_layout.addWidget(self.blue_swatch)
        primaries_layout.addWidget(self.white_swatch)
        primaries_layout.addWidget(self.black_swatch)

        right_panel.addWidget(primaries_group)

        # RGB Gradients
        gradients_group = QGroupBox("RGB Channel Gradients")
        gradients_layout = QVBoxLayout(gradients_group)

        self.red_gradient = ColorGradient('R')
        self.green_gradient = ColorGradient('G')
        self.blue_gradient = ColorGradient('B')

        gradients_layout.addWidget(QLabel("Red Channel"))
        gradients_layout.addWidget(self.red_gradient)
        gradients_layout.addWidget(QLabel("Green Channel"))
        gradients_layout.addWidget(self.green_gradient)
        gradients_layout.addWidget(QLabel("Blue Channel"))
        gradients_layout.addWidget(self.blue_gradient)

        right_panel.addWidget(gradients_group)

        # Grayscale ramp
        gray_group = QGroupBox("Grayscale Ramp (21 Steps)")
        gray_layout = QVBoxLayout(gray_group)
        self.gray_ramp = GrayRamp(21)
        gray_layout.addWidget(self.gray_ramp)
        right_panel.addWidget(gray_group)

        # Bottom row
        bottom_layout = QHBoxLayout()

        # White point indicator
        wp_group = QGroupBox("Current White Point")
        wp_layout = QVBoxLayout(wp_group)
        self.white_point = WhitePointIndicator()
        wp_layout.addWidget(self.white_point)
        bottom_layout.addWidget(wp_group)

        # ColorChecker
        cc_group = QGroupBox("ColorChecker Reference")
        cc_layout = QVBoxLayout(cc_group)
        self.color_checker = ColorChecker()
        cc_layout.addWidget(self.color_checker)
        bottom_layout.addWidget(cc_group)

        right_panel.addLayout(bottom_layout)

        layout.addLayout(right_panel, 2)

    def apply_calibration(self):
        """Apply current calibration settings to the display."""
        result = self.gamma_ctrl.apply_calibration(
            self.current_display,
            self.brightness, self.contrast,
            self.r_gain, self.g_gain, self.b_gain,
            self.r_offset, self.g_offset, self.b_offset,
            self.gamma
        )

        if result:
            self.status_label.setText("Calibration applied successfully")
            self.status_label.setStyleSheet("color: #4CAF50;")
        else:
            self.status_label.setText("Failed to apply calibration")
            self.status_label.setStyleSheet("color: #f44336;")

        # Update visual indicators
        self.update_color_displays()

    def update_color_displays(self):
        """Update all color displays based on current gains."""
        self.red_gradient.set_gain(self.r_gain)
        self.green_gradient.set_gain(self.g_gain)
        self.blue_gradient.set_gain(self.b_gain)
        self.white_point.set_gains(self.r_gain, self.g_gain, self.b_gain)
        self.gray_ramp.set_gamma(self.gamma)

    def on_display_changed(self, index: int):
        """Handle display selection change."""
        self.current_display = index
        self.reset_calibration()

    def on_brightness_changed(self, value: int):
        self.brightness = value
        self.brightness_label.setText(f"{value}%")
        self.apply_calibration()

    def on_contrast_changed(self, value: int):
        self.contrast = value
        self.contrast_label.setText(f"{value}%")
        self.apply_calibration()

    def on_gamma_changed(self, value: int):
        self.gamma = value / 10.0
        self.gamma_label.setText(f"{self.gamma:.1f}")
        self.apply_calibration()

    def on_red_gain_changed(self, value: int):
        self.r_gain = value
        self.red_label.setText(f"{value}%")
        self.apply_calibration()

    def on_green_gain_changed(self, value: int):
        self.g_gain = value
        self.green_label.setText(f"{value}%")
        self.apply_calibration()

    def on_blue_gain_changed(self, value: int):
        self.b_gain = value
        self.blue_label.setText(f"{value}%")
        self.apply_calibration()

    def on_red_offset_changed(self, value: int):
        self.r_offset = value
        self.red_offset_label.setText(str(value))
        self.apply_calibration()

    def on_green_offset_changed(self, value: int):
        self.g_offset = value
        self.green_offset_label.setText(str(value))
        self.apply_calibration()

    def on_blue_offset_changed(self, value: int):
        self.b_offset = value
        self.blue_offset_label.setText(str(value))
        self.apply_calibration()

    def apply_preset(self, preset_name: str):
        """Apply a calibration preset."""
        presets = {
            'D65': {'r': 100, 'g': 100, 'b': 100, 'gamma': 2.2},
            'D55': {'r': 100, 'g': 98, 'b': 90, 'gamma': 2.2},
            'D50': {'r': 100, 'g': 96, 'b': 82, 'gamma': 2.2},
            'sRGB': {'r': 100, 'g': 100, 'b': 100, 'gamma': 2.2},
            'BT1886': {'r': 100, 'g': 100, 'b': 100, 'gamma': 2.4},
            'Linear': {'r': 100, 'g': 100, 'b': 100, 'gamma': 1.0},
        }

        if preset_name not in presets:
            return

        p = presets[preset_name]

        # Block signals during update
        self.red_slider.blockSignals(True)
        self.green_slider.blockSignals(True)
        self.blue_slider.blockSignals(True)
        self.gamma_slider.blockSignals(True)

        self.r_gain = p['r']
        self.g_gain = p['g']
        self.b_gain = p['b']
        self.gamma = p['gamma']

        self.red_slider.setValue(p['r'])
        self.green_slider.setValue(p['g'])
        self.blue_slider.setValue(p['b'])
        self.gamma_slider.setValue(int(p['gamma'] * 10))

        self.red_label.setText(f"{p['r']}%")
        self.green_label.setText(f"{p['g']}%")
        self.blue_label.setText(f"{p['b']}%")
        self.gamma_label.setText(f"{p['gamma']:.1f}")

        self.red_slider.blockSignals(False)
        self.green_slider.blockSignals(False)
        self.blue_slider.blockSignals(False)
        self.gamma_slider.blockSignals(False)

        self.apply_calibration()

    def reset_calibration(self):
        """Reset to default values."""
        self.brightness = 100
        self.contrast = 100
        self.r_gain = 100
        self.g_gain = 100
        self.b_gain = 100
        self.r_offset = 0
        self.g_offset = 0
        self.b_offset = 0
        self.gamma = 2.2

        # Update all sliders
        self.brightness_slider.setValue(100)
        self.contrast_slider.setValue(100)
        self.gamma_slider.setValue(22)
        self.red_slider.setValue(100)
        self.green_slider.setValue(100)
        self.blue_slider.setValue(100)
        self.red_offset_slider.setValue(0)
        self.green_offset_slider.setValue(0)
        self.blue_offset_slider.setValue(0)

        # Reset display to original
        self.gamma_ctrl.reset_display(self.current_display)
        self.status_label.setText("Reset to default")
        self.status_label.setStyleSheet("color: #4CAF50;")

    def closeEvent(self, event):
        """Restore original gamma when closing."""
        for i in range(len(self.gamma_ctrl.displays)):
            self.gamma_ctrl.reset_display(i)
        event.accept()


def main():
    app = QApplication(sys.argv)
    app.setStyle('Fusion')

    window = CalibrationTestWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == '__main__':
    main()
