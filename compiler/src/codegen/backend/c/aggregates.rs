//! Fixed arrays held by value inside structs, and stores of a whole array.
//!
//! C has no array assignment and no array member in a positional compound
//! literal: `(P){ 7, w, -9 }` with `w` a `uint32_t[3]` puts the decayed
//! pointer in the first element and shifts every later member (G26). A
//! struct with an array member is therefore built one member at a time, and
//! every store whose value is an array is a `memcpy`.

use std::collections::HashMap;
use std::fmt::Write;

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::{
    AggregateKind, LocalId, MirLocal, MirModule, MirRValue, MirType, MirValue, TypeDefKind,
};

use super::CBackend;

/// Field names and types of every struct in a module, in declaration order.
pub(super) type StructFields = HashMap<String, Vec<(String, MirType)>>;

/// The struct table of `module`. An unnamed field takes the name the type
/// definition gives it, `fieldN`.
pub(super) fn struct_fields(module: &MirModule) -> StructFields {
    let mut table = StructFields::new();
    for def in &module.types {
        if let TypeDefKind::Struct { fields, .. } = &def.kind {
            let named = fields
                .iter()
                .enumerate()
                .map(|(i, (name, ty))| {
                    let name = name
                        .as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| format!("field{i}"));
                    (name, ty.clone())
                })
                .collect();
            table.insert(def.name.to_string(), named);
        }
    }
    table
}

fn is_array(ty: &MirType) -> bool {
    matches!(ty, MirType::Array(..))
}

fn local_ty(locals: &[MirLocal], id: LocalId) -> Option<&MirType> {
    locals.get(id.0 as usize).map(|l| &l.ty)
}

fn value_is_array(value: &MirValue, locals: &[MirLocal]) -> bool {
    matches!(value, MirValue::Local(id) if local_ty(locals, *id).is_some_and(is_array))
}

impl CBackend {
    /// The declared type of `field` in the struct `ty` names (through one
    /// pointer when `ty` is a reference to the struct).
    fn struct_field_type(&self, ty: Option<&MirType>, field: &str) -> Option<&MirType> {
        let name = match ty? {
            MirType::Struct(name) => name,
            MirType::Ptr(inner) => match inner.as_ref() {
                MirType::Struct(name) => name,
                _ => return None,
            },
            _ => return None,
        };
        let fields = self.struct_fields.get(name.as_ref())?;
        fields.iter().find(|(n, _)| n == field).map(|(_, t)| t)
    }

    /// The C expression an array store copies from. Only an array local or a
    /// global has storage to copy; anything else is refused, because C would
    /// accept it and copy the wrong bytes.
    fn array_source(&self, value: &MirValue, locals: &[MirLocal]) -> CodegenResult<String> {
        match value {
            MirValue::Local(_) if value_is_array(value, locals) => {
                Ok(self.value_to_c(value, locals))
            }
            MirValue::Global(_) => Ok(self.value_to_c(value, locals)),
            other => Err(CodegenError::Unsupported(format!(
                "the C backend cannot copy an array from `{other}`: it is not an array local"
            ))),
        }
    }

    /// `memcpy(target, value, sizeof(size_of));` for an array-typed `target`.
    fn emit_array_copy(
        &mut self,
        target: &str,
        size_of: &str,
        value: &MirValue,
        locals: &[MirLocal],
    ) -> CodegenResult<()> {
        let src = self.array_source(value, locals)?;
        self.write_indent();
        writeln!(self.output, "memcpy({target}, {src}, sizeof({size_of}));").unwrap();
        Ok(())
    }

    /// `dest = Struct { .. }` when the struct has an array member: one
    /// statement per member, in declaration order. Returns `false`, having
    /// emitted nothing, when the struct is unknown or has no array member.
    pub(super) fn emit_struct_with_array_members(
        &mut self,
        dest_name: &str,
        kind: &AggregateKind,
        operands: &[MirValue],
        locals: &[MirLocal],
    ) -> CodegenResult<bool> {
        let AggregateKind::Struct(name) = kind else {
            return Ok(false);
        };
        let Some(fields) = self.struct_fields.get(name.as_ref()).cloned() else {
            return Ok(false);
        };
        if !fields.iter().any(|(_, ty)| is_array(ty)) {
            return Ok(false);
        }
        if fields.len() != operands.len() {
            return Err(CodegenError::Unsupported(format!(
                "struct `{name}` has {} fields and an array among them, but the literal gives {}",
                fields.len(),
                operands.len()
            )));
        }
        for ((field, ty), value) in fields.iter().zip(operands) {
            let target = format!("{dest_name}.{}", Self::escape_c_keyword(field));
            if is_array(ty) {
                self.emit_array_copy(&target, &target, value, locals)?;
            } else {
                self.write_indent();
                let value = self.value_to_c(value, locals);
                writeln!(self.output, "{target} = {value};").unwrap();
            }
        }
        Ok(true)
    }

    /// Refuse an aggregate the expression form would get wrong: an array
    /// operand inside a tuple or an enum variant, or a struct with an array
    /// member that did not go through `emit_struct_with_array_members`.
    pub(super) fn check_aggregate_has_no_array_member(
        &self,
        kind: &AggregateKind,
        operands: &[MirValue],
        locals: &[MirLocal],
    ) -> CodegenResult<()> {
        let what = match kind {
            AggregateKind::Tuple => "a tuple".to_string(),
            AggregateKind::Variant(name, _, variant) => format!("`{name}::{variant}`"),
            AggregateKind::Struct(name) => {
                let known = self.struct_fields.get(name.as_ref());
                if !known.is_some_and(|f| f.iter().any(|(_, ty)| is_array(ty))) {
                    return Ok(());
                }
                format!("struct `{name}` in this position")
            }
            AggregateKind::Array(_) | AggregateKind::Closure(_) => return Ok(()),
        };
        if matches!(kind, AggregateKind::Struct(_))
            || operands.iter().any(|v| value_is_array(v, locals))
        {
            return Err(CodegenError::Unsupported(format!(
                "the C backend cannot build {what} with a fixed array inside it; \
                 put the array in a named struct and bind the struct with `let`"
            )));
        }
        Ok(())
    }

    /// A store into `target` whose declared type is `ty`. Returns `false`,
    /// having emitted nothing, unless `ty` is an array.
    fn emit_store_if_array(
        &mut self,
        target: &str,
        ty: Option<&MirType>,
        value: &MirRValue,
        locals: &[MirLocal],
    ) -> CodegenResult<bool> {
        if !ty.is_some_and(is_array) {
            return Ok(false);
        }
        let MirRValue::Use(src) = value else {
            return Err(CodegenError::Unsupported(format!(
                "the C backend cannot store this array expression into `{target}`"
            )));
        };
        self.emit_array_copy(target, target, src, locals)?;
        Ok(true)
    }

    /// `base.field = value` or `ptr->field = value` when the field is an array.
    pub(super) fn emit_array_field_store(
        &mut self,
        base: LocalId,
        field: &str,
        value: &MirRValue,
        locals: &[MirLocal],
    ) -> CodegenResult<bool> {
        let base_ty = local_ty(locals, base);
        let field_ty = self.struct_field_type(base_ty, field).cloned();
        let arrow =
            matches!(base_ty, Some(MirType::Ptr(_))) && !base_ty.is_some_and(Self::is_slice_ref);
        let target = format!(
            "{}{}{}",
            self.local_name(base, locals),
            if arrow { "->" } else { "." },
            Self::escape_c_keyword(field)
        );
        self.emit_store_if_array(&target, field_ty.as_ref(), value, locals)
    }

    /// `*ptr = value` when `ptr` points at an array. A pointer to an array is
    /// a pointer to its first element in the emitted C, so the size comes
    /// from the type.
    pub(super) fn emit_array_deref_store(
        &mut self,
        ptr: LocalId,
        value: &MirRValue,
        locals: &[MirLocal],
    ) -> CodegenResult<bool> {
        let Some(MirType::Ptr(inner)) = local_ty(locals, ptr) else {
            return Ok(false);
        };
        if !is_array(inner) {
            return Ok(false);
        }
        let MirRValue::Use(src) = value else {
            return Err(CodegenError::Unsupported(
                "the C backend cannot store this array expression through a pointer".to_string(),
            ));
        };
        let size_of = self.type_to_c(inner);
        let target = self.local_name(ptr, locals);
        self.emit_array_copy(&target, &size_of, src, locals)?;
        Ok(true)
    }

    /// `target = value` for an indexed store whose element is itself an array.
    pub(super) fn emit_array_element_store(
        &mut self,
        target: &str,
        elem_ty: &MirType,
        value: &MirRValue,
        locals: &[MirLocal],
    ) -> CodegenResult<bool> {
        self.emit_store_if_array(target, Some(elem_ty), value, locals)
    }
}
