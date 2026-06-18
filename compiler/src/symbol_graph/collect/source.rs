use quantalang::ast::{self, ItemKind, Module, Visibility};
use quantalang::lexer::Span;

use super::super::model::{SymbolSourceSignature, SymbolSourceSpan, SymbolSourceSymbol};
use super::super::sorted;
use super::source_nested::{
    collect_effect_operations, collect_foreign_items, collect_impl_items, collect_struct_fields,
    collect_trait_items,
};

fn source_span(span: Span) -> SymbolSourceSpan {
    SymbolSourceSpan {
        start: span.start.0,
        end: span.end.0,
    }
}

pub(super) fn visibility_name(vis: &Visibility) -> String {
    match vis {
        Visibility::Private => "private",
        Visibility::Public(_) => "public",
        Visibility::Crate(_) => "crate",
        Visibility::Super(_) => "super",
        Visibility::Restricted { .. } => "restricted",
    }
    .to_string()
}

fn path_last_name(path: &ast::Path) -> Option<String> {
    path.last_ident().map(|ident| ident.as_str().to_string())
}

pub(super) fn fn_signature(sig: &ast::FnSig) -> SymbolSourceSignature {
    SymbolSourceSignature {
        parameters: sig.params.len(),
        has_return: sig.return_ty.is_some(),
        is_async: sig.is_async,
        is_unsafe: sig.is_unsafe,
        declared_effects: sorted(sig.effects.iter().filter_map(path_last_name).collect()),
    }
}

pub(super) fn push_symbol(
    symbols: &mut Vec<SymbolSourceSymbol>,
    id: String,
    kind: &str,
    name: &str,
    visibility: String,
    span: Span,
    signature: Option<SymbolSourceSignature>,
) {
    symbols.push(SymbolSourceSymbol {
        id,
        kind: kind.to_string(),
        name: name.to_string(),
        visibility,
        span: source_span(span),
        signature,
    });
}

fn collect_items(items: &[ast::Item], symbols: &mut Vec<SymbolSourceSymbol>) {
    for item in items {
        match &item.kind {
            ItemKind::Function(f) => push_symbol(
                symbols,
                format!("source:function:{}", f.name),
                "function",
                f.name.as_str(),
                visibility_name(&item.vis),
                item.span,
                Some(fn_signature(&f.sig)),
            ),
            ItemKind::Struct(s) => {
                push_symbol(
                    symbols,
                    format!("source:struct:{}", s.name),
                    "struct",
                    s.name.as_str(),
                    visibility_name(&item.vis),
                    item.span,
                    None,
                );
                collect_struct_fields(symbols, s.name.as_str(), &s.fields);
            }
            ItemKind::Enum(e) => {
                push_symbol(
                    symbols,
                    format!("source:enum:{}", e.name),
                    "enum",
                    e.name.as_str(),
                    visibility_name(&item.vis),
                    item.span,
                    None,
                );
                for variant in &e.variants {
                    let name = variant.name.as_str();
                    push_symbol(
                        symbols,
                        format!("source:enum:{}.variant:{name}", e.name),
                        "enum_variant",
                        name,
                        "private".to_string(),
                        variant.span,
                        None,
                    );
                }
            }
            ItemKind::Trait(t) => collect_trait_items(symbols, item, t),
            ItemKind::Impl(imp) => collect_impl_items(symbols, imp),
            ItemKind::TypeAlias(t) => push_symbol(
                symbols,
                format!("source:type_alias:{}", t.name),
                "type_alias",
                t.name.as_str(),
                visibility_name(&item.vis),
                item.span,
                None,
            ),
            ItemKind::Const(c) => push_symbol(
                symbols,
                format!("source:const:{}", c.name),
                "const",
                c.name.as_str(),
                visibility_name(&item.vis),
                item.span,
                None,
            ),
            ItemKind::Static(s) => push_symbol(
                symbols,
                format!("source:static:{}", s.name),
                "static",
                s.name.as_str(),
                visibility_name(&item.vis),
                item.span,
                None,
            ),
            ItemKind::Mod(m) => {
                push_symbol(
                    symbols,
                    format!("source:module:{}", m.name),
                    "module",
                    m.name.as_str(),
                    visibility_name(&item.vis),
                    item.span,
                    None,
                );
                if let Some(content) = &m.content {
                    collect_items(&content.items, symbols);
                }
            }
            ItemKind::ExternCrate(e) => push_symbol(
                symbols,
                format!("source:extern_crate:{}", e.name),
                "extern_crate",
                e.name.as_str(),
                visibility_name(&item.vis),
                item.span,
                None,
            ),
            ItemKind::ExternBlock(block) => collect_foreign_items(symbols, block),
            ItemKind::Macro(m) => {
                if let Some(name) = &m.name {
                    push_symbol(
                        symbols,
                        format!("source:macro:{name}"),
                        "macro",
                        name.as_str(),
                        visibility_name(&item.vis),
                        item.span,
                        None,
                    );
                }
            }
            ItemKind::MacroRules(m) => push_symbol(
                symbols,
                format!("source:macro:{}", m.name),
                "macro",
                m.name.as_str(),
                visibility_name(&item.vis),
                item.span,
                None,
            ),
            ItemKind::Effect(e) => collect_effect_operations(symbols, item, e),
            ItemKind::Use(_) => {}
        }
    }
}

pub(super) fn sorted_source_symbols(
    mut symbols: Vec<SymbolSourceSymbol>,
) -> Vec<SymbolSourceSymbol> {
    symbols.sort();
    symbols.dedup();
    symbols
}

pub(in crate::symbol_graph) fn collect_source_symbols(module: &Module) -> Vec<SymbolSourceSymbol> {
    let mut symbols = Vec::new();
    collect_items(&module.items, &mut symbols);
    sorted_source_symbols(symbols)
}

#[cfg(test)]
mod tests {
    use super::super::super::model::{SymbolSourceSpan, SymbolSourceSymbol};
    use super::*;

    fn source(kind: &str, name: &str) -> SymbolSourceSymbol {
        SymbolSourceSymbol {
            id: format!("source:{kind}:{name}"),
            kind: kind.to_string(),
            name: name.to_string(),
            visibility: "private".to_string(),
            span: SymbolSourceSpan { start: 0, end: 1 },
            signature: None,
        }
    }

    #[test]
    fn symbol_graph_source_symbols_sort_and_deduplicate() {
        let symbols = sorted_source_symbols(vec![
            source("function", "z"),
            source("function", "a"),
            source("function", "a"),
        ]);
        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
    }
}
