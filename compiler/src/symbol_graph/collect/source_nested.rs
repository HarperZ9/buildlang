use buildlang::ast::{self, ImplItemKind, Item, StructFields, TraitDef, TraitItemKind};

use super::super::model::SymbolSourceSymbol;
use super::source::{fn_signature, push_symbol, visibility_name};

pub(super) fn collect_struct_fields(
    symbols: &mut Vec<SymbolSourceSymbol>,
    parent: &str,
    fields: &StructFields,
) {
    match fields {
        StructFields::Named(fields) => {
            for field in fields {
                let name = field.name.as_str();
                push_symbol(
                    symbols,
                    format!("source:struct:{parent}.field:{name}"),
                    "struct_field",
                    name,
                    visibility_name(&field.vis),
                    field.span,
                    None,
                );
            }
        }
        StructFields::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                let name = index.to_string();
                push_symbol(
                    symbols,
                    format!("source:struct:{parent}.field:{name}"),
                    "struct_field",
                    &name,
                    visibility_name(&field.vis),
                    field.span,
                    None,
                );
            }
        }
        StructFields::Unit => {}
    }
}

pub(super) fn collect_trait_items(
    symbols: &mut Vec<SymbolSourceSymbol>,
    item: &Item,
    trait_def: &TraitDef,
) {
    push_symbol(
        symbols,
        format!("source:trait:{}", trait_def.name),
        "trait",
        trait_def.name.as_str(),
        visibility_name(&item.vis),
        item.span,
        None,
    );
    for trait_item in &trait_def.items {
        match &trait_item.kind {
            TraitItemKind::Function(f) => push_symbol(
                symbols,
                format!("source:trait:{}.method:{}", trait_def.name, f.name),
                "trait_method",
                f.name.as_str(),
                "trait".to_string(),
                trait_item.span,
                Some(fn_signature(&f.sig)),
            ),
            TraitItemKind::Type { name, .. } => push_symbol(
                symbols,
                format!("source:trait:{}.type:{name}", trait_def.name),
                "trait_type",
                name.as_str(),
                "trait".to_string(),
                trait_item.span,
                None,
            ),
            TraitItemKind::Const { name, .. } => push_symbol(
                symbols,
                format!("source:trait:{}.const:{name}", trait_def.name),
                "trait_const",
                name.as_str(),
                "trait".to_string(),
                trait_item.span,
                None,
            ),
            TraitItemKind::Macro { .. } => {}
        }
    }
}

pub(super) fn collect_impl_items(symbols: &mut Vec<SymbolSourceSymbol>, imp: &ast::ImplDef) {
    for impl_item in &imp.items {
        match &impl_item.kind {
            ImplItemKind::Function(f) => push_symbol(
                symbols,
                format!("source:impl.method:{}", f.name),
                "impl_method",
                f.name.as_str(),
                visibility_name(&impl_item.vis),
                impl_item.span,
                Some(fn_signature(&f.sig)),
            ),
            ImplItemKind::Type { name, .. } => push_symbol(
                symbols,
                format!("source:impl.type:{name}"),
                "impl_type",
                name.as_str(),
                visibility_name(&impl_item.vis),
                impl_item.span,
                None,
            ),
            ImplItemKind::Const { name, .. } => push_symbol(
                symbols,
                format!("source:impl.const:{name}"),
                "impl_const",
                name.as_str(),
                visibility_name(&impl_item.vis),
                impl_item.span,
                None,
            ),
            ImplItemKind::Macro { .. } => {}
        }
    }
}

pub(super) fn collect_foreign_items(
    symbols: &mut Vec<SymbolSourceSymbol>,
    block: &ast::ExternBlockDef,
) {
    for item in &block.items {
        match &item.kind {
            ast::ForeignItemKind::Fn(f) => push_symbol(
                symbols,
                format!("source:extern:function:{}", f.name),
                "extern_function",
                f.name.as_str(),
                visibility_name(&item.vis),
                item.span,
                Some(fn_signature(&f.sig)),
            ),
            ast::ForeignItemKind::Static { name, .. } => push_symbol(
                symbols,
                format!("source:extern:static:{name}"),
                "extern_static",
                name.as_str(),
                visibility_name(&item.vis),
                item.span,
                None,
            ),
            ast::ForeignItemKind::Type { name, .. } => push_symbol(
                symbols,
                format!("source:extern:type:{name}"),
                "extern_type",
                name.as_str(),
                visibility_name(&item.vis),
                item.span,
                None,
            ),
            ast::ForeignItemKind::Macro { .. } => {}
        }
    }
}

pub(super) fn collect_effect_operations(
    symbols: &mut Vec<SymbolSourceSymbol>,
    item: &Item,
    effect: &ast::EffectDef,
) {
    push_symbol(
        symbols,
        format!("source:effect:{}", effect.name),
        "effect",
        effect.name.as_str(),
        visibility_name(&item.vis),
        item.span,
        None,
    );
    for op in &effect.operations {
        push_symbol(
            symbols,
            format!("source:effect:{}.operation:{}", effect.name, op.name),
            "effect_operation",
            op.name.as_str(),
            "effect".to_string(),
            op.span,
            None,
        );
    }
}
