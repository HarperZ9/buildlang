//! Initializers of `static` and `const` items of aggregate type.
//!
//! A global's initializer is a MIR constant. Two defects lived here: an array
//! initializer had no constant form, so `static mut T: [u32; 3] = [7, 8, 9]`
//! lost its values and the C global was zero (G34, G5); and a struct literal
//! kept the order its fields were written in, so `T { z: 5, a: 1 }` became
//! the positional `{ 5, 1 }` and set `a` to 5. Here the initializer is
//! evaluated against the declared type: one constant per element, struct
//! fields in declaration order, integers carrying the declared width.

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::*;

use super::MirLowerer;

/// Whether `ty` holds a fixed array. The initializer of such a global had no
/// constant form before, so no program relied on the old evaluation of it.
fn holds_array(lowerer: &MirLowerer<'_>, ty: &MirType) -> bool {
    match ty {
        MirType::Array(..) => true,
        MirType::Struct(name) => lowerer
            .struct_field_list(name)
            .is_some_and(|fields| fields.iter().any(|(_, fty)| holds_array(lowerer, fty))),
        MirType::Tuple(elems) => elems.iter().any(|t| holds_array(lowerer, t)),
        _ => false,
    }
}

fn is_aggregate(ty: &MirType) -> bool {
    matches!(
        ty,
        MirType::Array(..) | MirType::Struct(_) | MirType::Tuple(_)
    )
}

/// `value` as a constant of the scalar type `ty`, or `None` when it does not
/// fit or is not a scalar.
fn retype_scalar(value: MirConst, ty: &MirType) -> Option<MirConst> {
    let (bits, signed) = match ty {
        MirType::Int(size, signed) => (int_bits(*size)?, *signed),
        MirType::Bool => return matches!(value, MirConst::Bool(_)).then_some(value),
        MirType::Float(_) => {
            return match value {
                MirConst::Float(v, _) => Some(MirConst::Float(v, ty.clone())),
                _ => None,
            }
        }
        _ => return None,
    };
    let v: i128 = match value {
        MirConst::Int(v, _) => v,
        MirConst::Uint(v, _) => i128::try_from(v).ok()?,
        _ => return None,
    };
    let (lo, hi) = if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    };
    if v < lo || v > hi {
        return None;
    }
    Some(if signed {
        MirConst::Int(v, ty.clone())
    } else {
        MirConst::Uint(v as u128, ty.clone())
    })
}

/// Whether every bit of `c` is zero, so `[c; N]` is a zero fill.
fn is_zero(c: &MirConst) -> bool {
    match c {
        MirConst::Int(v, _) => *v == 0,
        MirConst::Uint(v, _) => *v == 0,
        MirConst::Bool(b) => !*b,
        MirConst::Float(v, _) => v.to_bits() == 0,
        MirConst::Zeroed(_) => true,
        MirConst::Array(_, vs) | MirConst::Struct(_, vs) => vs.iter().all(is_zero),
        _ => false,
    }
}

fn int_bits(size: IntSize) -> Option<u32> {
    match size {
        IntSize::I8 => Some(8),
        IntSize::I16 => Some(16),
        IntSize::I32 => Some(32),
        IntSize::I64 | IntSize::ISize => Some(64),
        IntSize::I128 => None,
    }
}

impl<'ctx> MirLowerer<'ctx> {
    /// The fields of struct `name` in declaration order.
    pub(super) fn struct_field_list(
        &self,
        name: &str,
    ) -> Option<Vec<(Option<std::sync::Arc<str>>, MirType)>> {
        match &self.module.find_type(name)?.kind {
            TypeDefKind::Struct { fields, .. } => Some(fields.clone()),
            _ => None,
        }
    }

    /// The initializer of global `name` of type `ty`. An aggregate gets a
    /// constant for every member or a compile error naming the global: with
    /// no constant the C global was zero, a silent wrong answer. A struct
    /// with no array member keeps the old evaluation when only that one
    /// succeeds, so no program that compiled before loses its initializer.
    pub(super) fn global_initializer(
        &self,
        name: &str,
        expr: &ast::Expr,
        ty: &MirType,
    ) -> CodegenResult<Option<MirConst>> {
        // A string constant is the string struct itself: pointer, length and a
        // capacity of 0, which marks text that is not on the heap. With no
        // constant the C global was a null string, and printing it crashed.
        if matches!(ty, MirType::Struct(n) if n.as_ref() == "BuildString") {
            if let Some(text) = string_literal(expr) {
                let len = MirConst::Uint(text.len() as u128, MirType::usize());
                return Ok(Some(MirConst::Struct(
                    std::sync::Arc::from("BuildString"),
                    vec![
                        MirConst::ByteStr(text.as_bytes().to_vec()),
                        len,
                        MirConst::Uint(0, MirType::usize()),
                    ],
                )));
            }
        }
        if !is_aggregate(ty) {
            return self.scalar_initializer(name, expr, ty);
        }
        if let Some(c) = self.const_eval_typed(expr, ty) {
            return Ok(Some(c));
        }
        if !holds_array(self, ty) {
            if let Some(c) = self.try_const_eval(expr) {
                return Ok(Some(c));
            }
        }
        let span = expr.span;
        Err(CodegenError::Rejected {
            message: format!(
                "the initializer of `{name}` is not a constant the compiler can evaluate"
            ),
            location: (span.end.0 > span.start.0).then_some((span.start.0, span.end.0)),
            help: Some(
                "write it with literals: `[1, 2, 3]`, `[0; N]` and struct literals that name every field"
                    .to_string(),
            ),
        })
    }

    /// The initializer of a global that is not an aggregate. A number or a
    /// boolean gets its value or a compile error: with no constant the C global
    /// was zero, so `const N: i32 = 3 + 4;` read as 0 with no diagnostic. Other
    /// types keep the earlier evaluation.
    fn scalar_initializer(
        &self,
        name: &str,
        expr: &ast::Expr,
        ty: &MirType,
    ) -> CodegenResult<Option<MirConst>> {
        let value = self.try_const_eval(expr);
        let refuse = |why: &str| {
            let span = expr.span;
            Err(CodegenError::Rejected {
                message: format!("the initializer of `{name}` {why}"),
                location: (span.end.0 > span.start.0).then_some((span.start.0, span.end.0)),
                help: Some(
                    "use literals, other constants, arithmetic, comparisons and `as` casts between number types"
                        .to_string(),
                ),
            })
        };
        match ty {
            MirType::Bool => match value {
                Some(MirConst::Bool(_)) => Ok(value),
                _ => refuse("is not a boolean the compiler can evaluate"),
            },
            MirType::Float(_) => match value {
                Some(MirConst::Float(v, _)) => Ok(Some(MirConst::Float(v, ty.clone()))),
                Some(MirConst::Int(v, _)) => Ok(Some(MirConst::Float(v as f64, ty.clone()))),
                Some(MirConst::Uint(v, _)) => Ok(Some(MirConst::Float(v as f64, ty.clone()))),
                _ => refuse("is not a number the compiler can evaluate"),
            },
            MirType::Int(..) => match value {
                Some(MirConst::Int(..)) | Some(MirConst::Uint(..)) => Ok(value),
                _ => refuse("is not an integer the compiler can evaluate"),
            },
            _ => Ok(value),
        }
    }

    /// `expr` as a constant of type `ty`, or `None` when it is not one.
    fn const_eval_typed(&self, expr: &ast::Expr, ty: &MirType) -> Option<MirConst> {
        match (&expr.kind, ty) {
            (ExprKind::Paren(inner), _) => self.const_eval_typed(inner, ty),
            (ExprKind::Array(elems), MirType::Array(elem, n)) => {
                if elems.len() as u64 != *n {
                    return None;
                }
                let values: Option<Vec<MirConst>> = elems
                    .iter()
                    .map(|e| self.const_eval_typed(e, elem))
                    .collect();
                Some(MirConst::Array((**elem).clone(), values?))
            }
            (ExprKind::ArrayRepeat { element, count }, MirType::Array(elem, n)) => {
                let count = match self.try_const_eval(count)? {
                    MirConst::Int(v, _) => u64::try_from(v).ok()?,
                    MirConst::Uint(v, _) => u64::try_from(v).ok()?,
                    _ => return None,
                };
                if count != *n {
                    return None;
                }
                let value = self.const_eval_typed(element, elem)?;
                // A zero fill stays one constant: a kernel stack or page
                // table of thousands of elements is `{0}` in the C.
                if is_zero(&value) {
                    return Some(MirConst::Zeroed(ty.clone()));
                }
                Some(MirConst::Array(
                    (**elem).clone(),
                    vec![value; count as usize],
                ))
            }
            (
                ExprKind::Struct {
                    fields, rest: None, ..
                },
                MirType::Struct(name),
            ) => self.const_struct(name, fields),
            (ExprKind::Tuple(elems), MirType::Tuple(tys)) if elems.len() == tys.len() => {
                let values: Option<Vec<MirConst>> = elems
                    .iter()
                    .zip(tys)
                    .map(|(e, t)| self.const_eval_typed(e, t))
                    .collect();
                Some(MirConst::Struct(MirType::tuple_type_name(tys), values?))
            }
            (_, MirType::Array(..) | MirType::Struct(_) | MirType::Tuple(_)) => None,
            _ => retype_scalar(self.try_const_eval(expr)?, ty),
        }
    }

    /// A struct literal as a constant with its fields in declaration order.
    /// Every field must be written once.
    fn const_struct(
        &self,
        name: &std::sync::Arc<str>,
        fields: &[ast::FieldExpr],
    ) -> Option<MirConst> {
        let decl = self.struct_field_list(name)?;
        if decl.len() != fields.len() {
            return None;
        }
        let mut values = Vec::with_capacity(decl.len());
        for (fname, fty) in &decl {
            let fname = fname.as_ref()?;
            let field = fields.iter().find(|f| f.name.name == *fname)?;
            let value = field.value.as_ref()?;
            values.push(self.const_eval_typed(value, fty)?);
        }
        Some(MirConst::Struct(name.clone(), values))
    }
}

/// The text of `expr` when it is a string literal, looking through parentheses.
pub(super) fn string_literal(expr: &ast::Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Paren(inner) => string_literal(inner),
        ExprKind::Literal(ast::Literal::Str { value, .. }) => Some(value.as_ref()),
        _ => None,
    }
}
