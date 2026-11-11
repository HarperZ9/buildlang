#!/usr/bin/env python3
"""Write the matrix programs of this folder and the output each must print.

    python gen_matrix.py      # rewrites *.bld and *.expected beside this file
    python gen_matrix.py --no-global DIR   # byval without its global, to DIR

The second form is for showing which lines a compiler without global arrays
(G34) gets wrong; it is not committed.

The expected output comes from the small model in this script (Python lists),
never from the compiler. `compiler/tests/value_array_params.rs` (byval) and
`compiler/tests/global_stores.rs` (globals) run
each program and compares its standard output with the `.expected` file byte
for byte after reading `\\r\\n` as `\\n`.

Programs:
  byval    fixed arrays passed by value (G33): a store into a `mut` array
           parameter, a whole-array assignment to it, the array forwarded to
           a second by-value call, a struct with an array field passed by
           value, a two-dimensional array, an array returned by value, and a
           by-value parameter read after the callee changed the global the
           caller copied it from. Each line prints the callee's result and
           every element the caller still holds.
  globals  arrays and structs held in `static mut` globals (G34, G35): the
           initializer of each (struct fields written out of declaration
           order), stores by constant and variable index, through `&mut [T; N]`
           and `&mut [T]` parameters, through a `&mut self` method, compound
           `^=` and `|=`, whole-array assignment, and the same into a field
           of a global struct. Every element and the fields beside the array
           are printed after each group of stores.

Every integer width, lengths 1, 2, 3, 5 and 8; values are distinct per
position and include the minimum and the maximum of each type.
"""
import os
import sys

NO_GLOBAL = "--no-global" in sys.argv
OUT_DIR = sys.argv[-1] if NO_GLOBAL else os.path.dirname(os.path.abspath(__file__))

TYPES = ["u8", "i8", "u16", "i16", "u32", "i32", "u64", "i64"]
LENGTHS = [1, 2, 3, 5, 8]


def bits(t):
    return int(t[1:])


def lo(t):
    return -(1 << (bits(t) - 1)) if t[0] == "i" else 0


def hi(t):
    return (1 << (bits(t) - 1)) - 1 if t[0] == "i" else (1 << bits(t)) - 1


def val(t, seed):
    """A value of type t, distinct for small distinct seeds."""
    if seed % 7 == 0:
        return lo(t)
    if seed % 7 == 1:
        return hi(t)
    span = hi(t) - lo(t) + 1
    return lo(t) + (seed * 2654435761 + 12345) % span


def wrap(v, t):
    """v cut to the width of t and read back with its sign."""
    m = v & ((1 << bits(t)) - 1)
    return m - (1 << bits(t)) if t[0] == "i" and m >> (bits(t) - 1) else m


def lit(v, t):
    return f"({v}_{t})" if v < 0 else f"{v}_{t}"


def arr(vals, t):
    return "[" + ", ".join(lit(v, t) for v in vals) + "]"


def pr(names):
    """A println! of the given expressions, one `{}` each."""
    return 'println!("' + " ".join("{}" for _ in names) + '", ' + ", ".join(names) + ");"


def byval_case(t, n, seed, top, body, out):
    sfx = f"{t}_{n}"
    x = [val(t, seed + i) for i in range(n)]
    v, w = val(t, seed + 40), [val(t, seed + 50 + i) for i in range(n)]
    top += [
        f"fn bv_{sfx}(mut a: [{t}; {n}], k: usize, v: {t}) -> {t} {{ a[k] = v; a[k] }}",
        f"fn bw_{sfx}(mut a: [{t}; {n}]) -> {t} {{ a = {arr(w, t)}; a[{n - 1}] }}",
        f"fn fw_{sfx}(a: [{t}; {n}]) -> {t} {{ let r = bv_{sfx}(a, 0_usize, {lit(v, t)}); a[0] }}",
    ]
    if not NO_GLOBAL:
        top += [
            f"static mut GB_{sfx}: [{t}; {n}] = {arr(x, t)};",
            f"fn al_{sfx}(a: [{t}; {n}]) -> {t} {{ unsafe {{ GB_{sfx}[0] = {lit(v, t)}; }} a[0] }}",
        ]
    xs = [f"x_{sfx}[{i}]" for i in range(n)]
    body += [
        f"    let x_{sfx}: [{t}; {n}] = {arr(x, t)};",
        f"    let r1_{sfx} = bv_{sfx}(x_{sfx}, {n - 1}_usize, {lit(v, t)});",
        f"    let r2_{sfx} = bw_{sfx}(x_{sfx});",
        f"    let r3_{sfx} = fw_{sfx}(x_{sfx});",
        f"    {pr([f'r1_{sfx}', f'r2_{sfx}', f'r3_{sfx}'] + xs)}",
    ]
    out.append(" ".join(str(z) for z in [v, w[-1], x[0]] + x))
    if NO_GLOBAL:
        return
    body += [
        f"    let g_{sfx} = unsafe {{ GB_{sfx} }};",
        f"    let r4_{sfx} = al_{sfx}(g_{sfx});",
        f"    let g0_{sfx} = unsafe {{ GB_{sfx}[0] }};",
        f"    {pr([f'r4_{sfx}', f'g0_{sfx}'])}",
    ]
    out.append(f"{x[0]} {v}")


def byval_extra(t, seed, top, body, out):
    s = [val(t, seed + i) for i in range(3)]
    m = [[val(t, seed + 10 + 2 * i + j) for j in range(2)] for i in range(3)]
    v = val(t, seed + 30)
    top += [
        f"struct R_{t} {{ a: u8, w: [{t}; 3], z: i64 }}",
        f"fn sv_{t}(mut s: R_{t}) -> {t} {{ s.w[1] = {lit(v, t)}; s.a = 9_u8; s.w[1] }}",
        f"fn nv_{t}(mut a: [[{t}; 2]; 3]) -> {t} {{ a[2][1] = {lit(v, t)}; a[2][1] }}",
        f"fn rt_{t}() -> [{t}; 3] {{ {arr(s, t)} }}",
    ]
    flat = [f"m_{t}[{i}][{j}]" for i in range(3) for j in range(2)]
    body += [
        f"    let s_{t} = R_{t} {{ a: 1_u8, w: {arr(s, t)}, z: -2_i64 }};",
        f"    let m_{t}: [[{t}; 2]; 3] = [{', '.join(arr(r, t) for r in m)}];",
        f"    let p_{t} = sv_{t}(s_{t});",
        f"    let q_{t} = nv_{t}(m_{t});",
        f"    let mut r_{t} = rt_{t}();",
        f"    r_{t}[0] = {lit(v, t)};",
        f"    let u_{t} = rt_{t}();",
        f"    {pr([f'p_{t}', f's_{t}.a', f's_{t}.w[0]', f's_{t}.w[1]', f's_{t}.w[2]', f's_{t}.z', f'q_{t}'] + flat)}",
        f"    {pr([f'r_{t}[0]', f'r_{t}[1]', f'u_{t}[0]', f'u_{t}[2]'])}",
    ]
    out.append(" ".join(str(z) for z in [v, 1, s[0], s[1], s[2], -2, v] + [e for r in m for e in r]))
    out.append(f"{v} {s[1]} {s[0]} {s[2]}")


def globals_case(t, n, seed, top, body, out):
    sfx = f"{t}_{n}"
    g = [val(t, seed + i) for i in range(n)]
    w = [val(t, seed + 20 + i) for i in range(n)]
    a, z = seed % 200, -(seed * 1000003)
    top += [
        f"static mut G_{sfx}: [{t}; {n}] = {arr(g, t)};",
        f"struct R_{sfx} {{ a: u8, w: [{t}; {n}], z: i64 }}",
        f"impl R_{sfx} {{ fn set(&mut self, k: usize, v: {t}) {{ self.w[k] = v; self.a = self.a ^ 1_u8; }} }}",
        f"static mut S_{sfx}: R_{sfx} = R_{sfx} {{ z: {lit(z, 'i64')}, w: {arr(w, t)}, a: {a}_u8 }};",
        f"fn pa_{sfx}(p: &mut [{t}; {n}], k: usize, v: {t}) {{ p[k] = v; }}",
        f"fn ps_{sfx}(p: &mut [{t}], k: usize, v: {t}) {{ p[k] = v; }}",
    ]
    gs = [f"G_{sfx}[{i}]" for i in range(n)]
    ss = [f"S_{sfx}.a"] + [f"S_{sfx}.w[{i}]" for i in range(n)] + [f"S_{sfx}.z"]
    body += [f"    unsafe {{ {pr(gs + ss)} }}"]
    out.append(" ".join(str(e) for e in g + [a] + w + [z]))
    k1, k2, k3 = 0, n - 1, n // 2
    v = [val(t, seed + 60 + i) for i in range(8)]
    body += [
        "    unsafe {",
        f"        G_{sfx}[{k1}] = {lit(v[0], t)};",
        f"        let j_{sfx} = {k2}_usize;",
        f"        G_{sfx}[j_{sfx}] ^= {lit(v[1], t)};",
        f"        pa_{sfx}(&mut G_{sfx}, {k3}_usize, {lit(v[2], t)});",
        f"        S_{sfx}.w[{k2}] = {lit(v[3], t)};",
        f"        S_{sfx}.z = {lit(z + 7, 'i64')};",
        f"        ps_{sfx}(&mut S_{sfx}.w, {k1}_usize, {lit(v[4], t)});",
        f"        S_{sfx}.set({k3}_usize, {lit(v[5], t)});",
        f"        S_{sfx}.w[j_{sfx}] |= {lit(v[6], t)};",
        f"        {pr(gs + ss)}",
        f"        G_{sfx} = {arr(w, t)};",
        f"        {pr(gs)}",
        "    }",
    ]
    g[k1] = v[0]
    g[k2] = wrap(g[k2] ^ v[1], t)
    g[k3] = v[2]
    w[k2], z = v[3], z + 7
    w[k1] = v[4]
    w[k3], a = v[5], a ^ 1
    w[k2] = wrap(w[k2] | v[6], t)
    out.append(" ".join(str(e) for e in g + [a] + w + [z]))
    out.append(" ".join(str(e) for e in [val(t, seed + 20 + i) for i in range(n)]))


def write(name, top, body, out):
    here = OUT_DIR
    src = "\n".join(top + ["fn main() ~ Console {"] + body + ["}"]) + "\n"
    with open(os.path.join(here, f"{name}.bld"), "w", newline="\n") as f:
        f.write(src)
    with open(os.path.join(here, f"{name}.expected"), "w", newline="\n") as f:
        f.write("\n".join(out) + "\n")


def main():
    programs = [("byval", byval_case, byval_extra), ("globals", globals_case, None)]
    for name, case, extra in programs[:1] if NO_GLOBAL else programs:
        top, body, out = [], [], []
        seed = 3
        for t in TYPES:
            for n in LENGTHS:
                case(t, n, seed, top, body, out)
                seed += 11
            if extra:
                extra(t, seed, top, body, out)
                seed += 11
        write(name, top, body, out)


if __name__ == "__main__":
    main()
