"""
Screen Uniformity Measurement and Compensation (Phase 3)

Measures luminance and chrominance uniformity across the screen using a grid
of measurement points, then generates per-pixel correction factors using
bilinear interpolation.  This allows LUT-based compensation for panel
non-uniformity (e.g., edge darkening, backlight hotspots, OLED vignetting).

Typical workflow:
    1. create_uniformity_measurement_plan() -> list of patch rects
    2. Display each patch, measure with colorimeter
    3. UniformityCompensation.from_measurements(data)
    4. get_correction_factor(x, y) for any screen position
"""

import math
import numpy as np
from dataclasses import dataclass, field
from typing import Dict, List, Tuple, Optional


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class UniformityGrid:
    """Measured uniformity data across the screen."""
    rows: int                    # Grid rows (e.g., 5)
    cols: int                    # Grid columns (e.g., 5)
    luminance: np.ndarray        # Measured luminance at each point (rows x cols)
    chrominance_x: np.ndarray    # Measured chromaticity x at each point
    chrominance_y: np.ndarray    # Measured chromaticity y at each point
    reference_luminance: float   # Center point luminance (reference)
    reference_x: float           # Center point chromaticity x
    reference_y: float           # Center point chromaticity y


# ---------------------------------------------------------------------------
# Uniformity compensation engine
# ---------------------------------------------------------------------------

class UniformityCompensation:
    """
    Computes per-position RGB correction factors from a measured
    ``UniformityGrid``.  The correction is designed so that, after
    multiplication, every screen position produces luminance and
    chromaticity matching the center reference point.
    """

    def __init__(self, grid: UniformityGrid):
        self.grid = grid
        # Pre-compute luminance and chrominance ratio maps for fast lookup.
        self._lum_ratio = np.where(
            grid.luminance > 0,
            grid.reference_luminance / grid.luminance,
            1.0,
        )
        self._dx = np.where(
            grid.luminance > 0,
            grid.reference_x - grid.chrominance_x,
            0.0,
        )
        self._dy = np.where(
            grid.luminance > 0,
            grid.reference_y - grid.chrominance_y,
            0.0,
        )

    # -----------------------------------------------------------------
    # Public API
    # -----------------------------------------------------------------

    def get_correction_factor(
        self, screen_x: float, screen_y: float
    ) -> Tuple[float, float, float]:
        """
        Get per-channel correction factor for a normalised screen position.

        Parameters
        ----------
        screen_x, screen_y : float
            Normalised screen coordinates in [0, 1].

        Returns
        -------
        (r_factor, g_factor, b_factor)
            Multiplicative correction to apply to each channel.
        """
        screen_x = max(0.0, min(1.0, screen_x))
        screen_y = max(0.0, min(1.0, screen_y))

        lum_ratio = self._bilinear(self._lum_ratio, screen_x, screen_y)
        dx = self._bilinear(self._dx, screen_x, screen_y)
        dy = self._bilinear(self._dy, screen_x, screen_y)

        # Convert chrominance deltas into approximate RGB gain adjustments.
        # Positive dx (need more x) -> boost red.
        # Positive dy (need more y) -> boost green.
        r_chrom = 1.0 + dx * 3.0 - dy * 0.5
        g_chrom = 1.0 + dy * 2.5
        b_chrom = 1.0 - dx * 2.0 - dy * 1.5

        r_factor = float(np.clip(lum_ratio * r_chrom, 0.5, 2.0))
        g_factor = float(np.clip(lum_ratio * g_chrom, 0.5, 2.0))
        b_factor = float(np.clip(lum_ratio * b_chrom, 0.5, 2.0))

        return (r_factor, g_factor, b_factor)

    def compute_uniformity_stats(self) -> Dict:
        """
        Compute uniformity statistics from the grid.

        Returns
        -------
        dict with keys:
            max_deviation_pct   - worst-point luminance deviation from center (%)
            avg_deviation_pct   - average luminance deviation (%)
            worst_corner        - label of the worst corner (e.g. "top-left")
            worst_row           - row index of worst point
            worst_col           - col index of worst point
            luminance_range     - (min, max) measured luminance
            chrominance_spread  - max chrominance distance from reference
        """
        grid = self.grid
        ref = grid.reference_luminance

        if ref <= 0:
            return {
                "max_deviation_pct": 0.0,
                "avg_deviation_pct": 0.0,
                "worst_corner": "center",
                "worst_row": grid.rows // 2,
                "worst_col": grid.cols // 2,
                "luminance_range": (0.0, 0.0),
                "chrominance_spread": 0.0,
            }

        deviation_pct = np.abs(grid.luminance - ref) / ref * 100.0
        max_dev = float(np.max(deviation_pct))
        avg_dev = float(np.mean(deviation_pct))

        worst_idx = np.unravel_index(np.argmax(deviation_pct), deviation_pct.shape)
        worst_row, worst_col = int(worst_idx[0]), int(worst_idx[1])

        # Label the worst corner / edge
        worst_corner = _label_position(worst_row, worst_col, grid.rows, grid.cols)

        lum_min = float(np.min(grid.luminance))
        lum_max = float(np.max(grid.luminance))

        chrom_dist = np.sqrt(
            (grid.chrominance_x - grid.reference_x) ** 2
            + (grid.chrominance_y - grid.reference_y) ** 2
        )
        chrom_spread = float(np.max(chrom_dist))

        return {
            "max_deviation_pct": round(max_dev, 2),
            "avg_deviation_pct": round(avg_dev, 2),
            "worst_corner": worst_corner,
            "worst_row": worst_row,
            "worst_col": worst_col,
            "luminance_range": (round(lum_min, 2), round(lum_max, 2)),
            "chrominance_spread": round(chrom_spread, 5),
        }

    # -----------------------------------------------------------------
    # Factory
    # -----------------------------------------------------------------

    @classmethod
    def from_measurements(
        cls,
        measurements: List[Tuple[int, int, float, float, float]],
    ) -> "UniformityCompensation":
        """
        Create from a list of (row, col, luminance, x, y) measurements.

        The grid dimensions are inferred from the maximum row/col indices.
        The center point is used as the reference.
        """
        if not measurements:
            raise ValueError("measurements list must not be empty")

        max_row = max(m[0] for m in measurements)
        max_col = max(m[1] for m in measurements)
        rows = max_row + 1
        cols = max_col + 1

        luminance = np.zeros((rows, cols), dtype=np.float64)
        chrom_x = np.zeros((rows, cols), dtype=np.float64)
        chrom_y = np.zeros((rows, cols), dtype=np.float64)

        for row, col, lum, cx, cy in measurements:
            luminance[row, col] = lum
            chrom_x[row, col] = cx
            chrom_y[row, col] = cy

        center_r = rows // 2
        center_c = cols // 2

        grid = UniformityGrid(
            rows=rows,
            cols=cols,
            luminance=luminance,
            chrominance_x=chrom_x,
            chrominance_y=chrom_y,
            reference_luminance=float(luminance[center_r, center_c]),
            reference_x=float(chrom_x[center_r, center_c]),
            reference_y=float(chrom_y[center_r, center_c]),
        )
        return cls(grid)

    # -----------------------------------------------------------------
    # Internal helpers
    # -----------------------------------------------------------------

    def _bilinear(self, data: np.ndarray, norm_x: float, norm_y: float) -> float:
        """Bilinear interpolation on a (rows, cols) grid."""
        rows, cols = data.shape
        # Map normalised coords to grid indices (0..rows-1, 0..cols-1)
        gy = norm_y * (rows - 1)
        gx = norm_x * (cols - 1)

        r0 = int(math.floor(gy))
        c0 = int(math.floor(gx))
        r1 = min(r0 + 1, rows - 1)
        c1 = min(c0 + 1, cols - 1)

        fy = gy - r0
        fx = gx - c0

        val = (
            data[r0, c0] * (1 - fx) * (1 - fy)
            + data[r0, c1] * fx * (1 - fy)
            + data[r1, c0] * (1 - fx) * fy
            + data[r1, c1] * fx * fy
        )
        return float(val)


# ---------------------------------------------------------------------------
# Measurement plan
# ---------------------------------------------------------------------------

def create_uniformity_measurement_plan(
    rows: int = 5,
    cols: int = 5,
    display_width: int = 3840,
    display_height: int = 2160,
) -> List[Tuple[int, int, int, int]]:
    """
    Create a list of (x, y, width, height) rectangles for displaying
    measurement patches at each grid position.

    Each patch is approximately 10 % of the screen size, centered on the
    grid point.
    """
    patch_w = max(1, int(display_width * 0.10))
    patch_h = max(1, int(display_height * 0.10))

    patches: List[Tuple[int, int, int, int]] = []

    for r in range(rows):
        for c in range(cols):
            # Center of the grid cell
            cx = int((c + 0.5) / cols * display_width)
            cy = int((r + 0.5) / rows * display_height)

            # Top-left corner of the patch, clamped to screen
            x = max(0, min(cx - patch_w // 2, display_width - patch_w))
            y = max(0, min(cy - patch_h // 2, display_height - patch_h))

            patches.append((x, y, patch_w, patch_h))

    return patches


# ---------------------------------------------------------------------------
# Simulated data generator (for --simulated mode)
# ---------------------------------------------------------------------------

def generate_simulated_uniformity(
    rows: int = 5,
    cols: int = 5,
    center_luminance: float = 120.0,
    edge_falloff: float = 0.12,
    seed: int = 42,
) -> List[Tuple[int, int, float, float, float]]:
    """
    Generate realistic simulated uniformity data.

    Simulates typical OLED/LCD behaviour: brightest at center with gradual
    fall-off toward edges and corners, plus small random per-point noise
    and minor chrominance shifts.
    """
    rng = np.random.RandomState(seed)
    measurements: List[Tuple[int, int, float, float, float]] = []

    center_r = (rows - 1) / 2.0
    center_c = (cols - 1) / 2.0
    max_dist = math.sqrt(center_r ** 2 + center_c ** 2)

    ref_x, ref_y = 0.3127, 0.3290  # D65

    for r in range(rows):
        for c in range(cols):
            dist = math.sqrt((r - center_r) ** 2 + (c - center_c) ** 2)
            norm_dist = dist / max_dist if max_dist > 0 else 0.0

            # Luminance: smooth fall-off + noise
            falloff_factor = 1.0 - edge_falloff * (norm_dist ** 1.5)
            noise = rng.normal(0, 0.008)
            lum = center_luminance * max(0.5, falloff_factor + noise)

            # Chrominance: slight shift at edges
            cx = ref_x + rng.normal(0, 0.001) + norm_dist * 0.002
            cy = ref_y + rng.normal(0, 0.001) - norm_dist * 0.001

            measurements.append((r, c, round(lum, 2), round(cx, 5), round(cy, 5)))

    return measurements


# ---------------------------------------------------------------------------
# CLI command
# ---------------------------------------------------------------------------

def cmd_uniformity(args) -> int:
    """CLI handler for the ``uniformity`` subcommand."""
    from calibrate_pro import __version__

    print(f"\nCalibrate Pro v{__version__} - Screen Uniformity Analysis")
    print("=" * 60)

    rows = getattr(args, "rows", 5) or 5
    cols = getattr(args, "cols", 5) or 5
    width = getattr(args, "width", 3840) or 3840
    height = getattr(args, "height", 2160) or 2160
    simulated = getattr(args, "simulated", False)

    # --- Step 1: Instructions ------------------------------------------------
    print("\nUniformity measurement requires displaying a white patch at")
    print(f"each point of a {rows}x{cols} grid and recording the luminance")
    print("and chromaticity with a colorimeter.\n")

    plan = create_uniformity_measurement_plan(rows, cols, width, height)
    print(f"Measurement plan: {len(plan)} patches on a {width}x{height} display")
    print(f"Patch size: {plan[0][2]}x{plan[0][3]} pixels\n")

    # --- Step 2: Acquire data ------------------------------------------------
    if simulated:
        print("[Simulated mode] Generating synthetic uniformity data...\n")
        measurements = generate_simulated_uniformity(rows, cols)
    else:
        print("Connect a colorimeter and use --simulated for testing without one.")
        print("Full measurement mode is not yet implemented in CLI.")
        print("Use --simulated to see a demonstration.\n")
        return 0

    # --- Step 3: Compute stats -----------------------------------------------
    comp = UniformityCompensation.from_measurements(measurements)
    stats = comp.compute_uniformity_stats()

    print("--- Uniformity Statistics ---")
    print(f"  Max deviation:  {stats['max_deviation_pct']:.1f}%")
    print(f"  Avg deviation:  {stats['avg_deviation_pct']:.1f}%")
    print(f"  Worst area:     {stats['worst_corner']} "
          f"(row {stats['worst_row']}, col {stats['worst_col']})")
    lmin, lmax = stats["luminance_range"]
    print(f"  Luminance range: {lmin:.1f} - {lmax:.1f} cd/m2")
    print(f"  Chrominance spread: {stats['chrominance_spread']:.5f}")

    # --- Step 4: Report worst areas ------------------------------------------
    print("\n--- Luminance Map (cd/m2) ---")
    grid = comp.grid
    for r in range(grid.rows):
        row_vals = "  ".join(f"{grid.luminance[r, c]:6.1f}" for c in range(grid.cols))
        print(f"  Row {r}: {row_vals}")

    print("\n--- Correction Factors (center of each cell) ---")
    for r in range(grid.rows):
        factors = []
        for c in range(grid.cols):
            nx = (c + 0.5) / grid.cols
            ny = (r + 0.5) / grid.rows
            rf, gf, bf = comp.get_correction_factor(nx, ny)
            factors.append(f"({rf:.3f},{gf:.3f},{bf:.3f})")
        print(f"  Row {r}: {'  '.join(factors)}")

    # Summary
    if stats["max_deviation_pct"] < 5:
        grade = "Excellent"
    elif stats["max_deviation_pct"] < 10:
        grade = "Good"
    elif stats["max_deviation_pct"] < 20:
        grade = "Fair"
    else:
        grade = "Poor"

    print(f"\nUniformity Grade: {grade}")
    print(f"Max Deviation: {stats['max_deviation_pct']:.1f}%  "
          f"(worst: {stats['worst_corner']})")
    print("=" * 60)

    return 0


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _label_position(row: int, col: int, rows: int, cols: int) -> str:
    """Return a human-friendly label for a grid position."""
    if rows <= 1 and cols <= 1:
        return "center"

    v = "top" if row <= rows // 4 else ("bottom" if row >= rows - 1 - rows // 4 else "middle")
    h = "left" if col <= cols // 4 else ("right" if col >= cols - 1 - cols // 4 else "center")

    if v == "middle" and h == "center":
        return "center"
    if v == "middle":
        return h
    if h == "center":
        return v
    return f"{v}-{h}"
