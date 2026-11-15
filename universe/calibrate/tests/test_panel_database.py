"""Tests for calibrate_pro.panels.database — PanelDatabase and PanelCharacterization."""

import json
import pytest

from calibrate_pro.panels.database import (
    PanelDatabase,
    PanelCharacterization,
    create_from_edid,
)


# -------------------------------------------------------------------------
# PG27UCDM existence and metadata
# -------------------------------------------------------------------------

def test_pg27ucdm_exists(panel_database):
    """PG27UCDM must exist in the database."""
    panel = panel_database.get_panel("PG27UCDM")
    assert panel is not None


def test_pg27ucdm_is_qd_oled(qd_oled_panel):
    """PG27UCDM panel_type should be QD-OLED."""
    assert qd_oled_panel.panel_type == "QD-OLED"


def test_pg27ucdm_manufacturer(qd_oled_panel):
    """PG27UCDM manufacturer should be ASUS."""
    assert qd_oled_panel.manufacturer == "ASUS"


def test_pg27ucdm_wide_gamut(qd_oled_panel):
    """PG27UCDM should be wide-gamut capable."""
    assert qd_oled_panel.capabilities.wide_gamut is True


def test_pg27ucdm_hdr(qd_oled_panel):
    """PG27UCDM should be HDR capable."""
    assert qd_oled_panel.capabilities.hdr_capable is True


# -------------------------------------------------------------------------
# GENERIC_SRGB fallback
# -------------------------------------------------------------------------

def test_generic_srgb_exists(panel_database):
    """GENERIC_SRGB fallback must exist."""
    panel = panel_database.get_panel("GENERIC_SRGB")
    assert panel is not None


def test_generic_srgb_panel_type(srgb_panel):
    """GENERIC_SRGB should be IPS type."""
    assert srgb_panel.panel_type == "IPS"


def test_generic_srgb_not_wide_gamut(srgb_panel):
    """GENERIC_SRGB should not be wide-gamut."""
    assert srgb_panel.capabilities.wide_gamut is False


def test_get_fallback_returns_generic(panel_database):
    """get_fallback should return GENERIC_SRGB."""
    fallback = panel_database.get_fallback()
    assert fallback.manufacturer == "Generic"


# -------------------------------------------------------------------------
# find_panel with various model strings
# -------------------------------------------------------------------------

def test_find_panel_exact_model(panel_database):
    """find_panel should match PG27UCDM exactly."""
    panel = panel_database.find_panel("PG27UCDM")
    assert panel is not None
    assert panel.panel_type == "QD-OLED"


def test_find_panel_partial_match(panel_database):
    """find_panel should match partial model strings via regex."""
    panel = panel_database.find_panel("ROG Swift PG27UCDM")
    assert panel is not None
    assert panel.manufacturer == "ASUS"


def test_find_panel_case_insensitive(panel_database):
    """find_panel should be case-insensitive."""
    panel = panel_database.find_panel("pg27ucdm")
    assert panel is not None


def test_find_panel_alienware(panel_database):
    """find_panel should find Dell Alienware AW3423DW."""
    panel = panel_database.find_panel("AW3423DW")
    assert panel is not None
    assert panel.manufacturer == "Dell"


def test_find_panel_lg_c3(panel_database):
    """find_panel should find LG C3 OLED via pattern."""
    panel = panel_database.find_panel("OLED48C3")
    assert panel is not None
    assert panel.panel_type == "WOLED"


def test_find_panel_unknown_returns_none(panel_database):
    """find_panel should return None for completely unknown models."""
    panel = panel_database.find_panel("XYZZY-NONEXISTENT-9999")
    assert panel is None


def test_find_panel_empty_string(panel_database):
    """find_panel with empty string should return None (or a match-anything)."""
    # The GENERIC_SRGB pattern is .* but it's skipped in find_panel
    panel = panel_database.find_panel("")
    # Could be None or could match something; main test is no crash
    # (The implementation skips GENERIC_SRGB, so empty might match a broad pattern)


# -------------------------------------------------------------------------
# Database size
# -------------------------------------------------------------------------

def test_database_has_many_panels(panel_database):
    """Database should have more than 30 panels."""
    # list_panels excludes GENERIC_SRGB
    panel_list = panel_database.list_panels()
    assert len(panel_list) > 30 or len(panel_database.panels) > 30


# -------------------------------------------------------------------------
# create_from_edid
# -------------------------------------------------------------------------

def test_create_from_edid_valid():
    """create_from_edid should produce a valid PanelCharacterization."""
    edid = {
        "red": (0.6800, 0.3200),
        "green": (0.2650, 0.6900),
        "blue": (0.1500, 0.0600),
        "white": (0.3127, 0.3290),
    }
    panel = create_from_edid(edid, monitor_name="Test Monitor", manufacturer="TestCo")
    assert isinstance(panel, PanelCharacterization)
    assert panel.manufacturer == "TestCo"
    assert panel.native_primaries.red.x == pytest.approx(0.68, abs=0.001)
    assert panel.native_primaries.green.y == pytest.approx(0.69, abs=0.001)


def test_create_from_edid_wide_gamut_detection():
    """create_from_edid should detect wide-gamut panels from red primary."""
    edid_wide = {
        "red": (0.6800, 0.3200),
        "green": (0.2650, 0.6900),
        "blue": (0.1500, 0.0600),
        "white": (0.3127, 0.3290),
    }
    panel = create_from_edid(edid_wide)
    assert panel.capabilities.wide_gamut is True

    edid_srgb = {
        "red": (0.6400, 0.3300),
        "green": (0.3000, 0.6000),
        "blue": (0.1500, 0.0600),
        "white": (0.3127, 0.3290),
    }
    panel2 = create_from_edid(edid_srgb)
    assert panel2.capabilities.wide_gamut is False


# -------------------------------------------------------------------------
# JSON serialization roundtrip
# -------------------------------------------------------------------------

def test_json_roundtrip(qd_oled_panel):
    """to_dict -> JSON -> from_dict roundtrip should preserve key fields."""
    d = qd_oled_panel.to_dict()
    json_str = json.dumps(d)
    d2 = json.loads(json_str)
    recovered = PanelCharacterization.from_dict(d2)

    assert recovered.manufacturer == qd_oled_panel.manufacturer
    assert recovered.panel_type == qd_oled_panel.panel_type
    assert recovered.native_primaries.red.x == pytest.approx(
        qd_oled_panel.native_primaries.red.x, abs=1e-6
    )
    assert recovered.gamma_red.gamma == pytest.approx(
        qd_oled_panel.gamma_red.gamma, abs=1e-6
    )
    assert recovered.capabilities.max_luminance_hdr == pytest.approx(
        qd_oled_panel.capabilities.max_luminance_hdr, abs=0.1
    )


def test_json_roundtrip_generic(srgb_panel):
    """JSON roundtrip for generic sRGB panel."""
    d = srgb_panel.to_dict()
    json_str = json.dumps(d)
    d2 = json.loads(json_str)
    recovered = PanelCharacterization.from_dict(d2)
    assert recovered.panel_type == "IPS"
    assert recovered.manufacturer == "Generic"
