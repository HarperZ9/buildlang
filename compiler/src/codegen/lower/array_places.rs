//! Fixed arrays reached through a field, an element or a reference.
//!
//! An array that sits inside a struct, inside another array or behind a
//! reference used to be read into a temporary first. A store then went to
//! the temporary (`q.w[1] = 42` was lost, G27), and a borrow of it as a
//! slice passed the wrong thing (`f(&mut s.bits)`, G7; a `&Vec<T>` or
//! `&[T; N]` parameter handed on to a `&[T]` parameter, G31). The functions
//! here address the array where it lives.

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;
use crate::lexer::Span;

use super::MirLowerer;

/// `expr` without its parentheses.
fn peel(expr: &ast::Expr) -> &ast::Expr {
    match &expr.kind {
        ExprKind::Paren(inner) => peel(inner),
        _ => expr,
    }
}

/// A plain name: a local, a parameter or a global.
pub(super) fn is_named(expr: &ast::Expr) -> bool {
    matches!(peel(expr).kind, ExprKind::Ident(_) | ExprKind::Path(_))
}

/// Built from names, fields and dereferences only, so making a place from it
/// emits no statement and can be tried without running anything twice.
fn is_static_path(expr: &ast::Expr) -> bool {
    match &expr.kind {
        ExprKind::Ident(_) => true,
        ExprKind::Paren(e) | ExprKind::Deref(e) => is_static_path(e),
        ExprKind::Field { expr: e, .. } | ExprKind::TupleField { expr: e, .. } => is_static_path(e),
        _ => false,
    }
}

fn location(span: Span) -> Option<(u32, u32)> {
    (span.end.0 > span.start.0).then_some((span.start.0, span.end.0))
}

fn element_mismatch(what: &str, have: &MirType, elem: &MirType, span: Span) -> CodegenError {
    CodegenError::Rejected {
        message: format!("this {what} holds `{have}` but the parameter is a slice of `{elem}`"),
        location: location(span),
        help: Some(format!("give the {what} the element type `{elem}`")),
    }
}

impl<'ctx> MirLowerer<'ctx> {
    /// Whether `expr` is a fixed array held by value that is reached through
    /// a projection (`s.w`, `a[i]`, `(*p).w`), or is a mutable global array,
    /// rather than a plain local.
    pub(super) fn is_projected_array(&mut self, expr: &ast::Expr) -> CodegenResult<bool> {
        if is_named(expr) {
            return Ok(self.is_global_array_name(expr));
        }
        if matches!(self.infer_expr_type(expr), MirType::Array(..)) {
            return Ok(true);
        }
        if !is_static_path(expr) {
            return Ok(false);
        }
        Ok(matches!(
            self.lower_place(expr)?,
            Some((_, MirType::Array(..)))
        ))
    }

    /// `s.w[i] = v`, `a[i][j] = v`, `xs[i].w[j] += v`: store through the
    /// address of the element. `None` when the indexed array is a plain local
    /// or is not a fixed array, which the caller's own paths handle.
    pub(super) fn store_into_projected_array(
        &mut self,
        op: ast::AssignOp,
        target: &ast::Expr,
        arr: &ast::Expr,
        val: &MirValue,
    ) -> CodegenResult<Option<MirValue>> {
        if !self.is_projected_array(arr)? {
            return Ok(None);
        }
        let Some((place, place_ty)) = self.lower_place(target)? else {
            return Err(Self::unaddressable_array(arr.span));
        };
        let store = self.compound_store_value(op, target, val.clone(), &place_ty)?;
        self.store_through_place(place, place_ty, MirRValue::Use(store))?;
        Ok(Some(values::unit()))
    }

    /// The error for a store into an array the compiler would have to copy
    /// first. The store used to land in the copy and vanish.
    pub(super) fn unaddressable_array(span: Span) -> CodegenError {
        CodegenError::Rejected {
            message: "cannot store into an element of this array: it is a temporary copy, \
                      so the store would be lost"
                .to_string(),
            location: location(span),
            help: Some(
                "bind the array to a `let mut` local, store into the local, then write it back"
                    .to_string(),
            ),
        }
    }

    /// The error for a field store into a value the compiler would have to
    /// copy first (a global struct, a call result).
    pub(super) fn unaddressable_field(span: Span) -> CodegenError {
        CodegenError::Rejected {
            message: "cannot assign to a field of this value: it is a temporary copy, \
                      so the assignment would be lost"
                .to_string(),
            location: location(span),
            help: Some(
                "copy the value to a `let mut` local, assign to the local, then store it back"
                    .to_string(),
            ),
        }
    }

    /// `{ ptr, len }` for a `&[elem]`, and the address of it.
    fn slice_from_parts(
        &mut self,
        ptr: MirValue,
        len: MirValue,
        elem: &MirType,
        target: &MirType,
    ) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let fat = builder.create_local(target.clone());
        builder.aggregate(
            fat,
            AggregateKind::Struct(MirType::slice_type_name(elem)),
            vec![ptr, len],
        );
        let result = builder.create_local(MirType::Ptr(Box::new(target.clone())));
        builder.assign(
            result,
            MirRValue::AddressOf {
                is_mut: false,
                place: MirPlace::local(fat),
            },
        );
        Ok(values::local(result))
    }

    /// `&s.w` or `&mut a[i]` given to a `&[elem]`, with the borrowed place a
    /// fixed array: the slice points at the array where it lives and carries
    /// its static length (G7). `None` when `inner` is not such an array.
    pub(super) fn projected_array_ref_as_slice(
        &mut self,
        inner: &ast::Expr,
        elem: &MirType,
        target: &MirType,
    ) -> CodegenResult<Option<MirValue>> {
        if !self.is_projected_array(inner)? {
            return Ok(None);
        }
        let (place, have, n) = match self.lower_place(inner)? {
            Some((place, MirType::Array(have, n))) => (place, have, n),
            // Not a place (a call result): the value is a temporary, and
            // the slice views that temporary for the length of the call.
            _ => self.array_temporary(inner)?,
        };
        if *have != *elem {
            return Err(element_mismatch("array", &have, elem, inner.span));
        }
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let ptr = builder.create_local(MirType::Ptr(Box::new(MirType::Array(have, n))));
        builder.assign(
            ptr,
            MirRValue::AddressOf {
                is_mut: true,
                place,
            },
        );
        let len = MirValue::Const(MirConst::Int(n as i128, MirType::usize()));
        self.slice_from_parts(values::local(ptr), len, elem, target)
            .map(Some)
    }

    /// Evaluate an array expression that is not a place into a local, and
    /// return that local as a place with the array's element type and length.
    fn array_temporary(
        &mut self,
        expr: &ast::Expr,
    ) -> CodegenResult<(MirPlace, Box<MirType>, u64)> {
        let value = self.lower_expr(expr)?;
        match (&value, self.type_of_value(&value)) {
            (MirValue::Local(id), MirType::Array(have, n)) => Ok((MirPlace::local(*id), have, n)),
            _ => Err(CodegenError::Rejected {
                message: "cannot borrow this expression as a slice: it is not a fixed array"
                    .to_string(),
                location: location(expr.span),
                help: Some("bind the array to a local and borrow the local".to_string()),
            }),
        }
    }

    /// The type a reference-typed expression points at, when it is a vector
    /// or a fixed array: `v` with `v: &Vec<T>`, `a` with `a: &[T; N]`.
    fn referent_collection(&self, expr: &ast::Expr) -> Option<MirType> {
        match self.infer_expr_type(expr) {
            MirType::Ptr(inner) if matches!(*inner, MirType::Vec(_) | MirType::Array(..)) => {
                Some(*inner)
            }
            _ => None,
        }
    }

    /// Whether `expr` has the type `&Vec<T>` or `&[T; N]`.
    pub(super) fn is_collection_ref(&self, expr: &ast::Expr) -> bool {
        self.referent_collection(expr).is_some()
    }

    /// A reference the function already holds, `v: &Vec<T>` or `a: &[T; N]`,
    /// given to a `&[elem]`: build the slice from what it points at. Before
    /// this the reference itself was passed and the callee read a vector
    /// handle, or the array's first bytes, as `{ ptr, len }` (G31). `None`
    /// when `arg` is not such a reference.
    pub(super) fn forwarded_ref_as_slice(
        &mut self,
        arg: &ast::Expr,
        elem: &MirType,
        target: &MirType,
    ) -> CodegenResult<Option<MirValue>> {
        let Some(referent) = self.referent_collection(arg) else {
            return Ok(None);
        };
        let ptr = self.lower_expr(arg)?;
        match referent {
            MirType::Vec(have) => {
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let handle = builder.create_local(MirType::Vec(have.clone()));
                builder.assign(
                    handle,
                    MirRValue::Deref {
                        ptr,
                        pointee_ty: MirType::Vec(have.clone()),
                    },
                );
                let target = Box::new(target.clone());
                self.vec_ref_as_slice(values::local(handle), &have, elem, target, arg.span)
                    .map(Some)
            }
            MirType::Array(have, n) => {
                if *have != *elem {
                    return Err(element_mismatch("array", &have, elem, arg.span));
                }
                let len = MirValue::Const(MirConst::Int(n as i128, MirType::usize()));
                self.slice_from_parts(ptr, len, elem, target).map(Some)
            }
            _ => Ok(None),
        }
    }
}
