//! M1 exit criterion: `buildc check --error-format=json` output validates
//! against the committed schema `docs/schemas/buildlang.diagnostics.v1.schema.json`,
//! every code it reports is in the published table, and the human output reports
//! the same codes in the same order.
//!
//! The schema check uses a small validator written here for the keywords the
//! schema uses. It panics on any keyword it does not implement, so a schema
//! change cannot make the check pass by being ignored.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn buildc() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"));
    cmd
}

fn schema() -> Value {
    let path = repo_root().join("docs/schemas/buildlang.diagnostics.v1.schema.json");
    let text = std::fs::read_to_string(&path).expect("schema file is committed");
    serde_json::from_str(&text).expect("schema is JSON")
}

fn resolve<'a>(root: &'a Value, reference: &str) -> &'a Value {
    let pointer = reference
        .strip_prefix('#')
        .expect("only local $ref is supported");
    root.pointer(pointer)
        .unwrap_or_else(|| panic!("unresolved $ref {reference}"))
}

fn type_matches(name: &str, v: &Value) -> bool {
    match name {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        other => panic!("unsupported type {other}"),
    }
}

fn pattern_matches(pattern: &str, s: &str) -> bool {
    match pattern {
        "^BL[0-9]{4}$" => {
            s.len() == 6 && s.starts_with("BL") && s[2..].chars().all(|c| c.is_ascii_digit())
        }
        other => panic!("the test validator does not implement pattern {other}"),
    }
}

/// Validate `v` against `schema`; push one message per violation.
fn validate(root: &Value, schema: &Value, v: &Value, at: &str, errs: &mut Vec<String>) {
    let obj = schema.as_object().expect("schema node is an object");
    for (key, rule) in obj {
        match key.as_str() {
            "$schema" | "$id" | "title" | "description" | "$defs" => {}
            "$ref" => validate(root, resolve(root, rule.as_str().unwrap()), v, at, errs),
            "type" => {
                if !type_matches(rule.as_str().unwrap(), v) {
                    errs.push(format!("{at}: expected {rule}, found {v}"));
                }
            }
            "const" => {
                if v != rule {
                    errs.push(format!("{at}: expected const {rule}, found {v}"));
                }
            }
            "enum" => {
                if !rule.as_array().unwrap().contains(v) {
                    errs.push(format!("{at}: {v} not in {rule}"));
                }
            }
            "pattern" => {
                if let Some(s) = v.as_str() {
                    if !pattern_matches(rule.as_str().unwrap(), s) {
                        errs.push(format!("{at}: {s} does not match {rule}"));
                    }
                }
            }
            "minimum" => {
                if let Some(n) = v.as_f64() {
                    if n < rule.as_f64().unwrap() {
                        errs.push(format!("{at}: {n} below minimum {rule}"));
                    }
                }
            }
            "required" => {
                if let Some(o) = v.as_object() {
                    for name in rule.as_array().unwrap() {
                        if !o.contains_key(name.as_str().unwrap()) {
                            errs.push(format!("{at}: missing required {name}"));
                        }
                    }
                }
            }
            "properties" => {
                if let Some(o) = v.as_object() {
                    for (name, sub) in rule.as_object().unwrap() {
                        if let Some(child) = o.get(name) {
                            validate(root, sub, child, &format!("{at}.{name}"), errs);
                        }
                    }
                }
            }
            "additionalProperties" => {
                assert_eq!(
                    rule,
                    &Value::Bool(false),
                    "only additionalProperties: false"
                );
                let known = obj
                    .get("properties")
                    .and_then(Value::as_object)
                    .expect("additionalProperties needs properties");
                if let Some(o) = v.as_object() {
                    for name in o.keys() {
                        if !known.contains_key(name) {
                            errs.push(format!("{at}: unexpected property {name}"));
                        }
                    }
                }
            }
            "items" => {
                if let Some(items) = v.as_array() {
                    for (i, item) in items.iter().enumerate() {
                        validate(root, rule, item, &format!("{at}[{i}]"), errs);
                    }
                }
            }
            "anyOf" => {
                let ok = rule.as_array().unwrap().iter().any(|alt| {
                    let mut sub = Vec::new();
                    validate(root, alt, v, at, &mut sub);
                    sub.is_empty()
                });
                if !ok {
                    errs.push(format!("{at}: matches no anyOf branch: {v}"));
                }
            }
            other => panic!("the test validator does not implement keyword {other}"),
        }
    }
}

fn published_codes() -> Vec<String> {
    let out = buildc()
        .arg("explain")
        .output()
        .expect("run buildc explain");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

struct Run {
    report: Value,
    success: bool,
}

fn check_json(file: &Path, extra: &[&str]) -> Run {
    let out = buildc()
        .arg("check")
        .arg(file)
        .args(extra)
        .arg("--error-format=json")
        .output()
        .expect("run buildc check --error-format=json");
    let text = String::from_utf8_lossy(&out.stdout);
    let report: Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!(
            "{}: stdout is not one JSON value ({e}):\n{text}",
            file.display()
        )
    });
    Run {
        report,
        success: out.status.success(),
    }
}

fn human_codes(file: &Path) -> Vec<String> {
    let out = buildc()
        .arg("check")
        .arg(file)
        .output()
        .expect("run buildc check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut codes = Vec::new();
    for line in stderr.lines() {
        if let Some(start) = line.find("[BL") {
            let code = &line[start + 1..start + 7];
            codes.push(code.to_string());
        }
    }
    codes
}

fn assert_valid(run: &Run, file: &Path, schema: &Value, codes: &[String]) {
    let mut errs = Vec::new();
    validate(schema, schema, &run.report, "$", &mut errs);
    assert!(
        errs.is_empty(),
        "{}: schema violations:\n{}",
        file.display(),
        errs.join("\n")
    );
    let diags = run.report["diagnostics"].as_array().unwrap();
    assert_eq!(
        run.report["error_count"].as_u64().unwrap() as usize,
        diags.len()
    );
    assert_eq!(
        run.report["ok"].as_bool().unwrap(),
        run.success,
        "{}",
        file.display()
    );
    assert_eq!(diags.is_empty(), run.success, "{}", file.display());
    for d in diags {
        let code = d["code"].as_str().unwrap();
        assert!(
            codes.iter().any(|c| c == code),
            "{code} is not in `buildc explain`"
        );
        let help: Vec<_> = d["help"].as_array().unwrap().iter().collect();
        for s in d["suggestions"].as_array().unwrap() {
            assert!(
                help.contains(&&s["message"]),
                "suggestion not printed as help: {s}"
            );
            if s["widens_authority"] == true {
                assert_ne!(s["applicability"], "MachineApplicable", "{s}");
            }
        }
    }
}

fn fixture(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("buildlang_diag_json_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write fixture");
    path
}

#[test]
fn json_reports_validate_for_targeted_failures() {
    let schema = schema();
    let codes = published_codes();
    let cases: &[(&str, &str, &str)] = &[
        ("parse.bld", "fn main() ~ Console {\n    let x = 5\n    println!(\"{}\", x);\n}\n", "BL0202"),
        ("lex.bld", "fn main() ~ Console {\n    let s = \"open;\n}\n", "BL0103"),
        ("type.bld", "fn main() ~ Console {\n    let x: i32 = \"a\";\n    println!(\"{}\", x);\n}\n", "BL0301"),
        ("effect.bld", "fn main() ~ Console {\n    let s = read_file(\"a.txt\");\n    println!(\"{}\", s);\n}\n", "BL0403"),
        ("pure.bld", "fn main() {\n    println!(\"hi\");\n}\n", "BL0402"),
        ("fmt.bld", "fn main() ~ Console {\n    let n = 3;\n    println!(n);\n}\n", "BL0620"),
    ];
    for (name, source, want) in cases {
        let file = fixture(name, source);
        let run = check_json(&file, &[]);
        assert_valid(&run, &file, &schema, &codes);
        assert!(!run.success, "{name} must fail");
        let got: Vec<_> = run.report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap().to_string())
            .collect();
        assert!(
            got.iter().any(|c| c == want),
            "{name}: expected {want}, got {got:?}"
        );
        let first = &run.report["diagnostics"][0];
        if *want != "BL0402" && *want != "BL0403" {
            assert!(
                first["span"]["line"].as_u64().unwrap() >= 1,
                "{name} has a span"
            );
        }
    }

    // Missing input: still one JSON report, code BL0001, no span.
    let missing = repo_root().join("no-such-file.bld");
    let run = check_json(&missing, &[]);
    assert_valid(&run, &missing, &schema, &codes);
    assert_eq!(run.report["diagnostics"][0]["code"], "BL0001");
    assert!(run.report["diagnostics"][0]["span"].is_null());

    // A policy violation is a diagnostic too.
    let file = fixture(
        "policy.bld",
        "fn main() ~ Console {\n    println!(\"hi\");\n}\n",
    );
    let run = check_json(&file, &["--profile", "pure"]);
    assert_valid(&run, &file, &schema, &codes);
    assert!(!run.success);
    assert!(run.report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .all(|d| d["code"] == "BL0090"));

    // A passing program: ok, empty list.
    let run = check_json(&file, &[]);
    assert_valid(&run, &file, &schema, &codes);
    assert!(run.success);
}

#[test]
fn json_and_human_output_agree_over_the_agent_probe() {
    let schema = schema();
    let codes = published_codes();
    let mut files: Vec<PathBuf> = std::fs::read_dir(repo_root().join("tests/agent-probe"))
        .expect("tests/agent-probe exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("bld"))
        .collect();
    files.sort();
    assert!(files.len() >= 20, "probe programs went missing");
    let mut failing = 0;
    for file in &files {
        let run = check_json(file, &[]);
        assert_valid(&run, file, &schema, &codes);
        let json_codes: Vec<String> = run.report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            json_codes,
            human_codes(file),
            "{}: human and JSON codes differ",
            file.display()
        );
        if !run.success {
            failing += 1;
        }
    }
    assert!(
        failing >= 1,
        "the probe holds programs that must be rejected"
    );
}

#[test]
fn json_with_receipt_on_stdout_is_refused() {
    let file = fixture(
        "both.bld",
        "fn main() ~ Console {\n    println!(\"hi\");\n}\n",
    );
    let out = buildc()
        .arg("check")
        .arg(&file)
        .args(["--receipt", "-", "--error-format=json"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--error-format=json writes to stdout"));
}

#[test]
fn explain_prints_a_code_and_rejects_an_unknown_one() {
    let out = buildc().args(["explain", "bl0403"]).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("BL0403: undeclared effect"));
    let out = buildc().args(["explain", "BL9999"]).output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn validator_rejects_a_bad_report() {
    // Check the check: a report broken three ways yields three violations.
    let schema = schema();
    let file = fixture(
        "bad.bld",
        "fn main() ~ Console {\n    let x: i32 = \"a\";\n}\n",
    );
    let mut report = check_json(&file, &[]).report;
    let d = &mut report["diagnostics"][0];
    d["code"] = Value::from("X1");
    d.as_object_mut().unwrap().remove("help");
    d["extra"] = Value::from(1);
    let mut errs = Vec::new();
    validate(&schema, &schema, &report, "$", &mut errs);
    assert_eq!(errs.len(), 3, "{errs:#?}");
}
