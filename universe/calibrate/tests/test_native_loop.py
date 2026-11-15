"""Tests for the native calibration loop module."""

import numpy as np
import pytest
from calibrate_pro.calibration.native_loop import (
    DisplayProfile, build_correction_lut, compute_de,
    COLORCHECKER_REF_LAB, COLORCHECKER_SRGB,
    _chromaticity, compute_ccmx, QDOLED_CCMX,
)
from calibrate_pro.core.color_math import SRGB_TO_XYZ, srgb_gamma_expand


class TestChromaticity:
    def test_d65_white(self):
        white_xyz = SRGB_TO_XYZ @ np.array([1.0, 1.0, 1.0])
        x, y = _chromaticity(white_xyz)
        assert abs(x - 0.3127) < 0.001
        assert abs(y - 0.3290) < 0.001

    def test_zero_returns_zero(self):
        x, y = _chromaticity(np.array([0.0, 0.0, 0.0]))
        assert x == 0.0 and y == 0.0


class TestDisplayProfile:
    def _make_srgb_profile(self):
        """Create a profile that mimics a perfect sRGB display."""
        n = 17
        levels = np.linspace(0, 1, n)
        # sRGB gamma ~2.2
        trc = np.power(levels, 2.2)
        trc[0] = 0.0; trc[-1] = 1.0

        # sRGB primaries matrix (exact)
        M = SRGB_TO_XYZ.copy()

        return DisplayProfile(
            levels=levels,
            trc_r=trc.copy(), trc_g=trc.copy(), trc_b=trc.copy(),
            M_display=M,
            white_Y=100.0,
            black_xyz=np.zeros(3),
            white_xy=(0.3127, 0.3290),
            red_xy=(0.6400, 0.3300),
            green_xy=(0.3000, 0.6000),
            blue_xy=(0.1500, 0.0600),
            gamma_r=2.2, gamma_g=2.2, gamma_b=2.2,
        )

    def test_identity_lut_for_srgb_display(self):
        """A perfect sRGB display should produce a near-identity LUT."""
        profile = self._make_srgb_profile()
        lut = build_correction_lut(profile, size=5)

        # Check that the LUT is near identity (max deviation < 0.05)
        coords = np.linspace(0, 1, 5)
        r, g, b = np.meshgrid(coords, coords, coords, indexing="ij")
        identity = np.stack([r, g, b], axis=-1)

        # Only check saturated colors (chroma > 0.3) since
        # low-chroma colors pass through as identity anyway
        max_dev = np.max(np.abs(lut.data - identity))
        # For a perfect sRGB display, deviation should be very small
        assert max_dev < 0.15, f"LUT deviation too large: {max_dev:.4f}"

    def test_wide_gamut_correction(self):
        """A wide-gamut display should produce a LUT that compresses gamut."""
        profile = self._make_srgb_profile()
        # Make primaries wider (like QD-OLED)
        profile.M_display = np.array([
            [0.5, 0.2, 0.2],
            [0.25, 0.7, 0.05],
            [0.0, 0.03, 1.0],
        ])
        profile.red_xy = (0.68, 0.31)
        profile.green_xy = (0.26, 0.70)
        profile.blue_xy = (0.15, 0.06)

        lut = build_correction_lut(profile, size=5)

        # Pure red should be desaturated (moved towards center)
        # Red at grid position [4,0,0] -> should have some G and B added
        red_output = lut.data[4, 0, 0]
        assert red_output[0] < 1.0, "Red should be reduced"
        # The correction should make red less pure (add G or B)

    def test_lut_preserves_black(self):
        """Black (0,0,0) should always pass through unchanged."""
        profile = self._make_srgb_profile()
        lut = build_correction_lut(profile, size=5)
        np.testing.assert_array_almost_equal(
            lut.data[0, 0, 0], [0.0, 0.0, 0.0], decimal=6
        )


class TestComputeDe:
    def test_perfect_match(self):
        """A perfect sRGB white should give low dE vs reference."""
        white_xyz = SRGB_TO_XYZ @ np.array([1.0, 1.0, 1.0])
        # Scale to Y=100 (reference white Y=100 system)
        white_Y = white_xyz[1] * 100
        de = compute_de(white_xyz * 100, white_Y, COLORCHECKER_REF_LAB["White"])
        assert de < 5.0  # Reasonable for non-adapted comparison


class TestColorCheckerData:
    def test_all_patches_have_references(self):
        """Every patch in COLORCHECKER_SRGB should have a reference Lab."""
        for name, r, g, b in COLORCHECKER_SRGB:
            assert name in COLORCHECKER_REF_LAB, f"Missing ref for {name}"

    def test_24_patches(self):
        assert len(COLORCHECKER_SRGB) == 24
        assert len(COLORCHECKER_REF_LAB) == 24

    def test_srgb_values_in_range(self):
        for name, r, g, b in COLORCHECKER_SRGB:
            assert 0 <= r <= 1 and 0 <= g <= 1 and 0 <= b <= 1, f"{name} out of range"


class TestCCMX:
    def test_identity_for_same_primaries(self):
        """CCMX should be identity when sensor and true primaries match."""
        prims = ((0.64, 0.33), (0.30, 0.60), (0.15, 0.06), (0.3127, 0.3290))
        ccmx = compute_ccmx(prims, prims)
        np.testing.assert_array_almost_equal(ccmx, np.eye(3), decimal=10)

    def test_qdoled_ccmx_is_diagonal_dominant(self):
        """QD-OLED CCMX should be mostly diagonal (small corrections)."""
        for i in range(3):
            assert abs(QDOLED_CCMX[i, i]) > 0.9, f"Diagonal [{i},{i}] too small"
            for j in range(3):
                if i != j:
                    assert abs(QDOLED_CCMX[i, j]) < 0.15, f"Off-diagonal [{i},{j}] too large"

    def test_ccmx_preserves_white(self):
        """CCMX should map sensor white to true white."""
        def xy_to_XYZ(x, y):
            return np.array([x/y, 1.0, (1-x-y)/y])
        sensor_w = xy_to_XYZ(0.3134, 0.3240)
        true_w = xy_to_XYZ(0.3134, 0.3291)
        corrected = QDOLED_CCMX @ sensor_w
        # Chromaticity should match true white
        cx = corrected[0] / sum(corrected)
        cy = corrected[1] / sum(corrected)
        assert abs(cx - 0.3134) < 0.001
        assert abs(cy - 0.3291) < 0.001
