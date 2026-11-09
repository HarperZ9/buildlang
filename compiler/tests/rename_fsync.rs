//! Issue #166: `rename_file` replaces a file in one step and `fsync_file`
//! flushes one to the device, both under the FileSystem effect. Before, a
//! program had no way to replace a file atomically; BuildDB wrote its whole
//! state twice (`<db>.ckpt`, then `<db>`) to survive a torn write.

use std::process::Command;

fn buildc() -> Command {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", root.join("stdlib"));
    cmd
}

fn have_c_compiler() -> bool {
    let probe = |cc: &str, arg: &str| {
        Command::new(cc)
            .arg(arg)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    ["gcc", "clang", "cc"]
        .iter()
        .any(|cc| probe(cc, "--version"))
        || probe("cl", "/?")
}

fn write(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("buildlang_rename_fsync_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, src).unwrap();
    path
}

/// Check, then run, and return trimmed stdout. Panics with the output on failure.
fn check_and_run(name: &str, src: &str) -> String {
    let file = write(name, src);
    let check = buildc().arg("check").arg(&file).output().unwrap();
    assert!(
        check.status.success(),
        "check failed: {}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let run = buildc().arg("run").arg(&file).output().unwrap();
    assert!(
        run.status.success(),
        "run failed ({:?}): {}{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string()
}

/// Replace an existing file: the target ends with the new text, the source
/// is gone, and both calls report success.
#[test]
fn rename_file_replaces_an_existing_file() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_rename_fsync_files_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("state.tmp");
    let dst = dir.join("state.db");
    std::fs::write(&dst, "old").unwrap();
    let t = tmp.to_string_lossy().replace('\\', "/");
    let d = dst.to_string_lossy().replace('\\', "/");
    let src = format!(
        "fn main() ~ Console + FileSystem {{ write_file(\"{t}\", \"new\"); let a = fsync_file(\"{t}\"); let b = rename_file(\"{t}\", \"{d}\"); println!(\"{{}} {{}}\", a, b); }}\n"
    );
    let out = check_and_run("replace.bld", &src);
    assert_eq!(out.lines().last().unwrap_or(""), "true true", "{out}");
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new");
    assert!(!tmp.exists(), "the source must be gone after the rename");
}

/// Failures report false: a missing source, and a flush of a missing file.
#[test]
fn rename_and_fsync_report_failure() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "buildlang_rename_fsync_none_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("missing.tmp").to_string_lossy().replace('\\', "/");
    let dst = dir.join("never.db").to_string_lossy().replace('\\', "/");
    let src = format!(
        "fn main() ~ Console + FileSystem {{ let a = rename_file(\"{missing}\", \"{dst}\"); let b = fsync_file(\"{missing}\"); println!(\"{{}} {{}}\", a, b); }}\n"
    );
    let out = check_and_run("fail.bld", &src);
    assert_eq!(out.lines().last().unwrap_or(""), "false false", "{out}");
}

/// The builtins carry the FileSystem effect: a function that does not
/// declare it is refused.
#[test]
fn rename_file_needs_the_filesystem_effect() {
    let file = write(
        "noeffect.bld",
        "fn main() ~ Console { let ok = rename_file(\"a\", \"b\"); println!(\"{}\", ok); }\n",
    );
    let check = buildc().arg("check").arg(&file).output().unwrap();
    assert!(
        !check.status.success(),
        "rename_file without FileSystem must be refused"
    );
}
