"""VCGT gamma ramp calibration + measured verification."""
import subprocess, tempfile, os, sys, time
import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from calibrate_pro.core.color_math import (
    xyz_to_lab, bradford_adapt, delta_e_2000, D50_WHITE, D65_WHITE,
    srgb_gamma_expand, SRGB_TO_XYZ
)
from calibrate_pro.panels.detection import enumerate_displays, set_gamma_ramp, reset_gamma_ramp

DISPREAD = os.environ.get("DISPREAD", "dispread")  # ArgyllCMS dispread; set DISPREAD to its full path

PATCHES = [
    ("Dark Skin", 0.453, 0.317, 0.264), ("Light Skin", 0.779, 0.577, 0.505),
    ("Blue Sky", 0.355, 0.480, 0.611), ("Foliage", 0.352, 0.422, 0.253),
    ("Blue Flower", 0.508, 0.502, 0.691), ("Bluish Green", 0.362, 0.745, 0.675),
    ("Orange", 0.879, 0.485, 0.183), ("Purplish Blue", 0.266, 0.358, 0.667),
    ("Moderate Red", 0.778, 0.321, 0.381), ("Purple", 0.367, 0.227, 0.414),
    ("Yellow Green", 0.623, 0.741, 0.246), ("Orange Yellow", 0.904, 0.634, 0.154),
    ("Blue", 0.139, 0.248, 0.577), ("Green", 0.262, 0.584, 0.291),
    ("Red", 0.752, 0.197, 0.178), ("Yellow", 0.938, 0.857, 0.159),
    ("Magenta", 0.752, 0.313, 0.577), ("Cyan", 0.121, 0.544, 0.659),
    ("White", 0.961, 0.961, 0.961), ("Neutral 8", 0.784, 0.784, 0.784),
    ("Neutral 6.5", 0.584, 0.584, 0.584), ("Neutral 5", 0.420, 0.420, 0.420),
    ("Neutral 3.5", 0.258, 0.258, 0.258), ("Black", 0.085, 0.085, 0.085),
]
REF_LAB = {
    "Dark Skin": (37.986, 13.555, 14.059), "Light Skin": (65.711, 18.130, 17.810),
    "Blue Sky": (49.927, -4.880, -21.925), "Foliage": (43.139, -13.095, 21.905),
    "Blue Flower": (55.112, 8.844, -25.399), "Bluish Green": (70.719, -33.397, -0.199),
    "Orange": (62.661, 36.067, 57.096), "Purplish Blue": (40.020, 10.410, -45.964),
    "Moderate Red": (51.124, 48.239, 16.248), "Purple": (30.325, 22.976, -21.587),
    "Yellow Green": (72.532, -23.709, 57.255), "Orange Yellow": (71.941, 19.363, 67.857),
    "Blue": (28.778, 14.179, -50.297), "Green": (55.261, -38.342, 31.370),
    "Red": (42.101, 53.378, 28.190), "Yellow": (81.733, 4.039, 79.819),
    "Magenta": (51.935, 49.986, -14.574), "Cyan": (51.038, -28.631, -28.638),
    "White": (96.539, -0.425, 1.186), "Neutral 8": (81.257, -0.638, -0.335),
    "Neutral 6.5": (66.766, -0.734, -0.504), "Neutral 5": (50.867, -0.153, -0.270),
    "Neutral 3.5": (35.656, -0.421, -1.231), "Black": (20.461, -0.079, -0.973),
}

def measure():
    ti1 = 'CTI1\nDESCRIPTOR "M"\nORIGINATOR "CP"\nCOLOR_REP "RGB"\n'
    ti1 += "NUMBER_OF_FIELDS 4\nBEGIN_DATA_FORMAT\nSAMPLE_ID RGB_R RGB_G RGB_B\nEND_DATA_FORMAT\n"
    ti1 += f"NUMBER_OF_SETS {len(PATCHES)}\nBEGIN_DATA\n"
    for i, (n, r, g, b) in enumerate(PATCHES, 1):
        ti1 += f"{i} {r*100:.2f} {g*100:.2f} {b*100:.2f}\n"
    ti1 += "END_DATA\n"
    with tempfile.TemporaryDirectory() as td:
        base = os.path.join(td, "m")
        with open(base + ".ti1", "w") as f:
            f.write(ti1)
        proc = subprocess.Popen(
            [DISPREAD, "-d", "1", "-c", "1", "-y", "o",
             "-Y", "p", "-N", "-F", "-P", "0.5,0.5,0.4", base],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, cwd=td)
        stdout, _ = proc.communicate(timeout=180)
        ti3 = base + ".ti3"
        if not os.path.exists(ti3):
            return None
        results = []
        in_data = False
        for line in open(ti3).read().split("\n"):
            if "BEGIN_DATA" in line and "FORMAT" not in line:
                in_data = True; continue
            if "END_DATA" in line:
                in_data = False
            if in_data and line.strip():
                p = line.split()
                if len(p) >= 7:
                    results.append((float(p[4]), float(p[5]), float(p[6])))
        return results if len(results) == len(PATCHES) else None

def compute_de(meas):
    return [float(delta_e_2000(
        xyz_to_lab(bradford_adapt(np.array([X,Y,Z])/100.0, D65_WHITE, D50_WHITE), D50_WHITE),
        np.array(REF_LAB[n])))
        for (n,r,g,b), (X,Y,Z) in zip(PATCHES, meas)]

print("=== VCGT GAMMA RAMP CALIBRATION ===")
print("Display: ASUS PG27UCDM (QD-OLED)")
print("Sensor:  NEC MDSVSENSOR3 (i1Display3)")
print()

# Remove DWM LUT
try:
    from calibrate_pro.lut_system.dwm_lut import remove_lut
    remove_lut(1)
except: pass

displays = enumerate_displays()
dev = displays[1].device_name if len(displays) > 1 else displays[0].device_name
reset_gamma_ramp(dev)
time.sleep(2)

# Measure uncalibrated
print("Step 1: Measuring uncalibrated...")
uncal = measure()
if not uncal:
    print("FAILED"); sys.exit(1)
uncal_de = compute_de(uncal)
print(f"  avg dE {np.mean(uncal_de):.2f}")

# White point from measurement
wXYZ = np.array(uncal[18])  # White patch
wsum = sum(wXYZ)
wx, wy = wXYZ[0]/wsum, wXYZ[1]/wsum
print(f"  White: x={wx:.4f} y={wy:.4f} (target: 0.3127, 0.3290)")

# Compute per-channel correction to shift white to D65
# Target white in XYZ (Y=1 normalized)
target_white = np.array([0.3127/0.3290, 1.0, (1-0.3127-0.3290)/0.3290])
measured_white = np.array([wx/wy, 1.0, (1-wx-wy)/wy])

# Simple per-channel gain: ratio of target to measured white XYZ
# converted through sRGB matrix
from calibrate_pro.core.color_math import XYZ_TO_SRGB
target_rgb = XYZ_TO_SRGB @ target_white
measured_rgb = XYZ_TO_SRGB @ measured_white

r_gain = target_rgb[0] / measured_rgb[0] if measured_rgb[0] > 0 else 1.0
g_gain = target_rgb[1] / measured_rgb[1] if measured_rgb[1] > 0 else 1.0
b_gain = target_rgb[2] / measured_rgb[2] if measured_rgb[2] > 0 else 1.0

# Normalize so max = 1.0
mx = max(r_gain, g_gain, b_gain)
r_gain /= mx; g_gain /= mx; b_gain /= mx
print(f"  WP correction: R={r_gain:.4f} G={g_gain:.4f} B={b_gain:.4f}")

# Build VCGT with white point correction only
x = np.linspace(0, 1, 256)
red = np.clip(x * r_gain, 0, 1)
green = np.clip(x * g_gain, 0, 1)
blue = np.clip(x * b_gain, 0, 1)

r16 = (red * 65535).astype(np.uint16)
g16 = (green * 65535).astype(np.uint16)
b16 = (blue * 65535).astype(np.uint16)

print("\nStep 2: Applying VCGT white point correction...")
ok = set_gamma_ramp(dev, r16, g16, b16)
print(f"  Applied: {ok}")
time.sleep(3)

# Re-measure
print("Step 3: Re-measuring with VCGT correction...")
cal = measure()
if not cal:
    print("FAILED"); sys.exit(1)
cal_de = compute_de(cal)

# Results
print()
print(f"{'Patch':20s}  {'Before':>6s}  {'After':>6s}  {'Change':>7s}  Status")
print("=" * 65)
for i, (n,r,g,b) in enumerate(PATCHES):
    bd, ad = uncal_de[i], cal_de[i]
    ch = ad - bd
    st = "PASS" if ad < 2.0 else "WARN" if ad < 3.0 else "FAIL"
    ar = "v" if ch < -0.5 else "^" if ch > 0.5 else "="
    print(f"  {n:20s}  {bd:5.2f}   {ad:5.2f}   {ch:+5.2f} {ar}  [{st}]")
print("=" * 65)
ab, aa = np.mean(uncal_de), np.mean(cal_de)
print(f"  Before: avg dE {ab:.2f}  max {np.max(uncal_de):.2f}")
print(f"  After:  avg dE {aa:.2f}  max {np.max(cal_de):.2f}")
pct = (1-aa/ab)*100 if ab > 0 else 0
print(f"  Change: {ab-aa:+.2f} dE ({pct:+.0f}%)")

# New white point
wXYZ2 = np.array(cal[18])
ws2 = sum(wXYZ2)
print(f"\n  New white: x={wXYZ2[0]/ws2:.4f} y={wXYZ2[1]/ws2:.4f}")
print("  ALL VALUES MEASURED.")
