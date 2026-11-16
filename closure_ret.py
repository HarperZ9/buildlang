"""A function can return a closure: `fn adder(k: i32) -> impl Fn(i32) -> i32`.

Run from the repo root, after closure_args.py. The function returns the values
the closure captured, as one struct; a call through the returned value calls
the closure's function with those values. Nothing is allocated.
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
            "    pub(crate) pending_closure_params: Vec<ClosureParam>,\n",
            "    pub(crate) pending_closure_params: Vec<ClosureParam>,\n"
            "    /// Functions declared to return `impl Trait`, by name, kept so one\n"
            "    /// can be lowered the first time it is called: what it returns is\n"
            "    /// known only from its body.\n"
            "    pub(crate) impl_return_fns: HashMap<Arc<str>, ast::FnDef>,\n"
            "    /// Functions from `impl_return_fns` already lowered.\n"
            "    pub(crate) impl_return_done: HashSet<Arc<str>>,\n"
            "    /// A function that returns a capturing closure -> that closure's name.\n"
            "    pub(crate) fn_closure_env: HashMap<Arc<str>, Arc<str>>,\n"
            "    /// Closure name -> the struct that carries its captured values out of\n"
            "    /// the function that made it, and each field's name and type.\n"
            "    pub(crate) closure_env_fields: HashMap<Arc<str>, ClosureEnv>,\n"
            "    /// Locals that hold such a struct -> the closure they stand for.\n"
            "    pub(crate) local_env_closure: HashMap<LocalId, Arc<str>>,\n",
            1,
        ),
        (
            "            pending_closure_params: Vec::new(),\n",
            "            pending_closure_params: Vec::new(),\n"
            "            impl_return_fns: HashMap::new(),\n"
            "            impl_return_done: HashSet::new(),\n"
            "            fn_closure_env: HashMap::new(),\n"
            "            closure_env_fields: HashMap::new(),\n"
            "            local_env_closure: HashMap::new(),\n",
            2,
        ),
        (
            "                    self.fn_ast_sigs.insert(f.name.name.clone(), (params, ret));\n",
            "                    self.fn_ast_sigs.insert(f.name.name.clone(), (params, ret));\n"
            "                    if f\n"
            "                        .sig\n"
            "                        .return_ty\n"
            "                        .as_ref()\n"
            "                        .is_some_and(|t| matches!(t.kind, ast::TypeKind::ImplTrait { .. }))\n"
            "                    {\n"
            "                        self.impl_return_fns.insert(f.name.name.clone(), f.clone());\n"
            "                    }\n",
            1,
        ),
        (
            "        // Skip generic functions - they are monomorphized on demand at call sites.\n"
            "        if self.fn_has_type_generics(f) {\n"
            "            return Ok(());\n"
            "        }\n"
            "\n"
            "        // Build function signature\n"
            "        let mut params: Vec<_> = f\n",
            "        // Skip generic functions - they are monomorphized on demand at call sites.\n"
            "        if self.fn_has_type_generics(f) {\n"
            "            return Ok(());\n"
            "        }\n"
            "        // A function that returns `impl Trait` is lowered once, at its first\n"
            "        // call or here, whichever comes first.\n"
            "        if self.impl_return_fns.contains_key(&f.name.name)\n"
            "            && !self.impl_return_done.insert(f.name.name.clone())\n"
            "        {\n"
            "            return Ok(());\n"
            "        }\n"
            "\n"
            "        // Build function signature\n"
            "        let mut params: Vec<_> = f\n",
            1,
        ),
        (
            "            let saved_closure_locals = std::mem::take(&mut self.local_closure_name);\n",
            "            let saved_closure_locals = std::mem::take(&mut self.local_closure_name);\n"
            "            let saved_env_locals = std::mem::take(&mut self.local_env_closure);\n",
            1,
        ),
        (
            "            self.local_closure_name = saved_closure_locals;\n",
            "            self.local_closure_name = saved_closure_locals;\n"
            "            self.local_env_closure = saved_env_locals;\n",
            1,
        ),
        (
            "            let result = self.lower_fn_body(body)?;\n"
            "\n"
            "            // Add return if needed\n",
            "            let result = self.lower_fn_body(body)?;\n"
            "            // A closure in the tail of a function that returns `impl Trait`\n"
            "            // leaves as its function pointer, or as its captured values.\n"
            "            let (result, closure_ret) = if self.impl_return_fns.contains_key(&f.name.name) {\n"
            "                self.closure_return_value(&emitted_name, result)\n"
            "            } else {\n"
            "                (result, None)\n"
            "            };\n"
            "\n"
            "            // Add return if needed\n",
            1,
        ),
        (
            "            let mut func = builder.build();\n"
            "            if is_main {\n"
            "                func.linkage = Linkage::External; // main must not be static\n"
            "            }\n",
            "            let mut func = builder.build();\n"
            "            if let Some(ty) = closure_ret {\n"
            "                func.sig.ret = ty.clone();\n"
            "                for declared in self.module.functions.iter_mut() {\n"
            "                    if declared.name == emitted_name {\n"
            "                        declared.sig.ret = ty.clone();\n"
            "                    }\n"
            "                }\n"
            "            }\n"
            "            if is_main {\n"
            "                func.linkage = Linkage::External; // main must not be static\n"
            "            }\n",
            1,
        ),
        (
            "pub(crate) type ClosureParam = (usize, Arc<str>, Vec<(Arc<str>, MirType)>);\n",
            "pub(crate) type ClosureParam = (usize, Arc<str>, Vec<(Arc<str>, MirType)>);\n"
            "\n"
            "/// The struct that carries a closure's captured values: its name, and each\n"
            "/// field's name and type.\n"
            "pub(crate) type ClosureEnv = (Arc<str>, Vec<(Arc<str>, MirType)>);\n",
            1,
        ),
    ],
)

rw(
    "compiler/src/codegen/lower/expr.rs",
    [
        (
            "            ExprKind::Call { func, args } => self.lower_call(func, args),\n",
            "            ExprKind::Call { func, args } => self.lower_call_with_closures(func, args),\n",
            1,
        ),
        (
            "                    if let Some(cname) = self.local_closure_name.get(src_id).cloned() {\n"
            "                        self.local_closure_name.insert(local_id, cname);\n"
            "                    }\n",
            "                    if let Some(cname) = self.local_closure_name.get(src_id).cloned() {\n"
            "                        self.local_closure_name.insert(local_id, cname);\n"
            "                    }\n"
            "                    if let Some(cname) = self.local_env_closure.get(src_id).cloned() {\n"
            "                        self.local_env_closure.insert(local_id, cname);\n"
            "                    }\n",
            1,
        ),
        (
            "    fn lower_call(&mut self, func: &ast::Expr, args: &[ast::Expr]) -> CodegenResult<MirValue> {\n",
            "    /// A call, with the two cases a returned closure adds. Calling a\n"
            "    /// function that returns `impl Trait` lowers that function first, so\n"
            "    /// its real return type is known, and notes when the result carries a\n"
            "    /// closure's captured values. Calling through such a result calls the\n"
            "    /// closure's function with those values after the written arguments.\n"
            "    fn lower_call_with_closures(\n"
            "        &mut self,\n"
            "        func: &ast::Expr,\n"
            "        args: &[ast::Expr],\n"
            "    ) -> CodegenResult<MirValue> {\n"
            "        let callee: Option<Arc<str>> = self.extract_call_name(func).map(Arc::from);\n"
            "        let Some(callee) = callee else {\n"
            "            return self.lower_call(func, args);\n"
            "        };\n"
            "        let held = self\n"
            "            .var_map\n"
            "            .get(&callee)\n"
            "            .and_then(|local| Some((*local, self.local_env_closure.get(local)?.clone())));\n"
            "        if let Some((env_local, closure_name)) = held {\n"
            "            return self.lower_env_closure_call(env_local, &closure_name, args);\n"
            "        }\n"
            "        if self.var_map.contains_key(&callee) {\n"
            "            return self.lower_call(func, args);\n"
            "        }\n"
            "        if let Some(def) = self.impl_return_fns.get(&callee).cloned() {\n"
            "            if !self.impl_return_done.contains(&callee) {\n"
            "                self.lower_function(&def, &[])?;\n"
            "            }\n"
            "        }\n"
            "        let value = self.lower_call(func, args)?;\n"
            "        if let (Some(closure_name), MirValue::Local(id)) =\n"
            "            (self.fn_closure_env.get(&callee).cloned(), &value)\n"
            "        {\n"
            "            self.local_env_closure.insert(*id, closure_name);\n"
            "        }\n"
            "        Ok(value)\n"
            "    }\n"
            "\n"
            "    /// Call the closure whose captured values `env_local` holds.\n"
            "    fn lower_env_closure_call(\n"
            "        &mut self,\n"
            "        env_local: LocalId,\n"
            "        closure_name: &Arc<str>,\n"
            "        args: &[ast::Expr],\n"
            "    ) -> CodegenResult<MirValue> {\n"
            "        let mut arg_vals: Vec<MirValue> = args\n"
            "            .iter()\n"
            "            .map(|a| self.lower_expr(a))\n"
            "            .collect::<CodegenResult<_>>()?;\n"
            "        let fields = self\n"
            "            .closure_env_fields\n"
            "            .get(closure_name)\n"
            "            .map(|(_, fields)| fields.clone())\n"
            "            .unwrap_or_default();\n"
            "        let ret_ty = self\n"
            "            .module\n"
            "            .find_function(closure_name.as_ref())\n"
            "            .map(|f| f.sig.ret.clone())\n"
            "            .unwrap_or(MirType::i32());\n"
            "        let builder = self\n"
            "            .current_fn\n"
            "            .as_mut()\n"
            "            .ok_or_else(|| CodegenError::Internal(\"No current function\".to_string()))?;\n"
            "        for (name, ty) in fields {\n"
            "            let held = builder.create_local(ty.clone());\n"
            "            builder.assign(\n"
            "                held,\n"
            "                MirRValue::FieldAccess {\n"
            "                    base: values::local(env_local),\n"
            "                    field_name: name,\n"
            "                    field_ty: ty,\n"
            "                },\n"
            "            );\n"
            "            arg_vals.push(values::local(held));\n"
            "        }\n"
            "        let cont = builder.create_block();\n"
            "        let func_val = MirValue::Function(closure_name.clone());\n"
            "        if matches!(ret_ty, MirType::Void) {\n"
            "            builder.call(func_val, arg_vals, None, cont);\n"
            "            builder.switch_to_block(cont);\n"
            "            Ok(values::unit())\n"
            "        } else {\n"
            "            let result = builder.create_local(ret_ty);\n"
            "            builder.call(func_val, arg_vals, Some(result), cont);\n"
            "            builder.switch_to_block(cont);\n"
            "            Ok(values::local(result))\n"
            "        }\n"
            "    }\n"
            "\n"
            "    /// What a function that returns `impl Trait` hands back when its body\n"
            "    /// ends in a closure, with the type to put in its signature. A closure\n"
            "    /// that captured nothing leaves as its function pointer. One that\n"
            "    /// captured values leaves as a struct of those values. Anything else\n"
            "    /// is returned as it was.\n"
            "    pub(crate) fn closure_return_value(\n"
            "        &mut self,\n"
            "        fn_name: &Arc<str>,\n"
            "        result: Option<MirValue>,\n"
            "    ) -> (Option<MirValue>, Option<MirType>) {\n"
            "        let Some(MirValue::Local(id)) = result else {\n"
            "            return (result, None);\n"
            "        };\n"
            "        let Some(closure_name) = self.local_closure_name.get(&id).cloned() else {\n"
            "            return (result, None);\n"
            "        };\n"
            "        let Some(builder) = self.current_fn.as_mut() else {\n"
            "            return (result, None);\n"
            "        };\n"
            "        let captures = self\n"
            "            .closure_captures\n"
            "            .get(&closure_name)\n"
            "            .cloned()\n"
            "            .unwrap_or_default();\n"
            "        if captures.is_empty() {\n"
            "            let ty = builder.local_type(id);\n"
            "            return (result, ty);\n"
            "        }\n"
            "        let env_name: Arc<str> = Arc::from(format!(\"{closure_name}_env\"));\n"
            "        let fields: Vec<(Arc<str>, MirType)> = captures\n"
            "            .iter()\n"
            "            .map(|(name, local)| {\n"
            "                (\n"
            "                    name.clone(),\n"
            "                    builder.local_type(*local).unwrap_or(MirType::i32()),\n"
            "                )\n"
            "            })\n"
            "            .collect();\n"
            "        let env_ty = MirType::Struct(env_name.clone());\n"
            "        let env = builder.create_local(env_ty.clone());\n"
            "        builder.aggregate(\n"
            "            env,\n"
            "            AggregateKind::Struct(env_name.clone()),\n"
            "            captures.iter().map(|(_, local)| values::local(*local)).collect(),\n"
            "        );\n"
            "        self.module.add_type(MirTypeDef {\n"
            "            name: env_name.clone(),\n"
            "            kind: TypeDefKind::Struct {\n"
            "                fields: fields\n"
            "                    .iter()\n"
            "                    .map(|(name, ty)| (Some(name.clone()), ty.clone()))\n"
            "                    .collect(),\n"
            "                packed: false,\n"
            "            },\n"
            "        });\n"
            "        self.fn_closure_env.insert(fn_name.clone(), closure_name.clone());\n"
            "        self.closure_env_fields\n"
            "            .insert(closure_name, (env_name, fields));\n"
            "        (Some(values::local(env)), Some(env_ty))\n"
            "    }\n"
            "\n"
            "    fn lower_call(&mut self, func: &ast::Expr, args: &[ast::Expr]) -> CodegenResult<MirValue> {\n",
            1,
        ),
    ],
)
print("applied")
