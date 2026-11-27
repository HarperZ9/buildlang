// ===============================================================================
// BUILDLANG CODEGEN - FORMAT STRINGS
// ===============================================================================
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.
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
use crate::codegen::builder::values;
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
    /// `<`, `>` or `^`; `None` is the type's default (left for text, right for numbers).
    pub align: Option<char>,
    /// `+`: always print the sign of a number.
    pub plus: bool,
    /// `#`: alternate form (`0x` prefix for hex).
    pub alternate: bool,
    /// `0`: pad numbers with zeros.
    pub zero: bool,
    pub width: Option<usize>,
    /// Dynamic width from an argument (`{:w$}` or `{:1$}`).
    pub dynamic_width: Option<ArgRef>,
    pub precision: Option<usize>,
    /// `' '` for Display, `'?'` for Debug, `x`, `X`, `o` for radix, or `e`, `E` for exponent.
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
        let num = s[start..i].iter().collect::<String>().parse().unwrap_or(0);
        if i < s.len() && s[i] == '$' {
            out.dynamic_width = Some(ArgRef::Index(num));
            i += 1;
        } else {
            out.width = Some(num);
        }
    } else if i < s.len() && (s[i].is_ascii_alphabetic() || s[i] == '_') {
        let id_start = i;
        let mut j = i;
        while j < s.len() && (s[j].is_ascii_alphanumeric() || s[j] == '_') {
            j += 1;
        }
        if j < s.len() && s[j] == '$' {
            let name: String = s[id_start..j].iter().collect();
            out.dynamic_width = Some(ArgRef::Name(name));
            i = j + 1;
        }
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
        ['e'] => out.kind = 'e',
        ['E'] => out.kind = 'E',
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

    /// Generate (once per enum type) a helper function `__bl_fmt_enum_<Name>`
    /// that maps each variant tag to its variant name as a `BuildString`.
    fn ensure_enum_fmt_helper(&mut self, enum_name: &Arc<str>) -> CodegenResult<Arc<str>> {
        let fn_name: Arc<str> = Arc::from(format!("__bl_fmt_enum_{}", enum_name));
        if self.monomorphized.contains(&fn_name)
            || self.module.find_function(fn_name.as_ref()).is_some()
        {
            return Ok(fn_name);
        }
        self.monomorphized.insert(fn_name.clone());

        let variants: Vec<(i128, Arc<str>)> =
            match self.module.find_type(enum_name).map(|td| &td.kind) {
                Some(TypeDefKind::Enum { variants, .. }) => variants
                    .iter()
                    .map(|v| (v.discriminant, v.name.clone()))
                    .collect(),
                _ => Vec::new(),
            };

        let enum_ty = MirType::Struct(enum_name.clone());
        let str_ty = MirType::Struct(Arc::from(STR));
        let sig = MirFnSig::new(vec![enum_ty], str_ty.clone());
        let mut b = crate::codegen::MirBuilder::with_linear_type_names(
            fn_name.clone(),
            sig,
            Arc::new(self.linear_type_names.clone()),
        );
        let p0 = b.param_local(0);
        b.set_param_name(0, Arc::from("val"));

        let tag = b.create_local(MirType::i32());
        b.assign(
            tag,
            MirRValue::FieldAccess {
                base: crate::codegen::values::local(p0),
                field_name: Arc::from("tag"),
                field_ty: MirType::i32(),
            },
        );

        let fallback_str = MirConst::Struct(
            Arc::from(STR),
            vec![
                MirConst::ByteStr(enum_name.as_bytes().to_vec()),
                MirConst::Uint(enum_name.len() as u128, MirType::usize()),
                MirConst::Uint(0, MirType::usize()),
            ],
        );

        for (disc, vname) in variants {
            let cmp = b.create_local(MirType::Bool);
            b.binary_op(
                cmp,
                BinOp::Eq,
                crate::codegen::values::local(tag),
                MirValue::Const(MirConst::Int(disc, MirType::i32())),
            );
            let hit_bb = b.create_block();
            let next_bb = b.create_block();
            b.branch(crate::codegen::values::local(cmp), hit_bb, next_bb);
            b.switch_to_block(hit_bb);
            let s_val = MirConst::Struct(
                Arc::from(STR),
                vec![
                    MirConst::ByteStr(vname.as_bytes().to_vec()),
                    MirConst::Uint(vname.len() as u128, MirType::usize()),
                    MirConst::Uint(0, MirType::usize()),
                ],
            );
            let ret_l = b.create_local(str_ty.clone());
            b.assign(ret_l, MirRValue::Use(MirValue::Const(s_val)));
            b.ret(Some(crate::codegen::values::local(ret_l)));
            b.switch_to_block(next_bb);
        }

        let def_l = b.create_local(str_ty);
        b.assign(def_l, MirRValue::Use(MirValue::Const(fallback_str)));
        b.ret(Some(crate::codegen::values::local(def_l)));

        let mut func = b.build();
        func.is_public = false;
        self.module.add_function(func);
        Ok(fn_name)
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
        dyn_width: Option<MirValue>,
    ) -> CodegenResult<(String, Option<MirValue>)> {
        // Centre alignment (`^`) or dynamic width (`w$` / `N$`): format the
        // value into a BuildString with its base spec, then pad via
        // `build_fmt_align(str, width_i64, align_code)`.
        if spec.align == Some('^') || dyn_width.is_some() {
            let inner_spec = Spec {
                align: None,
                width: None,
                dynamic_width: None,
                ..spec.clone()
            };
            let val_deref = self.deref_if_pointer(val.clone())?;
            let is_numeric = matches!(
                self.type_of_value(&val_deref),
                MirType::Int(..) | MirType::Float(_)
            );
            let (inner_conv, inner_arg) = self.fmt_placeholder(val, &inner_spec, what, None)?;
            let inner_str_idx = self.module.intern_string(inner_conv);
            let width_val = if let Some(dw) = dyn_width {
                let dw = self.deref_if_pointer(dw)?;
                let dw_ty = self.type_of_value(&dw);
                if dw_ty == MirType::i64() {
                    dw
                } else {
                    let builder = self.current_fn.as_mut().unwrap();
                    let w_local = builder.create_local(MirType::i64());
                    builder.cast(w_local, CastKind::IntToInt, dw, MirType::i64());
                    MirValue::Local(w_local)
                }
            } else {
                MirValue::Const(MirConst::Int(
                    spec.width.unwrap_or(0) as i128,
                    MirType::i64(),
                ))
            };
            let align_code: i128 = match spec.align {
                Some('<') => 0,
                Some('>') => 1,
                Some('^') => 2,
                _ => {
                    if is_numeric {
                        1
                    } else {
                        0
                    }
                }
            };
            let formatted = {
                let builder = self.current_fn.as_mut().unwrap();
                let fmt_l = builder.create_local(MirType::Ptr(Box::new(MirType::i8())));
                builder.assign(
                    fmt_l,
                    MirRValue::Use(MirValue::Const(MirConst::Str(inner_str_idx))),
                );
                let out_s = builder.create_local(MirType::Struct(Arc::from(STR)));
                let cont = builder.create_block();
                let mut call_args = vec![MirValue::Local(fmt_l)];
                if let Some(a) = inner_arg {
                    call_args.push(a);
                }
                builder.call(
                    MirValue::Function(Arc::from("build_sprintf")),
                    call_args,
                    Some(out_s),
                    cont,
                );
                builder.switch_to_block(cont);
                MirValue::Local(out_s)
            };
            let aligned_ptr = self.fmt_string_call(
                "build_fmt_align",
                vec![
                    formatted,
                    width_val,
                    MirValue::Const(MirConst::Int(align_code, MirType::i32())),
                ],
            )?;
            return Ok(("%s".to_string(), Some(aligned_ptr)));
        }
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
        let exp = matches!(spec.kind, 'e' | 'E');
        if exp && !matches!(&ty, MirType::Float(_)) {
            return Err(self.fmt_reject(
                format!(
                    "exponent formatting `{{:{}}}` of non-float `{what}` is not supported",
                    spec.kind
                ),
                "cast to f64 first or format with `{}`",
            ));
        }
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
                if exp {
                    if spec.zero {
                        return Err(self.fmt_reject(
                            format!("`0` padding with exponent formatting on `{what}` is not supported"),
                            "drop the `0` flag, for example `{:e}` or `{:.2e}`",
                        ));
                    }
                    let func = match size {
                        FloatSize::F32 => "build_f32_to_exp_string",
                        FloatSize::F64 => "build_f64_to_exp_string",
                    };
                    let upper = MirValue::Const(MirConst::Int(
                        if spec.kind == 'E' { 1 } else { 0 },
                        MirType::i32(),
                    ));
                    let prec = MirValue::Const(MirConst::Int(
                        spec.precision.map(|p| p as i128).unwrap_or(-1),
                        MirType::i32(),
                    ));
                    let plus = MirValue::Const(MirConst::Int(
                        if spec.plus { 1 } else { 0 },
                        MirType::i32(),
                    ));
                    let arg = self.fmt_string_call(func, vec![val, upper, prec, plus])?;
                    return Ok((format!("{}s", c_flags(spec, false, false)), Some(arg)));
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
            MirType::Slice(elem)
            | MirType::Ptr(elem)
                if matches!(&ty, MirType::Slice(_))
                    || matches!(elem.as_ref(), MirType::Slice(_)) =>
            {
                let slice_elem = match &ty {
                    MirType::Slice(e) => e.as_ref().clone(),
                    MirType::Ptr(inner) => match inner.as_ref() {
                        MirType::Slice(e) => e.as_ref().clone(),
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                };
                if !debug {
                    return Err(self.fmt_reject(
                        format!("`{what}` is a slice, which has no `{{}}` (Display) form"),
                        "print it with `{:?}`, or loop over it and print each element",
                    ));
                }
                let inner_elem = match &slice_elem {
                    MirType::Ptr(p) => p.as_ref(),
                    other => other,
                };
                let (suffix, vec_elem, new_fn, push_fn) = match inner_elem {
                    MirType::Int(IntSize::I64 | IntSize::ISize, _) => (
                        "i64",
                        MirType::i64(),
                        "build_hvec_new_i64",
                        "build_hvec_push_i64",
                    ),
                    MirType::Int(_, _) | MirType::Bool => (
                        "i32",
                        MirType::i32(),
                        "build_hvec_new_i32",
                        "build_hvec_push_i32",
                    ),
                    MirType::Float(_) => (
                        "f64",
                        MirType::f64(),
                        "build_hvec_new_f64",
                        "build_hvec_push_f64",
                    ),
                    t if is_string(t) => (
                        "str",
                        MirType::Struct(Arc::from(STR)),
                        "build_hvec_new_str",
                        "build_hvec_push_str",
                    ),
                    other => {
                        return Err(self.fmt_reject(
                            format!(
                                "`{{:?}}` of a slice of `{}` is not supported",
                                other.to_string().replace(STR, "str")
                            ),
                            "loop over the slice and print each element",
                        ))
                    }
                };
                let elem_is_ptr = matches!(&slice_elem, MirType::Ptr(_));
                let tmp_vec = {
                    let b = self.current_fn.as_mut().unwrap();
                    let sl = b.create_local(ty.clone());
                    b.assign(sl, MirRValue::Use(val));
                    let v = b.create_local(MirType::Vec(Box::new(vec_elem.clone())));
                    let c0 = b.create_block();
                    b.call(MirValue::Function(Arc::from(new_fn)), vec![], Some(v), c0);
                    b.switch_to_block(c0);
                    let len_u = b.create_local(MirType::usize());
                    b.assign(
                        len_u,
                        MirRValue::FieldAccess {
                            base: values::local(sl),
                            field_name: Arc::from("len"),
                            field_ty: MirType::usize(),
                        },
                    );
                    let idx = b.create_local(MirType::usize());
                    b.assign_const(idx, MirConst::Uint(0, MirType::usize()));
                    let cond_bb = b.create_block();
                    let body_bb = b.create_block();
                    let exit_bb = b.create_block();
                    b.goto(cond_bb);
                    b.switch_to_block(cond_bb);
                    let cmp = b.create_local(MirType::Bool);
                    b.binary_op(cmp, BinOp::Lt, values::local(idx), values::local(len_u));
                    b.branch(values::local(cmp), body_bb, exit_bb);
                    b.switch_to_block(body_bb);
                    let raw_el = b.create_local(slice_elem.clone());
                    b.assign(
                        raw_el,
                        MirRValue::IndexAccess {
                            base: values::local(sl),
                            index: values::local(idx),
                            elem_ty: slice_elem.clone(),
                        },
                    );
                    let pushed_val = if elem_is_ptr {
                        let d = b.create_local(vec_elem.clone());
                        b.assign(
                            d,
                            MirRValue::Deref {
                                ptr: values::local(raw_el),
                                pointee_ty: vec_elem.clone(),
                            },
                        );
                        values::local(d)
                    } else if slice_elem != vec_elem {
                        let c = b.create_local(vec_elem.clone());
                        let ck = match (&slice_elem, &vec_elem) {
                            (MirType::Float(_), MirType::Float(_)) => CastKind::FloatToFloat,
                            _ => CastKind::IntToInt,
                        };
                        b.cast(c, ck, values::local(raw_el), vec_elem.clone());
                        values::local(c)
                    } else {
                        values::local(raw_el)
                    };
                    let cp = b.create_block();
                    b.call(
                        MirValue::Function(Arc::from(push_fn)),
                        vec![values::local(v), pushed_val],
                        None,
                        cp,
                    );
                    b.switch_to_block(cp);
                    let ni = b.create_local(MirType::usize());
                    b.binary_op(
                        ni,
                        BinOp::Add,
                        values::local(idx),
                        MirValue::Const(MirConst::Uint(1, MirType::usize())),
                    );
                    b.assign(idx, MirRValue::Use(values::local(ni)));
                    b.goto(cond_bb);
                    b.switch_to_block(exit_bb);
                    values::local(v)
                };
                let arg =
                    self.fmt_string_call(&format!("build_hvec_debug_{suffix}"), vec![tmp_vec])?;
                Ok((format!("{}s", c_flags(spec, true, false)), Some(arg)))
            }
            MirType::Struct(name)
                if (debug && self.debug_impls.contains_key(name))
                    || self.display_impls.contains_key(name) =>
            {
                let fmt_fn = if debug {
                    self.debug_impls
                        .get(name)
                        .or_else(|| self.display_impls.get(name))
                        .cloned()
                        .unwrap()
                } else {
                    self.display_impls.get(name).cloned().unwrap()
                };
                let by_val = self
                    .by_value_self
                    .contains(&(name.clone(), Arc::from("fmt")));
                let val_ty = MirType::Struct(name.clone());
                let fmt_ty = MirType::Struct(Arc::from("BuildFormatter"));
                let finished_str = {
                    let builder = self.current_fn.as_mut().unwrap();
                    let val_local = match val {
                        MirValue::Local(id) => id,
                        other => {
                            let tmp = builder.create_local(val_ty.clone());
                            builder.assign(tmp, MirRValue::Use(other));
                            tmp
                        }
                    };
                    let self_arg = if by_val {
                        MirValue::Local(val_local)
                    } else {
                        let ptr = builder.create_local(MirType::Ptr(Box::new(val_ty)));
                        builder.assign(
                            ptr,
                            MirRValue::Ref {
                                place: MirPlace::local(val_local),
                                is_mut: false,
                            },
                        );
                        MirValue::Local(ptr)
                    };
                    let f_local = builder.create_local(fmt_ty.clone());
                    let cont1 = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from("build_formatter_new")),
                        vec![],
                        Some(f_local),
                        cont1,
                    );
                    builder.switch_to_block(cont1);
                    let f_ptr = builder.create_local(MirType::Ptr(Box::new(fmt_ty)));
                    builder.assign(
                        f_ptr,
                        MirRValue::Ref {
                            place: MirPlace::local(f_local),
                            is_mut: true,
                        },
                    );
                    let cont2 = builder.create_block();
                    builder.call(
                        MirValue::Function(fmt_fn),
                        vec![self_arg, MirValue::Local(f_ptr)],
                        None,
                        cont2,
                    );
                    builder.switch_to_block(cont2);
                    let res_s = builder.create_local(MirType::Struct(Arc::from(STR)));
                    let cont3 = builder.create_block();
                    builder.call(
                        MirValue::Function(Arc::from("build_formatter_finish")),
                        vec![MirValue::Local(f_ptr)],
                        Some(res_s),
                        cont3,
                    );
                    builder.switch_to_block(cont3);
                    MirValue::Local(res_s)
                };
                let arg = self.fmt_ptr_of(finished_str);
                Ok((format!("{}s", c_flags(spec, true, false)), Some(arg)))
            }
            MirType::Struct(name)
                if matches!(
                    self.module.find_type(name).map(|td| &td.kind),
                    Some(TypeDefKind::Enum { .. })
                ) =>
            {
                let helper = self.ensure_enum_fmt_helper(name)?;
                let arg = self.fmt_string_call(helper.as_ref(), vec![val])?;
                Ok((format!("{}s", c_flags(spec, true, false)), Some(arg)))
            }
            MirType::TraitObject(trait_name)
                if matches!(trait_name.as_ref(), "Error" | "Display" | "Debug")
                    || self
                        .trait_methods
                        .get(trait_name)
                        .is_some_and(|ms| ms.iter().any(|(n, _)| n.as_ref() == "fmt")) =>
            {
                let label = format!("<dyn {trait_name}>");
                let idx = self.module.intern_string(label);
                let builder = self.current_fn.as_mut().unwrap();
                let s = builder.create_local(MirType::Struct(Arc::from(STR)));
                let cont = builder.create_block();
                builder.call(
                    MirValue::Function(Arc::from("build_string_new")),
                    vec![MirValue::Const(MirConst::Str(idx))],
                    Some(s),
                    cont,
                );
                builder.switch_to_block(cont);
                let arg = self.fmt_ptr_of(MirValue::Local(s));
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
    fn parses_exponent_specs() {
        let p = parse_format("{:e} {:E} {:.2e} {:+E}").unwrap();
        let specs: Vec<Spec> = p
            .into_iter()
            .filter_map(|x| match x {
                Piece::Arg(_, s) => Some(s),
                Piece::Text(_) => None,
            })
            .collect();
        assert_eq!(specs.len(), 4);
        assert_eq!(specs[0].kind, 'e');
        assert_eq!(specs[1].kind, 'E');
        assert_eq!((specs[2].kind, specs[2].precision), ('e', Some(2)));
        assert_eq!((specs[3].kind, specs[3].plus), ('E', true));
    }

    #[test]
    fn parses_center_and_dynamic_width() {
        let p = parse_format("{:^5}{:^w$}{:<1$}").unwrap();
        let specs: Vec<Spec> = p
            .into_iter()
            .filter_map(|x| match x {
                Piece::Arg(_, s) => Some(s),
                Piece::Text(_) => None,
            })
            .collect();
        assert_eq!(specs[0].align, Some('^'));
        assert_eq!(specs[0].width, Some(5));
        assert_eq!(specs[1].align, Some('^'));
        assert_eq!(specs[1].dynamic_width, Some(ArgRef::Name("w".into())));
        assert_eq!(specs[2].align, Some('<'));
        assert_eq!(specs[2].dynamic_width, Some(ArgRef::Index(1)));
    }

    #[test]
    fn rejects_what_c_cannot_print() {
        for bad in ["{:*<5}", "{:b}", "{", "}", "{a.b}"] {
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
