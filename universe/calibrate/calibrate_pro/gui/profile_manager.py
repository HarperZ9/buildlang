"""
Profile Manager Widget

Provides multi-select file management for ICC/ICM color profiles.
"""

import os
from pathlib import Path
from typing import List, Optional
from dataclasses import dataclass
from datetime import datetime

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QTableWidget, QTableWidgetItem,
    QPushButton, QLabel, QMessageBox, QHeaderView, QAbstractItemView,
    QCheckBox, QFrame, QGroupBox, QProgressDialog
)
from PyQt6.QtCore import Qt, QThread, pyqtSignal
from PyQt6.QtGui import QColor, QFont


@dataclass
class ProfileInfo:
    """Information about an ICC/ICM profile."""
    path: Path
    name: str
    size: int
    modified: datetime
    is_system: bool = False


class ProfileScanner(QThread):
    """Background thread to scan for profiles."""
    finished = pyqtSignal(list)
    progress = pyqtSignal(str)

    # System profiles that should not be deleted
    SYSTEM_PROFILES = {
        'srgb color space profile.icm',
        'rswop.icm',
        'wscrgb.icc',
        'wsrgb.icc',
    }

    def __init__(self):
        super().__init__()
        self.locations = [
            Path(os.environ.get('WINDIR', 'C:/Windows')) / 'System32' / 'spool' / 'drivers' / 'color',
            Path(os.environ.get('APPDATA', '')) / 'CalibratePro',
            Path(os.environ.get('LOCALAPPDATA', '')) / 'CalibratePro',
        ]

    def run(self):
        profiles = []

        for location in self.locations:
            if not location.exists():
                continue

            self.progress.emit(f"Scanning {location.name}...")

            for ext in ['*.icc', '*.icm', '*.ICC', '*.ICM']:
                for f in location.glob(ext):
                    try:
                        stat = f.stat()
                        is_system = f.name.lower() in self.SYSTEM_PROFILES

                        profiles.append(ProfileInfo(
                            path=f,
                            name=f.name,
                            size=stat.st_size,
                            modified=datetime.fromtimestamp(stat.st_mtime),
                            is_system=is_system
                        ))
                    except Exception:
                        pass

        # Sort by name
        profiles.sort(key=lambda p: p.name.lower())
        self.finished.emit(profiles)


class ProfileManagerWidget(QWidget):
    """Widget for managing ICC/ICM color profiles with multi-select."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.profiles: List[ProfileInfo] = []
        self.setup_ui()
        self.refresh_profiles()

    def setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(20, 20, 20, 20)
        layout.setSpacing(15)

        # Header
        header = QLabel("Profile Manager")
        header.setFont(QFont("Segoe UI", 16, QFont.Weight.Bold))
        header.setStyleSheet("color: #e0e0e0;")
        layout.addWidget(header)

        desc = QLabel("Manage ICC and ICM color profiles installed on your system.")
        desc.setStyleSheet("color: #888;")
        layout.addWidget(desc)

        # Toolbar
        toolbar = QHBoxLayout()

        self.select_all_btn = QPushButton("Select All")
        self.select_all_btn.clicked.connect(self.select_all)
        self.select_all_btn.setStyleSheet(self._button_style())
        toolbar.addWidget(self.select_all_btn)

        self.select_none_btn = QPushButton("Select None")
        self.select_none_btn.clicked.connect(self.select_none)
        self.select_none_btn.setStyleSheet(self._button_style())
        toolbar.addWidget(self.select_none_btn)

        self.select_custom_btn = QPushButton("Select Custom Only")
        self.select_custom_btn.clicked.connect(self.select_custom)
        self.select_custom_btn.setStyleSheet(self._button_style())
        toolbar.addWidget(self.select_custom_btn)

        toolbar.addStretch()

        self.refresh_btn = QPushButton("Refresh")
        self.refresh_btn.clicked.connect(self.refresh_profiles)
        self.refresh_btn.setStyleSheet(self._button_style())
        toolbar.addWidget(self.refresh_btn)

        self.delete_btn = QPushButton("Delete Selected")
        self.delete_btn.clicked.connect(self.delete_selected)
        self.delete_btn.setStyleSheet(self._button_style("#c0392b", "#e74c3c"))
        toolbar.addWidget(self.delete_btn)

        layout.addLayout(toolbar)

        # Table
        self.table = QTableWidget()
        self.table.setColumnCount(5)
        self.table.setHorizontalHeaderLabels(["", "Name", "Size", "Modified", "Location"])
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Fixed)
        self.table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self.table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        self.table.horizontalHeader().setSectionResizeMode(3, QHeaderView.ResizeMode.ResizeToContents)
        self.table.horizontalHeader().setSectionResizeMode(4, QHeaderView.ResizeMode.ResizeToContents)
        self.table.setColumnWidth(0, 40)
        self.table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.table.setAlternatingRowColors(True)
        self.table.verticalHeader().setVisible(False)
        self.table.setStyleSheet("""
            QTableWidget {
                background-color: #2d2d2d;
                color: #e0e0e0;
                border: 1px solid #444;
                gridline-color: #444;
            }
            QTableWidget::item {
                padding: 5px;
            }
            QTableWidget::item:selected {
                background-color: #3d5a80;
            }
            QHeaderView::section {
                background-color: #1e1e1e;
                color: #e0e0e0;
                padding: 8px;
                border: none;
                border-bottom: 1px solid #444;
            }
        """)
        layout.addWidget(self.table)

        # Status bar
        self.status_label = QLabel("")
        self.status_label.setStyleSheet("color: #888;")
        layout.addWidget(self.status_label)

    def _button_style(self, bg="#3d5a80", hover="#4a6fa5"):
        return f"""
            QPushButton {{
                background-color: {bg};
                color: white;
                border: none;
                padding: 8px 16px;
                border-radius: 4px;
                font-weight: bold;
            }}
            QPushButton:hover {{
                background-color: {hover};
            }}
            QPushButton:pressed {{
                background-color: #2c4a6e;
            }}
            QPushButton:disabled {{
                background-color: #555;
                color: #888;
            }}
        """

    def refresh_profiles(self):
        """Scan and refresh the profile list."""
        self.table.setRowCount(0)
        self.status_label.setText("Scanning for profiles...")

        self.scanner = ProfileScanner()
        self.scanner.finished.connect(self._on_scan_complete)
        self.scanner.start()

    def _on_scan_complete(self, profiles: List[ProfileInfo]):
        """Handle scan completion."""
        self.profiles = profiles
        self.table.setRowCount(len(profiles))

        for row, profile in enumerate(profiles):
            # Checkbox
            checkbox = QCheckBox()
            checkbox.setStyleSheet("QCheckBox { margin-left: 10px; }")
            if profile.is_system:
                checkbox.setEnabled(False)
                checkbox.setToolTip("System profile - cannot be deleted")
            self.table.setCellWidget(row, 0, checkbox)

            # Name
            name_item = QTableWidgetItem(profile.name)
            if profile.is_system:
                name_item.setForeground(QColor("#888"))
                name_item.setToolTip("System profile")
            self.table.setItem(row, 1, name_item)

            # Size
            size_str = self._format_size(profile.size)
            self.table.setItem(row, 2, QTableWidgetItem(size_str))

            # Modified
            mod_str = profile.modified.strftime("%Y-%m-%d %H:%M")
            self.table.setItem(row, 3, QTableWidgetItem(mod_str))

            # Location
            loc_str = str(profile.path.parent.name)
            self.table.setItem(row, 4, QTableWidgetItem(loc_str))

        custom_count = sum(1 for p in profiles if not p.is_system)
        self.status_label.setText(
            f"Found {len(profiles)} profiles ({custom_count} custom, {len(profiles) - custom_count} system)"
        )

    def _format_size(self, size: int) -> str:
        """Format file size as human-readable string."""
        if size < 1024:
            return f"{size} B"
        elif size < 1024 * 1024:
            return f"{size / 1024:.1f} KB"
        else:
            return f"{size / (1024 * 1024):.1f} MB"

    def select_all(self):
        """Select all non-system profiles."""
        for row in range(self.table.rowCount()):
            checkbox = self.table.cellWidget(row, 0)
            if checkbox and checkbox.isEnabled():
                checkbox.setChecked(True)

    def select_none(self):
        """Deselect all profiles."""
        for row in range(self.table.rowCount()):
            checkbox = self.table.cellWidget(row, 0)
            if checkbox:
                checkbox.setChecked(False)

    def select_custom(self):
        """Select only custom (non-system) profiles."""
        for row, profile in enumerate(self.profiles):
            checkbox = self.table.cellWidget(row, 0)
            if checkbox and checkbox.isEnabled():
                checkbox.setChecked(not profile.is_system)

    def get_selected_profiles(self) -> List[ProfileInfo]:
        """Get list of selected profiles."""
        selected = []
        for row, profile in enumerate(self.profiles):
            checkbox = self.table.cellWidget(row, 0)
            if checkbox and checkbox.isChecked():
                selected.append(profile)
        return selected

    def delete_selected(self):
        """Delete all selected profiles."""
        selected = self.get_selected_profiles()

        if not selected:
            QMessageBox.information(self, "No Selection", "No profiles selected for deletion.")
            return

        # Confirm
        reply = QMessageBox.question(
            self, "Confirm Deletion",
            f"Are you sure you want to delete {len(selected)} profile(s)?\n\n"
            "This action cannot be undone.",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            QMessageBox.StandardButton.No
        )

        if reply != QMessageBox.StandardButton.Yes:
            return

        # Delete
        deleted = 0
        failed = []

        progress = QProgressDialog("Deleting profiles...", "Cancel", 0, len(selected), self)
        progress.setWindowModality(Qt.WindowModality.WindowModal)
        progress.setMinimumDuration(0)

        for i, profile in enumerate(selected):
            if progress.wasCanceled():
                break

            progress.setValue(i)
            progress.setLabelText(f"Deleting {profile.name}...")

            try:
                profile.path.unlink()
                deleted += 1
            except Exception as e:
                failed.append((profile.name, str(e)))

        progress.setValue(len(selected))

        # Show results
        msg = f"Deleted {deleted} of {len(selected)} profiles."
        if failed:
            msg += f"\n\n{len(failed)} failed (may need administrator rights):"
            for name, err in failed[:5]:
                msg += f"\n  - {name}"
            if len(failed) > 5:
                msg += f"\n  ... and {len(failed) - 5} more"

        QMessageBox.information(self, "Deletion Complete", msg)

        # Refresh
        self.refresh_profiles()


class ProfileManagerPage(QWidget):
    """Full page wrapper for Profile Manager."""

    def __init__(self, parent=None):
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        self.manager = ProfileManagerWidget()
        layout.addWidget(self.manager)
