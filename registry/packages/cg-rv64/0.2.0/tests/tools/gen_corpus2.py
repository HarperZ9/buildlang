# Writes the three corpus programs added in cg-rv64 0.2.0: sarrays.bld
# (struct literals with array fields, stores into array fields and through
# slices of structs), globals.bld (arrays and structs in mutable globals)
# and byval.bld (fixed arrays passed by value). Run from the package folder:
#
#     python -I tests/tools/gen_corpus2.py tests/programs
#
# The output is committed. Each program has `run(k)`: every k runs the
# same statements with indexes taken from k, and the last part of `run`
# traps for some k, a different trap for each. The differential check
# compares the hosted program with the emulated one; nothing here computes
# an expected value. Standard library only.
import sys

TYPES = [("u8", 8, False), ("i8", 8, True), ("u16", 16, False), ("i16", 16, True),
         ("u32", 32, False), ("i32", 32, True), ("u64", 64, False), ("i64", 64, True)]
MAIN = ("fn main() ~ Console + Process + Environment {\n"
        "    run(args_get(1).parse_int() as u64);\n}\n")


def val(t, seed):
    """A value of type t, distinct for small seeds; seeds 0 and 1 are the ends."""
    name, bits, signed = t
    lo = -(1 << (bits - 1)) if signed else 0
    hi = (1 << (bits - 1)) - 1 if signed else (1 << bits) - 1
    if seed % 5 == 0:
        return lo
    if seed % 5 == 1:
        return hi
    return lo + (seed * 2654435761 + 977) % (hi - lo + 1)


def lit(t, seed):
    v = val(t, seed)
    return "(%d_%s)" % (v, t[0]) if v < 0 else "%d_%s" % (v, t[0])


def arr(t, seeds):
    return "[" + ", ".join(lit(t, s) for s in seeds) + "]"


def show(names):
    return 'println!("' + " ".join("{}" for _ in names) + '", ' + ", ".join(names) + ");"


def sarrays(t, s):
    n = t[0]
    a = ["a.a"] + ["a.w[%d]" % i for i in range(3)] + ["a.z"]
    top = [
        "struct A_%s { a: u8, w: [%s; 3], z: i64 }" % (n, n),
        "struct B_%s { w: [%s; 4], b: u16 }" % (n, n),
        "struct C_%s { c: i32, w: [%s; 1] }" % (n, n),
        "struct N_%s { h: u8, inner: A_%s, ws: [A_%s; 2] }" % (n, n, n),
        "fn mk_%s(k: u64) -> A_%s { A_%s { a: k as u8, w: %s, z: -7_i64 } }" % (n, n, n, arr(t, [s, s + 1, s + 2])),
        "fn sw_%s(xs: &mut [A_%s], i: u64, v: %s) { xs[i].w[i %% 3] = v; xs[i].a = 9_u8; }" % (n, n, n),
        "fn byv_%s(mut x: A_%s) -> %s { x.w[1] = %s; x.w[1] }" % (n, n, n, lit(t, s + 3)),
        "fn sec_%s(k: u64) ~ Console {" % n,
        "    let four: [%s; 4] = %s;" % (n, arr(t, [s + 4, s + 5, s + 6, s + 7])),
        "    let mut a = A_%s { a: 1_u8, w: %s, z: -2_i64 };" % (n, arr(t, [s + 8, s + 9, s + 10])),
        "    let b = B_%s { w: four, b: 9_u16 };" % n,
        "    let c = C_%s { c: -1_i32, w: [%s] };" % (n, lit(t, s + 11)),
        "    " + show(a + ["b.w[0]", "b.w[3]", "b.b", "c.c", "c.w[0]"]),
        "    a.w[k %% 3] = %s;" % lit(t, s + 12),
        "    let r = byv_%s(a);" % n,
        "    " + show(["r"] + a),
        "    let mut nn = N_%s { h: 2_u8, inner: a, ws: [a, mk_%s(k)] };" % (n, n),
        "    nn.ws[1].w[2] = %s;" % lit(t, s + 13),
        "    nn.inner.w[(k + 1) %% 3] = %s;" % lit(t, s + 14),
        "    " + show(["nn.h", "nn.inner.w[0]", "nn.inner.w[1]", "nn.inner.w[2]",
                       "nn.ws[0].w[0]", "nn.ws[1].a", "nn.ws[1].w[2]", "nn.ws[1].z"]),
        "    let mut xs: [A_%s; 2] = [a, mk_%s(k + 1)];" % (n, n),
        "    sw_%s(&mut xs, k %% 2, %s);" % (n, lit(t, s + 15)),
        "    " + show(["xs[0].a", "xs[0].w[0]", "xs[1].a", "xs[1].w[1]", "xs[1].w[2]"]),
        "}",
    ]
    return top


def sarrays_program():
    out = ["// Differential corpus, since 0.2.0: struct literals with an array field",
           "// beside other fields (refused by 0.1.0), stores into array fields of",
           "// locals, nested structs and arrays of structs, and an element store",
           "// through a slice of structs. Written by tests/tools/gen_corpus2.py.", ""]
    for i, t in enumerate(TYPES):
        out += sarrays(t, 20 * i + 2)
    out += [
        "fn run(k: u64) ~ Console {",
        "    " + "\n    ".join("sec_%s(k %% 2);" % t[0] for t in TYPES),
        "    let mut xs: [A_u16; 2] = [mk_u16(1_u64), mk_u16(2_u64)];",
        "    if k == 2_u64 { sw_u16(&mut xs, k, 5_u16); }",
        "    let mut q = mk_i64(k);",
        "    if k >= 3_u64 { q.w[k] = 1_i64; }",
        "    println!(\"{} {}\", q.w[0], xs[1].w[1]);",
        "}",
    ]
    return out


def globals_(t, s):
    n = t[0]
    g = ["GA_%s[%d]" % (n, i) for i in range(3)]
    sg = ["SG_%s.a" % n] + ["SG_%s.w[%d]" % (n, i) for i in range(3)] + ["SG_%s.z" % n]
    gp = ["GP_%s[%d].a" % (n, i) for i in range(2)] + ["GP_%s[%d].w[1]" % (n, i) for i in range(2)]
    return [
        "struct G_%s { a: u8, w: [%s; 3], z: i64 }" % (n, n),
        "impl G_%s { fn bump(&mut self, i: u64, v: %s) { self.w[i] = v; self.a = self.a + 1_u8; } }" % (n, n),
        "static mut GA_%s: [%s; 3] = %s;" % (n, n, arr(t, [s, s + 1, s + 2])),
        "static mut SG_%s: G_%s = G_%s { z: -3_i64, w: %s, a: 5_u8 };" % (n, n, n, arr(t, [s + 3, s + 4, s + 5])),
        "static mut GP_%s: [G_%s; 2] = [G_%s { a: 0_u8, w: [0_%s; 3], z: 0_i64 }; 2];" % (n, n, n, n),
        "fn put_%s(p: &mut [%s; 3], i: u64, v: %s) { p[i] = v; }" % (n, n, n),
        "fn sl_%s(p: &mut [%s], i: u64, v: %s) { p[i] = v; }" % (n, n, n),
        "fn sec_%s(k: u64) ~ Console {" % n,
        "    unsafe {",
        "        " + show(g + sg),
        "        GA_%s[k %% 3] = %s;" % (n, lit(t, s + 6)),
        "        GA_%s[(k + 1) %% 3] ^= %s;" % (n, lit(t, s + 7)),
        "        put_%s(&mut GA_%s, (k + 2) %% 3, %s);" % (n, n, lit(t, s + 8)),
        "        SG_%s.w[k %% 3] = %s;" % (n, lit(t, s + 9)),
        "        SG_%s.z = SG_%s.z - 1_i64;" % (n, n),
        "        sl_%s(&mut SG_%s.w, (k + 1) %% 3, %s);" % (n, n, lit(t, s + 10)),
        "        SG_%s.bump((k + 2) %% 3, %s);" % (n, lit(t, s + 11)),
        "        GP_%s[k %% 2].w[1] = %s;" % (n, lit(t, s + 12)),
        "        GP_%s[1].a = 7_u8;" % n,
        "        let keep = GA_%s;" % n,
        "        let fresh: [%s; 3] = %s;" % (n, arr(t, [s + 13, s + 14, s + 15])),
        "        GA_%s = fresh;" % n,
        "        " + show(g + sg + gp + ["keep[0]", "keep[1]", "keep[2]"]),
        "    }",
        "}",
    ]


def globals_program():
    out = ["// Differential corpus, since 0.2.0: arrays and structs held in mutable",
           "// globals, with initial values (struct fields written out of order),",
           "// stores by index, compound stores, borrows as arrays and slices, a",
           "// method on a global, whole-array stores and reads of an immutable",
           "// global array. Written by tests/tools/gen_corpus2.py.", "",
           "static mut CNT: u64 = 4294967290_u64;",
           "const TAB: [u16; 4] = [3_u16, 65535_u16, 0_u16, 1000_u16];", ""]
    for i, t in enumerate(TYPES):
        out += globals_(t, 20 * i + 3)
    out += [
        "fn run(k: u64) ~ Console {",
        "    " + "\n    ".join("sec_%s(k);" % t[0] for t in TYPES),
        "    unsafe {",
        "        let mut i = 0_u64;",
        "        while i < 10_u64 { CNT = CNT + 1_u64; i = i + 1_u64; }",
        "        println!(\"{} {}\", CNT, TAB[k % 4]);",
        "        if k == 2_u64 { GA_u8[k + 1] = 1_u8; }",
        "        if k == 3_u64 { SG_i32.w[k] = 1_i32; }",
        "        if k == 4_u64 { GP_u64[k].z = 1_i64; }",
        "        if k == 5_u64 { println!(\"{}\", GA_i16[k]); }",
        "        if k == 6_u64 { println!(\"{}\", TAB[k]); }",
        "    }",
        "}",
    ]
    return out


def byval(t, s):
    n = t[0]
    xs = ["x[%d]" % i for i in range(4)]
    return [
        "fn bv_%s(mut a: [%s; 4], i: u64) -> %s { a[i] = %s; a[i] }" % (n, n, n, lit(t, s)),
        "fn bw_%s(mut a: [%s; 4]) -> %s { a = %s; a[3] }" % (n, n, n, arr(t, [s + 1, s + 2, s + 3, s + 4])),
        "fn bn_%s(mut a: [[%s; 2]; 2]) -> %s { a[1][0] = %s; a[1][0] }" % (n, n, n, lit(t, s + 5)),
        "fn sec_%s(k: u64) ~ Console {" % n,
        "    let x: [%s; 4] = %s;" % (n, arr(t, [s + 6, s + 7, s + 8, s + 9])),
        "    let m: [[%s; 2]; 2] = [%s, %s];" % (n, arr(t, [s + 10, s + 11]), arr(t, [s + 12, s + 13])),
        "    " + show(["bv_%s(x, k %% 4)" % n, "bw_%s(x)" % n, "bn_%s(m)" % n] + xs + ["m[1][0]"]),
        "}",
    ]


def byval_program():
    out = ["// Differential corpus, since 0.2.0: fixed arrays passed by value and",
           "// changed by the callee, which must not change the caller's array",
           "// (G33 of the hosted backend). Written by tests/tools/gen_corpus2.py.", ""]
    for i, t in enumerate(TYPES):
        out += byval(t, 20 * i + 4)
    out += ["fn run(k: u64) ~ Console {"] + ["    sec_%s(k);" % t[0] for t in TYPES] + ["}"]
    return out


def main():
    folder = sys.argv[1]
    for name, lines in [("sarrays", sarrays_program()), ("globals", globals_program()),
                        ("byval", byval_program())]:
        with open("%s/%s.bld" % (folder, name), "w", newline="\n") as f:
            f.write("\n".join(lines) + "\n\n" + MAIN)


main()
