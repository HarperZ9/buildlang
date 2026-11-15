"""
Measurement Coordinator

Bridges between patch display and colorimeter reading.
Provides a simple measure(r, g, b) -> (X, Y, Z) interface
that the hybrid calibration engine consumes.

Supports three measurement modes:
1. ArgyllCMS (spotread): automated, highest quality
2. Manual entry: user reads colorimeter software and types values
3. Simulated: for testing without hardware (returns panel-predicted values)
"""

import time
import tkinter as tk
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Optional, Tuple

import numpy as np


@dataclass
class MeasurementConfig:
    """Configuration for the measurement coordinator."""
    settle_time: float = 1.5      # Seconds to wait after displaying patch
    display_index: int = 0        # Which display to measure
    argyll_path: Optional[str] = None  # Path to ArgyllCMS bin directory
    device_index: int = 0         # Colorimeter device index
    mode: str = "argyll"          # "argyll", "manual", or "simulated"


class MeasurementCoordinator:
    """
    Coordinates patch display and colorimeter measurement.

    Creates fullscreen patches on the target display and reads
    the colorimeter to get measured XYZ values.
    """

    def __init__(self, config: Optional[MeasurementConfig] = None):
        self.config = config or MeasurementConfig()
        self._tk_root = None
        self._tk_canvas = None
        self._display_geometry = None
        self._argyll_backend = None

    def initialize(self) -> bool:
        """
        Initialize the measurement system.

        Returns True if ready to measure, False if setup failed.
        """
        if self.config.mode == "argyll":
            return self._init_argyll()
        elif self.config.mode == "manual":
            return True  # Manual mode always works
        elif self.config.mode == "simulated":
            return True
        return False

    def _init_argyll(self) -> bool:
        """Initialize ArgyllCMS backend."""
        try:
            from calibrate_pro.hardware.argyll_backend import (
                ArgyllBackend, ArgyllConfig
            )
            config = ArgyllConfig()
            if self.config.argyll_path:
                config.bin_path = Path(self.config.argyll_path)
            else:
                if not config.find_argyll():
                    self._init_error = "argyll_not_found"
                    return False

            self._argyll_backend = ArgyllBackend(config)
            self._argyll_path = config.bin_path

            # Detect and connect to colorimeter
            devices = self._argyll_backend.detect_devices()
            if not devices:
                self._init_error = "no_colorimeter"
                return False

            idx = min(self.config.device_index, len(devices) - 1)
            return self._argyll_backend.connect(idx)

        except Exception as e:
            self._init_error = f"error: {e}"
            return False

    def measure(self, r: float, g: float, b: float) -> Tuple[float, float, float]:
        """
        Display a patch and measure its XYZ values.

        This is the main entry point used by the hybrid calibration engine.

        Args:
            r, g, b: sRGB values in [0, 1]

        Returns:
            (X, Y, Z) measured tristimulus values

        Raises:
            RuntimeError if measurement fails
        """
        # Display the patch
        self._display_patch(r, g, b)

        # Wait for display to settle
        time.sleep(self.config.settle_time)

        # Read the colorimeter
        if self.config.mode == "argyll":
            return self._measure_argyll()
        elif self.config.mode == "manual":
            return self._measure_manual(r, g, b)
        elif self.config.mode == "simulated":
            return self._measure_simulated(r, g, b)
        else:
            raise RuntimeError(f"Unknown measurement mode: {self.config.mode}")

    def _display_patch(self, r: float, g: float, b: float):
        """Display a solid color patch fullscreen on the target display."""
        # Convert to 8-bit hex
        ri = max(0, min(255, int(round(r * 255))))
        gi = max(0, min(255, int(round(g * 255))))
        bi = max(0, min(255, int(round(b * 255))))
        color = f"#{ri:02x}{gi:02x}{bi:02x}"

        if self._tk_root is None:
            self._create_display_window()

        if self._tk_canvas is not None:
            self._tk_canvas.config(bg=color)
            self._tk_root.update()

    def _create_display_window(self):
        """Create fullscreen tkinter window on target display."""
        try:
            self._tk_root = tk.Tk()
            self._tk_root.withdraw()

            # Get display geometry
            geometry = self._get_display_geometry()
            if geometry:
                x, y, w, h = geometry
                self._tk_root.geometry(f"{w}x{h}+{x}+{y}")
            else:
                self._tk_root.attributes("-fullscreen", True)

            self._tk_root.overrideredirect(True)
            self._tk_root.attributes("-topmost", True)
            self._tk_root.deiconify()

            self._tk_canvas = tk.Canvas(
                self._tk_root, highlightthickness=0,
                cursor="none"
            )
            self._tk_canvas.pack(fill=tk.BOTH, expand=True)
            self._tk_root.update()

        except Exception:
            self._tk_root = None
            self._tk_canvas = None

    def _get_display_geometry(self) -> Optional[Tuple[int, int, int, int]]:
        """Get the geometry of the target display."""
        try:
            from calibrate_pro.panels.detection import enumerate_displays
            displays = enumerate_displays()
            if self.config.display_index < len(displays):
                d = displays[self.config.display_index]
                return (d.position_x, d.position_y, d.width, d.height)
        except Exception:
            pass
        return None

    def _measure_argyll(self) -> Tuple[float, float, float]:
        """Take a measurement using ArgyllCMS spotread."""
        if self._argyll_backend is None:
            raise RuntimeError("ArgyllCMS backend not initialized")

        result = self._argyll_backend.measure_spot()
        if result is None:
            raise RuntimeError("Measurement failed: no result from spotread")

        return (result.X, result.Y, result.Z)

    def _measure_manual(self, r: float, g: float, b: float) -> Tuple[float, float, float]:
        """Prompt user to enter measured XYZ values."""
        print(f"\n  Patch displayed: RGB({r:.3f}, {g:.3f}, {b:.3f})")
        print(f"  Read your colorimeter and enter measured XYZ values.")

        while True:
            try:
                line = input("  Enter X Y Z (space-separated): ").strip()
                parts = line.split()
                if len(parts) == 3:
                    x, y, z = float(parts[0]), float(parts[1]), float(parts[2])
                    return (x, y, z)
                print("  Please enter exactly 3 values.")
            except (ValueError, EOFError):
                print("  Invalid input. Enter three numbers separated by spaces.")

    def _measure_simulated(self, r: float, g: float, b: float) -> Tuple[float, float, float]:
        """
        Simulate a measurement for testing without hardware.

        Returns the expected XYZ with small random noise to simulate
        real-world measurement variation.
        """
        from calibrate_pro.core.color_math import srgb_gamma_expand, SRGB_TO_XYZ

        rgb_linear = srgb_gamma_expand(np.array([r, g, b]))
        xyz = SRGB_TO_XYZ @ rgb_linear

        # Add realistic measurement noise (dE ~0.5 equivalent)
        noise = np.random.normal(0, 0.002, 3)
        return tuple(xyz + noise)

    def close(self):
        """Clean up display window and colorimeter connection."""
        if self._tk_root is not None:
            try:
                self._tk_root.destroy()
            except Exception:
                pass
            self._tk_root = None
            self._tk_canvas = None

        if self._argyll_backend is not None:
            try:
                self._argyll_backend.disconnect()
            except Exception:
                pass

    def __enter__(self):
        self.initialize()
        return self

    def __exit__(self, *args):
        self.close()


def create_measure_fn(
    config: Optional[MeasurementConfig] = None
) -> Optional[Callable]:
    """
    Create a measure(r, g, b) -> (X, Y, Z) function from config.

    Returns None if no measurement hardware is available and
    mode is not manual/simulated.
    """
    coordinator = MeasurementCoordinator(config)
    if not coordinator.initialize():
        coordinator.close()
        return None

    def measure(r: float, g: float, b: float) -> Tuple[float, float, float]:
        return coordinator.measure(r, g, b)

    # Attach close method for cleanup
    measure.close = coordinator.close
    return measure
