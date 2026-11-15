"""
Auto-Calibration Loader - Applies display calibration at system startup.

This module handles automatic application of:
- DDC/CI monitor settings (color preset, RGB gains)
- ICC profile association
- Persistent calibration across reboots
"""

import ctypes
from ctypes import wintypes, Structure, POINTER, byref
import json
import os
import sys
import time
from pathlib import Path
from typing import Optional, Dict, Any
from dataclasses import dataclass, asdict
import logging

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


# ============================================================================
# DDC/CI Structures and Functions
# ============================================================================

class PHYSICAL_MONITOR(Structure):
    _fields_ = [
        ('hPhysicalMonitor', wintypes.HANDLE),
        ('szPhysicalMonitorDescription', wintypes.WCHAR * 128)
    ]


class DDCController:
    """Controls monitor settings via DDC/CI."""

    # VCP codes
    VCP_BRIGHTNESS = 0x10
    VCP_CONTRAST = 0x12
    VCP_COLOR_PRESET = 0x14
    VCP_RED_GAIN = 0x16
    VCP_GREEN_GAIN = 0x18
    VCP_BLUE_GAIN = 0x1A

    def __init__(self):
        self.user32 = ctypes.windll.user32
        self.dxva2 = ctypes.windll.dxva2
        self.monitors = []
        self._enumerate_monitors()

    def _enumerate_monitors(self):
        """Enumerate all physical monitors."""
        self.monitors = []

        def callback(hMonitor, hdcMonitor, lprcMonitor, dwData):
            num = wintypes.DWORD()
            self.dxva2.GetNumberOfPhysicalMonitorsFromHMONITOR(hMonitor, byref(num))
            if num.value > 0:
                physical = (PHYSICAL_MONITOR * num.value)()
                if self.dxva2.GetPhysicalMonitorsFromHMONITOR(hMonitor, num, physical):
                    for i in range(num.value):
                        self.monitors.append({
                            'hMonitor': hMonitor,
                            'physical': physical[i],
                            'description': physical[i].szPhysicalMonitorDescription
                        })
            return True

        MONITORENUMPROC = ctypes.WINFUNCTYPE(
            wintypes.BOOL,
            wintypes.HMONITOR,
            wintypes.HDC,
            POINTER(wintypes.RECT),
            wintypes.LPARAM
        )
        self.user32.EnumDisplayMonitors(None, None, MONITORENUMPROC(callback), 0)

    def get_vcp(self, monitor_idx: int, vcp_code: int) -> Optional[int]:
        """Get VCP value for a monitor."""
        if monitor_idx >= len(self.monitors):
            return None

        handle = self.monitors[monitor_idx]['physical'].hPhysicalMonitor
        vcp_type = wintypes.DWORD()
        current = wintypes.DWORD()
        maximum = wintypes.DWORD()

        if self.dxva2.GetVCPFeatureAndVCPFeatureReply(
            handle, vcp_code, byref(vcp_type), byref(current), byref(maximum)
        ):
            return current.value
        return None

    def set_vcp(self, monitor_idx: int, vcp_code: int, value: int) -> bool:
        """Set VCP value for a monitor."""
        if monitor_idx >= len(self.monitors):
            return False

        handle = self.monitors[monitor_idx]['physical'].hPhysicalMonitor
        return bool(self.dxva2.SetVCPFeature(handle, vcp_code, value))

    def apply_calibration(self, monitor_idx: int, settings: Dict[str, Any]) -> bool:
        """Apply calibration settings to a monitor."""
        success = True

        # Apply color preset
        if 'color_preset' in settings:
            if not self.set_vcp(monitor_idx, self.VCP_COLOR_PRESET, settings['color_preset']):
                logger.warning(f"Failed to set color preset to {settings['color_preset']}")
                success = False
            else:
                logger.info(f"Set color preset to 0x{settings['color_preset']:02X}")

        # Small delay for preset to take effect
        time.sleep(0.3)

        # Apply RGB gains
        if 'red_gain' in settings:
            if not self.set_vcp(monitor_idx, self.VCP_RED_GAIN, settings['red_gain']):
                logger.warning(f"Failed to set red gain to {settings['red_gain']}")
                success = False

        if 'green_gain' in settings:
            if not self.set_vcp(monitor_idx, self.VCP_GREEN_GAIN, settings['green_gain']):
                logger.warning(f"Failed to set green gain to {settings['green_gain']}")
                success = False

        if 'blue_gain' in settings:
            if not self.set_vcp(monitor_idx, self.VCP_BLUE_GAIN, settings['blue_gain']):
                logger.warning(f"Failed to set blue gain to {settings['blue_gain']}")
                success = False

        if success:
            logger.info(f"RGB gains set to R={settings.get('red_gain', 100)}, "
                       f"G={settings.get('green_gain', 100)}, B={settings.get('blue_gain', 100)}")

        return success

    def cleanup(self):
        """Release monitor handles."""
        for mon in self.monitors:
            try:
                physical_array = (PHYSICAL_MONITOR * 1)(mon['physical'])
                self.dxva2.DestroyPhysicalMonitors(1, physical_array)
            except:
                pass


# ============================================================================
# ICC Profile Management
# ============================================================================

class ICCProfileManager:
    """Manages ICC profile installation and association."""

    def __init__(self):
        self.mscms = ctypes.windll.mscms
        self.profile_dir = Path(os.environ.get('WINDIR', 'C:/Windows')) / 'System32' / 'spool' / 'drivers' / 'color'

    def get_monitor_device_id(self, display_index: int = 0) -> Optional[str]:
        """Get the device ID for a display's monitor."""
        user32 = ctypes.windll.user32

        class DISPLAY_DEVICEW(Structure):
            _fields_ = [
                ('cb', wintypes.DWORD),
                ('DeviceName', wintypes.WCHAR * 32),
                ('DeviceString', wintypes.WCHAR * 128),
                ('StateFlags', wintypes.DWORD),
                ('DeviceID', wintypes.WCHAR * 128),
                ('DeviceKey', wintypes.WCHAR * 128),
            ]

        display = DISPLAY_DEVICEW()
        display.cb = ctypes.sizeof(display)

        i = 0
        active_count = 0
        while user32.EnumDisplayDevicesW(None, i, byref(display), 0):
            if display.StateFlags & 0x00000001:  # ATTACHED_TO_DESKTOP
                if active_count == display_index:
                    # Get monitor device ID
                    monitor = DISPLAY_DEVICEW()
                    monitor.cb = ctypes.sizeof(monitor)
                    if user32.EnumDisplayDevicesW(display.DeviceName, 0, byref(monitor), 0):
                        return monitor.DeviceID
                active_count += 1
            i += 1

        return None

    def associate_profile(self, profile_name: str, device_id: str) -> bool:
        """Associate an ICC profile with a device."""
        WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER = 1

        try:
            result = self.mscms.WcsAssociateColorProfileWithDevice(
                WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
                device_id,
                profile_name
            )
            return bool(result)
        except Exception as e:
            logger.error(f"Failed to associate profile: {e}")
            return False

    def set_default_profile(self, profile_name: str, device_id: str) -> bool:
        """Set an ICC profile as the default for a device."""
        WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER = 1
        CPT_ICC = 1

        try:
            result = self.mscms.WcsSetDefaultColorProfile(
                WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
                device_id,
                CPT_ICC,
                0,  # subtype
                0,  # index
                profile_name
            )
            return bool(result)
        except Exception as e:
            logger.error(f"Failed to set default profile: {e}")
            return False


# ============================================================================
# Calibration Configuration
# ============================================================================

@dataclass
class MonitorCalibration:
    """Calibration settings for a single monitor."""
    monitor_id: str  # Device ID or model identifier
    description: str
    color_preset: int = 0x05  # sRGB mode
    red_gain: int = 100
    green_gain: int = 100
    blue_gain: int = 100
    icc_profile: Optional[str] = None
    white_point: str = "D65"  # D50, D55, D65, etc.
    ddc_supported: bool = True  # Whether DDC/CI is supported


@dataclass
class CalibrationProfile:
    """Complete calibration profile for all monitors."""
    version: str = "1.0"
    name: str = "Default"
    monitors: Dict[int, MonitorCalibration] = None

    def __post_init__(self):
        if self.monitors is None:
            self.monitors = {}


class AutoCalibrationManager:
    """Manages automatic calibration application."""

    def __init__(self):
        self.config_dir = self._get_config_dir()
        self.profile_file = self.config_dir / "auto_calibration.json"
        self.profile = self._load_profile()
        self.ddc = DDCController()
        self.icc = ICCProfileManager()

    def _get_config_dir(self) -> Path:
        """Get configuration directory."""
        appdata = os.environ.get('APPDATA', os.path.expanduser('~'))
        config_dir = Path(appdata) / "CalibratePro"
        config_dir.mkdir(parents=True, exist_ok=True)
        return config_dir

    def _load_profile(self) -> CalibrationProfile:
        """Load calibration profile from file."""
        if self.profile_file.exists():
            try:
                with open(self.profile_file, 'r') as f:
                    data = json.load(f)

                monitors = {}
                for idx, mon_data in data.get('monitors', {}).items():
                    monitors[int(idx)] = MonitorCalibration(**mon_data)

                return CalibrationProfile(
                    version=data.get('version', '1.0'),
                    name=data.get('name', 'Default'),
                    monitors=monitors
                )
            except Exception as e:
                logger.warning(f"Could not load profile: {e}")

        return CalibrationProfile()

    def save_profile(self):
        """Save calibration profile to file."""
        data = {
            'version': self.profile.version,
            'name': self.profile.name,
            'monitors': {
                str(idx): asdict(mon)
                for idx, mon in self.profile.monitors.items()
            }
        }

        with open(self.profile_file, 'w') as f:
            json.dump(data, f, indent=2)

        logger.info(f"Saved calibration profile to {self.profile_file}")

    def add_monitor_calibration(
        self,
        monitor_idx: int,
        monitor_id: str,
        description: str,
        color_preset: int = 0x05,
        red_gain: int = 100,
        green_gain: int = 100,
        blue_gain: int = 100,
        icc_profile: Optional[str] = None,
        white_point: str = "D65",
        ddc_supported: bool = True
    ):
        """Add or update calibration for a monitor."""
        self.profile.monitors[monitor_idx] = MonitorCalibration(
            monitor_id=monitor_id,
            description=description,
            color_preset=color_preset,
            red_gain=red_gain,
            green_gain=green_gain,
            blue_gain=blue_gain,
            icc_profile=icc_profile,
            white_point=white_point,
            ddc_supported=ddc_supported
        )
        self.save_profile()

    def apply_all_calibrations(self) -> Dict[int, bool]:
        """Apply calibration to all configured monitors."""
        results = {}

        logger.info("=" * 50)
        logger.info("Applying Auto-Calibration")
        logger.info("=" * 50)

        for monitor_idx, calibration in self.profile.monitors.items():
            logger.info(f"\nMonitor {monitor_idx}: {calibration.description}")

            # Apply DDC/CI settings (if supported)
            ddc_success = True
            if calibration.ddc_supported:
                settings = {
                    'color_preset': calibration.color_preset,
                    'red_gain': calibration.red_gain,
                    'green_gain': calibration.green_gain,
                    'blue_gain': calibration.blue_gain,
                }
                ddc_success = self.ddc.apply_calibration(monitor_idx, settings)
            else:
                logger.info("  DDC/CI not supported - skipping hardware controls")

            # Apply ICC profile
            icc_success = True
            if calibration.icc_profile:
                device_id = self.icc.get_monitor_device_id(monitor_idx)
                if device_id:
                    self.icc.associate_profile(calibration.icc_profile, device_id)
                    icc_success = self.icc.set_default_profile(calibration.icc_profile, device_id)
                    if icc_success:
                        logger.info(f"ICC profile applied: {calibration.icc_profile}")
                    else:
                        logger.warning(f"Failed to set ICC profile as default")

            results[monitor_idx] = ddc_success and icc_success

        logger.info("\n" + "=" * 50)
        logger.info("Calibration Complete")
        logger.info("=" * 50)

        return results

    def cleanup(self):
        """Cleanup resources."""
        self.ddc.cleanup()


# ============================================================================
# Windows Startup Registration
# ============================================================================

def register_startup(script_path: Optional[str] = None) -> bool:
    """Register auto-calibration to run at Windows startup."""
    import winreg

    STARTUP_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
    APP_NAME = "CalibratePro_AutoCalibration"

    if script_path is None:
        # Use current script
        script_path = os.path.abspath(__file__)

    # Build command - use pythonw for silent execution
    python_path = sys.executable.replace('python.exe', 'pythonw.exe')
    if not os.path.exists(python_path):
        python_path = sys.executable

    cmd = f'"{python_path}" "{script_path}" --apply'

    try:
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            STARTUP_KEY,
            0,
            winreg.KEY_SET_VALUE
        )
        winreg.SetValueEx(key, APP_NAME, 0, winreg.REG_SZ, cmd)
        winreg.CloseKey(key)
        logger.info(f"Registered startup: {cmd}")
        return True
    except Exception as e:
        logger.error(f"Failed to register startup: {e}")
        return False


def unregister_startup() -> bool:
    """Remove auto-calibration from Windows startup."""
    import winreg

    STARTUP_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
    APP_NAME = "CalibratePro_AutoCalibration"

    try:
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            STARTUP_KEY,
            0,
            winreg.KEY_SET_VALUE
        )
        try:
            winreg.DeleteValue(key, APP_NAME)
        except WindowsError:
            pass
        winreg.CloseKey(key)
        logger.info("Removed from startup")
        return True
    except Exception as e:
        logger.error(f"Failed to unregister startup: {e}")
        return False


def is_startup_registered() -> bool:
    """Check if auto-calibration is registered for startup."""
    import winreg

    STARTUP_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
    APP_NAME = "CalibratePro_AutoCalibration"

    try:
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            STARTUP_KEY,
            0,
            winreg.KEY_READ
        )
        try:
            winreg.QueryValueEx(key, APP_NAME)
            return True
        except WindowsError:
            return False
        finally:
            winreg.CloseKey(key)
    except:
        return False


# ============================================================================
# Main Entry Point
# ============================================================================

def main():
    """Main entry point for auto-calibration."""
    import argparse

    parser = argparse.ArgumentParser(description='CalibratePro Auto-Calibration')
    parser.add_argument('--apply', action='store_true', help='Apply calibration')
    parser.add_argument('--register', action='store_true', help='Register for Windows startup')
    parser.add_argument('--unregister', action='store_true', help='Remove from Windows startup')
    parser.add_argument('--status', action='store_true', help='Show current status')
    parser.add_argument('--setup', action='store_true', help='Setup current monitor calibration')

    args = parser.parse_args()

    if args.register:
        if register_startup():
            print("Successfully registered for Windows startup")
        else:
            print("Failed to register for startup")
        return

    if args.unregister:
        if unregister_startup():
            print("Successfully removed from Windows startup")
        else:
            print("Failed to remove from startup")
        return

    if args.status:
        registered = is_startup_registered()
        print(f"Startup registration: {'ENABLED' if registered else 'DISABLED'}")

        manager = AutoCalibrationManager()
        print(f"\nConfigured monitors: {len(manager.profile.monitors)}")
        for idx, cal in manager.profile.monitors.items():
            print(f"  Monitor {idx}: {cal.description}")
            print(f"    Preset: 0x{cal.color_preset:02X}, RGB: {cal.red_gain}/{cal.green_gain}/{cal.blue_gain}")
            print(f"    White point: {cal.white_point}")
            if cal.icc_profile:
                print(f"    ICC: {cal.icc_profile}")
        return

    if args.setup:
        # Interactive setup for current monitor
        manager = AutoCalibrationManager()

        print("Setting up calibration for primary monitor...")
        print()

        # Get monitor info
        if len(manager.ddc.monitors) == 0:
            print("No monitors found!")
            return

        description = manager.ddc.monitors[0]['description']
        device_id = manager.icc.get_monitor_device_id(0) or "Unknown"

        # Add default D65 sRGB calibration
        manager.add_monitor_calibration(
            monitor_idx=0,
            monitor_id=device_id,
            description=description,
            color_preset=0x05,  # sRGB
            red_gain=100,
            green_gain=100,
            blue_gain=100,
            icc_profile="CalibratePro_Display1.icc",
            white_point="D65"
        )

        print(f"Configured: {description}")
        print(f"  Color preset: 0x05 (sRGB)")
        print(f"  RGB gains: 100/100/100")
        print(f"  White point: D65")
        print(f"  ICC profile: CalibratePro_Display1.icc")
        print()

        # Apply immediately
        manager.apply_all_calibrations()

        # Register for startup
        if register_startup():
            print("\nRegistered for Windows startup")

        manager.cleanup()
        return

    if args.apply or len(sys.argv) == 1:
        # Default: apply calibration
        manager = AutoCalibrationManager()

        if len(manager.profile.monitors) == 0:
            logger.info("No monitors configured. Run with --setup first.")
            return

        manager.apply_all_calibrations()
        manager.cleanup()


if __name__ == '__main__':
    main()
