# sv39_cases3.py: the generic hand cases of `emu-rv64 scases`, part 3: the
# page walk, accesses that cross a page, and the atomic instructions.

from sv39_model import U, S, M, encode
from sv39_step import pte, RAM_LEN
from sv39_cases import (B, ROOT, L1, L0, PA, PB, VA, SATP, FULL, PV, PR, PW, PX, PU, PG, PA_, PD, case)
from sv39_cases2 import ADDI, LD, SD, code_map

DATA = 0x8877665544332211
NEW = 0x1111222233334444
FAR = 0x90000000            # a physical address outside memory
KINDS = ("load", "store", "fetch")


def chain(leaf, root_ptr=None, l1_ptr=None, va=VA):
    """The three PTE writes that map va: root pointer, level-1 pointer, leaf."""
    i2, i1, i0 = (va >> 30) & 511, (va >> 21) & 511, (va >> 12) & 511
    return [(ROOT + 8 * i2, pte(L1, PV) if root_ptr is None else root_ptr),
            (L1 + 8 * i1, pte(L0, PV) if l1_ptr is None else l1_ptr),
            (L0 + 8 * i0, leaf)]


def walk(note, kind, tables, va=VA, prv=S, st=None, target=PA, word=None, data=DATA, satp=SATP):
    """One access of `kind` at va through `tables`. The last write holds the
    data, so the case reports the doubleword there after the run."""
    pokes = ([code_map(prv)] if prv != M else []) + list(tables)
    pokes += [(0, 0)] * (5 - len(pokes))
    assert len(pokes) == 5, note
    if kind == "fetch":
        return case(note + ", fetch", [ADDI], prv=prv, st=st, satp=satp, pc=va, code=target, pokes=pokes)
    w = word or (LD if kind == "load" else SD)
    return case(note + ", " + kind, [w], prv=prv, st=st, satp=satp, x1=va, x2=NEW,
                pokes=pokes + [(target, data)])


def far_root(kind):
    """The root table outside memory. A load or store is made by M-mode with
    MPRV, so that the fetch of the instruction itself needs no table."""
    satp = (8 << 60) | ((B + RAM_LEN) >> 12)
    if kind == "fetch":
        return walk("root table outside memory", kind, [], satp=satp)
    return walk("root table outside memory", kind, [], prv=M, st=dict(MPRV=1, MPP=S), satp=satp)


def shape_cases():
    out = []
    for kind in KINDS:
        out.append(walk("4 KiB page", kind, chain(pte(PA, FULL))))
        for va, what in (((1 << 39) | VA, "bit 39 set over bit 38 clear"),
                         (0xFFFFFF8000000000 | VA, "bits 63 to 39 set over bit 38 clear"),
                         (0x7FFFFFC000000000 | VA, "bit 63 clear over bit 38 set"),
                         ((1 << 63) | VA, "bit 63 alone set")):
            out.append(walk("not canonical, %s" % what, kind, chain(pte(PA, FULL)), va=va))
        hi = 0xFFFFFFC000000000 | VA
        out.append(walk("canonical, bits 63 to 38 set", kind, chain(pte(PA, FULL), va=hi), va=hi))
        out.append(walk("megapage", kind, chain(0, l1_ptr=pte(B, FULL))[:2], va=0x40404000))
        out.append(walk("megapage, PPN[0] not zero", kind, chain(0, l1_ptr=pte(PA, FULL))[:2], va=0x40404000))
        out.append(walk("gigapage", kind, chain(0, root_ptr=pte(B, FULL))[:1], va=0x40004000))
        out.append(walk("gigapage, PPN[1] not zero", kind, chain(0, root_ptr=pte(B + 0x200000, FULL))[:1], va=0x40004000))
        out.append(walk("gigapage, PPN[0] not zero", kind, chain(0, root_ptr=pte(PA, FULL))[:1], va=0x40004000))
        out.append(walk("PTE not valid at level 2", kind, chain(pte(PA, FULL), root_ptr=0)))
        out.append(walk("PTE not valid at level 1", kind, chain(pte(PA, FULL), l1_ptr=pte(L0, 0))))
        out.append(walk("PTE not valid at level 0", kind, chain(pte(PA, FULL & ~PV))))
        out.append(walk("pointer at level 0", kind, chain(pte(PA, PV))))
        out.append(far_root(kind))
        out.append(walk("level-1 table outside memory", kind, chain(0, root_ptr=pte(FAR, PV))[:1]))
        out.append(walk("leaf maps outside memory", kind, chain(pte(FAR, FULL))))
    return out


def flag_cases():
    out = []
    for kind in KINDS:
        out.append(walk("W without R at level 0", kind, chain(pte(PA, PV | PW | PA_ | PD))))
        out.append(walk("W and X without R at level 0", kind, chain(pte(PA, PV | PW | PX | PA_ | PD))))
        out.append(walk("W without R at level 1", kind, chain(0, l1_ptr=pte(B, PV | PW | PA_ | PD))[:2], va=0x40404000))
        out.append(walk("W without R at level 2", kind, chain(0, root_ptr=pte(B, PV | PW | PA_ | PD))[:1], va=0x40004000))
        for bit in (63, 62, 61, 60, 57, 54):
            out.append(walk("leaf with reserved bit %d" % bit, kind, chain(pte(PA, FULL) | (1 << bit))))
        out.append(walk("pointer with reserved bit 54", kind, chain(pte(PA, FULL), root_ptr=pte(L1, PV) | (1 << 54))))
        for flag, name in ((PD, "D"), (PA_, "A"), (PU, "U")):
            out.append(walk("pointer with %s set at level 2" % name, kind, chain(pte(PA, FULL), root_ptr=pte(L1, PV | flag))))
            out.append(walk("pointer with %s set at level 1" % name, kind, chain(pte(PA, FULL), l1_ptr=pte(L0, PV | flag))))
        out.append(walk("pointer with G set", kind, chain(pte(PA, FULL), root_ptr=pte(L1, PV | PG))))
        out.append(walk("leaf with G and both RSW bits set", kind, chain(pte(PA, FULL | PG | 0x300))))
        out.append(walk("A clear", kind, chain(pte(PA, FULL & ~PA_))))
        out.append(walk("D clear", kind, chain(pte(PA, FULL & ~PD))))
        out.append(walk("A and D clear", kind, chain(pte(PA, FULL & ~PA_ & ~PD))))
        out.append(walk("megapage with A clear", kind, chain(0, l1_ptr=pte(B, FULL & ~PA_))[:2], va=0x40404000))
        out.append(walk("user page in S, SUM clear", kind, chain(pte(PA, FULL | PU))))
        out.append(walk("user page in S, SUM set", kind, chain(pte(PA, FULL | PU)), st=dict(SUM=1)))
        out.append(walk("user page in U", kind, chain(pte(PA, FULL | PU)), prv=U))
        out.append(walk("supervisor page in U", kind, chain(pte(PA, FULL)), prv=U, st=dict(SUM=1)))
        out.append(walk("execute-only page, MXR clear", kind, chain(pte(PA, PV | PX | PA_ | PD))))
        out.append(walk("execute-only page, MXR set", kind, chain(pte(PA, PV | PX | PA_ | PD)), st=dict(MXR=1)))
        out.append(walk("read-only page, MXR set", kind, chain(pte(PA, PV | PR | PA_ | PD)), st=dict(MXR=1)))
        out.append(walk("satp MODE 0 in S: the address is physical", kind, [], va=PA, satp=0))
        out.append(walk("satp MODE 0 in U: the address is physical", kind, [], va=PA, satp=0, prv=U))
        for mpp in (U, S, M):
            out.append(walk("M with MPRV, MPP = %d, supervisor page" % mpp, kind, chain(pte(PA, FULL)),
                            prv=M, st=dict(MPRV=1, MPP=mpp)))
        out.append(walk("M without MPRV: no translation", kind, chain(pte(PA, FULL)), prv=M, st=dict(MPP=S)))
    return out


def address_cases():
    out = []
    va2 = (5 << 30) | (511 << 21) | 0xFF8
    out.append(walk("vpn 5, 511, 0 and offset ff8", "load", chain(pte(PB, FULL), va=va2), va=va2,
                    target=PB + 0xFF8))
    for word, what in ((encode("lbu", rd=10, rs1=1), "lbu"), (encode("lb", rd=10, rs1=1), "lb"),
                       (encode("lh", rd=10, rs1=1), "lh"), (encode("lw", rd=10, rs1=1), "lw")):
        out.append(walk("%s at offset 0x7f8" % what, "load", chain(pte(PA, FULL)), va=VA + 0x7F8,
                        target=PA + 0x7F8, word=word, data=0x80F0E0D0C0B0A090))
    for word, what in ((encode("sb", rs1=1, rs2=2), "sb"), (encode("sh", rs1=1, rs2=2), "sh"),
                       (encode("sw", rs1=1, rs2=2), "sw")):
        out.append(walk("%s at offset 0x7f8" % what, "store", chain(pte(PA, FULL)), va=VA + 0x7F8,
                        target=PA + 0x7F8, word=word))
    out.append(walk("megapage offset 0x4018", "load", chain(0, l1_ptr=pte(B, FULL))[:2], va=0x40404018,
                    target=PA + 0x18))
    out.append(walk("load from the serial status register through a page", "load", chain(pte(0x10000000, FULL)),
                    va=VA + 5, word=encode("lbu", rd=10, rs1=1)))
    out.append(walk("load of mtimecmp through a page", "load", chain(pte(0x02004000, FULL))))
    return out


def cross(note, word, at, second, first=None, data=(DATA, 0xA1B2C3D4E5F60718), probe_first=False):
    """An access at VA + at by M-mode with MPRV and MPP = S. The first page maps
    to PB and the second to PA, so the two parts are not neighbours."""
    tables = chain(pte(PB, FULL) if first is None else first)
    tables.append((L0 + 8 * 4, second))
    cells = [(PB + 0xFF8, data[0]), (PA, data[1])]
    if probe_first:
        cells.reverse()
    return case(note, [word], st=dict(MPRV=1, MPP=S), satp=SATP, x1=VA + at, x2=NEW, pokes=tables + cells)


def cross_cases():
    out = []
    ok = pte(PA, FULL)
    for at in (0xFFC, 0xFF9, 0xFFF):
        out.append(cross("ld across two pages at offset %x" % at, LD, at, ok))
        out.append(cross("sd across two pages at offset %x, second part" % at, SD, at, ok))
        out.append(cross("sd across two pages at offset %x, first part" % at, SD, at, ok, probe_first=True))
    for word, what in ((encode("lh", rd=10, rs1=1), "lh"), (encode("lw", rd=10, rs1=1), "lw"),
                       (encode("sh", rs1=1, rs2=2), "sh"), (encode("sw", rs1=1, rs2=2), "sw")):
        out.append(cross("%s across two pages at offset fff" % what, word, 0xFFF, ok, probe_first=True))
    for word, what in ((LD, "ld"), (SD, "sd")):
        out.append(cross("%s across, second page not mapped" % what, word, 0xFFC, 0, probe_first=True))
        out.append(cross("%s across, first page not mapped" % what, word, 0xFFC, ok, first=0))
        out.append(cross("%s across, second page outside memory" % what, word, 0xFFC, pte(0x90000000, FULL), probe_first=True))
        out.append(cross("%s across, first page outside memory" % what, word, 0xFFC, ok, first=pte(0x90000000, FULL)))
        out.append(cross("%s across, second page read-only" % what, word, 0xFFC, pte(PA, FULL & ~PW), probe_first=True))
        out.append(cross("%s across, second page with D clear" % what, word, 0xFFC, pte(PA, FULL & ~PD), probe_first=True))
        out.append(cross("%s across, second page is the serial device" % what, word, 0xFFC, pte(0x10000000, FULL), probe_first=True))
    return out


def atomic_cases():
    out = []
    lr, sc = encode("lr.d", rd=10, rs1=1), encode("sc.d", rd=11, rs1=1, rs2=2)
    amo = encode("amoswap.d", rd=10, rs1=1, rs2=2)
    full, ro, xo = pte(PA, FULL), pte(PA, FULL & ~PW), pte(PA, PV | PX | PA_ | PD)
    out.append(walk("lr.d through a page: the reservation is physical", "load", chain(full), word=lr))
    out.append(case("lr.d then sc.d through a page", [lr, sc], prv=S, satp=SATP, x1=VA, x2=NEW,
                    pokes=[code_map(S)] + chain(full) + [(0, 0), (PA, DATA)]))
    out.append(case("lr.d at one address, sc.d at another address of the same page",
                    [lr, encode("sc.d", rd=11, rs1=2, rs2=1)], prv=S, satp=SATP, x1=VA, x2=VA + 0x1000,
                    pokes=[code_map(S)] + chain(full) + [(L0 + 8 * 4, full), (PA, DATA)]))
    for leaf, what in ((ro, "a read-only page"), (xo, "an execute-only page"), (0, "a page that is not mapped"),
                       (pte(PA, FULL & ~PD), "a page with D clear"), (pte(PA, FULL | PU), "a user page")):
        out.append(walk("sc.d with no reservation on %s" % what, "store", chain(leaf), word=sc))
        out.append(walk("amoswap.d on %s" % what, "store", chain(leaf), word=amo))
        out.append(walk("lr.d on %s" % what, "load", chain(leaf), word=lr))
    out.append(walk("amoswap.d through a page", "store", chain(full), word=amo))
    out.append(walk("lr.d at an address that is misaligned and not mapped", "load", chain(0), va=VA + 4, word=lr))
    out.append(walk("sc.d at an address that is misaligned and not mapped", "store", chain(0), va=VA + 4, word=sc))
    out.append(walk("lr.d on a page outside memory", "load", chain(pte(FAR, FULL)), word=lr))
    out.append(walk("amoswap.d on a page outside memory", "store", chain(pte(FAR, FULL)), word=amo))
    out.append(walk("amoswap.d on mtimecmp through a page", "store", chain(pte(0x02004000, FULL)), word=amo))
    return out


def part3():
    return shape_cases() + flag_cases() + address_cases() + cross_cases() + atomic_cases()
