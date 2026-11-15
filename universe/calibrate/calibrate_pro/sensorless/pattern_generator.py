"""
Test Pattern Generation for Sensorless Calibration

Generates various test patterns for display calibration including:
- Grayscale ramps
- Color primaries and secondaries
- Color checker patches
- Gradient patterns
- Uniformity test patterns
"""

from dataclasses import dataclass
from enum import Enum
from typing import Optional, Generator
import numpy as np


class PatternType(Enum):
    """Types of calibration test patterns."""
    SOLID = "solid"
    GRAYSCALE_RAMP = "grayscale_ramp"
    RGB_PRIMARIES = "rgb_primaries"
    RGBCMY = "rgbcmy"
    COLORCHECKER = "colorchecker"
    GRADIENT_H = "gradient_horizontal"
    GRADIENT_V = "gradient_vertical"
    UNIFORMITY_GRID = "uniformity_grid"
    WINDOW = "window"
    CROSSHATCH = "crosshatch"


@dataclass
class PatternConfig:
    """Configuration for pattern generation."""
    width: int = 1920
    height: int = 1080
    bit_depth: int = 8
    background_level: float = 0.0
    window_size: float = 0.1  # Percentage of screen for window patterns


@dataclass
class TestPattern:
    """Generated test pattern."""
    pattern_type: PatternType
    data: np.ndarray
    rgb_value: Optional[tuple[int, int, int]] = None
    name: str = ""
    metadata: Optional[dict] = None


class PatternGenerator:
    """Generates test patterns for display calibration."""

    def __init__(self, config: Optional[PatternConfig] = None):
        """
        Initialize pattern generator.

        Args:
            config: Pattern configuration settings
        """
        self.config = config or PatternConfig()
        self.max_value = (2 ** self.config.bit_depth) - 1

    def generate_solid(self, r: int, g: int, b: int) -> TestPattern:
        """
        Generate a solid color pattern.

        Args:
            r: Red value (0-255)
            g: Green value (0-255)
            b: Blue value (0-255)

        Returns:
            Solid color test pattern
        """
        data = np.zeros((self.config.height, self.config.width, 3), dtype=np.uint8)
        data[:, :] = [r, g, b]

        return TestPattern(
            pattern_type=PatternType.SOLID,
            data=data,
            rgb_value=(r, g, b),
            name=f"Solid RGB({r},{g},{b})"
        )

    def generate_grayscale_ramp(self, steps: int = 21) -> list[TestPattern]:
        """
        Generate grayscale ramp patterns.

        Args:
            steps: Number of grayscale steps (default 21 for 5% increments)

        Returns:
            List of grayscale test patterns
        """
        patterns = []
        for i in range(steps):
            level = int((i / (steps - 1)) * 255)
            pattern = self.generate_solid(level, level, level)
            pattern.name = f"Gray {int(i / (steps - 1) * 100)}%"
            patterns.append(pattern)
        return patterns

    def generate_primaries(self) -> list[TestPattern]:
        """
        Generate RGB primary color patterns.

        Returns:
            List of R, G, B test patterns
        """
        return [
            self.generate_solid(255, 0, 0),
            self.generate_solid(0, 255, 0),
            self.generate_solid(0, 0, 255),
        ]

    def generate_rgbcmy(self) -> list[TestPattern]:
        """
        Generate RGB + CMY patterns.

        Returns:
            List of R, G, B, C, M, Y test patterns
        """
        colors = [
            (255, 0, 0, "Red"),
            (0, 255, 0, "Green"),
            (0, 0, 255, "Blue"),
            (0, 255, 255, "Cyan"),
            (255, 0, 255, "Magenta"),
            (255, 255, 0, "Yellow"),
        ]
        patterns = []
        for r, g, b, name in colors:
            pattern = self.generate_solid(r, g, b)
            pattern.name = name
            patterns.append(pattern)
        return patterns

    def generate_window(self, r: int, g: int, b: int,
                        window_percent: float = 10.0) -> TestPattern:
        """
        Generate a window pattern (colored rectangle on black background).

        Args:
            r, g, b: Window color
            window_percent: Window size as percentage of screen

        Returns:
            Window test pattern
        """
        data = np.zeros((self.config.height, self.config.width, 3), dtype=np.uint8)

        # Calculate window dimensions
        window_w = int(self.config.width * (window_percent / 100))
        window_h = int(self.config.height * (window_percent / 100))

        # Center the window
        x1 = (self.config.width - window_w) // 2
        y1 = (self.config.height - window_h) // 2
        x2 = x1 + window_w
        y2 = y1 + window_h

        data[y1:y2, x1:x2] = [r, g, b]

        return TestPattern(
            pattern_type=PatternType.WINDOW,
            data=data,
            rgb_value=(r, g, b),
            name=f"Window {window_percent}% RGB({r},{g},{b})",
            metadata={"window_percent": window_percent}
        )

    def generate_gradient_horizontal(self) -> TestPattern:
        """
        Generate horizontal grayscale gradient.

        Returns:
            Horizontal gradient test pattern
        """
        data = np.zeros((self.config.height, self.config.width, 3), dtype=np.uint8)

        for x in range(self.config.width):
            level = int((x / (self.config.width - 1)) * 255)
            data[:, x] = [level, level, level]

        return TestPattern(
            pattern_type=PatternType.GRADIENT_H,
            data=data,
            name="Horizontal Gradient"
        )

    def generate_gradient_vertical(self) -> TestPattern:
        """
        Generate vertical grayscale gradient.

        Returns:
            Vertical gradient test pattern
        """
        data = np.zeros((self.config.height, self.config.width, 3), dtype=np.uint8)

        for y in range(self.config.height):
            level = int((y / (self.config.height - 1)) * 255)
            data[y, :] = [level, level, level]

        return TestPattern(
            pattern_type=PatternType.GRADIENT_V,
            data=data,
            name="Vertical Gradient"
        )

    def generate_uniformity_grid(self, rows: int = 5, cols: int = 5,
                                  level: int = 255) -> TestPattern:
        """
        Generate uniformity test grid pattern.

        Args:
            rows: Number of grid rows
            cols: Number of grid columns
            level: Gray level for patches

        Returns:
            Uniformity grid test pattern
        """
        data = np.zeros((self.config.height, self.config.width, 3), dtype=np.uint8)

        patch_w = self.config.width // cols
        patch_h = self.config.height // rows

        # Create grid of patches with borders
        for row in range(rows):
            for col in range(cols):
                x1 = col * patch_w + 2
                y1 = row * patch_h + 2
                x2 = (col + 1) * patch_w - 2
                y2 = (row + 1) * patch_h - 2
                data[y1:y2, x1:x2] = [level, level, level]

        return TestPattern(
            pattern_type=PatternType.UNIFORMITY_GRID,
            data=data,
            name=f"Uniformity Grid {rows}x{cols}",
            metadata={"rows": rows, "cols": cols, "level": level}
        )

    def generate_crosshatch(self, line_spacing: int = 100,
                            line_width: int = 1) -> TestPattern:
        """
        Generate crosshatch pattern for geometry testing.

        Args:
            line_spacing: Pixels between lines
            line_width: Line thickness in pixels

        Returns:
            Crosshatch test pattern
        """
        data = np.zeros((self.config.height, self.config.width, 3), dtype=np.uint8)

        # Draw vertical lines
        for x in range(0, self.config.width, line_spacing):
            x1 = max(0, x - line_width // 2)
            x2 = min(self.config.width, x + line_width // 2 + 1)
            data[:, x1:x2] = [255, 255, 255]

        # Draw horizontal lines
        for y in range(0, self.config.height, line_spacing):
            y1 = max(0, y - line_width // 2)
            y2 = min(self.config.height, y + line_width // 2 + 1)
            data[y1:y2, :] = [255, 255, 255]

        return TestPattern(
            pattern_type=PatternType.CROSSHATCH,
            data=data,
            name=f"Crosshatch {line_spacing}px",
            metadata={"line_spacing": line_spacing, "line_width": line_width}
        )

    def generate_colorchecker_patches(self) -> list[TestPattern]:
        """
        Generate ColorChecker Classic 24-patch patterns.

        Returns:
            List of 24 ColorChecker test patterns
        """
        # ColorChecker Classic sRGB values (approximate)
        colorchecker = [
            (115, 82, 68, "Dark Skin"),
            (194, 150, 130, "Light Skin"),
            (98, 122, 157, "Blue Sky"),
            (87, 108, 67, "Foliage"),
            (133, 128, 177, "Blue Flower"),
            (103, 189, 170, "Bluish Green"),
            (214, 126, 44, "Orange"),
            (80, 91, 166, "Purplish Blue"),
            (193, 90, 99, "Moderate Red"),
            (94, 60, 108, "Purple"),
            (157, 188, 64, "Yellow Green"),
            (224, 163, 46, "Orange Yellow"),
            (56, 61, 150, "Blue"),
            (70, 148, 73, "Green"),
            (175, 54, 60, "Red"),
            (231, 199, 31, "Yellow"),
            (187, 86, 149, "Magenta"),
            (8, 133, 161, "Cyan"),
            (243, 243, 242, "White"),
            (200, 200, 200, "Neutral 8"),
            (160, 160, 160, "Neutral 6.5"),
            (122, 122, 121, "Neutral 5"),
            (85, 85, 85, "Neutral 3.5"),
            (52, 52, 52, "Black"),
        ]

        patterns = []
        for r, g, b, name in colorchecker:
            pattern = self.generate_solid(r, g, b)
            pattern.name = name
            patterns.append(pattern)
        return patterns

    def generate_calibration_sequence(self,
                                       include_grayscale: bool = True,
                                       grayscale_steps: int = 21,
                                       include_primaries: bool = True,
                                       include_colorchecker: bool = False) -> Generator[TestPattern, None, None]:
        """
        Generate a complete calibration pattern sequence.

        Args:
            include_grayscale: Include grayscale ramp
            grayscale_steps: Number of grayscale steps
            include_primaries: Include RGB primaries
            include_colorchecker: Include ColorChecker patches

        Yields:
            Test patterns in sequence
        """
        if include_grayscale:
            for pattern in self.generate_grayscale_ramp(grayscale_steps):
                yield pattern

        if include_primaries:
            for pattern in self.generate_rgbcmy():
                yield pattern

        if include_colorchecker:
            for pattern in self.generate_colorchecker_patches():
                yield pattern


def create_pattern_generator(width: int = 1920, height: int = 1080,
                              bit_depth: int = 8) -> PatternGenerator:
    """
    Create a pattern generator with specified settings.

    Args:
        width: Display width in pixels
        height: Display height in pixels
        bit_depth: Color bit depth

    Returns:
        Configured PatternGenerator instance
    """
    config = PatternConfig(width=width, height=height, bit_depth=bit_depth)
    return PatternGenerator(config)
