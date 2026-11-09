//! Translation validation for the C backend: `buildc tv` and `buildc tv-check`.
//!
//! Expected results are written here before the runs they govern. Every
//! function of `tests/tv/subset.bld` is certified; with the planted
//! miscompilation (a checked `+` emitted as the subtract helper) every function
//! that adds becomes not certified; a forged certificate or tampered C is
//! refused; the checker's sources import nothing from the code generator.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture() -> PathBuf {
    repo().join("tests/tv/subset.bld")
}

fn statuses(doc: &Value) -> Vec<(String, String)> {
    doc["functions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().to_string(),
                f["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn tv(plant: bool, out: Option<&Path>) -> (Value, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.args(["tv", "--json"]).arg(fixture());
    if let Some(o) = out {
        cmd.arg("--out").arg(o);
    }
    if plant {
        cmd.env("BUILDLANG_PLANT_MISCOMPILE", "add-to-sub");
    } else {
        cmd.env_remove("BUILDLANG_PLANT_MISCOMPILE");
    }
    let o = cmd.output().expect("buildc runs");
    let doc = serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&o.stderr)));
    (doc, o.status.code().unwrap_or(-1))
}

const ADDS: [&str; 3] = ["sum_to", "clamp_add", "twice_max"];

#[test]
fn every_subset_function_is_certified() {
    let (doc, code) = tv(false, None);
    assert_eq!(code, 0, "{doc}");
    // `main` prints; since widening 3 (effects) it is certified too.
    for (name, status) in statuses(&doc) {
        assert_eq!(status, "certified", "{name}: {doc}");
    }
}

#[test]
fn the_planted_miscompilation_is_caught() {
    let (doc, code) = tv(true, None);
    assert_eq!(code, 1, "{doc}");
    for (name, status) in statuses(&doc) {
        let want = if ADDS.contains(&name.as_str()) {
            "not-certified"
        } else {
            "certified"
        };
        assert_eq!(status, want, "{name}: {doc}");
    }
}

fn tv_check(dir: &Path) -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .args(["tv-check", "--json"])
        .arg(fixture())
        .arg(dir.join("subset.c"))
        .arg(dir.join("subset.tv-cert.json"))
        .output()
        .expect("buildc runs");
    serde_json::from_slice(&o.stdout).expect("JSON")
}

fn status_of(doc: &Value, name: &str) -> String {
    statuses(doc)
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
}

#[test]
fn forged_certificates_and_tampered_c_are_refused() {
    let dir = std::env::temp_dir().join(format!("bl-tv-forge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (_, code) = tv(false, Some(&dir));
    assert_eq!(code, 0);
    let cert_path = dir.join("subset.tv-cert.json");
    let c_path = dir.join("subset.c");
    let cert0: Value = serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
    let c0 = std::fs::read_to_string(&c_path).unwrap();
    assert_eq!(status_of(&tv_check(&dir), "sum_to"), "certified");

    let entry = |cert: &mut Value, name: &str| -> usize {
        cert["functions"]
            .as_array()
            .unwrap()
            .iter()
            .position(|f| f["name"] == name)
            .unwrap()
    };
    // A wrong variable map: the certificate claims `s` lives in C's `i`.
    let mut cert = cert0.clone();
    let i = entry(&mut cert, "sum_to");
    cert["functions"][i]["vars"]["s"] = Value::String("i".into());
    std::fs::write(&cert_path, cert.to_string()).unwrap();
    assert_ne!(
        status_of(&tv_check(&dir), "sum_to"),
        "certified",
        "wrong variable map accepted"
    );

    // A wrong loop head.
    let mut cert = cert0.clone();
    let i = entry(&mut cert, "gcd");
    cert["functions"][i]["loop_heads"] = serde_json::json!(["bb3"]);
    std::fs::write(&cert_path, cert.to_string()).unwrap();
    assert_ne!(
        status_of(&tv_check(&dir), "gcd"),
        "certified",
        "wrong loop head accepted"
    );

    // Tampered C: an add turned into a subtract. The hash catches it; a forger
    // who also rewrites the hash is caught by the semantics.
    let text = super_function(&c0, "clamp_add");
    let bad = text.replace("bl_iadd_i64", "bl_isub_i64");
    assert_ne!(text, bad);
    let c1 = c0.replace(&text, &bad);
    std::fs::write(&c_path, &c1).unwrap();
    std::fs::write(&cert_path, cert0.to_string()).unwrap();
    assert_eq!(status_of(&tv_check(&dir), "clamp_add"), "not-certified");
    let mut cert = cert0.clone();
    let i = entry(&mut cert, "clamp_add");
    cert["functions"][i]["c_sha256"] = Value::String(sha256(&bad));
    std::fs::write(&cert_path, cert.to_string()).unwrap();
    let doc = tv_check(&dir);
    assert_eq!(status_of(&doc, "clamp_add"), "not-certified", "{doc}");
    let _ = std::fs::remove_dir_all(&dir);
}

fn super_function(c: &str, name: &str) -> String {
    let head = format!("static int64_t {name}(");
    let start = c.rfind(&head).expect("function in C");
    let end = c[start..].find("\n}").unwrap() + start + 2;
    c[start..end].to_string()
}

fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn the_checker_imports_nothing_from_the_code_generator() {
    let dir = repo().join("compiler/src/tv/check");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for banned in [
            "crate::codegen",
            "codegen::",
            "crate::tv::cert",
            "super::cert",
            "mir",
        ] {
            for line in text.lines().filter(|l| l.trim_start().starts_with("use ")) {
                assert!(!line.contains(banned), "{}: `{line}`", path.display());
            }
        }
        seen += 1;
    }
    assert!(seen >= 5, "checker sources went missing");
}
