"""
Calibrate Pro - Sensorless Calibration Module

Sensorless Calibration Engine for achieving Delta E < 1.0 calibration
without hardware colorimeters.

This module provides:
- Sensorless panel-database calibration
- Test pattern generation
- Visual matching algorithms for user-guided calibration
"""

# =============================================================================
# Sensorless Calibration Engine
# =============================================================================
from .neuralux import (
    SensorlessEngine,
    NeuralUXEngine,  # Backwards compatibility alias
    ColorPatch,
    COLORCHECKER_CLASSIC,
    get_colorchecker_reference,
    calibrate_display,
    verify_display,
)

# =============================================================================
# Pattern Generator
# =============================================================================
from .pattern_generator import (
    # Enums
    PatternType,

    # Data Classes
    PatternConfig,
    TestPattern,

    # Main Classes
    PatternGenerator,

    # Functions
    create_pattern_generator,
)

# =============================================================================
# Visual Matcher
# =============================================================================
from .visual_matcher import (
    # Enums
    MatchingMethod,
    AdjustmentType,

    # Data Classes
    MatchResult,
    CalibrationAdjustment,

    # Main Classes
    VisualMatcher,
    GrayscaleBalancer,
    WhitepointMatcher,

    # Functions
    create_visual_matcher,
)

# =============================================================================
# Auto-Calibration Engine (Zero-Input Calibration)
# =============================================================================
from .auto_calibration import (
    # Enums
    CalibrationRisk,
    CalibrationStep,

    # Data Classes
    UserConsent,
    CalibrationTarget,
    AutoCalibrationResult,

    # Main Classes
    AutoCalibrationEngine,

    # Functions
    one_click_calibrate,
    auto_calibrate_all,
    generate_consent_warning,
)

# =============================================================================
# Module Info
# =============================================================================

__all__ = [
    # Sensorless Calibration Engine
    "SensorlessEngine",
    "NeuralUXEngine",  # Backwards compatibility alias
    "ColorPatch",
    "COLORCHECKER_CLASSIC",
    "get_colorchecker_reference",
    "calibrate_display",
    "verify_display",

    # Pattern Generator
    "PatternType",
    "PatternConfig",
    "TestPattern",
    "PatternGenerator",
    "create_pattern_generator",

    # Visual Matcher
    "MatchingMethod",
    "AdjustmentType",
    "MatchResult",
    "CalibrationAdjustment",
    "VisualMatcher",
    "GrayscaleBalancer",
    "WhitepointMatcher",
    "create_visual_matcher",

    # Auto-Calibration (Zero-Input)
    "CalibrationRisk",
    "CalibrationStep",
    "UserConsent",
    "CalibrationTarget",
    "AutoCalibrationResult",
    "AutoCalibrationEngine",
    "one_click_calibrate",
    "auto_calibrate_all",
    "generate_consent_warning",
]

__version__ = "1.0.0"
