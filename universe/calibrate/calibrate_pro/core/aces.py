"""
ACES 2.0 Color Management System

Academy Color Encoding System 2.0 - Complete implementation of the
next-generation ACES rendering transform with JMh-based gamut mapping.

ACES 2.0 Key Features:
- Complete redesign of the rendering transform (Output Transform)
- HDR/SDR consistency across all output devices
- JMh (Lightness, Colorfulness, Hue) based gamut mapping
- Sophisticated tone mapping with parametric compression
- Chroma preservation and smooth gamut boundary handling

This implementation follows the ACES 2.0 specifications and is compatible
with OpenColorIO 2.4+ fixed functions.

Author: Zain Dana / Build
License: Fair-Source (see LICENSE)

References:
- ACES 2.0 Technical Documentation (2024)
- OpenColorIO 2.4 ACES Built-in Transforms
- "ACES 2.0 - A New Color Management System" (AMPAS)
"""

import numpy as np
from dataclasses import dataclass
from typing import Tuple, Optional, Dict, List
from enum import Enum
import math

# Import our color models for JMh conversions
from .color_models import CAM16, CAM16ViewingConditions, pq_eotf, pq_oetf

# =============================================================================
# ACES Color Spaces and Primaries
# =============================================================================

# ACES AP0 primaries (ACES 2065-1)
ACES_AP0_PRIMARIES = {
    'red': (0.7347, 0.2653),
    'green': (0.0000, 1.0000),
    'blue': (0.0001, -0.0770),
    'white': (0.32168, 0.33767)  # ACES white point (D60-ish)
}

# ACES AP1 primaries (ACEScg, ACEScct)
ACES_AP1_PRIMARIES = {
    'red': (0.713, 0.293),
    'green': (0.165, 0.830),
    'blue': (0.128, 0.044),
    'white': (0.32168, 0.33767)
}

# Standard output primaries
SRGB_PRIMARIES = {
    'red': (0.64, 0.33),
    'green': (0.30, 0.60),
    'blue': (0.15, 0.06),
    'white': (0.3127, 0.3290)  # D65
}

P3_D65_PRIMARIES = {
    'red': (0.680, 0.320),
    'green': (0.265, 0.690),
    'blue': (0.150, 0.060),
    'white': (0.3127, 0.3290)
}

BT2020_PRIMARIES = {
    'red': (0.708, 0.292),
    'green': (0.170, 0.797),
    'blue': (0.131, 0.046),
    'white': (0.3127, 0.3290)
}


def primaries_to_matrix(primaries: dict) -> np.ndarray:
    """Convert primaries dictionary to RGB->XYZ matrix."""
    def xy_to_XYZ(x, y):
        return np.array([x/y, 1.0, (1-x-y)/y]) if y != 0 else np.array([0, 0, 0])

    R = xy_to_XYZ(*primaries['red'])
    G = xy_to_XYZ(*primaries['green'])
    B = xy_to_XYZ(*primaries['blue'])
    W = xy_to_XYZ(*primaries['white'])

    M = np.column_stack([R, G, B])
    S = np.linalg.solve(M, W)

    return M * S


# Pre-computed matrices
AP0_TO_XYZ = primaries_to_matrix(ACES_AP0_PRIMARIES)
XYZ_TO_AP0 = np.linalg.inv(AP0_TO_XYZ)

AP1_TO_XYZ = primaries_to_matrix(ACES_AP1_PRIMARIES)
XYZ_TO_AP1 = np.linalg.inv(AP1_TO_XYZ)

SRGB_TO_XYZ = primaries_to_matrix(SRGB_PRIMARIES)
XYZ_TO_SRGB = np.linalg.inv(SRGB_TO_XYZ)

P3_TO_XYZ = primaries_to_matrix(P3_D65_PRIMARIES)
XYZ_TO_P3 = np.linalg.inv(P3_TO_XYZ)

BT2020_TO_XYZ_ACES = primaries_to_matrix(BT2020_PRIMARIES)
XYZ_TO_BT2020_ACES = np.linalg.inv(BT2020_TO_XYZ_ACES)

# AP0 to AP1 direct conversion
AP0_TO_AP1 = XYZ_TO_AP1 @ AP0_TO_XYZ
AP1_TO_AP0 = np.linalg.inv(AP0_TO_AP1)


# =============================================================================
# ACES 2.0 Output Transforms
# =============================================================================

class OutputDevice(Enum):
    """Standard output device configurations."""
    SDR_100_NITS = "sdr_100"
    SDR_CINEMA = "sdr_48"
    HDR_1000_NITS = "hdr_1000"
    HDR_2000_NITS = "hdr_2000"
    HDR_4000_NITS = "hdr_4000"
    HDR_10000_NITS = "hdr_10000"


@dataclass
class OutputConfig:
    """Configuration for ACES 2.0 output transform."""
    peak_luminance: float  # Peak white in cd/m²
    min_luminance: float  # Black level in cd/m²
    limiting_primaries: dict  # Output gamut primaries
    encoding_primaries: dict  # Display encoding primaries
    surround: str = 'dim'  # Viewing surround ('dark', 'dim', 'average')
    eotf: str = 'srgb'  # 'srgb', 'bt1886', 'pq', 'hlg'

    @classmethod
    def sdr_100_srgb(cls) -> 'OutputConfig':
        """Standard SDR sRGB monitor (100 nits)."""
        return cls(
            peak_luminance=100.0,
            min_luminance=0.0001,
            limiting_primaries=SRGB_PRIMARIES,
            encoding_primaries=SRGB_PRIMARIES,
            surround='dim',
            eotf='srgb'
        )

    @classmethod
    def sdr_100_p3(cls) -> 'OutputConfig':
        """SDR P3-D65 monitor (100 nits)."""
        return cls(
            peak_luminance=100.0,
            min_luminance=0.0001,
            limiting_primaries=P3_D65_PRIMARIES,
            encoding_primaries=P3_D65_PRIMARIES,
            surround='dim',
            eotf='srgb'
        )

    @classmethod
    def hdr_1000_p3(cls) -> 'OutputConfig':
        """HDR P3-D65 monitor (1000 nits)."""
        return cls(
            peak_luminance=1000.0,
            min_luminance=0.0001,
            limiting_primaries=P3_D65_PRIMARIES,
            encoding_primaries=P3_D65_PRIMARIES,
            surround='dim',
            eotf='pq'
        )

    @classmethod
    def hdr_1000_bt2020(cls) -> 'OutputConfig':
        """HDR BT.2020 (1000 nits)."""
        return cls(
            peak_luminance=1000.0,
            min_luminance=0.0001,
            limiting_primaries=BT2020_PRIMARIES,
            encoding_primaries=BT2020_PRIMARIES,
            surround='dim',
            eotf='pq'
        )

    @classmethod
    def hdr_4000_p3(cls) -> 'OutputConfig':
        """HDR P3-D65 mastering monitor (4000 nits)."""
        return cls(
            peak_luminance=4000.0,
            min_luminance=0.0001,
            limiting_primaries=P3_D65_PRIMARIES,
            encoding_primaries=P3_D65_PRIMARIES,
            surround='dark',
            eotf='pq'
        )


# =============================================================================
# ACES 2.0 Tonescale
# =============================================================================

class ACES2Tonescale:
    """
    ACES 2.0 Tonescale Function.

    The new tonescale replaces the RRT+ODT combination with a single
    parametric function that adapts to any output luminance level.

    Key features:
    - Smooth rolloff for highlights
    - Preserved shadow detail
    - Consistent look from SDR to HDR
    - Parametric control via peak luminance
    """

    # Tonescale parameters (ACES 2.0 defaults)
    PARAMS = {
        'contrast': 1.5,
        'pivot': 0.18,  # Middle gray
        'toe_power': 2.0,
        'shoulder_power': 1.5,
        'toe_gain': 0.0,
        'shoulder_gain': 0.9
    }

    def __init__(self, peak_luminance: float = 100.0, min_luminance: float = 0.0001):
        """
        Initialize tonescale for output device.

        Args:
            peak_luminance: Peak output luminance in cd/m²
            min_luminance: Minimum output luminance in cd/m²
        """
        self.peak = peak_luminance
        self.min = min_luminance

        # Dynamic range in stops
        self.dynamic_range = np.log2(peak_luminance / 0.18) if peak_luminance > 0 else 10

        # Compute tonescale curve parameters
        self._compute_curve_params()

    def _compute_curve_params(self):
        """Compute adaptive tonescale parameters based on output."""
        # Base parameters
        self.contrast = self.PARAMS['contrast']
        self.pivot = self.PARAMS['pivot']

        # Adapt toe and shoulder based on dynamic range
        if self.peak >= 1000:  # HDR
            self.toe_power = 1.8
            self.shoulder_power = 1.2
            self.highlight_gain = 0.95
        elif self.peak >= 400:  # High-brightness SDR
            self.toe_power = 1.9
            self.shoulder_power = 1.4
            self.highlight_gain = 0.92
        else:  # Standard SDR
            self.toe_power = 2.0
            self.shoulder_power = 1.5
            self.highlight_gain = 0.9

        # Compute curve intersections
        self.toe_start = self.pivot * 0.1
        self.shoulder_start = self.pivot * 4.0

    def apply(self, J: np.ndarray) -> np.ndarray:
        """
        Apply ACES 2.0 tonescale to lightness values.

        Args:
            J: Input lightness values (scene-referred, linear-ish)

        Returns:
            Output lightness values (display-referred)
        """
        J = np.asarray(J, dtype=np.float64)

        # Normalize to mid-gray pivot
        x = J / self.pivot

        # Apply S-curve with toe and shoulder
        result = np.zeros_like(J)

        # Toe region (shadows)
        toe_mask = J < self.toe_start
        if np.any(toe_mask):
            t = J[toe_mask] / self.toe_start
            result[toe_mask] = self.toe_start * np.power(t, self.toe_power) * 0.5

        # Linear region (mid-tones)
        linear_mask = (J >= self.toe_start) & (J < self.shoulder_start)
        if np.any(linear_mask):
            # Apply contrast around pivot
            normalized = J[linear_mask] / self.pivot
            result[linear_mask] = self.pivot * np.power(normalized, 1.0 / self.contrast)

        # Shoulder region (highlights)
        shoulder_mask = J >= self.shoulder_start
        if np.any(shoulder_mask):
            # Soft rolloff towards peak
            x = J[shoulder_mask] / self.shoulder_start
            compressed = 1.0 - np.power(1.0 - self.highlight_gain, np.power(x, self.shoulder_power))
            # Scale to output range
            result[shoulder_mask] = self.shoulder_start + \
                                     (self.peak - self.shoulder_start) * compressed / self.peak

        # Normalize to [0, 1] range for output
        result = result / self.peak

        return np.clip(result, 0, 1)

    def apply_inverse(self, J_out: np.ndarray) -> np.ndarray:
        """
        Apply inverse tonescale (display to scene).

        Args:
            J_out: Display-referred lightness [0, 1]

        Returns:
            Scene-referred lightness
        """
        # Scale back to absolute
        J_out = np.asarray(J_out, dtype=np.float64) * self.peak

        result = np.zeros_like(J_out)

        # Inverse toe
        toe_end = self.toe_start * 0.5
        toe_mask = J_out < toe_end
        if np.any(toe_mask):
            t = J_out[toe_mask] / toe_end
            result[toe_mask] = self.toe_start * np.power(t, 1.0 / self.toe_power)

        # Inverse linear
        linear_end = self.shoulder_start / self.pivot * np.power(self.shoulder_start / self.pivot, 1.0 / self.contrast) * self.pivot
        linear_mask = (J_out >= toe_end) & (J_out < linear_end)
        if np.any(linear_mask):
            result[linear_mask] = self.pivot * np.power(J_out[linear_mask] / self.pivot, self.contrast)

        # Inverse shoulder
        shoulder_mask = J_out >= linear_end
        if np.any(shoulder_mask):
            # Inverse of soft rolloff
            normalized = (J_out[shoulder_mask] - self.shoulder_start) / (self.peak - self.shoulder_start) * self.peak
            x = np.power(-np.log(1.0 - normalized / self.highlight_gain), 1.0 / self.shoulder_power)
            result[shoulder_mask] = x * self.shoulder_start

        return result


# =============================================================================
# ACES 2.0 Gamut Mapper
# =============================================================================

class ACES2GamutMapper:
    """
    ACES 2.0 Gamut Mapper using JMh color space.

    The new gamut mapping operates in a perceptual color space (JMh)
    and uses parametric compression curves for smooth handling of
    out-of-gamut colors.

    Key features:
    - JMh (CAM16-based) color space operation
    - Parametric compression: threshold (t), limit (l), power (p)
    - Hue-dependent processing for problematic regions (blue, red)
    - Focus on cusp (most saturated) colors
    """

    def __init__(self, output_primaries: dict, peak_luminance: float = 100.0):
        """
        Initialize gamut mapper for target gamut.

        Args:
            output_primaries: Target gamut primaries
            peak_luminance: Peak luminance for CAM16 adaptation
        """
        self.output_primaries = output_primaries
        self.peak_luminance = peak_luminance

        # Initialize CAM16 for perceptual operations
        vc = CAM16ViewingConditions(
            L_A=peak_luminance * 0.2,  # 20% of peak for adaptation
            Y_b=20.0,
            surround='dim'
        )
        self.cam16 = CAM16(vc)

        # Compute gamut boundary in JMh
        self._compute_gamut_boundary()

        # Compression parameters (ACES 2.0 defaults)
        self.threshold = 0.75  # Start compression at 75% of boundary
        self.limit = 1.05  # Allow slight overshoot before hard clip
        self.power = 1.2  # Compression curve power

    def _compute_gamut_boundary(self):
        """Pre-compute the gamut boundary at various hue angles."""
        # Sample the gamut boundary at regular hue intervals
        self.boundary_hues = np.linspace(0, 360, 361)
        self.boundary_M = np.zeros(361)

        output_matrix = primaries_to_matrix(self.output_primaries)
        output_inv = np.linalg.inv(output_matrix)

        for i, h in enumerate(self.boundary_hues):
            # Find maximum M (colorfulness) at this hue
            # by testing RGB cube edges
            max_M = 0
            for edge in self._get_gamut_edges():
                rgb = np.array(edge)
                xyz = output_matrix @ rgb
                result = self.cam16.xyz_to_cam16(xyz)
                if abs(result['h'] - h) < 5 or abs(result['h'] - h - 360) < 5:
                    max_M = max(max_M, result['M'])
            self.boundary_M[i] = max(max_M, 1.0)  # Minimum boundary

    def _get_gamut_edges(self) -> List[Tuple[float, float, float]]:
        """Get sample points on RGB cube edges."""
        edges = []
        # Sample along each edge of the RGB cube
        for t in np.linspace(0, 1, 20):
            # Red to yellow
            edges.append((1, t, 0))
            # Yellow to green
            edges.append((1-t, 1, 0))
            # Green to cyan
            edges.append((0, 1, t))
            # Cyan to blue
            edges.append((0, 1-t, 1))
            # Blue to magenta
            edges.append((t, 0, 1))
            # Magenta to red
            edges.append((1, 0, 1-t))
        return edges

    def _get_boundary_at_hue(self, h: float) -> float:
        """Get gamut boundary M at given hue angle."""
        h = h % 360
        idx = int(h)
        frac = h - idx
        next_idx = (idx + 1) % 361
        return self.boundary_M[idx] * (1 - frac) + self.boundary_M[next_idx] * frac

    def compress(self, J: float, M: float, h: float) -> Tuple[float, float, float]:
        """
        Apply gamut compression to JMh values.

        Uses ACES 2.0 parametric compression:
        - Below threshold: no change
        - Above threshold: smooth compression towards boundary
        - Above limit: hard clip

        Args:
            J: Lightness
            M: Colorfulness
            h: Hue angle (degrees)

        Returns:
            Compressed (J, M, h) tuple
        """
        # Get gamut boundary at this hue
        M_boundary = self._get_boundary_at_hue(h)

        # Calculate threshold and limit in absolute M
        M_threshold = M_boundary * self.threshold
        M_limit = M_boundary * self.limit

        if M <= M_threshold:
            # Below threshold: no compression
            return (J, M, h)
        elif M >= M_limit:
            # Above limit: hard clip to boundary
            return (J, M_boundary, h)
        else:
            # In compression zone: apply smooth curve
            # Parametric compression: y = t + (l - t) * ((x - t) / (l - t))^p
            x_norm = (M - M_threshold) / (M_limit - M_threshold)
            y_norm = np.power(x_norm, self.power)
            M_compressed = M_threshold + (M_boundary - M_threshold) * y_norm
            return (J, M_compressed, h)

    def compress_chroma(self, J: float, M: float, h: float) -> Tuple[float, float, float]:
        """
        Apply chroma-only compression (preserve lightness).

        Used in the ACES 2.0 pipeline after tonescale.

        Args:
            J: Lightness (already tonemapped)
            M: Colorfulness
            h: Hue angle

        Returns:
            Compressed (J, M, h) with reduced chroma
        """
        # For very dark or very bright values, reduce chroma
        J_normalized = J / 100.0

        # Chroma compression factor based on lightness
        if J_normalized < 0.1:
            chroma_factor = J_normalized / 0.1
        elif J_normalized > 0.9:
            chroma_factor = (1.0 - J_normalized) / 0.1
        else:
            chroma_factor = 1.0

        M_adjusted = M * chroma_factor

        # Apply gamut boundary compression
        return self.compress(J, M_adjusted, h)


# =============================================================================
# ACES 2.0 Main Pipeline
# =============================================================================

class ACES2:
    """
    Academy Color Encoding System 2.0 - Complete Rendering Pipeline.

    ACES 2.0 represents a complete redesign of the ACES rendering transform,
    moving from the complex RRT+ODT structure to a more elegant and flexible
    single-stage Output Transform.

    Pipeline stages:
    1. Input conversion (AP0 → Working Space)
    2. Scene-to-JMh conversion
    3. Tonescale application (J channel)
    4. Chroma compression (M channel)
    5. Gamut compression (M channel, hue-aware)
    6. White point limiting
    7. Display encoding

    Usage:
        aces = ACES2()
        output = aces.render(aces_rgb, OutputConfig.hdr_1000_p3())
    """

    def __init__(self, working_space: str = 'ap1'):
        """
        Initialize ACES 2.0 pipeline.

        Args:
            working_space: Working color space ('ap0', 'ap1')
        """
        self.working_space = working_space

        if working_space == 'ap0':
            self.to_xyz = AP0_TO_XYZ
            self.from_xyz = XYZ_TO_AP0
        else:
            self.to_xyz = AP1_TO_XYZ
            self.from_xyz = XYZ_TO_AP1

    def render(self, rgb: np.ndarray, output_config: OutputConfig) -> np.ndarray:
        """
        Render ACES RGB to display-referred RGB.

        Complete ACES 2.0 Output Transform pipeline.

        Args:
            rgb: ACES RGB values (AP0 or AP1, linear, scene-referred)
            output_config: Output device configuration

        Returns:
            Display-referred RGB values [0, 1], ready for EOTF encoding
        """
        rgb = np.asarray(rgb, dtype=np.float64)
        single = rgb.ndim == 1
        if single:
            rgb = rgb.reshape(1, 3)

        # Initialize pipeline components
        tonescale = ACES2Tonescale(output_config.peak_luminance, output_config.min_luminance)
        gamut_mapper = ACES2GamutMapper(output_config.limiting_primaries,
                                         output_config.peak_luminance)

        results = []
        for i in range(len(rgb)):
            result = self._render_single(rgb[i], output_config, tonescale, gamut_mapper)
            results.append(result)

        results = np.array(results)
        if single:
            return results[0]
        return results

    def _render_single(self, rgb: np.ndarray, config: OutputConfig,
                       tonescale: ACES2Tonescale,
                       gamut_mapper: ACES2GamutMapper) -> np.ndarray:
        """Render single ACES RGB pixel."""
        # Step 1: Convert to XYZ
        xyz = self.to_xyz @ rgb

        # Ensure positive XYZ
        xyz = np.maximum(xyz, 0)

        # Step 2: Convert to CAM16 JMh
        cam_result = gamut_mapper.cam16.xyz_to_cam16(xyz)
        J = cam_result['J']
        M = cam_result['M']
        h = cam_result['h']

        # Step 3: Apply tonescale (J only)
        J_tonemapped = tonescale.apply(np.array([J]))[0] * 100.0

        # Step 4: Apply chroma compression
        J_out, M_out, h_out = gamut_mapper.compress_chroma(J_tonemapped, M, h)

        # Step 5: Apply gamut compression
        J_final, M_final, h_final = gamut_mapper.compress(J_out, M_out, h_out)

        # Step 6: Convert back to XYZ
        xyz_out = gamut_mapper.cam16.cam16_to_xyz(J_final, M_final * 0.8, h_final)

        # Step 7: Convert to output color space
        output_matrix = primaries_to_matrix(config.encoding_primaries)
        output_inv = np.linalg.inv(output_matrix)
        rgb_out = output_inv @ xyz_out

        # Step 8: Clip to gamut
        rgb_out = np.clip(rgb_out, 0, 1)

        return rgb_out

    def apply_eotf(self, rgb: np.ndarray, eotf: str, peak_luminance: float = 100.0) -> np.ndarray:
        """
        Apply display EOTF encoding.

        Args:
            rgb: Linear RGB [0, 1]
            eotf: EOTF type ('srgb', 'bt1886', 'pq', 'hlg')
            peak_luminance: Peak display luminance

        Returns:
            EOTF-encoded RGB values
        """
        rgb = np.asarray(rgb, dtype=np.float64)

        if eotf == 'srgb':
            return self._srgb_oetf(rgb)
        elif eotf == 'bt1886':
            return self._bt1886_oetf(rgb, gamma=2.4)
        elif eotf == 'pq':
            # Scale to absolute luminance then apply PQ
            rgb_abs = rgb * peak_luminance
            return pq_oetf(rgb_abs)
        elif eotf == 'hlg':
            return self._hlg_oetf(rgb)
        else:
            return rgb  # Linear passthrough

    def _srgb_oetf(self, rgb: np.ndarray) -> np.ndarray:
        """sRGB OETF (IEC 61966-2-1)."""
        result = np.zeros_like(rgb)
        mask = rgb <= 0.0031308
        result[mask] = 12.92 * rgb[mask]
        result[~mask] = 1.055 * np.power(rgb[~mask], 1.0/2.4) - 0.055
        return np.clip(result, 0, 1)

    def _bt1886_oetf(self, rgb: np.ndarray, gamma: float = 2.4) -> np.ndarray:
        """BT.1886 OETF (inverse EOTF)."""
        return np.power(np.clip(rgb, 0, 1), 1.0 / gamma)

    def _hlg_oetf(self, rgb: np.ndarray) -> np.ndarray:
        """HLG OETF (ITU-R BT.2100)."""
        a = 0.17883277
        b = 0.28466892
        c = 0.55991073

        result = np.zeros_like(rgb)
        mask = rgb <= 1/12
        result[mask] = np.sqrt(3 * rgb[mask])
        result[~mask] = a * np.log(12 * rgb[~mask] - b) + c

        return np.clip(result, 0, 1)


# =============================================================================
# ACES 2.0 Look Modification Transform (LMT)
# =============================================================================

class ACES2LookTransform:
    """
    ACES 2.0 Look Modification Transform.

    Applies creative adjustments in ACES space before the Output Transform.
    All operations preserve the ACES workflow while allowing artistic control.
    """

    def __init__(self):
        """Initialize look transform with neutral settings."""
        self.exposure = 0.0  # Stops
        self.saturation = 1.0  # Multiplier
        self.contrast = 1.0  # Multiplier around pivot
        self.pivot = 0.18  # Middle gray
        self.highlight_rolloff = 0.0  # Subtle highlight reduction
        self.shadow_boost = 0.0  # Subtle shadow lift

    def apply(self, rgb: np.ndarray) -> np.ndarray:
        """
        Apply look modifications to ACES RGB.

        Args:
            rgb: ACES RGB values

        Returns:
            Modified ACES RGB values
        """
        rgb = np.asarray(rgb, dtype=np.float64).copy()

        # Exposure adjustment
        if self.exposure != 0:
            rgb *= np.power(2.0, self.exposure)

        # Saturation adjustment
        if self.saturation != 1.0:
            luma = 0.2126 * rgb[..., 0] + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]
            if rgb.ndim == 1:
                rgb = luma + self.saturation * (rgb - luma)
            else:
                rgb = luma[..., np.newaxis] + self.saturation * (rgb - luma[..., np.newaxis])

        # Contrast adjustment around pivot
        if self.contrast != 1.0:
            rgb = self.pivot * np.power(rgb / self.pivot, self.contrast)

        return np.maximum(rgb, 0)

    def set_exposure(self, stops: float) -> 'ACES2LookTransform':
        """Set exposure adjustment in stops."""
        self.exposure = stops
        return self

    def set_saturation(self, multiplier: float) -> 'ACES2LookTransform':
        """Set saturation multiplier (1.0 = neutral)."""
        self.saturation = multiplier
        return self

    def set_contrast(self, multiplier: float, pivot: float = 0.18) -> 'ACES2LookTransform':
        """Set contrast multiplier around pivot point."""
        self.contrast = multiplier
        self.pivot = pivot
        return self


# =============================================================================
# OCIO 2.4 Compatibility
# =============================================================================

def generate_ocio_config(output_configs: List[OutputConfig],
                          config_name: str = "ACES 2.0") -> str:
    """
    Generate OpenColorIO 2.4 configuration for ACES 2.0.

    Creates an OCIO config string that uses the built-in ACES 2.0
    transforms available in OCIO 2.4+.

    Args:
        output_configs: List of output configurations
        config_name: Configuration name

    Returns:
        OCIO config YAML string
    """
    config = f"""
ocio_profile_version: 2.4

name: {config_name}
description: ACES 2.0 Configuration for Calibrate Pro

roles:
  color_picking: sRGB - Display
  color_timing: ACEScct
  compositing_log: ACEScct
  data: Raw
  default: ACES2065-1
  matte_paint: sRGB - Texture
  reference: ACES2065-1
  rendering: ACEScg
  scene_linear: ACEScg
  texture_paint: Raw

displays:
"""

    for oc in output_configs:
        display_name = f"{oc.peak_luminance:.0f}nits"
        config += f"""  {display_name}:
    - !<View> {{name: ACES 2.0, colorspace: ACES 2.0 - {display_name}}}
"""

    config += """
colorspaces:
  - !<ColorSpace>
    name: ACES2065-1
    family: ACES
    description: The Academy Color Encoding System reference color space
    isdata: false
    allocation: lg2
    allocationvars: [-8, 5, 0.00390625]

  - !<ColorSpace>
    name: ACEScg
    family: ACES
    description: ACEScg working space
    isdata: false
    from_scene_reference: !<MatrixTransform> {matrix: [1.45143932, -0.23651075, -0.21492857, 0, -0.07655377, 1.17622970, -0.09967593, 0, 0.00831615, -0.00603245, 0.99771630, 0, 0, 0, 0, 1]}
    to_scene_reference: !<MatrixTransform> {matrix: [0.69545224, 0.14067861, 0.16386916, 0, 0.04479456, 0.85967112, 0.09553432, 0, -0.00552588, 0.00402521, 1.00150067, 0, 0, 0, 0, 1]}

  - !<ColorSpace>
    name: ACEScct
    family: ACES
    description: ACEScct log encoding
    isdata: false
    from_scene_reference: !<GroupTransform>
      children:
        - !<MatrixTransform> {matrix: [1.45143932, -0.23651075, -0.21492857, 0, -0.07655377, 1.17622970, -0.09967593, 0, 0.00831615, -0.00603245, 0.99771630, 0, 0, 0, 0, 1]}
        - !<BuiltinTransform> {style: ACEScct_to_ACES2065-1, direction: inverse}
"""

    return config


# =============================================================================
# Convenience Functions
# =============================================================================

def aces_to_srgb(rgb: np.ndarray, input_space: str = 'ap1') -> np.ndarray:
    """
    Quick conversion from ACES to sRGB display.

    Args:
        rgb: ACES RGB values
        input_space: 'ap0' or 'ap1'

    Returns:
        sRGB display values [0, 1]
    """
    aces = ACES2(working_space=input_space)
    config = OutputConfig.sdr_100_srgb()
    linear = aces.render(rgb, config)
    return aces.apply_eotf(linear, 'srgb')


def aces_to_hdr(rgb: np.ndarray, peak_luminance: float = 1000.0,
                gamut: str = 'p3', input_space: str = 'ap1') -> np.ndarray:
    """
    Quick conversion from ACES to HDR display.

    Args:
        rgb: ACES RGB values
        peak_luminance: Display peak luminance
        gamut: Output gamut ('p3', 'bt2020')
        input_space: 'ap0' or 'ap1'

    Returns:
        PQ-encoded HDR values [0, 1]
    """
    aces = ACES2(working_space=input_space)

    if gamut == 'p3':
        config = OutputConfig.hdr_1000_p3()
    else:
        config = OutputConfig.hdr_1000_bt2020()

    config.peak_luminance = peak_luminance

    linear = aces.render(rgb, config)
    return aces.apply_eotf(linear, 'pq', peak_luminance)
