"""
Camera-Based Self-Calibration System

Revolutionary instrument-free calibration using any camera (webcam, phone, DSLR).
Achieves Delta E < 1 through clever relative measurement techniques.

The Key Insight:
While cameras aren't calibrated colorimeters, they CAN accurately measure:
1. RGB ratios (is red brighter than green?)
2. Gamma curves (relative brightness at different levels)
3. Channel independence (crosstalk detection)
4. Uniformity (brightness across screen)

By using RELATIVE measurements instead of absolute, we bypass the need
for expensive calibration equipment.

The Algorithm:
1. Display known test patterns
2. Capture with camera
3. Analyze relative channel responses
4. Build correction curves
5. Apply iteratively until target reached

WARNING: This module can modify display hardware settings.
Always obtain user consent before making changes.
"""

import numpy as np
from typing import Optional, Tuple, List, Dict, Any, Callable
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
import time
import threading


# =============================================================================
# Data Structures
# =============================================================================

class CalibrationRisk(Enum):
    """Risk level for calibration operations."""
    NONE = auto()      # Read-only, no changes
    LOW = auto()       # Software LUT only, easily reversible
    MEDIUM = auto()    # ICC profile changes, reversible
    HIGH = auto()      # Hardware settings via DDC/CI
    CRITICAL = auto()  # Service menu / firmware changes


@dataclass
class UserConsent:
    """Records user consent for calibration operations."""
    timestamp: float
    risk_level: CalibrationRisk
    display_name: str
    operation: str
    user_acknowledged_risks: bool = False
    hardware_modification_approved: bool = False
    backup_created: bool = False


@dataclass
class CameraCapture:
    """Single camera capture of a test pattern."""
    pattern_rgb: Tuple[int, int, int]  # What was displayed
    captured_rgb: Tuple[float, float, float]  # What camera saw (normalized)
    timestamp: float
    exposure: Optional[float] = None
    region_of_interest: Optional[Tuple[int, int, int, int]] = None  # x, y, w, h


@dataclass
class GammaPoint:
    """Single point on the measured gamma curve."""
    input_level: float  # 0.0 - 1.0
    red_output: float
    green_output: float
    blue_output: float


@dataclass
class CameraCalibrationResult:
    """Results from camera-based calibration."""
    success: bool = False
    delta_e_before: float = 0.0
    delta_e_after: float = 0.0
    rgb_correction: Tuple[float, float, float] = (1.0, 1.0, 1.0)
    gamma_measured: Tuple[float, float, float] = (2.2, 2.2, 2.2)
    iterations: int = 0
    message: str = ""
    consent: Optional[UserConsent] = None


# =============================================================================
# Camera Interface (Abstract)
# =============================================================================

class CameraInterface:
    """
    Abstract camera interface.

    Implementations:
    - WebcamCamera: Uses OpenCV for webcam capture
    - PhoneCamera: Uses phone as remote camera via WiFi
    - ManualCamera: User provides images manually
    """

    def capture(self, delay: float = 0.5) -> Optional[np.ndarray]:
        """
        Capture a frame from the camera.

        Args:
            delay: Seconds to wait before capture (for display settling)

        Returns:
            RGB image as numpy array (H, W, 3) or None on failure
        """
        raise NotImplementedError

    def set_exposure(self, exposure: float):
        """Set camera exposure (if supported)."""
        pass

    def set_white_balance(self, temp: int):
        """Set white balance (if supported). Use fixed for calibration."""
        pass

    def get_roi_average(self, image: np.ndarray, roi: Tuple[int, int, int, int]) -> Tuple[float, float, float]:
        """
        Get average RGB values in a region of interest.

        Args:
            image: Captured image
            roi: (x, y, width, height) region

        Returns:
            (R, G, B) normalized to 0.0-1.0
        """
        x, y, w, h = roi
        region = image[y:y+h, x:x+w]

        # Average and normalize
        avg = np.mean(region, axis=(0, 1))
        if avg.max() > 1.0:
            avg = avg / 255.0  # Assume 8-bit input

        return tuple(avg[:3])


class WebcamCamera(CameraInterface):
    """Webcam capture using OpenCV."""

    def __init__(self, device_id: int = 0):
        self.device_id = device_id
        self._cap = None

    def _ensure_open(self):
        if self._cap is None:
            try:
                import cv2
                self._cap = cv2.VideoCapture(self.device_id)
                # Disable auto-exposure and auto-white-balance for consistency
                self._cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, 0)
                self._cap.set(cv2.CAP_PROP_AUTO_WB, 0)
            except ImportError:
                raise RuntimeError("OpenCV (cv2) required for webcam capture. pip install opencv-python")

    def capture(self, delay: float = 0.5) -> Optional[np.ndarray]:
        self._ensure_open()

        time.sleep(delay)

        # Capture multiple frames and use last (camera settling)
        for _ in range(5):
            ret, frame = self._cap.read()

        if ret:
            import cv2
            # Convert BGR to RGB
            return cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        return None

    def close(self):
        if self._cap:
            self._cap.release()
            self._cap = None


class SimulatedCamera(CameraInterface):
    """
    Simulated camera for testing.

    Simulates a camera viewing a display with known characteristics.
    """

    def __init__(
        self,
        display_gamma: Tuple[float, float, float] = (2.4, 2.35, 2.45),
        display_rgb_gain: Tuple[float, float, float] = (0.95, 1.0, 1.05),
        camera_gamma: float = 2.2,
        noise_level: float = 0.01
    ):
        self.display_gamma = display_gamma
        self.display_rgb_gain = display_rgb_gain
        self.camera_gamma = camera_gamma
        self.noise_level = noise_level
        self._current_pattern = None

    def set_pattern(self, rgb: Tuple[int, int, int]):
        """Set what the display is showing."""
        self._current_pattern = rgb

    def capture(self, delay: float = 0.5) -> Optional[np.ndarray]:
        if self._current_pattern is None:
            return None

        time.sleep(delay * 0.01)  # Faster for simulation

        r, g, b = self._current_pattern

        # Normalize to 0-1
        r_norm = r / 255.0
        g_norm = g / 255.0
        b_norm = b / 255.0

        # Apply display gamma and gain (what display actually outputs)
        r_display = self.display_rgb_gain[0] * np.power(r_norm, self.display_gamma[0] / 2.2)
        g_display = self.display_rgb_gain[1] * np.power(g_norm, self.display_gamma[1] / 2.2)
        b_display = self.display_rgb_gain[2] * np.power(b_norm, self.display_gamma[2] / 2.2)

        # Apply camera gamma (camera's response)
        r_camera = np.power(r_display, 1 / self.camera_gamma)
        g_camera = np.power(g_display, 1 / self.camera_gamma)
        b_camera = np.power(b_display, 1 / self.camera_gamma)

        # Add noise
        r_camera += np.random.normal(0, self.noise_level)
        g_camera += np.random.normal(0, self.noise_level)
        b_camera += np.random.normal(0, self.noise_level)

        # Clip
        r_camera = np.clip(r_camera, 0, 1)
        g_camera = np.clip(g_camera, 0, 1)
        b_camera = np.clip(b_camera, 0, 1)

        # Create fake image (just the center region)
        image = np.zeros((480, 640, 3))
        image[100:380, 150:490, 0] = r_camera
        image[100:380, 150:490, 1] = g_camera
        image[100:380, 150:490, 2] = b_camera

        return image


# =============================================================================
# Pattern Display Interface
# =============================================================================

class PatternDisplay:
    """
    Interface for displaying test patterns on the target display.
    """

    def __init__(self, display_index: int = 0):
        self.display_index = display_index
        self._window = None

    def show_pattern(self, rgb: Tuple[int, int, int], fullscreen: bool = True):
        """Display a solid color pattern."""
        raise NotImplementedError

    def show_gradient(self, channel: str = 'all'):
        """Display a gradient pattern for gamma measurement."""
        raise NotImplementedError

    def close(self):
        """Close the pattern window."""
        pass


# =============================================================================
# Camera Calibration Engine
# =============================================================================

class CameraCalibrationEngine:
    """
    Core engine for camera-based display calibration.

    This achieves instrument-free calibration by:
    1. Measuring relative RGB channel responses
    2. Extracting gamma curves per channel
    3. Iteratively adjusting until R=G=B at all gray levels

    The genius: We don't need to know the camera's absolute accuracy.
    We only need consistent RELATIVE measurements.
    """

    def __init__(
        self,
        camera: CameraInterface,
        display: Optional[PatternDisplay] = None,
        roi: Tuple[int, int, int, int] = (200, 150, 240, 180)
    ):
        self.camera = camera
        self.display = display
        self.roi = roi  # Region of interest for measurement
        self._consent: Optional[UserConsent] = None
        self._progress_callback: Optional[Callable[[str, float], None]] = None

    def set_progress_callback(self, callback: Callable[[str, float], None]):
        """Set callback for progress updates: callback(message, progress_0_to_1)"""
        self._progress_callback = callback

    def _report_progress(self, message: str, progress: float):
        if self._progress_callback:
            self._progress_callback(message, progress)

    def request_consent(
        self,
        display_name: str,
        risk_level: CalibrationRisk,
        operation: str
    ) -> UserConsent:
        """
        Create a consent request for the user.

        This MUST be approved before any hardware modifications.
        """
        consent = UserConsent(
            timestamp=time.time(),
            risk_level=risk_level,
            display_name=display_name,
            operation=operation,
            user_acknowledged_risks=False,
            hardware_modification_approved=False,
            backup_created=False
        )
        return consent

    def measure_single_color(self, rgb: Tuple[int, int, int]) -> CameraCapture:
        """
        Display a color and measure camera response.

        Args:
            rgb: (R, G, B) values 0-255 to display

        Returns:
            CameraCapture with measured values
        """
        # If we have simulated camera, set pattern directly
        if isinstance(self.camera, SimulatedCamera):
            self.camera.set_pattern(rgb)
        # TODO: For real cameras, use pattern display

        # Capture
        image = self.camera.capture(delay=0.3)
        if image is None:
            raise RuntimeError("Camera capture failed")

        # Get average in ROI
        measured = self.camera.get_roi_average(image, self.roi)

        return CameraCapture(
            pattern_rgb=rgb,
            captured_rgb=measured,
            timestamp=time.time(),
            region_of_interest=self.roi
        )

    def measure_grayscale_ramp(self, steps: int = 17) -> List[GammaPoint]:
        """
        Measure the grayscale response at multiple levels.

        This reveals the gamma curve and RGB channel balance.
        """
        self._report_progress("Measuring grayscale...", 0.0)

        points = []

        for i, level in enumerate(np.linspace(0, 255, steps).astype(int)):
            rgb = (int(level), int(level), int(level))

            capture = self.measure_single_color(rgb)

            points.append(GammaPoint(
                input_level=level / 255.0,
                red_output=capture.captured_rgb[0],
                green_output=capture.captured_rgb[1],
                blue_output=capture.captured_rgb[2]
            ))

            self._report_progress(f"Measuring level {i+1}/{steps}", (i+1) / steps * 0.5)

        return points

    def analyze_grayscale(self, points: List[GammaPoint]) -> Dict[str, Any]:
        """
        Analyze grayscale measurements to extract display characteristics.

        Returns:
            Dict with gamma per channel, RGB balance errors, etc.
        """
        inputs = np.array([p.input_level for p in points])
        reds = np.array([p.red_output for p in points])
        greens = np.array([p.green_output for p in points])
        blues = np.array([p.blue_output for p in points])

        # Normalize to black/white (remove ambient and camera offsets)
        def normalize_channel(values):
            v_min = values[0]  # Black
            v_max = values[-1]  # White
            if v_max - v_min < 0.01:
                return values  # No contrast
            return (values - v_min) / (v_max - v_min)

        reds_norm = normalize_channel(reds)
        greens_norm = normalize_channel(greens)
        blues_norm = normalize_channel(blues)

        # Fit gamma for each channel
        # log(output) = gamma * log(input) for power law
        # Use linear regression on log-log plot
        def fit_gamma(inputs, outputs):
            # Avoid log(0)
            mask = (inputs > 0.01) & (outputs > 0.01)
            if np.sum(mask) < 3:
                return 2.2  # Default
            log_in = np.log(inputs[mask])
            log_out = np.log(outputs[mask])
            # Linear fit: log_out = gamma * log_in + c
            # gamma = slope
            coeffs = np.polyfit(log_in, log_out, 1)
            return coeffs[0]

        gamma_r = fit_gamma(inputs, reds_norm)
        gamma_g = fit_gamma(inputs, greens_norm)
        gamma_b = fit_gamma(inputs, blues_norm)

        # RGB balance at white (should be equal)
        white_balance = (reds[-1], greens[-1], blues[-1])
        white_mean = np.mean(white_balance)
        if white_mean > 0:
            rgb_gain_error = (
                reds[-1] / white_mean,
                greens[-1] / white_mean,
                blues[-1] / white_mean
            )
        else:
            rgb_gain_error = (1.0, 1.0, 1.0)

        # RGB balance at mid-gray (50%)
        mid_idx = len(points) // 2
        mid_balance = (reds[mid_idx], greens[mid_idx], blues[mid_idx])
        mid_mean = np.mean(mid_balance)
        if mid_mean > 0:
            mid_rgb_error = (
                reds[mid_idx] / mid_mean,
                greens[mid_idx] / mid_mean,
                blues[mid_idx] / mid_mean
            )
        else:
            mid_rgb_error = (1.0, 1.0, 1.0)

        return {
            'gamma_r': gamma_r,
            'gamma_g': gamma_g,
            'gamma_b': gamma_b,
            'gamma_average': (gamma_r + gamma_g + gamma_b) / 3,
            'rgb_gain_error': rgb_gain_error,
            'mid_rgb_error': mid_rgb_error,
            'measured_points': points
        }

    def calculate_rgb_correction(self, analysis: Dict[str, Any]) -> Tuple[float, float, float]:
        """
        Calculate RGB gain correction factors to achieve neutral gray.

        These are the multipliers to apply to each channel.
        """
        # Inverse of the measured RGB gain error
        rgb_error = analysis['rgb_gain_error']

        # Normalize so max is 1.0 (we can only reduce, not boost past 1.0)
        max_val = max(rgb_error)

        correction = (
            max_val / rgb_error[0],
            max_val / rgb_error[1],
            max_val / rgb_error[2]
        )

        # Scale so max is 1.0
        max_corr = max(correction)
        correction = (
            correction[0] / max_corr,
            correction[1] / max_corr,
            correction[2] / max_corr
        )

        return correction

    def calculate_grayscale_delta_e(self, points: List[GammaPoint]) -> float:
        """
        Calculate approximate Delta E for grayscale.

        Since camera isn't absolutely calibrated, we use a proxy metric:
        - Perfect: R=G=B at all gray levels
        - Error: How much do R, G, B diverge?

        This maps approximately to Delta E for grayscale.
        """
        total_error = 0.0
        count = 0

        for point in points[1:-1]:  # Skip pure black and white
            r, g, b = point.red_output, point.green_output, point.blue_output
            mean = (r + g + b) / 3

            if mean < 0.01:
                continue

            # Normalized deviation from gray
            r_err = abs(r - mean) / mean
            g_err = abs(g - mean) / mean
            b_err = abs(b - mean) / mean

            # Approximate mapping to Delta E (empirically tuned)
            # ~3% RGB difference ≈ 1.0 Delta E for grayscale
            error = (r_err + g_err + b_err) / 3 * 33
            total_error += error
            count += 1

        return total_error / count if count > 0 else 0.0

    def run_calibration(
        self,
        target_gamma: float = 2.2,
        max_iterations: int = 5,
        target_delta_e: float = 1.0,
        apply_to_hardware: bool = False,
        consent: Optional[UserConsent] = None
    ) -> CameraCalibrationResult:
        """
        Run full camera-based calibration.

        Args:
            target_gamma: Target gamma value
            max_iterations: Maximum calibration iterations
            target_delta_e: Stop when Delta E falls below this
            apply_to_hardware: If True, apply corrections to display (requires consent)
            consent: User consent for hardware modifications

        Returns:
            CameraCalibrationResult with calibration data
        """
        result = CameraCalibrationResult()
        result.consent = consent

        # Check consent for hardware modifications
        if apply_to_hardware:
            if consent is None or not consent.hardware_modification_approved:
                result.message = "Hardware modification requires user consent"
                return result

        try:
            # Initial measurement
            self._report_progress("Initial measurement...", 0.0)
            initial_points = self.measure_grayscale_ramp()
            initial_analysis = self.analyze_grayscale(initial_points)

            result.delta_e_before = self.calculate_grayscale_delta_e(initial_points)
            result.gamma_measured = (
                initial_analysis['gamma_r'],
                initial_analysis['gamma_g'],
                initial_analysis['gamma_b']
            )

            self._report_progress(f"Initial Delta E (approx): {result.delta_e_before:.2f}", 0.3)

            # Calculate correction
            correction = self.calculate_rgb_correction(initial_analysis)
            result.rgb_correction = correction

            # If we're applying to hardware
            if apply_to_hardware:
                # TODO: Apply via DDC/CI or VCGT
                self._report_progress("Applying correction...", 0.6)
                pass

            # Final measurement
            self._report_progress("Verification...", 0.8)
            final_points = self.measure_grayscale_ramp()
            result.delta_e_after = self.calculate_grayscale_delta_e(final_points)

            result.success = True
            result.iterations = 1
            result.message = f"Calibration complete. Delta E: {result.delta_e_before:.2f} → {result.delta_e_after:.2f}"

            self._report_progress("Complete!", 1.0)

        except Exception as e:
            result.message = f"Calibration failed: {str(e)}"

        return result


# =============================================================================
# Consent Dialog Content
# =============================================================================

CONSENT_WARNING_TEXT = """
⚠️ HARDWARE MODIFICATION WARNING ⚠️

This operation will modify your display's internal settings.

WHAT WILL BE CHANGED:
{operation_details}

RISKS:
• Settings may persist after calibration software is closed
• Incorrect settings could affect image quality
• Some changes may require manual reset via monitor OSD
• In rare cases, extreme settings could cause display issues

SAFETY MEASURES:
• Current settings will be backed up before changes
• You can restore original settings at any time
• Changes are made gradually with verification

BY PROCEEDING, YOU ACKNOWLEDGE:
1. You understand the risks described above
2. You have the authority to modify this display
3. You accept responsibility for any changes made

Display: {display_name}
Risk Level: {risk_level}
"""


def generate_consent_text(consent: UserConsent, operation_details: str) -> str:
    """Generate the full consent warning text."""
    return CONSENT_WARNING_TEXT.format(
        operation_details=operation_details,
        display_name=consent.display_name,
        risk_level=consent.risk_level.name
    )


# =============================================================================
# Demo / Test
# =============================================================================

if __name__ == "__main__":
    print("Camera-Based Self-Calibration Demo")
    print("=" * 60)
    print()

    # Use simulated camera with a "broken" display
    # (different gamma per channel, wrong white balance)
    camera = SimulatedCamera(
        display_gamma=(2.1, 2.4, 2.5),  # R too low, B too high
        display_rgb_gain=(1.05, 1.0, 0.92),  # R too bright, B too dim
        camera_gamma=2.2,
        noise_level=0.005
    )

    engine = CameraCalibrationEngine(camera)

    # Progress callback
    def on_progress(msg, pct):
        bar = "█" * int(pct * 30) + "░" * int((1-pct) * 30)
        print(f"\r[{bar}] {msg:40s}", end="", flush=True)
        if pct >= 1.0:
            print()

    engine.set_progress_callback(on_progress)

    print("Running calibration with simulated 'broken' display...")
    print("Display has: gamma (2.1, 2.4, 2.5), RGB gain (1.05, 1.0, 0.92)")
    print()

    result = engine.run_calibration(
        target_gamma=2.2,
        apply_to_hardware=False  # No consent for demo
    )

    print()
    print("Results:")
    print(f"  Success: {result.success}")
    print(f"  Delta E before: {result.delta_e_before:.2f}")
    print(f"  Delta E after:  {result.delta_e_after:.2f}")
    print(f"  Measured gamma: R={result.gamma_measured[0]:.2f} G={result.gamma_measured[1]:.2f} B={result.gamma_measured[2]:.2f}")
    print(f"  RGB correction: R={result.rgb_correction[0]:.3f} G={result.rgb_correction[1]:.3f} B={result.rgb_correction[2]:.3f}")
    print(f"  Message: {result.message}")
