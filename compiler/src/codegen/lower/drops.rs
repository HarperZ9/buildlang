//! User `impl Drop for T`: `drop(&mut self)` runs when an owned `T` goes away.
//!
//! Every `let` and parameter whose type needs dropping gets a drop flag, set
//! when the value is stored. A type needs dropping when it has a Drop impl or
//! a field whose type needs dropping. At the end of its block the value is
//! dropped if the flag is still set, newest first: its own `drop` runs, then
//! its fields' drops in declaration order, as in Rust. A `return` drops every
//! live value of the function first. A move clears the flag: the value named
//! on its own as a `let` initialiser, a call or method argument, a struct
//! field (written out or shorthand), a tuple or array element, a returned or
//! assigned value, a block's tail, or the receiver of a method that takes
//! `self` by value. Assigning to a live variable drops the old value after the
//! new one is computed. Before, `drop` never ran.
//!
//! Moving one field out (`let x = s.g;` where `g` needs dropping) is refused,
//! since `s` would still drop it. Not covered yet: `break`, `continue` and `?`
//! leave without dropping the values of the blocks they leave; a temporary
//! that is never bound is not dropped; `match g { x => .. }` keeps `g` as the
//! owner, so it is dropped once, at the end of `g`'s block.

use std::sync::Arc;

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;

use super::MirLowerer;

/// One owned value that needs dropping: the local, its flag and its type.
#[derive(Clone)]
pub(crate) struct DropSlot {
    value: LocalId,
    flag: LocalId,
    ty: MirType,
}

fn peel(expr: &ast::Expr) -> &ast::Expr {
    match &expr.kind {
        ExprKind::Paren(inner) => peel(inner),
        _ => expr,
    }
}

/// Struct nesting deeper than this is not searched for drops.
const MAX_DROP_DEPTH: usize = 8;

impl MirLowerer<'_> {
    /// Record `impl Drop for T`, and which methods of `T` take `self` by
    /// value, while impls are collected.
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
        for item in &impl_def.items {
            let ast::ImplItemKind::Function(f) = &item.kind else {
                continue;
            };
            let by_value = f.sig.params.first().is_some_and(|p| {
                matches!(&p.pattern.kind,
                    ast::PatternKind::Ident { name, .. } if name.name.as_ref() == "self")
                    && !matches!(p.ty.kind, ast::TypeKind::Ref { .. })
            });
            if by_value {
                self.by_value_self
                    .insert((type_name.clone(), f.name.name.clone()));
            }
        }
    }

    /// Whether a value of `ty` has anything to drop.
    fn needs_drop(&self, ty: &MirType, depth: usize) -> bool {
        let MirType::Struct(name) = ty else {
            return false;
        };
        if self.drop_fns.contains_key(name) {
            return true;
        }
        depth < MAX_DROP_DEPTH
            && self
                .struct_fields_of(ty)
                .is_some_and(|fs| fs.iter().any(|(_, t)| self.needs_drop(t, depth + 1)))
    }

    fn slot_of(&self, local: LocalId) -> Option<DropSlot> {
        self.drop_scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|s| s.value == local)
            .cloned()
    }

    fn droppable_named(&self, name: &Arc<str>) -> Option<DropSlot> {
        if self.drop_scopes.is_empty() {
            return None;
        }
        let local = *self.var_map.get(name)?;
        self.slot_of(local)
    }

    fn droppable_name(&self, expr: &ast::Expr) -> Option<DropSlot> {
        let ExprKind::Ident(ident) = &peel(expr).kind else {
            return None;
        };
        self.droppable_named(&ident.name)
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
    /// Moving out a field that needs dropping is refused.
    pub(super) fn mark_moved(&mut self, expr: &ast::Expr) -> CodegenResult<()> {
        if let Some(slot) = self.droppable_name(expr) {
            return self.set_flag(slot.flag, false);
        }
        if let ExprKind::Field { expr: base, field } = &peel(expr).kind {
            if let Some(slot) = self.droppable_name(base) {
                let field_ty = self
                    .struct_fields_of(&slot.ty)
                    .and_then(|fs| fs.into_iter().find(|(n, _)| *n == field.name))
                    .map(|(_, t)| t);
                if field_ty.is_some_and(|t| self.needs_drop(&t, 0)) {
                    let span = field.span;
                    return Err(CodegenError::Rejected {
                        message: format!(
                            "moving the field `{}` out of a value that is dropped is not supported",
                            field.name
                        ),
                        location: (span.end.0 > span.start.0).then_some((span.start.0, span.end.0)),
                        help: Some(
                            "move the whole value, or borrow the field: `&s.field`".to_string(),
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// The shorthand field `S { g }` moves the variable `g`.
    pub(super) fn mark_moved_name(&mut self, name: &Arc<str>) -> CodegenResult<()> {
        if let Some(slot) = self.droppable_named(name) {
            self.set_flag(slot.flag, false)?;
        }
        Ok(())
    }

    /// `g.consume()` where `consume` takes `self` by value moves `g`.
    pub(super) fn mark_receiver_moved(
        &mut self,
        receiver: &ast::Expr,
        method: &ast::Ident,
    ) -> CodegenResult<()> {
        let Some(slot) = self.droppable_name(receiver) else {
            return Ok(());
        };
        let MirType::Struct(name) = &slot.ty else {
            return Ok(());
        };
        if self
            .by_value_self
            .contains(&(name.clone(), method.name.clone()))
        {
            self.set_flag(slot.flag, false)?;
        }
        Ok(())
    }

    /// Give `local` a drop flag in the innermost scope when its type needs
    /// dropping.
    pub(super) fn register_drop(&mut self, local: LocalId) -> CodegenResult<()> {
        if self.drop_scopes.is_empty() {
            return Ok(());
        }
        let Some(ty) = self.current_fn.as_ref().and_then(|b| b.local_type(local)) else {
            return Ok(());
        };
        if !self.needs_drop(&ty, 0) {
            return Ok(());
        }
        let builder = self.current_fn.as_mut().unwrap();
        let flag = builder.create_local(MirType::Bool);
        builder.assign(flag, MirRValue::Use(values::bool(true)));
        let slot = DropSlot {
            value: local,
            flag,
            ty,
        };
        self.drop_scopes.last_mut().unwrap().push(slot);
        Ok(())
    }

    /// Drop the value at `place`: its own `drop`, then its fields in order.
    fn emit_drop_glue(&mut self, place: MirPlace, ty: &MirType, depth: usize) -> CodegenResult<()> {
        if let MirType::Struct(name) = ty {
            if let Some(func) = self.drop_fns.get(name).cloned() {
                let builder = self.current_fn.as_mut().unwrap();
                let ptr = builder.create_local(MirType::Ptr(Box::new(ty.clone())));
                builder.make_ref(ptr, true, place.clone());
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(func),
                    vec![values::local(ptr)],
                    None,
                    cont,
                );
                builder.switch_to_block(cont);
            }
        }
        if depth >= MAX_DROP_DEPTH {
            return Ok(());
        }
        let fields = self.struct_fields_of(ty).unwrap_or_default();
        for (i, (fname, fty)) in fields.into_iter().enumerate() {
            if self.needs_drop(&fty, depth + 1) {
                let field_place = place.clone().field(i as u32, fname, fty.clone());
                self.emit_drop_glue(field_place, &fty, depth + 1)?;
            }
        }
        Ok(())
    }

    /// `if flag { flag = false; <drop glue for value> }`
    fn emit_drop(&mut self, slot: &DropSlot) -> CodegenResult<()> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let drop_bb = builder.create_block();
        let done_bb = builder.create_block();
        builder.branch(values::local(slot.flag), drop_bb, done_bb);
        builder.switch_to_block(drop_bb);
        builder.assign(slot.flag, MirRValue::Use(values::bool(false)));
        self.emit_drop_glue(MirPlace::local(slot.value), &slot.ty, 0)?;
        let builder = self.current_fn.as_mut().unwrap();
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

    /// `x = value` where `x` holds a value that needs dropping: compute the
    /// new value, drop the old one if it is live, store, and set the flag.
    /// `None` when `target` is not such a variable.
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

    /// The parameters of a function body that need dropping, in a scope of
    /// their own around the body.
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
