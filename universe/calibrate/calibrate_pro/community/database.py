"""
Community Panel Database (Phase 6)

Provides panel data exchange so users can share measured panel
characterizations as portable JSON files and import profiles contributed
by the community.

Workflow:
    export-panel  -> serialise the current panel profile to JSON
    import-panel  -> load a community JSON into the local database
    submit_panel_cli() -> interactive helper for preparing a submission
"""

import json
import sys
from dataclasses import dataclass, field, asdict
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional

from calibrate_pro.panels.database import (
    PanelCharacterization,
    PanelPrimaries,
    ChromaticityCoord,
    GammaCurve,
    PanelCapabilities,
    PanelDatabase,
)


# ---------------------------------------------------------------------------
# Data model
# ---------------------------------------------------------------------------

@dataclass
class PanelSubmission:
    """Data format for community panel submissions."""
    panel_key: str
    manufacturer: str
    model: str
    panel_type: str
    primaries: Dict       # red/green/blue/white xy
    gamma: Dict           # red/green/blue values
    capabilities: Dict
    measured_by: str      # submitter name
    measurement_date: str
    measurement_device: str  # e.g., "i1Display Pro"
    notes: str = ""

    def to_dict(self) -> dict:
        """Serialise to a plain dictionary."""
        return {
            "calibrate_pro_community": True,
            "version": 1,
            "panel_key": self.panel_key,
            "manufacturer": self.manufacturer,
            "model": self.model,
            "panel_type": self.panel_type,
            "primaries": self.primaries,
            "gamma": self.gamma,
            "capabilities": self.capabilities,
            "measured_by": self.measured_by,
            "measurement_date": self.measurement_date,
            "measurement_device": self.measurement_device,
            "notes": self.notes,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "PanelSubmission":
        return cls(
            panel_key=data["panel_key"],
            manufacturer=data["manufacturer"],
            model=data["model"],
            panel_type=data["panel_type"],
            primaries=data["primaries"],
            gamma=data["gamma"],
            capabilities=data["capabilities"],
            measured_by=data.get("measured_by", "unknown"),
            measurement_date=data.get("measurement_date", ""),
            measurement_device=data.get("measurement_device", ""),
            notes=data.get("notes", ""),
        )


# ---------------------------------------------------------------------------
# Export
# ---------------------------------------------------------------------------

def export_panel(panel: PanelCharacterization, output_path: Path) -> Path:
    """
    Export a panel characterization as a shareable community JSON file.

    Parameters
    ----------
    panel : PanelCharacterization
        The panel profile to export.
    output_path : Path
        Destination file path (should end in ``.json``).

    Returns
    -------
    Path
        The path the file was written to.
    """
    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    prims = panel.native_primaries
    data = {
        "calibrate_pro_community": True,
        "version": 1,
        "panel_key": panel.model_pattern.split("|")[0],
        "manufacturer": panel.manufacturer,
        "model": panel.model_pattern,
        "panel_type": panel.panel_type,
        "display_name": panel.display_name,
        "primaries": {
            "red": {"x": prims.red.x, "y": prims.red.y},
            "green": {"x": prims.green.x, "y": prims.green.y},
            "blue": {"x": prims.blue.x, "y": prims.blue.y},
            "white": {"x": prims.white.x, "y": prims.white.y},
        },
        "gamma": {
            "red": panel.gamma_red.gamma,
            "green": panel.gamma_green.gamma,
            "blue": panel.gamma_blue.gamma,
        },
        "capabilities": {
            "max_sdr": panel.capabilities.max_luminance_sdr,
            "max_hdr": panel.capabilities.max_luminance_hdr,
            "min_luminance": panel.capabilities.min_luminance,
            "bit_depth": panel.capabilities.bit_depth,
            "hdr": panel.capabilities.hdr_capable,
            "wide_gamut": panel.capabilities.wide_gamut,
            "vrr": panel.capabilities.vrr_capable,
        },
        "notes": panel.notes,
        "exported_date": datetime.now().isoformat(),
    }

    with open(output_path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2, ensure_ascii=False)

    return output_path


# ---------------------------------------------------------------------------
# Import
# ---------------------------------------------------------------------------

def import_panel(json_path: Path) -> PanelCharacterization:
    """
    Import a panel from a community JSON file.

    Parameters
    ----------
    json_path : Path
        Path to the community JSON file.

    Returns
    -------
    PanelCharacterization
        A fully initialised panel object ready for use in calibration.

    Raises
    ------
    ValueError
        If the file is not a valid community panel JSON.
    FileNotFoundError
        If the file does not exist.
    """
    json_path = Path(json_path)
    if not json_path.exists():
        raise FileNotFoundError(f"Panel file not found: {json_path}")

    with open(json_path, "r", encoding="utf-8") as fh:
        data = json.load(fh)

    if not data.get("calibrate_pro_community"):
        raise ValueError(
            f"File does not appear to be a Calibrate Pro community panel: {json_path}"
        )

    prims = data["primaries"]
    gamma = data.get("gamma", {})
    caps = data.get("capabilities", {})

    panel = PanelCharacterization(
        manufacturer=data.get("manufacturer", "Community"),
        model_pattern=data.get("model", data.get("panel_key", "unknown")),
        panel_type=data.get("panel_type", "unknown"),
        display_name=data.get("display_name", ""),
        native_primaries=PanelPrimaries(
            red=ChromaticityCoord(prims["red"]["x"], prims["red"]["y"]),
            green=ChromaticityCoord(prims["green"]["x"], prims["green"]["y"]),
            blue=ChromaticityCoord(prims["blue"]["x"], prims["blue"]["y"]),
            white=ChromaticityCoord(prims["white"]["x"], prims["white"]["y"]),
        ),
        gamma_red=GammaCurve(gamma=gamma.get("red", 2.2)),
        gamma_green=GammaCurve(gamma=gamma.get("green", 2.2)),
        gamma_blue=GammaCurve(gamma=gamma.get("blue", 2.2)),
        capabilities=PanelCapabilities(
            max_luminance_sdr=caps.get("max_sdr", 100.0),
            max_luminance_hdr=caps.get("max_hdr", 400.0),
            min_luminance=caps.get("min_luminance", 0.0001),
            bit_depth=caps.get("bit_depth", 10),
            hdr_capable=caps.get("hdr", False),
            wide_gamut=caps.get("wide_gamut", False),
            vrr_capable=caps.get("vrr", False),
        ),
        notes=data.get("notes", ""),
    )

    return panel


# ---------------------------------------------------------------------------
# Interactive submission helper
# ---------------------------------------------------------------------------

def submit_panel_cli():
    """Interactive CLI for submitting panel data."""
    print("\n--- Community Panel Submission ---\n")

    panel_key = input("Panel key (e.g., PG27UCDM): ").strip()
    manufacturer = input("Manufacturer (e.g., ASUS): ").strip()
    model = input("Model name / regex (e.g., PG27UCDM|ROG.*PG27): ").strip() or panel_key
    panel_type = input("Panel type (QD-OLED / WOLED / IPS / VA): ").strip()

    print("\nPrimaries (CIE 1931 xy chromaticity):")
    def _read_xy(label: str) -> Dict[str, float]:
        raw = input(f"  {label} (x y): ").strip().split()
        return {"x": float(raw[0]), "y": float(raw[1])}

    primaries = {
        "red": _read_xy("Red"),
        "green": _read_xy("Green"),
        "blue": _read_xy("Blue"),
        "white": _read_xy("White"),
    }

    print("\nGamma (per-channel measured gamma values):")
    gamma = {
        "red": float(input("  Red gamma: ").strip()),
        "green": float(input("  Green gamma: ").strip()),
        "blue": float(input("  Blue gamma: ").strip()),
    }

    print("\nCapabilities:")
    capabilities: Dict = {}
    capabilities["max_sdr"] = float(input("  SDR peak (cd/m2): ").strip() or "100")
    capabilities["max_hdr"] = float(input("  HDR peak (cd/m2): ").strip() or "400")
    capabilities["hdr"] = input("  HDR capable? (y/n): ").strip().lower() == "y"
    capabilities["wide_gamut"] = input("  Wide gamut? (y/n): ").strip().lower() == "y"
    capabilities["vrr"] = input("  VRR capable? (y/n): ").strip().lower() == "y"
    capabilities["bit_depth"] = int(input("  Bit depth (8/10/12): ").strip() or "10")

    measured_by = input("\nYour name: ").strip()
    measurement_device = input("Measurement device (e.g., i1Display Pro): ").strip()
    notes = input("Notes (optional): ").strip()

    submission = PanelSubmission(
        panel_key=panel_key,
        manufacturer=manufacturer,
        model=model,
        panel_type=panel_type,
        primaries=primaries,
        gamma=gamma,
        capabilities=capabilities,
        measured_by=measured_by,
        measurement_date=datetime.now().strftime("%Y-%m-%d"),
        measurement_device=measurement_device,
        notes=notes,
    )

    filename = f"{panel_key}_community.json"
    out_path = Path(filename)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(submission.to_dict(), fh, indent=2, ensure_ascii=False)

    print(f"\nSubmission saved to: {out_path.absolute()}")
    print("Share this file with the Calibrate Pro community!")

    return submission


# ---------------------------------------------------------------------------
# CLI command handlers
# ---------------------------------------------------------------------------

def cmd_export_panel(args) -> int:
    """CLI handler for the ``export-panel`` subcommand."""
    from calibrate_pro import __version__
    from calibrate_pro.panels.detection import enumerate_displays, identify_display

    print(f"\nCalibrate Pro v{__version__} - Export Panel Profile")
    print("=" * 60)

    db = PanelDatabase()

    # Find the panel for the requested display (or primary)
    displays = enumerate_displays()
    if not displays:
        print("Error: No displays detected")
        return 1

    display_index = (args.display - 1) if hasattr(args, "display") and args.display else 0
    if display_index >= len(displays):
        display_index = 0

    display = displays[display_index]
    panel_key = identify_display(display)
    panel = db.get_panel(panel_key) if panel_key else None

    if panel is None:
        model_string = display.monitor_name or display.model or ""
        panel = db.find_panel(model_string)
    if panel is None:
        panel = db.get_fallback()

    # Determine output path
    if hasattr(args, "output") and args.output:
        out_path = Path(args.output)
    else:
        safe_name = panel.name.replace(" ", "_").replace("/", "_")
        out_path = Path(f"{safe_name}_community.json")

    result_path = export_panel(panel, out_path)

    print(f"\n  Panel: {panel.name}")
    print(f"  Type:  {panel.panel_type}")
    print(f"  File:  {result_path.absolute()}")
    print(f"\nShare this file with the Calibrate Pro community!")
    print("=" * 60)

    return 0


def cmd_import_panel(args) -> int:
    """CLI handler for the ``import-panel`` subcommand."""
    from calibrate_pro import __version__

    print(f"\nCalibrate Pro v{__version__} - Import Community Panel")
    print("=" * 60)

    json_path = Path(args.file)
    if not json_path.exists():
        print(f"Error: File not found: {json_path}")
        return 1

    try:
        panel = import_panel(json_path)
    except (ValueError, KeyError, json.JSONDecodeError) as exc:
        print(f"Error: {exc}")
        return 1

    # Save into the local profiles directory
    db = PanelDatabase()
    profiles_dir = db.profiles_dir
    profiles_dir.mkdir(parents=True, exist_ok=True)

    panel_key = panel.model_pattern.split("|")[0]
    dest = profiles_dir / f"{panel_key}.json"

    with open(dest, "w", encoding="utf-8") as fh:
        json.dump(panel.to_dict(), fh, indent=2, ensure_ascii=False)

    print(f"\n  Imported: {panel.name}")
    print(f"  Type:     {panel.panel_type}")
    print(f"  Key:      {panel_key}")
    print(f"  Saved to: {dest}")
    print(f"\nThe panel is now available in the local database.")
    print("=" * 60)

    return 0
