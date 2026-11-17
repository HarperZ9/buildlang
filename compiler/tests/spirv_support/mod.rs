// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2026 Zain Dana Harper.
//
//! Helpers for the SPIR-V compute tests: run buildc, read the emitted words,
//! and validate and disassemble with spirv-tools where they are installed.

#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub fn buildc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_buildc"))
}

pub fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .join("examples")
        .join("gpu")
        .join(name)
}

pub fn tools_required() -> bool {
    std::env::var("BUILDLANG_SHADER_TOOLS_REQUIRE").as_deref() == Ok("1")
}

/// `program` when it runs, else `None` after a skip line; a panic instead of
/// the skip when the tools are required.
pub fn tool_or_skip(test: &str, program: &'static str) -> Option<&'static str> {
    let runs = Command::new(program)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if runs {
        return Some(program);
    }
    assert!(
        !tools_required(),
        "{test}: {program} not on PATH, and BUILDLANG_SHADER_TOOLS_REQUIRE is 1"
    );
    eprintln!("skipping the {program} part of {test}: {program} not on PATH");
    None
}

pub fn temp_path(label: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "buildlang_compute_entries_{}_{}.{ext}",
        label,
        std::process::id()
    ))
}

/// Write `source` to a temp `.bld` file named after `label`.
pub fn write_source(label: &str, source: &str) -> PathBuf {
    let path = temp_path(label, "bld");
    std::fs::write(&path, source).expect("write test source");
    path
}

/// Run `buildc SRC --target spirv -o OUT [extra..]`.
pub fn compile_spirv(src: &Path, out: &Path, extra: &[&str]) -> Output {
    buildc()
        .arg(src)
        .arg("--target")
        .arg("spirv")
        .arg("-o")
        .arg(out)
        .args(extra)
        .output()
        .expect("run buildc")
}

pub fn text(o: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Compile and return the module words; panics with diagnostics on failure.
pub fn spirv_words(label: &str, src: &Path, extra: &[&str]) -> (PathBuf, Vec<u32>) {
    let out = temp_path(label, "spv");
    let o = compile_spirv(src, &out, extra);
    assert!(
        o.status.success(),
        "buildc should compile {}\n{}",
        src.display(),
        text(&o)
    );
    let bytes = std::fs::read(&out).expect("read emitted module");
    assert!(
        bytes.len() >= 20 && bytes.len() % 4 == 0,
        "not a SPIR-V module"
    );
    let words = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    (out, words)
}

const OP_ENTRY_POINT: u32 = 15;
const OP_EXECUTION_MODE: u32 = 16;
const MODE_LOCAL_SIZE: u32 = 17;
const MODEL_GL_COMPUTE: u32 = 5;

/// Each GLCompute entry point's name with its LocalSize, read from the words.
pub fn local_sizes(words: &[u32]) -> BTreeMap<String, (u32, u32, u32)> {
    let mut names: BTreeMap<u32, String> = BTreeMap::new();
    let mut sizes: BTreeMap<u32, (u32, u32, u32)> = BTreeMap::new();
    let mut i = 5usize;
    while i < words.len() {
        let count = (words[i] >> 16) as usize;
        let op = words[i] & 0xFFFF;
        assert!(
            count > 0 && i + count <= words.len(),
            "malformed instruction stream"
        );
        let ins = &words[i..i + count];
        if op == OP_ENTRY_POINT && ins[1] == MODEL_GL_COMPUTE {
            let mut bytes = Vec::new();
            for w in &ins[3..] {
                bytes.extend_from_slice(&w.to_le_bytes());
                if bytes.contains(&0) {
                    break;
                }
            }
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            names.insert(ins[2], String::from_utf8_lossy(&bytes[..end]).into_owned());
        }
        if op == OP_EXECUTION_MODE && ins[2] == MODE_LOCAL_SIZE {
            sizes.insert(ins[1], (ins[3], ins[4], ins[5]));
        }
        i += count;
    }
    names
        .into_iter()
        .map(|(id, name)| {
            let size = *sizes
                .get(&id)
                .unwrap_or_else(|| panic!("entry `{name}` has no LocalSize"));
            (name, size)
        })
        .collect()
}

/// spirv-val accepts the module and spirv-dis prints each `lines` entry.
pub fn validate_and_disassemble(test: &str, spv: &Path, lines: &[&str]) {
    if let Some(val) = tool_or_skip(test, "spirv-val") {
        let o = Command::new(val)
            .arg("--target-env")
            .arg("vulkan1.0")
            .arg(spv)
            .output()
            .expect("run spirv-val");
        assert!(
            o.status.success(),
            "{test}: spirv-val refused the module\n{}",
            text(&o)
        );
    }
    if let Some(dis) = tool_or_skip(test, "spirv-dis") {
        let o = Command::new(dis).arg(spv).output().expect("run spirv-dis");
        assert!(o.status.success(), "{test}: spirv-dis failed\n{}", text(&o));
        let asm = String::from_utf8_lossy(&o.stdout);
        for line in lines {
            assert!(
                asm.lines().any(|l| l.trim() == *line),
                "{test}: disassembly lacks `{line}`\n{asm}"
            );
        }
    }
}
