//! `xs.get_unchecked(i)` and `xs.set_unchecked(i, v)`: slice access with no bounds check,
//! for a loop whose range the program checked once before it. The C is a plain subscript,
//! so gcc can vectorise the loop. Accept: the values are right and the generated C has no
//! `bl_idx_chk` for them. Refusals: outside `unsafe` (BL0382), and a write through a
//! read-only `&[T]`.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn scratch(name: &str, src: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "buildlang-unchecked-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.bld"), src).unwrap();
    dir
}

fn buildc(args: &[&str], path: &std::path::Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(args)
        .arg(path)
        .output()
        .expect("buildc runs");
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

const GOOD: &str = "fn scale(xs: &mut [f64], k: f64) {\n    let n = xs.len();\n    let mut i: usize = 0;\n    while i < n {\n        unsafe {\n            let v = xs.get_unchecked(i);\n            xs.set_unchecked(i, v * k);\n        }\n        i = i + 1;\n    }\n}\n\nfn main() ~ Console {\n    let mut a: [f64; 4] = [1.0, 2.0, 3.0, 4.0];\n    scale(&mut a, 2.5);\n    println!(\"{} {}\", a[0], a[3]);\n}\n";

#[test]
fn unchecked_access_reads_and_writes_without_a_bounds_check() {
    let dir = scratch("good", GOOD);
    let (ok, text) = buildc(&["build", "--emit", "c"], &dir);
    assert!(ok, "{text}");
    let c = std::fs::read_to_string(dir.join("target/debug/main.c")).unwrap();
    let body = c.split("static void scale(").nth(2).unwrap_or("");
    let body = &body[..body.find("\n}\n").unwrap_or(body.len())];
    assert!(!body.contains("bl_idx_chk"), "{body}");
    if has_cc() {
        let (ok, text) = buildc(&["run"], &dir.join("main.bld"));
        assert!(ok && text.lines().any(|l| l.trim() == "2.5 10"), "{text}");
    }
}

#[test]
fn unchecked_access_needs_unsafe_and_a_mutable_slice() {
    let outside = "fn get(xs: &[f64]) -> f64 {\n    xs.get_unchecked(0)\n}\n";
    let (ok, text) = buildc(&["check"], &scratch("outside", outside).join("main.bld"));
    assert!(!ok && text.contains("BL0382"), "{text}");
    let readonly =
        "fn put(xs: &[f64]) {\n    unsafe {\n        xs.set_unchecked(0, 1.0);\n    }\n}\n";
    let (ok, text) = buildc(&["check"], &scratch("readonly", readonly).join("main.bld"));
    assert!(!ok && text.contains("read-only"), "{text}");
}
