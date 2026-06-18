use std::collections::BTreeSet;

use crate::mir_representation::MirRepresentationSymbols;

use super::super::model::{SymbolGraphEdge, SymbolGraphEffectSymbol, SymbolSourceSymbol};

pub(in crate::symbol_graph) fn collect_edges(
    source: &[SymbolSourceSymbol],
    mir: &MirRepresentationSymbols,
    effects: &[SymbolGraphEffectSymbol],
) -> Vec<SymbolGraphEdge> {
    let mir_functions = mir.functions.iter().cloned().collect::<BTreeSet<_>>();
    let mir_types = mir.types.iter().cloned().collect::<BTreeSet<_>>();
    let mir_externals = mir.externals.iter().cloned().collect::<BTreeSet<_>>();
    let effect_functions = effects
        .iter()
        .map(|effect| effect.function.clone())
        .collect::<BTreeSet<_>>();
    let mut edges = Vec::new();
    for symbol in source {
        if symbol.kind == "function" && mir_functions.contains(&symbol.name) {
            edges.push(edge(
                "source_to_mir_function",
                &symbol.id,
                &format!("mir:function:{}", symbol.name),
            ));
        }
        if matches!(symbol.kind.as_str(), "struct" | "enum" | "type_alias")
            && mir_types.contains(&symbol.name)
        {
            edges.push(edge(
                "source_to_mir_type",
                &symbol.id,
                &format!("mir:type:{}", symbol.name),
            ));
        }
        if matches!(
            symbol.kind.as_str(),
            "extern_function" | "extern_static" | "extern_type"
        ) && mir_externals.contains(&symbol.name)
        {
            edges.push(edge(
                "source_to_mir_external",
                &symbol.id,
                &format!("mir:external:{}", symbol.name),
            ));
        }
        if symbol.kind == "function" && effect_functions.contains(&symbol.name) {
            edges.push(edge(
                "source_to_effect_summary",
                &symbol.id,
                &format!("effect:function:{}", symbol.name),
            ));
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

fn edge(kind: &str, from: &str, to: &str) -> SymbolGraphEdge {
    SymbolGraphEdge {
        kind: kind.to_string(),
        from: from.to_string(),
        to: to.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::model::{
        SymbolGraphEffectSymbol, SymbolSourceSpan, SymbolSourceSymbol,
    };
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
    fn symbol_graph_edges_use_only_exact_supported_matches() {
        let source = vec![
            source("function", "main"),
            source("function", "helper"),
            source("struct", "Point"),
        ];
        let mir = MirRepresentationSymbols {
            functions: vec!["main".to_string()],
            types: vec!["Point".to_string()],
            globals: vec![],
            externals: vec![],
        };
        let effects = vec![SymbolGraphEffectSymbol {
            function: "helper".to_string(),
            declared_effects: vec![],
            observed_capabilities: vec![],
            propagated_effect_sources: vec![],
        }];
        let edges = collect_edges(&source, &mir, &effects);
        assert_eq!(
            edges
                .iter()
                .map(|edge| edge.kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "source_to_effect_summary",
                "source_to_mir_function",
                "source_to_mir_type"
            ]
        );
    }
}
