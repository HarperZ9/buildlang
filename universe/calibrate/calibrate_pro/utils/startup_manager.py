"""
Startup Manager - Windows startup integration and calibration persistence.

Handles:
- Adding/removing Calibrate Pro from Windows startup
- Saving calibration state for persistence
- Auto-loading calibrations on application start
"""

import os
import sys
import json
import winreg
from pathlib import Path
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, asdict
from datetime import datetime


# Registry key for startup programs
STARTUP_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
APP_NAME = "CalibratePro"


@dataclass
class DisplayCalibrationState:
    """Saved calibration state for a display."""
    display_id: int
    display_name: str
    model: str
    lut_path: Optional[str] = None
    icc_path: Optional[str] = None
    hdr_mode: bool = False
    last_calibrated: Optional[str] = None
    delta_e_avg: float = 0.0
    delta_e_max: float = 0.0


@dataclass
class CalibrationConfig:
    """Complete calibration configuration."""
    version: str = "1.0"
    auto_start: bool = False
    auto_apply: bool = True
    refresh_interval: int = 300  # seconds
    displays: Dict[str, DisplayCalibrationState] = None

    def __post_init__(self):
        if self.displays is None:
            self.displays = {}


class StartupManager:
    """Manages Windows startup integration and calibration persistence."""

    def __init__(self):
        self.config_dir = self._get_config_dir()
        self.config_file = self.config_dir / "calibration_config.json"
        self.config = self._load_config()

    def _get_config_dir(self) -> Path:
        """Get application configuration directory."""
        appdata = os.environ.get('APPDATA', os.path.expanduser('~'))
        config_dir = Path(appdata) / "CalibratePro"
        config_dir.mkdir(parents=True, exist_ok=True)
        return config_dir

    def _get_executable_path(self) -> str:
        """Get path to the executable."""
        if getattr(sys, 'frozen', False):
            # Running as compiled executable
            return sys.executable
        else:
            # Running as script - use pythonw to avoid console
            return f'pythonw -m calibrate_pro.app startup-service'

    def _load_config(self) -> CalibrationConfig:
        """Load configuration from file."""
        if self.config_file.exists():
            try:
                with open(self.config_file, 'r') as f:
                    data = json.load(f)

                # Reconstruct display states
                displays = {}
                for key, val in data.get('displays', {}).items():
                    displays[key] = DisplayCalibrationState(**val)

                return CalibrationConfig(
                    version=data.get('version', '1.0'),
                    auto_start=data.get('auto_start', False),
                    auto_apply=data.get('auto_apply', True),
                    refresh_interval=data.get('refresh_interval', 300),
                    displays=displays
                )
            except Exception as e:
                print(f"Warning: Could not load config: {e}")

        return CalibrationConfig()

    def save_config(self):
        """Save configuration to file."""
        data = {
            'version': self.config.version,
            'auto_start': self.config.auto_start,
            'auto_apply': self.config.auto_apply,
            'refresh_interval': self.config.refresh_interval,
            'displays': {
                key: asdict(val) for key, val in self.config.displays.items()
            }
        }

        with open(self.config_file, 'w') as f:
            json.dump(data, f, indent=2)

    def is_startup_enabled(self) -> bool:
        """Check if application is set to run at startup."""
        try:
            key = winreg.OpenKey(
                winreg.HKEY_CURRENT_USER,
                STARTUP_KEY,
                0,
                winreg.KEY_READ
            )
            try:
                winreg.QueryValueEx(key, APP_NAME)
                return True
            except WindowsError:
                return False
            finally:
                winreg.CloseKey(key)
        except WindowsError:
            return False

    def enable_startup(self, silent: bool = True) -> bool:
        """Add application to Windows startup."""
        try:
            exe_path = self._get_executable_path()

            # Add --startup flag for silent background mode
            # Use calibration_loader which has proper DWM + VCGT fallback
            if silent:
                if getattr(sys, 'frozen', False):
                    startup_cmd = f'"{exe_path}" start-service --silent'
                else:
                    startup_cmd = f'pythonw -m calibrate_pro.startup.calibration_loader start-service --silent'
            else:
                if getattr(sys, 'frozen', False):
                    startup_cmd = f'"{exe_path}" start-service'
                else:
                    startup_cmd = f'pythonw -m calibrate_pro.startup.calibration_loader start-service'

            key = winreg.OpenKey(
                winreg.HKEY_CURRENT_USER,
                STARTUP_KEY,
                0,
                winreg.KEY_SET_VALUE
            )
            winreg.SetValueEx(key, APP_NAME, 0, winreg.REG_SZ, startup_cmd)
            winreg.CloseKey(key)

            self.config.auto_start = True
            self.save_config()

            return True
        except WindowsError as e:
            print(f"Error enabling startup: {e}")
            return False

    def disable_startup(self) -> bool:
        """Remove application from Windows startup."""
        try:
            key = winreg.OpenKey(
                winreg.HKEY_CURRENT_USER,
                STARTUP_KEY,
                0,
                winreg.KEY_SET_VALUE
            )
            try:
                winreg.DeleteValue(key, APP_NAME)
            except WindowsError:
                pass  # Value doesn't exist
            winreg.CloseKey(key)

            self.config.auto_start = False
            self.save_config()

            return True
        except WindowsError as e:
            print(f"Error disabling startup: {e}")
            return False

    def save_display_calibration(
        self,
        display_id: int,
        display_name: str,
        model: str,
        lut_path: Optional[str] = None,
        icc_path: Optional[str] = None,
        hdr_mode: bool = False,
        delta_e_avg: float = 0.0,
        delta_e_max: float = 0.0
    ):
        """Save calibration state for a display."""
        state = DisplayCalibrationState(
            display_id=display_id,
            display_name=display_name,
            model=model,
            lut_path=str(lut_path) if lut_path else None,
            icc_path=str(icc_path) if icc_path else None,
            hdr_mode=hdr_mode,
            last_calibrated=datetime.now().isoformat(),
            delta_e_avg=delta_e_avg,
            delta_e_max=delta_e_max
        )

        self.config.displays[str(display_id)] = state
        self.save_config()

    def get_display_calibration(self, display_id: int) -> Optional[DisplayCalibrationState]:
        """Get saved calibration state for a display."""
        return self.config.displays.get(str(display_id))

    def get_all_calibrations(self) -> Dict[str, DisplayCalibrationState]:
        """Get all saved calibration states."""
        return self.config.displays

    def clear_calibration(self, display_id: int):
        """Clear saved calibration for a display."""
        key = str(display_id)
        if key in self.config.displays:
            del self.config.displays[key]
            self.save_config()

    def clear_all_calibrations(self):
        """Clear all saved calibrations."""
        self.config.displays = {}
        self.save_config()


def get_startup_manager() -> StartupManager:
    """Get the startup manager instance."""
    return StartupManager()


def enable_auto_start(silent: bool = True) -> bool:
    """Enable auto-start at Windows startup."""
    return StartupManager().enable_startup(silent)


def disable_auto_start() -> bool:
    """Disable auto-start."""
    return StartupManager().disable_startup()


def is_auto_start_enabled() -> bool:
    """Check if auto-start is enabled."""
    return StartupManager().is_startup_enabled()
