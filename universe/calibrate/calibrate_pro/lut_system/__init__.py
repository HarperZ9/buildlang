"""
LUT System - System-wide 3D LUT Application

Provides unified interface for applying 3D LUTs across all GPU vendors:
- DWM-level LUT loading (works with any GPU)
- NVIDIA NVAPI integration
- AMD ADL integration
- Intel IGCL integration

LUT Format Support:
- .cube (DaVinci Resolve / Adobe)
- .3dl (Autodesk Lustre / Flame)
- .mga (Pandora)
- .cal (ArgyllCMS calibration)
- .csp (Cinespace)
- .spi3d (Sony Imageworks)
- .clf (ACES Common LUT Format)

Usage:
    from calibrate_pro.lut_system import LUTManager

    # Detect available GPU and apply LUT
    manager = LUTManager()
    manager.load_lut_file(0, "calibration.cube")

    # Or manually select backend
    manager = LUTManager(preferred_backend="nvidia")
"""

from pathlib import Path
from typing import List, Optional, Dict, Any, Union
from dataclasses import dataclass
from enum import Enum
import numpy as np

# Import LUT format handlers
from calibrate_pro.lut_system.lut_formats import (
    LUT1D,
    LUT3D,
    LUTType,
    LUTFormat,
    LUTReader,
    LUTWriter,
    load_lut,
    save_lut,
    convert_lut,
    create_identity_lut,
    combine_luts,
    resize_lut,
    invert_lut,
)


class LUTBackend(Enum):
    """Available LUT application backends."""
    DWM = "dwm"           # Desktop Window Manager (universal)
    NVIDIA = "nvidia"     # NVIDIA NVAPI
    AMD = "amd"           # AMD ADL
    INTEL = "intel"       # Intel IGCL
    GAMMA_RAMP = "gamma"  # Windows gamma ramp (fallback)


@dataclass
class DisplayInfo:
    """Display information for LUT targeting."""
    id: int
    name: str
    is_primary: bool
    gpu_vendor: str
    resolution: tuple
    hdr_capable: bool
    current_lut: Optional[str] = None


@dataclass
class BackendStatus:
    """Status of a LUT backend."""
    available: bool
    name: str
    vendor: str
    message: str
    display_count: int = 0


class LUTManager:
    """
    Unified LUT management across all GPU vendors.

    Automatically detects the best available backend and provides
    a consistent interface for LUT loading and management.
    """

    def __init__(self, preferred_backend: Optional[str] = None):
        """
        Initialize LUT manager.

        Args:
            preferred_backend: Preferred backend ("dwm", "nvidia", "amd", "intel", "gamma")
                             If None, auto-detects the best option.
        """
        self._backends = {}
        self._active_backend = None
        self._displays: List[DisplayInfo] = []
        self._loaded_luts: Dict[int, str] = {}

        self._initialize_backends()
        self._select_backend(preferred_backend)
        self._enumerate_displays()

    def _initialize_backends(self):
        """Initialize all available backends."""
        # Try DWM backend (universal, preferred)
        try:
            from calibrate_pro.lut_system.dwm_lut import DwmLutController
            dwm = DwmLutController()
            if dwm.is_available:
                self._backends[LUTBackend.DWM] = dwm
        except Exception:
            pass

        # Try NVIDIA backend
        try:
            from calibrate_pro.lut_system.nvidia_api import NvidiaAPI
            nvidia = NvidiaAPI()
            if nvidia.is_available:
                self._backends[LUTBackend.NVIDIA] = nvidia
        except Exception:
            pass

        # Try AMD backend
        try:
            from calibrate_pro.lut_system.amd_api import AMDAPI
            amd = AMDAPI()
            if amd.is_available:
                self._backends[LUTBackend.AMD] = amd
        except Exception:
            pass

        # Try Intel backend
        try:
            from calibrate_pro.lut_system.intel_api import IntelAPI
            intel = IntelAPI()
            if intel.is_available:
                self._backends[LUTBackend.INTEL] = intel
        except Exception:
            pass

        # Fallback: Windows gamma ramp
        try:
            from calibrate_pro.lut_system.dwm_lut import GammaRampController
            gamma = GammaRampController()
            self._backends[LUTBackend.GAMMA_RAMP] = gamma
        except Exception:
            pass

    def _select_backend(self, preferred: Optional[str]):
        """Select the best available backend."""
        if preferred:
            preferred_enum = {
                "dwm": LUTBackend.DWM,
                "nvidia": LUTBackend.NVIDIA,
                "amd": LUTBackend.AMD,
                "intel": LUTBackend.INTEL,
                "gamma": LUTBackend.GAMMA_RAMP,
            }.get(preferred.lower())

            if preferred_enum and preferred_enum in self._backends:
                self._active_backend = preferred_enum
                return

        # Auto-select priority: DWM > GPU-specific > Gamma ramp
        priority = [
            LUTBackend.DWM,
            LUTBackend.NVIDIA,
            LUTBackend.AMD,
            LUTBackend.INTEL,
            LUTBackend.GAMMA_RAMP,
        ]

        for backend in priority:
            if backend in self._backends:
                self._active_backend = backend
                return

    def _enumerate_displays(self):
        """Enumerate all displays from all backends."""
        self._displays = []
        seen_displays = set()

        # Get displays from active backend
        if self._active_backend:
            backend = self._backends[self._active_backend]

            if hasattr(backend, 'displays'):
                displays = backend.displays
                for i, d in enumerate(displays):
                    display_id = getattr(d, 'display_id', i)
                    if display_id not in seen_displays:
                        seen_displays.add(display_id)
                        self._displays.append(DisplayInfo(
                            id=display_id,
                            name=getattr(d, 'name', f"Display {display_id}"),
                            is_primary=getattr(d, 'is_primary', i == 0),
                            gpu_vendor=self._active_backend.value,
                            resolution=getattr(d, 'resolution', (0, 0)),
                            hdr_capable=getattr(d, 'is_hdr', False) or getattr(d, 'hdr_supported', False),
                        ))

        # If no displays found, try Windows enumeration
        if not self._displays:
            self._enumerate_windows_displays()

    def _enumerate_windows_displays(self):
        """Enumerate displays via Windows API."""
        try:
            import ctypes
            from ctypes import wintypes

            user32 = ctypes.windll.user32

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
                if device.StateFlags & 0x00000001:  # ACTIVE
                    # Detect GPU vendor from device ID
                    device_id = device.DeviceID.lower()
                    if "nvidia" in device_id:
                        vendor = "nvidia"
                    elif "amd" in device_id or "ati" in device_id:
                        vendor = "amd"
                    elif "intel" in device_id or "8086" in device_id:
                        vendor = "intel"
                    else:
                        vendor = "unknown"

                    self._displays.append(DisplayInfo(
                        id=i,
                        name=device.DeviceString,
                        is_primary=bool(device.StateFlags & 0x00000004),
                        gpu_vendor=vendor,
                        resolution=(0, 0),
                        hdr_capable=False,
                    ))
                i += 1

        except Exception:
            pass

    @property
    def is_available(self) -> bool:
        """Check if any LUT backend is available."""
        return self._active_backend is not None

    @property
    def active_backend(self) -> Optional[LUTBackend]:
        """Get the active backend."""
        return self._active_backend

    @property
    def displays(self) -> List[DisplayInfo]:
        """Get list of available displays."""
        return self._displays

    def get_backend_status(self) -> Dict[str, BackendStatus]:
        """Get status of all backends."""
        status = {}

        backends_info = [
            (LUTBackend.DWM, "DWM Color Pipeline", "Universal"),
            (LUTBackend.NVIDIA, "NVIDIA NVAPI", "NVIDIA"),
            (LUTBackend.AMD, "AMD ADL", "AMD"),
            (LUTBackend.INTEL, "Intel IGCL", "Intel"),
            (LUTBackend.GAMMA_RAMP, "Windows Gamma Ramp", "Windows"),
        ]

        for backend, name, vendor in backends_info:
            if backend in self._backends:
                api = self._backends[backend]
                display_count = len(api.displays) if hasattr(api, 'displays') else 0
                status[backend.value] = BackendStatus(
                    available=True,
                    name=name,
                    vendor=vendor,
                    message="Available",
                    display_count=display_count
                )
            else:
                status[backend.value] = BackendStatus(
                    available=False,
                    name=name,
                    vendor=vendor,
                    message="Not available"
                )

        return status

    def load_lut(
        self,
        display_id: int,
        lut: Union[LUT3D, np.ndarray],
        persist: bool = False
    ) -> bool:
        """
        Load a 3D LUT to a display.

        Args:
            display_id: Display index
            lut: LUT3D object or numpy array [size, size, size, 3]
            persist: If True, save LUT path for reload on restart

        Returns:
            True if successful
        """
        if not self._active_backend:
            return False

        # Convert numpy array to LUT3D if needed
        if isinstance(lut, np.ndarray):
            size = lut.shape[0]
            lut = LUT3D(size=size, data=lut)

        backend = self._backends[self._active_backend]

        try:
            # Different backends have different interfaces
            if self._active_backend == LUTBackend.DWM:
                return backend.load_lut(display_id, lut.data)
            elif self._active_backend == LUTBackend.NVIDIA:
                return backend.load_3d_lut(display_id, lut.data)
            elif self._active_backend == LUTBackend.AMD:
                return backend.load_3d_lut(display_id, lut.data)
            elif self._active_backend == LUTBackend.INTEL:
                return backend.load_3d_lut(display_id, lut.data)
            elif self._active_backend == LUTBackend.GAMMA_RAMP:
                # Convert 3D LUT to 1D approximation for gamma ramp
                lut1d = lut.to_1d_approximation(256)
                r = (lut1d.data[:, 0] * 65535).astype(np.uint16)
                g = (lut1d.data[:, 1] * 65535).astype(np.uint16)
                b = (lut1d.data[:, 2] * 65535).astype(np.uint16)
                return backend.set_gamma_ramp(display_id, r, g, b)

        except Exception:
            pass

        return False

    def load_lut_file(
        self,
        display_id: int,
        filepath: Union[str, Path],
        persist: bool = True
    ) -> bool:
        """
        Load a 3D LUT from file to a display.

        Args:
            display_id: Display index
            filepath: Path to LUT file (.cube, .3dl, etc.)
            persist: If True, remember for reload on restart

        Returns:
            True if successful
        """
        filepath = Path(filepath)

        try:
            lut = load_lut(filepath)

            if isinstance(lut, LUT1D):
                # Convert 1D to gamma ramp
                if self._active_backend == LUTBackend.GAMMA_RAMP:
                    backend = self._backends[self._active_backend]
                    r = (lut.data[:, 0] * 65535).astype(np.uint16)
                    g = (lut.data[:, 1] * 65535).astype(np.uint16)
                    b = (lut.data[:, 2] * 65535).astype(np.uint16)
                    return backend.set_gamma_ramp(display_id, r, g, b)
                return False

            success = self.load_lut(display_id, lut, persist)

            if success and persist:
                self._loaded_luts[display_id] = str(filepath)

            return success

        except Exception:
            return False

    def unload_lut(self, display_id: int) -> bool:
        """
        Remove LUT from display (reset to identity).

        Args:
            display_id: Display index

        Returns:
            True if successful
        """
        if not self._active_backend:
            return False

        backend = self._backends[self._active_backend]

        try:
            if hasattr(backend, 'unload_lut'):
                result = backend.unload_lut(display_id)
            elif hasattr(backend, 'reset_lut'):
                result = backend.reset_lut(display_id)
            else:
                # Load identity LUT
                identity = LUT3D.create_identity(33)
                result = self.load_lut(display_id, identity)

            if result and display_id in self._loaded_luts:
                del self._loaded_luts[display_id]

            return result

        except Exception:
            return False

    def reload_all_luts(self) -> Dict[int, bool]:
        """
        Reload all previously loaded LUTs.

        Returns:
            Dict mapping display_id to success status
        """
        results = {}
        for display_id, filepath in list(self._loaded_luts.items()):
            results[display_id] = self.load_lut_file(display_id, filepath, persist=False)
        return results

    def get_display_info(self, display_id: int) -> Optional[DisplayInfo]:
        """Get information about a specific display."""
        for display in self._displays:
            if display.id == display_id:
                return display
        return None

    def get_loaded_lut(self, display_id: int) -> Optional[str]:
        """Get path of currently loaded LUT for a display."""
        return self._loaded_luts.get(display_id)

    def cleanup(self):
        """Clean up all backends and reset LUTs."""
        for display in self._displays:
            try:
                self.unload_lut(display.id)
            except Exception:
                pass

        for backend in self._backends.values():
            if hasattr(backend, 'cleanup'):
                try:
                    backend.cleanup()
                except Exception:
                    pass


# Convenience functions

def get_lut_manager(preferred_backend: Optional[str] = None) -> LUTManager:
    """Get a LUT manager instance."""
    return LUTManager(preferred_backend)


def apply_lut_to_display(
    display_id: int,
    lut_path: Union[str, Path],
    backend: Optional[str] = None
) -> bool:
    """
    Quick function to apply a LUT file to a display.

    Args:
        display_id: Display index
        lut_path: Path to LUT file
        backend: Optional preferred backend

    Returns:
        True if successful
    """
    manager = LUTManager(backend)
    return manager.load_lut_file(display_id, lut_path)


def reset_display_lut(display_id: int, backend: Optional[str] = None) -> bool:
    """
    Quick function to reset a display's LUT.

    Args:
        display_id: Display index
        backend: Optional preferred backend

    Returns:
        True if successful
    """
    manager = LUTManager(backend)
    return manager.unload_lut(display_id)


def list_displays() -> List[DisplayInfo]:
    """Get list of all displays."""
    manager = LUTManager()
    return manager.displays


def check_lut_support() -> Dict[str, BackendStatus]:
    """Check which LUT backends are available."""
    manager = LUTManager()
    return manager.get_backend_status()


# Import color loader
from calibrate_pro.lut_system.color_loader import (
    ColorLoader,
    LoaderConfig,
    LoaderStatus,
    DisplayCalibration,
    get_color_loader,
    apply_calibration,
)

# Import per-display calibration
from calibrate_pro.lut_system.per_display_calibration import (
    PerDisplayCalibrationManager,
    PerDisplayCalibrationConfig,
    DisplayCalibrationProfile,
    CalibrationSource,
    CalibrationTarget,
    get_per_display_manager,
    auto_calibrate_all_displays,
    apply_forum_calibration,
    list_detected_displays,
    get_display_status,
)


__all__ = [
    # Main classes
    "LUTManager",
    "LUTBackend",
    "DisplayInfo",
    "BackendStatus",
    # LUT format classes
    "LUT1D",
    "LUT3D",
    "LUTType",
    "LUTFormat",
    "LUTReader",
    "LUTWriter",
    # LUT operations
    "load_lut",
    "save_lut",
    "convert_lut",
    "create_identity_lut",
    "combine_luts",
    "resize_lut",
    "invert_lut",
    # Convenience functions
    "get_lut_manager",
    "apply_lut_to_display",
    "reset_display_lut",
    "list_displays",
    "check_lut_support",
    # Color loader
    "ColorLoader",
    "LoaderConfig",
    "LoaderStatus",
    "DisplayCalibration",
    "get_color_loader",
    "apply_calibration",
    # Per-display calibration
    "PerDisplayCalibrationManager",
    "PerDisplayCalibrationConfig",
    "DisplayCalibrationProfile",
    "CalibrationSource",
    "CalibrationTarget",
    "get_per_display_manager",
    "auto_calibrate_all_displays",
    "apply_forum_calibration",
    "list_detected_displays",
    "get_display_status",
]
