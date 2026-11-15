"""Tests for calibrate_pro.core.lut_engine — LUT3D and LUTGenerator."""

import tempfile
from pathlib import Path

import numpy as np
import pytest

from calibrate_pro.core.lut_engine import LUT3D, LUTGenerator


# -------------------------------------------------------------------------
# LUT3D.create_identity
# -------------------------------------------------------------------------

@pytest.mark.parametrize("size", [17, 33, 65])
def test_identity_lut_shape(size):
    """Identity LUT should have shape (size, size, size, 3)."""
    lut = LUT3D.create_identity(size)
    assert lut.data.shape == (size, size, size, 3)
    assert lut.size == size


def test_identity_lut_samples():
    """Identity LUT should return the input at sampled grid points."""
    lut = LUT3D.create_identity(17)
    rng = np.random.RandomState(42)
    for _ in range(20):
        # Pick a random grid-aligned point
        idx = rng.randint(0, 17, size=3)
        r, g, b = idx / 16.0
        expected = np.array([r, g, b])
        actual = lut.data[idx[0], idx[1], idx[2]]
        np.testing.assert_allclose(actual, expected, atol=1e-10)


def test_identity_lut_corners():
    """Identity LUT corners should be exact (0,0,0), (1,1,1), etc."""
    lut = LUT3D.create_identity(33)
    np.testing.assert_allclose(lut.data[0, 0, 0], [0, 0, 0], atol=1e-14)
    np.testing.assert_allclose(lut.data[-1, -1, -1], [1, 1, 1], atol=1e-14)
    np.testing.assert_allclose(lut.data[-1, 0, 0], [1, 0, 0], atol=1e-14)
    np.testing.assert_allclose(lut.data[0, -1, 0], [0, 1, 0], atol=1e-14)
    np.testing.assert_allclose(lut.data[0, 0, -1], [0, 0, 1], atol=1e-14)


# -------------------------------------------------------------------------
# LUT3D.apply (trilinear interpolation)
# -------------------------------------------------------------------------

def test_apply_identity_preserves_input():
    """Applying an identity LUT should return approximately the input."""
    lut = LUT3D.create_identity(33)
    rgb = np.array([0.5, 0.3, 0.7])
    result = lut.apply(rgb)
    np.testing.assert_allclose(result, rgb, atol=1e-3)


def test_apply_identity_black():
    """Identity LUT applied to black should return black."""
    lut = LUT3D.create_identity(33)
    result = lut.apply(np.array([0.0, 0.0, 0.0]))
    np.testing.assert_allclose(result, [0, 0, 0], atol=1e-10)


def test_apply_identity_white():
    """Identity LUT applied to white should return white."""
    lut = LUT3D.create_identity(33)
    result = lut.apply(np.array([1.0, 1.0, 1.0]))
    np.testing.assert_allclose(result, [1, 1, 1], atol=1e-3)


def test_apply_batch():
    """LUT apply should handle (N, 3) batch input."""
    lut = LUT3D.create_identity(17)
    batch = np.array([
        [0.0, 0.0, 0.0],
        [0.5, 0.5, 0.5],
        [1.0, 1.0, 1.0],
    ])
    result = lut.apply(batch)
    assert result.shape == (3, 3)
    np.testing.assert_allclose(result[0], [0, 0, 0], atol=1e-10)


# -------------------------------------------------------------------------
# LUT3D save/load roundtrip (.cube format)
# -------------------------------------------------------------------------

def test_save_load_cube_roundtrip():
    """Save a LUT to .cube then load it back; data should match."""
    lut = LUT3D.create_identity(17)
    lut.title = "Test LUT"

    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "test.cube"
        lut.save(path)
        loaded = LUT3D.load_cube(path)

    assert loaded.size == 17
    np.testing.assert_allclose(loaded.data, lut.data, atol=1e-5)


def test_save_load_cube_non_identity():
    """Save/load roundtrip with a non-identity LUT."""
    lut = LUT3D.create_identity(17)
    # Apply a simple modification: boost red channel by 10%
    lut.data[:, :, :, 0] = np.clip(lut.data[:, :, :, 0] * 1.1, 0, 1)
    lut.title = "Red Boost"

    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "red_boost.cube"
        lut.save(path)
        loaded = LUT3D.load_cube(path)

    np.testing.assert_allclose(loaded.data, lut.data, atol=1e-5)


# -------------------------------------------------------------------------
# create_calibration_lut preserves neutral axis
# -------------------------------------------------------------------------

def test_calibration_lut_preserves_neutral_axis(srgb_panel):
    """A calibration LUT for the generic sRGB panel should preserve neutrals."""
    gen = LUTGenerator(size=17)
    primaries = srgb_panel.native_primaries

    lut = gen.create_calibration_lut(
        panel_primaries=(
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple(),
        ),
        panel_white=primaries.white.as_tuple(),
        gamma_red=srgb_panel.gamma_red.gamma,
        gamma_green=srgb_panel.gamma_green.gamma,
        gamma_blue=srgb_panel.gamma_blue.gamma,
        target_gamma=2.2,
    )

    # Check neutral axis: input R=G=B should give output R close to G close to B
    for i in range(lut.size):
        val = lut.data[i, i, i]
        # All three channels should be close to each other
        spread = val.max() - val.min()
        assert spread < 0.02, f"Neutral axis broken at index {i}: {val}"


# -------------------------------------------------------------------------
# create_oklab_perceptual_lut preserves neutral axis and black
# -------------------------------------------------------------------------

def test_oklab_lut_preserves_black(srgb_panel):
    """Oklab perceptual LUT must preserve (0,0,0) -> (0,0,0)."""
    gen = LUTGenerator(size=17)
    primaries = srgb_panel.native_primaries

    lut = gen.create_oklab_perceptual_lut(
        panel_primaries=(
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple(),
        ),
        panel_white=primaries.white.as_tuple(),
        gamma_red=srgb_panel.gamma_red.gamma,
        gamma_green=srgb_panel.gamma_green.gamma,
        gamma_blue=srgb_panel.gamma_blue.gamma,
        target_gamma=2.2,
    )
    np.testing.assert_allclose(lut.data[0, 0, 0], [0, 0, 0], atol=1e-10)


def test_oklab_lut_preserves_neutral(srgb_panel):
    """Oklab perceptual LUT should roughly preserve neutral axis for sRGB panels."""
    gen = LUTGenerator(size=17)
    primaries = srgb_panel.native_primaries

    lut = gen.create_oklab_perceptual_lut(
        panel_primaries=(
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple(),
        ),
        panel_white=primaries.white.as_tuple(),
        gamma_red=srgb_panel.gamma_red.gamma,
        gamma_green=srgb_panel.gamma_green.gamma,
        gamma_blue=srgb_panel.gamma_blue.gamma,
        target_gamma=2.2,
    )

    for i in range(1, lut.size):  # skip black (already tested)
        val = lut.data[i, i, i]
        spread = val.max() - val.min()
        assert spread < 0.02, f"Neutral axis broken at index {i}: {val}"


# -------------------------------------------------------------------------
# create_hdr_calibration_lut preserves black
# -------------------------------------------------------------------------

def test_hdr_lut_preserves_black(srgb_panel):
    """HDR calibration LUT must preserve black (PQ=0 -> PQ=0)."""
    gen = LUTGenerator(size=17)
    primaries = srgb_panel.native_primaries

    lut = gen.create_hdr_calibration_lut(
        panel_primaries=(
            primaries.red.as_tuple(),
            primaries.green.as_tuple(),
            primaries.blue.as_tuple(),
        ),
        panel_white=primaries.white.as_tuple(),
        gamma_red=srgb_panel.gamma_red.gamma,
        gamma_green=srgb_panel.gamma_green.gamma,
        gamma_blue=srgb_panel.gamma_blue.gamma,
        peak_luminance=400.0,
    )
    np.testing.assert_allclose(lut.data[0, 0, 0], [0, 0, 0], atol=1e-10)


# -------------------------------------------------------------------------
# LUT size options
# -------------------------------------------------------------------------

@pytest.mark.parametrize("size", [17, 33, 65])
def test_lut_generator_sizes(size):
    """LUTGenerator should create LUTs of various sizes."""
    gen = LUTGenerator(size=size)
    lut = gen.create_from_function(lambda rgb: rgb, title="Identity")
    assert lut.size == size
    assert lut.data.shape == (size, size, size, 3)
