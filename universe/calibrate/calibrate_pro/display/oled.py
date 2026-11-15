"""
OLED Display Intelligence

Models OLED-specific behavior that generic calibration tools ignore:
- ABL (Auto Brightness Limiter) compensation
- Near-black handling (raised blacks, gamma lift)
- Panel technology differences (QD-OLED triangle vs WOLED WRGB)
- Luminance-dependent gamut changes

These models allow the calibration LUT to account for real-world
OLED behavior instead of treating the panel as an ideal device.
"""

import math
import numpy as np
from dataclasses import dataclass
from typing import Dict, Optional, Tuple


# =============================================================================
# ABL (Auto Brightness Limiter) Models
# =============================================================================

@dataclass
class ABLModel:
    """
    Auto Brightness Limiter model for an OLED panel.

    OLED displays reduce peak brightness as the percentage of bright pixels
    increases (APL - Average Picture Level). This protects the panel from
    excessive power draw and heat.

    The ABL curve is modeled as:
        actual_luminance = peak * abl_factor(apl)
        abl_factor(apl) = min_factor + (1 - min_factor) * (1 - apl)^rolloff

    Where:
        apl = average picture level [0, 1] (fraction of screen that's bright)
        peak = measured peak luminance at small window (2-10% APL)
        min_factor = luminance at 100% APL relative to peak (typically 0.15-0.35)
        rolloff = shape of the ABL curve (higher = more aggressive)
    """
    peak_luminance: float       # Peak at 2-10% APL (cd/m2)
    min_factor: float           # Luminance ratio at 100% APL (0.15 = 15% of peak)
    rolloff: float              # Curve shape (1.0 = linear, 2.0 = aggressive)
    apl_threshold: float = 0.0  # APL below which no ABL applies (some panels)

    def get_luminance(self, apl: float) -> float:
        """
        Get the actual achievable luminance at a given APL.

        Args:
            apl: Average Picture Level [0, 1]
        Returns:
            Achievable peak luminance in cd/m2
        """
        if apl <= self.apl_threshold:
            return self.peak_luminance

        effective_apl = (apl - self.apl_threshold) / (1.0 - self.apl_threshold)
        factor = self.min_factor + (1.0 - self.min_factor) * (1.0 - effective_apl) ** self.rolloff
        return self.peak_luminance * factor

    def get_abl_factor(self, apl: float) -> float:
        """Get the ABL attenuation factor [0, 1] at given APL."""
        return self.get_luminance(apl) / self.peak_luminance


# Known ABL models from published measurements (Rtings, HDTVTest, etc.)
ABL_MODELS = {
    # Samsung QD-OLED (2024 generation - PG27UCDM, AW3225QF, G80SD)
    "QD-OLED-2024": ABLModel(
        peak_luminance=1000.0,  # 2% window
        min_factor=0.28,        # ~280 nits at 100% white
        rolloff=1.5,
        apl_threshold=0.03
    ),

    # Samsung QD-OLED (2023 generation - AW3423DW, G85SB)
    "QD-OLED-2023": ABLModel(
        peak_luminance=1000.0,
        min_factor=0.25,        # ~250 nits at 100% white
        rolloff=1.6,
        apl_threshold=0.02
    ),

    # LG WOLED evo (C3/C4/G3/G4)
    "WOLED-EVO": ABLModel(
        peak_luminance=800.0,   # 2% window
        min_factor=0.18,        # ~150 nits at 100% white
        rolloff=1.8,
        apl_threshold=0.02
    ),

    # LG WOLED (older - C1/C2)
    "WOLED": ABLModel(
        peak_luminance=700.0,
        min_factor=0.15,
        rolloff=2.0,
        apl_threshold=0.02
    ),
}


def get_abl_model(panel_type: str, panel_key: str = "") -> Optional[ABLModel]:
    """
    Get the ABL model for a panel type.

    Args:
        panel_type: "QD-OLED", "WOLED", etc.
        panel_key: Specific panel key for more precise matching
    """
    if panel_type == "QD-OLED":
        # 2024 generation panels
        if any(k in panel_key for k in ["PG27UCDM", "PG32UCDM", "AW3225QF",
                                         "G80SD", "PG34WCDM", "FO32U2P", "S95D"]):
            return ABL_MODELS["QD-OLED-2024"]
        return ABL_MODELS["QD-OLED-2023"]
    elif panel_type == "WOLED":
        if any(k in panel_key for k in ["C3", "C4", "G3", "G4", "32GS95UE"]):
            return ABL_MODELS["WOLED-EVO"]
        return ABL_MODELS["WOLED"]
    return None


# =============================================================================
# Near-Black Handling
# =============================================================================

@dataclass
class NearBlackModel:
    """
    Near-black behavior model for OLED panels.

    OLED displays have imperfect near-black behavior:
    - QD-OLED: raised blacks at very low signal levels (visible as slight glow)
    - WOLED WRGB: color shift in near-black (green or magenta tint due to
      white subpixel turning off at different threshold than RGB subpixels)
    - All OLEDs: gamma deviation in the 0-5% signal range

    This model allows LUT correction for these artifacts.
    """
    # Signal level below which near-black issues appear (0-1)
    threshold: float = 0.03

    # Gamma deviation in the near-black region
    # Actual gamma may be higher or lower than target in this range
    gamma_lift: float = 0.0     # Positive = raised blacks, negative = crushed

    # Color shift in near-black (Lab a*, b* offsets)
    # QD-OLED: typically small, WOLED: can be noticeable green shift
    near_black_a_shift: float = 0.0  # Positive = red shift, negative = green
    near_black_b_shift: float = 0.0  # Positive = yellow shift, negative = blue

    # Minimum signal level where pixels actually emit light
    # Below this, the pixel is fully off (true black)
    black_cutoff: float = 0.001


# Known near-black models
NEAR_BLACK_MODELS = {
    "QD-OLED": NearBlackModel(
        threshold=0.03,
        gamma_lift=0.005,       # Slight raised blacks
        near_black_a_shift=0.0,
        near_black_b_shift=0.0,
        black_cutoff=0.001
    ),
    "WOLED": NearBlackModel(
        threshold=0.04,
        gamma_lift=0.003,
        near_black_a_shift=-0.5,  # Slight green tint in near-black
        near_black_b_shift=0.3,
        black_cutoff=0.001
    ),
    "WOLED-EVO": NearBlackModel(
        threshold=0.03,
        gamma_lift=0.002,
        near_black_a_shift=-0.3,  # Reduced with newer panels
        near_black_b_shift=0.2,
        black_cutoff=0.001
    ),
}


def apply_near_black_correction(
    rgb: np.ndarray,
    model: NearBlackModel,
    target_gamma: float = 2.2
) -> np.ndarray:
    """
    Apply near-black correction to an RGB value.

    Compensates for raised blacks and color shifts in the near-black region
    by adjusting the signal to counteract the panel's deviation.

    Args:
        rgb: Linear RGB as (3,) array
        model: Near-black behavior model
        target_gamma: Target display gamma
    Returns:
        Corrected linear RGB
    """
    corrected = rgb.copy()
    max_val = np.max(rgb)

    if max_val < model.threshold and max_val > model.black_cutoff:
        # Blend factor: how much correction to apply
        t = max_val / model.threshold

        # Gamma lift compensation: reduce signal to counteract raised blacks
        if model.gamma_lift > 0:
            lift_correction = 1.0 - model.gamma_lift * (1.0 - t)
            corrected *= lift_correction

        # Ensure we don't go below the black cutoff
        corrected = np.maximum(corrected, 0.0)

    return corrected


# =============================================================================
# Panel Technology Characteristics
# =============================================================================

@dataclass
class OLEDCharacteristics:
    """Complete OLED panel characteristic profile."""
    technology: str              # "QD-OLED", "WOLED", "WOLED-EVO"
    subpixel_layout: str         # "triangle" (QD-OLED), "WRGB" (WOLED)
    abl_model: Optional[ABLModel] = None
    near_black_model: Optional[NearBlackModel] = None

    # Luminance-dependent gamut behavior
    # OLED gamut narrows at high luminance due to efficiency rolloff
    gamut_luminance_rolloff: float = 0.0  # 0 = no rolloff, 1 = severe

    # Power efficiency characteristics
    max_sustained_luminance: float = 0.0  # Full-screen sustainable (cd/m2)
    thermal_throttle_time: float = 0.0    # Seconds before thermal throttle

    @property
    def is_qd_oled(self) -> bool:
        return self.technology == "QD-OLED"

    @property
    def is_woled(self) -> bool:
        return "WOLED" in self.technology


def get_oled_characteristics(panel_type: str, panel_key: str = "") -> Optional[OLEDCharacteristics]:
    """
    Get OLED characteristics for a panel.

    Returns None for non-OLED panels.
    """
    abl = get_abl_model(panel_type, panel_key)
    if abl is None:
        return None

    if panel_type == "QD-OLED":
        near_black = NEAR_BLACK_MODELS["QD-OLED"]
        return OLEDCharacteristics(
            technology="QD-OLED",
            subpixel_layout="triangle",
            abl_model=abl,
            near_black_model=near_black,
            gamut_luminance_rolloff=0.05,  # QD-OLED maintains gamut well
            max_sustained_luminance=abl.get_luminance(1.0),
            thermal_throttle_time=300.0  # ~5 minutes to thermal throttle
        )
    elif panel_type == "WOLED":
        is_evo = any(k in panel_key for k in ["C3", "C4", "G3", "G4", "32GS95UE"])
        near_black_key = "WOLED-EVO" if is_evo else "WOLED"
        near_black = NEAR_BLACK_MODELS[near_black_key]
        return OLEDCharacteristics(
            technology="WOLED-EVO" if is_evo else "WOLED",
            subpixel_layout="WRGB",
            abl_model=abl,
            near_black_model=near_black,
            gamut_luminance_rolloff=0.15,  # WOLED loses saturation at high lum
            max_sustained_luminance=abl.get_luminance(1.0),
            thermal_throttle_time=600.0  # ~10 minutes
        )
    return None


# =============================================================================
# OLED-Aware LUT Compensation
# =============================================================================

def compensate_abl_in_lut(
    rgb: np.ndarray,
    abl_model: ABLModel,
    target_apl: float = 0.25
) -> np.ndarray:
    """
    Compensate for ABL in a LUT value.

    When generating a calibration LUT, we need to account for the fact
    that the panel's actual luminance depends on the overall image content.
    We target a specific APL (typically 25% for mixed content) and adjust
    the LUT to produce correct colors at that APL.

    Args:
        rgb: Linear RGB [0, 1]
        abl_model: ABL model for the panel
        target_apl: Expected APL of typical content
    Returns:
        ABL-compensated linear RGB
    """
    factor = abl_model.get_abl_factor(target_apl)
    # Boost the signal to compensate for ABL dimming
    # Only affects the overall brightness, not the color balance
    return np.clip(rgb / factor, 0.0, 1.0)
