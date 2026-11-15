"""
Advanced Perceptual Color Models for HDR and Wide Color Gamut

This module implements state-of-the-art color appearance models that surpass
CIELAB/CIEDE2000 for HDR and WCG applications:

- CAM16-UCS: CIE 2016 Color Appearance Model (Uniform Color Space)
- Jzazbz: Perceptually uniform for HDR up to 10,000 nits
- ICtCp: Dolby/ITU-R BT.2100 standard for HDR video

These models are critical for accurate color difference calculation in HDR
where traditional CIELAB breaks down above 100 nits.

Author: Zain Dana / Build
License: Fair-Source (see LICENSE)
"""

import numpy as np
from dataclasses import dataclass
from typing import Tuple, Union, Optional
import math

# =============================================================================
# Constants and Matrices
# =============================================================================

# D65 white point XYZ (normalized Y=1)
D65_XYZ = np.array([0.95047, 1.0, 1.08883])

# sRGB to XYZ (D65)
SRGB_TO_XYZ = np.array([
    [0.4124564, 0.3575761, 0.1804375],
    [0.2126729, 0.7151522, 0.0721750],
    [0.0193339, 0.1191920, 0.9503041]
], dtype=np.float64)

XYZ_TO_SRGB = np.linalg.inv(SRGB_TO_XYZ)

# BT.2020 to XYZ (D65)
BT2020_TO_XYZ = np.array([
    [0.6369580, 0.1446169, 0.1688810],
    [0.2627002, 0.6779981, 0.0593017],
    [0.0000000, 0.0280727, 1.0609851]
], dtype=np.float64)

XYZ_TO_BT2020 = np.linalg.inv(BT2020_TO_XYZ)

# =============================================================================
# ST.2084 (PQ) Transfer Function
# =============================================================================

# PQ constants (SMPTE ST 2084)
PQ_M1 = 2610.0 / 16384.0  # 0.1593017578125
PQ_M2 = 2523.0 / 32.0 * 128.0  # 78.84375
PQ_C1 = 3424.0 / 4096.0  # 0.8359375
PQ_C2 = 2413.0 / 128.0  # 18.8515625
PQ_C3 = 2392.0 / 128.0  # 18.6875


def pq_eotf(E: np.ndarray) -> np.ndarray:
    """
    PQ (ST.2084) Electro-Optical Transfer Function.

    Converts PQ encoded signal [0, 1] to absolute luminance [0, 10000] nits.

    Args:
        E: PQ encoded signal in [0, 1] range

    Returns:
        Absolute luminance in cd/m² (nits), range [0, 10000]
    """
    E = np.asarray(E, dtype=np.float64)
    E = np.clip(E, 0, 1)

    E_pow = np.power(E, 1.0 / PQ_M2)
    num = np.maximum(E_pow - PQ_C1, 0)
    den = PQ_C2 - PQ_C3 * E_pow

    # Avoid division by zero
    den = np.maximum(den, 1e-10)

    return 10000.0 * np.power(num / den, 1.0 / PQ_M1)


def pq_oetf(Y: np.ndarray) -> np.ndarray:
    """
    PQ (ST.2084) Opto-Electronic Transfer Function (inverse EOTF).

    Converts absolute luminance [0, 10000] nits to PQ encoded signal [0, 1].

    Args:
        Y: Absolute luminance in cd/m² (nits)

    Returns:
        PQ encoded signal in [0, 1] range
    """
    Y = np.asarray(Y, dtype=np.float64)
    Y = np.clip(Y, 0, 10000)

    Ym = np.power(Y / 10000.0, PQ_M1)
    num = PQ_C1 + PQ_C2 * Ym
    den = 1.0 + PQ_C3 * Ym

    return np.power(num / den, PQ_M2)


# =============================================================================
# CAM16 Color Appearance Model
# =============================================================================

@dataclass
class CAM16ViewingConditions:
    """
    CAM16 viewing conditions parameters.

    Attributes:
        L_A: Adapting luminance in cd/m² (typically 64 for average surround)
        Y_b: Relative background luminance (typically 20)
        surround: Surround type ('average', 'dim', 'dark')
        discounting: Whether to discount illuminant (False for displays)
    """
    L_A: float = 64.0  # Adapting luminance
    Y_b: float = 20.0  # Background luminance
    surround: str = 'average'  # 'average', 'dim', 'dark'
    discounting: bool = False

    def __post_init__(self):
        # Surround parameters (c, Nc, F)
        surround_params = {
            'average': (0.69, 1.0, 1.0),
            'dim': (0.59, 0.95, 0.9),
            'dark': (0.525, 0.8, 0.8)
        }
        self.c, self.Nc, self.F = surround_params.get(self.surround, (0.69, 1.0, 1.0))

        # Chromatic induction factor
        k = 1.0 / (5.0 * self.L_A + 1.0)
        k4 = k ** 4
        self.F_L = 0.2 * k4 * (5.0 * self.L_A) + 0.1 * (1.0 - k4) ** 2 * (5.0 * self.L_A) ** (1.0/3.0)

        # Background induction factor
        self.n = self.Y_b / 100.0
        self.z = 1.48 + np.sqrt(self.n)
        self.N_bb = 0.725 * (1.0 / self.n) ** 0.2
        self.N_cb = self.N_bb

        # Degree of adaptation
        if self.discounting:
            self.D = 1.0
        else:
            self.D = self.F * (1.0 - (1.0 / 3.6) * np.exp((-self.L_A - 42.0) / 92.0))
            self.D = np.clip(self.D, 0, 1)


# CAM16 transformation matrix (Hunt-Pointer-Estevez with D65 adaptation)
CAM16_M = np.array([
    [ 0.401288,  0.650173, -0.051461],
    [-0.250268,  1.204414,  0.045854],
    [-0.002079,  0.048952,  0.953127]
], dtype=np.float64)

CAM16_M_INV = np.linalg.inv(CAM16_M)


class CAM16:
    """
    CAM16 Color Appearance Model (CIE 2016).

    Most versatile color appearance model for wide color gamut applications
    up to 1,611 cd/m². Excels at predicting small color differences.

    Used in Google's Material Design HCT color system.

    Reference:
        CIE 159:2004 "A Colour Appearance Model for Colour Management Systems: CIECAM02"
        Updated parameters for CAM16 (Li, Luo, et al. 2017)
    """

    def __init__(self, viewing_conditions: CAM16ViewingConditions = None):
        """
        Initialize CAM16 with viewing conditions.

        Args:
            viewing_conditions: CAM16ViewingConditions instance (default: average surround)
        """
        if viewing_conditions is None:
            viewing_conditions = CAM16ViewingConditions()
        self.vc = viewing_conditions

        # Pre-compute adapted white point
        self._compute_white_adaptation()

    def _compute_white_adaptation(self):
        """Pre-compute white point adaptation values."""
        # Transform D65 white to CAM16 space
        RGB_w = CAM16_M @ D65_XYZ

        # Apply degree of adaptation
        D = self.vc.D
        self.D_RGB = D * D65_XYZ[1] / RGB_w + (1 - D)

        # Adapted white point
        RGB_wc = self.D_RGB * RGB_w

        # Post-adaptation nonlinear compression
        self.RGB_aw = self._nonlinear_adaptation(RGB_wc)

        # Achromatic response of white
        self.A_w = (2.0 * self.RGB_aw[0] + self.RGB_aw[1] +
                   0.05 * self.RGB_aw[2] - 0.305) * self.vc.N_bb

    def _nonlinear_adaptation(self, RGB: np.ndarray) -> np.ndarray:
        """Apply CAM16 nonlinear adaptation."""
        F_L = self.vc.F_L

        # Handle negative values for out-of-gamut colors
        sign = np.sign(RGB)
        RGB_abs = np.abs(RGB)

        adapted = sign * 400.0 * (F_L * RGB_abs / 100.0) ** 0.42 / \
                  (27.13 + (F_L * RGB_abs / 100.0) ** 0.42) + 0.1

        return adapted

    def _nonlinear_adaptation_inv(self, RGB_a: np.ndarray) -> np.ndarray:
        """Inverse CAM16 nonlinear adaptation."""
        F_L = self.vc.F_L

        # Handle the offset
        RGB_a_adj = RGB_a - 0.1
        sign = np.sign(RGB_a_adj)
        RGB_a_abs = np.abs(RGB_a_adj)

        # Inverse of the nonlinear function
        RGB = sign * (100.0 / F_L) * \
              np.power(27.13 * RGB_a_abs / (400.0 - RGB_a_abs), 1.0 / 0.42)

        return RGB

    def xyz_to_cam16(self, xyz: np.ndarray) -> dict:
        """
        Convert XYZ to CAM16 correlates.

        Args:
            xyz: XYZ values as (3,) or (N, 3) array (D65 reference white)

        Returns:
            Dictionary with CAM16 correlates:
            - J: Lightness
            - C: Chroma
            - h: Hue angle (degrees)
            - M: Colorfulness
            - s: Saturation
            - Q: Brightness
            - a: Red-green component
            - b: Yellow-blue component
        """
        xyz = np.asarray(xyz, dtype=np.float64)
        single = xyz.ndim == 1
        if single:
            xyz = xyz.reshape(1, 3)

        results = []
        for i in range(len(xyz)):
            result = self._forward_single(xyz[i])
            results.append(result)

        if single:
            return results[0]
        return results

    def _forward_single(self, xyz: np.ndarray) -> dict:
        """Convert single XYZ to CAM16."""
        # Step 1: Transform to CAM16 RGB
        RGB = CAM16_M @ xyz

        # Step 2: Chromatic adaptation
        RGB_c = self.D_RGB * RGB

        # Step 3: Nonlinear compression
        RGB_a = self._nonlinear_adaptation(RGB_c)

        # Step 4: Calculate appearance correlates
        a = RGB_a[0] - 12.0 * RGB_a[1] / 11.0 + RGB_a[2] / 11.0
        b = (RGB_a[0] + RGB_a[1] - 2.0 * RGB_a[2]) / 9.0

        # Hue angle
        h = np.degrees(np.arctan2(b, a)) % 360.0

        # Eccentricity factor
        h_rad = np.radians(h)
        e_t = 0.25 * (np.cos(h_rad + 2.0) + 3.8)

        # Achromatic response
        A = (2.0 * RGB_a[0] + RGB_a[1] + 0.05 * RGB_a[2] - 0.305) * self.vc.N_bb

        # Lightness
        J = 100.0 * np.power(A / self.A_w, self.vc.c * self.vc.z)

        # Brightness
        Q = (4.0 / self.vc.c) * np.sqrt(J / 100.0) * \
            (self.A_w + 4.0) * np.power(self.vc.F_L, 0.25)

        # Chroma
        t = (50000.0 / 13.0 * self.vc.Nc * self.vc.N_cb * e_t *
             np.sqrt(a**2 + b**2) / (RGB_a[0] + RGB_a[1] + 21.0 * RGB_a[2] / 20.0))

        C = t ** 0.9 * np.sqrt(J / 100.0) * np.power(1.64 - 0.29 ** self.vc.n, 0.73)

        # Colorfulness
        M = C * np.power(self.vc.F_L, 0.25)

        # Saturation
        s = 100.0 * np.sqrt(M / Q) if Q > 0 else 0.0

        return {
            'J': J,
            'C': C,
            'h': h,
            'M': M,
            's': s,
            'Q': Q,
            'a': a,
            'b': b
        }

    def cam16_to_xyz(self, J: float, C: float, h: float) -> np.ndarray:
        """
        Convert CAM16 JCh to XYZ.

        Args:
            J: Lightness [0, 100]
            C: Chroma [0, ~100]
            h: Hue angle in degrees [0, 360]

        Returns:
            XYZ values (D65 reference white)
        """
        # Handle edge case: J = 0 means black
        if J <= 0:
            return np.array([0.0, 0.0, 0.0])

        # Hue in radians
        h_rad = np.radians(h)
        cos_h = np.cos(h_rad)
        sin_h = np.sin(h_rad)

        # Eccentricity factor
        e_t = 0.25 * (np.cos(h_rad + 2.0) + 3.8)

        # Achromatic response
        A = self.A_w * np.power(J / 100.0, 1.0 / (self.vc.c * self.vc.z))

        # p2 is related to achromatic response
        p2 = A / self.vc.N_bb + 0.305

        # Handle achromatic case (C = 0)
        if C <= 0:
            a = 0.0
            b = 0.0
        else:
            # Calculate t from C and J
            t = np.power(
                C / (np.sqrt(J / 100.0) * np.power(1.64 - 0.29 ** self.vc.n, 0.73)),
                1.0 / 0.9
            )

            # p1 for chroma calculation
            p1 = (50000.0 / 13.0) * self.vc.Nc * self.vc.N_cb * e_t / t

            # Derived formula for gamma = sqrt(a^2 + b^2):
            # From forward: t = (50000/13 * Nc * Ncb * e_t * gamma) / (R + G + 1.05*B)
            # And R + G + 1.05*B = p2 - (671*a + 6588*b)/1403
            # Solving gives: gamma = p2 / [p1 + (671*cos(h) + 6588*sin(h))/1403]
            denom = p1 + (671.0 * cos_h + 6588.0 * sin_h) / 1403.0

            if abs(denom) < 1e-10:
                gamma = 0.0
            else:
                gamma = p2 / denom

            a = gamma * cos_h
            b = gamma * sin_h

        # RGB_a from a, b, and p2
        RGB_a = np.array([
            (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0,
            (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0,
            (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0
        ])

        # Inverse nonlinear adaptation
        RGB_c = self._nonlinear_adaptation_inv(RGB_a)

        # Inverse chromatic adaptation
        RGB = RGB_c / self.D_RGB

        # Transform back to XYZ
        xyz = CAM16_M_INV @ RGB

        return xyz

    def to_ucs(self, J: float, M: float, h: float) -> Tuple[float, float, float]:
        """
        Convert CAM16 JMh to CAM16-UCS coordinates.

        CAM16-UCS provides uniform color difference for wide color gamut.

        Args:
            J: Lightness
            M: Colorfulness
            h: Hue angle in degrees

        Returns:
            (J', a', b') CAM16-UCS coordinates
        """
        J_prime = 1.7 * J / (1.0 + 0.007 * J)
        M_prime = np.log(1.0 + 0.0228 * M) / 0.0228

        h_rad = np.radians(h)
        a_prime = M_prime * np.cos(h_rad)
        b_prime = M_prime * np.sin(h_rad)

        return (J_prime, a_prime, b_prime)

    def delta_E_cam16(self, jmh1: Tuple[float, float, float],
                      jmh2: Tuple[float, float, float]) -> float:
        """
        Calculate CAM16-UCS color difference.

        More accurate than CIEDE2000 for HDR and wide color gamut.

        Args:
            jmh1: (J, M, h) of first color
            jmh2: (J, M, h) of second color

        Returns:
            Delta E in CAM16-UCS space
        """
        ucs1 = self.to_ucs(*jmh1)
        ucs2 = self.to_ucs(*jmh2)

        dJ = ucs2[0] - ucs1[0]
        da = ucs2[1] - ucs1[1]
        db = ucs2[2] - ucs1[2]

        return np.sqrt(dJ**2 + da**2 + db**2)


# =============================================================================
# Jzazbz Color Space
# =============================================================================

# Jzazbz constants
JZAZBZ_B = 1.15
JZAZBZ_G = 0.66
JZAZBZ_C1 = 0.8359375  # Same as PQ_C1
JZAZBZ_C2 = 18.8515625  # Same as PQ_C2
JZAZBZ_C3 = 18.6875  # Same as PQ_C3
JZAZBZ_N = 2610.0 / 16384.0  # Same as PQ_M1
JZAZBZ_P = 1.7 * 2523.0 / 32.0  # 134.034375
JZAZBZ_D = -0.56
JZAZBZ_D0 = 1.6295499532821566e-11

# Jzazbz matrices
JZAZBZ_M1 = np.array([
    [0.41478972, 0.579999, 0.0146480],
    [-0.2015100, 1.120649, 0.0531008],
    [-0.0166008, 0.264800, 0.6684799]
], dtype=np.float64)

JZAZBZ_M2 = np.array([
    [0.5, 0.5, 0],
    [3.524000, -4.066708, 0.542708],
    [0.199076, 1.096799, -1.295875]
], dtype=np.float64)

JZAZBZ_M1_INV = np.linalg.inv(JZAZBZ_M1)
JZAZBZ_M2_INV = np.linalg.inv(JZAZBZ_M2)


class Jzazbz:
    """
    Jzazbz Perceptually Uniform Color Space.

    Designed specifically for HDR and wide color gamut (WCG) applications.
    Maintains perceptual uniformity up to 10,000 nits.

    Key advantages:
    - Best iso-hue prediction (minimal hue shift with saturation changes)
    - Excellent for large color differences
    - Natural extension of CIELAB principles to HDR
    - Used in HDR10+ processing

    Reference:
        Safdar et al. (2017) "Perceptually uniform color space for image
        signals including high dynamic range and wide gamut"
    """

    def __init__(self, peak_luminance: float = 10000.0):
        """
        Initialize Jzazbz for given peak luminance.

        Args:
            peak_luminance: Display peak luminance in cd/m² (default: 10000)
        """
        self.peak_luminance = peak_luminance

    def xyz_to_jzazbz(self, xyz: np.ndarray) -> np.ndarray:
        """
        Convert XYZ to Jzazbz.

        Args:
            xyz: XYZ values as (3,) or (N, 3) array.
                 For absolute XYZ: Y is in cd/m²
                 For relative XYZ: Y=1 corresponds to peak_luminance

        Returns:
            Jzazbz values as (3,) or (N, 3) array
            [Jz, az, bz] where Jz is lightness, az/bz are chromatic
        """
        xyz = np.asarray(xyz, dtype=np.float64)
        single = xyz.ndim == 1
        if single:
            xyz = xyz.reshape(1, 3)

        results = []
        for i in range(len(xyz)):
            jzazbz = self._forward_single(xyz[i])
            results.append(jzazbz)

        results = np.array(results)
        if single:
            return results[0]
        return results

    def _forward_single(self, xyz: np.ndarray) -> np.ndarray:
        """Convert single XYZ to Jzazbz."""
        X, Y, Z = xyz

        # Modified XYZ (account for absolute luminance)
        X_prime = JZAZBZ_B * X - (JZAZBZ_B - 1) * Z
        Y_prime = JZAZBZ_G * Y - (JZAZBZ_G - 1) * X

        XYZ_prime = np.array([X_prime, Y_prime, Z])

        # Transform to LMS
        LMS = JZAZBZ_M1 @ XYZ_prime

        # Normalize by peak luminance and apply PQ-like transfer
        LMS_normalized = np.abs(LMS) / self.peak_luminance
        LMS_sign = np.sign(LMS)

        # PQ-like nonlinearity
        LMS_pq = LMS_sign * self._pq_transfer(LMS_normalized)

        # Transform to Izazbz
        Izazbz = JZAZBZ_M2 @ LMS_pq

        # Apply final Jz transform
        Iz = Izazbz[0]
        Jz = ((1.0 + JZAZBZ_D) * Iz) / (1.0 + JZAZBZ_D * Iz) - JZAZBZ_D0

        return np.array([Jz, Izazbz[1], Izazbz[2]])

    def _pq_transfer(self, x: np.ndarray) -> np.ndarray:
        """PQ-like transfer function for Jzazbz."""
        x_n = np.power(x, JZAZBZ_N)
        num = JZAZBZ_C1 + JZAZBZ_C2 * x_n
        den = 1.0 + JZAZBZ_C3 * x_n
        return np.power(num / den, JZAZBZ_P)

    def _pq_transfer_inv(self, x: np.ndarray) -> np.ndarray:
        """Inverse PQ-like transfer function."""
        x_p = np.power(x, 1.0 / JZAZBZ_P)
        # Correct inverse: num = x_p - C1, den = C2 - C3*x_p
        num = np.maximum(x_p - JZAZBZ_C1, 0)
        den = JZAZBZ_C2 - JZAZBZ_C3 * x_p
        den = np.where(np.abs(den) < 1e-10, 1e-10, den)
        return np.power(num / den, 1.0 / JZAZBZ_N)

    def jzazbz_to_xyz(self, jzazbz: np.ndarray) -> np.ndarray:
        """
        Convert Jzazbz to XYZ.

        Args:
            jzazbz: Jzazbz values as (3,) or (N, 3) array

        Returns:
            XYZ values
        """
        jzazbz = np.asarray(jzazbz, dtype=np.float64)
        single = jzazbz.ndim == 1
        if single:
            jzazbz = jzazbz.reshape(1, 3)

        results = []
        for i in range(len(jzazbz)):
            xyz = self._inverse_single(jzazbz[i])
            results.append(xyz)

        results = np.array(results)
        if single:
            return results[0]
        return results

    def _inverse_single(self, jzazbz: np.ndarray) -> np.ndarray:
        """Convert single Jzazbz to XYZ."""
        Jz, az, bz = jzazbz

        # Inverse Jz transform
        Iz = (Jz + JZAZBZ_D0) / (1.0 + JZAZBZ_D - JZAZBZ_D * (Jz + JZAZBZ_D0))

        # Reconstruct Izazbz
        Izazbz = np.array([Iz, az, bz])

        # Inverse transform to LMS_pq
        LMS_pq = JZAZBZ_M2_INV @ Izazbz

        # Inverse PQ transfer
        LMS_sign = np.sign(LMS_pq)
        LMS_normalized = self._pq_transfer_inv(np.abs(LMS_pq))
        LMS = LMS_sign * LMS_normalized * self.peak_luminance

        # Inverse LMS to XYZ_prime
        XYZ_prime = JZAZBZ_M1_INV @ LMS

        # Inverse modified XYZ
        X_prime, Y_prime, Z = XYZ_prime
        X = (X_prime + (JZAZBZ_B - 1) * Z) / JZAZBZ_B
        Y = (Y_prime + (JZAZBZ_G - 1) * X) / JZAZBZ_G

        return np.array([X, Y, Z])

    def to_jzczhz(self, jzazbz: np.ndarray) -> np.ndarray:
        """
        Convert Jzazbz to cylindrical JzCzhz.

        Args:
            jzazbz: Jzazbz values as (3,) or (N, 3) array

        Returns:
            JzCzhz values [Jz, Cz (chroma), hz (hue in degrees)]
        """
        jzazbz = np.asarray(jzazbz, dtype=np.float64)

        if jzazbz.ndim == 1:
            Jz, az, bz = jzazbz
            Cz = np.sqrt(az**2 + bz**2)
            hz = np.degrees(np.arctan2(bz, az)) % 360.0
            return np.array([Jz, Cz, hz])
        else:
            Jz = jzazbz[:, 0]
            az = jzazbz[:, 1]
            bz = jzazbz[:, 2]
            Cz = np.sqrt(az**2 + bz**2)
            hz = np.degrees(np.arctan2(bz, az)) % 360.0
            return np.column_stack([Jz, Cz, hz])

    def delta_Ez(self, jzazbz1: np.ndarray, jzazbz2: np.ndarray) -> float:
        """
        Calculate Jzazbz color difference (Delta Ez).

        Perceptually uniform color difference for HDR content.

        Args:
            jzazbz1: First color in Jzazbz
            jzazbz2: Second color in Jzazbz

        Returns:
            Delta Ez (Euclidean distance in Jzazbz space)
        """
        jzazbz1 = np.asarray(jzazbz1, dtype=np.float64)
        jzazbz2 = np.asarray(jzazbz2, dtype=np.float64)

        dJz = jzazbz2[0] - jzazbz1[0]
        daz = jzazbz2[1] - jzazbz1[1]
        dbz = jzazbz2[2] - jzazbz1[2]

        return np.sqrt(dJz**2 + daz**2 + dbz**2)


# =============================================================================
# ICtCp Color Space
# =============================================================================

# ICtCp matrices (BT.2100)
ICTCP_M1 = np.array([
    [0.3592, 0.6976, -0.0358],
    [-0.1922, 1.1004, 0.0754],
    [0.0070, 0.0749, 0.8434]
], dtype=np.float64)

ICTCP_M2 = np.array([
    [2048, 2048, 0],
    [6610, -13613, 7003],
    [17933, -17390, -543]
], dtype=np.float64) / 4096.0

ICTCP_M1_INV = np.linalg.inv(ICTCP_M1)
ICTCP_M2_INV = np.linalg.inv(ICTCP_M2)


class ICtCp:
    """
    ICtCp Color Space (Dolby / ITU-R BT.2100).

    Optimized for HDR video with PQ transfer function.
    The reference color space for Dolby Vision processing.

    Key advantages:
    - Excellent blue hue prediction
    - Better than CIELAB for modern displays
    - Native PQ encoding support
    - Standard for HDR video production

    Correlates:
    - I (Intensity): Achromatic/brightness component
    - Ct (Chroma-Tritan): Blue-yellow axis
    - Cp (Chroma-Protan): Red-green axis

    Reference:
        ITU-R BT.2100-2 (2018) "Image parameter values for HDR television"
    """

    def __init__(self, peak_luminance: float = 10000.0):
        """
        Initialize ICtCp for given peak luminance.

        Args:
            peak_luminance: Display peak luminance in cd/m² (default: 10000)
        """
        self.peak_luminance = peak_luminance

    def rgb_to_ictcp(self, rgb: np.ndarray, input_space: str = 'bt2020') -> np.ndarray:
        """
        Convert linear RGB to ICtCp.

        Args:
            rgb: Linear RGB values as (3,) or (N, 3) array
                 Normalized to [0, 1] where 1 = peak luminance
            input_space: Input color space ('bt2020', 'srgb')

        Returns:
            ICtCp values as (3,) or (N, 3) array
        """
        rgb = np.asarray(rgb, dtype=np.float64)
        single = rgb.ndim == 1
        if single:
            rgb = rgb.reshape(1, 3)

        # Convert to BT.2020 if needed
        if input_space == 'srgb':
            # sRGB linear -> XYZ -> BT.2020 linear
            xyz = (SRGB_TO_XYZ @ rgb.T).T
            rgb = (XYZ_TO_BT2020 @ xyz.T).T

        # Apply absolute luminance scaling
        rgb_abs = rgb * self.peak_luminance

        results = []
        for i in range(len(rgb)):
            ictcp = self._forward_single(rgb_abs[i])
            results.append(ictcp)

        results = np.array(results)
        if single:
            return results[0]
        return results

    def _forward_single(self, rgb: np.ndarray) -> np.ndarray:
        """Convert single linear RGB to ICtCp."""
        # Transform to LMS
        LMS = ICTCP_M1 @ rgb

        # Apply PQ transfer
        LMS_pq = pq_oetf(np.clip(LMS, 0, 10000))

        # Transform to ICtCp
        ICtCp = ICTCP_M2 @ LMS_pq

        return ICtCp

    def ictcp_to_rgb(self, ictcp: np.ndarray, output_space: str = 'bt2020') -> np.ndarray:
        """
        Convert ICtCp to linear RGB.

        Args:
            ictcp: ICtCp values as (3,) or (N, 3) array
            output_space: Output color space ('bt2020', 'srgb')

        Returns:
            Linear RGB values
        """
        ictcp = np.asarray(ictcp, dtype=np.float64)
        single = ictcp.ndim == 1
        if single:
            ictcp = ictcp.reshape(1, 3)

        results = []
        for i in range(len(ictcp)):
            rgb = self._inverse_single(ictcp[i])
            results.append(rgb)

        results = np.array(results) / self.peak_luminance

        # Convert from BT.2020 if needed
        if output_space == 'srgb':
            xyz = (BT2020_TO_XYZ @ results.T).T
            results = (XYZ_TO_SRGB @ xyz.T).T

        if single:
            return results[0]
        return results

    def _inverse_single(self, ictcp: np.ndarray) -> np.ndarray:
        """Convert single ICtCp to linear RGB."""
        # Inverse transform to LMS_pq
        LMS_pq = ICTCP_M2_INV @ ictcp

        # Inverse PQ transfer
        LMS = pq_eotf(np.clip(LMS_pq, 0, 1))

        # Inverse transform to RGB
        RGB = ICTCP_M1_INV @ LMS

        return RGB

    def delta_E_ITP(self, ictcp1: np.ndarray, ictcp2: np.ndarray) -> float:
        """
        Calculate ICtCp color difference (Delta E ITP).

        Optimized for HDR video color differences.
        Includes 720 scaling factor as per BT.2124.

        Args:
            ictcp1: First color in ICtCp
            ictcp2: Second color in ICtCp

        Returns:
            Delta E ITP (scaled to approximate CIELAB Delta E at SDR)
        """
        ictcp1 = np.asarray(ictcp1, dtype=np.float64)
        ictcp2 = np.asarray(ictcp2, dtype=np.float64)

        # BT.2124 scaling: 720 for T and P, 1 for I
        dI = ictcp2[0] - ictcp1[0]
        dT = 0.5 * (ictcp2[1] - ictcp1[1])  # Ct scaled by 0.5
        dP = ictcp2[2] - ictcp1[2]

        return 720.0 * np.sqrt(dI**2 + dT**2 + dP**2)


# =============================================================================
# Convenience Functions
# =============================================================================

def xyz_to_cam16_jmh(xyz: np.ndarray,
                     viewing_conditions: CAM16ViewingConditions = None) -> Tuple[float, float, float]:
    """
    Quick conversion from XYZ to CAM16 JMh (Lightness, Colorfulness, Hue).

    Args:
        xyz: XYZ values
        viewing_conditions: Optional CAM16 viewing conditions

    Returns:
        (J, M, h) tuple
    """
    cam16 = CAM16(viewing_conditions)
    result = cam16.xyz_to_cam16(xyz)
    return (result['J'], result['M'], result['h'])


def xyz_to_jzazbz(xyz: np.ndarray, peak_luminance: float = 10000.0) -> np.ndarray:
    """
    Quick conversion from XYZ to Jzazbz.

    Args:
        xyz: XYZ values
        peak_luminance: Display peak luminance in cd/m²

    Returns:
        Jzazbz values
    """
    jz = Jzazbz(peak_luminance)
    return jz.xyz_to_jzazbz(xyz)


def rgb_to_ictcp(rgb: np.ndarray, input_space: str = 'bt2020',
                 peak_luminance: float = 10000.0) -> np.ndarray:
    """
    Quick conversion from linear RGB to ICtCp.

    Args:
        rgb: Linear RGB values [0, 1]
        input_space: 'bt2020' or 'srgb'
        peak_luminance: Display peak luminance in cd/m²

    Returns:
        ICtCp values
    """
    ictcp = ICtCp(peak_luminance)
    return ictcp.rgb_to_ictcp(rgb, input_space)


def delta_e_hdr(color1: np.ndarray, color2: np.ndarray,
                color_space: str = 'xyz',
                method: str = 'jzazbz',
                peak_luminance: float = 10000.0) -> float:
    """
    Calculate perceptually uniform color difference for HDR content.

    Provides a unified interface for HDR color difference calculations.

    Args:
        color1: First color
        color2: Second color
        color_space: Input color space ('xyz', 'jzazbz', 'ictcp', 'cam16_jmh')
        method: Difference method ('jzazbz', 'ictcp', 'cam16')
        peak_luminance: Display peak luminance in cd/m²

    Returns:
        Perceptually uniform color difference
    """
    if color_space == 'xyz':
        if method == 'jzazbz':
            jz = Jzazbz(peak_luminance)
            c1 = jz.xyz_to_jzazbz(color1)
            c2 = jz.xyz_to_jzazbz(color2)
            return jz.delta_Ez(c1, c2)
        elif method == 'cam16':
            cam = CAM16()
            r1 = cam.xyz_to_cam16(color1)
            r2 = cam.xyz_to_cam16(color2)
            return cam.delta_E_cam16((r1['J'], r1['M'], r1['h']),
                                      (r2['J'], r2['M'], r2['h']))
        elif method == 'ictcp':
            # Need to go through BT.2020 RGB
            rgb1 = (XYZ_TO_BT2020 @ color1).clip(0, 1)
            rgb2 = (XYZ_TO_BT2020 @ color2).clip(0, 1)
            ictcp = ICtCp(peak_luminance)
            c1 = ictcp.rgb_to_ictcp(rgb1)
            c2 = ictcp.rgb_to_ictcp(rgb2)
            return ictcp.delta_E_ITP(c1, c2)
    elif color_space == 'jzazbz':
        jz = Jzazbz(peak_luminance)
        return jz.delta_Ez(color1, color2)
    elif color_space == 'ictcp':
        ictcp = ICtCp(peak_luminance)
        return ictcp.delta_E_ITP(color1, color2)
    elif color_space == 'cam16_jmh':
        cam = CAM16()
        return cam.delta_E_cam16(color1, color2)

    raise ValueError(f"Unknown color_space '{color_space}' or method '{method}'")
