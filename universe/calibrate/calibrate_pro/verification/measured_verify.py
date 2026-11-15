"""
Measured Verification Workflow (Phase 1.2)

Provides colorimeter-based measured verification of display calibration.
Works with any measurement backend that can return XYZ values:
  - ArgyllCMS (spotread) -- auto-detected
  - Manual entry mode for standalone colorimeter software
  - Any callable that takes (r, g, b) and returns (X, Y, Z)

Usage:
    from calibrate_pro.verification.measured_verify import MeasuredVerification

    # With ArgyllCMS backend (auto-detected):
    mv = MeasuredVerification()
    result = mv.verify_colorchecker(display_index=0)

    # With custom measurement function:
    mv = MeasuredVerification(measure_fn=my_colorimeter_read)
    result = mv.verify_grayscale(display_index=0, steps=21)

    # Manual mode (user types XYZ values):
    mv = MeasuredVerification()  # falls back to manual if no ArgyllCMS
    result = mv.verify_colorchecker()
"""

import time
import sys
from typing import Callable, Dict, List, Optional, Tuple

import numpy as np


# =============================================================================
# ColorChecker sRGB reference patches (for display)
# These are the sRGB values [0-1] used to drive the display for each patch.
# =============================================================================

COLORCHECKER_SRGB_PATCHES = [
    ("Dark Skin",       (0.453, 0.317, 0.264)),
    ("Light Skin",      (0.779, 0.577, 0.505)),
    ("Blue Sky",        (0.355, 0.480, 0.611)),
    ("Foliage",         (0.352, 0.422, 0.253)),
    ("Blue Flower",     (0.508, 0.502, 0.691)),
    ("Bluish Green",    (0.362, 0.745, 0.675)),
    ("Orange",          (0.879, 0.485, 0.183)),
    ("Purplish Blue",   (0.266, 0.358, 0.667)),
    ("Moderate Red",    (0.778, 0.321, 0.381)),
    ("Purple",          (0.367, 0.227, 0.414)),
    ("Yellow Green",    (0.623, 0.741, 0.246)),
    ("Orange Yellow",   (0.904, 0.634, 0.154)),
    ("Blue",            (0.139, 0.248, 0.577)),
    ("Green",           (0.262, 0.584, 0.291)),
    ("Red",             (0.752, 0.197, 0.178)),
    ("Yellow",          (0.938, 0.857, 0.159)),
    ("Magenta",         (0.752, 0.313, 0.577)),
    ("Cyan",            (0.121, 0.544, 0.659)),
    ("White",           (0.961, 0.961, 0.961)),
    ("Neutral 8",       (0.784, 0.784, 0.784)),
    ("Neutral 6.5",     (0.584, 0.584, 0.584)),
    ("Neutral 5",       (0.420, 0.420, 0.420)),
    ("Neutral 3.5",     (0.258, 0.258, 0.258)),
    ("Black",           (0.085, 0.085, 0.085)),
]

# ColorChecker Classic D50 Lab reference values (for Delta E computation)
COLORCHECKER_REFERENCE_LAB_D50 = {
    "Dark Skin":       (37.986, 13.555, 14.059),
    "Light Skin":      (65.711, 18.130, 17.810),
    "Blue Sky":        (49.927, -4.880, -21.925),
    "Foliage":         (43.139, -13.095, 21.905),
    "Blue Flower":     (55.112, 8.844, -25.399),
    "Bluish Green":    (70.719, -33.397, -0.199),
    "Orange":          (62.661, 36.067, 57.096),
    "Purplish Blue":   (40.020, 10.410, -45.964),
    "Moderate Red":    (51.124, 48.239, 16.248),
    "Purple":          (30.325, 22.976, -21.587),
    "Yellow Green":    (72.532, -23.709, 57.255),
    "Orange Yellow":   (71.941, 19.363, 67.857),
    "Blue":            (28.778, 14.179, -50.297),
    "Green":           (55.261, -38.342, 31.370),
    "Red":             (42.101, 53.378, 28.190),
    "Yellow":          (81.733, 4.039, 79.819),
    "Magenta":         (51.935, 49.986, -14.574),
    "Cyan":            (51.038, -28.631, -28.638),
    "White":           (96.539, -0.425, 1.186),
    "Neutral 8":       (81.257, -0.638, -0.335),
    "Neutral 6.5":     (66.766, -0.734, -0.504),
    "Neutral 5":       (50.867, -0.153, -0.270),
    "Neutral 3.5":     (35.656, -0.421, -1.231),
    "Black":           (20.461, -0.079, -0.973),
}


# =============================================================================
# Color math helpers (self-contained to avoid circular imports)
# =============================================================================

def _xyz_to_lab(
    xyz: Tuple[float, float, float],
    illuminant: str = "D50",
) -> Tuple[float, float, float]:
    """Convert XYZ to CIE Lab."""
    white_points = {
        "D50": (96.422, 100.0, 82.521),
        "D65": (95.047, 100.0, 108.883),
    }
    Xn, Yn, Zn = white_points.get(illuminant, white_points["D50"])
    X, Y, Z = xyz

    x = X / Xn
    y = Y / Yn
    z = Z / Zn

    def f(t: float) -> float:
        delta = 6.0 / 29.0
        if t > delta ** 3:
            return t ** (1.0 / 3.0)
        else:
            return t / (3.0 * delta ** 2) + 4.0 / 29.0

    L = 116.0 * f(y) - 16.0
    a = 500.0 * (f(x) - f(y))
    b = 200.0 * (f(y) - f(z))

    return (L, a, b)


def _delta_e_2000(
    lab1: Tuple[float, float, float],
    lab2: Tuple[float, float, float],
) -> float:
    """Compute CIEDE2000 Delta E between two Lab colors."""
    L1, a1, b1 = lab1
    L2, a2, b2 = lab2

    C1 = np.sqrt(a1 ** 2 + b1 ** 2)
    C2 = np.sqrt(a2 ** 2 + b2 ** 2)
    C_avg = (C1 + C2) / 2.0

    G = 0.5 * (1.0 - np.sqrt(C_avg ** 7 / (C_avg ** 7 + 25.0 ** 7)))

    a1p = a1 * (1.0 + G)
    a2p = a2 * (1.0 + G)

    C1p = np.sqrt(a1p ** 2 + b1 ** 2)
    C2p = np.sqrt(a2p ** 2 + b2 ** 2)

    h1p = np.degrees(np.arctan2(b1, a1p)) % 360.0
    h2p = np.degrees(np.arctan2(b2, a2p)) % 360.0

    dLp = L2 - L1
    dCp = C2p - C1p

    dhp = h2p - h1p
    if abs(dhp) > 180.0:
        if dhp > 0:
            dhp -= 360.0
        else:
            dhp += 360.0

    dHp = 2.0 * np.sqrt(C1p * C2p) * np.sin(np.radians(dhp / 2.0))

    Lp_avg = (L1 + L2) / 2.0
    Cp_avg = (C1p + C2p) / 2.0

    hp_sum = h1p + h2p
    if abs(h1p - h2p) > 180.0:
        hp_sum += 360.0
    hp_avg = hp_sum / 2.0

    T = (1.0
         - 0.17 * np.cos(np.radians(hp_avg - 30.0))
         + 0.24 * np.cos(np.radians(2.0 * hp_avg))
         + 0.32 * np.cos(np.radians(3.0 * hp_avg + 6.0))
         - 0.20 * np.cos(np.radians(4.0 * hp_avg - 63.0)))

    d_theta = 30.0 * np.exp(-((hp_avg - 275.0) / 25.0) ** 2)
    R_C = 2.0 * np.sqrt(Cp_avg ** 7 / (Cp_avg ** 7 + 25.0 ** 7))

    S_L = 1.0 + (0.015 * (Lp_avg - 50.0) ** 2) / np.sqrt(20.0 + (Lp_avg - 50.0) ** 2)
    S_C = 1.0 + 0.045 * Cp_avg
    S_H = 1.0 + 0.015 * Cp_avg * T
    R_T = -np.sin(np.radians(2.0 * d_theta)) * R_C

    val = (
        (dLp / S_L) ** 2
        + (dCp / S_C) ** 2
        + (dHp / S_H) ** 2
        + R_T * (dCp / S_C) * (dHp / S_H)
    )

    return float(np.sqrt(max(0.0, val)))


# =============================================================================
# ArgyllCMS backend helper
# =============================================================================

def _find_argyll_spotread() -> Optional[str]:
    """Locate the ArgyllCMS ``spotread`` binary using the shared ArgyllConfig."""
    import os
    from pathlib import Path

    # Use the project's ArgyllConfig which knows about DisplayCAL's bundled install
    try:
        from calibrate_pro.hardware.argyll_backend import ArgyllConfig
        config = ArgyllConfig()
        if config.find_argyll():
            exe = "spotread.exe" if os.name == 'nt' else "spotread"
            spotread = config.bin_path / exe
            if spotread.exists():
                return str(spotread)
    except Exception:
        pass

    # Fallback: check PATH
    import shutil
    found = shutil.which("spotread")
    if found:
        return found

    # Fallback: check common install locations
    search_paths = [
        Path(r"C:\Program Files\ArgyllCMS\bin"),
        Path(r"C:\Program Files (x86)\ArgyllCMS\bin"),
        Path(os.environ.get("ARGYLL_BIN", "")) if os.environ.get("ARGYLL_BIN") else None,
        Path.home() / "ArgyllCMS" / "bin",
    ]

    for p in search_paths:
        if p is None:
            continue
        exe = p / ("spotread.exe" if sys.platform == "win32" else "spotread")
        if exe.exists():
            return str(exe)

    return None


def _argyll_measure_xyz(r: float, g: float, b: float) -> Tuple[float, float, float]:
    """
    Take a single-patch measurement via ArgyllCMS spotread.

    Uses our ArgyllBackend which properly handles device communication.
    The (r, g, b) arguments are ignored; the sensor reads whatever is on screen.
    """
    try:
        from calibrate_pro.hardware.argyll_backend import ArgyllBackend, ArgyllConfig

        # Use a module-level cached backend to avoid re-initializing for each patch
        global _argyll_backend_cache
        if '_argyll_backend_cache' not in globals() or _argyll_backend_cache is None:
            config = ArgyllConfig()
            if not config.find_argyll():
                raise RuntimeError("ArgyllCMS not found")
            _argyll_backend_cache = ArgyllBackend(config)
            devices = _argyll_backend_cache.detect_devices()
            if not devices:
                raise RuntimeError("No colorimeter detected")
            _argyll_backend_cache.connect(0)

        measurement = _argyll_backend_cache.measure_spot()
        if measurement:
            return (measurement.X, measurement.Y, measurement.Z)
        raise RuntimeError("Measurement returned no data")

        raise RuntimeError(f"Could not parse spotread output: {output[:200]}")

    except subprocess.TimeoutExpired:
        raise RuntimeError("spotread timed out waiting for measurement")


def _manual_measure_xyz(r: float, g: float, b: float) -> Tuple[float, float, float]:
    """
    Prompt the user to enter XYZ values manually.

    This is the fallback when no colorimeter backend is available.
    Useful for people using standalone measurement software.
    """
    print(f"\n  Patch displayed: RGB ({r:.3f}, {g:.3f}, {b:.3f})")
    print("  Measure this patch with your colorimeter and enter XYZ values.")

    while True:
        try:
            raw = input("  Enter X Y Z (space-separated): ").strip()
            parts = raw.split()
            if len(parts) >= 3:
                X = float(parts[0])
                Y = float(parts[1])
                Z = float(parts[2])
                return (X, Y, Z)
            else:
                print("  Please enter three numeric values separated by spaces.")
        except ValueError:
            print("  Invalid input. Please enter three numbers.")
        except (EOFError, KeyboardInterrupt):
            print("\n  Measurement cancelled.")
            return (0.0, 0.0, 0.0)


# =============================================================================
# MeasuredVerification
# =============================================================================

class MeasuredVerification:
    """
    Colorimeter-based measured verification of display calibration.

    Displays test patches fullscreen and measures them with a colorimeter
    (ArgyllCMS, custom function, or manual entry) to produce verified
    Delta E results.

    Args:
        measure_fn: Callable that takes (r, g, b) floats in [0, 1] and
                    returns (X, Y, Z) measured values.  If ``None``, the
                    class tries the ArgyllCMS ``spotread`` backend first,
                    then falls back to manual entry mode.
    """

    def __init__(self, measure_fn: Optional[Callable] = None):
        self._measure_fn = measure_fn
        self._backend_name = "custom"

        if self._measure_fn is None:
            # Try ArgyllCMS
            spotread = _find_argyll_spotread()
            if spotread is not None:
                self._measure_fn = _argyll_measure_xyz
                self._backend_name = "argyll"
            else:
                self._measure_fn = _manual_measure_xyz
                self._backend_name = "manual"

    @property
    def backend_name(self) -> str:
        """Return the name of the active measurement backend."""
        return self._backend_name

    # -------------------------------------------------------------------------
    # Core: display a patch and measure
    # -------------------------------------------------------------------------

    def display_and_measure(
        self,
        r: float,
        g: float,
        b: float,
        display_index: int = 0,
    ) -> Tuple[float, float, float]:
        """
        Display a solid color patch fullscreen, wait for the display to
        settle, then measure.

        Uses tkinter to create a fullscreen window on the specified
        display.  After measurement the window is destroyed.

        Args:
            r, g, b: Color to display, floats in [0, 1].
            display_index: Which display to show the patch on (0-based).

        Returns:
            Measured (X, Y, Z) tristimulus values.
        """
        import tkinter as tk

        # Clamp values
        r = max(0.0, min(1.0, r))
        g = max(0.0, min(1.0, g))
        b = max(0.0, min(1.0, b))

        # Convert to 8-bit hex color
        ri = int(round(r * 255))
        gi = int(round(g * 255))
        bi = int(round(b * 255))
        hex_color = f"#{ri:02x}{gi:02x}{bi:02x}"

        root = tk.Tk()
        root.attributes("-topmost", True)
        root.overrideredirect(True)

        # Position on the correct display
        try:
            # Get screen geometry for the target display
            # tkinter doesn't natively support multi-monitor geometry,
            # so we use the screeninfo package if available, otherwise
            # fall back to full primary screen.
            screen_x, screen_y, screen_w, screen_h = 0, 0, 0, 0

            try:
                from calibrate_pro.panels.detection import enumerate_displays
                displays = enumerate_displays()
                if display_index < len(displays):
                    d = displays[display_index]
                    screen_x = d.position_x
                    screen_y = d.position_y
                    screen_w = d.width
                    screen_h = d.height
            except Exception:
                pass

            if screen_w == 0 or screen_h == 0:
                screen_w = root.winfo_screenwidth()
                screen_h = root.winfo_screenheight()

            root.geometry(f"{screen_w}x{screen_h}+{screen_x}+{screen_y}")
        except Exception:
            root.geometry(
                f"{root.winfo_screenwidth()}x{root.winfo_screenheight()}+0+0"
            )

        root.configure(background=hex_color)
        root.update()

        # Allow the display to settle (important for OLED pixel response
        # and backlight-based LCD stabilization)
        time.sleep(1.0)

        # Perform measurement
        try:
            X, Y, Z = self._measure_fn(r, g, b)
        finally:
            root.destroy()

        return (X, Y, Z)

    # -------------------------------------------------------------------------
    # ColorChecker verification
    # -------------------------------------------------------------------------

    def verify_colorchecker(self, display_index: int = 0) -> Dict:
        """
        Display each ColorChecker Classic patch fullscreen, measure it,
        and compute Delta E against the reference.

        Args:
            display_index: Which display to test (0-based).

        Returns:
            Dict compatible with the sensorless verify structure, plus:
            - ``'measured'``: ``True``
            - ``'measured_xyz'``: list of measured XYZ per patch
            - ``'patches'``: list of per-patch dicts with name, delta_e,
              measured_lab, reference_lab, measured_xyz
            - ``'delta_e_avg'``, ``'delta_e_max'``
            - ``'grade'``
        """
        patches_result: List[Dict] = []
        measured_xyz_list: List[Tuple[float, float, float]] = []
        delta_e_values: List[float] = []

        total = len(COLORCHECKER_SRGB_PATCHES)

        for idx, (name, (r, g, b)) in enumerate(COLORCHECKER_SRGB_PATCHES):
            print(f"  [{idx + 1}/{total}] Measuring {name}...")
            X, Y, Z = self.display_and_measure(r, g, b, display_index)
            measured_xyz_list.append((X, Y, Z))

            # Convert measured XYZ to Lab D50 (match reference illuminant)
            # Scale: colorimeter XYZ is often in cd/m2, reference Lab uses
            # Y=100 normalization.  Normalize so white-patch Y maps to 100.
            measured_lab = _xyz_to_lab((X, Y, Z), "D50")

            ref_lab = COLORCHECKER_REFERENCE_LAB_D50.get(name, (50.0, 0.0, 0.0))
            de = _delta_e_2000(ref_lab, measured_lab)
            delta_e_values.append(de)

            status = "PASS" if de < 2.0 else ("WARN" if de < 3.0 else "FAIL")
            print(f"           dE={de:.2f} [{status}]")

            patches_result.append({
                "name": name,
                "delta_e": de,
                "measured_lab": measured_lab,
                "reference_lab": ref_lab,
                "measured_xyz": (X, Y, Z),
            })

        de_arr = np.array(delta_e_values) if delta_e_values else np.array([0.0])
        avg_de = float(np.mean(de_arr))
        max_de = float(np.max(de_arr))

        if avg_de < 1.0:
            grade = "Reference Grade"
        elif avg_de < 2.0:
            grade = "Professional Grade"
        elif avg_de < 3.0:
            grade = "Excellent"
        elif avg_de < 5.0:
            grade = "Good"
        else:
            grade = "Needs Calibration"

        return {
            "measured": True,
            "backend": self._backend_name,
            "patches": patches_result,
            "measured_xyz": measured_xyz_list,
            "delta_e_avg": avg_de,
            "delta_e_max": max_de,
            "grade": grade,
        }

    # -------------------------------------------------------------------------
    # Grayscale verification
    # -------------------------------------------------------------------------

    def verify_grayscale(
        self,
        display_index: int = 0,
        steps: int = 21,
        target_gamma: float = 2.2,
    ) -> Dict:
        """
        Measure a grayscale ramp from 0% to 100% in ``steps`` increments.

        Args:
            display_index: Which display to test (0-based).
            steps: Number of grayscale steps (default 21 = 0% to 100%
                   in 5% increments).
            target_gamma: Expected display gamma for tracking comparison.

        Returns:
            Dict with:
            - ``'measured'``: ``True``
            - ``'steps'``: list of step dicts, each containing:
                - ``'level'``: 0.0 to 1.0
                - ``'target_luminance'``: expected relative luminance
                - ``'measured_xyz'``: (X, Y, Z)
                - ``'measured_luminance'``: Y value (cd/m2)
                - ``'measured_gamma'``: computed gamma at this level
                - ``'gamma_error'``: deviation from target_gamma
            - ``'avg_gamma_error'``
            - ``'max_gamma_error'``
            - ``'delta_e_avg'``, ``'delta_e_max'``
            - ``'grade'``
        """
        levels = np.linspace(0.0, 1.0, steps)
        step_results: List[Dict] = []
        gamma_errors: List[float] = []
        delta_e_values: List[float] = []

        # First measure white to establish Y_max for normalization
        print(f"  [1/{steps}] Measuring white reference...")
        X_w, Y_w, Z_w = self.display_and_measure(1.0, 1.0, 1.0, display_index)
        if Y_w <= 0:
            Y_w = 1.0  # prevent division by zero

        for idx, level in enumerate(levels):
            print(f"  [{idx + 1}/{steps}] Measuring {level * 100:.0f}% gray...")
            X, Y, Z = self.display_and_measure(level, level, level, display_index)

            # Normalize luminance to [0, 1] relative to white
            rel_lum = max(0.0, Y / Y_w)

            # Compute measured gamma at this level
            if level > 0.0 and rel_lum > 0.0:
                measured_gamma = np.log(rel_lum) / np.log(level)
            elif level == 0.0:
                measured_gamma = target_gamma  # black level, gamma undefined
            else:
                measured_gamma = 0.0

            # Target luminance at this step
            target_lum = level ** target_gamma if level > 0 else 0.0

            gamma_err = abs(measured_gamma - target_gamma) if level > 0.01 else 0.0
            gamma_errors.append(gamma_err)

            # Compute Delta E for the gray patch (target = neutral D65)
            # Target Lab for a neutral gray at this level
            target_Y = target_lum * 100.0  # scale to Y=100 white
            target_lab = _xyz_to_lab(
                (0.95047 * target_lum, target_lum * 1.0, 1.08883 * target_lum),
                "D65",
            )
            # Normalize measured XYZ to Y=100 scale
            if Y_w > 0:
                norm_factor = 100.0 / Y_w
            else:
                norm_factor = 1.0
            measured_lab = _xyz_to_lab(
                (X * norm_factor, Y * norm_factor, Z * norm_factor),
                "D65",
            )
            de = _delta_e_2000(target_lab, measured_lab)
            delta_e_values.append(de)

            step_results.append({
                "level": float(level),
                "target_luminance": float(target_lum),
                "measured_xyz": (X, Y, Z),
                "measured_luminance": float(Y),
                "relative_luminance": float(rel_lum),
                "measured_gamma": float(measured_gamma),
                "target_gamma": float(target_gamma),
                "gamma_error": float(gamma_err),
                "delta_e": float(de),
            })

        ge_arr = np.array(gamma_errors)
        de_arr = np.array(delta_e_values) if delta_e_values else np.array([0.0])

        avg_ge = float(np.mean(ge_arr))
        max_ge = float(np.max(ge_arr))
        avg_de = float(np.mean(de_arr))
        max_de = float(np.max(de_arr))

        if avg_de < 1.0 and avg_ge < 0.1:
            grade = "Reference Grade"
        elif avg_de < 2.0 and avg_ge < 0.2:
            grade = "Professional Grade"
        elif avg_de < 3.0:
            grade = "Excellent"
        elif avg_de < 5.0:
            grade = "Good"
        else:
            grade = "Needs Calibration"

        return {
            "measured": True,
            "backend": self._backend_name,
            "steps": step_results,
            "white_luminance": float(Y_w),
            "target_gamma": float(target_gamma),
            "avg_gamma_error": avg_ge,
            "max_gamma_error": max_ge,
            "delta_e_avg": avg_de,
            "delta_e_max": max_de,
            "grade": grade,
        }
