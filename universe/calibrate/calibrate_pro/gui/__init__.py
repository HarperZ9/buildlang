"""
Professional GUI - Calibrate Pro PyQt6 Interface

Comprehensive display calibration interface featuring:
- Dark theme optimized for calibration environment
- Step-by-step calibration wizard
- Multi-monitor support with visual layout
- Real-time measurement display
- Fullscreen test patterns
- CIE chromaticity diagrams
- Gamma curve visualization
- Delta E charts and statistics
- 3D LUT preview and comparison
- Professional verification reports

Usage:
    from calibrate_pro.gui import (
        # Main application
        run_application, MainWindow,
        # Wizard
        CalibrationWizard, CalibrationConfig,
        # Display selection
        DisplaySelector, DisplayInfo,
        # Patterns
        PatternWindow, PatternConfig, PatternType,
        # Visualization
        CIEDiagramWidget, GammaCurveWidget, DeltaEBarChart,
        # LUT preview
        LUTPreviewWidget, LUT3D,
        # Reports
        ReportViewer, CalibrationReport,
    )

    # Run the application
    run_application()
"""

# =============================================================================
# Main Application
# =============================================================================

from calibrate_pro.gui.main_window import (
    MainWindow,
    run_application,
    APP_NAME,
    APP_VERSION,
    APP_ORGANIZATION,
    COLORS,
    DARK_STYLESHEET,
    IconFactory,
    ColorManagementStatus,
    ConsentDialog,
    CalibrationWorker,
    SimulatedMeasurementWindow,
    DashboardPage,
    CalibrationPage,
    VerificationPage,
    ProfilesPage,
    VCGTToolsPage,
    SoftwareColorControlPage,
    DDCControlPage,
    SettingsPage,
)

# =============================================================================
# Calibration Wizard
# =============================================================================

from calibrate_pro.gui.calibration_wizard import (
    CalibrationWizard,
    CalibrationConfig,
    CalibrationMode,
    WhitepointTarget,
    GammaTarget,
    GamutTarget,
    WizardStep,
    DisplaySelectionStep,
    TargetSettingsStep,
    CalibrationModeStep,
    MeasurementStep,
    ProfileGenerationStep,
    VerificationStep,
)

# =============================================================================
# Display Selection
# =============================================================================

from calibrate_pro.gui.display_selector import (
    DisplaySelector,
    DisplayLayoutPreview,
    DisplayMonitorWidget,
    DisplayInfoPanel,
    DisplayInfo,
    DisplayTechnology,
    CalibrationStatus,
)

# =============================================================================
# Pattern Window
# =============================================================================

from calibrate_pro.gui.pattern_window import (
    PatternWindow,
    PatternCanvas,
    PatternRenderer,
    PatternSequencer,
    PatternType,
    PatternConfig,
)

# =============================================================================
# Measurement View
# =============================================================================

from calibrate_pro.gui.measurement_view import (
    MeasurementView,
    Measurement,
    ColorPatchDisplay,
    DeltaEDisplay,
    ValuesPanel,
    MeasurementHistoryTable,
)

# =============================================================================
# LUT Preview
# =============================================================================

from calibrate_pro.gui.lut_preview import (
    LUTPreviewWidget,
    LUTCubeView,
    LUTSliceView,
    BeforeAfterView,
    LUT3D,
)

# =============================================================================
# Report Viewer
# =============================================================================

from calibrate_pro.gui.report_viewer import (
    ReportViewer,
    ReportSummaryPanel,
    SummaryCard,
    CalibrationReport,
    GrayscaleResult,
    ColorCheckerResult,
    GamutCoverage,
)

# =============================================================================
# Visualization Widgets
# =============================================================================

from calibrate_pro.gui.widgets import (
    # CIE Diagram
    CIEDiagramWidget,
    MeasuredPoint,
    SPECTRAL_LOCUS,
    WHITE_POINTS,
    GAMUTS,
    # Gamma Curves
    GammaCurveWidget,
    GammaInfoPanel,
    CurveData,
    srgb_eotf,
    srgb_oetf,
    bt1886_eotf,
    power_law_eotf,
    l_star_eotf,
    # Delta E Charts
    DeltaEBarChart,
    DeltaEStatsPanel,
    DeltaEMeasurement,
    DeltaEQuality,
    classify_delta_e,
    get_delta_e_color,
    # Color Swatches
    ColorSwatch,
    ComparisonSwatch,
    ColorInfoPanel,
    ColorGrid,
    rgb_to_xyz,
    xyz_to_lab,
    rgb_to_lab,
    delta_e_2000,
)

# =============================================================================
# Public API
# =============================================================================

__all__ = [
    # -------------------------------------------------------------------------
    # Main Application
    # -------------------------------------------------------------------------
    "MainWindow",
    "run_application",
    "APP_NAME",
    "APP_VERSION",
    "APP_ORGANIZATION",
    "COLORS",
    "DARK_STYLESHEET",
    "IconFactory",
    "ColorManagementStatus",
    "ConsentDialog",
    "CalibrationWorker",
    "SimulatedMeasurementWindow",
    "DashboardPage",
    "CalibrationPage",
    "VerificationPage",
    "ProfilesPage",
    "VCGTToolsPage",
    "SoftwareColorControlPage",
    "DDCControlPage",
    "SettingsPage",

    # -------------------------------------------------------------------------
    # Calibration Wizard
    # -------------------------------------------------------------------------
    "CalibrationWizard",
    "CalibrationConfig",
    "CalibrationMode",
    "WhitepointTarget",
    "GammaTarget",
    "GamutTarget",
    "WizardStep",
    "DisplaySelectionStep",
    "TargetSettingsStep",
    "CalibrationModeStep",
    "MeasurementStep",
    "ProfileGenerationStep",
    "VerificationStep",

    # -------------------------------------------------------------------------
    # Display Selection
    # -------------------------------------------------------------------------
    "DisplaySelector",
    "DisplayLayoutPreview",
    "DisplayMonitorWidget",
    "DisplayInfoPanel",
    "DisplayInfo",
    "DisplayTechnology",
    "CalibrationStatus",

    # -------------------------------------------------------------------------
    # Pattern Window
    # -------------------------------------------------------------------------
    "PatternWindow",
    "PatternCanvas",
    "PatternRenderer",
    "PatternSequencer",
    "PatternType",
    "PatternConfig",

    # -------------------------------------------------------------------------
    # Measurement View
    # -------------------------------------------------------------------------
    "MeasurementView",
    "Measurement",
    "ColorPatchDisplay",
    "DeltaEDisplay",
    "ValuesPanel",
    "MeasurementHistoryTable",

    # -------------------------------------------------------------------------
    # LUT Preview
    # -------------------------------------------------------------------------
    "LUTPreviewWidget",
    "LUTCubeView",
    "LUTSliceView",
    "BeforeAfterView",
    "LUT3D",

    # -------------------------------------------------------------------------
    # Report Viewer
    # -------------------------------------------------------------------------
    "ReportViewer",
    "ReportSummaryPanel",
    "SummaryCard",
    "CalibrationReport",
    "GrayscaleResult",
    "ColorCheckerResult",
    "GamutCoverage",

    # -------------------------------------------------------------------------
    # CIE Diagram Widget
    # -------------------------------------------------------------------------
    "CIEDiagramWidget",
    "MeasuredPoint",
    "SPECTRAL_LOCUS",
    "WHITE_POINTS",
    "GAMUTS",

    # -------------------------------------------------------------------------
    # Gamma Curve Widget
    # -------------------------------------------------------------------------
    "GammaCurveWidget",
    "GammaInfoPanel",
    "CurveData",
    "srgb_eotf",
    "srgb_oetf",
    "bt1886_eotf",
    "power_law_eotf",
    "l_star_eotf",

    # -------------------------------------------------------------------------
    # Delta E Chart Widget
    # -------------------------------------------------------------------------
    "DeltaEBarChart",
    "DeltaEStatsPanel",
    "DeltaEMeasurement",
    "DeltaEQuality",
    "classify_delta_e",
    "get_delta_e_color",

    # -------------------------------------------------------------------------
    # Color Swatch Widgets
    # -------------------------------------------------------------------------
    "ColorSwatch",
    "ComparisonSwatch",
    "ColorInfoPanel",
    "ColorGrid",
    "rgb_to_xyz",
    "xyz_to_lab",
    "rgb_to_lab",
    "delta_e_2000",
]
