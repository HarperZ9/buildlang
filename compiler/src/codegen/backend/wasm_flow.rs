//! Control flow, printing and module assembly for the WASM backend.
//!
//! Every function body is a dispatch loop: one `loop` holding one `block`
//! per MIR basic block, entered through a `br_table` on the `$__bb` local.
//! A jump sets `$__bb` and branches back to the loop, so any MIR CFG is
//! correct, including loops whose header is not block 0. Structured
//! reconstruction is a later optimisation.

use std::collections::HashMap;

use super::super::wasm_bin::{self, ModuleSpec};
use super::*;

/// Base address of the data pool in linear memory (0 stays the null pointer).
const POOL_BASE: u32 = 16;

/// Host imports for output: (field, params). No WASI is needed.
const PRINT_IMPORTS: [(&str, &[&str]); 5] = [
    ("print_i64", &["i64"]),
    ("print_u64", &["i64"]),
    ("print_f64", &["f64"]),
    ("print_bool", &["i32"]),
    ("print_str", &["i32", "i32"]),
];

#[derive(Default)]
pub(super) struct FlowState {
    local_decls: Vec<(String, &'static str)>,
    block_pos: HashMap<BlockId, usize>,
    fn_rets: HashMap<String, MirType>,
    strings: Vec<String>,
    pool: Vec<u8>,
    str_offsets: Vec<u32>,
    string_locals: HashMap<LocalId, usize>,
}

fn unsupported<T>(msg: String) -> CodegenResult<T> {
    Err(CodegenError::Unsupported(msg))
}

impl WasmBackend {
    fn prepare_module(&mut self, mir: &MirModule) {
        self.fl = FlowState::default();
        for f in &mir.functions {
            if !f.is_declaration() {
                self.fl
                    .fn_rets
                    .insert(f.name.to_string(), f.sig.ret.clone());
            }
        }
        for s in &mir.strings {
            self.fl.strings.push(s.to_string());
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            let off = self.intern(&bytes);
            self.fl.str_offsets.push(off);
        }
    }

    fn intern(&mut self, bytes: &[u8]) -> u32 {
        let off = POOL_BASE + self.fl.pool.len() as u32;
        self.fl.pool.extend_from_slice(bytes);
        off
    }

    pub(super) fn str_offset(&self, idx: usize) -> u32 {
        self.fl.str_offsets.get(idx).copied().unwrap_or(0)
    }

    /// Declare locals: params are `$argN`, others get id-unique names.
    pub(super) fn gen_locals(&mut self, func: &MirFunction) -> CodegenResult<()> {
        self.fl.local_decls.clear();
        let mut param_no = 0;
        for local in &func.locals {
            if local.is_param {
                self.local_names
                    .insert(local.id, format!("$arg{}", param_no));
                param_no += 1;
                continue;
            }
            let name = match &local.name {
                Some(n) => format!("$l{}_{}", local.id.0, n),
                None => format!("$l{}", local.id.0),
            };
            self.local_names.insert(local.id, name.clone());
            self.fl.local_decls.push((name, self.emit_type(&local.ty)));
        }
        self.plan_frame(func)?;
        self.fl.local_decls.push(("$__fp".into(), "i32"));
        self.fl.local_decls.push(("$__bb".into(), "i32"));
        self.fl.local_decls.push(("$__switch_val".into(), "i64"));
        for (name, ty) in self.fl.local_decls.clone() {
            self.emit_line(&format!("(local {} {})", name, ty));
        }
        Ok(())
    }

    /// Dispatch-loop body: correct for any MIR control-flow graph.
    pub(super) fn gen_function_body(&mut self, func: &MirFunction) -> CodegenResult<()> {
        let blocks = match &func.blocks {
            Some(b) if !b.is_empty() => b,
            _ => {
                if func.sig.ret != MirType::Void {
                    self.gen_default_value(&func.sig.ret);
                }
                return Ok(());
            }
        };
        self.fl.block_pos = blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
        self.fl.string_locals.clear();
        self.emit_prologue()?;
        self.emit_line("loop $dispatch");
        for i in (0..blocks.len()).rev() {
            self.emit_line(&format!("block $bb{}", i));
        }
        self.emit_line("local.get $__bb");
        let targets: Vec<String> = (0..blocks.len()).map(|i| format!("$bb{}", i)).collect();
        self.emit_line(&format!("br_table {} $bb0", targets.join(" ")));
        for block in blocks {
            self.emit_line("end");
            for stmt in &block.stmts {
                self.gen_stmt(stmt, func)?;
            }
            match &block.terminator {
                Some(term) => self.gen_terminator(term, func)?,
                None => self.emit_line("unreachable"),
            }
        }
        self.emit_line("end ;; $dispatch");
        self.emit_line("unreachable");
        Ok(())
    }

    fn emit_br_to(&mut self, target: BlockId) -> CodegenResult<()> {
        let pos = *self.fl.block_pos.get(&target).ok_or_else(|| {
            CodegenError::Internal(format!("branch to unknown block {:?}", target))
        })?;
        self.emit_line(&format!("i32.const {}", pos));
        self.emit_line("local.set $__bb");
        self.emit_line("br $dispatch");
        Ok(())
    }

    /// Binary operation with integer constants coerced to the other operand's type.
    pub(super) fn gen_binop(
        &mut self,
        op: BinOp,
        left: &MirValue,
        right: &MirValue,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        let left_const = matches!(left, MirValue::Const(_));
        let ty = if left_const && !matches!(right, MirValue::Const(_)) {
            self.infer_value_type(right, func)?
        } else {
            self.infer_value_type(left, func)?
        };
        for v in [left, right] {
            match (v, &ty) {
                (MirValue::Const(MirConst::Int(n, _)), MirType::Int(..)) => {
                    self.gen_const(&MirConst::Int(*n, ty.clone()))?
                }
                (MirValue::Const(MirConst::Uint(n, _)), MirType::Int(..)) => {
                    self.gen_const(&MirConst::Uint(*n, ty.clone()))?
                }
                _ => self.gen_value(v, func)?,
            }
        }
        let instr = self.wasm_binop(op, &ty);
        self.emit_line(&instr);
        if !matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        ) {
            self.narrow_int(&ty);
        }
        Ok(())
    }

    fn local_ty(func: &MirFunction, id: LocalId) -> MirType {
        func.locals
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.ty.clone())
            .unwrap_or(MirType::Void)
    }

    fn set_dest(&mut self, dest: LocalId) -> CodegenResult<()> {
        let name = self
            .local_names
            .get(&dest)
            .cloned()
            .ok_or_else(|| CodegenError::Internal(format!("Unknown local: {:?}", dest)))?;
        self.emit_line(&format!("local.set {}", name));
        Ok(())
    }

    /// `println`/`print` with a literal format: pieces and `{}` holes.
    fn emit_print(
        &mut self,
        newline: bool,
        args: &[MirValue],
        func: &MirFunction,
    ) -> CodegenResult<()> {
        let fmt = match args.first() {
            None => String::new(),
            Some(MirValue::Local(id)) if self.fl.string_locals.contains_key(id) => {
                self.fl.strings[self.fl.string_locals[id]].clone()
            }
            Some(MirValue::Const(MirConst::Str(i))) => self.fl.strings[*i as usize].clone(),
            Some(_) => return unsupported("print needs a literal format string".into()),
        };
        let mut pieces: Vec<&str> = fmt.split("{}").collect();
        if pieces.iter().any(|p| p.contains('{') || p.contains('}')) {
            return unsupported(format!(
                "format `{fmt}`: only plain `{{}}` holes are supported"
            ));
        }
        if pieces.len() != args.len() {
            return unsupported(format!("format `{fmt}` does not match its argument count"));
        }
        let last = if newline {
            format!("{}\n", pieces.pop().unwrap_or(""))
        } else {
            pieces.pop().unwrap_or("").to_string()
        };
        for (i, piece) in pieces.iter().enumerate() {
            self.emit_print_str(piece.as_bytes());
            let arg = &args[i + 1];
            let ty = self.infer_value_type(arg, func)?;
            self.gen_value(arg, func)?;
            let call = match &ty {
                MirType::Bool => "print_bool",
                MirType::Int(_, signed) => {
                    if self.emit_type(&ty) == "i32" {
                        self.emit_line(if *signed {
                            "i64.extend_i32_s"
                        } else {
                            "i64.extend_i32_u"
                        });
                    }
                    if *signed {
                        "print_i64"
                    } else {
                        "print_u64"
                    }
                }
                MirType::Float(FloatSize::F64) => "print_f64",
                other => return unsupported(format!("printing a {:?} value in WASM", other)),
            };
            self.emit_line(&format!("call $__env_{}", call));
        }
        self.emit_print_str(last.as_bytes());
        Ok(())
    }

    fn emit_print_str(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let off = self.intern(bytes);
        self.emit_line(&format!("i32.const {}", off));
        self.emit_line(&format!("i32.const {}", bytes.len()));
        self.emit_line("call $__env_print_str");
    }

    fn gen_call(
        &mut self,
        callee: &MirValue,
        args: &[MirValue],
        dest: Option<LocalId>,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        let name = match callee {
            MirValue::Function(n) | MirValue::Global(n) => n.to_string(),
            _ => return unsupported("indirect calls in the WASM backend".into()),
        };
        let ret = match name.as_str() {
            "build_string_new" => {
                if let (Some(d), Some(MirValue::Const(MirConst::Str(i)))) = (dest, args.first()) {
                    self.fl.string_locals.insert(d, *i as usize);
                }
                MirType::Void
            }
            "println" | "print" => {
                self.emit_print(name == "println", args, func)?;
                MirType::Void
            }
            _ => {
                let ret = match self.fl.fn_rets.get(&name) {
                    Some(r) => r.clone(),
                    None => return unsupported(format!("no WASM runtime for `{}`", name)),
                };
                for arg in args {
                    self.gen_value(arg, func)?;
                }
                self.emit_line(&format!("call ${}", name));
                ret
            }
        };
        match (dest, ret == MirType::Void) {
            (Some(d), true) => {
                self.gen_default_value(&Self::local_ty(func, d));
                self.set_dest(d)?;
            }
            (Some(d), false) => self.set_dest(d)?,
            (None, false) => self.emit_line("drop"),
            (None, true) => {}
        }
        Ok(())
    }

    pub(super) fn gen_terminator(
        &mut self,
        term: &MirTerminator,
        func: &MirFunction,
    ) -> CodegenResult<()> {
        match term {
            MirTerminator::Goto(t) | MirTerminator::Drop { target: t, .. } => {
                self.emit_br_to(*t)?
            }
            MirTerminator::If {
                cond,
                then_block,
                else_block,
            } => {
                self.gen_value(cond, func)?;
                self.emit_line("if");
                self.emit_br_to(*then_block)?;
                self.emit_line("else");
                self.emit_br_to(*else_block)?;
                self.emit_line("end");
                self.emit_line("unreachable");
            }
            MirTerminator::Switch {
                value,
                targets,
                default,
            } => {
                let ty = self.infer_value_type(value, func)?;
                self.gen_value(value, func)?;
                if self.emit_type(&ty) == "i32" {
                    self.emit_line("i64.extend_i32_s");
                }
                self.emit_line("local.set $__switch_val");
                for (c, target) in targets {
                    let v = match c {
                        MirConst::Int(v, _) => *v as i128,
                        MirConst::Uint(v, _) => *v as i128,
                        MirConst::Bool(b) => *b as i128,
                        other => return unsupported(format!("switch on {:?}", other)),
                    };
                    self.emit_line("local.get $__switch_val");
                    self.emit_line(&format!("i64.const {}", v));
                    self.emit_line("i64.eq");
                    self.emit_line("if");
                    self.emit_br_to(*target)?;
                    self.emit_line("end");
                }
                self.emit_br_to(*default)?;
            }
            MirTerminator::Call {
                func: callee,
                args,
                dest,
                target,
                ..
            } => {
                self.gen_call(callee, args, *dest, func)?;
                match target {
                    Some(t) => self.emit_br_to(*t)?,
                    None => self.emit_line("unreachable"),
                }
            }
            MirTerminator::Return(value) => {
                if let Some(v) = value {
                    if func.sig.ret != MirType::Void {
                        self.gen_value(v, func)?;
                    }
                } else if func.sig.ret != MirType::Void {
                    self.gen_default_value(&func.sig.ret);
                }
                self.emit_epilogue();
                self.emit_line("return");
            }
            MirTerminator::Assert {
                cond,
                expected,
                target,
                ..
            } => {
                self.gen_value(cond, func)?;
                if *expected {
                    self.emit_line("i32.eqz");
                }
                self.emit_line("if");
                self.emit_line("unreachable");
                self.emit_line("end");
                self.emit_br_to(*target)?;
            }
            MirTerminator::Unreachable | MirTerminator::Resume => self.emit_line("unreachable"),
            MirTerminator::Abort => {
                if self.wasi {
                    self.emit_line("i32.const 1");
                    self.emit_line("call $__wasi_proc_exit");
                }
                self.emit_line("unreachable");
            }
        }
        Ok(())
    }

    /// Memory-shaped MIR has no sound lowering yet; refuse it in binary output.
    fn check_binary_subset(func: &MirFunction) -> CodegenResult<()> {
        for block in func.blocks.iter().flatten() {
            for stmt in &block.stmts {
                let bad = !Self::stmt_supported(stmt);
                if bad {
                    return unsupported(format!(
                        "`{}`: memory operations (structs, arrays, references, stores) are not yet lowered to binary WASM: {:?}",
                        func.name, stmt.kind
                    ));
                }
            }
        }
        Ok(())
    }

    fn gen_print_imports_wat(&mut self) {
        for (field, params) in PRINT_IMPORTS {
            let p: Vec<String> = params.iter().map(|t| format!("(param {})", t)).collect();
            self.emit_line(&format!(
                "(import \"env\" \"{0}\" (func $__env_{0} {1}))",
                field,
                p.join(" ")
            ));
        }
    }

    fn gen_data_section(&mut self) {
        if self.fl.pool.is_empty() {
            return;
        }
        let escaped: String = self
            .fl
            .pool
            .iter()
            .map(|b| format!("\\{:02x}", b))
            .collect();
        self.emit_line(&format!("(data (i32.const {}) \"{}\")", POOL_BASE, escaped));
    }

    /// WebAssembly text, from the same instruction selection as the binary.
    pub fn generate_wat(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        self.output.clear();
        self.indent = 0;
        self.prepare_module(mir);
        self.gen_module_header(mir);
        self.gen_print_imports_wat();
        if self.wasi {
            self.gen_wasi_imports();
        }
        self.gen_memory();
        self.emit_line(&format!(
            "(global $__sp (mut i32) (i32.const {}))",
            self.memory_pages.max(1) * 65536
        ));
        self.gen_types(mir);
        self.gen_globals(mir);
        self.gen_table(mir);
        if self.wasi {
            self.gen_wasi_helpers();
        }
        for func in &mir.functions {
            self.gen_function(func)?;
        }
        self.gen_data_section();
        if self.wasi {
            self.gen_wasi_start(mir);
        }
        self.indent -= 1;
        self.emit_line(")");
        Ok(GeneratedCode::new(
            OutputFormat::Wat,
            self.output.as_bytes().to_vec(),
        ))
    }

    fn valtypes(&self, tys: &[MirType]) -> CodegenResult<Vec<u8>> {
        tys.iter()
            .map(|t| wasm_bin::valtype(self.emit_type(t)).map_err(CodegenError::Unsupported))
            .collect()
    }

    /// Binary WebAssembly module (imports `env.print_*` for output).
    pub fn generate_binary(&mut self, mir: &MirModule) -> CodegenResult<Vec<u8>> {
        self.prepare_module(mir);
        let mut spec = ModuleSpec {
            memory_pages: self.memory_pages.max(1),
            ..Default::default()
        };
        spec.globals
            .push((0x7f, true, spec.memory_pages as i64 * 65536));
        let global_index: HashMap<String, u32> = [("$__sp".to_string(), 0)].into_iter().collect();
        let mut func_index: HashMap<String, u32> = HashMap::new();
        for (field, params) in PRINT_IMPORTS {
            let p = params
                .iter()
                .map(|t| wasm_bin::valtype(t))
                .collect::<Result<Vec<_>, _>>();
            spec.types
                .push((p.map_err(CodegenError::Internal)?, vec![]));
            let ty = spec.types.len() as u32 - 1;
            func_index.insert(format!("__env_{}", field), spec.imports.len() as u32);
            spec.imports.push(("env".into(), field.into(), ty));
        }
        let defined: Vec<&MirFunction> = mir
            .functions
            .iter()
            .filter(|f| !f.is_declaration())
            .collect();
        for (i, f) in defined.iter().enumerate() {
            func_index.insert(f.name.to_string(), (spec.imports.len() + i) as u32);
        }
        for f in defined {
            Self::check_binary_subset(f)?;
            let params = self.valtypes(&f.sig.params)?;
            let results = if f.sig.ret == MirType::Void {
                vec![]
            } else {
                self.valtypes(&[f.sig.ret.clone()])?
            };
            spec.types.push((params, results));
            spec.funcs.push(spec.types.len() as u32 - 1);
            if f.is_public || &*f.name == "main" {
                spec.exports
                    .push((f.name.to_string(), func_index[&*f.name]));
            }
            self.reset_function_state();
            let saved = std::mem::take(&mut self.output);
            self.indent = 0;
            self.gen_locals(f)?;
            self.output.clear();
            self.gen_function_body(f)?;
            let text = std::mem::replace(&mut self.output, saved);
            let mut local_index: HashMap<String, u32> = HashMap::new();
            for i in 0..f.sig.params.len() {
                local_index.insert(format!("$arg{}", i), i as u32);
            }
            let mut locals = Vec::new();
            for (k, (name, ty)) in self.fl.local_decls.iter().enumerate() {
                local_index.insert(name.clone(), (f.sig.params.len() + k) as u32);
                locals.push(wasm_bin::valtype(ty).map_err(CodegenError::Unsupported)?);
            }
            let body =
                wasm_bin::assemble_body(&text, &locals, &local_index, &func_index, &global_index)
                    .map_err(|e| CodegenError::Unsupported(format!("`{}`: {}", f.name, e)))?;
            spec.bodies.push(body);
        }
        if !self.fl.pool.is_empty() {
            spec.data.push((POOL_BASE, self.fl.pool.clone()));
        }
        Ok(wasm_bin::encode_module(&spec))
    }
}

impl Backend for WasmBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        let bytes = self.generate_binary(mir)?;
        Ok(GeneratedCode::new(OutputFormat::Wasm, bytes))
    }

    fn target(&self) -> Target {
        Target::Wasm
    }
}
