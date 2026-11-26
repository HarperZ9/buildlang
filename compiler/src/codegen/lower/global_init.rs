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
        // An opaque external struct/enum not defined in this module (for
        // example, an imported type in a standalone `buildc check` file) has
        // no field or variant layout here to constant-evaluate against.
        if let MirType::Struct(struct_name) = ty {
            if struct_name.as_ref() != "BuildString" && self.module.find_type(struct_name).is_none()
            {
                return Ok(self.try_const_eval(expr));
            }
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
            (ExprKind::Call { func, args }, MirType::Struct(name)) => self
                .const_math_ctor(name, func, args)
                .or_else(|| self.const_tuple_struct(name, func, args)),
            (ExprKind::Path(p), MirType::Struct(name)) if p.segments.len() >= 2 => {
                let variant = p.last_ident()?.name.as_ref();
                let (disc, fields) = self.lookup_enum_variant(name, variant)?;
                if fields.is_empty() {
                    Some(MirConst::Struct(
                        name.clone(),
                        vec![MirConst::Int(disc, MirType::i32())],
                    ))
                } else {
                    None
                }
            }
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

    /// Evaluate a tuple-struct constant constructor (`CornerFlags(0)`,
    /// `Wrapper(1 << 2)`).
    fn const_tuple_struct(
        &self,
        name: &std::sync::Arc<str>,
        func: &ast::Expr,
        args: &[ast::Expr],
    ) -> Option<MirConst> {
        let callee = match &func.kind {
            ExprKind::Ident(id) => id.name.as_ref(),
            ExprKind::Path(p) => p.last_ident()?.name.as_ref(),
            _ => return None,
        };
        if callee != name.as_ref() && callee != "Self" {
            return None;
        }
        let decl = self.struct_field_list(name)?;
        if decl.len() != args.len() {
            return None;
        }
        let mut values = Vec::with_capacity(decl.len());
        for ((_, fty), arg) in decl.iter().zip(args.iter()) {
            values.push(self.const_eval_typed(arg, fty)?);
        }
        Some(MirConst::Struct(name.clone(), values))
    }

    /// Evaluate pure stdlib math struct constructors (`Vec3::new`, `Mat3::from_rows`,
    /// `Mat3x3::from_rows`, `Mat4::from_cols`, etc.) as compile-time constants.
    fn const_math_ctor(
        &self,
        name: &std::sync::Arc<str>,
        func: &ast::Expr,
        args: &[ast::Expr],
    ) -> Option<MirConst> {
        let ExprKind::Path(p) = &func.kind else {
            return None;
        };
        if p.segments.len() < 2 {
            return None;
        }
        let type_seg = p.segments[p.segments.len() - 2].ident.name.as_ref();
        let method = p.segments[p.segments.len() - 1].ident.name.as_ref();
        let canon = match type_seg {
            "Mat3x3" => "Mat3",
            other => other,
        };
        if canon != name.as_ref() && name.as_ref() != type_seg {
            return None;
        }
        let f32_ty = MirType::Float(crate::codegen::ir::FloatSize::F32);
        let f64_ty = MirType::Float(crate::codegen::ir::FloatSize::F64);
        let mk_float = |e: &ast::Expr, ft: &MirType| self.const_eval_typed(e, ft);
        let zero_f = |ft: &MirType| MirConst::Float(0.0, ft.clone());
        let one_f = |ft: &MirType| MirConst::Float(1.0, ft.clone());

        match (canon, method) {
            ("Vec2", "new") if args.len() == 2 => Some(MirConst::Struct(
                name.clone(),
                vec![mk_float(&args[0], &f32_ty)?, mk_float(&args[1], &f32_ty)?],
            )),
            ("Vec2", "splat") if args.len() == 1 => {
                let v = mk_float(&args[0], &f32_ty)?;
                Some(MirConst::Struct(name.clone(), vec![v.clone(), v]))
            }
            ("Vec2", "zero") if args.is_empty() => Some(MirConst::Struct(
                name.clone(),
                vec![zero_f(&f32_ty), zero_f(&f32_ty)],
            )),
            ("Vec2", "one") if args.is_empty() => Some(MirConst::Struct(
                name.clone(),
                vec![one_f(&f32_ty), one_f(&f32_ty)],
            )),
            ("Vec3", "new") if args.len() == 3 => Some(MirConst::Struct(
                name.clone(),
                vec![
                    mk_float(&args[0], &f32_ty)?,
                    mk_float(&args[1], &f32_ty)?,
                    mk_float(&args[2], &f32_ty)?,
                ],
            )),
            ("Vec3", "splat") if args.len() == 1 => {
                let v = mk_float(&args[0], &f32_ty)?;
                Some(MirConst::Struct(
                    name.clone(),
                    vec![v.clone(), v.clone(), v],
                ))
            }
            ("Vec3", "zero") if args.is_empty() => Some(MirConst::Struct(
                name.clone(),
                vec![zero_f(&f32_ty), zero_f(&f32_ty), zero_f(&f32_ty)],
            )),
            ("Vec3", "one") if args.is_empty() => Some(MirConst::Struct(
                name.clone(),
                vec![one_f(&f32_ty), one_f(&f32_ty), one_f(&f32_ty)],
            )),
            ("DVec3", "new") if args.len() == 3 => Some(MirConst::Struct(
                name.clone(),
                vec![
                    mk_float(&args[0], &f64_ty)?,
                    mk_float(&args[1], &f64_ty)?,
                    mk_float(&args[2], &f64_ty)?,
                ],
            )),
            ("Vec4" | "Quaternion", "new") if args.len() == 4 => Some(MirConst::Struct(
                name.clone(),
                vec![
                    mk_float(&args[0], &f32_ty)?,
                    mk_float(&args[1], &f32_ty)?,
                    mk_float(&args[2], &f32_ty)?,
                    mk_float(&args[3], &f32_ty)?,
                ],
            )),
            ("DQuat", "new") if args.len() == 4 => Some(MirConst::Struct(
                name.clone(),
                vec![
                    mk_float(&args[0], &f64_ty)?,
                    mk_float(&args[1], &f64_ty)?,
                    mk_float(&args[2], &f64_ty)?,
                    mk_float(&args[3], &f64_ty)?,
                ],
            )),
            ("Mat3" | "DMat3", "from_rows") if args.len() == 3 => {
                let vec_name = std::sync::Arc::from(if canon == "Mat3" { "Vec3" } else { "DVec3" });
                let vec_ty = MirType::Struct(vec_name);
                let r0 = match self.const_eval_typed(&args[0], &vec_ty)? {
                    MirConst::Struct(_, fs) if fs.len() == 3 => fs,
                    _ => return None,
                };
                let r1 = match self.const_eval_typed(&args[1], &vec_ty)? {
                    MirConst::Struct(_, fs) if fs.len() == 3 => fs,
                    _ => return None,
                };
                let r2 = match self.const_eval_typed(&args[2], &vec_ty)? {
                    MirConst::Struct(_, fs) if fs.len() == 3 => fs,
                    _ => return None,
                };
                let vn: std::sync::Arc<str> =
                    std::sync::Arc::from(if canon == "Mat3" { "Vec3" } else { "DVec3" });
                let c0 = MirConst::Struct(
                    vn.clone(),
                    vec![r0[0].clone(), r1[0].clone(), r2[0].clone()],
                );
                let c1 = MirConst::Struct(
                    vn.clone(),
                    vec![r0[1].clone(), r1[1].clone(), r2[1].clone()],
                );
                let c2 = MirConst::Struct(vn, vec![r0[2].clone(), r1[2].clone(), r2[2].clone()]);
                Some(MirConst::Struct(name.clone(), vec![c0, c1, c2]))
            }
            ("Mat3" | "DMat3", "from_cols") if args.len() == 3 => {
                let vec_name = std::sync::Arc::from(if canon == "Mat3" { "Vec3" } else { "DVec3" });
                let vec_ty = MirType::Struct(vec_name);
                Some(MirConst::Struct(
                    name.clone(),
                    vec![
                        self.const_eval_typed(&args[0], &vec_ty)?,
                        self.const_eval_typed(&args[1], &vec_ty)?,
                        self.const_eval_typed(&args[2], &vec_ty)?,
                    ],
                ))
            }
            ("Mat3" | "DMat3", "new") if args.len() == 9 => {
                let ft = if canon == "Mat3" { &f32_ty } else { &f64_ty };
                let vn: std::sync::Arc<str> =
                    std::sync::Arc::from(if canon == "Mat3" { "Vec3" } else { "DVec3" });
                let c0 = MirConst::Struct(
                    vn.clone(),
                    vec![
                        mk_float(&args[0], ft)?,
                        mk_float(&args[1], ft)?,
                        mk_float(&args[2], ft)?,
                    ],
                );
                let c1 = MirConst::Struct(
                    vn.clone(),
                    vec![
                        mk_float(&args[3], ft)?,
                        mk_float(&args[4], ft)?,
                        mk_float(&args[5], ft)?,
                    ],
                );
                let c2 = MirConst::Struct(
                    vn,
                    vec![
                        mk_float(&args[6], ft)?,
                        mk_float(&args[7], ft)?,
                        mk_float(&args[8], ft)?,
                    ],
                );
                Some(MirConst::Struct(name.clone(), vec![c0, c1, c2]))
            }
            ("Mat4", "from_cols") if args.len() == 4 => {
                let vec_ty = MirType::Struct(std::sync::Arc::from("Vec4"));
                Some(MirConst::Struct(
                    name.clone(),
                    vec![
                        self.const_eval_typed(&args[0], &vec_ty)?,
                        self.const_eval_typed(&args[1], &vec_ty)?,
                        self.const_eval_typed(&args[2], &vec_ty)?,
                        self.const_eval_typed(&args[3], &vec_ty)?,
                    ],
                ))
            }
            _ => None,
        }
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
