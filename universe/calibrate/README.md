# Calibrate Pro

Professional display calibration for Windows. Sensorless or measured.

Calibrate Pro detects your monitors, identifies their panel characteristics from a database of 39+ characterized displays, and applies color corrections via 3D LUTs and DDC/CI hardware adjustments. Works without a colorimeter (sensorless mode) or with an i1Display3 for measured accuracy.

## Quick Start

```bash
pip install ".[gui]"
calibrate-pro auto
```

Or launch the GUI:

```bash
calibrate-pro gui
```

## How It Works

1. **Detects** all connected displays via Windows APIs and EDID parsing
2. **Identifies** each panel from a database of 39 characterized monitors (QD-OLED, WOLED, IPS, VA, Mini-LED)
3. **Adjusts** hardware settings via DDC/CI (brightness, contrast, RGB gains)
4. **Generates** an ICC v4 profile and 33x33x33 3D LUT with Oklab perceptual gamut mapping
5. **Applies** the LUT system-wide via [dwm_lut](https://github.com/ledoge/dwm_lut) (with VCGT gamma ramp fallback)
6. **Guards** calibration against Windows resetting it (background service monitors VCGT state)
7. **Persists** across reboots via Windows startup registration

## Calibration Modes

| Mode | Requires | Accuracy |
|------|----------|----------|
| **Sensorless** | Nothing | Predicted dE < 1.0 (panel-database dependent) |
| **Native USB** | i1Display3 colorimeter | Measured dE ~4.2 (36% improvement over uncalibrated) |
| **Hybrid** | i1Display3 + ArgyllCMS | Measured + iterative refinement |

### Native USB Colorimeter

Calibrate Pro includes a native USB HID driver for the X-Rite i1Display3 family (including NEC MDSVSENSOR3, ColorMunki Display, Calibrite ColorChecker Display). No ArgyllCMS or DisplayCAL required.

```bash
calibrate-pro native-calibrate --apply
```

This profiles your display's per-channel transfer response curves and primary chromaticities, then builds a chroma-adaptive 3D correction LUT that compresses the wide gamut to sRGB while preserving neutral accuracy.

## Commands

| Command | Description |
|---------|-------------|
| `calibrate-pro auto` | Calibrate all displays automatically (sensorless) |
| `calibrate-pro native-calibrate` | Calibrate with i1Display3 native USB driver |
| `calibrate-pro gui` | Launch the calibration GUI |
| `calibrate-pro detect` | List connected displays and panel matches |
| `calibrate-pro verify` | Verify calibration accuracy (ColorChecker) |
| `calibrate-pro restore` | Undo calibration (reset to defaults) |
| `calibrate-pro ddc-calibrate` | DDC/CI hardware-first calibration |
| `calibrate-pro generate-profiles` | Generate sRGB, P3, Rec.709, AdobeRGB profiles |
| `calibrate-pro patterns` | Display fullscreen test patterns |
| `calibrate-pro tray` | Launch system tray application |
| `calibrate-pro status` | Show calibration age and drift status |
| `calibrate-pro hdr-status` | Show HDR mode status |
| `calibrate-pro list-panels` | Show all 39 supported panel profiles |

Run `calibrate-pro --help` for the full command list.

## Installation

### From source

```bash
git clone <repository>
cd calibrate
pip install ".[gui]"
```

### Dependencies

**Required:**
- Python 3.10+
- numpy, scipy

**Platform support:**
- **Windows 10/11** - Full support (DWM 3D LUT, VCGT, DDC/CI, ICC, calibration guard)
- **macOS** - Display detection (CoreGraphics), gamma ramps (CGSetDisplayTransferByTable), ICC profiles (ColorSync). Requires `pip install ".[macos]"` for pyobjc bindings.
- **Linux** - Planned (xrandr/colord integration)

**Recommended (Windows):**
- [dwm_lut](https://github.com/ledoge/dwm_lut) - System-wide 3D LUT via DWM compositor. Calibrate Pro auto-launches it with elevation.

**Recommended (all platforms):**
- PyQt6 - For the GUI (`pip install ".[gui]"`)
- hidapi - For native i1Display3 driver (`pip install hidapi`)
- pystray + Pillow - For system tray mode (`pip install ".[tray]"`)

## Output Files

Each calibration produces (saved to `~/Documents/Calibrate Pro/Calibrations/`):

| File | Usage |
|------|-------|
| `.cube` | 33x33x33 3D LUT - DaVinci Resolve, OBS, any LUT-capable app |
| `.icc` | ICC v4 profile - Windows Color Management |
| `.3dlut` | MadVR format |
| `_reshade.png` | ReShade LUT texture |
| `_specialk.png` | Special K LUT texture |
| `_obs.cube` | OBS Studio LUT |
| `_mpv.conf` | mpv player config snippet |
| `_report.html` | Calibration report with CIE diagram, gamma curves, gamut coverage |

## Supported Displays

39 characterized panels including:

- **QD-OLED**: ASUS PG27UCDM, Samsung Odyssey G8/G9, Dell AW3225QF, MSI 321URX
- **WOLED**: LG C3/C4/G4, ASUS PG42UQ, Dell AW3423DWF
- **IPS**: Dell U2723QE, ASUS ProArt PA278QV, BenQ PD2706U, EIZO CS2740
- **VA**: Samsung Odyssey G7, Dell S2722QC, Gigabyte M27Q
- **Mini-LED**: ASUS PG32UCDM, Samsung Odyssey Neo G8

Unknown monitors are calibrated using EDID chromaticity data extracted from the display's firmware.

## Architecture

```
calibrate_pro/
  core/           Color math, LUT engine, ICC profiles
  panels/         Display detection, panel database (39 profiles)
  sensorless/     Sensorless calibration engine
  calibration/    Native measurement loop, hybrid refinement
  hardware/       i1Display3 native USB, DDC/CI, ArgyllCMS backend
  lut_system/     DWM 3D LUT, VCGT gamma ramp
  services/       CalibrationGuard, GamutClamp, AppSwitcher, DriftMonitor
  gui/            PyQt6 application with warm pastel theme
  startup/        Boot-time calibration restoration
```

## Color Science

Built on a complete color science library:

- **Perceptual spaces**: Oklab/Oklch, JzAzBz/JzCzhz, ICtCp, CAM16-UCS, CIE Lab/Luv
- **Transfer functions**: PQ (ST.2084), HLG (BT.2100), sRGB, BT.1886, BT.2390 EETF
- **Color spaces**: sRGB, Display P3, Rec.2020, AdobeRGB, ACES (ACEScg, ACEScc, ACEScct)
- **Gamut mapping**: Oklab perceptual compression (SDR), JzCzhz for HDR
- **Chromatic adaptation**: Bradford transform with exact ICC D50/D65 illuminants
- **Verification**: CIEDE2000 + CAM16-UCS dual-metric against ColorChecker Classic

## License

Copyright 2024-2026 Zain Dana Harper. All rights reserved.
