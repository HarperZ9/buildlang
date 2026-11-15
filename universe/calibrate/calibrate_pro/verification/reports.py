"""
Verification Report Generation Module

Provides comprehensive report generation for calibration verification:
- PDF reports using ReportLab
- HTML interactive reports
- JSON data export
- Before/after comparison
- Charts and visualizations
"""

from dataclasses import dataclass, field, asdict
from enum import Enum, auto
from pathlib import Path
from typing import Optional, Any
from datetime import datetime
import json
import io
import base64

# Optional imports for PDF generation
try:
    from reportlab.lib import colors
    from reportlab.lib.pagesizes import letter, A4
    from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
    from reportlab.lib.units import inch, mm
    from reportlab.platypus import (
        SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle,
        Image, PageBreak, KeepTogether, ListFlowable, ListItem
    )
    from reportlab.graphics.shapes import Drawing, Rect, String, Line, Circle
    from reportlab.graphics.charts.barcharts import VerticalBarChart
    from reportlab.graphics.charts.linecharts import HorizontalLineChart
    from reportlab.graphics import renderPDF
    REPORTLAB_AVAILABLE = True
except ImportError:
    REPORTLAB_AVAILABLE = False

# Import verification modules
from calibrate_pro.verification.colorchecker import (
    ColorCheckerResult, PatchMeasurement, VerificationGrade,
    grade_to_string as cc_grade_to_string,
)
from calibrate_pro.verification.grayscale import (
    GrayscaleResult, GrayscalePatch, GrayscaleGrade,
    grade_to_string as gs_grade_to_string,
)
from calibrate_pro.verification.gamut_volume import (
    GamutAnalysisResult, GamutCoverage, GamutGrade,
    grade_to_string as gv_grade_to_string,
)


# =============================================================================
# Enums
# =============================================================================

class ReportFormat(Enum):
    """Report output format."""
    PDF = auto()
    HTML = auto()
    JSON = auto()


class ReportType(Enum):
    """Type of verification report."""
    COLORCHECKER = auto()
    GRAYSCALE = auto()
    GAMUT = auto()
    COMPREHENSIVE = auto()  # All verification types


# =============================================================================
# Data Classes
# =============================================================================

@dataclass
class ReportMetadata:
    """Report metadata."""
    title: str
    display_name: str
    profile_name: str
    operator: str = ""
    organization: str = ""
    notes: str = ""
    timestamp: str = field(default_factory=lambda: datetime.now().isoformat())
    software_version: str = "Calibrate Pro 1.0"


@dataclass
class ReportConfig:
    """Report generation configuration."""
    format: ReportFormat = ReportFormat.PDF
    page_size: str = "letter"  # letter, A4
    include_charts: bool = True
    include_detailed_data: bool = True
    include_recommendations: bool = True
    color_theme: str = "professional"  # professional, dark, light
    logo_path: Optional[str] = None
    output_path: Optional[str] = None


@dataclass
class VerificationSummary:
    """Summary of all verification results."""
    colorchecker: Optional[ColorCheckerResult] = None
    grayscale: Optional[GrayscaleResult] = None
    gamut: Optional[GamutAnalysisResult] = None

    overall_pass: bool = True
    overall_grade: str = "Unknown"
    recommendations: list[str] = field(default_factory=list)


# =============================================================================
# Color Definitions
# =============================================================================

REPORT_COLORS = {
    "professional": {
        "primary": "#1a365d",
        "secondary": "#2c5282",
        "accent": "#3182ce",
        "success": "#38a169",
        "warning": "#d69e2e",
        "error": "#e53e3e",
        "text": "#2d3748",
        "text_light": "#718096",
        "background": "#ffffff",
        "border": "#e2e8f0",
    },
    "dark": {
        "primary": "#1a202c",
        "secondary": "#2d3748",
        "accent": "#4299e1",
        "success": "#48bb78",
        "warning": "#ecc94b",
        "error": "#fc8181",
        "text": "#e2e8f0",
        "text_light": "#a0aec0",
        "background": "#1a202c",
        "border": "#4a5568",
    },
}


# =============================================================================
# Report Generator Class
# =============================================================================

class ReportGenerator:
    """
    Professional calibration report generator.

    Generates comprehensive verification reports in PDF, HTML, or JSON format.
    """

    def __init__(self, config: Optional[ReportConfig] = None):
        """
        Initialize report generator.

        Args:
            config: Report configuration options
        """
        self.config = config or ReportConfig()
        self.colors = REPORT_COLORS.get(self.config.color_theme, REPORT_COLORS["professional"])

    def generate(self,
                 summary: VerificationSummary,
                 metadata: ReportMetadata,
                 output_path: Optional[str] = None) -> str:
        """
        Generate verification report.

        Args:
            summary: Verification results summary
            metadata: Report metadata
            output_path: Output file path (auto-generated if None)

        Returns:
            Path to generated report
        """
        if output_path:
            self.config.output_path = output_path

        if self.config.format == ReportFormat.PDF:
            return self._generate_pdf(summary, metadata)
        elif self.config.format == ReportFormat.HTML:
            return self._generate_html(summary, metadata)
        elif self.config.format == ReportFormat.JSON:
            return self._generate_json(summary, metadata)
        else:
            raise ValueError(f"Unsupported format: {self.config.format}")

    # =========================================================================
    # PDF Generation
    # =========================================================================

    def _generate_pdf(self, summary: VerificationSummary, metadata: ReportMetadata) -> str:
        """Generate PDF report using ReportLab."""
        if not REPORTLAB_AVAILABLE:
            raise ImportError("ReportLab is required for PDF generation. "
                            "Install with: pip install reportlab")

        # Determine output path
        if self.config.output_path:
            output_path = Path(self.config.output_path)
        else:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            output_path = Path(f"calibration_report_{timestamp}.pdf")

        # Page size
        page_size = A4 if self.config.page_size.lower() == "a4" else letter

        # Create document
        doc = SimpleDocTemplate(
            str(output_path),
            pagesize=page_size,
            rightMargin=0.75*inch,
            leftMargin=0.75*inch,
            topMargin=0.75*inch,
            bottomMargin=0.75*inch,
        )

        # Build story
        story = []
        styles = getSampleStyleSheet()

        # Custom styles
        title_style = ParagraphStyle(
            'CustomTitle',
            parent=styles['Heading1'],
            fontSize=24,
            spaceAfter=30,
            textColor=colors.HexColor(self.colors["primary"]),
        )

        heading_style = ParagraphStyle(
            'CustomHeading',
            parent=styles['Heading2'],
            fontSize=16,
            spaceBefore=20,
            spaceAfter=10,
            textColor=colors.HexColor(self.colors["secondary"]),
        )

        subheading_style = ParagraphStyle(
            'CustomSubheading',
            parent=styles['Heading3'],
            fontSize=12,
            spaceBefore=15,
            spaceAfter=8,
            textColor=colors.HexColor(self.colors["text"]),
        )

        body_style = ParagraphStyle(
            'CustomBody',
            parent=styles['Normal'],
            fontSize=10,
            spaceAfter=6,
            textColor=colors.HexColor(self.colors["text"]),
        )

        # Title
        story.append(Paragraph(metadata.title, title_style))
        story.append(Spacer(1, 12))

        # Metadata table
        meta_data = [
            ["Display:", metadata.display_name],
            ["Profile:", metadata.profile_name],
            ["Date:", datetime.now().strftime("%Y-%m-%d %H:%M")],
            ["Software:", metadata.software_version],
        ]
        if metadata.operator:
            meta_data.append(["Operator:", metadata.operator])
        if metadata.organization:
            meta_data.append(["Organization:", metadata.organization])

        meta_table = Table(meta_data, colWidths=[1.5*inch, 4*inch])
        meta_table.setStyle(TableStyle([
            ('FONTNAME', (0, 0), (0, -1), 'Helvetica-Bold'),
            ('FONTSIZE', (0, 0), (-1, -1), 10),
            ('TEXTCOLOR', (0, 0), (-1, -1), colors.HexColor(self.colors["text"])),
            ('BOTTOMPADDING', (0, 0), (-1, -1), 6),
        ]))
        story.append(meta_table)
        story.append(Spacer(1, 20))

        # Overall Summary
        story.append(Paragraph("Verification Summary", heading_style))

        summary_text = self._get_overall_summary_text(summary)
        story.append(Paragraph(summary_text, body_style))
        story.append(Spacer(1, 20))

        # ColorChecker Results
        if summary.colorchecker:
            story.append(Paragraph("ColorChecker Verification", heading_style))
            story.extend(self._create_colorchecker_section(summary.colorchecker, styles, body_style))
            story.append(Spacer(1, 15))

        # Grayscale Results
        if summary.grayscale:
            story.append(Paragraph("Grayscale Verification", heading_style))
            story.extend(self._create_grayscale_section(summary.grayscale, styles, body_style))
            story.append(Spacer(1, 15))

        # Gamut Results
        if summary.gamut:
            story.append(Paragraph("Gamut Analysis", heading_style))
            story.extend(self._create_gamut_section(summary.gamut, styles, body_style))
            story.append(Spacer(1, 15))

        # Recommendations
        if self.config.include_recommendations and summary.recommendations:
            story.append(PageBreak())
            story.append(Paragraph("Recommendations", heading_style))
            for rec in summary.recommendations:
                story.append(Paragraph(f"• {rec}", body_style))
            story.append(Spacer(1, 15))

        # Notes
        if metadata.notes:
            story.append(Paragraph("Notes", heading_style))
            story.append(Paragraph(metadata.notes, body_style))

        # Build PDF
        doc.build(story)

        return str(output_path)

    def _get_overall_summary_text(self, summary: VerificationSummary) -> str:
        """Generate overall summary text."""
        lines = []

        if summary.overall_pass:
            lines.append("<b>Status: PASSED</b>")
        else:
            lines.append("<b>Status: NEEDS ATTENTION</b>")

        lines.append(f"<br/>Overall Grade: {summary.overall_grade}")

        if summary.colorchecker:
            grade = cc_grade_to_string(summary.colorchecker.overall_grade)
            lines.append(f"<br/>ColorChecker: {grade} (Mean ΔE = {summary.colorchecker.delta_e_mean:.2f})")

        if summary.grayscale:
            grade = gs_grade_to_string(summary.grayscale.overall_grade)
            lines.append(f"<br/>Grayscale: {grade} (Mean ΔE = {summary.grayscale.delta_e_mean:.2f})")

        if summary.gamut:
            lines.append(f"<br/>sRGB Coverage: {summary.gamut.srgb_coverage.coverage_percent:.1f}%")
            lines.append(f"<br/>DCI-P3 Coverage: {summary.gamut.p3_coverage.coverage_percent:.1f}%")

        return "".join(lines)

    def _create_colorchecker_section(self, result: ColorCheckerResult,
                                     styles, body_style) -> list:
        """Create ColorChecker section for PDF."""
        elements = []

        # Summary stats
        stats_text = (
            f"<b>Grade:</b> {cc_grade_to_string(result.overall_grade)}<br/>"
            f"<b>Mean ΔE:</b> {result.delta_e_mean:.2f}<br/>"
            f"<b>Max ΔE:</b> {result.delta_e_max:.2f}<br/>"
            f"<b>StdDev:</b> {result.delta_e_std:.2f}<br/>"
            f"<b>95th Percentile:</b> {result.delta_e_95th:.2f}"
        )
        elements.append(Paragraph(stats_text, body_style))
        elements.append(Spacer(1, 10))

        # Patch table (if detailed data enabled)
        if self.config.include_detailed_data:
            table_data = [["Patch", "Reference L*a*b*", "Measured L*a*b*", "ΔE00"]]

            for patch in result.patch_measurements[:12]:  # First 12 patches
                ref = f"({patch.reference_l:.1f}, {patch.reference_a:.1f}, {patch.reference_b:.1f})"
                meas = f"({patch.measured_l:.1f}, {patch.measured_a:.1f}, {patch.measured_b:.1f})"
                table_data.append([
                    patch.patch_name,
                    ref,
                    meas,
                    f"{patch.delta_e_2000:.2f}"
                ])

            table = Table(table_data, colWidths=[1.5*inch, 1.8*inch, 1.8*inch, 0.8*inch])
            table.setStyle(TableStyle([
                ('BACKGROUND', (0, 0), (-1, 0), colors.HexColor(self.colors["primary"])),
                ('TEXTCOLOR', (0, 0), (-1, 0), colors.white),
                ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
                ('FONTSIZE', (0, 0), (-1, -1), 8),
                ('ALIGN', (0, 0), (-1, -1), 'CENTER'),
                ('GRID', (0, 0), (-1, -1), 0.5, colors.HexColor(self.colors["border"])),
                ('BOTTOMPADDING', (0, 0), (-1, -1), 4),
                ('TOPPADDING', (0, 0), (-1, -1), 4),
            ]))
            elements.append(table)

        return elements

    def _create_grayscale_section(self, result: GrayscaleResult,
                                  styles, body_style) -> list:
        """Create Grayscale section for PDF."""
        elements = []

        # Summary stats
        stats_text = (
            f"<b>Grade:</b> {gs_grade_to_string(result.overall_grade)}<br/>"
            f"<b>Mean ΔE:</b> {result.delta_e_mean:.2f}<br/>"
            f"<b>Max ΔE:</b> {result.delta_e_max:.2f}<br/>"
            f"<b>Mean Gamma:</b> {result.gamma_mean:.2f}<br/>"
            f"<b>CCT:</b> {result.cct_mean:.0f}K (Target: {result.target_whitepoint})<br/>"
            f"<b>Contrast Ratio:</b> {result.contrast_ratio:.0f}:1"
        )
        elements.append(Paragraph(stats_text, body_style))
        elements.append(Spacer(1, 10))

        # Region analysis
        if result.region_analysis:
            region_text = "<b>Region Analysis:</b><br/>"
            for name, analysis in result.region_analysis.items():
                region_text += f"• {name.title()}: ΔE={analysis.delta_e_mean:.2f}, Gamma={analysis.gamma_mean:.2f}<br/>"
            elements.append(Paragraph(region_text, body_style))

        return elements

    def _create_gamut_section(self, result: GamutAnalysisResult,
                              styles, body_style) -> list:
        """Create Gamut section for PDF."""
        elements = []

        # Coverage table
        table_data = [
            ["Color Space", "Coverage", "Grade"],
            ["sRGB", f"{result.srgb_coverage.coverage_percent:.1f}%",
             gv_grade_to_string(result.srgb_coverage.grade)],
            ["DCI-P3", f"{result.p3_coverage.coverage_percent:.1f}%",
             gv_grade_to_string(result.p3_coverage.grade)],
            ["BT.2020", f"{result.bt2020_coverage.coverage_percent:.1f}%",
             gv_grade_to_string(result.bt2020_coverage.grade)],
            ["Adobe RGB", f"{result.adobe_rgb_coverage.coverage_percent:.1f}%",
             gv_grade_to_string(result.adobe_rgb_coverage.grade)],
        ]

        table = Table(table_data, colWidths=[1.5*inch, 1.2*inch, 2*inch])
        table.setStyle(TableStyle([
            ('BACKGROUND', (0, 0), (-1, 0), colors.HexColor(self.colors["primary"])),
            ('TEXTCOLOR', (0, 0), (-1, 0), colors.white),
            ('FONTNAME', (0, 0), (-1, 0), 'Helvetica-Bold'),
            ('FONTSIZE', (0, 0), (-1, -1), 10),
            ('ALIGN', (0, 0), (-1, -1), 'CENTER'),
            ('GRID', (0, 0), (-1, -1), 0.5, colors.HexColor(self.colors["border"])),
            ('BOTTOMPADDING', (0, 0), (-1, -1), 6),
            ('TOPPADDING', (0, 0), (-1, -1), 6),
        ]))
        elements.append(table)
        elements.append(Spacer(1, 10))

        # White point info
        wp_text = (
            f"<b>White Point:</b> ({result.white_point_xy[0]:.4f}, {result.white_point_xy[1]:.4f})<br/>"
            f"<b>CCT:</b> {result.white_point_cct:.0f}K<br/>"
            f"<b>Duv:</b> {result.white_point_duv:.4f}"
        )
        elements.append(Paragraph(wp_text, body_style))

        return elements

    # =========================================================================
    # HTML Generation
    # =========================================================================

    def _generate_html(self, summary: VerificationSummary, metadata: ReportMetadata) -> str:
        """Generate HTML report."""
        if self.config.output_path:
            output_path = Path(self.config.output_path)
        else:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            output_path = Path(f"calibration_report_{timestamp}.html")

        html = self._build_html_content(summary, metadata)

        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(html)

        return str(output_path)

    def _build_html_content(self, summary: VerificationSummary, metadata: ReportMetadata) -> str:
        """Build complete HTML content."""
        colors = self.colors

        html = f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{metadata.title}</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: {colors["text"]};
            background-color: {colors["background"]};
            padding: 40px;
            max-width: 1200px;
            margin: 0 auto;
        }}
        h1 {{
            color: {colors["primary"]};
            font-size: 28px;
            margin-bottom: 20px;
            padding-bottom: 10px;
            border-bottom: 3px solid {colors["accent"]};
        }}
        h2 {{
            color: {colors["secondary"]};
            font-size: 20px;
            margin-top: 30px;
            margin-bottom: 15px;
        }}
        h3 {{
            color: {colors["text"]};
            font-size: 16px;
            margin-top: 20px;
            margin-bottom: 10px;
        }}
        .metadata {{
            background: linear-gradient(135deg, {colors["primary"]}10, {colors["secondary"]}10);
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 30px;
        }}
        .metadata-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
            gap: 10px;
        }}
        .metadata-item {{
            display: flex;
        }}
        .metadata-label {{
            font-weight: 600;
            color: {colors["text_light"]};
            min-width: 100px;
        }}
        .summary-card {{
            background: white;
            border: 1px solid {colors["border"]};
            border-radius: 8px;
            padding: 20px;
            margin-bottom: 20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .grade-badge {{
            display: inline-block;
            padding: 4px 12px;
            border-radius: 20px;
            font-weight: 600;
            font-size: 12px;
        }}
        .grade-reference {{ background: {colors["success"]}20; color: {colors["success"]}; }}
        .grade-excellent {{ background: {colors["success"]}20; color: {colors["success"]}; }}
        .grade-good {{ background: {colors["accent"]}20; color: {colors["accent"]}; }}
        .grade-acceptable {{ background: {colors["warning"]}20; color: {colors["warning"]}; }}
        .grade-poor {{ background: {colors["error"]}20; color: {colors["error"]}; }}
        .status-pass {{ color: {colors["success"]}; font-weight: 700; }}
        .status-fail {{ color: {colors["error"]}; font-weight: 700; }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 15px 0;
            font-size: 14px;
        }}
        th {{
            background: {colors["primary"]};
            color: white;
            padding: 12px;
            text-align: left;
            font-weight: 600;
        }}
        td {{
            padding: 10px 12px;
            border-bottom: 1px solid {colors["border"]};
        }}
        tr:hover {{
            background: {colors["primary"]}05;
        }}
        .stat-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
            gap: 15px;
            margin: 15px 0;
        }}
        .stat-item {{
            background: {colors["background"]};
            border: 1px solid {colors["border"]};
            border-radius: 6px;
            padding: 15px;
            text-align: center;
        }}
        .stat-value {{
            font-size: 24px;
            font-weight: 700;
            color: {colors["primary"]};
        }}
        .stat-label {{
            font-size: 12px;
            color: {colors["text_light"]};
            margin-top: 5px;
        }}
        .recommendations {{
            background: {colors["warning"]}10;
            border-left: 4px solid {colors["warning"]};
            padding: 15px 20px;
            margin: 20px 0;
            border-radius: 0 8px 8px 0;
        }}
        .recommendations ul {{
            margin-left: 20px;
            margin-top: 10px;
        }}
        .footer {{
            margin-top: 40px;
            padding-top: 20px;
            border-top: 1px solid {colors["border"]};
            text-align: center;
            color: {colors["text_light"]};
            font-size: 12px;
        }}
    </style>
</head>
<body>
    <h1>{metadata.title}</h1>

    <div class="metadata">
        <div class="metadata-grid">
            <div class="metadata-item">
                <span class="metadata-label">Display:</span>
                <span>{metadata.display_name}</span>
            </div>
            <div class="metadata-item">
                <span class="metadata-label">Profile:</span>
                <span>{metadata.profile_name}</span>
            </div>
            <div class="metadata-item">
                <span class="metadata-label">Date:</span>
                <span>{datetime.now().strftime("%Y-%m-%d %H:%M")}</span>
            </div>
            <div class="metadata-item">
                <span class="metadata-label">Software:</span>
                <span>{metadata.software_version}</span>
            </div>
        </div>
    </div>

    <div class="summary-card">
        <h2>Verification Summary</h2>
        <p>
            <strong>Status:</strong>
            <span class="{"status-pass" if summary.overall_pass else "status-fail"}">
                {"PASSED" if summary.overall_pass else "NEEDS ATTENTION"}
            </span>
        </p>
        <p><strong>Overall Grade:</strong> {summary.overall_grade}</p>
    </div>
'''

        # ColorChecker Section
        if summary.colorchecker:
            html += self._build_colorchecker_html(summary.colorchecker)

        # Grayscale Section
        if summary.grayscale:
            html += self._build_grayscale_html(summary.grayscale)

        # Gamut Section
        if summary.gamut:
            html += self._build_gamut_html(summary.gamut)

        # Recommendations
        if summary.recommendations:
            html += f'''
    <div class="recommendations">
        <h3>Recommendations</h3>
        <ul>
            {"".join(f"<li>{rec}</li>" for rec in summary.recommendations)}
        </ul>
    </div>
'''

        # Footer
        html += f'''
    <div class="footer">
        Generated by {metadata.software_version} on {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}
    </div>
</body>
</html>
'''
        return html

    def _build_colorchecker_html(self, result: ColorCheckerResult) -> str:
        """Build ColorChecker HTML section."""
        grade_class = result.overall_grade.name.lower()

        html = f'''
    <div class="summary-card">
        <h2>ColorChecker Verification</h2>
        <span class="grade-badge grade-{grade_class}">{cc_grade_to_string(result.overall_grade)}</span>

        <div class="stat-grid">
            <div class="stat-item">
                <div class="stat-value">{result.delta_e_mean:.2f}</div>
                <div class="stat-label">Mean ΔE</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.delta_e_max:.2f}</div>
                <div class="stat-label">Max ΔE</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.delta_e_std:.2f}</div>
                <div class="stat-label">Std Dev</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.delta_e_95th:.2f}</div>
                <div class="stat-label">95th %ile</div>
            </div>
        </div>
'''

        if self.config.include_detailed_data:
            html += '''
        <h3>Patch Measurements</h3>
        <table>
            <tr>
                <th>Patch</th>
                <th>Reference L*a*b*</th>
                <th>Measured L*a*b*</th>
                <th>ΔE00</th>
            </tr>
'''
            for patch in result.patch_measurements:
                html += f'''
            <tr>
                <td>{patch.patch_name}</td>
                <td>({patch.reference_l:.1f}, {patch.reference_a:.1f}, {patch.reference_b:.1f})</td>
                <td>({patch.measured_l:.1f}, {patch.measured_a:.1f}, {patch.measured_b:.1f})</td>
                <td>{patch.delta_e_2000:.2f}</td>
            </tr>
'''
            html += '''
        </table>
'''

        html += '''
    </div>
'''
        return html

    def _build_grayscale_html(self, result: GrayscaleResult) -> str:
        """Build Grayscale HTML section."""
        grade_class = result.overall_grade.name.lower()

        return f'''
    <div class="summary-card">
        <h2>Grayscale Verification</h2>
        <span class="grade-badge grade-{grade_class}">{gs_grade_to_string(result.overall_grade)}</span>

        <div class="stat-grid">
            <div class="stat-item">
                <div class="stat-value">{result.delta_e_mean:.2f}</div>
                <div class="stat-label">Mean ΔE</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.gamma_mean:.2f}</div>
                <div class="stat-label">Mean Gamma</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.cct_mean:.0f}K</div>
                <div class="stat-label">Mean CCT</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.contrast_ratio:.0f}:1</div>
                <div class="stat-label">Contrast</div>
            </div>
        </div>

        <h3>Region Analysis</h3>
        <table>
            <tr>
                <th>Region</th>
                <th>Mean ΔE</th>
                <th>Mean Gamma</th>
                <th>CCT</th>
                <th>Grade</th>
            </tr>
            {"".join(f'''
            <tr>
                <td>{name.title()}</td>
                <td>{analysis.delta_e_mean:.2f}</td>
                <td>{analysis.gamma_mean:.2f}</td>
                <td>{analysis.cct_mean:.0f}K</td>
                <td><span class="grade-badge grade-{analysis.grade.name.lower()}">{analysis.grade.name}</span></td>
            </tr>
            ''' for name, analysis in result.region_analysis.items())}
        </table>
    </div>
'''

    def _build_gamut_html(self, result: GamutAnalysisResult) -> str:
        """Build Gamut HTML section."""
        return f'''
    <div class="summary-card">
        <h2>Gamut Analysis</h2>

        <h3>Coverage</h3>
        <table>
            <tr>
                <th>Color Space</th>
                <th>Coverage</th>
                <th>Volume Ratio</th>
                <th>Grade</th>
            </tr>
            <tr>
                <td>sRGB</td>
                <td>{result.srgb_coverage.coverage_percent:.1f}%</td>
                <td>{result.srgb_coverage.volume_ratio:.2f}</td>
                <td><span class="grade-badge grade-{result.srgb_coverage.grade.name.lower()}">{result.srgb_coverage.grade.name}</span></td>
            </tr>
            <tr>
                <td>DCI-P3</td>
                <td>{result.p3_coverage.coverage_percent:.1f}%</td>
                <td>{result.p3_coverage.volume_ratio:.2f}</td>
                <td><span class="grade-badge grade-{result.p3_coverage.grade.name.lower()}">{result.p3_coverage.grade.name}</span></td>
            </tr>
            <tr>
                <td>BT.2020</td>
                <td>{result.bt2020_coverage.coverage_percent:.1f}%</td>
                <td>{result.bt2020_coverage.volume_ratio:.2f}</td>
                <td><span class="grade-badge grade-{result.bt2020_coverage.grade.name.lower()}">{result.bt2020_coverage.grade.name}</span></td>
            </tr>
            <tr>
                <td>Adobe RGB</td>
                <td>{result.adobe_rgb_coverage.coverage_percent:.1f}%</td>
                <td>{result.adobe_rgb_coverage.volume_ratio:.2f}</td>
                <td><span class="grade-badge grade-{result.adobe_rgb_coverage.grade.name.lower()}">{result.adobe_rgb_coverage.grade.name}</span></td>
            </tr>
        </table>

        <h3>White Point</h3>
        <div class="stat-grid">
            <div class="stat-item">
                <div class="stat-value">({result.white_point_xy[0]:.4f}, {result.white_point_xy[1]:.4f})</div>
                <div class="stat-label">xy Chromaticity</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.white_point_cct:.0f}K</div>
                <div class="stat-label">CCT</div>
            </div>
            <div class="stat-item">
                <div class="stat-value">{result.white_point_duv:.4f}</div>
                <div class="stat-label">Duv</div>
            </div>
        </div>

        <h3>Measured Primaries</h3>
        <table>
            <tr>
                <th>Primary</th>
                <th>x</th>
                <th>y</th>
            </tr>
            {"".join(f'''
            <tr>
                <td>{name}</td>
                <td>{xy[0]:.4f}</td>
                <td>{xy[1]:.4f}</td>
            </tr>
            ''' for name, xy in result.measured_primaries.items())}
        </table>
    </div>
'''

    # =========================================================================
    # JSON Generation
    # =========================================================================

    def _generate_json(self, summary: VerificationSummary, metadata: ReportMetadata) -> str:
        """Generate JSON data export."""
        if self.config.output_path:
            output_path = Path(self.config.output_path)
        else:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            output_path = Path(f"calibration_report_{timestamp}.json")

        data = {
            "metadata": {
                "title": metadata.title,
                "display_name": metadata.display_name,
                "profile_name": metadata.profile_name,
                "operator": metadata.operator,
                "organization": metadata.organization,
                "timestamp": metadata.timestamp,
                "software_version": metadata.software_version,
                "notes": metadata.notes,
            },
            "summary": {
                "overall_pass": summary.overall_pass,
                "overall_grade": summary.overall_grade,
                "recommendations": summary.recommendations,
            },
        }

        # ColorChecker results
        if summary.colorchecker:
            cc = summary.colorchecker
            data["colorchecker"] = {
                "grade": cc.overall_grade.name,
                "delta_e_mean": cc.delta_e_mean,
                "delta_e_max": cc.delta_e_max,
                "delta_e_min": cc.delta_e_min,
                "delta_e_std": cc.delta_e_std,
                "delta_e_median": cc.delta_e_median,
                "delta_e_95th": cc.delta_e_95th,
                "grayscale_grade": cc.grayscale_grade.name,
                "skin_tone_grade": cc.skin_tone_grade.name,
                "patches": [
                    {
                        "id": p.patch_id,
                        "name": p.patch_name,
                        "reference_lab": list(p.reference_lab),
                        "measured_lab": list(p.measured_lab),
                        "delta_e_2000": p.delta_e_2000,
                        "delta_e_76": p.delta_e_76,
                    }
                    for p in cc.patch_measurements
                ],
            }

        # Grayscale results
        if summary.grayscale:
            gs = summary.grayscale
            data["grayscale"] = {
                "grade": gs.overall_grade.name,
                "target_whitepoint": gs.target_whitepoint,
                "target_gamma": gs.target_gamma_value,
                "delta_e_mean": gs.delta_e_mean,
                "delta_e_max": gs.delta_e_max,
                "gamma_mean": gs.gamma_mean,
                "gamma_deviation": gs.gamma_deviation,
                "cct_mean": gs.cct_mean,
                "cct_deviation": gs.cct_deviation,
                "contrast_ratio": gs.contrast_ratio,
                "dynamic_range_stops": gs.dynamic_range_stops,
                "regions": {
                    name: {
                        "delta_e_mean": a.delta_e_mean,
                        "gamma_mean": a.gamma_mean,
                        "cct_mean": a.cct_mean,
                        "grade": a.grade.name,
                    }
                    for name, a in gs.region_analysis.items()
                },
            }

        # Gamut results
        if summary.gamut:
            gm = summary.gamut
            data["gamut"] = {
                "white_point_xy": list(gm.white_point_xy),
                "white_point_cct": gm.white_point_cct,
                "white_point_duv": gm.white_point_duv,
                "total_volume_lab": gm.total_volume_lab,
                "primaries": {k: list(v) for k, v in gm.measured_primaries.items()},
                "coverage": {
                    "srgb": {
                        "percent": gm.srgb_coverage.coverage_percent,
                        "volume_ratio": gm.srgb_coverage.volume_ratio,
                        "grade": gm.srgb_coverage.grade.name,
                    },
                    "p3": {
                        "percent": gm.p3_coverage.coverage_percent,
                        "volume_ratio": gm.p3_coverage.volume_ratio,
                        "grade": gm.p3_coverage.grade.name,
                    },
                    "bt2020": {
                        "percent": gm.bt2020_coverage.coverage_percent,
                        "volume_ratio": gm.bt2020_coverage.volume_ratio,
                        "grade": gm.bt2020_coverage.grade.name,
                    },
                    "adobe_rgb": {
                        "percent": gm.adobe_rgb_coverage.coverage_percent,
                        "volume_ratio": gm.adobe_rgb_coverage.volume_ratio,
                        "grade": gm.adobe_rgb_coverage.grade.name,
                    },
                },
            }

        with open(output_path, 'w', encoding='utf-8') as f:
            json.dump(data, f, indent=2)

        return str(output_path)


# =============================================================================
# Utility Functions
# =============================================================================

def generate_recommendations(summary: VerificationSummary) -> list[str]:
    """Generate calibration recommendations based on verification results."""
    recommendations = []

    # ColorChecker recommendations
    if summary.colorchecker:
        cc = summary.colorchecker
        if cc.delta_e_mean > 3.0:
            recommendations.append(
                f"ColorChecker accuracy needs improvement (Mean ΔE = {cc.delta_e_mean:.2f}). "
                "Consider re-calibrating with more measurement patches."
            )
        if cc.grayscale_grade in [VerificationGrade.ACCEPTABLE, VerificationGrade.POOR]:
            recommendations.append(
                "Grayscale patches show color tint. Check RGB balance in grayscale calibration."
            )
        if cc.skin_tone_grade == VerificationGrade.POOR:
            recommendations.append(
                "Skin tone accuracy is poor. This is critical for portrait/video work."
            )

    # Grayscale recommendations
    if summary.grayscale:
        gs = summary.grayscale
        if abs(gs.gamma_mean - gs.target_gamma_value) > 0.1:
            recommendations.append(
                f"Gamma tracking deviates from target (Measured: {gs.gamma_mean:.2f}, "
                f"Target: {gs.target_gamma_value}). Adjust gamma calibration."
            )
        if abs(gs.cct_mean - 6500) > 200:  # Assuming D65 target
            recommendations.append(
                f"White point CCT ({gs.cct_mean:.0f}K) deviates from D65 (6500K). "
                "Check white balance calibration."
            )
        if gs.contrast_ratio < 500:
            recommendations.append(
                f"Low contrast ratio ({gs.contrast_ratio:.0f}:1). "
                "Consider adjusting backlight or checking black level."
            )

    # Gamut recommendations
    if summary.gamut:
        gm = summary.gamut
        if gm.srgb_coverage.coverage_percent < 95:
            recommendations.append(
                f"sRGB coverage is {gm.srgb_coverage.coverage_percent:.1f}%. "
                "Display may not accurately reproduce web/photo content."
            )
        if abs(gm.white_point_duv) > 0.005:
            recommendations.append(
                f"White point Duv ({gm.white_point_duv:.4f}) indicates tint. "
                "Aim for Duv < 0.005 for neutral whites."
            )

    return recommendations


def create_verification_summary(
    colorchecker: Optional[ColorCheckerResult] = None,
    grayscale: Optional[GrayscaleResult] = None,
    gamut: Optional[GamutAnalysisResult] = None,
) -> VerificationSummary:
    """Create a verification summary from individual results."""
    summary = VerificationSummary(
        colorchecker=colorchecker,
        grayscale=grayscale,
        gamut=gamut,
    )

    # Determine overall pass/fail
    grades = []
    if colorchecker:
        grades.append(colorchecker.overall_grade.value)
        if colorchecker.overall_grade == VerificationGrade.POOR:
            summary.overall_pass = False
    if grayscale:
        grades.append(grayscale.overall_grade.value)
        if grayscale.overall_grade == GrayscaleGrade.POOR:
            summary.overall_pass = False
    if gamut and gamut.srgb_coverage.coverage_percent < 80:
        summary.overall_pass = False

    # Overall grade (average of individual grades)
    if grades:
        avg_grade = sum(grades) / len(grades)
        if avg_grade <= 1.5:
            summary.overall_grade = "Reference"
        elif avg_grade <= 2.5:
            summary.overall_grade = "Excellent"
        elif avg_grade <= 3.5:
            summary.overall_grade = "Good"
        elif avg_grade <= 4.5:
            summary.overall_grade = "Acceptable"
        else:
            summary.overall_grade = "Poor"

    # Generate recommendations
    summary.recommendations = generate_recommendations(summary)

    return summary


# =============================================================================
# Module Test
# =============================================================================

if __name__ == "__main__":
    from calibrate_pro.verification.colorchecker import (
        ColorCheckerVerifier, create_test_measurements as create_cc_test
    )
    from calibrate_pro.verification.grayscale import (
        GrayscaleVerifier, create_test_measurements as create_gs_test
    )
    from calibrate_pro.verification.gamut_volume import (
        GamutAnalyzer, create_test_primaries, generate_gamut_samples, ColorSpace
    )

    # Create test results
    cc_verifier = ColorCheckerVerifier()
    cc_result = cc_verifier.verify(create_cc_test(), "Test Display", "Test Profile")

    gs_verifier = GrayscaleVerifier()
    gs_result = gs_verifier.verify(create_gs_test(), "Test Display", "Test Profile")

    gm_analyzer = GamutAnalyzer()
    gm_result = gm_analyzer.analyze(
        create_test_primaries(0.98),
        generate_gamut_samples(ColorSpace.SRGB, 9),
        "Test Display",
        "Test Profile"
    )

    # Create summary
    summary = create_verification_summary(cc_result, gs_result, gm_result)

    # Create metadata
    metadata = ReportMetadata(
        title="Display Calibration Verification Report",
        display_name="Test Display",
        profile_name="Test Profile",
        operator="Test Operator",
        organization="Test Organization",
    )

    # Generate reports
    generator = ReportGenerator(ReportConfig(format=ReportFormat.HTML))

    print("Generating HTML report...")
    html_path = generator.generate(summary, metadata)
    print(f"HTML report saved to: {html_path}")

    print("\nGenerating JSON report...")
    generator.config.format = ReportFormat.JSON
    json_path = generator.generate(summary, metadata)
    print(f"JSON report saved to: {json_path}")

    if REPORTLAB_AVAILABLE:
        print("\nGenerating PDF report...")
        generator.config.format = ReportFormat.PDF
        pdf_path = generator.generate(summary, metadata)
        print(f"PDF report saved to: {pdf_path}")
    else:
        print("\nPDF generation skipped (ReportLab not installed)")
