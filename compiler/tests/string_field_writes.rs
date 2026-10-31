//! A `String` field appended with `push_str` keeps the append on the C backend.
//!
//! Found by the typesetting work (2026-10-31): `self.ir.push_str(s)` inside a
//! `&mut self` method printed `ir=[]`. The lowering read the field into a
//! temporary, concatenated, and wrote the result back to the temporary, so
//! every append to a field was lost: through `&mut self`, through a `&mut`
//! parameter, and on a struct held by value. Only a plain string local kept
//! its append. Each program here must print exactly the expected text.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

fn have_c_compiler() -> bool {
    ["gcc", "clang", "cc"].iter().any(|cc| {
        Command::new(cc)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

fn run(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "buildlang-strfield-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(format!("{name}.bld"));
    std::fs::write(&path, src).expect("write program");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "{name} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&out.stdout)
        .replace("\r\n", "\n")
        .trim_end()
        .to_string()
}

const DOC: &str = r#"
pub struct Doc {
    ir: String,
    count: i32,
}

pub struct Outer {
    doc: Doc,
}

impl Doc {
    pub fn add(&mut self, s: str) {
        self.ir.push_str(s);
        self.count = self.count + 1;
    }
}

fn free_add(d: &mut Doc, s: str) {
    d.ir.push_str(s);
}
"#;

#[test]
fn push_str_through_mut_self_keeps_the_append() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let src = format!(
        "{DOC}\nfn main() ~ Console {{\n    let mut d = Doc {{ ir: String::new(), count: 0 }};\n    d.add(\"a\");\n    d.add(\"b\");\n    println!(\"count={{}} ir=[{{}}]\", d.count, d.ir);\n}}\n"
    );
    assert_eq!(run("self", &src), "count=2 ir=[ab]");
}

#[test]
fn push_str_through_a_mut_parameter_keeps_the_append() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let src = format!(
        "{DOC}\nfn main() ~ Console {{\n    let mut d = Doc {{ ir: String::new(), count: 0 }};\n    free_add(&mut d, \"x\");\n    free_add(&mut d, \"y\");\n    println!(\"ir=[{{}}]\", d.ir);\n}}\n"
    );
    assert_eq!(run("param", &src), "ir=[xy]");
}

#[test]
fn push_str_on_fields_held_by_value_keeps_the_append() {
    if !have_c_compiler() {
        eprintln!("skipping: no C compiler");
        return;
    }
    let src = format!(
        "{DOC}\nfn main() ~ Console {{\n    let mut d = Doc {{ ir: String::from(\"<\"), count: 0 }};\n    d.ir.push_str(\"1\");\n    let mut o = Outer {{ doc: Doc {{ ir: String::new(), count: 0 }} }};\n    o.doc.ir.push_str(\"2\");\n    o.doc.ir.push_str(\"3\");\n    let mut s = String::new();\n    s.push_str(\"4\");\n    println!(\"{{}} {{}} {{}}\", d.ir, o.doc.ir, s);\n}}\n"
    );
    assert_eq!(run("value", &src), "<1 23 4");
}
