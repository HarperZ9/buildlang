"""
Abstract base class for platform backends.

Every platform backend (Windows, macOS, Linux) implements this interface.
"""

from __future__ import annotations

import abc
from dataclasses import dataclass
from typing import List, Optional, Tuple


@dataclass
class DisplayInfo:
    """
    Platform-agnostic display information.

    This mirrors the essential fields from
    ``calibrate_pro.panels.detection.DisplayInfo`` but is kept
    independent so the platform layer has no upward dependency.
    """
    index: int
    name: str               # Human-readable name (e.g. "ASUS PG27UCDM")
    device_path: str         # OS-specific device identifier
    is_primary: bool
    width: int
    height: int
    refresh_rate: int
    bit_depth: int = 8
    position_x: int = 0
    position_y: int = 0
    manufacturer: str = ""
    model: str = ""
    serial: str = ""
    current_icc_profile: Optional[str] = None


class PlatformBackend(abc.ABC):
    """
    Abstract platform backend for display calibration operations.

    Concrete subclasses live in ``windows.py``, ``macos.py``, and
    ``linux.py``.
    """

    # ------------------------------------------------------------------
    # Display enumeration
    # ------------------------------------------------------------------

    @abc.abstractmethod
    def enumerate_displays(self) -> List[DisplayInfo]:
        """
        Enumerate all connected, active displays.

        Returns
        -------
        list of DisplayInfo
        """
        ...

    # ------------------------------------------------------------------
    # Gamma ramp (VCGT)
    # ------------------------------------------------------------------

    @abc.abstractmethod
    def apply_gamma_ramp(
        self,
        display_index: int,
        red: List[int],
        green: List[int],
        blue: List[int],
    ) -> bool:
        """
        Set the hardware gamma look-up table (VCGT) for *display_index*.

        Parameters
        ----------
        display_index : int
            Zero-based display index from :meth:`enumerate_displays`.
        red, green, blue : list of int
            256 entries each, values in 0-65535.

        Returns
        -------
        bool
            ``True`` on success.
        """
        ...

    @abc.abstractmethod
    def reset_gamma_ramp(self, display_index: int) -> bool:
        """
        Reset the gamma ramp to a linear (identity) curve.

        Parameters
        ----------
        display_index : int
            Zero-based display index.

        Returns
        -------
        bool
            ``True`` on success.
        """
        ...

    # ------------------------------------------------------------------
    # ICC profile management
    # ------------------------------------------------------------------

    @abc.abstractmethod
    def install_icc_profile(
        self,
        profile_path: str,
        display_index: int,
    ) -> bool:
        """
        Install an ICC profile and associate it with *display_index*.

        Parameters
        ----------
        profile_path : str
            Absolute path to the ``.icc`` / ``.icm`` file.
        display_index : int
            Zero-based display index.

        Returns
        -------
        bool
            ``True`` on success.
        """
        ...

    @abc.abstractmethod
    def get_icc_profile(self, display_index: int) -> Optional[str]:
        """
        Get the path of the currently active ICC profile for *display_index*.

        Returns
        -------
        str or None
            Absolute path to the profile, or ``None`` if none is set.
        """
        ...
