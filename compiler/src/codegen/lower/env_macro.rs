// ===============================================================================
// BUILDLANG CODEGEN - COMPILE-TIME ENVIRONMENT MACRO
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
// ===============================================================================

//! Lowers `env!("NAME")` and `env!("NAME", "message")`.
//!
//! The compiler reads the variable from its own environment while it compiles
//! and puts the value in the program as a string. The value is fixed at that
//! moment: the built program does not read the variable again. A variable
//! that is not set is a compile error, with `message` as its text when one is
//! given.
//!
//! The value is copied into the output, so a secret passed through `env!` is
//! readable in the generated C and in the binary. The `Environment` effect
//! and the check receipt record every use.

use crate::ast::{self, ExprKind, Literal};
use crate::codegen::backend::CodegenResult;
use crate::codegen::ir::MirValue;
use crate::lexer::SourceFile;
use crate::parser::Parser;

use super::asserts::{inner, rejected, split_commas, trees_span};
use super::MirLowerer;

const HELP: &str = "write `env!(\"NAME\")` or `env!(\"NAME\", \"message when it is not set\")`";

/// The value `env!` puts in the program for `name`, or why it cannot.
pub(super) fn resolve(
    name: &str,
    custom: Option<&str>,
    found: Result<String, std::env::VarError>,
    loc: Option<(u32, u32)>,
) -> CodegenResult<String> {
    match found {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Err(rejected(
            match custom {
                Some(message) => message.to_string(),
                None => format!("environment variable `{name}` is not set at compile time"),
            },
            loc,
            "set the variable before running buildc, or read it at run time with `getenv`",
        )),
        Err(std::env::VarError::NotUnicode(_)) => Err(rejected(
            format!("environment variable `{name}` does not hold valid Unicode text"),
            loc,
            "`env!` produces a string, so the value must be valid Unicode",
        )),
    }
}

impl<'ctx> MirLowerer<'ctx> {
    /// One macro argument, which must be a plain string literal.
    fn env_string_arg(&self, trees: &[ast::TokenTree]) -> CodegenResult<String> {
        let loc = trees_span(trees);
        let bad = || {
            rejected(
                "each argument of `env!` must be a string literal".to_string(),
                loc,
                HELP,
            )
        };
        let (start, end) = loc.ok_or_else(bad)?;
        let source = self.source.as_ref().ok_or_else(bad)?;
        let text = source.get(start as usize..end as usize).ok_or_else(bad)?;
        let tokens = crate::lexer::tokenize(text).map_err(|_| bad())?;
        let file = SourceFile::anonymous(text);
        let expr = Parser::new(&file, tokens)
            .parse_standalone_expr()
            .ok_or_else(bad)?;
        match expr.kind {
            ExprKind::Literal(Literal::Str { value, .. }) => Ok(value),
            _ => Err(bad()),
        }
    }

    /// Lower `env!`: read the variable now and emit its value as a string.
    pub(super) fn lower_env_macro(&mut self, tokens: &[ast::TokenTree]) -> CodegenResult<MirValue> {
        let loc = trees_span(tokens);
        let parts = split_commas(inner(tokens));
        if parts.is_empty() || parts.len() > 2 {
            return Err(rejected(
                format!(
                    "`env!` takes one or two arguments, and {} were given",
                    parts.len()
                ),
                loc,
                HELP,
            ));
        }
        let name = self.env_string_arg(parts[0])?;
        let custom = match parts.get(1) {
            Some(part) => Some(self.env_string_arg(part)?),
            None => None,
        };
        if name.is_empty() || name.contains('=') || name.contains('\0') {
            return Err(rejected(
                format!("`{name}` is not a valid environment variable name"),
                loc,
                HELP,
            ));
        }
        let value = resolve(&name, custom.as_deref(), std::env::var(&name), loc)?;
        if value.contains('\0') {
            return Err(rejected(
                format!("environment variable `{name}` holds a NUL byte"),
                loc,
                "a string in the program cannot carry a NUL byte",
            ));
        }
        self.lower_literal(&Literal::Str {
            value,
            is_raw: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use std::env::VarError;

    fn message(err: crate::codegen::backend::CodegenError) -> String {
        match err {
            crate::codegen::backend::CodegenError::Rejected { message, .. } => message,
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn a_set_variable_gives_its_value() {
        assert_eq!(
            resolve("X", None, Ok("v 1".into()), None).unwrap(),
            "v 1".to_string()
        );
    }

    #[test]
    fn an_unset_variable_is_an_error_naming_it() {
        let err = resolve("BL_X", None, Err(VarError::NotPresent), None).unwrap_err();
        assert_eq!(
            message(err),
            "environment variable `BL_X` is not set at compile time"
        );
    }

    #[test]
    fn the_custom_message_replaces_the_default() {
        let err = resolve("BL_X", Some("set BL_X"), Err(VarError::NotPresent), None).unwrap_err();
        assert_eq!(message(err), "set BL_X");
    }

    #[test]
    fn a_value_that_is_not_unicode_is_an_error() {
        let err = resolve("BL_X", None, Err(VarError::NotUnicode("x".into())), None).unwrap_err();
        assert!(message(err).contains("valid Unicode"));
    }
}
