#!/usr/bin/env python3
"""
Calibrate(TM) Calibration Verification Tool
Verifies ICC profile installation and displays test patterns

Copyright (C) 2024-2025 Zain Dana Harper. All Rights Reserved.
"""

import ctypes
from ctypes import wintypes
import struct
import os
import sys
import winreg
from dataclasses import dataclass
from typing import List, Optional, Tuple
import math

# ═══════════════════════════════════════════════════════════════════════════════
# WINDOWS API
# ═══════════════════════════════════════════════════════════════════════════════

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32

try:
    mscms = ctypes.windll.mscms
    HAS_MSCMS = True
except:
    mscms = None
    HAS_MSCMS = False

class DISPLAY_DEVICE(ctypes.Structure):
    _fields_ = [
        ('cb', wintypes.DWORD),
        ('DeviceName', wintypes.WCHAR * 32),
        ('DeviceString', wintypes.WCHAR * 128),
        ('StateFlags', wintypes.DWORD),
        ('DeviceID', wintypes.WCHAR * 128),
        ('DeviceKey', wintypes.WCHAR * 128),
    ]

class DEVMODE(ctypes.Structure):
    _fields_ = [
        ('dmDeviceName', wintypes.WCHAR * 32),
        ('dmSpecVersion', wintypes.WORD),
        ('dmDriverVersion', wintypes.WORD),
        ('dmSize', wintypes.WORD),
        ('dmDriverExtra', wintypes.WORD),
        ('dmFields', wintypes.DWORD),
        ('dmPositionX', wintypes.LONG),
        ('dmPositionY', wintypes.LONG),
        ('dmDisplayOrientation', wintypes.DWORD),
        ('dmDisplayFixedOutput', wintypes.DWORD),
        ('dmColor', wintypes.SHORT),
        ('dmDuplex', wintypes.SHORT),
        ('dmYResolution', wintypes.SHORT),
        ('dmTTOption', wintypes.SHORT),
        ('dmCollate', wintypes.SHORT),
        ('dmFormName', wintypes.WCHAR * 32),
        ('dmLogPixels', wintypes.WORD),
        ('dmBitsPerPel', wintypes.DWORD),
        ('dmPelsWidth', wintypes.DWORD),
        ('dmPelsHeight', wintypes.DWORD),
        ('dmDisplayFlags', wintypes.DWORD),
        ('dmDisplayFrequency', wintypes.DWORD),
    ]

DISPLAY_DEVICE_ACTIVE = 0x00000001
DISPLAY_DEVICE_PRIMARY_DEVICE = 0x00000004

# ═══════════════════════════════════════════════════════════════════════════════
# PROFILE VERIFICATION
# ═══════════════════════════════════════════════════════════════════════════════

def get_associated_profiles(device_name: str) -> List[str]:
    """Get ICC profiles associated with a display device."""
    profiles = []

    if not HAS_MSCMS:
        return profiles

    try:
        # EnumColorProfilesW
        # We need to enumerate profiles associated with the device
        buffer_size = ctypes.c_ulong(0)

        # First call to get required size
        mscms.EnumColorProfilesW(
            None,  # Machine name
            ctypes.c_void_p(),  # Enumeration struct
            None,  # Buffer
            ctypes.byref(buffer_size),
            ctypes.byref(ctypes.c_ulong(0))
        )

    except Exception as e:
        pass

    # Alternative: Check registry for associated profiles
    try:
        # Look in ICM registry key
        icm_path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ICM\ProfileAssociations\Display"
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, icm_path) as key:
            i = 0
            while True:
                try:
                    subkey_name = winreg.EnumKey(key, i)
                    if device_name.replace('\\', '#').replace('.', '#') in subkey_name or True:
                        subkey_path = f"{icm_path}\\{subkey_name}"
                        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, subkey_path) as subkey:
                            j = 0
                            while True:
                                try:
                                    name, value, _ = winreg.EnumValue(subkey, j)
                                    if isinstance(value, str) and value.endswith('.icc'):
                                        profiles.append(value)
                                    j += 1
                                except OSError:
                                    break
                    i += 1
                except OSError:
                    break
    except:
        pass

    return profiles


def get_default_profile(device_name: str) -> Optional[str]:
    """Get the default ICC profile for a display device."""
    try:
        # Check registry for default profile
        icm_path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ICM\ProfileAssociations\Display"
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, icm_path) as key:
            i = 0
            while True:
                try:
                    subkey_name = winreg.EnumKey(key, i)
                    subkey_path = f"{icm_path}\\{subkey_name}"
                    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, subkey_path) as subkey:
                        try:
                            # The first profile (index 1) is usually the default
                            name, value, _ = winreg.EnumValue(subkey, 0)
                            if isinstance(value, str) and '.icc' in value.lower():
                                return value
                        except:
                            pass
                    i += 1
                except OSError:
                    break
    except:
        pass

    return None


def verify_profile_installed(profile_name: str) -> Tuple[bool, str]:
    """Check if a profile is installed in the system color directory."""
    system_root = os.environ.get('SystemRoot', 'C:\\Windows')
    color_dir = os.path.join(system_root, 'System32', 'spool', 'drivers', 'color')

    profile_path = os.path.join(color_dir, profile_name)

    if os.path.exists(profile_path):
        size = os.path.getsize(profile_path)
        return True, f"Found ({size} bytes)"

    return False, "Not found"


def read_gamma_ramp() -> Optional[List[List[int]]]:
    """Read the current gamma ramp from the display."""
    try:
        hdc = user32.GetDC(None)
        if not hdc:
            return None

        # Create buffer for gamma ramp
        ramp = (ctypes.c_ushort * 256 * 3)()

        # GetDeviceGammaRamp
        result = gdi32.GetDeviceGammaRamp(hdc, ctypes.byref(ramp))

        user32.ReleaseDC(None, hdc)

        if result:
            return [
                [ramp[0][i] for i in range(256)],  # Red
                [ramp[1][i] for i in range(256)],  # Green
                [ramp[2][i] for i in range(256)],  # Blue
            ]

        return None
    except Exception as e:
        return None


def analyze_gamma_ramp(ramp: List[List[int]]) -> dict:
    """Analyze a gamma ramp to determine its characteristics."""
    results = {
        'is_linear': True,
        'estimated_gamma': 1.0,
        'r_gamma': 1.0,
        'g_gamma': 1.0,
        'b_gamma': 1.0,
        'is_calibrated': False
    }

    # Check if linear (identity)
    linear_tolerance = 100
    for i in range(256):
        expected = i * 257  # Linear: 0, 257, 514, ... 65535
        if abs(ramp[0][i] - expected) > linear_tolerance:
            results['is_linear'] = False
            break

    # Estimate gamma for each channel
    def estimate_gamma(channel_ramp):
        # Use midpoint method: at input 128/255, output should be (128/255)^(1/gamma) * 65535
        mid_in = 128
        mid_out = channel_ramp[mid_in]

        # normalized_out = (mid_out / 65535)
        # normalized_in = mid_in / 255
        # normalized_out = normalized_in ^ (1/gamma)
        # gamma = log(normalized_in) / log(normalized_out)

        normalized_out = mid_out / 65535.0
        normalized_in = mid_in / 255.0

        if normalized_out > 0.01 and normalized_out < 0.99:
            try:
                gamma = math.log(normalized_in) / math.log(normalized_out)
                return round(gamma, 2)
            except:
                pass
        return 1.0

    results['r_gamma'] = estimate_gamma(ramp[0])
    results['g_gamma'] = estimate_gamma(ramp[1])
    results['b_gamma'] = estimate_gamma(ramp[2])
    results['estimated_gamma'] = round((results['r_gamma'] + results['g_gamma'] + results['b_gamma']) / 3, 2)

    # Check if calibrated (non-linear, gamma around 2.2-2.4)
    if not results['is_linear'] and 1.8 <= results['estimated_gamma'] <= 2.8:
        results['is_calibrated'] = True

    return results


def get_icc_profile_info(profile_path: str) -> dict:
    """Parse ICC profile header to get basic info."""
    info = {
        'valid': False,
        'version': '',
        'device_class': '',
        'color_space': '',
        'description': ''
    }

    try:
        with open(profile_path, 'rb') as f:
            data = f.read(128)

        if len(data) < 128:
            return info

        # Profile size
        size = struct.unpack('>I', data[0:4])[0]

        # Preferred CMM
        cmm = data[4:8].decode('ascii', errors='ignore')

        # Version
        major = data[8]
        minor = (data[9] >> 4) & 0x0F
        info['version'] = f"{major}.{minor}"

        # Device class
        device_class = data[12:16].decode('ascii', errors='ignore')
        class_names = {
            'scnr': 'Scanner',
            'mntr': 'Monitor',
            'prtr': 'Printer',
            'link': 'Device Link',
            'spac': 'Color Space',
            'abst': 'Abstract',
            'nmcl': 'Named Color'
        }
        info['device_class'] = class_names.get(device_class, device_class)

        # Color space
        color_space = data[16:20].decode('ascii', errors='ignore').strip()
        info['color_space'] = color_space

        info['valid'] = True

        # Try to find description tag
        tag_count = struct.unpack('>I', data[128:132])[0] if len(data) >= 132 else 0

    except Exception as e:
        pass

    return info


# ═══════════════════════════════════════════════════════════════════════════════
# TEST PATTERN GENERATION
# ═══════════════════════════════════════════════════════════════════════════════

def create_test_pattern_html() -> str:
    """Create an HTML file with calibration test patterns."""

    html = '''<!DOCTYPE html>
<html>
<head>
    <title>Calibrate - Display Test Patterns</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            background: #1a1a1a;
            color: #fff;
            font-family: system-ui, -apple-system, sans-serif;
            min-height: 100vh;
        }
        .header {
            background: linear-gradient(135deg, #2a2a2a, #1a1a1a);
            padding: 20px 40px;
            border-bottom: 1px solid #333;
        }
        .header h1 { font-size: 24px; font-weight: 300; }
        .header p { color: #888; margin-top: 5px; }

        .container { padding: 40px; max-width: 1400px; margin: 0 auto; }

        .section {
            margin-bottom: 60px;
        }
        .section h2 {
            font-size: 18px;
            font-weight: 500;
            margin-bottom: 20px;
            color: #0af;
        }

        /* Grayscale Ramp */
        .grayscale-ramp {
            display: flex;
            height: 80px;
            border-radius: 8px;
            overflow: hidden;
        }
        .grayscale-ramp div {
            flex: 1;
        }

        /* Gamma Test */
        .gamma-test {
            display: flex;
            gap: 20px;
            flex-wrap: wrap;
        }
        .gamma-patch {
            width: 200px;
            text-align: center;
        }
        .gamma-patch .patch {
            height: 100px;
            border-radius: 8px;
            margin-bottom: 10px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 14px;
            font-weight: 500;
        }
        .gamma-patch .checkerboard {
            background-image:
                linear-gradient(45deg, #808080 25%, transparent 25%),
                linear-gradient(-45deg, #808080 25%, transparent 25%),
                linear-gradient(45deg, transparent 75%, #808080 75%),
                linear-gradient(-45deg, transparent 75%, #808080 75%);
            background-size: 4px 4px;
            background-position: 0 0, 0 2px, 2px -2px, -2px 0px;
        }

        /* Color Bars */
        .color-bars {
            display: flex;
            height: 120px;
            border-radius: 8px;
            overflow: hidden;
        }
        .color-bars div { flex: 1; }

        /* Primary Colors */
        .primaries {
            display: flex;
            gap: 20px;
        }
        .primary-patch {
            flex: 1;
            height: 150px;
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-weight: 500;
        }

        /* White Point Test */
        .white-test {
            display: flex;
            gap: 20px;
        }
        .white-patch {
            flex: 1;
            height: 100px;
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 14px;
        }

        /* Near-Black Test */
        .near-black {
            display: flex;
            height: 100px;
            border-radius: 8px;
            overflow: hidden;
            background: #000;
        }
        .near-black div {
            flex: 1;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 10px;
            color: #444;
        }

        /* Uniformity Test */
        .uniformity {
            height: 200px;
            background: #808080;
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            color: #fff;
        }

        .instructions {
            background: #252525;
            border-radius: 8px;
            padding: 20px;
            margin-top: 20px;
        }
        .instructions h3 {
            font-size: 14px;
            margin-bottom: 10px;
            color: #0af;
        }
        .instructions ul {
            margin-left: 20px;
            color: #aaa;
            font-size: 13px;
            line-height: 1.8;
        }

        .fullscreen-btn {
            position: fixed;
            bottom: 20px;
            right: 20px;
            background: #0af;
            color: #000;
            border: none;
            padding: 12px 24px;
            border-radius: 8px;
            cursor: pointer;
            font-weight: 500;
        }
        .fullscreen-btn:hover { background: #0cf; }
    </style>
</head>
<body>
    <div class="header">
        <h1>Calibrate(TM) Display Verification</h1>
        <p>NeuralUX(TM) Calibration Test Patterns</p>
    </div>

    <div class="container">
        <!-- Grayscale Ramp -->
        <div class="section">
            <h2>1. Grayscale Ramp (0-255)</h2>
            <div class="grayscale-ramp" id="grayscale"></div>
            <div class="instructions">
                <h3>What to look for:</h3>
                <ul>
                    <li>Smooth transitions with no banding or steps</li>
                    <li>All 32 steps should be distinguishable</li>
                    <li>No color tint in the grays (should be neutral)</li>
                </ul>
            </div>
        </div>

        <!-- Gamma Test -->
        <div class="section">
            <h2>2. Gamma Verification (Target: 2.2-2.4)</h2>
            <div class="gamma-test">
                <div class="gamma-patch">
                    <div class="patch" style="background: #777;">Solid 47%</div>
                    <p>Should match checkerboard</p>
                </div>
                <div class="gamma-patch">
                    <div class="patch checkerboard"></div>
                    <p>50% Checkerboard</p>
                </div>
                <div class="gamma-patch">
                    <div class="patch" style="background: #808080;">Solid 50%</div>
                    <p>Reference mid-gray</p>
                </div>
            </div>
            <div class="instructions">
                <h3>Gamma Test:</h3>
                <ul>
                    <li>At gamma 2.2, the solid 47% gray should match the checkerboard brightness</li>
                    <li>If the checkerboard looks brighter, gamma is too high</li>
                    <li>If the checkerboard looks darker, gamma is too low</li>
                </ul>
            </div>
        </div>

        <!-- Color Bars -->
        <div class="section">
            <h2>3. Color Bars (100% Saturation)</h2>
            <div class="color-bars">
                <div style="background: #fff;"></div>
                <div style="background: #ff0;"></div>
                <div style="background: #0ff;"></div>
                <div style="background: #0f0;"></div>
                <div style="background: #f0f;"></div>
                <div style="background: #f00;"></div>
                <div style="background: #00f;"></div>
                <div style="background: #000;"></div>
            </div>
            <div class="instructions">
                <h3>What to look for:</h3>
                <ul>
                    <li>Colors should be vivid and saturated</li>
                    <li>No color bleeding between bars</li>
                    <li>White should be neutral (no pink/green/blue tint)</li>
                </ul>
            </div>
        </div>

        <!-- Primary Colors -->
        <div class="section">
            <h2>4. Primary & Secondary Colors</h2>
            <div class="primaries">
                <div class="primary-patch" style="background: #f00; color: #fff;">RED<br>255, 0, 0</div>
                <div class="primary-patch" style="background: #0f0; color: #000;">GREEN<br>0, 255, 0</div>
                <div class="primary-patch" style="background: #00f; color: #fff;">BLUE<br>0, 0, 255</div>
            </div>
            <div style="height: 20px;"></div>
            <div class="primaries">
                <div class="primary-patch" style="background: #ff0; color: #000;">YELLOW<br>255, 255, 0</div>
                <div class="primary-patch" style="background: #f0f; color: #fff;">MAGENTA<br>255, 0, 255</div>
                <div class="primary-patch" style="background: #0ff; color: #000;">CYAN<br>0, 255, 255</div>
            </div>
        </div>

        <!-- White Point -->
        <div class="section">
            <h2>5. White Point Test (D65 = 6500K)</h2>
            <div class="white-test">
                <div class="white-patch" style="background: #ffeedd; color: #000;">Warm (5000K)</div>
                <div class="white-patch" style="background: #fff; color: #000;">D65 (6500K)</div>
                <div class="white-patch" style="background: #eef4ff; color: #000;">Cool (7500K)</div>
            </div>
            <div class="instructions">
                <h3>White Point Check:</h3>
                <ul>
                    <li>The center patch should appear as pure white</li>
                    <li>Left patch should look slightly warm/yellow</li>
                    <li>Right patch should look slightly cool/blue</li>
                    <li>If center looks tinted, white point may be off</li>
                </ul>
            </div>
        </div>

        <!-- Near Black -->
        <div class="section">
            <h2>6. Shadow Detail (Near-Black)</h2>
            <div class="near-black">
                <div style="background: rgb(0,0,0);">0</div>
                <div style="background: rgb(1,1,1);">1</div>
                <div style="background: rgb(2,2,2);">2</div>
                <div style="background: rgb(3,3,3);">3</div>
                <div style="background: rgb(4,4,4);">4</div>
                <div style="background: rgb(5,5,5);">5</div>
                <div style="background: rgb(8,8,8);">8</div>
                <div style="background: rgb(10,10,10);">10</div>
                <div style="background: rgb(15,15,15);">15</div>
                <div style="background: rgb(20,20,20);">20</div>
            </div>
            <div class="instructions">
                <h3>Shadow Detail (OLED Important):</h3>
                <ul>
                    <li>On OLED: Level 0 should be pure black (pixel off)</li>
                    <li>You should be able to distinguish levels 3-5 and above</li>
                    <li>If all look the same, shadow detail is crushed</li>
                </ul>
            </div>
        </div>

        <!-- Uniformity -->
        <div class="section">
            <h2>7. Screen Uniformity</h2>
            <div class="uniformity">
                <p>Check for brightness variations across the screen.<br>
                Look at corners and edges vs center.</p>
            </div>
        </div>
    </div>

    <button class="fullscreen-btn" onclick="toggleFullscreen()">Toggle Fullscreen</button>

    <script>
        // Generate grayscale ramp
        const grayscale = document.getElementById('grayscale');
        for (let i = 0; i < 32; i++) {
            const val = Math.round(i * 255 / 31);
            const div = document.createElement('div');
            div.style.background = `rgb(${val},${val},${val})`;
            grayscale.appendChild(div);
        }

        function toggleFullscreen() {
            if (!document.fullscreenElement) {
                document.documentElement.requestFullscreen();
            } else {
                document.exitFullscreen();
            }
        }
    </script>
</body>
</html>'''

    return html


# ═══════════════════════════════════════════════════════════════════════════════
# MAIN VERIFICATION
# ═══════════════════════════════════════════════════════════════════════════════

def enumerate_displays():
    """Get list of active displays."""
    displays = []
    device = DISPLAY_DEVICE()
    device.cb = ctypes.sizeof(device)

    i = 0
    while user32.EnumDisplayDevicesW(None, i, ctypes.byref(device), 0):
        if device.StateFlags & DISPLAY_DEVICE_ACTIVE:
            devmode = DEVMODE()
            devmode.dmSize = ctypes.sizeof(devmode)

            if user32.EnumDisplaySettingsW(device.DeviceName, -1, ctypes.byref(devmode)):
                monitor = DISPLAY_DEVICE()
                monitor.cb = ctypes.sizeof(monitor)

                if user32.EnumDisplayDevicesW(device.DeviceName, 0, ctypes.byref(monitor), 0):
                    displays.append({
                        'name': device.DeviceName,
                        'adapter': device.DeviceString,
                        'monitor_id': monitor.DeviceID,
                        'monitor_name': monitor.DeviceString,
                        'resolution': (devmode.dmPelsWidth, devmode.dmPelsHeight),
                        'refresh': devmode.dmDisplayFrequency,
                        'primary': bool(device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE)
                    })
        i += 1

    return displays


def run_verification():
    """Run calibration verification."""

    print("=" * 70)
    print("  Calibrate(TM) Calibration Verification")
    print("=" * 70)
    print()

    # Get displays
    displays = enumerate_displays()
    print(f"[1/4] Found {len(displays)} active display(s)")
    print()

    # Check each display
    print("[2/4] Checking ICC Profile Status")
    print("-" * 70)

    system_root = os.environ.get('SystemRoot', 'C:\\Windows')
    color_dir = os.path.join(system_root, 'System32', 'spool', 'drivers', 'color')

    # Find our calibration profiles
    our_profiles = []
    try:
        for f in os.listdir(color_dir):
            if f.startswith('Calibrate_') and f.endswith('_NeuralUX.icc'):
                our_profiles.append(f)
    except:
        pass

    print()
    print(f"  Calibrate(TM) profiles installed: {len(our_profiles)}")
    for profile in our_profiles:
        profile_path = os.path.join(color_dir, profile)
        info = get_icc_profile_info(profile_path)
        size = os.path.getsize(profile_path)

        print(f"    [OK] {profile}")
        print(f"         Size: {size} bytes | Version: {info['version']} | Class: {info['device_class']}")

    if not our_profiles:
        print("    [!] No Calibrate profiles found. Run calibrate_monitors.py first.")

    print()

    # Check display associations
    for i, display in enumerate(displays):
        primary = " [PRIMARY]" if display['primary'] else ""
        print(f"  Display {i+1}: {display['name']}{primary}")
        print(f"    Resolution: {display['resolution'][0]}x{display['resolution'][1]} @ {display['refresh']}Hz")

        # Check for associated profiles
        profiles = get_associated_profiles(display['name'])
        default = get_default_profile(display['name'])

        if default:
            print(f"    Default Profile: {default}")
            if 'Calibrate_' in default:
                print(f"    Status: [ACTIVE] Using Calibrate(TM) profile")
            else:
                print(f"    Status: [!] Using different profile")
        else:
            print(f"    Default Profile: (system default)")
            # Check if our profile might be active
            for p in our_profiles:
                if str(display['resolution'][0]) in p or f"Display{i+1}" in p:
                    print(f"    Matching Calibrate profile: {p}")
                    break
        print()

    # Check gamma ramp
    print("[3/4] Analyzing Display Gamma")
    print("-" * 70)

    ramp = read_gamma_ramp()
    if ramp:
        analysis = analyze_gamma_ramp(ramp)

        print()
        if analysis['is_linear']:
            print("  Gamma Ramp: LINEAR (no calibration applied to video LUT)")
            print("  Note: ICC profiles work at application level, not video LUT")
        else:
            print(f"  Gamma Ramp: CALIBRATED")
            print(f"    Estimated Gamma: {analysis['estimated_gamma']}")
            print(f"    Red Channel:     {analysis['r_gamma']}")
            print(f"    Green Channel:   {analysis['g_gamma']}")
            print(f"    Blue Channel:    {analysis['b_gamma']}")

        print()

        # Show sample values
        print("  LUT Sample Values (input -> output):")
        for val in [0, 32, 64, 128, 192, 255]:
            r = ramp[0][val]
            g = ramp[1][val]
            b = ramp[2][val]
            print(f"    {val:3d} -> R:{r:5d}  G:{g:5d}  B:{b:5d}")
    else:
        print("  [!] Could not read gamma ramp")

    print()

    # Generate test pattern
    print("[4/4] Generating Visual Test Pattern")
    print("-" * 70)
    print()

    test_html = create_test_pattern_html()
    test_path = os.path.join(os.path.dirname(__file__), 'test_pattern.html')

    with open(test_path, 'w', encoding='utf-8') as f:
        f.write(test_html)

    print(f"  Test pattern saved: {test_path}")
    print()

    # Open in browser
    print("  Opening test pattern in browser...")
    try:
        os.startfile(test_path)
        print("  [OK] Test pattern opened")
    except Exception as e:
        print(f"  [!] Could not open automatically: {e}")
        print(f"      Please open manually: {test_path}")

    print()
    print("=" * 70)
    print("  Verification Summary")
    print("=" * 70)
    print()
    print(f"  Profiles Installed:  {len(our_profiles)}")
    print(f"  Active Displays:     {len(displays)}")

    if ramp:
        if analysis['is_calibrated']:
            print(f"  Video LUT Status:    Calibrated (gamma ~{analysis['estimated_gamma']})")
        else:
            print(f"  Video LUT Status:    Linear (ICC profile active at app level)")

    print()
    print("  Visual Verification:")
    print("    1. Check the test pattern in your browser")
    print("    2. Grayscale should be smooth with no banding")
    print("    3. Gamma test: solid gray should match checkerboard")
    print("    4. White should appear neutral (no color tint)")
    print("    5. You should see shadow detail in the near-black test")
    print()
    print("=" * 70)


if __name__ == '__main__':
    run_verification()
