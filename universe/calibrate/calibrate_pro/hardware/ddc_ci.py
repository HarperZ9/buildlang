"""
DDC/CI (Display Data Channel Command Interface) - Hardware Monitor Control

Provides direct hardware control of monitor settings via the VESA DDC/CI standard.
This allows calibration at the hardware level before ICC/LUT correction.

Supported adjustments (monitor-dependent):
- Brightness (luminance)
- Contrast
- RGB Gain (color balance)
- RGB Drive/Bias
- Color Temperature presets
- Gamma presets
- Input source selection

For professional monitors with hardware LUT support:
- EIZO ColorEdge (via ColorNavigator protocol)
- NEC SpectraView (via SpectraView protocol)
- BenQ SW series (via Palette Master protocol)
- Dell UltraSharp (via Dell SDK)
"""

import ctypes
from ctypes import wintypes
from typing import Optional, Dict, List, Tuple, Any
from dataclasses import dataclass, field
from enum import IntEnum
from pathlib import Path
import struct


# =============================================================================
# DDC/CI VCP (Virtual Control Panel) Codes - VESA MCCS Standard
# =============================================================================

class VCPCode(IntEnum):
    """
    VESA Monitor Control Command Set (MCCS) VCP codes.

    Comprehensive list of all standard and common manufacturer codes.
    Not all monitors support all codes - use get_vcp to test.
    """
    # =========================================================================
    # Preset Operations (0x00-0x0F)
    # =========================================================================
    CODE_PAGE = 0x00
    DEGAUSS = 0x01
    NEW_CONTROL_VALUE = 0x02
    SOFT_CONTROLS = 0x03
    RESTORE_FACTORY_DEFAULTS = 0x04
    RESTORE_FACTORY_LUMINANCE = 0x05
    RESTORE_FACTORY_CONTRAST = 0x06
    RESTORE_FACTORY_GEOMETRY = 0x08
    RESTORE_FACTORY_COLOR = 0x0A
    COLOR_TEMP_REQUEST = 0x0B
    COLOR_TEMP_INCREMENT = 0x0C

    # =========================================================================
    # Image Adjustment (0x10-0x1F)
    # =========================================================================
    BRIGHTNESS = 0x10          # Luminance
    CONTRAST = 0x12
    BACKLIGHT = 0x13           # Backlight control (LED displays)
    COLOR_PRESET = 0x14        # Color temperature preset
    RED_GAIN = 0x16            # Video gain (drive) - highlights
    GREEN_GAIN = 0x18
    BLUE_GAIN = 0x1A

    # =========================================================================
    # Geometry (0x20-0x4F) - mostly for CRT but some LCDs use for OSD position
    # =========================================================================
    HORIZONTAL_POSITION = 0x20
    HORIZONTAL_SIZE = 0x22
    HORIZONTAL_PINCUSHION = 0x24
    HORIZONTAL_PINCUSHION_BALANCE = 0x26
    HORIZONTAL_CONVERGENCE_RB = 0x28
    HORIZONTAL_CONVERGENCE_MG = 0x29
    HORIZONTAL_LINEARITY = 0x2A
    HORIZONTAL_LINEARITY_BALANCE = 0x2C
    VERTICAL_POSITION = 0x30
    VERTICAL_SIZE = 0x32
    VERTICAL_PINCUSHION = 0x34
    VERTICAL_PINCUSHION_BALANCE = 0x36
    VERTICAL_CONVERGENCE_RB = 0x38
    VERTICAL_CONVERGENCE_MG = 0x39
    VERTICAL_LINEARITY = 0x3A
    VERTICAL_LINEARITY_BALANCE = 0x3C
    PARALLELOGRAM_DISTORTION = 0x40
    TRAPEZOIDAL_DISTORTION = 0x42
    TILT_ROTATION = 0x44
    TOP_CORNER_DISTORTION = 0x46
    TOP_CORNER_DISTORTION_BALANCE = 0x48
    BOTTOM_CORNER_DISTORTION = 0x4A
    BOTTOM_CORNER_DISTORTION_BALANCE = 0x4C

    # =========================================================================
    # Miscellaneous (0x50-0x5F)
    # =========================================================================
    HORIZONTAL_MOIRE = 0x56
    VERTICAL_MOIRE = 0x58
    SIX_AXIS_SATURATION_RED = 0x59
    SIX_AXIS_SATURATION_YELLOW = 0x5A
    SIX_AXIS_SATURATION_GREEN = 0x5B
    SIX_AXIS_SATURATION_CYAN = 0x5C
    SIX_AXIS_SATURATION_BLUE = 0x5D
    SIX_AXIS_SATURATION_MAGENTA = 0x5E

    # =========================================================================
    # Input/Output (0x60-0x6F)
    # =========================================================================
    INPUT_SOURCE = 0x60
    AUDIO_SPEAKER_VOLUME = 0x62
    SPEAKER_SELECT = 0x63
    AUDIO_MICROPHONE_VOLUME = 0x64
    AMBIENT_LIGHT_SENSOR = 0x66
    REMOTE_PROCEDURE_CALL = 0x6A
    RED_BLACK_LEVEL = 0x6C     # Video black level (offset) - shadows
    GREEN_BLACK_LEVEL = 0x6E
    BLUE_BLACK_LEVEL = 0x70

    # =========================================================================
    # 6-Axis Color Control (0x72-0x7F) - Some high-end monitors
    # =========================================================================
    GRAY_SCALE_EXPANSION = 0x72
    WINDOW_BACKGROUND = 0x7A
    SIX_AXIS_HUE_RED = 0x7B
    SIX_AXIS_HUE_YELLOW = 0x7C
    SIX_AXIS_HUE_GREEN = 0x7D
    SIX_AXIS_HUE_CYAN = 0x7E
    SIX_AXIS_HUE_BLUE = 0x7F
    SIX_AXIS_HUE_MAGENTA = 0x80

    # =========================================================================
    # TV/Video (0x82-0x8F)
    # =========================================================================
    AUDIO_BALANCE_LR = 0x82
    AUDIO_TREBLE = 0x8C
    AUDIO_BASS = 0x8E
    SHARPNESS = 0x87
    SATURATION = 0x8A           # Color saturation
    TV_SHARPNESS = 0x8C
    TV_CONTRAST = 0x8E
    FLESH_TONE_ENHANCEMENT = 0x90
    TV_BLACK_LEVEL = 0x92
    WINDOW_CONTROL = 0x9B       # Window position/size
    WINDOW_SELECT = 0x9C
    WINDOW_SIZE = 0x9D
    WINDOW_TRANSPARENCY = 0x9E

    # =========================================================================
    # Picture Mode/Color Space (0xA0-0xBF) - Important for calibration!
    # =========================================================================
    SCREEN_ORIENTATION = 0xAA
    HORIZONTAL_FREQUENCY = 0xAC
    VERTICAL_FREQUENCY = 0xAE
    FLAT_PANEL_SUBPIXEL_LAYOUT = 0xB2
    SOURCE_TIMING_MODE = 0xB4
    DISPLAY_TECHNOLOGY = 0xB6
    APPLICATION_ENABLE_KEY = 0xC6
    DISPLAY_CONTROLLER_ID = 0xC8
    FIRMWARE_LEVEL = 0xC9
    OSD_ENABLED = 0xCA
    OSD_LANGUAGE = 0xCC
    STATUS_INDICATORS = 0xCD
    AUXILIARY_DISPLAY_SIZE = 0xCE
    AUXILIARY_DISPLAY_DATA = 0xCF
    OUTPUT_SELECT = 0xD0

    # =========================================================================
    # Power and DPMS (0xD0-0xDF)
    # =========================================================================
    ASSET_TAG = 0xD2
    DISPLAY_USAGE_TIME = 0xC0
    POWER_MODE = 0xD6           # DPMS power state
    AUXILIARY_POWER_OUTPUT = 0xD7
    SCAN_MODE = 0xDA            # Interlaced/Progressive
    IMAGE_MODE = 0xDB           # Picture mode preset
    DISPLAY_APPLICATION = 0xDC  # Application mode

    # =========================================================================
    # Manufacturer Specific (0xE0-0xFF) - Varies by brand
    # =========================================================================
    VCP_VERSION = 0xDF
    MANUFACTURER_SPECIFIC_E0 = 0xE0
    MANUFACTURER_SPECIFIC_E1 = 0xE1
    MANUFACTURER_SPECIFIC_E2 = 0xE2
    MANUFACTURER_SPECIFIC_E3 = 0xE3
    MANUFACTURER_SPECIFIC_E4 = 0xE4
    MANUFACTURER_SPECIFIC_E5 = 0xE5
    MANUFACTURER_SPECIFIC_E6 = 0xE6
    MANUFACTURER_SPECIFIC_E7 = 0xE7
    MANUFACTURER_SPECIFIC_E8 = 0xE8
    MANUFACTURER_SPECIFIC_E9 = 0xE9
    MANUFACTURER_SPECIFIC_EA = 0xEA
    MANUFACTURER_SPECIFIC_EB = 0xEB
    MANUFACTURER_SPECIFIC_EC = 0xEC
    MANUFACTURER_SPECIFIC_ED = 0xED
    MANUFACTURER_SPECIFIC_EE = 0xEE
    MANUFACTURER_SPECIFIC_EF = 0xEF
    GAMMA = 0xF2                # Manufacturer-specific gamma selection
    MANUFACTURER_SPECIFIC_F4 = 0xF4
    MANUFACTURER_SPECIFIC_F5 = 0xF5
    MANUFACTURER_SPECIFIC_F6 = 0xF6
    MANUFACTURER_SPECIFIC_F7 = 0xF7
    MANUFACTURER_SPECIFIC_F8 = 0xF8
    MANUFACTURER_SPECIFIC_F9 = 0xF9
    MANUFACTURER_SPECIFIC_FA = 0xFA
    MANUFACTURER_SPECIFIC_FB = 0xFB
    MANUFACTURER_SPECIFIC_FC = 0xFC
    MANUFACTURER_SPECIFIC_FD = 0xFD
    MANUFACTURER_SPECIFIC_FE = 0xFE
    MANUFACTURER_SPECIFIC_FF = 0xFF


# VCP Code descriptions for UI
VCP_DESCRIPTIONS = {
    0x04: ("Factory Reset", "Reset all settings to factory defaults"),
    0x05: ("Reset Brightness", "Reset brightness to factory default"),
    0x06: ("Reset Contrast", "Reset contrast to factory default"),
    0x0A: ("Reset Color", "Reset color settings to factory default"),
    0x0B: ("Color Temp Request", "Request current color temperature in Kelvin"),
    0x0C: ("Color Temp Increment", "Color temperature step size in Kelvin"),
    0x10: ("Brightness", "Display luminance level (0-100)"),
    0x12: ("Contrast", "Display contrast ratio (0-100)"),
    0x13: ("Backlight", "LED backlight level (independent of brightness)"),
    0x14: ("Color Preset", "Color temperature/mode preset"),
    0x16: ("Red Gain", "Red channel gain - affects highlights/whites"),
    0x18: ("Green Gain", "Green channel gain - affects highlights/whites"),
    0x1A: ("Blue Gain", "Blue channel gain - affects highlights/whites"),
    0x59: ("Red Saturation", "6-axis: Red color saturation"),
    0x5A: ("Yellow Saturation", "6-axis: Yellow color saturation"),
    0x5B: ("Green Saturation", "6-axis: Green color saturation"),
    0x5C: ("Cyan Saturation", "6-axis: Cyan color saturation"),
    0x5D: ("Blue Saturation", "6-axis: Blue color saturation"),
    0x5E: ("Magenta Saturation", "6-axis: Magenta color saturation"),
    0x60: ("Input Source", "Select input (HDMI, DP, etc.)"),
    0x62: ("Volume", "Speaker volume level"),
    0x6C: ("Red Black Level", "Red channel offset - affects shadows/blacks"),
    0x6E: ("Green Black Level", "Green channel offset - affects shadows/blacks"),
    0x70: ("Blue Black Level", "Blue channel offset - affects shadows/blacks"),
    0x7B: ("Red Hue", "6-axis: Red color hue shift"),
    0x7C: ("Yellow Hue", "6-axis: Yellow color hue shift"),
    0x7D: ("Green Hue", "6-axis: Green color hue shift"),
    0x7E: ("Cyan Hue", "6-axis: Cyan color hue shift"),
    0x7F: ("Blue Hue", "6-axis: Blue color hue shift"),
    0x80: ("Magenta Hue", "6-axis: Magenta color hue shift"),
    0x87: ("Sharpness", "Image sharpness/edge enhancement"),
    0x8A: ("Saturation", "Overall color saturation"),
    0xB6: ("Display Technology", "Panel type (LCD, OLED, etc.)"),
    0xC0: ("Usage Time", "Display operating hours"),
    0xC9: ("Firmware", "Firmware version"),
    0xCA: ("OSD Enabled", "On-Screen Display on/off"),
    0xCC: ("OSD Language", "On-Screen Display language"),
    0xD6: ("Power Mode", "DPMS power state"),
    0xDA: ("Scan Mode", "Interlaced/Progressive"),
    0xDB: ("Image Mode", "Picture mode preset (Standard, Movie, Game, etc.)"),
    0xDC: ("Display App", "Application/usage mode"),
    0xDF: ("VCP Version", "MCCS/VCP version"),
    0xF2: ("Gamma", "Gamma curve preset selection"),
}


class ColorPreset(IntEnum):
    """Standard color temperature/mode presets."""
    NATIVE = 0x01
    SRGB = 0x02
    COLOR_TEMP_4000K = 0x03
    COLOR_TEMP_5000K = 0x04
    COLOR_TEMP_6500K = 0x05
    COLOR_TEMP_7500K = 0x06
    COLOR_TEMP_8200K = 0x07
    COLOR_TEMP_9300K = 0x08
    COLOR_TEMP_10000K = 0x09
    COLOR_TEMP_11500K = 0x0A
    USER_1 = 0x0B
    USER_2 = 0x0C
    USER_3 = 0x0D


# =============================================================================
# Windows Physical Monitor API
# =============================================================================

class PHYSICAL_MONITOR(ctypes.Structure):
    _fields_ = [
        ("hPhysicalMonitor", wintypes.HANDLE),
        ("szPhysicalMonitorDescription", wintypes.WCHAR * 128),
    ]


@dataclass
class MonitorCapabilities:
    """DDC/CI capabilities for a monitor."""
    model: str = ""
    supported_vcp_codes: List[int] = field(default_factory=list)
    color_temp_range: Tuple[int, int] = (0, 0)
    has_rgb_gain: bool = False
    has_rgb_black_level: bool = False
    has_hardware_lut: bool = False
    raw_capabilities: str = ""


@dataclass
class MonitorSettings:
    """Current monitor hardware settings."""
    brightness: int = 0
    contrast: int = 0
    red_gain: int = 0
    green_gain: int = 0
    blue_gain: int = 0
    red_black_level: int = 0
    green_black_level: int = 0
    blue_black_level: int = 0
    color_preset: int = 0
    color_temp: int = 6500


class DDCCIController:
    """
    Controls monitor hardware settings via DDC/CI.

    This allows hardware-level calibration for:
    - RGB gain (white balance)
    - RGB black level (black point)
    - Brightness and contrast
    - Color temperature

    Usage:
        controller = DDCCIController()
        monitors = controller.enumerate_monitors()

        for monitor in monitors:
            # Read current settings
            settings = controller.get_settings(monitor)

            # Adjust RGB gain for D65 white point
            controller.set_vcp(monitor, VCPCode.RED_GAIN, 95)
            controller.set_vcp(monitor, VCPCode.GREEN_GAIN, 100)
            controller.set_vcp(monitor, VCPCode.BLUE_GAIN, 98)
    """

    def __init__(self):
        self._load_libraries()
        self._monitors: List[Dict[str, Any]] = []

    def _load_libraries(self):
        """Load Windows DLLs for monitor control."""
        try:
            self.dxva2 = ctypes.windll.dxva2
            self.user32 = ctypes.windll.user32
            self._available = True
        except Exception:
            self._available = False

    @property
    def available(self) -> bool:
        """Check if DDC/CI is available on this system."""
        return self._available

    def enumerate_monitors(self) -> List[Dict[str, Any]]:
        """
        Enumerate all monitors with DDC/CI support.

        Returns:
            List of monitor info dicts with handle, name, and capabilities.
        """
        if not self._available:
            return []

        self._monitors = []

        # Callback for EnumDisplayMonitors
        MONITORENUMPROC = ctypes.WINFUNCTYPE(
            wintypes.BOOL,
            wintypes.HMONITOR,
            wintypes.HDC,
            ctypes.POINTER(wintypes.RECT),
            wintypes.LPARAM
        )

        def monitor_callback(hMonitor, hdcMonitor, lprcMonitor, dwData):
            try:
                # Get number of physical monitors
                num_physical = wintypes.DWORD()
                if self.dxva2.GetNumberOfPhysicalMonitorsFromHMONITOR(
                    hMonitor, ctypes.byref(num_physical)
                ):
                    # Get physical monitor handles
                    physical_monitors = (PHYSICAL_MONITOR * num_physical.value)()
                    if self.dxva2.GetPhysicalMonitorsFromHMONITOR(
                        hMonitor, num_physical.value, physical_monitors
                    ):
                        for pm in physical_monitors:
                            monitor_info = {
                                'handle': pm.hPhysicalMonitor,
                                'name': pm.szPhysicalMonitorDescription,
                                'hmonitor': hMonitor,
                                'capabilities': None
                            }

                            # Try to get capabilities
                            try:
                                caps = self._get_capabilities(pm.hPhysicalMonitor)
                                monitor_info['capabilities'] = caps
                            except Exception:
                                pass

                            self._monitors.append(monitor_info)
            except Exception:
                pass
            return True

        callback = MONITORENUMPROC(monitor_callback)
        self.user32.EnumDisplayMonitors(None, None, callback, 0)

        return self._monitors

    def _get_capabilities(self, handle: wintypes.HANDLE) -> MonitorCapabilities:
        """Parse monitor capabilities string."""
        caps = MonitorCapabilities()

        # Get capabilities string length
        caps_len = wintypes.DWORD()
        if not self.dxva2.GetCapabilitiesStringLength(handle, ctypes.byref(caps_len)):
            return caps

        # Get capabilities string
        caps_str = ctypes.create_string_buffer(caps_len.value + 1)
        if not self.dxva2.CapabilitiesRequestAndCapabilitiesReply(
            handle, caps_str, caps_len.value
        ):
            return caps

        caps.raw_capabilities = caps_str.value.decode('ascii', errors='ignore')

        # Parse VCP codes from capabilities string
        # Format: "(vcp(10 12 16 18 1A ...))"
        import re
        vcp_match = re.search(r'vcp\(([^)]+)\)', caps.raw_capabilities, re.IGNORECASE)
        if vcp_match:
            vcp_str = vcp_match.group(1)
            for code in vcp_str.split():
                try:
                    code_int = int(code, 16)
                    caps.supported_vcp_codes.append(code_int)
                except ValueError:
                    pass

        # Check for RGB gain support
        caps.has_rgb_gain = all(
            code in caps.supported_vcp_codes
            for code in [VCPCode.RED_GAIN, VCPCode.GREEN_GAIN, VCPCode.BLUE_GAIN]
        )

        # Check for RGB black level support
        caps.has_rgb_black_level = all(
            code in caps.supported_vcp_codes
            for code in [VCPCode.RED_BLACK_LEVEL, VCPCode.GREEN_BLACK_LEVEL, VCPCode.BLUE_BLACK_LEVEL]
        )

        # Parse model name
        model_match = re.search(r'model\(([^)]+)\)', caps.raw_capabilities, re.IGNORECASE)
        if model_match:
            caps.model = model_match.group(1).strip()

        return caps

    def get_vcp(self, monitor: Dict, code: VCPCode) -> Tuple[int, int]:
        """
        Get VCP value from monitor.

        Returns:
            Tuple of (current_value, max_value)
        """
        if not self._available:
            raise RuntimeError("DDC/CI not available")

        current = wintypes.DWORD()
        maximum = wintypes.DWORD()

        # MC_VCP_CODE_TYPE enum: MC_MOMENTARY=0, MC_SET_PARAMETER=1
        vcp_type = wintypes.DWORD()

        if self.dxva2.GetVCPFeatureAndVCPFeatureReply(
            monitor['handle'],
            code,
            ctypes.byref(vcp_type),
            ctypes.byref(current),
            ctypes.byref(maximum)
        ):
            return (current.value, maximum.value)
        else:
            raise RuntimeError(f"Failed to get VCP code 0x{code:02X}")

    def set_vcp(self, monitor: Dict, code: VCPCode, value: int) -> bool:
        """
        Set VCP value on monitor.

        Args:
            monitor: Monitor dict from enumerate_monitors()
            code: VCP code to set
            value: New value (0-100 for most settings, or absolute for others)

        Returns:
            True if successful
        """
        if not self._available:
            raise RuntimeError("DDC/CI not available")

        return bool(self.dxva2.SetVCPFeature(
            monitor['handle'],
            code,
            value
        ))

    def scan_all_vcp_codes(
        self,
        monitor: Dict,
        progress_callback: Optional[callable] = None
    ) -> Dict[int, Tuple[int, int]]:
        """
        Scan ALL VCP codes (0x00-0xFF) to discover what the monitor supports.

        This is a brute-force discovery that tests each VCP code.
        Some monitors support more codes than they report in capabilities.

        Args:
            monitor: Monitor dict from enumerate_monitors()
            progress_callback: Optional callback(code, total) for progress

        Returns:
            Dict of {vcp_code: (current_value, max_value)} for supported codes
        """
        if not self._available:
            return {}

        supported = {}
        total = 256

        for code in range(0x00, 0x100):
            if progress_callback:
                progress_callback(code, total)

            try:
                current = wintypes.DWORD()
                maximum = wintypes.DWORD()
                vcp_type = wintypes.DWORD()

                if self.dxva2.GetVCPFeatureAndVCPFeatureReply(
                    monitor['handle'],
                    code,
                    ctypes.byref(vcp_type),
                    ctypes.byref(current),
                    ctypes.byref(maximum)
                ):
                    # Some monitors return success but max=0 for unsupported
                    if maximum.value > 0 or current.value > 0:
                        supported[code] = (current.value, maximum.value)
            except Exception:
                pass

        return supported

    def try_set_vcp(self, monitor: Dict, code: int, value: int) -> Tuple[bool, str]:
        """
        Try to set a VCP value and return detailed result.

        Returns:
            Tuple of (success, message)
        """
        if not self._available:
            return False, "DDC/CI not available"

        try:
            # First try to read the current value
            current = wintypes.DWORD()
            maximum = wintypes.DWORD()
            vcp_type = wintypes.DWORD()

            can_read = self.dxva2.GetVCPFeatureAndVCPFeatureReply(
                monitor['handle'],
                code,
                ctypes.byref(vcp_type),
                ctypes.byref(current),
                ctypes.byref(maximum)
            )

            if not can_read:
                return False, f"VCP code 0x{code:02X} not readable"

            old_value = current.value

            # Clamp value to max
            if maximum.value > 0 and value > maximum.value:
                value = maximum.value

            # Try to set
            result = self.dxva2.SetVCPFeature(monitor['handle'], code, value)

            if not result:
                return False, f"SetVCPFeature failed for 0x{code:02X}"

            # Read back to verify
            import time
            time.sleep(0.1)  # Give monitor time to apply

            if self.dxva2.GetVCPFeatureAndVCPFeatureReply(
                monitor['handle'],
                code,
                ctypes.byref(vcp_type),
                ctypes.byref(current),
                ctypes.byref(maximum)
            ):
                new_value = current.value
                if new_value == value:
                    return True, f"Set 0x{code:02X}: {old_value} -> {new_value}"
                elif new_value != old_value:
                    return True, f"Set 0x{code:02X}: {old_value} -> {new_value} (requested {value})"
                else:
                    return False, f"Value unchanged: {old_value} (monitor may ignore this code)"

            return True, f"Set 0x{code:02X} to {value} (cannot verify)"

        except Exception as e:
            return False, f"Error: {e}"

    def get_settings(self, monitor: Dict) -> MonitorSettings:
        """Get all available hardware settings from monitor."""
        settings = MonitorSettings()

        try:
            settings.brightness, _ = self.get_vcp(monitor, VCPCode.BRIGHTNESS)
        except Exception:
            pass

        try:
            settings.contrast, _ = self.get_vcp(monitor, VCPCode.CONTRAST)
        except Exception:
            pass

        # RGB Gain
        try:
            settings.red_gain, _ = self.get_vcp(monitor, VCPCode.RED_GAIN)
            settings.green_gain, _ = self.get_vcp(monitor, VCPCode.GREEN_GAIN)
            settings.blue_gain, _ = self.get_vcp(monitor, VCPCode.BLUE_GAIN)
        except Exception:
            pass

        # RGB Black Level
        try:
            settings.red_black_level, _ = self.get_vcp(monitor, VCPCode.RED_BLACK_LEVEL)
            settings.green_black_level, _ = self.get_vcp(monitor, VCPCode.GREEN_BLACK_LEVEL)
            settings.blue_black_level, _ = self.get_vcp(monitor, VCPCode.BLUE_BLACK_LEVEL)
        except Exception:
            pass

        try:
            settings.color_preset, _ = self.get_vcp(monitor, VCPCode.COLOR_PRESET)
        except Exception:
            pass

        return settings

    def set_rgb_gain(self, monitor: Dict, red: int, green: int, blue: int) -> bool:
        """
        Set RGB gain values for white point adjustment.

        Args:
            monitor: Monitor dict
            red, green, blue: Gain values (typically 0-100)

        Returns:
            True if all settings applied successfully
        """
        success = True
        success &= self.set_vcp(monitor, VCPCode.RED_GAIN, red)
        success &= self.set_vcp(monitor, VCPCode.GREEN_GAIN, green)
        success &= self.set_vcp(monitor, VCPCode.BLUE_GAIN, blue)
        return success

    def set_rgb_black_level(self, monitor: Dict, red: int, green: int, blue: int) -> bool:
        """
        Set RGB black level values for black point adjustment.

        Args:
            monitor: Monitor dict
            red, green, blue: Black level values (typically 0-100)

        Returns:
            True if all settings applied successfully
        """
        success = True
        success &= self.set_vcp(monitor, VCPCode.RED_BLACK_LEVEL, red)
        success &= self.set_vcp(monitor, VCPCode.GREEN_BLACK_LEVEL, green)
        success &= self.set_vcp(monitor, VCPCode.BLUE_BLACK_LEVEL, blue)
        return success

    def set_color_preset(self, monitor: Dict, preset: ColorPreset) -> bool:
        """Set color temperature/mode preset."""
        return self.set_vcp(monitor, VCPCode.COLOR_PRESET, preset)

    def close(self):
        """Release monitor handles."""
        if self._available:
            for monitor in self._monitors:
                try:
                    self.dxva2.DestroyPhysicalMonitor(monitor['handle'])
                except Exception:
                    pass
        self._monitors = []


# =============================================================================
# Hardware Calibration Engine
# =============================================================================

@dataclass
class HardwareCalibrationTarget:
    """Target values for hardware calibration."""
    white_point_x: float = 0.3127  # D65
    white_point_y: float = 0.3290
    target_brightness: float = 120.0  # cd/m²
    target_gamma: float = 2.2
    black_level: float = 0.1  # cd/m²


@dataclass
class HardwareCalibrationResult:
    """Results from hardware calibration."""
    success: bool = False
    rgb_gain: Tuple[int, int, int] = (100, 100, 100)
    rgb_black: Tuple[int, int, int] = (50, 50, 50)
    brightness: int = 50
    contrast: int = 50
    measured_white_x: float = 0.0
    measured_white_y: float = 0.0
    measured_brightness: float = 0.0
    delta_e: float = 0.0
    iterations: int = 0
    message: str = ""


class HardwareCalibrator:
    """
    Performs hardware-level calibration via DDC/CI.

    This adjusts the monitor's internal settings (RGB gain, black level)
    to achieve the target white point and grayscale tracking BEFORE
    applying any ICC profile or LUT.

    Benefits:
    - Preserves monitor's native bit depth
    - No GPU processing overhead
    - Works even without color management software running
    - ICC profile only needs to handle gamut mapping, not grayscale
    """

    def __init__(self, ddc_controller: DDCCIController = None):
        self.ddc = ddc_controller or DDCCIController()
        self._measurement_callback = None

    def set_measurement_callback(self, callback):
        """
        Set callback for getting color measurements.

        Callback signature: callback(rgb: Tuple[int, int, int]) -> Tuple[float, float, float]
        Returns: (X, Y, Z) tristimulus values
        """
        self._measurement_callback = callback

    def calibrate_white_point(
        self,
        monitor: Dict,
        target: HardwareCalibrationTarget,
        max_iterations: int = 20,
        tolerance: float = 0.002  # xy chromaticity tolerance
    ) -> HardwareCalibrationResult:
        """
        Iteratively adjust RGB gain to achieve target white point.

        This uses a feedback loop with the colorimeter to dial in
        the exact white point through hardware adjustments.

        Args:
            monitor: Monitor dict from DDCCIController
            target: Calibration targets
            max_iterations: Maximum adjustment iterations
            tolerance: xy chromaticity tolerance for convergence

        Returns:
            HardwareCalibrationResult with final settings and measurements
        """
        result = HardwareCalibrationResult()

        if not self._measurement_callback:
            result.message = "No measurement callback set - cannot calibrate"
            return result

        caps = monitor.get('capabilities')
        if not caps or not caps.has_rgb_gain:
            result.message = "Monitor does not support RGB gain adjustment via DDC/CI"
            return result

        # Start with current settings
        current = self.ddc.get_settings(monitor)
        r_gain = current.red_gain or 100
        g_gain = current.green_gain or 100
        b_gain = current.blue_gain or 100

        # Set to User color mode for manual RGB control
        try:
            self.ddc.set_color_preset(monitor, ColorPreset.USER_1)
        except Exception:
            pass

        for iteration in range(max_iterations):
            # Apply current RGB gain
            self.ddc.set_rgb_gain(monitor, r_gain, g_gain, b_gain)

            # Measure white (255, 255, 255)
            try:
                X, Y, Z = self._measurement_callback((255, 255, 255))
            except Exception as e:
                result.message = f"Measurement failed: {e}"
                return result

            # Convert to xy chromaticity
            total = X + Y + Z
            if total > 0:
                x = X / total
                y = Y / total
            else:
                result.message = "Invalid measurement (zero luminance)"
                return result

            result.measured_white_x = x
            result.measured_white_y = y
            result.measured_brightness = Y

            # Calculate error
            error_x = target.white_point_x - x
            error_y = target.white_point_y - y

            # Check convergence
            if abs(error_x) < tolerance and abs(error_y) < tolerance:
                result.success = True
                result.message = f"Converged in {iteration + 1} iterations"
                break

            # Adjust RGB gain based on chromaticity error
            # Positive error_x means too blue, reduce blue and/or increase red
            # Positive error_y means too green, reduce green

            # Sensitivity factors (tuned for typical monitors)
            k_r = 30  # Red gain sensitivity
            k_g = 30  # Green gain sensitivity
            k_b = 30  # Blue gain sensitivity

            # Red primarily affects x (moving right)
            # Green primarily affects y (moving up)
            # Blue affects both x (left) and y (down)

            delta_r = int(error_x * k_r * 2 + error_y * k_r * 0.5)
            delta_g = int(-error_y * k_g * 2)
            delta_b = int(-error_x * k_b * 1.5 - error_y * k_b * 0.5)

            # Apply adjustments with clamping
            r_gain = max(0, min(100, r_gain + delta_r))
            g_gain = max(0, min(100, g_gain + delta_g))
            b_gain = max(0, min(100, b_gain + delta_b))

            result.iterations = iteration + 1

        result.rgb_gain = (r_gain, g_gain, b_gain)

        if not result.success:
            result.message = f"Did not converge after {max_iterations} iterations"

        # Calculate final Delta E (simplified for white point)
        # Using CIE 1976 UCS for chromaticity difference
        u_target = 4 * target.white_point_x / (-2 * target.white_point_x + 12 * target.white_point_y + 3)
        v_target = 9 * target.white_point_y / (-2 * target.white_point_x + 12 * target.white_point_y + 3)
        u_meas = 4 * result.measured_white_x / (-2 * result.measured_white_x + 12 * result.measured_white_y + 3)
        v_meas = 9 * result.measured_white_y / (-2 * result.measured_white_x + 12 * result.measured_white_y + 3)

        result.delta_e = ((u_target - u_meas)**2 + (v_target - v_meas)**2)**0.5 * 100

        return result

    def calibrate_brightness(
        self,
        monitor: Dict,
        target_nits: float,
        tolerance: float = 5.0  # cd/m² tolerance
    ) -> Tuple[bool, int]:
        """
        Adjust monitor brightness to target luminance.

        Returns:
            Tuple of (success, final_brightness_setting)
        """
        if not self._measurement_callback:
            return False, 0

        # Binary search for correct brightness
        low, high = 0, 100
        best_brightness = 50
        best_diff = float('inf')

        for _ in range(10):  # Max 10 iterations
            mid = (low + high) // 2
            self.ddc.set_vcp(monitor, VCPCode.BRIGHTNESS, mid)

            # Measure white luminance
            X, Y, Z = self._measurement_callback((255, 255, 255))
            measured_nits = Y  # Y is luminance in cd/m²

            diff = abs(measured_nits - target_nits)
            if diff < best_diff:
                best_diff = diff
                best_brightness = mid

            if diff < tolerance:
                return True, mid

            if measured_nits < target_nits:
                low = mid + 1
            else:
                high = mid - 1

            if low > high:
                break

        return best_diff < tolerance * 2, best_brightness


# =============================================================================
# Professional Monitor Hardware LUT Support
# =============================================================================

class HardwareLUTUploader:
    """
    Upload calibration LUTs directly to professional monitors.

    Supported monitors:
    - EIZO ColorEdge (CG/CS series)
    - NEC SpectraView
    - BenQ SW series
    - LG UltraFine (some models)

    Note: Each manufacturer uses proprietary protocols.
    This class provides a unified interface.
    """

    def __init__(self):
        self._supported_monitors = {}
        self._detect_supported_monitors()

    def _detect_supported_monitors(self):
        """Detect monitors with hardware LUT support."""
        # In production, this would probe for specific monitors
        # using USB HID or proprietary DDC extensions
        pass

    def upload_lut(
        self,
        monitor_id: str,
        lut_data: bytes,
        lut_size: int = 4096
    ) -> bool:
        """
        Upload a 1D or 3D LUT to the monitor's hardware.

        Args:
            monitor_id: Monitor identifier
            lut_data: LUT data in monitor-specific format
            lut_size: Number of entries (e.g., 4096 for 12-bit)

        Returns:
            True if upload successful
        """
        # Placeholder - actual implementation would use:
        # - USB HID for EIZO ColorNavigator protocol
        # - Custom DDC extensions for NEC SpectraView
        # - Proprietary API for BenQ Palette Master
        raise NotImplementedError(
            "Hardware LUT upload requires monitor-specific implementation. "
            "Consider using the manufacturer's calibration software for now."
        )

    def get_supported_monitors(self) -> List[str]:
        """Get list of detected monitors with hardware LUT support."""
        return list(self._supported_monitors.keys())


# =============================================================================
# Convenience Functions
# =============================================================================

def detect_ddc_monitors() -> List[Dict]:
    """Quick detection of all DDC/CI capable monitors."""
    controller = DDCCIController()
    monitors = controller.enumerate_monitors()
    return monitors


def print_monitor_capabilities(monitor: Dict):
    """Print monitor DDC/CI capabilities for debugging."""
    print(f"\nMonitor: {monitor['name']}")
    caps = monitor.get('capabilities')
    if caps:
        print(f"  Model: {caps.model}")
        print(f"  RGB Gain: {'Yes' if caps.has_rgb_gain else 'No'}")
        print(f"  RGB Black Level: {'Yes' if caps.has_rgb_black_level else 'No'}")
        print(f"  Supported VCP codes: {[hex(c) for c in caps.supported_vcp_codes[:20]]}...")
    else:
        print("  No capabilities data available")


if __name__ == "__main__":
    # Demo: enumerate monitors and print capabilities
    print("DDC/CI Monitor Detection")
    print("=" * 50)

    controller = DDCCIController()
    if not controller.available:
        print("DDC/CI not available on this system")
    else:
        monitors = controller.enumerate_monitors()
        print(f"Found {len(monitors)} monitor(s)")

        for monitor in monitors:
            print_monitor_capabilities(monitor)

            # Try to read current settings
            try:
                settings = controller.get_settings(monitor)
                print(f"  Current Brightness: {settings.brightness}")
                print(f"  Current Contrast: {settings.contrast}")
                print(f"  RGB Gain: R={settings.red_gain} G={settings.green_gain} B={settings.blue_gain}")
            except Exception as e:
                print(f"  Could not read settings: {e}")

        controller.close()
