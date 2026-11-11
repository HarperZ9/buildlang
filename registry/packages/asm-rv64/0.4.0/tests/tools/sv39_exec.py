# sv39_exec.py: the rules of the oracle machine. See sv39_model.py for the
# sources and for what CHOICE marks.

from sv39_model import (M64, U, S, M, FETCH, LOAD, STORE, RAM_BASE, MTIMECMP, MTIME, UART,
                        MPOS, SPOS, CSR, CSR_NAME, bits, decode, Trap, Machine)

PAGE_FAULT = {FETCH: 12, LOAD: 13, STORE: 15}
ACCESS_FAULT = {FETCH: 1, LOAD: 5, STORE: 7}
# medeleg: one bit for each exception this machine can raise below M-mode.
# "medeleg[11] is read-only zero."
DELEGABLE = sum(1 << k for k in (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 15))
S_IRQS = (1 << 9) | (1 << 5) | (1 << 1)     # SEIP, STIP, SSIP (mipreg-standard.edn)
ALL_IRQS = S_IRQS | (1 << 11) | (1 << 7) | (1 << 3)


class Hart(Machine):
    # ---- control and status registers ------------------------------------
    def csr_allowed(self, n, writes):
        if n not in CSR_NAME:
            return False        # CHOICE: an access to a CSR that does not exist is illegal
        if bits(n, 9, 8) > self.prv:
            return False        # "csr[9:8] encode the lowest privilege level that can access"
        if bits(n, 11, 10) == 3 and writes:
            return False        # "read-only (11)"
        if n == CSR["satp"] and self.prv == S and self.st["TVM"]:
            return False        # "When TVM=1, attempts to read or write the satp CSR ..."
        return True

    def csr_read(self, n):
        name = CSR_NAME[n]
        if name == "mstatus":
            return self.status(MPOS)
        if name == "sstatus":
            return self.status(SPOS)
        if name == "misa":
            return (2 << 62) | sum(1 << (ord(ch) - ord("A")) for ch in "AIMSU")
        if name == "mip":
            return self.mip()
        if name == "sip":
            return self.mip() & self.c["mideleg"]
        if name == "sie":
            return self.c["mie"] & self.c["mideleg"]
        if name in ("mcounteren", "scounteren") or n >= 0xF11:
            return 0            # "any of the bits may be read-only zero"
        return self.c[name]

    def csr_write(self, n, v):
        name, c = CSR_NAME[n], self.c
        if name == "mstatus":
            self.set_status(v, MPOS)
        elif name == "sstatus":
            self.set_status(v, SPOS)
        elif name == "mie":
            c["mie"] = v & ALL_IRQS
        elif name == "sie":
            c["mie"] = (c["mie"] & ~c["mideleg"]) | (v & c["mideleg"])
        elif name == "mip":
            c["mip"] = (c["mip"] & ~S_IRQS) | (v & S_IRQS)    # SEIP, STIP, SSIP "writable in mip"
        elif name == "sip":
            w = c["mideleg"] & 2                              # only SSIP "is writable in sip"
            c["mip"] = (c["mip"] & ~w) | (v & w)
        elif name == "medeleg":
            c["medeleg"] = v & DELEGABLE
        elif name == "mideleg":
            c["mideleg"] = v & S_IRQS       # CHOICE: machine interrupts cannot be delegated
        elif name in ("mtvec", "stvec"):
            c[name] = v & ~2 & M64          # MODE 0 and 1 only
        elif name in ("mepc", "sepc"):
            c[name] = v & ~3 & M64          # IALIGN=32: "the two low bits are always zero"
        elif name == "satp":
            mode = v >> 60
            if mode == 0:
                c["satp"] = 0               # CHOICE: Bare stores zero in every field
            elif mode == 8:
                c["satp"] = (8 << 60) | bits(v, 43, 0)      # CHOICE: ASIDLEN = 0
            # any other MODE: "the entire write has no effect"
        elif name in ("menvcfg", "senvcfg"):
            c[name] = v & 1                 # FIOM
        elif name in ("misa", "mcounteren", "scounteren"):
            pass
        else:
            c[name] = v & M64

    # ---- traps -------------------------------------------------------------
    def take(self, cause, tval, irq=False):
        st, c = self.st, self.c
        deleg = c["mideleg"] if irq else c["medeleg"]
        code = cause | ((1 << 63) if irq else 0)
        if self.prv != M and (deleg >> cause) & 1:
            c["sepc"], c["scause"], c["stval"] = self.pc & ~3, code, tval
            st["SPP"], st["SPIE"], st["SIE"] = (0 if self.prv == U else 1), st["SIE"], 0
            self.prv, vec = S, c["stvec"]
        else:
            c["mepc"], c["mcause"], c["mtval"] = self.pc & ~3, code, tval
            st["MPP"], st["MPIE"], st["MIE"] = self.prv, st["MIE"], 0
            self.prv, vec = M, c["mtvec"]
        self.pc = ((vec & ~3) + (4 * cause if irq and vec & 1 else 0)) & M64
        self.traps += 1

    def xret(self, x):
        st = self.st
        y = st[x + "PP"]
        st[x + "IE"], st[x + "PIE"], st[x + "PP"] = st[x + "PIE"], 1, U
        if y != M:
            st["MPRV"] = 0
        self.prv, self.pc = y, self.c[x.lower() + "epc"]

    def pending_irq(self):
        ready, dl = self.mip() & self.c["mie"], self.c["mideleg"]
        for i in (11, 3, 7, 9, 1, 5):       # "MEI, MSI, MTI, SEI, SSI, STI"
            if (ready >> i) & 1 and not (dl >> i) & 1 and (self.prv < M or self.st["MIE"]):
                return i
        for i in (9, 1, 5):                 # "SEI, SSI, STI"
            if (ready >> i) & 1 and (dl >> i) & 1 and (self.prv < S or (self.prv == S and self.st["SIE"])):
                return i
        return 0

    # ---- address translation, steps 1 to 10 of the manual ------------------
    def translate(self, va, acc):
        prv = self.st["MPP"] if (self.st["MPRV"] and acc != FETCH) else self.prv
        satp = self.c["satp"]
        if prv == M or satp >> 60 != 8:
            return va
        pf, af = Trap(PAGE_FAULT[acc], va), Trap(ACCESS_FAULT[acc], va)
        if any(bits(va, k, k) != bits(va, 38, 38) for k in range(39, 64)):
            raise pf
        vpn = [bits(va, 20, 12), bits(va, 29, 21), bits(va, 38, 30)]
        a, i = bits(satp, 43, 0) * 4096, 2
        while True:
            at = a + vpn[i] * 8
            if not all(self.in_ram(at + k) for k in range(8)):
                raise af
            pte = self.peek(at, 8)
            v, r, w, x, u, _g, acc_bit, dirty = [bits(pte, k, k) for k in range(8)]
            if v == 0 or (r == 0 and w == 1) or bits(pte, 63, 54):
                raise pf
            if r or x:
                break
            if dirty or acc_bit or u:
                raise pf            # "reserved for future standard use" in a pointer PTE
            i -= 1
            if i < 0:
                raise pf
            a = bits(pte, 53, 10) * 4096
        ppn = [bits(pte, 18, 10), bits(pte, 27, 19), bits(pte, 53, 28)]
        if any(ppn[k] for k in range(i)):
            raise pf                # misaligned superpage
        if prv == U and not u:
            raise pf
        if prv == S and u and (acc == FETCH or not self.st["SUM"]):
            raise pf
        if acc == FETCH and not x:
            raise pf
        if acc == LOAD and not (r or (x and self.st["MXR"])):
            raise pf
        if acc == STORE and not w:
            raise pf
        if not acc_bit or (acc == STORE and not dirty):
            raise pf                # Svade: the machine never writes a PTE
        for k in range(i):
            ppn[k] = vpn[k]
        return (ppn[2] << 30) | (ppn[1] << 21) | (ppn[0] << 12) | bits(va, 11, 0)

    # ---- explicit memory access, one byte at a time -------------------------
    def access(self, va, n, acc):
        pas = [self.translate((va + k) & M64, acc) for k in range(n)]
        if n == 1 and UART <= pas[0] < UART + 8:
            return pas
        run = pas == list(range(pas[0], pas[0] + n))
        for base in (MTIMECMP, MTIME):
            if run and base <= pas[0] and pas[-1] < base + 8:
                return pas
        for k, p in enumerate(pas):
            if not self.in_ram(p):
                raise Trap(ACCESS_FAULT[acc], (va + k) & M64)
        return pas

    def read(self, pas):
        v = 0
        for k, p in enumerate(pas):
            if UART <= p < UART + 8:
                b = 0x60 if p == UART + 5 else 0
            elif MTIMECMP <= p < MTIMECMP + 8:
                b = bits(self.mtimecmp, 8 * (p - MTIMECMP) + 7, 8 * (p - MTIMECMP))
            elif MTIME <= p < MTIME + 8:
                b = bits(self.mtime, 8 * (p - MTIME) + 7, 8 * (p - MTIME))
            else:
                b = self.mem.get(p, 0)
            v |= b << (8 * k)
        return v

    def write(self, pas, v):
        for k, p in enumerate(pas):
            b = (v >> (8 * k)) & 0xFF
            if UART <= p < UART + 8:
                if p == UART:
                    self.out.append(b)
            elif MTIMECMP <= p < MTIMECMP + 8:
                sh = 8 * (p - MTIMECMP)
                self.mtimecmp = (self.mtimecmp & ~(0xFF << sh)) | (b << sh)
            elif MTIME <= p < MTIME + 8:
                sh = 8 * (p - MTIME)
                self.mtime = (self.mtime & ~(0xFF << sh)) | (b << sh)
            else:
                self.mem[p] = b
