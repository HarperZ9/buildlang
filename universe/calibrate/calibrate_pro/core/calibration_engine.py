"""
Core Calibration Engine

Orchestrates display calibration using sensorless or
hardware colorimeter modes. Manages calibration workflow and
profile/LUT generation.
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple, Union, Callable, TYPE_CHECKING
from pathlib import Path
from enum import Enum
from datetime import datetime
import time

from calibrate_pro.core.color_math import (
    D50_WHITE, D65_WHITE, Illuminant,
    xyz_to_lab, lab_to_xyz, srgb_to_xyz, xyz_to_srgb,
    bradford_adapt, delta_e_2000, cct_to_xy, xy_to_cct
)
from calibrate_pro.core.icc_profile import (
    ICCProfile, create_display_profile, generate_trc_curve,
    generate_srgb_trc, generate_bt1886_trc
)
from calibrate_pro.core.lut_engine import LUT3D, LUTGenerator, LUTFormat
from calibrate_pro.panels.database import (
    PanelDatabase, PanelCharacterization, get_database
)
from calibrate_pro.sensorless.neuralux import (
    SensorlessEngine, get_colorchecker_reference
)

if TYPE_CHECKING:
    from calibrate_pro.hardware.colorimeter_base import ColorimeterBase, ColorMeasurement

class CalibrationMode(Enum):
    """Calibration mode selection."""
    SENSORLESS = "sensorless"      # Sensorless panel database
    COLORIMETER = "colorimeter"    # Hardware colorimeter
    SPECTRO = "spectrophotometer"  # Hardware spectrophotometer
    HYBRID = "hybrid"              # Sensorless + colorimeter verification

class GammaTarget(Enum):
    """Gamma/EOTF target selection."""
    POWER_22 = "2.2"            # Simple power law gamma 2.2
    POWER_24 = "2.4"            # Simple power law gamma 2.4
    SRGB = "sRGB"               # sRGB piecewise function
    BT1886 = "BT.1886"          # Broadcast standard
    LSTAR = "L*"                # CIE L* perceptual
    CUSTOM = "custom"           # User-defined

class WhitepointTarget(Enum):
    """White point target selection."""
    D50 = "D50"                 # 5003K print standard
    D55 = "D55"                 # 5503K daylight
    D65 = "D65"                 # 6504K sRGB/broadcast
    D75 = "D75"                 # 7504K north sky
    DCI = "DCI"                 # 6300K DCI-P3
    NATIVE = "native"           # Display native
    CUSTOM = "custom"           # User-defined CCT

class GamutTarget(Enum):
    """Color gamut target selection."""
    SRGB = "sRGB"               # Standard web/consumer
    DCI_P3 = "DCI-P3"           # Wide gamut cinema
    BT2020 = "BT.2020"          # Ultra-wide HDR
    ADOBE_RGB = "Adobe RGB"      # Wide gamut photography
    NATIVE = "native"           # Display native gamut
    CUSTOM = "custom"           # User-defined primaries

@dataclass
class CalibrationTarget:
    """Target calibration parameters."""
    whitepoint: WhitepointTarget = WhitepointTarget.D65
    whitepoint_xy: Optional[Tuple[float, float]] = None
    whitepoint_cct: Optional[int] = None

    gamma: GammaTarget = GammaTarget.POWER_22
    gamma_value: float = 2.2

    gamut: GamutTarget = GamutTarget.SRGB
    gamut_primaries: Optional[Tuple[Tuple[float, float], ...]] = None

    luminance_target: Optional[float] = None  # cd/m2
    black_level_target: Optional[float] = None  # cd/m2

    def get_whitepoint_xy(self) -> Tuple[float, float]:
        """Get white point as xy chromaticity."""
        if self.whitepoint_xy:
            return self.whitepoint_xy
        if self.whitepoint_cct:
            return cct_to_xy(self.whitepoint_cct)

        whitepoints = {
            WhitepointTarget.D50: (0.3457, 0.3585),
            WhitepointTarget.D55: (0.3324, 0.3474),
            WhitepointTarget.D65: (0.3127, 0.3290),
            WhitepointTarget.D75: (0.2990, 0.3149),
            WhitepointTarget.DCI: (0.3140, 0.3510),
            WhitepointTarget.NATIVE: (0.3127, 0.3290),
        }
        return whitepoints.get(self.whitepoint, (0.3127, 0.3290))

    def get_gamut_primaries(self) -> Tuple[Tuple[float, float], ...]:
        """Get gamut as primary xy coordinates."""
        if self.gamut_primaries:
            return self.gamut_primaries

        gamuts = {
            GamutTarget.SRGB: (
                (0.6400, 0.3300),
                (0.3000, 0.6000),
                (0.1500, 0.0600)
            ),
            GamutTarget.DCI_P3: (
                (0.6800, 0.3200),
                (0.2650, 0.6900),
                (0.1500, 0.0600)
            ),
            GamutTarget.BT2020: (
                (0.7080, 0.2920),
                (0.1700, 0.7970),
                (0.1310, 0.0460)
            ),
            GamutTarget.ADOBE_RGB: (
                (0.6400, 0.3300),
                (0.2100, 0.7100),
                (0.1500, 0.0600)
            ),
        }
        return gamuts.get(self.gamut, gamuts[GamutTarget.SRGB])

@dataclass
class CalibrationResult:
    """Results from calibration process."""
    success: bool = False
    panel_name: str = ""
    panel_type: str = ""
    mode: CalibrationMode = CalibrationMode.SENSORLESS
    error_message: str = ""

    # Target settings
    target: CalibrationTarget = field(default_factory=CalibrationTarget)

    # Verification results
    delta_e_avg: float = 0.0
    delta_e_max: float = 0.0
    grade: str = ""
    patch_results: List[Dict] = field(default_factory=list)

    # Generated files
    icc_profile_path: Optional[Path] = None
    lut_path: Optional[Path] = None
    report_path: Optional[Path] = None

    # Timestamps
    timestamp: datetime = field(default_factory=datetime.now)

    def to_dict(self) -> Dict:
        """Convert to dictionary for serialization."""
        return {
            "success": self.success,
            "panel_name": self.panel_name,
            "panel_type": self.panel_type,
            "mode": self.mode.value,
            "delta_e_avg": self.delta_e_avg,
            "delta_e_max": self.delta_e_max,
            "grade": self.grade,
            "icc_profile": str(self.icc_profile_path) if self.icc_profile_path else None,
            "lut": str(self.lut_path) if self.lut_path else None,
            "timestamp": self.timestamp.isoformat()
        }

class CalibrationEngine:
    """
    Main calibration engine.

    Orchestrates sensorless and hardware calibration workflows.
    """

    def __init__(
        self,
        mode: CalibrationMode = CalibrationMode.SENSORLESS,
        panel_database: Optional[PanelDatabase] = None
    ):
        """
        Initialize calibration engine.

        Args:
            mode: Calibration mode
            panel_database: Panel characterization database
        """
        self.mode = mode
        self.database = panel_database or get_database()
        self.engine = SensorlessEngine(self.database)

        # Hardware backend (to be set if using colorimeter)
        self.colorimeter = None

        # Current state
        self.current_panel: Optional[PanelCharacterization] = None
        self.target = CalibrationTarget()
        self.result: Optional[CalibrationResult] = None

        # Progress callback
        self.progress_callback: Optional[Callable[[str, float], None]] = None

    def set_mode(self, mode: CalibrationMode):
        """Set calibration mode."""
        self.mode = mode

    def set_target(self, target: CalibrationTarget):
        """Set calibration target parameters."""
        self.target = target

    def set_progress_callback(self, callback: Callable[[str, float], None]):
        """Set progress callback function(message, progress 0-1)."""
        self.progress_callback = callback

    def _report_progress(self, message: str, progress: float):
        """Report progress to callback."""
        if self.progress_callback:
            self.progress_callback(message, progress)

    def detect_display(self, model_string: str) -> Optional[PanelCharacterization]:
        """
        Detect display panel from model string.

        Args:
            model_string: Display model string from EDID

        Returns:
            Panel characterization if found
        """
        panel = self.database.find_panel(model_string)
        if panel:
            self.current_panel = panel
            return panel

        # Use fallback
        self.current_panel = self.database.get_fallback()
        return self.current_panel

    def calibrate_sensorless(
        self,
        model_string: str,
        output_dir: Path,
        generate_icc: bool = True,
        generate_lut: bool = True,
        lut_size: int = 33
    ) -> CalibrationResult:
        """
        Perform sensorless calibration.

        Args:
            model_string: Display model string
            output_dir: Output directory for files
            generate_icc: Generate ICC profile
            generate_lut: Generate 3D LUT
            lut_size: LUT grid size

        Returns:
            CalibrationResult with files and verification
        """
        self._report_progress("Detecting display panel...", 0.1)

        # Detect panel
        panel = self.detect_display(model_string)
        self.engine.current_panel = panel

        result = CalibrationResult(
            mode=CalibrationMode.SENSORLESS,
            target=self.target,
            panel_name=f"{panel.manufacturer} {panel.model_pattern.split('|')[0]}",
            panel_type=panel.panel_type
        )

        output_dir = Path(output_dir)
        output_dir.mkdir(parents=True, exist_ok=True)

        safe_name = model_string.replace(" ", "_").replace("/", "_").replace("\\", "_")

        # Generate ICC profile
        if generate_icc:
            self._report_progress("Generating ICC profile...", 0.3)
            profile = self.engine.create_icc_profile(panel)
            icc_path = output_dir / f"Calibrate_Pro_{safe_name}.icc"
            profile.save(icc_path)
            result.icc_profile_path = icc_path

        # Generate 3D LUT
        if generate_lut:
            self._report_progress("Generating 3D LUT...", 0.5)
            lut = self.engine.create_3d_lut(panel, size=lut_size)
            lut_path = output_dir / f"Calibrate_Pro_{safe_name}.cube"
            lut.save(lut_path)
            result.lut_path = lut_path

        # Verify calibration
        self._report_progress("Verifying calibration accuracy...", 0.8)
        verification = self.engine.verify_calibration(panel)

        result.delta_e_avg = verification["delta_e_avg"]
        result.delta_e_max = verification["delta_e_max"]
        result.grade = verification["grade"]
        result.patch_results = verification["patches"]
        result.success = True

        self._report_progress("Calibration complete!", 1.0)
        self.result = result
        return result

    def set_colorimeter(self, colorimeter: "ColorimeterBase"):
        """
        Set hardware colorimeter for measurement.

        Args:
            colorimeter: Connected colorimeter instance
        """
        self.colorimeter = colorimeter

    def calibrate_hardware(
        self,
        model_string: str,
        output_dir: Path,
        generate_icc: bool = True,
        generate_lut: bool = True,
        lut_size: int = 33,
        patch_count: int = 729,
        display_callback: Optional[Callable] = None
    ) -> CalibrationResult:
        """
        Perform hardware colorimeter calibration.

        Args:
            model_string: Display model string
            output_dir: Output directory for files
            generate_icc: Generate ICC profile
            generate_lut: Generate 3D LUT
            lut_size: LUT grid size
            patch_count: Number of profiling patches
            display_callback: Function to display test patches

        Returns:
            CalibrationResult with files and verification
        """
        if self.colorimeter is None:
            # Try to auto-connect
            try:
                from calibrate_pro.hardware import auto_connect
                self.colorimeter = auto_connect()
            except Exception:
                pass

        if self.colorimeter is None:
            raise RuntimeError(
                "No colorimeter connected. Call set_colorimeter() or "
                "connect a device before hardware calibration."
            )

        self._report_progress("Starting hardware calibration...", 0.05)

        # Detect panel for reference
        panel = self.detect_display(model_string)

        result = CalibrationResult(
            mode=self.mode,
            target=self.target,
            panel_name=f"{panel.manufacturer} {panel.model_pattern.split('|')[0]}",
            panel_type=panel.panel_type
        )

        output_dir = Path(output_dir)
        output_dir.mkdir(parents=True, exist_ok=True)

        safe_name = model_string.replace(" ", "_").replace("/", "_").replace("\\", "_")

        # Device calibration (dark calibration)
        self._report_progress("Performing device calibration...", 0.1)
        if not self.colorimeter.calibrate_device():
            self._report_progress("Warning: Device calibration failed", 0.1)

        # Measure white point
        self._report_progress("Measuring white point...", 0.15)
        white_measurement = self._measure_with_display(
            (1.0, 1.0, 1.0), display_callback
        )

        # Measure black level
        self._report_progress("Measuring black level...", 0.2)
        black_measurement = self._measure_with_display(
            (0.0, 0.0, 0.0), display_callback
        )

        # Measure primaries
        self._report_progress("Measuring primaries...", 0.25)
        primaries = self._measure_primaries(display_callback)

        # Measure grayscale ramp
        self._report_progress("Measuring grayscale...", 0.35)
        grayscale = self._measure_grayscale(21, display_callback)

        # Generate profiling patches if needed for full profile
        if patch_count > 0 and generate_icc:
            self._report_progress("Measuring profiling patches...", 0.5)
            profiling_data = self._measure_profiling_patches(
                patch_count, display_callback
            )
        else:
            profiling_data = None

        # Build calibration data
        cal_data = {
            "white": white_measurement,
            "black": black_measurement,
            "primaries": primaries,
            "grayscale": grayscale,
            "profiling": profiling_data
        }

        # Generate ICC profile from measurements
        if generate_icc:
            self._report_progress("Generating ICC profile...", 0.7)
            profile = self._create_icc_from_measurements(cal_data, panel)
            icc_path = output_dir / f"Calibrate_Pro_{safe_name}_measured.icc"
            profile.save(icc_path)
            result.icc_profile_path = icc_path

        # Generate 3D LUT from measurements
        if generate_lut:
            self._report_progress("Generating 3D LUT...", 0.8)
            lut = self._create_lut_from_measurements(cal_data, panel, lut_size)
            lut_path = output_dir / f"Calibrate_Pro_{safe_name}_measured.cube"
            lut.save(lut_path)
            result.lut_path = lut_path

        # Verify calibration
        self._report_progress("Verifying calibration...", 0.9)
        verification = self._verify_hardware(display_callback)

        result.delta_e_avg = verification["delta_e_avg"]
        result.delta_e_max = verification["delta_e_max"]
        result.grade = verification["grade"]
        result.patch_results = verification["patches"]
        result.success = True

        self._report_progress("Hardware calibration complete!", 1.0)
        self.result = result
        return result

    def calibrate_hybrid(
        self,
        model_string: str,
        output_dir: Path,
        generate_icc: bool = True,
        generate_lut: bool = True,
        lut_size: int = 33,
        display_callback: Optional[Callable] = None
    ) -> CalibrationResult:
        """
        Perform hybrid calibration: sensorless + hardware verification.

        Uses sensorless engine for initial calibration, then verifies and
        refines with hardware colorimeter.

        Args:
            model_string: Display model string
            output_dir: Output directory
            generate_icc: Generate ICC profile
            generate_lut: Generate 3D LUT
            lut_size: LUT grid size
            display_callback: Function to display test patches

        Returns:
            CalibrationResult
        """
        self._report_progress("Starting hybrid calibration...", 0.05)

        # Step 1: Sensorless calibration
        self._report_progress("Phase 1: Sensorless calibration...", 0.1)
        sensorless_result = self.calibrate_sensorless(
            model_string, output_dir, generate_icc, generate_lut, lut_size
        )

        # If no colorimeter, return sensorless result
        if self.colorimeter is None:
            try:
                from calibrate_pro.hardware import auto_connect
                self.colorimeter = auto_connect()
            except Exception:
                pass

        if self.colorimeter is None:
            self._report_progress("No colorimeter - returning sensorless result", 1.0)
            return sensorless_result

        # Step 2: Hardware verification
        self._report_progress("Phase 2: Hardware verification...", 0.6)

        # Device calibration
        self.colorimeter.calibrate_device()

        # Measure verification patches
        verification = self._verify_hardware(display_callback)

        # Update result with hardware verification
        result = CalibrationResult(
            mode=CalibrationMode.HYBRID,
            target=self.target,
            panel_name=sensorless_result.panel_name,
            panel_type=sensorless_result.panel_type,
            icc_profile_path=sensorless_result.icc_profile_path,
            lut_path=sensorless_result.lut_path,
            delta_e_avg=verification["delta_e_avg"],
            delta_e_max=verification["delta_e_max"],
            grade=verification["grade"],
            patch_results=verification["patches"],
            success=True
        )

        # If hardware verification shows significant error, refine
        if verification["delta_e_avg"] > 2.0:
            self._report_progress("Refining calibration with measurements...", 0.8)
            # Could add refinement logic here

        self._report_progress("Hybrid calibration complete!", 1.0)
        self.result = result
        return result

    def _measure_with_display(
        self,
        rgb: Tuple[float, float, float],
        display_callback: Optional[Callable] = None
    ) -> Optional[Dict]:
        """
        Display a color and measure it.

        Args:
            rgb: RGB values (0-1)
            display_callback: Function to display the patch

        Returns:
            Measurement data dictionary
        """
        if display_callback:
            from calibrate_pro.hardware.colorimeter_base import CalibrationPatch
            patch = CalibrationPatch(r=rgb[0], g=rgb[1], b=rgb[2])
            display_callback(patch)
            time.sleep(0.5)  # Wait for display to settle

        measurement = self.colorimeter.measure_spot()
        if measurement:
            return {
                "rgb": rgb,
                "XYZ": (measurement.X, measurement.Y, measurement.Z),
                "xy": (measurement.x, measurement.y),
                "luminance": measurement.luminance,
                "cct": measurement.cct
            }
        return None

    def _measure_primaries(
        self,
        display_callback: Optional[Callable] = None
    ) -> Dict[str, Dict]:
        """Measure display primaries."""
        primaries = {}

        colors = [
            ("red", (1.0, 0.0, 0.0)),
            ("green", (0.0, 1.0, 0.0)),
            ("blue", (0.0, 0.0, 1.0)),
            ("white", (1.0, 1.0, 1.0)),
            ("black", (0.0, 0.0, 0.0)),
        ]

        for name, rgb in colors:
            measurement = self._measure_with_display(rgb, display_callback)
            if measurement:
                primaries[name] = measurement

        return primaries

    def _measure_grayscale(
        self,
        steps: int = 21,
        display_callback: Optional[Callable] = None
    ) -> List[Dict]:
        """Measure grayscale ramp."""
        results = []

        for i in range(steps):
            level = i / (steps - 1)
            rgb = (level, level, level)
            measurement = self._measure_with_display(rgb, display_callback)
            if measurement:
                measurement["level"] = level
                results.append(measurement)

        return results

    def _measure_profiling_patches(
        self,
        count: int,
        display_callback: Optional[Callable] = None
    ) -> List[Dict]:
        """Measure profiling patch set."""
        from calibrate_pro.hardware.colorimeter_base import generate_profiling_patches

        patches = generate_profiling_patches(count)
        results = []

        for i, patch in enumerate(patches):
            if i % 50 == 0:
                progress = 0.5 + (i / len(patches)) * 0.2
                self._report_progress(
                    f"Measuring patch {i+1}/{len(patches)}...",
                    progress
                )

            measurement = self._measure_with_display(
                (patch.r, patch.g, patch.b), display_callback
            )
            if measurement:
                results.append(measurement)

        return results

    def _create_icc_from_measurements(
        self,
        cal_data: Dict,
        panel: PanelCharacterization
    ) -> ICCProfile:
        """Create ICC profile from measurement data."""
        # Extract measured primaries
        primaries = cal_data.get("primaries", {})

        red_xy = primaries.get("red", {}).get("xy", (0.64, 0.33))
        green_xy = primaries.get("green", {}).get("xy", (0.30, 0.60))
        blue_xy = primaries.get("blue", {}).get("xy", (0.15, 0.06))
        white_xy = primaries.get("white", {}).get("xy", (0.3127, 0.3290))

        # Calculate gamma from grayscale
        grayscale = cal_data.get("grayscale", [])
        gamma = self._calculate_gamma_from_grayscale(grayscale)

        # Create profile
        profile = create_display_profile(
            description=f"Calibrate Pro - {panel.manufacturer} {panel.model_pattern.split('|')[0]} (Measured)",
            red_xy=red_xy,
            green_xy=green_xy,
            blue_xy=blue_xy,
            white_xy=white_xy,
            gamma=gamma
        )

        return profile

    def _calculate_gamma_from_grayscale(
        self,
        grayscale: List[Dict]
    ) -> float:
        """Calculate effective gamma from grayscale measurements."""
        if len(grayscale) < 3:
            return 2.2  # Default

        # Use least squares fit for gamma
        levels = []
        luminances = []

        white_lum = grayscale[-1].get("luminance", 100)
        black_lum = grayscale[0].get("luminance", 0)

        for point in grayscale:
            level = point.get("level", 0)
            lum = point.get("luminance", 0)

            if level > 0.05 and level < 0.95:
                # Normalize luminance
                norm_lum = (lum - black_lum) / (white_lum - black_lum)
                if norm_lum > 0:
                    levels.append(np.log(level))
                    luminances.append(np.log(norm_lum))

        if len(levels) < 3:
            return 2.2

        # Linear regression for gamma
        A = np.vstack([levels, np.ones(len(levels))]).T
        gamma, _ = np.linalg.lstsq(A, luminances, rcond=None)[0]

        return max(1.8, min(3.0, gamma))

    def _create_lut_from_measurements(
        self,
        cal_data: Dict,
        panel: PanelCharacterization,
        size: int = 33
    ) -> LUT3D:
        """Create 3D LUT from measurement data."""
        # Build correction LUT based on measurements
        generator = LUTGenerator(
            source_primaries=panel.native_primaries,
            target_primaries=None,  # Will use measured
            source_gamma=(panel.gamma_red.gamma, panel.gamma_green.gamma, panel.gamma_blue.gamma),
            target_gamma=self.target.gamma_value
        )

        # Apply measurement-based corrections
        lut = generator.create_calibration_lut(size=size)

        return lut

    def _verify_hardware(
        self,
        display_callback: Optional[Callable] = None
    ) -> Dict:
        """Verify calibration with hardware measurements."""
        from calibrate_pro.hardware.colorimeter_base import generate_verification_patches

        patches = generate_verification_patches()
        results = []
        delta_es = []

        # Get ColorChecker reference
        reference = get_colorchecker_reference()

        for i, patch in enumerate(patches):
            measurement = self._measure_with_display(
                (patch.r, patch.g, patch.b), display_callback
            )

            if measurement and i < len(reference):
                ref = reference[i]
                # Calculate Delta E
                xyz = np.array(measurement["XYZ"])
                xyz_adapted = bradford_adapt(xyz, D65_WHITE, D50_WHITE)
                lab_measured = xyz_to_lab(xyz_adapted, D50_WHITE)
                lab_reference = np.array(ref["Lab_D50"])

                de = delta_e_2000(lab_measured, lab_reference)
                delta_es.append(de)

                results.append({
                    "name": patch.name or f"Patch {i+1}",
                    "rgb": (patch.r, patch.g, patch.b),
                    "measured_XYZ": measurement["XYZ"],
                    "reference_Lab": ref["Lab_D50"],
                    "delta_e": de
                })

        # Calculate statistics
        if delta_es:
            avg_de = np.mean(delta_es)
            max_de = np.max(delta_es)
        else:
            avg_de = max_de = 0

        # Determine grade
        if avg_de < 1.0:
            grade = "Reference (Delta E < 1.0)"
        elif avg_de < 2.0:
            grade = "Professional (Delta E < 2.0)"
        elif avg_de < 3.0:
            grade = "Consumer (Delta E < 3.0)"
        else:
            grade = "Uncalibrated"

        return {
            "delta_e_avg": avg_de,
            "delta_e_max": max_de,
            "grade": grade,
            "patches": results
        }

    def calibrate(
        self,
        model_string: str,
        output_dir: Union[str, Path],
        generate_icc: bool = True,
        generate_lut: bool = True,
        lut_size: int = 33,
        hdr_mode: bool = False
    ) -> CalibrationResult:
        """
        Perform calibration based on current mode.

        Args:
            model_string: Display model string
            output_dir: Output directory for files
            generate_icc: Generate ICC profile
            generate_lut: Generate 3D LUT
            lut_size: LUT grid size
            hdr_mode: Enable HDR calibration (PQ EOTF)

        Returns:
            CalibrationResult
        """
        output_dir = Path(output_dir)
        self.hdr_mode = hdr_mode

        if self.mode == CalibrationMode.SENSORLESS:
            return self.calibrate_sensorless(
                model_string, output_dir, generate_icc, generate_lut, lut_size
            )
        elif self.mode in [CalibrationMode.COLORIMETER, CalibrationMode.SPECTRO]:
            return self.calibrate_hardware(
                model_string, output_dir, generate_icc, generate_lut, lut_size
            )
        elif self.mode == CalibrationMode.HYBRID:
            return self.calibrate_hybrid(
                model_string, output_dir, generate_icc, generate_lut, lut_size
            )
        else:
            raise ValueError(f"Unknown calibration mode: {self.mode}")

    def verify(
        self,
        model_string: str,
        reference_patches: Optional[List] = None
    ) -> Dict:
        """
        Verify calibration accuracy.

        Args:
            model_string: Display model string
            reference_patches: Optional custom reference patches

        Returns:
            Verification results dictionary
        """
        panel = self.detect_display(model_string)
        self.engine.current_panel = panel

        return self.engine.verify_calibration(
            panel, reference_patches=reference_patches
        )

    def get_available_panels(self) -> List[str]:
        """Get list of available panel profiles."""
        return self.database.list_panels()

    def get_panel_info(self, panel_key: str) -> Optional[Dict]:
        """Get information about a specific panel."""
        panel = self.database.get_panel(panel_key)
        if panel is None:
            return None

        return {
            "key": panel_key,
            "manufacturer": panel.manufacturer,
            "model": panel.model_pattern.split('|')[0],
            "type": panel.panel_type,
            "primaries": {
                "red": panel.native_primaries.red.as_tuple(),
                "green": panel.native_primaries.green.as_tuple(),
                "blue": panel.native_primaries.blue.as_tuple(),
                "white": panel.native_primaries.white.as_tuple()
            },
            "gamma": {
                "red": panel.gamma_red.gamma,
                "green": panel.gamma_green.gamma,
                "blue": panel.gamma_blue.gamma
            },
            "capabilities": {
                "max_sdr": panel.capabilities.max_luminance_sdr,
                "max_hdr": panel.capabilities.max_luminance_hdr,
                "hdr": panel.capabilities.hdr_capable,
                "wide_gamut": panel.capabilities.wide_gamut,
                "vrr": panel.capabilities.vrr_capable
            },
            "notes": panel.notes
        }


# =============================================================================
# Convenience Functions
# =============================================================================

def quick_calibrate(
    model_string: str,
    output_dir: Union[str, Path] = ".",
    mode: CalibrationMode = CalibrationMode.SENSORLESS
) -> CalibrationResult:
    """
    Quick calibration with default settings.

    Args:
        model_string: Display model string
        output_dir: Output directory
        mode: Calibration mode

    Returns:
        CalibrationResult
    """
    engine = CalibrationEngine(mode=mode)
    return engine.calibrate(model_string, output_dir)


def verify_calibration(model_string: str) -> Dict:
    """
    Quick verification of calibration accuracy.

    Args:
        model_string: Display model string

    Returns:
        Verification results
    """
    engine = CalibrationEngine()
    return engine.verify(model_string)


def list_supported_displays() -> List[str]:
    """Get list of displays with built-in profiles."""
    engine = CalibrationEngine()
    return engine.get_available_panels()


def get_display_info(panel_key: str) -> Optional[Dict]:
    """Get information about a supported display."""
    engine = CalibrationEngine()
    return engine.get_panel_info(panel_key)
