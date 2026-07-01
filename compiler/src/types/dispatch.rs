// ===============================================================================
// BUILDLANG TYPE SYSTEM - MULTIPLE DISPATCH RESOLVER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Shared static multiple-dispatch resolver.
//!
//! Julia-style multiple dispatch: multiple functions may share one name, each
//! with a different parameter-type signature. A call selects the method whose
//! parameter types best match the tuple of ALL argument types (not just the
//! first / receiver). Resolution is static: the argument-type tuple is known at
//! type-check time (and again at codegen), so the selected method is chosen once
//! and emitted as a direct call.
//!
//! This module holds the ONE ranking algorithm. Both the type checker
//! (`types::Ty`) and codegen (`MirType`) call [`resolve_overload`], supplying a
//! per-position match function that answers "how well does this argument match
//! this parameter?" (exact / coercion / generic / no match). The algorithm below
//! filters by arity, scores each candidate position-wise, and picks the unique
//! most-specific one. Both sides therefore agree on the selected method by
//! construction; they never duplicate the ranking logic.

/// How well a single argument matches a single parameter position.
///
/// Ordered from most specific to least specific. A concrete parameter that is
/// exactly the argument type is more specific than one reached by coercion,
/// which is more specific than a generic (type-parameter) parameter that
/// matches anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PositionMatch {
    /// Parameter type is exactly the argument type (most specific).
    Exact,
    /// Argument is coercible to the parameter (subtype / reborrow / numeric).
    Coercion,
    /// Parameter is a generic type parameter that matches any argument.
    Generic,
}

impl PositionMatch {
    /// Specificity weight; higher is more specific. Summed across positions to
    /// rank whole candidates. Exact beats coercion beats generic per position.
    fn weight(self) -> u32 {
        match self {
            PositionMatch::Exact => 3,
            PositionMatch::Coercion => 2,
            PositionMatch::Generic => 1,
        }
    }
}

/// The outcome of resolving an overloaded call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// No candidate's parameters match the argument tuple (after arity filter).
    NoMatchingMethod,
    /// Two or more equally-most-specific candidates; the call is ambiguous.
    /// Carries the indices of the tied candidates (into the input slice).
    AmbiguousMethod(Vec<usize>),
}

/// A resolvable candidate: its number of parameters plus enough identity for
/// the caller to map the winning index back to a definition. The ranking only
/// needs `arity`; identity lives in the caller's parallel candidate list.
pub trait DispatchCandidate {
    /// Number of declared parameters (the arity to match against the args).
    fn arity(&self) -> usize;
}

/// Resolve an overloaded call to the unique most-specific candidate.
///
/// * `candidates`: every method sharing the call's name.
/// * `n_args`: the number of arguments at the call site.
/// * `position_match`: `(candidate_index, position) -> Option<PositionMatch>`:
///   `None` means that position does not match at all (rules the candidate out);
///   `Some(kind)` reports how well it matches. Called only for candidates whose
///   arity equals `n_args`.
///
/// Returns the index (into `candidates`) of the selected method, or a
/// [`DispatchError`]. The algorithm is intentionally simple and sound: it
/// over-reports ambiguity rather than silently guessing. A candidate is
/// selected only when it is strictly more specific than every other viable
/// candidate (dominates position-wise, i.e. no worse anywhere and strictly
/// better somewhere), OR it is the sole viable candidate.
pub fn resolve_overload<C, F>(
    candidates: &[C],
    n_args: usize,
    mut position_match: F,
) -> Result<usize, DispatchError>
where
    C: DispatchCandidate,
    F: FnMut(usize, usize) -> Option<PositionMatch>,
{
    // 1. Arity filter, then per-position match. A candidate is viable only if
    //    every position matches (Some(..)).
    let mut viable: Vec<(usize, Vec<PositionMatch>)> = Vec::new();
    for (idx, cand) in candidates.iter().enumerate() {
        if cand.arity() != n_args {
            continue;
        }
        let mut positions = Vec::with_capacity(n_args);
        let mut all_match = true;
        for pos in 0..n_args {
            match position_match(idx, pos) {
                Some(m) => positions.push(m),
                None => {
                    all_match = false;
                    break;
                }
            }
        }
        if all_match {
            viable.push((idx, positions));
        }
    }

    match viable.len() {
        0 => Err(DispatchError::NoMatchingMethod),
        1 => Ok(viable[0].0),
        _ => pick_most_specific(&viable),
    }
}

/// Given 2+ viable candidates (each with its per-position match kinds), pick the
/// unique candidate that dominates all others. Domination is the partial order
/// "no worse at any position, strictly better at some position". If a single
/// candidate dominates every other, it wins. Otherwise the top tier (by summed
/// specificity) is ambiguous.
fn pick_most_specific(viable: &[(usize, Vec<PositionMatch>)]) -> Result<usize, DispatchError> {
    // A candidate `a` dominates `b` when a is no-worse at every position and
    // strictly-better at some position (Julia-style specificity partial order).
    let dominates = |a: &[PositionMatch], b: &[PositionMatch]| -> bool {
        debug_assert_eq!(a.len(), b.len());
        let mut strictly_better_somewhere = false;
        for (pa, pb) in a.iter().zip(b.iter()) {
            // More specific == higher weight.
            if pa.weight() < pb.weight() {
                return false; // worse at this position → cannot dominate
            }
            if pa.weight() > pb.weight() {
                strictly_better_somewhere = true;
            }
        }
        strictly_better_somewhere
    };

    // Find candidates that are dominated by no one (the maximal set).
    let mut maximal: Vec<usize> = Vec::new();
    for (i, (_, pi)) in viable.iter().enumerate() {
        let dominated = viable
            .iter()
            .enumerate()
            .any(|(j, (_, pj))| i != j && dominates(pj, pi));
        if !dominated {
            maximal.push(i);
        }
    }

    match maximal.len() {
        1 => Ok(viable[maximal[0]].0),
        _ => {
            // Two or more incomparable maxima → ambiguous. Report their
            // original candidate indices for a helpful diagnostic.
            let tied: Vec<usize> = maximal.iter().map(|&i| viable[i].0).collect();
            Err(DispatchError::AmbiguousMethod(tied))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal candidate for unit tests: a list of per-position "kinds" is
    /// supplied directly, keyed by the abstract param descriptor below.
    struct Cand {
        params: Vec<Param>,
    }

    /// A test parameter is either a concrete type name or a generic slot.
    #[derive(Clone, PartialEq)]
    enum Param {
        Concrete(&'static str),
        Generic,
    }

    impl DispatchCandidate for Cand {
        fn arity(&self) -> usize {
            self.params.len()
        }
    }

    /// Build the match function from a list of concrete argument type names.
    /// Exact when a concrete param equals the arg; Generic when the param is a
    /// type slot; None when concrete param disagrees with the arg. (Coercion is
    /// exercised via an explicit variant in the coercion test.)
    fn matcher<'a>(
        cands: &'a [Cand],
        args: &'a [&'static str],
    ) -> impl FnMut(usize, usize) -> Option<PositionMatch> + 'a {
        move |ci, pos| {
            let p = &cands[ci].params[pos];
            let a = args[pos];
            match p {
                Param::Concrete(name) if *name == a => Some(PositionMatch::Exact),
                Param::Concrete(_) => None,
                Param::Generic => Some(PositionMatch::Generic),
            }
        }
    }

    fn c(params: &[Param]) -> Cand {
        Cand {
            params: params.to_vec(),
        }
    }

    #[test]
    fn selects_i32_add() {
        // add(i32,i32), add(f64,f64), add(&str,&str) ; call add(i32,i32)
        let cands = vec![
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
            c(&[Param::Concrete("f64"), Param::Concrete("f64")]),
            c(&[Param::Concrete("str"), Param::Concrete("str")]),
        ];
        let args = ["i32", "i32"];
        let sel = resolve_overload(&cands, 2, matcher(&cands, &args)).unwrap();
        assert_eq!(sel, 0);
    }

    #[test]
    fn selects_f64_add() {
        let cands = vec![
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
            c(&[Param::Concrete("f64"), Param::Concrete("f64")]),
            c(&[Param::Concrete("str"), Param::Concrete("str")]),
        ];
        let args = ["f64", "f64"];
        let sel = resolve_overload(&cands, 2, matcher(&cands, &args)).unwrap();
        assert_eq!(sel, 1);
    }

    #[test]
    fn selects_str_add() {
        let cands = vec![
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
            c(&[Param::Concrete("f64"), Param::Concrete("f64")]),
            c(&[Param::Concrete("str"), Param::Concrete("str")]),
        ];
        let args = ["str", "str"];
        let sel = resolve_overload(&cands, 2, matcher(&cands, &args)).unwrap();
        assert_eq!(sel, 2);
    }

    #[test]
    fn dispatches_on_second_arg() {
        // f(i32,i32) vs f(i32,f64) proves dispatch is NOT receiver-only.
        let cands = vec![
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
            c(&[Param::Concrete("i32"), Param::Concrete("f64")]),
        ];
        let sel_ii = resolve_overload(&cands, 2, matcher(&cands, &["i32", "i32"])).unwrap();
        assert_eq!(sel_ii, 0);
        let sel_if = resolve_overload(&cands, 2, matcher(&cands, &["i32", "f64"])).unwrap();
        assert_eq!(sel_if, 1);
    }

    #[test]
    fn concrete_beats_generic() {
        // g<T>(T,T) and g(i32,i32) ; call g(i32,i32) → the concrete one wins.
        let cands = vec![
            c(&[Param::Generic, Param::Generic]),
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
        ];
        let args = ["i32", "i32"];
        let sel = resolve_overload(&cands, 2, matcher(&cands, &args)).unwrap();
        assert_eq!(sel, 1); // the concrete candidate
    }

    #[test]
    fn generic_used_when_no_concrete_matches() {
        // g<T>(T,T) and g(i32,i32) ; call g(f64,f64) → only the generic matches.
        let cands = vec![
            c(&[Param::Generic, Param::Generic]),
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
        ];
        let args = ["f64", "f64"];
        let sel = resolve_overload(&cands, 2, matcher(&cands, &args)).unwrap();
        assert_eq!(sel, 0); // the generic candidate
    }

    #[test]
    fn no_matching_method() {
        let cands = vec![
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
            c(&[Param::Concrete("f64"), Param::Concrete("f64")]),
        ];
        let args = ["str", "str"];
        let err = resolve_overload(&cands, 2, matcher(&cands, &args)).unwrap_err();
        assert_eq!(err, DispatchError::NoMatchingMethod);
    }

    #[test]
    fn arity_filter_excludes_wrong_length() {
        let cands = vec![
            c(&[Param::Concrete("i32")]),
            c(&[Param::Concrete("i32"), Param::Concrete("i32")]),
        ];
        let sel = resolve_overload(&cands, 1, matcher(&cands, &["i32"])).unwrap();
        assert_eq!(sel, 0);
    }

    #[test]
    fn ambiguous_incomparable_pair() {
        // f(i32, T) vs f(T, i32) ; call f(i32, i32).
        // Candidate A matches (Exact, Generic); B matches (Generic, Exact).
        // Neither dominates the other → AmbiguousMethod.
        let cands = vec![
            c(&[Param::Concrete("i32"), Param::Generic]),
            c(&[Param::Generic, Param::Concrete("i32")]),
        ];
        let args = ["i32", "i32"];
        let err = resolve_overload(&cands, 2, matcher(&cands, &args)).unwrap_err();
        match err {
            DispatchError::AmbiguousMethod(tied) => {
                assert_eq!(tied.len(), 2);
                assert!(tied.contains(&0) && tied.contains(&1));
            }
            other => panic!("expected AmbiguousMethod, got {:?}", other),
        }
    }

    #[test]
    fn coercion_beats_generic_but_loses_to_exact() {
        // Custom matcher exercising the Coercion tier directly.
        // Three candidates against a single arg:
        //   A: Exact, B: Coercion, C: Generic → A wins.
        struct One;
        impl DispatchCandidate for One {
            fn arity(&self) -> usize {
                1
            }
        }
        let cands = vec![One, One, One];
        let kinds = [
            PositionMatch::Exact,
            PositionMatch::Coercion,
            PositionMatch::Generic,
        ];
        let sel = resolve_overload(&cands, 1, |ci, _pos| Some(kinds[ci])).unwrap();
        assert_eq!(sel, 0);

        // Without the exact one, coercion beats generic.
        let cands2 = vec![One, One];
        let kinds2 = [PositionMatch::Coercion, PositionMatch::Generic];
        let sel2 = resolve_overload(&cands2, 1, |ci, _pos| Some(kinds2[ci])).unwrap();
        assert_eq!(sel2, 0);
    }
}
