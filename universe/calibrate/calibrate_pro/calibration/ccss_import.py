"""
CCSS / CCMX Colorimeter Correction Import

Parses CGATS-format colorimeter correction files used by ArgyllCMS,
DisplayCAL and other calibration tools:

- **CCMX** (Colorimeter Correction Matrix): Contains a 3x3 matrix that
  corrects a colorimeter's XYZ readings for a specific display technology.
  The matrix is derived by comparing colorimeter measurements to a
  reference spectrophotometer on the same display.

- **CCSS** (Colorimeter Calibration Spectral Sample): Contains spectral
  power distribution (SPD) data measured from a display.  A colorimeter
  driver can use this to compute per-channel integration weights that
  match the display's emission spectrum, giving more accurate XYZ
  without needing a full 3x3 correction.

Both formats use the CGATS ASCII text structure with keyword/value
headers and a BEGIN_DATA / END_DATA table section.

Usage::

    from calibrate_pro.calibration.ccss_import import (
        load_ccmx, load_ccss, apply_ccmx, list_builtin_corrections,
    )

    ccmx = load_ccmx("Samsung_QD-OLED.ccmx")     # -> (3, 3) ndarray
    corrected_xyz = apply_ccmx(raw_xyz, ccmx)

    spectral = load_ccss("OLED_spectral.ccss")     # -> dict
    wavelengths = spectral["wavelengths"]           # nm values
    spd_samples = spectral["samples"]               # list of SPD arrays
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Union

import numpy as np


# ---------------------------------------------------------------------------
# CGATS parsing helpers
# ---------------------------------------------------------------------------

def _parse_cgats_keywords(text: str) -> Dict[str, str]:
    """
    Extract KEYWORD/value pairs from CGATS header lines.

    Handles both quoted and unquoted values::

        KEYWORD "DISPLAY"
        DISPLAY "Samsung QD-OLED"
        TECHNOLOGY "OLED"

    Returns a dict mapping uppercase keyword names to their string values
    (quotes stripped).
    """
    keywords: Dict[str, str] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        # Match: KEY "value"  or  KEY value
        m = re.match(r'^(\w+)\s+"(.+)"', line)
        if m:
            keywords[m.group(1).upper()] = m.group(2)
        else:
            m = re.match(r'^(\w+)\s+(.+)', line)
            if m:
                key = m.group(1).upper()
                val = m.group(2).strip().strip('"')
                keywords[key] = val
    return keywords


def _extract_data_block(text: str) -> Tuple[List[str], List[List[str]]]:
    """
    Extract the field names and data rows between BEGIN_DATA_FORMAT /
    END_DATA_FORMAT and BEGIN_DATA / END_DATA markers.

    Returns:
        (fields, rows) where fields is a list of column names and rows
        is a list of lists of string tokens.
    """
    lines = text.splitlines()

    # --- field names ---
    fields: List[str] = []
    in_format = False
    for line in lines:
        stripped = line.strip()
        if stripped == "BEGIN_DATA_FORMAT":
            in_format = True
            continue
        if stripped == "END_DATA_FORMAT":
            in_format = False
            continue
        if in_format and stripped:
            fields.extend(stripped.split())

    # --- data rows ---
    rows: List[List[str]] = []
    in_data = False
    for line in lines:
        stripped = line.strip()
        if stripped == "BEGIN_DATA":
            in_data = True
            continue
        if stripped == "END_DATA":
            in_data = False
            continue
        if in_data and stripped:
            rows.append(stripped.split())

    return fields, rows


# ---------------------------------------------------------------------------
# CCMX loading
# ---------------------------------------------------------------------------

def load_ccmx(path: Union[str, Path]) -> np.ndarray:
    """
    Load a CCMX (Colorimeter Correction Matrix) file.

    A CCMX file contains a 3x3 matrix in its data section.  The three
    rows map uncorrected XYZ to corrected XYZ::

        corrected_XYZ = CCMX @ raw_XYZ

    Args:
        path: Path to the ``.ccmx`` file.

    Returns:
        A (3, 3) numpy float64 array -- the correction matrix.

    Raises:
        ValueError: If the file cannot be parsed or does not contain
            exactly 3 rows of 3 numeric values.
    """
    path = Path(path)
    text = path.read_text(encoding="utf-8", errors="replace")

    _, rows = _extract_data_block(text)

    if len(rows) < 3:
        raise ValueError(
            f"CCMX file must contain at least 3 data rows, got {len(rows)}"
        )

    matrix_rows: List[List[float]] = []
    for row in rows[:3]:
        # Some CCMX files include a row index as the first column; detect
        # and skip non-numeric leading tokens.
        floats: List[float] = []
        for token in row:
            try:
                floats.append(float(token))
            except ValueError:
                continue
        if len(floats) < 3:
            raise ValueError(
                f"Expected at least 3 numeric values per row, got {len(floats)}: {row}"
            )
        # Take the last 3 values (handles optional leading index column)
        matrix_rows.append(floats[-3:])

    return np.array(matrix_rows, dtype=np.float64)


# ---------------------------------------------------------------------------
# CCSS loading
# ---------------------------------------------------------------------------

def load_ccss(path: Union[str, Path]) -> dict:
    """
    Load a CCSS (Colorimeter Calibration Spectral Sample) file.

    CCSS files contain spectral power distribution measurements taken
    from a display.  The data section has columns for ``SAMPLE_ID``,
    ``SPEC_<wavelength>`` values (e.g. ``SPEC_380``, ``SPEC_390``, ...
    ``SPEC_730``).

    Args:
        path: Path to the ``.ccss`` file.

    Returns:
        A dict with the following keys:

        - ``"display"`` -- Display name (str or ``None``).
        - ``"technology"`` -- Display technology string (str or ``None``).
        - ``"reference"`` -- Reference instrument (str or ``None``).
        - ``"wavelengths"`` -- 1-D numpy array of wavelength values in nm.
        - ``"samples"`` -- List of 1-D numpy arrays, one SPD per sample.
        - ``"num_samples"`` -- Number of spectral samples (int).
        - ``"keywords"`` -- Full dict of parsed CGATS keywords.

    Raises:
        ValueError: If no spectral columns are found.
    """
    path = Path(path)
    text = path.read_text(encoding="utf-8", errors="replace")

    keywords = _parse_cgats_keywords(text)
    fields, rows = _extract_data_block(text)

    # Identify spectral columns: SPEC_380, SPEC_390, etc.
    spec_indices: List[Tuple[int, float]] = []
    for i, field in enumerate(fields):
        m = re.match(r"^SPEC_(\d+(?:\.\d+)?)$", field, re.IGNORECASE)
        if m:
            spec_indices.append((i, float(m.group(1))))

    if not spec_indices:
        raise ValueError(
            "No SPEC_* columns found in CCSS file. "
            f"Fields: {fields}"
        )

    # Sort by wavelength
    spec_indices.sort(key=lambda x: x[1])
    wavelengths = np.array([wl for _, wl in spec_indices], dtype=np.float64)
    col_indices = [idx for idx, _ in spec_indices]

    # Extract spectral data from each row
    samples: List[np.ndarray] = []
    for row in rows:
        spd_values: List[float] = []
        for ci in col_indices:
            if ci < len(row):
                try:
                    spd_values.append(float(row[ci]))
                except ValueError:
                    spd_values.append(0.0)
            else:
                spd_values.append(0.0)
        samples.append(np.array(spd_values, dtype=np.float64))

    return {
        "display": keywords.get("DISPLAY"),
        "technology": keywords.get("TECHNOLOGY"),
        "reference": keywords.get("REFERENCE"),
        "wavelengths": wavelengths,
        "samples": samples,
        "num_samples": len(samples),
        "keywords": keywords,
    }


# ---------------------------------------------------------------------------
# Applying corrections
# ---------------------------------------------------------------------------

def apply_ccmx(xyz: np.ndarray, ccmx: np.ndarray) -> np.ndarray:
    """
    Apply a CCMX correction matrix to XYZ measurements.

    Supports single triplets, batches, and images::

        corrected = apply_ccmx(raw_xyz, ccmx)

    Args:
        xyz: Input XYZ values.  Shape ``(3,)``, ``(N, 3)``, or
            ``(H, W, 3)``.
        ccmx: 3x3 correction matrix (from :func:`load_ccmx`).

    Returns:
        Corrected XYZ array with the same shape as *xyz*.
    """
    xyz = np.asarray(xyz, dtype=np.float64)
    ccmx = np.asarray(ccmx, dtype=np.float64)

    if ccmx.shape != (3, 3):
        raise ValueError(f"CCMX must be (3, 3), got {ccmx.shape}")

    original_shape = xyz.shape

    if xyz.ndim == 1:
        return ccmx @ xyz
    elif xyz.ndim == 2:
        # (N, 3) -> transpose, multiply, transpose back
        return (ccmx @ xyz.T).T
    elif xyz.ndim == 3:
        h, w, _ = xyz.shape
        flat = xyz.reshape(-1, 3)
        corrected = (ccmx @ flat.T).T
        return corrected.reshape(h, w, 3)
    else:
        raise ValueError(f"Unsupported xyz shape: {xyz.shape}")


# ---------------------------------------------------------------------------
# Built-in corrections
# ---------------------------------------------------------------------------

def _get_qdoled_ccmx() -> np.ndarray:
    """
    Return the built-in QD-OLED CCMX from the native calibration loop.

    This is the correction matrix computed for i1Display3 + QD-OLED
    (PG27UCDM) by comparing sensor-reported primaries against EDID
    primaries.
    """
    from calibrate_pro.calibration.native_loop import QDOLED_CCMX
    return QDOLED_CCMX.copy()


# Registry of built-in corrections
_BUILTIN_CORRECTIONS = {
    "QD-OLED (i1Display3 - PG27UCDM)": {
        "loader": _get_qdoled_ccmx,
        "display": "Samsung QD-OLED (PG27UCDM)",
        "technology": "QD-OLED",
        "reference": "i1 Pro (via EDID primaries)",
        "colorimeter": "i1Display3 (OLED EEPROM matrix)",
    },
}


def list_builtin_corrections() -> list:
    """
    List available built-in CCMX corrections.

    Returns:
        A list of dicts, each containing:

        - ``"name"`` -- Human-readable correction name.
        - ``"display"`` -- Display model / technology.
        - ``"technology"`` -- Panel technology (e.g. QD-OLED).
        - ``"reference"`` -- Reference instrument or source.
        - ``"colorimeter"`` -- Colorimeter the correction applies to.
    """
    result = []
    for name, info in _BUILTIN_CORRECTIONS.items():
        result.append({
            "name": name,
            "display": info["display"],
            "technology": info["technology"],
            "reference": info["reference"],
            "colorimeter": info["colorimeter"],
        })
    return result


def get_builtin_ccmx(name: str) -> np.ndarray:
    """
    Retrieve a built-in CCMX correction matrix by name.

    Args:
        name: Correction name as returned by :func:`list_builtin_corrections`.

    Returns:
        A (3, 3) numpy float64 array.

    Raises:
        KeyError: If the name is not found.
    """
    if name not in _BUILTIN_CORRECTIONS:
        available = list(_BUILTIN_CORRECTIONS.keys())
        raise KeyError(
            f"Unknown built-in correction '{name}'. "
            f"Available: {available}"
        )
    return _BUILTIN_CORRECTIONS[name]["loader"]()
