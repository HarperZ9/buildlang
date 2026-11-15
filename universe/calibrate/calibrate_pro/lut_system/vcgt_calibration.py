"""
VCGT (Video Card Gamma Table) Calibration

For monitors that don't support hardware DDC/CI RGB gain control,
we apply color correction through the graphics card's gamma ramps.

This is the method used by DisplayCAL, ArgyllCMS, and other professional
calibration tools.
"""

import ctypes
from ctypes import wintypes
import numpy as np
from typing import Tuple, Optional, List
from dataclasses import dataclass
from pathlib import Path


# Windows GDI32 functions for gamma ramp control
try:
    gdi32 = ctypes.windll.gdi32
    user32 = ctypes.windll.user32

    # CreateDC / DeleteDC - Required for gamma ramp access
    gdi32.CreateDCW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.LPCWSTR, ctypes.c_void_p]
    gdi32.CreateDCW.restype = wintypes.HDC
    gdi32.DeleteDC.argtypes = [wintypes.HDC]
    gdi32.DeleteDC.restype = wintypes.BOOL

    # GetDeviceGammaRamp / SetDeviceGammaRamp
    gdi32.GetDeviceGammaRamp.argtypes = [wintypes.HDC, ctypes.c_void_p]
    gdi32.GetDeviceGammaRamp.restype = wintypes.BOOL
    gdi32.SetDeviceGammaRamp.argtypes = [wintypes.HDC, ctypes.c_void_p]
    gdi32.SetDeviceGammaRamp.restype = wintypes.BOOL

    # EnumDisplayDevices for getting display names
    class DISPLAY_DEVICE(ctypes.Structure):
        _fields_ = [
            ('cb', wintypes.DWORD),
            ('DeviceName', wintypes.WCHAR * 32),
            ('DeviceString', wintypes.WCHAR * 128),
            ('StateFlags', wintypes.DWORD),
            ('DeviceID', wintypes.WCHAR * 128),
            ('DeviceKey', wintypes.WCHAR * 128),
        ]

    user32.EnumDisplayDevicesW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(DISPLAY_DEVICE), wintypes.DWORD]
    user32.EnumDisplayDevicesW.restype = wintypes.BOOL

    VCGT_AVAILABLE = True
except Exception:
    VCGT_AVAILABLE = False


def get_display_devices() -> List[Tuple[int, str, str]]:
    """Get list of active display devices.

    Returns:
        List of (index, device_name, device_string) tuples
    """
    if not VCGT_AVAILABLE:
        return []

    devices = []
    i = 0
    while True:
        dd = DISPLAY_DEVICE()
        dd.cb = ctypes.sizeof(dd)
        if not user32.EnumDisplayDevicesW(None, i, ctypes.byref(dd), 0):
            break

        # Check if display is active (DISPLAY_DEVICE_ACTIVE = 1)
        if dd.StateFlags & 1:
            devices.append((i, dd.DeviceName, dd.DeviceString))
        i += 1

    return devices


def _get_display_dc(display_index: int = 0) -> Optional[wintypes.HDC]:
    """Get device context for a specific display.

    Args:
        display_index: Index of display (0 = primary)

    Returns:
        HDC handle or None if failed
    """
    devices = get_display_devices()
    if display_index >= len(devices):
        display_index = 0

    if not devices:
        return None

    device_name = devices[display_index][1]
    return gdi32.CreateDCW(device_name, device_name, None, None)


def _release_display_dc(hdc: wintypes.HDC) -> None:
    """Release a display device context."""
    if hdc:
        gdi32.DeleteDC(hdc)


# Gamma ramp structure: 3 channels x 256 entries x 16-bit values
GammaRamp = (ctypes.c_ushort * 256) * 3


@dataclass
class CalibrationCurves:
    """RGB calibration curves (256 points each, 0.0-1.0 range)."""
    red: np.ndarray
    green: np.ndarray
    blue: np.ndarray

    def to_gamma_ramp(self) -> GammaRamp:
        """Convert to Windows gamma ramp structure."""
        ramp = GammaRamp()
        for i in range(256):
            ramp[0][i] = int(np.clip(self.red[i] * 65535, 0, 65535))
            ramp[1][i] = int(np.clip(self.green[i] * 65535, 0, 65535))
            ramp[2][i] = int(np.clip(self.blue[i] * 65535, 0, 65535))
        return ramp


def get_current_gamma_ramp(display_index: int = 0) -> Optional[Tuple[np.ndarray, np.ndarray, np.ndarray]]:
    """Get the current gamma ramp from the display.

    Args:
        display_index: Index of display (0 = primary)

    Returns:
        Tuple of (red, green, blue) arrays or None if failed
    """
    if not VCGT_AVAILABLE:
        return None

    hdc = _get_display_dc(display_index)
    if not hdc:
        return None

    try:
        ramp = GammaRamp()
        if gdi32.GetDeviceGammaRamp(hdc, ctypes.byref(ramp)):
            red = np.array([ramp[0][i] / 65535.0 for i in range(256)])
            green = np.array([ramp[1][i] / 65535.0 for i in range(256)])
            blue = np.array([ramp[2][i] / 65535.0 for i in range(256)])
            return (red, green, blue)
        return None
    finally:
        _release_display_dc(hdc)


def set_gamma_ramp(curves: CalibrationCurves, display_index: int = 0) -> bool:
    """Apply gamma ramp to the display.

    Args:
        curves: CalibrationCurves to apply
        display_index: Index of display (0 = primary)

    Returns:
        True if successful
    """
    if not VCGT_AVAILABLE:
        return False

    hdc = _get_display_dc(display_index)
    if not hdc:
        return False

    try:
        ramp = curves.to_gamma_ramp()
        return bool(gdi32.SetDeviceGammaRamp(hdc, ctypes.byref(ramp)))
    finally:
        _release_display_dc(hdc)


def reset_gamma_ramp() -> bool:
    """Reset gamma ramp to linear (no correction)."""
    linear = CalibrationCurves(
        red=np.linspace(0, 1, 256),
        green=np.linspace(0, 1, 256),
        blue=np.linspace(0, 1, 256),
    )
    return set_gamma_ramp(linear)


def create_white_balance_curves(
    red_gain: float = 1.0,
    green_gain: float = 1.0,
    blue_gain: float = 1.0,
    gamma: float = 2.2,
    target_gamma: float = 2.2,
) -> CalibrationCurves:
    """
    Create calibration curves for white balance adjustment.

    Args:
        red_gain: Red channel multiplier (0.5-1.5 typical)
        green_gain: Green channel multiplier
        blue_gain: Blue channel multiplier
        gamma: Native display gamma
        target_gamma: Target gamma (typically 2.2)

    Returns:
        CalibrationCurves ready to apply
    """
    x = np.linspace(0, 1, 256)

    # Apply gamma correction and gain
    # Input -> Linear -> Apply gain -> Target gamma -> Output
    def create_curve(gain: float, native_gamma: float, target_gamma: float) -> np.ndarray:
        # Linearize input (undo native gamma)
        linear = np.power(x, native_gamma)
        # Apply gain
        linear = linear * gain
        # Clip to valid range
        linear = np.clip(linear, 0, 1)
        # Apply target gamma
        output = np.power(linear, 1.0 / target_gamma)
        return output

    return CalibrationCurves(
        red=create_curve(red_gain, gamma, target_gamma),
        green=create_curve(green_gain, gamma, target_gamma),
        blue=create_curve(blue_gain, gamma, target_gamma),
    )


def create_whitepoint_correction_curves(
    native_white_xy: Tuple[float, float],
    target_white_xy: Tuple[float, float],
    native_gamma: float = 2.2,
    target_gamma: float = 2.2,
) -> CalibrationCurves:
    """
    Create calibration curves to correct white point.

    Uses Bradford chromatic adaptation to calculate the required
    RGB channel gains.
    """
    # Import color math
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from calibrate_pro.hardware.sensorless_calibration import (
        xy_to_XYZ, bradford_adapt, BRADFORD_M, BRADFORD_M_INV
    )

    # Convert xy to XYZ
    native_XYZ = xy_to_XYZ(*native_white_xy)
    target_XYZ = xy_to_XYZ(*target_white_xy)

    # Calculate adaptation matrix using Bradford transform
    cone_native = BRADFORD_M @ native_XYZ
    cone_target = BRADFORD_M @ target_XYZ

    # Diagonal scaling in cone space
    scale = cone_target / cone_native

    # For a simple white point shift, we can approximate with RGB gains
    # The blue channel needs the biggest adjustment typically
    rgb_gains = scale / np.max(scale)  # Normalize to max = 1

    return create_white_balance_curves(
        red_gain=rgb_gains[0],
        green_gain=rgb_gains[1],
        blue_gain=rgb_gains[2],
        gamma=native_gamma,
        target_gamma=target_gamma,
    )


class VCGTCalibrator:
    """
    Video Card Gamma Table calibrator for software-side color correction.

    This applies color correction through the graphics card's LUT,
    which works on all monitors regardless of DDC/CI support.
    """

    def __init__(self, display_index: int = 0):
        """Initialize calibrator for a specific display.

        Args:
            display_index: Index of display (0 = primary)
        """
        self._display_index = display_index
        self._original_ramp = None
        self._current_curves = None
        self._backup_saved = False

    @property
    def available(self) -> bool:
        """Check if VCGT control is available."""
        return VCGT_AVAILABLE

    @staticmethod
    def get_displays() -> List[Tuple[int, str, str]]:
        """Get list of available displays.

        Returns:
            List of (index, device_name, description) tuples
        """
        return get_display_devices()

    def backup_current_ramp(self) -> bool:
        """Save the current gamma ramp for later restoration."""
        ramp = get_current_gamma_ramp(self._display_index)
        if ramp:
            self._original_ramp = ramp
            self._backup_saved = True
            return True
        return False

    def restore_original_ramp(self) -> bool:
        """Restore the backed up gamma ramp."""
        if self._original_ramp:
            curves = CalibrationCurves(
                red=self._original_ramp[0],
                green=self._original_ramp[1],
                blue=self._original_ramp[2],
            )
            return set_gamma_ramp(curves, self._display_index)
        return False

    def reset_to_linear(self) -> bool:
        """Reset to linear (uncorrected) gamma."""
        linear = CalibrationCurves(
            red=np.linspace(0, 1, 256),
            green=np.linspace(0, 1, 256),
            blue=np.linspace(0, 1, 256),
        )
        return set_gamma_ramp(linear, self._display_index)

    def apply_white_balance(
        self,
        red_gain: float = 1.0,
        green_gain: float = 1.0,
        blue_gain: float = 1.0,
        gamma: float = 2.2,
    ) -> bool:
        """
        Apply white balance correction.

        Args:
            red_gain: Red multiplier (< 1.0 reduces red)
            green_gain: Green multiplier
            blue_gain: Blue multiplier
            gamma: Target gamma

        Returns:
            True if successful
        """
        curves = create_white_balance_curves(
            red_gain=red_gain,
            green_gain=green_gain,
            blue_gain=blue_gain,
            gamma=gamma,
            target_gamma=gamma,
        )
        self._current_curves = curves
        return set_gamma_ramp(curves, self._display_index)

    def calibrate_to_target(
        self,
        target_whitepoint: str = "D65",
        target_gamma: float = 2.2,
        native_white_x: float = 0.3127,
        native_white_y: float = 0.3290,
        native_gamma: float = 2.2,
    ) -> bool:
        """
        Calibrate display to target white point and gamma.

        Args:
            target_whitepoint: Target white point (D50, D55, D65, D75)
            target_gamma: Target gamma value
            native_white_x: Display's native white x chromaticity
            native_white_y: Display's native white y chromaticity
            native_gamma: Display's native gamma

        Returns:
            True if calibration applied successfully
        """
        from calibrate_pro.hardware.sensorless_calibration import ILLUMINANTS

        target_xy = ILLUMINANTS.get(target_whitepoint, ILLUMINANTS["D65"])
        native_xy = (native_white_x, native_white_y)

        curves = create_whitepoint_correction_curves(
            native_white_xy=native_xy,
            target_white_xy=target_xy,
            native_gamma=native_gamma,
            target_gamma=target_gamma,
        )

        self._current_curves = curves
        return set_gamma_ramp(curves, self._display_index)

    def get_current_curves(self) -> Optional[Tuple[np.ndarray, np.ndarray, np.ndarray]]:
        """Get the currently applied gamma curves."""
        return get_current_gamma_ramp(self._display_index)


def quick_calibrate(
    whitepoint: str = "D65",
    gamma: float = 2.2,
    panel_model: str = "PG27UCDM",
) -> bool:
    """
    Quick one-shot calibration function.

    Args:
        whitepoint: Target white point (D50, D55, D65, D75)
        gamma: Target gamma
        panel_model: Panel model for native characteristics lookup

    Returns:
        True if calibration successful
    """
    # Get panel data
    from calibrate_pro.panels.database import get_database

    db = get_database()
    panel = db.find_panel(panel_model)

    if panel:
        native_x = panel.native_primaries.white.x
        native_y = panel.native_primaries.white.y
        native_gamma = panel.gamma_red.gamma  # Use red channel gamma
    else:
        # Fallback to sRGB
        native_x = 0.3127
        native_y = 0.3290
        native_gamma = 2.2

    calibrator = VCGTCalibrator()
    return calibrator.calibrate_to_target(
        target_whitepoint=whitepoint,
        target_gamma=gamma,
        native_white_x=native_x,
        native_white_y=native_y,
        native_gamma=native_gamma,
    )


if __name__ == "__main__":
    print("VCGT Calibration Test")
    print("=" * 50)
    print(f"VCGT Available: {VCGT_AVAILABLE}")

    if VCGT_AVAILABLE:
        # Get current ramp
        current = get_current_gamma_ramp()
        if current:
            print(f"Current ramp - Red[128]: {current[0][128]:.4f}")
            print(f"Current ramp - Green[128]: {current[1][128]:.4f}")
            print(f"Current ramp - Blue[128]: {current[2][128]:.4f}")
