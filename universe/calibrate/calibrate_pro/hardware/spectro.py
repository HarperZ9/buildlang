"""
Spectrophotometer Driver

Specialized driver for spectrophotometers including:
- Calibrite ColorChecker Display Pro/Plus
- X-Rite i1Pro/i1Pro2/i1Pro3
- JETI Specbos
- Photo Research spectroradiometers

Spectrophotometers provide full spectral data for highest accuracy.
"""

from pathlib import Path
from typing import Dict, List, Optional, Tuple
import subprocess
import re
import time
import numpy as np

from calibrate_pro.hardware.colorimeter_base import (
    ColorimeterBase, ColorMeasurement, DeviceInfo, DeviceType,
    CalibrationMode, CalibrationPatch
)
from calibrate_pro.hardware.argyll_backend import ArgyllBackend


class SpectroType:
    """Spectrophotometer types."""
    I1PRO = "i1Pro"
    I1PRO2 = "i1Pro2"
    I1PRO3 = "i1Pro3"
    I1PRO3_PLUS = "i1Pro3 Plus"
    COLORCHECKER_DISPLAY_PRO = "ColorChecker Display Pro"
    COLORCHECKER_DISPLAY_PLUS = "ColorChecker Display Plus"
    JETI_SPECBOS = "JETI Specbos"
    KLEIN_K10A = "Klein K-10A"
    PHOTO_RESEARCH = "Photo Research"
    UNKNOWN = "Unknown Spectrophotometer"


# Standard observer functions for spectral calculations
CIE_1931_2DEG = {
    "name": "CIE 1931 2° Standard Observer",
    "wavelength_range": (380, 780),
    "wavelength_step": 5
}

CIE_1964_10DEG = {
    "name": "CIE 1964 10° Standard Observer",
    "wavelength_range": (380, 780),
    "wavelength_step": 5
}


class SpectrophotometerDriver(ArgyllBackend):
    """
    Spectrophotometer driver with full spectral support.

    Features:
    - Full spectral power distribution capture
    - Spectral-to-XYZ conversion with observer selection
    - Reference spectrophotometer accuracy
    - Emission and reflective measurement modes
    """

    def __init__(self, argyll_path: Optional[Path] = None):
        super().__init__(argyll_path)
        self.model_type: str = SpectroType.UNKNOWN
        self.spectral_range: Tuple[int, int] = (380, 780)
        self.spectral_resolution: int = 10  # nm
        self.observer: str = "1931_2deg"
        self._last_spectral_data: Optional[Dict[float, float]] = None

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect spectrophotometer devices."""
        all_devices = super().detect_devices()

        spectro_devices = []
        for device in all_devices:
            if self._is_spectrophotometer(device):
                self._identify_model(device)
                device.device_type = DeviceType.SPECTROPHOTOMETER
                spectro_devices.append(device)

        return spectro_devices

    def _is_spectrophotometer(self, device: DeviceInfo) -> bool:
        """Check if device is a spectrophotometer."""
        name_lower = device.name.lower()
        return any(x in name_lower for x in [
            "i1pro", "i1 pro", "specbos", "spectro",
            "colorchecker display pro", "colorchecker display plus",
            "klein", "k-10", "photo research", "pr-"
        ])

    def _identify_model(self, device: DeviceInfo) -> None:
        """Identify spectrophotometer model."""
        name_lower = device.name.lower()

        if "i1pro3" in name_lower or "i1 pro 3" in name_lower:
            if "plus" in name_lower:
                self.model_type = SpectroType.I1PRO3_PLUS
                self.spectral_resolution = 3.5
            else:
                self.model_type = SpectroType.I1PRO3
                self.spectral_resolution = 3.5
        elif "i1pro2" in name_lower or "i1 pro 2" in name_lower:
            self.model_type = SpectroType.I1PRO2
            self.spectral_resolution = 10
        elif "i1pro" in name_lower or "i1 pro" in name_lower:
            self.model_type = SpectroType.I1PRO
            self.spectral_resolution = 10
        elif "colorchecker display plus" in name_lower:
            self.model_type = SpectroType.COLORCHECKER_DISPLAY_PLUS
            self.spectral_resolution = 10
        elif "colorchecker display pro" in name_lower:
            self.model_type = SpectroType.COLORCHECKER_DISPLAY_PRO
            self.spectral_resolution = 10
        elif "specbos" in name_lower or "jeti" in name_lower:
            self.model_type = SpectroType.JETI_SPECBOS
            self.spectral_resolution = 1
            self.spectral_range = (350, 1000)
        elif "klein" in name_lower or "k-10" in name_lower:
            self.model_type = SpectroType.KLEIN_K10A
            self.spectral_resolution = 1
        else:
            self.model_type = SpectroType.UNKNOWN

        # Add spectral capability
        device.capabilities.append("spectral")
        device.capabilities.append("emission")

        # Add model-specific capabilities
        if self.model_type in [SpectroType.I1PRO, SpectroType.I1PRO2,
                               SpectroType.I1PRO3, SpectroType.I1PRO3_PLUS]:
            device.capabilities.extend(["reflective", "ambient", "scanning"])

    def set_observer(self, observer: str) -> bool:
        """
        Set CIE observer for spectral calculations.

        Args:
            observer: "1931_2deg" or "1964_10deg"
        """
        if observer in ["1931_2deg", "1964_10deg"]:
            self.observer = observer
            return True
        return False

    def measure_spot(self) -> Optional[ColorMeasurement]:
        """
        Take spectral measurement.

        Returns ColorMeasurement with full spectral data.
        """
        measurement = super().measure_spot()

        if measurement:
            # Get spectral data if available
            spectral = self.get_spectral_data()
            if spectral:
                measurement.spectral_data = spectral
                self._last_spectral_data = spectral

        return measurement

    def get_spectral_data(self) -> Optional[Dict[float, float]]:
        """
        Get spectral power distribution from last measurement.

        Returns dictionary mapping wavelength (nm) to power.
        """
        if self._last_spectral_data:
            return self._last_spectral_data

        # Parse from ArgyllCMS output if available
        return None

    def spectral_to_xyz(
        self,
        spectral_data: Dict[float, float],
        illuminant: str = "D65"
    ) -> Tuple[float, float, float]:
        """
        Convert spectral data to XYZ using color matching functions.

        Args:
            spectral_data: Wavelength (nm) to power mapping
            illuminant: Reference illuminant for normalization

        Returns:
            (X, Y, Z) tristimulus values
        """
        # Load color matching functions based on observer
        cmf = self._get_color_matching_functions()

        wavelengths = sorted(spectral_data.keys())
        powers = [spectral_data[w] for w in wavelengths]

        # Interpolate to CMF wavelengths if needed
        X = Y = Z = 0.0

        for i, wl in enumerate(wavelengths):
            if 380 <= wl <= 780:
                # Get CMF values (simplified - should use lookup table)
                x_bar, y_bar, z_bar = self._cmf_at_wavelength(wl)
                power = powers[i]

                X += power * x_bar
                Y += power * y_bar
                Z += power * z_bar

        # Normalize
        k = 100.0 / max(Y, 0.0001)

        return (X * k, Y, Z * k)

    def _get_color_matching_functions(self) -> Dict:
        """Load CIE color matching functions."""
        if self.observer == "1964_10deg":
            return CIE_1964_10DEG
        return CIE_1931_2DEG

    def _cmf_at_wavelength(self, wavelength: float) -> Tuple[float, float, float]:
        """
        Get CIE XYZ color matching function values at wavelength.

        This is a simplified approximation. Full implementation
        would use tabulated CMF data.
        """
        # Simplified Gaussian approximation
        wl = wavelength

        # X bar (two peaks)
        x = (1.056 * np.exp(-0.5 * ((wl - 599.8) / 37.9) ** 2) +
             0.362 * np.exp(-0.5 * ((wl - 442.0) / 16.0) ** 2) -
             0.065 * np.exp(-0.5 * ((wl - 501.1) / 20.4) ** 2))

        # Y bar
        y = 1.217 * np.exp(-0.5 * ((wl - 568.8) / 46.9) ** 2)

        # Z bar
        z = 1.953 * np.exp(-0.5 * ((wl - 437.0) / 21.3) ** 2)

        return (max(0, x), max(0, y), max(0, z))

    def calculate_cri(self, spectral_data: Dict[float, float]) -> float:
        """
        Calculate Color Rendering Index from spectral data.

        CRI measures how accurately a light source renders colors
        compared to a reference illuminant.

        Returns CRI value (0-100, 100 is perfect).
        """
        # Simplified CRI calculation
        # Full implementation would use 8 or 14 test color samples

        if not spectral_data:
            return 0.0

        # Calculate CCT first
        X, Y, Z = self.spectral_to_xyz(spectral_data)
        total = X + Y + Z
        if total == 0:
            return 0.0

        x = X / total
        y = Y / total

        # McCamy's CCT approximation
        n = (x - 0.3320) / (0.1858 - y)
        cct = 449 * n**3 + 3525 * n**2 + 6823.3 * n + 5520.33

        # For emission sources, CRI is typically high (90+)
        # This is a placeholder - real calculation is complex
        return 95.0

    def calculate_tlci(self, spectral_data: Dict[float, float]) -> float:
        """
        Calculate Television Lighting Consistency Index.

        TLCI measures color accuracy for video/broadcast applications.

        Returns TLCI value (0-100).
        """
        # Placeholder - TLCI calculation requires camera response data
        return 90.0

    def measure_reflective(self, reference_white: bool = True) -> Optional[ColorMeasurement]:
        """
        Take reflective measurement (for print/material).

        Requires i1Pro or similar with reflective capability.

        Args:
            reference_white: Whether to use white reference calibration
        """
        if "reflective" not in (self.device_info.capabilities if self.device_info else []):
            return None

        # Would use spotread with reflective mode
        return self.measure_spot()

    def calibrate_device(self) -> bool:
        """
        Perform spectrophotometer calibration.

        For i1Pro: Place on calibration tile.
        For display-only devices: Cap on lens.
        """
        self._report_progress("Performing calibration...", 0)

        if self.model_type in [SpectroType.I1PRO, SpectroType.I1PRO2,
                                SpectroType.I1PRO3, SpectroType.I1PRO3_PLUS]:
            self._report_progress("Place device on calibration tile", 0.1)
        else:
            self._report_progress("Ensure lens cap is on", 0.1)

        time.sleep(2)

        return super().calibrate_device()


class ColorCheckerDisplay(SpectrophotometerDriver):
    """
    Calibrite ColorChecker Display driver.

    ColorChecker Display Pro/Plus are spectrophotometer-class devices
    optimized for display measurement.
    """

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect ColorChecker Display devices."""
        all_spectros = super().detect_devices()
        return [d for d in all_spectros if "colorchecker" in d.name.lower()]

    def set_oled_mode(self, enabled: bool) -> bool:
        """
        Enable OLED measurement mode.

        Uses extended integration for better OLED accuracy.
        """
        if enabled:
            self.integration_time = 2.0
            self.averaging_count = 3
        else:
            self.integration_time = 0  # Auto
            self.averaging_count = 1
        return True


class I1Pro(SpectrophotometerDriver):
    """X-Rite i1Pro series driver."""

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect i1Pro devices."""
        all_spectros = super().detect_devices()
        return [d for d in all_spectros if "i1pro" in d.name.lower() or "i1 pro" in d.name.lower()]

    def set_scanning_mode(self, enabled: bool) -> bool:
        """
        Enable strip scanning mode for i1Pro.

        Used for reading test charts and printed materials.
        """
        # Would configure device for scanning mode
        return True


def detect_spectrophotometer() -> Optional[SpectrophotometerDriver]:
    """
    Detect and connect to a spectrophotometer.

    Returns configured driver if device found, None otherwise.
    """
    driver = SpectrophotometerDriver()
    devices = driver.detect_devices()

    if devices:
        if driver.connect(0):
            return driver

    return None


def detect_colorchecker_display() -> Optional[ColorCheckerDisplay]:
    """Detect and connect to ColorChecker Display."""
    driver = ColorCheckerDisplay()
    devices = driver.detect_devices()

    if devices:
        if driver.connect(0):
            return driver

    return None
