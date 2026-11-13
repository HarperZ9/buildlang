//! `s.parse::<T>()` on a string: `Result<T, str>`, as `json_parse` returns.
//!
//! The runtime helpers `build_parse_int` and `build_parse_float` follow Rust's
//! rules: the whole string is the number, a leading `+` is allowed, whitespace
//! is an invalid digit, and a value outside `T`'s range is an error. The error
//! text names the cause with Rust's wording ("invalid digit found in string").

use std::sync::Arc;

use crate::ast;
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;
use crate::lexer::Span;

use super::MirLowerer;

/// The runtime call for one target type: its name and the arguments after
/// the string.
fn parse_call(target: &MirType) -> Option<(&'static str, Vec<MirValue>)> {
    match target {
        MirType::Int(size, signed) => {
            let bits = match size {
                IntSize::I8 => 8,
                IntSize::I16 => 16,
                IntSize::I32 => 32,
                IntSize::I64 | IntSize::ISize => 64,
                IntSize::I128 => return None,
            };
            Some((
                "build_parse_int",
                vec![
                    MirValue::Const(MirConst::Int(bits, MirType::i32())),
                    MirValue::Const(MirConst::Bool(*signed)),
                ],
            ))
        }
        MirType::Float(size) => Some((
            "build_parse_float",
            vec![MirValue::Const(MirConst::Bool(matches!(
                size,
                FloatSize::F32
            )))],
        )),
        _ => None,
    }
}

impl MirLowerer<'_> {
    /// A `Result<T, E>` parameter records its payload types, as a `let`
    /// annotation does, so `match r { Ok(v) => .. }` reads the right slot.
    /// Before, an f64 or i64 Ok in a parameter was read from the i32 slot.
    /// The tables are keyed by local id, so a parameter that is not a Result
    /// clears an entry left by an earlier function.
    pub(super) fn record_result_param(&mut self, local_id: LocalId, ty: &ast::Type) {
        match self.result_ok_inner_from_ast(ty) {
            Some(ok) => self.result_ok_types.insert(local_id, ok),
            None => self.result_ok_types.remove(&local_id),
        };
        match self.result_err_inner_from_ast(ty) {
            Some(err) => self.result_err_types.insert(local_id, err),
            None => self.result_err_types.remove(&local_id),
        };
    }

    /// True when `receiver` is a string, held or borrowed.
    pub(super) fn is_string_receiver(&self, receiver: &ast::Expr) -> bool {
        let is_str = |t: &MirType| matches!(t, MirType::Struct(n) if n.as_ref() == "BuildString");
        match self.infer_expr_type(receiver) {
            MirType::Ptr(inner) => is_str(&inner),
            t => is_str(&t),
        }
    }

    /// Lower `receiver.parse::<T>()`. `T` is the turbofish type, else the Ok
    /// type of the `let` annotation the call initialises.
    pub(super) fn lower_string_parse(
        &mut self,
        receiver: &ast::Expr,
        generics: &[ast::GenericArg],
        span: Span,
    ) -> CodegenResult<MirValue> {
        // Both refusals point at the `parse` name, so `check` reports the line.
        let location = (span.end.0 > span.start.0).then_some((span.start.0, span.end.0));
        let target = match generics.first() {
            Some(ast::GenericArg::Type(t)) => Some(self.lower_type_from_ast(t)),
            _ => self.parse_target_hint.clone(),
        };
        let Some(target) = target else {
            return Err(CodegenError::Rejected {
                message: "`parse` needs its target type here".to_string(),
                location,
                help: Some(
                    "write the type: `s.parse::<i32>()`, or annotate the result: \
                     `let r: Result<i32, str> = s.parse();`"
                        .to_string(),
                ),
            });
        };
        let Some((func, extra)) = parse_call(&target) else {
            return Err(CodegenError::Rejected {
                message: format!("`parse::<{target}>()` is not supported"),
                location,
                help: Some(
                    "parse gives an i8 to i64, u8 to u64, isize, usize, f32 or f64".to_string(),
                ),
            });
        };
        let val = self.lower_expr(receiver)?;
        let val = self.deref_if_pointer(val)?;
        let mut args = vec![val];
        args.extend(extra);
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(MirType::Struct(Arc::from("Result")));
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from(func)),
            args,
            Some(result),
            cont,
        );
        builder.switch_to_block(cont);
        self.result_ok_types.insert(result, target);
        self.result_err_types
            .insert(result, MirType::Struct(Arc::from("BuildString")));
        Ok(values::local(result))
    }
}
