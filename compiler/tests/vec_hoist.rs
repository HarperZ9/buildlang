//! Vector accesses as plain C array reads (the C backend's `c/vec_hoist.rs`).
//!
//! A function that cannot resize or replace a vector loads the vector's buffer
//! and length once and indexes them directly. These cases hold the contract:
//! the same output, the same bounds checks with the same messages and exit
//! code, and no hoisting in a function that can resize a vector.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
    c: String,
}

/// Compile `src` to C and run it; `None` when no C compiler is available.
fn run(tag: &str, src: &str) -> Option<Out> {
    let dir = std::env::temp_dir().join(format!("bl-vhoist-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let cfile = dir.join("main.c");
    let emit = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("-o")
        .arg(&cfile)
        .arg(&file)
        .output()
        .expect("buildc runs");
    assert!(
        emit.status.success(),
        "{}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let c = std::fs::read_to_string(&cfile).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    if stderr.contains("no C compiler") {
        return None;
    }
    Some(Out {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr,
        c,
    })
}

/// The C text of function `name`, from its definition to the next blank-line
/// separated top-level item.
fn c_fn<'a>(c: &'a str, name: &str) -> &'a str {
    let pat = format!(" {name}(");
    let head = c
        .match_indices(&pat)
        .map(|(i, _)| i)
        .find(|&i| {
            c[i..]
                .lines()
                .next()
                .is_some_and(|l| l.trim_end().ends_with('{'))
        })
        .expect("function definition");
    let end = c[head..].find("\n}\n").map(|e| head + e).unwrap_or(c.len());
    &c[head..end]
}

const PREFIX: &str = "struct P { x: f64, v: f64 }\n\
fn step(b: &mut Vec<P>, dt: f64) {\n\
\x20   let mut i = 0;\n\
\x20   while i < 3 {\n\
\x20       let p = b[i];\n\
\x20       b[i] = P { x: p.x + dt * p.v, v: p.v };\n\
\x20       i = i + 1;\n\
\x20   }\n\
}\n\
fn total(b: &Vec<P>) -> f64 {\n\
\x20   let mut s = 0.0;\n\
\x20   let mut i = 0;\n\
\x20   while i < 3 { s = s + sqrt(b[i].x * b[i].x); i = i + 1; }\n\
\x20   s\n\
}\n\
fn grow(b: &mut Vec<P>) { b.push(P { x: 0.0, v: 0.0 }); }\n";

fn main_with(body: &str) -> String {
    format!(
        "{PREFIX}fn main() ~ Console {{\n\
         \x20   let mut b: Vec<P> = Vec::new();\n\
         \x20   b.push(P {{ x: 1.0, v: 1.0 }});\n\
         \x20   b.push(P {{ x: 2.0, v: 2.0 }});\n\
         \x20   b.push(P {{ x: 3.0, v: 3.0 }});\n\
         {body}\n}}\n"
    )
}

#[test]
fn read_write_functions_are_hoisted_and_compute_the_same() {
    let src = main_with("    step(&mut b, 0.5);\n    println!(\"{:.3}\", total(&b));\n    grow(&mut b);\n    println!(\"{}\", b.len());");
    let Some(o) = run("same", &src) else { return };
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout, "9.000\n4\n");
    assert!(
        c_fn(&o.c, "step").contains("bl_vat_set(bl_vv_"),
        "step is hoisted"
    );
    assert!(
        c_fn(&o.c, "total").contains("bl_vat(bl_vv_"),
        "total is hoisted"
    );
    assert!(
        !c_fn(&o.c, "grow").contains("bl_vv_"),
        "a function that pushes is not hoisted"
    );
}

#[test]
fn out_of_bounds_read_keeps_the_message_and_exit_code() {
    let src = format!(
        "{PREFIX}fn at(b: &Vec<P>, i: i64) -> f64 {{ b[i].x }}\n\
         fn main() ~ Console {{\n\
         \x20   let mut b: Vec<P> = Vec::new();\n\
         \x20   b.push(P {{ x: 1.0, v: 1.0 }});\n\
         \x20   println!(\"{{}}\", at(&b, 0));\n\
         \x20   println!(\"{{}}\", at(&b, 5));\n\
         }}\n"
    );
    let Some(o) = run("oobr", &src) else { return };
    assert!(c_fn(&o.c, "at").contains("bl_vat(bl_vv_"), "at is hoisted");
    assert_eq!(o.stdout, "1\n");
    assert_eq!(o.code, 101);
    assert!(
        o.stderr
            .contains("index out of bounds: the len is 1 but the index is 5"),
        "{}",
        o.stderr
    );
}

#[test]
fn out_of_bounds_struct_write_keeps_the_message_and_exit_code() {
    let src = format!(
        "{PREFIX}fn put(b: &mut Vec<P>, i: i64) {{ b[i] = P {{ x: 9.0, v: 9.0 }}; }}\n\
         fn main() ~ Console {{\n\
         \x20   let mut b: Vec<P> = Vec::new();\n\
         \x20   b.push(P {{ x: 1.0, v: 1.0 }});\n\
         \x20   put(&mut b, 0);\n\
         \x20   println!(\"{{}}\", b[0].x);\n\
         \x20   put(&mut b, 3);\n\
         }}\n"
    );
    let Some(o) = run("oobw", &src) else { return };
    assert!(
        c_fn(&o.c, "put").contains("bl_vat_set(bl_vv_"),
        "put is hoisted"
    );
    assert_eq!(o.stdout, "9\n");
    assert_eq!(o.code, 101);
    assert!(
        o.stderr.contains("Vec index out of bounds: 3 >= 1"),
        "{}",
        o.stderr
    );
}

#[test]
fn scalar_vectors_are_hoisted_with_their_checks() {
    let src = "fn dot(a: &Vec<f64>, b: &Vec<f64>, n: i64) -> f64 {\n\
               \x20   let mut s = 0.0;\n\
               \x20   let mut i = 0;\n\
               \x20   while i < n { s = s + a[i] * b[i]; i = i + 1; }\n\
               \x20   s\n\
               }\n\
               fn main() ~ Console {\n\
               \x20   let mut a: Vec<f64> = Vec::new();\n\
               \x20   let mut b: Vec<f64> = Vec::new();\n\
               \x20   a.push(1.0); a.push(2.0); b.push(3.0); b.push(4.0);\n\
               \x20   println!(\"{}\", dot(&a, &b, 2));\n\
               \x20   println!(\"{}\", dot(&a, &b, 3));\n\
               }\n";
    let Some(o) = run("f64", src) else { return };
    assert!(
        c_fn(&o.c, "dot").contains("bl_vat(bl_vv_"),
        "dot is hoisted"
    );
    assert_eq!(o.stdout, "11\n");
    assert_eq!(o.code, 101);
    assert!(
        o.stderr
            .contains("index out of bounds: the len is 2 but the index is 2"),
        "{}",
        o.stderr
    );
}

#[test]
fn a_call_to_a_vector_free_function_keeps_hoisting() {
    // spectral-norm's shape: the loop calls a scalar user function.
    let src =
        "fn a(i: i64, j: i64) -> f64 { 1.0 / (((i + j) * (i + j + 1) / 2 + i + 1) as f64) }\n\
               fn row(n: i64, v: &Vec<f64>, i: i64) -> f64 {\n\
               \x20   let mut s = 0.0;\n\
               \x20   let mut j = 0;\n\
               \x20   while j < n { s = s + a(i, j) * v[j]; j = j + 1; }\n\
               \x20   s\n\
               }\n\
               fn show(v: &Vec<f64>) ~ Console { println!(\"{}\", v[0]); }\n\
               fn main() ~ Console {\n\
               \x20   let mut v: Vec<f64> = Vec::new();\n\
               \x20   v.push(1.0); v.push(1.0);\n\
               \x20   println!(\"{:.6}\", row(2, &v, 0));\n\
               \x20   show(&v);\n\
               }\n";
    let Some(o) = run("call", src) else { return };
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout, "1.500000\n1\n");
    assert!(
        c_fn(&o.c, "row").contains("bl_vat(bl_vv_"),
        "row is hoisted"
    );
    assert!(
        !c_fn(&o.c, "show").contains("bl_vv_"),
        "a function that prints is not hoisted"
    );
}
