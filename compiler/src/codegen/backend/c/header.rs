//! Struct definitions for the C export header (M4).
//!
//! The header declares each `extern "C"` export, so it must also define every
//! struct those signatures reach, by value or through a pointer, in an order a
//! C compiler accepts: a struct embedded by value comes before the struct that
//! embeds it. The lowering pass already refused any export type without a C
//! ABI, so only structs, scalars, pointers and fixed arrays arrive here.

use super::CBackend;
use crate::codegen::ir::{MirFunction, MirModule, MirType, TypeDefKind};

impl CBackend {
    /// `typedef struct` definitions for every struct the exports reach.
    pub(super) fn export_struct_defs(
        &self,
        module: &MirModule,
        exports: &[&MirFunction],
    ) -> String {
        let mut order: Vec<String> = Vec::new();
        for func in exports {
            for local in func.locals.iter().filter(|l| l.is_param) {
                Self::visit_struct_deps(module, &local.ty, &mut order, &mut Vec::new());
            }
            Self::visit_struct_deps(module, &func.sig.ret, &mut order, &mut Vec::new());
        }
        let mut out = String::new();
        for name in &order {
            let Some(def) = module.types.iter().find(|t| t.name.as_ref() == name) else {
                continue;
            };
            let TypeDefKind::Struct { fields, .. } = &def.kind else {
                continue;
            };
            out.push_str(&format!("typedef struct {name} {{\n"));
            if fields.is_empty() {
                out.push_str("    char _pad;\n");
            }
            for (i, (field_name, field_ty)) in fields.iter().enumerate() {
                let raw = field_name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("field{i}"));
                let field = Self::escape_c_keyword(&raw);
                if matches!(field_ty, MirType::Array(_, _)) {
                    out.push_str(&format!("    {};\n", self.fmt_array_decl(field_ty, &field)));
                } else {
                    out.push_str(&format!("    {} {};\n", self.type_to_c(field_ty), field));
                }
            }
            out.push_str(&format!("}} {name};\n\n"));
        }
        out
    }

    /// Post-order walk: a struct's by-value and pointed-to struct fields are
    /// listed before the struct itself. `stack` stops pointer cycles.
    fn visit_struct_deps(
        module: &MirModule,
        ty: &MirType,
        order: &mut Vec<String>,
        stack: &mut Vec<String>,
    ) {
        match ty {
            MirType::Ptr(inner) | MirType::Array(inner, _) => {
                Self::visit_struct_deps(module, inner, order, stack)
            }
            MirType::Struct(name) => {
                let name = name.to_string();
                if order.contains(&name) || stack.contains(&name) {
                    return;
                }
                let Some(def) = module.types.iter().find(|t| t.name.as_ref() == name) else {
                    return;
                };
                if let TypeDefKind::Struct { fields, .. } = &def.kind {
                    stack.push(name.clone());
                    for (_, field_ty) in fields {
                        Self::visit_struct_deps(module, field_ty, order, stack);
                    }
                    stack.pop();
                    order.push(name);
                }
            }
            _ => {}
        }
    }
}
