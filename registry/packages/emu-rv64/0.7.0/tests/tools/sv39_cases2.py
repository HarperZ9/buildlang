# sv39_cases2.py: the generic hand cases of `emu-rv64 scases`, part 2: the
# delegation matrix for exceptions and interrupts, priority and vectoring.

from sv39_model import U, S, M, encode
from sv39_step import pte, RAM_LEN
from sv39_cases import (B, ROOT, PA, VA, SATP, FULL, PU, MODES, MTVEC, STVEC, case)

ADDI = encode("addi", rd=10, rs1=0, imm=5)
LD = encode("ld", rd=10, rs1=1)
SD = encode("sd", rs1=1, rs2=2)


def code_map(prv):
    """A gigapage that maps the code where it lies, for user or supervisor."""
    return (ROOT + 16, pte(B, FULL | (PU if prv == U else 0)))


def provoke(cause, prv):
    """Keyword arguments of a case in which mode prv raises exception `cause`,
    or None when that mode cannot raise it."""
    paged = dict(satp=SATP, pokes=[code_map(prv)])
    if cause in (8, 9, 11):
        return dict(words=[encode("ecall")]) if cause == {U: 8, S: 9, M: 11}[prv] else None
    if cause in (12, 13, 15) and prv == M:
        if cause == 12:
            return None
        return dict(words=[LD if cause == 13 else SD], x1=VA, st=dict(MPRV=1, MPP=S), **paged)
    table = {
        0: dict(words=[encode("jal", rd=0, imm=2)]),
        1: dict(words=[ADDI], pc=B + RAM_LEN),
        2: dict(words=[0], steps=1),
        3: dict(words=[encode("ebreak")]),
        4: dict(words=[encode("lr.d", rd=10, rs1=1)], x1=PA + 1),
        5: dict(words=[LD], x1=0x1000),
        6: dict(words=[encode("sc.d", rd=10, rs1=1, rs2=2)], x1=PA + 4),
        7: dict(words=[SD], x1=0x1000),
        12: dict(words=[ADDI], pc=VA, **paged),
        13: dict(words=[LD], x1=VA, **paged),
        15: dict(words=[SD], x1=VA, **paged),
    }
    return table[cause]


def exception_cases():
    out = []
    for cause in (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 15):
        for prv, pn in MODES:
            kw = provoke(cause, prv)
            if kw is None:
                continue
            bit = (1 << cause) & 0xB3FF
            for dl, what in ((0, "not delegated"), (bit, "delegated"), (0xB3FF & ~bit, "every other cause delegated")):
                if cause == 11 and dl == 0 and what == "delegated":
                    continue        # medeleg[11] is read-only zero
                kw2 = dict(kw)
                st = dict(kw2.pop("st", {}), SIE=1, MIE=1)
                out.append(case("cause %d in %s, %s" % (cause, pn, what), prv=prv, medeleg=dl, st=st, **kw2))
    return out


def interrupt_cases():
    out = []
    names = {1: "SSI", 5: "STI", 9: "SEI", 3: "MSI", 7: "MTI", 11: "MEI"}
    for irq in (1, 5, 9, 3, 7, 11):
        delegs = (0, 1 << irq) if irq in (1, 5, 9) else (0, 0x222)
        for prv, pn in MODES:
            for dl in delegs:
                for mie in (0, 1):
                    for sie in (0, 1):
                        out.append(case("%s pending in %s, mideleg = %03x, MIE = %d, SIE = %d" % (names[irq], pn, dl, mie, sie),
                                        [ADDI], prv=prv, mideleg=dl, mie=1 << irq, mip=1 << irq,
                                        st=dict(MIE=mie, SIE=sie, MPP=S)))
        out.append(case("%s pending and not enabled in mie, in U" % names[irq], [ADDI], prv=U,
                        mip=1 << irq, mie=0xAAA & ~(1 << irq), st=dict(MIE=1, SIE=1)))
        out.append(case("%s enabled and not pending, in U" % names[irq], [ADDI], prv=U,
                        mie=1 << irq, mip=0xA2A & ~(1 << irq), st=dict(MIE=1, SIE=1)))
    return out


def priority_cases():
    out = []
    on = dict(MIE=1, SIE=1)
    for pend in (0xAAA, 0x2AA, 0x2A2, 0x222, 0x22, 0x20, 0x880, 0x820, 0x28):
        for dl in (0, 0x222, 0x200, 0x22):
            for prv, pn in ((U, "U"), (S, "S")):
                out.append(case("pending %03x in %s, mideleg = %03x" % (pend, pn, dl), [ADDI], prv=prv,
                                mideleg=dl, mie=0xAAA, mip=pend, st=on))
    out.append(case("pending aaa in M, mideleg = 222: the machine interrupts only", [ADDI], mideleg=0x222,
                    mie=0xAAA, mip=0xAAA, st=on))
    out.append(case("pending 222 in M, mideleg = 222: none is taken", [ADDI], mideleg=0x222,
                    mie=0xAAA, mip=0x222, st=on))
    out.append(case("pending 222 in M, mideleg = 0: SEI goes to M", [ADDI], mie=0xAAA, mip=0x222, st=on))
    return out


def vector_cases():
    out = []
    on = dict(MIE=1, SIE=1)
    for irq in (1, 5, 9, 7):
        for dl in (0, 0x222):
            out.append(case("vectored: interrupt %d in U, mideleg = %03x" % (irq, dl), [ADDI], prv=U,
                            mideleg=dl, mie=1 << irq, mip=1 << irq, st=on, mtvec=MTVEC | 1, stvec=STVEC | 1))
    for dl in (0, 0x100):
        out.append(case("vectored: an exception goes to BASE, medeleg = %03x" % dl, [encode("ecall")], prv=U,
                        medeleg=dl, st=on, mtvec=MTVEC | 1, stvec=STVEC | 1))
    return out


def sequence_cases():
    out = []
    out.append(case("mret to S, sret to U, ecall delegated to S",
                    [encode("mret"), encode("sret"), encode("ecall")], st=dict(MPP=S, SPP=0),
                    medeleg=0x100, mepc=B + 4, sepc=B + 8))
    out.append(case("mret to U, then ecall goes to M",
                    [encode("mret"), encode("ecall")], st=dict(MPP=U), mepc=B + 4))
    out.append(case("S: ecall delegated to S",
                    [encode("ecall"), ADDI], steps=1, prv=S, medeleg=0x200))
    out.append(case("M: a delegated illegal word in M stays in M", [0], steps=1, medeleg=0xB3FF))
    out.append(case("S: csrrw to sepc, then sret goes there",
                    [encode("csrrw", rs1=1, csr=0x141), encode("sret"), 0, ADDI], steps=3, prv=S,
                    x1=B + 12, st=dict(SPP=1)))
    out.append(case("M: STIP set through mip is taken in S after mret",
                    [encode("csrrs", rs1=1, csr=0x344), encode("mret"), ADDI], steps=3, x1=0x20,
                    mideleg=0x20, mie=0x20, st=dict(MPP=S, SIE=1), mepc=B + 8))
    out.append(case("S: wfi with a pending delegated interrupt takes the interrupt",
                    [encode("wfi")], prv=S, mideleg=0x2, mie=0x2, mip=0x2, st=dict(SIE=1)))
    return out


def part2():
    return exception_cases() + interrupt_cases() + priority_cases() + vector_cases() + sequence_cases()
