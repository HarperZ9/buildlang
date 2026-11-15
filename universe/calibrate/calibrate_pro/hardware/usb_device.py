"""
USB/HID Device Communication Layer

Provides low-level USB communication for colorimeters without ArgyllCMS.
Supports both HID (Human Interface Device) and raw USB protocols.
"""

import struct
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Dict, List, Optional, Tuple, Union
import threading

# Try to import USB libraries
HID_AVAILABLE = False
USB_AVAILABLE = False

try:
    import hid
    HID_AVAILABLE = True
except ImportError:
    try:
        import hidapi as hid
        HID_AVAILABLE = True
    except ImportError:
        pass

try:
    import usb.core
    import usb.util
    USB_AVAILABLE = True
except ImportError:
    pass


class USBError(Exception):
    """USB communication error."""
    pass


class DeviceNotFoundError(USBError):
    """Device not found."""
    pass


class CommunicationError(USBError):
    """Communication with device failed."""
    pass


@dataclass
class USBDeviceInfo:
    """Information about a USB device."""
    vendor_id: int
    product_id: int
    manufacturer: str
    product: str
    serial_number: str
    path: Optional[bytes] = None  # HID device path
    bus: Optional[int] = None
    address: Optional[int] = None

    @property
    def vid_pid(self) -> str:
        return f"{self.vendor_id:04x}:{self.product_id:04x}"


# Known colorimeter USB IDs
COLORIMETER_USB_IDS = {
    # X-Rite devices
    (0x0765, 0x5001): "i1Display Pro",
    (0x0765, 0x5011): "i1Display Pro Plus",
    (0x0765, 0x5020): "i1Display Studio",
    (0x0765, 0x5010): "ColorMunki Display",
    (0x0765, 0x6003): "i1Pro",
    (0x0765, 0x6008): "i1Pro 2",
    (0x0765, 0x6009): "i1Pro 3",
    (0x0765, 0xD094): "i1Display 2",
    (0x0765, 0xD095): "i1Display LT",
    (0x0765, 0xD096): "ColorMunki Smile",

    # Datacolor Spyder devices
    (0x085C, 0x0200): "Spyder2",
    (0x085C, 0x0300): "Spyder3",
    (0x085C, 0x0400): "Spyder4",
    (0x085C, 0x0500): "Spyder5",
    (0x085C, 0x0600): "SpyderX",
    (0x085C, 0x0700): "SpyderX2",

    # Calibrite (rebranded X-Rite)
    (0x0765, 0x5021): "ColorChecker Display",
    (0x0765, 0x5022): "ColorChecker Display Pro",
    (0x0765, 0x5023): "ColorChecker Display Plus",
}


class USBTransport(ABC):
    """Abstract USB transport layer."""

    @abstractmethod
    def open(self, device_info: USBDeviceInfo) -> bool:
        """Open connection to device."""
        pass

    @abstractmethod
    def close(self) -> None:
        """Close connection."""
        pass

    @abstractmethod
    def write(self, data: bytes, timeout: int = 1000) -> int:
        """Write data to device."""
        pass

    @abstractmethod
    def read(self, size: int, timeout: int = 1000) -> bytes:
        """Read data from device."""
        pass

    @property
    @abstractmethod
    def is_open(self) -> bool:
        """Check if device is open."""
        pass


class HIDTransport(USBTransport):
    """HID-based USB transport."""

    def __init__(self):
        if not HID_AVAILABLE:
            raise USBError("HID library not available. Install 'hidapi' package.")
        self._device = None
        self._lock = threading.Lock()

    def open(self, device_info: USBDeviceInfo) -> bool:
        """Open HID device."""
        with self._lock:
            try:
                self._device = hid.device()
                if device_info.path:
                    self._device.open_path(device_info.path)
                else:
                    self._device.open(device_info.vendor_id, device_info.product_id)
                self._device.set_nonblocking(False)
                return True
            except Exception as e:
                self._device = None
                raise CommunicationError(f"Failed to open HID device: {e}")

    def close(self) -> None:
        """Close HID device."""
        with self._lock:
            if self._device:
                try:
                    self._device.close()
                except Exception:
                    pass
                self._device = None

    def write(self, data: bytes, timeout: int = 1000) -> int:
        """Write to HID device."""
        with self._lock:
            if not self._device:
                raise CommunicationError("Device not open")
            try:
                # HID writes need report ID as first byte
                if len(data) < 64:
                    data = data + bytes(64 - len(data))
                return self._device.write(data)
            except Exception as e:
                raise CommunicationError(f"Write failed: {e}")

    def read(self, size: int, timeout: int = 1000) -> bytes:
        """Read from HID device."""
        with self._lock:
            if not self._device:
                raise CommunicationError("Device not open")
            try:
                data = self._device.read(size, timeout_ms=timeout)
                return bytes(data) if data else b''
            except Exception as e:
                raise CommunicationError(f"Read failed: {e}")

    @property
    def is_open(self) -> bool:
        return self._device is not None


class RawUSBTransport(USBTransport):
    """Raw USB transport using PyUSB."""

    def __init__(self):
        if not USB_AVAILABLE:
            raise USBError("PyUSB not available. Install 'pyusb' package.")
        self._device = None
        self._ep_in = None
        self._ep_out = None
        self._lock = threading.Lock()

    def open(self, device_info: USBDeviceInfo) -> bool:
        """Open raw USB device."""
        with self._lock:
            try:
                self._device = usb.core.find(
                    idVendor=device_info.vendor_id,
                    idProduct=device_info.product_id
                )
                if self._device is None:
                    raise DeviceNotFoundError("Device not found")

                # Detach kernel driver if needed
                try:
                    if self._device.is_kernel_driver_active(0):
                        self._device.detach_kernel_driver(0)
                except (usb.core.USBError, NotImplementedError):
                    pass

                # Set configuration
                self._device.set_configuration()

                # Find endpoints
                cfg = self._device.get_active_configuration()
                intf = cfg[(0, 0)]

                self._ep_out = usb.util.find_descriptor(
                    intf,
                    custom_match=lambda e: usb.util.endpoint_direction(e.bEndpointAddress) == usb.util.ENDPOINT_OUT
                )
                self._ep_in = usb.util.find_descriptor(
                    intf,
                    custom_match=lambda e: usb.util.endpoint_direction(e.bEndpointAddress) == usb.util.ENDPOINT_IN
                )

                return True
            except Exception as e:
                self._device = None
                raise CommunicationError(f"Failed to open USB device: {e}")

    def close(self) -> None:
        """Close USB device."""
        with self._lock:
            if self._device:
                try:
                    usb.util.dispose_resources(self._device)
                except Exception:
                    pass
                self._device = None

    def write(self, data: bytes, timeout: int = 1000) -> int:
        """Write to USB device."""
        with self._lock:
            if not self._device or not self._ep_out:
                raise CommunicationError("Device not open")
            try:
                return self._ep_out.write(data, timeout=timeout)
            except Exception as e:
                raise CommunicationError(f"Write failed: {e}")

    def read(self, size: int, timeout: int = 1000) -> bytes:
        """Read from USB device."""
        with self._lock:
            if not self._device or not self._ep_in:
                raise CommunicationError("Device not open")
            try:
                data = self._ep_in.read(size, timeout=timeout)
                return bytes(data)
            except Exception as e:
                raise CommunicationError(f"Read failed: {e}")

    @property
    def is_open(self) -> bool:
        return self._device is not None


def enumerate_hid_devices() -> List[USBDeviceInfo]:
    """Enumerate all HID colorimeter devices."""
    devices = []

    if not HID_AVAILABLE:
        return devices

    try:
        for dev in hid.enumerate():
            vid = dev.get('vendor_id', 0)
            pid = dev.get('product_id', 0)

            if (vid, pid) in COLORIMETER_USB_IDS:
                devices.append(USBDeviceInfo(
                    vendor_id=vid,
                    product_id=pid,
                    manufacturer=dev.get('manufacturer_string', '') or COLORIMETER_USB_IDS.get((vid, pid), ''),
                    product=dev.get('product_string', '') or COLORIMETER_USB_IDS.get((vid, pid), ''),
                    serial_number=dev.get('serial_number', '') or '',
                    path=dev.get('path')
                ))
    except Exception:
        pass

    return devices


def enumerate_usb_devices() -> List[USBDeviceInfo]:
    """Enumerate all USB colorimeter devices."""
    devices = []

    if not USB_AVAILABLE:
        return devices

    try:
        for vid, pid in COLORIMETER_USB_IDS.keys():
            dev = usb.core.find(idVendor=vid, idProduct=pid)
            if dev:
                try:
                    manufacturer = usb.util.get_string(dev, dev.iManufacturer) if dev.iManufacturer else ''
                    product = usb.util.get_string(dev, dev.iProduct) if dev.iProduct else ''
                    serial = usb.util.get_string(dev, dev.iSerialNumber) if dev.iSerialNumber else ''
                except Exception:
                    manufacturer = product = serial = ''

                devices.append(USBDeviceInfo(
                    vendor_id=vid,
                    product_id=pid,
                    manufacturer=manufacturer or COLORIMETER_USB_IDS.get((vid, pid), ''),
                    product=product or COLORIMETER_USB_IDS.get((vid, pid), ''),
                    serial_number=serial,
                    bus=dev.bus,
                    address=dev.address
                ))
    except Exception:
        pass

    return devices


def enumerate_all_colorimeters() -> List[USBDeviceInfo]:
    """Enumerate colorimeters using both HID and raw USB."""
    devices = []
    seen = set()

    # Try HID first (usually works better on Windows)
    for dev in enumerate_hid_devices():
        key = (dev.vendor_id, dev.product_id, dev.serial_number)
        if key not in seen:
            seen.add(key)
            devices.append(dev)

    # Try raw USB for any not found via HID
    for dev in enumerate_usb_devices():
        key = (dev.vendor_id, dev.product_id, dev.serial_number)
        if key not in seen:
            seen.add(key)
            devices.append(dev)

    return devices


def get_transport(device_info: USBDeviceInfo) -> USBTransport:
    """Get appropriate transport for device."""
    # Prefer HID for most colorimeters
    if HID_AVAILABLE and device_info.path:
        return HIDTransport()
    elif HID_AVAILABLE:
        return HIDTransport()
    elif USB_AVAILABLE:
        return RawUSBTransport()
    else:
        raise USBError("No USB library available. Install 'hidapi' or 'pyusb'.")


def check_usb_available() -> Tuple[bool, str]:
    """Check if USB communication is available."""
    if HID_AVAILABLE:
        return True, "HID library available"
    elif USB_AVAILABLE:
        return True, "PyUSB available"
    else:
        return False, "No USB library. Install 'hidapi' (pip install hidapi) or 'pyusb' (pip install pyusb)"
