"""Tests for calibrate_pro.sensorless.auto_calibration — AutoCalibrationEngine."""

import struct
import tempfile
from pathlib import Path

import numpy as np
import pytest

from calibrate_pro.sensorless.auto_calibration import (
    AutoCalibrationEngine,
    AutoCalibrationResult,
    CalibrationTarget,
    CalibrationRisk,
    UserConsent,
)


# -------------------------------------------------------------------------
# AutoCalibrationEngine can be instantiated
# -------------------------------------------------------------------------

def test_engine_instantiation():
    """AutoCalibrationEngine should instantiate without errors."""
    engine = AutoCalibrationEngine()
    assert engine is not None
    assert engine._consent is None
    assert engine._result is None


def test_engine_has_progress_callback():
    """Engine should accept a progress callback."""
    engine = AutoCalibrationEngine()
    calls = []
    engine.set_progress_callback(lambda msg, prog, step: calls.append((msg, prog, step)))
    assert engine._progress_callback is not None


# -------------------------------------------------------------------------
# _extract_edid_chromaticity with synthetic EDID bytes
# -------------------------------------------------------------------------

def _build_synthetic_edid(
    red_x=0.640, red_y=0.330,
    green_x=0.300, green_y=0.600,
    blue_x=0.150, blue_y=0.060,
    white_x=0.3127, white_y=0.3290,
):
    """Build synthetic EDID bytes (at least 35 bytes) with chromaticity data.

    EDID chromaticity encoding:
      Bytes 25-26: packed low 2 bits for R, G, B, W (x and y)
      Bytes 27-34: high 8 bits for Rx, Ry, Gx, Gy, Bx, By, Wx, Wy
    """
    def encode_10bit(val):
        """Encode a chromaticity value [0, 1] as 10-bit integer."""
        return int(round(val * 1024)) & 0x3FF

    rx = encode_10bit(red_x)
    ry = encode_10bit(red_y)
    gx = encode_10bit(green_x)
    gy = encode_10bit(green_y)
    bx = encode_10bit(blue_x)
    by = encode_10bit(blue_y)
    wx = encode_10bit(white_x)
    wy = encode_10bit(white_y)

    # Low bits packed into bytes 25-26
    # Byte 25: RxL(7:6) RyL(5:4) GxL(3:2) GyL(1:0)
    byte25 = (
        ((rx & 0x03) << 6) |
        ((ry & 0x03) << 4) |
        ((gx & 0x03) << 2) |
        (gy & 0x03)
    )
    # Byte 26: BxL(7:6) ByL(5:4) WxL(3:2) WyL(1:0)
    byte26 = (
        ((bx & 0x03) << 6) |
        ((by & 0x03) << 4) |
        ((wx & 0x03) << 2) |
        (wy & 0x03)
    )

    # High bytes (upper 8 bits of each 10-bit value)
    high_bytes = [
        rx >> 2, ry >> 2,
        gx >> 2, gy >> 2,
        bx >> 2, by >> 2,
        wx >> 2, wy >> 2,
    ]

    # Build EDID: 25 padding bytes, then our chromaticity data
    edid = bytearray(25)  # bytes 0-24: padding
    edid.append(byte25)   # byte 25
    edid.append(byte26)   # byte 26
    edid.extend(high_bytes)  # bytes 27-34
    return bytes(edid)


def test_extract_edid_chromaticity_srgb():
    """Extracting sRGB chromaticity from synthetic EDID should match input."""
    edid = _build_synthetic_edid()
    result = AutoCalibrationEngine._extract_edid_chromaticity(edid)

    assert result is not None
    assert result["red"][0] == pytest.approx(0.640, abs=0.002)
    assert result["red"][1] == pytest.approx(0.330, abs=0.002)
    assert result["green"][0] == pytest.approx(0.300, abs=0.002)
    assert result["green"][1] == pytest.approx(0.600, abs=0.002)
    assert result["blue"][0] == pytest.approx(0.150, abs=0.002)
    assert result["blue"][1] == pytest.approx(0.060, abs=0.002)
    assert result["white"][0] == pytest.approx(0.3127, abs=0.002)
    assert result["white"][1] == pytest.approx(0.3290, abs=0.002)


def test_extract_edid_chromaticity_wide_gamut():
    """Extracting QD-OLED-like chromaticity from synthetic EDID."""
    edid = _build_synthetic_edid(
        red_x=0.680, red_y=0.310,
        green_x=0.233, green_y=0.711,
        blue_x=0.138, blue_y=0.050,
    )
    result = AutoCalibrationEngine._extract_edid_chromaticity(edid)
    assert result is not None
    assert result["red"][0] == pytest.approx(0.680, abs=0.002)
    assert result["green"][1] == pytest.approx(0.711, abs=0.002)


def test_extract_edid_too_short():
    """EDID shorter than 35 bytes should return None."""
    result = AutoCalibrationEngine._extract_edid_chromaticity(b"\x00" * 20)
    assert result is None


# -------------------------------------------------------------------------
# _match_panel finds known panels
# -------------------------------------------------------------------------

def test_match_panel_pg27ucdm():
    """_match_panel should find PG27UCDM from display info."""
    engine = AutoCalibrationEngine()
    display_info = {"name": "PG27UCDM", "manufacturer": "ASUS", "model": "PG27UCDM"}
    panel = engine._match_panel(display_info)
    assert panel is not None
    assert panel.panel_type == "QD-OLED"


def test_match_panel_fallback():
    """_match_panel should fall back to GENERIC_SRGB for unknown displays."""
    engine = AutoCalibrationEngine()
    display_info = {"name": "Unknown XYZ-999", "manufacturer": "NoName"}
    panel = engine._match_panel(display_info)
    assert panel is not None
    # It should be EITHER a panel created from EDID or the generic fallback
    # Since there's no device_id, it'll be the generic fallback
    assert panel.manufacturer in ("Generic", "Unknown", "NoName")


# -------------------------------------------------------------------------
# run_calibration with apply_ddc=False, apply_lut=False
# -------------------------------------------------------------------------

def test_run_calibration_software_only():
    """run_calibration with apply_ddc=False, apply_lut=False should produce
    a result with ICC and LUT file paths."""
    engine = AutoCalibrationEngine()

    with tempfile.TemporaryDirectory() as tmpdir:
        result = engine.run_calibration(
            output_dir=Path(tmpdir),
            apply_ddc=False,
            apply_lut=False,
            install_profile=False,
            display_name="Test Display PG27UCDM",
        )

    assert isinstance(result, AutoCalibrationResult)
    assert result.success is True
    assert result.icc_profile_path is not None
    assert result.lut_path is not None
    assert result.icc_profile_path.endswith(".icc")
    assert result.lut_path.endswith(".cube")


def test_run_calibration_produces_verification():
    """run_calibration should populate verification data."""
    engine = AutoCalibrationEngine()

    with tempfile.TemporaryDirectory() as tmpdir:
        result = engine.run_calibration(
            output_dir=Path(tmpdir),
            apply_ddc=False,
            apply_lut=False,
            install_profile=False,
            display_name="Test PG27UCDM",
        )

    assert result.verification is not None
    assert "delta_e_avg" in result.verification
    assert "grade" in result.verification
    assert result.delta_e_predicted >= 0.0


# -------------------------------------------------------------------------
# CalibrationTarget defaults
# -------------------------------------------------------------------------

def test_calibration_target_defaults():
    """CalibrationTarget should have sensible defaults."""
    target = CalibrationTarget()
    assert target.whitepoint == "D65"
    assert target.whitepoint_xy == (0.3127, 0.3290)
    assert target.gamma == 2.2
    assert target.gamma_type == "power"
    assert target.luminance == 250.0
    assert target.black_level == 0.0
    assert target.gamut == "sRGB"


def test_calibration_target_custom():
    """CalibrationTarget with custom values should work."""
    target = CalibrationTarget(
        whitepoint="D50",
        whitepoint_xy=(0.3457, 0.3585),
        gamma=2.4,
        luminance=120.0,
        gamut="P3",
    )
    assert target.whitepoint == "D50"
    assert target.gamma == 2.4
    assert target.gamut == "P3"


# -------------------------------------------------------------------------
# UserConsent logic
# -------------------------------------------------------------------------

def test_user_consent_not_approved_by_default():
    """UserConsent should not be approved by default."""
    consent = UserConsent()
    assert consent.user_acknowledged_risks is False
    assert consent.is_approved_for(CalibrationRisk.MEDIUM) is False
    assert consent.is_approved_for(CalibrationRisk.HIGH) is False


def test_user_consent_medium_approved():
    """UserConsent with acknowledged risks should approve MEDIUM."""
    consent = UserConsent(user_acknowledged_risks=True)
    assert consent.is_approved_for(CalibrationRisk.MEDIUM) is True
    assert consent.is_approved_for(CalibrationRisk.HIGH) is False


def test_user_consent_high_requires_hardware_approval():
    """HIGH risk requires both acknowledged_risks and hardware_modification_approved."""
    consent = UserConsent(
        user_acknowledged_risks=True,
        hardware_modification_approved=False,
    )
    assert consent.is_approved_for(CalibrationRisk.HIGH) is False

    consent.hardware_modification_approved = True
    assert consent.is_approved_for(CalibrationRisk.HIGH) is True


def test_request_consent():
    """request_consent should return a UserConsent object."""
    engine = AutoCalibrationEngine()
    consent = engine.request_consent("Test Display", apply_ddc=False)
    assert isinstance(consent, UserConsent)
    assert consent.display_name == "Test Display"
    assert consent.risk_level == CalibrationRisk.MEDIUM
    assert consent.user_acknowledged_risks is False


def test_request_consent_ddc():
    """request_consent with apply_ddc=True should set HIGH risk."""
    engine = AutoCalibrationEngine()
    consent = engine.request_consent("Test Display", apply_ddc=True)
    assert consent.risk_level == CalibrationRisk.HIGH


# -------------------------------------------------------------------------
# AutoCalibrationResult defaults
# -------------------------------------------------------------------------

def test_result_defaults():
    """AutoCalibrationResult should have sensible defaults."""
    result = AutoCalibrationResult()
    assert result.success is False
    assert result.display_name == ""
    assert result.icc_profile_path is None
    assert result.lut_path is None
    assert result.lut_applied is False
    assert result.ddc_available is False
    assert len(result.warnings) == 0
    assert len(result.steps_completed) == 0
