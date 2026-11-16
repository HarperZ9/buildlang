"""Type aliases resolve in the C backend; a trait's default methods reach each impl."""
import io

N = "\r\n"


def crlf(t):
    return t.replace("\r\n", "\n").replace("\n", N)


def rw(path, edits):
    s = io.open(path, newline="").read()
    for old, new, count in edits:
        old, new = crlf(old), crlf(new)
        assert s.count(old) == count, (path, old[:70], s.count(old))
        s = s.replace(old, new)
    io.open(path, "w", newline="").write(s)


rw(
    "compiler/src/codegen/lower/mod.rs",
    [
        (
            "    pub(crate) str_ref_locals: std::collections::HashSet<LocalId>,\n",
            "    pub(crate) str_ref_locals: std::collections::HashSet<LocalId>,\n"
            "    /// `type Name = T;` at the top level: the name stands for `T`.\n"
            "    pub(crate) type_aliases: HashMap<Arc<str>, ast::Type>,\n",
            1,
        ),
        (
            "            str_ref_locals: std::collections::HashSet::new(),\n",
            "            str_ref_locals: std::collections::HashSet::new(),\n"
            "            type_aliases: HashMap::new(),\n",
            2,
        ),
        (
            "        let items = self.rename_shadowing_fns(&module.items);\n",
            "        let items = self.rename_shadowing_fns(&module.items);\n"
            "        let items = Self::add_trait_default_methods(items);\n"
            "        for item in &items {\n"
            "            if let ItemKind::TypeAlias(alias) = &item.kind {\n"
            "                if let Some(ty) = alias.ty.as_ref() {\n"
            "                    self.type_aliases\n"
            "                        .insert(alias.name.name.clone(), (**ty).clone());\n"
            "                }\n"
            "            }\n"
            "        }\n",
            1,
        ),
        (
            "    /// Top-level functions with a body whose names collide (see\n",
            "    /// Give every `impl Trait for T` the trait's methods that have a body and\n"
            "    /// that the impl does not write itself. After this an inherited method is\n"
            "    /// an ordinary method of `T`: `T_method` exists, and `Self` in its body is\n"
            "    /// `T`. Only traits declared at the top level of the file are followed.\n"
            "    fn add_trait_default_methods(items: Vec<ast::Item>) -> Vec<ast::Item> {\n"
            "        let mut defaults: HashMap<Arc<str>, Vec<ast::FnDef>> = HashMap::new();\n"
            "        for item in &items {\n"
            "            if let ItemKind::Trait(t) = &item.kind {\n"
            "                let with_body: Vec<ast::FnDef> = t\n"
            "                    .items\n"
            "                    .iter()\n"
            "                    .filter_map(|ti| match &ti.kind {\n"
            "                        ast::TraitItemKind::Function(f) if f.body.is_some() => {\n"
            "                            Some((**f).clone())\n"
            "                        }\n"
            "                        _ => None,\n"
            "                    })\n"
            "                    .collect();\n"
            "                if !with_body.is_empty() {\n"
            "                    defaults.insert(t.name.name.clone(), with_body);\n"
            "                }\n"
            "            }\n"
            "        }\n"
            "        if defaults.is_empty() {\n"
            "            return items;\n"
            "        }\n"
            "        items\n"
            "            .into_iter()\n"
            "            .map(|mut item| {\n"
            "                let ItemKind::Impl(impl_def) = &mut item.kind else {\n"
            "                    return item;\n"
            "                };\n"
            "                let Some(inherited) = impl_def\n"
            "                    .trait_ref\n"
            "                    .as_ref()\n"
            "                    .and_then(|tr| tr.path.last_ident())\n"
            "                    .and_then(|name| defaults.get(&name.name))\n"
            "                else {\n"
            "                    return item;\n"
            "                };\n"
            "                for f in inherited {\n"
            "                    let written = impl_def.items.iter().any(|ii| {\n"
            "                        matches!(&ii.kind, ast::ImplItemKind::Function(g)\n"
            "                            if g.name.name == f.name.name)\n"
            "                    });\n"
            "                    if !written {\n"
            "                        impl_def.items.push(ast::ImplItem {\n"
            "                            vis: ast::Visibility::Private,\n"
            "                            attrs: Vec::new(),\n"
            "                            is_default: false,\n"
            "                            kind: ast::ImplItemKind::Function(Box::new(f.clone())),\n"
            "                            span: f.name.span,\n"
            "                            id: ast::NodeId::DUMMY,\n"
            "                        });\n"
            "                    }\n"
            "                }\n"
            "                item\n"
            "            })\n"
            "            .collect()\n"
            "    }\n"
            "\n"
            "    /// Top-level functions with a body whose names collide (see\n",
            1,
        ),
    ],
)

rw(
    "compiler/src/codegen/lower/types.rs",
    [
        (
            "    fn lower_type_path(&self, path: &ast::Path) -> MirType {\n"
            "        if let Some(ident) = path.last_ident() {\n",
            "    fn lower_type_path(&self, path: &ast::Path) -> MirType {\n"
            "        // `type Meters = f64;`: the name stands for the type it names.\n"
            "        if path.segments.len() == 1 && path.last_generics().is_none_or(|g| g.is_empty()) {\n"
            "            if let Some(aliased) = path\n"
            "                .last_ident()\n"
            "                .and_then(|i| self.type_aliases.get(&i.name))\n"
            "            {\n"
            "                return self.lower_type_from_ast(aliased);\n"
            "            }\n"
            "        }\n"
            "        if let Some(ident) = path.last_ident() {\n",
            1,
        ),
    ],
)
print("applied")
