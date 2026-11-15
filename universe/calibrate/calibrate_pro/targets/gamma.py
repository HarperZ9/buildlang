"""
Gamma/EOTF Targets - Professional gamma and electro-optical transfer function calibration.

Supports:
- Power law gamma (2.2, 2.4, custom)
- sRGB piecewise function
- BT.1886 (broadcast EOTF)
- PQ ST.2084 (HDR10, Dolby Vision)
- HLG (Hybrid Log-Gamma)
- L* (CIE perceptual)
- Adobe RGB (2.199)

For professional colorists, broadcast engineers, and enthusiasts.
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Optional, Tuple, Dict, List, Callable
from enum import Enum


class GammaPreset(Enum):
    """Standard gamma/EOTF presets."""
    # Power law
    POWER_18 = "Power 1.8"        # Legacy Mac
    POWER_20 = "Power 2.0"        # Linear approximation
    POWER_22 = "Power 2.2"        # Common display
    POWER_24 = "Power 2.4"        # Professional video
    POWER_26 = "Power 2.6"        # Dark room viewing

    # Standard curves
    SRGB = "sRGB"                 # IEC 61966-2-1
    BT1886 = "BT.1886"            # ITU-R BT.1886 broadcast
    ADOBE_RGB = "Adobe RGB"       # Adobe RGB (1998)
    L_STAR = "L*"                 # CIE L* perceptual

    # HDR
    PQ = "PQ (ST.2084)"           # Perceptual Quantizer HDR
    HLG = "HLG"                   # Hybrid Log-Gamma
    SLOG3 = "S-Log3"              # Sony S-Log3
    LOG_C = "Log C"               # ARRI Log C

    # Custom
    CUSTOM = "Custom"


# =============================================================================
# Standard EOTF Functions
# =============================================================================

def power_eotf(x: np.ndarray, gamma: float = 2.2) -> np.ndarray:
    """
    Simple power law EOTF.

    Args:
        x: Normalized signal (0-1)
        gamma: Power law exponent

    Returns:
        Normalized linear light (0-1)
    """
    x = np.clip(x, 0, 1)
    return np.power(x, gamma)


def power_oetf(L: np.ndarray, gamma: float = 2.2) -> np.ndarray:
    """
    Simple power law OETF (inverse of EOTF).

    Args:
        L: Normalized linear light (0-1)
        gamma: Power law exponent

    Returns:
        Normalized signal (0-1)
    """
    L = np.clip(L, 0, 1)
    return np.power(L, 1.0 / gamma)


def srgb_eotf(x: np.ndarray) -> np.ndarray:
    """
    sRGB EOTF (IEC 61966-2-1).

    Piecewise function with linear portion near black.

    Args:
        x: Normalized signal (0-1)

    Returns:
        Normalized linear light (0-1)
    """
    x = np.clip(x, 0, 1)
    return np.where(
        x <= 0.04045,
        x / 12.92,
        np.power((x + 0.055) / 1.055, 2.4)
    )


def srgb_oetf(L: np.ndarray) -> np.ndarray:
    """
    sRGB OETF (inverse of EOTF).

    Args:
        L: Normalized linear light (0-1)

    Returns:
        Normalized signal (0-1)
    """
    L = np.clip(L, 0, 1)
    return np.where(
        L <= 0.0031308,
        L * 12.92,
        1.055 * np.power(L, 1/2.4) - 0.055
    )


def bt1886_eotf(
    x: np.ndarray,
    L_W: float = 100.0,
    L_B: float = 0.0,
    gamma: float = 2.4
) -> np.ndarray:
    """
    BT.1886 EOTF (ITU-R BT.1886).

    Used for broadcast and professional video. Accounts for
    display black level to provide consistent appearance.

    Args:
        x: Normalized signal (0-1)
        L_W: Peak white luminance (cd/m2)
        L_B: Black luminance (cd/m2)
        gamma: Target gamma (typically 2.4)

    Returns:
        Absolute luminance (cd/m2)
    """
    x = np.clip(x, 0, 1)

    # BT.1886 parameters
    a = (L_W ** (1/gamma) - L_B ** (1/gamma)) ** gamma
    b = L_B ** (1/gamma) / (L_W ** (1/gamma) - L_B ** (1/gamma))

    # EOTF
    L = a * np.maximum(x + b, 0) ** gamma

    return L


def bt1886_oetf(
    L: np.ndarray,
    L_W: float = 100.0,
    L_B: float = 0.0,
    gamma: float = 2.4
) -> np.ndarray:
    """
    BT.1886 OETF (inverse of EOTF).

    Args:
        L: Absolute luminance (cd/m2)
        L_W: Peak white luminance (cd/m2)
        L_B: Black luminance (cd/m2)
        gamma: Target gamma

    Returns:
        Normalized signal (0-1)
    """
    L = np.clip(L, L_B, L_W)

    # Inverse parameters
    a = (L_W ** (1/gamma) - L_B ** (1/gamma)) ** gamma
    b = L_B ** (1/gamma) / (L_W ** (1/gamma) - L_B ** (1/gamma))

    # Inverse EOTF
    x = np.power(L / a, 1/gamma) - b

    return np.clip(x, 0, 1)


def pq_eotf(x: np.ndarray) -> np.ndarray:
    """
    PQ (Perceptual Quantizer) EOTF - ST.2084 / HDR10.

    Converts PQ signal to absolute luminance (0-10000 cd/m2).

    Args:
        x: Normalized PQ signal (0-1)

    Returns:
        Absolute luminance (cd/m2, 0-10000)
    """
    x = np.clip(x, 0, 1)

    # ST.2084 constants
    m1 = 2610 / 16384  # 0.1593017578125
    m2 = 2523 / 32 * 128  # 78.84375
    c1 = 3424 / 4096  # 0.8359375
    c2 = 2413 / 128  # 18.8515625
    c3 = 2392 / 128  # 18.6875

    # EOTF
    x_pow = np.power(x, 1/m2)
    num = np.maximum(x_pow - c1, 0)
    den = c2 - c3 * x_pow

    L = 10000 * np.power(num / den, 1/m1)

    return L


def pq_oetf(L: np.ndarray) -> np.ndarray:
    """
    PQ OETF (inverse of EOTF).

    Args:
        L: Absolute luminance (cd/m2, 0-10000)

    Returns:
        Normalized PQ signal (0-1)
    """
    L = np.clip(L, 0, 10000)
    L_norm = L / 10000

    # ST.2084 constants
    m1 = 2610 / 16384
    m2 = 2523 / 32 * 128
    c1 = 3424 / 4096
    c2 = 2413 / 128
    c3 = 2392 / 128

    # OETF
    L_pow = np.power(L_norm, m1)
    num = c1 + c2 * L_pow
    den = 1 + c3 * L_pow

    x = np.power(num / den, m2)

    return x


def hlg_eotf(x: np.ndarray, L_W: float = 1000.0, gamma: float = 1.2) -> np.ndarray:
    """
    HLG (Hybrid Log-Gamma) EOTF - ITU-R BT.2100.

    Converts HLG signal to absolute luminance.

    Args:
        x: Normalized HLG signal (0-1)
        L_W: Peak white luminance (cd/m2)
        gamma: System gamma (typically 1.2)

    Returns:
        Absolute luminance (cd/m2)
    """
    x = np.clip(x, 0, 1)

    # HLG OETF^-1 constants
    a = 0.17883277
    b = 0.28466892  # 1 - 4a
    c = 0.55991073  # 0.5 - a * ln(4a)

    # OETF^-1 (signal to scene light)
    E = np.where(
        x <= 0.5,
        (x ** 2) / 3,
        (np.exp((x - c) / a) + b) / 12
    )

    # OOTF (scene to display, with system gamma)
    L = L_W * np.power(E, gamma)

    return L


def hlg_oetf(L: np.ndarray, L_W: float = 1000.0, gamma: float = 1.2) -> np.ndarray:
    """
    HLG OETF (inverse of EOTF).

    Args:
        L: Absolute luminance (cd/m2)
        L_W: Peak white luminance (cd/m2)
        gamma: System gamma

    Returns:
        Normalized HLG signal (0-1)
    """
    L = np.clip(L, 0, L_W)

    # Inverse OOTF
    E = np.power(L / L_W, 1 / gamma)

    # HLG OETF constants
    a = 0.17883277
    b = 0.28466892
    c = 0.55991073

    # OETF
    x = np.where(
        E <= 1/12,
        np.sqrt(3 * E),
        a * np.log(12 * E - b) + c
    )

    return np.clip(x, 0, 1)


def l_star_eotf(x: np.ndarray) -> np.ndarray:
    """
    CIE L* EOTF (perceptually uniform).

    Args:
        x: Normalized signal (0-1, representing L*/100)

    Returns:
        Normalized linear light (0-1)
    """
    x = np.clip(x, 0, 1)

    # L* to Y
    L_star = x * 100

    return np.where(
        L_star > 8.0,
        np.power((L_star + 16) / 116, 3),
        L_star / 903.3
    )


def l_star_oetf(Y: np.ndarray) -> np.ndarray:
    """
    CIE L* OETF (inverse).

    Args:
        Y: Normalized linear light (0-1)

    Returns:
        Normalized L* signal (0-1)
    """
    Y = np.clip(Y, 0, 1)

    L_star = np.where(
        Y > 0.008856,
        116 * np.power(Y, 1/3) - 16,
        903.3 * Y
    )

    return L_star / 100


def slog3_eotf(x: np.ndarray) -> np.ndarray:
    """
    Sony S-Log3 EOTF.

    Args:
        x: S-Log3 signal

    Returns:
        Normalized linear light
    """
    x = np.clip(x, 0, 1)

    return np.where(
        x >= 171.2102946929 / 1023,
        np.power(10, (x * 1023 - 420) / 261.5) * (0.18 + 0.01) - 0.01,
        (x * 1023 - 95) * 0.01125000 / (171.2102946929 - 95)
    )


def log_c_eotf(x: np.ndarray) -> np.ndarray:
    """
    ARRI Log C EOTF (EI 800).

    Args:
        x: Log C signal

    Returns:
        Normalized linear light
    """
    x = np.clip(x, 0, 1)

    # Log C constants (EI 800)
    cut = 0.010591
    a = 5.555556
    b = 0.052272
    c = 0.247190
    d = 0.385537
    e = 5.367655
    f = 0.092809

    return np.where(
        x > e * cut + f,
        (np.power(10, (x - d) / c) - b) / a,
        (x - f) / e
    )


# =============================================================================
# Gamma Target Class
# =============================================================================

@dataclass
class GammaTarget:
    """
    Professional gamma/EOTF target specification.

    Supports all major transfer functions for SDR and HDR content.

    Attributes:
        preset: Standard gamma preset
        gamma_value: Power law exponent (for power presets)
        peak_luminance: Peak white for BT.1886/HLG (cd/m2)
        black_luminance: Black level for BT.1886 (cd/m2)
        hlg_system_gamma: System gamma for HLG
        tolerance_percent: Acceptable gamma deviation
    """
    preset: GammaPreset = GammaPreset.POWER_22
    gamma_value: float = 2.2
    peak_luminance: float = 100.0  # cd/m2 for BT.1886/HLG
    black_luminance: float = 0.0   # cd/m2 for BT.1886
    hlg_system_gamma: float = 1.2

    # Tolerance
    tolerance_percent: float = 3.0

    # Display
    name: str = ""
    description: str = ""

    def __post_init__(self):
        if not self.name:
            self.name = self.preset.value

        # Set gamma value for power presets
        if self.preset == GammaPreset.POWER_18:
            self.gamma_value = 1.8
        elif self.preset == GammaPreset.POWER_20:
            self.gamma_value = 2.0
        elif self.preset == GammaPreset.POWER_22:
            self.gamma_value = 2.2
        elif self.preset == GammaPreset.POWER_24:
            self.gamma_value = 2.4
        elif self.preset == GammaPreset.POWER_26:
            self.gamma_value = 2.6
        elif self.preset == GammaPreset.ADOBE_RGB:
            self.gamma_value = 2.199

    def get_eotf(self) -> Callable:
        """Get EOTF function for this target."""
        if self.preset in {GammaPreset.POWER_18, GammaPreset.POWER_20,
                           GammaPreset.POWER_22, GammaPreset.POWER_24,
                           GammaPreset.POWER_26, GammaPreset.CUSTOM,
                           GammaPreset.ADOBE_RGB}:
            return lambda x: power_eotf(x, self.gamma_value)

        elif self.preset == GammaPreset.SRGB:
            return srgb_eotf

        elif self.preset == GammaPreset.BT1886:
            return lambda x: bt1886_eotf(x, self.peak_luminance,
                                         self.black_luminance, 2.4) / self.peak_luminance

        elif self.preset == GammaPreset.L_STAR:
            return l_star_eotf

        elif self.preset == GammaPreset.PQ:
            return lambda x: pq_eotf(x) / 10000

        elif self.preset == GammaPreset.HLG:
            return lambda x: hlg_eotf(x, self.peak_luminance,
                                      self.hlg_system_gamma) / self.peak_luminance

        elif self.preset == GammaPreset.SLOG3:
            return slog3_eotf

        elif self.preset == GammaPreset.LOG_C:
            return log_c_eotf

        else:
            return lambda x: power_eotf(x, 2.2)

    def get_oetf(self) -> Callable:
        """Get OETF (inverse EOTF) function for this target."""
        if self.preset in {GammaPreset.POWER_18, GammaPreset.POWER_20,
                           GammaPreset.POWER_22, GammaPreset.POWER_24,
                           GammaPreset.POWER_26, GammaPreset.CUSTOM,
                           GammaPreset.ADOBE_RGB}:
            return lambda L: power_oetf(L, self.gamma_value)

        elif self.preset == GammaPreset.SRGB:
            return srgb_oetf

        elif self.preset == GammaPreset.BT1886:
            return lambda L: bt1886_oetf(L * self.peak_luminance,
                                         self.peak_luminance, self.black_luminance, 2.4)

        elif self.preset == GammaPreset.L_STAR:
            return l_star_oetf

        elif self.preset == GammaPreset.PQ:
            return lambda L: pq_oetf(L * 10000)

        elif self.preset == GammaPreset.HLG:
            return lambda L: hlg_oetf(L * self.peak_luminance,
                                      self.peak_luminance, self.hlg_system_gamma)

        else:
            return lambda L: power_oetf(L, 2.2)

    def get_target_curve(self, steps: int = 256) -> Tuple[np.ndarray, np.ndarray]:
        """
        Generate target gamma/EOTF curve.

        Args:
            steps: Number of points

        Returns:
            (input_levels, output_levels) both normalized 0-1
        """
        x = np.linspace(0, 1, steps)
        eotf = self.get_eotf()
        y = eotf(x)
        return x, y

    def is_hdr(self) -> bool:
        """Check if this is an HDR transfer function."""
        return self.preset in {GammaPreset.PQ, GammaPreset.HLG,
                               GammaPreset.SLOG3, GammaPreset.LOG_C}

    def calculate_effective_gamma(self, signal_level: float = 0.5) -> float:
        """
        Calculate effective power law gamma at a signal level.

        Args:
            signal_level: Input signal level (0-1)

        Returns:
            Effective gamma value
        """
        if signal_level <= 0 or signal_level >= 1:
            return self.gamma_value

        eotf = self.get_eotf()
        L = eotf(np.array([signal_level]))[0]

        if L <= 0:
            return self.gamma_value

        # Effective gamma = log(L) / log(signal)
        return np.log(L) / np.log(signal_level)

    def verify(self, measured_curve: List[Tuple[float, float]]) -> Dict:
        """
        Verify measured gamma curve against target.

        Args:
            measured_curve: List of (input_level, measured_output) tuples

        Returns:
            Verification results
        """
        eotf = self.get_eotf()

        errors = []
        results = []

        for level, measured in measured_curve:
            target = eotf(np.array([level]))[0]
            error = abs(measured - target) / max(target, 0.001) * 100

            errors.append(error)
            results.append({
                "level": level,
                "target": target,
                "measured": measured,
                "error_percent": error
            })

        avg_error = np.mean(errors)
        max_error = np.max(errors)

        return {
            "target_preset": self.preset.value,
            "target_gamma": self.gamma_value,
            "average_error_percent": avg_error,
            "max_error_percent": max_error,
            "points": results,
            "passed": avg_error <= self.tolerance_percent,
            "grade": self._grade_result(avg_error)
        }

    def _grade_result(self, avg_error: float) -> str:
        """Grade gamma tracking accuracy."""
        if avg_error < 1:
            return "Reference Grade"
        elif avg_error < 3:
            return "Professional"
        elif avg_error < 5:
            return "Consumer"
        else:
            return "Uncalibrated"

    def to_dict(self) -> Dict:
        """Serialize to dictionary."""
        return {
            "preset": self.preset.value,
            "gamma_value": self.gamma_value,
            "peak_luminance": self.peak_luminance,
            "black_luminance": self.black_luminance,
            "hlg_system_gamma": self.hlg_system_gamma,
            "name": self.name,
            "description": self.description,
            "is_hdr": self.is_hdr()
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "GammaTarget":
        """Create from dictionary."""
        return cls(
            preset=GammaPreset(data.get("preset", "Power 2.2")),
            gamma_value=data.get("gamma_value", 2.2),
            peak_luminance=data.get("peak_luminance", 100.0),
            black_luminance=data.get("black_luminance", 0.0),
            hlg_system_gamma=data.get("hlg_system_gamma", 1.2),
            name=data.get("name", ""),
            description=data.get("description", "")
        )


# =============================================================================
# Standard Presets
# =============================================================================

GAMMA_22 = GammaTarget(
    preset=GammaPreset.POWER_22,
    name="Gamma 2.2",
    description="Standard display gamma"
)

GAMMA_24 = GammaTarget(
    preset=GammaPreset.POWER_24,
    name="Gamma 2.4",
    description="Professional video, darker viewing"
)

GAMMA_SRGB = GammaTarget(
    preset=GammaPreset.SRGB,
    name="sRGB",
    description="IEC 61966-2-1 sRGB transfer function"
)

GAMMA_BT1886 = GammaTarget(
    preset=GammaPreset.BT1886,
    peak_luminance=100.0,
    black_luminance=0.0,
    name="BT.1886",
    description="ITU-R BT.1886 broadcast EOTF"
)

GAMMA_PQ = GammaTarget(
    preset=GammaPreset.PQ,
    name="PQ (ST.2084)",
    description="HDR10 Perceptual Quantizer"
)

GAMMA_HLG = GammaTarget(
    preset=GammaPreset.HLG,
    peak_luminance=1000.0,
    name="HLG",
    description="Hybrid Log-Gamma broadcast HDR"
)

GAMMA_L_STAR = GammaTarget(
    preset=GammaPreset.L_STAR,
    name="L* (Perceptual)",
    description="CIE L* perceptually uniform"
)


def get_gamma_presets() -> List[GammaTarget]:
    """Get list of standard gamma presets."""
    return [
        GAMMA_22,
        GAMMA_24,
        GAMMA_SRGB,
        GAMMA_BT1886,
        GAMMA_L_STAR,
        GAMMA_PQ,
        GAMMA_HLG,
        GammaTarget(preset=GammaPreset.POWER_18, name="Gamma 1.8 (Legacy Mac)"),
        GammaTarget(preset=GammaPreset.ADOBE_RGB, name="Adobe RGB"),
    ]


def get_sdr_presets() -> List[GammaTarget]:
    """Get SDR gamma presets."""
    return [p for p in get_gamma_presets() if not p.is_hdr()]


def get_hdr_presets() -> List[GammaTarget]:
    """Get HDR EOTF presets."""
    return [p for p in get_gamma_presets() if p.is_hdr()]


def create_custom_gamma(
    gamma: float,
    name: str = "Custom"
) -> GammaTarget:
    """Create a custom power law gamma target."""
    return GammaTarget(
        preset=GammaPreset.CUSTOM,
        gamma_value=gamma,
        name=name
    )


def create_bt1886_target(
    peak_luminance: float = 100.0,
    black_luminance: float = 0.0
) -> GammaTarget:
    """Create a BT.1886 target with specific luminance levels."""
    return GammaTarget(
        preset=GammaPreset.BT1886,
        peak_luminance=peak_luminance,
        black_luminance=black_luminance,
        name=f"BT.1886 ({peak_luminance:.0f} cd/m2)"
    )
