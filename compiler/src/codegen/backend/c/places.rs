//! A MIR place (a local followed by field, index and dereference steps) as a
//! C lvalue.
//!
//! `&(place)` is the address of the real location, so a projected store or a
//! reference writes through instead of into a copy. Every index step is
//! checked against the length its base carries: `arr[9].y = v` on an array
//! of two structs used to write past the array with no trap (G28).

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::{MirLocal, MirPlace, MirType, PlaceProjection};

use super::CBackend;

fn is_array(ty: &MirType) -> bool {
    matches!(ty, MirType::Array(..))
}

/// `*expr` for a pointer of type `ty`, with the pointee type. A pointer to
/// an array is already the array's first element in the emitted C, so the
/// text stays as it is; `*p` would name one element.
fn deref_step(expr: String, ty: Option<MirType>) -> (String, Option<MirType>) {
    let pointee = match ty {
        Some(MirType::Ptr(inner)) => Some(*inner),
        _ => None,
    };
    if pointee
        .as_ref()
        .is_some_and(|t| is_array(t) || matches!(t, MirType::Slice(_)))
    {
        (expr, pointee)
    } else {
        (format!("(*{expr})"), pointee)
    }
}

/// `expr[idx]` for a base of type `ty`, with the element type. An array (by
/// value or behind a reference) is checked against its static length and a
/// slice against the length it carries. A raw pointer has no length and
/// stays an unchecked subscript.
fn index_step(expr: &str, idx: &str, ty: Option<&MirType>) -> (String, Option<MirType>) {
    let chk = |len: String| format!("bl_idx_chk((long long)({idx}), {len})");
    let (through_ptr, inner) = match ty {
        // A view is its slice struct by value: read it with `.`.
        Some(MirType::Ptr(inner)) if matches!(inner.as_ref(), MirType::Slice(_)) => {
            (false, Some(inner.as_ref()))
        }
        Some(MirType::Ptr(inner)) => (true, Some(inner.as_ref())),
        other => (false, other),
    };
    match inner {
        Some(MirType::Array(elem, len)) => (
            format!("({expr})[{}]", chk(format!("{len}ULL"))),
            Some((**elem).clone()),
        ),
        Some(MirType::Slice(elem)) => {
            let dot = if through_ptr { "->" } else { "." };
            (
                format!("({expr}){dot}ptr[{}]", chk(format!("({expr}){dot}len"))),
                Some((**elem).clone()),
            )
        }
        Some(MirType::Vec(elem)) if !through_ptr => {
            (format!("({expr})[{idx}]"), Some((**elem).clone()))
        }
        Some(other) if through_ptr => (format!("({expr})[{idx}]"), Some(other.clone())),
        _ => (format!("({expr})[{idx}]"), None),
    }
}

impl CBackend {
    /// Render a place as a C expression, with the type it ends at.
    pub(super) fn place_to_c_typed(
        &self,
        place: &MirPlace,
        locals: &[MirLocal],
    ) -> CodegenResult<(String, Option<MirType>)> {
        let mut expr = self.local_name(place.local, locals);
        let mut ty = locals.get(place.local.0 as usize).map(|l| l.ty.clone());
        for proj in &place.projections {
            (expr, ty) = match proj {
                PlaceProjection::Deref => deref_step(expr, ty),
                PlaceProjection::Field(_idx, name, fty) => {
                    let field = Self::escape_c_keyword(name);
                    let through_ptr = matches!(ty, Some(MirType::Ptr(_)))
                        && !ty.as_ref().is_some_and(Self::is_slice_ref);
                    let sep = if through_ptr { "->" } else { "." };
                    let base = if through_ptr {
                        expr
                    } else {
                        format!("({expr})")
                    };
                    (format!("{base}{sep}{field}"), Some(fty.clone()))
                }
                PlaceProjection::Index(idx_local) => {
                    let idx = self.local_name(*idx_local, locals);
                    index_step(&expr, &idx, ty.as_ref())
                }
                PlaceProjection::ConstantIndex {
                    offset,
                    from_end: false,
                } => index_step(&expr, &offset.to_string(), ty.as_ref()),
                PlaceProjection::ConstantIndex { from_end: true, .. }
                | PlaceProjection::Subslice { .. }
                | PlaceProjection::Downcast(_) => {
                    return Err(CodegenError::Internal(
                        "from-end index, subslice and downcast are not supported as a C place"
                            .to_string(),
                    ));
                }
            };
        }
        Ok((expr, ty))
    }

    /// `&place`. An array place is its own address in C: `&(s.w)` has the
    /// type `T (*)[N]`, which is not the `T*` a reference to an array is.
    pub(super) fn address_of_place(
        &self,
        place: &MirPlace,
        locals: &[MirLocal],
    ) -> CodegenResult<String> {
        let (expr, ty) = self.place_to_c_typed(place, locals)?;
        // A slice struct is its own view: a view is carried by value.
        if ty
            .as_ref()
            .is_some_and(|t| is_array(t) || matches!(t, MirType::Slice(_)))
        {
            Ok(format!("({expr})"))
        } else {
            Ok(format!("&({expr})"))
        }
    }
}
