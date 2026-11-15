"""Unlock i1Display3 and take a measurement."""
import hid, struct, time, sys

device = hid.device()
device.open(0x0765, 0x5020)
print(f"Device: {device.get_product_string()}")

M = 0xFFFFFFFF

KEY_TABLE = [
    (0xe9622e9f, 0x8d63e133, "retail i1Display3"),
    (0xa9119479, 0x5b168761, "NEC SpectraSensor"),
    (0xcaa62b2c, 0x30815b61, "OEM generic"),
    (0xe01e6e0a, 0x257462de, "ColorMunki Display"),
    (0x160eb6ae, 0x14440e70, "Quato Silver Haze"),
    (0x291e41d7, 0x51937bdd, "HP DreamColor"),
    (0x1abfae03, 0xf25ac8e8, "Wacom DC"),
    (0x828c43e9, 0xcbb8a8ed, "Toshiba TPA-1"),
    (0xe8d1a980, 0xd146f7ad, "Barco"),
    (0x171ae295, 0x2e5c7664, "PhotoCrysta"),
    (0x64d8c546, 0x4b24b4a7, "ViewSonic CS-XRi1"),
]

def unlock_attempt(k0, k1, name):
    # Request challenge
    cmd = bytearray(65)
    cmd[0] = 0x00
    cmd[1] = 0x99
    device.write(cmd)
    time.sleep(0.2)
    c = bytes(device.read(64, timeout_ms=3000) or [])
    if len(c) < 64:
        return False

    # Compute response
    sc = bytearray(8)
    for i in range(8):
        sc[i] = c[3] ^ c[35 + i]

    ci0 = (sc[3] << 24) + (sc[0] << 16) + (sc[4] << 8) + sc[6]
    ci1 = (sc[1] << 24) + (sc[7] << 16) + (sc[2] << 8) + sc[5]

    nk0 = (-k0) & M
    nk1 = (-k1) & M

    co = [(nk0 - ci1) & M, (nk1 - ci0) & M, (ci1 * nk0) & M, (ci0 * nk1) & M]

    s = sum(sc)
    for shift in [0, 8, 16, 24]:
        s += (nk0 >> shift) & 0xFF
        s += (nk1 >> shift) & 0xFF
    s0, s1 = s & 0xFF, (s >> 8) & 0xFF

    sr = bytearray(16)
    sr[0]  = (((co[0]>>16)&0xFF)+s0)&0xFF; sr[1]  = (((co[2]>>8)&0xFF)-s1)&0xFF
    sr[2]  = ((co[3]&0xFF)+s1)&0xFF;       sr[3]  = (((co[1]>>16)&0xFF)+s0)&0xFF
    sr[4]  = (((co[2]>>16)&0xFF)-s1)&0xFF; sr[5]  = (((co[3]>>16)&0xFF)-s0)&0xFF
    sr[6]  = (((co[1]>>24)&0xFF)-s0)&0xFF; sr[7]  = ((co[0]&0xFF)-s1)&0xFF
    sr[8]  = (((co[3]>>8)&0xFF)+s0)&0xFF;  sr[9]  = (((co[2]>>24)&0xFF)-s1)&0xFF
    sr[10] = (((co[0]>>8)&0xFF)+s0)&0xFF;  sr[11] = (((co[1]>>8)&0xFF)-s1)&0xFF
    sr[12] = ((co[1]&0xFF)+s1)&0xFF;       sr[13] = (((co[3]>>24)&0xFF)+s1)&0xFF
    sr[14] = ((co[2]&0xFF)+s0)&0xFF;       sr[15] = (((co[0]>>24)&0xFF)-s0)&0xFF

    resp_buf = bytearray(65)
    resp_buf[0] = 0x00
    resp_buf[1] = 0x9A
    for i in range(16):
        resp_buf[25 + i] = c[2] ^ sr[i]

    device.write(resp_buf)
    time.sleep(0.3)
    r = device.read(64, timeout_ms=3000)
    if r and len(r) > 2:
        result = r[2]
        print(f"  {name}: result=0x{result:02X}", end="")
        if result == 0x77:
            print(" -> UNLOCKED!")
            return True
        else:
            print(f" (rejected)")
    return False

print("\nTrying all unlock keys...")
unlocked = False
for k0, k1, name in KEY_TABLE:
    if unlock_attempt(k0, k1, name):
        unlocked = True
        break

if not unlocked:
    print("\nAll keys rejected. Device may need a different key.")
    device.close()
    sys.exit(1)

# Verify with measurement
print("\nTaking measurement (1 second)...")
intclks = int(1.0 * 12000000)
cmd = bytearray(65)
cmd[0] = 0x00
cmd[1] = 0x01
struct.pack_into("<I", cmd, 2, intclks)
device.write(cmd)
resp = device.read(64, timeout_ms=5000)
if resp:
    print(f"  Status: 0x{resp[0]:02X}")
    if resp[0] == 0x00:
        r = struct.unpack("<I", bytes(resp[2:6]))[0]
        g = struct.unpack("<I", bytes(resp[6:10]))[0]
        b = struct.unpack("<I", bytes(resp[10:14]))[0]
        print(f"  Counts: R={r} G={g} B={b}")
        if r > 0:
            rf = 0.5 * (r + 0.5) / 1.0
            gf = 0.5 * (g + 0.5) / 1.0
            bf = 0.5 * (b + 0.5) / 1.0
            print(f"  Freq: R={rf:.1f} G={gf:.1f} B={bf:.1f} Hz")
            print("  NATIVE MEASUREMENT WORKING!")
    else:
        msg = bytes(resp[2:20]).decode("ascii", errors="replace")
        print(f"  Error: {msg}")

device.close()
