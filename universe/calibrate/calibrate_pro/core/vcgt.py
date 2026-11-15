"""
VCGT (Video Card Gamma Table) Tools

Provides conversion between 3D LUTs and 1D VCGT tables for ICC profiles,
as well as standalone VCGT export for GPU loading.

VCGT tables are 1D LUTs that:
- Are stored in ICC profiles (vcgt tag)
- Apply gamma/grayscale corrections
- Work at the GPU level before display
- Are limited to per-channel 1D corrections (no color crosstalk)

Usage:
    # Convert 3D LUT to VCGT
    vcgt = lut3d_to_vcgt(lut3d_data, size=4096)

    # Export as standalone file
    export_vcgt_cal(vcgt, "calibration.cal")
    export_vcgt_csv(vcgt, "calibration.csv")

    # Embed in ICC profile
    embed_vcgt_in_profile(vcgt, "profile.icc")
"""

import numpy as np
from typing import Tuple, Optional, List, Dict, Any
from dataclasses import dataclass
from pathlib import Path
import struct


@dataclass
class VCGTTable:
    """
    Video Card Gamma Table data structure.

    Attributes:
        red: Red channel LUT (0.0-1.0 values, typically 256-4096 entries)
        green: Green channel LUT
        blue: Blue channel LUT
        size: Number of entries per channel
        bit_depth: Original bit depth (8, 10, 12, 16)
    """
    red: np.ndarray
    green: np.ndarray
    blue: np.ndarray
    size: int = 256
    bit_depth: int = 16

    @property
    def channels(self) -> np.ndarray:
        """Return all channels as (3, size) array."""
        return np.vstack([self.red, self.green, self.blue])

    def to_integers(self, bit_depth: int = 16) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
        """Convert to integer values for the specified bit depth."""
        max_val = (1 << bit_depth) - 1
        r = np.clip(self.red * max_val, 0, max_val).astype(np.uint16 if bit_depth <= 16 else np.uint32)
        g = np.clip(self.green * max_val, 0, max_val).astype(np.uint16 if bit_depth <= 16 else np.uint32)
        b = np.clip(self.blue * max_val, 0, max_val).astype(np.uint16 if bit_depth <= 16 else np.uint32)
        return r, g, b


def lut3d_to_vcgt(
    lut3d: np.ndarray,
    output_size: int = 4096,
    method: str = "neutral_axis"
) -> VCGTTable:
    """
    Convert a 3D LUT to a 1D VCGT table.

    This extracts the grayscale response from the 3D LUT by sampling
    along the neutral axis (where R=G=B).

    Args:
        lut3d: 3D LUT as (size, size, size, 3) numpy array
        output_size: Number of entries in output VCGT (256, 1024, 4096)
        method: Extraction method:
            - "neutral_axis": Sample where R=G=B (most accurate for grayscale)
            - "channel_average": Average per-channel response
            - "luminance_weighted": Weight by luminance contribution

    Returns:
        VCGTTable with extracted 1D curves
    """
    lut_size = lut3d.shape[0]

    if method == "neutral_axis":
        # Sample the 3D LUT along the neutral axis
        indices = np.linspace(0, lut_size - 1, output_size)

        # Interpolate along neutral axis
        red = np.zeros(output_size)
        green = np.zeros(output_size)
        blue = np.zeros(output_size)

        for i, idx in enumerate(indices):
            # Trilinear interpolation at neutral position
            idx_low = int(idx)
            idx_high = min(idx_low + 1, lut_size - 1)
            frac = idx - idx_low

            # Sample at (idx, idx, idx) with interpolation
            val_low = lut3d[idx_low, idx_low, idx_low]
            val_high = lut3d[idx_high, idx_high, idx_high]
            val = val_low * (1 - frac) + val_high * frac

            red[i] = val[0]
            green[i] = val[1]
            blue[i] = val[2]

    elif method == "channel_average":
        # Average the response for each channel independently
        indices = np.linspace(0, lut_size - 1, output_size).astype(int)

        red = np.zeros(output_size)
        green = np.zeros(output_size)
        blue = np.zeros(output_size)

        for i, idx in enumerate(indices):
            # Average all values where this channel has this input
            red[i] = np.mean(lut3d[idx, :, :, 0])
            green[i] = np.mean(lut3d[:, idx, :, 1])
            blue[i] = np.mean(lut3d[:, :, idx, 2])

    elif method == "luminance_weighted":
        # Weight by Rec.709 luminance coefficients
        indices = np.linspace(0, lut_size - 1, output_size)

        red = np.zeros(output_size)
        green = np.zeros(output_size)
        blue = np.zeros(output_size)

        # Rec.709 coefficients
        wr, wg, wb = 0.2126, 0.7152, 0.0722

        for i, idx in enumerate(indices):
            idx_int = int(idx)
            idx_int = min(idx_int, lut_size - 1)

            # Sample along weighted neutral
            r_idx = int(idx * wr / (wr + wg + wb) * 3)
            g_idx = int(idx * wg / (wr + wg + wb) * 3)
            b_idx = int(idx * wb / (wr + wg + wb) * 3)

            r_idx = min(max(r_idx, 0), lut_size - 1)
            g_idx = min(max(g_idx, 0), lut_size - 1)
            b_idx = min(max(b_idx, 0), lut_size - 1)

            val = lut3d[idx_int, idx_int, idx_int]
            red[i] = val[0]
            green[i] = val[1]
            blue[i] = val[2]

    else:
        raise ValueError(f"Unknown method: {method}")

    return VCGTTable(
        red=np.clip(red, 0, 1),
        green=np.clip(green, 0, 1),
        blue=np.clip(blue, 0, 1),
        size=output_size,
        bit_depth=16
    )


def gamma_to_vcgt(
    gamma: float = 2.2,
    output_size: int = 256,
    rgb_gains: Tuple[float, float, float] = (1.0, 1.0, 1.0),
    black_level: float = 0.0
) -> VCGTTable:
    """
    Generate a VCGT table from gamma and gain parameters.

    Args:
        gamma: Power law gamma value
        output_size: Number of table entries
        rgb_gains: Per-channel gain multipliers for white balance
        black_level: Black level offset (0.0-1.0)

    Returns:
        VCGTTable with computed curves
    """
    x = np.linspace(0, 1, output_size)

    # Apply gamma with black level offset
    # output = black_level + (1 - black_level) * input^gamma
    base_curve = black_level + (1 - black_level) * np.power(x, gamma)

    red = np.clip(base_curve * rgb_gains[0], 0, 1)
    green = np.clip(base_curve * rgb_gains[1], 0, 1)
    blue = np.clip(base_curve * rgb_gains[2], 0, 1)

    return VCGTTable(red=red, green=green, blue=blue, size=output_size)


def srgb_vcgt(output_size: int = 256) -> VCGTTable:
    """Generate VCGT for sRGB transfer function."""
    x = np.linspace(0, 1, output_size)

    # sRGB EOTF (electrical to optical)
    curve = np.where(
        x <= 0.04045,
        x / 12.92,
        np.power((x + 0.055) / 1.055, 2.4)
    )

    return VCGTTable(red=curve, green=curve, blue=curve, size=output_size)


def bt1886_vcgt(
    output_size: int = 256,
    gamma: float = 2.4,
    Lw: float = 100.0,  # White luminance
    Lb: float = 0.1     # Black luminance
) -> VCGTTable:
    """
    Generate VCGT for BT.1886 transfer function.

    BT.1886 is the EOTF for broadcast video, accounting for
    display black level.
    """
    x = np.linspace(0, 1, output_size)

    # BT.1886 formula
    a = np.power(np.power(Lw, 1/gamma) - np.power(Lb, 1/gamma), gamma)
    b = np.power(Lb, 1/gamma) / (np.power(Lw, 1/gamma) - np.power(Lb, 1/gamma))

    curve = np.power(np.maximum(x + b, 0), gamma) / a
    curve = np.clip(curve, 0, 1)

    return VCGTTable(red=curve, green=curve, blue=curve, size=output_size)


# =============================================================================
# Export Functions
# =============================================================================

def export_vcgt_cal(vcgt: VCGTTable, filepath: str):
    """
    Export VCGT as ArgyllCMS .cal file format.

    This format can be loaded by dispwin and other tools.
    """
    path = Path(filepath)

    with open(path, 'w') as f:
        f.write("CAL\n\n")
        f.write("DESCRIPTOR \"Calibrate Pro VCGT Export\"\n")
        f.write("ORIGINATOR \"Calibrate Pro\"\n")
        f.write("CREATED \"\"\n")
        f.write("KEYWORD \"DEVICE_CLASS\"\n")
        f.write("DEVICE_CLASS \"DISPLAY\"\n")
        f.write("KEYWORD \"COLOR_REP\"\n")
        f.write("COLOR_REP \"RGB\"\n\n")
        f.write(f"NUMBER_OF_FIELDS 4\n")
        f.write("BEGIN_DATA_FORMAT\n")
        f.write("RGB_I RGB_R RGB_G RGB_B\n")
        f.write("END_DATA_FORMAT\n\n")
        f.write(f"NUMBER_OF_SETS {vcgt.size}\n")
        f.write("BEGIN_DATA\n")

        for i in range(vcgt.size):
            input_val = i / (vcgt.size - 1)
            f.write(f"{input_val:.6f} {vcgt.red[i]:.6f} {vcgt.green[i]:.6f} {vcgt.blue[i]:.6f}\n")

        f.write("END_DATA\n")


def export_vcgt_csv(vcgt: VCGTTable, filepath: str, include_header: bool = True):
    """Export VCGT as CSV file."""
    path = Path(filepath)

    with open(path, 'w') as f:
        if include_header:
            f.write("Input,Red,Green,Blue\n")

        for i in range(vcgt.size):
            input_val = i / (vcgt.size - 1)
            f.write(f"{input_val:.6f},{vcgt.red[i]:.6f},{vcgt.green[i]:.6f},{vcgt.blue[i]:.6f}\n")


def export_vcgt_cube1d(vcgt: VCGTTable, filepath: str, title: str = "VCGT Export"):
    """
    Export VCGT as .cube 1D LUT format.

    Note: Standard .cube format is for 3D LUTs, but 1D is often supported.
    """
    path = Path(filepath)

    with open(path, 'w') as f:
        f.write(f"TITLE \"{title}\"\n")
        f.write(f"LUT_1D_SIZE {vcgt.size}\n")
        f.write("DOMAIN_MIN 0.0 0.0 0.0\n")
        f.write("DOMAIN_MAX 1.0 1.0 1.0\n\n")

        for i in range(vcgt.size):
            f.write(f"{vcgt.red[i]:.6f} {vcgt.green[i]:.6f} {vcgt.blue[i]:.6f}\n")


def export_vcgt_icc_bytes(vcgt: VCGTTable) -> bytes:
    """
    Generate ICC profile vcgt tag data.

    Returns bytes that can be embedded in an ICC profile.
    """
    # vcgt tag structure:
    # 4 bytes: 'vcgt' signature
    # 4 bytes: reserved (0)
    # 4 bytes: tag type (0 = table, 1 = formula)
    # 2 bytes: number of channels (3)
    # 2 bytes: number of entries per channel
    # 2 bytes: entry size in bytes (2 for 16-bit)
    # Then the actual table data (interleaved R, G, B)

    data = bytearray()

    # Signature
    data.extend(b'vcgt')

    # Reserved
    data.extend(struct.pack('>I', 0))

    # Tag type (0 = table)
    data.extend(struct.pack('>I', 0))

    # Number of channels
    data.extend(struct.pack('>H', 3))

    # Number of entries
    data.extend(struct.pack('>H', vcgt.size))

    # Entry size (2 bytes = 16-bit)
    data.extend(struct.pack('>H', 2))

    # Table data (16-bit values, big-endian)
    r_int, g_int, b_int = vcgt.to_integers(16)

    for i in range(vcgt.size):
        data.extend(struct.pack('>H', r_int[i]))
        data.extend(struct.pack('>H', g_int[i]))
        data.extend(struct.pack('>H', b_int[i]))

    return bytes(data)


# =============================================================================
# Import Functions
# =============================================================================

def import_vcgt_cal(filepath: str) -> VCGTTable:
    """Import VCGT from ArgyllCMS .cal file."""
    path = Path(filepath)

    red = []
    green = []
    blue = []
    in_data = False

    with open(path, 'r') as f:
        for line in f:
            line = line.strip()

            if line == "BEGIN_DATA":
                in_data = True
                continue
            elif line == "END_DATA":
                break
            elif in_data and line:
                parts = line.split()
                if len(parts) >= 4:
                    red.append(float(parts[1]))
                    green.append(float(parts[2]))
                    blue.append(float(parts[3]))

    return VCGTTable(
        red=np.array(red),
        green=np.array(green),
        blue=np.array(blue),
        size=len(red)
    )


def import_vcgt_csv(filepath: str, has_header: bool = True) -> VCGTTable:
    """Import VCGT from CSV file."""
    path = Path(filepath)

    red = []
    green = []
    blue = []

    with open(path, 'r') as f:
        lines = f.readlines()

        start = 1 if has_header else 0
        for line in lines[start:]:
            parts = line.strip().split(',')
            if len(parts) >= 4:
                red.append(float(parts[1]))
                green.append(float(parts[2]))
                blue.append(float(parts[3]))

    return VCGTTable(
        red=np.array(red),
        green=np.array(green),
        blue=np.array(blue),
        size=len(red)
    )


# =============================================================================
# VCGT Application (Windows)
# =============================================================================

def apply_vcgt_windows(
    vcgt: VCGTTable,
    display_index: int = 0,
    device_name: str = ""
) -> bool:
    """
    Apply VCGT to Windows display gamma ramp.

    This directly modifies the GPU gamma ramp for immediate effect.
    Does NOT require admin privileges.

    For non-primary displays, either provide a ``device_name``
    (e.g. ``'\\\\.\\DISPLAY2'``) or a ``display_index`` which will be
    resolved to a device name automatically.

    Args:
        vcgt: VCGT table to apply
        display_index: Display to apply to (0 = primary). Used only when
            ``device_name`` is not provided.
        device_name: Windows device name such as ``'\\\\.\\DISPLAY1'``.
            When provided, ``display_index`` is ignored.

    Returns:
        True if successful
    """
    try:
        import ctypes
        from ctypes import wintypes

        user32 = ctypes.windll.user32
        gdi32 = ctypes.windll.gdi32

        # --- Obtain a device context for the correct display ---------------
        hdc = None
        release_fn = None  # callable(hdc) to release when done

        # Resolve device_name from display_index if not given
        if not device_name and display_index > 0:
            device_name = _resolve_display_device_name(display_index)

        if device_name:
            # CreateDCW gives us a DC for a specific display adapter
            hdc = user32.CreateDCW("DISPLAY", device_name, None, None)
            if hdc:
                release_fn = lambda h: user32.DeleteDC(h)

        if not hdc:
            # Fallback: primary display DC
            hdc = user32.GetDC(None)
            if not hdc:
                return False
            release_fn = lambda h: user32.ReleaseDC(None, h)

        try:
            # Prepare gamma ramp (256 entries, 16-bit per channel)
            if vcgt.size != 256:
                x_old = np.linspace(0, 1, vcgt.size)
                x_new = np.linspace(0, 1, 256)
                red = np.interp(x_new, x_old, vcgt.red)
                green = np.interp(x_new, x_old, vcgt.green)
                blue = np.interp(x_new, x_old, vcgt.blue)
            else:
                red = vcgt.red
                green = vcgt.green
                blue = vcgt.blue

            # Convert to 16-bit integers
            ramp = (wintypes.WORD * 256 * 3)()

            for i in range(256):
                ramp[0][i] = int(np.clip(red[i] * 65535, 0, 65535))
                ramp[1][i] = int(np.clip(green[i] * 65535, 0, 65535))
                ramp[2][i] = int(np.clip(blue[i] * 65535, 0, 65535))

            # Apply gamma ramp
            result = gdi32.SetDeviceGammaRamp(hdc, ctypes.byref(ramp))
            return bool(result)

        finally:
            if release_fn:
                release_fn(hdc)

    except Exception:
        return False


def _resolve_display_device_name(display_index: int) -> str:
    """
    Map a 0-based display index to a Windows device name.

    Uses EnumDisplayDevicesW to walk the adapter list and return the
    device name for the Nth active adapter.

    Args:
        display_index: 0-based index

    Returns:
        Device name string, or "" if not found.
    """
    try:
        import ctypes
        from ctypes import wintypes

        class _DISPLAY_DEVICE(ctypes.Structure):
            _fields_ = [
                ("cb", wintypes.DWORD),
                ("DeviceName", wintypes.WCHAR * 32),
                ("DeviceString", wintypes.WCHAR * 128),
                ("StateFlags", wintypes.DWORD),
                ("DeviceID", wintypes.WCHAR * 128),
                ("DeviceKey", wintypes.WCHAR * 128),
            ]

        user32 = ctypes.windll.user32
        ACTIVE_FLAG = 0x00000001

        dev = _DISPLAY_DEVICE()
        dev.cb = ctypes.sizeof(dev)

        active_count = 0
        adapter_idx = 0
        while user32.EnumDisplayDevicesW(None, adapter_idx, ctypes.byref(dev), 0):
            if dev.StateFlags & ACTIVE_FLAG:
                if active_count == display_index:
                    return dev.DeviceName
                active_count += 1
            adapter_idx += 1

    except Exception:
        pass
    return ""


def get_current_vcgt_windows() -> Optional[VCGTTable]:
    """Get the current Windows gamma ramp as a VCGT table."""
    try:
        import ctypes
        from ctypes import wintypes

        user32 = ctypes.windll.user32
        gdi32 = ctypes.windll.gdi32

        hdc = user32.GetDC(None)
        if not hdc:
            return None

        try:
            ramp = (wintypes.WORD * 256 * 3)()

            if not gdi32.GetDeviceGammaRamp(hdc, ctypes.byref(ramp)):
                return None

            red = np.array([ramp[0][i] / 65535.0 for i in range(256)])
            green = np.array([ramp[1][i] / 65535.0 for i in range(256)])
            blue = np.array([ramp[2][i] / 65535.0 for i in range(256)])

            return VCGTTable(red=red, green=green, blue=blue, size=256)

        finally:
            user32.ReleaseDC(None, hdc)

    except Exception:
        return None


def reset_vcgt_windows() -> bool:
    """Reset Windows gamma ramp to linear (identity)."""
    linear = VCGTTable(
        red=np.linspace(0, 1, 256),
        green=np.linspace(0, 1, 256),
        blue=np.linspace(0, 1, 256),
        size=256
    )
    return apply_vcgt_windows(linear)


if __name__ == "__main__":
    # Demo: Generate and export various VCGT tables
    print("VCGT Tools Demo")
    print("=" * 50)

    # Generate sRGB VCGT
    srgb = srgb_vcgt(256)
    print(f"sRGB VCGT: {srgb.size} entries")
    print(f"  First: R={srgb.red[0]:.4f} G={srgb.green[0]:.4f} B={srgb.blue[0]:.4f}")
    print(f"  Mid:   R={srgb.red[127]:.4f} G={srgb.green[127]:.4f} B={srgb.blue[127]:.4f}")
    print(f"  Last:  R={srgb.red[-1]:.4f} G={srgb.green[-1]:.4f} B={srgb.blue[-1]:.4f}")

    # Generate gamma 2.2 with white balance adjustment
    adjusted = gamma_to_vcgt(2.2, 256, rgb_gains=(0.98, 1.0, 1.02))
    print(f"\nGamma 2.2 with WB adjustment:")
    print(f"  RGB gains applied: R=0.98, G=1.0, B=1.02")

    # Get current Windows gamma
    current = get_current_vcgt_windows()
    if current:
        print(f"\nCurrent Windows gamma ramp:")
        print(f"  Entries: {current.size}")
        print(f"  Mid R={current.red[127]:.4f} G={current.green[127]:.4f} B={current.blue[127]:.4f}")
