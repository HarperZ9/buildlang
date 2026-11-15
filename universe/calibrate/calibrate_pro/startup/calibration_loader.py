"""
Calibrate Pro - Calibration Startup Service

Provides a persistent calibration service that:
1. Reads saved CalibrationConfig from %APPDATA%/CalibratePro/calibration_config.json
2. Re-applies LUTs for each saved display calibration (DWM LUT or VCGT fallback)
3. Watches for display connect/disconnect via polling EnumDisplayDevices
4. Re-applies calibration when a display reconnects
5. Exposes run_service() for background loop and apply_saved_calibrations() for one-shot
"""

import os
import sys
import time
import ctypes
from ctypes import wintypes
import logging
import shutil
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

import numpy as np

# ---------------------------------------------------------------------------
# Ensure calibrate_pro package is importable when invoked standalone
# ---------------------------------------------------------------------------
_package_root = str(Path(__file__).resolve().parent.parent.parent)
if _package_root not in sys.path:
    sys.path.insert(0, _package_root)

from calibrate_pro.utils.startup_manager import (
    StartupManager,
    CalibrationConfig,
    DisplayCalibrationState,
)

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------

_LOG_INITIALIZED = False


def _get_logger() -> logging.Logger:
    """Return (and lazily initialise) the service logger."""
    global _LOG_INITIALIZED
    logger = logging.getLogger("CalibratePro.Service")
    if not _LOG_INITIALIZED:
        log_dir = Path(os.environ.get("APPDATA", "")) / "CalibratePro" / "logs"
        log_dir.mkdir(parents=True, exist_ok=True)
        log_file = log_dir / "calibration_service.log"
        handler = logging.FileHandler(log_file, encoding="utf-8")
        handler.setFormatter(
            logging.Formatter("%(asctime)s [%(levelname)s] %(message)s")
        )
        logger.addHandler(handler)
        logger.setLevel(logging.INFO)
        _LOG_INITIALIZED = True
    return logger


# ---------------------------------------------------------------------------
# Win32 structures & helpers
# ---------------------------------------------------------------------------

class DISPLAY_DEVICE(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("DeviceName", wintypes.WCHAR * 32),
        ("DeviceString", wintypes.WCHAR * 128),
        ("StateFlags", wintypes.DWORD),
        ("DeviceID", wintypes.WCHAR * 128),
        ("DeviceKey", wintypes.WCHAR * 128),
    ]


class GAMMARAMP(ctypes.Structure):
    _fields_ = [
        ("Red", wintypes.WORD * 256),
        ("Green", wintypes.WORD * 256),
        ("Blue", wintypes.WORD * 256),
    ]


_DISPLAY_DEVICE_ACTIVE = 0x00000001


def _enumerate_active_displays() -> List[Dict]:
    """
    Return a list of dicts describing every active display adapter.

    Each dict contains:
        device_name  - e.g. ``\\\\.\\DISPLAY1``
        device_string - friendly name
        device_id     - hardware id string
        state_flags   - raw StateFlags value
    """
    user32 = ctypes.windll.user32
    results: List[Dict] = []
    device = DISPLAY_DEVICE()
    device.cb = ctypes.sizeof(device)
    idx = 0
    while user32.EnumDisplayDevicesW(None, idx, ctypes.byref(device), 0):
        if device.StateFlags & _DISPLAY_DEVICE_ACTIVE:
            results.append(
                {
                    "device_name": device.DeviceName,
                    "device_string": device.DeviceString,
                    "device_id": device.DeviceID,
                    "state_flags": device.StateFlags,
                }
            )
        idx += 1
    return results


# ---------------------------------------------------------------------------
# LUT application helpers
# ---------------------------------------------------------------------------

def _get_dwm_lut_dir() -> Path:
    """Return the directory where dwm_lut expects .cube files."""
    system_root = os.environ.get("SYSTEMROOT", r"C:\Windows")
    return Path(system_root) / "Temp" / "luts"


def _try_apply_dwm_lut(lut_path: str, display_state: DisplayCalibrationState) -> bool:
    """
    Try to apply a LUT via the DWM LUT mechanism (copy the .cube file
    into ``C:\\Windows\\Temp\\luts`` with positional naming).

    Returns True on success, False otherwise.
    """
    logger = _get_logger()
    src = Path(lut_path)
    if not src.exists():
        logger.warning("DWM LUT source does not exist: %s", lut_path)
        return False

    dest_dir = _get_dwm_lut_dir()
    try:
        dest_dir.mkdir(parents=True, exist_ok=True)
    except PermissionError:
        logger.warning("Cannot create DWM LUT dir (permission denied): %s", dest_dir)
        return False

    # Determine display position for naming.
    # We try to look up the display via EnumDisplaySettings to get its
    # screen coordinates. Fall back to display_id based naming.
    try:
        from calibrate_pro.lut_system.dwm_lut import get_monitors, get_lut_filename, LUTType

        monitors = get_monitors()
        # Match on display_id index or display_name
        monitor = None
        if display_state.display_id < len(monitors):
            monitor = monitors[display_state.display_id]
        else:
            for m in monitors:
                if m.device_name == display_state.display_name:
                    monitor = m
                    break

        if monitor is None and monitors:
            monitor = monitors[0]

        if monitor is None:
            logger.warning("No monitors detected for DWM LUT placement")
            return False

        lut_type = LUTType.HDR if display_state.hdr_mode else LUTType.SDR
        dest_name = get_lut_filename(monitor, lut_type)
    except Exception as exc:
        logger.debug("Could not resolve monitor for DWM naming: %s", exc)
        # Fallback naming using display id
        suffix = "_hdr" if display_state.hdr_mode else ""
        dest_name = f"{display_state.display_id}{suffix}.cube"

    dest_path = dest_dir / dest_name
    try:
        shutil.copy2(str(src), str(dest_path))
        logger.info("DWM LUT applied: %s -> %s", src.name, dest_path)
        return True
    except Exception as exc:
        logger.warning("Failed to copy DWM LUT: %s", exc)
        return False


def _apply_vcgt_gamma_ramp(lut_path: str, display_state: DisplayCalibrationState) -> bool:
    """
    Fallback: extract 1-D curves from the diagonal of the 3-D LUT and push
    them into the display's VCGT gamma ramp via SetDeviceGammaRamp.

    Returns True on success, False otherwise.
    """
    logger = _get_logger()

    try:
        from calibrate_pro.core.lut_engine import LUT3D
    except ImportError:
        logger.error("Cannot import LUT3D for VCGT fallback")
        return False

    src = Path(lut_path)
    if not src.exists():
        logger.warning("LUT file not found for VCGT: %s", lut_path)
        return False

    try:
        lut = LUT3D.load(src)
    except Exception as exc:
        logger.error("Failed to load LUT for VCGT: %s", exc)
        return False

    size = lut.size

    # Extract per-channel 1-D curves from the LUT diagonal
    r_curve = np.zeros(256, dtype=np.float64)
    g_curve = np.zeros(256, dtype=np.float64)
    b_curve = np.zeros(256, dtype=np.float64)

    for i in range(256):
        lut_idx = i / 255.0 * (size - 1)
        idx_low = int(lut_idx)
        idx_high = min(idx_low + 1, size - 1)
        frac = lut_idx - idx_low

        # Extract from the neutral diagonal (R=G=B), not single-channel axes
        r_curve[i] = lut.data[idx_low, idx_low, idx_low, 0] * (1.0 - frac) + lut.data[idx_high, idx_high, idx_high, 0] * frac
        g_curve[i] = lut.data[idx_low, idx_low, idx_low, 1] * (1.0 - frac) + lut.data[idx_high, idx_high, idx_high, 1] * frac
        b_curve[i] = lut.data[idx_low, idx_low, idx_low, 2] * (1.0 - frac) + lut.data[idx_high, idx_high, idx_high, 2] * frac

    # Apply gamma ramp via platform backend (works on Windows, macOS, Linux)
    red_int = [int(np.clip(r_curve[i], 0.0, 1.0) * 65535) for i in range(256)]
    green_int = [int(np.clip(g_curve[i], 0.0, 1.0) * 65535) for i in range(256)]
    blue_int = [int(np.clip(b_curve[i], 0.0, 1.0) * 65535) for i in range(256)]

    try:
        from calibrate_pro.platform import get_platform_backend
        backend = get_platform_backend()
        ok = backend.apply_gamma_ramp(display_state.display_id, red_int, green_int, blue_int)
        if ok:
            logger.info(
                "VCGT gamma ramp applied for display %s (idx %d)",
                display_state.display_name,
                display_state.display_id,
            )
            return True
        else:
            logger.warning(
                "Gamma ramp application failed for %s",
                display_state.display_name,
            )
            return False
    except Exception as e:
        logger.warning("Gamma ramp failed: %s", e)
        return False


def _apply_single_calibration(display_state: DisplayCalibrationState) -> bool:
    """
    Apply a single display's saved calibration.

    Strategy:
        1. Try DWM LUT first (file-copy into ``C:\\Windows\\Temp\\luts``).
        2. If that fails, fall back to VCGT gamma ramp via SetDeviceGammaRamp.
    """
    logger = _get_logger()
    lut_path = display_state.lut_path
    if not lut_path:
        logger.info(
            "No LUT path for display %s (id=%d); skipping.",
            display_state.display_name,
            display_state.display_id,
        )
        return False

    if _try_apply_dwm_lut(lut_path, display_state):
        # Ensure DwmLutGUI is running so the placed LUT file is active
        try:
            from calibrate_pro.lut_system.dwm_lut import DwmLutController
            dwm = DwmLutController()
            if dwm.is_available and not dwm._is_dwm_lut_running():
                dwm.start_dwm_lut_gui()
        except Exception as e:
            logger.warning("Could not start DwmLutGUI: %s", e)
        return True

    logger.info("DWM LUT unavailable; falling back to VCGT for display %s", display_state.display_name)
    return _apply_vcgt_gamma_ramp(lut_path, display_state)


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def apply_saved_calibrations() -> bool:
    """
    One-shot: load the saved CalibrationConfig and re-apply every display
    calibration that has a stored LUT.

    Returns True if at least one display was calibrated.
    """
    logger = _get_logger()
    logger.info("--- apply_saved_calibrations: start ---")

    manager = StartupManager()
    config: CalibrationConfig = manager.config
    displays = config.displays

    if not displays:
        logger.info("No saved display calibrations found.")
        return False

    ok_count = 0
    for key, state in displays.items():
        logger.info(
            "Applying calibration for display '%s' (id=%d, hdr=%s)",
            state.display_name,
            state.display_id,
            state.hdr_mode,
        )
        if _apply_single_calibration(state):
            ok_count += 1

    logger.info(
        "--- apply_saved_calibrations: done (%d/%d applied) ---",
        ok_count,
        len(displays),
    )
    return ok_count > 0


def run_service(silent: bool = True) -> None:
    """
    Run calibration persistence service.

    1. Apply all saved calibrations immediately.
    2. Enter a polling loop that checks EnumDisplayDevices every 30 seconds.
    3. When a display that was previously absent reappears, re-apply its
       calibration automatically.

    Args:
        silent: If True, suppress stdout output (logging still goes to file).
    """
    logger = _get_logger()

    if not silent:
        # Add a console handler so the user sees output
        console = logging.StreamHandler(sys.stdout)
        console.setFormatter(
            logging.Formatter("%(asctime)s [%(levelname)s] %(message)s")
        )
        logger.addHandler(console)

    logger.info("=" * 60)
    logger.info("Calibrate Pro Calibration Service starting (silent=%s)", silent)
    logger.info("=" * 60)

    # --- initial apply ---
    apply_saved_calibrations()

    # --- build initial snapshot of connected displays ---
    previous_ids: Set[str] = set()
    for d in _enumerate_active_displays():
        previous_ids.add(d["device_name"])
    logger.info("Initial display set: %s", previous_ids)

    poll_interval = 30  # seconds

    # --- poll loop ---
    try:
        while True:
            time.sleep(poll_interval)

            current_displays = _enumerate_active_displays()
            current_ids = {d["device_name"] for d in current_displays}

            # Detect newly connected displays (present now but absent before)
            new_ids = current_ids - previous_ids
            gone_ids = previous_ids - current_ids

            if gone_ids:
                logger.info("Display(s) disconnected: %s", gone_ids)

            if new_ids:
                logger.info("Display(s) reconnected: %s", new_ids)
                # Re-apply calibrations -- we just re-apply all saved
                # calibrations because the display index may have shifted.
                apply_saved_calibrations()

            previous_ids = current_ids

    except KeyboardInterrupt:
        logger.info("Service stopped by user (KeyboardInterrupt).")
    except Exception as exc:
        logger.exception("Service crashed: %s", exc)
        raise


# ---------------------------------------------------------------------------
# start-service command handler
# ---------------------------------------------------------------------------

def start_service_command(args: Optional[List[str]] = None) -> None:
    """
    Entry point for the ``start-service`` CLI command.

    Can be invoked from the Windows startup registry entry created by
    :class:`StartupManager.enable_startup`.

    Usage::

        python -m calibrate_pro.startup.calibration_loader start-service [--silent]
        python -m calibrate_pro.startup.calibration_loader apply
    """
    import argparse

    parser = argparse.ArgumentParser(
        prog="calibration_loader",
        description="Calibrate Pro - Calibration Startup Service",
    )
    sub = parser.add_subparsers(dest="command")

    svc_parser = sub.add_parser("start-service", help="Run persistent calibration service")
    svc_parser.add_argument(
        "--silent",
        action="store_true",
        default=False,
        help="Suppress console output (log to file only)",
    )

    sub.add_parser("apply", help="One-shot: apply all saved calibrations and exit")

    parsed = parser.parse_args(args)

    if parsed.command == "start-service":
        run_service(silent=parsed.silent)
    elif parsed.command == "apply":
        ok = apply_saved_calibrations()
        if not ok:
            sys.exit(1)
    else:
        parser.print_help()


# ---------------------------------------------------------------------------
# Direct invocation
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    start_service_command()
