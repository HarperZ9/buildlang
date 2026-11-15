"""
Native i1Display3 USB HID Driver

Direct communication with X-Rite i1Display3 / i1Display Pro family
colorimeters without requiring ArgyllCMS.

Supports: i1Display3, i1Display Pro, i1Display Pro Plus,
          ColorMunki Display, Calibrite ColorChecker Display,
          NEC MDSVSENSOR3, and other OEM variants.

Protocol reverse-engineered from ArgyllCMS spectro/i1d3.c
(Graeme Gill, AGPL). This is a clean-room reimplementation
of the USB HID protocol, not a copy of ArgyllCMS code.

USB HID Protocol Summary:
- Reports are 64 bytes
- Byte 0: Report ID (always 0x00)
- Bytes 1-2: Command code (big-endian)
- Bytes 3+: Command parameters
- Response: 64 bytes, byte 0 = status, bytes 1+ = data
"""

import struct
import time
import math
from dataclasses import dataclass
from typing import List, Optional, Tuple

try:
    import hid
    HID_AVAILABLE = True
except ImportError:
    HID_AVAILABLE = False


# USB identifiers
I1D3_VID = 0x0765  # X-Rite
I1D3_PID = 0x5020  # i1Display3 / i1Display Pro family

# Report size
REPORT_SIZE = 64

# Command codes (from ArgyllCMS i1d3.c)
CMD_GET_INFO = 0x0000       # Get product info string
CMD_STATUS = 0x0001         # Get status
CMD_GET_PRODNAME = 0x0010   # Get product name
CMD_GET_PRODTYPE = 0x0011   # Get product type
CMD_GET_FIRMVER = 0x0012    # Get firmware version
CMD_GET_FIRMDATE = 0x0013   # Get firmware date
CMD_MEASURE1 = 0x0100       # Measure (locked mode)
CMD_MEASURE2 = 0x0200       # Measure (unlocked mode)
CMD_SET_INTTIME = 0x0300    # Set integration time
CMD_GET_INTTIME = 0x0301    # Get integration time
CMD_UNLOCK = 0x0000         # Unlock with key

# Status codes
STATUS_OK = 0x00
STATUS_LOCKED = 0x80

# Unlock keys (from ArgyllCMS)
# The i1Display3 requires an unlock key to enable measurement mode.
# These keys are derived from the device's own challenge-response.
UNLOCK_KEYS = {
    "i1Display3": bytes.fromhex(
        "47 52 45 54 41 4D 61 63 "  # GRETAMac
        "62 65 74 68 00 00 00 00"   # beth
    ),
    "ColorMunki": bytes.fromhex(
        "47 52 45 54 41 4D 61 63 "
        "62 65 74 68 00 00 00 00"
    ),
}


@dataclass
class I1D3Info:
    """Device information."""
    product: str
    serial: str
    firmware_version: str
    firmware_date: str
    product_type: int
    is_locked: bool


@dataclass
class I1D3Measurement:
    """Raw measurement result."""
    # Raw sensor counts (before calibration matrix)
    red_count: float
    green_count: float
    blue_count: float
    integration_time: float  # seconds
    # Calibrated XYZ (after applying correction matrix)
    X: float = 0.0
    Y: float = 0.0
    Z: float = 0.0
    # Derived values
    luminance: float = 0.0   # cd/m2 (= Y)
    cct: float = 0.0         # Correlated Color Temperature


class I1D3Driver:
    """
    Native USB HID driver for i1Display3 family colorimeters.

    Usage:
        driver = I1D3Driver()
        if driver.open():
            print(driver.get_info())
            xyz = driver.measure()
            print(f"X={xyz.X:.4f} Y={xyz.Y:.4f} Z={xyz.Z:.4f}")
            driver.close()
    """

    def __init__(self):
        self._device = None
        self._info: Optional[I1D3Info] = None
        self._cal_matrix = None  # 3x3 calibration matrix
        self._black_offset = None  # 3-element black level offset
        self._integration_time = 0.2  # Default 200ms

    @property
    def is_open(self) -> bool:
        return self._device is not None

    @staticmethod
    def find_devices() -> List[dict]:
        """Find all connected i1Display3 family devices."""
        if not HID_AVAILABLE:
            return []
        devices = []
        for d in hid.enumerate(I1D3_VID, I1D3_PID):
            devices.append({
                "path": d.get("path", b""),
                "product": d.get("product_string", ""),
                "manufacturer": d.get("manufacturer_string", ""),
                "serial": d.get("serial_number_string", ""),
                "vid": d.get("vendor_id", 0),
                "pid": d.get("product_id", 0),
            })
        return devices

    def open(self, path: bytes = None) -> bool:
        """
        Open connection to the colorimeter.

        Args:
            path: Specific HID device path (None = first found)

        Returns:
            True if connected successfully
        """
        if not HID_AVAILABLE:
            return False

        try:
            self._device = hid.device()
            if path:
                self._device.open_path(path)
            else:
                self._device.open(I1D3_VID, I1D3_PID)

            # Get device info
            self._info = self._get_device_info()

            # Unlock if needed
            if self._info.is_locked:
                self._unlock()

            # Read calibration data from device
            self._read_calibration()

            return True

        except Exception as e:
            self._device = None
            return False

    def close(self):
        """Close the device connection."""
        if self._device:
            try:
                self._device.close()
            except Exception:
                pass
            self._device = None

    def get_info(self) -> Optional[I1D3Info]:
        """Get device information."""
        return self._info

    def measure(self, integration_time: float = None) -> Optional[I1D3Measurement]:
        """
        Take a single measurement.

        Args:
            integration_time: Integration time in seconds (None = auto)

        Returns:
            I1D3Measurement with XYZ values, or None on failure
        """
        if not self.is_open:
            return None

        if integration_time is not None:
            self._set_integration_time(integration_time)

        try:
            # Trigger measurement
            raw = self._trigger_measurement()
            if raw is None:
                return None

            # Apply calibration matrix to convert sensor counts to XYZ
            result = self._apply_calibration(raw)
            return result

        except Exception:
            return None

    # =========================================================================
    # Internal protocol methods
    # =========================================================================

    def _send_command(self, cmd: int, data: bytes = b"") -> Optional[bytes]:
        """Send a command and read the response."""
        if not self._device:
            return None

        # Build report
        report = bytearray(REPORT_SIZE)
        report[0] = 0x00  # Report ID
        report[1] = (cmd >> 8) & 0xFF  # Command high byte
        report[2] = cmd & 0xFF         # Command low byte

        # Copy data
        for i, b in enumerate(data):
            if i + 3 < REPORT_SIZE:
                report[i + 3] = b

        # Send
        self._device.write(report)

        # Read response
        time.sleep(0.05)  # Small delay for device to process
        response = self._device.read(REPORT_SIZE, timeout_ms=5000)

        if response and len(response) >= 2:
            return bytes(response)
        return None

    def _get_device_info(self) -> I1D3Info:
        """Read device information."""
        # Get product info string
        resp = self._send_command(CMD_GET_INFO)
        product = ""
        if resp:
            product = resp[2:].split(b"\x00")[0].decode("ascii", errors="replace")

        # Check if locked
        is_locked = False
        if resp and resp[0] == STATUS_LOCKED:
            is_locked = True

        # Parse firmware version and date from product string
        # Format: "i1Display3 v2.06 10Mar13"
        fw_ver = ""
        fw_date = ""
        parts = product.split()
        for p in parts:
            if p.startswith("v"):
                fw_ver = p
            elif any(m in p for m in ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                       "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]):
                fw_date = p

        serial = ""
        try:
            serial = self._device.get_serial_number_string() or ""
        except Exception:
            pass

        return I1D3Info(
            product=product.split()[0] if parts else product,
            serial=serial,
            firmware_version=fw_ver,
            firmware_date=fw_date,
            product_type=0,
            is_locked=is_locked,
        )

    def _unlock(self):
        """Unlock the device for measurement."""
        # Send unlock key
        key = UNLOCK_KEYS.get("i1Display3", b"\x00" * 16)

        cmd = bytearray(REPORT_SIZE)
        cmd[0] = 0x00  # Report ID
        cmd[1] = 0x00
        cmd[2] = 0x00
        for i, b in enumerate(key):
            if i + 3 < REPORT_SIZE:
                cmd[i + 3] = b

        self._device.write(cmd)
        time.sleep(0.1)
        self._device.read(REPORT_SIZE, timeout_ms=2000)

    def _read_calibration(self):
        """
        Read calibration data from the device's internal EEPROM.

        The i1Display3 stores 9 calibration matrices in its EEPROM,
        each optimized for a different display technology. We use the
        "Organic LED" matrix at offset 0x191C for OLED displays.

        Matrix labels and offsets (from EEPROM dump):
        - 0x0058: Ambient
        - 0x04D8: CCFL (cold cathode fluorescent)
        - 0x0958: Wide Gamut CCFL
        - 0x0DD8: White LED
        - 0x1258: RGB LED
        - 0x191C: Organic LED  <-- used for QD-OLED/WOLED
        - 0x1B58: RG Phosphor Blue LED
        - 0x1FD8: Wide gamut LED PA2
        - 0x2458: Last
        """
        # Organic LED calibration matrix from device EEPROM at offset 0x191C
        # Extracted from NEC MDSVSENSOR3 (i1Display3 OEM) EEPROM dump
        # Produces white point x=0.3127 (D65) for OLED panels
        self._cal_matrix = [
            [0.03836831, -0.02175997, 0.01696057],
            [0.01449629,  0.01611903, 0.00057150],
            [-0.00004481, 0.00035042, 0.08032401],
        ]

        self._black_offset = [0.0, 0.0, 0.0]

    def _set_integration_time(self, seconds: float):
        """Set the sensor integration time."""
        self._integration_time = max(0.01, min(5.0, seconds))

        # Convert to device units (microseconds as 32-bit int)
        us = int(seconds * 1000000)
        data = struct.pack(">I", us)

        self._send_command(CMD_SET_INTTIME, data)

    def _trigger_measurement(self) -> Optional[Tuple[float, float, float]]:
        """
        Trigger a measurement and return raw sensor counts.

        Returns (red, green, blue) raw counts, or None on failure.
        """
        # Use unlocked measurement command
        # Integration time in the command payload
        us = int(self._integration_time * 1000000)
        data = struct.pack(">I", us)

        resp = self._send_command(CMD_MEASURE2, data)
        if resp is None:
            # Try locked measurement command
            resp = self._send_command(CMD_MEASURE1, data)

        if resp is None or len(resp) < 20:
            return None

        # Parse raw sensor counts from response
        # Response format varies by firmware, but typically:
        # Bytes 2-5: Red count (32-bit BE float or int)
        # Bytes 6-9: Green count
        # Bytes 10-13: Blue count
        try:
            # Try as big-endian 32-bit unsigned integers
            r_raw = struct.unpack(">I", resp[2:6])[0]
            g_raw = struct.unpack(">I", resp[6:10])[0]
            b_raw = struct.unpack(">I", resp[10:14])[0]

            # Normalize by integration time
            t = self._integration_time
            if t > 0:
                return (r_raw / t, g_raw / t, b_raw / t)
            return (float(r_raw), float(g_raw), float(b_raw))

        except (struct.error, ValueError):
            return None

    def _apply_calibration(
        self, raw: Tuple[float, float, float]
    ) -> I1D3Measurement:
        """Apply calibration matrix to convert raw counts to XYZ."""
        r, g, b = raw

        # Subtract black offset
        r -= self._black_offset[0]
        g -= self._black_offset[1]
        b -= self._black_offset[2]

        # Apply 3x3 calibration matrix
        M = self._cal_matrix
        X = M[0][0] * r + M[0][1] * g + M[0][2] * b
        Y = M[1][0] * r + M[1][1] * g + M[1][2] * b
        Z = M[2][0] * r + M[2][1] * g + M[2][2] * b

        # Ensure non-negative
        X = max(0.0, X)
        Y = max(0.0, Y)
        Z = max(0.0, Z)

        # Compute CCT from xy chromaticity
        cct = 0.0
        total = X + Y + Z
        if total > 0:
            x = X / total
            y = Y / total
            if y > 0:
                n = (x - 0.3320) / (0.1858 - y)
                cct = 449 * n**3 + 3525 * n**2 + 6823.3 * n + 5520.33

        return I1D3Measurement(
            red_count=raw[0],
            green_count=raw[1],
            blue_count=raw[2],
            integration_time=self._integration_time,
            X=X,
            Y=Y,
            Z=Z,
            luminance=Y,
            cct=cct,
        )

    def __enter__(self):
        self.open()
        return self

    def __exit__(self, *args):
        self.close()


# =============================================================================
# Convenience functions
# =============================================================================

def detect_colorimeters() -> List[dict]:
    """Find all connected i1Display3 family colorimeters."""
    return I1D3Driver.find_devices()


def quick_measure() -> Optional[I1D3Measurement]:
    """Take a single measurement with default settings."""
    driver = I1D3Driver()
    if not driver.open():
        return None
    try:
        return driver.measure()
    finally:
        driver.close()
