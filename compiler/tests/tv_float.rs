//! Translation validation of float functions (preregistered in
//! docs/TRANSLATION-VALIDATION.md, "Widening 1: floats", before the run).
//!
//! Every function of `tests/tv/floats.bld` is certified. With the planted
//! float miscompilation (a float `+` emitted as `-`) every function that adds
//! floats becomes not certified and the others stay certified. C rewrites that
//! look harmless but change IEEE results are refused, even with the hash
//! rewritten to match.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/tv/floats.bld")
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

fn tv(plant: Option<&str>, out: Option<&Path>) -> (Value, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.args(["tv", "--json"]).arg(fixture());
    if let Some(o) = out {
        cmd.arg("--out").arg(o);
    }
    match plant {
        Some(p) => cmd.env("BUILDLANG_PLANT_MISCOMPILE", p),
        None => cmd.env_remove("BUILDLANG_PLANT_MISCOMPILE"),
    };
    let o = cmd.output().expect("buildc runs");
    let doc = serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&o.stderr)));
    (doc, o.status.code().unwrap_or(-1))
}

const FLOAT_ADDS: [&str; 6] = ["fadd2", "poly2", "lerp", "sum_n", "hyp", "assoc"];

#[test]
fn every_float_function_is_certified() {
    let (doc, code) = tv(None, None);
    assert_eq!(code, 0, "{doc}");
    // `main` prints; since widening 3 (effects) it is certified too.
    let mut certified = 0;
    for (name, status) in statuses(&doc) {
        assert_eq!(status, "certified", "{name}: {doc}");
        certified += (status == "certified") as usize;
    }
    assert_eq!(certified, 18, "{doc}");
}

#[test]
fn the_planted_float_miscompilation_is_caught() {
    let (doc, code) = tv(Some("fadd-to-fsub"), None);
    assert_eq!(code, 1, "{doc}");
    for (name, status) in statuses(&doc) {
        let want = if FLOAT_ADDS.contains(&name.as_str()) {
            "not-certified"
        } else {
            "certified"
        };
        assert_eq!(status, want, "{name}: {doc}");
    }
}

fn function_text(c: &str, name: &str) -> String {
    let needle = format!(" {name}(");
    let mut offset = 0;
    for line in c.split_inclusive('\n') {
        let t = line.trim_end();
        if t.starts_with("static ") && t.contains(&needle) && t.ends_with(") {") {
            let end = c[offset..].find("\n}").unwrap() + offset + 2;
            return c[offset..end].to_string();
        }
        offset += line.len();
    }
    panic!("{name} not in C");
}

fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// C rewrites that are wrong under IEEE 754 (NaN, infinities, signed zero,
/// rounding), each applied to one function with its C hash rewritten.
const REFUSALS: [(&str, &[(&str, &str)]); 8] = [
    ("eq_self", &[("_2 = (x == x);", "_2 = true;")]),
    ("sub_self", &[("_2 = (x - x);", "_2 = 0.0;")]),
    ("mul_zero", &[("_2 = (x * 0.0);", "_2 = 0.0;")]),
    ("not_lt", &[("_4 = (!_3);", "_4 = (a >= b);")]),
    (
        "assoc",
        &[
            ("_4 = (a + b);", "_4 = (b + c);"),
            ("_5 = (_4 + c);", "_5 = (a + _4);"),
        ],
    ),
    ("neg_sub", &[("_4 = (-_3);", "_4 = (b - a);")]),
    ("round_trip", &[("_3 = ((double)_2);", "_3 = x;")]),
    ("tenth", &[("return 0.1;", "return 0.10000000000000002;")]),
];

#[test]
fn ieee_unsafe_rewrites_are_refused() {
    let dir = std::env::temp_dir().join(format!("bl-tv-float-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (_, code) = tv(None, Some(&dir));
    assert_eq!(code, 0);
    let cert_path = dir.join("floats.tv-cert.json");
    let c_path = dir.join("floats.c");
    let cert0: Value = serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
    let c0 = std::fs::read_to_string(&c_path).unwrap();
    for (name, edits) in REFUSALS {
        let text = function_text(&c0, name);
        let mut bad = text.clone();
        for (from, to) in edits {
            assert!(bad.contains(from), "{name}: `{from}` not in\n{text}");
            bad = bad.replace(from, to);
        }
        std::fs::write(&c_path, c0.replace(&text, &bad)).unwrap();
        let mut cert = cert0.clone();
        let i = cert["functions"]
            .as_array()
            .unwrap()
            .iter()
            .position(|f| f["name"] == name)
            .unwrap();
        cert["functions"][i]["c_sha256"] = Value::String(sha256(&bad));
        std::fs::write(&cert_path, cert.to_string()).unwrap();
        let o = Command::new(env!("CARGO_BIN_EXE_buildc"))
            .args(["tv-check", "--json"])
            .arg(fixture())
            .arg(&c_path)
            .arg(&cert_path)
            .output()
            .expect("buildc runs");
        let doc: Value = serde_json::from_slice(&o.stdout).expect("JSON");
        let status = statuses(&doc)
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap()
            .1;
        assert_eq!(
            status, "not-certified",
            "{name} accepted after rewrite: {doc}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_keeps_floats_outside_its_subset() {
    let dir = std::env::temp_dir().join(format!("bl-verify-float-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("fl.bld");
    std::fs::write(
        &src,
        "law self_eq(x: f64) { x == x }\nlaw sub_zero(x: f64) { x - x == 0.0 }\nfn main() {}\n",
    )
    .unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .args(["verify", "--json"])
        .arg(&src)
        .output()
        .expect("buildc runs");
    let doc: Value = serde_json::from_slice(&o.stdout).expect("JSON");
    assert_eq!(doc["summary"]["proved"], 0, "a float law was proved: {doc}");
    assert_eq!(doc["summary"]["unknown"], 2, "{doc}");
    let _ = std::fs::remove_dir_all(&dir);
}
