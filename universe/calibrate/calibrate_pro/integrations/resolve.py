"""
DaVinci Resolve Integration

The #1 question on every color forum: "How do I use my calibration in Resolve?"

This module handles:
1. Finding Resolve's LUT directory
2. Copying calibration LUT to the right location
3. Generating instructions for the user
4. Detecting whether Color Viewer or Video Monitor LUT path is needed
"""

import shutil
from pathlib import Path
from typing import Optional, List


# Known Resolve LUT directories by platform
RESOLVE_LUT_PATHS = {
    "win32": [
        Path.home() / "AppData" / "Roaming" / "Blackmagic Design" / "DaVinci Resolve" / "Support" / "LUT",
        Path("C:/ProgramData/Blackmagic Design/DaVinci Resolve/Support/LUT"),
    ],
    "darwin": [
        Path.home() / "Library" / "Application Support" / "Blackmagic Design" / "DaVinci Resolve" / "LUT",
    ],
    "linux": [
        Path.home() / ".local" / "share" / "DaVinciResolve" / "LUT",
    ],
}


def find_resolve_lut_dir() -> Optional[Path]:
    """Find DaVinci Resolve's LUT directory."""
    import sys
    platform = sys.platform

    for path in RESOLVE_LUT_PATHS.get(platform, []):
        if path.exists():
            return path

    # Try common locations
    for path in RESOLVE_LUT_PATHS.get("win32", []):
        if path.exists():
            return path

    return None


def install_lut_to_resolve(
    lut_path: str,
    subfolder: str = "Calibrate Pro"
) -> Optional[Path]:
    """
    Copy a calibration LUT into Resolve's LUT directory.

    Args:
        lut_path: Path to the .cube LUT file
        subfolder: Subfolder within Resolve's LUT directory

    Returns:
        Path where the LUT was installed, or None on failure
    """
    resolve_dir = find_resolve_lut_dir()
    if resolve_dir is None:
        return None

    src = Path(lut_path)
    if not src.exists():
        return None

    dest_dir = resolve_dir / subfolder
    dest_dir.mkdir(parents=True, exist_ok=True)

    dest = dest_dir / src.name
    shutil.copy2(str(src), str(dest))

    return dest


def get_resolve_instructions(monitor_count: int = 1) -> str:
    """
    Generate setup instructions for Resolve.

    The instructions differ based on whether the user has one or two monitors.
    """
    if monitor_count == 1:
        return """DaVinci Resolve Setup (Single Monitor):

1. Open Resolve > Preferences > General
2. Under "Color management":
   - Set "3D LUT" to the Calibrate Pro LUT
   - This applies to the Color page viewer

Note: With a single monitor, use the "Color Viewer" LUT path.
Do NOT use "3D Video Monitor LUT" — that's for dedicated
monitoring output only.

Warning: If using ACES, the monitor LUT may conflict with
the ACES color pipeline. In ACES workflows, set the Output
Transform to match your display instead of using a monitor LUT.
"""
    else:
        return """DaVinci Resolve Setup (Dual Monitor):

For your GRADING monitor (calibrated display):
1. Open Resolve > Preferences > General
2. Under "Video monitor":
   - Set "3D Video Monitor LUT" to the Calibrate Pro LUT

For your GUI/timeline monitor:
1. Resolve currently only supports ONE monitor LUT
2. Your GUI monitor will use the system-wide calibration
   (ICC profile + dwm_lut)

Important: You cannot have two different 3D LUTs active
simultaneously in Resolve. The system-wide calibration
handles the secondary monitor.
"""


def detect_resolve_installed() -> bool:
    """Check if DaVinci Resolve is installed."""
    return find_resolve_lut_dir() is not None


def list_installed_luts(subfolder: str = "Calibrate Pro") -> List[Path]:
    """List LUTs installed in Resolve's Calibrate Pro subfolder."""
    resolve_dir = find_resolve_lut_dir()
    if resolve_dir is None:
        return []

    lut_dir = resolve_dir / subfolder
    if not lut_dir.exists():
        return []

    return list(lut_dir.glob("*.cube")) + list(lut_dir.glob("*.3dl"))
