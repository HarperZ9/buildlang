// ===============================================================================
// BUILDLANG CODE GENERATOR - WASM BINARY ENCODER
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Binary encoder for the flat WAT instruction subset that `wasm.rs` emits.
//!
//! The WASM backend keeps one instruction selector, which writes flat text
//! (one instruction per line, `block`/`loop`/`if`/`else`/`end` unfolded).
//! This module assembles those lines into the binary format, so the text
//! and the binary come from the same selection. An instruction the encoder
//! does not know is an error, never a silent skip.

use std::collections::HashMap;

/// One function signature: parameter and result value types.
pub type FuncType = (Vec<u8>, Vec<u8>);

/// A module ready for binary encoding.
#[derive(Default)]
pub struct ModuleSpec {
    pub types: Vec<FuncType>,
    /// (module, field, type index)
    pub imports: Vec<(String, String, u32)>,
    /// Type index of each defined function, in function-index order.
    pub funcs: Vec<u32>,
    /// (export name, function index)
    pub exports: Vec<(String, u32)>,
    pub memory_pages: u32,
    /// (value type, mutable, i32 initial value) globals.
    pub globals: Vec<(u8, bool, i64)>,
    /// (offset, bytes) active data segments in memory 0.
    pub data: Vec<(u32, Vec<u8>)>,
    /// Encoded code-section entries, one per defined function.
    pub bodies: Vec<Vec<u8>>,
}

pub fn valtype(name: &str) -> Result<u8, String> {
    match name {
        "i32" => Ok(0x7f),
        "i64" => Ok(0x7e),
        "f32" => Ok(0x7d),
        "f64" => Ok(0x7c),
        other => Err(format!("value type `{other}` has no binary encoding here")),
    }
}

pub fn uleb(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub fn sleb(out: &mut Vec<u8>, mut v: i64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        let done = (v == 0 && byte & 0x40 == 0) || (v == -1 && byte & 0x40 != 0);
        if done {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn name(out: &mut Vec<u8>, s: &str) {
    uleb(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn section(out: &mut Vec<u8>, id: u8, body: Vec<u8>) {
    out.push(id);
    uleb(out, body.len() as u64);
    out.extend(body);
}

/// Encode a whole module.
pub fn encode_module(m: &ModuleSpec) -> Vec<u8> {
    let mut out = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut b = Vec::new();
    uleb(&mut b, m.types.len() as u64);
    for (params, results) in &m.types {
        b.push(0x60);
        uleb(&mut b, params.len() as u64);
        b.extend(params);
        uleb(&mut b, results.len() as u64);
        b.extend(results);
    }
    section(&mut out, 1, b);
    let mut b = Vec::new();
    uleb(&mut b, m.imports.len() as u64);
    for (module, field, ty) in &m.imports {
        name(&mut b, module);
        name(&mut b, field);
        b.push(0x00);
        uleb(&mut b, *ty as u64);
    }
    section(&mut out, 2, b);
    let mut b = Vec::new();
    uleb(&mut b, m.funcs.len() as u64);
    for ty in &m.funcs {
        uleb(&mut b, *ty as u64);
    }
    section(&mut out, 3, b);
    let mut b = vec![0x01, 0x00];
    uleb(&mut b, m.memory_pages as u64);
    section(&mut out, 5, b);
    let mut b = Vec::new();
    uleb(&mut b, m.globals.len() as u64);
    for (ty, mutable, init) in &m.globals {
        b.push(*ty);
        b.push(*mutable as u8);
        b.push(0x41);
        sleb(&mut b, *init);
        b.push(0x0b);
    }
    section(&mut out, 6, b);
    let mut b = Vec::new();
    uleb(&mut b, m.exports.len() as u64 + 1);
    name(&mut b, "memory");
    b.extend([0x02, 0x00]);
    for (ename, idx) in &m.exports {
        name(&mut b, ename);
        b.push(0x00);
        uleb(&mut b, *idx as u64);
    }
    section(&mut out, 7, b);
    let mut b = Vec::new();
    uleb(&mut b, m.bodies.len() as u64);
    for body in &m.bodies {
        uleb(&mut b, body.len() as u64);
        b.extend(body);
    }
    section(&mut out, 10, b);
    let mut b = Vec::new();
    uleb(&mut b, m.data.len() as u64);
    for (offset, bytes) in &m.data {
        b.push(0x00);
        b.push(0x41);
        sleb(&mut b, *offset as i64);
        b.push(0x0b);
        uleb(&mut b, bytes.len() as u64);
        b.extend(bytes);
    }
    section(&mut out, 11, b);
    out
}

/// Opcode for an instruction that takes no immediate.
fn plain_opcode(op: &str) -> Option<u8> {
    const NAMES: [&str; 32] = [
        "eqz", "eq", "ne", "lt_s", "lt_u", "gt_s", "gt_u", "le_s", "le_u", "ge_s", "ge_u", "clz",
        "ctz", "popcnt", "add", "sub", "mul", "div_s", "div_u", "rem_s", "rem_u", "and", "or",
        "xor", "shl", "shr_s", "shr_u", "rotl", "rotr", "", "", "",
    ];
    let fixed = match op {
        "unreachable" => Some(0x00),
        "nop" => Some(0x01),
        "return" => Some(0x0f),
        "drop" => Some(0x1a),
        "select" => Some(0x1b),
        "i32.extend8_s" => Some(0xc0),
        "i32.extend16_s" => Some(0xc1),
        "f32.neg" => Some(0x8c),
        "f32.add" => Some(0x92),
        "f32.sub" => Some(0x93),
        "f32.mul" => Some(0x94),
        "f32.div" => Some(0x95),
        "f64.neg" => Some(0x9a),
        "f64.sqrt" => Some(0x9f),
        "f64.add" => Some(0xa0),
        "f64.sub" => Some(0xa1),
        "f64.mul" => Some(0xa2),
        "f64.div" => Some(0xa3),
        "f32.eq" => Some(0x5b),
        "f32.ne" => Some(0x5c),
        "f32.lt" => Some(0x5d),
        "f32.gt" => Some(0x5e),
        "f32.le" => Some(0x5f),
        "f32.ge" => Some(0x60),
        "f64.eq" => Some(0x61),
        "f64.ne" => Some(0x62),
        "f64.lt" => Some(0x63),
        "f64.gt" => Some(0x64),
        "f64.le" => Some(0x65),
        "f64.ge" => Some(0x66),
        "i32.wrap_i64" => Some(0xa7),
        "i64.extend_i32_s" => Some(0xac),
        "i64.extend_i32_u" => Some(0xad),
        "f32.demote_f64" => Some(0xb6),
        "f64.promote_f32" => Some(0xbb),
        "i32.reinterpret_f32" => Some(0xbc),
        "i64.reinterpret_f64" => Some(0xbd),
        "f32.reinterpret_i32" => Some(0xbe),
        "f64.reinterpret_i64" => Some(0xbf),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }
    // Integer families: i32 eqz..ge_u = 0x45.., clz..rotr = 0x67..;
    // i64 eqz..ge_u = 0x50.., clz..rotr = 0x79...
    let (ty, rest) = op.split_once('.')?;
    let pos = NAMES.iter().position(|n| *n == rest && !n.is_empty())? as u8;
    match (ty, pos) {
        ("i32", 0..=10) => Some(0x45 + pos),
        ("i32", _) => Some(0x67 + pos - 11),
        ("i64", 0..=10) => Some(0x50 + pos),
        ("i64", _) => Some(0x79 + pos - 11),
        _ => None,
    }
}

fn mem_opcode(op: &str) -> Option<(u8, u8)> {
    match op {
        "i32.load" => Some((0x28, 2)),
        "i64.load" => Some((0x29, 3)),
        "f32.load" => Some((0x2a, 2)),
        "f64.load" => Some((0x2b, 3)),
        "i32.store" => Some((0x36, 2)),
        "i64.store" => Some((0x37, 3)),
        "f32.store" => Some((0x38, 2)),
        "f64.store" => Some((0x39, 3)),
        "i32.load8_s" => Some((0x2c, 0)),
        "i32.load8_u" => Some((0x2d, 0)),
        "i32.load16_s" => Some((0x2e, 1)),
        "i32.load16_u" => Some((0x2f, 1)),
        "i32.store8" => Some((0x3a, 0)),
        "i32.store16" => Some((0x3b, 1)),
        _ => None,
    }
}

fn int_imm(tok: Option<&&str>, line: &str) -> Result<i128, String> {
    tok.and_then(|t| t.parse::<i128>().ok())
        .ok_or_else(|| format!("bad integer immediate in `{line}`"))
}

/// Assemble one function body (locals already given as value types).
pub fn assemble_body(
    text: &str,
    locals: &[u8],
    local_index: &HashMap<String, u32>,
    func_index: &HashMap<String, u32>,
    global_index: &HashMap<String, u32>,
) -> Result<Vec<u8>, String> {
    let mut code = Vec::new();
    uleb(&mut code, locals.len() as u64);
    for ty in locals {
        code.push(0x01);
        code.push(*ty);
    }
    let mut labels: Vec<Option<String>> = Vec::new();
    let depth = |labels: &Vec<Option<String>>, l: &str| -> Result<u64, String> {
        labels
            .iter()
            .rev()
            .position(|x| x.as_deref() == Some(l))
            .map(|d| d as u64)
            .ok_or_else(|| format!("branch to unknown label `{l}`"))
    };
    for raw in text.lines() {
        let line = raw.split(";;").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let toks: Vec<&str> = line.split_whitespace().collect();
        let op = toks[0];
        match op {
            "block" | "loop" => {
                code.push(if op == "block" { 0x02 } else { 0x03 });
                code.push(0x40);
                labels.push(toks.get(1).map(|s| s.to_string()));
            }
            "if" => {
                code.extend([0x04, 0x40]);
                labels.push(None);
            }
            "else" => code.push(0x05),
            "end" => {
                labels.pop();
                code.push(0x0b);
            }
            "br" | "br_if" => {
                code.push(if op == "br" { 0x0c } else { 0x0d });
                let l = toks.get(1).ok_or("br without label")?;
                uleb(&mut code, depth(&labels, l)?);
            }
            "br_table" => {
                code.push(0x0e);
                uleb(&mut code, toks.len() as u64 - 2);
                for l in &toks[1..] {
                    uleb(&mut code, depth(&labels, l)?);
                }
            }
            "local.get" | "local.set" | "local.tee" => {
                code.push(match op {
                    "local.get" => 0x20,
                    "local.set" => 0x21,
                    _ => 0x22,
                });
                let n = toks.get(1).ok_or("local op without operand")?;
                let idx = local_index
                    .get(*n)
                    .ok_or_else(|| format!("unknown local `{n}`"))?;
                uleb(&mut code, *idx as u64);
            }
            "global.get" | "global.set" => {
                code.push(if op == "global.get" { 0x23 } else { 0x24 });
                let n = toks.get(1).ok_or("global op without operand")?;
                let idx = global_index
                    .get(*n)
                    .ok_or_else(|| format!("unknown global `{n}`"))?;
                uleb(&mut code, *idx as u64);
            }
            "call" => {
                let n = toks.get(1).ok_or("call without callee")?;
                let idx = func_index
                    .get(n.trim_start_matches('$'))
                    .ok_or_else(|| format!("call to unknown function `{n}`"))?;
                code.push(0x10);
                uleb(&mut code, *idx as u64);
            }
            "i32.const" => {
                code.push(0x41);
                sleb(&mut code, int_imm(toks.get(1), line)? as u32 as i32 as i64);
            }
            "i64.const" => {
                code.push(0x42);
                sleb(&mut code, int_imm(toks.get(1), line)? as u64 as i64);
            }
            "f32.const" | "f64.const" => {
                let v: f64 = toks
                    .get(1)
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| format!("bad float immediate in `{line}`"))?;
                if op == "f32.const" {
                    code.push(0x43);
                    code.extend((v as f32).to_le_bytes());
                } else {
                    code.push(0x44);
                    code.extend(v.to_le_bytes());
                }
            }
            _ => {
                if let Some((opc, align)) = mem_opcode(op) {
                    code.extend([opc, align, 0x00]);
                } else if let Some(opc) = plain_opcode(op) {
                    code.push(opc);
                } else {
                    return Err(format!("no binary encoding for `{line}`"));
                }
            }
        }
    }
    code.push(0x0b);
    Ok(code)
}
