//! `unwrap`, `expect` and `unwrap_err` stop the program when the value is not
//! there. Before, `None.unwrap()` read the empty payload slot and went on with
//! 0. The stop goes through the runtime's `bl_unwrap_*` and `bl_expect_*`
//! helpers, which print Rust's message on stderr and exit with 101, as an
//! index out of bounds or an integer overflow does.

use std::sync::Arc;

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;

use super::MirLowerer;

/// What a failed unwrap prints.
pub(super) enum UnwrapFail {
    /// A fixed message, as `unwrap()` on None prints.
    Fixed(&'static str),
    /// A fixed message, then the Err payload of this type.
    FixedWithErr(&'static str, MirType),
    /// The `expect` message (a lowered string).
    Expect(MirValue),
    /// The `expect` message, then the Err payload of this type.
    ExpectWithErr(MirValue, MirType),
}

/// How the runtime prints an Err payload of type `ty` (see `bl_unwrap_err_detail`).
fn err_kind(ty: &MirType) -> i128 {
    match ty {
        MirType::Struct(n) if n.as_ref() == "BuildString" => 1,
        MirType::Int(IntSize::I128, _) => 0,
        MirType::Int(_, true) => 2,
        MirType::Int(_, false) => 3,
        MirType::Float(_) => 4,
        _ => 0,
    }
}

impl MirLowerer<'_> {
    /// Continue only when `scrut.flag` is true; otherwise stop the program
    /// with the message `fail` describes. `flag` is `has_value` for an Option
    /// and `is_ok` for a Result; `want` false inverts it (`unwrap_err`).
    pub(super) fn trap_unless(
        &mut self,
        scrut: LocalId,
        flag: &str,
        want: bool,
        fail: UnwrapFail,
    ) -> CodegenResult<()> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let present = builder.create_local(MirType::Bool);
        builder.assign(
            present,
            MirRValue::FieldAccess {
                base: values::local(scrut),
                field_name: Arc::from(flag),
                field_ty: MirType::Bool,
            },
        );
        let ok_bb = builder.create_block();
        let fail_bb = builder.create_block();
        if want {
            builder.branch(values::local(present), ok_bb, fail_bb);
        } else {
            builder.branch(values::local(present), fail_bb, ok_bb);
        }
        builder.switch_to_block(fail_bb);
        let (func, args) = match fail {
            UnwrapFail::Fixed(msg) => {
                let idx = self.module.intern_string(Arc::from(msg));
                ("bl_unwrap_none", vec![MirValue::Const(MirConst::Str(idx))])
            }
            UnwrapFail::FixedWithErr(msg, ty) => {
                let idx = self.module.intern_string(Arc::from(msg));
                (
                    "bl_unwrap_fail",
                    vec![
                        MirValue::Const(MirConst::Str(idx)),
                        values::local(scrut),
                        MirValue::Const(MirConst::Int(err_kind(&ty), MirType::i32())),
                    ],
                )
            }
            UnwrapFail::Expect(msg) => ("bl_expect_none", vec![msg]),
            UnwrapFail::ExpectWithErr(msg, ty) => (
                "bl_expect_fail",
                vec![
                    msg,
                    values::local(scrut),
                    MirValue::Const(MirConst::Int(err_kind(&ty), MirType::i32())),
                ],
            ),
        };
        let builder = self.current_fn.as_mut().unwrap();
        let dead = builder.create_block();
        builder.call(MirValue::Function(Arc::from(func)), args, None, dead);
        builder.switch_to_block(dead);
        // The helper does not return; `abort()` ends the block for C.
        builder.abort();
        builder.switch_to_block(ok_bb);
        Ok(())
    }
}
