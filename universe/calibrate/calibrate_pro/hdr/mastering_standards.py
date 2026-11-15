"""
Professional Mastering Standards

Defines compliance specifications for major streaming and broadcast standards:
- Netflix HDR/Dolby Vision Mastering
- EBU Tech 3320 Grade 1 Monitors
- DCI P3 Cinema Projection
- Disney+/Apple TV+ Requirements
- BBC/ITU Broadcast Standards

These specifications are used for calibration target validation
and professional compliance verification.

Author: Zain Dana / Build
License: Fair-Source (see LICENSE)
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Dict, List, Tuple, Optional
from enum import Enum


class ComplianceLevel(Enum):
    """Compliance verification levels."""
    FULL = "full"          # Meets all requirements
    PARTIAL = "partial"    # Meets most requirements
    FAILED = "failed"      # Does not meet requirements


@dataclass
class MasteringSpec:
    """Base mastering specification."""
    name: str
    description: str

    # Colorimetry
    primaries: Dict[str, Tuple[float, float]]
    white_point: Tuple[float, float]

    # Luminance
    peak_luminance: float  # cd/m²
    min_luminance: float   # cd/m²
    sdr_reference: float = 100.0  # SDR white reference

    # Gamma/EOTF
    eotf: str = "pq"  # pq, hlg, gamma, bt1886
    gamma: float = 2.4

    # Tolerances
    primary_tolerance_de: float = 4.0  # Delta E
    white_point_tolerance_de: float = 3.0
    gamma_tolerance: float = 0.1
    luminance_tolerance_percent: float = 5.0

    # Viewing environment
    surround_luminance: float = 5.0  # cd/m²
    viewing_distance_heights: float = 3.0  # Picture heights

    def validate(self, measurements: Dict) -> Tuple[ComplianceLevel, List[str]]:
        """
        Validate measurements against this specification.

        Args:
            measurements: Dictionary with measured values

        Returns:
            (compliance_level, list_of_issues)
        """
        issues = []

        # Check luminance
        if 'peak_luminance' in measurements:
            peak = measurements['peak_luminance']
            target = self.peak_luminance
            error = abs(peak - target) / target * 100
            if error > self.luminance_tolerance_percent:
                issues.append(f"Peak luminance {peak:.0f} nits, expected {target:.0f} (±{self.luminance_tolerance_percent}%)")

        # Check white point
        if 'white_point_de' in measurements:
            de = measurements['white_point_de']
            if de > self.white_point_tolerance_de:
                issues.append(f"White point Delta E {de:.2f}, maximum {self.white_point_tolerance_de}")

        # Check primaries
        for color in ['red', 'green', 'blue']:
            key = f'{color}_primary_de'
            if key in measurements:
                de = measurements[key]
                if de > self.primary_tolerance_de:
                    issues.append(f"{color.title()} primary Delta E {de:.2f}, maximum {self.primary_tolerance_de}")

        # Check gamma
        if 'gamma' in measurements and self.eotf in ['gamma', 'bt1886']:
            gamma = measurements['gamma']
            if abs(gamma - self.gamma) > self.gamma_tolerance:
                issues.append(f"Gamma {gamma:.2f}, expected {self.gamma} (±{self.gamma_tolerance})")

        # Determine compliance level
        if len(issues) == 0:
            return ComplianceLevel.FULL, issues
        elif len(issues) <= 2:
            return ComplianceLevel.PARTIAL, issues
        else:
            return ComplianceLevel.FAILED, issues


# =============================================================================
# Netflix Mastering Standards
# =============================================================================

@dataclass
class NetflixMasteringProfile(MasteringSpec):
    """
    Netflix HDR Mastering Requirements.

    Netflix requires Dolby Vision for HDR originals, with P3-D65 as the
    working color space (NOT Rec.2020).

    Reference monitors:
    - Sony BVM-HX310 (1000+ nits)
    - Canon DP-V2421 (1000+ nits)
    - Flanders Scientific XM310K (3000+ nits)
    """

    def __init__(self):
        super().__init__(
            name="Netflix HDR",
            description="Netflix HDR/Dolby Vision Mastering Spec",

            # P3-D65 primaries (NOT Rec.2020)
            primaries={
                'red': (0.680, 0.320),
                'green': (0.265, 0.690),
                'blue': (0.150, 0.060)
            },
            white_point=(0.3127, 0.3290),  # D65

            # Luminance
            peak_luminance=1000.0,  # Minimum mastering peak
            min_luminance=0.0001,   # Target for OLED
            sdr_reference=100.0,

            # PQ EOTF
            eotf="pq",

            # Tight tolerances for professional mastering
            primary_tolerance_de=2.0,
            white_point_tolerance_de=2.0,
            gamma_tolerance=0.05,
            luminance_tolerance_percent=3.0,

            # Viewing environment
            surround_luminance=5.0,
            viewing_distance_heights=3.0
        )

    # Netflix viewing environment specs
    VIEWING_ENVIRONMENT = {
        "surround_illuminant": "D65",
        "viewing_distance_hd": "3-3.2 picture heights",
        "viewing_distance_uhd": "1.5-1.6 picture heights",
        "horizontal_fov_min": 90,  # degrees
        "vertical_fov_min": 60,    # degrees
        "ambient_light_max": 10,   # cd/m² reflected off screen
    }

    # Required reference monitors
    APPROVED_MONITORS = [
        "Sony BVM-HX310",
        "Sony BVM-X310",
        "Canon DP-V2421",
        "Canon DP-V3120",
        "Flanders Scientific XM310K",
        "Dolby Pulsar",
        "Dolby Maui",
    ]


# =============================================================================
# EBU Grade 1 Standards
# =============================================================================

@dataclass
class EBUGrade1Profile(MasteringSpec):
    """
    EBU Tech 3320 Grade 1 Monitor Specifications.

    The European Broadcasting Union standard for broadcast mastering monitors.

    SDR Grade 1:
    - Luminance: 70-100 nits adjustable
    - Black level: <0.05 nits
    - Contrast: >2000:1 sequential
    - Gamma: 2.4 ± 0.10
    - Primary tolerance: 4 ΔE(u'v')

    HDR Grade 1:
    - Luminance: 0.005 - 1000 nits
    - Simultaneous contrast: 10,000:1
    - Gamut: >90% BT.2020
    """

    def __init__(self, mode: str = "sdr"):
        if mode == "sdr":
            super().__init__(
                name="EBU Grade 1 SDR",
                description="EBU Tech 3320 Grade 1 SDR Broadcast Monitor",

                # BT.709/sRGB primaries
                primaries={
                    'red': (0.640, 0.330),
                    'green': (0.300, 0.600),
                    'blue': (0.150, 0.060)
                },
                white_point=(0.3127, 0.3290),

                peak_luminance=100.0,
                min_luminance=0.05,  # <0.05 nits black
                sdr_reference=100.0,

                eotf="bt1886",
                gamma=2.4,

                primary_tolerance_de=4.0,  # In u'v' units originally
                white_point_tolerance_de=3.0,
                gamma_tolerance=0.10,
                luminance_tolerance_percent=5.0,

                surround_luminance=5.0,
                viewing_distance_heights=3.2
            )
        else:  # HDR mode
            super().__init__(
                name="EBU Grade 1 HDR",
                description="EBU Tech 3320 Grade 1 HDR Broadcast Monitor",

                # BT.2020 primaries
                primaries={
                    'red': (0.708, 0.292),
                    'green': (0.170, 0.797),
                    'blue': (0.131, 0.046)
                },
                white_point=(0.3127, 0.3290),

                peak_luminance=1000.0,
                min_luminance=0.005,
                sdr_reference=203.0,  # HLG/PQ reference white

                eotf="pq",  # or "hlg"

                primary_tolerance_de=4.0,
                white_point_tolerance_de=3.0,
                gamma_tolerance=0.05,
                luminance_tolerance_percent=3.0,

                surround_luminance=5.0,
                viewing_distance_heights=3.2
            )

    # EBU Grade 1 specific requirements
    REQUIREMENTS = {
        "sdr": {
            "contrast_ratio_min": 2000,
            "viewing_angle_min": 176,  # degrees
            "uniformity_variation_max": 10,  # percent
            "color_uniformity_de_max": 3.0,
            "temporal_stability_de_max": 0.5,
        },
        "hdr": {
            "contrast_ratio_min": 10000,
            "bt2020_coverage_min": 90,  # percent
            "pq_tracking_error_max": 3,  # percent
        }
    }


# =============================================================================
# DCI Cinema Standards
# =============================================================================

@dataclass
class DCIMasteringProfile(MasteringSpec):
    """
    DCI P3 Cinema Projection Standards.

    Digital Cinema Initiatives specifications for theatrical projection.

    - Color space: DCI-P3 with DCI white (x=0.314, y=0.351)
    - Luminance: 48 cd/m² (14 fL) ± 10%
    - Gamma: 2.6
    """

    def __init__(self):
        super().__init__(
            name="DCI P3 Cinema",
            description="DCI Digital Cinema Projection Standard",

            # DCI-P3 primaries
            primaries={
                'red': (0.680, 0.320),
                'green': (0.265, 0.690),
                'blue': (0.150, 0.060)
            },
            # DCI white point (NOT D65!)
            white_point=(0.314, 0.351),

            # Cinema luminance (14 foot-lamberts)
            peak_luminance=48.0,
            min_luminance=0.0,
            sdr_reference=48.0,

            eotf="gamma",
            gamma=2.6,

            primary_tolerance_de=4.0,
            white_point_tolerance_de=2.0,
            gamma_tolerance=0.05,
            luminance_tolerance_percent=10.0,

            surround_luminance=0.0,  # Dark cinema
            viewing_distance_heights=1.5
        )

    # DCI specific requirements
    REQUIREMENTS = {
        "luminance_fl": 14.0,  # foot-lamberts
        "luminance_tolerance": 0.10,  # ±10%
        "contrast_ratio_min": 2000,
        "color_gamut_coverage_min": 95,  # percent of P3
        "uniformity_center_to_edge": 0.80,  # 80% minimum
    }


# =============================================================================
# Additional Streaming Standards
# =============================================================================

@dataclass
class DisneyPlusProfile(MasteringSpec):
    """Disney+ HDR Mastering Requirements."""

    def __init__(self):
        super().__init__(
            name="Disney+ HDR",
            description="Disney+ HDR10/Dolby Vision Mastering",

            primaries={
                'red': (0.680, 0.320),
                'green': (0.265, 0.690),
                'blue': (0.150, 0.060)
            },
            white_point=(0.3127, 0.3290),

            peak_luminance=1000.0,
            min_luminance=0.0001,
            sdr_reference=100.0,

            eotf="pq",

            primary_tolerance_de=3.0,
            white_point_tolerance_de=2.0,
            gamma_tolerance=0.05,
            luminance_tolerance_percent=5.0,

            surround_luminance=5.0,
            viewing_distance_heights=3.0
        )


@dataclass
class AppleTVProfile(MasteringSpec):
    """Apple TV+ HDR Mastering Requirements."""

    def __init__(self):
        super().__init__(
            name="Apple TV+ HDR",
            description="Apple TV+ Dolby Vision Mastering",

            # P3-D65
            primaries={
                'red': (0.680, 0.320),
                'green': (0.265, 0.690),
                'blue': (0.150, 0.060)
            },
            white_point=(0.3127, 0.3290),

            peak_luminance=1000.0,
            min_luminance=0.0001,
            sdr_reference=100.0,

            eotf="pq",

            primary_tolerance_de=2.5,
            white_point_tolerance_de=2.0,
            gamma_tolerance=0.05,
            luminance_tolerance_percent=3.0,

            surround_luminance=5.0,
            viewing_distance_heights=3.0
        )


@dataclass
class BBCBroadcastProfile(MasteringSpec):
    """BBC HDR Broadcast Requirements (HLG)."""

    def __init__(self):
        super().__init__(
            name="BBC HLG",
            description="BBC HLG Broadcast Mastering",

            # BT.2020 primaries
            primaries={
                'red': (0.708, 0.292),
                'green': (0.170, 0.797),
                'blue': (0.131, 0.046)
            },
            white_point=(0.3127, 0.3290),

            peak_luminance=1000.0,
            min_luminance=0.01,
            sdr_reference=203.0,  # HLG nominal white

            eotf="hlg",
            gamma=1.2,  # System gamma

            primary_tolerance_de=4.0,
            white_point_tolerance_de=3.0,
            gamma_tolerance=0.10,
            luminance_tolerance_percent=5.0,

            surround_luminance=5.0,
            viewing_distance_heights=3.0
        )


# =============================================================================
# Validation Functions
# =============================================================================

def validate_mastering_compliance(
    measurements: Dict,
    standard: str = "netflix"
) -> Tuple[ComplianceLevel, List[str], MasteringSpec]:
    """
    Validate calibration measurements against a mastering standard.

    Args:
        measurements: Dictionary with calibration measurements:
            - peak_luminance: float (cd/m²)
            - min_luminance: float (cd/m²)
            - white_point_de: float (Delta E from target)
            - red_primary_de: float
            - green_primary_de: float
            - blue_primary_de: float
            - gamma: float (measured gamma)
            - eotf_error: float (EOTF tracking error %)

        standard: Standard to validate against:
            - "netflix": Netflix HDR
            - "ebu_sdr": EBU Grade 1 SDR
            - "ebu_hdr": EBU Grade 1 HDR
            - "dci": DCI Cinema
            - "disney": Disney+
            - "apple": Apple TV+
            - "bbc": BBC HLG

    Returns:
        (compliance_level, list_of_issues, spec_used)
    """
    standards = {
        "netflix": NetflixMasteringProfile(),
        "ebu_sdr": EBUGrade1Profile("sdr"),
        "ebu_hdr": EBUGrade1Profile("hdr"),
        "dci": DCIMasteringProfile(),
        "disney": DisneyPlusProfile(),
        "apple": AppleTVProfile(),
        "bbc": BBCBroadcastProfile(),
    }

    if standard not in standards:
        raise ValueError(f"Unknown standard: {standard}. Valid options: {list(standards.keys())}")

    spec = standards[standard]
    level, issues = spec.validate(measurements)

    return level, issues, spec


def get_recommended_targets(use_case: str) -> Dict[str, MasteringSpec]:
    """
    Get recommended calibration targets for a use case.

    Args:
        use_case: One of:
            - "streaming_hdr": General streaming HDR mastering
            - "broadcast_hdr": Broadcast HDR (HLG primary)
            - "cinema": Digital cinema projection
            - "prosumer": High-end consumer HDR display

    Returns:
        Dictionary of recommended specifications
    """
    if use_case == "streaming_hdr":
        return {
            "primary": NetflixMasteringProfile(),
            "alt_1": DisneyPlusProfile(),
            "alt_2": AppleTVProfile(),
        }
    elif use_case == "broadcast_hdr":
        return {
            "primary": EBUGrade1Profile("hdr"),
            "alt_1": BBCBroadcastProfile(),
        }
    elif use_case == "cinema":
        return {
            "primary": DCIMasteringProfile(),
        }
    elif use_case == "prosumer":
        return {
            "primary": NetflixMasteringProfile(),
            "sdr": EBUGrade1Profile("sdr"),
        }
    else:
        raise ValueError(f"Unknown use case: {use_case}")


def generate_compliance_report(
    measurements: Dict,
    standards: List[str] = None
) -> Dict:
    """
    Generate a comprehensive compliance report against multiple standards.

    Args:
        measurements: Calibration measurements
        standards: List of standards to check (default: all)

    Returns:
        Report dictionary with compliance status for each standard
    """
    if standards is None:
        standards = ["netflix", "ebu_sdr", "ebu_hdr", "dci", "disney", "apple", "bbc"]

    report = {
        "measurements": measurements,
        "standards_checked": [],
        "summary": {
            "full_compliance": [],
            "partial_compliance": [],
            "failed": []
        }
    }

    for std in standards:
        try:
            level, issues, spec = validate_mastering_compliance(measurements, std)

            result = {
                "standard": std,
                "name": spec.name,
                "compliance": level.value,
                "issues": issues
            }
            report["standards_checked"].append(result)

            if level == ComplianceLevel.FULL:
                report["summary"]["full_compliance"].append(std)
            elif level == ComplianceLevel.PARTIAL:
                report["summary"]["partial_compliance"].append(std)
            else:
                report["summary"]["failed"].append(std)

        except Exception as e:
            report["standards_checked"].append({
                "standard": std,
                "error": str(e)
            })

    return report
