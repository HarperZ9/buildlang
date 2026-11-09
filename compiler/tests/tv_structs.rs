//! Translation validation with struct values (preregistered in
//! docs/TRANSLATION-VALIDATION.md, "Widening 4: structs", before the run).
//! Every function of `tests/tv/structs.bld` is certified. Each planted C
//! rewrite swaps, misreads or miswrites a field, or changes the C struct
//! definition; each must be refused as not certified. With the planted
//! `add-to-sub` miscompilation every function that adds is refused.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/tv/structs.bld")
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

fn tv(out: Option<&Path>, plant: Option<&str>) -> Value {
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
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&o.stderr)))
}

/// The text of C function `name`, header through closing brace.
fn function_text(c: &str, name: &str) -> String {
    let needle = format!(" {name}(");
    let mut offset = 0;
    for line in c.split_inclusive('\n') {
        let t = line.trim_end();
        if !line.starts_with(' ') && t.contains(&needle) && t.ends_with(") {") {
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

const FNS: [&str; 8] = [
    "make", "sum", "diff", "scale", "span", "bump", "twice", "main",
];

/// Functions whose C contains a checked integer add.
const ADDS: [&str; 4] = ["sum", "span", "bump", "twice"];

#[test]
fn every_struct_function_is_certified() {
    let doc = tv(None, None);
    for name in FNS {
        assert_eq!(status_of(&doc, name), "certified", "{name}: {doc}");
    }
}

#[test]
fn the_planted_add_miscompilation_is_caught() {
    let doc = tv(None, Some("add-to-sub"));
    for name in FNS {
        let want = if ADDS.contains(&name) {
            "not-certified"
        } else {
            "certified"
        };
        assert_eq!(status_of(&doc, name), want, "{name}: {doc}");
    }
}

type Plant = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

/// (plant, function, edits). An edit whose text is outside the function
/// (the struct definition) applies to the whole file.
const PLANTS: [Plant; 6] = [
    (
        "S1 two fields swapped in a compound literal",
        "make",
        &[("(Point){ x, y }", "(Point){ y, x }")],
    ),
    (
        "S2 a member read of the wrong field",
        "diff",
        &[("_2 = p.x;", "_2 = p.y;")],
    ),
    (
        "S3 a member write to the wrong field",
        "bump",
        &[("q.x = _4;", "q.y = _4;")],
    ),
    (
        "S4 the C struct definition with two fields swapped",
        "make",
        &[(
            "struct Point {
    int32_t x;
    int32_t y;
};",
            "struct Point {
    int32_t y;
    int32_t x;
};",
        )],
    ),
    (
        "S5 a struct argument replaced by another",
        "twice",
        &[("_4 = diff(q);", "_4 = diff(p);")],
    ),
    (
        "S6 a field of a returned struct left as the parameter's",
        "scale",
        &[("(Color){ _4, _6, _8 }", "(Color){ _4, _5, _8 }")],
    ),
];

#[test]
fn planted_struct_rewrites_are_refused() {
    let dir = std::env::temp_dir().join(format!("bl-tv-structs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let doc = tv(Some(&dir), None);
    assert_eq!(status_of(&doc, "main"), "certified", "{doc}");
    let cert_path = dir.join("structs.tv-cert.json");
    let c_path = dir.join("structs.c");
    let cert0: Value = serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
    let c0 = std::fs::read_to_string(&c_path).unwrap().replace(
        "
", "
",
    );
    let mut accepted = Vec::new();
    for (plant, name, edits) in PLANTS {
        let text = function_text(&c0, name);
        let mut c = c0.clone();
        let mut bad = text.clone();
        for (from, to) in edits {
            if bad.contains(from) {
                bad = bad.replace(from, to);
            } else {
                assert!(c.contains(from), "{plant}: `{from}` not in the C");
                c = c.replace(from, to);
            }
        }
        std::fs::write(&c_path, c.replace(&text, &bad)).unwrap();
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
        let status = status_of(&doc, name);
        if status != "not-certified" {
            accepted.push(format!("{plant}: {status}: {doc}"));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        accepted.is_empty(),
        "{}",
        accepted.join(
            "
"
        )
    );
}
