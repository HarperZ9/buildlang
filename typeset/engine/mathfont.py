"""OpenType MATH table: the constants that play the part of TeX's math fontdimens.

TeXbook Appendix G reads sigma and xi parameters from the math fonts;
OpenType MATH carries the same quantities by name (Microsoft, OpenType
spec "MATH - The mathematical typesetting table").
"""
from .sfnt import u16, s16

PERCENT = ["ScriptPercentScaleDown", "ScriptScriptPercentScaleDown"]
HEIGHTS = ["DelimitedSubFormulaMinHeight", "DisplayOperatorMinHeight"]
VALUES = [
    "MathLeading", "AxisHeight", "AccentBaseHeight", "FlattenedAccentBaseHeight",
    "SubscriptShiftDown", "SubscriptTopMax", "SubscriptBaselineDropMin",
    "SuperscriptShiftUp", "SuperscriptShiftUpCramped", "SuperscriptBottomMin",
    "SuperscriptBaselineDropMax", "SubSuperscriptGapMin", "SuperscriptBottomMaxWithSubscript",
    "SpaceAfterScript", "UpperLimitGapMin", "UpperLimitBaselineRiseMin", "LowerLimitGapMin",
    "LowerLimitBaselineDropMin", "StackTopShiftUp", "StackTopDisplayStyleShiftUp",
    "StackBottomShiftDown", "StackBottomDisplayStyleShiftDown", "StackGapMin",
    "StackDisplayStyleGapMin", "StretchStackTopShiftUp", "StretchStackBottomShiftDown",
    "StretchStackGapAboveMin", "StretchStackGapBelowMin", "FractionNumeratorShiftUp",
    "FractionNumeratorDisplayStyleShiftUp", "FractionDenominatorShiftDown",
    "FractionDenominatorDisplayStyleShiftDown", "FractionNumeratorGapMin",
    "FractionNumDisplayStyleGapMin", "FractionRuleThickness", "FractionDenominatorGapMin",
    "FractionDenomDisplayStyleGapMin", "SkewedFractionHorizontalGap",
    "SkewedFractionVerticalGap", "OverbarVerticalGap", "OverbarRuleThickness",
    "OverbarExtraAscender", "UnderbarVerticalGap", "UnderbarRuleThickness",
    "UnderbarExtraDescender", "RadicalVerticalGap", "RadicalDisplayStyleVerticalGap",
    "RadicalRuleThickness", "RadicalExtraAscender", "RadicalKernBeforeDegree",
    "RadicalKernAfterDegree",
]


def read_math_constants(font):
    if "MATH" not in font.tables:
        raise ValueError(f"{font.name}: no MATH table")
    d, o = font.table("MATH")
    c = o + u16(d, o + 4)
    out = {PERCENT[0]: s16(d, c), PERCENT[1]: s16(d, c + 2),
           HEIGHTS[0]: u16(d, c + 4), HEIGHTS[1]: u16(d, c + 6)}
    p = c + 8
    for name in VALUES:
        out[name] = s16(d, p)       # MathValueRecord: value, device offset
        p += 4
    out["RadicalDegreeBottomRaisePercent"] = s16(d, p)
    return out


def glyph_bounds(font, gid):
    """(xmin, ymin, xmax, ymax) in font units, from the outline."""
    pts = [(x, y) for c in font.outline(gid) for x, y, _ in c]
    if not pts:
        return 0, 0, 0, 0
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    return min(xs), min(ys), max(xs), max(ys)
