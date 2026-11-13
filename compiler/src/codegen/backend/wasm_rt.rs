//! In-module runtime for the WASM backend: a bump heap and Vec handles.
//!
//! The heap global `$__heap` grows up from above the shadow stack and calls
//! `memory.grow` when it reaches the end of memory; a failed grow traps.
//! A Vec is an i32 handle to a 12-byte header `{ptr, len, cap}`, so copying
//! a Vec value shares the vector, as `BuildVecHandle` does in the C runtime.
//! Element widths follow the element type (u8 is one byte, u32 four, u64
//! eight). Out-of-bounds `get`/`set` and `pop` on an empty vector trap.

use std::collections::BTreeSet;

use super::super::wasm_bin;
use super::*;

/// (suffix, width, value type, load, store)
const ELEMS: [(&str, u32, &str, &str, &str); 11] = [
    ("i8", 1, "i32", "i32.load8_s", "i32.store8"),
    ("u8", 1, "i32", "i32.load8_u", "i32.store8"),
    ("i16", 2, "i32", "i32.load16_s", "i32.store16"),
    ("u16", 2, "i32", "i32.load16_u", "i32.store16"),
    ("i32", 4, "i32", "i32.load", "i32.store"),
    ("u32", 4, "i32", "i32.load", "i32.store"),
    ("i64", 8, "i64", "i64.load", "i64.store"),
    ("u64", 8, "i64", "i64.load", "i64.store"),
    ("isize", 8, "i64", "i64.load", "i64.store"),
    ("usize", 8, "i64", "i64.load", "i64.store"),
    ("f64", 8, "f64", "f64.load", "f64.store"),
];

pub(super) struct RtFunc {
    pub(super) name: String,
    params: Vec<&'static str>,
    ret: Option<&'static str>,
    locals: Vec<&'static str>,
    body: String,
}

#[derive(Default)]
pub(super) struct RtState {
    pub(super) funcs: Vec<RtFunc>,
}

fn mir_of(valtype: &str) -> MirType {
    match valtype {
        "i64" => MirType::i64(),
        "f64" => MirType::f64(),
        _ => MirType::i32(),
    }
}

/// Runtime helper that implements a MIR call name, if any.
pub(super) fn runtime_target(name: &str) -> Option<String> {
    match name {
        "Vec_new" | "vec_new" => return Some("__vec_new".into()),
        "build_hvec_len" => return Some("__vec_len".into()),
        _ => {}
    }
    let rest = name.strip_prefix("build_hvec_")?;
    let (op, sfx) = rest.split_once('_')?;
    let known = ELEMS.iter().any(|e| e.0 == sfx);
    (known && matches!(op, "push" | "get" | "set" | "pop")).then(|| format!("__vec_{op}_{sfx}"))
}

/// Runtime element suffix for a Vec element type (matches the lowering's names).
pub(super) fn vec_elem_suffix(ty: &MirType) -> Option<&'static str> {
    Some(match ty {
        MirType::Int(IntSize::I8, s) => {
            if *s {
                "i8"
            } else {
                "u8"
            }
        }
        MirType::Int(IntSize::I16, s) => {
            if *s {
                "i16"
            } else {
                "u16"
            }
        }
        MirType::Int(IntSize::I32, s) => {
            if *s {
                "i32"
            } else {
                "u32"
            }
        }
        MirType::Int(IntSize::I64, s) => {
            if *s {
                "i64"
            } else {
                "u64"
            }
        }
        MirType::Int(IntSize::ISize, s) => {
            if *s {
                "isize"
            } else {
                "usize"
            }
        }
        MirType::Bool => "i32",
        MirType::Float(FloatSize::F64) => "f64",
        _ => return None,
    })
}

const ALLOC: &str =
    "global.get $__heap\nlocal.tee $t1\nlocal.get $p0\ni32.add\ni32.const 7\ni32.add\n\
i32.const -8\ni32.and\nlocal.tee $t0\nglobal.set $__heap\nblock $done\nloop $grow\nlocal.get $t0\n\
memory.size\ni32.const 16\ni32.shl\ni32.le_u\nbr_if $done\ni32.const 1\nmemory.grow\ni32.const -1\n\
i32.eq\nif\nunreachable\nend\nbr $grow\nend\nend\nlocal.get $t1\n";

const LEN_FIELD: &str = "local.get $p0\ni32.const 4\ni32.add\n";

fn bounds_check() -> String {
    format!(
        "local.get $p1\n{LEN_FIELD}i32.load\ni64.extend_i32_u\ni64.ge_u\nif\nunreachable\nend\n"
    )
}

fn elem_addr(index: &str, w: u32) -> String {
    format!("local.get $p0\ni32.load\n{index}i32.const {w}\ni32.mul\ni32.add\n")
}

fn build_func(name: &str) -> Option<RtFunc> {
    let f = |params: Vec<&'static str>, ret, locals: Vec<&'static str>, body: String| RtFunc {
        name: name.to_string(),
        params,
        ret,
        locals,
        body,
    };
    match name {
        "__alloc" => {
            return Some(f(
                vec!["i32"],
                Some("i32"),
                vec!["i32", "i32"],
                ALLOC.into(),
            ))
        }
        "__vec_new" => {
            return Some(f(
                vec![],
                Some("i32"),
                vec![],
                "i32.const 12\ncall $__alloc\n".into(),
            ))
        }
        "__vec_len" => {
            let body = format!("{LEN_FIELD}i32.load\ni64.extend_i32_u\n");
            return Some(f(vec!["i32"], Some("i64"), vec![], body));
        }
        _ => {}
    }
    let rest = name.strip_prefix("__vec_")?;
    let (op, sfx) = rest.split_once('_')?;
    let &(_, w, vt, load, store) = ELEMS.iter().find(|e| e.0 == sfx)?;
    let wrap_idx = "local.get $p1\ni32.wrap_i64\n";
    Some(match op {
        "push" => {
            let body = format!(
                "{LEN_FIELD}i32.load\nlocal.set $t0\nlocal.get $t0\nlocal.get $p0\ni32.const 8\ni32.add\n\
i32.load\ni32.eq\nif\nlocal.get $t0\ni32.const 1\ni32.shl\nlocal.tee $t1\ni32.const 4\nlocal.get $t1\n\
i32.const 4\ni32.gt_u\nselect\nlocal.set $t1\nlocal.get $p0\ni32.const 8\ni32.add\nlocal.get $t1\n\
i32.store\nlocal.get $t1\ni32.const {w}\ni32.mul\ncall $__alloc\nlocal.set $t2\nlocal.get $t2\n\
local.get $p0\ni32.load\nlocal.get $t0\ni32.const {w}\ni32.mul\nmemory.copy\nlocal.get $p0\n\
local.get $t2\ni32.store\nend\n{}local.get $p1\n{store}\n{LEN_FIELD}local.get $t0\ni32.const 1\n\
i32.add\ni32.store\n",
                elem_addr("local.get $t0\n", w)
            );
            f(vec!["i32", vt], None, vec!["i32", "i32", "i32"], body)
        }
        "get" => {
            let body = format!("{}{}{load}\n", bounds_check(), elem_addr(wrap_idx, w));
            f(vec!["i32", "i64"], Some(vt), vec![], body)
        }
        "set" => {
            let body = format!(
                "{}{}local.get $p2\n{store}\n",
                bounds_check(),
                elem_addr(wrap_idx, w)
            );
            f(vec!["i32", "i64", vt], None, vec![], body)
        }
        "pop" => {
            let body = format!(
                "{LEN_FIELD}i32.load\ni32.eqz\nif\nunreachable\nend\n{LEN_FIELD}{LEN_FIELD}i32.load\n\
i32.const 1\ni32.sub\nlocal.tee $t0\ni32.store\n{}{load}\n",
                elem_addr("local.get $t0\n", w)
            );
            f(vec!["i32"], Some(vt), vec!["i32"], body)
        }
        _ => return None,
    })
}

impl WasmBackend {
    /// Collect the runtime helpers this module calls; returns their result types.
    pub(super) fn plan_runtime(&mut self, mir: &MirModule) -> Vec<(String, MirType)> {
        let mut names = BTreeSet::new();
        for func in &mir.functions {
            for block in func.blocks.iter().flatten() {
                if let Some(MirTerminator::Call {
                    func: MirValue::Function(n) | MirValue::Global(n),
                    ..
                }) = &block.terminator
                {
                    if let Some(t) = runtime_target(n) {
                        names.insert(t);
                    }
                }
            }
        }
        for func in &mir.functions {
            for stmt in func.blocks.iter().flatten().flat_map(|b| b.stmts.iter()) {
                let (op, base, elem) = match &stmt.kind {
                    MirStmtKind::Assign {
                        value: MirRValue::IndexAccess { base, elem_ty, .. },
                        ..
                    } => ("get", base, elem_ty),
                    MirStmtKind::IndexStore { base, elem_ty, .. } => ("set", base, elem_ty),
                    _ => continue,
                };
                let is_vec = matches!(base, MirValue::Local(id) if func.locals.iter().any(|l| l.id == *id && matches!(l.ty, MirType::Vec(_))));
                if let (true, Some(sfx)) = (is_vec, vec_elem_suffix(elem)) {
                    names.insert(format!("__vec_{op}_{sfx}"));
                }
            }
        }
        self.rt = RtState::default();
        if names.is_empty() {
            return Vec::new();
        }
        self.rt.funcs.push(build_func("__alloc").expect("alloc"));
        for n in names {
            if let Some(rf) = build_func(&n) {
                self.rt.funcs.push(rf);
            }
        }
        self.rt
            .funcs
            .iter()
            .map(|rf| (rf.name.clone(), rf.ret.map(mir_of).unwrap_or(MirType::Void)))
            .collect()
    }

    /// `v[i]` read or write on a Vec handle through the runtime helpers.
    pub(super) fn gen_vec_access(
        &mut self,
        op: &str,
        base: &MirValue,
        index: &MirValue,
        value: Option<&MirRValue>,
        elem_ty: &MirType,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        let Some(sfx) = vec_elem_suffix(elem_ty) else {
            return Err(CodegenError::Unsupported(format!(
                "Vec of {:?} in WASM",
                elem_ty
            )));
        };
        self.gen_value(base, func)?;
        self.gen_value(index, func)?;
        if self.emit_type(&self.infer_value_type(index, func)?) == "i32" {
            self.emit_line("i64.extend_i32_u");
        }
        if let Some(v) = value {
            self.gen_rvalue(v, func)?;
        }
        self.emit_line(&format!("call $__vec_{op}_{sfx}"));
        Ok(())
    }

    /// Push call arguments, converting to a runtime helper's parameter types.
    pub(super) fn gen_call_args(
        &mut self,
        callee: &str,
        args: &[MirValue],
        func: &MirFunction,
    ) -> CodegenResult<()> {
        let params = self
            .rt
            .funcs
            .iter()
            .find(|rf| rf.name == callee)
            .map(|rf| rf.params.clone());
        for (i, arg) in args.iter().enumerate() {
            self.gen_value(arg, func)?;
            let want = params.as_ref().and_then(|p| p.get(i)).copied();
            let have = self.emit_type(&self.infer_value_type(arg, func)?);
            match (have, want) {
                ("i32", Some("i64")) => self.emit_line("i64.extend_i32_u"),
                ("i64", Some("i32")) => self.emit_line("i32.wrap_i64"),
                _ => {}
            }
        }
        Ok(())
    }

    /// Runtime helpers as WAT text.
    pub(super) fn runtime_wat(&mut self) {
        let mut text = String::new();
        for rf in &self.rt.funcs {
            text.push_str(&format!("(func ${}", rf.name));
            for (i, p) in rf.params.iter().enumerate() {
                text.push_str(&format!(" (param $p{} {})", i, p));
            }
            if let Some(r) = rf.ret {
                text.push_str(&format!(" (result {})", r));
            }
            text.push('\n');
            for (i, l) in rf.locals.iter().enumerate() {
                text.push_str(&format!("  (local $t{} {})\n", i, l));
            }
            for line in rf.body.lines() {
                text.push_str(&format!("  {}\n", line));
            }
            text.push_str(")\n");
        }
        for line in text.lines() {
            self.emit_line(line);
        }
    }

    /// Runtime helpers as (type, body) pairs for the binary module.
    pub(super) fn runtime_binary(
        &self,
        func_index: &HashMap<String, u32>,
        global_index: &HashMap<String, u32>,
    ) -> CodegenResult<Vec<(wasm_bin::FuncType, Vec<u8>)>> {
        let mut out = Vec::new();
        for rf in &self.rt.funcs {
            let vt = |t: &&str| wasm_bin::valtype(t).map_err(CodegenError::Internal);
            let params = rf
                .params
                .iter()
                .map(vt)
                .collect::<CodegenResult<Vec<u8>>>()?;
            let results = rf.ret.iter().map(vt).collect::<CodegenResult<Vec<u8>>>()?;
            let locals = rf
                .locals
                .iter()
                .map(vt)
                .collect::<CodegenResult<Vec<u8>>>()?;
            let mut idx = HashMap::new();
            for i in 0..rf.params.len() {
                idx.insert(format!("$p{}", i), i as u32);
            }
            for i in 0..rf.locals.len() {
                idx.insert(format!("$t{}", i), (rf.params.len() + i) as u32);
            }
            let body =
                wasm_bin::assemble_body(&rf.body, &locals, &idx, func_index, global_index)
                    .map_err(|e| CodegenError::Internal(format!("runtime `{}`: {}", rf.name, e)))?;
            out.push(((params, results), body));
        }
        Ok(out)
    }
}
