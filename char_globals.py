"""Globals of type bool and char print as what they are. Run from the repo root."""
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
            "    pub(crate) shadow_fns: std::collections::HashSet<Arc<str>>,\n",
            "    pub(crate) shadow_fns: std::collections::HashSet<Arc<str>>,\n"
            "    /// Constants and statics declared `char`. A char is an integer in MIR, so\n"
            "    /// printing one as a character needs to know where it came from.\n"
            "    pub(crate) char_globals: std::collections::HashSet<Arc<str>>,\n",
            1,
        ),
        (
            "            shadow_fns: std::collections::HashSet::new(),\n",
            "            shadow_fns: std::collections::HashSet::new(),\n"
            "            char_globals: std::collections::HashSet::new(),\n",
            2,
        ),
        (
            "    fn lower_static(&mut self, s: &ast::StaticDef) -> CodegenResult<()> {\n"
            "        let ty = self.lower_type_from_ast(&s.ty);\n",
            "    fn lower_static(&mut self, s: &ast::StaticDef) -> CodegenResult<()> {\n"
            "        let ty = self.lower_type_from_ast(&s.ty);\n"
            "        if Self::ast_type_is_char(&s.ty) {\n"
            "            self.char_globals.insert(s.name.name.clone());\n"
            "        }\n",
            1,
        ),
        (
            "    fn lower_const(&mut self, c: &ast::ConstDef) -> CodegenResult<()> {\n"
            "        let ty = self.lower_type_from_ast(&c.ty);\n",
            "    fn lower_const(&mut self, c: &ast::ConstDef) -> CodegenResult<()> {\n"
            "        let ty = self.lower_type_from_ast(&c.ty);\n"
            "        if Self::ast_type_is_char(&c.ty) {\n"
            "            self.char_globals.insert(c.name.name.clone());\n"
            "        }\n",
            1,
        ),
    ],
)

rw(
    "compiler/src/codegen/lower/expr.rs",
    [
        (
            "                || matches!(&init_val, Some(MirValue::Local(src)) if self.char_locals.contains(src));\n",
            "                || matches!(&init_val, Some(MirValue::Local(src)) if self.char_locals.contains(src))\n"
            "                || matches!(&init_val, Some(MirValue::Global(g)) if self.char_globals.contains(g));\n",
            1,
        ),
    ],
)

rw(
    "compiler/src/codegen/lower/macros.rs",
    [
        (
            "        let val = result?;\n"
            "        // `x as char` or a call to a `-> char` function prints as a character.\n",
            "        let val = result?;\n"
            "        // A global read inline has no local to carry its type: a `bool` was\n"
            "        // passed to `%s` as it was (a crash) and a `char` printed as its code.\n"
            "        // Copy it into a typed local, as a `let` would.\n"
            "        if let MirValue::Global(name) = &val {\n"
            "            let is_char = self.char_globals.contains(name);\n"
            "            let ty = self.module.find_global(name).map(|g| g.ty.clone());\n"
            "            if let Some(ty) = ty.filter(|t| is_char || matches!(t, MirType::Bool)) {\n"
            "                let builder = self.current_fn.as_mut().ok_or_else(|| {\n"
            "                    CodegenError::Internal(\"No current function for macro\".into())\n"
            "                })?;\n"
            "                let local = builder.create_local(ty);\n"
            "                builder.assign(local, MirRValue::Use(val.clone()));\n"
            "                if is_char {\n"
            "                    self.char_locals.insert(local);\n"
            "                }\n"
            "                return Ok(MirValue::Local(local));\n"
            "            }\n"
            "        }\n"
            "        // `x as char` or a call to a `-> char` function prints as a character.\n",
            1,
        ),
    ],
)
print("applied")
