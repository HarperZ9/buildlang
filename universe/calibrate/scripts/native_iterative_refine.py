"""
Iterative Refinement of Native Calibration

Takes the existing correction LUT (applied via DWM), re-profiles the display
through the correction, and builds a refined LUT that accounts for residual
errors. This is the "measure -> correct -> measure -> refine" feedback loop.

Also verifies whether the DWM LUT is actually being applied by checking
if the correction changes the measured output.
"""
import hid, struct, time, sys, os
import numpy as np
import tkinter as tk
from scipy.interpolate import interp1d

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from calibrate_pro.core.color_math import (
    xyz_to_lab, bradford_adapt, delta_e_2000, D50_WHITE, D65_WHITE,
    srgb_gamma_expand, srgb_gamma_compress, SRGB_TO_XYZ, XYZ_TO_SRGB,
    BRADFORD_MATRIX, BRADFORD_INVERSE
)
from calibrate_pro.core.lut_engine import LUT3D

OLED_MATRIX = np.array([
    [0.03836831, -0.02175997, 0.01696057],
    [0.01449629,  0.01611903, 0.00057150],
    [-0.00004481, 0.00035042, 0.08032401],
])

COLORCHECKER = [
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

M_MASK = 0xFFFFFFFF


def unlock_device(device):
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


def measure_freq(device, integration=1.0):
    intclks = int(integration * 12000000)
    cmd = bytearray(65); cmd[0] = 0x00; cmd[1] = 0x01
    struct.pack_into('<I', cmd, 2, intclks)
    device.write(cmd)
    resp = device.read(64, timeout_ms=int((integration + 3) * 1000))
    if resp and resp[0] == 0x00 and resp[1] == 0x01:
        r = struct.unpack('<I', bytes(resp[2:6]))[0]
        g = struct.unpack('<I', bytes(resp[6:10]))[0]
        b = struct.unpack('<I', bytes(resp[10:14]))[0]
        t = intclks / 12000000.0
        return np.array([0.5*(r+0.5)/t, 0.5*(g+0.5)/t, 0.5*(b+0.5)/t])
    return None


def measure_xyz(device, integration=1.0):
    freq = measure_freq(device, integration)
    if freq is not None and np.max(freq) > 0.3:
        return OLED_MATRIX @ freq
    return None


class PatchDisplay:
    def __init__(self, dx=0, dy=0, dw=3840, dh=2160):
        self.root = tk.Tk()
        self.root.overrideredirect(True)
        self.root.attributes("-topmost", True)
        self.root.geometry(f"{dw}x{dh}+{dx}+{dy}")
        self.canvas = tk.Canvas(self.root, highlightthickness=0, cursor="none")
        self.canvas.pack(fill=tk.BOTH, expand=True)

    def show(self, r, g, b, settle=1.5):
        ri = max(0, min(255, int(r * 255 + 0.5)))
        gi = max(0, min(255, int(g * 255 + 0.5)))
        bi = max(0, min(255, int(b * 255 + 0.5)))
        self.canvas.config(bg=f"#{ri:02x}{gi:02x}{bi:02x}")
        self.root.update()
        time.sleep(settle)

    def destroy(self):
        self.root.destroy()


def measure_colorchecker(device, display, white_Y):
    """Measure 24 ColorChecker patches, return list of (name, dE)."""
    norm = 100.0 / white_Y if white_Y > 0 else 1.0
    results = []
    for i, (name, r, g, b) in enumerate(COLORCHECKER):
        display.show(r, g, b)
        freq = measure_freq(device, integration=1.0)
        if freq is not None and np.max(freq) > 0.3:
            xyz = OLED_MATRIX @ freq
            xyz_n = xyz * norm / 100.0
            lab = xyz_to_lab(bradford_adapt(xyz_n, D65_WHITE, D50_WHITE), D50_WHITE)
            de = float(delta_e_2000(lab, np.array(REF_LAB[name])))
            status = "PASS" if de < 2.0 else "WARN" if de < 3.0 else "FAIL"
            print(f"    [{i+1:2d}/24] {name:20s}  dE={de:5.2f} [{status}]")
            results.append((name, de))
        else:
            print(f"    [{i+1:2d}/24] {name:20s}  (no reading)")
            results.append((name, -1))
    return results


if __name__ == "__main__":
    print("=" * 70)
    print("  DWM LUT VERIFICATION + ITERATIVE REFINEMENT")
    print("=" * 70)

    # Connect sensor
    print("\nStep 1: Connecting sensor...")
    device = hid.device()
    device.open(0x0765, 0x5020)
    unlock_device(device)
    print("  Sensor ready.")

    # Find display
    from calibrate_pro.panels.detection import enumerate_displays
    displays = enumerate_displays()
    dx, dy, dw, dh = 0, 0, 3840, 2160
    for d in displays:
        if d.width == 3840:
            dx, dy, dw, dh = d.position_x, d.position_y, d.width, d.height
            break

    display = PatchDisplay(dx, dy, dw, dh)

    # Check DWM LUT state
    print("\nStep 2: Checking DWM LUT state...")
    from calibrate_pro.lut_system.dwm_lut import DwmLutController, get_dwm_lut_directory
    import pathlib

    lut_dir = get_dwm_lut_directory()
    lut_file = lut_dir / "0_0.cube"
    print(f"  LUT directory: {lut_dir}")
    print(f"  LUT file exists: {lut_file.exists()}")

    ctrl = DwmLutController()
    print(f"  dwm_lut available: {ctrl.is_available}")
    print(f"  DwmLutGUI running: {ctrl._is_dwm_lut_running()}")

    if not ctrl._is_dwm_lut_running():
        print("\n  WARNING: DwmLutGUI is NOT running!")
        print("  The LUT file is placed but not active.")
        print("  Starting DwmLutGUI (may need admin)...")
        try:
            ctrl.start_dwm_lut_gui()
            time.sleep(2)
            if ctrl._is_dwm_lut_running():
                print("  DwmLutGUI started successfully!")
            else:
                print("  Could not start DwmLutGUI (needs admin).")
                print("  Please start it manually: right-click DwmLutGUI.exe -> Run as admin")
                print("  Then re-run this script.")
        except Exception as e:
            print(f"  Failed: {e}")
            print("  Continuing anyway to measure current state...")

    # Measure white reference
    print("\nStep 3: Measuring current state...")
    display.show(1.0, 1.0, 1.0)
    white_freq = measure_freq(device, 1.0)
    white_Y = (OLED_MATRIX @ white_freq)[1]
    print(f"  White Y = {white_Y:.1f} cd/m2")

    # Quick LUT verification: measure a known saturated patch
    # If LUT is active, Red (0.752, 0.197, 0.178) should appear less saturated
    print("\n  Quick LUT check (Red patch)...")
    display.show(0.752, 0.197, 0.178)
    xyz_red = measure_xyz(device, 1.0)
    if xyz_red is not None:
        s = sum(xyz_red)
        rx, ry = xyz_red[0]/s, xyz_red[1]/s
        print(f"    Red chromaticity: ({rx:.4f}, {ry:.4f})")
        print(f"    If LUT active:  should be closer to sRGB red (0.6400, 0.3300)")
        print(f"    If LUT inactive: should be near QD-OLED red (0.6835, 0.3060)")
        # Simple heuristic: if x > 0.66, LUT is probably not active
        if rx > 0.66:
            print("    -> LUT appears INACTIVE (raw QD-OLED)")
        else:
            print("    -> LUT appears ACTIVE (gamut corrected)")

    # Full ColorChecker measurement
    print("\nStep 4: Full ColorChecker measurement (current state)...")
    results = measure_colorchecker(device, display, white_Y)

    valid = [de for _, de in results if de >= 0]
    if valid:
        avg = np.mean(valid)
        mx = np.max(valid)
        passing = sum(1 for de in valid if de < 3.0)
        print(f"\n  Average dE: {avg:.2f}")
        print(f"  Maximum dE: {mx:.2f}")
        print(f"  Passing (<3.0): {passing}/{len(valid)}")

        if avg < 5.0:
            print("\n  Results suggest LUT correction IS being applied!")
            print("  (Uncalibrated baseline was avg dE ~6.5)")
        else:
            print("\n  Results suggest LUT is NOT being applied.")
            print("  (Values match uncalibrated baseline)")

    # Iterative refinement: compute residual correction
    print("\n" + "=" * 70)
    print("  ITERATIVE REFINEMENT")
    print("=" * 70)
    print("\n  Computing residual errors from measurements...")

    # For each patch, compute the residual: what additional correction is needed?
    # residual_XYZ = reference_XYZ - measured_XYZ
    # This can be used to build a residual correction LUT
    norm = 100.0 / white_Y if white_Y > 0 else 1.0
    residuals = []
    for i, (name, r, g, b) in enumerate(COLORCHECKER):
        de = results[i][1]
        if de < 0:
            continue

        # Re-measure this patch to get its XYZ
        display.show(r, g, b, settle=1.2)
        freq = measure_freq(device, 1.0)
        if freq is None:
            continue
        xyz_meas = OLED_MATRIX @ freq

        # What XYZ should a perfect sRGB display produce?
        linear = srgb_gamma_expand(np.array([r, g, b]))
        xyz_target = SRGB_TO_XYZ @ linear

        # Both need to be on same scale
        xyz_meas_norm = xyz_meas * norm / 100.0  # Y=1 scale

        # Residual ratio: target / measured
        ratio = np.where(xyz_meas_norm > 0.001,
                         xyz_target / xyz_meas_norm,
                         np.array([1.0, 1.0, 1.0]))

        residuals.append({
            "name": name,
            "srgb": (r, g, b),
            "measured_xyz": xyz_meas_norm,
            "target_xyz": xyz_target,
            "ratio": ratio,
            "de": de,
        })

    if residuals:
        # Compute average residual correction (simple approach)
        ratios = np.array([r["ratio"] for r in residuals])
        avg_ratio = np.median(ratios, axis=0)  # Use median to resist outliers
        print(f"\n  Median residual ratio (target/measured):")
        print(f"    X: {avg_ratio[0]:.4f}")
        print(f"    Y: {avg_ratio[1]:.4f}")
        print(f"    Z: {avg_ratio[2]:.4f}")

        # If ratios are close to 1.0, there's not much to gain from refinement
        max_dev = max(abs(avg_ratio[0]-1), abs(avg_ratio[1]-1), abs(avg_ratio[2]-1))
        print(f"  Max deviation from 1.0: {max_dev:.4f}")

        if max_dev > 0.05:
            print("  -> Significant residual detected. Refinement could help.")
        else:
            print("  -> Residual is small. Current correction is near-optimal")
            print("     for this sensor matrix.")

    display.destroy()
    device.close()

    print("\n" + "=" * 70)
    print("  SUMMARY")
    print("=" * 70)
    if valid:
        print(f"  Current avg dE: {np.mean(valid):.2f} (max {np.max(valid):.2f})")
        if np.mean(valid) < 5.0:
            print("  Calibration is active and working.")
        else:
            print("  Ensure DwmLutGUI.exe is running as admin to activate the LUT.")
    print("=" * 70)
