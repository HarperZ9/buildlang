"""
Core Calibration Modules - Professional Color Science Engine

This module provides state-of-the-art color science capabilities:

Color Math (color_math.py):
    - XYZ ↔ Lab ↔ sRGB conversions
    - Bradford/Von Kries chromatic adaptation
    - CIEDE2000 Delta E calculation
    - Standard illuminants (D50, D65)

Advanced Color Models (color_models.py):
    - CAM16 Color Appearance Model (CIE 2016)
    - Jzazbz perceptual color space (HDR up to 10,000 nits)
    - ICtCp color space (Dolby/ITU-R BT.2100)
    - PQ ST.2084 transfer functions

ACES 2.0 (aces.py):
    - Academy Color Encoding System 2.0
    - JMh-based gamut mapping
    - Adaptive tonescale for any output luminance
    - OpenColorIO 2.4 config generation

LUT Engine (lut_engine.py, lut_engine_advanced.py):
    - 3D LUT generation up to 256³
    - Tetrahedral interpolation
    - CAM16-UCS perceptual gamut mapping
    - HDR LUT support (PQ/HLG)
    - Parallel processing for large LUTs

Usage:
    from calibrate_pro.core import (
        # Color math
        xyz_to_lab, delta_e_2000, D65_WHITE,
        # Advanced color models
        CAM16, Jzazbz, ICtCp,
        # ACES 2.0
        ACES2, ACES2Tonescale, ACES2GamutMapper,
        # LUT engine
        AdvancedLUT3D, AdvancedLUTGenerator, LUTManipulator,
    )
"""

# =============================================================================
# Color Math - Basic color space conversions
# =============================================================================

from calibrate_pro.core.color_math import (
    # White points
    D50_WHITE,
    D65_WHITE,
    # XYZ conversions
    xyz_to_lab,
    lab_to_xyz,
    srgb_to_xyz,
    xyz_to_srgb,
    # Chromatic adaptation
    bradford_adapt,
    # Delta E
    delta_e_2000,
)

# =============================================================================
# Advanced Color Models - CAM16, Jzazbz, ICtCp
# =============================================================================

from calibrate_pro.core.color_models import (
    # CAM16 Color Appearance Model
    CAM16,
    CAM16ViewingConditions,
    # Jzazbz (HDR perceptual color space)
    Jzazbz,
    # ICtCp (Dolby/BT.2100)
    ICtCp,
    # PQ transfer functions
    pq_eotf as pq_eotf_10000,
    pq_oetf as pq_oetf_10000,
    # Convenience functions
    xyz_to_cam16_jmh,
    xyz_to_jzazbz,
    rgb_to_ictcp,
    delta_e_hdr,
)

# =============================================================================
# ACES 2.0 - Academy Color Encoding System
# =============================================================================

from calibrate_pro.core.aces import (
    # Color spaces
    AP0_TO_XYZ,
    AP1_TO_XYZ,
    XYZ_TO_AP0,
    XYZ_TO_AP1,
    AP0_TO_AP1,
    AP1_TO_AP0,
    # Output configurations
    OutputDevice,
    OutputConfig,
    # Tonescale
    ACES2Tonescale,
    # Gamut mapping
    ACES2GamutMapper,
    # Main pipeline
    ACES2,
    # Look transforms
    ACES2LookTransform,
    # OCIO config generation
    generate_ocio_config,
    # Convenience functions
    aces_to_srgb,
    aces_to_hdr,
)

# =============================================================================
# Advanced LUT Engine - 256³, CAM16 gamut mapping, HDR
# =============================================================================

from calibrate_pro.core.lut_engine_advanced import (
    # Advanced 3D LUT class
    AdvancedLUT3D,
    # LUT generator with CAM16 gamut mapping
    AdvancedLUTGenerator,
    # LUT manipulation
    LUTManipulator,
)


# =============================================================================
# Public API
# =============================================================================

__all__ = [
    # -------------------------------------------------------------------------
    # Color Math
    # -------------------------------------------------------------------------
    "D50_WHITE",
    "D65_WHITE",
    "xyz_to_lab",
    "lab_to_xyz",
    "srgb_to_xyz",
    "xyz_to_srgb",
    "bradford_adapt",
    "delta_e_2000",

    # -------------------------------------------------------------------------
    # Advanced Color Models
    # -------------------------------------------------------------------------
    # CAM16
    "CAM16",
    "CAM16ViewingConditions",
    # Jzazbz
    "Jzazbz",
    # ICtCp
    "ICtCp",
    # PQ functions (10,000 nit reference)
    "pq_eotf_10000",
    "pq_oetf_10000",
    # Convenience functions
    "xyz_to_cam16_jmh",
    "xyz_to_jzazbz",
    "rgb_to_ictcp",
    "delta_e_hdr",

    # -------------------------------------------------------------------------
    # ACES 2.0
    # -------------------------------------------------------------------------
    # Matrices
    "AP0_TO_XYZ",
    "AP1_TO_XYZ",
    "XYZ_TO_AP0",
    "XYZ_TO_AP1",
    "AP0_TO_AP1",
    "AP1_TO_AP0",
    # Output config
    "OutputDevice",
    "OutputConfig",
    # Pipeline components
    "ACES2Tonescale",
    "ACES2GamutMapper",
    "ACES2",
    "ACES2LookTransform",
    # OCIO
    "generate_ocio_config",
    # Convenience
    "aces_to_srgb",
    "aces_to_hdr",

    # -------------------------------------------------------------------------
    # Advanced LUT Engine
    # -------------------------------------------------------------------------
    "AdvancedLUT3D",
    "AdvancedLUTGenerator",
    "LUTManipulator",
]
