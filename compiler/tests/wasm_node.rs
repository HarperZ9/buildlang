// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The WASM backend's binary output, run in Node's built-in `WebAssembly`,
//! must print what the C backend prints for the same program.
//! Skips with a message when `node` is not on PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUNNER: &str = r#"
const { load } = require(process.argv[1]);
const fs = require('fs');
let out = '';
load(fs.readFileSync(process.argv[2]), { write: (s) => { out += s; } })
  .then((m) => { m.main(); process.stdout.write(out); })
  .catch((e) => { console.error(String(e)); process.exit(1); });
"#;

/// The page loader under test; the Node runs go through it.
const LOADER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/wasm/loader.js");

const PROGRAMS: [(&str, &str, &str); 5] = [
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

    (
        "vec_u8_u32_shared",
        r#"fn fill(n: usize) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    let mut i: usize = 0;
    while i < n {
        v.push((i % 251) as u8);
        i = i + 1;
    }
    v
}

fn main() ~ Console {
    let bytes: Vec<u8> = fill(1000);
    let mut words: Vec<u32> = Vec::new();
    let mut i: usize = 0;
    while i < 100 {
        words.push((i as u32) * 3000000_u32);
        i = i + 1;
    }
    let mut bsum: u64 = 0;
    let mut k: usize = 0;
    while k < vec_len(bytes) {
        bsum = bsum + (bytes[k] as u64);
        k = k + 1;
    }
    let mut wsum: u32 = 0;
    let mut j: usize = 0;
    while j < vec_len(words) {
        wsum = wsum.wrapping_add(words[j]);
        j = j + 1;
    }
    let last: u32 = vec_pop(words);
    let alias = words;
    alias.push(7_u32);
    println("bytes {} {} words {} {} last {} after {}", vec_len(bytes), bsum, vec_len(words), wsum, last, words[99]);
}
"#,
        "bytes 1000 124506 words 100 1965098112 last 297000000 after 7
",
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
        .arg(LOADER)
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

/// The registry's sha256 package, unmodified, compiled to wasm and run in Node.
#[test]
fn sha256_package_digests_match_c_backend_in_node() {
    if !node_available() {
        eprintln!("SKIP wasm_node: `node` is not on PATH; nothing was run");
        return;
    }
    let package = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../registry/packages/sha256/1.0.0/src/lib.bld"
    ));
    let source = format!(
        "{package}\nfn main() ~ Console {{\n    println(\"{{}}\", sha256_hex(\"abc\"));\n    println(\"{{}}\", sha256_hex(\"\"));\n}}\n"
    );
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n\
                    e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n";
    let dir = std::env::temp_dir().join(format!("bl-wasm-sha256-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create sha256 dir");
    let src = dir.join("sha256_main.bld");
    let wasm = dir.join("sha256_main.wasm");
    fs::write(&src, source).expect("write source");
    let build = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg(&src)
        .args(["--target", "wasm", "-o"])
        .arg(&wasm)
        .output()
        .expect("run buildc");
    assert!(
        build.status.success(),
        "sha256: buildc --target wasm failed: {}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let got = run_wasm(&wasm);
    assert_eq!(got, expected, "sha256: Node output");
    match run_c(&src) {
        Some(c_out) => assert_eq!(got, c_out, "sha256: WASM and C backends disagree"),
        None => eprintln!("sha256: C comparison skipped; checked against the literal"),
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The sha256 package built as a library: the host calls `sha256_hex` with
/// strings it supplies at run time and checks each digest against Node's crypto.
#[test]
fn sha256_library_exports_called_from_node_match_node_crypto() {
    if !node_available() {
        eprintln!("SKIP wasm_node: `node` is not on PATH; nothing was run");
        return;
    }
    let host = r#"
const { load } = require(process.argv[1]);
const fs = require('fs');
const crypto = require('crypto');
let big = '';
for (let i = 0; i < 1000; i++) big += String.fromCharCode(97 + ((i * 7) % 26));
const inputs = ['abc', 'The quick brown fox jumps over the lazy dog', big];
load(fs.readFileSync(process.argv[2]), { write: () => {} }).then((m) => {
  for (const s of inputs) {
    const got = m.readStr(m.exports.sha256_hex(m.str(s)));
    const want = crypto.createHash('sha256').update(s).digest('hex');
    console.log(s.length + ' ' + got + ' ' + want);
  }
}).catch((e) => { console.error(String(e)); process.exit(1); });
"#;
    let dir = std::env::temp_dir().join(format!("bl-wasm-shalib-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create dir");
    let wasm = dir.join("sha256.wasm");
    let lib = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../registry/packages/sha256/1.0.0/src/lib.bld"
    );
    let build = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg(lib)
        .args(["--target", "wasm", "-o"])
        .arg(&wasm)
        .output()
        .expect("run buildc");
    assert!(
        build.status.success(),
        "sha256 library build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let out = Command::new("node")
        .arg("-e")
        .arg(host)
        .arg(LOADER)
        .arg(&wasm)
        .output()
        .expect("spawn node");
    assert!(
        out.status.success(),
        "node: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "three digests expected, got {text}");
    for (line, len) in lines.iter().zip(["3", "43", "1000"]) {
        let parts: Vec<&str> = line.split(' ').collect();
        assert_eq!(parts.len(), 3, "{line}");
        assert_eq!(parts[0], len, "{line}");
        assert_eq!(parts[1].len(), 64, "{line}");
        assert_eq!(
            parts[1], parts[2],
            "wasm digest differs from Node crypto: {line}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The merkle package (which `use`s sha256) built as one library module; the
/// host builds leaf vectors and compares `merkle_root_hex` with an RFC 6962
/// root computed by Node's crypto.
#[test]
fn merkle_package_root_matches_rfc6962_reference_in_node() {
    if !node_available() {
        eprintln!("SKIP wasm_node: `node` is not on PATH; nothing was run");
        return;
    }
    let host = r#"
const { load } = require(process.argv[1]);
const fs = require('fs');
const crypto = require('crypto');
const h = (...parts) => crypto.createHash('sha256').update(Buffer.concat(parts)).digest();
const mth = (ls) => {
  if (ls.length === 1) return h(Buffer.from([0]), ls[0]);
  let k = 1;
  while (k * 2 < ls.length) k *= 2;
  return h(Buffer.from([1]), mth(ls.slice(0, k)), mth(ls.slice(k)));
};
const sets = [1, 2, 3, 7, 16].map((n) =>
  Array.from({ length: n }, (_, i) => Buffer.from('leaf-' + i + '-' + 'x'.repeat(i))));
load(fs.readFileSync(process.argv[2]), { write: () => {} }).then((m) => {
  for (const leaves of sets) {
    const handles = leaves.map((b) => m.bytes(b));
    const got = m.readStr(m.exports.merkle_root_hex(m.handles(handles)));
    console.log(leaves.length + ' ' + got + ' ' + mth(leaves).toString('hex'));
  }
}).catch((e) => { console.error(String(e)); process.exit(1); });
"#;
    let dir = std::env::temp_dir().join(format!("bl-wasm-merkle-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create dir");
    let wasm = dir.join("merkle.wasm");
    let lib = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../registry/packages/merkle/1.0.0/src/lib.bld"
    );
    let build = Command::new(env!("CARGO_BIN_EXE_buildc"))
        .arg(lib)
        .args(["--target", "wasm", "-o"])
        .arg(&wasm)
        .output()
        .expect("run buildc");
    assert!(
        build.status.success(),
        "merkle library build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let out = Command::new("node")
        .arg("-e")
        .arg(host)
        .arg(LOADER)
        .arg(&wasm)
        .output()
        .expect("spawn node");
    assert!(
        out.status.success(),
        "node: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5, "five roots expected, got {text}");
    // The C backend prints this root for the same three leaves (buildc run, 2026-11-13).
    assert!(lines[2]
        .starts_with("3 sha256:6268ab34683bcdf98c013555b86bfa156fe3e03a3020aeadb7fc69d36f660290 "));
    for (line, n) in lines.iter().zip(["1", "2", "3", "7", "16"]) {
        let parts: Vec<&str> = line.split(' ').collect();
        assert_eq!(parts.len(), 3, "{line}");
        assert_eq!(parts[0], n, "{line}");
        assert_eq!(
            parts[1],
            format!("sha256:{}", parts[2]),
            "wasm root differs from the RFC 6962 reference: {line}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
