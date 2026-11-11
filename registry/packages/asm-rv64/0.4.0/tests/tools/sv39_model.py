# sv39_model.py: the machine of the oracle for the system profile of
# emu-rv64 0.6.0 (privilege modes M, S and U, trap delegation, Sv39).
#
# Standard library only. It shares no code and no table with the package.
# It is written from the text of the RISC-V manual (riscv/riscv-isa-manual,
# commit d50ad50f): src/priv/machine.adoc, supervisor.adoc and csrs.adoc.
# Field positions are the field lists of the figures, typed below in the
# order of the figure, least significant field first. Encodings are rows of
# the listings (src/unpriv/rv-32-64g.adoc and the figure
# priv-instr-set.edn), typed as bit strings.
#
# Rules that are choices of emu-rv64 and not of the manual are marked
# CHOICE. They are the ones SPEC.md states.

M64 = (1 << 64) - 1
U, S, M = 0, 1, 3
FETCH, LOAD, STORE = 0, 1, 2
RAM_BASE = 0x80000000
MTIMECMP, MTIME = 0x02004000, 0x0200BFF8
UART = 0x10000000

# Figure mstatusreg.edn (RV64), and the sstatus figure of supervisor.adoc
# for SXLEN=64: (width, name), from bit 0 upward.
MSTATUS_FIG = [(1, "WPRI"), (1, "SIE"), (1, "WPRI"), (1, "MIE"), (1, "WPRI"), (1, "SPIE"),
               (1, "UBE"), (1, "MPIE"), (1, "SPP"), (2, "VS"), (2, "MPP"), (2, "FS"), (2, "XS"),
               (1, "MPRV"), (1, "SUM"), (1, "MXR"), (1, "TVM"), (1, "TW"), (1, "TSR"),
               (1, "SPELP"), (1, "SDT"), (7, "WPRI"), (2, "UXL"), (2, "SXL"), (1, "SBE"),
               (1, "MBE"), (1, "GVA"), (1, "MPV"), (1, "WPRI"), (1, "MPELP"), (1, "MDT"),
               (20, "WPRI"), (1, "SD")]
SSTATUS_FIG = [(1, "WPRI"), (1, "SIE"), (3, "WPRI"), (1, "SPIE"), (1, "UBE"), (1, "WPRI"),
               (1, "SPP"), (2, "VS"), (2, "WPRI"), (2, "FS"), (2, "XS"), (1, "WPRI"), (1, "SUM"),
               (1, "MXR"), (3, "WPRI"), (1, "SPELP"), (1, "SDT"), (7, "WPRI"), (2, "UXL"),
               (29, "WPRI"), (1, "SD")]


def layout(fig):
    pos, at = {}, 0
    for width, name in fig:
        if name != "WPRI":
            pos[name] = (at, width)
        at += width
    assert at == 64
    return pos


MPOS, SPOS = layout(MSTATUS_FIG), layout(SSTATUS_FIG)
for _n, _p in SPOS.items():
    assert MPOS[_n] == _p, _n   # "The sstatus register is a subset of the mstatus register."
# The fields this machine implements. Every other field reads 0 (CHOICE,
# each is WARL or belongs to an extension that is absent).
WRITABLE = ["SIE", "MIE", "SPIE", "MPIE", "SPP", "MPP", "MPRV", "SUM", "MXR", "TVM", "TW", "TSR"]
FIXED = {"UXL": 2, "SXL": 2}    # read-only, XLEN 64 in S and U

ROWS = {
    "addi":  "imm[11:0] rs1 000 rd 0010011",
    "lb":    "imm[11:0] rs1 000 rd 0000011",
    "lh":    "imm[11:0] rs1 001 rd 0000011",
    "lw":    "imm[11:0] rs1 010 rd 0000011",
    "ld":    "imm[11:0] rs1 011 rd 0000011",
    "lbu":   "imm[11:0] rs1 100 rd 0000011",
    "sb":    "imm[11:5] rs2 rs1 000 imm[4:0] 0100011",
    "sh":    "imm[11:5] rs2 rs1 001 imm[4:0] 0100011",
    "sw":    "imm[11:5] rs2 rs1 010 imm[4:0] 0100011",
    "sd":    "imm[11:5] rs2 rs1 011 imm[4:0] 0100011",
    "jal":   "imm[20|10:1|11|19:12] rd 1101111",
    "csrrw": "csr rs1 001 rd 1110011",
    "csrrs": "csr rs1 010 rd 1110011",
    "csrrc": "csr rs1 011 rd 1110011",
    "ecall":  "000000000000 00000 000 00000 1110011",
    "ebreak": "000000000001 00000 000 00000 1110011",
    "sret":   "0001000 00010 00000 000 00000 1110011",
    "mret":   "0011000 00010 00000 000 00000 1110011",
    "wfi":    "0001000 00101 00000 000 00000 1110011",
    "sfence.vma": "0001001 rs2 rs1 000 00000 1110011",
    "lr.d":      "00010 0 0 00000 rs1 011 rd 0101111",
    "sc.d":      "00011 0 0 rs2 rs1 011 rd 0101111",
    "amoswap.d": "00001 0 0 rs2 rs1 011 rd 0101111",
}
CSR = {"sstatus": 0x100, "sie": 0x104, "stvec": 0x105, "scounteren": 0x106, "senvcfg": 0x10A,
       "sscratch": 0x140, "sepc": 0x141, "scause": 0x142, "stval": 0x143, "sip": 0x144,
       "satp": 0x180, "mstatus": 0x300, "misa": 0x301, "medeleg": 0x302, "mideleg": 0x303,
       "mie": 0x304, "mtvec": 0x305, "mcounteren": 0x306, "menvcfg": 0x30A, "mscratch": 0x340,
       "mepc": 0x341, "mcause": 0x342, "mtval": 0x343, "mip": 0x344, "mvendorid": 0xF11,
       "marchid": 0xF12, "mimpid": 0xF13, "mhartid": 0xF14, "mconfigptr": 0xF15}
CSR_NAME = dict((n, k) for k, n in CSR.items())


def bits(v, hi, lo):
    return (v >> lo) & ((1 << (hi - lo + 1)) - 1)


def encode(name, rd=0, rs1=0, rs2=0, imm=0, csr=0):
    word, imm = "", imm & M64
    for part in ROWS[name].split():
        if part == "rd":
            word += format(rd, "05b")
        elif part == "rs1":
            word += format(rs1, "05b")
        elif part == "rs2":
            word += format(rs2, "05b")
        elif part == "csr":
            word += format(csr, "012b")
        elif part.startswith("imm["):
            for piece in part[4:-1].split("|"):
                hi, lo = piece.split(":") if ":" in piece else (piece, piece)
                word += format(bits(imm, int(hi), int(lo)), "0%db" % (int(hi) - int(lo) + 1))
        else:
            word += part
    assert len(word) == 32, name
    return int(word, 2)


def decode(word):
    """(name, fields) of a word of ROWS, or (None, {})."""
    text = format(word, "032b")
    for name, row in ROWS.items():
        at, f, ok = 0, {"imm": 0}, True
        for part in row.split():
            if part in ("rd", "rs1", "rs2"):
                f[part] = int(text[at:at + 5], 2)
                at += 5
            elif part == "csr":
                f["csr"] = int(text[at:at + 12], 2)
                at += 12
            elif part.startswith("imm["):
                for piece in part[4:-1].split("|"):
                    hi, lo = [int(t) for t in ((piece.split(":")) if ":" in piece else (piece, piece))]
                    f["imm"] |= int(text[at:at + hi - lo + 1], 2) << lo
                    f["top"] = max(f.get("top", 0), hi)
                    at += hi - lo + 1
            else:
                ok = ok and text[at:at + len(part)] == part
                at += len(part)
        if ok:
            if "top" in f and (f["imm"] >> f["top"]) & 1:
                f["imm"] |= M64 & ~((1 << (f["top"] + 1)) - 1)     # sign extension
            return name, f
    return None, {}


class Trap(Exception):
    def __init__(self, cause, tval):
        self.cause, self.tval = cause, tval


class Machine:
    """One hart in the system profile. ram_len bytes of memory at RAM_BASE."""

    def __init__(self, ram_len):
        self.ram_len, self.mem = ram_len, {}
        self.x, self.pc, self.prv = [0] * 32, RAM_BASE, M
        self.st = dict((n, 0) for n in WRITABLE)
        self.st["MPP"] = M
        self.c = dict((n, 0) for n in ["medeleg", "mideleg", "mie", "mip", "mtvec", "stvec",
                                        "mscratch", "sscratch", "mepc", "sepc", "mcause", "scause",
                                        "mtval", "stval", "satp", "menvcfg", "senvcfg"])
        self.mtime, self.mtimecmp = 0, M64
        self.res = (0, 0)
        self.steps = self.traps = 0

    # ---- memory, physical ------------------------------------------------
    def in_ram(self, pa):
        return RAM_BASE <= pa < RAM_BASE + self.ram_len

    def peek(self, pa, n):
        return sum(self.mem.get(pa + k, 0) << (8 * k) for k in range(n))

    def poke(self, pa, n, v):
        for k in range(n):
            self.mem[pa + k] = (v >> (8 * k)) & 0xFF

    # ---- mstatus and its view --------------------------------------------
    def status(self, pos=MPOS):
        v = 0
        for name, (lo, width) in pos.items():
            v |= (self.st.get(name, FIXED.get(name, 0)) & ((1 << width) - 1)) << lo
        return v

    def set_status(self, v, pos):
        for name, (lo, width) in pos.items():
            if name not in WRITABLE:
                continue
            f = bits(v, lo + width - 1, lo)
            if name == "MPP" and f == 2:
                continue        # CHOICE: WARL, 2 names no mode, the field keeps its value
            self.st[name] = f

    def mip(self):
        # MTIP "is cleared by writing to the memory-mapped machine-mode timer
        # compare register": pending while mtime >= mtimecmp.
        return (self.c["mip"] & 0xA2A) | (0x80 if self.mtime >= self.mtimecmp else 0)
