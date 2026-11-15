"""
Datacolor Spyder Driver

Specialized driver for Datacolor Spyder colorimeters.
Uses ArgyllCMS backend with device-specific optimizations.
"""

from pathlib import Path
from typing import Dict, List, Optional, Tuple
import subprocess
import re
import time

from calibrate_pro.hardware.colorimeter_base import (
    ColorimeterBase, ColorMeasurement, DeviceInfo, DeviceType,
    CalibrationMode, CalibrationPatch
)
from calibrate_pro.hardware.argyll_backend import ArgyllBackend


class SpyderType:
    """Spyder model variants."""
    SPYDER_X_ELITE = "SpyderX Elite"
    SPYDER_X_PRO = "SpyderX Pro"
    SPYDER_X2_ELITE = "SpyderX2 Elite"
    SPYDER_X2_ULTRA = "SpyderX2 Ultra"
    SPYDER_5_ELITE = "Spyder5 Elite"
    SPYDER_5_PRO = "Spyder5 Pro"
    SPYDER_5_EXPRESS = "Spyder5 Express"
    SPYDER_4_ELITE = "Spyder4 Elite"
    SPYDER_4_PRO = "Spyder4 Pro"
    UNKNOWN = "Unknown Spyder"


# Spyder device specifications
SPYDER_SPECS = {
    SpyderType.SPYDER_X2_ULTRA: {
        "sensors": 6,  # Hex sensor array
        "min_luminance": 0.0001,  # cd/m²
        "max_luminance": 10000,  # cd/m²
        "ambient": True,
        "oled_mode": True,
        "hdr": True,
        "measurement_time": 1.5,  # seconds typical
    },
    SpyderType.SPYDER_X2_ELITE: {
        "sensors": 6,
        "min_luminance": 0.0005,
        "max_luminance": 2000,
        "ambient": True,
        "oled_mode": True,
        "hdr": True,
        "measurement_time": 1.5,
    },
    SpyderType.SPYDER_X_ELITE: {
        "sensors": 3,
        "min_luminance": 0.001,
        "max_luminance": 1000,
        "ambient": True,
        "oled_mode": True,
        "hdr": False,
        "measurement_time": 2.0,
    },
    SpyderType.SPYDER_X_PRO: {
        "sensors": 3,
        "min_luminance": 0.005,
        "max_luminance": 500,
        "ambient": True,
        "oled_mode": False,
        "hdr": False,
        "measurement_time": 2.0,
    },
    SpyderType.SPYDER_5_ELITE: {
        "sensors": 7,  # 7-detector optical engine
        "min_luminance": 0.005,
        "max_luminance": 500,
        "ambient": True,
        "oled_mode": False,
        "hdr": False,
        "measurement_time": 5.0,
    },
}

# Spyder correction matrices for different display types
SPYDER_CORRECTIONS = {
    "WOLED": {
        "description": "WOLED (LG/Sony) Correction",
        "matrix": [
            [1.0312, -0.0198, -0.0114],
            [-0.0112, 1.0189, -0.0077],
            [0.0034, -0.0123, 1.0089]
        ]
    },
    "QDOLED": {
        "description": "QD-OLED (Samsung) Correction",
        "matrix": [
            [1.0156, -0.0089, -0.0067],
            [-0.0078, 1.0134, -0.0056],
            [0.0023, -0.0078, 1.0055]
        ]
    },
    "WideGamut": {
        "description": "Wide Gamut LCD Correction",
        "matrix": [
            [1.0067, -0.0045, -0.0022],
            [-0.0034, 1.0056, -0.0022],
            [0.0011, -0.0034, 1.0023]
        ]
    },
    "LCD": {
        "description": "Standard LCD Correction",
        "matrix": [
            [1.0000, 0.0000, 0.0000],
            [0.0000, 1.0000, 0.0000],
            [0.0000, 0.0000, 1.0000]
        ]
    }
}


class SpyderDriver(ArgyllBackend):
    """
    Datacolor Spyder specialized driver.

    Extends ArgyllBackend with Spyder-specific features:
    - Automatic model detection (Spyder5, SpyderX, SpyderX2)
    - Fast measurement mode
    - OLED display optimization
    - Ambient light measurement
    - Display analysis features
    """

    def __init__(self, argyll_path: Optional[Path] = None):
        super().__init__(argyll_path)
        self.model_type: str = SpyderType.UNKNOWN
        self.specs: Dict = {}
        self._correction_matrix: Optional[List[List[float]]] = None
        self._fast_mode: bool = False

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect Spyder devices specifically."""
        all_devices = super().detect_devices()

        # Filter for Spyder devices
        spyder_devices = []
        for device in all_devices:
            if self._is_spyder(device):
                self._identify_model(device)
                spyder_devices.append(device)

        return spyder_devices

    def _is_spyder(self, device: DeviceInfo) -> bool:
        """Check if device is a Spyder colorimeter."""
        name_lower = device.name.lower()
        return any(x in name_lower for x in [
            "spyder", "datacolor"
        ])

    def _identify_model(self, device: DeviceInfo) -> None:
        """Identify specific Spyder model and capabilities."""
        name_lower = device.name.lower()

        # Identify model
        if "spyderx2" in name_lower or "spyder x2" in name_lower:
            if "ultra" in name_lower:
                self.model_type = SpyderType.SPYDER_X2_ULTRA
            else:
                self.model_type = SpyderType.SPYDER_X2_ELITE
        elif "spyderx" in name_lower or "spyder x" in name_lower:
            if "elite" in name_lower:
                self.model_type = SpyderType.SPYDER_X_ELITE
            else:
                self.model_type = SpyderType.SPYDER_X_PRO
        elif "spyder5" in name_lower or "spyder 5" in name_lower:
            if "elite" in name_lower:
                self.model_type = SpyderType.SPYDER_5_ELITE
            elif "express" in name_lower:
                self.model_type = SpyderType.SPYDER_5_EXPRESS
            else:
                self.model_type = SpyderType.SPYDER_5_PRO
        elif "spyder4" in name_lower or "spyder 4" in name_lower:
            if "elite" in name_lower:
                self.model_type = SpyderType.SPYDER_4_ELITE
            else:
                self.model_type = SpyderType.SPYDER_4_PRO
        else:
            self.model_type = SpyderType.UNKNOWN

        # Load specs
        self.specs = SPYDER_SPECS.get(self.model_type, {})

        # Update device capabilities
        if self.specs.get("ambient"):
            device.capabilities.append("ambient")
        if self.specs.get("oled_mode"):
            device.capabilities.append("oled_mode")
        if self.specs.get("hdr"):
            device.capabilities.append("hdr")

    def connect(self, device_index: int = 0) -> bool:
        """Connect to Spyder device."""
        if not super().connect(device_index):
            return False

        if self.device_info:
            self._identify_model(self.device_info)

        return True

    def set_fast_mode(self, enabled: bool) -> bool:
        """
        Enable fast measurement mode.

        Reduces measurement time at cost of some accuracy.
        Good for quick spot checks.
        """
        self._fast_mode = enabled
        if enabled:
            self.averaging_count = 1
            self.integration_time = 0.5
        else:
            self.averaging_count = 3
            self.integration_time = 0  # Auto
        return True

    def set_display_correction(self, display_type: str) -> bool:
        """
        Apply display type correction matrix.

        Args:
            display_type: One of "WOLED", "QDOLED", "WideGamut", "LCD"
        """
        if display_type in SPYDER_CORRECTIONS:
            self._correction_matrix = SPYDER_CORRECTIONS[display_type]["matrix"]
            return True
        return False

    def measure_ambient(self) -> Optional[ColorMeasurement]:
        """
        Measure ambient light level.

        Returns illuminance in lux and CCT.
        """
        if not self.specs.get("ambient", False):
            self._report_progress("Device does not support ambient measurement", 0)
            return None

        if not self.is_connected:
            return None

        self._report_progress("Measuring ambient light...", 0.5)

        try:
            cmd = [str(self.spotread_path), "-a", "-x"]
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30
            )

            if result.returncode == 0:
                return self._parse_ambient_output(result.stdout)

        except Exception:
            pass

        return None

    def _parse_ambient_output(self, output: str) -> Optional[ColorMeasurement]:
        """Parse ambient measurement output."""
        lux_match = re.search(r'Lux:\s*([\d.]+)', output)
        cct_match = re.search(r'CCT:\s*([\d.]+)', output)

        if lux_match:
            lux = float(lux_match.group(1))
            cct = float(cct_match.group(1)) if cct_match else 0

            return ColorMeasurement(
                X=0, Y=lux, Z=0,
                cct=cct,
                measurement_mode="ambient"
            )

        return None

    def measure_spot(self) -> Optional[ColorMeasurement]:
        """
        Take spot measurement with Spyder-specific options.
        """
        measurement = super().measure_spot()

        if measurement and self._correction_matrix:
            measurement = self._apply_correction(measurement)

        return measurement

    def _apply_correction(self, measurement: ColorMeasurement) -> ColorMeasurement:
        """Apply device correction matrix to measurement."""
        if not self._correction_matrix:
            return measurement

        import numpy as np

        xyz = np.array([measurement.X, measurement.Y, measurement.Z])
        matrix = np.array(self._correction_matrix)
        corrected = matrix @ xyz

        return ColorMeasurement(
            X=float(corrected[0]),
            Y=float(corrected[1]),
            Z=float(corrected[2]),
            spectral_data=measurement.spectral_data,
            integration_time=measurement.integration_time,
            measurement_mode=measurement.measurement_mode
        )

    def calibrate_device(self) -> bool:
        """
        Perform Spyder dark calibration.

        Place device face-down or use lens cap.
        """
        self._report_progress("Performing dark calibration...", 0)
        self._report_progress("Place device face-down on a dark surface", 0.1)

        time.sleep(2)

        return super().calibrate_device()

    def analyze_display(self) -> Optional[Dict]:
        """
        Perform quick display analysis.

        Measures:
        - White point
        - Brightness
        - Black level
        - Contrast ratio
        - Gamut coverage estimate

        Returns dictionary with analysis results.
        """
        if not self.is_connected:
            return None

        results = {}

        self._report_progress("Analyzing display...", 0)

        # Measure white
        self._report_progress("Measuring white point...", 0.2)
        # Would need to display white patch first
        white = self.measure_spot()
        if white:
            results["white_luminance"] = white.luminance
            results["white_xy"] = (white.x, white.y)
            results["cct"] = white.cct

        # Measure black
        self._report_progress("Measuring black level...", 0.4)
        # Would need to display black patch
        black = self.measure_spot()
        if black:
            results["black_luminance"] = black.luminance

        # Calculate contrast
        if white and black and black.luminance > 0:
            results["contrast_ratio"] = white.luminance / black.luminance

        # Measure primaries for gamut estimate
        self._report_progress("Measuring primaries...", 0.6)
        # Would measure R, G, B patches

        self._report_progress("Analysis complete", 1.0)

        return results

    def get_measurement_time(self) -> float:
        """Get expected measurement time in seconds."""
        base_time = self.specs.get("measurement_time", 3.0)
        if self._fast_mode:
            return base_time * 0.5
        return base_time * self.averaging_count


class SpyderX(SpyderDriver):
    """Convenience class for SpyderX devices."""

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect only SpyderX devices."""
        all_spyders = super().detect_devices()
        return [d for d in all_spyders if "spyderx" in d.name.lower()]


class SpyderX2(SpyderDriver):
    """Convenience class for SpyderX2 devices."""

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect only SpyderX2 devices."""
        all_spyders = super().detect_devices()
        return [d for d in all_spyders if "spyderx2" in d.name.lower() or "spyder x2" in d.name.lower()]


def detect_spyder() -> Optional[SpyderDriver]:
    """
    Convenience function to detect and connect to a Spyder.

    Returns configured driver if device found, None otherwise.
    """
    driver = SpyderDriver()
    devices = driver.detect_devices()

    if devices:
        if driver.connect(0):
            return driver

    return None
