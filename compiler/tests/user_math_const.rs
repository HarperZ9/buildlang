//! A user constant named like a built-in maths constant shadows it (#177).

use std::process::Command;

#[test]
fn a_user_pi_e_and_tau_shadow_the_built_ins() {
    let dir = std::env::temp_dir().join(format!("bl-user-pi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("pi.bld");
    std::fs::write(
        &src,
        "const PI: f64 = 3.0;\nconst E: f64 = 2.0;\nconst TAU: f64 = 6.0;\n\
         fn area(r: f64) -> f64 { PI * r * r }\n\
         fn main() ~ Console {\n    println!(\"{} {} {} {}\", area(1.0), PI, E, TAU);\n}\n",
    )
    .unwrap();
    let exe = dir.join(if cfg!(windows) { "pi.exe" } else { "pi" });
    let b = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("buildc runs");
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let r = Command::new(&exe).output().expect("program runs");
    let out = String::from_utf8_lossy(&r.stdout);
    assert_eq!(out.trim(), "3 3 2 6", "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_built_in_pi_is_kept_without_a_user_constant() {
    let dir = std::env::temp_dir().join(format!("bl-builtin-pi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("pi.bld");
    std::fs::write(
        &src,
        "fn main() ~ Console {\n    println!(\"{}\", PI);\n}\n",
    )
    .unwrap();
    let exe = dir.join(if cfg!(windows) { "pi.exe" } else { "pi" });
    let b = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("buildc runs");
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let r = Command::new(&exe).output().expect("program runs");
    assert_eq!(
        String::from_utf8_lossy(&r.stdout).trim(),
        "3.141592653589793"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
