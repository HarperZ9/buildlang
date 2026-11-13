//! Linear memory for the WASM backend: a shadow stack and fixed arrays.
//!
//! `$__sp` is a mutable i32 global holding the shadow stack pointer; it starts
//! at the top of memory and grows down. A function that has array locals
//! reserves one frame on entry (`$__fp`) and gives each array a fixed slot,
//! so an array local holds the address of its own storage. Array assignment
//! copies bytes, which keeps value semantics. Every `return` releases the
//! frame. Elements use their real sizes (u8 is one byte, u32 four, u64 eight).

use super::*;

#[derive(Default)]
pub(super) struct MemState {
    /// Array locals: local -> (frame offset, element type, element count).
    slots: HashMap<LocalId, (u32, MirType, u64)>,
    frame: u32,
}

fn unsupported<T>(msg: String) -> CodegenResult<T> {
    Err(CodegenError::Unsupported(msg))
}

/// (size, load, store) for a scalar element type.
fn mem_ops(ty: &MirType) -> Option<(u32, &'static str, &'static str)> {
    Some(match ty {
        MirType::Bool => (1, "i32.load8_u", "i32.store8"),
        MirType::Int(IntSize::I8, signed) => (
            1,
            if *signed {
                "i32.load8_s"
            } else {
                "i32.load8_u"
            },
            "i32.store8",
        ),
        MirType::Int(IntSize::I16, signed) => (
            2,
            if *signed {
                "i32.load16_s"
            } else {
                "i32.load16_u"
            },
            "i32.store16",
        ),
        MirType::Int(IntSize::I32, _) => (4, "i32.load", "i32.store"),
        MirType::Int(IntSize::I64 | IntSize::ISize, _) => (8, "i64.load", "i64.store"),
        MirType::Float(FloatSize::F32) => (4, "f32.load", "f32.store"),
        MirType::Float(FloatSize::F64) => (8, "f64.load", "f64.store"),
        _ => return None,
    })
}

impl WasmBackend {
    /// Lay out the frame for this function's array locals.
    pub(super) fn plan_frame(&mut self, func: &MirFunction) -> CodegenResult<()> {
        self.mem = MemState::default();
        for local in &func.locals {
            if let MirType::Array(elem, count) = &local.ty {
                if local.is_param {
                    return unsupported(format!("`{}`: array parameters in WASM", func.name));
                }
                let Some((size, _, _)) = mem_ops(elem) else {
                    return unsupported(format!("`{}`: arrays of {:?} in WASM", func.name, elem));
                };
                let off = (self.mem.frame + 7) & !7;
                self.mem.frame = off + size * (*count as u32);
                self.mem
                    .slots
                    .insert(local.id, (off, (**elem).clone(), *count as u64));
            }
        }
        if matches!(func.sig.ret, MirType::Array(..)) {
            return unsupported(format!("`{}`: array return values in WASM", func.name));
        }
        self.mem.frame = (self.mem.frame + 7) & !7;
        Ok(())
    }

    fn local_name(&self, id: LocalId) -> CodegenResult<String> {
        self.local_names
            .get(&id)
            .cloned()
            .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", id)))
    }

    /// Reserve the frame and point each array local at its slot.
    pub(super) fn emit_prologue(&mut self) -> CodegenResult<()> {
        if self.mem.frame == 0 {
            return Ok(());
        }
        self.emit_line("global.get $__sp");
        self.emit_line(&format!("i32.const {}", self.mem.frame));
        self.emit_line("i32.sub");
        self.emit_line("local.tee $__fp");
        self.emit_line("global.get $__stack_limit");
        self.emit_line("i32.lt_u");
        self.emit_line("if");
        self.emit_line("unreachable");
        self.emit_line("end");
        self.emit_line("local.get $__fp");
        self.emit_line("global.set $__sp");
        let mut slots: Vec<(LocalId, u32)> =
            self.mem.slots.iter().map(|(id, s)| (*id, s.0)).collect();
        slots.sort_by_key(|s| s.1);
        for (id, off) in slots {
            let name = self.local_name(id)?;
            self.emit_line("local.get $__fp");
            self.emit_line(&format!("i32.const {}", off));
            self.emit_line("i32.add");
            self.emit_line(&format!("local.set {}", name));
        }
        Ok(())
    }

    /// Release the frame (before `return`).
    pub(super) fn emit_epilogue(&mut self) {
        if self.mem.frame == 0 {
            return;
        }
        self.emit_line("local.get $__fp");
        self.emit_line(&format!("i32.const {}", self.mem.frame));
        self.emit_line("i32.add");
        self.emit_line("global.set $__sp");
    }

    fn array_of(&self, v: &MirValue) -> Option<(u32, MirType, u64)> {
        match v {
            MirValue::Local(id) => self.mem.slots.get(id).cloned(),
            _ => None,
        }
    }

    /// Push `base + index * size` as an i32 address.
    fn gen_index_addr(
        &mut self,
        base: &MirValue,
        index: &MirValue,
        size: u32,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        self.gen_value(base, func)?;
        let ity = self.infer_value_type(index, func)?;
        match index {
            MirValue::Const(MirConst::Int(n, _)) => self.emit_line(&format!("i32.const {}", n)),
            MirValue::Const(MirConst::Uint(n, _)) => self.emit_line(&format!("i32.const {}", n)),
            _ => {
                self.gen_value(index, func)?;
                if self.emit_type(&ity) == "i64" {
                    self.emit_line("i32.wrap_i64");
                }
            }
        }
        self.emit_line(&format!("i32.const {}", size));
        self.emit_line("i32.mul");
        self.emit_line("i32.add");
        Ok(())
    }

    /// `base[index]` read with the element's real width.
    pub(super) fn gen_index_load(
        &mut self,
        base: &MirValue,
        index: &MirValue,
        elem_ty: &MirType,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        if self.array_of(base).is_none() {
            if let MirType::Vec(_) = self.infer_value_type(base, func)? {
                return self.gen_vec_access("get", base, index, None, elem_ty, func);
            }
            return unsupported("indexing a value that is not a fixed array local".into());
        }
        let Some((size, load, _)) = mem_ops(elem_ty) else {
            return unsupported(format!("array element {:?}", elem_ty));
        };
        self.gen_index_addr(base, index, size, func)?;
        self.emit_line(load);
        Ok(())
    }

    fn store_at(
        &mut self,
        addr_local: &str,
        off: u32,
        store: &str,
        emit_value: &dyn Fn(&mut Self) -> CodegenResult<()>,
    ) -> CodegenResult<()> {
        self.emit_line(&format!("local.get {}", addr_local));
        self.emit_line(&format!("i32.const {}", off));
        self.emit_line("i32.add");
        emit_value(self)?;
        self.emit_line(store);
        Ok(())
    }

    /// Statements that touch memory. Returns false when the statement is not one.
    pub(super) fn gen_mem_stmt(
        &mut self,
        stmt: &MirStmt,
        func: &MirFunction,
    ) -> CodegenResult<bool> {
        match &stmt.kind {
            MirStmtKind::IndexStore {
                base,
                index,
                elem_ty,
                value,
            } => {
                if self.array_of(base).is_none() {
                    if let MirType::Vec(_) = self.infer_value_type(base, func)? {
                        self.gen_vec_access("set", base, index, Some(value), elem_ty, func)?;
                        return Ok(true);
                    }
                    return unsupported(
                        "indexed store to a value that is not a fixed array local".into(),
                    );
                }
                let Some((size, _, store)) = mem_ops(elem_ty) else {
                    return unsupported(format!("array element {:?}", elem_ty));
                };
                self.gen_index_addr(base, index, size, func)?;
                self.gen_rvalue(value, func)?;
                self.emit_line(store);
                Ok(true)
            }
            MirStmtKind::Assign { dest, value } => {
                let Some((_, elem, count)) = self.mem.slots.get(dest).cloned() else {
                    return Ok(false);
                };
                let (size, load, store) = mem_ops(&elem).expect("planned element");
                let d = self.local_name(*dest)?;
                match value {
                    MirRValue::Aggregate { operands, .. } => {
                        for (i, op) in operands.iter().enumerate() {
                            let op = op.clone();
                            self.store_at(&d, i as u32 * size, store, &|s: &mut Self| {
                                s.gen_value(&op, func)
                            })?;
                        }
                    }
                    MirRValue::Repeat { value, .. } => {
                        if count > 4096 {
                            return unsupported(format!("repeat of {} elements in WASM", count));
                        }
                        for i in 0..count as u32 {
                            let v = value.clone();
                            self.store_at(&d, i * size, store, &|s: &mut Self| {
                                s.gen_value(&v, func)
                            })?;
                        }
                    }
                    MirRValue::Use(MirValue::Local(src)) if self.mem.slots.contains_key(src) => {
                        let s = self.local_name(*src)?;
                        for i in 0..count as u32 {
                            let s2 = s.clone();
                            self.store_at(&d, i * size, store, &|b: &mut Self| {
                                b.emit_line(&format!("local.get {}", s2));
                                b.emit_line(&format!("i32.const {}", i * size));
                                b.emit_line("i32.add");
                                b.emit_line(load);
                                Ok(())
                            })?;
                        }
                    }
                    other => {
                        return unsupported(format!("array assignment from {:?}", other));
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Keep sub-word integers in range after a cast or arithmetic.
    pub(super) fn narrow_int(&mut self, ty: &MirType) {
        match ty {
            MirType::Int(IntSize::I8, false) => {
                self.emit_line("i32.const 255");
                self.emit_line("i32.and");
            }
            MirType::Int(IntSize::I16, false) => {
                self.emit_line("i32.const 65535");
                self.emit_line("i32.and");
            }
            MirType::Int(IntSize::I8, true) => self.emit_line("i32.extend8_s"),
            MirType::Int(IntSize::I16, true) => self.emit_line("i32.extend16_s"),
            _ => {}
        }
    }

    /// Whether binary output can lower this statement.
    pub(super) fn stmt_supported(stmt: &MirStmt) -> bool {
        match &stmt.kind {
            MirStmtKind::Assign { value, .. } => match value {
                MirRValue::Use(_)
                | MirRValue::BinaryOp { .. }
                | MirRValue::UnaryOp { .. }
                | MirRValue::Cast { .. }
                | MirRValue::IndexAccess { .. }
                | MirRValue::Repeat { .. } => true,
                MirRValue::Aggregate { kind, .. } => matches!(kind, AggregateKind::Array(_)),
                _ => false,
            },
            MirStmtKind::IndexStore { .. }
            | MirStmtKind::StorageLive(_)
            | MirStmtKind::StorageDead(_)
            | MirStmtKind::Nop => true,
            _ => false,
        }
    }
}
