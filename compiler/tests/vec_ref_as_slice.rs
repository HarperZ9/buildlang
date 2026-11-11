//! `&v` with `v: Vec<T>` passed where a `&[T]` parameter is expected. The
//! vector handle used to be passed as it was, the callee read it as a slice,
//! and the program crashed at run time. These run real programs.

use std::path::PathBuf;
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn buildc(tag: &str, src: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("bl-vecslice-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// `buildc run` on `src`; returns stdout, and panics with stderr on failure.
fn run(tag: &str, src: &str) -> String {
    let out = buildc(tag, src);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

const SUM_U8: &str = "fn sum(b: &[u8]) -> u64 {\n\
                      let mut s: u64 = 0_u64;\n\
                      let mut i: usize = 0;\n\
                      while i < b.len() { s = s + b[i] as u64; i = i + 1; }\n\
                      s\n\
                      }\n";

/// The original reproduction: two bytes, sum 5. It crashed with an access
/// violation before the fix.
#[test]
fn a_borrowed_byte_vector_is_read_as_a_slice() {
    let src = format!(
        "{SUM_U8}fn main() ~ Console {{\n\
         let mut v: Vec<u8> = Vec::new();\n\
         v.push(2_u8);\n\
         v.push(3_u8);\n\
         println!(\"{{}}\", sum(&v));\n\
         }}\n"
    );
    assert_eq!(run("bytes", &src).trim(), "5");
}

/// The slice carries the vector's length at the call: empty, then after the
/// buffer has grown and moved.
#[test]
fn the_slice_length_follows_the_vector() {
    let src = format!(
        "{SUM_U8}fn main() ~ Console {{\n\
         let mut v: Vec<u8> = Vec::new();\n\
         println!(\"{{}}\", sum(&v));\n\
         v.push(250_u8);\n\
         println!(\"{{}}\", sum(&v));\n\
         let mut i: usize = 0;\n\
         while i < 1000 {{ v.push(1_u8); i = i + 1; }}\n\
         println!(\"{{}}\", sum(&v));\n\
         }}\n"
    );
    assert_eq!(run("length", &src).trim(), "0\n250\n1250");
}

/// Eight-byte and floating elements keep their width through the view.
#[test]
fn wide_and_float_elements_keep_their_width() {
    let src = "fn sum64(b: &[u64]) -> u64 {\n\
               let mut s: u64 = 0_u64;\n\
               let mut i: usize = 0;\n\
               while i < b.len() { s = s + b[i]; i = i + 1; }\n\
               s\n\
               }\n\
               fn total(b: &[f64]) -> f64 {\n\
               let mut s: f64 = 0.0;\n\
               let mut i: usize = 0;\n\
               while i < b.len() { s = s + b[i]; i = i + 1; }\n\
               s\n\
               }\n\
               fn main() ~ Console {\n\
               let mut w: Vec<u64> = Vec::new();\n\
               w.push(1_u64 << 40);\n\
               w.push(5_u64);\n\
               let mut f: Vec<f64> = Vec::new();\n\
               f.push(1.5);\n\
               f.push(2.25);\n\
               println!(\"{} {}\", sum64(&w), total(&f));\n\
               }\n";
    assert_eq!(run("wide", src).trim(), "1099511627781 3.75");
}

/// A write through `&mut [T]` lands in the vector's own buffer.
#[test]
fn a_mutable_slice_writes_into_the_vector() {
    let src = "fn bump(b: &mut [i32]) {\n\
               let mut i: usize = 0;\n\
               while i < b.len() { b[i] = b[i] + 10; i = i + 1; }\n\
               }\n\
               fn main() ~ Console {\n\
               let mut m: Vec<i32> = Vec::new();\n\
               m.push(1);\n\
               m.push(2);\n\
               bump(&mut m);\n\
               println!(\"{} {}\", m[0], m[1]);\n\
               }\n";
    assert_eq!(run("mutable", src).trim(), "11 12");
}

/// An index past the vector's length is caught by the slice bounds check.
#[test]
fn an_index_past_the_length_is_refused_at_run_time() {
    let src = "fn third(b: &[u8]) -> u8 { b[2] }\n\
               fn main() ~ Console {\n\
               let mut v: Vec<u8> = Vec::new();\n\
               v.push(1_u8);\n\
               v.push(2_u8);\n\
               println!(\"{}\", third(&v));\n\
               }\n";
    let out = buildc("bounds", src);
    assert!(
        !out.status.success(),
        "an out-of-bounds read must not succeed"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("out of bounds"), "stderr: {stderr}");
}

/// A vector of one element type is not a slice of another: the bytes would be
/// read at the wrong width.
#[test]
fn a_vector_of_another_element_type_is_rejected() {
    let src = format!(
        "{SUM_U8}fn main() ~ Console {{\n\
         let mut v: Vec<i32> = Vec::new();\n\
         v.push(2);\n\
         println!(\"{{}}\", sum(&v));\n\
         }}\n"
    );
    let out = buildc("mismatch", &src);
    assert!(!out.status.success(), "a width mismatch must not build");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("this vector holds") && stderr.contains("slice of"),
        "stderr: {stderr}"
    );
}

/// A fixed array still becomes a slice the way it did.
#[test]
fn a_borrowed_array_still_works() {
    let src = format!(
        "{SUM_U8}fn main() ~ Console {{\n\
         let a: [u8; 3] = [4_u8, 5_u8, 6_u8];\n\
         println!(\"{{}}\", sum(&a));\n\
         }}\n"
    );
    assert_eq!(run("array", &src).trim(), "15");
}

/// The vector may be a struct field. This crashed the same way.
#[test]
fn a_vector_in_a_struct_field_is_read_as_a_slice() {
    let src = format!(
        "struct Bag {{ items: Vec<u8> }}\n\
         {SUM_U8}fn main() ~ Console {{\n\
         let mut bag = Bag {{ items: Vec::new() }};\n\
         bag.items.push(7_u8);\n\
         bag.items.push(8_u8);\n\
         println!(\"{{}}\", sum(&bag.items));\n\
         }}\n"
    );
    assert_eq!(run("field", &src).trim(), "15");
}

/// `let s: &[T] = &v;` binds the same slice view. With a vector this crashed.
#[test]
fn a_let_bound_slice_of_a_vector_works() {
    let src = format!(
        "{SUM_U8}fn main() ~ Console {{\n\
         let mut v: Vec<u8> = Vec::new();\n\
         v.push(7_u8);\n\
         v.push(9_u8);\n\
         let s: &[u8] = &v;\n\
         println!(\"{{}} {{}} {{}}\", sum(s), s[1], s.len());\n\
         }}\n"
    );
    assert_eq!(run("letvec", &src).trim(), "16 9 2");
}

/// With a fixed array the same binding gave a slice of length 0: the sum was
/// 0 and any index was out of bounds.
#[test]
fn a_let_bound_slice_of_an_array_has_the_array_length() {
    let src = format!(
        "{SUM_U8}fn main() ~ Console {{\n\
         let a: [u8; 3] = [4_u8, 5_u8, 6_u8];\n\
         let s: &[u8] = &a;\n\
         println!(\"{{}} {{}} {{}}\", sum(s), s[1], s.len());\n\
         }}\n"
    );
    assert_eq!(run("letarr", &src).trim(), "15 5 3");
}
