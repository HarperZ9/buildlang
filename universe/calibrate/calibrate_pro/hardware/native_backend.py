"""
Native Colorimeter Backend

Unified interface for native USB colorimeter drivers.
Replaces ArgyllCMS with direct device communication.

Supports:
- X-Rite i1Display Pro/Plus/Studio
- Datacolor SpyderX/X2/5
- Calibrite ColorChecker Display
"""

from typing import Dict, List, Optional, Tuple, Type
from pathlib import Path
import numpy as np

from calibrate_pro.hardware.colorimeter_base import (
    ColorimeterBase, ColorMeasurement, DeviceInfo, DeviceType,
    CalibrationMode, CalibrationPatch,
    generate_grayscale_patches, generate_primary_patches,
    generate_verification_patches, generate_profiling_patches
)
from calibrate_pro.hardware.usb_device import (
    enumerate_all_colorimeters, check_usb_available,
    USBDeviceInfo, COLORIMETER_USB_IDS
)


class NativeBackend(ColorimeterBase):
    """
    Native colorimeter backend with automatic device detection.

    Replaces ArgyllCMS with direct USB communication.
    Automatically selects the appropriate driver for connected devices.
    """

    def __init__(self):
        super().__init__()
        self._driver: Optional[ColorimeterBase] = None
        self._driver_class: Optional[Type[ColorimeterBase]] = None

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect all connected colorimeters."""
        devices = []
        usb_available, msg = check_usb_available()

        if not usb_available:
            self._report_progress(f"USB not available: {msg}", 0)
            return devices

        # Enumerate USB devices
        usb_devices = enumerate_all_colorimeters()

        for usb_dev in usb_devices:
            # Determine device type and create DeviceInfo
            name = COLORIMETER_USB_IDS.get(
                (usb_dev.vendor_id, usb_dev.product_id),
                "Unknown Colorimeter"
            )

            # Determine device type
            if "i1Pro" in name or "ColorMunki" in name and "Display" not in name:
                dev_type = DeviceType.SPECTROPHOTOMETER
            else:
                dev_type = DeviceType.COLORIMETER

            # Determine capabilities
            caps = ["spot", "emission"]
            if any(x in name for x in ["Pro", "Elite", "Plus", "Ultra"]):
                caps.extend(["ambient", "refresh"])
            if "OLED" in name or "Pro Plus" in name:
                caps.append("oled_mode")

            devices.append(DeviceInfo(
                name=name,
                manufacturer=usb_dev.manufacturer or self._get_manufacturer(usb_dev.vendor_id),
                model=name,
                serial=usb_dev.serial_number or "",
                device_type=dev_type,
                capabilities=caps
            ))

        return devices

    def _get_manufacturer(self, vendor_id: int) -> str:
        """Get manufacturer name from vendor ID."""
        manufacturers = {
            0x0765: "X-Rite",
            0x085C: "Datacolor",
        }
        return manufacturers.get(vendor_id, "Unknown")

    def _get_driver_for_device(self, usb_dev: USBDeviceInfo) -> Optional[ColorimeterBase]:
        """Get appropriate driver for device."""
        vid, pid = usb_dev.vendor_id, usb_dev.product_id

        # X-Rite devices
        if vid == 0x0765:
            # i1Display family
            if pid in [0x5001, 0x5011, 0x5020, 0x5010, 0x5021, 0x5022, 0x5023]:
                from calibrate_pro.hardware.i1display_native import I1DisplayNative
                return I1DisplayNative()
            # i1Pro family would go here

        # Datacolor Spyder devices
        elif vid == 0x085C:
            from calibrate_pro.hardware.spyder_native import SpyderNative
            return SpyderNative()

        return None

    def connect(self, device_index: int = 0) -> bool:
        """Connect to a colorimeter."""
        usb_devices = enumerate_all_colorimeters()

        if device_index >= len(usb_devices):
            return False

        usb_dev = usb_devices[device_index]
        self._driver = self._get_driver_for_device(usb_dev)

        if self._driver is None:
            self._report_progress(f"No driver for device {usb_dev.vid_pid}", 0)
            return False

        # Forward progress callback
        if self.progress_callback:
            self._driver.set_progress_callback(self.progress_callback)

        if self._driver.connect(0):  # Connect to first matching device
            self.is_connected = True
            self.device_info = self._driver.device_info
            return True

        return False

    def disconnect(self) -> bool:
        """Disconnect from colorimeter."""
        if self._driver:
            result = self._driver.disconnect()
            self._driver = None
            self.is_connected = False
            return result
        return True

    def calibrate_device(self) -> bool:
        """Perform device calibration."""
        if self._driver:
            return self._driver.calibrate_device()
        return False

    def measure_spot(self) -> Optional[ColorMeasurement]:
        """Take spot measurement."""
        if self._driver:
            return self._driver.measure_spot()
        return None

    def measure_ambient(self) -> Optional[ColorMeasurement]:
        """Measure ambient light."""
        if self._driver:
            return self._driver.measure_ambient()
        return None

    def set_integration_time(self, seconds: float) -> bool:
        """Set integration time."""
        if self._driver and hasattr(self._driver, 'set_integration_time'):
            return self._driver.set_integration_time(seconds)
        return False

    def set_display_type(self, display_type: str) -> bool:
        """Set display type for optimized measurement."""
        if self._driver:
            return self._driver.set_display_type(display_type)
        return True

    def set_refresh_mode(self, refresh_rate: float) -> bool:
        """Set refresh display mode."""
        if self._driver:
            return self._driver.set_refresh_mode(refresh_rate)
        return True


def check_native_available() -> Tuple[bool, str]:
    """Check if native colorimeter support is available."""
    return check_usb_available()


def detect_colorimeters() -> List[DeviceInfo]:
    """Detect all connected colorimeters."""
    backend = NativeBackend()
    return backend.detect_devices()


def auto_connect() -> Optional[NativeBackend]:
    """
    Automatically detect and connect to a colorimeter.

    Returns connected NativeBackend or None.
    """
    backend = NativeBackend()
    devices = backend.detect_devices()

    if devices:
        # Prefer spectrophotometer, then i1Display Pro Plus, then any
        priority_order = [
            "i1Pro 3", "i1Pro 2", "i1Pro",
            "ColorChecker Display Plus", "ColorChecker Display Pro",
            "i1Display Pro Plus", "i1Display Pro",
            "SpyderX2", "SpyderX",
            "ColorMunki Display", "i1Display Studio",
            "Spyder5", "Spyder4"
        ]

        # Sort devices by priority
        def get_priority(dev: DeviceInfo) -> int:
            for i, name in enumerate(priority_order):
                if name in dev.name:
                    return i
            return len(priority_order)

        sorted_devices = sorted(
            enumerate(devices),
            key=lambda x: get_priority(x[1])
        )

        for idx, _ in sorted_devices:
            if backend.connect(idx):
                return backend

    return None


# Convenient aliases
def measure_xyz() -> Optional[Tuple[float, float, float]]:
    """Quick one-shot XYZ measurement."""
    backend = auto_connect()
    if backend:
        try:
            measurement = backend.measure_spot()
            if measurement:
                return (measurement.X, measurement.Y, measurement.Z)
        finally:
            backend.disconnect()
    return None


def get_luminance() -> Optional[float]:
    """Quick luminance measurement in cd/m²."""
    backend = auto_connect()
    if backend:
        try:
            measurement = backend.measure_spot()
            if measurement:
                return measurement.luminance
        finally:
            backend.disconnect()
    return None


def get_white_point() -> Optional[Tuple[float, float]]:
    """Quick white point measurement as xy chromaticity."""
    backend = auto_connect()
    if backend:
        try:
            measurement = backend.measure_spot()
            if measurement:
                return (measurement.x, measurement.y)
        finally:
            backend.disconnect()
    return None
