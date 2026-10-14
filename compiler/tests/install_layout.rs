//! M2: an installed `buildc` with no repository beside it finds the standard
//! library. `cargo install buildlang` ships only the binary; the stdlib is
//! compiled in and written to a versioned temp directory on first use.

use std::{path::PathBuf, process::Command};

#[test]
fn installed_binary_runs_a_stdlib_program_without_a_checkout() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let iso = std::env::temp_dir().join(format!("buildlang_install_layout_{}", std::process::id()));
    let bin = iso.join("bin");
    let prog = iso.join("prog");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&prog).unwrap();
    let exe_name = if cfg!(windows) {
        "buildc.exe"
    } else {
        "buildc"
    };
    let exe = bin.join(exe_name);
    std::fs::copy(env!("CARGO_BIN_EXE_buildc"), &exe).unwrap();
    std::fs::copy(
        root.join("stdlib/131_stdlib_integration.bld"),
        prog.join("main.bld"),
    )
    .unwrap();

    let doctor = Command::new(&exe)
        .arg("doctor")
        .env_remove("BUILDLANG_STDLIB")
        .current_dir(&prog)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        text.contains("buildlang-stdlib-"),
        "doctor must report the embedded stdlib:\n{text}"
    );

    let run = Command::new(&exe)
        .args(["run", "main.bld"])
        .env_remove("BUILDLANG_STDLIB")
        .current_dir(&prog)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let expected =
        std::fs::read_to_string(root.join("stdlib/131_stdlib_integration.expected")).unwrap();
    let norm = |s: &str| s.replace("\r\n", "\n").trim_end().to_string();
    assert_eq!(norm(&String::from_utf8_lossy(&run.stdout)), norm(&expected));
    let _ = std::fs::remove_dir_all(&iso);
}
