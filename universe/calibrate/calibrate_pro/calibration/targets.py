"""
Professional Calibration Targets

Defines standard calibration targets used in broadcast, cinema, photography,
and HDR production. Each target specifies the complete set of parameters
needed for accurate display calibration.

Standards supported:
- ITU-R BT.709 (broadcast SDR)
- ITU-R BT.1886 (broadcast EOTF)
- DCI-P3 (digital cinema)
- Display P3 (Apple)
- ITU-R BT.2020 (UHDTV)
- SMPTE ST.2084 (PQ / HDR10)
- ITU-R BT.2100 (HLG)
- sRGB (IEC 61966-2-1)
- Adobe RGB (1998)
- ACES (Academy Color Encoding System)
- Netflix SDR/HDR delivery specs
- EBU Tech 3320 Grade 1
"""

from dataclasses import dataclass, field
from typing import Optional, Tuple, Dict, List


@dataclass
class CalibrationTarget:
    """
    Complete specification for a display calibration target.

    All chromaticity values are CIE 1931 xy coordinates.
    """
    name: str
    description: str

    # Color space
    red_xy: Tuple[float, float]
    green_xy: Tuple[float, float]
    blue_xy: Tuple[float, float]
    white_xy: Tuple[float, float]
    white_cct: int  # Correlated color temperature

    # Transfer function
    eotf: str           # "gamma", "srgb", "bt1886", "pq", "hlg"
    gamma: float        # Power-law gamma (2.2, 2.4, 2.6, etc.)

    # Luminance
    peak_luminance: float    # cd/m2 (nits)
    black_level: float       # cd/m2
    sdr_reference_white: float = 100.0  # cd/m2

    # Tolerances
    white_point_tolerance: float = 0.005  # Delta xy
    gamma_tolerance: float = 0.1          # +/- from target
    delta_e_target: float = 2.0           # Max acceptable dE2000

    # Metadata
    standard: str = ""     # ITU/SMPTE/IEC standard number
    category: str = ""     # "broadcast", "cinema", "photography", "hdr", "web"
    notes: str = ""


# =============================================================================
# Standard Primaries
# =============================================================================

# sRGB / BT.709 primaries
BT709_RED = (0.6400, 0.3300)
BT709_GREEN = (0.3000, 0.6000)
BT709_BLUE = (0.1500, 0.0600)

# DCI-P3 primaries
P3_RED = (0.6800, 0.3200)
P3_GREEN = (0.2650, 0.6900)
P3_BLUE = (0.1500, 0.0600)

# BT.2020 primaries
BT2020_RED = (0.7080, 0.2920)
BT2020_GREEN = (0.1700, 0.7970)
BT2020_BLUE = (0.1310, 0.0460)

# Adobe RGB primaries
ADOBERGB_RED = (0.6400, 0.3300)
ADOBERGB_GREEN = (0.2100, 0.7100)
ADOBERGB_BLUE = (0.1500, 0.0600)

# Standard white points
D65 = (0.3127, 0.3290)
D50 = (0.3457, 0.3585)
D63_DCI = (0.3140, 0.3510)  # DCI theatrical white (greenish tint)

# =============================================================================
# SDR Targets
# =============================================================================

REC709_BT1886 = CalibrationTarget(
    name="Rec.709 / BT.1886",
    description="Broadcast SDR reference (100 nits, gamma 2.4, D65)",
    red_xy=BT709_RED, green_xy=BT709_GREEN, blue_xy=BT709_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="bt1886", gamma=2.4,
    peak_luminance=100.0, black_level=0.05,
    white_point_tolerance=0.003,
    gamma_tolerance=0.10,
    delta_e_target=1.0,
    standard="ITU-R BT.1886",
    category="broadcast",
    notes="Standard for SDR broadcast grading. BT.1886 is not pure gamma 2.4 — "
          "it models black level offset for CRT-like response.",
)

SRGB = CalibrationTarget(
    name="sRGB",
    description="Web/desktop standard (80 nits, sRGB TRC, D65)",
    red_xy=BT709_RED, green_xy=BT709_GREEN, blue_xy=BT709_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="srgb", gamma=2.2,
    peak_luminance=80.0, black_level=0.2,
    sdr_reference_white=80.0,
    white_point_tolerance=0.005,
    gamma_tolerance=0.15,
    delta_e_target=2.0,
    standard="IEC 61966-2-1",
    category="web",
    notes="sRGB uses a piecewise TRC (linear toe + power), not pure gamma 2.2. "
          "Effective gamma is ~2.2.",
)

ADOBE_RGB = CalibrationTarget(
    name="Adobe RGB (1998)",
    description="Wide-gamut photography standard (D65, gamma 2.2)",
    red_xy=ADOBERGB_RED, green_xy=ADOBERGB_GREEN, blue_xy=ADOBERGB_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="gamma", gamma=2.19921875,  # Exact: 563/256
    peak_luminance=160.0, black_level=0.5,
    white_point_tolerance=0.005,
    delta_e_target=2.0,
    standard="Adobe RGB (1998)",
    category="photography",
    notes="Wider green primary than sRGB. Standard for print-oriented photography.",
)

PRINT_PROOFING_D50 = CalibrationTarget(
    name="Print Proofing (D50)",
    description="ISO 3664 soft-proofing (D50, sRGB gamut, 120 nits)",
    red_xy=BT709_RED, green_xy=BT709_GREEN, blue_xy=BT709_BLUE,
    white_xy=D50, white_cct=5003,
    eotf="srgb", gamma=2.2,
    peak_luminance=120.0, black_level=0.5,
    white_point_tolerance=0.003,
    delta_e_target=1.5,
    standard="ISO 3664",
    category="photography",
    notes="D50 white point for ICC Profile Connection Space consistency. "
          "Used when soft-proofing print output.",
)

# =============================================================================
# Cinema Targets
# =============================================================================

DCI_P3 = CalibrationTarget(
    name="DCI-P3",
    description="Digital Cinema Initiative (DCI white, gamma 2.6)",
    red_xy=P3_RED, green_xy=P3_GREEN, blue_xy=P3_BLUE,
    white_xy=D63_DCI, white_cct=6300,
    eotf="gamma", gamma=2.6,
    peak_luminance=48.0, black_level=0.005,  # 48 cd/m2 = 14 fL
    white_point_tolerance=0.002,
    delta_e_target=1.0,
    standard="SMPTE 431-2",
    category="cinema",
    notes="DCI theatrical specification. Note D63 white (greenish) not D65.",
)

DISPLAY_P3 = CalibrationTarget(
    name="Display P3",
    description="Apple Display P3 (D65, sRGB TRC)",
    red_xy=P3_RED, green_xy=P3_GREEN, blue_xy=P3_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="srgb", gamma=2.2,
    peak_luminance=500.0, black_level=0.05,
    delta_e_target=1.0,
    standard="Display P3 (Apple)",
    category="cinema",
    notes="P3 primaries with D65 white and sRGB TRC. "
          "Used by Apple devices and as the practical HDR gamut.",
)

# =============================================================================
# HDR Targets
# =============================================================================

HDR10_1000 = CalibrationTarget(
    name="HDR10 (1000 nits)",
    description="HDR10 mastering at 1000 nits peak (PQ, P3-D65 limited)",
    red_xy=P3_RED, green_xy=P3_GREEN, blue_xy=P3_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="pq", gamma=0.0,  # PQ is not gamma-based
    peak_luminance=1000.0, black_level=0.005,
    sdr_reference_white=203.0,
    delta_e_target=1.0,
    standard="SMPTE ST.2084 + BT.2020",
    category="hdr",
    notes="Most common HDR mastering format. Container is Rec.2020 but content "
          "is typically P3-D65 limited. Reference white at 203 nits per ITU-R BT.2408.",
)

HDR10_4000 = CalibrationTarget(
    name="HDR10 (4000 nits)",
    description="HDR10 premium mastering at 4000 nits peak",
    red_xy=P3_RED, green_xy=P3_GREEN, blue_xy=P3_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="pq", gamma=0.0,
    peak_luminance=4000.0, black_level=0.005,
    sdr_reference_white=203.0,
    delta_e_target=1.0,
    standard="SMPTE ST.2084 + BT.2020",
    category="hdr",
    notes="Premium HDR mastering. Used by Dolby Vision and high-end productions.",
)

HLG = CalibrationTarget(
    name="HLG (Hybrid Log-Gamma)",
    description="Broadcast HDR (relative, self-scaling)",
    red_xy=BT709_RED, green_xy=BT709_GREEN, blue_xy=BT709_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="hlg", gamma=1.2,  # System gamma
    peak_luminance=1000.0, black_level=0.005,
    sdr_reference_white=75.0,  # Nominal reference
    delta_e_target=2.0,
    standard="ITU-R BT.2100",
    category="hdr",
    notes="Relative HDR standard. Self-scales to display capability. "
          "Backwards compatible with SDR. Used by BBC, NHK, broadcasters.",
)

# =============================================================================
# Professional / Delivery Targets
# =============================================================================

NETFLIX_SDR = CalibrationTarget(
    name="Netflix SDR",
    description="Netflix SDR delivery specification",
    red_xy=BT709_RED, green_xy=BT709_GREEN, blue_xy=BT709_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="bt1886", gamma=2.4,
    peak_luminance=100.0, black_level=0.05,
    white_point_tolerance=0.003,
    gamma_tolerance=0.05,
    delta_e_target=1.0,
    standard="Netflix Partner Help Center",
    category="broadcast",
    notes="Rec.709, D65, BT.1886 at 100 nits. Contrast >= 2000:1. "
          "Display must be calibrated within last 6 months.",
)

NETFLIX_HDR = CalibrationTarget(
    name="Netflix HDR (Dolby Vision)",
    description="Netflix HDR/DV delivery specification",
    red_xy=P3_RED, green_xy=P3_GREEN, blue_xy=P3_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="pq", gamma=0.0,
    peak_luminance=1000.0, black_level=0.005,
    sdr_reference_white=203.0,
    white_point_tolerance=0.002,
    delta_e_target=1.0,
    standard="Netflix Partner Help Center",
    category="hdr",
    notes="P3-D65 limited, PQ, 1000 nits peak, 0.005 nits black. "
          "Contrast >= 200,000:1. Dolby Vision compatible.",
)

EBU_GRADE1 = CalibrationTarget(
    name="EBU Grade 1",
    description="EBU Tech 3320 Grade 1 broadcast reference",
    red_xy=BT709_RED, green_xy=BT709_GREEN, blue_xy=BT709_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="bt1886", gamma=2.4,
    peak_luminance=100.0, black_level=0.05,
    white_point_tolerance=0.003,
    gamma_tolerance=0.10,
    delta_e_target=1.0,
    standard="EBU Tech 3320",
    category="broadcast",
    notes="European Broadcasting Union Grade 1 reference. "
          "Gamma within +/-0.10 from 10%-90% input. "
          "White point within 0.003 xy of D65.",
)

REC2020 = CalibrationTarget(
    name="Rec.2020",
    description="UHDTV wide gamut (BT.2020 primaries)",
    red_xy=BT2020_RED, green_xy=BT2020_GREEN, blue_xy=BT2020_BLUE,
    white_xy=D65, white_cct=6504,
    eotf="pq", gamma=0.0,
    peak_luminance=1000.0, black_level=0.005,
    sdr_reference_white=203.0,
    delta_e_target=2.0,
    standard="ITU-R BT.2020",
    category="hdr",
    notes="Full BT.2020 gamut. No current consumer display covers 100%. "
          "Typically used as container with P3-D65 limited content.",
)

# =============================================================================
# Target Registry
# =============================================================================

ALL_TARGETS: Dict[str, CalibrationTarget] = {
    "rec709": REC709_BT1886,
    "srgb": SRGB,
    "adobe_rgb": ADOBE_RGB,
    "print_d50": PRINT_PROOFING_D50,
    "dci_p3": DCI_P3,
    "display_p3": DISPLAY_P3,
    "hdr10_1000": HDR10_1000,
    "hdr10_4000": HDR10_4000,
    "hlg": HLG,
    "netflix_sdr": NETFLIX_SDR,
    "netflix_hdr": NETFLIX_HDR,
    "ebu_grade1": EBU_GRADE1,
    "rec2020": REC2020,
}


def get_target(name: str) -> Optional[CalibrationTarget]:
    """Get a calibration target by name."""
    return ALL_TARGETS.get(name.lower().replace(" ", "_").replace("-", "_"))


def list_targets() -> List[Dict[str, str]]:
    """List all available calibration targets."""
    return [
        {
            "key": key,
            "name": t.name,
            "category": t.category,
            "description": t.description,
            "standard": t.standard,
        }
        for key, t in ALL_TARGETS.items()
    ]


def get_targets_by_category(category: str) -> List[CalibrationTarget]:
    """Get all targets in a category (broadcast, cinema, hdr, photography, web)."""
    return [t for t in ALL_TARGETS.values() if t.category == category]
