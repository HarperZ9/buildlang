"""
HDR10+ Dynamic Metadata

Implements Samsung HDR10+ (SMPTE ST.2094-40) dynamic metadata for
scene-by-scene tone mapping optimization.

HDR10+ extends HDR10 with:
- Per-scene brightness information
- Bezier curve-based tone mapping
- MaxSCL (Scene Content Light) values
- Distribution percentiles
"""

import numpy as np
from dataclasses import dataclass, field
from typing import List, Optional, Tuple, Dict, Any, Union
from enum import IntEnum
import struct
import json


# =============================================================================
# HDR10+ Constants
# =============================================================================

# Application identifiers
HDR10PLUS_APPLICATION_IDENTIFIER = 4
HDR10PLUS_APPLICATION_VERSION = 1

# Processing windows
MAX_WINDOWS = 3

# Bezier curve
MAX_BEZIER_ANCHORS = 15

# Distribution percentiles
NUM_PERCENTILES = 9
DEFAULT_PERCENTILES = [1, 5, 10, 25, 50, 75, 90, 95, 99]


class ProcessingWindowFlag(IntEnum):
    """Processing window types."""
    FULL_FRAME = 0
    ELLIPTICAL = 1
    RECTANGULAR = 2


# =============================================================================
# HDR10+ Data Structures
# =============================================================================

@dataclass
class BezierCurve:
    """
    Bezier curve for tone mapping.

    HDR10+ uses cubic Bezier curves to define the tone mapping function
    from mastering display to target display.
    """
    knee_point_x: float = 0.0      # Normalized [0, 1]
    knee_point_y: float = 0.0      # Normalized [0, 1]
    anchors: List[float] = field(default_factory=list)  # Up to 15 anchor points

    def evaluate(self, t: np.ndarray) -> np.ndarray:
        """
        Evaluate Bezier curve at parameter t.

        Args:
            t: Parameter values [0, 1]

        Returns:
            Curve values at each t
        """
        t = np.asarray(t, dtype=np.float64)
        t = np.clip(t, 0.0, 1.0)

        if not self.anchors:
            # Linear fallback
            return t

        # Build control points
        # Start at (0, 0), knee point, anchors, end at (1, 1)
        n = len(self.anchors) + 2

        # Simple linear interpolation through anchors for now
        # Full Bezier implementation would use de Casteljau's algorithm
        x_points = np.linspace(0, 1, n)
        y_points = np.concatenate([
            [0.0],
            [self.knee_point_y],
            self.anchors[:n-2] if len(self.anchors) >= n-2 else self.anchors + [1.0] * (n-2-len(self.anchors)),
            [1.0]
        ])[:n]

        # Interpolate
        result = np.interp(t, x_points, y_points)

        return np.clip(result, 0.0, 1.0)

    def to_lut(self, size: int = 1024) -> np.ndarray:
        """Convert Bezier curve to 1D LUT."""
        t = np.linspace(0, 1, size)
        return self.evaluate(t)


@dataclass
class DistributionData:
    """
    Luminance distribution data for a scene.

    Describes how brightness values are distributed in the scene
    using percentile values.
    """
    percentiles: List[int] = field(default_factory=lambda: DEFAULT_PERCENTILES.copy())
    values: List[float] = field(default_factory=list)  # Luminance at each percentile

    def get_percentile_value(self, percentile: int) -> Optional[float]:
        """Get luminance value at a specific percentile."""
        if percentile in self.percentiles:
            idx = self.percentiles.index(percentile)
            if idx < len(self.values):
                return self.values[idx]
        return None

    @property
    def median(self) -> Optional[float]:
        """Get median (50th percentile) luminance."""
        return self.get_percentile_value(50)

    @property
    def peak(self) -> Optional[float]:
        """Get 99th percentile as approximate peak."""
        return self.get_percentile_value(99)


@dataclass
class ProcessingWindow:
    """
    HDR10+ processing window.

    Defines a region of the frame with specific tone mapping parameters.
    """
    window_id: int = 0
    window_type: ProcessingWindowFlag = ProcessingWindowFlag.FULL_FRAME

    # Window geometry (for non-full-frame)
    center_x: float = 0.5
    center_y: float = 0.5
    width: float = 1.0
    height: float = 1.0
    rotation: float = 0.0

    # Scene content light levels (cd/m²)
    max_scl: Tuple[float, float, float] = (0.0, 0.0, 0.0)  # RGB maxima
    average_maxrgb: float = 0.0

    # Distribution
    distribution: DistributionData = field(default_factory=DistributionData)

    # Tone mapping curve
    tone_mapping_curve: Optional[BezierCurve] = None

    # Fraction of bright pixels
    fraction_bright_pixels: float = 0.0


@dataclass
class HDR10PlusMetadata:
    """
    Complete HDR10+ dynamic metadata for a single frame/scene.
    """
    # Application info
    application_identifier: int = HDR10PLUS_APPLICATION_IDENTIFIER
    application_version: int = HDR10PLUS_APPLICATION_VERSION

    # Targeted display info
    targeted_system_display_maximum_luminance: float = 1000.0  # cd/m²
    targeted_system_display_actual_peak_luminance_flag: bool = False

    # Mastering display info
    mastering_display_actual_peak_luminance: float = 1000.0

    # Processing windows
    num_windows: int = 1
    windows: List[ProcessingWindow] = field(default_factory=lambda: [ProcessingWindow()])

    # Frame info
    frame_number: int = 0
    scene_id: int = 0

    def get_max_scl(self) -> float:
        """Get maximum scene content light level across all windows."""
        max_val = 0.0
        for window in self.windows:
            max_val = max(max_val, max(window.max_scl))
        return max_val

    def get_average_brightness(self) -> float:
        """Get average MaxRGB across windows."""
        if not self.windows:
            return 0.0
        return sum(w.average_maxrgb for w in self.windows) / len(self.windows)

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "application_identifier": self.application_identifier,
            "application_version": self.application_version,
            "targeted_display_max_luminance": self.targeted_system_display_maximum_luminance,
            "mastering_display_peak_luminance": self.mastering_display_actual_peak_luminance,
            "num_windows": self.num_windows,
            "max_scl": self.get_max_scl(),
            "average_brightness": self.get_average_brightness(),
            "frame_number": self.frame_number,
            "scene_id": self.scene_id,
            "windows": [
                {
                    "window_id": w.window_id,
                    "max_scl_rgb": w.max_scl,
                    "average_maxrgb": w.average_maxrgb,
                    "fraction_bright": w.fraction_bright_pixels,
                }
                for w in self.windows
            ]
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "HDR10PlusMetadata":
        """Create from dictionary."""
        metadata = cls(
            targeted_system_display_maximum_luminance=data.get("targeted_display_max_luminance", 1000.0),
            mastering_display_actual_peak_luminance=data.get("mastering_display_peak_luminance", 1000.0),
            frame_number=data.get("frame_number", 0),
            scene_id=data.get("scene_id", 0),
        )

        if "windows" in data:
            metadata.windows = []
            for w_data in data["windows"]:
                window = ProcessingWindow(
                    window_id=w_data.get("window_id", 0),
                    max_scl=tuple(w_data.get("max_scl_rgb", (0, 0, 0))),
                    average_maxrgb=w_data.get("average_maxrgb", 0),
                    fraction_bright_pixels=w_data.get("fraction_bright", 0),
                )
                metadata.windows.append(window)
            metadata.num_windows = len(metadata.windows)

        return metadata


# =============================================================================
# HDR10+ Tone Mapping
# =============================================================================

class HDR10PlusToneMapper:
    """
    HDR10+ dynamic tone mapper.

    Applies scene-adaptive tone mapping based on HDR10+ metadata.
    """

    def __init__(
        self,
        target_max_luminance: float = 1000.0,
        target_min_luminance: float = 0.005
    ):
        """
        Initialize tone mapper.

        Args:
            target_max_luminance: Target display peak (cd/m²)
            target_min_luminance: Target display black (cd/m²)
        """
        self.target_max = target_max_luminance
        self.target_min = target_min_luminance

        # Cache for generated LUTs
        self._lut_cache: Dict[int, np.ndarray] = {}

    def generate_tone_curve(
        self,
        metadata: HDR10PlusMetadata,
        size: int = 1024
    ) -> np.ndarray:
        """
        Generate tone mapping curve from HDR10+ metadata.

        Args:
            metadata: HDR10+ metadata for current scene
            size: Output LUT size

        Returns:
            1D LUT for tone mapping
        """
        # Get source (mastering) peak
        source_max = metadata.mastering_display_actual_peak_luminance
        scene_max = metadata.get_max_scl()

        # Use scene max if available, otherwise mastering peak
        effective_source = min(scene_max if scene_max > 0 else source_max, source_max)

        # Check for Bezier curve in metadata
        if metadata.windows and metadata.windows[0].tone_mapping_curve:
            curve = metadata.windows[0].tone_mapping_curve
            return curve.to_lut(size)

        # Generate adaptive curve
        return self._generate_adaptive_curve(effective_source, size)

    def _generate_adaptive_curve(
        self,
        source_peak: float,
        size: int
    ) -> np.ndarray:
        """Generate adaptive tone curve based on source/target ratio."""
        t = np.linspace(0, 1, size)

        # Source luminance
        source_lum = t * source_peak

        if source_peak <= self.target_max:
            # No tone mapping needed - linear passthrough
            output = t
        else:
            # Apply tone mapping with soft roll-off
            ratio = self.target_max / source_peak

            # Knee point at 50% of target peak
            knee = 0.5 * self.target_max
            knee_norm = knee / source_peak

            # Below knee: linear with slight compression
            # Above knee: soft roll-off
            below_knee = source_lum <= knee

            output = np.zeros_like(t)

            # Linear region
            output[below_knee] = source_lum[below_knee] / self.target_max

            # Roll-off region using modified Reinhard
            above_lum = source_lum[~below_knee]
            compressed = knee + (self.target_max - knee) * (
                (above_lum - knee) / (above_lum - knee + (source_peak - knee) * 0.5)
            )
            output[~below_knee] = compressed / self.target_max

        return np.clip(output, 0.0, 1.0)

    def apply(
        self,
        rgb_linear: np.ndarray,
        metadata: HDR10PlusMetadata
    ) -> np.ndarray:
        """
        Apply HDR10+ tone mapping to linear RGB.

        Args:
            rgb_linear: Linear RGB values (luminance in cd/m²)
            metadata: HDR10+ metadata

        Returns:
            Tone-mapped linear RGB
        """
        # Generate or retrieve cached curve
        scene_id = metadata.scene_id
        if scene_id not in self._lut_cache:
            self._lut_cache[scene_id] = self.generate_tone_curve(metadata)

        curve = self._lut_cache[scene_id]

        # Normalize input to [0, 1]
        source_max = metadata.mastering_display_actual_peak_luminance
        rgb_norm = rgb_linear / source_max
        rgb_norm = np.clip(rgb_norm, 0, 1)

        # Apply curve per channel
        result = np.zeros_like(rgb_norm)
        for c in range(3):
            result[..., c] = np.interp(rgb_norm[..., c], np.linspace(0, 1, len(curve)), curve)

        # Scale to target
        return result * self.target_max

    def clear_cache(self):
        """Clear LUT cache."""
        self._lut_cache.clear()


# =============================================================================
# HDR10+ Scene Analysis
# =============================================================================

def analyze_frame(
    rgb_linear: np.ndarray,
    mastering_peak: float = 1000.0,
    num_percentiles: int = NUM_PERCENTILES
) -> HDR10PlusMetadata:
    """
    Analyze a frame to generate HDR10+ metadata.

    Args:
        rgb_linear: Linear RGB frame data (H, W, 3) in cd/m²
        mastering_peak: Mastering display peak luminance
        num_percentiles: Number of distribution percentiles

    Returns:
        HDR10PlusMetadata for the frame
    """
    rgb_linear = np.asarray(rgb_linear, dtype=np.float64)

    # Calculate MaxRGB per pixel
    max_rgb = np.max(rgb_linear, axis=-1)

    # Scene content light levels
    max_scl_r = float(np.max(rgb_linear[..., 0]))
    max_scl_g = float(np.max(rgb_linear[..., 1]))
    max_scl_b = float(np.max(rgb_linear[..., 2]))

    # Average MaxRGB
    avg_maxrgb = float(np.mean(max_rgb))

    # Calculate distribution
    percentiles = DEFAULT_PERCENTILES[:num_percentiles]
    percentile_values = [float(np.percentile(max_rgb, p)) for p in percentiles]

    # Fraction of bright pixels (above 50% of peak)
    bright_threshold = mastering_peak * 0.5
    fraction_bright = float(np.mean(max_rgb > bright_threshold))

    # Create metadata
    window = ProcessingWindow(
        window_id=0,
        window_type=ProcessingWindowFlag.FULL_FRAME,
        max_scl=(max_scl_r, max_scl_g, max_scl_b),
        average_maxrgb=avg_maxrgb,
        distribution=DistributionData(
            percentiles=percentiles,
            values=percentile_values
        ),
        fraction_bright_pixels=fraction_bright
    )

    return HDR10PlusMetadata(
        mastering_display_actual_peak_luminance=mastering_peak,
        targeted_system_display_maximum_luminance=1000.0,
        windows=[window],
        num_windows=1
    )


def detect_scene_change(
    current: HDR10PlusMetadata,
    previous: HDR10PlusMetadata,
    threshold: float = 0.3
) -> bool:
    """
    Detect if a scene change occurred between frames.

    Args:
        current: Current frame metadata
        previous: Previous frame metadata
        threshold: Change threshold (0-1)

    Returns:
        True if scene change detected
    """
    if not current.windows or not previous.windows:
        return True

    curr_avg = current.get_average_brightness()
    prev_avg = previous.get_average_brightness()

    if prev_avg == 0:
        return True

    # Relative change in average brightness
    change = abs(curr_avg - prev_avg) / max(curr_avg, prev_avg)

    return change > threshold


# =============================================================================
# HDR10+ Metadata Parsing
# =============================================================================

def parse_sei_payload(data: bytes) -> Optional[HDR10PlusMetadata]:
    """
    Parse HDR10+ SEI (Supplemental Enhancement Information) payload.

    Args:
        data: Raw SEI payload bytes

    Returns:
        HDR10PlusMetadata or None if parsing fails
    """
    if len(data) < 7:
        return None

    try:
        # ITU-T T.35 header
        itu_t_t35_country_code = data[0]
        if itu_t_t35_country_code != 0xB5:  # USA
            return None

        terminal_provider_code = (data[1] << 8) | data[2]
        if terminal_provider_code != 0x003C:  # Samsung
            return None

        terminal_provider_oriented_code = (data[3] << 8) | data[4]
        if terminal_provider_oriented_code != 0x0001:
            return None

        application_identifier = data[5]
        application_version = data[6]

        if application_identifier != HDR10PLUS_APPLICATION_IDENTIFIER:
            return None

        # Parse remaining metadata
        # This is a simplified parser - full implementation would need bit-level parsing
        metadata = HDR10PlusMetadata(
            application_identifier=application_identifier,
            application_version=application_version
        )

        return metadata

    except (IndexError, struct.error):
        return None


def serialize_metadata(metadata: HDR10PlusMetadata) -> bytes:
    """
    Serialize HDR10+ metadata to SEI payload format.

    Args:
        metadata: HDR10+ metadata to serialize

    Returns:
        SEI payload bytes
    """
    # ITU-T T.35 header
    payload = bytearray([
        0xB5,  # Country code (USA)
        0x00, 0x3C,  # Terminal provider code (Samsung)
        0x00, 0x01,  # Terminal provider oriented code
        metadata.application_identifier,
        metadata.application_version
    ])

    # Add simplified metadata payload
    # Full implementation would need proper bit-level serialization

    return bytes(payload)


# =============================================================================
# HDR10+ Calibration
# =============================================================================

def generate_hdr10plus_test_scenes() -> List[HDR10PlusMetadata]:
    """
    Generate test scenes for HDR10+ calibration.

    Returns:
        List of metadata for different test scenes
    """
    scenes = []

    # Dark scene
    dark = HDR10PlusMetadata(
        mastering_display_actual_peak_luminance=1000.0,
        scene_id=1
    )
    dark.windows[0].max_scl = (50.0, 50.0, 50.0)
    dark.windows[0].average_maxrgb = 10.0
    scenes.append(dark)

    # Medium scene
    medium = HDR10PlusMetadata(
        mastering_display_actual_peak_luminance=1000.0,
        scene_id=2
    )
    medium.windows[0].max_scl = (400.0, 400.0, 400.0)
    medium.windows[0].average_maxrgb = 100.0
    scenes.append(medium)

    # Bright scene
    bright = HDR10PlusMetadata(
        mastering_display_actual_peak_luminance=1000.0,
        scene_id=3
    )
    bright.windows[0].max_scl = (1000.0, 1000.0, 1000.0)
    bright.windows[0].average_maxrgb = 400.0
    scenes.append(bright)

    # Specular highlights
    specular = HDR10PlusMetadata(
        mastering_display_actual_peak_luminance=4000.0,
        scene_id=4
    )
    specular.windows[0].max_scl = (4000.0, 3000.0, 2000.0)
    specular.windows[0].average_maxrgb = 200.0
    specular.windows[0].fraction_bright_pixels = 0.05
    scenes.append(specular)

    return scenes


def create_hdr10plus_calibration_luts(
    target_peak: float,
    target_black: float = 0.005,
    size: int = 33
) -> Dict[str, np.ndarray]:
    """
    Create calibration LUTs for different HDR10+ scene types.

    Args:
        target_peak: Target display peak (cd/m²)
        target_black: Target display black (cd/m²)
        size: LUT size

    Returns:
        Dict mapping scene type to LUT
    """
    tone_mapper = HDR10PlusToneMapper(target_peak, target_black)
    scenes = generate_hdr10plus_test_scenes()

    luts = {}
    for scene in scenes:
        name = f"scene_{scene.scene_id}"
        luts[name] = tone_mapper.generate_tone_curve(scene, size)

    return luts
