//! Device access must reach the C compiler as written. `ptr::read_volatile`
//! and `ptr::write_volatile` failed to compile (G2), and `asm!("hlt")` built
//! with an empty body (G3). These check the emitted C and the error for the
//! unsupported `asm!` operand form.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// `buildc build --emit c` on `src`: (success, emitted C, stderr).
fn emit_c(tag: &str, src: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("bl-lowlevel-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.bld"), src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build", "--emit", "c"])
        .arg(&dir)
        .output()
        .expect("buildc runs");
    let c = std::fs::read_to_string(dir.join("target").join("debug").join("main.c"))
        .unwrap_or_default();
    let stderr =
        String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.success(), c, stderr)
}

const VOLATILE_SRC: &str = "fn wait_ready(a: usize) -> u32 {\n\
    let p = a as *const u8;\n\
    let mut n: u32 = 0_u32;\n\
    while unsafe { ptr::read_volatile(p) } == 0_u8 { n = n + 1_u32; }\n\
    n\n\
}\n\
fn put(a: usize, v: u32) {\n\
    let p = a as *mut u32;\n\
    unsafe { ptr::write_volatile(p, v); }\n\
}\n\
fn main() ~ Console {\n\
    let mut flag: u8 = 1_u8;\n\
    let fa = &mut flag as *mut u8 as usize;\n\
    println!(\"{}\", wait_ready(fa));\n\
}\n";

#[test]
fn volatile_read_and_write_are_volatile_in_c() {
    let (ok, c, err) = emit_c("vol", VOLATILE_SRC);
    assert!(ok, "build failed: {err}");
    assert!(
        c.contains("*(volatile uint8_t *)(p)"),
        "no volatile read:\n{c}"
    );
    assert!(
        c.contains("*(volatile uint32_t *)(p) = (uint32_t)(v);"),
        "no volatile write"
    );
}

#[test]
fn volatile_read_in_loop_condition_is_read_each_iteration() {
    let (ok, c, err) = emit_c("loop", VOLATILE_SRC);
    assert!(ok, "build failed: {err}");
    let c = c.replace("\r\n", "\n");
    let start = c
        .find("wait_ready(uintptr_t a) {")
        .expect("wait_ready definition");
    let body = &c[start..];
    let body = &body[..body.find("\n}\n").unwrap()];
    let read = body
        .find("*(volatile uint8_t *)(p)")
        .expect("volatile read in wait_ready");
    // The block that holds the read: the last `bbN:` label before it.
    let label_at = body[..read]
        .rfind("\nbb")
        .expect("the read sits in a labelled block")
        + 1;
    let label = &body[label_at..label_at + body[label_at..].find(':').unwrap()];
    assert!(
        !body[label_at..read].contains("goto"),
        "the read is not in the labelled block\n{body}"
    );
    // A jump back to that block after the read is the loop's back edge, so
    // the C performs the read again on every iteration.
    assert!(
        body[read..].contains(&format!("goto {label};")),
        "no back edge to the block holding the read\n{body}"
    );
}

#[test]
fn asm_string_template_is_emitted() {
    let src = "fn halt() { unsafe { asm!(\"nop\"); } }\n\
               fn two() { unsafe { asm!(\"nop\", \"nop\"); } }\n\
               fn main() ~ Console { halt(); two(); println!(\"ok\"); }\n";
    let (ok, c, err) = emit_c("asm", src);
    assert!(ok, "build failed: {err}");
    assert!(
        c.contains("__asm__ __volatile__(\"nop\");"),
        "asm dropped:\n{c}"
    );
    assert!(
        c.contains("__asm__ __volatile__(\"nop\\n\\tnop\");"),
        "two templates not joined"
    );
}

#[test]
fn asm_with_operands_is_a_compile_error_with_a_line() {
    let src = "fn f(x: u64) {\n    unsafe { asm!(\"mov {0}, 1\", out(reg) x); }\n}\n\
               fn main() ~ Console { f(1_u64); }\n";
    let (ok, _c, err) = emit_c("asmop", src);
    assert!(!ok, "operand form must not build");
    assert!(
        err.contains("operands and options are not supported yet"),
        "{err}"
    );
    assert!(
        err.contains("main.bld:2:"),
        "error does not name line 2: {err}"
    );
}

#[test]
fn other_backends_refuse_asm_and_volatile() {
    let dir = std::env::temp_dir().join(format!("bl-lowlevel-{}-llvm", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.bld"),
        "fn halt() { unsafe { asm!(\"nop\"); } }\nfn main() ~ Console { halt(); }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .args(["build", "--emit", "c", "--target", "llvm"])
        .arg(&dir)
        .output()
        .expect("buildc runs");
    let text =
        String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!out.status.success(), "llvm target accepted asm!: {text}");
    assert!(text.contains("lowered only by the C backend"), "{text}");
}
