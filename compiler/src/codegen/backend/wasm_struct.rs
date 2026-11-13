//! Struct values for the WASM backend.
//!
//! A struct value is an i32 handle to a heap record laid out from the
//! module's `MirTypeDef` (natural alignment, total size a multiple of 8).
//! A field write copies the record first and rebinds the local to the copy,
//! so no other holder of the old handle sees the change: the observable
//! behaviour is the C backend's by-value structs.

use super::*;

pub(super) struct StructLayout {
    /// (field name, offset, type)
    fields: Vec<(String, u32, MirType)>,
    size: u32,
}

/// (size, load, store) for a field; handles (Vec, string, struct) are i32.
fn slot_ops(ty: &MirType) -> (u32, &'static str, &'static str) {
    super::mem::mem_ops(ty).unwrap_or((4, "i32.load", "i32.store"))
}

impl WasmBackend {
    /// Record the layout of every struct type in the module.
    pub(super) fn plan_structs(&mut self, mir: &MirModule) {
        for def in &mir.types {
            if let TypeDefKind::Struct { fields, .. } = &def.kind {
                let mut off = 0u32;
                let mut out = Vec::new();
                for (i, (name, ty)) in fields.iter().enumerate() {
                    let (size, _, _) = slot_ops(ty);
                    let align = size.clamp(1, 8);
                    off = (off + align - 1) / align * align;
                    let fname = name
                        .as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or(i.to_string());
                    out.push((fname, off, ty.clone()));
                    off += size;
                }
                let size = ((off + 7) & !7).max(8);
                self.rt
                    .structs
                    .insert(def.name.to_string(), StructLayout { fields: out, size });
            }
        }
    }

    fn struct_name_of(ty: &MirType) -> Option<String> {
        match ty {
            MirType::Struct(n) if n.as_ref() != "BuildString" => Some(n.to_string()),
            _ => None,
        }
    }

    fn field_slot(&self, sname: &str, field: &str) -> CodegenResult<(u32, MirType, u32)> {
        let layout = self.rt.structs.get(sname).ok_or_else(|| {
            CodegenError::Unsupported(format!("struct `{sname}` has no layout in WASM"))
        })?;
        layout
            .fields
            .iter()
            .find(|f| f.0 == field)
            .map(|f| (f.1, f.2.clone(), layout.size))
            .ok_or_else(|| CodegenError::Unsupported(format!("field `{sname}.{field}`")))
    }

    /// `base.field` read.
    pub(super) fn gen_field_load(
        &mut self,
        base: &MirValue,
        field_name: &str,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        let bty = self.infer_value_type(base, func)?;
        let Some(sname) = Self::struct_name_of(&bty) else {
            return Err(CodegenError::Unsupported(format!(
                "field access on a {:?} value in WASM",
                bty
            )));
        };
        let (off, fty, _) = self.field_slot(&sname, field_name)?;
        self.gen_value(base, func)?;
        self.emit_line(&format!("i32.const {}", off));
        self.emit_line("i32.add");
        self.emit_line(slot_ops(&fty).1);
        Ok(())
    }

    /// Struct construction and field writes. Returns false for other statements.
    pub(super) fn gen_struct_stmt(
        &mut self,
        stmt: &MirStmt,
        func: &MirFunction,
    ) -> CodegenResult<bool> {
        match &stmt.kind {
            MirStmtKind::Assign {
                dest,
                value:
                    MirRValue::Aggregate {
                        kind: AggregateKind::Struct(sname),
                        operands,
                    },
            } => {
                let layout = self.rt.structs.get(sname.as_ref()).ok_or_else(|| {
                    CodegenError::Unsupported(format!("struct `{sname}` has no layout in WASM"))
                })?;
                if layout.fields.len() != operands.len() {
                    return Err(CodegenError::Unsupported(format!(
                        "struct `{sname}` literal with {} of {} fields",
                        operands.len(),
                        layout.fields.len()
                    )));
                }
                let slots: Vec<(u32, MirType)> =
                    layout.fields.iter().map(|f| (f.1, f.2.clone())).collect();
                let size = layout.size;
                let d = self.local_names.get(dest).cloned().unwrap_or_default();
                self.emit_line(&format!("i32.const {}", size));
                self.emit_line("call $__alloc");
                self.emit_line(&format!("local.set {}", d));
                for ((off, fty), op) in slots.iter().zip(operands) {
                    self.emit_line(&format!("local.get {}", d));
                    self.emit_line(&format!("i32.const {}", off));
                    self.emit_line("i32.add");
                    self.gen_value(op, func)?;
                    self.emit_line(slot_ops(fty).2);
                }
                Ok(true)
            }
            MirStmtKind::FieldAssign {
                base,
                field_name,
                value,
            } => {
                let bty = func
                    .locals
                    .iter()
                    .find(|l| l.id == *base)
                    .map(|l| l.ty.clone())
                    .unwrap_or(MirType::Void);
                let Some(sname) = Self::struct_name_of(&bty) else {
                    return Err(CodegenError::Unsupported(format!(
                        "field write on a {:?} value in WASM",
                        bty
                    )));
                };
                let (off, fty, size) = self.field_slot(&sname, field_name)?;
                let b = self.local_names.get(base).cloned().unwrap_or_default();
                for line in [
                    format!("i32.const {}", size),
                    "call $__alloc".into(),
                    "local.set $__tmp".into(),
                    "local.get $__tmp".into(),
                    format!("local.get {}", b),
                    format!("i32.const {}", size),
                    "memory.copy".into(),
                    "local.get $__tmp".into(),
                    format!("local.set {}", b),
                    format!("local.get {}", b),
                    format!("i32.const {}", off),
                    "i32.add".into(),
                ] {
                    self.emit_line(&line);
                }
                self.gen_rvalue(value, func)?;
                self.emit_line(slot_ops(&fty).2);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
