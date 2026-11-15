"""Shared pytest fixtures for Calibrate Pro test suite."""

import sys
import numpy as np
import pytest

# Ensure the calibrate package is importable
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent.parent))

from calibrate_pro.panels.database import PanelDatabase


@pytest.fixture(scope="session")
def panel_database():
    """Return a PanelDatabase instance (session-scoped for speed)."""
    return PanelDatabase()


@pytest.fixture(scope="session")
def qd_oled_panel(panel_database):
    """Return the PG27UCDM QD-OLED panel characterization."""
    panel = panel_database.get_panel("PG27UCDM")
    assert panel is not None, "PG27UCDM must exist in the database"
    return panel


@pytest.fixture(scope="session")
def srgb_panel(panel_database):
    """Return the GENERIC_SRGB fallback panel characterization."""
    panel = panel_database.get_panel("GENERIC_SRGB")
    assert panel is not None, "GENERIC_SRGB must exist in the database"
    return panel


@pytest.fixture
def sample_xyz():
    """Common XYZ test values (D65 white, mid-gray, red-ish)."""
    return {
        "white_d65": np.array([0.95047, 1.0, 1.08883]),
        "mid_gray": np.array([0.2034, 0.2140, 0.2330]),
        "red_ish": np.array([0.4124, 0.2126, 0.0193]),
        "black": np.array([0.0, 0.0, 0.0]),
    }


@pytest.fixture
def sample_linear_rgb():
    """Common linear sRGB test values."""
    return {
        "white": np.array([1.0, 1.0, 1.0]),
        "black": np.array([0.0, 0.0, 0.0]),
        "red": np.array([1.0, 0.0, 0.0]),
        "green": np.array([0.0, 1.0, 0.0]),
        "blue": np.array([0.0, 0.0, 1.0]),
        "mid_gray": np.array([0.2140, 0.2140, 0.2140]),
        "orange": np.array([0.8, 0.4, 0.1]),
    }
