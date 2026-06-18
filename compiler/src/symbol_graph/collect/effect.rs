use quantalang::types::FunctionEffectSummary;

use super::super::model::SymbolGraphEffectSymbol;
use super::super::sorted;

pub(in crate::symbol_graph) fn collect_effect_symbols(
    summaries: &[FunctionEffectSummary],
) -> Vec<SymbolGraphEffectSymbol> {
    let mut output = Vec::new();
    for summary in summaries {
        let observed_capabilities = sorted(summary.observed_capabilities.keys().cloned().collect());
        let propagated_effect_sources = sorted(
            summary
                .propagated_effects
                .iter()
                .flat_map(|(effect, sources)| {
                    sources
                        .iter()
                        .map(move |source| format!("{effect}:{source}"))
                })
                .collect(),
        );
        output.push(SymbolGraphEffectSymbol {
            function: summary.function.clone(),
            declared_effects: summary.declared_effects.clone(),
            observed_capabilities,
            propagated_effect_sources,
        });
    }
    output.sort();
    output.dedup();
    output
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    #[test]
    fn symbol_graph_effect_symbols_sort_names() {
        let mut observed = BTreeMap::new();
        observed.insert("Network".to_string(), BTreeSet::new());
        observed.insert("Console".to_string(), BTreeSet::new());
        let summaries = vec![FunctionEffectSummary {
            function: "main".to_string(),
            declared_effects: vec!["Console".to_string()],
            observed_capabilities: observed,
            propagated_effects: BTreeMap::new(),
        }];
        assert_eq!(
            collect_effect_symbols(&summaries)[0].observed_capabilities,
            vec!["Console", "Network"]
        );
    }
}
