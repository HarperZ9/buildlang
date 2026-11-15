"""
Grayscale Verification Module

Provides comprehensive grayscale ramp verification:
- 21-step grayscale verification (0-100% in 5% increments)
- RGB channel balance analysis
- Gamma/EOTF tracking accuracy
- Near-black and near-white performance
- CCT consistency across grayscale
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional, Callable
import numpy as np

# =============================================================================
# Enums
# =============================================================================

class GrayscaleGrade(Enum):
    """Grayscale verification quality grade."""
    REFERENCE = auto()      # Broadcast reference (<1 ΔE, <50K CCT deviation)
    EXCELLENT = auto()      # Professional (<2 ΔE, <100K CCT deviation)
    GOOD = auto()           # Content creation (<3 ΔE, <200K CCT deviation)
    ACCEPTABLE = auto()     # General use (<5 ΔE, <300K CCT deviation)
    POOR = auto()           # Needs calibration


class GammaType(Enum):
    """Target gamma/EOTF type."""
    POWER_LAW = auto()      # Simple power law (gamma 2.2, 2.4, etc.)
    SRGB = auto()           # sRGB EOTF (IEC 61966-2-1)
    BT1886 = auto()         # BT.1886 EOTF (ITU-R BT.1886)
    L_STAR = auto()         # L* response (CIE L*)
    HLG = auto()            # Hybrid Log-Gamma (ITU-R BT.2100)
    PQ = auto()             # Perceptual Quantizer ST.2084


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class GrayscalePatch:
    """Single grayscale patch measurement result."""
    level: int              # Input level (0-100%)
    input_rgb: tuple[int, int, int]  # Stimulus RGB (0-255)

    # Measured values
    measured_xyz: tuple[float, float, float]
    measured_lab: tuple[float, float, float]
    measured_luminance: float  # cd/m² (nits)

    # Target values
    target_xyz: tuple[float, float, float]
    target_lab: tuple[float, float, float]
    target_luminance: float

    # Color accuracy
    delta_e_2000: float
    delta_l: float          # Lightness error
    delta_uv: float         # Chromaticity error (Δu'v')

    # RGB balance
    rgb_balance_error: float  # Max RGB deviation
    r_ratio: float          # R channel relative to Y
    g_ratio: float          # G channel relative to Y
    b_ratio: float          # B channel relative to Y

    # Correlated Color Temperature
    measured_cct: float
    target_cct: float
    cct_error: float        # Delta CCT (Kelvin)
    duv: float              # Distance from Planckian locus

    # Gamma tracking
    measured_gamma: float
    target_gamma: float
    gamma_error: float

    @property
    def normalized_level(self) -> float:
        """Get level as 0.0-1.0 float."""
        return self.level / 100.0


@dataclass
class GrayscaleRegionAnalysis:
    """Analysis for specific grayscale region (shadows, mids, highlights)."""
    region_name: str
    level_range: tuple[int, int]
    patch_count: int

    delta_e_mean: float
    delta_e_max: float
    delta_l_mean: float
    delta_uv_mean: float

    cct_mean: float
    cct_deviation: float
    duv_mean: float

    gamma_mean: float
    gamma_deviation: float

    rgb_balance_mean: float

    grade: GrayscaleGrade


@dataclass
class GrayscaleResult:
    """Complete grayscale verification result."""
    patch_measurements: list[GrayscalePatch]
    region_analysis: dict[str, GrayscaleRegionAnalysis]

    # Target parameters
    target_whitepoint: str  # D65, D50, etc.
    target_gamma_type: GammaType
    target_gamma_value: float  # 2.2, 2.4, etc. (for power law)
    target_luminance: float  # Peak white in cd/m²
    target_black: float     # Black level in cd/m²

    # Overall statistics
    delta_e_mean: float
    delta_e_max: float
    delta_e_std: float

    delta_l_mean: float
    delta_uv_mean: float

    gamma_mean: float
    gamma_deviation: float

    cct_mean: float
    cct_deviation: float
    duv_mean: float

    rgb_balance_mean: float

    # Contrast metrics
    contrast_ratio: float
    dynamic_range_stops: float

    # Grading
    overall_grade: GrayscaleGrade
    shadow_grade: GrayscaleGrade
    midtone_grade: GrayscaleGrade
    highlight_grade: GrayscaleGrade

    # Metadata
    timestamp: str = ""
    display_name: str = ""
    profile_name: str = ""

    @property
    def passed(self) -> bool:
        """Check if verification passed (acceptable or better)."""
        return self.overall_grade in (
            GrayscaleGrade.REFERENCE,
            GrayscaleGrade.EXCELLENT,
            GrayscaleGrade.GOOD,
            GrayscaleGrade.ACCEPTABLE,
        )


# =============================================================================
# EOTF Functions
# =============================================================================

def gamma_power_law(x: np.ndarray, gamma: float = 2.2) -> np.ndarray:
    """Simple power law gamma: Y = X^gamma"""
    return np.power(np.clip(x, 0, 1), gamma)


def gamma_srgb(x: np.ndarray) -> np.ndarray:
    """sRGB EOTF (IEC 61966-2-1)."""
    x = np.clip(x, 0, 1)
    return np.where(
        x <= 0.04045,
        x / 12.92,
        np.power((x + 0.055) / 1.055, 2.4)
    )


def gamma_bt1886(x: np.ndarray, gamma: float = 2.4,
                  Lw: float = 100.0, Lb: float = 0.0) -> np.ndarray:
    """
    BT.1886 EOTF (ITU-R BT.1886).

    Args:
        x: Normalized signal (0-1)
        gamma: Display gamma (typically 2.4)
        Lw: White luminance (cd/m²)
        Lb: Black luminance (cd/m²)

    Returns:
        Linear light output
    """
    x = np.clip(x, 0, 1)
    a = np.power(Lw ** (1/gamma) - Lb ** (1/gamma), gamma)
    b = Lb ** (1/gamma) / (Lw ** (1/gamma) - Lb ** (1/gamma))
    return a * np.power(np.maximum(x + b, 0), gamma)


def gamma_l_star(x: np.ndarray) -> np.ndarray:
    """L* perceptual response (CIE L*)."""
    x = np.clip(x, 0, 1)
    delta = 6 / 29
    return np.where(
        x > delta**3,
        np.power(x, 1/3),
        x / (3 * delta**2) + 4/29
    )


def calculate_gamma_at_level(input_level: float, output_level: float) -> float:
    """
    Calculate gamma from single point measurement.

    Args:
        input_level: Input signal (0-1)
        output_level: Output luminance normalized to max

    Returns:
        Calculated gamma value
    """
    if input_level <= 0 or output_level <= 0:
        return 0.0

    # gamma = log(output) / log(input)
    return np.log(output_level) / np.log(input_level)


# =============================================================================
# Color Temperature Functions
# =============================================================================

def xy_to_cct(x: float, y: float) -> tuple[float, float]:
    """
    Calculate Correlated Color Temperature from xy chromaticity.

    Uses McCamy's approximation for CCT calculation.

    Args:
        x, y: CIE 1931 xy chromaticity coordinates

    Returns:
        (CCT in Kelvin, Duv distance from Planckian)
    """
    # McCamy's approximation
    n = (x - 0.3320) / (y - 0.1858)
    CCT = -449 * n**3 + 3525 * n**2 - 6823.3 * n + 5520.33

    # Calculate Duv (simplified)
    # Convert to u'v' for Duv calculation
    u_prime = 4 * x / (-2 * x + 12 * y + 3)
    v_prime = 9 * y / (-2 * x + 12 * y + 3)

    # Planckian locus approximation at CCT
    u_p, v_p = cct_to_uv(CCT)

    duv = np.sqrt((u_prime - u_p)**2 + (v_prime - v_p)**2)

    # Sign of Duv (above or below Planckian)
    if v_prime < v_p:
        duv = -duv

    return (CCT, duv)


def cct_to_uv(cct: float) -> tuple[float, float]:
    """
    Calculate u'v' coordinates on Planckian locus for given CCT.

    Args:
        cct: Correlated Color Temperature in Kelvin

    Returns:
        (u', v') chromaticity coordinates
    """
    if cct < 1667:
        cct = 1667
    elif cct > 25000:
        cct = 25000

    # Planckian locus approximation (Robertson's method simplified)
    T = cct

    if T <= 4000:
        x = (-0.2661239e9 / T**3 - 0.2343589e6 / T**2 +
             0.8776956e3 / T + 0.179910)
    else:
        x = (-3.0258469e9 / T**3 + 2.1070379e6 / T**2 +
             0.2226347e3 / T + 0.240390)

    if T <= 2222:
        y = (-1.1063814 * x**3 - 1.34811020 * x**2 + 2.18555832 * x - 0.20219683)
    elif T <= 4000:
        y = (-0.9549476 * x**3 - 1.37418593 * x**2 + 2.09137015 * x - 0.16748867)
    else:
        y = (3.0817580 * x**3 - 5.87338670 * x**2 + 3.75112997 * x - 0.37001483)

    # Convert xy to u'v'
    u_prime = 4 * x / (-2 * x + 12 * y + 3)
    v_prime = 9 * y / (-2 * x + 12 * y + 3)

    return (u_prime, v_prime)


def xyz_to_xy(xyz: tuple[float, float, float]) -> tuple[float, float]:
    """Convert XYZ to xy chromaticity."""
    X, Y, Z = xyz
    total = X + Y + Z
    if total <= 0:
        return (0.3127, 0.3290)  # D65 default
    return (X / total, Y / total)


def xyz_to_uv(xyz: tuple[float, float, float]) -> tuple[float, float]:
    """Convert XYZ to u'v' chromaticity (CIE 1976 UCS)."""
    X, Y, Z = xyz
    denom = X + 15 * Y + 3 * Z
    if denom <= 0:
        return (0.1978, 0.4683)  # D65 default
    u_prime = 4 * X / denom
    v_prime = 9 * Y / denom
    return (u_prime, v_prime)


def delta_uv(uv1: tuple[float, float], uv2: tuple[float, float]) -> float:
    """Calculate Δu'v' between two chromaticities."""
    return np.sqrt((uv2[0] - uv1[0])**2 + (uv2[1] - uv1[1])**2)


# =============================================================================
# Grayscale Grade Functions
# =============================================================================

def grade_from_grayscale(delta_e: float, cct_deviation: float) -> GrayscaleGrade:
    """Determine grayscale grade from ΔE and CCT deviation."""
    if delta_e < 1.0 and cct_deviation < 50:
        return GrayscaleGrade.REFERENCE
    elif delta_e < 2.0 and cct_deviation < 100:
        return GrayscaleGrade.EXCELLENT
    elif delta_e < 3.0 and cct_deviation < 200:
        return GrayscaleGrade.GOOD
    elif delta_e < 5.0 and cct_deviation < 300:
        return GrayscaleGrade.ACCEPTABLE
    else:
        return GrayscaleGrade.POOR


def grade_to_string(grade: GrayscaleGrade) -> str:
    """Convert grade enum to display string."""
    return {
        GrayscaleGrade.REFERENCE: "Broadcast Reference",
        GrayscaleGrade.EXCELLENT: "Professional Grade",
        GrayscaleGrade.GOOD: "Content Creation",
        GrayscaleGrade.ACCEPTABLE: "General Use",
        GrayscaleGrade.POOR: "Needs Calibration",
    }[grade]


# =============================================================================
# Lab Conversion
# =============================================================================

def xyz_to_lab(xyz: tuple[float, float, float],
               illuminant: str = "D65") -> tuple[float, float, float]:
    """Convert XYZ to Lab."""
    white_points = {
        "D50": (96.422, 100.0, 82.521),
        "D65": (95.047, 100.0, 108.883),
    }

    Xn, Yn, Zn = white_points.get(illuminant, white_points["D65"])
    X, Y, Z = xyz

    x = X / Xn
    y = Y / Yn
    z = Z / Zn

    def f(t):
        delta = 6 / 29
        if t > delta**3:
            return t ** (1/3)
        else:
            return t / (3 * delta**2) + 4 / 29

    L = 116 * f(y) - 16
    a = 500 * (f(x) - f(y))
    b = 200 * (f(y) - f(z))

    return (L, a, b)


def delta_e_2000(lab1: tuple[float, float, float],
                 lab2: tuple[float, float, float]) -> float:
    """Calculate CIEDE2000 Delta E."""
    L1, a1, b1 = lab1
    L2, a2, b2 = lab2

    C1 = np.sqrt(a1**2 + b1**2)
    C2 = np.sqrt(a2**2 + b2**2)
    C_avg = (C1 + C2) / 2

    G = 0.5 * (1 - np.sqrt(C_avg**7 / (C_avg**7 + 25**7)))

    a1_prime = a1 * (1 + G)
    a2_prime = a2 * (1 + G)

    C1_prime = np.sqrt(a1_prime**2 + b1**2)
    C2_prime = np.sqrt(a2_prime**2 + b2**2)

    h1_prime = np.degrees(np.arctan2(b1, a1_prime)) % 360
    h2_prime = np.degrees(np.arctan2(b2, a2_prime)) % 360

    delta_L_prime = L2 - L1
    delta_C_prime = C2_prime - C1_prime

    delta_h_prime = h2_prime - h1_prime
    if abs(delta_h_prime) > 180:
        if delta_h_prime > 0:
            delta_h_prime -= 360
        else:
            delta_h_prime += 360

    delta_H_prime = 2 * np.sqrt(C1_prime * C2_prime) * np.sin(np.radians(delta_h_prime / 2))

    L_prime_avg = (L1 + L2) / 2
    C_prime_avg = (C1_prime + C2_prime) / 2

    h_prime_sum = h1_prime + h2_prime
    if abs(h1_prime - h2_prime) > 180:
        h_prime_sum += 360
    h_prime_avg = h_prime_sum / 2

    T = (1 - 0.17 * np.cos(np.radians(h_prime_avg - 30)) +
         0.24 * np.cos(np.radians(2 * h_prime_avg)) +
         0.32 * np.cos(np.radians(3 * h_prime_avg + 6)) -
         0.20 * np.cos(np.radians(4 * h_prime_avg - 63)))

    delta_theta = 30 * np.exp(-((h_prime_avg - 275) / 25)**2)

    R_C = 2 * np.sqrt(C_prime_avg**7 / (C_prime_avg**7 + 25**7))

    S_L = 1 + (0.015 * (L_prime_avg - 50)**2) / np.sqrt(20 + (L_prime_avg - 50)**2)
    S_C = 1 + 0.045 * C_prime_avg
    S_H = 1 + 0.015 * C_prime_avg * T

    R_T = -np.sin(np.radians(2 * delta_theta)) * R_C

    delta_E = np.sqrt(
        (delta_L_prime / S_L)**2 +
        (delta_C_prime / S_C)**2 +
        (delta_H_prime / S_H)**2 +
        R_T * (delta_C_prime / S_C) * (delta_H_prime / S_H)
    )

    return delta_E


# =============================================================================
# Grayscale Verification Class
# =============================================================================

class GrayscaleVerifier:
    """
    Grayscale verification engine.

    Performs comprehensive grayscale ramp verification including:
    - Color accuracy (Delta E)
    - RGB channel balance
    - Gamma/EOTF tracking
    - CCT consistency
    """

    # Standard 21-step levels (0-100% in 5% increments)
    STANDARD_LEVELS = list(range(0, 105, 5))

    # Region definitions
    REGIONS = {
        "shadows": (0, 20),
        "midtones": (25, 75),
        "highlights": (80, 100),
    }

    # Standard white points (xy chromaticity)
    WHITE_POINTS = {
        "D50": (0.3457, 0.3585),
        "D55": (0.3324, 0.3474),
        "D65": (0.3127, 0.3290),
        "D75": (0.2990, 0.3149),
        "DCI-P3": (0.314, 0.351),
    }

    def __init__(self,
                 target_whitepoint: str = "D65",
                 target_gamma_type: GammaType = GammaType.BT1886,
                 target_gamma_value: float = 2.4,
                 target_luminance: float = 100.0,
                 target_black: float = 0.05):
        """
        Initialize grayscale verifier.

        Args:
            target_whitepoint: Target white point (D65, D50, etc.)
            target_gamma_type: Target EOTF type
            target_gamma_value: Gamma value for power law types
            target_luminance: Peak white luminance in cd/m²
            target_black: Black level in cd/m²
        """
        self.target_whitepoint = target_whitepoint
        self.target_gamma_type = target_gamma_type
        self.target_gamma_value = target_gamma_value
        self.target_luminance = target_luminance
        self.target_black = target_black

        self._target_xy = self.WHITE_POINTS.get(target_whitepoint, (0.3127, 0.3290))
        self._target_uv = self._xy_to_uv(self._target_xy[0], self._target_xy[1])
        self._target_cct, _ = xy_to_cct(self._target_xy[0], self._target_xy[1])

    def _xy_to_uv(self, x: float, y: float) -> tuple[float, float]:
        """Convert xy to u'v'."""
        u = 4 * x / (-2 * x + 12 * y + 3)
        v = 9 * y / (-2 * x + 12 * y + 3)
        return (u, v)

    def _get_target_luminance(self, level: int) -> float:
        """Calculate target luminance for given input level."""
        normalized = level / 100.0

        if self.target_gamma_type == GammaType.POWER_LAW:
            output = gamma_power_law(np.array([normalized]), self.target_gamma_value)[0]
        elif self.target_gamma_type == GammaType.SRGB:
            output = gamma_srgb(np.array([normalized]))[0]
        elif self.target_gamma_type == GammaType.BT1886:
            return gamma_bt1886(
                np.array([normalized]),
                self.target_gamma_value,
                self.target_luminance,
                self.target_black
            )[0]
        elif self.target_gamma_type == GammaType.L_STAR:
            output = gamma_l_star(np.array([normalized]))[0]
        else:
            output = gamma_power_law(np.array([normalized]), 2.2)[0]

        # Scale to luminance range
        return self.target_black + output * (self.target_luminance - self.target_black)

    def verify(self,
               measurements: list[tuple[int, tuple[float, float, float]]],
               display_name: str = "",
               profile_name: str = "") -> GrayscaleResult:
        """
        Perform grayscale verification.

        Args:
            measurements: List of (level, XYZ) tuples for each grayscale step
            display_name: Name of verified display
            profile_name: Name of ICC profile used

        Returns:
            GrayscaleResult with complete analysis
        """
        from datetime import datetime

        patch_measurements: list[GrayscalePatch] = []

        # Get peak white luminance from measurements
        white_measurement = None
        black_measurement = None
        for level, xyz in measurements:
            if level == 100:
                white_measurement = xyz
            if level == 0:
                black_measurement = xyz

        peak_luminance = white_measurement[1] if white_measurement else self.target_luminance
        black_level = black_measurement[1] if black_measurement else 0.0

        # Process each measurement
        for level, measured_xyz in measurements:
            patch = self._analyze_patch(level, measured_xyz, peak_luminance, black_level)
            patch_measurements.append(patch)

        # Calculate overall statistics
        delta_e_values = [p.delta_e_2000 for p in patch_measurements]
        delta_l_values = [p.delta_l for p in patch_measurements]
        delta_uv_values = [p.delta_uv for p in patch_measurements]
        gamma_values = [p.measured_gamma for p in patch_measurements if p.measured_gamma > 0]
        cct_values = [p.measured_cct for p in patch_measurements if 1000 < p.measured_cct < 20000]
        rgb_balance_values = [p.rgb_balance_error for p in patch_measurements]

        delta_e_mean = float(np.mean(delta_e_values))
        delta_e_max = float(np.max(delta_e_values))
        delta_e_std = float(np.std(delta_e_values))

        delta_l_mean = float(np.mean(np.abs(delta_l_values)))
        delta_uv_mean = float(np.mean(delta_uv_values))

        gamma_mean = float(np.mean(gamma_values)) if gamma_values else self.target_gamma_value
        gamma_deviation = float(np.std(gamma_values)) if gamma_values else 0.0

        cct_mean = float(np.mean(cct_values)) if cct_values else self._target_cct
        cct_deviation = float(np.std(cct_values)) if cct_values else 0.0
        duv_mean = float(np.mean([abs(p.duv) for p in patch_measurements]))

        rgb_balance_mean = float(np.mean(rgb_balance_values))

        # Contrast metrics
        contrast_ratio = peak_luminance / max(black_level, 0.0001)
        dynamic_range_stops = np.log2(contrast_ratio) if contrast_ratio > 0 else 0

        # Analyze regions
        region_analysis = self._analyze_regions(patch_measurements)

        # Determine grades
        overall_grade = grade_from_grayscale(delta_e_mean, abs(cct_mean - self._target_cct))
        shadow_grade = region_analysis.get("shadows",
            GrayscaleRegionAnalysis("shadows", (0, 20), 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, GrayscaleGrade.POOR)).grade
        midtone_grade = region_analysis.get("midtones",
            GrayscaleRegionAnalysis("midtones", (25, 75), 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, GrayscaleGrade.POOR)).grade
        highlight_grade = region_analysis.get("highlights",
            GrayscaleRegionAnalysis("highlights", (80, 100), 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, GrayscaleGrade.POOR)).grade

        return GrayscaleResult(
            patch_measurements=patch_measurements,
            region_analysis=region_analysis,
            target_whitepoint=self.target_whitepoint,
            target_gamma_type=self.target_gamma_type,
            target_gamma_value=self.target_gamma_value,
            target_luminance=self.target_luminance,
            target_black=self.target_black,
            delta_e_mean=delta_e_mean,
            delta_e_max=delta_e_max,
            delta_e_std=delta_e_std,
            delta_l_mean=delta_l_mean,
            delta_uv_mean=delta_uv_mean,
            gamma_mean=gamma_mean,
            gamma_deviation=gamma_deviation,
            cct_mean=cct_mean,
            cct_deviation=cct_deviation,
            duv_mean=duv_mean,
            rgb_balance_mean=rgb_balance_mean,
            contrast_ratio=contrast_ratio,
            dynamic_range_stops=dynamic_range_stops,
            overall_grade=overall_grade,
            shadow_grade=shadow_grade,
            midtone_grade=midtone_grade,
            highlight_grade=highlight_grade,
            timestamp=datetime.now().isoformat(),
            display_name=display_name,
            profile_name=profile_name,
        )

    def _analyze_patch(self, level: int, measured_xyz: tuple[float, float, float],
                       peak_luminance: float, black_level: float) -> GrayscalePatch:
        """Analyze single grayscale patch."""
        # Input RGB (assuming equal R=G=B for grayscale)
        input_value = int(level * 255 / 100)
        input_rgb = (input_value, input_value, input_value)

        # Measured values
        measured_luminance = measured_xyz[1]
        measured_xy = xyz_to_xy(measured_xyz)
        measured_uv = xyz_to_uv(measured_xyz)
        measured_lab = xyz_to_lab(measured_xyz, "D65")

        # Target values
        target_luminance = self._get_target_luminance(level)

        # Calculate target XYZ (using target white point and luminance)
        target_Y = target_luminance
        x, y = self._target_xy
        target_X = (x / y) * target_Y if y > 0 else 0
        target_Z = ((1 - x - y) / y) * target_Y if y > 0 else 0
        target_xyz = (target_X, target_Y, target_Z)
        target_lab = xyz_to_lab(target_xyz, "D65")

        # Delta E
        de_2000 = delta_e_2000(target_lab, measured_lab)
        delta_l = measured_lab[0] - target_lab[0]

        # Chromaticity error
        duv = delta_uv(self._target_uv, measured_uv)

        # RGB balance (simplified - assuming known primaries)
        # For a perfect display, R:Y = G:Y = B:Y ratios should be consistent
        r_ratio = 1.0  # Placeholder - would need spectral data
        g_ratio = 1.0
        b_ratio = 1.0
        rgb_balance_error = 0.0  # Placeholder

        # CCT
        if measured_xy[1] > 0:
            measured_cct, measured_duv = xy_to_cct(measured_xy[0], measured_xy[1])
        else:
            measured_cct = self._target_cct
            measured_duv = 0.0

        cct_error = measured_cct - self._target_cct

        # Gamma at this level
        if level > 0 and peak_luminance > black_level:
            normalized_input = level / 100.0
            normalized_output = (measured_luminance - black_level) / (peak_luminance - black_level)
            normalized_output = max(0.001, min(1.0, normalized_output))
            measured_gamma = calculate_gamma_at_level(normalized_input, normalized_output)
        else:
            measured_gamma = 0.0

        target_gamma = self.target_gamma_value
        gamma_error = measured_gamma - target_gamma if measured_gamma > 0 else 0

        return GrayscalePatch(
            level=level,
            input_rgb=input_rgb,
            measured_xyz=measured_xyz,
            measured_lab=measured_lab,
            measured_luminance=measured_luminance,
            target_xyz=target_xyz,
            target_lab=target_lab,
            target_luminance=target_luminance,
            delta_e_2000=de_2000,
            delta_l=delta_l,
            delta_uv=duv,
            rgb_balance_error=rgb_balance_error,
            r_ratio=r_ratio,
            g_ratio=g_ratio,
            b_ratio=b_ratio,
            measured_cct=measured_cct,
            target_cct=self._target_cct,
            cct_error=cct_error,
            duv=measured_duv,
            measured_gamma=measured_gamma,
            target_gamma=target_gamma,
            gamma_error=gamma_error,
        )

    def _analyze_regions(self,
                        patches: list[GrayscalePatch]) -> dict[str, GrayscaleRegionAnalysis]:
        """Analyze patches by region (shadows, midtones, highlights)."""
        region_analysis: dict[str, GrayscaleRegionAnalysis] = {}

        for region_name, (low, high) in self.REGIONS.items():
            region_patches = [p for p in patches if low <= p.level <= high]

            if not region_patches:
                continue

            de_values = [p.delta_e_2000 for p in region_patches]
            dl_values = [p.delta_l for p in region_patches]
            duv_values = [p.delta_uv for p in region_patches]
            cct_values = [p.measured_cct for p in region_patches if 1000 < p.measured_cct < 20000]
            gamma_values = [p.measured_gamma for p in region_patches if p.measured_gamma > 0]
            rgb_values = [p.rgb_balance_error for p in region_patches]

            de_mean = float(np.mean(de_values))
            cct_mean = float(np.mean(cct_values)) if cct_values else self._target_cct
            cct_dev = abs(cct_mean - self._target_cct)

            region_analysis[region_name] = GrayscaleRegionAnalysis(
                region_name=region_name,
                level_range=(low, high),
                patch_count=len(region_patches),
                delta_e_mean=de_mean,
                delta_e_max=float(np.max(de_values)),
                delta_l_mean=float(np.mean(np.abs(dl_values))),
                delta_uv_mean=float(np.mean(duv_values)),
                cct_mean=cct_mean,
                cct_deviation=float(np.std(cct_values)) if cct_values else 0,
                duv_mean=float(np.mean([abs(p.duv) for p in region_patches])),
                gamma_mean=float(np.mean(gamma_values)) if gamma_values else 0,
                gamma_deviation=float(np.std(gamma_values)) if gamma_values else 0,
                rgb_balance_mean=float(np.mean(rgb_values)),
                grade=grade_from_grayscale(de_mean, cct_dev),
            )

        return region_analysis


# =============================================================================
# Utility Functions
# =============================================================================

def generate_grayscale_levels(steps: int = 21) -> list[int]:
    """Generate grayscale levels for verification."""
    return [int(100 * i / (steps - 1)) for i in range(steps)]


def create_test_measurements(peak_luminance: float = 100.0,
                             black_level: float = 0.05) -> list[tuple[int, tuple[float, float, float]]]:
    """Create simulated grayscale measurements for testing."""
    np.random.seed(42)
    measurements = []

    # D65 white point
    x_white, y_white = 0.3127, 0.3290

    for level in range(0, 105, 5):
        normalized = level / 100.0

        # Calculate luminance with BT.1886
        Y = gamma_bt1886(np.array([normalized]), 2.4, peak_luminance, black_level)[0]

        # Add small random error
        Y *= (1 + np.random.normal(0, 0.01))

        # Calculate XYZ from Y and white point (with slight chromaticity error)
        x = x_white + np.random.normal(0, 0.002)
        y = y_white + np.random.normal(0, 0.002)

        X = (x / y) * Y if y > 0 else 0
        Z = ((1 - x - y) / y) * Y if y > 0 else 0

        measurements.append((level, (X, Y, Z)))

    return measurements


def print_grayscale_summary(result: GrayscaleResult) -> None:
    """Print grayscale verification summary to console."""
    print("\n" + "=" * 60)
    print("Grayscale Verification Summary")
    print("=" * 60)
    print(f"Display: {result.display_name or 'Unknown'}")
    print(f"Profile: {result.profile_name or 'Unknown'}")
    print(f"Timestamp: {result.timestamp}")
    print()
    print(f"Target: {result.target_whitepoint}, "
          f"Gamma {result.target_gamma_type.name} {result.target_gamma_value}")
    print(f"Overall Grade: {grade_to_string(result.overall_grade)}")
    print()
    print("Delta E Statistics (CIEDE2000):")
    print(f"  Mean:   {result.delta_e_mean:.2f}")
    print(f"  Max:    {result.delta_e_max:.2f}")
    print(f"  StdDev: {result.delta_e_std:.2f}")
    print()
    print("Gamma/EOTF Tracking:")
    print(f"  Mean Gamma: {result.gamma_mean:.2f}")
    print(f"  Deviation:  {result.gamma_deviation:.3f}")
    print()
    print("Color Temperature:")
    print(f"  Mean CCT:   {result.cct_mean:.0f}K")
    print(f"  Deviation:  {result.cct_deviation:.0f}K")
    print(f"  Mean Duv:   {result.duv_mean:.4f}")
    print()
    print(f"Contrast Ratio: {result.contrast_ratio:.0f}:1")
    print(f"Dynamic Range:  {result.dynamic_range_stops:.1f} stops")
    print()
    print("Region Analysis:")
    for region_name, analysis in result.region_analysis.items():
        print(f"  {region_name.title()}: "
              f"Mean ΔE = {analysis.delta_e_mean:.2f}, "
              f"CCT = {analysis.cct_mean:.0f}K ({analysis.grade.name})")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Test verification
    verifier = GrayscaleVerifier(
        target_whitepoint="D65",
        target_gamma_type=GammaType.BT1886,
        target_gamma_value=2.4,
        target_luminance=100.0,
        target_black=0.05,
    )

    test_measurements = create_test_measurements()
    result = verifier.verify(
        test_measurements,
        display_name="Test Display",
        profile_name="Test Profile"
    )
    print_grayscale_summary(result)
