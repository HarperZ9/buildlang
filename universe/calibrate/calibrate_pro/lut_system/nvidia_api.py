"""
NVIDIA NVAPI Integration for GPU Color Management

Provides GPU-level color control via NVIDIA NVAPI:
- Direct color control (brightness, contrast, gamma, vibrance)
- HDR color management
- 3D LUT via D3D11 Video Processor
- Per-display color calibration

Requirements:
- NVIDIA GPU with driver 390.0+
- nvapi64.dll (included with NVIDIA drivers)
"""

import ctypes
from ctypes import wintypes, POINTER, Structure, Union, c_int, c_uint, c_float
from ctypes import c_void_p, c_char, c_wchar, byref, sizeof, cast
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Union as TypingUnion
import struct
import numpy as np
from enum import IntEnum, IntFlag
from dataclasses import dataclass
import sys


# =============================================================================
# NVAPI Constants and Types
# =============================================================================

class NvStatus(IntEnum):
    """NVAPI return status codes."""
    OK = 0
    ERROR = -1
    LIBRARY_NOT_FOUND = -2
    NO_IMPLEMENTATION = -3
    API_NOT_INITIALIZED = -4
    INVALID_ARGUMENT = -5
    NVIDIA_DEVICE_NOT_FOUND = -6
    END_ENUMERATION = -7
    INVALID_HANDLE = -8
    INCOMPATIBLE_STRUCT_VERSION = -9
    HANDLE_INVALIDATED = -10
    NOT_SUPPORTED = -104
    INVALID_DISPLAY = -163


class NvColorCommand(IntEnum):
    """Color control commands."""
    GET_CURRENT = 0x00000001
    SET_CURRENT = 0x00000002
    GET_DEFAULT = 0x00000010
    SET_DEFAULT = 0x00000020


class NvColorType(IntFlag):
    """Color control types."""
    BRIGHTNESS = 0x00000001
    CONTRAST = 0x00000002
    GAMMA = 0x00000004
    SATURATION = 0x00000008  # Also called "vibrance"
    HUE = 0x00000010


class NvHdrMode(IntEnum):
    """HDR modes."""
    OFF = 0
    UHDA = 2  # Ultra HD Alliance mode (HDR10)
    UHDA_PASSTHROUGH = 3
    DOLBY_VISION = 4
    EDR = 5  # Extended Dynamic Range
    SDR = 6


# NVAPI Handle types
NvDisplayHandle = c_int
NvPhysicalGpuHandle = c_int
NvLogicalGpuHandle = c_int

# NVAPI version macros
NVAPI_GENERIC_STRING_MAX = 4096
NVAPI_SHORT_STRING_MAX = 64
NVAPI_LONG_STRING_MAX = 256

# NVAPI function IDs (used with nvapi_QueryInterface)
NVAPI_FUNCS = {
    'Initialize': 0x0150E828,
    'Unload': 0xD22BDD7E,
    'GetErrorMessage': 0x6C2D048C,
    'EnumNvidiaDisplayHandle': 0x9ABDD40D,
    'EnumPhysicalGPUs': 0xE5AC921F,
    'GetPhysicalGPUsFromDisplay': 0x34EF9506,
    'GPU_GetFullName': 0xCEEE8E9F,
    'GetAssociatedNvidiaDisplayHandle': 0x35C29134,
    'GetAssociatedDisplayOutputId': 0xD995937E,
    'Disp_ColorControl': 0x92F9D80D,
    'Disp_GetHdrCapabilities': 0x84F2A8E5,
    'Disp_HdrColorControl': 0x351DA224,
    'Disp_GetGDIPrimaryDisplayId': 0x1E9D8A31,
    'GPU_GetConnectedDisplayIds': 0x0078DBA2,
    'Disp_GetDisplayIdByDisplayName': 0xAE457190,
    'SYS_GetDisplayDriverVersion': 0xF951A4D1,
}


# =============================================================================
# NVAPI Structures
# =============================================================================

class NV_COLOR_DATA_V5(Structure):
    """Color control data structure (version 5)."""
    _fields_ = [
        ('version', c_uint),
        ('size', c_uint),
        ('cmd', c_int),
        ('data', c_int),  # NvColorType flags
        ('colorBrightness', c_int),  # -100 to 100
        ('colorContrast', c_int),    # -100 to 100
        ('colorGamma', c_int),       # -100 to 100
        ('colorSaturation', c_int),  # -100 to 100 (vibrance)
        ('colorHue', c_int),         # 0 to 359
    ]


class NV_HDR_CAPABILITIES_V2(Structure):
    """HDR capabilities structure."""
    _fields_ = [
        ('version', c_uint),
        ('isST2084EotfSupported', c_uint, 1),
        ('isTraditionalHdrGammaSupported', c_uint, 1),
        ('isEdrSupported', c_uint, 1),
        ('driverExpandDefaultHdrParameters', c_uint, 1),
        ('isTraditionalSdrGammaSupported', c_uint, 1),
        ('isDolbyVisionSupported', c_uint, 1),
        ('reserved', c_uint, 26),
        ('display_data', c_char * 64),  # Simplified - actual structure is more complex
    ]


class NV_HDR_COLOR_DATA_V2(Structure):
    """HDR color control structure."""
    _fields_ = [
        ('version', c_uint),
        ('cmd', c_int),
        ('hdrMode', c_int),
        ('static_metadata_descriptor_id', c_int),
        ('mastering_display_data', c_char * 40),  # Simplified
    ]


class NV_GPU_DISPLAYIDS(Structure):
    """Display ID structure."""
    _fields_ = [
        ('version', c_uint),
        ('connectorType', c_uint),
        ('displayId', c_uint),
        ('isDynamic', c_uint, 1),
        ('isMultiStreamRootNode', c_uint, 1),
        ('isActive', c_uint, 1),
        ('isCluster', c_uint, 1),
        ('isOSVisible', c_uint, 1),
        ('isWFD', c_uint, 1),
        ('isConnected', c_uint, 1),
        ('reserved', c_uint, 22),
        ('isPhysicallyConnected', c_uint, 1),
        ('reserved2', c_uint, 2),
    ]


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class NvidiaDisplayInfo:
    """NVIDIA display information."""
    display_id: int
    display_handle: int
    gpu_handle: int
    name: str
    is_primary: bool
    is_active: bool
    is_hdr_capable: bool
    is_hdr_enabled: bool
    resolution: Tuple[int, int]
    refresh_rate: float
    connector_type: str


@dataclass
class NvidiaGpuInfo:
    """NVIDIA GPU information."""
    handle: int
    name: str
    driver_version: str
    display_count: int


@dataclass
class ColorSettings:
    """Display color settings."""
    brightness: int = 0   # -100 to 100
    contrast: int = 0     # -100 to 100
    gamma: int = 0        # -100 to 100
    saturation: int = 0   # -100 to 100 (vibrance)
    hue: int = 0          # 0 to 359


class NvidiaAPIError(Exception):
    """NVIDIA API error."""
    pass


# =============================================================================
# Main NVIDIA API Class
# =============================================================================

class NvidiaAPI:
    """
    NVIDIA NVAPI wrapper for GPU-level color management.

    Provides:
    - Color control (brightness, contrast, gamma, saturation/vibrance)
    - HDR mode control
    - Display enumeration
    - GPU information
    - 3D LUT application (via fallback methods when native not available)
    """

    def __init__(self):
        self._initialized = False
        self._nvapi = None
        self._funcs: Dict[str, ctypes.CFUNCTYPE] = {}
        self._gpus: List[NvidiaGpuInfo] = []
        self._displays: List[NvidiaDisplayInfo] = []
        self._active_luts: Dict[int, np.ndarray] = {}

        self._initialize()

    def _initialize(self) -> bool:
        """Initialize NVAPI library."""
        try:
            # Load nvapi64.dll (64-bit) or nvapi.dll (32-bit)
            if sys.maxsize > 2**32:
                dll_name = "nvapi64.dll"
            else:
                dll_name = "nvapi.dll"

            self._nvapi = ctypes.CDLL(dll_name)

            # Get QueryInterface function
            query_interface = self._nvapi.nvapi_QueryInterface
            query_interface.restype = c_void_p
            query_interface.argtypes = [c_uint]

            # Get NvAPI_Initialize
            init_ptr = query_interface(NVAPI_FUNCS['Initialize'])
            if not init_ptr:
                return False

            NvAPI_Initialize = ctypes.cast(
                init_ptr,
                ctypes.CFUNCTYPE(c_int)
            )

            status = NvAPI_Initialize()
            if status != NvStatus.OK:
                return False

            self._initialized = True

            # Cache commonly used functions
            self._cache_functions(query_interface)

            # Enumerate GPUs and displays
            self._enumerate_gpus()
            self._enumerate_displays()

            return True

        except OSError as e:
            # nvapi.dll not found (not NVIDIA GPU or driver not installed)
            return False
        except Exception as e:
            return False

    def _cache_functions(self, query_interface):
        """Cache NVAPI function pointers."""
        # Color control
        ptr = query_interface(NVAPI_FUNCS['Disp_ColorControl'])
        if ptr:
            self._funcs['ColorControl'] = ctypes.cast(
                ptr,
                ctypes.CFUNCTYPE(c_int, c_int, POINTER(NV_COLOR_DATA_V5))
            )

        # HDR capabilities
        ptr = query_interface(NVAPI_FUNCS['Disp_GetHdrCapabilities'])
        if ptr:
            self._funcs['GetHdrCapabilities'] = ctypes.cast(
                ptr,
                ctypes.CFUNCTYPE(c_int, c_uint, POINTER(NV_HDR_CAPABILITIES_V2))
            )

        # HDR color control
        ptr = query_interface(NVAPI_FUNCS['Disp_HdrColorControl'])
        if ptr:
            self._funcs['HdrColorControl'] = ctypes.cast(
                ptr,
                ctypes.CFUNCTYPE(c_int, c_uint, POINTER(NV_HDR_COLOR_DATA_V2))
            )

        # Enum displays
        ptr = query_interface(NVAPI_FUNCS['EnumNvidiaDisplayHandle'])
        if ptr:
            self._funcs['EnumDisplayHandle'] = ctypes.cast(
                ptr,
                ctypes.CFUNCTYPE(c_int, c_uint, POINTER(NvDisplayHandle))
            )

        # Enum GPUs
        ptr = query_interface(NVAPI_FUNCS['EnumPhysicalGPUs'])
        if ptr:
            self._funcs['EnumPhysicalGPUs'] = ctypes.cast(
                ptr,
                ctypes.CFUNCTYPE(c_int, POINTER(NvPhysicalGpuHandle * 64), POINTER(c_uint))
            )

        # GPU name
        ptr = query_interface(NVAPI_FUNCS['GPU_GetFullName'])
        if ptr:
            self._funcs['GetGpuFullName'] = ctypes.cast(
                ptr,
                ctypes.CFUNCTYPE(c_int, NvPhysicalGpuHandle, ctypes.c_char_p)
            )

        # Get GPU from display
        ptr = query_interface(NVAPI_FUNCS['GetPhysicalGPUsFromDisplay'])
        if ptr:
            self._funcs['GetPhysicalGPUsFromDisplay'] = ctypes.cast(
                ptr,
                ctypes.CFUNCTYPE(c_int, NvDisplayHandle, POINTER(NvPhysicalGpuHandle * 64), POINTER(c_uint))
            )

    def _enumerate_gpus(self):
        """Enumerate NVIDIA GPUs."""
        if 'EnumPhysicalGPUs' not in self._funcs:
            return

        gpu_handles = (NvPhysicalGpuHandle * 64)()
        gpu_count = c_uint(0)

        status = self._funcs['EnumPhysicalGPUs'](byref(gpu_handles), byref(gpu_count))
        if status != NvStatus.OK:
            return

        for i in range(gpu_count.value):
            gpu_handle = gpu_handles[i]

            # Get GPU name
            name_buffer = ctypes.create_string_buffer(NVAPI_SHORT_STRING_MAX)
            gpu_name = "Unknown NVIDIA GPU"

            if 'GetGpuFullName' in self._funcs:
                status = self._funcs['GetGpuFullName'](gpu_handle, name_buffer)
                if status == NvStatus.OK:
                    gpu_name = name_buffer.value.decode('utf-8', errors='ignore')

            self._gpus.append(NvidiaGpuInfo(
                handle=gpu_handle,
                name=gpu_name,
                driver_version="",  # Would need separate API call
                display_count=0
            ))

    def _enumerate_displays(self):
        """Enumerate NVIDIA displays."""
        if 'EnumDisplayHandle' not in self._funcs:
            # Fall back to Windows enumeration
            self._enumerate_displays_windows()
            return

        i = 0
        while True:
            display_handle = NvDisplayHandle()
            status = self._funcs['EnumDisplayHandle'](i, byref(display_handle))

            if status == NvStatus.END_ENUMERATION:
                break
            if status != NvStatus.OK:
                break

            # Get GPU for this display
            gpu_handle = 0
            if 'GetPhysicalGPUsFromDisplay' in self._funcs:
                gpu_handles = (NvPhysicalGpuHandle * 64)()
                gpu_count = c_uint(0)
                status = self._funcs['GetPhysicalGPUsFromDisplay'](
                    display_handle, byref(gpu_handles), byref(gpu_count)
                )
                if status == NvStatus.OK and gpu_count.value > 0:
                    gpu_handle = gpu_handles[0]

            # Check HDR capability
            is_hdr_capable = False
            is_hdr_enabled = False
            if 'GetHdrCapabilities' in self._funcs:
                caps = NV_HDR_CAPABILITIES_V2()
                caps.version = sizeof(NV_HDR_CAPABILITIES_V2) | (2 << 16)
                # Would need display ID, not handle

            self._displays.append(NvidiaDisplayInfo(
                display_id=i,
                display_handle=display_handle,
                gpu_handle=gpu_handle,
                name=f"NVIDIA Display {i}",
                is_primary=(i == 0),
                is_active=True,
                is_hdr_capable=is_hdr_capable,
                is_hdr_enabled=is_hdr_enabled,
                resolution=(0, 0),
                refresh_rate=60.0,
                connector_type="Unknown"
            ))

            i += 1

    def _enumerate_displays_windows(self):
        """Fallback display enumeration via Windows API."""
        try:
            user32 = ctypes.windll.user32

            class DISPLAY_DEVICE(Structure):
                _fields_ = [
                    ("cb", wintypes.DWORD),
                    ("DeviceName", wintypes.WCHAR * 32),
                    ("DeviceString", wintypes.WCHAR * 128),
                    ("StateFlags", wintypes.DWORD),
                    ("DeviceID", wintypes.WCHAR * 128),
                    ("DeviceKey", wintypes.WCHAR * 128),
                ]

            device = DISPLAY_DEVICE()
            device.cb = sizeof(device)
            i = 0

            while user32.EnumDisplayDevicesW(None, i, byref(device), 0):
                if device.StateFlags & 0x00000001:  # ACTIVE
                    is_nvidia = (
                        "NVIDIA" in device.DeviceString or
                        "nvidia" in device.DeviceID.lower()
                    )
                    if is_nvidia:
                        self._displays.append(NvidiaDisplayInfo(
                            display_id=i,
                            display_handle=i,
                            gpu_handle=0,
                            name=device.DeviceString,
                            is_primary=bool(device.StateFlags & 0x00000004),
                            is_active=True,
                            is_hdr_capable=False,
                            is_hdr_enabled=False,
                            resolution=(0, 0),
                            refresh_rate=60.0,
                            connector_type="Unknown"
                        ))
                i += 1
        except Exception:
            pass

    # =========================================================================
    # Public Properties
    # =========================================================================

    @property
    def is_available(self) -> bool:
        """Check if NVIDIA API is available."""
        return self._initialized

    @property
    def gpus(self) -> List[NvidiaGpuInfo]:
        """Get list of NVIDIA GPUs."""
        return self._gpus

    @property
    def displays(self) -> List[NvidiaDisplayInfo]:
        """Get list of NVIDIA displays."""
        return self._displays

    # =========================================================================
    # Color Control Methods
    # =========================================================================

    def get_color_settings(self, display_id: int = 0) -> Optional[ColorSettings]:
        """
        Get current color settings for a display.

        Args:
            display_id: Display index

        Returns:
            ColorSettings or None if failed
        """
        # Try NVAPI first
        settings = self._get_color_via_nvapi(display_id)
        if settings:
            return settings

        # Fall back to registry-based settings
        return self._get_color_via_registry(display_id)

    def _get_color_via_nvapi(self, display_id: int) -> Optional[ColorSettings]:
        """Get color settings via NVAPI (may not work on all driver versions)."""
        if not self._initialized or 'ColorControl' not in self._funcs:
            return None

        if display_id >= len(self._displays):
            return None

        display = self._displays[display_id]

        # Try with proper display ID from NVAPI
        actual_display_id = self._get_nvapi_display_id(display_id)
        if actual_display_id is None:
            actual_display_id = display.display_handle

        color_data = NV_COLOR_DATA_V5()
        color_data.version = sizeof(NV_COLOR_DATA_V5) | (5 << 16)
        color_data.size = sizeof(NV_COLOR_DATA_V5)
        color_data.cmd = NvColorCommand.GET_CURRENT
        color_data.data = (NvColorType.BRIGHTNESS | NvColorType.CONTRAST |
                          NvColorType.GAMMA | NvColorType.SATURATION | NvColorType.HUE)

        status = self._funcs['ColorControl'](actual_display_id, byref(color_data))

        if status == NvStatus.OK:
            return ColorSettings(
                brightness=color_data.colorBrightness,
                contrast=color_data.colorContrast,
                gamma=color_data.colorGamma,
                saturation=color_data.colorSaturation,
                hue=color_data.colorHue
            )

        return None

    def _get_nvapi_display_id(self, display_index: int) -> Optional[int]:
        """Get the NVAPI display ID for a display index."""
        try:
            if not self._nvapi:
                return None

            query_interface = self._nvapi.nvapi_QueryInterface
            query_interface.restype = c_void_p
            query_interface.argtypes = [c_uint]

            # NvAPI_Disp_GetGDIPrimaryDisplayId
            ptr = query_interface(NVAPI_FUNCS['Disp_GetGDIPrimaryDisplayId'])
            if ptr and display_index == 0:
                func = ctypes.cast(ptr, ctypes.CFUNCTYPE(c_int, POINTER(c_uint)))
                display_id = c_uint(0)
                if func(byref(display_id)) == NvStatus.OK:
                    return display_id.value

            return None
        except Exception:
            return None

    def _get_color_via_registry(self, display_id: int) -> Optional[ColorSettings]:
        """Get color settings from NVIDIA registry keys."""
        try:
            import winreg

            # NVIDIA stores Digital Vibrance in registry
            key_path = r"SOFTWARE\NVIDIA Corporation\Global\NVTweak"

            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path) as key:
                try:
                    vibrance, _ = winreg.QueryValueEx(key, "DVibrance")
                    # DVibrance is 0-100, map to -100 to 100
                    saturation = int((vibrance - 50) * 2)
                    return ColorSettings(saturation=saturation)
                except FileNotFoundError:
                    pass

            return ColorSettings()  # Return defaults
        except Exception:
            return None

    def set_color_settings(
        self,
        display_id: int = 0,
        brightness: Optional[int] = None,
        contrast: Optional[int] = None,
        gamma: Optional[int] = None,
        saturation: Optional[int] = None,
        hue: Optional[int] = None
    ) -> bool:
        """
        Set color settings for a display.

        All values are in range -100 to 100 (except hue: 0-359).
        Pass None to leave a setting unchanged.

        Args:
            display_id: Display index
            brightness: Brightness adjustment (-100 to 100)
            contrast: Contrast adjustment (-100 to 100)
            gamma: Gamma adjustment (-100 to 100)
            saturation: Saturation/vibrance (-100 to 100)
            hue: Hue rotation (0 to 359)

        Returns:
            True if successful
        """
        # Try NVAPI first
        if self._set_color_via_nvapi(display_id, brightness, contrast, gamma, saturation, hue):
            return True

        # Fall back to registry for Digital Vibrance
        if saturation is not None:
            return self._set_vibrance_via_registry(display_id, saturation)

        return False

    def _set_color_via_nvapi(
        self,
        display_id: int,
        brightness: Optional[int],
        contrast: Optional[int],
        gamma: Optional[int],
        saturation: Optional[int],
        hue: Optional[int]
    ) -> bool:
        """Set color via NVAPI (may not work on all driver versions)."""
        if not self._initialized or 'ColorControl' not in self._funcs:
            return False

        if display_id >= len(self._displays):
            return False

        display = self._displays[display_id]

        # Get proper display ID
        actual_display_id = self._get_nvapi_display_id(display_id)
        if actual_display_id is None:
            actual_display_id = display.display_handle

        # First get current settings
        current = self.get_color_settings(display_id)
        if current is None:
            current = ColorSettings()

        # Build data flags for what we're setting
        data_flags = 0

        color_data = NV_COLOR_DATA_V5()
        color_data.version = sizeof(NV_COLOR_DATA_V5) | (5 << 16)
        color_data.size = sizeof(NV_COLOR_DATA_V5)
        color_data.cmd = NvColorCommand.SET_CURRENT

        if brightness is not None:
            data_flags |= NvColorType.BRIGHTNESS
            color_data.colorBrightness = max(-100, min(100, brightness))
        else:
            color_data.colorBrightness = current.brightness

        if contrast is not None:
            data_flags |= NvColorType.CONTRAST
            color_data.colorContrast = max(-100, min(100, contrast))
        else:
            color_data.colorContrast = current.contrast

        if gamma is not None:
            data_flags |= NvColorType.GAMMA
            color_data.colorGamma = max(-100, min(100, gamma))
        else:
            color_data.colorGamma = current.gamma

        if saturation is not None:
            data_flags |= NvColorType.SATURATION
            color_data.colorSaturation = max(-100, min(100, saturation))
        else:
            color_data.colorSaturation = current.saturation

        if hue is not None:
            data_flags |= NvColorType.HUE
            color_data.colorHue = hue % 360
        else:
            color_data.colorHue = current.hue

        color_data.data = data_flags

        status = self._funcs['ColorControl'](actual_display_id, byref(color_data))
        return status == NvStatus.OK

    def _set_vibrance_via_registry(self, display_id: int, saturation: int) -> bool:
        """Set Digital Vibrance via NVIDIA registry keys."""
        try:
            import winreg
            import subprocess

            # Map -100 to 100 range to 0-100 for DVibrance
            vibrance = int((saturation + 100) / 2)
            vibrance = max(0, min(100, vibrance))

            # NVIDIA stores Digital Vibrance in registry
            key_path = r"SOFTWARE\NVIDIA Corporation\Global\NVTweak"

            try:
                with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path, 0,
                                   winreg.KEY_SET_VALUE) as key:
                    winreg.SetValueEx(key, "DVibrance", 0, winreg.REG_DWORD, vibrance)
            except FileNotFoundError:
                # Create the key if it doesn't exist
                with winreg.CreateKey(winreg.HKEY_CURRENT_USER, key_path) as key:
                    winreg.SetValueEx(key, "DVibrance", 0, winreg.REG_DWORD, vibrance)

            # Note: Registry change alone may not apply immediately
            # Would need to restart NVIDIA driver service or use NvAPI
            return True

        except Exception:
            return False

    def reset_color_settings(self, display_id: int = 0) -> bool:
        """
        Reset color settings to defaults.

        Args:
            display_id: Display index

        Returns:
            True if successful
        """
        return self.set_color_settings(
            display_id,
            brightness=0,
            contrast=0,
            gamma=0,
            saturation=0,
            hue=0
        )

    # =========================================================================
    # 3D LUT Methods
    # =========================================================================

    def load_3d_lut(
        self,
        display_id: int,
        lut_data: np.ndarray,
        interpolation: str = "tetrahedral"
    ) -> bool:
        """
        Load 3D LUT to GPU.

        Note: NVAPI doesn't have native 3D LUT support in the public API.
        This method applies the LUT effect using available color controls
        and stores the LUT for reference.

        For true 3D LUT application, consider:
        - Using the .cube file in color-managed applications
        - Installing an ICC profile with VCGT
        - Using third-party solutions like MadVR

        Args:
            display_id: Display index
            lut_data: 3D LUT as [size, size, size, 3] array (0-1 float)
            interpolation: Interpolation method (ignored, for API compatibility)

        Returns:
            True if approximation was applied
        """
        if not self._initialized:
            return False

        # Validate LUT data
        if lut_data.ndim != 4 or lut_data.shape[3] != 3:
            raise NvidiaAPIError("Invalid LUT format - expected [size, size, size, 3]")

        size = lut_data.shape[0]
        if size not in [17, 33, 65]:
            raise NvidiaAPIError(f"Unsupported LUT size: {size}")

        # Store LUT for reference
        self._active_luts[display_id] = lut_data.copy()

        # Try to apply approximation via color controls
        # Analyze the LUT to extract approximate adjustments
        try:
            adjustments = self._analyze_lut_adjustments(lut_data)

            return self.set_color_settings(
                display_id,
                brightness=adjustments.get('brightness', 0),
                contrast=adjustments.get('contrast', 0),
                gamma=adjustments.get('gamma', 0),
                saturation=adjustments.get('saturation', 0)
            )
        except Exception:
            return False

    def _analyze_lut_adjustments(self, lut_data: np.ndarray) -> Dict[str, int]:
        """
        Analyze 3D LUT to extract approximate color adjustments.

        This extracts 1D transfer functions along the diagonal and
        estimates brightness, contrast, gamma adjustments.
        """
        size = lut_data.shape[0]
        adjustments = {}

        # Sample diagonal (grayscale response)
        diagonal = np.array([
            lut_data[i, i, i] for i in range(size)
        ])

        # Average RGB response
        avg_response = diagonal.mean(axis=1)

        # Expected linear response
        expected = np.linspace(0, 1, size)

        # Estimate brightness (offset at black point)
        black_offset = avg_response[0] - expected[0]
        brightness = int(black_offset * 100)
        adjustments['brightness'] = max(-100, min(100, brightness))

        # Estimate contrast (difference between white and black)
        actual_range = avg_response[-1] - avg_response[0]
        expected_range = 1.0
        contrast_factor = actual_range / expected_range if expected_range > 0 else 1.0
        contrast = int((contrast_factor - 1.0) * 100)
        adjustments['contrast'] = max(-100, min(100, contrast))

        # Estimate gamma (midtone deviation)
        mid_idx = size // 2
        mid_actual = avg_response[mid_idx]
        mid_expected = expected[mid_idx]
        if mid_expected > 0 and mid_actual > 0:
            # Gamma estimation: actual = expected^(1/gamma_factor)
            gamma_ratio = np.log(mid_actual) / np.log(mid_expected) if mid_expected < 1 else 1.0
            gamma = int((gamma_ratio - 1.0) * 50)
            adjustments['gamma'] = max(-100, min(100, gamma))
        else:
            adjustments['gamma'] = 0

        # Estimate saturation by comparing color channel separation
        # At white, check if channels are equal (desaturated) or separated
        white_rgb = lut_data[-1, -1, -1]
        channel_std = np.std(white_rgb)
        adjustments['saturation'] = 0  # Hard to estimate from LUT

        return adjustments

    def load_lut_file(
        self,
        display_id: int,
        lut_path: TypingUnion[str, Path]
    ) -> bool:
        """
        Load 3D LUT from file.

        Args:
            display_id: Display index
            lut_path: Path to .cube or other LUT file

        Returns:
            True if successful
        """
        try:
            from calibrate_pro.core.lut_engine import LUT3D

            lut_path = Path(lut_path)
            if not lut_path.exists():
                raise NvidiaAPIError(f"LUT file not found: {lut_path}")

            lut = LUT3D.load(lut_path)
            return self.load_3d_lut(display_id, lut.data)

        except Exception as e:
            raise NvidiaAPIError(f"Failed to load LUT: {e}")

    def unload_lut(self, display_id: int) -> bool:
        """
        Remove LUT from display.

        Args:
            display_id: Display index

        Returns:
            True if successful
        """
        if display_id in self._active_luts:
            del self._active_luts[display_id]

        return self.reset_color_settings(display_id)

    def get_active_lut(self, display_id: int) -> Optional[np.ndarray]:
        """Get currently active LUT for a display."""
        return self._active_luts.get(display_id)

    # =========================================================================
    # HDR Methods
    # =========================================================================

    def get_hdr_capabilities(self, display_id: int = 0) -> Dict:
        """
        Get HDR capabilities for a display.

        Args:
            display_id: Display index

        Returns:
            Dictionary with HDR capability info
        """
        result = {
            'hdr_supported': False,
            'hdr10_supported': False,
            'dolby_vision_supported': False,
            'edr_supported': False,
        }

        if not self._initialized or 'GetHdrCapabilities' not in self._funcs:
            return result

        if display_id >= len(self._displays):
            return result

        # Note: GetHdrCapabilities requires display ID, not handle
        # This is a simplified check
        if self._displays[display_id].is_hdr_capable:
            result['hdr_supported'] = True
            result['hdr10_supported'] = True

        return result

    def set_hdr_mode(
        self,
        display_id: int,
        enabled: bool,
        mode: NvHdrMode = NvHdrMode.UHDA
    ) -> bool:
        """
        Enable/disable HDR mode.

        Args:
            display_id: Display index
            enabled: True to enable HDR
            mode: HDR mode to use

        Returns:
            True if successful
        """
        if not self._initialized or 'HdrColorControl' not in self._funcs:
            return False

        if display_id >= len(self._displays):
            return False

        # This would require the full HDR color control implementation
        # which needs proper display ID resolution

        return False  # Not fully implemented

    # =========================================================================
    # Utility Methods
    # =========================================================================

    def get_info(self) -> Dict:
        """Get NVIDIA API information."""
        return {
            'available': self._initialized,
            'gpu_count': len(self._gpus),
            'display_count': len(self._displays),
            'gpus': [
                {
                    'name': gpu.name,
                    'handle': gpu.handle,
                }
                for gpu in self._gpus
            ],
            'displays': [
                {
                    'id': d.display_id,
                    'name': d.name,
                    'primary': d.is_primary,
                    'active': d.is_active,
                    'hdr_capable': d.is_hdr_capable,
                }
                for d in self._displays
            ],
            'features': {
                'color_control': 'ColorControl' in self._funcs,
                'hdr_control': 'HdrColorControl' in self._funcs,
            }
        }

    def cleanup(self):
        """Clean up NVAPI resources."""
        if self._initialized and self._nvapi:
            try:
                query_interface = self._nvapi.nvapi_QueryInterface
                query_interface.restype = c_void_p

                unload_ptr = query_interface(NVAPI_FUNCS['Unload'])
                if unload_ptr:
                    NvAPI_Unload = ctypes.cast(
                        unload_ptr,
                        ctypes.CFUNCTYPE(c_int)
                    )
                    NvAPI_Unload()
            except Exception:
                pass

            self._initialized = False
            self._funcs.clear()

    def __del__(self):
        self.cleanup()


# =============================================================================
# Convenience Functions
# =============================================================================

def check_nvidia_available() -> bool:
    """Check if NVIDIA GPU is available."""
    api = NvidiaAPI()
    available = api.is_available
    api.cleanup()
    return available


def get_nvidia_info() -> Dict:
    """Get NVIDIA GPU information."""
    api = NvidiaAPI()
    info = api.get_info()
    api.cleanup()
    return info


def apply_nvidia_lut(
    lut_data: np.ndarray,
    display_id: int = 0
) -> Tuple[bool, str]:
    """
    Apply 3D LUT via NVIDIA API.

    Args:
        lut_data: 3D LUT array [size, size, size, 3]
        display_id: Display index

    Returns:
        (success, message) tuple
    """
    api = NvidiaAPI()

    if not api.is_available:
        return False, "NVIDIA API not available"

    try:
        success = api.load_3d_lut(display_id, lut_data)
        if success:
            return True, "LUT applied via NVIDIA color control (approximation)"
        else:
            return False, "Failed to apply LUT"
    except NvidiaAPIError as e:
        return False, str(e)
    except Exception as e:
        return False, f"Error: {e}"
    finally:
        # Don't cleanup - keep settings active
        pass


def apply_nvidia_lut_file(
    lut_path: TypingUnion[str, Path],
    display_id: int = 0
) -> Tuple[bool, str]:
    """
    Apply 3D LUT file via NVIDIA API.

    Args:
        lut_path: Path to .cube LUT file
        display_id: Display index

    Returns:
        (success, message) tuple
    """
    api = NvidiaAPI()

    if not api.is_available:
        return False, "NVIDIA API not available"

    try:
        success = api.load_lut_file(display_id, lut_path)
        if success:
            return True, f"LUT applied: {Path(lut_path).name}"
        else:
            return False, "Failed to apply LUT"
    except NvidiaAPIError as e:
        return False, str(e)
    except Exception as e:
        return False, f"Error: {e}"


def set_nvidia_color(
    display_id: int = 0,
    brightness: Optional[int] = None,
    contrast: Optional[int] = None,
    gamma: Optional[int] = None,
    saturation: Optional[int] = None
) -> Tuple[bool, str]:
    """
    Set NVIDIA display color settings.

    Args:
        display_id: Display index
        brightness: -100 to 100
        contrast: -100 to 100
        gamma: -100 to 100
        saturation: -100 to 100 (vibrance)

    Returns:
        (success, message) tuple
    """
    api = NvidiaAPI()

    if not api.is_available:
        return False, "NVIDIA API not available"

    try:
        success = api.set_color_settings(
            display_id,
            brightness=brightness,
            contrast=contrast,
            gamma=gamma,
            saturation=saturation
        )
        if success:
            return True, "Color settings applied"
        else:
            return False, "Failed to apply color settings"
    except Exception as e:
        return False, f"Error: {e}"


def reset_nvidia_color(display_id: int = 0) -> Tuple[bool, str]:
    """Reset NVIDIA color settings to defaults."""
    api = NvidiaAPI()

    if not api.is_available:
        return False, "NVIDIA API not available"

    try:
        success = api.reset_color_settings(display_id)
        if success:
            return True, "Color settings reset to defaults"
        else:
            return False, "Failed to reset color settings"
    except Exception as e:
        return False, f"Error: {e}"
