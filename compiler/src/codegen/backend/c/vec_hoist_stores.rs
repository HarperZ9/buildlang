//! Stores that can replace a vector handle behind a hoisted parameter.
//!
//! The language has no borrow checker, so a caller may pass `&mut h.data`
//! beside `&mut h`. A callee that then writes `h.data = w` or `*h = g` swaps
//! the handle the vector parameter points at, and a buffer loaded at entry
//! would be the old one. Any store through a pointer, into a field, an element
//! or a global disqualifies hoisting unless the stored value is a plain number
//! or a struct of plain numbers, which holds no handle.

use std::collections::HashSet;

use crate::codegen::ir::{
    MirFunction, MirModule, MirRValue, MirStmtKind, MirType, MirValue, TypeDefKind,
};

/// Structs whose fields, at any depth, are only numbers and booleans.
pub(super) fn plain_structs(module: &MirModule) -> HashSet<String> {
    let mut plain: HashSet<String> = HashSet::new();
    loop {
        let before = plain.len();
        for def in &module.types {
            if let TypeDefKind::Struct { fields, .. } = &def.kind {
                if fields.iter().all(|(_, ty)| !may_hold_handle(ty, &plain)) {
                    plain.insert(def.name.to_string());
                }
            }
        }
        if plain.len() == before {
            return plain;
        }
    }
}

/// Whether a value of this type can carry a vector or map handle.
fn may_hold_handle(ty: &MirType, plain: &HashSet<String>) -> bool {
    match ty {
        MirType::Void | MirType::Bool | MirType::Int(..) | MirType::Float(_) | MirType::Never => {
            false
        }
        MirType::Struct(name) => !plain.contains(name.as_ref()),
        _ => true,
    }
}

fn value_may_hold_handle(value: &MirRValue, func: &MirFunction, plain: &HashSet<String>) -> bool {
    match value {
        MirRValue::Use(MirValue::Const(_)) => false,
        MirRValue::Use(MirValue::Local(id)) => func
            .locals
            .get(id.0 as usize)
            .is_none_or(|l| may_hold_handle(&l.ty, plain)),
        MirRValue::BinaryOp { .. }
        | MirRValue::UnaryOp { .. }
        | MirRValue::Len(_)
        | MirRValue::Discriminant(_) => false,
        MirRValue::Cast { ty, .. } => may_hold_handle(ty, plain),
        MirRValue::FieldAccess { field_ty: ty, .. }
        | MirRValue::VariantField { field_ty: ty, .. }
        | MirRValue::IndexAccess { elem_ty: ty, .. }
        | MirRValue::Deref { pointee_ty: ty, .. } => may_hold_handle(ty, plain),
        _ => true,
    }
}

/// Whether `stmt` may overwrite a vector handle that a parameter points at.
pub(super) fn store_may_replace_handle(
    stmt: &MirStmtKind,
    func: &MirFunction,
    plain: &HashSet<String>,
) -> bool {
    match stmt {
        MirStmtKind::DerefAssign { ptr, value } => {
            let pointee_holds = match func.locals.get(ptr.0 as usize).map(|l| &l.ty) {
                Some(MirType::Ptr(inner)) => may_hold_handle(inner, plain),
                _ => true,
            };
            pointee_holds || value_may_hold_handle(value, func, plain)
        }
        MirStmtKind::FieldDerefAssign { value, .. }
        | MirStmtKind::FieldAssign { value, .. }
        | MirStmtKind::IndexStore { value, .. }
        | MirStmtKind::GlobalStore { value, .. } => value_may_hold_handle(value, func, plain),
        _ => false,
    }
}
