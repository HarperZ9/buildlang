//! Fixed arrays held inside structs and inside other arrays, on the hosted C
//! backend. Each of these built and ran before the fix and printed wrong
//! values or skipped a check, with no diagnostic:
//!
//! - G26: a struct literal with an array field beside other fields put the
//!   array's address in its first element and shifted the later fields.
//! - G27: `q.w[1] = 42` stored into a copy of the array and was lost.
//! - G28: `arr[9].y = v` on an array of two structs was not bounds-checked.
//! - G7: `f(&mut s.w)` did not pass a slice of the field.
//!
//! The matrix programs under `tests/fixtures/struct_arrays` cover every
//! integer width, array length and field position; their expected output is
//! written by `gen_matrix.py` from a Python model, not by the compiler.
//!
//! Every test prints all elements of the arrays it touches and the fields
//! beside them: a test that printed one element would pass with a shifted
//! array. The bounds tests pair every trap with the same store in range, so
//! a compiler that trapped on every store would fail them too.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn has_cc() -> bool {
    ["cc", "gcc", "clang"].iter().any(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

struct Ran {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// `buildc run` on a file; standard output with `\r\n` read as `\n`.
fn run_file(file: &std::path::Path) -> Ran {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(file)
        .output()
        .expect("buildc runs");
    Ran {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    }
}

fn run(tag: &str, src: &str) -> Ran {
    let dir = std::env::temp_dir().join(format!("bl-structarr-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let ran = run_file(&file);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

/// The program must exit 0 and print exactly `expected`.
fn assert_prints(tag: &str, src: &str, expected: &str) {
    let ran = run(tag, src);
    assert_eq!(ran.code, Some(0), "{tag}: stderr: {}", ran.stderr);
    assert_eq!(ran.stdout, expected, "{tag}: stderr: {}", ran.stderr);
}

/// The G26 reproduction. It printed `7 1981806948 4294967287 0 0`.
#[test]
fn g26_a_struct_literal_keeps_its_array_field_and_the_fields_beside_it() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let src = "struct P { tag: u8, w: [u32; 3], z: i64 }\n\
               fn main() ~ Console {\n\
               let p = P { tag: 7_u8, w: [1_u32, 2_u32, 3_u32], z: -9 };\n\
               println!(\"{} {} {} {} {}\", p.tag, p.w[0], p.w[1], p.w[2], p.z);\n\
               }\n";
    assert_prints("g26", src, "7 1 2 3 -9\n");
}

/// The G27 reproduction, printing the neighbours of the stored element too.
/// It printed the old value, 2.
#[test]
fn g27_a_store_into_an_array_field_lands_in_the_struct() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let src = "struct Q { w: [u32; 3] }\n\
               fn main() ~ Console {\n\
               let mut q = Q { w: [1_u32, 2_u32, 3_u32] };\n\
               q.w[1] = 42_u32;\n\
               println!(\"{} {} {}\", q.w[0], q.w[1], q.w[2]);\n\
               }\n";
    assert_prints("g27", src, "1 42 3\n");
}

/// A store through `&mut self`, a `&mut` parameter and a nested array, and a
/// copy of the struct taken before the stores, which must keep its values.
#[test]
fn stores_reach_the_array_through_references_and_leave_copies_alone() {
    if !has_cc() {
        return;
    }
    let src = "struct S { a: i8, w: [i16; 3], z: u64 }\n\
               impl S { fn put(&mut self, i: usize, v: i16) { self.w[i] = v; } }\n\
               fn set(s: &mut S, i: usize, v: i16) { s.w[i] = v; }\n\
               fn main() ~ Console {\n\
               let mut s = S { a: -7_i8, w: [-1_i16, 2_i16, -3_i16], z: 18446744073709551615_u64 };\n\
               let before = s;\n\
               s.put(0, 300_i16);\n\
               set(&mut s, 2, -300_i16);\n\
               s.w[1] += 5_i16;\n\
               let mut g: [[u8; 2]; 2] = [[1_u8, 2_u8], [3_u8, 4_u8]];\n\
               g[1][0] = 9_u8;\n\
               println!(\"{} {} {} {} {}\", s.a, s.w[0], s.w[1], s.w[2], s.z);\n\
               println!(\"{} {} {} {} {}\", before.a, before.w[0], before.w[1], before.w[2], before.z);\n\
               println!(\"{} {} {} {}\", g[0][0], g[0][1], g[1][0], g[1][1]);\n\
               }\n";
    assert_prints(
        "refs",
        src,
        "-7 300 7 -300 18446744073709551615\n-7 -1 2 -3 18446744073709551615\n1 2 9 4\n",
    );
}

/// G7: a field array borrowed as a slice is the field itself, with its
/// length, for reading and for writing.
#[test]
fn g7_a_field_array_borrowed_as_a_slice_is_the_field() {
    if !has_cc() {
        return;
    }
    let src = "struct S { a: u8, w: [u32; 3], z: i64 }\n\
               fn sum(b: &[u32]) -> u32 { let mut s: u32 = 0_u32; let mut i: usize = 0;\n\
               while i < b.len() { s = s + b[i]; i = i + 1; } s }\n\
               fn bump(b: &mut [u32]) { let mut i: usize = 0;\n\
               while i < b.len() { b[i] = b[i] + 10_u32; i = i + 1; } }\n\
               fn via(s: &mut S) -> u32 { bump(&mut s.w); sum(&s.w) }\n\
               fn main() ~ Console {\n\
               let mut s = S { a: 1_u8, w: [1_u32, 2_u32, 3_u32], z: 5 };\n\
               bump(&mut s.w);\n\
               println!(\"{}\", sum(&s.w));\n\
               println!(\"{}\", via(&mut s));\n\
               println!(\"{} {} {} {} {}\", s.a, s.w[0], s.w[1], s.w[2], s.z);\n\
               }\n";
    assert_prints("g7", src, "36\n66\n1 21 22 23 5\n");
}

/// `S { a: 9, ..base }` takes every field it does not write from `base`, an
/// array among them. The `..base` part was ignored: the reproduction
/// `T { b: 20, ..t }` printed `0 20 0`. A literal that writes every field
/// out of order and still names a base keeps each value with its own field.
#[test]
fn a_struct_update_literal_takes_the_other_fields_from_its_base() {
    if !has_cc() {
        return;
    }
    let src = "struct S { a: u8, w: [u16; 3], b: i32, c: u8 }\n\
               struct T { a: u8, b: u8, c: u8 }\n\
               fn main() ~ Console {\n\
               let o = S { a: 1_u8, w: [4_u16, 5_u16, 6_u16], b: -2, c: 3_u8 };\n\
               let n = S { a: 9_u8, ..o };\n\
               let m = S { c: 7_u8, w: [8_u16, 9_u16, 10_u16], ..o };\n\
               let all = S { c: 11_u8, b: 12, w: [13_u16, 14_u16, 15_u16], a: 16_u8, ..o };\n\
               let t = T { a: 1_u8, b: 2_u8, c: 3_u8 };\n\
               let u = T { b: 20_u8, ..t };\n\
               println!(\"{} {} {} {} {} {}\", n.a, n.w[0], n.w[1], n.w[2], n.b, n.c);\n\
               println!(\"{} {} {} {} {} {}\", m.a, m.w[0], m.w[1], m.w[2], m.b, m.c);\n\
               println!(\"{} {} {} {} {} {}\", all.a, all.w[0], all.w[1], all.w[2], all.b, all.c);\n\
               println!(\"{} {} {} {} {} {}\", o.a, o.w[0], o.w[1], o.w[2], o.b, o.c);\n\
               println!(\"{} {} {}\", u.a, u.b, u.c);\n\
               }\n";
    assert_prints(
        "update",
        src,
        "9 4 5 6 -2 3\n1 8 9 10 -2 7\n16 13 14 15 12 11\n1 4 5 6 -2 3\n1 20 3\n",
    );
}

/// One store per shape of place: (name, statement, length of the indexed
/// array, the values printed after the store when the index is 1). In range
/// the program prints every array; out of range it must stop with exit code
/// 101 before that line. The values are `arr`, `guard`, `q.w`, then `g`.
const BOUNDS_SHAPES: &[(&str, &str, u32, &str)] = &[
    (
        "field_of_element",
        "arr[i].y = -7_i8;",
        2,
        "1 2 3 -7 5 6 1 2 3 1 2 3 4",
    ),
    (
        "element_of_field",
        "q.w[i] = 42_u32;",
        3,
        "1 2 3 4 5 6 1 42 3 1 2 3 4",
    ),
    (
        "nested_array",
        "g[i][0] = 9_u8;",
        2,
        "1 2 3 4 5 6 1 2 3 1 2 9 4",
    ),
    (
        "through_mut_param",
        "set(&mut q, i, 42_u32);",
        3,
        "1 2 3 4 5 6 1 42 3 1 2 3 4",
    ),
    (
        "slice_of_structs",
        "poke(&mut arr, i, -7_i8);",
        2,
        "1 2 3 -7 5 6 1 2 3 1 2 3 4",
    ),
    (
        "compound",
        "q.w[i] += 1_u32;",
        3,
        "1 2 3 4 5 6 1 3 3 1 2 3 4",
    ),
];

fn bounds_program(stmt: &str, index: u32) -> String {
    format!(
        "struct In {{ x: u16, y: i8 }}\n\
         struct Q {{ w: [u32; 3] }}\n\
         fn set(q: &mut Q, i: usize, v: u32) {{ q.w[i] = v; }}\n\
         fn poke(xs: &mut [In], i: usize, v: i8) {{ xs[i].y = v; }}\n\
         fn main() ~ Console {{\n\
         let mut arr: [In; 2] = [In {{ x: 1_u16, y: 2_i8 }}, In {{ x: 3_u16, y: 4_i8 }}];\n\
         let mut guard: [u64; 2] = [5_u64, 6_u64];\n\
         let mut q = Q {{ w: [1_u32, 2_u32, 3_u32] }};\n\
         let mut g: [[u8; 2]; 2] = [[1_u8, 2_u8], [3_u8, 4_u8]];\n\
         let i: usize = {index};\n\
         println!(\"before\");\n\
         {stmt}\n\
         println!(\"after {{}} {{}} {{}} {{}} {{}} {{}}\", arr[0].x, arr[0].y, arr[1].x, arr[1].y, \
         guard[0], guard[1]);\n\
         println!(\"{{}} {{}} {{}} {{}} {{}} {{}} {{}}\", q.w[0], q.w[1], q.w[2], \
         g[0][0], g[0][1], g[1][0], g[1][1]);\n\
         }}\n"
    )
}

/// G28 and its class. The reproduction is `field_of_element` with index 9.
#[test]
fn g28_a_store_through_an_index_is_bounds_checked_in_every_shape() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    for (name, stmt, len, in_range) in BOUNDS_SHAPES {
        let ok = run(&format!("ok-{name}"), &bounds_program(stmt, 1));
        assert_eq!(ok.code, Some(0), "{name} in range: {}", ok.stderr);
        // Two lines are printed after the store: six values, then seven.
        let printed: Vec<&str> = ok.stdout.split_whitespace().collect();
        let mut wanted = vec!["before", "after"];
        wanted.extend(in_range.split(' '));
        assert_eq!(printed, wanted, "{name} in range: {:?}", ok.stdout);

        for index in [*len, 9] {
            let trap = run(
                &format!("trap-{name}-{index}"),
                &bounds_program(stmt, index),
            );
            assert_eq!(
                trap.code,
                Some(101),
                "{name} index {index} must trap: stdout {:?} stderr {}",
                trap.stdout,
                trap.stderr
            );
            assert_eq!(trap.stdout, "before\n", "{name} index {index}");
            let message = format!("index out of bounds: the len is {len} but the index is {index}");
            assert!(trap.stderr.contains(&message), "{name}: {}", trap.stderr);
        }
    }
}

/// The matrix programs: every integer width, array length and field position
/// for literals and stores, nested shapes, and slices of field arrays. The
/// expected files come from the Python model in `gen_matrix.py`.
#[test]
fn the_matrix_programs_print_what_the_python_model_computed() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let dir = repo_root().join("compiler/tests/fixtures/struct_arrays");
    for name in ["literals", "stores", "nesting", "slices"] {
        let expected = std::fs::read_to_string(dir.join(format!("{name}.expected")))
            .expect("expected file")
            .replace("\r\n", "\n");
        assert!(
            expected.lines().count() >= 80,
            "{name}: expected file shrank"
        );
        let ran = run_file(&dir.join(format!("{name}.bld")));
        assert_eq!(ran.code, Some(0), "{name}: {}", ran.stderr);
        if ran.stdout != expected {
            let wrong = ran
                .stdout
                .lines()
                .zip(expected.lines())
                .filter(|(got, want)| got != want)
                .take(5)
                .map(|(got, want)| format!("  got  {got}\n  want {want}"))
                .collect::<Vec<_>>()
                .join("\n");
            panic!(
                "{name}: {} lines printed, {} expected; first differences:\n{wrong}",
                ran.stdout.lines().count(),
                expected.lines().count()
            );
        }
    }
}

/// What the C backend cannot yet build is refused with a diagnostic. It
/// compiled before and printed garbage.
#[test]
fn shapes_that_cannot_be_built_are_refused_not_miscompiled() {
    let tuple = "fn main() ~ Console {\n\
                 let t = (7_u8, [1_u32, 2_u32, 3_u32], -9);\n\
                 println!(\"{} {}\", t.0, t.1[0]);\n\
                 }\n";
    let ran = run("tuple", tuple);
    assert_ne!(ran.code, Some(0), "stdout: {}", ran.stdout);
    assert!(
        ran.stderr.contains("a tuple with a fixed array inside it"),
        "{}",
        ran.stderr
    );
}

/// A field store into a global struct was dropped, then refused (PR 254). It
/// now lands in the global (G35); `compiler/tests/global_stores.rs` covers the
/// class.
#[test]
fn a_field_store_into_a_global_struct_lands() {
    let global = "struct S { a: u8, b: u8 }\n\
                  static mut G: S = S { a: 1_u8, b: 2_u8 };\n\
                  fn main() ~ Console {\n\
                  unsafe { G.a = 5_u8; println!(\"{} {}\", G.a, G.b); }\n\
                  }\n";
    let ran = run("global", global);
    assert_eq!(ran.code, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stdout.replace("\r\n", "\n"), "5 2\n");
}
