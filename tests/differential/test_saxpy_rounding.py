# SPDX-FileCopyrightText: 2026 Zain Dana Harper
# SPDX-License-Identifier: AGPL-3.0-only
"""Seed of a differential test: saxpy in float32 has two rounding classes, and they differ.

    python tests/differential/test_saxpy_rounding.py

The reference uses exact rationals, so it depends on no float unit. fused rounds a x + y once;
unfused rounds a x, then the sum. A backend comparison will read `cases()` and compare each
backend's bytes with the class that backend declares; that step has not been built. See docs/FIELD.md.
"""
import random
import struct
import sys
from fractions import Fraction


def round32(q):
    """Round an exact rational to the nearest float32 value, ties to even, as a Fraction. Normal range only."""
    if q == 0:
        return Fraction(0)
    sign = -1 if q < 0 else 1
    q = abs(q)
    e = 0
    while q >= Fraction(2) ** (e + 24):
        e += 1
    while q < Fraction(2) ** (e + 23):
        e -= 1
    unit = Fraction(2) ** e
    n = q / unit
    fl = n.numerator // n.denominator
    rem = n - fl
    if rem > Fraction(1, 2) or (rem == Fraction(1, 2) and fl % 2):
        fl += 1
    return sign * fl * unit


def fused(a, x, y):
    return round32(a * x + y)


def unfused(a, x, y):
    return round32(round32(a * x) + y)


def bits(q):
    return struct.pack("<f", float(q)).hex()


def cases(n=4096, seed=20261031):
    """a = 2.5; x and y on the 2^-24 grid in [0, 1), so every input is an exact float32."""
    r = random.Random(seed)
    grain = Fraction(1, 1 << 24)
    return [(Fraction(5, 2), r.randrange(1 << 24) * grain, r.randrange(1 << 24) * grain) for _ in range(n)]


def test_round32_answers_known_values():
    assert round32(Fraction(1)) == 1
    assert round32(Fraction(1) + Fraction(1, 1 << 24)) == 1, "a tie goes to even"
    assert round32(Fraction(1) + Fraction(3, 1 << 24)) == 1 + Fraction(1, 1 << 22), "a tie goes to even, upward"
    assert struct.unpack("<f", struct.pack("<f", float(round32(Fraction(1, 3)))))[0] == float(round32(Fraction(1, 3)))


def test_reference_agrees_with_the_float_unit_on_the_unfused_class():
    """float64 holds a x exactly here and rounds to float32 correctly, so struct gives an independent answer."""
    to32 = lambda v: struct.unpack("<f", struct.pack("<f", v))[0]  # noqa: E731
    for a, x, y in cases(512):
        assert float(unfused(a, x, y)) == to32(to32(float(a) * float(x)) + float(y))


def test_the_two_classes_differ_and_stay_within_one_ulp():
    differ = 0
    for a, x, y in cases():
        f, u = fused(a, x, y), unfused(a, x, y)
        if f != u:
            differ += 1
            assert abs(f - u) <= Fraction(1, 1 << 21), (x, y)
    assert differ > 0, "the control must be able to fail: the classes never differed"


if __name__ == "__main__":
    test_round32_answers_known_values()
    test_reference_agrees_with_the_float_unit_on_the_unfused_class()
    test_the_two_classes_differ_and_stay_within_one_ulp()
    n = sum(fused(*c) != unfused(*c) for c in cases())
    print(f"saxpy rounding classes: {n} of 4096 inputs differ")
    sys.exit(0)
