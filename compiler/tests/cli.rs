use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};

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

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("write digest hex");
    }
    hex
}

fn write_temp_policy(label: &str, json: &str) -> PathBuf {
    let policy = std::env::temp_dir().join(format!(
        "quantalang_check_policy_{}_{}.json",
        label,
        std::process::id()
    ));
    fs::write(&policy, json)
        .unwrap_or_else(|err| panic!("write policy fixture {}: {}", policy.display(), err));
    policy
}

fn input_digest_hex(receipt: &serde_json::Value, role: &str, source_suffix: &str) -> String {
    let records = receipt["input_digests"]
        .as_array()
        .expect("input_digests should be an array");
    let record = records
        .iter()
        .find(|record| {
            record["role"] == role
                && record["source"]
                    .as_str()
                    .is_some_and(|source| source.ends_with(source_suffix))
        })
        .unwrap_or_else(|| {
            panic!("missing input digest role={role:?} suffix={source_suffix:?} in {records:#?}")
        });
    assert_eq!(record["digest"]["algorithm"], "sha256");
    let hex = record["digest"]["hex"]
        .as_str()
        .expect("input digest hex should be a string");
    assert_eq!(hex.len(), 64);
    hex.to_string()
}

fn input_graph_digest_hex(receipt: &serde_json::Value) -> String {
    assert_eq!(receipt["input_graph_digest"]["algorithm"], "sha256");
    let hex = receipt["input_graph_digest"]["hex"]
        .as_str()
        .expect("input graph digest hex should be a string");
    assert_eq!(hex.len(), 64);
    hex.to_string()
}

fn verification_check<'a>(report: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    report["checks"]
        .as_array()
        .expect("verification checks should be an array")
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("missing verification check {name:?} in {report:#?}"))
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

fn write_substrate_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("substrate-semantic-corpus-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read substrate receipt"))
            .expect("parse substrate receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified substrate receipt");
    fs::write(&receipt_path, format!("{rendered}\n")).expect("write modified substrate receipt");
}

fn write_mir_representation_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("mir-representation-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read MIR representation receipt"))
            .expect("parse MIR representation receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified MIR representation receipt");
    fs::write(&receipt_path, format!("{rendered}\n"))
        .expect("write modified MIR representation receipt");
}

fn write_memory_layout_receipt_copy(
    corpus_root: &Path,
    transform: impl FnOnce(serde_json::Value) -> serde_json::Value,
) {
    let receipt_path = corpus_root
        .join("receipts")
        .join("memory-layout-2026-06-18.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).expect("read memory layout receipt"))
            .expect("parse memory layout receipt");
    let receipt = transform(receipt);
    let rendered =
        serde_json::to_string_pretty(&receipt).expect("render modified memory layout receipt");
    fs::write(&receipt_path, format!("{rendered}\n"))
        .expect("write modified memory layout receipt");
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
        "Substrate evidence:",
        "receipt   ok",
        "quantalang-substrate-receipt/v0",
        "corpus    ok",
        "8 semantic program(s)",
        "c         anchor",
        "rust      subset",
        "spirv     unverified",
        "memory    partial",
        "6 verified surface(s), 3 known gap(s)",
        "repr      MIR",
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
fn check_reports_capability_effect_for_qualified_ambient_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_qualified_capability_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { io::read_file("ops.txt"); }"#)
        .expect("write qualified capability fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "qualified ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("io::read_file"),
        "diagnostic should name qualified ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_reports_capability_effect_for_gpu_runtime_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_gpu_capability_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() { quanta_vk_init(); }"#)
        .expect("write gpu capability fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "GPU runtime call should fail without Gpu effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Gpu"),
        "diagnostic should name Gpu effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("quanta_vk_init"),
        "diagnostic should name triggering GPU helper:\n{}",
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
fn check_reports_capability_effect_for_include_str_macro() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_include_str_capability_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() { let embedded = include_str!("ops.txt"); }"#,
    )
    .expect("write include_str macro capability fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "include_str! should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("include_str!"),
        "diagnostic should name triggering include_str macro:\n{}",
        stderr
    );
}

#[test]
fn check_reports_capability_effect_for_ambient_call_inside_macro_argument() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_macro_arg_capability_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ Console { println!(read_file("ops.txt")); }"#,
    )
    .expect("write macro argument capability fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "macro argument ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name triggering macro argument ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_reports_capability_effect_for_macro_argument_call_in_external_module() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_module_macro_arg_capability_gate_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create module macro argument fixture dir");
    let entry = dir.join("main.quanta");
    let module = dir.join("ops.quanta");

    fs::write(
        &entry,
        r#"mod ops;
fn main() {}
"#,
    )
    .expect("write module macro argument entry fixture");
    fs::write(
        &module,
        r#"fn leak() ~ Console {
    println!(read_file("ops.txt"));
}
"#,
    )
    .expect("write module macro argument module fixture");

    let output = quantac()
        .arg("check")
        .arg(&entry)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !output.status.success(),
        "external module macro argument ambient file call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name triggering module macro argument ambient call:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_env_macro_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_env_macro_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ Environment { let token_name = env!("TOKEN"); }"#,
    )
    .expect("write env macro receipt fixture");

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
        "env macro receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Environment"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Environment"],
        serde_json::json!(["env!"])
    );
}

#[test]
fn check_receipt_records_macro_argument_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_macro_arg_capability_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ Console + FileSystem { println!(read_file("ops.txt")); }"#,
    )
    .expect("write macro argument capability receipt fixture");

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
        "macro argument capability receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Console", "FileSystem"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Console"],
        serde_json::json!(["println!"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
}

#[test]
fn check_receipt_records_macro_argument_capability_source_in_external_module() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_module_macro_arg_capability_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create module macro argument receipt fixture dir");
    let entry = dir.join("main.quanta");
    let module = dir.join("ops.quanta");

    fs::write(
        &entry,
        r#"mod ops;
fn main() {}
"#,
    )
    .expect("write module macro argument receipt entry fixture");
    fs::write(
        &module,
        r#"fn leak() ~ Console + FileSystem {
    println!(read_file("ops.txt"));
}
"#,
    )
    .expect("write module macro argument receipt module fixture");

    let output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "external module macro argument capability receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["ops_leak"],
        serde_json::json!(["Console", "FileSystem"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["ops_leak"]["Console"],
        serde_json::json!(["println!"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["ops_leak"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
}

#[test]
fn check_receipt_records_gpu_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_gpu_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Gpu { quanta_vk_init(); }"#)
        .expect("write gpu receipt fixture");

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
        "GPU receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Gpu"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Gpu"],
        serde_json::json!(["quanta_vk_init"])
    );
}

#[test]
fn check_receipt_records_graphics_runtime_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_graphics_runtime_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" {
    fn quanta_gfx_init(width: i32, height: i32, title: &str) -> i32;
}

fn main() ~ Gpu {
    quanta_gfx_init(800, 600, "QuantaLang Triangle");
}
"#,
    )
    .expect("write graphics runtime receipt fixture");

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
        "graphics runtime receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Gpu"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Gpu"],
        serde_json::json!(["quanta_gfx_init"])
    );
}

#[test]
fn check_receipt_records_qualified_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_qualified_capability_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { io::read_file("ops.txt"); }"#,
    )
    .expect("write qualified capability receipt fixture");

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
        "qualified capability receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]["FileSystem"],
        serde_json::json!(["io::read_file"])
    );
}

#[test]
fn check_receipt_records_foreign_call_as_direct_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_foreign_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { fn touch(); }

fn main() ~ Foreign {
    touch();
}
"#,
    )
    .expect("write foreign receipt fixture");

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
        "foreign receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Foreign"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Foreign"],
        serde_json::json!(["touch"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]
            .as_object()
            .expect("main propagated effects")
            .len(),
        0
    );
}

#[test]
fn check_receipt_records_foreign_static_as_direct_capability_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_foreign_static_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { static QUANTA_ERRNO: i32; }

fn main() ~ Foreign {
    let code = QUANTA_ERRNO;
}
"#,
    )
    .expect("write foreign static receipt fixture");

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
        "foreign static receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Foreign"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Foreign"],
        serde_json::json!(["QUANTA_ERRNO"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]
            .as_object()
            .expect("main propagated effects")
            .len(),
        0
    );
}

#[test]
fn check_reports_foreign_call_inside_macro_argument() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_macro_arg_foreign_call_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { fn touch(); }

fn main() ~ Console {
    println!(touch());
}
"#,
    )
    .expect("write macro argument foreign call fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "macro argument foreign call should fail without Foreign effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Foreign"),
        "diagnostic should name Foreign effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("touch"),
        "diagnostic should name triggering foreign call:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_foreign_static_inside_macro_argument() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_macro_arg_foreign_static_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
extern "C" { static QUANTA_ERRNO: i32; }

fn main() ~ Console + Foreign {
    println!("{}", QUANTA_ERRNO);
}
"#,
    )
    .expect("write macro argument foreign static receipt fixture");

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
        "macro argument foreign static receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Console", "Foreign"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Console"],
        serde_json::json!(["println!"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]["Foreign"],
        serde_json::json!(["QUANTA_ERRNO"])
    );
}

#[test]
fn check_receipt_records_propagated_effects_separately() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_propagated_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated receipt fixture");

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
        "check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["load_config"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["load_config"]
            .as_object()
            .expect("load_config propagated effects")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}

#[test]
fn check_reports_effect_for_effectful_method_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_method_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load(self) ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() {
    let config = Config;
    config.load();
}
"#,
    )
    .expect("write effectful method fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful method call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load"),
        "diagnostic should name triggering method:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_method_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_effectful_method_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load(self) ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() ~ FileSystem {
    let config = Config;
    config.load();
}
"#,
    )
    .expect("write effectful method receipt fixture");

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
        "effectful method receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["load"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["Config.load"])
    );
}

#[test]
fn check_reports_effect_for_effectful_associated_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_associated_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load() ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() {
    Config::load();
}
"#,
    )
    .expect("write effectful associated function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful associated function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Config::load"),
        "diagnostic should name triggering associated function:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_associated_function_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_effectful_associated_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load() ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() ~ FileSystem {
    Config::load();
}
"#,
    )
    .expect("write effectful associated function receipt fixture");

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
        "effectful associated function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["load"]["FileSystem"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["Config::load"])
    );
}

#[test]
fn check_reports_effect_for_effectful_trait_object_method_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_trait_object_method_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
trait Loader {
    fn load(self) ~ FileSystem;
}

fn run(loader: dyn Loader) {
    loader.load();
}
"#,
    )
    .expect("write effectful trait object method fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful dyn trait method call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Loader.load"),
        "diagnostic should name triggering trait object method:\n{}",
        stderr
    );
}

#[test]
fn check_reports_effect_for_effectful_callback_parameter() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_callback_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) {
    loader();
}
"#,
    )
    .expect("write effectful callback fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful callback should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name callback source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_callback_parameter_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_effectful_callback_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) ~ FileSystem {
    loader();
}
"#,
    )
    .expect("write effectful callback receipt fixture");

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
        "effectful callback receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["run"]
            .as_object()
            .expect("run observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["run"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_records_effectful_returning_callback_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_effectful_returning_callback_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: (fn() -> str) with FileSystem) -> str ~ FileSystem {
    loader()
}
"#,
    )
    .expect("write returning effectful callback receipt fixture");

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
        "returning effectful callback receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["propagated_effects"]["run"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_records_effectful_callback_argument_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_effectful_callback_arg_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) ~ FileSystem {
    loader();
}

fn load_config() ~ FileSystem {
    read_file("config.toml");
}

fn main() ~ FileSystem {
    run(load_config);
}
"#,
    )
    .expect("write effectful callback argument receipt fixture");

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
        "effectful callback argument receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["run"]["FileSystem"],
        serde_json::json!(["loader"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "run"])
    );
}

#[test]
fn check_reports_effect_for_effectful_callback_argument_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_callback_arg_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn() with FileSystem) ~ FileSystem {
    loader();
}

fn load_config() ~ FileSystem {
    read_file("config.toml");
}

fn main() {
    run(load_config);
}
"#,
    )
    .expect("write effectful callback argument gate fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful callback argument should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("run"),
        "diagnostic should name wrapper source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name callback argument source:\n{}",
        stderr
    );
}

#[test]
fn check_rejects_effectful_callback_erasure_into_pure_signature() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_callback_erasure_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn run(loader: fn(str) -> str) {
    loader("ops.txt");
}

fn main() {
    run(read_file);
}
"#,
    )
    .expect("write effectful callback erasure fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful callback should not be accepted by a pure callback signature"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name erased FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("fn(str) -> str"),
        "diagnostic should show the pure callback boundary:\n{}",
        stderr
    );
}

#[test]
fn check_reports_effect_for_ambient_function_alias_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_ambient_alias_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write ambient alias fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "ambient helper alias call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name alias call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_ambient_function_alias_as_propagated_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_ambient_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write ambient alias receipt fixture");

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
        "ambient alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_clears_stale_sources_for_shadowed_ambient_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_shadowed_ambient_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn main() ~ FileSystem {
    let loader = load_config;
    let loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write shadowed ambient alias receipt fixture");

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
        "shadowed ambient alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_function_alias_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_assigned_effectful_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("config.toml");
}

fn load_secret() ~ FileSystem {
    read_file("secret.toml");
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    loader = load_secret;
    loader();
}
"#,
    )
    .expect("write assigned effectful alias receipt fixture");

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
        "assigned effectful alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_struct_field_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_assigned_effectful_field_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut ops = Ops { loader: load_config };
    ops.loader = load_secret;
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write assigned effectful field receipt fixture");

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
        "assigned effectful field receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_struct_object_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_assigned_effectful_struct_object_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut ops = Ops { loader: load_config };
    let defaults = Ops { loader: load_secret };
    ops = defaults;
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write assigned effectful struct object receipt fixture");

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
        "assigned effectful struct object receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_tuple_field_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_assigned_effectful_tuple_field_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loaders = (load_config,);
    loaders.0 = load_secret;
    (loaders.0)("ops.txt");
}
"#,
    )
    .expect("write assigned effectful tuple field receipt fixture");

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
        "assigned effectful tuple field receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loaders.0"])
    );
}

#[test]
fn check_receipt_rebinds_assigned_effectful_index_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_assigned_effectful_index_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loaders = [load_config];
    loaders[0] = load_secret;
    (loaders[0])("ops.txt");
}
"#,
    )
    .expect("write assigned effectful index receipt fixture");

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
        "assigned effectful index receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loaders[0]"])
    );
}

#[test]
fn check_receipt_records_repeated_effectful_index_source_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_repeated_effectful_index_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let loaders = [load_config; 2];
    (loaders[1])("ops.txt");
}
"#,
    )
    .expect("write repeated effectful index receipt fixture");

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
        "repeated indexed effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "loaders[1]"])
    );
}

#[test]
fn check_receipt_clears_stale_sources_for_assigned_ambient_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_assigned_ambient_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    loader = read_file;
    loader("ops.txt");
}
"#,
    )
    .expect("write assigned ambient alias receipt fixture");

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
        "assigned ambient alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_effectful_closure_literal_is_pure_until_called() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_closure_literal_pure_until_called_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loader = |path: str| read_file(path);
}
"#,
    )
    .expect("write effectful closure literal fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "defining an effectful closure should not perform its effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_effectful_tuple_struct_constructor_is_pure_until_called() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_tuple_struct_constructor_pure_until_called_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Slot((fn() -> str) with FileSystem);

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn main() {
    let slot = Slot(load_config);
}
"#,
    )
    .expect("write effectful tuple struct constructor fixture");

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
        "storing an effectful callback in a tuple struct should stay pure until call\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]
            .as_object()
            .expect("main propagated effects")
            .len(),
        0
    );
}

#[test]
fn check_reports_effect_for_effectful_closure_alias_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_closure_alias_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loader = |path: str| read_file(path);
    loader("ops.txt");
}
"#,
    )
    .expect("write effectful closure alias fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "calling an effectful closure should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name closure alias source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_closure_alias_as_propagated_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_effectful_closure_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loader = |path: str| read_file(path);
    loader("ops.txt");
}
"#,
    )
    .expect("write effectful closure alias receipt fixture");

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
        "effectful closure alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loader"])
    );
}

#[test]
fn check_reports_effect_for_immediate_effectful_closure_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_immediate_effectful_closure_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    (|path: str| read_file(path))("ops.txt");
}
"#,
    )
    .expect("write immediate effectful closure fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "immediate effectful closure call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("<closure>"),
        "diagnostic should name anonymous closure source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_immediate_effectful_closure_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_immediate_effectful_closure_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    (|path: str| read_file(path))("ops.txt");
}
"#,
    )
    .expect("write immediate effectful closure receipt fixture");

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
        "immediate effectful closure receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["<closure>"])
    );
}

#[test]
fn check_effectful_async_block_is_pure_until_awaited() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_effectful_async_block_pure_until_awaited_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let task = async { read_file("ops.txt") };
}
"#,
    )
    .expect("write effectful async block fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "constructing an effectful async block should not perform its effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_reports_effect_for_awaited_effectful_async_block() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_awaited_effectful_async_block_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let task = async { read_file("ops.txt") };
    task.await;
}
"#,
    )
    .expect("write awaited effectful async block fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "awaited effectful async block should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("task"),
        "diagnostic should name awaited async source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_awaited_async_block_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_awaited_async_block_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let task = async { read_file("ops.txt") };
    task.await;
}
"#,
    )
    .expect("write awaited async block receipt fixture");

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
        "awaited async block receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["task", "task <- read_file"])
    );
}

#[test]
fn check_rejects_await_operator_on_selected_effectful_function_value() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_await_selected_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader.await;
}
"#,
    )
    .expect("write await selected effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "await operator on selected effectful function value should be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("await"),
        "diagnostic should name the await operator:\n{}",
        stderr
    );
    assert!(
        stderr.contains("fn()"),
        "diagnostic should name the invalid function operand:\n{}",
        stderr
    );
}

#[test]
fn check_selected_async_block_is_pure_until_awaited() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_selected_async_block_pure_until_awaited_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let use_secret = true;
    let task = if use_secret {
        async { read_file("ops.txt") }
    } else {
        async { getenv("TOKEN") }
    };
}
"#,
    )
    .expect("write selected async block fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        output.status.success(),
        "selecting an effectful async block should not perform its effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_reports_effects_for_awaited_selected_async_block() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_awaited_selected_async_block_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let use_secret = true;
    let task = if use_secret {
        async { read_file("ops.txt") }
    } else {
        async { getenv("TOKEN") }
    };
    task.await;
}
"#,
    )
    .expect("write awaited selected async block fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "awaited selected async block should fail without declared effects"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Environment"),
        "diagnostic should name Environment effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("task"),
        "diagnostic should name awaited selected async source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name selected async FileSystem origin:\n{}",
        stderr
    );
    assert!(
        stderr.contains("getenv"),
        "diagnostic should name selected async Environment origin:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_awaited_selected_async_block_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_awaited_selected_async_block_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem + Environment {
    let use_secret = true;
    let task = if use_secret {
        async { read_file("ops.txt") }
    } else {
        async { getenv("TOKEN") }
    };
    task.await;
}
"#,
    )
    .expect("write awaited selected async block receipt fixture");

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
        "awaited selected async block receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["task", "task <- read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["Environment"],
        serde_json::json!(["task", "task <- getenv"])
    );
}

#[test]
fn check_reports_effects_for_awaited_match_selected_async_block() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_awaited_match_selected_async_block_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let mode = 1;
    let task = match mode {
        0 => async { read_file("ops.txt") },
        _ => async { getenv("TOKEN") },
    };
    task.await;
}
"#,
    )
    .expect("write awaited match selected async block fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "awaited match selected async block should fail without declared effects"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("Environment"),
        "diagnostic should name Environment effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("task"),
        "diagnostic should name awaited match selected async source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("read_file"),
        "diagnostic should name match-selected async FileSystem origin:\n{}",
        stderr
    );
    assert!(
        stderr.contains("getenv"),
        "diagnostic should name match-selected async Environment origin:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_awaited_match_selected_async_block_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_awaited_match_selected_async_block_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem + Environment {
    let mode = 1;
    let task = match mode {
        0 => async { read_file("ops.txt") },
        _ => async { getenv("TOKEN") },
    };
    task.await;
}
"#,
    )
    .expect("write awaited match selected async block receipt fixture");

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
        "awaited match selected async block receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["task", "task <- read_file"])
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["Environment"],
        serde_json::json!(["task", "task <- getenv"])
    );
}

#[test]
fn check_reports_effect_for_effectful_struct_field_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_struct_field_effect_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn main() {
    let ops = Ops { loader: read_file };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct field effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful struct field call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("ops.loader"),
        "diagnostic should name field call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_struct_field_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_struct_field_effect_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn main() ~ FileSystem {
    let ops = Ops { loader: read_file };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct field effect receipt fixture");

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
        "struct field effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["ops.loader"])
    );
}

#[test]
fn check_receipt_records_struct_update_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_struct_update_effectful_field_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Ops { loader: load_config };
    let ops = Ops { ..defaults };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct update effectful field receipt fixture");

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
        "struct update field effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_nested_struct_update_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_nested_struct_update_effectful_field_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let outer = Outer { ..defaults };
    (outer.ops.loader)("ops.txt");
}
"#,
    )
    .expect("write nested struct update effectful field receipt fixture");

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
        "nested struct update effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "outer.ops.loader"])
    );
}

#[test]
fn check_receipt_records_destructured_nested_struct_update_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_destructured_nested_struct_update_effectful_field_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let outer = Outer { ..defaults };
    let Outer { ops } = outer;
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write destructured nested struct update effectful field receipt fixture");

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
        "destructured nested struct update effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_struct_update_expression_destructured_effectful_field_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_struct_update_expression_destructured_effectful_field_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let Outer { ops } = Outer { ..defaults };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct update expression destructured effectful field receipt fixture");

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
        "struct update expression destructured effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_struct_update_expression_explicit_field_destructured_origin() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_struct_update_expression_explicit_field_destructured_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let defaults = Outer { ops: Ops { loader: load_config } };
    let replacement = Ops { loader: load_config };
    let Outer { ops } = Outer { ops: replacement, ..defaults };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write struct update expression explicit field destructured receipt fixture");

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
        "struct update expression explicit field destructured receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_tuple_literal_destructured_aggregate_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_tuple_literal_destructured_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let replacement = Ops { loader: load_config };
    let (ops,) = (replacement,);
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write tuple literal destructured aggregate receipt fixture");

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
        "tuple literal destructured aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_stored_variant_aggregate_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_stored_variant_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

enum Slot {
    Ready(Ops),
    Empty
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let replacement = Ops { loader: load_config };
    let slot = Slot::Ready(replacement);
    match slot {
        Slot::Ready(ops) => { (ops.loader)("ops.txt"); }
        Slot::Empty => { }
    }
}
"#,
    )
    .expect("write stored variant aggregate receipt fixture");

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
        "stored variant aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_selected_aggregate_origin_without_stale_branch_aliases() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_selected_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let ops = if use_secret { secret } else { config };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write selected aggregate receipt fixture");

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
        "selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_tuple_destructured_selected_aggregate_origins() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_tuple_selected_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let (ops,) = (if use_secret { secret } else { config },);
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write tuple destructured selected aggregate receipt fixture");

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
        "tuple destructured selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_nested_if_let_selected_aggregate_origins() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_nested_if_let_selected_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let outer = Outer { ops: if let Slot::Ready(version) = slot { secret } else { config } };
    (outer.ops.loader)("ops.txt");
}
"#,
    )
    .expect("write nested if-let selected aggregate receipt fixture");

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
        "nested if-let selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "outer.ops.loader"])
    );
}

#[test]
fn check_receipt_records_tuple_destructured_nested_if_let_selected_aggregate_origins() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_tuple_nested_if_let_selected_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let config = Ops { loader: load_config };
    let secret = Ops { loader: load_secret };
    let Outer { ops } = Outer { ops: if let Slot::Ready(version) = slot { secret } else { config } };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write destructured nested if-let selected aggregate receipt fixture");

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
        "destructured nested if-let selected aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_shorthand_aggregate_field_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_shorthand_aggregate_field_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    let outer = Outer { ops };
    (outer.ops.loader)("ops.txt");
}
"#,
    )
    .expect("write shorthand aggregate field receipt fixture");

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
        "shorthand aggregate field receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "outer.ops.loader"])
    );
}

#[test]
fn check_receipt_records_direct_shorthand_aggregate_destructuring_origin_without_stale_alias() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_direct_shorthand_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

struct Outer {
    ops: Ops
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    let Outer { ops: unpacked } = Outer { ops };
    (unpacked.loader)("ops.txt");
}
"#,
    )
    .expect("write direct shorthand aggregate destructuring receipt fixture");

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
        "direct shorthand aggregate destructuring receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "unpacked.loader"])
    );
}

#[test]
fn check_receipt_clears_stale_aggregate_sources_for_shadowed_opaque_binding() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_shadowed_opaque_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn make_ops() -> Ops {
    Ops { loader: read_file }
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    let ops = make_ops();
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write shadowed opaque aggregate receipt fixture");

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
        "shadowed opaque aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["make_ops().loader", "ops.loader"])
    );
}

#[test]
fn check_receipt_clears_outer_aggregate_sources_for_inner_shadowed_opaque_binding() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_inner_shadowed_opaque_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn make_ops() -> Ops {
    Ops { loader: read_file }
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    {
        let ops = make_ops();
        (ops.loader)("ops.txt");
    }
}
"#,
    )
    .expect("write inner shadowed opaque aggregate receipt fixture");

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
        "inner shadowed opaque aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["make_ops().loader", "ops.loader"])
    );
}

#[test]
fn check_receipt_clears_outer_aggregate_sources_when_inner_shadow_is_copied() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_inner_shadowed_aggregate_copy_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn make_ops() -> Ops {
    Ops { loader: read_file }
}

fn main() ~ FileSystem {
    let ops = Ops { loader: load_config };
    {
        let ops = make_ops();
        let copied = ops;
        (copied.loader)("ops.txt");
    }
}
"#,
    )
    .expect("write inner shadowed aggregate copy receipt fixture");

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
        "inner shadowed aggregate copy receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["copied.loader", "make_ops().loader", "ops.loader"])
    );
}

#[test]
fn check_reports_effect_for_effectful_tuple_field_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_tuple_field_effect_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loaders = (read_file,);
    (loaders.0)("ops.txt");
}
"#,
    )
    .expect("write tuple field effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful tuple field call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loaders.0"),
        "diagnostic should name tuple field call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_refreshes_outer_alias_source_after_inner_block_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_inner_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write inner assignment alias receipt fixture");

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
        "inner assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "loader"])
    );
}

#[test]
fn check_receipt_refreshes_outer_aggregate_source_after_inner_block_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_inner_assignment_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut ops = Ops { loader: load_config };
    {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write inner assignment aggregate receipt fixture");

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
        "inner assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_conditional_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_conditional_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let mut loader = load_config;
    if use_secret {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write conditional assignment alias receipt fixture");

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
        "conditional assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_conditional_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_conditional_assignment_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let mut ops = Ops { loader: load_config };
    if use_secret {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write conditional assignment aggregate receipt fixture");

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
        "conditional assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_if_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_if_let_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
    Empty,
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut loader = load_config;
    if let Slot::Ready(version) = slot {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write if-let assignment alias receipt fixture");

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
        "if-let assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_if_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_if_let_assignment_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

enum Slot {
    Ready(i32),
    Empty,
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut ops = Ops { loader: load_config };
    if let Slot::Ready(version) = slot {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write if-let assignment aggregate receipt fixture");

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
        "if-let assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_if_else_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_if_else_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let use_secret = true;
    let mut loader = load_config;
    if use_secret {
        loader = load_secret;
    } else {
        loader = load_backup;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write if-else assignment alias receipt fixture");

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
        "if-else assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_match_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_match_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mode = 0;
    let mut loader = load_config;
    match mode {
        0 => { loader = load_secret; }
        _ => { loader = load_backup; }
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write match assignment alias receipt fixture");

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
        "match assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_keeps_pre_match_alias_source_after_guarded_match_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_guarded_match_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mode = true;
    let allow_secret = false;
    let mut loader = load_config;
    match mode {
        true if allow_secret => { loader = load_secret; },
        false => { loader = load_backup; }
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write guarded match assignment alias receipt fixture");

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
        "guarded match assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_match_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_match_assignment_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_backup(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mode = 0;
    let mut ops = Ops { loader: load_config };
    match mode {
        0 => { ops.loader = load_secret; }
        _ => { ops.loader = load_backup; }
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write match assignment aggregate receipt fixture");

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
        "match assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_backup", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_while_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_while_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let should_reload = false;
    let mut loader = load_config;
    while should_reload {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write while assignment alias receipt fixture");

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
        "while assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_while_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_while_assignment_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let should_reload = false;
    let mut ops = Ops { loader: load_config };
    while should_reload {
        ops.loader = load_secret;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write while assignment aggregate receipt fixture");

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
        "while assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_for_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_for_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let mut loader = load_config;
    for item in [0] {
        loader = load_secret;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write for assignment alias receipt fixture");

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
        "for assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_while_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_while_let_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut loader = load_config;
    while let Slot::Ready(version) = slot {
        loader = load_secret;
        break;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write while-let assignment alias receipt fixture");

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
        "while-let assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_while_let_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_while_let_assignment_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

enum Slot {
    Ready(i32),
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let mut ops = Ops { loader: load_config };
    while let Slot::Ready(version) = slot {
        ops.loader = load_secret;
        break;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write while-let assignment aggregate receipt fixture");

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
        "while-let assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_merges_outer_alias_sources_after_loop_break_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_loop_break_assignment_alias_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let stop_now = true;
    let mut loader = load_config;
    loop {
        if stop_now {
            break;
        };
        loader = load_secret;
        break;
    };
    loader("ops.txt");
}
"#,
    )
    .expect("write loop break assignment alias receipt fixture");

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
        "loop break assignment alias receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_merges_outer_aggregate_sources_after_loop_break_assignment() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_loop_break_assignment_aggregate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn(str) -> str) with FileSystem
}

fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    let stop_now = true;
    let mut ops = Ops { loader: load_config };
    loop {
        if stop_now {
            break;
        };
        ops.loader = load_secret;
        break;
    };
    (ops.loader)("ops.txt");
}
"#,
    )
    .expect("write loop break assignment aggregate receipt fixture");

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
        "loop break assignment aggregate receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "ops.loader"])
    );
}

#[test]
fn check_receipt_records_effectful_tuple_field_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_tuple_field_effect_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loaders = (read_file,);
    (loaders.0)("ops.txt");
}
"#,
    )
    .expect("write tuple field effect receipt fixture");

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
        "tuple field effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loaders.0"])
    );
}

#[test]
fn check_reports_effect_for_effectful_index_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_index_effect_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() {
    let loaders = [read_file];
    (loaders[0])("ops.txt");
}
"#,
    )
    .expect("write index effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "effectful index call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loaders[0]"),
        "diagnostic should name indexed call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_effectful_index_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_index_effect_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn main() ~ FileSystem {
    let loaders = [read_file];
    (loaders[0])("ops.txt");
}
"#,
    )
    .expect("write index effect receipt fixture");

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
        "indexed effect receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["loaders[0]"])
    );
}

#[test]
fn check_reports_effect_for_immediate_returned_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_returned_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn make_loader() -> (fn(str) -> str) with FileSystem {
    read_file
}

fn main() {
    make_loader()("ops.txt");
}
"#,
    )
    .expect("write returned effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "immediate returned effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("make_loader()"),
        "diagnostic should name returned function call source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_immediate_returned_effectful_function_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_returned_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn make_loader() -> (fn(str) -> str) with FileSystem {
    read_file
}

fn main() ~ FileSystem {
    make_loader()("ops.txt");
}
"#,
    )
    .expect("write returned effectful function receipt fixture");

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
        "returned effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["make_loader()"])
    );
}

#[test]
fn check_reports_effect_for_pipe_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_pipe_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() {
    "ops.toml" |> load_config;
}
"#,
    )
    .expect("write pipe effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "pipe effectful function call should fail without FileSystem effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for pipe effectful function call\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_receipt_records_pipe_effectful_function_call_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_pipe_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() ~ FileSystem {
    "ops.toml" |> load_config;
}
"#,
    )
    .expect("write pipe effectful function receipt fixture");

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
        "pipe effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}

#[test]
fn check_rejects_function_values_with_shift_operator() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_function_shift_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn load_secret(path: str) -> str ~ FileSystem {
    read_file(path)
}

fn main() {
    let pipeline = load_config >> load_secret;
}
"#,
    )
    .expect("write function shift fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "function values used with >> should be rejected\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("binary operator `>>`") && message.contains("FileSystem")
        }),
        "missing invalid function shift diagnostic\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_reports_effect_for_control_flow_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_control_flow_selected_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    (if use_secret { load_secret } else { load_config })();
}
"#,
    )
    .expect("write control-flow selected effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "control-flow selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name one possible branch source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name the other possible branch source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_control_flow_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_control_flow_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    (if use_secret { load_secret } else { load_config })();
}
"#,
    )
    .expect("write control-flow selected effectful function receipt fixture");

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
        "control-flow selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret"])
    );
}

#[test]
fn check_reports_effect_for_match_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_match_selected_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    (match use_secret {
        true => load_secret,
        false => load_config,
    })();
}
"#,
    )
    .expect("write match selected effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "match selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name one possible match source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name the other possible match source:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_match_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_match_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    (match use_secret {
        true => load_secret,
        false => load_config,
    })();
}
"#,
    )
    .expect("write match selected effectful function receipt fixture");

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
        "match selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret"])
    );
}

#[test]
fn check_reports_effect_for_let_bound_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_let_bound_selected_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write let-bound selected effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "let-bound selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name selected binding source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name one possible selected source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name the other possible selected source:\n{}",
        stderr
    );
}

#[test]
fn check_reports_effect_for_if_let_bound_selected_effectful_function_call() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_if_let_bound_selected_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let slot = Slot::Ready(0);
    let loader = if let Slot::Ready(version) = slot { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write if-let-bound selected effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "if-let-bound selected effectful function call should fail without FileSystem effect"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should name FileSystem effect:\n{}",
        stderr
    );
    assert!(
        stderr.contains("loader"),
        "diagnostic should name selected binding source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_config"),
        "diagnostic should name if-let fallback source:\n{}",
        stderr
    );
    assert!(
        stderr.contains("load_secret"),
        "diagnostic should name if-let matched source:\n{}",
        stderr
    );
}

#[test]
fn check_rejects_try_operator_on_selected_effectful_function_value() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_try_selected_effectful_function_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader?();
}
"#,
    )
    .expect("write try selected effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "try operator on selected effectful function value should be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`?`"),
        "diagnostic should name the try operator:\n{}",
        stderr
    );
    assert!(
        stderr.contains("fn()"),
        "diagnostic should name the invalid function operand:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_let_bound_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_let_bound_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write let-bound selected effectful function receipt fixture");

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
        "let-bound selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_if_let_bound_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_if_let_bound_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready(i32),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let slot = Slot::Ready(0);
    let loader = if let Slot::Ready(version) = slot { load_secret } else { load_config };
    loader();
}
"#,
    )
    .expect("write if-let-bound selected effectful function receipt fixture");

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
        "if-let-bound selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_cast_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_cast_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loader = (if use_secret { load_secret } else { load_config }) as (fn() -> str) with FileSystem;
    loader();
}
"#,
    )
    .expect("write cast selected effectful function receipt fixture");

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
        "cast selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_rejects_cast_laundering_selected_effectful_function_to_pure_callback() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_cast_selected_effectful_function_to_pure_gate_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let loader = (if use_secret { load_secret } else { load_config }) as fn() -> str;
    loader();
}
"#,
    )
    .expect("write pure cast selected effectful function fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("run quantac check");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "casting selected effectful function to pure callback should be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FileSystem"),
        "diagnostic should retain the erased effect row:\n{}",
        stderr
    );
}

#[test]
fn check_receipt_records_ref_deref_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_ref_deref_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loader = if use_secret { load_secret } else { load_config };
    let loader_ref = &loader;
    (*loader_ref)();
}
"#,
    )
    .expect("write ref-deref selected effectful function receipt fixture");

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
        "ref-deref selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader", "loader_ref"])
    );
}

#[test]
fn check_receipt_records_tuple_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_tuple_destructured_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let (loader,) = (if use_secret { load_secret } else { load_config },);
    loader();
}
"#,
    )
    .expect("write tuple-destructured effectful function receipt fixture");

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
        "tuple-destructured effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_struct_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_struct_destructured_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Ops {
    loader: (fn() -> str) with FileSystem
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let ops = Ops { loader: if use_secret { load_secret } else { load_config } };
    let Ops { loader } = ops;
    loader();
}
"#,
    )
    .expect("write struct-destructured effectful function receipt fixture");

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
        "struct-destructured effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_slice_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_slice_destructured_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let loaders = [if use_secret { load_secret } else { load_config }];
    let [loader] = loaders;
    loader();
}
"#,
    )
    .expect("write slice-destructured effectful function receipt fixture");

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
        "slice-destructured effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_tuple_struct_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_tuple_struct_destructured_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
struct Slot((fn() -> str) with FileSystem);

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let slot = Slot(if use_secret { load_secret } else { load_config });
    let Slot(loader) = slot;
    loader();
}
"#,
    )
    .expect("write tuple-struct destructured selected effectful function receipt fixture");

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
        "tuple-struct destructured selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_receipt_records_enum_variant_destructured_selected_effectful_function_sources() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_enum_variant_destructured_selected_effectful_function_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready((fn() -> str) with FileSystem),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() ~ FileSystem {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready(load_secret)
    } else {
        Slot::Ready(load_config)
    };
    match slot {
        Slot::Ready(loader) => loader(),
    };
}
"#,
    )
    .expect("write enum-variant destructured selected effectful function receipt fixture");

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
        "enum-variant destructured selected effectful function receipt check should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["observed_capabilities"]["main"]
            .as_object()
            .expect("main observed capabilities")
            .len(),
        0
    );
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
}

#[test]
fn check_struct_enum_variant_destructured_callback_requires_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_struct_enum_variant_callback_requires_effect_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready { loader: (fn() -> str) with FileSystem },
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready { loader: load_secret }
    } else {
        Slot::Ready { loader: load_config }
    };
    match slot {
        Slot::Ready { loader } => loader(),
    };
}
"#,
    )
    .expect("write struct enum variant callback effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "struct enum variant callback without declared effect should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for struct enum variant callback\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_if_let_destructured_enum_variant_callback_requires_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_if_let_enum_variant_callback_requires_effect_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready((fn() -> str) with FileSystem),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready(load_secret)
    } else {
        Slot::Ready(load_config)
    };
    if let Slot::Ready(loader) = slot {
        loader();
    };
}
"#,
    )
    .expect("write if-let enum variant callback effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "if-let destructured callback without declared effect should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for if-let callback\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
}

#[test]
fn check_while_let_destructured_enum_variant_callback_requires_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_while_let_enum_variant_callback_requires_effect_{}.quanta",
        std::process::id()
    ));
    fs::write(
        &fixture,
        r#"
enum Slot {
    Ready((fn() -> str) with FileSystem),
}

fn load_config() -> str ~ FileSystem {
    read_file("config.toml")
}

fn load_secret() -> str ~ FileSystem {
    read_file("secret.toml")
}

fn main() {
    let use_secret = true;
    let slot = if use_secret {
        Slot::Ready(load_secret)
    } else {
        Slot::Ready(load_config)
    };
    while let Slot::Ready(loader) = slot {
        loader();
        break;
    };
}
"#,
    )
    .expect("write while-let enum variant callback effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check --receipt -");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "while-let destructured callback without declared effect should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config", "load_secret", "loader"])
    );
    let diagnostics = receipt["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diag| {
            let message = diag["message"].as_str().unwrap_or_default();
            message.contains("FileSystem") && message.contains("main")
        }),
        "missing FileSystem diagnostic for while-let callback\nreceipt:\n{}",
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize")
    );
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
fn check_receipt_input_digests_track_included_source_changes() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_inputs_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create input digest fixture dir");
    let entry = dir.join("entry.quanta");
    let shared = dir.join("shared.quanta");
    fs::write(
        &entry,
        r#"include!("shared.quanta");
fn main() ~ Console { println!("{}", value()); }
"#,
    )
    .expect("write entry fixture");
    fs::write(&shared, "fn value() -> i32 { 7 }\n").expect("write first include fixture");

    let first_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run first input digest receipt");
    assert!(
        first_output.status.success(),
        "first check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first_output.stdout),
        String::from_utf8_lossy(&first_output.stderr)
    );
    let first_receipt = receipt_from_stdout(&first_output);
    let first_entry_digest = first_receipt["source_digest"]["hex"]
        .as_str()
        .expect("entry source digest")
        .to_string();
    let first_input_entry = input_digest_hex(&first_receipt, "entry", "entry.quanta");
    let first_input_include = input_digest_hex(&first_receipt, "include", "shared.quanta");
    let first_graph_digest = input_graph_digest_hex(&first_receipt);
    assert_eq!(first_entry_digest, first_input_entry);

    fs::write(&shared, "fn value() -> i32 { 8 }\n").expect("write changed include fixture");
    let second_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run second input digest receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        second_output.status.success(),
        "second check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second_output.stdout),
        String::from_utf8_lossy(&second_output.stderr)
    );
    let second_receipt = receipt_from_stdout(&second_output);
    assert_eq!(second_receipt["source_digest"]["hex"], first_entry_digest);
    assert_eq!(
        input_digest_hex(&second_receipt, "entry", "entry.quanta"),
        first_input_entry
    );
    assert_ne!(input_graph_digest_hex(&second_receipt), first_graph_digest);
    assert_ne!(
        input_digest_hex(&second_receipt, "include", "shared.quanta"),
        first_input_include
    );
}

#[test]
fn check_receipt_input_digests_record_imports_and_modules() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_check_receipt_graph_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let package_dir = dir.join("registry/packages/std-math/src");
    fs::create_dir_all(&package_dir).expect("create import package dir");
    fs::create_dir_all(dir.join("helpers")).expect("create helper module dir");
    let entry = dir.join("entry.quanta");
    let imported = package_dir.join("lib.quanta");
    let module = dir.join("helpers/mod.quanta");

    fs::write(&imported, "fn imported_value() -> i32 { 2 }\n").expect("write import fixture");
    fs::write(&module, "fn module_value() -> i32 { 5 }\n").expect("write module fixture");
    fs::write(
        &entry,
        r#"use std_math;
mod helpers;
fn main() ~ Console {
    println!("{}", imported_value() + helpers::module_value());
}
"#,
    )
    .expect("write graph entry fixture");

    let output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run graph input digest receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "graph check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    input_digest_hex(&receipt, "entry", "entry.quanta");
    input_digest_hex(&receipt, "import", "lib.quanta");
    input_digest_hex(&receipt, "module", "mod.quanta");
}

#[test]
fn check_receipt_input_graph_digest_is_path_portable() {
    let mut graph_digests = Vec::new();
    let mut entry_sources = Vec::new();

    for label in ["left", "right"] {
        let dir = std::env::temp_dir().join(format!(
            "quantalang_check_receipt_graph_digest_{}_{}",
            label,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create graph digest fixture dir");
        let entry = dir.join("entry.quanta");
        let shared = dir.join("shared.quanta");
        fs::write(
            &entry,
            r#"include!("shared.quanta");
fn main() ~ Console { println!("{}", value()); }
"#,
        )
        .expect("write graph digest entry fixture");
        fs::write(&shared, "fn value() -> i32 { 11 }\n")
            .expect("write graph digest include fixture");

        let output = quantac()
            .arg("check")
            .arg(&entry)
            .arg("--receipt")
            .arg("-")
            .output()
            .expect("run graph digest receipt");

        let _ = fs::remove_dir_all(&dir);

        assert!(
            output.status.success(),
            "graph digest check should pass for {label}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let receipt = receipt_from_stdout(&output);
        entry_sources.push(receipt["source"].as_str().unwrap_or("").to_string());
        graph_digests.push(input_graph_digest_hex(&receipt));
    }

    assert_ne!(entry_sources[0], entry_sources[1]);
    assert_eq!(graph_digests[0], graph_digests[1]);
}

#[test]
fn receipt_verify_accepts_fresh_check_receipt() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_fresh_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt verify fixture dir");
    let entry = dir.join("entry.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write receipt verify entry");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write fresh check receipt");
    assert!(
        check_output.status.success(),
        "check should pass before receipt verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify fresh check receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify_output.status.success(),
        "fresh receipt should verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_output.stdout),
        String::from_utf8_lossy(&verify_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stdout).contains("Receipt verified"),
        "stdout should confirm verification:\n{}",
        String::from_utf8_lossy(&verify_output.stdout)
    );
}

#[test]
fn receipt_verify_json_reports_passed_checks() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_json_pass_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt json fixture dir");
    let entry = dir.join("entry.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write receipt json entry");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write check receipt for json verify");
    assert!(
        check_output.status.success(),
        "check should pass before json receipt verify\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        verify_output.status.success(),
        "json receipt verification should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_output.stdout),
        String::from_utf8_lossy(&verify_output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&verify_output.stdout).expect("verification report should be JSON");
    assert_eq!(report["schema"], "quantalang-receipt-verification/v1");
    assert_eq!(report["status"], "passed");
    assert_eq!(
        verification_check(&report, "source_digest")["status"],
        "passed"
    );
    assert_eq!(
        verification_check(&report, "input_graph_digest")["status"],
        "passed"
    );
    assert_eq!(
        verification_check(&report, "policy_profile_digest")["status"],
        "passed"
    );
}

#[test]
fn receipt_verify_expect_profile_rejects_stripped_policy() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_expect_profile_stripped_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect profile fixture dir");
    let entry = dir.join("entry.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write expect profile entry");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("ci-review")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write profile-backed receipt");
    assert!(
        check_output.status.success(),
        "check should pass before stripping policy\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read profile receipt"))
            .expect("profile receipt should parse");
    saved
        .as_object_mut()
        .expect("receipt should be an object")
        .remove("policy");
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize stripped receipt"),
    )
    .expect("write stripped profile receipt");

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-profile")
        .arg("ci-review")
        .output()
        .expect("verify stripped receipt with expected profile");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stripped policy receipt should fail expected-profile verification"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr)
            .contains("receipt built-in profile mismatch"),
        "stderr should report expected profile mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_json_reports_expected_profile_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_expect_profile_json_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect profile json fixture dir");
    let entry = dir.join("entry.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write expect profile json entry");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write pure profile receipt");
    assert!(
        check_output.status.success(),
        "check should pass before expected-profile mismatch\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-profile")
        .arg("ci-review")
        .arg("--json")
        .output()
        .expect("verify mismatched profile receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "mismatched profile receipt should fail json verification"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("profile mismatch report should be JSON");
    assert_eq!(report["status"], "failed");
    let profile_check = verification_check(&report, "expected_profile");
    assert_eq!(profile_check["status"], "failed");
    assert_eq!(profile_check["expected"], "builtin:ci-review");
    assert_eq!(profile_check["actual"], "builtin:pure");
    assert!(
        profile_check["message"]
            .as_str()
            .expect("expected profile failure message")
            .contains("receipt built-in profile mismatch"),
        "profile failure should explain mismatch: {profile_check:#?}"
    );
}

#[test]
fn receipt_verify_json_reports_failed_input_graph() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_json_fail_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt json failure fixture dir");
    let entry = dir.join("entry.quanta");
    let shared = dir.join("shared.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(
        &entry,
        r#"include!("shared.quanta");
fn main() ~ Console { println!("{}", value()); }
"#,
    )
    .expect("write receipt json failure entry");
    fs::write(&shared, "fn value() -> i32 { 7 }\n").expect("write first shared source");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write staleable check receipt for json verify");
    assert!(
        check_output.status.success(),
        "check should pass before json graph mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(&shared, "fn value() -> i32 { 8 }\n").expect("mutate shared source for json verify");
    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify stale receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale json receipt verification should fail"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&verify_output.stdout).expect("failure report should be JSON");
    assert_eq!(report["schema"], "quantalang-receipt-verification/v1");
    assert_eq!(report["status"], "failed");
    let graph_check = verification_check(&report, "input_graph_digest");
    assert_eq!(graph_check["status"], "failed");
    assert!(
        graph_check["message"]
            .as_str()
            .expect("failure check message")
            .contains("input graph digest mismatch"),
        "graph failure should explain mismatch: {graph_check:#?}"
    );
}

#[test]
fn receipt_verify_rejects_changed_policy_file_digest() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_policy_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt policy fixture dir");
    let entry = dir.join("entry.quanta");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write policy receipt entry");
    fs::write(
        &policy,
        r#"{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write initial policy");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write file-backed policy receipt");
    assert!(
        check_output.status.success(),
        "check should pass before policy mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(
        &policy,
        r#"{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["FileSystem"],
  "require_source_digest": true
}
"#,
    )
    .expect("mutate policy file");
    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify stale policy receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale policy digest receipt should fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr).contains("policy source digest mismatch"),
        "stderr should report policy digest mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_json_reports_failed_policy_file_digest() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_policy_json_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt policy json fixture dir");
    let entry = dir.join("entry.quanta");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write policy json receipt entry");
    fs::write(
        &policy,
        r#"{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write initial json policy");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write file-backed policy receipt for json verify");
    assert!(
        check_output.status.success(),
        "check should pass before json policy mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(
        &policy,
        r#"{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["FileSystem"],
  "require_source_digest": true
}
"#,
    )
    .expect("mutate json policy file");
    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify stale policy receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale policy json receipt should fail"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("policy failure report should be JSON");
    assert_eq!(report["status"], "failed");
    let policy_check = verification_check(&report, "policy_source_digest");
    assert_eq!(policy_check["status"], "failed");
    assert!(
        policy_check["message"]
            .as_str()
            .expect("policy failure message")
            .contains("policy source digest mismatch"),
        "policy failure should explain mismatch: {policy_check:#?}"
    );
}

#[test]
fn receipt_verify_expect_policy_digest_rejects_stripped_policy() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_expect_policy_stripped_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect policy digest fixture dir");
    let entry = dir.join("entry.quanta");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write expect policy digest entry");
    fs::write(
        &policy,
        r#"{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write expected policy");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy-backed receipt");
    assert!(
        check_output.status.success(),
        "check should pass before stripping policy\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read policy receipt"))
            .expect("policy receipt should parse");
    let expected_digest = saved["policy"]["source_digest"]["hex"]
        .as_str()
        .expect("policy digest")
        .to_string();
    saved
        .as_object_mut()
        .expect("receipt should be an object")
        .remove("policy");
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize stripped policy receipt"),
    )
    .expect("write stripped policy receipt");

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-policy-digest")
        .arg(format!("sha256:{expected_digest}"))
        .output()
        .expect("verify stripped policy with expected digest");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stripped policy receipt should fail policy digest expectation"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr).contains("receipt policy digest mismatch"),
        "stderr should report expected policy digest mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_expect_policy_digest_rejects_algorithm_tamper() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_expect_policy_algorithm_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect policy digest algorithm fixture dir");
    let entry = dir.join("entry.quanta");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write expect policy digest algorithm entry");
    fs::write(
        &policy,
        r#"{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write expected algorithm policy");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy-backed receipt for algorithm tamper");
    assert!(
        check_output.status.success(),
        "check should pass before tampering policy digest algorithm\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read policy algorithm receipt"))
            .expect("policy algorithm receipt should parse");
    let expected_digest = saved["policy"]["source_digest"]["hex"]
        .as_str()
        .expect("policy digest")
        .to_string();
    saved["policy"]
        .as_object_mut()
        .expect("policy should be an object")
        .remove("source");
    saved["policy"]["source_digest"]["algorithm"] = serde_json::Value::String("sha512".into());
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize algorithm-tampered receipt"),
    )
    .expect("write algorithm-tampered policy receipt");

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-policy-digest")
        .arg(format!("sha256:{expected_digest}"))
        .arg("--json")
        .output()
        .expect("verify algorithm-tampered policy digest");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "algorithm-tampered policy digest receipt should fail policy digest expectation"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("algorithm-tampered policy digest report should be JSON");
    let policy_check = verification_check(&report, "expected_policy_digest");
    assert_eq!(policy_check["status"], "failed");
    assert_eq!(
        policy_check["expected"],
        format!("sha256:{expected_digest}")
    );
    assert_eq!(policy_check["actual"], format!("sha512:{expected_digest}"));
}

#[test]
fn receipt_verify_json_reports_expected_policy_digest_mismatch() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_expect_policy_json_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create expect policy digest json fixture dir");
    let entry = dir.join("entry.quanta");
    let policy = dir.join("policy.json");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write expect policy digest json entry");
    fs::write(
        &policy,
        r#"{
  "schema": "quantalang-check-policy/v1",
  "allowed_effects": ["Console"],
  "require_source_digest": true
}
"#,
    )
    .expect("write expected json policy");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy-backed receipt for json expectation");
    assert!(
        check_output.status.success(),
        "check should pass before policy digest mismatch\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read policy digest receipt"))
            .expect("policy digest receipt should parse");
    let actual_digest = saved["policy"]["source_digest"]["hex"]
        .as_str()
        .expect("policy digest")
        .to_string();
    let wrong_digest = "0".repeat(64);

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--expect-policy-digest")
        .arg(format!("sha256:{wrong_digest}"))
        .arg("--json")
        .output()
        .expect("verify policy digest mismatch as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "mismatched policy digest receipt should fail json verification"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("policy digest mismatch report should be JSON");
    assert_eq!(report["status"], "failed");
    let policy_check = verification_check(&report, "expected_policy_digest");
    assert_eq!(policy_check["status"], "failed");
    assert_eq!(policy_check["expected"], format!("sha256:{wrong_digest}"));
    assert_eq!(policy_check["actual"], format!("sha256:{actual_digest}"));
    assert!(
        policy_check["message"]
            .as_str()
            .expect("expected policy digest failure message")
            .contains("receipt policy digest mismatch"),
        "policy digest failure should explain mismatch: {policy_check:#?}"
    );
}

#[test]
fn receipt_verify_rejects_tampered_observed_capabilities() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_tampered_capabilities_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create tampered receipt fixture dir");
    let entry = dir.join("entry.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write tampered receipt entry");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write receipt before capability tamper");
    assert!(
        check_output.status.success(),
        "check should pass before receipt tamper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read saved receipt"))
            .expect("saved receipt should parse");
    saved["observed_capabilities"]["main"]["Console"] = serde_json::json!(["forged_console"]);
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize tampered receipt"),
    )
    .expect("write tampered receipt");

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify tampered receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "tampered capability receipt should fail verification"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr)
            .contains("receipt observed_capabilities mismatch"),
        "stderr should report observed capability mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_json_reports_tampered_propagated_effects() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_tampered_propagated_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create tampered propagated receipt fixture dir");
    let entry = dir.join("entry.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(
        &entry,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated receipt entry");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write receipt before propagated tamper");
    assert!(
        check_output.status.success(),
        "check should pass before propagated tamper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let mut saved: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read saved propagated receipt"))
            .expect("saved propagated receipt should parse");
    saved["propagated_effects"]["main"]["FileSystem"] = serde_json::json!(["forged_boundary"]);
    fs::write(
        &receipt,
        serde_json::to_string_pretty(&saved).expect("serialize tampered propagated receipt"),
    )
    .expect("write tampered propagated receipt");

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .arg("--json")
        .output()
        .expect("verify tampered propagated receipt as json");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "tampered propagated receipt should fail verification"
    );
    let report: serde_json::Value = serde_json::from_slice(&verify_output.stdout)
        .expect("tampered propagated failure report should be JSON");
    assert_eq!(report["status"], "failed");
    let replay_check = verification_check(&report, "propagated_effects");
    assert_eq!(replay_check["status"], "failed");
    assert!(
        replay_check["message"]
            .as_str()
            .expect("propagated replay failure message")
            .contains("receipt propagated_effects mismatch"),
        "propagated failure should explain mismatch: {replay_check:#?}"
    );
}

#[test]
fn receipt_verify_rejects_changed_input_graph() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_graph_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt graph fixture dir");
    let entry = dir.join("entry.quanta");
    let shared = dir.join("shared.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(
        &entry,
        r#"include!("shared.quanta");
fn main() ~ Console { println!("{}", value()); }
"#,
    )
    .expect("write receipt graph entry");
    fs::write(&shared, "fn value() -> i32 { 7 }\n").expect("write first shared source");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write graph receipt");
    assert!(
        check_output.status.success(),
        "check should pass before graph mutation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    fs::write(&shared, "fn value() -> i32 { 8 }\n").expect("mutate shared source");
    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt)
        .output()
        .expect("verify stale graph receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "stale input graph receipt should fail"
    );
    assert!(
        String::from_utf8_lossy(&verify_output.stderr).contains("input graph digest mismatch"),
        "stderr should report graph mismatch:\n{}",
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn receipt_verify_rejects_tampered_builtin_profile_digest() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_receipt_verify_profile_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create receipt profile fixture dir");
    let entry = dir.join("entry.quanta");
    let receipt_path = dir.join("receipt.json");
    fs::write(&entry, r#"fn main() {}"#).expect("write pure entry");

    let check_output = quantac()
        .arg("check")
        .arg(&entry)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg(&receipt_path)
        .output()
        .expect("write built-in profile receipt");
    assert!(
        check_output.status.success(),
        "check should pass before profile digest tamper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let receipt_text = fs::read_to_string(&receipt_path).expect("read receipt json");
    let mut receipt_json: serde_json::Value =
        serde_json::from_str(&receipt_text).expect("receipt should be JSON");
    receipt_json["policy"]["profile_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
    fs::write(
        &receipt_path,
        serde_json::to_string_pretty(&receipt_json).expect("serialize tampered receipt"),
    )
    .expect("write tampered receipt");

    let verify_output = quantac()
        .args(["receipt", "verify"])
        .arg(&receipt_path)
        .output()
        .expect("verify tampered profile receipt");

    let _ = fs::remove_dir_all(&dir);

    assert!(
        !verify_output.status.success(),
        "tampered profile digest receipt should fail"
    );
    let stderr = String::from_utf8_lossy(&verify_output.stderr);
    assert!(
        stderr.contains("built-in policy profile digest mismatch"),
        "stderr should report profile digest mismatch:\n{stderr}"
    );
    assert!(
        stderr.contains("pure"),
        "stderr should name the profile:\n{stderr}"
    );
}

#[test]
fn check_policy_allows_console_receipt() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_console_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "console_allow",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["Console"],
          "require_source_digest": true
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write policy console fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with passing policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "console policy check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "passed");
    assert_eq!(receipt["policy"]["schema"], "quantalang-check-policy/v1");
    assert_eq!(receipt["policy"]["status"], "passed");
    assert_eq!(receipt["policy"]["source_digest"]["algorithm"], "sha256");
    assert_eq!(
        receipt["policy"]["source_digest"]["hex"]
            .as_str()
            .expect("policy digest")
            .len(),
        64
    );
    assert!(receipt["policy"]["violations"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn check_policy_denies_filesystem_even_when_typecheck_passes() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_deny_fs_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "deny_fs",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "denied_effects": ["FileSystem"]
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write denied filesystem fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with denied filesystem policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(!output.status.success(), "policy denial should fail check");
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("policy denies effect `FileSystem`")
        }),
        "expected FileSystem denied violation in {violations:#?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Policy violation"),
        "stderr should include policy diagnostic:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_policy_denies_gpu_even_when_typecheck_passes() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_deny_gpu_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "deny_gpu",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "denied_effects": ["Gpu"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Gpu { quanta_vk_init(); }"#)
        .expect("write denied gpu fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with denied gpu policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(!output.status.success(), "policy denial should fail check");
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "Gpu"
                && violation["function"] == "main"
                && violation["source"] == "quanta_vk_init"
        }),
        "expected Gpu denied violation in {violations:#?}"
    );
}

#[test]
fn check_policy_rejects_unknown_effect_name() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_unknown_effect_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unknown_effect",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "denied_effects": ["Netwrok"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() {}"#).expect("write unknown policy effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with unknown policy effect");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "unknown policy effect should fail check\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["status"], "failed");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnknownPolicyEffect"
                && violation["effect"] == "Netwrok"
                && violation["surface"] == "denied_effects"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("unknown effect")
        }),
        "expected unknown policy effect violation in {violations:#?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown effect"),
        "stderr should report unknown policy effect:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_policy_allows_source_defined_effect_name() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_user_effect_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "user_effect",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["Audit"]
        }"#,
    );
    fs::write(
        &fixture,
        r#"
effect Audit {
    fn record();
}

fn main() ~ Audit {}
"#,
    )
    .expect("write source-defined effect fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with source-defined effect policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "source-defined effect should be accepted by policy validator\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert_eq!(
        receipt["declared_effects"]["main"],
        serde_json::json!(["Audit"])
    );
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_allow_list_rejects_unlisted_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_allow_list_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "allow_console_only",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write allow-list filesystem fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with allow-list policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "unlisted effect should fail policy"
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
        }),
        "expected FileSystem disallowed violation in {violations:#?}"
    );
}

#[test]
fn check_policy_required_effect_allowlist_rejects_unlisted_declared_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_require_effect_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_effect_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": [],
          "require_effect_allowlist": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
effect Audit {
    fn record();
}

fn main() ~ Audit {}
"#,
    )
    .expect("write required effect allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with required effect allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "required empty effect allowlist should reject declared effect\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "Audit"
                && violation["function"] == "main"
        }),
        "expected Audit disallowed violation in {violations:#?}"
    );
}

#[test]
fn check_policy_direct_allowlist_rejects_unapproved_direct_effect() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_direct_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "direct_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write direct allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with direct allowlist policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject direct helper\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "read_file"
        }),
        "expected direct effect allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_provenance_allowlists_accept_boundary_and_caller() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_provenance_accept_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "provenance_accept",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write provenance allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with provenance allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept allowlisted provenance\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_direct_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_unused_direct_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_direct_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config", "legacy_loader"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused direct allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with strict allowlist coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused direct allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedDirectEffectAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "legacy_loader"
                && violation["surface"] == "direct_effect_allowlist"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("not matched")
        }),
        "expected unused direct allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_propagated_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_unused_propagated_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_propagated_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main", "legacy_workflow"]
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused propagated allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with strict propagated allowlist coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused propagated allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedPropagatedEffectAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "legacy_workflow"
                && violation["surface"] == "propagated_effect_allowlist"
                && violation["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("not matched")
        }),
        "expected unused propagated allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_strict_allowlist_coverage_accepts_used_entries() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_used_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "used_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write used allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with used strict allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "strict policy should accept fully-used allowlist entries\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_requires_direct_provenance_allowlist_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_require_direct_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_direct_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "require_provenance_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"fn main() ~ FileSystem { read_file("ops.txt"); }"#,
    )
    .expect("write required direct allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with required direct allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require explicit direct provenance allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "read_file"
        }),
        "expected required direct provenance violation in {violations:#?}"
    );
}

#[test]
fn check_policy_requires_propagated_provenance_allowlist_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_require_propagated_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_propagated_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "require_provenance_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required propagated allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with required propagated allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require explicit propagated provenance allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_config"
        }),
        "expected required propagated provenance violation in {violations:#?}"
    );
}

#[test]
fn check_policy_required_provenance_allowlists_accept_explicit_entries() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_required_allowlists_accept_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "required_allowlists_accept",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_provenance_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required allowlists accept fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with required explicit allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept explicit direct and propagated allowlists\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_direct_capability_source_allowlist_rejects_unapproved_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_direct_source_reject_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "direct_source_reject",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    write_file("ops.txt", "x");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write direct source allowlist rejection fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with direct source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject unapproved direct capability source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectCapabilitySourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "load_config"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "write_file"
        }),
        "expected direct capability source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_direct_capability_source_allowlist_accepts_approved_source() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_direct_source_accept_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "direct_source_accept",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write direct source allowlist acceptance fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with approved direct source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept approved direct capability source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_direct_capability_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_unused_direct_source_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_direct_source_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file", "legacy_reader"]
            }
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused direct source allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with strict direct source coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused direct capability source entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedDirectCapabilitySourceAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "load_config"
                && violation["surface"] == "direct_capability_source_allowlist"
                && violation["source"] == "legacy_reader"
        }),
        "expected unused direct capability source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_propagated_effect_source_allowlist_rejects_unapproved_callee() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_propagated_source_reject_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_source_reject",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config", "load_secret"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn load_secret() ~ FileSystem {
    read_file("secret.txt");
}

fn main() ~ FileSystem {
    load_secret();
}
"#,
    )
    .expect("write propagated source allowlist rejection fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with propagated source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject unapproved propagated effect source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectSourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_secret"
        }),
        "expected propagated effect source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_propagated_effect_source_allowlist_accepts_approved_callee() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_propagated_source_accept_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_source_accept",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated source allowlist acceptance fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with approved propagated source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept approved propagated effect source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_propagated_effect_source_allowlist_accepts_approved_method() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_propagated_method_source_accept_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_method_source_accept",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["Config.load"]
            }
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
struct Config;

impl Config {
    fn load(self) ~ FileSystem {
        read_file("ops.txt");
    }
}

fn main() ~ FileSystem {
    let config = Config;
    config.load();
}
"#,
    )
    .expect("write propagated method source allowlist acceptance fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with approved propagated method source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept approved propagated method source\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(
        receipt["propagated_effects"]["main"]["FileSystem"],
        serde_json::json!(["Config.load"])
    );
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_strict_allowlist_coverage_rejects_unused_propagated_effect_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_unused_propagated_source_allowlist_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "unused_propagated_source_allowlist",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config", "legacy_loader"]
            }
          },
          "require_allowlist_coverage": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write unused propagated source allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with strict propagated source coverage");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "strict policy should reject unused propagated effect source entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "UnusedPropagatedEffectSourceAllowlist"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effect_source_allowlist"
                && violation["source"] == "legacy_loader"
        }),
        "expected unused propagated effect source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_require_source_allowlists_rejects_missing_direct_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_require_direct_source_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_direct_source",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "require_source_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}
"#,
    )
    .expect("write required direct source allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with required direct source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require direct capability source allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DirectCapabilitySourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "load_config"
                && violation["surface"] == "observed_capabilities"
                && violation["source"] == "read_file"
        }),
        "expected required direct source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_require_source_allowlists_rejects_missing_propagated_source_entry() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_require_propagated_source_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_propagated_source",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "require_source_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required propagated source allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with required propagated source allowlist");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should require propagated effect source allowlist entry\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectSourceNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_config"
        }),
        "expected required propagated source allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_require_source_allowlists_accepts_explicit_source_entries() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_require_sources_accept_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "require_sources_accept",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "direct_capability_source_allowlist": {
            "FileSystem": {
              "load_config": ["read_file"]
            }
          },
          "propagated_effect_allowlist": {
            "FileSystem": ["main"]
          },
          "propagated_effect_source_allowlist": {
            "FileSystem": {
              "main": ["load_config"]
            }
          },
          "require_source_allowlists": true
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write required source allowlists acceptance fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with required explicit source allowlists");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        output.status.success(),
        "policy should accept explicit source allowlist entries\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "passed");
    assert!(receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());
}

#[test]
fn check_policy_propagated_allowlist_rejects_unlisted_caller() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_propagated_reject_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "propagated_reject",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["FileSystem"],
          "direct_effect_allowlist": {
            "FileSystem": ["load_config"]
          },
          "propagated_effect_allowlist": {
            "FileSystem": []
          }
        }"#,
    );
    fs::write(
        &fixture,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write propagated allowlist fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with propagated allowlist policy");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy should reject propagated caller\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "PropagatedEffectNotAllowed"
                && violation["effect"] == "FileSystem"
                && violation["function"] == "main"
                && violation["surface"] == "propagated_effects"
                && violation["source"] == "load_config"
        }),
        "expected propagated effect allowlist violation in {violations:#?}"
    );
}

#[test]
fn check_policy_rejects_unsupported_schema() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_policy_bad_schema_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "bad_schema",
        r#"{
          "schema": "quantalang-check-policy/v0",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write bad schema fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .output()
        .expect("run quantac check with bad policy schema");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "unsupported policy schema should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Unsupported check policy schema 'quantalang-check-policy/v0'"),
        "stderr should report unsupported schema:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn policy_list_includes_builtin_security_profiles() {
    let output = quantac()
        .args(["policy", "list"])
        .output()
        .expect("run quantac policy list");

    assert!(
        output.status.success(),
        "policy list should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pure"), "missing pure profile:\n{stdout}");
    assert!(
        stdout.contains("console-only"),
        "missing console-only profile:\n{stdout}"
    );
    assert!(
        stdout.contains("offline"),
        "missing offline profile:\n{stdout}"
    );
    assert!(
        stdout.contains("ci-review"),
        "missing ci-review profile:\n{stdout}"
    );
    assert!(
        stdout.contains("strict-accountability"),
        "missing strict-accountability profile:\n{stdout}"
    );
}

#[test]
fn policy_list_json_emits_catalog_with_profile_digests() {
    let output = quantac()
        .args(["policy", "list", "--json"])
        .output()
        .expect("run quantac policy list --json");

    assert!(
        output.status.success(),
        "policy list --json should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("catalog should be JSON");
    assert_eq!(catalog["schema"], "quantalang-policy-catalog/v1");
    let profiles = catalog["profiles"]
        .as_array()
        .expect("profiles should be an array");
    let names = profiles
        .iter()
        .map(|profile| profile["name"].as_str().unwrap_or(""))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "pure",
            "console-only",
            "offline",
            "ci-review",
            "strict-accountability"
        ]
    );
    for profile in profiles {
        assert_eq!(
            profile["policy_schema"], "quantalang-check-policy/v1",
            "profile should name the policy schema: {profile:#?}"
        );
        assert!(
            profile["summary"]
                .as_str()
                .is_some_and(|summary| !summary.is_empty()),
            "profile should include a summary: {profile:#?}"
        );
        assert_eq!(profile["digest"]["algorithm"], "sha256");
        assert_eq!(
            profile["digest"]["hex"]
                .as_str()
                .expect("profile digest hex")
                .len(),
            64
        );
    }
}

#[test]
fn policy_list_json_digest_matches_printed_profile() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_policy_catalog_digest_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create policy catalog digest directory");
    let profile_path = dir.join("ci-review.json");

    let catalog_output = quantac()
        .args(["policy", "list", "--json"])
        .output()
        .expect("run quantac policy list --json");
    let print_output = quantac()
        .args(["policy", "print", "ci-review", "--output"])
        .arg(&profile_path)
        .output()
        .expect("write ci-review policy");

    assert!(
        catalog_output.status.success(),
        "policy catalog should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&catalog_output.stdout),
        String::from_utf8_lossy(&catalog_output.stderr)
    );
    assert!(
        print_output.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&print_output.stdout),
        String::from_utf8_lossy(&print_output.stderr)
    );

    let catalog: serde_json::Value =
        serde_json::from_slice(&catalog_output.stdout).expect("catalog should be JSON");
    let ci_review_digest = catalog["profiles"]
        .as_array()
        .expect("profiles should be an array")
        .iter()
        .find(|profile| profile["name"] == "ci-review")
        .expect("ci-review profile")["digest"]
        .clone();
    let printed_text = fs::read(&profile_path).expect("read printed policy");
    let expected_hex = sha256_hex(&printed_text);

    assert_eq!(ci_review_digest["algorithm"], "sha256");
    assert_eq!(ci_review_digest["hex"], expected_hex);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_print_emits_valid_pure_profile() {
    let output = quantac()
        .args(["policy", "print", "pure"])
        .output()
        .expect("run quantac policy print pure");

    assert!(
        output.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let profile: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("policy profile should be JSON");
    assert_eq!(profile["schema"], "quantalang-check-policy/v1");
    assert_eq!(profile["require_source_digest"], true);
    assert_eq!(profile["require_input_graph_digest"], true);
    let denied = profile["denied_effects"]
        .as_array()
        .expect("denied_effects should be an array");
    assert!(
        denied.iter().any(|effect| effect == "Network"),
        "pure profile should deny Network: {denied:#?}"
    );
    assert!(
        denied.iter().any(|effect| effect == "Foreign"),
        "pure profile should deny Foreign: {denied:#?}"
    );
}

#[test]
fn policy_print_emits_strict_accountability_profile_with_source_gates() {
    let output = quantac()
        .args(["policy", "print", "strict-accountability"])
        .output()
        .expect("run quantac policy print strict-accountability");

    assert!(
        output.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let profile: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("policy profile should be JSON");
    assert_eq!(profile["schema"], "quantalang-check-policy/v1");
    assert_eq!(profile["require_source_digest"], true);
    assert_eq!(profile["require_input_graph_digest"], true);
    assert_eq!(profile["require_effect_allowlist"], true);
    assert_eq!(profile["require_provenance_allowlists"], true);
    assert_eq!(profile["require_source_allowlists"], true);
    assert_eq!(profile["require_allowlist_coverage"], true);
    let denied = profile["denied_effects"]
        .as_array()
        .expect("denied_effects should be an array");
    for effect in ["Network", "Process", "Foreign", "Gpu"] {
        assert!(
            denied.iter().any(|denied| denied == effect),
            "strict-accountability profile should deny {effect}: {denied:#?}"
        );
    }
}

#[test]
fn policy_scaffold_from_receipt_emits_exact_source_allowlists() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_policy_scaffold_receipt_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create policy scaffold fixture directory");
    let input = dir.join("app.quanta");
    let receipt = dir.join("receipt.json");
    let policy = dir.join("policy.json");
    fs::write(
        &input,
        r#"
fn load_config() ~ FileSystem {
    read_file("ops.txt");
}

fn main() ~ FileSystem {
    load_config();
}
"#,
    )
    .expect("write policy scaffold input");

    let check = quantac()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write policy scaffold receipt");
    assert!(
        check.status.success(),
        "check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = quantac()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .arg("--output")
        .arg(&policy)
        .output()
        .expect("scaffold policy from receipt");
    assert!(
        scaffold.status.success(),
        "policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );

    let scaffolded: serde_json::Value =
        serde_json::from_slice(&fs::read(&policy).expect("read scaffolded policy"))
            .expect("scaffolded policy should be JSON");
    assert_eq!(scaffolded["schema"], "quantalang-check-policy/v1");
    assert_eq!(
        scaffolded["allowed_effects"],
        serde_json::json!(["FileSystem"])
    );
    assert_eq!(
        scaffolded["direct_effect_allowlist"]["FileSystem"],
        serde_json::json!(["load_config"])
    );
    assert_eq!(
        scaffolded["direct_capability_source_allowlist"]["FileSystem"]["load_config"],
        serde_json::json!(["read_file"])
    );
    assert_eq!(
        scaffolded["propagated_effect_allowlist"]["FileSystem"],
        serde_json::json!(["main"])
    );
    assert_eq!(
        scaffolded["propagated_effect_source_allowlist"]["FileSystem"]["main"],
        serde_json::json!(["load_config"])
    );
    assert_eq!(scaffolded["require_source_digest"], true);
    assert_eq!(scaffolded["require_input_graph_digest"], true);
    assert_eq!(scaffolded["require_effect_allowlist"], true);
    assert_eq!(scaffolded["require_provenance_allowlists"], true);
    assert_eq!(scaffolded["require_source_allowlists"], true);
    assert_eq!(scaffolded["require_allowlist_coverage"], true);

    let verify_scaffold = quantac()
        .arg("check")
        .arg(&input)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("check source with scaffolded policy");
    assert!(
        verify_scaffold.status.success(),
        "scaffolded policy should accept the exact receipt evidence\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_scaffold.stdout),
        String::from_utf8_lossy(&verify_scaffold.stderr)
    );
    let verified_receipt = receipt_from_stdout(&verify_scaffold);
    assert_eq!(verified_receipt["policy"]["status"], "passed");
    assert!(verified_receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations")
        .is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_scaffold_from_pure_receipt_rejects_later_effect_drift() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_policy_scaffold_pure_drift_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create pure scaffold drift fixture directory");
    let input = dir.join("app.quanta");
    let receipt = dir.join("receipt.json");
    let policy = dir.join("policy.json");
    fs::write(&input, "fn main() {}\n").expect("write pure policy scaffold input");

    let check = quantac()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write pure policy scaffold receipt");
    assert!(
        check.status.success(),
        "pure check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = quantac()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .arg("--output")
        .arg(&policy)
        .output()
        .expect("scaffold policy from pure receipt");
    assert!(
        scaffold.status.success(),
        "pure policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let scaffolded: serde_json::Value =
        serde_json::from_slice(&fs::read(&policy).expect("read pure scaffolded policy"))
            .expect("pure scaffolded policy should be JSON");
    assert_eq!(scaffolded["allowed_effects"], serde_json::json!([]));
    assert_eq!(scaffolded["require_effect_allowlist"], true);

    fs::write(
        &input,
        r#"
effect Audit {
    fn record();
}

fn main() ~ Audit {}
"#,
    )
    .expect("write drifted effect input");
    let drift = quantac()
        .arg("check")
        .arg(&input)
        .arg("--policy")
        .arg(&policy)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("check drifted source with pure scaffolded policy");
    assert!(
        !drift.status.success(),
        "pure scaffolded policy should reject later declared effect drift\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&drift.stdout),
        String::from_utf8_lossy(&drift.stderr)
    );
    let receipt = receipt_from_stdout(&drift);
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "Audit"
                && violation["function"] == "main"
        }),
        "expected declared effect drift violation in {violations:#?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_scaffold_from_foreign_receipt_preserves_direct_ffi_boundary() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_policy_scaffold_foreign_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create foreign scaffold fixture directory");
    let input = dir.join("app.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(
        &input,
        r#"
extern "C" { fn touch(); }

fn main() ~ Foreign {
    touch();
}
"#,
    )
    .expect("write foreign policy scaffold input");

    let check = quantac()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write foreign policy scaffold receipt");
    assert!(
        check.status.success(),
        "foreign check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = quantac()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .output()
        .expect("scaffold policy from foreign receipt");
    assert!(
        scaffold.status.success(),
        "foreign policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let scaffolded: serde_json::Value =
        serde_json::from_slice(&scaffold.stdout).expect("foreign scaffold should be JSON");
    assert_eq!(
        scaffolded["allowed_effects"],
        serde_json::json!(["Foreign"])
    );
    assert_eq!(
        scaffolded["direct_effect_allowlist"]["Foreign"],
        serde_json::json!(["main"])
    );
    assert_eq!(
        scaffolded["direct_capability_source_allowlist"]["Foreign"]["main"],
        serde_json::json!(["touch"])
    );
    assert!(
        scaffolded["propagated_effect_allowlist"]
            .as_object()
            .expect("propagated effect allowlist")
            .is_empty(),
        "direct FFI boundary should not be scaffolded as propagated: {scaffolded:#?}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn policy_scaffold_from_qualified_capability_receipt_preserves_source_path() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_policy_scaffold_qualified_capability_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create qualified scaffold fixture directory");
    let input = dir.join("app.quanta");
    let receipt = dir.join("receipt.json");
    fs::write(
        &input,
        r#"fn main() ~ FileSystem { io::read_file("ops.txt"); }"#,
    )
    .expect("write qualified policy scaffold input");

    let check = quantac()
        .arg("check")
        .arg(&input)
        .arg("--receipt")
        .arg(&receipt)
        .output()
        .expect("write qualified policy scaffold receipt");
    assert!(
        check.status.success(),
        "qualified check should produce a receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let scaffold = quantac()
        .arg("policy")
        .arg("scaffold")
        .arg(&receipt)
        .output()
        .expect("scaffold policy from qualified receipt");
    assert!(
        scaffold.status.success(),
        "qualified policy scaffold should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&scaffold.stdout),
        String::from_utf8_lossy(&scaffold.stderr)
    );
    let scaffolded: serde_json::Value =
        serde_json::from_slice(&scaffold.stdout).expect("qualified scaffold should be JSON");
    assert_eq!(
        scaffolded["direct_effect_allowlist"]["FileSystem"],
        serde_json::json!(["main"])
    );
    assert_eq!(
        scaffolded["direct_capability_source_allowlist"]["FileSystem"]["main"],
        serde_json::json!(["io::read_file"])
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn printed_pure_policy_rejects_console_program() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_printed_pure_policy_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create pure policy fixture directory");
    let policy_path = dir.join("pure-policy.json");
    let fixture = dir.join("console.quanta");

    let print = quantac()
        .args(["policy", "print", "pure", "--output"])
        .arg(&policy_path)
        .output()
        .expect("write pure policy");
    assert!(
        print.status.success(),
        "policy print --output should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&print.stdout),
        String::from_utf8_lossy(&print.stderr)
    );

    fs::write(&fixture, r#"fn main() ~ Console { println!("blocked"); }"#)
        .expect("write console fixture");
    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with pure policy");

    assert!(
        !output.status.success(),
        "pure policy should reject console program\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "Console"
                && violation["function"] == "main"
        }),
        "expected Console denial in {violations:#?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_profile_strict_accountability_rejects_ambient_console_without_allowlists() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_profile_strict_accountability_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Console { println!("blocked"); }"#)
        .expect("write console fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("strict-accountability")
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with strict-accountability profile");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "strict-accountability profile should reject console without allowlists\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["source"], "builtin:strict-accountability");
    assert_eq!(receipt["policy"]["profile"], "strict-accountability");
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DisallowedEffect"
                && violation["effect"] == "Console"
                && violation["function"] == "main"
        }),
        "expected strict Console effect allowlist denial in {violations:#?}"
    );
}

#[test]
fn check_profile_pure_rejects_console_program_without_policy_file() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_profile_pure_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() ~ Console { println!("blocked"); }"#)
        .expect("write console fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run quantac check with pure profile");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "pure profile should reject console program\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["source"], "builtin:pure");
    assert_eq!(receipt["policy"]["profile"], "pure");
    assert_eq!(receipt["policy"]["profile_digest"]["algorithm"], "sha256");
    assert_eq!(
        receipt["policy"]["profile_digest"]["hex"]
            .as_str()
            .expect("profile digest")
            .len(),
        64
    );
    assert_eq!(receipt["policy"]["status"], "failed");
    let violations = receipt["policy"]["violations"]
        .as_array()
        .expect("policy violations");
    assert!(
        violations.iter().any(|violation| {
            violation["kind"] == "DeniedEffect"
                && violation["effect"] == "Console"
                && violation["function"] == "main"
        }),
        "expected Console denial in {violations:#?}"
    );
}

#[test]
fn check_profile_receipt_digest_matches_printed_builtin_profile() {
    let dir =
        std::env::temp_dir().join(format!("quantalang_profile_digest_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create profile digest fixture directory");
    let profile_path = dir.join("pure.json");
    let input = dir.join("pure.quanta");

    let print = quantac()
        .args(["policy", "print", "pure", "--output"])
        .arg(&profile_path)
        .output()
        .expect("write pure profile");
    assert!(
        print.status.success(),
        "policy print should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&print.stdout),
        String::from_utf8_lossy(&print.stderr)
    );
    fs::write(&input, r#"fn main() {}"#).expect("write pure input");

    let via_profile = quantac()
        .arg("check")
        .arg(&input)
        .arg("--profile")
        .arg("pure")
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run check with built-in profile");
    let via_policy = quantac()
        .arg("check")
        .arg(&input)
        .arg("--policy")
        .arg(&profile_path)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run check with printed profile");

    assert!(
        via_profile.status.success(),
        "built-in profile check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&via_profile.stdout),
        String::from_utf8_lossy(&via_profile.stderr)
    );
    assert!(
        via_policy.status.success(),
        "printed policy check should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&via_policy.stdout),
        String::from_utf8_lossy(&via_policy.stderr)
    );

    let profile_receipt = receipt_from_stdout(&via_profile);
    let policy_receipt = receipt_from_stdout(&via_policy);
    assert_eq!(profile_receipt["policy"]["profile"], "pure");
    assert_eq!(
        profile_receipt["policy"]["profile_digest"],
        policy_receipt["policy"]["source_digest"]
    );
    assert_eq!(policy_receipt["policy"]["profile"], serde_json::Value::Null);
    assert_eq!(
        policy_receipt["policy"]["profile_digest"],
        serde_json::Value::Null
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_profile_expect_digest_accepts_matching_builtin_digest() {
    let dir = std::env::temp_dir().join(format!(
        "quantalang_profile_expect_digest_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create profile digest fixture directory");
    let input = dir.join("pure.quanta");
    fs::write(&input, r#"fn main() {}"#).expect("write pure input");

    let catalog_output = quantac()
        .args(["policy", "list", "--json"])
        .output()
        .expect("run quantac policy list --json");
    assert!(
        catalog_output.status.success(),
        "policy catalog should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&catalog_output.stdout),
        String::from_utf8_lossy(&catalog_output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&catalog_output.stdout).expect("catalog should be JSON");
    let pure_digest = catalog["profiles"]
        .as_array()
        .expect("profiles should be an array")
        .iter()
        .find(|profile| profile["name"] == "pure")
        .expect("pure profile")["digest"]["hex"]
        .as_str()
        .expect("pure digest")
        .to_string();

    let output = quantac()
        .arg("check")
        .arg(&input)
        .arg("--profile")
        .arg("pure")
        .arg("--expect-profile-digest")
        .arg(&pure_digest)
        .arg("--receipt")
        .arg("-")
        .output()
        .expect("run check with matching profile digest");

    assert!(
        output.status.success(),
        "matching profile digest should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = receipt_from_stdout(&output);
    assert_eq!(receipt["policy"]["profile"], "pure");
    assert_eq!(receipt["policy"]["profile_digest"]["hex"], pure_digest);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_profile_expect_digest_rejects_mismatch() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_profile_digest_mismatch_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() {}"#).expect("write pure fixture");
    let wrong_digest = "0".repeat(64);

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("pure")
        .arg("--expect-profile-digest")
        .arg(&wrong_digest)
        .output()
        .expect("run check with mismatched profile digest");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "mismatched profile digest should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Built-in policy profile digest mismatch"),
        "stderr should report digest mismatch:\n{stderr}"
    );
    assert!(
        stderr.contains("pure"),
        "stderr should name the profile:\n{stderr}"
    );
}

#[test]
fn check_expect_profile_digest_requires_builtin_profile() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_profile_digest_without_profile_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() {}"#).expect("write pure fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--expect-profile-digest")
        .arg("0".repeat(64))
        .output()
        .expect("run check with profile digest but no profile");

    let _ = fs::remove_file(&fixture);

    assert!(
        !output.status.success(),
        "profile digest pin without profile should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires --profile"),
        "stderr should report missing --profile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_profile_rejects_unknown_builtin_profile() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_profile_unknown_{}.quanta",
        std::process::id()
    ));
    fs::write(&fixture, r#"fn main() {}"#).expect("write pure fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--profile")
        .arg("missing")
        .output()
        .expect("run quantac check with missing profile");

    let _ = fs::remove_file(&fixture);

    assert!(!output.status.success(), "unknown profile should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Unknown built-in policy profile 'missing'"),
        "stderr should name the unknown profile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_rejects_policy_file_and_builtin_profile_together() {
    let fixture = std::env::temp_dir().join(format!(
        "quantalang_check_profile_conflict_{}.quanta",
        std::process::id()
    ));
    let policy = write_temp_policy(
        "profile_conflict",
        r#"{
          "schema": "quantalang-check-policy/v1",
          "allowed_effects": ["Console"]
        }"#,
    );
    fs::write(&fixture, r#"fn main() ~ Console { println!("ok"); }"#)
        .expect("write conflict fixture");

    let output = quantac()
        .arg("check")
        .arg(&fixture)
        .arg("--policy")
        .arg(&policy)
        .arg("--profile")
        .arg("pure")
        .output()
        .expect("run quantac check with conflicting policy inputs");

    let _ = fs::remove_file(&fixture);
    let _ = fs::remove_file(&policy);

    assert!(
        !output.status.success(),
        "policy file and profile should conflict"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
        "stderr should report the argument conflict:\n{}",
        String::from_utf8_lossy(&output.stderr)
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
fn corpus_verify_checks_substrate_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping substrate receipt verification because no C backend is available");
        return;
    }

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run quantac corpus verify with substrate receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept substrate receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("substrate receipt: ok"),
        "corpus verify should report substrate receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_checks_mir_representation_receipt() {
    if !c_backend_ready() {
        eprintln!(
            "skipping MIR representation receipt verification because no C backend is available"
        );
        return;
    }

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run quantac corpus verify with MIR representation receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept MIR representation receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("mir representation receipt: ok"),
        "corpus verify should report MIR representation receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_checks_memory_layout_receipt() {
    if !c_backend_ready() {
        eprintln!("skipping memory layout receipt verification because no C backend is available");
        return;
    }

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(repo_root().join("semantic-corpus"))
        .output()
        .expect("run quantac corpus verify with memory layout receipt");

    assert!(
        output.status.success(),
        "corpus verify should accept memory layout receipt\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(
        stdout.contains("memory layout receipt: ok"),
        "corpus verify should report memory layout receipt status:\n{}",
        stdout
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_schema_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_schema");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("quantalang-memory-layout-receipt/v9".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against bad memory layout schema");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "schema drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout receipt has unsupported schema"),
        "stderr should name memory layout schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_program_count_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_program_count");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout program count drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "program count drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout source_set.program_count mismatch"),
        "stderr should name memory layout program count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_path_escape() {
    let corpus_root = temp_semantic_corpus("memory_layout_path_escape");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.quanta".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "path escape should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program.path must stay within corpus root"),
        "stderr should name memory layout path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_source_digest");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout source digest drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "source digest drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program scalar_branch source_digest mismatch"),
        "stderr should name memory layout source digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_observed_surface_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_observed_surface");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][1]["observed_memory_surfaces"]["references"] =
            serde_json::Value::Bool(false);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout observed surface drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "observed memory surface drift should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("memory layout program references_mutation observed_memory_surfaces drift"),
        "stderr should name memory layout observed surface drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_known_gap_drift() {
    let corpus_root = temp_semantic_corpus("memory_layout_known_gap");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["summary"]["known_gaps"] = serde_json::json!(["none"]);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout known gap drift");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(!output.status.success(), "known gap drift should fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory layout summary drift"),
        "stderr should name memory layout summary drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_memory_layout_byte_layout_overclaim() {
    let corpus_root = temp_semantic_corpus("memory_layout_overclaim");
    write_memory_layout_receipt_copy(&corpus_root, |mut receipt| {
        receipt["memory_model"]["layout_claim"] =
            serde_json::Value::String("byte-offset ABI layout verified".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against memory layout overclaim");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "byte layout overclaim should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory layout memory_model drift"),
        "stderr should name memory layout overclaim:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_receipt_schema_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_schema");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] =
            serde_json::Value::String("quantalang-mir-representation-receipt/v9".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against bad MIR representation schema");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation schema drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation receipt has unsupported schema"),
        "stderr should name MIR representation schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_program_count_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_program_count");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation program count drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation program count drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation source_set.program_count mismatch"),
        "stderr should name MIR representation program count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_path_escape() {
    let corpus_root = temp_semantic_corpus("mir_repr_path_escape");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] = serde_json::Value::String("../outside.quanta".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name MIR representation path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_path_escape_absolute() {
    let corpus_root = temp_semantic_corpus("mir_repr_path_escape_absolute");
    let absolute_path = std::env::temp_dir().join("outside.quanta");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["path"] =
            serde_json::Value::String(absolute_path.to_string_lossy().into_owned());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation absolute path");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation absolute path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name MIR representation path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_source_digest_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_source_digest");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["source_digest"]["hex"] = serde_json::Value::String("0".repeat(64));
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation source digest drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation source digest drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch source_digest mismatch"),
        "stderr should name MIR representation source digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_operation_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_operation_drift");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["operations"]["rvalues"] = serde_json::json!(["ForgedRValue"]);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation operation drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation operation drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch operations.rvalues drift"),
        "stderr should name MIR representation operation drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_input_graph_digest_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_input_graph_digest");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["input_graph_digest"]["hex"] =
            serde_json::Value::String("f".repeat(64));
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation input graph digest drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation input graph digest drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch input_graph_digest mismatch"),
        "stderr should name MIR representation input graph digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_mir_representation_mir_digest_drift() {
    let corpus_root = temp_semantic_corpus("mir_repr_mir_digest");
    write_mir_representation_receipt_copy(&corpus_root, |mut receipt| {
        receipt["programs"][0]["mir_digest"]["hex"] = serde_json::Value::String("e".repeat(64));
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against MIR representation MIR digest drift");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject MIR representation MIR digest drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("mir representation program scalar_branch mir_digest mismatch"),
        "stderr should name MIR representation MIR digest drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_receipt_schema_drift() {
    let corpus_root = temp_semantic_corpus("substrate_schema");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["schema"] = serde_json::Value::String("quantalang-substrate-receipt/v9".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against bad substrate schema");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject bad substrate schema"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate receipt has unsupported schema"),
        "stderr should name schema drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_program_count_drift() {
    let corpus_root = temp_semantic_corpus("substrate_program_count");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["source_set"]["program_count"] = serde_json::Value::from(7);
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against bad substrate program count");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate program count drift"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate source_set.program_count mismatch"),
        "stderr should name program-count drift:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_production_substrate_backend_without_receipt() {
    let corpus_root = temp_semantic_corpus("substrate_missing_receipt");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        if let Some(c) = receipt["execution_surface"]["c"].as_object_mut() {
            c.remove("receipt");
        }
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against missing production receipt");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject production backend without receipt"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate execution_surface.c is production-anchor but receipt is missing"),
        "stderr should name missing production receipt:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_rust_subset_without_receipt() {
    let corpus_root = temp_semantic_corpus("substrate_rust_subset_missing_receipt");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        if let Some(rust) = receipt["execution_surface"]["rust"].as_object_mut() {
            rust.remove("receipt");
        }
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against missing rust subset receipt");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject rust subset without receipt evidence"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "substrate execution_surface.rust experimental-subset requires receipt evidence"
        ),
        "stderr should name missing rust subset receipt:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_missing_required_spirv_target() {
    let corpus_root = temp_semantic_corpus("substrate_missing_spirv");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        if let Some(execution_surface) = receipt["execution_surface"].as_object_mut() {
            execution_surface.remove("spirv");
        }
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against missing spirv target");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject missing required spirv target"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate execution_surface missing required target spirv"),
        "stderr should name missing required spirv target:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["execution_surface"]["c"]["receipt"] =
            serde_json::Value::String("../outside.json".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against substrate path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate receipt path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_repr_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String("../outside.json".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against substrate representation receipt path escape");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt path escape"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_memory_receipt_path_escape() {
    let corpus_root = temp_semantic_corpus("substrate_memory_path_escape");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["memory_surface"]["memory_receipt"] =
            serde_json::Value::String("../memory-layout-2026-06-18.json".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against substrate memory receipt path escape");
    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "substrate memory receipt path escape should fail"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate memory_surface.memory_receipt must stay within corpus root"),
        "stderr should name substrate memory receipt path containment:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_root_qualified_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_path_root");
    let rooted = if cfg!(windows) {
        "\\receipts\\mir-representation-2026-06-18.json"
    } else {
        "/receipts/mir-representation-2026-06-18.json"
    };
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String(rooted.into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect(
            "run quantac corpus verify against substrate representation receipt root-qualified path",
        );

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt root-qualified path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_absolute_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_path_absolute");
    let absolute_path = std::env::temp_dir().join("outside.json");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String(absolute_path.to_string_lossy().into_owned());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against substrate representation receipt absolute path");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt absolute path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_windows_drive_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_windows_drive_path");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String("C:\\outside.json".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect(
            "run quantac corpus verify against substrate representation receipt windows drive path",
        );

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt windows drive path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_substrate_representation_receipt_windows_drive_relative_path() {
    let corpus_root = temp_semantic_corpus("substrate_repr_windows_drive_relative_path");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["representation_surface"]["representation_receipt"] =
            serde_json::Value::String("C:outside.json".into());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect(
            "run quantac corpus verify against substrate representation receipt windows drive-relative path",
        );

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject substrate representation receipt windows drive-relative path"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must stay within corpus root"),
        "stderr should name substrate representation receipt path containment failure:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corpus_verify_rejects_empty_substrate_evidence_commands() {
    let corpus_root = temp_semantic_corpus("substrate_empty_commands");
    write_substrate_receipt_copy(&corpus_root, |mut receipt| {
        receipt["evidence_surface"]["commands"] = serde_json::Value::Array(Vec::new());
        receipt
    });

    let output = quantac()
        .arg("corpus")
        .arg("verify")
        .arg("--root")
        .arg(&corpus_root)
        .output()
        .expect("run quantac corpus verify against empty substrate commands");

    let _ = fs::remove_dir_all(&corpus_root);

    assert!(
        !output.status.success(),
        "corpus verify should reject empty evidence commands"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("substrate evidence_surface.commands must not be empty"),
        "stderr should name empty evidence commands:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
