"""
HDR Content Type Detection (Phase 2)

Identifies the type of HDR (or SDR) content from metadata and recommends
the appropriate calibration LUT for each content type.

Supported content types:
    SDR          - Standard Dynamic Range (BT.709 / sRGB)
    HDR10        - Static HDR metadata, PQ transfer, BT.2020 primaries
    HDR10+       - Dynamic HDR metadata (Samsung), PQ transfer, BT.2020
    HLG          - Hybrid Log-Gamma (broadcast), BT.2020
    DolbyVision  - Dolby Vision (dynamic metadata, IPT-PQ core)
    scRGB        - Windows scRGB linear (apps using IDXGISwapChain FP16)
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, Optional


# ---------------------------------------------------------------------------
# Content information
# ---------------------------------------------------------------------------

@dataclass
class HDRContentInfo:
    """Description of an HDR (or SDR) content stream."""

    content_type: str
    """One of: ``"SDR"``, ``"HDR10"``, ``"HDR10+"``, ``"HLG"``,
    ``"DolbyVision"``, ``"scRGB"``."""

    peak_luminance: float
    """Content's declared peak luminance in cd/m2.
    For SDR this is typically 100 (reference) or 0 if unknown.
    For HDR10/HDR10+ this comes from the MaxCLL metadata field.
    """

    color_primaries: str
    """Color primaries: ``"BT.709"``, ``"BT.2020"``, ``"P3"``."""

    transfer_function: str
    """Transfer function / EOTF: ``"sRGB"``, ``"PQ"``, ``"HLG"``,
    ``"Linear"``."""

    # Optional extended fields
    max_fall: float = 0.0
    """Maximum Frame-Average Light Level (MaxFALL) in cd/m2."""

    dynamic_metadata: bool = False
    """Whether the content carries dynamic (per-scene / per-frame) metadata."""

    mastering_display_peak: float = 0.0
    """Mastering display peak luminance from SMPTE ST.2086 metadata (cd/m2)."""

    mastering_display_min: float = 0.0
    """Mastering display minimum luminance from ST.2086 (cd/m2)."""

    mastering_primaries: str = ""
    """Mastering display colour primaries identifier (e.g. ``"P3-D65"``)."""


# ---------------------------------------------------------------------------
# Detection helpers
# ---------------------------------------------------------------------------

# Known transfer function keywords -> canonical names
_TF_MAP = {
    "srgb": "sRGB",
    "bt709": "sRGB",
    "gamma22": "sRGB",
    "gamma2.2": "sRGB",
    "pq": "PQ",
    "st2084": "PQ",
    "st.2084": "PQ",
    "smpte2084": "PQ",
    "hlg": "HLG",
    "arib-std-b67": "HLG",
    "linear": "Linear",
    "scrgb": "Linear",
}

# Known primaries keywords -> canonical names
_PRIMARIES_MAP = {
    "bt709": "BT.709",
    "bt.709": "BT.709",
    "srgb": "BT.709",
    "bt2020": "BT.2020",
    "bt.2020": "BT.2020",
    "rec2020": "BT.2020",
    "rec.2020": "BT.2020",
    "p3": "P3",
    "dci-p3": "P3",
    "dcip3": "P3",
    "display-p3": "P3",
    "p3-d65": "P3",
}


def _normalise_tf(raw: str) -> str:
    """Normalise a transfer-function string to a canonical name."""
    key = raw.strip().lower().replace(" ", "").replace("_", "")
    return _TF_MAP.get(key, raw)


def _normalise_primaries(raw: str) -> str:
    """Normalise a colour-primaries string to a canonical name."""
    key = raw.strip().lower().replace(" ", "").replace("_", "")
    return _PRIMARIES_MAP.get(key, raw)


def detect_content_type_from_metadata(metadata: Dict[str, Any]) -> HDRContentInfo:
    """
    Detect content type from HDR metadata dictionary.

    The *metadata* dict may contain any subset of the following keys
    (case-insensitive matching is applied):

    - ``transfer_function`` / ``eotf`` / ``tf``
    - ``color_primaries`` / ``primaries``
    - ``max_cll`` / ``peak_luminance`` / ``maxcll``
    - ``max_fall`` / ``maxfall``
    - ``dynamic_metadata`` (bool)
    - ``dolby_vision`` / ``dv`` (bool)
    - ``hdr10plus`` / ``hdr10+`` (bool)
    - ``mastering_display_luminance`` (tuple of (min, max) nits)
    - ``mastering_primaries``

    Parameters
    ----------
    metadata : dict
        Metadata about the content stream.

    Returns
    -------
    HDRContentInfo
        Detected content information.
    """
    # --- build a lower-cased copy for flexible key matching ---
    md: Dict[str, Any] = {k.lower().replace(" ", "_"): v for k, v in metadata.items()}

    # Transfer function
    raw_tf = (
        md.get("transfer_function")
        or md.get("eotf")
        or md.get("tf")
        or ""
    )
    tf = _normalise_tf(str(raw_tf)) if raw_tf else ""

    # Color primaries
    raw_pri = md.get("color_primaries") or md.get("primaries") or ""
    primaries = _normalise_primaries(str(raw_pri)) if raw_pri else ""

    # Peak luminance
    peak = float(
        md.get("max_cll")
        or md.get("peak_luminance")
        or md.get("maxcll")
        or 0
    )

    # MaxFALL
    max_fall = float(md.get("max_fall") or md.get("maxfall") or 0)

    # Dynamic metadata flags
    has_dynamic = bool(md.get("dynamic_metadata", False))
    has_dv = bool(md.get("dolby_vision") or md.get("dv"))
    has_hdr10plus = bool(md.get("hdr10plus") or md.get("hdr10+"))

    # Mastering display
    mast_lum = md.get("mastering_display_luminance", (0, 0))
    if isinstance(mast_lum, (list, tuple)) and len(mast_lum) >= 2:
        mast_min, mast_max = float(mast_lum[0]), float(mast_lum[1])
    else:
        mast_min, mast_max = 0.0, 0.0
    mast_pri = str(md.get("mastering_primaries", ""))

    # --- determine content type ---
    content_type: str

    if has_dv:
        content_type = "DolbyVision"
        if not tf:
            tf = "PQ"
        if not primaries:
            primaries = "BT.2020"
        has_dynamic = True
    elif has_hdr10plus:
        content_type = "HDR10+"
        if not tf:
            tf = "PQ"
        if not primaries:
            primaries = "BT.2020"
        has_dynamic = True
    elif tf == "HLG":
        content_type = "HLG"
        if not primaries:
            primaries = "BT.2020"
    elif tf == "Linear":
        content_type = "scRGB"
        if not primaries:
            primaries = "BT.709"
    elif tf == "PQ":
        content_type = "HDR10"
        if not primaries:
            primaries = "BT.2020"
    else:
        # Default: SDR
        content_type = "SDR"
        if not tf:
            tf = "sRGB"
        if not primaries:
            primaries = "BT.709"
        if peak == 0:
            peak = 100.0  # SDR reference white

    return HDRContentInfo(
        content_type=content_type,
        peak_luminance=peak,
        color_primaries=primaries,
        transfer_function=tf,
        max_fall=max_fall,
        dynamic_metadata=has_dynamic,
        mastering_display_peak=mast_max,
        mastering_display_min=mast_min,
        mastering_primaries=mast_pri,
    )


# ---------------------------------------------------------------------------
# LUT recommendation
# ---------------------------------------------------------------------------

def get_recommended_lut_for_content(
    content: HDRContentInfo,
    panel: Optional[Any] = None,
) -> str:
    """
    Recommend which calibration LUT to use for the given content type.

    The recommendation depends on:
    1. The content's transfer function (PQ vs HLG vs sRGB vs Linear).
    2. Whether Windows HDR mode is active (inferred from panel info).
    3. The panel's capabilities (peak luminance, gamut).

    Parameters
    ----------
    content : HDRContentInfo
        Content description.
    panel : object, optional
        A panel info object (e.g. from ``PanelDatabase.get_panel()``).
        If provided, peak luminance and gamut are used to refine the
        recommendation.

    Returns
    -------
    str
        LUT name / identifier to use.  Possible values:

        - ``"sdr"`` -- standard SDR calibration LUT (sRGB target)
        - ``"hdr_pq"`` -- HDR10 / HDR10+ PQ-based LUT
        - ``"hdr_hlg"`` -- HLG-specific LUT (broadcast content)
        - ``"hdr_dv"`` -- Dolby Vision LUT (if supported)
        - ``"hdr_scrgb"`` -- scRGB linear HDR LUT (Windows desktop HDR apps)
        - ``"hdr_tonemapped"`` -- tone-mapped HDR LUT for panels that
          cannot reach content peak
    """

    # --- Panel peak luminance (for tone-mapping decision) ---
    panel_peak: float = 0.0
    if panel is not None:
        # Accept either an object with a capabilities attribute or a plain float
        if hasattr(panel, "capabilities"):
            cap = panel.capabilities
            panel_peak = getattr(cap, "max_luminance_hdr", 0.0)
        elif isinstance(panel, (int, float)):
            panel_peak = float(panel)

    ct = content.content_type

    if ct == "SDR":
        return "sdr"

    if ct == "scRGB":
        return "hdr_scrgb"

    if ct == "HLG":
        return "hdr_hlg"

    if ct == "DolbyVision":
        return "hdr_dv"

    # HDR10 or HDR10+
    # If the panel cannot reach the content's declared peak, a tone-mapped
    # LUT is preferred so that highlights are gracefully compressed rather
    # than clipped.
    if panel_peak > 0 and content.peak_luminance > 0:
        if content.peak_luminance > panel_peak * 1.1:
            return "hdr_tonemapped"

    return "hdr_pq"


# ---------------------------------------------------------------------------
# Convenience: build HDRContentInfo from common shorthand
# ---------------------------------------------------------------------------

def content_info_from_type(
    content_type: str,
    peak_luminance: float = 0.0,
) -> HDRContentInfo:
    """
    Build an :class:`HDRContentInfo` from a short content-type string.

    Parameters
    ----------
    content_type : str
        One of ``"SDR"``, ``"HDR10"``, ``"HDR10+"``, ``"HLG"``,
        ``"DolbyVision"``, ``"scRGB"``.
    peak_luminance : float
        Peak luminance override (cd/m2).

    Returns
    -------
    HDRContentInfo
    """
    ct = content_type.strip()

    defaults = {
        "SDR": ("sRGB", "BT.709", 100.0),
        "HDR10": ("PQ", "BT.2020", 1000.0),
        "HDR10+": ("PQ", "BT.2020", 1000.0),
        "HLG": ("HLG", "BT.2020", 1000.0),
        "DolbyVision": ("PQ", "BT.2020", 4000.0),
        "scRGB": ("Linear", "BT.709", 0.0),
    }

    tf, pri, default_peak = defaults.get(ct, ("sRGB", "BT.709", 100.0))
    if peak_luminance <= 0:
        peak_luminance = default_peak

    return HDRContentInfo(
        content_type=ct,
        peak_luminance=peak_luminance,
        color_primaries=pri,
        transfer_function=tf,
        dynamic_metadata=ct in ("HDR10+", "DolbyVision"),
    )
