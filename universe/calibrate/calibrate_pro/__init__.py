"""
Calibrate Pro - Professional Display Calibration Suite

A comprehensive display calibration application featuring:
- Sensorless Calibration (Delta E < 1.0)
- Hardware Colorimeter Integration (ArgyllCMS)
- System-wide 3D LUT Support (dwm_lut)
- Full HDR Suite (HDR10, HDR10+, HLG, Dolby Vision)
- Professional GUI (PyQt6)

Copyright (c) 2024-2025 Zain Dana Harper
"""

__version__ = "1.0.0"
__author__ = "Zain Dana Harper"

# Lazy imports to avoid circular dependencies
def __getattr__(name):
    if name == "color_math":
        from calibrate_pro.core import color_math
        return color_math
    elif name == "icc_profile":
        from calibrate_pro.core import icc_profile
        return icc_profile
    elif name == "lut_engine":
        from calibrate_pro.core import lut_engine
        return lut_engine
    elif name == "calibration_engine":
        from calibrate_pro.core import calibration_engine
        return calibration_engine
    elif name == "database":
        from calibrate_pro.panels import database
        return database
    elif name == "detection":
        from calibrate_pro.panels import detection
        return detection
    elif name == "neuralux":
        from calibrate_pro.sensorless import neuralux
        return neuralux
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")

__all__ = [
    "color_math",
    "icc_profile",
    "lut_engine",
    "calibration_engine",
    "database",
    "detection",
    "neuralux",
]
