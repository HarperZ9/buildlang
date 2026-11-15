"""
Hardware Colorimeter Integration Module

Provides unified interface for professional colorimeters and spectrophotometers:
- X-Rite i1Display Pro/Plus
- Datacolor Spyder X/X2
- Calibrite ColorChecker Display Pro/Plus
- X-Rite i1Pro/i1Pro2/i1Pro3 spectrophotometers

DDC/CI Hardware Control:
- Direct monitor control via VESA DDC/CI standard
- RGB gain/black level adjustment for hardware calibration
- Brightness/contrast control
- Support for hardware LUT upload (professional monitors)

Two backends available:
1. Native USB drivers (no external dependencies)
2. ArgyllCMS backend (requires ArgyllCMS installation)

Native drivers are preferred and used by default.
"""

from calibrate_pro.hardware.colorimeter_base import (
    ColorimeterBase,
    ColorMeasurement,
    DeviceInfo,
    DeviceType,
    CalibrationMode,
    MeasurementType,
    CalibrationPatch,
    generate_grayscale_patches,
    generate_primary_patches,
    generate_verification_patches,
    generate_profiling_patches,
)

# Check what's available
def _check_backends():
    """Check available backends."""
    native_ok = False
    argyll_ok = False

    try:
        from calibrate_pro.hardware.usb_device import check_usb_available
        native_ok, _ = check_usb_available()
    except ImportError:
        pass

    try:
        from calibrate_pro.hardware.argyll_backend import check_argyll_installation
        argyll_ok = check_argyll_installation()
    except ImportError:
        pass

    return native_ok, argyll_ok


# Lazy imports for drivers
def __getattr__(name):
    # Native drivers (preferred)
    if name == "NativeBackend":
        from calibrate_pro.hardware.native_backend import NativeBackend
        return NativeBackend
    elif name == "I1DisplayNative":
        from calibrate_pro.hardware.i1display_native import I1DisplayNative
        return I1DisplayNative
    elif name == "SpyderNative":
        from calibrate_pro.hardware.spyder_native import SpyderNative
        return SpyderNative
    elif name == "detect_i1display_native":
        from calibrate_pro.hardware.i1display_native import detect_i1display_native
        return detect_i1display_native
    elif name == "detect_spyder_native":
        from calibrate_pro.hardware.spyder_native import detect_spyder_native
        return detect_spyder_native

    # ArgyllCMS backend (fallback)
    elif name == "ArgyllBackend":
        from calibrate_pro.hardware.argyll_backend import ArgyllBackend
        return ArgyllBackend
    elif name == "check_argyll_installation":
        from calibrate_pro.hardware.argyll_backend import check_argyll_installation
        return check_argyll_installation
    elif name == "get_argyll_config":
        from calibrate_pro.hardware.argyll_backend import get_argyll_config
        return get_argyll_config
    elif name == "set_argyll_path":
        from calibrate_pro.hardware.argyll_backend import set_argyll_path
        return set_argyll_path

    # ArgyllCMS-based drivers
    elif name == "I1DisplayDriver":
        from calibrate_pro.hardware.i1display import I1DisplayDriver
        return I1DisplayDriver
    elif name == "detect_i1display":
        from calibrate_pro.hardware.i1display import detect_i1display
        return detect_i1display
    elif name == "SpyderDriver":
        from calibrate_pro.hardware.spyder import SpyderDriver
        return SpyderDriver
    elif name == "detect_spyder":
        from calibrate_pro.hardware.spyder import detect_spyder
        return detect_spyder
    elif name == "SpectrophotometerDriver":
        from calibrate_pro.hardware.spectro import SpectrophotometerDriver
        return SpectrophotometerDriver
    elif name == "ColorCheckerDisplay":
        from calibrate_pro.hardware.spectro import ColorCheckerDisplay
        return ColorCheckerDisplay
    elif name == "I1Pro":
        from calibrate_pro.hardware.spectro import I1Pro
        return I1Pro
    elif name == "detect_spectrophotometer":
        from calibrate_pro.hardware.spectro import detect_spectrophotometer
        return detect_spectrophotometer
    elif name == "detect_colorchecker_display":
        from calibrate_pro.hardware.spectro import detect_colorchecker_display
        return detect_colorchecker_display

    # USB layer
    elif name == "check_usb_available":
        from calibrate_pro.hardware.usb_device import check_usb_available
        return check_usb_available
    elif name == "enumerate_all_colorimeters":
        from calibrate_pro.hardware.usb_device import enumerate_all_colorimeters
        return enumerate_all_colorimeters

    # Hardware Calibration Engine
    elif name == "HardwareCalibrationEngine":
        from calibrate_pro.hardware.hardware_calibration import HardwareCalibrationEngine
        return HardwareCalibrationEngine
    elif name == "CalibrationTargets":
        from calibrate_pro.hardware.hardware_calibration import CalibrationTargets
        return CalibrationTargets
    elif name == "CalibrationPhase":
        from calibrate_pro.hardware.hardware_calibration import CalibrationPhase
        return CalibrationPhase
    elif name == "MeasurementResult":
        from calibrate_pro.hardware.hardware_calibration import MeasurementResult
        return MeasurementResult
    elif name == "HardwareCalibrationResult":
        from calibrate_pro.hardware.hardware_calibration import HardwareCalibrationResult as HWCalResult
        return HWCalResult

    # Sensorless Calibration Engine
    elif name == "SensorlessCalibrationEngine":
        from calibrate_pro.hardware.sensorless_calibration import SensorlessCalibrationEngine
        return SensorlessCalibrationEngine
    elif name == "CalibrationTarget":
        from calibrate_pro.hardware.sensorless_calibration import CalibrationTarget
        return CalibrationTarget
    elif name == "run_sensorless_calibration":
        from calibrate_pro.hardware.sensorless_calibration import run_sensorless_calibration
        return run_sensorless_calibration
    elif name == "auto_calibrate":
        from calibrate_pro.hardware.sensorless_calibration import auto_calibrate
        return auto_calibrate
    elif name == "detect_displays":
        from calibrate_pro.hardware.sensorless_calibration import detect_displays
        return detect_displays
    elif name == "DisplayInfo":
        from calibrate_pro.hardware.sensorless_calibration import DisplayInfo
        return DisplayInfo
    elif name == "ILLUMINANTS":
        from calibrate_pro.hardware.sensorless_calibration import ILLUMINANTS
        return ILLUMINANTS
    elif name == "GAMUT_PRIMARIES":
        from calibrate_pro.hardware.sensorless_calibration import GAMUT_PRIMARIES
        return GAMUT_PRIMARIES
    elif name == "apply_lut":
        from calibrate_pro.hardware.sensorless_calibration import apply_lut
        return apply_lut
    elif name == "remove_lut":
        from calibrate_pro.hardware.sensorless_calibration import remove_lut
        return remove_lut
    elif name == "get_lut_status":
        from calibrate_pro.hardware.sensorless_calibration import get_lut_status
        return get_lut_status

    # DDC/CI Hardware Control
    elif name == "DDCCIController":
        from calibrate_pro.hardware.ddc_ci import DDCCIController
        return DDCCIController
    elif name == "VCPCode":
        from calibrate_pro.hardware.ddc_ci import VCPCode
        return VCPCode
    elif name == "ColorPreset":
        from calibrate_pro.hardware.ddc_ci import ColorPreset
        return ColorPreset
    elif name == "MonitorCapabilities":
        from calibrate_pro.hardware.ddc_ci import MonitorCapabilities
        return MonitorCapabilities
    elif name == "MonitorSettings":
        from calibrate_pro.hardware.ddc_ci import MonitorSettings
        return MonitorSettings
    elif name == "HardwareCalibrator":
        from calibrate_pro.hardware.ddc_ci import HardwareCalibrator
        return HardwareCalibrator
    elif name == "HardwareCalibrationTarget":
        from calibrate_pro.hardware.ddc_ci import HardwareCalibrationTarget
        return HardwareCalibrationTarget
    elif name == "HardwareCalibrationResult":
        from calibrate_pro.hardware.ddc_ci import HardwareCalibrationResult
        return HardwareCalibrationResult
    elif name == "detect_ddc_monitors":
        from calibrate_pro.hardware.ddc_ci import detect_ddc_monitors
        return detect_ddc_monitors

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


__all__ = [
    # Base classes
    "ColorimeterBase",
    "ColorMeasurement",
    "DeviceInfo",
    "DeviceType",
    "CalibrationMode",
    "MeasurementType",
    "CalibrationPatch",
    # Patch generators
    "generate_grayscale_patches",
    "generate_primary_patches",
    "generate_verification_patches",
    "generate_profiling_patches",
    # Native backend (preferred)
    "NativeBackend",
    "I1DisplayNative",
    "SpyderNative",
    "detect_i1display_native",
    "detect_spyder_native",
    "check_usb_available",
    "enumerate_all_colorimeters",
    # ArgyllCMS backend (fallback)
    "ArgyllBackend",
    "check_argyll_installation",
    "get_argyll_config",
    "set_argyll_path",
    # ArgyllCMS-based device drivers
    "I1DisplayDriver",
    "SpyderDriver",
    "SpectrophotometerDriver",
    "ColorCheckerDisplay",
    "I1Pro",
    # Detection functions
    "detect_i1display",
    "detect_spyder",
    "detect_spectrophotometer",
    "detect_colorchecker_display",
    # Main API
    "detect_all_devices",
    "auto_connect",
    # Hardware Calibration Engine
    "HardwareCalibrationEngine",
    "CalibrationTargets",
    "CalibrationPhase",
    "MeasurementResult",
    # Sensorless Calibration Engine
    "SensorlessCalibrationEngine",
    "CalibrationTarget",
    "run_sensorless_calibration",
    "auto_calibrate",
    "detect_displays",
    "DisplayInfo",
    "ILLUMINANTS",
    "GAMUT_PRIMARIES",
    # System-wide LUT functions
    "apply_lut",
    "remove_lut",
    "get_lut_status",
    # DDC/CI Hardware Control
    "DDCCIController",
    "VCPCode",
    "ColorPreset",
    "MonitorCapabilities",
    "MonitorSettings",
    "HardwareCalibrator",
    "HardwareCalibrationTarget",
    "HardwareCalibrationResult",
    "detect_ddc_monitors",
]


def detect_all_devices():
    """
    Detect all connected colorimeters and spectrophotometers.

    Uses native USB drivers first, falls back to ArgyllCMS if needed.

    Returns list of DeviceInfo objects.
    """
    devices = []

    # Try native detection first
    try:
        from calibrate_pro.hardware.native_backend import detect_colorimeters
        native_devices = detect_colorimeters()
        devices.extend(native_devices)
    except Exception:
        pass

    # If no native devices, try ArgyllCMS
    if not devices:
        try:
            from calibrate_pro.hardware.argyll_backend import ArgyllBackend
            argyll = ArgyllBackend()
            argyll_devices = argyll.detect_devices()
            devices.extend(argyll_devices)
        except Exception:
            pass

    return devices


def auto_connect(prefer_native: bool = True):
    """
    Automatically detect and connect to the best available device.

    Args:
        prefer_native: If True, prefer native USB drivers over ArgyllCMS

    Preference order:
    1. Spectrophotometer (highest accuracy)
    2. i1Display Pro/Plus
    3. Spyder X/X2

    Returns connected driver or None.
    """
    if prefer_native:
        # Try native drivers first
        try:
            from calibrate_pro.hardware.native_backend import auto_connect as native_auto
            driver = native_auto()
            if driver:
                return driver
        except Exception:
            pass

    # Fall back to ArgyllCMS
    try:
        from calibrate_pro.hardware.spectro import detect_spectrophotometer
        driver = detect_spectrophotometer()
        if driver:
            return driver
    except Exception:
        pass

    try:
        from calibrate_pro.hardware.i1display import detect_i1display
        driver = detect_i1display()
        if driver:
            return driver
    except Exception:
        pass

    try:
        from calibrate_pro.hardware.spyder import detect_spyder
        driver = detect_spyder()
        if driver:
            return driver
    except Exception:
        pass

    return None


def get_backend_info():
    """Get information about available backends."""
    info = {
        "native_usb": {"available": False, "message": ""},
        "argyll": {"available": False, "message": ""}
    }

    try:
        from calibrate_pro.hardware.usb_device import check_usb_available
        available, msg = check_usb_available()
        info["native_usb"]["available"] = available
        info["native_usb"]["message"] = msg
    except ImportError as e:
        info["native_usb"]["message"] = str(e)

    try:
        from calibrate_pro.hardware.argyll_backend import check_argyll_installation
        available = check_argyll_installation()
        info["argyll"]["available"] = available
        info["argyll"]["message"] = "ArgyllCMS found" if available else "ArgyllCMS not found"
    except ImportError as e:
        info["argyll"]["message"] = str(e)

    return info
