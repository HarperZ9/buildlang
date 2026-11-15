"""
Gamut Volume Analysis Module

Provides comprehensive gamut coverage and volume analysis:
- sRGB, DCI-P3, BT.2020, Adobe RGB coverage calculation
- 3D gamut volume comparison
- Gamut boundary detection
- Out-of-gamut analysis
- Color space intersection/union calculations
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional
import numpy as np

# Optional scipy import
try:
    from scipy.spatial import ConvexHull, Delaunay
    SCIPY_AVAILABLE = True
except ImportError:
    SCIPY_AVAILABLE = False
    ConvexHull = None
    Delaunay = None

# =============================================================================
# Enums
# =============================================================================

class ColorSpace(Enum):
    """Standard color space definitions."""
    SRGB = auto()           # IEC 61966-2-1 sRGB
    DCI_P3 = auto()         # DCI-P3 (D65 white)
    DISPLAY_P3 = auto()     # Apple Display P3
    BT2020 = auto()         # ITU-R BT.2020
    ADOBE_RGB = auto()       # Adobe RGB (1998)
    PROPHOTO_RGB = auto()   # ProPhoto RGB (ROMM)
    ACES_AP0 = auto()       # ACES Primaries 0
    ACES_AP1 = auto()       # ACES Primaries 1 (ACEScg)


class GamutGrade(Enum):
    """Gamut coverage quality grade."""
    REFERENCE = auto()      # >99% coverage
    EXCELLENT = auto()      # >95% coverage
    GOOD = auto()           # >90% coverage
    ACCEPTABLE = auto()     # >80% coverage
    POOR = auto()           # <80% coverage


# =============================================================================
# Color Space Primaries (CIE xy chromaticity)
# =============================================================================

# Primaries defined as (R_xy, G_xy, B_xy, W_xy)
COLORSPACE_PRIMARIES: dict[ColorSpace, dict[str, tuple[float, float]]] = {
    ColorSpace.SRGB: {
        "R": (0.6400, 0.3300),
        "G": (0.3000, 0.6000),
        "B": (0.1500, 0.0600),
        "W": (0.3127, 0.3290),  # D65
    },
    ColorSpace.DCI_P3: {
        "R": (0.6800, 0.3200),
        "G": (0.2650, 0.6900),
        "B": (0.1500, 0.0600),
        "W": (0.3140, 0.3510),  # DCI white
    },
    ColorSpace.DISPLAY_P3: {
        "R": (0.6800, 0.3200),
        "G": (0.2650, 0.6900),
        "B": (0.1500, 0.0600),
        "W": (0.3127, 0.3290),  # D65
    },
    ColorSpace.BT2020: {
        "R": (0.7080, 0.2920),
        "G": (0.1700, 0.7970),
        "B": (0.1310, 0.0460),
        "W": (0.3127, 0.3290),  # D65
    },
    ColorSpace.ADOBE_RGB: {
        "R": (0.6400, 0.3300),
        "G": (0.2100, 0.7100),
        "B": (0.1500, 0.0600),
        "W": (0.3127, 0.3290),  # D65
    },
    ColorSpace.PROPHOTO_RGB: {
        "R": (0.7347, 0.2653),
        "G": (0.1596, 0.8404),
        "B": (0.0366, 0.0001),
        "W": (0.3457, 0.3585),  # D50
    },
    ColorSpace.ACES_AP0: {
        "R": (0.7347, 0.2653),
        "G": (0.0000, 1.0000),
        "B": (0.0001, -0.0770),
        "W": (0.32168, 0.33767),  # ACES white
    },
    ColorSpace.ACES_AP1: {
        "R": (0.7130, 0.2930),
        "G": (0.1650, 0.8300),
        "B": (0.1280, 0.0440),
        "W": (0.32168, 0.33767),  # ACES white
    },
}


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class GamutPrimary:
    """Measured primary chromaticity."""
    name: str               # "R", "G", "B", "W"
    target_xy: tuple[float, float]
    measured_xy: tuple[float, float]
    delta_xy: float         # Distance in xy
    delta_uv: float         # Distance in u'v'


@dataclass
class GamutCoverage:
    """Coverage results for a single target color space."""
    color_space: ColorSpace
    coverage_percent: float  # Percentage of target gamut covered
    volume_ratio: float     # Measured volume / target volume
    exceeds_percent: float  # Percentage outside target (can exceed)

    # Primary accuracy
    primaries: list[GamutPrimary]
    primary_accuracy_mean: float  # Mean ΔE of primaries

    # Detailed metrics
    area_xy: float          # 2D area in xy chromaticity
    area_uv: float          # 2D area in u'v' chromaticity
    volume_lab: float       # 3D volume in Lab space

    grade: GamutGrade


@dataclass
class GamutBoundary:
    """Gamut boundary representation."""
    # 2D boundary in xy chromaticity
    boundary_xy: list[tuple[float, float]]

    # 2D boundary in u'v' chromaticity
    boundary_uv: list[tuple[float, float]]

    # 3D hull in Lab space
    hull_lab: Optional[ConvexHull] = None

    # Measured sample points used to construct boundary
    sample_points_lab: Optional[np.ndarray] = None


@dataclass
class OutOfGamutAnalysis:
    """Analysis of out-of-gamut colors."""
    target_space: ColorSpace
    total_samples: int
    in_gamut_count: int
    out_of_gamut_count: int
    out_of_gamut_percent: float

    # Severity metrics
    max_distance: float      # Maximum distance outside gamut
    mean_distance: float     # Mean distance for OOG samples
    severe_count: int        # Samples with significant OOG

    # Problem areas (in Lab)
    problem_regions: list[tuple[float, float, float]]


@dataclass
class GamutAnalysisResult:
    """Complete gamut analysis result."""
    # Measured display gamut
    measured_boundary: GamutBoundary
    measured_primaries: dict[str, tuple[float, float]]  # R, G, B, W xy

    # Coverage against standard spaces
    srgb_coverage: GamutCoverage
    p3_coverage: GamutCoverage
    bt2020_coverage: GamutCoverage
    adobe_rgb_coverage: GamutCoverage

    # Additional coverage results
    all_coverage: dict[ColorSpace, GamutCoverage]

    # Out-of-gamut analysis
    oog_analysis: dict[ColorSpace, OutOfGamutAnalysis]

    # Overall metrics
    total_volume_lab: float
    white_point_xy: tuple[float, float]
    white_point_cct: float
    white_point_duv: float

    # Metadata
    timestamp: str = ""
    display_name: str = ""
    profile_name: str = ""


# =============================================================================
# Color Conversion Functions
# =============================================================================

def xy_to_uv(x: float, y: float) -> tuple[float, float]:
    """Convert CIE 1931 xy to CIE 1976 u'v'."""
    denom = -2 * x + 12 * y + 3
    if denom <= 0:
        return (0.1978, 0.4683)  # D65 default
    u = 4 * x / denom
    v = 9 * y / denom
    return (u, v)


def uv_to_xy(u: float, v: float) -> tuple[float, float]:
    """Convert CIE 1976 u'v' to CIE 1931 xy."""
    denom = 6 * u - 16 * v + 12
    if abs(denom) < 1e-10:
        return (0.3127, 0.3290)  # D65 default
    x = 9 * u / denom
    y = 4 * v / denom
    return (x, y)


def xy_to_xyz(x: float, y: float, Y: float = 1.0) -> tuple[float, float, float]:
    """Convert xy chromaticity to XYZ (with given Y luminance)."""
    if y <= 0:
        return (0, 0, 0)
    X = (x / y) * Y
    Z = ((1 - x - y) / y) * Y
    return (X, Y, Z)


def xyz_to_lab(xyz: tuple[float, float, float],
               white_xyz: tuple[float, float, float] = (0.95047, 1.0, 1.08883)) -> tuple[float, float, float]:
    """Convert XYZ to Lab."""
    X, Y, Z = xyz
    Xn, Yn, Zn = white_xyz

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


def rgb_to_xyz(rgb: tuple[float, float, float],
               color_space: ColorSpace = ColorSpace.SRGB) -> tuple[float, float, float]:
    """
    Convert RGB to XYZ.

    Args:
        rgb: RGB values (0-1 range)
        color_space: Source color space

    Returns:
        XYZ values
    """
    r, g, b = rgb

    # Get primaries
    primaries = COLORSPACE_PRIMARIES[color_space]

    # Calculate RGB to XYZ matrix from primaries
    matrix = _calculate_rgb_to_xyz_matrix(primaries)

    # Apply matrix
    xyz = np.dot(matrix, [r, g, b])
    return tuple(xyz)


def _calculate_rgb_to_xyz_matrix(primaries: dict[str, tuple[float, float]]) -> np.ndarray:
    """Calculate RGB to XYZ conversion matrix from primaries."""
    # Get primaries
    xr, yr = primaries["R"]
    xg, yg = primaries["G"]
    xb, yb = primaries["B"]
    xw, yw = primaries["W"]

    # Calculate XYZ of primaries (with Y=1)
    Xr = xr / yr
    Yr = 1.0
    Zr = (1 - xr - yr) / yr

    Xg = xg / yg
    Yg = 1.0
    Zg = (1 - xg - yg) / yg

    Xb = xb / yb
    Yb = 1.0
    Zb = (1 - xb - yb) / yb

    # White point XYZ
    Xw = xw / yw
    Yw = 1.0
    Zw = (1 - xw - yw) / yw

    # Create matrix and solve for scaling factors
    M = np.array([
        [Xr, Xg, Xb],
        [Yr, Yg, Yb],
        [Zr, Zg, Zb]
    ])

    S = np.linalg.solve(M, [Xw, Yw, Zw])

    # Final matrix
    return np.array([
        [S[0] * Xr, S[1] * Xg, S[2] * Xb],
        [S[0] * Yr, S[1] * Yg, S[2] * Yb],
        [S[0] * Zr, S[1] * Zg, S[2] * Zb]
    ])


# =============================================================================
# Gamut Geometry Functions
# =============================================================================

def calculate_triangle_area(p1: tuple[float, float],
                           p2: tuple[float, float],
                           p3: tuple[float, float]) -> float:
    """Calculate area of triangle in 2D using cross product."""
    return 0.5 * abs(
        (p2[0] - p1[0]) * (p3[1] - p1[1]) -
        (p3[0] - p1[0]) * (p2[1] - p1[1])
    )


def calculate_gamut_area_xy(primaries: dict[str, tuple[float, float]]) -> float:
    """Calculate gamut area in xy chromaticity (triangle)."""
    return calculate_triangle_area(
        primaries["R"],
        primaries["G"],
        primaries["B"]
    )


def calculate_gamut_area_uv(primaries: dict[str, tuple[float, float]]) -> float:
    """Calculate gamut area in u'v' chromaticity."""
    r_uv = xy_to_uv(*primaries["R"])
    g_uv = xy_to_uv(*primaries["G"])
    b_uv = xy_to_uv(*primaries["B"])
    return calculate_triangle_area(r_uv, g_uv, b_uv)


def point_in_triangle(point: tuple[float, float],
                     v1: tuple[float, float],
                     v2: tuple[float, float],
                     v3: tuple[float, float]) -> bool:
    """Check if point is inside triangle using barycentric coordinates."""
    def sign(p1, p2, p3):
        return (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])

    d1 = sign(point, v1, v2)
    d2 = sign(point, v2, v3)
    d3 = sign(point, v3, v1)

    has_neg = (d1 < 0) or (d2 < 0) or (d3 < 0)
    has_pos = (d1 > 0) or (d2 > 0) or (d3 > 0)

    return not (has_neg and has_pos)


def calculate_triangle_intersection_area(t1: list[tuple[float, float]],
                                         t2: list[tuple[float, float]]) -> float:
    """
    Calculate intersection area of two triangles.

    Uses Sutherland-Hodgman polygon clipping algorithm.
    """
    from functools import reduce

    def clip_polygon(polygon: list[tuple[float, float]],
                    edge_start: tuple[float, float],
                    edge_end: tuple[float, float]) -> list[tuple[float, float]]:
        """Clip polygon against a single edge."""
        if not polygon:
            return []

        def inside(p):
            return (edge_end[0] - edge_start[0]) * (p[1] - edge_start[1]) > \
                   (edge_end[1] - edge_start[1]) * (p[0] - edge_start[0])

        def intersection(p1, p2):
            dc = (edge_start[0] - edge_end[0], edge_start[1] - edge_end[1])
            dp = (p1[0] - p2[0], p1[1] - p2[1])
            n1 = (edge_start[0] - p1[0]) * dc[1] - (edge_start[1] - p1[1]) * dc[0]
            n2 = dp[0] * dc[1] - dp[1] * dc[0]
            if abs(n2) < 1e-10:
                return p1
            t = n1 / n2
            return (p1[0] + t * dp[0], p1[1] + t * dp[1])

        result = []
        for i in range(len(polygon)):
            current = polygon[i]
            next_v = polygon[(i + 1) % len(polygon)]

            if inside(current):
                if inside(next_v):
                    result.append(next_v)
                else:
                    result.append(intersection(current, next_v))
            elif inside(next_v):
                result.append(intersection(current, next_v))
                result.append(next_v)

        return result

    # Start with first triangle as polygon
    clipped = list(t1)

    # Clip against each edge of second triangle
    for i in range(3):
        if not clipped:
            break
        clipped = clip_polygon(clipped, t2[i], t2[(i + 1) % 3])

    # Calculate area of resulting polygon
    if len(clipped) < 3:
        return 0.0

    # Shoelace formula for polygon area
    area = 0.0
    for i in range(len(clipped)):
        j = (i + 1) % len(clipped)
        area += clipped[i][0] * clipped[j][1]
        area -= clipped[j][0] * clipped[i][1]

    return abs(area) / 2.0


def calculate_gamut_coverage(measured_primaries: dict[str, tuple[float, float]],
                             target_space: ColorSpace) -> float:
    """
    Calculate percentage of target gamut covered by measured gamut.

    Args:
        measured_primaries: Measured R, G, B primaries in xy
        target_space: Target color space to compare against

    Returns:
        Coverage percentage (0-100)
    """
    target_primaries = COLORSPACE_PRIMARIES[target_space]

    # Get triangles
    measured_triangle = [
        measured_primaries["R"],
        measured_primaries["G"],
        measured_primaries["B"],
    ]
    target_triangle = [
        target_primaries["R"],
        target_primaries["G"],
        target_primaries["B"],
    ]

    # Calculate intersection area
    intersection_area = calculate_triangle_intersection_area(measured_triangle, target_triangle)

    # Calculate target area
    target_area = calculate_gamut_area_xy(target_primaries)

    if target_area <= 0:
        return 0.0

    coverage = (intersection_area / target_area) * 100
    return min(coverage, 100.0)


def calculate_gamut_exceeds(measured_primaries: dict[str, tuple[float, float]],
                            target_space: ColorSpace) -> float:
    """
    Calculate percentage of measured gamut that exceeds target gamut.

    Returns:
        Percentage of measured gamut outside target (can exceed 100%)
    """
    target_primaries = COLORSPACE_PRIMARIES[target_space]

    # Get triangles
    measured_triangle = [
        measured_primaries["R"],
        measured_primaries["G"],
        measured_primaries["B"],
    ]
    target_triangle = [
        target_primaries["R"],
        target_primaries["G"],
        target_primaries["B"],
    ]

    # Calculate areas
    measured_area = calculate_triangle_area(*measured_triangle)
    intersection_area = calculate_triangle_intersection_area(measured_triangle, target_triangle)

    if measured_area <= 0:
        return 0.0

    exceeds_area = measured_area - intersection_area
    return (exceeds_area / measured_area) * 100


# =============================================================================
# 3D Volume Calculation
# =============================================================================

def generate_gamut_samples(color_space: ColorSpace,
                          samples_per_axis: int = 17) -> np.ndarray:
    """
    Generate sample points throughout a color space gamut.

    Returns array of Lab values.
    """
    samples = []
    primaries = COLORSPACE_PRIMARIES[color_space]
    white_xy = primaries["W"]
    white_xyz = xy_to_xyz(white_xy[0], white_xy[1], 1.0)

    for r in np.linspace(0, 1, samples_per_axis):
        for g in np.linspace(0, 1, samples_per_axis):
            for b in np.linspace(0, 1, samples_per_axis):
                xyz = rgb_to_xyz((r, g, b), color_space)
                lab = xyz_to_lab(xyz, white_xyz)
                samples.append(lab)

    return np.array(samples)


def calculate_gamut_volume_lab(samples: np.ndarray) -> float:
    """
    Calculate gamut volume in Lab space using convex hull.

    Args:
        samples: Array of Lab values

    Returns:
        Volume in Lab³ units
    """
    try:
        hull = ConvexHull(samples)
        return hull.volume
    except Exception:
        return 0.0


def calculate_gamut_volume_ratio(measured_samples: np.ndarray,
                                 target_space: ColorSpace) -> float:
    """
    Calculate ratio of measured volume to target color space volume.

    Returns:
        Volume ratio (1.0 = same volume)
    """
    target_samples = generate_gamut_samples(target_space, 17)

    measured_volume = calculate_gamut_volume_lab(measured_samples)
    target_volume = calculate_gamut_volume_lab(target_samples)

    if target_volume <= 0:
        return 0.0

    return measured_volume / target_volume


# =============================================================================
# Grade Functions
# =============================================================================

def grade_from_coverage(coverage: float) -> GamutGrade:
    """Determine grade from coverage percentage."""
    if coverage >= 99:
        return GamutGrade.REFERENCE
    elif coverage >= 95:
        return GamutGrade.EXCELLENT
    elif coverage >= 90:
        return GamutGrade.GOOD
    elif coverage >= 80:
        return GamutGrade.ACCEPTABLE
    else:
        return GamutGrade.POOR


def grade_to_string(grade: GamutGrade) -> str:
    """Convert grade enum to display string."""
    return {
        GamutGrade.REFERENCE: "Reference (≥99%)",
        GamutGrade.EXCELLENT: "Excellent (≥95%)",
        GamutGrade.GOOD: "Good (≥90%)",
        GamutGrade.ACCEPTABLE: "Acceptable (≥80%)",
        GamutGrade.POOR: "Poor (<80%)",
    }[grade]


# =============================================================================
# Gamut Analyzer Class
# =============================================================================

class GamutAnalyzer:
    """
    Gamut analysis engine.

    Performs comprehensive gamut coverage and volume analysis
    comparing measured display to standard color spaces.
    """

    def __init__(self, reference_white: str = "D65"):
        """
        Initialize analyzer.

        Args:
            reference_white: Reference white point (D65, D50, etc.)
        """
        self.reference_white = reference_white

        # White point XYZ for Lab conversion
        white_points = {
            "D65": (0.95047, 1.0, 1.08883),
            "D50": (0.96422, 1.0, 0.82521),
        }
        self.white_xyz = white_points.get(reference_white, white_points["D65"])

    def analyze(self,
                measured_primaries: dict[str, tuple[float, float]],
                measured_samples: Optional[np.ndarray] = None,
                display_name: str = "",
                profile_name: str = "") -> GamutAnalysisResult:
        """
        Perform comprehensive gamut analysis.

        Args:
            measured_primaries: Measured R, G, B, W primaries in xy chromaticity
            measured_samples: Optional array of measured Lab samples for volume
            display_name: Name of analyzed display
            profile_name: Name of ICC profile

        Returns:
            GamutAnalysisResult with complete analysis
        """
        from datetime import datetime

        # Calculate coverage for each standard space
        all_coverage: dict[ColorSpace, GamutCoverage] = {}

        for color_space in [ColorSpace.SRGB, ColorSpace.DISPLAY_P3,
                           ColorSpace.BT2020, ColorSpace.ADOBE_RGB]:
            coverage = self._analyze_coverage(measured_primaries, color_space, measured_samples)
            all_coverage[color_space] = coverage

        # Create measured boundary
        measured_boundary = self._create_boundary(measured_primaries, measured_samples)

        # Out-of-gamut analysis
        oog_analysis: dict[ColorSpace, OutOfGamutAnalysis] = {}
        if measured_samples is not None:
            for color_space in [ColorSpace.SRGB, ColorSpace.DISPLAY_P3]:
                oog = self._analyze_out_of_gamut(measured_samples, color_space)
                oog_analysis[color_space] = oog

        # Calculate total volume
        total_volume = 0.0
        if measured_samples is not None:
            total_volume = calculate_gamut_volume_lab(measured_samples)

        # White point analysis
        white_xy = measured_primaries.get("W", (0.3127, 0.3290))
        white_cct, white_duv = self._calculate_cct(white_xy)

        return GamutAnalysisResult(
            measured_boundary=measured_boundary,
            measured_primaries=measured_primaries,
            srgb_coverage=all_coverage[ColorSpace.SRGB],
            p3_coverage=all_coverage[ColorSpace.DISPLAY_P3],
            bt2020_coverage=all_coverage[ColorSpace.BT2020],
            adobe_rgb_coverage=all_coverage[ColorSpace.ADOBE_RGB],
            all_coverage=all_coverage,
            oog_analysis=oog_analysis,
            total_volume_lab=total_volume,
            white_point_xy=white_xy,
            white_point_cct=white_cct,
            white_point_duv=white_duv,
            timestamp=datetime.now().isoformat(),
            display_name=display_name,
            profile_name=profile_name,
        )

    def _analyze_coverage(self,
                         measured_primaries: dict[str, tuple[float, float]],
                         target_space: ColorSpace,
                         measured_samples: Optional[np.ndarray]) -> GamutCoverage:
        """Analyze coverage against single target space."""
        target_primaries = COLORSPACE_PRIMARIES[target_space]

        # Calculate 2D coverage
        coverage_percent = calculate_gamut_coverage(measured_primaries, target_space)
        exceeds_percent = calculate_gamut_exceeds(measured_primaries, target_space)

        # Calculate areas
        area_xy = calculate_gamut_area_xy(measured_primaries)
        area_uv = calculate_gamut_area_uv(measured_primaries)

        # Calculate volume ratio
        volume_ratio = 1.0
        volume_lab = 0.0
        if measured_samples is not None:
            volume_ratio = calculate_gamut_volume_ratio(measured_samples, target_space)
            volume_lab = calculate_gamut_volume_lab(measured_samples)

        # Analyze primary accuracy
        primaries: list[GamutPrimary] = []
        delta_sum = 0.0

        for name in ["R", "G", "B"]:
            target_xy = target_primaries[name]
            measured_xy = measured_primaries.get(name, target_xy)

            delta_xy = np.sqrt(
                (measured_xy[0] - target_xy[0])**2 +
                (measured_xy[1] - target_xy[1])**2
            )

            target_uv = xy_to_uv(*target_xy)
            measured_uv = xy_to_uv(*measured_xy)
            delta_uv = np.sqrt(
                (measured_uv[0] - target_uv[0])**2 +
                (measured_uv[1] - target_uv[1])**2
            )

            primaries.append(GamutPrimary(
                name=name,
                target_xy=target_xy,
                measured_xy=measured_xy,
                delta_xy=delta_xy,
                delta_uv=delta_uv,
            ))
            delta_sum += delta_uv

        primary_accuracy_mean = delta_sum / 3

        return GamutCoverage(
            color_space=target_space,
            coverage_percent=coverage_percent,
            volume_ratio=volume_ratio,
            exceeds_percent=exceeds_percent,
            primaries=primaries,
            primary_accuracy_mean=primary_accuracy_mean,
            area_xy=area_xy,
            area_uv=area_uv,
            volume_lab=volume_lab,
            grade=grade_from_coverage(coverage_percent),
        )

    def _create_boundary(self,
                        primaries: dict[str, tuple[float, float]],
                        samples: Optional[np.ndarray]) -> GamutBoundary:
        """Create gamut boundary representation."""
        # 2D boundaries (triangles in xy and u'v')
        boundary_xy = [
            primaries["R"],
            primaries["G"],
            primaries["B"],
        ]

        boundary_uv = [
            xy_to_uv(*primaries["R"]),
            xy_to_uv(*primaries["G"]),
            xy_to_uv(*primaries["B"]),
        ]

        # 3D hull
        hull_lab = None
        if samples is not None and len(samples) >= 4:
            try:
                hull_lab = ConvexHull(samples)
            except Exception:
                pass

        return GamutBoundary(
            boundary_xy=boundary_xy,
            boundary_uv=boundary_uv,
            hull_lab=hull_lab,
            sample_points_lab=samples,
        )

    def _analyze_out_of_gamut(self,
                             samples: np.ndarray,
                             target_space: ColorSpace) -> OutOfGamutAnalysis:
        """Analyze out-of-gamut samples."""
        # Generate target gamut hull
        target_samples = generate_gamut_samples(target_space, 17)

        try:
            target_hull = Delaunay(target_samples)
        except Exception:
            return OutOfGamutAnalysis(
                target_space=target_space,
                total_samples=len(samples),
                in_gamut_count=len(samples),
                out_of_gamut_count=0,
                out_of_gamut_percent=0.0,
                max_distance=0.0,
                mean_distance=0.0,
                severe_count=0,
                problem_regions=[],
            )

        # Check each sample
        in_gamut = target_hull.find_simplex(samples) >= 0
        oog_mask = ~in_gamut

        in_gamut_count = int(np.sum(in_gamut))
        oog_count = int(np.sum(oog_mask))
        oog_percent = (oog_count / len(samples)) * 100

        # Calculate distances for OOG samples (simplified)
        oog_samples = samples[oog_mask]
        distances = []
        problem_regions = []

        for sample in oog_samples[:100]:  # Limit for performance
            # Simple distance to hull (approximation)
            # Find closest point on hull
            min_dist = float('inf')
            for vertex in target_hull.points[target_hull.convex_hull.flatten()]:
                dist = np.sqrt(np.sum((sample - vertex)**2))
                min_dist = min(min_dist, dist)
            distances.append(min_dist)

            if min_dist > 10:  # Severe OOG threshold
                problem_regions.append(tuple(sample))

        max_distance = max(distances) if distances else 0.0
        mean_distance = np.mean(distances) if distances else 0.0
        severe_count = len([d for d in distances if d > 10])

        return OutOfGamutAnalysis(
            target_space=target_space,
            total_samples=len(samples),
            in_gamut_count=in_gamut_count,
            out_of_gamut_count=oog_count,
            out_of_gamut_percent=oog_percent,
            max_distance=max_distance,
            mean_distance=mean_distance,
            severe_count=severe_count,
            problem_regions=problem_regions[:10],  # Limit to top 10
        )

    def _calculate_cct(self, xy: tuple[float, float]) -> tuple[float, float]:
        """Calculate CCT and Duv from xy chromaticity."""
        x, y = xy

        # McCamy's approximation
        n = (x - 0.3320) / (y - 0.1858) if y != 0.1858 else 0
        CCT = -449 * n**3 + 3525 * n**2 - 6823.3 * n + 5520.33

        # Simplified Duv
        u = 4 * x / (-2 * x + 12 * y + 3)
        v = 9 * y / (-2 * x + 12 * y + 3)

        # D65 reference
        u_d65, v_d65 = 0.1978, 0.4683
        duv = np.sqrt((u - u_d65)**2 + (v - v_d65)**2)

        if v < v_d65:
            duv = -duv

        return (CCT, duv)


# =============================================================================
# Utility Functions
# =============================================================================

def create_test_primaries(coverage_srgb: float = 0.95) -> dict[str, tuple[float, float]]:
    """Create simulated display primaries for testing."""
    np.random.seed(42)

    # Start with sRGB primaries and add slight variations
    srgb = COLORSPACE_PRIMARIES[ColorSpace.SRGB]

    # Scale primaries slightly to simulate different coverage
    scale = np.sqrt(coverage_srgb)

    # Centroid of sRGB triangle
    cx = (srgb["R"][0] + srgb["G"][0] + srgb["B"][0]) / 3
    cy = (srgb["R"][1] + srgb["G"][1] + srgb["B"][1]) / 3

    primaries = {}
    for name in ["R", "G", "B"]:
        x, y = srgb[name]
        # Scale from centroid
        new_x = cx + (x - cx) * scale + np.random.normal(0, 0.005)
        new_y = cy + (y - cy) * scale + np.random.normal(0, 0.005)
        primaries[name] = (new_x, new_y)

    # White point with slight error
    primaries["W"] = (
        0.3127 + np.random.normal(0, 0.002),
        0.3290 + np.random.normal(0, 0.002),
    )

    return primaries


def print_gamut_summary(result: GamutAnalysisResult) -> None:
    """Print gamut analysis summary to console."""
    print("\n" + "=" * 60)
    print("Gamut Analysis Summary")
    print("=" * 60)
    print(f"Display: {result.display_name or 'Unknown'}")
    print(f"Profile: {result.profile_name or 'Unknown'}")
    print(f"Timestamp: {result.timestamp}")
    print()
    print("Measured Primaries (xy):")
    for name, xy in result.measured_primaries.items():
        print(f"  {name}: ({xy[0]:.4f}, {xy[1]:.4f})")
    print()
    print(f"White Point: {result.white_point_cct:.0f}K, Duv = {result.white_point_duv:.4f}")
    print()
    print("Coverage Results:")
    print(f"  sRGB:      {result.srgb_coverage.coverage_percent:.1f}% - {grade_to_string(result.srgb_coverage.grade)}")
    print(f"  Display P3: {result.p3_coverage.coverage_percent:.1f}% - {grade_to_string(result.p3_coverage.grade)}")
    print(f"  BT.2020:   {result.bt2020_coverage.coverage_percent:.1f}% - {grade_to_string(result.bt2020_coverage.grade)}")
    print(f"  Adobe RGB: {result.adobe_rgb_coverage.coverage_percent:.1f}% - {grade_to_string(result.adobe_rgb_coverage.grade)}")
    print()
    if result.total_volume_lab > 0:
        print(f"Total Volume (Lab³): {result.total_volume_lab:.0f}")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Test analysis
    analyzer = GamutAnalyzer(reference_white="D65")

    test_primaries = create_test_primaries(0.98)
    test_samples = generate_gamut_samples(ColorSpace.SRGB, 9)

    result = analyzer.analyze(
        test_primaries,
        test_samples,
        display_name="Test Display",
        profile_name="Test Profile"
    )
    print_gamut_summary(result)
