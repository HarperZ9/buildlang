"""
scRGB Compositor Pipeline Model (Phase 2)

Models the Windows DWM HDR compositing pipeline end-to-end:

    App (sRGB / scRGB) --> DWM compositor (scRGB FP16) --> Display output (PQ / sRGB)

Pipeline stages explained
-------------------------

1. **Application output**
   - SDR apps output sRGB (gamma ~2.2, 0-1 range).
   - HDR-aware apps output scRGB: linear-light, BT.709 primaries,
     values >1.0 represent super-white / HDR highlights.

2. **DWM compositor (scRGB FP16 back-buffer)**
   - Windows composites *all* windows into a single scRGB FP16 surface.
   - SDR content is up-converted: sRGB EOTF is removed, then scaled so
     that 1.0 in scRGB equals the user's SDR-white-level setting
     (typically 80-480 cd/m2, default ~200 on OLED).
   - HDR content passes through in scRGB (values above 1.0 are preserved).

3. **DWM LUT insertion point** (our calibration hook)
   - When dwm_lut is active, a per-monitor 3D LUT is applied to the
     compositor's scRGB output BEFORE the final transfer-function encode.
   - This is where Calibrate Pro's calibration correction lives.
   - The LUT input domain is normalised scRGB [0..1] where 1.0 maps to
     peak_nits (HDR) or sdr_white_nits (SDR-only mode).

4. **Transfer-function encode**
   - For HDR displays: scRGB is converted to PQ (ST.2084) with BT.2020
     primaries and sent over HDMI/DP as HDR10.
   - For SDR displays: scRGB is gamma-encoded back to sRGB and sent as
     8/10-bit per channel.

This module provides numeric helpers to convert between these stages,
compute HDR headroom, and verify round-trip accuracy.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Optional, Tuple

import numpy as np


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

# PQ (ST.2084) constants
_PQ_M1 = 2610.0 / 16384.0     # 0.1593017578125
_PQ_M2 = 2523.0 / 4096.0 * 128.0  # 78.84375
_PQ_C1 = 3424.0 / 4096.0      # 0.8359375
_PQ_C2 = 2413.0 / 4096.0 * 32.0   # 18.8515625
_PQ_C3 = 2392.0 / 4096.0 * 32.0   # 18.6875

# Reference white for sRGB / Rec.709
_SRGB_WHITE_NITS = 80.0

# PQ absolute peak
_PQ_PEAK_NITS = 10000.0


# ---------------------------------------------------------------------------
# Low-level transfer functions
# ---------------------------------------------------------------------------

def _srgb_eotf(v: np.ndarray) -> np.ndarray:
    """sRGB EOTF: gamma-encoded [0,1] -> linear-light [0,1]."""
    v = np.clip(v, 0.0, 1.0)
    return np.where(
        v <= 0.04045,
        v / 12.92,
        ((v + 0.055) / 1.055) ** 2.4,
    )


def _srgb_oetf(lin: np.ndarray) -> np.ndarray:
    """sRGB OETF: linear-light [0,1] -> gamma-encoded [0,1]."""
    lin = np.clip(lin, 0.0, 1.0)
    return np.where(
        lin <= 0.0031308,
        lin * 12.92,
        1.055 * lin ** (1.0 / 2.4) - 0.055,
    )


def _pq_eotf(pq: np.ndarray, peak_nits: float = _PQ_PEAK_NITS) -> np.ndarray:
    """
    PQ (ST.2084) EOTF: PQ-encoded [0,1] -> absolute luminance in cd/m2.

    The result is normalised to *peak_nits* so that 1.0 == peak_nits.
    """
    pq = np.clip(pq, 0.0, 1.0)
    vp = pq ** (1.0 / _PQ_M2)
    num = np.maximum(vp - _PQ_C1, 0.0)
    den = _PQ_C2 - _PQ_C3 * vp
    # Avoid division by zero
    den = np.where(np.abs(den) < 1e-12, 1e-12, den)
    linear = (num / den) ** (1.0 / _PQ_M1)
    return linear * peak_nits


def _pq_oetf(nits: np.ndarray, peak_nits: float = _PQ_PEAK_NITS) -> np.ndarray:
    """
    PQ (ST.2084) OETF: absolute luminance in cd/m2 -> PQ-encoded [0,1].

    Input *nits* is absolute luminance (not normalised).
    """
    nits = np.clip(nits, 0.0, peak_nits)
    y = nits / peak_nits  # normalise to [0, 1]
    yp = y ** _PQ_M1
    num = _PQ_C1 + _PQ_C2 * yp
    den = 1.0 + _PQ_C3 * yp
    return (num / den) ** _PQ_M2


# ---------------------------------------------------------------------------
# Pipeline conversion helpers
# ---------------------------------------------------------------------------

def sdr_to_scrgb(
    srgb: np.ndarray,
    sdr_white_nits: float = _SRGB_WHITE_NITS,
) -> np.ndarray:
    """
    Convert SDR sRGB content to scRGB linear values as the DWM sees them.

    The DWM removes the sRGB EOTF and then scales so that reference white
    (1.0 in sRGB) maps to ``sdr_white_nits / peak_nits`` in the scRGB
    back-buffer. For pure SDR usage the result stays in [0, 1].

    Parameters
    ----------
    srgb : np.ndarray
        sRGB gamma-encoded values in [0, 1].
    sdr_white_nits : float
        SDR white level in cd/m2 (Windows "SDR content brightness" slider).

    Returns
    -------
    np.ndarray
        scRGB linear-light values.  1.0 == ``sdr_white_nits``.
    """
    srgb = np.asarray(srgb, dtype=np.float64)
    linear = _srgb_eotf(srgb)
    # In the DWM's scRGB buffer, SDR white (linear 1.0 after EOTF) stays
    # at 1.0; HDR content goes above 1.0.  We keep the convention that
    # 1.0 == sdr_white_nits so the caller can scale further if needed.
    return linear


def scrgb_to_pq(
    scrgb: np.ndarray,
    sdr_white_nits: float = _SRGB_WHITE_NITS,
    peak_nits: float = _PQ_PEAK_NITS,
) -> np.ndarray:
    """
    Convert scRGB linear values to PQ-encoded signal for the display.

    This models the DWM's final output stage when sending to an HDR display.

    Parameters
    ----------
    scrgb : np.ndarray
        scRGB linear values where 1.0 == ``sdr_white_nits``.
    sdr_white_nits : float
        The SDR white level setting.
    peak_nits : float
        Peak luminance of the PQ signal (normally 10 000).

    Returns
    -------
    np.ndarray
        PQ-encoded values in [0, 1].
    """
    scrgb = np.asarray(scrgb, dtype=np.float64)
    # Convert scRGB units (1.0 == sdr_white_nits) to absolute nits
    absolute_nits = scrgb * sdr_white_nits
    return _pq_oetf(absolute_nits, peak_nits)


def pq_to_scrgb(
    pq: np.ndarray,
    sdr_white_nits: float = _SRGB_WHITE_NITS,
    peak_nits: float = _PQ_PEAK_NITS,
) -> np.ndarray:
    """
    Convert PQ-encoded signal back to scRGB linear values (inverse of
    :func:`scrgb_to_pq`).

    Parameters
    ----------
    pq : np.ndarray
        PQ-encoded values in [0, 1].
    sdr_white_nits : float
        SDR white level setting.
    peak_nits : float
        Peak luminance of the PQ signal (normally 10 000).

    Returns
    -------
    np.ndarray
        scRGB linear values where 1.0 == ``sdr_white_nits``.
    """
    pq = np.asarray(pq, dtype=np.float64)
    absolute_nits = _pq_eotf(pq, peak_nits)
    return absolute_nits / sdr_white_nits


def compute_hdr_headroom(
    sdr_white_nits: float = 200.0,
    peak_nits: float = 1000.0,
) -> float:
    """
    Compute the HDR headroom: how many stops above SDR white the panel
    can reach.

    For a 1000-nit panel with SDR white at 200 nits the headroom is
    ``1000 / 200 = 5x`` or about 2.32 stops.

    Parameters
    ----------
    sdr_white_nits : float
        User's SDR content brightness setting (cd/m2).
    peak_nits : float
        Panel peak luminance (cd/m2).

    Returns
    -------
    float
        Headroom as a linear multiplier (e.g. 5.0 means the panel can
        produce 5x the SDR white level).
    """
    if sdr_white_nits <= 0:
        return 0.0
    return peak_nits / sdr_white_nits


def compute_hdr_headroom_stops(
    sdr_white_nits: float = 200.0,
    peak_nits: float = 1000.0,
) -> float:
    """
    Headroom expressed in photographic stops (log2).

    Parameters
    ----------
    sdr_white_nits : float
        SDR white level.
    peak_nits : float
        Panel peak luminance.

    Returns
    -------
    float
        Headroom in stops (e.g. ~2.32 for 5x headroom).
    """
    headroom = compute_hdr_headroom(sdr_white_nits, peak_nits)
    if headroom <= 0:
        return 0.0
    return math.log2(headroom)


# ---------------------------------------------------------------------------
# ScRGBPipelineModel - end-to-end pipeline modelling
# ---------------------------------------------------------------------------

@dataclass
class ScRGBPipelineModel:
    """
    Models the complete Windows DWM HDR compositing pipeline and tracks
    where the Calibrate Pro LUT is inserted.

    Attributes
    ----------
    sdr_white_nits : float
        User's "SDR content brightness" slider value.
    peak_nits : float
        Panel peak luminance (cd/m2).
    hdr_enabled : bool
        Whether Windows HDR mode is active.
    lut_applied : bool
        Whether a dwm_lut calibration LUT is active.

    Pipeline diagram::

        App output        DWM scRGB buffer        [LUT hook]        Display encode
        ----------        ----------------        ----------        --------------
        sRGB 0-1    -->   linear FP16        -->  3D LUT     -->   PQ 0-1 (HDR)
        scRGB >1.0  -->   (merged)           -->  correction  -->  sRGB 0-1 (SDR)
    """

    sdr_white_nits: float = 200.0
    peak_nits: float = 1000.0
    hdr_enabled: bool = True
    lut_applied: bool = False

    # Internal: cached headroom
    _headroom: float = field(init=False, repr=False, default=0.0)

    def __post_init__(self) -> None:
        self._headroom = compute_hdr_headroom(self.sdr_white_nits, self.peak_nits)

    # ----- convenience properties -----

    @property
    def headroom(self) -> float:
        """Linear headroom multiplier (peak / SDR white)."""
        return self._headroom

    @property
    def headroom_stops(self) -> float:
        """Headroom in photographic stops."""
        return compute_hdr_headroom_stops(self.sdr_white_nits, self.peak_nits)

    @property
    def scrgb_peak(self) -> float:
        """
        The scRGB value that maps to the panel's peak luminance.

        For an SDR-white of 200 nits and peak of 1000 nits this is 5.0.
        """
        return self._headroom

    # ----- pipeline stages -----

    def sdr_to_compositor(self, srgb: np.ndarray) -> np.ndarray:
        """
        Stage 1: SDR app sRGB -> DWM scRGB compositor buffer.

        Applies the sRGB EOTF to linearise and leaves the result scaled
        so 1.0 == SDR white.
        """
        return sdr_to_scrgb(srgb, self.sdr_white_nits)

    def compositor_to_display(self, scrgb: np.ndarray) -> np.ndarray:
        """
        Stage 3+4: DWM scRGB compositor -> display wire signal.

        If HDR is enabled, encodes to PQ.  Otherwise re-applies sRGB OETF.
        """
        if self.hdr_enabled:
            return scrgb_to_pq(scrgb, self.sdr_white_nits, self.peak_nits)
        else:
            # SDR path: re-encode to sRGB gamma
            return _srgb_oetf(np.clip(scrgb, 0.0, 1.0))

    def display_to_compositor(self, signal: np.ndarray) -> np.ndarray:
        """
        Inverse of :meth:`compositor_to_display`.

        Takes a display wire signal and converts back to the scRGB
        compositor domain.
        """
        if self.hdr_enabled:
            return pq_to_scrgb(signal, self.sdr_white_nits, self.peak_nits)
        else:
            return _srgb_eotf(signal)

    def end_to_end_sdr(self, srgb: np.ndarray) -> np.ndarray:
        """
        Full pipeline: SDR sRGB app input -> display wire signal.

        This is what an un-calibrated SDR window looks like after passing
        through the DWM.
        """
        scrgb = self.sdr_to_compositor(srgb)
        return self.compositor_to_display(scrgb)

    # ----- LUT placement documentation -----

    @staticmethod
    def get_lut_insertion_point() -> str:
        """
        Describe where in the pipeline the dwm_lut calibration LUT is applied.

        Returns a human-readable explanation.
        """
        return (
            "The dwm_lut 3D LUT is applied BETWEEN the DWM compositor's "
            "scRGB FP16 back-buffer and the final transfer-function encode "
            "(PQ for HDR, sRGB OETF for SDR).\n\n"
            "Pipeline position:\n"
            "  App -> [sRGB EOTF] -> scRGB compositor -> ** 3D LUT ** -> "
            "[PQ OETF / sRGB OETF] -> Display\n\n"
            "The LUT operates on normalised scRGB values where:\n"
            "  - 0.0 = black\n"
            "  - 1.0 = SDR reference white (sdr_white_nits)\n"
            "  - >1.0 = HDR highlights (up to peak_nits / sdr_white_nits)\n\n"
            "Because the LUT sits after compositing but before the output "
            "encode, it can correct for panel-specific colour errors, "
            "tone-map HDR headroom, and adjust white-point -- all in one pass."
        )

    # ----- round-trip verification -----

    def verify_roundtrip(
        self,
        n_samples: int = 256,
        tolerance: float = 1e-4,
    ) -> Tuple[bool, float]:
        """
        Verify that compositor -> display -> compositor round-trips
        within *tolerance*.

        Returns
        -------
        (passed, max_error)
            Whether all samples round-tripped within tolerance and the
            maximum absolute error.
        """
        originals = np.linspace(0.0, 1.0, n_samples)
        encoded = self.compositor_to_display(originals)
        decoded = self.display_to_compositor(encoded)
        errors = np.abs(decoded - originals)
        max_err = float(np.max(errors))
        return max_err <= tolerance, max_err

    # ----- repr -----

    def __repr__(self) -> str:
        mode = "HDR" if self.hdr_enabled else "SDR"
        lut = "LUT active" if self.lut_applied else "no LUT"
        return (
            f"ScRGBPipelineModel(mode={mode}, sdr_white={self.sdr_white_nits} nits, "
            f"peak={self.peak_nits} nits, headroom={self.headroom:.1f}x / "
            f"{self.headroom_stops:.1f} stops, {lut})"
        )
