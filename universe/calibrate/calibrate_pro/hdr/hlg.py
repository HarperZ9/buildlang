"""
HLG (Hybrid Log-Gamma) HDR Transfer Function

Implements the ARIB STD-B67 / BT.2100 HLG transfer function.
HLG is designed for broadcast with backwards compatibility to SDR displays.
"""

import numpy as np
from dataclasses import dataclass
from typing import Tuple, Optional

# =============================================================================
# HLG Constants (ARIB STD-B67 / BT.2100)
# =============================================================================

# HLG curve parameters
HLG_A = 0.17883277
HLG_B = 0.28466892    # 1 - 4*a
HLG_C = 0.55991073    # 0.5 - a*ln(4*a)

# Reference luminance
HLG_REFERENCE_WHITE = 1000.0  # Nominal peak white (cd/m2)
HLG_BLACK_LEVEL = 0.0

# System gamma values for different viewing environments
SYSTEM_GAMMA_NOMINAL = 1.2       # Reference viewing
SYSTEM_GAMMA_BRIGHT = 1.0        # Bright environment
SYSTEM_GAMMA_DARK = 1.4          # Dark environment

# =============================================================================
# HLG Transfer Functions
# =============================================================================

def hlg_oetf(scene_linear: np.ndarray) -> np.ndarray:
    """
    HLG Opto-Electronic Transfer Function (scene to signal).

    Converts scene-referred linear light to HLG signal.

    Args:
        scene_linear: Scene linear light values [0, 1]

    Returns:
        HLG signal values [0, 1]
    """
    scene_linear = np.asarray(scene_linear, dtype=np.float64)
    scene_linear = np.clip(scene_linear, 0.0, 1.0)

    signal = np.zeros_like(scene_linear)

    # Low-light segment (linear)
    mask = scene_linear <= 1.0 / 12.0
    signal[mask] = np.sqrt(3.0 * scene_linear[mask])

    # High-light segment (log)
    signal[~mask] = HLG_A * np.log(12.0 * scene_linear[~mask] - HLG_B) + HLG_C

    return np.clip(signal, 0.0, 1.0)


def hlg_oetf_inv(signal: np.ndarray) -> np.ndarray:
    """
    Inverse HLG OETF (signal to scene linear).

    Args:
        signal: HLG signal values [0, 1]

    Returns:
        Scene linear light values [0, 1]
    """
    signal = np.asarray(signal, dtype=np.float64)
    signal = np.clip(signal, 0.0, 1.0)

    scene = np.zeros_like(signal)

    # Low-light segment
    mask = signal <= 0.5
    scene[mask] = (signal[mask] ** 2) / 3.0

    # High-light segment
    scene[~mask] = (np.exp((signal[~mask] - HLG_C) / HLG_A) + HLG_B) / 12.0

    return np.clip(scene, 0.0, 1.0)


def hlg_eotf(signal: np.ndarray, system_gamma: float = SYSTEM_GAMMA_NOMINAL) -> np.ndarray:
    """
    HLG Electro-Optical Transfer Function (signal to display light).

    Converts HLG signal to display-referred linear light.

    Args:
        signal: HLG signal values [0, 1]
        system_gamma: System gamma for viewing environment (1.0-1.4)

    Returns:
        Display linear light values [0, 1]
    """
    signal = np.asarray(signal, dtype=np.float64)
    signal = np.clip(signal, 0.0, 1.0)

    # Inverse OETF to get scene linear
    scene = hlg_oetf_inv(signal)

    # Apply OOTF (system gamma)
    display = np.power(scene, system_gamma)

    return display


def hlg_eotf_inv(
    display_linear: np.ndarray,
    system_gamma: float = SYSTEM_GAMMA_NOMINAL
) -> np.ndarray:
    """
    Inverse HLG EOTF (display light to signal).

    Args:
        display_linear: Display linear light values [0, 1]
        system_gamma: System gamma

    Returns:
        HLG signal values [0, 1]
    """
    display_linear = np.asarray(display_linear, dtype=np.float64)
    display_linear = np.clip(display_linear, 0.0, 1.0)

    # Inverse OOTF
    scene = np.power(display_linear, 1.0 / system_gamma)

    # OETF
    return hlg_oetf(scene)


def hlg_ootf(
    scene_linear: np.ndarray,
    system_gamma: float = SYSTEM_GAMMA_NOMINAL,
    peak_luminance: float = HLG_REFERENCE_WHITE
) -> np.ndarray:
    """
    HLG Opto-Optical Transfer Function (scene to display light).

    Converts scene-referred to display-referred light, including
    system gamma for the viewing environment.

    Args:
        scene_linear: Scene linear light [0, 1]
        system_gamma: System gamma for environment
        peak_luminance: Display peak luminance (cd/m2)

    Returns:
        Display luminance (cd/m2)
    """
    scene_linear = np.asarray(scene_linear, dtype=np.float64)
    scene_linear = np.clip(scene_linear, 0.0, 1.0)

    # Calculate luminance component (Y from RGB)
    # Assuming equal energy white, Y = (R + G + B) / 3
    # For single channel, Y = value
    Y_s = scene_linear

    # Apply OOTF
    display = peak_luminance * np.power(Y_s, system_gamma - 1) * scene_linear

    return display

# =============================================================================
# HLG Calibration
# =============================================================================

@dataclass
class HLGDisplaySettings:
    """HLG display calibration settings."""
    system_gamma: float = SYSTEM_GAMMA_NOMINAL
    peak_luminance: float = HLG_REFERENCE_WHITE
    black_level: float = 0.0

    # Ambient light adaptation
    ambient_luminance: float = 5.0  # cd/m2 (viewing environment)

    def calculate_adaptive_gamma(self) -> float:
        """
        Calculate system gamma based on ambient light.

        BT.2100 recommends adjusting gamma based on viewing environment.
        """
        # Reference ambient is 5 cd/m2
        reference_ambient = 5.0

        # Gamma adjustment factor
        if self.ambient_luminance <= reference_ambient:
            return SYSTEM_GAMMA_NOMINAL + 0.2 * (1.0 - self.ambient_luminance / reference_ambient)
        else:
            return max(1.0, SYSTEM_GAMMA_NOMINAL - 0.1 * np.log10(self.ambient_luminance / reference_ambient))


def generate_hlg_calibration_lut(
    display_peak: float,
    display_black: float = 0.0,
    system_gamma: float = SYSTEM_GAMMA_NOMINAL,
    size: int = 33
) -> np.ndarray:
    """
    Generate HLG calibration 1D LUT.

    Args:
        display_peak: Display peak luminance (cd/m2)
        display_black: Display black level (cd/m2)
        system_gamma: System gamma for environment
        size: LUT size

    Returns:
        1D LUT array for HLG calibration
    """
    signal = np.linspace(0, 1, size)

    # Apply EOTF
    display = hlg_eotf(signal, system_gamma)

    # Scale to display range
    display_range = display_peak - display_black
    output = display_black + display * display_range

    # Normalize
    output = (output - display_black) / display_range

    return np.clip(output, 0, 1)


def calculate_hlg_eotf_error(
    measured_luminance: np.ndarray,
    signal_levels: np.ndarray,
    display_peak: float,
    system_gamma: float = SYSTEM_GAMMA_NOMINAL
) -> Tuple[np.ndarray, float]:
    """
    Calculate EOTF tracking error for HLG.

    Args:
        measured_luminance: Measured display luminance
        signal_levels: Input HLG signal levels [0, 1]
        display_peak: Display peak luminance
        system_gamma: System gamma

    Returns:
        (error_percentage, average_error)
    """
    # Target luminance
    target = hlg_eotf(signal_levels, system_gamma) * display_peak

    # Calculate percentage error
    errors = np.abs(measured_luminance - target) / np.maximum(target, 0.01) * 100
    errors = np.nan_to_num(errors, nan=0.0, posinf=100.0)

    return errors, float(np.mean(errors))


def generate_hlg_verification_patches(num_patches: int = 21) -> np.ndarray:
    """Generate HLG signal levels for verification."""
    return np.linspace(0, 1, num_patches)

# =============================================================================
# HLG to PQ Conversion
# =============================================================================

def hlg_to_pq(
    hlg_signal: np.ndarray,
    hlg_peak: float = 1000.0,
    system_gamma: float = SYSTEM_GAMMA_NOMINAL
) -> np.ndarray:
    """
    Convert HLG signal to PQ signal.

    Useful for displays that only support PQ but need to show HLG content.

    Args:
        hlg_signal: HLG signal values [0, 1]
        hlg_peak: HLG nominal peak luminance
        system_gamma: HLG system gamma

    Returns:
        PQ signal values [0, 1]
    """
    from calibrate_pro.hdr.pq_st2084 import pq_oetf

    # HLG to luminance
    display = hlg_eotf(hlg_signal, system_gamma) * hlg_peak

    # Luminance to PQ
    return pq_oetf(display)


def pq_to_hlg(
    pq_signal: np.ndarray,
    hlg_peak: float = 1000.0,
    system_gamma: float = SYSTEM_GAMMA_NOMINAL
) -> np.ndarray:
    """
    Convert PQ signal to HLG signal.

    Args:
        pq_signal: PQ signal values [0, 1]
        hlg_peak: Target HLG peak luminance
        system_gamma: Target HLG system gamma

    Returns:
        HLG signal values [0, 1]
    """
    from calibrate_pro.hdr.pq_st2084 import pq_eotf

    # PQ to luminance
    luminance = pq_eotf(pq_signal)

    # Normalize to HLG range
    display_normalized = np.clip(luminance / hlg_peak, 0, 1)

    # Display to HLG signal
    return hlg_eotf_inv(display_normalized, system_gamma)

# =============================================================================
# SDR Compatibility
# =============================================================================

def hlg_to_sdr(
    hlg_signal: np.ndarray,
    sdr_gamma: float = 2.2,
    desaturation: float = 0.0
) -> np.ndarray:
    """
    Convert HLG to SDR for backwards-compatible display.

    HLG is designed to look reasonable on SDR displays when
    interpreted as gamma 2.2.

    Args:
        hlg_signal: HLG signal values [0, 1]
        sdr_gamma: Target SDR gamma
        desaturation: Amount of desaturation for HDR->SDR (0-1)

    Returns:
        SDR signal values [0, 1]
    """
    # HLG can be displayed directly with gamma 2.2
    # The OETF was designed for this compatibility
    sdr = np.power(hlg_signal, sdr_gamma / 2.2)

    return np.clip(sdr, 0, 1)
