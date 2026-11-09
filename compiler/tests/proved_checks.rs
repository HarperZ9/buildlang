//! Overflow checks dropped where two independent passes prove a step cannot
//! overflow (the C backend's `c/proved_checks.rs`), with a planted wrong proof
//! that must not remove a needed check, and `buildc tv` certifying the result.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

const SRC: &str = "fn count(n: i8) -> i8 {\n\
\x20   let mut i: i8 = 0;\n\
\x20   while i < n { i = i + 1; }\n\
\x20   i\n\
}\n\
fn down(n: u8) -> u8 {\n\
\x20   let mut i: u8 = n;\n\
\x20   while i > 0 { i = i - 1; }\n\
\x20   i\n\
}\n\
fn inc(x: i8) -> i8 { x + 1 }\n\
fn killed(n: i8, m: i8) -> i8 {\n\
\x20   let mut i: i8 = 0;\n\
\x20   if i < n { i = m; i = i + 1; }\n\
\x20   i\n\
}\n";

fn buildc(dir: &Path, plant: bool) -> Command {
    buildc_mode(dir, if plant { "every-step" } else { "" })
}

fn buildc_mode(dir: &Path, mode: &str) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_buildc"));
    c.env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .current_dir(dir);
    if mode.is_empty() {
        c.env_remove("BUILDLANG_PLANT_PROOF");
    } else {
        c.env("BUILDLANG_PLANT_PROOF", mode);
    }
    c
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
    c: String,
}

fn run(tag: &str, main: &str, plant: bool) -> Option<Out> {
    let dir = std::env::temp_dir().join(format!("bl-proved-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.bld"),
        format!("{SRC}fn main() ~ Console {{\n{main}\n}}\n"),
    )
    .unwrap();
    let emit = buildc(&dir, plant)
        .args(["-o", "main.c", "main.bld"])
        .output()
        .unwrap();
    assert!(
        emit.status.success(),
        "{}",
        String::from_utf8_lossy(&emit.stderr)
    );
    let c = std::fs::read_to_string(dir.join("main.c")).unwrap();
    let out = buildc(&dir, plant)
        .args(["run", "main.bld"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    if stderr.contains("no C compiler") {
        return None;
    }
    Some(Out {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr,
        c,
    })
}

/// The body of C function `name`.
fn c_fn<'a>(c: &'a str, name: &str) -> &'a str {
    let pat = format!(" {name}(");
    let head = c
        .match_indices(&pat)
        .map(|(i, _)| i)
        .find(|&i| {
            c[i..]
                .lines()
                .next()
                .is_some_and(|l| l.trim_end().ends_with('{'))
        })
        .expect("function definition");
    let end = c[head..].find("\n}\n").map(|e| head + e).unwrap_or(c.len());
    &c[head..end]
}

#[test]
fn guarded_steps_drop_their_check_and_compute_the_same() {
    let main = "    println!(\"{}\", count(127));\n    println!(\"{}\", down(255));";
    let Some(o) = run("guarded", main, false) else {
        return;
    };
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(o.stdout, "127\n0\n");
    assert!(
        c_fn(&o.c, "count").contains("bl_iadd_wrapping_i8("),
        "count step proved"
    );
    assert!(
        c_fn(&o.c, "down").contains("bl_usub_wrapping_u8("),
        "down step proved"
    );
    assert!(
        c_fn(&o.c, "inc").contains("bl_iadd_i8("),
        "unguarded step keeps its check"
    );
    assert!(
        c_fn(&o.c, "killed").contains("bl_iadd_i8("),
        "a step after a reassignment keeps its check"
    );
}

#[test]
fn unguarded_and_killed_steps_still_trap() {
    for (tag, call) in [("inc", "inc(127)"), ("killed", "killed(5, 127)")] {
        let main = format!("    println!(\"{{}}\", {call});");
        let Some(o) = run(tag, &main, false) else {
            return;
        };
        assert_eq!(o.code, 101, "{call}: {}", o.stdout);
        assert!(
            o.stderr.contains("attempt to add with overflow"),
            "{}",
            o.stderr
        );
    }
}

#[test]
fn a_planted_wrong_proof_does_not_remove_a_needed_check() {
    // The prover claims every `+ 1` and `- 1`; the checker must refuse the
    // claims for `inc` and `killed`, so both still trap.
    for (tag, call) in [("pinc", "inc(127)"), ("pkilled", "killed(5, 127)")] {
        let main = format!("    println!(\"{{}}\", {call});");
        let Some(o) = run(tag, &main, true) else {
            return;
        };
        assert!(
            c_fn(&o.c, "inc").contains("bl_iadd_i8("),
            "plant removed inc's check"
        );
        assert!(
            c_fn(&o.c, "killed").contains("bl_iadd_i8("),
            "plant removed killed's check"
        );
        assert!(c_fn(&o.c, "count").contains("bl_iadd_wrapping_i8("));
        assert_eq!(o.code, 101, "{call}");
    }
}

#[test]
fn translation_validation_certifies_the_functions_with_dropped_checks() {
    let dir = std::env::temp_dir().join(format!("bl-proved-tv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.bld"),
        format!("{SRC}fn main() ~ Console {{ println!(\"{{}}\", count(3)); }}\n"),
    )
    .unwrap();
    let out = buildc(&dir, false)
        .args(["tv", "--json", "main.bld"])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if String::from_utf8_lossy(&out.stderr).contains("no C compiler") {
        return;
    }
    let v: serde_json::Value = serde_json::from_str(&text).expect("tv json");
    for f in v["functions"].as_array().unwrap() {
        let name = f["name"].as_str().unwrap();
        if ["count", "down", "inc", "killed"].contains(&name) {
            assert_eq!(f["status"], "certified", "{name}: {f}");
        }
    }
}

#[test]
fn translation_validation_refuses_a_check_dropped_without_a_proof() {
    // With the checker bypassed, `inc`'s needed check is dropped; tv must
    // refuse `inc` (the C no longer traps where the source does) and still
    // certify `count`, whose proof is real.
    let dir = std::env::temp_dir().join(format!("bl-proved-tvp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.bld"),
        format!("{SRC}fn main() ~ Console {{ println!(\"{{}}\", count(3)); }}\n"),
    )
    .unwrap();
    let out = buildc_mode(&dir, "every-step-unchecked")
        .args(["tv", "--json", "main.bld"])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    if String::from_utf8_lossy(&out.stderr).contains("no C compiler") {
        return;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("tv json");
    let status = |n: &str| {
        v["functions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == n)
            .map(|f| f["status"].clone())
    };
    assert_eq!(
        status("inc"),
        Some(serde_json::json!("not-certified")),
        "{v}"
    );
    assert_eq!(
        status("killed"),
        Some(serde_json::json!("not-certified")),
        "{v}"
    );
    assert_eq!(status("count"), Some(serde_json::json!("certified")), "{v}");
}
