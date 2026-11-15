"""
Calibrate Pro — Profiles Page

Profile management: view, activate, export, and delete calibration profiles.
Scans ~/Documents/Calibrate Pro/Calibrations/ for .cube and .icc file pairs.
"""

from datetime import datetime
from pathlib import Path
from typing import Optional, List, Dict

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QFrame, QScrollArea, QSizePolicy, QMessageBox, QFileDialog,
    QSpacerItem,
)
from PyQt6.QtCore import Qt, QSize

from calibrate_pro.gui.app import C, Card, Heading, Stat, StatusDot


# =============================================================================
# Constants
# =============================================================================

CALIBRATIONS_DIR = Path.home() / "Documents" / "Calibrate Pro" / "Calibrations"

TARGET_GAMUTS = ["sRGB", "Display P3", "BT.709", "Adobe RGB"]


# =============================================================================
# Profile Data
# =============================================================================

def _scan_profiles() -> List[Dict]:
    """
    Scan the calibrations directory for .cube / .icc pairs.

    Returns a list of dicts with keys:
        name, cube_path, icc_path, cube_size, icc_size
    """
    profiles: List[Dict] = []

    if not CALIBRATIONS_DIR.exists():
        return profiles

    # Collect all .cube files, then look for matching .icc
    cube_files = sorted(CALIBRATIONS_DIR.glob("*.cube"))

    seen_stems: set = set()

    for cube in cube_files:
        stem = cube.stem
        if stem in seen_stems:
            continue
        seen_stems.add(stem)

        icc = CALIBRATIONS_DIR / f"{stem}.icc"
        icc_path = icc if icc.exists() else None

        cube_stat = cube.stat() if cube.exists() else None
        icc_stat = icc_path.stat() if icc_path and icc_path.exists() else None

        # Use the most recent modification time from either file
        mod_time = None
        if cube_stat:
            mod_time = datetime.fromtimestamp(cube_stat.st_mtime)
        if icc_stat:
            icc_mod = datetime.fromtimestamp(icc_stat.st_mtime)
            if mod_time is None or icc_mod > mod_time:
                mod_time = icc_mod

        profiles.append({
            "name": stem.replace("_", " ").replace("-", " — ", 1),
            "stem": stem,
            "cube_path": cube,
            "icc_path": icc_path,
            "cube_size": cube_stat.st_size if cube_stat else 0,
            "icc_size": icc_stat.st_size if icc_stat else 0,
            "modified": mod_time,
        })

    # Also pick up .icc files that have no matching .cube
    for icc in sorted(CALIBRATIONS_DIR.glob("*.icc")):
        if icc.stem not in seen_stems:
            seen_stems.add(icc.stem)
            icc_stat = icc.stat()
            mod_time = datetime.fromtimestamp(icc_stat.st_mtime)
            profiles.append({
                "name": icc.stem.replace("_", " ").replace("-", " — ", 1),
                "stem": icc.stem,
                "cube_path": None,
                "icc_path": icc,
                "cube_size": 0,
                "icc_size": icc_stat.st_size,
                "modified": mod_time,
            })

    return profiles


def _format_size(size_bytes: int) -> str:
    """Human-readable file size."""
    if size_bytes == 0:
        return "—"
    if size_bytes < 1024:
        return f"{size_bytes} B"
    if size_bytes < 1024 * 1024:
        return f"{size_bytes / 1024:.1f} KB"
    return f"{size_bytes / (1024 * 1024):.1f} MB"


# =============================================================================
# Profile Card Widget
# =============================================================================

class ProfileCard(Card):
    """Card showing a single calibration profile."""

    def __init__(self, profile: Dict, is_active: bool = False, parent=None):
        super().__init__(parent)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.setMinimumHeight(110)
        self._profile = profile

        root = QVBoxLayout(self)
        root.setContentsMargins(20, 16, 20, 16)
        root.setSpacing(10)

        # --- Top row: name + active pill ---
        top = QHBoxLayout()
        top.setSpacing(10)

        name_label = QLabel(profile["name"])
        name_label.setStyleSheet(
            f"font-size: 14px; font-weight: 500; color: {C.TEXT};"
        )
        top.addWidget(name_label)

        if is_active:
            pill = QLabel("Active")
            pill.setStyleSheet(
                f"background: {C.GREEN}; color: white; font-size: 10px; "
                f"font-weight: 600; border-radius: 9px; padding: 3px 12px;"
            )
            pill.setFixedHeight(20)
            top.addWidget(pill)

        top.addStretch()
        root.addLayout(top)

        # --- Detail row: display name, file size, date ---
        detail_parts = []

        # Extract display name from the profile filename
        display_name = profile["stem"].replace("_", " ").replace("-", " ")
        detail_parts.append(display_name)

        # Total file size
        total_size = profile.get("cube_size", 0) + profile.get("icc_size", 0)
        if total_size > 0:
            detail_parts.append(_format_size(total_size))

        # Modification date
        mod_time = profile.get("modified")
        if mod_time:
            detail_parts.append(mod_time.strftime("%b %d, %Y  %H:%M"))

        detail = QLabel("  |  ".join(detail_parts))
        detail.setStyleSheet(f"font-size: 11px; color: {C.TEXT2};")
        root.addWidget(detail)

        # --- File paths row ---
        files_row = QVBoxLayout()
        files_row.setSpacing(4)

        if profile.get("cube_path"):
            cube_label = QLabel(
                f".cube  {_format_size(profile['cube_size'])}  \u2014  "
                f"{profile['cube_path']}"
            )
            cube_label.setStyleSheet(
                f"font-size: 10px; color: {C.TEXT3}; "
                f"font-family: 'Cascadia Code', 'Consolas', monospace;"
            )
            cube_label.setTextInteractionFlags(
                Qt.TextInteractionFlag.TextSelectableByMouse
            )
            files_row.addWidget(cube_label)

        if profile.get("icc_path"):
            icc_label = QLabel(
                f".icc  {_format_size(profile['icc_size'])}  \u2014  "
                f"{profile['icc_path']}"
            )
            icc_label.setStyleSheet(
                f"font-size: 10px; color: {C.TEXT3}; "
                f"font-family: 'Cascadia Code', 'Consolas', monospace;"
            )
            icc_label.setTextInteractionFlags(
                Qt.TextInteractionFlag.TextSelectableByMouse
            )
            files_row.addWidget(icc_label)

        if not profile.get("cube_path") and not profile.get("icc_path"):
            no_files = QLabel("No files")
            no_files.setStyleSheet(f"font-size: 10px; color: {C.TEXT3};")
            files_row.addWidget(no_files)

        root.addLayout(files_row)

        # --- Buttons row ---
        btn_row = QHBoxLayout()
        btn_row.setSpacing(8)
        btn_row.addStretch()

        self._activate_btn = QPushButton("Activate")
        self._activate_btn.setFixedHeight(30)
        self._activate_btn.setFixedWidth(90)
        self._activate_btn.setProperty("primary", True)
        self._activate_btn.setStyleSheet(
            f"QPushButton {{ background: {C.ACCENT}; color: white; "
            f"border: none; border-radius: 10px; font-size: 11px; "
            f"font-weight: 600; padding: 4px 14px; }}"
            f"QPushButton:hover {{ background: {C.ACCENT_HI}; }}"
        )
        self._activate_btn.clicked.connect(self._on_activate)
        btn_row.addWidget(self._activate_btn)

        self._export_btn = QPushButton("Export")
        self._export_btn.setFixedHeight(30)
        self._export_btn.setFixedWidth(80)
        self._export_btn.setStyleSheet(
            f"QPushButton {{ background: {C.SURFACE}; border: 1px solid {C.BORDER}; "
            f"border-radius: 10px; font-size: 11px; padding: 4px 14px; }}"
            f"QPushButton:hover {{ border-color: {C.ACCENT}; background: {C.SURFACE2}; }}"
        )
        self._export_btn.clicked.connect(self._on_export)
        btn_row.addWidget(self._export_btn)

        self._delete_btn = QPushButton("Delete")
        self._delete_btn.setFixedHeight(30)
        self._delete_btn.setFixedWidth(80)
        self._delete_btn.setStyleSheet(
            f"QPushButton {{ background: {C.SURFACE}; border: 1px solid {C.BORDER}; "
            f"border-radius: 10px; font-size: 11px; padding: 4px 14px; "
            f"color: {C.RED}; }}"
            f"QPushButton:hover {{ border-color: {C.RED}; background: {C.SURFACE2}; }}"
        )
        self._delete_btn.clicked.connect(self._on_delete)
        btn_row.addWidget(self._delete_btn)

        root.addLayout(btn_row)

    # --- Actions ---

    def _on_activate(self):
        """Activate this profile (load LUT + install ICC)."""
        try:
            cube = self._profile.get("cube_path")
            icc = self._profile.get("icc_path")

            if cube and cube.exists():
                from calibrate_pro.lut_system.dwm_lut import load_lut
                load_lut(str(cube), display_index=0)

            if icc and icc.exists():
                from calibrate_pro.panels.detection import install_profile
                install_profile(str(icc))

            QMessageBox.information(
                self, "Profile Activated",
                f"Activated: {self._profile['name']}"
            )
        except Exception as e:
            QMessageBox.warning(self, "Activation Error", str(e))

    def _on_export(self):
        """Export profile files to a chosen directory."""
        dest = QFileDialog.getExistingDirectory(self, "Export Profile To")
        if not dest:
            return
        try:
            import shutil
            dest_path = Path(dest)
            for key in ("cube_path", "icc_path"):
                src = self._profile.get(key)
                if src and src.exists():
                    shutil.copy2(str(src), str(dest_path / src.name))
            QMessageBox.information(
                self, "Exported",
                f"Profile exported to {dest_path}"
            )
        except Exception as e:
            QMessageBox.warning(self, "Export Error", str(e))

    def _on_delete(self):
        """Delete profile files after confirmation."""
        reply = QMessageBox.question(
            self, "Delete Profile",
            f"Delete '{self._profile['name']}' and its files?\n\nThis cannot be undone.",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        )
        if reply != QMessageBox.StandardButton.Yes:
            return
        try:
            for key in ("cube_path", "icc_path"):
                p = self._profile.get(key)
                if p and p.exists():
                    p.unlink()
            # Remove this card from the layout
            self.setParent(None)
            self.deleteLater()
        except Exception as e:
            QMessageBox.warning(self, "Delete Error", str(e))


# =============================================================================
# Profiles Page
# =============================================================================

class ProfilesPage(QWidget):
    """Profile management page."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._build()

    def _build(self):
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.Shape.NoFrame)
        outer.addWidget(scroll)

        self._content = QWidget()
        self._layout = QVBoxLayout(self._content)
        self._layout.setContentsMargins(32, 28, 32, 28)
        self._layout.setSpacing(20)

        # --- Header ---
        header = QHBoxLayout()
        header.addWidget(Heading("Profiles"))
        header.addStretch()

        self._generate_btn = QPushButton("Generate All")
        self._generate_btn.setFixedHeight(34)
        self._generate_btn.setProperty("primary", True)
        self._generate_btn.setStyleSheet(
            f"QPushButton {{ background: {C.ACCENT}; color: white; "
            f"border: none; border-radius: 10px; font-size: 12px; "
            f"font-weight: 600; padding: 6px 22px; }}"
            f"QPushButton:hover {{ background: {C.ACCENT_HI}; }}"
        )
        self._generate_btn.clicked.connect(self._generate_all)
        header.addWidget(self._generate_btn)

        refresh_btn = QPushButton("Refresh")
        refresh_btn.setFixedHeight(34)
        refresh_btn.clicked.connect(self._populate)
        header.addWidget(refresh_btn)

        self._layout.addLayout(header)

        # --- Cards container ---
        self._cards_layout = QVBoxLayout()
        self._cards_layout.setSpacing(12)
        self._layout.addLayout(self._cards_layout)

        self._layout.addStretch()
        scroll.setWidget(self._content)

        # Initial population
        self._populate()

    def _populate(self):
        """Scan for profiles and rebuild the card list."""
        # Clear existing cards
        while self._cards_layout.count():
            item = self._cards_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        profiles = _scan_profiles()

        if not profiles:
            self._show_empty_state()
            return

        # Determine which profile is currently active (if any)
        active_stem: Optional[str] = None
        try:
            from calibrate_pro.utils.startup_manager import StartupManager
            mgr = StartupManager()
            cal = mgr.get_display_calibration(0)
            if cal and cal.lut_path:
                active_stem = Path(cal.lut_path).stem
        except Exception:
            pass

        for profile in profiles:
            is_active = (active_stem is not None and profile["stem"] == active_stem)
            card = ProfileCard(profile, is_active=is_active)
            self._cards_layout.addWidget(card)

    def _show_empty_state(self):
        """Show a friendly message when no profiles exist."""
        card, layout = Card.with_layout()
        card.setMinimumHeight(120)

        msg = QLabel("No profiles found.")
        msg.setStyleSheet(f"font-size: 14px; color: {C.TEXT2}; font-weight: 500;")
        msg.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(msg)

        hint = QLabel(
            "Run 'Calibrate' to create your first profile."
        )
        hint.setStyleSheet(f"font-size: 12px; color: {C.TEXT3};")
        hint.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(hint)

        self._cards_layout.addWidget(card)

    def _generate_all(self):
        """Generate profiles for sRGB, P3, BT.709, and Adobe RGB."""
        reply = QMessageBox.question(
            self, "Generate All Profiles",
            "Generate calibration profiles for:\n\n"
            "  - sRGB\n"
            "  - Display P3\n"
            "  - BT.709\n"
            "  - Adobe RGB\n\n"
            "This may take a few minutes.",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        )
        if reply != QMessageBox.StandardButton.Yes:
            return

        try:
            CALIBRATIONS_DIR.mkdir(parents=True, exist_ok=True)

            from calibrate_pro.calibration.engine import CalibrationEngine
            engine = CalibrationEngine()

            for gamut in TARGET_GAMUTS:
                try:
                    engine.calibrate(target_gamut=gamut, output_dir=str(CALIBRATIONS_DIR))
                except Exception as e:
                    QMessageBox.warning(
                        self, "Generation Error",
                        f"Failed to generate {gamut} profile:\n{e}"
                    )

            self._populate()

        except Exception as e:
            QMessageBox.warning(self, "Error", str(e))
