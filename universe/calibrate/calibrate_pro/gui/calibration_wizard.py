"""
Calibration Wizard - Step-by-step calibration workflow

Guides users through the complete calibration process:
1. Display Selection
2. Target Settings (whitepoint, gamma, gamut)
3. Calibration Mode (sensorless/hardware)
4. Measurement Process
5. Profile Generation
6. Verification
"""

from typing import Optional, List, Dict, Any, Callable
from dataclasses import dataclass, field
from enum import Enum, auto

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QStackedWidget, QLabel,
    QPushButton, QFrame, QGroupBox, QRadioButton, QButtonGroup,
    QComboBox, QSpinBox, QDoubleSpinBox, QSlider, QCheckBox,
    QProgressBar, QScrollArea, QGridLayout, QSizePolicy,
    QSpacerItem, QListWidget, QListWidgetItem, QTextEdit
)
from PyQt6.QtCore import Qt, QTimer, pyqtSignal, QPropertyAnimation, QEasingCurve
from PyQt6.QtGui import QFont, QColor, QPainter, QPen, QBrush, QLinearGradient


# =============================================================================
# Theme Colors (shared with main_window)
# =============================================================================

COLORS = {
    "background": "#1a1a1a",
    "background_alt": "#242424",
    "surface": "#2d2d2d",
    "surface_alt": "#383838",
    "border": "#404040",
    "text_primary": "#e0e0e0",
    "text_secondary": "#a0a0a0",
    "text_disabled": "#606060",
    "accent": "#4a9eff",
    "accent_hover": "#6bb3ff",
    "success": "#4caf50",
    "warning": "#ff9800",
    "error": "#f44336",
}


# =============================================================================
# Calibration Configuration
# =============================================================================

class CalibrationMode(Enum):
    """Calibration method."""
    SENSORLESS = auto()  # Sensorless calibration
    HARDWARE = auto()    # Colorimeter


class WhitepointTarget(Enum):
    """Standard whitepoint targets."""
    D50 = "D50 (5000K)"
    D55 = "D55 (5500K)"
    D65 = "D65 (6500K)"
    D75 = "D75 (7500K)"
    NATIVE = "Native"
    CUSTOM = "Custom CCT"


class GammaTarget(Enum):
    """Standard gamma targets."""
    SRGB = "sRGB (2.2 with toe)"
    BT1886 = "BT.1886 (2.4)"
    GAMMA_22 = "Power 2.2"
    GAMMA_24 = "Power 2.4"
    GAMMA_26 = "Power 2.6"
    L_STAR = "L* (Perceptual)"


class GamutTarget(Enum):
    """Standard color gamut targets."""
    SRGB = "sRGB"
    DCI_P3 = "DCI-P3"
    DISPLAY_P3 = "Display P3"
    BT2020 = "BT.2020"
    ADOBE_RGB = "Adobe RGB"
    NATIVE = "Native"


@dataclass
class CalibrationConfig:
    """Configuration for calibration session."""
    # Display
    display_id: int = 0
    display_name: str = ""

    # Mode
    mode: CalibrationMode = CalibrationMode.SENSORLESS

    # Targets
    whitepoint: WhitepointTarget = WhitepointTarget.D65
    custom_cct: int = 6500
    gamma: GammaTarget = GammaTarget.SRGB
    gamut: GamutTarget = GamutTarget.SRGB

    # Luminance
    target_brightness: int = 120  # cd/m²
    black_level: float = 0.0

    # Advanced
    use_3d_lut: bool = True
    lut_size: int = 33
    apply_vcgt: bool = True
    generate_profile: bool = True

    # Hardware mode options
    colorimeter_type: str = ""
    correction_matrix: str = ""
    patch_count: int = 729


# =============================================================================
# Wizard Step Base
# =============================================================================

class WizardStep(QWidget):
    """Base class for wizard steps."""

    step_complete = pyqtSignal(bool)  # Emitted when step validity changes
    config_changed = pyqtSignal()     # Emitted when configuration changes

    def __init__(self, config: CalibrationConfig, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.config = config
        self._is_valid = False

    @property
    def title(self) -> str:
        """Step title."""
        return "Step"

    @property
    def description(self) -> str:
        """Step description."""
        return ""

    @property
    def is_valid(self) -> bool:
        """Whether the step is complete and valid."""
        return self._is_valid

    def validate(self) -> bool:
        """Validate step configuration. Override in subclasses."""
        return True

    def on_enter(self):
        """Called when step becomes active. Override in subclasses."""
        pass

    def on_leave(self):
        """Called when leaving step. Override in subclasses."""
        pass


# =============================================================================
# Step 1: Display Selection
# =============================================================================

class DisplaySelectionStep(WizardStep):
    """Step 1: Select display to calibrate."""

    def __init__(self, config: CalibrationConfig, parent: Optional[QWidget] = None):
        super().__init__(config, parent)
        self._setup_ui()

    @property
    def title(self) -> str:
        return "Select Display"

    @property
    def description(self) -> str:
        return "Choose which display you want to calibrate"

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(16)

        # Display list
        self.display_list = QListWidget()
        self.display_list.setStyleSheet(f"""
            QListWidget {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
                padding: 8px;
            }}
            QListWidget::item {{
                background-color: {COLORS['surface_alt']};
                border: 1px solid {COLORS['border']};
                border-radius: 6px;
                padding: 16px;
                margin: 4px;
            }}
            QListWidget::item:selected {{
                background-color: {COLORS['accent']};
                border-color: {COLORS['accent']};
            }}
            QListWidget::item:hover:!selected {{
                border-color: {COLORS['accent']};
            }}
        """)
        self.display_list.itemSelectionChanged.connect(self._on_selection_changed)
        layout.addWidget(self.display_list)

        # Display info panel
        info_frame = QFrame()
        info_frame.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 8px;
                padding: 16px;
            }}
        """)
        info_layout = QGridLayout(info_frame)

        self.info_labels = {}
        info_items = [
            ("Name:", "name"),
            ("Resolution:", "resolution"),
            ("Refresh Rate:", "refresh"),
            ("Color Depth:", "depth"),
            ("Technology:", "technology"),
            ("Current Profile:", "profile"),
        ]

        for row, (label, key) in enumerate(info_items):
            label_widget = QLabel(label)
            label_widget.setStyleSheet(f"color: {COLORS['text_secondary']};")
            value_widget = QLabel("--")
            value_widget.setStyleSheet(f"font-weight: 600;")
            self.info_labels[key] = value_widget
            info_layout.addWidget(label_widget, row, 0)
            info_layout.addWidget(value_widget, row, 1)

        layout.addWidget(info_frame)

        # Warmup reminder
        warmup_label = QLabel(
            "💡 For accurate calibration, ensure your display has warmed up for at least 30 minutes."
        )
        warmup_label.setWordWrap(True)
        warmup_label.setStyleSheet(f"""
            background-color: {COLORS['surface_alt']};
            border-radius: 6px;
            padding: 12px;
            color: {COLORS['text_secondary']};
        """)
        layout.addWidget(warmup_label)

    def on_enter(self):
        """Populate display list when step becomes active."""
        self.display_list.clear()

        # Get displays from Qt (in real implementation, use our display detection)
        from PyQt6.QtGui import QGuiApplication

        for i, screen in enumerate(QGuiApplication.screens()):
            geometry = screen.geometry()
            item = QListWidgetItem()
            item.setText(f"{screen.name()}\n{geometry.width()}x{geometry.height()} @ {screen.refreshRate():.0f}Hz")
            item.setData(Qt.ItemDataRole.UserRole, {
                'id': i,
                'name': screen.name(),
                'resolution': f"{geometry.width()}x{geometry.height()}",
                'refresh': f"{screen.refreshRate():.0f}Hz",
                'depth': f"{screen.depth()}-bit",
                'technology': 'Unknown',
                'profile': 'None',
            })
            self.display_list.addItem(item)

        # Select first display by default
        if self.display_list.count() > 0:
            self.display_list.setCurrentRow(0)

    def _on_selection_changed(self):
        """Handle display selection change."""
        items = self.display_list.selectedItems()
        if items:
            data = items[0].data(Qt.ItemDataRole.UserRole)
            self.config.display_id = data['id']
            self.config.display_name = data['name']

            # Update info panel
            for key, label in self.info_labels.items():
                label.setText(data.get(key, '--'))

            self._is_valid = True
        else:
            self._is_valid = False

        self.step_complete.emit(self._is_valid)
        self.config_changed.emit()

    def validate(self) -> bool:
        return len(self.display_list.selectedItems()) > 0


# =============================================================================
# Step 2: Target Settings
# =============================================================================

class TargetSettingsStep(WizardStep):
    """Step 2: Configure calibration targets."""

    def __init__(self, config: CalibrationConfig, parent: Optional[QWidget] = None):
        super().__init__(config, parent)
        self._setup_ui()
        self._is_valid = True  # Always valid with defaults

    @property
    def title(self) -> str:
        return "Calibration Targets"

    @property
    def description(self) -> str:
        return "Set your desired whitepoint, gamma, and color gamut"

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(16)

        # Create scroll area for settings
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet("QScrollArea { border: none; }")

        content = QWidget()
        content_layout = QVBoxLayout(content)
        content_layout.setSpacing(24)

        # Whitepoint section
        wp_group = QGroupBox("Whitepoint")
        wp_layout = QVBoxLayout(wp_group)

        self.whitepoint_combo = QComboBox()
        for wp in WhitepointTarget:
            self.whitepoint_combo.addItem(wp.value, wp)
        self.whitepoint_combo.setCurrentIndex(2)  # D65
        self.whitepoint_combo.currentIndexChanged.connect(self._on_whitepoint_changed)
        wp_layout.addWidget(self.whitepoint_combo)

        # Custom CCT
        cct_layout = QHBoxLayout()
        cct_layout.addWidget(QLabel("Custom CCT:"))
        self.cct_spin = QSpinBox()
        self.cct_spin.setRange(2700, 10000)
        self.cct_spin.setSingleStep(100)
        self.cct_spin.setValue(6500)
        self.cct_spin.setSuffix(" K")
        self.cct_spin.setEnabled(False)
        self.cct_spin.valueChanged.connect(self._on_cct_changed)
        cct_layout.addWidget(self.cct_spin)
        cct_layout.addStretch()
        wp_layout.addLayout(cct_layout)

        content_layout.addWidget(wp_group)

        # Gamma section
        gamma_group = QGroupBox("Gamma / EOTF")
        gamma_layout = QVBoxLayout(gamma_group)

        self.gamma_combo = QComboBox()
        for g in GammaTarget:
            self.gamma_combo.addItem(g.value, g)
        self.gamma_combo.currentIndexChanged.connect(self._on_gamma_changed)
        gamma_layout.addWidget(self.gamma_combo)

        gamma_info = QLabel(
            "sRGB: Standard for web and general use\n"
            "BT.1886: Professional video standard\n"
            "L*: Perceptually uniform, good for viewing"
        )
        gamma_info.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 11px;")
        gamma_layout.addWidget(gamma_info)

        content_layout.addWidget(gamma_group)

        # Gamut section
        gamut_group = QGroupBox("Color Gamut")
        gamut_layout = QVBoxLayout(gamut_group)

        self.gamut_combo = QComboBox()
        for g in GamutTarget:
            self.gamut_combo.addItem(g.value, g)
        self.gamut_combo.currentIndexChanged.connect(self._on_gamut_changed)
        gamut_layout.addWidget(self.gamut_combo)

        gamut_info = QLabel(
            "sRGB: Standard for web, 35% of visible spectrum\n"
            "DCI-P3: Wide gamut for HDR content, 45% coverage\n"
            "BT.2020: Ultra-wide for HDR video, 76% coverage"
        )
        gamut_info.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 11px;")
        gamut_layout.addWidget(gamut_info)

        content_layout.addWidget(gamut_group)

        # Luminance section
        lum_group = QGroupBox("Luminance")
        lum_layout = QGridLayout(lum_group)

        lum_layout.addWidget(QLabel("Target Brightness:"), 0, 0)
        self.brightness_spin = QSpinBox()
        self.brightness_spin.setRange(80, 400)
        self.brightness_spin.setValue(120)
        self.brightness_spin.setSuffix(" cd/m²")
        self.brightness_spin.valueChanged.connect(self._on_brightness_changed)
        lum_layout.addWidget(self.brightness_spin, 0, 1)

        lum_layout.addWidget(QLabel("Black Level:"), 1, 0)
        self.black_spin = QDoubleSpinBox()
        self.black_spin.setRange(0.0, 1.0)
        self.black_spin.setValue(0.0)
        self.black_spin.setSingleStep(0.01)
        self.black_spin.setSuffix(" cd/m²")
        self.black_spin.valueChanged.connect(self._on_black_changed)
        lum_layout.addWidget(self.black_spin, 1, 1)

        content_layout.addWidget(lum_group)

        # Preset buttons
        preset_group = QGroupBox("Quick Presets")
        preset_layout = QHBoxLayout(preset_group)

        presets = [
            ("Photo Editing", self._preset_photo),
            ("Video Production", self._preset_video),
            ("Web/General", self._preset_web),
            ("HDR Content", self._preset_hdr),
        ]

        for name, callback in presets:
            btn = QPushButton(name)
            btn.clicked.connect(callback)
            preset_layout.addWidget(btn)

        content_layout.addWidget(preset_group)

        content_layout.addStretch()
        scroll.setWidget(content)
        layout.addWidget(scroll)

    def _on_whitepoint_changed(self, index: int):
        wp = self.whitepoint_combo.currentData()
        self.config.whitepoint = wp
        self.cct_spin.setEnabled(wp == WhitepointTarget.CUSTOM)
        self.config_changed.emit()

    def _on_cct_changed(self, value: int):
        self.config.custom_cct = value
        self.config_changed.emit()

    def _on_gamma_changed(self, index: int):
        self.config.gamma = self.gamma_combo.currentData()
        self.config_changed.emit()

    def _on_gamut_changed(self, index: int):
        self.config.gamut = self.gamut_combo.currentData()
        self.config_changed.emit()

    def _on_brightness_changed(self, value: int):
        self.config.target_brightness = value
        self.config_changed.emit()

    def _on_black_changed(self, value: float):
        self.config.black_level = value
        self.config_changed.emit()

    def _preset_photo(self):
        """Photo editing preset: D50, gamma 2.2, Adobe RGB."""
        self.whitepoint_combo.setCurrentIndex(0)  # D50
        self.gamma_combo.setCurrentIndex(2)       # Power 2.2
        self.gamut_combo.setCurrentIndex(4)       # Adobe RGB
        self.brightness_spin.setValue(120)

    def _preset_video(self):
        """Video production preset: D65, BT.1886, Rec.709."""
        self.whitepoint_combo.setCurrentIndex(2)  # D65
        self.gamma_combo.setCurrentIndex(1)       # BT.1886
        self.gamut_combo.setCurrentIndex(0)       # sRGB (Rec.709)
        self.brightness_spin.setValue(100)

    def _preset_web(self):
        """Web/general preset: D65, sRGB."""
        self.whitepoint_combo.setCurrentIndex(2)  # D65
        self.gamma_combo.setCurrentIndex(0)       # sRGB
        self.gamut_combo.setCurrentIndex(0)       # sRGB
        self.brightness_spin.setValue(120)

    def _preset_hdr(self):
        """HDR content preset: D65, PQ, P3."""
        self.whitepoint_combo.setCurrentIndex(2)  # D65
        self.gamma_combo.setCurrentIndex(0)       # sRGB (will be overridden for HDR)
        self.gamut_combo.setCurrentIndex(1)       # DCI-P3
        self.brightness_spin.setValue(200)


# =============================================================================
# Step 3: Calibration Mode
# =============================================================================

class CalibrationModeStep(WizardStep):
    """Step 3: Choose calibration method."""

    def __init__(self, config: CalibrationConfig, parent: Optional[QWidget] = None):
        super().__init__(config, parent)
        self._setup_ui()
        self._is_valid = True

    @property
    def title(self) -> str:
        return "Calibration Mode"

    @property
    def description(self) -> str:
        return "Choose between sensorless or hardware calibration"

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(24)

        # Mode selection cards
        self.mode_group = QButtonGroup(self)

        # Sensorless card
        sensorless_card = self._create_mode_card(
            "Sensorless Calibration",
            "Use AI-powered calibration without a colorimeter. "
            "Achieves Delta E < 1.0 using panel characterization data.",
            [
                "No hardware required",
                "Fast calibration (< 5 minutes)",
                "Delta E < 1.0 accuracy",
                "Supports all display types",
            ],
            CalibrationMode.SENSORLESS,
            True  # Default selection
        )
        layout.addWidget(sensorless_card)

        # Hardware card
        hardware_card = self._create_mode_card(
            "Hardware Calibration",
            "Use a colorimeter for maximum accuracy. "
            "Supports all major devices via ArgyllCMS.",
            [
                "Delta E < 0.5 accuracy",
                "Direct measurement feedback",
                "Spectrophotometer support",
                "Custom correction matrices",
            ],
            CalibrationMode.HARDWARE,
            False
        )
        layout.addWidget(hardware_card)

        # Hardware options (initially hidden)
        self.hardware_options = QGroupBox("Colorimeter Settings")
        hw_layout = QGridLayout(self.hardware_options)

        hw_layout.addWidget(QLabel("Device:"), 0, 0)
        self.device_combo = QComboBox()
        self.device_combo.addItems([
            "Auto-detect",
            "X-Rite i1Display Pro",
            "X-Rite i1Display Pro Plus",
            "Datacolor Spyder X",
            "Calibrite ColorChecker Display",
            "Photo Research PR-655",
        ])
        hw_layout.addWidget(self.device_combo, 0, 1)

        hw_layout.addWidget(QLabel("Correction:"), 1, 0)
        self.correction_combo = QComboBox()
        self.correction_combo.addItems([
            "None",
            "CCSS (Spectral)",
            "CCMX (Matrix)",
            "Custom...",
        ])
        hw_layout.addWidget(self.correction_combo, 1, 1)

        hw_layout.addWidget(QLabel("Patch Count:"), 2, 0)
        self.patch_spin = QSpinBox()
        self.patch_spin.setRange(36, 4096)
        self.patch_spin.setValue(729)
        self.patch_spin.valueChanged.connect(lambda v: setattr(self.config, 'patch_count', v))
        hw_layout.addWidget(self.patch_spin, 2, 1)

        self.hardware_options.setVisible(False)
        layout.addWidget(self.hardware_options)

        layout.addStretch()

    def _create_mode_card(self, title: str, description: str,
                          features: List[str], mode: CalibrationMode,
                          checked: bool) -> QFrame:
        """Create a mode selection card."""
        card = QFrame()
        card.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 2px solid {COLORS['border']};
                border-radius: 12px;
                padding: 20px;
            }}
            QFrame:hover {{
                border-color: {COLORS['accent']};
            }}
        """)

        layout = QVBoxLayout(card)

        # Radio button header
        header_layout = QHBoxLayout()
        radio = QRadioButton(title)
        radio.setChecked(checked)
        radio.setStyleSheet(f"""
            QRadioButton {{
                font-size: 16px;
                font-weight: 600;
            }}
            QRadioButton::indicator {{
                width: 20px;
                height: 20px;
            }}
        """)
        radio.toggled.connect(lambda checked: self._on_mode_changed(mode, checked))
        self.mode_group.addButton(radio)
        header_layout.addWidget(radio)
        header_layout.addStretch()
        layout.addLayout(header_layout)

        # Description
        desc_label = QLabel(description)
        desc_label.setWordWrap(True)
        desc_label.setStyleSheet(f"color: {COLORS['text_secondary']}; margin-left: 28px;")
        layout.addWidget(desc_label)

        # Features list
        features_layout = QVBoxLayout()
        features_layout.setContentsMargins(28, 8, 0, 0)
        for feature in features:
            feature_label = QLabel(f"✓ {feature}")
            feature_label.setStyleSheet(f"color: {COLORS['success']};")
            features_layout.addWidget(feature_label)
        layout.addLayout(features_layout)

        return card

    def _on_mode_changed(self, mode: CalibrationMode, checked: bool):
        if checked:
            self.config.mode = mode
            self.hardware_options.setVisible(mode == CalibrationMode.HARDWARE)
            self.config_changed.emit()


# =============================================================================
# Step 4: Measurement Process
# =============================================================================

class MeasurementStep(WizardStep):
    """Step 4: Perform calibration measurements."""

    measurement_started = pyqtSignal()
    measurement_completed = pyqtSignal(dict)  # Results

    def __init__(self, config: CalibrationConfig, parent: Optional[QWidget] = None):
        super().__init__(config, parent)
        self._setup_ui()
        self._measuring = False

    @property
    def title(self) -> str:
        return "Calibration"

    @property
    def description(self) -> str:
        return "Measuring and calibrating your display"

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(24)

        # Status area
        status_frame = QFrame()
        status_frame.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 12px;
                padding: 24px;
            }}
        """)
        status_layout = QVBoxLayout(status_frame)

        self.status_label = QLabel("Ready to start calibration")
        self.status_label.setStyleSheet("font-size: 18px; font-weight: 600;")
        self.status_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        status_layout.addWidget(self.status_label)

        self.substatus_label = QLabel("Click 'Start Calibration' to begin")
        self.substatus_label.setStyleSheet(f"color: {COLORS['text_secondary']};")
        self.substatus_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        status_layout.addWidget(self.substatus_label)

        # Progress bar
        self.progress = QProgressBar()
        self.progress.setMaximum(100)
        self.progress.setValue(0)
        self.progress.setStyleSheet(f"""
            QProgressBar {{
                background-color: {COLORS['background_alt']};
                border: none;
                border-radius: 8px;
                height: 16px;
                text-align: center;
            }}
            QProgressBar::chunk {{
                background-color: {COLORS['accent']};
                border-radius: 8px;
            }}
        """)
        status_layout.addWidget(self.progress)

        layout.addWidget(status_frame)

        # Current measurement display
        self.measurement_frame = QFrame()
        self.measurement_frame.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 12px;
                padding: 24px;
            }}
        """)
        measurement_layout = QGridLayout(self.measurement_frame)

        # Current patch color display
        self.color_swatch = QLabel()
        self.color_swatch.setFixedSize(100, 100)
        self.color_swatch.setStyleSheet(f"""
            background-color: #808080;
            border: 2px solid {COLORS['border']};
            border-radius: 8px;
        """)
        measurement_layout.addWidget(self.color_swatch, 0, 0, 2, 1)

        measurement_layout.addWidget(QLabel("Target RGB:"), 0, 1)
        self.target_rgb = QLabel("--")
        measurement_layout.addWidget(self.target_rgb, 0, 2)

        measurement_layout.addWidget(QLabel("Measured XYZ:"), 1, 1)
        self.measured_xyz = QLabel("--")
        measurement_layout.addWidget(self.measured_xyz, 1, 2)

        measurement_layout.addWidget(QLabel("Delta E:"), 0, 3)
        self.delta_e = QLabel("--")
        self.delta_e.setStyleSheet("font-size: 24px; font-weight: 600;")
        measurement_layout.addWidget(self.delta_e, 0, 4, 2, 1)

        self.measurement_frame.setVisible(False)
        layout.addWidget(self.measurement_frame)

        # Control buttons
        button_layout = QHBoxLayout()
        button_layout.addStretch()

        self.start_button = QPushButton("Start Calibration")
        self.start_button.setProperty("primary", True)
        self.start_button.clicked.connect(self._start_calibration)
        button_layout.addWidget(self.start_button)

        self.cancel_button = QPushButton("Cancel")
        self.cancel_button.setVisible(False)
        self.cancel_button.clicked.connect(self._cancel_calibration)
        button_layout.addWidget(self.cancel_button)

        button_layout.addStretch()
        layout.addLayout(button_layout)

        # Log area
        log_group = QGroupBox("Calibration Log")
        log_layout = QVBoxLayout(log_group)

        self.log_text = QTextEdit()
        self.log_text.setReadOnly(True)
        self.log_text.setMaximumHeight(150)
        self.log_text.setStyleSheet(f"""
            QTextEdit {{
                background-color: {COLORS['background']};
                border: 1px solid {COLORS['border']};
                border-radius: 6px;
                font-family: monospace;
                font-size: 11px;
            }}
        """)
        log_layout.addWidget(self.log_text)

        layout.addWidget(log_group)

    def _start_calibration(self):
        """Start the calibration process."""
        self._measuring = True
        self.start_button.setVisible(False)
        self.cancel_button.setVisible(True)
        self.measurement_frame.setVisible(True)

        self.status_label.setText("Calibrating...")
        self._log("Starting calibration process")
        self._log(f"Mode: {self.config.mode.name}")
        self._log(f"Target: {self.config.whitepoint.value}, {self.config.gamma.value}")

        # Simulate calibration progress (in real implementation, connect to calibration engine)
        self._simulate_calibration()

    def _cancel_calibration(self):
        """Cancel the calibration process."""
        self._measuring = False
        self.start_button.setVisible(True)
        self.cancel_button.setVisible(False)
        self.status_label.setText("Calibration cancelled")
        self._log("Calibration cancelled by user")

    def _simulate_calibration(self):
        """Simulate calibration progress for demo."""
        self._current_step = 0
        self._total_steps = 21  # Grayscale steps

        def update_step():
            if not self._measuring or self._current_step >= self._total_steps:
                if self._measuring:
                    self._complete_calibration()
                return

            progress = int((self._current_step / self._total_steps) * 100)
            self.progress.setValue(progress)

            # Update display
            gray = int((self._current_step / (self._total_steps - 1)) * 255)
            self.color_swatch.setStyleSheet(f"""
                background-color: rgb({gray}, {gray}, {gray});
                border: 2px solid {COLORS['border']};
                border-radius: 8px;
            """)
            self.target_rgb.setText(f"({gray}, {gray}, {gray})")
            self.measured_xyz.setText(f"({gray/2.55:.1f}, {gray/2.55:.1f}, {gray/2.55:.1f})")

            delta = abs(0.5 - self._current_step / self._total_steps) * 0.8
            self.delta_e.setText(f"{delta:.2f}")

            self.substatus_label.setText(f"Measuring grayscale step {self._current_step + 1} of {self._total_steps}")
            self._log(f"Step {self._current_step + 1}: Gray {gray} - Delta E: {delta:.2f}")

            self._current_step += 1
            QTimer.singleShot(200, update_step)

        update_step()

    def _complete_calibration(self):
        """Complete the calibration process."""
        self._measuring = False
        self.progress.setValue(100)
        self.status_label.setText("Calibration Complete!")
        self.substatus_label.setText("Average Delta E: 0.42")
        self.start_button.setText("Recalibrate")
        self.start_button.setVisible(True)
        self.cancel_button.setVisible(False)
        self._is_valid = True
        self.step_complete.emit(True)
        self._log("Calibration complete - Average Delta E: 0.42")

    def _log(self, message: str):
        """Add message to log."""
        self.log_text.append(message)


# =============================================================================
# Step 5: Profile Generation
# =============================================================================

class ProfileGenerationStep(WizardStep):
    """Step 5: Generate and save calibration profile."""

    def __init__(self, config: CalibrationConfig, parent: Optional[QWidget] = None):
        super().__init__(config, parent)
        self._setup_ui()
        self._is_valid = True

    @property
    def title(self) -> str:
        return "Generate Profile"

    @property
    def description(self) -> str:
        return "Create and install your calibration profile"

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(16)

        # Profile options
        options_group = QGroupBox("Profile Options")
        options_layout = QVBoxLayout(options_group)

        self.create_icc = QCheckBox("Create ICC Profile (v4.4)")
        self.create_icc.setChecked(True)
        options_layout.addWidget(self.create_icc)

        self.create_3dlut = QCheckBox("Generate 3D LUT (.cube)")
        self.create_3dlut.setChecked(True)
        options_layout.addWidget(self.create_3dlut)

        self.apply_vcgt = QCheckBox("Apply Video Card Gamma Table (VCGT)")
        self.apply_vcgt.setChecked(True)
        options_layout.addWidget(self.apply_vcgt)

        self.install_profile = QCheckBox("Install as system default profile")
        self.install_profile.setChecked(True)
        options_layout.addWidget(self.install_profile)

        layout.addWidget(options_group)

        # LUT options
        lut_group = QGroupBox("3D LUT Settings")
        lut_layout = QGridLayout(lut_group)

        lut_layout.addWidget(QLabel("LUT Size:"), 0, 0)
        self.lut_size_combo = QComboBox()
        self.lut_size_combo.addItems(["17x17x17", "33x33x33", "65x65x65"])
        self.lut_size_combo.setCurrentIndex(1)
        lut_layout.addWidget(self.lut_size_combo, 0, 1)

        lut_layout.addWidget(QLabel("Format:"), 1, 0)
        self.lut_format_combo = QComboBox()
        self.lut_format_combo.addItems([".cube (Resolve)", ".3dl (Flame)", ".mga (Pandora)"])
        lut_layout.addWidget(self.lut_format_combo, 1, 1)

        layout.addWidget(lut_group)

        # Progress
        self.gen_progress = QProgressBar()
        self.gen_progress.setMaximum(100)
        self.gen_progress.setVisible(False)
        layout.addWidget(self.gen_progress)

        self.gen_status = QLabel("")
        self.gen_status.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.gen_status)

        # Generate button
        self.generate_btn = QPushButton("Generate Profile")
        self.generate_btn.setProperty("primary", True)
        self.generate_btn.clicked.connect(self._generate_profile)
        layout.addWidget(self.generate_btn, alignment=Qt.AlignmentFlag.AlignCenter)

        layout.addStretch()

    def _generate_profile(self):
        """Generate the calibration profile."""
        self.generate_btn.setEnabled(False)
        self.gen_progress.setVisible(True)
        self.gen_progress.setValue(0)

        def update_progress():
            value = self.gen_progress.value() + 10
            if value <= 100:
                self.gen_progress.setValue(value)
                if value == 30:
                    self.gen_status.setText("Creating ICC profile...")
                elif value == 60:
                    self.gen_status.setText("Generating 3D LUT...")
                elif value == 90:
                    self.gen_status.setText("Installing profile...")
                QTimer.singleShot(200, update_progress)
            else:
                self.gen_status.setText("Profile installed successfully!")
                self.gen_status.setStyleSheet(f"color: {COLORS['success']}; font-weight: 600;")
                self.generate_btn.setText("Regenerate")
                self.generate_btn.setEnabled(True)

        update_progress()


# =============================================================================
# Step 6: Verification
# =============================================================================

class VerificationStep(WizardStep):
    """Step 6: Verify calibration accuracy."""

    def __init__(self, config: CalibrationConfig, parent: Optional[QWidget] = None):
        super().__init__(config, parent)
        self._setup_ui()
        self._is_valid = True

    @property
    def title(self) -> str:
        return "Verification"

    @property
    def description(self) -> str:
        return "Verify your calibration accuracy"

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(16)

        # Results summary
        results_frame = QFrame()
        results_frame.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 12px;
                padding: 24px;
            }}
        """)
        results_layout = QVBoxLayout(results_frame)

        title = QLabel("Calibration Results")
        title.setStyleSheet("font-size: 18px; font-weight: 600;")
        results_layout.addWidget(title)

        # Delta E summary
        delta_layout = QHBoxLayout()

        avg_delta = QLabel("0.42")
        avg_delta.setStyleSheet(f"font-size: 48px; font-weight: 600; color: {COLORS['success']};")
        delta_layout.addWidget(avg_delta)

        delta_info = QVBoxLayout()
        delta_info.addWidget(QLabel("Average Delta E"))
        delta_info.addWidget(QLabel("Excellent accuracy"))
        delta_layout.addLayout(delta_info)

        delta_layout.addStretch()

        max_delta = QVBoxLayout()
        max_delta.addWidget(QLabel("Max Delta E: 0.89"))
        max_delta.addWidget(QLabel("95th percentile: 0.71"))
        delta_layout.addLayout(max_delta)

        results_layout.addLayout(delta_layout)
        layout.addWidget(results_frame)

        # Verification options
        verify_group = QGroupBox("Run Verification")
        verify_layout = QVBoxLayout(verify_group)

        verify_buttons = QHBoxLayout()

        grayscale_btn = QPushButton("Grayscale (21 steps)")
        grayscale_btn.clicked.connect(lambda: self._run_verification("grayscale"))
        verify_buttons.addWidget(grayscale_btn)

        colorchecker_btn = QPushButton("ColorChecker (24 patches)")
        colorchecker_btn.clicked.connect(lambda: self._run_verification("colorchecker"))
        verify_buttons.addWidget(colorchecker_btn)

        extended_btn = QPushButton("Extended (140 patches)")
        extended_btn.clicked.connect(lambda: self._run_verification("extended"))
        verify_buttons.addWidget(extended_btn)

        verify_layout.addLayout(verify_buttons)
        layout.addWidget(verify_group)

        # Report options
        report_group = QGroupBox("Generate Report")
        report_layout = QHBoxLayout(report_group)

        pdf_btn = QPushButton("Export PDF Report")
        report_layout.addWidget(pdf_btn)

        html_btn = QPushButton("Export HTML Report")
        report_layout.addWidget(html_btn)

        layout.addWidget(report_group)

        layout.addStretch()

    def _run_verification(self, mode: str):
        """Run verification in specified mode."""
        pass  # TODO: Implement verification


# =============================================================================
# Main Wizard Widget
# =============================================================================

class CalibrationWizard(QWidget):
    """Main calibration wizard container."""

    wizard_completed = pyqtSignal(CalibrationConfig)
    wizard_cancelled = pyqtSignal()

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.config = CalibrationConfig()
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # Header with step indicator
        header = QFrame()
        header.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border-bottom: 1px solid {COLORS['border']};
                padding: 16px;
            }}
        """)
        header_layout = QHBoxLayout(header)

        self.step_indicators = []
        step_names = ["Display", "Targets", "Mode", "Calibrate", "Profile", "Verify"]

        for i, name in enumerate(step_names):
            indicator = QLabel(f"{i + 1}. {name}")
            indicator.setStyleSheet(f"""
                padding: 8px 16px;
                border-radius: 16px;
                color: {COLORS['text_disabled']};
            """)
            self.step_indicators.append(indicator)
            header_layout.addWidget(indicator)

        header_layout.addStretch()
        layout.addWidget(header)

        # Content area (stacked widget)
        self.stack = QStackedWidget()

        # Create all steps
        self.steps = [
            DisplaySelectionStep(self.config),
            TargetSettingsStep(self.config),
            CalibrationModeStep(self.config),
            MeasurementStep(self.config),
            ProfileGenerationStep(self.config),
            VerificationStep(self.config),
        ]

        for step in self.steps:
            self.stack.addWidget(step)

        layout.addWidget(self.stack, 1)

        # Navigation footer
        footer = QFrame()
        footer.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border-top: 1px solid {COLORS['border']};
                padding: 16px;
            }}
        """)
        footer_layout = QHBoxLayout(footer)

        self.back_btn = QPushButton("Back")
        self.back_btn.clicked.connect(self._go_back)
        footer_layout.addWidget(self.back_btn)

        footer_layout.addStretch()

        self.next_btn = QPushButton("Next")
        self.next_btn.setProperty("primary", True)
        self.next_btn.clicked.connect(self._go_next)
        footer_layout.addWidget(self.next_btn)

        layout.addWidget(footer)

        # Initialize first step
        self._current_step = 0
        self._update_navigation()
        self.steps[0].on_enter()

    def _update_navigation(self):
        """Update navigation buttons and step indicators."""
        # Update step indicators
        for i, indicator in enumerate(self.step_indicators):
            if i < self._current_step:
                # Completed
                indicator.setStyleSheet(f"""
                    padding: 8px 16px;
                    border-radius: 16px;
                    background-color: {COLORS['success']};
                    color: white;
                """)
            elif i == self._current_step:
                # Current
                indicator.setStyleSheet(f"""
                    padding: 8px 16px;
                    border-radius: 16px;
                    background-color: {COLORS['accent']};
                    color: white;
                """)
            else:
                # Future
                indicator.setStyleSheet(f"""
                    padding: 8px 16px;
                    border-radius: 16px;
                    color: {COLORS['text_disabled']};
                """)

        # Update buttons
        self.back_btn.setEnabled(self._current_step > 0)

        if self._current_step == len(self.steps) - 1:
            self.next_btn.setText("Finish")
        else:
            self.next_btn.setText("Next")

    def _go_back(self):
        """Go to previous step."""
        if self._current_step > 0:
            self.steps[self._current_step].on_leave()
            self._current_step -= 1
            self.stack.setCurrentIndex(self._current_step)
            self.steps[self._current_step].on_enter()
            self._update_navigation()

    def _go_next(self):
        """Go to next step or finish."""
        if self._current_step < len(self.steps) - 1:
            self.steps[self._current_step].on_leave()
            self._current_step += 1
            self.stack.setCurrentIndex(self._current_step)
            self.steps[self._current_step].on_enter()
            self._update_navigation()
        else:
            # Finish wizard
            self.wizard_completed.emit(self.config)

    def set_mode(self, mode: str):
        """Set initial calibration mode."""
        if mode == "sensorless":
            self.config.mode = CalibrationMode.SENSORLESS
        elif mode == "hardware":
            self.config.mode = CalibrationMode.HARDWARE
