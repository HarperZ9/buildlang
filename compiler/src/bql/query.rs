// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! The in-process BuildQL subset.
//!
//! The dialect is qdb's SQL. This evaluator runs the part of it a compiler
//! question needs and refuses the rest, naming qdb as the engine for it:
//!
//! ```text
//! SELECT [DISTINCT] * | col [, col]* FROM table
//!   [WHERE cond] [ORDER BY col [ASC|DESC] [, ...]] [LIMIT n]
//! cond   := term (OR term)*
//! term   := factor (AND factor)*
//! factor := col (= | != | <>) literal | col IS [NOT] NULL | ( cond )
//! ```
//!
//! NULL compares false with `=` and `!=`, as in qdb. ORDER BY puts NULL first,
//! sorts INTEGER columns by value and TEXT columns by bytes, and keeps table
//! order between equal keys.

use super::export::{tables, ColType, Table};
use super::store::Store;
use super::BqlError;

/// Column names and rows of a query result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

impl QueryResult {
    /// The values of one column, with NULL as an empty string.
    pub fn column(&self, name: &str) -> Vec<String> {
        let Some(i) = self.columns.iter().position(|c| c == name) else {
            return Vec::new();
        };
        self.rows
            .iter()
            .map(|r| r[i].clone().unwrap_or_default())
            .collect()
    }

    /// Plain-text rendering: a header, a rule and one line per row.
    pub fn render(&self) -> String {
        let mut widths: Vec<usize> = self.columns.iter().map(String::len).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.as_deref().unwrap_or("").len());
            }
        }
        let line = |cells: Vec<&str>| {
            cells
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{c:<w$}", w = widths[i]))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        let mut out = line(self.columns.iter().map(String::as_str).collect());
        out.push('\n');
        let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        out.push_str(&rule.join("-+-"));
        out.push('\n');
        for row in &self.rows {
            out.push_str(&line(
                row.iter().map(|c| c.as_deref().unwrap_or("")).collect(),
            ));
            out.push('\n');
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Int(i64),
    Sym(&'static str),
}

fn tokenize(sql: &str) -> Result<Vec<Tok>, BqlError> {
    let mut out = Vec::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || (c == ';' && i + 1 == chars.len()) {
            i += 1;
        } else if c == '\'' {
            let mut s = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None => return Err(BqlError::Query("unterminated string".into())),
                    Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                        s.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(ch) => {
                        s.push(*ch);
                        i += 1;
                    }
                }
            }
            out.push(Tok::Str(s));
        } else if c.is_ascii_digit()
            || (c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            i += 1;
            while chars.get(i).is_some_and(char::is_ascii_digit) {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Tok::Int(text.parse().map_err(|_| {
                BqlError::Query(format!("integer out of range: {text}"))
            })?));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while chars
                .get(i)
                .is_some_and(|ch| ch.is_alphanumeric() || *ch == '_')
            {
                i += 1;
            }
            out.push(Tok::Word(chars[start..i].iter().collect()));
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            let sym = match (two.as_str(), c) {
                ("!=", _) => "!=",
                ("<>", _) => "<>",
                (_, '=') => "=",
                (_, ',') => ",",
                (_, '*') => "*",
                (_, '(') => "(",
                (_, ')') => ")",
                _ => {
                    return Err(BqlError::Query(format!(
                        "'{c}' is outside the in-process subset; run the query in qdb"
                    )))
                }
            };
            i += sym.len();
            out.push(Tok::Sym(sym));
        }
    }
    Ok(out)
}

#[derive(Debug)]
enum Cond {
    Cmp { col: usize, neg: bool, lit: String },
    IsNull { col: usize, neg: bool },
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
}

impl Cond {
    fn eval(&self, row: &[Option<String>]) -> bool {
        match self {
            Cond::Cmp { col, neg, lit } => match &row[*col] {
                None => false,
                Some(v) => (v == lit) != *neg,
            },
            Cond::IsNull { col, neg } => row[*col].is_none() != *neg,
            Cond::And(a, b) => a.eval(row) && b.eval(row),
            Cond::Or(a, b) => a.eval(row) || b.eval(row),
        }
    }
}

struct P<'a> {
    toks: Vec<Tok>,
    pos: usize,
    table: Option<&'a Table>,
}

impl<'a> P<'a> {
    fn peek_kw(&self, kw: &str) -> bool {
        matches!(self.toks.get(self.pos), Some(Tok::Word(w)) if w.eq_ignore_ascii_case(kw))
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        let hit = self.peek_kw(kw);
        if hit {
            self.pos += 1;
        }
        hit
    }
    fn expect_kw(&mut self, kw: &str) -> Result<(), BqlError> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(BqlError::Query(format!("expected {kw}")))
        }
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        let hit = matches!(self.toks.get(self.pos), Some(Tok::Sym(x)) if *x == s);
        if hit {
            self.pos += 1;
        }
        hit
    }
    fn word(&mut self, what: &str) -> Result<String, BqlError> {
        match self.toks.get(self.pos) {
            Some(Tok::Word(w)) => {
                self.pos += 1;
                Ok(w.clone())
            }
            _ => Err(BqlError::Query(format!("expected {what}"))),
        }
    }
    fn col(&mut self) -> Result<usize, BqlError> {
        let name = self.word("a column name")?;
        let table = self.table.expect("table resolved before columns");
        table
            .column(&name)
            .ok_or_else(|| BqlError::Query(format!("table {} has no column {name}", table.name)))
    }
    fn cond(&mut self) -> Result<Cond, BqlError> {
        let mut left = self.term()?;
        while self.eat_kw("OR") {
            left = Cond::Or(Box::new(left), Box::new(self.term()?));
        }
        Ok(left)
    }
    fn term(&mut self) -> Result<Cond, BqlError> {
        let mut left = self.factor()?;
        while self.eat_kw("AND") {
            left = Cond::And(Box::new(left), Box::new(self.factor()?));
        }
        Ok(left)
    }
    fn factor(&mut self) -> Result<Cond, BqlError> {
        if self.eat_sym("(") {
            let c = self.cond()?;
            if !self.eat_sym(")") {
                return Err(BqlError::Query("expected )".into()));
            }
            return Ok(c);
        }
        let col = self.col()?;
        if self.eat_kw("IS") {
            let neg = self.eat_kw("NOT");
            self.expect_kw("NULL")?;
            return Ok(Cond::IsNull { col, neg });
        }
        let neg = if self.eat_sym("=") {
            false
        } else if self.eat_sym("!=") || self.eat_sym("<>") {
            true
        } else {
            return Err(BqlError::Query(
                "only =, != and IS [NOT] NULL run in process; run other operators in qdb".into(),
            ));
        };
        let lit = match self.toks.get(self.pos) {
            Some(Tok::Str(s)) => s.clone(),
            Some(Tok::Int(n)) => n.to_string(),
            _ => {
                return Err(BqlError::Query(
                    "expected a literal after the operator".into(),
                ))
            }
        };
        self.pos += 1;
        Ok(Cond::Cmp { col, neg, lit })
    }
}

/// Run one SELECT from the in-process subset against a store.
///
/// A refusal names qdb, which runs the full dialect over `buildc bql export`.
pub fn run_query(store: &Store, sql: &str) -> Result<QueryResult, BqlError> {
    run_select(store, sql).map_err(|e| match e {
        BqlError::Query(m) if !m.contains("qdb") => BqlError::Query(format!(
            "{m} (the in-process subset is SELECT, WHERE, ORDER BY and LIMIT over one table;              export with `buildc bql export` and run the full dialect in qdb)"
        )),
        other => other,
    })
}

fn run_select(store: &Store, sql: &str) -> Result<QueryResult, BqlError> {
    let all = tables(store);
    let mut p = P {
        toks: tokenize(sql)?,
        pos: 0,
        table: None,
    };
    p.expect_kw("SELECT")?;
    let distinct = p.eat_kw("DISTINCT");
    let mut names = Vec::new();
    let star = p.eat_sym("*");
    if !star {
        loop {
            names.push(p.word("a column name")?);
            if !p.eat_sym(",") {
                break;
            }
        }
    }
    p.expect_kw("FROM")?;
    let tname = p.word("a table name")?;
    let table = all.iter().find(|t| t.name == tname).ok_or_else(|| {
        BqlError::Query(format!(
            "no table {tname}; tables are {}",
            super::export::TABLES.join(", ")
        ))
    })?;
    p.table = Some(table);
    let picks: Vec<usize> = if star {
        (0..table.columns.len()).collect()
    } else {
        names
            .iter()
            .map(|n| {
                table
                    .column(n)
                    .ok_or_else(|| BqlError::Query(format!("table {tname} has no column {n}")))
            })
            .collect::<Result<_, _>>()?
    };
    let filter = if p.eat_kw("WHERE") {
        Some(p.cond()?)
    } else {
        None
    };
    let mut order = Vec::new();
    if p.eat_kw("ORDER") {
        p.expect_kw("BY")?;
        loop {
            let col = p.col()?;
            let desc = p.eat_kw("DESC");
            if !desc {
                p.eat_kw("ASC");
            }
            order.push((col, desc));
            if !p.eat_sym(",") {
                break;
            }
        }
    }
    let limit = if p.eat_kw("LIMIT") {
        match p.toks.get(p.pos) {
            Some(Tok::Int(n)) if *n >= 0 => {
                p.pos += 1;
                Some(*n as usize)
            }
            _ => return Err(BqlError::Query("LIMIT takes a non-negative integer".into())),
        }
    } else {
        None
    };
    if p.pos != p.toks.len() {
        return Err(BqlError::Query(format!(
            "unexpected text after the query (token {}); joins, grouping and aggregates run in qdb",
            p.pos + 1
        )));
    }

    let mut rows: Vec<&Vec<Option<String>>> = table
        .rows
        .iter()
        .filter(|r| filter.as_ref().is_none_or(|c| c.eval(r)))
        .collect();
    rows.sort_by(|a, b| {
        for (col, desc) in &order {
            let ord = match (&a[*col], &b[*col]) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) if table.columns[*col].1 == ColType::Integer => x
                    .parse::<i64>()
                    .unwrap_or(0)
                    .cmp(&y.parse::<i64>().unwrap_or(0)),
                (Some(x), Some(y)) => x.cmp(y),
            };
            let ord = if *desc { ord.reverse() } else { ord };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    });
    let mut out_rows: Vec<Vec<Option<String>>> = Vec::new();
    for r in rows {
        let projected: Vec<Option<String>> = picks.iter().map(|i| r[*i].clone()).collect();
        if distinct && out_rows.contains(&projected) {
            continue;
        }
        out_rows.push(projected);
    }
    if let Some(n) = limit {
        out_rows.truncate(n);
    }
    Ok(QueryResult {
        columns: picks
            .iter()
            .map(|i| table.columns[*i].0.to_string())
            .collect(),
        rows: out_rows,
    })
}
