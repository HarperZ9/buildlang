"""
Linux Platform Backend (Stub)

This module provides the Linux implementation of the
:class:`~calibrate_pro.platform.base.PlatformBackend` interface.

All methods currently raise ``NotImplementedError`` with guidance on
which Linux tools and APIs to use for a full implementation.

Relevant Linux APIs / Tools
----------------------------

Display enumeration
    ``xrandr`` -- ``xrandr --query --verbose`` lists all outputs, modes,
    EDID blobs, and connected displays.
    ``python-xlib`` -- programmatic access to X11 RandR extension.
    ``Wayland`` -- ``wlr-output-management`` protocol, or compositor-
    specific D-Bus interfaces (e.g. GNOME ``org.gnome.Mutter.DisplayConfig``).

EDID parsing
    ``xrandr --prop`` includes raw EDID hex.
    ``/sys/class/drm/card*/edid`` contains binary EDID on DRM-based systems.

Gamma ramp / VCGT
    ``xrandr --output <name> --gamma R:G:B`` -- simple 3-value gamma.
    ``xcalib`` -- load VCGT (ICC profile's gamma table) into X11 gamma LUT.
    ``xgamma`` -- simple gamma adjustment.
    Programmatic: ``XRRSetCrtcGamma`` via python-xlib or ctypes.
    Wayland: ``wlr-gamma-control`` protocol (if supported by compositor).

ICC / Color management
    ``colord`` (D-Bus service) -- manages ICC profiles per-device.
    ``colormgr`` CLI -- ``colormgr find-device`` / ``colormgr device-add-profile``.
    Profile directory: ``/usr/share/color/icc/`` (system) or
    ``~/.local/share/icc/`` (per-user).
    ``oyranos`` -- alternative colour management framework.

    Wayland compositors are beginning native colour management via the
    ``wp-color-management-v1`` protocol (still experimental as of 2025).
"""

from __future__ import annotations

from typing import List, Optional

from calibrate_pro.platform.base import (
    DisplayInfo as PlatformDisplayInfo,
    PlatformBackend,
)


class LinuxBackend(PlatformBackend):
    """
    Linux stub backend.

    Every method raises ``NotImplementedError`` with a description of the
    native tool or API that should be used.
    """

    def enumerate_displays(self) -> List[PlatformDisplayInfo]:
        """
        Enumerate displays on Linux.

        Implementation notes (X11):
        - Parse ``xrandr --query --verbose`` to list connected outputs.
        - For each output, extract name (e.g. ``DP-1``), resolution,
          refresh rate, and EDID blob.
        - Parse the EDID blob with the same logic used on Windows
          (``calibrate_pro.panels.detection.parse_edid``).

        Implementation notes (Wayland):
        - On ``wlroots``-based compositors, use ``wlr-output-management``
          protocol via ``pywlroots`` or D-Bus.
        - On GNOME/Mutter, query ``org.gnome.Mutter.DisplayConfig``
          via D-Bus (``GetResources`` method).
        - On KDE, use ``org.kde.KScreen`` D-Bus interface.

        Implementation notes (DRM/KMS):
        - Read ``/sys/class/drm/card*/status`` and
          ``/sys/class/drm/card*/edid`` for low-level enumeration that
          works without a display server.
        """
        raise NotImplementedError(
            "Linux display enumeration is not yet implemented. "
            "Use xrandr (X11) or wlr-output-management / Mutter D-Bus (Wayland)."
        )

    def apply_gamma_ramp(
        self,
        display_index: int,
        red: List[int],
        green: List[int],
        blue: List[int],
    ) -> bool:
        """
        Apply a gamma ramp on Linux.

        Implementation notes (X11):
        - Use ``XRRSetCrtcGamma`` from the X11 RandR extension.
        - Access via ``python-xlib`` or ``ctypes`` binding to
          ``libXrandr.so``.
        - Alternatively, shell out to ``xcalib`` with a temporary ICC
          profile containing the desired VCGT tag.
        - The ramp size varies by GPU driver (typically 256 or 1024).
          Query with ``XRRGetCrtcGamma`` first.

        Implementation notes (Wayland):
        - Use ``wlr-gamma-control-unstable-v1`` protocol on
          ``wlroots``-based compositors (Sway, Hyprland, etc.).
        - GNOME currently does not expose gamma ramp control to clients;
          a ``colord`` profile-based approach is the alternative.
        - KDE supports gamma via ``colord`` integration.
        """
        raise NotImplementedError(
            "Linux gamma ramp application is not yet implemented. "
            "Use XRRSetCrtcGamma (X11) or wlr-gamma-control (Wayland)."
        )

    def reset_gamma_ramp(self, display_index: int) -> bool:
        """
        Reset gamma ramp to identity on Linux.

        Implementation notes:
        - X11: Call ``XRRSetCrtcGamma`` with a linear 0-65535 ramp,
          or run ``xcalib -c`` to clear calibration.
        - Wayland: Destroy the ``wlr-gamma-control`` object to restore
          default gamma, or reload the ``colord`` device profile.
        """
        raise NotImplementedError(
            "Linux gamma ramp reset is not yet implemented. "
            "Use xcalib -c (X11) or destroy wlr-gamma-control (Wayland)."
        )

    def install_icc_profile(
        self,
        profile_path: str,
        display_index: int,
    ) -> bool:
        """
        Install an ICC profile on Linux.

        Implementation notes:
        - Copy the profile to ``~/.local/share/icc/`` (user) or
          ``/usr/share/color/icc/`` (system-wide, requires root).
        - Register with ``colord`` via D-Bus or the ``colormgr`` CLI::

              colormgr device-add-profile <device-id> <profile-object-path>

        - To make it the default, use::

              colormgr device-make-profile-default <device-id> <profile-object-path>

        - On X11, additionally apply the VCGT from the profile using
          ``xcalib <profile_path>`` so that the gamma ramp takes effect
          immediately.
        """
        raise NotImplementedError(
            "Linux ICC profile installation is not yet implemented. "
            "Copy to ~/.local/share/icc/ and register with colord."
        )

    def get_icc_profile(self, display_index: int) -> Optional[str]:
        """
        Get the active ICC profile for a display on Linux.

        Implementation notes:
        - Query ``colord`` via D-Bus::

              colormgr find-device-by-property OutputName <xrandr-output-name>
              colormgr device-get-default-profile <device-object-path>

        - The profile object path includes the file path, or query the
          ``Filename`` property of the profile object.
        - Alternatively, parse ``~/.local/share/icc/`` and
          ``/usr/share/color/icc/`` to find manually installed profiles.
        """
        raise NotImplementedError(
            "Linux ICC profile query is not yet implemented. "
            "Use colormgr device-get-default-profile via colord D-Bus."
        )
