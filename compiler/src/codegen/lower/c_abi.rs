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
//! `#[repr(C)]` is required on every struct an export reaches by value or
//! through a `#[repr(C)]` struct, so the layout is an explicit promise that
//! later layout work cannot silently break. The C backend already lays out
//! fields in declaration order with C alignment. A struct without
//! `#[repr(C)]` may appear only behind a raw pointer, as an opaque handle: the
//! header forward-declares it and states no layout.

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

    /// A user struct without `#[repr(C)]`, which an export may reach only
    /// through a pointer.
    fn is_opaque_struct(&self, name: &Arc<str>) -> bool {
        name.as_ref() != "BuildString"
            && !self.repr_c_structs.contains(name)
            && self
                .module
                .find_type(name)
                .is_some_and(|def| matches!(def.kind, TypeDefKind::Struct { .. }))
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
                // A pointer to a struct without `#[repr(C)]` is an opaque
                // handle: the header forward-declares the struct, so the host
                // can hold and pass the pointer but cannot see or rely on the
                // layout. This is how a C library keeps state behind a handle.
                MirType::Struct(name) if self.is_opaque_struct(name) => None,
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

/// The annotation prefix that carries a raw pointer's `const` levels from the
/// source type to the generated C header (see [`c_const_levels`]).
pub(crate) const C_CONST_ANNOTATION: &str = "c_const:";

/// The `const` levels of a raw pointer type, outermost first: `T` where that
/// level is `*const`, `F` where it is `*mut`. `*const *mut u8` gives `TF`.
/// `None` when the type is not a raw pointer or no level is `const`, so a
/// header for a signature without `*const` is unchanged.
pub(crate) fn c_const_levels(ty: &ast::Type) -> Option<Arc<str>> {
    let mut levels = String::new();
    let mut cur = ty;
    while let ast::TypeKind::Ptr { mutability, ty } = &cur.kind {
        levels.push(if *mutability == ast::Mutability::Immutable {
            'T'
        } else {
            'F'
        });
        cur = ty;
    }
    levels.contains('T').then(|| Arc::from(levels.as_str()))
}

/// Record the `const` levels of an export's raw-pointer parameters and return
/// type on its MIR, for the header. The C definition is left as it was: the
/// symbol and its calling convention do not depend on `const`, and the body
/// may pass the pointer on to runtime helpers that take a plain pointer.
pub(super) fn record_c_const(f: &ast::FnDef, func: &mut MirFunction) {
    for (i, param) in f.sig.params.iter().enumerate() {
        if let (Some(levels), Some(local)) = (c_const_levels(&param.ty), func.locals.get_mut(i)) {
            if local.is_param {
                local
                    .annotations
                    .push(Arc::from(format!("{C_CONST_ANNOTATION}{levels}").as_str()));
            }
        }
    }
    func.c_const_ret = f.sig.return_ty.as_deref().and_then(c_const_levels);
}

/// The C spelling of a pointer type whose levels are `levels` (outermost
/// first, as [`c_const_levels`] gives them) over the pointee `base`:
/// `*const *const u8` is `const uint8_t* const*`. `None` when `ty` does not
/// have that many pointer levels, so the caller keeps its plain spelling.
pub(crate) fn c_const_pointer(
    ty: &MirType,
    levels: &str,
    base_to_c: impl Fn(&MirType) -> String,
) -> Option<String> {
    let flags: Vec<bool> = levels.chars().map(|c| c == 'T').collect();
    let mut base = ty;
    for _ in 0..flags.len() {
        match base {
            MirType::Ptr(inner) => base = inner,
            _ => return None,
        }
    }
    let n = flags.len();
    let mut s = base_to_c(base);
    if flags[n - 1] {
        s = format!("const {s}");
    }
    for i in (0..n).rev() {
        s.push('*');
        if i > 0 && flags[i - 1] {
            s.push_str(" const");
        }
    }
    Some(s)
}

#[cfg(test)]
mod const_tests {
    use super::*;

    fn render(ty: &MirType, levels: &str) -> Option<String> {
        c_const_pointer(ty, levels, |t| match t {
            MirType::Int(_, false) => "uint8_t".to_string(),
            other => format!("{other:?}"),
        })
    }

    #[test]
    fn const_levels_render_in_c_order() {
        let p = MirType::Ptr(Box::new(MirType::u8()));
        let pp = MirType::Ptr(Box::new(p.clone()));
        assert_eq!(render(&p, "T").as_deref(), Some("const uint8_t*"));
        assert_eq!(render(&pp, "TT").as_deref(), Some("const uint8_t* const*"));
        assert_eq!(render(&pp, "TF").as_deref(), Some("uint8_t* const*"));
        assert_eq!(render(&pp, "FT").as_deref(), Some("const uint8_t**"));
        // More levels than the type has: the plain spelling is kept.
        assert_eq!(render(&p, "TT"), None);
    }
}
