"""
3D Color Volume Mapping

Moves beyond 2D gamut area (CIE xy triangles) to 3D color volume
that accounts for luminance-dependent gamut changes.

OLED panels lose saturation at high luminance due to efficiency
rolloff. A display might cover 97% of DCI-P3 at mid-brightness
but only 80% at peak brightness. 2D gamut area misses this entirely.

Color volume is computed in Oklch space (perceptually uniform,
lightness-dependent) by sampling the gamut boundary at multiple
lightness levels and computing the enclosed volume.
"""

import math
import numpy as np
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple


@dataclass
class ColorVolumeResult:
    """Results from 3D color volume analysis."""
    # Absolute volumes (arbitrary units, relative comparison only)
    panel_volume: float
    srgb_volume: float
    p3_volume: float
    bt2020_volume: float

    # Coverage percentages
    srgb_volume_pct: float    # Panel volume that covers sRGB
    p3_volume_pct: float      # Panel volume that covers DCI-P3
    bt2020_volume_pct: float  # Panel volume that covers BT.2020

    # Relative to sRGB
    relative_to_srgb_pct: float  # Panel volume / sRGB volume * 100

    # Per-lightness gamut area (shows how gamut changes with brightness)
    lightness_levels: List[float]       # L* or Jz values
    gamut_area_per_level: List[float]   # Area at each lightness


def compute_color_volume(
    panel_primaries: Tuple[Tuple[float, float], ...],
    panel_white: Tuple[float, float] = (0.3127, 0.3290),
    lightness_steps: int = 21,
    hue_steps: int = 72,
    panel_type: str = "",
    peak_luminance: float = 1000.0
) -> ColorVolumeResult:
    """
    Compute 3D color volume for a panel.

    Samples the gamut boundary at multiple lightness levels in
    CIE LCH space, computes the area at each level, and integrates
    to get the total volume.

    This captures luminance-dependent gamut changes that 2D gamut
    area measurements miss entirely.

    Args:
        panel_primaries: ((rx,ry), (gx,gy), (bx,by))
        panel_white: (wx, wy) white point
        lightness_steps: Number of L* levels to sample
        hue_steps: Number of hue angles per level
        panel_type: "QD-OLED", "WOLED", etc. for luminance rolloff
        peak_luminance: Panel peak luminance for rolloff modeling

    Returns:
        ColorVolumeResult with volumes and per-level areas
    """
    from calibrate_pro.core.color_math import primaries_to_xyz_matrix

    # Build XYZ-to-RGB matrices for each gamut
    def make_xyz_to_rgb(prims, white):
        m = primaries_to_xyz_matrix(prims[0], prims[1], prims[2], white)
        return np.linalg.inv(m)

    panel_xyz_to_rgb = make_xyz_to_rgb(panel_primaries, panel_white)

    srgb_prims = ((0.6400, 0.3300), (0.3000, 0.6000), (0.1500, 0.0600))
    p3_prims = ((0.6800, 0.3200), (0.2650, 0.6900), (0.1500, 0.0600))
    bt2020_prims = ((0.7080, 0.2920), (0.1700, 0.7970), (0.1310, 0.0460))
    d65 = (0.3127, 0.3290)

    srgb_xyz_to_rgb = make_xyz_to_rgb(srgb_prims, d65)
    p3_xyz_to_rgb = make_xyz_to_rgb(p3_prims, d65)
    bt2020_xyz_to_rgb = make_xyz_to_rgb(bt2020_prims, d65)

    # Reference white XYZ
    ref_xyz = np.array([0.95047, 1.0, 1.08883])

    lightness_levels = np.linspace(5, 95, lightness_steps)
    hue_angles = np.linspace(0, 360, hue_steps, endpoint=False)

    def find_max_chroma_at_lh(xyz_to_rgb, L, h_deg, rolloff_factor=1.0):
        """Binary search for max chroma in a gamut at given L, h."""
        h_rad = math.radians(h_deg)
        cos_h = math.cos(h_rad)
        sin_h = math.sin(h_rad)

        delta = 6.0 / 29.0
        delta_cu = delta ** 3

        lo, hi = 0.0, 180.0
        for _ in range(25):
            mid = (lo + hi) / 2.0
            a = mid * cos_h
            b = mid * sin_h

            # Lab to XYZ
            fy = (L + 16.0) / 116.0
            fx = a / 500.0 + fy
            fz = fy - b / 200.0

            xr = fx ** 3 if fx ** 3 > delta_cu else (fx - 4.0 / 29.0) * 3 * delta ** 2
            yr = fy ** 3 if fy ** 3 > delta_cu else (fy - 4.0 / 29.0) * 3 * delta ** 2
            zr = fz ** 3 if fz ** 3 > delta_cu else (fz - 4.0 / 29.0) * 3 * delta ** 2

            xyz = np.array([xr * ref_xyz[0], yr * ref_xyz[1], zr * ref_xyz[2]])
            rgb = xyz_to_rgb @ xyz

            if np.all(rgb >= -0.001) and np.all(rgb <= 1.001 * rolloff_factor):
                lo = mid
            else:
                hi = mid
        return lo

    def compute_area_at_lightness(xyz_to_rgb, L, rolloff=1.0):
        """Compute gamut area at a specific lightness level."""
        chromas = []
        for h in hue_angles:
            c = find_max_chroma_at_lh(xyz_to_rgb, L, h, rolloff)
            chromas.append(c)

        # Area of the polar polygon (chroma vs hue)
        area = 0.0
        n = len(chromas)
        d_theta = math.radians(360.0 / hue_steps)
        for i in range(n):
            # Sector area: 0.5 * r1 * r2 * sin(d_theta)
            r1 = chromas[i]
            r2 = chromas[(i + 1) % n]
            area += 0.5 * r1 * r2 * math.sin(d_theta)
        return area

    # Compute luminance rolloff for OLED panels
    def get_rolloff_factor(L, panel_type_str):
        """
        Model how gamut narrows at high luminance for OLED.
        At L=50 (mid), factor=1.0 (full gamut).
        At L=90+ (bright), factor reduces for OLED.
        """
        if "QD-OLED" in panel_type_str:
            # QD-OLED maintains gamut well, small rolloff
            if L > 70:
                return 1.0 - 0.05 * ((L - 70) / 30) ** 1.5
            return 1.0
        elif "WOLED" in panel_type_str:
            # WOLED loses more saturation at high luminance
            if L > 60:
                return 1.0 - 0.15 * ((L - 60) / 40) ** 1.5
            return 1.0
        return 1.0  # LCD: no significant rolloff

    # Compute volumes by integrating area over lightness
    panel_areas = []
    srgb_areas = []
    p3_areas = []
    bt2020_areas = []

    for L in lightness_levels:
        rolloff = get_rolloff_factor(L, panel_type)
        panel_areas.append(compute_area_at_lightness(panel_xyz_to_rgb, L, rolloff))
        srgb_areas.append(compute_area_at_lightness(srgb_xyz_to_rgb, L))
        p3_areas.append(compute_area_at_lightness(p3_xyz_to_rgb, L))
        bt2020_areas.append(compute_area_at_lightness(bt2020_xyz_to_rgb, L))

    # Integrate (trapezoidal rule) to get volumes
    dL = lightness_levels[1] - lightness_levels[0]
    panel_vol = float(np.trapezoid(panel_areas, lightness_levels))
    srgb_vol = float(np.trapezoid(srgb_areas, lightness_levels))
    p3_vol = float(np.trapezoid(p3_areas, lightness_levels))
    bt2020_vol = float(np.trapezoid(bt2020_areas, lightness_levels))

    # Coverage: approximate by comparing areas per level
    # (exact intersection would require polygon clipping at each level)
    srgb_coverage = 0.0
    p3_coverage = 0.0
    bt2020_coverage = 0.0
    for pa, sa, p3a, ba in zip(panel_areas, srgb_areas, p3_areas, bt2020_areas):
        srgb_coverage += min(pa, sa)
        p3_coverage += min(pa, p3a)
        bt2020_coverage += min(pa, ba)
    srgb_coverage_vol = float(np.trapezoid([min(pa, sa) for pa, sa in zip(panel_areas, srgb_areas)], lightness_levels))
    p3_coverage_vol = float(np.trapezoid([min(pa, p3a) for pa, p3a in zip(panel_areas, p3_areas)], lightness_levels))
    bt2020_coverage_vol = float(np.trapezoid([min(pa, ba) for pa, ba in zip(panel_areas, bt2020_areas)], lightness_levels))

    return ColorVolumeResult(
        panel_volume=panel_vol,
        srgb_volume=srgb_vol,
        p3_volume=p3_vol,
        bt2020_volume=bt2020_vol,
        srgb_volume_pct=min(100.0, srgb_coverage_vol / srgb_vol * 100) if srgb_vol > 0 else 0,
        p3_volume_pct=min(100.0, p3_coverage_vol / p3_vol * 100) if p3_vol > 0 else 0,
        bt2020_volume_pct=min(100.0, bt2020_coverage_vol / bt2020_vol * 100) if bt2020_vol > 0 else 0,
        relative_to_srgb_pct=panel_vol / srgb_vol * 100 if srgb_vol > 0 else 0,
        lightness_levels=list(lightness_levels),
        gamut_area_per_level=panel_areas
    )


def print_color_volume(result: ColorVolumeResult, panel_name: str = ""):
    """Print color volume analysis."""
    print(f"\n  Color Volume Analysis{f' - {panel_name}' if panel_name else ''}:")
    print(f"    Panel volume: {result.relative_to_srgb_pct:.0f}% of sRGB volume")
    print(f"    sRGB coverage:    {result.srgb_volume_pct:.1f}% (volume)")
    print(f"    DCI-P3 coverage:  {result.p3_volume_pct:.1f}% (volume)")
    print(f"    BT.2020 coverage: {result.bt2020_volume_pct:.1f}% (volume)")

    # Show per-lightness gamut
    print(f"\n    Gamut area by lightness:")
    for i in range(0, len(result.lightness_levels), max(1, len(result.lightness_levels) // 5)):
        L = result.lightness_levels[i]
        area = result.gamut_area_per_level[i]
        bar = "#" * int(area / max(result.gamut_area_per_level) * 30)
        print(f"      L*={L:5.1f}: {bar}")
