// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A parser for the C the backend emits for one function, written for the
//! checker alone. It reads the generated text, not the code generator's data,
//! and accepts only the three-address shape: declarations, labels,
//! assignments, conditional and plain `goto`, `return` and call statements.
//! Any other line makes the function uncertifiable.

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
        if is_ident(lhs.trim()) {
            return Ok(vec![CStmt::Assign(
                lhs.trim().to_string(),
                parse_expr(rhs)?,
            )]);
        }
        return Err(format!("unparsed `{line}`"));
    }
    Ok(vec![CStmt::Eval(parse_expr(stmt)?)])
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Id(String),
    Num(i128),
    Flt(String),
    Str(String),
    Op(&'static str),
}

const OPS: [&str; 26] = [
    "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "&", "|",
    "^", "!", "~", "(", ")", ",", "?", ":", ".",
];

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' {
            let (text, end) = super::c_effects::c_string(s, i)?;
            out.push(Tok::Str(text));
            i = end;
        } else if c.is_ascii_digit() && float_end(b, i).is_some() {
            let end = float_end(b, i).unwrap_or(i);
            out.push(Tok::Flt(s[i..end].to_string()));
            i = end;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i] as char).is_ascii_digit() {
                i += 1;
            }
            let v: i128 = s[start..i]
                .parse()
                .map_err(|_| format!("number in `{s}`"))?;
            while i < b.len() && matches!(b[i], b'U' | b'L' | b'u' | b'l') {
                i += 1;
            }
            out.push(Tok::Num(v));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Id(s[start..i].to_string()));
        } else {
            let op = OPS
                .iter()
                .find(|op| s[i..].starts_with(**op))
                .ok_or_else(|| format!("character `{c}` in `{s}`"))?;
            out.push(Tok::Op(op));
            i += op.len();
        }
    }
    Ok(out)
}

/// The end of a decimal float literal starting at `i` (digits, then a `.`
/// or an exponent, then an optional `f`), or `None` for an integer.
fn float_end(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    let mut is_float = false;
    if j < b.len() && b[j] == b'.' {
        is_float = true;
        j += 1;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
    }
    if j < b.len() && (b[j] == b'e' || b[j] == b'E') {
        let mut k = j + 1;
        if k < b.len() && (b[k] == b'+' || b[k] == b'-') {
            k += 1;
        }
        if k < b.len() && b[k].is_ascii_digit() {
            is_float = true;
            j = k;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
        }
    }
    if !is_float {
        return None;
    }
    if j < b.len() && (b[j] == b'f' || b[j] == b'F') {
        j += 1;
    }
    Some(j)
}

/// Parse one C expression.
pub fn parse_expr(s: &str) -> Result<CExpr, String> {
    let toks = lex(s)?;
    let mut p = P { t: toks, i: 0 };
    let e = p.ternary()?;
    if p.i != p.t.len() {
        return Err(format!("trailing tokens in `{s}`"));
    }
    Ok(e)
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

const LEVELS: [&[&str]; 10] = [
    &["||"],
    &["&&"],
    &["|"],
    &["^"],
    &["&"],
    &["==", "!="],
    &["<", "<=", ">", ">="],
    &["<<", ">>"],
    &["+", "-"],
    &["*", "/", "%"],
];

impl P {
    fn peek_op(&self) -> Option<&'static str> {
        match self.t.get(self.i) {
            Some(Tok::Op(o)) => Some(o),
            _ => None,
        }
    }

    fn eat(&mut self, op: &str) -> bool {
        if self.peek_op() == Some(op) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn ternary(&mut self) -> Result<CExpr, String> {
        let c = self.binary(0)?;
        if !self.eat("?") {
            return Ok(c);
        }
        let a = self.ternary()?;
        if !self.eat(":") {
            return Err("a `?` without `:`".into());
        }
        let b = self.ternary()?;
        Ok(CExpr::Cond(Box::new(c), Box::new(a), Box::new(b)))
    }

    fn binary(&mut self, level: usize) -> Result<CExpr, String> {
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        while let Some(op) = self.peek_op().filter(|o| LEVELS[level].contains(o)) {
            self.i += 1;
            let right = self.binary(level + 1)?;
            left = CExpr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<CExpr, String> {
        for op in ['!', '-', '~'] {
            if self.eat(&op.to_string()) {
                let inner = self.unary()?;
                return Ok(match (op, inner) {
                    ('-', CExpr::Int(v)) => CExpr::Int(-v),
                    ('-', CExpr::Float(t)) if !t.starts_with('-') => CExpr::Float(format!("-{t}")),
                    (o, e) => CExpr::Un(o, Box::new(e)),
                });
            }
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<CExpr, String> {
        let mut e = self.atom()?;
        while self.eat(".") {
            match self.t.get(self.i).cloned() {
                Some(Tok::Id(f)) => {
                    self.i += 1;
                    e = CExpr::Member(Box::new(e), f);
                }
                _ => return Err("a member name".into()),
            }
        }
        Ok(e)
    }

    fn atom(&mut self) -> Result<CExpr, String> {
        match self.t.get(self.i).cloned() {
            Some(Tok::Str(t)) => {
                self.i += 1;
                Ok(CExpr::Str(t))
            }
            Some(Tok::Num(v)) => {
                self.i += 1;
                Ok(CExpr::Int(v))
            }
            Some(Tok::Flt(t)) => {
                self.i += 1;
                Ok(CExpr::Float(t))
            }
            Some(Tok::Id(name)) => {
                self.i += 1;
                if name == "true" || name == "false" {
                    return Ok(CExpr::Bool(name == "true"));
                }
                if self.eat("(") {
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.ternary()?);
                            if self.eat(")") {
                                break;
                            }
                            if !self.eat(",") {
                                return Err("call arguments".into());
                            }
                        }
                    }
                    return Ok(CExpr::Call(name, args));
                }
                Ok(CExpr::Var(name))
            }
            Some(Tok::Op("(")) => {
                self.i += 1;
                if let (Some(Tok::Id(ty)), Some(Tok::Op(")"))) =
                    (self.t.get(self.i), self.t.get(self.i + 1))
                {
                    if is_scalar(ty) {
                        let ty = ty.clone();
                        self.i += 2;
                        let inner = self.unary()?;
                        return Ok(CExpr::Cast(ty, Box::new(inner)));
                    }
                }
                let e = self.ternary()?;
                if !self.eat(")") {
                    return Err("unclosed parenthesis".into());
                }
                Ok(e)
            }
            other => Err(format!("unexpected token {other:?}")),
        }
    }
}

/// The top-level `const` definitions of a C file whose type is a modelled
/// scalar and whose initializer is a literal: name to value and type.
pub fn c_consts(
    c: &str,
) -> std::collections::HashMap<String, (crate::verify::term::Term, crate::verify::ty::Ty)> {
    use crate::verify::{float, term::Term, ty::Ty};
    let mut out = std::collections::HashMap::new();
    for line in c.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let rest = line
            .trim_end()
            .strip_prefix("static ")
            .unwrap_or(line.trim_end());
        let Some(rest) = rest.strip_prefix("const ") else {
            continue;
        };
        let Some((decl, init)) = rest.split_once(" = ") else {
            continue;
        };
        let Some(init) = init.strip_suffix(';') else {
            continue;
        };
        let mut words: Vec<&str> = decl.split_whitespace().collect();
        let (Some(name), Some(ty)) = (words.pop(), words.pop()) else {
            continue;
        };
        if !words.is_empty() || !is_ident(name) {
            continue;
        }
        let Some(t) = super::c_exec::c_ty(ty) else {
            continue;
        };
        let term = match (parse_expr(init), t) {
            (Ok(CExpr::Int(v)), t) if t.is_int() && t.holds(v) => Term::Int(v),
            (Ok(CExpr::Bool(b)), Ty::Bool) => Term::Bool(b),
            (Ok(CExpr::Float(text)), Ty::Float { bits: 64 }) if !text.ends_with(['f', 'F']) => {
                match text.parse::<f64>() {
                    Ok(v) => float::fconst(Ty::F64, v),
                    Err(_) => continue,
                }
            }
            (Ok(CExpr::Float(text)), Ty::Float { bits: 32 }) => {
                match text.trim_end_matches(['f', 'F']).parse::<f32>() {
                    Ok(v) => float::fconst32_bits(v.to_bits()),
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        out.insert(name.to_string(), (term, t));
    }
    out
}
