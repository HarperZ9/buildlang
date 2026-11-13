//! A program the Drop lowering cannot run soundly is refused at `buildc check`
//! with the reason and a fix, rather than dropping a value twice.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn moving_a_dropped_field_out_is_refused_at_its_line() {
    let dir = std::env::temp_dir().join(format!("bl-drop-refuse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(
        &file,
        "struct G { v: i32 }\n\
         impl Drop for G { fn drop(&mut self) ~ Console { println!(\"drop\"); } }\n\
         struct H { g: G }\n\
         fn main() ~ Console {\n\
         \x20   let h: H = H { g: G { v: 1 } };\n\
         \x20   let x: G = h.g;\n\
         \x20   println!(\"{}\", x.v);\n\
         }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("check")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let _ = std::fs::remove_dir_all(&dir);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "check must refuse:\n{text}");
    assert!(
        text.contains("moving the field `g` out of a value that is dropped"),
        "{text}"
    );
    assert!(
        text.contains("6:"),
        "diagnostic should point at line 6:\n{text}"
    );
}
