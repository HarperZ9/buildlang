//! Translation validation with effects (preregistered in
//! docs/TRANSLATION-VALIDATION.md, "Widening 3: effects", before the run).
//! Every function of `tests/tv/effects.bld` is certified. Each planted C
//! rewrite changes what is printed, in what order, on which stream, or what
//! is printed before a trap; each is fed to `tv-check` with the function's C
//! hash rewritten, and each must be refused as not certified.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/tv/effects.bld")
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

fn tv(out: Option<&Path>) -> Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.args(["tv", "--json"]).arg(fixture());
    if let Some(o) = out {
        cmd.arg("--out").arg(o);
    }
    cmd.env_remove("BUILDLANG_PLANT_MISCOMPILE");
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
    "sq",
    "inc",
    "note",
    "report",
    "calls",
    "loop_print",
    "floats",
    "main",
];

#[test]
fn every_effect_function_is_certified() {
    let doc = tv(None);
    for name in FNS {
        assert_eq!(status_of(&doc, name), "certified", "{name}: {doc}");
    }
}

type Plant = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

/// (plant, function, edits). An edit whose text is outside the function
/// (the string table) applies to the whole file.
const PLANTS: [Plant; 16] = [
    (
        "P1 two prints swapped",
        "report",
        &[(
            "_6 = __str2;\n    printf(_6, c);\n    _7 = __str3;\n    printf(_7, f ? \"true\" : \"false\");",
            "_7 = __str3;\n    printf(_7, f ? \"true\" : \"false\");\n    _6 = __str2;\n    printf(_6, c);",
        )],
    ),
    (
        "P2 a print moved after a checked add that can trap",
        "report",
        &[(
            "_3 = __str1;\n    printf(_3, a, b);\n    _4 = bl_iadd_i32(a, 1);",
            "_4 = bl_iadd_i32(a, 1);\n    _3 = __str1;\n    printf(_3, a, b);",
        )],
    ),
    ("P3 a print dropped", "report", &[("    printf(_6, c);\n", "")]),
    (
        "P3 a print duplicated",
        "report",
        &[("printf(_6, c);", "printf(_6, c);\n    printf(_6, c);")],
    ),
    (
        "P4 a literal changed in the string table",
        "report",
        &[("\"c=%d\\n\"", "\"C=%d\\n\"")],
    ),
    (
        "P5 %d turned into %u for a signed value",
        "note",
        &[("\"note %d\\n\"", "\"note %u\\n\"")],
    ),
    (
        "P6 stdout turned into stderr",
        "note",
        &[("printf(_1, x);", "fprintf(stderr, _1, x);")],
    ),
    (
        "P7 the bool rendering swapped",
        "report",
        &[("f ? \"true\" : \"false\"", "f ? \"false\" : \"true\"")],
    ),
    (
        "P8 two pure calls swapped",
        "calls",
        &[(
            "_2 = sq(x);\n    p = _2;\n    _4 = inc(x);",
            "_4 = inc(x);\n    _2 = sq(x);\n    p = _2;",
        )],
    ),
    (
        "P9 a call duplicated",
        "calls",
        &[("_4 = inc(x);", "_4 = inc(x);\n    _4 = inc(x);")],
    ),
    ("P10 main returns 1", "main", &[("return 0;", "return 1;")]),
    (
        "a printed value changed inside a loop",
        "loop_print",
        &[("printf(_3, i);", "printf(_3, n);")],
    ),
    (
        "a precision changed in the string table",
        "floats",
        &[("\"%s %s %.3f\\n\"", "\"%s %s %.4f\\n\"")],
    ),
    (
        "Debug rendering turned into Display",
        "floats",
        &[(
            "_3 = build_f64_to_debug_string(x);",
            "_3 = build_f64_to_string(x);",
        )],
    ),
    (
        "stderr turned into stdout",
        "floats",
        &[("fprintf(stderr, _8, _7);", "printf(_8, _7);")],
    ),
    (
        "two calls in main swapped",
        "main",
        &[(
            "loop_print(3);\nbb4:\n    floats(2.5);",
            "floats(2.5);\nbb4:\n    loop_print(3);",
        )],
    ),
];

#[test]
fn planted_effect_rewrites_are_refused() {
    let dir = std::env::temp_dir().join(format!("bl-tv-effects-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let doc = tv(Some(&dir));
    assert_eq!(status_of(&doc, "main"), "certified", "{doc}");
    let cert_path = dir.join("effects.tv-cert.json");
    let c_path = dir.join("effects.c");
    let cert0: Value = serde_json::from_str(&std::fs::read_to_string(&cert_path).unwrap()).unwrap();
    let c0 = std::fs::read_to_string(&c_path).unwrap();
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
    assert!(accepted.is_empty(), "{}", accepted.join("\n"));
}
