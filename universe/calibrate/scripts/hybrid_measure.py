"""
Hybrid measured refinement script.
Measures uncalibrated display, computes correction from real data,
applies corrected LUT, re-measures to verify improvement.
"""
import subprocess
import tempfile
import os
import sys
import time
import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from calibrate_pro.core.color_math import (
    xyz_to_lab, bradford_adapt, delta_e_2000,
    D50_WHITE, D65_WHITE, srgb_gamma_expand, srgb_gamma_compress,
    SRGB_TO_XYZ, XYZ_TO_SRGB
)
from calibrate_pro.core.lut_engine import LUT3D

DISPREAD = os.environ.get("DISPREAD", "dispread")  # ArgyllCMS dispread; set DISPREAD to its full path

PATCHES = [
    ("Dark Skin",    0.453, 0.317, 0.264),
    ("Light Skin",   0.779, 0.577, 0.505),
    ("Blue Sky",     0.355, 0.480, 0.611),
    ("Foliage",      0.352, 0.422, 0.253),
    ("Blue Flower",  0.508, 0.502, 0.691),
    ("Bluish Green", 0.362, 0.745, 0.675),
    ("Orange",       0.879, 0.485, 0.183),
    ("Purplish Blue",0.266, 0.358, 0.667),
    ("Moderate Red", 0.778, 0.321, 0.381),
    ("Purple",       0.367, 0.227, 0.414),
    ("Yellow Green", 0.623, 0.741, 0.246),
    ("Orange Yellow",0.904, 0.634, 0.154),
    ("Blue",         0.139, 0.248, 0.577),
    ("Green",        0.262, 0.584, 0.291),
    ("Red",          0.752, 0.197, 0.178),
    ("Yellow",       0.938, 0.857, 0.159),
    ("Magenta",      0.752, 0.313, 0.577),
    ("Cyan",         0.121, 0.544, 0.659),
    ("White",        0.961, 0.961, 0.961),
    ("Neutral 8",    0.784, 0.784, 0.784),
    ("Neutral 6.5",  0.584, 0.584, 0.584),
    ("Neutral 5",    0.420, 0.420, 0.420),
    ("Neutral 3.5",  0.258, 0.258, 0.258),
    ("Black",        0.085, 0.085, 0.085),
]

REF_LAB = {
    "Dark Skin":    (37.986, 13.555, 14.059),
    "Light Skin":   (65.711, 18.130, 17.810),
    "Blue Sky":     (49.927, -4.880, -21.925),
    "Foliage":      (43.139, -13.095, 21.905),
    "Blue Flower":  (55.112, 8.844, -25.399),
    "Bluish Green": (70.719, -33.397, -0.199),
    "Orange":       (62.661, 36.067, 57.096),
    "Purplish Blue":(40.020, 10.410, -45.964),
    "Moderate Red": (51.124, 48.239, 16.248),
    "Purple":       (30.325, 22.976, -21.587),
    "Yellow Green": (72.532, -23.709, 57.255),
    "Orange Yellow":(71.941, 19.363, 67.857),
    "Blue":         (28.778, 14.179, -50.297),
    "Green":        (55.261, -38.342, 31.370),
    "Red":          (42.101, 53.378, 28.190),
    "Yellow":       (81.733, 4.039, 79.819),
    "Magenta":      (51.935, 49.986, -14.574),
    "Cyan":         (51.038, -28.631, -28.638),
    "White":        (96.539, -0.425, 1.186),
    "Neutral 8":    (81.257, -0.638, -0.335),
    "Neutral 6.5":  (66.766, -0.734, -0.504),
    "Neutral 5":    (50.867, -0.153, -0.270),
    "Neutral 3.5":  (35.656, -0.421, -1.231),
    "Black":        (20.461, -0.079, -0.973),
}


def measure_display():
    """Run dispread and return list of (X, Y, Z) per patch."""
    ti1 = 'CTI1\nDESCRIPTOR "M"\nORIGINATOR "CP"\nCOLOR_REP "RGB"\n'
    ti1 += 'NUMBER_OF_FIELDS 4\nBEGIN_DATA_FORMAT\n'
    ti1 += 'SAMPLE_ID RGB_R RGB_G RGB_B\nEND_DATA_FORMAT\n'
    ti1 += f'NUMBER_OF_SETS {len(PATCHES)}\nBEGIN_DATA\n'
    for i, (name, r, g, b) in enumerate(PATCHES, 1):
        ti1 += f'{i} {r*100:.2f} {g*100:.2f} {b*100:.2f}\n'
    ti1 += 'END_DATA\n'

    with tempfile.TemporaryDirectory() as td:
        base = os.path.join(td, "meas")
        with open(base + ".ti1", "w") as f:
            f.write(ti1)

        proc = subprocess.Popen(
            [DISPREAD, "-d", "1", "-c", "1", "-y", "o",
             "-Y", "p", "-N", "-F", "-P", "0.5,0.5,0.4", base],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, cwd=td,
        )
        try:
            stdout, _ = proc.communicate(timeout=180)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.communicate()
            return None

        ti3 = base + ".ti3"
        if not os.path.exists(ti3):
            return None

        with open(ti3) as f:
            content = f.read()

        results = []
        in_data = False
        for line in content.split("\n"):
            if "BEGIN_DATA" in line and "FORMAT" not in line:
                in_data = True
                continue
            if "END_DATA" in line:
                in_data = False
            if in_data and line.strip():
                parts = line.split()
                if len(parts) >= 7:
                    results.append(
                        (float(parts[4]), float(parts[5]), float(parts[6]))
                    )
        return results if len(results) == len(PATCHES) else None


def compute_de(measured):
    """Compute CIEDE2000 for each patch."""
    des = []
    for (name, r, g, b), (X, Y, Z) in zip(PATCHES, measured):
        xyz_m = np.array([X, Y, Z]) / 100.0
        lab_m = xyz_to_lab(bradford_adapt(xyz_m, D65_WHITE, D50_WHITE), D50_WHITE)
        lab_r = np.array(REF_LAB[name])
        des.append(float(delta_e_2000(lab_m, lab_r)))
    return des


def compute_correction_matrix(measured):
    """Least-squares 3x3 correction matrix from measured XYZ."""
    expected = []
    meas = []
    for (name, r, g, b), (X, Y, Z) in zip(PATCHES, measured):
        rgb_lin = srgb_gamma_expand(np.array([r, g, b]))
        xyz_exp = SRGB_TO_XYZ @ rgb_lin
        expected.append(xyz_exp)
        meas.append(np.array([X, Y, Z]) / 100.0)

    E = np.array(expected).T
    M = np.array(meas).T
    return E @ M.T @ np.linalg.inv(M @ M.T)


def build_correction_lut(R_inv, size=33):
    """Build a 3D LUT that pre-corrects for measured display error."""
    lut = LUT3D.create_identity(size)
    coords = np.linspace(0, 1, size)
    EPS = 1e-10

    r_grid, g_grid, b_grid = np.meshgrid(coords, coords, coords, indexing="ij")
    all_rgb = np.stack([r_grid.ravel(), g_grid.ravel(), b_grid.ravel()], axis=1)

    # Linearize
    rgb_linear = np.where(all_rgb > EPS, np.power(all_rgb, 2.2), 0.0)

    # To XYZ
    xyz_all = (SRGB_TO_XYZ @ rgb_linear.T).T

    # Apply inverse correction
    xyz_corrected = (R_inv @ xyz_all.T).T

    # Back to linear RGB
    rgb_corrected = (XYZ_TO_SRGB @ xyz_corrected.T).T
    rgb_corrected = np.clip(rgb_corrected, 0.0, 1.0)

    # Gamma encode
    rgb_output = np.where(rgb_corrected > EPS, np.power(rgb_corrected, 1.0 / 2.2), 0.0)
    rgb_output = np.clip(rgb_output, 0.0, 1.0)

    # Black stays black
    is_black = np.all(all_rgb == 0.0, axis=1)
    rgb_output[is_black] = 0.0

    lut.data = rgb_output.reshape(size, size, size, 3)
    lut.title = "Calibrate Pro - PG27UCDM (Measured Correction)"
    return lut


def print_results(before_de, after_de):
    print()
    print(f'{"Patch":20s}  {"Before":>6s}  {"After":>6s}  {"Change":>7s}  Status')
    print("=" * 65)
    for i, (name, r, g, b) in enumerate(PATCHES):
        b_de = before_de[i]
        a_de = after_de[i]
        change = a_de - b_de
        status = "PASS" if a_de < 2.0 else "WARN" if a_de < 3.0 else "FAIL"
        arrow = "v" if change < -0.5 else "^" if change > 0.5 else "="
        print(f"  {name:20s}  {b_de:5.2f}   {a_de:5.2f}   {change:+5.2f} {arrow}  [{status}]")
    print("=" * 65)
    avg_b = np.mean(before_de)
    avg_a = np.mean(after_de)
    print(f"  Before:      avg dE {avg_b:.2f}   max dE {np.max(before_de):.2f}")
    print(f"  After:       avg dE {avg_a:.2f}   max dE {np.max(after_de):.2f}")
    pct = (1 - avg_a / avg_b) * 100 if avg_b > 0 else 0
    print(f"  Improvement: {avg_b - avg_a:.2f} dE ({pct:.0f}% reduction)")


if __name__ == "__main__":
    print("=== HYBRID MEASURED CALIBRATION ===")
    print("Display: ASUS PG27UCDM (QD-OLED)")
    print("Sensor:  NEC MDSVSENSOR3 (i1Display3)")
    print()

    # Step 1: Remove any existing LUT
    print("Step 1: Removing existing LUT...")
    try:
        from calibrate_pro.lut_system.dwm_lut import remove_lut
        remove_lut(1)
    except Exception:
        pass
    time.sleep(2)

    # Step 2: Measure uncalibrated
    print("Step 2: Measuring uncalibrated display (24 patches)...")
    uncal = measure_display()
    if not uncal:
        print("  FAILED - check sensor placement")
        sys.exit(1)
    uncal_de = compute_de(uncal)
    print(f"  Uncalibrated: avg dE {np.mean(uncal_de):.2f}, max dE {np.max(uncal_de):.2f}")

    # Step 3: Compute correction
    print("Step 3: Computing measured correction matrix...")
    R = compute_correction_matrix(uncal)
    R_inv = np.linalg.inv(R)
    print("  Correction matrix computed from real measurements")

    # Step 4: Build and apply LUT
    print("Step 4: Building 33^3 correction LUT...")
    lut = build_correction_lut(R_inv, size=33)

    lut_dir = os.path.expanduser("~/Documents/Calibrate Pro/Calibrations")
    os.makedirs(lut_dir, exist_ok=True)
    lut_path = os.path.join(lut_dir, "ASUS_PG27UCDM_measured.cube")
    lut.save(lut_path)
    print(f"  Saved: {lut_path}")

    print("Step 5: Applying measured-correction LUT...")
    try:
        from calibrate_pro.lut_system.dwm_lut import DwmLutController
        dwm = DwmLutController()
        if dwm.is_available:
            dwm.load_lut_file(1, lut_path)
            print("  Applied via DWM LUT")
    except Exception as e:
        print(f"  LUT application failed: {e}")
    time.sleep(3)

    # Step 6: Re-measure
    print("Step 6: Re-measuring with correction applied (24 patches)...")
    cal = measure_display()
    if not cal:
        print("  FAILED - check sensor placement")
        sys.exit(1)
    cal_de = compute_de(cal)

    # Print comparison
    print_results(uncal_de, cal_de)
    print()
    print("  All values MEASURED by i1Display3 colorimeter.")
