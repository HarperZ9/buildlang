//! Arrays and structs held in `static mut` globals, on the hosted C backend.
//!
//! - G34: an array global was declared `uint32_t[3] T;` (not C) and its
//!   initializer was dropped; a static struct literal written out of field
//!   order was emitted in the order written, so the fields swapped silently.
//! - G35: nothing could address a global. `&mut G` borrowed a copy, a slice
//!   of a global's array field was a copy, `G.a = 5` was refused, and
//!   `G[i] = v` on a global array had no bounds check.
//! - A `&mut self` method called on a field, an element or a global borrowed
//!   a copy and lost its writes (silent on main for locals too).
//!
//! Every bounds test pairs the trap with the same store in range, so a
//! compiler that trapped on every store fails too. The matrix
//! `tests/fixtures/globals_values/globals.bld` covers every integer width and
//! lengths 1 to 8 against a Python model.

mod support;

use support::{assert_prints, assert_traps, has_cc, repo_root, run, run_file};

/// The G34 reproduction (`static mut T: [u32; 3] = [7, 8, 9]` failed in the
/// C compiler), with every element printed.
#[test]
fn a_global_array_keeps_its_initializer_and_takes_a_store() {
    if !has_cc() {
        return;
    }
    let src = "static mut T: [u32; 3] = [7_u32, 8_u32, 9_u32];\n\
               const C: [i8; 3] = [(-128_i8), 0_i8, 127_i8];\n\
               static mut Z: [u64; 4096] = [0_u64; 4096];\n\
               fn main() ~ Console { unsafe {\n\
               T[2] = 90_u32; Z[4095] = 5_u64;\n\
               println!(\"{} {} {} {} {} {} {} {}\", T[0], T[1], T[2], C[0], C[1], C[2], Z[0], Z[4095]);\n\
               } }\n";
    assert_prints("g34", src, "7 8 90 -128 0 127 0 5\n");
}

/// `T { z: 5, a: 1 }` was `{ 5, 1 }`: `a` became 5. Struct and tuple.
#[test]
fn a_global_struct_literal_takes_its_fields_by_name() {
    if !has_cc() {
        return;
    }
    let src = "struct T { a: u32, z: i64 }\n\
               static mut TB: T = T { z: 5_i64, a: 1_u32 };\n\
               const K: T = T { z: -7_i64, a: 3_u32 };\n\
               static mut TU: (u8, i32) = (1_u8, -2_i32);\n\
               fn main() ~ Console { let k = K; unsafe {\n\
               println!(\"{} {} {} {} {} {}\", TB.a, TB.z, k.a, k.z, TU.0, TU.1);\n\
               } }\n";
    assert_prints("order", src, "1 5 3 -7 1 -2\n");
}

/// An aggregate initializer that is not a constant is refused by name; the
/// global used to be zero-filled.
#[test]
fn an_initializer_that_is_not_a_constant_is_refused() {
    let src = "static mut T: [u32; 2] = [1_u32 + 1_u32, 2_u32];\n\
               fn main() ~ Console { unsafe { println!(\"{}\", T[0]); } }\n";
    let ran = run("noconst", src);
    assert_ne!(ran.code, Some(0), "stdout: {}", ran.stdout);
    assert!(
        ran.stderr
            .contains("the initializer of `T` is not a constant"),
        "{}",
        ran.stderr
    );
}

/// `&mut G` borrowed a copy (`1 5` on main) and `fill(&mut G.w)` crashed.
#[test]
fn a_borrow_of_a_global_writes_the_global() {
    if !has_cc() {
        return;
    }
    let src = "struct T { a: u32, w: [u16; 3], z: i64 }\n\
               static mut TB: T = T { a: 1_u32, w: [0_u16, 0_u16, 0_u16], z: 5_i64 };\n\
               static mut CNT: u64 = 7_u64;\n\
               fn bump(t: &mut T) { t.a = t.a + 10_u32; }\n\
               fn fill(s: &mut [u16]) { s[0] = 41_u16; }\n\
               fn main() ~ Console { unsafe {\n\
               bump(&mut TB); fill(&mut TB.w);\n\
               let p = &mut CNT; *p = 9_u64;\n\
               println!(\"{} {} {} {} {} {}\", TB.a, TB.w[0], TB.w[1], TB.w[2], TB.z, CNT);\n\
               } }\n";
    assert_prints("borrow", src, "11 41 0 0 5 9\n");
}

/// Stores into fields, elements, nested arrays and arrays of structs of
/// globals, with compound assignment. `G.a = 5` was refused by PR 254.
#[test]
fn stores_into_global_fields_and_elements_land() {
    if !has_cc() {
        return;
    }
    let src = "struct P { x: i16, y: u64 }\n\
               struct T { n: u8, w: [u32; 4], ps: [P; 2], k: i64 }\n\
               static mut TB: T = T { k: -1_i64, n: 2_u8, w: [1_u32, 2_u32, 3_u32, 4_u32], ps: [P { y: 9_u64, x: -3_i16 }, P { x: 4_i16, y: 5_u64 }] };\n\
               static mut GA: [[u8; 2]; 3] = [[1_u8, 2_u8], [3_u8, 4_u8], [5_u8, 6_u8]];\n\
               fn setp(p: &mut P) { p.x = p.x - 100_i16; }\n\
               fn main() ~ Console { unsafe {\n\
               TB.n = 200_u8; TB.w[2] = 30_u32; TB.w[1] += 5_u32; TB.ps[1].y = 77_u64;\n\
               TB.k = TB.k * 3_i64; setp(&mut TB.ps[0]);\n\
               GA[2][1] = 60_u8; GA[0] = [9_u8, 8_u8];\n\
               println!(\"{} {} {} {} {} {} {} {} {}\", TB.n, TB.w[0], TB.w[1], TB.w[2], TB.w[3], TB.ps[0].x, TB.ps[0].y, TB.ps[1].y, TB.k);\n\
               println!(\"{} {} {} {} {} {}\", GA[0][0], GA[0][1], GA[1][0], GA[1][1], GA[2][0], GA[2][1]);\n\
               } }\n";
    assert_prints("stores", src, "200 1 7 30 4 -103 9 77 -3\n9 8 3 4 5 60\n");
}

const BOUNDS_HEAD: &str = "struct P { x: i16, y: u64 }\n\
                           struct T { n: u8, w: [u32; 4] }\n\
                           static mut TB: T = T { n: 1_u8, w: [0_u32; 4] };\n\
                           static mut GA: [u64; 3] = [0_u64; 3];\n\
                           static mut PS: [P; 2] = [P { x: 0_i16, y: 0_u64 }; 2];\n\
                           fn main() ~ Console { let mut i = 0_usize; i = i + ";

/// Every index into a global is checked, store and read, and the same
/// statement with an index in range runs.
#[test]
fn indexes_into_globals_are_bounds_checked() {
    if !has_cc() {
        return;
    }
    let cases = [
        ("gstore", "GA[i] = 7_u64;", "GA[i]", 3),
        ("fstore", "TB.w[i] = 7_u32;", "TB.w[i]", 4),
        ("sstore", "PS[i].y = 7_u64;", "PS[i].y", 2),
        ("gread", "GA[0] = GA[i];", "GA[0]", 3),
    ];
    for (tag, stmt, read, len) in cases {
        let ok = format!(
            "{BOUNDS_HEAD}{}_usize; unsafe {{ {stmt} println!(\"{{}}\", {read}); }} }}\n",
            len - 1
        );
        let want = if tag == "gread" { "0\n" } else { "7\n" };
        assert_prints(tag, &ok, want);
        let bad = format!(
            "{BOUNDS_HEAD}{len}_usize; unsafe {{ {stmt} println!(\"{{}}\", {read}); }} }}\n"
        );
        let msg = format!("index out of bounds: the len is {len} but the index is {len}");
        assert_traps(tag, &bad, "", &msg);
    }
}

/// A `&mut self` method on a field, an element and a global lost its writes:
/// main printed `1 7` and `1 0 1`.
#[test]
fn a_mut_self_method_on_a_projection_or_a_global_writes_in_place() {
    if !has_cc() {
        return;
    }
    let src = "struct C { a: u32, w: [u8; 3] }\n\
               impl C { fn inc(&mut self) { self.a = self.a + 1_u32; self.w[2] = 9_u8; }\n\
               fn get(&self) -> u32 { self.a } }\n\
               struct O { c: C, cs: [C; 2] }\n\
               static mut TB: C = C { a: 1_u32, w: [0_u8; 3] };\n\
               fn main() ~ Console {\n\
               let mut o = O { c: C { a: 1_u32, w: [0_u8; 3] }, cs: [C { a: 5_u32, w: [0_u8; 3] }; 2] };\n\
               o.c.inc(); o.cs[1].inc();\n\
               println!(\"{} {} {} {} {}\", o.c.a, o.c.w[2], o.cs[0].a, o.cs[1].a, o.cs[1].w[2]);\n\
               unsafe { TB.inc(); println!(\"{} {} {}\", TB.a, TB.w[2], TB.get()); }\n\
               }\n";
    assert_prints("receiver", src, "2 9 5 6 9\n2 9 2\n");
}

/// The globals matrix, every width and length, against the Python model.
#[test]
fn globals_matrix_matches_the_model() {
    if !has_cc() {
        return;
    }
    let dir = repo_root().join("compiler/tests/fixtures/globals_values");
    let expected = std::fs::read_to_string(dir.join("globals.expected"))
        .expect("expected file")
        .replace("\r\n", "\n");
    assert!(
        expected.lines().count() >= 110,
        "globals: expected file shrank"
    );
    let ran = run_file(&dir.join("globals.bld"));
    assert_eq!(ran.code, Some(0), "globals: {}", ran.stderr);
    support::assert_same_lines("globals", &ran.stdout, &expected);
}
