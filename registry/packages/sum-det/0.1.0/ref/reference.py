"""Reference for sum-det: inputs and goldens, written from Python floats.

Usage (from the package folder):
    python ref/reference.py write    rewrite every input and golden file
    python ref/reference.py check    exit 1 if any file differs from a fresh write

Python floats are IEEE 754 binary64 and `a + b` is one correctly rounded
addition, so a loop in the stated order gives the contract's bits. The exact
sum has two references that share no code: math.fsum for order `fsum`, and
a sum of fractions.Fraction rounded once for order `binned`.
"""
import math
import struct
import sys
from fractions import Fraction
from pathlib import Path

M64 = (1 << 64) - 1
ORDERS = ("serial", "tree", "i8", "i8ref", "fsum", "binned")


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def from_bits(b):
    return struct.unpack("<d", struct.pack("<Q", b))[0]


class Rng:
    """splitmix64, the same steps as src/d_gen.bld."""

    def __init__(self, seed):
        self.s = seed & M64

    def next(self):
        self.s = (self.s + 0x9E3779B97F4A7C15) & M64
        z = self.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M64
        return z ^ (z >> 31)


def make_float(u, lo, span):
    e = lo + ((u >> 52) & 2047) % span
    return from_bits((u & (1 << 63)) | (e << 52) | (u & ((1 << 52) - 1)))


def gen(seed, n, kind):
    r, out, big = Rng(seed), [], 0.0
    for i in range(n):
        u = r.next()
        if kind == 0:
            x = float(u >> 11) / 4503599627370496.0 - 1.0
        elif kind == 1:
            x = make_float(u, 963, 121)
        elif kind == 2:
            if i % 3 == 0:
                big = x = make_float(u, 1223, 400)
            elif i % 3 == 1:
                x = make_float(u, 900, 100)
            else:
                x = 0.0 - big
        elif kind == 3:
            x = make_float(u, 0, 2)
        elif kind == 4:
            x = make_float(u, 1, 1900)
        else:
            x = float((u >> 40) % 2001) - 1000.0
        out.append(x)
    return out


def serial(xs):
    s = 0.0
    for x in xs:
        s = s + x
    return s


def tree(xs):
    n = len(xs)
    if n == 0:
        return 0.0
    if n == 1:
        return xs[0]
    p = 1
    while p * 2 < n:
        p *= 2
    return tree(xs[:p]) + tree(xs[p:])


def i8(xs):
    lanes = [0.0] * 8
    for i, x in enumerate(xs):
        lanes[i % 8] = lanes[i % 8] + x
    a = lanes
    return ((a[0] + a[1]) + (a[2] + a[3])) + ((a[4] + a[5]) + (a[6] + a[7]))


def exact_fraction(xs):
    return float(sum((Fraction(x) for x in xs), Fraction(0)))


SUMS = {"serial": serial, "tree": tree, "i8": i8, "i8ref": i8,
        "fsum": math.fsum, "binned": exact_fraction}


def hand_open():
    tiny, half = 5e-324, 2.0 ** -53
    rows = [
        [], [-0.0], [-0.0, -0.0], [1.0], [1e100, 1.0, -1e100, 1e-100],
        [1.0, 1e100, 1.0, -1e100] * 25, [1e-16, 1.0, 1e16],
        [2.0 ** 53, -0.5, -(2.0 ** -54)], [2.0 ** 53, 1.0, 2.0 ** -100],
        [2.0 ** 53 + 10.0, 1.0, 2.0 ** -100], [2.0 ** 53 - 4.0, 0.5, 2.0 ** -54],
        [1.0, half], [1.0, half, tiny], [1.0, half, -tiny], [1.0 + 2.0 ** -52, half],
        [tiny] * 3, [tiny, -tiny, tiny], [2.2250738585072014e-308, -tiny],
        [tiny, 1.0, -1.0], [1.7976931348623157e308, -1.7976931348623157e308, 1e308],
        [1.0 / n for n in range(1, 1001)], [(-1.0) ** n / n for n in range(1, 1001)],
        [1.7 ** (i + 1) - 1.7 ** i for i in range(1000)] + [-(1.7 ** 1000)],
        [0.1] * 10, [1e307, 1e307, -1e307, -1e307, 3.0],
    ]
    return rows


def hand_holdout():
    tiny = 5e-324
    return [
        [3.0, tiny, -3.0], [2.0 ** 60, 1.0, -(2.0 ** 60), 2.0 ** -60] * 9,
        [0.3] * 17, [1.0 / 3.0, 1e17, -1e17] * 11, [tiny * 7, -tiny * 3] * 5,
        [1.0 / (n * n) for n in range(1, 500)], [1e200, -1e-200] * 13 + [-1e200] * 13,
    ]


def gen_rows(seed0, sizes):
    return [(kind, seed0 + 1000 * kind + n, n) for kind in range(6) for n in sizes]


def hand_text(rows):
    return "".join((" ".join("%016x" % bits(x) for x in row) or "-") + "\n" for row in rows)


def golden(order, vectors):
    return "".join("%016x\n" % bits(SUMS[order](list(v))) for v in vectors)


def plan(root):
    """Every generated file as (path, text)."""
    files = {}
    sets = (("tests", hand_open(), gen_rows(20261110, (1, 7, 8, 9, 64, 100, 1000)), ORDERS),
            ("holdout", hand_holdout(), gen_rows(77003311, (3, 15, 16, 17, 255, 4099, 100003)), ("i8",)))
    for folder, hand, rows, orders in sets:
        files[f"{folder}/hand.txt"] = hand_text(hand)
        files[f"{folder}/gen.txt"] = "".join("%d %d %d\n" % r for r in rows)
        vectors = [gen(seed, n, kind) for kind, seed, n in rows]
        cases = []
        for order in orders:
            files[f"{folder}/golden-hand-{order}.txt"] = golden(order, hand)
            files[f"{folder}/golden-gen-{order}.txt"] = golden(order, vectors)
            cases.append(f"exact {folder}/golden-hand-{order}.txt sum {order} {folder}/hand.txt\n")
            cases.append(f"exact {folder}/golden-gen-{order}.txt gens {order} {folder}/gen.txt\n")
        files[f"{folder}/cases.txt"] = "# mode golden args... (run from the package folder)\n" + "".join(cases)
    return {root / k: v for k, v in files.items()}


def main():
    root = Path(__file__).resolve().parent.parent
    mode = sys.argv[1] if len(sys.argv) > 1 else "check"
    bad = 0
    for path, text in plan(root).items():
        if mode == "write":
            path.write_bytes(text.encode("ascii"))
        elif not path.is_file() or path.read_bytes() != text.encode("ascii"):
            print("differs:", path.relative_to(root))
            bad += 1
    print("files", len(plan(root)), "differing", bad)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
