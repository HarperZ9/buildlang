// ===============================================================================
// BUILDLANG MIR LOWERING - PARALLEL MAP AND FOR (M5)
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Lowers `par_for(n, ctx, f)` and `par_map(xs, f)` to one call of a
//! per-body C wrapper, `build_par_for__<f>` or `build_par_map__<f>`. The C
//! backend emits each wrapper beside a fixed thread pool; the wrapper writes
//! `f`'s result for index `i` into slot `i` of a new `Vec`, so the result is
//! the serial result whatever the schedule.
//!
//! The checker (`types::parallel`) has already proved the body has an empty
//! effect row and takes no `&mut`; lowering repeats the shape checks it needs
//! to emit correct C and rejects anything else instead of guessing.

use std::sync::Arc;

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;

use super::MirLowerer;

/// The wrapper name prefix for `par_for` sites.
pub(crate) const PAR_FOR_PREFIX: &str = "build_par_for__";
/// The wrapper name prefix for `par_map` sites over a `Vec` value.
pub(crate) const PAR_MAP_PREFIX: &str = "build_par_map__";
/// The wrapper name prefix for `par_map` sites over a `&Vec`.
pub(crate) const PAR_MAP_REF_PREFIX: &str = "build_par_map_ref__";

/// `Some("par_for")` or `Some("par_map")` when `func` names one of them.
pub(crate) fn par_callee(func: &ast::Expr) -> Option<&'static str> {
    let name = match &func.kind {
        ExprKind::Ident(i) => i.name.clone(),
        ExprKind::Path(p) if p.is_simple() => p.last_ident()?.name.clone(),
        _ => return None,
    };
    match name.as_ref() {
        "par_for" => Some("par_for"),
        "par_map" => Some("par_map"),
        _ => None,
    }
}

fn rejected(message: String, help: &str) -> CodegenError {
    CodegenError::Rejected {
        message,
        location: None,
        help: Some(help.to_string()),
    }
}

impl<'ctx> MirLowerer<'ctx> {
    /// Lower a `par_for` or `par_map` call. `kind` is the callee name.
    pub(crate) fn lower_par_call(
        &mut self,
        kind: &str,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        let (data_args, body_expr) = match (kind, args.len()) {
            ("par_for", 3) => (&args[..2], &args[2]),
            ("par_map", 2) => (&args[..1], &args[1]),
            _ => {
                return Err(rejected(
                    format!("`{kind}` called with {} arguments", args.len()),
                    "write `par_for(n, &ctx, f)` or `par_map(xs, f)`",
                ))
            }
        };
        let body = self.par_body_function(kind, body_expr)?;
        let sig = self
            .module
            .find_function(body.as_ref())
            .map(|f| f.sig.clone())
            .ok_or_else(|| {
                rejected(
                    format!("the parallel body `{body}` has no known signature"),
                    "pass a named function or a closure that captures nothing",
                )
            })?;
        if matches!(sig.ret, MirType::Void | MirType::Never | MirType::Array(..)) {
            return Err(rejected(
                format!("the parallel body `{body}` must return a value"),
                "return the element the result vector holds at this index",
            ));
        }
        let mut lowered = Vec::with_capacity(data_args.len());
        for a in data_args {
            lowered.push(self.lower_expr(a)?);
        }
        let wrapper = match kind {
            "par_for" => {
                let ok = sig.params.len() == 2 && matches!(sig.params[1], MirType::Int(..));
                if !ok {
                    return Err(rejected(
                        format!("`par_for` body `{body}` must take `(ctx, i: i64)`"),
                        "declare the body as `fn f(ctx: &C, i: i64) -> T`",
                    ));
                }
                format!("{PAR_FOR_PREFIX}{body}")
            }
            _ => {
                if sig.params.len() != 1 {
                    return Err(rejected(
                        format!("`par_map` body `{body}` must take one element"),
                        "declare the body as `fn f(x: T) -> U`",
                    ));
                }
                let elem = match self.type_of_value(&lowered[0]) {
                    MirType::Vec(e) => (*e, PAR_MAP_PREFIX),
                    MirType::Ptr(inner) => match *inner {
                        MirType::Vec(e) => (*e, PAR_MAP_REF_PREFIX),
                        other => return Err(par_map_input_error(&other)),
                    },
                    other => return Err(par_map_input_error(&other)),
                };
                if elem.0 != sig.params[0] {
                    return Err(rejected(
                        format!(
                            "`par_map` body `{body}` takes `{:?}` but the vector holds `{:?}`",
                            sig.params[0], elem.0
                        ),
                        "make the body's parameter the vector's element type",
                    ));
                }
                format!("{}{body}", elem.1)
            }
        };
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(MirType::Vec(Box::new(sig.ret.clone())));
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from(wrapper)),
            lowered,
            Some(result),
            cont,
        );
        builder.switch_to_block(cont);
        Ok(values::local(result))
    }

    /// The MIR name of a parallel body: a named function, or a closure that
    /// captures nothing (lowered to a plain function).
    fn par_body_function(&mut self, kind: &str, expr: &ast::Expr) -> CodegenResult<Arc<str>> {
        match &expr.kind {
            ExprKind::Paren(inner) => self.par_body_function(kind, inner),
            ExprKind::Path(_) | ExprKind::Ident(_) if par_simple_name(expr).is_some() => {
                let name: Arc<str> = Arc::from(par_simple_name(expr).unwrap_or_default());
                let prefixed = self.prefixed_name(&name);
                if self.module.find_function(prefixed.as_ref()).is_some() {
                    Ok(prefixed)
                } else {
                    Ok(name)
                }
            }
            ExprKind::Closure { .. } => {
                let value = self.lower_expr(expr)?;
                let name = match value {
                    MirValue::Local(id) => self.local_closure_name.get(&id).cloned(),
                    _ => None,
                };
                let name = name.ok_or_else(|| {
                    CodegenError::Internal("closure did not lower to a function".to_string())
                })?;
                if self.closure_captures.contains_key(&name) {
                    return Err(rejected(
                        format!("the `{kind}` closure captures variables"),
                        "pass captured data through the context argument of `par_for`",
                    ));
                }
                Ok(name)
            }
            _ => Err(rejected(
                format!("the `{kind}` body must be a function name or a closure"),
                "a function-pointer variable hides its effect row from the checker",
            )),
        }
    }
}

fn par_simple_name(e: &ast::Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(i) => Some(i.name.to_string()),
        ExprKind::Path(p) if p.is_simple() => p.last_ident().map(|i| i.name.to_string()),
        _ => None,
    }
}

fn par_map_input_error(found: &MirType) -> CodegenError {
    rejected(
        format!("`par_map` needs a `Vec` or `&Vec` input, found `{found:?}`"),
        "pass the vector whose elements the body maps",
    )
}
