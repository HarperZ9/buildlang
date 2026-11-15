"""
Calibrate Pro Startup Module

Provides auto-load functionality for calibration LUTs on system startup.
"""

from .lut_autoload import (
    load_calibration_luts,
    create_startup_shortcut,
    remove_startup,
    check_startup_enabled,
)

from .calibration_loader import (
    run_service,
    apply_saved_calibrations,
    start_service_command,
)

__all__ = [
    'load_calibration_luts',
    'create_startup_shortcut',
    'remove_startup',
    'check_startup_enabled',
    'run_service',
    'apply_saved_calibrations',
    'start_service_command',
]
