//! Option combinators: `map`, `and_then`, `filter`, `map_or`, `unwrap_or_else`
//! and `or`.
//!
//! Each one is lowered to a branch on the Option's `has_value` flag. The
//! function argument may be a closure written in place, a local that holds a
//! closure, or a function's name. A closure written in place takes the
//! payload type for any parameter written without a type, and the captured
//! values are passed after the written arguments, as for any closure call.

use std::sync::Arc;

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;

use super::MirLowerer;

/// A function value ready to call: the callee, its signature, and the
/// captured values that follow the written arguments.
pub(super) struct FnValue {
    callee: MirValue,
    sig: MirFnSig,
    captured: Vec<MirValue>,
}

impl FnValue {
    pub(super) fn ret(&self) -> &MirType {
        &self.sig.ret
    }
}

impl MirLowerer<'_> {
    /// Lower `arg` as a function value. A closure written in place takes
    /// `param_tys` for its parameters that have no written type.
    pub(super) fn lower_fn_value_arg(
        &mut self,
        arg: &ast::Expr,
        param_tys: &[MirType],
    ) -> CodegenResult<FnValue> {
        let is_closure = matches!(arg.kind, ExprKind::Closure { .. });
        if is_closure {
            self.closure_param_hint = Some(param_tys.to_vec());
        }
        let lowered = self.lower_expr(arg);
        self.closure_param_hint = None;
        let callee = lowered?;
        let sig = match &callee {
            MirValue::Function(name) | MirValue::Global(name) => self.fn_value_type(name),
            other => Some(self.type_of_value(other)),
        };
        let Some(MirType::FnPtr(sig)) = sig else {
            return Err(CodegenError::TypeError(
                "this argument has to be a closure or a function".to_string(),
            ));
        };
        let mut captured = Vec::new();
        if let MirValue::Local(id) = &callee {
            if let Some(name) = self.local_closure_name.get(id) {
                if let Some(captures) = self.closure_captures.get(name) {
                    captured.extend(captures.iter().map(|(_, l)| values::local(*l)));
                }
            }
        }
        Ok(FnValue {
            callee,
            sig: *sig,
            captured,
        })
    }

    /// Call a function value with `args`, then its captured values.
    pub(super) fn call_fn_value(
        &mut self,
        f: &FnValue,
        args: Vec<MirValue>,
    ) -> CodegenResult<MirValue> {
        let mut all = args;
        all.extend(f.captured.iter().cloned());
        let builder = self.builder_mut()?;
        let cont = builder.create_block();
        if matches!(f.sig.ret, MirType::Void) {
            builder.call(f.callee.clone(), all, None, cont);
            builder.switch_to_block(cont);
            return Ok(values::unit());
        }
        let result = builder.create_local(f.sig.ret.clone());
        builder.call(f.callee.clone(), all, Some(result), cont);
        builder.switch_to_block(cont);
        if let MirType::Option(inner) = &f.sig.ret {
            self.option_inner_types.insert(result, (**inner).clone());
        }
        Ok(values::local(result))
    }

    /// The return type of a closure with no written return type, from its
    /// lowered body, when that differs from the old i32 default. `None` keeps
    /// the default: a body with no value, or an i32 body.
    pub(super) fn closure_body_type(&self, body_val: &MirValue) -> Option<MirType> {
        if matches!(body_val, MirValue::Const(MirConst::Unit)) {
            return None;
        }
        let ty = self.type_of_value(body_val);
        let ty = match (&ty, body_val) {
            (MirType::Struct(n), MirValue::Local(id)) if n.as_ref() == "Option" => self
                .option_inner_types
                .get(id)
                .map(|inner| MirType::Option(Box::new(inner.clone())))
                .unwrap_or(ty),
            _ => ty,
        };
        match ty {
            MirType::Void | MirType::Never => None,
            t if t == MirType::i32() => None,
            t => Some(t),
        }
    }

    fn builder_mut(&mut self) -> CodegenResult<&mut crate::codegen::builder::MirBuilder> {
        self.current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))
    }

    /// The payload type of an Option-typed function result: `Option<T>` gives
    /// `T`; an Option built by `Some(..)` with no recorded payload gives i32.
    fn option_payload_of(ty: &MirType) -> MirType {
        match ty {
            MirType::Option(inner) => (**inner).clone(),
            _ => MirType::i32(),
        }
    }

    /// Lower an Option combinator, or give `None` when `method` is not one.
    pub(super) fn try_lower_option_combinator(
        &mut self,
        receiver_val: MirValue,
        receiver_ty: &MirType,
        inner_ty: &MirType,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<CodegenResult<MirValue>> {
        let arity = match method {
            "map" | "and_then" | "filter" | "unwrap_or_else" | "or" | "or_else" => 1,
            "map_or" => 2,
            _ => return None,
        };
        if args.len() != arity {
            return None;
        }
        Some(self.lower_option_combinator(receiver_val, receiver_ty, inner_ty, method, args))
    }

    fn lower_option_combinator(
        &mut self,
        receiver_val: MirValue,
        receiver_ty: &MirType,
        inner_ty: &MirType,
        method: &str,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        // The receiver is held in a local, so the payload is read once.
        let builder = self.builder_mut()?;
        let scrut = builder.create_local(receiver_ty.clone());
        builder.assign(scrut, MirRValue::Use(receiver_val));

        // The default of `map_or` is evaluated first, as Rust does.
        let default_val = if method == "map_or" {
            Some(self.lower_expr(&args[0])?)
        } else {
            None
        };
        let f_arg = args.last().expect("arity checked");

        // `or` takes an Option value, not a function.
        let f = if method == "or" {
            None
        } else {
            let params: Vec<MirType> = match method {
                "unwrap_or_else" | "or_else" => Vec::new(),
                _ => vec![inner_ty.clone()],
            };
            Some(self.lower_fn_value_arg(f_arg, &params)?)
        };

        let result_ty = match (method, &f) {
            ("map", Some(f)) => MirType::Option(Box::new(f.ret().clone())),
            ("and_then" | "or_else", Some(f)) => {
                MirType::Option(Box::new(Self::option_payload_of(f.ret())))
            }
            ("map_or", Some(f)) => f.ret().clone(),
            ("unwrap_or_else", _) => inner_ty.clone(),
            _ => receiver_ty_as_option(receiver_ty, inner_ty),
        };

        let builder = self.builder_mut()?;
        let has = builder.create_local(MirType::Bool);
        builder.assign(
            has,
            MirRValue::FieldAccess {
                base: values::local(scrut),
                field_name: Arc::from("has_value"),
                field_ty: MirType::Bool,
            },
        );
        let result = builder.create_local(result_ty.clone());
        let some_bb = builder.create_block();
        let none_bb = builder.create_block();
        let merge = builder.create_block();
        builder.branch(values::local(has), some_bb, none_bb);

        // Some: the payload goes through the function, or the receiver stays.
        builder.switch_to_block(some_bb);
        let payload = builder.create_local(inner_ty.clone());
        builder.assign(
            payload,
            MirRValue::FieldAccess {
                base: values::local(scrut),
                field_name: Arc::from("value"),
                field_ty: inner_ty.clone(),
            },
        );
        match method {
            "map" => {
                let f = f.as_ref().expect("map has a function");
                let mapped = self.call_fn_value(f, vec![values::local(payload)])?;
                self.make_some(result, mapped)?;
            }
            "and_then" | "map_or" => {
                let f = f.as_ref().expect("a function");
                let out = self.call_fn_value(f, vec![values::local(payload)])?;
                self.builder_mut()?.assign(result, MirRValue::Use(out));
            }
            "filter" => {
                let f = f.as_ref().expect("filter has a function");
                let keep = self.call_fn_value(f, vec![values::local(payload)])?;
                let builder = self.builder_mut()?;
                let keep_bb = builder.create_block();
                let drop_bb = builder.create_block();
                builder.branch(keep, keep_bb, drop_bb);
                builder.switch_to_block(keep_bb);
                builder.assign(result, MirRValue::Use(values::local(scrut)));
                builder.goto(merge);
                builder.switch_to_block(drop_bb);
                builder.assign(result, MirRValue::Use(MirValue::Global(Arc::from("None"))));
            }
            "unwrap_or_else" => {
                self.builder_mut()?
                    .assign(result, MirRValue::Use(values::local(payload)));
            }
            _ => {
                // `or` and `or_else` keep a receiver that has a value.
                self.builder_mut()?
                    .assign(result, MirRValue::Use(values::local(scrut)));
            }
        }
        self.builder_mut()?.goto(merge);

        // None: the default, the fallback function, or None.
        self.builder_mut()?.switch_to_block(none_bb);
        match method {
            "map_or" => {
                let d = default_val.expect("map_or has a default");
                self.builder_mut()?.assign(result, MirRValue::Use(d));
            }
            "unwrap_or_else" | "or_else" => {
                let f = f.as_ref().expect("a function");
                let out = self.call_fn_value(f, Vec::new())?;
                self.builder_mut()?.assign(result, MirRValue::Use(out));
            }
            "or" => {
                let prev = self.expected_type.replace(result_ty.clone());
                let other = self.lower_expr(f_arg);
                self.expected_type = prev;
                let other = other?;
                self.builder_mut()?.assign(result, MirRValue::Use(other));
            }
            _ => {
                self.builder_mut()?
                    .assign(result, MirRValue::Use(MirValue::Global(Arc::from("None"))));
            }
        }
        let builder = self.builder_mut()?;
        builder.goto(merge);
        builder.switch_to_block(merge);
        if let MirType::Option(inner) = &result_ty {
            self.option_inner_types.insert(result, (**inner).clone());
        }
        Ok(values::local(result))
    }

    /// `dest = Some(val)`.
    fn make_some(&mut self, dest: LocalId, val: MirValue) -> CodegenResult<()> {
        let builder = self.builder_mut()?;
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("Some")),
            vec![val],
            Some(dest),
            cont,
        );
        builder.switch_to_block(cont);
        Ok(())
    }
}

/// The receiver's type as `Option<T>`, with `T` the known payload type.
fn receiver_ty_as_option(receiver_ty: &MirType, inner_ty: &MirType) -> MirType {
    match receiver_ty {
        MirType::Option(_) => receiver_ty.clone(),
        _ => MirType::Option(Box::new(inner_ty.clone())),
    }
}
