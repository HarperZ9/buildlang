//! M4: the integer and division rules the Low Water traces use.
//!
//! `tests/int32/js_rules.bld` and its `.expected` file are generated together
//! by `tools/int32_fixture.mjs`, so every expected line is what node computed:
//! - the game's `hashStr` (32-bit xor and `Math.imul`) over 9 strings and 6
//!   seeds, ported with `u32` `^`, `>>` and `wrapping_mul`;
//! - JS ToInt32 and ToUint32 (`x | 0`, `x >>> 0`) on 20 doubles, including
//!   NaN, the infinities, -0 and values past 2^32, through `to_int32()` and
//!   `to_uint32()`;
//! - `fmod_or_zero` and `div_or_zero`, which give 0 for a zero divisor, as bit
//!   patterns, so a -0 or NaN in place of 0 fails;
//! - `i32` `wrapping_mul` and `^` on a negative operand.
//!
//! On 1.8.0 the program does not build: `0_u32` failed to lex, and the four
//! float methods did not exist.

use std::{path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler manifest should have a repository parent")
        .to_path_buf()
}

#[test]
fn js_integer_and_division_rules_match_node() {
    let dir = repo_root().join("tests/int32");
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(dir.join("js_rules.bld"))
        .output()
        .expect("buildc runs");
    assert!(
        out.status.success(),
        "js_rules.bld failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let want = std::fs::read_to_string(dir.join("js_rules.expected"))
        .unwrap()
        .replace("\r\n", "\n");
    let got: Vec<&str> = got.lines().collect();
    let want: Vec<&str> = want.lines().collect();
    assert_eq!(got.len(), want.len(), "line count differs");
    let bad: Vec<String> = got
        .iter()
        .zip(&want)
        .enumerate()
        .filter(|(_, (g, w))| g != w)
        .map(|(i, (g, w))| format!("line {}: got {g}, want {w}", i + 1))
        .collect();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
