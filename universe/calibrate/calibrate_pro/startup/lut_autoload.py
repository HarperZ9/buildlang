"""
LUT Auto-Load Service

Automatically loads calibration LUTs on system startup.
Runs silently in the background to apply per-display color corrections.
"""

import os
import sys
import time
import logging
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from calibrate_pro.lut_system.dwm_lut import DwmLutController
from calibrate_pro.lut_system.per_display_calibration import PerDisplayCalibrationManager


def setup_logging():
    """Configure logging for the auto-load service."""
    log_dir = Path(os.environ.get('APPDATA', '')) / 'CalibratePro' / 'logs'
    log_dir.mkdir(parents=True, exist_ok=True)

    log_file = log_dir / 'autoload.log'

    logging.basicConfig(
        level=logging.INFO,
        format='%(asctime)s - %(levelname)s - %(message)s',
        handlers=[
            logging.FileHandler(log_file),
            logging.StreamHandler()
        ]
    )
    return logging.getLogger('CalibratePro.AutoLoad')


def load_calibration_luts():
    """Load all calibration LUTs for detected displays."""
    logger = setup_logging()
    logger.info("=" * 50)
    logger.info("Calibrate Pro - LUT Auto-Load Service Starting")
    logger.info("=" * 50)

    try:
        # Initialize components
        dwm = DwmLutController()
        manager = PerDisplayCalibrationManager()

        logger.info(f"DWM LUT Available: {dwm.is_available}")

        # Get all calibrated displays
        displays = manager.list_displays()
        logger.info(f"Detected {len(displays)} display(s)")

        loaded_count = 0

        for display in displays:
            display_id = display['id']
            profile = manager.get_display_profile(display_id)

            if not profile:
                logger.warning(f"Display {display_id}: No profile found")
                continue

            if not profile.lut_path or not os.path.exists(profile.lut_path):
                logger.warning(f"Display {display_id}: No LUT file found")
                continue

            # Load the LUT
            logger.info(f"Display {display_id}: Loading {os.path.basename(profile.lut_path)}")

            success = dwm.load_lut_file(display_id, profile.lut_path)

            if success:
                logger.info(f"Display {display_id}: LUT loaded successfully")
                loaded_count += 1
            else:
                logger.error(f"Display {display_id}: Failed to load LUT")

        logger.info(f"Loaded {loaded_count}/{len(displays)} LUTs")
        logger.info("Auto-load complete")

        return loaded_count > 0

    except Exception as e:
        logger.error(f"Auto-load failed: {e}")
        return False


def create_startup_shortcut():
    """Create a Windows startup shortcut for auto-loading LUTs."""
    try:
        import winreg

        # Get the path to this script
        script_path = Path(__file__).resolve()
        python_exe = sys.executable

        # Create a batch file that runs silently
        startup_dir = Path(os.environ.get('APPDATA', '')) / 'CalibratePro'
        startup_dir.mkdir(parents=True, exist_ok=True)

        batch_file = startup_dir / 'autoload_luts.bat'
        vbs_file = startup_dir / 'autoload_luts.vbs'

        # Create batch file
        batch_content = f'''@echo off
cd /d "{script_path.parent.parent.parent}"
"{python_exe}" -c "from calibrate_pro.startup.lut_autoload import load_calibration_luts; load_calibration_luts()"
'''
        batch_file.write_text(batch_content)

        # Create VBS wrapper to run silently
        vbs_content = f'''Set WshShell = CreateObject("WScript.Shell")
WshShell.Run chr(34) & "{batch_file}" & chr(34), 0
Set WshShell = Nothing
'''
        vbs_file.write_text(vbs_content)

        # Add to Windows startup registry
        key_path = r"Software\Microsoft\Windows\CurrentVersion\Run"

        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path, 0,
                           winreg.KEY_SET_VALUE) as key:
            winreg.SetValueEx(key, "CalibratePro_LUT_AutoLoad", 0,
                            winreg.REG_SZ, f'wscript.exe "{vbs_file}"')

        return True, str(vbs_file)

    except Exception as e:
        return False, str(e)


def remove_startup():
    """Remove the auto-load from Windows startup."""
    try:
        import winreg

        key_path = r"Software\Microsoft\Windows\CurrentVersion\Run"

        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path, 0,
                           winreg.KEY_SET_VALUE) as key:
            try:
                winreg.DeleteValue(key, "CalibratePro_LUT_AutoLoad")
            except FileNotFoundError:
                pass

        return True

    except Exception as e:
        return False


def check_startup_enabled():
    """Check if auto-load is enabled in Windows startup."""
    try:
        import winreg

        key_path = r"Software\Microsoft\Windows\CurrentVersion\Run"

        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key_path, 0,
                           winreg.KEY_READ) as key:
            try:
                value, _ = winreg.QueryValueEx(key, "CalibratePro_LUT_AutoLoad")
                return True, value
            except FileNotFoundError:
                return False, None

    except Exception:
        return False, None


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Calibrate Pro LUT Auto-Load")
    parser.add_argument('--install', action='store_true',
                       help='Install auto-load to Windows startup')
    parser.add_argument('--uninstall', action='store_true',
                       help='Remove auto-load from Windows startup')
    parser.add_argument('--status', action='store_true',
                       help='Check if auto-load is enabled')
    parser.add_argument('--load', action='store_true',
                       help='Load LUTs now')

    args = parser.parse_args()

    if args.install:
        success, result = create_startup_shortcut()
        if success:
            print(f"Auto-load installed: {result}")
        else:
            print(f"Failed to install: {result}")

    elif args.uninstall:
        if remove_startup():
            print("Auto-load removed from startup")
        else:
            print("Failed to remove auto-load")

    elif args.status:
        enabled, path = check_startup_enabled()
        if enabled:
            print(f"Auto-load is ENABLED: {path}")
        else:
            print("Auto-load is DISABLED")

    elif args.load:
        load_calibration_luts()

    else:
        # Default: load LUTs
        load_calibration_luts()
