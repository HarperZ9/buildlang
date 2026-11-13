//! User `impl Drop for T`: `drop(&mut self)` runs when an owned `T` goes away.
//!
//! Every `let` of a type with a Drop impl (and every parameter of one) gets a
//! drop flag, set when the value is stored. At the end of its block the value
//! is dropped if the flag is still set, in reverse declaration order. A
//! `return` drops every live value of the function first. A move clears the
//! flag: the value named on its own as a `let` initialiser, a call or method
//! argument, a struct field, a tuple or array element, a returned value, an
//! assigned value or a block's tail. Assigning to a live variable drops the
//! old value after the new one is computed. Before, `drop` never ran.
//!
//! Not covered yet: `break` and `continue` out of a block, and `?`, leave
//! without dropping that block's values; a temporary that is never bound is
//! not dropped; a `match` or method receiver taken by value is not a move.

use std::sync::Arc;

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;

use super::MirLowerer;

/// One owned value with a Drop impl: the local, its drop flag, the drop fn.
#[derive(Clone)]
pub(crate) struct DropSlot {
    value: LocalId,
    flag: LocalId,
    func: Arc<str>,
}

fn peel(expr: &ast::Expr) -> &ast::Expr {
    match &expr.kind {
        ExprKind::Paren(inner) => peel(inner),
        _ => expr,
    }
}

impl MirLowerer<'_> {
    /// Record `impl Drop for T` while impls are collected.
    pub(super) fn note_drop_impl(&mut self, impl_def: &ast::ImplDef, type_name: &Arc<str>) {
        let is_drop = impl_def
            .trait_ref
            .as_ref()
            .and_then(|t| t.path.last_ident())
            .is_some_and(|i| i.name.as_ref() == "Drop");
        if is_drop {
            self.drop_fns
                .insert(type_name.clone(), Arc::from(format!("{type_name}_drop")));
        }
    }

    fn drop_fn_for(&self, ty: &MirType) -> Option<Arc<str>> {
        match ty {
            MirType::Struct(name) => self.drop_fns.get(name).cloned(),
            _ => None,
        }
    }

    fn slot_of(&self, local: LocalId) -> Option<DropSlot> {
        self.drop_scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|s| s.value == local)
            .cloned()
    }

    fn droppable_name(&self, expr: &ast::Expr) -> Option<DropSlot> {
        if self.drop_scopes.is_empty() {
            return None;
        }
        let ExprKind::Ident(ident) = &peel(expr).kind else {
            return None;
        };
        let local = *self.var_map.get(&ident.name)?;
        self.slot_of(local)
    }

    fn set_flag(&mut self, flag: LocalId, on: bool) -> CodegenResult<()> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        builder.assign(flag, MirRValue::Use(values::bool(on)));
        Ok(())
    }

    /// A value named on its own in a moving position leaves its variable.
    pub(super) fn mark_moved(&mut self, expr: &ast::Expr) -> CodegenResult<()> {
        if let Some(slot) = self.droppable_name(expr) {
            self.set_flag(slot.flag, false)?;
        }
        Ok(())
    }

    /// Give `local` a drop flag in the innermost scope when its type has a
    /// Drop impl.
    pub(super) fn register_drop(&mut self, local: LocalId) -> CodegenResult<()> {
        if self.drop_scopes.is_empty() {
            return Ok(());
        }
        let ty = self.current_fn.as_ref().and_then(|b| b.local_type(local));
        let Some(func) = ty.as_ref().and_then(|t| self.drop_fn_for(t)) else {
            return Ok(());
        };
        let builder = self.current_fn.as_mut().unwrap();
        let flag = builder.create_local(MirType::Bool);
        builder.assign(flag, MirRValue::Use(values::bool(true)));
        let slot = DropSlot {
            value: local,
            flag,
            func,
        };
        self.drop_scopes.last_mut().unwrap().push(slot);
        Ok(())
    }

    /// `if flag { T_drop(&mut value); flag = false; }`
    fn emit_drop(&mut self, slot: &DropSlot) -> CodegenResult<()> {
        let ty = self
            .current_fn
            .as_ref()
            .and_then(|b| b.local_type(slot.value))
            .unwrap_or(MirType::Void);
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let drop_bb = builder.create_block();
        let done_bb = builder.create_block();
        builder.branch(values::local(slot.flag), drop_bb, done_bb);
        builder.switch_to_block(drop_bb);
        builder.assign(slot.flag, MirRValue::Use(values::bool(false)));
        let ptr = builder.create_local(MirType::Ptr(Box::new(ty)));
        builder.make_ref(ptr, true, MirPlace::local(slot.value));
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(slot.func.clone()),
            vec![values::local(ptr)],
            None,
            cont,
        );
        builder.switch_to_block(cont);
        builder.goto(done_bb);
        builder.switch_to_block(done_bb);
        Ok(())
    }

    pub(super) fn push_drop_scope(&mut self) {
        self.drop_scopes.push(Vec::new());
    }

    /// End the innermost scope: drop its live values, newest first.
    pub(super) fn pop_drop_scope(&mut self) -> CodegenResult<()> {
        let scope = self.drop_scopes.pop().unwrap_or_default();
        for slot in scope.iter().rev() {
            self.emit_drop(slot)?;
        }
        Ok(())
    }

    /// Before a `return`: drop every live value of the function, innermost
    /// scope first. The scopes stay; their flags are now clear.
    pub(super) fn drop_all_for_return(&mut self) -> CodegenResult<()> {
        let all: Vec<DropSlot> = self
            .drop_scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev().cloned())
            .collect();
        for slot in &all {
            self.emit_drop(slot)?;
        }
        Ok(())
    }

    /// `x = value` where `x` holds a value with a Drop impl: compute the new
    /// value, drop the old one if it is live, store, and set the flag. `None`
    /// when `target` is not such a variable.
    pub(super) fn try_lower_drop_assign(
        &mut self,
        target: &ast::Expr,
        value: &ast::Expr,
    ) -> Option<CodegenResult<MirValue>> {
        let slot = self.droppable_name(target)?;
        Some((|| {
            let new_val = self.lower_expr(value)?;
            self.mark_moved(value)?;
            self.emit_drop(&slot)?;
            let builder = self.current_fn.as_mut().unwrap();
            builder.assign(slot.value, MirRValue::Use(new_val));
            self.set_flag(slot.flag, true)?;
            Ok(values::unit())
        })())
    }

    /// The parameters of a function body that have a Drop impl, in a scope
    /// of their own around the body.
    pub(super) fn register_param_drops(&mut self) -> CodegenResult<()> {
        let Some(builder) = self.current_fn.as_ref() else {
            return Ok(());
        };
        let nparams = builder.param_count();
        let mut params: Vec<LocalId> = self
            .var_map
            .values()
            .copied()
            .filter(|l| (l.0 as usize) < nparams)
            .collect();
        params.sort_by_key(|l| l.0);
        for local in params {
            self.register_drop(local)?;
        }
        Ok(())
    }
}
