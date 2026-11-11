# sv39_oracle.py: an independent oracle for the hand cases of the system
# profile of emu-rv64 0.6.0 (privilege modes, trap delegation, Sv39).
#
# Standard library only. It shares no code and no table with the package.
# The machine is in sv39_model.py, sv39_exec.py and sv39_step.py, written
# from the text of the RISC-V manual at commit d50ad50f; the cases are in
# sv39_cases.py, sv39_cases2.py and sv39_cases3.py, and the cases of the
# assembler in sv39_asm.py.
#
# Usage: python sv39_oracle.py OUTDIR
# It writes the case tables as BuildLang source: svec_g*.bld (generic
# cases), svec_p*.bld (the permission matrix) and svec_run.bld for this
# package, and acase9.bld for asm-rv64 0.3.0. Run again, it writes the same
# bytes.

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from sv39_model import U, S, M, FETCH, LOAD, STORE, CSR, MPOS, encode, decode, layout, MSTATUS_FIG
from sv39_step import run_generic, matrix_row, MX_ADDI, MX_STORED, MX_VA
from sv39_cases import part1, status
from sv39_cases2 import part2
from sv39_cases3 import part3
from sv39_asm import emit_asm

FN_LINES, FILE_LINES = 44, 280


def h(v):
    return "0x%X_u64" % v


def wrap(head, values, tail="];"):
    """`head` then the values, wrapped at 100 columns with an 8-space indent."""
    lines, cur = [], head
    for k, v in enumerate(values):
        piece = h(v) + ("," if k + 1 < len(values) else tail)
        if len(cur) + 1 + len(piece) > 100:
            lines.append(cur)
            cur = "        " + piece
        else:
            cur += ("" if cur.endswith("[") else " ") + piece
    return lines + [cur]


def generic_block(n, note, i, e):
    """The lines of one generic case. `@` stands for the number of the case
    inside its function, which pack() fills in."""
    pairs = []
    for idx, v in enumerate(i):
        if v:
            pairs += [idx, v]
    pairs = pairs or [4, 0]
    lines = ["    // %d: %s" % (n, note)]
    lines += wrap("    let i@: [u64; %d] = [" % len(pairs), pairs)
    lines += wrap("    let e@: [u64; 22] = [", e)
    lines.append("    sv_g(st, &i@, &e@);")
    return lines


def pack(blocks, name, header):
    """Groups blocks of lines into functions of at most FN_LINES lines and files
    of at most FILE_LINES lines. Returns ([(file, text)], [function names])."""
    files, names, fns, cur, k = [], [], [], [], 0
    for b in blocks:
        if cur and len(cur) + len(b) > FN_LINES:
            fns.append(cur)
            cur, k = [], 0
        cur = cur + [ln.replace("@", str(k)) if not ln.startswith("    //") else ln for ln in b]
        k += 1
    fns.append(cur)
    text, fi, count = list(header), 1, 0
    for body in fns:
        if count and len(text) + len(body) + 3 > FILE_LINES:
            files.append(("svec_%s%d.bld" % (name, fi), "\n".join(text)))
            text, fi, count = list(header), fi + 1, 0
        count += 1
        fn = "sv_%s%d_%d" % (name, fi, count)
        names.append(fn)
        text += ["", "fn %s(st: &mut XSt) ~ Console {" % fn] + body + ["}"]
    files.append(("svec_%s%d.bld" % (name, fi), "\n".join(text)))
    return files, names


def emit_generic(cases):
    header = ["// Hand cases of the system profile, generic part. Written by",
              "// tests/tools/sv39_oracle.py; do not edit by hand. svec.bld explains the",
              "// two arrays of a case."]
    blocks = [generic_block(n, note, i, run_generic(i)) for n, (note, i) in enumerate(cases, 1)]
    return pack(blocks, "g", header)


def matrix_rows():
    rows = []
    modes = [(U, 0, U), (S, 0, U), (M, 0, U), (M, 1, U), (M, 1, S), (M, 1, M)]
    for prv, mprv, mpp in modes:
        for sum_ in (0, 1):
            for mxr in (0, 1):
                for rwx in range(8):
                    for user in (0, 1):
                        for ad in (0xC0, 0x40, 0x00):
                            for acc in (FETCH, LOAD, STORE):
                                flags = 1 | (rwx << 1) | (user << 4) | ad
                                rows.append(matrix_row(flags, prv, mprv, mpp, sum_, mxr, acc))
    return rows


def emit_matrix(rows):
    header = ["// Hand cases of the system profile: the permission matrix. Written by",
              "// tests/tools/sv39_oracle.py; do not edit by hand. A row holds the PTE",
              "// flags in bits 7 to 0, the mode in bits 9 and 8, MPRV in bit 10, MPP in",
              "// bits 12 and 11, SUM in bit 13, MXR in bit 14, the access kind in bits",
              "// 16 and 15 (0 fetch, 1 load, 2 store) and the expected cause in bits 31",
              "// to 24, 0 when the access succeeds. svec2.bld runs a row."]
    blocks = []
    for at in range(0, len(rows), 256):
        part = rows[at:at + 256]
        body = wrap("    let r: [u64; %d] = [" % len(part), part)
        blocks.append(body + ["    sv_rows(st, &r);"])
    return pack(blocks, "p", header)


def emit_run(g_names, p_names, n_generic, n_rows):
    lines = ["// Hand cases of the system profile: the list of tables. Written by",
             "// tests/tools/sv39_oracle.py; do not edit by hand.", ""]
    groups = [g_names[k:k + 40] for k in range(0, len(g_names), 40)]
    for gi, part in enumerate(groups, 1):
        lines.append("fn sv_tables_%d(st: &mut XSt) ~ Console {" % gi)
        lines += ["    %s(st);" % fn for fn in part]
        lines += ["}", ""]
    lines.append("// Cases 1 to %d are generic cases. The %d rows of the permission matrix" % (n_generic, n_rows))
    lines.append("// follow them.")
    lines.append("fn sv_tables(st: &mut XSt) ~ Console {")
    lines += ["    sv_tables_%d(st);" % gi for gi in range(1, len(groups) + 1)]
    lines += ["}", "", "fn sv_matrix(st: &mut XSt) ~ Console {"]
    lines += ["    %s(st);" % fn for fn in p_names]
    lines += ["}", "",
              "// The words and numbers the matrix runner needs, from the encoder of the",
              "// oracle: `addi x10, x0, 77`, `ld x10, 0(x1)`, `sd x2, 0(x1)`, the value",
              "// stored and the virtual address of the page.",
              "fn sv_px_word(acc: u64) -> u64 {",
              "    if acc == 0_u64 { return %s; }" % h(MX_ADDI),
              "    if acc == 1_u64 { return %s; }" % h(encode("ld", rd=10, rs1=1)),
              "    %s" % h(encode("sd", rs1=1, rs2=2)),
              "}", "",
              "fn sv_px_stored() -> u64 { %s }" % h(MX_STORED), "",
              "fn sv_px_va() -> u64 { %s }" % h(MX_VA), ""]
    return [("svec_run.bld", "\n".join(lines))]


def self_check():
    pos = layout(MSTATUS_FIG)
    assert pos["MPP"] == (11, 2) and pos["SXL"] == (34, 2) and pos["TSR"] == (22, 1)
    assert status(MPP=M) == 0xA00001800
    for name in ("addi", "ld", "sd", "jal", "csrrs", "sret", "mret", "wfi", "sfence.vma", "lr.d", "sc.d"):
        w = encode(name, rd=10, rs1=1, rs2=2, imm=8, csr=CSR["satp"])
        assert decode(w)[0] == name, name
    assert encode("sret") == 0x10200073 and encode("mret") == 0x30200073 and encode("wfi") == 0x10500073
    assert encode("sfence.vma", rs1=1, rs2=2) == 0x12208073
    assert decode(encode("jal", rd=0, imm=-8))[1]["imm"] == (1 << 64) - 8


def main():
    outdir = sys.argv[1]
    self_check()
    cases = part1() + part2() + part3()
    rows = matrix_rows()
    g_files, g_names = emit_generic(cases)
    p_files, p_names = emit_matrix(rows)
    for name, text in g_files + p_files + emit_run(g_names, p_names, len(cases), len(rows)) + emit_asm():
        with open(os.path.join(outdir, name), "w", newline="\n") as f:
            f.write(text + "\n")
    with open(os.path.join(outdir, "scases-listing.txt"), "w", newline="\n") as f:
        for n, (note, i) in enumerate(cases, 1):
            f.write("%d %s\n" % (n, note))
    faults = len([r for r in rows if r >> 24])
    print("generic cases %d, matrix rows %d (%d fault, %d succeed), files %d" % (
        len(cases), len(rows), faults, len(rows) - faults, len(g_files) + len(p_files) + 1))


if __name__ == "__main__":
    main()
