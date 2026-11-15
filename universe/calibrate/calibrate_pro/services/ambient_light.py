"""
Ambient Light Sensor Integration (Phase 5)

Reads ambient light levels and recommends calibration adjustments.

Supports three sensor modes:

- **windows** -- reads the built-in laptop ambient light sensor via WMI /
  PowerShell.  Works on most modern laptops with a light sensor (Surface,
  Dell XPS, ThinkPad, etc.).
- **usb** -- reserved for external USB light sensors (e.g. i1Display,
  Spyder).  Currently a placeholder that can be extended.
- **manual** -- user selects a lighting preset and the service returns
  fixed recommendations.

Recommendations follow ISF (Imaging Science Foundation) and THX
guidelines for display brightness and white point relative to ambient
lighting conditions.
"""

from __future__ import annotations

import logging
import subprocess
import sys
from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple

logger = logging.getLogger("CalibratePro.AmbientLight")


# ---------------------------------------------------------------------------
# Lighting presets for manual mode
# ---------------------------------------------------------------------------

@dataclass
class LightingPreset:
    """A named ambient lighting environment."""
    name: str
    description: str
    lux_low: float
    lux_high: float
    recommended_brightness_nits: float
    recommended_cct: int  # Correlated Color Temperature in Kelvin

    @property
    def lux_midpoint(self) -> float:
        return (self.lux_low + self.lux_high) / 2.0


LIGHTING_PRESETS: Dict[str, LightingPreset] = {
    "bright_office": LightingPreset(
        name="bright_office",
        description="Brightly-lit office or daytime window light (500+ lux)",
        lux_low=500.0,
        lux_high=1000.0,
        recommended_brightness_nits=250.0,
        recommended_cct=6500,  # D65 daylight
    ),
    "dim_office": LightingPreset(
        name="dim_office",
        description="Moderately-lit office or overcast daylight (200-500 lux)",
        lux_low=200.0,
        lux_high=500.0,
        recommended_brightness_nits=180.0,
        recommended_cct=6500,
    ),
    "living_room": LightingPreset(
        name="living_room",
        description="Typical living room or warm interior lighting (100-200 lux)",
        lux_low=100.0,
        lux_high=200.0,
        recommended_brightness_nits=120.0,
        recommended_cct=5500,  # Slightly warm
    ),
    "dark_room": LightingPreset(
        name="dark_room",
        description="Dark or dimly-lit room for critical viewing (<100 lux)",
        lux_low=0.0,
        lux_high=100.0,
        recommended_brightness_nits=80.0,
        recommended_cct=5000,  # Warm / ISF dark-room recommendation
    ),
}


# ---------------------------------------------------------------------------
# Sensor backends
# ---------------------------------------------------------------------------

class _WindowsSensorBackend:
    """Read ambient light from the Windows WMI light sensor."""

    # PowerShell command that queries the Windows Sensor API via WMI.
    # Works on laptops with a built-in ambient light sensor.
    _PS_COMMAND_WMI = (
        "Get-CimInstance -Namespace root/WMI "
        "-ClassName SENSOR_DATA -ErrorAction SilentlyContinue "
        "| Select-Object -ExpandProperty CurrentValue -ErrorAction SilentlyContinue"
    )

    # Alternative: use the Windows.Devices.Sensors namespace (UWP).
    # This works on newer Windows 10/11 machines.
    _PS_COMMAND_UWP = (
        "[Windows.Devices.Sensors.LightSensor, Windows.Foundation, "
        "ContentType=WindowsRuntime] | Out-Null; "
        "$sensor = [Windows.Devices.Sensors.LightSensor]::GetDefault(); "
        "if ($sensor) { $sensor.GetCurrentReading().IlluminanceInLux } "
        "else { 'NO_SENSOR' }"
    )

    def read_lux(self) -> Optional[float]:
        """
        Attempt to read the ambient light sensor.

        Returns the illuminance in lux or ``None`` if no sensor is available.
        """
        # Try UWP sensor API first (more reliable on modern laptops)
        lux = self._try_powershell(self._PS_COMMAND_UWP)
        if lux is not None:
            return lux

        # Fallback to WMI
        lux = self._try_powershell(self._PS_COMMAND_WMI)
        return lux

    @staticmethod
    def _try_powershell(command: str) -> Optional[float]:
        """Run a PowerShell command and parse a numeric lux value."""
        try:
            result = subprocess.run(
                ["powershell", "-NoProfile", "-Command", command],
                capture_output=True,
                text=True,
                timeout=10,
            )
            output = result.stdout.strip()
            if not output or "NO_SENSOR" in output:
                return None
            # The output may contain multiple lines; take the first numeric one
            for line in output.splitlines():
                line = line.strip()
                try:
                    return float(line)
                except ValueError:
                    continue
        except (subprocess.TimeoutExpired, FileNotFoundError, OSError) as exc:
            logger.debug("PowerShell sensor query failed: %s", exc)
        return None


class _USBSensorBackend:
    """Placeholder for external USB light sensors."""

    def read_lux(self) -> Optional[float]:
        """
        Read lux from an external USB colorimeter / light sensor.

        This is a stub.  A full implementation would communicate with
        devices like the X-Rite i1Display Pro or Datacolor SpyderX via
        their USB HID interface (or via ArgyllCMS ``spotread``).
        """
        logger.warning(
            "USB ambient light sensor support is not yet implemented. "
            "Use 'manual' mode instead."
        )
        return None


class _ManualBackend:
    """Manual mode: user picks a lighting preset."""

    def __init__(self) -> None:
        self._preset: Optional[LightingPreset] = None

    def set_preset(self, preset_name: str) -> None:
        """
        Set the current lighting environment.

        Parameters
        ----------
        preset_name : str
            One of ``"bright_office"``, ``"dim_office"``,
            ``"living_room"``, ``"dark_room"``.
        """
        if preset_name not in LIGHTING_PRESETS:
            raise ValueError(
                f"Unknown preset '{preset_name}'. "
                f"Valid presets: {list(LIGHTING_PRESETS.keys())}"
            )
        self._preset = LIGHTING_PRESETS[preset_name]
        logger.info("Manual ambient light set to '%s'", preset_name)

    def read_lux(self) -> Optional[float]:
        """Return the midpoint lux for the selected preset."""
        if self._preset is None:
            return None
        return self._preset.lux_midpoint


# ---------------------------------------------------------------------------
# Public AmbientLightService
# ---------------------------------------------------------------------------

class AmbientLightService:
    """
    Service that reads ambient light levels and recommends calibration
    adjustments (brightness, white-point CCT).

    Parameters
    ----------
    sensor_type : str
        ``"windows"`` (built-in laptop sensor via WMI/UWP),
        ``"usb"`` (external sensor -- stub), or
        ``"manual"`` (user selects a lighting preset).
    """

    def __init__(self, sensor_type: str = "windows") -> None:
        self.sensor_type = sensor_type.lower().strip()

        if self.sensor_type == "windows":
            self._backend = _WindowsSensorBackend()
        elif self.sensor_type == "usb":
            self._backend = _USBSensorBackend()
        elif self.sensor_type == "manual":
            self._backend = _ManualBackend()
        else:
            raise ValueError(
                f"Unknown sensor_type '{sensor_type}'. "
                f"Valid types: 'windows', 'usb', 'manual'."
            )

    # --- Manual-mode helpers ---

    def set_manual_preset(self, preset_name: str) -> None:
        """
        Set the lighting environment for manual mode.

        Only applicable when ``sensor_type="manual"``.

        Parameters
        ----------
        preset_name : str
            One of ``"bright_office"``, ``"dim_office"``,
            ``"living_room"``, ``"dark_room"``.
        """
        if not isinstance(self._backend, _ManualBackend):
            raise RuntimeError(
                "set_manual_preset() is only available in 'manual' mode."
            )
        self._backend.set_preset(preset_name)

    # --- Core API ---

    def get_ambient_lux(self) -> float:
        """
        Read the current ambient light level in lux.

        Returns
        -------
        float
            Illuminance in lux.  Returns ``-1.0`` if the sensor is
            unavailable or the reading failed.
        """
        lux = self._backend.read_lux()
        if lux is None:
            logger.warning("Ambient light sensor returned no data.")
            return -1.0
        return lux

    def get_recommended_brightness(self, lux: float) -> float:
        """
        Recommend display brightness (cd/m2) for the given ambient light.

        Uses a piecewise-linear model based on ISF guidelines:

        - Dark room (<50 lux): 80 nits
        - Dim room (50-200 lux): 80-180 nits (linear ramp)
        - Office (200-500 lux): 180-250 nits
        - Bright (>500 lux): 250-350 nits

        Parameters
        ----------
        lux : float
            Ambient illuminance in lux.

        Returns
        -------
        float
            Recommended display luminance in cd/m2 (nits).
        """
        if lux < 0:
            return 120.0  # safe default

        # Piecewise-linear mapping
        breakpoints: List[Tuple[float, float]] = [
            (0.0, 80.0),
            (50.0, 80.0),
            (200.0, 180.0),
            (500.0, 250.0),
            (1000.0, 350.0),
        ]

        # Clamp to range
        if lux <= breakpoints[0][0]:
            return breakpoints[0][1]
        if lux >= breakpoints[-1][0]:
            return breakpoints[-1][1]

        # Interpolate
        for i in range(len(breakpoints) - 1):
            x0, y0 = breakpoints[i]
            x1, y1 = breakpoints[i + 1]
            if x0 <= lux <= x1:
                t = (lux - x0) / (x1 - x0) if x1 != x0 else 0.0
                return y0 + t * (y1 - y0)

        return 120.0  # fallback

    def get_recommended_whitepoint_cct(self, lux: float) -> int:
        """
        Recommend white-point Correlated Color Temperature for ambient light.

        In bright environments, D65 (6500 K) is standard.
        In dark rooms, a warmer white point (5000-5500 K) reduces eye strain
        and matches the lower-CCT ambient lighting typical of evening
        environments.

        Parameters
        ----------
        lux : float
            Ambient illuminance in lux.

        Returns
        -------
        int
            Recommended CCT in Kelvin (e.g. 5000, 5500, 6500).
        """
        if lux < 0:
            return 6500  # safe default

        # Piecewise mapping:
        #   0 - 50 lux   -> 5000 K  (dark room, warm)
        #   50 - 100 lux  -> 5000 - 5500 K
        #   100 - 300 lux -> 5500 - 6500 K
        #   300+ lux       -> 6500 K (daylight)
        breakpoints: List[Tuple[float, float]] = [
            (0.0, 5000.0),
            (50.0, 5000.0),
            (100.0, 5500.0),
            (300.0, 6500.0),
            (1000.0, 6500.0),
        ]

        if lux <= breakpoints[0][0]:
            return int(breakpoints[0][1])
        if lux >= breakpoints[-1][0]:
            return int(breakpoints[-1][1])

        for i in range(len(breakpoints) - 1):
            x0, y0 = breakpoints[i]
            x1, y1 = breakpoints[i + 1]
            if x0 <= lux <= x1:
                t = (lux - x0) / (x1 - x0) if x1 != x0 else 0.0
                return int(y0 + t * (y1 - y0))

        return 6500

    def get_full_recommendation(self, lux: Optional[float] = None) -> Dict:
        """
        Get a complete ambient-light recommendation.

        If *lux* is ``None``, the sensor is read automatically.

        Returns
        -------
        dict
            Keys: ``lux``, ``brightness_nits``, ``whitepoint_cct``,
            ``preset_name``, ``sensor_type``.
        """
        if lux is None:
            lux = self.get_ambient_lux()

        brightness = self.get_recommended_brightness(lux)
        cct = self.get_recommended_whitepoint_cct(lux)

        # Find closest preset name for display purposes
        preset_name = "unknown"
        if lux >= 0:
            for name, preset in LIGHTING_PRESETS.items():
                if preset.lux_low <= lux <= preset.lux_high:
                    preset_name = name
                    break
            else:
                if lux > 1000:
                    preset_name = "bright_office"
                elif lux < 0:
                    preset_name = "unknown"

        return {
            "lux": lux,
            "brightness_nits": round(brightness, 1),
            "whitepoint_cct": cct,
            "preset_name": preset_name,
            "sensor_type": self.sensor_type,
        }

    @staticmethod
    def get_available_presets() -> Dict[str, str]:
        """
        Return available manual presets with descriptions.

        Returns
        -------
        dict
            ``{preset_name: description}``
        """
        return {
            name: preset.description
            for name, preset in LIGHTING_PRESETS.items()
        }
