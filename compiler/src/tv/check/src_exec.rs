// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Symbolic execution of a source function between cut points.
//!
//! The cut points are the function entry and the head of every loop written
//! as a statement of a block. A segment runs from one cut to the next cut or
//! to a return. Statements between loops run through the verifier's
//! symbolic executor with calls left uninterpreted; the loop structure is
//! walked here with a stack of block frames, so the source semantics never
//! passes through the compiler's lowering.

use crate::ast::*;
use crate::verify::sym::{Out, St, Sym};
use crate::verify::term::{not, Term};
use crate::verify::ty::Ty;
use std::collections::HashMap;

/// A statement-level loop, numbered in source pre-order.
pub struct LoopInfo<'a> {
    pub stmt: *const Stmt,
    pub cond: Option<&'a Expr>,
    pub body: &'a Block,
}

fn loop_of(s: &Stmt) -> Option<(Option<&Expr>, &Block)> {
    let e = match &s.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => e,
        _ => return None,
    };
    match &e.kind {
        ExprKind::While {
            condition,
            body,
            label: None,
        } => Some((Some(condition), body)),
        ExprKind::Loop { body, label: None } => Some((None, body)),
        _ => None,
    }
}

/// The loops of `body` in pre-order.
pub fn loops(body: &Block) -> Vec<LoopInfo<'_>> {
    fn walk<'a>(stmts: &'a [Stmt], out: &mut Vec<LoopInfo<'a>>) {
        for s in stmts {
            if let Some((cond, body)) = loop_of(s) {
                out.push(LoopInfo {
                    stmt: s as *const Stmt,
                    cond,
                    body,
                });
                walk(&body.stmts, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&body.stmts, &mut out);
    out
}

#[derive(Clone)]
pub struct Frame<'a> {
    stmts: &'a [Stmt],
    idx: usize,
    depth: usize,
    loop_id: Option<usize>,
}

/// Where a source path left the segment.
#[derive(Clone, Debug)]
pub enum SExit {
    Cut(usize),
    Ret(Term),
}

/// The block frames and variables in scope at a loop head.
#[derive(Clone)]
pub struct Arrival<'a> {
    pub frames: Vec<Frame<'a>>,
    pub vars: Vec<(String, Ty)>,
}

pub struct SrcExec<'a> {
    pub sym: Sym<'a>,
    pub body: &'a Block,
    pub loops: Vec<LoopInfo<'a>>,
    pub ret: Option<Ty>,
    pub arrivals: HashMap<usize, Arrival<'a>>,
}

enum Pos {
    Run,
    Head(usize),
}

impl<'a> SrcExec<'a> {
    fn loop_id(&self, s: &Stmt) -> Option<usize> {
        self.loops.iter().position(|l| std::ptr::eq(l.stmt, s))
    }

    /// Run the segment from the entry (`None`) or from loop head `k`.
    pub fn run(
        &mut self,
        cut: Option<usize>,
        st: St,
        params: usize,
    ) -> Result<Vec<(St, SExit)>, String> {
        let (frames, pos) = match cut {
            None => (
                vec![Frame {
                    stmts: &self.body.stmts,
                    idx: 0,
                    depth: params,
                    loop_id: None,
                }],
                Pos::Run,
            ),
            Some(k) => {
                let mut frames = self
                    .arrivals
                    .get(&k)
                    .ok_or("a loop head never reached")?
                    .frames
                    .clone();
                frames.last_mut().ok_or("no frame")?.idx += 1;
                (frames, Pos::Head(k))
            }
        };
        let mut out = Vec::new();
        let mut work = vec![(st, frames, pos)];
        while let Some((st, frames, pos)) = work.pop() {
            if out.len() + work.len() > self.sym.max_paths {
                return Err("too many paths".into());
            }
            if let Pos::Head(k) = pos {
                let info = &self.loops[k];
                let body = info.body;
                let enter = |s: St, frames: &Vec<Frame<'a>>| {
                    let mut f = frames.clone();
                    f.push(Frame {
                        stmts: &body.stmts,
                        idx: 0,
                        depth: s.vars.len(),
                        loop_id: Some(k),
                    });
                    (s, f, Pos::Run)
                };
                match info.cond {
                    None => work.push(enter(st, &frames)),
                    Some(c) => {
                        for (mut s, o) in self.sym.expr(c, st, Some(Ty::Bool))? {
                            let Out::Val(c, _) = o else {
                                return Err("control flow in a loop condition".into());
                            };
                            let mut t = s.clone();
                            t.assume(c.clone());
                            s.assume(not(c));
                            work.push(enter(t, &frames));
                            work.push((s, frames.clone(), Pos::Run));
                        }
                    }
                }
                continue;
            }
            let top = frames.last().ok_or("no frame")?.clone();
            if top.idx == top.stmts.len() {
                let mut st = st;
                match top.loop_id {
                    Some(k) => {
                        st.vars.truncate(top.depth);
                        out.push((st, SExit::Cut(k)));
                    }
                    None => return Err("the function ends without a value".into()),
                }
                continue;
            }
            let stmt = &top.stmts[top.idx];
            if let Some(k) = self.loop_id(stmt) {
                let vars = st.vars.iter().map(|v| (v.0.clone(), v.2)).collect();
                self.arrivals.entry(k).or_insert(Arrival {
                    frames: frames.clone(),
                    vars,
                });
                out.push((st, SExit::Cut(k)));
                continue;
            }
            let is_value = frames.len() == 1
                && top.idx + 1 == top.stmts.len()
                && matches!(stmt.kind, StmtKind::Expr(_));
            let rest = &top.stmts[top.idx + 1..];
            for (s, o) in self.sym.stmt(stmt, rest, st, is_value, self.ret)? {
                match o {
                    Out::Val(v, _) if is_value => out.push((s, SExit::Ret(v))),
                    Out::Val(..) => {
                        let mut f = frames.clone();
                        f.last_mut().unwrap().idx += 1;
                        work.push((s, f, Pos::Run));
                    }
                    Out::Ret(v, _) => out.push((s, SExit::Ret(v))),
                    Out::Break | Out::Continue => {
                        let at = frames
                            .iter()
                            .rposition(|f| f.loop_id.is_some())
                            .ok_or("break outside a loop")?;
                        let body_frame = frames[at].clone();
                        let mut s = s;
                        s.vars.truncate(body_frame.depth);
                        if matches!(o, Out::Continue) {
                            out.push((s, SExit::Cut(body_frame.loop_id.unwrap())));
                        } else {
                            let f: Vec<Frame<'a>> = frames[..at].to_vec();
                            work.push((s, f, Pos::Run));
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}
