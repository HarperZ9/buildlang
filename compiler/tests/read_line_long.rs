//! `read_line` returns one whole line, however long.
//!
//! The runtime used a single `fgets` into a 4 KB buffer, so a longer line
//! came back in 4,095-byte pieces, each looking like a line of its own. qdb's
//! REPL read a 6 KB INSERT as two statements and loaded nothing.

use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn c_backend_ready() -> bool {
    let output = buildc().arg("doctor").output().expect("run buildc doctor");
    String::from_utf8_lossy(&output.stdout).contains("Ready for practical C-backend examples: yes")
}

fn temp_dir() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "buildlang_read_line_{}_{}",
        std::process::id(),
        now
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn read_line_returns_a_line_longer_than_4096_bytes_whole() {
    if !c_backend_ready() {
        eprintln!("skipping read_line run test: no C backend available");
        return;
    }
    let dir = temp_dir();
    let src = dir.join("main.bld");
    fs::write(
        &src,
        r#"fn main() ~ Console {
    let a = read_line();
    let b = read_line();
    let c = read_line();
    println!("{} {} {}", a.len(), b.len(), c.len());
}
"#,
    )
    .expect("write fixture");
    let mut child = buildc()
        .arg("run")
        .arg("--stdio-mode")
        .arg("portable-lf")
        .arg(&src)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn buildc run");
    let mut input = "x".repeat(10_000);
    input.push('\n');
    input.push_str("bc\n");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    let _ = fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // 10,000 bytes plus the line feed, then "bc\n", then end of input.
    assert_eq!(
        stdout.trim_end(),
        "10001 3 0",
        "stdout={stdout:?} stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
