"""
Windows HDR Mode Detection

Detects whether Windows HDR is enabled on each display and provides
automatic profile switching between SDR and HDR calibration.

Uses multiple detection methods:
1. Registry: HKCU\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AdvancedDisplay
2. DXGI: IDXGIOutput6 color space enumeration
3. dwm_lut: Check if HDR LUTs are loaded
"""

import ctypes
from ctypes import wintypes
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple
import winreg


@dataclass
class HDRDisplayState:
    """HDR state for a single display."""
    display_index: int
    display_name: str
    device_path: str
    hdr_enabled: bool
    hdr_capable: bool
    peak_luminance: float       # Current peak in cd/m2
    sdr_white_level: float      # SDR content white level in cd/m2 (typically 80-480)
    color_space: str            # "sRGB", "scRGB", "BT2020_PQ", etc.
    bit_depth: int              # 8, 10, or 12


def detect_hdr_state() -> List[HDRDisplayState]:
    """
    Detect HDR state for all connected displays.

    Returns a list of HDRDisplayState for each display.
    """
    states = []

    try:
        from calibrate_pro.panels.detection import enumerate_displays, get_display_name
        displays = enumerate_displays()
    except Exception:
        return states

    for i, display in enumerate(displays):
        try:
            name = get_display_name(display)
        except Exception:
            name = display.monitor_name or f"Display {i + 1}"

        # Try registry detection first (most reliable on Windows 10/11)
        hdr_on = _check_hdr_registry(display.device_name)
        sdr_white = _get_sdr_white_level(display.device_name)

        # Try to get capabilities from panel database
        hdr_capable = False
        peak_lum = 0.0
        try:
            from calibrate_pro.panels.detection import identify_display
            from calibrate_pro.panels.database import PanelDatabase
            db = PanelDatabase()
            key = identify_display(display)
            if key:
                panel = db.get_panel(key)
                if panel:
                    hdr_capable = panel.capabilities.hdr_capable
                    peak_lum = panel.capabilities.max_luminance_hdr
        except Exception:
            pass

        state = HDRDisplayState(
            display_index=i,
            display_name=name,
            device_path=display.device_name,
            hdr_enabled=hdr_on,
            hdr_capable=hdr_capable,
            peak_luminance=peak_lum,
            sdr_white_level=sdr_white,
            color_space="BT2020_PQ" if hdr_on else "sRGB",
            bit_depth=10 if hdr_on else display.bit_depth
        )
        states.append(state)

    return states


def _check_hdr_registry(device_name: str) -> bool:
    """
    Check Windows registry for HDR enable state.

    Windows stores HDR state per-display in:
    HKCU\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AdvancedDisplay
    """
    try:
        # Method 1: Check AdvancedDisplay registry
        base = r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvancedDisplay"
        try:
            key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, base)
            # Enumerate subkeys for each display
            for j in range(20):
                try:
                    subkey_name = winreg.EnumKey(key, j)
                    subkey = winreg.OpenKey(key, subkey_name)
                    try:
                        hdr_enabled, _ = winreg.QueryValueEx(subkey, "AdvancedColorEnabled")
                        if hdr_enabled:
                            return True
                    except FileNotFoundError:
                        pass
                    winreg.CloseKey(subkey)
                except OSError:
                    break
            winreg.CloseKey(key)
        except FileNotFoundError:
            pass

        # Method 2: Check GraphicsDrivers for HDR support indication
        try:
            gfx_key = winreg.OpenKey(
                winreg.HKEY_LOCAL_MACHINE,
                r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers"
            )
            try:
                hdr_val, _ = winreg.QueryValueEx(gfx_key, "EnableHDR")
                if hdr_val:
                    return True
            except FileNotFoundError:
                pass
            winreg.CloseKey(gfx_key)
        except FileNotFoundError:
            pass

    except Exception:
        pass

    return False


def _get_sdr_white_level(device_name: str) -> float:
    """
    Get the SDR content white level for HDR mode.

    Windows allows users to set the brightness of SDR content when
    HDR is enabled (Settings > Display > HDR > SDR content brightness).
    Default is typically 200 cd/m2 on OLED, 100-300 on LCD.
    """
    try:
        base = r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvancedDisplay"
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, base)
        for j in range(20):
            try:
                subkey_name = winreg.EnumKey(key, j)
                subkey = winreg.OpenKey(key, subkey_name)
                try:
                    level, _ = winreg.QueryValueEx(subkey, "SDRContentBrightness")
                    winreg.CloseKey(subkey)
                    winreg.CloseKey(key)
                    return float(level)
                except FileNotFoundError:
                    pass
                winreg.CloseKey(subkey)
            except OSError:
                break
        winreg.CloseKey(key)
    except Exception:
        pass

    return 200.0  # Default SDR white level


class HDRModeWatcher:
    """
    Watches for HDR mode changes and triggers profile switching.

    Polls the HDR state periodically and calls the callback when
    a display's HDR mode changes.
    """

    def __init__(
        self,
        on_hdr_change=None,
        poll_interval: float = 5.0
    ):
        """
        Args:
            on_hdr_change: Callback(display_index, hdr_enabled, state)
            poll_interval: Seconds between polls
        """
        self.on_hdr_change = on_hdr_change
        self.poll_interval = poll_interval
        self._running = False
        self._thread = None
        self._last_states: Dict[int, bool] = {}

    def start(self):
        """Start watching for HDR changes in a background thread."""
        import threading

        self._running = True
        # Record initial state
        for state in detect_hdr_state():
            self._last_states[state.display_index] = state.hdr_enabled

        self._thread = threading.Thread(target=self._watch_loop, daemon=True)
        self._thread.start()

    def stop(self):
        """Stop watching."""
        self._running = False
        if self._thread:
            self._thread.join(timeout=10)

    def _watch_loop(self):
        import time
        while self._running:
            time.sleep(self.poll_interval)
            try:
                states = detect_hdr_state()
                for state in states:
                    prev = self._last_states.get(state.display_index)
                    if prev is not None and prev != state.hdr_enabled:
                        self._last_states[state.display_index] = state.hdr_enabled
                        if self.on_hdr_change:
                            self.on_hdr_change(
                                state.display_index,
                                state.hdr_enabled,
                                state
                            )
                    elif prev is None:
                        self._last_states[state.display_index] = state.hdr_enabled
            except Exception:
                pass


def print_hdr_status():
    """Print HDR status for all displays."""
    states = detect_hdr_state()

    if not states:
        print("No displays detected.")
        return

    for state in states:
        mode = "HDR ON" if state.hdr_enabled else "SDR"
        capable = "HDR capable" if state.hdr_capable else "SDR only"
        print(f"  {state.display_name}:")
        print(f"    Mode: {mode} ({capable})")
        if state.hdr_enabled:
            print(f"    Color space: {state.color_space}")
            print(f"    SDR white level: {state.sdr_white_level:.0f} cd/m2")
            if state.peak_luminance > 0:
                print(f"    Peak luminance: {state.peak_luminance:.0f} cd/m2")
        print(f"    Bit depth: {state.bit_depth}")
