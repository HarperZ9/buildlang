# sv39_step.py: instructions and the step of the oracle machine, and the two
# case runners. See sv39_model.py for the sources and for what CHOICE marks.

from sv39_model import (M64, U, S, M, FETCH, LOAD, STORE, MPOS, bits, encode, decode, Trap)
from sv39_exec import Hart

RAM_LEN = 32768             # the hand cases run in eight pages of memory
LOADS = {"lb": (1, True), "lh": (2, True), "lw": (4, True), "ld": (8, False), "lbu": (1, False)}
STORES = {"sb": 1, "sh": 2, "sw": 4, "sd": 8}


class Cpu(Hart):
    def __init__(self, ram_len=RAM_LEN):
        Hart.__init__(self, ram_len)
        self.out = []

    def atomic(self, name, f):
        va = self.x[f["rs1"]]
        acc = LOAD if name == "lr.d" else STORE     # "AMOs never raise load page-fault exceptions"
        if va & 7:
            raise Trap(4 if acc == LOAD else 6, va)     # CHOICE: misaligned before the rest
        pas = self.access(va, 8, acc)
        old, val = self.read(pas), 0
        if name == "lr.d":
            self.res, val = (8, pas[0]), old        # CHOICE: the reservation is physical
        elif name == "sc.d":
            if self.res == (8, pas[0]):
                self.write(pas, self.x[f["rs2"]])
            else:
                val = 1
            self.res = (0, 0)
        else:
            self.write(pas, self.x[f["rs2"]])
            val = old
        self.x[f["rd"]] = val

    def system(self, name, f, word):
        ill = Trap(2, word)
        if name == "ecall":
            raise Trap({U: 8, S: 9, M: 11}[self.prv], 0)
        if name == "ebreak":
            raise Trap(3, 0)
        if name == "mret":
            if self.prv != M:
                raise ill
            self.xret("M")
            return False
        if name == "sret":
            if self.prv == U or (self.prv == S and self.st["TSR"]):
                raise ill
            self.xret("S")
            return False
        if name == "wfi" and self.prv != M and self.st["TW"]:
            raise ill               # CHOICE: "may have WFI always raise" with TW=1
        if name == "sfence.vma" and (self.prv == U or (self.prv == S and self.st["TVM"])):
            raise ill
        return True                 # WFI and SFENCE.VMA retire as no-ops

    def csr_op(self, name, f, word):
        writes = name == "csrrw" or f["rs1"] != 0
        if not self.csr_allowed(f["csr"], writes):
            raise Trap(2, word)
        old, src = self.csr_read(f["csr"]), self.x[f["rs1"]]
        if writes:
            self.csr_write(f["csr"], {"csrrw": src, "csrrs": old | src, "csrrc": old & ~src}[name])
        self.x[f["rd"]] = old

    def execute(self):
        if self.pc & 3:
            raise Trap(0, self.pc)
        pa = self.translate(self.pc, FETCH)
        if not all(self.in_ram(pa + k) for k in range(4)):
            raise Trap(1, self.pc)
        word = self.peek(pa, 4)
        name, f = decode(word)
        nxt = (self.pc + 4) & M64
        if name is None:
            raise Trap(2, word)
        if name == "addi":
            self.x[f["rd"]] = (self.x[f["rs1"]] + f["imm"]) & M64
        elif name == "jal":
            target = (self.pc + f["imm"]) & M64
            if target & 3:
                raise Trap(0, target)
            self.x[f["rd"]], nxt = nxt, target
        elif name in LOADS:
            n, signed = LOADS[name]
            v = self.read(self.access((self.x[f["rs1"]] + f["imm"]) & M64, n, LOAD))
            if signed and v >> (8 * n - 1):
                v |= M64 & ~((1 << (8 * n)) - 1)
            self.x[f["rd"]] = v
        elif name in STORES:
            pas = self.access((self.x[f["rs1"]] + f["imm"]) & M64, STORES[name], STORE)
            self.write(pas, self.x[f["rs2"]])
        elif name in ("lr.d", "sc.d", "amoswap.d"):
            self.atomic(name, f)
        elif name.startswith("csrr"):
            self.csr_op(name, f, word)
        elif not self.system(name, f, word):
            nxt = self.pc
        self.x[0], self.pc = 0, nxt
        self.steps += 1

    def step(self):
        irq = self.pending_irq()
        rc = 3
        if irq:
            self.take(irq, 0, True)
        else:
            try:
                self.execute()
                rc = 0
            except Trap as t:
                self.take(t.cause, t.tval)
        self.mtime = (self.mtime + 1) & M64     # CHOICE: the tick ends the step
        return rc


def run_generic(i):
    """The 22 expected numbers of a generic case with the 32 input numbers i."""
    m = Cpu()
    for k in range(4):
        m.poke(i[13] + 4 * k, 4, i[k])
    m.prv, m.pc = i[5], i[12]
    m.set_status(i[6], MPOS)
    c = m.c
    c["satp"], c["medeleg"], c["mideleg"], c["mie"], c["mip"] = i[7], i[8], i[9], i[10], i[11] & ~0x80
    if i[11] & 0x80:
        m.mtimecmp = 0
    m.x[1], m.x[2] = i[14], i[15]
    for k in range(6):
        if i[16 + 2 * k]:
            m.poke(i[16 + 2 * k], 8, i[17 + 2 * k])
    c["stvec"], c["mtvec"], c["sepc"], c["mepc"] = i[28:32]
    assert m.status() == i[6], "case starts from an illegal mstatus"
    rc, n = 0, 0
    while n < i[4] and rc == 0:
        rc, n = m.step(), n + 1
    probe = m.peek(i[26], 8) if i[26] else 0
    return [rc, m.pc, m.prv, m.status(), c["mcause"], c["mepc"], c["mtval"], c["scause"],
            c["sepc"], c["stval"], m.x[10], m.x[11], m.steps, m.traps, c["satp"], probe,
            c["mip"], c["mie"], m.res[0], m.res[1], c["medeleg"], c["mideleg"]]


# The permission matrix. One row is one access to the page at MX_VA, whose
# leaf PTE has the flags of the row, by a machine in the mode of the row.
MX_VA, MX_PAGE, MX_ROOT, MX_L1, MX_L0 = 0x40403000, 0x80004000, 0x80001000, 0x80002000, 0x80003000
MX_ADDI = encode("addi", rd=10, rs1=0, imm=77)
MX_STORED = 0xA5A5A5A55A5A5A5A


def pte(target, flags):
    return ((target >> 12) << 10) | flags


def matrix_row(flags, prv, mprv, mpp, sum_, mxr, acc):
    """The packed row: inputs in bits 0 to 16, the expected cause in bits 24 to 31
    (0 when the access succeeds)."""
    m = Cpu()
    code_flags = 0xCF | (0x10 if prv == U else 0)          # V R W X A D, and U for user code
    m.poke(MX_ROOT + 8 * 2, 8, pte(0x80000000, code_flags))  # a gigapage: virtual = physical
    m.poke(MX_ROOT + 8 * 1, 8, pte(MX_L1, 1))
    m.poke(MX_L1 + 8 * 2, 8, pte(MX_L0, 1))
    m.poke(MX_L0 + 8 * 3, 8, pte(MX_PAGE, flags))
    m.poke(MX_PAGE, 8, MX_ADDI)
    word = {FETCH: MX_ADDI, LOAD: encode("ld", rd=10, rs1=1), STORE: encode("sd", rs1=1, rs2=2)}[acc]
    m.poke(0x80000000, 4, word)
    m.prv, m.pc = prv, (MX_VA if acc == FETCH else 0x80000000)
    m.st.update(MPRV=mprv, MPP=mpp, SUM=sum_, MXR=mxr)
    m.c["satp"], m.c["mtvec"] = (8 << 60) | (MX_ROOT >> 12), 0x80000100
    m.x[1], m.x[2] = MX_VA, MX_STORED
    rc = m.step()
    cause = m.c["mcause"] if rc == 3 else 0
    if rc == 0:
        want = {FETCH: (m.x[10] == 77), LOAD: (m.x[10] == MX_ADDI),
                STORE: (m.peek(MX_PAGE, 8) == MX_STORED)}[acc]
        assert want and m.prv == prv
    else:
        assert cause in (1, 5, 7, 12, 13, 15) and m.peek(MX_PAGE, 8) == MX_ADDI and m.x[10] == 0
        assert m.c["mtval"] == MX_VA and m.prv == M
    return flags | (prv << 8) | (mprv << 10) | (mpp << 11) | (sum_ << 13) | (mxr << 14) | (acc << 15) | (cause << 24)
