"""Reference driver: run the ORIGINAL build_color modules and print the same
layout as the BuildLang `color` program.

Usage: python -I tests/reference.py PATH/TO/build_color (de|convert) FILE

difference.py and spaces.py are loaded by file path (both import only
numpy), so the package's other modules and its __init__ are not needed.

Number format: BuildLang `{}` prints an f64 with the shortest digits that
round-trip (the same digits as Python repr), but written positionally with
no exponent and without a trailing ".0": 100.0 -> "100", 1e-05 ->
"0.00001", -0.0 -> "-0", 1e+16 -> "10000000000000000". fmt() below takes
repr's digits and lays them out that way, so the goldens are byte-identical
wherever the two programs compute the same double.
"""

import importlib.util
import os
import sys
from decimal import Decimal

import numpy as np


def load(pkg_dir, name):
    path = os.path.join(pkg_dir, name + ".py")
    spec = importlib.util.spec_from_file_location("bc_" + name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def fmt(x):
    x = float(x)
    if x != x or x in (float("inf"), float("-inf")):
        return repr(x)
    return format(Decimal(repr(x)).normalize(), "f")


def rows(path, want):
    out = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            w = line.split()
            if len(w) >= want and not w[0].startswith("#"):
                out.append([float(t) for t in w[:want]])
    return out


def cmd_de(dif, path):
    for r in rows(path, 6):
        p, q = np.array(r[:3]), np.array(r[3:])
        vals = [
            dif.delta_e_76(p, q),
            dif.delta_e_94(p, q, application="graphic_arts"),
            dif.delta_e_94(p, q, application="textiles"),
            dif.delta_e_2000(p, q),
            dif.delta_e_cmc(p, q, l=2.0, c=1.0),
            dif.delta_e_cmc(p, q, l=1.0, c=1.0),
            dif.delta_e_hyab(p, q),
        ]
        print(" ".join(fmt(v) for v in vals))


def show(label, v):
    print(label, " ".join(fmt(t) for t in v))


def cmd_convert(dif, sp, path):
    white_ok = sp.srgb_to_oklab(np.array([1.0, 1.0, 1.0]))
    for r in rows(path, 3):
        s = np.array(r)
        lin = sp.srgb_to_linear(s)
        xyz = sp.srgb_to_xyz(s)
        xyy = sp.xyz_to_xyY(xyz)
        lab = sp.xyz_to_lab(xyz)
        lch = sp.lab_to_lch(lab)
        ok = sp.srgb_to_oklab(s)
        okch = sp.oklab_to_oklch(ok)
        show("lin", lin)
        show("enc", sp.linear_to_srgb(lin))
        show("xyz", xyz)
        show("rgb", sp.xyz_to_srgb(xyz, clip=True))
        show("rgbu", sp.xyz_to_srgb(xyz, clip=False))
        show("xyy", xyy)
        show("xyyxyz", sp.xyY_to_xyz(xyy))
        show("lab", lab)
        show("labxyz", sp.lab_to_xyz(lab))
        show("lch", lch)
        show("lchlab", sp.lch_to_lab(lch))
        show("oklab", ok)
        show("oklin", sp.oklab_to_linear_srgb(ok))
        show("okrgb", sp.oklab_to_srgb(ok, clip=True))
        show("oklch", okch)
        show("oklchlab", sp.oklch_to_oklab(okch))
        lum = float(sp.luminance(s))
        print("lum", fmt(lum), fmt(dif.contrast_ratio(lum, 1.0)), fmt(dif.delta_e_oklab(ok, white_ok)))


def main():
    sys.stdout.reconfigure(newline="\n")
    # np.where evaluates both branches; out-of-range inputs warn on the
    # discarded branch only. Silencing that changes no value.
    np.seterr(all="ignore")
    pkg, cmd, path = sys.argv[1], sys.argv[2], sys.argv[3]
    dif = load(pkg, "difference")
    if cmd == "de":
        cmd_de(dif, path)
    elif cmd == "convert":
        cmd_convert(dif, load(pkg, "spaces"), path)
    else:
        sys.exit("usage: reference.py PATH/TO/build_color (de|convert) FILE")


if __name__ == "__main__":
    main()
