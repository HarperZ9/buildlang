#!/usr/bin/env python3
"""Write the matrix programs of this folder and the output each must print.

    python gen_matrix.py          # rewrites *.bld and *.expected beside this file
    python gen_matrix.py --no-min DIR   # the same programs without the minimum
                                        # of each signed type, written to DIR

The expected output comes from the small model in this script (Python lists
and dicts), never from the compiler. `compiler/tests/struct_array_fields.rs`
runs each program and compares its standard output with the `.expected` file
byte for byte.

Every program prints every element of every array it touches and the fields
on both sides of it, so a shifted, truncated or dropped store shows up. The
values are distinct per position and include the minimum and the maximum of
each integer type.

Programs:
  literals  struct literals with an array field: every integer width, five
            lengths, the array first, in the middle and last; then the ways
            an array value can reach the literal (G26)
  stores    stores into an array field: constant and variable index, through
            `&mut` parameters and methods, compound assignment, whole-array
            assignment, and a copy taken before the stores (G27)
  nesting   arrays inside structs inside arrays, two-dimensional arrays,
            slices of structs and vectors of structs
  slices    an array field borrowed as a slice (G7) and a `&Vec<T>` or
            `&[T; N]` parameter handed on to a `&[T]` parameter (G31)
"""
import os
import sys

# With --no-min the smallest value used is one above the minimum, so the
# programs also compile on a compiler that rejects `-128_i8` (G32). This is
# for showing which lines an older compiler gets wrong; it is not committed.
NO_MIN = "--no-min" in sys.argv

TYPES = ["u8", "i8", "u16", "i16", "u32", "i32", "u64", "i64"]
NEIGHBOURS = ["u8", "i64", "u16", "i32"]


def bits(t):
    return int(t[1:])


def lo(t):
    if t[0] == "u":
        return 0
    return -(1 << (bits(t) - 1)) + (1 if NO_MIN else 0)


def hi(t):
    return (1 << (bits(t) - 1)) - 1 if t[0] == "i" else (1 << bits(t)) - 1


def lit(v, t):
    """A literal of type t; a negative one is parenthesised."""
    return f"({v}_{t})" if v < 0 else f"{v}_{t}"


class Rng:
    """A fixed linear congruential generator, so the files never change."""

    def __init__(self, seed):
        self.s = seed

    def next(self):
        self.s = (self.s * 6364136223846793005 + 1442695040888963407) % (1 << 64)
        return self.s >> 11

    def value(self, t):
        return lo(t) + self.next() % (hi(t) - lo(t) + 1)

    def below(self, n):
        return self.next() % n


def elements(rng, t, n):
    """n values of type t: the maximum, the minimum, then spread values."""
    vals = [hi(t), lo(t)] + [rng.value(t) for _ in range(n)]
    return vals[:n]


def fields(pos, t, n, at, zt):
    """Field declarations with the array `w` first, in the middle or last."""
    order = [["w", "a", "z"], ["a", "w", "z"], ["a", "z", "w"]][pos]
    types = {"a": at, "z": zt, "w": f"[{t}; {n}]"}
    return order, ", ".join(f"{f}: {types[f]}" for f in order)


def show_args(var, n):
    return [f"{var}.a"] + [f"{var}.w[{j}]" for j in range(n)] + [f"{var}.z"]


def println(tag, args):
    return f'println!("{tag}{" {}" * len(args)}", {", ".join(args)});'


def line(tag, values):
    return " ".join([tag] + [str(v) for v in values])


# ---------------------------------------------------------------- literals

def gen_literals():
    rng = Rng(26)
    src, out, calls = [], [], []
    k = 0
    for t in TYPES:
        for n in (1, 2, 3, 5, 8):
            for pos in (0, 1, 2):
                at, zt = NEIGHBOURS[k % 4], NEIGHBOURS[(k + 1) % 4]
                order, decl = fields(pos, t, n, at, zt)
                a, z, w = rng.value(at), rng.value(zt), elements(rng, t, n)
                init = {"a": lit(a, at), "z": lit(z, zt),
                        "w": "[" + ", ".join(lit(v, t) for v in w) + "]"}
                written = order[::-1] if k % 5 == 0 else order
                body = ", ".join(f"{f}: {init[f]}" for f in written)
                src.append(f"struct L{k} {{ {decl} }}")
                src.append(f"fn lit{k}() ~ Console {{ let s = L{k} {{ {body} }}; "
                           f"{println(f'L{k}', show_args('s', n))} }}")
                out.append(line(f"L{k}", [a] + w + [z]))
                calls.append(f"lit{k}();")
                k += 1
    for t in TYPES:
        w = elements(rng, t, 3)
        a, z = rng.value("u8"), rng.value("i64")
        wl = "[" + ", ".join(lit(v, t) for v in w) + "]"
        rep = rng.value(t)
        s = f"K_{t}"
        src += [
            f"struct {s} {{ a: u8, w: [{t}; 3], z: i64 }}",
            f"fn mkw_{t}() -> [{t}; 3] {{ {wl} }}",
            f"fn mks_{t}() -> {s} {{ {s} {{ a: {lit(a, 'u8')}, w: {wl}, z: {lit(z, 'i64')} }} }}",
            f"fn byval_{t}(s: {s}) ~ Console {{ {println(f'byval_{t}', show_args('s', 3))} }}",
            f"fn kinds_{t}() ~ Console {{",
            f"    let w = {wl};",
            f"    let named = {s} {{ a: {lit(a, 'u8')}, w: w, z: {lit(z, 'i64')} }};",
            f"    let a = {lit(a, 'u8')}; let z = {lit(z, 'i64')};",
            f"    let short = {s} {{ a, w, z }};",
            f"    let called = {s} {{ a: a, w: mkw_{t}(), z: z }};",
            f"    let copied = {s} {{ a: a, w: named.w, z: z }};",
            f"    let repeated = {s} {{ a: a, w: [{lit(rep, t)}; 3], z: z }};",
            f"    let returned = mks_{t}();",
            f"    let moved = returned;",
        ]
        for name in ("named", "short", "called", "copied", "repeated", "returned", "moved"):
            src.append(f"    {println(f'{name}_{t}', show_args(name, 3))}")
            vals = [rep] * 3 if name == "repeated" else w
            out.append(line(f"{name}_{t}", [a] + vals + [z]))
        src += [f"    byval_{t}(named);", "}"]
        out.append(line(f"byval_{t}", [a] + w + [z]))
        calls.append(f"kinds_{t}();")
    src.append("fn main() ~ Console {\n    " + "\n    ".join(calls) + "\n}")
    return src, out


# ------------------------------------------------------------------ stores

def gen_stores():
    rng = Rng(27)
    src, out, calls = [], [], []
    k = 0
    for t in TYPES:
        for n in (1, 3, 8):
            for pos in (0, 1, 2):
                at, zt = NEIGHBOURS[(k + 2) % 4], NEIGHBOURS[(k + 3) % 4]
                order, decl = fields(pos, t, n, at, zt)
                a, z, w = rng.value(at), rng.value(zt), elements(rng, t, n)
                init = {"a": lit(a, at), "z": lit(z, zt),
                        "w": "[" + ", ".join(lit(v, t) for v in w) + "]"}
                s = f"W{k}"
                src += [
                    f"struct {s} {{ {decl} }}",
                    f"impl {s} {{",
                    f"    fn put(&mut self, i: usize, v: {t}) {{ self.w[i] = v; }}",
                    f"    fn add(&mut self, i: usize, d: {t}) {{ self.w[i] += d; }}",
                    f"    fn sub(&mut self, i: usize, d: {t}) {{ self.w[i] -= d; }}",
                    "}",
                    f"fn set{k}(s: &mut {s}, i: usize, v: {t}) {{ s.w[i] = v; }}",
                    f"fn show{k}(s: &{s}) ~ Console {{ {println(s, show_args('s', n))} }}",
                    f"fn st{k}() ~ Console {{",
                    f"    let mut s = {s} {{ {', '.join(f'{f}: {init[f]}' for f in order)} }};",
                    "    let t = s;",
                ]
                before = list(w)
                i0, i1, i2, i3, i4, i5 = (rng.below(n) for _ in range(6))
                v0, v1, v2, v3 = (rng.value(t) for _ in range(4))
                w[i0] = v0
                src.append(f"    s.w[{i0}] = {lit(v0, t)};")
                w[i1] = v1
                src.append(f"    let i: usize = {i1}; s.w[i] = {lit(v1, t)};")
                w[i2] = v2
                src.append(f"    set{k}(&mut s, {i2}, {lit(v2, t)});")
                w[i3] = v3
                src.append(f"    s.put({i3}, {lit(v3, t)});")
                if w[i4] < hi(t):
                    w[i4] += 1
                    src.append(f"    let j: usize = {i4}; s.w[j] += {lit(1, t)};")
                else:
                    w[i4] -= 1
                    src.append(f"    let j: usize = {i4}; s.w[j] -= {lit(1, t)};")
                if w[i5] < hi(t):
                    w[i5] += 1
                    src.append(f"    s.add({i5}, {lit(1, t)});")
                else:
                    w[i5] -= 1
                    src.append(f"    s.sub({i5}, {lit(1, t)});")
                src += [f"    show{k}(&s);", f"    show{k}(&t);"]
                out.append(line(s, [a] + w + [z]))
                out.append(line(s, [a] + before + [z]))
                w2, a2 = elements(rng, t, n)[::-1], rng.value(at)
                src += [
                    f"    s.w = [{', '.join(lit(v, t) for v in w2)}];",
                    f"    s.a = {lit(a2, at)};",
                    f"    {println(f'{s}b', show_args('s', n))}",
                    "}",
                ]
                out.append(line(f"{s}b", [a2] + w2 + [z]))
                calls.append(f"st{k}();")
                k += 1
    src.append("fn main() ~ Console {\n    " + "\n    ".join(calls) + "\n}")
    return src, out


# ----------------------------------------------------------------- nesting

def gen_nesting():
    rng = Rng(28)
    src, out, calls = [], [], []
    for t in TYPES:
        I, O = f"In_{t}", f"Out_{t}"

        def inner():
            return {"t": rng.value("u8"), "w": [rng.value(t) for _ in range(3)]}

        def inner_lit(v):
            return f"{I} {{ t: {lit(v['t'], 'u8')}, w: [{', '.join(lit(x, t) for x in v['w'])}] }}"

        def inner_args(e):
            return [f"{e}.t"] + [f"{e}.w[{j}]" for j in range(3)]

        def inner_vals(v):
            return [v["t"]] + v["w"]

        o = {"k": rng.value("u8"), "inner": inner(), "arr": [inner(), inner()],
             "g": [[rng.value(t) for _ in range(2)] for _ in range(3)], "z": rng.value("i64")}
        loc = [inner(), inner()]
        a2 = [[rng.value(t) for _ in range(2)] for _ in range(3)]
        vec = [inner(), inner()]
        g_lit = "[" + ", ".join("[" + ", ".join(lit(x, t) for x in row) + "]" for row in o["g"]) + "]"
        a2_lit = "[" + ", ".join("[" + ", ".join(lit(x, t) for x in row) + "]" for row in a2) + "]"
        src += [
            f"struct {I} {{ t: u8, w: [{t}; 3] }}",
            f"struct {O} {{ k: u8, inner: {I}, arr: [{I}; 2], g: [[{t}; 2]; 3], z: i64 }}",
            f"fn pin_{t}(p: &mut {I}, j: usize, v: {t}) {{ p.w[j] = v; }}",
            f"fn parr_{t}(o: &mut {O}, i: usize, j: usize, v: {t}) {{ o.arr[i].w[j] = v; }}",
            f"fn pgrid_{t}(o: &mut {O}, i: usize, j: usize, v: {t}) {{ o.g[i][j] = v; }}",
            f"fn pinner_{t}(o: &mut {O}, j: usize, v: {t}) {{ o.inner.w[j] = v; }}",
            f"fn pslice_{t}(xs: &mut [{I}], i: usize, j: usize, v: {t}, tag: u8) "
            f"{{ xs[i].w[j] = v; xs[i].t = tag; }}",
            f"fn rslice_{t}(xs: &[{I}], i: usize, j: usize) -> {t} {{ xs[i].w[j] }}",
            f"fn rgrid_{t}(o: &{O}, i: usize, j: usize) -> {t} {{ o.g[i][j] }}",
            f"fn nest_{t}() ~ Console {{",
            f"    let mut o = {O} {{ k: {lit(o['k'], 'u8')}, inner: {inner_lit(o['inner'])}, "
            f"arr: [{inner_lit(o['arr'][0])}, {inner_lit(o['arr'][1])}], g: {g_lit}, "
            f"z: {lit(o['z'], 'i64')} }};",
            f"    let mut loc: [{I}; 2] = [{inner_lit(loc[0])}, {inner_lit(loc[1])}];",
            f"    let mut a2: [[{t}; 2]; 3] = {a2_lit};",
            f"    let mut v: Vec<{I}> = Vec::new();",
            f"    v.push({inner_lit(vec[0])});",
            f"    v.push({inner_lit(vec[1])});",
        ]

        x = [rng.value(t) for _ in range(16)]
        tags = [rng.value("u8") for _ in range(4)]
        o["inner"]["w"][1] = x[0]
        src.append(f"    o.inner.w[1] = {lit(x[0], t)};")
        o["arr"][1]["w"][2] = x[1]
        src.append(f"    o.arr[1].w[2] = {lit(x[1], t)};")
        o["arr"][0]["t"] = tags[0]
        src.append(f"    o.arr[0].t = {lit(tags[0], 'u8')};")
        o["g"][2][1] = x[2]
        src.append(f"    o.g[2][1] = {lit(x[2], t)};")
        o["inner"]["w"][0] = x[3]
        src.append(f"    pin_{t}(&mut o.inner, 0, {lit(x[3], t)});")
        o["arr"][0]["w"][1] = x[4]
        src.append(f"    pin_{t}(&mut o.arr[0], 1, {lit(x[4], t)});")
        o["arr"][1]["w"][0] = x[5]
        src.append(f"    parr_{t}(&mut o, 1, 0, {lit(x[5], t)});")
        o["g"][0][0] = x[6]
        src.append(f"    pgrid_{t}(&mut o, 0, 0, {lit(x[6], t)});")
        o["inner"]["w"][2] = x[7]
        src.append(f"    pinner_{t}(&mut o, 2, {lit(x[7], t)});")
        o["arr"][0]["w"][2] = x[8]
        o["arr"][0]["t"] = tags[1]
        src.append(f"    pslice_{t}(&mut o.arr, 0, 2, {lit(x[8], t)}, {lit(tags[1], 'u8')});")
        loc[1]["w"][0] = x[9]
        src.append(f"    let i: usize = 1; loc[i].w[0] = {lit(x[9], t)};")
        loc[0]["w"][2] = x[10]
        loc[0]["t"] = tags[2]
        src.append(f"    pslice_{t}(&mut loc, 0, 2, {lit(x[10], t)}, {lit(tags[2], 'u8')});")
        a2[1][0] = x[11]
        src.append(f"    a2[1][0] = {lit(x[11], t)};")
        a2[2][1] = x[12]
        src.append(f"    let r: usize = 2; let c: usize = 1; a2[r][c] = {lit(x[12], t)};")
        vec[1]["w"][2] = x[13]
        src.append(f"    v[1].w[2] = {lit(x[13], t)};")
        vec[0]["t"] = tags[3]
        src.append(f"    v[0].t = {lit(tags[3], 'u8')};")
        vec[0]["w"][0] = x[14]
        src.append(f"    let q: usize = 0; v[q].w[q] = {lit(x[14], t)};")
        rows = [
            ("o", ["o.k", "o.z"], [o["k"], o["z"]]),
            ("inner", inner_args("o.inner"), inner_vals(o["inner"])),
            ("arr0", inner_args("o.arr[0]"), inner_vals(o["arr"][0])),
            ("arr1", inner_args("o.arr[1]"), inner_vals(o["arr"][1])),
            ("g", [f"o.g[{i}][{j}]" for i in range(3) for j in range(2)],
             [o["g"][i][j] for i in range(3) for j in range(2)]),
            ("loc0", inner_args("loc[0]"), inner_vals(loc[0])),
            ("loc1", inner_args("loc[1]"), inner_vals(loc[1])),
            ("a2", [f"a2[{i}][{j}]" for i in range(3) for j in range(2)],
             [a2[i][j] for i in range(3) for j in range(2)]),
            ("vec0", inner_args("v[0]"), inner_vals(vec[0])),
            ("vec1", inner_args("v[1]"), inner_vals(vec[1])),
            ("read", [f"rslice_{t}(&o.arr, 1, 0)", f"rslice_{t}(&loc, 0, 2)", f"rgrid_{t}(&o, 2, 1)"],
             [o["arr"][1]["w"][0], loc[0]["w"][2], o["g"][2][1]]),
        ]
        for tag, args, vals in rows:
            src.append(f"    {println(f'{tag}_{t}', args)}")
            out.append(line(f"{tag}_{t}", vals))
        src.append("}")
        calls.append(f"nest_{t}();")
    src.append("fn main() ~ Console {\n    " + "\n    ".join(calls) + "\n}")
    return src, out


# ------------------------------------------------------------------ slices

def gen_slices():
    rng = Rng(31)
    src, out, calls = [], [], []
    for t in TYPES:
        S = f"Sl_{t}"
        small_lo, small_hi = max(lo(t), -100), min(hi(t), 100)

        def small():
            return small_lo + rng.below(small_hi - small_lo)

        w = [small() for _ in range(4)]
        g = [[small() for _ in range(4)] for _ in range(2)]
        vec = [small() for _ in range(5)]
        arr = [small() for _ in range(4)]
        a, z = rng.value("u8"), rng.value("i64")

        def weigh(xs):
            return sum((i + 1) * x for i, x in enumerate(xs))

        def lits(xs):
            return "[" + ", ".join(lit(x, t) for x in xs) + "]"

        src += [
            f"struct {S} {{ a: u8, w: [{t}; 4], z: i64 }}",
            f"struct Two_{t} {{ g: [[{t}; 4]; 2], s: {S} }}",
            f"fn sum_{t}(b: &[{t}]) -> i64 {{ let mut s: i64 = 0_i64; let mut i: usize = 0; "
            f"while i < b.len() {{ s = s + (i as i64 + 1_i64) * (b[i] as i64); i = i + 1; }} s }}",
            f"fn bump_{t}(b: &mut [{t}]) {{ let mut i: usize = 0; "
            f"while i < b.len() {{ b[i] = b[i] + {lit(1, t)}; i = i + 1; }} }}",
            f"fn len_{t}(b: &[{t}]) -> usize {{ b.len() }}",
            f"fn via_{t}(s: &mut {S}) -> i64 {{ bump_{t}(&mut s.w); sum_{t}(&s.w) }}",
            f"fn fwd_vec_{t}(v: &Vec<{t}>) -> i64 {{ sum_{t}(v) }}",
            f"fn fwd_len_{t}(v: &Vec<{t}>) -> usize {{ len_{t}(v) }}",
            f"fn fwd_mut_vec_{t}(v: &mut Vec<{t}>) {{ bump_{t}(v); }}",
            f"fn reborrow_{t}(v: &Vec<{t}>) -> i64 {{ sum_{t}(&*v) }}",
            f"fn twice_{t}(v: &Vec<{t}>) -> i64 {{ fwd_vec_{t}(v) }}",
            f"fn fwd_arr_{t}(a: &[{t}; 4]) -> i64 {{ sum_{t}(a) }}",
            f"fn fwd_arr_len_{t}(a: &[{t}; 4]) -> usize {{ len_{t}(a) }}",
            f"fn fwd_mut_arr_{t}(a: &mut [{t}; 4]) {{ bump_{t}(a); }}",
            f"fn mk_{t}() -> [{t}; 4] {{ {lits(arr)} }}",
            f"fn slices_{t}() ~ Console {{",
            f"    let mut s = {S} {{ a: {lit(a, 'u8')}, w: {lits(w)}, z: {lit(z, 'i64')} }};",
            f"    let mut two = Two_{t} {{ g: [{lits(g[0])}, {lits(g[1])}], s: s }};",
            f"    let mut v: Vec<{t}> = Vec::new();",
        ]
        src += [f"    v.push({lit(x, t)});" for x in vec]
        src.append(f"    let mut arr: [{t}; 4] = {lits(arr)};")

        def row(tag, exprs, vals):
            src.append(f"    {println(f'{tag}_{t}', exprs)}")
            out.append(line(f"{tag}_{t}", vals))

        row("field", [f"sum_{t}(&s.w)", f"len_{t}(&s.w)"], [weigh(w), 4])
        src.append(f"    bump_{t}(&mut s.w);")
        w = [x + 1 for x in w]
        row("bumped", show_args("s", 4), [a] + w + [z])
        w = [x + 1 for x in w]
        row("via", [f"via_{t}(&mut s)"] + show_args("s", 4), [weigh(w), a] + w + [z])
        two_w = [x - 2 for x in w]
        row("deep", [f"sum_{t}(&two.s.w)", f"sum_{t}(&two.g[1])", f"len_{t}(&two.g[0])"],
            [weigh(two_w), weigh(g[1]), 4])
        src.append(f"    bump_{t}(&mut two.g[0]);")
        src.append(f"    bump_{t}(&mut two.s.w);")
        g[0] = [x + 1 for x in g[0]]
        two_w = [x + 1 for x in two_w]
        row("deepw", [f"two.g[0][{j}]" for j in range(4)] + [f"two.g[1][{j}]" for j in range(4)]
            + [f"two.s.w[{j}]" for j in range(4)], g[0] + g[1] + two_w)
        src.append(f"    let sl: &[{t}] = &s.w;")
        row("bound", [f"sum_{t}(sl)", "sl.len()", "sl[3]"], [weigh(w), 4, w[3]])
        row("vec", [f"fwd_vec_{t}(&v)", f"fwd_len_{t}(&v)", f"reborrow_{t}(&v)", f"twice_{t}(&v)"],
            [weigh(vec), 5, weigh(vec), weigh(vec)])
        src.append(f"    fwd_mut_vec_{t}(&mut v);")
        vec = [x + 1 for x in vec]
        row("vecw", [f"v[{j}]" for j in range(5)], vec)
        row("arr", [f"fwd_arr_{t}(&arr)", f"fwd_arr_len_{t}(&arr)", f"fwd_arr_{t}(&s.w)"],
            [weigh(arr), 4, weigh(w)])
        src.append(f"    fwd_mut_arr_{t}(&mut arr);")
        src.append(f"    fwd_mut_arr_{t}(&mut s.w);")
        arr_b = [x + 1 for x in arr]
        w = [x + 1 for x in w]
        row("arrw", [f"arr[{j}]" for j in range(4)] + show_args("s", 4), arr_b + [a] + w + [z])
        row("temp", [f"sum_{t}(&mk_{t}())", f"len_{t}(&mk_{t}())"], [weigh(arr), 4])
        src.append("}")
        calls.append(f"slices_{t}();")
    src.append("fn main() ~ Console {\n    " + "\n    ".join(calls) + "\n}")
    return src, out


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    if NO_MIN:
        here = sys.argv[sys.argv.index("--no-min") + 1]
        os.makedirs(here, exist_ok=True)
    for name, gen in (("literals", gen_literals), ("stores", gen_stores),
                      ("nesting", gen_nesting), ("slices", gen_slices)):
        src, out = gen()
        header = f"// Written by gen_matrix.py; do not edit. Expected output: {name}.expected\n"
        with open(os.path.join(here, f"{name}.bld"), "w", newline="\n") as f:
            f.write(header + "\n".join(src) + "\n")
        with open(os.path.join(here, f"{name}.expected"), "w", newline="\n") as f:
            f.write("\n".join(out) + "\n")
        print(name, len(src), "source lines,", len(out), "expected lines")


if __name__ == "__main__":
    main()
