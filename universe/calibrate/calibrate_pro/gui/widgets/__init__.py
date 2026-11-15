"""
Custom Widgets for Calibrate Pro GUI

Provides specialized visualization widgets for color calibration:
- CIE chromaticity diagram
- Gamma/EOTF curves
- Delta E bar charts
- Color swatches
"""

# =============================================================================
# CIE Diagram
# =============================================================================

from calibrate_pro.gui.widgets.cie_diagram import (
    CIEDiagramWidget,
    MeasuredPoint,
    SPECTRAL_LOCUS,
    WHITE_POINTS,
    GAMUTS,
)

# =============================================================================
# Gamma Curves
# =============================================================================

from calibrate_pro.gui.widgets.gamma_curve import (
    GammaCurveWidget,
    GammaInfoPanel,
    CurveData,
    srgb_eotf,
    srgb_oetf,
    bt1886_eotf,
    power_law_eotf,
    l_star_eotf,
)

# =============================================================================
# Delta E Charts
# =============================================================================

from calibrate_pro.gui.widgets.delta_e_chart import (
    DeltaEBarChart,
    DeltaEStatsPanel,
    DeltaEMeasurement,
    DeltaEQuality,
    classify_delta_e,
    get_delta_e_color,
)

# =============================================================================
# Color Swatches
# =============================================================================

from calibrate_pro.gui.widgets.color_swatch import (
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
    # CIE Diagram
    "CIEDiagramWidget",
    "MeasuredPoint",
    "SPECTRAL_LOCUS",
    "WHITE_POINTS",
    "GAMUTS",

    # Gamma Curves
    "GammaCurveWidget",
    "GammaInfoPanel",
    "CurveData",
    "srgb_eotf",
    "srgb_oetf",
    "bt1886_eotf",
    "power_law_eotf",
    "l_star_eotf",

    # Delta E Charts
    "DeltaEBarChart",
    "DeltaEStatsPanel",
    "DeltaEMeasurement",
    "DeltaEQuality",
    "classify_delta_e",
    "get_delta_e_color",

    # Color Swatches
    "ColorSwatch",
    "ComparisonSwatch",
    "ColorInfoPanel",
    "ColorGrid",
    "rgb_to_xyz",
    "xyz_to_lab",
    "rgb_to_lab",
    "delta_e_2000",
]
