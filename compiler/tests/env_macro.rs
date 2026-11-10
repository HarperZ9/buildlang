//! `env!("NAME")` reads a variable from the compiler's environment while it
//! compiles and puts the value in the program as a string. These run real
//! programs with the variable set and unset.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// `buildc run` on `src`, with `BL_ENV_TEST` set to `value` or removed.
fn run(tag: &str, src: &str, value: Option<&str>) -> Run {
    let dir = std::env::temp_dir().join(format!("bl-env-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buildc"));
    cmd.env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file);
    match value {
        Some(v) => cmd.env("BL_ENV_TEST", v),
        None => cmd.env_remove("BL_ENV_TEST"),
    };
    let out = cmd.output().expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

const PRINT: &str = "fn main() ~ Console + Environment {\n\
                     let v = env!(\"BL_ENV_TEST\");\n\
                     println!(\"[{}] {}\", v, v.len());\n\
                     }\n";

#[test]
fn the_value_set_at_compile_time_is_in_the_program() {
    let r = run("set", PRINT, Some("alpha beta"));
    assert!(r.ok, "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim(), "[alpha beta] 10");
}

#[test]
fn quotes_backslashes_and_percent_signs_survive() {
    let value = r#"a"b\c %d {x}"#;
    let r = run("escape", PRINT, Some(value));
    assert!(r.ok, "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim(), format!("[{value}] {}", value.len()));
}

#[test]
fn an_empty_value_is_an_empty_string() {
    let r = run("empty", PRINT, Some(""));
    assert!(r.ok, "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim(), "[] 0");
}

#[test]
fn an_unset_variable_is_a_compile_error_naming_it() {
    let r = run("unset", PRINT, None);
    assert!(!r.ok, "built and ran: {}", r.stdout);
    assert!(r.stdout.is_empty(), "program ran: {}", r.stdout);
    assert!(
        r.stderr
            .contains("environment variable `BL_ENV_TEST` is not set at compile time"),
        "{}",
        r.stderr
    );
}

#[test]
fn the_custom_message_is_the_error_text() {
    let src = "fn main() ~ Console + Environment {\n\
               let v = env!(\"BL_ENV_TEST\", \"set BL_ENV_TEST to the build label\");\n\
               println!(\"{}\", v);\n\
               }\n";
    let r = run("custom", src, None);
    assert!(!r.ok);
    assert!(
        r.stderr.contains("set BL_ENV_TEST to the build label"),
        "{}",
        r.stderr
    );
    let r = run("custom-set", src, Some("label-7"));
    assert!(r.ok, "stderr: {}", r.stderr);
    assert_eq!(r.stdout.trim(), "label-7");
}

#[test]
fn a_name_that_is_not_a_string_literal_is_a_compile_error() {
    for (tag, call) in [
        ("ident", "env!(BL_ENV_TEST)"),
        ("none", "env!()"),
        ("three", "env!(\"BL_ENV_TEST\", \"m\", \"n\")"),
        ("number", "env!(7)"),
    ] {
        let src = format!("fn main() ~ Console + Environment {{\nlet v = {call};\n}}\n");
        let r = run(tag, &src, Some("x"));
        assert!(!r.ok, "{tag}: built");
        assert!(r.stderr.contains("env!"), "{tag}: {}", r.stderr);
    }
}

#[test]
fn the_environment_effect_is_required() {
    let src = "fn main() ~ Console {\n\
               let v = env!(\"BL_ENV_TEST\");\n\
               println!(\"{}\", v);\n\
               }\n";
    let r = run("effect", src, Some("x"));
    assert!(!r.ok, "built without the Environment effect");
    assert!(r.stderr.contains("Environment"), "{}", r.stderr);
}

#[test]
fn the_value_has_the_string_type() {
    let src = "fn main() ~ Console + Environment {\n\
               let n: i32 = env!(\"BL_ENV_TEST\");\n\
               println!(\"{}\", n);\n\
               }\n";
    let r = run("type", src, Some("12"));
    assert!(!r.ok, "a string was accepted as i32: {}", r.stdout);
}
