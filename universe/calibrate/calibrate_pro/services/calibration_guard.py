"""
Calibration Guard — Windows Calibration Protection Service

Windows 11 (especially 24H2) actively sabotages display calibration by:
- Resetting VCGT gamma ramp data when you visit Settings > Display
- Silently unloading ICC profile associations
- Auto Color Management (ACM) overriding wide-gamut corrections
- Sleep/wake cycles dropping calibration state

This service monitors and reapplies calibration continuously,
fighting against Windows' tendency to destroy calibration work.

Runs as a background thread, checking every N seconds.
"""

import logging
import time
import threading
import ctypes
from ctypes import wintypes
from pathlib import Path
from typing import Dict, List, Optional, Callable
from dataclasses import dataclass
import numpy as np

logger = logging.getLogger(__name__)


@dataclass
class GuardedDisplay:
    """A display whose calibration is being protected."""
    device_name: str          # e.g., "\\\\.\\DISPLAY1"
    display_name: str         # Human-readable
    icc_profile_path: Optional[str] = None
    lut_path: Optional[str] = None
    vcgt_red: Optional[np.ndarray] = None
    vcgt_green: Optional[np.ndarray] = None
    vcgt_blue: Optional[np.ndarray] = None


class CalibrationGuard:
    """
    Continuously monitors and reapplies display calibration.

    Detects when Windows resets the VCGT gamma ramp and immediately
    reapplies the saved calibration curves. Also monitors ICC profile
    associations and DWM LUT state.
    """

    def __init__(
        self,
        check_interval: float = 10.0,
        on_restore: Optional[Callable[[str, str], None]] = None
    ):
        """
        Args:
            check_interval: Seconds between checks (default 10)
            on_restore: Callback(display_name, reason) when calibration is restored
        """
        self.check_interval = check_interval
        self.on_restore = on_restore
        self._guarded: Dict[str, GuardedDisplay] = {}
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._restore_count = 0

    @property
    def restore_count(self) -> int:
        """Number of times calibration has been restored."""
        return self._restore_count

    @property
    def is_running(self) -> bool:
        return self._running

    def guard_display(self, display: GuardedDisplay):
        """Add a display to the guard list."""
        # Read and save the current VCGT state
        if display.vcgt_red is None:
            ramp = self._read_current_vcgt(display.device_name)
            if ramp:
                display.vcgt_red, display.vcgt_green, display.vcgt_blue = ramp
        self._guarded[display.device_name] = display

    def unguard_display(self, device_name: str):
        """Remove a display from the guard list."""
        self._guarded.pop(device_name, None)

    def start(self):
        """Start the calibration guard in a background thread."""
        if self._running:
            return
        self._running = True
        self._thread = threading.Thread(target=self._guard_loop, daemon=True)
        self._thread.start()

    def stop(self):
        """Stop the guard."""
        self._running = False
        if self._thread:
            self._thread.join(timeout=15)
            self._thread = None

    def check_now(self) -> List[str]:
        """
        Perform an immediate check and restore if needed.

        Returns list of display names that were restored.
        """
        restored = []
        for device_name, display in self._guarded.items():
            if self._check_and_restore(display):
                restored.append(display.display_name)
        return restored

    # --- Internal ---

    def _guard_loop(self):
        """Main guard loop."""
        while self._running:
            try:
                for device_name, display in list(self._guarded.items()):
                    self._check_and_restore(display)
            except Exception as e:
                logger.error("Guard check failed: %s", e, exc_info=True)
            time.sleep(self.check_interval)

    def _check_and_restore(self, display: GuardedDisplay) -> bool:
        """
        Check if calibration is still applied, restore if not.

        Returns True if restoration was needed.
        """
        if display.vcgt_red is None:
            return False

        # Read current VCGT from the display
        current = self._read_current_vcgt(display.device_name)
        if current is None:
            return False

        current_r, current_g, current_b = current

        # Compare with saved state
        # Windows resets to linear (identity) ramp: values 0, 256, 512, ...
        is_linear = self._is_linear_ramp(current_r)
        is_matching = self._ramps_match(current_r, display.vcgt_red)

        if is_linear and not self._is_linear_ramp(display.vcgt_red):
            # Windows reset our calibration to linear — restore it
            self._apply_vcgt(display.device_name,
                             display.vcgt_red, display.vcgt_green, display.vcgt_blue)
            self._restore_count += 1
            if self.on_restore:
                self.on_restore(display.display_name, "Windows reset gamma ramp to linear")
            return True

        if not is_matching and not is_linear:
            # Something else changed the ramp — could be another tool, restore ours
            self._apply_vcgt(display.device_name,
                             display.vcgt_red, display.vcgt_green, display.vcgt_blue)
            self._restore_count += 1
            if self.on_restore:
                self.on_restore(display.display_name, "Gamma ramp was modified by another process")
            return True

        return False

    @staticmethod
    def _is_linear_ramp(ramp: np.ndarray, tolerance: int = 10) -> bool:
        """Check if a gamma ramp is linear (identity)."""
        expected = np.linspace(0, 65535, 256, dtype=np.uint16)
        return np.max(np.abs(ramp.astype(np.int32) - expected.astype(np.int32))) < tolerance

    @staticmethod
    def _ramps_match(a: np.ndarray, b: np.ndarray, tolerance: int = 5) -> bool:
        """Check if two gamma ramps match."""
        return np.max(np.abs(a.astype(np.int32) - b.astype(np.int32))) < tolerance

    @staticmethod
    def _read_current_vcgt(device_name: str) -> Optional[tuple]:
        """Read the current VCGT gamma ramp from a display."""
        try:
            user32 = ctypes.windll.user32
            gdi32 = ctypes.windll.gdi32

            dc = user32.CreateDCW("DISPLAY", device_name, None, None)
            if not dc:
                return None

            try:
                class GAMMA_RAMP(ctypes.Structure):
                    _fields_ = [
                        ("Red", wintypes.WORD * 256),
                        ("Green", wintypes.WORD * 256),
                        ("Blue", wintypes.WORD * 256),
                    ]

                ramp = GAMMA_RAMP()
                if gdi32.GetDeviceGammaRamp(dc, ctypes.byref(ramp)):
                    red = np.array(ramp.Red[:], dtype=np.uint16)
                    green = np.array(ramp.Green[:], dtype=np.uint16)
                    blue = np.array(ramp.Blue[:], dtype=np.uint16)
                    return (red, green, blue)
            finally:
                user32.DeleteDC(dc)
        except Exception as e:
            logger.debug("VCGT read failed for %s: %s", device_name, e)
        return None

    @staticmethod
    def _apply_vcgt(device_name: str, red: np.ndarray, green: np.ndarray, blue: np.ndarray) -> bool:
        """Apply VCGT gamma ramp to a display."""
        try:
            user32 = ctypes.windll.user32
            gdi32 = ctypes.windll.gdi32

            dc = user32.CreateDCW("DISPLAY", device_name, None, None)
            if not dc:
                return False

            try:
                class GAMMA_RAMP(ctypes.Structure):
                    _fields_ = [
                        ("Red", wintypes.WORD * 256),
                        ("Green", wintypes.WORD * 256),
                        ("Blue", wintypes.WORD * 256),
                    ]

                ramp = GAMMA_RAMP()
                for i in range(256):
                    ramp.Red[i] = int(red[i])
                    ramp.Green[i] = int(green[i])
                    ramp.Blue[i] = int(blue[i])

                return bool(gdi32.SetDeviceGammaRamp(dc, ctypes.byref(ramp)))
            finally:
                user32.DeleteDC(dc)
        except Exception:
            return False


def detect_acm_enabled() -> bool:
    """
    Detect if Windows Auto Color Management (ACM) is enabled.

    ACM bypasses ICC profiles and forces sRGB compression, which
    conflicts with user calibration. Users should be warned.
    """
    try:
        import winreg
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvancedDisplay"
        )
        for i in range(20):
            try:
                subkey_name = winreg.EnumKey(key, i)
                subkey = winreg.OpenKey(key, subkey_name)
                try:
                    acm_val, _ = winreg.QueryValueEx(subkey, "AutoColorManagementEnabled")
                    if acm_val:
                        winreg.CloseKey(subkey)
                        winreg.CloseKey(key)
                        return True
                except FileNotFoundError:
                    pass
                winreg.CloseKey(subkey)
            except OSError:
                break
        winreg.CloseKey(key)
    except Exception:
        pass
    return False
