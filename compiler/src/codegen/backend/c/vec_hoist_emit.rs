//! Emission for `vec_hoist.rs`: the hoisted declarations at function entry,
//! and each read or write of a hoisted vector as a checked plain C access.

use std::fmt::Write as _;

use super::vec_hoist::{hoistable_elem, vec_param_kind, VecHoist};
use super::CBackend;
use crate::codegen::ir::{LocalId, MirLocal, MirType, MirValue};

impl CBackend {
    /// Declare the hoisted buffer, length and element size of every vector
    /// parameter the plan uses.
    pub(super) fn emit_vec_hoist_decls(&mut self, plan: &VecHoist, locals: &[MirLocal]) {
        for p in plan.params() {
            let Some(local) = locals.get(p.0 as usize) else {
                continue;
            };
            let name = self.local_name(p, locals);
            let Some((by_ptr, elem)) = vec_param_kind(&local.ty) else {
                continue;
            };
            let ct = self.type_to_c(elem);
            let inner = if by_ptr {
                format!("({name})->inner")
            } else {
                format!("{name}.inner")
            };
            // Accesses index by `sizeof(T)`, a constant the C compiler folds. A
            // vector whose runtime element size differs gets a hoisted length
            // of 0, so its first access stops with a layout message instead of
            // reading at the wrong offsets. A select, not a branch to an abort
            // at entry: the abort call at entry cost n-body about 20 %.
            self.write_indent();
            writeln!(
                self.output,
                "BuildVec* const bl_vv_{id} = {inner}; char* const bl_vp_{id} = (char*)bl_vv_{id}->ptr; const size_t bl_vn_{id} = bl_vv_{id}->elem_size == sizeof({ct}) ? bl_vv_{id}->len : 0;",
                id = p.0
            )
            .unwrap();
        }
    }

    /// `v[i]` as a hoisted read, or `None` to take the runtime path.
    pub(super) fn hoisted_vec_read(
        &self,
        base: &MirValue,
        index_str: &str,
        elem_ty: &MirType,
    ) -> Option<String> {
        let p = self.current_vec_hoist.as_ref()?.param_for(base)?;
        if !hoistable_elem(elem_ty) || self.hoisted_elem(p)? != elem_ty {
            return None;
        }
        let ct = self.type_to_c(elem_ty);
        Some(format!(
            "(*({ct}*)bl_vat(bl_vv_{id}, bl_vp_{id}, bl_vn_{id}, sizeof({ct}), (size_t)({index_str})))",
            id = p.0
        ))
    }

    fn hoisted_elem(&self, p: LocalId) -> Option<&MirType> {
        self.current_vec_hoist.as_ref()?.elems.get(&p)
    }

    /// A `build_hvec_set_<T>(v, i, x)` call as a hoisted write, or `None`.
    pub(super) fn hoisted_set_call(
        &self,
        func_str: &str,
        args: &[MirValue],
        has_dest: bool,
        locals: &[MirLocal],
    ) -> Option<String> {
        let suffix = func_str.strip_prefix("build_hvec_set_")?;
        if has_dest || args.len() != 3 {
            return None;
        }
        let MirValue::Local(v) = &args[0] else {
            return None;
        };
        let MirType::Vec(elem) = &locals.get(v.0 as usize)?.ty else {
            return None;
        };
        if Self::hvec_elem_suffix(elem) != suffix {
            return None;
        }
        let index_str = self.value_to_c(&args[1], locals);
        let rvalue = self.value_to_c(&args[2], locals);
        self.hoisted_vec_write(&args[0], &index_str, elem, &rvalue)
    }

    /// `v[i] = x` as a hoisted write, or `None` to take the runtime path. A
    /// struct element keeps the struct setter's abort message.
    pub(super) fn hoisted_vec_write(
        &self,
        base: &MirValue,
        index_str: &str,
        elem_ty: &MirType,
        rvalue: &str,
    ) -> Option<String> {
        let p = self.current_vec_hoist.as_ref()?.param_for(base)?;
        if !hoistable_elem(elem_ty) || self.hoisted_elem(p)? != elem_ty {
            return None;
        }
        let ct = self.type_to_c(elem_ty);
        let at = if Self::vec_elem_needs_sized_wrapper(elem_ty) {
            "bl_vat_set"
        } else {
            "bl_vat"
        };
        Some(format!(
            "*({ct}*){at}(bl_vv_{id}, bl_vp_{id}, bl_vn_{id}, sizeof({ct}), (size_t)({index_str})) = {rvalue};\n",
            id = p.0
        ))
    }
}
