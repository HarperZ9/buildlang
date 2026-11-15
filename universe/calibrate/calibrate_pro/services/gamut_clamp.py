"""
System-Wide sRGB Gamut Clamp

Wide-gamut displays (QD-OLED, P3, etc.) show oversaturated colors in
non-color-managed applications — games, browsers, the Windows desktop.
This is the single most common complaint from wide-gamut display owners.

This service applies a system-wide sRGB gamut clamp via dwm_lut that
affects ALL applications, solving the oversaturation problem at the
compositor level. Can be toggled instantly from the system tray.

The clamp LUT maps the panel's native gamut to sRGB using Oklab
perceptual compression (no hue shifts in blue/purple).
"""

import logging
from pathlib import Path
from typing import Optional
import os

logger = logging.getLogger(__name__)


class GamutClamp:
    """
    System-wide gamut clamp toggle.

    Applies or removes an sRGB compression LUT via dwm_lut.
    Designed to be toggled instantly from the tray icon.
    """

    def __init__(self, display_index: int = 0):
        self.display_index = display_index
        self._active = False
        self._lut_path: Optional[Path] = None

    @property
    def is_active(self) -> bool:
        return self._active

    def enable(self, panel_key: str = None) -> bool:
        """
        Enable sRGB gamut clamp for the display.

        Generates (or reuses) an sRGB compression LUT and applies it
        system-wide via dwm_lut.

        Args:
            panel_key: Panel database key (auto-detects if None)

        Returns:
            True if clamp was applied successfully
        """
        # Generate the clamp LUT if we don't have one cached
        if self._lut_path is None or not self._lut_path.exists():
            self._lut_path = self._generate_clamp_lut(panel_key)
            if self._lut_path is None:
                return False

        # Apply via dwm_lut
        try:
            from calibrate_pro.lut_system.dwm_lut import DwmLutController
            dwm = DwmLutController()
            if dwm.is_available:
                if dwm.load_lut_file(self.display_index, self._lut_path):
                    self._active = True
                    return True
        except Exception as e:
            logger.error("GamutClamp enable failed: %s", e)

        return False

    def disable(self) -> bool:
        """Remove the sRGB gamut clamp."""
        try:
            from calibrate_pro.lut_system.dwm_lut import remove_lut
            remove_lut(self.display_index)
            self._active = False
            return True
        except Exception:
            return False

    def toggle(self, panel_key: str = None) -> bool:
        """Toggle the clamp on/off. Returns new state."""
        if self._active:
            self.disable()
            return False
        else:
            self.enable(panel_key)
            return True

    def _generate_clamp_lut(self, panel_key: str = None) -> Optional[Path]:
        """Generate an sRGB compression LUT for the panel."""
        try:
            from calibrate_pro.panels.detection import enumerate_displays, identify_display
            from calibrate_pro.panels.database import PanelDatabase
            from calibrate_pro.sensorless.neuralux import SensorlessEngine

            # Find panel
            db = PanelDatabase()
            if panel_key:
                panel = db.get_panel(panel_key)
            else:
                displays = enumerate_displays()
                if self.display_index < len(displays):
                    key = identify_display(displays[self.display_index])
                    panel = db.get_panel(key) if key else db.get_fallback()
                else:
                    panel = db.get_fallback()

            if panel is None:
                return None

            # Generate sRGB target LUT using Oklab perceptual mapping
            engine = SensorlessEngine()
            engine.current_panel = panel
            lut = engine.create_3d_lut(panel, size=33, target="sRGB")

            # Save to temp location
            clamp_dir = Path(os.environ.get("APPDATA", Path.home())) / "CalibratePro" / "clamp"
            clamp_dir.mkdir(parents=True, exist_ok=True)
            lut_path = clamp_dir / f"srgb_clamp_{self.display_index}.cube"
            lut.save(lut_path)

            return lut_path

        except Exception as e:
            logger.error("Clamp LUT generation failed: %s", e)
            return None


def get_clamp_for_display(display_index: int = 0) -> GamutClamp:
    """Get a GamutClamp instance for a display."""
    return GamutClamp(display_index)
