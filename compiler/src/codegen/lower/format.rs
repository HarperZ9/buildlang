// ===============================================================================
// BUILDLANG CODEGEN - FORMAT STRINGS
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Format strings for `println!`, `print!`, `eprintln!`, `eprint!` and `format!`.
//!
//! A format string is parsed into literal text and placeholders, each placeholder
//! is resolved to an argument (next positional, `{0}`, or a captured name such as
//! `{x}`), and each is turned into a C `printf` conversion that prints what Rust
//! would print. A spec the C backend cannot honour is rejected at check time with a
//! hint. The old path printed `{x}` literally, ignored widths and alignment, fell
//! back to `%d` for any spec it did not know, and printed `{:?}` of a vector as
//! `i32(<pointer>)`: programs that compiled and printed the wrong thing.

use std::sync::Arc;

use crate::codegen::backend::{CodegenError, CodegenResult};
use crate::codegen::ir::*;

use super::MirLowerer;

/// Which argument a placeholder prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArgRef {
    /// `{}`: the next positional argument.
    Next,
    /// `{2}`: positional argument 2.
    Index(usize),
    /// `{name}`: a variable captured from the enclosing scope.
    Name(String),
}

/// The parts of a `{:spec}` the C backend supports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Spec {
    /// `<` or `>`; `None` is the type's default (left for text, right for numbers).
    pub align: Option<char>,
    /// `+`: always print the sign of a number.
    pub plus: bool,
    /// `#`: alternate form (`0x` prefix for hex).
    pub alternate: bool,
    /// `0`: pad numbers with zeros.
    pub zero: bool,
    pub width: Option<usize>,
    pub precision: Option<usize>,
    /// `' '` for Display, `'?'` for Debug, or `x`, `X`, `o`.
    pub kind: char,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Piece {
    Text(String),
    Arg(ArgRef, Spec),
}

/// Parse a Rust format string into pieces. Returns a message naming the first
/// construct the C backend cannot print.
pub(crate) fn parse_format(fmt: &str) -> Result<Vec<Piece>, String> {
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push('}');
            }
            '}' => {
                return Err(
                    "unmatched `}` in format string (write `}}` for a literal brace)".into(),
                )
            }
            '{' => {
                let mut inner = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    inner.push(c);
                }
                if !closed {
                    return Err(
                        "unclosed `{` in format string (write `{{` for a literal brace)".into(),
                    );
                }
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                let (arg, spec) = match inner.split_once(':') {
                    Some((a, s)) => (a.trim(), Some(s)),
                    None => (inner.trim(), None),
                };
                let arg_ref = if arg.is_empty() {
                    ArgRef::Next
                } else if let Ok(i) = arg.parse::<usize>() {
                    ArgRef::Index(i)
                } else if arg.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !arg.starts_with(|c: char| c.is_ascii_digit())
                {
                    ArgRef::Name(arg.to_string())
                } else {
                    return Err(format!("`{{{inner}}}` is not a supported placeholder"));
                };
                let spec = match spec {
                    Some(s) => parse_spec(s)?,
                    None => Spec {
                        kind: ' ',
                        ..Spec::default()
                    },
                };
                pieces.push(Piece::Arg(arg_ref, spec));
            }
            _ => text.push(ch),
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok(pieces)
}

fn parse_spec(spec: &str) -> Result<Spec, String> {
    let mut out = Spec {
        kind: ' ',
        ..Spec::default()
    };
    let s: Vec<char> = spec.chars().collect();
    let mut i = 0;
    // [[fill]align]: only the default space fill is supported.
    if s.len() >= 2 && matches!(s[1], '<' | '>' | '^') {
        if s[0] != ' ' {
            return Err(format!(
                "fill character `{}` in `{{:{spec}}}` is not supported",
                s[0]
            ));
        }
        i = 1;
    }
    if i < s.len() && matches!(s[i], '<' | '>' | '^') {
        if s[i] == '^' {
            return Err(format!(
                "centre alignment `^` in `{{:{spec}}}` is not supported"
            ));
        }
        out.align = Some(s[i]);
        i += 1;
    }
    if i < s.len() && s[i] == '+' {
        out.plus = true;
        i += 1;
    } else if i < s.len() && s[i] == '-' {
        i += 1;
    }
    if i < s.len() && s[i] == '#' {
        out.alternate = true;
        i += 1;
    }
    if i < s.len() && s[i] == '0' {
        out.zero = true;
        i += 1;
    }
    let start = i;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    if i > start {
        out.width = Some(s[start..i].iter().collect::<String>().parse().unwrap_or(0));
    }
    if i < s.len() && s[i] == '$' {
        return Err(format!(
            "a width taken from an argument in `{{:{spec}}}` is not supported"
        ));
    }
    if i < s.len() && s[i] == '.' {
        i += 1;
        let p0 = i;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
        if i == p0 {
            return Err(format!("precision in `{{:{spec}}}` must be a number"));
        }
        out.precision = Some(s[p0..i].iter().collect::<String>().parse().unwrap_or(0));
    }
    match &s[i..] {
        [] => {}
        ['?'] => out.kind = '?',
        ['x'] => out.kind = 'x',
        ['X'] => out.kind = 'X',
        ['o'] => out.kind = 'o',
        rest => {
            let rest: String = rest.iter().collect();
            return Err(format!(
                "format type `{rest}` in `{{:{spec}}}` is not supported"
            ));
        }
    }
    Ok(out)
}

/// C printf flags and width/precision for a spec. `left_default` is Rust's default
/// alignment for the value (left for text, right for numbers).
fn c_flags(spec: &Spec, left_default: bool, numeric: bool) -> String {
    let mut f = String::from("%");
    // Alignment only matters with a width; without one, emit no flag so the
    // plain conversions (`%s`, `%d`) stay as other backends expect them.
    let left = spec.width.is_some()
        && match spec.align {
            Some('<') => true,
            Some('>') => false,
            _ => left_default,
        };
    if left {
        f.push('-');
    }
    if numeric && spec.plus {
        f.push('+');
    }
    if spec.alternate {
        f.push('#');
    }
    if numeric && spec.zero && !left {
        f.push('0');
    }
    if let Some(w) = spec.width {
        f.push_str(&w.to_string());
    }
    f
}

const STR: &str = "BuildString";

fn is_string(ty: &MirType) -> bool {
    matches!(ty, MirType::Struct(n) if n.as_ref() == STR)
}

impl<'ctx> MirLowerer<'ctx> {
    fn fmt_reject(&self, message: String, help: &str) -> CodegenError {
        CodegenError::Rejected {
            message,
            location: None,
            help: Some(help.to_string()),
        }
    }

    /// Call a runtime function that returns a BuildString and give back its
    /// `.ptr` for `%s`.
    fn fmt_string_call(&mut self, func: &str, args: Vec<MirValue>) -> CodegenResult<MirValue> {
        let builder = self
            .current_fn
            .as_mut()
            .ok_or_else(|| CodegenError::Internal("No current function for macro".into()))?;
        let s = builder.create_local(MirType::Struct(Arc::from(STR)));
        let cont = builder.create_block();
        builder.call(MirValue::Function(Arc::from(func)), args, Some(s), cont);
        builder.switch_to_block(cont);
        Ok(self.fmt_ptr_of(MirValue::Local(s)))
    }

    /// `.ptr` of a BuildString value.
    fn fmt_ptr_of(&mut self, val: MirValue) -> MirValue {
        let builder = self.current_fn.as_mut().expect("current function");
        let base = match val {
            MirValue::Local(_) => val,
            other => {
                let tmp = builder.create_local(MirType::Struct(Arc::from(STR)));
                builder.assign(tmp, MirRValue::Use(other));
                MirValue::Local(tmp)
            }
        };
        let ptr = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
        builder.assign(
            ptr,
            MirRValue::FieldAccess {
                base,
                field_name: Arc::from("ptr"),
                field_ty: MirType::Ptr(Box::new(MirType::i8())),
            },
        );
        MirValue::Local(ptr)
    }

    /// The C conversion and the argument to pass for one placeholder.
    pub(crate) fn fmt_placeholder(
        &mut self,
        val: MirValue,
        spec: &Spec,
        what: &str,
    ) -> CodegenResult<(String, Option<MirValue>)> {
        let val = self.deref_if_pointer(val)?;
        let ty = self.type_of_value(&val);
        let debug = spec.kind == '?';
        // A char prints as the character (Debug in single quotes), not its code.
        let is_char = what.starts_with('\'')
            || matches!(&val, MirValue::Local(id) if self.char_locals.contains(id));
        if is_char && matches!(ty, MirType::Int(..)) && !matches!(spec.kind, 'x' | 'X' | 'o') {
            let arg = self.fmt_string_call("build_char_to_string", vec![val])?;
            if debug {
                return Ok(("'%s'".to_string(), Some(arg)));
            }
            return Ok((format!("{}s", c_flags(spec, true, false)), Some(arg)));
        }
        let radix = matches!(spec.kind, 'x' | 'X' | 'o');
        match &ty {
            MirType::Int(IntSize::I128, signed) => {
                if radix {
                    return Err(self.fmt_reject(
                        format!("hex or octal formatting of the 128-bit integer `{what}` is not supported"),
                        "convert to i64 or u64 first",
                    ));
                }
                let func = if *signed { "build_i128_to_string" } else { "build_u128_to_string" };
                let arg = self.fmt_string_call(func, vec![val])?;
                Ok((format!("{}s", c_flags(spec, false, false)), Some(arg)))
            }
            MirType::Int(size, signed) => {
                let wide = matches!(size, IntSize::I64 | IntSize::ISize);
                let conv = match (spec.kind, *signed) {
                    ('x', _) => "x",
                    ('X', _) => "X",
                    ('o', _) => "o",
                    (_, true) => "d",
                    (_, false) => "u",
                };
                let len = if wide { "ll" } else { "" };
                // Rust ignores a precision on integers, and so does this.
                if spec.alternate && radix {
                    // Rust writes `0xff`, `0xFF` and `0o10`; C's `#` gives
                    // `0XFF`, `010` and no prefix for zero. Write the prefix
                    // as text instead.
                    if spec.width.is_some() {
                        return Err(self.fmt_reject(
                            format!("a width together with `#` on `{what}` is not supported"),
                            "drop the width, or drop `#` and write the prefix in the format string",
                        ));
                    }
                    let prefix = if spec.kind == 'o' { "0o" } else { "0x" };
                    let plain = Spec { alternate: false, ..spec.clone() };
                    return Ok((
                        format!("{prefix}{}{len}{conv}", c_flags(&plain, false, true)),
                        Some(val),
                    ));
                }
                Ok((format!("{}{len}{conv}", c_flags(spec, false, true)), Some(val)))
            }
            MirType::Float(size) => {
                if radix {
                    return Err(self.fmt_reject(
                        format!("hex or octal formatting of the float `{what}` is not supported"),
                        "format the float with `{}` or `{:.N}`",
                    ));
                }
                if let Some(p) = spec.precision {
                    return Ok((format!("{}.{p}f", c_flags(spec, false, true)), Some(val)));
                }
                // Shortest round-trip text, as Rust prints it; Debug adds `.0`
                // to a whole number.
                let func = match (size, debug) {
                    (FloatSize::F32, false) => "build_f32_to_string",
                    (FloatSize::F64, false) => "build_f64_to_string",
                    (FloatSize::F32, true) => "build_f32_to_debug_string",
                    (FloatSize::F64, true) => "build_f64_to_debug_string",
                };
                if spec.plus || spec.zero {
                    return Err(self.fmt_reject(
                        format!("`+` or `0` padding on the float `{what}` needs a precision here"),
                        "add a precision, for example `{:+08.2}`",
                    ));
                }
                let arg = self.fmt_string_call(func, vec![val])?;
                Ok((format!("{}s", c_flags(spec, false, false)), Some(arg)))
            }
            MirType::Bool => {
                // The C backend passes a bool to printf as "true"/"false".
                Ok((format!("{}s", c_flags(spec, true, false)), Some(val)))
            }
            t if is_string(t) => {
                let arg = self.fmt_ptr_of(val);
                let conv = match spec.precision {
                    Some(p) => format!("{}.{p}s", c_flags(spec, true, false)),
                    None => format!("{}s", c_flags(spec, true, false)),
                };
                if debug {
                    if spec.width.is_some() {
                        return Err(self.fmt_reject(
                            format!("a width on `{{:?}}` of the string `{what}` is not supported"),
                            "use `{}` with the width, or `{:?}` without it",
                        ));
                    }
                    return Ok(("\"%s\"".to_string(), Some(arg)));
                }
                Ok((conv, Some(arg)))
            }
            MirType::Ptr(inner) if matches!(**inner, MirType::Int(IntSize::I8, _)) => {
                // A C string (`const char*`), already printable.
                Ok((format!("{}s", c_flags(spec, true, false)), Some(val)))
            }
            MirType::Vec(elem) => {
                if !debug {
                    return Err(self.fmt_reject(
                        format!("`{what}` is a vector, which has no `{{}}` (Display) form"),
                        "print it with `{:?}`, or loop over it and print each element",
                    ));
                }
                let suffix = match elem.as_ref() {
                    MirType::Int(IntSize::I64 | IntSize::ISize, _) => "i64",
                    MirType::Int(_, _) | MirType::Bool => "i32",
                    MirType::Float(_) => "f64",
                    t if is_string(t) => "str",
                    other => {
                        return Err(self.fmt_reject(
                            format!(
                                "`{{:?}}` of a vector of `{}` is not supported",
                                other.to_string().replace(STR, "str")
                            ),
                            "loop over the vector and print each element",
                        ))
                    }
                };
                let arg = self.fmt_string_call(&format!("build_hvec_debug_{suffix}"), vec![val])?;
                Ok((format!("{}s", c_flags(spec, true, false)), Some(arg)))
            }
            other => Err(self.fmt_reject(
                format!(
                    "`{what}` has type `{}`, which cannot be formatted by the C backend",
                    other.to_string().replace(STR, "str")
                ),
                "format its fields one by one, or give the type a `to_string` method and print that",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: char) -> Spec {
        Spec {
            kind,
            ..Spec::default()
        }
    }

    #[test]
    fn parses_positional_named_and_escaped_pieces() {
        let p = parse_format("a {} {{b}} {0} {x} {:?}").unwrap();
        assert_eq!(
            p,
            vec![
                Piece::Text("a ".into()),
                Piece::Arg(ArgRef::Next, spec(' ')),
                Piece::Text(" {b} ".into()),
                Piece::Arg(ArgRef::Index(0), spec(' ')),
                Piece::Text(" ".into()),
                Piece::Arg(ArgRef::Name("x".into()), spec(' ')),
                Piece::Text(" ".into()),
                Piece::Arg(ArgRef::Next, spec('?')),
            ]
        );
    }

    #[test]
    fn parses_width_alignment_precision_and_radix() {
        let p = parse_format("{:>8.3}{:<5}{:08x}{:#X}{:+}").unwrap();
        let specs: Vec<Spec> = p
            .into_iter()
            .map(|x| match x {
                Piece::Arg(_, s) => s,
                Piece::Text(_) => unreachable!(),
            })
            .collect();
        assert_eq!(specs[0].align, Some('>'));
        assert_eq!((specs[0].width, specs[0].precision), (Some(8), Some(3)));
        assert_eq!((specs[1].align, specs[1].width), (Some('<'), Some(5)));
        assert!(specs[2].zero && specs[2].width == Some(8) && specs[2].kind == 'x');
        assert!(specs[3].alternate && specs[3].kind == 'X');
        assert!(specs[4].plus);
    }

    #[test]
    fn rejects_what_c_cannot_print() {
        for bad in [
            "{:^5}", "{:*<5}", "{:1$}", "{:e}", "{:b}", "{", "}", "{a.b}",
        ] {
            assert!(parse_format(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn c_flags_follow_rust_default_alignment() {
        let s = Spec {
            width: Some(4),
            kind: ' ',
            ..Spec::default()
        };
        assert_eq!(c_flags(&s, false, true), "%4");
        assert_eq!(c_flags(&s, true, false), "%-4");
        let z = Spec {
            width: Some(5),
            zero: true,
            kind: ' ',
            ..Spec::default()
        };
        assert_eq!(c_flags(&z, false, true), "%05");
    }
}
