"""Tests for calibrate_pro.core.color_math — every color space conversion with roundtrip verification."""

import numpy as np
import pytest

from calibrate_pro.core.color_math import (
    # Oklab / Oklch
    linear_srgb_to_oklab, oklab_to_linear_srgb,
    oklab_to_oklch, oklch_to_oklab,
    # JzAzBz / JzCzhz
    xyz_abs_to_jzazbz, jzazbz_to_xyz_abs,
    jzazbz_to_jzczhz, jzczhz_to_jzazbz,
    # ICtCp
    xyz_abs_to_ictcp, ictcp_to_xyz_abs,
    # PQ / HLG
    pq_oetf, pq_eotf, hlg_oetf, hlg_eotf,
    # ACES
    acescg_to_xyz, xyz_to_acescg,
    acescc_encode, acescc_decode,
    acescct_encode, acescct_decode,
    # Display P3 / Rec.2020
    display_p3_to_xyz, xyz_to_display_p3,
    rec2020_to_xyz, xyz_to_rec2020,
    rec2020_oetf, rec2020_eotf,
    # CIE Luv
    xyz_to_luv, luv_to_xyz,
    # HSL / HSV / HWB
    srgb_to_hsl, hsl_to_srgb,
    srgb_to_hsv, hsv_to_srgb,
    srgb_to_hwb, hwb_to_srgb,
    # CAM16
    cam16_environment, xyz_to_cam16, cam16_to_xyz,
    cam16_to_ucs, cam16_ucs_delta_e,
    # Bradford
    bradford_adapt, D65_WHITE, D50_WHITE,
    # Delta E
    delta_e_2000, xyz_to_lab,
    # Gamut
    compute_gamut_boundary, get_max_chroma, gamut_map_chroma_compress,
    XYZ_TO_SRGB,
    # sRGB helpers
    srgb_gamma_expand, srgb_gamma_compress,
)


# -------------------------------------------------------------------------
# Oklab / Oklch roundtrip
# -------------------------------------------------------------------------

_OKLAB_COLORS = [
    ("black", np.array([0.0, 0.0, 0.0])),
    ("white", np.array([1.0, 1.0, 1.0])),
    ("red", np.array([1.0, 0.0, 0.0])),
    ("green", np.array([0.0, 1.0, 0.0])),
    ("blue", np.array([0.0, 0.0, 1.0])),
    ("mid_gray_25", np.array([0.25, 0.25, 0.25])),
    ("mid_gray_50", np.array([0.5, 0.5, 0.5])),
    ("mid_gray_75", np.array([0.75, 0.75, 0.75])),
]


@pytest.mark.parametrize("name,rgb", _OKLAB_COLORS, ids=[c[0] for c in _OKLAB_COLORS])
def test_oklab_roundtrip(name, rgb):
    """linear sRGB -> Oklab -> linear sRGB roundtrip."""
    oklab = linear_srgb_to_oklab(rgb)
    recovered = oklab_to_linear_srgb(oklab)
    np.testing.assert_allclose(recovered, rgb, atol=1e-6)


@pytest.mark.parametrize("name,rgb", _OKLAB_COLORS, ids=[c[0] for c in _OKLAB_COLORS])
def test_oklch_roundtrip(name, rgb):
    """linear sRGB -> Oklab -> Oklch -> Oklab -> linear sRGB roundtrip."""
    oklab = linear_srgb_to_oklab(rgb)
    oklch = oklab_to_oklch(oklab)
    oklab2 = oklch_to_oklab(oklch)
    recovered = oklab_to_linear_srgb(oklab2)
    np.testing.assert_allclose(recovered, rgb, atol=1e-6)


# -------------------------------------------------------------------------
# JzAzBz / JzCzhz roundtrip (HDR luminance levels)
# -------------------------------------------------------------------------

_JZAZBZ_LUMINANCES = [1.0, 100.0, 1000.0, 10000.0]


@pytest.mark.parametrize("Y_nits", _JZAZBZ_LUMINANCES)
def test_jzazbz_roundtrip(Y_nits):
    """Absolute XYZ -> JzAzBz -> absolute XYZ roundtrip at various luminances."""
    # D65 white scaled to Y_nits
    xyz = np.array([0.95047 * Y_nits, Y_nits, 1.08883 * Y_nits])
    jab = xyz_abs_to_jzazbz(xyz)
    recovered = jzazbz_to_xyz_abs(jab)
    np.testing.assert_allclose(recovered, xyz, atol=1.0)


@pytest.mark.parametrize("Y_nits", _JZAZBZ_LUMINANCES)
def test_jzczhz_roundtrip(Y_nits):
    """JzAzBz -> JzCzhz -> JzAzBz roundtrip."""
    xyz = np.array([0.95047 * Y_nits, Y_nits, 1.08883 * Y_nits])
    jab = xyz_abs_to_jzazbz(xyz)
    jch = jzazbz_to_jzczhz(jab)
    jab2 = jzczhz_to_jzazbz(jch)
    np.testing.assert_allclose(jab2, jab, atol=1e-10)


# -------------------------------------------------------------------------
# ICtCp roundtrip
# -------------------------------------------------------------------------

def test_ictcp_roundtrip():
    """Absolute XYZ -> ICtCp -> absolute XYZ roundtrip."""
    xyz = np.array([95.047, 100.0, 108.883])  # D65 at 100 cd/m2
    ictcp = xyz_abs_to_ictcp(xyz)
    recovered = ictcp_to_xyz_abs(ictcp)
    np.testing.assert_allclose(recovered, xyz, atol=1.0)


def test_ictcp_roundtrip_dark():
    """ICtCp roundtrip at low luminance."""
    xyz = np.array([0.95047, 1.0, 1.08883])  # 1 cd/m2
    ictcp = xyz_abs_to_ictcp(xyz)
    recovered = ictcp_to_xyz_abs(ictcp)
    np.testing.assert_allclose(recovered, xyz, atol=1.0)


# -------------------------------------------------------------------------
# PQ encode/decode roundtrip
# -------------------------------------------------------------------------

_PQ_KEY_VALUES = [0.0, 100.0, 203.0, 1000.0, 10000.0]


@pytest.mark.parametrize("nits", _PQ_KEY_VALUES)
def test_pq_roundtrip(nits):
    """PQ encode -> decode roundtrip at key luminance values."""
    val = np.array([nits])
    encoded = pq_oetf(val)
    decoded = pq_eotf(encoded)
    np.testing.assert_allclose(decoded, val, atol=0.01)


def test_pq_zero():
    """PQ(0) should be very close to 0 (PQ has a small constant offset)."""
    assert float(pq_oetf(np.array([0.0]))[0]) == pytest.approx(0.0, abs=1e-5)


def test_pq_peak():
    """PQ(10000 nits) should be close to 1.0."""
    assert float(pq_oetf(np.array([10000.0]))[0]) == pytest.approx(1.0, abs=1e-3)


# -------------------------------------------------------------------------
# HLG encode/decode roundtrip
# -------------------------------------------------------------------------

@pytest.mark.parametrize("v", [0.0, 0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 1.0])
def test_hlg_roundtrip(v):
    """HLG OETF -> EOTF roundtrip."""
    val = np.array([v])
    encoded = hlg_oetf(val)
    decoded = hlg_eotf(encoded)
    np.testing.assert_allclose(decoded, val, atol=1e-6)


# -------------------------------------------------------------------------
# ACEScg / XYZ roundtrip
# -------------------------------------------------------------------------

def test_acescg_xyz_roundtrip():
    """ACEScg -> XYZ -> ACEScg roundtrip."""
    rgb = np.array([0.5, 0.3, 0.7])
    xyz = acescg_to_xyz(rgb)
    recovered = xyz_to_acescg(xyz)
    np.testing.assert_allclose(recovered, rgb, atol=1e-6)


# -------------------------------------------------------------------------
# ACEScc encode/decode roundtrip
# -------------------------------------------------------------------------

@pytest.mark.parametrize("v", [0.001, 0.01, 0.1, 0.5, 1.0, 10.0])
def test_acescc_roundtrip(v):
    """ACEScc encode -> decode roundtrip."""
    val = np.array([v])
    encoded = acescc_encode(val)
    decoded = acescc_decode(encoded)
    np.testing.assert_allclose(decoded, val, atol=1e-4)


# -------------------------------------------------------------------------
# ACEScct encode/decode roundtrip
# -------------------------------------------------------------------------

@pytest.mark.parametrize("v", [0.001, 0.005, 0.01, 0.1, 0.5, 1.0, 10.0])
def test_acescct_roundtrip(v):
    """ACEScct encode -> decode roundtrip."""
    val = np.array([v])
    encoded = acescct_encode(val)
    decoded = acescct_decode(encoded)
    np.testing.assert_allclose(decoded, val, atol=1e-4)


# -------------------------------------------------------------------------
# Display P3 roundtrip
# -------------------------------------------------------------------------

def test_display_p3_roundtrip():
    """Display P3 -> XYZ -> Display P3 roundtrip."""
    rgb = np.array([0.6, 0.4, 0.2])
    xyz = display_p3_to_xyz(rgb)
    recovered = xyz_to_display_p3(xyz)
    np.testing.assert_allclose(recovered, rgb, atol=1e-6)


# -------------------------------------------------------------------------
# Rec.2020 roundtrip (OETF/EOTF)
# -------------------------------------------------------------------------

@pytest.mark.parametrize("v", [0.0, 0.01, 0.1, 0.5, 0.9, 1.0])
def test_rec2020_oetf_eotf_roundtrip(v):
    """Rec.2020 OETF -> EOTF roundtrip."""
    val = np.array([v])
    encoded = rec2020_oetf(val)
    decoded = rec2020_eotf(encoded)
    np.testing.assert_allclose(decoded, val, atol=1e-6)


def test_rec2020_xyz_roundtrip():
    """Rec.2020 -> XYZ -> Rec.2020 roundtrip."""
    rgb = np.array([0.5, 0.3, 0.7])
    xyz = rec2020_to_xyz(rgb)
    recovered = xyz_to_rec2020(xyz)
    np.testing.assert_allclose(recovered, rgb, atol=1e-5)


# -------------------------------------------------------------------------
# CIE Luv roundtrip
# -------------------------------------------------------------------------

def test_luv_roundtrip(sample_xyz):
    """XYZ -> Luv -> XYZ roundtrip."""
    xyz = sample_xyz["white_d65"]
    luv = xyz_to_luv(xyz)
    recovered = luv_to_xyz(luv)
    np.testing.assert_allclose(recovered, xyz, atol=1e-6)


def test_luv_roundtrip_midgray(sample_xyz):
    """Luv roundtrip for mid gray."""
    xyz = sample_xyz["mid_gray"]
    luv = xyz_to_luv(xyz)
    recovered = luv_to_xyz(luv)
    np.testing.assert_allclose(recovered, xyz, atol=1e-6)


# -------------------------------------------------------------------------
# HSL / HSV / HWB roundtrips
# -------------------------------------------------------------------------

_HSL_COLORS = [
    ("red", np.array([1.0, 0.0, 0.0])),
    ("green", np.array([0.0, 1.0, 0.0])),
    ("blue", np.array([0.0, 0.0, 1.0])),
    ("white", np.array([1.0, 1.0, 1.0])),
    ("mid_gray", np.array([0.5, 0.5, 0.5])),
    ("cyan", np.array([0.0, 1.0, 1.0])),
    ("orange", np.array([1.0, 0.5, 0.0])),
]


@pytest.mark.parametrize("name,rgb", _HSL_COLORS, ids=[c[0] for c in _HSL_COLORS])
def test_hsl_roundtrip(name, rgb):
    """sRGB -> HSL -> sRGB roundtrip."""
    hsl = srgb_to_hsl(rgb)
    recovered = hsl_to_srgb(hsl)
    np.testing.assert_allclose(recovered, rgb, atol=1e-10)


@pytest.mark.parametrize("name,rgb", _HSL_COLORS, ids=[c[0] for c in _HSL_COLORS])
def test_hsv_roundtrip(name, rgb):
    """sRGB -> HSV -> sRGB roundtrip."""
    hsv = srgb_to_hsv(rgb)
    recovered = hsv_to_srgb(hsv)
    np.testing.assert_allclose(recovered, rgb, atol=1e-10)


@pytest.mark.parametrize("name,rgb", _HSL_COLORS, ids=[c[0] for c in _HSL_COLORS])
def test_hwb_roundtrip(name, rgb):
    """sRGB -> HWB -> sRGB roundtrip."""
    hwb = srgb_to_hwb(rgb)
    recovered = hwb_to_srgb(hwb)
    np.testing.assert_allclose(recovered, rgb, atol=1e-10)


# -------------------------------------------------------------------------
# CAM16 roundtrip
# -------------------------------------------------------------------------

def test_cam16_white_has_high_J_low_C():
    """D65 white should have J near 100 and C near 0."""
    env = cam16_environment()
    white_xyz = np.array([95.047, 100.0, 108.883])
    result = xyz_to_cam16(white_xyz, env)
    assert result["J"] == pytest.approx(100.0, abs=0.5)
    assert result["C"] == pytest.approx(0.0, abs=2.0)  # CAM16 can show small residual chroma


def test_cam16_roundtrip():
    """XYZ -> CAM16 (J, C, h) -> XYZ roundtrip."""
    env = cam16_environment()
    xyz = np.array([40.0, 30.0, 20.0])
    cam = xyz_to_cam16(xyz, env)
    recovered = cam16_to_xyz(cam["J"], cam["C"], cam["h"], env)
    np.testing.assert_allclose(recovered, xyz, atol=0.2)  # CAM16 inverse has limited precision


# -------------------------------------------------------------------------
# CAM16-UCS Delta E
# -------------------------------------------------------------------------

def test_cam16_ucs_delta_e_identical():
    """Identical colors should have Delta E = 0."""
    env = cam16_environment()
    xyz = np.array([50.0, 50.0, 50.0])
    cam = xyz_to_cam16(xyz, env)
    ucs = cam16_to_ucs(cam["J"], cam["M"], cam["h"])
    assert cam16_ucs_delta_e(ucs, ucs) == pytest.approx(0.0, abs=1e-10)


def test_cam16_ucs_delta_e_different():
    """Different colors should have Delta E > 0."""
    env = cam16_environment()
    xyz1 = np.array([50.0, 50.0, 50.0])
    xyz2 = np.array([60.0, 40.0, 30.0])
    cam1 = xyz_to_cam16(xyz1, env)
    cam2 = xyz_to_cam16(xyz2, env)
    ucs1 = cam16_to_ucs(cam1["J"], cam1["M"], cam1["h"])
    ucs2 = cam16_to_ucs(cam2["J"], cam2["M"], cam2["h"])
    assert cam16_ucs_delta_e(ucs1, ucs2) > 0.0


# -------------------------------------------------------------------------
# Bradford chromatic adaptation
# -------------------------------------------------------------------------

def test_bradford_d65_d50_d65_roundtrip():
    """D65 -> D50 -> D65 roundtrip should recover original XYZ."""
    xyz = np.array([0.5, 0.4, 0.3])
    adapted = bradford_adapt(xyz, D65_WHITE, D50_WHITE)
    recovered = bradford_adapt(adapted, D50_WHITE, D65_WHITE)
    np.testing.assert_allclose(recovered, xyz, atol=1e-10)


def test_bradford_same_illuminant():
    """Adapting to the same illuminant should be identity."""
    xyz = np.array([0.5, 0.4, 0.3])
    adapted = bradford_adapt(xyz, D65_WHITE, D65_WHITE)
    np.testing.assert_allclose(adapted, xyz, atol=1e-14)


# -------------------------------------------------------------------------
# CIEDE2000
# -------------------------------------------------------------------------

def test_ciede2000_identical():
    """Identical Lab colors should have Delta E = 0."""
    lab = np.array([50.0, 20.0, -10.0])
    assert delta_e_2000(lab, lab) == pytest.approx(0.0, abs=1e-10)


def test_ciede2000_known_pair():
    """Known CIEDE2000 pair should give a positive value."""
    lab1 = np.array([50.0, 2.6772, -79.7751])
    lab2 = np.array([50.0, 0.0, -82.7485])
    de = delta_e_2000(lab1, lab2)
    assert de > 0.0
    # Known result for this pair is approximately 2.0425
    assert de == pytest.approx(2.0425, abs=0.1)


# -------------------------------------------------------------------------
# Gamut boundary computation
# -------------------------------------------------------------------------

def test_gamut_boundary_mid_lightness():
    """Max chroma at L=50 should be > 0 for sRGB."""
    boundary = compute_gamut_boundary(XYZ_TO_SRGB, lightness_steps=11, hue_steps=36)
    # L=50 is index 5 in an 11-step grid
    max_c = get_max_chroma(boundary, 50.0, 0.0)
    assert max_c > 0.0


def test_gamut_boundary_black():
    """Max chroma at L=0 should be approximately 0."""
    boundary = compute_gamut_boundary(XYZ_TO_SRGB, lightness_steps=11, hue_steps=36)
    max_c = get_max_chroma(boundary, 0.0, 0.0)
    assert max_c == pytest.approx(0.0, abs=5.0)


# -------------------------------------------------------------------------
# Gamut mapping
# -------------------------------------------------------------------------

def test_gamut_map_reduces_chroma():
    """An out-of-gamut color should have reduced chroma after mapping."""
    boundary = compute_gamut_boundary(XYZ_TO_SRGB, lightness_steps=11, hue_steps=36)
    # A highly saturated Lab color likely out of sRGB gamut
    lab_oog = np.array([50.0, 100.0, 50.0])
    mapped = gamut_map_chroma_compress(lab_oog, boundary, method="clip")
    import math
    original_c = math.sqrt(lab_oog[1]**2 + lab_oog[2]**2)
    mapped_c = math.sqrt(mapped[1]**2 + mapped[2]**2)
    assert mapped_c <= original_c


def test_gamut_map_in_gamut_unchanged():
    """An in-gamut color should not be changed by gamut mapping."""
    boundary = compute_gamut_boundary(XYZ_TO_SRGB, lightness_steps=11, hue_steps=36)
    # A mild Lab color that is definitely in sRGB gamut
    lab_ig = np.array([50.0, 5.0, 5.0])
    mapped = gamut_map_chroma_compress(lab_ig, boundary, method="clip")
    np.testing.assert_allclose(mapped, lab_ig, atol=1e-6)
