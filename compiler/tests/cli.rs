use std::{fs, path::PathBuf, process::Command};

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
