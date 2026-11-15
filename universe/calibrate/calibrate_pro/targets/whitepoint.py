"""
White Point Targets - Professional white point calibration targets.

Supports:
- CIE Standard Illuminants (D50, D55, D65, D75, etc.)
- DCI-P3 white point
- Custom CCT (Correlated Color Temperature)
- Custom xy chromaticity
- Duv (distance from Planckian locus) adjustment
- ACES white point

For professional colorists and enthusiasts.
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Optional, Tuple, Dict, List
from enum import Enum


class WhitepointPreset(Enum):
    """Standard white point presets."""
    # CIE Standard Illuminants
    D50 = "D50"           # 5003K - Print/Photography standard (ICC PCS)
    D55 = "D55"           # 5503K - Daylight, motion picture
    D60 = "D60"           # 6000K - ACES reference
    D65 = "D65"           # 6504K - sRGB/Broadcast/Web standard
    D75 = "D75"           # 7504K - North sky daylight
    D93 = "D93"           # 9300K - Legacy CRT

    # Industry Standards
    DCI = "DCI-P3"        # 6300K - Digital Cinema
    ACES = "ACES"         # D60 (6000K) - Academy Color Encoding System

    # Tungsten/Studio
    A = "Illuminant A"    # 2856K - Incandescent/Tungsten
    B = "Illuminant B"    # 4874K - Direct sunlight
    C = "Illuminant C"    # 6774K - Average daylight

    # Custom
    NATIVE = "Native"     # Display native white point
    CUSTOM_CCT = "Custom CCT"    # User-defined CCT
    CUSTOM_XY = "Custom xy"      # User-defined chromaticity


# Standard illuminant chromaticity coordinates (x, y)
ILLUMINANT_XY: Dict[str, Tuple[float, float]] = {
    # CIE D-series (daylight)
    "D50": (0.34567, 0.35850),
    "D55": (0.33242, 0.34743),
    "D60": (0.32168, 0.33767),
    "D65": (0.31270, 0.32900),
    "D75": (0.29902, 0.31485),
    "D93": (0.28315, 0.29711),

    # Other CIE illuminants
    "A": (0.44757, 0.40745),
    "B": (0.34842, 0.35161),
    "C": (0.31006, 0.31616),
    "E": (0.33333, 0.33333),  # Equal energy

    # Industry
    "DCI-P3": (0.31400, 0.35100),
    "ACES": (0.32168, 0.33767),  # Same as D60
}

# CCT values for standard illuminants
ILLUMINANT_CCT: Dict[str, int] = {
    "A": 2856,
    "B": 4874,
    "C": 6774,
    "D50": 5003,
    "D55": 5503,
    "D60": 6000,
    "D65": 6504,
    "D75": 7504,
    "D93": 9305,
    "DCI-P3": 6300,
    "ACES": 6000,
}


def planckian_locus_xy(cct: float) -> Tuple[float, float]:
    """
    Calculate xy chromaticity on the Planckian (blackbody) locus.

    Uses Krystek's algorithm for CCT range 1667K - 25000K.

    Args:
        cct: Correlated Color Temperature in Kelvin

    Returns:
        (x, y) chromaticity coordinates
    """
    if cct < 1667:
        cct = 1667
    elif cct > 25000:
        cct = 25000

    # Krystek's algorithm
    u = (0.860117757 + 1.54118254e-4 * cct + 1.28641212e-7 * cct**2) / \
        (1 + 8.42420235e-4 * cct + 7.08145163e-7 * cct**2)
    v = (0.317398726 + 4.22806245e-5 * cct + 4.20481691e-8 * cct**2) / \
        (1 - 2.89741816e-5 * cct + 1.61456053e-7 * cct**2)

    # Convert CIE 1960 UCS to xy
    x = 3 * u / (2 * u - 8 * v + 4)
    y = 2 * v / (2 * u - 8 * v + 4)

    return (x, y)


def daylight_locus_xy(cct: float) -> Tuple[float, float]:
    """
    Calculate xy chromaticity on the CIE daylight locus.

    Valid for CCT range 4000K - 25000K.

    Args:
        cct: Correlated Color Temperature in Kelvin

    Returns:
        (x, y) chromaticity coordinates
    """
    if cct < 4000:
        # Use Planckian locus for very warm CCT
        return planckian_locus_xy(cct)
    elif cct > 25000:
        cct = 25000

    # CIE daylight locus equations
    if cct <= 7000:
        x = -4.6070e9 / cct**3 + 2.9678e6 / cct**2 + 0.09911e3 / cct + 0.244063
    else:
        x = -2.0064e9 / cct**3 + 1.9018e6 / cct**2 + 0.24748e3 / cct + 0.237040

    y = -3.000 * x**2 + 2.870 * x - 0.275

    return (x, y)


def cct_to_xy(cct: float, daylight: bool = True, duv: float = 0.0) -> Tuple[float, float]:
    """
    Convert CCT to xy chromaticity with optional Duv offset.

    Args:
        cct: Correlated Color Temperature in Kelvin
        daylight: Use daylight locus (True) or Planckian locus (False)
        duv: Distance from Planckian locus (+ = green tint, - = magenta tint)

    Returns:
        (x, y) chromaticity coordinates
    """
    if daylight and cct >= 4000:
        x, y = daylight_locus_xy(cct)
    else:
        x, y = planckian_locus_xy(cct)

    if duv != 0.0:
        # Apply Duv offset perpendicular to locus
        x, y = apply_duv_offset(x, y, cct, duv)

    return (x, y)


def apply_duv_offset(x: float, y: float, cct: float, duv: float) -> Tuple[float, float]:
    """
    Apply Duv offset perpendicular to the Planckian locus.

    Args:
        x, y: Original chromaticity
        cct: Color temperature
        duv: Distance from Planckian locus

    Returns:
        Adjusted (x, y) coordinates
    """
    # Calculate slope of isotherm (perpendicular to locus)
    # Use numerical differentiation
    delta = 10  # K
    x1, y1 = planckian_locus_xy(cct - delta)
    x2, y2 = planckian_locus_xy(cct + delta)

    dx = x2 - x1
    dy = y2 - y1

    # Perpendicular direction (normalized)
    length = np.sqrt(dx**2 + dy**2)
    perp_x = -dy / length
    perp_y = dx / length

    # Apply Duv offset
    x_new = x + duv * perp_x
    y_new = y + duv * perp_y

    return (x_new, y_new)


def xy_to_cct(x: float, y: float) -> Tuple[float, float]:
    """
    Calculate CCT and Duv from xy chromaticity.

    Uses McCamy's approximation for CCT and geometric distance for Duv.

    Args:
        x, y: Chromaticity coordinates

    Returns:
        (CCT in Kelvin, Duv)
    """
    # McCamy's approximation (accurate for 2000K - 12500K)
    n = (x - 0.3320) / (0.1858 - y)
    cct = 449 * n**3 + 3525 * n**2 + 6823.3 * n + 5520.33

    # Calculate Duv
    x_locus, y_locus = planckian_locus_xy(cct)

    # Convert to CIE 1960 UCS for proper Duv calculation
    u = 4 * x / (-2 * x + 12 * y + 3)
    v = 6 * y / (-2 * x + 12 * y + 3)
    u_locus = 4 * x_locus / (-2 * x_locus + 12 * y_locus + 3)
    v_locus = 6 * y_locus / (-2 * x_locus + 12 * y_locus + 3)

    duv = np.sqrt((u - u_locus)**2 + (v - v_locus)**2)

    # Determine sign (+ = above locus = green, - = below = magenta)
    if v < v_locus:
        duv = -duv

    return (cct, duv)


def xy_to_XYZ(x: float, y: float, Y: float = 1.0) -> np.ndarray:
    """
    Convert xy chromaticity to XYZ tristimulus.

    Args:
        x, y: Chromaticity coordinates
        Y: Luminance (default 1.0 for normalized white)

    Returns:
        XYZ array
    """
    X = (x / y) * Y
    Z = ((1 - x - y) / y) * Y
    return np.array([X, Y, Z])


def XYZ_to_xy(XYZ: np.ndarray) -> Tuple[float, float]:
    """Convert XYZ to xy chromaticity."""
    total = XYZ[0] + XYZ[1] + XYZ[2]
    if total == 0:
        return (0.3127, 0.3290)  # D65 fallback
    return (XYZ[0] / total, XYZ[1] / total)


@dataclass
class WhitepointTarget:
    """
    Professional white point target specification.

    Supports multiple ways to specify white point:
    1. Preset illuminant (D65, D50, DCI, etc.)
    2. CCT with optional Duv
    3. Direct xy chromaticity
    4. Native display white

    Attributes:
        preset: Standard illuminant preset
        cct: Correlated Color Temperature (Kelvin)
        duv: Distance from Planckian locus
        xy: Direct chromaticity specification
        use_daylight_locus: Use daylight locus for CCT conversion
        tolerance: Acceptable Delta uv for verification
    """
    preset: WhitepointPreset = WhitepointPreset.D65
    cct: Optional[float] = None
    duv: float = 0.0
    xy: Optional[Tuple[float, float]] = None
    use_daylight_locus: bool = True
    tolerance: float = 0.005  # Delta uv tolerance

    # For display
    name: str = ""
    description: str = ""

    def __post_init__(self):
        if not self.name:
            self.name = self.preset.value

    def get_xy(self) -> Tuple[float, float]:
        """Get target xy chromaticity."""
        # Priority: direct xy > CCT > preset
        if self.xy is not None:
            return self.xy

        if self.cct is not None:
            return cct_to_xy(self.cct, self.use_daylight_locus, self.duv)

        # Preset lookup
        preset_name = self.preset.value
        if preset_name in ILLUMINANT_XY:
            x, y = ILLUMINANT_XY[preset_name]
            if self.duv != 0.0 and preset_name in ILLUMINANT_CCT:
                x, y = apply_duv_offset(x, y, ILLUMINANT_CCT[preset_name], self.duv)
            return (x, y)

        # Default to D65
        return ILLUMINANT_XY["D65"]

    def get_cct(self) -> float:
        """Get target CCT."""
        if self.cct is not None:
            return self.cct

        preset_name = self.preset.value
        if preset_name in ILLUMINANT_CCT:
            return ILLUMINANT_CCT[preset_name]

        # Calculate from xy
        xy = self.get_xy()
        cct, _ = xy_to_cct(xy[0], xy[1])
        return cct

    def get_XYZ(self, Y: float = 1.0) -> np.ndarray:
        """Get target white point as XYZ."""
        x, y = self.get_xy()
        return xy_to_XYZ(x, y, Y)

    def verify(self, measured_xy: Tuple[float, float]) -> Dict:
        """
        Verify measured white point against target.

        Args:
            measured_xy: Measured chromaticity

        Returns:
            Verification results
        """
        target_xy = self.get_xy()

        # Calculate Delta uv (CIE 1960 UCS)
        u_target = 4 * target_xy[0] / (-2 * target_xy[0] + 12 * target_xy[1] + 3)
        v_target = 6 * target_xy[1] / (-2 * target_xy[0] + 12 * target_xy[1] + 3)

        u_measured = 4 * measured_xy[0] / (-2 * measured_xy[0] + 12 * measured_xy[1] + 3)
        v_measured = 6 * measured_xy[1] / (-2 * measured_xy[0] + 12 * measured_xy[1] + 3)

        delta_uv = np.sqrt((u_target - u_measured)**2 + (v_target - v_measured)**2)

        # Calculate CCT and Duv of measurement
        measured_cct, measured_duv = xy_to_cct(measured_xy[0], measured_xy[1])
        target_cct = self.get_cct()

        passed = delta_uv <= self.tolerance

        return {
            "target_xy": target_xy,
            "measured_xy": measured_xy,
            "target_cct": target_cct,
            "measured_cct": measured_cct,
            "target_duv": self.duv,
            "measured_duv": measured_duv,
            "delta_uv": delta_uv,
            "tolerance": self.tolerance,
            "passed": passed,
            "grade": self._grade_result(delta_uv)
        }

    def _grade_result(self, delta_uv: float) -> str:
        """Grade white point accuracy."""
        if delta_uv < 0.002:
            return "Reference (Delta uv < 0.002)"
        elif delta_uv < 0.005:
            return "Professional (Delta uv < 0.005)"
        elif delta_uv < 0.010:
            return "Consumer (Delta uv < 0.01)"
        else:
            return "Uncalibrated"

    def to_dict(self) -> Dict:
        """Serialize to dictionary."""
        return {
            "preset": self.preset.value,
            "cct": self.cct,
            "duv": self.duv,
            "xy": self.xy,
            "name": self.name,
            "description": self.description,
            "target_xy": self.get_xy(),
            "target_cct": self.get_cct()
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "WhitepointTarget":
        """Create from dictionary."""
        return cls(
            preset=WhitepointPreset(data.get("preset", "D65")),
            cct=data.get("cct"),
            duv=data.get("duv", 0.0),
            xy=data.get("xy"),
            name=data.get("name", ""),
            description=data.get("description", "")
        )


# =============================================================================
# Standard Presets
# =============================================================================

# Photography/Print
WHITEPOINT_D50 = WhitepointTarget(
    preset=WhitepointPreset.D50,
    name="D50 (Print Standard)",
    description="ICC Profile Connection Space, photography standard"
)

# Broadcast/Web
WHITEPOINT_D65 = WhitepointTarget(
    preset=WhitepointPreset.D65,
    name="D65 (sRGB/Broadcast)",
    description="sRGB, Rec.709, web content standard"
)

# Digital Cinema
WHITEPOINT_DCI = WhitepointTarget(
    preset=WhitepointPreset.DCI,
    name="DCI-P3 (Digital Cinema)",
    description="Digital Cinema Initiative standard"
)

# ACES/VFX
WHITEPOINT_ACES = WhitepointTarget(
    preset=WhitepointPreset.ACES,
    name="D60 (ACES)",
    description="Academy Color Encoding System"
)

# North Sky
WHITEPOINT_D75 = WhitepointTarget(
    preset=WhitepointPreset.D75,
    name="D75 (North Sky)",
    description="North sky daylight, cooler than D65"
)


def get_whitepoint_presets() -> List[WhitepointTarget]:
    """Get list of standard white point presets."""
    return [
        WHITEPOINT_D65,
        WHITEPOINT_D50,
        WHITEPOINT_DCI,
        WHITEPOINT_ACES,
        WHITEPOINT_D75,
        WhitepointTarget(preset=WhitepointPreset.D55, name="D55 (Motion Picture)"),
        WhitepointTarget(preset=WhitepointPreset.A, name="Illuminant A (Tungsten)"),
        WhitepointTarget(preset=WhitepointPreset.D93, name="D93 (Legacy CRT)"),
    ]


def create_custom_whitepoint(
    cct: Optional[float] = None,
    xy: Optional[Tuple[float, float]] = None,
    duv: float = 0.0,
    name: str = "Custom"
) -> WhitepointTarget:
    """
    Create a custom white point target.

    Args:
        cct: Color temperature in Kelvin
        xy: Direct chromaticity (x, y)
        duv: Distance from Planckian locus
        name: Display name

    Returns:
        WhitepointTarget
    """
    if xy is not None:
        return WhitepointTarget(
            preset=WhitepointPreset.CUSTOM_XY,
            xy=xy,
            duv=duv,
            name=name
        )
    elif cct is not None:
        return WhitepointTarget(
            preset=WhitepointPreset.CUSTOM_CCT,
            cct=cct,
            duv=duv,
            name=name
        )
    else:
        raise ValueError("Must specify either cct or xy")
