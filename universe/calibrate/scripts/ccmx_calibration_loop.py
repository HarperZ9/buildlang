"""
CCMX-Corrected Calibration Loop

Applies the CCMX spectral correction BEFORE profiling, so the display
model is built from accurate XYZ values. This should fix both the TRC
(gamma ~1.64 was wrong, should be ~2.2) and the primaries, enabling
the full TRC+gamut correction to work uniformly for all colors.

This is the final calibration pipeline:
1. CCMX corrects sensor readings (WOLED matrix -> QD-OLED accurate)
2. Profile with corrected XYZ (accurate TRC + primaries)
3. Build full correction LUT (TRC + gamut, no chroma-adaptive hack needed)
4. Verify via pre-corrected patches
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


def compute_ccmx():
    """CCMX from sensor vs EDID primaries."""
    def xy_to_XYZ(x, y, Y=1.0):
        if y == 0: return np.array([0, 0, 0])
        return np.array([(Y/y)*x, Y, (Y/y)*(1-x-y)])

    def build_matrix(r_xy, g_xy, b_xy, w_xy):
        R, G, B = xy_to_XYZ(*r_xy), xy_to_XYZ(*g_xy), xy_to_XYZ(*b_xy)
        W = xy_to_XYZ(*w_xy)
        M = np.column_stack([R, G, B])
        S = np.linalg.solve(M, W)
        return M * S[np.newaxis, :]

    M_sensor = build_matrix(
        (0.6835, 0.3060), (0.2622, 0.7006),
        (0.1481, 0.0575), (0.3134, 0.3240))
    M_true = build_matrix(
        (0.6835, 0.3164), (0.2373, 0.7080),
        (0.1396, 0.0527), (0.3134, 0.3291))
    return M_true @ np.linalg.inv(M_sensor)


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


if __name__ == "__main__":
    print("=" * 70)
    print("  CCMX-CORRECTED CALIBRATION LOOP")
    print("  Sensor correction -> Profile -> Full TRC+Gamut LUT -> Verify")
    print("=" * 70)

    CCMX = compute_ccmx()
    print("\n  CCMX applied to all measurements.")

    # Combined matrix: raw sensor freq -> CCMX-corrected XYZ
    CORRECTED_MATRIX = CCMX @ OLED_MATRIX

    device = hid.device()
    device.open(0x0765, 0x5020)
    unlock_device(device)
    print("  Sensor unlocked.")

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

    def show(r, g, b, settle=1.2):
        ri = max(0, min(255, int(r * 255 + 0.5)))
        gi = max(0, min(255, int(g * 255 + 0.5)))
        bi = max(0, min(255, int(b * 255 + 0.5)))
        canvas.config(bg=f"#{ri:02x}{gi:02x}{bi:02x}")
        root.update()
        time.sleep(settle)

    def measure_xyz(integ=1.0):
        freq = measure_freq(device, integ)
        if freq is not None and np.max(freq) > 0.3:
            return CORRECTED_MATRIX @ freq  # CCMX-corrected!
        return None

    # === Baseline (CCMX-corrected) ===
    print("\n  Step 1: Baseline measurement (CCMX-corrected)...")
    show(1.0, 1.0, 1.0)
    white_xyz = measure_xyz(1.0)
    white_Y = white_xyz[1]
    norm = 100.0 / white_Y
    print(f"    White Y = {white_Y:.1f} cd/m2")

    baseline = []
    for i, (name, r, g, b) in enumerate(COLORCHECKER):
        show(r, g, b)
        xyz = measure_xyz(1.0)
        if xyz is not None:
            lab = xyz_to_lab(bradford_adapt(xyz * norm / 100.0, D65_WHITE, D50_WHITE), D50_WHITE)
            de = float(delta_e_2000(lab, np.array(REF_LAB[name])))
            baseline.append(de)
        else:
            baseline.append(-1)
    valid_base = [d for d in baseline if d >= 0]
    print(f"    Baseline: avg dE = {np.mean(valid_base):.2f}, max = {np.max(valid_base):.2f}")

    # === Profile with CCMX-corrected measurements ===
    print("\n  Step 2: Profiling display (CCMX-corrected)...")
    n_steps = 17
    levels = np.linspace(0, 1, n_steps)

    def measure_ramp(name, make_color):
        xyz_list = []
        for v in levels:
            r, g, b = make_color(v)
            show(r, g, b, settle=1.0)
            integ = 2.0 if v < 0.05 else 1.2 if v < 0.15 else 0.8
            xyz = measure_xyz(integ)
            xyz_list.append(xyz if xyz is not None else np.zeros(3))
        return np.array(xyz_list)

    white_ramp = measure_ramp("White", lambda v: (v, v, v))
    red_ramp = measure_ramp("Red", lambda v: (v, 0, 0))
    green_ramp = measure_ramp("Green", lambda v: (0, v, 0))
    blue_ramp = measure_ramp("Blue", lambda v: (0, 0, v))

    # Black subtraction
    black = white_ramp[0].copy()
    for arr in [white_ramp, red_ramp, green_ramp, blue_ramp]:
        arr -= black
    white_ramp[0] = 0; red_ramp[0] = 0; green_ramp[0] = 0; blue_ramp[0] = 0

    profile_white_Y = white_ramp[-1][1]
    R_xyz = red_ramp[-1]; G_xyz = green_ramp[-1]; B_xyz = blue_ramp[-1]
    M_display = np.column_stack([R_xyz, G_xyz, B_xyz])

    def normalize_trc(xyz_arr, primary_Y):
        trc = np.maximum(xyz_arr[:, 1], 0)
        if primary_Y > 0: trc /= primary_Y
        trc[0] = 0.0; trc[-1] = 1.0
        for i in range(1, len(trc)): trc[i] = max(trc[i], trc[i-1])
        return trc

    trc_r = normalize_trc(red_ramp, R_xyz[1])
    trc_g = normalize_trc(green_ramp, G_xyz[1])
    trc_b = normalize_trc(blue_ramp, B_xyz[1])

    def _xy(xyz):
        s = np.sum(xyz)
        return (xyz[0]/s, xyz[1]/s) if s > 0 else (0, 0)

    def est_gamma(trc):
        mid = trc[n_steps//2]
        return np.log(mid)/np.log(0.5) if 0 < mid < 1 else 2.2

    print(f"    White Y:   {profile_white_Y:.1f} cd/m2")
    print(f"    White WP:  ({_xy(white_ramp[-1])[0]:.4f}, {_xy(white_ramp[-1])[1]:.4f})")
    print(f"    R primary: ({_xy(R_xyz)[0]:.4f}, {_xy(R_xyz)[1]:.4f})")
    print(f"    G primary: ({_xy(G_xyz)[0]:.4f}, {_xy(G_xyz)[1]:.4f})")
    print(f"    B primary: ({_xy(B_xyz)[0]:.4f}, {_xy(B_xyz)[1]:.4f})")
    print(f"    Gamma: R={est_gamma(trc_r):.2f}, G={est_gamma(trc_g):.2f}, B={est_gamma(trc_b):.2f}")

    # === Build correction ===
    print("\n  Step 3: Building full TRC+gamut correction...")

    display_white = M_display @ np.array([1.0, 1.0, 1.0])
    M_norm = M_display / display_white[1]
    inv_M = np.linalg.inv(M_norm)

    inv_trc_r = interp1d(trc_r, levels, kind='linear', bounds_error=False, fill_value=(0, 1))
    inv_trc_g = interp1d(trc_g, levels, kind='linear', bounds_error=False, fill_value=(0, 1))
    inv_trc_b = interp1d(trc_b, levels, kind='linear', bounds_error=False, fill_value=(0, 1))

    srgb_white = SRGB_TO_XYZ @ np.array([1.0, 1.0, 1.0])
    dw_norm = display_white / display_white[1]
    sw_norm = srgb_white / srgb_white[1]
    wp_shift = ((dw_norm[0]/sum(dw_norm) - sw_norm[0]/sum(sw_norm))**2 +
                (dw_norm[1]/sum(dw_norm) - sw_norm[1]/sum(sw_norm))**2)**0.5

    if wp_shift > 0.003:
        source_cone = BRADFORD_MATRIX @ sw_norm
        dest_cone = BRADFORD_MATRIX @ dw_norm
        adapt = BRADFORD_INVERSE @ np.diag(dest_cone / source_cone) @ BRADFORD_MATRIX
        print(f"    WP shift: {wp_shift:.4f} -> Bradford adaptation applied")
    else:
        adapt = np.eye(3)
        print(f"    WP shift: {wp_shift:.4f} -> within tolerance")

    def correct(r, g, b):
        rgb_in = np.array([r, g, b])
        linear = srgb_gamma_expand(rgb_in)
        target_xyz = adapt @ (SRGB_TO_XYZ @ linear)
        display_linear = np.clip(inv_M @ target_xyz, 0.0, 1.0)
        corrected = np.clip(np.array([
            float(inv_trc_r(display_linear[0])),
            float(inv_trc_g(display_linear[1])),
            float(inv_trc_b(display_linear[2])),
        ]), 0.0, 1.0)

        # Near-black protection
        lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
        if lum < 0.03:
            blend = lum / 0.03
            corrected = rgb_in * (1 - blend) + corrected * blend

        return corrected

    # Show correction examples
    print("\n  Correction examples:")
    for name, r, g, b in [("White", 0.961, 0.961, 0.961), ("Mid gray", 0.5, 0.5, 0.5),
                           ("Red", 0.752, 0.197, 0.178), ("Green", 0.262, 0.584, 0.291)]:
        cr, cg, cb = correct(r, g, b)
        print(f"    {name:10s}: ({r:.3f},{g:.3f},{b:.3f}) -> ({cr:.3f},{cg:.3f},{cb:.3f})")

    # === Verify with pre-corrected patches ===
    print("\n  Step 4: Verifying with pre-corrected patches (CCMX-corrected sensor)...")

    # Re-measure white through correction
    wr, wg, wb = correct(0.961, 0.961, 0.961)
    show(wr, wg, wb)
    cal_white = measure_xyz(1.0)
    cal_white_Y = cal_white[1] if cal_white is not None else white_Y
    norm2 = 100.0 / white_Y  # Normalize to display's actual white

    corrected_des = []
    print(f"\n  {'Patch':20s}  {'Before':>6s}  {'After':>6s}  {'Change':>7s}  Status")
    print("  " + "=" * 62)

    for i, (name, r, g, b) in enumerate(COLORCHECKER):
        cr, cg, cb = correct(r, g, b)
        show(cr, cg, cb)
        xyz = measure_xyz(1.0)

        if xyz is not None:
            lab = xyz_to_lab(bradford_adapt(xyz * norm2 / 100.0, D65_WHITE, D50_WHITE), D50_WHITE)
            de = float(delta_e_2000(lab, np.array(REF_LAB[name])))
            corrected_des.append(de)

            b_de = baseline[i]
            if b_de >= 0:
                change = de - b_de
                status = "PASS" if de < 2.0 else "WARN" if de < 3.0 else "FAIL"
                arrow = "v" if change < -0.5 else "^" if change > 0.5 else "~"
                print(f"  {name:20s}  {b_de:5.2f}   {de:5.2f}   {change:+5.2f} {arrow}  [{status}]")
            else:
                print(f"  {name:20s}   n/a   {de:5.2f}")
        else:
            corrected_des.append(-1)
            print(f"  {name:20s}  (no reading)")

    root.destroy()
    device.close()

    # Summary
    valid_cor = [d for d in corrected_des if d >= 0]
    print("  " + "=" * 62)
    if valid_base and valid_cor:
        avg_b = np.mean(valid_base)
        avg_c = np.mean(valid_cor)
        pct = (1 - avg_c / avg_b) * 100 if avg_b > 0 else 0
        passing = sum(1 for d in valid_cor if d < 3.0)

        print(f"  Before:  avg dE = {avg_b:.2f},  max = {np.max(valid_base):.2f}")
        print(f"  After:   avg dE = {avg_c:.2f},  max = {np.max(valid_cor):.2f}")
        print(f"  Passing: {passing}/{len(valid_cor)} patches < 3.0")

        if avg_c < avg_b:
            print(f"\n  IMPROVEMENT: {pct:.0f}% ({avg_b - avg_c:.2f} dE)")
        else:
            print(f"\n  No improvement ({pct:+.0f}%)")
    print("=" * 70)
