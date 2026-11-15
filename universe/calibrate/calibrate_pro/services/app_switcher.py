"""
Per-Application LUT / Profile Switching Service

Monitors the foreground Windows application and automatically switches the
active calibration LUT/profile based on the detected app.  This enables
seamless transitions between color spaces -- e.g. sRGB for web browsers,
Display P3 for Photoshop, native gamut for games.

Usage:
    switcher = AppProfileSwitcher(config_path="app_profiles.json",
                                   lut_dir="C:/Users/.../Calibrations")
    switcher.start()
    ...
    switcher.stop()

Config file format (app_profiles.json):
    {
        "default": "native",
        "rules": [
            {"match": "chrome.exe",      "profile": "sRGB"},
            {"match": "Photoshop.exe",   "profile": "p3"},
            {"match": "explorer.exe",    "profile": "native"},
            {"match": "*.exe",           "profile": "native"}
        ]
    }

Each ``profile`` value maps to a .cube LUT file in ``lut_dir``.
For example, profile ``"sRGB"`` loads ``<lut_dir>/sRGB.cube``.
"""

import ctypes
import ctypes.wintypes
import json
import fnmatch
import logging
import threading
import time
from pathlib import Path
from typing import Dict, List, Optional, Callable

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Win32 constants and type aliases
# ---------------------------------------------------------------------------
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
MAX_PATH = 260


# ---------------------------------------------------------------------------
# Win32 helpers (ctypes)
# ---------------------------------------------------------------------------

def _get_foreground_window() -> int:
    """Return the HWND of the current foreground window, or 0."""
    try:
        user32 = ctypes.windll.user32
        hwnd = user32.GetForegroundWindow()
        return hwnd
    except Exception:
        return 0


def _get_pid_from_hwnd(hwnd: int) -> int:
    """Return the process ID that owns *hwnd*, or 0."""
    try:
        user32 = ctypes.windll.user32
        pid = ctypes.wintypes.DWORD(0)
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        return pid.value
    except Exception:
        return 0


def _get_process_exe(pid: int) -> str:
    """
    Return the full executable path for a given PID.

    Uses ``QueryFullProcessImageNameW`` which works even for elevated
    processes when called with ``PROCESS_QUERY_LIMITED_INFORMATION``.
    Falls back to an empty string on failure.
    """
    if pid == 0:
        return ""
    try:
        kernel32 = ctypes.windll.kernel32
        handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
        if not handle:
            return ""
        try:
            buf = ctypes.create_unicode_buffer(1024)
            size = ctypes.wintypes.DWORD(1024)
            ok = kernel32.QueryFullProcessImageNameW(handle, 0, buf, ctypes.byref(size))
            if ok:
                return buf.value
            return ""
        finally:
            kernel32.CloseHandle(handle)
    except Exception:
        return ""


def _exe_name(exe_path: str) -> str:
    """Extract the filename from a full executable path."""
    if not exe_path:
        return ""
    try:
        return Path(exe_path).name
    except Exception:
        return exe_path.rsplit("\\", 1)[-1].rsplit("/", 1)[-1]


# ---------------------------------------------------------------------------
# Default config
# ---------------------------------------------------------------------------

_DEFAULT_CONFIG: Dict = {
    "default": "native",
    "rules": [
        {"match": "chrome.exe", "profile": "sRGB"},
        {"match": "firefox.exe", "profile": "sRGB"},
        {"match": "msedge.exe", "profile": "sRGB"},
        {"match": "Photoshop.exe", "profile": "p3"},
        {"match": "DaVinci Resolve", "profile": "p3"},
        {"match": "explorer.exe", "profile": "native"},
        {"match": "*.exe", "profile": "native"},
    ],
}


# ---------------------------------------------------------------------------
# Core service class
# ---------------------------------------------------------------------------

class AppProfileSwitcher:
    """
    Monitors the foreground Windows application and switches the active
    calibration LUT/profile based on a configurable rule set.

    The monitor loop polls every ``poll_interval`` seconds (default 0.5).
    When the foreground executable changes, the matching profile's .cube LUT
    is loaded and applied via dwm_lut or VCGT.
    """

    def __init__(
        self,
        config_path: Optional[str] = None,
        lut_dir: Optional[str] = None,
        poll_interval: float = 0.5,
        display_index: int = 0,
        on_switch: Optional[Callable[[str, str, str], None]] = None,
    ):
        """
        Args:
            config_path: Path to ``app_profiles.json``.  If *None*, a
                default configuration is used.
            lut_dir: Directory containing LUT files (e.g. ``native.cube``,
                ``sRGB.cube``, ``p3.cube``).  Falls back to the config
                file's parent directory, then ``~/Documents/Calibrate Pro/Calibrations``.
            poll_interval: Seconds between foreground-app polls (default 0.5).
            display_index: Which display to apply LUTs to (0 = primary).
            on_switch: Optional callback ``(app_name, old_profile, new_profile)``
                invoked whenever the active profile changes.
        """
        self._poll_interval = poll_interval
        self._display_index = display_index
        self._on_switch = on_switch

        # Load configuration
        self._config: Dict = _DEFAULT_CONFIG.copy()
        self._config_path: Optional[Path] = None
        if config_path is not None:
            self._config_path = Path(config_path)
            self._load_config()

        # Resolve LUT directory
        if lut_dir is not None:
            self._lut_dir = Path(lut_dir)
        elif self._config_path is not None:
            self._lut_dir = self._config_path.parent
        else:
            self._lut_dir = Path.home() / "Documents" / "Calibrate Pro" / "Calibrations"

        # Runtime state
        self._current_app: str = ""
        self._current_profile: str = ""
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._lock = threading.Lock()

        # Cache loaded LUT objects keyed by profile name
        self._lut_cache: Dict[str, object] = {}

    # ------------------------------------------------------------------
    # Configuration
    # ------------------------------------------------------------------

    def _load_config(self):
        """Load app_profiles.json from disk."""
        if self._config_path is None or not self._config_path.exists():
            logger.warning("Config file not found at %s; using defaults", self._config_path)
            return
        try:
            with open(self._config_path, "r", encoding="utf-8") as f:
                self._config = json.load(f)
            logger.info("Loaded app profile config from %s", self._config_path)
        except Exception as exc:
            logger.error("Failed to load config %s: %s", self._config_path, exc)

    def reload_config(self):
        """Reload the configuration file without stopping the service."""
        with self._lock:
            self._load_config()
            self._lut_cache.clear()

    @property
    def rules(self) -> List[Dict]:
        """Return the current rule list."""
        return self._config.get("rules", [])

    @property
    def default_profile(self) -> str:
        """Return the default profile name."""
        return self._config.get("default", "native")

    # ------------------------------------------------------------------
    # App detection
    # ------------------------------------------------------------------

    def get_current_app(self) -> str:
        """
        Get the executable name of the current foreground application.

        Returns:
            Executable filename (e.g. ``"chrome.exe"``), or ``""`` if
            detection fails.
        """
        hwnd = _get_foreground_window()
        if not hwnd:
            return ""
        pid = _get_pid_from_hwnd(hwnd)
        if not pid:
            return ""
        exe_path = _get_process_exe(pid)
        return _exe_name(exe_path)

    # ------------------------------------------------------------------
    # Profile matching
    # ------------------------------------------------------------------

    def get_profile_for_app(self, app_name: str) -> str:
        """
        Look up which profile to use for a given application.

        Matching is done against the ``rules`` list in order.  The first
        rule whose ``match`` pattern (case-insensitive fnmatch) matches
        ``app_name`` wins.  If no rule matches, the ``default`` profile
        is returned.

        Args:
            app_name: Executable filename, e.g. ``"chrome.exe"``.

        Returns:
            Profile name string, e.g. ``"sRGB"``.
        """
        if not app_name:
            return self.default_profile

        for rule in self.rules:
            pattern = rule.get("match", "")
            profile = rule.get("profile", self.default_profile)
            # Case-insensitive pattern matching
            if fnmatch.fnmatch(app_name.lower(), pattern.lower()):
                return profile
            # Also match if the pattern appears as a substring
            # (e.g. "DaVinci Resolve" matches any exe containing that string)
            if not ("*" in pattern or "?" in pattern):
                if pattern.lower() in app_name.lower():
                    return profile

        return self.default_profile

    # ------------------------------------------------------------------
    # LUT application
    # ------------------------------------------------------------------

    def _get_lut(self, profile_name: str):
        """
        Load (and cache) the LUT3D object for a profile name.

        Looks for ``<lut_dir>/<profile_name>.cube``.  Returns *None*
        if the file does not exist or cannot be parsed.
        """
        if profile_name in self._lut_cache:
            return self._lut_cache[profile_name]

        lut_path = self._lut_dir / f"{profile_name}.cube"
        if not lut_path.exists():
            logger.debug("LUT file not found: %s", lut_path)
            self._lut_cache[profile_name] = None
            return None

        try:
            from calibrate_pro.core.lut_engine import LUT3D
            lut = LUT3D.load(lut_path)
            self._lut_cache[profile_name] = lut
            logger.info("Cached LUT for profile '%s' from %s", profile_name, lut_path)
            return lut
        except Exception as exc:
            logger.error("Failed to load LUT %s: %s", lut_path, exc)
            self._lut_cache[profile_name] = None
            return None

    def _apply_profile(self, profile_name: str) -> bool:
        """
        Apply the named profile's LUT to the display.

        Attempts dwm_lut first, then VCGT gamma ramp as fallback.
        If the profile is ``"native"`` and no LUT file exists, the
        system LUT is cleared (identity / reset).

        Returns:
            True if the profile was applied successfully.
        """
        lut_path = self._lut_dir / f"{profile_name}.cube"

        # --- Method 1: DWM 3D LUT (highest quality) ---
        try:
            from calibrate_pro.lut_system.dwm_lut import DwmLutController

            dwm = DwmLutController()
            if dwm.is_available:
                if lut_path.exists():
                    if dwm.load_lut_file(self._display_index, lut_path):
                        logger.info("Applied profile '%s' via DWM LUT", profile_name)
                        return True
                else:
                    # No LUT file -- clear to identity (native)
                    if dwm.clear_lut(self._display_index):
                        logger.info("Cleared DWM LUT for profile '%s'", profile_name)
                        return True
        except Exception as exc:
            logger.debug("DWM LUT method failed: %s", exc)

        # --- Method 2: VCGT gamma ramp (1D fallback) ---
        try:
            from calibrate_pro.core.lut_engine import LUT3D
            from calibrate_pro.core.vcgt import lut3d_to_vcgt, apply_vcgt_windows

            if lut_path.exists():
                lut = self._get_lut(profile_name)
                if lut is not None:
                    vcgt = lut3d_to_vcgt(lut.data, method="neutral_axis", output_size=256)
                    if apply_vcgt_windows(vcgt, display_index=self._display_index):
                        logger.info("Applied profile '%s' via VCGT", profile_name)
                        return True
            else:
                # Reset to identity gamma ramp
                import numpy as np
                from calibrate_pro.core.vcgt import VCGTTable, apply_vcgt_windows

                identity = np.linspace(0.0, 1.0, 256)
                vcgt = VCGTTable(red=identity, green=identity, blue=identity,
                                 size=256, bit_depth=16)
                if apply_vcgt_windows(vcgt, display_index=self._display_index):
                    logger.info("Reset VCGT to identity for profile '%s'", profile_name)
                    return True
        except Exception as exc:
            logger.debug("VCGT method failed: %s", exc)

        logger.warning("Failed to apply profile '%s'", profile_name)
        return False

    # ------------------------------------------------------------------
    # Monitor loop
    # ------------------------------------------------------------------

    def _monitor_loop(self):
        """Background thread: poll foreground app and switch profile."""
        logger.info("App profile switcher started (poll=%.1fs)", self._poll_interval)

        while self._running:
            try:
                app_name = self.get_current_app()

                if app_name and app_name != self._current_app:
                    new_profile = self.get_profile_for_app(app_name)

                    if new_profile != self._current_profile:
                        old_profile = self._current_profile
                        logger.info(
                            "Foreground changed: %s -> %s (profile: %s -> %s)",
                            self._current_app, app_name, old_profile, new_profile,
                        )
                        self._apply_profile(new_profile)
                        self._current_profile = new_profile

                        if self._on_switch is not None:
                            try:
                                self._on_switch(app_name, old_profile, new_profile)
                            except Exception:
                                pass

                    self._current_app = app_name

            except Exception as exc:
                logger.error("Monitor loop error: %s", exc)

            time.sleep(self._poll_interval)

        logger.info("App profile switcher stopped")

    # ------------------------------------------------------------------
    # Public start / stop
    # ------------------------------------------------------------------

    def start(self):
        """Start monitoring foreground app changes in a background thread."""
        if self._running:
            logger.warning("App profile switcher is already running")
            return

        self._running = True
        self._thread = threading.Thread(
            target=self._monitor_loop,
            name="AppProfileSwitcher",
            daemon=True,
        )
        self._thread.start()

    def stop(self):
        """Stop monitoring."""
        self._running = False
        if self._thread is not None:
            self._thread.join(timeout=2.0)
            self._thread = None
        logger.info("App profile switcher stopped")

    @property
    def is_running(self) -> bool:
        """Whether the monitor loop is currently active."""
        return self._running

    @property
    def current_app(self) -> str:
        """The last-detected foreground application executable name."""
        return self._current_app

    @property
    def current_profile(self) -> str:
        """The currently active profile name."""
        return self._current_profile
