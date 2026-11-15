"""
ICC Profile System Installation

Handles ICC profile installation and management on Windows:
- System profile installation via mscms.dll
- Default profile assignment per display
- Profile backup and restore
- Display enumeration and association
- Color management settings

Uses the Windows Color Management API (ICM/WCS).
"""

import ctypes
from ctypes import wintypes
from dataclasses import dataclass
from typing import Optional, List, Dict, Tuple, Union
from pathlib import Path
from enum import IntEnum
import shutil
import json
from datetime import datetime
import winreg


# =============================================================================
# Windows Color Management API Definitions
# =============================================================================

# MSCMS.dll functions
try:
    mscms = ctypes.windll.mscms
    MSCMS_AVAILABLE = True
except Exception:
    mscms = None
    MSCMS_AVAILABLE = False

# GDI32 for display enumeration
try:
    gdi32 = ctypes.windll.gdi32
    user32 = ctypes.windll.user32
    GDI_AVAILABLE = True

    # DISPLAY_DEVICE structure for EnumDisplayDevicesW
    class _DISPLAY_DEVICE(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("DeviceName", wintypes.WCHAR * 32),
            ("DeviceString", wintypes.WCHAR * 128),
            ("StateFlags", wintypes.DWORD),
            ("DeviceID", wintypes.WCHAR * 128),
            ("DeviceKey", wintypes.WCHAR * 128),
        ]

    # Set up function signatures
    user32.EnumDisplayDevicesW.argtypes = [
        wintypes.LPCWSTR,
        wintypes.DWORD,
        ctypes.POINTER(_DISPLAY_DEVICE),
        wintypes.DWORD
    ]
    user32.EnumDisplayDevicesW.restype = wintypes.BOOL

except Exception:
    gdi32 = None
    user32 = None
    GDI_AVAILABLE = False
    _DISPLAY_DEVICE = None


# Profile scope
class ProfileScope(IntEnum):
    """Profile installation scope."""
    SYSTEM = 0      # All users (requires admin)
    USER = 1        # Current user only


# Profile association type
class ProfileAssociation(IntEnum):
    """Profile association type."""
    DEFAULT = 0     # Default profile for device
    PERCEPTUAL = 1
    RELATIVE = 2
    SATURATION = 3
    ABSOLUTE = 4


# Color profile type
class ColorProfileType(IntEnum):
    """Color profile type."""
    INPUT = 1
    DISPLAY = 2
    OUTPUT = 3
    LINK = 4
    SPACE = 5
    ABSTRACT = 6
    NAMED = 7


# =============================================================================
# Display Information
# =============================================================================

@dataclass
class DisplayDevice:
    """Information about a display device."""
    device_name: str          # e.g., "\\\\.\\DISPLAY1"
    device_string: str        # Friendly name
    device_id: str           # Hardware ID
    device_key: str          # Registry key
    is_primary: bool
    is_active: bool
    is_attached: bool
    monitor_name: str = ""
    monitor_id: str = ""

    @property
    def display_number(self) -> int:
        """Extract display number from device name."""
        try:
            return int(self.device_name.replace("\\\\.\\DISPLAY", ""))
        except ValueError:
            return 0


@dataclass
class MonitorInfo:
    """Extended monitor information."""
    device: DisplayDevice
    edid_manufacturer: str = ""
    edid_model: str = ""
    edid_serial: str = ""
    resolution: Tuple[int, int] = (0, 0)
    refresh_rate: float = 0.0
    hdr_supported: bool = False
    current_profile: Optional[str] = None


def enumerate_displays() -> List[DisplayDevice]:
    """
    Enumerate all display devices.

    Returns:
        List of DisplayDevice objects
    """
    if not GDI_AVAILABLE or _DISPLAY_DEVICE is None:
        return []

    displays = []

    device = _DISPLAY_DEVICE()
    device.cb = ctypes.sizeof(device)

    i = 0
    while user32.EnumDisplayDevicesW(None, i, ctypes.byref(device), 0):
        if device.StateFlags & 0x00000001:  # DISPLAY_DEVICE_ACTIVE
            displays.append(DisplayDevice(
                device_name=device.DeviceName,
                device_string=device.DeviceString,
                device_id=device.DeviceID,
                device_key=device.DeviceKey,
                is_primary=bool(device.StateFlags & 0x00000004),
                is_active=bool(device.StateFlags & 0x00000001),
                is_attached=bool(device.StateFlags & 0x00000002)
            ))

            # Get monitor info
            monitor = _DISPLAY_DEVICE()
            monitor.cb = ctypes.sizeof(monitor)

            if user32.EnumDisplayDevicesW(device.DeviceName, 0, ctypes.byref(monitor), 0):
                displays[-1].monitor_name = monitor.DeviceString
                displays[-1].monitor_id = monitor.DeviceID

        i += 1

    return displays


def get_monitor_info(device: DisplayDevice) -> MonitorInfo:
    """
    Get extended monitor information.

    Args:
        device: DisplayDevice object

    Returns:
        MonitorInfo with extended details
    """
    info = MonitorInfo(device=device)

    # Get current mode
    class DEVMODEW(ctypes.Structure):
        _fields_ = [
            ("dmDeviceName", wintypes.WCHAR * 32),
            ("dmSpecVersion", wintypes.WORD),
            ("dmDriverVersion", wintypes.WORD),
            ("dmSize", wintypes.WORD),
            ("dmDriverExtra", wintypes.WORD),
            ("dmFields", wintypes.DWORD),
            ("dmPositionX", wintypes.LONG),
            ("dmPositionY", wintypes.LONG),
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
        ]

    if GDI_AVAILABLE:
        devmode = DEVMODEW()
        devmode.dmSize = ctypes.sizeof(devmode)

        if user32.EnumDisplaySettingsW(device.device_name, -1, ctypes.byref(devmode)):  # ENUM_CURRENT_SETTINGS
            info.resolution = (devmode.dmPelsWidth, devmode.dmPelsHeight)
            info.refresh_rate = float(devmode.dmDisplayFrequency)

    # Get current profile
    info.current_profile = get_display_profile(device.device_name)

    return info


# =============================================================================
# Profile Installation
# =============================================================================

def get_profile_directory() -> Path:
    """Get the system color profile directory."""
    import os

    # Get Windows system directory
    system_root = os.environ.get('SystemRoot', r'C:\Windows')
    color_dir = Path(system_root) / 'System32' / 'spool' / 'drivers' / 'color'

    if color_dir.exists():
        return color_dir

    # Fallback locations
    fallbacks = [
        Path(r'C:\Windows\System32\spool\drivers\color'),
        Path(r'C:\WINDOWS\system32\spool\drivers\color'),
    ]

    for fallback in fallbacks:
        if fallback.exists():
            return fallback

    # Return default even if not exists
    return Path(r'C:\Windows\System32\spool\drivers\color')


def install_profile(
    profile_path: Union[str, Path],
    scope: ProfileScope = ProfileScope.SYSTEM
) -> Tuple[bool, str]:
    """
    Install ICC profile to system.

    Args:
        profile_path: Path to ICC profile
        scope: Installation scope (SYSTEM or USER)

    Returns:
        (success, message)
    """
    profile_path = Path(profile_path)

    if not profile_path.exists():
        return False, f"Profile not found: {profile_path}"

    # Validate profile
    try:
        data = profile_path.read_bytes()
        if len(data) < 128 or data[36:40] != b'acsp':
            return False, "Invalid ICC profile"
    except Exception as e:
        return False, f"Cannot read profile: {e}"

    # Copy to system directory
    color_dir = get_profile_directory()
    dest_path = color_dir / profile_path.name

    try:
        shutil.copy2(profile_path, dest_path)
    except PermissionError:
        return False, "Permission denied. Run as administrator for system-wide installation."
    except Exception as e:
        return False, f"Failed to copy profile: {e}"

    # Register with Windows Color Management
    if MSCMS_AVAILABLE:
        try:
            result = mscms.InstallColorProfileW(None, str(dest_path))
            if not result:
                return False, "Windows color management rejected the profile"
        except Exception as e:
            return False, f"Color management error: {e}"

    return True, f"Profile installed: {dest_path}"


def uninstall_profile(profile_name: str) -> Tuple[bool, str]:
    """
    Uninstall ICC profile from system.

    Args:
        profile_name: Profile filename (e.g., "calibration.icc")

    Returns:
        (success, message)
    """
    color_dir = get_profile_directory()
    profile_path = color_dir / profile_name

    if not profile_path.exists():
        return False, f"Profile not found: {profile_name}"

    # Unregister with Windows
    if MSCMS_AVAILABLE:
        try:
            mscms.UninstallColorProfileW(None, str(profile_path), True)
        except Exception:
            pass

    # Delete file
    try:
        profile_path.unlink()
    except PermissionError:
        return False, "Permission denied. Run as administrator."
    except Exception as e:
        return False, f"Failed to delete profile: {e}"

    return True, f"Profile uninstalled: {profile_name}"


# =============================================================================
# Profile Association
# =============================================================================

def associate_profile_with_display(
    profile_name: str,
    device_name: str,
    make_default: bool = True
) -> Tuple[bool, str]:
    """
    Associate ICC profile with a display.

    Args:
        profile_name: Profile filename
        device_name: Display device name (e.g., "\\\\.\\DISPLAY1")
        make_default: Set as default profile

    Returns:
        (success, message)
    """
    if not MSCMS_AVAILABLE:
        return False, "Color management API not available"

    color_dir = get_profile_directory()
    profile_path = color_dir / profile_name

    if not profile_path.exists():
        return False, f"Profile not found: {profile_name}"

    try:
        # Associate profile with device
        result = mscms.WcsAssociateColorProfileWithDevice(
            0,  # scope
            profile_name.encode('utf-16-le') + b'\x00\x00',
            device_name.encode('utf-16-le') + b'\x00\x00'
        )

        if not result:
            return False, "Failed to associate profile"

        if make_default:
            # Set as default
            result = mscms.WcsSetDefaultColorProfile(
                0,  # scope
                device_name.encode('utf-16-le') + b'\x00\x00',
                2,  # display device type
                0,  # subtype
                0,  # profile ID
                profile_name.encode('utf-16-le') + b'\x00\x00'
            )

        return True, f"Profile {profile_name} associated with {device_name}"

    except Exception as e:
        return False, f"Association error: {e}"


def disassociate_profile_from_display(
    profile_name: str,
    device_name: str
) -> Tuple[bool, str]:
    """
    Remove profile association from display.

    Args:
        profile_name: Profile filename
        device_name: Display device name

    Returns:
        (success, message)
    """
    if not MSCMS_AVAILABLE:
        return False, "Color management API not available"

    try:
        result = mscms.WcsDisassociateColorProfileFromDevice(
            0,
            profile_name.encode('utf-16-le') + b'\x00\x00',
            device_name.encode('utf-16-le') + b'\x00\x00'
        )

        if result:
            return True, "Profile disassociated"
        else:
            return False, "Failed to disassociate profile"

    except Exception as e:
        return False, f"Error: {e}"


def get_display_profile(device_name: str) -> Optional[str]:
    """
    Get the default profile for a display.

    Args:
        device_name: Display device name

    Returns:
        Profile filename or None
    """
    if not MSCMS_AVAILABLE:
        return None

    try:
        buffer = ctypes.create_unicode_buffer(260)
        size = wintypes.DWORD(260)

        result = mscms.WcsGetDefaultColorProfile(
            0,  # scope
            device_name.encode('utf-16-le') + b'\x00\x00',
            2,  # display device type
            0,  # subtype
            0,  # profile ID
            size,
            buffer
        )

        if result:
            return buffer.value

    except Exception:
        pass

    return None


def get_associated_profiles(device_name: str) -> List[str]:
    """
    Get all profiles associated with a display.

    Args:
        device_name: Display device name

    Returns:
        List of profile filenames
    """
    profiles = []

    # Read from registry
    try:
        key_path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ICM\ProfileAssociations\Display"
        key_path += "\\" + device_name.replace("\\", "_")

        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, key_path) as key:
            i = 0
            while True:
                try:
                    name, value, _ = winreg.EnumValue(key, i)
                    if value:
                        profiles.append(value)
                    i += 1
                except OSError:
                    break

    except Exception:
        pass

    return profiles


# =============================================================================
# Profile Backup and Restore
# =============================================================================

@dataclass
class ProfileBackup:
    """Profile backup data."""
    timestamp: str
    profiles: Dict[str, str]  # display_name -> profile_name
    profile_data: Dict[str, bytes]  # profile_name -> bytes

    def to_dict(self) -> Dict:
        """Convert to dictionary (profiles only, not bytes)."""
        return {
            "timestamp": self.timestamp,
            "profiles": self.profiles
        }


def backup_profiles(
    backup_dir: Union[str, Path],
    include_data: bool = True
) -> Tuple[bool, str]:
    """
    Backup current display profile assignments.

    Args:
        backup_dir: Directory for backup
        include_data: Include profile files in backup

    Returns:
        (success, message)
    """
    backup_dir = Path(backup_dir)
    backup_dir.mkdir(parents=True, exist_ok=True)

    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    backup_name = f"profile_backup_{timestamp}"

    profiles = {}
    profile_data = {}

    # Get current assignments
    for display in enumerate_displays():
        profile = get_display_profile(display.device_name)
        if profile:
            profiles[display.device_name] = profile

            if include_data:
                color_dir = get_profile_directory()
                profile_path = color_dir / profile

                if profile_path.exists():
                    profile_data[profile] = profile_path.read_bytes()

    # Save backup
    backup_info = {
        "timestamp": timestamp,
        "profiles": profiles
    }

    info_path = backup_dir / f"{backup_name}.json"
    info_path.write_text(json.dumps(backup_info, indent=2))

    if include_data:
        data_dir = backup_dir / backup_name
        data_dir.mkdir(exist_ok=True)

        for name, data in profile_data.items():
            (data_dir / name).write_bytes(data)

    return True, f"Backup created: {backup_name}"


def restore_profiles(
    backup_path: Union[str, Path],
    restore_data: bool = True
) -> Tuple[bool, str]:
    """
    Restore profile assignments from backup.

    Args:
        backup_path: Path to backup JSON file
        restore_data: Also restore profile files

    Returns:
        (success, message)
    """
    backup_path = Path(backup_path)

    if not backup_path.exists():
        return False, f"Backup not found: {backup_path}"

    try:
        backup_info = json.loads(backup_path.read_text())
    except Exception as e:
        return False, f"Cannot read backup: {e}"

    profiles = backup_info.get("profiles", {})
    backup_name = backup_path.stem
    data_dir = backup_path.parent / backup_name

    restored = 0
    errors = []

    for device_name, profile_name in profiles.items():
        # Restore profile file if needed
        if restore_data and data_dir.exists():
            profile_file = data_dir / profile_name
            if profile_file.exists():
                success, msg = install_profile(profile_file)
                if not success:
                    errors.append(msg)
                    continue

        # Associate profile
        success, msg = associate_profile_with_display(profile_name, device_name)
        if success:
            restored += 1
        else:
            errors.append(msg)

    if errors:
        return False, f"Restored {restored} profiles with errors: {'; '.join(errors)}"

    return True, f"Restored {restored} profile assignments"


def list_installed_profiles() -> List[str]:
    """
    List all installed ICC profiles.

    Returns:
        List of profile filenames
    """
    color_dir = get_profile_directory()

    profiles = []

    for ext in ['*.icc', '*.icm']:
        profiles.extend([p.name for p in color_dir.glob(ext)])

    return sorted(profiles)


# =============================================================================
# Profile Loader (VCGT/Gamma Ramp)
# =============================================================================

def load_profile_vcgt(
    profile_path: Union[str, Path],
    display_id: int = 0
) -> Tuple[bool, str]:
    """
    Load VCGT from profile and apply to display gamma ramp.

    Args:
        profile_path: Path to ICC profile
        display_id: Display index

    Returns:
        (success, message)
    """
    from calibrate_pro.profiles.vcgt import (
        extract_vcgt_from_profile,
        GammaRampController
    )

    vcgt = extract_vcgt_from_profile(profile_path)

    if vcgt is None:
        return False, "No VCGT tag in profile"

    controller = GammaRampController()

    if not controller.is_available:
        return False, "Gamma ramp controller not available"

    if controller.set_gamma_ramp(vcgt, display_id):
        return True, "VCGT applied to display"
    else:
        return False, "Failed to apply gamma ramp"


def reset_display_gamma(display_id: int = 0) -> Tuple[bool, str]:
    """
    Reset display to linear gamma ramp.

    Args:
        display_id: Display index

    Returns:
        (success, message)
    """
    from calibrate_pro.profiles.vcgt import GammaRampController

    controller = GammaRampController()

    if not controller.is_available:
        return False, "Gamma ramp controller not available"

    if controller.reset_gamma_ramp(display_id):
        return True, "Display gamma reset to linear"
    else:
        return False, "Failed to reset gamma"


# =============================================================================
# Convenience Functions
# =============================================================================

def quick_calibrate_display(
    profile_path: Union[str, Path],
    display_id: int = 0,
    make_default: bool = True,
    apply_vcgt: bool = True
) -> Tuple[bool, str]:
    """
    Quick display calibration: install profile, set as default, apply VCGT.

    Args:
        profile_path: Path to ICC profile
        display_id: Display index
        make_default: Set as default profile
        apply_vcgt: Apply VCGT immediately

    Returns:
        (success, message)
    """
    messages = []

    # Get display device name
    displays = enumerate_displays()

    if display_id >= len(displays):
        return False, f"Display {display_id} not found"

    device = displays[display_id]

    # Install profile
    success, msg = install_profile(profile_path)
    messages.append(msg)

    if not success:
        return False, "; ".join(messages)

    # Get profile name
    profile_name = Path(profile_path).name

    # Associate with display
    if make_default:
        success, msg = associate_profile_with_display(
            profile_name,
            device.device_name,
            make_default=True
        )
        messages.append(msg)

    # Apply VCGT
    if apply_vcgt:
        success, msg = load_profile_vcgt(profile_path, display_id)
        messages.append(msg)

    return True, "; ".join(messages)


def get_display_calibration_status() -> List[Dict]:
    """
    Get calibration status for all displays.

    Returns:
        List of display status dictionaries
    """
    status = []

    for display in enumerate_displays():
        info = get_monitor_info(display)

        entry = {
            "device_name": display.device_name,
            "display_name": display.device_string,
            "monitor_name": display.monitor_name,
            "is_primary": display.is_primary,
            "resolution": info.resolution,
            "refresh_rate": info.refresh_rate,
            "current_profile": info.current_profile,
            "calibrated": info.current_profile is not None,
            "hdr_supported": info.hdr_supported
        }

        status.append(entry)

    return status
