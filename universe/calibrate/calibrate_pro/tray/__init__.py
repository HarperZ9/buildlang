"""
Calibrate Pro - System Tray Application

Provides a system tray icon for quick access to calibration functions.
Uses pystray+PIL when available, falls back to a console service otherwise.
"""

from .tray_app import run_tray_app

__all__ = ["run_tray_app"]
