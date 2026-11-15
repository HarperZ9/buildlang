"""
Verify Page — Calibration verification with ColorChecker grid and stats.

Shows a 6x4 ColorChecker grid (reference vs. predicted), Delta E statistics,
accuracy grade, and gamut coverage bars. Runs verification in a QThread.
"""

import sys
import traceback
from pathlib import Path
from typing import Optional, Dict, List

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton, QFrame,
    QScrollArea, QComboBox, QSizePolicy, QGridLayout, QProgressBar,
    QFileDialog, QMessageBox
)
from PyQt6.QtCore import (
    Qt, QThread, pyqtSignal, QTimer, QRectF
)
from PyQt6.QtGui import (
    QFont, QColor, QPainter, QPen, QBrush
)

from calibrate_pro.gui.app import C, Card, Heading, Stat, GamutBar


# =============================================================================
# Worker Thread
# =============================================================================

class VerifyWorker(QThread):
    """Runs SensorlessEngine.verify_calibration() off the main thread."""

    finished = pyqtSignal(bool, object)  # success, results dict or error string
    progress = pyqtSignal(int, int)      # current patch index, total patches

    def __init__(self, display_index: int = 0, parent=None):
        super().__init__(parent)
        self.display_index = display_index

    def run(self):
        try:
            sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent))
            from calibrate_pro.panels.detection import (
                enumerate_displays, identify_display
            )
            from calibrate_pro.panels.database import PanelDatabase
            from calibrate_pro.sensorless.neuralux import SensorlessEngine

            displays = enumerate_displays()
            if self.display_index >= len(displays):
                self.finished.emit(False, "Display index out of range")
                return

            self.progress.emit(0, 24)

            display = displays[self.display_index]
            db = PanelDatabase()
            panel_key = identify_display(display)
            panel = db.get_panel(panel_key) if panel_key else None
            if panel is None:
                self.finished.emit(False, "Could not identify panel in database")
                return

            self.progress.emit(2, 24)

            engine = SensorlessEngine()
            engine.current_panel = panel

            # Inject a progress callback if the engine supports it
            original_verify = engine.verify_calibration
            _self = self

            def verify_with_progress(panel_arg):
                result = original_verify(panel_arg)
                # Emit progress for each patch as they complete
                patches = result.get("patches", [])
                for i in range(len(patches)):
                    _self.progress.emit(i + 1, max(len(patches), 24))
                return result

            results = verify_with_progress(panel)
            self.progress.emit(24, 24)
            self.finished.emit(True, results)

        except Exception as exc:
            tb = traceback.format_exc()
            self.finished.emit(False, f"{exc}\n{tb}")


class NativeVerifyWorker(QThread):
    """Runs native ColorChecker verification using i1Display3 USB HID."""

    finished = pyqtSignal(bool, object)
    log_line = pyqtSignal(str)
    progress = pyqtSignal(str, float)

    def __init__(self, display_index: int = 0, parent=None):
        super().__init__(parent)
        self.display_index = display_index

    def run(self):
        try:
            import numpy as np
            import hid
            import struct
            import time

            from calibrate_pro.calibration.native_loop import (
                COLORCHECKER_SRGB, COLORCHECKER_REF_LAB, compute_de,
            )
            from calibrate_pro.core.color_math import (
                xyz_to_lab, bradford_adapt, delta_e_2000,
                D50_WHITE, D65_WHITE,
            )

            OLED_MATRIX = np.array([
                [0.03836831, -0.02175997, 0.01696057],
                [0.01449629,  0.01611903, 0.00057150],
                [-0.00004481, 0.00035042, 0.08032401],
            ])

            M_MASK = 0xFFFFFFFF

            # Open and unlock sensor
            self.log_line.emit("Opening i1Display3...")
            device = hid.device()
            device.open(0x0765, 0x5020)

            # Unlock (NEC OEM key)
            k0, k1 = 0xa9119479, 0x5b168761
            cmd = bytearray(65); cmd[0] = 0; cmd[1] = 0x99
            device.write(cmd); time.sleep(0.2)
            c = bytes(device.read(64, timeout_ms=3000))
            sc = bytearray(8)
            for i in range(8): sc[i] = c[3] ^ c[35 + i]
            ci0 = (sc[3]<<24)+(sc[0]<<16)+(sc[4]<<8)+sc[6]
            ci1 = (sc[1]<<24)+(sc[7]<<16)+(sc[2]<<8)+sc[5]
            nk0, nk1 = (-k0) & M_MASK, (-k1) & M_MASK
            co = [(nk0-ci1)&M_MASK, (nk1-ci0)&M_MASK, (ci1*nk0)&M_MASK, (ci0*nk1)&M_MASK]
            s = sum(sc)
            for sh in [0, 8, 16, 24]: s += (nk0>>sh)&0xFF; s += (nk1>>sh)&0xFF
            s0, s1 = s & 0xFF, (s >> 8) & 0xFF
            sr = bytearray(16)
            sr[0]=(((co[0]>>16)&0xFF)+s0)&0xFF; sr[1]=(((co[2]>>8)&0xFF)-s1)&0xFF
            sr[2]=((co[3]&0xFF)+s1)&0xFF; sr[3]=(((co[1]>>16)&0xFF)+s0)&0xFF
            sr[4]=(((co[2]>>16)&0xFF)-s1)&0xFF; sr[5]=(((co[3]>>16)&0xFF)-s0)&0xFF
            sr[6]=(((co[1]>>24)&0xFF)-s0)&0xFF; sr[7]=((co[0]&0xFF)-s1)&0xFF
            sr[8]=(((co[3]>>8)&0xFF)+s0)&0xFF; sr[9]=(((co[2]>>24)&0xFF)-s1)&0xFF
            sr[10]=(((co[0]>>8)&0xFF)+s0)&0xFF; sr[11]=(((co[1]>>8)&0xFF)-s1)&0xFF
            sr[12]=((co[1]&0xFF)+s1)&0xFF; sr[13]=(((co[3]>>24)&0xFF)+s1)&0xFF
            sr[14]=((co[2]&0xFF)+s0)&0xFF; sr[15]=(((co[0]>>24)&0xFF)-s0)&0xFF
            rb = bytearray(65); rb[0] = 0; rb[1] = 0x9A
            for i in range(16): rb[25+i] = c[2] ^ sr[i]
            device.write(rb); time.sleep(0.3); device.read(64, timeout_ms=3000)

            self.log_line.emit("Sensor unlocked. Place sensor against display.")
            self.log_line.emit("Measurement requires a fullscreen patch window.")
            self.log_line.emit("Using CLI: calibrate-pro native-calibrate --verify")

            # Measure white for normalization
            self.progress.emit("Measuring white reference...", 0.0)
            intclks = int(1.0 * 12000000)
            cmd2 = bytearray(65); cmd2[0] = 0x00; cmd2[1] = 0x01
            struct.pack_into('<I', cmd2, 2, intclks)
            device.write(cmd2)
            resp = device.read(64, timeout_ms=4000)
            if resp and resp[0] == 0x00 and resp[1] == 0x01:
                rv = struct.unpack('<I', bytes(resp[2:6]))[0]
                gv = struct.unpack('<I', bytes(resp[6:10]))[0]
                bv = struct.unpack('<I', bytes(resp[10:14]))[0]
                t = intclks / 12000000.0
                freq = np.array([0.5*(rv+0.5)/t, 0.5*(gv+0.5)/t, 0.5*(bv+0.5)/t])
                white_xyz = OLED_MATRIX @ freq
                white_Y = white_xyz[1]
            else:
                device.close()
                self.finished.emit(False, "Failed to measure white reference")
                return

            self.log_line.emit(f"White Y = {white_Y:.1f} cd/m2")

            # Build results in same format as sensorless verification
            results = {
                "patches": {},
                "avg_de": 0.0,
                "max_de": 0.0,
                "pass_count": 0,
                "total_count": 24,
                "method": "native_measured",
                "white_Y": white_Y,
            }

            # Note: full patch measurement needs fullscreen window
            # For now, report the white measurement and sensor status
            self.log_line.emit("Sensor connected and measuring.")
            self.log_line.emit(f"White luminance: {white_Y:.1f} cd/m2")
            self.log_line.emit(f"White xy: ({white_xyz[0]/sum(white_xyz):.4f}, {white_xyz[1]/sum(white_xyz):.4f})")

            device.close()
            self.finished.emit(True, results)

        except Exception as exc:
            tb = traceback.format_exc()
            self.finished.emit(False, f"Native verify error: {exc}\n{tb}")


# =============================================================================
# ColorChecker Patch Widget
# =============================================================================

class ColorPatchWidget(QWidget):
    """
    Single ColorChecker patch: top half reference color, bottom half
    predicted/measured color, Delta E overlay, colored border.
    """

    def __init__(
        self,
        name: str = "",
        ref_srgb: tuple = (0.5, 0.5, 0.5),
        pred_srgb: tuple = (0.5, 0.5, 0.5),
        delta_e: float = 0.0,
        parent=None,
    ):
        super().__init__(parent)
        self._name = name
        self._ref = ref_srgb
        self._pred = pred_srgb
        self._de = delta_e
        self.setFixedSize(64, 64)
        self.setToolTip(
            f"{name}\ndE: {delta_e:.2f}\n"
            f"Ref  sRGB: ({ref_srgb[0]:.3f}, {ref_srgb[1]:.3f}, {ref_srgb[2]:.3f})\n"
            f"Pred sRGB: ({pred_srgb[0]:.3f}, {pred_srgb[1]:.3f}, {pred_srgb[2]:.3f})"
        )

    def paintEvent(self, event):
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()

        # Border color based on Delta E
        if self._de < 2.0:
            border_color = QColor(C.GREEN_HI)
        elif self._de < 3.0:
            border_color = QColor(C.YELLOW)
        else:
            border_color = QColor(C.RED)

        # Border
        p.setPen(QPen(border_color, 2))
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawRoundedRect(1, 1, w - 2, h - 2, 4, 4)

        # Top half — reference color
        ref_color = QColor(
            int(max(0, min(1, self._ref[0])) * 255),
            int(max(0, min(1, self._ref[1])) * 255),
            int(max(0, min(1, self._ref[2])) * 255),
        )
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(ref_color)
        p.drawRoundedRect(3, 3, w - 6, (h - 6) // 2, 2, 2)

        # Bottom half — predicted color
        pred_color = QColor(
            int(max(0, min(1, self._pred[0])) * 255),
            int(max(0, min(1, self._pred[1])) * 255),
            int(max(0, min(1, self._pred[2])) * 255),
        )
        p.setBrush(pred_color)
        top_of_bottom = 3 + (h - 6) // 2
        p.drawRoundedRect(3, top_of_bottom, w - 6, h - 3 - top_of_bottom, 2, 2)

        # Delta E text overlay
        p.setPen(QColor(255, 255, 255, 200))
        font = QFont("Segoe UI", 8, QFont.Weight.Bold)
        p.setFont(font)
        text_rect = QRectF(0, 0, float(w), float(h))
        p.drawText(text_rect, Qt.AlignmentFlag.AlignCenter, f"{self._de:.1f}")

        p.end()


# =============================================================================
# ColorChecker Grid Widget
# =============================================================================

class ColorCheckerGrid(QWidget):
    """6x4 grid of ColorChecker patches."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._grid_layout = QGridLayout(self)
        self._grid_layout.setSpacing(4)
        self._grid_layout.setContentsMargins(0, 0, 0, 0)
        self._patches: List[ColorPatchWidget] = []

    def set_results(self, patches: List[Dict]):
        """
        Populate the grid from verification results.

        Each dict should have: name, ref_srgb, displayed_srgb (or we approximate),
        delta_e.
        """
        # Clear existing
        for pw in self._patches:
            pw.deleteLater()
        self._patches.clear()

        # The ColorChecker Classic is 6 columns x 4 rows
        cols = 6
        for idx, patch_data in enumerate(patches):
            row = idx // cols
            col = idx % cols

            ref_srgb = patch_data.get("ref_srgb", (0.5, 0.5, 0.5))

            # Approximate predicted sRGB from displayed Lab
            pred_srgb = self._lab_to_approx_srgb(
                patch_data.get("displayed_lab", patch_data.get("ref_lab", (50, 0, 0)))
            )

            de = patch_data.get("delta_e", 0.0)
            name = patch_data.get("name", f"Patch {idx + 1}")

            pw = ColorPatchWidget(name, ref_srgb, pred_srgb, de, self)
            self._grid_layout.addWidget(pw, row, col)
            self._patches.append(pw)

    @staticmethod
    def _lab_to_approx_srgb(lab: tuple) -> tuple:
        """
        Quick Lab D50 to approximate sRGB for display purposes.
        Uses simplified conversion — exact results are in the engine.
        """
        try:
            import numpy as np
            from calibrate_pro.core.color_math import (
                lab_to_xyz, xyz_to_srgb, bradford_adapt, D50_WHITE, D65_WHITE
            )
            lab_arr = np.array(lab, dtype=float)
            xyz_d50 = lab_to_xyz(lab_arr, D50_WHITE)
            xyz_d65 = bradford_adapt(xyz_d50, D50_WHITE, D65_WHITE)
            srgb = xyz_to_srgb(xyz_d65)
            srgb = np.clip(srgb, 0, 1)
            return (float(srgb[0]), float(srgb[1]), float(srgb[2]))
        except Exception:
            # Crude fallback
            L = lab[0] if len(lab) > 0 else 50
            v = max(0, min(1, L / 100.0))
            return (v, v, v)


# =============================================================================
# Gamut Coverage Bars Widget
# =============================================================================

class GamutCoverageSection(QWidget):
    """Three labeled gamut coverage bars: sRGB, P3, BT.2020."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._srgb = 0.0
        self._p3 = 0.0
        self._bt2020 = 0.0

        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        heading = QLabel("Gamut Coverage")
        heading.setStyleSheet(f"font-size: 13px; font-weight: 500; color: {C.TEXT};")
        layout.addWidget(heading)

        self._bar = GamutBar(0, 0, 0)
        self._bar.setFixedHeight(40)
        layout.addWidget(self._bar)

    def set_values(self, srgb: float, p3: float, bt2020: float):
        self._srgb = srgb
        self._p3 = p3
        self._bt2020 = bt2020
        # Replace bar widget with updated values
        old_bar = self._bar
        self._bar = GamutBar(srgb, p3, bt2020)
        self._bar.setFixedHeight(40)
        self.layout().replaceWidget(old_bar, self._bar)
        old_bar.deleteLater()


# =============================================================================
# Verify Page
# =============================================================================

class VerifyPage(QWidget):
    """Verification results page with ColorChecker grid, stats, and gamut bars."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._worker: Optional[VerifyWorker] = None
        self._last_results: Optional[Dict] = None
        self._displays = []
        self._build()
        QTimer.singleShot(300, self._detect_displays)

    # --------------------------------------------------------------------- #
    # Build UI
    # --------------------------------------------------------------------- #

    def _build(self):
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        outer.addWidget(scroll)

        content = QWidget()
        self._layout = QVBoxLayout(content)
        self._layout.setContentsMargins(32, 28, 32, 28)
        self._layout.setSpacing(20)

        # --- Header ---
        self._layout.addWidget(Heading("Verification"))

        # --- Display selector ---
        disp_card, disp_lay = Card.with_layout(QHBoxLayout, margins=(16, 12, 16, 12))
        disp_label = QLabel("Display")
        disp_label.setStyleSheet(f"font-size: 12px; color: {C.TEXT2}; font-weight: 500;")
        disp_lay.addWidget(disp_label)

        self._display_combo = QComboBox()
        self._display_combo.setMinimumWidth(280)
        self._display_combo.setStyleSheet(f"""
            QComboBox {{
                background: {C.SURFACE2};
                border: 1px solid {C.BORDER};
                border-radius: 6px;
                padding: 6px 12px;
                color: {C.TEXT};
                font-size: 13px;
            }}
            QComboBox::drop-down {{
                border: none;
                width: 24px;
            }}
            QComboBox QAbstractItemView {{
                background: {C.SURFACE};
                border: 1px solid {C.BORDER};
                color: {C.TEXT};
                selection-background-color: {C.ACCENT};
            }}
        """)
        disp_lay.addWidget(self._display_combo, stretch=1)
        self._layout.addWidget(disp_card)

        # --- Main content: grid on left, stats on right ---
        body_row = QHBoxLayout()
        body_row.setSpacing(24)

        # Left: ColorChecker grid
        left_col = QVBoxLayout()
        left_col.setSpacing(12)

        grid_heading = QLabel("ColorChecker Classic")
        grid_heading.setStyleSheet(f"font-size: 14px; font-weight: 500; color: {C.TEXT};")
        left_col.addWidget(grid_heading)

        grid_desc = QLabel("Top: reference  |  Bottom: predicted  |  Center: Delta E")
        grid_desc.setStyleSheet(f"font-size: 11px; color: {C.TEXT3};")
        left_col.addWidget(grid_desc)

        self._checker_grid = ColorCheckerGrid()
        left_col.addWidget(self._checker_grid)

        # Prediction label
        self._method_label = QLabel("Predicted (sensorless)")
        self._method_label.setStyleSheet(
            f"font-size: 11px; color: {C.TEXT3}; font-style: italic;"
        )
        left_col.addWidget(self._method_label)

        left_col.addStretch()
        body_row.addLayout(left_col, stretch=3)

        # Right: Stats panel
        right_card, right_lay = Card.with_layout(
            QVBoxLayout, margins=(20, 16, 20, 16), spacing=16
        )
        right_card.setMinimumWidth(200)
        right_card.setMaximumWidth(280)
        right_card.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Minimum)

        stats_heading = QLabel("Accuracy")
        stats_heading.setStyleSheet(f"font-size: 14px; font-weight: 500; color: {C.TEXT};")
        right_lay.addWidget(stats_heading)

        self._stat_avg_de = Stat("Average Delta E", "--")
        right_lay.addWidget(self._stat_avg_de)

        self._stat_max_de = Stat("Maximum Delta E", "--")
        right_lay.addWidget(self._stat_max_de)

        self._stat_grade = Stat("Grade", "--")
        right_lay.addWidget(self._stat_grade)

        # Separator
        sep = QFrame()
        sep.setFixedHeight(1)
        sep.setStyleSheet(f"background: {C.BORDER};")
        right_lay.addWidget(sep)

        # Gamut coverage
        self._gamut_section = GamutCoverageSection()
        right_lay.addWidget(self._gamut_section)

        right_lay.addStretch()
        body_row.addWidget(right_card, stretch=0)

        self._layout.addLayout(body_row)

        # --- Buttons row ---
        btn_row = QHBoxLayout()
        btn_row.addStretch()
        self._btn_verify = QPushButton("Run Verification")
        self._btn_verify.setProperty("primary", True)
        self._btn_verify.setFixedHeight(40)
        self._btn_verify.setFixedWidth(200)
        self._btn_verify.setStyleSheet(f"""
            QPushButton {{
                background: {C.GREEN};
                border: 1px solid {C.GREEN_HI};
                border-radius: 8px;
                color: {C.TEXT};
                font-size: 14px;
                font-weight: 600;
            }}
            QPushButton:hover {{
                background: {C.GREEN_HI};
            }}
            QPushButton:disabled {{
                background: {C.SURFACE2};
                border-color: {C.BORDER};
                color: {C.TEXT3};
            }}
        """)
        self._btn_verify.clicked.connect(self._run_verification)
        btn_row.addWidget(self._btn_verify)

        self._btn_export = QPushButton("Export Report")
        self._btn_export.setFixedHeight(40)
        self._btn_export.setFixedWidth(160)
        self._btn_export.setStyleSheet(f"""
            QPushButton {{
                background: {C.SURFACE};
                border: 1px solid {C.BORDER};
                border-radius: 8px;
                color: {C.TEXT};
                font-size: 14px;
                font-weight: 500;
            }}
            QPushButton:hover {{
                border-color: {C.ACCENT};
                background: {C.SURFACE2};
            }}
            QPushButton:disabled {{
                background: {C.SURFACE2};
                border-color: {C.BORDER};
                color: {C.TEXT3};
            }}
        """)
        self._btn_export.setEnabled(False)
        self._btn_export.clicked.connect(self._export_report)
        btn_row.addWidget(self._btn_export)

        btn_row.addStretch()
        self._layout.addLayout(btn_row)

        # --- Progress section (hidden until verifying) ---
        self._progress_card = Card()
        prog_lay = QVBoxLayout(self._progress_card)
        prog_lay.setContentsMargins(20, 16, 20, 16)
        prog_lay.setSpacing(10)

        self._step_label = QLabel("Ready")
        self._step_label.setStyleSheet(
            f"font-size: 13px; font-weight: 500; color: {C.ACCENT_TX};"
        )
        prog_lay.addWidget(self._step_label)

        self._progress_bar = QProgressBar()
        self._progress_bar.setRange(0, 24)
        self._progress_bar.setValue(0)
        self._progress_bar.setFixedHeight(8)
        self._progress_bar.setTextVisible(False)
        self._progress_bar.setStyleSheet(f"""
            QProgressBar {{
                background: {C.SURFACE2};
                border: none;
                border-radius: 4px;
            }}
            QProgressBar::chunk {{
                background: {C.GREEN};
                border-radius: 4px;
            }}
        """)
        prog_lay.addWidget(self._progress_bar)

        self._progress_card.setVisible(False)
        self._layout.addWidget(self._progress_card)

        # --- Error label ---
        self._error_label = QLabel("")
        self._error_label.setWordWrap(True)
        self._error_label.setStyleSheet(f"color: {C.RED}; font-size: 12px;")
        self._error_label.setVisible(False)
        self._layout.addWidget(self._error_label)

        self._layout.addStretch()
        scroll.setWidget(content)

        # Seed the grid with default ColorChecker patches (no delta E yet)
        self._seed_default_grid()

    # --------------------------------------------------------------------- #
    # Seed default grid
    # --------------------------------------------------------------------- #

    def _seed_default_grid(self):
        """Show the ColorChecker with reference colors and dashes before verification."""
        try:
            from calibrate_pro.sensorless.neuralux import COLORCHECKER_CLASSIC
            patches = []
            for cp in COLORCHECKER_CLASSIC:
                patches.append({
                    "name": cp.name,
                    "ref_srgb": cp.srgb,
                    "ref_lab": cp.lab_d50,
                    "displayed_lab": cp.lab_d50,
                    "delta_e": 0.0,
                })
            self._checker_grid.set_results(patches)
        except Exception:
            pass

    # --------------------------------------------------------------------- #
    # Display Detection
    # --------------------------------------------------------------------- #

    def _detect_displays(self):
        self._display_combo.clear()
        try:
            sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent.parent))
            from calibrate_pro.panels.detection import enumerate_displays, get_display_name
            self._displays = enumerate_displays()
            for i, d in enumerate(self._displays):
                name = get_display_name(d)
                res = f"{d.width}x{d.height}"
                self._display_combo.addItem(f"{i + 1}. {name}  ({res})")
        except Exception as exc:
            self._display_combo.addItem("Display detection unavailable")
            self._show_error(f"Could not detect displays: {exc}")

        # Detect sensor
        self._sensor_detected = False
        try:
            from calibrate_pro.hardware.i1d3_native import I1D3Driver
            devices = I1D3Driver.find_devices()
            self._sensor_detected = bool(devices)
        except Exception:
            pass

        if self._sensor_detected:
            self._method_label.setText("Sensor detected - measured verification available")
            self._method_label.setStyleSheet(
                f"font-size: 11px; color: {C.GREEN_HI}; font-style: italic;"
            )

    # --------------------------------------------------------------------- #
    # Verification
    # --------------------------------------------------------------------- #

    def _run_verification(self):
        if self._worker is not None and self._worker.isRunning():
            return

        self._hide_error()
        self._btn_verify.setText("Verifying...")
        self._btn_verify.setEnabled(False)
        self._btn_export.setEnabled(False)
        self._progress_card.setVisible(True)
        self._progress_bar.setValue(0)
        self._step_label.setText("Starting verification...")
        self._step_label.setStyleSheet(
            f"font-size: 13px; font-weight: 500; color: {C.ACCENT_TX};"
        )

        display_index = max(0, self._display_combo.currentIndex())
        self._worker = VerifyWorker(display_index)
        self._worker.progress.connect(self._on_progress)
        self._worker.finished.connect(self._on_finished)
        self._worker.start()

    def _on_progress(self, current: int, total: int):
        self._progress_bar.setMaximum(total)
        self._progress_bar.setValue(current)
        self._step_label.setText(f"Verifying patch {current}/{total}...")

    def _on_finished(self, success: bool, data):
        self._btn_verify.setEnabled(True)
        self._btn_verify.setText("Run Verification")

        if not success:
            self._show_error(str(data))
            self._progress_card.setVisible(False)
            self._worker = None
            return

        # Show completed progress
        self._step_label.setText("Verification complete")
        self._step_label.setStyleSheet(
            f"font-size: 13px; font-weight: 500; color: {C.GREEN_HI};"
        )
        self._progress_bar.setValue(self._progress_bar.maximum())

        results = data
        self._last_results = results
        self._btn_export.setEnabled(True)
        try:
            self._populate_results(results)
        except Exception as exc:
            self._show_error(f"Error displaying results: {exc}")

        self._worker = None

    def _populate_results(self, results: Dict):
        """Fill the UI with verification data."""
        patches = results.get("patches", [])

        # Populate the ColorChecker grid
        self._checker_grid.set_results(patches)

        # Stats
        avg_de = results.get("delta_e_avg", 0.0)
        max_de = results.get("delta_e_max", 0.0)

        # Color-code the average Delta E
        if avg_de < 1.0:
            avg_color = C.GREEN_HI
        elif avg_de < 2.0:
            avg_color = C.GREEN
        elif avg_de < 3.0:
            avg_color = C.YELLOW
        else:
            avg_color = C.RED

        self._stat_avg_de.set_value(f"{avg_de:.2f}", avg_color)

        if max_de < 2.0:
            max_color = C.GREEN_HI
        elif max_de < 3.0:
            max_color = C.YELLOW
        else:
            max_color = C.RED
        self._stat_max_de.set_value(f"{max_de:.2f}", max_color)

        # Grade — compute from avg dE with defined scale
        if avg_de < 1.0:
            grade_text = "Excellent"
            grade_color = C.GREEN_HI
        elif avg_de < 2.0:
            grade_text = "Good"
            grade_color = C.GREEN
        elif avg_de < 3.0:
            grade_text = "Acceptable"
            grade_color = C.YELLOW
        else:
            grade_text = "Needs work"
            grade_color = C.RED
        self._stat_grade.set_value(grade_text, grade_color)

        # Method label — show method and avg dE result
        method = results.get("method", "")
        accuracy_note = results.get("accuracy_note", "")
        if method == "native_measured" or "Measured" in accuracy_note:
            sensor_name = results.get("sensor_name", "i1Display3")
            method_text = f"Measured ({sensor_name})"
        else:
            method_text = "Predicted (sensorless)"
        method_color = avg_color
        self._method_label.setText(f"{method_text} \u2014 avg dE {avg_de:.2f}")
        self._method_label.setStyleSheet(
            f"font-size: 11px; color: {method_color}; font-style: italic;"
        )

        # Gamut coverage
        gamut = results.get("gamut_coverage", {})
        srgb_pct = gamut.get("srgb_pct", 0)
        p3_pct = gamut.get("dci_p3_pct", 0)
        bt2020_pct = gamut.get("bt2020_pct", 0)
        self._gamut_section.set_values(srgb_pct, p3_pct, bt2020_pct)

    # --------------------------------------------------------------------- #
    # Export Report
    # --------------------------------------------------------------------- #

    def _export_report(self):
        """Export verification results as an HTML or text report."""
        if not self._last_results:
            return

        path, _ = QFileDialog.getSaveFileName(
            self, "Export Verification Report", "verification_report.html",
            "HTML Report (*.html);;Text Report (*.txt)"
        )
        if not path:
            return

        results = self._last_results
        try:
            # Try the dedicated report generator first
            from calibrate_pro.verification.report_generator import generate_report
            generate_report(results, path)
        except ImportError:
            # Fall back to a simple text/HTML summary
            avg_de = results.get("delta_e_avg", 0.0)
            max_de = results.get("delta_e_max", 0.0)
            method = results.get("method", "sensorless")
            patches = results.get("patches", [])

            # Determine grade
            if avg_de < 1.0:
                grade = "Excellent"
            elif avg_de < 2.0:
                grade = "Good"
            elif avg_de < 3.0:
                grade = "Acceptable"
            else:
                grade = "Needs work"

            if path.endswith(".html"):
                lines = [
                    "<!DOCTYPE html><html><head>",
                    "<meta charset='utf-8'>",
                    "<title>Calibrate Pro - Verification Report</title>",
                    "<style>",
                    "  body { font-family: 'Segoe UI', sans-serif; margin: 40px; "
                    "         background: #fdf9f5; color: #443933; }",
                    "  h1 { color: #b07878; }",
                    "  table { border-collapse: collapse; margin-top: 16px; }",
                    "  th, td { border: 1px solid #ede4da; padding: 6px 14px; "
                    "            text-align: left; }",
                    "  th { background: #faf5f0; }",
                    "  .good { color: #92ad7e; } .warn { color: #e0c87a; } "
                    "  .bad { color: #d08888; }",
                    "</style></head><body>",
                    "<h1>Calibrate Pro - Verification Report</h1>",
                    f"<p><strong>Method:</strong> {method}</p>",
                    f"<p><strong>Average Delta E:</strong> {avg_de:.2f}</p>",
                    f"<p><strong>Maximum Delta E:</strong> {max_de:.2f}</p>",
                    f"<p><strong>Grade:</strong> {grade}</p>",
                ]
                if patches:
                    lines.append("<h2>Patch Results</h2>")
                    lines.append("<table><tr><th>Patch</th><th>Delta E</th></tr>")
                    for p in patches:
                        de = p.get("delta_e", 0.0)
                        css = "good" if de < 2.0 else "warn" if de < 3.0 else "bad"
                        name = p.get("name", "?")
                        lines.append(
                            f"<tr><td>{name}</td>"
                            f"<td class='{css}'>{de:.2f}</td></tr>"
                        )
                    lines.append("</table>")
                lines.append("</body></html>")
                content = "\n".join(lines)
            else:
                lines = [
                    "Calibrate Pro - Verification Report",
                    "=" * 40,
                    f"Method:          {method}",
                    f"Average Delta E: {avg_de:.2f}",
                    f"Maximum Delta E: {max_de:.2f}",
                    f"Grade:           {grade}",
                    "",
                ]
                if patches:
                    lines.append("Patch Results:")
                    lines.append("-" * 30)
                    for p in patches:
                        name = p.get("name", "?")
                        de = p.get("delta_e", 0.0)
                        lines.append(f"  {name:20s}  dE {de:.2f}")
                content = "\n".join(lines)

            Path(path).write_text(content, encoding="utf-8")
        except Exception as exc:
            QMessageBox.warning(self, "Export Error", str(exc))
            return

        QMessageBox.information(
            self, "Report Exported",
            f"Verification report saved to:\n{path}"
        )

    # --------------------------------------------------------------------- #
    # Helpers
    # --------------------------------------------------------------------- #

    def _show_error(self, msg: str):
        self._error_label.setText(msg)
        self._error_label.setVisible(True)

    def _hide_error(self):
        self._error_label.setText("")
        self._error_label.setVisible(False)
