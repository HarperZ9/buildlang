"""
Video Card Gamma Table (VCGT) Handling

Provides high-precision VCGT management for:
- 1024-point (or higher) gamma tables
- Per-channel calibration curves
- Windows gamma ramp loading via SetDeviceGammaRamp
- LUT extraction from ICC profiles
- Smooth interpolation and curve generation

The VCGT tag in ICC profiles stores calibration curves that are
loaded into the video card's hardware LUT for real-time color correction.
"""

import numpy as np
from dataclasses import dataclass
from typing import Optional, Tuple, List, Union
from pathlib import Path
import struct
import ctypes
from ctypes import wintypes


# =============================================================================
# VCGT Constants
# =============================================================================

# Standard table sizes
VCGT_SIZE_STANDARD = 256      # Windows gamma ramp size
VCGT_SIZE_EXTENDED = 1024     # High-precision
VCGT_SIZE_MAXIMUM = 4096      # Maximum precision

# Gamma ramp limits (Windows API)
GAMMA_RAMP_SIZE = 256
GAMMA_RAMP_MAX = 65535
GAMMA_RAMP_MIN = 0


# =============================================================================
# VCGT Data Structures
# =============================================================================

@dataclass
class VCGTTable:
    """
    Video Card Gamma Table data.

    Stores per-channel calibration curves for display correction.
    """
    red: np.ndarray      # Red channel LUT [0, 1]
    green: np.ndarray    # Green channel LUT [0, 1]
    blue: np.ndarray     # Blue channel LUT [0, 1]
    size: int = 256

    def __post_init__(self):
        """Validate and normalize table data."""
        self.red = np.asarray(self.red, dtype=np.float64)
        self.green = np.asarray(self.green, dtype=np.float64)
        self.blue = np.asarray(self.blue, dtype=np.float64)

        # Ensure all channels have same size
        assert len(self.red) == len(self.green) == len(self.blue)
        self.size = len(self.red)

        # Clip to valid range
        self.red = np.clip(self.red, 0.0, 1.0)
        self.green = np.clip(self.green, 0.0, 1.0)
        self.blue = np.clip(self.blue, 0.0, 1.0)

    @classmethod
    def identity(cls, size: int = 256) -> 'VCGTTable':
        """Create identity (linear) VCGT table."""
        linear = np.linspace(0, 1, size)
        return cls(red=linear.copy(), green=linear.copy(), blue=linear.copy())

    @classmethod
    def from_gamma(cls, gamma: float, size: int = 256) -> 'VCGTTable':
        """Create VCGT from gamma value."""
        x = np.linspace(0, 1, size)
        curve = np.power(x, 1.0 / gamma)  # Inverse gamma for correction
        return cls(red=curve.copy(), green=curve.copy(), blue=curve.copy())

    @classmethod
    def from_rgb_gamma(
        cls,
        gamma_r: float,
        gamma_g: float,
        gamma_b: float,
        size: int = 256
    ) -> 'VCGTTable':
        """Create VCGT with per-channel gamma."""
        x = np.linspace(0, 1, size)
        red = np.power(x, 1.0 / gamma_r)
        green = np.power(x, 1.0 / gamma_g)
        blue = np.power(x, 1.0 / gamma_b)
        return cls(red=red, green=green, blue=blue)

    @classmethod
    def from_srgb_correction(cls, size: int = 256) -> 'VCGTTable':
        """Create VCGT for sRGB correction."""
        x = np.linspace(0, 1, size)

        # sRGB EOTF (decode)
        def srgb_eotf(v):
            return np.where(
                v <= 0.04045,
                v / 12.92,
                np.power((v + 0.055) / 1.055, 2.4)
            )

        # For correction, we apply inverse (OETF)
        def srgb_oetf(v):
            return np.where(
                v <= 0.0031308,
                v * 12.92,
                1.055 * np.power(v, 1/2.4) - 0.055
            )

        curve = srgb_oetf(x)
        return cls(red=curve.copy(), green=curve.copy(), blue=curve.copy())

    def resize(self, new_size: int) -> 'VCGTTable':
        """Resize table using linear interpolation."""
        old_x = np.linspace(0, 1, self.size)
        new_x = np.linspace(0, 1, new_size)

        red = np.interp(new_x, old_x, self.red)
        green = np.interp(new_x, old_x, self.green)
        blue = np.interp(new_x, old_x, self.blue)

        return VCGTTable(red=red, green=green, blue=blue)

    def apply_brightness(self, brightness: float) -> 'VCGTTable':
        """
        Apply brightness adjustment (0-2, 1.0 = no change).

        Args:
            brightness: Brightness multiplier

        Returns:
            New VCGTTable with adjustment applied
        """
        return VCGTTable(
            red=np.clip(self.red * brightness, 0, 1),
            green=np.clip(self.green * brightness, 0, 1),
            blue=np.clip(self.blue * brightness, 0, 1)
        )

    def apply_contrast(self, contrast: float) -> 'VCGTTable':
        """
        Apply contrast adjustment (0-2, 1.0 = no change).

        Args:
            contrast: Contrast multiplier

        Returns:
            New VCGTTable with adjustment applied
        """
        mid = 0.5
        return VCGTTable(
            red=np.clip((self.red - mid) * contrast + mid, 0, 1),
            green=np.clip((self.green - mid) * contrast + mid, 0, 1),
            blue=np.clip((self.blue - mid) * contrast + mid, 0, 1)
        )

    def apply_rgb_gain(
        self,
        r_gain: float = 1.0,
        g_gain: float = 1.0,
        b_gain: float = 1.0
    ) -> 'VCGTTable':
        """
        Apply per-channel gain adjustment.

        Args:
            r_gain: Red channel gain
            g_gain: Green channel gain
            b_gain: Blue channel gain

        Returns:
            New VCGTTable with gains applied
        """
        return VCGTTable(
            red=np.clip(self.red * r_gain, 0, 1),
            green=np.clip(self.green * g_gain, 0, 1),
            blue=np.clip(self.blue * b_gain, 0, 1)
        )

    def apply_black_point(
        self,
        black_r: float = 0.0,
        black_g: float = 0.0,
        black_b: float = 0.0
    ) -> 'VCGTTable':
        """
        Apply black point lift.

        Args:
            black_r: Red black level (0-1)
            black_g: Green black level (0-1)
            black_b: Blue black level (0-1)

        Returns:
            New VCGTTable with black point applied
        """
        return VCGTTable(
            red=black_r + self.red * (1.0 - black_r),
            green=black_g + self.green * (1.0 - black_g),
            blue=black_b + self.blue * (1.0 - black_b)
        )

    def combine(self, other: 'VCGTTable') -> 'VCGTTable':
        """
        Combine with another VCGT (apply this first, then other).

        Args:
            other: VCGT to apply after this one

        Returns:
            Combined VCGTTable
        """
        # Resize if needed
        if self.size != other.size:
            other = other.resize(self.size)

        # Apply second LUT to output of first
        x = np.linspace(0, 1, other.size)

        red = np.interp(self.red, x, other.red)
        green = np.interp(self.green, x, other.green)
        blue = np.interp(self.blue, x, other.blue)

        return VCGTTable(red=red, green=green, blue=blue)

    def invert(self) -> 'VCGTTable':
        """
        Invert the VCGT curves.

        Returns:
            Inverted VCGTTable
        """
        x = np.linspace(0, 1, self.size)

        # Sort for monotonic interpolation
        def invert_channel(y):
            # Handle non-monotonic curves
            y_sorted_idx = np.argsort(y)
            y_sorted = y[y_sorted_idx]
            x_sorted = x[y_sorted_idx]

            # Remove duplicates
            unique_mask = np.concatenate([[True], np.diff(y_sorted) > 1e-10])
            y_unique = y_sorted[unique_mask]
            x_unique = x_sorted[unique_mask]

            return np.interp(x, y_unique, x_unique)

        return VCGTTable(
            red=invert_channel(self.red),
            green=invert_channel(self.green),
            blue=invert_channel(self.blue)
        )

    def smooth(self, window_size: int = 5) -> 'VCGTTable':
        """
        Apply smoothing to curves.

        Args:
            window_size: Moving average window size

        Returns:
            Smoothed VCGTTable
        """
        from scipy.ndimage import uniform_filter1d

        return VCGTTable(
            red=uniform_filter1d(self.red, window_size, mode='nearest'),
            green=uniform_filter1d(self.green, window_size, mode='nearest'),
            blue=uniform_filter1d(self.blue, window_size, mode='nearest')
        )

    def to_uint16(self) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
        """Convert to 16-bit unsigned integer arrays."""
        return (
            (self.red * 65535).astype(np.uint16),
            (self.green * 65535).astype(np.uint16),
            (self.blue * 65535).astype(np.uint16)
        )

    def to_icc_bytes(self) -> bytes:
        """Serialize to ICC VCGT tag format."""
        data = b'vcgt' + b'\x00\x00\x00\x00'

        # Type 0 = table
        data += struct.pack('>I', 0)

        # Channels, entries, entry size
        data += struct.pack('>HHH', 3, self.size, 2)

        # Channel data (16-bit)
        r_u16, g_u16, b_u16 = self.to_uint16()

        for v in r_u16:
            data += struct.pack('>H', v)
        for v in g_u16:
            data += struct.pack('>H', v)
        for v in b_u16:
            data += struct.pack('>H', v)

        # Pad to 4-byte boundary
        while len(data) % 4 != 0:
            data += b'\x00'

        return data

    @classmethod
    def from_icc_bytes(cls, data: bytes) -> Optional['VCGTTable']:
        """Parse from ICC VCGT tag data."""
        if len(data) < 18:
            return None

        # Check signature
        if data[:4] != b'vcgt':
            return None

        # Parse header
        tag_type = struct.unpack('>I', data[8:12])[0]

        if tag_type == 0:
            # Table type
            channels = struct.unpack('>H', data[12:14])[0]
            entries = struct.unpack('>H', data[14:16])[0]
            entry_size = struct.unpack('>H', data[16:18])[0]

            if channels != 3 or entry_size != 2:
                return None

            offset = 18

            red = np.zeros(entries)
            green = np.zeros(entries)
            blue = np.zeros(entries)

            for i in range(entries):
                red[i] = struct.unpack('>H', data[offset:offset+2])[0] / 65535.0
                offset += 2

            for i in range(entries):
                green[i] = struct.unpack('>H', data[offset:offset+2])[0] / 65535.0
                offset += 2

            for i in range(entries):
                blue[i] = struct.unpack('>H', data[offset:offset+2])[0] / 65535.0
                offset += 2

            return cls(red=red, green=green, blue=blue)

        elif tag_type == 1:
            # Formula type
            gamma_r = struct.unpack('>H', data[12:14])[0] / 256.0
            min_r = struct.unpack('>H', data[14:16])[0] / 65535.0
            max_r = struct.unpack('>H', data[16:18])[0] / 65535.0

            gamma_g = struct.unpack('>H', data[18:20])[0] / 256.0
            min_g = struct.unpack('>H', data[20:22])[0] / 65535.0
            max_g = struct.unpack('>H', data[22:24])[0] / 65535.0

            gamma_b = struct.unpack('>H', data[24:26])[0] / 256.0
            min_b = struct.unpack('>H', data[26:28])[0] / 65535.0
            max_b = struct.unpack('>H', data[28:30])[0] / 65535.0

            x = np.linspace(0, 1, 256)

            red = min_r + np.power(x, gamma_r) * (max_r - min_r)
            green = min_g + np.power(x, gamma_g) * (max_g - min_g)
            blue = min_b + np.power(x, gamma_b) * (max_b - min_b)

            return cls(red=red, green=green, blue=blue)

        return None


# =============================================================================
# Windows Gamma Ramp API
# =============================================================================

class GammaRampController:
    """
    Windows gamma ramp controller using GDI32.

    Provides direct access to the video card's hardware LUT.
    """

    def __init__(self):
        """Initialize gamma ramp controller."""
        self._gdi32 = None
        self._user32 = None
        self._is_available = False

        try:
            self._gdi32 = ctypes.windll.gdi32
            self._user32 = ctypes.windll.user32
            self._is_available = True
        except Exception:
            pass

    @property
    def is_available(self) -> bool:
        """Check if gamma ramp API is available."""
        return self._is_available

    def _get_dc(self, display_id: int = 0) -> Optional[int]:
        """Get device context for display."""
        if not self._is_available:
            return None

        # Get display name
        display_name = self._get_display_name(display_id)

        if display_name:
            # Get DC for specific display
            hdc = self._user32.CreateDCW(
                display_name, display_name, None, None
            )
        else:
            # Get DC for primary display
            hdc = self._user32.GetDC(0)

        return hdc if hdc else None

    def _release_dc(self, hdc: int):
        """Release device context."""
        if hdc and self._user32:
            self._user32.ReleaseDC(0, hdc)

    def _get_display_name(self, display_id: int) -> Optional[str]:
        """Get display device name."""
        if not self._user32:
            return None

        class DISPLAY_DEVICE(ctypes.Structure):
            _fields_ = [
                ("cb", wintypes.DWORD),
                ("DeviceName", wintypes.WCHAR * 32),
                ("DeviceString", wintypes.WCHAR * 128),
                ("StateFlags", wintypes.DWORD),
                ("DeviceID", wintypes.WCHAR * 128),
                ("DeviceKey", wintypes.WCHAR * 128),
            ]

        device = DISPLAY_DEVICE()
        device.cb = ctypes.sizeof(device)

        if self._user32.EnumDisplayDevicesW(None, display_id, ctypes.byref(device), 0):
            if device.StateFlags & 0x00000001:  # DISPLAY_DEVICE_ACTIVE
                return device.DeviceName

        return None

    def get_gamma_ramp(self, display_id: int = 0) -> Optional[VCGTTable]:
        """
        Read current gamma ramp from display.

        Args:
            display_id: Display index (0 = primary)

        Returns:
            VCGTTable with current ramp or None
        """
        if not self._is_available:
            return None

        hdc = self._get_dc(display_id)
        if not hdc:
            return None

        try:
            # Gamma ramp structure: 3 * 256 * uint16
            ramp = (wintypes.WORD * 256 * 3)()

            if self._gdi32.GetDeviceGammaRamp(hdc, ctypes.byref(ramp)):
                red = np.array(ramp[0], dtype=np.float64) / 65535.0
                green = np.array(ramp[1], dtype=np.float64) / 65535.0
                blue = np.array(ramp[2], dtype=np.float64) / 65535.0

                return VCGTTable(red=red, green=green, blue=blue)

        finally:
            self._release_dc(hdc)

        return None

    def set_gamma_ramp(
        self,
        table: VCGTTable,
        display_id: int = 0
    ) -> bool:
        """
        Set gamma ramp for display.

        Args:
            table: VCGTTable to apply
            display_id: Display index

        Returns:
            True if successful
        """
        if not self._is_available:
            return False

        # Resize to 256 entries if needed
        if table.size != 256:
            table = table.resize(256)

        hdc = self._get_dc(display_id)
        if not hdc:
            return False

        try:
            # Build ramp structure
            ramp = (wintypes.WORD * 256 * 3)()

            r_u16, g_u16, b_u16 = table.to_uint16()

            for i in range(256):
                ramp[0][i] = int(r_u16[i])
                ramp[1][i] = int(g_u16[i])
                ramp[2][i] = int(b_u16[i])

            return bool(self._gdi32.SetDeviceGammaRamp(hdc, ctypes.byref(ramp)))

        finally:
            self._release_dc(hdc)

    def reset_gamma_ramp(self, display_id: int = 0) -> bool:
        """
        Reset gamma ramp to identity (linear).

        Args:
            display_id: Display index

        Returns:
            True if successful
        """
        return self.set_gamma_ramp(VCGTTable.identity(256), display_id)

    def apply_vcgt_from_profile(
        self,
        profile_path: Union[str, Path],
        display_id: int = 0
    ) -> bool:
        """
        Apply VCGT from ICC profile.

        Args:
            profile_path: Path to ICC profile
            display_id: Display index

        Returns:
            True if successful
        """
        vcgt = extract_vcgt_from_profile(profile_path)

        if vcgt:
            return self.set_gamma_ramp(vcgt, display_id)

        return False


# =============================================================================
# Profile VCGT Extraction
# =============================================================================

def extract_vcgt_from_profile(profile_path: Union[str, Path]) -> Optional[VCGTTable]:
    """
    Extract VCGT table from ICC profile.

    Args:
        profile_path: Path to ICC profile

    Returns:
        VCGTTable or None if not found
    """
    path = Path(profile_path)

    if not path.exists():
        return None

    try:
        data = path.read_bytes()

        if len(data) < 128:
            return None

        # Parse tag table
        tag_count = struct.unpack('>I', data[128:132])[0]

        for i in range(tag_count):
            offset = 132 + i * 12
            sig = data[offset:offset+4]
            tag_offset = struct.unpack('>I', data[offset+4:offset+8])[0]
            tag_size = struct.unpack('>I', data[offset+8:offset+12])[0]

            if sig == b'vcgt':
                vcgt_data = data[tag_offset:tag_offset+tag_size]
                return VCGTTable.from_icc_bytes(vcgt_data)

    except Exception:
        pass

    return None


def embed_vcgt_in_profile(
    profile_path: Union[str, Path],
    vcgt: VCGTTable,
    output_path: Optional[Union[str, Path]] = None
) -> bool:
    """
    Embed or update VCGT in ICC profile.

    Args:
        profile_path: Source profile path
        vcgt: VCGTTable to embed
        output_path: Output path (defaults to overwriting source)

    Returns:
        True if successful
    """
    # This is a simplified implementation
    # Full implementation would properly rebuild the tag table

    path = Path(profile_path)
    output = Path(output_path) if output_path else path

    try:
        data = bytearray(path.read_bytes())

        if len(data) < 128:
            return False

        # Find existing vcgt tag
        tag_count = struct.unpack('>I', data[128:132])[0]
        vcgt_index = -1

        for i in range(tag_count):
            offset = 132 + i * 12
            sig = data[offset:offset+4]

            if sig == b'vcgt':
                vcgt_index = i
                break

        vcgt_data = vcgt.to_icc_bytes()

        if vcgt_index >= 0:
            # Update existing tag (simplified - assumes same size)
            offset = 132 + vcgt_index * 12
            tag_offset = struct.unpack('>I', data[offset+4:offset+8])[0]
            old_size = struct.unpack('>I', data[offset+8:offset+12])[0]

            if len(vcgt_data) <= old_size:
                # Can replace in place
                data[tag_offset:tag_offset+len(vcgt_data)] = vcgt_data
                output.write_bytes(bytes(data))
                return True

        # Would need full profile rebuild for adding/resizing tags
        return False

    except Exception:
        return False


# =============================================================================
# Calibration Curve Generation
# =============================================================================

def generate_correction_vcgt(
    measured_trc: np.ndarray,
    target_gamma: float = 2.2,
    size: int = 256
) -> VCGTTable:
    """
    Generate correction VCGT from measured TRC.

    Args:
        measured_trc: Measured transfer function (normalized luminance)
        target_gamma: Target gamma value
        size: Output table size

    Returns:
        VCGTTable for correction
    """
    x = np.linspace(0, 1, size)

    # Target curve
    target = np.power(x, target_gamma)

    # Measured curve (resample if needed)
    if len(measured_trc) != size:
        measured_x = np.linspace(0, 1, len(measured_trc))
        measured = np.interp(x, measured_x, measured_trc)
    else:
        measured = measured_trc

    # Correction: find input that produces target output
    # For each target value, find what input gives that measured output

    # Sort measured for interpolation
    sort_idx = np.argsort(measured)
    measured_sorted = measured[sort_idx]
    x_sorted = x[sort_idx]

    # Find correction
    correction = np.interp(target, measured_sorted, x_sorted)

    return VCGTTable(red=correction, green=correction.copy(), blue=correction.copy())


def generate_rgb_correction_vcgt(
    measured_r: np.ndarray,
    measured_g: np.ndarray,
    measured_b: np.ndarray,
    target_gamma: float = 2.2,
    size: int = 256
) -> VCGTTable:
    """
    Generate per-channel correction VCGT.

    Args:
        measured_r: Measured red TRC
        measured_g: Measured green TRC
        measured_b: Measured blue TRC
        target_gamma: Target gamma
        size: Output size

    Returns:
        VCGTTable with per-channel correction
    """
    x = np.linspace(0, 1, size)
    target = np.power(x, target_gamma)

    def correct_channel(measured):
        if len(measured) != size:
            measured_x = np.linspace(0, 1, len(measured))
            measured = np.interp(x, measured_x, measured)

        sort_idx = np.argsort(measured)
        return np.interp(target, measured[sort_idx], x[sort_idx])

    return VCGTTable(
        red=correct_channel(measured_r),
        green=correct_channel(measured_g),
        blue=correct_channel(measured_b)
    )


def generate_whitepoint_vcgt(
    current_kelvin: float,
    target_kelvin: float,
    size: int = 256
) -> VCGTTable:
    """
    Generate VCGT for white point adjustment.

    Args:
        current_kelvin: Current color temperature
        target_kelvin: Target color temperature
        size: Table size

    Returns:
        VCGTTable for white point shift
    """
    def kelvin_to_rgb(temp: float) -> Tuple[float, float, float]:
        """Approximate CCT to RGB multipliers."""
        temp = temp / 100.0

        if temp <= 66:
            r = 1.0
            g = np.clip(0.390 * np.log(temp) - 0.631, 0, 1)
        else:
            r = np.clip(1.292 * np.power(temp - 60, -0.1332), 0, 1)
            g = np.clip(1.130 * np.power(temp - 60, -0.0755), 0, 1)

        if temp >= 66:
            b = 1.0
        elif temp <= 19:
            b = 0.0
        else:
            b = np.clip(0.543 * np.log(temp - 10) - 1.196, 0, 1)

        return r, g, b

    # Get RGB multipliers
    cur_r, cur_g, cur_b = kelvin_to_rgb(current_kelvin)
    tgt_r, tgt_g, tgt_b = kelvin_to_rgb(target_kelvin)

    # Calculate correction ratios
    r_ratio = tgt_r / cur_r if cur_r > 0 else 1.0
    g_ratio = tgt_g / cur_g if cur_g > 0 else 1.0
    b_ratio = tgt_b / cur_b if cur_b > 0 else 1.0

    # Normalize to not exceed 1.0
    max_ratio = max(r_ratio, g_ratio, b_ratio)
    r_ratio /= max_ratio
    g_ratio /= max_ratio
    b_ratio /= max_ratio

    x = np.linspace(0, 1, size)

    return VCGTTable(
        red=x * r_ratio,
        green=x * g_ratio,
        blue=x * b_ratio
    )
