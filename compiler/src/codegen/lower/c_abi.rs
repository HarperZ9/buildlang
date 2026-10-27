//! C ABI export checks (M4).
//!
//! An `extern "C" fn` definition is a promise to C callers: its parameters and
//! return type must have a C representation that a generated header can state.
//! Allowed: integers, floats, `bool`, raw pointers, and `#[repr(C)]` structs
//! whose fields are allowed types or fixed arrays of them. A BuildLang `str`,
//! `Vec`, map, `Option`, enum or closure has no stable C layout, so an export
//! that uses one is refused at `buildc check` (BL0901) with a fix, rather than
//! producing a header the host cannot use.
//!
//! `#[repr(C)]` is required on every struct an export reaches, so the layout is
//! an explicit promise that later layout work cannot silently break. The C
//! backend already lays out fields in declaration order with C alignment.

use std::sync::Arc;

use super::MirLowerer;
use crate::ast;
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::{MirFunction, MirType, TypeDefKind};

/// True for `#[repr(C)]`. The attribute's argument is not re-read here: `repr`
/// with a single argument is accepted on structs, and `C` is the only form the
/// docs describe.
fn is_repr_attr(attr: &ast::Attribute) -> bool {
    attr.path
        .segments
        .first()
        .is_some_and(|seg| seg.ident.name.as_ref() == "repr")
        && matches!(&attr.args, ast::AttrArgs::Delimited(tokens) if tokens.len() == 1)
}

impl<'ctx> MirLowerer<'ctx> {
    /// Pre-pass: remember which structs carry `#[repr(C)]`.
    pub(super) fn collect_repr_c(&mut self, item: &ast::Item) {
        if let ast::ItemKind::Struct(s) = &item.kind {
            if item.attrs.iter().any(is_repr_attr) {
                let name = self.prefixed_name(&s.name.name);
                self.repr_c_structs.insert(name);
                self.repr_c_structs.insert(s.name.name.clone());
            }
        }
    }

    /// Refuse an `extern "C"` definition whose signature has no C ABI.
    pub(super) fn check_c_export_signature(
        &self,
        f: &ast::FnDef,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        for local in func.locals.iter().filter(|l| l.is_param) {
            let what = match &local.name {
                Some(n) => format!("parameter `{n}`"),
                None => "a parameter".to_string(),
            };
            self.require_c_abi(&local.ty, f, &what)?;
        }
        if !matches!(func.sig.ret, MirType::Void) {
            self.require_c_abi(&func.sig.ret, f, "the return type")?;
        }
        Ok(())
    }

    fn require_c_abi(&self, ty: &MirType, f: &ast::FnDef, what: &str) -> CodegenResult<()> {
        let Some((problem, help)) = self.c_abi_problem(ty, false, &mut Vec::new()) else {
            return Ok(());
        };
        let span = f.name.span;
        Err(CodegenError::Rejected {
            message: format!(
                "`extern \"C\" fn {}` cannot be exported: {what} {problem}",
                f.name.name
            ),
            location: (span.end.0 > span.start.0).then_some((span.start.0, span.end.0)),
            help: Some(help),
        })
    }

    /// Why `ty` has no C ABI, with a fix, or `None` when it does.
    fn c_abi_problem(
        &self,
        ty: &MirType,
        in_field: bool,
        seen: &mut Vec<Arc<str>>,
    ) -> Option<(String, String)> {
        const SCALARS: &str =
            "use integers, floats, bool, raw pointers, or a #[repr(C)] struct of those";
        match ty {
            MirType::Int(..) | MirType::Float(..) | MirType::Bool => None,
            MirType::Ptr(inner) => match inner.as_ref() {
                MirType::Void => None,
                other => self.c_abi_problem(other, true, seen),
            },
            MirType::Array(elem, _) if in_field => self.c_abi_problem(elem, true, seen),
            MirType::Array(..) => Some((
                "is a fixed array, which C passes as a pointer".to_string(),
                "wrap the array in a #[repr(C)] struct, or pass a pointer and a length".to_string(),
            )),
            MirType::Slice(elem) => Some((
                format!("is a slice `&[{elem}]`, which has no C layout"),
                format!(
                    "take `p: *const {elem}, n: usize` and read it with `unsafe {{ slice::from_raw_parts(p, n) }}`"
                ),
            )),
            MirType::Struct(name) if name.as_ref() == "BuildString" => Some((
                "is a BuildLang `str`, which has no C layout".to_string(),
                "pass a `*const u8` pointer and an `i64` length instead".to_string(),
            )),
            MirType::Struct(name) => {
                if seen.contains(name) {
                    return None;
                }
                let Some(def) = self.module.find_type(name) else {
                    return Some((
                        format!("has type `{name}`, which has no C ABI"),
                        SCALARS.to_string(),
                    ));
                };
                let TypeDefKind::Struct { fields, .. } = &def.kind else {
                    return Some((
                        format!("has enum or union type `{name}`, which has no stable C layout"),
                        "pass an integer tag and a #[repr(C)] struct for the payload".to_string(),
                    ));
                };
                if !self.repr_c_structs.contains(name) {
                    return Some((
                        format!("uses struct `{name}`, which is not marked #[repr(C)]"),
                        format!("add `#[repr(C)]` on the line above `struct {name}`"),
                    ));
                }
                seen.push(name.clone());
                fields
                    .iter()
                    .find_map(|(_, field_ty)| self.c_abi_problem(field_ty, true, seen))
            }
            other => Some((
                format!("has type `{other:?}`, which has no C ABI"),
                SCALARS.to_string(),
            )),
        }
    }
}
