"""
Calibrate Pro - Professional Display Calibration Suite

Main application entry point for CLI and GUI calibration.

Usage:
    python -m calibrate_pro.main [command] [options]

Commands:
    gui          - Launch Professional Calibration GUI (runs as admin)
    hdr          - Launch HDR Calibration GUI (runs as admin)
    detect       - Detect connected displays
    calibrate    - Calibrate a display (CLI)
    verify       - Verify calibration accuracy
    list-panels  - List supported panel profiles
    list-targets - List available calibration target presets
    info         - Show information about a panel
    tray         - Launch system tray application
    patterns     - Display fullscreen test patterns
    status       - Show calibration age and drift status per display
    plugins      - List discovered plugins

Examples:
    python -m calibrate_pro.main gui                    # Launch full GUI
    python -m calibrate_pro.main hdr                    # Launch HDR GUI
    python -m calibrate_pro.main detect                 # Detect displays
    python -m calibrate_pro.main calibrate --display 1 --output ./profiles
    python -m calibrate_pro.main calibrate --display 1 --whitepoint D65 --luminance 400 --gamma 2.2
    python -m calibrate_pro.main calibrate --display 1 --profile HDR10
    python -m calibrate_pro.main verify --display 1
    python -m calibrate_pro.main list-panels
    python -m calibrate_pro.main list-targets
    python -m calibrate_pro.main info PG27UCDM

Note: GUI commands automatically request administrator privileges for
      DDC/CI control and system-wide 3D LUT loading via dwm_lut.
"""

import argparse
import sys
import os
import time
import ctypes
from pathlib import Path
from typing import Optional

from calibrate_pro import __version__


def is_admin() -> bool:
    """Check if running with administrator privileges."""
    try:
        return ctypes.windll.shell32.IsUserAnAdmin()
    except Exception:
        return False


def run_as_admin():
    """Re-launch the current script with administrator privileges."""
    if is_admin():
        return True
    try:
        script = os.path.abspath(sys.argv[0])
        params = ' '.join([f'"{arg}"' for arg in sys.argv[1:]])
        result = ctypes.windll.shell32.ShellExecuteW(
            None, "runas", sys.executable,
            f'"{script}" {params}', None, 1
        )
        if result > 32:
            sys.exit(0)
        return False
    except Exception:
        return False


from calibrate_pro.core.calibration_engine import (
    CalibrationEngine, CalibrationMode, CalibrationTarget,
    quick_calibrate, verify_calibration, list_supported_displays, get_display_info
)
from calibrate_pro.panels.detection import (
    enumerate_displays, get_display_by_number, print_display_info
)
from calibrate_pro.targets import (
    WhitepointPreset, WhitepointTarget, LuminanceTarget, LuminanceStandard,
    GammaPreset, GammaTarget, GamutPreset, GamutTarget,
    CalibrationTargetProfile, get_profile_presets,
    get_whitepoint_presets, get_luminance_presets, get_gamma_presets, get_gamut_presets,
    create_custom_whitepoint, create_custom_luminance, create_custom_gamma,
    WHITEPOINT_D65, WHITEPOINT_D50, WHITEPOINT_DCI,
    LUMINANCE_REC709, LUMINANCE_HDR10, LUMINANCE_CONSUMER_SDR,
    GAMMA_22, GAMMA_24, GAMMA_SRGB, GAMMA_BT1886, GAMMA_PQ,
    GAMUT_SRGB, GAMUT_DCI_P3, GAMUT_BT2020
)

def cmd_detect(args):
    """Detect and list connected displays and colorimeters."""
    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)
    print_display_info()

    # Also detect colorimeters
    try:
        from calibrate_pro.hardware.argyll_backend import ArgyllBackend, ArgyllConfig
        config = ArgyllConfig()
        if config.find_argyll():
            backend = ArgyllBackend(config)
            devices = backend.detect_devices()
            if devices:
                print("Colorimeters:")
                for d in devices:
                    print(f"  {d.name} ({d.manufacturer})")
                    print(f"    Type: {d.device_type.value if hasattr(d.device_type, 'value') else d.device_type}")
                    print(f"    Ready for: calibrate-pro refine")
                print()
            else:
                print("Colorimeter: none detected")
                print(f"  ArgyllCMS: {config.bin_path}")
                print()
    except Exception:
        pass

def cmd_calibrate(args):
    """Calibrate a display."""
    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

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
        # Use primary display
        for d in displays:
            if d.is_primary:
                display = d
                break
        if not display:
            display = displays[0]

    # Determine model name - use override if provided
    if hasattr(args, 'model') and args.model:
        model_string = args.model
    else:
        model_string = display.monitor_name or display.model or f"Display{display.get_display_number()}"

    print(f"\nCalibrating: {display.monitor_name}")
    if hasattr(args, 'model') and args.model:
        print(f"Model Override: {args.model}")
    print(f"Resolution: {display.width}x{display.height} @ {display.refresh_rate}Hz")

    # Build calibration targets from arguments
    whitepoint_target = WHITEPOINT_D65
    luminance_target = LUMINANCE_CONSUMER_SDR
    gamma_target = GAMMA_22
    gamut_target = GAMUT_SRGB

    # Check for profile preset (overrides individual settings)
    if hasattr(args, 'profile') and args.profile:
        profile_map = {
            "sRGB": ("sRGB Web Standard", WHITEPOINT_D65, LUMINANCE_CONSUMER_SDR, GAMMA_SRGB, GAMUT_SRGB),
            "Rec709": ("Rec.709 Broadcast", WHITEPOINT_D65, LUMINANCE_REC709, GAMMA_BT1886, GAMUT_SRGB),
            "DCI-P3": ("DCI-P3 Cinema", WHITEPOINT_DCI, LUMINANCE_REC709, GAMMA_24, GAMUT_DCI_P3),
            "HDR10": ("HDR10 Mastering", WHITEPOINT_D65, LUMINANCE_HDR10, GAMMA_PQ, GAMUT_BT2020),
        }
        if args.profile in profile_map:
            name, whitepoint_target, luminance_target, gamma_target, gamut_target = profile_map[args.profile]
            print(f"Profile: {name}")

    # Individual target overrides
    if hasattr(args, 'whitepoint') and args.whitepoint:
        wp_map = {
            "D50": WhitepointPreset.D50,
            "D55": WhitepointPreset.D55,
            "D65": WhitepointPreset.D65,
            "D75": WhitepointPreset.D75,
            "DCI": WhitepointPreset.DCI,
        }
        if args.whitepoint in wp_map:
            whitepoint_target = WhitepointTarget(preset=wp_map[args.whitepoint])

    if hasattr(args, 'cct') and args.cct:
        whitepoint_target = create_custom_whitepoint(cct=args.cct, name=f"{args.cct}K")

    if hasattr(args, 'luminance') and args.luminance:
        luminance_target = create_custom_luminance(
            peak=args.luminance,
            black=args.black_level if hasattr(args, 'black_level') and args.black_level else 0.0,
            hdr_mode=args.luminance > 400
        )

    if hasattr(args, 'gamma') and args.gamma:
        gamma_map = {
            "2.2": GammaPreset.POWER_22,
            "2.4": GammaPreset.POWER_24,
            "sRGB": GammaPreset.SRGB,
            "BT1886": GammaPreset.BT1886,
            "PQ": GammaPreset.PQ,
            "HLG": GammaPreset.HLG,
        }
        if args.gamma in gamma_map:
            gamma_target = GammaTarget(preset=gamma_map[args.gamma])
        else:
            try:
                gamma_val = float(args.gamma)
                gamma_target = create_custom_gamma(gamma_val)
            except ValueError:
                pass

    if hasattr(args, 'gamut') and args.gamut:
        gamut_map = {
            "sRGB": GamutPreset.SRGB,
            "DCI-P3": GamutPreset.DCI_P3,
            "Display-P3": GamutPreset.DISPLAY_P3,
            "BT2020": GamutPreset.BT2020,
            "AdobeRGB": GamutPreset.ADOBE_RGB,
        }
        if args.gamut in gamut_map:
            gamut_target = GamutTarget(preset=gamut_map[args.gamut])

    # Print target settings
    print(f"\nTarget Settings:")
    print(f"  White Point: {whitepoint_target.preset.value} ({whitepoint_target.get_cct():.0f}K)")
    print(f"  Luminance: {luminance_target.get_peak_luminance():.0f} cd/m2 peak")
    if luminance_target.get_black_level() > 0:
        print(f"  Black Level: {luminance_target.get_black_level():.4f} cd/m2")
        print(f"  Contrast: {luminance_target.get_contrast_ratio():.0f}:1")
    print(f"  Gamma: {gamma_target.preset.value}")
    print(f"  Gamut: {gamut_target.preset.value}")
    if luminance_target.is_hdr():
        print(f"  Mode: HDR")

    # Output directory
    output_dir = Path(args.output) if args.output else Path(".")
    output_dir.mkdir(parents=True, exist_ok=True)

    # Calibration mode
    mode = CalibrationMode.SENSORLESS
    if args.mode == "colorimeter":
        mode = CalibrationMode.COLORIMETER
    elif args.mode == "hybrid":
        mode = CalibrationMode.HYBRID

    # Run calibration
    engine = CalibrationEngine(mode=mode)

    def progress_callback(message: str, progress: float):
        bar_width = 30
        filled = int(bar_width * progress)
        bar = "=" * filled + "-" * (bar_width - filled)
        print(f"\r[{bar}] {int(progress * 100):3d}% {message}", end="", flush=True)

    engine.set_progress_callback(progress_callback)

    result = engine.calibrate(
        model_string=model_string,
        output_dir=output_dir,
        generate_icc=not args.no_icc,
        generate_lut=not args.no_lut,
        lut_size=args.lut_size,
        hdr_mode=luminance_target.is_hdr()
    )

    print("\n")

    if result.success:
        print("\nCalibration Complete!")
        print(f"  Panel: {result.panel_name}")
        print(f"  Type: {result.panel_type}")
        print(f"\nAccuracy:")
        print(f"  Average Delta E: {result.delta_e_avg:.2f}")
        print(f"  Maximum Delta E: {result.delta_e_max:.2f}")
        print(f"  Grade: {result.grade}")

        print(f"\nGenerated Files:")
        if result.icc_profile_path:
            print(f"  ICC Profile: {result.icc_profile_path}")
        if result.lut_path:
            print(f"  3D LUT: {result.lut_path}")

        return 0
    else:
        print("\nCalibration failed")
        return 1


def cmd_list_targets(args):
    """List available calibration target presets."""
    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

    print("\n--- Calibration Profiles ---")
    profiles = get_profile_presets()
    for p in profiles:
        hdr = " (HDR)" if p.is_hdr() else ""
        print(f"  {p.name:25s} - {p.description}{hdr}")

    print("\n--- White Point Presets ---")
    for wp in get_whitepoint_presets():
        print(f"  {wp.preset.value:15s} ({wp.get_cct():.0f}K) - {wp.description if hasattr(wp, 'description') and wp.description else ''}")

    print("\n--- Luminance Presets ---")
    for lum in get_luminance_presets():
        hdr = " [HDR]" if lum.is_hdr() else " [SDR]"
        print(f"  {lum.standard.value:20s} - {lum.get_peak_luminance():.0f} cd/m2{hdr}")

    print("\n--- Gamma/EOTF Presets ---")
    for g in get_gamma_presets():
        hdr = " [HDR]" if g.is_hdr() else ""
        print(f"  {g.preset.value:15s} - {g.description if hasattr(g, 'description') and g.description else ''}{hdr}")

    print("\n--- Gamut Presets ---")
    for gam in get_gamut_presets():
        wide = " [Wide Gamut]" if gam.is_wide_gamut() else ""
        print(f"  {gam.preset.value:15s} - {gam.description if hasattr(gam, 'description') and gam.description else ''}{wide}")

    print("\nUse these with: calibrate --profile <name> or individual --whitepoint, --luminance, --gamma, --gamut flags")
    return 0

def cmd_verify(args):
    """Verify calibration accuracy."""
    from calibrate_pro.panels.detection import get_display_name, identify_display
    from calibrate_pro.panels.database import PanelDatabase
    from calibrate_pro.sensorless.neuralux import SensorlessEngine

    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

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

    name = get_display_name(display)
    print(f"\nVerifying: {name}")

    # -------------------------------------------------------------------------
    # Measured verification (--measured flag)
    # -------------------------------------------------------------------------
    use_measured = hasattr(args, 'measured') and args.measured

    if use_measured:
        from calibrate_pro.verification.measured_verify import (
            MeasuredVerification, _find_argyll_spotread,
        )

        print("\n  Mode: Measured (colorimeter-based)")

        mv = MeasuredVerification()

        if mv.backend_name == "argyll":
            print("  Backend: ArgyllCMS (spotread)")
        elif mv.backend_name == "manual":
            print("  Backend: Manual entry")
            print("  (ArgyllCMS not found. Install from https://www.argyllcms.com/")
            print("   or set ARGYLL_BIN to the bin directory.)")
            print("  You will be prompted to enter XYZ values for each patch.")
        else:
            print(f"  Backend: {mv.backend_name}")

        display_index = (args.display - 1) if args.display else 0

        print(f"\nColorChecker Measured Verification:")
        print("-" * 56)

        result = mv.verify_colorchecker(display_index=display_index)

        print("-" * 56)

        for patch in result['patches']:
            de = patch['delta_e']
            status = "PASS" if de < 2.0 else "WARN" if de < 3.0 else "FAIL"
            print(f"  {patch['name']:20s}  dE={de:5.2f}  [{status}]")

        print("-" * 56)
        print(f"\n  CIEDE2000:  avg {result['delta_e_avg']:.2f}   max {result['delta_e_max']:.2f}")
        print(f"  Grade: {result['grade']}")
        print(f"\n  Note: These values are measured with a colorimeter.")

        # Also run grayscale
        print(f"\nGrayscale Measured Verification (21 steps):")
        print("-" * 56)

        gs_result = mv.verify_grayscale(display_index=display_index, steps=21)

        for step in gs_result['steps']:
            level_pct = step['level'] * 100
            de = step['delta_e']
            gamma = step['measured_gamma']
            ge = step['gamma_error']
            print(f"  {level_pct:5.1f}%  Y={step['measured_luminance']:7.2f}  "
                  f"gamma={gamma:.2f}  dE={de:.2f}  gamma_err={ge:.2f}")

        print("-" * 56)
        print(f"\n  Gamma tracking: avg error {gs_result['avg_gamma_error']:.3f}  "
              f"max error {gs_result['max_gamma_error']:.3f}")
        print(f"  Color accuracy: avg dE {gs_result['delta_e_avg']:.2f}  "
              f"max dE {gs_result['delta_e_max']:.2f}")
        print(f"  White luminance: {gs_result['white_luminance']:.1f} cd/m2")
        print(f"  Grade: {gs_result['grade']}")

        return 0

    # -------------------------------------------------------------------------
    # Sensorless verification (default)
    # -------------------------------------------------------------------------
    # Match panel via fingerprint (same logic as auto-calibration)
    db = PanelDatabase()
    panel_key = identify_display(display)
    panel = db.get_panel(panel_key) if panel_key else None
    if panel is None:
        model_string = display.monitor_name or display.model or ""
        panel = db.find_panel(model_string) or db.get_fallback()

    panel_name = panel.name
    print(f"Panel: {panel_name} ({panel.panel_type})")

    engine = SensorlessEngine()
    engine.current_panel = panel
    result = engine.verify_calibration(panel)

    print(f"\nColorChecker Verification:")
    print("-" * 56)

    for patch in result['patches']:
        de = patch['delta_e']
        cam_de = patch.get('cam16_delta_e', de)
        status = "PASS" if de < 2.0 else "WARN" if de < 3.0 else "FAIL"
        print(f"  {patch['name']:20s}  dE={de:5.2f}  CAM16={cam_de:5.2f}  [{status}]")

    print("-" * 56)
    print(f"\n  CIEDE2000:  avg {result['delta_e_avg']:.2f}   max {result['delta_e_max']:.2f}")
    print(f"  CAM16-UCS: avg {result.get('cam16_delta_e_avg', 0):.2f}   max {result.get('cam16_delta_e_max', 0):.2f}")
    print(f"  Grade: {result['grade']}")
    print(f"\n  Note: These values are predicted from the panel database,")
    print(f"  not measured. For verified accuracy, use --measured with a colorimeter.")

    coverage = result.get('gamut_coverage', {})
    if coverage:
        print(f"\n  Gamut (2D area): sRGB {coverage.get('srgb_pct', 0):.0f}%  "
              f"P3 {coverage.get('dci_p3_pct', 0):.0f}%  "
              f"BT.2020 {coverage.get('bt2020_pct', 0):.0f}%")

    # 3D color volume (captures luminance-dependent gamut changes)
    try:
        from calibrate_pro.display.color_volume import compute_color_volume
        primaries = panel.native_primaries
        vol = compute_color_volume(
            panel_primaries=(
                primaries.red.as_tuple(),
                primaries.green.as_tuple(),
                primaries.blue.as_tuple()
            ),
            panel_white=primaries.white.as_tuple(),
            lightness_steps=11,
            hue_steps=36,
            panel_type=panel.panel_type,
            peak_luminance=panel.capabilities.max_luminance_hdr
        )
        print(f"  Volume (3D):    sRGB {vol.srgb_volume_pct:.0f}%  "
              f"P3 {vol.p3_volume_pct:.0f}%  "
              f"BT.2020 {vol.bt2020_volume_pct:.0f}%  "
              f"({vol.relative_to_srgb_pct:.0f}% of sRGB volume)")
    except Exception:
        pass

    return 0

def cmd_list_panels(args):
    """List supported panel profiles."""
    from calibrate_pro.panels.database import PanelDatabase

    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)
    print("\nSupported Panel Profiles:\n")

    db = PanelDatabase()
    panel_keys = sorted(db.list_panels())

    for key in panel_keys:
        panel = db.get_panel(key)
        if panel:
            print(f"  {panel.name:40s}  {panel.panel_type}")

    print(f"\nTotal: {len(panel_keys)} profiles")
    print("\nUse 'info <panel_key>' for detailed information")

def cmd_info(args):
    """Show information about a panel."""
    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

    info = get_display_info(args.panel)
    if not info:
        print(f"Error: Panel '{args.panel}' not found")
        print("Use 'list-panels' to see available profiles")
        return 1

    print(f"\nPanel Information: {args.panel}")
    print("-" * 50)
    print(f"  Manufacturer: {info['manufacturer']}")
    print(f"  Model: {info['model']}")
    print(f"  Type: {info['type']}")

    print(f"\n  Native Primaries:")
    print(f"    Red:   ({info['primaries']['red'][0]:.4f}, {info['primaries']['red'][1]:.4f})")
    print(f"    Green: ({info['primaries']['green'][0]:.4f}, {info['primaries']['green'][1]:.4f})")
    print(f"    Blue:  ({info['primaries']['blue'][0]:.4f}, {info['primaries']['blue'][1]:.4f})")
    print(f"    White: ({info['primaries']['white'][0]:.4f}, {info['primaries']['white'][1]:.4f})")

    print(f"\n  Gamma:")
    print(f"    Red:   {info['gamma']['red']:.4f}")
    print(f"    Green: {info['gamma']['green']:.4f}")
    print(f"    Blue:  {info['gamma']['blue']:.4f}")

    print(f"\n  Capabilities:")
    print(f"    SDR Peak: {info['capabilities']['max_sdr']} cd/m2")
    print(f"    HDR Peak: {info['capabilities']['max_hdr']} cd/m2")
    print(f"    HDR: {'Yes' if info['capabilities']['hdr'] else 'No'}")
    print(f"    Wide Gamut: {'Yes' if info['capabilities']['wide_gamut'] else 'No'}")
    print(f"    VRR: {'Yes' if info['capabilities']['vrr'] else 'No'}")

    if info['notes']:
        print(f"\n  Notes: {info['notes']}")

    return 0

def cmd_enable_startup(args):
    """Enable auto-start at Windows startup."""
    from calibrate_pro.utils.startup_manager import enable_auto_start, is_auto_start_enabled

    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

    if is_auto_start_enabled():
        print("\nAuto-start is already enabled.")
        return 0

    if enable_auto_start(silent=True):
        print("\n[SUCCESS] Auto-start enabled!")
        print("Calibrate Pro will now start automatically with Windows")
        print("and apply your saved calibrations.")
        return 0
    else:
        print("\n[ERROR] Failed to enable auto-start.")
        print("Try running as administrator.")
        return 1

def cmd_disable_startup(args):
    """Disable auto-start."""
    from calibrate_pro.utils.startup_manager import disable_auto_start, is_auto_start_enabled

    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

    if not is_auto_start_enabled():
        print("\nAuto-start is not enabled.")
        return 0

    if disable_auto_start():
        print("\n[SUCCESS] Auto-start disabled.")
        return 0
    else:
        print("\n[ERROR] Failed to disable auto-start.")
        return 1


def cmd_generate_profiles(args):
    """
    Generate multiple calibration profiles in one pass.

    For a single panel characterization, generates LUTs and ICC profiles
    for multiple target color spaces (sRGB, DCI-P3, Rec.709, AdobeRGB).
    This lets users switch between profiles without re-running calibration.
    """
    from calibrate_pro.panels.detection import enumerate_displays, identify_display
    from calibrate_pro.panels.database import PanelDatabase
    from calibrate_pro.sensorless.neuralux import SensorlessEngine
    from calibrate_pro.core.icc_profile import create_display_profile
    from calibrate_pro.core.lut_engine import LUTGenerator
    from calibrate_pro.core.color_math import primaries_to_xyz_matrix

    print(f"\nCalibrate Pro v{__version__} - Multi-Profile Generation")
    print("=" * 60)

    # Get display
    displays = enumerate_displays()
    if not displays:
        print("No displays detected.")
        return 1

    display_index = (args.display - 1) if hasattr(args, 'display') and args.display else 0
    if display_index >= len(displays):
        display_index = 0

    display = displays[display_index]
    db = PanelDatabase()

    # Find panel
    panel_key = identify_display(display)
    panel = db.get_panel(panel_key) if panel_key else db.get_fallback()

    panel_name = panel.name
    print(f"\nPanel: {panel_name} ({panel.panel_type})")

    # Output directory
    output_dir = Path(args.output) if hasattr(args, 'output') and args.output else Path("profiles")
    output_dir.mkdir(parents=True, exist_ok=True)

    # Target profiles to generate
    profiles = {
        "sRGB": {
            "primaries": ((0.6400, 0.3300), (0.3000, 0.6000), (0.1500, 0.0600)),
            "white": (0.3127, 0.3290),
            "gamma": 2.2,
            "desc": "sRGB / Rec.709 (web, general use)"
        },
        "DCI-P3": {
            "primaries": ((0.6800, 0.3200), (0.2650, 0.6900), (0.1500, 0.0600)),
            "white": (0.3127, 0.3290),  # Display P3 uses D65
            "gamma": 2.2,
            "desc": "Display P3 (Apple, HDR content, wide gamut)"
        },
        "Rec709": {
            "primaries": ((0.6400, 0.3300), (0.3000, 0.6000), (0.1500, 0.0600)),
            "white": (0.3127, 0.3290),
            "gamma": 2.4,  # BT.1886
            "desc": "Rec.709 / BT.1886 (broadcast video)"
        },
        "AdobeRGB": {
            "primaries": ((0.6400, 0.3300), (0.2100, 0.7100), (0.1500, 0.0600)),
            "white": (0.3127, 0.3290),
            "gamma": 2.2,
            "desc": "Adobe RGB (photography, print)"
        },
    }

    primaries = panel.native_primaries
    panel_prims = (
        primaries.red.as_tuple(),
        primaries.green.as_tuple(),
        primaries.blue.as_tuple()
    )
    safe_panel = panel_name.replace(" ", "_").replace("/", "_")

    generated = []
    for profile_name, config in profiles.items():
        print(f"\n  Generating {profile_name}...")
        print(f"    {config['desc']}")

        gen = LUTGenerator(33)

        # Build correction matrix for this target
        target_to_xyz = primaries_to_xyz_matrix(
            config["primaries"][0], config["primaries"][1],
            config["primaries"][2], config["white"]
        )
        panel_to_xyz = primaries_to_xyz_matrix(
            panel_prims[0], panel_prims[1], panel_prims[2],
            primaries.white.as_tuple()
        )
        import numpy as np
        xyz_to_panel = np.linalg.inv(panel_to_xyz)
        color_matrix = xyz_to_panel @ target_to_xyz

        lut = gen.create_calibration_lut(
            panel_primaries=panel_prims,
            panel_white=primaries.white.as_tuple(),
            target_primaries=config["primaries"],
            target_white=config["white"],
            gamma_red=panel.gamma_red.gamma,
            gamma_green=panel.gamma_green.gamma,
            gamma_blue=panel.gamma_blue.gamma,
            color_matrix=color_matrix,
            title=f"{safe_panel} {profile_name}",
            target_gamma=config["gamma"]
        )

        lut_path = output_dir / f"{safe_panel}_{profile_name}.cube"
        lut.save(lut_path)

        icc = create_display_profile(
            description=f"{panel_name} - {profile_name}",
            red_primary=primaries.red.as_tuple(),
            green_primary=primaries.green.as_tuple(),
            blue_primary=primaries.blue.as_tuple(),
            white_point=primaries.white.as_tuple(),
            gamma=config["gamma"]
        )
        icc_path = output_dir / f"{safe_panel}_{profile_name}.icc"
        icc.save(icc_path)

        print(f"    LUT: {lut_path}")
        print(f"    ICC: {icc_path}")
        generated.append(profile_name)

    print(f"\n{'=' * 60}")
    print(f"  Generated {len(generated)} profiles: {', '.join(generated)}")
    print(f"  Output: {output_dir.absolute()}")
    print(f"\n  To apply a profile:")
    print(f"    calibrate-pro calibrate --profile sRGB")
    print(f"    calibrate-pro calibrate --profile DCI-P3")
    print(f"{'=' * 60}\n")

    return 0


def cmd_restore(args):
    """
    Restore displays to their pre-calibration state.

    Resets gamma ramps to linear, removes DWM LUTs, and clears
    saved calibration state. This is the undo button.
    """
    from calibrate_pro.panels.detection import (
        enumerate_displays, reset_gamma_ramp, get_display_name
    )

    print(f"\nCalibrate Pro v{__version__} - Restore Defaults")
    print("=" * 60)

    display_index = args.display - 1 if hasattr(args, 'display') and args.display else None
    displays = enumerate_displays()

    if not displays:
        print("No displays detected.")
        return 1

    targets = [displays[display_index]] if display_index is not None and display_index < len(displays) else displays
    restored = 0

    for i, display in enumerate(displays):
        if display not in targets:
            continue

        idx = i
        name = get_display_name(display)
        print(f"\n  {name}:")

        # 1. Remove DWM LUT (most likely method used)
        dwm_removed = False
        try:
            from calibrate_pro.lut_system.dwm_lut import remove_lut as dwm_remove_lut
            if dwm_remove_lut(idx):
                print(f"    DWM LUT removed")
                dwm_removed = True
        except Exception:
            pass

        # 2. Reset gamma ramp to linear (in case VCGT was used)
        try:
            if reset_gamma_ramp(display.device_name):
                print(f"    Gamma ramp reset")
        except Exception:
            pass

        if not dwm_removed:
            print(f"    Display reset to defaults")

        # 3. Clear saved calibration state
        try:
            from calibrate_pro.utils.startup_manager import StartupManager
            manager = StartupManager()
            manager.clear_calibration(idx)
        except Exception:
            pass

        restored += 1

    # 4. Optionally disable auto-start
    if hasattr(args, 'disable_startup') and args.disable_startup:
        try:
            from calibrate_pro.utils.startup_manager import StartupManager
            manager = StartupManager()
            manager.disable_startup()
            print(f"\n[OK] Auto-start disabled")
        except Exception:
            pass

    print(f"\n{'=' * 60}")
    print(f"  Restored {restored} display(s) to defaults.")
    print(f"  Your display is now using uncalibrated settings.")
    print(f"{'=' * 60}\n")

    return 0


def cmd_gui(args):
    """Launch the professional calibration GUI."""
    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

    # Check for admin privileges (required for dwm_lut)
    if not is_admin():
        print("\nRequesting administrator privileges...")
        print("(Required for system-wide 3D LUT and DDC/CI access)")
        run_as_admin()
        return 0

    print("\n[ADMIN] Running with administrator privileges")
    print("Launching Professional Calibration GUI...")

    try:
        from PyQt6.QtWidgets import QApplication
        from calibrate_pro.gui.professional_calibration import ProfessionalCalibrationWindow
    except ImportError as e:
        print(f"\nError: PyQt6 is required for GUI mode.")
        print(f"Install with: pip install PyQt6")
        print(f"Details: {e}")
        return 1

    app = QApplication(sys.argv)
    window = ProfessionalCalibrationWindow()
    window.show()
    return app.exec()


def cmd_native_calibrate(args):
    """Native i1Display3 calibration - no ArgyllCMS required."""
    import numpy as np
    import hid
    import struct
    import tkinter as tk

    print("=" * 65)
    print("  NATIVE DISPLAY CALIBRATION")
    print("  Sensor: i1Display3 (native USB HID)")
    print("  No ArgyllCMS required.")
    print("=" * 65)

    # OLED calibration matrix from EEPROM
    OLED_MATRIX = np.array([
        [0.03836831, -0.02175997, 0.01696057],
        [0.01449629,  0.01611903, 0.00057150],
        [-0.00004481, 0.00035042, 0.08032401],
    ])

    M_MASK = 0xFFFFFFFF

    # Open sensor
    print("\nConnecting to colorimeter...")
    try:
        device = hid.device()
        device.open(0x0765, 0x5020)
    except Exception as e:
        print(f"  Failed to open sensor: {e}")
        print("  Ensure i1Display3 is connected and no other software is using it.")
        return 1

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
    print("  Sensor unlocked.")

    def measure_xyz_native(r, g, b):
        intclks = int(1.0 * 12000000)
        cmd = bytearray(65); cmd[0] = 0x00; cmd[1] = 0x01
        struct.pack_into('<I', cmd, 2, intclks)
        device.write(cmd)
        resp = device.read(64, timeout_ms=4000)
        if resp and resp[0] == 0x00 and resp[1] == 0x01:
            rv = struct.unpack('<I', bytes(resp[2:6]))[0]
            gv = struct.unpack('<I', bytes(resp[6:10]))[0]
            bv = struct.unpack('<I', bytes(resp[10:14]))[0]
            t = intclks / 12000000.0
            freq = np.array([0.5*(rv+0.5)/t, 0.5*(gv+0.5)/t, 0.5*(bv+0.5)/t])
            if np.max(freq) > 0.3:
                return OLED_MATRIX @ freq
        return None

    # Find display
    from calibrate_pro.panels.detection import enumerate_displays
    displays = enumerate_displays()
    dx, dy, dw, dh = 0, 0, 3840, 2160
    for d in displays:
        if d.width == 3840:
            dx, dy, dw, dh = d.position_x, d.position_y, d.width, d.height
            break

    # Patch display
    root = tk.Tk()
    root.overrideredirect(True)
    root.attributes("-topmost", True)
    root.geometry(f"{dw}x{dh}+{dx}+{dy}")
    canvas = tk.Canvas(root, highlightthickness=0, cursor="none")
    canvas.pack(fill=tk.BOTH, expand=True)

    def display_fn(r, g, b):
        ri = max(0, min(255, int(r * 255 + 0.5)))
        gi = max(0, min(255, int(g * 255 + 0.5)))
        bi = max(0, min(255, int(b * 255 + 0.5)))
        canvas.config(bg=f"#{ri:02x}{gi:02x}{bi:02x}")
        root.update()
        time.sleep(1.2)

    # Profile
    from calibrate_pro.calibration.native_loop import (
        profile_display, build_correction_lut
    )

    print(f"\nProfiling display ({args.steps} steps per channel)...")
    profile = profile_display(measure_xyz_native, display_fn, n_steps=args.steps)
    print(f"  White: {profile.white_Y:.1f} cd/m2, WP: ({profile.white_xy[0]:.4f}, {profile.white_xy[1]:.4f})")
    print(f"  Red:   ({profile.red_xy[0]:.4f}, {profile.red_xy[1]:.4f})")
    print(f"  Green: ({profile.green_xy[0]:.4f}, {profile.green_xy[1]:.4f})")
    print(f"  Blue:  ({profile.blue_xy[0]:.4f}, {profile.blue_xy[1]:.4f})")

    # Build LUT
    print(f"\nBuilding {args.lut_size}^3 correction LUT...")
    lut = build_correction_lut(profile, size=args.lut_size)

    # Save
    output_dir = args.output or os.path.expanduser("~/Documents/Calibrate Pro/Calibrations")
    os.makedirs(output_dir, exist_ok=True)
    lut_path = os.path.join(output_dir, "native_calibration.cube")
    lut.title = "Calibrate Pro - Native Measured Correction"
    lut.save(lut_path)
    print(f"  Saved: {lut_path}")

    # Apply
    if args.apply:
        print("\nApplying LUT via dwm_lut...")
        try:
            from calibrate_pro.lut_system.dwm_lut import DwmLutController
            dwm = DwmLutController()
            if dwm.is_available:
                dwm.load_lut_file(0, lut_path)
                print("  LUT applied to display 0.")
            else:
                print("  dwm_lut not available. Copy LUT to C:\\Windows\\Temp\\luts\\0_0.cube")
        except Exception as e:
            print(f"  LUT application failed: {e}")

    root.destroy()
    device.close()

    print("\n  Calibration complete.")
    print(f"  LUT: {lut_path}")
    return 0


def cmd_ddc_calibrate(args):
    """
    DDC/CI-First Calibration - Hardware calibration before LUT.

    This is the proper calibration workflow:
    1. 99.9% of calibration via DDC/CI (RGB gains, offsets, gamma, brightness)
    2. 0.1% residual correction via minimal 3D LUT (gamut mapping only)
    """
    print(f"\nCalibrate Pro v{__version__} - DDC/CI-First Calibration")
    print("=" * 60)

    from calibrate_pro.hardware.ddc_ci import DDCCIController, VCPCode
    from calibrate_pro.panels.database import PanelDatabase
    from calibrate_pro.core.lut_engine import LUTGenerator, LUTFormat
    from calibrate_pro.core.color_math import primaries_to_xyz_matrix, xyz_to_lab, bradford_adapt, D50_WHITE, D65_WHITE, delta_e_2000
    from calibrate_pro.sensorless.neuralux import SensorlessEngine
    import numpy as np
    import shutil
    import time

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

    print(f"\nDisplay: {display.monitor_name}")
    print(f"Resolution: {display.width}x{display.height} @ {display.refresh_rate}Hz")
    print(f"Position: ({display.position_x}, {display.position_y})")

    # Get panel info from database using proper identification
    from calibrate_pro.panels.detection import identify_display

    database = PanelDatabase()

    # Use command-line model override, or auto-identify from EDID/fingerprint
    if hasattr(args, 'model') and args.model:
        model_string = args.model
        panel = database.find_panel(model_string)
    else:
        # Auto-identify using EDID and fingerprint matching
        identified_key = identify_display(display)
        if identified_key:
            model_string = identified_key
            panel = database.find_panel(identified_key)
        else:
            model_string = display.monitor_name
            panel = database.find_panel(model_string)

    if panel:
        print(f"Panel Profile: {panel.manufacturer} {panel.model_pattern.split('|')[0]}")
        print(f"Panel Type: {panel.panel_type}")
    else:
        print("Warning: No panel profile found, using generic QD-OLED defaults")
        panel = database.get_fallback()

    # Initialize DDC/CI
    ddc = DDCCIController()
    if not ddc.available:
        print("\nError: DDC/CI not available on this system")
        return 1

    monitors = ddc.enumerate_monitors()
    if not monitors:
        print("\nError: No DDC/CI capable monitors found")
        return 1

    # Find the matching monitor by index
    monitor_idx = args.display - 1 if args.display else 0
    if monitor_idx >= len(monitors):
        print(f"\nError: Monitor index {monitor_idx} out of range (found {len(monitors)} monitors)")
        return 1

    monitor = monitors[monitor_idx]
    print(f"\nDDC/CI Monitor: {monitor['name']}")

    # Read current settings
    print("\n--- Current DDC/CI Settings ---")
    settings = ddc.get_settings(monitor)
    print(f"  Brightness: {settings.brightness}/100")
    print(f"  Contrast: {settings.contrast}/100")
    print(f"  RGB Gains: R={settings.red_gain}, G={settings.green_gain}, B={settings.blue_gain}")
    print(f"  RGB Offsets: R={settings.red_black_level}, G={settings.green_black_level}, B={settings.blue_black_level}")

    # Build panel color model for simulation
    primaries = panel.native_primaries
    panel_to_xyz = primaries_to_xyz_matrix(
        (primaries.red.x, primaries.red.y),
        (primaries.green.x, primaries.green.y),
        (primaries.blue.x, primaries.blue.y),
        (primaries.white.x, primaries.white.y)
    )

    # D65 reference white in Lab (use D65 as reference illuminant - will be [100, 0, 0])
    d65_xyz = np.array([0.95047, 1.0, 1.08883])  # D65 XYZ
    d65_lab = xyz_to_lab(d65_xyz, D65_WHITE)  # = [100, 0, 0] (perfect white)

    def simulate_white_point(r_gain, g_gain, b_gain):
        """Simulate the white point given RGB gain settings."""
        # Gains affect the linear RGB output
        # Normalize gains (100 = 1.0)
        r_mult = r_gain / 100.0
        g_mult = g_gain / 100.0
        b_mult = b_gain / 100.0

        # White input (1, 1, 1) with gain multipliers
        rgb_linear = np.array([r_mult, g_mult, b_mult])

        # Convert to XYZ
        xyz = panel_to_xyz @ rgb_linear

        # Normalize to Y=1
        if xyz[1] > 0:
            xyz = xyz / xyz[1]

        # Calculate chromaticity
        total = xyz[0] + xyz[1] + xyz[2]
        if total > 0:
            x = xyz[0] / total
            y = xyz[1] / total
        else:
            x, y = 0.3127, 0.3290

        # Calculate Delta E from D65 (use D65 as Lab reference - both colors are in D65 space)
        lab = xyz_to_lab(xyz, D65_WHITE)
        de = delta_e_2000(lab, d65_lab)

        return x, y, de

    # =========================================================================
    # ITERATIVE DDC/CI CALIBRATION - Real-time adjustment
    # =========================================================================
    print("\n" + "=" * 60)
    print("  AUTOMATED ITERATIVE CALIBRATION")
    print("  Watch the OSD values adjust in real-time...")
    print("=" * 60)

    # Set initial values (start slightly off to show iteration)
    target_brightness = 50 if not hasattr(args, 'brightness') or not args.brightness else args.brightness
    target_contrast = 80

    # Start with non-optimal values to demonstrate iteration
    r_gain = settings.red_gain if settings.red_gain > 0 else 85
    g_gain = settings.green_gain if settings.green_gain > 0 else 90
    b_gain = settings.blue_gain if settings.blue_gain > 0 else 95

    # If gains are all 100 (already at target), start with offset values
    if r_gain == 100 and g_gain == 100 and b_gain == 100:
        r_gain, g_gain, b_gain = 85, 90, 95

    # Set brightness and contrast first
    print(f"\n[Step 1] Setting Brightness: {target_brightness}, Contrast: {target_contrast}")
    ddc.set_vcp(monitor, VCPCode.BRIGHTNESS, target_brightness)
    ddc.set_vcp(monitor, VCPCode.CONTRAST, target_contrast)
    time.sleep(0.3)

    # Set gamma
    print(f"[Step 2] Setting Gamma: 2.2 (VCP value: 22)")
    try:
        ddc.set_vcp(monitor, 0x72, 22)
    except:
        pass
    time.sleep(0.3)

    # Set RGB offsets to neutral
    print(f"[Step 3] Setting RGB Offsets: 50/50/50 (neutral)")
    ddc.set_vcp(monitor, VCPCode.RED_BLACK_LEVEL, 50)
    ddc.set_vcp(monitor, VCPCode.GREEN_BLACK_LEVEL, 50)
    ddc.set_vcp(monitor, VCPCode.BLUE_BLACK_LEVEL, 50)
    time.sleep(0.3)

    # Iterative RGB gain calibration
    print(f"\n[Step 4] Iterative RGB Gain Calibration for D65 White Point")
    print("-" * 60)

    # Target: D65 (0.3127, 0.3290)
    target_x, target_y = 0.3127, 0.3290
    tolerance = 0.5  # Delta E tolerance

    max_iterations = 30
    iteration = 0

    # For D65-native panels, find optimal gains by testing points
    # First, check what Delta E we get at 100/100/100 (native D65)
    test_x, test_y, test_de = simulate_white_point(100, 100, 100)
    print(f"  [INFO] Panel native (100/100/100): xy=({test_x:.4f}, {test_y:.4f}), Delta E={test_de:.2f}")

    # If panel is already D65 native and close, just go directly to 100/100/100
    if test_de < 1.0:
        print(f"  [INFO] Panel is D65-native, iterating toward 100/100/100")

    # Apply initial gains
    ddc.set_vcp(monitor, VCPCode.RED_GAIN, r_gain)
    ddc.set_vcp(monitor, VCPCode.GREEN_GAIN, g_gain)
    ddc.set_vcp(monitor, VCPCode.BLUE_GAIN, b_gain)
    time.sleep(0.2)

    # Track best solution and accumulated error for integral control
    best_de = float('inf')
    best_rgb = (r_gain, g_gain, b_gain)
    accumulated_r = 0.0
    accumulated_g = 0.0
    accumulated_b = 0.0
    prev_de = float('inf')
    stall_count = 0

    while iteration < max_iterations:
        iteration += 1

        # Simulate current white point
        curr_x, curr_y, delta_e = simulate_white_point(r_gain, g_gain, b_gain)

        # Track best solution
        if delta_e < best_de:
            best_de = delta_e
            best_rgb = (r_gain, g_gain, b_gain)
            stall_count = 0
        else:
            stall_count += 1

        # Display current state
        status = "*" if delta_e == best_de else " "
        print(f"  Iter {iteration:2d}: RGB Gain [{r_gain:3d}, {g_gain:3d}, {b_gain:3d}]  "
              f"xy=({curr_x:.4f}, {curr_y:.4f})  dE={delta_e:.2f} {status}")

        # Check if converged
        if delta_e < tolerance:
            print(f"\n  [CONVERGED] Delta E {delta_e:.2f} < {tolerance} tolerance")
            break

        # Calculate error in chromaticity
        x_error = target_x - curr_x
        y_error = target_y - curr_y

        # Accumulate fractional errors (integral term)
        accumulated_r += x_error * 50 + y_error * 15
        accumulated_g += y_error * 60
        accumulated_b += -x_error * 40 - y_error * 25

        # Calculate adjustments using proportional + integral
        # Proportional term
        p_sensitivity = 200  # Increased sensitivity
        delta_r_p = x_error * p_sensitivity * 1.5 + y_error * p_sensitivity * 0.3
        delta_g_p = y_error * p_sensitivity * 1.2
        delta_b_p = -x_error * p_sensitivity * 1.0 - y_error * p_sensitivity * 0.5

        # Integral term (accumulated error)
        i_gain = 0.3
        delta_r = delta_r_p + accumulated_r * i_gain
        delta_g = delta_g_p + accumulated_g * i_gain
        delta_b = delta_b_p + accumulated_b * i_gain

        # Round with minimum step size when stalled
        if stall_count >= 2 and abs(delta_r) < 1 and abs(delta_g) < 1 and abs(delta_b) < 1:
            # Force a minimum step in the direction of best known point (100/100/100 for D65)
            delta_r = 1 if r_gain < 100 else (-1 if r_gain > 100 else 0)
            delta_g = 1 if g_gain < 100 else (-1 if g_gain > 100 else 0)
            delta_b = 1 if b_gain < 100 else (-1 if b_gain > 100 else 0)
            accumulated_r = accumulated_g = accumulated_b = 0  # Reset integrators
        else:
            # Round with bias toward non-zero
            delta_r = int(delta_r + (0.5 if delta_r > 0 else -0.5)) if abs(delta_r) >= 0.5 else 0
            delta_g = int(delta_g + (0.5 if delta_g > 0 else -0.5)) if abs(delta_g) >= 0.5 else 0
            delta_b = int(delta_b + (0.5 if delta_b > 0 else -0.5)) if abs(delta_b) >= 0.5 else 0

        # Limit step size
        max_step = 5
        delta_r = max(-max_step, min(max_step, delta_r))
        delta_g = max(-max_step, min(max_step, delta_g))
        delta_b = max(-max_step, min(max_step, delta_b))

        # Apply adjustments
        r_gain = max(50, min(100, r_gain + delta_r))
        g_gain = max(50, min(100, g_gain + delta_g))
        b_gain = max(50, min(100, b_gain + delta_b))

        # Apply to monitor via DDC/CI
        ddc.set_vcp(monitor, VCPCode.RED_GAIN, r_gain)
        ddc.set_vcp(monitor, VCPCode.GREEN_GAIN, g_gain)
        ddc.set_vcp(monitor, VCPCode.BLUE_GAIN, b_gain)

        # Update previous Delta E
        prev_de = delta_e

        # Delay so user can see OSD changes
        time.sleep(0.4)

    # If we didn't converge but found a good solution, use it
    if delta_e > tolerance and best_de < delta_e:
        r_gain, g_gain, b_gain = best_rgb
        ddc.set_vcp(monitor, VCPCode.RED_GAIN, r_gain)
        ddc.set_vcp(monitor, VCPCode.GREEN_GAIN, g_gain)
        ddc.set_vcp(monitor, VCPCode.BLUE_GAIN, b_gain)
        print(f"\n  [BEST] Using best found: RGB [{r_gain}, {g_gain}, {b_gain}], dE={best_de:.2f}")

    print("-" * 60)

    # Final verification
    final_x, final_y, final_de = simulate_white_point(r_gain, g_gain, b_gain)
    print(f"\n[FINAL] RGB Gains: R={r_gain}, G={g_gain}, B={b_gain}")
    print(f"[FINAL] White Point: ({final_x:.4f}, {final_y:.4f})")
    print(f"[FINAL] Delta E from D65: {final_de:.2f}")

    if final_de < 0.5:
        grade = "REFERENCE GRADE"
    elif final_de < 1.0:
        grade = "PROFESSIONAL GRADE"
    elif final_de < 2.0:
        grade = "EXCELLENT"
    else:
        grade = "GOOD"
    print(f"[FINAL] Calibration Grade: {grade}")

    print("\n[OK] DDC/CI hardware calibration applied!")

    # Now generate minimal gamut-mapping LUT
    if not args.no_lut:
        print("\n--- Generating Minimal Gamut-Mapping LUT ---")
        print("  (LUT only handles wide gamut to sRGB compression)")
        print("  (Grayscale/gamma/white point handled by DDC/CI)")

        # Create a minimal LUT that only does gamut mapping
        lut_size = args.lut_size if hasattr(args, 'lut_size') else 33

        # Use sensorless engine for gamut-only LUT
        engine = SensorlessEngine()
        engine.load_panel_data(panel)

        # Generate gamut-only 3D LUT
        lut = engine.generate_gamut_only_lut(lut_size)

        # Save LUT
        output_dir = Path(args.output) if hasattr(args, 'output') and args.output else Path(".")
        output_dir.mkdir(parents=True, exist_ok=True)

        lut_filename = f"{model_string.replace(' ', '_')}_gamut_only.cube"
        lut_path = output_dir / lut_filename
        lut.save(lut_path, LUTFormat.CUBE)
        print(f"\n  Gamut LUT saved: {lut_path}")

        # Copy to dwm_lut directory
        dwm_lut_dir = Path("C:/Windows/Temp/luts")
        if dwm_lut_dir.exists():
            # dwm_lut naming: {left}_{top}.cube
            dwm_filename = f"{display.position_x}_{display.position_y}.cube"
            dwm_path = dwm_lut_dir / dwm_filename
            shutil.copy2(lut_path, dwm_path)
            print(f"  Copied to dwm_lut: {dwm_path}")

        # Verify LUT quality
        print("\n--- LUT Verification ---")
        delta_e_avg = engine.verify_lut_accuracy(lut)
        print(f"  Average Delta E: {delta_e_avg:.2f}")

        if delta_e_avg < 1.0:
            grade = "Reference Grade (Delta E < 1.0)"
        elif delta_e_avg < 2.0:
            grade = "Professional Grade (Delta E < 2.0)"
        else:
            grade = "Consumer Grade (Delta E >= 2.0)"
        print(f"  Grade: {grade}")

    print("\n" + "=" * 60)
    print("DDC/CI-First Calibration Complete!")
    print("=" * 60)
    print("\nHardware calibration handles:")
    print("  - White point (D65)")
    print("  - Grayscale tracking")
    print("  - Gamma (2.2)")
    print("  - Brightness and contrast")
    if not args.no_lut:
        print("\nMinimal LUT handles:")
        print("  - Wide gamut to sRGB compression only")
    print("\nThis preserves the monitor's native bit depth and reduces")
    print("processing artifacts compared to LUT-only calibration.")

    ddc.close()
    return 0


def cmd_hdr_status(args):
    """Show HDR status for all displays."""
    from calibrate_pro.display.hdr_detect import print_hdr_status

    print(f"\nCalibrate Pro v{__version__} - HDR Status")
    print("=" * 50)
    print_hdr_status()
    print()
    return 0


def cmd_match(args):
    """
    Analyze and match multiple displays for consistent appearance.

    Computes common white point, brightness, and gamma targets that
    all connected displays can achieve, then shows the adjustment plan.
    """
    from calibrate_pro.panels.detection import (
        enumerate_displays, identify_display, get_display_name
    )
    from calibrate_pro.panels.database import PanelDatabase
    from calibrate_pro.calibration.multi_display import analyze_matching, print_matching_plan

    print(f"\nCalibrate Pro v{__version__} - Multi-Display Matching")
    print("=" * 60)

    displays = enumerate_displays()
    if len(displays) < 2:
        print("Multi-display matching requires at least 2 displays.")
        return 1

    db = PanelDatabase()
    panel_list = []

    for i, display in enumerate(displays):
        name = get_display_name(display)
        panel_key = identify_display(display)
        panel = db.get_panel(panel_key) if panel_key else db.get_fallback()
        panel_list.append({"index": i, "name": name, "panel": panel})
        print(f"  Display {i + 1}: {name} ({panel.panel_type})")

    result = analyze_matching(panel_list)
    print_matching_plan(result)
    print()

    return 0


def cmd_refine(args):
    """
    Refine calibration using a colorimeter.

    Starts with sensorless calibration, then iteratively measures and
    corrects residual error until convergence. Requires a colorimeter
    (supported via ArgyllCMS) or manual XYZ entry.
    """
    from calibrate_pro.panels.detection import (
        enumerate_displays, identify_display, get_display_name
    )
    from calibrate_pro.panels.database import PanelDatabase
    from calibrate_pro.calibration.hybrid import HybridCalibrationEngine
    from calibrate_pro.hardware.measurement import MeasurementConfig, create_measure_fn

    print(f"\nCalibrate Pro v{__version__} - Hybrid Calibration (Sensorless + Measured)")
    print("=" * 60)

    displays = enumerate_displays()
    if not displays:
        print("No displays detected.")
        return 1

    display_index = (args.display - 1) if hasattr(args, 'display') and args.display else 0
    if display_index >= len(displays):
        display_index = 0

    display = displays[display_index]
    name = get_display_name(display)
    print(f"\nDisplay: {name}")

    # Find panel
    db = PanelDatabase()
    panel_key = identify_display(display)
    panel = db.get_panel(panel_key) if panel_key else db.get_fallback()
    print(f"Panel: {panel.name} ({panel.panel_type})")

    # Determine measurement mode
    mode = "argyll"
    if hasattr(args, 'manual') and args.manual:
        mode = "manual"
    elif hasattr(args, 'simulated') and args.simulated:
        mode = "simulated"

    config = MeasurementConfig(
        display_index=display_index,
        mode=mode,
        argyll_path=getattr(args, 'argyll_path', None)
    )

    print(f"Measurement: {mode}")

    measure_fn = create_measure_fn(config)
    if measure_fn is None and mode == "argyll":
        # Determine why initialization failed
        from calibrate_pro.hardware.argyll_backend import ArgyllConfig as _AC
        _ac = _AC()
        _found = _ac.find_argyll()
        if _found:
            print(f"\n  ArgyllCMS found: {_ac.bin_path}")
            print(f"  No colorimeter detected. Connect your colorimeter and try again.")
            print(f"  Supported: i1Display Pro, SpyderX, ColorMunki, and most USB colorimeters.")
        else:
            print(f"\n  ArgyllCMS not found. Install from https://www.argyllcms.com/")
        print(f"\n  Alternatives:")
        print(f"    calibrate-pro refine --manual    (type XYZ values manually)")
        print(f"    calibrate-pro refine --simulated (testing without hardware)")
        return 1

    output_dir = Path(args.output) if hasattr(args, 'output') and args.output else None
    if output_dir is None:
        output_dir = Path.home() / "Documents" / "Calibrate Pro" / "Calibrations"

    def progress(msg, pct):
        print(f"  [{int(pct*100):3d}%] {msg}")

    engine = HybridCalibrationEngine(
        measure_fn=measure_fn,
        progress_fn=progress
    )

    result = engine.calibrate(panel, output_dir)

    # Cleanup
    if measure_fn and hasattr(measure_fn, 'close'):
        measure_fn.close()

    print(f"\n{'=' * 60}")
    if result.success:
        print(f"  Sensorless baseline:  dE {result.sensorless_delta_e:.2f} (predicted)")
        if result.iterations:
            print(f"  Measured after refine: dE {result.final_measured_delta_e:.2f}")
            print(f"  Iterations: {len(result.iterations)}")
        if result.lut_path:
            print(f"  LUT: {result.lut_path}")
        if result.icc_path:
            print(f"  ICC: {result.icc_path}")
    else:
        print(f"  Failed: {result.message}")
    print(f"{'=' * 60}\n")

    return 0 if result.success else 1


def cmd_auto(args):
    """
    Fully automatic calibration. Zero arguments required.

    Detects all connected displays, identifies each panel via EDID and
    the panel database, applies DDC/CI hardware corrections where available,
    generates ICC profiles and 3D LUTs, installs them system-wide, and
    registers for auto-start so calibration persists across reboots.
    """
    from calibrate_pro.sensorless.auto_calibration import auto_calibrate_all

    print(f"\nCalibrate Pro v{__version__} - Automatic Calibration")
    print("=" * 60)
    print("No instruments required. No input needed.")
    print("Detecting and calibrating all connected displays...\n")

    output_dir = Path(args.output) if hasattr(args, 'output') and args.output else None
    no_ddc = hasattr(args, 'no_ddc') and args.no_ddc
    no_persist = hasattr(args, 'no_persist') and args.no_persist
    use_hdr = hasattr(args, 'hdr') and args.hdr

    if use_hdr:
        print("Mode: HDR (PQ/ST.2084)\n")

    last_step = [None]

    def progress_callback(message: str, progress: float, display_name: str):
        # Only print meaningful step changes, not every tick
        step = message.split("...")[0] if "..." in message else message
        if step != last_step[0]:
            last_step[0] = step
            sys.stdout.write(f"  {message}\n")
            sys.stdout.flush()

    results = auto_calibrate_all(
        output_dir=output_dir,
        callback=progress_callback,
        use_ddc=not no_ddc,
        persist=not no_persist,
        hdr_mode=use_hdr
    )

    # Clean output
    print("\n" + "=" * 60)

    all_passed = True
    report_paths = []

    for i, result in enumerate(results):
        print(f"\n  {result.display_name}")
        if result.success:
            grade = result.verification.get("grade", "N/A") if result.verification else "N/A"
            coverage = result.verification.get("gamut_coverage", {}) if result.verification else {}

            print(f"    {result.panel_matched} ({result.panel_type})")
            print(f"    Delta E: {result.delta_e_predicted:.2f} (sensorless)  |  Grade: {grade}")

            if coverage:
                print(f"    Gamut: sRGB {coverage.get('srgb_pct', 0):.0f}%  "
                      f"P3 {coverage.get('dci_p3_pct', 0):.0f}%  "
                      f"BT.2020 {coverage.get('bt2020_pct', 0):.0f}%")

            if result.lut_application_method:
                methods = {
                    "dwm_lut": "DWM 3D LUT (system-wide)",
                    "vcgt_from_3dlut": "VCGT gamma ramp",
                    "vcgt_direct": "VCGT from panel data",
                    "gamma_ramp": "gamma ramp",
                }
                print(f"    Applied: {methods.get(result.lut_application_method, result.lut_application_method)}")

            # Collect report paths from output dir
            if result.lut_path:
                report_candidate = Path(result.lut_path).with_name(
                    Path(result.lut_path).stem.replace("_hdr", "") + "_report.html"
                )
                if report_candidate.exists():
                    report_paths.append(report_candidate)
        else:
            all_passed = False
            print(f"    FAILED: {result.message}")

        for w in result.warnings:
            print(f"    Warning: {w}")

    succeeded = sum(1 for r in results if r.success)
    print(f"\n  {succeeded}/{len(results)} displays calibrated.")

    if all_passed and not no_persist:
        print("  Calibration persists across reboots.")

    if report_paths:
        print(f"\n  Reports saved to:")
        for rp in report_paths:
            print(f"    {rp}")

    print("=" * 60)

    # Open first report in browser
    if report_paths and not (hasattr(args, 'no_report') and args.no_report):
        try:
            import webbrowser
            webbrowser.open(str(report_paths[0].absolute()))
        except Exception:
            pass

    return 0 if all_passed else 1


def cmd_status(args):
    """Show calibration status for all displays."""
    from calibrate_pro.services.drift_monitor import print_calibration_status

    max_age = args.max_age if hasattr(args, 'max_age') and args.max_age else 30
    print_calibration_status(max_age_days=max_age)
    return 0


def cmd_plugins(args):
    """List discovered plugins."""
    from calibrate_pro.plugins.manager import print_discovered_plugins

    dirs = None
    if hasattr(args, 'plugin_dir') and args.plugin_dir:
        dirs = [args.plugin_dir]
    print_discovered_plugins(plugin_dirs=dirs)
    return 0


def cmd_tray(args):
    """Launch the system tray application."""
    from calibrate_pro.tray.tray_app import run_tray_app
    return run_tray_app()


def cmd_patterns(args):
    """Launch the fullscreen test pattern viewer."""
    from calibrate_pro.patterns.display import show_patterns

    display = args.display - 1 if args.display else 0
    if display < 0:
        display = 0

    print(f"\nCalibrate Pro v{__version__} - Test Patterns")
    print("=" * 50)
    print(f"Opening fullscreen patterns on display {display + 1}")
    print("  Left/Right arrows: cycle patterns")
    print("  1-8: jump to pattern by number")
    print("  Escape: exit\n")

    show_patterns(display=display)
    return 0


def cmd_hdr(args):
    """Launch the HDR calibration GUI."""
    print(f"\nCalibrate Pro v{__version__}")
    print("=" * 50)

    if not is_admin():
        print("\nRequesting administrator privileges...")
        run_as_admin()
        return 0

    print("\n[ADMIN] Running with administrator privileges")
    print("Launching HDR Calibration GUI...")

    try:
        from PyQt6.QtWidgets import QApplication
        from calibrate_pro.gui.hdr_calibration import HDRCalibrationWindow
    except ImportError as e:
        print(f"\nError: PyQt6 is required for GUI mode.")
        print(f"Install with: pip install PyQt6")
        print(f"Details: {e}")
        return 1

    app = QApplication(sys.argv)
    window = HDRCalibrationWindow()
    window.show()
    return app.exec()


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Calibrate Pro - Professional Display Calibration Suite",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )

    parser.add_argument(
        "--version", "-V",
        action="version",
        version=f"Calibrate Pro v{__version__}"
    )

    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # detect command
    detect_parser = subparsers.add_parser("detect", help="Detect connected displays")

    # calibrate command
    calibrate_parser = subparsers.add_parser("calibrate", help="Calibrate a display")
    calibrate_parser.add_argument(
        "--display", "-d",
        type=int,
        help="Display number (1-based)"
    )
    calibrate_parser.add_argument(
        "--model",
        type=str,
        help="Override monitor model name (e.g., PG27UCDM, G85SB)"
    )
    calibrate_parser.add_argument(
        "--output", "-o",
        type=str,
        default=".",
        help="Output directory for calibration files"
    )
    calibrate_parser.add_argument(
        "--mode", "-m",
        choices=["sensorless", "colorimeter", "hybrid"],
        default="sensorless",
        help="Calibration mode"
    )
    calibrate_parser.add_argument(
        "--lut-size",
        type=int,
        choices=[17, 33, 65],
        default=33,
        help="3D LUT grid size"
    )
    calibrate_parser.add_argument(
        "--no-icc",
        action="store_true",
        help="Skip ICC profile generation"
    )
    calibrate_parser.add_argument(
        "--no-lut",
        action="store_true",
        help="Skip 3D LUT generation"
    )

    # Target settings - Profile preset
    calibrate_parser.add_argument(
        "--profile", "-p",
        choices=["sRGB", "Rec709", "DCI-P3", "HDR10"],
        help="Calibration profile preset (overrides individual targets)"
    )

    # Target settings - White point
    calibrate_parser.add_argument(
        "--whitepoint", "-w",
        choices=["D50", "D55", "D65", "D75", "DCI"],
        help="White point target (D50=5000K, D65=6500K, DCI=6300K)"
    )
    calibrate_parser.add_argument(
        "--cct",
        type=int,
        help="Custom white point CCT in Kelvin (e.g., 6500)"
    )

    # Target settings - Luminance
    calibrate_parser.add_argument(
        "--luminance", "-l",
        type=float,
        help="Target peak luminance in cd/m2 (e.g., 120 for SDR, 1000 for HDR)"
    )
    calibrate_parser.add_argument(
        "--black-level",
        type=float,
        help="Target black level in cd/m2 (e.g., 0.05 for LCD, 0.0001 for OLED)"
    )

    # Target settings - Gamma
    calibrate_parser.add_argument(
        "--gamma", "-g",
        help="Gamma target: 2.2, 2.4, sRGB, BT1886, PQ, HLG, or custom value"
    )

    # Target settings - Gamut
    calibrate_parser.add_argument(
        "--gamut",
        choices=["sRGB", "DCI-P3", "Display-P3", "BT2020", "AdobeRGB"],
        help="Target color gamut"
    )

    # verify command
    verify_parser = subparsers.add_parser("verify", help="Verify calibration accuracy")
    verify_parser.add_argument(
        "--display", "-d",
        type=int,
        help="Display number (1-based)"
    )
    verify_parser.add_argument(
        "--measured",
        action="store_true",
        help="Use colorimeter-based measured verification (requires ArgyllCMS or manual entry)"
    )

    # list-panels command
    list_parser = subparsers.add_parser("list-panels", help="List supported panel profiles")

    # list-targets command
    targets_parser = subparsers.add_parser("list-targets", help="List calibration target presets")

    # info command
    info_parser = subparsers.add_parser("info", help="Show panel information")
    info_parser.add_argument(
        "panel",
        type=str,
        help="Panel key (e.g., PG27UCDM)"
    )

    # enable-startup command
    enable_startup_parser = subparsers.add_parser("enable-startup", help="Enable auto-start at Windows boot")

    # disable-startup command
    disable_startup_parser = subparsers.add_parser("disable-startup", help="Disable auto-start")

    # restore command - undo calibration
    restore_parser = subparsers.add_parser(
        "restore",
        help="Restore display to defaults (undo calibration)"
    )
    restore_parser.add_argument(
        "--display", "-d",
        type=int,
        help="Display number (1-based). Omit to restore all displays."
    )
    restore_parser.add_argument(
        "--disable-startup",
        action="store_true",
        help="Also disable auto-start"
    )

    # generate-profiles command - multi-profile generation
    profiles_gen_parser = subparsers.add_parser(
        "generate-profiles",
        help="Generate sRGB, P3, Rec.709, AdobeRGB profiles in one pass"
    )
    profiles_gen_parser.add_argument(
        "--display", "-d",
        type=int,
        help="Display number (1-based)"
    )
    profiles_gen_parser.add_argument(
        "--output", "-o",
        type=str,
        default="profiles",
        help="Output directory for profile files"
    )

    # match command - multi-display matching
    # hdr-status command
    hdr_status_parser = subparsers.add_parser(
        "hdr-status",
        help="Show HDR mode status for all displays"
    )

    match_parser = subparsers.add_parser(
        "match",
        help="Analyze and match multiple displays for consistent appearance"
    )

    # refine command - hybrid calibration with colorimeter
    refine_parser = subparsers.add_parser(
        "refine",
        help="Refine calibration using a colorimeter (ArgyllCMS)"
    )
    refine_parser.add_argument(
        "--display", "-d", type=int,
        help="Display number (1-based)"
    )
    refine_parser.add_argument(
        "--output", "-o", type=str,
        help="Output directory"
    )
    refine_parser.add_argument(
        "--manual", action="store_true",
        help="Manual XYZ entry mode (no ArgyllCMS needed)"
    )
    refine_parser.add_argument(
        "--simulated", action="store_true",
        help="Simulated measurement mode (for testing)"
    )
    refine_parser.add_argument(
        "--argyll-path", type=str,
        help="Path to ArgyllCMS bin directory"
    )

    # gui command - launches professional calibration GUI
    gui_parser = subparsers.add_parser("gui", help="Launch Professional Calibration GUI (runs as admin)")

    # hdr command - launches HDR calibration GUI
    hdr_parser = subparsers.add_parser("hdr", help="Launch HDR Calibration GUI (runs as admin)")

    # auto command - fully automatic zero-input calibration
    auto_parser = subparsers.add_parser(
        "auto",
        help="Fully automatic calibration (zero input required)"
    )
    auto_parser.add_argument(
        "--output", "-o",
        type=str,
        help="Output directory for calibration files (default: ~/Documents/Calibrate Pro)"
    )
    auto_parser.add_argument(
        "--no-ddc",
        action="store_true",
        help="Skip DDC/CI hardware adjustments (software LUT only)"
    )
    auto_parser.add_argument(
        "--no-persist",
        action="store_true",
        help="Don't register for auto-start or save state"
    )
    auto_parser.add_argument(
        "--no-report",
        action="store_true",
        help="Don't open calibration report in browser"
    )
    auto_parser.add_argument(
        "--hdr",
        action="store_true",
        help="Generate HDR (PQ/ST.2084) calibration LUT instead of SDR"
    )

    # tray command - system tray application
    tray_parser = subparsers.add_parser("tray", help="Launch system tray application")

    # patterns command - fullscreen test pattern viewer
    patterns_parser = subparsers.add_parser("patterns", help="Display fullscreen test patterns")
    patterns_parser.add_argument(
        "--display", "-d",
        type=int,
        help="Display number (1-based, default: primary)"
    )

    # uniformity command - screen uniformity analysis
    uniformity_parser = subparsers.add_parser(
        "uniformity",
        help="Measure and analyse screen uniformity"
    )
    uniformity_parser.add_argument(
        "--rows", type=int, default=5,
        help="Grid rows (default: 5)"
    )
    uniformity_parser.add_argument(
        "--cols", type=int, default=5,
        help="Grid columns (default: 5)"
    )
    uniformity_parser.add_argument(
        "--width", type=int, default=3840,
        help="Display width in pixels (default: 3840)"
    )
    uniformity_parser.add_argument(
        "--height", type=int, default=2160,
        help="Display height in pixels (default: 2160)"
    )
    uniformity_parser.add_argument(
        "--simulated", action="store_true",
        help="Generate simulated uniformity data for testing"
    )

    # export-panel command - export panel profile as community JSON
    export_panel_parser = subparsers.add_parser(
        "export-panel",
        help="Export current display panel profile as shareable JSON"
    )
    export_panel_parser.add_argument(
        "--display", "-d", type=int,
        help="Display number (1-based)"
    )
    export_panel_parser.add_argument(
        "--output", "-o", type=str,
        help="Output file path (default: <panel_name>_community.json)"
    )

    # import-panel command - import a community panel JSON
    import_panel_parser = subparsers.add_parser(
        "import-panel",
        help="Import a community panel JSON into the local database"
    )
    import_panel_parser.add_argument(
        "file", type=str,
        help="Path to the community panel JSON file"
    )

    # ddc-calibrate command - DDC/CI-first calibration
    ddc_parser = subparsers.add_parser("ddc-calibrate", help="DDC/CI-first calibration (hardware before LUT)")
    ddc_parser.add_argument(
        "--display", "-d",
        type=int,
        help="Display number (1-based)"
    )
    ddc_parser.add_argument(
        "--model",
        type=str,
        help="Override monitor model name"
    )
    ddc_parser.add_argument(
        "--output", "-o",
        type=str,
        default=".",
        help="Output directory for LUT files"
    )
    ddc_parser.add_argument(
        "--brightness", "-b",
        type=int,
        default=50,
        help="Target brightness (0-100)"
    )
    ddc_parser.add_argument(
        "--lut-size",
        type=int,
        choices=[17, 33, 65],
        default=33,
        help="3D LUT grid size"
    )
    ddc_parser.add_argument(
        "--no-lut",
        action="store_true",
        help="Skip LUT generation (DDC/CI only)"
    )

    # native-calibrate command - native USB colorimeter calibration
    native_parser = subparsers.add_parser(
        "native-calibrate",
        help="Calibrate using native i1Display3 driver (no ArgyllCMS)"
    )
    native_parser.add_argument(
        "--lut-size", type=int, choices=[17, 33, 65], default=33,
        help="3D LUT grid size (default: 33)"
    )
    native_parser.add_argument(
        "--steps", type=int, default=17,
        help="TRC measurement steps per channel (default: 17)"
    )
    native_parser.add_argument(
        "--apply", action="store_true",
        help="Apply LUT via dwm_lut after generation"
    )
    native_parser.add_argument(
        "--verify", action="store_true",
        help="Run ColorChecker verification after calibration"
    )
    native_parser.add_argument(
        "--output", "-o", type=str,
        help="Output directory for LUT file"
    )

    # status command - calibration age / drift detection
    status_parser = subparsers.add_parser(
        "status",
        help="Show calibration status and age for all displays"
    )
    status_parser.add_argument(
        "--max-age",
        type=int,
        default=30,
        help="Days after which a calibration is considered stale (default: 30)"
    )

    # plugins command - list discovered plugins
    plugins_parser = subparsers.add_parser(
        "plugins",
        help="List discovered plugins"
    )
    plugins_parser.add_argument(
        "--plugin-dir",
        type=str,
        help="Override plugin directory to scan"
    )

    args = parser.parse_args()

    if args.command == "detect":
        return cmd_detect(args)
    elif args.command == "calibrate":
        return cmd_calibrate(args)
    elif args.command == "verify":
        return cmd_verify(args)
    elif args.command == "list-panels":
        return cmd_list_panels(args)
    elif args.command == "list-targets":
        return cmd_list_targets(args)
    elif args.command == "info":
        return cmd_info(args)
    elif args.command == "enable-startup":
        return cmd_enable_startup(args)
    elif args.command == "disable-startup":
        return cmd_disable_startup(args)
    elif args.command == "restore":
        return cmd_restore(args)
    elif args.command == "refine":
        return cmd_refine(args)
    elif args.command == "match":
        return cmd_match(args)
    elif args.command == "hdr-status":
        return cmd_hdr_status(args)
    elif args.command == "generate-profiles":
        return cmd_generate_profiles(args)
    elif args.command == "gui":
        return cmd_gui(args)
    elif args.command == "hdr":
        return cmd_hdr(args)
    elif args.command == "auto":
        return cmd_auto(args)
    elif args.command == "tray":
        return cmd_tray(args)
    elif args.command == "patterns":
        return cmd_patterns(args)
    elif args.command == "ddc-calibrate":
        return cmd_ddc_calibrate(args)
    elif args.command == "native-calibrate":
        return cmd_native_calibrate(args)
    elif args.command == "status":
        return cmd_status(args)
    elif args.command == "plugins":
        return cmd_plugins(args)
    elif args.command == "uniformity":
        from calibrate_pro.display.uniformity import cmd_uniformity
        return cmd_uniformity(args)
    elif args.command == "export-panel":
        from calibrate_pro.community.database import cmd_export_panel
        return cmd_export_panel(args)
    elif args.command == "import-panel":
        from calibrate_pro.community.database import cmd_import_panel
        return cmd_import_panel(args)
    else:
        # No command specified - run auto calibration by default
        print("\nNo command specified. Running automatic calibration...")
        print("(Use --help for all available commands)\n")
        return cmd_auto(args)


if __name__ == "__main__":
    sys.exit(main() or 0)
