"""
Tone Mapping Engine

Comprehensive tone mapping for HDR to SDR conversion and
display adaptation. Supports multiple algorithms optimized
for different content types.

Algorithms:
- Reinhard (global/local)
- ACES (Academy Color Encoding System)
- Hable/Filmic (Uncharted 2 style)
- BT.2390 EETF (ITU-R BT.2390)
- Custom S-curve
"""

import numpy as np
from dataclasses import dataclass
from typing import Optional, Tuple, Callable, Dict, Any
from enum import Enum


# =============================================================================
# Tone Mapping Algorithms
# =============================================================================

class ToneMapOperator(Enum):
    """Available tone mapping operators."""
    LINEAR = "linear"           # No tone mapping (clipping)
    REINHARD = "reinhard"       # Reinhard global
    REINHARD_EXT = "reinhard_extended"  # Extended Reinhard
    ACES = "aces"               # ACES filmic
    HABLE = "hable"             # Hable/Uncharted 2 filmic
    BT2390 = "bt2390"           # ITU-R BT.2390 EETF
    SPLINE = "spline"           # Custom spline curve
    EXPONENTIAL = "exponential"  # Exponential roll-off


@dataclass
class ToneMapSettings:
    """Tone mapping configuration."""
    operator: ToneMapOperator = ToneMapOperator.BT2390

    # Source/target characteristics
    source_peak: float = 1000.0      # Source peak luminance (cd/m²)
    source_black: float = 0.0        # Source black level
    target_peak: float = 100.0       # Target peak luminance
    target_black: float = 0.0        # Target black level

    # Curve parameters
    knee_start: float = 0.5          # Where roll-off begins (0-1 of target)
    shoulder_strength: float = 0.5   # Roll-off strength
    mid_gray: float = 0.18           # Mid-gray reference
    white_clip: float = 1.0          # Maximum output value

    # Highlight handling
    highlight_desaturation: float = 0.3  # Desaturate bright areas
    preserve_hue: bool = True            # Maintain hue in highlights

    # Shadow handling
    shadow_contrast: float = 1.0     # Shadow contrast adjustment
    black_rolloff: float = 0.0       # Soft black clipping


# =============================================================================
# Core Tone Mapping Functions
# =============================================================================

def tone_map_linear(
    luminance: np.ndarray,
    source_peak: float,
    target_peak: float
) -> np.ndarray:
    """Simple linear scaling with clipping."""
    scale = target_peak / source_peak
    return np.clip(luminance * scale, 0, target_peak)


def tone_map_reinhard(
    luminance: np.ndarray,
    source_peak: float = 1000.0,
    mid_gray: float = 0.18
) -> np.ndarray:
    """
    Reinhard global tone mapping.

    Simple and fast, preserves relative luminance well.
    """
    # Normalize
    L = luminance / source_peak

    # Key value (geometric mean of scene)
    key = mid_gray

    # Scale to key
    Lm = (key / (np.mean(L) + 0.0001)) * L

    # Compress
    Ld = Lm / (1.0 + Lm)

    return Ld


def tone_map_reinhard_extended(
    luminance: np.ndarray,
    source_peak: float = 1000.0,
    target_peak: float = 100.0,
    white_point: float = None
) -> np.ndarray:
    """
    Extended Reinhard with white point control.

    Allows specifying where pure white occurs.
    """
    if white_point is None:
        white_point = source_peak

    L = luminance / source_peak
    Lw = white_point / source_peak

    # Extended formula with white point
    Ld = L * (1.0 + L / (Lw * Lw)) / (1.0 + L)

    return Ld * target_peak / source_peak


def tone_map_aces(
    rgb: np.ndarray,
    source_peak: float = 1000.0
) -> np.ndarray:
    """
    ACES filmic tone mapping.

    Industry-standard curve with good highlight roll-off
    and shadow detail preservation.
    """
    # Normalize to [0, 1] range
    x = rgb / source_peak

    # ACES parameters (sRGB approximation)
    a = 2.51
    b = 0.03
    c = 2.43
    d = 0.59
    e = 0.14

    # Apply curve
    result = (x * (a * x + b)) / (x * (c * x + d) + e)

    return np.clip(result, 0.0, 1.0)


def tone_map_hable(
    rgb: np.ndarray,
    source_peak: float = 1000.0,
    exposure_bias: float = 2.0
) -> np.ndarray:
    """
    Hable/Uncharted 2 filmic tone mapping.

    Natural-looking curve popular in games and film.
    """
    # Curve parameters (Uncharted 2)
    A = 0.15  # Shoulder strength
    B = 0.50  # Linear strength
    C = 0.10  # Linear angle
    D = 0.20  # Toe strength
    E = 0.02  # Toe numerator
    F = 0.30  # Toe denominator

    def hable_curve(x):
        return ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F

    x = rgb / source_peak * exposure_bias
    W = 11.2  # Linear white point

    numerator = hable_curve(x)
    denominator = hable_curve(W)

    return np.clip(numerator / denominator, 0.0, 1.0)


def tone_map_bt2390(
    luminance: np.ndarray,
    source_peak: float = 1000.0,
    target_peak: float = 100.0,
    source_black: float = 0.0,
    target_black: float = 0.0
) -> np.ndarray:
    """
    ITU-R BT.2390 EETF (Electro-Electro Transfer Function).

    Standard broadcast tone mapping with defined knee and shoulder.
    """
    from calibrate_pro.hdr.pq_st2084 import pq_eotf, pq_oetf

    # Convert to PQ domain
    E1 = pq_oetf(luminance)

    # Source/target PQ values
    LB = pq_oetf(np.array([source_black]))[0]
    LW = pq_oetf(np.array([source_peak]))[0]
    LMin = pq_oetf(np.array([target_black]))[0]
    LMax = pq_oetf(np.array([target_peak]))[0]

    # Normalize to [0, 1] in source range
    E1_norm = (E1 - LB) / (LW - LB)
    E1_norm = np.clip(E1_norm, 0.0, 1.0)

    # EETF parameters
    KS = 1.5 * LMax - 0.5  # Knee start
    KB = LMin  # Black offset

    # Spline-based mapping
    # Below knee: linear
    # Above knee: hermite spline roll-off

    E2 = np.zeros_like(E1_norm)

    # Knee region
    t = (E1_norm - KS) / (1.0 - KS)
    t = np.clip(t, 0.0, 1.0)

    # Hermite interpolation above knee
    above_knee = E1_norm > KS

    # Below knee - linear mapping
    E2[~above_knee] = E1_norm[~above_knee]

    # Above knee - spline
    P1 = KS
    P2 = 1.0
    T1 = 1.0  # Tangent at P1
    T2 = 0.0  # Tangent at P2 (horizontal)

    t_above = t[above_knee]
    h00 = 2 * t_above**3 - 3 * t_above**2 + 1
    h10 = t_above**3 - 2 * t_above**2 + t_above
    h01 = -2 * t_above**3 + 3 * t_above**2
    h11 = t_above**3 - t_above**2

    E2[above_knee] = h00 * P1 + h10 * (1 - KS) * T1 + h01 * LMax + h11 * (1 - KS) * T2

    # Scale to target range
    E3 = E2 * (LMax - LMin) + LMin

    # Convert back from PQ
    result = pq_eotf(E3)

    return np.clip(result, 0.0, target_peak)


def tone_map_exponential(
    luminance: np.ndarray,
    source_peak: float = 1000.0,
    target_peak: float = 100.0,
    exposure: float = 1.0
) -> np.ndarray:
    """
    Exponential tone mapping with soft roll-off.

    Good for preserving shadow detail.
    """
    L = luminance / source_peak * exposure

    # Exponential curve
    Ld = 1.0 - np.exp(-L)

    return Ld * target_peak


# =============================================================================
# RGB Tone Mapping with Hue Preservation
# =============================================================================

def tone_map_rgb(
    rgb: np.ndarray,
    settings: ToneMapSettings,
    preserve_hue: bool = True
) -> np.ndarray:
    """
    Apply tone mapping to RGB data with optional hue preservation.

    Args:
        rgb: Linear RGB values (shape: ..., 3)
        settings: Tone mapping settings
        preserve_hue: Preserve original hue during mapping

    Returns:
        Tone-mapped RGB
    """
    original_shape = rgb.shape
    rgb = np.atleast_2d(rgb)

    if preserve_hue or settings.preserve_hue:
        # Calculate luminance (using BT.2020 coefficients)
        luminance = 0.2627 * rgb[..., 0] + 0.6780 * rgb[..., 1] + 0.0593 * rgb[..., 2]

        # Tone map luminance
        if settings.operator == ToneMapOperator.REINHARD:
            mapped_lum = tone_map_reinhard(luminance, settings.source_peak)
        elif settings.operator == ToneMapOperator.REINHARD_EXT:
            mapped_lum = tone_map_reinhard_extended(
                luminance, settings.source_peak, settings.target_peak
            )
        elif settings.operator == ToneMapOperator.ACES:
            mapped_lum = tone_map_aces(luminance[..., np.newaxis], settings.source_peak)[..., 0]
        elif settings.operator == ToneMapOperator.HABLE:
            mapped_lum = tone_map_hable(luminance[..., np.newaxis], settings.source_peak)[..., 0]
        elif settings.operator == ToneMapOperator.BT2390:
            mapped_lum = tone_map_bt2390(
                luminance, settings.source_peak, settings.target_peak,
                settings.source_black, settings.target_black
            )
        elif settings.operator == ToneMapOperator.EXPONENTIAL:
            mapped_lum = tone_map_exponential(
                luminance, settings.source_peak, settings.target_peak
            )
        else:
            mapped_lum = tone_map_linear(luminance, settings.source_peak, settings.target_peak)

        # Scale RGB by luminance ratio
        ratio = np.where(luminance > 0, mapped_lum / (luminance + 0.0001), 0)
        result = rgb * ratio[..., np.newaxis]

        # Apply highlight desaturation
        if settings.highlight_desaturation > 0:
            desat_amount = settings.highlight_desaturation * np.clip(
                (mapped_lum - settings.knee_start * settings.target_peak) /
                ((1.0 - settings.knee_start) * settings.target_peak + 0.0001),
                0, 1
            )
            gray = mapped_lum[..., np.newaxis] * np.array([[[1, 1, 1]]])
            result = result * (1 - desat_amount[..., np.newaxis]) + gray * desat_amount[..., np.newaxis]

    else:
        # Per-channel tone mapping
        if settings.operator == ToneMapOperator.ACES:
            result = tone_map_aces(rgb, settings.source_peak) * settings.target_peak
        elif settings.operator == ToneMapOperator.HABLE:
            result = tone_map_hable(rgb, settings.source_peak) * settings.target_peak
        else:
            # Apply to each channel
            result = np.zeros_like(rgb)
            for c in range(3):
                if settings.operator == ToneMapOperator.REINHARD:
                    result[..., c] = tone_map_reinhard(rgb[..., c], settings.source_peak)
                elif settings.operator == ToneMapOperator.BT2390:
                    result[..., c] = tone_map_bt2390(
                        rgb[..., c], settings.source_peak, settings.target_peak
                    )
                else:
                    result[..., c] = tone_map_linear(
                        rgb[..., c], settings.source_peak, settings.target_peak
                    )

    # Clip to valid range
    result = np.clip(result, 0, settings.target_peak)

    # Restore original shape
    if len(original_shape) == 1:
        result = result[0]

    return result


# =============================================================================
# Tone Mapping LUT Generation
# =============================================================================

def generate_tonemap_1d_lut(
    settings: ToneMapSettings,
    size: int = 1024
) -> np.ndarray:
    """
    Generate 1D tone mapping LUT.

    Args:
        settings: Tone mapping settings
        size: LUT size

    Returns:
        1D LUT array
    """
    # Input luminance range
    lum_in = np.linspace(0, settings.source_peak, size)

    # Apply tone mapping
    if settings.operator == ToneMapOperator.REINHARD:
        lum_out = tone_map_reinhard(lum_in, settings.source_peak)
    elif settings.operator == ToneMapOperator.REINHARD_EXT:
        lum_out = tone_map_reinhard_extended(
            lum_in, settings.source_peak, settings.target_peak
        )
    elif settings.operator == ToneMapOperator.ACES:
        lum_out = tone_map_aces(lum_in[:, np.newaxis], settings.source_peak)[:, 0]
        lum_out *= settings.target_peak
    elif settings.operator == ToneMapOperator.HABLE:
        lum_out = tone_map_hable(lum_in[:, np.newaxis], settings.source_peak)[:, 0]
        lum_out *= settings.target_peak
    elif settings.operator == ToneMapOperator.BT2390:
        lum_out = tone_map_bt2390(
            lum_in, settings.source_peak, settings.target_peak,
            settings.source_black, settings.target_black
        )
    elif settings.operator == ToneMapOperator.EXPONENTIAL:
        lum_out = tone_map_exponential(lum_in, settings.source_peak, settings.target_peak)
    else:
        lum_out = tone_map_linear(lum_in, settings.source_peak, settings.target_peak)

    # Normalize to [0, 1]
    return np.clip(lum_out / settings.target_peak, 0, 1)


def generate_tonemap_3d_lut(
    settings: ToneMapSettings,
    size: int = 33
) -> np.ndarray:
    """
    Generate 3D tone mapping LUT.

    Args:
        settings: Tone mapping settings
        size: LUT size per dimension

    Returns:
        3D LUT array (size, size, size, 3)
    """
    # Create RGB grid
    coords = np.linspace(0, 1, size)
    r, g, b = np.meshgrid(coords, coords, coords, indexing='ij')

    # Stack and scale to source range
    rgb = np.stack([r, g, b], axis=-1) * settings.source_peak

    # Apply tone mapping
    mapped = tone_map_rgb(rgb.reshape(-1, 3), settings).reshape(size, size, size, 3)

    # Normalize output
    return np.clip(mapped / settings.target_peak, 0, 1)


# =============================================================================
# HDR to SDR Conversion
# =============================================================================

class HDRToSDRConverter:
    """
    Complete HDR to SDR conversion with color management.
    """

    def __init__(
        self,
        source_peak: float = 1000.0,
        target_peak: float = 100.0,
        operator: ToneMapOperator = ToneMapOperator.BT2390
    ):
        """
        Initialize converter.

        Args:
            source_peak: HDR peak luminance
            target_peak: SDR target luminance
            operator: Tone mapping algorithm
        """
        self.settings = ToneMapSettings(
            operator=operator,
            source_peak=source_peak,
            target_peak=target_peak
        )

    def convert_pq_to_sdr(
        self,
        pq_rgb: np.ndarray,
        apply_gamma: bool = True
    ) -> np.ndarray:
        """
        Convert PQ-encoded HDR to SDR.

        Args:
            pq_rgb: PQ signal values [0, 1]
            apply_gamma: Apply sRGB gamma encoding

        Returns:
            SDR RGB values [0, 1]
        """
        from calibrate_pro.hdr.pq_st2084 import pq_eotf

        # Decode PQ to linear light
        linear = pq_eotf(pq_rgb)

        # Tone map
        mapped = tone_map_rgb(linear, self.settings)

        # Normalize to [0, 1]
        result = mapped / self.settings.target_peak

        # Apply gamma encoding
        if apply_gamma:
            # sRGB gamma
            result = np.where(
                result <= 0.0031308,
                result * 12.92,
                1.055 * np.power(result, 1/2.4) - 0.055
            )

        return np.clip(result, 0, 1)

    def convert_hlg_to_sdr(
        self,
        hlg_rgb: np.ndarray,
        system_gamma: float = 1.2,
        apply_gamma: bool = True
    ) -> np.ndarray:
        """
        Convert HLG-encoded HDR to SDR.

        Args:
            hlg_rgb: HLG signal values [0, 1]
            system_gamma: HLG system gamma
            apply_gamma: Apply sRGB gamma encoding

        Returns:
            SDR RGB values [0, 1]
        """
        from calibrate_pro.hdr.hlg import hlg_eotf

        # Decode HLG to linear light
        linear = hlg_eotf(hlg_rgb, system_gamma) * self.settings.source_peak

        # Tone map
        mapped = tone_map_rgb(linear, self.settings)

        # Normalize
        result = mapped / self.settings.target_peak

        # Apply gamma
        if apply_gamma:
            result = np.where(
                result <= 0.0031308,
                result * 12.92,
                1.055 * np.power(result, 1/2.4) - 0.055
            )

        return np.clip(result, 0, 1)

    def get_lut(self, lut_type: str = "3d", size: int = 33) -> np.ndarray:
        """
        Get tone mapping LUT.

        Args:
            lut_type: "1d" or "3d"
            size: LUT size

        Returns:
            LUT array
        """
        if lut_type == "1d":
            return generate_tonemap_1d_lut(self.settings, size)
        else:
            return generate_tonemap_3d_lut(self.settings, size)


# =============================================================================
# Convenience Functions
# =============================================================================

def hdr_to_sdr(
    hdr_rgb: np.ndarray,
    source_peak: float = 1000.0,
    operator: ToneMapOperator = ToneMapOperator.BT2390
) -> np.ndarray:
    """
    Quick HDR to SDR conversion.

    Args:
        hdr_rgb: Linear HDR RGB (cd/m²)
        source_peak: Source peak luminance
        operator: Tone mapping algorithm

    Returns:
        SDR RGB [0, 1]
    """
    converter = HDRToSDRConverter(source_peak, 100.0, operator)
    return tone_map_rgb(hdr_rgb, converter.settings) / 100.0


def compare_operators(
    source_peak: float = 1000.0,
    target_peak: float = 100.0
) -> Dict[str, np.ndarray]:
    """
    Compare different tone mapping operators.

    Returns 1D LUTs for each operator for comparison.
    """
    results = {}

    for op in ToneMapOperator:
        settings = ToneMapSettings(
            operator=op,
            source_peak=source_peak,
            target_peak=target_peak
        )
        results[op.value] = generate_tonemap_1d_lut(settings, 256)

    return results
