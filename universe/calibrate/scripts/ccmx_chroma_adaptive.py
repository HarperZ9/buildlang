"""
CCMX + Chroma-Adaptive Calibration (Final Pipeline)

Combines the two complementary corrections:
1. CCMX: Fixes raw sensor XYZ (white point, chromaticity accuracy)
2. Chroma-adaptive: Full gamut correction for saturated colors,
   identity for neutrals (which CCMX already made accurate)

Expected result: dE < 3.0 average across all 24 patches.
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

OLED_MATRIX = np.array([
    [0.03836831, -0.02175997, 0.01696057],
    [0.01449629,  0.01611903, 0.00057150],
    [-0.00004481, 0.00035042, 0.08032401],
])

COLORCHECKER = [
    ("Dark Skin",    0.453, 0.317, 0.264), ("Light Skin",   0.779, 0.577, 0.505),
    ("Blue Sky",     0.355, 0.480, 0.611), ("Foliage",      0.352, 0.422, 0.253),
    ("Blue Flower",  0.508, 0.502, 0.691), ("Bluish Green", 0.362, 0.745, 0.675),
    ("Orange",       0.879, 0.485, 0.183), ("Purplish Blue",0.266, 0.358, 0.667),
    ("Moderate Red", 0.778, 0.321, 0.381), ("Purple",       0.367, 0.227, 0.414),
    ("Yellow Green", 0.623, 0.741, 0.246), ("Orange Yellow",0.904, 0.634, 0.154),
    ("Blue",         0.139, 0.248, 0.577), ("Green",        0.262, 0.584, 0.291),
    ("Red",          0.752, 0.197, 0.178), ("Yellow",       0.938, 0.857, 0.159),
    ("Magenta",      0.752, 0.313, 0.577), ("Cyan",         0.121, 0.544, 0.659),
    ("White",        0.961, 0.961, 0.961), ("Neutral 8",    0.784, 0.784, 0.784),
    ("Neutral 6.5",  0.584, 0.584, 0.584), ("Neutral 5",    0.420, 0.420, 0.420),
    ("Neutral 3.5",  0.258, 0.258, 0.258), ("Black",        0.085, 0.085, 0.085),
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
    def xy_to_XYZ(x, y, Y=1.0):
        if y == 0: return np.array([0,0,0])
        return np.array([(Y/y)*x, Y, (Y/y)*(1-x-y)])
    def build_matrix(r, g, b, w):
        R, G, B = xy_to_XYZ(*r), xy_to_XYZ(*g), xy_to_XYZ(*b)
        W = xy_to_XYZ(*w)
        M = np.column_stack([R, G, B])
        S = np.linalg.solve(M, W)
        return M * S[np.newaxis, :]
    Ms = build_matrix((0.6835,0.3060),(0.2622,0.7006),(0.1481,0.0575),(0.3134,0.3240))
    Mt = build_matrix((0.6835,0.3164),(0.2373,0.7080),(0.1396,0.0527),(0.3134,0.3291))
    return Mt @ np.linalg.inv(Ms)

def unlock_device(device):
    k0, k1 = 0xa9119479, 0x5b168761
    cmd = bytearray(65); cmd[0] = 0; cmd[1] = 0x99
    device.write(cmd); time.sleep(0.2)
    c = bytes(device.read(64, timeout_ms=3000))
    sc = bytearray(8)
    for i in range(8): sc[i] = c[3] ^ c[35+i]
    ci0=(sc[3]<<24)+(sc[0]<<16)+(sc[4]<<8)+sc[6]; ci1=(sc[1]<<24)+(sc[7]<<16)+(sc[2]<<8)+sc[5]
    nk0,nk1=(-k0)&M_MASK,(-k1)&M_MASK
    co=[(nk0-ci1)&M_MASK,(nk1-ci0)&M_MASK,(ci1*nk0)&M_MASK,(ci0*nk1)&M_MASK]
    s=sum(sc)
    for sh in [0,8,16,24]: s+=(nk0>>sh)&0xFF; s+=(nk1>>sh)&0xFF
    s0,s1=s&0xFF,(s>>8)&0xFF
    sr=bytearray(16)
    sr[0]=(((co[0]>>16)&0xFF)+s0)&0xFF;sr[1]=(((co[2]>>8)&0xFF)-s1)&0xFF
    sr[2]=((co[3]&0xFF)+s1)&0xFF;sr[3]=(((co[1]>>16)&0xFF)+s0)&0xFF
    sr[4]=(((co[2]>>16)&0xFF)-s1)&0xFF;sr[5]=(((co[3]>>16)&0xFF)-s0)&0xFF
    sr[6]=(((co[1]>>24)&0xFF)-s0)&0xFF;sr[7]=((co[0]&0xFF)-s1)&0xFF
    sr[8]=(((co[3]>>8)&0xFF)+s0)&0xFF;sr[9]=(((co[2]>>24)&0xFF)-s1)&0xFF
    sr[10]=(((co[0]>>8)&0xFF)+s0)&0xFF;sr[11]=(((co[1]>>8)&0xFF)-s1)&0xFF
    sr[12]=((co[1]&0xFF)+s1)&0xFF;sr[13]=(((co[3]>>24)&0xFF)+s1)&0xFF
    sr[14]=((co[2]&0xFF)+s0)&0xFF;sr[15]=(((co[0]>>24)&0xFF)-s0)&0xFF
    rb=bytearray(65);rb[0]=0;rb[1]=0x9A
    for i in range(16): rb[25+i]=c[2]^sr[i]
    device.write(rb);time.sleep(0.3);device.read(64,timeout_ms=3000)

def measure_freq(device, integration=1.0):
    intclks=int(integration*12000000)
    cmd=bytearray(65);cmd[0]=0x00;cmd[1]=0x01
    struct.pack_into('<I',cmd,2,intclks)
    device.write(cmd)
    resp=device.read(64,timeout_ms=int((integration+3)*1000))
    if resp and resp[0]==0x00 and resp[1]==0x01:
        r=struct.unpack('<I',bytes(resp[2:6]))[0]
        g=struct.unpack('<I',bytes(resp[6:10]))[0]
        b=struct.unpack('<I',bytes(resp[10:14]))[0]
        t=intclks/12000000.0
        return np.array([0.5*(r+0.5)/t,0.5*(g+0.5)/t,0.5*(b+0.5)/t])
    return None

if __name__ == "__main__":
    print("=" * 70)
    print("  CCMX + CHROMA-ADAPTIVE CALIBRATION (Final Pipeline)")
    print("=" * 70)

    CCMX = compute_ccmx()
    SENSOR = CCMX @ OLED_MATRIX  # Combined: raw freq -> corrected XYZ

    device = hid.device()
    device.open(0x0765, 0x5020)
    unlock_device(device)
    print("  Sensor unlocked. CCMX applied.\n")

    from calibrate_pro.panels.detection import enumerate_displays
    displays = enumerate_displays()
    dx,dy,dw,dh = 0,0,3840,2160
    for d in displays:
        if d.width == 3840:
            dx,dy,dw,dh = d.position_x,d.position_y,d.width,d.height; break

    root = tk.Tk(); root.overrideredirect(True); root.attributes("-topmost", True)
    root.geometry(f"{dw}x{dh}+{dx}+{dy}")
    canvas = tk.Canvas(root, highlightthickness=0, cursor="none")
    canvas.pack(fill=tk.BOTH, expand=True)

    def show(r,g,b,s=1.2):
        canvas.config(bg=f"#{max(0,min(255,int(r*255+0.5))):02x}{max(0,min(255,int(g*255+0.5))):02x}{max(0,min(255,int(b*255+0.5))):02x}")
        root.update(); time.sleep(s)

    def mxyz(integ=1.0):
        f=measure_freq(device,integ)
        return SENSOR@f if f is not None and np.max(f)>0.3 else None

    # Remove existing LUTs
    try:
        from calibrate_pro.lut_system.dwm_lut import remove_lut
        remove_lut(0); time.sleep(1)
    except: pass

    # Baseline
    show(1,1,1); wY = mxyz(1.0)[1]; norm = 100.0/wY
    print(f"  White Y = {wY:.1f} cd/m2, WP = D65 (CCMX-corrected)")
    print(f"  Measuring baseline...\n")

    base = []
    for name,r,g,b in COLORCHECKER:
        show(r,g,b); xyz = mxyz(1.0)
        if xyz is not None:
            lab = xyz_to_lab(bradford_adapt(xyz*norm/100, D65_WHITE, D50_WHITE), D50_WHITE)
            base.append(float(delta_e_2000(lab, np.array(REF_LAB[name]))))
        else: base.append(-1)
    vb = [d for d in base if d >= 0]
    print(f"  Baseline: avg dE = {np.mean(vb):.2f}, max = {np.max(vb):.2f}\n")

    # Profile (CCMX-corrected)
    print("  Profiling (17 steps x 4 channels)...")
    n=17; levels=np.linspace(0,1,n)
    def ramp(mk):
        out=[]
        for v in levels:
            r,g,b=mk(v); show(r,g,b,0.8 if v>0.1 else 1.5)
            xyz=mxyz(0.8 if v>0.1 else 1.5)
            out.append(xyz if xyz is not None else np.zeros(3))
        return np.array(out)
    wr=ramp(lambda v:(v,v,v)); rr=ramp(lambda v:(v,0,0))
    gr=ramp(lambda v:(0,v,0)); br=ramp(lambda v:(0,0,v))
    blk=wr[0].copy()
    for a in [wr,rr,gr,br]: a-=blk
    wr[0]=0;rr[0]=0;gr[0]=0;br[0]=0
    Md=np.column_stack([rr[-1],gr[-1],br[-1]])
    def ntrc(a,pY):
        t=np.maximum(a[:,1],0)
        if pY>0: t/=pY
        t[0]=0;t[-1]=1
        for i in range(1,len(t)): t[i]=max(t[i],t[i-1])
        return t
    tr=ntrc(rr,rr[-1][1]); tg=ntrc(gr,gr[-1][1]); tb=ntrc(br,br[-1][1])
    dw_xyz=Md@np.array([1,1,1]); Mn=Md/dw_xyz[1]; iM=np.linalg.inv(Mn)
    itr=interp1d(tr,levels,kind='linear',bounds_error=False,fill_value=(0,1))
    itg=interp1d(tg,levels,kind='linear',bounds_error=False,fill_value=(0,1))
    itb=interp1d(tb,levels,kind='linear',bounds_error=False,fill_value=(0,1))

    # Bradford
    sw=SRGB_TO_XYZ@np.array([1,1,1]); dwn=dw_xyz/dw_xyz[1]; swn=sw/sw[1]
    sc_c=BRADFORD_MATRIX@swn; dc=BRADFORD_MATRIX@dwn
    adapt=BRADFORD_INVERSE@np.diag(dc/sc_c)@BRADFORD_MATRIX

    def _xy(x):
        s=sum(x); return (x[0]/s,x[1]/s) if s>0 else (0,0)
    gR=np.log(max(tr[n//2],0.001))/np.log(0.5)
    gG=np.log(max(tg[n//2],0.001))/np.log(0.5)
    gB=np.log(max(tb[n//2],0.001))/np.log(0.5)
    print(f"  WP: ({_xy(wr[-1])[0]:.4f}, {_xy(wr[-1])[1]:.4f})")
    print(f"  Gamma: R={gR:.2f} G={gG:.2f} B={gB:.2f}")

    # Chroma-adaptive correction
    def correct(r,g,b):
        rgb_in=np.array([r,g,b])
        lin=srgb_gamma_expand(rgb_in)
        txyz=adapt@(SRGB_TO_XYZ@lin)
        dl=np.clip(iM@txyz,0,1)
        full=np.clip(np.array([float(itr(dl[0])),float(itg(dl[1])),float(itb(dl[2]))]),0,1)

        mx=max(r,g,b); mn=min(r,g,b)
        chroma=(mx-mn)/max(mx,1e-6)
        if chroma<=0.05: bl=0
        elif chroma>=0.3: bl=1
        else: t=(chroma-0.05)/0.25; bl=t*t*(3-2*t)

        result=rgb_in*(1-bl)+full*bl
        lum=0.2126*r+0.7152*g+0.0722*b
        if lum<0.03: result=rgb_in*(1-lum/0.03)+result*(lum/0.03)
        return np.clip(result,0,1)

    # Verify
    print("\n  Verifying with pre-corrected patches...\n")
    print(f"  {'Patch':20s}  {'Before':>6s}  {'After':>6s}  {'Change':>7s}  Status")
    print("  "+"="*62)

    cor=[]
    for i,(name,r,g,b) in enumerate(COLORCHECKER):
        cr,cg,cb=correct(r,g,b)
        show(cr,cg,cb)
        xyz=mxyz(1.0)
        if xyz is not None:
            lab=xyz_to_lab(bradford_adapt(xyz*norm/100,D65_WHITE,D50_WHITE),D50_WHITE)
            de=float(delta_e_2000(lab,np.array(REF_LAB[name])))
            cor.append(de)
            bd=base[i]
            if bd>=0:
                ch=de-bd; st="PASS" if de<2 else "WARN" if de<3 else "FAIL"
                ar="v" if ch<-0.5 else "^" if ch>0.5 else "~"
                print(f"  {name:20s}  {bd:5.2f}   {de:5.2f}   {ch:+5.2f} {ar}  [{st}]")
        else: cor.append(-1)

    root.destroy(); device.close()

    vc=[d for d in cor if d>=0]
    print("  "+"="*62)
    avg_b,avg_c=np.mean(vb),np.mean(vc)
    pct=(1-avg_c/avg_b)*100 if avg_b>0 else 0
    ps=sum(1 for d in vc if d<3)
    print(f"  Before:  avg dE = {avg_b:.2f},  max = {np.max(vb):.2f}")
    print(f"  After:   avg dE = {avg_c:.2f},  max = {np.max(vc):.2f}")
    print(f"  Passing: {ps}/{len(vc)} (<3.0)")
    print(f"\n  {'IMPROVEMENT' if avg_c<avg_b else 'No improvement'}: {pct:.0f}% ({avg_b-avg_c:.2f} dE)")
    print("="*70)
