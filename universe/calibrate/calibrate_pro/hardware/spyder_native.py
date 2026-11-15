"""
Native Spyder Driver

Direct USB communication with Datacolor Spyder colorimeters.
No ArgyllCMS dependency.

Supported devices:
- SpyderX (085C:0600)
- SpyderX2 (085C:0700)
- Spyder5 (085C:0500)
- Spyder4 (085C:0400)
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


# Spyder command codes
class SpyderCommand:
    """Spyder device command codes."""
    RESET = 0x00
    GET_STATUS = 0x01
    GET_SERIAL = 0x02
    GET_VERSION = 0x03
    SET_LED = 0x04
    SET_INTEGRATION = 0x05
    MEASURE = 0x06
    DARK_CAL = 0x07
    GET_CAL_DATA = 0x08
    GET_AMBIENT = 0x09
    SET_REFRESH = 0x0A
    GET_REFRESH = 0x0B
    MEASURE_CONT = 0x0C


# SpyderX default calibration matrix
# Maps sensor RGB to CIE XYZ
SPYDER_X_MATRIX = np.array([
    [0.4361, 0.3851, 0.1431],
    [0.2225, 0.7169, 0.0606],
    [0.0139, 0.0971, 0.7141]
])

# SpyderX2 has improved sensors
SPYDER_X2_MATRIX = np.array([
    [0.4243, 0.3836, 0.1570],
    [0.2126, 0.7152, 0.0722],
    [0.0193, 0.1192, 0.9503]
])

# Spyder5 matrix
SPYDER_5_MATRIX = np.array([
    [0.4124, 0.3576, 0.1805],
    [0.2126, 0.7152, 0.0722],
    [0.0193, 0.1192, 0.9505]
])


@dataclass
class SpyderCalibrationData:
    """Spyder calibration data from EEPROM."""
    serial: str
    model: str
    firmware_version: str
    calibration_matrix: np.ndarray
    dark_offsets: np.ndarray
    sensor_sensitivities: np.ndarray
    manufacture_date: str = ""


class SpyderNative(ColorimeterBase):
    """
    Native USB driver for Datacolor Spyder colorimeters.

    Supports SpyderX, SpyderX2, and Spyder5 series.
    """

    # Supported device IDs
    SUPPORTED_DEVICES = {
        (0x085C, 0x0700): ("SpyderX2 Ultra", 6, True),   # (name, sensors, has_ambient)
        (0x085C, 0x0600): ("SpyderX Elite", 3, True),
        (0x085C, 0x0500): ("Spyder5 Elite", 7, True),
        (0x085C, 0x0400): ("Spyder4 Elite", 7, True),
        (0x085C, 0x0300): ("Spyder3", 7, False),
    }

    def __init__(self):
        super().__init__()
        self._transport: Optional[USBTransport] = None
        self._usb_info: Optional[USBDeviceInfo] = None
        self._cal_data: Optional[SpyderCalibrationData] = None
        self._integration_time: float = 1.0
        self._averaging: int = 1
        self._sensor_count: int = 3
        self._default_matrix: np.ndarray = SPYDER_X_MATRIX

    def detect_devices(self) -> List[DeviceInfo]:
        """Detect connected Spyder devices."""
        devices = []

        for usb_dev in enumerate_all_colorimeters():
            key = (usb_dev.vendor_id, usb_dev.product_id)
            if key in self.SUPPORTED_DEVICES:
                name, sensors, has_ambient = self.SUPPORTED_DEVICES[key]
                caps = ["spot", "emission"]
                if has_ambient:
                    caps.append("ambient")

                devices.append(DeviceInfo(
                    name=name,
                    manufacturer="Datacolor",
                    model=name,
                    serial=usb_dev.serial_number or "Unknown",
                    device_type=DeviceType.COLORIMETER,
                    firmware_version="",
                    capabilities=caps
                ))

        return devices

    def connect(self, device_index: int = 0) -> bool:
        """Connect to Spyder device."""
        devices = []
        for usb_dev in enumerate_all_colorimeters():
            key = (usb_dev.vendor_id, usb_dev.product_id)
            if key in self.SUPPORTED_DEVICES:
                devices.append(usb_dev)

        if device_index >= len(devices):
            return False

        self._usb_info = devices[device_index]
        key = (self._usb_info.vendor_id, self._usb_info.product_id)
        device_info = self.SUPPORTED_DEVICES.get(key, ("Spyder", 3, True))
        self._sensor_count = device_info[1]

        # Select appropriate calibration matrix
        if "X2" in device_info[0]:
            self._default_matrix = SPYDER_X2_MATRIX
        elif "X" in device_info[0]:
            self._default_matrix = SPYDER_X_MATRIX
        else:
            self._default_matrix = SPYDER_5_MATRIX

        try:
            self._transport = get_transport(self._usb_info)
            self._transport.open(self._usb_info)
            self.is_connected = True

            # Initialize device
            self._init_device()

            # Read calibration data
            self._read_calibration_data()

            return True
        except Exception as e:
            self._report_progress(f"Connection failed: {e}", 0)
            return False

    def disconnect(self) -> bool:
        """Disconnect from device."""
        if self._transport:
            # Turn off LED
            try:
                self._set_led(0)
            except Exception:
                pass
            self._transport.close()
            self._transport = None
        self.is_connected = False
        return True

    def _send_command(self, cmd: int, data: bytes = b'') -> bytes:
        """Send command and receive response."""
        if not self._transport or not self._transport.is_open:
            raise CommunicationError("Device not connected")

        # Spyder command format: [0x00] [cmd] [len] [data...]
        packet = bytes([0x00, cmd, len(data)]) + data
        packet = packet.ljust(64, b'\x00')

        self._transport.write(packet)
        time.sleep(0.02)

        response = self._transport.read(64, timeout=5000)
        return response

    def _init_device(self):
        """Initialize device after connection."""
        # Reset device state
        try:
            self._send_command(SpyderCommand.RESET)
            time.sleep(0.2)
        except Exception:
            pass

        # Read device info
        self._read_device_info()

        # Turn on green LED to indicate connected
        self._set_led(1)

    def _read_device_info(self):
        """Read device information."""
        try:
            # Get version
            resp = self._send_command(SpyderCommand.GET_VERSION)
            version = ""
            if len(resp) >= 6:
                major = resp[3]
                minor = resp[4]
                version = f"{major}.{minor}"

            # Get serial
            resp = self._send_command(SpyderCommand.GET_SERIAL)
            serial = ""
            if len(resp) >= 16:
                serial = resp[3:19].decode('ascii', errors='ignore').strip('\x00')

            key = (self._usb_info.vendor_id, self._usb_info.product_id)
            name = self.SUPPORTED_DEVICES.get(key, ("Spyder", 3, True))[0]

            self.device_info = DeviceInfo(
                name=name,
                manufacturer="Datacolor",
                model=name,
                serial=serial or self._usb_info.serial_number or "",
                device_type=DeviceType.COLORIMETER,
                firmware_version=version,
                capabilities=["spot", "ambient", "emission"]
            )

        except Exception:
            if self._usb_info:
                self.device_info = DeviceInfo(
                    name=self._usb_info.product,
                    manufacturer="Datacolor",
                    model=self._usb_info.product,
                    serial=self._usb_info.serial_number or "",
                    device_type=DeviceType.COLORIMETER,
                    capabilities=["spot", "emission"]
                )

    def _read_calibration_data(self):
        """Read calibration data from device."""
        try:
            resp = self._send_command(SpyderCommand.GET_CAL_DATA)

            if len(resp) >= 40:
                # Parse calibration matrix
                matrix = np.zeros((3, 3))
                for i in range(3):
                    for j in range(3):
                        idx = 4 + (i * 3 + j) * 4
                        if idx + 4 <= len(resp):
                            matrix[i, j] = struct.unpack('<f', resp[idx:idx+4])[0]

                # Parse dark offsets
                dark = np.zeros(self._sensor_count)
                for i in range(min(3, self._sensor_count)):
                    idx = 40 + i * 4
                    if idx + 4 <= len(resp):
                        dark[i] = struct.unpack('<f', resp[idx:idx+4])[0]

                self._cal_data = SpyderCalibrationData(
                    serial=self.device_info.serial if self.device_info else "",
                    model=self.device_info.name if self.device_info else "",
                    firmware_version=self.device_info.firmware_version if self.device_info else "",
                    calibration_matrix=matrix if np.any(matrix) else self._default_matrix,
                    dark_offsets=dark,
                    sensor_sensitivities=np.ones(self._sensor_count)
                )
            else:
                self._use_default_calibration()

        except Exception:
            self._use_default_calibration()

    def _use_default_calibration(self):
        """Use default calibration data."""
        self._cal_data = SpyderCalibrationData(
            serial="",
            model="",
            firmware_version="",
            calibration_matrix=self._default_matrix,
            dark_offsets=np.zeros(self._sensor_count),
            sensor_sensitivities=np.ones(self._sensor_count)
        )

    def _set_led(self, state: int):
        """Set LED state (0=off, 1=green, 2=red, 3=blue)."""
        try:
            self._send_command(SpyderCommand.SET_LED, bytes([state]))
        except Exception:
            pass

    def calibrate_device(self) -> bool:
        """Perform dark calibration."""
        self._report_progress("Performing dark calibration...", 0)
        self._report_progress("Place device face-down on a dark surface", 0.1)

        # Flash LED to indicate calibration mode
        self._set_led(2)  # Red

        try:
            time.sleep(1.5)

            # Send dark calibration command
            resp = self._send_command(SpyderCommand.DARK_CAL)
            time.sleep(2.0)

            # Read updated dark offsets
            resp = self._send_command(SpyderCommand.GET_CAL_DATA)
            if len(resp) >= 52 and self._cal_data:
                for i in range(min(3, self._sensor_count)):
                    idx = 40 + i * 4
                    if idx + 4 <= len(resp):
                        self._cal_data.dark_offsets[i] = struct.unpack('<f', resp[idx:idx+4])[0]

            self._set_led(1)  # Green
            self._report_progress("Dark calibration complete", 1.0)
            return True

        except Exception as e:
            self._set_led(2)  # Red for error
            self._report_progress(f"Calibration failed: {e}", 0)
            return False

    def set_integration_time(self, seconds: float) -> bool:
        """Set measurement integration time."""
        if 0.1 <= seconds <= 5.0:
            self._integration_time = seconds

            ms = int(seconds * 1000)
            data = struct.pack('<H', ms)
            try:
                self._send_command(SpyderCommand.SET_INTEGRATION, data)
                return True
            except Exception:
                pass
        return False

    def measure_spot(self) -> Optional[ColorMeasurement]:
        """Take a spot measurement."""
        if not self.is_connected:
            return None

        # Flash LED during measurement
        self._set_led(3)  # Blue

        try:
            # Set integration time
            ms = int(self._integration_time * 1000)
            int_data = struct.pack('<H', ms)
            self._send_command(SpyderCommand.SET_INTEGRATION, int_data)

            # Accumulate for averaging
            sensor_sum = np.zeros(self._sensor_count)

            for _ in range(self._averaging):
                # Trigger measurement
                resp = self._send_command(SpyderCommand.MEASURE)

                # Wait for integration
                time.sleep(self._integration_time + 0.2)

                # Parse sensor values
                if len(resp) >= 4 + self._sensor_count * 4:
                    for i in range(min(3, self._sensor_count)):
                        idx = 4 + i * 4
                        if idx + 4 <= len(resp):
                            sensor_sum[i] += struct.unpack('<I', resp[idx:idx+4])[0]

            # Average
            sensor_avg = sensor_sum / self._averaging

            # Apply dark offset correction
            if self._cal_data:
                sensor_avg[:3] = sensor_avg[:3] - self._cal_data.dark_offsets[:3]
                sensor_avg = np.maximum(sensor_avg, 0)

            # Convert to XYZ
            xyz = self._sensor_to_xyz(sensor_avg[:3])

            self._set_led(1)  # Back to green

            return ColorMeasurement(
                X=float(xyz[0]),
                Y=float(xyz[1]),
                Z=float(xyz[2]),
                integration_time=self._integration_time,
                measurement_mode="spot"
            )

        except Exception as e:
            self._set_led(2)  # Red for error
            self._report_progress(f"Measurement failed: {e}", 0)
            return None

    def _sensor_to_xyz(self, sensor: np.ndarray) -> np.ndarray:
        """Convert raw sensor values to XYZ."""
        if self._cal_data and np.any(self._cal_data.calibration_matrix):
            matrix = self._cal_data.calibration_matrix
        else:
            matrix = self._default_matrix

        # Normalize by integration time
        scale = 1.0 / (self._integration_time * 1000)
        sensor_norm = sensor * scale

        # Apply calibration matrix
        xyz = matrix @ sensor_norm

        # Scale to standard range (cd/m²)
        xyz = xyz * 0.01

        return xyz

    def measure_ambient(self) -> Optional[ColorMeasurement]:
        """Measure ambient light."""
        if not self.is_connected:
            return None

        try:
            resp = self._send_command(SpyderCommand.GET_AMBIENT)
            time.sleep(1.0)

            if len(resp) >= 8:
                lux = struct.unpack('<f', resp[4:8])[0]
                return ColorMeasurement(
                    X=0,
                    Y=lux,
                    Z=0,
                    measurement_mode="ambient"
                )
        except Exception:
            pass

        return None

    def set_display_type(self, display_type: str) -> bool:
        """
        Set display type for optimized measurement.

        Args:
            display_type: "LCD", "OLED", "CRT", etc.
        """
        # Adjust integration and averaging for display type
        if display_type.upper() == "OLED":
            self._integration_time = 2.0
            self._averaging = 3
        elif display_type.upper() == "LCD":
            self._integration_time = 1.0
            self._averaging = 1
        return True


class SpyderX(SpyderNative):
    """Convenience class for SpyderX devices."""

    def detect_devices(self) -> List[DeviceInfo]:
        all_devices = super().detect_devices()
        return [d for d in all_devices if "SpyderX" in d.name and "X2" not in d.name]


class SpyderX2(SpyderNative):
    """Convenience class for SpyderX2 devices."""

    def detect_devices(self) -> List[DeviceInfo]:
        all_devices = super().detect_devices()
        return [d for d in all_devices if "SpyderX2" in d.name]


def detect_spyder_native() -> Optional[SpyderNative]:
    """Detect and connect to Spyder device."""
    driver = SpyderNative()
    devices = driver.detect_devices()

    if devices:
        if driver.connect(0):
            return driver

    return None
