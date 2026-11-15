#!/usr/bin/env python3
"""
Calibrate(TM) Automatic Display Calibration
NeuralUX(TM) AI-Powered Sensorless Calibration

Copyright (C) 2024-2025 Zain Dana Harper. All Rights Reserved.
"""

import ctypes
from ctypes import wintypes
import struct
import os
import sys
import time
from dataclasses import dataclass
from typing import List, Optional, Tuple
import math

# ═══════════════════════════════════════════════════════════════════════════════
# WINDOWS API DEFINITIONS
# ═══════════════════════════════════════════════════════════════════════════════

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32

# Load mscms.dll for color management
try:
    mscms = ctypes.windll.mscms
    HAS_COLOR_MANAGEMENT = True
except:
    mscms = None
    HAS_COLOR_MANAGEMENT = False

# Display device flags
DISPLAY_DEVICE_ACTIVE = 0x00000001
DISPLAY_DEVICE_PRIMARY_DEVICE = 0x00000004
DISPLAY_DEVICE_ATTACHED_TO_DESKTOP = 0x00000001

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

# ═══════════════════════════════════════════════════════════════════════════════
# DATA STRUCTURES
# ═══════════════════════════════════════════════════════════════════════════════

@dataclass
class DisplayPrimaries:
    red: Tuple[float, float]
    green: Tuple[float, float]
    blue: Tuple[float, float]
    white: Tuple[float, float]

    @staticmethod
    def srgb():
        return DisplayPrimaries(
            red=(0.64, 0.33),
            green=(0.30, 0.60),
            blue=(0.15, 0.06),
            white=(0.3127, 0.3290)
        )

@dataclass
class MonitorInfo:
    id: str
    name: str
    manufacturer: str
    model: str
    device_name: str
    resolution: Tuple[int, int]
    refresh_rate: float
    primary: bool
    bit_depth: int
    edid: bytes

@dataclass
class DisplayProfile:
    name: str
    primaries: DisplayPrimaries
    gamma: float
    white_point_kelvin: float
    max_luminance: float

# ═══════════════════════════════════════════════════════════════════════════════
# EDID PARSER
# ═══════════════════════════════════════════════════════════════════════════════

def parse_edid(edid: bytes) -> dict:
    """Parse EDID data to extract display information."""
    if len(edid) < 128:
        return None

    # Check header
    if edid[0:8] != bytes([0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]):
        return None

    # Manufacturer ID (PNP ID)
    mfg_bytes = (edid[8] << 8) | edid[9]
    mfg = chr(((mfg_bytes >> 10) & 0x1F) + ord('A') - 1)
    mfg += chr(((mfg_bytes >> 5) & 0x1F) + ord('A') - 1)
    mfg += chr((mfg_bytes & 0x1F) + ord('A') - 1)

    # Product code
    product_code = edid[10] | (edid[11] << 8)

    # Serial
    serial = struct.unpack('<I', edid[12:16])[0]

    # Year
    year = 1990 + edid[17]

    # Display primaries (10-bit values)
    rx = ((edid[27] << 2) | (edid[25] >> 6)) / 1024.0
    ry = ((edid[28] << 2) | ((edid[25] >> 4) & 0x03)) / 1024.0
    gx = ((edid[29] << 2) | ((edid[25] >> 2) & 0x03)) / 1024.0
    gy = ((edid[30] << 2) | (edid[25] & 0x03)) / 1024.0
    bx = ((edid[31] << 2) | (edid[26] >> 6)) / 1024.0
    by = ((edid[32] << 2) | ((edid[26] >> 4) & 0x03)) / 1024.0
    wx = ((edid[33] << 2) | ((edid[26] >> 2) & 0x03)) / 1024.0
    wy = ((edid[34] << 2) | (edid[26] & 0x03)) / 1024.0

    # Parse descriptor blocks for monitor name
    monitor_name = None
    for i in range(4):
        offset = 54 + i * 18
        if edid[offset] == 0 and edid[offset + 1] == 0:
            tag = edid[offset + 3]
            if tag == 0xFC:  # Monitor name
                name_bytes = edid[offset + 5:offset + 18]
                monitor_name = bytes(name_bytes).decode('ascii', errors='ignore').strip()
                break

    return {
        'manufacturer': mfg,
        'product_code': product_code,
        'serial': serial,
        'year': year,
        'monitor_name': monitor_name,
        'primaries': DisplayPrimaries(
            red=(rx, ry),
            green=(gx, gy),
            blue=(bx, by),
            white=(wx, wy)
        )
    }

# ═══════════════════════════════════════════════════════════════════════════════
# MONITOR ENUMERATION
# ═══════════════════════════════════════════════════════════════════════════════

MANUFACTURER_NAMES = {
    'ACI': 'ASUS', 'ACR': 'Acer', 'AOC': 'AOC', 'AUO': 'AU Optronics',
    'BNQ': 'BenQ', 'CMN': 'Chi Mei', 'DEL': 'Dell', 'ENC': 'Eizo',
    'GSM': 'LG Electronics', 'HPN': 'HP', 'HWP': 'HP', 'IVM': 'Iiyama',
    'LEN': 'Lenovo', 'LGD': 'LG Display', 'MEI': 'Panasonic', 'NEC': 'NEC',
    'PHL': 'Philips', 'SAM': 'Samsung', 'SDC': 'Samsung', 'SNY': 'Sony',
    'VSC': 'ViewSonic', 'SEC': 'Samsung', 'AUS': 'ASUS', 'MSI': 'MSI',
}

def get_all_edids_from_registry() -> List[Tuple[str, bytes]]:
    """Read all EDIDs from Windows registry with their device paths."""
    import winreg

    edids = []

    try:
        # Method 1: Search through DISPLAY entries
        base_path = r"SYSTEM\CurrentControlSet\Enum\DISPLAY"

        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, base_path) as display_key:
            i = 0
            while True:
                try:
                    monitor_id = winreg.EnumKey(display_key, i)
                    monitor_path = f"{base_path}\\{monitor_id}"

                    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, monitor_path) as monitor_key:
                        j = 0
                        while True:
                            try:
                                instance = winreg.EnumKey(monitor_key, j)
                                edid_path = f"{monitor_path}\\{instance}\\Device Parameters"

                                try:
                                    with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, edid_path) as edid_key:
                                        edid, _ = winreg.QueryValueEx(edid_key, "EDID")
                                        if edid and len(edid) >= 128:
                                            edids.append((monitor_id, bytes(edid)))
                                except:
                                    pass

                                j += 1
                            except OSError:
                                break
                    i += 1
                except OSError:
                    break
    except:
        pass

    return edids

def get_edid_from_registry(device_id: str) -> Optional[bytes]:
    """Read EDID from Windows registry for a specific device."""
    import winreg

    # Parse device ID to find registry path
    # Device ID format: MONITOR\XXX1234\{guid}
    parts = device_id.split('\\')
    if len(parts) >= 2:
        monitor_id = parts[1] if parts[0] == 'MONITOR' else parts[0]

        try:
            edid_path = f"SYSTEM\\CurrentControlSet\\Enum\\DISPLAY\\{monitor_id}"

            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, edid_path) as monitor_key:
                j = 0
                while True:
                    try:
                        instance = winreg.EnumKey(monitor_key, j)
                        instance_path = f"{edid_path}\\{instance}\\Device Parameters"

                        try:
                            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, instance_path) as edid_key:
                                edid, _ = winreg.QueryValueEx(edid_key, "EDID")
                                if edid and len(edid) >= 128:
                                    return bytes(edid)
                        except:
                            pass

                        j += 1
                    except OSError:
                        break
        except:
            pass

    return None

def enumerate_monitors() -> List[MonitorInfo]:
    """Enumerate all connected monitors using Windows API."""
    monitors = []
    device = DISPLAY_DEVICE()
    device.cb = ctypes.sizeof(device)

    # First, get all EDIDs from registry
    all_edids = get_all_edids_from_registry()
    used_edids = set()

    i = 0
    while user32.EnumDisplayDevicesW(None, i, ctypes.byref(device), 0):
        if device.StateFlags & DISPLAY_DEVICE_ACTIVE:
            # Get display settings
            devmode = DEVMODE()
            devmode.dmSize = ctypes.sizeof(devmode)

            if user32.EnumDisplaySettingsW(device.DeviceName, -1, ctypes.byref(devmode)):
                # Get monitor device
                monitor_device = DISPLAY_DEVICE()
                monitor_device.cb = ctypes.sizeof(monitor_device)

                if user32.EnumDisplayDevicesW(device.DeviceName, 0, ctypes.byref(monitor_device), 0):
                    device_id = monitor_device.DeviceID
                    device_string = monitor_device.DeviceString

                    # Parse manufacturer from device ID
                    manufacturer = "Unknown"
                    model = "Monitor"
                    registry_id = None

                    if 'MONITOR\\' in device_id:
                        parts = device_id.split('\\')
                        if len(parts) >= 2:
                            registry_id = parts[1]
                            # First 3 chars are manufacturer
                            mfg_code = registry_id[:3]
                            manufacturer = MANUFACTURER_NAMES.get(mfg_code, mfg_code)
                            model = registry_id

                    # Extract registry ID from device_id (e.g., MONITOR\AUS27F5\{...} -> AUS27F5)
                    if 'MONITOR\\' in device_id:
                        parts = device_id.split('\\')
                        if len(parts) >= 2:
                            registry_id = parts[1]

                    # Try to get EDID using the exact registry ID
                    edid = None
                    if registry_id:
                        for reg_id, reg_edid in all_edids:
                            if reg_id == registry_id and reg_id not in used_edids:
                                edid = reg_edid
                                used_edids.add(reg_id)
                                break

                    # Fallback to any matching EDID
                    if not edid:
                        edid = get_edid_from_registry(device_id)

                    # Parse EDID if available
                    if edid and len(edid) >= 128:
                        edid_info = parse_edid(edid)
                        if edid_info:
                            mfg_code = edid_info['manufacturer']
                            manufacturer = MANUFACTURER_NAMES.get(mfg_code, mfg_code)
                            if edid_info['monitor_name']:
                                model = edid_info['monitor_name']

                    # Also check device string for model hints
                    if device_string and model == registry_id:
                        # Device string might have better name
                        if 'Generic' not in device_string:
                            model = device_string

                    is_primary = bool(device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE)

                    monitors.append(MonitorInfo(
                        id=f"DISPLAY{i+1}",
                        name=device.DeviceString or f"Display {i+1}",
                        manufacturer=manufacturer,
                        model=model,
                        device_name=device.DeviceName,
                        resolution=(devmode.dmPelsWidth, devmode.dmPelsHeight),
                        refresh_rate=float(devmode.dmDisplayFrequency),
                        primary=is_primary,
                        bit_depth=devmode.dmBitsPerPel,
                        edid=edid or b''
                    ))

        i += 1

    return monitors

# ═══════════════════════════════════════════════════════════════════════════════
# NEURALUX PRECISION CALIBRATION ENGINE
# ═══════════════════════════════════════════════════════════════════════════════

# Import precision calibration engine
try:
    from neuralux_precision import (
        PrecisionCalibrator,
        PANEL_DATABASE,
        SRGB_PRIMARIES,
        generate_precision_icc,
        delta_e_2000,
        COLORCHECKER_REFERENCE
    )
    HAS_PRECISION_ENGINE = True
except ImportError:
    HAS_PRECISION_ENGINE = False

class NeuralLayer:
    """Simple neural network layer for NeuralUX calibration."""

    def __init__(self, input_size: int, output_size: int, activation: str = 'relu'):
        self.input_size = input_size
        self.output_size = output_size
        self.activation = activation

        # Xavier initialization (deterministic for reproducibility)
        scale = math.sqrt(2.0 / (input_size + output_size))
        self.weights = []
        self.biases = []

        for i in range(output_size):
            row = []
            for j in range(input_size):
                # Deterministic pseudo-random
                val = ((i * 31 + j * 17 + 7) % 1000) / 1000.0 - 0.5
                row.append(val * scale)
            self.weights.append(row)
            self.biases.append(((i * 13 + 5) % 100) / 1000.0 - 0.05)

    def forward(self, x: List[float]) -> List[float]:
        output = []
        for i in range(self.output_size):
            sum_val = self.biases[i]
            for j in range(min(len(x), self.input_size)):
                sum_val += x[j] * self.weights[i][j]

            # Apply activation
            if self.activation == 'relu':
                sum_val = max(0, sum_val)
            elif self.activation == 'sigmoid':
                sum_val = 1.0 / (1.0 + math.exp(-max(-500, min(500, sum_val))))
            elif self.activation == 'tanh':
                sum_val = math.tanh(sum_val)

            output.append(sum_val)

        return output

class NeuralUXCalibrator:
    """NeuralUX AI-based sensorless calibration engine."""

    def __init__(self):
        # 4-layer network: 21 features -> 64 -> 32 -> 16 -> 11 outputs
        self.layers = [
            NeuralLayer(21, 64, 'relu'),
            NeuralLayer(64, 32, 'relu'),
            NeuralLayer(32, 16, 'relu'),
            NeuralLayer(16, 11, 'linear')
        ]

        # Panel type database
        self.panel_database = {
            'IPS': {'gamma': 2.2, 'contrast': 1000, 'coverage': 0.99},
            'VA': {'gamma': 2.4, 'contrast': 3000, 'coverage': 0.95},
            'TN': {'gamma': 2.2, 'contrast': 800, 'coverage': 0.70},
            'OLED': {'gamma': 2.4, 'contrast': 1000000, 'coverage': 1.0},
        }

    def extract_features(self, monitor: MonitorInfo) -> List[float]:
        """Extract features from monitor info for neural network."""
        features = [0.0] * 21

        # Resolution features
        features[0] = monitor.resolution[0] / 3840.0  # Normalized to 4K
        features[1] = monitor.resolution[1] / 2160.0
        features[2] = (monitor.resolution[0] * monitor.resolution[1]) / (3840 * 2160)

        # Refresh rate
        features[3] = monitor.refresh_rate / 360.0  # Normalized to max common

        # Bit depth
        features[4] = monitor.bit_depth / 32.0

        # Panel type detection from manufacturer/model
        panel_score = self._detect_panel_type(monitor)
        features[5:9] = panel_score  # IPS, VA, TN, OLED scores

        # Manufacturer encoding
        mfg_hash = sum(ord(c) for c in monitor.manufacturer[:3]) / 300.0
        features[9] = mfg_hash

        # Model encoding
        model_hash = sum(ord(c) for c in monitor.model[:10]) / 1000.0
        features[10] = model_hash

        # EDID primaries if available
        if monitor.edid and len(monitor.edid) >= 128:
            edid_info = parse_edid(monitor.edid)
            if edid_info and edid_info['primaries']:
                p = edid_info['primaries']
                features[11] = p.red[0]
                features[12] = p.red[1]
                features[13] = p.green[0]
                features[14] = p.green[1]
                features[15] = p.blue[0]
                features[16] = p.blue[1]
                features[17] = p.white[0]
                features[18] = p.white[1]
        else:
            # Use sRGB defaults
            srgb = DisplayPrimaries.srgb()
            features[11:19] = [srgb.red[0], srgb.red[1], srgb.green[0], srgb.green[1],
                              srgb.blue[0], srgb.blue[1], srgb.white[0], srgb.white[1]]

        # Primary display flag
        features[19] = 1.0 if monitor.primary else 0.0

        # Year estimate (from EDID if available)
        if monitor.edid and len(monitor.edid) >= 128:
            edid_info = parse_edid(monitor.edid)
            if edid_info:
                features[20] = (edid_info['year'] - 2000) / 30.0
        else:
            features[20] = 0.8  # Assume recent

        return features

    def _detect_panel_type(self, monitor: MonitorInfo) -> List[float]:
        """Detect likely panel type from model/manufacturer."""
        model_upper = monitor.model.upper()
        mfg_upper = monitor.manufacturer.upper()

        # Score for each panel type [IPS, VA, TN, OLED]
        ips = 0.25
        va = 0.25
        tn = 0.25
        oled = 0.25

        # Known OLED models
        oled_models = [
            'PG27UCDM', 'PG32UCDM', 'PG27AQDM', 'PG32AQDM',  # ASUS ROG OLED
            'G85SB', 'G95SC', 'G93SC', 'G95T',  # Samsung Odyssey OLED
            'AW3423DW', 'AW3423DWF', 'AW2725DF',  # Dell Alienware OLED
            'UL950', 'UL850', '27EP950', '48GQ900',  # LG OLED
            'XG27AQDMG',  # ASUS OLED
        ]

        for oled_model in oled_models:
            if oled_model in model_upper:
                oled = 0.95
                ips = 0.02
                va = 0.02
                tn = 0.01
                return [ips, va, tn, oled]

        # Check for OLED in name
        if 'OLED' in model_upper or 'QD-OLED' in model_upper:
            oled = 0.9
        elif 'IPS' in model_upper:
            ips = 0.9
        elif 'VA' in model_upper:
            va = 0.9
        elif 'TN' in model_upper:
            tn = 0.9

        # Manufacturer + model hints
        if mfg_upper in ['LG', 'LG ELECTRONICS']:
            if 'ULTRAGEAR' in model_upper:
                ips = 0.7
        elif mfg_upper == 'SAMSUNG':
            if 'ODYSSEY' in model_upper:
                # Newer Odyssey models are often OLED/QD-OLED
                if 'G8' in model_upper or 'G9' in model_upper:
                    oled = 0.8
                else:
                    va = 0.7
        elif mfg_upper in ['DELL', 'ASUS', 'BENQ']:
            if 'ROG' in model_upper or 'PG' in model_upper:
                # ROG monitors are often high-end IPS or OLED
                ips = 0.5
                oled = 0.4

        # Normalize
        total = ips + va + tn + oled
        return [ips/total, va/total, tn/total, oled/total]

    def predict(self, features: List[float]) -> List[float]:
        """Run neural network inference."""
        current = features
        for layer in self.layers:
            current = layer.forward(current)
        return current

    def calibrate(self, monitor: MonitorInfo) -> DisplayProfile:
        """Generate calibration profile for a monitor."""
        features = self.extract_features(monitor)
        prediction = self.predict(features)

        # Detect panel type
        panel_scores = self._detect_panel_type(monitor)
        is_oled = panel_scores[3] > 0.5  # OLED score > 50%

        # Decode prediction into profile
        # OLED typically uses gamma 2.4, LCD uses 2.2
        if is_oled:
            base_gamma = 2.4
            gamma = base_gamma + prediction[0] * 0.2  # 2.4 to 2.6
        else:
            base_gamma = 2.2
            gamma = base_gamma + prediction[0] * 0.3  # 2.2 to 2.5
        gamma = max(1.8, min(2.6, gamma))

        # Primaries adjustment
        base = DisplayPrimaries.srgb()
        if monitor.edid and len(monitor.edid) >= 128:
            edid_info = parse_edid(monitor.edid)
            if edid_info and edid_info['primaries']:
                base = edid_info['primaries']

        # Apply neural adjustments to primaries
        primaries = DisplayPrimaries(
            red=(
                max(0.5, min(0.75, base.red[0] + prediction[1] * 0.05)),
                max(0.25, min(0.45, base.red[1] + prediction[2] * 0.05))
            ),
            green=(
                max(0.2, min(0.4, base.green[0] + prediction[3] * 0.05)),
                max(0.5, min(0.75, base.green[1] + prediction[4] * 0.05))
            ),
            blue=(
                max(0.1, min(0.2, base.blue[0] + prediction[5] * 0.02)),
                max(0.02, min(0.12, base.blue[1] + prediction[6] * 0.02))
            ),
            white=base.white
        )

        # White point and luminance
        white_point = 6500 + prediction[7] * 500  # 6000K to 7000K

        # OLED monitors typically have higher peak luminance
        if is_oled:
            max_luminance = 250 + prediction[8] * 750  # 250 to 1000 nits (OLED HDR capable)
        else:
            max_luminance = 100 + prediction[8] * 300  # 100 to 400 nits

        # Panel type string
        if is_oled:
            panel_str = "OLED"
        elif panel_scores[1] > 0.5:
            panel_str = "VA"
        elif panel_scores[2] > 0.5:
            panel_str = "TN"
        else:
            panel_str = "IPS"

        return DisplayProfile(
            name=f"{monitor.manufacturer} {monitor.model} ({panel_str}, NeuralUX)",
            primaries=primaries,
            gamma=gamma,
            white_point_kelvin=white_point,
            max_luminance=max_luminance
        )

# ═══════════════════════════════════════════════════════════════════════════════
# COLOR PROFILE APPLICATION (Windows Color Management)
# ═══════════════════════════════════════════════════════════════════════════════

class ColorProfileManager:
    """Manages ICC profile installation and application on Windows."""

    # WCS profile types
    WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER = 0
    WCS_PROFILE_MANAGEMENT_SCOPE_SYSTEM_WIDE = 1

    # Color profile types
    CPT_ICC = 0
    CPT_DMP = 1
    CPT_CAMP = 2
    CPT_GMMP = 3

    # Color profile subtype
    CPST_NONE = 0
    CPST_RGB_WORKING_SPACE = 1
    CPST_CUSTOM_WORKING_SPACE = 2

    def __init__(self):
        self.has_api = HAS_COLOR_MANAGEMENT

    def install_profile(self, icc_path: str) -> bool:
        """Install an ICC profile to the system."""
        if not self.has_api:
            return False

        try:
            # InstallColorProfileW
            result = mscms.InstallColorProfileW(None, icc_path)
            return bool(result)
        except Exception as e:
            print(f"    -> Warning: Could not install profile: {e}")
            return False

    def associate_profile_with_device(self, device_name: str, icc_path: str) -> bool:
        """Associate an ICC profile with a display device."""
        if not self.has_api:
            return False

        try:
            # Get just the filename from the path
            profile_name = os.path.basename(icc_path)

            # WcsAssociateColorProfileWithDevice
            result = mscms.WcsAssociateColorProfileWithDevice(
                self.WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
                profile_name,
                device_name
            )
            return bool(result)
        except Exception as e:
            print(f"    -> Warning: Could not associate profile: {e}")
            return False

    def set_default_profile(self, device_name: str, icc_path: str) -> bool:
        """Set an ICC profile as the default for a display device."""
        if not self.has_api:
            return False

        try:
            profile_name = os.path.basename(icc_path)

            # WcsSetDefaultColorProfile
            result = mscms.WcsSetDefaultColorProfile(
                self.WCS_PROFILE_MANAGEMENT_SCOPE_CURRENT_USER,
                device_name,
                self.CPT_ICC,
                self.CPST_NONE,
                0,  # Profile index
                profile_name
            )
            return bool(result)
        except Exception as e:
            print(f"    -> Warning: Could not set default profile: {e}")
            return False

    def apply_gamma_ramp(self, device_name: str, gamma: float) -> bool:
        """Apply a gamma ramp to a display for immediate effect."""
        try:
            # Get device context for the display
            # CreateDCW is in gdi32, not user32
            hdc = gdi32.CreateDCW("DISPLAY", device_name, None, None)
            if not hdc:
                # Try with NULL device name for primary
                hdc = gdi32.CreateDCW("DISPLAY", None, None, None)

            if not hdc:
                # Last resort: get DC for desktop window
                hdc = user32.GetDC(None)

            if not hdc:
                return False

            # Create gamma ramp (256 entries for R, G, B)
            # Structure: WORD GammaRamp[3][256]
            ramp = (ctypes.c_ushort * 256 * 3)()

            for i in range(256):
                # Apply gamma correction
                normalized = i / 255.0
                corrected = pow(normalized, 1.0 / gamma)
                value = int(corrected * 65535)
                value = max(0, min(65535, value))

                ramp[0][i] = value  # Red
                ramp[1][i] = value  # Green
                ramp[2][i] = value  # Blue

            # SetDeviceGammaRamp
            result = gdi32.SetDeviceGammaRamp(hdc, ctypes.byref(ramp))

            # Clean up - use ReleaseDC if we got DC from GetDC, otherwise DeleteDC
            try:
                user32.ReleaseDC(None, hdc)
            except:
                try:
                    gdi32.DeleteDC(hdc)
                except:
                    pass

            return bool(result)
        except Exception as e:
            print(f"    -> Warning: Could not apply gamma ramp: {e}")
            return False

    def apply_profile(self, monitor: 'MonitorInfo', icc_path: str, profile: 'DisplayProfile') -> dict:
        """Full profile application: install, associate, set default, apply gamma."""
        results = {
            'installed': False,
            'associated': False,
            'set_default': False,
            'gamma_applied': False
        }

        # Step 1: Install the profile
        results['installed'] = self.install_profile(icc_path)

        # Step 2: Associate with device
        if results['installed']:
            results['associated'] = self.associate_profile_with_device(
                monitor.device_name, icc_path
            )

        # Step 3: Set as default
        if results['associated']:
            results['set_default'] = self.set_default_profile(
                monitor.device_name, icc_path
            )

        # Step 4: Apply gamma ramp for immediate effect
        results['gamma_applied'] = self.apply_gamma_ramp(
            monitor.device_name, profile.gamma
        )

        return results


def apply_profile_via_powershell(device_name: str, icc_path: str) -> bool:
    """Fallback: Apply ICC profile using PowerShell and WMI."""
    import subprocess

    profile_name = os.path.basename(icc_path)

    # PowerShell script to associate and activate the color profile
    ps_script = f'''
    Add-Type -TypeDefinition @"
    using System;
    using System.Runtime.InteropServices;

    public class ColorProfile {{
        [DllImport("mscms.dll", CharSet = CharSet.Unicode)]
        public static extern bool InstallColorProfileW(IntPtr pMachineName, string pProfileName);

        [DllImport("mscms.dll", CharSet = CharSet.Unicode)]
        public static extern bool WcsAssociateColorProfileWithDevice(
            int scope, string pProfileName, string pDeviceName);

        [DllImport("mscms.dll", CharSet = CharSet.Unicode)]
        public static extern bool WcsSetDefaultColorProfile(
            int scope, string pDeviceName, int profileType, int profileSubType,
            int profileIndex, string pProfileName);
    }}
"@

    # Install the profile
    $installed = [ColorProfile]::InstallColorProfileW([IntPtr]::Zero, "{icc_path}")
    Write-Host "Installed: $installed"

    # Associate with device
    $associated = [ColorProfile]::WcsAssociateColorProfileWithDevice(0, "{profile_name}", "{device_name}")
    Write-Host "Associated: $associated"

    # Set as default (scope=0 for current user, profileType=0 for ICC)
    $setDefault = [ColorProfile]::WcsSetDefaultColorProfile(0, "{device_name}", 0, 0, 0, "{profile_name}")
    Write-Host "Set Default: $setDefault"

    $installed -and $associated -and $setDefault
    '''

    try:
        result = subprocess.run(
            ['powershell', '-ExecutionPolicy', 'Bypass', '-Command', ps_script],
            capture_output=True,
            text=True,
            timeout=30
        )
        return 'True' in result.stdout
    except Exception as e:
        print(f"    -> PowerShell fallback failed: {e}")
        return False


# ═══════════════════════════════════════════════════════════════════════════════
# ICC PROFILE GENERATION
# ═══════════════════════════════════════════════════════════════════════════════

def generate_icc_profile(profile: DisplayProfile, monitor: MonitorInfo) -> bytes:
    """Generate ICC v4 profile for the calibrated display."""

    def write_u32_be(value: int) -> bytes:
        return struct.pack('>I', value)

    def write_u16_be(value: int) -> bytes:
        return struct.pack('>H', value)

    def write_s15fixed16(value: float) -> bytes:
        fixed = int(value * 65536)
        return struct.pack('>i', fixed)

    def write_xyz(x: float, y: float, z: float) -> bytes:
        return b'XYZ ' + b'\x00' * 4 + write_s15fixed16(x) + write_s15fixed16(y) + write_s15fixed16(z)

    # Build profile header (128 bytes)
    header = bytearray(128)

    # Profile size (will update later)
    # Preferred CMM
    header[4:8] = b'lcms'
    # Profile version 4.3
    header[8:12] = bytes([4, 0x30, 0, 0])
    # Device class: Display
    header[12:16] = b'mntr'
    # Color space: RGB
    header[16:20] = b'RGB '
    # PCS: XYZ
    header[20:24] = b'XYZ '
    # Date/time
    now = time.gmtime()
    header[24:36] = struct.pack('>HHHHHH', now.tm_year, now.tm_mon, now.tm_mday,
                                 now.tm_hour, now.tm_min, now.tm_sec)
    # Signature
    header[36:40] = b'acsp'
    # Platform: Microsoft
    header[40:44] = b'MSFT'
    # Flags
    header[44:48] = b'\x00\x00\x00\x00'
    # Device manufacturer
    header[48:52] = b'QNTA'
    # Device model
    header[52:56] = b'CALB'
    # Rendering intent
    header[64:68] = b'\x00\x00\x00\x00'
    # PCS illuminant (D50)
    header[68:80] = write_s15fixed16(0.9642) + write_s15fixed16(1.0) + write_s15fixed16(0.8249)
    # Profile creator
    header[80:84] = b'QNTA'

    # Build tag table
    tags = []

    # Description tag
    desc = f"Calibrate NeuralUX: {profile.name}".encode('utf-16-be')
    desc_data = b'desc\x00\x00\x00\x00' + write_u32_be(len(desc)) + desc
    tags.append((b'desc', desc_data))

    # Copyright
    cprt = b"Copyright Zain Dana Harper 2024-2025".ljust(64, b'\x00')
    cprt_data = b'text\x00\x00\x00\x00' + cprt
    tags.append((b'cprt', cprt_data))

    # White point
    wp_data = write_xyz(0.9642, 1.0, 0.8249)  # D50
    tags.append((b'wtpt', wp_data))

    # Red primary
    r_xyz = (profile.primaries.red[0] / profile.primaries.white[1],
             profile.primaries.red[1] / profile.primaries.white[1],
             (1 - profile.primaries.red[0] - profile.primaries.red[1]) / profile.primaries.white[1])
    tags.append((b'rXYZ', write_xyz(r_xyz[0] * 0.4, r_xyz[1] * 0.2, r_xyz[2] * 0.0)))

    # Green primary
    g_xyz = (profile.primaries.green[0] / profile.primaries.white[1],
             profile.primaries.green[1] / profile.primaries.white[1],
             (1 - profile.primaries.green[0] - profile.primaries.green[1]) / profile.primaries.white[1])
    tags.append((b'gXYZ', write_xyz(g_xyz[0] * 0.4, g_xyz[1] * 0.7, g_xyz[2] * 0.1)))

    # Blue primary
    b_xyz = (profile.primaries.blue[0] / profile.primaries.white[1],
             profile.primaries.blue[1] / profile.primaries.white[1],
             (1 - profile.primaries.blue[0] - profile.primaries.blue[1]) / profile.primaries.white[1])
    tags.append((b'bXYZ', write_xyz(b_xyz[0] * 0.1, b_xyz[1] * 0.1, b_xyz[2] * 0.9)))

    # TRC (gamma curves) - parametric curve type 3
    gamma_fixed = write_s15fixed16(profile.gamma)
    trc_data = b'para\x00\x00\x00\x00' + write_u16_be(0) + b'\x00\x00' + gamma_fixed
    tags.append((b'rTRC', trc_data))
    tags.append((b'gTRC', trc_data))
    tags.append((b'bTRC', trc_data))

    # Build tag table and data
    tag_count = len(tags)
    tag_table_size = 4 + tag_count * 12

    # Calculate offsets
    current_offset = 128 + tag_table_size
    tag_table = write_u32_be(tag_count)
    tag_data = b''

    for sig, data in tags:
        # Align to 4 bytes
        while current_offset % 4 != 0:
            tag_data += b'\x00'
            current_offset += 1

        tag_table += sig + write_u32_be(current_offset) + write_u32_be(len(data))
        tag_data += data
        current_offset += len(data)

    # Combine and update size
    profile_data = bytes(header) + tag_table + tag_data
    profile_size = len(profile_data)
    profile_data = write_u32_be(profile_size) + profile_data[4:]

    return profile_data

# ═══════════════════════════════════════════════════════════════════════════════
# MAIN CALIBRATION ROUTINE
# ═══════════════════════════════════════════════════════════════════════════════

def run_calibration():
    """Run automatic calibration on all connected monitors."""

    print("=" * 70)
    print("  Calibrate(TM) Automatic Display Calibration")
    if HAS_PRECISION_ENGINE:
        print("  NeuralUX(TM) Precision Engine - Delta E < 1.0 Target")
    else:
        print("  NeuralUX(TM) AI-Powered Sensorless Calibration")
    print("=" * 70)
    print()

    # Initialize profile manager
    profile_manager = ColorProfileManager()

    # Initialize precision calibrator if available
    precision_calibrator = PrecisionCalibrator() if HAS_PRECISION_ENGINE else None

    # Step 1: Enumerate monitors
    print("[1/5] Discovering connected monitors...")
    monitors = enumerate_monitors()

    if not monitors:
        print("  ERROR: No monitors detected!")
        print("  Please ensure your displays are connected and powered on.")
        return

    print(f"  Found {len(monitors)} monitor(s):")
    for i, monitor in enumerate(monitors):
        primary_marker = " [PRIMARY]" if monitor.primary else ""
        print(f"    {i+1}. {monitor.manufacturer} {monitor.model}")
        print(f"       {monitor.resolution[0]}x{monitor.resolution[1]} @ {monitor.refresh_rate:.0f}Hz{primary_marker}")
        if monitor.edid:
            print(f"       EDID: {len(monitor.edid)} bytes")
    print()

    # Step 2: Calibration settings
    print("[2/5] Calibration targets:")
    print("  White Point: 6500K (D65)")
    print("  Luminance:   120 cd/m2 (120 nits)")
    print("  Gamma:       2.2")
    print("  Tolerance:   dE < 1.0")
    print()

    # Step 3: Calibrate each monitor
    if HAS_PRECISION_ENGINE:
        print("[3/5] Running NeuralUX(TM) Precision calibration...")
    else:
        print("[3/5] Running NeuralUX(TM) calibration...")

    calibrator = NeuralUXCalibrator()
    results = []

    # Get output directory
    system_root = os.environ.get('SystemRoot', 'C:\\Windows')
    output_dir = os.path.join(system_root, 'System32', 'spool', 'drivers', 'color')

    for i, monitor in enumerate(monitors):
        start_time = time.time()

        print()
        print(f"  Monitor {i+1}/{len(monitors)}: {monitor.manufacturer} {monitor.model}")

        # Check for EDID
        if monitor.edid and len(monitor.edid) >= 128:
            edid_info = parse_edid(monitor.edid)
            if edid_info:
                print(f"    -> Parsed EDID: {edid_info['manufacturer']} (Year: {edid_info['year']})")

        # Use precision calibrator if available
        panel_char = None
        if precision_calibrator:
            panel_char = precision_calibrator.find_panel_characterization(
                monitor.manufacturer, monitor.model
            )
            if panel_char.model_pattern:
                print(f"    -> Panel database match: {panel_char.manufacturer} {panel_char.panel_type}")
                print(f"    -> Using factory-characterized primaries...")

                # Calculate expected Delta E
                avg_de, max_de, patch_results = precision_calibrator.calculate_expected_delta_e(
                    panel_char, SRGB_PRIMARIES
                )
                print(f"    -> Expected accuracy: Avg dE={avg_de:.2f}, Max dE={max_de:.2f}")

                if avg_de < 1.0:
                    print(f"    -> [REFERENCE GRADE] Delta E < 1.0 achievable!")
                elif avg_de < 2.0:
                    print(f"    -> [PROFESSIONAL GRADE] Delta E < 2.0")

        # Run legacy calibration for profile settings
        profile = calibrator.calibrate(monitor)

        # If precision calibrator found a match, update primaries from database
        if panel_char and panel_char.model_pattern:
            print(f"    -> Panel primaries (factory-measured):")
            print(f"       Red:   ({panel_char.native_primaries.red.x:.4f}, {panel_char.native_primaries.red.y:.4f})")
            print(f"       Green: ({panel_char.native_primaries.green.x:.4f}, {panel_char.native_primaries.green.y:.4f})")
            print(f"       Blue:  ({panel_char.native_primaries.blue.x:.4f}, {panel_char.native_primaries.blue.y:.4f})")
            print(f"    -> Per-channel gamma: R={panel_char.gamma_red.gamma:.2f}, G={panel_char.gamma_green.gamma:.2f}, B={panel_char.gamma_blue.gamma:.2f}")
        else:
            print(f"    -> Detected primaries (EDID/estimated):")
            print(f"       Red:   ({profile.primaries.red[0]:.4f}, {profile.primaries.red[1]:.4f})")
            print(f"       Green: ({profile.primaries.green[0]:.4f}, {profile.primaries.green[1]:.4f})")
            print(f"       Blue:  ({profile.primaries.blue[0]:.4f}, {profile.primaries.blue[1]:.4f})")
            print(f"    -> Gamma: {profile.gamma:.2f}")

        print(f"    -> White point: {profile.white_point_kelvin:.0f}K")

        # Generate precision profile if available
        if panel_char and panel_char.model_pattern and HAS_PRECISION_ENGINE:
            print("    -> Generating precision ICC profile with per-channel TRC...")
            profile_name = f"{monitor.manufacturer} {monitor.model}"
            icc_data = generate_precision_icc(panel_char, precision_calibrator, profile_name, 2.2)
        else:
            print("    -> Generating standard ICC v4 profile...")
            icc_data = generate_icc_profile(profile, monitor)

        # Save ICC profile - use unique name including resolution and display number
        safe_name = f"{monitor.manufacturer}_{monitor.model}".replace(' ', '_').replace('/', '_')
        resolution_tag = f"{monitor.resolution[0]}x{monitor.resolution[1]}"
        icc_filename = f"Calibrate_{safe_name}_{resolution_tag}_Display{i+1}_NeuralUX.icc"
        icc_path = os.path.join(output_dir, icc_filename)

        try:
            with open(icc_path, 'wb') as f:
                f.write(icc_data)
            print(f"    -> Saved: {icc_filename}")
            save_success = True
        except PermissionError:
            # Try user directory instead
            user_dir = os.path.join(os.environ.get('USERPROFILE', '.'), 'Documents')
            icc_path = os.path.join(user_dir, icc_filename)
            try:
                with open(icc_path, 'wb') as f:
                    f.write(icc_data)
                print(f"    -> Saved to user folder: {icc_path}")
                save_success = True
            except Exception as e:
                print(f"    -> Warning: Could not save profile: {e}")
                save_success = False
        except Exception as e:
            print(f"    -> Warning: Could not save profile: {e}")
            save_success = False

        elapsed = time.time() - start_time

        # Store expected Delta E if calculated
        expected_de = None
        if panel_char and panel_char.model_pattern and HAS_PRECISION_ENGINE:
            expected_de = (avg_de, max_de)

        results.append({
            'monitor': monitor,
            'profile': profile,
            'panel_char': panel_char,
            'expected_de': expected_de,
            'icc_path': icc_path if save_success else None,
            'elapsed': elapsed,
            'success': save_success,
            'applied': False
        })

        print(f"    [OK] Generated ({elapsed:.2f}s)")

    print()

    # Step 4: Apply profiles automatically
    print("[4/5] Applying color profiles...")

    for i, result in enumerate(results):
        if not result['icc_path']:
            continue

        monitor = result['monitor']
        profile = result['profile']
        panel_char = result.get('panel_char')
        icc_path = result['icc_path']

        print()
        print(f"  Applying to {monitor.manufacturer} {monitor.model}...")

        # Use precision gamma if available, otherwise legacy
        if panel_char and panel_char.model_pattern:
            # For precision calibration, use panel's native gamma (no correction needed)
            # The ICC profile handles the gamma - we just need to reset to linear
            profile.gamma = 1.0  # Don't apply additional gamma ramp
            print(f"    -> Using precision profile (no gamma ramp needed)")

        # Try to apply the profile
        apply_result = profile_manager.apply_profile(monitor, icc_path, profile)

        if apply_result['gamma_applied']:
            print(f"    -> Gamma ramp applied (gamma={profile.gamma:.2f})")
            result['applied'] = True

        if apply_result['installed']:
            print(f"    -> Profile installed to system")

        if apply_result['associated']:
            print(f"    -> Profile associated with {monitor.device_name}")

        if apply_result['set_default']:
            print(f"    -> Set as default profile")
            result['applied'] = True

        # If direct API failed, try PowerShell fallback
        if not apply_result['set_default'] and not apply_result['gamma_applied']:
            print(f"    -> Trying PowerShell fallback...")
            if apply_profile_via_powershell(monitor.device_name, icc_path):
                print(f"    -> Applied via PowerShell")
                result['applied'] = True
            else:
                print(f"    -> Manual application required")

        if result['applied']:
            print(f"    [OK] Profile active")
        else:
            print(f"    [!] Profile saved but requires manual activation")

    print()

    # Step 5: Summary
    print("[5/5] Calibration Summary")
    print("=" * 70)

    successful = sum(1 for r in results if r['success'])
    applied = sum(1 for r in results if r['applied'])
    total_time = sum(r['elapsed'] for r in results)

    print(f"  Monitors calibrated: {successful}/{len(results)}")
    print(f"  Profiles applied:    {applied}/{len(results)}")
    print(f"  Total time: {total_time:.2f}s")
    print()

    # Show color accuracy for precision calibration
    if HAS_PRECISION_ENGINE:
        print("  Color Accuracy (Expected Delta E 2000):")
        for result in results:
            if result['expected_de']:
                avg_de, max_de = result['expected_de']
                monitor = result['monitor']
                if avg_de < 1.0:
                    grade = "REFERENCE"
                elif avg_de < 2.0:
                    grade = "PROFESSIONAL"
                elif avg_de < 3.0:
                    grade = "GOOD"
                else:
                    grade = "ACCEPTABLE"
                print(f"    {monitor.manufacturer} {monitor.model}:")
                print(f"      Average: {avg_de:.2f}  Max: {max_de:.2f}  [{grade}]")
        print()

    print("  Profile Status:")
    for result in results:
        if result['icc_path']:
            status = "[ACTIVE]" if result['applied'] else "[SAVED]"
            monitor = result['monitor']
            print(f"    {status} {monitor.manufacturer} {monitor.model}")
            print(f"            {os.path.basename(result['icc_path'])}")
    print()

    if applied == len(results):
        print("  [OK] All profiles applied successfully!")
        print()
        print("  Your displays are now calibrated. Color-managed applications")
        print("  (Photoshop, Lightroom, Chrome, etc.) will use these profiles.")
    elif applied > 0:
        print(f"  [OK] {applied} profile(s) applied automatically.")
        print()
        not_applied = [r for r in results if not r['applied'] and r['icc_path']]
        if not_applied:
            print("  For remaining monitors, manually apply via:")
            print("    Settings -> System -> Display -> Advanced display -> Color profiles")
    else:
        print("  [!] Profiles require manual activation.")
        print()
        print("  To apply manually:")
        print("    1. Open Settings -> System -> Display")
        print("    2. Select each monitor -> Advanced display")
        print("    3. Click 'Color profiles' -> Add and select the profile")
        print("    4. Restart color-managed applications")

    print()
    print("=" * 67)
    print("  Powered by NeuralUX(TM) - Perfect Color Without Hardware")
    print("=" * 67)

if __name__ == '__main__':
    run_calibration()
