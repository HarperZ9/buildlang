// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Effects on the C side: the string table, string locals, and the output
//! and start-up calls the backend emits. Output becomes the same events the
//! source side builds (`crate::verify::effects`), read from the C text:
//! `printf` and `fprintf(stderr, ...)` with a format string from the C
//! string table. The formatting table is part of the trusted base and is
//! stated in `docs/TRANSLATION-VALIDATION.md`.

use super::c_exec::CExec;
use super::c_parse::CExpr;
use crate::verify::effects::{btxt, dec, fmt, lit, out_event};
use crate::verify::sym::St;
use crate::verify::term::{not, Term};
use crate::verify::ty::Ty;
use std::collections::HashMap;

/// A C string literal starting at byte `i` of `s` (the opening quote):
/// its decoded text and the byte after the closing quote.
pub fn c_string(s: &str, i: usize) -> Result<(String, usize), String> {
    let mut out = String::new();
    let mut it = s[i + 1..].char_indices();
    while let Some((k, c)) = it.next() {
        match c {
            '"' => return Ok((out, i + 1 + k + 1)),
            '\\' => {
                let (_, e) = it.next().ok_or("an escape at the end of a string")?;
                out.push(match e {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '0' => '\0',
                    '\\' => '\\',
                    '"' => '"',
                    '\'' => '\'',
                    other => return Err(format!("C escape `\\{other}` outside the model")),
                });
            }
            c => out.push(c),
        }
    }
    Err("an unterminated C string".into())
}

/// The C file's string table: `static const char* __strN = "...";`.
pub fn string_table(c: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in c.lines() {
        let Some(rest) = line.strip_prefix("static const char* ") else {
            continue;
        };
        let Some((name, init)) = rest.split_once(" = ") else {
            continue;
        };
        if !init.starts_with('"') || !init.ends_with("\";") {
            continue;
        }
        if let Ok((text, end)) = c_string(init, 0) {
            if end + 1 == init.len() {
                out.insert(name.to_string(), text);
            }
        }
    }
    out
}

/// C type names that hold strings, never compared or computed with.
pub fn is_string_ty(t: &str) -> bool {
    matches!(t, "int8_t*" | "BuildString" | "const char*" | "char*")
}

/// Calls that write no output and change nothing the checker models:
/// `__build_init_io` makes stdout and stderr unbuffered, `build_args_init`
/// stores the command line, and `fflush(stdout)` writes nothing new on an
/// unbuffered stream.
fn is_silent(name: &str, args: &[CExpr]) -> bool {
    match (name, args) {
        ("__build_init_io", []) => true,
        ("build_args_init", [CExpr::Var(a), CExpr::Var(b)]) => a == "argc" && b == "argv",
        ("fflush", [CExpr::Var(s)]) => s == "stdout",
        _ => false,
    }
}

impl CExec<'_> {
    /// The value of a string expression: a literal, a string local, or a
    /// float rendered by the runtime.
    pub(super) fn str_value(&self, e: &CExpr, s: &mut St) -> Result<Term, String> {
        match e {
            CExpr::Str(t) => Ok(lit(t)),
            CExpr::Var(n) if self.strs.contains(n) => s
                .strs
                .iter()
                .rev()
                .find(|v| v.0 == *n)
                .map(|v| v.1.clone())
                .ok_or(format!("string local `{n}` read before it is set")),
            CExpr::Var(n) => self
                .strings
                .get(n)
                .map(|t| lit(t))
                .ok_or(format!("unknown C string `{n}`")),
            CExpr::Member(inner, f) if f == "ptr" => self.str_value(inner, s),
            CExpr::Call(name, args) => {
                let (kind, want) = match name.as_str() {
                    "build_f64_to_string" => ("f64", Ty::F64),
                    "build_f64_to_debug_string" => ("f64dbg", Ty::F64),
                    "build_f32_to_string" => ("f32", Ty::F32),
                    "build_f32_to_debug_string" => ("f32dbg", Ty::F32),
                    _ => return Err(format!("C string from `{name}` outside the model")),
                };
                let [a] = args.as_slice() else {
                    return Err(format!("`{name}` arity"));
                };
                let (v, ty) = self.expr(a, s, Some(want), false)?;
                if ty != Some(want) {
                    return Err(format!("mismatch: `{name}` of a value that is not {want}"));
                }
                Ok(fmt(kind, v))
            }
            _ => Err("a C string expression outside the model".into()),
        }
    }

    /// Run an effect statement. `Ok(false)` when `name` is not one.
    pub(super) fn effect(&self, name: &str, args: &[CExpr], s: &mut St) -> Result<bool, String> {
        if is_silent(name, args) {
            return Ok(true);
        }
        let (stream, rest) = match (name, args) {
            ("printf", [f, rest @ ..]) => (1u8, (f, rest)),
            ("fprintf", [CExpr::Var(e), f, rest @ ..]) if e == "stderr" => (2, (f, rest)),
            _ => return Ok(false),
        };
        let text = match self.str_value(rest.0, s)? {
            Term::App(n, a) if a.is_empty() && n.starts_with("lit$") => n[4..].to_string(),
            _ => return Err("a format string that is not a literal".into()),
        };
        let ev = self.printf(stream, &text, rest.1, s)?;
        s.event(ev);
        Ok(true)
    }

    fn printf(&self, stream: u8, text: &str, args: &[CExpr], s: &mut St) -> Result<Term, String> {
        let mut chunks = Vec::new();
        let mut buf = String::new();
        let mut args = args.iter();
        let mut it = text.chars().peekable();
        while let Some(c) = it.next() {
            if c != '%' {
                buf.push(c);
                continue;
            }
            let mut conv = String::new();
            while let Some(&d) = it.peek() {
                conv.push(d);
                it.next();
                if d.is_ascii_alphabetic() && d != 'l' || d == '%' {
                    break;
                }
            }
            if conv == "%" {
                buf.push('%');
                continue;
            }
            chunks.push(lit(&std::mem::take(&mut buf)));
            let a = args
                .next()
                .ok_or("mismatch: printf has fewer arguments than conversions")?;
            chunks.push(self.conversion(&conv, a, s)?);
        }
        chunks.push(lit(&buf));
        if args.next().is_some() {
            return Err("mismatch: printf has more arguments than conversions".into());
        }
        Ok(out_event(stream, chunks))
    }

    /// One conversion: the chunk it prints for argument `a`.
    fn conversion(&self, conv: &str, a: &CExpr, s: &mut St) -> Result<Term, String> {
        if conv == "s" {
            return match a {
                CExpr::Cond(c, t, f) => {
                    let (b, ty) = self.expr(c, s, Some(Ty::Bool), false)?;
                    if ty != Some(Ty::Bool) {
                        return Err("a bool rendering of a non-bool".into());
                    }
                    match (&**t, &**f) {
                        (CExpr::Str(x), CExpr::Str(y)) if x == "true" && y == "false" => {
                            Ok(btxt(b))
                        }
                        (CExpr::Str(x), CExpr::Str(y)) if x == "false" && y == "true" => {
                            Ok(btxt(not(b)))
                        }
                        _ => {
                            Err("mismatch: a bool printed as text other than true or false".into())
                        }
                    }
                }
                _ => self.str_value(a, s),
            };
        }
        if let Some(n) = conv.strip_prefix('.').and_then(|r| r.strip_suffix('f')) {
            let n: usize = n.parse().map_err(|_| format!("C conversion `%{conv}`"))?;
            let (v, ty) = self.expr(a, s, Some(Ty::F64), false)?;
            if !ty.is_some_and(|t| t.is_float()) {
                return Err(format!(
                    "mismatch: `%{conv}` of a value that is not a float"
                ));
            }
            return Ok(fmt(&format!(".{n}"), v));
        }
        let fits = |ty: Option<Ty>| match (conv, ty) {
            ("d", Some(Ty::Int { bits, signed: true })) => bits <= 32,
            (
                "d",
                Some(Ty::Int {
                    bits,
                    signed: false,
                }),
            ) => bits <= 16,
            (
                "u",
                Some(Ty::Int {
                    bits,
                    signed: false,
                }),
            ) => bits <= 32,
            ("lld", Some(Ty::Int { bits: 64, signed })) => signed,
            ("llu", Some(Ty::Int { bits: 64, signed })) => !signed,
            _ => false,
        };
        if !matches!(conv, "d" | "u" | "lld" | "llu") {
            return Err(format!("C conversion `%{conv}` outside the model"));
        }
        let lit_ty = match (conv, a) {
            (_, CExpr::Int(_)) => Some(match conv {
                "d" => Ty::I32,
                "lld" => Ty::I64,
                "u" => Ty::Int {
                    bits: 32,
                    signed: false,
                },
                _ => Ty::Int {
                    bits: 64,
                    signed: false,
                },
            }),
            _ => None,
        };
        let (v, ty) = self.expr(a, s, lit_ty, false)?;
        let ty = ty.or(lit_ty);
        if let (Some(t), Term::Int(k)) = (ty, &v) {
            if !t.holds(*k) {
                return Err(format!("mismatch: `%{conv}` of a literal out of range"));
            }
        }
        if !fits(ty) {
            return Err(format!(
                "mismatch: `%{conv}` of a value of another width or sign"
            ));
        }
        Ok(dec(v))
    }
}
