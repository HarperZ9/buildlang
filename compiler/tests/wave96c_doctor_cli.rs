// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Wave 96C integration tests for `buildc doctor [--json]`.

use std::process::{Command, Output};

fn run_buildc(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn buildc")
}

#[test]
fn doctor_json_reports_schema_targets_tools_and_live_self_test() {
    let out = run_buildc(&["doctor", "--json"], &[]);
    assert!(
        out.status.success(),
        "buildc doctor --json failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "buildc doctor --json must keep stderr empty on success: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let val: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse doctor JSON");
    assert_eq!(val["schema"], "buildlang.doctor/v1");
    assert_eq!(val["ok"], true);
    assert!(!val["compiler_version"].as_str().unwrap_or("").is_empty());
    assert!(!val["host_triple"].as_str().unwrap_or("").is_empty());

    let cpu = val["targets"]["cpu"].as_array().expect("cpu targets");
    assert!(cpu.iter().any(|v| v == "c"));
    let wasm = val["targets"]["wasm"].as_array().expect("wasm targets");
    assert!(wasm.iter().any(|v| v == "wasm"));
    let gpu = val["targets"]["gpu"].as_array().expect("gpu targets");
    for expected in ["hlsl", "glsl", "spirv", "wgsl", "msl"] {
        assert!(
            gpu.iter().any(|v| v == expected),
            "missing GPU target {expected}"
        );
    }

    let tools = val["tools"].as_array().expect("tools array");
    for expected_tool in [
        "cc",
        "clang",
        "cl",
        "dxc",
        "glslangValidator",
        "spirv-val",
        "naga",
    ] {
        let entry = tools
            .iter()
            .find(|t| t["name"] == expected_tool)
            .unwrap_or_else(|| panic!("missing tool probe {expected_tool}"));
        let status = entry["status"].as_str().expect("tool status");
        assert!(
            status == "present" || status == "absent",
            "unexpected status {status} for {expected_tool}"
        );
        if status == "present" {
            assert!(
                !entry["version"].as_str().unwrap_or("").is_empty(),
                "present tool {expected_tool} must report a version string"
            );
        } else {
            assert!(
                entry["version"].is_null(),
                "absent tool {expected_tool} must have null version"
            );
        }
    }

    let st = &val["self_test"];
    assert_eq!(st["ok"], true);
    assert_eq!(st["stages"]["lex"], true);
    assert_eq!(st["stages"]["parse"], true);
    assert_eq!(st["stages"]["typecheck"], true);
    assert_eq!(st["stages"]["mir_superopt"], true);
    assert_eq!(st["stages"]["verify_cert"], true);
    assert_eq!(st["stages"]["planted_invalid_cert_rejected"], true);
    assert!(st["rewrites_applied"].as_u64().unwrap_or(0) >= 1);
    let cert_sha = st["cert_sha256"].as_str().expect("cert_sha256");
    assert!(cert_sha.len() >= 64);
}

#[test]
fn doctor_human_mode_reports_toolchain_and_self_test() {
    let out = run_buildc(&["doctor"], &[]);
    assert!(
        out.status.success(),
        "buildc doctor failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "buildc doctor must keep stderr empty on success: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "BuildLang Doctor",
        "host:",
        "Optional tools:",
        "Backend maturity:",
        "Self-test:",
        "planted_invalid_cert_rejected=true",
        "Ready for practical C-backend examples:",
    ] {
        assert!(
            stdout.contains(needle),
            "human doctor output missing {needle:?}:\n{stdout}"
        );
    }
}

#[test]
fn doctor_planted_type_error_self_test_fails_closed() {
    let out = run_buildc(
        &["doctor", "--json"],
        &[(
            "BUILDC_DOCTOR_SELF_TEST_SRC",
            "fn broken() -> i64 { \"not_an_int\" }",
        )],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "planted type error must fail closed with exit 1; stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let val: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse failed doctor JSON");
    assert_eq!(val["schema"], "buildlang.doctor/v1");
    assert_eq!(val["ok"], false);
    assert_eq!(val["self_test"]["ok"], false);
    assert_eq!(val["self_test"]["stages"]["parse"], true);
    assert_eq!(val["self_test"]["stages"]["typecheck"], false);
    assert!(val["self_test"]["error"].as_str().is_some());
}

#[test]
fn doctor_planted_parse_error_self_test_fails_closed() {
    let out = run_buildc(
        &["doctor", "--json"],
        &[("BUILDC_DOCTOR_SELF_TEST_SRC", "fn broken( ->")],
    );
    assert_eq!(out.status.code(), Some(1));
    let val: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse failed doctor JSON");
    assert_eq!(val["ok"], false);
    assert_eq!(val["self_test"]["ok"], false);
    assert_eq!(val["self_test"]["stages"]["parse"], false);
}

#[test]
fn doctor_json_and_human_leak_zero_local_usernames() {
    let json_out = run_buildc(&["doctor", "--json"], &[]);
    assert!(json_out.status.success());
    let json_text = String::from_utf8_lossy(&json_out.stdout);

    let human_out = run_buildc(&["doctor"], &[]);
    assert!(human_out.status.success());
    let human_text = String::from_utf8_lossy(&human_out.stdout);

    let win_marker = ["c:", "\\", "users", "\\"].concat();
    let mac_marker = ["/", "users", "/"].concat();
    let linux_marker = ["/", "home", "/"].concat();

    for (label, text) in [("json", &json_text), ("human", &human_text)] {
        let lower = text.to_ascii_lowercase();
        assert!(
            !lower.contains(&win_marker)
                && !lower.contains(&mac_marker)
                && !lower.contains(&linux_marker),
            "{label} doctor output leaked a user home path:\n{text}"
        );
        for key in ["USERNAME", "USER"] {
            if let Ok(user) = std::env::var(key) {
                let u = user.trim().to_ascii_lowercase();
                if u.len() >= 3 {
                    // Check token-bounded occurrence so unrelated substrings cannot false-match.
                    for token in lower.split(|c: char| !c.is_ascii_alphanumeric()) {
                        assert_ne!(
                            token, u,
                            "{label} doctor output leaked local username token:\n{text}"
                        );
                    }
                }
            }
        }
    }
}
