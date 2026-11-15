"""Tests for calibrate_pro.sensorless.neuralux — NeuralUXEngine.verify_calibration."""

import pytest

from calibrate_pro.sensorless.neuralux import NeuralUXEngine
from calibrate_pro.panels.database import PanelDatabase


# -------------------------------------------------------------------------
# verify_calibration returns expected keys
# -------------------------------------------------------------------------

def test_verify_returns_expected_keys(panel_database, qd_oled_panel):
    """verify_calibration result dict should have the documented keys."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = qd_oled_panel
    result = engine.verify_calibration(qd_oled_panel)

    expected_keys = [
        "panel", "patches", "delta_e_values", "delta_e_avg", "delta_e_max",
        "cam16_delta_e_values", "cam16_delta_e_avg", "cam16_delta_e_max",
        "grade", "gamut_coverage",
    ]
    for key in expected_keys:
        assert key in result, f"Missing key: {key}"


# -------------------------------------------------------------------------
# CIEDE2000 and CAM16-UCS metrics are both present
# -------------------------------------------------------------------------

def test_verify_has_ciede2000_metrics(panel_database, qd_oled_panel):
    """Verification should include CIEDE2000 Delta E values."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = qd_oled_panel
    result = engine.verify_calibration(qd_oled_panel)

    assert len(result["delta_e_values"]) > 0
    assert result["delta_e_avg"] >= 0.0
    assert result["delta_e_max"] >= result["delta_e_avg"]


def test_verify_has_cam16_metrics(panel_database, qd_oled_panel):
    """Verification should include CAM16-UCS Delta E values."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = qd_oled_panel
    result = engine.verify_calibration(qd_oled_panel)

    assert len(result["cam16_delta_e_values"]) > 0
    assert result["cam16_delta_e_avg"] >= 0.0
    assert result["cam16_delta_e_max"] >= result["cam16_delta_e_avg"]


# -------------------------------------------------------------------------
# Gamut coverage (QD-OLED should be >95% sRGB)
# -------------------------------------------------------------------------

def test_qd_oled_srgb_coverage(panel_database, qd_oled_panel):
    """QD-OLED panel should have >95% sRGB gamut coverage."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = qd_oled_panel
    result = engine.verify_calibration(qd_oled_panel)

    coverage = result["gamut_coverage"]
    assert "srgb_pct" in coverage
    assert coverage["srgb_pct"] > 95.0, (
        f"QD-OLED sRGB coverage should be >95%, got {coverage['srgb_pct']:.1f}%"
    )


def test_generic_srgb_coverage(panel_database, srgb_panel):
    """Generic sRGB panel should have ~100% sRGB coverage."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = srgb_panel
    result = engine.verify_calibration(srgb_panel)

    coverage = result["gamut_coverage"]
    assert coverage["srgb_pct"] > 90.0


# -------------------------------------------------------------------------
# Grade assignment matches Delta E thresholds
# -------------------------------------------------------------------------

def test_grade_reference(panel_database, qd_oled_panel):
    """Grade should be based on Delta E thresholds."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = qd_oled_panel
    result = engine.verify_calibration(qd_oled_panel)

    grade = result["grade"]
    avg = max(result["delta_e_avg"], result["cam16_delta_e_avg"])

    grade_lower = grade.lower()
    if avg < 0.5:
        assert "reference" in grade_lower
    elif avg < 1.0:
        assert "professional" in grade_lower
    elif avg < 2.0:
        assert "excellent" in grade_lower
    elif avg < 3.0:
        assert "good" in grade_lower
    else:
        assert "acceptable" in grade_lower


def test_srgb_panel_grade(panel_database, srgb_panel):
    """Generic sRGB panel should produce a valid grade string."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = srgb_panel
    result = engine.verify_calibration(srgb_panel)

    assert isinstance(result["grade"], str)
    assert len(result["grade"]) > 0


# -------------------------------------------------------------------------
# Per-patch data
# -------------------------------------------------------------------------

def test_patches_have_correct_structure(panel_database, qd_oled_panel):
    """Each patch entry should have name, ref_lab, delta_e, cam16_delta_e."""
    engine = NeuralUXEngine(panel_database=panel_database)
    engine.current_panel = qd_oled_panel
    result = engine.verify_calibration(qd_oled_panel)

    assert len(result["patches"]) == 24  # ColorChecker Classic has 24 patches
    for patch in result["patches"]:
        assert "name" in patch
        assert "ref_lab" in patch
        assert "delta_e" in patch
        assert "cam16_delta_e" in patch
        assert patch["delta_e"] >= 0.0
