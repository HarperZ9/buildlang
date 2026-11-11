# sv39_asm.py: the hand cases of asm-rv64 0.3.0 for the statements the
# system profile adds: sret, sfence.vma and fifteen CSR names. The words
# come from the encoder of sv39_model.py, which holds the bit rows of the
# manual's listings and shares no code and no table with the encoder of
# emu-rv64 or with the assembler.

from sv39_model import CSR, encode

NEW_NAMES = ["sstatus", "sie", "stvec", "scounteren", "senvcfg", "sscratch", "sepc", "scause",
             "stval", "sip", "satp", "medeleg", "mideleg", "mcounteren", "menvcfg"]
REFUSED = [("sret x1", "AE_COUNT"), ("sfence.vma", "AE_COUNT"), ("sfence.vma x1", "AE_COUNT"),
           ("sfence.vma x1, x2, x3", "AE_COUNT"), ("sfence.vma x1, 5", "AE_REG"),
           ("sfence.vma x32, x1", "AE_REG"), ("sfence x1, x2", "AE_MNEMONIC"),
           ("sfence.vma x1, x2 x3", "AE_TRAIL"), ("csrrw x0, sstatuz, x1", "AE_CSR"),
           ("csrrs x1, SATP, x2", "AE_CSR")]


def emit_asm():
    L = ["// Hand cases of asm-rv64, ninth part: the statements of the system",
         "// profile, one each. Written by tests/tools/sv39_oracle.py of emu-rv64;",
         "// do not edit by hand. The words come from that script, which builds",
         "// each from the bit rows of the manual's listings and shares no code and",
         "// no table with the encoder or the assembler.", "",
         "// SRET, SFENCE.VMA and one instruction that names each new CSR.",
         "fn ac_s_words(st: &mut AcSt) ~ Console {",
         '    ac_w(st, "sret", 0x%08X_u64);' % encode("sret")]
    for rs1, rs2 in ((0, 0), (1, 2), (31, 31), (5, 0), (0, 7), (10, 11)):
        L.append('    ac_w(st, "sfence.vma x%d, x%d", 0x%08X_u64);' % (rs1, rs2, encode("sfence.vma", rs1=rs1, rs2=rs2)))
    for name in NEW_NAMES:
        L.append('    ac_w(st, "csrrs x1, %s, x2", 0x%08X_u64);' % (name, encode("csrrs", rd=1, rs1=2, csr=CSR[name])))
    L.append('    ac_w(st, "csrrw x0, satp, x6", 0x%08X_u64);' % encode("csrrw", rd=0, rs1=6, csr=CSR["satp"]))
    L.append('    ac_w(st, "csrrc x0, sstatus, x6", 0x%08X_u64);' % encode("csrrc", rd=0, rs1=6, csr=CSR["sstatus"]))
    L += ["}", "", "// Statements that must be refused.", "fn ac_s_refuse(st: &mut AcSt) ~ Console {"]
    L += ['    ac_e(st, "%s", %s);' % (text, code) for text, code in REFUSED]
    L += ["}", "", "fn ac_s_all(st: &mut AcSt) ~ Console {", "    ac_s_words(st);", "    ac_s_refuse(st);", "}"]
    return [("acase9.bld", "\n".join(L))]
