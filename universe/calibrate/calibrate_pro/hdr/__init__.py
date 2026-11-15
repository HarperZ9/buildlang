"""
HDR Calibration Suite - Complete HDR Support

Provides comprehensive HDR calibration capabilities:
- PQ ST.2084 (HDR10) EOTF and metadata
- HLG (Hybrid Log-Gamma) ARIB STD-B67
- HDR10+ dynamic metadata (SMPTE ST.2094-40)
- Dolby Vision profile support (Profile 5/8)
- EOTF calibration and verification
- Tone mapping (HDR to SDR conversion)

Usage:
    from calibrate_pro.hdr import (
        # PQ functions
        pq_eotf, pq_oetf, HDR10Metadata,
        # HLG functions
        hlg_eotf, hlg_oetf,
        # HDR10+
        HDR10PlusMetadata, HDR10PlusToneMapper,
        # Dolby Vision
        DVRPU, DolbyVisionToneMapper,
        # Calibration
        EOTFCalibrator, analyze_pq_eotf,
        # Tone mapping
        hdr_to_sdr, ToneMapOperator
    )
"""

# =============================================================================
# PQ ST.2084 (HDR10)
# =============================================================================

from calibrate_pro.hdr.pq_st2084 import (
    # Constants
    PQ_M1,
    PQ_M2,
    PQ_C1,
    PQ_C2,
    PQ_C3,
    PQ_REFERENCE_WHITE,
    SDR_REFERENCE_WHITE,
    # Transfer functions
    pq_eotf,
    pq_oetf,
    # Calibration functions
    calculate_pq_eotf_error,
    generate_pq_calibration_lut,
    generate_pq_verification_patches,
    pq_code_to_nits,
    nits_to_pq_code,
    assess_pq_display,
    # Metadata
    HDR10Metadata,
    HDR10_PRESETS,
    PQDisplayAssessment,
)

# =============================================================================
# HLG (Hybrid Log-Gamma)
# =============================================================================

from calibrate_pro.hdr.hlg import (
    # Constants
    HLG_A,
    HLG_B,
    HLG_C,
    HLG_REFERENCE_WHITE,
    HLG_BLACK_LEVEL,
    SYSTEM_GAMMA_NOMINAL,
    SYSTEM_GAMMA_BRIGHT,
    SYSTEM_GAMMA_DARK,
    # Transfer functions
    hlg_oetf,
    hlg_oetf_inv,
    hlg_eotf,
    hlg_eotf_inv,
    hlg_ootf,
    # Calibration functions
    generate_hlg_calibration_lut,
    calculate_hlg_eotf_error,
    generate_hlg_verification_patches,
    # Conversion functions
    hlg_to_pq,
    pq_to_hlg,
    hlg_to_sdr,
    # Settings
    HLGDisplaySettings,
)

# =============================================================================
# HDR10+ Dynamic Metadata
# =============================================================================

from calibrate_pro.hdr.hdr10plus import (
    # Constants
    HDR10PLUS_APPLICATION_IDENTIFIER,
    HDR10PLUS_APPLICATION_VERSION,
    MAX_WINDOWS,
    MAX_BEZIER_ANCHORS,
    NUM_PERCENTILES,
    DEFAULT_PERCENTILES,
    # Enums
    ProcessingWindowFlag,
    # Data structures
    BezierCurve,
    DistributionData,
    ProcessingWindow,
    HDR10PlusMetadata,
    # Tone mapping
    HDR10PlusToneMapper,
    # Analysis functions
    analyze_frame,
    detect_scene_change,
    # Serialization
    parse_sei_payload,
    serialize_metadata,
    # Calibration
    generate_hdr10plus_test_scenes,
    create_hdr10plus_calibration_luts,
)

# =============================================================================
# Dolby Vision
# =============================================================================

from calibrate_pro.hdr.dolby_vision import (
    # Enums
    DVProfile,
    DVColorSpace,
    DVTransferFunction,
    DVSignalRange,
    # Data structures
    DVPrimaries,
    DVContentRange,
    DVTrimPass,
    DVPolynomialCurve,
    DVRPU,
    # Tone mapping
    DolbyVisionToneMapper,
    # Profile creation
    create_profile5_rpu,
    create_profile8_rpu,
    create_calibration_rpu,
    # Parsing
    parse_rpu_header,
    # Calibration
    DVCalibrationResult,
    calibrate_for_dolby_vision,
    generate_dv_verification_patches,
)

# =============================================================================
# EOTF Calibration
# =============================================================================

from calibrate_pro.hdr.eotf_calibration import (
    # Enums
    EOTFType,
    CalibrationStandard,
    # Constants
    REFERENCE_LEVELS,
    # Patch generation
    EOTFPatch,
    generate_pq_patches,
    generate_hlg_patches,
    # Measurement and analysis
    EOTFMeasurement,
    EOTFAnalysis,
    analyze_pq_eotf,
    analyze_hlg_eotf,
    # LUT generation
    generate_eotf_correction_lut,
    generate_grayscale_correction_matrix,
    # Calibration workflow
    CalibrationTarget,
    CalibrationResult,
    EOTFCalibrator,
)

# =============================================================================
# Tone Mapping
# =============================================================================

from calibrate_pro.hdr.tone_mapping import (
    # Operators
    ToneMapOperator,
    ToneMapSettings,
    # Core tone mapping functions
    tone_map_linear,
    tone_map_reinhard,
    tone_map_reinhard_extended,
    tone_map_aces,
    tone_map_hable,
    tone_map_bt2390,
    tone_map_exponential,
    # RGB tone mapping
    tone_map_rgb,
    # LUT generation
    generate_tonemap_1d_lut,
    generate_tonemap_3d_lut,
    # HDR to SDR conversion
    HDRToSDRConverter,
    hdr_to_sdr,
    compare_operators,
)

# =============================================================================
# Professional Mastering Standards
# =============================================================================

from calibrate_pro.hdr.mastering_standards import (
    # Enums
    ComplianceLevel,
    # Base class
    MasteringSpec,
    # Mastering profiles
    NetflixMasteringProfile,
    EBUGrade1Profile,
    DCIMasteringProfile,
    DisneyPlusProfile,
    AppleTVProfile,
    BBCBroadcastProfile,
    # Validation and utilities
    validate_mastering_compliance,
    get_recommended_targets,
    generate_compliance_report,
)

# =============================================================================
# Unified HDR Calibration Suite
# =============================================================================

from calibrate_pro.hdr.hdr_calibration import (
    # Enums
    HDRFormat,
    CalibrationMode,
    # Configuration and results
    HDRCalibrationConfig,
    HDRMeasurement,
    HDRCalibrationResult,
    # Calibration classes
    HDR10Calibration,
    HDR10PlusCalibration,
    HLGCalibration,
    # Unified suite
    HDRCalibrationSuite,
    # Convenience functions
    calibrate_hdr10,
    calibrate_hlg,
)


# =============================================================================
# Public API
# =============================================================================

__all__ = [
    # -------------------------------------------------------------------------
    # PQ ST.2084 (HDR10)
    # -------------------------------------------------------------------------
    # Constants
    "PQ_M1",
    "PQ_M2",
    "PQ_C1",
    "PQ_C2",
    "PQ_C3",
    "PQ_REFERENCE_WHITE",
    "SDR_REFERENCE_WHITE",
    # Transfer functions
    "pq_eotf",
    "pq_oetf",
    # Calibration
    "calculate_pq_eotf_error",
    "generate_pq_calibration_lut",
    "generate_pq_verification_patches",
    "pq_code_to_nits",
    "nits_to_pq_code",
    "assess_pq_display",
    # Metadata
    "HDR10Metadata",
    "HDR10_PRESETS",
    "PQDisplayAssessment",

    # -------------------------------------------------------------------------
    # HLG
    # -------------------------------------------------------------------------
    # Constants
    "HLG_A",
    "HLG_B",
    "HLG_C",
    "HLG_REFERENCE_WHITE",
    "HLG_BLACK_LEVEL",
    "SYSTEM_GAMMA_NOMINAL",
    "SYSTEM_GAMMA_BRIGHT",
    "SYSTEM_GAMMA_DARK",
    # Transfer functions
    "hlg_oetf",
    "hlg_oetf_inv",
    "hlg_eotf",
    "hlg_eotf_inv",
    "hlg_ootf",
    # Calibration
    "generate_hlg_calibration_lut",
    "calculate_hlg_eotf_error",
    "generate_hlg_verification_patches",
    # Conversion
    "hlg_to_pq",
    "pq_to_hlg",
    "hlg_to_sdr",
    # Settings
    "HLGDisplaySettings",

    # -------------------------------------------------------------------------
    # HDR10+
    # -------------------------------------------------------------------------
    # Constants
    "HDR10PLUS_APPLICATION_IDENTIFIER",
    "HDR10PLUS_APPLICATION_VERSION",
    "MAX_WINDOWS",
    "MAX_BEZIER_ANCHORS",
    "NUM_PERCENTILES",
    "DEFAULT_PERCENTILES",
    # Enums
    "ProcessingWindowFlag",
    # Data structures
    "BezierCurve",
    "DistributionData",
    "ProcessingWindow",
    "HDR10PlusMetadata",
    # Tone mapping
    "HDR10PlusToneMapper",
    # Analysis
    "analyze_frame",
    "detect_scene_change",
    # Serialization
    "parse_sei_payload",
    "serialize_metadata",
    # Calibration
    "generate_hdr10plus_test_scenes",
    "create_hdr10plus_calibration_luts",

    # -------------------------------------------------------------------------
    # Dolby Vision
    # -------------------------------------------------------------------------
    # Enums
    "DVProfile",
    "DVColorSpace",
    "DVTransferFunction",
    "DVSignalRange",
    # Data structures
    "DVPrimaries",
    "DVContentRange",
    "DVTrimPass",
    "DVPolynomialCurve",
    "DVRPU",
    # Tone mapping
    "DolbyVisionToneMapper",
    # Profile creation
    "create_profile5_rpu",
    "create_profile8_rpu",
    "create_calibration_rpu",
    # Parsing
    "parse_rpu_header",
    # Calibration
    "DVCalibrationResult",
    "calibrate_for_dolby_vision",
    "generate_dv_verification_patches",

    # -------------------------------------------------------------------------
    # EOTF Calibration
    # -------------------------------------------------------------------------
    # Enums
    "EOTFType",
    "CalibrationStandard",
    # Constants
    "REFERENCE_LEVELS",
    # Patch generation
    "EOTFPatch",
    "generate_pq_patches",
    "generate_hlg_patches",
    # Measurement
    "EOTFMeasurement",
    "EOTFAnalysis",
    "analyze_pq_eotf",
    "analyze_hlg_eotf",
    # LUT generation
    "generate_eotf_correction_lut",
    "generate_grayscale_correction_matrix",
    # Workflow
    "CalibrationTarget",
    "CalibrationResult",
    "EOTFCalibrator",

    # -------------------------------------------------------------------------
    # Tone Mapping
    # -------------------------------------------------------------------------
    # Operators
    "ToneMapOperator",
    "ToneMapSettings",
    # Core functions
    "tone_map_linear",
    "tone_map_reinhard",
    "tone_map_reinhard_extended",
    "tone_map_aces",
    "tone_map_hable",
    "tone_map_bt2390",
    "tone_map_exponential",
    # RGB processing
    "tone_map_rgb",
    # LUT generation
    "generate_tonemap_1d_lut",
    "generate_tonemap_3d_lut",
    # Conversion
    "HDRToSDRConverter",
    "hdr_to_sdr",
    "compare_operators",

    # -------------------------------------------------------------------------
    # Professional Mastering Standards
    # -------------------------------------------------------------------------
    # Enums
    "ComplianceLevel",
    # Base class
    "MasteringSpec",
    # Profiles
    "NetflixMasteringProfile",
    "EBUGrade1Profile",
    "DCIMasteringProfile",
    "DisneyPlusProfile",
    "AppleTVProfile",
    "BBCBroadcastProfile",
    # Validation and utilities
    "validate_mastering_compliance",
    "get_recommended_targets",
    "generate_compliance_report",

    # -------------------------------------------------------------------------
    # Unified HDR Calibration Suite
    # -------------------------------------------------------------------------
    # Enums
    "HDRFormat",
    "CalibrationMode",
    # Configuration
    "HDRCalibrationConfig",
    "HDRMeasurement",
    "HDRCalibrationResult",
    # Calibration classes
    "HDR10Calibration",
    "HDR10PlusCalibration",
    "HLGCalibration",
    # Unified suite
    "HDRCalibrationSuite",
    # Convenience functions
    "calibrate_hdr10",
    "calibrate_hlg",
]
