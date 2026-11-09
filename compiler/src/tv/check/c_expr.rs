// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The expression grammar of the emitted C: a lexer and a precedence
//! parser for the right sides of three-address statements.

use super::c_parse::{is_scalar, CExpr};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Id(String),
    Num(i128),
    Flt(String),
    Str(String),
    Op(&'static str),
}

const OPS: [&str; 28] = [
    "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "&", "|",
    "^", "!", "~", "(", ")", ",", "?", ":", ".", "{", "}",
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
                    if self.t.get(self.i + 2) == Some(&Tok::Op("{")) {
                        let ty = ty.clone();
                        self.i += 3;
                        let mut items = Vec::new();
                        if !self.eat("}") {
                            loop {
                                items.push(self.ternary()?);
                                if self.eat("}") {
                                    break;
                                }
                                if !self.eat(",") {
                                    return Err("compound literal items".into());
                                }
                            }
                        }
                        return Ok(CExpr::Compound(ty, items));
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
