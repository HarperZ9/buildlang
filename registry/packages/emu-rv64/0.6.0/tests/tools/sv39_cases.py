# sv39_cases.py: the generic hand cases of `emu-rv64 scases`, part 1: the
# case builder, the CSR cases, the privileged instructions and the status
# stack. A case is a list of 32 input numbers; svec.bld of the package
# explains them. The expected numbers come from sv39_step.run_generic.

from sv39_model import M64, U, S, M, RAM_BASE, CSR, encode
from sv39_step import Cpu, pte

B = RAM_BASE
ROOT, L1, L0, PA, PB = B + 0x1000, B + 0x2000, B + 0x3000, B + 0x4000, B + 0x5000
MTVEC, STVEC = B + 0x100, B + 0x200
VA = 0x40403000             # vpn[2] = 1, vpn[1] = 2, vpn[0] = 3
SATP = (8 << 60) | (ROOT >> 12)
PV, PR, PW, PX, PU, PG, PA_, PD = 1, 2, 4, 8, 16, 32, 64, 128
FULL = PV | PR | PW | PX | PA_ | PD
MODES = ((U, "U"), (S, "S"), (M, "M"))


def status(**fields):
    m = Cpu()
    m.st.update(fields)
    return m.status()


def case(note, words, steps=None, prv=M, st=None, satp=0, medeleg=0, mideleg=0, mie=0, mip=0,
         pc=B, code=B, x1=0, x2=0, pokes=(), stvec=STVEC, mtvec=MTVEC, sepc=0, mepc=0):
    words = list(words) + [0] * (4 - len(words))
    flat = []
    for addr, value in list(pokes) + [(0, 0)] * (6 - len(pokes)):
        flat += [addr, value]
    assert len(words) == 4 and len(flat) == 12, note
    i = words + [len([w for w in words if w]) if steps is None else steps, prv,
                 status(**(st or {})), satp, medeleg, mideleg, mie, mip, pc, code, x1, x2]
    return note, i + flat + [stvec, mtvec, sepc, mepc]


def rd(name):
    return encode("csrrs", rd=10, rs1=0, csr=CSR[name] if isinstance(name, str) else name)


def wr(name, kind="csrrw", rd_=0):
    return encode(kind, rd=rd_, rs1=1, csr=CSR[name] if isinstance(name, str) else name)


M_CSRS = ["mstatus", "misa", "medeleg", "mideleg", "mie", "mtvec", "mcounteren", "menvcfg",
          "mscratch", "mepc", "mcause", "mtval", "mip"]
S_CSRS = ["sstatus", "sie", "stvec", "scounteren", "senvcfg", "sscratch", "sepc", "scause",
          "stval", "sip", "satp"]
ALL_ST = dict(SIE=1, MIE=1, SPIE=1, MPIE=1, SPP=1, MPP=M, MPRV=1, SUM=1, MXR=1, TVM=1, TW=1, TSR=1)


def csr_warl_cases():
    out = []
    for name in M_CSRS + S_CSRS:
        for val, what in ((M64, "all ones"), (0, "zero")):
            out.append(case("M: write %s to %s, read it back" % (what, name),
                            [wr(name), rd(name)], x1=val, st=dict(MPP=S, SPIE=1), satp=SATP,
                            mideleg=0x222 if name in ("sie", "sip") else 0))
    for name in S_CSRS:
        if name == "satp":
            continue
        out.append(case("S: write all ones to %s, read it back" % name, [wr(name), rd(name)],
                        prv=S, x1=M64, mideleg=0x22 if name in ("sie", "sip") else 0))
    for n in (0xF11, 0xF12, 0xF13, 0xF14, 0xF15):
        out.append(case("M: read CSR %03x" % n, [rd(n)]))
        out.append(case("M: csrrw to read-only CSR %03x" % n, [wr(n)], x1=5))
    out.append(case("M: csrrs with x0 on read-only mvendorid is a read", [rd("mvendorid")]))
    out.append(case("M: csrrs with x1 = 0 on mvendorid counts as a write", [wr("mvendorid", "csrrs")]))
    out.append(case("M: csrrc with x1 = 0 on mhartid counts as a write", [wr("mhartid", "csrrc")]))
    return out


def csr_status_cases():
    out = []
    for old in (U, S, M):
        out.append(case("M: mstatus write of MPP = 2 keeps MPP = %d" % old, [wr("mstatus"), rd("mstatus")],
                        x1=0x1000, st=dict(MPP=old)))
        out.append(case("M: mstatus write of MPP = 1 over MPP = %d" % old, [wr("mstatus"), rd("mstatus")],
                        x1=0x800, st=dict(MPP=old)))
    out.append(case("S: sstatus read shows only the supervisor fields", [rd("sstatus")], prv=S, st=ALL_ST))
    out.append(case("S: sstatus read of the reset state", [rd("sstatus")], prv=S))
    out.append(case("S: csrrc of all ones on sstatus clears only its own fields",
                    [wr("sstatus", "csrrc", 10)], prv=S, x1=M64, st=ALL_ST))
    out.append(case("S: csrrs of all ones on sstatus sets only its own fields",
                    [wr("sstatus", "csrrs", 10)], prv=S, x1=M64, st=dict(MPP=U)))
    out.append(case("M: sstatus is readable in M-mode", [rd("sstatus")], st=ALL_ST))
    for bit, name in ((1, "SIE"), (5, "SPIE"), (8, "SPP"), (18, "SUM"), (19, "MXR")):
        out.append(case("S: sstatus write of bit %d alone is %s in mstatus" % (bit, name),
                        [wr("sstatus")], prv=S, x1=1 << bit, st=dict(MPP=S, MPIE=1, TW=1)))
    for bit, name in ((3, "MIE"), (7, "MPIE"), (11, "MPP"), (17, "MPRV"), (20, "TVM"), (21, "TW"), (22, "TSR")):
        out.append(case("S: sstatus write of bit %d (%s) changes nothing" % (bit, name),
                        [wr("sstatus"), rd("sstatus")], prv=S, x1=1 << bit, st=dict(MPP=U)))
    return out


def csr_irq_cases():
    out = []
    for dl in (0, 0x2, 0x20, 0x200, 0x222):
        out.append(case("S: sip read, mideleg = %03x, every bit pending" % dl, [rd("sip")],
                        prv=S, mideleg=dl, mip=0xA2A))
        out.append(case("S: sie read, mideleg = %03x, every bit enabled" % dl, [rd("sie")],
                        prv=S, mideleg=dl, mie=0xAAA))
        out.append(case("S: sie write of all ones, mideleg = %03x" % dl, [wr("sie")],
                        prv=S, mideleg=dl, x1=M64, mie=0x888))
        out.append(case("S: sie write of zero, mideleg = %03x" % dl, [wr("sie")],
                        prv=S, mideleg=dl, x1=0, mie=0xAAA))
        out.append(case("S: sip write of all ones, mideleg = %03x" % dl, [wr("sip")],
                        prv=S, mideleg=dl, x1=M64))
        out.append(case("S: sip write of zero, mideleg = %03x, every bit pending" % dl, [wr("sip")],
                        prv=S, mideleg=dl, x1=0, mip=0xA2A))
    out.append(case("M: mip write of all ones keeps the host bits", [wr("mip"), rd("mip")], x1=M64, mip=0x808))
    out.append(case("M: mip write of zero clears SSIP, STIP, SEIP only", [wr("mip"), rd("mip")], x1=0, mip=0xA2A))
    out.append(case("M: mip shows MTIP from the timer", [rd("mip")], mip=0x80))
    out.append(case("M: mip write cannot clear MTIP", [wr("mip"), rd("mip")], x1=0, mip=0x80))
    out.append(case("M: mip STIP set by csrrs", [wr("mip", "csrrs"), rd("mip")], x1=0x20))
    out.append(case("M: mie write of all ones", [wr("mie"), rd("mie")], x1=M64))
    return out


def csr_satp_cases():
    out = []
    for val, what in (((8 << 60) | (0xFFFF << 44) | 0x80001, "MODE 8 with every ASID bit"),
                      ((8 << 60) | ((1 << 44) - 1), "MODE 8 with every PPN bit"),
                      ((9 << 60) | 0x80002, "MODE 9 (Sv48)"), ((10 << 60) | 0x80002, "MODE 10 (Sv57)"),
                      ((1 << 60) | 0x80002, "MODE 1"), ((15 << 60) | 0x80002, "MODE 15"),
                      (0x80002, "MODE 0 with a PPN"), (0, "zero")):
        for start in (0, SATP):
            out.append(case("M: satp write of %s over %x" % (what, start), [wr("satp"), rd("satp")],
                            x1=val, satp=start))
    for tvm in (0, 1):
        out.append(case("S: satp read with TVM = %d" % tvm, [rd("satp")], prv=S, st=dict(TVM=tvm)))
        out.append(case("S: satp write with TVM = %d" % tvm, [wr("satp")], prv=S, st=dict(TVM=tvm), x1=0))
        out.append(case("M: satp read with TVM = %d" % tvm, [rd("satp")], st=dict(TVM=tvm), satp=SATP))
    return out


def csr_priv_cases():
    out = []
    for prv, pn in MODES:
        for name in ("sstatus", "sie", "sip", "satp", "sscratch", "mstatus", "medeleg", "mip", "misa", "mhartid"):
            out.append(case("%s: read of %s" % (pn, name), [rd(name)], prv=prv))
        out.append(case("%s: write of sscratch" % pn, [wr("sscratch")], prv=prv, x1=7))
        out.append(case("%s: write of mscratch" % pn, [wr("mscratch")], prv=prv, x1=7))
        for n in (0xC00, 0xC01, 0xC02, 0x7A0, 0x200, 0x5A8, 0x000, 0xB00, 0x10B, 0x181):
            out.append(case("%s: read of CSR %03x, which does not exist" % (pn, n), [rd(n)], prv=prv))
    return out


def priv_insn_cases():
    out = []
    ins = (("sret", "TSR"), ("mret", "TSR"), ("wfi", "TW"), ("sfence.vma", "TVM"))
    for prv, pn in MODES:
        for name, bit in ins:
            for on in (0, 1):
                out.append(case("%s: %s with %s = %d" % (pn, name, bit, on), [encode(name, rs1=1, rs2=2)],
                                prv=prv, st={bit: on, "MPP": S, "SPP": 0}, sepc=B + 0x40, mepc=B + 0x80))
        out.append(case("%s: ecall" % pn, [encode("ecall")], prv=prv))
        out.append(case("%s: ebreak" % pn, [encode("ebreak")], prv=prv))
        out.append(case("%s: sfence.vma x0, x0" % pn, [encode("sfence.vma")], prv=prv))
    return out


def stack_cases():
    out = []
    keep = dict(SUM=1, MXR=1, TW=1)
    for prv, pn in MODES:
        for mie in (0, 1):
            for old in ((U, 0), (M, 1)):
                st = dict(keep, MIE=mie, MPP=old[0], MPIE=old[1], SIE=1, SPP=1)
                out.append(case("trap to M from %s, MIE = %d, old MPP = %d, old MPIE = %d" % (pn, mie, old[0], old[1]),
                                [encode("ecall")], prv=prv, st=st))
    for prv, pn in MODES[:2]:
        for sie in (0, 1):
            for old in ((0, 0), (1, 1)):
                st = dict(keep, SIE=sie, SPP=old[0], SPIE=old[1], MIE=1, MPP=S)
                out.append(case("trap to S from %s, SIE = %d, old SPP = %d, old SPIE = %d" % (pn, sie, old[0], old[1]),
                                [encode("ecall")], prv=prv, st=st, medeleg=0x300))
    for mpp in (U, S, M):
        for mpie in (0, 1):
            for mprv in (0, 1):
                st = dict(keep, MPP=mpp, MPIE=mpie, MIE=1 - mpie, MPRV=mprv, SIE=1, SPP=1)
                out.append(case("mret with MPP = %d, MPIE = %d, MPRV = %d" % (mpp, mpie, mprv),
                                [encode("mret")], st=st, mepc=B + 0x80))
    for prv, pn in MODES[1:]:
        for spp in (0, 1):
            for spie in (0, 1):
                for mprv in (0, 1):
                    st = dict(keep, SPP=spp, SPIE=spie, SIE=1 - spie, MPRV=mprv, MIE=1, MPP=M)
                    out.append(case("sret in %s with SPP = %d, SPIE = %d, MPRV = %d" % (pn, spp, spie, mprv),
                                    [encode("sret")], prv=prv, st=st, sepc=B + 0x40))
    return out


def part1():
    return (csr_warl_cases() + csr_status_cases() + csr_irq_cases() + csr_satp_cases()
            + csr_priv_cases() + priv_insn_cases() + stack_cases())
