"""
Windows HDR MHC2 Tag Support

Implements the Microsoft MHC2 (Matrix HDR Calibration 2) private ICC tag
for Windows 10/11 HDR color management.

The MHC2 tag enables:
- SDR white level configuration (paper white)
- HDR headroom specification
- Color matrix for HDR output
- Tone mapping parameters

This tag is essential for proper HDR display calibration on Windows.

Reference: Microsoft Color Management documentation
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Optional, Tuple, List, Union
from pathlib import Path
import struct


# =============================================================================
# MHC2 Constants
# =============================================================================

# MHC2 tag signature
MHC2_TAG_SIGNATURE = b'MHC2'

# MHC2 version
MHC2_VERSION_1 = 1
MHC2_VERSION_2 = 2

# Default values
DEFAULT_SDR_WHITE_LEVEL = 80.0      # cd/m² (Windows default)
DEFAULT_MIN_LUMINANCE = 0.0         # cd/m²
DEFAULT_MAX_LUMINANCE = 1000.0      # cd/m²

# SDR white level range
SDR_WHITE_MIN = 40.0    # Minimum practical SDR white
SDR_WHITE_MAX = 480.0   # Maximum Windows allows

# HDR luminance range
HDR_MIN_LUMINANCE = 0.0001
HDR_MAX_LUMINANCE = 10000.0


# =============================================================================
# MHC2 Data Structures
# =============================================================================

@dataclass
class MHC2ColorMatrix:
    """
    3x3 color matrix for MHC2.

    Transforms from source color space to display color space.
    Typically used for gamut mapping in HDR.
    """
    matrix: np.ndarray = field(default_factory=lambda: np.eye(3))

    def __post_init__(self):
        self.matrix = np.asarray(self.matrix, dtype=np.float64)
        if self.matrix.shape != (3, 3):
            raise ValueError("Matrix must be 3x3")

    @classmethod
    def identity(cls) -> 'MHC2ColorMatrix':
        """Create identity matrix."""
        return cls(np.eye(3))

    @classmethod
    def from_primaries(
        cls,
        src_red: Tuple[float, float],
        src_green: Tuple[float, float],
        src_blue: Tuple[float, float],
        src_white: Tuple[float, float],
        dst_red: Tuple[float, float],
        dst_green: Tuple[float, float],
        dst_blue: Tuple[float, float],
        dst_white: Tuple[float, float]
    ) -> 'MHC2ColorMatrix':
        """
        Create gamut mapping matrix from chromaticity coordinates.

        Args:
            src_*: Source primaries (xy chromaticity)
            dst_*: Destination primaries (xy chromaticity)

        Returns:
            MHC2ColorMatrix for gamut conversion
        """
        def xy_to_XYZ(x: float, y: float) -> np.ndarray:
            """Convert xy chromaticity to XYZ with Y=1."""
            if y == 0:
                return np.array([0, 0, 0])
            return np.array([x/y, 1.0, (1-x-y)/y])

        def primaries_to_matrix(
            red: Tuple[float, float],
            green: Tuple[float, float],
            blue: Tuple[float, float],
            white: Tuple[float, float]
        ) -> np.ndarray:
            """Build RGB to XYZ matrix from primaries."""
            # Primary XYZ
            Xr, Yr, Zr = xy_to_XYZ(*red)
            Xg, Yg, Zg = xy_to_XYZ(*green)
            Xb, Yb, Zb = xy_to_XYZ(*blue)

            # White point XYZ
            Xw, Yw, Zw = xy_to_XYZ(*white)

            # Solve for S (scaling factors)
            M = np.array([
                [Xr, Xg, Xb],
                [Yr, Yg, Yb],
                [Zr, Zg, Zb]
            ])

            S = np.linalg.solve(M, np.array([Xw, Yw, Zw]))

            # RGB to XYZ matrix
            return M * S

        # Build source and destination matrices
        src_matrix = primaries_to_matrix(src_red, src_green, src_blue, src_white)
        dst_matrix = primaries_to_matrix(dst_red, dst_green, dst_blue, dst_white)

        # Combined transform: RGB_src -> XYZ -> RGB_dst
        # M = dst_inv @ src
        dst_inv = np.linalg.inv(dst_matrix)
        combined = dst_inv @ src_matrix

        return cls(combined)

    @classmethod
    def bt2020_to_p3(cls) -> 'MHC2ColorMatrix':
        """Create BT.2020 to Display P3 matrix."""
        return cls.from_primaries(
            src_red=(0.708, 0.292),
            src_green=(0.170, 0.797),
            src_blue=(0.131, 0.046),
            src_white=(0.3127, 0.3290),
            dst_red=(0.680, 0.320),
            dst_green=(0.265, 0.690),
            dst_blue=(0.150, 0.060),
            dst_white=(0.3127, 0.3290)
        )

    @classmethod
    def bt2020_to_srgb(cls) -> 'MHC2ColorMatrix':
        """Create BT.2020 to sRGB matrix."""
        return cls.from_primaries(
            src_red=(0.708, 0.292),
            src_green=(0.170, 0.797),
            src_blue=(0.131, 0.046),
            src_white=(0.3127, 0.3290),
            dst_red=(0.640, 0.330),
            dst_green=(0.300, 0.600),
            dst_blue=(0.150, 0.060),
            dst_white=(0.3127, 0.3290)
        )

    def to_bytes(self) -> bytes:
        """Serialize matrix to MHC2 format (s15Fixed16)."""
        data = b''
        for row in self.matrix:
            for val in row:
                # s15Fixed16 format
                ival = int(round(val * 65536))
                if ival < 0:
                    ival += 0x100000000
                data += struct.pack('>I', ival & 0xFFFFFFFF)
        return data

    @classmethod
    def from_bytes(cls, data: bytes) -> 'MHC2ColorMatrix':
        """Parse matrix from MHC2 format."""
        matrix = np.zeros((3, 3))
        offset = 0

        for i in range(3):
            for j in range(3):
                val = struct.unpack('>I', data[offset:offset+4])[0]
                # Convert from s15Fixed16
                if val >= 0x80000000:
                    val -= 0x100000000
                matrix[i, j] = val / 65536.0
                offset += 4

        return cls(matrix)


@dataclass
class MHC2ToneCurve:
    """
    Tone mapping curve for MHC2.

    Defines the PQ-based tone curve for HDR to display mapping.
    """
    # Curve defined by control points (up to 256)
    points: np.ndarray = field(default_factory=lambda: np.linspace(0, 1, 256))

    def __post_init__(self):
        self.points = np.asarray(self.points, dtype=np.float64)
        self.points = np.clip(self.points, 0.0, 1.0)

    @classmethod
    def identity(cls) -> 'MHC2ToneCurve':
        """Create identity (linear) curve."""
        return cls(np.linspace(0, 1, 256))

    @classmethod
    def from_gamma(cls, gamma: float, size: int = 256) -> 'MHC2ToneCurve':
        """Create gamma curve."""
        x = np.linspace(0, 1, size)
        return cls(np.power(x, gamma))

    @classmethod
    def from_pq_to_display(
        cls,
        display_peak: float = 1000.0,
        display_black: float = 0.0,
        sdr_white: float = 80.0,
        size: int = 256
    ) -> 'MHC2ToneCurve':
        """
        Create tone curve for PQ to display mapping.

        Args:
            display_peak: Display peak luminance (cd/m²)
            display_black: Display black level (cd/m²)
            sdr_white: SDR white reference level (cd/m²)
            size: Curve size

        Returns:
            MHC2ToneCurve for HDR display
        """
        from calibrate_pro.hdr.pq_st2084 import pq_eotf, pq_oetf

        # Input PQ values
        pq_input = np.linspace(0, 1, size)

        # Convert to luminance
        luminance = pq_eotf(pq_input)

        # Scale to display range
        # Below SDR white: linear
        # Above SDR white: tone mapped

        output = np.zeros_like(luminance)

        sdr_pq = pq_oetf(np.array([sdr_white]))[0]
        sdr_idx = int(sdr_pq * (size - 1))

        # Linear below SDR white
        for i in range(min(sdr_idx + 1, size)):
            output[i] = luminance[i] / display_peak

        # Tone mapped above SDR white
        if sdr_idx < size - 1:
            for i in range(sdr_idx + 1, size):
                # Soft roll-off
                excess = luminance[i] - sdr_white
                headroom = display_peak - sdr_white

                if headroom > 0:
                    # Reinhard-style compression
                    compressed = sdr_white + headroom * excess / (excess + headroom)
                    output[i] = compressed / display_peak
                else:
                    output[i] = 1.0

        return cls(np.clip(output, 0, 1))

    def to_bytes(self) -> bytes:
        """Serialize to MHC2 format (16-bit)."""
        data = struct.pack('>I', len(self.points))

        for val in self.points:
            data += struct.pack('>H', int(val * 65535))

        # Pad to 4-byte boundary
        while len(data) % 4 != 0:
            data += b'\x00'

        return data

    @classmethod
    def from_bytes(cls, data: bytes) -> 'MHC2ToneCurve':
        """Parse from MHC2 format."""
        count = struct.unpack('>I', data[:4])[0]
        points = np.zeros(count)

        offset = 4
        for i in range(count):
            points[i] = struct.unpack('>H', data[offset:offset+2])[0] / 65535.0
            offset += 2

        return cls(points)


@dataclass
class MHC2Tag:
    """
    Complete MHC2 tag for Windows HDR calibration.

    Contains all parameters needed for Windows HDR color management.
    """
    # Version
    version: int = MHC2_VERSION_2

    # Luminance settings
    min_luminance: float = 0.0       # Display black level (cd/m²)
    max_luminance: float = 1000.0    # Display peak luminance (cd/m²)
    sdr_white_level: float = 80.0    # SDR reference white (cd/m²)

    # Color matrix (3x3)
    color_matrix: MHC2ColorMatrix = field(default_factory=MHC2ColorMatrix.identity)

    # Tone curves (R, G, B)
    red_curve: Optional[MHC2ToneCurve] = None
    green_curve: Optional[MHC2ToneCurve] = None
    blue_curve: Optional[MHC2ToneCurve] = None

    # Gamut mapping mode
    # 0 = None, 1 = Clip, 2 = Compress
    gamut_mapping_mode: int = 1

    # Flags
    use_tone_curve: bool = True
    use_color_matrix: bool = True

    def __post_init__(self):
        # Validate luminance values
        self.min_luminance = max(HDR_MIN_LUMINANCE, self.min_luminance)
        self.max_luminance = min(HDR_MAX_LUMINANCE, self.max_luminance)
        self.sdr_white_level = np.clip(self.sdr_white_level, SDR_WHITE_MIN, SDR_WHITE_MAX)

        # Initialize default curves if needed
        if self.red_curve is None:
            self.red_curve = MHC2ToneCurve.identity()
        if self.green_curve is None:
            self.green_curve = MHC2ToneCurve.identity()
        if self.blue_curve is None:
            self.blue_curve = MHC2ToneCurve.identity()

    @classmethod
    def for_display(
        cls,
        peak_luminance: float,
        black_level: float = 0.0,
        sdr_white: float = 80.0,
        color_gamut: str = "sRGB"
    ) -> 'MHC2Tag':
        """
        Create MHC2 tag for a specific display.

        Args:
            peak_luminance: Display peak luminance (cd/m²)
            black_level: Display black level (cd/m²)
            sdr_white: SDR white reference (cd/m²)
            color_gamut: Display gamut ("sRGB", "P3", "BT2020")

        Returns:
            MHC2Tag configured for display
        """
        # Color matrix based on gamut
        if color_gamut.upper() == "P3":
            matrix = MHC2ColorMatrix.bt2020_to_p3()
        elif color_gamut.upper() == "SRGB":
            matrix = MHC2ColorMatrix.bt2020_to_srgb()
        else:
            matrix = MHC2ColorMatrix.identity()

        # Tone curve for this display
        curve = MHC2ToneCurve.from_pq_to_display(
            display_peak=peak_luminance,
            display_black=black_level,
            sdr_white=sdr_white
        )

        return cls(
            min_luminance=black_level,
            max_luminance=peak_luminance,
            sdr_white_level=sdr_white,
            color_matrix=matrix,
            red_curve=curve,
            green_curve=MHC2ToneCurve(curve.points.copy()),
            blue_curve=MHC2ToneCurve(curve.points.copy()),
        )

    def to_bytes(self) -> bytes:
        """Serialize to ICC tag format."""
        # Tag type signature
        data = MHC2_TAG_SIGNATURE
        data += b'\x00\x00\x00\x00'  # Reserved

        # Version
        data += struct.pack('>I', self.version)

        # Luminance values (as s15Fixed16)
        def to_s15f16(val: float) -> bytes:
            ival = int(round(val * 65536)) & 0xFFFFFFFF
            return struct.pack('>I', ival)

        data += to_s15f16(self.min_luminance)
        data += to_s15f16(self.max_luminance)
        data += to_s15f16(self.sdr_white_level)

        # Flags
        flags = 0
        if self.use_tone_curve:
            flags |= 0x01
        if self.use_color_matrix:
            flags |= 0x02

        data += struct.pack('>I', flags)

        # Gamut mapping mode
        data += struct.pack('>I', self.gamut_mapping_mode)

        # Color matrix
        data += self.color_matrix.to_bytes()

        # Tone curves
        data += self.red_curve.to_bytes()
        data += self.green_curve.to_bytes()
        data += self.blue_curve.to_bytes()

        return data

    @classmethod
    def from_bytes(cls, data: bytes) -> Optional['MHC2Tag']:
        """Parse from ICC tag format."""
        if len(data) < 64:
            return None

        # Check signature
        if data[:4] != MHC2_TAG_SIGNATURE:
            return None

        offset = 8  # Skip signature and reserved

        # Version
        version = struct.unpack('>I', data[offset:offset+4])[0]
        offset += 4

        # Luminance values
        def from_s15f16(d: bytes) -> float:
            val = struct.unpack('>I', d)[0]
            if val >= 0x80000000:
                val -= 0x100000000
            return val / 65536.0

        min_lum = from_s15f16(data[offset:offset+4])
        offset += 4
        max_lum = from_s15f16(data[offset:offset+4])
        offset += 4
        sdr_white = from_s15f16(data[offset:offset+4])
        offset += 4

        # Flags
        flags = struct.unpack('>I', data[offset:offset+4])[0]
        offset += 4

        use_tone = bool(flags & 0x01)
        use_matrix = bool(flags & 0x02)

        # Gamut mode
        gamut_mode = struct.unpack('>I', data[offset:offset+4])[0]
        offset += 4

        # Color matrix (36 bytes)
        matrix = MHC2ColorMatrix.from_bytes(data[offset:offset+36])
        offset += 36

        # Tone curves
        red_curve = MHC2ToneCurve.from_bytes(data[offset:])
        curve_size = 4 + len(red_curve.points) * 2
        curve_size += (4 - (curve_size % 4)) % 4  # Padding
        offset += curve_size

        green_curve = MHC2ToneCurve.from_bytes(data[offset:])
        offset += curve_size

        blue_curve = MHC2ToneCurve.from_bytes(data[offset:])

        return cls(
            version=version,
            min_luminance=min_lum,
            max_luminance=max_lum,
            sdr_white_level=sdr_white,
            color_matrix=matrix,
            red_curve=red_curve,
            green_curve=green_curve,
            blue_curve=blue_curve,
            gamut_mapping_mode=gamut_mode,
            use_tone_curve=use_tone,
            use_color_matrix=use_matrix
        )


# =============================================================================
# Profile Integration
# =============================================================================

def extract_mhc2_from_profile(profile_path: Union[str, Path]) -> Optional[MHC2Tag]:
    """
    Extract MHC2 tag from ICC profile.

    Args:
        profile_path: Path to ICC profile

    Returns:
        MHC2Tag or None if not found
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

            if sig == MHC2_TAG_SIGNATURE:
                mhc2_data = data[tag_offset:tag_offset+tag_size]
                return MHC2Tag.from_bytes(mhc2_data)

    except Exception:
        pass

    return None


def create_hdr_profile_with_mhc2(
    description: str,
    peak_luminance: float,
    black_level: float = 0.0,
    sdr_white: float = 80.0,
    color_gamut: str = "P3"
) -> bytes:
    """
    Create HDR ICC profile with MHC2 tag.

    Args:
        description: Profile description
        peak_luminance: Display peak (cd/m²)
        black_level: Display black (cd/m²)
        sdr_white: SDR white reference (cd/m²)
        color_gamut: Display gamut

    Returns:
        ICC profile bytes
    """
    from calibrate_pro.profiles.icc_v4 import ICCProfile, ParametricCurve

    # Create base profile
    if color_gamut.upper() == "P3":
        profile = ICCProfile.create_display_p3()
    elif color_gamut.upper() == "BT2020":
        profile = ICCProfile.create_bt2020()
    else:
        profile = ICCProfile.create_srgb()

    profile.description = description

    # Create MHC2 tag
    mhc2 = MHC2Tag.for_display(
        peak_luminance=peak_luminance,
        black_level=black_level,
        sdr_white=sdr_white,
        color_gamut=color_gamut
    )

    profile.mhc2_data = mhc2.to_bytes()

    return profile.build()


# =============================================================================
# Windows HDR Settings
# =============================================================================

@dataclass
class WindowsHDRSettings:
    """
    Windows HDR display settings.

    Represents the HDR configuration for a Windows display.
    """
    # Display identification
    display_name: str = ""
    display_id: int = 0

    # HDR state
    hdr_enabled: bool = False
    hdr_supported: bool = False

    # Luminance settings
    sdr_white_level: float = 80.0
    peak_luminance: float = 1000.0
    min_luminance: float = 0.0

    # Gamut
    color_gamut: str = "sRGB"  # sRGB, P3, BT2020

    # Active profile
    active_profile_path: Optional[str] = None

    def to_mhc2(self) -> MHC2Tag:
        """Create MHC2 tag from settings."""
        return MHC2Tag.for_display(
            peak_luminance=self.peak_luminance,
            black_level=self.min_luminance,
            sdr_white=self.sdr_white_level,
            color_gamut=self.color_gamut
        )


def get_recommended_sdr_white(
    ambient_lux: float,
    display_peak: float
) -> float:
    """
    Calculate recommended SDR white level.

    Based on ambient lighting and display capabilities.

    Args:
        ambient_lux: Ambient light level (lux)
        display_peak: Display peak luminance (cd/m²)

    Returns:
        Recommended SDR white level (cd/m²)
    """
    # Base recommendation from viewing environment
    if ambient_lux < 50:
        # Dark room
        base = 80.0
    elif ambient_lux < 200:
        # Dim room
        base = 120.0
    elif ambient_lux < 500:
        # Normal indoor
        base = 160.0
    else:
        # Bright room
        base = 200.0

    # Cap at 50% of display peak for HDR headroom
    max_sdr = display_peak * 0.5

    return min(base, max_sdr, SDR_WHITE_MAX)


def calculate_hdr_headroom(
    sdr_white: float,
    display_peak: float
) -> float:
    """
    Calculate HDR headroom in stops.

    Args:
        sdr_white: SDR white level (cd/m²)
        display_peak: Display peak (cd/m²)

    Returns:
        HDR headroom in stops (f-stops)
    """
    if sdr_white <= 0 or display_peak <= sdr_white:
        return 0.0

    return np.log2(display_peak / sdr_white)


# =============================================================================
# MHC2 ICC Profile Generation (Phase 2.1)
# =============================================================================

def _xy_to_XYZ(x: float, y: float) -> np.ndarray:
    """Convert xy chromaticity to XYZ with Y=1."""
    if y <= 0:
        return np.array([0.0, 1.0, 0.0])
    return np.array([x / y, 1.0, (1.0 - x - y) / y])


def _build_rgb_to_xyz_matrix(
    red_xy: Tuple[float, float],
    green_xy: Tuple[float, float],
    blue_xy: Tuple[float, float],
    white_xy: Tuple[float, float],
) -> np.ndarray:
    """
    Build a 3x3 RGB-to-XYZ matrix from chromaticity coordinates.

    Uses the standard method: place primary XYZs as columns, then
    scale by the white-point constraint S = M_inv @ W.

    Returns:
        3x3 numpy array (row = X/Y/Z, col = R/G/B).
    """
    Xr, Yr, Zr = _xy_to_XYZ(*red_xy)
    Xg, Yg, Zg = _xy_to_XYZ(*green_xy)
    Xb, Yb, Zb = _xy_to_XYZ(*blue_xy)
    Xw, Yw, Zw = _xy_to_XYZ(*white_xy)

    M = np.array([
        [Xr, Xg, Xb],
        [Yr, Yg, Yb],
        [Zr, Zg, Zb],
    ])

    S = np.linalg.solve(M, np.array([Xw, Yw, Zw]))

    # Scale each column by its S factor
    return M * S  # broadcasting: column j scaled by S[j]


def _float_to_s15fixed16(val: float) -> int:
    """Convert a float to s15Fixed16Number (signed 32-bit integer)."""
    ival = int(round(val * 65536.0))
    # Clamp to 32-bit signed range
    ival = max(-2147483648, min(2147483647, ival))
    # Convert to unsigned representation for struct packing
    if ival < 0:
        ival += 0x100000000
    return ival & 0xFFFFFFFF


def generate_mhc2_profile(
    panel_primaries: Tuple[Tuple[float, float], Tuple[float, float], Tuple[float, float]],
    panel_white: Tuple[float, float],
    target_white: Tuple[float, float] = (0.3127, 0.3290),
    peak_luminance: float = 1000.0,
    min_luminance: float = 0.0001,
    description: str = "Calibrate Pro HDR",
    output_path: Optional[Union[str, Path]] = None,
) -> bytes:
    """
    Generate a complete ICC v4 profile containing the MHC2 tag for
    Windows HDR display calibration.

    The MHC2 tag carries a 3x4 matrix that Windows applies in the HDR
    compositing pipeline to transform display-referred linear RGB to XYZ.

    The profile also includes the standard ICC tags (desc, cprt, wtpt,
    rXYZ, gXYZ, bXYZ, rTRC, gTRC, bTRC, chad) so it is recognized by
    any ICC-aware application.

    Args:
        panel_primaries: ((rx,ry), (gx,gy), (bx,by)) measured panel
                         chromaticity coordinates.
        panel_white:     (wx, wy) measured panel white point.
        target_white:    (wx, wy) desired white point (default D65).
        peak_luminance:  Display peak luminance in cd/m².
        min_luminance:   Display minimum luminance in cd/m².
        description:     Profile description string.
        output_path:     If given, write profile bytes to this file.

    Returns:
        Complete ICC profile as bytes.
    """
    import hashlib
    from datetime import datetime

    # -------------------------------------------------------------------------
    # 1. Compute the 3x3 panel RGB-to-XYZ matrix
    # -------------------------------------------------------------------------
    red_xy, green_xy, blue_xy = panel_primaries
    rgb_to_xyz = _build_rgb_to_xyz_matrix(red_xy, green_xy, blue_xy, panel_white)

    # -------------------------------------------------------------------------
    # 2. Chromatic adaptation from panel white to target white (Bradford)
    # -------------------------------------------------------------------------
    BRADFORD = np.array([
        [ 0.8951000,  0.2664000, -0.1614000],
        [-0.7502000,  1.7135000,  0.0367000],
        [ 0.0389000, -0.0685000,  1.0296000],
    ])
    BRADFORD_INV = np.linalg.inv(BRADFORD)

    src_XYZ = _xy_to_XYZ(*panel_white)
    dst_XYZ = _xy_to_XYZ(*target_white)

    src_cone = BRADFORD @ src_XYZ
    dst_cone = BRADFORD @ dst_XYZ

    scale = np.diag(dst_cone / src_cone)
    adapt = BRADFORD_INV @ scale @ BRADFORD

    # Adapted RGB-to-XYZ: maps panel RGB to XYZ under target illuminant
    adapted_rgb_to_xyz = adapt @ rgb_to_xyz

    # -------------------------------------------------------------------------
    # 3. Build the 3x4 MHC2 matrix (row-major)
    #    3x3 part = adapted RGB->XYZ, 4th column = offset (zeros)
    # -------------------------------------------------------------------------
    mhc2_matrix_3x4 = np.zeros((3, 4))
    mhc2_matrix_3x4[:, :3] = adapted_rgb_to_xyz
    # Column 4 (offset) stays zero -- no offset needed for most displays

    # -------------------------------------------------------------------------
    # 4. Build MHC2 tag binary
    #    Layout:
    #      4 bytes  - tag type signature 'MHC2'
    #      4 bytes  - reserved (0)
    #      4 bytes  - matrix type (1 = 3x4 matrix)
    #      48 bytes - 12 s15Fixed16Number values (3x4 matrix, row-major)
    #    Total: 60 bytes
    # -------------------------------------------------------------------------
    mhc2_tag = MHC2_TAG_SIGNATURE                    # 'MHC2'
    mhc2_tag += b'\x00\x00\x00\x00'                 # reserved
    mhc2_tag += struct.pack('>I', 1)                 # matrix type 1 = 3x4

    for row in range(3):
        for col in range(4):
            mhc2_tag += struct.pack('>I', _float_to_s15fixed16(mhc2_matrix_3x4[row, col]))

    # -------------------------------------------------------------------------
    # 5. Build standard ICC tags
    # -------------------------------------------------------------------------
    # D50 PCS illuminant
    D50_X, D50_Y, D50_Z = 0.96420, 1.00000, 0.82491

    # Bradford D65->D50 for standard ICC chad tag
    d65_XYZ = _xy_to_XYZ(0.3127, 0.3290)
    d50_XYZ = np.array([D50_X, D50_Y, D50_Z])
    d65_cone = BRADFORD @ d65_XYZ
    d50_cone = BRADFORD @ d50_XYZ
    chad_scale = np.diag(d50_cone / d65_cone)
    chad_matrix = BRADFORD_INV @ chad_scale @ BRADFORD

    # Adapt primaries from D65 (target) to D50 (PCS)
    target_to_d50 = np.eye(3)
    if abs(target_white[0] - 0.3127) < 0.001 and abs(target_white[1] - 0.3290) < 0.001:
        # Target is D65, use standard chad
        target_to_d50 = chad_matrix
    else:
        tgt_XYZ = _xy_to_XYZ(*target_white)
        tgt_cone = BRADFORD @ tgt_XYZ
        d50_cone2 = BRADFORD @ d50_XYZ
        tgt_scale = np.diag(d50_cone2 / tgt_cone)
        target_to_d50 = BRADFORD_INV @ tgt_scale @ BRADFORD

    adapted_d50 = target_to_d50 @ adapted_rgb_to_xyz
    red_xyz_d50 = adapted_d50[:, 0]
    green_xyz_d50 = adapted_d50[:, 1]
    blue_xyz_d50 = adapted_d50[:, 2]

    # -- Tag builders (minimal, spec-compliant) --

    def build_mluc_tag(text: str) -> bytes:
        """Multi-localized Unicode description tag."""
        text_bytes = text.encode('utf-16-be')
        tag = b'mluc'
        tag += b'\x00\x00\x00\x00'     # reserved
        tag += struct.pack('>I', 1)     # 1 record
        tag += struct.pack('>I', 12)    # record size
        tag += b'enUS'                  # language/country
        tag += struct.pack('>I', len(text_bytes) + 2)  # string length + BOM
        tag += struct.pack('>I', 28)    # offset to string
        tag += b'\xfe\xff'             # UTF-16 BOM
        tag += text_bytes
        while len(tag) % 4 != 0:
            tag += b'\x00'
        return tag

    def build_text_tag(text: str) -> bytes:
        """Simple ASCII text tag."""
        text_bytes = text.encode('ascii', errors='replace') + b'\x00'
        tag = b'text'
        tag += b'\x00\x00\x00\x00'
        tag += text_bytes
        while len(tag) % 4 != 0:
            tag += b'\x00'
        return tag

    def build_xyz_tag(x: float, y: float, z: float) -> bytes:
        """XYZ data tag."""
        tag = b'XYZ '
        tag += b'\x00\x00\x00\x00'
        tag += struct.pack('>III',
                           _float_to_s15fixed16(x),
                           _float_to_s15fixed16(y),
                           _float_to_s15fixed16(z))
        return tag

    def build_curv_tag(gamma: float) -> bytes:
        """
        Curve tag with a single gamma value.
        For HDR profiles we use a linear TRC (gamma 1.0) because the MHC2
        matrix operates on linear-light values.
        """
        gamma_fixed = int(round(gamma * 256)) & 0xFFFF
        tag = b'curv'
        tag += b'\x00\x00\x00\x00'
        tag += struct.pack('>I', 1)          # count = 1 (parametric)
        tag += struct.pack('>H', gamma_fixed)
        while len(tag) % 4 != 0:
            tag += b'\x00'
        return tag

    def build_chad_tag(matrix_3x3: np.ndarray) -> bytes:
        """Chromatic adaptation tag (sf32 type, 3x3 matrix)."""
        tag = b'sf32'
        tag += b'\x00\x00\x00\x00'
        for row in matrix_3x3:
            for val in row:
                ival = int(round(val * 65536))
                tag += struct.pack('>i', ival)
        return tag

    # Assemble tag dictionary
    tags = {}
    tags[b'desc'] = build_mluc_tag(description)
    tags[b'cprt'] = build_text_tag("Copyright Zain Dana Harper 2024-2025 - Calibrate Pro HDR")
    tags[b'wtpt'] = build_xyz_tag(D50_X, D50_Y, D50_Z)
    tags[b'rXYZ'] = build_xyz_tag(*red_xyz_d50)
    tags[b'gXYZ'] = build_xyz_tag(*green_xyz_d50)
    tags[b'bXYZ'] = build_xyz_tag(*blue_xyz_d50)
    # Linear TRC for HDR (gamma 1.0) -- the MHC2 matrix works on linear values
    tags[b'rTRC'] = build_curv_tag(1.0)
    tags[b'gTRC'] = build_curv_tag(1.0)
    tags[b'bTRC'] = build_curv_tag(1.0)
    tags[b'chad'] = build_chad_tag(chad_matrix)
    tags[b'MHC2'] = mhc2_tag

    # -------------------------------------------------------------------------
    # 6. Assemble ICC profile
    # -------------------------------------------------------------------------
    tag_count = len(tags)
    tag_table_size = 4 + tag_count * 12  # 4 bytes count + 12 per entry
    header_size = 128

    # Calculate offsets
    current_offset = header_size + tag_table_size
    tag_offsets = {}
    tag_data = b''
    for sig, data in tags.items():
        tag_offsets[sig] = current_offset
        tag_data += data
        current_offset += len(data)

    # Build tag table
    tag_table = struct.pack('>I', tag_count)
    for sig, data in tags.items():
        tag_table += sig
        tag_table += struct.pack('>II', tag_offsets[sig], len(data))

    profile_size = header_size + len(tag_table) + len(tag_data)

    # Build header (128 bytes)
    dt = datetime.now()
    date_bytes = struct.pack('>HHHHHH',
                             dt.year, dt.month, dt.day,
                             dt.hour, dt.minute, dt.second)

    header = struct.pack('>I', profile_size)            # 0-3   profile size
    header += b'lcms'                                    # 4-7   preferred CMM
    header += struct.pack('>I', 0x04400000)             # 8-11  version 4.4
    header += b'mntr'                                    # 12-15 display class
    header += b'RGB '                                    # 16-19 color space
    header += b'XYZ '                                    # 20-23 PCS
    header += date_bytes                                 # 24-35 date/time
    header += b'acsp'                                    # 36-39 ICC magic
    header += b'MSFT'                                    # 40-43 platform
    header += struct.pack('>I', 0)                      # 44-47 flags
    header += b'QNTA'                                    # 48-51 manufacturer
    header += b'CALB'                                    # 52-55 model
    header += struct.pack('>Q', 0)                      # 56-63 attributes
    header += struct.pack('>I', 0)                      # 64-67 intent (perceptual)
    header += struct.pack('>III',                        # 68-79 PCS illuminant
                          _float_to_s15fixed16(D50_X),
                          _float_to_s15fixed16(D50_Y),
                          _float_to_s15fixed16(D50_Z))
    header += b'QNTA'                                    # 80-83 creator
    header += b'\x00' * 16                               # 84-99 MD5 (filled later)
    header += b'\x00' * 28                               # 100-127 reserved

    # Combine
    profile = header + tag_table + tag_data

    # Compute MD5 (per ICC spec, zero out fields 44-47, 64-67, 84-99)
    profile_for_hash = bytearray(profile)
    profile_for_hash[44:48] = b'\x00\x00\x00\x00'
    profile_for_hash[64:68] = b'\x00\x00\x00\x00'
    profile_for_hash[84:100] = b'\x00' * 16
    md5_hash = hashlib.md5(bytes(profile_for_hash)).digest()
    profile = profile[:84] + md5_hash + profile[100:]

    # -------------------------------------------------------------------------
    # 7. Optionally save to file
    # -------------------------------------------------------------------------
    if output_path is not None:
        out = Path(output_path)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(profile)

    return profile


def install_mhc2_profile(profile_path: str, display_index: int = 0) -> bool:
    """
    Install an MHC2 ICC profile for a Windows display.

    Copies the profile to the system color directory, registers it via
    the Windows Color Management API, and associates it with the
    specified display.

    Args:
        profile_path: Path to the MHC2 ICC profile file.
        display_index: 0-based display index.

    Returns:
        True if the profile was installed and associated successfully.
    """
    import shutil

    profile_path = Path(profile_path)
    if not profile_path.exists():
        return False

    # Validate that the file is an ICC profile
    try:
        data = profile_path.read_bytes()
        if len(data) < 128 or data[36:40] != b'acsp':
            return False
    except Exception:
        return False

    # Step 1: Copy to system color directory
    try:
        import os
        system_root = os.environ.get('SystemRoot', r'C:\Windows')
        color_dir = Path(system_root) / 'System32' / 'spool' / 'drivers' / 'color'
        if color_dir.exists():
            dest = color_dir / profile_path.name
            shutil.copy2(str(profile_path), str(dest))
        else:
            return False
    except PermissionError:
        # Need admin privileges
        return False
    except Exception:
        return False

    # Step 2: Register and associate with display using mscms.dll
    try:
        import ctypes
        mscms_dll = ctypes.windll.mscms

        # InstallColorProfileW
        result = mscms_dll.InstallColorProfileW(None, str(dest))
        if not result:
            return False

        # Find the target display device name
        user32_dll = ctypes.windll.user32

        class DISPLAY_DEVICE(ctypes.Structure):
            _fields_ = [
                ("cb", ctypes.c_ulong),
                ("DeviceName", ctypes.c_wchar * 32),
                ("DeviceString", ctypes.c_wchar * 128),
                ("StateFlags", ctypes.c_ulong),
                ("DeviceID", ctypes.c_wchar * 128),
                ("DeviceKey", ctypes.c_wchar * 128),
            ]

        dd = DISPLAY_DEVICE()
        dd.cb = ctypes.sizeof(dd)

        device_name = None
        idx = 0
        active_count = 0
        while user32_dll.EnumDisplayDevicesW(None, idx, ctypes.byref(dd), 0):
            if dd.StateFlags & 0x00000001:  # DISPLAY_DEVICE_ACTIVE
                if active_count == display_index:
                    device_name = dd.DeviceName
                    break
                active_count += 1
            idx += 1

        if device_name is None:
            return False

        # WcsAssociateColorProfileWithDevice
        profile_name = profile_path.name
        mscms_dll.WcsAssociateColorProfileWithDevice(
            0,
            profile_name,
            device_name,
        )

        # WcsSetDefaultColorProfile
        mscms_dll.WcsSetDefaultColorProfile(
            0,           # scope (system)
            device_name,
            2,           # display device type
            0,           # subtype
            0,           # profile ID
            profile_name,
        )

        return True

    except Exception:
        return False
