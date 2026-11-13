//! Tail-resumptive handler clauses.
//!
//! A clause whose every exit is `resume(value)` is lowered as a function
//! `void f(BuildHandler args, BuildHandler caps, R* result)`. `handle`
//! registers it on its handler with `build_handler_tail`, and `build_perform`
//! calls it at the `perform` site, so `perform` returns `value` and the code
//! after it runs. The clause reads its arguments from `args.handler_data` and
//! the locals it captures from `caps.handler_data` (pointers into the frame
//! of the `handle`, which is live below the `perform`). A clause without
//! `resume` keeps the setjmp/longjmp path and ends the `handle`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::ast;
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::MirBuilder;
use crate::codegen::ir::*;

use super::MirLowerer;

/// The operation id `perform` passes and `handle` dispatches on.
pub(super) fn op_hash(name: &str) -> i32 {
    let mut h: i32 = 0;
    for b in name.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as i32);
    }
    h.abs()
}

/// A `void*[n]` array of the addresses of `locals`, or null when empty.
fn address_array(builder: &mut MirBuilder, locals: &[LocalId]) -> MirValue {
    if locals.is_empty() {
        return MirValue::Const(MirConst::Null(MirType::Void));
    }
    let void_ptr = MirType::Ptr(Box::new(MirType::Void));
    let arr = builder.create_local(MirType::Array(
        Box::new(void_ptr.clone()),
        locals.len() as u64,
    ));
    for (i, id) in locals.iter().enumerate() {
        builder.push_index_store(
            MirValue::Local(arr),
            MirValue::Const(MirConst::Int(i as i128, MirType::i32())),
            void_ptr.clone(),
            MirRValue::AddressOf {
                is_mut: false,
                place: MirPlace::local(*id),
            },
        );
    }
    MirValue::Local(arr)
}

impl MirLowerer<'_> {
    /// Lower `handler` as a tail clause and register it on `handler_local`.
    /// `false` when the clause does not resume and keeps the abort path.
    pub(super) fn lower_tail_clause(
        &mut self,
        handler: &ast::EffectHandler,
        effect_name: &str,
        handler_local: LocalId,
    ) -> CodegenResult<bool> {
        let (tails, plain) = crate::types::resume_tail::tail_resumes(&handler.body);
        if tails.is_empty() || plain {
            return Ok(false);
        }
        let op_name = handler.operation.name.clone();
        let mut params: Vec<(Arc<str>, MirType)> = Vec::new();
        for (i, p) in handler.params.iter().enumerate() {
            if let ast::PatternKind::Ident { name, .. } = &p.pattern.kind {
                let ty =
                    p.ty.as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or_else(|| {
                            self.lookup_effect_param_type(effect_name, &op_name, i)
                                .unwrap_or(MirType::i32())
                        });
                params.push((name.name.clone(), ty));
            }
        }
        let names: HashSet<Arc<str>> = params.iter().map(|(n, _)| n.clone()).collect();
        let captures =
            Self::collect_free_vars(&handler.body, &names, &self.var_map, self.source.as_deref());
        let outer = self
            .current_fn
            .as_ref()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let cap_types: Vec<MirType> = captures
            .iter()
            .map(|(_, id)| outer.local_type(*id).unwrap_or(MirType::i32()))
            .collect();
        let ret = self
            .effect_op_rets
            .get(&(Arc::from(effect_name), op_name.clone()))
            .cloned()
            .unwrap_or(MirType::Void);
        let slot = if ret == MirType::Void {
            MirType::i32()
        } else {
            ret.clone()
        };
        let hty = MirType::Struct(Arc::from("BuildHandler"));
        let sig = MirFnSig::new(
            vec![hty.clone(), hty, MirType::Ptr(Box::new(slot))],
            MirType::Void,
        );
        let name: Arc<str> = Arc::from(format!("__effect_clause_{}", self.closure_count));
        self.closure_count += 1;

        let saved_fn = self.current_fn.take();
        let saved_vars = std::mem::take(&mut self.var_map);
        let mut b = MirBuilder::with_linear_type_names(
            name.clone(),
            sig,
            Arc::new(self.linear_type_names.clone()),
        );
        let args = b.param_local(0);
        let caps = b.param_local(1);
        let result = b.param_local(2);
        let sources = params.iter().map(|(n, t)| (args, n, t)).enumerate().chain(
            captures
                .iter()
                .zip(cap_types.iter())
                .map(|((n, _), t)| (caps, n, t))
                .enumerate(),
        );
        for (i, (base, n, ty)) in sources {
            let local = b.create_named_local(n.clone(), ty.clone());
            b.assign(
                local,
                MirRValue::FieldAccess {
                    base: MirValue::Local(base),
                    field_name: Arc::from(format!("handler_data#{}", i)),
                    field_ty: MirType::Ptr(Box::new(MirType::Void)),
                },
            );
            self.var_map.insert(n.clone(), local);
        }
        self.current_fn = Some(b);
        let saved_drops = std::mem::take(&mut self.drop_scopes);
        let value = self.lower_expr(&handler.body);
        self.drop_scopes = saved_drops;
        let mut b = self.current_fn.take().unwrap();
        self.current_fn = saved_fn;
        self.var_map = saved_vars;
        let value = value?;
        if ret != MirType::Void && !matches!(value, MirValue::Const(MirConst::Unit)) {
            b.push_deref_assign(result, MirRValue::Use(value));
        }
        b.ret_void();
        let mut func = b.build();
        func.linkage = Linkage::Internal;
        func.is_public = false;
        self.module.add_function(func);

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let cap_ids: Vec<LocalId> = captures.iter().map(|(_, id)| *id).collect();
        let env = address_array(builder, &cap_ids);
        let hptr = builder.create_local(MirType::Ptr(Box::new(MirType::Struct(Arc::from(
            "BuildHandler",
        )))));
        builder.assign(
            hptr,
            MirRValue::AddressOf {
                is_mut: true,
                place: MirPlace::local(handler_local),
            },
        );
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("build_handler_tail")),
            vec![
                MirValue::Local(hptr),
                MirValue::Const(MirConst::Int(op_hash(&op_name) as i128, MirType::i32())),
                MirValue::Function(name),
                env,
            ],
            None,
            cont,
        );
        builder.switch_to_block(cont);
        Ok(true)
    }
}
