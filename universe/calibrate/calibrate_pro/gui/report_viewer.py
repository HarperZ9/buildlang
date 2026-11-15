"""
Report Viewer - Calibration report display and export

Displays calibration verification reports with:
- Summary statistics
- Delta E charts
- Gamut coverage
- PDF/HTML export
"""

from typing import Optional, List, Dict, Any
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
import json

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QFrame, QScrollArea, QTabWidget, QTextEdit, QFileDialog,
    QGroupBox, QGridLayout, QSizePolicy, QMessageBox
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFont, QColor, QPainter, QPixmap
from PyQt6.QtPrintSupport import QPrinter


# =============================================================================
# Theme Colors
# =============================================================================

COLORS = {
    "background": "#1a1a1a",
    "surface": "#2d2d2d",
    "surface_alt": "#383838",
    "border": "#404040",
    "text_primary": "#e0e0e0",
    "text_secondary": "#a0a0a0",
    "accent": "#4a9eff",
    "success": "#4caf50",
    "warning": "#ff9800",
    "error": "#f44336",
}


# =============================================================================
# Report Data Structures
# =============================================================================

@dataclass
class GamutCoverage:
    """Gamut coverage analysis."""
    srgb_coverage: float = 0.0
    dci_p3_coverage: float = 0.0
    bt2020_coverage: float = 0.0
    adobe_rgb_coverage: float = 0.0
    volume_ratio: float = 0.0


@dataclass
class GrayscaleResult:
    """Grayscale calibration results."""
    steps: int = 21
    avg_delta_e: float = 0.0
    max_delta_e: float = 0.0
    gamma_measured: float = 2.2
    gamma_target: float = 2.2
    rgb_balance: tuple = (0.0, 0.0, 0.0)


@dataclass
class ColorCheckerResult:
    """ColorChecker verification results."""
    patches: int = 24
    avg_delta_e: float = 0.0
    max_delta_e: float = 0.0
    min_delta_e: float = 0.0
    p95_delta_e: float = 0.0
    patch_results: List[Dict[str, Any]] = field(default_factory=list)


@dataclass
class CalibrationReport:
    """Complete calibration report."""
    # Metadata
    report_id: str = ""
    created_at: datetime = field(default_factory=datetime.now)
    software_version: str = "1.0.0"

    # Display info
    display_name: str = ""
    display_model: str = ""
    display_resolution: str = ""
    display_technology: str = ""

    # Calibration settings
    whitepoint_target: str = "D65"
    gamma_target: str = "sRGB"
    gamut_target: str = "sRGB"
    calibration_mode: str = "Sensorless"

    # Results
    grayscale: Optional[GrayscaleResult] = None
    colorchecker: Optional[ColorCheckerResult] = None
    gamut: Optional[GamutCoverage] = None

    # Profile info
    profile_name: str = ""
    profile_path: str = ""

    def to_dict(self) -> dict:
        """Convert to dictionary for JSON export."""
        return {
            "report_id": self.report_id,
            "created_at": self.created_at.isoformat(),
            "software_version": self.software_version,
            "display": {
                "name": self.display_name,
                "model": self.display_model,
                "resolution": self.display_resolution,
                "technology": self.display_technology,
            },
            "settings": {
                "whitepoint": self.whitepoint_target,
                "gamma": self.gamma_target,
                "gamut": self.gamut_target,
                "mode": self.calibration_mode,
            },
            "results": {
                "grayscale": {
                    "avg_delta_e": self.grayscale.avg_delta_e if self.grayscale else None,
                    "max_delta_e": self.grayscale.max_delta_e if self.grayscale else None,
                },
                "colorchecker": {
                    "avg_delta_e": self.colorchecker.avg_delta_e if self.colorchecker else None,
                    "max_delta_e": self.colorchecker.max_delta_e if self.colorchecker else None,
                },
            },
            "profile": {
                "name": self.profile_name,
                "path": self.profile_path,
            }
        }


# =============================================================================
# Summary Card Widget
# =============================================================================

class SummaryCard(QFrame):
    """Large summary statistic card."""

    def __init__(self, title: str, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                border-radius: 12px;
                padding: 16px;
            }}
        """)
        self.setMinimumWidth(150)

        layout = QVBoxLayout(self)
        layout.setSpacing(8)

        self.title_label = QLabel(title)
        self.title_label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 12px;")
        layout.addWidget(self.title_label)

        self.value_label = QLabel("--")
        self.value_label.setStyleSheet("font-size: 32px; font-weight: 600;")
        layout.addWidget(self.value_label)

        self.subtitle_label = QLabel("")
        self.subtitle_label.setStyleSheet(f"color: {COLORS['text_secondary']}; font-size: 11px;")
        layout.addWidget(self.subtitle_label)

    def set_value(self, value: str, color: str = None, subtitle: str = ""):
        """Set the card value."""
        self.value_label.setText(value)
        if color:
            self.value_label.setStyleSheet(f"font-size: 32px; font-weight: 600; color: {color};")
        self.subtitle_label.setText(subtitle)


# =============================================================================
# Report Summary Panel
# =============================================================================

class ReportSummaryPanel(QWidget):
    """Summary panel showing key calibration results."""

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(16)

        # Cards row
        cards_layout = QHBoxLayout()
        cards_layout.setSpacing(16)

        self.avg_delta_e_card = SummaryCard("Average Delta E")
        cards_layout.addWidget(self.avg_delta_e_card)

        self.max_delta_e_card = SummaryCard("Max Delta E")
        cards_layout.addWidget(self.max_delta_e_card)

        self.gamma_card = SummaryCard("Measured Gamma")
        cards_layout.addWidget(self.gamma_card)

        self.gamut_card = SummaryCard("sRGB Coverage")
        cards_layout.addWidget(self.gamut_card)

        layout.addLayout(cards_layout)

        # Details section
        details_group = QGroupBox("Calibration Details")
        details_layout = QGridLayout(details_group)

        self.detail_labels = {}
        details = [
            ("Display:", "display"),
            ("Resolution:", "resolution"),
            ("Target:", "target"),
            ("Mode:", "mode"),
            ("Profile:", "profile"),
            ("Date:", "date"),
        ]

        for i, (label, key) in enumerate(details):
            row = i // 2
            col = (i % 2) * 2

            label_widget = QLabel(label)
            label_widget.setStyleSheet(f"color: {COLORS['text_secondary']};")
            details_layout.addWidget(label_widget, row, col)

            value_widget = QLabel("--")
            value_widget.setStyleSheet("font-weight: 500;")
            self.detail_labels[key] = value_widget
            details_layout.addWidget(value_widget, row, col + 1)

        layout.addWidget(details_group)

    def set_report(self, report: CalibrationReport):
        """Update panel with report data."""
        # Summary cards
        if report.grayscale:
            avg = report.grayscale.avg_delta_e
            color = COLORS['success'] if avg < 1.0 else COLORS['warning'] if avg < 2.0 else COLORS['error']
            quality = "Excellent" if avg < 1.0 else "Good" if avg < 2.0 else "Needs improvement"
            self.avg_delta_e_card.set_value(f"{avg:.2f}", color, quality)

            max_de = report.grayscale.max_delta_e
            color = COLORS['success'] if max_de < 2.0 else COLORS['warning'] if max_de < 4.0 else COLORS['error']
            self.max_delta_e_card.set_value(f"{max_de:.2f}", color)

            gamma = report.grayscale.gamma_measured
            target = report.grayscale.gamma_target
            diff = abs(gamma - target)
            color = COLORS['success'] if diff < 0.05 else COLORS['warning'] if diff < 0.1 else COLORS['error']
            self.gamma_card.set_value(f"{gamma:.2f}", color, f"Target: {target:.2f}")

        if report.gamut:
            coverage = report.gamut.srgb_coverage
            color = COLORS['success'] if coverage > 99 else COLORS['warning'] if coverage > 95 else COLORS['error']
            self.gamut_card.set_value(f"{coverage:.1f}%", color)

        # Details
        self.detail_labels["display"].setText(report.display_name)
        self.detail_labels["resolution"].setText(report.display_resolution)
        self.detail_labels["target"].setText(f"{report.whitepoint_target} / {report.gamma_target}")
        self.detail_labels["mode"].setText(report.calibration_mode)
        self.detail_labels["profile"].setText(report.profile_name or "None")
        self.detail_labels["date"].setText(report.created_at.strftime("%Y-%m-%d %H:%M"))


# =============================================================================
# Report Viewer Widget
# =============================================================================

class ReportViewer(QWidget):
    """Complete report viewer with export capabilities."""

    report_exported = pyqtSignal(str)  # Path to exported file

    def __init__(self, parent: Optional[QWidget] = None):
        super().__init__(parent)
        self.report: Optional[CalibrationReport] = None
        self._setup_ui()

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # Header with export buttons
        header = QFrame()
        header.setStyleSheet(f"""
            QFrame {{
                background-color: {COLORS['surface']};
                border-bottom: 1px solid {COLORS['border']};
                padding: 12px;
            }}
        """)
        header_layout = QHBoxLayout(header)

        title = QLabel("Calibration Report")
        title.setStyleSheet("font-size: 18px; font-weight: 600;")
        header_layout.addWidget(title)

        header_layout.addStretch()

        # Export buttons
        export_pdf_btn = QPushButton("Export PDF")
        export_pdf_btn.clicked.connect(self._export_pdf)
        header_layout.addWidget(export_pdf_btn)

        export_html_btn = QPushButton("Export HTML")
        export_html_btn.clicked.connect(self._export_html)
        header_layout.addWidget(export_html_btn)

        export_json_btn = QPushButton("Export JSON")
        export_json_btn.clicked.connect(self._export_json)
        header_layout.addWidget(export_json_btn)

        layout.addWidget(header)

        # Content tabs
        self.tabs = QTabWidget()
        self.tabs.setStyleSheet(f"""
            QTabWidget::pane {{
                border: none;
                background-color: {COLORS['background']};
            }}
            QTabBar::tab {{
                background-color: {COLORS['surface']};
                border: 1px solid {COLORS['border']};
                padding: 10px 20px;
                margin-right: 2px;
            }}
            QTabBar::tab:selected {{
                background-color: {COLORS['accent']};
                color: white;
            }}
        """)

        # Summary tab
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet("QScrollArea { border: none; }")

        self.summary_panel = ReportSummaryPanel()
        scroll.setWidget(self.summary_panel)
        self.tabs.addTab(scroll, "Summary")

        # Grayscale tab (placeholder for charts)
        grayscale_widget = QWidget()
        grayscale_layout = QVBoxLayout(grayscale_widget)
        grayscale_layout.addWidget(QLabel("Grayscale verification results"))
        grayscale_layout.addStretch()
        self.tabs.addTab(grayscale_widget, "Grayscale")

        # ColorChecker tab
        colorchecker_widget = QWidget()
        colorchecker_layout = QVBoxLayout(colorchecker_widget)
        colorchecker_layout.addWidget(QLabel("ColorChecker verification results"))
        colorchecker_layout.addStretch()
        self.tabs.addTab(colorchecker_widget, "ColorChecker")

        # Gamut tab
        gamut_widget = QWidget()
        gamut_layout = QVBoxLayout(gamut_widget)
        gamut_layout.addWidget(QLabel("Gamut coverage analysis"))
        gamut_layout.addStretch()
        self.tabs.addTab(gamut_widget, "Gamut")

        # Raw data tab
        self.raw_text = QTextEdit()
        self.raw_text.setReadOnly(True)
        self.raw_text.setStyleSheet(f"""
            QTextEdit {{
                background-color: {COLORS['background']};
                border: none;
                font-family: monospace;
                font-size: 12px;
            }}
        """)
        self.tabs.addTab(self.raw_text, "Raw Data")

        layout.addWidget(self.tabs)

    def set_report(self, report: CalibrationReport):
        """Load a report for viewing."""
        self.report = report
        self.summary_panel.set_report(report)
        self.raw_text.setText(json.dumps(report.to_dict(), indent=2))

    def _export_pdf(self):
        """Export report to PDF."""
        if not self.report:
            QMessageBox.warning(self, "Export", "No report loaded")
            return

        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "Export PDF Report",
            f"calibration_report_{datetime.now().strftime('%Y%m%d_%H%M%S')}.pdf",
            "PDF Files (*.pdf)"
        )

        if file_path:
            try:
                self._generate_pdf(file_path)
                self.report_exported.emit(file_path)
                QMessageBox.information(self, "Export", f"Report exported to {file_path}")
            except Exception as e:
                QMessageBox.critical(self, "Export Error", str(e))

    def _export_html(self):
        """Export report to HTML."""
        if not self.report:
            QMessageBox.warning(self, "Export", "No report loaded")
            return

        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "Export HTML Report",
            f"calibration_report_{datetime.now().strftime('%Y%m%d_%H%M%S')}.html",
            "HTML Files (*.html)"
        )

        if file_path:
            try:
                html = self._generate_html()
                with open(file_path, 'w', encoding='utf-8') as f:
                    f.write(html)
                self.report_exported.emit(file_path)
                QMessageBox.information(self, "Export", f"Report exported to {file_path}")
            except Exception as e:
                QMessageBox.critical(self, "Export Error", str(e))

    def _export_json(self):
        """Export report to JSON."""
        if not self.report:
            QMessageBox.warning(self, "Export", "No report loaded")
            return

        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "Export JSON Report",
            f"calibration_report_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json",
            "JSON Files (*.json)"
        )

        if file_path:
            try:
                with open(file_path, 'w', encoding='utf-8') as f:
                    json.dump(self.report.to_dict(), f, indent=2)
                self.report_exported.emit(file_path)
                QMessageBox.information(self, "Export", f"Report exported to {file_path}")
            except Exception as e:
                QMessageBox.critical(self, "Export Error", str(e))

    def _generate_pdf(self, path: str):
        """Generate PDF report."""
        printer = QPrinter(QPrinter.PrinterMode.HighResolution)
        printer.setOutputFormat(QPrinter.OutputFormat.PdfFormat)
        printer.setOutputFileName(path)

        painter = QPainter()
        if not painter.begin(printer):
            raise RuntimeError("Failed to create PDF")

        # Draw report content
        font = QFont("Segoe UI", 12)
        painter.setFont(font)
        painter.drawText(100, 100, f"Calibration Report - {self.report.display_name}")
        painter.drawText(100, 130, f"Date: {self.report.created_at.strftime('%Y-%m-%d %H:%M')}")

        if self.report.grayscale:
            painter.drawText(100, 180, f"Average Delta E: {self.report.grayscale.avg_delta_e:.3f}")
            painter.drawText(100, 210, f"Max Delta E: {self.report.grayscale.max_delta_e:.3f}")

        painter.end()

    def _generate_html(self) -> str:
        """Generate HTML report."""
        report = self.report

        avg_de = report.grayscale.avg_delta_e if report.grayscale else "--"
        max_de = report.grayscale.max_delta_e if report.grayscale else "--"

        html = f"""
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Calibration Report - {report.display_name}</title>
    <style>
        body {{
            font-family: 'Segoe UI', Arial, sans-serif;
            background: #1a1a1a;
            color: #e0e0e0;
            max-width: 900px;
            margin: 0 auto;
            padding: 40px;
        }}
        h1 {{ color: #4a9eff; }}
        .card {{
            background: #2d2d2d;
            border: 1px solid #404040;
            border-radius: 8px;
            padding: 20px;
            margin: 16px 0;
        }}
        .stat {{
            font-size: 32px;
            font-weight: bold;
        }}
        .good {{ color: #4caf50; }}
        .warning {{ color: #ff9800; }}
        .poor {{ color: #f44336; }}
        table {{
            width: 100%;
            border-collapse: collapse;
        }}
        td, th {{
            padding: 8px;
            border-bottom: 1px solid #404040;
        }}
    </style>
</head>
<body>
    <h1>Calibration Report</h1>
    <p>Generated by Calibrate Pro v{report.software_version}</p>

    <div class="card">
        <h2>Display Information</h2>
        <table>
            <tr><td>Display</td><td>{report.display_name}</td></tr>
            <tr><td>Model</td><td>{report.display_model}</td></tr>
            <tr><td>Resolution</td><td>{report.display_resolution}</td></tr>
            <tr><td>Technology</td><td>{report.display_technology}</td></tr>
        </table>
    </div>

    <div class="card">
        <h2>Calibration Settings</h2>
        <table>
            <tr><td>Whitepoint</td><td>{report.whitepoint_target}</td></tr>
            <tr><td>Gamma</td><td>{report.gamma_target}</td></tr>
            <tr><td>Target Gamut</td><td>{report.gamut_target}</td></tr>
            <tr><td>Mode</td><td>{report.calibration_mode}</td></tr>
        </table>
    </div>

    <div class="card">
        <h2>Results Summary</h2>
        <p>Average Delta E: <span class="stat {'good' if float(avg_de) < 1 else 'warning' if float(avg_de) < 2 else 'poor'}">{avg_de}</span></p>
        <p>Maximum Delta E: <span class="stat">{max_de}</span></p>
    </div>

    <div class="card">
        <p><small>Report generated on {report.created_at.strftime('%Y-%m-%d %H:%M:%S')}</small></p>
    </div>
</body>
</html>
        """
        return html
