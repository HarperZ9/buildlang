"""
3D LUT Optimization Module

Provides advanced LUT optimization techniques:
- Perceptual smoothing
- Gamut boundary optimization
- Minimum Delta E LUT generation
- LUT compression and quality metrics
- Interpolation quality analysis
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Optional, Callable, Tuple
import numpy as np

# Optional scipy import
try:
    from scipy.ndimage import gaussian_filter, uniform_filter
    from scipy.interpolate import RegularGridInterpolator
    from scipy.optimize import minimize
    SCIPY_AVAILABLE = True
except ImportError:
    SCIPY_AVAILABLE = False
    gaussian_filter = None
    uniform_filter = None
    RegularGridInterpolator = None
    minimize = None

# =============================================================================
# Enums
# =============================================================================

class SmoothingMethod(Enum):
    """LUT smoothing methods."""
    GAUSSIAN = auto()       # Gaussian blur
    BILATERAL = auto()      # Edge-preserving bilateral
    ANISOTROPIC = auto()    # Anisotropic diffusion
    PERCEPTUAL = auto()     # Perceptual smoothing in Lab


class GamutMappingMethod(Enum):
    """Gamut mapping methods for out-of-gamut colors."""
    CLIP = auto()           # Simple RGB clipping
    COMPRESS = auto()       # Chroma compression
    PERCEPTUAL = auto()     # Perceptual intent mapping
    ABSOLUTE = auto()       # Absolute colorimetric
    SATURATION = auto()     # Saturation preserving


class OptimizationGoal(Enum):
    """LUT optimization goal."""
    MIN_DELTA_E = auto()    # Minimize average Delta E
    MIN_MAX_ERROR = auto()  # Minimize maximum error
    SMOOTH = auto()         # Prioritize smoothness
    BALANCED = auto()       # Balance accuracy and smoothness


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class LUTQualityMetrics:
    """Quality metrics for a 3D LUT."""
    # Size
    lut_size: int

    # Error metrics (vs target)
    delta_e_mean: float = 0.0
    delta_e_max: float = 0.0
    delta_e_std: float = 0.0
    delta_e_95th: float = 0.0

    # Smoothness metrics
    gradient_mean: float = 0.0
    gradient_max: float = 0.0
    discontinuity_count: int = 0

    # Interpolation quality
    interpolation_error_mean: float = 0.0
    interpolation_error_max: float = 0.0

    # Gamut metrics
    out_of_gamut_percent: float = 0.0
    clipped_values_percent: float = 0.0

    # Compression metrics
    bits_per_value: int = 16
    total_size_bytes: int = 0


@dataclass
class OptimizationResult:
    """Result of LUT optimization."""
    optimized_lut: np.ndarray
    original_metrics: LUTQualityMetrics
    optimized_metrics: LUTQualityMetrics

    # Optimization details
    method: str
    iterations: int = 0
    improvement_percent: float = 0.0
    processing_time: float = 0.0


@dataclass
class SmoothingConfig:
    """Configuration for LUT smoothing."""
    method: SmoothingMethod = SmoothingMethod.PERCEPTUAL
    sigma: float = 0.5              # Smoothing strength
    preserve_edges: bool = True     # Edge preservation
    edge_threshold: float = 0.1     # Edge detection threshold
    iterations: int = 1


@dataclass
class GamutConfig:
    """Configuration for gamut mapping."""
    method: GamutMappingMethod = GamutMappingMethod.PERCEPTUAL
    source_gamut: str = "wide"      # Source gamut (wide, p3, bt2020)
    target_gamut: str = "srgb"      # Target gamut
    compression_factor: float = 0.8 # Chroma compression factor
    black_point_compensation: bool = True


# =============================================================================
# Color Conversion Functions
# =============================================================================

def rgb_to_xyz(rgb: np.ndarray, gamma: float = 2.2) -> np.ndarray:
    """Convert RGB to XYZ (sRGB primaries)."""
    # Linearize
    rgb_linear = np.power(np.clip(rgb, 0, 1), gamma)

    # sRGB to XYZ matrix
    matrix = np.array([
        [0.4124564, 0.3575761, 0.1804375],
        [0.2126729, 0.7151522, 0.0721750],
        [0.0193339, 0.1191920, 0.9503041]
    ])

    return np.dot(rgb_linear, matrix.T)


def xyz_to_lab(xyz: np.ndarray,
               white: Tuple[float, float, float] = (0.95047, 1.0, 1.08883)) -> np.ndarray:
    """Convert XYZ to Lab."""
    xyz_normalized = xyz / np.array(white)

    def f(t):
        delta = 6 / 29
        return np.where(t > delta**3, np.cbrt(t), t / (3 * delta**2) + 4/29)

    fx = f(xyz_normalized[..., 0])
    fy = f(xyz_normalized[..., 1])
    fz = f(xyz_normalized[..., 2])

    L = 116 * fy - 16
    a = 500 * (fx - fy)
    b = 200 * (fy - fz)

    return np.stack([L, a, b], axis=-1)


def lab_to_xyz(lab: np.ndarray,
               white: Tuple[float, float, float] = (0.95047, 1.0, 1.08883)) -> np.ndarray:
    """Convert Lab to XYZ."""
    L, a, b = lab[..., 0], lab[..., 1], lab[..., 2]

    fy = (L + 16) / 116
    fx = a / 500 + fy
    fz = fy - b / 200

    def f_inv(t):
        delta = 6 / 29
        return np.where(t > delta, t**3, 3 * delta**2 * (t - 4/29))

    x = f_inv(fx)
    y = f_inv(fy)
    z = f_inv(fz)

    xyz = np.stack([x, y, z], axis=-1) * np.array(white)
    return xyz


def xyz_to_rgb(xyz: np.ndarray, gamma: float = 2.2) -> np.ndarray:
    """Convert XYZ to RGB (sRGB primaries)."""
    # XYZ to sRGB matrix
    matrix = np.array([
        [3.2404542, -1.5371385, -0.4985314],
        [-0.9692660, 1.8760108, 0.0415560],
        [0.0556434, -0.2040259, 1.0572252]
    ])

    rgb_linear = np.dot(xyz, matrix.T)
    rgb_linear = np.clip(rgb_linear, 0, 1)

    # Apply gamma
    return np.power(rgb_linear, 1/gamma)


def rgb_to_lab(rgb: np.ndarray) -> np.ndarray:
    """Convert RGB to Lab."""
    xyz = rgb_to_xyz(rgb)
    return xyz_to_lab(xyz)


def lab_to_rgb(lab: np.ndarray) -> np.ndarray:
    """Convert Lab to RGB."""
    xyz = lab_to_xyz(lab)
    return xyz_to_rgb(xyz)


def delta_e_2000(lab1: np.ndarray, lab2: np.ndarray) -> np.ndarray:
    """Calculate CIEDE2000 Delta E (vectorized)."""
    L1, a1, b1 = lab1[..., 0], lab1[..., 1], lab1[..., 2]
    L2, a2, b2 = lab2[..., 0], lab2[..., 1], lab2[..., 2]

    C1 = np.sqrt(a1**2 + b1**2)
    C2 = np.sqrt(a2**2 + b2**2)
    C_avg = (C1 + C2) / 2

    G = 0.5 * (1 - np.sqrt(C_avg**7 / (C_avg**7 + 25**7)))

    a1_prime = a1 * (1 + G)
    a2_prime = a2 * (1 + G)

    C1_prime = np.sqrt(a1_prime**2 + b1**2)
    C2_prime = np.sqrt(a2_prime**2 + b2**2)

    h1_prime = np.degrees(np.arctan2(b1, a1_prime)) % 360
    h2_prime = np.degrees(np.arctan2(b2, a2_prime)) % 360

    delta_L_prime = L2 - L1
    delta_C_prime = C2_prime - C1_prime

    delta_h_prime = h2_prime - h1_prime
    delta_h_prime = np.where(np.abs(delta_h_prime) > 180,
                              delta_h_prime - np.sign(delta_h_prime) * 360,
                              delta_h_prime)

    delta_H_prime = 2 * np.sqrt(C1_prime * C2_prime) * np.sin(np.radians(delta_h_prime / 2))

    L_prime_avg = (L1 + L2) / 2
    C_prime_avg = (C1_prime + C2_prime) / 2

    h_prime_sum = h1_prime + h2_prime
    h_prime_avg = np.where(
        np.abs(h1_prime - h2_prime) > 180,
        (h_prime_sum + 360) / 2,
        h_prime_sum / 2
    )

    T = (1 - 0.17 * np.cos(np.radians(h_prime_avg - 30)) +
         0.24 * np.cos(np.radians(2 * h_prime_avg)) +
         0.32 * np.cos(np.radians(3 * h_prime_avg + 6)) -
         0.20 * np.cos(np.radians(4 * h_prime_avg - 63)))

    delta_theta = 30 * np.exp(-((h_prime_avg - 275) / 25)**2)
    R_C = 2 * np.sqrt(C_prime_avg**7 / (C_prime_avg**7 + 25**7))

    S_L = 1 + (0.015 * (L_prime_avg - 50)**2) / np.sqrt(20 + (L_prime_avg - 50)**2)
    S_C = 1 + 0.045 * C_prime_avg
    S_H = 1 + 0.015 * C_prime_avg * T

    R_T = -np.sin(np.radians(2 * delta_theta)) * R_C

    delta_E = np.sqrt(
        (delta_L_prime / S_L)**2 +
        (delta_C_prime / S_C)**2 +
        (delta_H_prime / S_H)**2 +
        R_T * (delta_C_prime / S_C) * (delta_H_prime / S_H)
    )

    return delta_E


# =============================================================================
# LUT Analysis Functions
# =============================================================================

def analyze_lut_quality(lut: np.ndarray,
                        reference: Optional[np.ndarray] = None) -> LUTQualityMetrics:
    """
    Analyze quality metrics of a 3D LUT.

    Args:
        lut: 3D LUT array (size x size x size x 3)
        reference: Optional reference LUT to compare against

    Returns:
        LUTQualityMetrics with comprehensive analysis
    """
    size = lut.shape[0]
    metrics = LUTQualityMetrics(lut_size=size)

    # Calculate error metrics if reference provided
    if reference is not None:
        lut_lab = rgb_to_lab(lut.reshape(-1, 3))
        ref_lab = rgb_to_lab(reference.reshape(-1, 3))

        delta_e = delta_e_2000(lut_lab, ref_lab)

        metrics.delta_e_mean = float(np.mean(delta_e))
        metrics.delta_e_max = float(np.max(delta_e))
        metrics.delta_e_std = float(np.std(delta_e))
        metrics.delta_e_95th = float(np.percentile(delta_e, 95))

    # Calculate gradient (smoothness)
    gradients = []
    for channel in range(3):
        gx = np.gradient(lut[..., channel], axis=0)
        gy = np.gradient(lut[..., channel], axis=1)
        gz = np.gradient(lut[..., channel], axis=2)
        grad_mag = np.sqrt(gx**2 + gy**2 + gz**2)
        gradients.append(grad_mag)

    total_gradient = np.sqrt(sum(g**2 for g in gradients))
    metrics.gradient_mean = float(np.mean(total_gradient))
    metrics.gradient_max = float(np.max(total_gradient))

    # Detect discontinuities
    threshold = metrics.gradient_mean * 3
    metrics.discontinuity_count = int(np.sum(total_gradient > threshold))

    # Check for clipped values
    clipped = np.logical_or(lut < 0, lut > 1)
    metrics.clipped_values_percent = float(np.mean(clipped)) * 100

    # Check out of gamut
    rgb_values = lut.reshape(-1, 3)
    oog = np.any(np.logical_or(rgb_values < 0, rgb_values > 1), axis=1)
    metrics.out_of_gamut_percent = float(np.mean(oog)) * 100

    # Calculate size
    metrics.total_size_bytes = lut.nbytes

    return metrics


def analyze_interpolation_quality(lut: np.ndarray,
                                  test_points: int = 1000) -> Tuple[float, float]:
    """
    Analyze interpolation quality of a LUT.

    Compares direct LUT lookup vs trilinear interpolation
    at random intermediate points.

    Returns:
        (mean_error, max_error)
    """
    size = lut.shape[0]

    # Create interpolator
    x = np.linspace(0, 1, size)
    interpolator = RegularGridInterpolator(
        (x, x, x), lut,
        method='linear', bounds_error=False, fill_value=None
    )

    # Generate random test points
    np.random.seed(42)
    test_rgb = np.random.rand(test_points, 3)

    # Get interpolated values
    interpolated = interpolator(test_rgb)

    # Get nearest LUT values for comparison
    indices = np.round(test_rgb * (size - 1)).astype(int)
    indices = np.clip(indices, 0, size - 1)
    direct = lut[indices[:, 0], indices[:, 1], indices[:, 2]]

    # Calculate Lab difference
    interp_lab = rgb_to_lab(interpolated)
    direct_lab = rgb_to_lab(direct)
    errors = delta_e_2000(interp_lab, direct_lab)

    return float(np.mean(errors)), float(np.max(errors))


# =============================================================================
# LUT Smoothing Functions
# =============================================================================

def smooth_lut_gaussian(lut: np.ndarray, sigma: float = 0.5) -> np.ndarray:
    """Apply Gaussian smoothing to LUT."""
    smoothed = np.zeros_like(lut)
    for channel in range(3):
        smoothed[..., channel] = gaussian_filter(lut[..., channel], sigma=sigma)
    return np.clip(smoothed, 0, 1)


def smooth_lut_perceptual(lut: np.ndarray,
                          sigma: float = 0.5,
                          preserve_edges: bool = True) -> np.ndarray:
    """
    Apply perceptual smoothing in Lab space.

    Smoothing is applied in Lab space for perceptually uniform results.
    """
    size = lut.shape[0]

    # Convert to Lab
    lut_lab = rgb_to_lab(lut.reshape(-1, 3)).reshape(size, size, size, 3)

    # Apply smoothing to each Lab channel
    smoothed_lab = np.zeros_like(lut_lab)

    for channel in range(3):
        if preserve_edges and channel > 0:  # Preserve edges in a,b channels
            # Use smaller sigma for chromatic channels
            smoothed_lab[..., channel] = gaussian_filter(
                lut_lab[..., channel], sigma=sigma * 0.5
            )
        else:
            smoothed_lab[..., channel] = gaussian_filter(
                lut_lab[..., channel], sigma=sigma
            )

    # Convert back to RGB
    smoothed_rgb = lab_to_rgb(smoothed_lab.reshape(-1, 3)).reshape(size, size, size, 3)

    return np.clip(smoothed_rgb, 0, 1)


def smooth_lut_bilateral(lut: np.ndarray,
                         sigma_spatial: float = 1.0,
                         sigma_range: float = 0.1) -> np.ndarray:
    """
    Apply bilateral smoothing to LUT (edge-preserving).

    This is a simplified bilateral filter implementation.
    """
    size = lut.shape[0]
    smoothed = np.zeros_like(lut)

    # Kernel size
    kernel_size = int(sigma_spatial * 3) * 2 + 1
    half_k = kernel_size // 2

    for i in range(size):
        for j in range(size):
            for k in range(size):
                # Get neighborhood
                i_min, i_max = max(0, i - half_k), min(size, i + half_k + 1)
                j_min, j_max = max(0, j - half_k), min(size, j + half_k + 1)
                k_min, k_max = max(0, k - half_k), min(size, k + half_k + 1)

                neighborhood = lut[i_min:i_max, j_min:j_max, k_min:k_max]
                center = lut[i, j, k]

                # Spatial weights
                ii, jj, kk = np.meshgrid(
                    np.arange(i_min, i_max) - i,
                    np.arange(j_min, j_max) - j,
                    np.arange(k_min, k_max) - k,
                    indexing='ij'
                )
                spatial_weight = np.exp(-(ii**2 + jj**2 + kk**2) / (2 * sigma_spatial**2))

                # Range weights
                diff = np.sqrt(np.sum((neighborhood - center)**2, axis=-1))
                range_weight = np.exp(-diff**2 / (2 * sigma_range**2))

                # Combined weights
                weight = spatial_weight * range_weight
                weight = weight / np.sum(weight)

                # Apply weighted average
                for c in range(3):
                    smoothed[i, j, k, c] = np.sum(weight * neighborhood[..., c])

    return np.clip(smoothed, 0, 1)


# =============================================================================
# Gamut Mapping Functions
# =============================================================================

def map_gamut_clip(lut: np.ndarray) -> np.ndarray:
    """Simple RGB clipping for out-of-gamut colors."""
    return np.clip(lut, 0, 1)


def map_gamut_compress(lut: np.ndarray,
                       compression_factor: float = 0.8) -> np.ndarray:
    """
    Compress chroma for out-of-gamut colors.

    Reduces saturation to bring colors within gamut while
    preserving hue and relative saturation.
    """
    size = lut.shape[0]

    # Convert to Lab
    lut_lab = rgb_to_lab(lut.reshape(-1, 3)).reshape(size, size, size, 3)

    # Get chroma
    L = lut_lab[..., 0]
    a = lut_lab[..., 1]
    b = lut_lab[..., 2]
    C = np.sqrt(a**2 + b**2)
    h = np.arctan2(b, a)

    # Compress chroma
    C_compressed = C * compression_factor

    # Reconstruct Lab
    lut_lab[..., 1] = C_compressed * np.cos(h)
    lut_lab[..., 2] = C_compressed * np.sin(h)

    # Convert back to RGB
    result = lab_to_rgb(lut_lab.reshape(-1, 3)).reshape(size, size, size, 3)

    # Final clip
    return np.clip(result, 0, 1)


def map_gamut_perceptual(lut: np.ndarray,
                         target_gamut: str = "srgb") -> np.ndarray:
    """
    Perceptual gamut mapping.

    Compresses out-of-gamut colors toward the gamut boundary
    while preserving perceptual attributes.
    """
    size = lut.shape[0]

    # Convert to Lab
    lut_lab = rgb_to_lab(lut.reshape(-1, 3)).reshape(size, size, size, 3)

    # For each point, check if in gamut and compress if needed
    for i in range(size):
        for j in range(size):
            for k in range(size):
                rgb = lut[i, j, k]

                # Check if out of gamut
                if np.any(rgb < 0) or np.any(rgb > 1):
                    lab = lut_lab[i, j, k]
                    L, a, b = lab

                    # Binary search for gamut boundary
                    C = np.sqrt(a**2 + b**2)
                    if C > 0:
                        h = np.arctan2(b, a)

                        # Find maximum chroma in gamut
                        low, high = 0, C
                        for _ in range(10):  # Binary search iterations
                            mid = (low + high) / 2
                            test_lab = np.array([L, mid * np.cos(h), mid * np.sin(h)])
                            test_rgb = lab_to_rgb(test_lab.reshape(1, 3))[0]

                            if np.all(test_rgb >= 0) and np.all(test_rgb <= 1):
                                low = mid
                            else:
                                high = mid

                        # Apply compressed chroma
                        lut_lab[i, j, k, 1] = low * np.cos(h)
                        lut_lab[i, j, k, 2] = low * np.sin(h)

    # Convert back to RGB
    result = lab_to_rgb(lut_lab.reshape(-1, 3)).reshape(size, size, size, 3)

    return np.clip(result, 0, 1)


# =============================================================================
# LUT Optimizer Class
# =============================================================================

class LUTOptimizer:
    """
    Advanced 3D LUT optimization engine.

    Provides various optimization techniques for improving
    LUT accuracy and smoothness.
    """

    def __init__(self,
                 goal: OptimizationGoal = OptimizationGoal.BALANCED,
                 smoothing_config: Optional[SmoothingConfig] = None,
                 gamut_config: Optional[GamutConfig] = None):
        """
        Initialize optimizer.

        Args:
            goal: Optimization goal
            smoothing_config: Smoothing configuration
            gamut_config: Gamut mapping configuration
        """
        self.goal = goal
        self.smoothing = smoothing_config or SmoothingConfig()
        self.gamut = gamut_config or GamutConfig()

    def optimize(self,
                 lut: np.ndarray,
                 reference: Optional[np.ndarray] = None,
                 target_delta_e: float = 1.0) -> OptimizationResult:
        """
        Optimize a 3D LUT.

        Args:
            lut: Input 3D LUT
            reference: Optional reference LUT for error calculation
            target_delta_e: Target average Delta E

        Returns:
            OptimizationResult with optimized LUT and metrics
        """
        import time
        start_time = time.time()

        # Analyze original
        original_metrics = analyze_lut_quality(lut, reference)

        # Apply optimizations based on goal
        optimized = lut.copy()

        if self.goal == OptimizationGoal.SMOOTH:
            optimized = self._optimize_smooth(optimized)
        elif self.goal == OptimizationGoal.MIN_DELTA_E:
            optimized = self._optimize_accuracy(optimized, reference, target_delta_e)
        elif self.goal == OptimizationGoal.MIN_MAX_ERROR:
            optimized = self._optimize_max_error(optimized, reference)
        else:  # BALANCED
            optimized = self._optimize_balanced(optimized, reference, target_delta_e)

        # Apply gamut mapping
        optimized = self._apply_gamut_mapping(optimized)

        # Analyze optimized
        optimized_metrics = analyze_lut_quality(optimized, reference)

        # Calculate improvement
        if original_metrics.delta_e_mean > 0:
            improvement = (1 - optimized_metrics.delta_e_mean / original_metrics.delta_e_mean) * 100
        else:
            improvement = 0.0

        processing_time = time.time() - start_time

        return OptimizationResult(
            optimized_lut=optimized,
            original_metrics=original_metrics,
            optimized_metrics=optimized_metrics,
            method=self.goal.name,
            improvement_percent=improvement,
            processing_time=processing_time,
        )

    def _optimize_smooth(self, lut: np.ndarray) -> np.ndarray:
        """Optimize for smoothness."""
        if self.smoothing.method == SmoothingMethod.GAUSSIAN:
            return smooth_lut_gaussian(lut, self.smoothing.sigma)
        elif self.smoothing.method == SmoothingMethod.PERCEPTUAL:
            return smooth_lut_perceptual(
                lut, self.smoothing.sigma, self.smoothing.preserve_edges
            )
        elif self.smoothing.method == SmoothingMethod.BILATERAL:
            return smooth_lut_bilateral(lut, self.smoothing.sigma)
        return lut

    def _optimize_accuracy(self,
                          lut: np.ndarray,
                          reference: Optional[np.ndarray],
                          target_delta_e: float) -> np.ndarray:
        """Optimize for minimum Delta E."""
        if reference is None:
            return lut

        # Iteratively adjust towards reference
        result = lut.copy()
        size = lut.shape[0]

        for _ in range(3):  # Iterations
            lut_lab = rgb_to_lab(result.reshape(-1, 3))
            ref_lab = rgb_to_lab(reference.reshape(-1, 3))

            # Calculate per-point Delta E
            delta_e = delta_e_2000(lut_lab, ref_lab)

            # Adjust points with high error
            mask = delta_e > target_delta_e
            if not np.any(mask):
                break

            # Blend towards reference
            blend = 0.5
            lut_lab[mask] = lut_lab[mask] * (1 - blend) + ref_lab[mask] * blend

            result = lab_to_rgb(lut_lab).reshape(size, size, size, 3)
            result = np.clip(result, 0, 1)

        return result

    def _optimize_max_error(self,
                           lut: np.ndarray,
                           reference: Optional[np.ndarray]) -> np.ndarray:
        """Optimize to minimize maximum error."""
        if reference is None:
            return lut

        result = lut.copy()
        size = lut.shape[0]

        for _ in range(5):
            lut_lab = rgb_to_lab(result.reshape(-1, 3))
            ref_lab = rgb_to_lab(reference.reshape(-1, 3))

            delta_e = delta_e_2000(lut_lab, ref_lab)
            max_error = np.max(delta_e)

            if max_error < 2.0:
                break

            # Find worst points
            threshold = max_error * 0.8
            mask = delta_e > threshold

            # Blend towards reference
            blend = 0.7
            lut_lab[mask] = lut_lab[mask] * (1 - blend) + ref_lab[mask] * blend

            result = lab_to_rgb(lut_lab).reshape(size, size, size, 3)
            result = np.clip(result, 0, 1)

        return result

    def _optimize_balanced(self,
                          lut: np.ndarray,
                          reference: Optional[np.ndarray],
                          target_delta_e: float) -> np.ndarray:
        """Balance accuracy and smoothness."""
        # First optimize accuracy
        result = self._optimize_accuracy(lut, reference, target_delta_e)

        # Then apply light smoothing
        old_sigma = self.smoothing.sigma
        self.smoothing.sigma = old_sigma * 0.5
        result = self._optimize_smooth(result)
        self.smoothing.sigma = old_sigma

        return result

    def _apply_gamut_mapping(self, lut: np.ndarray) -> np.ndarray:
        """Apply gamut mapping."""
        if self.gamut.method == GamutMappingMethod.CLIP:
            return map_gamut_clip(lut)
        elif self.gamut.method == GamutMappingMethod.COMPRESS:
            return map_gamut_compress(lut, self.gamut.compression_factor)
        elif self.gamut.method == GamutMappingMethod.PERCEPTUAL:
            return map_gamut_perceptual(lut, self.gamut.target_gamut)
        return map_gamut_clip(lut)


# =============================================================================
# Utility Functions
# =============================================================================

def create_identity_lut(size: int = 17) -> np.ndarray:
    """Create an identity 3D LUT."""
    lut = np.zeros((size, size, size, 3), dtype=np.float64)

    for r in range(size):
        for g in range(size):
            for b in range(size):
                lut[r, g, b] = [
                    r / (size - 1),
                    g / (size - 1),
                    b / (size - 1)
                ]

    return lut


def create_test_lut(size: int = 17,
                    contrast: float = 1.1,
                    saturation: float = 1.05) -> np.ndarray:
    """Create a test LUT with contrast/saturation adjustments."""
    identity = create_identity_lut(size)

    # Convert to Lab for adjustments
    lab = rgb_to_lab(identity.reshape(-1, 3))

    # Adjust L (contrast)
    lab[:, 0] = (lab[:, 0] - 50) * contrast + 50

    # Adjust a,b (saturation)
    lab[:, 1] *= saturation
    lab[:, 2] *= saturation

    # Convert back
    result = lab_to_rgb(lab).reshape(size, size, size, 3)
    return np.clip(result, 0, 1)


def print_optimization_summary(result: OptimizationResult) -> None:
    """Print optimization summary."""
    print("\n" + "=" * 60)
    print("LUT Optimization Summary")
    print("=" * 60)
    print(f"Method: {result.method}")
    print(f"Processing Time: {result.processing_time:.2f}s")
    print()
    print("Original Metrics:")
    print(f"  Delta E Mean: {result.original_metrics.delta_e_mean:.3f}")
    print(f"  Delta E Max:  {result.original_metrics.delta_e_max:.3f}")
    print(f"  Gradient Mean: {result.original_metrics.gradient_mean:.4f}")
    print()
    print("Optimized Metrics:")
    print(f"  Delta E Mean: {result.optimized_metrics.delta_e_mean:.3f}")
    print(f"  Delta E Max:  {result.optimized_metrics.delta_e_max:.3f}")
    print(f"  Gradient Mean: {result.optimized_metrics.gradient_mean:.4f}")
    print()
    print(f"Improvement: {result.improvement_percent:.1f}%")
    print("=" * 60)


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    # Create test LUTs
    print("Creating test LUTs...")
    identity = create_identity_lut(17)
    test_lut = create_test_lut(17, contrast=1.15, saturation=1.1)

    # Add some noise
    noise = np.random.normal(0, 0.02, test_lut.shape)
    noisy_lut = np.clip(test_lut + noise, 0, 1)

    # Analyze original
    print("\nAnalyzing noisy LUT...")
    metrics = analyze_lut_quality(noisy_lut, identity)
    print(f"Delta E Mean: {metrics.delta_e_mean:.3f}")
    print(f"Delta E Max: {metrics.delta_e_max:.3f}")
    print(f"Gradient Mean: {metrics.gradient_mean:.4f}")

    # Optimize
    print("\nOptimizing...")
    optimizer = LUTOptimizer(
        goal=OptimizationGoal.BALANCED,
        smoothing_config=SmoothingConfig(
            method=SmoothingMethod.PERCEPTUAL,
            sigma=0.5,
        ),
        gamut_config=GamutConfig(
            method=GamutMappingMethod.PERCEPTUAL,
        ),
    )

    result = optimizer.optimize(noisy_lut, identity, target_delta_e=1.0)
    print_optimization_summary(result)

    # Test interpolation quality
    print("\nInterpolation quality analysis...")
    mean_err, max_err = analyze_interpolation_quality(result.optimized_lut)
    print(f"Mean interpolation error: {mean_err:.4f}")
    print(f"Max interpolation error: {max_err:.4f}")
