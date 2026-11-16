"""User globals get their own C names, so a constant named EOF, NULL, TRUE or LONG builds."""
import io

N = "\r\n"


def rw(path, edits):
    s = io.open(path, newline="").read()
    nl = "\r\n" if "\r\n" in s else "\n"
    for old, new, count in edits:
        old, new = old.replace("\n", nl), new.replace("\n", nl)
        assert s.count(old) == count, (path, old[:70], s.count(old))
        s = s.replace(old, new)
    io.open(path, "w", newline="").write(s)


rw(
    "compiler/src/codegen/backend/c.rs",
    [
        (
            "    global_types: std::collections::HashMap<String, MirType>,\n",
            "    global_types: std::collections::HashMap<String, MirType>,\n"
            "    /// Globals the program defines (not foreign declarations). Each is\n"
            "    /// emitted under `global_emit_name`, so a constant named like a C macro\n"
            "    /// or a platform typedef (`EOF`, `NULL`, `TRUE`, `LONG`) still builds.\n"
            "    user_globals: std::collections::HashSet<String>,\n",
            1,
        ),
        (
            "            global_types: std::collections::HashMap::new(),\n",
            "            global_types: std::collections::HashMap::new(),\n"
            "            user_globals: std::collections::HashSet::new(),\n",
            1,
        ),
        (
            "            if global.is_extern_decl {\n"
            "                if global.link_header.is_none() {\n",
            "            if !global.is_extern_decl {\n"
            "                self.user_globals.insert(global.name.to_string());\n"
            "            }\n"
            "            if global.is_extern_decl {\n"
            "                if global.link_header.is_none() {\n",
            1,
        ),
        (
            "                // A function reference in value position (a direct call's callee)\n"
            "                // must get the same stdlib-collision escape as its definition.\n"
            "                other if self.escaped_user_fns.contains(other) => Self::user_fn_emit_name(other),\n",
            "                other if self.user_globals.contains(other) => Self::global_emit_name(other),\n"
            "                // A function reference in value position (a direct call's callee)\n"
            "                // must get the same stdlib-collision escape as its definition.\n"
            "                other if self.escaped_user_fns.contains(other) => Self::user_fn_emit_name(other),\n",
            1,
        ),
        (
            '                write!(self.output, "{} = {};\\n", name, rvalue).unwrap();\n',
            '                let c_name = self.value_to_c(&MirValue::Global(name.clone()), locals);\n'
            '                write!(self.output, "{} = {};\\n", c_name, rvalue).unwrap();\n',
            1,
        ),
    ],
)

rw(
    "compiler/src/codegen/backend/c/globals.rs",
    [
        (
            "        let decl = if matches!(global.ty, MirType::Array(..)) {\n"
            "            self.fmt_array_decl(&global.ty, &global.name)\n"
            "        } else {\n"
            '            format!("{} {}", self.type_to_c(&global.ty), global.name)\n'
            "        };\n",
            "        // A foreign static names a C symbol and keeps its name.\n"
            "        let c_name = if global.is_extern_decl {\n"
            "            global.name.to_string()\n"
            "        } else {\n"
            "            Self::global_emit_name(&global.name)\n"
            "        };\n"
            "        let decl = if matches!(global.ty, MirType::Array(..)) {\n"
            "            self.fmt_array_decl(&global.ty, &c_name)\n"
            "        } else {\n"
            '            format!("{} {}", self.type_to_c(&global.ty), c_name)\n'
            "        };\n",
            1,
        ),
        (
            "    /// The C name a global is read under, as `value_to_c` writes it.\n",
            "    /// The C name of a global the program defines. C headers define macros\n"
            "    /// and types under ordinary constant names (`EOF`, `NULL`, `TRUE`, `LONG`\n"
            "    /// and `IN` on Windows), and no list of them can be complete, so every\n"
            "    /// such global gets a prefix no header uses.\n"
            "    pub(super) fn global_emit_name(name: &str) -> String {\n"
            '        format!("bld_g_{name}")\n'
            "    }\n"
            "\n"
            "    /// The C name a global is read under, as `value_to_c` writes it.\n",
            1,
        ),
    ],
)
print("applied")
