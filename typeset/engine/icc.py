"""A minimal ICC v2 sRGB display profile, built from the sRGB primaries.

Matrix/TRC profile: D50-adapted colorants (Bradford, as in the published
sRGB ICC profiles), a 1024-entry tone curve from the IEC 61966-2-1
transfer function. Enough for a PDF/A OutputIntent.
"""
import struct


def _s15(v):
    return struct.pack(">i", int(round(v * 65536)))


def _xyz(x, y, z):
    return b"XYZ " + b"\0" * 4 + _s15(x) + _s15(y) + _s15(z)


def _curve():
    n = 1024
    vals = []
    for i in range(n):
        c = i / (n - 1)
        lin = c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
        vals.append(int(round(lin * 65535)))
    return b"curv" + b"\0" * 4 + struct.pack(">I", n) + struct.pack(">%dH" % n, *vals)


def _desc(text):
    t = text.encode("ascii") + b"\0"
    return (b"desc" + b"\0" * 4 + struct.pack(">I", len(t)) + t + b"\0" * 8 +
            b"\0" * 3 + b"\0" * 67)


def _text(text):
    return b"text" + b"\0" * 4 + text.encode("ascii") + b"\0"


def srgb_profile():
    tags = [
        (b"desc", _desc("sRGB (buildlang typeset)")),
        (b"cprt", _text("No copyright, use freely")),
        (b"wtpt", _xyz(0.9642, 1.0, 0.8249)),
        (b"rXYZ", _xyz(0.4361, 0.2225, 0.0139)),
        (b"gXYZ", _xyz(0.3851, 0.7169, 0.0971)),
        (b"bXYZ", _xyz(0.1431, 0.0606, 0.7141)),
    ]
    curve = _curve()
    tags += [(b"rTRC", curve), (b"gTRC", curve), (b"bTRC", curve)]
    count = len(tags)
    offset = 128 + 4 + 12 * count
    table, body, seen = b"", b"", {}
    for sig, data in tags:
        if data in seen:
            off, ln = seen[data]
        else:
            pad = (4 - (offset + len(body)) % 4) % 4
            body += b"\0" * pad
            off, ln = offset + len(body), len(data)
            body += data
            seen[data] = (off, ln)
        table += sig + struct.pack(">II", off, ln)
    size = offset + len(body)
    header = struct.pack(">I", size) + b"none" + struct.pack(">I", 0x02100000) + b"mntrRGB XYZ "
    header += struct.pack(">6H", 2026, 10, 31, 0, 0, 0) + b"acsp" + b"MSFT" + struct.pack(">I", 0)
    header += b"none" + b"none" + b"\0" * 8 + struct.pack(">I", 0)
    header += _s15(0.9642) + _s15(1.0) + _s15(0.8249) + b"none"
    header += b"\0" * (128 - len(header))
    return header + struct.pack(">I", count) + table + body
