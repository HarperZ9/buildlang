//! Vector reads and writes as plain C array accesses, with the bounds check kept.
//!
//! A vector lives behind a handle (`BuildVecHandle`, a pointer to a `BuildVec`
//! holding the buffer, the length and the element size). Every `v[i]` went
//! through `build_vec_get`, which reloads the handle, the buffer and the length
//! from memory. In a function that cannot resize or replace any vector, those
//! three words are fixed for the whole call, so they are loaded once at entry
//! and each access becomes `*(T*)bl_vat(buf, len, size, i)`: the same bounds
//! check and the same abort message, on values the C compiler keeps in
//! registers.
//!
//! A function qualifies when every call it makes is a pure maths routine and
//! no statement writes through a pointer to a vector handle. Anything that can
//! push, pop, insert, remove, clear or reassign a vector is a call or such a
//! write, so it disqualifies the function and its accesses stay on the runtime
//! path. So does a store that could swap a handle behind a parameter through an
//! aliasing pointer (`vec_hoist_stores.rs`). Only vectors reached from a
//! parameter (`&Vec<T>`, `&mut Vec<T>` or `Vec<T>`) are hoisted.

use std::collections::{HashMap, HashSet};

use super::vec_hoist_stores::store_may_replace_handle;
use super::CBackend;
use crate::codegen::ir::{
    FloatSize, LocalId, MirFunction, MirModule, MirRValue, MirStmtKind, MirTerminator, MirType,
    MirValue,
};

/// Calls a hoisting function may make: maths routines that touch no vector.
const PURE_CALLS: &[&str] = &[
    "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "exp", "exp2", "log", "log2",
    "log10", "pow", "fabs", "floor", "ceil", "round", "trunc", "fmin", "fmax", "fmod", "hypot",
    "cbrt", "abs",
];

/// Element writes and length reads: calls that never resize a vector.
const VEC_ELEMENT_CALLS: &[&str] = &["build_hvec_len"];

fn is_pure_call(name: &str) -> bool {
    PURE_CALLS.contains(&name) || name.starts_with("build_det_")
}

fn is_allowed_call(name: &str, vector_free: &HashSet<String>) -> bool {
    is_pure_call(name)
        || VEC_ELEMENT_CALLS.contains(&name)
        || name.starts_with("build_hvec_set_")
        || vector_free.contains(name)
}

/// Whether a type can hold or reach a vector or map.
fn reaches_heap_collection(ty: &MirType) -> bool {
    match ty {
        MirType::Vec(_) | MirType::Map(_, _) => true,
        MirType::Ptr(inner) | MirType::Slice(inner) | MirType::Array(inner, _) => {
            reaches_heap_collection(inner)
        }
        MirType::Option(inner) => reaches_heap_collection(inner),
        MirType::Tuple(ts) => ts.iter().any(reaches_heap_collection),
        // A struct may hold a vector field; treat every user struct as able to.
        MirType::Struct(n) => n.as_ref() != "BuildString",
        _ => false,
    }
}

/// User functions that cannot resize any vector: no local, parameter or
/// return value that can hold or reach a vector, map or struct, no global
/// store, and every call is a maths routine or another such function.
/// Computed to a fixed point over the module, so recursion is handled.
pub(super) fn vector_free_functions(module: &MirModule) -> HashSet<String> {
    let mut free: HashSet<String> = module
        .functions
        .iter()
        .filter(|f| {
            f.blocks.is_some()
                && !f.locals.iter().any(|l| reaches_heap_collection(&l.ty))
                && !reaches_heap_collection(&f.sig.ret)
                && !f.blocks.iter().flatten().any(|b| {
                    b.stmts
                        .iter()
                        .any(|st| matches!(st.kind, MirStmtKind::GlobalStore { .. }))
                })
        })
        .map(|f| f.name.to_string())
        .collect();
    loop {
        let before = free.len();
        let snapshot = free.clone();
        free.retain(|name| {
            let Some(f) = module.functions.iter().find(|f| f.name.as_ref() == name) else {
                return false;
            };
            f.blocks.iter().flatten().all(|b| match &b.terminator {
                Some(MirTerminator::Call { func, .. }) => CBackend::callee_name(func)
                    .is_some_and(|n| is_pure_call(n) || snapshot.contains(n)),
                _ => true,
            })
        });
        if free.len() == before {
            return free;
        }
    }
}

/// The hoisting plan of one function: which vector-typed local reads its
/// handle from which parameter.
#[derive(Debug, Default, Clone)]
pub(super) struct VecHoist {
    /// Vector local -> the parameter it was loaded from.
    pub(super) alias: HashMap<LocalId, LocalId>,
    /// Hoisted parameter -> its element type.
    pub(super) elems: HashMap<LocalId, MirType>,
}

/// For a vector parameter: (passed by pointer, element type).
pub(super) fn vec_param_kind(ty: &MirType) -> Option<(bool, &MirType)> {
    match ty {
        MirType::Vec(elem) => Some((false, elem)),
        MirType::Ptr(inner) => match &**inner {
            MirType::Vec(elem) => Some((true, elem)),
            _ => None,
        },
        _ => None,
    }
}

/// Element types whose runtime getter and setter are `*(T*)build_vec_get(..)`
/// with `T` the C type of the element, so a typed access at `i * elem_size`
/// reads and writes the same bytes.
pub(super) fn hoistable_elem(elem: &MirType) -> bool {
    match elem {
        MirType::Float(FloatSize::F64) => true,
        MirType::Int(..) => true,
        MirType::Struct(_) => true,
        _ => false,
    }
}

impl VecHoist {
    /// Plan `func`, or `None` when it may resize or replace a vector.
    pub(super) fn plan(
        func: &MirFunction,
        vector_free: &HashSet<String>,
        plain_structs: &HashSet<String>,
    ) -> Option<VecHoist> {
        let blocks = func.blocks.as_ref()?;
        let ty_of = |id: LocalId| func.locals.get(id.0 as usize).map(|l| &l.ty);
        let is_vec_param = |id: LocalId| {
            func.locals
                .get(id.0 as usize)
                .is_some_and(|l| l.is_param && vec_param_kind(&l.ty).is_some())
        };
        let hoists = |id: LocalId| {
            func.locals.get(id.0 as usize).is_some_and(|l| {
                l.is_param && vec_param_kind(&l.ty).is_some_and(|(_, e)| hoistable_elem(e))
            })
        };
        let mut alias: HashMap<LocalId, LocalId> = HashMap::new();
        let mut poisoned: Vec<LocalId> = Vec::new();
        for block in blocks {
            for stmt in &block.stmts {
                if store_may_replace_handle(&stmt.kind, func, plain_structs) {
                    return None;
                }
                match &stmt.kind {
                    MirStmtKind::Assign { dest, value } => {
                        if is_vec_param(*dest) {
                            return None;
                        }
                        if !matches!(ty_of(*dest), Some(MirType::Vec(_))) {
                            continue;
                        }
                        let src = match value {
                            MirRValue::Deref {
                                ptr: MirValue::Local(p),
                                ..
                            } if matches!(ty_of(*p), Some(MirType::Ptr(_))) => Some(*p),
                            MirRValue::Use(MirValue::Local(p))
                                if matches!(ty_of(*p), Some(MirType::Vec(_))) =>
                            {
                                Some(*p)
                            }
                            _ => None,
                        };
                        match src {
                            Some(p) if hoists(p) => {
                                if alias.insert(*dest, p).is_some_and(|old| old != p) {
                                    poisoned.push(*dest);
                                }
                            }
                            _ => poisoned.push(*dest),
                        }
                    }
                    _ => {}
                }
            }
            match &block.terminator {
                Some(MirTerminator::Call {
                    func: callee, dest, ..
                }) => {
                    let name = CBackend::callee_name(callee)?;
                    if !is_allowed_call(name, vector_free) {
                        return None;
                    }
                    if let Some(d) = dest {
                        if is_vec_param(*d) || matches!(ty_of(*d), Some(MirType::Vec(_))) {
                            return None;
                        }
                    }
                }
                Some(MirTerminator::Return(_)) | Some(MirTerminator::Goto { .. }) | None => {}
                Some(_) => {}
            }
        }
        for id in poisoned {
            alias.remove(&id);
        }
        if alias.is_empty() {
            None
        } else {
            let elems = alias
                .values()
                .filter_map(|p| {
                    let l = func.locals.get(p.0 as usize)?;
                    Some((*p, vec_param_kind(&l.ty)?.1.clone()))
                })
                .collect();
            Some(VecHoist { alias, elems })
        }
    }

    /// The parameter whose hoisted buffer serves `base`, when there is one.
    pub(super) fn param_for(&self, base: &MirValue) -> Option<LocalId> {
        match base {
            MirValue::Local(id) => self.alias.get(id).copied(),
            _ => None,
        }
    }

    pub(super) fn params(&self) -> Vec<LocalId> {
        let mut ps: Vec<LocalId> = self.alias.values().copied().collect();
        ps.sort_by_key(|p| p.0);
        ps.dedup();
        ps
    }
}
