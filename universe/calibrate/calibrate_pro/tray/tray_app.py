"""
Calibrate Pro - System Tray Application

Provides two runtime paths:
  1. **pystray + PIL** -- a real Windows system-tray icon with right-click
     context menu (green square = calibrated, grey = uncalibrated).
  2. **Console fallback** -- when pystray/PIL are not installed the app runs
     the calibration-loader service with visible console output so that the
     user still gets persistent calibration.

Public API
----------
``run_tray_app()`` -- call from the CLI ``tray`` command.
"""

import os
import sys
import glob
import threading
import webbrowser
import logging
from pathlib import Path
from typing import Optional

from calibrate_pro import __version__

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _get_config_dir() -> Path:
    """Return ``%APPDATA%/CalibratePro``."""
    appdata = os.environ.get("APPDATA", os.path.expanduser("~"))
    return Path(appdata) / "CalibratePro"


def _get_output_dir() -> Path:
    """Return the default calibration output directory."""
    docs = Path.home() / "Documents" / "Calibrate Pro"
    if docs.exists():
        return docs
    return _get_config_dir()


def _find_latest_report() -> Optional[Path]:
    """Find the most-recently modified ``*_report.html`` file."""
    search_dirs = [
        _get_output_dir(),
        _get_config_dir(),
        Path.home() / "Documents" / "Calibrate Pro",
        Path("."),
    ]
    candidates = []
    for d in search_dirs:
        if d.exists():
            candidates.extend(d.glob("*_report.html"))

    if not candidates:
        # Broader search in home
        candidates.extend(Path.home().glob("**/*_report.html"))
        # Limit depth to avoid traversing everything
        candidates = candidates[:50]

    if not candidates:
        return None

    return max(candidates, key=lambda p: p.stat().st_mtime)


def _is_calibrated() -> bool:
    """Return True if at least one display has a saved calibration."""
    config_file = _get_config_dir() / "calibration_config.json"
    if not config_file.exists():
        return False
    try:
        import json
        with open(config_file, "r") as fh:
            data = json.load(fh)
        displays = data.get("displays", {})
        return len(displays) > 0
    except Exception:
        return False


# ---------------------------------------------------------------------------
# pystray path
# ---------------------------------------------------------------------------

def _needs_recalibration() -> bool:
    """Return True if any display's calibration is older than 30 days."""
    try:
        from calibrate_pro.services.drift_monitor import any_needs_recalibration
        return any_needs_recalibration(max_age_days=30)
    except Exception:
        return False


def _create_icon_image(calibrated: bool, stale: bool = False):
    """Create a 64x64 PIL Image.

    Colours:
    - green  = calibrated and current
    - yellow = calibrated but stale (needs re-calibration)
    - grey   = uncalibrated
    """
    from PIL import Image, ImageDraw  # type: ignore

    size = 64
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    if calibrated and stale:
        fill = (255, 193, 7, 255)        # Material amber / yellow
        border = (255, 160, 0, 255)
    elif calibrated:
        fill = (76, 175, 80, 255)       # Material green
        border = (56, 142, 60, 255)
    else:
        fill = (158, 158, 158, 255)      # Grey
        border = (117, 117, 117, 255)

    margin = 4
    draw.rounded_rectangle(
        [margin, margin, size - margin, size - margin],
        radius=8,
        fill=fill,
        outline=border,
        width=2,
    )

    # Draw a small "C" letter in the centre
    try:
        from PIL import ImageFont  # type: ignore
        font = ImageFont.truetype("arial.ttf", 32)
    except Exception:
        font = ImageFont.load_default()

    text = "C"
    bbox = draw.textbbox((0, 0), text, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    tx = (size - tw) // 2 - bbox[0]
    ty = (size - th) // 2 - bbox[1]
    draw.text((tx, ty), text, fill=(255, 255, 255, 255), font=font)

    return img


def _run_pystray():
    """Run the tray app using the *pystray* library."""
    import pystray  # type: ignore
    from PIL import Image  # type: ignore  # noqa: F811

    calibrated = _is_calibrated()
    stale = _needs_recalibration() if calibrated else False

    # ---- Menu actions ----

    def on_calibrate_all(icon, item):
        """Run ``auto_calibrate_all`` in a background thread."""
        def _work():
            try:
                from calibrate_pro.sensorless.auto_calibration import auto_calibrate_all
                auto_calibrate_all()
                # Refresh icon colour -- freshly calibrated, not stale
                icon.icon = _create_icon_image(True, stale=False)
                icon.title = f"Calibrate Pro v{__version__}"
            except Exception as exc:
                logger.error("Calibrate all failed: %s", exc)
        threading.Thread(target=_work, daemon=True).start()

    def on_restore(icon, item):
        """Reset calibrations on all displays."""
        def _work():
            try:
                from calibrate_pro.panels.detection import (
                    enumerate_displays, reset_gamma_ramp,
                )
                for display in enumerate_displays():
                    try:
                        reset_gamma_ramp(display.device_name)
                    except Exception:
                        pass
                icon.icon = _create_icon_image(False)
                icon.title = f"Calibrate Pro v{__version__}"
            except Exception as exc:
                logger.error("Restore defaults failed: %s", exc)
        threading.Thread(target=_work, daemon=True).start()

    def on_show_status(icon, item):
        """Print calibration status to console."""
        try:
            from calibrate_pro.services.drift_monitor import print_calibration_status
            print_calibration_status()
        except Exception as exc:
            logger.error("Status check failed: %s", exc)

    def on_open_report(icon, item):
        report = _find_latest_report()
        if report:
            webbrowser.open(str(report.absolute()))

    def _startup_enabled():
        try:
            from calibrate_pro.utils.startup_manager import is_auto_start_enabled
            return is_auto_start_enabled()
        except Exception:
            return False

    def on_toggle_startup(icon, item):
        try:
            from calibrate_pro.utils.startup_manager import (
                enable_auto_start, disable_auto_start, is_auto_start_enabled,
            )
            if is_auto_start_enabled():
                disable_auto_start()
            else:
                enable_auto_start(silent=True)
        except Exception as exc:
            logger.error("Toggle startup failed: %s", exc)

    def on_exit(icon, item):
        icon.stop()

    def startup_text(item):
        if _startup_enabled():
            return "Disable Startup"
        return "Enable Startup"

    # ---- Build menu ----

    menu = pystray.Menu(
        pystray.MenuItem(
            f"Calibrate Pro v{__version__}",
            None,
            enabled=False,
        ),
        pystray.Menu.SEPARATOR,
        pystray.MenuItem("Calibration Status", on_show_status),
        pystray.MenuItem("Calibrate All Displays", on_calibrate_all),
        pystray.MenuItem("Restore Defaults", on_restore),
        pystray.Menu.SEPARATOR,
        pystray.MenuItem("Open Last Report", on_open_report),
        pystray.MenuItem(startup_text, on_toggle_startup),
        pystray.Menu.SEPARATOR,
        pystray.MenuItem("Exit", on_exit),
    )

    # Tooltip reflects stale status
    title = f"Calibrate Pro v{__version__}"
    if stale:
        title += " (re-calibration recommended)"

    icon = pystray.Icon(
        name="CalibratePro",
        icon=_create_icon_image(calibrated, stale=stale),
        title=title,
        menu=menu,
    )

    # Apply saved calibrations on launch
    try:
        from calibrate_pro.startup.calibration_loader import apply_saved_calibrations
        apply_saved_calibrations()
    except Exception:
        pass

    icon.run()


# ---------------------------------------------------------------------------
# Console-service fallback
# ---------------------------------------------------------------------------

def _run_console_service():
    """
    Fallback when pystray is not installed.

    Applies saved calibrations and then runs the persistent
    calibration-loader service with console output.
    """
    print(f"Calibrate Pro v{__version__} - Background Calibration Service")
    print("=" * 60)
    print("(Install pystray and Pillow for a system-tray icon)")
    print()

    try:
        from calibrate_pro.startup.calibration_loader import (
            apply_saved_calibrations,
            run_service,
        )

        print("Applying saved calibrations...")
        result = apply_saved_calibrations()
        if result:
            print("[OK] Calibrations applied.")
        else:
            print("[--] No saved calibrations found.")

        print()
        print("Running calibration persistence service.")
        print("Press Ctrl+C to stop.\n")

        run_service(silent=False)

    except KeyboardInterrupt:
        print("\nService stopped.")
    except Exception as exc:
        print(f"\nError: {exc}")
        return 1

    return 0


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------

def run_tray_app():
    """
    Run the system tray application.

    Tries pystray first; if unavailable, falls back to a console service
    that still applies and persists calibrations.
    """
    try:
        return _run_pystray()
    except ImportError:
        return _run_console_service()
