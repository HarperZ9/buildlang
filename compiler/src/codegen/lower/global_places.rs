//! Places rooted at a mutable global.
//!
//! A MIR place starts at a local, and a bare name with no local is a global,
//! so `lower_place` used to give up on it. The callers then fell back to a
//! copy: `&mut TABLE` pointed at a copy of the global (a write through it
//! was lost), `fill(&mut TABLE.w)` passed a copied array, `TABLE.a = 5` was
//! refused and `G[i] = v` on a global array had no bounds check (G35). The
//! place of a mutable global is now `*p` with `p = &GLOBAL` in a pointer
//! local (MIR `AddressOfGlobal`), and every projection after it is the
//! ordinary one, bounds checks included.
//!
//! Immutable globals keep the copy: nothing can change them, so a copy reads
//! the same values.

use std::sync::Arc;

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::*;

use super::MirLowerer;

/// Names, fields, dereferences and indexes by names or literals: lowering it
/// twice runs nothing twice.
fn is_pure_place(expr: &ast::Expr) -> bool {
    match &expr.kind {
        ExprKind::Ident(_) | ExprKind::Literal(_) => true,
        ExprKind::Paren(e) | ExprKind::Deref(e) => is_pure_place(e),
        ExprKind::Field { expr: e, .. } | ExprKind::TupleField { expr: e, .. } => is_pure_place(e),
        ExprKind::Index { expr: e, index } => is_pure_place(e) && is_pure_place(index),
        _ => false,
    }
}

impl<'ctx> MirLowerer<'ctx> {
    /// The mutable global a bare name refers to, and its type. `None` when a
    /// local shadows the name or no such mutable global exists.
    pub(super) fn mutable_global(&self, name: &Arc<str>) -> Option<(Arc<str>, MirType)> {
        if self.var_map.contains_key(name) {
            return None;
        }
        let candidates = [self.prefixed_name(name), name.clone()];
        candidates.into_iter().find_map(|n| {
            let global = self.module.find_global(n.as_ref())?;
            (global.is_mut && !global.is_extern_decl).then(|| (n.clone(), global.ty.clone()))
        })
    }

    /// Whether `expr` is a bare name of a mutable global array.
    pub(super) fn is_global_array_name(&self, expr: &ast::Expr) -> bool {
        match &expr.kind {
            ExprKind::Paren(inner) => self.is_global_array_name(inner),
            ExprKind::Ident(id) => {
                matches!(self.mutable_global(&id.name), Some((_, MirType::Array(..))))
            }
            _ => false,
        }
    }

    /// `p = &GLOBAL` into a new pointer local.
    pub(super) fn global_address(&mut self, name: Arc<str>, ty: MirType) -> CodegenResult<LocalId> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let ptr = builder.create_local(MirType::Ptr(Box::new(ty)));
        builder.assign(ptr, MirRValue::AddressOfGlobal { is_mut: true, name });
        Ok(ptr)
    }

    /// The place `*(&GLOBAL)` for a bare name that is a mutable global.
    pub(super) fn global_place(
        &mut self,
        ident: &ast::Ident,
    ) -> CodegenResult<Option<(MirPlace, MirType)>> {
        let Some((name, ty)) = self.mutable_global(&ident.name) else {
            return Ok(None);
        };
        let ptr = self.global_address(name, ty.clone())?;
        Ok(Some((MirPlace::local(ptr).deref(), ty)))
    }

    /// `&receiver` at the receiver's real location, for a method that takes
    /// `&self` or `&mut self`. `None` for a plain local (the caller's path
    /// borrows it already), for a receiver that is not a place, and for one
    /// whose index expressions could have a side effect: the receiver value
    /// was lowered once already, and making the place lowers them again.
    pub(super) fn receiver_place_ref(
        &mut self,
        receiver: &ast::Expr,
    ) -> CodegenResult<Option<MirValue>> {
        let is_local =
            matches!(&receiver.kind, ExprKind::Ident(id) if self.var_map.contains_key(&id.name));
        if is_local || !is_pure_place(receiver) {
            return Ok(None);
        }
        let Some((place, ty)) = self.lower_place(receiver)? else {
            return Ok(None);
        };
        if matches!(ty, MirType::Ptr(_)) {
            return Ok(None);
        }
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let ptr = builder.create_local(MirType::Ptr(Box::new(ty)));
        builder.make_ref(ptr, true, place);
        Ok(Some(MirValue::Local(ptr)))
    }

    /// `&GLOBAL` and `&mut GLOBAL` for a bare mutable global: the pointer
    /// itself, with no second address step.
    pub(super) fn global_ref(&mut self, inner: &ast::Expr) -> CodegenResult<Option<MirValue>> {
        let ExprKind::Ident(ident) = &inner.kind else {
            return Ok(None);
        };
        let Some((name, ty)) = self.mutable_global(&ident.name) else {
            return Ok(None);
        };
        let ptr = self.global_address(name, ty)?;
        Ok(Some(MirValue::Local(ptr)))
    }
}
