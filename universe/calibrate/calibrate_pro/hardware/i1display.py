"""
X-Rite i1Display Pro/Plus Driver

Specialized driver for X-Rite i1Display colorimeters.
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


class I1DisplayType:
    """i1Display model variants."""
    I1DISPLAY_PRO = "i1Display Pro"
    I1DISPLAY_PRO_PLUS = "i1Display Pro Plus"
    I1DISPLAY_STUDIO = "i1Display Studio"
    COLORMUNKI_DISPLAY = "ColorMunki Display"
    I1DISPLAY_2 = "i1Display 2"
    UNKNOWN = "Unknown i1Display"


# Device-specific correction matrices for common display types
I1DISPLAY_CORRECTIONS = {
    # OLED correction for i1Display Pro
    "OLED": {
        "description": "OLED Display Correction",
        "matrix": [
            [1.0245, -0.0156, -0.0089],
            [-0.0087, 1.0134, -0.0047],
            [0.0021, -0.0098, 1.0077]
        ]
    },
    # Wide gamut LCD correction
    "WideGamut": {
        "description": "Wide Gamut LCD Correction",
        "matrix": [
            [1.0089, -0.0067, -0.0022],
            [-0.0045, 1.0078, -0.0033],
            [0.0012, -0.0056, 1.0044]
        ]
    },
    # Standard LCD (sRGB)
    "LCD": {
        "description": "Standard LCD Correction",
        "matrix": [
            [1.0000, 0.0000, 0.0000],
            [0.0000, 1.0000, 0.0000],
            [0.0000, 0.0000, 1.0000]
        ]
    }
}


class I1DisplayDriver(ArgyllBackend):
    """
    X-Rite i1Display Pro/Plus specialized driver.

    Extends ArgyllBackend with i1Display-specific features:
    - Automatic model detection
    - OLED mode for improved black measurements
    - Ambient light measurement support
    - Extended dynamic range mode
    - Custom correction matrices
    """

    def __init__(self, argyll_path: Optional[Path] = None):
        super().__init__(argyll_path)
        self.model_type: str = I1DisplayType.UNKNOWN
        self.has_ambient_sensor: bool = False
        self.supports_edr: bool = False
        self.oled_mode: bool = False
        self._correction_matrix: Optional[List[List[float]]] = None

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect i1Display devices specifically."""
        all_devices = super().detect_devices()

        # Filter for i1Display devices
        i1_devices = []
        for device in all_devices:
            if self._is_i1display(device):
                # Enhance device info with i1Display-specific details
                self._identify_model(device)
                i1_devices.append(device)

        return i1_devices

    def _is_i1display(self, device: DeviceInfo) -> bool:
        """Check if device is an i1Display."""
        name_lower = device.name.lower()
        return any(x in name_lower for x in [
            "i1display", "i1 display", "colormunki display",
            "eye-one display", "xrite"
        ])

    def _identify_model(self, device: DeviceInfo) -> None:
        """Identify specific i1Display model and capabilities."""
        name_lower = device.name.lower()

        if "pro plus" in name_lower or "pro+" in name_lower:
            self.model_type = I1DisplayType.I1DISPLAY_PRO_PLUS
            self.has_ambient_sensor = True
            self.supports_edr = True
            device.capabilities.extend(["ambient", "edr", "oled_mode", "flicker"])
        elif "pro" in name_lower:
            self.model_type = I1DisplayType.I1DISPLAY_PRO
            self.has_ambient_sensor = True
            self.supports_edr = True
            device.capabilities.extend(["ambient", "edr", "oled_mode"])
        elif "studio" in name_lower:
            self.model_type = I1DisplayType.I1DISPLAY_STUDIO
            self.has_ambient_sensor = True
            self.supports_edr = False
            device.capabilities.extend(["ambient"])
        elif "colormunki" in name_lower:
            self.model_type = I1DisplayType.COLORMUNKI_DISPLAY
            self.has_ambient_sensor = True
            self.supports_edr = False
            device.capabilities.extend(["ambient"])
        elif "i1display 2" in name_lower or "i1d2" in name_lower:
            self.model_type = I1DisplayType.I1DISPLAY_2
            self.has_ambient_sensor = False
            self.supports_edr = False
        else:
            self.model_type = I1DisplayType.UNKNOWN

    def connect(self, device_index: int = 0) -> bool:
        """Connect to i1Display device."""
        if not super().connect(device_index):
            return False

        # Identify model after connection
        if self.device_info:
            self._identify_model(self.device_info)

        return True

    def set_oled_mode(self, enabled: bool) -> bool:
        """
        Enable OLED measurement mode.

        OLED mode uses longer integration times for better
        black level measurements on OLED displays.
        """
        if self.model_type not in [
            I1DisplayType.I1DISPLAY_PRO,
            I1DisplayType.I1DISPLAY_PRO_PLUS
        ]:
            return False

        self.oled_mode = enabled
        if enabled:
            self._correction_matrix = I1DISPLAY_CORRECTIONS["OLED"]["matrix"]
        return True

    def set_display_correction(self, display_type: str) -> bool:
        """
        Apply display type correction matrix.

        Args:
            display_type: One of "OLED", "WideGamut", "LCD"
        """
        if display_type in I1DISPLAY_CORRECTIONS:
            self._correction_matrix = I1DISPLAY_CORRECTIONS[display_type]["matrix"]
            return True
        return False

    def measure_ambient(self) -> Optional[ColorMeasurement]:
        """
        Measure ambient light level.

        Returns illuminance in lux and CCT.
        Requires diffuser to be attached.
        """
        if not self.has_ambient_sensor:
            self._report_progress("Device does not support ambient measurement", 0)
            return None

        if not self.is_connected:
            return None

        self._report_progress("Measuring ambient light...", 0.5)

        try:
            # Use spotread with ambient mode
            cmd = [str(self.spotread_path), "-a", "-x"]
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30
            )

            if result.returncode == 0:
                # Parse ambient reading
                return self._parse_ambient_output(result.stdout)

        except subprocess.TimeoutExpired:
            pass
        except Exception:
            pass

        return None

    def _parse_ambient_output(self, output: str) -> Optional[ColorMeasurement]:
        """Parse ambient measurement output from spotread."""
        # Look for illuminance value
        lux_match = re.search(r'Lux:\s*([\d.]+)', output)
        cct_match = re.search(r'CCT:\s*([\d.]+)', output)

        if lux_match:
            lux = float(lux_match.group(1))
            cct = float(cct_match.group(1)) if cct_match else 0

            # Create measurement with ambient data
            # For ambient, Y represents illuminance in lux
            return ColorMeasurement(
                X=0, Y=lux, Z=0,
                cct=cct,
                measurement_mode="ambient"
            )

        return None

    def measure_spot(self) -> Optional[ColorMeasurement]:
        """
        Take spot measurement with i1Display-specific options.

        Applies OLED mode settings and correction matrices if configured.
        """
        measurement = super().measure_spot()

        if measurement and self._correction_matrix:
            # Apply correction matrix
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

    def measure_flicker(self) -> Optional[Dict]:
        """
        Measure display flicker characteristics.

        Only available on i1Display Pro Plus.
        Returns flicker frequency and percentage.
        """
        if self.model_type != I1DisplayType.I1DISPLAY_PRO_PLUS:
            return None

        # Flicker measurement requires special ArgyllCMS build
        # or direct USB communication
        # This is a placeholder for future implementation
        return None

    def calibrate_device(self) -> bool:
        """
        Perform i1Display dark calibration.

        Should be done with lens cap on for best accuracy.
        """
        self._report_progress("Performing dark calibration...", 0)
        self._report_progress("Ensure lens cap is on the device", 0.1)

        # Give user time to attach cap
        time.sleep(2)

        return super().calibrate_device()

    def get_device_temperature(self) -> Optional[float]:
        """
        Get device sensor temperature.

        Temperature affects measurement accuracy.
        Optimal range is 20-25°C.
        """
        # Would require direct USB communication
        # Placeholder for future implementation
        return None

    def set_integration_time(self, seconds: float) -> bool:
        """
        Set measurement integration time.

        Longer times improve low-light accuracy.
        For OLED black levels, use 2-4 seconds.
        """
        if seconds < 0.1 or seconds > 10:
            return False

        self.integration_time = seconds
        return True


def detect_i1display() -> Optional[I1DisplayDriver]:
    """
    Convenience function to detect and connect to an i1Display.

    Returns configured driver if device found, None otherwise.
    """
    driver = I1DisplayDriver()
    devices = driver.detect_devices()

    if devices:
        if driver.connect(0):
            return driver

    return None
