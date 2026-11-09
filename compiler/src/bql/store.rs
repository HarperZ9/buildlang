// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The BuildDB store: one compiled module held as relational tables.
//!
//! Every table is derived from compiler data, never from a second parser:
//! the AST gives definitions and their spans, the type checker gives the
//! effect summaries, and MIR gives the call graph and linkage. Rows are kept
//! sorted, so the JSON form and its digest are deterministic.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ast::{ItemKind, Module, Visibility};
use crate::codegen::{MirLowerer, MirModule, MirModuleEnvelope, MirTerminator, MirValue};
use crate::lexer::{Lexer, SourceFile};
use crate::parser::Parser;
use crate::types::{FunctionEffectSummary, TypeChecker, TypeContext};

use super::BqlError;

/// Schema tag of the serialized store.
pub const BDB_SCHEMA: &str = "buildlang.bdb/v1";

/// A function defined or declared in the module.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FunctionRow {
    pub name: String,
    /// `defined` (has a body in MIR) or `extern` (declaration only).
    pub kind: String,
    pub public: bool,
    /// Byte span of the name in the entry source, when the AST has it.
    pub name_span: Option<(u32, u32)>,
    pub shader_stage: Option<String>,
}

/// One effect fact. `how` is `declared`, `observed` or `propagated`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EffectRow {
    pub function: String,
    pub effect: String,
    pub how: String,
    /// The call or callee that produced an observed or propagated effect.
    pub via: Option<String>,
}

/// One direct call edge from a MIR `Call` terminator.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CallRow {
    pub caller: String,
    /// The callee name, or `<indirect>` when MIR calls through a local.
    pub callee: String,
}

/// A named type, global or other top-level definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemRow {
    pub name: String,
    pub kind: String,
}

/// One resolved use of a module-scope name: a top-level function, constant,
/// static or extern, as the type checker bound it. A use that a local,
/// parameter or pattern shadows is a use of that binding and is not here.
/// `role` is `call` for the callee of a call and `value` otherwise.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RefRow {
    pub symbol: String,
    pub span: (u32, u32),
    pub role: String,
}

/// The program as a database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store {
    pub schema: String,
    pub module: String,
    pub source_sha256: String,
    pub mir_sha256: String,
    pub functions: Vec<FunctionRow>,
    pub effects: Vec<EffectRow>,
    pub calls: Vec<CallRow>,
    pub items: Vec<ItemRow>,
    /// Added in `buildlang.bdb/v1`; a v0 store reads with none.
    #[serde(default)]
    pub refs: Vec<RefRow>,
}

/// The compiler products a store is built from.
pub struct Lowered {
    pub source: Arc<str>,
    pub ast: Module,
    pub summaries: Vec<FunctionEffectSummary>,
    pub mir: MirModule,
    /// Resolved module-scope references from the type checker.
    pub refs: Vec<RefRow>,
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn mir_sha256(mir: &MirModule) -> Result<String, BqlError> {
    let json = MirModuleEnvelope::wrap(mir)
        .to_json()
        .map_err(|e| BqlError::Compile(format!("MIR did not serialize: {e}")))?;
    Ok(sha256_hex(json.as_bytes()))
}

/// Run lex, parse, type check and MIR lowering over one self-contained file.
///
/// A program that pulls in other files (`use`, `mod name;`, `include!`) is
/// refused: the store would hold only part of it, and a rename would miss
/// references in the other files.
pub fn lower_source(name: &str, source: &str) -> Result<Lowered, BqlError> {
    let source_file = SourceFile::new(name.to_string(), source.to_string());
    let tokens = Lexer::new(&source_file)
        .tokenize()
        .map_err(|e| BqlError::Compile(format!("lexer: {e}")))?;
    let mut parser = Parser::new(&source_file, tokens);
    let ast = parser
        .parse()
        .map_err(|e| BqlError::Compile(format!("parser: {e}")))?;
    if !parser.errors().is_empty() {
        let errors: Vec<String> = parser.errors().iter().map(ToString::to_string).collect();
        return Err(BqlError::Compile(format!("parser: {}", errors.join("; "))));
    }
    for item in &ast.items {
        let outside = match &item.kind {
            ItemKind::Use(_) | ItemKind::ExternCrate(_) => true,
            ItemKind::Mod(m) => m.content.is_none(),
            _ => false,
        };
        if outside {
            return Err(BqlError::Unsupported(
                "the program reads other files (use, extern crate or mod NAME;); \
                 the v0 store holds single-file programs only"
                    .to_string(),
            ));
        }
    }
    if source.contains("include!") || source.contains("include_str!") {
        return Err(BqlError::Unsupported(
            "the program includes other files; the v0 store holds single-file programs only"
                .to_string(),
        ));
    }
    let mut ctx = TypeContext::new();
    let mut checker = TypeChecker::new(&mut ctx);
    checker.set_source_file(&source_file);
    checker.check_module(&ast);
    if checker.has_errors() {
        let errors: Vec<String> = checker.errors().iter().map(ToString::to_string).collect();
        return Err(BqlError::Compile(format!(
            "type check: {}",
            errors.join("; ")
        )));
    }
    let summaries = checker.function_effect_summaries().to_vec();
    drop(checker);
    let refs: BTreeSet<RefRow> = ctx
        .module_refs()
        .iter()
        .map(|(name, start, end, call)| RefRow {
            symbol: name.to_string(),
            span: (*start, *end),
            role: if *call { "call" } else { "value" }.to_string(),
        })
        .collect();
    let text: Arc<str> = Arc::from(source_file.source());
    let mir = MirLowerer::with_source(&ctx, text.clone())
        .lower_module(&ast)
        .map_err(|e| BqlError::Compile(format!("MIR lowering: {e}")))?;
    Ok(Lowered {
        source: text,
        ast,
        summaries,
        mir,
        refs: refs.into_iter().collect(),
    })
}

impl Store {
    /// Load a single-file program into a store.
    pub fn load_source(name: &str, source: &str) -> Result<Store, BqlError> {
        Store::from_lowered(name, &lower_source(name, source)?)
    }

    /// Build the tables from compiler products.
    pub fn from_lowered(name: &str, lowered: &Lowered) -> Result<Store, BqlError> {
        let mut spans: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        let mut public: BTreeSet<String> = BTreeSet::new();
        let mut items = BTreeSet::new();
        for item in &lowered.ast.items {
            let Some(ident) = item.name() else { continue };
            let item_name = ident.name.to_string();
            let kind = match &item.kind {
                ItemKind::Function(_) => {
                    spans.insert(item_name.clone(), (ident.span.start.0, ident.span.end.0));
                    if !matches!(item.vis, Visibility::Private) {
                        public.insert(item_name.clone());
                    }
                    continue;
                }
                ItemKind::Struct(_) => "struct",
                ItemKind::Enum(_) => "enum",
                ItemKind::Trait(_) => "trait",
                ItemKind::TypeAlias(_) => "type_alias",
                ItemKind::Const(_) => "const",
                ItemKind::Static(_) => "static",
                ItemKind::Effect(_) => "effect",
                ItemKind::Mod(_) => "module",
                _ => "other",
            };
            items.insert(ItemRow {
                name: item_name,
                kind: kind.to_string(),
            });
        }

        let mut functions = BTreeSet::new();
        let mut calls = BTreeSet::new();
        for f in &lowered.mir.functions {
            let fname = f.name.to_string();
            functions.insert(FunctionRow {
                name: fname.clone(),
                kind: if f.blocks.is_some() {
                    "defined"
                } else {
                    "extern"
                }
                .to_string(),
                public: f.is_public || public.contains(&fname),
                name_span: spans.get(&fname).copied(),
                shader_stage: f.shader_stage.map(|s| format!("{s:?}")),
            });
            for block in f.blocks.iter().flatten() {
                if let Some(MirTerminator::Call { func, .. }) = &block.terminator {
                    let callee = match func {
                        MirValue::Function(n) | MirValue::Global(n) => n.to_string(),
                        _ => "<indirect>".to_string(),
                    };
                    calls.insert(CallRow {
                        caller: fname.clone(),
                        callee,
                    });
                }
            }
        }

        let mut effects = BTreeSet::new();
        for s in &lowered.summaries {
            for e in &s.declared_effects {
                effects.insert(EffectRow {
                    function: s.function.clone(),
                    effect: e.clone(),
                    how: "declared".to_string(),
                    via: None,
                });
            }
            for (how, map) in [
                ("observed", &s.observed_capabilities),
                ("propagated", &s.propagated_effects),
            ] {
                for (effect, sources) in map {
                    for via in sources {
                        effects.insert(EffectRow {
                            function: s.function.clone(),
                            effect: effect.clone(),
                            how: how.to_string(),
                            via: Some(via.clone()),
                        });
                    }
                }
            }
        }

        Ok(Store {
            schema: BDB_SCHEMA.to_string(),
            module: name.to_string(),
            source_sha256: sha256_hex(lowered.source.as_bytes()),
            mir_sha256: mir_sha256(&lowered.mir)?,
            functions: functions.into_iter().collect(),
            effects: effects.into_iter().collect(),
            calls: calls.into_iter().collect(),
            items: items.into_iter().collect(),
            refs: lowered.refs.clone(),
        })
    }

    /// Canonical JSON form of the store.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("store rows are plain data")
    }

    /// Digest of the canonical JSON form.
    pub fn digest(&self) -> String {
        sha256_hex(self.to_json().as_bytes())
    }

    /// The function row with this name, if any.
    pub fn function(&self, name: &str) -> Option<&FunctionRow> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Every effect a function carries, from any source.
    pub fn effects_of(&self, function: &str) -> BTreeSet<&str> {
        self.effects
            .iter()
            .filter(|e| e.function == function)
            .map(|e| e.effect.as_str())
            .collect()
    }

    /// Direct callees of a function.
    pub fn callees_of(&self, function: &str) -> BTreeSet<&str> {
        self.calls
            .iter()
            .filter(|c| c.caller == function)
            .map(|c| c.callee.as_str())
            .collect()
    }
}
