"""Exact reference for BuildLang's correctly rounded f64 cube root.

Writes tests/detmath/cbrt.bld (the inputs, as BuildLang) and
tests/detmath/cbrt.expected (the double nearest the true cube root of each
input), or checks a file of "input output" hex pairs against the reference.

The reference is integer arithmetic: for x = M * 2^E, it finds the integer
Y in [2^52, 2^53] nearest to cbrt(x) / 2^k and returns Y * 2^k. A second,
separate reference (mpmath at 300 bits, rounded to a double) is checked
against the first on every generated input when mpmath is installed.

    python tools/detmath/cbrt_vectors.py            # rewrite the two files
    python tools/detmath/cbrt_vectors.py --verify pairs.txt
"""

import math
import random
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHUNK = 400


def to_bits(v):
    return struct.unpack("<Q", struct.pack("<d", v))[0]


def from_bits(b):
    return struct.unpack("<d", struct.pack("<Q", b))[0]


def icbrt(n):
    """floor(cbrt(n)) for an integer n >= 0."""
    if n == 0:
        return 0
    y = 1 << ((n.bit_length() + 2) // 3)  # above the root
    while True:
        z = (2 * y + n // (y * y)) // 3
        if z >= y:
            break
        y = z
    while y * y * y > n:
        y -= 1
    while (y + 1) ** 3 <= n:
        y += 1
    return y


def cbrt_ref(bits):
    """The bits of the double nearest cbrt(x); NaN in gives None."""
    sign = bits >> 63
    mag = bits & ((1 << 63) - 1)
    if mag > 0x7FF0000000000000:
        return None
    if mag in (0, 0x7FF0000000000000):
        return bits
    ex = mag >> 52
    frac = mag & ((1 << 52) - 1)
    if ex == 0:
        m, e = frac, -1074
    else:
        m, e = frac | (1 << 52), ex - 1075
    # x = m * 2^e; choose k so that cbrt(x) / 2^k is in [2^52, 2^53)
    j = (e + m.bit_length() - 1) // 3
    k = j - 52
    shift = e - 3 * k
    assert shift >= 0
    n = m << shift
    y = icbrt(n)
    # the midpoint (y + 1/2) is never an exact cube root: compare 8n with (2y+1)^3
    if (2 * y + 1) ** 3 < 8 * n:
        y += 1
    out = to_bits(math.ldexp(y, k))
    return out | (sign << 63)


def cbrt_mpmath(bits):
    import mpmath

    x = from_bits(bits)
    if x != x:
        return None
    if x == 0 or math.isinf(x):
        return bits
    with mpmath.workprec(300):
        r = mpmath.cbrt(abs(mpmath.mpf(x)))
        v = float(r)
    return to_bits(math.copysign(v, x))


def inputs():
    rng = random.Random(20261116)
    out = []

    def add(b):
        out.append(b & 0xFFFFFFFFFFFFFFFF)

    # special values and range ends
    for b in [
        0,
        1 << 63,
        0x7FF0000000000000,
        0xFFF0000000000000,
        0x7FF8000000000000,
        0xFFF8000000000000,
        0x7FF0000000000001,
        1,
        (1 << 63) | 1,
        0x000FFFFFFFFFFFFF,
        0x0010000000000000,
        0x7FEFFFFFFFFFFFFF,
        0xFFEFFFFFFFFFFFFF,
        to_bits(1.0),
        to_bits(-1.0),
        to_bits(2.0),
        to_bits(8.0),
        to_bits(-8.0),
        to_bits(27.0),
        to_bits(0.001),
        to_bits(1e-300),
        to_bits(1e300),
        to_bits(3.5),
        to_bits(math.nextafter(8.0, 0.0)),
        to_bits(math.nextafter(1.0, 2.0)),
        to_bits(math.nextafter(1.0, 0.0)),
    ]:
        add(b)
    # every power of two, normal and subnormal, both signs for one in three
    for p in range(-1074, 1024):
        b = to_bits(math.ldexp(1.0, p))
        add(b)
        if p % 3 == 0:
            add(b | (1 << 63))
    # perfect cubes: a^3 * 2^(3j) is exact when a has at most 17 bits
    for _ in range(400):
        a = rng.randrange(1, 1 << 17)
        j = rng.randrange(-110, 110)
        v = math.ldexp(float(a**3), 3 * j)
        if v != 0 and not math.isinf(v):
            add(to_bits(rng.choice((1.0, -1.0)) * v))
    # near halfway: x is the double nearest (y + u/2)^3 and its two neighbours
    for _ in range(700):
        y = rng.randrange(1 << 52, 1 << 53)
        k = rng.randrange(-410, 290)
        h3 = (2 * y + 1) ** 3  # (y + 1/2)^3 * 8, scaled by 2^(3k - 3)
        e = 3 * k - 3
        # nearest double to h3 * 2^e
        top = h3.bit_length() - 53
        q, r = divmod(h3, 1 << top)
        if 2 * r > (1 << top):
            q += 1
        x = math.ldexp(q, top + e)
        if x == 0 or math.isinf(x):
            continue
        sgn = rng.choice((0, 1 << 63))
        for b in (to_bits(x) - 1, to_bits(x), to_bits(x) + 1):
            add(b | sgn)
    # random bit patterns over the whole range (NaNs included where they fall)
    for _ in range(4000):
        add(rng.getrandbits(64))
    # random subnormals
    for _ in range(300):
        add(rng.randrange(1, 1 << 52) | rng.choice((0, 1 << 63)))
    return out


def fmt(b):
    return "nan" if b is None else f"{b:016x}"


def write():
    xs = inputs()
    try:
        import mpmath  # noqa: F401

        have_mp = True
    except ImportError:
        have_mp = False
        print("mpmath missing: the second reference was not checked", file=sys.stderr)
    expected = []
    for i, b in enumerate(xs):
        r = cbrt_ref(b)
        if have_mp:
            assert r == cbrt_mpmath(b), f"references disagree at {b:016x}"
        expected.append(f"{i} {fmt(r)}")
    lines = [
        "// Generated by tools/detmath/cbrt_vectors.py; do not edit.",
        "// Each row prints the bits of cbrt(x), and a line if x.cbrt() differs from it.",
        "// cbrt.expected holds the double nearest the true cube root (exact integers).",
        "fn row(i: i64, hi: i64, lo: i64) ~ Console {",
        "    let x = f64::from_bits(((hi as u64) << 32) | (lo as u64));",
        "    let a = cbrt(x);",
        "    let b = x.cbrt();",
        "    if a != a {",
        '        println!("{} nan", i);',
        "    } else {",
        '        println!("{} {:016x}", i, a.to_bits());',
        "    }",
        "    if a.to_bits() != b.to_bits() {",
        '        println!("{} method differs", i);',
        "    }",
        "}",
        "",
    ]
    parts = 0
    for start in range(0, len(xs), CHUNK):
        lines.append(f"fn part{parts}() ~ Console {{")
        for i in range(start, min(start + CHUNK, len(xs))):
            b = xs[i]
            lines.append(f"    row({i}, {b >> 32}, {b & 0xFFFFFFFF});")
        lines.append("}")
        lines.append("")
        parts += 1
    lines.append("fn main() ~ Console {")
    for p in range(parts):
        lines.append(f"    part{p}();")
    lines.append("}")
    (ROOT / "tests/detmath/cbrt.bld").write_text("\n".join(lines) + "\n", newline="\n")
    (ROOT / "tests/detmath/cbrt.expected").write_text("\n".join(expected) + "\n", newline="\n")
    print(f"{len(xs)} inputs written", file=sys.stderr)


def verify(path):
    bad = n = 0
    for line in open(path):
        parts = line.split()
        if not parts:
            continue
        x, got = int(parts[0], 16), int(parts[1], 16)
        want = cbrt_ref(x)
        n += 1
        ok = (want is None and from_bits(got) != from_bits(got)) or want == got
        if not ok:
            bad += 1
            if bad <= 10:
                print(f"x {x:016x} got {got:016x} want {fmt(want)}")
    print(f"{n - bad} of {n} correctly rounded")
    return bad == 0


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--verify":
        sys.exit(0 if verify(sys.argv[2]) else 1)
    write()
