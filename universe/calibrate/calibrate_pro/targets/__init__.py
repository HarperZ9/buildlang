"""
Calibration Targets - Professional calibration target specifications.

Provides comprehensive target settings for:
- White point (CCT, xy, Duv, standard illuminants)
- Luminance (SDR/HDR peak, black level, contrast)
- Gamma/EOTF (power law, sRGB, BT.1886, PQ, HLG)
- Gamut (sRGB, DCI-P3, BT.2020, Adobe RGB, etc.)

For professional colorists, broadcast engineers, and enthusiasts.
"""

# White Point Targets
from calibrate_pro.targets.whitepoint import (
    WhitepointPreset,
    WhitepointTarget,
    ILLUMINANT_XY,
    ILLUMINANT_CCT,
    cct_to_xy,
    xy_to_cct,
    planckian_locus_xy,
    daylight_locus_xy,
    apply_duv_offset,
    xy_to_XYZ,
    XYZ_to_xy,
    get_whitepoint_presets,
    create_custom_whitepoint,
    # Standard presets
    WHITEPOINT_D50,
    WHITEPOINT_D65,
    WHITEPOINT_DCI,
    WHITEPOINT_ACES,
    WHITEPOINT_D75,
)

# Luminance Targets
from calibrate_pro.targets.luminance import (
    LuminanceStandard,
    BlackLevelStandard,
    LuminanceTarget,
    GrayscaleTarget,
    LUMINANCE_STANDARDS,
    nits_to_footlamberts,
    footlamberts_to_nits,
    calculate_contrast_ratio,
    calculate_black_level,
    get_luminance_presets,
    get_sdr_presets as get_sdr_luminance_presets,
    get_hdr_presets as get_hdr_luminance_presets,
    create_custom_luminance,
    calculate_recommended_luminance,
    # Standard presets
    LUMINANCE_REC709,
    LUMINANCE_FILM,
    LUMINANCE_DCI,
    LUMINANCE_HDR10,
    LUMINANCE_HDR10_PLUS,
    LUMINANCE_DOLBY_VISION,
    LUMINANCE_CONSUMER_SDR,
    LUMINANCE_CONSUMER_HDR,
)

# Gamma/EOTF Targets
from calibrate_pro.targets.gamma import (
    GammaPreset,
    GammaTarget,
    # EOTF functions
    power_eotf,
    power_oetf,
    srgb_eotf,
    srgb_oetf,
    bt1886_eotf,
    bt1886_oetf,
    pq_eotf,
    pq_oetf,
    hlg_eotf,
    hlg_oetf,
    l_star_eotf,
    l_star_oetf,
    slog3_eotf,
    log_c_eotf,
    get_gamma_presets,
    get_sdr_presets as get_sdr_gamma_presets,
    get_hdr_presets as get_hdr_gamma_presets,
    create_custom_gamma,
    create_bt1886_target,
    # Standard presets
    GAMMA_22,
    GAMMA_24,
    GAMMA_SRGB,
    GAMMA_BT1886,
    GAMMA_PQ,
    GAMMA_HLG,
    GAMMA_L_STAR,
)

# Gamut Targets
from calibrate_pro.targets.gamut import (
    GamutPreset,
    GamutTarget,
    ColorPrimaries,
    calculate_gamut_coverage,
    calculate_gamut_volume_coverage,
    get_gamut_presets,
    get_sdr_presets as get_sdr_gamut_presets,
    get_wide_gamut_presets,
    create_custom_gamut,
    get_gamut_comparison,
    # Standard primaries
    PRIMARIES_SRGB,
    PRIMARIES_REC709,
    PRIMARIES_DCI_P3,
    PRIMARIES_DCI_P3_THEATER,
    PRIMARIES_DISPLAY_P3,
    PRIMARIES_ADOBE_RGB,
    PRIMARIES_BT2020,
    PRIMARIES_PROPHOTO,
    PRIMARIES_ACES_CG,
    GAMUT_PRIMARIES,
    GAMUT_AREAS,
    # Standard presets
    GAMUT_SRGB,
    GAMUT_DCI_P3,
    GAMUT_DCI_P3_THEATER,
    GAMUT_DISPLAY_P3,
    GAMUT_ADOBE_RGB,
    GAMUT_BT2020,
    GAMUT_PROPHOTO,
    GAMUT_ACES_CG,
)


# =============================================================================
# Unified Calibration Target
# =============================================================================

from dataclasses import dataclass, field
from typing import Optional, Dict


@dataclass
class CalibrationTargetProfile:
    """
    Complete calibration target profile combining all target types.

    This is the main class users should interact with to specify
    complete calibration targets for professional workflows.
    """
    name: str = "Default"
    description: str = ""

    # Individual targets
    whitepoint: WhitepointTarget = field(default_factory=lambda: WHITEPOINT_D65)
    luminance: LuminanceTarget = field(default_factory=lambda: LUMINANCE_REC709)
    gamma: GammaTarget = field(default_factory=lambda: GAMMA_22)
    gamut: GamutTarget = field(default_factory=lambda: GAMUT_SRGB)

    def is_hdr(self) -> bool:
        """Check if this is an HDR target profile."""
        return self.luminance.is_hdr() or self.gamma.is_hdr()

    def get_summary(self) -> Dict:
        """Get summary of all target settings."""
        return {
            "name": self.name,
            "description": self.description,
            "whitepoint": {
                "preset": self.whitepoint.preset.value,
                "cct": self.whitepoint.get_cct(),
                "xy": self.whitepoint.get_xy()
            },
            "luminance": {
                "preset": self.luminance.standard.value,
                "peak": self.luminance.get_peak_luminance(),
                "black": self.luminance.get_black_level(),
                "contrast": self.luminance.get_contrast_ratio(),
                "hdr": self.luminance.is_hdr()
            },
            "gamma": {
                "preset": self.gamma.preset.value,
                "value": self.gamma.gamma_value,
                "hdr": self.gamma.is_hdr()
            },
            "gamut": {
                "preset": self.gamut.preset.value,
                "wide_gamut": self.gamut.is_wide_gamut(),
                "area": self.gamut.get_gamut_area()
            }
        }

    def to_dict(self) -> Dict:
        """Serialize to dictionary."""
        return {
            "name": self.name,
            "description": self.description,
            "whitepoint": self.whitepoint.to_dict(),
            "luminance": self.luminance.to_dict(),
            "gamma": self.gamma.to_dict(),
            "gamut": self.gamut.to_dict()
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "CalibrationTargetProfile":
        """Create from dictionary."""
        return cls(
            name=data.get("name", "Custom"),
            description=data.get("description", ""),
            whitepoint=WhitepointTarget.from_dict(data.get("whitepoint", {})),
            luminance=LuminanceTarget.from_dict(data.get("luminance", {})),
            gamma=GammaTarget.from_dict(data.get("gamma", {})),
            gamut=GamutTarget.from_dict(data.get("gamut", {}))
        )


# =============================================================================
# Standard Calibration Profiles
# =============================================================================

# sRGB / Web Standard
PROFILE_SRGB = CalibrationTargetProfile(
    name="sRGB Web Standard",
    description="Standard sRGB for web and consumer content",
    whitepoint=WHITEPOINT_D65,
    luminance=LUMINANCE_CONSUMER_SDR,
    gamma=GAMMA_SRGB,
    gamut=GAMUT_SRGB
)

# Rec.709 Broadcast
PROFILE_REC709 = CalibrationTargetProfile(
    name="Rec.709 Broadcast",
    description="EBU Grade 1 broadcast reference",
    whitepoint=WHITEPOINT_D65,
    luminance=LUMINANCE_REC709,
    gamma=GAMMA_BT1886,
    gamut=GAMUT_SRGB
)

# DCI-P3 Cinema
PROFILE_DCI_P3 = CalibrationTargetProfile(
    name="DCI-P3 Cinema",
    description="Digital Cinema Initiative standard",
    whitepoint=WHITEPOINT_DCI,
    luminance=LUMINANCE_DCI,
    gamma=GAMMA_26 if 'GAMMA_26' in dir() else GammaTarget(preset=GammaPreset.POWER_26),
    gamut=GAMUT_DCI_P3_THEATER
)

# HDR10 Mastering
PROFILE_HDR10 = CalibrationTargetProfile(
    name="HDR10 Mastering",
    description="HDR10 professional mastering",
    whitepoint=WHITEPOINT_D65,
    luminance=LUMINANCE_HDR10,
    gamma=GAMMA_PQ,
    gamut=GAMUT_BT2020
)

# Photography (Adobe RGB / D50)
PROFILE_PHOTOGRAPHY = CalibrationTargetProfile(
    name="Photography",
    description="Adobe RGB with D50 for photography",
    whitepoint=WHITEPOINT_D50,
    luminance=LUMINANCE_CONSUMER_SDR,
    gamma=GammaTarget(preset=GammaPreset.ADOBE_RGB),
    gamut=GAMUT_ADOBE_RGB
)

# Film Grading
PROFILE_FILM_GRADING = CalibrationTargetProfile(
    name="Film Grading",
    description="SMPTE RP 166 film grading environment",
    whitepoint=WHITEPOINT_D65,
    luminance=LUMINANCE_FILM,
    gamma=GAMMA_24,
    gamut=GAMUT_DCI_P3
)


def get_profile_presets() -> list:
    """Get list of standard calibration profiles."""
    return [
        PROFILE_SRGB,
        PROFILE_REC709,
        PROFILE_DCI_P3,
        PROFILE_HDR10,
        PROFILE_PHOTOGRAPHY,
        PROFILE_FILM_GRADING,
    ]


__all__ = [
    # White Point
    'WhitepointPreset', 'WhitepointTarget',
    'WHITEPOINT_D50', 'WHITEPOINT_D65', 'WHITEPOINT_DCI', 'WHITEPOINT_ACES',
    'cct_to_xy', 'xy_to_cct', 'get_whitepoint_presets', 'create_custom_whitepoint',

    # Luminance
    'LuminanceStandard', 'LuminanceTarget', 'GrayscaleTarget',
    'LUMINANCE_REC709', 'LUMINANCE_HDR10', 'LUMINANCE_DOLBY_VISION',
    'get_luminance_presets', 'create_custom_luminance',

    # Gamma
    'GammaPreset', 'GammaTarget',
    'GAMMA_22', 'GAMMA_24', 'GAMMA_SRGB', 'GAMMA_BT1886', 'GAMMA_PQ', 'GAMMA_HLG',
    'pq_eotf', 'pq_oetf', 'hlg_eotf', 'hlg_oetf', 'bt1886_eotf', 'bt1886_oetf',
    'get_gamma_presets', 'create_custom_gamma', 'create_bt1886_target',

    # Gamut
    'GamutPreset', 'GamutTarget', 'ColorPrimaries',
    'GAMUT_SRGB', 'GAMUT_DCI_P3', 'GAMUT_BT2020', 'GAMUT_ADOBE_RGB',
    'PRIMARIES_SRGB', 'PRIMARIES_DCI_P3', 'PRIMARIES_BT2020',
    'get_gamut_presets', 'create_custom_gamut', 'calculate_gamut_coverage',

    # Unified Profile
    'CalibrationTargetProfile',
    'PROFILE_SRGB', 'PROFILE_REC709', 'PROFILE_DCI_P3', 'PROFILE_HDR10',
    'PROFILE_PHOTOGRAPHY', 'PROFILE_FILM_GRADING',
    'get_profile_presets',
]
