use super::model::{ModuleGraphDigest, ModuleGraphEdge, ModuleGraphInput, ModuleGraphProgram};
use super::{program_known_gaps, summarize_programs};

fn digest() -> ModuleGraphDigest {
    ModuleGraphDigest {
        algorithm: "sha256".to_string(),
        hex: "0".repeat(64),
    }
}

#[test]
fn summary_sorts_and_deduplicates_roles_and_edges() {
    let programs = vec![
        ModuleGraphProgram {
            id: "left".to_string(),
            path: "programs/left.bld".to_string(),
            source_digest: digest(),
            input_graph_digest: digest(),
            module_graph_digest: digest(),
            inputs: vec![
                ModuleGraphInput {
                    id: "input:module:programs/shared.bld".to_string(),
                    role: "module".to_string(),
                    path: "programs/shared.bld".to_string(),
                    source_digest: digest(),
                },
                ModuleGraphInput {
                    id: "input:entry:programs/left.bld".to_string(),
                    role: "entry".to_string(),
                    path: "programs/left.bld".to_string(),
                    source_digest: digest(),
                },
            ],
            edges: vec![
                ModuleGraphEdge {
                    kind: "program_module".to_string(),
                    from: "program:left".to_string(),
                    to: "input:module:programs/shared.bld".to_string(),
                },
                ModuleGraphEdge {
                    kind: "program_entry".to_string(),
                    from: "program:left".to_string(),
                    to: "input:entry:programs/left.bld".to_string(),
                },
            ],
            known_gaps: program_known_gaps(),
        },
        ModuleGraphProgram {
            id: "right".to_string(),
            path: "programs/right.bld".to_string(),
            source_digest: digest(),
            input_graph_digest: digest(),
            module_graph_digest: digest(),
            inputs: vec![ModuleGraphInput {
                id: "input:entry:programs/right.bld".to_string(),
                role: "entry".to_string(),
                path: "programs/right.bld".to_string(),
                source_digest: digest(),
            }],
            edges: vec![ModuleGraphEdge {
                kind: "program_entry".to_string(),
                from: "program:right".to_string(),
                to: "input:entry:programs/right.bld".to_string(),
            }],
            known_gaps: program_known_gaps(),
        },
    ];
    let summary = summarize_programs(&programs);
    assert_eq!(summary.input_roles, vec!["entry", "module"]);
    assert_eq!(summary.edge_kinds, vec!["program_entry", "program_module"]);
    assert_eq!(summary.input_count, 3);
}
