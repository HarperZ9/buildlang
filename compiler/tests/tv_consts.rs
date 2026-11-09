//! Translation validation with named constants (preregistered in
//! docs/TRANSLATION-VALIDATION.md, "Widening 2: named constants", before the
//! run). Every function of `tests/tv/consts.bld` is certified; a changed
//! constant initializer in the C is refused; a user `PI` is certified against
//! its own value (the backend used the built-in value before issue #177 was fixed).

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/tv")
        .join(name)
}

fn status_of(doc: &Value, name: &str) -> String {
    doc["functions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("{name} missing: {doc}"))["status"]
        .as_str()
        .unwrap()
        .to_string()
}

fn tv(file: &Path, out: Option<&Path>) -> Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.args(["tv", "--json"]).arg(file);
    if let Some(o) = out {
        cmd.arg("--out").arg(o);
    }
    cmd.env_remove("BUILDLANG_PLANT_MISCOMPILE");
    let o = cmd.output().expect("buildc runs");
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&o.stderr)))
}

const USERS: [&str; 4] = ["scaled", "halve", "circle", "gate"];

#[test]
fn functions_reading_constants_are_certified() {
    let doc = tv(&fixture("consts.bld"), None);
    for name in USERS {
        assert_eq!(status_of(&doc, name), "certified", "{doc}");
    }
}

#[test]
fn a_changed_constant_initializer_is_refused() {
    let dir = std::env::temp_dir().join(format!("bl-tv-consts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = fixture("consts.bld");
    tv(&file, Some(&dir));
    let c_path = dir.join("consts.c");
    let c0 = std::fs::read_to_string(&c_path).unwrap();
    for (name, from, to) in [
        (
            "scaled",
            "const int64_t SCALE = 1000;",
            "const int64_t SCALE = 1001;",
        ),
        (
            "halve",
            "const double HALF = 0.5;",
            "const double HALF = 0.25;",
        ),
        (
            "gate",
            "const bool ENABLED = true;",
            "const bool ENABLED = false;",
        ),
    ] {
        assert!(c0.contains(from), "`{from}` not in the C");
        std::fs::write(&c_path, c0.replace(from, to)).unwrap();
        let o = Command::new(env!("CARGO_BIN_EXE_buildc"))
            .args(["tv-check", "--json"])
            .arg(&file)
            .arg(&c_path)
            .arg(dir.join("consts.tv-cert.json"))
            .output()
            .expect("buildc runs");
        let doc: Value = serde_json::from_slice(&o.stdout).expect("JSON");
        assert_eq!(status_of(&doc, name), "not-certified", "{name}: {doc}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_user_pi_is_checked_against_its_own_value() {
    // Issue #177: the backend compiled a user `PI` to the built-in value,
    // and this function was not certified. Fixed: the user value is used.
    let doc = tv(&fixture("shadow_pi.bld"), None);
    assert_eq!(status_of(&doc, "area"), "certified", "{doc}");
}
