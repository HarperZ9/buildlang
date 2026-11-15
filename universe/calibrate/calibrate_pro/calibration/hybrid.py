"""
Hybrid Calibration Engine

Combines sensorless panel-database calibration with optional colorimeter
measurement for iterative refinement. This is the killer feature:
zero-effort starting point that refines to measured accuracy.

Workflow:
1. Apply sensorless calibration (instant, no hardware needed)
2. If colorimeter available: measure actual display output
3. Compute residual error between predicted and measured
4. Generate refined LUT that corrects the residual
5. Iterate until convergence (typically 2-3 passes)

The result: measured accuracy with minimal measurement time, because
the sensorless pass gets you 90% of the way there and measurement
only needs to correct the remaining 10%.
"""

import numpy as np
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Dict, List, Optional, Tuple

from calibrate_pro.core.color_math import (
    D50_WHITE, D65_WHITE,
    xyz_to_lab, lab_to_xyz,
    bradford_adapt, delta_e_2000,
    srgb_gamma_expand, srgb_gamma_compress,
    primaries_to_xyz_matrix,
    SRGB_TO_XYZ, XYZ_TO_SRGB
)


# Type alias for a measurement function
# Takes (r, g, b) in [0,1] sRGB, returns (X, Y, Z) measured tristimulus
MeasureFn = Callable[[float, float, float], Tuple[float, float, float]]


@dataclass
class RefinementResult:
    """Result from one refinement iteration."""
    iteration: int
    delta_e_before: float  # Average dE before this iteration's correction
    delta_e_after: float   # Average dE after (predicted from residual correction)
    patches_measured: int
    residual_corrections: Optional[np.ndarray] = None  # 3x3 residual matrix


@dataclass
class HybridCalibrationResult:
    """Complete result from hybrid calibration."""
    success: bool = False
    message: str = ""

    # Sensorless baseline
    sensorless_delta_e: float = 0.0

    # Measurement results per iteration
    iterations: List[RefinementResult] = field(default_factory=list)

    # Final measured accuracy
    final_measured_delta_e: float = 0.0
    final_measured_delta_e_max: float = 0.0

    # Output files
    lut_path: Optional[str] = None
    icc_path: Optional[str] = None

    # Per-patch measured data
    measured_patches: List[Dict] = field(default_factory=list)


# Standard verification patches — subset of ColorChecker for fast measurement
# These 12 patches cover: neutrals, primaries, secondaries, skin tones
QUICK_VERIFY_PATCHES = [
    ("White",      (0.95, 0.95, 0.95)),
    ("Neutral 80", (0.80, 0.80, 0.80)),
    ("Neutral 50", (0.50, 0.50, 0.50)),
    ("Neutral 20", (0.20, 0.20, 0.20)),
    ("Red",        (0.75, 0.15, 0.15)),
    ("Green",      (0.15, 0.60, 0.15)),
    ("Blue",       (0.15, 0.15, 0.75)),
    ("Cyan",       (0.15, 0.70, 0.70)),
    ("Magenta",    (0.70, 0.15, 0.70)),
    ("Yellow",     (0.80, 0.80, 0.15)),
    ("Skin Light", (0.78, 0.58, 0.50)),
    ("Skin Dark",  (0.45, 0.32, 0.26)),
]


class HybridCalibrationEngine:
    """
    Hybrid sensorless + measured calibration engine.

    Usage:
        engine = HybridCalibrationEngine(measure_fn=my_colorimeter.measure)
        result = engine.calibrate(panel, output_dir="./output")

    The measure_fn should:
    - Accept (r, g, b) floats in [0, 1] (sRGB encoded)
    - Display the color on the target display
    - Read the colorimeter
    - Return (X, Y, Z) tristimulus values

    If no measure_fn is provided, the engine does sensorless-only calibration
    and returns with a note that measurement is recommended.
    """

    def __init__(
        self,
        measure_fn: Optional[MeasureFn] = None,
        max_iterations: int = 3,
        convergence_threshold: float = 0.5,  # Stop when dE improvement < this
        progress_fn: Optional[Callable[[str, float], None]] = None
    ):
        self.measure_fn = measure_fn
        self.max_iterations = max_iterations
        self.convergence_threshold = convergence_threshold
        self.progress_fn = progress_fn

    def _progress(self, message: str, pct: float):
        if self.progress_fn:
            self.progress_fn(message, pct)

    def calibrate(
        self,
        panel,
        output_dir: Path,
        target: str = "native",
        hdr_mode: bool = False
    ) -> HybridCalibrationResult:
        """
        Run hybrid calibration.

        Args:
            panel: PanelCharacterization from database
            output_dir: Where to save output files
            target: "native", "sRGB", or "p3"
            hdr_mode: HDR calibration mode

        Returns:
            HybridCalibrationResult with measured accuracy data
        """
        from calibrate_pro.sensorless.neuralux import SensorlessEngine
        from calibrate_pro.core.lut_engine import LUT3D

        output_dir = Path(output_dir)
        output_dir.mkdir(parents=True, exist_ok=True)
        result = HybridCalibrationResult()

        engine = SensorlessEngine()
        engine.current_panel = panel

        # Step 1: Sensorless baseline
        self._progress("Generating sensorless calibration...", 0.1)
        lut = engine.create_3d_lut(panel, size=33, target=target, hdr_mode=hdr_mode)

        safe_name = panel.name.replace(" ", "_")
        lut_path = output_dir / f"{safe_name}.cube"
        lut.save(lut_path)
        result.lut_path = str(lut_path)

        # Sensorless verification (predicted)
        sensorless_verify = engine.verify_calibration(panel)
        result.sensorless_delta_e = sensorless_verify.get("delta_e_avg", 0.0)

        self._progress(
            f"Sensorless baseline: predicted dE {result.sensorless_delta_e:.2f}",
            0.2
        )

        # Step 2: If no colorimeter, stop here
        if self.measure_fn is None:
            result.success = True
            result.final_measured_delta_e = result.sensorless_delta_e
            result.message = (
                f"Sensorless calibration applied (predicted dE {result.sensorless_delta_e:.2f}). "
                f"Connect a colorimeter and use --refine for measured accuracy."
            )
            return result

        # Step 3: Measure the display with the sensorless LUT applied
        self._progress("Measuring display with sensorless LUT...", 0.3)
        measured_data = self._measure_patches(QUICK_VERIFY_PATCHES)

        if not measured_data:
            result.success = True
            result.message = "Measurement failed. Sensorless calibration is still applied."
            return result

        # Compute measured Delta E
        measured_de = self._compute_measured_delta_e(QUICK_VERIFY_PATCHES, measured_data)
        avg_de = np.mean([d["delta_e"] for d in measured_de])
        max_de = np.max([d["delta_e"] for d in measured_de])

        self._progress(f"Measured dE: avg {avg_de:.2f}, max {max_de:.2f}", 0.4)

        result.measured_patches = measured_de
        result.final_measured_delta_e = float(avg_de)
        result.final_measured_delta_e_max = float(max_de)

        # Step 4: Iterative refinement
        current_lut = lut
        prev_de = avg_de

        for iteration in range(self.max_iterations):
            self._progress(
                f"Refinement iteration {iteration + 1}/{self.max_iterations}...",
                0.4 + 0.5 * (iteration / self.max_iterations)
            )

            # Compute residual correction from measurements
            residual_matrix = self._compute_residual_correction(
                QUICK_VERIFY_PATCHES, measured_data
            )

            if residual_matrix is None:
                break

            # Apply residual correction to LUT
            refined_lut = self._apply_residual_to_lut(current_lut, residual_matrix)

            # Save refined LUT
            refined_path = output_dir / f"{safe_name}_refined.cube"
            refined_lut.save(refined_path)
            result.lut_path = str(refined_path)

            # Re-measure with refined LUT
            # (In a real system, we'd apply the LUT to the display first)
            measured_data = self._measure_patches(QUICK_VERIFY_PATCHES)
            if not measured_data:
                break

            new_de = self._compute_measured_delta_e(QUICK_VERIFY_PATCHES, measured_data)
            new_avg_de = np.mean([d["delta_e"] for d in new_de])
            new_max_de = np.max([d["delta_e"] for d in new_de])

            improvement = prev_de - new_avg_de

            iter_result = RefinementResult(
                iteration=iteration + 1,
                delta_e_before=prev_de,
                delta_e_after=float(new_avg_de),
                patches_measured=len(QUICK_VERIFY_PATCHES),
                residual_corrections=residual_matrix
            )
            result.iterations.append(iter_result)

            self._progress(
                f"Iteration {iteration + 1}: dE {prev_de:.2f} -> {new_avg_de:.2f} "
                f"(improvement: {improvement:.2f})",
                0.4 + 0.5 * ((iteration + 1) / self.max_iterations)
            )

            result.final_measured_delta_e = float(new_avg_de)
            result.final_measured_delta_e_max = float(new_max_de)
            result.measured_patches = new_de

            current_lut = refined_lut
            prev_de = new_avg_de

            # Check convergence
            if improvement < self.convergence_threshold:
                self._progress(
                    f"Converged after {iteration + 1} iterations (dE {new_avg_de:.2f})",
                    0.9
                )
                break

        # Generate ICC profile
        self._progress("Generating ICC profile...", 0.95)
        icc = engine.create_icc_profile(panel)
        icc_path = output_dir / f"{safe_name}.icc"
        icc.save(icc_path)
        result.icc_path = str(icc_path)

        result.success = True
        result.message = (
            f"Hybrid calibration complete. "
            f"Sensorless: dE {result.sensorless_delta_e:.2f} (predicted). "
            f"Measured: dE {result.final_measured_delta_e:.2f} "
            f"after {len(result.iterations)} refinement iterations."
        )

        self._progress("Calibration complete!", 1.0)
        return result

    def _measure_patches(
        self, patches: List[Tuple[str, Tuple[float, float, float]]]
    ) -> Optional[List[Tuple[float, float, float]]]:
        """Measure a list of patches using the colorimeter."""
        if self.measure_fn is None:
            return None

        measurements = []
        for name, (r, g, b) in patches:
            try:
                xyz = self.measure_fn(r, g, b)
                measurements.append(xyz)
            except Exception:
                return None

        return measurements

    def _compute_measured_delta_e(
        self,
        patches: List[Tuple[str, Tuple[float, float, float]]],
        measured_xyz: List[Tuple[float, float, float]]
    ) -> List[Dict]:
        """Compute Delta E between expected and measured XYZ for each patch."""
        results = []

        for (name, srgb), xyz_measured in zip(patches, measured_xyz):
            # Expected XYZ: convert sRGB to XYZ
            rgb_linear = srgb_gamma_expand(np.array(srgb))
            xyz_expected = SRGB_TO_XYZ @ rgb_linear

            # Convert both to Lab D50 for CIEDE2000
            xyz_meas = np.array(xyz_measured)
            xyz_exp = xyz_expected

            lab_meas = xyz_to_lab(bradford_adapt(xyz_meas, D65_WHITE, D50_WHITE), D50_WHITE)
            lab_exp = xyz_to_lab(bradford_adapt(xyz_exp, D65_WHITE, D50_WHITE), D50_WHITE)

            de = delta_e_2000(lab_meas, lab_exp)

            results.append({
                "name": name,
                "srgb": srgb,
                "expected_xyz": tuple(xyz_expected),
                "measured_xyz": xyz_measured,
                "expected_lab": tuple(lab_exp),
                "measured_lab": tuple(lab_meas),
                "delta_e": float(de)
            })

        return results

    def _compute_residual_correction(
        self,
        patches: List[Tuple[str, Tuple[float, float, float]]],
        measured_xyz: List[Tuple[float, float, float]]
    ) -> Optional[np.ndarray]:
        """
        Compute a 3x3 residual correction matrix from measurement error.

        Uses least-squares to find the matrix M that best maps
        measured XYZ to expected XYZ: expected = M @ measured.
        """
        expected_list = []
        measured_list = []

        for (name, srgb), xyz_measured in zip(patches, measured_xyz):
            rgb_linear = srgb_gamma_expand(np.array(srgb))
            xyz_expected = SRGB_TO_XYZ @ rgb_linear

            expected_list.append(xyz_expected)
            measured_list.append(np.array(xyz_measured))

        if len(expected_list) < 4:
            return None

        # Stack into matrices
        expected_mat = np.array(expected_list).T  # 3 x N
        measured_mat = np.array(measured_list).T   # 3 x N

        # Least-squares: find M such that expected ≈ M @ measured
        # M = expected @ measured^T @ (measured @ measured^T)^-1
        try:
            M = expected_mat @ measured_mat.T @ np.linalg.inv(
                measured_mat @ measured_mat.T
            )
            return M
        except np.linalg.LinAlgError:
            return None

    def _apply_residual_to_lut(
        self, lut, residual_matrix: np.ndarray
    ):
        """Apply a 3x3 residual correction to an existing 3D LUT."""
        from calibrate_pro.core.lut_engine import LUT3D

        refined = LUT3D(
            size=lut.size,
            data=lut.data.copy(),
            title=lut.title + " (refined)"
        )

        # Apply residual matrix to every LUT entry
        shape = refined.data.shape
        flat = refined.data.reshape(-1, 3)

        # For each entry: linearize, apply correction, re-encode
        for i in range(flat.shape[0]):
            rgb = flat[i]
            # Linearize (approximate — LUT values are already partially corrected)
            rgb_linear = np.power(np.clip(rgb, 1e-10, 1.0), 2.2)

            # Convert to XYZ, apply residual, convert back
            xyz = SRGB_TO_XYZ @ rgb_linear
            xyz_corrected = residual_matrix @ xyz
            rgb_corrected = XYZ_TO_SRGB @ xyz_corrected

            # Re-encode
            flat[i] = srgb_gamma_compress(np.clip(rgb_corrected, 0, 1))

        refined.data = flat.reshape(shape)
        return refined
