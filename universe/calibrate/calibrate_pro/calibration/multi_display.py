"""
Multi-Display Matching

Ensures consistent appearance across multiple displays by matching
white point, brightness, and gamma to common achievable targets.

When you have a QD-OLED next to a VA panel, they look different
even when both are "calibrated" because they target their own native
capabilities. Multi-display matching finds the common ground.
"""

import numpy as np
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple


@dataclass
class DisplayTarget:
    """Matched calibration target for one display."""
    display_index: int
    display_name: str
    panel_type: str

    # Matched targets (common across all displays)
    target_white_xy: Tuple[float, float]
    target_luminance: float       # cd/m2 — limited to weakest display's capability
    target_gamma: float

    # Per-display adjustments needed
    brightness_adjustment: float  # DDC-CI brightness (0-100)
    rgb_gain_r: float            # DDC-CI red gain
    rgb_gain_g: float            # DDC-CI green gain
    rgb_gain_b: float            # DDC-CI blue gain


@dataclass
class MatchingResult:
    """Result from multi-display matching analysis."""
    matched_white: Tuple[float, float]     # Common white point (typically D65)
    matched_luminance: float               # Common achievable brightness
    matched_gamma: float                   # Common gamma target
    per_display: List[DisplayTarget]
    notes: List[str]


def analyze_matching(panels: List[dict]) -> MatchingResult:
    """
    Analyze multiple displays and compute matching targets.

    Finds the common achievable white point, brightness, and gamma
    that all displays can hit. Constrains to the least capable display
    so all displays match.

    Args:
        panels: List of dicts with keys:
            - index: display index
            - name: display name
            - panel: PanelCharacterization object

    Returns:
        MatchingResult with per-display targets
    """
    if not panels:
        return MatchingResult(
            matched_white=(0.3127, 0.3290),
            matched_luminance=120.0,
            matched_gamma=2.2,
            per_display=[],
            notes=["No displays provided"]
        )

    notes = []

    # Target white point: D65 (standard for all displays)
    target_white = (0.3127, 0.3290)

    # Target gamma: 2.2 (universal)
    target_gamma = 2.2

    # Target luminance: constrained to the lowest SDR peak
    sdr_peaks = []
    for p in panels:
        panel = p["panel"]
        sdr_peak = panel.capabilities.max_luminance_sdr
        sdr_peaks.append(sdr_peak)

    # Use 80% of the weakest display's peak as the matching target
    # This ensures all displays can sustain the target brightness
    min_peak = min(sdr_peaks)
    matched_luminance = min_peak * 0.8

    if max(sdr_peaks) / min_peak > 1.5:
        notes.append(
            f"Large brightness difference between displays "
            f"({min(sdr_peaks):.0f} vs {max(sdr_peaks):.0f} cd/m2). "
            f"Matching to {matched_luminance:.0f} cd/m2."
        )

    # Per-display targets
    per_display = []
    for p in panels:
        panel = p["panel"]
        idx = p["index"]
        name = p["name"]

        # Calculate DDC-CI brightness for this display to hit target luminance
        brightness_pct = min(100, (matched_luminance / panel.capabilities.max_luminance_sdr) * 100)

        # White point: all displays target D65
        # RGB gains to correct native white to D65
        wp_x = panel.native_primaries.white.x
        wp_y = panel.native_primaries.white.y
        gains = _compute_wp_gains(wp_x, wp_y, target_white[0], target_white[1])

        dt = DisplayTarget(
            display_index=idx,
            display_name=name,
            panel_type=panel.panel_type,
            target_white_xy=target_white,
            target_luminance=matched_luminance,
            target_gamma=target_gamma,
            brightness_adjustment=brightness_pct,
            rgb_gain_r=gains[0],
            rgb_gain_g=gains[1],
            rgb_gain_b=gains[2]
        )
        per_display.append(dt)

    # Note display technology differences
    types = set(p["panel"].panel_type for p in panels)
    if len(types) > 1:
        notes.append(
            f"Mixed panel technologies ({', '.join(sorted(types))}). "
            f"Metamerism between panel types may cause slight visual "
            f"differences even after matching."
        )

    return MatchingResult(
        matched_white=target_white,
        matched_luminance=matched_luminance,
        matched_gamma=target_gamma,
        per_display=per_display,
        notes=notes
    )


def _compute_wp_gains(
    panel_x: float, panel_y: float,
    target_x: float, target_y: float
) -> Tuple[float, float, float]:
    """
    Compute RGB gain adjustments to shift white point from panel to target.

    Returns (r_gain, g_gain, b_gain) where 1.0 = no change.
    Values are normalized so max is 1.0 (reduce only).
    """
    if abs(panel_x - target_x) < 0.001 and abs(panel_y - target_y) < 0.001:
        return (1.0, 1.0, 1.0)

    # Sensitivity matrix (how RGB gains affect xy chromaticity)
    dx = target_x - panel_x
    dy = target_y - panel_y

    r_gain = 1.0 + dx * 2.5 + dy * (-0.3)
    g_gain = 1.0 + dx * (-1.0) + dy * 2.8
    b_gain = 1.0 + dx * (-1.5) + dy * (-2.5)

    # Normalize so max is 1.0
    max_gain = max(r_gain, g_gain, b_gain)
    if max_gain > 1.0:
        r_gain /= max_gain
        g_gain /= max_gain
        b_gain /= max_gain

    return (
        max(0.5, min(1.0, r_gain)),
        max(0.5, min(1.0, g_gain)),
        max(0.5, min(1.0, b_gain))
    )


def print_matching_plan(result: MatchingResult):
    """Print a human-readable matching plan."""
    print(f"\nMulti-Display Matching Plan")
    print(f"=" * 50)
    print(f"  Target white point: ({result.matched_white[0]:.4f}, {result.matched_white[1]:.4f})")
    print(f"  Target luminance:   {result.matched_luminance:.0f} cd/m2")
    print(f"  Target gamma:       {result.matched_gamma}")

    for dt in result.per_display:
        print(f"\n  {dt.display_name} ({dt.panel_type}):")
        print(f"    Brightness: {dt.brightness_adjustment:.0f}%")
        print(f"    RGB gains:  R={dt.rgb_gain_r:.3f} G={dt.rgb_gain_g:.3f} B={dt.rgb_gain_b:.3f}")

    for note in result.notes:
        print(f"\n  Note: {note}")
