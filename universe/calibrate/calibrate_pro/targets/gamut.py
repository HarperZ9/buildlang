"""
Gamut Targets - Professional color gamut calibration.

Supports:
- sRGB (Rec.709)
- DCI-P3 (D65 and Theater)
- BT.2020 (Rec.2020)
- Adobe RGB (1998)
- ProPhoto RGB
- ACEScg
- Display P3 (Apple)
- Custom primaries

For professional colorists, photographers, and enthusiasts.
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Optional, Tuple, Dict, List
from enum import Enum


class GamutPreset(Enum):
    """Standard color gamut presets."""
    # SDR Standards
    SRGB = "sRGB"                     # Rec.709 / Web standard
    REC709 = "Rec.709"                # Same as sRGB primaries

    # Wide Gamut
    DCI_P3 = "DCI-P3"                 # DCI-P3 (D65 white)
    DCI_P3_THEATER = "DCI-P3 Theater" # DCI-P3 (theater white 6300K)
    DISPLAY_P3 = "Display P3"         # Apple Display P3 (D65)
    ADOBE_RGB = "Adobe RGB"           # Adobe RGB (1998)

    # Ultra Wide
    BT2020 = "BT.2020"                # Rec.2020 / Ultra HD
    PROPHOTO = "ProPhoto RGB"         # Very wide gamut photography
    ACES_CG = "ACEScg"                # ACES computer graphics

    # Legacy
    NTSC_1953 = "NTSC 1953"           # Original NTSC
    PAL_SECAM = "PAL/SECAM"           # European broadcast

    # Native
    NATIVE = "Native"                 # Display native gamut
    CUSTOM = "Custom"


@dataclass
class ColorPrimaries:
    """
    CIE xy chromaticity coordinates for RGB primaries and white point.
    """
    red: Tuple[float, float]      # (x, y)
    green: Tuple[float, float]    # (x, y)
    blue: Tuple[float, float]     # (x, y)
    white: Tuple[float, float]    # (x, y)

    def to_matrix(self) -> np.ndarray:
        """
        Convert primaries to RGB to XYZ transformation matrix.

        Returns:
            3x3 matrix for RGB to XYZ conversion
        """
        # Build primaries matrix
        xr, yr = self.red
        xg, yg = self.green
        xb, yb = self.blue
        xw, yw = self.white

        # Calculate XYZ of primaries (assuming Y=1 for each)
        Xr, Yr, Zr = xr / yr, 1.0, (1 - xr - yr) / yr
        Xg, Yg, Zg = xg / yg, 1.0, (1 - xg - yg) / yg
        Xb, Yb, Zb = xb / yb, 1.0, (1 - xb - yb) / yb

        # Primaries matrix
        P = np.array([
            [Xr, Xg, Xb],
            [Yr, Yg, Yb],
            [Zr, Zg, Zb]
        ])

        # White point XYZ
        Xw = xw / yw
        Yw = 1.0
        Zw = (1 - xw - yw) / yw
        W = np.array([Xw, Yw, Zw])

        # Solve for scaling factors
        S = np.linalg.solve(P, W)

        # Final RGB to XYZ matrix
        M = P * S

        return M

    def to_inverse_matrix(self) -> np.ndarray:
        """Get XYZ to RGB transformation matrix."""
        return np.linalg.inv(self.to_matrix())

    def get_gamut_area(self) -> float:
        """
        Calculate gamut area in xy chromaticity diagram.

        Returns:
            Gamut area (dimensionless)
        """
        # Shoelace formula for triangle area
        x = [self.red[0], self.green[0], self.blue[0]]
        y = [self.red[1], self.green[1], self.blue[1]]

        area = 0.5 * abs(
            (x[0] * (y[1] - y[2])) +
            (x[1] * (y[2] - y[0])) +
            (x[2] * (y[0] - y[1]))
        )

        return area

    def to_dict(self) -> Dict:
        """Serialize to dictionary."""
        return {
            "red": self.red,
            "green": self.green,
            "blue": self.blue,
            "white": self.white
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "ColorPrimaries":
        """Create from dictionary."""
        return cls(
            red=tuple(data["red"]),
            green=tuple(data["green"]),
            blue=tuple(data["blue"]),
            white=tuple(data["white"])
        )


# =============================================================================
# Standard Color Space Primaries
# =============================================================================

PRIMARIES_SRGB = ColorPrimaries(
    red=(0.6400, 0.3300),
    green=(0.3000, 0.6000),
    blue=(0.1500, 0.0600),
    white=(0.3127, 0.3290)  # D65
)

PRIMARIES_REC709 = PRIMARIES_SRGB  # Identical primaries

PRIMARIES_DCI_P3 = ColorPrimaries(
    red=(0.6800, 0.3200),
    green=(0.2650, 0.6900),
    blue=(0.1500, 0.0600),
    white=(0.3127, 0.3290)  # D65 for Display P3
)

PRIMARIES_DCI_P3_THEATER = ColorPrimaries(
    red=(0.6800, 0.3200),
    green=(0.2650, 0.6900),
    blue=(0.1500, 0.0600),
    white=(0.3140, 0.3510)  # DCI white (6300K)
)

PRIMARIES_DISPLAY_P3 = PRIMARIES_DCI_P3  # Same as DCI-P3 D65

PRIMARIES_ADOBE_RGB = ColorPrimaries(
    red=(0.6400, 0.3300),
    green=(0.2100, 0.7100),
    blue=(0.1500, 0.0600),
    white=(0.3127, 0.3290)  # D65
)

PRIMARIES_BT2020 = ColorPrimaries(
    red=(0.7080, 0.2920),
    green=(0.1700, 0.7970),
    blue=(0.1310, 0.0460),
    white=(0.3127, 0.3290)  # D65
)

PRIMARIES_PROPHOTO = ColorPrimaries(
    red=(0.7347, 0.2653),
    green=(0.1596, 0.8404),
    blue=(0.0366, 0.0001),
    white=(0.3457, 0.3585)  # D50
)

PRIMARIES_ACES_CG = ColorPrimaries(
    red=(0.713, 0.293),
    green=(0.165, 0.830),
    blue=(0.128, 0.044),
    white=(0.32168, 0.33767)  # D60 (ACES white)
)

PRIMARIES_NTSC_1953 = ColorPrimaries(
    red=(0.6700, 0.3300),
    green=(0.2100, 0.7100),
    blue=(0.1400, 0.0800),
    white=(0.3100, 0.3160)  # Illuminant C
)

PRIMARIES_PAL_SECAM = ColorPrimaries(
    red=(0.6400, 0.3300),
    green=(0.2900, 0.6000),
    blue=(0.1500, 0.0600),
    white=(0.3127, 0.3290)  # D65
)

# Lookup table
GAMUT_PRIMARIES: Dict[str, ColorPrimaries] = {
    "sRGB": PRIMARIES_SRGB,
    "Rec.709": PRIMARIES_REC709,
    "DCI-P3": PRIMARIES_DCI_P3,
    "DCI-P3 Theater": PRIMARIES_DCI_P3_THEATER,
    "Display P3": PRIMARIES_DISPLAY_P3,
    "Adobe RGB": PRIMARIES_ADOBE_RGB,
    "BT.2020": PRIMARIES_BT2020,
    "ProPhoto RGB": PRIMARIES_PROPHOTO,
    "ACEScg": PRIMARIES_ACES_CG,
    "NTSC 1953": PRIMARIES_NTSC_1953,
    "PAL/SECAM": PRIMARIES_PAL_SECAM,
}


# Reference gamut areas for coverage calculation
GAMUT_AREAS: Dict[str, float] = {
    "sRGB": PRIMARIES_SRGB.get_gamut_area(),
    "DCI-P3": PRIMARIES_DCI_P3.get_gamut_area(),
    "BT.2020": PRIMARIES_BT2020.get_gamut_area(),
    "Adobe RGB": PRIMARIES_ADOBE_RGB.get_gamut_area(),
}


def calculate_gamut_coverage(
    display_primaries: ColorPrimaries,
    reference_primaries: ColorPrimaries
) -> float:
    """
    Calculate how much of a reference gamut is covered by display primaries.

    Uses triangle overlap calculation.

    Args:
        display_primaries: Display color primaries
        reference_primaries: Target/reference color primaries

    Returns:
        Coverage percentage (0-100+, can exceed 100% if wider)
    """
    # Simple area ratio (approximate)
    display_area = display_primaries.get_gamut_area()
    reference_area = reference_primaries.get_gamut_area()

    # This is approximate - proper calculation requires polygon intersection
    return (display_area / reference_area) * 100


def calculate_gamut_volume_coverage(
    display_primaries: ColorPrimaries,
    reference_primaries: ColorPrimaries
) -> float:
    """
    Calculate 3D gamut volume coverage.

    More accurate than 2D area calculation.

    Args:
        display_primaries: Display primaries
        reference_primaries: Reference primaries

    Returns:
        Volume coverage percentage
    """
    # For accurate volume calculation, we'd need to:
    # 1. Generate 3D gamut boundary meshes
    # 2. Calculate intersection volume
    # 3. Compare to reference volume

    # Simplified approximation using area ratio raised to 1.5
    # (accounts for 3D nature of gamut)
    area_ratio = calculate_gamut_coverage(display_primaries, reference_primaries) / 100
    return (area_ratio ** 1.5) * 100


@dataclass
class GamutTarget:
    """
    Professional gamut target specification.

    Supports all major color spaces with primary verification
    and gamut coverage calculation.

    Attributes:
        preset: Standard gamut preset
        primaries: Custom primaries (if preset is Custom or Native)
        target_coverage_srgb: Target sRGB coverage %
        target_coverage_p3: Target DCI-P3 coverage %
        target_coverage_bt2020: Target BT.2020 coverage %
        tolerance_delta_xy: Acceptable primary deviation
    """
    preset: GamutPreset = GamutPreset.SRGB
    primaries: Optional[ColorPrimaries] = None

    # Coverage targets
    target_coverage_srgb: float = 100.0
    target_coverage_p3: float = 0.0
    target_coverage_bt2020: float = 0.0

    # Tolerance
    tolerance_delta_xy: float = 0.005  # Acceptable chromaticity error

    # Display
    name: str = ""
    description: str = ""

    def __post_init__(self):
        if not self.name:
            self.name = self.preset.value

    def get_primaries(self) -> ColorPrimaries:
        """Get target color primaries."""
        if self.primaries is not None:
            return self.primaries

        preset_name = self.preset.value
        if preset_name in GAMUT_PRIMARIES:
            return GAMUT_PRIMARIES[preset_name]

        return PRIMARIES_SRGB

    def get_rgb_to_xyz_matrix(self) -> np.ndarray:
        """Get RGB to XYZ transformation matrix."""
        return self.get_primaries().to_matrix()

    def get_xyz_to_rgb_matrix(self) -> np.ndarray:
        """Get XYZ to RGB transformation matrix."""
        return self.get_primaries().to_inverse_matrix()

    def get_gamut_area(self) -> float:
        """Get gamut area in xy diagram."""
        return self.get_primaries().get_gamut_area()

    def get_srgb_coverage(self) -> float:
        """Calculate sRGB coverage percentage."""
        return calculate_gamut_coverage(self.get_primaries(), PRIMARIES_SRGB)

    def get_p3_coverage(self) -> float:
        """Calculate DCI-P3 coverage percentage."""
        return calculate_gamut_coverage(self.get_primaries(), PRIMARIES_DCI_P3)

    def get_bt2020_coverage(self) -> float:
        """Calculate BT.2020 coverage percentage."""
        return calculate_gamut_coverage(self.get_primaries(), PRIMARIES_BT2020)

    def is_wide_gamut(self) -> bool:
        """Check if this is a wide gamut target (wider than sRGB)."""
        return self.preset in {
            GamutPreset.DCI_P3, GamutPreset.DCI_P3_THEATER,
            GamutPreset.DISPLAY_P3, GamutPreset.ADOBE_RGB,
            GamutPreset.BT2020, GamutPreset.PROPHOTO,
            GamutPreset.ACES_CG
        }

    def verify_primaries(
        self,
        measured_red: Tuple[float, float],
        measured_green: Tuple[float, float],
        measured_blue: Tuple[float, float]
    ) -> Dict:
        """
        Verify measured primaries against target.

        Args:
            measured_red: Measured red primary (x, y)
            measured_green: Measured green primary (x, y)
            measured_blue: Measured blue primary (x, y)

        Returns:
            Verification results
        """
        target = self.get_primaries()

        # Calculate chromaticity errors
        def delta_xy(target: Tuple[float, float], measured: Tuple[float, float]) -> float:
            return np.sqrt((target[0] - measured[0])**2 + (target[1] - measured[1])**2)

        red_error = delta_xy(target.red, measured_red)
        green_error = delta_xy(target.green, measured_green)
        blue_error = delta_xy(target.blue, measured_blue)

        avg_error = (red_error + green_error + blue_error) / 3
        max_error = max(red_error, green_error, blue_error)

        # Check pass/fail
        passed = max_error <= self.tolerance_delta_xy

        return {
            "target_primaries": {
                "red": target.red,
                "green": target.green,
                "blue": target.blue
            },
            "measured_primaries": {
                "red": measured_red,
                "green": measured_green,
                "blue": measured_blue
            },
            "errors": {
                "red": red_error,
                "green": green_error,
                "blue": blue_error
            },
            "average_delta_xy": avg_error,
            "max_delta_xy": max_error,
            "tolerance": self.tolerance_delta_xy,
            "passed": passed,
            "grade": self._grade_result(max_error)
        }

    def verify_coverage(
        self,
        display_primaries: ColorPrimaries
    ) -> Dict:
        """
        Verify gamut coverage of display.

        Args:
            display_primaries: Measured display primaries

        Returns:
            Coverage results
        """
        srgb_cov = calculate_gamut_coverage(display_primaries, PRIMARIES_SRGB)
        p3_cov = calculate_gamut_coverage(display_primaries, PRIMARIES_DCI_P3)
        bt2020_cov = calculate_gamut_coverage(display_primaries, PRIMARIES_BT2020)
        adobe_cov = calculate_gamut_coverage(display_primaries, PRIMARIES_ADOBE_RGB)

        target_cov = calculate_gamut_coverage(display_primaries, self.get_primaries())

        return {
            "sRGB_coverage": srgb_cov,
            "DCI-P3_coverage": p3_cov,
            "BT.2020_coverage": bt2020_cov,
            "Adobe_RGB_coverage": adobe_cov,
            "target_coverage": target_cov,
            "display_area": display_primaries.get_gamut_area(),
            "is_wide_gamut": srgb_cov > 100,
            "grade": self._grade_coverage(srgb_cov, p3_cov)
        }

    def _grade_result(self, max_error: float) -> str:
        """Grade primary accuracy."""
        if max_error < 0.002:
            return "Reference Grade"
        elif max_error < 0.005:
            return "Professional"
        elif max_error < 0.010:
            return "Consumer"
        else:
            return "Uncalibrated"

    def _grade_coverage(self, srgb_cov: float, p3_cov: float) -> str:
        """Grade gamut coverage."""
        if p3_cov >= 99:
            return "Wide Gamut (P3+)"
        elif p3_cov >= 90:
            return "Wide Gamut (90%+ P3)"
        elif srgb_cov >= 99:
            return "Full sRGB"
        elif srgb_cov >= 90:
            return "Standard Gamut"
        else:
            return "Limited Gamut"

    def to_dict(self) -> Dict:
        """Serialize to dictionary."""
        return {
            "preset": self.preset.value,
            "primaries": self.primaries.to_dict() if self.primaries else None,
            "name": self.name,
            "description": self.description,
            "target_primaries": self.get_primaries().to_dict(),
            "gamut_area": self.get_gamut_area(),
            "is_wide_gamut": self.is_wide_gamut()
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "GamutTarget":
        """Create from dictionary."""
        primaries = None
        if data.get("primaries"):
            primaries = ColorPrimaries.from_dict(data["primaries"])

        return cls(
            preset=GamutPreset(data.get("preset", "sRGB")),
            primaries=primaries,
            name=data.get("name", ""),
            description=data.get("description", "")
        )


# =============================================================================
# Standard Presets
# =============================================================================

GAMUT_SRGB = GamutTarget(
    preset=GamutPreset.SRGB,
    target_coverage_srgb=100.0,
    name="sRGB (Rec.709)",
    description="Standard web and consumer content"
)

GAMUT_DCI_P3 = GamutTarget(
    preset=GamutPreset.DCI_P3,
    target_coverage_srgb=100.0,
    target_coverage_p3=100.0,
    name="DCI-P3 (D65)",
    description="Wide gamut for cinema and HDR content"
)

GAMUT_DCI_P3_THEATER = GamutTarget(
    preset=GamutPreset.DCI_P3_THEATER,
    target_coverage_srgb=100.0,
    target_coverage_p3=100.0,
    name="DCI-P3 (Theater)",
    description="Digital cinema with 6300K white point"
)

GAMUT_DISPLAY_P3 = GamutTarget(
    preset=GamutPreset.DISPLAY_P3,
    target_coverage_srgb=100.0,
    target_coverage_p3=100.0,
    name="Display P3",
    description="Apple Display P3 (wide gamut with D65)"
)

GAMUT_ADOBE_RGB = GamutTarget(
    preset=GamutPreset.ADOBE_RGB,
    target_coverage_srgb=100.0,
    name="Adobe RGB (1998)",
    description="Wide gamut for photography"
)

GAMUT_BT2020 = GamutTarget(
    preset=GamutPreset.BT2020,
    target_coverage_srgb=100.0,
    target_coverage_p3=100.0,
    target_coverage_bt2020=100.0,
    name="BT.2020 (Rec.2020)",
    description="Ultra-wide gamut for UHD/HDR"
)

GAMUT_PROPHOTO = GamutTarget(
    preset=GamutPreset.PROPHOTO,
    name="ProPhoto RGB",
    description="Very wide gamut for photography archival"
)

GAMUT_ACES_CG = GamutTarget(
    preset=GamutPreset.ACES_CG,
    name="ACEScg",
    description="ACES computer graphics working space"
)


def get_gamut_presets() -> List[GamutTarget]:
    """Get list of standard gamut presets."""
    return [
        GAMUT_SRGB,
        GAMUT_DCI_P3,
        GAMUT_DISPLAY_P3,
        GAMUT_ADOBE_RGB,
        GAMUT_BT2020,
        GAMUT_DCI_P3_THEATER,
        GAMUT_PROPHOTO,
        GAMUT_ACES_CG,
    ]


def get_sdr_presets() -> List[GamutTarget]:
    """Get SDR gamut presets."""
    return [
        GAMUT_SRGB,
        GAMUT_ADOBE_RGB,
    ]


def get_wide_gamut_presets() -> List[GamutTarget]:
    """Get wide gamut presets."""
    return [p for p in get_gamut_presets() if p.is_wide_gamut()]


def create_custom_gamut(
    red: Tuple[float, float],
    green: Tuple[float, float],
    blue: Tuple[float, float],
    white: Tuple[float, float] = (0.3127, 0.3290),
    name: str = "Custom"
) -> GamutTarget:
    """
    Create a custom gamut target.

    Args:
        red: Red primary (x, y)
        green: Green primary (x, y)
        blue: Blue primary (x, y)
        white: White point (x, y)
        name: Display name

    Returns:
        GamutTarget
    """
    primaries = ColorPrimaries(
        red=red,
        green=green,
        blue=blue,
        white=white
    )

    return GamutTarget(
        preset=GamutPreset.CUSTOM,
        primaries=primaries,
        name=name
    )


def get_gamut_comparison(gamut1: GamutTarget, gamut2: GamutTarget) -> Dict:
    """
    Compare two gamut targets.

    Args:
        gamut1: First gamut
        gamut2: Second gamut

    Returns:
        Comparison results
    """
    p1 = gamut1.get_primaries()
    p2 = gamut2.get_primaries()

    area1 = p1.get_gamut_area()
    area2 = p2.get_gamut_area()

    # Coverage of each in terms of the other
    cov_1_of_2 = calculate_gamut_coverage(p1, p2)
    cov_2_of_1 = calculate_gamut_coverage(p2, p1)

    return {
        "gamut1": gamut1.name,
        "gamut2": gamut2.name,
        "area1": area1,
        "area2": area2,
        "area_ratio": area1 / area2,
        "gamut1_covers_gamut2": cov_1_of_2,
        "gamut2_covers_gamut1": cov_2_of_1,
        "larger": gamut1.name if area1 > area2 else gamut2.name
    }
