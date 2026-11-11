//! Fixed arrays passed by value, on the hosted C backend (G33).
//!
//! A C array parameter is a pointer, so before the fix a store into a `mut`
//! array parameter wrote into the caller's array, and `a = [..]` copied the
//! size of a pointer into it. Each test prints every element the caller
//! still holds beside the callee's result: a test that printed the result
//! alone would pass with the caller's array overwritten.
//!
//! The matrix `tests/fixtures/globals_values/byval.bld` covers every integer
//! width and lengths 1 to 8; its expected output is written by
//! `gen_matrix.py` from a Python model, not by the compiler.

mod support;

use support::{assert_prints, has_cc, repo_root, run_file};

/// The G33 reproduction. It printed `99 99 99` on main.
#[test]
fn a_store_into_a_mut_array_parameter_stays_in_the_callee() {
    if !has_cc() {
        return;
    }
    let src = "fn clobber(a: [u32; 3]) -> u32 { let mut b = a; b[0] = 99_u32; b[0] }\n\
               fn clobber2(mut a: [u32; 3]) -> u32 { a[0] = 99_u32; a[0] }\n\
               fn main() ~ Console {\n\
               let x: [u32; 3] = [1_u32, 2_u32, 3_u32];\n\
               let r = clobber(x);\n\
               let r2 = clobber2(x);\n\
               println!(\"{} {} {} {} {}\", r, r2, x[0], x[1], x[2]);\n\
               }\n";
    assert_prints("g33", src, "99 99 1 2 3\n");
}

/// `a = [..]` on a parameter copied eight bytes into the caller's `u64`
/// array (`6 4 1 1` on main).
#[test]
fn a_whole_array_assignment_to_a_parameter_stays_in_the_callee() {
    if !has_cc() {
        return;
    }
    let src =
        "fn whole(mut a: [u64; 3]) -> u64 { a = [4_u64, 5_u64, 6_u64]; a[0] + a[1] + a[2] }\n\
               fn main() ~ Console {\n\
               let x: [u64; 3] = [1_u64, 1_u64, 1_u64];\n\
               let s = whole(x);\n\
               println!(\"{} {} {} {}\", s, x[0], x[1], x[2]);\n\
               }\n";
    assert_prints("whole", src, "15 1 1 1\n");
}

/// A by-value array handed to a second by-value call, and a two-dimensional
/// one. Main printed `2000 1000` and `200 200`.
#[test]
fn forwarded_and_nested_array_parameters_are_copies() {
    if !has_cc() {
        return;
    }
    let src = "fn inner(mut a: [u32; 2]) -> u32 { a[0] = 1000_u32; a[0] }\n\
               fn outer(a: [u32; 2]) -> u32 { let r = inner(a); r + a[0] }\n\
               fn nested(mut a: [[u8; 2]; 3]) -> u8 { a[2][1] = 200_u8; a[2][1] }\n\
               fn main() ~ Console {\n\
               let y: [u32; 2] = [11_u32, 12_u32];\n\
               let n: [[u8; 2]; 3] = [[1_u8, 2_u8], [3_u8, 4_u8], [5_u8, 6_u8]];\n\
               println!(\"{} {} {}\", outer(y), y[0], y[1]);\n\
               println!(\"{} {} {}\", nested(n), n[2][1], n[0][0]);\n\
               }\n";
    assert_prints("forward", src, "1011 11 12\n200 6 1\n");
}

/// A struct with an array field passed by value and declared `mut`, and an
/// array returned by value then changed by the caller. Both were right on
/// main; they pin the rest of the class.
#[test]
fn structs_by_value_and_returned_arrays_are_copies() {
    if !has_cc() {
        return;
    }
    let src = "struct S { a: u8, w: [u16; 3], z: i64 }\n\
               fn smut(mut s: S) -> u16 { s.w[1] = 999_u16; s.a = 3_u8; s.w[1] }\n\
               fn ret() -> [u16; 3] { [1_u16, 2_u16, 3_u16] }\n\
               fn main() ~ Console {\n\
               let s = S { a: 1_u8, w: [7_u16, 8_u16, 9_u16], z: -2_i64 };\n\
               println!(\"{} {} {} {} {} {}\", smut(s), s.a, s.w[0], s.w[1], s.w[2], s.z);\n\
               let mut r = ret();\n\
               r[0] = 100_u16;\n\
               let r2 = ret();\n\
               println!(\"{} {} {}\", r[0], r2[0], r2[1]);\n\
               }\n";
    assert_prints("structs", src, "999 1 7 8 9 -2\n100 1 2\n");
}

/// The by-value matrix, every width and length, against the Python model.
#[test]
fn byval_matrix_matches_the_model() {
    if !has_cc() {
        return;
    }
    let dir = repo_root().join("compiler/tests/fixtures/globals_values");
    let expected = std::fs::read_to_string(dir.join("byval.expected"))
        .expect("expected file")
        .replace("\r\n", "\n");
    assert!(
        expected.lines().count() >= 90,
        "byval: expected file shrank"
    );
    let ran = run_file(&dir.join("byval.bld"));
    assert_eq!(ran.code, Some(0), "byval: {}", ran.stderr);
    support::assert_same_lines("byval", &ran.stdout, &expected);
}
