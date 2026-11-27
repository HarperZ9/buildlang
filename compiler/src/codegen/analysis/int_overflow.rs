// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Cross-backend integer overflow analysis for GPU shader targets.
//!
//! BuildLang defines plain integer `+`, `-`, and `*` as checked non-overflowing
//! arithmetic across every target:
//! - On CPU (`Target::C`), plain integer arithmetic traps at runtime (`exit 101`).
//! - On GPU shader targets (`HLSL`, `GLSL`, `WGSL`, `MSL`, `SPIR-V`), hardware
//!   compute lanes cannot raise a host trap, so this MIR pass rejects fail-closed
//!   at compile time any plain integer `+`, `-`, or `*` that either overflows on
//!   compile-time constants or operates on unguarded integer data values.
//! - Explicit `.wrapping_add`, `.wrapping_sub`, and `.wrapping_mul` lower to
//!   `BinOp::{Add,Sub,Mul}Wrapping` and compile to native modular two's-complement
//!   arithmetic (`mod 2^W`) on every target.

use std::collections::{HashMap, HashSet};

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::{
    BinOp, IntSize, LocalId, MirConst, MirFunction, MirModule, MirRValue, MirStmtKind,
    MirTerminator, MirType, MirValue, NullaryOp, UnaryOp,
};
use crate::codegen::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntOrigin {
    /// Exact compile-time signed integer constant.
    ConstSigned(i128),
    /// Exact compile-time unsigned integer constant.
    ConstUnsigned(u128),
    /// Dispatch coordinate or bounded loop index (e.g. `gl_GlobalInvocationID`).
    DispatchIndex,
    /// Scalar uniform parameter (may be bounded when used in `< param` / index).
    UniformParam,
    /// Arbitrary data-derived integer (loaded from buffer, bitwise-mixed, or
    /// returned from a call/wrapping op). Plain `+`, `-`, `*` on `Data` can
    /// overflow silently on GPU and must use explicit `.wrapping_*`.
    Data,
}

impl IntOrigin {
    fn join(self, other: Self) -> Self {
        if self == other {
            return self;
        }
        match (self, other) {
            (Self::Data, _) | (_, Self::Data) => Self::Data,
            (Self::UniformParam, _) | (_, Self::UniformParam) => Self::UniformParam,
            _ => Self::DispatchIndex,
        }
    }
}

fn int_bounds(size: IntSize, signed: bool) -> (i128, i128) {
    match (size, signed) {
        (IntSize::I8, true) => (i8::MIN as i128, i8::MAX as i128),
        (IntSize::I16, true) => (i16::MIN as i128, i16::MAX as i128),
        (IntSize::I32, true) => (i32::MIN as i128, i32::MAX as i128),
        (IntSize::I64 | IntSize::ISize, true) => (i64::MIN as i128, i64::MAX as i128),
        (IntSize::I128, true) => (i128::MIN, i128::MAX),
        (IntSize::I8, false) => (0, u8::MAX as i128),
        (IntSize::I16, false) => (0, u16::MAX as i128),
        (IntSize::I32, false) => (0, u32::MAX as i128),
        (IntSize::I64 | IntSize::ISize, false) => (0, u64::MAX as i128),
        (IntSize::I128, false) => (0, i128::MAX),
    }
}

fn eval_const_overflow(
    op: BinOp,
    l: IntOrigin,
    r: IntOrigin,
    size: IntSize,
    signed: bool,
) -> Option<Result<IntOrigin, String>> {
    let to_i128 = |o: IntOrigin| -> Option<i128> {
        match o {
            IntOrigin::ConstSigned(v) => Some(v),
            IntOrigin::ConstUnsigned(v) => i128::try_from(v).ok(),
            _ => None,
        }
    };
    let lv = to_i128(l)?;
    let rv = to_i128(r)?;
    let (min_v, max_v) = int_bounds(size, signed);
    let exact = match op {
        BinOp::Add => lv.checked_add(rv),
        BinOp::Sub => lv.checked_sub(rv),
        BinOp::Mul => lv.checked_mul(rv),
        _ => return None,
    };
    match exact {
        Some(res) if res >= min_v && res <= max_v => {
            if signed {
                Some(Ok(IntOrigin::ConstSigned(res)))
            } else {
                Some(Ok(IntOrigin::ConstUnsigned(res as u128)))
            }
        }
        _ => Some(Err(format!(
            "constant integer expression ({lv} {} {rv}) overflows {}{}",
            op_symbol(op),
            if signed { "i" } else { "u" },
            size_bits(size)
        ))),
    }
}

fn size_bits(size: IntSize) -> u32 {
    match size {
        IntSize::I8 => 8,
        IntSize::I16 => 16,
        IntSize::I32 => 32,
        IntSize::I64 | IntSize::ISize => 64,
        IntSize::I128 => 128,
    }
}

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        _ => "?",
    }
}

fn wrapping_method_for(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "wrapping_add",
        BinOp::Sub => "wrapping_sub",
        BinOp::Mul => "wrapping_mul",
        _ => "wrapping_add",
    }
}

fn value_origin(val: &MirValue, origins: &HashMap<LocalId, IntOrigin>) -> IntOrigin {
    match val {
        MirValue::Const(MirConst::Int(v, _)) => IntOrigin::ConstSigned(*v),
        MirValue::Const(MirConst::Uint(v, _)) => IntOrigin::ConstUnsigned(*v),
        MirValue::Local(id) => origins.get(id).copied().unwrap_or(IntOrigin::Data),
        _ => IntOrigin::Data,
    }
}

fn is_const_zero(o: IntOrigin) -> bool {
    matches!(o, IntOrigin::ConstSigned(0) | IntOrigin::ConstUnsigned(0))
}

fn is_const_one(o: IntOrigin) -> bool {
    matches!(o, IntOrigin::ConstSigned(1) | IntOrigin::ConstUnsigned(1))
}

fn value_int_type(val: &MirValue, func: &MirFunction) -> Option<(IntSize, bool)> {
    match val {
        MirValue::Local(id) => match func.locals.get(id.0 as usize).map(|l| &l.ty) {
            Some(MirType::Int(sz, signed)) => Some((*sz, *signed)),
            _ => None,
        },
        MirValue::Const(MirConst::Int(_, MirType::Int(sz, signed))) => Some((*sz, *signed)),
        MirValue::Const(MirConst::Uint(_, MirType::Int(sz, signed))) => Some((*sz, *signed)),
        _ => None,
    }
}

fn mark_rvalue_sink_locals(rvalue: &MirRValue, sinks: &mut HashSet<LocalId>) -> bool {
    let mut added = false;
    match rvalue {
        MirRValue::Use(MirValue::Local(src))
        | MirRValue::UnaryOp {
            operand: MirValue::Local(src),
            ..
        }
        | MirRValue::Cast {
            value: MirValue::Local(src),
            ..
        } => {
            if sinks.insert(*src) {
                added = true;
            }
        }
        MirRValue::BinaryOp { left, right, .. } => {
            if let MirValue::Local(l) = left {
                if sinks.insert(*l) {
                    added = true;
                }
            }
            if let MirValue::Local(r) = right {
                if sinks.insert(*r) {
                    added = true;
                }
            }
        }
        _ => {}
    }
    added
}

fn check_function(func: &MirFunction, target: Target) -> CodegenResult<()> {
    let Some(blocks) = &func.blocks else {
        return Ok(());
    };

    let mut origins: HashMap<LocalId, IntOrigin> = HashMap::new();
    for local in &func.locals {
        if local.is_param && matches!(local.ty, MirType::Int(_, _)) {
            origins.insert(local.id, IntOrigin::UniformParam);
        }
    }

    let mut lt_lhs_locals: HashSet<LocalId> = HashSet::new();
    let mut lt_rhs_locals: HashSet<LocalId> = HashSet::new();
    let mut zero_checked_locals: HashSet<LocalId> = HashSet::new();
    let mut int_data_sink_locals: HashSet<LocalId> = HashSet::new();

    for block in blocks {
        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign {
                    value: MirRValue::BinaryOp { op, left, right },
                    ..
                } => match op {
                    BinOp::Lt => {
                        if let MirValue::Local(id) = left {
                            lt_lhs_locals.insert(*id);
                        }
                        if let MirValue::Local(id) = right {
                            lt_rhs_locals.insert(*id);
                        }
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Gt => {
                        if let (
                            MirValue::Local(id),
                            MirValue::Const(MirConst::Int(0, _) | MirConst::Uint(0, _)),
                        ) = (left, right)
                        {
                            zero_checked_locals.insert(*id);
                        }
                    }
                    _ => {}
                },
                MirStmtKind::IndexStore { elem_ty, value, .. } => {
                    if matches!(elem_ty, MirType::Int(_, _)) {
                        mark_rvalue_sink_locals(value, &mut int_data_sink_locals);
                    }
                }
                _ => {}
            }
        }
        if let Some(MirTerminator::Return(Some(MirValue::Local(id)))) = &block.terminator {
            if value_int_type(&MirValue::Local(*id), func).is_some() {
                int_data_sink_locals.insert(*id);
            }
        }
    }

    // Back-propagate integer data sinks through plain assignments and binary ops.
    let mut sink_changed = true;
    while sink_changed {
        sink_changed = false;
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                    if int_data_sink_locals.contains(dest)
                        && mark_rvalue_sink_locals(value, &mut int_data_sink_locals)
                    {
                        sink_changed = true;
                    }
                }
            }
        }
    }

    // Forward fixed-point classification of integer local origins.
    let mut changed = true;
    let mut iters = 0usize;
    while changed && iters < 32 {
        changed = false;
        iters += 1;
        for block in blocks {
            for stmt in &block.stmts {
                if let MirStmtKind::Assign { dest, value } = &stmt.kind {
                    let dest_ty = func.locals.get(dest.0 as usize).map(|l| &l.ty);
                    if !matches!(dest_ty, Some(MirType::Int(_, _))) {
                        continue;
                    }
                    let new_origin = match value {
                        MirRValue::Use(v) => value_origin(v, &origins),
                        MirRValue::NullaryOp(
                            NullaryOp::ThreadIndex(_)
                            | NullaryOp::LocalInvocationId(_)
                            | NullaryOp::WorkgroupId(_),
                            _,
                        ) => IntOrigin::DispatchIndex,
                        MirRValue::IndexAccess { .. } | MirRValue::Deref { .. } => IntOrigin::Data,
                        MirRValue::BinaryOp { op, left, right } => {
                            let lo = value_origin(left, &origins);
                            let ro = value_origin(right, &origins);
                            if matches!(
                                op,
                                BinOp::AddWrapping | BinOp::SubWrapping | BinOp::MulWrapping
                            ) {
                                IntOrigin::Data
                            } else if matches!(
                                op,
                                BinOp::BitXor | BinOp::BitOr | BinOp::Shl | BinOp::Shr
                            ) {
                                if matches!(lo, IntOrigin::Data) || matches!(ro, IntOrigin::Data) {
                                    IntOrigin::Data
                                } else {
                                    lo.join(ro)
                                }
                            } else if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
                                if let Some(MirType::Int(sz, signed)) = dest_ty {
                                    if let Some(Ok(folded)) =
                                        eval_const_overflow(*op, lo, ro, *sz, *signed)
                                    {
                                        folded
                                    } else {
                                        lo.join(ro)
                                    }
                                } else {
                                    lo.join(ro)
                                }
                            } else {
                                lo.join(ro)
                            }
                        }
                        MirRValue::UnaryOp {
                            op: UnaryOp::BitNot,
                            ..
                        } => IntOrigin::Data,
                        MirRValue::Cast { value, .. } => value_origin(value, &origins),
                        _ => IntOrigin::Data,
                    };

                    let merged = match origins.get(dest).copied() {
                        Some(prev) => prev.join(new_origin),
                        None => new_origin,
                    };
                    if origins.get(dest).copied() != Some(merged) {
                        origins.insert(*dest, merged);
                        changed = true;
                    }
                }
            }
            if let Some(MirTerminator::Call {
                dest: Some(dest_id),
                ..
            }) = &block.terminator
            {
                if origins.get(dest_id).copied() != Some(IntOrigin::Data) {
                    origins.insert(*dest_id, IntOrigin::Data);
                    changed = true;
                }
            }
        }
    }

    // Helper to validate a single plain integer BinaryOp rvalue.
    let check_rvalue = |dest: Option<LocalId>, rvalue: &MirRValue| -> CodegenResult<()> {
        let MirRValue::BinaryOp { op, left, right } = rvalue else {
            return Ok(());
        };
        if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul) {
            return Ok(());
        }
        let Some((size, signed)) = value_int_type(left, func)
            .or_else(|| value_int_type(right, func))
            .or_else(|| {
                dest.and_then(|d| match func.locals.get(d.0 as usize).map(|l| &l.ty) {
                    Some(MirType::Int(sz, s)) => Some((*sz, *s)),
                    _ => None,
                })
            })
        else {
            return Ok(());
        };

        let lo = value_origin(left, &origins);
        let ro = value_origin(right, &origins);

        // 1. Constant overflow check (always fail-closed).
        if let Some(const_eval) = eval_const_overflow(*op, lo, ro, size, signed) {
            if let Err(detail) = const_eval {
                return Err(CodegenError::Rejected {
                    message: format!(
                        "integer overflow in `{}` on GPU target `{target}` in function `{}`: {detail}",
                        op_symbol(*op),
                        func.name
                    ),
                    location: None,
                    help: Some(format!(
                        "use `.{}(...)` for explicit modular two's-complement arithmetic",
                        wrapping_method_for(*op)
                    )),
                });
            }
            return Ok(());
        }

        // 2. Identity no-ops cannot overflow.
        let is_identity = match op {
            BinOp::Add => is_const_zero(lo) || is_const_zero(ro),
            BinOp::Sub => is_const_zero(ro),
            BinOp::Mul => {
                is_const_zero(lo) || is_const_zero(ro) || is_const_one(lo) || is_const_one(ro)
            }
            _ => false,
        };
        if is_identity {
            return Ok(());
        }

        // 3. Guarded loop-counter increment (`x + 1` where `x < upper`) or
        //    guarded non-zero decrement (`x - 1` where `x == 0`/`x > 0` or `i < x`).
        if *op == BinOp::Add && is_const_one(ro) {
            if let MirValue::Local(id) = left {
                if lt_lhs_locals.contains(id) {
                    return Ok(());
                }
            }
        }
        if *op == BinOp::Sub && is_const_one(ro) {
            if let MirValue::Local(id) = left {
                if zero_checked_locals.contains(id) || lt_rhs_locals.contains(id) {
                    return Ok(());
                }
            }
        }

        // 4. Reject any plain `+`, `-`, `*` on data-derived integer operands
        //    or directly storing into an integer data buffer / integer return.
        let touches_data = matches!(lo, IntOrigin::Data) || matches!(ro, IntOrigin::Data);
        let feeds_int_sink =
            dest.is_none() || dest.is_some_and(|d| int_data_sink_locals.contains(&d));
        if touches_data || feeds_int_sink {
            let ty_name = format!("{}{}", if signed { "i" } else { "u" }, size_bits(size));
            return Err(CodegenError::Rejected {
                message: format!(
                    "plain integer `{}` on `{ty_name}` in GPU target `{target}` function `{}` cannot be proven overflow-free; GPU lanes cannot trap on overflow",
                    op_symbol(*op),
                    func.name
                ),
                location: None,
                help: Some(format!(
                    "use `.{}(...)` for explicit modular wrap-around consistent across CPU and GPU",
                    wrapping_method_for(*op)
                )),
            });
        }

        Ok(())
    };

    for block in blocks {
        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign { dest, value } => check_rvalue(Some(*dest), value)?,
                MirStmtKind::IndexStore { elem_ty, value, .. }
                    if matches!(elem_ty, MirType::Int(_, _)) =>
                {
                    check_rvalue(None, value)?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Verify cross-backend checked integer overflow rules before emitting GPU shader code.
pub(crate) fn check_gpu_int_overflow(mir: &MirModule, target: Target) -> CodegenResult<()> {
    for func in &mir.functions {
        check_function(func, target)?;
    }
    Ok(())
}
