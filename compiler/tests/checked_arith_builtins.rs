//! Plain integer `+`, `-` and `*` trap on overflow. The C helpers now use the
//! gcc and clang overflow builtins; these cases hold the semantics at every
//! width: a result at the edge of the type is exact, and one past it traps
//! with the operation's message and exit code 101.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run `body` as `main`; `None` when no C compiler is available.
fn run(tag: &str, helpers: &str, body: &str) -> Option<(i32, String, String)> {
    let dir = std::env::temp_dir().join(format!("bl-arith-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.bld");
    std::fs::write(
        &file,
        format!("{helpers}\nfn main() ~ Console {{\n{body}\n}}\n"),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .env("BUILDLANG_STDLIB", repo_root().join("stdlib"))
        .arg("run")
        .arg(&file)
        .output()
        .expect("buildc runs");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    if stderr.contains("no C compiler") {
        return None;
    }
    Some((out.status.code().unwrap_or(-1), stdout, stderr))
}

/// (type, min, max)
const TYPES: [(&str, i128, i128); 8] = [
    ("i8", -128, 127),
    ("i16", -32768, 32767),
    ("i32", -2147483648, 2147483647),
    ("i64", -9223372036854775808, 9223372036854775807),
    ("u8", 0, 255),
    ("u16", 0, 65535),
    ("u32", 0, 4294967295),
    ("u64", 0, 18446744073709551615),
];

fn helpers(t: &str) -> String {
    format!(
        "fn add(a: {t}, b: {t}) -> {t} {{ a + b }}\n\
         fn sub(a: {t}, b: {t}) -> {t} {{ a - b }}\n\
         fn mul(a: {t}, b: {t}) -> {t} {{ a * b }}\n\
         fn from_parts(hi: u64, lo: u64) -> u64 {{ hi * 4294967296 + lo }}\n"
    )
}

/// `v` as an expression of type `t`. `i64::MIN` is written by name, and a
/// `u64` past `i64` is built from two 32-bit halves at run time: written
/// from `u64::MAX` it hits issue #215 (the temporary is an `int32_t`).
fn lit(t: &str, v: i128) -> String {
    if v == i64::MIN as i128 {
        "i64::MIN".to_string()
    } else if v > i64::MAX as i128 {
        format!(
            "from_parts(({} as u64), ({} as u64))",
            v >> 32,
            v & 0xffff_ffff
        )
    } else {
        format!("({v} as {t})")
    }
}

#[test]
fn results_at_the_edge_of_each_type_are_exact() {
    for (t, min, max) in TYPES {
        let cases = [
            ("add", max - 1, 1, max),
            ("sub", min + 1, 1, min),
            ("mul", max / 2, 2, (max / 2) * 2),
            ("add", min, 0, min),
        ];
        let mut body = String::new();
        let mut want = String::new();
        for (op, a, b, r) in cases {
            body.push_str(&format!(
                "    let r_{op}_{n}: {t} = {op}({}, {});\n    println!(\"{{}}\", r_{op}_{n});\n",
                lit(t, a),
                lit(t, b),
                n = want.lines().count()
            ));
            want.push_str(&format!("{r}\n"));
        }
        let Some((code, out, err)) = run(&format!("edge-{t}"), &helpers(t), &body) else {
            eprintln!("skipping: no C compiler");
            return;
        };
        assert_eq!(code, 0, "{t}: {err}");
        assert_eq!(out, want, "{t}");
    }
}

#[test]
fn one_past_the_edge_traps() {
    for (t, min, max) in TYPES {
        let mut cases = vec![
            ("add", max, 1, "attempt to add with overflow"),
            ("sub", min, 1, "attempt to subtract with overflow"),
            ("mul", max / 2 + 1, 2, "attempt to multiply with overflow"),
        ];
        if min < 0 {
            cases.push(("mul", min, -1, "attempt to multiply with overflow"));
        }
        for (op, a, b, msg) in cases {
            let body = format!(
                "    let r: {t} = {op}({}, {});\n    println!(\"{{}}\", r);",
                lit(t, a),
                lit(t, b)
            );
            let Some((code, out, err)) = run(&format!("trap-{t}-{op}"), &helpers(t), &body) else {
                eprintln!("skipping: no C compiler");
                return;
            };
            assert_eq!(code, 101, "{t} {op}({a}, {b}) printed {out:?}: {err}");
            assert!(err.contains(msg), "{t} {op}({a}, {b}): {err}");
        }
    }
}
