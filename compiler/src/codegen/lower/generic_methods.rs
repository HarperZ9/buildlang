//! Methods of generic instances are compiled when they are first called.
//!
//! Before, naming an instance (`Chain<Zero, R>` in a struct literal) lowered
//! every method of the type at once. A method that builds the type around
//! its own instance then named a larger instance, whose methods named a
//! larger one again, and the compiler ran out of memory (#303). An instance
//! now gets its methods' names and signatures when it is named, and a
//! method's body is lowered at its first call, so a method nobody calls is
//! never compiled. A chain of calls that does not end (a method of `W<T>`
//! calling the same method on `W<W<T>>`) stops with a refusal when one
//! generic type is being lowered `MAX_GENERIC_NESTING` times inside itself.
//!
//! An associated function of a generic type (`Buffer::new(4)`) takes its
//! instance from the type the call is expected to give: a `let` annotation,
//! a parameter or a return type (#305). With no such type it stays refused.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ast::{self, ExprKind};
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::MirBuilder;
use crate::codegen::ir::*;

use super::{ClosureParam, MirLowerer, MAX_GENERIC_NESTING};

/// Type parameter name to concrete type.
pub(crate) type Subst = HashMap<Arc<str>, MirType>;

/// A method of a generic instance, specialised and waiting for its first call.
pub(crate) struct PendingMethod {
    /// The generic type's own name (`Chain`).
    base: Arc<str>,
    /// The instance's mangled name (`Chain_Zero_R`).
    type_name: Arc<str>,
    /// The method with the instance's types written in.
    def: ast::FnDef,
    subst: Subst,
}

/// The per-function state a method body lowered in the middle of another
/// function must not see or change.
struct OuterFn {
    current_fn: Option<MirBuilder>,
    current_item_vis: Option<bool>,
    var_map: HashMap<Arc<str>, LocalId>,
    loop_stack: Vec<(BlockId, BlockId, Option<String>)>,
    loop_drop_depths: Vec<usize>,
    loop_break_values: HashMap<BlockId, LocalId>,
    char_locals: HashSet<LocalId>,
    ref_locals: HashSet<LocalId>,
    char_vec_locals: HashSet<LocalId>,
    str_ref_locals: HashSet<LocalId>,
    pending_enumerate: Option<(ast::Pattern, LocalId)>,
    enumerate_bound: bool,
    pending_untyped_vecs: HashMap<LocalId, Vec<LocalId>>,
    untyped_vec_new_binding: bool,
    untyped_vec_new_dest: Option<LocalId>,
    observed_pending_vecs: HashSet<LocalId>,
    in_macro_fragment: bool,
    pending_closure_params: Vec<ClosureParam>,
    closure_param_hint: Option<Vec<MirType>>,
    parse_target_hint: Option<MirType>,
    expected_type: Option<MirType>,
    type_param_subst: Subst,
    current_impl_type: Option<Arc<str>>,
    local_closure_name: HashMap<LocalId, Arc<str>>,
    local_env_closure: HashMap<LocalId, Arc<str>>,
    option_inner_types: HashMap<LocalId, MirType>,
    result_ok_types: HashMap<LocalId, MirType>,
    result_err_types: HashMap<LocalId, MirType>,
}

impl<'ctx> MirLowerer<'ctx> {
    fn take_outer_fn(&mut self) -> OuterFn {
        OuterFn {
            current_fn: self.current_fn.take(),
            current_item_vis: self.current_item_vis.take(),
            var_map: std::mem::take(&mut self.var_map),
            loop_stack: std::mem::take(&mut self.loop_stack),
            loop_drop_depths: std::mem::take(&mut self.loop_drop_depths),
            loop_break_values: std::mem::take(&mut self.loop_break_values),
            char_locals: std::mem::take(&mut self.char_locals),
            ref_locals: std::mem::take(&mut self.ref_locals),
            char_vec_locals: std::mem::take(&mut self.char_vec_locals),
            str_ref_locals: std::mem::take(&mut self.str_ref_locals),
            pending_enumerate: self.pending_enumerate.take(),
            enumerate_bound: std::mem::take(&mut self.enumerate_bound),
            pending_untyped_vecs: std::mem::take(&mut self.pending_untyped_vecs),
            untyped_vec_new_binding: std::mem::take(&mut self.untyped_vec_new_binding),
            untyped_vec_new_dest: self.untyped_vec_new_dest.take(),
            observed_pending_vecs: std::mem::take(&mut self.observed_pending_vecs),
            in_macro_fragment: std::mem::take(&mut self.in_macro_fragment),
            pending_closure_params: std::mem::take(&mut self.pending_closure_params),
            closure_param_hint: self.closure_param_hint.take(),
            parse_target_hint: self.parse_target_hint.take(),
            expected_type: self.expected_type.take(),
            type_param_subst: std::mem::take(&mut self.type_param_subst),
            current_impl_type: self.current_impl_type.take(),
            local_closure_name: std::mem::take(&mut self.local_closure_name),
            local_env_closure: std::mem::take(&mut self.local_env_closure),
            option_inner_types: std::mem::take(&mut self.option_inner_types),
            result_ok_types: std::mem::take(&mut self.result_ok_types),
            result_err_types: std::mem::take(&mut self.result_err_types),
        }
    }

    fn restore_outer_fn(&mut self, outer: OuterFn) {
        self.current_fn = outer.current_fn;
        self.current_item_vis = outer.current_item_vis;
        self.var_map = outer.var_map;
        self.loop_stack = outer.loop_stack;
        self.loop_drop_depths = outer.loop_drop_depths;
        self.loop_break_values = outer.loop_break_values;
        self.char_locals = outer.char_locals;
        self.ref_locals = outer.ref_locals;
        self.char_vec_locals = outer.char_vec_locals;
        self.str_ref_locals = outer.str_ref_locals;
        self.pending_enumerate = outer.pending_enumerate;
        self.enumerate_bound = outer.enumerate_bound;
        self.pending_untyped_vecs = outer.pending_untyped_vecs;
        self.untyped_vec_new_binding = outer.untyped_vec_new_binding;
        self.untyped_vec_new_dest = outer.untyped_vec_new_dest;
        self.observed_pending_vecs = outer.observed_pending_vecs;
        self.in_macro_fragment = outer.in_macro_fragment;
        self.pending_closure_params = outer.pending_closure_params;
        self.closure_param_hint = outer.closure_param_hint;
        self.parse_target_hint = outer.parse_target_hint;
        self.expected_type = outer.expected_type;
        self.type_param_subst = outer.type_param_subst;
        self.current_impl_type = outer.current_impl_type;
        self.local_closure_name = outer.local_closure_name;
        self.local_env_closure = outer.local_env_closure;
        self.option_inner_types = outer.option_inner_types;
        self.result_ok_types = outer.result_ok_types;
        self.result_err_types = outer.result_err_types;
    }

    /// The refusal for instances nested inside one another without end.
    pub(super) fn nesting_refusal(base: &str, mangled: &str) -> CodegenError {
        let shown: String = mangled.chars().take(60).collect();
        CodegenError::Rejected {
            message: format!(
                "the methods of generic type `{base}` build `{base}` around their own \
                 type without end: stopped at {MAX_GENERIC_NESTING} instances inside \
                 one another (`{shown}...`)"
            ),
            location: None,
            help: Some(format!(
                "a method of `{base}` calls a method of a larger instance of \
                 `{base}`, which calls a larger one again; give the recursion a \
                 fixed type, or write it as a loop"
            )),
        }
    }

    /// Record the signature of a specialised method and keep its body for
    /// its first call.
    pub(super) fn schedule_generic_method(
        &mut self,
        base: &str,
        type_name: &Arc<str>,
        def: ast::FnDef,
        subst: &Subst,
    ) {
        let saved_impl = self.current_impl_type.replace(type_name.clone());
        let saved_subst = std::mem::replace(&mut self.type_param_subst, subst.clone());
        let sig = self.generic_method_sig(type_name, &def);
        self.current_impl_type = saved_impl;
        self.type_param_subst = saved_subst;
        let name = def.name.name.clone();
        self.generic_method_sigs.insert(name.clone(), sig);
        self.pending_generic_methods.insert(
            name,
            PendingMethod {
                base: Arc::from(base),
                type_name: type_name.clone(),
                def,
                subst: subst.clone(),
            },
        );
    }

    /// The signature `lower_generic_impl_method` gives the method.
    fn generic_method_sig(&self, type_name: &Arc<str>, f: &ast::FnDef) -> MirFnSig {
        let params = f
            .sig
            .params
            .iter()
            .map(|param| {
                let is_self = matches!(&param.pattern.kind,
                    ast::PatternKind::Ident { name, .. } if name.name.as_ref() == "self");
                match (is_self, &param.ty.kind) {
                    (true, ast::TypeKind::Ref { .. }) => {
                        MirType::Ptr(Box::new(MirType::Struct(type_name.clone())))
                    }
                    (true, _) => MirType::Struct(type_name.clone()),
                    (false, _) => self.lower_type_from_ast(&param.ty),
                }
            })
            .collect();
        let ret = f
            .sig
            .return_ty
            .as_ref()
            .map(|t| self.lower_type_from_ast(t))
            .unwrap_or(MirType::Void);
        MirFnSig::new(params, ret)
    }

    /// Lower the body of a scheduled method if it has not been lowered yet.
    /// The method is lowered in its own state and the caller's is put back.
    pub(super) fn lower_pending_generic_method(&mut self, name: &str) -> CodegenResult<()> {
        let Some(pending) = self.pending_generic_methods.remove(name) else {
            return Ok(());
        };
        let nested = self
            .generic_lowering_stack
            .iter()
            .filter(|b| **b == pending.base)
            .count();
        if nested >= MAX_GENERIC_NESTING {
            return Err(Self::nesting_refusal(&pending.base, &pending.type_name));
        }
        self.generic_lowering_stack.push(pending.base.clone());
        let outer = self.take_outer_fn();
        self.current_impl_type = Some(pending.type_name.clone());
        self.type_param_subst = pending.subst.clone();
        let done = self.lower_generic_impl_method(&pending.type_name, &pending.def);
        self.restore_outer_fn(outer);
        self.generic_lowering_stack.pop();
        done
    }

    /// `impl_methods` for a call: the method's C name, its body lowered.
    pub(super) fn impl_method_for_call(
        &mut self,
        type_name: &Arc<str>,
        method: &str,
    ) -> CodegenResult<Option<Arc<str>>> {
        let key = (type_name.clone(), Arc::from(method));
        let Some(name) = self.impl_methods.get(&key).cloned() else {
            return Ok(None);
        };
        self.lower_pending_generic_method(&name)?;
        Ok(Some(name))
    }

    /// The signature of a function by its C name: a lowered or declared
    /// function, or a method of a generic instance not lowered yet.
    pub(super) fn known_fn_sig(&self, name: &str) -> Option<MirFnSig> {
        if let Some(f) = self.module.find_function(name) {
            return Some(f.sig.clone());
        }
        self.generic_method_sigs.get(name).cloned()
    }

    /// Note which generic type and arguments a mangled instance name stands
    /// for, so a later call can instantiate it.
    pub(super) fn note_generic_shape(&self, mangled: &Arc<str>, base: &str, subst: &Subst) {
        self.generic_shapes
            .borrow_mut()
            .entry(mangled.clone())
            .or_insert_with(|| (Arc::from(base), subst.clone()));
    }

    /// Instantiate `base` with `subst` (a struct or an enum).
    fn instantiate_generic(&mut self, base: &str, subst: &Subst) -> CodegenResult<Arc<str>> {
        if self.generic_structs.contains_key(base) {
            self.monomorphize_struct(base, subst)
        } else {
            self.monomorphize_enum_multi(base, subst)
        }
    }

    /// The instance of generic type `base` the expected type names, created
    /// if it has not been yet.
    pub(super) fn expected_instance_of(&mut self, base: &str) -> CodegenResult<Option<Arc<str>>> {
        let wanted = match &self.expected_type {
            Some(MirType::Struct(name)) => name.clone(),
            _ => return Ok(None),
        };
        let shape = self.generic_shapes.borrow().get(&wanted).cloned();
        match shape {
            Some((shape_base, subst)) if shape_base.as_ref() == base => {
                self.instantiate_generic(base, &subst).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// `Type::f(..)` on a generic type, rewritten to the instance it calls
    /// (`Buffer_i32::new`) with that function lowered. `None` leaves the call
    /// to the usual path: not such a call, or no instance can be told, in
    /// which case the usual path refuses it as before.
    pub(super) fn generic_assoc_callee(
        &mut self,
        func: &ast::Expr,
    ) -> CodegenResult<Option<ast::Expr>> {
        let ExprKind::Path(path) = &func.kind else {
            return Ok(None);
        };
        if path.segments.len() != 2 {
            return Ok(None);
        }
        let ty = path.segments[0].ident.name.clone();
        let method = path.segments[1].ident.name.clone();
        let direct = format!("{ty}_{method}");
        if self.pending_generic_methods.contains_key(direct.as_str()) {
            self.lower_pending_generic_method(&direct)?;
            return Ok(None);
        }
        if !self.generic_impls.contains_key(&ty) {
            return Ok(None);
        }
        let explicit = &path.segments[0].generics;
        let instance = if explicit.is_empty() {
            self.expected_instance_of(&ty)?
        } else {
            let subst = self.resolve_generic_args_with_subst(&ty, explicit, &HashMap::new());
            if subst.is_empty() {
                None
            } else {
                Some(self.instantiate_generic(&ty, &subst)?)
            }
        };
        let Some(instance) = instance else {
            return Ok(None);
        };
        let target = format!("{instance}_{method}");
        self.lower_pending_generic_method(&target)?;
        if self.module.find_function(&target).is_none() {
            return Ok(None);
        }
        let mut rewritten = func.clone();
        if let ExprKind::Path(p) = &mut rewritten.kind {
            p.segments[0].ident.name = instance;
            p.segments[0].generics.clear();
        }
        Ok(Some(rewritten))
    }
}
