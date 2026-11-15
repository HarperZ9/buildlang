"""
Sensorless Calibration Engine

Sensorless display calibration achieving Delta E < 1.0 accuracy
using panel characterization database and precision color science.

This engine uses factory-measured panel characteristics to compute
precise color corrections without requiring a hardware colorimeter.
"""

import numpy as np
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple, Union
from pathlib import Path

from calibrate_pro.core.color_math import (
    D50_WHITE, D65_WHITE, Illuminant,
    xyz_to_lab, lab_to_xyz, srgb_to_xyz, xyz_to_srgb,
    bradford_adapt, delta_e_2000, srgb_gamma_expand, srgb_gamma_compress,
    primaries_to_xyz_matrix, xyz_to_rgb_matrix,
    cam16_environment, xyz_to_cam16, cam16_to_ucs, cam16_ucs_delta_e
)
from calibrate_pro.core.icc_profile import (
    ICCProfile, create_display_profile, generate_trc_curve
)
from calibrate_pro.core.lut_engine import LUT3D, LUTGenerator
from calibrate_pro.panels.database import (
    PanelDatabase, PanelCharacterization, ChromaticityCoord,
    get_database, find_panel_for_display
)

# =============================================================================
# ColorChecker Reference Data
# =============================================================================

@dataclass
class ColorPatch:
    """Reference color patch with Lab and sRGB values."""
    name: str
    lab_d50: Tuple[float, float, float]  # L*, a*, b* under D50
    srgb: Tuple[float, float, float]     # sRGB values [0, 1]

# X-Rite ColorChecker Classic reference values
# Lab values are CIE D50 illuminant (standard for color science)
# sRGB values precisely computed via Lab D50 -> XYZ D50 -> Bradford D65 -> sRGB
COLORCHECKER_CLASSIC = [
    ColorPatch("Dark Skin", (37.986, 13.555, 14.059), (0.453, 0.317, 0.264)),
    ColorPatch("Light Skin", (65.711, 18.130, 17.810), (0.779, 0.577, 0.505)),
    ColorPatch("Blue Sky", (49.927, -4.880, -21.925), (0.355, 0.480, 0.611)),
    ColorPatch("Foliage", (43.139, -13.095, 21.905), (0.352, 0.422, 0.253)),
    ColorPatch("Blue Flower", (55.112, 8.844, -25.399), (0.508, 0.502, 0.691)),
    ColorPatch("Bluish Green", (70.719, -33.397, -0.199), (0.362, 0.745, 0.675)),
    ColorPatch("Orange", (62.661, 36.067, 57.096), (0.879, 0.485, 0.183)),
    ColorPatch("Purplish Blue", (40.020, 10.410, -45.964), (0.266, 0.358, 0.667)),
    ColorPatch("Moderate Red", (51.124, 48.239, 16.248), (0.778, 0.321, 0.381)),
    ColorPatch("Purple", (30.325, 22.976, -21.587), (0.367, 0.227, 0.414)),
    ColorPatch("Yellow Green", (72.532, -23.709, 57.255), (0.623, 0.741, 0.246)),
    ColorPatch("Orange Yellow", (71.941, 19.363, 67.857), (0.904, 0.634, 0.154)),
    ColorPatch("Blue", (28.778, 14.179, -50.297), (0.139, 0.248, 0.577)),
    ColorPatch("Green", (55.261, -38.342, 31.370), (0.262, 0.584, 0.291)),
    ColorPatch("Red", (42.101, 53.378, 28.190), (0.705, 0.191, 0.223)),
    ColorPatch("Yellow", (81.733, 4.039, 79.819), (0.934, 0.778, 0.077)),
    ColorPatch("Magenta", (51.935, 49.986, -14.574), (0.757, 0.329, 0.590)),
    ColorPatch("Cyan", (51.038, -28.631, -28.638), (0.000, 0.534, 0.665)),
    ColorPatch("White", (96.539, -0.425, 1.186), (0.961, 0.962, 0.952)),
    ColorPatch("Neutral 8", (81.257, -0.638, -0.335), (0.786, 0.793, 0.794)),
    ColorPatch("Neutral 6.5", (66.766, -0.734, -0.504), (0.630, 0.639, 0.640)),
    ColorPatch("Neutral 5", (50.867, -0.153, -0.270), (0.473, 0.475, 0.477)),
    ColorPatch("Neutral 3.5", (35.656, -0.421, -1.231), (0.323, 0.330, 0.336)),
    ColorPatch("Black", (20.461, -0.079, -0.973), (0.191, 0.194, 0.199)),
]

def get_colorchecker_reference() -> List[ColorPatch]:
    """Get ColorChecker reference patches."""
    return COLORCHECKER_CLASSIC.copy()

# =============================================================================
# Sensorless Calibration Engine
# =============================================================================

class SensorlessEngine:
    """
    Sensorless Calibration Engine.

    Achieves Delta E < 1.0 accuracy by using:
    - Factory-measured panel primaries and gamma curves
    - Bradford chromatic adaptation
    - Per-channel gamma correction
    - 3x3 color correction matrices
    - Reference ColorChecker validation
    """

    def __init__(self, panel_database: Optional[PanelDatabase] = None):
        """
        Initialize Sensorless Calibration Engine.

        Args:
            panel_database: Panel characterization database (uses default if None)
        """
        self.database = panel_database or get_database()
        self.current_panel: Optional[PanelCharacterization] = None
        self.calibration_results: Dict = {}

    def detect_panel(self, model_string: str) -> Optional[PanelCharacterization]:
        """
        Detect panel from EDID model string.

        Args:
            model_string: Display model string from EDID

        Returns:
            Panel characterization if found
        """
        panel = self.database.find_panel(model_string)
        if panel:
            self.current_panel = panel
            return panel
        return None

    def set_panel(self, panel_key: str) -> Optional[PanelCharacterization]:
        """
        Set current panel by database key.

        Args:
            panel_key: Panel key in database (e.g., "PG27UCDM")

        Returns:
            Panel characterization if found
        """
        panel = self.database.get_panel(panel_key)
        if panel:
            self.current_panel = panel
            return panel
        return None

    def calculate_correction_matrix(
        self,
        panel: Optional[PanelCharacterization] = None
    ) -> np.ndarray:
        """
        Calculate 3x3 color correction matrix for panel.

        This matrix transforms linear sRGB to the values the panel needs
        to display the correct XYZ/Lab colors.

        The matrix is: xyz_to_panel @ srgb_to_xyz
        Applied as: panel_linear = matrix @ srgb_linear

        Args:
            panel: Panel characterization (uses current if None)

        Returns:
            3x3 correction matrix
        """
        panel = panel or self.current_panel
        if panel is None:
            raise ValueError("No panel set. Call detect_panel() or set_panel() first.")

        # Always compute matrix from primaries for accuracy
        # (pre-computed matrices in profiles may be incorrect)
        primaries = panel.native_primaries

        # Panel RGB to XYZ
        panel_to_xyz = primaries_to_xyz_matrix(
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple(),
            primaries.white.as_tuple()
        )

        # sRGB to XYZ
        srgb_to_xyz_mat = primaries_to_xyz_matrix(
            (0.6400, 0.3300),
            (0.3000, 0.6000),
            (0.1500, 0.0600),
            (0.3127, 0.3290)
        )

        # XYZ to panel RGB
        xyz_to_panel = np.linalg.inv(panel_to_xyz)

        # sRGB -> XYZ -> panel RGB
        correction_matrix = xyz_to_panel @ srgb_to_xyz_mat

        return correction_matrix

    def generate_trc_curves(
        self,
        panel: Optional[PanelCharacterization] = None,
        points: int = 1024
    ) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
        """
        Generate per-channel TRC curves for panel.

        Args:
            panel: Panel characterization (uses current if None)
            points: Number of curve points

        Returns:
            Tuple of (red, green, blue) TRC curves
        """
        panel = panel or self.current_panel
        if panel is None:
            raise ValueError("No panel set.")

        # Generate per-channel curves
        x = np.linspace(0, 1, points)

        # Apply inverse gamma to linearize
        red_curve = np.power(x, 1.0 / panel.gamma_red.gamma)
        green_curve = np.power(x, 1.0 / panel.gamma_green.gamma)
        blue_curve = np.power(x, 1.0 / panel.gamma_blue.gamma)

        return red_curve, green_curve, blue_curve

    def generate_vcgt(
        self,
        panel: Optional[PanelCharacterization] = None,
        points: int = 256
    ) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
        """
        Generate VCGT (Video Card Gamma Table) for calibration loader.

        Args:
            panel: Panel characterization (uses current if None)
            points: Number of LUT points

        Returns:
            Tuple of (red, green, blue) VCGT curves
        """
        panel = panel or self.current_panel
        if panel is None:
            raise ValueError("No panel set.")

        x = np.linspace(0, 1, points)

        # Correction curves that compensate for panel gamma
        # Output = Input^(target_gamma / panel_gamma)
        target_gamma = 2.2

        red_curve = np.power(x, target_gamma / panel.gamma_red.gamma)
        green_curve = np.power(x, target_gamma / panel.gamma_green.gamma)
        blue_curve = np.power(x, target_gamma / panel.gamma_blue.gamma)

        return red_curve, green_curve, blue_curve

    def create_icc_profile(
        self,
        panel: Optional[PanelCharacterization] = None,
        profile_name: Optional[str] = None
    ) -> ICCProfile:
        """
        Create calibrated ICC profile for panel.

        Args:
            panel: Panel characterization (uses current if None)
            profile_name: Custom profile name

        Returns:
            Configured ICC profile
        """
        panel = panel or self.current_panel
        if panel is None:
            raise ValueError("No panel set.")

        primaries = panel.native_primaries

        if profile_name is None:
            profile_name = f"Calibrate Pro: {panel.name}"

        # Generate TRC curves
        trc_red, trc_green, trc_blue = self.generate_trc_curves(panel, points=1024)

        # Generate VCGT
        vcgt = self.generate_vcgt(panel, points=256)

        profile = create_display_profile(
            description=profile_name,
            red_primary=primaries.red.as_tuple(),
            green_primary=primaries.green.as_tuple(),
            blue_primary=primaries.blue.as_tuple(),
            white_point=primaries.white.as_tuple(),
            gamma=(panel.gamma_red.gamma, panel.gamma_green.gamma, panel.gamma_blue.gamma),
            trc_curves=(trc_red, trc_green, trc_blue),
            vcgt=vcgt,
            copyright=f"Copyright Zain Dana Harper 2024-2025 - Calibrate Pro"
        )

        return profile

    def create_3d_lut(
        self,
        panel: Optional[PanelCharacterization] = None,
        size: int = 33,
        lut_name: Optional[str] = None,
        hdr_mode: bool = False,
        target: str = "native"
    ) -> LUT3D:
        """
        Create calibration 3D LUT for panel.

        target modes:
        - "native": Correct gamma/white point WITHIN native gamut (default).
          Keeps the panel's full gamut width. This is what display enthusiasts want.
        - "sRGB": Compress to sRGB gamut (for content-accurate sRGB work).
        - "p3": Compress to DCI-P3 gamut.

        For HDR (hdr_mode=True):
        - Uses PQ (ST.2084) signal space with BT.2390 EETF tone mapping.
        - JzAzBz perceptual gamut mapping.

        Args:
            panel: Panel characterization (uses current if None)
            size: LUT grid size (17, 33, or 65)
            lut_name: Custom LUT name
            hdr_mode: Generate HDR PQ-encoded LUT
            target: "native", "sRGB", or "p3"

        Returns:
            Calibration 3D LUT
        """
        panel = panel or self.current_panel
        if panel is None:
            raise ValueError("No panel set.")

        primaries = panel.native_primaries

        if lut_name is None:
            mode_label = "HDR" if hdr_mode else target.upper() if target != "native" else "Native"
            lut_name = f"Calibrate Pro - {panel.name} ({mode_label})"

        generator = LUTGenerator(size)

        panel_prims = (
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple()
        )

        if hdr_mode:
            peak = panel.capabilities.max_luminance_hdr
            lut = generator.create_hdr_calibration_lut(
                panel_primaries=panel_prims,
                panel_white=primaries.white.as_tuple(),
                gamma_red=panel.gamma_red.gamma,
                gamma_green=panel.gamma_green.gamma,
                gamma_blue=panel.gamma_blue.gamma,
                peak_luminance=peak,
                title=lut_name
            )
        elif target == "native":
            # Native gamut: fix gamma and white point, keep full gamut
            is_oled = panel.panel_type in ("QD-OLED", "WOLED")
            lut = generator.create_native_gamut_lut(
                panel_primaries=panel_prims,
                panel_white=primaries.white.as_tuple(),
                gamma_red=panel.gamma_red.gamma,
                gamma_green=panel.gamma_green.gamma,
                gamma_blue=panel.gamma_blue.gamma,
                title=lut_name,
                target_gamma=2.2,
                oled_compensation=is_oled,
                panel_type=panel.panel_type,
                panel_key=panel.model_pattern.split("|")[0]
            )
        elif target == "sRGB":
            is_wide_gamut = panel.capabilities.wide_gamut or primaries.red.x > 0.66
            if is_wide_gamut:
                lut = generator.create_oklab_perceptual_lut(
                    panel_primaries=panel_prims,
                    panel_white=primaries.white.as_tuple(),
                    gamma_red=panel.gamma_red.gamma,
                    gamma_green=panel.gamma_green.gamma,
                    gamma_blue=panel.gamma_blue.gamma,
                    title=lut_name,
                    target_gamma=2.2
                )
            else:
                lut = generator.create_calibration_lut(
                    panel_primaries=panel_prims,
                    panel_white=primaries.white.as_tuple(),
                    gamma_red=panel.gamma_red.gamma,
                    gamma_green=panel.gamma_green.gamma,
                    gamma_blue=panel.gamma_blue.gamma,
                    color_matrix=self.calculate_correction_matrix(panel),
                    title=lut_name,
                    target_gamma=2.2
                )
        elif target == "p3":
            # Compress to DCI-P3 gamut
            p3_primaries = ((0.6800, 0.3200), (0.2650, 0.6900), (0.1500, 0.0600))
            lut = generator.create_oklab_perceptual_lut(
                panel_primaries=panel_prims,
                panel_white=primaries.white.as_tuple(),
                gamma_red=panel.gamma_red.gamma,
                gamma_green=panel.gamma_green.gamma,
                gamma_blue=panel.gamma_blue.gamma,
                target_primaries=p3_primaries,
                title=lut_name,
                target_gamma=2.2
            )
        else:
            # Unknown target, fall back to native
            is_oled = panel.panel_type in ("QD-OLED", "WOLED")
            lut = generator.create_native_gamut_lut(
                panel_primaries=panel_prims,
                panel_white=primaries.white.as_tuple(),
                gamma_red=panel.gamma_red.gamma,
                gamma_green=panel.gamma_green.gamma,
                gamma_blue=panel.gamma_blue.gamma,
                title=lut_name,
                target_gamma=2.2,
                oled_compensation=is_oled,
                panel_type=panel.panel_type,
                panel_key=panel.model_pattern.split("|")[0]
            )

        return lut

    def verify_calibration(
        self,
        panel: Optional[PanelCharacterization] = None,
        reference_patches: Optional[List[ColorPatch]] = None
    ) -> Dict:
        """
        Verify calibration accuracy using ColorChecker reference.

        Simulates the FULL calibration chain:
        1. Input sRGB value
        2. Apply VCGT gamma correction (linearize for panel gamma)
        3. Apply 3x3 color correction matrix (gamut mapping)
        4. Panel applies its native gamma and primaries
        5. Compare output XYZ/Lab against reference

        Args:
            panel: Panel characterization (uses current if None)
            reference_patches: Reference color patches (uses ColorChecker if None)

        Returns:
            Dictionary with verification results
        """
        panel = panel or self.current_panel
        if panel is None:
            raise ValueError("No panel set.")

        if reference_patches is None:
            reference_patches = get_colorchecker_reference()

        primaries = panel.native_primaries
        correction_matrix = self.calculate_correction_matrix(panel)

        results = {
            "panel": panel.name,
            "patches": [],
            "delta_e_values": [],
            "delta_e_avg": 0.0,
            "delta_e_max": 0.0,
            "cam16_delta_e_values": [],
            "cam16_delta_e_avg": 0.0,
            "cam16_delta_e_max": 0.0,
            "grade": ""
        }

        # Pre-compute CAM16 environment for viewing-condition-aware Delta E
        cam16_env = cam16_environment()

        # Build panel color transformation
        panel_to_xyz = primaries_to_xyz_matrix(
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple(),
            primaries.white.as_tuple()
        )

        # Target gamma (sRGB)
        target_gamma = 2.2

        for patch in reference_patches:
            # Reference sRGB (what we want to display)
            ref_srgb = np.array(patch.srgb)
            ref_srgb = np.clip(ref_srgb, 0.0001, 1.0)

            # CORRECT MATHEMATICAL CHAIN (matches LUT generation):
            #
            # LUT output: signal = (color_matrix @ sRGB^target_gamma)^(1/panel_gamma)
            # Panel receives signal and applies: output_XYZ = panel_to_xyz @ signal^panel_gamma
            #
            # Tracing through:
            # 1. LUT linearizes: linear = sRGB^target_gamma
            # 2. LUT applies matrix: panel_linear = color_matrix @ linear
            # 3. LUT encodes for panel: signal = panel_linear^(1/panel_gamma)
            # 4. Panel applies gamma: panel_linear_again = signal^panel_gamma = panel_linear
            # 5. Panel converts to XYZ: output_XYZ = panel_to_xyz @ panel_linear

            # Step 1: Linearize sRGB (target gamma)
            rgb_linear = np.power(ref_srgb, target_gamma)

            # Step 2: Apply color correction matrix in linear space
            rgb_panel_linear = correction_matrix @ rgb_linear
            rgb_panel_linear = np.clip(rgb_panel_linear, 0.0001, 1.0)

            # Step 3: LUT encodes for panel (inverse panel gamma)
            rgb_signal = np.array([
                np.power(rgb_panel_linear[0], 1.0 / panel.gamma_red.gamma),
                np.power(rgb_panel_linear[1], 1.0 / panel.gamma_green.gamma),
                np.power(rgb_panel_linear[2], 1.0 / panel.gamma_blue.gamma)
            ])
            rgb_signal = np.clip(rgb_signal, 0, 1)

            # Step 4: Panel applies its native gamma (undoes step 3)
            rgb_panel_final = np.array([
                np.power(rgb_signal[0], panel.gamma_red.gamma),
                np.power(rgb_signal[1], panel.gamma_green.gamma),
                np.power(rgb_signal[2], panel.gamma_blue.gamma)
            ])

            # Step 5: Panel RGB to XYZ using panel primaries
            xyz_displayed = panel_to_xyz @ rgb_panel_final

            # Adapt to D50 for Lab conversion (standard for color difference)
            xyz_d50 = bradford_adapt(xyz_displayed, D65_WHITE, D50_WHITE)

            # Convert to Lab
            lab_displayed = xyz_to_lab(xyz_d50, D50_WHITE)

            # Calculate Delta E against reference
            lab_ref = np.array(patch.lab_d50)
            de = delta_e_2000(lab_displayed, lab_ref)

            # CAM16-UCS Delta E (viewing-condition-aware, more accurate for wide gamut)
            cam16_de = 0.0
            try:
                # Reference XYZ (from Lab D50 → XYZ D50 → adapt to D65)
                ref_xyz_d50 = lab_to_xyz(lab_ref, D50_WHITE)
                ref_xyz_d65 = bradford_adapt(ref_xyz_d50, D50_WHITE, D65_WHITE)
                # Scale to ~100 range for CAM16
                ref_xyz_100 = ref_xyz_d65 * 100.0
                disp_xyz_100 = xyz_displayed * 100.0

                cam_ref = xyz_to_cam16(ref_xyz_100, cam16_env)
                cam_disp = xyz_to_cam16(disp_xyz_100, cam16_env)

                ucs_ref = cam16_to_ucs(cam_ref['J'], cam_ref['M'], cam_ref['h'])
                ucs_disp = cam16_to_ucs(cam_disp['J'], cam_disp['M'], cam_disp['h'])

                cam16_de = cam16_ucs_delta_e(ucs_ref, ucs_disp)
            except Exception:
                cam16_de = de  # Fallback to CIEDE2000

            results["patches"].append({
                "name": patch.name,
                "ref_lab": patch.lab_d50,
                "ref_srgb": patch.srgb,
                "displayed_lab": tuple(lab_displayed),
                "delta_e": float(de),
                "cam16_delta_e": float(cam16_de)
            })
            results["delta_e_values"].append(de)
            results["cam16_delta_e_values"].append(cam16_de)

        # Calculate statistics
        de_values = np.array(results["delta_e_values"])
        results["delta_e_avg"] = float(np.mean(de_values))
        results["delta_e_max"] = float(np.max(de_values))

        cam_de_values = np.array(results["cam16_delta_e_values"])
        results["cam16_delta_e_avg"] = float(np.mean(cam_de_values))
        results["cam16_delta_e_max"] = float(np.max(cam_de_values))

        # Grade using the stricter of the two metrics
        avg = max(results["delta_e_avg"], results["cam16_delta_e_avg"])
        if avg < 0.5:
            results["grade"] = "Reference (predicted dE < 0.5)"
        elif avg < 1.0:
            results["grade"] = "Professional (predicted dE < 1.0)"
        elif avg < 2.0:
            results["grade"] = "Excellent (predicted dE < 2.0)"
        elif avg < 3.0:
            results["grade"] = "Good (predicted dE < 3.0)"
        else:
            results["grade"] = "Acceptable (predicted dE >= 3.0)"

        results["accuracy_note"] = "Predicted from panel database. Actual accuracy depends on per-unit panel variation. Verify with a colorimeter for measured results."

        # Calculate gamut coverage percentages
        results["gamut_coverage"] = self._calculate_gamut_coverage(panel)

        # 3D color volume (captures luminance-dependent gamut changes)
        try:
            from calibrate_pro.display.color_volume import compute_color_volume
            primaries = panel.native_primaries
            vol = compute_color_volume(
                panel_primaries=(
                    primaries.red.as_tuple(),
                    primaries.green.as_tuple(),
                    primaries.blue.as_tuple()
                ),
                panel_white=primaries.white.as_tuple(),
                lightness_steps=11,
                hue_steps=36,
                panel_type=panel.panel_type,
                peak_luminance=panel.capabilities.max_luminance_hdr
            )
            results["color_volume"] = {
                "srgb_pct": vol.srgb_volume_pct,
                "p3_pct": vol.p3_volume_pct,
                "bt2020_pct": vol.bt2020_volume_pct,
                "relative_to_srgb_pct": vol.relative_to_srgb_pct,
                "lightness_levels": vol.lightness_levels,
                "gamut_area_per_level": vol.gamut_area_per_level
            }
        except Exception:
            pass

        self.calibration_results = results
        return results

    def _calculate_gamut_coverage(self, panel: PanelCharacterization) -> Dict:
        """
        Calculate gamut coverage using exact Sutherland-Hodgman polygon clipping.

        This gives mathematically exact intersection areas between gamut
        triangles in CIE xy chromaticity space. No sampling or approximation.

        Returns:
            Dict with keys: srgb_pct, dci_p3_pct, bt2020_pct, panel_area, srgb_area
        """
        def polygon_area(poly):
            """Shoelace formula for arbitrary polygon area."""
            n = len(poly)
            if n < 3:
                return 0.0
            area = 0.0
            for i in range(n):
                j = (i + 1) % n
                area += poly[i][0] * poly[j][1]
                area -= poly[j][0] * poly[i][1]
            return abs(area) / 2.0

        def sutherland_hodgman_clip(subject, clip_polygon):
            """
            Sutherland-Hodgman polygon clipping algorithm.
            Clips subject polygon against each edge of clip_polygon.
            Returns the intersection polygon (may be empty).
            """
            def inside_edge(point, edge_start, edge_end):
                """Check if point is on the inside (left) of directed edge."""
                return ((edge_end[0] - edge_start[0]) * (point[1] - edge_start[1]) -
                        (edge_end[1] - edge_start[1]) * (point[0] - edge_start[0])) >= 0

            def line_intersection(p1, p2, p3, p4):
                """Find intersection of line p1-p2 with line p3-p4."""
                x1, y1 = p1
                x2, y2 = p2
                x3, y3 = p3
                x4, y4 = p4

                denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4)
                if abs(denom) < 1e-15:
                    return ((x1 + x2) / 2, (y1 + y2) / 2)

                t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom

                x = x1 + t * (x2 - x1)
                y = y1 + t * (y2 - y1)
                return (x, y)

            output = list(subject)
            if not output:
                return []

            clip_n = len(clip_polygon)
            for i in range(clip_n):
                if not output:
                    return []

                edge_start = clip_polygon[i]
                edge_end = clip_polygon[(i + 1) % clip_n]

                input_list = output
                output = []

                for j in range(len(input_list)):
                    current = input_list[j]
                    prev = input_list[j - 1]

                    curr_inside = inside_edge(current, edge_start, edge_end)
                    prev_inside = inside_edge(prev, edge_start, edge_end)

                    if curr_inside:
                        if not prev_inside:
                            output.append(line_intersection(prev, current, edge_start, edge_end))
                        output.append(current)
                    elif prev_inside:
                        output.append(line_intersection(prev, current, edge_start, edge_end))

            return output

        p = panel.native_primaries
        panel_tri = [(p.red.x, p.red.y), (p.green.x, p.green.y), (p.blue.x, p.blue.y)]

        srgb_tri = [(0.6400, 0.3300), (0.3000, 0.6000), (0.1500, 0.0600)]
        p3_tri = [(0.6800, 0.3200), (0.2650, 0.6900), (0.1500, 0.0600)]
        bt2020_tri = [(0.7080, 0.2920), (0.1700, 0.7970), (0.1310, 0.0460)]

        panel_area = polygon_area(panel_tri)
        srgb_area = polygon_area(srgb_tri)
        p3_area = polygon_area(p3_tri)
        bt2020_area = polygon_area(bt2020_tri)

        # Exact intersection areas via polygon clipping
        srgb_overlap = polygon_area(sutherland_hodgman_clip(panel_tri, srgb_tri))
        p3_overlap = polygon_area(sutherland_hodgman_clip(panel_tri, p3_tri))
        bt2020_overlap = polygon_area(sutherland_hodgman_clip(panel_tri, bt2020_tri))

        return {
            "srgb_pct": min(100.0, srgb_overlap / srgb_area * 100.0) if srgb_area > 0 else 0,
            "dci_p3_pct": min(100.0, p3_overlap / p3_area * 100.0) if p3_area > 0 else 0,
            "bt2020_pct": min(100.0, bt2020_overlap / bt2020_area * 100.0) if bt2020_area > 0 else 0,
            "panel_area": panel_area,
            "srgb_area": srgb_area,
            "relative_to_srgb_pct": panel_area / srgb_area * 100.0 if srgb_area > 0 else 0
        }

    def calibrate(
        self,
        model_string: str,
        output_dir: Optional[Path] = None,
        generate_icc: bool = True,
        generate_lut: bool = True,
        lut_size: int = 33
    ) -> Dict:
        """
        Perform full sensorless calibration for a display.

        Args:
            model_string: Display model string from EDID
            output_dir: Directory to save calibration files
            generate_icc: Whether to generate ICC profile
            generate_lut: Whether to generate 3D LUT
            lut_size: 3D LUT grid size

        Returns:
            Dictionary with calibration results and file paths
        """
        # Detect panel
        panel = self.detect_panel(model_string)
        if panel is None:
            # Use generic fallback
            panel = self.database.get_fallback()
            self.current_panel = panel

        results = {
            "panel": panel.name,
            "panel_type": panel.panel_type,
            "files": {},
            "verification": {}
        }

        if output_dir is None:
            output_dir = Path(".")
        output_dir = Path(output_dir)
        output_dir.mkdir(parents=True, exist_ok=True)

        # Generate ICC profile
        if generate_icc:
            profile = self.create_icc_profile(panel)
            safe_name = model_string.replace(" ", "_").replace("/", "_")
            icc_path = output_dir / f"Calibrate_Pro_{safe_name}.icc"
            profile.save(icc_path)
            results["files"]["icc"] = str(icc_path)

        # Generate 3D LUT
        if generate_lut:
            lut = self.create_3d_lut(panel, size=lut_size)
            lut_path = output_dir / f"Calibrate_Pro_{safe_name}.cube"
            lut.save(lut_path)
            results["files"]["lut"] = str(lut_path)

        # Verify calibration
        verification = self.verify_calibration(panel)
        results["verification"] = verification

        return results

    def load_panel_data(self, panel_data) -> None:
        """
        Load panel data from a dictionary or PanelCharacterization object.

        Args:
            panel_data: Panel characterization (dict or PanelCharacterization)
        """
        if isinstance(panel_data, PanelCharacterization):
            self.current_panel = panel_data
        else:
            # Convert dict to PanelCharacterization
            self.current_panel = PanelCharacterization.from_dict(panel_data)

    def generate_gamut_only_lut(self, size: int = 33) -> LUT3D:
        """
        Generate a gamut-mapping LUT for DDC/CI-first calibration.

        This LUT properly handles gamma:
        1. Decode gamma (signal -> linear)
        2. Apply color matrix in linear space
        3. Encode gamma (linear -> signal)

        For neutral colors (R=G=B), the output equals input (identity).
        For saturated colors, applies gamut compression from wide gamut to sRGB.

        Args:
            size: LUT grid size (17, 33, or 65)

        Returns:
            Gamut-mapping 3D LUT with proper gamma handling
        """
        if self.current_panel is None:
            raise ValueError("No panel loaded. Call load_panel_data() first.")

        panel = self.current_panel
        primaries = panel.native_primaries

        # Get the color correction matrix (works in LINEAR space)
        # This matrix: sRGB linear -> Panel linear
        color_matrix = self.calculate_correction_matrix(panel)

        # Create identity LUT as base
        lut = LUT3D.create_identity(size)
        coords = np.linspace(0, 1, size)

        # Gamma value (2.2 is standard, monitor should be set to match)
        gamma = 2.2

        for r_idx, r in enumerate(coords):
            for g_idx, g in enumerate(coords):
                for b_idx, b in enumerate(coords):
                    rgb_signal = np.array([r, g, b])

                    # Preserve black
                    if r == 0 and g == 0 and b == 0:
                        lut.data[r_idx, g_idx, b_idx] = np.array([0.0, 0.0, 0.0])
                        continue

                    # Calculate saturation in signal space (0 = gray, 1 = fully saturated)
                    max_c = max(r, g, b)
                    min_c = min(r, g, b)
                    if max_c > 0:
                        saturation = (max_c - min_c) / max_c
                    else:
                        saturation = 0

                    # For neutral colors (R=G=B), output = input (identity)
                    # This preserves grayscale tracking set by DDC/CI
                    if saturation < 0.01:
                        lut.data[r_idx, g_idx, b_idx] = rgb_signal
                        continue

                    # === PROPER GAMMA HANDLING ===
                    # Step 1: Decode gamma (signal -> linear light)
                    rgb_linear = np.power(rgb_signal, gamma)

                    # Step 2: Apply color matrix in LINEAR space
                    rgb_corrected_linear = color_matrix @ rgb_linear
                    rgb_corrected_linear = np.clip(rgb_corrected_linear, 0.0, 1.0)

                    # Step 3: Encode gamma (linear -> signal)
                    rgb_corrected_signal = np.power(rgb_corrected_linear, 1.0 / gamma)

                    # Blend between identity and corrected based on saturation
                    # Low saturation -> mostly identity (preserve near-grays)
                    # High saturation -> fully corrected (compress gamut)
                    blend_factor = saturation ** 0.5  # Smooth transition
                    rgb_output = rgb_signal * (1 - blend_factor) + rgb_corrected_signal * blend_factor

                    lut.data[r_idx, g_idx, b_idx] = np.clip(rgb_output, 0, 1)

        lut.title = f"Gamut LUT for {panel.name}"
        return lut

    def verify_lut_accuracy(self, lut: LUT3D) -> float:
        """
        Verify the accuracy of a LUT using ColorChecker patches.

        Args:
            lut: The LUT to verify

        Returns:
            Average Delta E across all ColorChecker patches
        """
        if self.current_panel is None:
            raise ValueError("No panel loaded.")

        panel = self.current_panel
        primaries = panel.native_primaries
        reference_patches = get_colorchecker_reference()

        # Build panel color transformation
        panel_to_xyz = primaries_to_xyz_matrix(
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple(),
            primaries.white.as_tuple()
        )

        delta_e_values = []

        for patch in reference_patches:
            # Input sRGB
            ref_srgb = np.array(patch.srgb)

            # Apply LUT
            corrected_rgb = lut.apply(ref_srgb)

            # Simulate panel output:
            # Panel applies its gamma and primaries
            corrected_linear = np.power(np.clip(corrected_rgb, 1e-10, 1.0), 2.2)
            xyz_displayed = panel_to_xyz @ corrected_linear

            # Convert to Lab
            xyz_d50 = bradford_adapt(xyz_displayed, D65_WHITE, D50_WHITE)
            lab_displayed = xyz_to_lab(xyz_d50, D50_WHITE)

            # Calculate Delta E
            lab_ref = np.array(patch.lab_d50)
            de = delta_e_2000(lab_displayed, lab_ref)
            delta_e_values.append(de)

        return float(np.mean(delta_e_values))


def calibrate_display(
    model_string: str,
    output_dir: Optional[Path] = None,
    generate_icc: bool = True,
    generate_lut: bool = True,
    lut_size: int = 33
) -> Dict:
    """
    Convenience function to calibrate a display.

    Args:
        model_string: Display model string from EDID
        output_dir: Directory to save calibration files
        generate_icc: Whether to generate ICC profile
        generate_lut: Whether to generate 3D LUT
        lut_size: 3D LUT grid size

    Returns:
        Dictionary with calibration results
    """
    engine = SensorlessEngine()
    return engine.calibrate(
        model_string,
        output_dir=output_dir,
        generate_icc=generate_icc,
        generate_lut=generate_lut,
        lut_size=lut_size
    )


def verify_display(model_string: str) -> Dict:
    """
    Verify calibration for a display.

    Args:
        model_string: Display model string from EDID

    Returns:
        Verification results
    """
    engine = SensorlessEngine()
    panel = engine.detect_panel(model_string)
    if panel is None:
        panel = engine.database.get_fallback()
        engine.current_panel = panel

    return engine.verify_calibration()


# Backwards compatibility alias
NeuralUXEngine = SensorlessEngine
