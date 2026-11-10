//! Calls the lowerer reserves for machine-level access: inline assembly
//! (`asm!` with string templates only) and volatile loads and stores through
//! a raw pointer (`ptr::read_volatile`, `ptr::write_volatile`). Each becomes
//! one C statement; none is a call to a runtime function.

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::{LocalId, MirConst, MirLocal, MirType, MirValue};

use super::CBackend;

/// The reserved callee names. `codegen::lower::lowlevel` emits them.
pub(crate) const INLINE_ASM: &str = "bl_inline_asm";
pub(crate) const VOLATILE_READ: &str = "bl_volatile_read";
pub(crate) const VOLATILE_WRITE: &str = "bl_volatile_write";

/// The source form of the first reserved low-level call in `mir`, if any.
/// Only the C backend lowers these; every other target must refuse them
/// rather than emit a call to a function that does not exist.
pub(crate) fn reserved_call_in(mir: &crate::codegen::ir::MirModule) -> Option<&'static str> {
    use crate::codegen::ir::MirTerminator;
    let blocks = mir
        .functions
        .iter()
        .filter_map(|f| f.blocks.as_ref())
        .flatten();
    for term in blocks.filter_map(|b| b.terminator.as_ref()) {
        if let MirTerminator::Call {
            func: MirValue::Function(name),
            ..
        } = term
        {
            match name.as_ref() {
                INLINE_ASM => return Some("`asm!`"),
                VOLATILE_READ => return Some("`ptr::read_volatile`"),
                VOLATILE_WRITE => return Some("`ptr::write_volatile`"),
                _ => {}
            }
        }
    }
    None
}

fn internal(msg: &str) -> CodegenError {
    CodegenError::Internal(msg.to_string())
}

impl CBackend {
    /// The C statement for a reserved low-level call, or `None` when
    /// `func` is not one of them.
    pub(super) fn lowlevel_call(
        &self,
        func: &str,
        args: &[MirValue],
        dest: Option<&LocalId>,
        locals: &[MirLocal],
    ) -> CodegenResult<Option<String>> {
        match func {
            INLINE_ASM => self.inline_asm_stmt(args).map(Some),
            VOLATILE_READ => {
                let dest = dest.ok_or_else(|| internal("volatile read has no destination"))?;
                let ty = locals
                    .get(dest.0 as usize)
                    .map(|l| l.ty.clone())
                    .ok_or_else(|| internal("volatile read destination is unknown"))?;
                let ptr = args
                    .first()
                    .ok_or_else(|| internal("volatile read has no pointer"))?;
                Ok(Some(format!(
                    "{} = *(volatile {} *)({});\n",
                    self.local_name(*dest, locals),
                    self.type_to_c(&ty),
                    self.value_to_c(ptr, locals)
                )))
            }
            VOLATILE_WRITE => {
                let (ptr, val) = match args {
                    [p, v] => (p, v),
                    _ => return Err(internal("volatile write takes a pointer and a value")),
                };
                let pointee = match self.value_mir_type(ptr, locals) {
                    Some(MirType::Ptr(inner)) => (**inner).clone(),
                    _ => return Err(internal("volatile write pointer type is unknown")),
                };
                let cty = self.type_to_c(&pointee);
                Ok(Some(format!(
                    "*(volatile {cty} *)({}) = ({cty})({});\n",
                    self.value_to_c(ptr, locals),
                    self.value_to_c(val, locals)
                )))
            }
            _ => Ok(None),
        }
    }

    /// `__asm__ __volatile__("...");` from the template strings, one line
    /// each, as `asm!` joins them.
    fn inline_asm_stmt(&self, args: &[MirValue]) -> CodegenResult<String> {
        let mut lines = Vec::with_capacity(args.len());
        for arg in args {
            let MirValue::Const(MirConst::Str(idx)) = arg else {
                return Err(internal("asm! template is not a string literal"));
            };
            let text = self
                .string_literals
                .get(*idx as usize)
                .ok_or_else(|| internal("asm! template string is missing"))?;
            lines.push(self.escape_string(text));
        }
        Ok(format!(
            "__asm__ __volatile__(\"{}\");\n",
            lines.join("\\n\\t")
        ))
    }
}
