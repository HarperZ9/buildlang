# ma_oracle.py: an independent oracle for the hand cases of emu-rv64 0.5.0
# and asm-rv64 0.2.0 (RISC-V M and A extensions, RV64).
#
# Standard library only. It shares no code and no table with the packages.
# It is written from the text of the RISC-V manual (riscv/riscv-isa-manual,
# commit d50ad50f): the encodings are the rows of the listing in
# src/unpriv/rv-32-64g.adoc, typed below as bit strings; the semantics are
# the sentences of m-st-ext.adoc, zalrsc.adoc and zaamo.adoc, written with
# Python integers, which have no width.
#
# The rules that are choices of emu-rv64 and not of the manual are marked
# CHOICE. They are the ones SPEC.md of emu-rv64 states.
#
# Usage: python ma_oracle.py OUTDIR
# It writes the case tables as BuildLang source and a plain listing.

import sys

M64 = (1 << 64) - 1
M32 = (1 << 32) - 1

# ---- encodings: rows of the listing, most significant bit first ----------
ROWS = {
    "mul":    "0000001 rs2 rs1 000 rd 0110011",
    "mulh":   "0000001 rs2 rs1 001 rd 0110011",
    "mulhsu": "0000001 rs2 rs1 010 rd 0110011",
    "mulhu":  "0000001 rs2 rs1 011 rd 0110011",
    "div":    "0000001 rs2 rs1 100 rd 0110011",
    "divu":   "0000001 rs2 rs1 101 rd 0110011",
    "rem":    "0000001 rs2 rs1 110 rd 0110011",
    "remu":   "0000001 rs2 rs1 111 rd 0110011",
    "mulw":   "0000001 rs2 rs1 000 rd 0111011",
    "divw":   "0000001 rs2 rs1 100 rd 0111011",
    "divuw":  "0000001 rs2 rs1 101 rd 0111011",
    "remw":   "0000001 rs2 rs1 110 rd 0111011",
    "remuw":  "0000001 rs2 rs1 111 rd 0111011",
    "lr.w":      "00010 aq rl 00000 rs1 010 rd 0101111",
    "sc.w":      "00011 aq rl rs2 rs1 010 rd 0101111",
    "amoswap.w": "00001 aq rl rs2 rs1 010 rd 0101111",
    "amoadd.w":  "00000 aq rl rs2 rs1 010 rd 0101111",
    "amoxor.w":  "00100 aq rl rs2 rs1 010 rd 0101111",
    "amoand.w":  "01100 aq rl rs2 rs1 010 rd 0101111",
    "amoor.w":   "01000 aq rl rs2 rs1 010 rd 0101111",
    "amomin.w":  "10000 aq rl rs2 rs1 010 rd 0101111",
    "amomax.w":  "10100 aq rl rs2 rs1 010 rd 0101111",
    "amominu.w": "11000 aq rl rs2 rs1 010 rd 0101111",
    "amomaxu.w": "11100 aq rl rs2 rs1 010 rd 0101111",
    "lr.d":      "00010 aq rl 00000 rs1 011 rd 0101111",
    "sc.d":      "00011 aq rl rs2 rs1 011 rd 0101111",
    "amoswap.d": "00001 aq rl rs2 rs1 011 rd 0101111",
    "amoadd.d":  "00000 aq rl rs2 rs1 011 rd 0101111",
    "amoxor.d":  "00100 aq rl rs2 rs1 011 rd 0101111",
    "amoand.d":  "01100 aq rl rs2 rs1 011 rd 0101111",
    "amoor.d":   "01000 aq rl rs2 rs1 011 rd 0101111",
    "amomin.d":  "10000 aq rl rs2 rs1 011 rd 0101111",
    "amomax.d":  "10100 aq rl rs2 rs1 011 rd 0101111",
    "amominu.d": "11000 aq rl rs2 rs1 011 rd 0101111",
    "amomaxu.d": "11100 aq rl rs2 rs1 011 rd 0101111",
}
M_NAMES = [n for n in ROWS if "." not in n]
A_NAMES = [n for n in ROWS if "." in n]


def encode(name, rd=0, rs1=0, rs2=0, aq=0, rl=0):
    bits = ""
    for part in ROWS[name].split():
        if part == "rd":
            bits += format(rd, "05b")
        elif part == "rs1":
            bits += format(rs1, "05b")
        elif part == "rs2":
            bits += format(rs2, "05b")
        elif part == "aq":
            bits += str(aq)
        elif part == "rl":
            bits += str(rl)
        else:
            bits += part
    assert len(bits) == 32, name
    return int(bits, 2)


def decode(word):
    """Returns (name, rd, rs1, rs2, aq, rl) or None, by trying every row."""
    text = format(word, "032b")
    for name, row in ROWS.items():
        pos = 0
        got = {"rd": 0, "rs1": 0, "rs2": 0, "aq": 0, "rl": 0}
        ok = True
        for part in row.split():
            width = {"rd": 5, "rs1": 5, "rs2": 5, "aq": 1, "rl": 1}.get(part, len(part))
            piece = text[pos:pos + width]
            pos += width
            if part in got:
                got[part] = int(piece, 2)
            elif piece != part:
                ok = False
                break
        if ok:
            return (name, got["rd"], got["rs1"], got["rs2"], got["aq"], got["rl"])
    return None


# ---- M semantics ----------------------------------------------------------
def signed(v, bits):
    v &= (1 << bits) - 1
    return v - (1 << bits) if v >> (bits - 1) else v


def sext32(v):
    return signed(v, 32) & M64


def trunc_div(a, b):
    """Integer division "rounding towards zero" of two Python integers."""
    q = a // b                      # floor
    if q < 0 and q * b != a:
        q += 1                      # move a negative inexact quotient up
    return q


def div_rule(a, b, bits, is_signed):
    """(quotient, remainder) as mathematical integers, table of m-st-ext."""
    if b == 0:
        return (-1 if is_signed else (1 << bits) - 1, a)
    if is_signed and a == -(1 << (bits - 1)) and b == -1:
        return (a, 0)
    q = trunc_div(a, b)
    return (q, a - q * b)


def m_value(name, a, b):
    """a, b: register values, 0 .. 2^64-1. Returns the value written to rd."""
    sa, sb = signed(a, 64), signed(b, 64)
    if name == "mul":
        return (a * b) & M64
    if name == "mulh":
        return ((sa * sb) >> 64) & M64
    if name == "mulhsu":
        return ((sa * b) >> 64) & M64
    if name == "mulhu":
        return ((a * b) >> 64) & M64
    if name == "div":
        return div_rule(sa, sb, 64, True)[0] & M64
    if name == "divu":
        return div_rule(a, b, 64, False)[0] & M64
    if name == "rem":
        return div_rule(sa, sb, 64, True)[1] & M64
    if name == "remu":
        return div_rule(a, b, 64, False)[1] & M64
    wa, wb = signed(a, 32), signed(b, 32)
    ua, ub = a & M32, b & M32
    if name == "mulw":
        return sext32(ua * ub)
    if name == "divw":
        return sext32(div_rule(wa, wb, 32, True)[0])
    if name == "divuw":
        return sext32(div_rule(ua, ub, 32, False)[0])
    if name == "remw":
        return sext32(div_rule(wa, wb, 32, True)[1])
    if name == "remuw":
        return sext32(div_rule(ua, ub, 32, False)[1])
    raise ValueError(name)


# ---- a small machine for the A cases --------------------------------------
RAM_BASE = 0x80000000
RAM_LEN = 512
MTIMECMP = 0x02004000
MTIME = 0x0200BFF8
MTVEC = 0x80000180


class Mach:
    def __init__(self):
        self.x = [0] * 32
        self.ram = bytearray(RAM_LEN)
        self.pc = RAM_BASE
        self.res = (0, 0)           # (width, address); width 0: none
        self.mtime = 0
        self.mtimecmp = M64
        self.steps = 0
        self.traps = 0
        self.mcause = 0
        self.mtval = 0
        self.mepc = 0

    def poke(self, addr, n, v):
        for i in range(n):
            self.ram[addr - RAM_BASE + i] = (v >> (8 * i)) & 0xFF

    def peek(self, addr, n):
        return sum(self.ram[addr - RAM_BASE + i] << (8 * i) for i in range(n))

    def region(self, addr, n):
        """'ram', 'cmp', 'time' or None for the n bytes at addr."""
        if RAM_BASE <= addr and addr + n <= RAM_BASE + RAM_LEN:
            return "ram"
        if MTIMECMP <= addr and addr + n <= MTIMECMP + 8:
            return "cmp"
        if MTIME <= addr and addr + n <= MTIME + 8:
            return "time"
        return None

    def read(self, addr, n, where):
        if where == "ram":
            return self.peek(addr, n)
        reg, base = (self.mtimecmp, MTIMECMP) if where == "cmp" else (self.mtime, MTIME)
        return (reg >> (8 * (addr - base))) & ((1 << (8 * n)) - 1)

    def write(self, addr, n, where, v):
        v &= (1 << (8 * n)) - 1
        if where == "ram":
            self.poke(addr, n, v)
            return
        base = MTIMECMP if where == "cmp" else MTIME
        shift = 8 * (addr - base)
        keep = M64 & ~(((1 << (8 * n)) - 1) << shift)
        if where == "cmp":
            self.mtimecmp = (self.mtimecmp & keep) | (v << shift)
        else:
            self.mtime = (self.mtime & keep) | (v << shift)

    def trap(self, cause, tval):
        self.mepc, self.mcause, self.mtval = self.pc, cause, tval
        self.pc = MTVEC
        self.traps += 1
        return 3

    def setx(self, r, v):
        if r:
            self.x[r] = v & M64

    def first_bad(self, addr, n):
        for i in range(n):
            a = addr + i
            if not (RAM_BASE <= a < RAM_BASE + RAM_LEN):
                return a
        return addr

    def step(self, word):
        rc = self.execute(word)
        self.mtime = (self.mtime + 1) & M64     # CHOICE: mtime counts steps
        return rc

    def execute(self, word):
        if word == 0x30102573:                  # csrrs x10, misa, x0
            # misa: MXL = 2 in bits 63:62; bit 0 A, bit 8 I, bit 12 M.
            self.setx(10, (2 << 62) | (1 << 12) | (1 << 8) | 1)
            return self.retire()
        name, rd, rs1, rs2, _aq, _rl = decode(word)
        if "." not in name:
            self.setx(rd, m_value(name, self.x[rs1], self.x[rs2]))
            return self.retire()
        op, width = name.split(".")
        n = 4 if width == "w" else 8
        addr = self.x[rs1]
        load = op == "lr"
        # "the address held in rs1 be naturally aligned to the size of the
        # operand ... If the address is not naturally aligned, an
        # address-misaligned exception or an access-fault exception will be
        # generated." CHOICE: address-misaligned, tested first.
        if addr % n:
            return self.trap(4 if load else 6, addr)
        where = self.region(addr, n)
        if where is None:
            return self.trap(5 if load else 7, self.first_bad(addr, n))
        old = self.read(addr, n, where)
        src = self.x[rs2]
        if n == 4:
            old, src = sext32(old), sext32(src)
        if op == "lr":
            self.res = (n, addr)                # CHOICE: the set is the bytes read
            self.setx(rd, old)
        elif op == "sc":
            ok = self.res == (n, addr)          # CHOICE: same address and width
            self.res = (0, 0)                   # "Regardless of success or failure"
            if ok:
                self.write(addr, n, where, src)
            self.setx(rd, 0 if ok else 1)
        else:
            self.write(addr, n, where, amo(op, old, src, n))
            self.setx(rd, old)
        return self.retire()

    def retire(self):
        self.pc += 4
        self.steps += 1
        return 0


def amo(op, old, src, n):
    """old and src are 64-bit patterns, sign-extended from 32 when n is 4."""
    bits = 8 * n
    so, ss = signed(old, 64), signed(src, 64)
    uo, us = old & ((1 << bits) - 1), src & ((1 << bits) - 1)
    if op == "amoswap":
        return src
    if op == "amoadd":
        return (old + src) & M64
    if op == "amoxor":
        return old ^ src
    if op == "amoand":
        return old & src
    if op == "amoor":
        return old | src
    if op == "amomin":
        return old if so < ss else src
    if op == "amomax":
        return old if so > ss else src
    if op == "amominu":
        return old if uo < us else src
    if op == "amomaxu":
        return old if uo > us else src
    raise ValueError(op)


# ---- the cases ------------------------------------------------------------
MIN64 = 1 << 63
MAX64 = MIN64 - 1


def neg(v):
    return (-v) & M64


MUL_PAIRS = [
    (7, 6), (neg(7), 6), (M64, M64), (MIN64, M64), (MIN64, 2), (MAX64, MAX64),
    (1 << 32, 1 << 32), (M32, M32), (0x123456789ABCDEF0, 0x0FEDCBA987654321),
    (MIN64, MIN64), (0, M64), (1, MIN64), (M64, 1), (M64, 2), (2, M64),
    (MAX64, MIN64), (0xFFFFFFFF00000001, 0x00000001FFFFFFFF),
]
DIV_PAIRS = [
    (7, 2), (neg(7), 2), (7, neg(2)), (neg(7), neg(2)), (0, 0), (1, 0),
    (M64, 0), (MIN64, 0), (12345, 0), (MIN64, M64), (MIN64, 1), (MAX64, M64),
    (1, MIN64), (M64, 1), (0x8000000000000001, M32), (5, 7), (neg(5), 7),
    (MIN64, 2), (MIN64, MIN64), (M64, MAX64), (1 << 32, M32),
]
MULW_PAIRS = [
    (7, 6), (0x7FFFFFFF, 2), (M32, M32), (0x100000003, 0x200000005),
    (0x80000000, 0x80000000), (0x10000, 0x8000), (M64, 1), (0x80000000, 1),
    (0xFFFF, 0x10001),
]
DIVW_PAIRS = [
    (7, 2), (neg(7), 2), (7, neg(2)), (neg(7), neg(2)), (0, 0), (5, 0),
    (0x80000000, 0), (0xFFFFFFFF00000005, 0), (M64, 0),
    (0x80000000, M32), (0xAAAAAAAA80000000, 0x55555555FFFFFFFF),
    (M32, 1), (0x7FFFFFFF, M32), (1 << 32, 3), (0x80000000, 2),
    (0x100000007, 0x100000000), (0xFFFFFFFE, 0x7FFFFFFF),
]


def m_cases():
    out = []
    for name in M_NAMES:
        if name in ("mul", "mulh", "mulhsu", "mulhu"):
            pairs = MUL_PAIRS
        elif name == "mulw":
            pairs = MULW_PAIRS
        elif name.endswith("w"):
            pairs = DIVW_PAIRS
        else:
            pairs = DIV_PAIRS
        for a, b in pairs:
            out.append((name, encode(name, rd=3, rs1=1, rs2=2), a, b, m_value(name, a, b)))
    return out


CELL = 0x80000100
D0 = 0x1122334455667788
D1 = 0x99AABBCCDDEEFF00
DNEG = 0x11223344D5667788           # its low word is negative as 32 bits


def a_case(note, words, x1=CELL, x2=0, x5=0, x6=0, d0=D0, d1=D1, res=(0, 0),
           mtime=0, mtimecmp=M64):
    m = Mach()
    m.x[1], m.x[2], m.x[5], m.x[6] = x1, x2, x5, x6
    for k, w in enumerate(words):
        m.poke(RAM_BASE + 4 * k, 4, w)
    m.poke(CELL, 8, d0)
    m.poke(CELL + 8, 8, d1)
    m.res, m.mtime, m.mtimecmp = res, mtime, mtimecmp
    rc = 0
    for w in words:
        rc = m.step(m.peek(m.pc, 4))
        if rc != 0:
            break
    padded = list(words) + [0] * (4 - len(words))
    i = padded + [len(words), x1, x2, x5, x6, d0, d1, res[0], res[1], mtime, mtimecmp]
    e = [rc, m.x[1], m.x[2], m.x[10], m.x[11], m.x[12], m.x[13],
         m.peek(CELL, 8), m.peek(CELL + 8, 8), m.res[0], m.res[1], m.steps, m.traps,
         m.mcause, m.mtval, m.mepc, m.pc, m.mtime, m.mtimecmp]
    # nothing outside the words and the two doublewords may have changed
    for off in list(range(16, 256)) + list(range(272, 512)):
        assert m.ram[off] == 0, note
    return (note, i, e)


def E(name, **kw):
    return encode(name, **kw)


def lr_sc_cases():
    c = []
    for wd in ("w", "d"):
        n = 4 if wd == "w" else 8
        lr, sc = "lr." + wd, "sc." + wd
        c.append(a_case(f"{lr} x10, (x1)", [E(lr, rd=10, rs1=1)]))
        c.append(a_case(f"{lr} x10, (x1): the word in memory is negative", [E(lr, rd=10, rs1=1)], d0=DNEG))
        c.append(a_case(f"{lr} x0, (x1): still reserves", [E(lr, rd=0, rs1=1)]))
        c.append(a_case(f"{lr} x10, (x1): replaces an older reservation", [E(lr, rd=10, rs1=1)], res=(8, CELL + 8)))
        c.append(a_case(f"{lr}.aq x10, (x1)", [E(lr, rd=10, rs1=1, aq=1)]))
        for off in (1, 2, 3) + ((4,) if n == 8 else ()):
            c.append(a_case(f"{lr} at offset {off}: misaligned, cause 4, reservation kept",
                            [E(lr, rd=10, rs1=1)], x1=CELL + off, res=(n, CELL)))
        c.append(a_case(f"{lr} one byte past memory: cause 5", [E(lr, rd=10, rs1=1)], x1=RAM_BASE + RAM_LEN))
        c.append(a_case(f"{lr} below memory: cause 5", [E(lr, rd=10, rs1=1)], x1=0x7FFFFFF8))
        c.append(a_case(f"{lr} at the serial device: cause 5", [E(lr, rd=10, rs1=1)], x1=0x10000000))
        c.append(a_case(f"{sc} x10, x2, (x1): reservation held", [E(sc, rd=10, rs1=1, rs2=2)],
                        x2=0xA1B2C3D4E5F60718, res=(n, CELL)))
        c.append(a_case(f"{sc} x10, x2, (x1): no reservation", [E(sc, rd=10, rs1=1, rs2=2)], x2=0xA1B2C3D4E5F60718))
        c.append(a_case(f"{sc}: reservation at another address", [E(sc, rd=10, rs1=1, rs2=2)],
                        x2=0x77, res=(n, CELL + 8)))
        c.append(a_case(f"{sc}: reservation of the other width", [E(sc, rd=10, rs1=1, rs2=2)],
                        x2=0x77, res=(12 - n, CELL)))
        c.append(a_case(f"{sc}.rl x10, x2, (x1): reservation held", [E(sc, rd=10, rs1=1, rs2=2, rl=1)],
                        x2=0x55, res=(n, CELL)))
        c.append(a_case(f"{sc} x2, x2, (x1): rd is rs2", [E(sc, rd=2, rs1=1, rs2=2)], x2=0x0123456789ABCDEF, res=(n, CELL)))
        c.append(a_case(f"{sc} x0, x2, (x1): rd is x0", [E(sc, rd=0, rs1=1, rs2=2)], x2=0x42, res=(n, CELL)))
        c.append(a_case(f"{sc} at offset 2: misaligned, cause 6, reservation kept",
                        [E(sc, rd=10, rs1=1, rs2=2)], x1=CELL + 2, x2=9, res=(n, CELL)))
        c.append(a_case(f"{sc} past memory with no reservation: cause 7, not rd = 1",
                        [E(sc, rd=10, rs1=1, rs2=2)], x1=RAM_BASE + RAM_LEN, x2=9))
        c.append(a_case(f"{sc} past memory with that address reserved: cause 7, reservation kept",
                        [E(sc, rd=10, rs1=1, rs2=2)], x1=RAM_BASE + RAM_LEN, x2=9, res=(n, RAM_BASE + RAM_LEN)))
        # sequences
        c.append(a_case(f"{lr}; {sc}: succeeds", [E(lr, rd=10, rs1=1), E(sc, rd=11, rs1=1, rs2=2)], x2=0xCAFE0000BEEF0001))
        c.append(a_case(f"{lr}; {sc}; {sc}: the second SC fails",
                        [E(lr, rd=10, rs1=1), E(sc, rd=11, rs1=1, rs2=2), E(sc, rd=12, rs1=1, rs2=5)],
                        x2=0x1111, x5=0x2222))
        c.append(a_case(f"{lr} A; {lr} B; {sc} A: fails",
                        [E(lr, rd=10, rs1=1), E(lr, rd=11, rs1=5), E(sc, rd=12, rs1=1, rs2=2)], x2=0x3333, x5=CELL + 8))
        c.append(a_case(f"{lr} A; {lr} B; {sc} B: succeeds",
                        [E(lr, rd=10, rs1=1), E(lr, rd=11, rs1=5), E(sc, rd=12, rs1=5, rs2=2)], x2=0x3333, x5=CELL + 8))
        c.append(a_case(f"{lr} A; {sc} B; {sc} A: both fail",
                        [E(lr, rd=10, rs1=1), E(sc, rd=11, rs1=5, rs2=2), E(sc, rd=12, rs1=1, rs2=2)], x2=0x4444, x5=CELL + 8))
        c.append(a_case(f"{lr}; amoadd.{wd} to the same address; {sc}: succeeds, a store of this hart keeps the reservation",
                        [E(lr, rd=10, rs1=1), E("amoadd." + wd, rd=11, rs1=1, rs2=2), E(sc, rd=12, rs1=1, rs2=5)],
                        x2=1, x5=0x5555))
        c.append(a_case(f"{lr}; mul; {sc}: succeeds",
                        [E(lr, rd=10, rs1=1), E("mul", rd=11, rs1=10, rs2=2), E(sc, rd=12, rs1=1, rs2=11)], x2=3))
    c.append(a_case("lr.w; sc.d at the same address: fails", [E("lr.w", rd=10, rs1=1), E("sc.d", rd=11, rs1=1, rs2=2)], x2=0x66))
    c.append(a_case("lr.d; sc.w at the same address: fails", [E("lr.d", rd=10, rs1=1), E("sc.w", rd=11, rs1=1, rs2=2)], x2=0x66))
    c.append(a_case("lr.d; sc.w at offset 4: fails", [E("lr.d", rd=10, rs1=1), E("sc.w", rd=11, rs1=5, rs2=2)], x2=0x66, x5=CELL + 4))
    c.append(a_case("lr.w at offset 4; sc.w at offset 4: succeeds, the low word stays",
                    [E("lr.w", rd=10, rs1=1), E("sc.w", rd=11, rs1=1, rs2=2)], x1=CELL + 4, x2=0xFFFFFFFF80000001))
    c.append(a_case("lr.d; misaligned lr.d traps; the reservation stays",
                    [E("lr.d", rd=10, rs1=1), E("lr.d", rd=11, rs1=5)], x5=CELL + 12))
    return c


AMO_OPS = ["amoswap", "amoadd", "amoxor", "amoand", "amoor", "amomin", "amomax", "amominu", "amomaxu"]


def amo_cases():
    c = []
    for wd in ("w", "d"):
        for op in AMO_OPS:
            nm = f"{op}.{wd}"
            w = E(nm, rd=10, rs1=1, rs2=2)
            c.append(a_case(f"{nm} x10, x2, (x1)", [w], x2=0x0F0F0F0F33335555))
            c.append(a_case(f"{nm}: memory -1, source 1", [w], x2=1, d0=M64))
            c.append(a_case(f"{nm}: memory 1, source -1", [w], x2=M64, d0=1))
            if wd == "w":
                c.append(a_case(f"{nm}: memory word 0x80000000, source 0x7FFFFFFF with a dirty upper half",
                                [w], x2=0xDEADBEEF7FFFFFFF, d0=0x1234567880000000))
            else:
                c.append(a_case(f"{nm}: memory most negative, source most positive", [w], x2=MAX64, d0=MIN64))
        c.append(a_case(f"amoadd.{wd} x0, x2, (x1): memory changes, no register", [E("amoadd." + wd, rd=0, rs1=1, rs2=2)], x2=5))
        c.append(a_case(f"amoadd.{wd} x2, x2, (x1): rd is rs2", [E("amoadd." + wd, rd=2, rs1=1, rs2=2)], x2=5))
        c.append(a_case(f"amoswap.{wd} x1, x2, (x1): rd is rs1", [E("amoswap." + wd, rd=1, rs1=1, rs2=2)], x2=5))
        c.append(a_case(f"amoadd.{wd} x10, x1, (x1): rs2 is rs1", [E("amoadd." + wd, rd=10, rs1=1, rs2=1)]))
        c.append(a_case(f"amoswap.{wd}.aqrl x10, x2, (x1)", [E("amoswap." + wd, rd=10, rs1=1, rs2=2, aq=1, rl=1)], x2=0x99))
        c.append(a_case(f"amoor.{wd} at the second doubleword", [E("amoor." + wd, rd=10, rs1=1, rs2=2)], x1=CELL + 8, x2=0xFF))
        c.append(a_case(f"amoxor.{wd} at offset 2: misaligned, cause 6", [E("amoxor." + wd, rd=10, rs1=1, rs2=2)], x1=CELL + 2, x2=0xFF))
        c.append(a_case(f"amoand.{wd} past memory: cause 7", [E("amoand." + wd, rd=10, rs1=1, rs2=2)], x1=RAM_BASE + RAM_LEN, x2=0xFF))
        c.append(a_case(f"amoswap.{wd} at the serial device: cause 7", [E("amoswap." + wd, rd=10, rs1=1, rs2=2)], x1=0x10000000, x2=0x41))
        c.append(a_case(f"amoswap.{wd} twice: memory is back",
                        [E("amoswap." + wd, rd=10, rs1=1, rs2=2), E("amoswap." + wd, rd=11, rs1=1, rs2=10)], x2=0x0BADF00D0BADF00D))
        c.append(a_case(f"amoadd.{wd} with a reservation held: it stays", [E("amoadd." + wd, rd=10, rs1=1, rs2=2)], x2=1, res=(8, CELL)))
    c.append(a_case("amoadd.w at offset 4: the low word stays", [E("amoadd.w", rd=10, rs1=1, rs2=2)], x1=CELL + 4, x2=0x10))
    c.append(a_case("amoadd.d at offset 4: misaligned, cause 6", [E("amoadd.d", rd=10, rs1=1, rs2=2)], x1=CELL + 4, x2=0x10))
    c.append(a_case("amoadd.w: 0xFFFFFFFF + 1 wraps inside the word", [E("amoadd.w", rd=10, rs1=1, rs2=2)], x2=1, d0=0x00000007FFFFFFFF))
    return c


def device_cases():
    c = []
    c.append(a_case("lr.d at mtime: reads the time of this step", [E("lr.d", rd=10, rs1=1)], x1=MTIME, mtime=500))
    c.append(a_case("lr.w at mtime + 4: the upper half", [E("lr.w", rd=10, rs1=1)], x1=MTIME + 4, mtime=0x8000000100000002))
    c.append(a_case("lr.w at mtime + 2: misaligned, cause 4", [E("lr.w", rd=10, rs1=1)], x1=MTIME + 2, mtime=500))
    c.append(a_case("amoadd.d at mtimecmp", [E("amoadd.d", rd=10, rs1=1, rs2=2)], x1=MTIMECMP, x2=5, mtimecmp=1000))
    c.append(a_case("amoswap.w at mtimecmp + 4", [E("amoswap.w", rd=10, rs1=1, rs2=2)], x1=MTIMECMP + 4, x2=0x1234, mtimecmp=0xFFFFFFFF00000064))
    c.append(a_case("lr.d at mtime; sc.d stores 77: the tick follows the store",
                    [E("lr.d", rd=10, rs1=1), E("sc.d", rd=11, rs1=1, rs2=2)], x1=MTIME, x2=77, mtime=500))
    c.append(a_case("amomaxu.d at mtimecmp + 8: outside the register, cause 7", [E("amomaxu.d", rd=10, rs1=1, rs2=2)], x1=MTIMECMP + 8, x2=5))
    c.append(a_case("csrrs x10, misa, x0: MXL 2 and the bits A, I and M", [0x30102573]))
    return c


# ---- output ---------------------------------------------------------------
def h(v):
    return "0x%X_u64" % v


def wrap(head, values):
    """An array literal on lines of at most 100 characters."""
    out, cur = [], head
    for n, v in enumerate(values):
        piece = h(v) + ("];" if n == len(values) - 1 else ", ")
        if len(cur) + len(piece.rstrip()) > 100:
            out.append(cur.rstrip())
            cur = "        " + piece
        else:
            cur += piece
    out.append(cur)
    return out


def chunks(seq, size):
    for i in range(0, len(seq), size):
        yield seq[i:i + size]


def emit_m(outdir, cases, first):
    """Writes xvec_m1.bld, xvec_m2.bld ... Returns the function names."""
    names = []
    files = list(chunks(cases, 100))
    num = first
    for fi, group in enumerate(files, 1):
        lines = ["// Hand cases of the M extension, part %d. Written by" % fi,
                 "// tests/tools/ma_oracle.py; do not edit by hand. Each row is",
                 "// word, a, b, expected x3 for `op x3, x1, x2`.", ""]
        for gi, part in enumerate(chunks(group, 20), 1):
            fn = "xv_m%d_%d" % (fi, gi)
            names.append(fn)
            lines.append("fn %s(st: &mut XSt) ~ Console {" % fn)
            for name, word, a, b, want in part:
                lines.append("    // %d: %s x3, x1, x2" % (num, name))
                lines.append("    xv_m(st, %s, %s, %s, %s);" % (h(word), h(a), h(b), h(want)))
                num += 1
            lines.append("}")
            lines.append("")
        with open("%s/xvec_m%d.bld" % (outdir, fi), "w", newline="\n") as f:
            f.write("\n".join(lines))
    return names, num


def emit_a(outdir, cases, first):
    names = []
    num = first
    for fi, group in enumerate(chunks(cases, 24), 1):
        lines = ["// Hand cases of the A extension, part %d. Written by" % fi,
                 "// tests/tools/ma_oracle.py; do not edit by hand. xvec.bld",
                 "// explains the two arrays of a case.", ""]
        for gi, part in enumerate(chunks(group, 4), 1):
            fn = "xv_a%d_%d" % (fi, gi)
            names.append(fn)
            lines.append("fn %s(st: &mut XSt) ~ Console {" % fn)
            for k, (note, i, e) in enumerate(part):
                lines.append("    // %d: %s" % (num, note))
                lines += wrap("    let i%d: [u64; 15] = [" % k, i)
                lines += wrap("    let e%d: [u64; 19] = [" % k, e)
                lines.append("    xv_a(st, &i%d, &e%d);" % (k, k))
                num += 1
            lines.append("}")
            lines.append("")
        with open("%s/xvec_a%d.bld" % (outdir, fi), "w", newline="\n") as f:
            f.write("\n".join(lines))
    return names, num


def emit_run(outdir, m_names, a_names, m_count, total):
    lines = ["// Hand cases of the M and A extensions: the list of tables. Written by",
             "// tests/tools/ma_oracle.py; do not edit by hand.", ""]
    for gi, part in enumerate(chunks(m_names + a_names, 30), 1):
        lines.append("fn xv_tables_%d(st: &mut XSt) ~ Console {" % gi)
        lines += ["    %s(st);" % n for n in part]
        lines.append("}")
        lines.append("")
    groups = len(list(chunks(m_names + a_names, 30)))
    lines.append("// Cases 1 to %d are M cases, %d to %d are A cases, and the last two" % (m_count, m_count + 1, total))
    lines.append("// run an M word and an A word in the report profile.")
    lines.append("fn xv_tables(st: &mut XSt) ~ Console {")
    lines += ["    xv_tables_%d(st);" % g for g in range(1, groups + 1)]
    lines.append("    xv_report(st, %s);" % h(encode("mul", rd=3, rs1=1, rs2=2)))
    lines.append("    xv_report(st, %s);" % h(encode("amoadd.w", rd=3, rs1=1, rs2=2)))
    lines.append("}")
    lines.append("")
    with open("%s/xvec_run.bld" % outdir, "w", newline="\n") as f:
        f.write("\n".join(lines))


# ---- cases of the assembler: text and the word it must give ---------------
def asm_ok_cases():
    """Groups of (text, word). The text is built from the same field values
    that go to encode(), except where a way of writing is the point."""
    m = [("%s x3, x1, x2" % n, encode(n, rd=3, rs1=1, rs2=2)) for n in M_NAMES]
    m += [("mul x0, x0, x0", encode("mul")),
          ("mul x31, x31, x31", encode("mul", rd=31, rs1=31, rs2=31)),
          ("divu x10, x20, x30", encode("divu", rd=10, rs1=20, rs2=30)),
          ("remw x9, x19, x29", encode("remw", rd=9, rs1=19, rs2=29)),
          ("mulhsu x7, x8, x9", encode("mulhsu", rd=7, rs1=8, rs2=9))]
    a = []
    for n in A_NAMES:
        if n.startswith("lr"):
            a.append(("%s x3, (x1)" % n, encode(n, rd=3, rs1=1)))
        else:
            a.append(("%s x3, x2, (x1)" % n, encode(n, rd=3, rs1=1, rs2=2)))
    f = [
        # the three statements the manual prints, with t0 = x5, t1 = x6,
        # a0 = x10 and a2 = x12
        ("lr.w x5, (x10)", encode("lr.w", rd=5, rs1=10)),
        ("sc.w x5, x12, (x10)", encode("sc.w", rd=5, rs1=10, rs2=12)),
        ("amoswap.w.aq x6, x5, (x10)", encode("amoswap.w", rd=6, rs1=10, rs2=5, aq=1)),
        ("amoswap.w.rl x0, x0, (x10)", encode("amoswap.w", rd=0, rs1=10, rs2=0, rl=1)),
        ("lr.w.aq x3, (x1)", encode("lr.w", rd=3, rs1=1, aq=1)),
        ("lr.d.rl x3, (x1)", encode("lr.d", rd=3, rs1=1, rl=1)),
        ("lr.d.aqrl x3, (x1)", encode("lr.d", rd=3, rs1=1, aq=1, rl=1)),
        ("sc.w.rl x3, x2, (x1)", encode("sc.w", rd=3, rs1=1, rs2=2, rl=1)),
        ("sc.d.aq x3, x2, (x1)", encode("sc.d", rd=3, rs1=1, rs2=2, aq=1)),
        ("sc.d.aqrl x3, x2, (x1)", encode("sc.d", rd=3, rs1=1, rs2=2, aq=1, rl=1)),
        ("amoadd.d.aqrl x31, x30, (x29)", encode("amoadd.d", rd=31, rs1=29, rs2=30, aq=1, rl=1)),
        ("amomaxu.w.aqrl x31, x31, (x31)", encode("amomaxu.w", rd=31, rs1=31, rs2=31, aq=1, rl=1)),
        ("amomin.d.aq x9, x8, (x7)", encode("amomin.d", rd=9, rs1=7, rs2=8, aq=1)),
        ("lr.w x0, (x0)", encode("lr.w")),
        ("lr.d x31, (x31)", encode("lr.d", rd=31, rs1=31)),
        ("sc.w x0, x0, (x0)", encode("sc.w")),
        ("sc.d x1, x31, (x2)", encode("sc.d", rd=1, rs1=2, rs2=31)),
        ("amoor.d x4, x5, (x6)", encode("amoor.d", rd=4, rs1=6, rs2=5)),
        # ways of writing
        ("lr.w x5,(x10)", encode("lr.w", rd=5, rs1=10)),
        ("lr.w  x5 , ( x10 )", encode("lr.w", rd=5, rs1=10)),
        ("sc.d   x5,x12,( x10)", encode("sc.d", rd=5, rs1=10, rs2=12)),
        ("amoadd.w x1, x2, (x3) # note", encode("amoadd.w", rd=1, rs1=3, rs2=2)),
        ("here: amoxor.d x4, x5, (x6)", encode("amoxor.d", rd=4, rs1=6, rs2=5)),
        ("  mulw x4,x5,x6", encode("mulw", rd=4, rs1=5, rs2=6)),
    ]
    return m, a, f


ASM_REFUSED = [
    ("lr.w x1, x2", "AE_ADDR"), ("lr.w x1, 0(x2)", "AE_ADDR"), ("lr.d x1, 8(x2)", "AE_ADDR"),
    ("lr.w x1, (x2", "AE_ADDR"), ("lr.w x1, (x2) x3", "AE_TRAIL"), ("lr.w x1, (x32)", "AE_REG"),
    ("lr.w x1, ()", "AE_REG"), ("lr.w x1, (5)", "AE_REG"), ("lr.w x1, (x2), x3", "AE_COUNT"),
    ("lr.w x1", "AE_COUNT"), ("lr.w (x2)", "AE_COUNT"), ("lr.w 1, (x2)", "AE_REG"),
    ("sc.w x1, (x2)", "AE_COUNT"), ("sc.w x1, x2, x3", "AE_ADDR"), ("sc.d x1, 3, (x2)", "AE_REG"),
    ("sc.d x1, (x2), x3", "AE_REG"), ("amoadd.w x1, x2, 0(x3)", "AE_ADDR"),
    ("amoadd.w x1, x2, (x3))", "AE_TRAIL"), ("amoadd.w x1, x2, (x3), x4", "AE_COUNT"),
    ("amoadd.w x1, , (x3)", "AE_EMPTY"), ("amoswap.d x1, x2", "AE_COUNT"),
    ("mul.aq x1, x2, x3", "AE_MNEMONIC"), ("add.aqrl x1, x2, x3", "AE_MNEMONIC"),
    ("lr.w.rlaq x1, (x2)", "AE_MNEMONIC"), ("lr.w.aq.rl x1, (x2)", "AE_MNEMONIC"),
    ("lr.q x1, (x2)", "AE_MNEMONIC"), ("lr x1, (x2)", "AE_MNEMONIC"), ("amoadd x1, x2, (x3)", "AE_MNEMONIC"),
    ("LR.W x1, (x2)", "AE_MNEMONIC"), ("amoadd.w.AQ x1, x2, (x3)", "AE_MNEMONIC"),
    (".aq x1, (x2)", "AE_DIRECTIVE"), ("mulhsu x1, x2", "AE_COUNT"), ("divw x1, x2, 3", "AE_REG"),
    ("rem x1, x2, x3, x4", "AE_COUNT"), ("mulh x1, x2, (x3)", "AE_REG"),
]


def bl_text(t):
    return t.replace("\\", "\\\\").replace("\t", "\\t").replace('"', '\\"')


def emit_asm(outdir):
    m, a, f = asm_ok_cases()
    lines = ["// Hand cases of asm-rv64, seventh part: the M and A extensions, one",
             "// statement each. Written by tests/tools/ma_oracle.py; do not edit by",
             "// hand. The words come from that script, which builds each from the bit",
             "// rows of the instruction listing in the manual and shares no code and",
             "// no table with the encoder or the assembler.", ""]
    groups = [("ac_x_mul", "The M extension: rd, rs1, rs2.", m),
              ("ac_x_atomic_w", "The 32-bit atomic forms.", a[:11]),
              ("ac_x_atomic_d", "The 64-bit atomic forms.", a[11:]),
              ("ac_x_order", "Ordering suffixes, register edges and ways of writing.", f)]
    for fn, note, part in groups:
        lines.append("// " + note)
        lines.append("fn %s(st: &mut AcSt) ~ Console {" % fn)
        lines += ['    ac_w(st, "%s", 0x%08X_u64);' % (bl_text(t), w) for t, w in part]
        lines += ["}", ""]
    for gi, part in enumerate(chunks(ASM_REFUSED, 18), 1):
        lines.append("// Statements that must be refused, part %d." % gi)
        lines.append("fn ac_x_refuse_%d(st: &mut AcSt) ~ Console {" % gi)
        lines += ['    ac_e(st, "%s", %s);' % (bl_text(t), c) for t, c in part]
        lines += ["}", ""]
    lines.append("fn ac_x_all(st: &mut AcSt) ~ Console {")
    lines += ["    %s(st);" % g[0] for g in groups]
    lines += ["    ac_x_refuse_%d(st);" % g for g in range(1, len(list(chunks(ASM_REFUSED, 18))) + 1)]
    lines += ["}", ""]
    with open(outdir + "/acase7.bld", "w", newline="\n") as fh:
        fh.write("\n".join(lines))
    return len(m) + len(a) + len(f), len(ASM_REFUSED)


def text_of(word):
    """The statement for a word of the tables above, in the manual's form."""
    if word == 0x30102573:
        return "csrrs x10, misa, x0"
    name, rd, rs1, rs2, aq, rl = decode(word)
    if "." not in name:
        return "%s x%d, x%d, x%d" % (name, rd, rs1, rs2)
    name += {(0, 0): "", (1, 0): ".aq", (0, 1): ".rl", (1, 1): ".aqrl"}[(aq, rl)]
    if name.startswith("lr"):
        return "%s x%d, (x%d)" % (name, rd, rs1)
    return "%s x%d, x%d, (x%d)" % (name, rd, rs2, rs1)


def emit_hand(outdir, mc, ac):
    lines = ["# The hand cases of `emu-rv64 xcases` (0.5.0) as text, one statement per",
             "# case, in case order: the instruction of an M case, and the first",
             "# instruction of an A case. Written by tests/tools/ma_oracle.py.",
             "# `asm-rv64 hand` compares the words with the `word=` values of the",
             "# golden file of that command.", ""]
    n = 1
    for name, word, a, b, want in mc:
        lines.append("%-40s# xcase %d" % (text_of(word), n))
        n += 1
    for note, i, e in ac:
        lines.append("%-40s# xcase %d" % (text_of(i[0]), n))
        n += 1
    lines.append("%-40s# xcase %d" % ("mul x3, x1, x2", n))
    lines.append("%-40s# xcase %d" % ("amoadd.w x3, x2, (x1)", n + 1))
    with open(outdir + "/hand-xcases.s", "w", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")


def self_check():
    # every row decodes to itself and to nothing else
    for name in ROWS:
        w = encode(name, rd=3, rs1=1, rs2=0 if name.startswith("lr") else 2, aq=1, rl=0)
        assert decode(w)[0] == name, name
    # identities that hold whatever the implementation
    for a, b in MUL_PAIRS + DIV_PAIRS:
        lo, hi = m_value("mul", a, b), m_value("mulhu", a, b)
        assert (hi << 64) | lo == a * b
        if b and not (a == MIN64 and b == M64):
            q, r = signed(m_value("div", a, b), 64), signed(m_value("rem", a, b), 64)
            assert q * signed(b, 64) + r == signed(a, 64)
            assert r == 0 or (r < 0) == (signed(a, 64) < 0)
            assert abs(r) < abs(signed(b, 64))
    assert m_value("div", MIN64, M64) == MIN64 and m_value("rem", MIN64, M64) == 0
    assert m_value("divu", 5, 0) == M64 and m_value("remu", 5, 0) == 5
    assert m_value("divw", 0x80000000, M32) == 0xFFFFFFFF80000000
    assert m_value("remuw", 0x80000000, 0) == 0xFFFFFFFF80000000


def main():
    outdir = sys.argv[1]
    self_check()
    mc = m_cases()
    ac = lr_sc_cases() + amo_cases() + device_cases()
    m_names, nxt = emit_m(outdir, mc, 1)
    a_names, end = emit_a(outdir, ac, nxt)
    emit_run(outdir, m_names, a_names, len(mc), end - 1)
    with open(outdir + "/cases-listing.txt", "w", newline="\n") as f:
        n = 1
        for name, word, a, b, want in mc:
            f.write("%d %s word=%08x a=%x b=%x x3=%x\n" % (n, name, word, a, b, want))
            n += 1
        for note, i, e in ac:
            f.write("%d %s\n   in  %s\n   out %s\n" % (n, note, " ".join("%x" % v for v in i), " ".join("%x" % v for v in e)))
            n += 1
    print("M cases %d, A cases %d, report cases 2, total %d" % (len(mc), len(ac), end + 1))
    ok, bad = emit_asm(outdir)
    emit_hand(outdir, mc, ac)
    print("assembler cases: %d words, %d refusals" % (ok, bad))


if __name__ == "__main__":
    main()
