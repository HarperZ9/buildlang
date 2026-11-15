"""
ICC Profile System - Professional Profile Generation and Management

Provides comprehensive ICC profile capabilities:
- ICC v4.4 profile generation with full tag support
- Video Card Gamma Table (VCGT) handling
- Windows HDR MHC2 tag support
- System profile installation and management

Usage:
    from calibrate_pro.profiles import (
        # Profile creation
        ICCProfile, create_calibration_profile,
        # VCGT
        VCGTTable, GammaRampController,
        # HDR
        MHC2Tag, create_hdr_profile_with_mhc2,
        # Installation
        install_profile, associate_profile_with_display,
        quick_calibrate_display
    )
"""

# =============================================================================
# ICC v4.4 Profile Generation
# =============================================================================

from calibrate_pro.profiles.icc_v4 import (
    # Constants
    ICC_MAGIC,
    ICC_VERSION_4_4,
    # Enums
    ProfileClass,
    ColorSpace,
    Platform,
    RenderingIntent,
    TagSignature,
    # Data structures
    XYZNumber,
    DateTimeNumber,
    MultiLocalizedString,
    ParametricCurve,
    CurveData,
    MeasurementData,
    CLUT,
    # Main profile class
    ICCProfile,
    # Helper functions
    create_calibration_profile,
    create_lut_profile,
)

# =============================================================================
# Video Card Gamma Table (VCGT)
# =============================================================================

from calibrate_pro.profiles.vcgt import (
    # Constants
    VCGT_SIZE_STANDARD,
    VCGT_SIZE_EXTENDED,
    VCGT_SIZE_MAXIMUM,
    GAMMA_RAMP_SIZE,
    # Main VCGT class
    VCGTTable,
    # Gamma ramp controller
    GammaRampController,
    # Profile VCGT extraction
    extract_vcgt_from_profile,
    embed_vcgt_in_profile,
    # Calibration helpers
    generate_correction_vcgt,
    generate_rgb_correction_vcgt,
    generate_whitepoint_vcgt,
)

# =============================================================================
# Windows HDR MHC2 Tag
# =============================================================================

from calibrate_pro.profiles.mhc2 import (
    # Constants
    MHC2_TAG_SIGNATURE,
    MHC2_VERSION_1,
    MHC2_VERSION_2,
    DEFAULT_SDR_WHITE_LEVEL,
    DEFAULT_MIN_LUMINANCE,
    DEFAULT_MAX_LUMINANCE,
    SDR_WHITE_MIN,
    SDR_WHITE_MAX,
    # Data structures
    MHC2ColorMatrix,
    MHC2ToneCurve,
    MHC2Tag,
    WindowsHDRSettings,
    # Profile integration
    extract_mhc2_from_profile,
    create_hdr_profile_with_mhc2,
    # MHC2 ICC profile generation (Phase 2.1)
    generate_mhc2_profile,
    install_mhc2_profile,
    # Helpers
    get_recommended_sdr_white,
    calculate_hdr_headroom,
)

# =============================================================================
# Profile Installation and Management
# =============================================================================

from calibrate_pro.profiles.profile_installer import (
    # Constants
    ProfileScope,
    ProfileAssociation,
    ColorProfileType,
    # Display enumeration
    DisplayDevice,
    MonitorInfo,
    enumerate_displays,
    get_monitor_info,
    # Profile installation
    get_profile_directory,
    install_profile,
    uninstall_profile,
    list_installed_profiles,
    # Profile association
    associate_profile_with_display,
    disassociate_profile_from_display,
    get_display_profile,
    get_associated_profiles,
    # Backup and restore
    ProfileBackup,
    backup_profiles,
    restore_profiles,
    # VCGT loading
    load_profile_vcgt,
    reset_display_gamma,
    # Convenience functions
    quick_calibrate_display,
    get_display_calibration_status,
)


# =============================================================================
# Public API
# =============================================================================

__all__ = [
    # -------------------------------------------------------------------------
    # ICC v4.4 Profile
    # -------------------------------------------------------------------------
    # Constants
    "ICC_MAGIC",
    "ICC_VERSION_4_4",
    # Enums
    "ProfileClass",
    "ColorSpace",
    "Platform",
    "RenderingIntent",
    "TagSignature",
    # Data structures
    "XYZNumber",
    "DateTimeNumber",
    "MultiLocalizedString",
    "ParametricCurve",
    "CurveData",
    "MeasurementData",
    "CLUT",
    # Profile class
    "ICCProfile",
    # Helpers
    "create_calibration_profile",
    "create_lut_profile",

    # -------------------------------------------------------------------------
    # VCGT
    # -------------------------------------------------------------------------
    # Constants
    "VCGT_SIZE_STANDARD",
    "VCGT_SIZE_EXTENDED",
    "VCGT_SIZE_MAXIMUM",
    "GAMMA_RAMP_SIZE",
    # Classes
    "VCGTTable",
    "GammaRampController",
    # Functions
    "extract_vcgt_from_profile",
    "embed_vcgt_in_profile",
    "generate_correction_vcgt",
    "generate_rgb_correction_vcgt",
    "generate_whitepoint_vcgt",

    # -------------------------------------------------------------------------
    # MHC2 (Windows HDR)
    # -------------------------------------------------------------------------
    # Constants
    "MHC2_TAG_SIGNATURE",
    "MHC2_VERSION_1",
    "MHC2_VERSION_2",
    "DEFAULT_SDR_WHITE_LEVEL",
    "DEFAULT_MIN_LUMINANCE",
    "DEFAULT_MAX_LUMINANCE",
    "SDR_WHITE_MIN",
    "SDR_WHITE_MAX",
    # Data structures
    "MHC2ColorMatrix",
    "MHC2ToneCurve",
    "MHC2Tag",
    "WindowsHDRSettings",
    # Functions
    "extract_mhc2_from_profile",
    "create_hdr_profile_with_mhc2",
    "generate_mhc2_profile",
    "install_mhc2_profile",
    "get_recommended_sdr_white",
    "calculate_hdr_headroom",

    # -------------------------------------------------------------------------
    # Profile Installation
    # -------------------------------------------------------------------------
    # Enums
    "ProfileScope",
    "ProfileAssociation",
    "ColorProfileType",
    # Display info
    "DisplayDevice",
    "MonitorInfo",
    "enumerate_displays",
    "get_monitor_info",
    # Installation
    "get_profile_directory",
    "install_profile",
    "uninstall_profile",
    "list_installed_profiles",
    # Association
    "associate_profile_with_display",
    "disassociate_profile_from_display",
    "get_display_profile",
    "get_associated_profiles",
    # Backup/Restore
    "ProfileBackup",
    "backup_profiles",
    "restore_profiles",
    # VCGT loading
    "load_profile_vcgt",
    "reset_display_gamma",
    # Convenience
    "quick_calibrate_display",
    "get_display_calibration_status",
]
