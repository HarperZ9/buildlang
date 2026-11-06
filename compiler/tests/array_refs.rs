//! References and raw pointers to fixed arrays. The C backend wrote `double[4]*`, which C
//! rejects, for `&[f64; 4]` parameters, for `&mut buf`, and for the cast
//! `&mut buf as *mut [f64; 4] as *mut f64`, after the check had passed. A pointer to an
//! array is now a pointer to its first element, and the address of an array local is the
//! array itself. Also: deferred initialisation and `let mut` still assign, while a plain
//! initialised `let` does not.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn run(name: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!(
        "buildlang-arrayref-{}-{}",
        name,
        std::process::id()
    ));
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
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
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

#[test]
fn references_and_pointers_to_fixed_arrays_compile_and_run() {
    if !has_cc() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let src = "fn second(a: &[f64; 4]) -> f64 {\n    a[1]\n}\n\nfn set(a: &mut [f64; 4]) {\n    a[2] = 5.0;\n}\n\nfn fill(p: *mut f64, n: usize) {\n    let xs = unsafe { slice::from_raw_parts_mut(p, n) };\n    xs[3] = 9.0;\n}\n\nfn main() ~ Console {\n    let mut b: [f64; 4] = [1.0, 2.0, 3.0, 4.0];\n    set(&mut b);\n    let p = unsafe { &mut b as *mut [f64; 4] as *mut f64 };\n    fill(p, 4);\n    println!(\"{} {} {}\", second(&b), b[2], b[3]);\n}\n";
    let (ok, text) = run("ok", src);
    assert!(ok, "{text}");
    assert!(text.lines().any(|l| l.trim() == "2 5 9"), "{text}");
}

#[test]
fn deferred_and_mutable_lets_assign_but_a_fixed_let_does_not() {
    if !has_cc() {
        return;
    }
    let ok_src = "fn main() ~ Console {\n    let mut y = 1;\n    y = 3;\n    let z: i32;\n    z = 4;\n    println!(\"{} {}\", y, z);\n}\n";
    let (ok, text) = run("mut", ok_src);
    assert!(ok && text.lines().any(|l| l.trim() == "3 4"), "{text}");
    let bad = "fn main() ~ Console {\n    let x = 1;\n    x = 2;\n    println!(\"{}\", x);\n}\n";
    let (ok, text) = run("fixed", bad);
    assert!(!ok && text.contains("cannot assign to immutable variable `x`"), "{text}");
}
