//! `s.sha256()` returns the SHA-256 of a string's bytes as 64 lowercase hex
//! digits, from the C runtime. BuildDB's receipts (bdb/COMPLETE.md P16) used
//! a SHA-256 written in BuildLang, at about 6 microseconds per 64-byte block,
//! which cost more than the queries it named.

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
    let dir = std::env::temp_dir().join(format!("buildlang_string_sha256_{}", std::process::id()));
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

/// The FIPS 180-4 test vectors, a two-block message, a 1,000-byte string and
/// the empty string, against the published digests.
#[test]
fn sha256_matches_the_published_vectors() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let out = check_and_run(
        "sha_vectors.bld",
        r#"fn main() ~ Console {
    println!("{}", "".sha256());
    println!("{}", "abc".sha256());
    println!("{}", "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".sha256());
    let x = "x".repeat(1000);
    println!("{}", x.sha256());
    println!("{}", x.sha256().len());
}
"#,
    );
    let want = [
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        "44f8354494a5ba03ba1792a8d3e9c534c47a9181980fde7a3f44b06ef2ae7c7f",
        "64",
    ];
    let got: Vec<&str> = out.lines().collect();
    assert_eq!(got, want, "output: {}", out);
}

/// Lengths 55, 56, 64 and 119 sit on the padding boundaries (the length
/// field fits in the last block, or needs one more). Digests from Python's
/// hashlib.
#[test]
fn sha256_handles_the_padding_boundaries() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler on PATH");
        return;
    }
    let out = check_and_run(
        "sha_bounds.bld",
        r#"fn main() ~ Console {
    println!("{}", "a".repeat(55).sha256());
    println!("{}", "a".repeat(56).sha256());
    println!("{}", "a".repeat(64).sha256());
    println!("{}", "a".repeat(119).sha256());
}
"#,
    );
    let want = [
        "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
        "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a",
        "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb",
        "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb",
    ];
    let got: Vec<&str> = out.lines().collect();
    assert_eq!(got, want, "output: {}", out);
}
