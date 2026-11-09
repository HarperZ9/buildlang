// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Floats as opaque IEEE values, for translation validation.
//!
//! A float value is never reasoned about arithmetically. Each operation is an
//! uninterpreted function named by operator and width, so two sides agree only
//! when they apply the same operations to the same operands in the same order.
//! A comparison is an uninterpreted predicate (an atom equal to 1), never
//! folded and never rewritten into another comparison: with NaN, `!(a < b)`
//! and `a >= b` differ. A literal is its exact bit pattern. Folding is limited
//! to steps IEEE fixes exactly: negating a constant flips its sign bit, and an
//! integer constant converts by round to nearest, ties to even.

use super::term::{cmp, not, COp, Term};
use super::ty::Ty;

/// The literal with bit pattern of `v` at width `ty`.
pub fn fconst(ty: Ty, v: f64) -> Term {
    match ty {
        Ty::Float { bits: 32 } => {
            Term::App(format!("fconst.f32.{:08x}", (v as f32).to_bits()), vec![])
        }
        _ => Term::App(format!("fconst.f64.{:016x}", v.to_bits()), vec![]),
    }
}

/// The literal of an f32 written in decimal, rounded once to binary32.
pub fn fconst32_bits(bits: u32) -> Term {
    Term::App(format!("fconst.f32.{bits:08x}"), vec![])
}

fn const_bits(t: &Term) -> Option<(u8, u64)> {
    let Term::App(name, args) = t else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    let rest = name.strip_prefix("fconst.f")?;
    let (w, hex) = rest.split_once('.')?;
    let bits = u64::from_str_radix(hex, 16).ok()?;
    Some((w.parse().ok()?, bits))
}

/// Whether `t` is a float literal term.
pub fn is_const(t: &Term) -> bool {
    const_bits(t).is_some()
}

/// An arithmetic operation: `add sub mul div rem` on two operands, `neg` on one.
pub fn fop(op: &str, ty: Ty, args: Vec<Term>) -> Term {
    if op == "neg" {
        if let [a] = args.as_slice() {
            if let Some((w, b)) = const_bits(a) {
                let flipped = if w == 32 {
                    b ^ 0x8000_0000
                } else {
                    b ^ (1u64 << 63)
                };
                let digits = if w == 32 { 8 } else { 16 };
                return Term::App(format!("fconst.f{w}.{flipped:0digits$x}"), vec![]);
            }
        }
    }
    Term::App(format!("f{op}.{ty}"), args)
}

/// A float comparison, as a boolean term over an opaque predicate.
pub fn fcmp(op: COp, ty: Ty, a: Term, b: Term) -> Term {
    let is = |name: &str, x: Term, y: Term| {
        cmp(
            COp::Eq,
            Term::App(format!("{name}.{ty}"), vec![x, y]),
            Term::Int(1),
        )
    };
    match op {
        COp::Lt => is("flt", a, b),
        COp::Le => is("fle", a, b),
        COp::Gt => is("flt", b, a),
        COp::Ge => is("fle", b, a),
        COp::Eq => is("feq", a, b),
        COp::Ne => not(is("feq", a, b)),
    }
}

/// Convert `v` of type `from` (`None`: an integer literal) to `to`, where
/// either side is a float. Integer results are left to the caller's range fact.
pub fn conv(to: Ty, from: Option<Ty>, v: Term) -> Result<Term, String> {
    if from == Some(to) {
        return Ok(v);
    }
    match (from, to) {
        (Some(Ty::Bool), _) | (_, Ty::Bool) => Err("a conversion between bool and float".into()),
        (None | Some(Ty::Int { .. }), Ty::Float { .. }) => Ok(match v {
            Term::Int(i) => match to {
                Ty::Float { bits: 32 } => fconst32_bits((i as f32).to_bits()),
                _ => fconst(to, i as f64),
            },
            other => Term::App(format!("itof.{to}"), vec![other]),
        }),
        (Some(f @ Ty::Float { .. }), Ty::Float { .. }) => {
            Ok(Term::App(format!("fconv.{f}.{to}"), vec![v]))
        }
        (Some(Ty::Float { .. }), Ty::Int { .. }) => Ok(Term::App(format!("f2i.{to}"), vec![v])),
        _ => Err("a conversion outside the float model".into()),
    }
}

/// The C function a float maths method lowers to, by method name, arity
/// (not counting the receiver) and width. Only `f64` is listed: the `f32`
/// methods go through `double` functions in the backend.
pub fn method_c_name(method: &str, arity: usize, ty: Ty) -> Option<&'static str> {
    if ty != Ty::F64 {
        return None;
    }
    Some(match (method, arity) {
        ("abs", 0) => "fabs",
        ("sqrt", 0) => "sqrt",
        ("cbrt", 0) => "cbrt",
        ("ceil", 0) => "ceil",
        ("floor", 0) => "floor",
        ("round", 0) => "round",
        ("trunc", 0) => "trunc",
        ("sin", 0) => "sin",
        ("cos", 0) => "cos",
        ("tan", 0) => "tan",
        ("asin", 0) => "asin",
        ("acos", 0) => "acos",
        ("atan", 0) => "atan",
        ("sinh", 0) => "sinh",
        ("cosh", 0) => "cosh",
        ("tanh", 0) => "tanh",
        ("exp", 0) => "exp",
        ("exp2", 0) => "exp2",
        ("ln", 0) => "log",
        ("log2", 0) => "log2",
        ("log10", 0) => "log10",
        ("powi" | "powf", 1) => "pow",
        ("log", 1) => "build_det_log_base",
        ("atan2", 1) => "atan2",
        ("hypot", 1) => "hypot",
        ("copysign", 1) => "copysign",
        ("max", 1) => "build_max_f64",
        ("min", 1) => "build_min_f64",
        _ => return None,
    })
}

/// The C function a free float builtin lowers to, by name and arity.
pub fn builtin_c_name(name: &str, arity: usize) -> Option<&'static str> {
    Some(match (name, arity) {
        ("sqrt", 1) => "sqrt",
        ("sin", 1) => "build_det_sin",
        ("cos", 1) => "build_det_cos",
        ("exp", 1) => "build_det_exp",
        ("log", 1) => "build_det_log",
        ("pow", 2) => "build_det_pow",
        ("floor", 1) => "floor",
        ("ceil", 1) => "ceil",
        ("round", 1) => "round",
        ("abs", 1) => "fabs",
        ("min", 2) => "build_min_f64",
        ("max", 2) => "build_max_f64",
        ("tan", 1) => "tan",
        ("atan2", 2) => "atan2",
        ("exp2", 1) => "exp2",
        ("log2", 1) => "log2",
        ("log10", 1) => "log10",
        ("fract", 1) => "build_fract",
        _ => return None,
    })
}

/// Whether a C function name is one the tables above produce, with its
/// result width. A call to it is the same uninterpreted function on both sides.
pub fn c_math(name: &str) -> Option<Ty> {
    const NAMES: [&str; 34] = [
        "fabs",
        "sqrt",
        "cbrt",
        "ceil",
        "floor",
        "round",
        "trunc",
        "sin",
        "cos",
        "tan",
        "asin",
        "acos",
        "atan",
        "sinh",
        "cosh",
        "tanh",
        "exp",
        "exp2",
        "log",
        "log2",
        "log10",
        "pow",
        "build_det_log_base",
        "atan2",
        "hypot",
        "copysign",
        "build_max_f64",
        "build_min_f64",
        "build_det_sin",
        "build_det_cos",
        "build_det_exp",
        "build_det_log",
        "build_det_pow",
        "build_fract",
    ];
    NAMES.contains(&name).then_some(Ty::F64)
}

/// A call of C maths function `name` on `args`.
pub fn math_call(name: &str, args: Vec<Term>) -> Term {
    Term::App(format!("call.{name}"), args)
}
