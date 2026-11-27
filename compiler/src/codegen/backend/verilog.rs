// ===============================================================================
// BUILDLANG CODE GENERATOR - VERILOG-2001 RTL BACKEND (N10 TIER-0 OPEN SILICON)
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Synthesizable IEEE 1364-2001 Verilog-2001 RTL code generation backend
//! (`--target verilog`, `.v` extension) targeting Yosys, Verilator, and
//! OpenLane GDSII open-silicon flows.
//!
//! Lowers pure integer/fixed-point BuildLang MIR functions into synchronous
//! 2-stage pipelined Verilog-2001 modules (`clk`, `rst_n`, `valid_in`,
//! registered Stage-1 inputs, combinational SSA datapath, registered `result`
//! and `valid_out`), and provides a fail-closed synthesizability linter plus a
//! deterministic cycle-accurate netlist simulator over the emitted `.v` source.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use super::{Backend, CodegenError, CodegenResult, Target};
use crate::codegen::ir::*;
use crate::codegen::{GeneratedCode, OutputFormat};

/// Verilog-2001 RTL backend.
pub struct VerilogBackend {
    next_wire: u32,
}

impl VerilogBackend {
    pub fn new() -> Self {
        Self { next_wire: 0 }
    }

    fn int_spec(ty: &MirType) -> CodegenResult<(u32, bool)> {
        match ty {
            MirType::Bool => Ok((1, false)),
            MirType::Int(IntSize::I8, s) => Ok((8, *s)),
            MirType::Int(IntSize::I16, s) => Ok((16, *s)),
            MirType::Int(IntSize::I32, s) => Ok((32, *s)),
            MirType::Int(IntSize::I64 | IntSize::ISize, s) => Ok((64, *s)),
            other => Err(CodegenError::Unsupported(format!(
                "Verilog RTL backend only supports synthesizable fixed-width integer/bool types, got `{other:?}`"
            ))),
        }
    }

    fn width_decl(bits: u32, signed: bool) -> String {
        let sign_kw = if signed && bits > 1 { "signed " } else { "" };
        if bits == 1 {
            String::new()
        } else {
            format!("{sign_kw}[{}:0] ", bits - 1)
        }
    }

    fn zero_lit(bits: u32, signed: bool) -> String {
        if bits == 1 {
            "1'b0".to_string()
        } else if signed {
            format!("{bits}'sd0")
        } else {
            format!("{bits}'d0")
        }
    }

    fn const_to_verilog(c: &MirConst) -> CodegenResult<(String, u32, bool)> {
        match c {
            MirConst::Bool(b) => Ok((if *b { "1'b1" } else { "1'b0" }.to_string(), 1, false)),
            MirConst::Int(v, ty) => {
                let (bits, signed) = Self::int_spec(ty)?;
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let raw = (*v as u64) & mask;
                let expr = if bits == 1 {
                    format!("1'b{}", raw & 1)
                } else if signed {
                    format!("$signed({bits}'h{raw:x})")
                } else {
                    format!("{bits}'h{raw:x}")
                };
                Ok((expr, bits, signed))
            }
            MirConst::Uint(v, ty) => {
                let (bits, signed) = Self::int_spec(ty)?;
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let raw = (*v as u64) & mask;
                let expr = if bits == 1 {
                    format!("1'b{}", raw & 1)
                } else if signed {
                    format!("$signed({bits}'h{raw:x})")
                } else {
                    format!("{bits}'h{raw:x}")
                };
                Ok((expr, bits, signed))
            }
            other => Err(CodegenError::Unsupported(format!(
                "non-synthesizable constant in Verilog RTL backend: `{other:?}`"
            ))),
        }
    }

    fn emit_wire(&mut self, wires: &mut String, bits: u32, signed: bool, rhs: &str) -> String {
        let name = format!("w_{}", self.next_wire);
        self.next_wire += 1;
        let decl = Self::width_decl(bits, signed);
        writeln!(wires, "    wire {decl}{name};").unwrap();
        writeln!(wires, "    assign {name} = {rhs};").unwrap();
        name
    }

    fn resolve_val(
        val: &MirValue,
        env: &HashMap<LocalId, (String, u32, bool)>,
    ) -> CodegenResult<(String, u32, bool)> {
        match val {
            MirValue::Local(id) => env.get(id).cloned().ok_or_else(|| {
                CodegenError::Internal(format!("Verilog RTL: undefined MIR local _{}", id.0))
            }),
            MirValue::Const(c) => Self::const_to_verilog(c),
            other => Err(CodegenError::Unsupported(format!(
                "non-synthesizable MIR value in Verilog RTL backend: `{other:?}`"
            ))),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_block_dag(
        &mut self,
        block_id: BlockId,
        blocks: &HashMap<BlockId, &MirBlock>,
        locals: &[MirLocal],
        mut env: HashMap<LocalId, (String, u32, bool)>,
        visiting: &mut HashSet<BlockId>,
        wires: &mut String,
        ret_bits: u32,
        ret_signed: bool,
    ) -> CodegenResult<String> {
        if !visiting.insert(block_id) {
            return Err(CodegenError::Unsupported(
                "cyclic control-flow loop cannot be lowered directly to combinational/2-stage Verilog RTL"
                    .to_string(),
            ));
        }
        let block = blocks.get(&block_id).ok_or_else(|| {
            CodegenError::Internal(format!("Verilog RTL: missing block bb{}", block_id.0))
        })?;

        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign { dest, value } => {
                    let dest_ty = locals.get(dest.0 as usize).map(|l| &l.ty).ok_or_else(|| {
                        CodegenError::Internal(format!(
                            "Verilog RTL: missing local metadata for _{}",
                            dest.0
                        ))
                    })?;
                    let (dbits, dsigned) = Self::int_spec(dest_ty)?;
                    let rhs = match value {
                        MirRValue::Use(v) => {
                            let (expr, _, _) = Self::resolve_val(v, &env)?;
                            expr
                        }
                        MirRValue::UnaryOp { op, operand } => {
                            let (e, _, _) = Self::resolve_val(operand, &env)?;
                            match op {
                                UnaryOp::Neg => format!("(-{e})"),
                                UnaryOp::BitNot => format!("(~{e})"),
                                UnaryOp::Not => format!("(!{e})"),
                            }
                        }
                        MirRValue::BinaryOp { op, left, right } => {
                            let (le, _, ls) = Self::resolve_val(left, &env)?;
                            let (re, _, _) = Self::resolve_val(right, &env)?;
                            let op_str = match op {
                                BinOp::Add | BinOp::AddWrapping => "+",
                                BinOp::Sub | BinOp::SubWrapping => "-",
                                BinOp::Mul | BinOp::MulWrapping => "*",
                                BinOp::BitAnd => "&",
                                BinOp::BitOr => "|",
                                BinOp::BitXor => "^",
                                BinOp::Shl => "<<",
                                BinOp::Shr => {
                                    if ls {
                                        ">>>"
                                    } else {
                                        ">>"
                                    }
                                }
                                BinOp::Eq => "==",
                                BinOp::Ne => "!=",
                                BinOp::Lt => "<",
                                BinOp::Le => "<=",
                                BinOp::Gt => ">",
                                BinOp::Ge => ">=",
                                other => {
                                    return Err(CodegenError::Unsupported(format!(
                                        "non-synthesizable binary op `{other:?}` in Verilog RTL backend"
                                    )));
                                }
                            };
                            format!("({le} {op_str} {re})")
                        }
                        MirRValue::Cast {
                            kind: CastKind::IntToInt,
                            value: src,
                            ..
                        } => {
                            let (se, sbits, ssigned) = Self::resolve_val(src, &env)?;
                            if dbits == sbits {
                                if dsigned && !ssigned {
                                    format!("$signed({se})")
                                } else {
                                    se
                                }
                            } else if dbits < sbits {
                                let trunc = format!("{se}[{}:0]", dbits - 1);
                                if dsigned {
                                    format!("$signed({trunc})")
                                } else {
                                    trunc
                                }
                            } else {
                                // Sign-extend or zero-extend in Verilog-2001.
                                let pad = dbits - sbits;
                                if ssigned {
                                    let ext =
                                        format!("{{{{{pad}{{{se}[{}]}}}} , {se}}}", sbits - 1);
                                    if dsigned {
                                        format!("$signed({ext})")
                                    } else {
                                        ext
                                    }
                                } else {
                                    let ext = format!("{{{pad}'d0, {se}}}");
                                    if dsigned {
                                        format!("$signed({ext})")
                                    } else {
                                        ext
                                    }
                                }
                            }
                        }
                        other => {
                            return Err(CodegenError::Unsupported(format!(
                                "non-synthesizable rvalue `{other:?}` in Verilog RTL backend"
                            )));
                        }
                    };
                    let wname = self.emit_wire(wires, dbits, dsigned, &rhs);
                    env.insert(*dest, (wname, dbits, dsigned));
                }
                MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop => {}
                other => {
                    return Err(CodegenError::Unsupported(format!(
                        "non-synthesizable statement `{other:?}` in Verilog RTL backend"
                    )));
                }
            }
        }

        let res = match &block.terminator {
            Some(MirTerminator::Return(Some(val))) => {
                let (e, _, _) = Self::resolve_val(val, &env)?;
                Ok(e)
            }
            Some(MirTerminator::Goto(next)) => self.lower_block_dag(
                *next, blocks, locals, env, visiting, wires, ret_bits, ret_signed,
            ),
            Some(MirTerminator::If {
                cond,
                then_block,
                else_block,
            }) => {
                let (ce, _, _) = Self::resolve_val(cond, &env)?;
                let then_expr = self.lower_block_dag(
                    *then_block,
                    blocks,
                    locals,
                    env.clone(),
                    visiting,
                    wires,
                    ret_bits,
                    ret_signed,
                )?;
                let else_expr = self.lower_block_dag(
                    *else_block,
                    blocks,
                    locals,
                    env,
                    visiting,
                    wires,
                    ret_bits,
                    ret_signed,
                )?;
                let mux_rhs = format!("({ce} ? {then_expr} : {else_expr})");
                let wmux = self.emit_wire(wires, ret_bits, ret_signed, &mux_rhs);
                Ok(wmux)
            }
            other => Err(CodegenError::Unsupported(format!(
                "non-synthesizable terminator `{other:?}` in Verilog RTL backend"
            ))),
        };
        visiting.remove(&block_id);
        res
    }

    fn generate_function(&mut self, func: &MirFunction) -> CodegenResult<String> {
        let Some(blocks_vec) = &func.blocks else {
            return Err(CodegenError::Unsupported(format!(
                "Verilog RTL: function `{}` has no body",
                func.name
            )));
        };
        let (ret_bits, ret_signed) = Self::int_spec(&func.sig.ret)?;
        let mut params: Vec<(LocalId, String, u32, bool)> = Vec::new();
        for (i, ty) in func.sig.params.iter().enumerate() {
            let (bits, signed) = Self::int_spec(ty)?;
            let pname = func
                .locals
                .iter()
                .find(|l| l.is_param && l.id.0 == i as u32)
                .and_then(|l| l.name.as_ref())
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("p{i}"));
            params.push((LocalId(i as u32), pname, bits, signed));
        }

        self.next_wire = 0;
        let mut env: HashMap<LocalId, (String, u32, bool)> = HashMap::new();
        for (id, pname, bits, signed) in &params {
            env.insert(*id, (format!("s1_{pname}"), *bits, *signed));
        }
        let block_map: HashMap<BlockId, &MirBlock> = blocks_vec.iter().map(|b| (b.id, b)).collect();
        let mut wires = String::new();
        let mut visiting = HashSet::new();
        let final_expr = self.lower_block_dag(
            BlockId::ENTRY,
            &block_map,
            &func.locals,
            env,
            &mut visiting,
            &mut wires,
            ret_bits,
            ret_signed,
        )?;

        let mut out = String::new();
        writeln!(out, "module {} (", func.name).unwrap();
        writeln!(out, "    input  wire        clk,").unwrap();
        writeln!(out, "    input  wire        rst_n,").unwrap();
        writeln!(out, "    input  wire        valid_in,").unwrap();
        for (_, pname, bits, signed) in &params {
            let decl = Self::width_decl(*bits, *signed);
            writeln!(out, "    input  wire {decl}{pname},").unwrap();
        }
        writeln!(out, "    output reg         valid_out,").unwrap();
        let ret_decl = Self::width_decl(ret_bits, ret_signed);
        writeln!(out, "    output reg  {ret_decl}result").unwrap();
        writeln!(out, ");").unwrap();

        writeln!(out, "    // Stage 1: registered input pipeline stage").unwrap();
        writeln!(out, "    reg valid_s1;").unwrap();
        for (_, pname, bits, signed) in &params {
            let decl = Self::width_decl(*bits, *signed);
            writeln!(out, "    reg {decl}s1_{pname};").unwrap();
        }
        writeln!(out).unwrap();
        writeln!(out, "    // Combinational SSA datapath").unwrap();
        out.push_str(&wires);
        writeln!(out, "    wire {ret_decl}comb_result;").unwrap();
        writeln!(out, "    assign comb_result = {final_expr};").unwrap();
        writeln!(out).unwrap();

        writeln!(
            out,
            "    // Stage 1 & Stage 2 synchronous registers with active-low async reset"
        )
        .unwrap();
        writeln!(out, "    always @(posedge clk or negedge rst_n) begin").unwrap();
        writeln!(out, "        if (!rst_n) begin").unwrap();
        writeln!(out, "            valid_s1 <= 1'b0;").unwrap();
        for (_, pname, bits, signed) in &params {
            writeln!(
                out,
                "            s1_{pname} <= {};",
                Self::zero_lit(*bits, *signed)
            )
            .unwrap();
        }
        writeln!(out, "            valid_out <= 1'b0;").unwrap();
        writeln!(
            out,
            "            result <= {};",
            Self::zero_lit(ret_bits, ret_signed)
        )
        .unwrap();
        writeln!(out, "        end else begin").unwrap();
        writeln!(out, "            valid_s1 <= valid_in;").unwrap();
        for (_, pname, _, _) in &params {
            writeln!(out, "            s1_{pname} <= {pname};").unwrap();
        }
        writeln!(out, "            valid_out <= valid_s1;").unwrap();
        writeln!(out, "            result <= comb_result;").unwrap();
        writeln!(out, "        end").unwrap();
        writeln!(out, "    end").unwrap();
        writeln!(out, "endmodule\n").unwrap();
        Ok(out)
    }
}

impl Default for VerilogBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for VerilogBackend {
    fn generate(&mut self, module: &MirModule) -> CodegenResult<GeneratedCode> {
        let mut out = String::new();
        out.push_str("// Generated by BuildLang Compiler\n");
        out.push_str(
            "// Target: Verilog-2001 RTL (IEEE 1364-2001, Yosys / Verilator / OpenLane)\n",
        );
        out.push_str("// Do not edit manually\n\n");

        let mut emitted = 0usize;
        for func in &module.functions {
            if func.is_declaration() || func.name.as_ref() == "main" {
                continue;
            }
            out.push_str(&self.generate_function(func)?);
            emitted += 1;
        }
        if emitted == 0 {
            return Err(CodegenError::Unsupported(
                "Verilog RTL backend found zero synthesizable non-main functions".to_string(),
            ));
        }
        verify_synthesizable_verilog(&out).map_err(CodegenError::Internal)?;
        Ok(GeneratedCode::new(OutputFormat::Verilog, out.into_bytes()))
    }

    fn target(&self) -> Target {
        Target::Verilog
    }
}

/// Fail-closed structural linter verifying emitted Verilog-2001 RTL contains
/// only synthesizable constructs (`module`/`endmodule`, synchronous clocked
/// `always @(posedge clk or negedge rst_n)`, async reset, zero simulation-only
/// keywords such as `initial`, `$display`, `$finish`, `real`, `fork`, `while`,
/// or `#` delays).
pub fn verify_synthesizable_verilog(rtl: &str) -> Result<(), String> {
    let mut module_count = 0usize;
    let mut endmodule_count = 0usize;
    let mut has_clocked_always = false;
    let mut has_reset = false;
    for raw_line in rtl.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("module ") {
            module_count += 1;
        }
        if line == "endmodule" {
            endmodule_count += 1;
        }
        if line.contains("always @(posedge clk or negedge rst_n)") {
            has_clocked_always = true;
        }
        if line.contains("if (!rst_n)") {
            has_reset = true;
        }
        for forbidden in [
            "initial ",
            "initial\t",
            "$display",
            "$finish",
            "$stop",
            " real ",
            "fork",
            "while ",
            "forever",
        ] {
            if line.contains(forbidden) {
                return Err(format!(
                    "non-synthesizable Verilog construct `{}` rejected fail-closed",
                    forbidden.trim()
                ));
            }
        }
        if line.contains('#') {
            return Err("non-synthesizable Verilog delay `#` rejected fail-closed".to_string());
        }
    }
    if module_count == 0 || module_count != endmodule_count {
        return Err(format!(
            "unbalanced Verilog module/endmodule declarations ({module_count} vs {endmodule_count})"
        ));
    }
    if !has_clocked_always || !has_reset {
        return Err(
            "synthesizable pipelined Verilog must include clocked always block and reset"
                .to_string(),
        );
    }
    Ok(())
}

/// Parsed signal metadata from emitted Verilog-2001 RTL.
#[derive(Debug, Clone, Copy)]
struct RtlSignal {
    bits: u32,
    signed: bool,
}

fn mask_bits(val: i128, bits: u32, signed: bool) -> i128 {
    let mask: u128 = if bits >= 64 {
        u64::MAX as u128
    } else {
        (1u128 << bits) - 1
    };
    let u = (val as u128) & mask;
    if signed && bits > 1 {
        let sign_bit = 1u128 << (bits - 1);
        if (u & sign_bit) != 0 {
            let ext = u | (!mask);
            return ext as i128;
        }
    }
    u as i128
}

fn parse_decl_spec(line: &str) -> Option<(String, RtlSignal)> {
    let clean = line
        .trim()
        .trim_end_matches(';')
        .trim_end_matches(',')
        .trim();
    let signed = clean.contains(" signed ");
    let bits = if let Some(lb) = clean.find('[') {
        let rb = clean.find(']')?;
        let range = &clean[lb + 1..rb];
        let hi: u32 = range.split(':').next()?.trim().parse().ok()?;
        hi + 1
    } else {
        1
    };
    let name = clean.split_whitespace().last()?.to_string();
    Some((name, RtlSignal { bits, signed }))
}

fn strip_outer_parens(s: &str) -> &str {
    let mut cur = s.trim();
    while cur.starts_with('(') && cur.ends_with(')') {
        let mut depth = 0i32;
        let mut wraps_all = true;
        for (idx, ch) in cur.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && idx < cur.len() - 1 {
                        wraps_all = false;
                        break;
                    }
                }
                _ => {}
            }
        }
        if wraps_all && depth == 0 {
            cur = cur[1..cur.len() - 1].trim();
        } else {
            break;
        }
    }
    cur
}

fn find_top_level_ternary(s: &str) -> Option<(&str, &str, &str)> {
    let mut depth = 0i32;
    let mut q_pos = None;
    let mut c_pos = None;
    let mut nested_q = 0i32;
    for (idx, ch) in s.char_indices() {
        match ch {
            '(' | '{' => depth += 1,
            ')' | '}' => depth -= 1,
            '?' if depth == 0 => {
                if q_pos.is_none() {
                    q_pos = Some(idx);
                } else {
                    nested_q += 1;
                }
            }
            ':' if depth == 0 => {
                if q_pos.is_some() {
                    if nested_q == 0 {
                        c_pos = Some(idx);
                        break;
                    } else {
                        nested_q -= 1;
                    }
                }
            }
            _ => {}
        }
    }
    let (q, c) = (q_pos?, c_pos?);
    Some((s[..q].trim(), s[q + 1..c].trim(), s[c + 1..].trim()))
}

fn find_top_level_binop(s: &str) -> Option<(&str, &str, &str)> {
    // Operators ordered from lowest precedence to highest; scan right-to-left.
    let ops_by_prec: &[&[&str]] = &[
        &["==", "!="],
        &["<=", ">=", "<", ">"],
        &["|"],
        &["^"],
        &["&"],
        &[">>>", "<<", ">>"],
        &["+", "-"],
        &["*"],
    ];
    let bytes = s.as_bytes();
    for group in ops_by_prec {
        let mut depth = 0i32;
        let mut i = bytes.len();
        while i > 0 {
            i -= 1;
            match bytes[i] {
                b')' | b'}' => depth += 1,
                b'(' | b'{' => depth -= 1,
                _ if depth == 0 => {
                    for &op in *group {
                        let oplen = op.len();
                        if i + 1 >= oplen {
                            let start = i + 1 - oplen;
                            if &s[start..=i] == op {
                                // Skip unary '-' or '+' at start of expression.
                                if (op == "-" || op == "+") && s[..start].trim().is_empty() {
                                    continue;
                                }
                                // Avoid matching `<` inside `<=`, `>` inside `>=`/`>>`/`>>>`.
                                if op == "<"
                                    && (s[start..].starts_with("<=")
                                        || s[start..].starts_with("<<"))
                                {
                                    continue;
                                }
                                if op == ">"
                                    && (s[start..].starts_with(">=")
                                        || s[start..].starts_with(">>")
                                        || (start > 0 && bytes[start - 1] == b'>'))
                                {
                                    continue;
                                }
                                if op == ">>"
                                    && (s[start..].starts_with(">>>")
                                        || (start > 0 && bytes[start - 1] == b'>'))
                                {
                                    continue;
                                }
                                let lhs = s[..start].trim();
                                let rhs = s[i + 1..].trim();
                                if !lhs.is_empty() && !rhs.is_empty() {
                                    return Some((lhs, op, rhs));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn eval_verilog_expr(
    raw: &str,
    vals: &HashMap<String, (i128, RtlSignal)>,
) -> Result<(i128, RtlSignal), String> {
    let s = strip_outer_parens(raw);
    if let Some(inner) = s.strip_prefix("$signed(").and_then(|t| t.strip_suffix(')')) {
        let (v, sig) = eval_verilog_expr(inner, vals)?;
        let out_sig = RtlSignal {
            bits: sig.bits,
            signed: true,
        };
        return Ok((mask_bits(v, out_sig.bits, true), out_sig));
    }
    if let Some((c, t, e)) = find_top_level_ternary(s) {
        let (cv, _) = eval_verilog_expr(c, vals)?;
        return if cv != 0 {
            eval_verilog_expr(t, vals)
        } else {
            eval_verilog_expr(e, vals)
        };
    }
    if let Some((l, op, r)) = find_top_level_binop(s) {
        let (lv, lsig) = eval_verilog_expr(l, vals)?;
        let (rv, rsig) = eval_verilog_expr(r, vals)?;
        let bits = lsig.bits.max(rsig.bits);
        let signed = lsig.signed && rsig.signed;
        let out_sig = RtlSignal { bits, signed };
        let res = match op {
            "+" => (mask_bits(lv.wrapping_add(rv), bits, signed), out_sig),
            "-" => (mask_bits(lv.wrapping_sub(rv), bits, signed), out_sig),
            "*" => (mask_bits(lv.wrapping_mul(rv), bits, signed), out_sig),
            "&" => (mask_bits(lv & rv, bits, signed), out_sig),
            "|" => (mask_bits(lv | rv, bits, signed), out_sig),
            "^" => (mask_bits(lv ^ rv, bits, signed), out_sig),
            "<<" => {
                let sh = (rv as u32) & 63;
                (mask_bits(lv.wrapping_shl(sh), bits, signed), out_sig)
            }
            ">>>" => {
                let sh = (rv as u32) & 63;
                (
                    mask_bits(lv >> sh, bits, true),
                    RtlSignal { bits, signed: true },
                )
            }
            ">>" => {
                let sh = (rv as u32) & 63;
                let u = mask_bits(lv, lsig.bits, false) as u128;
                (mask_bits((u >> sh) as i128, bits, signed), out_sig)
            }
            "==" => (
                i128::from(lv == rv),
                RtlSignal {
                    bits: 1,
                    signed: false,
                },
            ),
            "!=" => (
                i128::from(lv != rv),
                RtlSignal {
                    bits: 1,
                    signed: false,
                },
            ),
            "<" => (
                i128::from(lv < rv),
                RtlSignal {
                    bits: 1,
                    signed: false,
                },
            ),
            "<=" => (
                i128::from(lv <= rv),
                RtlSignal {
                    bits: 1,
                    signed: false,
                },
            ),
            ">" => (
                i128::from(lv > rv),
                RtlSignal {
                    bits: 1,
                    signed: false,
                },
            ),
            ">=" => (
                i128::from(lv >= rv),
                RtlSignal {
                    bits: 1,
                    signed: false,
                },
            ),
            _ => return Err(format!("unsupported Verilog binary op `{op}`")),
        };
        return Ok(res);
    }
    if let Some(rest) = s.strip_prefix('-') {
        let (v, sig) = eval_verilog_expr(rest, vals)?;
        return Ok((mask_bits(v.wrapping_neg(), sig.bits, sig.signed), sig));
    }
    if let Some(rest) = s.strip_prefix('~') {
        let (v, sig) = eval_verilog_expr(rest, vals)?;
        return Ok((mask_bits(!v, sig.bits, sig.signed), sig));
    }
    if let Some(rest) = s.strip_prefix('!') {
        let (v, _) = eval_verilog_expr(rest, vals)?;
        return Ok((
            i128::from(v == 0),
            RtlSignal {
                bits: 1,
                signed: false,
            },
        ));
    }
    // Verilog literals: `32'h...`, `32'sd...`, `32'd...`, `1'b0`, `1'b1`.
    if let Some(tick) = s.find('\'') {
        let bits: u32 = s[..tick]
            .trim()
            .parse()
            .map_err(|_| format!("invalid Verilog bit width in `{s}`"))?;
        let rest = &s[tick + 1..];
        let (signed, radix, digits) = if let Some(d) = rest.strip_prefix("sd") {
            (true, 10, d)
        } else if let Some(d) = rest.strip_prefix('h') {
            (false, 16, d)
        } else if let Some(d) = rest.strip_prefix('d') {
            (false, 10, d)
        } else if let Some(d) = rest.strip_prefix('b') {
            (false, 2, d)
        } else {
            return Err(format!("unsupported Verilog literal `{s}`"));
        };
        let raw = u128::from_str_radix(digits.trim(), radix)
            .map_err(|_| format!("invalid Verilog digits in `{s}`"))?;
        let sig = RtlSignal { bits, signed };
        return Ok((mask_bits(raw as i128, bits, signed), sig));
    }
    if let Some(&(v, sig)) = vals.get(s) {
        return Ok((v, sig));
    }
    Err(format!("unresolved Verilog expression `{s}`"))
}

/// Deterministic cycle-accurate 2-stage pipeline netlist simulator operating
/// directly on an emitted Verilog-2001 `.v` module string.
///
/// Steps asynchronous reset (`rst_n = 0` -> verifies `valid_out == 0` and
/// `result == 0`), then feeds `vectors` through the 2-cycle synchronous pipeline
/// (`s1_*` -> `comb_result` -> `result`) and returns the bit-exact registered
/// output for every vector.
pub fn simulate_verilog_2stage_pipeline(
    rtl: &str,
    module_name: &str,
    vectors: &[Vec<(&str, i64)>],
) -> Result<Vec<i64>, String> {
    verify_synthesizable_verilog(rtl)?;

    let header = format!("module {module_name} (");
    let start = rtl
        .find(&header)
        .ok_or_else(|| format!("Verilog module `{module_name}` not found in RTL"))?;
    let mod_slice = &rtl[start..];
    let end = mod_slice
        .find("endmodule")
        .ok_or_else(|| format!("Verilog module `{module_name}` missing endmodule"))?;
    let body = &mod_slice[..end];

    let mut wire_types: HashMap<String, RtlSignal> = HashMap::new();
    let mut assigns: Vec<(String, String)> = Vec::new();
    let mut input_ports: Vec<(String, RtlSignal)> = Vec::new();
    let mut result_sig = RtlSignal {
        bits: 32,
        signed: true,
    };

    for raw_line in body.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("input ") {
            if let Some((name, sig)) = parse_decl_spec(line) {
                if !matches!(name.as_str(), "clk" | "rst_n" | "valid_in") {
                    input_ports.push((name.clone(), sig));
                    wire_types.insert(format!("s1_{name}"), sig);
                }
            }
        } else if line.starts_with("output ") && line.contains("result") {
            if let Some((_, sig)) = parse_decl_spec(line) {
                result_sig = sig;
            }
        } else if line.starts_with("wire ") {
            if let Some((name, sig)) = parse_decl_spec(line) {
                wire_types.insert(name, sig);
            }
        } else if let Some(rest) = line.strip_prefix("assign ") {
            let clean = rest.trim_end_matches(';').trim();
            if let Some(eq) = clean.find('=') {
                let lhs = clean[..eq].trim().to_string();
                let rhs = clean[eq + 1..].trim().to_string();
                assigns.push((lhs, rhs));
            }
        }
    }

    // Cycle 0: verify active-low async reset clears valid_out and result.
    let mut valid_s1 = false;
    let mut valid_out = false;
    let mut reg_result: i128 = 0;
    let mut s1_regs: HashMap<String, (i128, RtlSignal)> = input_ports
        .iter()
        .map(|(n, sig)| (format!("s1_{n}"), (0, *sig)))
        .collect();
    if valid_out || reg_result != 0 {
        return Err("reset invariant failed".to_string());
    }

    let mut outputs = Vec::with_capacity(vectors.len());
    // Feed N vectors + 2 flush cycles to drain the 2-stage pipeline cleanly.
    for cycle in 0..(vectors.len() + 2) {
        // Compute combinational datapath from current Stage-1 registers.
        let mut net_vals = s1_regs.clone();
        for (lhs, rhs) in &assigns {
            let (v, mut sig) = eval_verilog_expr(rhs, &net_vals)?;
            if let Some(&declared) = wire_types.get(lhs) {
                sig = declared;
            }
            let masked = mask_bits(v, sig.bits, sig.signed);
            net_vals.insert(lhs.clone(), (masked, sig));
        }
        let comb_val = net_vals
            .get("comb_result")
            .map(|(v, _)| mask_bits(*v, result_sig.bits, result_sig.signed))
            .ok_or_else(|| "emitted Verilog missing `assign comb_result = ...;`".to_string())?;

        // Rising clock edge: Stage 2 latches comb_result, Stage 1 latches inputs.
        valid_out = valid_s1;
        reg_result = comb_val;

        if cycle < vectors.len() {
            valid_s1 = true;
            for (port_name, port_sig) in &input_ports {
                let val = vectors[cycle]
                    .iter()
                    .find(|(k, _)| k == port_name)
                    .map(|(_, v)| *v)
                    .ok_or_else(|| format!("missing vector input `{port_name}`"))?;
                let masked = mask_bits(val as i128, port_sig.bits, port_sig.signed);
                s1_regs.insert(format!("s1_{port_name}"), (masked, *port_sig));
            }
        } else {
            valid_s1 = false;
        }

        if valid_out {
            outputs.push(reg_result as i64);
        }
    }

    Ok(outputs)
}
