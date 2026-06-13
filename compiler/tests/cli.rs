use std::process::Command;

fn quantac() -> Command {
    Command::new(env!("CARGO_BIN_EXE_quantac"))
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
