"""
Uniformity Compensation Module

Provides display uniformity measurement and correction:
- 5x5 and 9x9 uniformity grids
- Per-region correction LUTs
- Edge fall-off compensation
- Luminance and color uniformity analysis
- Uniformity-corrected 3D LUT generation
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional, Callable
import numpy as np

# Optional scipy import
try:
    from scipy.interpolate import RegularGridInterpolator, RectBivariateSpline
    SCIPY_AVAILABLE = True
except ImportError:
    SCIPY_AVAILABLE = False
    RegularGridInterpolator = None
    RectBivariateSpline = None

# =============================================================================
# Enums
# =============================================================================

class UniformityGrid(Enum):
    """Uniformity measurement grid sizes."""
    GRID_3X3 = (3, 3)
    GRID_5X5 = (5, 5)
    GRID_7X7 = (7, 7)
    GRID_9X9 = (9, 9)
    GRID_11X11 = (11, 11)


class UniformityGrade(Enum):
    """Uniformity quality grade."""
    REFERENCE = auto()      # <2% deviation
    EXCELLENT = auto()      # <5% deviation
    GOOD = auto()           # <10% deviation
    ACCEPTABLE = auto()     # <15% deviation
    POOR = auto()           # >=15% deviation


class CompensationMode(Enum):
    """Uniformity compensation mode."""
    LUMINANCE_ONLY = auto()     # Only correct luminance
    COLOR_ONLY = auto()         # Only correct color (xy chromaticity)
    FULL = auto()               # Correct both luminance and color


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class UniformityMeasurement:
    """Single uniformity measurement point."""
    grid_x: int             # Grid column (0-indexed)
    grid_y: int             # Grid row (0-indexed)
    screen_x: float         # Normalized screen X (0-1)
    screen_y: float         # Normalized screen Y (0-1)

    # Measured values
    luminance: float        # cd/m² (nits)
    chromaticity_x: float   # CIE xy x
    chromaticity_y: float   # CIE xy y
    xyz: tuple[float, float, float] = (0, 0, 0)

    # Deviation from center/reference
    luminance_deviation: float = 0.0    # Percentage deviation
    delta_uv: float = 0.0               # Chromaticity deviation


@dataclass
class UniformityRegion:
    """Analysis for a screen region."""
    region_name: str        # e.g., "top-left", "center", "bottom-right"
    grid_positions: list[tuple[int, int]]

    luminance_mean: float
    luminance_min: float
    luminance_max: float
    luminance_deviation: float

    chromaticity_x_mean: float
    chromaticity_y_mean: float
    delta_uv_mean: float
    delta_uv_max: float

    grade: UniformityGrade


@dataclass
class UniformityResult:
    """Complete uniformity measurement result."""
    grid_size: UniformityGrid
    measurements: list[UniformityMeasurement]
    measurement_grid: np.ndarray  # 2D array of measurements

    # Reference point (usually center)
    reference_luminance: float
    reference_x: float
    reference_y: float

    # Overall statistics
    luminance_mean: float
    luminance_min: float
    luminance_max: float
    luminance_uniformity: float     # Percentage (100 = perfect)

    chromaticity_uniformity: float  # Based on max delta_uv
    delta_uv_mean: float
    delta_uv_max: float

    # Regional analysis
    center_grade: UniformityGrade
    corner_grade: UniformityGrade
    edge_grade: UniformityGrade
    overall_grade: UniformityGrade

    region_analysis: dict[str, UniformityRegion] = field(default_factory=dict)

    # Edge fall-off analysis
    left_falloff: float = 0.0       # Percentage drop at left edge
    right_falloff: float = 0.0
    top_falloff: float = 0.0
    bottom_falloff: float = 0.0

    # Metadata
    timestamp: str = ""
    display_name: str = ""


@dataclass
class UniformityCorrectionLUT:
    """Per-region uniformity correction LUT."""
    grid_size: UniformityGrid
    correction_grid: np.ndarray     # 2D array of correction factors

    # Luminance correction (multiplicative)
    luminance_corrections: np.ndarray

    # Chromaticity correction (additive in xy)
    chromaticity_x_corrections: np.ndarray
    chromaticity_y_corrections: np.ndarray

    # Interpolation functions for sub-pixel correction
    luminance_interpolator: Optional[Callable] = None
    chromaticity_x_interpolator: Optional[Callable] = None
    chromaticity_y_interpolator: Optional[Callable] = None

    # Applied compensation mode
    mode: CompensationMode = CompensationMode.FULL


# =============================================================================
# Grade Functions
# =============================================================================

def grade_from_uniformity(deviation: float) -> UniformityGrade:
    """Determine grade from uniformity deviation percentage."""
    if deviation < 2:
        return UniformityGrade.REFERENCE
    elif deviation < 5:
        return UniformityGrade.EXCELLENT
    elif deviation < 10:
        return UniformityGrade.GOOD
    elif deviation < 15:
        return UniformityGrade.ACCEPTABLE
    else:
        return UniformityGrade.POOR


def grade_to_string(grade: UniformityGrade) -> str:
    """Convert grade enum to display string."""
    return {
        UniformityGrade.REFERENCE: "Reference (<2%)",
        UniformityGrade.EXCELLENT: "Excellent (<5%)",
        UniformityGrade.GOOD: "Good (<10%)",
        UniformityGrade.ACCEPTABLE: "Acceptable (<15%)",
        UniformityGrade.POOR: "Poor (≥15%)",
    }[grade]


# =============================================================================
# Utility Functions
# =============================================================================

def xy_to_uv(x: float, y: float) -> tuple[float, float]:
    """Convert CIE xy to CIE u'v'."""
    denom = -2 * x + 12 * y + 3
    if denom <= 0:
        return (0.1978, 0.4683)
    u = 4 * x / denom
    v = 9 * y / denom
    return (u, v)


def delta_uv(x1: float, y1: float, x2: float, y2: float) -> float:
    """Calculate Δu'v' between two chromaticities."""
    u1, v1 = xy_to_uv(x1, y1)
    u2, v2 = xy_to_uv(x2, y2)
    return np.sqrt((u2 - u1)**2 + (v2 - v1)**2)


def generate_grid_positions(grid_size: UniformityGrid,
                           screen_width: int = 1920,
                           screen_height: int = 1080,
                           margin: float = 0.05) -> list[tuple[int, int, float, float]]:
    """
    Generate measurement positions for uniformity grid.

    Args:
        grid_size: Grid size enum
        screen_width: Display width in pixels
        screen_height: Display height in pixels
        margin: Margin from edge as fraction (0-0.5)

    Returns:
        List of (grid_x, grid_y, screen_x_norm, screen_y_norm)
    """
    cols, rows = grid_size.value
    positions = []

    # Calculate usable area
    x_start = margin
    x_end = 1.0 - margin
    y_start = margin
    y_end = 1.0 - margin

    for row in range(rows):
        for col in range(cols):
            # Normalized position (0-1)
            if cols > 1:
                x_norm = x_start + (x_end - x_start) * col / (cols - 1)
            else:
                x_norm = 0.5

            if rows > 1:
                y_norm = y_start + (y_end - y_start) * row / (rows - 1)
            else:
                y_norm = 0.5

            positions.append((col, row, x_norm, y_norm))

    return positions


def get_region_name(col: int, row: int, cols: int, rows: int) -> str:
    """Determine region name from grid position."""
    # Determine horizontal position
    if col < cols / 3:
        h_pos = "left"
    elif col >= 2 * cols / 3:
        h_pos = "right"
    else:
        h_pos = "center"

    # Determine vertical position
    if row < rows / 3:
        v_pos = "top"
    elif row >= 2 * rows / 3:
        v_pos = "bottom"
    else:
        v_pos = "middle"

    if h_pos == "center" and v_pos == "middle":
        return "center"
    elif h_pos == "center":
        return v_pos
    elif v_pos == "middle":
        return h_pos
    else:
        return f"{v_pos}-{h_pos}"


# =============================================================================
# Uniformity Analyzer Class
# =============================================================================

class UniformityAnalyzer:
    """
    Display uniformity analysis engine.

    Measures and analyzes luminance and color uniformity across
    the display surface using configurable grid patterns.
    """

    REGION_NAMES = [
        "top-left", "top", "top-right",
        "left", "center", "right",
        "bottom-left", "bottom", "bottom-right"
    ]

    def __init__(self, grid_size: UniformityGrid = UniformityGrid.GRID_5X5):
        """
        Initialize analyzer.

        Args:
            grid_size: Measurement grid size
        """
        self.grid_size = grid_size
        self.cols, self.rows = grid_size.value

    def analyze(self,
                measurements: list[UniformityMeasurement],
                display_name: str = "") -> UniformityResult:
        """
        Analyze uniformity measurements.

        Args:
            measurements: List of uniformity measurements
            display_name: Name of analyzed display

        Returns:
            UniformityResult with complete analysis
        """
        from datetime import datetime

        # Build measurement grid
        measurement_grid = np.empty((self.rows, self.cols), dtype=object)
        for m in measurements:
            measurement_grid[m.grid_y, m.grid_x] = m

        # Find reference (center) measurement
        center_col = self.cols // 2
        center_row = self.rows // 2
        reference = measurement_grid[center_row, center_col]

        if reference is None:
            # Use first measurement as reference
            reference = measurements[0]

        ref_luminance = reference.luminance
        ref_x = reference.chromaticity_x
        ref_y = reference.chromaticity_y

        # Calculate deviations
        luminance_values = []
        for m in measurements:
            m.luminance_deviation = ((m.luminance - ref_luminance) / ref_luminance) * 100
            m.delta_uv = delta_uv(ref_x, ref_y, m.chromaticity_x, m.chromaticity_y)
            luminance_values.append(m.luminance)

        # Overall statistics
        lum_array = np.array(luminance_values)
        luminance_mean = float(np.mean(lum_array))
        luminance_min = float(np.min(lum_array))
        luminance_max = float(np.max(lum_array))

        # Uniformity percentage (VESA standard)
        luminance_uniformity = (luminance_min / luminance_max) * 100

        # Chromaticity uniformity
        delta_uv_values = [m.delta_uv for m in measurements]
        delta_uv_mean = float(np.mean(delta_uv_values))
        delta_uv_max = float(np.max(delta_uv_values))
        chromaticity_uniformity = max(0, 100 - delta_uv_max * 1000)  # Scale for display

        # Regional analysis
        region_analysis = self._analyze_regions(measurements, ref_luminance, ref_x, ref_y)

        # Grade regions
        center_region = region_analysis.get("center")
        center_grade = center_region.grade if center_region else UniformityGrade.GOOD

        corner_regions = ["top-left", "top-right", "bottom-left", "bottom-right"]
        corner_deviations = [
            region_analysis[r].luminance_deviation
            for r in corner_regions if r in region_analysis
        ]
        corner_grade = grade_from_uniformity(
            max(abs(d) for d in corner_deviations) if corner_deviations else 0
        )

        edge_regions = ["top", "bottom", "left", "right"]
        edge_deviations = [
            region_analysis[r].luminance_deviation
            for r in edge_regions if r in region_analysis
        ]
        edge_grade = grade_from_uniformity(
            max(abs(d) for d in edge_deviations) if edge_deviations else 0
        )

        # Overall grade (based on worst uniformity)
        max_deviation = max(abs(m.luminance_deviation) for m in measurements)
        overall_grade = grade_from_uniformity(max_deviation)

        # Edge fall-off analysis
        left_falloff, right_falloff = self._calculate_horizontal_falloff(measurement_grid)
        top_falloff, bottom_falloff = self._calculate_vertical_falloff(measurement_grid)

        return UniformityResult(
            grid_size=self.grid_size,
            measurements=measurements,
            measurement_grid=measurement_grid,
            reference_luminance=ref_luminance,
            reference_x=ref_x,
            reference_y=ref_y,
            luminance_mean=luminance_mean,
            luminance_min=luminance_min,
            luminance_max=luminance_max,
            luminance_uniformity=luminance_uniformity,
            chromaticity_uniformity=chromaticity_uniformity,
            delta_uv_mean=delta_uv_mean,
            delta_uv_max=delta_uv_max,
            center_grade=center_grade,
            corner_grade=corner_grade,
            edge_grade=edge_grade,
            overall_grade=overall_grade,
            region_analysis=region_analysis,
            left_falloff=left_falloff,
            right_falloff=right_falloff,
            top_falloff=top_falloff,
            bottom_falloff=bottom_falloff,
            timestamp=datetime.now().isoformat(),
            display_name=display_name,
        )

    def _analyze_regions(self,
                        measurements: list[UniformityMeasurement],
                        ref_lum: float,
                        ref_x: float,
                        ref_y: float) -> dict[str, UniformityRegion]:
        """Analyze measurements by screen region."""
        regions: dict[str, list[UniformityMeasurement]] = {
            name: [] for name in self.REGION_NAMES
        }

        # Assign measurements to regions
        for m in measurements:
            region = get_region_name(m.grid_x, m.grid_y, self.cols, self.rows)
            if region in regions:
                regions[region].append(m)

        # Analyze each region
        analysis: dict[str, UniformityRegion] = {}

        for region_name, region_measurements in regions.items():
            if not region_measurements:
                continue

            lum_values = [m.luminance for m in region_measurements]
            x_values = [m.chromaticity_x for m in region_measurements]
            y_values = [m.chromaticity_y for m in region_measurements]
            duv_values = [m.delta_uv for m in region_measurements]

            lum_mean = float(np.mean(lum_values))
            lum_deviation = ((lum_mean - ref_lum) / ref_lum) * 100

            analysis[region_name] = UniformityRegion(
                region_name=region_name,
                grid_positions=[(m.grid_x, m.grid_y) for m in region_measurements],
                luminance_mean=lum_mean,
                luminance_min=float(np.min(lum_values)),
                luminance_max=float(np.max(lum_values)),
                luminance_deviation=lum_deviation,
                chromaticity_x_mean=float(np.mean(x_values)),
                chromaticity_y_mean=float(np.mean(y_values)),
                delta_uv_mean=float(np.mean(duv_values)),
                delta_uv_max=float(np.max(duv_values)),
                grade=grade_from_uniformity(abs(lum_deviation)),
            )

        return analysis

    def _calculate_horizontal_falloff(self, grid: np.ndarray) -> tuple[float, float]:
        """Calculate left and right edge luminance fall-off."""
        center_col = self.cols // 2

        # Get center column luminances
        center_lums = []
        for row in range(self.rows):
            if grid[row, center_col] is not None:
                center_lums.append(grid[row, center_col].luminance)
        center_mean = np.mean(center_lums) if center_lums else 1.0

        # Left edge
        left_lums = []
        for row in range(self.rows):
            if grid[row, 0] is not None:
                left_lums.append(grid[row, 0].luminance)
        left_mean = np.mean(left_lums) if left_lums else center_mean
        left_falloff = ((center_mean - left_mean) / center_mean) * 100

        # Right edge
        right_lums = []
        for row in range(self.rows):
            if grid[row, -1] is not None:
                right_lums.append(grid[row, -1].luminance)
        right_mean = np.mean(right_lums) if right_lums else center_mean
        right_falloff = ((center_mean - right_mean) / center_mean) * 100

        return (left_falloff, right_falloff)

    def _calculate_vertical_falloff(self, grid: np.ndarray) -> tuple[float, float]:
        """Calculate top and bottom edge luminance fall-off."""
        center_row = self.rows // 2

        # Get center row luminances
        center_lums = []
        for col in range(self.cols):
            if grid[center_row, col] is not None:
                center_lums.append(grid[center_row, col].luminance)
        center_mean = np.mean(center_lums) if center_lums else 1.0

        # Top edge
        top_lums = []
        for col in range(self.cols):
            if grid[0, col] is not None:
                top_lums.append(grid[0, col].luminance)
        top_mean = np.mean(top_lums) if top_lums else center_mean
        top_falloff = ((center_mean - top_mean) / center_mean) * 100

        # Bottom edge
        bottom_lums = []
        for col in range(self.cols):
            if grid[-1, col] is not None:
                bottom_lums.append(grid[-1, col].luminance)
        bottom_mean = np.mean(bottom_lums) if bottom_lums else center_mean
        bottom_falloff = ((center_mean - bottom_mean) / center_mean) * 100

        return (top_falloff, bottom_falloff)


# =============================================================================
# Uniformity Compensation Class
# =============================================================================

class UniformityCompensator:
    """
    Generates uniformity correction LUTs.

    Creates per-region correction factors to compensate for
    luminance and color non-uniformity across the display.
    """

    def __init__(self, mode: CompensationMode = CompensationMode.FULL):
        """
        Initialize compensator.

        Args:
            mode: Type of compensation to apply
        """
        self.mode = mode

    def generate_correction_lut(self,
                                result: UniformityResult,
                                target_luminance: Optional[float] = None) -> UniformityCorrectionLUT:
        """
        Generate uniformity correction LUT from measurements.

        Args:
            result: Uniformity measurement result
            target_luminance: Target luminance (uses reference if None)

        Returns:
            UniformityCorrectionLUT with correction factors
        """
        cols, rows = result.grid_size.value
        target_lum = target_luminance or result.reference_luminance

        # Initialize correction arrays
        lum_corrections = np.ones((rows, cols), dtype=np.float64)
        x_corrections = np.zeros((rows, cols), dtype=np.float64)
        y_corrections = np.zeros((rows, cols), dtype=np.float64)

        # Calculate corrections for each measurement point
        for m in result.measurements:
            row, col = m.grid_y, m.grid_x

            # Luminance correction (multiplicative factor to reach target)
            if m.luminance > 0:
                lum_corrections[row, col] = target_lum / m.luminance
            else:
                lum_corrections[row, col] = 1.0

            # Chromaticity correction (additive to reach reference)
            x_corrections[row, col] = result.reference_x - m.chromaticity_x
            y_corrections[row, col] = result.reference_y - m.chromaticity_y

        # Clamp luminance corrections to reasonable range
        lum_corrections = np.clip(lum_corrections, 0.5, 2.0)

        # Create interpolators for sub-pixel correction
        x_grid = np.linspace(0, 1, cols)
        y_grid = np.linspace(0, 1, rows)

        lum_interp = RectBivariateSpline(y_grid, x_grid, lum_corrections, kx=3, ky=3)
        x_interp = RectBivariateSpline(y_grid, x_grid, x_corrections, kx=3, ky=3)
        y_interp = RectBivariateSpline(y_grid, x_grid, y_corrections, kx=3, ky=3)

        return UniformityCorrectionLUT(
            grid_size=result.grid_size,
            correction_grid=lum_corrections,
            luminance_corrections=lum_corrections,
            chromaticity_x_corrections=x_corrections,
            chromaticity_y_corrections=y_corrections,
            luminance_interpolator=lambda x, y: float(lum_interp(y, x)[0, 0]),
            chromaticity_x_interpolator=lambda x, y: float(x_interp(y, x)[0, 0]),
            chromaticity_y_interpolator=lambda x, y: float(y_interp(y, x)[0, 0]),
            mode=self.mode,
        )

    def apply_correction(self,
                        lut: UniformityCorrectionLUT,
                        rgb: tuple[float, float, float],
                        screen_x: float,
                        screen_y: float) -> tuple[float, float, float]:
        """
        Apply uniformity correction to RGB value.

        Args:
            lut: Uniformity correction LUT
            rgb: Input RGB values (0-1)
            screen_x: Normalized screen X position (0-1)
            screen_y: Normalized screen Y position (0-1)

        Returns:
            Corrected RGB values
        """
        r, g, b = rgb

        if lut.mode in (CompensationMode.LUMINANCE_ONLY, CompensationMode.FULL):
            # Apply luminance correction
            if lut.luminance_interpolator:
                lum_factor = lut.luminance_interpolator(screen_x, screen_y)
            else:
                lum_factor = 1.0

            # Scale RGB by luminance factor
            r *= lum_factor
            g *= lum_factor
            b *= lum_factor

        # Note: Color correction would require full colorimetric model
        # This is a simplified implementation

        # Clamp to valid range
        r = max(0, min(1, r))
        g = max(0, min(1, g))
        b = max(0, min(1, b))

        return (r, g, b)

    def generate_3d_lut_with_uniformity(self,
                                        lut: UniformityCorrectionLUT,
                                        base_lut: Optional[np.ndarray] = None,
                                        lut_size: int = 17,
                                        screen_regions: int = 9) -> dict[str, np.ndarray]:
        """
        Generate per-region 3D LUTs with uniformity correction.

        Creates multiple 3D LUTs for different screen regions,
        each incorporating the appropriate uniformity correction.

        Args:
            lut: Uniformity correction LUT
            base_lut: Base calibration 3D LUT (identity if None)
            lut_size: Size of 3D LUT cube
            screen_regions: Number of regions (9 = 3x3 grid)

        Returns:
            Dictionary mapping region names to 3D LUT arrays
        """
        # Generate base LUT if not provided
        if base_lut is None:
            base_lut = self._create_identity_lut(lut_size)

        region_luts: dict[str, np.ndarray] = {}

        # Define region centers for 3x3 grid
        region_centers = {
            "top-left": (0.167, 0.167),
            "top": (0.5, 0.167),
            "top-right": (0.833, 0.167),
            "left": (0.167, 0.5),
            "center": (0.5, 0.5),
            "right": (0.833, 0.5),
            "bottom-left": (0.167, 0.833),
            "bottom": (0.5, 0.833),
            "bottom-right": (0.833, 0.833),
        }

        for region_name, (cx, cy) in region_centers.items():
            # Create corrected LUT for this region
            corrected_lut = base_lut.copy()

            # Get correction factor for region center
            if lut.luminance_interpolator:
                lum_factor = lut.luminance_interpolator(cx, cy)
            else:
                lum_factor = 1.0

            # Apply correction to entire LUT
            corrected_lut = corrected_lut * lum_factor
            corrected_lut = np.clip(corrected_lut, 0, 1)

            region_luts[region_name] = corrected_lut

        return region_luts

    def _create_identity_lut(self, size: int) -> np.ndarray:
        """Create identity 3D LUT."""
        lut = np.zeros((size, size, size, 3), dtype=np.float64)

        for r in range(size):
            for g in range(size):
                for b in range(size):
                    lut[r, g, b] = [
                        r / (size - 1),
                        g / (size - 1),
                        b / (size - 1)
                    ]

        return lut


# =============================================================================
# Utility Functions
# =============================================================================

def create_test_measurements(grid_size: UniformityGrid = UniformityGrid.GRID_5X5,
                             center_luminance: float = 100.0,
                             edge_falloff: float = 0.15) -> list[UniformityMeasurement]:
    """Create simulated uniformity measurements for testing."""
    np.random.seed(42)
    measurements = []

    cols, rows = grid_size.value
    positions = generate_grid_positions(grid_size)

    center_x = 0.3127  # D65
    center_y = 0.3290

    for col, row, x_norm, y_norm in positions:
        # Distance from center
        dist = np.sqrt((x_norm - 0.5)**2 + (y_norm - 0.5)**2)

        # Luminance falls off towards edges
        lum = center_luminance * (1 - edge_falloff * dist * 2)
        lum += np.random.normal(0, center_luminance * 0.01)  # 1% noise

        # Slight color shift towards edges
        x = center_x + np.random.normal(0, 0.002) + dist * 0.005
        y = center_y + np.random.normal(0, 0.002) - dist * 0.003

        measurements.append(UniformityMeasurement(
            grid_x=col,
            grid_y=row,
            screen_x=x_norm,
            screen_y=y_norm,
            luminance=max(0, lum),
            chromaticity_x=x,
            chromaticity_y=y,
            xyz=(0, lum, 0),  # Simplified
        ))

    return measurements


def print_uniformity_summary(result: UniformityResult) -> None:
    """Print uniformity analysis summary to console."""
    print("\n" + "=" * 60)
    print("Uniformity Analysis Summary")
    print("=" * 60)
    print(f"Display: {result.display_name or 'Unknown'}")
    print(f"Grid Size: {result.grid_size.value[0]}x{result.grid_size.value[1]}")
    print(f"Timestamp: {result.timestamp}")
    print()
    print(f"Overall Grade: {grade_to_string(result.overall_grade)}")
    print()
    print("Luminance Statistics:")
    print(f"  Reference: {result.reference_luminance:.1f} cd/m²")
    print(f"  Mean:      {result.luminance_mean:.1f} cd/m²")
    print(f"  Min:       {result.luminance_min:.1f} cd/m²")
    print(f"  Max:       {result.luminance_max:.1f} cd/m²")
    print(f"  Uniformity: {result.luminance_uniformity:.1f}%")
    print()
    print("Chromaticity Statistics:")
    print(f"  Reference: ({result.reference_x:.4f}, {result.reference_y:.4f})")
    print(f"  Mean Δu'v': {result.delta_uv_mean:.4f}")
    print(f"  Max Δu'v':  {result.delta_uv_max:.4f}")
    print()
    print("Edge Fall-off:")
    print(f"  Left:   {result.left_falloff:+.1f}%")
    print(f"  Right:  {result.right_falloff:+.1f}%")
    print(f"  Top:    {result.top_falloff:+.1f}%")
    print(f"  Bottom: {result.bottom_falloff:+.1f}%")
    print()
    print("Region Grades:")
    print(f"  Center:  {grade_to_string(result.center_grade)}")
    print(f"  Edges:   {grade_to_string(result.edge_grade)}")
    print(f"  Corners: {grade_to_string(result.corner_grade)}")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Test uniformity analysis
    analyzer = UniformityAnalyzer(UniformityGrid.GRID_5X5)
    test_measurements = create_test_measurements(
        UniformityGrid.GRID_5X5,
        center_luminance=100.0,
        edge_falloff=0.12
    )

    result = analyzer.analyze(test_measurements, "Test Display")
    print_uniformity_summary(result)

    # Test correction LUT generation
    compensator = UniformityCompensator(CompensationMode.FULL)
    correction_lut = compensator.generate_correction_lut(result)

    print("\nCorrection LUT generated:")
    print(f"  Grid size: {correction_lut.grid_size.value}")
    print(f"  Mode: {correction_lut.mode.name}")
    print(f"  Luminance range: {correction_lut.luminance_corrections.min():.3f} - "
          f"{correction_lut.luminance_corrections.max():.3f}")
