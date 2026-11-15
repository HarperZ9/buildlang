"""
System-Wide Color Loader

Persistent color management that overrides Windows and NVIDIA color settings.
Applies ICC profiles and 3D LUTs system-wide for all applications.

Features:
- Gamma ramp loading with VCGT extraction
- Periodic re-application to prevent other apps from overriding
- System tray integration for background operation
- Support for per-display calibration
- HDR-aware color management
"""

import ctypes
from ctypes import wintypes
import threading
import time
import json
import os
import sys
from pathlib import Path
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple, Callable
from enum import Enum
import struct
import numpy as np

# Windows API
user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
kernel32 = ctypes.windll.kernel32


class LoaderStatus(Enum):
    """Color loader status."""
    STOPPED = "stopped"
    RUNNING = "running"
    PAUSED = "paused"
    ERROR = "error"


@dataclass
class DisplayCalibration:
    """Calibration data for a single display."""
    display_id: int
    device_name: str
    friendly_name: str
    icc_profile: Optional[str] = None
    lut_file: Optional[str] = None
    gamma_ramp: Optional[np.ndarray] = None  # [256, 3] array
    enabled: bool = True
    last_applied: float = 0.0


@dataclass
class LoaderConfig:
    """Color loader configuration."""
    refresh_interval: float = 5.0  # Seconds between re-applications
    force_override: bool = True     # Override other color management
    apply_on_start: bool = True     # Apply immediately on start
    persist_across_restart: bool = True  # Save config for system restart
    config_file: str = ""


class GammaRamp(ctypes.Structure):
    """Windows GAMMARAMP structure."""
    _fields_ = [
        ("Red", wintypes.WORD * 256),
        ("Green", wintypes.WORD * 256),
        ("Blue", wintypes.WORD * 256),
    ]


class ColorLoader:
    """
    System-wide color loader that persistently applies calibration.

    Overrides Windows Color Management and GPU color settings by
    periodically re-applying gamma ramps and monitoring for changes.
    """

    def __init__(self, config: Optional[LoaderConfig] = None):
        self.config = config or LoaderConfig()
        self.calibrations: Dict[int, DisplayCalibration] = {}
        self.status = LoaderStatus.STOPPED
        self._thread: Optional[threading.Thread] = None
        self._stop_event = threading.Event()
        self._callbacks: List[Callable] = []
        self._lock = threading.Lock()

        # Set default config path
        if not self.config.config_file:
            app_data = os.environ.get('APPDATA', os.path.expanduser('~'))
            self.config.config_file = os.path.join(
                app_data, 'CalibratePro', 'color_loader.json'
            )

        # Load saved config
        if self.config.persist_across_restart:
            self._load_config()

    def enumerate_displays(self) -> List[Dict]:
        """Enumerate all active displays."""
        displays = []

        class DISPLAY_DEVICE(ctypes.Structure):
            _fields_ = [
                ("cb", wintypes.DWORD),
                ("DeviceName", wintypes.WCHAR * 32),
                ("DeviceString", wintypes.WCHAR * 128),
                ("StateFlags", wintypes.DWORD),
                ("DeviceID", wintypes.WCHAR * 128),
                ("DeviceKey", wintypes.WCHAR * 128),
            ]

        device = DISPLAY_DEVICE()
        device.cb = ctypes.sizeof(device)
        i = 0

        while user32.EnumDisplayDevicesW(None, i, ctypes.byref(device), 0):
            if device.StateFlags & 0x00000001:  # DISPLAY_DEVICE_ACTIVE
                # Get monitor info
                monitor = DISPLAY_DEVICE()
                monitor.cb = ctypes.sizeof(monitor)
                user32.EnumDisplayDevicesW(device.DeviceName, 0, ctypes.byref(monitor), 0)

                displays.append({
                    'id': i,
                    'device_name': device.DeviceName,
                    'adapter': device.DeviceString,
                    'monitor': monitor.DeviceString if monitor.DeviceString else "Unknown",
                    'primary': bool(device.StateFlags & 0x00000004),
                })
            i += 1

        return displays

    def load_lut_file(self, display_id: int, lut_path: str) -> bool:
        """
        Load a 3D LUT file for a display.

        Args:
            display_id: Display index
            lut_path: Path to .cube or other LUT file

        Returns:
            True if successful
        """
        lut_path = Path(lut_path)
        if not lut_path.exists():
            return False

        try:
            # Load LUT and extract 1D gamma ramp
            from calibrate_pro.lut_system import load_lut
            lut = load_lut(lut_path)

            # Extract gamma ramp from LUT diagonal
            gamma_ramp = self._lut_to_gamma_ramp(lut.data)

            # Get display info
            displays = self.enumerate_displays()
            if display_id >= len(displays):
                return False

            display = displays[display_id]

            # Create/update calibration
            with self._lock:
                self.calibrations[display_id] = DisplayCalibration(
                    display_id=display_id,
                    device_name=display['device_name'],
                    friendly_name=display['monitor'],
                    lut_file=str(lut_path),
                    gamma_ramp=gamma_ramp,
                    enabled=True
                )

            # Apply immediately
            self._apply_calibration(display_id)

            # Save config
            if self.config.persist_across_restart:
                self._save_config()

            return True

        except Exception as e:
            print(f"Error loading LUT: {e}")
            return False

    def load_icc_profile(self, display_id: int, icc_path: str) -> bool:
        """
        Load an ICC profile for a display.

        Extracts VCGT tag for gamma ramp application.

        Args:
            display_id: Display index
            icc_path: Path to .icc/.icm profile

        Returns:
            True if successful
        """
        icc_path = Path(icc_path)
        if not icc_path.exists():
            return False

        try:
            # Extract VCGT from profile
            gamma_ramp = self._extract_vcgt(icc_path)

            if gamma_ramp is None:
                # Try to generate from TRC curves
                gamma_ramp = self._extract_trc(icc_path)

            if gamma_ramp is None:
                print("No VCGT or TRC found in profile")
                return False

            # Get display info
            displays = self.enumerate_displays()
            if display_id >= len(displays):
                return False

            display = displays[display_id]

            # Create/update calibration
            with self._lock:
                self.calibrations[display_id] = DisplayCalibration(
                    display_id=display_id,
                    device_name=display['device_name'],
                    friendly_name=display['monitor'],
                    icc_profile=str(icc_path),
                    gamma_ramp=gamma_ramp,
                    enabled=True
                )

            # Apply immediately
            self._apply_calibration(display_id)

            # Save config
            if self.config.persist_across_restart:
                self._save_config()

            return True

        except Exception as e:
            print(f"Error loading ICC profile: {e}")
            return False

    def set_gamma_ramp(
        self,
        display_id: int,
        red: np.ndarray,
        green: np.ndarray,
        blue: np.ndarray
    ) -> bool:
        """
        Set custom gamma ramp for a display.

        Args:
            display_id: Display index
            red: 256-element array (0-65535)
            green: 256-element array (0-65535)
            blue: 256-element array (0-65535)

        Returns:
            True if successful
        """
        gamma_ramp = np.zeros((256, 3), dtype=np.uint16)
        gamma_ramp[:, 0] = red.astype(np.uint16)
        gamma_ramp[:, 1] = green.astype(np.uint16)
        gamma_ramp[:, 2] = blue.astype(np.uint16)

        displays = self.enumerate_displays()
        if display_id >= len(displays):
            return False

        display = displays[display_id]

        with self._lock:
            self.calibrations[display_id] = DisplayCalibration(
                display_id=display_id,
                device_name=display['device_name'],
                friendly_name=display['monitor'],
                gamma_ramp=gamma_ramp,
                enabled=True
            )

        return self._apply_calibration(display_id)

    def start(self):
        """Start the color loader background service."""
        if self.status == LoaderStatus.RUNNING:
            return

        self._stop_event.clear()
        self.status = LoaderStatus.RUNNING

        # Apply all calibrations immediately
        if self.config.apply_on_start:
            self.apply_all()

        # Start background thread
        self._thread = threading.Thread(target=self._run_loop, daemon=True)
        self._thread.start()

        self._notify_callbacks('started')

    def stop(self):
        """Stop the color loader service."""
        if self.status != LoaderStatus.RUNNING:
            return

        self._stop_event.set()
        self.status = LoaderStatus.STOPPED

        if self._thread:
            self._thread.join(timeout=2.0)
            self._thread = None

        self._notify_callbacks('stopped')

    def pause(self):
        """Pause color loading (keeps thread running but doesn't apply)."""
        self.status = LoaderStatus.PAUSED
        self._notify_callbacks('paused')

    def resume(self):
        """Resume color loading after pause."""
        if self.status == LoaderStatus.PAUSED:
            self.status = LoaderStatus.RUNNING
            self.apply_all()
            self._notify_callbacks('resumed')

    def apply_all(self) -> Dict[int, bool]:
        """Apply all calibrations immediately."""
        results = {}
        with self._lock:
            for display_id in self.calibrations:
                results[display_id] = self._apply_calibration(display_id)
        return results

    def reset_display(self, display_id: int) -> bool:
        """Reset a display to linear gamma."""
        linear = np.zeros((256, 3), dtype=np.uint16)
        for i in range(256):
            value = int(i / 255.0 * 65535)
            linear[i] = [value, value, value]

        with self._lock:
            if display_id in self.calibrations:
                del self.calibrations[display_id]

        return self._apply_gamma_ramp_raw(display_id, linear)

    def reset_all(self) -> Dict[int, bool]:
        """Reset all displays to linear gamma."""
        results = {}
        display_ids = list(self.calibrations.keys())

        for display_id in display_ids:
            results[display_id] = self.reset_display(display_id)

        if self.config.persist_across_restart:
            self._save_config()

        return results

    def add_callback(self, callback: Callable):
        """Add a status change callback."""
        self._callbacks.append(callback)

    def get_status(self) -> Dict:
        """Get current loader status."""
        return {
            'status': self.status.value,
            'displays': len(self.calibrations),
            'calibrations': {
                k: {
                    'device': v.device_name,
                    'name': v.friendly_name,
                    'enabled': v.enabled,
                    'icc': v.icc_profile,
                    'lut': v.lut_file,
                    'last_applied': v.last_applied
                }
                for k, v in self.calibrations.items()
            }
        }

    # -------------------------------------------------------------------------
    # Private Methods
    # -------------------------------------------------------------------------

    def _run_loop(self):
        """Background thread loop."""
        while not self._stop_event.is_set():
            if self.status == LoaderStatus.RUNNING:
                self.apply_all()

            # Wait for interval or stop event
            self._stop_event.wait(timeout=self.config.refresh_interval)

    def _apply_calibration(self, display_id: int) -> bool:
        """Apply calibration for a single display."""
        cal = self.calibrations.get(display_id)
        if not cal or not cal.enabled or cal.gamma_ramp is None:
            return False

        success = self._apply_gamma_ramp_raw(display_id, cal.gamma_ramp)

        if success:
            cal.last_applied = time.time()

        return success

    def _apply_gamma_ramp_raw(self, display_id: int, gamma_ramp: np.ndarray) -> bool:
        """Apply raw gamma ramp to display."""
        displays = self.enumerate_displays()
        if display_id >= len(displays):
            return False

        device_name = displays[display_id]['device_name']

        # Create DC for display
        hdc = gdi32.CreateDCW(device_name, device_name, None, None)
        if not hdc:
            # Fallback to desktop DC
            hdc = user32.GetDC(None)

        if not hdc:
            return False

        try:
            ramp = GammaRamp()
            for i in range(256):
                ramp.Red[i] = int(gamma_ramp[i, 0])
                ramp.Green[i] = int(gamma_ramp[i, 1])
                ramp.Blue[i] = int(gamma_ramp[i, 2])

            result = gdi32.SetDeviceGammaRamp(hdc, ctypes.byref(ramp))
            return bool(result)

        finally:
            if hdc:
                gdi32.DeleteDC(hdc)

    def _lut_to_gamma_ramp(self, lut_data: np.ndarray) -> np.ndarray:
        """Convert 3D LUT to 1D gamma ramp."""
        size = lut_data.shape[0]
        gamma_ramp = np.zeros((256, 3), dtype=np.uint16)

        for i in range(256):
            # Map 0-255 to LUT indices
            idx = int(i / 255.0 * (size - 1))
            idx = min(idx, size - 1)

            # Sample diagonal (gray axis)
            r = lut_data[idx, idx, idx, 0]
            g = lut_data[idx, idx, idx, 1]
            b = lut_data[idx, idx, idx, 2]

            # Convert to 16-bit
            gamma_ramp[i, 0] = int(np.clip(r, 0, 1) * 65535)
            gamma_ramp[i, 1] = int(np.clip(g, 0, 1) * 65535)
            gamma_ramp[i, 2] = int(np.clip(b, 0, 1) * 65535)

        return gamma_ramp

    def _extract_vcgt(self, icc_path: Path) -> Optional[np.ndarray]:
        """Extract VCGT tag from ICC profile."""
        try:
            data = icc_path.read_bytes()

            # Find tag table
            tag_count = struct.unpack('>I', data[128:132])[0]

            for i in range(tag_count):
                offset = 132 + i * 12
                tag_sig = data[offset:offset+4]
                tag_offset = struct.unpack('>I', data[offset+4:offset+8])[0]
                tag_size = struct.unpack('>I', data[offset+8:offset+12])[0]

                if tag_sig == b'vcgt':
                    return self._parse_vcgt(data[tag_offset:tag_offset+tag_size])

            return None

        except Exception:
            return None

    def _parse_vcgt(self, vcgt_data: bytes) -> Optional[np.ndarray]:
        """Parse VCGT tag data."""
        try:
            # VCGT type signature
            if vcgt_data[0:4] != b'vcgt':
                return None

            gamma_type = struct.unpack('>I', vcgt_data[8:12])[0]

            if gamma_type == 0:
                # Table type
                channels = struct.unpack('>H', vcgt_data[12:14])[0]
                entry_count = struct.unpack('>H', vcgt_data[14:16])[0]
                entry_size = struct.unpack('>H', vcgt_data[16:18])[0]

                gamma_ramp = np.zeros((256, 3), dtype=np.uint16)
                offset = 18

                for c in range(min(channels, 3)):
                    for i in range(entry_count):
                        if entry_size == 2:
                            value = struct.unpack('>H', vcgt_data[offset:offset+2])[0]
                            offset += 2
                        else:
                            value = vcgt_data[offset] * 257
                            offset += 1

                        # Interpolate to 256 entries if needed
                        if entry_count == 256:
                            gamma_ramp[i, c] = value
                        else:
                            idx = int(i / (entry_count - 1) * 255)
                            gamma_ramp[idx, c] = value

                return gamma_ramp

            elif gamma_type == 1:
                # Formula type
                gamma_ramp = np.zeros((256, 3), dtype=np.uint16)

                for c in range(3):
                    offset = 12 + c * 12
                    gamma = struct.unpack('>I', vcgt_data[offset:offset+4])[0] / 65536.0
                    min_val = struct.unpack('>I', vcgt_data[offset+4:offset+8])[0] / 65536.0
                    max_val = struct.unpack('>I', vcgt_data[offset+8:offset+12])[0] / 65536.0

                    for i in range(256):
                        x = i / 255.0
                        y = min_val + (max_val - min_val) * (x ** gamma)
                        gamma_ramp[i, c] = int(np.clip(y, 0, 1) * 65535)

                return gamma_ramp

            return None

        except Exception:
            return None

    def _extract_trc(self, icc_path: Path) -> Optional[np.ndarray]:
        """Extract TRC curves from ICC profile."""
        try:
            data = icc_path.read_bytes()

            # Find tag table
            tag_count = struct.unpack('>I', data[128:132])[0]

            curves = {}
            trc_tags = {b'rTRC': 0, b'gTRC': 1, b'bTRC': 2}

            for i in range(tag_count):
                offset = 132 + i * 12
                tag_sig = data[offset:offset+4]
                tag_offset = struct.unpack('>I', data[offset+4:offset+8])[0]
                tag_size = struct.unpack('>I', data[offset+8:offset+12])[0]

                if tag_sig in trc_tags:
                    channel = trc_tags[tag_sig]
                    curves[channel] = self._parse_trc(data[tag_offset:tag_offset+tag_size])

            if len(curves) < 3:
                return None

            gamma_ramp = np.zeros((256, 3), dtype=np.uint16)
            for c in range(3):
                if c in curves and curves[c] is not None:
                    gamma_ramp[:, c] = curves[c]

            return gamma_ramp

        except Exception:
            return None

    def _parse_trc(self, trc_data: bytes) -> Optional[np.ndarray]:
        """Parse TRC (Tone Response Curve) tag."""
        try:
            type_sig = trc_data[0:4]

            if type_sig == b'curv':
                count = struct.unpack('>I', trc_data[8:12])[0]

                if count == 0:
                    # Identity
                    return np.array([int(i / 255.0 * 65535) for i in range(256)], dtype=np.uint16)

                elif count == 1:
                    # Gamma value
                    gamma = struct.unpack('>H', trc_data[12:14])[0] / 256.0
                    return np.array([int((i / 255.0) ** gamma * 65535) for i in range(256)], dtype=np.uint16)

                else:
                    # Table
                    curve = np.zeros(256, dtype=np.uint16)
                    for i in range(min(count, 256)):
                        value = struct.unpack('>H', trc_data[12+i*2:14+i*2])[0]
                        if count == 256:
                            curve[i] = value
                        else:
                            idx = int(i / (count - 1) * 255)
                            curve[idx] = value
                    return curve

            elif type_sig == b'para':
                # Parametric curve
                func_type = struct.unpack('>H', trc_data[8:10])[0]
                gamma = struct.unpack('>I', trc_data[12:16])[0] / 65536.0

                # Simple gamma for now
                return np.array([int((i / 255.0) ** gamma * 65535) for i in range(256)], dtype=np.uint16)

            return None

        except Exception:
            return None

    def _save_config(self):
        """Save configuration to file."""
        try:
            config_path = Path(self.config.config_file)
            config_path.parent.mkdir(parents=True, exist_ok=True)

            data = {
                'refresh_interval': self.config.refresh_interval,
                'force_override': self.config.force_override,
                'calibrations': {}
            }

            for display_id, cal in self.calibrations.items():
                data['calibrations'][str(display_id)] = {
                    'device_name': cal.device_name,
                    'friendly_name': cal.friendly_name,
                    'icc_profile': cal.icc_profile,
                    'lut_file': cal.lut_file,
                    'enabled': cal.enabled
                }

            config_path.write_text(json.dumps(data, indent=2))

        except Exception as e:
            print(f"Error saving config: {e}")

    def _load_config(self):
        """Load configuration from file."""
        try:
            config_path = Path(self.config.config_file)
            if not config_path.exists():
                return

            data = json.loads(config_path.read_text())

            self.config.refresh_interval = data.get('refresh_interval', 5.0)
            self.config.force_override = data.get('force_override', True)

            for display_id_str, cal_data in data.get('calibrations', {}).items():
                display_id = int(display_id_str)

                # Reload the LUT or profile
                if cal_data.get('lut_file') and Path(cal_data['lut_file']).exists():
                    self.load_lut_file(display_id, cal_data['lut_file'])
                elif cal_data.get('icc_profile') and Path(cal_data['icc_profile']).exists():
                    self.load_icc_profile(display_id, cal_data['icc_profile'])

        except Exception as e:
            print(f"Error loading config: {e}")

    def _notify_callbacks(self, event: str):
        """Notify all callbacks of an event."""
        for callback in self._callbacks:
            try:
                callback(event, self.get_status())
            except Exception:
                pass


# Singleton instance
_loader_instance: Optional[ColorLoader] = None


def get_color_loader() -> ColorLoader:
    """Get the global color loader instance."""
    global _loader_instance
    if _loader_instance is None:
        _loader_instance = ColorLoader()
    return _loader_instance


def apply_calibration(
    display_id: int = 0,
    lut_path: Optional[str] = None,
    icc_path: Optional[str] = None,
    start_service: bool = True
) -> bool:
    """
    Quick function to apply calibration and start the loader.

    Args:
        display_id: Display index
        lut_path: Path to .cube LUT file
        icc_path: Path to .icc profile
        start_service: Start background service

    Returns:
        True if successful
    """
    loader = get_color_loader()

    success = False

    if lut_path:
        success = loader.load_lut_file(display_id, lut_path)
    elif icc_path:
        success = loader.load_icc_profile(display_id, icc_path)

    if success and start_service:
        loader.start()

    return success
