//! `&v[a..b]` on a vector: a slice view of the elements in the range.
//!
//! The range used to be lowered as an element index, so the result was typed
//! as one element and `.len()`, indexing and `for` over it were refused. The
//! view here is the fat pointer `{ data + a, b - a }`, the same `&[T]` value a
//! slice parameter receives. The runtime checks `a <= b <= len` and stops the
//! program with 101 otherwise, as Rust does.

use std::sync::Arc;

use crate::ast;
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;

use super::MirLowerer;

impl MirLowerer<'_> {
    /// The slice view of `vec[start..end]`, or `None` when `vec` is not a
    /// vector (a string range is a substring and is handled elsewhere).
    pub(super) fn vec_range_view(
        &mut self,
        vec: &MirValue,
        start: Option<&ast::Expr>,
        end: Option<&ast::Expr>,
        inclusive: bool,
    ) -> CodegenResult<Option<MirValue>> {
        let vec = match self.type_of_value(vec) {
            MirType::Ptr(inner) if matches!(*inner, MirType::Vec(_)) => {
                self.deref_if_pointer(vec.clone())?
            }
            _ => vec.clone(),
        };
        let MirType::Vec(elem) = self.type_of_value(&vec) else {
            return Ok(None);
        };
        let i64_ty = MirType::i64();
        let start_val = match start {
            Some(e) => self.lower_expr(e)?,
            None => MirValue::Const(MirConst::Int(0, i64_ty.clone())),
        };
        let end_val = match end {
            Some(e) => Some(self.lower_expr(e)?),
            None => None,
        };
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let start_l = builder.create_local(i64_ty.clone());
        builder.assign(start_l, MirRValue::Use(start_val));
        let end_l = builder.create_local(i64_ty.clone());
        match end_val {
            Some(v) => builder.assign(end_l, MirRValue::Use(v)),
            None => {
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("build_hvec_len")),
                    vec![vec.clone()],
                    Some(end_l),
                    cont,
                );
                builder.switch_to_block(cont);
            }
        }
        if inclusive {
            builder.binary_op(
                end_l,
                BinOp::Add,
                values::local(end_l),
                MirValue::Const(MirConst::Int(1, i64_ty.clone())),
            );
        }
        let size = builder.create_local(MirType::usize());
        builder.assign(
            size,
            MirRValue::NullaryOp(NullaryOp::SizeOf, (*elem).clone()),
        );
        let data = builder.create_local(MirType::Ptr(elem.clone()));
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("build_hvec_range")),
            vec![
                vec,
                values::local(size),
                values::local(start_l),
                values::local(end_l),
            ],
            Some(data),
            cont,
        );
        builder.switch_to_block(cont);
        let len = builder.create_local(MirType::usize());
        builder.binary_op(
            len,
            BinOp::Sub,
            values::local(end_l),
            values::local(start_l),
        );
        let slice_ty = MirType::Slice(elem.clone());
        let fat = builder.create_local(slice_ty.clone());
        builder.aggregate(
            fat,
            AggregateKind::Struct(MirType::slice_type_name(&elem)),
            vec![values::local(data), values::local(len)],
        );
        let result = builder.create_local(MirType::Ptr(Box::new(slice_ty)));
        builder.assign(
            result,
            MirRValue::AddressOf {
                is_mut: false,
                place: MirPlace::local(fat),
            },
        );
        Ok(Some(values::local(result)))
    }

    /// The slice view of `s[start..end]` for a slice (`&[T]`) or a fixed
    /// array (`[T; N]`), or `None` for any other base. The range used to be
    /// lowered as an element index of type unit, which left the generated C
    /// reading an undeclared temporary wherever the view was used.
    pub(super) fn slice_range_view(
        &mut self,
        base: &MirValue,
        start: Option<&ast::Expr>,
        end: Option<&ast::Expr>,
        inclusive: bool,
    ) -> CodegenResult<Option<MirValue>> {
        let ty = self.type_of_value(base);
        let (elem, fixed_len) = match &ty {
            MirType::Array(e, n) => ((**e).clone(), Some(*n)),
            MirType::Slice(e) => ((**e).clone(), None),
            MirType::Ptr(inner) => match &**inner {
                MirType::Slice(e) => ((**e).clone(), None),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        let i64_ty = MirType::i64();
        let start_val = match start {
            Some(e) => self.lower_expr(e)?,
            None => MirValue::Const(MirConst::Int(0, i64_ty.clone())),
        };
        let end_val = match end {
            Some(e) => Some(self.lower_expr(e)?),
            None => None,
        };
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let ptr_ty = MirType::Ptr(Box::new(elem.clone()));
        let (data_v, len_v) = match fixed_len {
            Some(n) => (
                base.clone(),
                MirValue::Const(MirConst::Int(n as i128, i64_ty.clone())),
            ),
            None => {
                let data = builder.create_local(ptr_ty.clone());
                builder.assign(
                    data,
                    MirRValue::FieldAccess {
                        base: base.clone(),
                        field_name: Arc::from("ptr"),
                        field_ty: ptr_ty.clone(),
                    },
                );
                let len = builder.create_local(i64_ty.clone());
                builder.assign(
                    len,
                    MirRValue::FieldAccess {
                        base: base.clone(),
                        field_name: Arc::from("len"),
                        field_ty: i64_ty.clone(),
                    },
                );
                (values::local(data), values::local(len))
            }
        };
        let len_l = builder.create_local(i64_ty.clone());
        builder.assign(len_l, MirRValue::Use(len_v));
        let start_l = builder.create_local(i64_ty.clone());
        builder.assign(start_l, MirRValue::Use(start_val));
        let end_l = builder.create_local(i64_ty.clone());
        let end_src = end_val.unwrap_or_else(|| values::local(len_l));
        builder.assign(end_l, MirRValue::Use(end_src));
        if inclusive {
            builder.binary_op(
                end_l,
                BinOp::Add,
                values::local(end_l),
                MirValue::Const(MirConst::Int(1, i64_ty.clone())),
            );
        }
        let size = builder.create_local(MirType::usize());
        builder.assign(size, MirRValue::NullaryOp(NullaryOp::SizeOf, elem.clone()));
        let data = builder.create_local(ptr_ty);
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("build_slice_range")),
            vec![
                data_v,
                values::local(len_l),
                values::local(size),
                values::local(start_l),
                values::local(end_l),
            ],
            Some(data),
            cont,
        );
        builder.switch_to_block(cont);
        let len = builder.create_local(MirType::usize());
        builder.binary_op(
            len,
            BinOp::Sub,
            values::local(end_l),
            values::local(start_l),
        );
        let elem_box = Box::new(elem);
        let slice_ty = MirType::Slice(elem_box.clone());
        let fat = builder.create_local(slice_ty.clone());
        builder.aggregate(
            fat,
            AggregateKind::Struct(MirType::slice_type_name(&elem_box)),
            vec![values::local(data), values::local(len)],
        );
        let result = builder.create_local(MirType::Ptr(Box::new(slice_ty)));
        builder.assign(
            result,
            MirRValue::AddressOf {
                is_mut: false,
                place: MirPlace::local(fat),
            },
        );
        Ok(Some(values::local(result)))
    }
}
