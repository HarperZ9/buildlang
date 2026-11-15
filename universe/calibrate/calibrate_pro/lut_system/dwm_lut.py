"""
DWM 3D LUT Integration

System-wide 3D LUT application via Windows Desktop Window Manager.
Works with all GPU vendors (NVIDIA, AMD, Intel) and all applications.

Uses ledoge/dwm_lut for DWM hooking: https://github.com/ledoge/dwm_lut

This module provides functionality to:
1. Load 3D LUTs into DWM color pipeline via dwm_lut tool
2. Apply per-display color correction (HDR and SDR)
3. Support PQ (ST.2084) EOTF for HDR calibration
4. Hot-swap LUTs without restart

Requirements:
- Windows 10 20H2+ or Windows 11
- dwm_lut from ledoge/dwm_lut (DwmLutGUI.exe + dwm_lut.dll)

LUT File Placement:
- LUT files go to: %SYSTEMROOT%\\Temp\\luts (typically C:\\Windows\\Temp\\luts)
- SDR naming: {left}_{top}.cube (e.g., 0_0.cube for primary at 0,0)
- HDR naming: {left}_{top}_hdr.cube (e.g., 0_0_hdr.cube)
- DwmLutGUI.exe must be running to apply the LUTs
"""

import ctypes
import sys
from ctypes import wintypes, c_void_p, c_size_t, POINTER, byref
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Union
import struct
import numpy as np
from enum import Enum
from dataclasses import dataclass
import os
import shutil
import subprocess
import time

# Windows API constants
DISPLAY_DEVICE_ACTIVE = 0x00000001
DISPLAY_DEVICE_PRIMARY_DEVICE = 0x00000004
ENUM_CURRENT_SETTINGS = -1

# ST.2084 PQ constants (ITU-R BT.2100)
PQ_M1 = 2610 / 16384  # 0.1593017578125
PQ_M2 = 2523 / 4096 * 128  # 78.84375
PQ_C1 = 3424 / 4096  # 0.8359375
PQ_C2 = 2413 / 4096 * 32  # 18.8515625
PQ_C3 = 2392 / 4096 * 32  # 18.6875

# Reference white in nits
SDR_REFERENCE_WHITE = 80  # SDR content reference (sRGB)
HDR_REFERENCE_WHITE = 203  # HDR10 reference white
HDR_PEAK_LUMINANCE = 10000  # PQ max luminance


class ColorPipelineStage(Enum):
    """Color pipeline stages where LUT can be applied."""
    PRE_BLEND = "pre_blend"
    POST_BLEND = "post_blend"
    DISPLAY_OUTPUT = "output"


class LUTColorSpace(Enum):
    """Color space for LUT interpretation."""
    SRGB = "sRGB"
    SCRGB = "scRGB"
    HDR10 = "HDR10"
    LINEAR = "linear"


class LUTType(Enum):
    """Type of LUT (SDR or HDR)."""
    SDR = "sdr"
    HDR = "hdr"


@dataclass
class MonitorInfo:
    """Information about a connected monitor."""
    device_name: str
    friendly_name: str
    left: int
    top: int
    right: int
    bottom: int
    width: int
    height: int
    is_primary: bool
    is_hdr: bool
    device_id: str


@dataclass
class DisplayLUTInfo:
    """Information about LUT applied to a display."""
    display_id: int
    display_name: str
    lut_active: bool
    lut_path: Optional[str]
    lut_size: int
    color_space: LUTColorSpace
    lut_type: LUTType


class DwmLutError(Exception):
    """DWM LUT operation error."""
    pass


# =============================================================================
# PQ (ST.2084) EOTF Functions for HDR
# =============================================================================

def pq_eotf(E: np.ndarray) -> np.ndarray:
    """
    PQ Electro-Optical Transfer Function (ST.2084).
    Converts PQ-encoded signal (0-1) to linear light (0-10000 nits).

    Args:
        E: PQ-encoded signal values (0-1)

    Returns:
        Linear light values in nits (0-10000)
    """
    E = np.clip(E, 0, 1)
    E_pow = np.power(E, 1 / PQ_M2)
    num = np.maximum(E_pow - PQ_C1, 0)
    den = PQ_C2 - PQ_C3 * E_pow
    return HDR_PEAK_LUMINANCE * np.power(num / den, 1 / PQ_M1)


def pq_oetf(Y: np.ndarray) -> np.ndarray:
    """
    PQ Opto-Electronic Transfer Function (ST.2084 inverse).
    Converts linear light (nits) to PQ-encoded signal (0-1).

    Args:
        Y: Linear light values in nits (0-10000)

    Returns:
        PQ-encoded signal values (0-1)
    """
    Y = np.clip(Y, 0, HDR_PEAK_LUMINANCE)
    Y_norm = Y / HDR_PEAK_LUMINANCE
    Y_pow = np.power(Y_norm, PQ_M1)
    num = PQ_C1 + PQ_C2 * Y_pow
    den = 1 + PQ_C3 * Y_pow
    return np.power(num / den, PQ_M2)


def srgb_eotf(V: np.ndarray) -> np.ndarray:
    """
    sRGB Electro-Optical Transfer Function.
    Converts sRGB-encoded signal (0-1) to linear light (0-1).
    """
    V = np.clip(V, 0, 1)
    return np.where(
        V <= 0.04045,
        V / 12.92,
        np.power((V + 0.055) / 1.055, 2.4)
    )


def srgb_oetf(L: np.ndarray) -> np.ndarray:
    """
    sRGB Opto-Electronic Transfer Function (inverse).
    Converts linear light (0-1) to sRGB-encoded signal (0-1).
    """
    L = np.clip(L, 0, 1)
    return np.where(
        L <= 0.0031308,
        L * 12.92,
        1.055 * np.power(L, 1/2.4) - 0.055
    )


def bt1886_eotf(V: np.ndarray, gamma: float = 2.4, Lw: float = 1.0, Lb: float = 0.0) -> np.ndarray:
    """
    BT.1886 EOTF for broadcast reference monitors.

    Args:
        V: Input signal (0-1)
        gamma: Display gamma (typically 2.4)
        Lw: Peak white luminance (normalized)
        Lb: Black luminance (normalized)
    """
    V = np.clip(V, 0, 1)
    a = (Lw ** (1/gamma) - Lb ** (1/gamma)) ** gamma
    b = Lb ** (1/gamma) / (Lw ** (1/gamma) - Lb ** (1/gamma))
    return a * np.power(np.maximum(V + b, 0), gamma)


# =============================================================================
# Color Space Conversion Matrices
# =============================================================================

# sRGB/BT.709 to XYZ (D65)
SRGB_TO_XYZ = np.array([
    [0.4124564, 0.3575761, 0.1804375],
    [0.2126729, 0.7151522, 0.0721750],
    [0.0193339, 0.1191920, 0.9503041]
])

# XYZ to sRGB/BT.709 (D65)
XYZ_TO_SRGB = np.array([
    [ 3.2404542, -1.5371385, -0.4985314],
    [-0.9692660,  1.8760108,  0.0415560],
    [ 0.0556434, -0.2040259,  1.0572252]
])

# BT.2020 to XYZ (D65)
BT2020_TO_XYZ = np.array([
    [0.6369580, 0.1446169, 0.1688810],
    [0.2627002, 0.6779981, 0.0593017],
    [0.0000000, 0.0280727, 1.0609851]
])

# XYZ to BT.2020 (D65)
XYZ_TO_BT2020 = np.array([
    [ 1.7166512, -0.3556708, -0.2533663],
    [-0.6666844,  1.6164812,  0.0157685],
    [ 0.0176399, -0.0427706,  0.9421031]
])

# Direct sRGB to BT.2020 conversion
SRGB_TO_BT2020 = XYZ_TO_BT2020 @ SRGB_TO_XYZ

# Direct BT.2020 to sRGB conversion
BT2020_TO_SRGB = XYZ_TO_SRGB @ BT2020_TO_XYZ


def apply_matrix(rgb: np.ndarray, matrix: np.ndarray) -> np.ndarray:
    """Apply 3x3 color matrix to RGB values."""
    if rgb.ndim == 1:
        return matrix @ rgb
    else:
        return np.dot(rgb, matrix.T)


# =============================================================================
# LUT Generation
# =============================================================================

def generate_identity_lut(size: int = 33) -> np.ndarray:
    """Generate identity (pass-through) 3D LUT."""
    lut = np.zeros((size, size, size, 3), dtype=np.float32)
    for r in range(size):
        for g in range(size):
            for b in range(size):
                lut[r, g, b, 0] = r / (size - 1)
                lut[r, g, b, 1] = g / (size - 1)
                lut[r, g, b, 2] = b / (size - 1)
    return lut


def generate_hdr_calibration_lut(
    size: int = 33,
    target_gamma: float = 2.2,
    target_whitepoint: Tuple[float, float, float] = (1.0, 1.0, 1.0),
    rgb_gains: Tuple[float, float, float] = (1.0, 1.0, 1.0),
    rgb_offsets: Tuple[float, float, float] = (0.0, 0.0, 0.0),
    peak_luminance: float = 1000.0,
    sdr_white: float = 203.0,
) -> np.ndarray:
    """
    Generate HDR calibration 3D LUT with PQ EOTF.

    This LUT corrects HDR10 content for display calibration.
    Input: PQ-encoded BT.2020 HDR signal
    Output: Calibrated PQ-encoded BT.2020 signal

    Args:
        size: LUT size (17, 33, or 65)
        target_gamma: Target display gamma for SDR content mapping
        target_whitepoint: RGB multipliers for white point correction
        rgb_gains: Per-channel gain adjustments
        rgb_offsets: Per-channel offset adjustments (black level)
        peak_luminance: Display peak luminance in nits
        sdr_white: SDR reference white level in nits

    Returns:
        3D LUT as numpy array [size, size, size, 3]
    """
    lut = np.zeros((size, size, size, 3), dtype=np.float32)

    for ri in range(size):
        for gi in range(size):
            for bi in range(size):
                # Input PQ values (0-1)
                r_pq = ri / (size - 1)
                g_pq = gi / (size - 1)
                b_pq = bi / (size - 1)

                # Decode PQ to linear light (nits)
                rgb_pq = np.array([r_pq, g_pq, b_pq])
                rgb_linear_nits = pq_eotf(rgb_pq)

                # Normalize to 0-1 range based on peak luminance
                rgb_linear = rgb_linear_nits / peak_luminance

                # Apply calibration corrections
                # 1. Apply RGB offsets (black level adjustment)
                rgb_corrected = rgb_linear + np.array(rgb_offsets)

                # 2. Apply RGB gains
                rgb_corrected = rgb_corrected * np.array(rgb_gains)

                # 3. Apply white point correction
                rgb_corrected = rgb_corrected * np.array(target_whitepoint)

                # Clamp to valid range
                rgb_corrected = np.clip(rgb_corrected, 0, 1)

                # Convert back to nits
                rgb_corrected_nits = rgb_corrected * peak_luminance

                # Encode back to PQ
                rgb_out_pq = pq_oetf(rgb_corrected_nits)

                lut[ri, gi, bi] = rgb_out_pq

    return lut


def generate_sdr_calibration_lut(
    size: int = 33,
    target_gamma: float = 2.2,
    target_whitepoint: Tuple[float, float, float] = (1.0, 1.0, 1.0),
    rgb_gains: Tuple[float, float, float] = (1.0, 1.0, 1.0),
    rgb_offsets: Tuple[float, float, float] = (0.0, 0.0, 0.0),
    source_gamma: float = 2.2,
) -> np.ndarray:
    """
    Generate SDR calibration 3D LUT with sRGB/gamma correction.

    Args:
        size: LUT size (17, 33, or 65)
        target_gamma: Target display gamma
        target_whitepoint: RGB multipliers for white point correction
        rgb_gains: Per-channel gain adjustments
        rgb_offsets: Per-channel offset adjustments
        source_gamma: Assumed source content gamma

    Returns:
        3D LUT as numpy array [size, size, size, 3]
    """
    lut = np.zeros((size, size, size, 3), dtype=np.float32)

    for ri in range(size):
        for gi in range(size):
            for bi in range(size):
                # Input gamma-encoded values (0-1)
                r = ri / (size - 1)
                g = gi / (size - 1)
                b = bi / (size - 1)

                # Decode to linear using source gamma
                rgb_linear = np.power(np.array([r, g, b]), source_gamma)

                # Apply calibration corrections
                rgb_corrected = rgb_linear + np.array(rgb_offsets)
                rgb_corrected = rgb_corrected * np.array(rgb_gains)
                rgb_corrected = rgb_corrected * np.array(target_whitepoint)

                # Clamp
                rgb_corrected = np.clip(rgb_corrected, 0, 1)

                # Encode with target gamma
                rgb_out = np.power(rgb_corrected, 1 / target_gamma)

                lut[ri, gi, bi] = rgb_out

    return lut


# =============================================================================
# .cube File Format
# =============================================================================

def write_cube_file(path: Path, lut: np.ndarray, title: str = "Calibration LUT") -> None:
    """
    Write 3D LUT to .cube file format.

    Args:
        path: Output file path
        lut: 3D LUT array [size, size, size, 3] with values 0-1
        title: LUT title for header
    """
    size = lut.shape[0]

    with open(path, 'w') as f:
        f.write(f"TITLE \"{title}\"\n")
        f.write(f"LUT_3D_SIZE {size}\n")
        f.write("DOMAIN_MIN 0.0 0.0 0.0\n")
        f.write("DOMAIN_MAX 1.0 1.0 1.0\n")
        f.write("\n")

        # Write LUT data (blue varies fastest, then green, then red)
        for ri in range(size):
            for gi in range(size):
                for bi in range(size):
                    r, g, b = lut[ri, gi, bi]
                    f.write(f"{r:.6f} {g:.6f} {b:.6f}\n")


def read_cube_file(path: Path) -> Tuple[np.ndarray, int]:
    """
    Read 3D LUT from .cube file format.

    Args:
        path: Input file path

    Returns:
        Tuple of (lut array, size)
    """
    size = None
    data = []

    with open(path, 'r') as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith('#'):
                continue
            if line.startswith('TITLE'):
                continue
            if line.startswith('LUT_3D_SIZE'):
                size = int(line.split()[1])
                continue
            if line.startswith('DOMAIN'):
                continue

            # Parse RGB values
            parts = line.split()
            if len(parts) >= 3:
                try:
                    r, g, b = float(parts[0]), float(parts[1]), float(parts[2])
                    data.append([r, g, b])
                except ValueError:
                    continue

    if size is None:
        # Try to infer size from data count
        count = len(data)
        size = int(round(count ** (1/3)))

    # Reshape to 3D LUT
    lut = np.array(data, dtype=np.float32).reshape(size, size, size, 3)
    return lut, size


# =============================================================================
# Monitor Detection and Position
# =============================================================================

class DEVMODE(ctypes.Structure):
    """Windows DEVMODE structure for display settings."""
    _fields_ = [
        ("dmDeviceName", wintypes.WCHAR * 32),
        ("dmSpecVersion", wintypes.WORD),
        ("dmDriverVersion", wintypes.WORD),
        ("dmSize", wintypes.WORD),
        ("dmDriverExtra", wintypes.WORD),
        ("dmFields", wintypes.DWORD),
        ("dmPositionX", wintypes.LONG),  # Union member
        ("dmPositionY", wintypes.LONG),  # Union member
        ("dmDisplayOrientation", wintypes.DWORD),
        ("dmDisplayFixedOutput", wintypes.DWORD),
        ("dmColor", wintypes.SHORT),
        ("dmDuplex", wintypes.SHORT),
        ("dmYResolution", wintypes.SHORT),
        ("dmTTOption", wintypes.SHORT),
        ("dmCollate", wintypes.SHORT),
        ("dmFormName", wintypes.WCHAR * 32),
        ("dmLogPixels", wintypes.WORD),
        ("dmBitsPerPel", wintypes.DWORD),
        ("dmPelsWidth", wintypes.DWORD),
        ("dmPelsHeight", wintypes.DWORD),
        ("dmDisplayFlags", wintypes.DWORD),
        ("dmDisplayFrequency", wintypes.DWORD),
        ("dmICMMethod", wintypes.DWORD),
        ("dmICMIntent", wintypes.DWORD),
        ("dmMediaType", wintypes.DWORD),
        ("dmDitherType", wintypes.DWORD),
        ("dmReserved1", wintypes.DWORD),
        ("dmReserved2", wintypes.DWORD),
        ("dmPanningWidth", wintypes.DWORD),
        ("dmPanningHeight", wintypes.DWORD),
    ]


class DISPLAY_DEVICE(ctypes.Structure):
    """Windows DISPLAY_DEVICE structure."""
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("DeviceName", wintypes.WCHAR * 32),
        ("DeviceString", wintypes.WCHAR * 128),
        ("StateFlags", wintypes.DWORD),
        ("DeviceID", wintypes.WCHAR * 128),
        ("DeviceKey", wintypes.WCHAR * 128),
    ]


def get_monitors() -> List[MonitorInfo]:
    """
    Get list of connected monitors with position information.

    Returns:
        List of MonitorInfo objects with position and HDR status
    """
    monitors = []
    user32 = ctypes.windll.user32

    device = DISPLAY_DEVICE()
    device.cb = ctypes.sizeof(device)

    i = 0
    while user32.EnumDisplayDevicesW(None, i, ctypes.byref(device), 0):
        if device.StateFlags & DISPLAY_DEVICE_ACTIVE:
            # Get display settings including position
            devmode = DEVMODE()
            devmode.dmSize = ctypes.sizeof(devmode)

            if user32.EnumDisplaySettingsW(device.DeviceName, ENUM_CURRENT_SETTINGS, ctypes.byref(devmode)):
                left = devmode.dmPositionX
                top = devmode.dmPositionY
                width = devmode.dmPelsWidth
                height = devmode.dmPelsHeight

                # Check if HDR is enabled (Windows 10 1903+)
                is_hdr = _check_hdr_status(device.DeviceName)

                monitor = MonitorInfo(
                    device_name=device.DeviceName,
                    friendly_name=device.DeviceString,
                    left=left,
                    top=top,
                    right=left + width,
                    bottom=top + height,
                    width=width,
                    height=height,
                    is_primary=bool(device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE),
                    is_hdr=is_hdr,
                    device_id=device.DeviceID
                )
                monitors.append(monitor)

        i += 1

    return monitors


def _check_hdr_status(device_name: str) -> bool:
    """Check if HDR is enabled on a display (requires Windows 10 1903+)."""
    try:
        # Try to use DXGI to check HDR status
        # This is a simplified check - full implementation would use IDXGIOutput6
        import winreg

        # Check registry for HDR status
        key_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\VideoSettings"
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path) as key:
                enable_hdr, _ = winreg.QueryValueEx(key, "EnableHDRForPlayback")
                return bool(enable_hdr)
        except (FileNotFoundError, OSError):
            pass

        return False
    except Exception:
        return False


def get_lut_filename(monitor: MonitorInfo, lut_type: LUTType) -> str:
    """
    Get the LUT filename for dwm_lut based on monitor position.

    dwm_lut uses the format:
    - SDR: {left}_{top}.cube
    - HDR: {left}_{top}_hdr.cube

    Args:
        monitor: Monitor information with position
        lut_type: SDR or HDR LUT type

    Returns:
        Filename string (e.g., "0_0.cube" or "1920_0_hdr.cube")
    """
    if lut_type == LUTType.HDR:
        return f"{monitor.left}_{monitor.top}_hdr.cube"
    else:
        return f"{monitor.left}_{monitor.top}.cube"


def get_dwm_lut_directory() -> Path:
    """Get the dwm_lut LUT directory path."""
    # dwm_lut looks for LUTs in %SYSTEMROOT%\Temp\luts
    system_root = os.environ.get('SYSTEMROOT', 'C:\\Windows')
    return Path(system_root) / "Temp" / "luts"


# =============================================================================
# DWM LUT Controller
# =============================================================================

class DwmLutController:
    """
    Controller for DWM-level 3D LUT application using ledoge/dwm_lut.

    This controller manages LUT files and interacts with dwm_lut tool
    to apply system-wide color correction for both SDR and HDR content.
    """

    def __init__(self, dwm_lut_path: Optional[Path] = None):
        """
        Initialize DWM LUT controller.

        Args:
            dwm_lut_path: Path to dwm_lut directory containing DwmLutGUI.exe
        """
        self._dwm_lut_path = dwm_lut_path
        self._lut_directory = get_dwm_lut_directory()
        self._active_luts: Dict[str, DisplayLUTInfo] = {}
        self._monitors: List[MonitorInfo] = []

        # Find dwm_lut installation
        self._find_dwm_lut()

        # Ensure LUT directory exists
        self._ensure_lut_directory()

        # Refresh monitor list
        self.refresh_monitors()

    def _find_dwm_lut(self) -> None:
        """Find dwm_lut installation."""
        if self._dwm_lut_path and (self._dwm_lut_path / "DwmLutGUI.exe").exists():
            return

        # Search common locations
        search_paths = []

        # Frozen build: look next to the executable
        if getattr(sys, 'frozen', False):
            search_paths.append(Path(sys.executable).parent / "dwm_lut")

        search_paths.extend([
            Path(__file__).parent.parent.parent / "dwm_lut",  # calibrate/dwm_lut
            Path(__file__).parent / "bin",
            Path("C:/Program Files/dwm_lut"),
            Path.home() / "dwm_lut",
        ])

        for path in search_paths:
            if path.exists() and (path / "DwmLutGUI.exe").exists():
                self._dwm_lut_path = path
                return

        # Check PATH
        import shutil as sh
        exe = sh.which("DwmLutGUI.exe")
        if exe:
            self._dwm_lut_path = Path(exe).parent

    def _ensure_lut_directory(self) -> None:
        """Ensure the LUT directory exists."""
        try:
            self._lut_directory.mkdir(parents=True, exist_ok=True)
        except PermissionError:
            import logging
            logging.getLogger(__name__).warning(
                "Cannot create LUT directory %s (may need admin)", self._lut_directory
            )

    def _ensure_dwm_running(self) -> None:
        """Ensure DwmLutGUI is running after a LUT file is placed."""
        if not self.is_available:
            return
        if self._is_dwm_lut_running():
            return
        try:
            self.start_dwm_lut_gui()
        except DwmLutError:
            import logging
            logging.getLogger(__name__).warning(
                "DwmLutGUI not running. LUT file placed but not active. "
                "Start DwmLutGUI.exe manually as admin."
            )

    @property
    def is_available(self) -> bool:
        """Check if dwm_lut is available."""
        return self._dwm_lut_path is not None and (self._dwm_lut_path / "DwmLutGUI.exe").exists()

    @property
    def dwm_lut_exe(self) -> Optional[Path]:
        """Get path to DwmLutGUI.exe."""
        if self._dwm_lut_path:
            return self._dwm_lut_path / "DwmLutGUI.exe"
        return None

    def refresh_monitors(self) -> List[MonitorInfo]:
        """Refresh the list of connected monitors."""
        self._monitors = get_monitors()
        return self._monitors

    def get_monitors(self) -> List[MonitorInfo]:
        """Get list of connected monitors."""
        if not self._monitors:
            self.refresh_monitors()
        return self._monitors

    def get_monitor_by_index(self, index: int) -> Optional[MonitorInfo]:
        """Get monitor by index."""
        monitors = self.get_monitors()
        if 0 <= index < len(monitors):
            return monitors[index]
        return None

    def get_monitor_by_position(self, left: int, top: int) -> Optional[MonitorInfo]:
        """Get monitor by screen position."""
        for monitor in self.get_monitors():
            if monitor.left == left and monitor.top == top:
                return monitor
        return None

    def load_lut(
        self,
        monitor: Union[int, MonitorInfo],
        lut_data: np.ndarray,
        lut_type: LUTType = LUTType.SDR,
        title: str = "Calibration LUT"
    ) -> bool:
        """
        Load a 3D LUT for a specific monitor.

        Args:
            monitor: Monitor index or MonitorInfo object
            lut_data: 3D LUT array [size, size, size, 3] with values 0-1
            lut_type: SDR or HDR LUT
            title: LUT title for file header

        Returns:
            True if successful
        """
        # Get monitor info
        if isinstance(monitor, int):
            monitor_info = self.get_monitor_by_index(monitor)
            if not monitor_info:
                raise DwmLutError(f"Monitor index {monitor} not found")
        else:
            monitor_info = monitor

        # Validate LUT data
        if lut_data.ndim != 4 or lut_data.shape[3] != 3:
            raise DwmLutError("LUT must be [size, size, size, 3] array")

        size = lut_data.shape[0]
        if size not in [17, 33, 65]:
            raise DwmLutError(f"Unsupported LUT size: {size}. Use 17, 33, or 65.")

        # Get LUT filename and path
        filename = get_lut_filename(monitor_info, lut_type)
        lut_path = self._lut_directory / filename

        # Write LUT file
        try:
            write_cube_file(lut_path, lut_data, title)
        except PermissionError:
            raise DwmLutError(f"Permission denied writing to {lut_path}. Run as administrator.")
        except Exception as e:
            raise DwmLutError(f"Failed to write LUT file: {e}")

        # Track active LUT
        key = f"{monitor_info.left}_{monitor_info.top}_{lut_type.value}"
        self._active_luts[key] = DisplayLUTInfo(
            display_id=self._monitors.index(monitor_info) if monitor_info in self._monitors else 0,
            display_name=monitor_info.friendly_name,
            lut_active=True,
            lut_path=str(lut_path),
            lut_size=size,
            color_space=LUTColorSpace.HDR10 if lut_type == LUTType.HDR else LUTColorSpace.SRGB,
            lut_type=lut_type
        )

        # Ensure DwmLutGUI is running so the LUT is actually applied
        self._ensure_dwm_running()

        return True

    def load_lut_file(
        self,
        monitor: Union[int, MonitorInfo],
        source_path: Union[str, Path],
        lut_type: LUTType = LUTType.SDR
    ) -> bool:
        """
        Load a LUT from file for a specific monitor.

        Args:
            monitor: Monitor index or MonitorInfo object
            source_path: Path to .cube LUT file
            lut_type: SDR or HDR LUT

        Returns:
            True if successful
        """
        source_path = Path(source_path)
        if not source_path.exists():
            raise DwmLutError(f"LUT file not found: {source_path}")

        # Get monitor info
        if isinstance(monitor, int):
            monitor_info = self.get_monitor_by_index(monitor)
            if not monitor_info:
                raise DwmLutError(f"Monitor index {monitor} not found")
        else:
            monitor_info = monitor

        # Get destination filename and path
        filename = get_lut_filename(monitor_info, lut_type)
        dest_path = self._lut_directory / filename

        # Copy LUT file
        try:
            shutil.copy2(source_path, dest_path)
        except PermissionError:
            raise DwmLutError(f"Permission denied writing to {dest_path}. Run as administrator.")
        except Exception as e:
            raise DwmLutError(f"Failed to copy LUT file: {e}")

        # Read LUT to get size
        lut_data, size = read_cube_file(source_path)

        # Track active LUT
        key = f"{monitor_info.left}_{monitor_info.top}_{lut_type.value}"
        self._active_luts[key] = DisplayLUTInfo(
            display_id=self._monitors.index(monitor_info) if monitor_info in self._monitors else 0,
            display_name=monitor_info.friendly_name,
            lut_active=True,
            lut_path=str(dest_path),
            lut_size=size,
            color_space=LUTColorSpace.HDR10 if lut_type == LUTType.HDR else LUTColorSpace.SRGB,
            lut_type=lut_type
        )

        # Ensure DwmLutGUI is running so the LUT is actually applied
        self._ensure_dwm_running()

        return True

    def unload_lut(
        self,
        monitor: Union[int, MonitorInfo],
        lut_type: LUTType = LUTType.SDR
    ) -> bool:
        """
        Remove LUT from a monitor (restore identity).

        Args:
            monitor: Monitor index or MonitorInfo object
            lut_type: SDR or HDR LUT to remove

        Returns:
            True if successful
        """
        # Get monitor info
        if isinstance(monitor, int):
            monitor_info = self.get_monitor_by_index(monitor)
            if not monitor_info:
                raise DwmLutError(f"Monitor index {monitor} not found")
        else:
            monitor_info = monitor

        # Get LUT filename and path
        filename = get_lut_filename(monitor_info, lut_type)
        lut_path = self._lut_directory / filename

        # Remove LUT file
        try:
            if lut_path.exists():
                lut_path.unlink()
        except PermissionError:
            raise DwmLutError(f"Permission denied removing {lut_path}. Run as administrator.")
        except Exception as e:
            raise DwmLutError(f"Failed to remove LUT file: {e}")

        # Update tracking
        key = f"{monitor_info.left}_{monitor_info.top}_{lut_type.value}"
        if key in self._active_luts:
            del self._active_luts[key]

        return True

    def reset_all(self) -> bool:
        """Remove all LUTs from all monitors."""
        success = True

        # Remove all LUT files
        try:
            if self._lut_directory.exists():
                for lut_file in self._lut_directory.glob("*.cube"):
                    try:
                        lut_file.unlink()
                    except Exception:
                        success = False
        except Exception:
            success = False

        self._active_luts.clear()
        return success

    def get_active_luts(self) -> Dict[str, DisplayLUTInfo]:
        """Get information about active LUTs."""
        return self._active_luts.copy()

    def start_dwm_lut_gui(self) -> bool:
        """
        Start DwmLutGUI.exe to enable LUT application.

        DwmLutGUI must be running for LUTs to be applied. It requires
        admin elevation, so we use ShellExecuteW with "runas" verb.

        Returns:
            True if started successfully
        """
        if not self.is_available:
            raise DwmLutError("dwm_lut not found. Please install from https://github.com/ledoge/dwm_lut")

        try:
            # Check if already running
            if self._is_dwm_lut_running():
                return True

            # Try elevated launch via ShellExecuteW (UAC prompt)
            import logging
            logger = logging.getLogger(__name__)

            try:
                result = ctypes.windll.shell32.ShellExecuteW(
                    None, "runas", str(self.dwm_lut_exe), "",
                    str(self._dwm_lut_path), 0  # SW_HIDE
                )
                # ShellExecuteW returns > 32 on success
                if result > 32:
                    time.sleep(2)
                    if self._is_dwm_lut_running():
                        logger.info("DwmLutGUI started with elevation")
                        return True
            except Exception as e:
                logger.debug("Elevated launch failed: %s", e)

            # Fallback: try without elevation (works if already admin)
            try:
                subprocess.Popen(
                    [str(self.dwm_lut_exe)],
                    cwd=str(self._dwm_lut_path),
                    creationflags=subprocess.CREATE_NO_WINDOW
                )
                time.sleep(1)
                if self._is_dwm_lut_running():
                    return True
            except Exception as e:
                logger.debug("Non-elevated launch failed: %s", e)

            logger.warning("Could not start DwmLutGUI. LUT file placed but not active.")
            return False

        except Exception as e:
            raise DwmLutError(f"Failed to start DwmLutGUI: {e}")

    def stop_dwm_lut_gui(self) -> bool:
        """Stop DwmLutGUI.exe."""
        try:
            subprocess.run(
                ["taskkill", "/F", "/IM", "DwmLutGUI.exe"],
                capture_output=True,
                creationflags=subprocess.CREATE_NO_WINDOW
            )
            return True
        except Exception:
            return False

    def _is_dwm_lut_running(self) -> bool:
        """Check if DwmLutGUI is running."""
        try:
            result = subprocess.run(
                ["tasklist", "/FI", "IMAGENAME eq DwmLutGUI.exe"],
                capture_output=True,
                text=True,
                creationflags=subprocess.CREATE_NO_WINDOW
            )
            return "DwmLutGUI.exe" in result.stdout
        except Exception:
            return False

    def apply_hdr_calibration(
        self,
        monitor: Union[int, MonitorInfo],
        rgb_gains: Tuple[float, float, float] = (1.0, 1.0, 1.0),
        rgb_offsets: Tuple[float, float, float] = (0.0, 0.0, 0.0),
        whitepoint: Tuple[float, float, float] = (1.0, 1.0, 1.0),
        peak_luminance: float = 1000.0,
        lut_size: int = 33
    ) -> bool:
        """
        Apply HDR calibration to a monitor.

        Generates and loads an HDR calibration LUT with the specified parameters.

        Args:
            monitor: Monitor index or MonitorInfo object
            rgb_gains: Per-channel gain adjustments (1.0 = no change)
            rgb_offsets: Per-channel offset adjustments (0.0 = no change)
            whitepoint: RGB multipliers for white point correction
            peak_luminance: Display peak luminance in nits
            lut_size: LUT size (17, 33, or 65)

        Returns:
            True if successful
        """
        # Generate HDR calibration LUT
        lut = generate_hdr_calibration_lut(
            size=lut_size,
            target_whitepoint=whitepoint,
            rgb_gains=rgb_gains,
            rgb_offsets=rgb_offsets,
            peak_luminance=peak_luminance
        )

        # Load LUT
        return self.load_lut(monitor, lut, LUTType.HDR, "HDR Calibration LUT")

    def apply_sdr_calibration(
        self,
        monitor: Union[int, MonitorInfo],
        rgb_gains: Tuple[float, float, float] = (1.0, 1.0, 1.0),
        rgb_offsets: Tuple[float, float, float] = (0.0, 0.0, 0.0),
        whitepoint: Tuple[float, float, float] = (1.0, 1.0, 1.0),
        target_gamma: float = 2.2,
        lut_size: int = 33
    ) -> bool:
        """
        Apply SDR calibration to a monitor.

        Generates and loads an SDR calibration LUT with the specified parameters.

        Args:
            monitor: Monitor index or MonitorInfo object
            rgb_gains: Per-channel gain adjustments (1.0 = no change)
            rgb_offsets: Per-channel offset adjustments (0.0 = no change)
            whitepoint: RGB multipliers for white point correction
            target_gamma: Target display gamma
            lut_size: LUT size (17, 33, or 65)

        Returns:
            True if successful
        """
        # Generate SDR calibration LUT
        lut = generate_sdr_calibration_lut(
            size=lut_size,
            target_gamma=target_gamma,
            target_whitepoint=whitepoint,
            rgb_gains=rgb_gains,
            rgb_offsets=rgb_offsets
        )

        # Load LUT
        return self.load_lut(monitor, lut, LUTType.SDR, "SDR Calibration LUT")


# =============================================================================
# Convenience Functions
# =============================================================================

def apply_lut(lut_path: Union[str, Path], monitor_index: int = 0, lut_type: str = "sdr") -> bool:
    """
    Quick function to apply a LUT file to a monitor.

    Args:
        lut_path: Path to .cube LUT file
        monitor_index: Monitor index (0 = primary)
        lut_type: "sdr" or "hdr"

    Returns:
        True if successful
    """
    controller = DwmLutController()
    lt = LUTType.HDR if lut_type.lower() == "hdr" else LUTType.SDR
    return controller.load_lut_file(monitor_index, lut_path, lt)


def remove_lut(monitor_index: int = 0, lut_type: str = "sdr") -> bool:
    """
    Quick function to remove LUT from a monitor.

    Args:
        monitor_index: Monitor index (0 = primary)
        lut_type: "sdr" or "hdr"

    Returns:
        True if successful
    """
    controller = DwmLutController()
    lt = LUTType.HDR if lut_type.lower() == "hdr" else LUTType.SDR
    return controller.unload_lut(monitor_index, lt)


def reset_all_luts() -> bool:
    """Reset all displays to no LUT."""
    controller = DwmLutController()
    return controller.reset_all()


def get_lut_status() -> Dict:
    """
    Get the current status of the DWM LUT system.

    Returns a dict with:
    - available: bool - whether dwm_lut is installed
    - running: bool - whether DwmLutGUI is running
    - lut_dir: str - path to LUT directory
    - active_luts: list - names of active LUT files
    - monitors: list - monitor info
    """
    controller = DwmLutController()
    lut_dir = get_dwm_lut_directory()
    active = []
    if lut_dir.exists():
        active = [f.name for f in lut_dir.glob("*.cube")]

    return {
        "available": controller.is_available,
        "running": controller._is_dwm_lut_running(),
        "lut_dir": str(lut_dir),
        "active_luts": active,
        "monitors": list_monitors(),
        "exe_path": str(controller.dwm_lut_exe) if controller.dwm_lut_exe else None,
    }


def list_monitors() -> List[Dict]:
    """List all connected monitors with position information."""
    monitors = get_monitors()
    return [
        {
            "index": i,
            "name": m.device_name,
            "friendly_name": m.friendly_name,
            "position": (m.left, m.top),
            "size": (m.width, m.height),
            "is_primary": m.is_primary,
            "is_hdr": m.is_hdr,
            "sdr_lut_name": get_lut_filename(m, LUTType.SDR),
            "hdr_lut_name": get_lut_filename(m, LUTType.HDR),
        }
        for i, m in enumerate(monitors)
    ]


# =============================================================================
# CLI Interface
# =============================================================================

if __name__ == "__main__":
    import sys

    print("DWM 3D LUT Controller")
    print("=" * 50)

    # List monitors
    print("\nConnected Monitors:")
    for monitor in list_monitors():
        print(f"  [{monitor['index']}] {monitor['friendly_name']}")
        print(f"      Position: {monitor['position']}")
        print(f"      Size: {monitor['size']}")
        print(f"      Primary: {monitor['is_primary']}")
        print(f"      HDR: {monitor['is_hdr']}")
        print(f"      SDR LUT: {monitor['sdr_lut_name']}")
        print(f"      HDR LUT: {monitor['hdr_lut_name']}")

    # Check dwm_lut installation
    controller = DwmLutController()
    print(f"\ndwm_lut available: {controller.is_available}")
    if controller.dwm_lut_exe:
        print(f"dwm_lut path: {controller.dwm_lut_exe}")

    # Show LUT directory
    print(f"\nLUT directory: {get_dwm_lut_directory()}")
