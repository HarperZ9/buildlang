//! Every function the backend refuses is reported, one refusal each.
//!
//! Before, lowering stopped at the first function it could not compile, so
//! `buildc check` showed one BL0901 and the next one appeared only after the
//! first was fixed. The body pass now keeps a refusal, puts the lowerer back
//! in the state a finished function leaves it in, and goes on to the next
//! top-level function or method. The module still fails: no MIR is returned
//! when any function was refused.
//!
//! One refusal per function: lowering a body stops at its first refusal, as
//! before. A function nested in another is part of the outer one's report.

use crate::codegen::backend::{CodegenError, CodegenResult};

use super::MirLowerer;

impl<'ctx> MirLowerer<'ctx> {
    /// Run `lower` for one top-level function or method. A refusal is kept
    /// in `self.rejections` and the lowerer is reset, so the caller goes on.
    pub(super) fn lower_keeping_rejection(
        &mut self,
        lower: impl FnOnce(&mut Self) -> CodegenResult<()>,
    ) {
        let source = self.source.clone();
        let impl_type = self.current_impl_type.clone();
        let prefix_len = self.module_prefix.len();
        if let Err(err) = lower(self) {
            self.reset_function_state();
            self.source = source;
            self.current_impl_type = impl_type;
            self.module_prefix.truncate(prefix_len);
            self.keep_rejection(err);
        }
    }

    /// Add a refusal unless the same one is already kept (a body that is
    /// lowered from two places, such as a generic instance, reports once).
    pub(super) fn keep_rejection(&mut self, err: CodegenError) {
        let text = format!("{err:?}");
        if !self
            .rejections
            .iter()
            .any(|kept| format!("{kept:?}") == text)
        {
            self.rejections.push(err);
        }
    }

    /// Put every per-function field back to what a function that lowered
    /// without error leaves behind at the top level: no current function,
    /// no open loop or drop scope, no pending hint.
    fn reset_function_state(&mut self) {
        self.current_fn = None;
        self.current_item_vis = None;
        self.var_map.clear();
        self.loop_stack.clear();
        self.loop_drop_depths.clear();
        self.loop_break_values.clear();
        self.char_locals.clear();
        self.ref_locals.clear();
        self.char_vec_locals.clear();
        self.str_ref_locals.clear();
        self.pending_enumerate = None;
        self.enumerate_bound = false;
        self.pending_untyped_vecs.clear();
        self.untyped_vec_new_binding = false;
        self.untyped_vec_new_dest = None;
        self.observed_pending_vecs.clear();
        self.in_macro_fragment = false;
        self.local_closure_name.clear();
        self.local_env_closure.clear();
        self.pending_closure_params.clear();
        self.closure_param_hint = None;
        self.parse_target_hint = None;
        self.drop_scopes.clear();
        self.expected_type = None;
        self.type_param_subst.clear();
    }
}
