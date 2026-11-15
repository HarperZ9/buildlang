"""
Calibrate Pro - Professional Display Calibration Suite

Production-ready application with:
- Display calibration (sensorless and hardware)
- 3D LUT generation and system-wide application
- ICC profile creation with VCGT
- Background color loader service
- Multi-display support

This is the main entry point for the compiled executable.
"""

import argparse
import sys
import os
import time
import threading
from pathlib import Path
from typing import Optional, List, Dict
import json

# Ensure we can find our modules
if getattr(sys, 'frozen', False):
    # Running as compiled executable
    APP_DIR = Path(sys.executable).parent
else:
    # Running as script
    APP_DIR = Path(__file__).parent.parent

# Version
__version__ = "1.0.0"
__app_name__ = "Calibrate Pro"


def get_banner():
    """Get application banner."""
    return f"""
======================================================================
                      CALIBRATE PRO v{__version__}
            Professional Display Calibration Suite
======================================================================
"""


def cmd_detect(args) -> int:
    """Detect and list connected displays."""
    from calibrate_pro.panels.detection import enumerate_displays

    print(get_banner())
    displays = enumerate_displays()

    if not displays:
        print("No displays detected.")
        return 1

    print(f"Found {len(displays)} display(s):\n")

    for i, display in enumerate(displays):
        primary = " (Primary)" if display.is_primary else ""
        print(f"Display {i + 1}{primary}:")
        print(f"  Device: {display.device_name}")
        print(f"  Adapter: {display.device_string}")
        print(f"  Monitor: {display.monitor_name}")
        print(f"  Resolution: {display.width}x{display.height} @ {display.refresh_rate}Hz")
        print()

    return 0


def cmd_calibrate(args) -> int:
    """Calibrate a display and generate ICC profile + 3D LUT."""
    from calibrate_pro.panels.detection import enumerate_displays, get_display_by_number
    from calibrate_pro.core.calibration_engine import CalibrationEngine, CalibrationMode
    from calibrate_pro.lut_system.color_loader import get_color_loader

    print(get_banner())

    # Get display
    displays = enumerate_displays()
    if not displays:
        print("Error: No displays detected")
        return 1

    display = None
    if args.display:
        display = get_display_by_number(args.display)
        if not display:
            print(f"Error: Display {args.display} not found")
            return 1
    else:
        for d in displays:
            if d.is_primary:
                display = d
                break
        if not display:
            display = displays[0]

    # Determine model name
    model_string = args.model if args.model else (display.monitor_name or display.model or f"Display{display.get_display_number()}")

    print(f"Calibrating: {model_string}")
    print(f"Resolution: {display.width}x{display.height} @ {display.refresh_rate}Hz")
    if args.hdr:
        print(f"HDR Mode: ENABLED")
    print()

    # Output directory
    output_dir = Path(args.output) if args.output else Path("calibration_output")
    output_dir.mkdir(parents=True, exist_ok=True)

    # Calibration mode
    mode = CalibrationMode.SENSORLESS
    if args.mode == "colorimeter":
        mode = CalibrationMode.COLORIMETER
    elif args.mode == "hybrid":
        mode = CalibrationMode.HYBRID

    print(f"Mode: {args.mode.upper()}")
    print()

    # Progress callback
    def progress_callback(message: str, progress: float):
        bar_width = 40
        filled = int(bar_width * progress)
        bar = "=" * filled + "-" * (bar_width - filled)
        print(f"\r[{bar}] {int(progress * 100):3d}% {message}", end="", flush=True)

    # Run calibration
    engine = CalibrationEngine(mode=mode)
    engine.set_progress_callback(progress_callback)

    result = engine.calibrate(
        model_string=model_string,
        output_dir=output_dir,
        lut_size=args.lut_size,
        generate_icc=not args.no_icc,
        generate_lut=not args.no_lut,
        hdr_mode=args.hdr
    )

    print("\n")

    if result.success:
        print("Calibration Complete!")
        print(f"  Panel: {result.panel_type}")
        print(f"  Name: {result.panel_name}")
        print()
        print("Accuracy:")
        print(f"  Average Delta E: {result.delta_e_avg:.2f}")
        print(f"  Maximum Delta E: {result.delta_e_max:.2f}")
        print(f"  Grade: {result.grade}")
        print()

        if result.icc_profile_path:
            print("Generated Files:")
            print(f"  ICC Profile: {result.icc_profile_path}")
        if result.lut_path:
            print(f"  3D LUT: {result.lut_path}")

        # Save calibration for persistence
        from calibrate_pro.utils.startup_manager import get_startup_manager
        startup_mgr = get_startup_manager()
        display_id = display.get_display_number() - 1

        startup_mgr.save_display_calibration(
            display_id=display_id,
            display_name=display.monitor_name or model_string,
            model=model_string,
            lut_path=str(result.lut_path) if result.lut_path else None,
            icc_path=str(result.icc_profile_path) if result.icc_profile_path else None,
            hdr_mode=args.hdr,
            delta_e_avg=result.delta_e_avg,
            delta_e_max=result.delta_e_max
        )
        print()
        print("[SAVED] Calibration saved for auto-restore on startup.")

        # Apply LUT if requested
        if args.apply and result.lut_path:
            print()
            print("Applying calibration to display...")

            loader = get_color_loader()

            if loader.load_lut_file(display_id, str(result.lut_path)):
                loader.start()
                print("[SUCCESS] Calibration applied and color loader started!")
                print("Color correction will persist until you close this application.")
            else:
                print("[WARNING] Could not apply LUT automatically.")
                print("You can apply it manually using: calibrate_pro load-lut")

        return 0
    else:
        print(f"Calibration failed: {result.error_message}")
        return 1


def cmd_verify(args) -> int:
    """Verify calibration accuracy."""
    from calibrate_pro.panels.detection import enumerate_displays, get_display_by_number
    from calibrate_pro.verification.colorchecker import ColorCheckerVerifier

    print(get_banner())

    # Get display
    displays = enumerate_displays()
    display = None

    if args.display:
        display = get_display_by_number(args.display)
    else:
        for d in displays:
            if d.is_primary:
                display = d
                break
        if not display and displays:
            display = displays[0]

    if not display:
        print("Error: No display found")
        return 1

    print(f"Verifying: {display.monitor_name}\n")

    verifier = ColorCheckerVerifier()
    result = verifier.verify(display)

    print("ColorChecker Verification Results:")
    print("-" * 60)

    for patch in result.patches:
        status = "[PASS]" if patch.delta_e < 2.0 else "[WARN]" if patch.delta_e < 3.0 else "[FAIL]"
        print(f"  {patch.name:20s} Delta E: {patch.delta_e:5.2f}  {status}")

    print("-" * 60)
    print()
    print(f"Average Delta E: {result.average_delta_e:.2f}")
    print(f"Maximum Delta E: {result.max_delta_e:.2f}")
    print(f"Grade: {result.grade}")

    return 0


def cmd_load_lut(args) -> int:
    """Load a 3D LUT or ICC profile and apply to display."""
    from calibrate_pro.lut_system.color_loader import get_color_loader

    print(get_banner())

    loader = get_color_loader()
    displays = loader.enumerate_displays()

    if not displays:
        print("Error: No displays detected")
        return 1

    # Determine display ID
    display_id = args.display - 1 if args.display else 0

    if display_id >= len(displays):
        print(f"Error: Display {args.display} not found")
        return 1

    display = displays[display_id]
    file_path = Path(args.file)

    if not file_path.exists():
        print(f"Error: File not found: {file_path}")
        return 1

    print(f"Loading calibration for: {display['monitor']}")
    print(f"File: {file_path}")
    print()

    # Determine file type and load
    success = False
    if file_path.suffix.lower() in ['.icc', '.icm']:
        print("Loading ICC profile...")
        success = loader.load_icc_profile(display_id, str(file_path))
    elif file_path.suffix.lower() in ['.cube', '.3dl', '.mga']:
        print("Loading 3D LUT...")
        success = loader.load_lut_file(display_id, str(file_path))
    else:
        print(f"Error: Unsupported file type: {file_path.suffix}")
        return 1

    if success:
        print("[SUCCESS] Calibration loaded!")

        if args.persist:
            loader.start()
            print()
            print("Color loader service started.")
            print("Calibration will be maintained in the background.")
            print("Press Ctrl+C to stop.")

            try:
                while True:
                    time.sleep(1)
            except KeyboardInterrupt:
                print("\nStopping color loader...")
                loader.stop()

        return 0
    else:
        print("[FAILED] Could not load calibration")
        return 1


def cmd_start_service(args) -> int:
    """Start the color loader background service."""
    from calibrate_pro.lut_system.color_loader import get_color_loader
    from calibrate_pro.utils.startup_manager import get_startup_manager

    silent = getattr(args, 'silent', False)

    if not silent:
        print(get_banner())

    # Load saved calibrations from startup manager
    startup_mgr = get_startup_manager()
    loader = get_color_loader()

    # First, try to restore saved calibrations
    saved_cals = startup_mgr.get_all_calibrations()
    if saved_cals:
        if not silent:
            print("Restoring saved calibrations...")

        for display_id_str, cal_state in saved_cals.items():
            display_id = int(display_id_str)
            if cal_state.lut_path and Path(cal_state.lut_path).exists():
                loader.load_lut_file(display_id, cal_state.lut_path)
                if not silent:
                    print(f"  Display {display_id + 1}: Loaded {Path(cal_state.lut_path).name}")
            elif cal_state.icc_path and Path(cal_state.icc_path).exists():
                loader.load_icc_profile(display_id, cal_state.icc_path)
                if not silent:
                    print(f"  Display {display_id + 1}: Loaded {Path(cal_state.icc_path).name}")

    status = loader.get_status()

    if not status['calibrations']:
        if not silent:
            print("No calibrations configured.")
            print("Use 'calibrate' or 'load-lut' first to configure calibration.")
        return 1

    if not silent:
        print()
        print("Starting color loader service...")
        print()
        print("Configured displays:")
        for display_id, cal in status['calibrations'].items():
            print(f"  Display {int(display_id) + 1}: {cal['name']}")
            if cal['lut']:
                print(f"    LUT: {cal['lut']}")
            if cal['icc']:
                print(f"    ICC: {cal['icc']}")

        print()

    loader.start()

    if not silent:
        print("[RUNNING] Color loader service active")
        print("Press Ctrl+C to stop.")

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        if not silent:
            print("\nStopping...")
        loader.stop()
        if not silent:
            print("Service stopped.")

    return 0


def cmd_stop_service(args) -> int:
    """Stop the color loader service and reset displays."""
    from calibrate_pro.lut_system.color_loader import get_color_loader

    print(get_banner())

    loader = get_color_loader()
    loader.stop()

    print("Resetting displays to default gamma...")
    results = loader.reset_all()

    for display_id, success in results.items():
        status = "OK" if success else "FAILED"
        print(f"  Display {display_id + 1}: {status}")

    print()
    print("Color loader service stopped.")

    return 0


def cmd_list_panels(args) -> int:
    """List supported panel profiles."""
    from calibrate_pro.panels.database import PanelDatabase

    print(get_banner())

    db = PanelDatabase()
    panels = db.list_panels()

    print("Supported Panel Profiles:\n")

    for key, panel in panels.items():
        print(f"  {key:20s} - {panel.manufacturer} {panel.model} ({panel.panel_type})")

    print(f"\nTotal: {len(panels)} profiles")
    print("\nUse 'info <panel_key>' for detailed information")

    return 0


def cmd_info(args) -> int:
    """Show detailed panel information."""
    from calibrate_pro.panels.database import PanelDatabase

    print(get_banner())

    db = PanelDatabase()
    panel = db.get_panel(args.panel)

    if not panel:
        print(f"Panel not found: {args.panel}")
        print("\nUse 'list-panels' to see available panels")
        return 1

    print(f"Panel Information: {args.panel}")
    print("-" * 50)
    print(f"  Manufacturer: {panel.manufacturer}")
    print(f"  Model: {panel.model}")
    print(f"  Type: {panel.panel_type}")
    print()
    print("  Native Primaries:")
    print(f"    Red:   ({panel.red_primary[0]:.4f}, {panel.red_primary[1]:.4f})")
    print(f"    Green: ({panel.green_primary[0]:.4f}, {panel.green_primary[1]:.4f})")
    print(f"    Blue:  ({panel.blue_primary[0]:.4f}, {panel.blue_primary[1]:.4f})")
    print(f"    White: ({panel.white_point[0]:.4f}, {panel.white_point[1]:.4f})")
    print()
    print("  Gamma:")
    print(f"    Red:   {panel.gamma_red:.4f}")
    print(f"    Green: {panel.gamma_green:.4f}")
    print(f"    Blue:  {panel.gamma_blue:.4f}")
    print()
    print("  Capabilities:")
    print(f"    SDR Peak: {panel.sdr_peak_luminance} cd/m2")
    print(f"    HDR Peak: {panel.hdr_peak_luminance} cd/m2")
    print(f"    HDR: {'Yes' if panel.hdr_capable else 'No'}")
    print(f"    Wide Gamut: {'Yes' if panel.wide_gamut else 'No'}")
    print(f"    VRR: {'Yes' if panel.vrr_capable else 'No'}")

    if panel.notes:
        print()
        print(f"  Notes: {panel.notes}")

    return 0


def cmd_status(args) -> int:
    """Show current calibration status."""
    from calibrate_pro.lut_system.color_loader import get_color_loader

    print(get_banner())

    loader = get_color_loader()
    displays = loader.enumerate_displays()
    status = loader.get_status()

    print(f"Service Status: {status['status'].upper()}\n")

    print("Displays:")
    for display in displays:
        display_id = display['id']
        cal = status['calibrations'].get(str(display_id))

        primary = " (Primary)" if display['primary'] else ""
        print(f"\n  Display {display_id + 1}{primary}: {display['monitor']}")

        if cal:
            print(f"    Status: CALIBRATED")
            if cal['lut']:
                print(f"    LUT: {Path(cal['lut']).name}")
            if cal['icc']:
                print(f"    ICC: {Path(cal['icc']).name}")
            if cal['last_applied']:
                age = time.time() - cal['last_applied']
                print(f"    Last Applied: {age:.0f}s ago")
        else:
            print(f"    Status: Not calibrated")

    return 0


def cmd_enable_startup(args) -> int:
    """Enable auto-start at Windows startup."""
    from calibrate_pro.utils.startup_manager import get_startup_manager

    print(get_banner())

    startup_mgr = get_startup_manager()

    if startup_mgr.is_startup_enabled():
        print("Auto-start is already enabled.")
        return 0

    if startup_mgr.enable_startup(silent=True):
        print("[SUCCESS] Auto-start enabled!")
        print()
        print("Calibrate Pro will now start automatically when Windows boots.")
        print("Your display calibrations will be applied automatically.")
        return 0
    else:
        print("[FAILED] Could not enable auto-start.")
        print("Try running as administrator.")
        return 1


def cmd_disable_startup(args) -> int:
    """Disable auto-start at Windows startup."""
    from calibrate_pro.utils.startup_manager import get_startup_manager

    print(get_banner())

    startup_mgr = get_startup_manager()

    if not startup_mgr.is_startup_enabled():
        print("Auto-start is already disabled.")
        return 0

    if startup_mgr.disable_startup():
        print("[SUCCESS] Auto-start disabled.")
        print()
        print("Calibrate Pro will no longer start automatically.")
        return 0
    else:
        print("[FAILED] Could not disable auto-start.")
        return 1


def cmd_gui(args) -> int:
    """Launch the graphical user interface."""
    try:
        from calibrate_pro.gui.main_window import main as gui_main
        return gui_main()
    except ImportError as e:
        print(f"Error: GUI not available - {e}")
        print("Make sure PyQt6 is installed: pip install PyQt6")
        return 1


def create_parser() -> argparse.ArgumentParser:
    """Create command-line argument parser."""
    parser = argparse.ArgumentParser(
        prog='calibrate_pro',
        description=f'{__app_name__} v{__version__} - Professional Display Calibration',
        formatter_class=argparse.RawDescriptionHelpFormatter
    )

    parser.add_argument('--version', action='version', version=f'{__app_name__} v{__version__}')

    subparsers = parser.add_subparsers(dest='command', help='Available commands')

    # detect
    detect_parser = subparsers.add_parser('detect', help='Detect connected displays')
    detect_parser.set_defaults(func=cmd_detect)

    # calibrate
    cal_parser = subparsers.add_parser('calibrate', help='Calibrate a display')
    cal_parser.add_argument('--display', '-d', type=int, help='Display number (1-based)')
    cal_parser.add_argument('--model', type=str, help='Monitor model name (e.g., PG27UCDM, G85SB)')
    cal_parser.add_argument('--output', '-o', type=str, default='calibration_output', help='Output directory')
    cal_parser.add_argument('--mode', '-m', choices=['sensorless', 'colorimeter', 'hybrid'], default='sensorless')
    cal_parser.add_argument('--hdr', action='store_true', help='Enable HDR calibration mode')
    cal_parser.add_argument('--lut-size', type=int, choices=[17, 33, 65], default=33, help='3D LUT size')
    cal_parser.add_argument('--no-icc', action='store_true', help='Skip ICC profile generation')
    cal_parser.add_argument('--no-lut', action='store_true', help='Skip 3D LUT generation')
    cal_parser.add_argument('--apply', '-a', action='store_true', help='Apply calibration immediately')
    cal_parser.set_defaults(func=cmd_calibrate)

    # verify
    verify_parser = subparsers.add_parser('verify', help='Verify calibration accuracy')
    verify_parser.add_argument('--display', '-d', type=int, help='Display number')
    verify_parser.set_defaults(func=cmd_verify)

    # load-lut
    load_parser = subparsers.add_parser('load-lut', help='Load and apply LUT or ICC profile')
    load_parser.add_argument('file', type=str, help='Path to .cube, .icc, or .icm file')
    load_parser.add_argument('--display', '-d', type=int, default=1, help='Display number')
    load_parser.add_argument('--persist', '-p', action='store_true', help='Keep running to maintain calibration')
    load_parser.set_defaults(func=cmd_load_lut)

    # start-service
    start_parser = subparsers.add_parser('start-service', help='Start background color loader')
    start_parser.add_argument('--silent', '-s', action='store_true', help='Run silently (no output)')
    start_parser.set_defaults(func=cmd_start_service)

    # stop-service
    stop_parser = subparsers.add_parser('stop-service', help='Stop color loader and reset displays')
    stop_parser.set_defaults(func=cmd_stop_service)

    # enable-startup
    enable_startup_parser = subparsers.add_parser('enable-startup', help='Enable auto-start at Windows boot')
    enable_startup_parser.set_defaults(func=cmd_enable_startup)

    # disable-startup
    disable_startup_parser = subparsers.add_parser('disable-startup', help='Disable auto-start')
    disable_startup_parser.set_defaults(func=cmd_disable_startup)

    # list-panels
    panels_parser = subparsers.add_parser('list-panels', help='List supported panel profiles')
    panels_parser.set_defaults(func=cmd_list_panels)

    # info
    info_parser = subparsers.add_parser('info', help='Show panel information')
    info_parser.add_argument('panel', type=str, help='Panel key (e.g., PG27UCDM)')
    info_parser.set_defaults(func=cmd_info)

    # status
    status_parser = subparsers.add_parser('status', help='Show current calibration status')
    status_parser.set_defaults(func=cmd_status)

    # gui
    gui_parser = subparsers.add_parser('gui', help='Launch graphical interface')
    gui_parser.set_defaults(func=cmd_gui)

    return parser


def main() -> int:
    """Main entry point."""
    parser = create_parser()
    args = parser.parse_args()

    if args.command is None:
        # No command specified, show help
        parser.print_help()
        return 0

    try:
        return args.func(args)
    except KeyboardInterrupt:
        print("\nInterrupted.")
        return 130
    except Exception as e:
        print(f"\nError: {e}")
        if os.environ.get('DEBUG'):
            import traceback
            traceback.print_exc()
        return 1


if __name__ == '__main__':
    sys.exit(main())
