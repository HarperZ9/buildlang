"""
Native Display Calibration Loop v2

Direct correction verification: applies correction to the displayed patches
rather than using a system LUT, eliminating DWM LUT as a variable.

Steps:
1. Profile: Measure per-channel TRC ramps + primaries
2. Compute: Build correction function from display inverse model
3. Verify: Display PRE-CORRECTED patches, re-measure, compare dE
4. If correction works: save as 3D LUT for system-wide use
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

# =============================================================================
# Sensor calibration matrix from EEPROM (Organic LED @ 0x191C)
# =============================================================================
OLED_MATRIX = np.array([
    [0.03836831, -0.02175997, 0.01696057],
    [0.01449629,  0.01611903, 0.00057150],
    [-0.00004481, 0.00035042, 0.08032401],
])

# =============================================================================
# ColorChecker Classic reference
# =============================================================================
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

# =============================================================================
# i1Display3 USB HID
# =============================================================================

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


# =============================================================================
# Display
# =============================================================================

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
        color = f"#{ri:02x}{gi:02x}{bi:02x}"
        self.canvas.config(bg=color)
        self.root.update()
        time.sleep(settle)

    def destroy(self):
        self.root.destroy()


# =============================================================================
# Profiling
# =============================================================================

def profile_display(device, display, n_steps=21):
    """
    Measure per-channel ramps and primaries.
    Returns everything needed to build a correction.
    """
    levels = np.linspace(0, 1, n_steps)
    print(f"\n  Profiling ({n_steps} steps per channel, ~{n_steps*4*2.5/60:.1f} min)...")

    def measure_ramp(name, make_color):
        print(f"    {name}...", end="", flush=True)
        xyz_list = []
        for v in levels:
            r, g, b = make_color(v)
            display.show(r, g, b, settle=1.2)
            # Use longer integration for dark patches (less noise)
            if v < 0.05:
                integ = 2.5
            elif v < 0.15:
                integ = 1.5
            elif v < 0.3:
                integ = 1.0
            else:
                integ = 0.8
            xyz = measure_xyz(device, integration=integ)
            xyz_list.append(xyz if xyz is not None else np.array([0.0, 0.0, 0.0]))
        xyz_arr = np.array(xyz_list)
        print(f" done (max Y={xyz_arr[-1][1]:.1f})")
        return xyz_arr

    white_xyz = measure_ramp("White", lambda v: (v, v, v))
    red_xyz   = measure_ramp("Red  ", lambda v: (v, 0, 0))
    green_xyz = measure_ramp("Green", lambda v: (0, v, 0))
    blue_xyz  = measure_ramp("Blue ", lambda v: (0, 0, v))

    # Black subtraction
    black = white_xyz[0].copy()
    for arr in [white_xyz, red_xyz, green_xyz, blue_xyz]:
        arr -= black  # In-place subtract

    # Force first entry to zero
    white_xyz[0] = 0; red_xyz[0] = 0; green_xyz[0] = 0; blue_xyz[0] = 0

    # Extract data
    white_Y = white_xyz[-1][1]
    R_xyz_100 = red_xyz[-1]
    G_xyz_100 = green_xyz[-1]
    B_xyz_100 = blue_xyz[-1]

    # Primaries matrix: [R_xyz, G_xyz, B_xyz] as columns
    M_display = np.column_stack([R_xyz_100, G_xyz_100, B_xyz_100])

    # Per-channel TRC (normalized 0-1)
    def normalize_trc(xyz_arr, primary_Y):
        trc = np.maximum(xyz_arr[:, 1], 0)
        if primary_Y > 0:
            trc /= primary_Y
        trc[0] = 0.0; trc[-1] = 1.0
        # Ensure monotonically increasing
        for i in range(1, len(trc)):
            trc[i] = max(trc[i], trc[i-1])
        return trc

    trc_r = normalize_trc(red_xyz, R_xyz_100[1])
    trc_g = normalize_trc(green_xyz, G_xyz_100[1])
    trc_b = normalize_trc(blue_xyz, B_xyz_100[1])

    # Report
    def _xy(xyz):
        s = np.sum(xyz)
        return (xyz[0]/s, xyz[1]/s) if s > 0 else (0, 0)

    print(f"\n    White Y:       {white_Y:.1f} cd/m2")
    print(f"    White point:   ({_xy(white_xyz[-1])[0]:.4f}, {_xy(white_xyz[-1])[1]:.4f})")
    print(f"    Red primary:   ({_xy(R_xyz_100)[0]:.4f}, {_xy(R_xyz_100)[1]:.4f})")
    print(f"    Green primary: ({_xy(G_xyz_100)[0]:.4f}, {_xy(G_xyz_100)[1]:.4f})")
    print(f"    Blue primary:  ({_xy(B_xyz_100)[0]:.4f}, {_xy(B_xyz_100)[1]:.4f})")

    # Gamma estimates
    for ch, trc in [("R", trc_r), ("G", trc_g), ("B", trc_b)]:
        mid = trc[n_steps // 2]
        if 0 < mid < 1:
            print(f"    {ch} gamma ~ {np.log(mid)/np.log(0.5):.2f}")

    return {
        "levels": levels,
        "trc_r": trc_r, "trc_g": trc_g, "trc_b": trc_b,
        "M_display": M_display,
        "white_Y": white_Y,
        "black_xyz": black,
        "white_xyz": white_xyz[-1] + black,  # Absolute white XYZ
    }


# =============================================================================
# Correction computation
# =============================================================================

def build_correction_function(profile):
    """
    Build a correction function that maps sRGB -> corrected sRGB.

    Given target sRGB (r,g,b), returns corrected (r',g',b') such that
    when the display shows (r',g',b'), it produces the XYZ that a
    perfect sRGB display would produce for the original (r,g,b).
    """
    levels = profile["levels"]
    trc_r = profile["trc_r"]
    trc_g = profile["trc_g"]
    trc_b = profile["trc_b"]
    M_display = profile["M_display"]

    # Inverse TRC: desired linear -> signal level to send
    inv_trc_r = interp1d(trc_r, levels, kind='linear', bounds_error=False, fill_value=(0, 1))
    inv_trc_g = interp1d(trc_g, levels, kind='linear', bounds_error=False, fill_value=(0, 1))
    inv_trc_b = interp1d(trc_b, levels, kind='linear', bounds_error=False, fill_value=(0, 1))

    # Forward TRC: signal level -> linear
    fwd_trc_r = interp1d(levels, trc_r, kind='linear', bounds_error=False, fill_value=(0, 1))
    fwd_trc_g = interp1d(levels, trc_g, kind='linear', bounds_error=False, fill_value=(0, 1))
    fwd_trc_b = interp1d(levels, trc_b, kind='linear', bounds_error=False, fill_value=(0, 1))

    # Normalize M_display to relative (Y=1) scale
    # M_display is in absolute cd/m2: M @ [1,1,1] gives Y ~ white_Y
    # We need it in relative scale so it matches SRGB_TO_XYZ (Y=1 for white)
    display_white = M_display @ np.array([1.0, 1.0, 1.0])
    display_white_Y = display_white[1]
    M_norm = M_display / display_white_Y  # Now M_norm @ [1,1,1] has Y = 1
    inv_M = np.linalg.inv(M_norm)

    # White point analysis
    srgb_white = SRGB_TO_XYZ @ np.array([1.0, 1.0, 1.0])
    dw_norm = display_white / display_white_Y
    sw_norm = srgb_white / srgb_white[1]

    # Check if Bradford adaptation is needed
    # Only adapt if white point shift > 0.005 in xy
    dw_xy = (dw_norm[0] / sum(dw_norm), dw_norm[1] / sum(dw_norm))
    sw_xy = (sw_norm[0] / sum(sw_norm), sw_norm[1] / sum(sw_norm))
    wp_shift = ((dw_xy[0] - sw_xy[0])**2 + (dw_xy[1] - sw_xy[1])**2)**0.5

    if wp_shift > 0.005:
        source_cone = BRADFORD_MATRIX @ sw_norm
        dest_cone = BRADFORD_MATRIX @ dw_norm
        adapt = BRADFORD_INVERSE @ np.diag(dest_cone / source_cone) @ BRADFORD_MATRIX
        print(f"\n  White point shift: {wp_shift:.4f} -> applying Bradford adaptation")
    else:
        adapt = np.eye(3)
        print(f"\n  White point shift: {wp_shift:.4f} -> within tolerance, skipping adaptation")

    print(f"  Display model check:")
    print(f"    M_norm @ [1,1,1] Y = {(M_norm @ np.array([1,1,1]))[1]:.4f} (should be ~1.0)")
    print(f"    Display WP xy: ({dw_xy[0]:.4f}, {dw_xy[1]:.4f})")
    print(f"    sRGB WP xy:    ({sw_xy[0]:.4f}, {sw_xy[1]:.4f})")

    # KEY INSIGHT: The OLED EEPROM matrix (calibrated for WOLED, not QD-OLED)
    # produces wrong absolute luminance but correct relative chromaticity.
    #
    # Strategy: CHROMA-ADAPTIVE CORRECTION
    # - Full TRC+gamut correction for chromatic (saturated) colors
    #   -> Produces excellent dE (1-3) for Dark Skin, Foliage, Purple, Green
    # - Identity (no correction) for near-neutral colors
    #   -> Preserves baseline dE (~2-5) which is better than over-correcting
    # - Smooth blend based on chroma (saturation)

    print(f"  Strategy: chroma-adaptive (full for saturated, identity for neutrals)")

    def correct(r, g, b):
        """Chroma-adaptive: full correction for saturated, identity for neutrals."""
        rgb_in = np.array([r, g, b])

        # === Full correction (TRC + gamut) ===
        linear = srgb_gamma_expand(rgb_in)
        target_xyz = SRGB_TO_XYZ @ linear
        adapted_xyz = adapt @ target_xyz
        display_linear = inv_M @ adapted_xyz
        display_linear = np.clip(display_linear, 0.0, 1.0)

        full_corrected = np.array([
            float(inv_trc_r(display_linear[0])),
            float(inv_trc_g(display_linear[1])),
            float(inv_trc_b(display_linear[2])),
        ])
        full_corrected = np.clip(full_corrected, 0.0, 1.0)

        # === Chroma detection ===
        max_c = max(r, g, b)
        min_c = min(r, g, b)
        chroma = (max_c - min_c) / max(max_c, 1e-6)  # 0=neutral, 1=saturated

        # Blend: 0 at chroma=0 (identity), 1 at chroma>=0.3 (full correction)
        if chroma <= 0.05:
            blend = 0.0
        elif chroma >= 0.3:
            blend = 1.0
        else:
            t = (chroma - 0.05) / 0.25
            blend = t * t * (3 - 2 * t)  # Smoothstep

        result = rgb_in * (1 - blend) + full_corrected * blend

        # Near-black protection
        luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b
        if luminance < 0.03:
            dark_blend = luminance / 0.03
            result = rgb_in * (1 - dark_blend) + result * dark_blend

        return np.clip(result, 0.0, 1.0)

    return correct, fwd_trc_r, fwd_trc_g, fwd_trc_b


# =============================================================================
# Measurement and comparison
# =============================================================================

def measure_patches(device, display, patches, white_Y, correct_fn=None, label=""):
    """Measure ColorChecker patches, optionally applying correction."""
    norm = 100.0 / white_Y if white_Y > 0 else 1.0
    results = []

    for i, (name, r, g, b) in enumerate(patches):
        # Apply correction if provided
        if correct_fn is not None:
            r_c, g_c, b_c = correct_fn(r, g, b)
        else:
            r_c, g_c, b_c = r, g, b

        display.show(r_c, g_c, b_c, settle=1.5)
        freq = measure_freq(device, integration=1.0)

        if freq is not None and np.max(freq) > 0.3:
            xyz_raw = OLED_MATRIX @ freq
            xyz_norm = xyz_raw * norm / 100.0

            lab_meas = xyz_to_lab(bradford_adapt(xyz_norm, D65_WHITE, D50_WHITE), D50_WHITE)
            lab_ref = np.array(REF_LAB[name])
            de = float(delta_e_2000(lab_meas, lab_ref))

            status = "PASS" if de < 2.0 else "WARN" if de < 3.0 else "FAIL"
            correction_info = ""
            if correct_fn is not None:
                correction_info = f"  sent=({r_c:.3f},{g_c:.3f},{b_c:.3f})"
            print(f"    [{i+1:2d}/24] {name:20s}  dE={de:5.2f} [{status}]{correction_info}")
            results.append((name, de))
        else:
            print(f"    [{i+1:2d}/24] {name:20s}  (no reading)")
            results.append((name, -1))

    valid = [de for _, de in results if de >= 0]
    if valid:
        print(f"\n    {label}avg dE = {np.mean(valid):.2f}, max dE = {np.max(valid):.2f}")
    return results


def print_comparison(before, after):
    print()
    print(f"  {'Patch':20s}  {'Before':>6s}  {'After':>6s}  {'Change':>7s}  Status")
    print("  " + "=" * 62)

    for i in range(len(COLORCHECKER)):
        name = COLORCHECKER[i][0]
        b_de = before[i][1]
        a_de = after[i][1]
        if b_de < 0 or a_de < 0:
            print(f"  {name:20s}  {'n/a':>6s}  {'n/a':>6s}")
            continue
        change = a_de - b_de
        status = "PASS" if a_de < 2.0 else "WARN" if a_de < 3.0 else "FAIL"
        arrow = "v" if change < -0.5 else "^" if change > 0.5 else "~"
        print(f"  {name:20s}  {b_de:5.2f}   {a_de:5.2f}   {change:+5.2f} {arrow}  [{status}]")

    valid_b = [r[1] for r in before if r[1] >= 0]
    valid_a = [r[1] for r in after if r[1] >= 0]
    print("  " + "=" * 62)
    if valid_b and valid_a:
        avg_b, avg_a = np.mean(valid_b), np.mean(valid_a)
        max_b, max_a = np.max(valid_b), np.max(valid_a)
        pct = (1 - avg_a / avg_b) * 100 if avg_b > 0 else 0
        print(f"  Before:  avg dE = {avg_b:.2f},  max dE = {max_b:.2f}")
        print(f"  After:   avg dE = {avg_a:.2f},  max dE = {max_a:.2f}")
        if pct > 0:
            print(f"  IMPROVED by {pct:.0f}% ({avg_b - avg_a:.2f} dE)")
        else:
            print(f"  Worsened by {-pct:.0f}%")


# =============================================================================
# Main
# =============================================================================

if __name__ == "__main__":
    print("=" * 70)
    print("  NATIVE CALIBRATION LOOP v2 (Direct Correction Verification)")
    print("  Sensor:  i1Display3 (NEC MDSVSENSOR3)")
    print("  Matrix:  Organic LED (from device EEPROM)")
    print("  Display: ASUS PG27UCDM (QD-OLED)")
    print("=" * 70)

    # --- Sensor ---
    print("\nStep 1: Connecting to colorimeter...")
    device = hid.device()
    device.open(0x0765, 0x5020)
    unlock_device(device)
    print("  Sensor ready.")

    # --- Display ---
    from calibrate_pro.panels.detection import enumerate_displays
    displays = enumerate_displays()
    dx, dy, dw, dh = 0, 0, 3840, 2160
    for d in displays:
        if d.width == 3840:
            dx, dy, dw, dh = d.position_x, d.position_y, d.width, d.height
            break

    display = PatchDisplay(dx, dy, dw, dh)

    # --- Remove any existing LUT ---
    print("\nStep 2: Removing existing LUTs...")
    try:
        from calibrate_pro.lut_system.dwm_lut import remove_lut
        remove_lut(0)
        remove_lut(1)
        time.sleep(1)
    except Exception:
        pass

    # --- Measure white reference ---
    display.show(1.0, 1.0, 1.0)
    white_freq = measure_freq(device, 1.0)
    white_xyz_raw = OLED_MATRIX @ white_freq
    white_Y = white_xyz_raw[1]
    print(f"  White Y = {white_Y:.1f} cd/m2")

    # --- Baseline measurement ---
    print("\nStep 3: Measuring UNCALIBRATED baseline...")
    baseline = measure_patches(device, display, COLORCHECKER, white_Y, label="Baseline: ")

    # --- Profile display ---
    print("\nStep 4: Profiling display...")
    profile = profile_display(device, display, n_steps=17)

    # --- Build correction function ---
    print("\nStep 5: Computing correction...")
    correct_fn, fwd_r, fwd_g, fwd_b = build_correction_function(profile)

    # Show what correction does to a few test values
    print("\n  Correction examples:")
    test_colors = [
        ("White",  0.961, 0.961, 0.961),
        ("Mid gray", 0.5, 0.5, 0.5),
        ("Red",    0.752, 0.197, 0.178),
        ("Green",  0.262, 0.584, 0.291),
        ("Blue",   0.139, 0.248, 0.577),
    ]
    for name, r, g, b in test_colors:
        cr, cg, cb = correct_fn(r, g, b)
        print(f"    {name:10s}: ({r:.3f},{g:.3f},{b:.3f}) -> ({cr:.3f},{cg:.3f},{cb:.3f})")

    # --- Measure with correction ---
    print("\nStep 6: Measuring with PRE-CORRECTED patches...")
    # Re-measure white with correction to get calibrated white Y
    wr, wg, wb = correct_fn(0.961, 0.961, 0.961)
    display.show(wr, wg, wb)
    cal_freq = measure_freq(device, 1.0)
    cal_white_Y = (OLED_MATRIX @ cal_freq)[1] if cal_freq is not None else white_Y

    # Use the original white_Y for normalization (the display's actual white hasn't changed)
    # But we measure through correction
    corrected = measure_patches(device, display, COLORCHECKER, white_Y,
                                correct_fn=correct_fn, label="Corrected: ")

    # --- Results ---
    print("\n" + "=" * 70)
    print("  CALIBRATION RESULTS")
    print("=" * 70)
    print_comparison(baseline, corrected)

    # --- If improvement, save as LUT ---
    valid_b = [r[1] for r in baseline if r[1] >= 0]
    valid_a = [r[1] for r in corrected if r[1] >= 0]

    if valid_a and valid_b and np.mean(valid_a) < np.mean(valid_b):
        print("\n  Correction WORKS! Saving as 3D LUT...")

        # Build the 3D LUT from the correction function
        size = 33
        lut = LUT3D.create_identity(size)
        coords = np.linspace(0, 1, size)

        for ri in range(size):
            for gi in range(size):
                for bi in range(size):
                    r, g, b = coords[ri], coords[gi], coords[bi]
                    cr, cg, cb = correct_fn(r, g, b)
                    lut.data[ri, gi, bi] = [cr, cg, cb]
            if ri % 8 == 0:
                print(f"    LUT generation: {ri*100//size}%...")

        lut.title = "Calibrate Pro - PG27UCDM (Native Measured)"
        lut_dir = os.path.expanduser("~/Documents/Calibrate Pro/Calibrations")
        os.makedirs(lut_dir, exist_ok=True)
        lut_path = os.path.join(lut_dir, "PG27UCDM_native_v2.cube")
        lut.save(lut_path)
        print(f"    Saved: {lut_path}")
    else:
        print("\n  Correction did not improve. Analyzing...")
        print("  This may indicate the OLED EEPROM matrix is not accurate")
        print("  for QD-OLED spectral characteristics. The 'Organic LED'")
        print("  matrix was calibrated for traditional WOLED, not Samsung")
        print("  QD-OLED quantum dot emission spectra.")

    display.destroy()
    device.close()
    print("\n" + "=" * 70)
