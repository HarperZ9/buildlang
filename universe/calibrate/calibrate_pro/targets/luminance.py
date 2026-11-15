"""
Luminance Targets - Professional luminance and contrast calibration.

Supports:
- Peak luminance (SDR/HDR)
- Black level
- Contrast ratio
- Industry standards (SDR, HDR10, Dolby Vision, HLG)
- Reference monitor standards (Rec.709, EBU, SMPTE)
- Custom targets

For professional colorists, broadcast engineers, and enthusiasts.
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Optional, Tuple, Dict, List
from enum import Enum


class LuminanceStandard(Enum):
    """Industry luminance standards."""
    # SDR Standards
    SDR_GENERAL = "SDR General"           # 80-120 cd/m2
    REC709_BROADCAST = "Rec.709 Broadcast" # 100 cd/m2 (EBU Tech 3320)
    SMPTE_RP166 = "SMPTE RP 166"          # 35 cd/m2 (film grading)
    EBU_GRADE1 = "EBU Grade 1"            # 100 cd/m2
    DCI_P3_CINEMA = "DCI-P3 Cinema"       # 48 cd/m2 (14 fL)

    # HDR Standards
    HDR10 = "HDR10"                       # 1000+ cd/m2 peak
    HDR10_PLUS = "HDR10+"                 # 1000-4000 cd/m2
    DOLBY_VISION = "Dolby Vision"         # 1000-10000 cd/m2
    HLG = "HLG"                           # 1000+ cd/m2
    HDR_REFERENCE = "HDR Reference"       # 1000 cd/m2 mastering

    # Consumer Displays
    CONSUMER_SDR = "Consumer SDR"         # 200-350 cd/m2
    CONSUMER_HDR = "Consumer HDR"         # 400-1000 cd/m2

    # Professional Reference
    REFERENCE_GRADE = "Reference Grade"   # Calibrated to standard
    NATIVE = "Native"                     # Display maximum
    CUSTOM = "Custom"


class BlackLevelStandard(Enum):
    """Black level reference standards."""
    ABSOLUTE_BLACK = "Absolute Black"     # 0.0001 cd/m2 (OLED ideal)
    REFERENCE_BLACK = "Reference Black"   # 0.005 cd/m2 (high-end reference)
    BROADCAST_BLACK = "Broadcast Black"   # 0.01 cd/m2
    CONSUMER_LCD = "Consumer LCD"         # 0.05-0.1 cd/m2
    NATIVE = "Native"                     # Display native
    CUSTOM = "Custom"


# Standard luminance values (cd/m2 = nits)
LUMINANCE_STANDARDS: Dict[str, Dict] = {
    # SDR Standards
    "SDR General": {
        "peak": 120.0,
        "reference_white": 100.0,
        "min_black": 0.05,
        "contrast": 1000,
        "description": "General SDR content viewing"
    },
    "Rec.709 Broadcast": {
        "peak": 100.0,
        "reference_white": 100.0,
        "min_black": 0.01,
        "contrast": 10000,
        "description": "EBU Tech 3320 broadcast reference"
    },
    "SMPTE RP 166": {
        "peak": 35.0,
        "reference_white": 35.0,
        "min_black": 0.005,
        "contrast": 7000,
        "description": "Film grading in dark room (14 fL ambient)"
    },
    "EBU Grade 1": {
        "peak": 100.0,
        "reference_white": 100.0,
        "min_black": 0.01,
        "contrast": 10000,
        "description": "EBU Grade 1 reference monitor"
    },
    "DCI-P3 Cinema": {
        "peak": 48.0,
        "reference_white": 48.0,
        "min_black": 0.001,
        "contrast": 2000,
        "description": "Digital Cinema (14 fL / 48 cd/m2)"
    },

    # HDR Standards
    "HDR10": {
        "peak": 1000.0,
        "reference_white": 203.0,  # 75% signal = 203 nits (ITU-R BT.2408)
        "min_black": 0.005,
        "contrast": 200000,
        "max_cll": 1000,
        "max_fall": 400,
        "description": "HDR10 mastering standard"
    },
    "HDR10+": {
        "peak": 4000.0,
        "reference_white": 203.0,
        "min_black": 0.0005,
        "contrast": 8000000,
        "description": "HDR10+ with dynamic metadata"
    },
    "Dolby Vision": {
        "peak": 4000.0,
        "reference_white": 203.0,
        "min_black": 0.0001,
        "contrast": 40000000,
        "description": "Dolby Vision mastering"
    },
    "HLG": {
        "peak": 1000.0,
        "reference_white": 203.0,  # At 1000 nit display
        "min_black": 0.005,
        "contrast": 200000,
        "description": "Hybrid Log-Gamma broadcast HDR"
    },
    "HDR Reference": {
        "peak": 1000.0,
        "reference_white": 203.0,
        "min_black": 0.005,
        "contrast": 200000,
        "description": "Standard HDR reference mastering"
    },

    # Consumer
    "Consumer SDR": {
        "peak": 300.0,
        "reference_white": 200.0,
        "min_black": 0.1,
        "contrast": 3000,
        "description": "Typical consumer SDR display"
    },
    "Consumer HDR": {
        "peak": 600.0,
        "reference_white": 203.0,
        "min_black": 0.05,
        "contrast": 12000,
        "description": "Typical consumer HDR display"
    },
}


def nits_to_footlamberts(nits: float) -> float:
    """Convert cd/m2 (nits) to foot-lamberts."""
    return nits / 3.426


def footlamberts_to_nits(fl: float) -> float:
    """Convert foot-lamberts to cd/m2 (nits)."""
    return fl * 3.426


def calculate_contrast_ratio(peak: float, black: float) -> float:
    """Calculate contrast ratio from peak and black level."""
    if black <= 0:
        return float('inf')
    return peak / black


def calculate_black_level(peak: float, contrast: float) -> float:
    """Calculate black level from peak and contrast ratio."""
    if contrast <= 0:
        return peak
    return peak / contrast


@dataclass
class LuminanceTarget:
    """
    Professional luminance target specification.

    Supports SDR and HDR workflows with multiple ways to specify luminance:
    1. Standard preset (Rec.709, HDR10, etc.)
    2. Custom peak luminance
    3. Custom black level
    4. Target contrast ratio

    Attributes:
        standard: Industry standard preset
        peak_luminance: Target peak white (cd/m2)
        reference_white: Reference diffuse white level (cd/m2)
        black_level: Target black level (cd/m2)
        target_contrast: Target contrast ratio (calculated if None)
        hdr_mode: Enable HDR luminance targets
        max_cll: Maximum Content Light Level (HDR metadata)
        max_fall: Maximum Frame Average Light Level (HDR metadata)
        surround_luminance: Viewing environment (dark/dim/average)
        tolerance_percent: Acceptable deviation percentage
    """
    standard: LuminanceStandard = LuminanceStandard.SDR_GENERAL
    peak_luminance: Optional[float] = None  # cd/m2
    reference_white: Optional[float] = None  # cd/m2
    black_level: Optional[float] = None  # cd/m2
    target_contrast: Optional[float] = None
    hdr_mode: bool = False

    # HDR metadata
    max_cll: Optional[int] = None  # MaxCLL
    max_fall: Optional[int] = None  # MaxFALL

    # Viewing environment
    surround_luminance: float = 5.0  # cd/m2 (dim surround default)

    # Tolerance
    tolerance_percent: float = 5.0  # Acceptable deviation %

    # Display
    name: str = ""
    description: str = ""

    def __post_init__(self):
        if not self.name:
            self.name = self.standard.value

    def get_peak_luminance(self) -> float:
        """Get target peak luminance in cd/m2."""
        if self.peak_luminance is not None:
            return self.peak_luminance

        standard_name = self.standard.value
        if standard_name in LUMINANCE_STANDARDS:
            return LUMINANCE_STANDARDS[standard_name]["peak"]

        # Default SDR
        return 120.0

    def get_reference_white(self) -> float:
        """Get reference white level in cd/m2."""
        if self.reference_white is not None:
            return self.reference_white

        standard_name = self.standard.value
        if standard_name in LUMINANCE_STANDARDS:
            return LUMINANCE_STANDARDS[standard_name].get("reference_white",
                   LUMINANCE_STANDARDS[standard_name]["peak"])

        return self.get_peak_luminance()

    def get_black_level(self) -> float:
        """Get target black level in cd/m2."""
        if self.black_level is not None:
            return self.black_level

        standard_name = self.standard.value
        if standard_name in LUMINANCE_STANDARDS:
            return LUMINANCE_STANDARDS[standard_name]["min_black"]

        # Default for good LCD
        return 0.05

    def get_contrast_ratio(self) -> float:
        """Get target contrast ratio."""
        if self.target_contrast is not None:
            return self.target_contrast

        peak = self.get_peak_luminance()
        black = self.get_black_level()

        return calculate_contrast_ratio(peak, black)

    def get_dynamic_range_stops(self) -> float:
        """Get dynamic range in photographic stops."""
        contrast = self.get_contrast_ratio()
        if contrast <= 1:
            return 0.0
        return np.log2(contrast)

    def get_hdr_metadata(self) -> Dict:
        """Get HDR metadata for content creation."""
        peak = int(self.get_peak_luminance())

        max_cll = self.max_cll if self.max_cll else peak
        max_fall = self.max_fall if self.max_fall else int(peak * 0.4)

        return {
            "MaxCLL": max_cll,
            "MaxFALL": max_fall,
            "MinLuminance": self.get_black_level(),
            "MaxLuminance": peak,
            "ReferenceWhite": self.get_reference_white()
        }

    def is_hdr(self) -> bool:
        """Check if this is an HDR target."""
        if self.hdr_mode:
            return True

        hdr_standards = {
            LuminanceStandard.HDR10,
            LuminanceStandard.HDR10_PLUS,
            LuminanceStandard.DOLBY_VISION,
            LuminanceStandard.HLG,
            LuminanceStandard.HDR_REFERENCE
        }

        return self.standard in hdr_standards

    def verify(self, measured_peak: float, measured_black: float) -> Dict:
        """
        Verify measured luminance against target.

        Args:
            measured_peak: Measured peak luminance (cd/m2)
            measured_black: Measured black level (cd/m2)

        Returns:
            Verification results
        """
        target_peak = self.get_peak_luminance()
        target_black = self.get_black_level()
        target_contrast = self.get_contrast_ratio()

        measured_contrast = calculate_contrast_ratio(measured_peak, measured_black)

        # Calculate deviations
        peak_error = abs(measured_peak - target_peak) / target_peak * 100
        black_error = abs(measured_black - target_black) / max(target_black, 0.0001) * 100
        contrast_error = abs(measured_contrast - target_contrast) / target_contrast * 100

        # Pass/fail
        peak_pass = peak_error <= self.tolerance_percent
        contrast_pass = measured_contrast >= target_contrast * 0.8  # 80% of target contrast

        return {
            "target_peak": target_peak,
            "measured_peak": measured_peak,
            "peak_error_percent": peak_error,
            "peak_passed": peak_pass,

            "target_black": target_black,
            "measured_black": measured_black,
            "black_error_percent": black_error,

            "target_contrast": target_contrast,
            "measured_contrast": measured_contrast,
            "contrast_error_percent": contrast_error,
            "contrast_passed": contrast_pass,

            "dynamic_range_stops": self.get_dynamic_range_stops(),
            "measured_dr_stops": np.log2(measured_contrast) if measured_contrast > 1 else 0,

            "overall_passed": peak_pass and contrast_pass,
            "grade": self._grade_result(peak_error, contrast_error)
        }

    def _grade_result(self, peak_error: float, contrast_error: float) -> str:
        """Grade luminance calibration accuracy."""
        if peak_error < 2 and contrast_error < 5:
            return "Reference Grade"
        elif peak_error < 5 and contrast_error < 10:
            return "Professional"
        elif peak_error < 10 and contrast_error < 20:
            return "Consumer"
        else:
            return "Uncalibrated"

    def to_dict(self) -> Dict:
        """Serialize to dictionary."""
        return {
            "standard": self.standard.value,
            "peak_luminance": self.peak_luminance,
            "reference_white": self.reference_white,
            "black_level": self.black_level,
            "target_contrast": self.target_contrast,
            "hdr_mode": self.hdr_mode,
            "max_cll": self.max_cll,
            "max_fall": self.max_fall,
            "name": self.name,
            "description": self.description,
            # Computed values
            "computed_peak": self.get_peak_luminance(),
            "computed_black": self.get_black_level(),
            "computed_contrast": self.get_contrast_ratio(),
            "is_hdr": self.is_hdr()
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "LuminanceTarget":
        """Create from dictionary."""
        return cls(
            standard=LuminanceStandard(data.get("standard", "SDR General")),
            peak_luminance=data.get("peak_luminance"),
            reference_white=data.get("reference_white"),
            black_level=data.get("black_level"),
            target_contrast=data.get("target_contrast"),
            hdr_mode=data.get("hdr_mode", False),
            max_cll=data.get("max_cll"),
            max_fall=data.get("max_fall"),
            name=data.get("name", ""),
            description=data.get("description", "")
        )


@dataclass
class GrayscaleTarget:
    """
    Grayscale tracking target for luminance calibration.

    Defines expected luminance at each grayscale level based on gamma/EOTF.
    """
    gamma: float = 2.2
    peak_luminance: float = 100.0
    black_level: float = 0.0
    bt1886_mode: bool = False  # Use BT.1886 EOTF

    def get_luminance(self, level: float) -> float:
        """
        Get expected luminance for a grayscale level.

        Args:
            level: Input level 0-1

        Returns:
            Expected luminance in cd/m2
        """
        if self.bt1886_mode:
            # BT.1886 EOTF (accounts for black level)
            lw = self.peak_luminance
            lb = self.black_level

            # BT.1886 formula
            a = (lw ** (1/2.4) - lb ** (1/2.4)) ** 2.4
            b = lb ** (1/2.4) / (lw ** (1/2.4) - lb ** (1/2.4))

            luminance = a * max(level + b, 0) ** 2.4
        else:
            # Simple power law
            luminance = self.black_level + (self.peak_luminance - self.black_level) * (level ** self.gamma)

        return luminance

    def get_grayscale_ramp(self, steps: int = 21) -> List[Dict]:
        """Generate grayscale target ramp."""
        ramp = []
        for i in range(steps):
            level = i / (steps - 1)
            ramp.append({
                "level": level,
                "target_luminance": self.get_luminance(level),
                "level_percent": level * 100
            })
        return ramp


# =============================================================================
# Standard Presets
# =============================================================================

# SDR Broadcast
LUMINANCE_REC709 = LuminanceTarget(
    standard=LuminanceStandard.REC709_BROADCAST,
    name="Rec.709 Broadcast",
    description="EBU Tech 3320 broadcast reference (100 cd/m2)"
)

# Film Grading
LUMINANCE_FILM = LuminanceTarget(
    standard=LuminanceStandard.SMPTE_RP166,
    name="Film Grading (SMPTE RP 166)",
    description="Dark room film grading (35 cd/m2 / 10 fL)"
)

# DCI Cinema
LUMINANCE_DCI = LuminanceTarget(
    standard=LuminanceStandard.DCI_P3_CINEMA,
    name="DCI-P3 Cinema",
    description="Digital Cinema Initiative (48 cd/m2 / 14 fL)"
)

# HDR10 Mastering
LUMINANCE_HDR10 = LuminanceTarget(
    standard=LuminanceStandard.HDR10,
    hdr_mode=True,
    name="HDR10 Mastering",
    description="HDR10 mastering (1000 cd/m2 peak)"
)

# HDR10+ High-End
LUMINANCE_HDR10_PLUS = LuminanceTarget(
    standard=LuminanceStandard.HDR10_PLUS,
    peak_luminance=4000.0,
    hdr_mode=True,
    name="HDR10+ High-End",
    description="HDR10+ with dynamic metadata (4000 cd/m2 peak)"
)

# Dolby Vision
LUMINANCE_DOLBY_VISION = LuminanceTarget(
    standard=LuminanceStandard.DOLBY_VISION,
    hdr_mode=True,
    name="Dolby Vision Mastering",
    description="Dolby Vision reference (4000 cd/m2 peak)"
)

# Consumer SDR
LUMINANCE_CONSUMER_SDR = LuminanceTarget(
    standard=LuminanceStandard.CONSUMER_SDR,
    peak_luminance=250.0,
    name="Consumer SDR",
    description="Typical consumer SDR viewing (250 cd/m2)"
)

# Consumer HDR
LUMINANCE_CONSUMER_HDR = LuminanceTarget(
    standard=LuminanceStandard.CONSUMER_HDR,
    peak_luminance=600.0,
    hdr_mode=True,
    name="Consumer HDR",
    description="Typical consumer HDR viewing (600 cd/m2 peak)"
)


def get_luminance_presets() -> List[LuminanceTarget]:
    """Get list of standard luminance presets."""
    return [
        LUMINANCE_REC709,
        LUMINANCE_CONSUMER_SDR,
        LUMINANCE_DCI,
        LUMINANCE_FILM,
        LUMINANCE_HDR10,
        LUMINANCE_HDR10_PLUS,
        LUMINANCE_DOLBY_VISION,
        LUMINANCE_CONSUMER_HDR,
    ]


def get_sdr_presets() -> List[LuminanceTarget]:
    """Get SDR-only luminance presets."""
    return [p for p in get_luminance_presets() if not p.is_hdr()]


def get_hdr_presets() -> List[LuminanceTarget]:
    """Get HDR-only luminance presets."""
    return [p for p in get_luminance_presets() if p.is_hdr()]


def create_custom_luminance(
    peak: float,
    black: float = 0.0,
    reference_white: Optional[float] = None,
    hdr_mode: bool = False,
    name: str = "Custom"
) -> LuminanceTarget:
    """
    Create a custom luminance target.

    Args:
        peak: Peak luminance in cd/m2
        black: Black level in cd/m2
        reference_white: Reference white (defaults to peak for SDR, 203 for HDR)
        hdr_mode: HDR mode
        name: Display name

    Returns:
        LuminanceTarget
    """
    if reference_white is None:
        reference_white = 203.0 if hdr_mode else peak

    return LuminanceTarget(
        standard=LuminanceStandard.CUSTOM,
        peak_luminance=peak,
        black_level=black,
        reference_white=reference_white,
        hdr_mode=hdr_mode,
        name=name
    )


def calculate_recommended_luminance(
    viewing_distance_m: float,
    screen_diagonal_inches: float,
    ambient_lux: float = 50.0,
    hdr: bool = False
) -> Dict:
    """
    Calculate recommended luminance based on viewing conditions.

    Args:
        viewing_distance_m: Viewing distance in meters
        screen_diagonal_inches: Screen diagonal in inches
        ambient_lux: Ambient light level in lux
        hdr: HDR content

    Returns:
        Recommended luminance settings
    """
    # Screen area estimation
    screen_area = (screen_diagonal_inches * 0.0254) ** 2 * 0.5  # m2 approximate

    # Viewing angle factor
    view_angle = np.arctan((screen_diagonal_inches * 0.0254 / 2) / viewing_distance_m)

    # Base luminance recommendation
    if hdr:
        # HDR recommendation based on ITU-R BT.2408
        base_peak = 1000.0
        base_reference = 203.0

        # Adjust for ambient
        ambient_boost = min(ambient_lux / 50.0, 1.5)
        recommended_peak = base_peak * ambient_boost
        recommended_reference = base_reference * ambient_boost
    else:
        # SDR recommendation
        # EBU Tech 3320 with ambient compensation
        base_luminance = 100.0

        # Ambient light compensation (rough approximation)
        # In bright room, need more peak luminance
        ambient_factor = 1.0 + (ambient_lux / 200.0)
        recommended_peak = base_luminance * ambient_factor
        recommended_reference = recommended_peak

    return {
        "recommended_peak": min(recommended_peak, 4000 if hdr else 500),
        "recommended_reference_white": recommended_reference,
        "recommended_black": 0.005 if hdr else 0.05,
        "viewing_angle_degrees": np.degrees(view_angle) * 2,
        "ambient_lux": ambient_lux,
        "hdr_mode": hdr
    }
