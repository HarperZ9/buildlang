//! A user constant named like a built-in maths constant shadows it (#177).

use std::process::Command;

fn run(name: &str, src: &str) -> Option<String> {
    let have_cc = ["gcc", "clang", "cc"].iter().any(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });
    if !have_cc {
        eprintln!("no C compiler; skipped");
        return None;
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let dir = std::env::temp_dir().join(format!("bl-user-math-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", root.join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "run failed: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn a_user_pi_e_and_tau_shadow_the_built_ins() {
    let src = "const PI: f64 = 3.0;\nconst E: f64 = 2.0;\nconst TAU: f64 = 6.0;\n\
               fn area(r: f64) -> f64 { PI * r * r }\n\
               fn main() ~ Console {\n    println!(\"{} {} {} {}\", area(1.0), PI, E, TAU);\n}\n";
    if let Some(out) = run("user_pi.bld", src) {
        assert_eq!(out, "3 3 2 6");
    }
}

#[test]
fn the_built_in_pi_is_kept_without_a_user_constant() {
    let src = "fn main() ~ Console {\n    println!(\"{}\", PI);\n}\n";
    if let Some(out) = run("builtin_pi.bld", src) {
        assert_eq!(out, "3.141592653589793");
    }
}
