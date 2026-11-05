// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The first BuildDB transform: rename a free function and every reference.
//!
//! The edit is made on the source, token by token, and then checked on MIR:
//! the renamed program is lowered again, and its MIR must equal the original
//! MIR with only the function name substituted. A local, field or method that
//! shares the old name would change other parts of MIR, so such a rename is
//! refused instead of applied. The receipt records the digests before and
//! after, and every edited span.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::codegen::{MirModule, MirRValue, MirStmtKind, MirTerminator, MirValue};
use crate::lexer::{Lexer, SourceFile, TokenKind};
use crate::types::FunctionEffectSummary;

use super::drift::rename_rows;
use super::store::{lower_source, mir_sha256, sha256_hex, Store};
use super::BqlError;

/// Schema tag of the rename receipt.
pub const RENAME_RECEIPT_SCHEMA: &str = "buildlang.bql-transform-receipt/v0";

/// What a rename did and how it was checked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameReceipt {
    pub schema: String,
    pub transform: String,
    pub old_name: String,
    pub new_name: String,
    /// Byte spans `[start, end)` replaced in the original source.
    pub edits: Vec<(u32, u32)>,
    pub source_sha256_before: String,
    pub source_sha256_after: String,
    pub mir_sha256_before: String,
    pub mir_sha256_after: String,
    pub store_sha256_before: String,
    pub store_sha256_after: String,
    /// The check that passed: MIR after equals MIR before with the name substituted.
    pub check: String,
    /// Whether the incremental store (old rows renamed, see `drift::rename_rows`)
    /// equals the rebuilt store. The rebuild is the authority either way.
    pub store_incremental_agrees: bool,
}

/// A checked rename with both stores, for callers that measure the incremental path.
pub struct RenameOutcome {
    pub source: String,
    pub receipt: RenameReceipt,
    pub store_incremental: Store,
    pub store_rebuilt: Store,
}

fn swap(name: &mut Arc<str>, old: &str, new: &str) {
    if &**name == old {
        *name = Arc::from(new);
    }
}

fn swap_value(v: &mut MirValue, old: &str, new: &str) {
    match v {
        // Direct calls name their callee as `Global`, function values as `Function`.
        MirValue::Function(n) | MirValue::Global(n) => swap(n, old, new),
        MirValue::Local(_) | MirValue::Const(_) => {}
    }
}

fn swap_rvalue(rv: &mut MirRValue, old: &str, new: &str) {
    let s = |v: &mut MirValue| swap_value(v, old, new);
    match rv {
        MirRValue::Use(v) => s(v),
        MirRValue::BinaryOp { left, right, .. } => {
            s(left);
            s(right);
        }
        MirRValue::UnaryOp { operand, .. } => s(operand),
        MirRValue::Cast { value, .. } | MirRValue::Repeat { value, .. } => s(value),
        MirRValue::Aggregate { operands, .. } => operands.iter_mut().for_each(s),
        MirRValue::FieldAccess { base, .. } | MirRValue::VariantField { base, .. } => s(base),
        MirRValue::IndexAccess { base, index, .. } => {
            s(base);
            s(index);
        }
        MirRValue::Deref { ptr, .. } => s(ptr),
        MirRValue::TextureSample {
            texture,
            sampler,
            coords,
        } => {
            s(texture);
            s(sampler);
            s(coords);
        }
        MirRValue::Ref { .. }
        | MirRValue::AddressOf { .. }
        | MirRValue::Discriminant(_)
        | MirRValue::Len(_)
        | MirRValue::NullaryOp(..) => {}
    }
}

/// Substitute a function name everywhere MIR refers to a function by name.
pub fn rename_in_mir(module: &mut MirModule, old: &str, new: &str) {
    for f in &mut module.functions {
        swap(&mut f.name, old, new);
        for block in f.blocks.iter_mut().flatten() {
            for stmt in &mut block.stmts {
                match &mut stmt.kind {
                    MirStmtKind::Assign { value, .. }
                    | MirStmtKind::DerefAssign { value, .. }
                    | MirStmtKind::FieldDerefAssign { value, .. }
                    | MirStmtKind::FieldAssign { value, .. }
                    | MirStmtKind::GlobalStore { value, .. } => swap_rvalue(value, old, new),
                    MirStmtKind::IndexStore {
                        base, index, value, ..
                    } => {
                        swap_value(base, old, new);
                        swap_value(index, old, new);
                        swap_rvalue(value, old, new);
                    }
                    MirStmtKind::WorkgroupBarrier
                    | MirStmtKind::StorageLive(_)
                    | MirStmtKind::StorageDead(_)
                    | MirStmtKind::Nop => {}
                }
            }
            if let Some(term) = &mut block.terminator {
                match term {
                    MirTerminator::If { cond, .. } | MirTerminator::Assert { cond, .. } => {
                        swap_value(cond, old, new)
                    }
                    MirTerminator::Switch { value, .. } => swap_value(value, old, new),
                    MirTerminator::Call { func, args, .. } => {
                        swap_value(func, old, new);
                        args.iter_mut().for_each(|a| swap_value(a, old, new));
                    }
                    MirTerminator::Return(Some(v)) => swap_value(v, old, new),
                    MirTerminator::Return(None)
                    | MirTerminator::Goto(_)
                    | MirTerminator::Unreachable
                    | MirTerminator::Drop { .. }
                    | MirTerminator::Resume
                    | MirTerminator::Abort => {}
                }
            }
        }
    }
    for vt in &mut module.vtables {
        for (_, impl_fn, _) in &mut vt.methods {
            swap(impl_fn, old, new);
        }
    }
}

fn rename_in_summaries(summaries: &mut [FunctionEffectSummary], old: &str, new: &str) {
    for s in summaries {
        if s.function == old {
            s.function = new.to_string();
        }
        for sources in s.propagated_effects.values_mut() {
            if sources.remove(old) {
                sources.insert(new.to_string());
            }
        }
    }
}

fn check_identifier(new: &str) -> Result<(), BqlError> {
    let file = SourceFile::new("<new-name>", new.to_string());
    let tokens = Lexer::new(&file)
        .tokenize()
        .map_err(|e| BqlError::Refused(format!("'{new}' is not an identifier: {e}")))?;
    let real: Vec<_> = tokens.iter().filter(|t| !t.is_eof()).collect();
    match real.as_slice() {
        [t] if t.kind == TokenKind::Ident && t.span.end.to_usize() == new.len() => Ok(()),
        _ => Err(BqlError::Refused(format!(
            "'{new}' is not a plain identifier (keywords and raw identifiers are refused)"
        ))),
    }
}

/// Rename the free function `old` to `new` in a single-file program.
///
/// Returns the rewritten source and a receipt. Fails closed when the target is
/// not a defined top-level function, when `new` is not a free identifier, or
/// when the MIR check finds any change beyond the name.
pub fn rename_function(
    module_name: &str,
    source: &str,
    old: &str,
    new: &str,
) -> Result<(String, RenameReceipt), BqlError> {
    rename_checked(module_name, source, old, new).map(|o| (o.source, o.receipt))
}

/// [`rename_function`], returning the incremental and rebuilt stores as well.
pub fn rename_checked(
    module_name: &str,
    source: &str,
    old: &str,
    new: &str,
) -> Result<RenameOutcome, BqlError> {
    let before = lower_source(module_name, source)?;
    let store_before = Store::from_lowered(module_name, &before)?;
    let row = store_before
        .function(old)
        .filter(|f| f.kind == "defined" && f.name_span.is_some())
        .ok_or_else(|| {
            BqlError::Refused(format!(
                "'{old}' is not a function defined at the top level"
            ))
        })?;
    if old == "main" {
        return Err(BqlError::Refused(
            "renaming main changes the entry point".into(),
        ));
    }
    let def_span = row.name_span.expect("filtered above");
    check_identifier(new)?;

    let file = SourceFile::new(module_name.to_string(), source.to_string());
    let tokens = Lexer::new(&file)
        .tokenize()
        .map_err(|e| BqlError::Compile(format!("lexer: {e}")))?;
    let mut edits = Vec::new();
    for t in &tokens {
        if t.kind != TokenKind::Ident {
            continue;
        }
        let (s, e) = (t.span.start.0, t.span.end.0);
        let text = &source[s as usize..e as usize];
        if text == new {
            return Err(BqlError::Refused(format!(
                "'{new}' already appears in the program at byte {s}; a rename could capture it"
            )));
        }
        if text == old {
            edits.push((s, e));
        }
    }
    if !edits.contains(&def_span) {
        return Err(BqlError::Refused(
            "the definition's name token was not found among the edits".into(),
        ));
    }

    let mut after_src = source.to_string();
    for (s, e) in edits.iter().rev() {
        after_src.replace_range(*s as usize..*e as usize, new);
    }
    let after = lower_source(module_name, &after_src)
        .map_err(|e| BqlError::Refused(format!("the renamed program does not compile: {e}")))?;

    let mut expected = before.mir.clone();
    rename_in_mir(&mut expected, old, new);
    if expected != after.mir {
        return Err(BqlError::Refused(format!(
            "MIR changed beyond the name; '{old}' is also used for something other than \
             the function (a local, field, method or generated name)"
        )));
    }
    let mut expected_summaries = before.summaries.clone();
    rename_in_summaries(&mut expected_summaries, old, new);
    if expected_summaries != after.summaries {
        return Err(BqlError::Refused(
            "effect summaries changed beyond the name".into(),
        ));
    }
    let store_after = Store::from_lowered(module_name, &after)?;
    let source_sha256_after = sha256_hex(after_src.as_bytes());
    let mir_sha256_after = mir_sha256(&expected)?;
    let store_incremental = rename_rows(
        &store_before,
        old,
        new,
        &edits,
        &source_sha256_after,
        &mir_sha256_after,
    );
    let receipt = RenameReceipt {
        schema: RENAME_RECEIPT_SCHEMA.to_string(),
        transform: "rename-function".to_string(),
        old_name: old.to_string(),
        new_name: new.to_string(),
        edits,
        source_sha256_before: sha256_hex(source.as_bytes()),
        source_sha256_after,
        mir_sha256_before: mir_sha256(&before.mir)?,
        mir_sha256_after,
        store_sha256_before: store_before.digest(),
        store_sha256_after: store_after.digest(),
        check: "mir-equal-under-rename".to_string(),
        store_incremental_agrees: store_incremental == store_after,
    };
    Ok(RenameOutcome {
        source: after_src,
        receipt,
        store_incremental,
        store_rebuilt: store_after,
    })
}
