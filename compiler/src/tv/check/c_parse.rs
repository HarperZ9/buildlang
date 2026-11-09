// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A parser for the C the backend emits for one function, written for the
//! checker alone. It reads the generated text, not the code generator's data,
//! and accepts only the three-address shape: declarations, labels,
//! assignments, conditional and plain `goto`, `return` and call statements.
//! Any other line makes the function uncertifiable.

pub use super::c_expr::parse_expr;
pub use super::c_globals::{c_consts, c_structs};

/// A C expression.
#[derive(Clone, Debug, PartialEq)]
pub enum CExpr {
    Var(String),
    Int(i128),
    /// A float literal as written, `f` suffix included for `float`.
    Float(String),
    Bool(bool),
    Bin(&'static str, Box<CExpr>, Box<CExpr>),
    Un(char, Box<CExpr>),
    Cast(String, Box<CExpr>),
    Call(String, Vec<CExpr>),
    /// A string literal, C escapes decoded.
    Str(String),
    /// `c ? a : b`.
    Cond(Box<CExpr>, Box<CExpr>, Box<CExpr>),
    /// `e.field`.
    Member(Box<CExpr>, String),
    /// `(Name){ a, b }`: a struct compound literal, fields in C order.
    Compound(String, Vec<CExpr>),
}

/// A C statement.
#[derive(Clone, Debug, PartialEq)]
pub enum CStmt {
    Label(String),
    Assign(String, CExpr),
    Eval(CExpr),
    If(CExpr, String, String),
    Goto(String),
    Ret(Option<CExpr>),
}

/// One parsed function.
#[derive(Clone, Debug)]
pub struct CFunc {
    pub name: String,
    pub ret: String,
    pub params: Vec<(String, String)>,
    pub locals: Vec<(String, String)>,
    pub body: Vec<CStmt>,
}

/// The C integer and bool type names the checker models.
pub fn is_scalar(ty: &str) -> bool {
    matches!(
        ty,
        "bool"
            | "int8_t"
            | "int16_t"
            | "int32_t"
            | "int64_t"
            | "uint8_t"
            | "uint16_t"
            | "uint32_t"
            | "uint64_t"
            | "double"
            | "float"
    )
}

/// The text of function `name` in `c`: its header line through its closing
/// brace. `None` when the file does not define it.
pub fn function_text<'c>(c: &'c str, name: &str) -> Option<&'c str> {
    let mut offset = 0;
    for line in c.split_inclusive('\n') {
        let t = line.trim_end();
        if t.ends_with(") {") && !line.starts_with(' ') {
            let head = t.trim_start_matches("static ");
            if let Some(open) = head.find('(') {
                if head[..open].split_whitespace().last() == Some(name) {
                    let rest = &c[offset..];
                    let end = rest.find("\n}")? + 2;
                    return Some(&rest[..end]);
                }
            }
        }
        offset += line.len();
    }
    None
}

/// Parse a function's text.
pub fn parse_function(text: &str) -> Result<CFunc, String> {
    let mut lines = text.lines();
    let header = lines.next().ok_or("empty function")?.trim_end();
    let head = header.trim_start_matches("static ");
    let open = head.find('(').ok_or("no parameter list")?;
    let close = head.rfind(')').ok_or("no parameter list")?;
    let mut sig = head[..open].split_whitespace().collect::<Vec<_>>();
    let name = sig.pop().ok_or("no name")?.to_string();
    let ret = sig.join(" ");
    let mut params = Vec::new();
    let plist = head[open + 1..close].trim();
    if !plist.is_empty() && plist != "void" {
        for p in plist.split(',') {
            let mut parts = p.split_whitespace().collect::<Vec<_>>();
            let pname = parts.pop().ok_or("parameter")?.to_string();
            params.push((pname, parts.join(" ")));
        }
    }
    let mut locals = Vec::new();
    let mut body = Vec::new();
    for raw in lines {
        let line = raw.trim();
        if line.is_empty() || line == "}" {
            continue;
        }
        body.extend(parse_line(line, &mut locals)?);
    }
    Ok(CFunc {
        name,
        ret,
        params,
        locals,
        body,
    })
}

fn parse_line(line: &str, locals: &mut Vec<(String, String)>) -> Result<Vec<CStmt>, String> {
    if let Some(label) = line.strip_suffix(':') {
        if is_ident(label) {
            return Ok(vec![CStmt::Label(label.to_string())]);
        }
    }
    if let Some(rest) = line.strip_prefix("if (") {
        // `if (E) goto A; else goto B;`
        let close = rest
            .find(") goto ")
            .ok_or_else(|| format!("unparsed `{line}`"))?;
        let cond = parse_expr(&rest[..close])?;
        let tail = &rest[close + ") goto ".len()..];
        let (a, b) = tail
            .strip_suffix(';')
            .and_then(|t| t.split_once("; else goto "))
            .ok_or_else(|| format!("unparsed `{line}`"))?;
        return Ok(vec![CStmt::If(cond, a.to_string(), b.to_string())]);
    }
    let stmt = line
        .strip_suffix(';')
        .ok_or_else(|| format!("unparsed `{line}`"))?;
    if let Some(l) = stmt.strip_prefix("goto ") {
        return Ok(vec![CStmt::Goto(l.trim().to_string())]);
    }
    if stmt == "return" {
        return Ok(vec![CStmt::Ret(None)]);
    }
    if let Some(e) = stmt.strip_prefix("return ") {
        return Ok(vec![CStmt::Ret(Some(parse_expr(e)?))]);
    }
    let words: Vec<&str> = stmt.split_whitespace().collect();
    if words.len() == 2 && is_ident(words[1]) && !stmt.contains('=') && !stmt.contains('(') {
        locals.push((words[1].to_string(), words[0].to_string()));
        return Ok(Vec::new());
    }
    if let Some((lhs, rhs)) = stmt.split_once(" = ") {
        if lhs.trim().split('.').all(is_ident) {
            return Ok(vec![CStmt::Assign(
                lhs.trim().to_string(),
                parse_expr(rhs)?,
            )]);
        }
        return Err(format!("unparsed `{line}`"));
    }
    Ok(vec![CStmt::Eval(parse_expr(stmt)?)])
}

pub(super) fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
