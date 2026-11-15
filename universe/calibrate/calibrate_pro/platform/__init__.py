"""
Calibrate Pro - Cross-Platform Display Backend (Phase 6)

Provides a unified API for display enumeration, gamma ramp manipulation,
and ICC profile management across Windows, macOS, and Linux.

Usage::

    from calibrate_pro.platform import get_platform_backend

    backend = get_platform_backend()
    displays = backend.enumerate_displays()
    backend.apply_gamma_ramp(0, red, green, blue)

Currently only the Windows backend is fully implemented.  macOS and Linux
backends are stubs with ``NotImplementedError`` and comments indicating
which native APIs to use for a future implementation.
"""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from calibrate_pro.platform.base import PlatformBackend


def get_platform_backend() -> "PlatformBackend":
    """
    Get the appropriate platform backend for the current OS.

    Returns
    -------
    PlatformBackend
        A concrete backend for the running operating system.

    Raises
    ------
    NotImplementedError
        If the current platform is not yet supported (macOS, Linux stubs
        will raise on individual method calls).
    """
    if sys.platform == "win32":
        from calibrate_pro.platform.windows import WindowsBackend
        return WindowsBackend()
    elif sys.platform == "darwin":
        from calibrate_pro.platform.macos import MacOSBackend
        return MacOSBackend()
    else:
        from calibrate_pro.platform.linux import LinuxBackend
        return LinuxBackend()


__all__ = ["get_platform_backend"]
