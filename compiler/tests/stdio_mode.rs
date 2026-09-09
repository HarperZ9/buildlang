use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

fn temp_dir(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "buildlang_stdio_mode_{label}_{}_{}",
        std::process::id(),
        now
    ));
    fs::create_dir_all(&dir)
        .unwrap_or_else(|err| panic!("create temp dir {}: {err}", dir.display()));
    dir
}

fn write_fixture(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, source)
        .unwrap_or_else(|err| panic!("write fixture {}: {err}", path.display()));
    path
}

fn c_backend_ready() -> bool {
    let output = buildc().arg("doctor").output().expect("run buildc doctor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.contains("Ready for practical C-backend examples: yes")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write digest hex");
    }
    hex
}

#[test]
fn compile_default_and_explicit_native_generate_identical_c_source() {
    let dir = temp_dir("native_source");
    let fixture = write_fixture(
        &dir,
        "main.bld",
        r#"fn main() ~ Console { println!("line"); }"#,
    );
    let default_c = dir.join("default.c");
    let native_c = dir.join("native.c");

    let default = buildc()
        .arg(&fixture)
        .arg("-o")
        .arg(&default_c)
        .output()
        .expect("emit default C source");
    assert!(
        default.status.success(),
        "default C emission failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&default.stdout),
        String::from_utf8_lossy(&default.stderr)
    );

    let native = buildc()
        .arg("--stdio-mode")
        .arg("native")
        .arg(&fixture)
        .arg("-o")
        .arg(&native_c)
        .output()
        .expect("emit explicit native C source");
    assert!(
        native.status.success(),
        "explicit native C emission failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&native.stderr)
    );

    let default_bytes = fs::read(&default_c).expect("read default emitted C");
    let native_bytes = fs::read(&native_c).expect("read native emitted C");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        default_bytes, native_bytes,
        "the default mode must remain byte-identical to explicit native mode"
    );
    assert!(
        !String::from_utf8_lossy(&default_bytes).contains("__build_init_portable_lf_stdio"),
        "native source must not include the portable stdio hook"
    );
}

#[test]
fn compile_portable_lf_generates_windows_binary_stdio_hook_and_posix_guard() {
    let dir = temp_dir("portable_source");
    let fixture = write_fixture(
        &dir,
        "main.bld",
        r#"fn main() ~ Console { println!("line"); }"#,
    );
    let portable_c = dir.join("portable.c");

    let output = buildc()
        .arg("--stdio-mode")
        .arg("portable-lf")
        .arg(&fixture)
        .arg("-o")
        .arg(&portable_c)
        .output()
        .expect("emit portable C source");
    assert!(
        output.status.success(),
        "portable C emission failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let code = fs::read_to_string(&portable_c).expect("read portable emitted C");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        code.contains("#ifdef _WIN32\n#include <fcntl.h>\n#include <io.h>\n#endif"),
        "portable mode should include Windows CRT binary-mode headers behind a Windows guard:\n{code}"
    );
    assert!(
        code.contains("static void __build_init_portable_lf_stdio(void)"),
        "portable mode should emit a dedicated initialization function:\n{code}"
    );
    assert!(
        code.contains("#ifdef _WIN32\n    int stdout_fd = _fileno(stdout);"),
        "Windows CRT mode changes should be guarded so POSIX only retains the native unbuffered setup:\n{code}"
    );
    assert!(
        code.contains("_setmode(stdout_fd, _O_BINARY)")
            && code.contains("_setmode(stderr_fd, _O_BINARY)"),
        "portable mode must switch stdout and stderr to binary mode on Windows:\n{code}"
    );
    assert!(
        code.contains("__build_init_portable_lf_stdio();"),
        "generated main should call the portable initialization hook:\n{code}"
    );
}

#[test]
fn build_emit_c_threads_portable_lf_into_generated_source() {
    let dir = temp_dir("build_emit_c");
    let fixture = write_fixture(
        &dir,
        "main.bld",
        r#"fn main() ~ Console { println!("line"); }"#,
    );
    assert!(fixture.exists());

    let output = buildc()
        .arg("build")
        .arg(&dir)
        .arg("--emit")
        .arg("c")
        .arg("--stdio-mode")
        .arg("portable-lf")
        .output()
        .expect("run build --emit c");
    assert!(
        output.status.success(),
        "build --emit c with portable stdio failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(dir.join("target").join("debug").join("main.c"))
        .expect("read build-generated C");
    let _ = fs::remove_dir_all(&dir);

    assert!(generated.contains("__build_init_portable_lf_stdio();"));
    assert!(generated.contains("_setmode(stdout_fd, _O_BINARY)"));
    assert!(generated.contains("_setmode(stderr_fd, _O_BINARY)"));
}

#[test]
fn portable_lf_rejects_non_c_codegen_targets() {
    let dir = temp_dir("non_c_refusal");
    let fixture = write_fixture(
        &dir,
        "main.bld",
        r#"fn main() ~ Console { println!("line"); }"#,
    );
    let rust_output = dir.join("main.rs");

    let compile = buildc()
        .arg("--stdio-mode")
        .arg("portable-lf")
        .arg("--target")
        .arg("rust")
        .arg(&fixture)
        .arg("-o")
        .arg(&rust_output)
        .output()
        .expect("compile portable-lf to rust target");
    assert!(
        !compile.status.success(),
        "portable-lf must be refused for non-C compile targets"
    );
    let compile_stderr = String::from_utf8_lossy(&compile.stderr);
    assert!(
        compile_stderr.contains("portable-lf") && compile_stderr.contains("C backend"),
        "compile refusal should name the unsupported stdio mode and C-backend boundary:\n{compile_stderr}"
    );

    let build = buildc()
        .arg("build")
        .arg(&dir)
        .arg("--target")
        .arg("rust")
        .arg("--stdio-mode")
        .arg("portable-lf")
        .output()
        .expect("build portable-lf to rust target");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        !build.status.success(),
        "portable-lf must be refused for non-C build targets"
    );
    let build_stderr = String::from_utf8_lossy(&build.stderr);
    assert!(
        build_stderr.contains("portable-lf") && build_stderr.contains("C backend"),
        "build refusal should name the unsupported stdio mode and C-backend boundary:\n{build_stderr}"
    );
}

#[test]
fn run_portable_lf_writes_raw_lf_to_stdout_and_stderr() {
    if !c_backend_ready() {
        eprintln!("skipping portable stdio run test: no C backend available");
        return;
    }

    let dir = temp_dir("run_raw_lf");
    let fixture = write_fixture(
        &dir,
        "main.bld",
        r#"fn main() ~ Console {
    println!("out");
    let a = [11];
    let i = 1;
    println!("{}", a[i]);
}
"#,
    );

    let native = buildc()
        .arg("run")
        .arg(&fixture)
        .output()
        .expect("run native stdio program");
    let portable = buildc()
        .arg("run")
        .arg(&fixture)
        .arg("--stdio-mode")
        .arg("portable-lf")
        .output()
        .expect("run portable stdio program");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        !native.status.success() && !portable.status.success(),
        "fixture should abort through the runtime bounds check\nnative status={:?}\nportable status={:?}",
        native.status.code(),
        portable.status.code()
    );

    #[cfg(windows)]
    {
        assert_eq!(
            native.stdout, b"out\r\n",
            "default native Windows stdio should keep CRT text-mode CRLF"
        );
        assert!(
            native.stderr.windows(2).any(|pair| pair == b"\r\n"),
            "default native Windows stderr should keep CRT text-mode CRLF:\n{}",
            String::from_utf8_lossy(&native.stderr)
        );
    }

    assert_eq!(
        portable.stdout, b"out\n",
        "portable-lf stdout should be byte-stable LF"
    );
    assert!(
        String::from_utf8_lossy(&portable.stderr).contains("index out of bounds"),
        "portable-lf stderr should include the runtime diagnostic:\n{}",
        String::from_utf8_lossy(&portable.stderr)
    );
    assert!(
        !portable.stderr.windows(2).any(|pair| pair == b"\r\n"),
        "portable-lf stderr should not contain CRLF translation:\n{}",
        String::from_utf8_lossy(&portable.stderr)
    );
    assert!(
        portable.stderr.ends_with(b"\n"),
        "portable-lf stderr diagnostic should end with LF"
    );
}

#[test]
fn scientific_receipt_default_omits_stdio_mode_and_still_verifies() {
    if !c_backend_ready() {
        eprintln!("skipping native scientific receipt stdio test: no C backend available");
        return;
    }

    let dir = temp_dir("receipt_native");
    let fixture = write_fixture(
        &dir,
        "main.bld",
        r#"fn main() ~ Console {
    println!("{}", 3);
    println!("{}", 2);
}
"#,
    );
    let receipt_path = dir.join("receipt.json");

    let emit = buildc()
        .arg("run")
        .arg(&fixture)
        .arg("--emit-receipt")
        .arg(&receipt_path)
        .output()
        .expect("emit native scientific receipt");
    assert!(
        emit.status.success(),
        "native receipt emission failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&emit.stdout),
        String::from_utf8_lossy(&emit.stderr)
    );

    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read native receipt"))
            .expect("native receipt should be JSON");
    assert!(
        receipt["build_state"]
            .as_object()
            .expect("receipt should have build_state")
            .get("stdio_mode")
            .is_none(),
        "native receipts should retain the old missing-field shape: {receipt:#?}"
    );

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt_path)
        .arg("--json")
        .output()
        .expect("verify native scientific receipt");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify.status.success(),
        "missing-stdio-mode receipt should verify as native\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn scientific_receipt_portable_lf_seals_mode_and_verify_replays_it() {
    if !c_backend_ready() {
        eprintln!("skipping portable scientific receipt stdio test: no C backend available");
        return;
    }

    let dir = temp_dir("receipt_portable");
    let fixture = write_fixture(
        &dir,
        "main.bld",
        r#"fn main() ~ Console {
    println!("{}", 3);
    println!("{}", 2);
}
"#,
    );
    let receipt_path = dir.join("receipt.json");

    let emit = buildc()
        .arg("run")
        .arg(&fixture)
        .arg("--stdio-mode")
        .arg("portable-lf")
        .arg("--emit-receipt")
        .arg(&receipt_path)
        .output()
        .expect("emit portable scientific receipt");
    assert!(
        emit.status.success(),
        "portable receipt emission failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&emit.stdout),
        String::from_utf8_lossy(&emit.stderr)
    );

    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read portable receipt"))
            .expect("portable receipt should be JSON");
    assert_eq!(receipt["build_state"]["stdio_mode"], "portable-lf");
    assert_eq!(
        receipt["measurement"]["raw_stdout_digest"]["hex"],
        sha256_hex(b"3\n2\n")
    );

    let verify = buildc()
        .args(["receipt", "verify"])
        .arg(&receipt_path)
        .arg("--json")
        .output()
        .expect("verify portable scientific receipt");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify.status.success(),
        "portable receipt should verify by replaying the sealed stdio mode\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&verify.stdout).expect("verify --json output should parse");
    assert_eq!(report["stdio_mode"], "portable-lf");
    assert_eq!(report["raw_stdout_reproduced"], true);
}
