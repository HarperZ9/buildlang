"""
Visual Matching Algorithms for Sensorless Calibration

Implements visual comparison and matching algorithms for user-guided
calibration without a hardware colorimeter.
"""

from dataclasses import dataclass
from enum import Enum
from typing import Optional, Callable
import numpy as np


class MatchingMethod(Enum):
    """Visual matching methods."""
    FLICKER = "flicker"  # Alternate between reference and display
    SIDE_BY_SIDE = "side_by_side"  # Show both simultaneously
    SPLIT_SCREEN = "split_screen"  # Split view comparison
    SEQUENTIAL = "sequential"  # Show one after another


class AdjustmentType(Enum):
    """Types of visual adjustments."""
    BRIGHTNESS = "brightness"
    CONTRAST = "contrast"
    RGB_BALANCE = "rgb_balance"
    GAMMA = "gamma"
    SATURATION = "saturation"
    HUE = "hue"


@dataclass
class MatchResult:
    """Result from a visual matching session."""
    adjustment_type: AdjustmentType
    initial_value: float
    final_value: float
    confidence: float  # User confidence in the match (0-1)
    iterations: int  # Number of adjustments made
    time_seconds: float  # Time taken for this match


@dataclass
class CalibrationAdjustment:
    """Adjustment to apply during calibration."""
    red_gain: float = 1.0
    green_gain: float = 1.0
    blue_gain: float = 1.0
    red_offset: float = 0.0
    green_offset: float = 0.0
    blue_offset: float = 0.0
    brightness: float = 1.0
    contrast: float = 1.0
    gamma: float = 2.2


class VisualMatcher:
    """
    Visual matching engine for sensorless calibration.

    Guides users through visual comparisons to calibrate display
    without hardware measurement devices.
    """

    def __init__(self, method: MatchingMethod = MatchingMethod.FLICKER):
        """
        Initialize visual matcher.

        Args:
            method: Matching method to use
        """
        self.method = method
        self.current_adjustment = CalibrationAdjustment()
        self.match_history: list[MatchResult] = []

    def set_method(self, method: MatchingMethod) -> None:
        """Set the visual matching method."""
        self.method = method

    def create_reference_patch(self, target_rgb: tuple[int, int, int],
                                 size: tuple[int, int] = (200, 200)) -> np.ndarray:
        """
        Create a reference color patch.

        Args:
            target_rgb: Target RGB color
            size: Patch size (width, height)

        Returns:
            Reference patch as numpy array
        """
        patch = np.zeros((size[1], size[0], 3), dtype=np.uint8)
        patch[:, :] = target_rgb
        return patch

    def create_display_patch(self, target_rgb: tuple[int, int, int],
                              adjustment: Optional[CalibrationAdjustment] = None,
                              size: tuple[int, int] = (200, 200)) -> np.ndarray:
        """
        Create an adjusted display patch for comparison.

        Args:
            target_rgb: Original RGB color
            adjustment: Calibration adjustment to apply
            size: Patch size (width, height)

        Returns:
            Adjusted patch as numpy array
        """
        adj = adjustment or self.current_adjustment

        # Apply adjustments
        r = target_rgb[0] * adj.red_gain + adj.red_offset
        g = target_rgb[1] * adj.green_gain + adj.green_offset
        b = target_rgb[2] * adj.blue_gain + adj.blue_offset

        # Apply brightness and contrast
        r = (r - 128) * adj.contrast + 128 + (adj.brightness - 1) * 255
        g = (g - 128) * adj.contrast + 128 + (adj.brightness - 1) * 255
        b = (b - 128) * adj.contrast + 128 + (adj.brightness - 1) * 255

        # Clamp values
        r = int(np.clip(r, 0, 255))
        g = int(np.clip(g, 0, 255))
        b = int(np.clip(b, 0, 255))

        patch = np.zeros((size[1], size[0], 3), dtype=np.uint8)
        patch[:, :] = [r, g, b]
        return patch

    def create_flicker_sequence(self, reference: np.ndarray,
                                  display: np.ndarray,
                                  frequency: float = 2.0,
                                  duration: float = 5.0) -> list[tuple[np.ndarray, float]]:
        """
        Create a flicker sequence for comparison.

        Args:
            reference: Reference patch
            display: Display patch
            frequency: Flicker frequency in Hz
            duration: Total sequence duration in seconds

        Returns:
            List of (patch, display_time) tuples
        """
        sequence = []
        period = 1.0 / frequency
        num_cycles = int(duration * frequency)

        for i in range(num_cycles * 2):
            patch = reference if i % 2 == 0 else display
            sequence.append((patch, period / 2))

        return sequence

    def create_split_view(self, reference: np.ndarray,
                           display: np.ndarray,
                           split: str = "vertical") -> np.ndarray:
        """
        Create a split-screen comparison view.

        Args:
            reference: Reference patch
            display: Display patch
            split: Split orientation ("vertical" or "horizontal")

        Returns:
            Combined split view image
        """
        if split == "vertical":
            combined = np.hstack([reference, display])
        else:
            combined = np.vstack([reference, display])
        return combined

    def calculate_adjustment_step(self, adjustment_type: AdjustmentType,
                                    direction: int,
                                    fine_mode: bool = False) -> CalibrationAdjustment:
        """
        Calculate the next adjustment step.

        Args:
            adjustment_type: Type of adjustment to make
            direction: Direction of adjustment (-1, 0, or 1)
            fine_mode: Use fine adjustment steps

        Returns:
            New adjustment values
        """
        step = 0.01 if fine_mode else 0.05
        adj = CalibrationAdjustment(
            red_gain=self.current_adjustment.red_gain,
            green_gain=self.current_adjustment.green_gain,
            blue_gain=self.current_adjustment.blue_gain,
            red_offset=self.current_adjustment.red_offset,
            green_offset=self.current_adjustment.green_offset,
            blue_offset=self.current_adjustment.blue_offset,
            brightness=self.current_adjustment.brightness,
            contrast=self.current_adjustment.contrast,
            gamma=self.current_adjustment.gamma,
        )

        if adjustment_type == AdjustmentType.BRIGHTNESS:
            adj.brightness += direction * step
        elif adjustment_type == AdjustmentType.CONTRAST:
            adj.contrast += direction * step
        elif adjustment_type == AdjustmentType.RGB_BALANCE:
            # Adjust individual channels based on sub-type
            pass
        elif adjustment_type == AdjustmentType.GAMMA:
            adj.gamma += direction * step * 2  # Larger steps for gamma

        return adj

    def apply_adjustment(self, adjustment: CalibrationAdjustment) -> None:
        """Apply a new adjustment."""
        self.current_adjustment = adjustment

    def record_match(self, adjustment_type: AdjustmentType,
                      initial_value: float,
                      final_value: float,
                      confidence: float,
                      iterations: int,
                      time_seconds: float) -> MatchResult:
        """
        Record a completed visual match.

        Args:
            adjustment_type: Type of adjustment made
            initial_value: Starting value
            final_value: Final matched value
            confidence: User's confidence in the match
            iterations: Number of adjustment iterations
            time_seconds: Time taken

        Returns:
            The recorded match result
        """
        result = MatchResult(
            adjustment_type=adjustment_type,
            initial_value=initial_value,
            final_value=final_value,
            confidence=confidence,
            iterations=iterations,
            time_seconds=time_seconds
        )
        self.match_history.append(result)
        return result

    def get_average_confidence(self) -> float:
        """Get average confidence across all matches."""
        if not self.match_history:
            return 0.0
        return sum(m.confidence for m in self.match_history) / len(self.match_history)

    def reset(self) -> None:
        """Reset matcher state."""
        self.current_adjustment = CalibrationAdjustment()
        self.match_history.clear()


class GrayscaleBalancer:
    """
    Grayscale balance adjustment using visual matching.

    Guides users through RGB balance adjustment for neutral grays.
    """

    def __init__(self, matcher: Optional[VisualMatcher] = None):
        """
        Initialize grayscale balancer.

        Args:
            matcher: Visual matcher to use
        """
        self.matcher = matcher or VisualMatcher()
        self.gray_levels = [25, 50, 75, 100, 128, 150, 175, 200, 225, 255]

    def create_gray_target(self, level: int,
                            size: tuple[int, int] = (300, 300)) -> np.ndarray:
        """Create a neutral gray target patch."""
        patch = np.zeros((size[1], size[0], 3), dtype=np.uint8)
        patch[:, :] = [level, level, level]
        return patch

    def get_adjustment_for_level(self, level: int) -> CalibrationAdjustment:
        """Get the current adjustment for a gray level."""
        return self.matcher.current_adjustment

    def calculate_rgb_correction(self, measured_rgb: tuple[int, int, int],
                                   target_gray: int) -> tuple[float, float, float]:
        """
        Calculate RGB gain corrections to achieve neutral gray.

        Args:
            measured_rgb: Measured/perceived RGB values
            target_gray: Target gray level

        Returns:
            (red_correction, green_correction, blue_correction) multipliers
        """
        if measured_rgb[0] == 0 or measured_rgb[1] == 0 or measured_rgb[2] == 0:
            return (1.0, 1.0, 1.0)

        # Calculate corrections relative to green (reference channel)
        avg = (measured_rgb[0] + measured_rgb[1] + measured_rgb[2]) / 3
        r_corr = avg / measured_rgb[0] if measured_rgb[0] > 0 else 1.0
        g_corr = avg / measured_rgb[1] if measured_rgb[1] > 0 else 1.0
        b_corr = avg / measured_rgb[2] if measured_rgb[2] > 0 else 1.0

        return (r_corr, g_corr, b_corr)


class WhitepointMatcher:
    """
    Whitepoint matching for target color temperature.

    Guides users to match display white to a target CCT (D50, D55, D65, etc.).
    """

    # Standard illuminant chromaticity coordinates (CIE 1931 xy)
    STANDARD_ILLUMINANTS = {
        "D50": (0.3457, 0.3585),
        "D55": (0.3324, 0.3474),
        "D65": (0.3127, 0.3290),
        "D75": (0.2990, 0.3149),
        "A": (0.4476, 0.4074),  # Incandescent
        "F2": (0.3721, 0.3751),  # Cool white fluorescent
    }

    def __init__(self, target: str = "D65"):
        """
        Initialize whitepoint matcher.

        Args:
            target: Target illuminant (D50, D55, D65, D75, etc.)
        """
        self.target = target
        self.target_xy = self.STANDARD_ILLUMINANTS.get(target, (0.3127, 0.3290))

    def cct_to_rgb(self, cct: float) -> tuple[int, int, int]:
        """
        Convert color temperature to approximate RGB values.

        Args:
            cct: Correlated Color Temperature in Kelvin

        Returns:
            (R, G, B) tuple
        """
        # Algorithm based on Tanner Helland's approach
        temp = cct / 100

        if temp <= 66:
            r = 255
            g = 99.4708025861 * np.log(temp) - 161.1195681661
            if temp <= 19:
                b = 0
            else:
                b = 138.5177312231 * np.log(temp - 10) - 305.0447927307
        else:
            r = 329.698727446 * ((temp - 60) ** -0.1332047592)
            g = 288.1221695283 * ((temp - 60) ** -0.0755148492)
            b = 255

        r = int(np.clip(r, 0, 255))
        g = int(np.clip(g, 0, 255))
        b = int(np.clip(b, 0, 255))

        return (r, g, b)

    def get_target_rgb(self) -> tuple[int, int, int]:
        """Get RGB approximation for target whitepoint."""
        cct_map = {
            "D50": 5000,
            "D55": 5500,
            "D65": 6500,
            "D75": 7500,
            "A": 2856,
            "F2": 4230,
        }
        cct = cct_map.get(self.target, 6500)
        return self.cct_to_rgb(cct)

    def create_whitepoint_comparison(self,
                                       current_rgb: tuple[int, int, int],
                                       size: tuple[int, int] = (400, 200)) -> np.ndarray:
        """
        Create a side-by-side whitepoint comparison.

        Args:
            current_rgb: Current display white RGB
            size: Total comparison size

        Returns:
            Comparison image
        """
        half_width = size[0] // 2
        target = np.zeros((size[1], half_width, 3), dtype=np.uint8)
        current = np.zeros((size[1], half_width, 3), dtype=np.uint8)

        target[:, :] = self.get_target_rgb()
        current[:, :] = current_rgb

        return np.hstack([target, current])


def create_visual_matcher(method: MatchingMethod = MatchingMethod.FLICKER) -> VisualMatcher:
    """
    Create a visual matcher with specified method.

    Args:
        method: Matching method to use

    Returns:
        Configured VisualMatcher instance
    """
    return VisualMatcher(method)
