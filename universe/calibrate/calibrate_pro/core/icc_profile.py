"""
ICC Profile Generation Module

Creates ICC v4.4 compliant color profiles for display calibration.
Supports TRC curves, color matrices, and advanced features.
"""

import struct
import hashlib
from dataclasses import dataclass
from datetime import datetime
from typing import List, Optional, Tuple, Union
import numpy as np
from pathlib import Path

from calibrate_pro.core.color_math import (
    D50_WHITE, D65_WHITE, Illuminant,
    bradford_adapt, get_adaptation_matrix,
    primaries_to_xyz_matrix, xyz_to_rgb_matrix
)

# =============================================================================
# ICC Profile Constants
# =============================================================================

# Profile signatures
ICC_MAGIC = b'acsp'
PROFILE_VERSION = 0x04400000  # v4.4

# Device classes
DEVICE_CLASS_INPUT = b'scnr'
DEVICE_CLASS_DISPLAY = b'mntr'
DEVICE_CLASS_OUTPUT = b'prtr'
DEVICE_CLASS_LINK = b'link'
DEVICE_CLASS_ABSTRACT = b'abst'
DEVICE_CLASS_COLORSPACE = b'spac'
DEVICE_CLASS_NAMED = b'nmcl'

# Color spaces
COLOR_SPACE_XYZ = b'XYZ '
COLOR_SPACE_LAB = b'Lab '
COLOR_SPACE_RGB = b'RGB '
COLOR_SPACE_GRAY = b'GRAY'
COLOR_SPACE_CMYK = b'CMYK'

# Platform signatures
PLATFORM_MICROSOFT = b'MSFT'
PLATFORM_APPLE = b'APPL'
PLATFORM_SGI = b'SGI '
PLATFORM_SUN = b'SUNW'

# Rendering intents
INTENT_PERCEPTUAL = 0
INTENT_RELATIVE = 1
INTENT_SATURATION = 2
INTENT_ABSOLUTE = 3

# Tag signatures
TAG_DESC = b'desc'
TAG_CPRT = b'cprt'
TAG_WTPT = b'wtpt'
TAG_BKPT = b'bkpt'
TAG_RXYY = b'rXYZ'
TAG_GXYY = b'gXYZ'
TAG_BXYY = b'bXYZ'
TAG_RTRC = b'rTRC'
TAG_GTRC = b'gTRC'
TAG_BTRC = b'bTRC'
TAG_CHAD = b'chad'
TAG_VCGT = b'vcgt'
TAG_MHC2 = b'MHC2'
TAG_A2B0 = b'A2B0'
TAG_B2A0 = b'B2A0'

# Type signatures
TYPE_DESC = b'desc'
TYPE_MLUC = b'mluc'
TYPE_TEXT = b'text'
TYPE_XYZ = b'XYZ '
TYPE_CURV = b'curv'
TYPE_PARA = b'para'
TYPE_S15F16 = b'sf32'
TYPE_VCGT = b'vcgt'

@dataclass
class ICCHeader:
    """ICC profile header (128 bytes)."""
    profile_size: int = 0
    preferred_cmm: bytes = b'lcms'
    version: int = PROFILE_VERSION
    device_class: bytes = DEVICE_CLASS_DISPLAY
    color_space: bytes = COLOR_SPACE_RGB
    pcs: bytes = COLOR_SPACE_XYZ
    creation_date: datetime = None
    signature: bytes = ICC_MAGIC
    platform: bytes = PLATFORM_MICROSOFT
    flags: int = 0
    manufacturer: bytes = b'QNTA'
    model: bytes = b'CALB'
    attributes: int = 0
    intent: int = INTENT_RELATIVE
    illuminant_x: float = 0.96420
    illuminant_y: float = 1.00000
    illuminant_z: float = 0.82491
    creator: bytes = b'QNTA'

    def __post_init__(self):
        if self.creation_date is None:
            self.creation_date = datetime.now()

    def to_bytes(self) -> bytes:
        """Serialize header to 128 bytes."""
        # Date/time encoding
        dt = self.creation_date
        date_bytes = struct.pack('>HHHHHH',
            dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second)

        # Fixed-point illuminant values (s15Fixed16)
        def to_s15f16(v):
            return int(v * 65536) & 0xFFFFFFFF

        header = struct.pack('>I',  self.profile_size)       # 0-3
        header += self.preferred_cmm[:4].ljust(4, b'\x00')   # 4-7
        header += struct.pack('>I', self.version)            # 8-11
        header += self.device_class[:4].ljust(4, b'\x00')    # 12-15
        header += self.color_space[:4].ljust(4, b'\x00')     # 16-19
        header += self.pcs[:4].ljust(4, b'\x00')             # 20-23
        header += date_bytes                                  # 24-35
        header += self.signature[:4]                          # 36-39
        header += self.platform[:4].ljust(4, b'\x00')        # 40-43
        header += struct.pack('>I', self.flags)              # 44-47
        header += self.manufacturer[:4].ljust(4, b'\x00')    # 48-51
        header += self.model[:4].ljust(4, b'\x00')           # 52-55
        header += struct.pack('>Q', self.attributes)         # 56-63
        header += struct.pack('>I', self.intent)             # 64-67
        header += struct.pack('>III',                         # 68-79
            to_s15f16(self.illuminant_x),
            to_s15f16(self.illuminant_y),
            to_s15f16(self.illuminant_z))
        header += self.creator[:4].ljust(4, b'\x00')         # 80-83
        header += b'\x00' * 16                                # 84-99 (MD5)
        header += b'\x00' * 28                                # 100-127 (reserved)

        return header

class ICCProfile:
    """
    ICC v4 Profile Builder.

    Creates ICC-compliant color profiles for display calibration.
    """

    def __init__(
        self,
        description: str = "Calibrate Pro Display Profile",
        copyright: str = "Copyright Zain Dana Harper 2024-2025",
        manufacturer: str = "QNTA",
        model: str = "CALB"
    ):
        """
        Initialize ICC profile builder.

        Args:
            description: Profile description string
            copyright: Copyright notice
            manufacturer: 4-character manufacturer signature
            model: 4-character model signature
        """
        self.description = description
        self.copyright = copyright
        self.header = ICCHeader(
            manufacturer=manufacturer.encode()[:4],
            model=model.encode()[:4]
        )
        self.tags = {}

        # Default to D65 primaries (will be overwritten)
        self.red_primary = (0.6400, 0.3300)
        self.green_primary = (0.3000, 0.6000)
        self.blue_primary = (0.1500, 0.0600)
        self.white_point = (0.3127, 0.3290)

        # TRC curves (default gamma 2.2)
        self.gamma_red = 2.2
        self.gamma_green = 2.2
        self.gamma_blue = 2.2

        # Optional LUT curves (overrides gamma if set)
        self.trc_red: Optional[np.ndarray] = None
        self.trc_green: Optional[np.ndarray] = None
        self.trc_blue: Optional[np.ndarray] = None

        # VCGT (Video Card Gamma Table)
        self.vcgt: Optional[Tuple[np.ndarray, np.ndarray, np.ndarray]] = None

    def set_primaries(
        self,
        red: Tuple[float, float],
        green: Tuple[float, float],
        blue: Tuple[float, float],
        white: Tuple[float, float] = (0.3127, 0.3290)
    ):
        """Set display primary chromaticities."""
        self.red_primary = red
        self.green_primary = green
        self.blue_primary = blue
        self.white_point = white

    def set_gamma(self, red: float, green: float, blue: float):
        """Set per-channel gamma values (simple power law)."""
        self.gamma_red = red
        self.gamma_green = green
        self.gamma_blue = blue

    def set_trc_curves(
        self,
        red: np.ndarray,
        green: np.ndarray,
        blue: np.ndarray
    ):
        """
        Set per-channel TRC curves (overrides gamma).

        Args:
            red, green, blue: 1D arrays of 256 or 1024 values in [0, 1]
        """
        self.trc_red = np.asarray(red, dtype=np.float64)
        self.trc_green = np.asarray(green, dtype=np.float64)
        self.trc_blue = np.asarray(blue, dtype=np.float64)

    def set_vcgt(
        self,
        red: np.ndarray,
        green: np.ndarray,
        blue: np.ndarray
    ):
        """
        Set VCGT (Video Card Gamma Table) for calibration.

        Args:
            red, green, blue: 1D arrays of calibration values in [0, 1]
        """
        self.vcgt = (
            np.asarray(red, dtype=np.float64),
            np.asarray(green, dtype=np.float64),
            np.asarray(blue, dtype=np.float64)
        )

    def _build_desc_tag(self, text: str) -> bytes:
        """Build multi-localized Unicode description tag (mluc)."""
        # UTF-16BE encoded string
        text_bytes = text.encode('utf-16-be')

        # mluc tag structure
        tag = TYPE_MLUC
        tag += b'\x00\x00\x00\x00'  # Reserved
        tag += struct.pack('>I', 1)  # Number of records
        tag += struct.pack('>I', 12)  # Record size

        # Language/country record
        tag += b'enUS'  # Language and country
        tag += struct.pack('>I', len(text_bytes) + 2)  # String length (with BOM)
        tag += struct.pack('>I', 28)  # Offset to string

        # Padding and string
        tag += b'\xfe\xff'  # UTF-16 BOM
        tag += text_bytes

        # Pad to 4-byte boundary
        while len(tag) % 4 != 0:
            tag += b'\x00'

        return tag

    def _build_text_tag(self, text: str) -> bytes:
        """Build simple text tag."""
        text_bytes = text.encode('ascii', errors='replace') + b'\x00'

        tag = TYPE_TEXT
        tag += b'\x00\x00\x00\x00'  # Reserved
        tag += text_bytes

        # Pad to 4-byte boundary
        while len(tag) % 4 != 0:
            tag += b'\x00'

        return tag

    def _build_xyz_tag(self, x: float, y: float, z: float) -> bytes:
        """Build XYZ tag with single XYZ value."""
        def to_s15f16(v):
            return int(v * 65536) & 0xFFFFFFFF

        tag = TYPE_XYZ
        tag += b'\x00\x00\x00\x00'  # Reserved
        tag += struct.pack('>III',
            to_s15f16(x), to_s15f16(y), to_s15f16(z))

        return tag

    def _build_curv_tag(self, gamma_or_curve: Union[float, np.ndarray]) -> bytes:
        """
        Build TRC curve tag.

        Args:
            gamma_or_curve: Single gamma value or curve array
        """
        if isinstance(gamma_or_curve, (int, float)):
            # Parametric curve with single gamma
            count = 1
            # Store gamma as u8Fixed8 (8.8 fixed point)
            gamma_fixed = int(gamma_or_curve * 256) & 0xFFFF
            curve_data = struct.pack('>H', gamma_fixed)
        else:
            # Table-based curve
            curve = np.asarray(gamma_or_curve, dtype=np.float64)
            count = len(curve)
            # Convert to 16-bit values
            values = np.clip(curve * 65535, 0, 65535).astype(np.uint16)
            curve_data = struct.pack(f'>{count}H', *values)

        tag = TYPE_CURV
        tag += b'\x00\x00\x00\x00'  # Reserved
        tag += struct.pack('>I', count)
        tag += curve_data

        # Pad to 4-byte boundary
        while len(tag) % 4 != 0:
            tag += b'\x00'

        return tag

    def _build_chad_tag(self) -> bytes:
        """Build chromatic adaptation tag (Bradford D65 to D50)."""
        # Get Bradford matrix from D65 (display) to D50 (PCS)
        matrix = get_adaptation_matrix(D65_WHITE, D50_WHITE)

        def to_s15f16(v):
            return struct.pack('>i', int(v * 65536))

        tag = TYPE_S15F16
        tag += b'\x00\x00\x00\x00'  # Reserved

        # Write 3x3 matrix as row-major s15Fixed16 values
        for row in matrix:
            for val in row:
                tag += to_s15f16(val)

        return tag

    def _build_vcgt_tag(self) -> bytes:
        """Build VCGT (Video Card Gamma Table) tag."""
        if self.vcgt is None:
            return b''

        red, green, blue = self.vcgt
        count = len(red)

        tag = TYPE_VCGT
        tag += b'\x00\x00\x00\x00'  # Reserved
        tag += struct.pack('>I', 0)  # Type: table
        tag += struct.pack('>HHH', count, count, count)  # Table sizes
        tag += struct.pack('>H', 16)  # Entry size (16-bit)

        # Write tables as 16-bit values
        for curve in [red, green, blue]:
            values = np.clip(curve * 65535, 0, 65535).astype(np.uint16)
            tag += struct.pack(f'>{count}H', *values)

        # Pad to 4-byte boundary
        while len(tag) % 4 != 0:
            tag += b'\x00'

        return tag

    def _calculate_xyz_primaries(self) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
        """Calculate XYZ values for primaries adapted to D50."""
        # Build RGB to XYZ matrix from primaries
        rgb_to_xyz = primaries_to_xyz_matrix(
            self.red_primary,
            self.green_primary,
            self.blue_primary,
            self.white_point
        )

        # Get individual primary XYZ values
        red_xyz = rgb_to_xyz[:, 0]
        green_xyz = rgb_to_xyz[:, 1]
        blue_xyz = rgb_to_xyz[:, 2]

        # Adapt from D65 to D50 (ICC PCS)
        red_xyz_d50 = bradford_adapt(red_xyz, D65_WHITE, D50_WHITE)
        green_xyz_d50 = bradford_adapt(green_xyz, D65_WHITE, D50_WHITE)
        blue_xyz_d50 = bradford_adapt(blue_xyz, D65_WHITE, D50_WHITE)

        return red_xyz_d50, green_xyz_d50, blue_xyz_d50

    def build(self) -> bytes:
        """
        Build complete ICC profile.

        Returns:
            Complete ICC profile as bytes
        """
        # Build tags
        self.tags[TAG_DESC] = self._build_desc_tag(self.description)
        self.tags[TAG_CPRT] = self._build_text_tag(self.copyright)

        # White point (D50 for PCS)
        self.tags[TAG_WTPT] = self._build_xyz_tag(
            D50_WHITE.X, D50_WHITE.Y, D50_WHITE.Z)

        # Primary XYZ values (adapted to D50)
        red_xyz, green_xyz, blue_xyz = self._calculate_xyz_primaries()
        self.tags[TAG_RXYY] = self._build_xyz_tag(*red_xyz)
        self.tags[TAG_GXYY] = self._build_xyz_tag(*green_xyz)
        self.tags[TAG_BXYY] = self._build_xyz_tag(*blue_xyz)

        # TRC curves
        if self.trc_red is not None:
            self.tags[TAG_RTRC] = self._build_curv_tag(self.trc_red)
            self.tags[TAG_GTRC] = self._build_curv_tag(self.trc_green)
            self.tags[TAG_BTRC] = self._build_curv_tag(self.trc_blue)
        else:
            self.tags[TAG_RTRC] = self._build_curv_tag(self.gamma_red)
            self.tags[TAG_GTRC] = self._build_curv_tag(self.gamma_green)
            self.tags[TAG_BTRC] = self._build_curv_tag(self.gamma_blue)

        # Chromatic adaptation matrix (Bradford D65->D50)
        self.tags[TAG_CHAD] = self._build_chad_tag()

        # Optional VCGT
        if self.vcgt is not None:
            vcgt_data = self._build_vcgt_tag()
            if vcgt_data:
                self.tags[TAG_VCGT] = vcgt_data

        # Build tag table and tag data
        tag_count = len(self.tags)
        tag_table_size = 4 + tag_count * 12  # count + entries
        header_size = 128

        # Calculate tag offsets
        current_offset = header_size + tag_table_size
        tag_offsets = {}
        tag_data = b''

        for sig, data in self.tags.items():
            tag_offsets[sig] = current_offset
            tag_data += data
            current_offset += len(data)

        # Build tag table
        tag_table = struct.pack('>I', tag_count)
        for sig, data in self.tags.items():
            tag_table += sig
            tag_table += struct.pack('>II', tag_offsets[sig], len(data))

        # Calculate total profile size
        profile_size = header_size + len(tag_table) + len(tag_data)

        # Update header with size and build
        self.header.profile_size = profile_size
        header_data = self.header.to_bytes()

        # Combine all parts
        profile = header_data + tag_table + tag_data

        # Calculate and insert MD5 (bytes 84-99)
        # Zero out MD5 area for calculation
        profile_for_hash = bytearray(profile)
        profile_for_hash[44:48] = b'\x00\x00\x00\x00'  # flags
        profile_for_hash[64:68] = b'\x00\x00\x00\x00'  # intent
        profile_for_hash[84:100] = b'\x00' * 16        # MD5

        md5_hash = hashlib.md5(bytes(profile_for_hash)).digest()

        # Insert MD5 into profile
        profile = profile[:84] + md5_hash + profile[100:]

        return profile

    def save(self, filepath: Union[str, Path]) -> Path:
        """
        Save ICC profile to file.

        Args:
            filepath: Output file path

        Returns:
            Path to saved file
        """
        filepath = Path(filepath)
        profile_data = self.build()

        with open(filepath, 'wb') as f:
            f.write(profile_data)

        return filepath


def create_display_profile(
    description: str,
    red_primary: Tuple[float, float],
    green_primary: Tuple[float, float],
    blue_primary: Tuple[float, float],
    white_point: Tuple[float, float] = (0.3127, 0.3290),
    gamma: Union[float, Tuple[float, float, float]] = 2.2,
    trc_curves: Optional[Tuple[np.ndarray, np.ndarray, np.ndarray]] = None,
    vcgt: Optional[Tuple[np.ndarray, np.ndarray, np.ndarray]] = None,
    copyright: str = "Copyright Zain Dana Harper 2024-2025 - Calibrate Pro"
) -> ICCProfile:
    """
    Create a display calibration profile.

    Args:
        description: Profile description
        red_primary: Red primary (x, y) chromaticity
        green_primary: Green primary (x, y) chromaticity
        blue_primary: Blue primary (x, y) chromaticity
        white_point: White point (x, y) chromaticity
        gamma: Single gamma or (red, green, blue) gammas
        trc_curves: Optional (red, green, blue) TRC curves
        vcgt: Optional VCGT calibration curves
        copyright: Copyright string

    Returns:
        Configured ICCProfile instance
    """
    profile = ICCProfile(description=description, copyright=copyright)
    profile.set_primaries(red_primary, green_primary, blue_primary, white_point)

    # Generate full 256-point TRC curves for maximum compatibility.
    # Many applications don't handle single-gamma-value ICC curves correctly.
    if trc_curves is not None:
        profile.set_trc_curves(*trc_curves)
    elif isinstance(gamma, (tuple, list)):
        profile.set_trc_curves(
            generate_trc_curve(gamma[0]),
            generate_trc_curve(gamma[1]),
            generate_trc_curve(gamma[2])
        )
    else:
        curve = generate_trc_curve(gamma)
        profile.set_trc_curves(curve, curve, curve)

    if vcgt is not None:
        profile.set_vcgt(*vcgt)

    return profile


def generate_trc_curve(
    gamma: float,
    points: int = 256,
    black_offset: float = 0.0,
    white_offset: float = 0.0
) -> np.ndarray:
    """
    Generate a TRC curve with optional offsets.

    Args:
        gamma: Power-law gamma value
        points: Number of curve points
        black_offset: Offset for black level
        white_offset: Offset for white level

    Returns:
        TRC curve array
    """
    x = np.linspace(0, 1, points)
    curve = np.power(x, gamma)

    # Apply offsets
    if black_offset != 0 or white_offset != 0:
        curve = black_offset + curve * (1.0 - black_offset - white_offset)

    return np.clip(curve, 0, 1)


def generate_srgb_trc(points: int = 256) -> np.ndarray:
    """Generate sRGB transfer function curve."""
    x = np.linspace(0, 1, points)
    curve = np.zeros_like(x)

    mask = x <= 0.04045
    curve[mask] = x[mask] / 12.92
    curve[~mask] = np.power((x[~mask] + 0.055) / 1.055, 2.4)

    return curve


def generate_bt1886_trc(
    points: int = 256,
    gamma: float = 2.4,
    Lw: float = 1.0,
    Lb: float = 0.0
) -> np.ndarray:
    """
    Generate BT.1886 EOTF curve.

    Args:
        points: Number of curve points
        gamma: Display gamma
        Lw: White luminance (normalized)
        Lb: Black luminance (normalized)
    """
    x = np.linspace(0, 1, points)

    a = (Lw ** (1/gamma) - Lb ** (1/gamma)) ** gamma
    b = Lb ** (1/gamma) / (Lw ** (1/gamma) - Lb ** (1/gamma))

    curve = a * np.power(np.maximum(x + b, 0), gamma)
    curve = curve / Lw  # Normalize to [0, 1]

    return np.clip(curve, 0, 1)
