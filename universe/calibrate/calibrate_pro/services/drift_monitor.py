"""
Drift Detection and Calibration Age Tracking.

Monitors calibration freshness for all displays and flags displays
where calibration is older than a configurable threshold. Checks that
the referenced LUT and ICC files still exist on disk.
"""

from datetime import datetime, timedelta
from typing import Dict, List, Optional
from dataclasses import dataclass
from pathlib import Path

from calibrate_pro import __version__


@dataclass
class CalibrationStatus:
    """Status of calibration for one display."""
    display_index: int
    display_name: str
    is_calibrated: bool
    last_calibrated: Optional[datetime]
    age_days: float
    needs_recalibration: bool  # True if older than threshold
    lut_applied: bool
    lut_path: Optional[str]
    icc_installed: bool
    icc_path: Optional[str]


def check_calibration_status(max_age_days: int = 30) -> List[CalibrationStatus]:
    """
    Check calibration freshness for all displays.

    Reads saved calibration state from StartupManager and, for each
    display entry, computes how old the calibration is, flags it when
    stale, and verifies that the LUT/ICC files still exist on disk.

    Args:
        max_age_days: Number of days after which a calibration is
                      considered stale and re-calibration is recommended.

    Returns:
        A list of CalibrationStatus, one per saved display.
    """
    from calibrate_pro.utils.startup_manager import StartupManager

    manager = StartupManager()
    calibrations = manager.get_all_calibrations()
    now = datetime.now()
    results: List[CalibrationStatus] = []

    for key, state in calibrations.items():
        # Parse last_calibrated timestamp
        last_cal: Optional[datetime] = None
        age_days = 0.0
        if state.last_calibrated:
            try:
                last_cal = datetime.fromisoformat(state.last_calibrated)
                delta = now - last_cal
                age_days = delta.total_seconds() / 86400.0
            except (ValueError, TypeError):
                last_cal = None

        # Check file existence
        lut_exists = False
        if state.lut_path:
            lut_exists = Path(state.lut_path).exists()

        icc_exists = False
        if state.icc_path:
            icc_exists = Path(state.icc_path).exists()

        needs_recal = age_days > max_age_days if last_cal is not None else False

        results.append(CalibrationStatus(
            display_index=state.display_id,
            display_name=state.display_name or state.model or f"Display {state.display_id}",
            is_calibrated=last_cal is not None,
            last_calibrated=last_cal,
            age_days=age_days,
            needs_recalibration=needs_recal,
            lut_applied=lut_exists,
            lut_path=state.lut_path,
            icc_installed=icc_exists,
            icc_path=state.icc_path,
        ))

    return results


def any_needs_recalibration(max_age_days: int = 30) -> bool:
    """Return True if any saved display needs re-calibration."""
    statuses = check_calibration_status(max_age_days)
    return any(s.needs_recalibration for s in statuses)


def _format_age(age_days: float) -> str:
    """Return a human-friendly age string."""
    if age_days < 0.042:  # less than ~1 hour
        return "just now"
    if age_days < 1:
        hours = int(age_days * 24)
        return f"{hours} hour{'s' if hours != 1 else ''} ago"
    days = int(age_days)
    if days == 0:
        return "today"
    if days == 1:
        return "1 day ago"
    return f"{days} days ago"


def _file_status(path: Optional[str], exists: bool) -> str:
    """Format a file path with existence indicator."""
    if not path:
        return "(none)"
    tag = "exists" if exists else "MISSING"
    return f"{path} ({tag})"


def print_calibration_status(max_age_days: int = 30) -> None:
    """
    Print calibration status for all displays.

    Output format per display::

        ASUS PG27UCDM:
          Calibrated: 2024-03-19 (today)
          LUT: C:\\Users\\...\\ASUS_PG27UCDM.cube (exists)
          ICC: C:\\Users\\...\\ASUS_PG27UCDM.icc (exists)
          Status: Current

    or::

        Samsung Odyssey G7:
          Calibrated: 2024-02-15 (32 days ago)
          Status: Re-calibration recommended (>30 days)
    """
    statuses = check_calibration_status(max_age_days)

    print(f"\nCalibrate Pro v{__version__} - Calibration Status")
    print("=" * 56)

    if not statuses:
        print("\n  No saved calibrations found.")
        print("  Run 'calibrate-pro auto' to calibrate your displays.\n")
        return

    stale_count = 0

    for s in statuses:
        print(f"\n  {s.display_name}:")

        if s.is_calibrated and s.last_calibrated is not None:
            date_str = s.last_calibrated.strftime("%Y-%m-%d")
            age_str = _format_age(s.age_days)
            print(f"    Calibrated: {date_str} ({age_str})")
        else:
            print(f"    Calibrated: never")

        # LUT line
        print(f"    LUT: {_file_status(s.lut_path, s.lut_applied)}")

        # ICC line
        print(f"    ICC: {_file_status(s.icc_path, s.icc_installed)}")

        # Status line
        if not s.is_calibrated:
            print(f"    Status: Not calibrated")
        elif s.needs_recalibration:
            stale_count += 1
            print(f"    Status: Re-calibration recommended (>{max_age_days} days)")
        else:
            print(f"    Status: Current")

    print()

    if stale_count > 0:
        print(f"  WARNING: {stale_count} display(s) may need re-calibration.")
        print(f"  Run 'calibrate-pro auto' to refresh.\n")
