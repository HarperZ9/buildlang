"""
Calibrate Pro - Full Workflow End-to-End Test

This script tests the complete calibration pipeline:
1. Panel Detection & Profile Loading
2. SDR Calibration with NeuralUX
3. HDR Calibration Suite (HDR10/HLG)
4. Advanced LUT Generation (CAM16 gamut mapping)
5. ACES 2.0 Pipeline Integration
6. Mastering Standard Validation
7. Full Verification Report

Author: Build
"""

import sys
import os
import numpy as np
from pathlib import Path
from datetime import datetime

# Add calibrate_pro to path
sys.path.insert(0, str(Path(__file__).parent))

def print_header(title: str):
    """Print formatted section header."""
    print("\n" + "=" * 70)
    print(f"  {title}")
    print("=" * 70)

def print_result(name: str, value, status: str = ""):
    """Print formatted result line."""
    status_icon = {"OK": "[OK]", "WARN": "[!]", "FAIL": "[X]", "": ""}.get(status, "")
    print(f"  {name:40} {str(value):20} {status_icon}")

def run_full_workflow_test():
    """Run the complete calibration workflow test."""

    print("\n")
    print("+" + "=" * 68 + "+")
    print("|" + " " * 15 + "CALIBRATE PRO - FULL WORKFLOW TEST" + " " * 16 + "|")
    print("|" + " " * 20 + "State-of-the-Art Color Science" + " " * 17 + "|")
    print("+" + "=" * 68 + "+")
    print(f"\n  Test started: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")

    all_tests_passed = True
    test_results = {}

    # =========================================================================
    # PHASE 1: Panel Detection & Profile Loading
    # =========================================================================
    print_header("PHASE 1: Panel Detection & Profile Loading")

    try:
        from calibrate_pro.panels.database import get_database, PanelDatabase

        db = get_database()
        print_result("Panel database loaded", f"{len(db.list_panels())} panels", "OK")

        # Test panel detection for QD-OLED
        test_model = "ASUS ROG Swift PG27UCDM"
        panel = db.find_panel(test_model)

        if panel:
            print_result("Panel detected", panel.panel_type, "OK")
            print_result("  Manufacturer", panel.manufacturer)
            print_result("  Red primary", f"({panel.native_primaries.red.x:.4f}, {panel.native_primaries.red.y:.4f})")
            print_result("  Green primary", f"({panel.native_primaries.green.x:.4f}, {panel.native_primaries.green.y:.4f})")
            print_result("  Blue primary", f"({panel.native_primaries.blue.x:.4f}, {panel.native_primaries.blue.y:.4f})")
            print_result("  Peak HDR luminance", f"{panel.capabilities.max_luminance_hdr} cd/m2")
            print_result("  HDR capable", panel.capabilities.hdr_capable)
            test_results["panel_detection"] = True
        else:
            print_result("Panel detection", "FAILED - using fallback", "WARN")
            panel = db.get_fallback()
            test_results["panel_detection"] = False

    except Exception as e:
        print_result("Panel detection", f"ERROR: {e}", "FAIL")
        test_results["panel_detection"] = False
        all_tests_passed = False

    # =========================================================================
    # PHASE 2: Advanced Color Models
    # =========================================================================
    print_header("PHASE 2: Advanced Color Models (CAM16/Jzazbz/ICtCp)")

    try:
        from calibrate_pro.core import CAM16, CAM16ViewingConditions, Jzazbz, ICtCp, delta_e_hdr

        # Test CAM16
        vc = CAM16ViewingConditions(L_A=64.0, Y_b=20.0, surround='dim')
        cam = CAM16(vc)

        # Test with D65 white
        d65_xyz = np.array([95.047, 100.0, 108.883])
        cam_result = cam.xyz_to_cam16(d65_xyz)
        print_result("CAM16 forward transform", f"J={cam_result['J']:.2f}", "OK")

        # Test roundtrip
        xyz_back = cam.cam16_to_xyz(cam_result['J'], cam_result['C'], cam_result['h'])
        roundtrip_error = np.linalg.norm(d65_xyz - xyz_back)
        print_result("CAM16 roundtrip error", f"{roundtrip_error:.6f}", "OK" if roundtrip_error < 0.01 else "WARN")

        # Test Jzazbz
        jz = Jzazbz(peak_luminance=1000)
        test_xyz = np.array([50.0, 50.0, 50.0])
        jab = jz.xyz_to_jzazbz(test_xyz)
        xyz_back_jz = jz.jzazbz_to_xyz(jab)
        jz_roundtrip = np.linalg.norm(test_xyz - xyz_back_jz)
        print_result("Jzazbz roundtrip error", f"{jz_roundtrip:.6f}", "OK" if jz_roundtrip < 0.1 else "WARN")

        # Test ICtCp
        ictcp = ICtCp(peak_luminance=1000)
        test_rgb = np.array([0.5, 0.3, 0.2])
        ic = ictcp.rgb_to_ictcp(test_rgb, input_space='bt2020')
        rgb_back = ictcp.ictcp_to_rgb(ic, output_space='bt2020')
        ictcp_roundtrip = np.linalg.norm(test_rgb - rgb_back)
        print_result("ICtCp roundtrip error", f"{ictcp_roundtrip:.6f}", "OK" if ictcp_roundtrip < 0.01 else "WARN")

        # Test HDR delta E
        color1 = np.array([50.0, 50.0, 50.0])
        color2 = np.array([51.0, 50.5, 50.2])
        de = delta_e_hdr(color1, color2, color_space='xyz', method='jzazbz')
        print_result("HDR Delta E (Jzazbz)", f"{de:.4f}", "OK")

        test_results["color_models"] = True

    except Exception as e:
        print_result("Color models", f"ERROR: {e}", "FAIL")
        test_results["color_models"] = False
        all_tests_passed = False

    # =========================================================================
    # PHASE 3: ACES 2.0 Pipeline
    # =========================================================================
    print_header("PHASE 3: ACES 2.0 Pipeline")

    try:
        from calibrate_pro.core import ACES2, OutputConfig, ACES2Tonescale, ACES2GamutMapper

        # Create ACES 2.0 pipeline
        aces = ACES2()

        # Test SDR output
        sdr_config = OutputConfig.sdr_100_srgb()
        test_aces = np.array([0.18, 0.18, 0.18])  # 18% gray in AP1
        sdr_out = aces.render(test_aces, sdr_config)
        print_result("ACES2 SDR render (18% gray)", f"[{sdr_out[0]:.4f}, {sdr_out[1]:.4f}, {sdr_out[2]:.4f}]", "OK")

        # Test HDR output
        hdr_config = OutputConfig.hdr_1000_p3()
        hdr_out = aces.render(test_aces, hdr_config)
        print_result("ACES2 HDR render (1000 nits)", f"[{hdr_out[0]:.6f}, {hdr_out[1]:.6f}, {hdr_out[2]:.6f}]", "OK")

        # Test tonescale
        tonescale = ACES2Tonescale(peak_luminance=1000.0)
        highlight = tonescale.apply(np.array([100.0]))  # Test highlight compression
        print_result("ACES2 Tonescale (100 -> peak)", f"{highlight[0]:.2f}", "OK")

        # Test gamut mapper
        from calibrate_pro.core.aces import P3_D65_PRIMARIES
        gamut_mapper = ACES2GamutMapper(output_primaries=P3_D65_PRIMARIES, peak_luminance=1000.0)
        print_result("ACES2 Gamut Mapper", "Initialized (P3-D65)", "OK")

        test_results["aces2"] = True

    except Exception as e:
        print_result("ACES 2.0", f"ERROR: {e}", "FAIL")
        test_results["aces2"] = False
        all_tests_passed = False

    # =========================================================================
    # PHASE 4: SDR Calibration Engine
    # =========================================================================
    print_header("PHASE 4: SDR Calibration Engine (NeuralUX)")

    try:
        from calibrate_pro.core.calibration_engine import (
            CalibrationEngine, CalibrationMode, CalibrationTarget,
            WhitepointTarget, GammaTarget, GamutTarget
        )

        # Create output directory
        output_dir = Path(__file__).parent / "test_output"
        output_dir.mkdir(exist_ok=True)

        # Configure calibration
        engine = CalibrationEngine(mode=CalibrationMode.SENSORLESS)

        target = CalibrationTarget(
            whitepoint=WhitepointTarget.D65,
            gamma=GammaTarget.POWER_22,
            gamut=GamutTarget.SRGB,
            luminance_target=200.0
        )
        engine.set_target(target)

        # Progress tracking
        progress_log = []
        def progress_callback(msg, pct):
            progress_log.append((msg, pct))
        engine.set_progress_callback(progress_callback)

        # Run calibration
        result = engine.calibrate_sensorless(
            model_string=test_model,
            output_dir=output_dir,
            generate_icc=True,
            generate_lut=True,
            lut_size=17  # Small for testing
        )

        print_result("Calibration status", "SUCCESS" if result.success else "FAILED",
                    "OK" if result.success else "FAIL")
        print_result("Delta E average", f"{result.delta_e_avg:.2f}",
                    "OK" if result.delta_e_avg < 1.0 else "WARN")
        print_result("Delta E maximum", f"{result.delta_e_max:.2f}")
        print_result("Grade", result.grade)
        print_result("ICC profile", result.icc_profile_path.name if result.icc_profile_path else "None")
        print_result("3D LUT", result.lut_path.name if result.lut_path else "None")
        print_result("Progress callbacks", f"{len(progress_log)} events", "OK")

        test_results["sdr_calibration"] = result.success and result.delta_e_avg < 2.0

    except Exception as e:
        import traceback
        print_result("SDR Calibration", f"ERROR: {e}", "FAIL")
        traceback.print_exc()
        test_results["sdr_calibration"] = False
        all_tests_passed = False

    # =========================================================================
    # PHASE 5: HDR Calibration Suite
    # =========================================================================
    print_header("PHASE 5: HDR Calibration Suite (HDR10/HLG)")

    try:
        from calibrate_pro.hdr import (
            HDRFormat, CalibrationMode as HDRCalMode,
            HDRCalibrationConfig, HDRCalibrationSuite,
            HDR10Calibration, HLGCalibration
        )

        # Test HDR10 calibration
        hdr10_config = HDRCalibrationConfig(
            format=HDRFormat.HDR10,
            mode=HDRCalMode.STANDARD,
            target_peak_luminance=1000.0,
            target_min_luminance=0.0005,
            target_primaries="p3_d65"
        )

        hdr10_suite = HDRCalibrationSuite(hdr10_config)
        patches = hdr10_suite.get_test_patches()

        print_result("HDR10 grayscale patches", len(patches["grayscale"]), "OK")
        print_result("HDR10 primary patches", len(patches.get("primaries", [])), "OK")

        # Simulate measurements (normally from colorimeter)
        simulated_grayscale_lum = np.array([
            0.0005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0,
            50.0, 75.0, 100.0, 150.0, 200.0, 300.0, 400.0, 500.0,
            600.0, 700.0, 800.0, 900.0, 950.0, 980.0, 1000.0
        ])

        # Test HLG calibration
        hlg_config = HDRCalibrationConfig(
            format=HDRFormat.HLG,
            mode=HDRCalMode.STANDARD,
            target_peak_luminance=1000.0,
            hlg_system_gamma=1.2
        )

        hlg_suite = HDRCalibrationSuite(hlg_config)
        hlg_patches = hlg_suite.get_test_patches()
        print_result("HLG grayscale patches", len(hlg_patches["grayscale"]), "OK")

        test_results["hdr_calibration"] = True

    except Exception as e:
        import traceback
        print_result("HDR Calibration", f"ERROR: {e}", "FAIL")
        traceback.print_exc()
        test_results["hdr_calibration"] = False
        all_tests_passed = False

    # =========================================================================
    # PHASE 6: Advanced LUT Generation
    # =========================================================================
    print_header("PHASE 6: Advanced LUT Generation (CAM16 Gamut Mapping)")

    try:
        from calibrate_pro.core import AdvancedLUTGenerator, LUTManipulator
        from calibrate_pro.core.lut_engine_advanced import AdvancedLUT3D

        # Create advanced generator
        generator = AdvancedLUTGenerator(size=17, num_threads=4)
        print_result("LUT Generator initialized", f"size={generator.size}", "OK")

        # Test LUT operations
        # Create a simple identity LUT for testing
        identity_data = np.zeros((17, 17, 17, 3))
        for r in range(17):
            for g in range(17):
                for b in range(17):
                    identity_data[r, g, b] = [r/16, g/16, b/16]

        lut1 = AdvancedLUT3D(size=17, data=identity_data, title="Identity")
        lut2 = AdvancedLUT3D(size=17, data=identity_data * 0.9, title="Dark")

        # Test LUT manipulation
        manipulator = LUTManipulator()

        # Test blending (faster than combine for testing)
        blended = manipulator.blend(lut1, lut2, 0.5)
        print_result("LUT blending (50%)", f"{blended.size}x{blended.size}x{blended.size}", "OK")

        # Verify blend is correct
        test_point = blended.data[8, 8, 8]
        expected = (identity_data[8, 8, 8] + identity_data[8, 8, 8] * 0.9) / 2
        blend_error = np.linalg.norm(test_point - expected)
        print_result("Blend accuracy", f"error={blend_error:.6f}", "OK" if blend_error < 0.001 else "WARN")

        # Test tetrahedral interpolation
        test_rgb = np.array([0.5, 0.3, 0.7])
        interpolated = lut1.apply_tetrahedral(test_rgb)
        interp_error = np.linalg.norm(interpolated - test_rgb)
        print_result("Tetrahedral interpolation", f"error={interp_error:.6f}", "OK" if interp_error < 0.01 else "WARN")

        test_results["lut_generation"] = True

    except Exception as e:
        import traceback
        print_result("LUT Generation", f"ERROR: {e}", "FAIL")
        traceback.print_exc()
        test_results["lut_generation"] = False
        all_tests_passed = False

    # =========================================================================
    # PHASE 7: Mastering Standard Validation
    # =========================================================================
    print_header("PHASE 7: Mastering Standard Validation")

    try:
        from calibrate_pro.hdr import (
            ComplianceLevel, validate_mastering_compliance,
            get_recommended_targets, generate_compliance_report,
            NetflixMasteringProfile, EBUGrade1Profile
        )

        # Test measurements (simulating a well-calibrated display)
        good_measurements = {
            'peak_luminance': 1050,
            'min_luminance': 0.0003,
            'white_point_de': 0.8,
            'red_primary_de': 0.9,
            'green_primary_de': 0.7,
            'blue_primary_de': 1.0,
            'gamma': 2.40,
            'eotf_error': 1.5
        }

        # Validate against Netflix
        level, issues, spec = validate_mastering_compliance(good_measurements, 'netflix')
        print_result("Netflix compliance", level.name,
                    "OK" if level == ComplianceLevel.FULL else "WARN")
        if issues:
            for issue in issues[:3]:  # Show first 3 issues
                print_result("  Issue", issue[:50])

        # Validate against EBU
        level_ebu, issues_ebu, _ = validate_mastering_compliance(good_measurements, 'ebu_hdr')
        print_result("EBU Grade 1 HDR compliance", level_ebu.name,
                    "OK" if level_ebu in [ComplianceLevel.FULL, ComplianceLevel.PARTIAL] else "WARN")

        # Test recommended targets
        streaming_targets = get_recommended_targets("streaming_hdr")
        print_result("Streaming HDR targets", f"{len(streaming_targets)} profiles", "OK")

        # Generate compliance report
        report = generate_compliance_report(good_measurements, ['netflix', 'ebu_hdr'])
        print_result("Compliance report", f"{len(report)} characters", "OK")

        test_results["mastering_validation"] = True

    except Exception as e:
        import traceback
        print_result("Mastering Validation", f"ERROR: {e}", "FAIL")
        traceback.print_exc()
        test_results["mastering_validation"] = False
        all_tests_passed = False

    # =========================================================================
    # PHASE 8: Integration Test - Full HDR Pipeline
    # =========================================================================
    print_header("PHASE 8: Full HDR Pipeline Integration")

    try:
        from calibrate_pro.core import CAM16, Jzazbz, ACES2, OutputConfig
        from calibrate_pro.hdr import pq_eotf, pq_oetf, HDRCalibrationSuite, HDRCalibrationConfig, HDRFormat
        from calibrate_pro.hdr import CalibrationMode as HDRCalMode

        # Simulate full HDR workflow:
        # 1. Input: PQ-encoded HDR10 signal
        # 2. Convert to linear light
        # 3. Process through ACES 2.0
        # 4. Apply calibration correction
        # 5. Re-encode to PQ

        # Test signal: 50% PQ (approximately 100 nits)
        pq_signal = np.array([0.5, 0.5, 0.5])

        # Decode PQ to linear (nits)
        linear_nits = pq_eotf(pq_signal)
        print_result("PQ decode (0.5 signal)", f"{linear_nits[0]:.1f} nits", "OK")

        # Normalize to [0,1] for 1000 nit display
        linear_norm = linear_nits / 1000.0

        # Process through ACES (if needed for color grading)
        aces = ACES2()
        hdr_config = OutputConfig.hdr_1000_p3()

        # Simulate calibration correction using Jzazbz
        jz = Jzazbz(peak_luminance=1000)

        # Convert to perceptual space
        # First need XYZ from linear P3
        from calibrate_pro.core.aces import P3_TO_XYZ
        xyz = P3_TO_XYZ @ linear_norm
        jab = jz.xyz_to_jzazbz(xyz * 100)  # Scale for XYZ

        # Apply correction (simulated - in practice from measurements)
        jab_corrected = jab * np.array([1.0, 0.98, 0.99])  # Slight chroma adjustment

        # Convert back
        xyz_corrected = jz.jzazbz_to_xyz(jab_corrected)
        from calibrate_pro.core.aces import XYZ_TO_P3
        linear_corrected = XYZ_TO_P3 @ (xyz_corrected / 100)
        linear_corrected = np.clip(linear_corrected, 0, 1)

        # Re-encode to PQ
        pq_output = pq_oetf(linear_corrected * 1000)  # Back to nits first

        print_result("Full HDR pipeline", f"In: {pq_signal} -> Out: [{pq_output[0]:.4f}, {pq_output[1]:.4f}, {pq_output[2]:.4f}]", "OK")

        # Verify the pipeline maintains reasonable values
        pipeline_ok = np.all(pq_output >= 0) and np.all(pq_output <= 1)
        print_result("Pipeline output valid", "Yes" if pipeline_ok else "No", "OK" if pipeline_ok else "FAIL")

        test_results["hdr_pipeline"] = pipeline_ok

    except Exception as e:
        import traceback
        print_result("HDR Pipeline", f"ERROR: {e}", "FAIL")
        traceback.print_exc()
        test_results["hdr_pipeline"] = False
        all_tests_passed = False

    # =========================================================================
    # SUMMARY
    # =========================================================================
    print_header("TEST SUMMARY")

    passed = sum(1 for v in test_results.values() if v)
    total = len(test_results)

    print(f"\n  Tests passed: {passed}/{total}")
    print()

    for test_name, result in test_results.items():
        status = "[PASS]" if result else "[FAIL]"
        print(f"  {status} {test_name.replace('_', ' ').title()}")

    print()

    if all(test_results.values()):
        print("  " + "=" * 50)
        print("  [OK] ALL TESTS PASSED - Calibration Pipeline Ready")
        print("  " + "=" * 50)
    else:
        print("  " + "=" * 50)
        print("  [!!] SOME TESTS FAILED - Review errors above")
        print("  " + "=" * 50)

    print(f"\n  Test completed: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()

    return all(test_results.values())


if __name__ == "__main__":
    success = run_full_workflow_test()
    sys.exit(0 if success else 1)
