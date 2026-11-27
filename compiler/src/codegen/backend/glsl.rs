// ===============================================================================
// BUILDLANG CODE GENERATOR - GLSL BACKEND
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! GLSL code generation backend.
//!
//! Transpiles MIR to GLSL (OpenGL Shading Language) for use with:
//! - Vulkan (via GLSL → SPIR-V compilation with glslangValidator)
//! - OpenGL 4.5+ shaders
//! - Compute shaders
//!
//! Shares structured control flow reconstruction with the HLSL backend.

use std::collections::{HashMap, HashSet};

use super::{CodegenError, CodegenResult};
use crate::codegen::analysis::structured_cfg::{self, HeaderKind, StructuredCfg};
use crate::codegen::ir::*;

struct ComputeBindings {
    buffer_vars: HashMap<LocalId, String>,
    scalar_fields: HashMap<LocalId, String>,
    push_const_var: Option<String>,
    workgroup_vars: HashMap<LocalId, String>,
}

/// GLSL backend for code generation.
pub struct GlslBackend {
    /// Output buffer.
    output: String,
    /// Indentation level.
    indent: usize,
    /// Inlined expression strings for single-use temps (interior mutability for &self access).
    inlined_exprs: std::cell::RefCell<std::collections::HashMap<u32, String>>,
    /// Locals identified as single-use temps that will be inlined.
    single_use_temps: std::collections::HashSet<u32>,
    /// Locals whose declaration can be folded into their first assignment.
    /// (assigned exactly once, at function body top level)
    fold_decl: std::collections::HashSet<u32>,
    /// Locals that have already been declared (via folded assignment).
    declared: std::collections::HashSet<u32>,
}

impl GlslBackend {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            inlined_exprs: std::cell::RefCell::new(std::collections::HashMap::new()),
            single_use_temps: std::collections::HashSet::new(),
            fold_decl: std::collections::HashSet::new(),
            declared: std::collections::HashSet::new(),
        }
    }

    /// Count how many times each local is used across all blocks.
    fn compute_use_counts(blocks: &[MirBlock]) -> std::collections::HashMap<u32, u32> {
        let mut counts = std::collections::HashMap::new();

        fn count_value(val: &MirValue, counts: &mut std::collections::HashMap<u32, u32>) {
            if let MirValue::Local(id) = val {
                *counts.entry(id.0).or_insert(0) += 1;
            }
        }

        fn count_rvalue(rv: &MirRValue, counts: &mut std::collections::HashMap<u32, u32>) {
            match rv {
                MirRValue::Use(v) => count_value(v, counts),
                MirRValue::BinaryOp { left, right, .. } => {
                    count_value(left, counts);
                    count_value(right, counts);
                }
                MirRValue::UnaryOp { operand, .. } => count_value(operand, counts),
                MirRValue::Aggregate { operands, .. } => {
                    for op in operands {
                        count_value(op, counts);
                    }
                }
                MirRValue::FieldAccess { base, .. } => count_value(base, counts),
                MirRValue::Cast { value, .. } => count_value(value, counts),
                _ => {}
            }
        }

        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { value, .. } = &stmt.kind {
                    count_rvalue(value, &mut counts);
                }
            }
            if let Some(term) = &block.terminator {
                match term {
                    MirTerminator::Return(Some(v)) => count_value(v, &mut counts),
                    MirTerminator::If { cond, .. } => count_value(cond, &mut counts),
                    MirTerminator::Call { args, .. } => {
                        for a in args {
                            count_value(a, &mut counts);
                        }
                    }
                    _ => {}
                }
            }
        }
        counts
    }

    /// Identify single-use temporaries that can be inlined.
    /// A local is inlineable iff: assigned exactly once, used exactly once, is a temp, and is pure.
    fn compute_inlineable_temps(
        blocks: &[MirBlock],
        locals: &[MirLocal],
        use_counts: &std::collections::HashMap<u32, u32>,
    ) -> std::collections::HashSet<u32> {
        // Count how many times each local is ASSIGNED (must be exactly 1 for safe inlining)
        let mut assign_counts: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { dest, .. } = &stmt.kind {
                    *assign_counts.entry(dest.0).or_insert(0) += 1;
                }
            }
            if let Some(MirTerminator::Call {
                dest: Some(dest_id),
                ..
            }) = &block.terminator
            {
                *assign_counts.entry(dest_id.0).or_insert(0) += 1;
            }
        }

        let mut inlineable = std::collections::HashSet::new();
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                    let use_count = use_counts.get(&dest.0).copied().unwrap_or(0);
                    let def_count = assign_counts.get(&dest.0).copied().unwrap_or(0);
                    if use_count == 1 && def_count == 1 {
                        // Check it's a compiler temp (name starts with _)
                        let name = locals
                            .iter()
                            .find(|l| l.id == *dest)
                            .and_then(|l| l.name.as_ref());
                        let is_temp = match name {
                            Some(n) => {
                                n.starts_with('_')
                                    || n.as_ref().chars().next().map_or(true, |c| c == '_')
                            }
                            None => true, // unnamed locals are temps
                        };
                        // Only inline pure expressions (not used for control flow side effects)
                        let is_pure = matches!(
                            value,
                            MirRValue::Use(_)
                                | MirRValue::BinaryOp { .. }
                                | MirRValue::UnaryOp { .. }
                                | MirRValue::FieldAccess { .. }
                                | MirRValue::Cast { .. }
                                | MirRValue::Aggregate { .. }
                        );
                        if is_temp && is_pure {
                            inlineable.insert(dest.0);
                        }
                    }
                }
            }
            // Also check Call terminator destinations (single-use call results)
            if let Some(MirTerminator::Call {
                dest: Some(dest_id),
                ..
            }) = &block.terminator
            {
                let use_count = use_counts.get(&dest_id.0).copied().unwrap_or(0);
                let def_count = assign_counts.get(&dest_id.0).copied().unwrap_or(0);
                if use_count == 1 && def_count == 1 {
                    let name = locals
                        .iter()
                        .find(|l| l.id == *dest_id)
                        .and_then(|l| l.name.as_ref());
                    let is_temp = match name {
                        Some(n) => n.starts_with('_') || n.as_ref().starts_with('_'),
                        None => true,
                    };
                    if is_temp {
                        inlineable.insert(dest_id.0);
                    }
                }
            }
        }
        inlineable
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn writeln(&mut self, s: &str) {
        self.write_indent();
        self.output.push_str(s);
        self.output.push('\n');
    }

    /// Generate GLSL from a MIR module.
    pub fn generate(&mut self, module: &MirModule) -> CodegenResult<String> {
        self.output.clear();

        // Header
        self.output.push_str("// Generated by BuildLang Compiler\n");
        self.output.push_str("// Target: GLSL (OpenGL / Vulkan)\n");
        self.output.push_str("// Do not edit manually\n\n");
        self.output.push_str("#version 450\n\n");

        // Uniform declarations
        for u in &module.uniforms {
            let ty_str = self.type_to_glsl(&u.ty);
            if let Some(ref default) = u.default {
                self.writeln(&format!(
                    "uniform {} {} = {};",
                    ty_str,
                    u.name,
                    self.const_to_glsl(default)
                ));
            } else {
                self.writeln(&format!("uniform {} {};", ty_str, u.name));
            }
        }
        if !module.uniforms.is_empty() {
            self.output.push('\n');
        }

        // Struct definitions
        for ty in &module.types {
            self.generate_struct(ty)?;
        }

        let compute_count = module
            .functions
            .iter()
            .filter(|f| !f.is_declaration() && matches!(f.shader_stage, Some(ShaderStage::Compute)))
            .count();

        // Function definitions
        for func in &module.functions {
            if !func.is_declaration() {
                if matches!(func.shader_stage, Some(ShaderStage::Compute)) {
                    self.generate_compute_function(func, compute_count > 1)?;
                } else {
                    self.generate_function(func)?;
                }
            }
        }

        Ok(self.output.clone())
    }

    fn generate_struct(&mut self, ty: &MirTypeDef) -> CodegenResult<()> {
        // Skip built-in vector types - GLSL has native vec2/3/4
        match ty.name.as_ref() {
            "build_vec2" | "build_vec3" | "build_vec4" | "build_mat4" => return Ok(()),
            _ => {}
        }
        match &ty.kind {
            TypeDefKind::Struct { fields, .. } => {
                self.writeln(&format!("struct {} {{", ty.name));
                self.indent += 1;
                for (i, (name, field_ty)) in fields.iter().enumerate() {
                    let fname = name
                        .as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| format!("field_{}", i));
                    self.writeln(&format!("{} {};", self.type_to_glsl(field_ty), fname));
                }
                self.indent -= 1;
                self.writeln("};\n");
            }
            _ => {}
        }
        Ok(())
    }

    fn generate_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        // Skip non-shader main (test harness with printf/println)
        if func.name.as_ref() == "main" && func.shader_stage.is_none() {
            return Ok(());
        }

        // Emit type annotations as comments if any parameter has them
        let has_annotations = func
            .locals
            .iter()
            .any(|l| l.is_param && !l.annotations.is_empty());
        if has_annotations {
            let mut ann_parts = Vec::new();
            for local in func
                .locals
                .iter()
                .filter(|l| l.is_param && !l.annotations.is_empty())
            {
                let name = local
                    .name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                let anns: Vec<&str> = local.annotations.iter().map(|a| a.as_ref()).collect();
                ann_parts.push(format!("{}: {}", name, anns.join(", ")));
            }
            self.writeln(&format!("// @annotations {}", ann_parts.join("; ")));
        }

        let ret_ty = self.type_to_glsl(&func.sig.ret);
        let params: Vec<String> = func
            .sig
            .params
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let param_name = func
                    .locals
                    .iter()
                    .find(|l| l.is_param && l.id.0 == i as u32)
                    .and_then(|l| l.name.as_ref())
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("p{}", i));

                format!("{} {}", self.type_to_glsl(ty), param_name)
            })
            .collect();

        self.writeln(&format!(
            "{} {}({}) {{",
            ret_ty,
            func.name,
            params.join(", ")
        ));
        self.indent += 1;

        // Deep expression inlining: identify single-use temps and inline them
        self.inlined_exprs.borrow_mut().clear();
        self.single_use_temps.clear();
        self.fold_decl.clear();
        self.declared.clear();
        if let Some(blocks) = &func.blocks {
            let use_counts = Self::compute_use_counts(blocks);
            self.single_use_temps =
                Self::compute_inlineable_temps(blocks, &func.locals, &use_counts);

            // Identify locals that can have declaration folded into first assignment.
            // Compute the "entry chain" - blocks reachable from block 0 via linear
            // Call/Goto continuations (no branches). Assignments in this chain are top-level.
            let mut entry_chain: std::collections::HashSet<usize> =
                std::collections::HashSet::new();
            entry_chain.insert(0);
            let mut chain_idx = 0usize;
            loop {
                if chain_idx >= blocks.len() {
                    break;
                }
                match &blocks[chain_idx].terminator {
                    Some(MirTerminator::Call {
                        target: Some(t), ..
                    }) => {
                        if let Some(idx) = blocks.iter().position(|b| b.id == *t) {
                            entry_chain.insert(idx);
                            chain_idx = idx;
                            continue;
                        }
                    }
                    Some(MirTerminator::Goto(t)) => {
                        if let Some(idx) = blocks.iter().position(|b| b.id == *t) {
                            if idx > chain_idx {
                                entry_chain.insert(idx);
                                chain_idx = idx;
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
                break;
            }

            let mut assign_counts: std::collections::HashMap<u32, u32> =
                std::collections::HashMap::new();
            let mut entry_assigned: std::collections::HashSet<u32> =
                std::collections::HashSet::new();
            for (bi, block) in blocks.iter().enumerate() {
                let in_entry = entry_chain.contains(&bi);
                for stmt in &block.stmts {
                    if let MirStmtKind::Assign { dest, .. } = &stmt.kind {
                        *assign_counts.entry(dest.0).or_insert(0) += 1;
                        if in_entry {
                            entry_assigned.insert(dest.0);
                        }
                    }
                }
                if let Some(MirTerminator::Call {
                    dest: Some(dest_id),
                    ..
                }) = &block.terminator
                {
                    *assign_counts.entry(dest_id.0).or_insert(0) += 1;
                    if in_entry {
                        entry_assigned.insert(dest_id.0);
                    }
                }
            }
            for local in &func.locals {
                if local.is_param {
                    continue;
                }
                if matches!(local.ty, MirType::Void) {
                    continue;
                }
                if self.single_use_temps.contains(&local.id.0) {
                    continue;
                }
                let name = self.local_name(local.id, &func.locals);
                if name == "_ret" {
                    continue;
                }
                let count = assign_counts.get(&local.id.0).copied().unwrap_or(0);
                // Fold declaration for ANY single-assignment variable
                // (GLSL 4.5 allows declarations at any point in any scope)
                if count == 1 {
                    self.fold_decl.insert(local.id.0);
                }
            }
        }

        // Local variable declarations - only for locals that can't be folded
        for local in &func.locals {
            if !local.is_param {
                if matches!(local.ty, MirType::Void) {
                    continue;
                }
                let name = self.local_name(local.id, &func.locals);
                if name == "_ret" {
                    continue;
                }
                if self.single_use_temps.contains(&local.id.0) {
                    continue;
                }
                if self.fold_decl.contains(&local.id.0) {
                    continue;
                }
                let ty_str = self.type_to_glsl(&local.ty);
                self.writeln(&format!("{} {};", ty_str, name));
            }
        }

        // Generate function body from basic blocks
        if let Some(blocks) = &func.blocks {
            let mut emitted: std::collections::HashSet<u32> = std::collections::HashSet::new();
            for block in blocks {
                if !emitted.contains(&block.id.0) {
                    self.generate_block_structured(block, blocks, func, &mut emitted)?;
                }
            }

            // Safety return for non-void functions
            if !matches!(func.sig.ret, MirType::Void) {
                let last_is_return = self
                    .output
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .map_or(false, |l| l.trim().starts_with("return"));
                if !last_is_return {
                    if let Some(ret_val) = self.find_return_value(blocks, &func.locals) {
                        self.writeln(&format!("return {};", ret_val));
                    }
                }
            }
        }

        self.indent -= 1;
        self.writeln("}\n");
        Ok(())
    }

    fn generate_block_structured(
        &mut self,
        block: &MirBlock,
        all_blocks: &[MirBlock],
        func: &MirFunction,
        emitted: &mut std::collections::HashSet<u32>,
    ) -> CodegenResult<()> {
        if emitted.contains(&block.id.0) {
            return Ok(());
        }
        emitted.insert(block.id.0);

        // Emit statements with deep expression inlining
        for stmt in &block.stmts {
            if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                if self.single_use_temps.contains(&dest.0) {
                    // Don't emit - store the expression for inlining at use site
                    let expr_str = self.rvalue_to_glsl(value, func);
                    self.inlined_exprs.borrow_mut().insert(dest.0, expr_str);
                    continue;
                }
            }
            self.generate_statement(stmt, func)?;
        }

        // Handle terminator with structured control flow
        if let Some(term) = &block.terminator {
            match term {
                MirTerminator::If {
                    cond,
                    then_block,
                    else_block,
                } => {
                    // Detect while loop pattern
                    let is_while = Self::has_back_edge(*then_block, block.id, all_blocks);
                    let cond_raw = self.value_to_glsl(cond, &func.locals);
                    let cond_str = Self::strip_outer_parens(&cond_raw);

                    if is_while {
                        self.writeln(&format!("while ({}) {{", cond_str));
                        self.indent += 1;
                        if let Some(tb) = all_blocks.iter().find(|b| b.id == *then_block) {
                            self.generate_block_structured(tb, all_blocks, func, emitted)?;
                        }
                        self.indent -= 1;
                        self.writeln("}");
                        if let Some(eb) = all_blocks.iter().find(|b| b.id == *else_block) {
                            self.generate_block_structured(eb, all_blocks, func, emitted)?;
                        }
                    } else {
                        self.writeln(&format!("if ({}) {{", cond_str));
                        self.indent += 1;
                        if let Some(tb) = all_blocks.iter().find(|b| b.id == *then_block) {
                            self.generate_block_structured(tb, all_blocks, func, emitted)?;
                        }
                        self.indent -= 1;
                        self.writeln("} else {");
                        self.indent += 1;
                        if let Some(eb) = all_blocks.iter().find(|b| b.id == *else_block) {
                            self.generate_block_structured(eb, all_blocks, func, emitted)?;
                        }
                        self.indent -= 1;
                        self.writeln("}");
                    }
                }
                MirTerminator::Goto(target) => {
                    if let Some(tb) = all_blocks.iter().find(|b| b.id == *target) {
                        self.generate_block_structured(tb, all_blocks, func, emitted)?;
                    }
                }
                MirTerminator::Call {
                    func: callee,
                    args,
                    dest,
                    target,
                    ..
                } => {
                    let call_expr = self.generate_call_expr(callee, args, &func.locals);
                    if let Some(dest_id) = dest {
                        if self.single_use_temps.contains(&dest_id.0) {
                            // Inline: store call expression for later substitution
                            self.inlined_exprs.borrow_mut().insert(dest_id.0, call_expr);
                        } else {
                            let dest_name = self.local_name(*dest_id, &func.locals);
                            // Fold declaration with Call result assignment
                            if self.fold_decl.contains(&dest_id.0)
                                && !self.declared.contains(&dest_id.0)
                            {
                                let dest_ty =
                                    func.locals.iter().find(|l| l.id == *dest_id).map(|l| &l.ty);
                                if let Some(ty) = dest_ty {
                                    let ty_str = self.type_to_glsl(ty);
                                    self.writeln(&format!(
                                        "{} {} = {};",
                                        ty_str, dest_name, call_expr
                                    ));
                                    self.declared.insert(dest_id.0);
                                } else {
                                    self.writeln(&format!("{} = {};", dest_name, call_expr));
                                }
                            } else {
                                self.writeln(&format!("{} = {};", dest_name, call_expr));
                            }
                        }
                    } else {
                        self.writeln(&format!("{};", call_expr));
                    }
                    if let Some(target_id) = target {
                        if let Some(tb) = all_blocks.iter().find(|b| b.id == *target_id) {
                            self.generate_block_structured(tb, all_blocks, func, emitted)?;
                        }
                    }
                }
                MirTerminator::Return(Some(val)) => {
                    // Peephole: "temp = expr; return temp;" → "return expr;"
                    let val_str = self.value_to_glsl(val, &func.locals);
                    let mut folded = false;
                    let search_pat = format!("{} = ", val_str);
                    let trimmed_len = self.output.trim_end().len();
                    if trimmed_len > 0 {
                        let last_nl = self.output[..trimmed_len].rfind('\n').unwrap_or(0);
                        let line_start = if last_nl == 0 { 0 } else { last_nl + 1 };
                        let last_line = self.output[line_start..trimmed_len].trim();
                        if last_line.contains(&search_pat) && last_line.ends_with(';') {
                            if let Some(eq_pos) = last_line.find(" = ") {
                                let rhs = &last_line[eq_pos + 3..last_line.len() - 1];
                                let rhs_owned = rhs.to_string();
                                self.output.truncate(line_start);
                                self.writeln(&format!("return {};", rhs_owned));
                                folded = true;
                            }
                        }
                    }
                    if !folded {
                        self.writeln(&format!("return {};", val_str));
                    }
                }
                _ => {
                    self.generate_terminator(term, func)?;
                }
            }
        }
        Ok(())
    }

    fn generate_statement(&mut self, stmt: &MirStmt, func: &MirFunction) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { dest, value } => {
                // Skip void-typed assignments (unit values, no-ops)
                let dest_ty = func.locals.iter().find(|l| l.id == *dest).map(|l| &l.ty);
                if matches!(dest_ty, Some(MirType::Void)) {
                    return Ok(());
                }
                let dest_name = self.local_name(*dest, &func.locals);
                let val_str = self.rvalue_to_glsl(value, func);
                // Fold declaration with first assignment if eligible
                if self.fold_decl.contains(&dest.0) && !self.declared.contains(&dest.0) {
                    let ty_str = self.type_to_glsl(dest_ty.unwrap());
                    self.writeln(&format!("{} {} = {};", ty_str, dest_name, val_str));
                    self.declared.insert(dest.0);
                } else {
                    self.writeln(&format!("{} = {};", dest_name, val_str));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn generate_terminator(
        &mut self,
        term: &MirTerminator,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        match term {
            MirTerminator::Return(Some(val)) => {
                let val_str = self.value_to_glsl(val, &func.locals);
                self.writeln(&format!("return {};", val_str));
            }
            MirTerminator::Return(None) => {
                self.writeln("return;");
            }
            MirTerminator::Goto(_) | MirTerminator::If { .. } => {}
            MirTerminator::Call {
                func: callee,
                args,
                dest,
                ..
            } => {
                let call_expr = self.generate_call_expr(callee, args, &func.locals);
                if let Some(dest_id) = dest {
                    let dest_name = self.local_name(*dest_id, &func.locals);
                    self.writeln(&format!("{} = {};", dest_name, call_expr));
                } else {
                    self.writeln(&format!("{};", call_expr));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn rvalue_to_glsl(&self, rvalue: &MirRValue, func: &MirFunction) -> String {
        match rvalue {
            MirRValue::Use(val) => self.value_to_glsl(val, &func.locals),
            MirRValue::BinaryOp { op, left, right } => {
                let l = self.value_to_glsl(left, &func.locals);
                let r = self.value_to_glsl(right, &func.locals);
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Rem => "%",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<",
                    BinOp::Shr => ">>",
                    BinOp::Pow => "pow",
                    _ => "??",
                };
                if matches!(op, BinOp::Pow) {
                    format!("pow({}, {})", l, r)
                } else {
                    format!("({} {} {})", l, op_str, r)
                }
            }
            MirRValue::UnaryOp { op, operand } => {
                let v = self.value_to_glsl(operand, &func.locals);
                match op {
                    UnaryOp::Neg => format!("(-{})", v),
                    UnaryOp::Not => format!("(!{})", v),
                    UnaryOp::BitNot => format!("(~{})", v),
                }
            }
            MirRValue::Aggregate { kind, operands } => {
                match kind {
                    AggregateKind::Struct(name) => {
                        let fields: Vec<String> = operands
                            .iter()
                            .map(|o| self.value_to_glsl(o, &func.locals))
                            .collect();
                        // GLSL uses constructor syntax for vec types
                        match name.as_ref() {
                            "build_vec2" => format!("vec2({})", fields.join(", ")),
                            "build_vec3" => format!("vec3({})", fields.join(", ")),
                            "build_vec4" => format!("vec4({})", fields.join(", ")),
                            _ => format!("{}({})", name, fields.join(", ")),
                        }
                    }
                    _ => "0".to_string(),
                }
            }
            MirRValue::FieldAccess {
                base, field_name, ..
            } => {
                let b = self.value_to_glsl(base, &func.locals);
                format!("{}.{}", b, field_name)
            }
            MirRValue::Cast { value, ty, .. } => {
                let v = self.value_to_glsl(value, &func.locals);
                let t = self.type_to_glsl(ty);
                format!("{}({})", t, v) // GLSL uses constructor-style casts
            }
            _ => "0".to_string(),
        }
    }

    fn value_to_glsl(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        match value {
            MirValue::Local(id) => {
                // Check for inlined expression (single-use temp)
                if let Some(expr) = self.inlined_exprs.borrow().get(&id.0) {
                    return expr.clone();
                }
                self.local_name(*id, locals)
            }
            MirValue::Const(c) => self.const_to_glsl(c),
            MirValue::Global(name) | MirValue::Function(name) => {
                // Map C runtime functions to GLSL equivalents
                match name.as_ref() {
                    // Math
                    "fabs" => "abs".to_string(),
                    // Vector math
                    "build_dot2" | "build_dot3" | "build_dot4" => "dot".to_string(),
                    "build_normalize2" | "build_normalize3" | "build_normalize4" => {
                        "normalize".to_string()
                    }
                    "build_length2" | "build_length3" | "build_length4" => "length".to_string(),
                    "build_cross" => "cross".to_string(),
                    "build_reflect3" => "reflect".to_string(),
                    // Interpolation - GLSL uses mix(), not lerp()
                    "build_mix" | "lerp" | "build_lerp2" | "build_lerp3" | "build_lerp4" => {
                        "mix".to_string()
                    }
                    // Shader math
                    "build_clampf" | "build_clamp3" => "clamp".to_string(),
                    "build_smoothstep" => "smoothstep".to_string(),
                    "build_fract" => "fract".to_string(),
                    "build_step" => "step".to_string(),
                    // Min/max
                    "build_min_f64" | "build_min_f32" | "build_min_i32" | "build_min_i64" => {
                        "min".to_string()
                    }
                    "build_max_f64" | "build_max_f32" | "build_max_i32" | "build_max_i64" => {
                        "max".to_string()
                    }
                    // Texture sampling - GLSL uses texture(), not tex2D()
                    "texture_sample" | "build_texture_sample" => "texture".to_string(),
                    // Vector constructors
                    "build_vec2_new" => "vec2".to_string(),
                    "build_vec3_new" => "vec3".to_string(),
                    "build_vec4_new" => "vec4".to_string(),
                    // Matrix
                    "build_mat4_mul" => "matrixCompMult".to_string(),
                    // Strip build_ prefix for functions that map directly
                    _ => name.to_string(),
                }
            }
        }
    }

    fn const_to_glsl(&self, c: &MirConst) -> String {
        match c {
            MirConst::Bool(true) => "true".to_string(),
            MirConst::Bool(false) => "false".to_string(),
            MirConst::Int(v, _) => format!("{}", v),
            MirConst::Uint(v, _) => format!("{}u", v),
            MirConst::Float(v, _) => {
                // GLSL uses plain float literals (no 'f' suffix needed in #version 450)
                if v.fract() == 0.0 {
                    format!("{}.0", v)
                } else {
                    format!("{}", v)
                }
            }
            MirConst::Unit => "0".to_string(),
            _ => "0".to_string(),
        }
    }

    /// Convert MIR type to GLSL type name.
    fn type_to_glsl(&self, ty: &MirType) -> String {
        match ty {
            MirType::Void => "void".to_string(),
            MirType::Bool => "bool".to_string(),
            MirType::Int(size, signed) => match (size, signed) {
                (IntSize::I32, true) => "int".to_string(),
                (IntSize::I32, false) => "uint".to_string(),
                _ => "int".to_string(),
            },
            MirType::Float(FloatSize::F32) => "float".to_string(),
            MirType::Float(FloatSize::F64) => "double".to_string(),
            MirType::Struct(name) => match name.as_ref() {
                "build_vec2" => "vec2".to_string(),
                "build_vec3" => "vec3".to_string(),
                "build_vec4" => "vec4".to_string(),
                "build_mat4" => "mat4".to_string(),
                _ => name.to_string(),
            },
            MirType::Ptr(_) => "int".to_string(),
            MirType::Array(elem, len) => {
                format!("{}[{}]", self.type_to_glsl(elem), len)
            }
            MirType::Texture2D(_) => "sampler2D".to_string(),
            MirType::Sampler => "sampler2D".to_string(),
            _ => "float".to_string(),
        }
    }

    /// Check if a block (or its descendants) contains a back-edge to the target block.
    fn strip_outer_parens(s: &str) -> &str {
        let t = s.trim();
        if t.len() < 2 || !t.starts_with('(') || !t.ends_with(')') {
            return t;
        }
        let mut depth = 0i32;
        for (i, c) in t.chars().enumerate() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 && i < t.len() - 1 {
                        return t;
                    }
                }
                _ => {}
            }
        }
        if depth == 0 {
            &t[1..t.len() - 1]
        } else {
            t
        }
    }

    fn has_back_edge(from: BlockId, target: BlockId, all_blocks: &[MirBlock]) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![from];
        while let Some(id) = stack.pop() {
            if visited.contains(&id.0) {
                continue;
            }
            visited.insert(id.0);
            if let Some(block) = all_blocks.iter().find(|b| b.id == id) {
                let follow = |t: &BlockId, stack: &mut Vec<BlockId>| {
                    if *t == target {
                        return true;
                    }
                    if t.0 > target.0 {
                        stack.push(*t);
                    }
                    false
                };
                match &block.terminator {
                    Some(MirTerminator::Goto(t)) => {
                        if follow(t, &mut stack) {
                            return true;
                        }
                    }
                    Some(MirTerminator::Call {
                        target: Some(t), ..
                    }) => {
                        if follow(t, &mut stack) {
                            return true;
                        }
                    }
                    Some(MirTerminator::If {
                        then_block,
                        else_block,
                        ..
                    }) => {
                        if follow(then_block, &mut stack) {
                            return true;
                        }
                        if follow(else_block, &mut stack) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    /// Generate a function call expression, with special handling for intrinsics.
    fn generate_call_expr(
        &self,
        callee: &MirValue,
        args: &[MirValue],
        locals: &[MirLocal],
    ) -> String {
        let callee_name = match callee {
            MirValue::Function(name) | MirValue::Global(name) => Some(name.as_ref()),
            _ => None,
        };
        let callee_str = self.value_to_glsl(callee, locals);
        let arg_strs: Vec<String> = args.iter().map(|a| self.value_to_glsl(a, locals)).collect();

        match callee_name {
            Some("tex2d") => format!("texture(backbuffer, {})", arg_strs.join(", ")),
            Some("tex2d_depth") => format!("texture(depthbuffer, {}).r", arg_strs.join(", ")),
            Some("fma") | Some("fmaf") if arg_strs.len() == 3 => {
                format!("fma({}, {}, {})", arg_strs[0], arg_strs[1], arg_strs[2])
            }
            _ => format!("{}({})", callee_str, arg_strs.join(", ")),
        }
    }

    fn find_return_value(&self, blocks: &[MirBlock], locals: &[MirLocal]) -> Option<String> {
        for block in blocks {
            if let Some(MirTerminator::Return(Some(val))) = &block.terminator {
                return Some(self.value_to_glsl(val, locals));
            }
        }
        None
    }

    fn local_name(&self, id: LocalId, locals: &[MirLocal]) -> String {
        locals
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.name.as_ref())
            .map(|n| {
                // Escape GLSL reserved words
                match n.as_ref() {
                    "input" | "output" | "attribute" | "varying" | "uniform" | "buffer"
                    | "shared" | "sampler" | "image" | "texture" => format!("_{}", n),
                    _ => n.to_string(),
                }
            })
            .unwrap_or_else(|| format!("_{}", id.0))
    }

    fn compute_local_name(&self, id: LocalId, locals: &[MirLocal]) -> String {
        locals
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.name.as_ref())
            .map(|n| Self::sanitize_glsl_ident(n.as_ref()))
            .unwrap_or_else(|| format!("v_{}", id.0))
    }

    fn sanitize_glsl_ident(raw: &str) -> String {
        let trimmed = raw.trim_start_matches('_');
        let base = if trimmed.is_empty() {
            "v_tmp".to_string()
        } else if raw.starts_with('_') {
            format!("bl_{trimmed}")
        } else {
            raw.to_string()
        };
        let no_double = base.replace("__", "_x_");
        match no_double.as_str() {
            "in" | "out" | "inout" | "input" | "output" | "attribute" | "varying" | "uniform"
            | "buffer" | "shared" | "sampler" | "image" | "texture" | "layout" | "readonly"
            | "writeonly" | "restrict" | "coherent" | "volatile" | "precision" | "flat"
            | "smooth" | "noperspective" | "centroid" | "sample" | "patch" | "invariant"
            | "subroutine" | "struct" | "void" | "main" | "bool" | "int" | "uint" | "float"
            | "double" | "vec2" | "vec3" | "vec4" | "mat4" => format!("bl_{no_double}"),
            _ => no_double,
        }
    }

    fn compute_buffer_elem_type(ty: &MirType) -> Option<&MirType> {
        match ty {
            MirType::Slice(elem) => Some(elem.as_ref()),
            MirType::Ptr(inner) => match inner.as_ref() {
                MirType::Slice(elem) | MirType::Array(elem, _) => Some(elem.as_ref()),
                _ => None,
            },
            _ => None,
        }
    }

    fn compute_workgroup_array_len(local: &MirLocal) -> Option<u32> {
        local
            .annotations
            .iter()
            .find_map(|a| a.strip_prefix("Workgroup:")?.parse::<u32>().ok())
    }

    fn compute_find_written_buffers(func: &MirFunction) -> HashSet<LocalId> {
        let mut out = HashSet::new();
        if let Some(blocks) = &func.blocks {
            for b in blocks {
                for s in &b.stmts {
                    if let MirStmtKind::IndexStore {
                        base: MirValue::Local(id),
                        ..
                    } = &s.kind
                    {
                        out.insert(*id);
                    }
                }
            }
        }
        out
    }

    fn compute_reads_nullary(func: &MirFunction, pred: impl Fn(&NullaryOp) -> bool) -> bool {
        func.blocks.as_ref().is_some_and(|blocks| {
            blocks.iter().any(|b| {
                b.stmts.iter().any(|s| match &s.kind {
                    MirStmtKind::Assign {
                        value: MirRValue::NullaryOp(op, _),
                        ..
                    } => pred(op),
                    _ => false,
                })
            })
        })
    }

    fn resolve_workgroup_size(func: &MirFunction) -> [u32; 3] {
        if let Some(sz) = func.workgroup_size {
            return sz;
        }
        if Self::compute_reads_nullary(
            func,
            |op| matches!(op, NullaryOp::ThreadIndex(c) if *c >= 1),
        ) {
            [16, 16, 1]
        } else {
            [64, 1, 1]
        }
    }

    fn generate_compute_function(
        &mut self,
        func: &MirFunction,
        multi_entry: bool,
    ) -> CodegenResult<()> {
        let wg = Self::resolve_workgroup_size(func);
        if !multi_entry {
            self.writeln(&format!(
                "layout(local_size_x = {}, local_size_y = {}, local_size_z = {}) in;",
                wg[0], wg[1], wg[2]
            ));
            self.output.push('\n');
        }

        let written = Self::compute_find_written_buffers(func);
        let param_locals: Vec<&MirLocal> = func.locals.iter().filter(|l| l.is_param).collect();
        let fn_tag = Self::sanitize_glsl_ident(func.name.as_ref());

        let mut buffer_vars = HashMap::new();
        let mut scalar_fields = HashMap::new();
        let mut scalar_decls: Vec<(String, String)> = Vec::new();
        let mut binding_idx: u32 = 0;

        for (idx, param_ty) in func.sig.params.iter().enumerate() {
            let local = param_locals.get(idx).copied();
            let raw_name = local
                .map(|l| self.compute_local_name(l.id, &func.locals))
                .unwrap_or_else(|| format!("p_{idx}"));
            let var_name = if multi_entry {
                format!("{fn_tag}_{raw_name}")
            } else {
                raw_name.clone()
            };

            if let Some(elem_ty) = Self::compute_buffer_elem_type(param_ty) {
                let elem_glsl = self.type_to_glsl(elem_ty);
                let is_writable = local.is_some_and(|l| l.is_mut || written.contains(&l.id));
                let qual = if is_writable { "" } else { "readonly " };
                self.writeln(&format!(
                    "layout(std430, set = 0, binding = {binding_idx}) {qual}buffer Buf_{binding_idx}_{var_name} {{ {elem_glsl} {var_name}[]; }};"
                ));
                if let Some(l) = local {
                    buffer_vars.insert(l.id, var_name);
                }
                binding_idx += 1;
            } else {
                let gty = self.type_to_glsl(param_ty);
                scalar_decls.push((raw_name.clone(), gty));
                if let Some(l) = local {
                    scalar_fields.insert(l.id, raw_name);
                }
            }
        }

        let push_const_var = if !scalar_decls.is_empty() {
            let block_name = format!("Params_{fn_tag}");
            let inst_name = format!("params_{fn_tag}");
            self.writeln(&format!("layout(push_constant) uniform {block_name} {{"));
            self.indent += 1;
            for (fname, fty) in &scalar_decls {
                self.writeln(&format!("{fty} {fname};"));
            }
            self.indent -= 1;
            self.writeln(&format!("}} {inst_name};"));
            Some(inst_name)
        } else {
            None
        };

        let mut workgroup_vars = HashMap::new();
        for local in &func.locals {
            if let Some(count) = Self::compute_workgroup_array_len(local) {
                let elem_ty = match &local.ty {
                    MirType::Array(inner, _) | MirType::Slice(inner) => inner.as_ref(),
                    MirType::Ptr(inner) => match inner.as_ref() {
                        MirType::Array(e, _) | MirType::Slice(e) => e.as_ref(),
                        other => other,
                    },
                    other => other,
                };
                let elem_glsl = self.type_to_glsl(elem_ty);
                let raw = self.compute_local_name(local.id, &func.locals);
                let wg_name = if multi_entry {
                    format!("{fn_tag}_{raw}")
                } else {
                    raw
                };
                self.writeln(&format!("shared {elem_glsl} {wg_name}[{count}];"));
                workgroup_vars.insert(local.id, wg_name);
            }
        }

        if !buffer_vars.is_empty() || push_const_var.is_some() || !workgroup_vars.is_empty() {
            self.output.push('\n');
        }

        let bindings = ComputeBindings {
            buffer_vars,
            scalar_fields,
            push_const_var,
            workgroup_vars,
        };

        if multi_entry {
            self.writeln(&format!("void {fn_tag}() {{"));
        } else {
            self.writeln("void main() {");
        }
        self.indent += 1;

        for local in &func.locals {
            if local.is_param
                || matches!(local.ty, MirType::Void)
                || bindings.workgroup_vars.contains_key(&local.id)
            {
                continue;
            }
            let name = self.compute_local_name(local.id, &func.locals);
            let gty = self.type_to_glsl(&local.ty);
            self.writeln(&format!("{gty} {name};"));
        }

        if let Some(blocks) = &func.blocks {
            let cfg = structured_cfg::analyze(blocks);
            let mut visited = HashSet::new();
            if let Some(entry) = blocks.first() {
                self.emit_compute_region(
                    entry.id,
                    None,
                    blocks,
                    func,
                    &cfg,
                    &bindings,
                    &mut visited,
                )?;
            }
        }

        self.indent -= 1;
        self.writeln("}\n");
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_compute_region(
        &mut self,
        start: BlockId,
        stop_at: Option<BlockId>,
        blocks: &[MirBlock],
        func: &MirFunction,
        cfg: &StructuredCfg,
        bindings: &ComputeBindings,
        visited: &mut HashSet<BlockId>,
    ) -> CodegenResult<()> {
        let mut current = Some(start);
        while let Some(bid) = current {
            if stop_at == Some(bid) || !visited.insert(bid) {
                break;
            }
            let Some(block) = blocks.iter().find(|b| b.id == bid) else {
                break;
            };

            match cfg.header(bid) {
                Some(HeaderKind::Loop { merge, .. }) => {
                    let Some(MirTerminator::If {
                        cond,
                        then_block,
                        else_block,
                    }) = &block.terminator
                    else {
                        return Err(CodegenError::InvalidMir(
                            "loop header missing If terminator".to_string(),
                        ));
                    };
                    self.writeln("while (true) {");
                    self.indent += 1;
                    self.emit_compute_block_stmts(block, func, bindings)?;
                    let cond_s = self.compute_value_to_glsl(cond, &func.locals, bindings)?;
                    let (body_block, exit_block, negate) = if *else_block == merge {
                        (*then_block, *else_block, true)
                    } else {
                        (*else_block, *then_block, false)
                    };
                    if negate {
                        self.writeln(&format!("if (!({cond_s})) {{ break; }}"));
                    } else {
                        self.writeln(&format!("if ({cond_s}) {{ break; }}"));
                    }
                    self.emit_compute_region(
                        body_block,
                        Some(bid),
                        blocks,
                        func,
                        cfg,
                        bindings,
                        visited,
                    )?;
                    self.indent -= 1;
                    self.writeln("}");
                    current = Some(exit_block);
                }
                Some(HeaderKind::Selection { merge }) => {
                    self.emit_compute_block_stmts(block, func, bindings)?;
                    let Some(MirTerminator::If {
                        cond,
                        then_block,
                        else_block,
                    }) = &block.terminator
                    else {
                        return Err(CodegenError::InvalidMir(
                            "selection header missing If terminator".to_string(),
                        ));
                    };
                    let cond_s = self.compute_value_to_glsl(cond, &func.locals, bindings)?;
                    self.writeln(&format!("if ({cond_s}) {{"));
                    self.indent += 1;
                    self.emit_compute_region(
                        *then_block,
                        Some(merge),
                        blocks,
                        func,
                        cfg,
                        bindings,
                        visited,
                    )?;
                    self.indent -= 1;
                    if *else_block != merge {
                        self.writeln("} else {");
                        self.indent += 1;
                        self.emit_compute_region(
                            *else_block,
                            Some(merge),
                            blocks,
                            func,
                            cfg,
                            bindings,
                            visited,
                        )?;
                        self.indent -= 1;
                    }
                    self.writeln("}");
                    current = Some(merge);
                }
                None => {
                    self.emit_compute_block_stmts(block, func, bindings)?;
                    current = self.emit_compute_plain_terminator(
                        &block.terminator,
                        func,
                        bindings,
                        stop_at,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn emit_compute_block_stmts(
        &mut self,
        block: &MirBlock,
        func: &MirFunction,
        bindings: &ComputeBindings,
    ) -> CodegenResult<()> {
        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign { dest, value } => {
                    if bindings.workgroup_vars.contains_key(dest) {
                        continue;
                    }
                    let dest_ty = func.locals.iter().find(|l| l.id == *dest).map(|l| &l.ty);
                    if matches!(dest_ty, Some(MirType::Void)) {
                        continue;
                    }
                    let lhs = self.compute_local_name(*dest, &func.locals);
                    let rhs = self.compute_rvalue_to_glsl(value, func, bindings)?;
                    self.writeln(&format!("{lhs} = {rhs};"));
                }
                MirStmtKind::IndexStore {
                    base, index, value, ..
                } => {
                    let b = self.compute_value_to_glsl(base, &func.locals, bindings)?;
                    let idx = self.compute_value_to_glsl(index, &func.locals, bindings)?;
                    let val = self.compute_rvalue_to_glsl(value, func, bindings)?;
                    self.writeln(&format!("{b}[{idx}] = {val};"));
                }
                MirStmtKind::FieldAssign {
                    base,
                    field_name,
                    value,
                } => {
                    let b = self.compute_local_name(*base, &func.locals);
                    let val = self.compute_rvalue_to_glsl(value, func, bindings)?;
                    self.writeln(&format!("{b}.{field_name} = {val};"));
                }
                MirStmtKind::WorkgroupBarrier => {
                    self.writeln("memoryBarrierShared();");
                    self.writeln("barrier();");
                }
                MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop => {}
                MirStmtKind::GlobalStore { name, .. } => {
                    return Err(CodegenError::Unsupported(format!(
                        "mutable module global `{name}` store is not supported in GLSL compute"
                    )));
                }
                MirStmtKind::DerefAssign { .. } | MirStmtKind::FieldDerefAssign { .. } => {
                    return Err(CodegenError::Unsupported(
                        "raw pointer dereference assignment is not supported in GLSL compute"
                            .to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn emit_compute_plain_terminator(
        &mut self,
        term: &Option<MirTerminator>,
        func: &MirFunction,
        bindings: &ComputeBindings,
        stop_at: Option<BlockId>,
    ) -> CodegenResult<Option<BlockId>> {
        match term {
            None => Ok(None),
            Some(MirTerminator::Goto(target)) => {
                if stop_at == Some(*target) {
                    Ok(None)
                } else {
                    Ok(Some(*target))
                }
            }
            Some(MirTerminator::Return(_)) | Some(MirTerminator::Unreachable) => {
                self.writeln("return;");
                Ok(None)
            }
            Some(MirTerminator::Call {
                func: callee,
                args,
                dest,
                target,
                ..
            }) => {
                let expr = self.compute_call_expr_to_glsl(callee, args, &func.locals, bindings)?;
                if let Some(d) = dest {
                    let lhs = self.compute_local_name(*d, &func.locals);
                    self.writeln(&format!("{lhs} = {expr};"));
                } else {
                    self.writeln(&format!("{expr};"));
                }
                Ok(target.filter(|t| stop_at != Some(*t)))
            }
            Some(MirTerminator::If { .. }) => Err(CodegenError::Unsupported(
                "unstructured control flow is not supported in GLSL compute".to_string(),
            )),
            Some(MirTerminator::Switch { .. }) => Err(CodegenError::Unsupported(
                "switch terminator is not yet supported in GLSL compute".to_string(),
            )),
            Some(MirTerminator::Drop { target, .. })
            | Some(MirTerminator::Assert { target, .. }) => Ok(Some(*target)),
            Some(MirTerminator::Resume | MirTerminator::Abort) => Err(CodegenError::Unsupported(
                "abort/unwind is not supported in GLSL compute shaders".to_string(),
            )),
        }
    }

    fn compute_rvalue_to_glsl(
        &self,
        rv: &MirRValue,
        func: &MirFunction,
        bindings: &ComputeBindings,
    ) -> CodegenResult<String> {
        match rv {
            MirRValue::Use(v) => self.compute_value_to_glsl(v, &func.locals, bindings),
            MirRValue::BinaryOp { op, left, right } => {
                let l = self.compute_value_to_glsl(left, &func.locals, bindings)?;
                let r = self.compute_value_to_glsl(right, &func.locals, bindings)?;
                // Wrapping-Add Spec Note (Wave 99B / Wave 106A):
                // GLSL 450 (section 5.9, matching WGSL, HLSL, MSL, and SPIR-V) defines unsigned
                // `uint` (`u32`) `+`, `-`, and `*` as exact two's-complement modular arithmetic
                // modulo 2^32 (required by FIPS 180-4 SHA-256 and bitwise compute kernels).
                // By contrast, BuildLang's CPU C backend inserts checked-overflow traps on
                // `AddChecked` (`exit 101`). Compute shaders therefore lower all Add/Sub/Mul
                // variants to native GLSL modular operators without software overflow traps.
                let sym = match op {
                    BinOp::Add | BinOp::AddChecked | BinOp::AddWrapping | BinOp::AddSaturating => {
                        "+"
                    }
                    BinOp::Sub | BinOp::SubChecked | BinOp::SubWrapping | BinOp::SubSaturating => {
                        "-"
                    }
                    BinOp::Mul | BinOp::MulChecked | BinOp::MulWrapping | BinOp::MulSaturating => {
                        "*"
                    }
                    BinOp::Div => "/",
                    BinOp::Rem => "%",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::BitAnd => "&",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<",
                    BinOp::Shr => ">>",
                    BinOp::Pow => return Ok(format!("pow({l}, {r})")),
                };
                Ok(format!("({l} {sym} {r})"))
            }
            MirRValue::UnaryOp { op, operand } => {
                let v = self.compute_value_to_glsl(operand, &func.locals, bindings)?;
                Ok(match op {
                    UnaryOp::Neg => format!("(-{v})"),
                    UnaryOp::Not => format!("(!{v})"),
                    UnaryOp::BitNot => format!("(~{v})"),
                })
            }
            MirRValue::Cast { value, ty, .. } => {
                let v = self.compute_value_to_glsl(value, &func.locals, bindings)?;
                let t = self.type_to_glsl(ty);
                Ok(format!("{t}({v})"))
            }
            MirRValue::FieldAccess {
                base, field_name, ..
            } => {
                let b = self.compute_value_to_glsl(base, &func.locals, bindings)?;
                Ok(format!("{b}.{field_name}"))
            }
            MirRValue::IndexAccess { base, index, .. } => {
                let b = self.compute_value_to_glsl(base, &func.locals, bindings)?;
                let i = self.compute_value_to_glsl(index, &func.locals, bindings)?;
                Ok(format!("{b}[{i}]"))
            }
            MirRValue::Aggregate { kind, operands } => {
                let mut ops = Vec::with_capacity(operands.len());
                for o in operands {
                    ops.push(self.compute_value_to_glsl(o, &func.locals, bindings)?);
                }
                match kind {
                    AggregateKind::Struct(name) => {
                        let ctor = match name.as_ref() {
                            "build_vec2" => "vec2",
                            "build_vec3" => "vec3",
                            "build_vec4" => "vec4",
                            other => other,
                        };
                        Ok(format!("{ctor}({})", ops.join(", ")))
                    }
                    AggregateKind::Array(elem_ty) => {
                        let gty = self.type_to_glsl(elem_ty);
                        Ok(format!("{gty}[{}]({})", ops.len(), ops.join(", ")))
                    }
                    _ => Err(CodegenError::Unsupported(format!(
                        "aggregate kind `{kind:?}` is not supported in GLSL compute"
                    ))),
                }
            }
            MirRValue::NullaryOp(op, _) => match op {
                NullaryOp::ThreadIndex(0) => Ok("gl_GlobalInvocationID.x".to_string()),
                NullaryOp::ThreadIndex(1) => Ok("gl_GlobalInvocationID.y".to_string()),
                NullaryOp::ThreadIndex(2) => Ok("gl_GlobalInvocationID.z".to_string()),
                NullaryOp::LocalInvocationId(0) => Ok("gl_LocalInvocationID.x".to_string()),
                NullaryOp::LocalInvocationId(1) => Ok("gl_LocalInvocationID.y".to_string()),
                NullaryOp::LocalInvocationId(2) => Ok("gl_LocalInvocationID.z".to_string()),
                NullaryOp::WorkgroupId(0) => Ok("gl_WorkGroupID.x".to_string()),
                NullaryOp::WorkgroupId(1) => Ok("gl_WorkGroupID.y".to_string()),
                NullaryOp::WorkgroupId(2) => Ok("gl_WorkGroupID.z".to_string()),
                other => Err(CodegenError::Unsupported(format!(
                    "nullary op `{other:?}` is not supported in GLSL compute"
                ))),
            },
            other => Err(CodegenError::Unsupported(format!(
                "rvalue `{other:?}` is not supported in GLSL compute"
            ))),
        }
    }

    fn compute_call_expr_to_glsl(
        &self,
        callee: &MirValue,
        args: &[MirValue],
        locals: &[MirLocal],
        bindings: &ComputeBindings,
    ) -> CodegenResult<String> {
        let name = match callee {
            MirValue::Function(n) | MirValue::Global(n) => n.as_ref(),
            _ => {
                return Err(CodegenError::Unsupported(
                    "indirect function calls are not supported in GLSL compute".to_string(),
                ));
            }
        };
        if matches!(name, "fma" | "fmaf") {
            if args.len() != 3 {
                return Err(CodegenError::Unsupported(format!(
                    "GLSL 450 `fma(a, b, c)` requires exactly 3 arguments, got {}",
                    args.len()
                )));
            }
            let a = self.compute_value_to_glsl(&args[0], locals, bindings)?;
            let b = self.compute_value_to_glsl(&args[1], locals, bindings)?;
            let c = self.compute_value_to_glsl(&args[2], locals, bindings)?;
            return Ok(format!("fma({a}, {b}, {c})"));
        }
        let mapped = match name {
            "fabs" | "fabsf" => "abs",
            "sqrtf" => "sqrt",
            "sinf" => "sin",
            "cosf" => "cos",
            "tanf" => "tan",
            "expf" => "exp",
            "logf" => "log",
            "powf" => "pow",
            "floorf" => "floor",
            "ceilf" => "ceil",
            "build_dot2" | "build_dot3" | "build_dot4" => "dot",
            "build_normalize2" | "build_normalize3" | "build_normalize4" => "normalize",
            "build_length2" | "build_length3" | "build_length4" => "length",
            "build_cross" => "cross",
            "build_reflect3" => "reflect",
            "build_mix" | "lerp" | "build_lerp2" | "build_lerp3" | "build_lerp4" => "mix",
            "build_clampf" | "build_clamp3" => "clamp",
            "build_smoothstep" => "smoothstep",
            "build_fract" => "fract",
            "build_step" => "step",
            "build_min_f32" | "build_min_f64" | "build_min_i32" | "build_min_i64" => "min",
            "build_max_f32" | "build_max_f64" | "build_max_i32" | "build_max_i64" => "max",
            "build_vec2_new" => "vec2",
            "build_vec3_new" => "vec3",
            "build_vec4_new" => "vec4",
            other => other,
        };
        let mut arg_strs = Vec::with_capacity(args.len());
        for a in args {
            arg_strs.push(self.compute_value_to_glsl(a, locals, bindings)?);
        }
        Ok(format!("{mapped}({})", arg_strs.join(", ")))
    }

    fn compute_value_to_glsl(
        &self,
        value: &MirValue,
        locals: &[MirLocal],
        bindings: &ComputeBindings,
    ) -> CodegenResult<String> {
        match value {
            MirValue::Local(id) => {
                if let Some(buf) = bindings.buffer_vars.get(id) {
                    return Ok(buf.clone());
                }
                if let Some(wg) = bindings.workgroup_vars.get(id) {
                    return Ok(wg.clone());
                }
                if let (Some(pvar), Some(field)) =
                    (&bindings.push_const_var, bindings.scalar_fields.get(id))
                {
                    return Ok(format!("{pvar}.{field}"));
                }
                Ok(self.compute_local_name(*id, locals))
            }
            MirValue::Const(c) => self.compute_const_to_glsl(c),
            MirValue::Global(name) | MirValue::Function(name) => Ok(name.to_string()),
        }
    }

    fn compute_const_to_glsl(&self, c: &MirConst) -> CodegenResult<String> {
        match c {
            MirConst::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
            MirConst::Int(v, ty) => match ty {
                MirType::Int(_, false) => Ok(format!("{v}u")),
                _ => Ok(format!("{v}")),
            },
            MirConst::Uint(v, _) => Ok(format!("{v}u")),
            MirConst::Float(v, _) => {
                let s = format!("{v}");
                if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                    Ok(format!("{s}.0"))
                } else {
                    Ok(s)
                }
            }
            MirConst::Unit => Ok("0u".to_string()),
            other => Err(CodegenError::Unsupported(format!(
                "constant `{other:?}` is not supported in GLSL compute"
            ))),
        }
    }
}
