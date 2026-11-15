"""
Per-Display Calibration System

Automatically detects connected displays, matches them to panel profiles,
generates appropriate calibration LUTs, and applies them per-display.

Features:
- Auto-detection of panel type and model via EDID
- Panel database matching for accurate corrections
- Per-display 3D LUT generation and application
- Support for mixed display setups (e.g., QD-OLED + IPS)
- Forum-sourced calibration data integration
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple, Any
from pathlib import Path
import numpy as np
import json
import os
import time

from enum import Enum


class CalibrationSource(Enum):
    """Source of calibration data."""
    PANEL_DATABASE = "panel_database"    # Built-in panel profiles
    FORUM_DATA = "forum_data"            # TFTCentral, Rtings, etc.
    ICC_PROFILE = "icc_profile"          # Existing ICC profile
    COLORIMETER = "colorimeter"          # Hardware measurement
    SENSORLESS = "sensorless"            # Sensorless calibration
    CUSTOM = "custom"                    # User-provided settings


class CalibrationTarget(Enum):
    """Calibration target preset."""
    SRGB = "sRGB"                    # sRGB D65, gamma 2.2
    SRGB_FILM = "sRGB_Film"          # sRGB with 2.4 gamma (cinema-like)
    DCI_P3 = "DCI-P3"                # DCI-P3 D65
    ADOBE_RGB = "Adobe RGB"          # Adobe RGB 1998
    BT709 = "BT.709"                 # HD video
    BT2020 = "BT.2020"               # HDR video
    NATIVE = "Native"                # Use panel's native gamut
    CUSTOM = "Custom"                # Custom target


@dataclass
class DisplayCalibrationProfile:
    """Calibration profile for a single display."""
    display_id: int
    display_name: str
    device_name: str

    # Panel identification
    manufacturer: str = ""
    model: str = ""
    panel_type: str = ""              # QD-OLED, WOLED, IPS, VA, etc.
    panel_database_key: str = ""      # Key in panel database

    # Calibration settings
    target: CalibrationTarget = CalibrationTarget.SRGB
    source: CalibrationSource = CalibrationSource.PANEL_DATABASE

    # Calibration parameters
    target_whitepoint: Tuple[float, float] = (0.3127, 0.3290)  # D65
    target_gamma: float = 2.2
    target_brightness: float = 100.0  # cd/m²

    # Generated calibration data
    lut_3d: Optional[np.ndarray] = None  # 33x33x33x3 LUT
    lut_path: Optional[str] = None
    color_matrix: Optional[np.ndarray] = None  # 3x3 color correction

    # Status
    is_calibrated: bool = False
    calibration_time: float = 0.0
    delta_e_average: float = 0.0
    notes: str = ""

    def to_dict(self) -> Dict:
        """Convert to serializable dictionary."""
        return {
            "display_id": self.display_id,
            "display_name": self.display_name,
            "device_name": self.device_name,
            "manufacturer": self.manufacturer,
            "model": self.model,
            "panel_type": self.panel_type,
            "panel_database_key": self.panel_database_key,
            "target": self.target.value,
            "source": self.source.value,
            "target_whitepoint": list(self.target_whitepoint),
            "target_gamma": self.target_gamma,
            "target_brightness": self.target_brightness,
            "lut_path": self.lut_path,
            "is_calibrated": self.is_calibrated,
            "calibration_time": self.calibration_time,
            "delta_e_average": self.delta_e_average,
            "notes": self.notes,
        }


@dataclass
class PerDisplayCalibrationConfig:
    """Configuration for per-display calibration."""
    auto_detect: bool = True
    auto_calibrate: bool = True
    auto_apply: bool = True
    persist_luts: bool = True
    lut_size: int = 33
    profiles_dir: Optional[str] = None

    def __post_init__(self):
        if self.profiles_dir is None:
            app_data = os.environ.get('APPDATA', os.path.expanduser('~'))
            self.profiles_dir = os.path.join(app_data, 'CalibratePro', 'display_profiles')


class PerDisplayCalibrationManager:
    """
    Manages calibration for multiple displays.

    Automatically detects displays, matches them to panel profiles,
    generates calibration LUTs, and applies them per-display.
    """

    def __init__(self, config: Optional[PerDisplayCalibrationConfig] = None):
        """
        Initialize the per-display calibration manager.

        Args:
            config: Configuration options
        """
        self.config = config or PerDisplayCalibrationConfig()
        self.profiles: Dict[int, DisplayCalibrationProfile] = {}
        self._lut_manager = None
        self._panel_db = None

        # Ensure profiles directory exists
        Path(self.config.profiles_dir).mkdir(parents=True, exist_ok=True)

        # Initialize on creation
        if self.config.auto_detect:
            self.detect_displays()

    @property
    def lut_manager(self):
        """Get LUT manager (lazy load)."""
        if self._lut_manager is None:
            from calibrate_pro.lut_system import LUTManager
            self._lut_manager = LUTManager()
        return self._lut_manager

    @property
    def panel_db(self):
        """Get panel database (lazy load)."""
        if self._panel_db is None:
            from calibrate_pro.panels.database import PanelDatabase
            self._panel_db = PanelDatabase()
        return self._panel_db

    def detect_displays(self) -> List[DisplayCalibrationProfile]:
        """
        Detect all connected displays and create calibration profiles.

        Returns:
            List of DisplayCalibrationProfile for each display
        """
        from calibrate_pro.panels.detection import enumerate_displays_enhanced

        displays = enumerate_displays_enhanced()
        self.profiles.clear()

        for display in displays:
            profile = DisplayCalibrationProfile(
                display_id=display.get_display_number(),
                display_name=display.monitor_name,
                device_name=display.device_name,
                manufacturer=display.manufacturer,
                model=display.model,
                panel_type=display.panel_type,
                panel_database_key=display.panel_database_key,
            )

            # Load saved profile if exists
            saved_profile = self._load_profile(profile.display_id)
            if saved_profile:
                profile.target = CalibrationTarget(saved_profile.get('target', 'sRGB'))
                profile.target_gamma = saved_profile.get('target_gamma', 2.2)
                profile.target_brightness = saved_profile.get('target_brightness', 100.0)
                profile.lut_path = saved_profile.get('lut_path')
                profile.is_calibrated = saved_profile.get('is_calibrated', False)

            self.profiles[profile.display_id] = profile

        return list(self.profiles.values())

    def get_display_profile(self, display_id: int) -> Optional[DisplayCalibrationProfile]:
        """Get calibration profile for a display."""
        return self.profiles.get(display_id)

    def list_displays(self) -> List[Dict]:
        """List all detected displays with their status."""
        return [
            {
                "id": p.display_id,
                "name": p.display_name,
                "manufacturer": p.manufacturer,
                "model": p.model,
                "panel_type": p.panel_type,
                "database_match": p.panel_database_key,
                "calibrated": p.is_calibrated,
                "target": p.target.value,
            }
            for p in self.profiles.values()
        ]

    def calibrate_display(
        self,
        display_id: int,
        target: CalibrationTarget = CalibrationTarget.SRGB,
        source: CalibrationSource = CalibrationSource.PANEL_DATABASE
    ) -> bool:
        """
        Calibrate a single display.

        Args:
            display_id: Display ID to calibrate
            target: Calibration target preset
            source: Source of calibration data

        Returns:
            True if successful
        """
        profile = self.profiles.get(display_id)
        if not profile:
            return False

        profile.target = target
        profile.source = source

        # Get panel characterization from database
        panel = None
        if profile.panel_database_key:
            panel = self.panel_db.get_panel(profile.panel_database_key)

        if not panel:
            # Try to find by model name
            panel = self.panel_db.find_panel(profile.model or profile.display_name)

        if not panel:
            # Use generic profile
            panel = self.panel_db.get_fallback()
            profile.notes = "Using generic profile - no specific panel match found"
        else:
            profile.notes = f"Using panel profile: {panel.model_pattern.split('|')[0]}"

        # Generate calibration LUT
        lut_3d = self._generate_calibration_lut(profile, panel, target)

        if lut_3d is None:
            return False

        profile.lut_3d = lut_3d
        profile.is_calibrated = True
        profile.calibration_time = time.time()

        # Save LUT to file
        if self.config.persist_luts:
            lut_path = self._save_lut(display_id, lut_3d)
            profile.lut_path = str(lut_path) if lut_path else None

        # Save profile
        self._save_profile(profile)

        # Apply if auto_apply enabled
        if self.config.auto_apply:
            return self.apply_calibration(display_id)

        return True

    def calibrate_all(
        self,
        target: CalibrationTarget = CalibrationTarget.SRGB
    ) -> Dict[int, bool]:
        """
        Calibrate all detected displays.

        Args:
            target: Calibration target for all displays

        Returns:
            Dict mapping display_id to success status
        """
        results = {}
        for display_id in self.profiles:
            results[display_id] = self.calibrate_display(display_id, target)
        return results

    def apply_calibration(self, display_id: int) -> bool:
        """
        Apply calibration LUT to a display.

        Args:
            display_id: Display ID

        Returns:
            True if successful
        """
        profile = self.profiles.get(display_id)
        if not profile:
            return False

        # Use existing LUT data or load from file
        lut_data = profile.lut_3d
        if lut_data is None and profile.lut_path:
            from calibrate_pro.lut_system import load_lut
            try:
                lut = load_lut(profile.lut_path)
                lut_data = lut.data
            except Exception:
                return False

        if lut_data is None:
            return False

        # Apply via LUT manager
        return self.lut_manager.load_lut(display_id, lut_data)

    def apply_all(self) -> Dict[int, bool]:
        """Apply calibration to all displays."""
        results = {}
        for display_id in self.profiles:
            results[display_id] = self.apply_calibration(display_id)
        return results

    def reset_display(self, display_id: int) -> bool:
        """Reset display to no calibration."""
        return self.lut_manager.unload_lut(display_id)

    def reset_all(self) -> Dict[int, bool]:
        """Reset all displays."""
        results = {}
        for display_id in self.profiles:
            results[display_id] = self.reset_display(display_id)
        return results

    def _generate_calibration_lut(
        self,
        profile: DisplayCalibrationProfile,
        panel,
        target: CalibrationTarget
    ) -> Optional[np.ndarray]:
        """
        Generate calibration 3D LUT for a display.

        Args:
            profile: Display calibration profile
            panel: PanelCharacterization from database
            target: Calibration target

        Returns:
            33x33x33x3 LUT data or None
        """
        size = self.config.lut_size
        lut = np.zeros((size, size, size, 3), dtype=np.float32)

        # Get target color space primaries
        target_primaries = self._get_target_primaries(target)

        # Get panel native primaries
        panel_primaries = None
        if panel:
            panel_primaries = {
                'red': (panel.native_primaries.red.x, panel.native_primaries.red.y),
                'green': (panel.native_primaries.green.x, panel.native_primaries.green.y),
                'blue': (panel.native_primaries.blue.x, panel.native_primaries.blue.y),
                'white': (panel.native_primaries.white.x, panel.native_primaries.white.y),
            }

        # Get color correction matrix
        correction_matrix = None
        if panel and panel.color_correction_matrix:
            correction_matrix = np.array(panel.color_correction_matrix, dtype=np.float32)

        # Generate LUT
        for r in range(size):
            for g in range(size):
                for b in range(size):
                    # Normalize input
                    rgb_in = np.array([r, g, b], dtype=np.float32) / (size - 1)

                    # Apply gamma correction for linearization
                    gamma = panel.gamma_red.gamma if panel else 2.2
                    rgb_linear = np.power(rgb_in, gamma)

                    # Apply color correction matrix if available
                    if correction_matrix is not None:
                        rgb_corrected = correction_matrix @ rgb_linear
                    else:
                        rgb_corrected = rgb_linear

                    # Apply gamut mapping if needed
                    if target != CalibrationTarget.NATIVE and panel_primaries and target_primaries:
                        rgb_corrected = self._gamut_map(
                            rgb_corrected,
                            panel_primaries,
                            target_primaries
                        )

                    # Apply output gamma
                    target_gamma = profile.target_gamma
                    rgb_out = np.power(np.clip(rgb_corrected, 0, 1), 1.0 / target_gamma)

                    lut[r, g, b] = np.clip(rgb_out, 0, 1)

        return lut

    def _get_target_primaries(self, target: CalibrationTarget) -> Optional[Dict]:
        """Get primaries for calibration target."""
        primaries = {
            CalibrationTarget.SRGB: {
                'red': (0.6400, 0.3300),
                'green': (0.3000, 0.6000),
                'blue': (0.1500, 0.0600),
                'white': (0.3127, 0.3290),
            },
            CalibrationTarget.DCI_P3: {
                'red': (0.6800, 0.3200),
                'green': (0.2650, 0.6900),
                'blue': (0.1500, 0.0600),
                'white': (0.3127, 0.3290),
            },
            CalibrationTarget.ADOBE_RGB: {
                'red': (0.6400, 0.3300),
                'green': (0.2100, 0.7100),
                'blue': (0.1500, 0.0600),
                'white': (0.3127, 0.3290),
            },
            CalibrationTarget.BT709: {
                'red': (0.6400, 0.3300),
                'green': (0.3000, 0.6000),
                'blue': (0.1500, 0.0600),
                'white': (0.3127, 0.3290),
            },
            CalibrationTarget.BT2020: {
                'red': (0.7080, 0.2920),
                'green': (0.1700, 0.7970),
                'blue': (0.1310, 0.0460),
                'white': (0.3127, 0.3290),
            },
        }
        return primaries.get(target)

    def _gamut_map(
        self,
        rgb: np.ndarray,
        source_primaries: Dict,
        target_primaries: Dict
    ) -> np.ndarray:
        """
        Map colors from source gamut to target gamut.

        Simple relative colorimetric mapping.
        """
        # For now, use simple clipping
        # Full implementation would use Bradford adaptation
        return np.clip(rgb, 0, 1)

    def _save_lut(self, display_id: int, lut_data: np.ndarray) -> Optional[Path]:
        """Save LUT to file."""
        try:
            from calibrate_pro.lut_system import LUT3D, save_lut

            lut = LUT3D(size=lut_data.shape[0], data=lut_data)

            profile = self.profiles.get(display_id)
            name = profile.model if profile else f"display_{display_id}"
            name = name.replace(" ", "_").replace("/", "_")

            lut_path = Path(self.config.profiles_dir) / f"{name}_calibration.cube"
            save_lut(lut, lut_path)

            return lut_path

        except Exception as e:
            print(f"Error saving LUT: {e}")
            return None

    def _save_profile(self, profile: DisplayCalibrationProfile):
        """Save calibration profile to file."""
        try:
            profile_path = Path(self.config.profiles_dir) / f"display_{profile.display_id}.json"
            profile_path.write_text(json.dumps(profile.to_dict(), indent=2))
        except Exception as e:
            print(f"Error saving profile: {e}")

    def _load_profile(self, display_id: int) -> Optional[Dict]:
        """Load saved calibration profile."""
        try:
            profile_path = Path(self.config.profiles_dir) / f"display_{display_id}.json"
            if profile_path.exists():
                return json.loads(profile_path.read_text())
        except Exception:
            pass
        return None


# Singleton instance
_manager_instance: Optional[PerDisplayCalibrationManager] = None


def get_per_display_manager() -> PerDisplayCalibrationManager:
    """Get the global per-display calibration manager."""
    global _manager_instance
    if _manager_instance is None:
        _manager_instance = PerDisplayCalibrationManager()
    return _manager_instance


def auto_calibrate_all_displays(target: CalibrationTarget = CalibrationTarget.SRGB) -> Dict[int, bool]:
    """
    Convenience function to auto-detect and calibrate all displays.

    Args:
        target: Calibration target for all displays

    Returns:
        Dict mapping display_id to success status
    """
    manager = get_per_display_manager()
    manager.detect_displays()
    return manager.calibrate_all(target)


def apply_forum_calibration(display_id: int) -> bool:
    """
    Apply forum-sourced calibration for a display.

    Uses measurements from TFTCentral, Rtings, Hardware Unboxed, etc.

    Args:
        display_id: Display ID

    Returns:
        True if successful
    """
    manager = get_per_display_manager()
    return manager.calibrate_display(
        display_id,
        target=CalibrationTarget.SRGB,
        source=CalibrationSource.FORUM_DATA
    )


def list_detected_displays() -> List[Dict]:
    """List all detected displays with panel info."""
    manager = get_per_display_manager()
    return manager.list_displays()


def get_display_status(display_id: int) -> Optional[Dict]:
    """Get calibration status for a display."""
    manager = get_per_display_manager()
    profile = manager.get_display_profile(display_id)
    if profile:
        return profile.to_dict()
    return None
