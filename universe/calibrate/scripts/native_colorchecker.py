"""
Native ColorChecker measurement using the i1Display3 EEPROM OLED matrix.
No ArgyllCMS required. Displays patches via tkinter, measures via USB HID.
"""
import hid, struct, time, sys, os
import numpy as np
import tkinter as tk

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from calibrate_pro.core.color_math import (
    xyz_to_lab, bradford_adapt, delta_e_2000, D50_WHITE, D65_WHITE,
    srgb_gamma_expand, SRGB_TO_XYZ
)

# OLED calibration matrix from device EEPROM at offset 0x191C
OLED_MATRIX = np.array([
    [0.03836831, -0.02175997, 0.01696057],
    [0.01449629,  0.01611903, 0.00057150],
    [-0.00004481, 0.00035042, 0.08032401],
])

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


def unlock_device(device):
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
    return matrix @ freq


if __name__ == "__main__":
    print("=" * 65)
    print("  NATIVE COLORCHECKER MEASUREMENT")
    print("  Sensor: i1Display3 (NEC MDSVSENSOR3)")
    print("  Matrix: Organic LED (from device EEPROM)")
    print("  No ArgyllCMS required.")
    print("=" * 65)

    # Open and unlock sensor
    device = hid.device()
    device.open(0x0765, 0x5020)
    unlock_device(device)
    print("Sensor unlocked.")

    # Find display position (use primary for now)
    from calibrate_pro.panels.detection import enumerate_displays
    displays = enumerate_displays()
    dx, dy, dw, dh = 0, 0, 3840, 2160
    for d in displays:
        if d.width == 3840:
            dx, dy, dw, dh = d.position_x, d.position_y, d.width, d.height
            break

    # Create fullscreen patch window
    root = tk.Tk()
    root.overrideredirect(True)
    root.attributes("-topmost", True)
    root.geometry(f"{dw}x{dh}+{dx}+{dy}")
    canvas = tk.Canvas(root, highlightthickness=0, cursor="none")
    canvas.pack(fill=tk.BOTH, expand=True)

    def show_color(r, g, b):
        color = f"#{int(r*255):02x}{int(g*255):02x}{int(b*255):02x}"
        canvas.config(bg=color)
        root.update()
        time.sleep(1.5)  # OLED settle time

    # Measure white first to get normalization factor
    print("\nMeasuring white reference...")
    show_color(1.0, 1.0, 1.0)
    white_freq = measure_freq(device, 1.0)
    white_xyz = freq_to_xyz(white_freq)
    Y_white = white_xyz[1]
    print(f"  White Y = {Y_white:.2f} cd/m2")
    print(f"  White xy = ({white_xyz[0]/sum(white_xyz):.4f}, {white_xyz[1]/sum(white_xyz):.4f})")

    # Normalization: scale so white Y = 100 (for Lab calculation)
    norm_factor = 100.0 / Y_white if Y_white > 0 else 1.0

    # Measure all patches
    print(f"\nMeasuring {len(PATCHES)} patches...\n")
    results = []

    for i, (name, r, g, b) in enumerate(PATCHES):
        show_color(r, g, b)
        freq = measure_freq(device, 1.0)

        if freq is not None and np.max(freq) > 0.5:
            xyz_raw = freq_to_xyz(freq)
            xyz_norm = xyz_raw * norm_factor / 100.0  # Normalize to Y=1 scale

            lab_meas = xyz_to_lab(bradford_adapt(xyz_norm, D65_WHITE, D50_WHITE), D50_WHITE)
            lab_ref = np.array(REF_LAB[name])
            de = delta_e_2000(lab_meas, lab_ref)

            status = "PASS" if de < 2.0 else "WARN" if de < 3.0 else "FAIL"
            print(f"  [{i+1:2d}/24] {name:20s}  dE={de:5.2f}  [{status}]  "
                  f"Y={xyz_raw[1]:.1f}")
            results.append((name, float(de)))
        else:
            print(f"  [{i+1:2d}/24] {name:20s}  (low light / no reading)")
            results.append((name, -1))

    root.destroy()
    device.close()

    # Summary
    valid = [de for _, de in results if de >= 0]
    print()
    print("=" * 65)
    if valid:
        avg = np.mean(valid)
        mx = np.max(valid)
        passing = sum(1 for de in valid if de < 3.0)
        print(f"  Average Delta E: {avg:.2f}")
        print(f"  Maximum Delta E: {mx:.2f}")
        print(f"  Patches passing: {passing}/{len(valid)}")
        print(f"  ALL MEASUREMENTS ARE NATIVE (no ArgyllCMS)")
    else:
        print("  No valid measurements")
    print("=" * 65)
