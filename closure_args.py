"""A closure or a function passed to a generic parameter specialises the callee.

Run from the repo root. `apply<F: Fn(i32) -> i32>(f: F, v: i32)` called with
a closure gets one specialisation for that closure, and the values the closure
captured travel as extra parameters after the declared ones.
"""
import io

N = "\r\n"


def crlf(t):
    return t.replace("\r\n", "\n").replace("\n", N)


def rw(path, edits):
    s = io.open(path, newline="").read()
    for old, new, count in edits:
        old, new = crlf(old), crlf(new)
        assert s.count(old) == count, (path, old[:80], s.count(old))
        s = s.replace(old, new)
    io.open(path, "w", newline="").write(s)


rw(
    "compiler/src/codegen/lower/mod.rs",
    [
        (
            "    local_closure_name: HashMap<LocalId, Arc<str>>,\n",
            "    local_closure_name: HashMap<LocalId, Arc<str>>,\n"
            "    /// Set by a generic call for the specialisation it is about to lower:\n"
            "    /// for each parameter that receives a capturing closure, the parameter\n"
            "    /// index, the closure's function name, and the name and type of every\n"
            "    /// captured value. The specialisation takes those values as extra\n"
            "    /// parameters after the declared ones.\n"
            "    pub(crate) pending_closure_params: Vec<ClosureParam>,\n"
            "    /// Parameter and return types of every top-level function that is not\n"
            "    /// generic, by name, so a function named as a value has a type before\n"
            "    /// its body is lowered.\n"
            "    pub(crate) fn_ast_sigs: HashMap<Arc<str>, (Vec<ast::Type>, Option<ast::Type>)>,\n",
            1,
        ),
        (
            "            local_closure_name: HashMap::new(),\n",
            "            local_closure_name: HashMap::new(),\n"
            "            pending_closure_params: Vec::new(),\n"
            "            fn_ast_sigs: HashMap::new(),\n",
            2,
        ),
        (
            "        let items = Self::add_trait_default_methods(items);\n",
            "        let items = Self::add_trait_default_methods(items);\n"
            "        for item in &items {\n"
            "            if let ItemKind::Function(f) = &item.kind {\n"
            "                if !self.fn_has_type_generics(f) {\n"
            "                    let params = f.sig.params.iter().map(|p| (*p.ty).clone()).collect();\n"
            "                    let ret = f.sig.return_ty.as_ref().map(|t| (**t).clone());\n"
            "                    self.fn_ast_sigs.insert(f.name.name.clone(), (params, ret));\n"
            "                }\n"
            "            }\n"
            "        }\n",
            1,
        ),
        (
            "        let params: Vec<_> = f\n"
            "            .sig\n"
            "            .params\n"
            "            .iter()\n"
            "            .map(|p| self.lower_type_from_ast(&p.ty))\n"
            "            .collect();\n"
            "\n"
            "        let is_main = f.name.name.as_ref() == \"main\";\n",
            "        let mut params: Vec<_> = f\n"
            "            .sig\n"
            "            .params\n"
            "            .iter()\n"
            "            .map(|p| self.lower_type_from_ast(&p.ty))\n"
            "            .collect();\n"
            "        // A specialisation made for a capturing closure takes the captured\n"
            "        // values after its declared parameters.\n"
            "        let closure_params = std::mem::take(&mut self.pending_closure_params);\n"
            "        let declared_params = params.len();\n"
            "        for (_, _, captured) in &closure_params {\n"
            "            params.extend(captured.iter().map(|(_, ty)| ty.clone()));\n"
            "        }\n"
            "\n"
            "        let is_main = f.name.name.as_ref() == \"main\";\n",
            1,
        ),
        (
            "            let saved_pending_vecs = std::mem::take(&mut self.pending_untyped_vecs);\n"
            "\n"
            "            // Create function builder\n",
            "            let saved_pending_vecs = std::mem::take(&mut self.pending_untyped_vecs);\n"
            "            // Local ids restart in every function, so which locals hold a\n"
            "            // closure is per function too.\n"
            "            let saved_closure_locals = std::mem::take(&mut self.local_closure_name);\n"
            "\n"
            "            // Create function builder\n",
            1,
        ),
        (
            "            self.current_fn = Some(builder);\n"
            "\n"
            "            // Lower function body (tail expression sees the declared return type)\n",
            "            let mut next_param = declared_params;\n"
            "            for (index, closure_name, captured) in &closure_params {\n"
            "                self.local_closure_name\n"
            "                    .insert(builder.param_local(*index), closure_name.clone());\n"
            "                let mut here = Vec::with_capacity(captured.len());\n"
            "                for (name, _) in captured {\n"
            "                    let local_id = builder.param_local(next_param);\n"
            "                    builder.set_param_name(\n"
            "                        next_param,\n"
            "                        Arc::from(format!(\"{}_{}\", closure_name, name)),\n"
            "                    );\n"
            "                    here.push((name.clone(), local_id));\n"
            "                    next_param += 1;\n"
            "                }\n"
            "                self.closure_captures.insert(closure_name.clone(), here);\n"
            "            }\n"
            "\n"
            "            self.current_fn = Some(builder);\n"
            "\n"
            "            // Lower function body (tail expression sees the declared return type)\n",
            1,
        ),
        (
            "            self.pending_untyped_vecs = saved_pending_vecs;\n"
            "        } else {\n",
            "            self.pending_untyped_vecs = saved_pending_vecs;\n"
            "            self.local_closure_name = saved_closure_locals;\n"
            "        } else {\n",
            1,
        ),
        (
            "    fn mangle_generic_name(base: &str, subst: &HashMap<Arc<str>, MirType>) -> Arc<str> {\n",
            "    /// A short stable tag for a type that has no readable mangled name.\n"
            "    pub(crate) fn type_tag(ty: &MirType) -> String {\n"
            "        let mut h: u32 = 0x811c_9dc5;\n"
            "        for b in format!(\"{ty:?}\").bytes() {\n"
            "            h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);\n"
            "        }\n"
            "        format!(\"{h:08x}\")\n"
            "    }\n"
            "\n"
            "    fn mangle_generic_name(base: &str, subst: &HashMap<Arc<str>, MirType>) -> Arc<str> {\n",
            1,
        ),
        (
            "/// selected generic winner is monomorphized via `lower_generic_call`, not emitted\n",
            "/// selected generic winner is monomorphized via `lower_generic_call`, not emitted\n",
            1,
        ),
    ],
)

# The alias sits beside the other lowering types at the top of the module.
s = io.open("compiler/src/codegen/lower/mod.rs", newline="").read()
anchor = crlf("pub struct MirLowerer")
assert s.count(anchor) == 1
head = s.index(anchor)
doc_start = s.rfind(N + N, 0, head) + len(N + N)
alias = crlf(
    "/// A parameter that receives a capturing closure: its index, the closure's\n"
    "/// function name, and the name and type of each captured value.\n"
    "pub(crate) type ClosureParam = (usize, Arc<str>, Vec<(Arc<str>, MirType)>);\n"
    "\n"
)
s = s[:doc_start] + alias + s[doc_start:]
io.open("compiler/src/codegen/lower/mod.rs", "w", newline="").write(s)

rw(
    "compiler/src/codegen/lower/types.rs",
    [
        (
            "        // Build multi-param substitution map by inferring types from arguments.\n"
            "        let subst = self.infer_subst_from_args(&generic_fndef, &type_param_names, args);\n"
            "        let mangled_name = Self::mangle_generic_name(fn_name.as_ref(), &subst);\n",
            "        // Lower the arguments first: a closure argument has to exist before\n"
            "        // the callee can be specialised for it.\n"
            "        let mut arg_vals: Vec<_> = args\n"
            "            .iter()\n"
            "            .map(|a| self.lower_expr(a))\n"
            "            .collect::<CodegenResult<_>>()?;\n"
            "\n"
            "        // Build multi-param substitution map by inferring types from arguments.\n"
            "        let subst = self.infer_subst_from_args(&generic_fndef, &type_param_names, args);\n"
            "\n"
            "        // A parameter typed by a bare type parameter (`f: F`) that receives a\n"
            "        // function or a closure: `F` is that function's pointer type. It stays\n"
            "        // out of `subst`, which is written back into the AST by type name, and\n"
            "        // is resolved through `type_param_subst` while the body is lowered. A\n"
            "        // closure that captured values gets its own specialisation, and the\n"
            "        // captured values are passed after the declared arguments.\n"
            "        let mut subst = subst;\n"
            "        let mut fn_subst: HashMap<Arc<str>, MirType> = HashMap::new();\n"
            "        let mut closure_params: Vec<super::ClosureParam> = Vec::new();\n"
            "        let mut captured_args: Vec<MirValue> = Vec::new();\n"
            "        let mut fn_suffix = String::new();\n"
            "        for (i, param) in generic_fndef.sig.params.iter().enumerate() {\n"
            "            let ast::TypeKind::Path(path) = &param.ty.kind else {\n"
            "                continue;\n"
            "            };\n"
            "            let Some(tp) = path\n"
            "                .last_ident()\n"
            "                .filter(|_| path.is_simple())\n"
            "                .and_then(|id| type_param_names.iter().find(|n| **n == id.name))\n"
            "                .cloned()\n"
            "            else {\n"
            "                continue;\n"
            "            };\n"
            "            let fn_ty = match arg_vals.get(i) {\n"
            "                Some(MirValue::Local(id)) => self\n"
            "                    .current_fn\n"
            "                    .as_ref()\n"
            "                    .and_then(|b| b.local_type(*id))\n"
            "                    .filter(|t| matches!(t, MirType::FnPtr(_))),\n"
            "                Some(MirValue::Function(name)) => self.fn_value_type(name),\n"
            "                _ => None,\n"
            "            };\n"
            "            let Some(fn_ty) = fn_ty else {\n"
            "                continue;\n"
            "            };\n"
            "            if fn_subst.contains_key(&tp) {\n"
            "                continue;\n"
            "            }\n"
            "            fn_suffix.push_str(&format!(\"_fn{}\", Self::type_tag(&fn_ty)));\n"
            "            subst.remove(&tp);\n"
            "            fn_subst.insert(tp, fn_ty);\n"
            "            let Some(MirValue::Local(id)) = arg_vals.get(i) else {\n"
            "                continue;\n"
            "            };\n"
            "            let Some(closure_name) = self.local_closure_name.get(id).cloned() else {\n"
            "                continue;\n"
            "            };\n"
            "            let Some(captures) = self.closure_captures.get(&closure_name).cloned() else {\n"
            "                continue;\n"
            "            };\n"
            "            let typed: Vec<(Arc<str>, MirType)> = captures\n"
            "                .iter()\n"
            "                .map(|(name, local)| {\n"
            "                    let ty = self\n"
            "                        .current_fn\n"
            "                        .as_ref()\n"
            "                        .and_then(|b| b.local_type(*local))\n"
            "                        .unwrap_or(MirType::i32());\n"
            "                    (name.clone(), ty)\n"
            "                })\n"
            "                .collect();\n"
            "            captured_args.extend(captures.iter().map(|(_, local)| values::local(*local)));\n"
            "            fn_suffix.push_str(&format!(\"_{}\", closure_name.trim_start_matches('_')));\n"
            "            closure_params.push((i, closure_name, typed));\n"
            "        }\n"
            "        let mangled_name: Arc<str> = Arc::from(format!(\n"
            "            \"{}{}\",\n"
            "            Self::mangle_generic_name(fn_name.as_ref(), &subst),\n"
            "            fn_suffix\n"
            "        ));\n",
            1,
        ),
        (
            "            let saved_subst = std::mem::replace(&mut self.type_param_subst, subst.clone());\n"
            "\n"
            "            // Lower the specialized function as a normal (non-generic) function.\n"
            "            let lowered = self.lower_function(&specialized, &[]);\n"
            "\n"
            "            // Restore the caller's function context.\n"
            "            self.type_param_subst = saved_subst;\n",
            "            let mut body_subst = subst.clone();\n"
            "            body_subst.extend(fn_subst.iter().map(|(k, v)| (k.clone(), v.clone())));\n"
            "            let saved_subst = std::mem::replace(&mut self.type_param_subst, body_subst);\n"
            "            // Inside the specialisation a captured value is one of its own\n"
            "            // parameters; the caller's view of the closure comes back after.\n"
            "            let saved_captures: Vec<_> = closure_params\n"
            "                .iter()\n"
            "                .map(|(_, name, _)| (name.clone(), self.closure_captures.get(name).cloned()))\n"
            "                .collect();\n"
            "            self.pending_closure_params = closure_params.clone();\n"
            "\n"
            "            // Lower the specialized function as a normal (non-generic) function.\n"
            "            let lowered = self.lower_function(&specialized, &[]);\n"
            "\n"
            "            // Restore the caller's function context.\n"
            "            self.pending_closure_params.clear();\n"
            "            for (name, captures) in saved_captures {\n"
            "                match captures {\n"
            "                    Some(c) => self.closure_captures.insert(name, c),\n"
            "                    None => self.closure_captures.remove(&name),\n"
            "                };\n"
            "            }\n"
            "            self.type_param_subst = saved_subst;\n",
            1,
        ),
        (
            "        let arg_vals: Vec<_> = args\n"
            "            .iter()\n"
            "            .map(|a| self.lower_expr(a))\n"
            "            .collect::<CodegenResult<_>>()?;\n"
            "\n"
            "        let builder = self\n"
            "            .current_fn\n"
            "            .as_mut()\n"
            "            .ok_or_else(|| CodegenError::Internal(\"No current function\".to_string()))?;\n"
            "        let result = builder.create_local(ret_ty);\n"
            "        let cont = builder.create_block();\n"
            "\n"
            "        builder.call(func_val, arg_vals, Some(result), cont);\n",
            "        arg_vals.extend(captured_args);\n"
            "\n"
            "        let builder = self\n"
            "            .current_fn\n"
            "            .as_mut()\n"
            "            .ok_or_else(|| CodegenError::Internal(\"No current function\".to_string()))?;\n"
            "        let result = builder.create_local(ret_ty);\n"
            "        let cont = builder.create_block();\n"
            "\n"
            "        builder.call(func_val, arg_vals, Some(result), cont);\n",
            1,
        ),
        (
            "    /// Match a parameter's AST type against an argument's lowered type and bind\n",
            "    /// The pointer type of a top-level function named as a value.\n"
            "    pub(crate) fn fn_value_type(&self, name: &str) -> Option<MirType> {\n"
            "        if let Some(f) = self.module.find_function(name) {\n"
            "            return Some(MirType::FnPtr(Box::new(f.sig.clone())));\n"
            "        }\n"
            "        let (params, ret) = self.fn_ast_sigs.get(name)?;\n"
            "        let params = params.iter().map(|t| self.lower_type_from_ast(t)).collect();\n"
            "        let ret = ret\n"
            "            .as_ref()\n"
            "            .map(|t| self.lower_type_from_ast(t))\n"
            "            .unwrap_or(MirType::Void);\n"
            "        Some(MirType::FnPtr(Box::new(MirFnSig::new(params, ret))))\n"
            "    }\n"
            "\n"
            "    /// Match a parameter's AST type against an argument's lowered type and bind\n",
            1,
        ),
    ],
)
print("applied")
