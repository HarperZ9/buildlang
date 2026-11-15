"""
Professional Verification Patch Sets - Calibrate Pro

Defines standard patch sets used by professional colorists, broadcast engineers,
and film post-production for display verification and calibration:

    - Grayscale ramps (21-step, 33-step, near-black, near-white)
    - Saturation sweeps for gamut accuracy
    - Broadcast standard bars (SMPTE, EBU)
    - ColorChecker Classic 24-patch
    - Skin tone reference patches
    - PLUGE (Picture Line-Up Generation Equipment)
    - Comprehensive combined set

All patch RGB values are in sRGB, normalized to the 0.0-1.0 range.

Usage:
    from calibrate_pro.verification.patch_sets import (
        GRAYSCALE_21, SATURATION_SWEEPS, get_patch_set, list_patch_sets,
    )

    for patch in GRAYSCALE_21:
        display(patch.r, patch.g, patch.b)
        measurement = measure()

    # Look up by name
    bars = get_patch_set("SMPTE_BARS")

    # List all available sets
    for name, description in list_patch_sets():
        print(f"{name}: {description}")
"""

from __future__ import annotations

import colorsys
from dataclasses import dataclass


# =============================================================================
# Patch Dataclass
# =============================================================================

@dataclass(frozen=True, slots=True)
class CalibrationPatch:
    """A single calibration patch with sRGB values and metadata.

    Attributes:
        name:     Human-readable patch identifier.
        r:        Red channel, sRGB 0.0-1.0.
        g:        Green channel, sRGB 0.0-1.0.
        b:        Blue channel, sRGB 0.0-1.0.
        category: Semantic category for grouping and analysis.
    """
    name: str
    r: float  # sRGB 0-1
    g: float  # sRGB 0-1
    b: float  # sRGB 0-1
    category: str  # "grayscale", "primary", "secondary", "saturation",
                    # "colorchecker", "skin", "pluge", "broadcast"

    @property
    def rgb(self) -> tuple[float, float, float]:
        """Return (R, G, B) tuple."""
        return (self.r, self.g, self.b)

    @property
    def rgb_8bit(self) -> tuple[int, int, int]:
        """Return 8-bit (0-255) RGB tuple."""
        return (
            min(255, max(0, round(self.r * 255))),
            min(255, max(0, round(self.g * 255))),
            min(255, max(0, round(self.b * 255))),
        )

    def __repr__(self) -> str:
        return (
            f"CalibrationPatch({self.name!r}, "
            f"r={self.r:.4f}, g={self.g:.4f}, b={self.b:.4f}, "
            f"cat={self.category!r})"
        )


# =============================================================================
# Helper: sRGB gamma
# =============================================================================

def _srgb_gamma_compress(linear: float) -> float:
    """IEC 61966-2-1 forward transfer (linear -> sRGB)."""
    if linear <= 0.0031308:
        return 12.92 * linear
    return 1.055 * (linear ** (1.0 / 2.4)) - 0.055


def _srgb_gamma_expand(srgb: float) -> float:
    """IEC 61966-2-1 inverse transfer (sRGB -> linear)."""
    if srgb <= 0.04045:
        return srgb / 12.92
    return ((srgb + 0.055) / 1.055) ** 2.4


# =============================================================================
# 1. GRAYSCALE_21 - 21-point grayscale ramp (0% to 100% in 5% steps)
# =============================================================================
# Used for EOTF/gamma tracking verification.  Each step is an equal-energy
# neutral at the given stimulus level in sRGB (i.e., R=G=B=level/100).

GRAYSCALE_21: list[CalibrationPatch] = [
    CalibrationPatch(
        name=f"Gray {pct}%",
        r=round(pct / 100.0, 4),
        g=round(pct / 100.0, 4),
        b=round(pct / 100.0, 4),
        category="grayscale",
    )
    for pct in range(0, 105, 5)  # 0, 5, 10, ... 100
]


# =============================================================================
# 2. GRAYSCALE_33 - 33-point grayscale (~3% steps)
# =============================================================================
# Finer ramp for precise gamma analysis and banding detection.
# 33 evenly-spaced points from 0% to 100% (~3.125% steps).

_gs33_levels = [round(i * 100.0 / 32.0, 2) for i in range(33)]

GRAYSCALE_33: list[CalibrationPatch] = [
    CalibrationPatch(
        name=f"Gray {pct}%",
        r=round(pct / 100.0, 4),
        g=round(pct / 100.0, 4),
        b=round(pct / 100.0, 4),
        category="grayscale",
    )
    for pct in _gs33_levels
]


# =============================================================================
# 3. NEAR_BLACK_11 - 0% to 10% in 1% steps
# =============================================================================
# Critical for shadow detail evaluation, black-level offset, and banding
# detection in near-black regions.

NEAR_BLACK_11: list[CalibrationPatch] = [
    CalibrationPatch(
        name=f"Near Black {pct}%",
        r=round(pct / 100.0, 4),
        g=round(pct / 100.0, 4),
        b=round(pct / 100.0, 4),
        category="grayscale",
    )
    for pct in range(0, 11)  # 0-10 inclusive
]


# =============================================================================
# 4. NEAR_WHITE_11 - 90% to 100% in 1% steps
# =============================================================================
# For highlight clipping analysis and near-white linearity.

NEAR_WHITE_11: list[CalibrationPatch] = [
    CalibrationPatch(
        name=f"Near White {pct}%",
        r=round(pct / 100.0, 4),
        g=round(pct / 100.0, 4),
        b=round(pct / 100.0, 4),
        category="grayscale",
    )
    for pct in range(90, 101)  # 90-100 inclusive
]


# =============================================================================
# 5. SATURATION_SWEEPS - Gamut accuracy across saturation levels
# =============================================================================
# For each of the six primary/secondary hues (R, G, B, C, M, Y), generate
# patches at 25%, 50%, 75%, and 100% saturation with Value=1.0 (full stimulus).
# HSV is converted to sRGB for display.
#
# Hue angles (HSV): R=0, Y=60, G=120, C=180, B=240, M=300

_SWEEP_HUES = [
    ("Red",     0.0),
    ("Yellow", 60.0),
    ("Green", 120.0),
    ("Cyan",  180.0),
    ("Blue",  240.0),
    ("Magenta", 300.0),
]

_SWEEP_SATS = [25, 50, 75, 100]


def _build_saturation_sweeps() -> list[CalibrationPatch]:
    patches: list[CalibrationPatch] = []
    for hue_name, hue_deg in _SWEEP_HUES:
        for sat_pct in _SWEEP_SATS:
            h = hue_deg / 360.0
            s = sat_pct / 100.0
            v = 1.0
            r, g, b = colorsys.hsv_to_rgb(h, s, v)
            # Determine category
            if hue_name in ("Red", "Green", "Blue"):
                cat = "primary"
            else:
                cat = "secondary"
            patches.append(CalibrationPatch(
                name=f"{hue_name} {sat_pct}% Sat",
                r=round(r, 4),
                g=round(g, 4),
                b=round(b, 4),
                category=cat,
            ))
    return patches


SATURATION_SWEEPS: list[CalibrationPatch] = _build_saturation_sweeps()


# =============================================================================
# 6. PRIMARIES_SECONDARIES - Full-saturation at 75% and 100% stimulus
# =============================================================================
# Standard broadcast verification set.  75% bars are the traditional
# broadcast standard; 100% bars stress-test peak gamut.

def _build_primaries_secondaries() -> list[CalibrationPatch]:
    patches: list[CalibrationPatch] = []
    colors = [
        # (name, category, r100, g100, b100)
        ("Red",     "primary",   1.0, 0.0, 0.0),
        ("Green",   "primary",   0.0, 1.0, 0.0),
        ("Blue",    "primary",   0.0, 0.0, 1.0),
        ("Cyan",    "secondary", 0.0, 1.0, 1.0),
        ("Magenta", "secondary", 1.0, 0.0, 1.0),
        ("Yellow",  "secondary", 1.0, 1.0, 0.0),
    ]
    for level_pct in (75, 100):
        scale = level_pct / 100.0
        for name, cat, r, g, b in colors:
            patches.append(CalibrationPatch(
                name=f"{name} {level_pct}%",
                r=round(r * scale, 4),
                g=round(g * scale, 4),
                b=round(b * scale, 4),
                category=cat,
            ))
    return patches


PRIMARIES_SECONDARIES: list[CalibrationPatch] = _build_primaries_secondaries()


# =============================================================================
# 7. SMPTE_BARS - SMPTE RP 219 color bar values
# =============================================================================
# SMPTE color bars as defined in SMPTE RP 219 / EG 1-1990.
# Two variants: 75% amplitude (standard) and 100% amplitude.
#
# Bar order (left to right): White, Yellow, Cyan, Green, Magenta, Red, Blue, Black
# sRGB values for 75% bars: the active component is 0.75, inactive is 0.0.
# sRGB values for 100% bars: the active component is 1.0, inactive is 0.0.

def _build_smpte_bars() -> list[CalibrationPatch]:
    patches: list[CalibrationPatch] = []

    # 75% amplitude bars (standard SMPTE)
    bars_75 = [
        ("SMPTE 75% White",   0.75, 0.75, 0.75, "grayscale"),
        ("SMPTE 75% Yellow",  0.75, 0.75, 0.00, "secondary"),
        ("SMPTE 75% Cyan",    0.00, 0.75, 0.75, "secondary"),
        ("SMPTE 75% Green",   0.00, 0.75, 0.00, "primary"),
        ("SMPTE 75% Magenta", 0.75, 0.00, 0.75, "secondary"),
        ("SMPTE 75% Red",     0.75, 0.00, 0.00, "primary"),
        ("SMPTE 75% Blue",    0.00, 0.00, 0.75, "primary"),
        ("SMPTE 75% Black",   0.00, 0.00, 0.00, "grayscale"),
    ]
    for name, r, g, b, cat in bars_75:
        patches.append(CalibrationPatch(name=name, r=r, g=g, b=b, category=cat))

    # 100% amplitude bars
    bars_100 = [
        ("SMPTE 100% White",   1.0, 1.0, 1.0, "grayscale"),
        ("SMPTE 100% Yellow",  1.0, 1.0, 0.0, "secondary"),
        ("SMPTE 100% Cyan",    0.0, 1.0, 1.0, "secondary"),
        ("SMPTE 100% Green",   0.0, 1.0, 0.0, "primary"),
        ("SMPTE 100% Magenta", 1.0, 0.0, 1.0, "secondary"),
        ("SMPTE 100% Red",     1.0, 0.0, 0.0, "primary"),
        ("SMPTE 100% Blue",    0.0, 0.0, 1.0, "primary"),
        ("SMPTE 100% Black",   0.0, 0.0, 0.0, "grayscale"),
    ]
    for name, r, g, b, cat in bars_100:
        patches.append(CalibrationPatch(name=name, r=r, g=g, b=b, category=cat))

    # Sub-black / PLUGE region of SMPTE bars
    # -4% (below black, should be invisible), 0% (black), +4% (just visible)
    patches.append(CalibrationPatch(
        name="SMPTE -4% Sub-Black", r=0.0, g=0.0, b=0.0, category="pluge",
    ))
    patches.append(CalibrationPatch(
        name="SMPTE 0% Black", r=0.0, g=0.0, b=0.0, category="pluge",
    ))
    patches.append(CalibrationPatch(
        name="SMPTE +4% Super-Black",
        r=round(4 / 100.0, 4), g=round(4 / 100.0, 4), b=round(4 / 100.0, 4),
        category="pluge",
    ))

    return patches


SMPTE_BARS: list[CalibrationPatch] = _build_smpte_bars()


# =============================================================================
# 8. EBU_BARS - EBU Tech 3325 colour bars
# =============================================================================
# EBU bars are 75% amplitude, 100% saturation by default.
# Bar order (left to right): White, Yellow, Cyan, Green, Magenta, Red, Blue, Black
# In EBU 75/0 mode, the peak level is 0.75 and the floor is 0.0.

EBU_BARS: list[CalibrationPatch] = [
    CalibrationPatch("EBU White",   0.75, 0.75, 0.75, "grayscale"),
    CalibrationPatch("EBU Yellow",  0.75, 0.75, 0.00, "secondary"),
    CalibrationPatch("EBU Cyan",    0.00, 0.75, 0.75, "secondary"),
    CalibrationPatch("EBU Green",   0.00, 0.75, 0.00, "primary"),
    CalibrationPatch("EBU Magenta", 0.75, 0.00, 0.75, "secondary"),
    CalibrationPatch("EBU Red",     0.75, 0.00, 0.00, "primary"),
    CalibrationPatch("EBU Blue",    0.00, 0.00, 0.75, "primary"),
    CalibrationPatch("EBU Black",   0.00, 0.00, 0.00, "grayscale"),
]


# =============================================================================
# 9. COLORCHECKER_CLASSIC - X-Rite ColorChecker Classic 24-patch
# =============================================================================
# sRGB values sourced from BabelColor / X-Rite published data.
# Reference Lab D50 values are in COLORCHECKER_CLASSIC_LAB_D50 below.

COLORCHECKER_CLASSIC: list[CalibrationPatch] = [
    # Row 1 - Natural colors
    CalibrationPatch("Dark Skin",    0.453, 0.317, 0.264, "colorchecker"),
    CalibrationPatch("Light Skin",   0.779, 0.577, 0.505, "colorchecker"),
    CalibrationPatch("Blue Sky",     0.355, 0.480, 0.611, "colorchecker"),
    CalibrationPatch("Foliage",      0.352, 0.422, 0.253, "colorchecker"),
    CalibrationPatch("Blue Flower",  0.508, 0.502, 0.691, "colorchecker"),
    CalibrationPatch("Bluish Green", 0.362, 0.745, 0.675, "colorchecker"),
    # Row 2 - Miscellaneous colors
    CalibrationPatch("Orange",       0.879, 0.485, 0.183, "colorchecker"),
    CalibrationPatch("Purplish Blue", 0.266, 0.358, 0.667, "colorchecker"),
    CalibrationPatch("Moderate Red", 0.778, 0.321, 0.381, "colorchecker"),
    CalibrationPatch("Purple",       0.367, 0.227, 0.414, "colorchecker"),
    CalibrationPatch("Yellow Green", 0.623, 0.741, 0.246, "colorchecker"),
    CalibrationPatch("Orange Yellow", 0.904, 0.634, 0.154, "colorchecker"),
    # Row 3 - Primary and secondary colors
    CalibrationPatch("Blue",         0.139, 0.248, 0.577, "colorchecker"),
    CalibrationPatch("Green",        0.262, 0.584, 0.291, "colorchecker"),
    CalibrationPatch("Red",          0.752, 0.197, 0.178, "colorchecker"),
    CalibrationPatch("Yellow",       0.938, 0.857, 0.159, "colorchecker"),
    CalibrationPatch("Magenta",      0.752, 0.313, 0.577, "colorchecker"),
    CalibrationPatch("Cyan",         0.121, 0.544, 0.659, "colorchecker"),
    # Row 4 - Grayscale
    CalibrationPatch("White",        0.961, 0.961, 0.961, "colorchecker"),
    CalibrationPatch("Neutral 8",    0.784, 0.784, 0.784, "colorchecker"),
    CalibrationPatch("Neutral 6.5",  0.584, 0.584, 0.584, "colorchecker"),
    CalibrationPatch("Neutral 5",    0.420, 0.420, 0.420, "colorchecker"),
    CalibrationPatch("Neutral 3.5",  0.258, 0.258, 0.258, "colorchecker"),
    CalibrationPatch("Black",        0.085, 0.085, 0.085, "colorchecker"),
]

# Reference Lab D50 values for ColorChecker Classic 24-patch.
# Based on X-Rite published data (2014 revision), D50 illuminant.
# Also available in calibrate_pro.verification.colorchecker.COLORCHECKER_CLASSIC_D50
COLORCHECKER_CLASSIC_LAB_D50: dict[str, tuple[float, float, float]] = {
    "Dark Skin":    (37.986,  13.555,  14.059),
    "Light Skin":   (65.711,  18.130,  17.810),
    "Blue Sky":     (49.927,  -4.880, -21.925),
    "Foliage":      (43.139, -13.095,  21.905),
    "Blue Flower":  (55.112,   8.844, -25.399),
    "Bluish Green": (70.719, -33.397,  -0.199),
    "Orange":       (62.661,  36.067,  57.096),
    "Purplish Blue": (40.020, 10.410, -45.964),
    "Moderate Red": (51.124,  48.239,  16.248),
    "Purple":       (30.325,  22.976, -21.587),
    "Yellow Green": (72.532, -23.709,  57.255),
    "Orange Yellow": (71.941, 19.363,  67.857),
    "Blue":         (28.778,  14.179, -50.297),
    "Green":        (55.261, -38.342,  31.370),
    "Red":          (42.101,  53.378,  28.190),
    "Yellow":       (81.733,   4.039,  79.819),
    "Magenta":      (51.935,  49.986, -14.574),
    "Cyan":         (51.038, -28.631, -28.638),
    "White":        (96.539,  -0.425,   1.186),
    "Neutral 8":    (81.257,  -0.638,  -0.335),
    "Neutral 6.5":  (66.766,  -0.734,  -0.504),
    "Neutral 5":    (50.867,  -0.153,  -0.270),
    "Neutral 3.5":  (35.656,  -0.421,  -1.231),
    "Black":        (20.461,  -0.079,  -0.973),
}


# =============================================================================
# 10. SKIN_TONES - Critical skin tone patches
# =============================================================================
# Curated from ColorChecker SG, Pantone SkinTone Guide, and industry references.
# Accurate skin reproduction is the single most important aspect of color
# grading; even small errors are immediately visible.
#
# sRGB values are derived from published Lab/spectral data and rounded to
# 3 decimal places.

SKIN_TONES: list[CalibrationPatch] = [
    # Light Caucasian skin
    CalibrationPatch("Skin Light 1",         0.890, 0.733, 0.635, "skin"),
    CalibrationPatch("Skin Light 2",         0.843, 0.694, 0.608, "skin"),
    # Medium Caucasian / Mediterranean
    CalibrationPatch("Skin Medium 1",        0.792, 0.612, 0.502, "skin"),
    CalibrationPatch("Skin Medium 2",        0.745, 0.569, 0.467, "skin"),
    # Olive / East Asian
    CalibrationPatch("Skin Olive 1",         0.710, 0.553, 0.416, "skin"),
    CalibrationPatch("Skin Olive 2",         0.659, 0.494, 0.369, "skin"),
    # Medium-dark / South Asian / Latin
    CalibrationPatch("Skin Medium-Dark 1",   0.580, 0.408, 0.310, "skin"),
    CalibrationPatch("Skin Medium-Dark 2",   0.502, 0.341, 0.259, "skin"),
    # Dark / African
    CalibrationPatch("Skin Dark 1",          0.400, 0.275, 0.216, "skin"),
    CalibrationPatch("Skin Dark 2",          0.318, 0.208, 0.165, "skin"),
    # ColorChecker reference skin patches
    CalibrationPatch("CC Dark Skin",         0.453, 0.317, 0.264, "skin"),
    CalibrationPatch("CC Light Skin",        0.779, 0.577, 0.505, "skin"),
]


# =============================================================================
# 11. PLUGE - Picture Line-Up Generation Equipment
# =============================================================================
# Per ITU-R BT.814 and SMPTE RP 219.
# PLUGE is used to set the brightness (black level) of a display.
#
# The three critical steps are:
#   - Below-black (3.5%): Should be invisible on a correctly adjusted display.
#   - Black reference (7.5%): The reference black level (NTSC setup, 7.5 IRE).
#   - Just-above-black (11.4%): Should be just barely visible.
#
# Additional steps are included for fine adjustment.

PLUGE: list[CalibrationPatch] = [
    CalibrationPatch("PLUGE 0% Black",       0.0,    0.0,    0.0,    "pluge"),
    CalibrationPatch("PLUGE 2% Sub-Black",   0.02,   0.02,   0.02,   "pluge"),
    CalibrationPatch("PLUGE 3.5% Below",     0.035,  0.035,  0.035,  "pluge"),
    CalibrationPatch("PLUGE 5%",             0.05,   0.05,   0.05,   "pluge"),
    CalibrationPatch("PLUGE 7.5% Reference", 0.075,  0.075,  0.075,  "pluge"),
    CalibrationPatch("PLUGE 10%",            0.10,   0.10,   0.10,   "pluge"),
    CalibrationPatch("PLUGE 11.4% Above",    0.114,  0.114,  0.114,  "pluge"),
    CalibrationPatch("PLUGE 15%",            0.15,   0.15,   0.15,   "pluge"),
    CalibrationPatch("PLUGE 20%",            0.20,   0.20,   0.20,   "pluge"),
]


# =============================================================================
# 12. COMPREHENSIVE_100 - Combined ~100 patch set
# =============================================================================
# A practical combined verification set covering all critical areas:
#   - Drift reference (white, mid-gray, black) at start and end  (6 patches)
#   - 11-step grayscale (0%, 10%, 20% ... 100%)
#   - Primaries + secondaries at 75% and 100%  (12 patches)
#   - Saturation sweeps (24 patches)
#   - Near-black 0-10%  (11 patches)
#   - Near-white 90-100%  (11 patches)
#   - ColorChecker 24-patch
#   - Skin tones  (10-12 patches)
#   - PLUGE  (9 patches)
#
# Total: ~100 patches (exact count depends on deduplication of shared colors).

def _build_comprehensive_100() -> list[CalibrationPatch]:
    seen: set[tuple[float, float, float]] = set()
    patches: list[CalibrationPatch] = []

    def _add(p: CalibrationPatch) -> None:
        key = (round(p.r, 4), round(p.g, 4), round(p.b, 4))
        if key not in seen:
            seen.add(key)
            patches.append(p)

    def _add_always(p: CalibrationPatch) -> None:
        """Add without dedup (for drift references that repeat by design)."""
        patches.append(p)

    # Drift reference - start (always included, even if duplicated later)
    _add_always(CalibrationPatch("Drift Ref White (Start)",  1.0, 1.0, 1.0, "grayscale"))
    _add_always(CalibrationPatch("Drift Ref Gray (Start)",   0.5, 0.5, 0.5, "grayscale"))
    _add_always(CalibrationPatch("Drift Ref Black (Start)",  0.0, 0.0, 0.0, "grayscale"))

    # 11-step grayscale (0%, 10%, 20% ... 100%)
    for pct in range(0, 110, 10):
        v = round(pct / 100.0, 4)
        _add(CalibrationPatch(f"Gray {pct}%", v, v, v, "grayscale"))

    # Primaries + secondaries at 75% and 100%
    for p in PRIMARIES_SECONDARIES:
        _add(p)

    # Saturation sweeps (R, G, B, C, M, Y at 25%, 50%, 75%, 100%)
    for p in SATURATION_SWEEPS:
        _add(p)

    # Near-black (0-10%)
    for p in NEAR_BLACK_11:
        _add(p)

    # Near-white (90-100%)
    for p in NEAR_WHITE_11:
        _add(p)

    # ColorChecker 24
    for p in COLORCHECKER_CLASSIC:
        _add(p)

    # Skin tones (unique colors not already in ColorChecker)
    for p in SKIN_TONES:
        _add(p)

    # PLUGE critical patches
    for p in PLUGE:
        _add(p)

    # Drift reference - end (always included for drift detection)
    _add_always(CalibrationPatch("Drift Ref White (End)",  1.0, 1.0, 1.0, "grayscale"))
    _add_always(CalibrationPatch("Drift Ref Gray (End)",   0.5, 0.5, 0.5, "grayscale"))
    _add_always(CalibrationPatch("Drift Ref Black (End)",  0.0, 0.0, 0.0, "grayscale"))

    return patches


COMPREHENSIVE_100: list[CalibrationPatch] = _build_comprehensive_100()


# =============================================================================
# Patch Set Registry
# =============================================================================

_PATCH_SET_REGISTRY: dict[str, tuple[list[CalibrationPatch], str]] = {
    "GRAYSCALE_21": (
        GRAYSCALE_21,
        "21-point grayscale ramp (0-100% in 5% steps) for EOTF/gamma tracking",
    ),
    "GRAYSCALE_33": (
        GRAYSCALE_33,
        "33-point grayscale (~3% steps) for precise gamma analysis",
    ),
    "NEAR_BLACK_11": (
        NEAR_BLACK_11,
        "0-10% in 1% steps for shadow detail and banding detection",
    ),
    "NEAR_WHITE_11": (
        NEAR_WHITE_11,
        "90-100% in 1% steps for highlight clipping analysis",
    ),
    "SATURATION_SWEEPS": (
        SATURATION_SWEEPS,
        "R/G/B/C/M/Y at 25/50/75/100% saturation for gamut accuracy",
    ),
    "PRIMARIES_SECONDARIES": (
        PRIMARIES_SECONDARIES,
        "Full-saturation R/G/B/C/M/Y at 75% and 100% (broadcast standard)",
    ),
    "SMPTE_BARS": (
        SMPTE_BARS,
        "SMPTE RP 219 color bars (75% and 100% amplitude variants)",
    ),
    "EBU_BARS": (
        EBU_BARS,
        "EBU Tech 3325 colour bars (75/0 standard)",
    ),
    "COLORCHECKER_CLASSIC": (
        COLORCHECKER_CLASSIC,
        "X-Rite ColorChecker Classic 24-patch verification set",
    ),
    "SKIN_TONES": (
        SKIN_TONES,
        "12 critical skin tone patches spanning light to dark complexions",
    ),
    "PLUGE": (
        PLUGE,
        "PLUGE patches per ITU-R BT.814 (3.5%, 7.5%, 11.4% + extras)",
    ),
    "COMPREHENSIVE_100": (
        COMPREHENSIVE_100,
        "Combined ~100 patches: grayscale + primaries + sweeps + near-black "
        "+ ColorChecker + drift references",
    ),
}


# =============================================================================
# Public API Functions
# =============================================================================

def get_patch_set(name: str) -> list[CalibrationPatch]:
    """Return a named patch set.

    Args:
        name: Patch set identifier (case-insensitive).  Valid names can be
              obtained from :func:`list_patch_sets`.

    Returns:
        List of :class:`CalibrationPatch` objects.

    Raises:
        KeyError: If *name* does not match any registered patch set.

    Example::

        patches = get_patch_set("GRAYSCALE_21")
        for p in patches:
            print(p.name, p.rgb)
    """
    key = name.upper().strip()
    if key not in _PATCH_SET_REGISTRY:
        available = ", ".join(sorted(_PATCH_SET_REGISTRY.keys()))
        raise KeyError(
            f"Unknown patch set {name!r}. Available sets: {available}"
        )
    return _PATCH_SET_REGISTRY[key][0]


def list_patch_sets() -> list[tuple[str, str]]:
    """Return all registered patch set names with descriptions.

    Returns:
        Sorted list of (name, description) tuples.

    Example::

        for name, desc in list_patch_sets():
            patches = get_patch_set(name)
            print(f"{name} ({len(patches)} patches): {desc}")
    """
    return sorted(
        (name, desc) for name, (_, desc) in _PATCH_SET_REGISTRY.items()
    )


def get_colorchecker_lab_reference(patch_name: str) -> tuple[float, float, float]:
    """Return the Lab D50 reference value for a ColorChecker Classic patch.

    Args:
        patch_name: Patch name as used in :data:`COLORCHECKER_CLASSIC`
                    (e.g. ``"Dark Skin"``, ``"Neutral 8"``).

    Returns:
        (L*, a*, b*) tuple under D50 illuminant.

    Raises:
        KeyError: If *patch_name* is not a valid ColorChecker patch name.
    """
    if patch_name not in COLORCHECKER_CLASSIC_LAB_D50:
        available = ", ".join(sorted(COLORCHECKER_CLASSIC_LAB_D50.keys()))
        raise KeyError(
            f"Unknown ColorChecker patch {patch_name!r}. "
            f"Available: {available}"
        )
    return COLORCHECKER_CLASSIC_LAB_D50[patch_name]


# =============================================================================
# Module-level summary
# =============================================================================

__all__ = [
    # Dataclass
    "CalibrationPatch",
    # Patch sets
    "GRAYSCALE_21",
    "GRAYSCALE_33",
    "NEAR_BLACK_11",
    "NEAR_WHITE_11",
    "SATURATION_SWEEPS",
    "PRIMARIES_SECONDARIES",
    "SMPTE_BARS",
    "EBU_BARS",
    "COLORCHECKER_CLASSIC",
    "COLORCHECKER_CLASSIC_LAB_D50",
    "SKIN_TONES",
    "PLUGE",
    "COMPREHENSIVE_100",
    # Functions
    "get_patch_set",
    "list_patch_sets",
    "get_colorchecker_lab_reference",
]
