"""
Abstract Colorimeter Interface

Defines the base class for all colorimeter/spectrophotometer implementations.
Supports various hardware devices through a unified API.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional, Tuple, Callable
from pathlib import Path
import numpy as np

class DeviceType(Enum):
    """Type of measurement device."""
    COLORIMETER = "colorimeter"           # Tristimulus colorimeter
    SPECTROPHOTOMETER = "spectrophotometer"  # Spectral measurement
    UNKNOWN = "unknown"

class CalibrationMode(Enum):
    """Device calibration/correction mode."""
    NONE = "none"                  # No correction
    FACTORY = "factory"            # Factory calibration
    CCSS = "ccss"                  # Colorimeter Calibration Spectral Set
    CCMX = "ccmx"                  # Colorimeter Correction Matrix
    EDR = "edr"                    # Extended Dynamic Range
    REFRESH = "refresh"            # Refresh display mode

class MeasurementType(Enum):
    """Type of measurement to perform."""
    SPOT = "spot"                  # Single spot reading
    AMBIENT = "ambient"            # Ambient light measurement
    EMISSION = "emission"          # Display emission
    REFRESH_RATE = "refresh"       # Display refresh rate detection

@dataclass
class DeviceInfo:
    """Information about a connected measurement device."""
    name: str                      # Device name
    manufacturer: str              # Manufacturer name
    model: str                     # Model identifier
    serial: str                    # Serial number
    device_type: DeviceType        # Device type
    firmware_version: str = ""     # Firmware version
    driver_version: str = ""       # Driver version
    capabilities: List[str] = field(default_factory=list)  # Supported features

    def supports_spectral(self) -> bool:
        """Check if device supports spectral measurements."""
        return self.device_type == DeviceType.SPECTROPHOTOMETER

    def supports_ambient(self) -> bool:
        """Check if device can measure ambient light."""
        return "ambient" in self.capabilities

    def to_dict(self) -> Dict:
        return {
            "name": self.name,
            "manufacturer": self.manufacturer,
            "model": self.model,
            "serial": self.serial,
            "type": self.device_type.value,
            "firmware": self.firmware_version,
            "capabilities": self.capabilities
        }

@dataclass
class ColorMeasurement:
    """Result of a color measurement."""
    # XYZ tristimulus values (Y in cd/m2 for absolute, or normalized)
    X: float
    Y: float
    Z: float

    # Chromaticity coordinates (calculated from XYZ)
    x: float = 0.0
    y: float = 0.0

    # Luminance in cd/m2 (nits)
    luminance: float = 0.0

    # CCT (Correlated Color Temperature)
    cct: float = 0.0

    # Delta uv from Planckian locus
    delta_uv: float = 0.0

    # Optional spectral data (wavelength -> power)
    spectral_data: Optional[Dict[float, float]] = None

    # Measurement metadata
    integration_time: float = 0.0  # seconds
    measurement_mode: str = ""

    def __post_init__(self):
        """Calculate derived values."""
        total = self.X + self.Y + self.Z
        if total > 0:
            self.x = self.X / total
            self.y = self.Y / total
        self.luminance = self.Y

        # Calculate CCT using McCamy's approximation
        if self.y > 0:
            n = (self.x - 0.3320) / (0.1858 - self.y)
            self.cct = 449.0 * n**3 + 3525.0 * n**2 + 6823.3 * n + 5520.33

    def get_xyz(self) -> np.ndarray:
        """Get XYZ as numpy array."""
        return np.array([self.X, self.Y, self.Z])

    def get_xy(self) -> Tuple[float, float]:
        """Get chromaticity coordinates."""
        return (self.x, self.y)

    def to_dict(self) -> Dict:
        return {
            "XYZ": [self.X, self.Y, self.Z],
            "xy": [self.x, self.y],
            "luminance_cdm2": self.luminance,
            "cct": self.cct,
            "delta_uv": self.delta_uv,
            "has_spectral": self.spectral_data is not None
        }

@dataclass
class CalibrationPatch:
    """A color patch for display calibration."""
    # Target RGB values [0-1]
    r: float
    g: float
    b: float

    # Patch identifier
    index: int = 0
    name: str = ""

    # Measured result (filled after measurement)
    measurement: Optional[ColorMeasurement] = None

    def get_rgb(self) -> Tuple[float, float, float]:
        return (self.r, self.g, self.b)

    def get_rgb_8bit(self) -> Tuple[int, int, int]:
        return (
            int(self.r * 255),
            int(self.g * 255),
            int(self.b * 255)
        )

class ColorimeterBase(ABC):
    """
    Abstract base class for colorimeter implementations.

    All hardware colorimeter drivers must inherit from this class
    and implement the abstract methods.
    """

    def __init__(self):
        self.device_info: Optional[DeviceInfo] = None
        self.is_connected: bool = False
        self.calibration_mode: CalibrationMode = CalibrationMode.FACTORY
        self.correction_matrix: Optional[np.ndarray] = None
        self.ccss_file: Optional[Path] = None

        # Measurement settings
        self.integration_time: float = 0.0  # 0 = auto
        self.averaging_count: int = 1

        # Progress callback
        self.progress_callback: Optional[Callable[[str, float], None]] = None

    def set_progress_callback(self, callback: Callable[[str, float], None]):
        """Set callback for progress updates."""
        self.progress_callback = callback

    def _report_progress(self, message: str, progress: float):
        """Report progress to callback."""
        if self.progress_callback:
            self.progress_callback(message, progress)

    # ==========================================================================
    # Abstract Methods - Must be implemented by subclasses
    # ==========================================================================

    @abstractmethod
    def detect_devices(self) -> List[DeviceInfo]:
        """
        Detect all connected measurement devices.

        Returns:
            List of DeviceInfo for each detected device
        """
        pass

    @abstractmethod
    def connect(self, device_index: int = 0) -> bool:
        """
        Connect to a measurement device.

        Args:
            device_index: Index of device from detect_devices()

        Returns:
            True if connection successful
        """
        pass

    @abstractmethod
    def disconnect(self) -> bool:
        """
        Disconnect from the current device.

        Returns:
            True if disconnection successful
        """
        pass

    @abstractmethod
    def calibrate_device(self) -> bool:
        """
        Perform device self-calibration (dark calibration, etc.).

        Returns:
            True if calibration successful
        """
        pass

    @abstractmethod
    def measure_spot(self) -> Optional[ColorMeasurement]:
        """
        Take a single spot measurement.

        Returns:
            ColorMeasurement or None if measurement failed
        """
        pass

    @abstractmethod
    def measure_ambient(self) -> Optional[ColorMeasurement]:
        """
        Measure ambient light level.

        Returns:
            ColorMeasurement with ambient light data, or None
        """
        pass

    # ==========================================================================
    # Optional Methods - Can be overridden by subclasses
    # ==========================================================================

    def set_display_type(self, display_type: str) -> bool:
        """
        Set display type for measurement optimization.

        Args:
            display_type: One of "LCD", "OLED", "CRT", "Projector"

        Returns:
            True if setting was applied
        """
        return True  # Default: no-op

    def set_refresh_mode(self, refresh_rate: float) -> bool:
        """
        Set refresh display mode for accurate measurement.

        Args:
            refresh_rate: Display refresh rate in Hz

        Returns:
            True if mode was set
        """
        return True  # Default: no-op

    def load_ccss(self, ccss_path: Path) -> bool:
        """
        Load Colorimeter Calibration Spectral Set file.

        Args:
            ccss_path: Path to .ccss file

        Returns:
            True if loaded successfully
        """
        if ccss_path.exists():
            self.ccss_file = ccss_path
            self.calibration_mode = CalibrationMode.CCSS
            return True
        return False

    def load_ccmx(self, ccmx_path: Path) -> bool:
        """
        Load Colorimeter Correction Matrix file.

        Args:
            ccmx_path: Path to .ccmx file

        Returns:
            True if loaded successfully
        """
        # Default implementation - subclasses should override
        return False

    def get_spectral_data(self) -> Optional[Dict[float, float]]:
        """
        Get spectral power distribution from last measurement.

        Returns:
            Dictionary mapping wavelength (nm) to power, or None
        """
        return None  # Default: not supported

    # ==========================================================================
    # Utility Methods
    # ==========================================================================

    def measure_patches(
        self,
        patches: List[CalibrationPatch],
        display_callback: Callable[[CalibrationPatch], None]
    ) -> List[CalibrationPatch]:
        """
        Measure a sequence of color patches.

        Args:
            patches: List of patches to measure
            display_callback: Function to display each patch on screen

        Returns:
            List of patches with measurements filled in
        """
        total = len(patches)

        for i, patch in enumerate(patches):
            self._report_progress(
                f"Measuring patch {i+1}/{total}: {patch.name or f'RGB({patch.r:.2f},{patch.g:.2f},{patch.b:.2f})'}",
                i / total
            )

            # Display the patch
            display_callback(patch)

            # Take measurement
            measurement = self.measure_spot()
            if measurement:
                patch.measurement = measurement

        self._report_progress("Measurement complete", 1.0)
        return patches

    def measure_grayscale(
        self,
        steps: int = 21,
        display_callback: Callable[[CalibrationPatch], None] = None
    ) -> List[CalibrationPatch]:
        """
        Measure grayscale ramp.

        Args:
            steps: Number of grayscale steps
            display_callback: Function to display each patch

        Returns:
            List of measured grayscale patches
        """
        patches = []
        for i in range(steps):
            level = i / (steps - 1)
            patches.append(CalibrationPatch(
                r=level, g=level, b=level,
                index=i,
                name=f"Gray {int(level * 100)}%"
            ))

        if display_callback:
            return self.measure_patches(patches, display_callback)
        return patches

    def measure_primaries(
        self,
        display_callback: Callable[[CalibrationPatch], None] = None
    ) -> Dict[str, ColorMeasurement]:
        """
        Measure display primary colors and white point.

        Args:
            display_callback: Function to display each patch

        Returns:
            Dictionary with "red", "green", "blue", "white" measurements
        """
        patches = [
            CalibrationPatch(1, 0, 0, 0, "Red"),
            CalibrationPatch(0, 1, 0, 1, "Green"),
            CalibrationPatch(0, 0, 1, 2, "Blue"),
            CalibrationPatch(1, 1, 1, 3, "White"),
            CalibrationPatch(0, 0, 0, 4, "Black"),
        ]

        if display_callback:
            patches = self.measure_patches(patches, display_callback)

        return {
            "red": patches[0].measurement,
            "green": patches[1].measurement,
            "blue": patches[2].measurement,
            "white": patches[3].measurement,
            "black": patches[4].measurement,
        }


# =============================================================================
# Patch Set Generators
# =============================================================================

def generate_grayscale_patches(steps: int = 21) -> List[CalibrationPatch]:
    """Generate grayscale ramp patches."""
    patches = []
    for i in range(steps):
        level = i / (steps - 1)
        patches.append(CalibrationPatch(
            r=level, g=level, b=level,
            index=i,
            name=f"Gray {int(level * 100)}%"
        ))
    return patches


def generate_primary_patches() -> List[CalibrationPatch]:
    """Generate primary color patches (R, G, B, W, K)."""
    return [
        CalibrationPatch(1, 0, 0, 0, "Red"),
        CalibrationPatch(0, 1, 0, 1, "Green"),
        CalibrationPatch(0, 0, 1, 2, "Blue"),
        CalibrationPatch(1, 1, 1, 3, "White"),
        CalibrationPatch(0, 0, 0, 4, "Black"),
        CalibrationPatch(1, 1, 0, 5, "Yellow"),
        CalibrationPatch(1, 0, 1, 6, "Magenta"),
        CalibrationPatch(0, 1, 1, 7, "Cyan"),
    ]


def generate_verification_patches() -> List[CalibrationPatch]:
    """Generate ColorChecker-like verification patches."""
    # Simplified ColorChecker-like set
    return [
        CalibrationPatch(0.459, 0.318, 0.267, 0, "Dark Skin"),
        CalibrationPatch(0.839, 0.610, 0.529, 1, "Light Skin"),
        CalibrationPatch(0.396, 0.502, 0.651, 2, "Blue Sky"),
        CalibrationPatch(0.392, 0.443, 0.290, 3, "Foliage"),
        CalibrationPatch(0.557, 0.525, 0.714, 4, "Blue Flower"),
        CalibrationPatch(0.400, 0.757, 0.706, 5, "Bluish Green"),
        CalibrationPatch(0.886, 0.498, 0.153, 6, "Orange"),
        CalibrationPatch(0.298, 0.380, 0.686, 7, "Purplish Blue"),
        CalibrationPatch(0.788, 0.353, 0.376, 8, "Moderate Red"),
        CalibrationPatch(0.341, 0.259, 0.424, 9, "Purple"),
        CalibrationPatch(0.600, 0.757, 0.227, 10, "Yellow Green"),
        CalibrationPatch(0.933, 0.635, 0.133, 11, "Orange Yellow"),
        CalibrationPatch(0.188, 0.259, 0.580, 12, "Blue"),
        CalibrationPatch(0.298, 0.588, 0.337, 13, "Green"),
        CalibrationPatch(0.710, 0.255, 0.220, 14, "Red"),
        CalibrationPatch(0.980, 0.804, 0.067, 15, "Yellow"),
        CalibrationPatch(0.773, 0.341, 0.541, 16, "Magenta"),
        CalibrationPatch(0.161, 0.537, 0.667, 17, "Cyan"),
        CalibrationPatch(0.980, 0.976, 0.949, 18, "White"),
        CalibrationPatch(0.788, 0.784, 0.773, 19, "Neutral 8"),
        CalibrationPatch(0.608, 0.604, 0.596, 20, "Neutral 6.5"),
        CalibrationPatch(0.439, 0.435, 0.431, 21, "Neutral 5"),
        CalibrationPatch(0.278, 0.278, 0.282, 22, "Neutral 3.5"),
        CalibrationPatch(0.137, 0.137, 0.145, 23, "Black"),
    ]


def generate_profiling_patches(size: int = 729) -> List[CalibrationPatch]:
    """
    Generate full profiling patch set.

    Args:
        size: Target number of patches (will be adjusted to cube)
               Common sizes: 125 (5^3), 343 (7^3), 729 (9^3), 1331 (11^3)

    Returns:
        List of profiling patches
    """
    # Calculate grid size
    grid = int(round(size ** (1/3)))
    actual_size = grid ** 3

    patches = []
    index = 0

    for r_idx in range(grid):
        for g_idx in range(grid):
            for b_idx in range(grid):
                r = r_idx / (grid - 1)
                g = g_idx / (grid - 1)
                b = b_idx / (grid - 1)

                patches.append(CalibrationPatch(
                    r=r, g=g, b=b,
                    index=index,
                    name=f"P{index:04d}"
                ))
                index += 1

    return patches
