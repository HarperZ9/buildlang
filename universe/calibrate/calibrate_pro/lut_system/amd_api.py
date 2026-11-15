"""
AMD ADL Integration for GPU Color Management

Provides GPU-level color control via AMD Display Library (ADL):
- Direct color control (brightness, contrast, gamma, saturation)
- Per-display color calibration
- 3D LUT via color control approximation
- Registry-based Radeon Software integration

Requirements:
- AMD GPU with Radeon Software
- atiadlxx.dll (included with AMD drivers)
"""

import ctypes
from ctypes import wintypes, POINTER, Structure, c_int, c_uint, c_float
from ctypes import c_void_p, c_char, byref, sizeof, cast
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Union as TypingUnion
import struct
import numpy as np
from enum import IntEnum
from dataclasses import dataclass
import sys
import winreg


# =============================================================================
# ADL Constants and Types
# =============================================================================

class ADLStatus(IntEnum):
    """ADL return status codes."""
    OK = 0
    ERR = -1
    ERR_NOT_INIT = -2
    ERR_INVALID_PARAM = -3
    ERR_INVALID_PARAM_SIZE = -4
    ERR_INVALID_ADL_IDX = -5
    ERR_INVALID_CONTROLLER_IDX = -6
    ERR_INVALID_DISPLAY_IDX = -7
    ERR_NOT_SUPPORTED = -8
    ERR_NULL_POINTER = -9
    ERR_DISABLED_ADAPTER = -10
    ERR_INVALID_CALLBACK = -11
    ERR_RESOURCE_CONFLICT = -12
    ERR_SET_INCOMPLETE = -20
    ERR_NO_XDISPLAY = -21


class ADLColorType(IntEnum):
    """ADL color control types."""
    BRIGHTNESS = 1
    CONTRAST = 2
    SATURATION = 3
    HUE = 4
    COLORTEMP = 5
    GAMMA = 6
    CUSTOM_COLOR_TEMP = 7
    VISION_DEFICIENCY_MODE = 8


class ADLColorDepth(IntEnum):
    """Color depth options."""
    BPC_6 = 6
    BPC_8 = 8
    BPC_10 = 10
    BPC_12 = 12
    BPC_16 = 16


class ADLPixelFormat(IntEnum):
    """Pixel format options."""
    RGB = 0
    YCRCB444 = 1
    YCRCB422 = 2
    YCRCB420 = 3


# ADL Memory allocation callback type
ADL_MAIN_MALLOC_CALLBACK = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_int)


# =============================================================================
# ADL Structures
# =============================================================================

class ADLAdapterInfo(Structure):
    """ADL adapter information structure."""
    _fields_ = [
        ('iSize', c_int),
        ('iAdapterIndex', c_int),
        ('strUDID', c_char * 256),
        ('iBusNumber', c_int),
        ('iDeviceNumber', c_int),
        ('iFunctionNumber', c_int),
        ('iVendorID', c_int),
        ('strAdapterName', c_char * 256),
        ('strDisplayName', c_char * 256),
        ('iPresent', c_int),
        ('iExist', c_int),
        ('strDriverPath', c_char * 256),
        ('strDriverPathExt', c_char * 256),
        ('strPNPString', c_char * 256),
        ('iOSDisplayIndex', c_int),
    ]


class ADLDisplayInfo(Structure):
    """ADL display information structure."""
    _fields_ = [
        ('displayID', c_int * 2),  # ADLDisplayID
        ('iDisplayControllerIndex', c_int),
        ('strDisplayName', c_char * 256),
        ('strDisplayManufacturerName', c_char * 256),
        ('iDisplayType', c_int),
        ('iDisplayOutputType', c_int),
        ('iDisplayConnector', c_int),
        ('iDisplayInfoMask', c_int),
        ('iDisplayInfoValue', c_int),
    ]


class ADLDisplayID(Structure):
    """ADL display ID structure."""
    _fields_ = [
        ('iDisplayLogicalIndex', c_int),
        ('iDisplayPhysicalIndex', c_int),
        ('iDisplayLogicalAdapterIndex', c_int),
        ('iDisplayPhysicalAdapterIndex', c_int),
    ]


class ADLGamma(Structure):
    """ADL gamma structure."""
    _fields_ = [
        ('fRed', c_float),
        ('fGreen', c_float),
        ('fBlue', c_float),
    ]


class ADLColorValue(Structure):
    """ADL color value structure (for brightness, contrast, saturation, hue)."""
    _fields_ = [
        ('iCurrent', c_int),
        ('iDefault', c_int),
        ('iMin', c_int),
        ('iMax', c_int),
        ('iStep', c_int),
    ]


class ADLDisplayColorCaps(Structure):
    """ADL display color capabilities."""
    _fields_ = [
        ('iColorType', c_int),  # Type of color setting
        ('iExpColorCaps', c_int),  # Extended capabilities
        ('iReserved1', c_int),
        ('iReserved2', c_int),
    ]


class ADLCustomMode(Structure):
    """ADL custom color mode structure."""
    _fields_ = [
        ('iFlags', c_int),
        ('iModeWidth', c_int),
        ('iModeHeight', c_int),
        ('iBaseModeWidth', c_int),
        ('iBaseModeHeight', c_int),
        ('iRefreshRate', c_int),
    ]


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class AMDDisplay:
    """AMD display information."""
    adapter_index: int
    display_index: int
    logical_index: int
    name: str
    manufacturer: str
    is_primary: bool
    is_connected: bool
    is_active: bool
    resolution: Tuple[int, int]
    refresh_rate: float
    color_depth: int
    display_type: str
    connector_type: str
    hdr_supported: bool


@dataclass
class AMDAdapter:
    """AMD adapter (GPU) information."""
    index: int
    name: str
    display_name: str
    is_present: bool
    is_active: bool
    vendor_id: int
    bus_number: int


@dataclass
class ColorSettings:
    """Display color settings."""
    brightness: int = 50   # 0 to 100
    contrast: int = 50     # 0 to 100
    saturation: int = 100  # 0 to 200
    hue: int = 0           # 0 to 360
    color_temp: int = 6500 # Color temperature in Kelvin
    gamma: float = 1.0     # 0.4 to 2.8


class AMDAPIError(Exception):
    """AMD API error."""
    pass


# =============================================================================
# Memory Allocation
# =============================================================================

# Storage for allocated memory to prevent garbage collection
_allocated_memory = []


def _adl_malloc(size: int) -> int:
    """Memory allocation callback for ADL."""
    buf = ctypes.create_string_buffer(size)
    _allocated_memory.append(buf)
    return ctypes.addressof(buf)


# =============================================================================
# Main AMD API Class
# =============================================================================

class AMDAPI:
    """
    AMD ADL wrapper for GPU-level color management.

    Provides:
    - Color control (brightness, contrast, saturation, hue, color temp)
    - Gamma control
    - Display enumeration
    - GPU information
    - 3D LUT application (via color control approximation)
    """

    def __init__(self):
        self._initialized = False
        self._adl = None
        self._adl2 = None
        self._context = None
        self._malloc_callback = None
        self._adapters: List[AMDAdapter] = []
        self._displays: List[AMDDisplay] = []
        self._active_luts: Dict[int, np.ndarray] = {}

        self._initialize()

    def _initialize(self) -> bool:
        """Initialize ADL library."""
        try:
            # Try to load ADL DLL
            if sys.maxsize > 2**32:
                dll_names = ["atiadlxx.dll", "amdadl64.dll"]
            else:
                dll_names = ["atiadlxy.dll", "amdadl32.dll"]

            for dll_name in dll_names:
                try:
                    self._adl = ctypes.CDLL(dll_name)
                    break
                except OSError:
                    continue

            if not self._adl:
                return False

            # Create malloc callback
            self._malloc_callback = ADL_MAIN_MALLOC_CALLBACK(_adl_malloc)

            # Initialize ADL2 (preferred) or ADL
            if self._try_init_adl2():
                self._initialized = True
            elif self._try_init_adl1():
                self._initialized = True
            else:
                return False

            # Enumerate adapters and displays
            self._enumerate_adapters()
            self._enumerate_displays()

            return True

        except Exception as e:
            return False

    def _try_init_adl2(self) -> bool:
        """Try to initialize ADL version 2."""
        try:
            if not hasattr(self._adl, 'ADL2_Main_Control_Create'):
                return False

            ADL2_Main_Control_Create = self._adl.ADL2_Main_Control_Create
            ADL2_Main_Control_Create.restype = c_int
            ADL2_Main_Control_Create.argtypes = [
                ADL_MAIN_MALLOC_CALLBACK,
                c_int,
                POINTER(c_void_p)
            ]

            context = c_void_p()
            status = ADL2_Main_Control_Create(
                self._malloc_callback,
                1,  # Retrieve adapter information for all adapters
                byref(context)
            )

            if status == ADLStatus.OK:
                self._context = context
                self._adl2 = self._adl
                return True

            return False

        except Exception:
            return False

    def _try_init_adl1(self) -> bool:
        """Try to initialize ADL version 1."""
        try:
            if not hasattr(self._adl, 'ADL_Main_Control_Create'):
                return False

            ADL_Main_Control_Create = self._adl.ADL_Main_Control_Create
            ADL_Main_Control_Create.restype = c_int
            ADL_Main_Control_Create.argtypes = [ADL_MAIN_MALLOC_CALLBACK, c_int]

            status = ADL_Main_Control_Create(self._malloc_callback, 1)
            return status == ADLStatus.OK

        except Exception:
            return False

    def _enumerate_adapters(self):
        """Enumerate AMD GPU adapters."""
        if not self._initialized:
            return

        try:
            # Get number of adapters
            num_adapters = c_int(0)

            if hasattr(self._adl, 'ADL_Adapter_NumberOfAdapters_Get'):
                self._adl.ADL_Adapter_NumberOfAdapters_Get.restype = c_int
                self._adl.ADL_Adapter_NumberOfAdapters_Get.argtypes = [POINTER(c_int)]
                self._adl.ADL_Adapter_NumberOfAdapters_Get(byref(num_adapters))

            if num_adapters.value == 0:
                return

            # Get adapter info
            adapter_info_array = (ADLAdapterInfo * num_adapters.value)()

            if hasattr(self._adl, 'ADL_Adapter_AdapterInfo_Get'):
                self._adl.ADL_Adapter_AdapterInfo_Get.restype = c_int
                self._adl.ADL_Adapter_AdapterInfo_Get.argtypes = [
                    POINTER(ADLAdapterInfo * num_adapters.value),
                    c_int
                ]
                status = self._adl.ADL_Adapter_AdapterInfo_Get(
                    byref(adapter_info_array),
                    sizeof(adapter_info_array)
                )

                if status == ADLStatus.OK:
                    for i in range(num_adapters.value):
                        info = adapter_info_array[i]

                        # Check if adapter is active
                        is_active = False
                        if hasattr(self._adl, 'ADL_Adapter_Active_Get'):
                            active = c_int(0)
                            self._adl.ADL_Adapter_Active_Get.restype = c_int
                            self._adl.ADL_Adapter_Active_Get.argtypes = [c_int, POINTER(c_int)]
                            if self._adl.ADL_Adapter_Active_Get(i, byref(active)) == ADLStatus.OK:
                                is_active = bool(active.value)

                        if info.iPresent or is_active:
                            self._adapters.append(AMDAdapter(
                                index=info.iAdapterIndex,
                                name=info.strAdapterName.decode('utf-8', errors='ignore').strip(),
                                display_name=info.strDisplayName.decode('utf-8', errors='ignore').strip(),
                                is_present=bool(info.iPresent),
                                is_active=is_active,
                                vendor_id=info.iVendorID,
                                bus_number=info.iBusNumber
                            ))

        except Exception:
            pass

    def _enumerate_displays(self):
        """Enumerate connected displays."""
        if not self._initialized or not self._adapters:
            self._enumerate_displays_windows()
            return

        try:
            for adapter in self._adapters:
                if not adapter.is_active and not adapter.is_present:
                    continue

                # Get number of displays for this adapter
                num_displays = c_int(0)

                if hasattr(self._adl, 'ADL_Display_NumberOfDisplays_Get'):
                    self._adl.ADL_Display_NumberOfDisplays_Get.restype = c_int
                    self._adl.ADL_Display_NumberOfDisplays_Get.argtypes = [c_int, POINTER(c_int)]
                    self._adl.ADL_Display_NumberOfDisplays_Get(adapter.index, byref(num_displays))

                if num_displays.value == 0:
                    continue

                # Get display info
                display_info_array = (ADLDisplayInfo * num_displays.value)()
                actual_displays = c_int(0)

                if hasattr(self._adl, 'ADL_Display_DisplayInfo_Get'):
                    self._adl.ADL_Display_DisplayInfo_Get.restype = c_int
                    self._adl.ADL_Display_DisplayInfo_Get.argtypes = [
                        c_int, POINTER(c_int), POINTER(ADLDisplayInfo * num_displays.value), c_int
                    ]
                    status = self._adl.ADL_Display_DisplayInfo_Get(
                        adapter.index,
                        byref(actual_displays),
                        byref(display_info_array),
                        0  # Force refresh
                    )

                    if status == ADLStatus.OK:
                        for j in range(actual_displays.value):
                            info = display_info_array[j]

                            # Check if display is connected and active
                            is_connected = bool(info.iDisplayInfoValue & 0x01)
                            is_active = bool(info.iDisplayInfoValue & 0x02)
                            is_primary = (adapter.index == 0 and j == 0)

                            if is_connected:
                                self._displays.append(AMDDisplay(
                                    adapter_index=adapter.index,
                                    display_index=j,
                                    logical_index=info.displayID[0],
                                    name=info.strDisplayName.decode('utf-8', errors='ignore').strip(),
                                    manufacturer=info.strDisplayManufacturerName.decode('utf-8', errors='ignore').strip(),
                                    is_primary=is_primary,
                                    is_connected=is_connected,
                                    is_active=is_active,
                                    resolution=(0, 0),
                                    refresh_rate=60.0,
                                    color_depth=8,
                                    display_type=self._get_display_type(info.iDisplayType),
                                    connector_type=self._get_connector_type(info.iDisplayConnector),
                                    hdr_supported=False
                                ))

        except Exception:
            # Fall back to Windows enumeration
            self._enumerate_displays_windows()

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
                    is_amd = any(x in device.DeviceID.lower() for x in ["amd", "ati", "radeon", "advanced micro"])
                    if is_amd:
                        self._displays.append(AMDDisplay(
                            adapter_index=0,
                            display_index=i,
                            logical_index=i,
                            name=device.DeviceString,
                            manufacturer="AMD",
                            is_primary=bool(device.StateFlags & 0x00000004),
                            is_connected=True,
                            is_active=True,
                            resolution=(0, 0),
                            refresh_rate=60.0,
                            color_depth=8,
                            display_type="Unknown",
                            connector_type="Unknown",
                            hdr_supported=False
                        ))
                i += 1

        except Exception:
            pass

    def _get_display_type(self, display_type: int) -> str:
        """Convert display type integer to string."""
        types = {
            0: "Monitor",
            1: "TV",
            2: "CRT",
            3: "Component",
            4: "Projector",
        }
        return types.get(display_type, "Unknown")

    def _get_connector_type(self, connector: int) -> str:
        """Convert connector type integer to string."""
        connectors = {
            0: "Unknown",
            1: "VGA",
            2: "DVI-D",
            3: "DVI-I",
            4: "CVBS",
            5: "YPbPr",
            6: "HDMI",
            7: "DisplayPort",
            8: "Mini DisplayPort",
            9: "USB-C",
        }
        return connectors.get(connector, "Unknown")

    # =========================================================================
    # Public Properties
    # =========================================================================

    @property
    def is_available(self) -> bool:
        """Check if AMD ADL is available."""
        return self._initialized

    @property
    def adapters(self) -> List[AMDAdapter]:
        """Get list of AMD adapters."""
        return self._adapters

    @property
    def displays(self) -> List[AMDDisplay]:
        """Get list of AMD displays."""
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
        # Try ADL first
        settings = self._get_color_via_adl(display_id)
        if settings:
            return settings

        # Fall back to registry (Radeon Software settings)
        return self._get_color_via_registry(display_id)

    def _get_color_via_adl(self, display_id: int) -> Optional[ColorSettings]:
        """Get color settings via ADL."""
        if not self._initialized:
            return None

        if display_id >= len(self._displays):
            return None

        display = self._displays[display_id]
        settings = ColorSettings()

        try:
            # Get brightness
            if hasattr(self._adl, 'ADL_Display_Color_Get'):
                self._adl.ADL_Display_Color_Get.restype = c_int
                self._adl.ADL_Display_Color_Get.argtypes = [
                    c_int, c_int, c_int,
                    POINTER(c_int), POINTER(c_int), POINTER(c_int), POINTER(c_int), POINTER(c_int)
                ]

                current = c_int(0)
                default = c_int(0)
                min_val = c_int(0)
                max_val = c_int(0)
                step = c_int(1)

                # Brightness
                status = self._adl.ADL_Display_Color_Get(
                    display.adapter_index, display.logical_index, ADLColorType.BRIGHTNESS,
                    byref(current), byref(default), byref(min_val), byref(max_val), byref(step)
                )
                if status == ADLStatus.OK:
                    settings.brightness = current.value

                # Contrast
                status = self._adl.ADL_Display_Color_Get(
                    display.adapter_index, display.logical_index, ADLColorType.CONTRAST,
                    byref(current), byref(default), byref(min_val), byref(max_val), byref(step)
                )
                if status == ADLStatus.OK:
                    settings.contrast = current.value

                # Saturation
                status = self._adl.ADL_Display_Color_Get(
                    display.adapter_index, display.logical_index, ADLColorType.SATURATION,
                    byref(current), byref(default), byref(min_val), byref(max_val), byref(step)
                )
                if status == ADLStatus.OK:
                    settings.saturation = current.value

                # Hue
                status = self._adl.ADL_Display_Color_Get(
                    display.adapter_index, display.logical_index, ADLColorType.HUE,
                    byref(current), byref(default), byref(min_val), byref(max_val), byref(step)
                )
                if status == ADLStatus.OK:
                    settings.hue = current.value

                # Color Temperature
                status = self._adl.ADL_Display_Color_Get(
                    display.adapter_index, display.logical_index, ADLColorType.COLORTEMP,
                    byref(current), byref(default), byref(min_val), byref(max_val), byref(step)
                )
                if status == ADLStatus.OK:
                    settings.color_temp = current.value

            # Get gamma
            if hasattr(self._adl, 'ADL_Display_Gamma_Get'):
                self._adl.ADL_Display_Gamma_Get.restype = c_int
                self._adl.ADL_Display_Gamma_Get.argtypes = [
                    c_int, c_int, POINTER(ADLGamma)
                ]

                gamma = ADLGamma()
                status = self._adl.ADL_Display_Gamma_Get(
                    display.adapter_index, display.logical_index, byref(gamma)
                )
                if status == ADLStatus.OK:
                    # Average the RGB gamma values
                    settings.gamma = (gamma.fRed + gamma.fGreen + gamma.fBlue) / 3.0

            return settings

        except Exception:
            return None

    def _get_color_via_registry(self, display_id: int) -> Optional[ColorSettings]:
        """Get color settings from Radeon Software registry."""
        try:
            # Radeon Software stores settings in registry
            key_paths = [
                r"SOFTWARE\AMD\CN\Display",
                r"SOFTWARE\ATI\ACE\Display",
                r"SOFTWARE\AMD\MPC\ColorSettings",
            ]

            settings = ColorSettings()

            for key_path in key_paths:
                try:
                    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, key_path) as key:
                        try:
                            brightness, _ = winreg.QueryValueEx(key, "Brightness")
                            settings.brightness = int(brightness)
                        except FileNotFoundError:
                            pass
                        try:
                            contrast, _ = winreg.QueryValueEx(key, "Contrast")
                            settings.contrast = int(contrast)
                        except FileNotFoundError:
                            pass
                        try:
                            saturation, _ = winreg.QueryValueEx(key, "Saturation")
                            settings.saturation = int(saturation)
                        except FileNotFoundError:
                            pass
                        break
                except FileNotFoundError:
                    continue

            return settings

        except Exception:
            return None

    def set_color_settings(
        self,
        display_id: int = 0,
        brightness: Optional[int] = None,
        contrast: Optional[int] = None,
        saturation: Optional[int] = None,
        hue: Optional[int] = None,
        color_temp: Optional[int] = None,
        gamma: Optional[float] = None
    ) -> bool:
        """
        Set color settings for a display.

        Args:
            display_id: Display index
            brightness: 0 to 100
            contrast: 0 to 100
            saturation: 0 to 200
            hue: 0 to 360
            color_temp: Color temperature in Kelvin (e.g., 6500)
            gamma: 0.4 to 2.8

        Returns:
            True if any setting was applied
        """
        # Try ADL first
        adl_success = self._set_color_via_adl(
            display_id, brightness, contrast, saturation, hue, color_temp, gamma
        )

        # Also try registry for persistence
        registry_success = self._set_color_via_registry(
            display_id, brightness, contrast, saturation
        )

        return adl_success or registry_success

    def _set_color_via_adl(
        self,
        display_id: int,
        brightness: Optional[int],
        contrast: Optional[int],
        saturation: Optional[int],
        hue: Optional[int],
        color_temp: Optional[int],
        gamma: Optional[float]
    ) -> bool:
        """Set color via ADL."""
        if not self._initialized:
            return False

        if display_id >= len(self._displays):
            return False

        display = self._displays[display_id]
        success = False

        try:
            if hasattr(self._adl, 'ADL_Display_Color_Set'):
                self._adl.ADL_Display_Color_Set.restype = c_int
                self._adl.ADL_Display_Color_Set.argtypes = [c_int, c_int, c_int, c_int]

                if brightness is not None:
                    status = self._adl.ADL_Display_Color_Set(
                        display.adapter_index, display.logical_index,
                        ADLColorType.BRIGHTNESS, max(0, min(100, brightness))
                    )
                    if status == ADLStatus.OK:
                        success = True

                if contrast is not None:
                    status = self._adl.ADL_Display_Color_Set(
                        display.adapter_index, display.logical_index,
                        ADLColorType.CONTRAST, max(0, min(100, contrast))
                    )
                    if status == ADLStatus.OK:
                        success = True

                if saturation is not None:
                    status = self._adl.ADL_Display_Color_Set(
                        display.adapter_index, display.logical_index,
                        ADLColorType.SATURATION, max(0, min(200, saturation))
                    )
                    if status == ADLStatus.OK:
                        success = True

                if hue is not None:
                    status = self._adl.ADL_Display_Color_Set(
                        display.adapter_index, display.logical_index,
                        ADLColorType.HUE, hue % 360
                    )
                    if status == ADLStatus.OK:
                        success = True

                if color_temp is not None:
                    status = self._adl.ADL_Display_Color_Set(
                        display.adapter_index, display.logical_index,
                        ADLColorType.COLORTEMP, color_temp
                    )
                    if status == ADLStatus.OK:
                        success = True

            # Set gamma separately
            if gamma is not None and hasattr(self._adl, 'ADL_Display_Gamma_Set'):
                self._adl.ADL_Display_Gamma_Set.restype = c_int
                self._adl.ADL_Display_Gamma_Set.argtypes = [c_int, c_int, POINTER(ADLGamma)]

                gamma_struct = ADLGamma()
                gamma_val = max(0.4, min(2.8, gamma))
                gamma_struct.fRed = c_float(gamma_val)
                gamma_struct.fGreen = c_float(gamma_val)
                gamma_struct.fBlue = c_float(gamma_val)

                status = self._adl.ADL_Display_Gamma_Set(
                    display.adapter_index, display.logical_index, byref(gamma_struct)
                )
                if status == ADLStatus.OK:
                    success = True

            return success

        except Exception:
            return False

    def _set_color_via_registry(
        self,
        display_id: int,
        brightness: Optional[int],
        contrast: Optional[int],
        saturation: Optional[int]
    ) -> bool:
        """Set color via Radeon Software registry (for persistence)."""
        try:
            key_path = r"SOFTWARE\AMD\CN\Display"

            try:
                with winreg.OpenKey(
                    winreg.HKEY_LOCAL_MACHINE, key_path, 0,
                    winreg.KEY_SET_VALUE | winreg.KEY_WOW64_64KEY
                ) as key:
                    if brightness is not None:
                        winreg.SetValueEx(key, "Brightness", 0, winreg.REG_DWORD, brightness)
                    if contrast is not None:
                        winreg.SetValueEx(key, "Contrast", 0, winreg.REG_DWORD, contrast)
                    if saturation is not None:
                        winreg.SetValueEx(key, "Saturation", 0, winreg.REG_DWORD, saturation)
                    return True
            except PermissionError:
                # Try current user instead
                with winreg.CreateKey(winreg.HKEY_CURRENT_USER, key_path) as key:
                    if brightness is not None:
                        winreg.SetValueEx(key, "Brightness", 0, winreg.REG_DWORD, brightness)
                    if contrast is not None:
                        winreg.SetValueEx(key, "Contrast", 0, winreg.REG_DWORD, contrast)
                    if saturation is not None:
                        winreg.SetValueEx(key, "Saturation", 0, winreg.REG_DWORD, saturation)
                    return True

        except Exception:
            return False

    def reset_color_settings(self, display_id: int = 0) -> bool:
        """Reset color settings to defaults."""
        return self.set_color_settings(
            display_id,
            brightness=50,
            contrast=50,
            saturation=100,
            hue=0,
            color_temp=6500,
            gamma=1.0
        )

    # =========================================================================
    # Gamma Ramp Methods
    # =========================================================================

    def load_gamma_ramp(
        self,
        display_id: int,
        red: np.ndarray,
        green: np.ndarray,
        blue: np.ndarray
    ) -> bool:
        """
        Load gamma ramp (1D LUT) via Windows API.

        AMD ADL doesn't expose gamma ramp directly, so we use Windows GDI.

        Args:
            display_id: Display index
            red, green, blue: 256-element arrays (0-65535)

        Returns:
            True if successful
        """
        try:
            from calibrate_pro.panels.detection import set_gamma_ramp, enumerate_displays

            displays = enumerate_displays()
            if display_id < len(displays):
                device_name = displays[display_id].device_name
                return set_gamma_ramp(device_name, red, green, blue)

            return False

        except Exception:
            return False

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

        Note: AMD ADL doesn't have native 3D LUT support in the public API.
        This method applies the LUT effect using available color controls
        and gamma ramp.

        Args:
            display_id: Display index
            lut_data: 3D LUT as [size, size, size, 3] array (0-1 float)
            interpolation: Interpolation method

        Returns:
            True if approximation was applied
        """
        if not self._initialized:
            # Fall back to Windows gamma ramp
            return self._apply_lut_via_gamma_ramp(display_id, lut_data)

        # Validate LUT data
        if lut_data.ndim != 4 or lut_data.shape[3] != 3:
            raise AMDAPIError("Invalid LUT format - expected [size, size, size, 3]")

        size = lut_data.shape[0]
        if size not in [17, 33, 65]:
            raise AMDAPIError(f"Unsupported LUT size: {size}")

        # Store LUT for reference
        self._active_luts[display_id] = lut_data.copy()

        # Method 1: Try to apply via ADL color controls
        try:
            adjustments = self._analyze_lut_adjustments(lut_data)

            adl_success = self.set_color_settings(
                display_id,
                brightness=adjustments.get('brightness'),
                contrast=adjustments.get('contrast'),
                saturation=adjustments.get('saturation'),
                gamma=adjustments.get('gamma')
            )

            if adl_success:
                return True
        except Exception:
            pass

        # Method 2: Fall back to gamma ramp (1D approximation)
        return self._apply_lut_via_gamma_ramp(display_id, lut_data)

    def _apply_lut_via_gamma_ramp(self, display_id: int, lut_data: np.ndarray) -> bool:
        """Apply 3D LUT via 1D gamma ramp approximation."""
        try:
            size = lut_data.shape[0]

            # Extract 1D curves along diagonal (grayscale response)
            diagonal = np.array([lut_data[i, i, i] for i in range(size)])

            # Interpolate to 256 points
            x_old = np.linspace(0, 1, size)
            x_new = np.linspace(0, 1, 256)

            red_1d = np.interp(x_new, x_old, diagonal[:, 0])
            green_1d = np.interp(x_new, x_old, diagonal[:, 1])
            blue_1d = np.interp(x_new, x_old, diagonal[:, 2])

            # Convert to 16-bit
            red_16 = (red_1d * 65535).astype(np.uint16)
            green_16 = (green_1d * 65535).astype(np.uint16)
            blue_16 = (blue_1d * 65535).astype(np.uint16)

            return self.load_gamma_ramp(display_id, red_16, green_16, blue_16)

        except Exception:
            return False

    def _analyze_lut_adjustments(self, lut_data: np.ndarray) -> Dict:
        """Analyze 3D LUT to extract approximate color adjustments."""
        size = lut_data.shape[0]
        adjustments = {}

        # Sample diagonal (grayscale response)
        diagonal = np.array([lut_data[i, i, i] for i in range(size)])
        avg_response = diagonal.mean(axis=1)

        # Expected linear response
        expected = np.linspace(0, 1, size)

        # Estimate brightness (offset at black point)
        black_offset = avg_response[0]
        brightness = int(50 + black_offset * 50)  # ADL uses 0-100 range
        adjustments['brightness'] = max(0, min(100, brightness))

        # Estimate contrast
        actual_range = avg_response[-1] - avg_response[0]
        contrast = int(50 * actual_range / 0.5) if actual_range < 1 else 50
        adjustments['contrast'] = max(0, min(100, contrast))

        # Estimate gamma
        mid_idx = size // 2
        mid_actual = avg_response[mid_idx]
        mid_expected = expected[mid_idx]
        if mid_expected > 0 and mid_actual > 0:
            gamma_ratio = np.log(mid_actual) / np.log(mid_expected) if mid_expected < 1 else 1.0
            adjustments['gamma'] = max(0.4, min(2.8, gamma_ratio))
        else:
            adjustments['gamma'] = 1.0

        # Estimate saturation
        # Check color separation at saturated points
        red_sat = lut_data[-1, 0, 0]
        green_sat = lut_data[0, -1, 0]
        blue_sat = lut_data[0, 0, -1]

        avg_sat = (np.max(red_sat) + np.max(green_sat) + np.max(blue_sat)) / 3
        saturation = int(100 * (avg_sat + 1))
        adjustments['saturation'] = max(0, min(200, saturation))

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
                raise AMDAPIError(f"LUT file not found: {lut_path}")

            lut = LUT3D.load(lut_path)
            return self.load_3d_lut(display_id, lut.data)

        except Exception as e:
            raise AMDAPIError(f"Failed to load LUT: {e}")

    def unload_lut(self, display_id: int) -> bool:
        """Remove LUT from display."""
        if display_id in self._active_luts:
            del self._active_luts[display_id]

        return self.reset_color_settings(display_id)

    def get_active_lut(self, display_id: int) -> Optional[np.ndarray]:
        """Get currently active LUT for a display."""
        return self._active_luts.get(display_id)

    # =========================================================================
    # Color Depth and Format
    # =========================================================================

    def set_color_depth(
        self,
        display_id: int,
        depth: ADLColorDepth
    ) -> bool:
        """
        Set display color depth.

        Args:
            display_id: Display index
            depth: Color depth (6, 8, 10, 12, or 16 bits)

        Returns:
            True if successful
        """
        if not self._initialized:
            return False

        if display_id >= len(self._displays):
            return False

        display = self._displays[display_id]

        try:
            if hasattr(self._adl, 'ADL_Display_ColorDepth_Set'):
                self._adl.ADL_Display_ColorDepth_Set.restype = c_int
                self._adl.ADL_Display_ColorDepth_Set.argtypes = [c_int, c_int, c_int]

                status = self._adl.ADL_Display_ColorDepth_Set(
                    display.adapter_index, display.logical_index, depth.value
                )
                return status == ADLStatus.OK

        except Exception:
            pass

        return False

    def get_color_depth(self, display_id: int = 0) -> Optional[int]:
        """Get current color depth for a display."""
        if not self._initialized:
            return None

        if display_id >= len(self._displays):
            return None

        display = self._displays[display_id]

        try:
            if hasattr(self._adl, 'ADL_Display_ColorDepth_Get'):
                self._adl.ADL_Display_ColorDepth_Get.restype = c_int
                self._adl.ADL_Display_ColorDepth_Get.argtypes = [
                    c_int, c_int, POINTER(c_int), POINTER(c_int), POINTER(c_int)
                ]

                current = c_int(0)
                default = c_int(0)
                supported = c_int(0)

                status = self._adl.ADL_Display_ColorDepth_Get(
                    display.adapter_index, display.logical_index,
                    byref(current), byref(default), byref(supported)
                )

                if status == ADLStatus.OK:
                    return current.value

        except Exception:
            pass

        return None

    # =========================================================================
    # Utility Methods
    # =========================================================================

    def get_info(self) -> Dict:
        """Get AMD API information."""
        return {
            'available': self._initialized,
            'adapter_count': len(self._adapters),
            'display_count': len(self._displays),
            'adapters': [
                {
                    'index': a.index,
                    'name': a.name,
                    'active': a.is_active,
                }
                for a in self._adapters
            ],
            'displays': [
                {
                    'id': d.display_index,
                    'name': d.name,
                    'manufacturer': d.manufacturer,
                    'primary': d.is_primary,
                    'active': d.is_active,
                    'connector': d.connector_type,
                    'hdr': d.hdr_supported,
                }
                for d in self._displays
            ],
            'features': {
                'color_control': self._initialized and hasattr(self._adl, 'ADL_Display_Color_Set'),
                'gamma_control': self._initialized and hasattr(self._adl, 'ADL_Display_Gamma_Set'),
                'color_depth': self._initialized and hasattr(self._adl, 'ADL_Display_ColorDepth_Set'),
            }
        }

    def cleanup(self):
        """Clean up ADL resources."""
        if self._initialized:
            try:
                if self._adl2 and self._context:
                    if hasattr(self._adl, 'ADL2_Main_Control_Destroy'):
                        self._adl.ADL2_Main_Control_Destroy(self._context)
                elif self._adl:
                    if hasattr(self._adl, 'ADL_Main_Control_Destroy'):
                        self._adl.ADL_Main_Control_Destroy()
            except Exception:
                pass

            self._initialized = False
            _allocated_memory.clear()

    def __del__(self):
        self.cleanup()


# =============================================================================
# Convenience Functions
# =============================================================================

def check_amd_available() -> bool:
    """Check if AMD GPU is available."""
    api = AMDAPI()
    available = api.is_available
    api.cleanup()
    return available


def get_amd_info() -> Dict:
    """Get AMD GPU information."""
    api = AMDAPI()
    info = api.get_info()
    api.cleanup()
    return info


def apply_amd_lut(
    lut_data: np.ndarray,
    display_id: int = 0
) -> Tuple[bool, str]:
    """
    Apply 3D LUT via AMD API.

    Args:
        lut_data: 3D LUT array [size, size, size, 3]
        display_id: Display index

    Returns:
        (success, message) tuple
    """
    api = AMDAPI()

    if not api.is_available:
        return False, "AMD ADL not available"

    try:
        success = api.load_3d_lut(display_id, lut_data)
        if success:
            return True, "LUT applied via AMD color control"
        else:
            return False, "Failed to apply LUT"
    except AMDAPIError as e:
        return False, str(e)
    except Exception as e:
        return False, f"Error: {e}"


def apply_amd_lut_file(
    lut_path: TypingUnion[str, Path],
    display_id: int = 0
) -> Tuple[bool, str]:
    """
    Apply 3D LUT file via AMD API.

    Args:
        lut_path: Path to .cube LUT file
        display_id: Display index

    Returns:
        (success, message) tuple
    """
    api = AMDAPI()

    if not api.is_available:
        return False, "AMD ADL not available"

    try:
        success = api.load_lut_file(display_id, lut_path)
        if success:
            return True, f"LUT applied: {Path(lut_path).name}"
        else:
            return False, "Failed to apply LUT"
    except AMDAPIError as e:
        return False, str(e)
    except Exception as e:
        return False, f"Error: {e}"


def set_amd_color(
    display_id: int = 0,
    brightness: Optional[int] = None,
    contrast: Optional[int] = None,
    saturation: Optional[int] = None,
    gamma: Optional[float] = None
) -> Tuple[bool, str]:
    """
    Set AMD display color settings.

    Args:
        display_id: Display index
        brightness: 0 to 100
        contrast: 0 to 100
        saturation: 0 to 200
        gamma: 0.4 to 2.8

    Returns:
        (success, message) tuple
    """
    api = AMDAPI()

    if not api.is_available:
        return False, "AMD ADL not available"

    try:
        success = api.set_color_settings(
            display_id,
            brightness=brightness,
            contrast=contrast,
            saturation=saturation,
            gamma=gamma
        )
        if success:
            return True, "Color settings applied"
        else:
            return False, "Failed to apply color settings"
    except Exception as e:
        return False, f"Error: {e}"


def reset_amd_color(display_id: int = 0) -> Tuple[bool, str]:
    """Reset AMD color settings to defaults."""
    api = AMDAPI()

    if not api.is_available:
        return False, "AMD ADL not available"

    try:
        success = api.reset_color_settings(display_id)
        if success:
            return True, "Color settings reset to defaults"
        else:
            return False, "Failed to reset color settings"
    except Exception as e:
        return False, f"Error: {e}"
