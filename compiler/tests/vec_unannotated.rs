//! `let v = Vec::new();` and `Vec::with_capacity(n)` with no type annotation.
//!
//! `let mut v = vec_new();` already took its element type from the first
//! push. The same vector written `Vec::new()` did not: the local had no
//! vector type, and `buildc check` refused the first `v.push(x)` (BL0901,
//! "`push` cannot be compiled: the C backend has no function or method by
//! that name"). `Vec::with_capacity(n)` was refused with or without an
//! annotation ("`Vec_with_capacity` cannot be compiled").
//!
//! 132 of the 267 files under `universe/` bind an unannotated `Vec::new()`
//! or `Vec::with_capacity(..)`.
//!
//! Expected outputs, worked out before the runs: three pushes of 1.5 sum to
//! 4.5; pushing 7 and 8 gives length 2 and item 8 at index 1; structs with
//! ids 0, 2, 4 sum to 6; "ab" and "cde" hold 5 bytes; an annotated
//! `Vec::with_capacity(2)` starts empty and holds 40 after one push; the
//! capacity expression runs once ("asked" is printed once); 2.5 + 4.0 = 6.5.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

fn program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-vecnew-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("program folder");
    let main = dir.join("main.bld");
    std::fs::write(&main, text).expect("source file");
    main
}

fn buildc(command: &str, file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg(command)
        .arg(file)
        .output()
        .expect("buildc runs")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Check, build and run the program; its stdout must be `wanted`.
fn checks_and_prints(main: &Path, wanted: &str) {
    let check = buildc("check", main);
    assert!(check.status.success(), "the check failed\n{}", text(&check));

    let run = buildc("run", main);
    let log = text(&run);
    // A machine with no C compiler cannot answer. CI has one, and there a
    // missing compiler is a failure, not a pass.
    if log.contains("no C compiler") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI has no C compiler\n{log}"
        );
        return;
    }
    assert_eq!(run.status.code(), Some(0), "{log}");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), wanted, "{log}");
}

#[test]
fn vec_new_takes_its_element_type_from_the_first_push() {
    let main = program(
        "new",
        r#"struct Mesh {
    id: i32,
}

fn halves(n: u32) -> Vec<f64> {
    let mut out = Vec::new();
    for _i in 0..n {
        out.push(1.5);
    }
    out
}

fn meshes(n: i32) -> Vec<Mesh> {
    let mut all = Vec::new();
    for i in 0..n {
        all.push(Mesh { id: i * 2 });
    }
    all
}

fn main() ~ Console {
    let v = halves(3);
    let mut total = 0.0;
    for x in v.iter() {
        total = total + x;
    }
    println!("{}", total);

    let mut w = Vec::new();
    w.push(7);
    w.push(8);
    println!("{} {}", w.len(), w[1]);

    let ms = meshes(3);
    let mut ids = 0;
    for m in ms.iter() {
        ids = ids + m.id;
    }
    println!("{}", ids);

    let mut words = Vec::new();
    words.push("ab");
    words.push("cde");
    let mut bytes: usize = 0;
    for s in words.iter() {
        bytes = bytes + s.len();
    }
    println!("{}", bytes);
}
"#,
    );
    checks_and_prints(&main, "4.5\n2 8\n6\n5");
}

#[test]
fn with_capacity_builds_an_empty_vector_and_runs_its_argument_once() {
    let main = program(
        "capacity",
        r#"fn capacity() -> usize ~ Console {
    println!("asked");
    8
}

fn main() ~ Console {
    let mut sized = Vec::with_capacity(capacity());
    sized.push(2.5);
    sized.push(4.0);
    let mut total = 0.0;
    for x in sized.iter() {
        total = total + x;
    }
    println!("{} {}", sized.len(), total);

    let mut wide: Vec<i64> = Vec::with_capacity(2);
    println!("{}", wide.len());
    wide.push(40);
    println!("{}", wide[0]);
}
"#,
    );
    checks_and_prints(&main, "asked\n2 6.5\n0\n40");
}
