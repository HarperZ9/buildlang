"""
Windows Platform Backend

Delegates to the existing ``calibrate_pro.panels.detection`` module for
display enumeration, gamma ramp, and ICC profile operations.  This is a
thin wrapper that exposes them through the unified
:class:`~calibrate_pro.platform.base.PlatformBackend` interface.
"""

from __future__ import annotations

from typing import List, Optional

from calibrate_pro.platform.base import (
    DisplayInfo as PlatformDisplayInfo,
    PlatformBackend,
)


class WindowsBackend(PlatformBackend):
    """
    Windows implementation using Win32 / GDI / MSCMS APIs.

    Internally delegates to ``calibrate_pro.panels.detection`` which
    already wraps ``EnumDisplayDevicesW``, ``SetDeviceGammaRamp``,
    ``GetICMProfileW``, etc.
    """

    # ------------------------------------------------------------------
    # Display enumeration
    # ------------------------------------------------------------------

    def enumerate_displays(self) -> List[PlatformDisplayInfo]:
        """Enumerate active displays via EnumDisplayDevicesW."""
        from calibrate_pro.panels.detection import (
            enumerate_displays as win_enumerate,
            get_display_name,
        )

        win_displays = win_enumerate()
        results: List[PlatformDisplayInfo] = []

        for idx, d in enumerate(win_displays):
            try:
                name = get_display_name(d)
            except Exception:
                name = d.monitor_name or f"Display {idx + 1}"

            results.append(
                PlatformDisplayInfo(
                    index=idx,
                    name=name,
                    device_path=d.device_name,
                    is_primary=d.is_primary,
                    width=d.width,
                    height=d.height,
                    refresh_rate=d.refresh_rate,
                    bit_depth=d.bit_depth,
                    position_x=d.position_x,
                    position_y=d.position_y,
                    manufacturer=d.manufacturer,
                    model=d.model,
                    serial=d.serial,
                    current_icc_profile=d.current_profile,
                )
            )

        return results

    # ------------------------------------------------------------------
    # Gamma ramp
    # ------------------------------------------------------------------

    def apply_gamma_ramp(
        self,
        display_index: int,
        red: List[int],
        green: List[int],
        blue: List[int],
    ) -> bool:
        """
        Apply a gamma ramp via ``SetDeviceGammaRamp``.

        Delegates to ``calibrate_pro.panels.detection.set_gamma_ramp``.
        """
        import numpy as np
        from calibrate_pro.panels.detection import (
            enumerate_displays as win_enumerate,
            set_gamma_ramp,
        )

        displays = win_enumerate()
        if display_index < 0 or display_index >= len(displays):
            return False

        device_name = displays[display_index].device_name

        r = np.array(red, dtype=np.uint16)
        g = np.array(green, dtype=np.uint16)
        b = np.array(blue, dtype=np.uint16)

        return set_gamma_ramp(device_name, r, g, b)

    def reset_gamma_ramp(self, display_index: int) -> bool:
        """Reset gamma ramp to linear identity."""
        from calibrate_pro.panels.detection import (
            enumerate_displays as win_enumerate,
            reset_gamma_ramp,
        )

        displays = win_enumerate()
        if display_index < 0 or display_index >= len(displays):
            return False

        device_name = displays[display_index].device_name
        return reset_gamma_ramp(device_name)

    # ------------------------------------------------------------------
    # ICC profile management
    # ------------------------------------------------------------------

    def install_icc_profile(
        self,
        profile_path: str,
        display_index: int,
    ) -> bool:
        """
        Install an ICC profile and set it for the given display.

        Uses ``InstallColorProfileW`` + ``SetICMProfileW`` via the
        existing detection module helpers.
        """
        from calibrate_pro.panels.detection import (
            enumerate_displays as win_enumerate,
            install_profile,
            set_display_profile,
        )

        # Install the profile system-wide
        if not install_profile(profile_path):
            return False

        # Associate with the specific display
        displays = win_enumerate()
        if display_index < 0 or display_index >= len(displays):
            return False

        device_name = displays[display_index].device_name
        return set_display_profile(device_name, profile_path)

    def get_icc_profile(self, display_index: int) -> Optional[str]:
        """Get the active ICC profile path for a display."""
        from calibrate_pro.panels.detection import (
            enumerate_displays as win_enumerate,
            get_display_profile,
        )

        displays = win_enumerate()
        if display_index < 0 or display_index >= len(displays):
            return None

        device_name = displays[display_index].device_name
        return get_display_profile(device_name)
