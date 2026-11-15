"""
CCMX Spectral Correction Verification

Computes a Colorimeter Correction Matrix (CCMX) from the difference
between sensor-reported chromaticities and EDID-reported chromaticities,
then applies it to ColorChecker measurements to verify dE improvement.

The CCMX corrects for the spectral mismatch between the sensor's
"Organic LED" EEPROM calibration (designed for WOLED) and the actual
QD-OLED emission spectrum (narrow-band quantum dot emitters).
"""
import hid, struct, time, sys, os
import numpy as np
import tkinter as tk

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from calibrate_pro.core.color_math import (
    xyz_to_lab, bradford_adapt, delta_e_2000, D50_WHITE, D65_WHITE,
    srgb_gamma_expand, SRGB_TO_XYZ
)

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


def compute_ccmx():
    """
    Compute CCMX from sensor-measured vs EDID primaries.

    The sensor + OLED EEPROM matrix reports certain chromaticities for
    the display's primaries. The EDID contains the manufacturer's measured
    chromaticities. The difference is the spectral correction.
    """
    def xy_to_XYZ(x, y, Y=1.0):
        if y == 0: return np.array([0, 0, 0])
        return np.array([(Y/y)*x, Y, (Y/y)*(1-x-y)])

    def build_matrix(r_xy, g_xy, b_xy, w_xy):
        R = xy_to_XYZ(*r_xy)
        G = xy_to_XYZ(*g_xy)
        B = xy_to_XYZ(*b_xy)
        W = xy_to_XYZ(*w_xy)
        M = np.column_stack([R, G, B])
        S = np.linalg.solve(M, W)
        return M * S[np.newaxis, :]

    # Sensor-reported (what our sensor + OLED matrix says)
    M_sensor = build_matrix(
        (0.6835, 0.3060), (0.2622, 0.7006),
        (0.1481, 0.0575), (0.3134, 0.3240)
    )

    # EDID-reported (what the display actually produces)
    # PG27UCDM EDID chromaticity, Samsung QD-OLED gen 3
    M_true = build_matrix(
        (0.6835, 0.3164), (0.2373, 0.7080),
        (0.1396, 0.0527), (0.3134, 0.3291)
    )

    return M_true @ np.linalg.inv(M_sensor)


if __name__ == "__main__":
    print("=" * 70)
    print("  CCMX SPECTRAL CORRECTION VERIFICATION")
    print("  Correcting WOLED sensor matrix for QD-OLED emission spectrum")
    print("=" * 70)

    # Compute CCMX
    CCMX = compute_ccmx()
    print("\n  CCMX (spectral correction matrix):")
    for row in CCMX:
        print(f"    [{row[0]:+.6f}, {row[1]:+.6f}, {row[2]:+.6f}]")

    # Connect sensor
    print("\n  Connecting sensor...")
    device = hid.device()
    device.open(0x0765, 0x5020)
    unlock_device(device)
    print("  Sensor ready.")

    # Remove any LUTs
    try:
        from calibrate_pro.lut_system.dwm_lut import remove_lut
        remove_lut(0)
        time.sleep(1)
    except Exception:
        pass

    # Find display
    from calibrate_pro.panels.detection import enumerate_displays
    displays = enumerate_displays()
    dx, dy, dw, dh = 0, 0, 3840, 2160
    for d in displays:
        if d.width == 3840:
            dx, dy, dw, dh = d.position_x, d.position_y, d.width, d.height
            break

    root = tk.Tk()
    root.overrideredirect(True)
    root.attributes("-topmost", True)
    root.geometry(f"{dw}x{dh}+{dx}+{dy}")
    canvas = tk.Canvas(root, highlightthickness=0, cursor="none")
    canvas.pack(fill=tk.BOTH, expand=True)

    def show(r, g, b, settle=1.5):
        ri = max(0, min(255, int(r * 255 + 0.5)))
        gi = max(0, min(255, int(g * 255 + 0.5)))
        bi = max(0, min(255, int(b * 255 + 0.5)))
        canvas.config(bg=f"#{ri:02x}{gi:02x}{bi:02x}")
        root.update()
        time.sleep(settle)

    # Measure white for normalization
    show(1.0, 1.0, 1.0)
    wf = measure_freq(device, 1.0)
    raw_white = OLED_MATRIX @ wf
    corrected_white = CCMX @ raw_white
    raw_Y = raw_white[1]
    corrected_Y = corrected_white[1]

    print(f"\n  Raw white Y:       {raw_Y:.1f} cd/m2")
    print(f"  Corrected white Y: {corrected_Y:.1f} cd/m2")

    # Measure all ColorChecker patches
    print(f"\n  Measuring 24 patches (raw vs CCMX-corrected)...\n")
    print(f"  {'Patch':20s}  {'Raw dE':>7s}  {'CCMX dE':>7s}  {'Change':>7s}  Status")
    print("  " + "=" * 65)

    raw_des = []
    ccmx_des = []

    for i, (name, r, g, b) in enumerate(COLORCHECKER):
        show(r, g, b)
        freq = measure_freq(device, 1.0)

        if freq is not None and np.max(freq) > 0.3:
            # Raw XYZ (no correction)
            xyz_raw = OLED_MATRIX @ freq
            norm_raw = 100.0 / raw_Y
            xyz_norm_raw = xyz_raw * norm_raw / 100.0
            lab_raw = xyz_to_lab(bradford_adapt(xyz_norm_raw, D65_WHITE, D50_WHITE), D50_WHITE)
            de_raw = float(delta_e_2000(lab_raw, np.array(REF_LAB[name])))

            # CCMX-corrected XYZ
            xyz_ccmx = CCMX @ xyz_raw
            norm_ccmx = 100.0 / corrected_Y
            xyz_norm_ccmx = xyz_ccmx * norm_ccmx / 100.0
            lab_ccmx = xyz_to_lab(bradford_adapt(xyz_norm_ccmx, D65_WHITE, D50_WHITE), D50_WHITE)
            de_ccmx = float(delta_e_2000(lab_ccmx, np.array(REF_LAB[name])))

            change = de_ccmx - de_raw
            status = "PASS" if de_ccmx < 2.0 else "WARN" if de_ccmx < 3.0 else "FAIL"
            arrow = "v" if change < -0.5 else "^" if change > 0.5 else "~"

            print(f"  {name:20s}  {de_raw:6.2f}   {de_ccmx:6.2f}   {change:+5.2f} {arrow}  [{status}]")
            raw_des.append(de_raw)
            ccmx_des.append(de_ccmx)
        else:
            print(f"  {name:20s}  (no reading)")
            raw_des.append(-1)
            ccmx_des.append(-1)

    root.destroy()
    device.close()

    # Summary
    valid_raw = [d for d in raw_des if d >= 0]
    valid_ccmx = [d for d in ccmx_des if d >= 0]

    print("  " + "=" * 65)
    if valid_raw and valid_ccmx:
        avg_raw = np.mean(valid_raw)
        avg_ccmx = np.mean(valid_ccmx)
        pct = (1 - avg_ccmx / avg_raw) * 100 if avg_raw > 0 else 0
        passing = sum(1 for d in valid_ccmx if d < 3.0)

        print(f"  Raw:      avg dE = {avg_raw:.2f},  max = {np.max(valid_raw):.2f}")
        print(f"  CCMX:     avg dE = {avg_ccmx:.2f},  max = {np.max(valid_ccmx):.2f}")
        print(f"  Passing:  {passing}/{len(valid_ccmx)} patches < 3.0")

        if avg_ccmx < avg_raw:
            print(f"\n  CCMX IMPROVED accuracy by {pct:.0f}% ({avg_raw - avg_ccmx:.2f} dE)")
        else:
            print(f"\n  CCMX did not improve (may need EDID refinement)")
    print("=" * 70)
