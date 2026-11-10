//! Lowers machine-level access to the reserved calls the C backend turns into
//! single statements (see `backend/c/lowlevel.rs`):
//!
//! - `ptr::read_volatile(p)` and `ptr::write_volatile(p, v)`, also spelled
//!   `p.read_volatile()` and `p.write_volatile(v)`, for a raw pointer to an
//!   integer. Each is one C `volatile` access, so the C compiler keeps every
//!   read and write a program makes to device memory.
//! - `asm!("...", "...")` with string templates only. Operands and options
//!   are a compile error that names the line; the macro is never dropped.

use std::sync::Arc;

use crate::ast;
use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::builder::values;
use crate::codegen::ir::*;
use crate::lexer::{LiteralKind, TokenKind};

use super::MirLowerer;

const VOLATILE_READ: &str = "bl_volatile_read";
const VOLATILE_WRITE: &str = "bl_volatile_write";
const INLINE_ASM: &str = "bl_inline_asm";

/// `Some(true)` for a volatile read path, `Some(false)` for a write.
pub(super) fn volatile_path(segs: &[&str]) -> Option<bool> {
    let tail = match segs {
        ["std" | "core", "ptr", f] | ["ptr", f] => *f,
        [f] => f.strip_prefix("ptr_")?,
        _ => return None,
    };
    match tail {
        "read_volatile" => Some(true),
        "write_volatile" => Some(false),
        _ => None,
    }
}

fn rejected(message: String, location: Option<(u32, u32)>, help: &str) -> CodegenError {
    CodegenError::Rejected {
        message,
        location,
        help: Some(help.to_string()),
    }
}

fn no_fn() -> CodegenError {
    CodegenError::Internal("No current function".to_string())
}

impl<'ctx> MirLowerer<'ctx> {
    /// Lower a volatile read (`is_read`) or write whose pointer and value
    /// are already lowered. `args` holds the value for a write.
    pub(super) fn lower_volatile(
        &mut self,
        is_read: bool,
        ptr: MirValue,
        args: Vec<MirValue>,
    ) -> CodegenResult<MirValue> {
        let pointee = match self.type_of_value(&ptr) {
            MirType::Ptr(inner) if matches!(*inner, MirType::Int(..)) => *inner,
            other => {
                return Err(rejected(
                    format!("volatile access needs a raw pointer to an integer, found `{other:?}`"),
                    None,
                    "use a `*const T` or `*mut T` where T is u8, u16, u32, u64, usize or a signed width",
                ))
            }
        };
        if !is_read && args.len() != 1 {
            return Err(rejected(
                "`write_volatile` takes a pointer and one value".to_string(),
                None,
                "write `ptr::write_volatile(p, v)` or `p.write_volatile(v)`",
            ));
        }
        let builder = self.current_fn.as_mut().ok_or_else(no_fn)?;
        let cont = builder.create_block();
        if is_read {
            let result = builder.create_local(pointee);
            builder.call(
                MirValue::Function(Arc::from(VOLATILE_READ)),
                vec![ptr],
                Some(result),
                cont,
            );
            builder.switch_to_block(cont);
            return Ok(values::local(result));
        }
        let mut call_args = vec![ptr];
        call_args.extend(args);
        builder.call(
            MirValue::Function(Arc::from(VOLATILE_WRITE)),
            call_args,
            None,
            cont,
        );
        builder.switch_to_block(cont);
        Ok(values::unit())
    }

    /// Lower `ptr::read_volatile(p)` or `ptr::write_volatile(p, v)`.
    pub(super) fn lower_volatile_call(
        &mut self,
        is_read: bool,
        args: &[ast::Expr],
    ) -> CodegenResult<MirValue> {
        let want = if is_read { 1 } else { 2 };
        if args.len() != want {
            return Err(rejected(
                format!(
                    "volatile access takes {want} argument(s), found {}",
                    args.len()
                ),
                None,
                "write `ptr::read_volatile(p)` or `ptr::write_volatile(p, v)`",
            ));
        }
        let ptr = self.lower_expr(&args[0])?;
        let rest = match args.get(1) {
            Some(v) => vec![self.lower_expr(v)?],
            None => Vec::new(),
        };
        self.lower_volatile(is_read, ptr, rest)
    }

    /// Lower `asm!` with string-literal templates and nothing else.
    pub(super) fn lower_asm_macro(&mut self, tokens: &[ast::TokenTree]) -> CodegenResult<()> {
        let mut templates: Vec<u32> = Vec::new();
        let mut expect_string = true;
        for item in flatten(tokens) {
            let tok = match item {
                Ok(tok) => tok,
                Err((start, end)) => return Err(asm_rejected(start, end)),
            };
            let is_str = matches!(
                &tok.kind,
                TokenKind::Literal {
                    kind: LiteralKind::Str { .. },
                    ..
                }
            );
            if expect_string && is_str {
                let text = self.extract_string_from_tokens(&[ast::TokenTree::Token(tok.clone())]);
                templates.push(self.module.intern_string(Arc::<str>::from(text)));
                expect_string = false;
            } else if !expect_string && matches!(tok.kind, TokenKind::Comma) {
                expect_string = true;
            } else {
                return Err(asm_rejected(tok.span.start.0, tok.span.end.0));
            }
        }
        if templates.is_empty() {
            return Err(rejected(
                "`asm!` needs at least one string-literal template".to_string(),
                None,
                "write `asm!(\"hlt\")`",
            ));
        }
        let args = templates
            .into_iter()
            .map(|idx| MirValue::Const(MirConst::Str(idx)))
            .collect();
        let builder = self.current_fn.as_mut().ok_or_else(no_fn)?;
        let cont = builder.create_block();
        builder.call(MirValue::Function(Arc::from(INLINE_ASM)), args, None, cont);
        builder.switch_to_block(cont);
        Ok(())
    }
}

fn asm_rejected(start: u32, end: u32) -> CodegenError {
    rejected(
        "`asm!` operands and options are not supported yet; only string-literal templates are"
            .to_string(),
        Some((start, end)),
        "keep the templates as string literals, such as `asm!(\"cli\", \"hlt\")`, \
         or move the code into an `extern \"C\"` function written in assembly",
    )
}

/// The tokens of an `asm!` call with its outer parentheses removed. A
/// nested group (part of an operand such as `in(reg) x`) is `Err` with its
/// byte range, so it is reported as unsupported.
fn flatten(tokens: &[ast::TokenTree]) -> Vec<Result<crate::lexer::Token, (u32, u32)>> {
    let inner: &[ast::TokenTree] = match tokens {
        [ast::TokenTree::Delimited { tokens, .. }] => tokens,
        _ => tokens,
    };
    inner
        .iter()
        .map(|tt| match tt {
            ast::TokenTree::Token(t) => Ok(t.clone()),
            ast::TokenTree::Delimited { span, .. } => Err((span.start.0, span.end.0)),
        })
        .collect()
}
