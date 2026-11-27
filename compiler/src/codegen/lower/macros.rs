// ===============================================================================
// BUILDLANG CODE GENERATOR - MACRO AND CLOSURE LOWERING
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Closure lowering, effect lowering, builtin macro expansion, and iterator
//! chain desugaring for MIR.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ast::{self, ExprKind, StmtKind};

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::{values, MirBuilder};
use crate::codegen::ir::*;

use super::{ChainSource, IterChain, IterStep, IterTerminal, MirLowerer};

impl<'ctx> MirLowerer<'ctx> {
    // =========================================================================
    // CLOSURE LOWERING
    // =========================================================================

    /// Collect free variables in an expression that are defined in the
    /// enclosing scope (i.e., present in `env_vars`) but are NOT among
    /// the closure's own `param_names`.  Returns a de-duplicated list of
    /// (variable_name, local_id_in_enclosing_scope).
    pub(super) fn collect_free_vars(
        expr: &ast::Expr,
        param_names: &HashSet<Arc<str>>,
        env_vars: &HashMap<Arc<str>, LocalId>,
        source: Option<&str>,
    ) -> Vec<(Arc<str>, LocalId)> {
        let mut found: Vec<(Arc<str>, LocalId)> = Vec::new();
        let mut seen: HashSet<Arc<str>> = HashSet::new();
        Self::collect_free_vars_inner(expr, param_names, env_vars, &mut found, &mut seen, source);
        found
    }

    fn collect_free_vars_inner(
        expr: &ast::Expr,
        param_names: &HashSet<Arc<str>>,
        env_vars: &HashMap<Arc<str>, LocalId>,
        found: &mut Vec<(Arc<str>, LocalId)>,
        seen: &mut HashSet<Arc<str>>,
        source: Option<&str>,
    ) {
        match &expr.kind {
            ExprKind::Ident(ident) => {
                if !param_names.contains(&ident.name) && !seen.contains(&ident.name) {
                    if let Some(&local_id) = env_vars.get(&ident.name) {
                        seen.insert(ident.name.clone());
                        found.push((ident.name.clone(), local_id));
                    }
                }
            }
            ExprKind::Binary { left, right, .. } => {
                Self::collect_free_vars_inner(left, param_names, env_vars, found, seen, source);
                Self::collect_free_vars_inner(right, param_names, env_vars, found, seen, source);
            }
            ExprKind::Unary { expr: inner, .. }
            | ExprKind::Paren(inner)
            | ExprKind::Ref { expr: inner, .. }
            | ExprKind::Deref(inner)
            | ExprKind::Return(Some(inner))
            | ExprKind::Resume(Some(inner))
            | ExprKind::Try(inner)
            | ExprKind::Await(inner)
            | ExprKind::Cast { expr: inner, .. } => {
                Self::collect_free_vars_inner(inner, param_names, env_vars, found, seen, source);
            }
            ExprKind::Call { func, args } => {
                Self::collect_free_vars_inner(func, param_names, env_vars, found, seen, source);
                for a in args {
                    Self::collect_free_vars_inner(a, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                Self::collect_free_vars_inner(receiver, param_names, env_vars, found, seen, source);
                for a in args {
                    Self::collect_free_vars_inner(a, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::collect_free_vars_inner(
                    condition,
                    param_names,
                    env_vars,
                    found,
                    seen,
                    source,
                );
                for stmt in &then_branch.stmts {
                    match &stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) => {
                            Self::collect_free_vars_inner(
                                e,
                                param_names,
                                env_vars,
                                found,
                                seen,
                                source,
                            );
                        }
                        StmtKind::Local(local) => {
                            if let Some(ref init) = local.init {
                                Self::collect_free_vars_inner(
                                    &init.expr,
                                    param_names,
                                    env_vars,
                                    found,
                                    seen,
                                    source,
                                );
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(e) = else_branch {
                    Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::Block(block) | ExprKind::Unsafe(block) => {
                for stmt in &block.stmts {
                    match &stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) => {
                            Self::collect_free_vars_inner(
                                e,
                                param_names,
                                env_vars,
                                found,
                                seen,
                                source,
                            );
                        }
                        StmtKind::Local(local) => {
                            if let Some(ref init) = local.init {
                                Self::collect_free_vars_inner(
                                    &init.expr,
                                    param_names,
                                    env_vars,
                                    found,
                                    seen,
                                    source,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            ExprKind::Closure {
                params: inner_params,
                body: inner_body,
                ..
            } => {
                let mut inner_names = param_names.clone();
                for p in inner_params {
                    if let ast::PatternKind::Ident { name, .. } = &p.pattern.kind {
                        inner_names.insert(name.name.clone());
                    }
                }
                Self::collect_free_vars_inner(
                    inner_body,
                    &inner_names,
                    env_vars,
                    found,
                    seen,
                    source,
                );
            }
            ExprKind::Struct { fields, rest, .. } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        Self::collect_free_vars_inner(
                            v,
                            param_names,
                            env_vars,
                            found,
                            seen,
                            source,
                        );
                    }
                }
                if let Some(r) = rest {
                    Self::collect_free_vars_inner(r, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::IfLet {
                expr: scrut,
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_free_vars_inner(scrut, param_names, env_vars, found, seen, source);
                for stmt in &then_branch.stmts {
                    match &stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) => {
                            Self::collect_free_vars_inner(
                                e,
                                param_names,
                                env_vars,
                                found,
                                seen,
                                source,
                            );
                        }
                        StmtKind::Local(local) => {
                            if let Some(ref init) = local.init {
                                Self::collect_free_vars_inner(
                                    &init.expr,
                                    param_names,
                                    env_vars,
                                    found,
                                    seen,
                                    source,
                                );
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(e) = else_branch {
                    Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::For { iter, body, .. } => {
                Self::collect_free_vars_inner(iter, param_names, env_vars, found, seen, source);
                for stmt in &body.stmts {
                    match &stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) => {
                            Self::collect_free_vars_inner(
                                e,
                                param_names,
                                env_vars,
                                found,
                                seen,
                                source,
                            );
                        }
                        StmtKind::Local(local) => {
                            if let Some(ref init) = local.init {
                                Self::collect_free_vars_inner(
                                    &init.expr,
                                    param_names,
                                    env_vars,
                                    found,
                                    seen,
                                    source,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            ExprKind::While {
                condition, body, ..
            } => {
                Self::collect_free_vars_inner(
                    condition,
                    param_names,
                    env_vars,
                    found,
                    seen,
                    source,
                );
                for stmt in &body.stmts {
                    match &stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) => {
                            Self::collect_free_vars_inner(
                                e,
                                param_names,
                                env_vars,
                                found,
                                seen,
                                source,
                            );
                        }
                        StmtKind::Local(local) => {
                            if let Some(ref init) = local.init {
                                Self::collect_free_vars_inner(
                                    &init.expr,
                                    param_names,
                                    env_vars,
                                    found,
                                    seen,
                                    source,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            ExprKind::Tuple(elems) | ExprKind::Array(elems) => {
                for e in elems {
                    Self::collect_free_vars_inner(e, param_names, env_vars, found, seen, source);
                }
            }
            ExprKind::Index { expr: arr, index } => {
                Self::collect_free_vars_inner(arr, param_names, env_vars, found, seen, source);
                Self::collect_free_vars_inner(index, param_names, env_vars, found, seen, source);
            }
            ExprKind::Field { expr: obj, .. } => {
                Self::collect_free_vars_inner(obj, param_names, env_vars, found, seen, source);
            }
            ExprKind::Assign { target, value, .. } => {
                Self::collect_free_vars_inner(target, param_names, env_vars, found, seen, source);
                Self::collect_free_vars_inner(value, param_names, env_vars, found, seen, source);
            }
            ExprKind::Match { scrutinee, arms } => {
                Self::collect_free_vars_inner(
                    scrutinee,
                    param_names,
                    env_vars,
                    found,
                    seen,
                    source,
                );
                for arm in arms {
                    Self::collect_free_vars_inner(
                        &arm.body,
                        param_names,
                        env_vars,
                        found,
                        seen,
                        source,
                    );
                    if let Some(guard) = &arm.guard {
                        Self::collect_free_vars_inner(
                            guard,
                            param_names,
                            env_vars,
                            found,
                            seen,
                            source,
                        );
                    }
                }
            }
            ExprKind::Macro { tokens, .. } => {
                // Scan token trees for identifiers
                for tt in tokens {
                    Self::collect_free_vars_in_token_tree(
                        tt,
                        param_names,
                        env_vars,
                        found,
                        seen,
                        source,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_free_vars_in_token_tree(
        tt: &ast::TokenTree,
        param_names: &HashSet<Arc<str>>,
        env_vars: &HashMap<Arc<str>, LocalId>,
        found: &mut Vec<(Arc<str>, LocalId)>,
        seen: &mut HashSet<Arc<str>>,
        source: Option<&str>,
    ) {
        match tt {
            ast::TokenTree::Token(token) => {
                // Check if this token is an identifier that references an
                // enclosing variable.  We need the source text to recover the
                // identifier string because Token stores only a span.
                if let crate::lexer::TokenKind::Ident = &token.kind {
                    if let Some(src) = source {
                        let start = token.span.start.to_usize();
                        let end = token.span.end.to_usize();
                        if end <= src.len() {
                            let name: Arc<str> = Arc::from(&src[start..end]);
                            if !param_names.contains(&name) && !seen.contains(&name) {
                                if let Some(&local_id) = env_vars.get(&name) {
                                    seen.insert(name.clone());
                                    found.push((name, local_id));
                                }
                            }
                        }
                    }
                }
            }
            ast::TokenTree::Delimited { tokens, .. } => {
                for inner in tokens {
                    Self::collect_free_vars_in_token_tree(
                        inner,
                        param_names,
                        env_vars,
                        found,
                        seen,
                        source,
                    );
                }
            }
        }
    }

    /// Lower a closure expression into a static function + function pointer.
    ///
    /// **Capturing closures (lambda lifting)**: if the closure body references
    /// variables from the enclosing scope, those variables are added as extra
    /// trailing parameters to the generated `__closure_N` function.  At the
    /// call site the captured values are automatically appended.
    ///
    /// NOTE: this approach works for closures that are called locally or passed
    /// to functions that invoke them in the same compilation unit.  Returning a
    /// capturing closure from a function is not yet supported because the
    /// function-pointer signature would differ from the declared type.
    pub(crate) fn lower_closure(
        &mut self,
        params: &[ast::ClosureParam],
        return_type: Option<&ast::Type>,
        body: &ast::Expr,
    ) -> CodegenResult<MirValue> {
        let closure_id = self.closure_count;
        self.closure_count += 1;
        let closure_name: Arc<str> = Arc::from(format!("__closure_{}", closure_id));

        // ---- Detect captured variables (lambda lifting) ----
        let param_names: HashSet<Arc<str>> = params
            .iter()
            .filter_map(|p| {
                if let ast::PatternKind::Ident { name, .. } = &p.pattern.kind {
                    Some(name.name.clone())
                } else {
                    None
                }
            })
            .collect();

        let captures =
            Self::collect_free_vars(body, &param_names, &self.var_map, self.source.as_deref());

        // ---- Build the MIR signature ----
        // Declared params first, then captured-variable params appended. A
        // parameter written without a type takes the caller's hint
        // (`opt.map(|x| ..)` gives `x` the payload type), else i32.
        let hint = self.closure_param_hint.take().unwrap_or_default();
        let mut mir_params: Vec<MirType> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                p.ty.as_ref()
                    .map(|t| self.lower_type_from_ast(t))
                    .or_else(|| hint.get(i).cloned())
                    .unwrap_or(MirType::i32())
            })
            .collect();

        // Resolve captured variable types from the enclosing builder.
        let capture_types: Vec<MirType> = captures
            .iter()
            .map(|(_name, local_id)| {
                if let Some(ref builder) = self.current_fn {
                    builder.local_type(*local_id).unwrap_or(MirType::i32())
                } else {
                    MirType::i32()
                }
            })
            .collect();

        mir_params.extend(capture_types.iter().cloned());

        let mir_ret = return_type
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::i32());

        let sig = MirFnSig::new(mir_params.clone(), mir_ret.clone());
        // The fn-ptr type must use the *full* parameter list (visible +
        // captured) so that the C declaration matches the call sites, which
        // append captured values as extra arguments.
        let mut fn_ptr_ty =
            MirType::FnPtr(Box::new(MirFnSig::new(mir_params.clone(), mir_ret.clone())));

        // Save current function state
        let saved_fn = self.current_fn.take();
        let saved_vars = std::mem::take(&mut self.var_map);

        let mut closure_builder = MirBuilder::with_linear_type_names(
            closure_name.clone(),
            sig,
            Arc::new(self.linear_type_names.clone()),
        );

        // Map declared params
        for (i, param) in params.iter().enumerate() {
            if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                let local_id = closure_builder.param_local(i);
                closure_builder.set_param_name(i, name.name.clone());
                self.var_map.insert(name.name.clone(), local_id);
            }
        }

        // Map captured-variable params (appended after declared params)
        for (ci, (cap_name, _)) in captures.iter().enumerate() {
            let param_idx = params.len() + ci;
            let local_id = closure_builder.param_local(param_idx);
            closure_builder.set_param_name(param_idx, cap_name.clone());
            self.var_map.insert(cap_name.clone(), local_id);
        }

        self.current_fn = Some(closure_builder);

        // The closure's drop scopes are its own; a `return` in it must not
        // drop the enclosing function's values.
        let saved_drops = std::mem::take(&mut self.drop_scopes);
        let body_val = self.lower_expr(body);
        self.drop_scopes = saved_drops;
        let body_val = body_val?;

        // A closure written without a return type returns its body's type
        // (`|x| x > 2` gives a bool, `|x| Some(x)` an Option). Before, it
        // always returned i32.
        let inferred_ret = if return_type.is_none() {
            self.closure_body_type(&body_val)
        } else {
            None
        };

        let mut closure_builder = self.current_fn.take().unwrap();
        if let Some(ret) = inferred_ret {
            if let Some(ret_local) = closure_builder.return_local() {
                closure_builder.retype_local(ret_local, ret.clone());
            }
            closure_builder.set_return_type(ret.clone());
            fn_ptr_ty = MirType::FnPtr(Box::new(MirFnSig::new(mir_params.clone(), ret)));
        }
        if mir_ret != MirType::Void {
            closure_builder.ret(Some(body_val));
        } else {
            closure_builder.ret_void();
        }

        let mut closure_func = closure_builder.build();
        closure_func.linkage = Linkage::Internal;
        closure_func.is_public = false;

        self.module.add_function(closure_func);

        // Restore the enclosing function state
        self.current_fn = saved_fn;
        self.var_map = saved_vars;

        // Register captures so that lower_call can append extra args.
        if !captures.is_empty() {
            self.closure_captures.insert(closure_name.clone(), captures);
        }

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for closure".to_string()))?;
        let result = builder.create_local(fn_ptr_ty);
        builder.assign(
            result,
            MirRValue::Use(MirValue::Function(closure_name.clone())),
        );

        // Track that this local holds the given closure so we can find
        // its captures later when calling through this local.
        self.local_closure_name.insert(result, closure_name);

        Ok(values::local(result))
    }

    pub(crate) fn lower_struct_expr(
        &mut self,
        path: &ast::Path,
        fields: &[ast::FieldExpr],
        rest: Option<&ast::Expr>,
    ) -> CodegenResult<MirValue> {
        // Handle enum struct variant literals: `EnumName::Variant { field: val, .. }`.
        if path.segments.len() >= 2 && rest.is_none() {
            let mut enum_name = path.segments[path.segments.len() - 2].ident.name.clone();
            if enum_name.as_ref() == "Self" {
                if let Some(ref impl_ty) = self.current_impl_type {
                    enum_name = impl_ty.clone();
                }
            }
            if !self.module_prefix.is_empty() {
                let prefixed = self.prefixed_name(&enum_name);
                if self.module.find_type(prefixed.as_ref()).is_some() {
                    enum_name = prefixed;
                }
            }
            let variant_name = path
                .last_ident()
                .map(|i| i.name.clone())
                .unwrap_or(Arc::from(""));
            if let Some((disc, variant_fields)) =
                self.lookup_enum_variant(&enum_name, &variant_name)
            {
                let mut field_vals: Vec<MirValue> = Vec::with_capacity(fields.len());
                for f in fields {
                    let expected = variant_fields
                        .iter()
                        .find(|(n, _)| n.as_deref() == Some(f.name.name.as_ref()))
                        .map(|(_, t)| t.clone());
                    let prev_expected = match expected {
                        Some(t) => self.expected_type.replace(t),
                        None => self.expected_type.take(),
                    };
                    let val = if let Some(v) = &f.value {
                        self.lower_expr(v)
                    } else {
                        self.lower_ident(&f.name)
                    };
                    self.expected_type = prev_expected;
                    field_vals.push(val?);
                }
                let mut ordered = Vec::with_capacity(variant_fields.len());
                for (vfname, _) in &variant_fields {
                    if let Some(pos) = fields
                        .iter()
                        .position(|f| Some(f.name.name.as_ref()) == vfname.as_deref())
                    {
                        ordered.push(field_vals[pos].clone());
                    }
                }
                let operands = if ordered.len() == variant_fields.len() {
                    ordered
                } else {
                    field_vals
                };
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let result = builder.create_local(MirType::Struct(enum_name.clone()));
                builder.aggregate(
                    result,
                    AggregateKind::Variant(enum_name, disc as u32, variant_name),
                    operands,
                );
                return Ok(values::local(result));
            }
        }

        let mut raw_name = path
            .last_ident()
            .map(|i| i.name.clone())
            .unwrap_or(Arc::from(""));

        // Resolve Self to concrete type name
        if raw_name.as_ref() == "Self" {
            if let Some(ref impl_ty) = self.current_impl_type {
                raw_name = impl_ty.clone();
            }
        }

        // Inside inline modules, try the prefixed struct name first so that
        // types defined in the current module are resolved correctly.
        if !self.module_prefix.is_empty() {
            let prefixed = self.prefixed_name(&raw_name);
            if self.module.find_type(prefixed.as_ref()).is_some() {
                raw_name = prefixed;
            }
        }

        // A generic struct written without arguments takes them from the
        // type it is expected to be (`-> Buffer<T>` in a method of
        // `Buffer<i32>`, or a `let` annotation), so its fields are lowered
        // against the instance's types (#305).
        let expected_instance = if self.generic_structs.contains_key(raw_name.as_ref())
            && path.last_generics().map_or(true, |g| g.is_empty())
        {
            self.expected_instance_of(raw_name.as_ref())?
        } else {
            None
        };
        if let Some(instance) = &expected_instance {
            raw_name = instance.clone();
        }

        // Virtual `Mat3 { m: [[f32; 3]; 3] }` / `Mat4 { m: [[f32; 4]; 4] }` literal form:
        // convert the 2D array into the canonical `x_axis..w_axis` struct layout.
        if matches!(raw_name.as_ref(), "Mat3" | "Mat4")
            && fields.len() == 1
            && fields[0].name.name.as_ref() == "m"
            && rest.is_none()
            && self
                .struct_field_types(raw_name.as_ref())
                .is_some_and(|d| d.iter().any(|(n, _)| n.as_deref() == Some("x_axis")))
        {
            let is_m4 = raw_name.as_ref() == "Mat4";
            let n = if is_m4 { 4usize } else { 3usize };
            let vec_name = if is_m4 { "Vec4" } else { "Vec3" };
            let row_ty = MirType::Array(Box::new(MirType::f32()), n as u64);
            let mat_ty = MirType::Array(Box::new(row_ty.clone()), n as u64);
            let prev_expected = self.expected_type.replace(mat_ty);
            let m_res = if let Some(val) = &fields[0].value {
                self.lower_expr(val)
            } else {
                self.lower_ident(&fields[0].name)
            };
            self.expected_type = prev_expected;
            let m_val = m_res?;
            let (actual_row_ty, actual_elem_ty) = match self.type_of_value(&m_val) {
                MirType::Array(inner, _) => match inner.as_ref() {
                    MirType::Array(el, _) => (inner.as_ref().clone(), el.as_ref().clone()),
                    _ => (row_ty.clone(), MirType::f32()),
                },
                _ => (row_ty.clone(), MirType::f32()),
            };
            let b = self.current_fn.as_mut().unwrap();
            let mut cols = Vec::with_capacity(n);
            for r in 0..n {
                let row = b.create_local(actual_row_ty.clone());
                b.assign(
                    row,
                    MirRValue::IndexAccess {
                        base: m_val.clone(),
                        index: MirValue::Const(MirConst::Uint(r as u128, MirType::usize())),
                        elem_ty: actual_row_ty.clone(),
                    },
                );
                let mut comps = Vec::with_capacity(n);
                for c in 0..n {
                    let elem = b.create_local(actual_elem_ty.clone());
                    b.assign(
                        elem,
                        MirRValue::IndexAccess {
                            base: values::local(row),
                            index: MirValue::Const(MirConst::Uint(c as u128, MirType::usize())),
                            elem_ty: actual_elem_ty.clone(),
                        },
                    );
                    let elem_f32 = if actual_elem_ty == MirType::f32() {
                        values::local(elem)
                    } else {
                        let casted = b.create_local(MirType::f32());
                        b.cast(
                            casted,
                            CastKind::FloatToFloat,
                            values::local(elem),
                            MirType::f32(),
                        );
                        values::local(casted)
                    };
                    comps.push(elem_f32);
                }
                let col = b.create_local(MirType::Struct(Arc::from(vec_name)));
                b.assign(
                    col,
                    MirRValue::Aggregate {
                        kind: AggregateKind::Struct(Arc::from(vec_name)),
                        operands: comps,
                    },
                );
                cols.push(values::local(col));
            }
            let out = b.create_local(MirType::Struct(raw_name.clone()));
            b.assign(
                out,
                MirRValue::Aggregate {
                    kind: AggregateKind::Struct(raw_name),
                    operands: cols,
                },
            );
            return Ok(values::local(out));
        }

        // Lower the field values in the order written (their side effects run
        // in that order), each against its declared type, so `tags: Vec::new()`
        // builds the field's vector type and not a `Vec<i32>` (M4 pilot).
        let declared = self.struct_field_types(raw_name.as_ref());
        let mut field_vals: Vec<MirValue> = Vec::with_capacity(fields.len());
        for f in fields {
            let expected = declared.as_ref().and_then(|d| {
                d.iter()
                    .find(|(n, _)| n.as_deref() == Some(f.name.name.as_ref()))
                    .map(|(_, t)| t.clone())
            });
            let prev_expected = match expected {
                Some(t) => self.expected_type.replace(t),
                None => self.expected_type.take(),
            };
            let val = if let Some(val) = &f.value {
                self.lower_expr(val)
            } else {
                // Field shorthand: `name` means `name: name`
                self.lower_ident(&f.name)
            };
            self.expected_type = prev_expected;
            field_vals.push(val?);
        }

        // Check if this is a generic struct that needs monomorphization
        let struct_name = if self.generic_structs.contains_key(raw_name.as_ref()) {
            // Try to resolve from explicit generic args on the path
            let generic_args = path.last_generics().unwrap_or(&[]);
            if !generic_args.is_empty() {
                let empty_subst = HashMap::new();
                let subst = self.resolve_generic_args_with_subst(
                    raw_name.as_ref(),
                    generic_args,
                    &empty_subst,
                );
                self.monomorphize_struct(raw_name.as_ref(), &subst)?
            } else {
                // Infer generic params from field values
                let subst =
                    self.infer_struct_generics_from_fields(raw_name.as_ref(), &field_vals, fields);
                self.monomorphize_struct(raw_name.as_ref(), &subst)?
            }
        } else {
            raw_name
        };

        // With `..base` the values are matched to fields by name further down;
        // keep them in the order written for that.
        let written_vals = rest.map(|_| field_vals.clone());

        // The aggregate is positional: put the values in declaration order.
        // Written out of order, `Q { c: "hi", a: 3, b: 2.5 }` assigned `a`
        // the string, and with two fields of one type it swapped them silently.
        let field_vals = match self.struct_field_types(struct_name.as_ref()) {
            Some(decl) if decl.len() == fields.len() => {
                let mut ordered = Vec::with_capacity(decl.len());
                for (name, _) in &decl {
                    let pos = fields
                        .iter()
                        .position(|f| Some(f.name.name.as_ref()) == name.as_deref());
                    match pos {
                        Some(i) => ordered.push(field_vals[i].clone()),
                        None => {
                            ordered.clear();
                            break;
                        }
                    }
                }
                if ordered.len() == decl.len() {
                    ordered
                } else {
                    field_vals
                }
            }
            _ => field_vals,
        };
        // `S { a: 1, ..base }`: the fields not written come from `base`.
        let field_vals = match (rest, written_vals) {
            (Some(rest), Some(written)) => {
                self.struct_update_operands(&struct_name, fields, written, rest)?
            }
            _ => field_vals,
        };

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let result = builder.create_local(MirType::Struct(struct_name.clone()));
        builder.aggregate(result, AggregateKind::Struct(struct_name), field_vals);

        Ok(values::local(result))
    }

    /// The operands of `S { written.., ..rest }` in declaration order: a
    /// written field keeps its value and every other field is read from
    /// `rest`. Before this the `..rest` part was ignored, the aggregate held
    /// only the written fields, and the others read as zero. `written` is in
    /// the order the fields were written, which is the order they were
    /// evaluated in; `rest` is evaluated after them.
    fn struct_update_operands(
        &mut self,
        struct_name: &Arc<str>,
        fields: &[ast::FieldExpr],
        written: Vec<MirValue>,
        rest: &ast::Expr,
    ) -> CodegenResult<Vec<MirValue>> {
        let unsupported = || CodegenError::Rejected {
            message: format!("`..` in a `{struct_name}` literal needs a struct with named fields"),
            location: (rest.span.end.0 > rest.span.start.0)
                .then_some((rest.span.start.0, rest.span.end.0)),
            help: Some("write every field of the literal".to_string()),
        };
        let decl = self
            .struct_field_types(struct_name.as_ref())
            .ok_or_else(unsupported)?;
        // Handle `..Default::default()` / `..Self::default()` using the target
        // struct's `default` method (or zero-initialization if derived/implicit).
        let is_default_call = matches!(
            &rest.kind,
            ExprKind::Call { func, args } if args.is_empty() && matches!(
                &func.kind,
                ExprKind::Path(p) if p.segments.len() >= 2
                    && matches!(p.segments[p.segments.len() - 2].ident.name.as_ref(), "Default" | "Self")
                    && p.last_ident().is_some_and(|i| i.name.as_ref() == "default")
            )
        );
        let base = if is_default_call {
            if let Some(def_fn) = self
                .impl_methods
                .get(&(struct_name.clone(), Arc::from("default")))
                .cloned()
            {
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let res = builder.create_local(MirType::Struct(struct_name.clone()));
                let cont = builder.create_block();
                builder.call(MirValue::Function(def_fn), vec![], Some(res), cont);
                builder.switch_to_block(cont);
                values::local(res)
            } else {
                let struct_ty = MirType::Struct(struct_name.clone());
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let res = builder.create_local(struct_ty.clone());
                builder.assign(
                    res,
                    MirRValue::Use(MirValue::Const(MirConst::Zeroed(struct_ty))),
                );
                values::local(res)
            }
        } else {
            self.lower_expr(rest)?
        };
        let mut operands = Vec::with_capacity(decl.len());
        for (name, ty) in decl {
            let name = name.ok_or_else(unsupported)?;
            let given = fields
                .iter()
                .position(|f| f.name.name.as_ref() == name.as_ref());
            if let Some(value) = given.and_then(|i| written.get(i)) {
                operands.push(value.clone());
                continue;
            }
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let field = builder.create_local(ty.clone());
            builder.assign(
                field,
                MirRValue::FieldAccess {
                    base: base.clone(),
                    field_name: name,
                    field_ty: ty,
                },
            );
            operands.push(values::local(field));
        }
        Ok(operands)
    }

    /// The named fields of a struct type in declaration order, if it is one.
    fn struct_field_types(&self, name: &str) -> Option<Vec<(Option<Arc<str>>, MirType)>> {
        match &self.module.find_type(name)?.kind {
            TypeDefKind::Struct { fields, .. } => Some(fields.clone()),
            _ => None,
        }
    }

    /// Infer generic type parameters for a struct from the types of field values
    /// at the construction site.
    fn infer_struct_generics_from_fields(
        &self,
        struct_name: &str,
        field_vals: &[MirValue],
        field_exprs: &[ast::FieldExpr],
    ) -> HashMap<Arc<str>, MirType> {
        let mut subst = HashMap::new();

        let struct_def = match self.generic_structs.get(struct_name) {
            Some(s) => s.clone(),
            None => return subst,
        };

        // Get type parameter names
        let type_param_names: Vec<Arc<str>> = struct_def
            .generics
            .params
            .iter()
            .filter_map(|p| match &p.kind {
                ast::GenericParamKind::Type { .. } => Some(p.ident.name.clone()),
                _ => None,
            })
            .collect();

        // Match field types against field values
        if let ast::StructFields::Named(def_fields) = &struct_def.fields {
            for (i, def_field) in def_fields.iter().enumerate() {
                if let Some(val) = field_vals.get(i) {
                    let val_ty = self.type_of_value(val);
                    // Check if this field's type is a generic parameter
                    if let ast::TypeKind::Path(path) = &def_field.ty.kind {
                        if path.is_simple() {
                            if let Some(ident) = path.last_ident() {
                                for tp_name in &type_param_names {
                                    if ident.name.as_ref() == tp_name.as_ref() {
                                        subst.entry(tp_name.clone()).or_insert(val_ty.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also try matching by field name from field_exprs
        if let ast::StructFields::Named(def_fields) = &struct_def.fields {
            for field_expr in field_exprs {
                let field_name = &field_expr.name.name;
                if let Some(def_field) = def_fields.iter().find(|f| &f.name.name == field_name) {
                    if let ast::TypeKind::Path(path) = &def_field.ty.kind {
                        if path.is_simple() {
                            if let Some(ident) = path.last_ident() {
                                for tp_name in &type_param_names {
                                    if ident.name.as_ref() == tp_name.as_ref() {
                                        // Already handled above via positional matching
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fill in any unbound params with i32 default
        for tp_name in &type_param_names {
            subst.entry(tp_name.clone()).or_insert(MirType::i32());
        }

        subst
    }

    // =========================================================================
    // EFFECT LOWERING
    // =========================================================================

    /// Compute a deterministic integer ID for a named effect.
    ///
    /// Built-in effects get fixed IDs; user-defined effects are hashed into a
    /// range starting at 100 so they never collide with the built-in set.
    fn effect_id(effect_name: &str) -> i32 {
        match effect_name {
            "IO" => 1,
            "Error" => 2,
            "Async" => 3,
            "State" => 4,
            "NonDet" => 5,
            _ => {
                // Hash the name for user-defined effects
                let mut hash: i32 = 0;
                for b in effect_name.bytes() {
                    hash = hash.wrapping_mul(31).wrapping_add(b as i32);
                }
                hash.abs() + 100 // offset to avoid collision with built-ins
            }
        }
    }

    /// Look up the type of a parameter in an effect operation definition.
    ///
    /// `effect_name` - the resolved effect name (e.g. "Console")
    /// `op_name`     - the operation name (e.g. "log")
    /// `param_idx`   - zero-based parameter index
    ///
    /// Returns `None` if the effect or operation is not found, or if the index
    /// is out of range.
    pub(super) fn lookup_effect_param_type(
        &self,
        effect_name: &str,
        op_name: &str,
        param_idx: usize,
    ) -> Option<MirType> {
        let ops = self.effect_defs.get(effect_name)?;
        for (name, param_types) in ops {
            if name.as_ref() == op_name {
                return param_types.get(param_idx).cloned();
            }
        }
        None
    }

    /// Lower an `ExprKind::Handle` expression.
    ///
    /// ```text
    /// handle { body } with { Effect.op(params) => handler_body, ... }
    /// ```
    ///
    /// Generates MIR that:
    /// 1. Allocates a `BuildHandler` on the stack (as a struct local).
    /// 2. Calls `build_push_handler(&handler, effect_id)`.
    /// 3. Calls `setjmp(handler.env)`:
    ///    - If the result is 0  -> execute the body normally, then pop the handler.
    ///    - If the result is N  -> dispatch to handler clause N-1.
    /// 4. Pops the handler on every exit path.
    pub(crate) fn lower_handle(
        &mut self,
        effect: &ast::Path,
        handlers: &[ast::EffectHandler],
        body: &ast::Block,
    ) -> CodegenResult<MirValue> {
        // Resolve the effect name and its integer ID.
        let effect_name = effect
            .segments
            .iter()
            .map(|s| s.ident.name.as_ref())
            .collect::<Vec<_>>()
            .join("::");
        let eid = Self::effect_id(&effect_name);

        // --- Allocate locals ---------------------------------------------------
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // The handler struct is opaque at the MIR level; the C backend will emit
        // a `BuildHandler` declaration for it.  We represent it as an i8 array
        // large enough to hold the C struct (the runtime defines the real type).
        let handler_local = builder.create_named_local(
            format!("__handler_{}", effect_name),
            MirType::Struct(Arc::from("BuildHandler")),
        );

        // The local that receives the setjmp return value (0 = normal, N = op N-1).
        let setjmp_result = builder.create_local(MirType::i32());

        // The final result of the handle expression.
        let handle_result = builder.create_local(MirType::i32());

        // --- Create blocks ------------------------------------------------------
        let push_block = builder.create_labeled_block(format!("effect_push_{}", setjmp_result.0));
        let body_block = builder.create_labeled_block(format!("effect_body_{}", setjmp_result.0));
        let merge_block = builder.create_labeled_block(format!("effect_merge_{}", setjmp_result.0));

        // Create a block for each handler clause.
        let handler_blocks: Vec<BlockId> = handlers
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let builder = self.current_fn.as_mut().unwrap();
                builder.create_labeled_block(format!("effect_handler_{}_{}", setjmp_result.0, i))
            })
            .collect();

        let mut is_tail: Vec<bool> = Vec::with_capacity(handlers.len());
        // --- Emit: push handler -------------------------------------------------
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(push_block);
            builder.switch_to_block(push_block);

            // build_push_handler(&handler, effect_id)
            let push_fn = MirValue::Function(Arc::from("build_push_handler"));
            // Take address of the handler struct so the C call gets a pointer.
            let handler_ptr_local = builder.create_local(MirType::Ptr(Box::new(MirType::Struct(
                Arc::from("BuildHandler"),
            ))));
            builder.assign(
                handler_ptr_local,
                MirRValue::AddressOf {
                    is_mut: true,
                    place: MirPlace::local(handler_local),
                },
            );
            let eid_val = MirValue::Const(MirConst::Int(eid as i128, MirType::i32()));
            let cont = builder.create_block();
            builder.call(
                push_fn,
                vec![MirValue::Local(handler_ptr_local), eid_val],
                None,
                cont,
            );
            builder.switch_to_block(cont);
            for h in handlers {
                is_tail.push(self.lower_tail_clause(h, &effect_name, handler_local)?);
            }
            let builder = self.current_fn.as_mut().unwrap();

            // setjmp(handler.env) - pass the handler local directly;
            // the C backend will emit `.env` when it sees a setjmp call
            // with a BuildHandler-typed argument.
            let setjmp_fn = MirValue::Function(Arc::from("setjmp"));
            let cont2 = builder.create_block();
            builder.call(
                setjmp_fn,
                vec![MirValue::Local(handler_local)],
                Some(setjmp_result),
                cont2,
            );
            builder.switch_to_block(cont2);

            // Branch: if setjmp_result == 0 -> body, else dispatch
            let zero = MirValue::Const(MirConst::Int(0, MirType::i32()));
            let is_normal = builder.create_local(MirType::Bool);
            builder.binary_op(is_normal, BinOp::Eq, MirValue::Local(setjmp_result), zero);

            // Build switch targets for handler dispatch.
            // setjmp returns op_id + 1, so handler clause i fires when result == i+1.
            let default_block = if handler_blocks.is_empty() {
                merge_block
            } else {
                handler_blocks[0]
            };

            if handler_blocks.len() <= 1 {
                // Simple: either body or first (only) handler
                builder.branch(MirValue::Local(is_normal), body_block, default_block);
            } else {
                // Multi-handler: first check if normal, then switch on op_id.
                let dispatch_block = builder.create_labeled_block("effect_dispatch");
                builder.branch(MirValue::Local(is_normal), body_block, dispatch_block);

                builder.switch_to_block(dispatch_block);
                let targets: Vec<(MirConst, BlockId)> = handler_blocks
                    .iter()
                    .enumerate()
                    .map(|(i, &blk)| {
                        (
                            MirConst::Int(
                                super::tail_resume::op_hash(&handlers[i].operation.name) as i128
                                    + 1,
                                MirType::i32(),
                            ),
                            blk,
                        )
                    })
                    .collect();
                builder.switch(MirValue::Local(setjmp_result), targets, merge_block);
            }
        }

        // --- Emit: body (normal path, setjmp returned 0) ------------------------
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(body_block);
        }
        let body_val = self.lower_block(body)?;
        {
            let builder = self.current_fn.as_mut().unwrap();
            if let Some(ref v) = body_val {
                // Don't assign void/unit values to the handle result.
                if !matches!(v, MirValue::Const(MirConst::Unit)) {
                    builder.assign(handle_result, MirRValue::Use(v.clone()));
                }
            }
            // Pop the handler after the body completes normally.
            let pop_fn = MirValue::Function(Arc::from("build_pop_handler"));
            let cont = builder.create_block();
            builder.call(pop_fn, vec![], None, cont);
            builder.switch_to_block(cont);
            builder.goto(merge_block);
        }

        // --- Emit: handler clauses ----------------------------------------------
        for (i, handler) in handlers.iter().enumerate() {
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(handler_blocks[i]);
            }
            if is_tail[i] {
                let builder = self.current_fn.as_mut().unwrap();
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("build_pop_handler")),
                    vec![],
                    None,
                    cont,
                );
                builder.switch_to_block(cont);
                builder.goto(merge_block);
                continue;
            }

            // Map handler parameters to locals so the handler body can reference
            // them.  For the setjmp model the parameter data is available via
            // `handler.handler_data`; at the MIR level we just create named
            // locals that the C backend will initialise from `handler_data`.
            for (param_idx, param) in handler.params.iter().enumerate() {
                if let ast::PatternKind::Ident { name, .. } = &param.pattern.kind {
                    // Determine parameter type: use explicit annotation if present,
                    // otherwise look up the effect operation's parameter types
                    // from the collected effect definitions.
                    let op_name = handler.operation.name.as_ref();
                    let ty = param
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or_else(|| {
                            self.lookup_effect_param_type(&effect_name, op_name, param_idx)
                                .unwrap_or(MirType::i32())
                        });
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_named_local(name.name.clone(), ty.clone());
                    self.var_map.insert(name.name.clone(), local);

                    // Load argument `param_idx` from handler_data.
                    // The perform site stores a `void*[N]` argv array in
                    // handler_data, where argv[i] points to argument i. The
                    // `handler_data#<i>` field marker tells the C backend which
                    // argument to read; it renders as:
                    //   msg = *(ParamType*)((void**)__handler_EffectName.handler_data)[i]
                    let handler_data_field = MirRValue::FieldAccess {
                        base: MirValue::Local(handler_local),
                        field_name: Arc::from(format!("handler_data#{}", param_idx)),
                        field_ty: MirType::Ptr(Box::new(MirType::Void)),
                    };
                    builder.assign(local, handler_data_field);
                }
            }

            // Lower the handler body expression.
            let handler_val = self.lower_expr(&handler.body)?;
            {
                let builder = self.current_fn.as_mut().unwrap();
                // Don't assign void/unit values to the handle result.
                // Also skip if the handler_val is a local that isn't declared in
                // the current function (can happen with resume return values).
                let should_assign = !matches!(handler_val, MirValue::Const(MirConst::Unit))
                    && !matches!(handler_val, MirValue::Const(MirConst::Bool(_)))
                    && match &handler_val {
                        MirValue::Local(lid) => builder.local_exists(*lid),
                        _ => true,
                    };
                if should_assign {
                    builder.assign(handle_result, MirRValue::Use(handler_val));
                }
                // Pop handler after handling.
                let pop_fn = MirValue::Function(Arc::from("build_pop_handler"));
                let cont = builder.create_block();
                builder.call(pop_fn, vec![], None, cont);
                builder.switch_to_block(cont);
                builder.goto(merge_block);
            }
        }

        // --- Merge block --------------------------------------------------------
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(merge_block);
        }

        Ok(values::local(handle_result))
    }

    /// Lower an `ExprKind::Resume` expression.
    ///
    /// In the setjmp/longjmp one-shot model, `resume(value)` stores the resume
    /// value into the handler result local and returns from the handler clause.
    /// Because the handler clause already ends with an assignment to the
    /// handle-expression result followed by a goto to the merge block, the
    /// simplest lowering is to evaluate the resume value and return it as the
    /// handler clause's value.
    pub(crate) fn lower_resume(&mut self, value: Option<&ast::Expr>) -> CodegenResult<MirValue> {
        if let Some(expr) = value {
            self.lower_expr(expr)
        } else {
            Ok(values::unit())
        }
    }

    /// Lower an `ExprKind::Perform` expression.
    ///
    /// ```text
    /// perform Effect.op(arg1, arg2, ...)
    /// ```
    ///
    /// Generates a call to `build_perform(effect_id, op_id, arg_ptr, result_ptr)`
    /// which longjmps to the nearest matching handler.  The first argument is
    /// passed via the `arg` pointer; the result pointer is set up so the handler
    /// can write a return value back to the perform-site (for the one-shot model
    /// this is not used, but the slot is allocated for future coroutine support).
    pub(crate) fn lower_perform(
        &mut self,
        effect: &ast::Ident,
        operation: &ast::Ident,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        let eid = Self::effect_id(effect.name.as_ref());

        // Compute a simple operation index from the operation name.
        // In a full implementation this would look up the effect declaration to
        // find the canonical index; here we use a hash so that different op
        // names within the same effect get distinct IDs.
        let op_id: i32 = super::tail_resume::op_hash(&operation.name);
        let ret_ty = self
            .effect_op_rets
            .get(&(effect.name.clone(), operation.name.clone()))
            .cloned()
            .filter(|t| *t != MirType::Void)
            .unwrap_or(MirType::i32());

        // Lower every argument. Each is marshalled into a `void*[N]` argv
        // array whose i-th slot holds the address of argument i; the handler
        // reads argument i back via `((void**)handler_data)[i]`. Passing the
        // whole argv (not just the first arg) is what lets a multi-parameter
        // operation see all of its arguments. A zero-argument operation passes
        // a null `arg` pointer, which its handler never dereferences.
        let arg_vals: Vec<MirValue> = args
            .iter()
            .map(|a| self.lower_expr(a))
            .collect::<CodegenResult<Vec<_>>>()?;

        // Compute each argument's type before borrowing current_fn mutably.
        let arg_tys: Vec<MirType> = arg_vals.iter().map(|v| self.type_of_value(v)).collect();

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;

        // Allocate a result slot on the stack.  Use an unnamed local to
        // avoid duplicate C declarations when perform is called multiple
        // times with the same effect/operation.
        let result_local = builder.create_local(ret_ty.clone());

        // Build the argv array of pointers-to-arguments. Each argument value is
        // stored into its own local so its address is stable, then written into
        // `argv[i]`; a `T*` slot value decays to the `void*` element type in C.
        let n = arg_vals.len();
        let arg_ptr_value: MirValue = if n == 0 {
            MirValue::Const(MirConst::Null(MirType::Void))
        } else {
            let argv = builder.create_local(MirType::Array(
                Box::new(MirType::Ptr(Box::new(MirType::Void))),
                n as u64,
            ));
            for (i, (val, ty)) in arg_vals.into_iter().zip(arg_tys.into_iter()).enumerate() {
                let arg_local = builder.create_local(ty);
                builder.assign(arg_local, MirRValue::Use(val));
                builder.push_index_store(
                    MirValue::Local(argv),
                    MirValue::Const(MirConst::Int(i as i128, MirType::i32())),
                    MirType::Ptr(Box::new(MirType::Void)),
                    MirRValue::AddressOf {
                        is_mut: false,
                        place: MirPlace::local(arg_local),
                    },
                );
            }
            MirValue::Local(argv)
        };

        // Take the address of the result slot for the void* result parameter.
        let result_ptr = builder.create_local(MirType::Ptr(Box::new(ret_ty)));
        builder.assign(
            result_ptr,
            MirRValue::AddressOf {
                is_mut: true,
                place: MirPlace::local(result_local),
            },
        );

        // build_perform(effect_id, op_id, argv, &result)
        let perform_fn = MirValue::Function(Arc::from("build_perform"));
        let eid_val = MirValue::Const(MirConst::Int(eid as i128, MirType::i32()));
        let op_val = MirValue::Const(MirConst::Int(op_id as i128, MirType::i32()));

        let cont = builder.create_block();
        builder.call(
            perform_fn,
            vec![eid_val, op_val, arg_ptr_value, MirValue::Local(result_ptr)],
            None,
            cont,
        );
        builder.switch_to_block(cont);

        // In practice build_perform never returns (it longjmps), but at the MIR
        // level we model the continuation so the CFG remains well-formed.  The
        // result local is available if a future coroutine-based implementation
        // stores a return value there.
        Ok(values::local(result_local))
    }

    // =========================================================================
    // BUILTIN MACRO LOWERING
    // =========================================================================

    /// Lower `vec![a, b, c]` (literal) and `vec![val; count]` (repeat) macros.
    ///
    /// Expansion:
    ///   vec![a, b, c]     =>  let v = vec_new_T(); vec_push_T(v, a); vec_push_T(v, b); vec_push_T(v, c); v
    ///   vec![val; count]  =>  let v = vec_new_T(); for i in 0..count { vec_push_T(v, val); } v
    ///
    /// The element type is inferred from the first argument expression.
    pub(crate) fn lower_vec_macro(&mut self, tokens: &[ast::TokenTree]) -> CodegenResult<MirValue> {
        // When a `Vec<T>` type is expected (from a let annotation or another
        // bidirectional hint), lower each element with `T` as its expected type
        // and build the vec at `T`. Without this an annotated
        // `let v: Vec<i64> = vec![1, 2, 3]` infers `i32` from the bare literals,
        // stores 4-byte-strided elements through the i32 push helper, and every
        // `v[i]` reads at the i64 width the annotation requires -> the read
        // spans two elements: a silent wrong answer with no diagnostic.
        let expected_elem: Option<MirType> = match &self.expected_type {
            Some(MirType::Vec(elem)) => Some((**elem).clone()),
            _ => None,
        };

        // Check if tokens have valid spans matching our source file.
        // When the macro expander expands nested vec! calls, the inner
        // tokens get synthetic spans that don't match self.source.
        // In that case, fall back to creating an empty vec (the macro
        // expander already handled the expansion at a higher level).
        if let Some(ref source) = self.source {
            let has_valid_spans = tokens.iter().any(|t| {
                if let ast::TokenTree::Token(tok) = t {
                    let s = tok.span.start.to_usize();
                    let e = tok.span.end.to_usize();
                    e > s
                        && e <= source.len()
                        && source.is_char_boundary(s)
                        && source.is_char_boundary(e)
                        && {
                            // Verify the span actually points to code, not a comment
                            let text = &source[s..e];
                            // For an Ident token, the text should be alphanumeric
                            match &tok.kind {
                                crate::lexer::TokenKind::Ident => {
                                    text.chars().all(|c| c.is_alphanumeric() || c == '_')
                                }
                                _ => true,
                            }
                        }
                } else {
                    true // Delimited groups are fine
                }
            });

            if !has_valid_spans && !tokens.is_empty() {
                // Tokens have synthetic spans - this vec! was already expanded
                // by the macro expander. Create an empty vec as placeholder,
                // at the expected element type when one is known (else the f64
                // default this path has always used).
                let placeholder_elem = expected_elem.clone().unwrap_or_else(MirType::f64);
                let (new_fn_name, _) = Self::vec_fn_names_for_type(&placeholder_elem)?;
                let new_fn = MirValue::Function(Arc::from(new_fn_name));
                let vec_ty = MirType::Vec(Box::new(placeholder_elem));
                let builder = self
                    .current_fn
                    .as_mut()
                    .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
                let vec_local = builder.create_local(vec_ty);
                let cont = builder.create_block();
                builder.call(new_fn, vec![], Some(vec_local), cont);
                builder.switch_to_block(cont);
                return Ok(MirValue::Local(vec_local));
            }
        }

        // Split macro tokens into argument groups at top-level commas/semicolons.
        let all_groups = self.split_vec_macro_token_groups(tokens);

        if all_groups.is_empty() {
            // vec![] with no args -- empty vec at the expected element type,
            // else i32.
            let empty_elem = expected_elem.clone().unwrap_or_else(MirType::i32);
            let (new_fn_name, _) = Self::vec_fn_names_for_type(&empty_elem)?;
            let new_fn = MirValue::Function(Arc::from(new_fn_name));
            let vec_ty = MirType::Vec(Box::new(empty_elem));
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let vec_local = builder.create_local(vec_ty);
            let cont = builder.create_block();
            builder.call(new_fn, vec![], Some(vec_local), cont);
            builder.switch_to_block(cont);
            return Ok(MirValue::Local(vec_local));
        }

        // Check for repeat syntax: vec![val; count]
        let is_repeat = self.detect_vec_repeat_syntax(tokens);

        if is_repeat && all_groups.len() == 2 {
            // vec![val; count] -- repeat form
            let val_tokens = all_groups[0].clone();
            let count_tokens = all_groups[1].clone();

            // Lower the value at the expected element type so a bare literal
            // adopts the annotated width; fall back to the inferred type.
            let fresh_per_iter =
                matches!(expected_elem, Some(MirType::Vec(_) | MirType::Map(_, _)));
            let mut val_opt = None;
            let elem_ty = if fresh_per_iter {
                expected_elem.clone().unwrap()
            } else {
                let prev_expected = self.expected_type.take();
                self.expected_type = expected_elem.clone();
                let val = self.parse_and_lower_token_group(val_tokens.clone());
                self.expected_type = prev_expected;
                let val = val?;
                let ty = expected_elem
                    .clone()
                    .unwrap_or_else(|| self.type_of_value(&val));
                val_opt = Some(val);
                ty
            };
            let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&elem_ty)?;
            let vec_ty = MirType::Vec(Box::new(elem_ty.clone()));

            // Create the vec
            let new_fn = MirValue::Function(Arc::from(new_fn_name));
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let vec_local = builder.create_local(vec_ty);
            let cont = builder.create_block();
            builder.call(new_fn, vec![], Some(vec_local), cont);
            builder.switch_to_block(cont);

            // Parse and lower the count expression
            let count_val = self.parse_and_lower_token_group(count_tokens)?;

            // Emit a counted loop: for i in 0..count { vec_push(v, val) }
            // We implement this as a simple while loop in MIR:
            //   let i = 0; while (i < count) { vec_push(v, val); i = i + 1; }
            let i_local = {
                let builder = self.current_fn.as_mut().unwrap();
                let i = builder.create_local(MirType::i64());
                builder.assign(
                    i,
                    MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                );
                i
            };

            // Create loop blocks
            let (loop_header, loop_body, loop_exit) = {
                let builder = self.current_fn.as_mut().unwrap();
                let header = builder.create_block();
                let body = builder.create_block();
                let exit = builder.create_block();
                (header, body, exit)
            };

            // Jump to loop header
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.goto(loop_header);
                builder.switch_to_block(loop_header);
            }

            // Condition: i < count
            let cond = {
                let builder = self.current_fn.as_mut().unwrap();
                let cond = builder.create_local(MirType::Bool);
                builder.assign(
                    cond,
                    MirRValue::BinaryOp {
                        op: BinOp::Lt,
                        left: MirValue::Local(i_local),
                        right: count_val.clone(),
                    },
                );
                cond
            };

            // Branch on condition
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.branch(MirValue::Local(cond), loop_body, loop_exit);
                builder.switch_to_block(loop_body);
            }

            // Push the value (re-evaluate container literals inside the loop so
            // `vec![Vec::new(); n]` produces `n` independent containers).
            let elem_val = if let Some(v) = val_opt {
                v
            } else {
                let prev_expected = self.expected_type.take();
                self.expected_type = Some(elem_ty.clone());
                let v = self.parse_and_lower_token_group(val_tokens.clone());
                self.expected_type = prev_expected;
                v?
            };
            let push_fn = MirValue::Function(Arc::from(push_fn_name));
            {
                let builder = self.current_fn.as_mut().unwrap();
                let cont2 = builder.create_block();
                builder.call(
                    push_fn,
                    vec![MirValue::Local(vec_local), elem_val],
                    None,
                    cont2,
                );
                builder.switch_to_block(cont2);
            }

            // Increment i
            {
                let builder = self.current_fn.as_mut().unwrap();
                let incremented = builder.create_local(MirType::i64());
                builder.assign(
                    incremented,
                    MirRValue::BinaryOp {
                        op: BinOp::Add,
                        left: MirValue::Local(i_local),
                        right: MirValue::Const(MirConst::Int(1, MirType::i64())),
                    },
                );
                builder.assign(i_local, MirRValue::Use(MirValue::Local(incremented)));
                builder.goto(loop_header);
            }

            // Switch to exit block
            {
                let builder = self.current_fn.as_mut().unwrap();
                builder.switch_to_block(loop_exit);
            }

            Ok(MirValue::Local(vec_local))
        } else {
            // vec![a, b, c] -- literal form
            // Lower the first argument at the expected element type (so a bare
            // literal adopts the annotated width), else infer from its value.
            let first_tokens = all_groups[0].clone();
            let prev_expected = self.expected_type.take();
            self.expected_type = expected_elem.clone();
            let first_val = self.parse_and_lower_token_group(first_tokens);
            self.expected_type = prev_expected;
            let first_val = first_val?;
            let elem_ty = expected_elem
                .clone()
                .unwrap_or_else(|| self.type_of_value(&first_val));
            let (new_fn_name, push_fn_name) = Self::vec_fn_names_for_type(&elem_ty)?;
            let vec_ty = MirType::Vec(Box::new(elem_ty));

            // Create the vec
            let new_fn = MirValue::Function(Arc::from(new_fn_name));
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let vec_local = builder.create_local(vec_ty);
            let cont = builder.create_block();
            builder.call(new_fn, vec![], Some(vec_local), cont);
            builder.switch_to_block(cont);

            // Push the first element
            let push_fn_val = MirValue::Function(Arc::from(push_fn_name.as_str()));
            {
                let builder = self.current_fn.as_mut().unwrap();
                let cont2 = builder.create_block();
                builder.call(
                    push_fn_val,
                    vec![MirValue::Local(vec_local), first_val],
                    None,
                    cont2,
                );
                builder.switch_to_block(cont2);
            }

            // Push remaining elements, each lowered at the expected element type.
            for group in &all_groups[1..] {
                let prev_expected = self.expected_type.take();
                self.expected_type = expected_elem.clone();
                let val = self.parse_and_lower_token_group(group.clone());
                self.expected_type = prev_expected;
                let val = val?;
                let push_fn_val = MirValue::Function(Arc::from(push_fn_name.as_str()));
                let builder = self.current_fn.as_mut().unwrap();
                let cont2 = builder.create_block();
                builder.call(
                    push_fn_val,
                    vec![MirValue::Local(vec_local), val],
                    None,
                    cont2,
                );
                builder.switch_to_block(cont2);
            }

            Ok(MirValue::Local(vec_local))
        }
    }

    /// Map a vec element type to its runtime build/push helper pair, or reject
    /// the element type when the HVec runtime cannot store it.
    ///
    /// Two storage strategies back a `Vec<T>`. Scalars and strings ride a
    /// built-in `build_hvec_*_<suffix>` family: f64 (also f32), i64 (also
    /// isize), i32 (the narrow integer widths, u32, char, and bool), and str.
    /// A user struct, a nested vector, or a map rides a monomorphized,
    /// element-sized wrapper the C backend emits per element type
    /// (`vec_elem_needs_sized_wrapper` in codegen/backend/c.rs), keyed by the
    /// struct name or the nested handle's C type. Selecting the wrapper suffix
    /// here keeps `vec![Point { .. }]` in step with `v.push(Point { .. })`,
    /// which the vec method accessors already lower through the same wrapper.
    ///
    /// A tuple or an array element has no representation in either strategy: no
    /// scalar family fits it and the backend emits no wrapper for it. Reject it
    /// rather than fall through to the i32 family, which for an array element
    /// silently miscompiled (the array decayed to a pointer truncated to
    /// `int32_t` under `-Wint-conversion`, warn-only) and for a tuple leaked a
    /// raw C type error. A vector of a tuple or array element is an honest gap;
    /// it needs a wrapper that stores and projects the aggregate, the same path
    /// structs and nested vectors already take.
    ///
    /// Keep the supported suffixes in sync with `hvec_elem_suffix` and
    /// `vec_elem_needs_sized_wrapper` in codegen/backend/c.rs and with the vec
    /// method and free-function accessor tables in codegen/lower/expr.rs, so a
    /// `vec![...]` builds the same typed handle those accessors later read.
    pub(super) fn vec_fn_names_for_type(elem_ty: &MirType) -> CodegenResult<(String, String)> {
        let suffix: String = match elem_ty {
            MirType::Float(FloatSize::F64) | MirType::Float(FloatSize::F32) => "f64".to_string(),
            MirType::Int(IntSize::I8, true) => "i8".to_string(),
            MirType::Int(IntSize::I16, true) => "i16".to_string(),
            MirType::Int(IntSize::I32, true) => "i32".to_string(),
            MirType::Int(IntSize::I64, true) => "i64".to_string(),
            MirType::Int(IntSize::I128, true) => "i128".to_string(),
            MirType::Int(IntSize::ISize, true) => "isize".to_string(),
            MirType::Int(IntSize::I8, false) => "u8".to_string(),
            MirType::Int(IntSize::I16, false) => "u16".to_string(),
            MirType::Int(IntSize::I32, false) => "u32".to_string(),
            MirType::Int(IntSize::I64, false) => "u64".to_string(),
            MirType::Int(IntSize::I128, false) => "u128".to_string(),
            MirType::Int(IntSize::ISize, false) => "usize".to_string(),
            MirType::Struct(n) if n.as_ref() == "BuildString" => "str".to_string(),
            MirType::Bool => "i32".to_string(),
            // A user struct, a nested vector, or a map rides the element-sized
            // wrapper the C backend generates; the suffix is the struct name or
            // the nested handle's C type, matching hvec_elem_suffix.
            MirType::Struct(n) => n.to_string(),
            MirType::Vec(_) => "BuildVecHandle".to_string(),
            MirType::Map(_, _) => "BuildMapHandle".to_string(),
            MirType::Option(_) => "Option".to_string(),
            // A view element: keyed by its slice struct name, as in
            // hvec_elem_suffix.
            MirType::Ptr(inner) if matches!(inner.as_ref(), MirType::Slice(_)) => {
                match inner.as_ref() {
                    MirType::Slice(e) => MirType::slice_type_name(e).to_string(),
                    _ => unreachable!(),
                }
            }
            MirType::Ptr(inner) => crate::codegen::backend::c::CBackend::hvec_ptr_suffix(inner),
            MirType::Tuple(elems)
                if crate::codegen::backend::c::CBackend::is_supported_vec_tuple(elems) =>
            {
                MirType::tuple_type_name(elems).to_string()
            }
            MirType::Array(inner, len)
                if crate::codegen::backend::c::CBackend::is_supported_vec_array(inner, *len) =>
            {
                crate::codegen::backend::c::CBackend::hvec_array_suffix(inner, *len)
            }
            _ => {
                return Err(CodegenError::Unsupported(format!(
                    "a vector of `{elem_ty}` elements is not supported yet: the Vec \
                     runtime stores scalar (integer, float, bool, char), string, \
                     struct, nested-vector, and map elements, not a tuple or array \
                     element"
                )))
            }
        };
        Ok((
            format!("build_hvec_new_{suffix}"),
            format!("build_hvec_push_{suffix}"),
        ))
    }

    /// Extract all argument source texts from vec! macro tokens.
    /// Unlike print macros, vec! has no format string to skip.
    /// Split vec! macro tokens into argument token groups at top-level
    /// commas or semicolons. Returns flattened token lists suitable for
    /// passing directly to the parser, avoiding span-based source text
    /// extraction which breaks when tokens have cross-source spans.
    fn split_vec_macro_token_groups(
        &self,
        tokens: &[ast::TokenTree],
    ) -> Vec<Vec<crate::lexer::Token>> {
        use crate::lexer::TokenKind;

        // Unwrap outermost Delimited group if present
        let inner: &[ast::TokenTree] = if tokens.len() == 1 {
            if let ast::TokenTree::Delimited {
                tokens: ref inner, ..
            } = tokens[0]
            {
                inner
            } else {
                tokens
            }
        } else {
            tokens
        };

        let mut groups: Vec<Vec<crate::lexer::Token>> = Vec::new();
        let mut current: Vec<crate::lexer::Token> = Vec::new();
        let mut depth: i32 = 0;

        // Recursively flatten a TokenTree into a Vec<Token>
        fn flatten_tree(tree: &ast::TokenTree, out: &mut Vec<crate::lexer::Token>) {
            match tree {
                ast::TokenTree::Token(tok) => out.push(tok.clone()),
                ast::TokenTree::Delimited {
                    delimiter,
                    tokens: inner,
                    span,
                } => {
                    out.push(crate::lexer::Token::new(
                        TokenKind::OpenDelim(*delimiter),
                        *span,
                    ));
                    for t in inner {
                        flatten_tree(t, out);
                    }
                    out.push(crate::lexer::Token::new(
                        TokenKind::CloseDelim(*delimiter),
                        *span,
                    ));
                }
            }
        }

        for token in inner {
            match token {
                ast::TokenTree::Token(tok) => match &tok.kind {
                    TokenKind::OpenDelim(_) => {
                        depth += 1;
                        current.push(tok.clone());
                    }
                    TokenKind::CloseDelim(_) => {
                        depth -= 1;
                        current.push(tok.clone());
                    }
                    TokenKind::Comma | TokenKind::Semi if depth == 0 => {
                        if !current.is_empty() {
                            groups.push(std::mem::take(&mut current));
                        }
                    }
                    _ => {
                        current.push(tok.clone());
                    }
                },
                ast::TokenTree::Delimited { .. } => {
                    // Flatten the delimited group into individual tokens
                    // so the parser can handle it
                    flatten_tree(token, &mut current);
                }
            }
        }

        if !current.is_empty() {
            groups.push(current);
        }

        groups
    }

    /// Parse a token group as an expression and lower it.
    fn parse_and_lower_token_group(
        &mut self,
        tokens: Vec<crate::lexer::Token>,
    ) -> CodegenResult<MirValue> {
        use crate::lexer::SourceFile;
        use crate::parser::Parser;

        // Build source text. First try span-based extraction from source,
        // then validate it by re-tokenizing. If validation fails, use
        // reconstructed text from token kinds.
        let source_text = self.extract_source_for_tokens(&tokens);

        let sf = SourceFile::anonymous(source_text.as_str());
        match crate::lexer::tokenize(source_text.as_str()) {
            Ok(new_tokens) => {
                let mut parser = Parser::new(&sf, new_tokens);
                match parser.parse_expr() {
                    Ok(expr) => {
                        // The parsed expression's token spans are relative to
                        // `source_text` (the anonymous re-tokenized string), not
                        // the original file. Point self.source at it while we
                        // lower, so a nested macro that re-extracts source by
                        // span reads the right text. Without this, an inner
                        // vec! inside a Vec<Vec<_>> literal sliced the original
                        // file at a stale offset and pushed a garbage element
                        // (an unrelated identifier, then the 0 parse-failure
                        // default). Restore the previous source afterward.
                        let prev_source = self.source.replace(Arc::from(source_text.as_str()));
                        let result = self.lower_expr(&expr);
                        self.source = prev_source;
                        result
                    }
                    Err(_) => {
                        // Parsing failed - tokens likely have synthetic spans.
                        // Return a sensible default (0 for numerics).
                        Ok(MirValue::Const(MirConst::Int(0, MirType::i64())))
                    }
                }
            }
            Err(_) => {
                // Tokenization failed - return default
                Ok(MirValue::Const(MirConst::Int(0, MirType::i64())))
            }
        }
    }

    /// Resolve the source buffer that owns `source_id` when not inside an
    /// anonymous re-parsed macro fragment, falling back to `self.source`.
    fn source_for_id(&self, source_id: Option<crate::lexer::SourceId>) -> Option<Arc<str>> {
        if !self.in_macro_fragment {
            if let Some(sid) = source_id.filter(|&s| s != crate::lexer::SourceId::ANONYMOUS) {
                if let Some(text) = crate::lexer::source_text_for_id(sid) {
                    return Some(text);
                }
            }
        }
        self.source.clone()
    }

    /// Extract source text for a token group. Tries span-based extraction first,
    /// validates by checking token count matches, falls back to reconstruction.
    fn extract_source_for_tokens(&self, tokens: &[crate::lexer::Token]) -> String {
        let source_id = tokens.first().map(|t| t.span.source_id);
        if let Some(src) = self.source_for_id(source_id) {
            let first_start = tokens.first().map(|t| t.span.start.to_usize()).unwrap_or(0);
            let last_end = tokens.last().map(|t| t.span.end.to_usize()).unwrap_or(0);
            if last_end > first_start
                && last_end <= src.len()
                && src.is_char_boundary(first_start)
                && src.is_char_boundary(last_end)
            {
                // With the module merge's renames applied (a module call in a
                // `vec![...]` element).
                let candidate = renamed_slice(&src, first_start, last_end, source_id);
                // Validate: re-tokenize and check it produces a similar token count
                if let Ok(re_tokens) = crate::lexer::tokenize(candidate.as_str()) {
                    // If re-tokenization gives roughly the same number of tokens, use it
                    if re_tokens.len() >= tokens.len().saturating_sub(2)
                        && re_tokens.len() <= tokens.len() + 2
                    {
                        return candidate;
                    }
                }
            }
        }
        // Fallback: reconstruct from token kinds
        self.reconstruct_source_from_tokens(tokens)
    }

    /// Reconstruct source text from token kinds when spans are unreliable.
    fn reconstruct_source_from_tokens(&self, tokens: &[crate::lexer::Token]) -> String {
        use crate::lexer::{Delimiter, LiteralKind, TokenKind};

        let mut parts = Vec::new();
        for tok in tokens {
            let text = match &tok.kind {
                TokenKind::Ident | TokenKind::RawIdent | TokenKind::Lifetime => {
                    // Try to extract from source
                    if let Some(ref src) = self.source {
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if e > s
                            && e <= src.len()
                            && src.is_char_boundary(s)
                            && src.is_char_boundary(e)
                        {
                            src[s..e].to_string()
                        } else {
                            "_".to_string()
                        }
                    } else {
                        "_".to_string()
                    }
                }
                TokenKind::Literal { kind, suffix: _ } => {
                    if let Some(ref src) = self.source {
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if e > s && e <= src.len() {
                            src[s..e].to_string()
                        } else {
                            match kind {
                                LiteralKind::Int { .. } => "0".to_string(),
                                LiteralKind::Float { .. } => "0.0".to_string(),
                                LiteralKind::Str { .. } => "\"\"".to_string(),
                                LiteralKind::Char { .. } => "'\\0'".to_string(),
                                _ => "0".to_string(),
                            }
                        }
                    } else {
                        "0".to_string()
                    }
                }
                TokenKind::Keyword(kw) => format!("{:?}", kw).to_lowercase(),
                TokenKind::Plus => "+".to_string(),
                TokenKind::Minus => "-".to_string(),
                TokenKind::Star => "*".to_string(),
                TokenKind::Slash => "/".to_string(),
                TokenKind::Percent => "%".to_string(),
                TokenKind::And => "&".to_string(),
                TokenKind::Or => "|".to_string(),
                TokenKind::Caret => "^".to_string(),
                TokenKind::Not => "!".to_string(),
                TokenKind::Dot => ".".to_string(),
                TokenKind::DotDot => "..".to_string(),
                TokenKind::DotDotEq => "..=".to_string(),
                TokenKind::Comma => ",".to_string(),
                TokenKind::Semi => ";".to_string(),
                TokenKind::Colon => ":".to_string(),
                TokenKind::ColonColon => "::".to_string(),
                TokenKind::Eq => "=".to_string(),
                TokenKind::EqEq => "==".to_string(),
                TokenKind::Ne => "!=".to_string(),
                TokenKind::Lt => "<".to_string(),
                TokenKind::Gt => ">".to_string(),
                TokenKind::Le => "<=".to_string(),
                TokenKind::Ge => ">=".to_string(),
                TokenKind::OpenDelim(Delimiter::Paren) => "(".to_string(),
                TokenKind::OpenDelim(Delimiter::Bracket) => "[".to_string(),
                TokenKind::OpenDelim(Delimiter::Brace) => "{".to_string(),
                TokenKind::CloseDelim(Delimiter::Paren) => ")".to_string(),
                TokenKind::CloseDelim(Delimiter::Bracket) => "]".to_string(),
                TokenKind::CloseDelim(Delimiter::Brace) => "}".to_string(),
                TokenKind::Arrow => "->".to_string(),
                TokenKind::FatArrow => "=>".to_string(),
                TokenKind::Underscore => "_".to_string(),
                TokenKind::Tilde => "~".to_string(),
                TokenKind::At => "@".to_string(),
                TokenKind::Pound => "#".to_string(),
                TokenKind::Question => "?".to_string(),
                TokenKind::Shl => "<<".to_string(),
                TokenKind::Shr => ">>".to_string(),
                _ => {
                    // Fallback: try span extraction
                    if let Some(ref src) = self.source {
                        let s = tok.span.start.to_usize();
                        let e = tok.span.end.to_usize();
                        if e > s
                            && e <= src.len()
                            && src.is_char_boundary(s)
                            && src.is_char_boundary(e)
                        {
                            src[s..e].to_string()
                        } else {
                            " ".to_string()
                        }
                    } else {
                        " ".to_string()
                    }
                }
            };
            parts.push(text);
        }
        parts.join(" ")
    }

    /// Detect whether the token stream contains a semicolon at the top level,
    /// indicating the `vec![val; count]` repeat syntax.
    fn detect_vec_repeat_syntax(&self, tokens: &[ast::TokenTree]) -> bool {
        use crate::lexer::{Delimiter, TokenKind};

        // Unwrap outermost Delimited if present, don't flatten
        let inner: &[ast::TokenTree] = if tokens.len() == 1 {
            if let ast::TokenTree::Delimited {
                tokens: ref inner, ..
            } = tokens[0]
            {
                inner
            } else {
                tokens
            }
        } else {
            tokens
        };

        let mut depth: i32 = 0;
        for token in inner {
            match token {
                ast::TokenTree::Delimited { .. } => {
                    // Nested delimited group - skip it entirely (don't
                    // look inside for semicolons).
                }
                ast::TokenTree::Token(tok) => match &tok.kind {
                    TokenKind::OpenDelim(Delimiter::Paren)
                    | TokenKind::OpenDelim(Delimiter::Bracket)
                    | TokenKind::OpenDelim(Delimiter::Brace) => depth += 1,
                    TokenKind::CloseDelim(Delimiter::Paren)
                    | TokenKind::CloseDelim(Delimiter::Bracket)
                    | TokenKind::CloseDelim(Delimiter::Brace) => depth -= 1,
                    TokenKind::Semi if depth == 0 => return true,
                    _ => {}
                },
            }
        }
        false
    }

    /// Shared format-macro processing for `print!`/`println!`/`format!`: parse
    /// the format string and arguments, convert `{}`/`{:?}`/`{:.N}` placeholders
    /// to C printf specifiers, intern the C format string, and return its string
    /// index plus the lowered argument values (BuildString args reduced to their
    /// `.ptr`, trimmed to the placeholder count).
    fn prepare_format_call(
        &mut self,
        tokens: &[ast::TokenTree],
        newline: bool,
    ) -> CodegenResult<(u32, Vec<MirValue>)> {
        use super::format::{parse_format, ArgRef, Piece};

        let format_str = self.extract_string_from_tokens(tokens);
        let pieces = parse_format(&format_str).map_err(|message| CodegenError::Rejected {
            message,
            location: None,
            help: Some(
                "supported: `{}`, `{:?}`, `{name}`, `{0}`, width, dynamic width (`w$`/`0$`), \
                 `<` `>` `^` alignment, `+`, `#`, `0`, precision, and `x` `X` `o` `e` `E`"
                    .to_string(),
            ),
        })?;

        // Lower explicit positional and named (`name = expr`) arguments in order.
        let arg_source_texts = self.extract_arg_source_texts(tokens);
        let mut explicit: Vec<(MirValue, String)> = Vec::new();
        let mut named: HashMap<String, (MirValue, String)> = HashMap::new();
        for src in &arg_source_texts {
            let trimmed = src.trim();
            if let Some((lhs, rhs)) = Self::split_named_format_arg(trimmed) {
                let val = self.parse_and_lower_macro_arg(rhs)?;
                named.insert(lhs.to_string(), (val, rhs.trim().to_string()));
            } else {
                let val = self.parse_and_lower_macro_arg(trimmed)?;
                explicit.push((val, trimmed.to_string()));
            }
        }

        let mut c_fmt = String::new();
        let mut values = Vec::new();
        let mut next = 0usize;
        for piece in &pieces {
            match piece {
                Piece::Text(t) => c_fmt.push_str(&t.replace('%', "%%")),
                Piece::Arg(arg_ref, spec) => {
                    let (val, what) = match arg_ref {
                        ArgRef::Next => {
                            let i = next;
                            next += 1;
                            explicit.get(i).cloned().ok_or_else(|| CodegenError::Rejected {
                                message: format!(
                                    "format string `{format_str}` has more `{{}}` placeholders than arguments"
                                ),
                                location: None,
                                help: Some("pass one argument per `{}`".to_string()),
                            })?
                        }
                        ArgRef::Index(i) => {
                            explicit.get(*i).cloned().ok_or_else(|| CodegenError::Rejected {
                                message: format!(
                                    "format string `{format_str}` refers to argument {i}, which was not passed"
                                ),
                                location: None,
                                help: Some("positional arguments count from 0".to_string()),
                            })?
                        }
                        ArgRef::Name(name) => {
                            if let Some(found) = named.get(name) {
                                found.clone()
                            } else {
                                (self.parse_and_lower_macro_arg(name)?, name.clone())
                            }
                        }
                    };
                    let dyn_width = match &spec.dynamic_width {
                        None => None,
                        Some(ArgRef::Index(i)) => {
                            let (w_val, _) =
                                explicit.get(*i).cloned().ok_or_else(|| CodegenError::Rejected {
                                    message: format!(
                                        "format string `{format_str}` refers to width argument {i}, which was not passed"
                                    ),
                                    location: None,
                                    help: Some("positional arguments count from 0".to_string()),
                                })?;
                            Some(w_val)
                        }
                        Some(ArgRef::Name(name)) => {
                            let w_val = if let Some((v, _)) = named.get(name) {
                                v.clone()
                            } else {
                                self.parse_and_lower_macro_arg(name)?
                            };
                            Some(w_val)
                        }
                        Some(ArgRef::Next) => None,
                    };
                    let (conv, arg) = self.fmt_placeholder(val, spec, &what, dyn_width)?;
                    c_fmt.push_str(&conv);
                    if let Some(a) = arg {
                        values.push(a);
                    }
                }
            }
        }
        if newline {
            c_fmt.push('\n');
        }
        let str_idx = self.module.intern_string(c_fmt);
        Ok((str_idx, values))
    }

    /// `print!`/`println!`/`eprint!`/`eprintln!`/`dbg!`: format the arguments
    /// and lower to a backend-neutral stdout or stderr printf-style output call.
    pub(crate) fn lower_print_macro(
        &mut self,
        tokens: &[ast::TokenTree],
        newline: bool,
        stderr: bool,
    ) -> CodegenResult<()> {
        let (str_idx, arg_values) = self.prepare_format_call(tokens, newline)?;
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;
        let fmt_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
        builder.assign(
            fmt_local,
            MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))),
        );
        let continue_block = builder.create_block();
        let print_fn = if stderr { "eprintf" } else { "printf" };
        let mut call_args = vec![MirValue::Local(fmt_local)];
        call_args.extend(arg_values);
        builder.call(
            MirValue::Function(Arc::from(print_fn)),
            call_args,
            None,
            continue_block,
        );
        builder.switch_to_block(continue_block);
        Ok(())
    }

    /// `format!`: build an owned `BuildString` from the format string and
    /// arguments via the variadic `build_sprintf` runtime function (no trailing
    /// newline; returns the string instead of printing it).
    pub(crate) fn lower_format_macro(
        &mut self,
        tokens: &[ast::TokenTree],
    ) -> CodegenResult<MirValue> {
        let (str_idx, arg_values) = self.prepare_format_call(tokens, false)?;
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;
        let fmt_local = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
        builder.assign(
            fmt_local,
            MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))),
        );
        let result = builder.create_local(MirType::Struct(Arc::from("BuildString")));
        let continue_block = builder.create_block();
        let sprintf_fn = MirValue::Function(Arc::from("build_sprintf"));
        let mut call_args = vec![MirValue::Local(fmt_local)];
        call_args.extend(arg_values);
        builder.call(sprintf_fn, call_args, Some(result), continue_block);
        builder.switch_to_block(continue_block);
        Ok(values::local(result))
    }

    /// Split a named format argument `ident = expr` while ignoring `==`, `!=`, `<=`, `>=`, `=>`.
    fn split_named_format_arg(s: &str) -> Option<(&str, &str)> {
        let bytes = s.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        if i == 0 || bytes[0].is_ascii_digit() {
            return None;
        }
        let name = &s[..i];
        let rest = s[i..].trim_start();
        if let Some(after_eq) = rest.strip_prefix('=') {
            if !after_eq.starts_with('=') && !after_eq.starts_with('>') {
                return Some((name, after_eq.trim()));
            }
        }
        None
    }

    /// `write!(f, "fmt", ...)` / `writeln!(f, "fmt", ...)`: format into a
    /// `&mut BuildFormatter` (or `&mut BuildString`), returning `0` (`Ok(())`).
    pub(crate) fn lower_write_macro(
        &mut self,
        tokens: &[ast::TokenTree],
        newline: bool,
    ) -> CodegenResult<MirValue> {
        use crate::lexer::{Delimiter, TokenKind};
        let inner: &[ast::TokenTree] = if tokens.len() == 1 {
            if let ast::TokenTree::Delimited {
                tokens: ref inner, ..
            } = tokens[0]
            {
                inner
            } else {
                tokens
            }
        } else {
            tokens
        };
        let mut depth: i32 = 0;
        let mut split_idx: Option<usize> = None;
        for (idx, tt) in inner.iter().enumerate() {
            if let ast::TokenTree::Token(tok) = tt {
                match tok.kind {
                    TokenKind::OpenDelim(Delimiter::Paren)
                    | TokenKind::OpenDelim(Delimiter::Bracket)
                    | TokenKind::OpenDelim(Delimiter::Brace) => depth += 1,
                    TokenKind::CloseDelim(Delimiter::Paren)
                    | TokenKind::CloseDelim(Delimiter::Bracket)
                    | TokenKind::CloseDelim(Delimiter::Brace) => depth -= 1,
                    TokenKind::Comma if depth == 0 => {
                        split_idx = Some(idx);
                        break;
                    }
                    _ => {}
                }
            }
        }
        let (target_tokens, fmt_tokens) = match split_idx {
            Some(i) => (&inner[..i], &inner[i + 1..]),
            None => (inner, &[][..]),
        };
        fn collect_flat(trees: &[ast::TokenTree], out: &mut Vec<crate::lexer::Token>) {
            for tt in trees {
                match tt {
                    ast::TokenTree::Token(tok) => out.push(tok.clone()),
                    ast::TokenTree::Delimited { tokens, .. } => collect_flat(tokens, out),
                }
            }
        }
        let mut flat_target = Vec::new();
        collect_flat(target_tokens, &mut flat_target);
        let target_src = self.reconstruct_source_from_tokens(&flat_target);
        let target_val = if !target_src.trim().is_empty() {
            self.parse_and_lower_macro_arg(target_src.trim())?
        } else {
            return Ok(MirValue::Const(MirConst::Int(0, MirType::i32())));
        };
        let formatted = if fmt_tokens.is_empty() {
            let nl = if newline { "\n" } else { "" };
            let str_idx = self.module.intern_string(nl.to_string());
            let builder = self.current_fn.as_mut().unwrap();
            let fmt_l = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
            builder.assign(
                fmt_l,
                MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))),
            );
            let res = builder.create_local(MirType::Struct(Arc::from("BuildString")));
            let cont = builder.create_block();
            builder.call(
                MirValue::Function(Arc::from("build_string_new")),
                vec![MirValue::Local(fmt_l)],
                Some(res),
                cont,
            );
            builder.switch_to_block(cont);
            values::local(res)
        } else {
            let (str_idx, arg_values) = self.prepare_format_call(fmt_tokens, newline)?;
            let builder = self.current_fn.as_mut().unwrap();
            let fmt_l = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
            builder.assign(
                fmt_l,
                MirRValue::Use(MirValue::Const(MirConst::Str(str_idx))),
            );
            let res = builder.create_local(MirType::Struct(Arc::from("BuildString")));
            let cont = builder.create_block();
            let mut call_args = vec![MirValue::Local(fmt_l)];
            call_args.extend(arg_values);
            builder.call(
                MirValue::Function(Arc::from("build_sprintf")),
                call_args,
                Some(res),
                cont,
            );
            builder.switch_to_block(cont);
            values::local(res)
        };
        let target_ty = self.type_of_value(&target_val);
        let is_formatter_ptr = matches!(
            &target_ty,
            MirType::Ptr(inner) if matches!(inner.as_ref(), MirType::Struct(n) if n.as_ref() == "BuildFormatter")
        );
        if is_formatter_ptr {
            let builder = self.current_fn.as_mut().unwrap();
            let rc = builder.create_local(MirType::i32());
            let cont = builder.create_block();
            builder.call(
                MirValue::Function(Arc::from("build_formatter_write")),
                vec![target_val, formatted],
                Some(rc),
                cont,
            );
            builder.switch_to_block(cont);
            return Ok(values::local(rc));
        }
        let is_str_ptr = matches!(
            &target_ty,
            MirType::Ptr(inner) if matches!(inner.as_ref(), MirType::Struct(n) if n.as_ref() == "BuildString")
        );
        if is_str_ptr {
            let cur_s = self.deref_if_pointer(target_val.clone())?;
            let builder = self.current_fn.as_mut().unwrap();
            let joined = builder.create_local(MirType::Struct(Arc::from("BuildString")));
            let cont = builder.create_block();
            builder.call(
                MirValue::Function(Arc::from("build_string_concat")),
                vec![cur_s, formatted],
                Some(joined),
                cont,
            );
            builder.switch_to_block(cont);
            if let MirValue::Local(ptr_id) = target_val {
                builder.push_deref_assign(ptr_id, MirRValue::Use(MirValue::Local(joined)));
            }
        }
        Ok(MirValue::Const(MirConst::Int(0, MirType::i32())))
    }

    /// Extract the source text of each argument expression in a macro call,
    /// after the format string.  Returns a Vec of source-text strings, one
    /// per argument.  Uses token spans to find comma-separated argument
    /// boundaries in the original source.
    fn extract_arg_source_texts(&self, tokens: &[ast::TokenTree]) -> Vec<String> {
        use crate::lexer::{Delimiter, TokenKind};

        // Unwrap the outermost Delimited group if present, but do NOT flatten
        // nested groups - their spans must stay intact for correct source extraction.
        let inner: &[ast::TokenTree] = if tokens.len() == 1 {
            if let ast::TokenTree::Delimited {
                tokens: ref inner, ..
            } = tokens[0]
            {
                inner
            } else {
                tokens
            }
        } else {
            tokens
        };

        // Walk the token list, tracking delimiter depth to distinguish
        // top-level argument-separating commas from commas inside nested expressions.
        let source_id = inner.iter().find_map(|tree| match tree {
            ast::TokenTree::Token(tok) => Some(tok.span.source_id),
            ast::TokenTree::Delimited { span, .. } => Some(span.source_id),
        });
        let source = match self.source_for_id(source_id) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let source = &source;
        let mut args = Vec::new();
        let mut past_format_string = false;
        let mut depth: i32 = 0;
        let mut current_arg_start: Option<usize> = None; // byte offset in source
        let mut current_arg_end: usize = 0;

        for token in inner {
            match token {
                ast::TokenTree::Delimited { span, .. } => {
                    if !past_format_string {
                        continue;
                    }
                    // Use the full span of the delimited group to extend
                    // the current argument range without flattening.
                    let s = span.start.to_usize();
                    let e = span.end.to_usize();
                    if current_arg_start.is_none() {
                        current_arg_start = Some(s);
                    }
                    if e > current_arg_end {
                        current_arg_end = e;
                    }
                }
                ast::TokenTree::Token(tok) => {
                    if !past_format_string {
                        if let TokenKind::Literal { kind, .. } = &tok.kind {
                            if matches!(
                                kind,
                                crate::lexer::LiteralKind::Str { .. }
                                    | crate::lexer::LiteralKind::RawStr { .. }
                            ) {
                                past_format_string = true;
                            }
                        }
                        continue;
                    }

                    // Track parenthesis/bracket/brace depth
                    match &tok.kind {
                        TokenKind::OpenDelim(Delimiter::Paren)
                        | TokenKind::OpenDelim(Delimiter::Bracket)
                        | TokenKind::OpenDelim(Delimiter::Brace) => {
                            depth += 1;
                            let s = tok.span.start.to_usize();
                            let e = tok.span.end.to_usize();
                            if current_arg_start.is_none() {
                                current_arg_start = Some(s);
                            }
                            if e > current_arg_end {
                                current_arg_end = e;
                            }
                        }
                        TokenKind::CloseDelim(Delimiter::Paren)
                        | TokenKind::CloseDelim(Delimiter::Bracket)
                        | TokenKind::CloseDelim(Delimiter::Brace) => {
                            depth -= 1;
                            let s = tok.span.start.to_usize();
                            let e = tok.span.end.to_usize();
                            if current_arg_start.is_none() {
                                current_arg_start = Some(s);
                            }
                            if e > current_arg_end {
                                current_arg_end = e;
                            }
                        }
                        TokenKind::Comma if depth == 0 => {
                            // Top-level comma: flush current argument
                            if let Some(start) = current_arg_start {
                                if current_arg_end > start && current_arg_end <= source.len() {
                                    let text =
                                        renamed_slice(source, start, current_arg_end, source_id);
                                    if !text.is_empty() {
                                        args.push(text);
                                    }
                                }
                                current_arg_start = None;
                                current_arg_end = 0;
                            }
                        }
                        _ => {
                            // Extend current argument span
                            let s = tok.span.start.to_usize();
                            let e = tok.span.end.to_usize();
                            if current_arg_start.is_none() {
                                current_arg_start = Some(s);
                            }
                            if e > current_arg_end {
                                current_arg_end = e;
                            }
                        }
                    }
                }
            }
        }

        // Flush any remaining argument
        if let Some(start) = current_arg_start {
            if current_arg_end > start && current_arg_end <= source.len() {
                let text = renamed_slice(source, start, current_arg_end, source_id);
                if !text.is_empty() {
                    args.push(text);
                }
            }
        }

        args
    }

    /// Parse a source text fragment as an expression, then lower it through
    /// the normal expression-lowering pipeline.
    fn parse_and_lower_macro_arg(&mut self, src: &str) -> CodegenResult<MirValue> {
        use crate::lexer::SourceFile;
        use crate::parser::Parser;

        // Create a mini source file for the expression fragment
        let sf = SourceFile::anonymous(src);
        let tokens = crate::lexer::tokenize(src).map_err(|e| {
            CodegenError::Internal(format!("Failed to tokenize macro arg '{}': {:?}", src, e))
        })?;
        let mut parser = Parser::new(&sf, tokens);

        let expr = parser.parse_expr().map_err(|e| {
            CodegenError::Internal(format!("Failed to parse macro arg '{}': {:?}", src, e))
        })?;

        // The fragment's spans index into `src`, not the file. A nested macro
        // (`println!("{:?}", vec![1, 2])`) reads its arguments through
        // `self.source` and used to read the file's text at those offsets
        // (`vec![1, 2]` became `vec![i, 0]`). Point `self.source` at the
        // fragment while lowering it, and drop locations meant for the file.
        let saved_source = self.source.replace(Arc::from(src));
        let saved_fragment = std::mem::replace(&mut self.in_macro_fragment, true);
        let result = self.lower_expr(&expr);
        self.source = saved_source;
        self.in_macro_fragment = saved_fragment;
        let val = result?;
        // A global read inline has no local to carry its type: a `bool` was
        // passed to `%s` as it was (a crash) and a `char` printed as its code.
        // Copy it into a typed local, as a `let` would.
        if let MirValue::Global(name) = &val {
            let is_char = self.char_globals.contains(name);
            let ty = self.module.find_global(name).map(|g| g.ty.clone());
            if let Some(ty) = ty.filter(|t| is_char || matches!(t, MirType::Bool)) {
                let builder = self.current_fn.as_mut().ok_or_else(|| {
                    CodegenError::Internal("No current function for macro".into())
                })?;
                let local = builder.create_local(ty);
                builder.assign(local, MirRValue::Use(val.clone()));
                if is_char {
                    self.char_locals.insert(local);
                }
                return Ok(MirValue::Local(local));
            }
        }
        // `x as char` or a call to a `-> char` function prints as a character.
        // A `let` marks its local as a char; an argument written inline has no
        // `let`, so mark its value here (`format!("{}", 65u32 as char)` gave
        // `65` before).
        let call_is_char = match &expr.kind {
            ExprKind::Call { func, .. } => match &func.kind {
                ExprKind::Ident(id) => self.fn_returns_char.contains(&id.name),
                _ => false,
            },
            _ => false,
        };
        if Self::expr_is_char_literal_or_cast(&expr) || call_is_char {
            let ty = self.type_of_value(&val);
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;
            let local = builder.create_local(ty);
            builder.assign(local, MirRValue::Use(val));
            self.char_locals.insert(local);
            return Ok(MirValue::Local(local));
        }
        Ok(val)
    }

    /// Walk a Rust-style format string and report, for each placeholder in
    /// order, whether it is a plain `{}` (default Display). Escaped braces
    /// (`{{`, `}}`) are skipped and every `{:...}` form reports `false`. The
    /// brace bookkeeping mirrors the placeholder loop in `prepare_format_call`,
    /// so index `i` here lines up with placeholder `i` there.
    fn placeholder_is_plain(format_str: &str) -> Vec<bool> {
        let mut kinds = Vec::new();
        let mut chars = format_str.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '{' {
                match chars.peek() {
                    Some('{') => {
                        chars.next();
                    }
                    Some('}') => {
                        chars.next();
                        kinds.push(true);
                    }
                    Some(':') => {
                        chars.next();
                        while let Some(&c) = chars.peek() {
                            chars.next();
                            if c == '}' {
                                break;
                            }
                        }
                        kinds.push(false);
                    }
                    _ => {}
                }
            } else if ch == '}' && chars.peek() == Some(&'}') {
                chars.next();
            }
        }
        kinds
    }

    /// Pick the correct printf format specifier based on the MIR type.
    fn format_specifier_for_type(&self, ty: Option<&MirType>, precision: Option<&str>) -> String {
        match ty {
            Some(MirType::Int(IntSize::I128, _)) => "%s".to_string(),
            Some(MirType::Int(IntSize::I64 | IntSize::ISize, true)) => "%lld".to_string(),
            Some(MirType::Int(IntSize::I64 | IntSize::ISize, false)) => "%llu".to_string(),
            Some(MirType::Int(_, true)) => "%d".to_string(),
            Some(MirType::Int(_, false)) => "%u".to_string(),
            Some(MirType::Float(FloatSize::F32)) | Some(MirType::Float(FloatSize::F64)) => {
                if let Some(prec) = precision {
                    format!("%.{}f", prec)
                } else {
                    "%g".to_string()
                }
            }
            Some(MirType::Bool) => "%s".to_string(), // printed via ternary in C
            Some(MirType::Ptr(_)) => "%s".to_string(), // assume string pointer
            Some(MirType::Struct(name)) if name.as_ref() == "BuildString" => "%s".to_string(),
            _ => "%d".to_string(), // default for integers (most common)
        }
    }

    /// Return a short debug type name for {:?} format.
    fn type_debug_name(&self, ty: Option<&MirType>) -> String {
        match ty {
            Some(MirType::Int(IntSize::I8, true)) => "i8",
            Some(MirType::Int(IntSize::I16, true)) => "i16",
            Some(MirType::Int(IntSize::I32, true)) => "i32",
            Some(MirType::Int(IntSize::I64, true)) => "i64",
            Some(MirType::Int(IntSize::I8, false)) => "u8",
            Some(MirType::Int(IntSize::I16, false)) => "u16",
            Some(MirType::Int(IntSize::I32, false)) => "u32",
            Some(MirType::Int(IntSize::I64, false)) => "u64",
            Some(MirType::Float(FloatSize::F32)) => "f32",
            Some(MirType::Float(FloatSize::F64)) => "f64",
            Some(MirType::Bool) => "bool",
            Some(MirType::Ptr(_)) => "str",
            Some(MirType::Struct(name)) => return name.to_string(),
            _ => "i32",
        }
        .to_string()
    }

    pub(crate) fn lower_panic_macro(&mut self, tokens: &[ast::TokenTree]) -> CodegenResult<()> {
        // Print the panic message first
        self.lower_print_macro(tokens, true, false)?;

        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;

        // Call abort() after printing
        builder.abort();

        // Create an unreachable continuation block for any code after panic
        let unreachable_block = builder.create_block();
        builder.switch_to_block(unreachable_block);

        Ok(())
    }

    pub(crate) fn extract_string_from_tokens(&self, tokens: &[ast::TokenTree]) -> String {
        use crate::lexer::TokenKind;

        for token in tokens {
            match token {
                ast::TokenTree::Token(tok) => {
                    if let TokenKind::Literal { kind, .. } = &tok.kind {
                        match kind {
                            crate::lexer::LiteralKind::Str { .. } => {
                                if let Some(source) = self.source_for_id(Some(tok.span.source_id)) {
                                    let start = tok.span.start.to_usize();
                                    let end = tok.span.end.to_usize();
                                    if start < source.len() && end <= source.len() && start < end {
                                        if let Some(raw) = source.get(start..end) {
                                            let content = raw.strip_prefix('"').unwrap_or(raw);
                                            let content =
                                                content.strip_suffix('"').unwrap_or(content);
                                            return crate::lexer::unescape_str(content);
                                        }
                                    }
                                }
                                return String::new();
                            }
                            crate::lexer::LiteralKind::RawStr { n_hashes, .. } => {
                                if let Some(source) = self.source_for_id(Some(tok.span.source_id)) {
                                    let start = tok.span.start.to_usize();
                                    let end = tok.span.end.to_usize();
                                    if start < source.len() && end <= source.len() && start < end {
                                        if let Some(raw) = source.get(start..end) {
                                            let hashes = n_hashes.unwrap_or(0) as usize;
                                            let prefix_len = 1 + hashes + 1;
                                            let suffix_len = 1 + hashes;
                                            if raw.len() >= prefix_len + suffix_len {
                                                return raw[prefix_len..raw.len() - suffix_len]
                                                    .to_string();
                                            }
                                        }
                                    }
                                }
                                return String::new();
                            }
                            _ => {}
                        }
                    }
                }
                ast::TokenTree::Delimited { tokens: inner, .. } => {
                    let result = self.extract_string_from_tokens(inner);
                    if !result.is_empty() {
                        return result;
                    }
                }
            }
        }
        String::new()
    }

    fn extract_arg_idents_from_tokens(&self, tokens: &[ast::TokenTree]) -> Vec<String> {
        use crate::lexer::TokenKind;

        let mut args = Vec::new();
        let mut past_first_comma = false;
        let mut current_arg = String::new();

        // Flatten delimited groups
        let flat: Vec<&ast::TokenTree> = tokens
            .iter()
            .flat_map(|t| match t {
                ast::TokenTree::Delimited { tokens: inner, .. } => inner.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .collect();

        for token in &flat {
            if let ast::TokenTree::Token(tok) = token {
                match &tok.kind {
                    TokenKind::Comma => {
                        if past_first_comma && !current_arg.is_empty() {
                            args.push(std::mem::take(&mut current_arg));
                        }
                        past_first_comma = true;
                    }
                    TokenKind::Ident if past_first_comma => {
                        // Extract identifier name from source via span
                        if let Some(ref source) = self.source {
                            let start = tok.span.start.to_usize();
                            let end = tok.span.end.to_usize();
                            if start < source.len() && end <= source.len() {
                                let name = source.get(start..end).unwrap_or("");
                                if current_arg.is_empty() {
                                    current_arg = name.to_string();
                                } else {
                                    // Appending after a dot
                                    current_arg.push_str(name);
                                }
                            }
                        }
                    }
                    TokenKind::Dot if past_first_comma && !current_arg.is_empty() => {
                        // Part of a field access expression: ident.field
                        current_arg.push('.');
                    }
                    _ => {}
                }
            }
        }
        // Flush any remaining argument
        if past_first_comma && !current_arg.is_empty() {
            args.push(current_arg);
        }
        args
    }

    // =================================================================
    // Iterator chain lowering: .iter().map(|x| ...).collect() → loop
    // =================================================================

    /// Try to parse a method call chain as an iterator pipeline.
    /// Returns `None` if the chain doesn't start with `.iter()`.
    ///
    /// Walks backwards from the terminal (`collect` or `fold`) through
    /// nested `MethodCall` receivers until it finds `.iter()`.
    pub(crate) fn try_parse_iter_chain<'a>(
        terminal_receiver: &'a ast::Expr,
        terminal_name: &str,
        terminal_args: &'a [ast::Expr],
    ) -> Option<IterChain<'a>> {
        let terminal = match terminal_name {
            "collect" => IterTerminal::Collect,
            "fold" if terminal_args.len() == 2 => IterTerminal::Fold {
                init: &terminal_args[0],
                closure: &terminal_args[1],
            },
            "sum" if terminal_args.is_empty() => IterTerminal::Sum,
            "count" if terminal_args.is_empty() => IterTerminal::Count,
            "product" if terminal_args.is_empty() => IterTerminal::Product,
            "any" if terminal_args.len() == 1 => IterTerminal::Any {
                closure: &terminal_args[0],
            },
            "all" if terminal_args.len() == 1 => IterTerminal::All {
                closure: &terminal_args[0],
            },
            "max" if terminal_args.is_empty() => IterTerminal::Max,
            "min" if terminal_args.is_empty() => IterTerminal::Min,
            "last" if terminal_args.is_empty() => IterTerminal::Last,
            "nth" if terminal_args.len() == 1 => IterTerminal::Nth {
                index: &terminal_args[0],
            },
            "position" if terminal_args.len() == 1 => IterTerminal::Position {
                closure: &terminal_args[0],
            },
            "find" if terminal_args.len() == 1 => IterTerminal::Find {
                closure: &terminal_args[0],
            },
            "for_each" if terminal_args.len() == 1 => IterTerminal::ForEach {
                closure: &terminal_args[0],
            },
            "reduce" if terminal_args.len() == 1 => IterTerminal::Reduce {
                closure: &terminal_args[0],
            },
            "min_by" if terminal_args.len() == 1 => IterTerminal::MinBy {
                closure: &terminal_args[0],
            },
            "max_by" if terminal_args.len() == 1 => IterTerminal::MaxBy {
                closure: &terminal_args[0],
            },
            "max_by_key" if terminal_args.len() == 1 => IterTerminal::MaxByKey {
                closure: &terminal_args[0],
            },
            _ => return None,
        };

        // Walk backwards through the MethodCall chain.
        let mut steps: Vec<IterStep<'a>> = Vec::new();
        let mut current = terminal_receiver;

        loop {
            match &current.kind {
                ExprKind::MethodCall {
                    receiver,
                    method,
                    args,
                    ..
                } => {
                    let name = method.name.as_ref();
                    match name {
                        "iter" | "iter_mut" | "into_iter" => {
                            // Found the base - `receiver` is the source collection.
                            steps.reverse();
                            return Some(IterChain {
                                source: receiver,
                                steps,
                                terminal,
                            });
                        }
                        // Direct iterator-producing string/collection methods whose
                        // lowered value IS the source sequence (Vec<String> or
                        // BuildString bytes/chars).
                        "split_whitespace" | "split" | "splitn" | "lines" | "bytes" | "chars"
                        | "char_indices" | "matches" | "keys" | "values" | "values_mut" => {
                            steps.reverse();
                            return Some(IterChain {
                                source: current,
                                steps,
                                terminal,
                            });
                        }
                        "windows"
                            if args.len() == 1
                                && !matches!(
                                    &receiver.kind,
                                    ExprKind::MethodCall { method: rm, .. }
                                        if matches!(
                                            rm.name.as_ref(),
                                            "iter" | "iter_mut" | "into_iter"
                                        )
                                ) =>
                        {
                            steps.reverse();
                            return Some(IterChain {
                                source: current,
                                steps,
                                terminal,
                            });
                        }
                        "map" if args.len() == 1 => {
                            steps.push(IterStep::Map { closure: &args[0] });
                            current = receiver;
                        }
                        "flat_map" if args.len() == 1 => {
                            steps.push(IterStep::FlatMap { closure: &args[0] });
                            current = receiver;
                        }
                        "filter_map" if args.len() == 1 => {
                            steps.push(IterStep::FilterMap { closure: &args[0] });
                            current = receiver;
                        }
                        "filter" if args.len() == 1 => {
                            steps.push(IterStep::Filter { closure: &args[0] });
                            current = receiver;
                        }
                        "enumerate" if args.is_empty() => {
                            steps.push(IterStep::Enumerate);
                            current = receiver;
                        }
                        "cloned" | "copied" if args.is_empty() => {
                            steps.push(IterStep::Cloned);
                            current = receiver;
                        }
                        "rev" if args.is_empty() => {
                            steps.push(IterStep::Rev);
                            current = receiver;
                        }
                        "take" if args.len() == 1 => {
                            steps.push(IterStep::Take { count: &args[0] });
                            current = receiver;
                        }
                        "take_while" if args.len() == 1 => {
                            steps.push(IterStep::TakeWhile { closure: &args[0] });
                            current = receiver;
                        }
                        "skip" if args.len() == 1 => {
                            steps.push(IterStep::Skip { count: &args[0] });
                            current = receiver;
                        }
                        "step_by" if args.len() == 1 => {
                            steps.push(IterStep::StepBy { step: &args[0] });
                            current = receiver;
                        }
                        "zip" if args.len() == 1 => {
                            steps.push(IterStep::Zip { other: &args[0] });
                            current = receiver;
                        }
                        "chain" if args.len() == 1 => {
                            steps.push(IterStep::Chain { other: &args[0] });
                            current = receiver;
                        }
                        "group_by" if args.len() == 1 => {
                            steps.push(IterStep::GroupBy { closure: &args[0] });
                            current = receiver;
                        }
                        "chunks" if args.len() == 1 => {
                            steps.push(IterStep::Chunks { size: &args[0] });
                            current = receiver;
                        }
                        "windows" if args.len() == 1 => {
                            steps.push(IterStep::Windows { size: &args[0] });
                            current = receiver;
                        }
                        "dedup" if args.is_empty() => {
                            steps.push(IterStep::Dedup);
                            current = receiver;
                        }
                        "unique" if args.is_empty() => {
                            steps.push(IterStep::Unique);
                            current = receiver;
                        }
                        "interleave" if args.len() == 1 => {
                            steps.push(IterStep::Interleave { other: &args[0] });
                            current = receiver;
                        }
                        "merge" if args.len() == 1 => {
                            steps.push(IterStep::Merge { other: &args[0] });
                            current = receiver;
                        }
                        "intersperse" if args.len() == 1 => {
                            steps.push(IterStep::Intersperse { sep: &args[0] });
                            current = receiver;
                        }
                        "cartesian_product" if args.len() == 1 => {
                            steps.push(IterStep::CartesianProduct { other: &args[0] });
                            current = receiver;
                        }
                        "running_fold" if args.len() == 2 => {
                            steps.push(IterStep::RunningFold {
                                init: &args[0],
                                closure: &args[1],
                            });
                            current = receiver;
                        }
                        _ => return None, // Unknown intermediate method
                    }
                }
                // `(a..b).map(f)`: a range with both bounds is a source; its
                // elements are the integers themselves.
                _ if Self::range_source_bounds(current).is_some() => {
                    steps.reverse();
                    return Some(IterChain {
                        source: current,
                        steps,
                        terminal,
                    });
                }
                // `std::iter::repeat(x)` / `iter::repeat(x)` paired with `.take(n)`
                ExprKind::Call { func, args } if args.len() == 1 => {
                    let is_repeat = match &func.kind {
                        ExprKind::Path(p) => {
                            p.last_ident().is_some_and(|i| i.name.as_ref() == "repeat")
                                && p.segments.iter().any(|s| s.ident.name.as_ref() == "iter")
                        }
                        _ => false,
                    };
                    if is_repeat {
                        steps.reverse();
                        return Some(IterChain {
                            source: current,
                            steps,
                            terminal,
                        });
                    }
                    return None;
                }
                // `successors(first, |prev| next)` / `std::iter::successors(...)`
                ExprKind::Call { func, args } if args.len() == 2 => {
                    let is_successors = match &func.kind {
                        ExprKind::Ident(id) => id.name.as_ref() == "successors",
                        ExprKind::Path(p) => p
                            .last_ident()
                            .is_some_and(|i| i.name.as_ref() == "successors"),
                        _ => false,
                    };
                    if is_successors {
                        steps.reverse();
                        return Some(IterChain {
                            source: current,
                            steps,
                            terminal,
                        });
                    }
                    return None;
                }
                _ => return None, // Chain doesn't lead to a MethodCall
            }
        }
    }

    /// Lower a fully parsed iterator chain into an imperative loop.
    ///
    /// For `.collect()` terminals, produces:
    /// ```text
    /// let result = vec_new_T();
    /// for i in 0..vec_len(source) {
    ///     let elem = vec_get_T(source, i);
    ///     // apply each step transform
    ///     vec_push_T(result, final_elem);
    /// }
    /// result
    /// ```
    ///
    /// For `.fold(init, |acc, x| body)` terminals, produces:
    /// ```text
    /// let acc = init;
    /// for i in 0..vec_len(source) {
    ///     let elem = vec_get_T(source, i);
    ///     // apply each step transform
    ///     acc = body(acc, elem);
    /// }
    /// acc
    /// ```
    pub(crate) fn lower_iter_chain(&mut self, chain: &IterChain<'_>) -> CodegenResult<MirValue> {
        // The vector a `collect` is bound to (`let v: Vec<T> = ...`), read
        // before the source is lowered resets it.
        let expected_collect = match (&chain.terminal, &self.expected_type) {
            (IterTerminal::Collect, Some(MirType::Vec(inner))) => Some(inner.as_ref().clone()),
            (IterTerminal::Sum | IterTerminal::Product, Some(t))
                if matches!(t, MirType::Int(..) | MirType::Float(..)) =>
            {
                Some(t.clone())
            }
            _ => None,
        };
        let collect_as_map: Option<(MirType, MirType)> =
            match (&chain.terminal, &self.expected_type) {
                (IterTerminal::Collect, Some(MirType::Map(k, v))) => {
                    Some((k.as_ref().clone(), v.as_ref().clone()))
                }
                _ => None,
            };
        let expected_collect_string = matches!(
            (&chain.terminal, &self.expected_type),
            (IterTerminal::Collect, Some(MirType::Struct(n))) if n.as_ref() == "BuildString"
        );
        // 1. Lower the source: a vector, or a range `a..b` / `a..=b`.
        let (mut elem_ty, mut source, mut len_local) = match Self::range_source_bounds(chain.source)
        {
            Some((start, end, inclusive)) => self.lower_range_source(start, end, inclusive)?,
            None => self.lower_vec_source(chain.source)?,
        };

        // 1a. If the chain has `.chain(other)` step(s), materialize the primary source
        // and each chained source into a single concatenated `Vec<elem_ty>`.
        let chained_others: Vec<&ast::Expr> = chain
            .steps
            .iter()
            .filter_map(|s| match s {
                IterStep::Chain { other } => Some(*other),
                _ => None,
            })
            .collect();
        if !chained_others.is_empty() {
            let (new_fn, push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
            let (get_fn, _) = Self::vec_get_len_fn_names(&elem_ty)?;
            let comb = {
                let b = self.current_fn.as_mut().unwrap();
                let c = b.create_local(MirType::Vec(Box::new(elem_ty.clone())));
                let cb = b.create_block();
                b.call(MirValue::Function(Arc::from(new_fn)), vec![], Some(c), cb);
                b.switch_to_block(cb);
                c
            };
            let mut sources_to_append: Vec<(ChainSource, LocalId)> = vec![(source, len_local)];
            for other_expr in chained_others {
                let other_src = match &other_expr.kind {
                    ExprKind::MethodCall {
                        receiver,
                        method,
                        args,
                        ..
                    } if args.is_empty()
                        && matches!(method.name.as_ref(), "iter" | "iter_mut" | "into_iter") =>
                    {
                        receiver.as_ref()
                    }
                    _ => other_expr,
                };
                let (_, src2, len2) = match Self::range_source_bounds(other_src) {
                    Some((start, end, inclusive)) => {
                        self.lower_range_source(start, end, inclusive)?
                    }
                    None => self.lower_vec_source(other_src)?,
                };
                sources_to_append.push((src2, len2));
            }
            for (src_part, len_part) in sources_to_append {
                let b = self.current_fn.as_mut().unwrap();
                let idx = b.create_local(MirType::i64());
                b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                let cond_bb = b.create_block();
                let body_bb = b.create_block();
                let exit_bb = b.create_block();
                b.goto(cond_bb);
                b.switch_to_block(cond_bb);
                let cmp = b.create_local(MirType::Bool);
                b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_part));
                b.branch(values::local(cmp), body_bb, exit_bb);
                b.switch_to_block(body_bb);
                let el = match &src_part {
                    ChainSource::Vec { local, get_fn } => {
                        let e = b.create_local(elem_ty.clone());
                        let cg = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from(get_fn.as_str())),
                            vec![values::local(*local), values::local(idx)],
                            Some(e),
                            cg,
                        );
                        b.switch_to_block(cg);
                        values::local(e)
                    }
                    ChainSource::Range { first } => {
                        let e = b.create_local(elem_ty.clone());
                        let sum = b.create_local(MirType::i64());
                        b.binary_op(sum, BinOp::Add, values::local(*first), values::local(idx));
                        b.cast(e, CastKind::IntToInt, values::local(sum), elem_ty.clone());
                        values::local(e)
                    }
                    ChainSource::Indexed { local } => {
                        let e = b.create_local(elem_ty.clone());
                        b.assign(
                            e,
                            MirRValue::IndexAccess {
                                base: values::local(*local),
                                index: values::local(idx),
                                elem_ty: elem_ty.clone(),
                            },
                        );
                        values::local(e)
                    }
                    ChainSource::StringBytes { local } => {
                        let idx_u = b.create_local(MirType::usize());
                        b.cast(
                            idx_u,
                            CastKind::IntToInt,
                            values::local(idx),
                            MirType::usize(),
                        );
                        let raw_b = b.create_local(MirType::i32());
                        let cb = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from("build_string_byte_at")),
                            vec![values::local(*local), values::local(idx_u)],
                            Some(raw_b),
                            cb,
                        );
                        b.switch_to_block(cb);
                        let e = b.create_local(elem_ty.clone());
                        b.cast(e, CastKind::IntToInt, values::local(raw_b), elem_ty.clone());
                        values::local(e)
                    }
                };
                let cp = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(push_fn.as_str())),
                    vec![values::local(comb), el],
                    None,
                    cp,
                );
                b.switch_to_block(cp);
                let ni = b.create_local(MirType::i64());
                b.binary_op(
                    ni,
                    BinOp::Add,
                    values::local(idx),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                b.assign(idx, MirRValue::Use(values::local(ni)));
                b.goto(cond_bb);
                b.switch_to_block(exit_bb);
            }
            let b = self.current_fn.as_mut().unwrap();
            let comb_len = b.create_local(MirType::i64());
            let cl = b.create_block();
            b.call(
                MirValue::Function(Arc::from("build_hvec_len")),
                vec![values::local(comb)],
                Some(comb_len),
                cl,
            );
            b.switch_to_block(cl);
            source = ChainSource::Vec {
                local: comb,
                get_fn,
            };
            len_local = comb_len;
        }

        // 1a2. If the chain has a `.group_by(|x| key)` step, materialize consecutive
        // elements with equal keys into `Vec<(K, Vec<T>)>`.
        let group_by_closure = chain.steps.iter().find_map(|s| match s {
            IterStep::GroupBy { closure } => Some(*closure),
            _ => None,
        });
        if let Some(gb_closure) = group_by_closure {
            let key_ty = if let ExprKind::Closure { body, .. } = &gb_closure.kind {
                // Recognize `|s| s.chars().next().unwrap()` -> char (`u32`).
                fn mentions_chars_next(e: &ast::Expr) -> bool {
                    match &e.kind {
                        ExprKind::MethodCall {
                            receiver, method, ..
                        } => {
                            if method.name.as_ref() == "chars" {
                                true
                            } else {
                                mentions_chars_next(receiver)
                            }
                        }
                        ExprKind::Paren(inner) => mentions_chars_next(inner),
                        _ => false,
                    }
                }
                if mentions_chars_next(body) {
                    MirType::u32()
                } else {
                    self.infer_chain_output_type(
                        &elem_ty,
                        &[IterStep::Map {
                            closure: gb_closure,
                        }],
                    )
                }
            } else {
                MirType::i32()
            };
            let grp_vec_ty = MirType::Vec(Box::new(elem_ty.clone()));
            let pair_elems = vec![key_ty.clone(), grp_vec_ty.clone()];
            self.ensure_tuple_type_def(&pair_elems);
            let pair_ty = MirType::Tuple(pair_elems);
            let (elem_new_fn, elem_push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
            let (grp_new_fn, grp_push_fn) = Self::vec_fn_names_for_type(&pair_ty)?;
            let (grp_get_fn, _) = Self::vec_get_len_fn_names(&pair_ty)?;

            let (groups, has_cur, cur_key, cur_vec, idx, cond_bb, body_bb, exit_bb, el_local) = {
                let b = self.current_fn.as_mut().unwrap();
                let groups = b.create_local(MirType::Vec(Box::new(pair_ty.clone())));
                let c0 = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(grp_new_fn.as_str())),
                    vec![],
                    Some(groups),
                    c0,
                );
                b.switch_to_block(c0);
                let has_cur = b.create_local(MirType::Bool);
                b.assign_const(has_cur, MirConst::Bool(false));
                let cur_key = b.create_local(key_ty.clone());
                let cur_vec = b.create_local(grp_vec_ty.clone());
                let c_init_v = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(elem_new_fn.as_str())),
                    vec![],
                    Some(cur_vec),
                    c_init_v,
                );
                b.switch_to_block(c_init_v);
                let idx = b.create_local(MirType::i64());
                b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                let cond_bb = b.create_block();
                let body_bb = b.create_block();
                let exit_bb = b.create_block();
                b.goto(cond_bb);
                b.switch_to_block(cond_bb);
                let cmp = b.create_local(MirType::Bool);
                b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_local));
                b.branch(values::local(cmp), body_bb, exit_bb);
                b.switch_to_block(body_bb);
                let el_local = b.create_local(elem_ty.clone());
                match &source {
                    ChainSource::Vec { local, get_fn } => {
                        let cg = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from(get_fn.as_str())),
                            vec![values::local(*local), values::local(idx)],
                            Some(el_local),
                            cg,
                        );
                        b.switch_to_block(cg);
                    }
                    ChainSource::Range { first } => {
                        let sum = b.create_local(MirType::i64());
                        b.binary_op(sum, BinOp::Add, values::local(*first), values::local(idx));
                        b.cast(
                            el_local,
                            CastKind::IntToInt,
                            values::local(sum),
                            elem_ty.clone(),
                        );
                    }
                    ChainSource::Indexed { local } => {
                        b.assign(
                            el_local,
                            MirRValue::IndexAccess {
                                base: values::local(*local),
                                index: values::local(idx),
                                elem_ty: elem_ty.clone(),
                            },
                        );
                    }
                    ChainSource::StringBytes { local } => {
                        let idx_u = b.create_local(MirType::usize());
                        b.cast(
                            idx_u,
                            CastKind::IntToInt,
                            values::local(idx),
                            MirType::usize(),
                        );
                        let raw_b = b.create_local(MirType::i32());
                        let cb = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from("build_string_byte_at")),
                            vec![values::local(*local), values::local(idx_u)],
                            Some(raw_b),
                            cb,
                        );
                        b.switch_to_block(cb);
                        b.cast(
                            el_local,
                            CastKind::IntToInt,
                            values::local(raw_b),
                            elem_ty.clone(),
                        );
                    }
                }
                (
                    groups, has_cur, cur_key, cur_vec, idx, cond_bb, body_bb, exit_bb, el_local,
                )
            };
            let _ = body_bb;
            let k_val =
                self.lower_iter_map_inline(gb_closure, values::local(el_local), None, None)?;
            let groups_len = {
                let b = self.current_fn.as_mut().unwrap();
                let check_eq_bb = b.create_block();
                let same_bb = b.create_block();
                let new_grp_bb = b.create_block();
                let flush_prev_bb = b.create_block();
                let start_grp_bb = b.create_block();
                let next_iter_bb = b.create_block();
                b.branch(values::local(has_cur), check_eq_bb, start_grp_bb);
                b.switch_to_block(check_eq_bb);
                let eq_k = b.create_local(MirType::Bool);
                b.binary_op(eq_k, BinOp::Eq, values::local(cur_key), k_val.clone());
                b.branch(values::local(eq_k), same_bb, new_grp_bb);
                b.switch_to_block(same_bb);
                let cp1 = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(elem_push_fn.as_str())),
                    vec![values::local(cur_vec), values::local(el_local)],
                    None,
                    cp1,
                );
                b.switch_to_block(cp1);
                b.goto(next_iter_bb);
                b.switch_to_block(new_grp_bb);
                b.goto(flush_prev_bb);
                b.switch_to_block(flush_prev_bb);
                let prev_pair = b.create_local(pair_ty.clone());
                b.assign(
                    prev_pair,
                    MirRValue::Aggregate {
                        kind: AggregateKind::Tuple,
                        operands: vec![values::local(cur_key), values::local(cur_vec)],
                    },
                );
                let cpg = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(grp_push_fn.as_str())),
                    vec![values::local(groups), values::local(prev_pair)],
                    None,
                    cpg,
                );
                b.switch_to_block(cpg);
                b.goto(start_grp_bb);
                b.switch_to_block(start_grp_bb);
                b.assign(cur_key, MirRValue::Use(k_val));
                let fresh_v = b.create_local(grp_vec_ty);
                let cnv = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(elem_new_fn.as_str())),
                    vec![],
                    Some(fresh_v),
                    cnv,
                );
                b.switch_to_block(cnv);
                b.assign(cur_vec, MirRValue::Use(values::local(fresh_v)));
                let cp2 = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(elem_push_fn.as_str())),
                    vec![values::local(cur_vec), values::local(el_local)],
                    None,
                    cp2,
                );
                b.switch_to_block(cp2);
                b.assign_const(has_cur, MirConst::Bool(true));
                b.goto(next_iter_bb);
                b.switch_to_block(next_iter_bb);
                let ni = b.create_local(MirType::i64());
                b.binary_op(
                    ni,
                    BinOp::Add,
                    values::local(idx),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                b.assign(idx, MirRValue::Use(values::local(ni)));
                b.goto(cond_bb);
                b.switch_to_block(exit_bb);
                let flush_last_bb = b.create_block();
                let done_gb_bb = b.create_block();
                b.branch(values::local(has_cur), flush_last_bb, done_gb_bb);
                b.switch_to_block(flush_last_bb);
                let last_pair = b.create_local(pair_ty.clone());
                b.assign(
                    last_pair,
                    MirRValue::Aggregate {
                        kind: AggregateKind::Tuple,
                        operands: vec![values::local(cur_key), values::local(cur_vec)],
                    },
                );
                let cpl = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(grp_push_fn.as_str())),
                    vec![values::local(groups), values::local(last_pair)],
                    None,
                    cpl,
                );
                b.switch_to_block(cpl);
                b.goto(done_gb_bb);
                b.switch_to_block(done_gb_bb);
                let gl = b.create_local(MirType::i64());
                let cgl = b.create_block();
                b.call(
                    MirValue::Function(Arc::from("build_hvec_len")),
                    vec![values::local(groups)],
                    Some(gl),
                    cgl,
                );
                b.switch_to_block(cgl);
                gl
            };
            elem_ty = pair_ty;
            source = ChainSource::Vec {
                local: groups,
                get_fn: grp_get_fn,
            };
            len_local = groups_len;
        }

        // 1a3. Materialize iterator-extension combinators (`chunks`, `windows`,
        // `dedup`, `unique`, `interleave`, `merge`, `intersperse`,
        // `cartesian_product`, `running_fold`) into concrete Vec sources.
        let read_source_elem =
            |b: &mut MirBuilder, src: &ChainSource, idx: LocalId, ety: &MirType| -> LocalId {
                let el = b.create_local(ety.clone());
                match src {
                    ChainSource::Vec { local, get_fn } => {
                        let cg = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from(get_fn.as_str())),
                            vec![values::local(*local), values::local(idx)],
                            Some(el),
                            cg,
                        );
                        b.switch_to_block(cg);
                    }
                    ChainSource::Range { first } => {
                        let sum = b.create_local(MirType::i64());
                        b.binary_op(sum, BinOp::Add, values::local(*first), values::local(idx));
                        b.cast(el, CastKind::IntToInt, values::local(sum), ety.clone());
                    }
                    ChainSource::Indexed { local } => {
                        b.assign(
                            el,
                            MirRValue::IndexAccess {
                                base: values::local(*local),
                                index: values::local(idx),
                                elem_ty: ety.clone(),
                            },
                        );
                    }
                    ChainSource::StringBytes { local } => {
                        let idx_u = b.create_local(MirType::usize());
                        b.cast(
                            idx_u,
                            CastKind::IntToInt,
                            values::local(idx),
                            MirType::usize(),
                        );
                        let raw_b = b.create_local(MirType::i32());
                        let cb = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from("build_string_byte_at")),
                            vec![values::local(*local), values::local(idx_u)],
                            Some(raw_b),
                            cb,
                        );
                        b.switch_to_block(cb);
                        b.cast(el, CastKind::IntToInt, values::local(raw_b), ety.clone());
                    }
                }
                el
            };
        let emit_elem_eq = |b: &mut MirBuilder, a: LocalId, c: LocalId, ety: &MirType| -> LocalId {
            let eq = b.create_local(MirType::Bool);
            if matches!(ety, MirType::Struct(n) if n.as_ref() == "BuildString") {
                let ceq = b.create_block();
                b.call(
                    MirValue::Function(Arc::from("build_string_eq")),
                    vec![values::local(a), values::local(c)],
                    Some(eq),
                    ceq,
                );
                b.switch_to_block(ceq);
            } else {
                b.binary_op(eq, BinOp::Eq, values::local(a), values::local(c));
            }
            eq
        };
        for step in &chain.steps {
            match step {
                IterStep::Chunks { size } => {
                    let sz_raw = self.lower_expr(size)?;
                    let chunk_vec_ty = MirType::Vec(Box::new(elem_ty.clone()));
                    let (elem_new_fn, elem_push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                    let (out_new_fn, out_push_fn) = Self::vec_fn_names_for_type(&chunk_vec_ty)?;
                    let (out_get_fn, _) = Self::vec_get_len_fn_names(&chunk_vec_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let sz = b.create_local(MirType::i64());
                    b.cast(sz, CastKind::IntToInt, sz_raw, MirType::i64());
                    let out = b.create_local(MirType::Vec(Box::new(chunk_vec_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(out_new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let idx = b.create_local(MirType::i64());
                    b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                    let outer_cond = b.create_block();
                    let outer_body = b.create_block();
                    let inner_cond = b.create_block();
                    let inner_body = b.create_block();
                    let inner_done = b.create_block();
                    let outer_exit = b.create_block();
                    let sz_ok = b.create_local(MirType::Bool);
                    b.binary_op(
                        sz_ok,
                        BinOp::Gt,
                        values::local(sz),
                        MirValue::Const(MirConst::Int(0, MirType::i64())),
                    );
                    b.branch(values::local(sz_ok), outer_cond, outer_exit);
                    b.switch_to_block(outer_cond);
                    let more = b.create_local(MirType::Bool);
                    b.binary_op(
                        more,
                        BinOp::Lt,
                        values::local(idx),
                        values::local(len_local),
                    );
                    b.branch(values::local(more), outer_body, outer_exit);
                    b.switch_to_block(outer_body);
                    let chunk = b.create_local(chunk_vec_ty.clone());
                    let cc = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(elem_new_fn.as_str())),
                        vec![],
                        Some(chunk),
                        cc,
                    );
                    b.switch_to_block(cc);
                    let k = b.create_local(MirType::i64());
                    b.assign_const(k, MirConst::Int(0, MirType::i64()));
                    b.goto(inner_cond);
                    b.switch_to_block(inner_cond);
                    let k_lt = b.create_local(MirType::Bool);
                    b.binary_op(k_lt, BinOp::Lt, values::local(k), values::local(sz));
                    let inner_check_i = b.create_block();
                    b.branch(values::local(k_lt), inner_check_i, inner_done);
                    b.switch_to_block(inner_check_i);
                    let i_lt = b.create_local(MirType::Bool);
                    b.binary_op(
                        i_lt,
                        BinOp::Lt,
                        values::local(idx),
                        values::local(len_local),
                    );
                    b.branch(values::local(i_lt), inner_body, inner_done);
                    b.switch_to_block(inner_body);
                    let el = read_source_elem(b, &source, idx, &elem_ty);
                    let cp = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(elem_push_fn.as_str())),
                        vec![values::local(chunk), values::local(el)],
                        None,
                        cp,
                    );
                    b.switch_to_block(cp);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(idx),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(idx, MirRValue::Use(values::local(ni)));
                    let nk = b.create_local(MirType::i64());
                    b.binary_op(
                        nk,
                        BinOp::Add,
                        values::local(k),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(k, MirRValue::Use(values::local(nk)));
                    b.goto(inner_cond);
                    b.switch_to_block(inner_done);
                    let cpo = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(out_push_fn.as_str())),
                        vec![values::local(out), values::local(chunk)],
                        None,
                        cpo,
                    );
                    b.switch_to_block(cpo);
                    b.goto(outer_cond);
                    b.switch_to_block(outer_exit);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    elem_ty = chunk_vec_ty;
                    source = ChainSource::Vec {
                        local: out,
                        get_fn: out_get_fn,
                    };
                    len_local = out_len;
                }
                IterStep::Windows { size } => {
                    let sz_raw = self.lower_expr(size)?;
                    let win_vec_ty = MirType::Vec(Box::new(elem_ty.clone()));
                    let (elem_new_fn, elem_push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                    let (out_new_fn, out_push_fn) = Self::vec_fn_names_for_type(&win_vec_ty)?;
                    let (out_get_fn, _) = Self::vec_get_len_fn_names(&win_vec_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let sz = b.create_local(MirType::i64());
                    b.cast(sz, CastKind::IntToInt, sz_raw, MirType::i64());
                    let out = b.create_local(MirType::Vec(Box::new(win_vec_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(out_new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let idx = b.create_local(MirType::i64());
                    b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                    let outer_cond = b.create_block();
                    let outer_body = b.create_block();
                    let inner_cond = b.create_block();
                    let inner_body = b.create_block();
                    let inner_done = b.create_block();
                    let outer_exit = b.create_block();
                    let sz_ok = b.create_local(MirType::Bool);
                    b.binary_op(
                        sz_ok,
                        BinOp::Gt,
                        values::local(sz),
                        MirValue::Const(MirConst::Int(0, MirType::i64())),
                    );
                    b.branch(values::local(sz_ok), outer_cond, outer_exit);
                    b.switch_to_block(outer_cond);
                    let end_i = b.create_local(MirType::i64());
                    b.binary_op(end_i, BinOp::Add, values::local(idx), values::local(sz));
                    let more = b.create_local(MirType::Bool);
                    b.binary_op(
                        more,
                        BinOp::Le,
                        values::local(end_i),
                        values::local(len_local),
                    );
                    b.branch(values::local(more), outer_body, outer_exit);
                    b.switch_to_block(outer_body);
                    let win = b.create_local(win_vec_ty.clone());
                    let cc = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(elem_new_fn.as_str())),
                        vec![],
                        Some(win),
                        cc,
                    );
                    b.switch_to_block(cc);
                    let k = b.create_local(MirType::i64());
                    b.assign_const(k, MirConst::Int(0, MirType::i64()));
                    b.goto(inner_cond);
                    b.switch_to_block(inner_cond);
                    let k_lt = b.create_local(MirType::Bool);
                    b.binary_op(k_lt, BinOp::Lt, values::local(k), values::local(sz));
                    b.branch(values::local(k_lt), inner_body, inner_done);
                    b.switch_to_block(inner_body);
                    let pos = b.create_local(MirType::i64());
                    b.binary_op(pos, BinOp::Add, values::local(idx), values::local(k));
                    let el = read_source_elem(b, &source, pos, &elem_ty);
                    let cp = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(elem_push_fn.as_str())),
                        vec![values::local(win), values::local(el)],
                        None,
                        cp,
                    );
                    b.switch_to_block(cp);
                    let nk = b.create_local(MirType::i64());
                    b.binary_op(
                        nk,
                        BinOp::Add,
                        values::local(k),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(k, MirRValue::Use(values::local(nk)));
                    b.goto(inner_cond);
                    b.switch_to_block(inner_done);
                    let cpo = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(out_push_fn.as_str())),
                        vec![values::local(out), values::local(win)],
                        None,
                        cpo,
                    );
                    b.switch_to_block(cpo);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(idx),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(idx, MirRValue::Use(values::local(ni)));
                    b.goto(outer_cond);
                    b.switch_to_block(outer_exit);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    elem_ty = win_vec_ty;
                    source = ChainSource::Vec {
                        local: out,
                        get_fn: out_get_fn,
                    };
                    len_local = out_len;
                }
                IterStep::Dedup => {
                    let (new_fn, push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                    let (get_fn, _) = Self::vec_get_len_fn_names(&elem_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let out = b.create_local(MirType::Vec(Box::new(elem_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let has_prev = b.create_local(MirType::Bool);
                    b.assign_const(has_prev, MirConst::Bool(false));
                    let prev = b.create_local(elem_ty.clone());
                    let idx = b.create_local(MirType::i64());
                    b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                    let cond_bb = b.create_block();
                    let body_bb = b.create_block();
                    let check_bb = b.create_block();
                    let push_bb = b.create_block();
                    let next_bb = b.create_block();
                    let exit_bb = b.create_block();
                    b.goto(cond_bb);
                    b.switch_to_block(cond_bb);
                    let cmp = b.create_local(MirType::Bool);
                    b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_local));
                    b.branch(values::local(cmp), body_bb, exit_bb);
                    b.switch_to_block(body_bb);
                    let el = read_source_elem(b, &source, idx, &elem_ty);
                    b.branch(values::local(has_prev), check_bb, push_bb);
                    b.switch_to_block(check_bb);
                    let eq = emit_elem_eq(b, prev, el, &elem_ty);
                    b.branch(values::local(eq), next_bb, push_bb);
                    b.switch_to_block(push_bb);
                    let cp = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(el)],
                        None,
                        cp,
                    );
                    b.switch_to_block(cp);
                    b.assign(prev, MirRValue::Use(values::local(el)));
                    b.assign_const(has_prev, MirConst::Bool(true));
                    b.goto(next_bb);
                    b.switch_to_block(next_bb);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(idx),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(idx, MirRValue::Use(values::local(ni)));
                    b.goto(cond_bb);
                    b.switch_to_block(exit_bb);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    source = ChainSource::Vec { local: out, get_fn };
                    len_local = out_len;
                }
                IterStep::Unique => {
                    let (new_fn, push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                    let (get_fn, _) = Self::vec_get_len_fn_names(&elem_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let out = b.create_local(MirType::Vec(Box::new(elem_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let idx = b.create_local(MirType::i64());
                    b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                    let cond_bb = b.create_block();
                    let body_bb = b.create_block();
                    let scan_cond = b.create_block();
                    let scan_body = b.create_block();
                    let scan_next = b.create_block();
                    let push_bb = b.create_block();
                    let next_bb = b.create_block();
                    let exit_bb = b.create_block();
                    b.goto(cond_bb);
                    b.switch_to_block(cond_bb);
                    let cmp = b.create_local(MirType::Bool);
                    b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_local));
                    b.branch(values::local(cmp), body_bb, exit_bb);
                    b.switch_to_block(body_bb);
                    let el = read_source_elem(b, &source, idx, &elem_ty);
                    let cur_len = b.create_local(MirType::i64());
                    let cl0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(cur_len),
                        cl0,
                    );
                    b.switch_to_block(cl0);
                    let j = b.create_local(MirType::i64());
                    b.assign_const(j, MirConst::Int(0, MirType::i64()));
                    b.goto(scan_cond);
                    b.switch_to_block(scan_cond);
                    let j_lt = b.create_local(MirType::Bool);
                    b.binary_op(j_lt, BinOp::Lt, values::local(j), values::local(cur_len));
                    b.branch(values::local(j_lt), scan_body, push_bb);
                    b.switch_to_block(scan_body);
                    let existing = b.create_local(elem_ty.clone());
                    let cg = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(get_fn.as_str())),
                        vec![values::local(out), values::local(j)],
                        Some(existing),
                        cg,
                    );
                    b.switch_to_block(cg);
                    let eq = emit_elem_eq(b, existing, el, &elem_ty);
                    b.branch(values::local(eq), next_bb, scan_next);
                    b.switch_to_block(scan_next);
                    let nj = b.create_local(MirType::i64());
                    b.binary_op(
                        nj,
                        BinOp::Add,
                        values::local(j),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(j, MirRValue::Use(values::local(nj)));
                    b.goto(scan_cond);
                    b.switch_to_block(push_bb);
                    let cp = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(el)],
                        None,
                        cp,
                    );
                    b.switch_to_block(cp);
                    b.goto(next_bb);
                    b.switch_to_block(next_bb);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(idx),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(idx, MirRValue::Use(values::local(ni)));
                    b.goto(cond_bb);
                    b.switch_to_block(exit_bb);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    source = ChainSource::Vec { local: out, get_fn };
                    len_local = out_len;
                }
                IterStep::Interleave { other } => {
                    let other_src_expr = match &other.kind {
                        ExprKind::MethodCall {
                            receiver,
                            method,
                            args,
                            ..
                        } if args.is_empty()
                            && matches!(
                                method.name.as_ref(),
                                "iter" | "iter_mut" | "into_iter"
                            ) =>
                        {
                            receiver.as_ref()
                        }
                        _ => *other,
                    };
                    let (_, src2, len2) = match Self::range_source_bounds(other_src_expr) {
                        Some((start, end, inclusive)) => {
                            self.lower_range_source(start, end, inclusive)?
                        }
                        None => self.lower_vec_source(other_src_expr)?,
                    };
                    let (new_fn, push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                    let (get_fn, _) = Self::vec_get_len_fn_names(&elem_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let out = b.create_local(MirType::Vec(Box::new(elem_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let idx = b.create_local(MirType::i64());
                    b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                    let cond_bb = b.create_block();
                    let check1 = b.create_block();
                    let push1 = b.create_block();
                    let check2 = b.create_block();
                    let push2 = b.create_block();
                    let next_bb = b.create_block();
                    let exit_bb = b.create_block();
                    b.goto(cond_bb);
                    b.switch_to_block(cond_bb);
                    let lt1 = b.create_local(MirType::Bool);
                    b.binary_op(lt1, BinOp::Lt, values::local(idx), values::local(len_local));
                    let lt2 = b.create_local(MirType::Bool);
                    b.binary_op(lt2, BinOp::Lt, values::local(idx), values::local(len2));
                    let cond_check2 = b.create_block();
                    b.branch(values::local(lt1), check1, cond_check2);
                    b.switch_to_block(cond_check2);
                    b.branch(values::local(lt2), check1, exit_bb);
                    b.switch_to_block(check1);
                    b.branch(values::local(lt1), push1, check2);
                    b.switch_to_block(push1);
                    let e1 = read_source_elem(b, &source, idx, &elem_ty);
                    let cp1 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(e1)],
                        None,
                        cp1,
                    );
                    b.switch_to_block(cp1);
                    b.goto(check2);
                    b.switch_to_block(check2);
                    b.branch(values::local(lt2), push2, next_bb);
                    b.switch_to_block(push2);
                    let e2 = read_source_elem(b, &src2, idx, &elem_ty);
                    let cp2 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(e2)],
                        None,
                        cp2,
                    );
                    b.switch_to_block(cp2);
                    b.goto(next_bb);
                    b.switch_to_block(next_bb);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(idx),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(idx, MirRValue::Use(values::local(ni)));
                    b.goto(cond_bb);
                    b.switch_to_block(exit_bb);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    source = ChainSource::Vec { local: out, get_fn };
                    len_local = out_len;
                }
                IterStep::Merge { other } => {
                    let other_src_expr = match &other.kind {
                        ExprKind::MethodCall {
                            receiver,
                            method,
                            args,
                            ..
                        } if args.is_empty()
                            && matches!(
                                method.name.as_ref(),
                                "iter" | "iter_mut" | "into_iter"
                            ) =>
                        {
                            receiver.as_ref()
                        }
                        _ => *other,
                    };
                    let (_, src2, len2) = match Self::range_source_bounds(other_src_expr) {
                        Some((start, end, inclusive)) => {
                            self.lower_range_source(start, end, inclusive)?
                        }
                        None => self.lower_vec_source(other_src_expr)?,
                    };
                    let (new_fn, push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                    let (get_fn, _) = Self::vec_get_len_fn_names(&elem_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let out = b.create_local(MirType::Vec(Box::new(elem_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let i = b.create_local(MirType::i64());
                    b.assign_const(i, MirConst::Int(0, MirType::i64()));
                    let j = b.create_local(MirType::i64());
                    b.assign_const(j, MirConst::Int(0, MirType::i64()));
                    let both_cond = b.create_block();
                    let both_body = b.create_block();
                    let take1 = b.create_block();
                    let take2 = b.create_block();
                    let rem1_cond = b.create_block();
                    let rem1_body = b.create_block();
                    let rem2_cond = b.create_block();
                    let rem2_body = b.create_block();
                    let done_bb = b.create_block();
                    b.goto(both_cond);
                    b.switch_to_block(both_cond);
                    let ilt = b.create_local(MirType::Bool);
                    b.binary_op(ilt, BinOp::Lt, values::local(i), values::local(len_local));
                    let check_j = b.create_block();
                    b.branch(values::local(ilt), check_j, rem1_cond);
                    b.switch_to_block(check_j);
                    let jlt = b.create_local(MirType::Bool);
                    b.binary_op(jlt, BinOp::Lt, values::local(j), values::local(len2));
                    b.branch(values::local(jlt), both_body, rem1_cond);
                    b.switch_to_block(both_body);
                    let e1 = read_source_elem(b, &source, i, &elem_ty);
                    let e2 = read_source_elem(b, &src2, j, &elem_ty);
                    let le = b.create_local(MirType::Bool);
                    b.binary_op(le, BinOp::Le, values::local(e1), values::local(e2));
                    b.branch(values::local(le), take1, take2);
                    b.switch_to_block(take1);
                    let cp1 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(e1)],
                        None,
                        cp1,
                    );
                    b.switch_to_block(cp1);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(i),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(i, MirRValue::Use(values::local(ni)));
                    b.goto(both_cond);
                    b.switch_to_block(take2);
                    let cp2 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(e2)],
                        None,
                        cp2,
                    );
                    b.switch_to_block(cp2);
                    let nj = b.create_local(MirType::i64());
                    b.binary_op(
                        nj,
                        BinOp::Add,
                        values::local(j),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(j, MirRValue::Use(values::local(nj)));
                    b.goto(both_cond);
                    b.switch_to_block(rem1_cond);
                    let r1 = b.create_local(MirType::Bool);
                    b.binary_op(r1, BinOp::Lt, values::local(i), values::local(len_local));
                    b.branch(values::local(r1), rem1_body, rem2_cond);
                    b.switch_to_block(rem1_body);
                    let re1 = read_source_elem(b, &source, i, &elem_ty);
                    let cpr1 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(re1)],
                        None,
                        cpr1,
                    );
                    b.switch_to_block(cpr1);
                    let ni2 = b.create_local(MirType::i64());
                    b.binary_op(
                        ni2,
                        BinOp::Add,
                        values::local(i),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(i, MirRValue::Use(values::local(ni2)));
                    b.goto(rem1_cond);
                    b.switch_to_block(rem2_cond);
                    let r2 = b.create_local(MirType::Bool);
                    b.binary_op(r2, BinOp::Lt, values::local(j), values::local(len2));
                    b.branch(values::local(r2), rem2_body, done_bb);
                    b.switch_to_block(rem2_body);
                    let re2 = read_source_elem(b, &src2, j, &elem_ty);
                    let cpr2 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(re2)],
                        None,
                        cpr2,
                    );
                    b.switch_to_block(cpr2);
                    let nj2 = b.create_local(MirType::i64());
                    b.binary_op(
                        nj2,
                        BinOp::Add,
                        values::local(j),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(j, MirRValue::Use(values::local(nj2)));
                    b.goto(rem2_cond);
                    b.switch_to_block(done_bb);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    source = ChainSource::Vec { local: out, get_fn };
                    len_local = out_len;
                }
                IterStep::Intersperse { sep } => {
                    let sep_val = self.lower_expr(sep)?;
                    let (new_fn, push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                    let (get_fn, _) = Self::vec_get_len_fn_names(&elem_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let out = b.create_local(MirType::Vec(Box::new(elem_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let idx = b.create_local(MirType::i64());
                    b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                    let cond_bb = b.create_block();
                    let body_bb = b.create_block();
                    let sep_bb = b.create_block();
                    let push_bb = b.create_block();
                    let exit_bb = b.create_block();
                    b.goto(cond_bb);
                    b.switch_to_block(cond_bb);
                    let cmp = b.create_local(MirType::Bool);
                    b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_local));
                    b.branch(values::local(cmp), body_bb, exit_bb);
                    b.switch_to_block(body_bb);
                    let gt0 = b.create_local(MirType::Bool);
                    b.binary_op(
                        gt0,
                        BinOp::Gt,
                        values::local(idx),
                        MirValue::Const(MirConst::Int(0, MirType::i64())),
                    );
                    b.branch(values::local(gt0), sep_bb, push_bb);
                    b.switch_to_block(sep_bb);
                    let cps = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), sep_val.clone()],
                        None,
                        cps,
                    );
                    b.switch_to_block(cps);
                    b.goto(push_bb);
                    b.switch_to_block(push_bb);
                    let el = read_source_elem(b, &source, idx, &elem_ty);
                    let cpe = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn.as_str())),
                        vec![values::local(out), values::local(el)],
                        None,
                        cpe,
                    );
                    b.switch_to_block(cpe);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(idx),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(idx, MirRValue::Use(values::local(ni)));
                    b.goto(cond_bb);
                    b.switch_to_block(exit_bb);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    source = ChainSource::Vec { local: out, get_fn };
                    len_local = out_len;
                }
                IterStep::CartesianProduct { other } => {
                    let other_src_expr = match &other.kind {
                        ExprKind::MethodCall {
                            receiver,
                            method,
                            args,
                            ..
                        } if args.is_empty()
                            && matches!(
                                method.name.as_ref(),
                                "iter" | "iter_mut" | "into_iter"
                            ) =>
                        {
                            receiver.as_ref()
                        }
                        _ => *other,
                    };
                    let (other_ty, src2, len2) = match Self::range_source_bounds(other_src_expr) {
                        Some((start, end, inclusive)) => {
                            self.lower_range_source(start, end, inclusive)?
                        }
                        None => self.lower_vec_source(other_src_expr)?,
                    };
                    let pair_elems = vec![elem_ty.clone(), other_ty.clone()];
                    self.ensure_tuple_type_def(&pair_elems);
                    let pair_ty = MirType::Tuple(pair_elems);
                    let (out_new_fn, out_push_fn) = Self::vec_fn_names_for_type(&pair_ty)?;
                    let (out_get_fn, _) = Self::vec_get_len_fn_names(&pair_ty)?;
                    let b = self.current_fn.as_mut().unwrap();
                    let out = b.create_local(MirType::Vec(Box::new(pair_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(out_new_fn.as_str())),
                        vec![],
                        Some(out),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let i = b.create_local(MirType::i64());
                    b.assign_const(i, MirConst::Int(0, MirType::i64()));
                    let outer_cond = b.create_block();
                    let outer_body = b.create_block();
                    let inner_cond = b.create_block();
                    let inner_body = b.create_block();
                    let inner_done = b.create_block();
                    let outer_exit = b.create_block();
                    b.goto(outer_cond);
                    b.switch_to_block(outer_cond);
                    let ilt = b.create_local(MirType::Bool);
                    b.binary_op(ilt, BinOp::Lt, values::local(i), values::local(len_local));
                    b.branch(values::local(ilt), outer_body, outer_exit);
                    b.switch_to_block(outer_body);
                    let e1 = read_source_elem(b, &source, i, &elem_ty);
                    let j = b.create_local(MirType::i64());
                    b.assign_const(j, MirConst::Int(0, MirType::i64()));
                    b.goto(inner_cond);
                    b.switch_to_block(inner_cond);
                    let jlt = b.create_local(MirType::Bool);
                    b.binary_op(jlt, BinOp::Lt, values::local(j), values::local(len2));
                    b.branch(values::local(jlt), inner_body, inner_done);
                    b.switch_to_block(inner_body);
                    let e2 = read_source_elem(b, &src2, j, &other_ty);
                    let pair_loc = b.create_local(pair_ty.clone());
                    b.assign(
                        pair_loc,
                        MirRValue::Aggregate {
                            kind: AggregateKind::Tuple,
                            operands: vec![values::local(e1), values::local(e2)],
                        },
                    );
                    let cp = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(out_push_fn.as_str())),
                        vec![values::local(out), values::local(pair_loc)],
                        None,
                        cp,
                    );
                    b.switch_to_block(cp);
                    let nj = b.create_local(MirType::i64());
                    b.binary_op(
                        nj,
                        BinOp::Add,
                        values::local(j),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(j, MirRValue::Use(values::local(nj)));
                    b.goto(inner_cond);
                    b.switch_to_block(inner_done);
                    let ni = b.create_local(MirType::i64());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(i),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(i, MirRValue::Use(values::local(ni)));
                    b.goto(outer_cond);
                    b.switch_to_block(outer_exit);
                    let out_len = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(out_len),
                        cl,
                    );
                    b.switch_to_block(cl);
                    elem_ty = pair_ty;
                    source = ChainSource::Vec {
                        local: out,
                        get_fn: out_get_fn,
                    };
                    len_local = out_len;
                }
                IterStep::RunningFold { init, closure } => {
                    let init_v = self.lower_expr(init)?;
                    let acc_ty = self.type_of_value(&init_v);
                    let (out_new_fn, out_push_fn) = Self::vec_fn_names_for_type(&acc_ty)?;
                    let (out_get_fn, _) = Self::vec_get_len_fn_names(&acc_ty)?;
                    let (out, acc, idx, cond_bb, exit_bb, el) = {
                        let b = self.current_fn.as_mut().unwrap();
                        let out = b.create_local(MirType::Vec(Box::new(acc_ty.clone())));
                        let c0 = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from(out_new_fn.as_str())),
                            vec![],
                            Some(out),
                            c0,
                        );
                        b.switch_to_block(c0);
                        let acc = b.create_local(acc_ty.clone());
                        b.assign(acc, MirRValue::Use(init_v));
                        let idx = b.create_local(MirType::i64());
                        b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                        let cond_bb = b.create_block();
                        let body_bb = b.create_block();
                        let exit_bb = b.create_block();
                        b.goto(cond_bb);
                        b.switch_to_block(cond_bb);
                        let cmp = b.create_local(MirType::Bool);
                        b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_local));
                        b.branch(values::local(cmp), body_bb, exit_bb);
                        b.switch_to_block(body_bb);
                        let el = read_source_elem(b, &source, idx, &elem_ty);
                        (out, acc, idx, cond_bb, exit_bb, el)
                    };
                    let next_acc = self.lower_iter_fold_inline(
                        closure,
                        values::local(acc),
                        values::local(el),
                    )?;
                    let out_len = {
                        let b = self.current_fn.as_mut().unwrap();
                        b.assign(acc, MirRValue::Use(next_acc));
                        let cp = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from(out_push_fn.as_str())),
                            vec![values::local(out), values::local(acc)],
                            None,
                            cp,
                        );
                        b.switch_to_block(cp);
                        let ni = b.create_local(MirType::i64());
                        b.binary_op(
                            ni,
                            BinOp::Add,
                            values::local(idx),
                            MirValue::Const(MirConst::Int(1, MirType::i64())),
                        );
                        b.assign(idx, MirRValue::Use(values::local(ni)));
                        b.goto(cond_bb);
                        b.switch_to_block(exit_bb);
                        let ol = b.create_local(MirType::i64());
                        let cl = b.create_block();
                        b.call(
                            MirValue::Function(Arc::from("build_hvec_len")),
                            vec![values::local(out)],
                            Some(ol),
                            cl,
                        );
                        b.switch_to_block(cl);
                        ol
                    };
                    elem_ty = acc_ty;
                    source = ChainSource::Vec {
                        local: out,
                        get_fn: out_get_fn,
                    };
                    len_local = out_len;
                }
                _ => {}
            }
        }

        // 1b. If the chain has `.zip(other)` step(s), materialize any leading
        // zips into a tuple Vec `Vec<(A, B)>` so arbitrary chained zips like
        // `a.iter().zip(b.iter()).zip(c.iter())` work, then lower the final
        // zip as `zipped_source` with length clamped to `min(len1, len2)`.
        let all_zip_others: Vec<&ast::Expr> = chain
            .steps
            .iter()
            .filter_map(|s| match s {
                IterStep::Zip { other } => Some(*other),
                _ => None,
            })
            .collect();
        fn unwrap_iter_src<'a>(other_expr: &'a ast::Expr) -> &'a ast::Expr {
            match &other_expr.kind {
                ExprKind::MethodCall {
                    receiver,
                    method,
                    args,
                    ..
                } if args.is_empty()
                    && matches!(method.name.as_ref(), "iter" | "iter_mut" | "into_iter") =>
                {
                    receiver.as_ref()
                }
                _ => other_expr,
            }
        }
        if all_zip_others.len() > 1 {
            for &other_expr in &all_zip_others[..all_zip_others.len() - 1] {
                let other_src = unwrap_iter_src(other_expr);
                let (elem2_ty, src2, len2_local) = match Self::range_source_bounds(other_src) {
                    Some((start, end, inclusive)) => {
                        self.lower_range_source(start, end, inclusive)?
                    }
                    None => self.lower_vec_source(other_src)?,
                };
                let pair_elems = vec![elem_ty.clone(), elem2_ty.clone()];
                self.ensure_tuple_type_def(&pair_elems);
                let pair_ty = MirType::Tuple(pair_elems);
                let (new_fn, push_fn) = Self::vec_fn_names_for_type(&pair_ty)?;
                let (get_fn, _) = Self::vec_get_len_fn_names(&pair_ty)?;
                let b = self.current_fn.as_mut().unwrap();
                let min_len = b.create_local(MirType::i64());
                let lt = b.create_local(MirType::Bool);
                b.binary_op(
                    lt,
                    BinOp::Lt,
                    values::local(len_local),
                    values::local(len2_local),
                );
                let b1 = b.create_block();
                let b2 = b.create_block();
                let bm = b.create_block();
                b.branch(values::local(lt), b1, b2);
                b.switch_to_block(b1);
                b.assign(min_len, MirRValue::Use(values::local(len_local)));
                b.goto(bm);
                b.switch_to_block(b2);
                b.assign(min_len, MirRValue::Use(values::local(len2_local)));
                b.goto(bm);
                b.switch_to_block(bm);
                let out = b.create_local(MirType::Vec(Box::new(pair_ty.clone())));
                let cn = b.create_block();
                b.call(MirValue::Function(Arc::from(new_fn)), vec![], Some(out), cn);
                b.switch_to_block(cn);
                let idx = b.create_local(MirType::i64());
                b.assign_const(idx, MirConst::Int(0, MirType::i64()));
                let cond_bb = b.create_block();
                let body_bb = b.create_block();
                let exit_bb = b.create_block();
                b.goto(cond_bb);
                b.switch_to_block(cond_bb);
                let cmp = b.create_local(MirType::Bool);
                b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(min_len));
                b.branch(values::local(cmp), body_bb, exit_bb);
                b.switch_to_block(body_bb);
                let e1 = read_source_elem(b, &source, idx, &elem_ty);
                let e2 = read_source_elem(b, &src2, idx, &elem2_ty);
                let tup = b.create_local(pair_ty.clone());
                b.assign(
                    tup,
                    MirRValue::Aggregate {
                        kind: AggregateKind::Tuple,
                        operands: vec![values::local(e1), values::local(e2)],
                    },
                );
                let cp = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(push_fn)),
                    vec![values::local(out), values::local(tup)],
                    None,
                    cp,
                );
                b.switch_to_block(cp);
                let ni = b.create_local(MirType::i64());
                b.binary_op(
                    ni,
                    BinOp::Add,
                    values::local(idx),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                b.assign(idx, MirRValue::Use(values::local(ni)));
                b.goto(cond_bb);
                b.switch_to_block(exit_bb);
                elem_ty = pair_ty;
                source = ChainSource::Vec { local: out, get_fn };
                len_local = min_len;
            }
        }
        let zip_other = all_zip_others.last().copied();
        let zipped_source: Option<(MirType, ChainSource)> = if let Some(other_expr) = zip_other {
            let other_src = unwrap_iter_src(other_expr);
            let (elem2_ty, src2, len2_local) = match Self::range_source_bounds(other_src) {
                Some((start, end, inclusive)) => self.lower_range_source(start, end, inclusive)?,
                None => self.lower_vec_source(other_src)?,
            };
            let builder = self.current_fn.as_mut().unwrap();
            let min_len = builder.create_local(MirType::i64());
            let lt = builder.create_local(MirType::Bool);
            builder.binary_op(
                lt,
                BinOp::Lt,
                values::local(len_local),
                values::local(len2_local),
            );
            let b_first = builder.create_block();
            let b_second = builder.create_block();
            let b_merge = builder.create_block();
            builder.branch(values::local(lt), b_first, b_second);
            builder.switch_to_block(b_first);
            builder.assign(min_len, MirRValue::Use(values::local(len_local)));
            builder.goto(b_merge);
            builder.switch_to_block(b_second);
            builder.assign(min_len, MirRValue::Use(values::local(len2_local)));
            builder.goto(b_merge);
            builder.switch_to_block(b_merge);
            len_local = min_len;
            Some((elem2_ty, src2))
        } else {
            None
        };

        // 3. Check for enumerate step - affects how many closure params we
        //    bind (index + element vs just element).
        let has_enumerate = chain.steps.iter().any(|s| matches!(s, IterStep::Enumerate));
        // `.rev()` iterates the source in reverse: start at len-1, step down,
        // exit when the (signed) index drops below 0.
        let reversed = chain.steps.iter().any(|s| matches!(s, IterStep::Rev));

        // 4. Determine result element type by walking the steps.
        //    After map closures, the type may change based on the closure's
        //    return type annotation.  For now we infer from the closure.
        let base_infer_ty = if let Some((ref elem2_ty, _)) = zipped_source {
            let pair_elems = vec![elem_ty.clone(), elem2_ty.clone()];
            self.ensure_tuple_type_def(&pair_elems);
            MirType::Tuple(pair_elems)
        } else {
            elem_ty.clone()
        };
        let mut output_elem_ty = self.infer_chain_output_type(&base_infer_ty, &chain.steps);
        // When the target binding annotates `let v: Vec<T> = ...collect()` and
        // the map closure has no explicit return annotation, use `T` directly.
        if let Some((ref mk, ref mv)) = collect_as_map {
            let pair = vec![mk.clone(), mv.clone()];
            self.ensure_tuple_type_def(&pair);
            output_elem_ty = MirType::Tuple(pair);
        } else if let Some(want) = &expected_collect {
            let annotated = chain.steps.iter().any(|s| {
                matches!(s, IterStep::Map { closure }
                    if matches!(&closure.kind, ExprKind::Closure { return_type: Some(_), .. }))
            });
            if !annotated {
                // `&str` is `BuildString` by value in the C backend, so do not
                // overwrite a `BuildString` element type with `*BuildString`.
                let want_is_str_ptr = matches!(
                    want,
                    MirType::Ptr(inner) if matches!(inner.as_ref(), MirType::Struct(n) if n.as_ref() == "BuildString")
                );
                let cur_is_str = matches!(
                    &output_elem_ty,
                    MirType::Struct(n) if n.as_ref() == "BuildString"
                );
                if !(want_is_str_ptr && cur_is_str) {
                    output_elem_ty = want.clone();
                }
            }
        }
        let (new_fn_name, push_fn_name) = if collect_as_map.is_some() {
            (String::new(), String::new())
        } else {
            Self::vec_fn_names_for_type(&output_elem_ty)?
        };

        // 5. Set up the result value depending on terminal type.
        let terminal_elem_ty = if has_enumerate
            && matches!(
                chain.terminal,
                IterTerminal::MinBy { .. }
                    | IterTerminal::MaxBy { .. }
                    | IterTerminal::MaxByKey { .. }
            ) {
            let pair = vec![MirType::usize(), output_elem_ty.clone()];
            self.ensure_tuple_type_def(&pair);
            MirType::Tuple(pair)
        } else {
            output_elem_ty.clone()
        };
        let collect_as_string = matches!(&chain.terminal, IterTerminal::Collect)
            && expected_collect.is_none()
            && collect_as_map.is_none()
            && (expected_collect_string
                || (matches!(output_elem_ty, MirType::Int(IntSize::I32, false))
                    && self.current_fn.as_ref().is_some_and(|b| {
                        matches!(b.return_type(), MirType::Struct(n) if n.as_ref() == "BuildString")
                    })));
        let mut nth_target_and_counter: Option<(MirValue, LocalId)> = None;
        let (result_local, _is_collect) = match &chain.terminal {
            IterTerminal::Collect if collect_as_map.is_some() => {
                let (mk, mv) = collect_as_map.as_ref().unwrap();
                let builder = self.current_fn.as_mut().unwrap();
                let map_ty = MirType::Map(Box::new(mk.clone()), Box::new(mv.clone()));
                let result = builder.create_local(map_ty);
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("build_hmap_new_str_f64")),
                    vec![],
                    Some(result),
                    cont,
                );
                builder.switch_to_block(cont);
                (result, true)
            }
            IterTerminal::Collect if collect_as_string => {
                let builder = self.current_fn.as_mut().unwrap();
                let str_ty = MirType::Struct(Arc::from("BuildString"));
                let result = builder.create_local(str_ty);
                let empty_idx = self.module.intern_string(String::new());
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("build_string_new")),
                    vec![MirValue::Const(MirConst::Str(empty_idx))],
                    Some(result),
                    cont,
                );
                builder.switch_to_block(cont);
                (result, true)
            }
            IterTerminal::Collect => {
                // Create the output vec.
                let builder = self.current_fn.as_mut().unwrap();
                let vec_ty = MirType::Vec(Box::new(output_elem_ty.clone()));
                let result = builder.create_local(vec_ty);
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from(new_fn_name)),
                    vec![],
                    Some(result),
                    cont,
                );
                builder.switch_to_block(cont);
                (result, true)
            }
            IterTerminal::Fold { init, .. } => {
                // Lower the initial accumulator value.
                let init_val = self.lower_expr(init)?;
                let init_ty = self.type_of_value(&init_val);
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(init_ty);
                builder.assign(acc, MirRValue::Use(init_val));
                (acc, false)
            }
            IterTerminal::Sum => {
                // `sum` adds numbers. Summing strings or structs has no meaning
                // here (Rust has no `Sum` for them either) and used to emit an
                // integer accumulator assigned to a string.
                if !matches!(output_elem_ty, MirType::Int(..) | MirType::Float(..)) {
                    return Err(CodegenError::Rejected {
                        message: format!(
                            "`sum` needs numeric elements, found `{}`",
                            output_elem_ty.to_string().replace("BuildString", "str")
                        ),
                        location: self
                            .source
                            .as_deref()
                            .and_then(|src| crate::codegen::c_verify::locate_call(src, "sum")),
                        help: Some(
                            "to join strings, loop over them and append each with `s = s + &x;`"
                                .to_string(),
                        ),
                    });
                }
                // Accumulator of the output element type, initialized to zero.
                let zero = match &output_elem_ty {
                    MirType::Float(_) => MirConst::Float(0.0, output_elem_ty.clone()),
                    _ => MirConst::Int(0, output_elem_ty.clone()),
                };
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(output_elem_ty.clone());
                builder.assign(acc, MirRValue::Use(MirValue::Const(zero)));
                (acc, false)
            }
            IterTerminal::Count => {
                // i64 counter initialized to zero; the element value is ignored.
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::i64());
                builder.assign(
                    acc,
                    MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                );
                (acc, false)
            }
            IterTerminal::Product => {
                // Accumulator of the output element type, initialized to one.
                let one = match &output_elem_ty {
                    MirType::Float(_) => MirConst::Float(1.0, output_elem_ty.clone()),
                    _ => MirConst::Int(1, output_elem_ty.clone()),
                };
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(output_elem_ty.clone());
                builder.assign(acc, MirRValue::Use(MirValue::Const(one)));
                (acc, false)
            }
            IterTerminal::Any { .. } => {
                // bool accumulator, false until a matching element is seen.
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Bool);
                builder.assign(acc, MirRValue::Use(MirValue::Const(MirConst::Bool(false))));
                (acc, false)
            }
            IterTerminal::All { .. } => {
                // bool accumulator, true until a non-matching element is seen.
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Bool);
                builder.assign(acc, MirRValue::Use(MirValue::Const(MirConst::Bool(true))));
                (acc, false)
            }
            IterTerminal::Max
            | IterTerminal::Min
            | IterTerminal::Last
            | IterTerminal::Reduce { .. } => {
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Option(Box::new(output_elem_ty.clone())));
                builder.assign(acc, MirRValue::Use(MirValue::Global(Arc::from("None"))));
                (acc, false)
            }
            IterTerminal::MinBy { .. }
            | IterTerminal::MaxBy { .. }
            | IterTerminal::MaxByKey { .. } => {
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Option(Box::new(terminal_elem_ty.clone())));
                builder.assign(acc, MirRValue::Use(MirValue::Global(Arc::from("None"))));
                (acc, false)
            }
            IterTerminal::Nth { index } => {
                let raw_target = self.lower_expr(index)?;
                let builder = self.current_fn.as_mut().unwrap();
                let target_i64 = builder.create_local(MirType::i64());
                builder.cast(target_i64, CastKind::IntToInt, raw_target, MirType::i64());
                let seen = builder.create_local(MirType::i64());
                builder.assign_const(seen, MirConst::Int(0, MirType::i64()));
                nth_target_and_counter = Some((values::local(target_i64), seen));
                let acc = builder.create_local(MirType::Option(Box::new(output_elem_ty.clone())));
                builder.assign(acc, MirRValue::Use(MirValue::Global(Arc::from("None"))));
                (acc, false)
            }
            IterTerminal::Position { .. } => {
                let builder = self.current_fn.as_mut().unwrap();
                let seen = builder.create_local(MirType::i64());
                builder.assign_const(seen, MirConst::Int(0, MirType::i64()));
                nth_target_and_counter =
                    Some((MirValue::Const(MirConst::Int(0, MirType::i64())), seen));
                let acc = builder.create_local(MirType::Option(Box::new(MirType::usize())));
                builder.assign(acc, MirRValue::Use(MirValue::Global(Arc::from("None"))));
                (acc, false)
            }
            IterTerminal::Find { .. } => {
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Option(Box::new(output_elem_ty.clone())));
                builder.assign(acc, MirRValue::Use(MirValue::Global(Arc::from("None"))));
                (acc, false)
            }
            IterTerminal::ForEach { .. } => {
                let builder = self.current_fn.as_mut().unwrap();
                let acc = builder.create_local(MirType::Void);
                (acc, false)
            }
        };

        // Per-step counters for take/skip/step_by, created before the loop so they
        // persist across iterations. Indexed by step position; None for steps
        // that don't need a counter. Counting yielded/skipped elements (rather
        // than checking the source index) makes take/skip/step_by compose correctly with
        // each other and with filter.
        let step_counters: Vec<Option<LocalId>> = {
            let builder = self.current_fn.as_mut().unwrap();
            chain
                .steps
                .iter()
                .map(|s| match s {
                    IterStep::Take { .. } | IterStep::Skip { .. } | IterStep::StepBy { .. } => {
                        let c = builder.create_local(MirType::i64());
                        builder.assign(
                            c,
                            MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                        );
                        Some(c)
                    }
                    _ => None,
                })
                .collect()
        };

        // 6. Create the loop: for i in 0..len { ... }
        let idx_local = {
            let builder = self.current_fn.as_mut().unwrap();
            let idx = builder.create_local(MirType::i64());
            if reversed {
                // idx = len - 1
                builder.binary_op(
                    idx,
                    BinOp::Sub,
                    values::local(len_local),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
            } else {
                builder.assign(
                    idx,
                    MirRValue::Use(MirValue::Const(MirConst::Int(0, MirType::i64()))),
                );
            }
            idx
        };

        let (cond_block, body_block, incr_block, exit_block) = {
            let builder = self.current_fn.as_mut().unwrap();
            let cond = builder.create_block();
            let body = builder.create_block();
            let incr = builder.create_block();
            let exit = builder.create_block();
            builder.goto(cond);
            builder.switch_to_block(cond);
            (cond, body, incr, exit)
        };

        // Condition: forward `idx < len`, reversed `idx >= 0`.
        {
            let builder = self.current_fn.as_mut().unwrap();
            let cmp = builder.create_local(MirType::Bool);
            if reversed {
                builder.binary_op(
                    cmp,
                    BinOp::Ge,
                    values::local(idx_local),
                    MirValue::Const(MirConst::Int(0, MirType::i64())),
                );
            } else {
                builder.binary_op(
                    cmp,
                    BinOp::Lt,
                    values::local(idx_local),
                    values::local(len_local),
                );
            }
            builder.branch(values::local(cmp), body_block, exit_block);
            builder.switch_to_block(body_block);
        }

        // 7. Loop body: get element (and zipped second element if present), apply transforms.
        let read_source_elem = |builder: &mut MirBuilder,
                                src: &ChainSource,
                                ety: &MirType,
                                idx: LocalId|
         -> LocalId {
            let elem = builder.create_local(ety.clone());
            match src {
                ChainSource::Vec { local, get_fn } => {
                    let cont = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from(get_fn.as_str())),
                        vec![values::local(*local), values::local(idx)],
                        Some(elem),
                        cont,
                    );
                    builder.switch_to_block(cont);
                }
                ChainSource::Indexed { local } => {
                    builder.assign(
                        elem,
                        MirRValue::IndexAccess {
                            base: values::local(*local),
                            index: values::local(idx),
                            elem_ty: ety.clone(),
                        },
                    );
                }
                ChainSource::Range { first } => {
                    let wide = builder.create_local(MirType::i64());
                    builder.binary_op(wide, BinOp::Add, values::local(*first), values::local(idx));
                    builder.cast(elem, CastKind::IntToInt, values::local(wide), ety.clone());
                }
                ChainSource::StringBytes { local } => {
                    let idx_u = builder.create_local(MirType::usize());
                    builder.cast(
                        idx_u,
                        CastKind::IntToInt,
                        values::local(idx),
                        MirType::usize(),
                    );
                    let raw = builder.create_local(MirType::i32());
                    let cont = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from("build_string_byte_at")),
                        vec![values::local(*local), values::local(idx_u)],
                        Some(raw),
                        cont,
                    );
                    builder.switch_to_block(cont);
                    builder.cast(elem, CastKind::IntToInt, values::local(raw), ety.clone());
                }
            }
            elem
        };
        let (elem_local, zip_elem_local) = {
            let builder = self.current_fn.as_mut().unwrap();
            let e1 = read_source_elem(builder, &source, &elem_ty, idx_local);
            let e2 = zipped_source
                .as_ref()
                .map(|(ety2, src2)| read_source_elem(builder, src2, ety2, idx_local));
            (e1, e2)
        };

        // Apply each step transform to produce the final value.
        let mut current_val = values::local(elem_local);
        let mut active_zip: Option<MirValue> = None;
        let mut flat_mapped = false;
        for (step_idx, step) in chain.steps.iter().enumerate() {
            match step {
                IterStep::FlatMap { closure } => {
                    if let ExprKind::Closure { params, body, .. } = &closure.kind {
                        let mut saved: Vec<(Arc<str>, Option<LocalId>)> = Vec::new();
                        if let Some(p0) = params.first() {
                            let pname = match &p0.pattern.kind {
                                ast::PatternKind::Ident { name, .. }
                                | ast::PatternKind::RefBind { name, .. } => Some(name.name.clone()),
                                _ => None,
                            };
                            if let Some(name) = pname {
                                if name.as_ref() != "_" {
                                    let old = self.var_map.get(&name).copied();
                                    saved.push((name.clone(), old));
                                    let pty = p0
                                        .ty
                                        .as_ref()
                                        .map(|t| self.lower_type_from_ast(t))
                                        .unwrap_or_else(|| self.type_of_value(&current_val));
                                    let b = self.current_fn.as_mut().unwrap();
                                    let loc = b.create_named_local(name.clone(), pty);
                                    b.assign(loc, MirRValue::Use(current_val.clone()));
                                    self.var_map.insert(name, loc);
                                }
                            }
                        }
                        // Unwrap a single-expression block body `{ inner_iter }`.
                        let mut inner_expr: &ast::Expr = body;
                        loop {
                            match &inner_expr.kind {
                                ExprKind::Paren(e) => inner_expr = e,
                                ExprKind::Block(blk) if blk.stmts.len() == 1 => {
                                    if let StmtKind::Expr(e) = &blk.stmts[0].kind {
                                        inner_expr = e;
                                    } else {
                                        break;
                                    }
                                }
                                _ => break,
                            }
                        }
                        let inner_vec = if let Some(inner_chain) =
                            Self::try_parse_iter_chain(inner_expr, "collect", &[])
                        {
                            let prev_exp = self
                                .expected_type
                                .replace(MirType::Vec(Box::new(output_elem_ty.clone())));
                            let res = self.lower_iter_chain(&inner_chain);
                            self.expected_type = prev_exp;
                            res?
                        } else {
                            self.lower_expr(body)?
                        };
                        for (name, old) in saved {
                            if let Some(id) = old {
                                self.var_map.insert(name, id);
                            } else {
                                self.var_map.remove(&name);
                            }
                        }
                        // Append all elements of `inner_vec` into `result_local` for Collect.
                        if matches!(chain.terminal, IterTerminal::Collect) {
                            let inner_ty = self.type_of_value(&inner_vec);
                            let inner_elem = match &inner_ty {
                                MirType::Vec(e) => (**e).clone(),
                                _ => output_elem_ty.clone(),
                            };
                            let (get_fn, len_fn) = Self::vec_get_len_fn_names(&inner_elem)?;
                            let b = self.current_fn.as_mut().unwrap();
                            let iv_loc = b.create_local(inner_ty);
                            b.assign(iv_loc, MirRValue::Use(inner_vec));
                            let ilen = b.create_local(MirType::i64());
                            let c1 = b.create_block();
                            b.call(
                                MirValue::Function(Arc::from(len_fn)),
                                vec![values::local(iv_loc)],
                                Some(ilen),
                                c1,
                            );
                            b.switch_to_block(c1);
                            let iidx = b.create_local(MirType::i64());
                            b.assign_const(iidx, MirConst::Int(0, MirType::i64()));
                            let icond = b.create_block();
                            let ibody = b.create_block();
                            let iexit = b.create_block();
                            b.goto(icond);
                            b.switch_to_block(icond);
                            let icmp = b.create_local(MirType::Bool);
                            b.binary_op(icmp, BinOp::Lt, values::local(iidx), values::local(ilen));
                            b.branch(values::local(icmp), ibody, iexit);
                            b.switch_to_block(ibody);
                            let ielem = b.create_local(inner_elem);
                            let c2 = b.create_block();
                            b.call(
                                MirValue::Function(Arc::from(get_fn.as_str())),
                                vec![values::local(iv_loc), values::local(iidx)],
                                Some(ielem),
                                c2,
                            );
                            b.switch_to_block(c2);
                            let c3 = b.create_block();
                            b.call(
                                MirValue::Function(Arc::from(push_fn_name.as_str())),
                                vec![values::local(result_local), values::local(ielem)],
                                None,
                                c3,
                            );
                            b.switch_to_block(c3);
                            let inext = b.create_local(MirType::i64());
                            b.binary_op(
                                inext,
                                BinOp::Add,
                                values::local(iidx),
                                MirValue::Const(MirConst::Int(1, MirType::i64())),
                            );
                            b.assign(iidx, MirRValue::Use(values::local(inext)));
                            b.goto(icond);
                            b.switch_to_block(iexit);
                            flat_mapped = true;
                        }
                    }
                }
                IterStep::Zip { .. } => {
                    active_zip = zip_elem_local.map(values::local);
                }
                IterStep::Chain { .. } => {
                    // Already concatenated into the loop source before iteration.
                }
                IterStep::Map { closure } => {
                    current_val = self.lower_iter_map_inline(
                        closure,
                        current_val,
                        if has_enumerate {
                            Some(values::local(idx_local))
                        } else {
                            None
                        },
                        active_zip.take(),
                    )?;
                }
                IterStep::FilterMap { closure } => {
                    let parse_hint = expected_collect
                        .as_ref()
                        .filter(|t| matches!(t, MirType::Int(..) | MirType::Float(..)))
                        .cloned();
                    let prev_parse =
                        parse_hint.map(|h| std::mem::replace(&mut self.parse_target_hint, Some(h)));
                    let prev_exp = {
                        let inner_exp = expected_collect
                            .clone()
                            .unwrap_or_else(|| output_elem_ty.clone());
                        Some(
                            self.expected_type
                                .replace(MirType::Option(Box::new(inner_exp))),
                        )
                    };
                    let opt_res = self.lower_iter_map_inline(
                        closure,
                        current_val,
                        if has_enumerate {
                            Some(values::local(idx_local))
                        } else {
                            None
                        },
                        active_zip.take(),
                    );
                    if let Some(p) = prev_exp {
                        self.expected_type = p;
                    }
                    if let Some(p) = prev_parse {
                        self.parse_target_hint = p;
                    }
                    let opt_val = opt_res?;
                    let opt_ty = self.type_of_value(&opt_val);
                    let inner_ty = Self::option_inner_from_mir_type(&opt_ty)
                        .or_else(|| expected_collect.clone())
                        .unwrap_or_else(|| output_elem_ty.clone());
                    let builder = self.current_fn.as_mut().unwrap();
                    let opt_loc = builder.create_local(opt_ty);
                    builder.assign(opt_loc, MirRValue::Use(opt_val));
                    let hv = builder.create_local(MirType::Bool);
                    builder.assign(
                        hv,
                        MirRValue::FieldAccess {
                            base: values::local(opt_loc),
                            field_name: Arc::from("has_value"),
                            field_ty: MirType::Bool,
                        },
                    );
                    let keep_bb = builder.create_block();
                    builder.branch(values::local(hv), keep_bb, incr_block);
                    builder.switch_to_block(keep_bb);
                    let payload = builder.create_local(inner_ty.clone());
                    builder.assign(
                        payload,
                        MirRValue::FieldAccess {
                            base: values::local(opt_loc),
                            field_name: Arc::from("value"),
                            field_ty: inner_ty,
                        },
                    );
                    current_val = values::local(payload);
                }
                IterStep::Filter { closure } => {
                    // Evaluate the predicate on the current element; if it does
                    // not hold, skip straight to the increment block (drop this
                    // element from the rest of the pipeline and the terminal).
                    let keep = self.lower_iter_map_inline(
                        closure,
                        current_val.clone(),
                        if has_enumerate {
                            Some(values::local(idx_local))
                        } else {
                            None
                        },
                        active_zip.clone(),
                    )?;
                    let builder = self.current_fn.as_mut().unwrap();
                    let keep_block = builder.create_block();
                    builder.branch(keep, keep_block, incr_block);
                    builder.switch_to_block(keep_block);
                }
                IterStep::Enumerate => {
                    // enumerate doesn't change the value; it just means
                    // subsequent map closures get (index, elem).  The index
                    // is passed via the idx_local when lowering map closures.
                }
                IterStep::Cloned => {
                    // No-op for Copy types.
                }
                IterStep::Rev => {
                    // Direction is handled by the loop bounds; per-element no-op.
                }
                IterStep::Skip { count } => {
                    // skip(n): while the skip counter < n, increment it and drop
                    // the element (goto increment); afterwards pass through. The
                    // counter (not the source index) makes this compose with
                    // filter and other skips.
                    let n_val = self.lower_expr(count)?;
                    let ctr = step_counters[step_idx].expect("skip counter");
                    let builder = self.current_fn.as_mut().unwrap();
                    let skipping = builder.create_local(MirType::Bool);
                    builder.binary_op(skipping, BinOp::Lt, values::local(ctr), n_val);
                    let skip_block = builder.create_block();
                    let pass_block = builder.create_block();
                    builder.branch(values::local(skipping), skip_block, pass_block);
                    // Skip: counter += 1, drop this element.
                    builder.switch_to_block(skip_block);
                    let nc = builder.create_local(MirType::i64());
                    builder.binary_op(
                        nc,
                        BinOp::Add,
                        values::local(ctr),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    builder.assign(ctr, MirRValue::Use(values::local(nc)));
                    builder.goto(incr_block);
                    builder.switch_to_block(pass_block);
                }
                IterStep::StepBy { step } => {
                    // step_by(n): yield when (ctr % n) == 0, increment ctr every
                    // element so it composes cleanly after skip/filter.
                    let step_raw = self.lower_expr(step)?;
                    let ctr = step_counters[step_idx].expect("step_by counter");
                    let builder = self.current_fn.as_mut().unwrap();
                    let step_i64 = builder.create_local(MirType::i64());
                    builder.cast(step_i64, CastKind::IntToInt, step_raw, MirType::i64());
                    let cur_ctr = builder.create_local(MirType::i64());
                    builder.assign(cur_ctr, MirRValue::Use(values::local(ctr)));
                    let nc = builder.create_local(MirType::i64());
                    builder.binary_op(
                        nc,
                        BinOp::Add,
                        values::local(ctr),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    builder.assign(ctr, MirRValue::Use(values::local(nc)));
                    let rem = builder.create_local(MirType::i64());
                    builder.binary_op(
                        rem,
                        BinOp::Rem,
                        values::local(cur_ctr),
                        values::local(step_i64),
                    );
                    let take_it = builder.create_local(MirType::Bool);
                    builder.binary_op(
                        take_it,
                        BinOp::Eq,
                        values::local(rem),
                        MirValue::Const(MirConst::Int(0, MirType::i64())),
                    );
                    let pass_block = builder.create_block();
                    builder.branch(values::local(take_it), pass_block, incr_block);
                    builder.switch_to_block(pass_block);
                }
                IterStep::Take { count } => {
                    // take(n): once the take counter reaches n, exit the loop;
                    // otherwise increment it and pass the element through. The
                    // counter (not the source index) makes this compose.
                    let n_val = self.lower_expr(count)?;
                    let ctr = step_counters[step_idx].expect("take counter");
                    let builder = self.current_fn.as_mut().unwrap();
                    let done = builder.create_local(MirType::Bool);
                    builder.binary_op(done, BinOp::Ge, values::local(ctr), n_val);
                    let take_block = builder.create_block();
                    builder.branch(values::local(done), exit_block, take_block);
                    // Within the limit: counter += 1, pass through.
                    builder.switch_to_block(take_block);
                    let nc = builder.create_local(MirType::i64());
                    builder.binary_op(
                        nc,
                        BinOp::Add,
                        values::local(ctr),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    builder.assign(ctr, MirRValue::Use(values::local(nc)));
                }
                IterStep::TakeWhile { closure } => {
                    let keep = self.lower_iter_map_inline(
                        closure,
                        current_val.clone(),
                        if has_enumerate {
                            Some(values::local(idx_local))
                        } else {
                            None
                        },
                        active_zip.clone(),
                    )?;
                    let builder = self.current_fn.as_mut().unwrap();
                    let keep_block = builder.create_block();
                    builder.branch(keep, keep_block, exit_block);
                    builder.switch_to_block(keep_block);
                }
                IterStep::GroupBy { .. }
                | IterStep::Chunks { .. }
                | IterStep::Windows { .. }
                | IterStep::Dedup
                | IterStep::Unique
                | IterStep::Interleave { .. }
                | IterStep::Merge { .. }
                | IterStep::Intersperse { .. }
                | IterStep::CartesianProduct { .. }
                | IterStep::RunningFold { .. } => {
                    // Already materialized into a Vec source before the main loop.
                }
            }
        }

        // 8. Terminal: push to result vec OR update accumulator.
        match &chain.terminal {
            IterTerminal::Collect if collect_as_map.is_some() => {
                let (mk, mv) = collect_as_map.as_ref().unwrap();
                let pair_ty = self.type_of_value(&current_val);
                let builder = self.current_fn.as_mut().unwrap();
                let pair_loc = builder.create_local(pair_ty);
                builder.assign(pair_loc, MirRValue::Use(current_val));
                let k_loc = builder.create_local(mk.clone());
                builder.assign(
                    k_loc,
                    MirRValue::FieldAccess {
                        base: values::local(pair_loc),
                        field_name: Arc::from("_0"),
                        field_ty: mk.clone(),
                    },
                );
                let v_loc = builder.create_local(mv.clone());
                builder.assign(
                    v_loc,
                    MirRValue::FieldAccess {
                        base: values::local(pair_loc),
                        field_name: Arc::from("_1"),
                        field_ty: mv.clone(),
                    },
                );
                let cont = builder.create_block();
                if matches!(mv, MirType::Float(_)) {
                    let kp = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                    builder.assign(
                        kp,
                        MirRValue::FieldAccess {
                            base: values::local(k_loc),
                            field_name: Arc::from("ptr"),
                            field_ty: MirType::Ptr(Box::new(MirType::i8())),
                        },
                    );
                    builder.call(
                        MirValue::Function(Arc::from("build_hmap_insert_str_f64")),
                        vec![
                            values::local(result_local),
                            values::local(kp),
                            values::local(v_loc),
                        ],
                        None,
                        cont,
                    );
                } else {
                    let ins_fn = format!("build_hmap_insert_val_{}", map_value_mangle(mv));
                    builder.call(
                        MirValue::Function(Arc::from(ins_fn.as_str())),
                        vec![
                            values::local(result_local),
                            values::local(k_loc),
                            values::local(v_loc),
                        ],
                        None,
                        cont,
                    );
                }
                builder.switch_to_block(cont);
            }
            IterTerminal::Collect if collect_as_string => {
                let val_ty = self.type_of_value(&current_val);
                let builder = self.current_fn.as_mut().unwrap();
                let ch_arg = if matches!(val_ty, MirType::Int(IntSize::I32, false)) {
                    current_val
                } else {
                    let c_u32 = builder.create_local(MirType::u32());
                    builder.cast(c_u32, CastKind::IntToInt, current_val, MirType::u32());
                    values::local(c_u32)
                };
                let ch_s = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                let c1 = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("build_char_to_string")),
                    vec![ch_arg],
                    Some(ch_s),
                    c1,
                );
                builder.switch_to_block(c1);
                let next_s = builder.create_local(MirType::Struct(Arc::from("BuildString")));
                let c2 = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("build_string_concat")),
                    vec![values::local(result_local), values::local(ch_s)],
                    Some(next_s),
                    c2,
                );
                builder.switch_to_block(c2);
                builder.assign(result_local, MirRValue::Use(values::local(next_s)));
            }
            IterTerminal::Collect => {
                if !flat_mapped {
                    let push_val = if let Some(v2) = active_zip.take() {
                        let t1 = self.type_of_value(&current_val);
                        let t2 = self.type_of_value(&v2);
                        let elems = vec![t1, t2];
                        self.ensure_tuple_type_def(&elems);
                        let builder = self.current_fn.as_mut().unwrap();
                        let tup = builder.create_local(MirType::Tuple(elems));
                        builder.assign(
                            tup,
                            MirRValue::Aggregate {
                                kind: AggregateKind::Tuple,
                                operands: vec![current_val, v2],
                            },
                        );
                        values::local(tup)
                    } else {
                        current_val
                    };
                    let builder = self.current_fn.as_mut().unwrap();
                    let cont = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from(push_fn_name)),
                        vec![values::local(result_local), push_val],
                        None,
                        cont,
                    );
                    builder.switch_to_block(cont);
                }
            }
            IterTerminal::Last => {
                let builder = self.current_fn.as_mut().unwrap();
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![current_val],
                    Some(result_local),
                    cont,
                );
                builder.switch_to_block(cont);
            }
            IterTerminal::Fold { closure, .. } => {
                // Inline the fold closure: acc = closure(acc, elem_or_zipped_pair)
                let fold_elem = if let Some(v2) = active_zip.take() {
                    let t1 = self.type_of_value(&current_val);
                    let t2 = self.type_of_value(&v2);
                    let elems = vec![t1, t2];
                    self.ensure_tuple_type_def(&elems);
                    let builder = self.current_fn.as_mut().unwrap();
                    let tup = builder.create_local(MirType::Tuple(elems));
                    builder.assign(
                        tup,
                        MirRValue::Aggregate {
                            kind: AggregateKind::Tuple,
                            operands: vec![current_val, v2],
                        },
                    );
                    values::local(tup)
                } else {
                    current_val
                };
                let new_acc =
                    self.lower_iter_fold_inline(closure, values::local(result_local), fold_elem)?;
                let builder = self.current_fn.as_mut().unwrap();
                builder.assign(result_local, MirRValue::Use(new_acc));
            }
            IterTerminal::Sum => {
                // acc = acc + current_val
                let val_ty = self.type_of_value(&current_val);
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(val_ty);
                builder.binary_op(next, BinOp::Add, values::local(result_local), current_val);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::Count => {
                // acc = acc + 1 (the element value is unused)
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(MirType::i64());
                builder.binary_op(
                    next,
                    BinOp::Add,
                    values::local(result_local),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::Product => {
                // acc = acc * current_val
                let val_ty = self.type_of_value(&current_val);
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(val_ty);
                builder.binary_op(next, BinOp::Mul, values::local(result_local), current_val);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::Any { closure } => {
                // acc = acc || pred(elem)  (bitwise-or on bool is correct here)
                let pred = self.lower_iter_map_inline(
                    closure,
                    current_val,
                    if has_enumerate {
                        Some(values::local(idx_local))
                    } else {
                        None
                    },
                    active_zip.clone(),
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(MirType::Bool);
                builder.binary_op(next, BinOp::BitOr, values::local(result_local), pred);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::All { closure } => {
                // acc = acc && pred(elem)  (bitwise-and on bool is correct here)
                let pred = self.lower_iter_map_inline(
                    closure,
                    current_val,
                    if has_enumerate {
                        Some(values::local(idx_local))
                    } else {
                        None
                    },
                    active_zip.clone(),
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let next = builder.create_local(MirType::Bool);
                builder.binary_op(next, BinOp::BitAnd, values::local(result_local), pred);
                builder.assign(result_local, MirRValue::Use(values::local(next)));
            }
            IterTerminal::Max | IterTerminal::Min => {
                let is_max = matches!(&chain.terminal, IterTerminal::Max);
                let builder = self.current_fn.as_mut().unwrap();
                let hv = builder.create_local(MirType::Bool);
                builder.assign(
                    hv,
                    MirRValue::FieldAccess {
                        base: values::local(result_local),
                        field_name: Arc::from("has_value"),
                        field_ty: MirType::Bool,
                    },
                );
                let has_bb = builder.create_block();
                let set_bb = builder.create_block();
                let done_bb = builder.create_block();
                builder.branch(values::local(hv), has_bb, set_bb);

                builder.switch_to_block(has_bb);
                let prev = builder.create_local(output_elem_ty.clone());
                builder.assign(
                    prev,
                    MirRValue::FieldAccess {
                        base: values::local(result_local),
                        field_name: Arc::from("value"),
                        field_ty: output_elem_ty.clone(),
                    },
                );
                let better = builder.create_local(MirType::Bool);
                builder.binary_op(
                    better,
                    if is_max { BinOp::Gt } else { BinOp::Lt },
                    current_val.clone(),
                    values::local(prev),
                );
                builder.branch(values::local(better), set_bb, done_bb);

                builder.switch_to_block(set_bb);
                let after_some = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![current_val],
                    Some(result_local),
                    after_some,
                );
                builder.switch_to_block(after_some);
                builder.goto(done_bb);

                builder.switch_to_block(done_bb);
            }
            IterTerminal::Position { closure } => {
                let (_, seen_local) = nth_target_and_counter.clone().expect("position counter");
                let pred = self.lower_iter_map_inline(
                    closure,
                    current_val,
                    if has_enumerate {
                        Some(values::local(idx_local))
                    } else {
                        None
                    },
                    active_zip.clone(),
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let hit_bb = builder.create_block();
                let miss_bb = builder.create_block();
                builder.branch(pred, hit_bb, miss_bb);
                builder.switch_to_block(hit_bb);
                let pos_u = builder.create_local(MirType::usize());
                builder.cast(
                    pos_u,
                    CastKind::IntToInt,
                    values::local(seen_local),
                    MirType::usize(),
                );
                let after_some = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![values::local(pos_u)],
                    Some(result_local),
                    after_some,
                );
                builder.switch_to_block(after_some);
                builder.goto(exit_block);
                builder.switch_to_block(miss_bb);
                let next_seen = builder.create_local(MirType::i64());
                builder.binary_op(
                    next_seen,
                    BinOp::Add,
                    values::local(seen_local),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                builder.assign(seen_local, MirRValue::Use(values::local(next_seen)));
            }
            IterTerminal::Nth { .. } => {
                let (target_val, seen_local) = nth_target_and_counter
                    .clone()
                    .expect("nth target and counter");
                let builder = self.current_fn.as_mut().unwrap();
                let is_match = builder.create_local(MirType::Bool);
                builder.binary_op(is_match, BinOp::Eq, values::local(seen_local), target_val);
                let hit_bb = builder.create_block();
                let miss_bb = builder.create_block();
                builder.branch(values::local(is_match), hit_bb, miss_bb);
                builder.switch_to_block(hit_bb);
                let after_some = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![current_val],
                    Some(result_local),
                    after_some,
                );
                builder.switch_to_block(after_some);
                builder.goto(exit_block);
                builder.switch_to_block(miss_bb);
                let next_seen = builder.create_local(MirType::i64());
                builder.binary_op(
                    next_seen,
                    BinOp::Add,
                    values::local(seen_local),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                builder.assign(seen_local, MirRValue::Use(values::local(next_seen)));
            }
            IterTerminal::Reduce { closure } => {
                let (has_bb, set_first_bb, done_bb, prev_local) = {
                    let builder = self.current_fn.as_mut().unwrap();
                    let hv = builder.create_local(MirType::Bool);
                    builder.assign(
                        hv,
                        MirRValue::FieldAccess {
                            base: values::local(result_local),
                            field_name: Arc::from("has_value"),
                            field_ty: MirType::Bool,
                        },
                    );
                    let has_bb = builder.create_block();
                    let set_first_bb = builder.create_block();
                    let done_bb = builder.create_block();
                    builder.branch(values::local(hv), has_bb, set_first_bb);
                    builder.switch_to_block(has_bb);
                    let prev = builder.create_local(output_elem_ty.clone());
                    builder.assign(
                        prev,
                        MirRValue::FieldAccess {
                            base: values::local(result_local),
                            field_name: Arc::from("value"),
                            field_ty: output_elem_ty.clone(),
                        },
                    );
                    (has_bb, set_first_bb, done_bb, prev)
                };
                let _ = has_bb;
                let reduced = self.lower_iter_fold_inline(
                    closure,
                    values::local(prev_local),
                    current_val.clone(),
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let after_red = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![reduced],
                    Some(result_local),
                    after_red,
                );
                builder.switch_to_block(after_red);
                builder.goto(done_bb);

                builder.switch_to_block(set_first_bb);
                let after_first = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![current_val],
                    Some(result_local),
                    after_first,
                );
                builder.switch_to_block(after_first);
                builder.goto(done_bb);

                builder.switch_to_block(done_bb);
            }
            IterTerminal::MinBy { closure } | IterTerminal::MaxBy { closure } => {
                let is_max = matches!(&chain.terminal, IterTerminal::MaxBy { .. });
                let item_val = if has_enumerate {
                    let builder = self.current_fn.as_mut().unwrap();
                    let idx_u = builder.create_local(MirType::usize());
                    builder.cast(
                        idx_u,
                        CastKind::IntToInt,
                        values::local(idx_local),
                        MirType::usize(),
                    );
                    let tup = builder.create_local(terminal_elem_ty.clone());
                    builder.assign(
                        tup,
                        MirRValue::Aggregate {
                            kind: AggregateKind::Tuple,
                            operands: vec![values::local(idx_u), current_val],
                        },
                    );
                    values::local(tup)
                } else {
                    current_val
                };
                let (set_bb, done_bb, prev_local) = {
                    let builder = self.current_fn.as_mut().unwrap();
                    let hv = builder.create_local(MirType::Bool);
                    builder.assign(
                        hv,
                        MirRValue::FieldAccess {
                            base: values::local(result_local),
                            field_name: Arc::from("has_value"),
                            field_ty: MirType::Bool,
                        },
                    );
                    let has_bb = builder.create_block();
                    let set_bb = builder.create_block();
                    let done_bb = builder.create_block();
                    builder.branch(values::local(hv), has_bb, set_bb);
                    builder.switch_to_block(has_bb);
                    let prev = builder.create_local(terminal_elem_ty.clone());
                    builder.assign(
                        prev,
                        MirRValue::FieldAccess {
                            base: values::local(result_local),
                            field_name: Arc::from("value"),
                            field_ty: terminal_elem_ty.clone(),
                        },
                    );
                    (set_bb, done_bb, prev)
                };
                let ord = self.lower_iter_fold_inline(
                    closure,
                    item_val.clone(),
                    values::local(prev_local),
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let better = builder.create_local(MirType::Bool);
                builder.binary_op(
                    better,
                    if is_max { BinOp::Gt } else { BinOp::Lt },
                    ord,
                    MirValue::Const(MirConst::Int(0, MirType::i32())),
                );
                builder.branch(values::local(better), set_bb, done_bb);

                builder.switch_to_block(set_bb);
                let after_some = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![item_val],
                    Some(result_local),
                    after_some,
                );
                builder.switch_to_block(after_some);
                builder.goto(done_bb);

                builder.switch_to_block(done_bb);
            }
            IterTerminal::MaxByKey { closure } => {
                let item_val = if has_enumerate {
                    let builder = self.current_fn.as_mut().unwrap();
                    let idx_u = builder.create_local(MirType::usize());
                    builder.cast(
                        idx_u,
                        CastKind::IntToInt,
                        values::local(idx_local),
                        MirType::usize(),
                    );
                    let tup = builder.create_local(terminal_elem_ty.clone());
                    builder.assign(
                        tup,
                        MirRValue::Aggregate {
                            kind: AggregateKind::Tuple,
                            operands: vec![values::local(idx_u), current_val],
                        },
                    );
                    values::local(tup)
                } else {
                    current_val
                };
                let cur_key = self.lower_iter_map_inline(closure, item_val.clone(), None, None)?;
                let key_ty = self.type_of_value(&cur_key);
                let builder = self.current_fn.as_mut().unwrap();
                let best_key = builder.create_local(key_ty);
                let hv = builder.create_local(MirType::Bool);
                builder.assign(
                    hv,
                    MirRValue::FieldAccess {
                        base: values::local(result_local),
                        field_name: Arc::from("has_value"),
                        field_ty: MirType::Bool,
                    },
                );
                let has_bb = builder.create_block();
                let set_bb = builder.create_block();
                let done_bb = builder.create_block();
                builder.branch(values::local(hv), has_bb, set_bb);

                builder.switch_to_block(has_bb);
                let better = builder.create_local(MirType::Bool);
                builder.binary_op(better, BinOp::Gt, cur_key.clone(), values::local(best_key));
                builder.branch(values::local(better), set_bb, done_bb);

                builder.switch_to_block(set_bb);
                builder.assign(best_key, MirRValue::Use(cur_key));
                let after_some = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![item_val],
                    Some(result_local),
                    after_some,
                );
                builder.switch_to_block(after_some);
                builder.goto(done_bb);

                builder.switch_to_block(done_bb);
            }
            IterTerminal::Find { closure } => {
                let pred = self.lower_iter_map_inline(
                    closure,
                    current_val.clone(),
                    if has_enumerate {
                        Some(values::local(idx_local))
                    } else {
                        None
                    },
                    active_zip.clone(),
                )?;
                let builder = self.current_fn.as_mut().unwrap();
                let hit_bb = builder.create_block();
                let miss_bb = builder.create_block();
                builder.branch(pred, hit_bb, miss_bb);
                builder.switch_to_block(hit_bb);
                let after_some = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("Some")),
                    vec![current_val],
                    Some(result_local),
                    after_some,
                );
                builder.switch_to_block(after_some);
                builder.goto(exit_block);
                builder.switch_to_block(miss_bb);
            }
            IterTerminal::ForEach { closure } => {
                let _ = self.lower_iter_map_inline(
                    closure,
                    current_val,
                    if has_enumerate {
                        Some(values::local(idx_local))
                    } else {
                        None
                    },
                    active_zip.clone(),
                )?;
            }
        }

        // 9. Increment and loop back.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.goto(incr_block);
            builder.switch_to_block(incr_block);
            let next_idx = builder.create_local(MirType::i64());
            builder.binary_op(
                next_idx,
                if reversed { BinOp::Sub } else { BinOp::Add },
                values::local(idx_local),
                MirValue::Const(MirConst::Int(1, MirType::i64())),
            );
            builder.assign(idx_local, MirRValue::Use(values::local(next_idx)));
            builder.goto(cond_block);
        }

        // 10. Switch to exit block and return the result.
        {
            let builder = self.current_fn.as_mut().unwrap();
            builder.switch_to_block(exit_block);
        }

        Ok(values::local(result_local))
    }

    /// The bounds of `a..b` or `a..=b` (through parentheses), when both are
    /// written: the only ranges an iterator chain can start from.
    pub(super) fn range_source_bounds(expr: &ast::Expr) -> Option<(&ast::Expr, &ast::Expr, bool)> {
        match &expr.kind {
            ExprKind::Paren(inner) => Self::range_source_bounds(inner),
            ExprKind::Range {
                start: Some(start),
                end: Some(end),
                inclusive,
            } => Some((start, end, *inclusive)),
            ExprKind::Binary {
                op: ast::BinOp::Range,
                left,
                right,
            } => Some((left, right, false)),
            ExprKind::Binary {
                op: ast::BinOp::RangeInclusive,
                left,
                right,
            } => Some((left, right, true)),
            _ => None,
        }
    }

    /// A vector as the source of a chain: its element type, the local that
    /// holds it with its accessor, and its length.
    fn lower_vec_source(
        &mut self,
        source_expr: &ast::Expr,
    ) -> CodegenResult<(MirType, ChainSource, LocalId)> {
        // `successors(first, |prev| next)` as an iterator chain source:
        // generate up to 64 elements while `Option` is `Some`.
        if let ExprKind::Call { func, args } = &source_expr.kind {
            if args.len() == 2
                && matches!(
                    &func.kind,
                    ExprKind::Ident(id) if id.name.as_ref() == "successors"
                )
                || (args.len() == 2
                    && matches!(
                        &func.kind,
                        ExprKind::Path(p)
                            if p.last_ident()
                                .is_some_and(|i| i.name.as_ref() == "successors")
                    ))
            {
                let seed = self.lower_expr(&args[0])?;
                let seed_ty = self.type_of_value(&seed);
                let elem_ty = Self::option_inner_from_mir_type(&seed_ty)
                    .or_else(|| {
                        if let MirValue::Local(id) = &seed {
                            self.option_inner_types.get(id).cloned()
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(MirType::i32);
                if let MirType::Tuple(ref elems) = elem_ty {
                    self.ensure_tuple_type_def(elems);
                }
                let opt_ty = MirType::Option(Box::new(elem_ty.clone()));
                let (new_fn, push_fn) = Self::vec_fn_names_for_type(&elem_ty)?;
                let (get_fn, _) = Self::vec_get_len_fn_names(&elem_ty)?;
                let (out, cur, cnt, cond_bb, body_bb, exit_bb, val_l) = {
                    let b = self.current_fn.as_mut().unwrap();
                    let out = b.create_local(MirType::Vec(Box::new(elem_ty.clone())));
                    let c0 = b.create_block();
                    b.call(MirValue::Function(Arc::from(new_fn)), vec![], Some(out), c0);
                    b.switch_to_block(c0);
                    let cur = b.create_local(opt_ty);
                    b.assign(cur, MirRValue::Use(seed));
                    let cnt = b.create_local(MirType::i64());
                    b.assign_const(cnt, MirConst::Int(0, MirType::i64()));
                    let cond_bb = b.create_block();
                    let check_opt_bb = b.create_block();
                    let body_bb = b.create_block();
                    let exit_bb = b.create_block();
                    b.goto(cond_bb);
                    b.switch_to_block(cond_bb);
                    let under = b.create_local(MirType::Bool);
                    b.binary_op(
                        under,
                        BinOp::Lt,
                        values::local(cnt),
                        MirValue::Const(MirConst::Int(64, MirType::i64())),
                    );
                    b.branch(values::local(under), check_opt_bb, exit_bb);
                    b.switch_to_block(check_opt_bb);
                    let hv = b.create_local(MirType::Bool);
                    b.assign(
                        hv,
                        MirRValue::FieldAccess {
                            base: values::local(cur),
                            field_name: Arc::from("has_value"),
                            field_ty: MirType::Bool,
                        },
                    );
                    b.branch(values::local(hv), body_bb, exit_bb);
                    b.switch_to_block(body_bb);
                    let val_l = b.create_local(elem_ty.clone());
                    b.assign(
                        val_l,
                        MirRValue::FieldAccess {
                            base: values::local(cur),
                            field_name: Arc::from("value"),
                            field_ty: elem_ty.clone(),
                        },
                    );
                    let cp = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn)),
                        vec![values::local(out), values::local(val_l)],
                        None,
                        cp,
                    );
                    b.switch_to_block(cp);
                    (out, cur, cnt, cond_bb, body_bb, exit_bb, val_l)
                };
                let _ = body_bb;
                let next_opt =
                    self.lower_iter_map_inline(&args[1], values::local(val_l), None, None)?;
                let len_l = {
                    let b = self.current_fn.as_mut().unwrap();
                    b.assign(cur, MirRValue::Use(next_opt));
                    let nc = b.create_local(MirType::i64());
                    b.binary_op(
                        nc,
                        BinOp::Add,
                        values::local(cnt),
                        MirValue::Const(MirConst::Int(1, MirType::i64())),
                    );
                    b.assign(cnt, MirRValue::Use(values::local(nc)));
                    b.goto(cond_bb);
                    b.switch_to_block(exit_bb);
                    let ll = b.create_local(MirType::i64());
                    let cl = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(ll),
                        cl,
                    );
                    b.switch_to_block(cl);
                    ll
                };
                return Ok((elem_ty, ChainSource::Vec { local: out, get_fn }, len_l));
            }
        }
        // `std::iter::repeat(val)` as an iterator chain source: bounded by `.take(n)`
        // (we materialize a virtual repeating source; `lower_iter_chain`'s `.take(n)`
        // exits after `n` elements).
        if let ExprKind::Call { func, args } = &source_expr.kind {
            if args.len() == 1
                && matches!(&func.kind, ExprKind::Path(p)
                    if p.last_ident().is_some_and(|i| i.name.as_ref() == "repeat")
                        && p.segments.iter().any(|s| s.ident.name.as_ref() == "iter"))
            {
                let v = self.lower_expr(&args[0])?;
                let v_ty = self.type_of_value(&v);
                let (new_fn, push_fn) = Self::vec_fn_names_for_type(&v_ty)?;
                let b = self.current_fn.as_mut().unwrap();
                let out = b.create_local(MirType::Vec(Box::new(v_ty.clone())));
                let c0 = b.create_block();
                b.call(MirValue::Function(Arc::from(new_fn)), vec![], Some(out), c0);
                b.switch_to_block(c0);
                // Seed up to 1024 copies (any `.take(n)` exits at `n`).
                let cap = b.create_local(MirType::i64());
                b.assign_const(cap, MirConst::Int(1024, MirType::i64()));
                let i = b.create_local(MirType::i64());
                b.assign_const(i, MirConst::Int(0, MirType::i64()));
                let cond_bb = b.create_block();
                let body_bb = b.create_block();
                let exit_bb = b.create_block();
                b.goto(cond_bb);
                b.switch_to_block(cond_bb);
                let cmp = b.create_local(MirType::Bool);
                b.binary_op(cmp, BinOp::Lt, values::local(i), values::local(cap));
                b.branch(values::local(cmp), body_bb, exit_bb);
                b.switch_to_block(body_bb);
                let cp = b.create_block();
                b.call(
                    MirValue::Function(Arc::from(push_fn)),
                    vec![values::local(out), v],
                    None,
                    cp,
                );
                b.switch_to_block(cp);
                let ni = b.create_local(MirType::i64());
                b.binary_op(
                    ni,
                    BinOp::Add,
                    values::local(i),
                    MirValue::Const(MirConst::Int(1, MirType::i64())),
                );
                b.assign(i, MirRValue::Use(values::local(ni)));
                b.goto(cond_bb);
                b.switch_to_block(exit_bb);
                let (get_fn, _) = Self::vec_get_len_fn_names(&v_ty)?;
                return Ok((v_ty, ChainSource::Vec { local: out, get_fn }, cap));
            }
        }
        // `s.chars()` as an iterator chain source: materialize UTF-8 code points
        // into `Vec<u32>` using the existing `BuildCharIter` runtime helpers.
        if let ExprKind::MethodCall {
            receiver: str_recv,
            method,
            args,
            ..
        } = &source_expr.kind
        {
            if method.name.as_ref() == "chars" && args.is_empty() {
                let s_raw = self.lower_expr(str_recv)?;
                let s_val = self.deref_if_pointer(s_raw)?;
                if matches!(self.type_of_value(&s_val), MirType::Struct(ref n) if n.as_ref() == "BuildString")
                {
                    let b = self.current_fn.as_mut().unwrap();
                    let it = b.create_local(MirType::Struct(Arc::from("BuildCharIter")));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_char_iter_new")),
                        vec![s_val],
                        Some(it),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let out = b.create_local(MirType::Vec(Box::new(MirType::u32())));
                    let c1 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_new_u32")),
                        vec![],
                        Some(out),
                        c1,
                    );
                    b.switch_to_block(c1);
                    let cond_bb = b.create_block();
                    let body_bb = b.create_block();
                    let exit_bb = b.create_block();
                    b.goto(cond_bb);
                    b.switch_to_block(cond_bb);
                    let it_ptr = b.create_local(MirType::Ptr(Box::new(MirType::Struct(
                        Arc::from("BuildCharIter"),
                    ))));
                    b.assign(
                        it_ptr,
                        MirRValue::AddressOf {
                            is_mut: true,
                            place: MirPlace::local(it),
                        },
                    );
                    let opt = b.create_local(MirType::Option(Box::new(MirType::u32())));
                    let after_next = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_char_iter_next")),
                        vec![values::local(it_ptr)],
                        Some(opt),
                        after_next,
                    );
                    b.switch_to_block(after_next);
                    let hv = b.create_local(MirType::Bool);
                    b.assign(
                        hv,
                        MirRValue::FieldAccess {
                            base: values::local(opt),
                            field_name: Arc::from("has_value"),
                            field_ty: MirType::Bool,
                        },
                    );
                    b.branch(values::local(hv), body_bb, exit_bb);
                    b.switch_to_block(body_bb);
                    let ch = b.create_local(MirType::u32());
                    b.assign(
                        ch,
                        MirRValue::FieldAccess {
                            base: values::local(opt),
                            field_name: Arc::from("value"),
                            field_ty: MirType::u32(),
                        },
                    );
                    let after_push = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_push_u32")),
                        vec![values::local(out), values::local(ch)],
                        None,
                        after_push,
                    );
                    b.switch_to_block(after_push);
                    b.goto(cond_bb);
                    b.switch_to_block(exit_bb);
                    let len = b.create_local(MirType::i64());
                    let after_len = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(out)],
                        Some(len),
                        after_len,
                    );
                    b.switch_to_block(after_len);
                    return Ok((
                        MirType::u32(),
                        ChainSource::Vec {
                            local: out,
                            get_fn: "build_hvec_get_u32".to_string(),
                        },
                        len,
                    ));
                }
            }
            if method.name.as_ref() == "keys" && args.is_empty() {
                let m_raw = self.lower_expr(str_recv)?;
                let m_val = self.deref_if_pointer(m_raw)?;
                if matches!(self.type_of_value(&m_val), MirType::Map(..)) {
                    let str_ty = MirType::Struct(Arc::from("BuildString"));
                    let b = self.current_fn.as_mut().unwrap();
                    let keys = b.create_local(MirType::Vec(Box::new(str_ty.clone())));
                    let c0 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hmap_keys_str_f64")),
                        vec![m_val],
                        Some(keys),
                        c0,
                    );
                    b.switch_to_block(c0);
                    let len = b.create_local(MirType::i64());
                    let c1 = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from("build_hvec_len")),
                        vec![values::local(keys)],
                        Some(len),
                        c1,
                    );
                    b.switch_to_block(c1);
                    return Ok((
                        str_ty,
                        ChainSource::Vec {
                            local: keys,
                            get_fn: "build_hvec_get_str".to_string(),
                        },
                        len,
                    ));
                }
            }
        }
        let raw_val = self.lower_expr(source_expr)?;
        let raw_ty = self.type_of_value(&raw_val);
        let (source_val, source_ty) = match &raw_ty {
            MirType::Ptr(inner)
                if matches!(
                    inner.as_ref(),
                    MirType::Vec(_) | MirType::Array(..) | MirType::Struct(_) | MirType::Map(_, _)
                ) =>
            {
                let derefed = self.deref_if_pointer(raw_val)?;
                let ty = self.type_of_value(&derefed);
                (derefed, ty)
            }
            _ => (raw_val, raw_ty),
        };
        // HashMap<K, V> source (`map.iter()`): materialize `Vec<(K, V)>` via keys + get.
        if let MirType::Map(k_ty, v_ty) = &source_ty {
            let k_c = k_ty.as_ref().clone();
            let v_c = v_ty.as_ref().clone();
            let str_ty = MirType::Struct(Arc::from("BuildString"));
            let pair_elems = vec![k_c.clone(), v_c.clone()];
            self.ensure_tuple_type_def(&pair_elems);
            let pair_ty = MirType::Tuple(pair_elems);
            let (new_fn, push_fn) = Self::vec_fn_names_for_type(&pair_ty)?;
            let (get_fn, _) = Self::vec_get_len_fn_names(&pair_ty)?;
            let b = self.current_fn.as_mut().unwrap();
            let map_loc = b.create_local(source_ty.clone());
            b.assign(map_loc, MirRValue::Use(source_val));
            let keys = b.create_local(MirType::Vec(Box::new(str_ty.clone())));
            let c0 = b.create_block();
            b.call(
                MirValue::Function(Arc::from("build_hmap_keys_str_f64")),
                vec![values::local(map_loc)],
                Some(keys),
                c0,
            );
            b.switch_to_block(c0);
            let out = b.create_local(MirType::Vec(Box::new(pair_ty.clone())));
            let c1 = b.create_block();
            b.call(MirValue::Function(Arc::from(new_fn)), vec![], Some(out), c1);
            b.switch_to_block(c1);
            let len_l = b.create_local(MirType::i64());
            let cl = b.create_block();
            b.call(
                MirValue::Function(Arc::from("build_hvec_len")),
                vec![values::local(keys)],
                Some(len_l),
                cl,
            );
            b.switch_to_block(cl);
            let idx = b.create_local(MirType::i64());
            b.assign_const(idx, MirConst::Int(0, MirType::i64()));
            let cond_bb = b.create_block();
            let body_bb = b.create_block();
            let exit_bb = b.create_block();
            b.goto(cond_bb);
            b.switch_to_block(cond_bb);
            let cmp = b.create_local(MirType::Bool);
            b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_l));
            b.branch(values::local(cmp), body_bb, exit_bb);
            b.switch_to_block(body_bb);
            let k_val = b.create_local(str_ty);
            let ck = b.create_block();
            b.call(
                MirValue::Function(Arc::from("build_hvec_get_str")),
                vec![values::local(keys), values::local(idx)],
                Some(k_val),
                ck,
            );
            b.switch_to_block(ck);
            let v_val = b.create_local(v_c.clone());
            let cv = b.create_block();
            if matches!(&v_c, MirType::Float(_)) {
                let kp = b.create_local(MirType::Ptr(Box::new(MirType::i8())));
                b.assign(
                    kp,
                    MirRValue::FieldAccess {
                        base: values::local(k_val),
                        field_name: Arc::from("ptr"),
                        field_ty: MirType::Ptr(Box::new(MirType::i8())),
                    },
                );
                b.call(
                    MirValue::Function(Arc::from("build_hmap_get_str_f64")),
                    vec![values::local(map_loc), values::local(kp)],
                    Some(v_val),
                    cv,
                );
            } else {
                let get_v = format!("build_hmap_get_val_{}", map_value_mangle(&v_c));
                b.call(
                    MirValue::Function(Arc::from(get_v.as_str())),
                    vec![values::local(map_loc), values::local(k_val)],
                    Some(v_val),
                    cv,
                );
            }
            b.switch_to_block(cv);
            let tup = b.create_local(pair_ty.clone());
            b.assign(
                tup,
                MirRValue::Aggregate {
                    kind: AggregateKind::Tuple,
                    operands: vec![values::local(k_val), values::local(v_val)],
                },
            );
            let cp = b.create_block();
            b.call(
                MirValue::Function(Arc::from(push_fn)),
                vec![values::local(out), values::local(tup)],
                None,
                cp,
            );
            b.switch_to_block(cp);
            let ni = b.create_local(MirType::i64());
            b.binary_op(
                ni,
                BinOp::Add,
                values::local(idx),
                MirValue::Const(MirConst::Int(1, MirType::i64())),
            );
            b.assign(idx, MirRValue::Use(values::local(ni)));
            b.goto(cond_bb);
            b.switch_to_block(exit_bb);
            return Ok((pair_ty, ChainSource::Vec { local: out, get_fn }, len_l));
        }
        // BuildString source (`s.bytes()`, `s.as_bytes()`): iterate raw bytes `u8`.
        if matches!(&source_ty, MirType::Struct(n) if n.as_ref() == "BuildString") {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let local = builder.create_local(source_ty);
            builder.assign(local, MirRValue::Use(source_val));
            let len_u = builder.create_local(MirType::usize());
            builder.assign(
                len_u,
                MirRValue::FieldAccess {
                    base: values::local(local),
                    field_name: Arc::from("len"),
                    field_ty: MirType::usize(),
                },
            );
            let len = builder.create_local(MirType::i64());
            builder.cast(
                len,
                CastKind::IntToInt,
                values::local(len_u),
                MirType::i64(),
            );
            return Ok((MirType::u8(), ChainSource::StringBytes { local }, len));
        }
        // Slice `&[T]` / `[T]` source
        let slice_elem = match &source_ty {
            MirType::Slice(inner) => Some((**inner).clone()),
            MirType::Ptr(inner) => match inner.as_ref() {
                MirType::Slice(e) => Some((**e).clone()),
                _ => None,
            },
            _ => None,
        };
        if let Some(elem_ty) = slice_elem {
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let local = builder.create_local(source_ty);
            builder.assign(local, MirRValue::Use(source_val));
            let len_u = builder.create_local(MirType::usize());
            builder.assign(
                len_u,
                MirRValue::FieldAccess {
                    base: values::local(local),
                    field_name: Arc::from("len"),
                    field_ty: MirType::usize(),
                },
            );
            let len = builder.create_local(MirType::i64());
            builder.cast(
                len,
                CastKind::IntToInt,
                values::local(len_u),
                MirType::i64(),
            );
            return Ok((elem_ty, ChainSource::Indexed { local }, len));
        }
        // Fixed array `[T; N]` source
        if let MirType::Array(inner, n) = &source_ty {
            let elem_ty = (**inner).clone();
            let n = *n;
            let builder = self
                .current_fn
                .as_mut()
                .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
            let local = builder.create_local(source_ty);
            builder.assign(local, MirRValue::Use(source_val));
            let len = builder.create_local(MirType::i64());
            builder.assign_const(len, MirConst::Int(n as i128, MirType::i64()));
            return Ok((elem_ty, ChainSource::Indexed { local }, len));
        }
        // Determine the element type from the Vec type.
        let elem_ty = match &source_ty {
            MirType::Vec(inner) => inner.as_ref().clone(),
            _ => MirType::f64(), // Fallback; most spectrum usage is f64
        };
        // Select the correct runtime function names for the element type.
        let (get_fn_name, len_fn_name) = Self::vec_get_len_fn_names(&elem_ty)?;
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let local = builder.create_local(source_ty.clone());
        builder.assign(local, MirRValue::Use(source_val));
        // len = vec_len(source)
        let len = builder.create_local(MirType::i64());
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from(len_fn_name)),
            vec![values::local(local)],
            Some(len),
            cont,
        );
        builder.switch_to_block(cont);
        Ok((
            elem_ty,
            ChainSource::Vec {
                local,
                get_fn: get_fn_name,
            },
            len,
        ))
    }

    /// A range as the source of a chain. The elements have the type of the
    /// bounds (a literal bound takes the other's type, as in `for`); the
    /// count is worked out in `i64`, so `5..2` is empty and an unsigned
    /// range does not wrap.
    fn lower_range_source(
        &mut self,
        start: &ast::Expr,
        end: &ast::Expr,
        inclusive: bool,
    ) -> CodegenResult<(MirType, ChainSource, LocalId)> {
        let start_val = self.lower_expr(start)?;
        let end_val = self.lower_expr(end)?;
        let elem_ty = if matches!(start_val, MirValue::Const(_)) {
            self.type_of_value(&end_val)
        } else {
            self.type_of_value(&start_val)
        };
        if !matches!(elem_ty, MirType::Int(..)) {
            return Err(self.reject_at(
                end,
                "an iterator chain over a range needs integer bounds".to_string(),
                "write the bounds as integers, `(0..n).map(...)`".to_string(),
            ));
        }
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function".to_string()))?;
        let first = builder.create_local(MirType::i64());
        builder.cast(first, CastKind::IntToInt, start_val, MirType::i64());
        let last = builder.create_local(MirType::i64());
        builder.cast(last, CastKind::IntToInt, end_val, MirType::i64());
        let len = builder.create_local(MirType::i64());
        builder.binary_op(len, BinOp::Sub, values::local(last), values::local(first));
        if inclusive {
            builder.binary_op(
                len,
                BinOp::Add,
                values::local(len),
                MirValue::Const(MirConst::Int(1, MirType::i64())),
            );
        }
        // An empty range runs no pass: a negative count becomes 0, so the
        // reversed form (which starts at len - 1) does not run either. A
        // call, not a branch: the count is `build_max_i64(len, 0)`.
        let count = builder.create_local(MirType::i64());
        let cont = builder.create_block();
        builder.call(
            MirValue::Function(Arc::from("build_max_i64")),
            vec![
                values::local(len),
                MirValue::Const(MirConst::Int(0, MirType::i64())),
            ],
            Some(count),
            cont,
        );
        builder.switch_to_block(cont);
        Ok((elem_ty, ChainSource::Range { first }, count))
    }

    /// Inline-lower a `.map()` closure call: evaluate the closure body
    /// with the closure's parameter bound to `elem_val`.
    ///
    /// Instead of emitting a real closure function + call, we directly
    /// lower the closure body expression in the current function scope
    /// with the parameter variable mapped to `elem_val`.
    pub(crate) fn lower_iter_map_inline(
        &mut self,
        closure_expr: &ast::Expr,
        elem_val: MirValue,
        index_val: Option<MirValue>,
        zip_val: Option<MirValue>,
    ) -> CodegenResult<MirValue> {
        // An unannotated element parameter takes the element's own type. It
        // used to default to f64, so `|x| *x != ""` over strings compared a
        // double with a string.
        let elem_ty_default = self.type_of_value(&elem_val);
        if let ExprKind::Closure { params, body, .. } = &closure_expr.kind {
            // Save the current var_map entries that will be shadowed.
            let mut saved: Vec<(Arc<str>, Option<LocalId>)> = Vec::new();
            let unwrap_pat_ident = |pat: &ast::Pattern| -> Option<Arc<str>> {
                let mut inner = pat;
                while let ast::PatternKind::Ref { pattern, .. } = &inner.kind {
                    inner = pattern.as_ref();
                }
                match &inner.kind {
                    ast::PatternKind::Ident { name, .. }
                    | ast::PatternKind::RefBind { name, .. } => Some(name.name.clone()),
                    _ => None,
                }
            };

            if params.len() == 1
                && zip_val.is_some()
                && matches!(&params[0].pattern.kind, ast::PatternKind::Tuple(t) if t.len() == 2)
            {
                // Recursive tuple destructuring: `|(a, b)|`, `|(&a, &b)|`, `|((&v, &t), &s)|`.
                fn bind_tuple_pat(
                    this: &mut MirLowerer<'_>,
                    pat: &ast::Pattern,
                    val: MirValue,
                    saved: &mut Vec<(Arc<str>, Option<LocalId>)>,
                ) {
                    let mut inner = pat;
                    while let ast::PatternKind::Ref { pattern, .. } = &inner.kind {
                        inner = pattern.as_ref();
                    }
                    match &inner.kind {
                        ast::PatternKind::Ident { name, .. }
                        | ast::PatternKind::RefBind { name, .. } => {
                            let val_ty = this.type_of_value(&val);
                            let old = this.var_map.get(&name.name).copied();
                            saved.push((name.name.clone(), old));
                            let builder = this.current_fn.as_mut().unwrap();
                            let local = builder.create_local(val_ty);
                            builder.assign(local, MirRValue::Use(val));
                            this.var_map.insert(name.name.clone(), local);
                        }
                        ast::PatternKind::Tuple(sub_pats) => {
                            let val_ty = this.type_of_value(&val);
                            if let MirType::Tuple(elem_tys) = val_ty {
                                for (idx, (sp, ety)) in
                                    sub_pats.iter().zip(elem_tys.iter()).enumerate()
                                {
                                    let field_val = {
                                        let builder = this.current_fn.as_mut().unwrap();
                                        let fl = builder.create_local(ety.clone());
                                        builder.assign(
                                            fl,
                                            MirRValue::FieldAccess {
                                                base: val.clone(),
                                                field_name: Arc::from(format!("_{}", idx)),
                                                field_ty: ety.clone(),
                                            },
                                        );
                                        values::local(fl)
                                    };
                                    bind_tuple_pat(this, sp, field_val, saved);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if let ast::PatternKind::Tuple(elems) = &params[0].pattern.kind {
                    let v2 = zip_val.unwrap();
                    bind_tuple_pat(self, &elems[0], elem_val, &mut saved);
                    bind_tuple_pat(self, &elems[1], v2, &mut saved);
                }
            } else if params.len() == 2 && index_val.is_some() {
                // enumerate-style: |i, x| body
                // First param = index, second param = element
                if let ast::PatternKind::Ident { name, .. } = &params[0].pattern.kind {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let idx_val = index_val.unwrap();
                    let param_ty = params[0]
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or(MirType::i64());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(idx_val));
                    self.var_map.insert(name.name.clone(), local);
                }
                if let ast::PatternKind::Ident { name, .. } = &params[1].pattern.kind {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let param_ty = params[1]
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or_else(|| elem_ty_default.clone());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(elem_val));
                    self.var_map.insert(name.name.clone(), local);
                }
            } else if params.len() == 1
                && index_val.is_some()
                && matches!(&params[0].pattern.kind, ast::PatternKind::Tuple(t) if t.len() == 2)
            {
                // enumerate-style with a tuple param: |(i, x)| body.
                // First tuple element = index, second = element.
                if let ast::PatternKind::Tuple(elems) = &params[0].pattern.kind {
                    if let ast::PatternKind::Ident { name, .. } = &elems[0].kind {
                        let old = self.var_map.get(&name.name).copied();
                        saved.push((name.name.clone(), old));
                        let builder = self.current_fn.as_mut().unwrap();
                        let local = builder.create_local(MirType::i64());
                        builder.assign(local, MirRValue::Use(index_val.clone().unwrap()));
                        self.var_map.insert(name.name.clone(), local);
                    }
                    if let ast::PatternKind::Ident { name, .. } = &elems[1].kind {
                        let old = self.var_map.get(&name.name).copied();
                        saved.push((name.name.clone(), old));
                        let elem_ty = self.type_of_value(&elem_val);
                        let builder = self.current_fn.as_mut().unwrap();
                        let local = builder.create_local(elem_ty);
                        builder.assign(local, MirRValue::Use(elem_val));
                        self.var_map.insert(name.name.clone(), local);
                    }
                }
            } else if let Some(first_param) = params.first() {
                // Single-param: |x|, |&x|, |&&x|, or tuple destructuring |(a, b)| / |(_, x)|
                let mut inner_pat = &first_param.pattern;
                while let ast::PatternKind::Ref { pattern, .. } = &inner_pat.kind {
                    inner_pat = pattern.as_ref();
                }
                if let ast::PatternKind::Tuple(elems) = &inner_pat.kind {
                    if let MirType::Tuple(elem_tys) = &elem_ty_default {
                        let tup_local = {
                            let builder = self.current_fn.as_mut().unwrap();
                            let l = builder.create_local(elem_ty_default.clone());
                            builder.assign(l, MirRValue::Use(elem_val));
                            l
                        };
                        for (i, ep) in elems.iter().enumerate() {
                            if let (Some(name), Some(fty)) = (unwrap_pat_ident(ep), elem_tys.get(i))
                            {
                                if name.as_ref() != "_" {
                                    let old = self.var_map.get(&name).copied();
                                    saved.push((name.clone(), old));
                                    let builder = self.current_fn.as_mut().unwrap();
                                    let fl = builder.create_local(fty.clone());
                                    builder.assign(
                                        fl,
                                        MirRValue::FieldAccess {
                                            base: values::local(tup_local),
                                            field_name: Arc::from(format!("_{i}")),
                                            field_ty: fty.clone(),
                                        },
                                    );
                                    self.var_map.insert(name, fl);
                                }
                            }
                        }
                    }
                } else if let ast::PatternKind::Ident { name, .. }
                | ast::PatternKind::RefBind { name, .. } = &inner_pat.kind
                {
                    let old = self.var_map.get(&name.name).copied();
                    saved.push((name.name.clone(), old));

                    let param_ty = first_param
                        .ty
                        .as_ref()
                        .map(|t| self.lower_type_from_ast(t))
                        .unwrap_or_else(|| elem_ty_default.clone());
                    let builder = self.current_fn.as_mut().unwrap();
                    let local = builder.create_local(param_ty);
                    builder.assign(local, MirRValue::Use(elem_val));
                    self.var_map.insert(name.name.clone(), local);
                }
            }

            // Lower the closure body inline.
            let result = self.lower_expr(body)?;

            // Restore shadowed var_map entries.
            for (name, old) in saved {
                if let Some(id) = old {
                    self.var_map.insert(name, id);
                } else {
                    self.var_map.remove(&name);
                }
            }

            Ok(result)
        } else {
            // Not a closure - shouldn't happen, but fall back to lowering as-is.
            self.lower_expr(closure_expr)
        }
    }

    /// Inline-lower a `.fold()` closure: `|acc, x| body`, or a binary function
    /// path such as `f32::max` / `f32::min` / `f64::max` / `f64::min`.
    fn lower_iter_fold_inline(
        &mut self,
        closure_expr: &ast::Expr,
        acc_val: MirValue,
        elem_val: MirValue,
    ) -> CodegenResult<MirValue> {
        let acc_ty_default = self.type_of_value(&acc_val);
        let elem_ty_default = self.type_of_value(&elem_val);
        if let ExprKind::Path(path) = &closure_expr.kind {
            if let Some(last) = path.last_ident() {
                if matches!(last.name.as_ref(), "max" | "min") {
                    let is_max = last.name.as_ref() == "max";
                    let builder = self.current_fn.as_mut().unwrap();
                    let out = builder.create_local(acc_ty_default.clone());
                    let cmp = builder.create_local(MirType::Bool);
                    builder.binary_op(
                        cmp,
                        if is_max { BinOp::Gt } else { BinOp::Lt },
                        acc_val.clone(),
                        elem_val.clone(),
                    );
                    let b_acc = builder.create_block();
                    let b_elem = builder.create_block();
                    let b_merge = builder.create_block();
                    builder.branch(values::local(cmp), b_acc, b_elem);
                    builder.switch_to_block(b_acc);
                    builder.assign(out, MirRValue::Use(acc_val));
                    builder.goto(b_merge);
                    builder.switch_to_block(b_elem);
                    builder.assign(out, MirRValue::Use(elem_val));
                    builder.goto(b_merge);
                    builder.switch_to_block(b_merge);
                    return Ok(values::local(out));
                }
            }
        }
        if let ExprKind::Closure { params, body, .. } = &closure_expr.kind {
            let mut saved: Vec<(Arc<str>, Option<LocalId>)> = Vec::new();
            let unwrap_pat_ident = |pat: &ast::Pattern| -> Option<Arc<str>> {
                let mut inner = pat;
                while let ast::PatternKind::Ref { pattern, .. } = &inner.kind {
                    inner = pattern.as_ref();
                }
                match &inner.kind {
                    ast::PatternKind::Ident { name, .. }
                    | ast::PatternKind::RefBind { name, .. } => Some(name.name.clone()),
                    _ => None,
                }
            };

            // Helper to bind either a simple ident or a tuple destructuring pattern.
            let bind_fold_param = |this: &mut Self,
                                   saved: &mut Vec<(Arc<str>, Option<LocalId>)>,
                                   param: &ast::ClosureParam,
                                   val: MirValue,
                                   default_ty: &MirType| {
                let inner_pat = match &param.pattern.kind {
                    ast::PatternKind::Ref { pattern, .. } => pattern.as_ref(),
                    _ => &param.pattern,
                };
                if let ast::PatternKind::Tuple(elems) = &inner_pat.kind {
                    if let MirType::Tuple(elem_tys) = default_ty {
                        let tup_local = {
                            let builder = this.current_fn.as_mut().unwrap();
                            let l = builder.create_local(default_ty.clone());
                            builder.assign(l, MirRValue::Use(val));
                            l
                        };
                        for (i, ep) in elems.iter().enumerate() {
                            if let (Some(name), Some(fty)) = (unwrap_pat_ident(ep), elem_tys.get(i))
                            {
                                if name.as_ref() != "_" {
                                    let old = this.var_map.get(&name).copied();
                                    saved.push((name.clone(), old));
                                    let builder = this.current_fn.as_mut().unwrap();
                                    let fl = builder.create_local(fty.clone());
                                    builder.assign(
                                        fl,
                                        MirRValue::FieldAccess {
                                            base: values::local(tup_local),
                                            field_name: Arc::from(format!("_{i}")),
                                            field_ty: fty.clone(),
                                        },
                                    );
                                    this.var_map.insert(name, fl);
                                }
                            }
                        }
                        return;
                    }
                }
                if let Some(name) = unwrap_pat_ident(&param.pattern) {
                    if name.as_ref() != "_" {
                        let old = this.var_map.get(&name).copied();
                        saved.push((name.clone(), old));

                        let param_ty = param
                            .ty
                            .as_ref()
                            .map(|t| this.lower_type_from_ast(t))
                            .unwrap_or_else(|| default_ty.clone());
                        let builder = this.current_fn.as_mut().unwrap();
                        let local = builder.create_local(param_ty);
                        builder.assign(local, MirRValue::Use(val));
                        this.var_map.insert(name, local);
                    }
                }
            };

            // Bind acc parameter.
            if let Some(acc_param) = params.first() {
                bind_fold_param(self, &mut saved, acc_param, acc_val, &acc_ty_default);
            }

            // Bind element parameter.
            if params.len() >= 2 {
                bind_fold_param(self, &mut saved, &params[1], elem_val, &elem_ty_default);
            }

            let result = self.lower_expr(body)?;

            for (name, old) in saved {
                if let Some(id) = old {
                    self.var_map.insert(name, id);
                } else {
                    self.var_map.remove(&name);
                }
            }

            Ok(result)
        } else {
            self.lower_expr(closure_expr)
        }
    }

    /// Infer the output element type of an iterator chain by examining
    /// the steps.  For `.map()` closures with a return type annotation,
    /// use that.  For closures whose body is a simple identifier matching
    /// a parameter, use that parameter's annotated type.  Otherwise,
    /// propagate the input element type.
    fn infer_chain_output_type(&self, input_ty: &MirType, steps: &[IterStep<'_>]) -> MirType {
        let mut ty = input_ty.clone();
        for step in steps {
            match step {
                IterStep::Map { closure } => {
                    if let ExprKind::Closure {
                        return_type,
                        params,
                        body,
                        ..
                    } = &closure.kind
                    {
                        if let Some(ret_ty) = return_type {
                            ty = self.lower_type_from_ast(ret_ty);
                        } else {
                            let single_param_name = if params.len() == 1 {
                                match &params[0].pattern.kind {
                                    ast::PatternKind::Ident { name, .. } => {
                                        Some(name.name.as_ref())
                                    }
                                    ast::PatternKind::Ref { pattern, .. } => match &pattern.kind {
                                        ast::PatternKind::Ident { name, .. } => {
                                            Some(name.name.as_ref())
                                        }
                                        _ => None,
                                    },
                                    _ => None,
                                }
                            } else {
                                None
                            };
                            let expr_is_param = |e: &ast::Expr| -> bool {
                                let mut cur = e;
                                loop {
                                    match &cur.kind {
                                        ExprKind::Paren(inner) | ExprKind::Deref(inner) => {
                                            cur = inner;
                                        }
                                        ExprKind::Ident(id) => {
                                            return single_param_name == Some(id.name.as_ref());
                                        }
                                        _ => return false,
                                    }
                                }
                            };
                            let struct_name_of = |t: &MirType| -> Option<Arc<str>> {
                                match t {
                                    MirType::Struct(n) => Some(n.clone()),
                                    MirType::Ptr(inner) => match inner.as_ref() {
                                        MirType::Struct(n) => Some(n.clone()),
                                        _ => None,
                                    },
                                    _ => None,
                                }
                            };
                            match &body.kind {
                                ExprKind::Cast { ty: cast_ty, .. } => {
                                    ty = self.lower_type_from_ast(cast_ty);
                                }
                                ExprKind::Macro { path, .. }
                                    if path
                                        .last_ident()
                                        .is_some_and(|i| i.name.as_ref() == "format") =>
                                {
                                    ty = MirType::Struct(Arc::from("BuildString"));
                                }
                                ExprKind::MethodCall { method, .. }
                                    if method.name.as_ref() == "len" =>
                                {
                                    ty = MirType::usize();
                                }
                                ExprKind::MethodCall {
                                    receiver, method, ..
                                } if matches!(
                                    method.name.as_ref(),
                                    "repeat" | "to_string" | "to_uppercase" | "to_lowercase"
                                ) && matches!(
                                    &receiver.kind,
                                    ExprKind::Literal(crate::ast::Literal::Str { .. })
                                ) =>
                                {
                                    ty = MirType::Struct(Arc::from("BuildString"));
                                }
                                ExprKind::Binary {
                                    op:
                                        crate::ast::BinOp::Sub
                                        | crate::ast::BinOp::Mul
                                        | crate::ast::BinOp::Div
                                        | crate::ast::BinOp::Rem,
                                    ..
                                } if matches!(
                                    &ty,
                                    MirType::Struct(n) if n.as_ref() == "BuildString"
                                ) =>
                                {
                                    ty = MirType::usize();
                                }
                                ExprKind::If { then_branch, .. }
                                    if then_branch.stmts.last().is_some_and(|s| {
                                        matches!(
                                            &s.kind,
                                            StmtKind::Expr(e)
                                                if matches!(
                                                    &e.kind,
                                                    ExprKind::Literal(
                                                        crate::ast::Literal::Str { .. }
                                                    )
                                                )
                                        )
                                    }) =>
                                {
                                    ty = MirType::Struct(Arc::from("BuildString"));
                                }
                                ExprKind::MethodCall {
                                    receiver, method, ..
                                } if expr_is_param(receiver) => {
                                    if let Some(sname) = struct_name_of(&ty) {
                                        if let Some(mangled) =
                                            self.impl_methods.get(&(sname, method.name.clone()))
                                        {
                                            if let Some(f) = self.module.find_function(mangled) {
                                                ty = f.sig.ret.clone();
                                            }
                                        }
                                    }
                                }
                                ExprKind::Field { expr, field } if expr_is_param(expr) => {
                                    if let Some(sname) = struct_name_of(&ty) {
                                        if let Some(fields) = self.struct_field_list(&sname) {
                                            if let Some((_, fty)) = fields.iter().find(|(n, _)| {
                                                n.as_deref() == Some(field.name.as_ref())
                                            }) {
                                                ty = fty.clone();
                                            }
                                        }
                                    }
                                }
                                ExprKind::Call { func, .. } => {
                                    let fn_name = match &func.kind {
                                        ExprKind::Ident(id) => Some(id.name.as_ref()),
                                        ExprKind::Path(p) => {
                                            p.last_ident().map(|i| i.name.as_ref())
                                        }
                                        _ => None,
                                    };
                                    if let Some(fname) = fn_name {
                                        if let Some(f) = self.module.find_function(fname) {
                                            ty = f.sig.ret.clone();
                                        }
                                    }
                                }
                                ExprKind::Ident(body_ident) => {
                                    for param in params {
                                        match &param.pattern.kind {
                                            ast::PatternKind::Ident { name, .. } => {
                                                if name.name.as_ref() == body_ident.name.as_ref() {
                                                    if let Some(pt) = &param.ty {
                                                        ty = self.lower_type_from_ast(pt);
                                                    }
                                                    break;
                                                }
                                            }
                                            ast::PatternKind::Tuple(sub_pats) => {
                                                if let MirType::Tuple(ref elems) = ty {
                                                    for (pi, sp) in sub_pats.iter().enumerate() {
                                                        if let ast::PatternKind::Ident {
                                                            name,
                                                            ..
                                                        } = &sp.kind
                                                        {
                                                            if name.name.as_ref()
                                                                == body_ident.name.as_ref()
                                                            {
                                                                if let Some(et) = elems.get(pi) {
                                                                    ty = et.clone();
                                                                }
                                                                break;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                _ if params.len() == 1 => {
                                    if let Some(p) = params.first() {
                                        if let Some(pt) = &p.ty {
                                            ty = self.lower_type_from_ast(pt);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                IterStep::FilterMap { closure } => {
                    if let ExprKind::Closure {
                        return_type: Some(rt),
                        ..
                    } = &closure.kind
                    {
                        let lowered = self.lower_type_from_ast(rt);
                        if let Some(inner) = Self::option_inner_from_mir_type(&lowered) {
                            ty = inner;
                        }
                    } else if let ExprKind::Closure { body, .. } = &closure.kind {
                        // Recognize `|s| s.parse::<T>().ok()`
                        if let ExprKind::MethodCall {
                            receiver,
                            method,
                            args,
                            ..
                        } = &body.kind
                        {
                            if method.name.as_ref() == "ok" && args.is_empty() {
                                if let ExprKind::MethodCall {
                                    method: inner_m,
                                    generics,
                                    ..
                                } = &receiver.kind
                                {
                                    if inner_m.name.as_ref() == "parse" {
                                        if let Some(ast::GenericArg::Type(t)) = generics.first() {
                                            ty = self.lower_type_from_ast(t);
                                        }
                                    }
                                }
                            } else if matches!(method.name.as_ref(), "get" | "get_mut") {
                                let recv_ty = self.infer_expr_type(receiver);
                                let map_v = match &recv_ty {
                                    MirType::Map(_, v) => Some((**v).clone()),
                                    MirType::Ptr(inner) => match inner.as_ref() {
                                        MirType::Map(_, v) => Some((**v).clone()),
                                        _ => None,
                                    },
                                    _ => None,
                                };
                                if let Some(vt) = map_v {
                                    ty = vt;
                                }
                            }
                        }
                    }
                }
                IterStep::Filter { .. }
                | IterStep::Enumerate
                | IterStep::Cloned
                | IterStep::Rev
                | IterStep::Take { .. }
                | IterStep::TakeWhile { .. }
                | IterStep::Skip { .. }
                | IterStep::StepBy { .. }
                | IterStep::FlatMap { .. }
                | IterStep::Zip { .. }
                | IterStep::Chain { .. }
                | IterStep::GroupBy { .. }
                | IterStep::Chunks { .. }
                | IterStep::Windows { .. }
                | IterStep::Dedup
                | IterStep::Unique
                | IterStep::Interleave { .. }
                | IterStep::Merge { .. }
                | IterStep::Intersperse { .. }
                | IterStep::CartesianProduct { .. }
                | IterStep::RunningFold { .. } => {
                    // These don't change the element type (flat_map uses expected_collect;
                    // zip already folded the pair type into base_infer_ty).
                }
            }
        }
        ty
    }

    /// Select the C runtime accessor for the element type, with the same
    /// element-to-suffix table every other Vec path uses. It used to know only
    /// f64 and i64 and read every other element (strings included) through the
    /// i32 accessor, which did not compile.
    pub(super) fn vec_get_len_fn_names(elem_ty: &MirType) -> CodegenResult<(String, &'static str)> {
        let suffix = match elem_ty {
            MirType::Float(_) => "f64".to_string(),
            MirType::Int(IntSize::I64, _) | MirType::Int(IntSize::ISize, _) => "i64".to_string(),
            MirType::Int(_, _) | MirType::Bool => "i32".to_string(),
            other => Self::hvec_elem_suffix(other)?,
        };
        Ok((format!("build_hvec_get_{suffix}"), "build_hvec_len"))
    }
}

/// The source text of one macro argument, with the module merge's renames
/// applied (see `register_macro_ident_rename`), trimmed.
pub(super) fn renamed_slice(
    source: &str,
    start: usize,
    end: usize,
    source_id: Option<crate::lexer::SourceId>,
) -> String {
    let Some(text) = source.get(start..end) else {
        return String::new();
    };
    let renames = match source_id {
        Some(id) => crate::lexer::macro_ident_renames_in(id, start as u32, end as u32),
        None => Vec::new(),
    };
    if renames.is_empty() {
        return text.trim().to_string();
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut at = start;
    for (s, e, name) in renames {
        let (s, e) = (s as usize, e as usize);
        if s < at {
            continue;
        }
        out.push_str(&source[at..s]);
        out.push_str(&name);
        at = e;
    }
    out.push_str(&source[at..end]);
    out.trim().to_string()
}
