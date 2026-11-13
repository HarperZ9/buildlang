// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The WASM backend's binary output, run in Node's built-in `WebAssembly`,
//! must print what the C backend prints for the same program.
//! Skips with a message when `node` is not on PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUNNER: &str = r#"
const fs = require('fs');
const bytes = fs.readFileSync(process.argv[1]);
let mem; let out = '';
const env = {
  print_i64: v => { out += v.toString(); },
  print_u64: v => { out += BigInt.asUintN(64, v).toString(); },
  print_f64: v => { out += String(v); },
  print_bool: v => { out += v ? 'true' : 'false'; },
  print_str: (p, l) => { out += Buffer.from(new Uint8Array(mem.buffer, p, l)).toString('utf8'); },
};
WebAssembly.instantiate(bytes, { env }).then(({ instance }) => {
  mem = instance.exports.memory;
  instance.exports.main();
  process.stdout.write(out);
}).catch(e => { console.error(String(e)); process.exit(1); });
"#;

const PROGRAMS: [(&str, &str, &str); 4] = [
    (
        "arith",
        "fn add_mul(a: i64, b: i64) -> i64 {\n    (a + b) * 3 - a / 2\n}\n\nfn main() ~ Console {\n    let r: i64 = add_mul(7, 5);\n    println(\"{}\", r);\n}\n",
        "33\n",
    ),
    (
        "while_sum",
        "fn main() ~ Console {\n    let mut i: i64 = 1;\n    let mut sum: i64 = 0;\n    while i <= 100 {\n        sum = sum + i;\n        i = i + 1;\n    }\n    println(\"sum {}\", sum);\n}\n",
        "sum 5050\n",
    ),
    (
        "if_else",
        "fn pick(x: i64) -> i64 {\n    if x > 10 { x * 2 } else { x - 1 }\n}\n\nfn main() ~ Console {\n    println(\"{}\", pick(21));\n    println(\"{} and {}\", pick(3), pick(11));\n}\n",
        "42\n2 and 22\n",
    ),
    (
        "arrays_u32_u8",
        r#"fn main() ~ Console {
    let mut a: [u32; 8] = [1_u32, 2_u32, 3_u32, 4_u32, 5_u32, 6_u32, 7_u32, 8_u32];
    let mut b: [u8; 4] = [0_u8; 4];
    let mut i: usize = 0;
    while i < 8 {
        a[i] = (a[i] << 28) | (a[i] >> 1);
        i = i + 1;
    }
    let mut j: usize = 0;
    while j < 4 {
        b[j] = (a[j] & 255_u32) as u8;
        j = j + 1;
    }
    let mut sum: u32 = 0;
    let mut k: usize = 0;
    while k < 8 {
        sum = sum.wrapping_add(a[k]);
        k = k + 1;
    }
    let mut bs: u32 = 0;
    let mut m: usize = 0;
    while m < 4 {
        bs = bs + (b[m] as u32);
        m = m + 1;
    }
    let big: u32 = 4000000000_u32;
    let mut c: u32 = 0;
    if big > 5_u32 { c = 1; }
    let wrapped: u8 = ((250_u32 + 10_u32) & 511_u32) as u8;
    println("sum {} bytes {} cmp {} div {} wrap {}", sum, bs, c, big / 3_u32, wrapped);
}
"#,
        "sum 1073741840 bytes 4 cmp 1 div 1333333333 wrap 4\n",
    ),

];

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn work_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bl-wasm-node-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn run_wasm(wasm: &Path) -> String {
    let out = Command::new("node")
        .arg("-e")
        .arg(RUNNER)
        .arg(wasm)
        .output()
        .expect("spawn node");
    assert!(
        out.status.success(),
        "node failed on {}: {}",
        wasm.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

/// stdout of `buildc run` (C backend), or None when no C toolchain works here.
fn run_c(src: &Path) -> Option<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg("run")
        .arg(src)
        .output()
        .ok()?;
    if !out.status.success() {
        eprintln!(
            "C backend run unavailable for {}: {}",
            src.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"))
}

#[test]
fn wasm_binary_runs_in_node_and_matches_c_backend() {
    if !node_available() {
        eprintln!("SKIP wasm_node: `node` is not on PATH; nothing was run");
        return;
    }
    let dir = work_dir();
    for (name, source, expected) in PROGRAMS {
        let src = dir.join(format!("{name}.bld"));
        let wasm = dir.join(format!("{name}.wasm"));
        fs::write(&src, source).expect("write source");
        let build = Command::new(env!("CARGO_BIN_EXE_buildc"))
            .arg(&src)
            .args(["--target", "wasm", "-o"])
            .arg(&wasm)
            .output()
            .expect("run buildc");
        assert!(
            build.status.success(),
            "{name}: buildc --target wasm failed: {}{}",
            String::from_utf8_lossy(&build.stdout),
            String::from_utf8_lossy(&build.stderr)
        );
        let bytes = fs::read(&wasm).expect("read wasm");
        assert_eq!(
            &bytes[..4],
            b"\0asm",
            "{name}: output is not a binary module"
        );
        let got = run_wasm(&wasm);
        assert_eq!(got, expected, "{name}: Node output");
        match run_c(&src) {
            Some(c_out) => assert_eq!(got, c_out, "{name}: WASM and C backends disagree"),
            None => eprintln!("{name}: C comparison skipped; checked against the literal"),
        }
    }
    let _ = fs::remove_dir_all(&dir);
}
