"""
Native Display Calibration Loop

Full measurement-to-correction pipeline using the i1Display3 native USB driver.
No ArgyllCMS required.

Steps:
1. Profile: Measure per-channel TRC ramps + white point
2. Compute: Build 3D correction LUT from display inverse model
3. Apply: Load via dwm_lut
4. Verify: Re-measure ColorChecker patches, compare before/after dE
"""
import hid, struct, time, sys, os
import numpy as np
import tkinter as tk
from scipy.interpolate import interp1d

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from calibrate_pro.core.color_math import (
    xyz_to_lab, bradford_adapt, delta_e_2000, D50_WHITE, D65_WHITE,
    srgb_gamma_expand, srgb_gamma_compress, SRGB_TO_XYZ, XYZ_TO_SRGB
)
from calibrate_pro.core.lut_engine import LUT3D

# =============================================================================
# Sensor: OLED calibration matrix from device EEPROM at offset 0x191C
# =============================================================================
OLED_MATRIX = np.array([
    [0.03836831, -0.02175997, 0.01696057],
    [0.01449629,  0.01611903, 0.00057150],
    [-0.00004481, 0.00035042, 0.08032401],
])

# =============================================================================
# ColorChecker Classic reference data
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

M = 0xFFFFFFFF

# =============================================================================
# i1Display3 USB HID communication
# =============================================================================

def unlock_device(device):
    """Challenge-response unlock for NEC OEM i1Display3."""
    k0, k1 = 0xa9119479, 0x5b168761
    cmd = bytearray(65); cmd[0] = 0; cmd[1] = 0x99
    device.write(cmd); time.sleep(0.2)
    c = bytes(device.read(64, timeout_ms=3000))
    sc = bytearray(8)
    for i in range(8): sc[i] = c[3] ^ c[35 + i]
    ci0 = (sc[3]<<24)+(sc[0]<<16)+(sc[4]<<8)+sc[6]
    ci1 = (sc[1]<<24)+(sc[7]<<16)+(sc[2]<<8)+sc[5]
    nk0, nk1 = (-k0) & M, (-k1) & M
    co = [(nk0-ci1)&M, (nk1-ci0)&M, (ci1*nk0)&M, (ci0*nk1)&M]
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
    """Frequency measurement mode — returns raw RGB sensor counts/sec."""
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


def freq_to_xyz(freq, matrix=OLED_MATRIX):
    """Convert raw sensor frequencies to CIE XYZ using calibration matrix."""
    return matrix @ freq


def measure_xyz(device, integration=1.0):
    """Single measurement: returns normalized XYZ (Y=1 scale)."""
    freq = measure_freq(device, integration)
    if freq is not None and np.max(freq) > 0.5:
        return freq_to_xyz(freq)
    return None


# =============================================================================
# Display patch presentation
# =============================================================================

class PatchDisplay:
    """Fullscreen patch display on the target monitor."""

    def __init__(self, dx=0, dy=0, dw=3840, dh=2160):
        self.root = tk.Tk()
        self.root.overrideredirect(True)
        self.root.attributes("-topmost", True)
        self.root.geometry(f"{dw}x{dh}+{dx}+{dy}")
        self.canvas = tk.Canvas(self.root, highlightthickness=0, cursor="none")
        self.canvas.pack(fill=tk.BOTH, expand=True)

    def show(self, r, g, b, settle=1.5):
        """Display an sRGB color and wait for OLED settle time."""
        ri, gi, bi = int(r * 255), int(g * 255), int(b * 255)
        color = f"#{ri:02x}{gi:02x}{bi:02x}"
        self.canvas.config(bg=color)
        self.root.update()
        time.sleep(settle)

    def destroy(self):
        self.root.destroy()


# =============================================================================
# Display profiling
# =============================================================================

def profile_display(device, display, n_steps=17):
    """
    Profile the display by measuring per-channel TRC ramps.

    Measures:
    - R-only ramp (n_steps levels)
    - G-only ramp (n_steps levels)
    - B-only ramp (n_steps levels)
    - White ramp (n_steps levels, used for normalization)

    Returns:
        white_Y:   Peak white luminance (cd/m²)
        trc_r, trc_g, trc_b:  Per-channel TRC as (signal_levels, measured_Y) arrays
        M_display: 3x3 measured primaries-to-XYZ matrix
    """
    levels = np.linspace(0, 1, n_steps)

    print(f"\n  Profiling display ({n_steps} steps per channel)...")

    # --- White ramp (for normalization and combined TRC) ---
    print("  Measuring white ramp...")
    white_xyz = []
    for v in levels:
        display.show(v, v, v)
        xyz = measure_xyz(device, integration=0.8 if v > 0.1 else 1.5)
        white_xyz.append(xyz if xyz is not None else np.array([0.0, 0.0, 0.0]))

    white_Y = white_xyz[-1][1]  # Peak white luminance
    print(f"    White Y = {white_Y:.2f} cd/m²")

    # --- Red ramp ---
    print("  Measuring red ramp...")
    red_xyz = []
    for v in levels:
        display.show(v, 0, 0)
        xyz = measure_xyz(device, integration=1.0 if v > 0.1 else 2.0)
        red_xyz.append(xyz if xyz is not None else np.array([0.0, 0.0, 0.0]))

    # --- Green ramp ---
    print("  Measuring green ramp...")
    green_xyz = []
    for v in levels:
        display.show(0, v, 0)
        xyz = measure_xyz(device, integration=1.0 if v > 0.1 else 2.0)
        green_xyz.append(xyz if xyz is not None else np.array([0.0, 0.0, 0.0]))

    # --- Blue ramp ---
    print("  Measuring blue ramp...")
    blue_xyz = []
    for v in levels:
        display.show(0, 0, v)
        xyz = measure_xyz(device, integration=1.0 if v > 0.1 else 2.0)
        blue_xyz.append(xyz if xyz is not None else np.array([0.0, 0.0, 0.0]))

    # Convert to arrays
    white_xyz = np.array(white_xyz)
    red_xyz = np.array(red_xyz)
    green_xyz = np.array(green_xyz)
    blue_xyz = np.array(blue_xyz)

    # Extract per-channel TRC: map signal level -> normalized luminance contribution
    # Subtract black from all measurements
    black_xyz = white_xyz[0]
    red_xyz_clean = red_xyz - black_xyz
    green_xyz_clean = green_xyz - black_xyz
    blue_xyz_clean = blue_xyz - black_xyz
    white_xyz_clean = white_xyz - black_xyz

    # TRC = how each channel's Y (luminance) responds to signal level
    # Normalize so max = 1.0
    trc_r_Y = np.maximum(red_xyz_clean[:, 1], 0)
    trc_g_Y = np.maximum(green_xyz_clean[:, 1], 0)
    trc_b_Y = np.maximum(blue_xyz_clean[:, 1], 0)

    # Use full XYZ of primaries at 100% for the matrix
    R_xyz = red_xyz_clean[-1]
    G_xyz = green_xyz_clean[-1]
    B_xyz = blue_xyz_clean[-1]

    # Build measured primaries matrix (columns = R, G, B primary XYZ)
    M_display = np.column_stack([R_xyz, G_xyz, B_xyz])

    # Normalize TRC by each primary's max Y
    max_r = trc_r_Y[-1] if trc_r_Y[-1] > 0 else 1.0
    max_g = trc_g_Y[-1] if trc_g_Y[-1] > 0 else 1.0
    max_b = trc_b_Y[-1] if trc_b_Y[-1] > 0 else 1.0

    trc_r = trc_r_Y / max_r
    trc_g = trc_g_Y / max_g
    trc_b = trc_b_Y / max_b

    # Force endpoints
    trc_r[0] = 0.0; trc_r[-1] = 1.0
    trc_g[0] = 0.0; trc_g[-1] = 1.0
    trc_b[0] = 0.0; trc_b[-1] = 1.0

    # Print measured primaries chromaticity
    def _xy(xyz):
        s = np.sum(xyz)
        return (xyz[0]/s, xyz[1]/s) if s > 0 else (0, 0)

    rx, ry = _xy(R_xyz)
    gx, gy = _xy(G_xyz)
    bx, by = _xy(B_xyz)
    wx, wy = _xy(white_xyz_clean[-1])
    print(f"    Red   primary: ({rx:.4f}, {ry:.4f})")
    print(f"    Green primary: ({gx:.4f}, {gy:.4f})")
    print(f"    Blue  primary: ({bx:.4f}, {by:.4f})")
    print(f"    White point:   ({wx:.4f}, {wy:.4f})")

    # Fit gamma for info
    for ch_name, trc in [("Red", trc_r), ("Green", trc_g), ("Blue", trc_b)]:
        mid = trc[n_steps // 2]
        if mid > 0:
            gamma_est = np.log(mid) / np.log(0.5) if mid > 0 and mid < 1 else 2.2
            print(f"    {ch_name:5s} gamma ~ {gamma_est:.2f}")

    return white_Y, black_xyz, levels, trc_r, trc_g, trc_b, M_display


def build_correction_lut(levels, trc_r, trc_g, trc_b, M_display, black_xyz, white_Y, size=33):
    """
    Build a 3D correction LUT from measured display profile.

    The display model is:
        XYZ_display = M_display @ [TRC_r(r), TRC_g(g), TRC_b(b)] + black

    The correction finds, for each target sRGB color:
        What signal (r', g', b') should we send so the display produces
        the same XYZ as a perfect sRGB display would?

    This is the inverse display model.
    """
    print("\n  Building correction LUT...")

    # Build interpolation functions for inverse TRC
    # Forward TRC: signal -> linear (measured)
    # Inverse TRC: linear -> signal (what signal to send for desired linear output)
    inv_trc_r = interp1d(trc_r, levels, kind='linear', bounds_error=False,
                         fill_value=(0.0, 1.0))
    inv_trc_g = interp1d(trc_g, levels, kind='linear', bounds_error=False,
                         fill_value=(0.0, 1.0))
    inv_trc_b = interp1d(trc_b, levels, kind='linear', bounds_error=False,
                         fill_value=(0.0, 1.0))

    # Invert the display primaries matrix
    # M_display maps linear [r,g,b] -> XYZ (minus black)
    # inv_M maps XYZ (minus black) -> linear [r,g,b]
    try:
        inv_M = np.linalg.inv(M_display)
    except np.linalg.LinAlgError:
        print("  WARNING: Display matrix is singular, using pseudo-inverse")
        inv_M = np.linalg.pinv(M_display)

    # sRGB target: what XYZ should each sRGB value produce?
    # XYZ_target = SRGB_TO_XYZ @ srgb_expand(rgb)
    # But we need to normalize to match the display's luminance level
    norm = white_Y  # Display white luminance

    # The target white XYZ (from sRGB spec): SRGB_TO_XYZ @ [1,1,1] = D65 white
    srgb_white_xyz = SRGB_TO_XYZ @ np.array([1.0, 1.0, 1.0])

    # The display white XYZ (measured): M_display @ [1,1,1]
    display_white_xyz = M_display @ np.array([1.0, 1.0, 1.0])

    # We want the LUT to map sRGB values such that:
    # Display produces the correct relative XYZ for each color
    # Use Bradford chromatic adaptation from D65 -> display white point
    from calibrate_pro.core.color_math import BRADFORD_MATRIX, BRADFORD_INVERSE
    wp_source = srgb_white_xyz / srgb_white_xyz[1]  # D65 (normalized)
    wp_dest = display_white_xyz / display_white_xyz[1]  # Display white (normalized)

    source_cone = BRADFORD_MATRIX @ wp_source
    dest_cone = BRADFORD_MATRIX @ wp_dest
    scale = np.diag(dest_cone / source_cone)
    adapt_matrix = BRADFORD_INVERSE @ scale @ BRADFORD_MATRIX

    # Generate LUT
    lut = LUT3D.create_identity(size)
    coords = np.linspace(0, 1, size)

    # Vectorized computation
    r_grid, g_grid, b_grid = np.meshgrid(coords, coords, coords, indexing="ij")
    all_srgb = np.stack([r_grid.ravel(), g_grid.ravel(), b_grid.ravel()], axis=1)

    # Step 1: sRGB decode -> linear
    linear = srgb_gamma_expand(all_srgb)

    # Step 2: Linear sRGB -> target XYZ
    target_xyz = (SRGB_TO_XYZ @ linear.T).T

    # Step 3: Adapt from sRGB white point (D65) to display white point
    target_xyz_adapted = (adapt_matrix @ target_xyz.T).T

    # Step 4: XYZ -> display linear RGB using inverse display matrix
    display_linear = (inv_M @ target_xyz_adapted.T).T

    # Step 5: Clip to display gamut (can't produce negative or >1 linear)
    display_linear = np.clip(display_linear, 0.0, 1.0)

    # Step 6: Apply inverse TRC per channel (linear -> signal value)
    corrected_r = inv_trc_r(display_linear[:, 0])
    corrected_g = inv_trc_g(display_linear[:, 1])
    corrected_b = inv_trc_b(display_linear[:, 2])

    # Assemble
    rgb_output = np.stack([corrected_r, corrected_g, corrected_b], axis=1)
    rgb_output = np.clip(rgb_output, 0.0, 1.0)

    # Preserve true black for OLED
    is_black = np.all(all_srgb < 1e-6, axis=1)
    rgb_output[is_black] = 0.0

    lut.data = rgb_output.reshape(size, size, size, 3).astype(np.float32)
    lut.title = "Calibrate Pro - PG27UCDM (Native Measured Correction)"

    # Report LUT deviation from identity
    identity = all_srgb.reshape(size, size, size, 3)
    max_deviation = np.max(np.abs(lut.data - identity))
    avg_deviation = np.mean(np.abs(lut.data - identity))
    print(f"    Max deviation from identity: {max_deviation:.4f}")
    print(f"    Avg deviation from identity: {avg_deviation:.4f}")

    return lut


def measure_colorchecker(device, display, white_Y):
    """Measure all 24 ColorChecker patches and return dE values."""
    norm_factor = 100.0 / white_Y if white_Y > 0 else 1.0
    results = []

    for i, (name, r, g, b) in enumerate(COLORCHECKER):
        display.show(r, g, b)
        freq = measure_freq(device, integration=1.0)

        if freq is not None and np.max(freq) > 0.5:
            xyz_raw = freq_to_xyz(freq)
            xyz_norm = xyz_raw * norm_factor / 100.0  # Y=1 scale

            lab_meas = xyz_to_lab(bradford_adapt(xyz_norm, D65_WHITE, D50_WHITE), D50_WHITE)
            lab_ref = np.array(REF_LAB[name])
            de = float(delta_e_2000(lab_meas, lab_ref))
            results.append((name, de, xyz_raw))
        else:
            results.append((name, -1, np.array([0, 0, 0])))

    return results


def print_comparison(before, after):
    """Print before/after comparison table."""
    print()
    print(f"  {'Patch':20s}  {'Before':>6s}  {'After':>6s}  {'Change':>7s}  Status")
    print("  " + "=" * 60)

    for i in range(len(COLORCHECKER)):
        name = COLORCHECKER[i][0]
        b_de = before[i][1]
        a_de = after[i][1]

        if b_de < 0 or a_de < 0:
            print(f"  {name:20s}  {'n/a':>6s}  {'n/a':>6s}  {'---':>7s}")
            continue

        change = a_de - b_de
        status = "PASS" if a_de < 2.0 else "WARN" if a_de < 3.0 else "FAIL"
        arrow = "v" if change < -0.5 else "^" if change > 0.5 else "~"
        print(f"  {name:20s}  {b_de:5.2f}   {a_de:5.2f}   {change:+5.2f} {arrow}  [{status}]")

    valid_b = [r[1] for r in before if r[1] >= 0]
    valid_a = [r[1] for r in after if r[1] >= 0]

    print("  " + "=" * 60)
    if valid_b and valid_a:
        avg_b, avg_a = np.mean(valid_b), np.mean(valid_a)
        max_b, max_a = np.max(valid_b), np.max(valid_a)
        pct = (1 - avg_a / avg_b) * 100 if avg_b > 0 else 0
        print(f"  Before:  avg dE = {avg_b:.2f},  max dE = {max_b:.2f}")
        print(f"  After:   avg dE = {avg_a:.2f},  max dE = {max_a:.2f}")
        print(f"  Change:  {avg_b - avg_a:+.2f} dE ({pct:+.0f}% {'reduction' if pct > 0 else 'increase'})")

        if avg_a < avg_b:
            print(f"\n  CALIBRATION IMPROVED BY {pct:.0f}%")
        else:
            print(f"\n  WARNING: Calibration did not improve. May need iterative refinement.")


# =============================================================================
# Main calibration loop
# =============================================================================

if __name__ == "__main__":
    print("=" * 70)
    print("  NATIVE DISPLAY CALIBRATION LOOP")
    print("  Sensor:  i1Display3 (NEC MDSVSENSOR3)")
    print("  Matrix:  Organic LED (from device EEPROM)")
    print("  Display: ASUS PG27UCDM (QD-OLED)")
    print("  Method:  Measured TRC + primaries -> 3D correction LUT")
    print("  No ArgyllCMS required.")
    print("=" * 70)

    # --- Open and unlock sensor ---
    print("\nStep 1: Connecting to colorimeter...")
    device = hid.device()
    device.open(0x0765, 0x5020)
    unlock_device(device)
    print("  Sensor unlocked and ready.")

    # --- Find display ---
    from calibrate_pro.panels.detection import enumerate_displays
    displays = enumerate_displays()
    dx, dy, dw, dh = 0, 0, 3840, 2160
    for d in displays:
        if d.width == 3840:
            dx, dy, dw, dh = d.position_x, d.position_y, d.width, d.height
            break

    display = PatchDisplay(dx, dy, dw, dh)

    # --- Remove any existing LUT ---
    print("\nStep 2: Removing existing calibration LUT...")
    try:
        from calibrate_pro.lut_system.dwm_lut import remove_lut
        remove_lut(1)
        time.sleep(2)
        print("  Existing LUT removed.")
    except Exception as e:
        print(f"  No existing LUT to remove ({e})")

    # --- Measure baseline ColorChecker (uncalibrated) ---
    print("\nStep 3: Measuring uncalibrated baseline (24 ColorChecker patches)...")
    display.show(1.0, 1.0, 1.0)
    white_freq = measure_freq(device, 1.0)
    white_Y_baseline = freq_to_xyz(white_freq)[1]
    print(f"  White luminance: {white_Y_baseline:.1f} cd/m²")

    baseline = measure_colorchecker(device, display, white_Y_baseline)
    valid_baseline = [r[1] for r in baseline if r[1] >= 0]
    print(f"\n  Baseline: avg dE = {np.mean(valid_baseline):.2f}, "
          f"max dE = {np.max(valid_baseline):.2f}")

    for i, (name, de, _) in enumerate(baseline):
        status = "PASS" if de < 2.0 else "WARN" if de < 3.0 else "FAIL"
        if de >= 0:
            print(f"    [{i+1:2d}/24] {name:20s}  dE = {de:5.2f}  [{status}]")
        else:
            print(f"    [{i+1:2d}/24] {name:20s}  (no reading)")

    # --- Profile the display ---
    print("\nStep 4: Profiling display (per-channel TRC measurement)...")
    profile = profile_display(device, display, n_steps=17)
    white_Y, black_xyz, levels, trc_r, trc_g, trc_b, M_display = profile

    # --- Build correction LUT ---
    print("\nStep 5: Computing correction LUT from display profile...")
    lut = build_correction_lut(levels, trc_r, trc_g, trc_b, M_display,
                                black_xyz, white_Y, size=33)

    # Save LUT
    lut_dir = os.path.expanduser("~/Documents/Calibrate Pro/Calibrations")
    os.makedirs(lut_dir, exist_ok=True)
    lut_path = os.path.join(lut_dir, "PG27UCDM_native_calibration.cube")
    lut.save(lut_path)
    print(f"  Saved: {lut_path}")

    # --- Apply LUT via dwm_lut ---
    print("\nStep 6: Applying correction LUT...")
    try:
        from calibrate_pro.lut_system.dwm_lut import DwmLutController
        dwm = DwmLutController()
        if dwm.is_available:
            dwm.load_lut_file(1, lut_path)
            print("  LUT applied via DWM.")
            time.sleep(3)  # Let DWM process the LUT
        else:
            print("  WARNING: dwm_lut not available. LUT saved but not applied.")
            print(f"  Manually copy to C:\\Windows\\Temp\\luts\\")
    except Exception as e:
        print(f"  LUT application failed: {e}")
        print(f"  LUT saved at: {lut_path}")

    # --- Re-measure ColorChecker with correction applied ---
    print("\nStep 7: Re-measuring with correction applied (24 patches)...")
    # Re-measure white with LUT active
    display.show(1.0, 1.0, 1.0)
    white_freq_cal = measure_freq(device, 1.0)
    white_Y_cal = freq_to_xyz(white_freq_cal)[1]
    print(f"  Calibrated white luminance: {white_Y_cal:.1f} cd/m²")

    calibrated = measure_colorchecker(device, display, white_Y_cal)

    # --- Results ---
    print("\n" + "=" * 70)
    print("  CALIBRATION RESULTS")
    print("=" * 70)
    print_comparison(baseline, calibrated)

    # Cleanup
    display.destroy()
    device.close()

    print("\n" + "=" * 70)
    print("  All measurements NATIVE (i1Display3 USB HID, no ArgyllCMS)")
    print("  Correction LUT: " + lut_path)
    print("=" * 70)
