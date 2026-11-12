//! Module globals and mutable statics of aggregate type.
//!
//! An array global was declared `uint32_t[2] G;`, which is not C, and its
//! initializer was dropped (G34, G5). A global is now declared with the array
//! declarator after its name. `&GLOBAL` (MIR `AddressOfGlobal`) is the global's
//! own address, and a whole-array store into an array global is a copy (G35).

use std::fmt::Write;

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::{MirGlobal, MirLocal, MirRValue, MirType, MirValue};

use super::CBackend;

impl CBackend {
    /// `T NAME`, `const T NAME` or `T NAME[N][M]`, without the initializer.
    pub(super) fn global_declarator(&self, global: &MirGlobal) -> String {
        let decl = if matches!(global.ty, MirType::Array(..)) {
            self.fmt_array_decl(&global.ty, &global.name)
        } else {
            format!("{} {}", self.type_to_c(&global.ty), global.name)
        };
        // A foreign static is declared without `const`: the other side owns
        // the object, and a C declaration that calls it constant would let
        // the C compiler fold a value the foreign code may change.
        if global.is_mut || global.is_extern_decl {
            decl
        } else {
            format!("const {decl}")
        }
    }

    /// The C name a global is read under, as `value_to_c` writes it.
    fn global_c_name(&self, name: &str) -> String {
        self.value_to_c(&MirValue::Global(name.into()), &[])
    }

    /// `&GLOBAL`. An array global is its own address in C: the name decays to
    /// a pointer to the first element, which is how `Ptr(Array)` is typed.
    pub(super) fn global_address_to_c(&self, name: &str) -> String {
        let c_name = self.global_c_name(name);
        match self.global_types.get(name) {
            Some(MirType::Array(..)) => c_name,
            _ => format!("&({c_name})"),
        }
    }

    /// `G[i]` checked against the length of array global `G`. A read of a
    /// global array element had no bounds check.
    pub(super) fn global_array_element_access(
        &self,
        base: &MirValue,
        base_str: &str,
        index_str: &str,
    ) -> Option<String> {
        let MirValue::Global(name) = base else {
            return None;
        };
        match self.global_types.get(name.as_ref())? {
            MirType::Array(_, len) => Some(format!(
                "{base_str}[bl_idx_chk((long long)({index_str}), {len}ULL)]"
            )),
            _ => None,
        }
    }

    /// `GLOBAL = value` for an array global: C cannot assign an array, so
    /// the value is copied. Returns `false` when `name` is not an array
    /// global; an array global stored from anything but a plain operand is
    /// refused by name.
    pub(super) fn emit_global_array_store(
        &mut self,
        name: &str,
        value: &MirRValue,
        locals: &[MirLocal],
    ) -> CodegenResult<bool> {
        if !matches!(self.global_types.get(name), Some(MirType::Array(..))) {
            return Ok(false);
        }
        let MirRValue::Use(src) = value else {
            return Err(CodegenError::Rejected {
                message: format!(
                    "the C backend cannot store this value into array global `{name}`"
                ),
                location: None,
                help: Some("bind the array to a local first, then assign the local".to_string()),
            });
        };
        let src = self.value_to_c(src, locals);
        let dst = self.global_c_name(name);
        self.write_indent();
        writeln!(self.output, "memcpy({dst}, {src}, sizeof({dst}));").unwrap();
        Ok(true)
    }
}
