"""
ICC v4.4 Profile Generation

Professional-grade ICC v4.4 profile creation with:
- Full tag support (A2B, B2A, TRC, CHAD, etc.)
- Multi-localized description strings
- Embedded measurement data
- Profile connection space optimization
- HDR profile extensions

Reference: ICC.1:2022 Specification
"""

import numpy as np
from dataclasses import dataclass, field
from typing import List, Optional, Tuple, Dict, Any, Union
from enum import IntEnum
from pathlib import Path
import struct
import hashlib
from datetime import datetime
import zlib


# =============================================================================
# ICC Profile Constants
# =============================================================================

# Profile signatures
ICC_MAGIC = b'acsp'
ICC_VERSION_4_4 = 0x04400000

# Profile/Device class signatures
class ProfileClass(IntEnum):
    """ICC profile classes."""
    INPUT = 0x73636E72      # 'scnr' - Scanner/Camera
    DISPLAY = 0x6D6E7472    # 'mntr' - Display
    OUTPUT = 0x70727472     # 'prtr' - Printer
    LINK = 0x6C696E6B       # 'link' - Device Link
    SPACE = 0x73706163      # 'spac' - Color Space
    ABSTRACT = 0x61627374   # 'abst' - Abstract
    NAMED_COLOR = 0x6E6D636C  # 'nmcl' - Named Color


# Color space signatures
class ColorSpace(IntEnum):
    """ICC color space signatures."""
    XYZ = 0x58595A20       # 'XYZ '
    LAB = 0x4C616220       # 'Lab '
    LUV = 0x4C757620       # 'Luv '
    YCBCR = 0x59436272     # 'YCbr'
    YXY = 0x59787920       # 'Yxy '
    RGB = 0x52474220       # 'RGB '
    GRAY = 0x47524159      # 'GRAY'
    HSV = 0x48535620       # 'HSV '
    HLS = 0x484C5320       # 'HLS '
    CMYK = 0x434D594B      # 'CMYK'


# Platform signatures
class Platform(IntEnum):
    """Platform signatures."""
    APPLE = 0x4150504C     # 'APPL'
    MICROSOFT = 0x4D534654 # 'MSFT'
    SILICON_GRAPHICS = 0x53474920  # 'SGI '
    SUN = 0x53554E57       # 'SUNW'


# Rendering intent
class RenderingIntent(IntEnum):
    """Rendering intent values."""
    PERCEPTUAL = 0
    RELATIVE_COLORIMETRIC = 1
    SATURATION = 2
    ABSOLUTE_COLORIMETRIC = 3


# Tag signatures
class TagSignature:
    """Common ICC tag signatures."""
    # Required tags
    PROFILE_DESCRIPTION = b'desc'
    COPYRIGHT = b'cprt'
    MEDIA_WHITE_POINT = b'wtpt'
    CHROMATIC_ADAPTATION = b'chad'

    # Colorimetric tags
    RED_MATRIX_COLUMN = b'rXYZ'
    GREEN_MATRIX_COLUMN = b'gXYZ'
    BLUE_MATRIX_COLUMN = b'bXYZ'
    RED_TRC = b'rTRC'
    GREEN_TRC = b'gTRC'
    BLUE_TRC = b'bTRC'

    # LUT tags
    A2B0 = b'A2B0'  # Perceptual
    A2B1 = b'A2B1'  # Relative colorimetric
    A2B2 = b'A2B2'  # Saturation
    B2A0 = b'B2A0'
    B2A1 = b'B2A1'
    B2A2 = b'B2A2'

    # Gamut tag
    GAMUT = b'gamt'

    # Measurement tags
    MEASUREMENT = b'meas'

    # Video Card Gamma Table
    VCGT = b'vcgt'

    # Windows HDR
    MHC2 = b'MHC2'

    # Viewing conditions
    VIEWING_CONDITIONS = b'view'
    VIEWING_COND_DESC = b'vued'

    # Technology
    TECHNOLOGY = b'tech'

    # Calibration
    CALIBRATION_DATE_TIME = b'calt'
    CHAR_TARGET = b'targ'

    # Multi-localized strings
    DEVICE_MFG_DESC = b'dmnd'
    DEVICE_MODEL_DESC = b'dmdd'


# =============================================================================
# Data Structures
# =============================================================================

@dataclass
class XYZNumber:
    """CIE XYZ values in ICC format (s15Fixed16)."""
    X: float = 0.0
    Y: float = 0.0
    Z: float = 0.0

    def to_bytes(self) -> bytes:
        """Convert to ICC s15Fixed16 format."""
        def to_s15f16(val: float) -> int:
            return int(round(val * 65536)) & 0xFFFFFFFF

        return struct.pack('>III',
            to_s15f16(self.X),
            to_s15f16(self.Y),
            to_s15f16(self.Z)
        )

    @classmethod
    def from_bytes(cls, data: bytes) -> 'XYZNumber':
        """Parse from ICC format."""
        x, y, z = struct.unpack('>III', data[:12])

        def from_s15f16(val: int) -> float:
            if val >= 0x80000000:
                val -= 0x100000000
            return val / 65536.0

        return cls(from_s15f16(x), from_s15f16(y), from_s15f16(z))


@dataclass
class DateTimeNumber:
    """ICC dateTimeNumber."""
    year: int = 2024
    month: int = 1
    day: int = 1
    hour: int = 0
    minute: int = 0
    second: int = 0

    def to_bytes(self) -> bytes:
        return struct.pack('>HHHHHH',
            self.year, self.month, self.day,
            self.hour, self.minute, self.second
        )

    @classmethod
    def now(cls) -> 'DateTimeNumber':
        now = datetime.now()
        return cls(now.year, now.month, now.day, now.hour, now.minute, now.second)


@dataclass
class MultiLocalizedString:
    """Multi-localized Unicode string (mluc type)."""
    strings: Dict[Tuple[str, str], str] = field(default_factory=dict)

    def __post_init__(self):
        if not self.strings:
            self.strings = {('en', 'US'): 'Default'}

    def set_string(self, text: str, language: str = 'en', country: str = 'US'):
        """Set string for a locale."""
        self.strings[(language, country)] = text

    def to_bytes(self) -> bytes:
        """Serialize to ICC mluc format."""
        # Tag type signature
        data = b'mluc' + b'\x00\x00\x00\x00'  # Type + reserved

        # Number of records
        num_records = len(self.strings)
        data += struct.pack('>I', num_records)

        # Record size (always 12)
        data += struct.pack('>I', 12)

        # Calculate string offsets
        header_size = 16 + 12 * num_records
        string_data = b''
        records = []

        for (lang, country), text in self.strings.items():
            # Encode as UTF-16BE
            encoded = text.encode('utf-16-be')
            offset = header_size + len(string_data)
            length = len(encoded)

            # Language and country codes
            lang_code = lang.encode('ascii').ljust(2, b'\x00')[:2]
            country_code = country.encode('ascii').ljust(2, b'\x00')[:2]

            records.append((lang_code, country_code, offset, length))
            string_data += encoded

        # Write records
        for lang_code, country_code, offset, length in records:
            data += lang_code + country_code
            data += struct.pack('>II', length, offset)

        # Write strings
        data += string_data

        # Pad to 4-byte boundary
        while len(data) % 4 != 0:
            data += b'\x00'

        return data


@dataclass
class ParametricCurve:
    """Parametric curve (para type)."""
    function_type: int = 0
    gamma: float = 2.2
    a: float = 1.0
    b: float = 0.0
    c: float = 0.0
    d: float = 0.0
    e: float = 0.0
    f: float = 0.0

    @classmethod
    def srgb(cls) -> 'ParametricCurve':
        """Create sRGB transfer function."""
        return cls(
            function_type=3,
            gamma=2.4,
            a=1.0 / 1.055,
            b=0.055 / 1.055,
            c=1.0 / 12.92,
            d=0.04045,
            e=0.0,
            f=0.0
        )

    @classmethod
    def gamma(cls, gamma: float) -> 'ParametricCurve':
        """Create simple gamma curve."""
        return cls(function_type=0, gamma=gamma)

    @classmethod
    def bt1886(cls, gamma: float = 2.4) -> 'ParametricCurve':
        """Create BT.1886 transfer function."""
        return cls(function_type=0, gamma=gamma)

    def to_bytes(self) -> bytes:
        """Serialize to ICC para format."""
        data = b'para' + b'\x00\x00\x00\x00'  # Type + reserved
        data += struct.pack('>H', self.function_type)
        data += b'\x00\x00'  # Reserved

        def to_s15f16(val: float) -> bytes:
            ival = int(round(val * 65536)) & 0xFFFFFFFF
            return struct.pack('>I', ival)

        # Parameters based on function type
        data += to_s15f16(self.gamma)

        if self.function_type >= 1:
            data += to_s15f16(self.a)
            data += to_s15f16(self.b)

        if self.function_type >= 2:
            data += to_s15f16(self.c)

        if self.function_type >= 3:
            data += to_s15f16(self.d)

        if self.function_type >= 4:
            data += to_s15f16(self.e)
            data += to_s15f16(self.f)

        return data


@dataclass
class CurveData:
    """TRC curve data (curv type)."""
    values: np.ndarray = field(default_factory=lambda: np.array([]))
    gamma: Optional[float] = None  # If set, use gamma instead of table

    @classmethod
    def from_gamma(cls, gamma: float) -> 'CurveData':
        """Create gamma curve."""
        return cls(gamma=gamma)

    @classmethod
    def from_table(cls, values: np.ndarray) -> 'CurveData':
        """Create from table values."""
        return cls(values=values)

    @classmethod
    def identity(cls) -> 'CurveData':
        """Create identity curve (gamma 1.0)."""
        return cls(gamma=1.0)

    def to_bytes(self) -> bytes:
        """Serialize to ICC curv format."""
        data = b'curv' + b'\x00\x00\x00\x00'  # Type + reserved

        if self.gamma is not None:
            # Gamma value encoded as u8Fixed8
            data += struct.pack('>I', 1)  # Count = 1
            gamma_u8f8 = int(round(self.gamma * 256)) & 0xFFFF
            data += struct.pack('>H', gamma_u8f8)
            data += b'\x00\x00'  # Pad
        elif len(self.values) == 0:
            # Identity
            data += struct.pack('>I', 0)
        else:
            # Table
            count = len(self.values)
            data += struct.pack('>I', count)

            # Convert to 16-bit
            values_u16 = (np.clip(self.values, 0, 1) * 65535).astype(np.uint16)
            for v in values_u16:
                data += struct.pack('>H', v)

            # Pad to 4-byte boundary
            if count % 2 != 0:
                data += b'\x00\x00'

        return data


@dataclass
class MeasurementData:
    """Measurement data for profile."""
    observer: int = 1  # 1 = CIE 1931
    backing: XYZNumber = field(default_factory=lambda: XYZNumber(0, 0, 0))
    geometry: int = 0  # 0 = unknown
    flare: float = 0.0
    illuminant: int = 1  # 1 = D50

    def to_bytes(self) -> bytes:
        """Serialize to ICC meas format."""
        data = b'meas' + b'\x00\x00\x00\x00'
        data += struct.pack('>I', self.observer)
        data += self.backing.to_bytes()
        data += struct.pack('>I', self.geometry)

        # Flare as u16Fixed16
        flare_u16f16 = int(round(self.flare * 65536)) & 0xFFFFFFFF
        data += struct.pack('>I', flare_u16f16)

        data += struct.pack('>I', self.illuminant)

        return data


# =============================================================================
# 3D LUT Support (mft2/mAB/mBA types)
# =============================================================================

@dataclass
class CLUT:
    """Color Look-Up Table for A2B/B2A tags."""
    data: np.ndarray  # Shape: (size, size, size, channels)
    input_channels: int = 3
    output_channels: int = 3
    grid_points: int = 17

    def to_bytes_mft2(self) -> bytes:
        """Serialize to mft2 format (16-bit precision)."""
        data = b'mft2' + b'\x00\x00\x00\x00'

        # Channels and grid
        data += struct.pack('>BBB', self.input_channels, self.output_channels, self.grid_points)
        data += b'\x00'  # Reserved

        # Identity matrix (for RGB to RGB)
        identity = [1, 0, 0, 0, 1, 0, 0, 0, 1]
        for val in identity:
            s15f16 = int(round(val * 65536)) & 0xFFFFFFFF
            data += struct.pack('>I', s15f16)

        # Input table entries (256 per channel)
        data += struct.pack('>H', 256)
        # Output table entries
        data += struct.pack('>H', 256)

        # Input tables (identity)
        for _ in range(self.input_channels):
            for i in range(256):
                data += struct.pack('>H', i * 257)  # 0-65535

        # CLUT data
        clut_flat = self.data.flatten()
        clut_u16 = (np.clip(clut_flat, 0, 1) * 65535).astype(np.uint16)
        for v in clut_u16:
            data += struct.pack('>H', v)

        # Output tables (identity)
        for _ in range(self.output_channels):
            for i in range(256):
                data += struct.pack('>H', i * 257)

        return data


# =============================================================================
# ICC Profile Class
# =============================================================================

@dataclass
class ICCProfile:
    """
    ICC v4.4 Profile.

    Supports display, input, and output profiles with full tag support.
    """
    # Header fields
    profile_class: ProfileClass = ProfileClass.DISPLAY
    color_space: ColorSpace = ColorSpace.RGB
    pcs: ColorSpace = ColorSpace.XYZ
    rendering_intent: RenderingIntent = RenderingIntent.PERCEPTUAL

    # Identification
    description: str = "Calibration Profile"
    copyright: str = "Created by Calibrate Pro"
    manufacturer: str = ""
    model: str = ""

    # Colorimetric data
    white_point: XYZNumber = field(default_factory=lambda: XYZNumber(0.9642, 1.0, 0.8249))  # D50
    red_primary: XYZNumber = field(default_factory=lambda: XYZNumber(0.4361, 0.2225, 0.0139))
    green_primary: XYZNumber = field(default_factory=lambda: XYZNumber(0.3851, 0.7169, 0.0971))
    blue_primary: XYZNumber = field(default_factory=lambda: XYZNumber(0.1431, 0.0606, 0.7139))

    # Chromatic adaptation
    chromatic_adaptation: Optional[np.ndarray] = None  # 3x3 matrix

    # TRC (Transfer Response Curves)
    red_trc: Optional[Union[CurveData, ParametricCurve]] = None
    green_trc: Optional[Union[CurveData, ParametricCurve]] = None
    blue_trc: Optional[Union[CurveData, ParametricCurve]] = None

    # LUT data
    a2b0: Optional[CLUT] = None  # Device to PCS (perceptual)
    b2a0: Optional[CLUT] = None  # PCS to device

    # VCGT (Video Card Gamma Table)
    vcgt_red: Optional[np.ndarray] = None
    vcgt_green: Optional[np.ndarray] = None
    vcgt_blue: Optional[np.ndarray] = None

    # MHC2 (Windows HDR)
    mhc2_data: Optional[bytes] = None

    # Measurement data
    measurement: Optional[MeasurementData] = None

    # Multi-localized strings
    localized_descriptions: Dict[Tuple[str, str], str] = field(default_factory=dict)

    # Creation date
    creation_date: DateTimeNumber = field(default_factory=DateTimeNumber.now)

    # Custom tags
    custom_tags: Dict[bytes, bytes] = field(default_factory=dict)

    def _build_header(self, profile_size: int, tag_count: int) -> bytes:
        """Build 128-byte profile header."""
        header = bytearray(128)

        # Profile size (offset 0)
        struct.pack_into('>I', header, 0, profile_size)

        # Preferred CMM (offset 4) - leave as 0

        # Version (offset 8) - v4.4
        struct.pack_into('>I', header, 8, ICC_VERSION_4_4)

        # Profile/Device class (offset 12)
        struct.pack_into('>I', header, 12, self.profile_class)

        # Color space (offset 16)
        struct.pack_into('>I', header, 16, self.color_space)

        # PCS (offset 20)
        struct.pack_into('>I', header, 20, self.pcs)

        # Date/time (offset 24)
        header[24:36] = self.creation_date.to_bytes()

        # Magic number (offset 36)
        header[36:40] = ICC_MAGIC

        # Platform (offset 40)
        struct.pack_into('>I', header, 40, Platform.MICROSOFT)

        # Flags (offset 44) - embedded, use with embedded data only
        struct.pack_into('>I', header, 44, 0x00000001)

        # Device manufacturer (offset 48)
        # Device model (offset 52)

        # Device attributes (offset 56)
        struct.pack_into('>Q', header, 56, 0)

        # Rendering intent (offset 64)
        struct.pack_into('>I', header, 64, self.rendering_intent)

        # PCS illuminant (offset 68) - D50
        d50 = XYZNumber(0.9642, 1.0, 0.8249)
        header[68:80] = d50.to_bytes()

        # Creator signature (offset 80)
        header[80:84] = b'CALP'  # Calibrate Pro

        # Profile ID (offset 84) - filled later with MD5

        # Reserved (offset 100-127)

        return bytes(header)

    def _build_tag_table(self, tags: List[Tuple[bytes, int, int]]) -> bytes:
        """Build tag table."""
        data = struct.pack('>I', len(tags))

        for sig, offset, size in tags:
            data += sig
            data += struct.pack('>II', offset, size)

        return data

    def _build_xyz_tag(self, xyz: XYZNumber) -> bytes:
        """Build XYZ tag."""
        return b'XYZ ' + b'\x00\x00\x00\x00' + xyz.to_bytes()

    def _build_chad_tag(self) -> bytes:
        """Build chromatic adaptation tag."""
        data = b'sf32' + b'\x00\x00\x00\x00'

        if self.chromatic_adaptation is not None:
            matrix = self.chromatic_adaptation
        else:
            # Bradford matrix for D65 to D50
            matrix = np.array([
                [1.0479, 0.0229, -0.0502],
                [0.0296, 0.9904, -0.0171],
                [-0.0092, 0.0151, 0.7519]
            ])

        for row in matrix:
            for val in row:
                s15f16 = int(round(val * 65536)) & 0xFFFFFFFF
                data += struct.pack('>I', s15f16)

        return data

    def _build_text_tag(self, text: str) -> bytes:
        """Build text tag (for compatibility)."""
        # Use mluc for v4
        mluc = MultiLocalizedString()
        mluc.set_string(text)

        # Add localized versions
        for (lang, country), localized in self.localized_descriptions.items():
            mluc.set_string(localized, lang, country)

        return mluc.to_bytes()

    def _build_vcgt_tag(self) -> Optional[bytes]:
        """Build VCGT tag."""
        if self.vcgt_red is None:
            return None

        data = b'vcgt' + b'\x00\x00\x00\x00'

        # Type 0 = table
        data += struct.pack('>I', 0)

        # Number of channels
        data += struct.pack('>H', 3)

        # Entries per channel
        num_entries = len(self.vcgt_red)
        data += struct.pack('>H', num_entries)

        # Entry size (2 = 16-bit)
        data += struct.pack('>H', 2)

        # Red channel
        for v in self.vcgt_red:
            data += struct.pack('>H', int(np.clip(v, 0, 1) * 65535))

        # Green channel
        for v in self.vcgt_green:
            data += struct.pack('>H', int(np.clip(v, 0, 1) * 65535))

        # Blue channel
        for v in self.vcgt_blue:
            data += struct.pack('>H', int(np.clip(v, 0, 1) * 65535))

        # Pad to 4-byte boundary
        while len(data) % 4 != 0:
            data += b'\x00'

        return data

    def build(self) -> bytes:
        """Build complete ICC profile."""
        tags_data = []

        # Profile description (required)
        desc_tag = self._build_text_tag(self.description)
        tags_data.append((TagSignature.PROFILE_DESCRIPTION, desc_tag))

        # Copyright (required)
        cprt_tag = self._build_text_tag(self.copyright)
        tags_data.append((TagSignature.COPYRIGHT, cprt_tag))

        # Media white point (required)
        wtpt_tag = self._build_xyz_tag(self.white_point)
        tags_data.append((TagSignature.MEDIA_WHITE_POINT, wtpt_tag))

        # Chromatic adaptation (required for v4)
        chad_tag = self._build_chad_tag()
        tags_data.append((TagSignature.CHROMATIC_ADAPTATION, chad_tag))

        # RGB primaries
        tags_data.append((TagSignature.RED_MATRIX_COLUMN, self._build_xyz_tag(self.red_primary)))
        tags_data.append((TagSignature.GREEN_MATRIX_COLUMN, self._build_xyz_tag(self.green_primary)))
        tags_data.append((TagSignature.BLUE_MATRIX_COLUMN, self._build_xyz_tag(self.blue_primary)))

        # TRC curves
        if self.red_trc is None:
            self.red_trc = ParametricCurve.srgb()
        if self.green_trc is None:
            self.green_trc = ParametricCurve.srgb()
        if self.blue_trc is None:
            self.blue_trc = ParametricCurve.srgb()

        tags_data.append((TagSignature.RED_TRC, self.red_trc.to_bytes()))
        tags_data.append((TagSignature.GREEN_TRC, self.green_trc.to_bytes()))
        tags_data.append((TagSignature.BLUE_TRC, self.blue_trc.to_bytes()))

        # A2B/B2A LUTs
        if self.a2b0 is not None:
            tags_data.append((TagSignature.A2B0, self.a2b0.to_bytes_mft2()))
        if self.b2a0 is not None:
            tags_data.append((TagSignature.B2A0, self.b2a0.to_bytes_mft2()))

        # VCGT
        vcgt_tag = self._build_vcgt_tag()
        if vcgt_tag:
            tags_data.append((TagSignature.VCGT, vcgt_tag))

        # MHC2 (Windows HDR)
        if self.mhc2_data:
            tags_data.append((TagSignature.MHC2, self.mhc2_data))

        # Measurement data
        if self.measurement:
            tags_data.append((TagSignature.MEASUREMENT, self.measurement.to_bytes()))

        # Manufacturer/Model descriptions
        if self.manufacturer:
            mfg_tag = self._build_text_tag(self.manufacturer)
            tags_data.append((TagSignature.DEVICE_MFG_DESC, mfg_tag))

        if self.model:
            model_tag = self._build_text_tag(self.model)
            tags_data.append((TagSignature.DEVICE_MODEL_DESC, model_tag))

        # Custom tags
        for sig, data in self.custom_tags.items():
            tags_data.append((sig, data))

        # Calculate offsets
        header_size = 128
        tag_table_size = 4 + 12 * len(tags_data)

        current_offset = header_size + tag_table_size
        tag_entries = []
        tag_data_block = b''

        for sig, data in tags_data:
            # Align to 4 bytes
            padding = (4 - (len(tag_data_block) % 4)) % 4
            tag_data_block += b'\x00' * padding

            offset = current_offset + len(tag_data_block)
            tag_entries.append((sig, offset, len(data)))
            tag_data_block += data

        # Final padding
        padding = (4 - (len(tag_data_block) % 4)) % 4
        tag_data_block += b'\x00' * padding

        # Total size
        profile_size = header_size + tag_table_size + len(tag_data_block)

        # Build final profile
        header = self._build_header(profile_size, len(tags_data))
        tag_table = self._build_tag_table(tag_entries)
        profile = header + tag_table + tag_data_block

        # Calculate and embed profile ID (MD5)
        profile_for_hash = bytearray(profile)
        profile_for_hash[44:48] = b'\x00\x00\x00\x00'  # Clear flags
        profile_for_hash[84:100] = b'\x00' * 16  # Clear profile ID

        md5_hash = hashlib.md5(bytes(profile_for_hash)).digest()

        profile = bytearray(profile)
        profile[84:100] = md5_hash

        return bytes(profile)

    def save(self, path: Union[str, Path]):
        """Save profile to file."""
        path = Path(path)
        data = self.build()
        path.write_bytes(data)

    @classmethod
    def create_srgb(cls) -> 'ICCProfile':
        """Create standard sRGB profile."""
        return cls(
            description="sRGB IEC61966-2.1",
            copyright="Public Domain",
            # sRGB primaries in XYZ
            red_primary=XYZNumber(0.4361, 0.2225, 0.0139),
            green_primary=XYZNumber(0.3851, 0.7169, 0.0971),
            blue_primary=XYZNumber(0.1431, 0.0606, 0.7139),
            white_point=XYZNumber(0.9505, 1.0, 1.0890),  # D65
            red_trc=ParametricCurve.srgb(),
            green_trc=ParametricCurve.srgb(),
            blue_trc=ParametricCurve.srgb(),
        )

    @classmethod
    def create_display_p3(cls) -> 'ICCProfile':
        """Create Display P3 profile."""
        return cls(
            description="Display P3",
            copyright="Public Domain",
            # P3 primaries
            red_primary=XYZNumber(0.5151, 0.2412, -0.0011),
            green_primary=XYZNumber(0.2920, 0.6922, 0.0419),
            blue_primary=XYZNumber(0.1571, 0.0666, 0.7841),
            white_point=XYZNumber(0.9505, 1.0, 1.0890),  # D65
            red_trc=ParametricCurve.srgb(),
            green_trc=ParametricCurve.srgb(),
            blue_trc=ParametricCurve.srgb(),
        )

    @classmethod
    def create_bt2020(cls) -> 'ICCProfile':
        """Create BT.2020 profile."""
        return cls(
            description="ITU-R BT.2020",
            copyright="Public Domain",
            # BT.2020 primaries
            red_primary=XYZNumber(0.6370, 0.2627, 0.0000),
            green_primary=XYZNumber(0.1446, 0.6780, 0.0281),
            blue_primary=XYZNumber(0.1689, 0.0593, 1.0694),
            white_point=XYZNumber(0.9505, 1.0, 1.0890),  # D65
            red_trc=ParametricCurve.gamma(2.4),  # BT.1886
            green_trc=ParametricCurve.gamma(2.4),
            blue_trc=ParametricCurve.gamma(2.4),
        )


# =============================================================================
# Profile Creation Helpers
# =============================================================================

def create_calibration_profile(
    red_xyz: Tuple[float, float, float],
    green_xyz: Tuple[float, float, float],
    blue_xyz: Tuple[float, float, float],
    white_xyz: Tuple[float, float, float],
    trc_red: np.ndarray,
    trc_green: np.ndarray,
    trc_blue: np.ndarray,
    description: str = "Calibration Profile",
    include_vcgt: bool = True
) -> ICCProfile:
    """
    Create calibration profile from measurements.

    Args:
        red_xyz: Red primary XYZ
        green_xyz: Green primary XYZ
        blue_xyz: Blue primary XYZ
        white_xyz: White point XYZ
        trc_red: Red TRC table (256 or 1024 points)
        trc_green: Green TRC table
        trc_blue: Blue TRC table
        description: Profile description
        include_vcgt: Include VCGT tag

    Returns:
        ICCProfile ready for saving
    """
    profile = ICCProfile(
        description=description,
        red_primary=XYZNumber(*red_xyz),
        green_primary=XYZNumber(*green_xyz),
        blue_primary=XYZNumber(*blue_xyz),
        white_point=XYZNumber(*white_xyz),
        red_trc=CurveData.from_table(trc_red),
        green_trc=CurveData.from_table(trc_green),
        blue_trc=CurveData.from_table(trc_blue),
    )

    if include_vcgt:
        profile.vcgt_red = trc_red
        profile.vcgt_green = trc_green
        profile.vcgt_blue = trc_blue

    return profile


def create_lut_profile(
    lut_data: np.ndarray,
    description: str = "3D LUT Profile",
    grid_size: int = 17
) -> ICCProfile:
    """
    Create profile with embedded 3D LUT.

    Args:
        lut_data: 3D LUT data (size, size, size, 3)
        description: Profile description
        grid_size: LUT grid size

    Returns:
        ICCProfile with A2B0 LUT
    """
    profile = ICCProfile(description=description)

    profile.a2b0 = CLUT(
        data=lut_data,
        grid_points=grid_size
    )

    return profile
