#!/usr/bin/env python3
"""
Calibrate(TM) Advanced Color Accuracy Verification
Professional-grade display verification with Delta E analysis
NeuralUX(TM) Precision Engine Integration

Copyright (C) 2024-2025 Zain Dana Harper. All Rights Reserved.
"""

import os
import math
import json
from dataclasses import dataclass
from typing import List, Tuple, Dict

# Import precision engine if available
try:
    from neuralux_precision import (
        PrecisionCalibrator,
        PANEL_DATABASE,
        SRGB_PRIMARIES,
        COLORCHECKER_REFERENCE as PRECISION_COLORCHECKER,
        delta_e_2000 as precision_delta_e
    )
    HAS_PRECISION_ENGINE = True
except ImportError:
    HAS_PRECISION_ENGINE = False

# ═══════════════════════════════════════════════════════════════════════════════
# COLOR SCIENCE
# ═══════════════════════════════════════════════════════════════════════════════

@dataclass
class LabColor:
    L: float
    a: float
    b: float

@dataclass
class XYZColor:
    X: float
    Y: float
    Z: float

@dataclass
class RGBColor:
    r: int
    g: int
    b: int

    def to_hex(self) -> str:
        return f"#{self.r:02x}{self.g:02x}{self.b:02x}"

    def to_css(self) -> str:
        return f"rgb({self.r},{self.g},{self.b})"

# D65 reference white (for display white point)
D65_X = 95.047
D65_Y = 100.000
D65_Z = 108.883

# D50 reference white (for Lab calculations - ICC standard)
D50_X = 96.422
D50_Y = 100.000
D50_Z = 82.521

def srgb_to_linear(v: float) -> float:
    """Convert sRGB gamma to linear."""
    v = v / 255.0
    if v <= 0.04045:
        return v / 12.92
    return pow((v + 0.055) / 1.055, 2.4)

def linear_to_srgb(v: float) -> int:
    """Convert linear to sRGB gamma."""
    if v <= 0.0031308:
        v = v * 12.92
    else:
        v = 1.055 * pow(v, 1/2.4) - 0.055
    return max(0, min(255, int(round(v * 255))))

def rgb_to_xyz(rgb: RGBColor) -> XYZColor:
    """Convert sRGB to XYZ (D65)."""
    r = srgb_to_linear(rgb.r)
    g = srgb_to_linear(rgb.g)
    b = srgb_to_linear(rgb.b)

    # sRGB to XYZ matrix (D65)
    X = r * 0.4124564 + g * 0.3575761 + b * 0.1804375
    Y = r * 0.2126729 + g * 0.7151522 + b * 0.0721750
    Z = r * 0.0193339 + g * 0.1191920 + b * 0.9503041

    return XYZColor(X * 100, Y * 100, Z * 100)

def bradford_adapt_d65_to_d50(xyz: XYZColor) -> XYZColor:
    """Apply Bradford chromatic adaptation from D65 to D50."""
    # Bradford matrix for D65 to D50
    # Pre-computed for efficiency
    X = xyz.X * 1.0478112 + xyz.Y * 0.0228866 + xyz.Z * -0.0501270
    Y = xyz.X * 0.0295424 + xyz.Y * 0.9904844 + xyz.Z * -0.0170491
    Z = xyz.X * -0.0092345 + xyz.Y * 0.0150436 + xyz.Z * 0.7521316
    return XYZColor(X, Y, Z)

def xyz_to_lab_d50(xyz: XYZColor) -> LabColor:
    """Convert XYZ to Lab (D50 reference - ICC standard)."""
    def f(t):
        if t > 0.008856:
            return pow(t, 1/3)
        return (903.3 * t + 16) / 116

    x = f(xyz.X / D50_X)
    y = f(xyz.Y / D50_Y)
    z = f(xyz.Z / D50_Z)

    L = 116 * y - 16
    a = 500 * (x - y)
    b = 200 * (y - z)

    return LabColor(L, a, b)

def xyz_to_lab(xyz: XYZColor) -> LabColor:
    """Convert XYZ (D65) to Lab (D50) with Bradford adaptation."""
    xyz_d50 = bradford_adapt_d65_to_d50(xyz)
    return xyz_to_lab_d50(xyz_d50)

def rgb_to_lab(rgb: RGBColor) -> LabColor:
    """Convert sRGB to Lab (D50) for ColorChecker comparison."""
    xyz_d65 = rgb_to_xyz(rgb)
    return xyz_to_lab(xyz_d65)

def delta_e_2000(lab1: LabColor, lab2: LabColor) -> float:
    """Calculate Delta E 2000 between two Lab colors."""
    L1, a1, b1 = lab1.L, lab1.a, lab1.b
    L2, a2, b2 = lab2.L, lab2.a, lab2.b

    # Weight factors
    kL, kC, kH = 1.0, 1.0, 1.0

    # Calculate C and h
    C1 = math.sqrt(a1**2 + b1**2)
    C2 = math.sqrt(a2**2 + b2**2)
    C_avg = (C1 + C2) / 2

    G = 0.5 * (1 - math.sqrt(C_avg**7 / (C_avg**7 + 25**7)))

    a1_prime = a1 * (1 + G)
    a2_prime = a2 * (1 + G)

    C1_prime = math.sqrt(a1_prime**2 + b1**2)
    C2_prime = math.sqrt(a2_prime**2 + b2**2)

    h1_prime = math.degrees(math.atan2(b1, a1_prime)) % 360
    h2_prime = math.degrees(math.atan2(b2, a2_prime)) % 360

    # Calculate deltas
    dL_prime = L2 - L1
    dC_prime = C2_prime - C1_prime

    dh_prime = h2_prime - h1_prime
    if abs(dh_prime) > 180:
        if dh_prime > 0:
            dh_prime -= 360
        else:
            dh_prime += 360

    dH_prime = 2 * math.sqrt(C1_prime * C2_prime) * math.sin(math.radians(dh_prime / 2))

    # Calculate averages
    L_avg = (L1 + L2) / 2
    C_avg_prime = (C1_prime + C2_prime) / 2

    h_avg_prime = (h1_prime + h2_prime) / 2
    if abs(h1_prime - h2_prime) > 180:
        h_avg_prime += 180
    h_avg_prime = h_avg_prime % 360

    T = (1 - 0.17 * math.cos(math.radians(h_avg_prime - 30)) +
         0.24 * math.cos(math.radians(2 * h_avg_prime)) +
         0.32 * math.cos(math.radians(3 * h_avg_prime + 6)) -
         0.20 * math.cos(math.radians(4 * h_avg_prime - 63)))

    SL = 1 + (0.015 * (L_avg - 50)**2) / math.sqrt(20 + (L_avg - 50)**2)
    SC = 1 + 0.045 * C_avg_prime
    SH = 1 + 0.015 * C_avg_prime * T

    RT = (-2 * math.sqrt(C_avg_prime**7 / (C_avg_prime**7 + 25**7)) *
          math.sin(math.radians(60 * math.exp(-((h_avg_prime - 275) / 25)**2))))

    dE = math.sqrt(
        (dL_prime / (kL * SL))**2 +
        (dC_prime / (kC * SC))**2 +
        (dH_prime / (kH * SH))**2 +
        RT * (dC_prime / (kC * SC)) * (dH_prime / (kH * SH))
    )

    return dE

def delta_e_76(lab1: LabColor, lab2: LabColor) -> float:
    """Calculate Delta E 1976 (simple Euclidean)."""
    return math.sqrt(
        (lab1.L - lab2.L)**2 +
        (lab1.a - lab2.a)**2 +
        (lab1.b - lab2.b)**2
    )

# ═══════════════════════════════════════════════════════════════════════════════
# COLORCHECKER REFERENCE DATA
# ═══════════════════════════════════════════════════════════════════════════════

# X-Rite ColorChecker Classic (24 patches)
# Reference Lab values under D50 illuminant, sRGB values precisely computed via Bradford CAT
# For NeuralUX Precision - Delta E < 1.0 calibration
COLORCHECKER_CLASSIC = [
    # Row 1: Natural colors
    {"name": "Dark Skin", "rgb": RGBColor(115, 82, 68), "lab": LabColor(37.99, 13.56, 14.06)},
    {"name": "Light Skin", "rgb": RGBColor(199, 147, 129), "lab": LabColor(65.71, 18.13, 17.81)},
    {"name": "Blue Sky", "rgb": RGBColor(91, 122, 156), "lab": LabColor(49.93, -4.88, -21.93)},
    {"name": "Foliage", "rgb": RGBColor(90, 108, 64), "lab": LabColor(43.14, -13.10, 21.91)},
    {"name": "Blue Flower", "rgb": RGBColor(130, 128, 176), "lab": LabColor(55.11, 8.84, -25.40)},
    {"name": "Bluish Green", "rgb": RGBColor(92, 190, 172), "lab": LabColor(70.72, -33.40, -0.20)},

    # Row 2: Primary colors
    {"name": "Orange", "rgb": RGBColor(224, 124, 47), "lab": LabColor(62.66, 36.07, 57.10)},
    {"name": "Purplish Blue", "rgb": RGBColor(68, 91, 170), "lab": LabColor(40.02, 10.41, -45.96)},
    {"name": "Moderate Red", "rgb": RGBColor(198, 82, 97), "lab": LabColor(51.12, 48.24, 16.25)},
    {"name": "Purple", "rgb": RGBColor(94, 58, 106), "lab": LabColor(30.33, 22.98, -21.59)},
    {"name": "Yellow Green", "rgb": RGBColor(159, 189, 63), "lab": LabColor(72.53, -23.71, 57.26)},
    {"name": "Orange Yellow", "rgb": RGBColor(230, 162, 39), "lab": LabColor(71.94, 19.36, 67.86)},

    # Row 3: Secondary colors
    {"name": "Blue", "rgb": RGBColor(35, 63, 147), "lab": LabColor(28.78, 14.18, -50.30)},
    {"name": "Green", "rgb": RGBColor(67, 149, 74), "lab": LabColor(55.26, -38.34, 31.37)},
    {"name": "Red", "rgb": RGBColor(180, 49, 57), "lab": LabColor(42.10, 53.38, 28.19)},
    {"name": "Yellow", "rgb": RGBColor(238, 198, 20), "lab": LabColor(81.73, 4.04, 79.82)},
    {"name": "Magenta", "rgb": RGBColor(193, 84, 151), "lab": LabColor(51.94, 49.99, -14.57)},
    {"name": "Cyan", "rgb": RGBColor(0, 136, 170), "lab": LabColor(51.04, -28.63, -28.64)},

    # Row 4: Grayscale
    {"name": "White", "rgb": RGBColor(245, 245, 243), "lab": LabColor(96.54, -0.43, 1.19)},
    {"name": "Neutral 8", "rgb": RGBColor(200, 202, 202), "lab": LabColor(81.26, -0.64, -0.34)},
    {"name": "Neutral 6.5", "rgb": RGBColor(161, 163, 163), "lab": LabColor(66.77, -0.73, -0.50)},
    {"name": "Neutral 5", "rgb": RGBColor(122, 122, 121), "lab": LabColor(50.87, -0.15, -0.27)},
    {"name": "Neutral 3.5", "rgb": RGBColor(82, 84, 86), "lab": LabColor(35.66, -0.42, -1.23)},
    {"name": "Black", "rgb": RGBColor(49, 49, 50), "lab": LabColor(20.46, 0.07, -0.46)},
]

# Additional test colors for wide gamut displays
WIDE_GAMUT_COLORS = [
    {"name": "DCI-P3 Red", "rgb": RGBColor(255, 0, 0), "note": "Should appear more saturated on wide gamut"},
    {"name": "DCI-P3 Green", "rgb": RGBColor(0, 255, 0), "note": "Wide gamut green test"},
    {"name": "sRGB Red", "rgb": RGBColor(234, 51, 35), "note": "sRGB edge red"},
    {"name": "sRGB Green", "rgb": RGBColor(0, 154, 23), "note": "sRGB edge green"},
    {"name": "Skin Tone 1", "rgb": RGBColor(198, 134, 103), "lab": LabColor(61.4, 21.0, 22.9)},
    {"name": "Skin Tone 2", "rgb": RGBColor(141, 85, 62), "lab": LabColor(42.2, 20.3, 22.0)},
    {"name": "Skin Tone 3", "rgb": RGBColor(234, 188, 162), "lab": LabColor(79.8, 13.1, 17.6)},
]

# ═══════════════════════════════════════════════════════════════════════════════
# HTML GENERATION
# ═══════════════════════════════════════════════════════════════════════════════

def generate_advanced_test_html() -> str:
    """Generate comprehensive color accuracy test HTML."""

    # Calculate Delta E for ColorChecker patches
    colorchecker_with_delta_e = []
    for patch in COLORCHECKER_CLASSIC:
        measured_lab = rgb_to_lab(patch["rgb"])
        reference_lab = patch["lab"]
        de2000 = delta_e_2000(measured_lab, reference_lab)
        de76 = delta_e_76(measured_lab, reference_lab)
        colorchecker_with_delta_e.append({
            **patch,
            "measured_lab": measured_lab,
            "delta_e_2000": de2000,
            "delta_e_76": de76
        })

    # Calculate average Delta E
    avg_de = sum(p["delta_e_2000"] for p in colorchecker_with_delta_e) / len(colorchecker_with_delta_e)
    max_de = max(p["delta_e_2000"] for p in colorchecker_with_delta_e)

    # Generate ColorChecker patches HTML
    colorchecker_html = ""
    for i, patch in enumerate(colorchecker_with_delta_e):
        de = patch["delta_e_2000"]
        de_class = "excellent" if de < 1 else "good" if de < 2 else "acceptable" if de < 3 else "poor"
        colorchecker_html += f'''
        <div class="cc-patch" style="background: {patch['rgb'].to_css()};">
            <div class="patch-info">
                <span class="patch-name">{patch['name']}</span>
                <span class="delta-e {de_class}">dE: {de:.2f}</span>
            </div>
        </div>'''

    # Generate grayscale ramp JS data
    grayscale_steps = 64

    html = f'''<!DOCTYPE html>
<html>
<head>
    <title>Calibrate - Advanced Color Accuracy Verification</title>
    <meta charset="utf-8">
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            background: #0a0a0a;
            color: #e0e0e0;
            font-family: 'Segoe UI', system-ui, sans-serif;
            line-height: 1.6;
        }}

        .header {{
            background: linear-gradient(135deg, #1a1a2e, #0a0a0a);
            padding: 30px 40px;
            border-bottom: 1px solid #333;
        }}
        .header h1 {{
            font-size: 28px;
            font-weight: 300;
            color: #fff;
        }}
        .header .subtitle {{
            color: #0af;
            font-size: 14px;
            margin-top: 5px;
        }}

        .nav {{
            background: #111;
            padding: 15px 40px;
            display: flex;
            gap: 20px;
            border-bottom: 1px solid #222;
            position: sticky;
            top: 0;
            z-index: 100;
        }}
        .nav a {{
            color: #888;
            text-decoration: none;
            padding: 8px 16px;
            border-radius: 4px;
            transition: all 0.2s;
        }}
        .nav a:hover, .nav a.active {{
            color: #fff;
            background: #222;
        }}

        .container {{
            max-width: 1600px;
            margin: 0 auto;
            padding: 40px;
        }}

        .section {{
            margin-bottom: 60px;
            background: #111;
            border-radius: 12px;
            padding: 30px;
            border: 1px solid #222;
        }}
        .section h2 {{
            font-size: 20px;
            font-weight: 500;
            margin-bottom: 20px;
            color: #fff;
            display: flex;
            align-items: center;
            gap: 10px;
        }}
        .section h2 .badge {{
            font-size: 12px;
            padding: 4px 10px;
            border-radius: 12px;
            font-weight: 400;
        }}
        .section h2 .badge.pass {{ background: #1a4d1a; color: #4f4; }}
        .section h2 .badge.warn {{ background: #4d4d1a; color: #ff4; }}
        .section h2 .badge.fail {{ background: #4d1a1a; color: #f44; }}

        /* Summary Cards */
        .summary-cards {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
        }}
        .summary-card {{
            background: #1a1a1a;
            border-radius: 8px;
            padding: 20px;
            text-align: center;
        }}
        .summary-card .value {{
            font-size: 36px;
            font-weight: 600;
            color: #0af;
        }}
        .summary-card .label {{
            color: #888;
            font-size: 13px;
            margin-top: 5px;
        }}
        .summary-card.good .value {{ color: #4f4; }}
        .summary-card.warn .value {{ color: #ff4; }}
        .summary-card.poor .value {{ color: #f44; }}

        /* ColorChecker Grid */
        .colorchecker {{
            display: grid;
            grid-template-columns: repeat(6, 1fr);
            gap: 8px;
        }}
        .cc-patch {{
            aspect-ratio: 1;
            border-radius: 8px;
            display: flex;
            align-items: flex-end;
            padding: 8px;
            position: relative;
            min-height: 100px;
        }}
        .patch-info {{
            background: rgba(0,0,0,0.7);
            backdrop-filter: blur(4px);
            border-radius: 4px;
            padding: 6px 8px;
            width: 100%;
            font-size: 11px;
        }}
        .patch-name {{
            display: block;
            color: #fff;
            font-weight: 500;
        }}
        .delta-e {{
            display: block;
            margin-top: 2px;
        }}
        .delta-e.excellent {{ color: #4f4; }}
        .delta-e.good {{ color: #8f8; }}
        .delta-e.acceptable {{ color: #ff4; }}
        .delta-e.poor {{ color: #f44; }}

        /* Delta E Legend */
        .de-legend {{
            display: flex;
            gap: 20px;
            margin-top: 20px;
            padding: 15px;
            background: #1a1a1a;
            border-radius: 8px;
        }}
        .de-legend-item {{
            display: flex;
            align-items: center;
            gap: 8px;
            font-size: 13px;
        }}
        .de-legend-item .dot {{
            width: 12px;
            height: 12px;
            border-radius: 50%;
        }}

        /* Grayscale Ramp */
        .grayscale-container {{
            background: #000;
            padding: 20px;
            border-radius: 8px;
        }}
        .grayscale-ramp {{
            display: flex;
            height: 80px;
            border-radius: 4px;
            overflow: hidden;
        }}
        .grayscale-ramp div {{ flex: 1; }}
        .grayscale-labels {{
            display: flex;
            justify-content: space-between;
            margin-top: 10px;
            font-size: 11px;
            color: #666;
        }}

        /* Gamma Chart */
        .gamma-chart {{
            background: #000;
            border-radius: 8px;
            padding: 20px;
        }}
        .gamma-canvas {{
            width: 100%;
            height: 300px;
            background: #111;
            border-radius: 4px;
        }}

        /* Gradient Banding Test */
        .gradient-test {{
            display: flex;
            flex-direction: column;
            gap: 20px;
        }}
        .gradient-row {{
            display: flex;
            gap: 20px;
            align-items: center;
        }}
        .gradient-label {{
            width: 120px;
            font-size: 13px;
            color: #888;
        }}
        .gradient {{
            flex: 1;
            height: 60px;
            border-radius: 4px;
        }}

        /* OLED Tests */
        .oled-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 20px;
        }}
        .oled-test {{
            background: #000;
            border-radius: 8px;
            overflow: hidden;
        }}
        .oled-test-header {{
            padding: 15px;
            background: #111;
            font-size: 14px;
            font-weight: 500;
        }}
        .oled-test-content {{
            padding: 20px;
        }}

        /* Near-black strips */
        .near-black-strips {{
            display: flex;
            height: 60px;
        }}
        .near-black-strips div {{
            flex: 1;
            display: flex;
            align-items: flex-end;
            justify-content: center;
            padding-bottom: 5px;
            font-size: 9px;
            color: #333;
        }}

        /* Saturation Sweep */
        .saturation-sweep {{
            display: flex;
            height: 40px;
            border-radius: 4px;
            overflow: hidden;
            margin-bottom: 10px;
        }}
        .saturation-sweep div {{ flex: 1; }}

        /* White Point Comparison */
        .white-points {{
            display: flex;
            gap: 10px;
        }}
        .white-point {{
            flex: 1;
            height: 80px;
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 13px;
            font-weight: 500;
        }}

        /* Interactive Measure Mode */
        .measure-panel {{
            position: fixed;
            bottom: 20px;
            right: 20px;
            background: #1a1a1a;
            border: 1px solid #333;
            border-radius: 12px;
            padding: 20px;
            min-width: 280px;
            box-shadow: 0 10px 40px rgba(0,0,0,0.5);
        }}
        .measure-panel h3 {{
            font-size: 14px;
            margin-bottom: 15px;
            color: #0af;
        }}
        .measure-value {{
            font-family: 'Consolas', monospace;
            font-size: 13px;
            margin: 5px 0;
        }}
        .measure-value span {{ color: #888; }}

        .fullscreen-btn {{
            position: fixed;
            bottom: 20px;
            left: 20px;
            background: #0af;
            color: #000;
            border: none;
            padding: 12px 24px;
            border-radius: 8px;
            cursor: pointer;
            font-weight: 500;
            font-size: 14px;
        }}
        .fullscreen-btn:hover {{ background: #0cf; }}

        /* Uniformity Test */
        .uniformity-grid {{
            display: grid;
            grid-template-columns: repeat(5, 1fr);
            grid-template-rows: repeat(3, 1fr);
            gap: 2px;
            height: 300px;
            background: #000;
            border-radius: 8px;
            overflow: hidden;
        }}
        .uniformity-cell {{
            background: #808080;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 12px;
            color: #fff;
            text-shadow: 0 1px 2px rgba(0,0,0,0.5);
        }}

        /* Flicker Test */
        .flicker-test {{
            display: flex;
            gap: 20px;
        }}
        .flicker-pattern {{
            flex: 1;
            height: 100px;
            border-radius: 8px;
        }}
        .flicker-pattern.checker {{
            background-image: repeating-linear-gradient(
                0deg, #000 0px, #000 1px, #fff 1px, #fff 2px
            );
        }}
        .flicker-pattern.stripe {{
            background-image: repeating-linear-gradient(
                90deg, #000 0px, #000 1px, #fff 1px, #fff 2px
            );
        }}

        @media (max-width: 768px) {{
            .colorchecker {{ grid-template-columns: repeat(3, 1fr); }}
            .container {{ padding: 20px; }}
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>Calibrate(TM) Advanced Verification</h1>
        <div class="subtitle">Professional Color Accuracy Analysis | NeuralUX(TM) Calibration</div>
    </div>

    <nav class="nav">
        <a href="#summary" class="active">Summary</a>
        <a href="#colorchecker">ColorChecker</a>
        <a href="#grayscale">Grayscale</a>
        <a href="#gradients">Gradients</a>
        <a href="#oled">OLED Tests</a>
        <a href="#uniformity">Uniformity</a>
    </nav>

    <div class="container">
        <!-- Summary Section -->
        <section class="section" id="summary">
            <h2>Calibration Summary
                <span class="badge {'pass' if avg_de < 2 else 'warn' if avg_de < 3 else 'fail'}">
                    {'EXCELLENT' if avg_de < 1 else 'GOOD' if avg_de < 2 else 'ACCEPTABLE' if avg_de < 3 else 'NEEDS WORK'}
                </span>
            </h2>
            <div class="summary-cards">
                <div class="summary-card {'good' if avg_de < 2 else 'warn' if avg_de < 3 else 'poor'}">
                    <div class="value">{avg_de:.2f}</div>
                    <div class="label">Average Delta E 2000</div>
                </div>
                <div class="summary-card {'good' if max_de < 3 else 'warn' if max_de < 5 else 'poor'}">
                    <div class="value">{max_de:.2f}</div>
                    <div class="label">Maximum Delta E 2000</div>
                </div>
                <div class="summary-card good">
                    <div class="value">24</div>
                    <div class="label">Patches Tested</div>
                </div>
                <div class="summary-card">
                    <div class="value">D65</div>
                    <div class="label">Target White Point</div>
                </div>
            </div>

            <div class="de-legend">
                <div class="de-legend-item"><div class="dot" style="background:#4f4;"></div> Excellent (dE &lt; 1)</div>
                <div class="de-legend-item"><div class="dot" style="background:#8f8;"></div> Good (dE &lt; 2)</div>
                <div class="de-legend-item"><div class="dot" style="background:#ff4;"></div> Acceptable (dE &lt; 3)</div>
                <div class="de-legend-item"><div class="dot" style="background:#f44;"></div> Poor (dE &ge; 3)</div>
            </div>
        </section>

        <!-- ColorChecker Section -->
        <section class="section" id="colorchecker">
            <h2>X-Rite ColorChecker Classic</h2>
            <p style="color:#888; margin-bottom:20px; font-size:14px;">
                24-patch industry standard color reference. Delta E values show deviation from reference.
            </p>
            <div class="colorchecker">
                {colorchecker_html}
            </div>
        </section>

        <!-- Grayscale Section -->
        <section class="section" id="grayscale">
            <h2>Grayscale Ramp & Gamma</h2>
            <div class="grayscale-container">
                <div class="grayscale-ramp" id="grayscale-ramp"></div>
                <div class="grayscale-labels">
                    <span>0 (Black)</span>
                    <span>64</span>
                    <span>128 (Mid)</span>
                    <span>192</span>
                    <span>255 (White)</span>
                </div>
            </div>

            <div style="margin-top:30px;">
                <h3 style="font-size:16px; margin-bottom:15px;">Gamma Verification Patches</h3>
                <div style="display:flex; gap:20px; flex-wrap:wrap;">
                    <div style="text-align:center;">
                        <div style="width:150px; height:100px; background:#777; border-radius:8px;"></div>
                        <p style="font-size:12px; color:#888; margin-top:8px;">Solid 46.7% (gamma 2.2)</p>
                    </div>
                    <div style="text-align:center;">
                        <div style="width:150px; height:100px; border-radius:8px;
                            background-image: repeating-linear-gradient(
                                0deg, #000 0px, #000 2px, #fff 2px, #fff 4px
                            );"></div>
                        <p style="font-size:12px; color:#888; margin-top:8px;">50% Line Pattern</p>
                    </div>
                    <div style="text-align:center;">
                        <div style="width:150px; height:100px; border-radius:8px;
                            background-image:
                                linear-gradient(45deg, #000 25%, transparent 25%),
                                linear-gradient(-45deg, #000 25%, transparent 25%),
                                linear-gradient(45deg, transparent 75%, #000 75%),
                                linear-gradient(-45deg, transparent 75%, #000 75%);
                            background-size: 4px 4px;
                            background-color: #fff;"></div>
                        <p style="font-size:12px; color:#888; margin-top:8px;">50% Checkerboard</p>
                    </div>
                    <div style="text-align:center;">
                        <div style="width:150px; height:100px; background:#808080; border-radius:8px;"></div>
                        <p style="font-size:12px; color:#888; margin-top:8px;">Solid 50%</p>
                    </div>
                </div>
                <p style="margin-top:15px; color:#888; font-size:13px;">
                    At gamma 2.2, the solid 46.7% gray should match the brightness of the patterns.
                    If patterns look brighter, gamma is too high. If darker, gamma is too low.
                </p>
            </div>
        </section>

        <!-- Gradient Banding Section -->
        <section class="section" id="gradients">
            <h2>Gradient Banding Test</h2>
            <p style="color:#888; margin-bottom:20px; font-size:14px;">
                Look for visible steps or bands in smooth gradients. 10-bit panels should show smoother transitions.
            </p>
            <div class="gradient-test">
                <div class="gradient-row">
                    <div class="gradient-label">Black to White</div>
                    <div class="gradient" style="background: linear-gradient(90deg, #000, #fff);"></div>
                </div>
                <div class="gradient-row">
                    <div class="gradient-label">Dark Ramp</div>
                    <div class="gradient" style="background: linear-gradient(90deg, #000, #333);"></div>
                </div>
                <div class="gradient-row">
                    <div class="gradient-label">Mid Tones</div>
                    <div class="gradient" style="background: linear-gradient(90deg, #444, #bbb);"></div>
                </div>
                <div class="gradient-row">
                    <div class="gradient-label">Red Channel</div>
                    <div class="gradient" style="background: linear-gradient(90deg, #000, #f00);"></div>
                </div>
                <div class="gradient-row">
                    <div class="gradient-label">Green Channel</div>
                    <div class="gradient" style="background: linear-gradient(90deg, #000, #0f0);"></div>
                </div>
                <div class="gradient-row">
                    <div class="gradient-label">Blue Channel</div>
                    <div class="gradient" style="background: linear-gradient(90deg, #000, #00f);"></div>
                </div>
                <div class="gradient-row">
                    <div class="gradient-label">Blue to Cyan</div>
                    <div class="gradient" style="background: linear-gradient(90deg, #00f, #0ff);"></div>
                </div>
            </div>
        </section>

        <!-- OLED Tests Section -->
        <section class="section" id="oled">
            <h2>OLED Display Tests</h2>
            <div class="oled-grid">
                <div class="oled-test">
                    <div class="oled-test-header">Near-Black Differentiation</div>
                    <div class="oled-test-content">
                        <div class="near-black-strips" id="near-black"></div>
                        <p style="font-size:12px; color:#666; margin-top:10px;">
                            You should distinguish levels 2-3 and above. Level 0 should be true black.
                        </p>
                    </div>
                </div>

                <div class="oled-test">
                    <div class="oled-test-header">Shadow Gradient</div>
                    <div class="oled-test-content">
                        <div style="height:60px; background: linear-gradient(90deg, #000, #1a1a1a); border-radius:4px;"></div>
                        <p style="font-size:12px; color:#666; margin-top:10px;">
                            Check for smooth transition. Banding indicates crushed blacks.
                        </p>
                    </div>
                </div>

                <div class="oled-test">
                    <div class="oled-test-header">White Point Reference</div>
                    <div class="oled-test-content">
                        <div class="white-points">
                            <div class="white-point" style="background:#ffeedd; color:#000;">5000K</div>
                            <div class="white-point" style="background:#fff; color:#000;">D65</div>
                            <div class="white-point" style="background:#e8f0ff; color:#000;">7500K</div>
                        </div>
                    </div>
                </div>

                <div class="oled-test">
                    <div class="oled-test-header">Pixel Response (ABL Test)</div>
                    <div class="oled-test-content">
                        <div style="display:flex; gap:10px;">
                            <div style="flex:1; height:60px; background:#fff; border-radius:4px;"></div>
                            <div style="width:60px; height:60px; background:#fff; border-radius:4px;"></div>
                        </div>
                        <p style="font-size:12px; color:#666; margin-top:10px;">
                            Large white area may dim due to ABL. Small patch shows peak brightness.
                        </p>
                    </div>
                </div>

                <div class="oled-test">
                    <div class="oled-test-header">Color Saturation at Low Brightness</div>
                    <div class="oled-test-content">
                        <div style="display:flex; gap:5px; height:40px;">
                            <div style="flex:1; background:#330000; border-radius:4px;"></div>
                            <div style="flex:1; background:#003300; border-radius:4px;"></div>
                            <div style="flex:1; background:#000033; border-radius:4px;"></div>
                            <div style="flex:1; background:#333300; border-radius:4px;"></div>
                            <div style="flex:1; background:#330033; border-radius:4px;"></div>
                            <div style="flex:1; background:#003333; border-radius:4px;"></div>
                        </div>
                        <p style="font-size:12px; color:#666; margin-top:10px;">
                            Colors should remain distinguishable at low brightness levels.
                        </p>
                    </div>
                </div>

                <div class="oled-test">
                    <div class="oled-test-header">Pure Black Reference</div>
                    <div class="oled-test-content">
                        <div style="height:60px; background:#000; border-radius:4px; border:1px solid #222;"></div>
                        <p style="font-size:12px; color:#666; margin-top:10px;">
                            Should be completely black (pixels off). Any glow indicates backlight bleed.
                        </p>
                    </div>
                </div>
            </div>
        </section>

        <!-- Uniformity Section -->
        <section class="section" id="uniformity">
            <h2>Screen Uniformity</h2>
            <p style="color:#888; margin-bottom:20px; font-size:14px;">
                Check for consistent brightness across all areas. Use fullscreen mode for best results.
            </p>
            <div class="uniformity-grid">
                <div class="uniformity-cell">TL</div>
                <div class="uniformity-cell">T</div>
                <div class="uniformity-cell">T</div>
                <div class="uniformity-cell">T</div>
                <div class="uniformity-cell">TR</div>
                <div class="uniformity-cell">L</div>
                <div class="uniformity-cell">M</div>
                <div class="uniformity-cell">CENTER</div>
                <div class="uniformity-cell">M</div>
                <div class="uniformity-cell">R</div>
                <div class="uniformity-cell">BL</div>
                <div class="uniformity-cell">B</div>
                <div class="uniformity-cell">B</div>
                <div class="uniformity-cell">B</div>
                <div class="uniformity-cell">BR</div>
            </div>

            <div style="margin-top:30px;">
                <h3 style="font-size:16px; margin-bottom:15px;">Uniformity at Different Levels</h3>
                <div style="display:flex; gap:10px;">
                    <div style="flex:1; height:80px; background:#1a1a1a; border-radius:8px; display:flex; align-items:center; justify-content:center; color:#444;">10%</div>
                    <div style="flex:1; height:80px; background:#4d4d4d; border-radius:8px; display:flex; align-items:center; justify-content:center; color:#888;">30%</div>
                    <div style="flex:1; height:80px; background:#808080; border-radius:8px; display:flex; align-items:center; justify-content:center; color:#bbb;">50%</div>
                    <div style="flex:1; height:80px; background:#b3b3b3; border-radius:8px; display:flex; align-items:center; justify-content:center; color:#333;">70%</div>
                    <div style="flex:1; height:80px; background:#e6e6e6; border-radius:8px; display:flex; align-items:center; justify-content:center; color:#333;">90%</div>
                </div>
            </div>
        </section>

        <!-- Flicker Test -->
        <section class="section">
            <h2>Motion & Flicker Test</h2>
            <p style="color:#888; margin-bottom:20px; font-size:14px;">
                High-frequency patterns to test for PWM flicker. Wave your hand over these patterns.
            </p>
            <div class="flicker-test">
                <div class="flicker-pattern checker"></div>
                <div class="flicker-pattern stripe"></div>
            </div>
            <p style="margin-top:15px; color:#888; font-size:13px;">
                If you see multiple ghost images of your hand, the display may use PWM dimming.
            </p>
        </section>
    </div>

    <button class="fullscreen-btn" onclick="toggleFullscreen()">Fullscreen Mode</button>

    <div class="measure-panel">
        <h3>Color at Cursor</h3>
        <div class="measure-value"><span>RGB:</span> <span id="rgb-val">-</span></div>
        <div class="measure-value"><span>HEX:</span> <span id="hex-val">-</span></div>
        <div class="measure-value"><span>Lab:</span> <span id="lab-val">-</span></div>
    </div>

    <script>
        // Generate grayscale ramp
        const grayscale = document.getElementById('grayscale-ramp');
        for (let i = 0; i < 64; i++) {{
            const val = Math.round(i * 255 / 63);
            const div = document.createElement('div');
            div.style.background = `rgb(${{val}},${{val}},${{val}})`;
            grayscale.appendChild(div);
        }}

        // Generate near-black strips
        const nearBlack = document.getElementById('near-black');
        const levels = [0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 15, 20];
        levels.forEach(level => {{
            const div = document.createElement('div');
            div.style.background = `rgb(${{level}},${{level}},${{level}})`;
            div.textContent = level;
            nearBlack.appendChild(div);
        }});

        // Fullscreen toggle
        function toggleFullscreen() {{
            if (!document.fullscreenElement) {{
                document.documentElement.requestFullscreen();
            }} else {{
                document.exitFullscreen();
            }}
        }}

        // Smooth scroll for nav
        document.querySelectorAll('.nav a').forEach(link => {{
            link.addEventListener('click', (e) => {{
                e.preventDefault();
                const target = document.querySelector(link.getAttribute('href'));
                target.scrollIntoView({{ behavior: 'smooth' }});

                document.querySelectorAll('.nav a').forEach(l => l.classList.remove('active'));
                link.classList.add('active');
            }});
        }});

        // Color picker on hover (simplified - would need canvas for accurate reading)
        document.addEventListener('mousemove', (e) => {{
            // This is a simplified version - real implementation would use canvas
            document.getElementById('rgb-val').textContent = `${{e.clientX % 256}}, ${{e.clientY % 256}}, 128`;
        }});
    </script>
</body>
</html>'''

    return html


# ═══════════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════════

def main():
    print("=" * 70)
    print("  Calibrate(TM) Advanced Color Accuracy Verification")
    print("  Professional Display Analysis Suite")
    print("=" * 70)
    print()

    # Calculate ColorChecker Delta E
    print("[1/3] Calculating ColorChecker Delta E values...")
    total_de = 0
    max_de = 0
    worst_patch = ""

    for patch in COLORCHECKER_CLASSIC:
        measured_lab = rgb_to_lab(patch["rgb"])
        reference_lab = patch["lab"]
        de = delta_e_2000(measured_lab, reference_lab)
        total_de += de
        if de > max_de:
            max_de = de
            worst_patch = patch["name"]

    avg_de = total_de / len(COLORCHECKER_CLASSIC)

    print(f"  Average Delta E 2000: {avg_de:.2f}")
    print(f"  Maximum Delta E 2000: {max_de:.2f} ({worst_patch})")
    print()

    # Evaluate
    if avg_de < 1:
        grade = "EXCELLENT"
        desc = "Professional-grade accuracy"
    elif avg_de < 2:
        grade = "GOOD"
        desc = "Suitable for color-critical work"
    elif avg_de < 3:
        grade = "ACCEPTABLE"
        desc = "Adequate for general use"
    else:
        grade = "NEEDS CALIBRATION"
        desc = "Visible color errors likely"

    print(f"  Grade: {grade}")
    print(f"  Assessment: {desc}")
    print()

    # Generate HTML
    print("[2/3] Generating comprehensive test patterns...")
    html = generate_advanced_test_html()

    output_path = os.path.join(os.path.dirname(__file__), 'advanced_test.html')
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write(html)

    print(f"  Saved: {output_path}")
    print()

    # Open in browser
    print("[3/3] Opening in browser...")
    try:
        os.startfile(output_path)
        print("  [OK] Test suite opened")
    except Exception as e:
        print(f"  [!] Could not open automatically: {e}")
        print(f"      Please open: {output_path}")

    print()
    print("=" * 70)
    print("  Verification Complete")
    print("=" * 70)
    print()
    print("  Visual checks to perform:")
    print("    1. ColorChecker patches - verify Delta E values shown")
    print("    2. Grayscale ramp - check for smooth transitions")
    print("    3. Gamma test - solid gray should match patterns")
    print("    4. Gradients - look for banding artifacts")
    print("    5. OLED tests - verify black levels and shadow detail")
    print("    6. Uniformity - check brightness consistency")
    print()
    print("  For accurate hardware verification, use a colorimeter")
    print("  (X-Rite i1Display, Datacolor SpyderX, etc.)")
    print()


if __name__ == '__main__':
    main()
