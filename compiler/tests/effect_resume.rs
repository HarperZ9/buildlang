//! `resume` in a handler clause: only tail position is supported. A clause
//! that resumes from tail position runs at the `perform` site; a `resume`
//! anywhere else is refused at check time with a message that names the
//! supported form.

use std::path::PathBuf;
use std::process::Command;

fn check(tag: &str, source: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("bl_effect_resume_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let file = dir.join("main.bld");
    std::fs::write(&file, source).expect("write program");
    let stdlib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stdlib");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg("check")
        .arg(&file)
        .env("BUILDLANG_STDLIB", stdlib)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

const EFFECT: &str = "effect Ask { fn ask() -> i32, }\nfn f() -> i32 ~ Ask { let v: i32 = perform Ask.ask(); v + 1 }\n";

fn program(clause: &str) -> String {
    format!(
        "{EFFECT}fn main() ~ Console {{ let v: i32 = handle {{ f() }} with {{ Ask.ask() => |resume| {clause}, }}; println!(\"{{}}\", v); }}\n"
    )
}

#[test]
fn resume_before_more_work_is_refused() {
    let (ok, text) = check("let", &program("{ let x: i32 = resume(1); x + 1 }"));
    assert!(!ok, "a non-tail resume must be refused:\n{text}");
    assert!(text.contains("outside tail position"), "{text}");
    assert!(
        text.contains("resume(4)"),
        "the message names the supported form:\n{text}"
    );
}

#[test]
fn resume_inside_an_argument_is_refused() {
    let (ok, text) = check("arg", &program("{ println!(\"{}\", resume(2)); 0 }"));
    assert!(!ok, "{text}");
    assert!(text.contains("outside tail position"), "{text}");
}

#[test]
fn resume_on_some_paths_only_is_refused() {
    let (ok, text) = check("partial", &program("{ if true { resume(1) } else { 0 } }"));
    assert!(!ok, "{text}");
    assert!(text.contains("resumes on some paths only"), "{text}");
}

#[test]
fn tail_resume_in_block_if_and_match_passes() {
    for (tag, clause) in [
        ("block", "{ resume(4) }"),
        ("if", "{ if true { resume(1) } else { resume(2) } }"),
        ("match", "{ match 3 { 3 => resume(1), _ => resume(2) } }"),
    ] {
        let (ok, text) = check(tag, &program(clause));
        assert!(ok, "{tag}: a tail resume must pass the check:\n{text}");
    }
}
