"""
Native i1Display Pro/Plus Driver

Direct USB communication with X-Rite i1Display colorimeters.
No ArgyllCMS dependency.

Supported devices:
- i1Display Pro (0765:5001)
- i1Display Pro Plus (0765:5011)
- i1Display Studio (0765:5020)
- ColorMunki Display (0765:5010)
"""

import struct
import time
import math
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple
import numpy as np

from calibrate_pro.hardware.usb_device import (
    USBDeviceInfo, USBTransport, HIDTransport,
    enumerate_all_colorimeters, get_transport,
    CommunicationError, DeviceNotFoundError
)
from calibrate_pro.hardware.colorimeter_base import (
    ColorimeterBase, ColorMeasurement, DeviceInfo, DeviceType,
    CalibrationMode, CalibrationPatch
)


# i1Display command codes
class I1Command:
    """i1Display Pro command codes."""
    GET_STATUS = 0x00
    SET_LED = 0x01
    GET_CALIBRATION = 0x02
    SET_INTEGRATION = 0x03
    MEASURE = 0x04
    GET_AMBIENT = 0x05
    DARK_CALIBRATION = 0x06
    GET_DIFFUSER = 0x07
    GET_REFRESH_RATE = 0x08
    SET_REFRESH_MODE = 0x09
    GET_VERSION = 0x0A
    GET_SERIAL = 0x0B
    UNLOCK = 0x0C
    GET_CAL_DATA = 0x0D


# i1Display Pro sensor calibration matrices
# These are device-specific and should be read from EEPROM
# Default matrices for typical devices
I1DISPLAY_DEFAULT_MATRIX = np.array([
    [0.4124564, 0.3575761, 0.1804375],
    [0.2126729, 0.7151522, 0.0721750],
    [0.0193339, 0.1191920, 0.9503041]
])

# Spectral sensitivities for i1Display Pro (approximate CIE 1931 response)
I1DISPLAY_SPECTRAL_RESPONSE = {
    "red": {"peak": 610, "width": 50},
    "green": {"peak": 545, "width": 60},
    "blue": {"peak": 460, "width": 40}
}


@dataclass
class I1CalibrationData:
    """Calibration data read from device EEPROM."""
    serial: str
    firmware_version: str
    calibration_matrix: np.ndarray
    dark_offsets: np.ndarray
    integration_scale: float
    ambient_matrix: Optional[np.ndarray] = None
    manufacture_date: str = ""


class I1DisplayNative(ColorimeterBase):
    """
    Native USB driver for i1Display Pro/Plus.

    Communicates directly with the device via HID protocol.
    """

    # Supported device IDs
    SUPPORTED_DEVICES = {
        (0x0765, 0x5001): ("i1Display Pro", True, True),      # (name, has_ambient, has_edr)
        (0x0765, 0x5011): ("i1Display Pro Plus", True, True),
        (0x0765, 0x5020): ("i1Display Studio", True, False),
        (0x0765, 0x5010): ("ColorMunki Display", True, False),
        (0x0765, 0x5021): ("ColorChecker Display", True, False),
        (0x0765, 0x5022): ("ColorChecker Display Pro", True, True),
        (0x0765, 0x5023): ("ColorChecker Display Plus", True, True),
    }

    def __init__(self):
        super().__init__()
        self._transport: Optional[USBTransport] = None
        self._usb_info: Optional[USBDeviceInfo] = None
        self._cal_data: Optional[I1CalibrationData] = None
        self._integration_time: float = 0.5  # seconds
        self._averaging: int = 1
        self._refresh_mode: bool = False
        self._refresh_rate: float = 60.0

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect connected i1Display devices."""
        devices = []

        for usb_dev in enumerate_all_colorimeters():
            key = (usb_dev.vendor_id, usb_dev.product_id)
            if key in self.SUPPORTED_DEVICES:
                name, has_ambient, has_edr = self.SUPPORTED_DEVICES[key]
                caps = ["spot", "emission"]
                if has_ambient:
                    caps.append("ambient")
                if has_edr:
                    caps.append("edr")

                devices.append(DeviceInfo(
                    name=name,
                    manufacturer="X-Rite",
                    model=name,
                    serial=usb_dev.serial_number or "Unknown",
                    device_type=DeviceType.COLORIMETER,
                    firmware_version="",
                    capabilities=caps
                ))

        return devices

    def connect(self, device_index: int = 0) -> bool:
        """Connect to i1Display device."""
        devices = []
        for usb_dev in enumerate_all_colorimeters():
            key = (usb_dev.vendor_id, usb_dev.product_id)
            if key in self.SUPPORTED_DEVICES:
                devices.append(usb_dev)

        if device_index >= len(devices):
            return False

        self._usb_info = devices[device_index]

        try:
            self._transport = get_transport(self._usb_info)
            self._transport.open(self._usb_info)
            self.is_connected = True

            # Read device info
            self._read_device_info()

            # Read calibration data from EEPROM
            self._read_calibration_data()

            return True
        except Exception as e:
            self._report_progress(f"Connection failed: {e}", 0)
            return False

    def disconnect(self) -> bool:
        """Disconnect from device."""
        if self._transport:
            self._transport.close()
            self._transport = None
        self.is_connected = False
        return True

    def _send_command(self, cmd: int, data: bytes = b'') -> bytes:
        """Send command and receive response."""
        if not self._transport or not self._transport.is_open:
            raise CommunicationError("Device not connected")

        # Build command packet
        # Format: [Report ID (0x00)] [Command] [Length] [Data...]
        packet = bytes([0x00, cmd, len(data)]) + data
        packet = packet.ljust(64, b'\x00')

        self._transport.write(packet)
        time.sleep(0.01)  # Small delay for device processing

        response = self._transport.read(64, timeout=5000)
        return response

    def _read_device_info(self):
        """Read device information."""
        try:
            # Get version
            resp = self._send_command(I1Command.GET_VERSION)
            if len(resp) >= 8:
                major = resp[2]
                minor = resp[3]
                self.device_info = DeviceInfo(
                    name=self.SUPPORTED_DEVICES.get(
                        (self._usb_info.vendor_id, self._usb_info.product_id),
                        ("i1Display", True, False)
                    )[0],
                    manufacturer="X-Rite",
                    model=self._usb_info.product,
                    serial=self._usb_info.serial_number or "",
                    device_type=DeviceType.COLORIMETER,
                    firmware_version=f"{major}.{minor}",
                    capabilities=["spot", "ambient", "emission"]
                )

            # Get serial
            resp = self._send_command(I1Command.GET_SERIAL)
            if len(resp) >= 16:
                serial = resp[2:18].decode('ascii', errors='ignore').strip('\x00')
                if serial and self.device_info:
                    self.device_info.serial = serial

        except Exception:
            # Use USB info as fallback
            if self._usb_info:
                self.device_info = DeviceInfo(
                    name=self._usb_info.product,
                    manufacturer=self._usb_info.manufacturer,
                    model=self._usb_info.product,
                    serial=self._usb_info.serial_number or "",
                    device_type=DeviceType.COLORIMETER,
                    capabilities=["spot", "emission"]
                )

    def _read_calibration_data(self):
        """Read calibration data from device EEPROM."""
        try:
            resp = self._send_command(I1Command.GET_CAL_DATA)
            if len(resp) >= 40:
                # Parse calibration matrix (3x3 float32)
                matrix = np.zeros((3, 3))
                for i in range(3):
                    for j in range(3):
                        idx = 4 + (i * 3 + j) * 4
                        if idx + 4 <= len(resp):
                            matrix[i, j] = struct.unpack('<f', resp[idx:idx+4])[0]

                # Parse dark offsets
                dark = np.zeros(3)
                for i in range(3):
                    idx = 40 + i * 4
                    if idx + 4 <= len(resp):
                        dark[i] = struct.unpack('<f', resp[idx:idx+4])[0]

                self._cal_data = I1CalibrationData(
                    serial=self.device_info.serial if self.device_info else "",
                    firmware_version=self.device_info.firmware_version if self.device_info else "",
                    calibration_matrix=matrix if np.any(matrix) else I1DISPLAY_DEFAULT_MATRIX,
                    dark_offsets=dark,
                    integration_scale=1.0
                )
            else:
                # Use default calibration
                self._cal_data = I1CalibrationData(
                    serial="",
                    firmware_version="",
                    calibration_matrix=I1DISPLAY_DEFAULT_MATRIX,
                    dark_offsets=np.zeros(3),
                    integration_scale=1.0
                )
        except Exception:
            # Use default calibration
            self._cal_data = I1CalibrationData(
                serial="",
                firmware_version="",
                calibration_matrix=I1DISPLAY_DEFAULT_MATRIX,
                dark_offsets=np.zeros(3),
                integration_scale=1.0
            )

    def calibrate_device(self) -> bool:
        """
        Perform dark calibration.

        Should be done with lens cap on or device face-down.
        """
        self._report_progress("Performing dark calibration...", 0)
        self._report_progress("Cover the sensor or place device face-down", 0.1)

        try:
            # Send dark calibration command
            resp = self._send_command(I1Command.DARK_CALIBRATION)

            # Wait for calibration
            time.sleep(2.0)

            # Read dark offsets
            resp = self._send_command(I1Command.GET_CALIBRATION)
            if len(resp) >= 16 and self._cal_data:
                for i in range(3):
                    idx = 4 + i * 4
                    if idx + 4 <= len(resp):
                        self._cal_data.dark_offsets[i] = struct.unpack('<f', resp[idx:idx+4])[0]

            self._report_progress("Dark calibration complete", 1.0)
            return True
        except Exception as e:
            self._report_progress(f"Calibration failed: {e}", 0)
            return False

    def set_integration_time(self, seconds: float) -> bool:
        """Set measurement integration time."""
        if 0.01 <= seconds <= 10.0:
            self._integration_time = seconds

            # Send to device (time in milliseconds)
            ms = int(seconds * 1000)
            data = struct.pack('<H', ms)
            try:
                self._send_command(I1Command.SET_INTEGRATION, data)
                return True
            except Exception:
                pass
        return False

    def set_refresh_mode(self, refresh_rate: float) -> bool:
        """
        Set refresh display mode for PWM/refresh sync.

        Args:
            refresh_rate: Display refresh rate in Hz
        """
        self._refresh_mode = True
        self._refresh_rate = refresh_rate

        try:
            # Send refresh rate to device
            data = struct.pack('<H', int(refresh_rate))
            self._send_command(I1Command.SET_REFRESH_MODE, data)
            return True
        except Exception:
            return False

    def measure_spot(self) -> Optional[ColorMeasurement]:
        """Take a spot measurement."""
        if not self.is_connected:
            return None

        try:
            # Set integration time
            ms = int(self._integration_time * 1000)
            int_data = struct.pack('<H', ms)
            self._send_command(I1Command.SET_INTEGRATION, int_data)

            # Accumulate readings for averaging
            rgb_sum = np.zeros(3)

            for _ in range(self._averaging):
                # Trigger measurement
                resp = self._send_command(I1Command.MEASURE)

                # Wait for measurement
                time.sleep(self._integration_time + 0.1)

                # Read result
                if len(resp) >= 16:
                    # Parse RGB sensor values (3 x uint32)
                    rgb = np.zeros(3)
                    for i in range(3):
                        idx = 4 + i * 4
                        if idx + 4 <= len(resp):
                            rgb[i] = struct.unpack('<I', resp[idx:idx+4])[0]
                    rgb_sum += rgb

            # Average readings
            rgb_avg = rgb_sum / self._averaging

            # Apply dark offset correction
            if self._cal_data:
                rgb_avg = rgb_avg - self._cal_data.dark_offsets
                rgb_avg = np.maximum(rgb_avg, 0)

            # Convert to XYZ using calibration matrix
            xyz = self._rgb_to_xyz(rgb_avg)

            return ColorMeasurement(
                X=float(xyz[0]),
                Y=float(xyz[1]),
                Z=float(xyz[2]),
                integration_time=self._integration_time,
                measurement_mode="spot"
            )

        except Exception as e:
            self._report_progress(f"Measurement failed: {e}", 0)
            return None

    def _rgb_to_xyz(self, rgb: np.ndarray) -> np.ndarray:
        """Convert raw RGB sensor values to XYZ."""
        if self._cal_data and np.any(self._cal_data.calibration_matrix):
            matrix = self._cal_data.calibration_matrix
        else:
            matrix = I1DISPLAY_DEFAULT_MATRIX

        # Normalize and apply matrix
        # The sensor values need scaling based on integration time
        scale = 1.0 / (self._integration_time * 1000)  # Normalize to 1 second
        rgb_norm = rgb * scale

        xyz = matrix @ rgb_norm

        # Scale to standard Y=100 range (typically nits)
        # This calibration factor depends on the specific device
        xyz = xyz * 0.01

        return xyz

    def measure_ambient(self) -> Optional[ColorMeasurement]:
        """Measure ambient light (requires diffuser)."""
        if not self.is_connected:
            return None

        try:
            # Check diffuser position
            resp = self._send_command(I1Command.GET_DIFFUSER)
            diffuser_on = resp[2] if len(resp) > 2 else 0

            if not diffuser_on:
                self._report_progress("Attach diffuser for ambient measurement", 0)

            # Trigger ambient measurement
            resp = self._send_command(I1Command.GET_AMBIENT)
            time.sleep(1.0)

            if len(resp) >= 8:
                # Parse illuminance (lux)
                lux = struct.unpack('<f', resp[4:8])[0] if len(resp) >= 8 else 0

                return ColorMeasurement(
                    X=0,
                    Y=lux,  # Y represents illuminance for ambient
                    Z=0,
                    measurement_mode="ambient"
                )

        except Exception:
            pass

        return None

    def detect_refresh_rate(self) -> Optional[float]:
        """Detect display refresh rate."""
        if not self.is_connected:
            return None

        try:
            resp = self._send_command(I1Command.GET_REFRESH_RATE)
            if len(resp) >= 6:
                # Parse refresh rate (Hz)
                rate = struct.unpack('<H', resp[4:6])[0]
                return float(rate)
        except Exception:
            pass

        return None

    def set_led(self, color: str = "off"):
        """Control device LED indicator."""
        colors = {"off": 0, "green": 1, "red": 2, "blue": 3}
        code = colors.get(color.lower(), 0)
        try:
            self._send_command(I1Command.SET_LED, bytes([code]))
        except Exception:
            pass


def detect_i1display_native() -> Optional[I1DisplayNative]:
    """Detect and connect to i1Display device."""
    driver = I1DisplayNative()
    devices = driver.detect_devices()

    if devices:
        if driver.connect(0):
            return driver

    return None
