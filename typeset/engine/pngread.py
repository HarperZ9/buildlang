"""A small PNG reader for figures: 8-bit greyscale, RGB, grey with alpha and RGBA, not interlaced.

Returns (width, height, rgb bytes). Alpha is composited onto a background colour, since the PDF
writer places plain DeviceRGB images. Dependency-free, like the rest of the engine.
"""
import struct
import zlib

SIGNATURE = b"\x89PNG\r\n\x1a\n"
CHANNELS = {0: 1, 2: 3, 4: 2, 6: 4}


def _unfilter(raw, width, height, bpp):
    stride = width * bpp
    out = bytearray(height * stride)
    prev = bytearray(stride)
    pos = 0
    for y in range(height):
        ftype = raw[pos]
        line = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        if ftype == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif ftype == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 255
        elif ftype == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
        elif ftype == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b, c = prev[i], prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        elif ftype != 0:
            raise ValueError(f"PNG filter type {ftype} on row {y}")
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return out


def read_png(path, background=(255, 255, 255)):
    with open(path, "rb") as fh:
        data = fh.read()
    if not data.startswith(SIGNATURE):
        raise ValueError(f"{path}: not a PNG file")
    pos, idat, head = 8, [], None
    while pos < len(data):
        n, tag = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if tag == b"IHDR":
            head = struct.unpack(">IIBBBBB", body)
        elif tag == b"IDAT":
            idat.append(body)
        elif tag == b"IEND":
            break
        pos += 12 + n
    width, height, depth, ctype, _, _, interlace = head
    if depth != 8 or ctype not in CHANNELS or interlace:
        raise ValueError(f"{path}: only 8-bit, non-interlaced grey, RGB, grey-alpha or RGBA PNG is read "
                         f"(depth {depth}, colour type {ctype}, interlace {interlace})")
    ch = CHANNELS[ctype]
    px = _unfilter(zlib.decompress(b"".join(idat)), width, height, ch)
    if ctype == 2:
        return width, height, bytes(px)
    out = bytearray(width * height * 3)
    br, bg, bb = background
    for i in range(width * height):
        s = px[i * ch:(i + 1) * ch]
        r, g, b = (s[0], s[0], s[0]) if ch <= 2 else (s[0], s[1], s[2])
        a = s[-1] if ch in (2, 4) else 255
        if a != 255:
            r, g, b = ((r * a + br * (255 - a)) // 255, (g * a + bg * (255 - a)) // 255, (b * a + bb * (255 - a)) // 255)
        out[i * 3:i * 3 + 3] = bytes((r, g, b))
    return width, height, bytes(out)
