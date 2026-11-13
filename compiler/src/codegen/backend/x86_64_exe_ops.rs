// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Statements, terminators and printing for the x86-64 native executable
//! backend. See `x86_64_exe.rs` for the value model.

use super::x86_64_exe::{refuse, FnGen};
use super::x86_64_exe_asm::RCX;
use super::CodegenResult;
use crate::codegen::ir::*;

fn is_signed(ty: &MirType) -> bool {
    matches!(ty, MirType::Int(_, true))
}

/// setcc condition byte for a comparison.
fn setcc(op: BinOp, signed: bool) -> Option<u8> {
    Some(match (op, signed) {
        (BinOp::Eq, _) => 0x94,
        (BinOp::Ne, _) => 0x95,
        (BinOp::Lt, true) => 0x9C,
        (BinOp::Le, true) => 0x9E,
        (BinOp::Gt, true) => 0x9F,
        (BinOp::Ge, true) => 0x9D,
        (BinOp::Lt, false) => 0x92,
        (BinOp::Le, false) => 0x96,
        (BinOp::Gt, false) => 0x97,
        (BinOp::Ge, false) => 0x93,
        _ => return None,
    })
}

impl FnGen<'_> {
    pub(super) fn stmt(&mut self, s: &MirStmt) -> CodegenResult<()> {
        match &s.kind {
            MirStmtKind::Assign { dest, value } => {
                self.rvalue(value)?;
                self.store(*dest)
            }
            MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop => Ok(()),
            other => refuse(format!("the statement `{other:?}`")),
        }
    }

    /// Evaluate an rvalue into rax.
    fn rvalue(&mut self, v: &MirRValue) -> CodegenResult<()> {
        match v {
            MirRValue::Use(x) => self.load(x),
            MirRValue::BinaryOp { op, left, right } => self.binary(*op, left, right),
            MirRValue::UnaryOp { op, operand } => {
                let ty = self.value_type(operand)?;
                self.load(operand)?;
                match op {
                    UnaryOp::Neg => self.a.b(&[0x48, 0xF7, 0xD8]),
                    UnaryOp::Not if ty == MirType::Bool => self.a.b(&[0x48, 0x83, 0xF0, 0x01]),
                    UnaryOp::Not | UnaryOp::BitNot => self.a.b(&[0x48, 0xF7, 0xD0]),
                }
                Ok(())
            }
            MirRValue::Cast {
                kind: CastKind::IntToInt,
                value,
                ty,
            } => {
                self.load(value)?;
                self.normalize(ty);
                Ok(())
            }
            MirRValue::Cast { kind, .. } => refuse(format!("the cast `{kind:?}`")),
            other => refuse(format!("the expression `{}`", rvalue_name(other))),
        }
    }

    fn binary(&mut self, op: BinOp, left: &MirValue, right: &MirValue) -> CodegenResult<()> {
        let signed = is_signed(&self.value_type(left)?);
        self.load_pair(left, right)?;
        let code: &[u8] = match op {
            BinOp::Add | BinOp::AddWrapping => &[0x48, 0x01, 0xC8],
            BinOp::Sub | BinOp::SubWrapping => &[0x48, 0x29, 0xC8],
            BinOp::Mul | BinOp::MulWrapping => &[0x48, 0x0F, 0xAF, 0xC1],
            BinOp::BitAnd => &[0x48, 0x21, 0xC8],
            BinOp::BitOr => &[0x48, 0x09, 0xC8],
            BinOp::BitXor => &[0x48, 0x31, 0xC8],
            BinOp::Shl => &[0x48, 0xD3, 0xE0],
            BinOp::Shr if signed => &[0x48, 0xD3, 0xF8],
            BinOp::Shr => &[0x48, 0xD3, 0xE8],
            BinOp::Div | BinOp::Rem => {
                self.divide(signed, op == BinOp::Rem);
                return Ok(());
            }
            _ => {
                let Some(cc) = setcc(op, signed) else {
                    return refuse(format!("the operator `{op:?}`"));
                };
                // cmp rax, rcx; setcc al; movzx eax, al
                self.a
                    .b(&[0x48, 0x39, 0xC8, 0x0F, cc, 0xC0, 0x0F, 0xB6, 0xC0]);
                return Ok(());
            }
        };
        self.a.b(code);
        Ok(())
    }

    pub(super) fn terminator(&mut self, t: &MirTerminator) -> CodegenResult<()> {
        match t {
            MirTerminator::Goto(b) => {
                let l = self.block(*b)?;
                self.a.jmp(l);
            }
            MirTerminator::If {
                cond,
                then_block,
                else_block,
            } => {
                let (t, e) = (self.block(*then_block)?, self.block(*else_block)?);
                self.load(cond)?;
                self.a.test(0);
                self.a.jcc(0x84, e);
                self.a.jmp(t);
            }
            MirTerminator::Switch {
                value,
                targets,
                default,
            } => {
                self.load(value)?;
                for (c, b) in targets {
                    let k = match c {
                        MirConst::Int(x, _) => *x as u64,
                        MirConst::Uint(x, _) => *x as u64,
                        MirConst::Bool(x) => *x as u64,
                        other => return refuse(format!("a switch on `{other}`")),
                    };
                    let l = self.block(*b)?;
                    self.a.mov_imm(RCX, k);
                    self.a.b(&[0x48, 0x39, 0xC8]);
                    self.a.jcc(0x84, l);
                }
                let d = self.block(*default)?;
                self.a.jmp(d);
            }
            MirTerminator::Call {
                func,
                args,
                dest,
                target,
                ..
            } => {
                let name = match func {
                    MirValue::Function(n) | MirValue::Global(n) => n.to_string(),
                    _ => return refuse("indirect calls".into()),
                };
                match name.as_str() {
                    "printf" => self.printf(args)?,
                    "build_string_new"
                        if dest.is_some_and(|d| self.str_locals.contains_key(&d)) => {}
                    "println" | "print" => self.print_holes(name == "println", args)?,
                    "eprintf" => return refuse("printing to stderr (`eprintln`)".into()),
                    _ => self.call_user(&name, args, *dest)?,
                }
                if let Some(b) = target {
                    let l = self.block(*b)?;
                    self.a.jmp(l);
                }
            }
            MirTerminator::Drop { target, .. } => {
                let l = self.block(*target)?;
                self.a.jmp(l);
            }
            MirTerminator::Return(v) => {
                match v {
                    Some(v) => self.load(v)?,
                    None => self.a.mov_imm(0, 0),
                }
                self.a.leave_ret();
            }
            MirTerminator::Unreachable => self.a.b(&[0x0F, 0x0B]),
            other => return refuse(format!("the terminator `{}`", term_name(other))),
        }
        Ok(())
    }

    fn write_text(&mut self, text: &[u8]) {
        if text.is_empty() {
            return;
        }
        let (a0, a1) = (self.os.arg_regs()[0], self.os.arg_regs()[1]);
        let l = self.a.data(text);
        self.a.lea_rip(a0, l);
        self.a.mov_imm(a1, text.len() as u64);
        self.a.call(self.rt.write);
    }

    /// `println`/`print` arrive as printf with a C format the lowering built.
    /// Supported conversions: %d %i %u %lld %llu %ld %lu, %s of a bool or a
    /// string literal, and %%.
    fn printf(&mut self, args: &[MirValue]) -> CodegenResult<()> {
        let fmt = match args.first() {
            Some(MirValue::Const(MirConst::Str(i))) => *i as usize,
            Some(MirValue::Local(l)) if self.str_locals.contains_key(l) => self.str_locals[l],
            _ => return refuse("a print without a literal format".into()),
        };
        let fmt = self.strings.get(fmt).cloned().unwrap_or_default();
        let mut text = Vec::new();
        let mut next = 1;
        let mut i = 0;
        while i < fmt.len() {
            let c = fmt[i];
            i += 1;
            if c != b'%' {
                text.push(c);
                continue;
            }
            let start = i;
            while i < fmt.len() && fmt[i] == b'l' {
                i += 1;
            }
            let conv = fmt.get(i).copied().unwrap_or(0);
            i += 1;
            if conv == b'%' && start + 1 == i {
                text.push(b'%');
                continue;
            }
            if !matches!(conv, b'd' | b'i' | b'u' | b's') {
                let spec = String::from_utf8_lossy(&fmt[start - 1..i.min(fmt.len())]).into_owned();
                return refuse(format!("the format conversion `{spec}` (width, flags, hex and floats are not supported yet)"));
            }
            let Some(arg) = args.get(next).cloned() else {
                return refuse("a format with more conversions than arguments".into());
            };
            next += 1;
            self.write_text(&std::mem::take(&mut text));
            self.print_arg(conv, &arg)?;
        }
        self.write_text(&text);
        Ok(())
    }

    /// `println("a {} b", x)` called as a function: plain `{}` holes only.
    fn print_holes(&mut self, newline: bool, args: &[MirValue]) -> CodegenResult<()> {
        let fmt = match args.first() {
            Some(MirValue::Local(l)) if self.str_locals.contains_key(l) => self.str_locals[l],
            _ => return refuse("a print without a literal format".into()),
        };
        let fmt = String::from_utf8_lossy(&self.strings.get(fmt).cloned().unwrap_or_default())
            .into_owned();
        let pieces: Vec<&str> = fmt.split("{}").collect();
        if pieces.iter().any(|p| p.contains('{') || p.contains('}')) {
            return refuse(format!(
                "the format `{fmt}` (only plain `{{}}` holes are supported)"
            ));
        }
        if pieces.len() != args.len() {
            return refuse(format!(
                "the format `{fmt}` with {} arguments",
                args.len() - 1
            ));
        }
        for (i, piece) in pieces.iter().enumerate() {
            self.write_text(piece.as_bytes());
            if let Some(arg) = args.get(i + 1) {
                let lit = matches!(arg, MirValue::Local(l) if self.str_locals.contains_key(l));
                let conv = if lit || self.value_type(arg)? == MirType::Bool {
                    b's'
                } else {
                    b'd'
                };
                self.print_arg(conv, arg)?;
            }
        }
        if newline {
            self.write_text(b"\n");
        }
        Ok(())
    }

    fn print_arg(&mut self, conv: u8, arg: &MirValue) -> CodegenResult<()> {
        let ty = self.value_type(arg)?;
        if conv == b's' {
            let lit = match arg {
                MirValue::Const(MirConst::Str(i)) => Some(*i as usize),
                MirValue::Local(l) => self.str_locals.get(l).copied(),
                _ => None,
            };
            if let Some(i) = lit {
                let bytes = self.strings.get(i).cloned().unwrap_or_default();
                self.write_text(&bytes);
                return Ok(());
            }
            if ty != MirType::Bool {
                return refuse(format!("printing a value of type `{ty}`"));
            }
        }
        let routine = match conv {
            b's' => self.rt.print_bool,
            b'u' => self.rt.print_u64,
            _ if is_signed(&ty) => self.rt.print_i64,
            _ => self.rt.print_u64,
        };
        self.load(arg)?;
        let a0 = self.os.arg_regs()[0];
        self.a.mov_rr(a0, 0);
        self.a.call(routine);
        Ok(())
    }
}

fn rvalue_name(v: &MirRValue) -> &'static str {
    match v {
        MirRValue::Ref { .. } | MirRValue::AddressOf { .. } | MirRValue::AddressOfGlobal { .. } => {
            "reference or address-of"
        }
        MirRValue::Aggregate { .. } => "struct, tuple or array literal",
        MirRValue::Repeat { .. } => "array repeat",
        MirRValue::Discriminant(_) => "enum discriminant",
        MirRValue::Len(_) => "length of",
        MirRValue::NullaryOp(..) => "size_of or align_of",
        MirRValue::FieldAccess { .. } | MirRValue::VariantField { .. } => "field access",
        MirRValue::IndexAccess { .. } => "indexing",
        MirRValue::Deref { .. } => "dereference",
        MirRValue::TextureSample { .. } => "texture sample",
        _ => "expression",
    }
}

fn term_name(t: &MirTerminator) -> &'static str {
    match t {
        MirTerminator::Assert { .. } => "assert",
        MirTerminator::Resume => "resume (unwinding)",
        MirTerminator::Abort => "abort",
        _ => "terminator",
    }
}
