// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Effects on the source side of translation validation: print macros and
//! calls kept opaque. Each becomes an event in the path's trace.

use super::effects::{btxt, call_event, dec, finish_atom, fmt, lit, out_event};
use super::sym::{Out, Res, St, Sym};
use super::sym_expr::val;
use super::term::*;
use super::ty::Ty;
use crate::ast::{Expr, Path};

/// An opaque call: an event, then the finish atom, then the result as an
/// uninterpreted term constrained to its type (or unit).
pub(super) fn opaque_call(sym: &mut Sym<'_>, name: &str, args: Vec<Term>, st: St) -> Res {
    let f = sym
        .fns
        .get(name)
        .ok_or(format!("call to `{name}` outside the subset"))?;
    let unit = f.sig.return_ty.is_none();
    let rty = sym.sigs.fns.get(name).and_then(|s| s.1);
    if !unit && rty.is_none() {
        return Err(format!("`{name}` returns a type outside the subset"));
    }
    let mut st = st;
    st.event(call_event(name, &args));
    st.assume(finish_atom(name, &args));
    match rty {
        Some(rty) if !unit => {
            let app = Term::App(name.to_string(), args);
            st.fact(in_range(&app, rty));
            val(st, app, Some(rty))
        }
        _ => val(st, Term::Int(0), None),
    }
}

/// One `{...}` of a format string.
enum Piece {
    Text(String),
    /// Argument index or captured name, and the rendering (`' '`, `'?'`, or
    /// a precision).
    Arg(ArgRef, Option<usize>, bool),
}

enum ArgRef {
    Next,
    Index(usize),
    Name(String),
}

fn pieces(fmt_str: &str) -> Result<Vec<Piece>, String> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut it = fmt_str.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                text.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                text.push('}');
            }
            '{' => {
                let mut inner = String::new();
                for c in it.by_ref() {
                    if c == '}' {
                        break;
                    }
                    inner.push(c);
                }
                out.push(Piece::Text(std::mem::take(&mut text)));
                let (a, spec) = inner.split_once(':').unwrap_or((inner.as_str(), ""));
                let a = a.trim();
                let r = if a.is_empty() {
                    ArgRef::Next
                } else if let Ok(i) = a.parse() {
                    ArgRef::Index(i)
                } else if a.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    ArgRef::Name(a.to_string())
                } else {
                    return Err("a format placeholder outside the subset".into());
                };
                let (prec, debug) = match spec {
                    "" => (None, false),
                    "?" => (None, true),
                    s => match s.strip_prefix('.').map(str::parse::<usize>) {
                        Some(Ok(n)) => (Some(n), false),
                        _ => return Err(format!("format spec `{s}` outside the subset")),
                    },
                };
                out.push(Piece::Arg(r, prec, debug));
            }
            '}' => return Err("an unmatched `}` in a format string".into()),
            c => text.push(c),
        }
    }
    out.push(Piece::Text(text));
    Ok(out)
}

/// Split macro arguments on top-level commas.
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for c in s.chars() {
        if in_str {
            cur.push(c);
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn parse_arg(src: &str) -> Result<Expr, String> {
    let sf = crate::lexer::SourceFile::anonymous(src.to_string());
    let tokens = crate::lexer::tokenize(src).map_err(|_| "a macro argument".to_string())?;
    let mut p = crate::parser::Parser::new(&sf, tokens);
    p.parse_expr().map_err(|_| "a macro argument".to_string())
}

/// The chunk that prints value `v` of type `ty`.
fn chunk(v: Term, ty: Option<Ty>, prec: Option<usize>, debug: bool) -> Result<Term, String> {
    Ok(match (ty, prec) {
        (Some(Ty::Float { .. }), Some(n)) => fmt(&format!(".{n}"), v),
        (_, Some(_)) => return Err("a precision on a non-float".into()),
        (Some(Ty::Int { .. }), None) => dec(v),
        (Some(Ty::Bool), None) => btxt(v),
        (Some(Ty::Float { bits }), None) => {
            fmt(&format!("f{bits}{}", if debug { "dbg" } else { "" }), v)
        }
        _ => return Err("a printed value of a type outside the subset".into()),
    })
}

/// `print!`, `println!`, `eprint!` and `eprintln!`: evaluate the arguments
/// in order, then one output event.
pub(super) fn print_macro(sym: &mut Sym<'_>, e: &Expr, path: &Path, st: St) -> Res {
    let name = path
        .segments
        .last()
        .map(|s| s.ident.name.to_string())
        .unwrap_or_default();
    let (stream, newline) = match name.as_str() {
        "print" => (1u8, false),
        "println" => (1, true),
        "eprint" => (2, false),
        "eprintln" => (2, true),
        _ => return Err(format!("macro `{name}!` outside the subset")),
    };
    let text = sym
        .source
        .and_then(|s| s.get(e.span.start.to_usize()..e.span.end.to_usize()))
        .ok_or("a macro without source text")?;
    let open = text.find('(').ok_or("a macro without arguments")?;
    let inner = text[open + 1..]
        .strip_suffix(')')
        .ok_or("a macro without arguments")?;
    let parts = split_args(inner);
    let fmt_src = parts.first().map(|p| p.trim()).unwrap_or("\"\"");
    let fmt_str = fmt_src
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or("a format string that is not a plain literal")?;
    let pcs = pieces(&crate::lexer::unescape_str(fmt_str))?;
    let args: Vec<Expr> = parts
        .iter()
        .skip(1)
        .map(|p| parse_arg(p))
        .collect::<Result<_, _>>()?;
    let mut res = vec![(st, Vec::<(Term, Option<Ty>)>::new())];
    for a in &args {
        let mut next = Vec::new();
        for (s, acc) in res {
            for (s2, o) in sym.expr(a, s, None)? {
                let Out::Val(t, ty) = o else {
                    return Err("control flow in a macro argument".into());
                };
                let mut acc2 = acc.clone();
                acc2.push((t, ty));
                next.push((s2, acc2));
            }
        }
        res = next;
    }
    let mut out = Vec::new();
    for (mut s, vals) in res {
        let mut chunks = Vec::new();
        let mut next = 0;
        for p in &pcs {
            match p {
                Piece::Text(t) => chunks.push(lit(t)),
                Piece::Arg(r, prec, debug) => {
                    let (v, ty) = match r {
                        ArgRef::Next => {
                            next += 1;
                            vals.get(next - 1).cloned()
                        }
                        ArgRef::Index(i) => vals.get(*i).cloned(),
                        ArgRef::Name(n) => s.get(n).map(|(t, ty)| (t, Some(ty))),
                    }
                    .ok_or("a placeholder without an argument")?;
                    chunks.push(chunk(v, ty, *prec, *debug)?);
                }
            }
        }
        if newline {
            chunks.push(lit("\n"));
        }
        s.event(out_event(stream, chunks));
        out.push((s, Out::Val(Term::Int(0), None)));
    }
    Ok(out)
}
