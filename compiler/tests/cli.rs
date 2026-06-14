use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn quantac() -> Command {
    Command::new(env!("CARGO_BIN_EXE_quantac"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn quickstart_example(name: &str) -> PathBuf {
    repo_root().join("examples").join("quickstart").join(name)
}

fn c_backend_ready() -> bool {
    let output = quantac()
        .arg("doctor")
        .output()
        .expect("run quantac doctor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.contains("Ready for practical C-backend examples: yes")
}

fn receipt_from_stdout(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout should be JSON receipt: {err}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn copy_dir_recursive(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap_or_else(|err| {
        panic!(
            "create destination directory {}: {}",
            destination.display(),
            err
        )
    });

    for entry in fs::read_dir(source)
        .unwrap_or_else(|err| panic!("read source directory {}: {}", source.display(), err))
    {
        let entry = entry.expect("read directory entry");
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .unwrap_or_else(|err| panic!("read file type for {}: {}", entry_path.display(), err))
            .is_dir()
        {
            copy_dir_recursive(&entry_path, &destination_path);
        } else {
            fs::copy(&entry_path, &destination_path).unwrap_or_else(|err| {
                panic!(
                    "copy {} to {}: {}",
                    entry_path.display(),
                    destination_path.display(),
                    err
                )
            });
        }
    }
}

fn temp_semantic_corpus(label: &str) -> PathBuf {
    let destination = std::env::temp_dir().join(format!(
        "quantalang_semantic_corpus_{}_{}",
        label,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&destination);
    copy_dir_recursive(&repo_root().join("semantic-corpus"), &destination);
    destination
}

#[test]
fn help_lists_doctor_command() {
    let output = quantac()
        .arg("--help")
        .output()
        .expect("run quantac --help");

    assert!(
        output.status.success(),
        "quantac --help should exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("doctor"),
        "help should list doctor command:\n{}",
        stdout
    );
}

#[test]
fn doctor_reports_adoption_readiness_summary() {
    let output = quantac()
        .arg("doctor")
        .output()
        .expect("run quantac doctor");

    assert!(
        output.status.success(),
        "quantac doctor should exit successfully; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "quantac doctor should report diagnostics on stdout only:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "QuantaLang Doctor",
        "quantac:",
        "C backend:",
        "stdlib:",
        "registry:",
        "Backend maturity:",
        "c        primary",
        "rust     experimental",
    ] {
        assert!(
            stdout.contains(expected),
            "doctor output should contain {expected:?}:\n{}",
            stdout
        );
    }
}

#[test]
fn help_lists_corpus_command() {
    let output = quantac()
        .arg("--help")
        .output()
        .expect("run quantac --help");

    assert!(
        output.status.success(),
        "quantac --help should exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("corpus"),
        "help should list corpus command:\n{}",
        stdout
    );
}

#[test]
fn check_reports_capability_effect_for_ambient_file_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_capability_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { read_file("ops.txt"); }"#)
        .expect("write capability fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name triggering ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_stdout_records_passing_capabilities() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_pass_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write passing receipt fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "passing receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON receipt");
    assert_eq!(receipt["schema"], "quantalang-check-receipt/v1");
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["compiler"], "quantac");
    assert_eq!(receipt["compiler_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(receipt["language_version"], "1.0.0");
    assert_eq!(receipt["source_digest"]["algorithm"], "sha256");
    let digest = receipt["source_digest"]["hex"]
        .as_str()
        .expect("source digest hex string");
    assert_eq!(digest.len(), 64);
    assert!(
        digest
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
        "digest should be lowercase hex: {digest}"
    );
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Console"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Console"],
        serde_json::json!(["println!"])
    );
    assert!(receipt["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn check_receipt_file_records_failing_capability_diagnostic() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_fail_{}.quanta",
        std::process::id()
    ));
    let receipt_path = fixture.with_extension("receipt.json");
    fs::write(&fixture, r#"fn main() { read_file("ops.txt"); }"#)
        .expect("write failing receipt fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg(&receipt_path)
        .output()
        .expect("run quantac check --receipt file");

    let receipt_text = fs::read_to_string(&receipt_path).expect("read receipt file");
    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&receipt_path);

    assert!(
        !output.status.success(),
        "failing capability check should return nonzero"
    );
    let receipt: serde_json::Value =
        serde_json::from_str(&receipt_text).expect("receipt file should be JSON");
    assert_eq!(receipt["schema"], "quantalang-check-receipt/v1");
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["compiler_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(receipt["language_version"], "1.0.0");
    assert_eq!(receipt["source_digest"]["algorithm"], "sha256");
    assert_eq!(
        receipt["source_digest"]["hex"]
            .as_str()
            .expect("failing receipt digest")
            .len(),
        64
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            diag["stage"] == "type"
                && diag["kind"] == "UnhandledEffect"
                && diag["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("FileSystem")
        }),
        "expected FileSystem UnhandledEffect diagnostic in {diagnostics:#?}"
    );
}

#[test]
fn check_receipt_source_digest_ignores_path_for_identical_content() {
    let id = std::process::id();
    let left =
        std::env::temp_dir().join(format!("quantalang_check_receipt_digest_left_{id}.quanta"));
    let right =
        std::env::temp_dir().join(format!("quantalang_check_receipt_digest_right_{id}.quanta"));
    let source = r#"fn main() ~ Console { println!("same"); }"#;
    fs::write(&left, source).expect("write left digest fixture");
    fs::write(&right, source).expect("write right digest fixture");

    let left_output = quantac()
        .arg("check")
        .arg(&left)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run left digest receipt");
    let right_output = quantac()
        .arg("check")
        .arg(&right)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run right digest receipt");

    let _ = fs::remove_file(&left);
    let _ = fs::remove_file(&right);

    assert!(left_output.status.success(), "left check should pass");
    assert!(right_output.status.success(), "right check should pass");
    let left_receipt = receipt_from_stdout(&left_output);
    let right_receipt = receipt_from_stdout(&right_output);
    assert_ne!(left_receipt["source"], right_receipt["source"]);
    let left_digest = left_receipt["source_digest"]["hex"]
        .as_str()
        .expect("left source digest hex string");
    let right_digest = right_receipt["source_digest"]["hex"]
        .as_str()
        .expect("right source digest hex string");
    assert_eq!(left_digest.len(), 64);
    assert_eq!(right_digest.len(), 64);
    assert_eq!(
        left_receipt["source_digest"]["hex"],
        right_receipt["source_digest"]["hex"]
    );
}

#[test]
fn check_receipt_source_digest_changes_when_source_changes() {
    let id = std::process::id();
    let first =
        std::env::temp_dir().join(format!("quantalang_check_receipt_digest_first_{id}.quanta"));
    let second = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_digest_second_{id}.quanta"
    ));
    fs::write(&first, r#"fn main() ~ Console { println!("first"); }"#)
        .expect("write first digest fixture");
    fs::write(&second, r#"fn main() ~ Console { println!("second"); }"#)
        .expect("write second digest fixture");

    let first_output = quantac()
        .arg("check")
        .arg(&first)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run first digest receipt");
    let second_output = quantac()
        .arg("check")
        .arg(&second)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run second digest receipt");

    let _ = fs::remove_file(&first);
    let _ = fs::remove_file(&second);

    assert!(first_output.status.success(), "first check should pass");
    assert!(second_output.status.success(), "second check should pass");
    let first_receipt = receipt_from_stdout(&first_output);
    let second_receipt = receipt_from_stdout(&second_output);
    assert_ne!(
        first_receipt["source_digest"]["hex"],
        second_receipt["source_digest"]["hex"]
    );
}

#[test]
fn corpus_verify_accepts_explicit_root() {
    if !c_backend_ready() {
        eprintln!("skipping semantic corpus root verification because no C backend is available");
        return;
    }

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run quantac corpus verify with explicit root");

    assert!(
        output.status.success(),
        "corpus verify --root should exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("c execution: 8 passed"),
        "corpus verify --root should run the manifest programs:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_write_repairs_receipt_program_drift_in_copy() {
    if !c_backend_ready() {
        eprintln!("skipping semantic corpus write verification because no C backend is available");
        return;
    }

    let corpus_root = temp_semantic_corpus("write");
    let c_receipt_path = corpus_root
        .join("receipts")
        .join("c-execution-2026-06-13.json");
    let original_receipt = fs::read_to_string(&c_receipt_path).expect("read copied C receipt");
    let drifted_receipt = original_receipt.replacen(
        r#""expected_stdout": "4\n""#,
        r#""expected_stdout": "999\n""#,
        1,
    );
    assert_ne!(
        original_receipt, drifted_receipt,
        "copied C receipt should contain the scalar stdout fixture"
    );
    fs::write(&c_receipt_path, drifted_receipt).expect("write drifted copied C receipt");

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .arg("--write")
        .output()
        .expect("run quantac corpus verify --write");

    assert!(
        output.status.success(),
        "corpus verify --write should repair copied receipt drift\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("c receipt: written"),
        "corpus verify --write should report the C receipt write:\n{}",
        stdout
    );

    let repaired_receipt = fs::read_to_string(&c_receipt_path).expect("read repaired C receipt");
    assert!(
        repaired_receipt.contains(r#""expected_stdout": "4\n""#),
        "repaired C receipt should restore manifest stdout:\n{}",
        repaired_receipt
    );
    assert!(
        !repaired_receipt.contains(r#""expected_stdout": "999\n""#),
        "repaired C receipt should remove drifted stdout:\n{}",
        repaired_receipt
    );

    let _ = fs::remove_dir_all(&corpus_root);
}

#[test]
fn corpus_verify_checks_manifest_receipts_and_c_execution() {
    if !c_backend_ready() {
        eprintln!("skipping semantic corpus verification because no C backend is available");
        return;
    }

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .output()
        .expect("run quantac corpus verify");

    assert!(
        output.status.success(),
        "corpus verify should exit successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    for expected in [
        "Semantic Corpus Verify",
        "manifest: 8 program(s)",
        "c receipt: ok",
        "rust receipt: ok",
        "c execution: 8 passed",
    ] {
        assert!(
            stdout.contains(expected),
            "corpus verify output should contain {expected:?}:\n{}",
            stdout
        );
    }
}

#[test]
fn quickstart_examples_are_typechecked() {
    for name in [
        "hello.quanta",
        "ledger.quanta",
        "effects_greeting.quanta",
        "vignette_shader.quanta",
    ] {
        let path = quickstart_example(name);
        let output = quantac()
            .arg("check")
            .arg(&path)
            .output()
            .unwrap_or_else(|err| panic!("run quantac check for {name}: {err}"));

        assert!(
            output.status.success(),
            "quickstart example {name} should typecheck\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn quickstart_cpu_examples_run_when_c_backend_is_ready() {
    if !c_backend_ready() {
        eprintln!("skipping quickstart run smoke test because no C backend is available");
        return;
    }

    for (name, expected_stdout) in [
        ("hello.quanta", "Hello from QuantaLang!\n"),
        ("ledger.quanta", "balance: 115\n"),
        ("effects_greeting.quanta", "Hello, teammate!\n"),
    ] {
        let output = quantac()
            .arg("run")
            .arg(quickstart_example(name))
            .output()
            .unwrap_or_else(|err| panic!("run quantac run for {name}: {err}"));

        assert!(
            output.status.success(),
            "quickstart example {name} should run\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
        assert_eq!(stdout, expected_stdout);
    }
}

#[test]
fn quickstart_shader_example_compiles_to_hlsl() {
    let out_dir = std::env::temp_dir().join(format!(
        "quantalang_quickstart_shader_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create quickstart shader temp dir");
    let output_path = out_dir.join("vignette_shader.hlsl");

    let output = quantac()
        .arg(quickstart_example("vignette_shader.quanta"))
        .arg("--target")
        .arg("hlsl")
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("compile quickstart shader to HLSL");

    assert!(
        output.status.success(),
        "quickstart shader should compile to HLSL\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let hlsl = fs::read_to_string(&output_path).expect("read generated HLSL");
    assert!(
        hlsl.contains("PS_Vignette"),
        "generated HLSL should contain the fragment entry point:\n{}",
        hlsl
    );

    let _ = fs::remove_dir_all(&out_dir);
}
