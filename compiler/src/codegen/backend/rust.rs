// ===============================================================================
// QUANTALANG CODE GENERATOR - RUST BACKEND
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Rust source backend.
//!
//! This backend is a conservative bridge from QuantaLang MIR to Rust source. It
//! is intentionally subset-based: MIR constructs that are not safely projected
//! yet return `CodegenError::Unsupported` instead of emitting plausible but
//! incorrect Rust.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Backend, CodegenError, CodegenResult, Target};
use crate::codegen::ir::*;
use crate::codegen::{GeneratedCode, OutputFormat};

/// Backend that emits Rust source from MIR.
pub struct RustBackend {
    output: String,
    indent: usize,
    strings: Vec<Arc<str>>,
    struct_fields: HashMap<String, Vec<String>>,
}

impl RustBackend {
    /// Create a new Rust backend.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            strings: Vec::new(),
            struct_fields: HashMap::new(),
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    fn writeln(&mut self, line: &str) {
        self.write_indent();
        self.output.push_str(line);
        self.output.push('\n');
    }

    fn collect_struct_fields(&mut self, types: &[MirTypeDef]) {
        self.struct_fields.clear();
        for ty in types {
            if let TypeDefKind::Struct { fields, .. } = &ty.kind {
                let names = fields
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| {
                        name.as_ref()
                            .map(|n| Self::rust_ident(n))
                            .unwrap_or_else(|| format!("field{}", i))
                    })
                    .collect();
                self.struct_fields.insert(ty.name.to_string(), names);
            }
        }
    }

    fn emit_runtime(&mut self) {
        self.writeln("fn quanta_string_new<S: AsRef<str>>(s: S) -> String {");
        self.indent += 1;
        self.writeln("s.as_ref().to_string()");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        self.writeln("fn quanta_format(fmt: &str, args: &[String]) -> String {");
        self.indent += 1;
        self.writeln("let mut out = String::new();");
        self.writeln("let mut args_iter = args.iter();");
        self.writeln("let mut chars = fmt.chars().peekable();");
        self.writeln("while let Some(ch) = chars.next() {");
        self.indent += 1;
        self.writeln("if ch != '%' {");
        self.indent += 1;
        self.writeln("out.push(ch);");
        self.writeln("continue;");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("match chars.next() {");
        self.indent += 1;
        self.writeln("Some('%') => out.push('%'),");
        self.writeln(
            "Some('d' | 'i' | 'u' | 's' | 'f' | 'g') => out.push_str(args_iter.next().map(String::as_str).unwrap_or(\"\")),",
        );
        self.writeln("Some('l') => {");
        self.indent += 1;
        self.writeln("if chars.peek() == Some(&'l') { chars.next(); }");
        self.writeln("match chars.next() {");
        self.indent += 1;
        self.writeln(
            "Some('d' | 'i' | 'u') => out.push_str(args_iter.next().map(String::as_str).unwrap_or(\"\")),",
        );
        self.writeln("Some(other) => { out.push('%'); out.push('l'); out.push(other); }");
        self.writeln("None => { out.push('%'); out.push('l'); }");
        self.indent -= 1;
        self.writeln("}");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("Some(other) => { out.push('%'); out.push(other); }");
        self.writeln("None => out.push('%'),");
        self.indent -= 1;
        self.writeln("}");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("out");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        self.writeln("fn quanta_printf<S: AsRef<str>>(fmt: S, args: &[String]) -> i32 {");
        self.indent += 1;
        self.writeln("print!(\"{}\", quanta_format(fmt.as_ref(), args));");
        self.writeln("0");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        self.writeln("fn quanta_println<S: AsRef<str>>(fmt: S, args: &[String]) -> i32 {");
        self.indent += 1;
        self.writeln("println!(\"{}\", quanta_format(fmt.as_ref(), args));");
        self.writeln("0");
        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
    }

    fn emit_type_definitions(&mut self, types: &[MirTypeDef]) -> CodegenResult<()> {
        for ty in types {
            match &ty.kind {
                TypeDefKind::Struct { fields, .. } => {
                    self.writeln("#[derive(Clone, Debug, Default)]");
                    self.writeln(&format!("struct {} {{", Self::rust_type_name(&ty.name)));
                    self.indent += 1;
                    for (i, (name, field_ty)) in fields.iter().enumerate() {
                        let field_name = name
                            .as_ref()
                            .map(|n| Self::rust_ident(n))
                            .unwrap_or_else(|| format!("field{}", i));
                        self.writeln(&format!("{}: {},", field_name, self.type_to_rust(field_ty)));
                    }
                    self.indent -= 1;
                    self.writeln("}");
                    self.writeln("");
                }
                TypeDefKind::Union { .. } => {
                    return Err(CodegenError::Unsupported(format!(
                        "Rust backend does not yet lower union type '{}'",
                        ty.name
                    )));
                }
                TypeDefKind::Enum { .. } => {
                    return Err(CodegenError::Unsupported(format!(
                        "Rust backend does not yet lower enum type '{}'",
                        ty.name
                    )));
                }
            }
        }
        Ok(())
    }

    fn emit_string_table(&mut self) {
        if self.strings.is_empty() {
            return;
        }
        for (i, s) in self.strings.clone().iter().enumerate() {
            self.writeln(&format!("const __STR{}: &str = {:?};", i, s.as_ref()));
        }
        self.writeln("");
    }

    fn generate_function(&mut self, func: &MirFunction) -> CodegenResult<()> {
        if func.is_declaration() {
            return Ok(());
        }

        let is_main = func.name.as_ref() == "main";
        let fn_name = Self::rust_ident(&func.name);
        let params = if is_main {
            Vec::new()
        } else {
            func.locals
                .iter()
                .filter(|local| local.is_param)
                .map(|local| {
                    format!(
                        "{}: {}",
                        self.local_name(local.id, &func.locals),
                        self.type_to_rust(&local.ty)
                    )
                })
                .collect::<Vec<_>>()
        };

        let ret = if is_main || matches!(func.sig.ret, MirType::Void | MirType::Never) {
            String::new()
        } else {
            format!(" -> {}", self.type_to_rust(&func.sig.ret))
        };

        self.writeln(&format!("fn {}({}){} {{", fn_name, params.join(", "), ret));
        self.indent += 1;

        for local in &func.locals {
            if local.is_param || matches!(local.ty, MirType::Void) {
                continue;
            }
            self.writeln(&format!(
                "let mut {}: {} = {};",
                self.local_name(local.id, &func.locals),
                self.type_to_rust(&local.ty),
                self.default_value(&local.ty)
            ));
        }

        if let Some(blocks) = &func.blocks {
            self.writeln("let mut __bb: u32 = 0;");
            self.writeln("loop {");
            self.indent += 1;
            self.writeln("match __bb {");
            self.indent += 1;
            for block in blocks {
                self.writeln(&format!("{} => {{", block.id.0));
                self.indent += 1;
                for stmt in &block.stmts {
                    self.generate_statement(stmt, &func.locals)?;
                }
                if let Some(term) = &block.terminator {
                    self.generate_terminator(term, &func.locals, is_main)?;
                } else {
                    self.generate_fallthrough_return(&func.sig.ret, is_main);
                }
                self.indent -= 1;
                self.writeln("}");
            }
            self.writeln("_ => unreachable!(),");
            self.indent -= 1;
            self.writeln("}");
            self.indent -= 1;
            self.writeln("}");
        } else {
            self.generate_fallthrough_return(&func.sig.ret, is_main);
        }

        self.indent -= 1;
        self.writeln("}");
        self.writeln("");
        Ok(())
    }

    fn generate_fallthrough_return(&mut self, ret_ty: &MirType, is_main: bool) {
        if is_main || matches!(ret_ty, MirType::Void | MirType::Never) {
            self.writeln("return;");
        } else {
            self.writeln(&format!("return {};", self.default_value(ret_ty)));
        }
    }

    fn generate_statement(&mut self, stmt: &MirStmt, locals: &[MirLocal]) -> CodegenResult<()> {
        match &stmt.kind {
            MirStmtKind::Assign { dest, value } => {
                if locals
                    .get(dest.0 as usize)
                    .map(|local| matches!(local.ty, MirType::Void))
                    .unwrap_or(false)
                {
                    return Ok(());
                }
                let dest_name = self.local_name(*dest, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!("{} = {};", dest_name, rvalue));
            }
            MirStmtKind::DerefAssign { ptr, value } => {
                let ptr_name = self.local_name(*ptr, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!("unsafe {{ *{} = {}; }}", ptr_name, rvalue));
            }
            MirStmtKind::FieldDerefAssign {
                ptr,
                field_name,
                value,
            } => {
                let ptr_name = self.local_name(*ptr, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!(
                    "unsafe {{ (*{}).{} = {}; }}",
                    ptr_name,
                    Self::rust_ident(field_name),
                    rvalue
                ));
            }
            MirStmtKind::FieldAssign {
                base,
                field_name,
                value,
            } => {
                let base_name = self.local_name(*base, locals);
                let rvalue = self.rvalue_to_rust(value, locals)?;
                self.writeln(&format!(
                    "{}.{} = {};",
                    base_name,
                    Self::rust_ident(field_name),
                    rvalue
                ));
            }
            MirStmtKind::StorageLive(_) | MirStmtKind::StorageDead(_) | MirStmtKind::Nop => {}
        }
        Ok(())
    }

    fn generate_terminator(
        &mut self,
        term: &MirTerminator,
        locals: &[MirLocal],
        is_main: bool,
    ) -> CodegenResult<()> {
        match term {
            MirTerminator::Goto(target) => self.emit_jump(*target),
            MirTerminator::If {
                cond,
                then_block,
                else_block,
            } => {
                let cond_str = self.value_to_rust(cond, locals);
                self.writeln(&format!(
                    "if {} {{ __bb = {}; }} else {{ __bb = {}; }}",
                    cond_str, then_block.0, else_block.0
                ));
                self.writeln("continue;");
            }
            MirTerminator::Switch {
                value,
                targets,
                default,
            } => {
                let val = self.value_to_rust(value, locals);
                self.writeln(&format!("match {} {{", val));
                self.indent += 1;
                for (case, target) in targets {
                    self.writeln(&format!(
                        "{} => __bb = {},",
                        self.const_to_rust(case),
                        target.0
                    ));
                }
                self.writeln(&format!("_ => __bb = {},", default.0));
                self.indent -= 1;
                self.writeln("}");
                self.writeln("continue;");
            }
            MirTerminator::Call {
                func,
                args,
                dest,
                target,
                ..
            } => {
                self.emit_call(func, args, *dest, locals)?;
                if let Some(target) = target {
                    self.emit_jump(*target);
                }
            }
            MirTerminator::Return(value) => self.emit_return(value.as_ref(), locals, is_main),
            MirTerminator::Unreachable => self.writeln("unreachable!();"),
            MirTerminator::Drop { target, .. } => self.emit_jump(*target),
            MirTerminator::Assert {
                cond,
                expected,
                target,
                msg,
                ..
            } => {
                let mut cond_str = self.value_to_rust(cond, locals);
                if !expected {
                    cond_str = format!("!({})", cond_str);
                }
                if msg.is_empty() {
                    self.writeln(&format!("assert!({});", cond_str));
                } else {
                    self.writeln(&format!("assert!({}, {:?});", cond_str, msg.as_ref()));
                }
                self.emit_jump(*target);
            }
            MirTerminator::Resume => self.writeln("panic!(\"resume unwinding\");"),
            MirTerminator::Abort => self.writeln("std::process::abort();"),
        }
        Ok(())
    }

    fn emit_jump(&mut self, target: BlockId) {
        self.writeln(&format!("__bb = {};", target.0));
        self.writeln("continue;");
    }

    fn emit_return(&mut self, value: Option<&MirValue>, locals: &[MirLocal], is_main: bool) {
        if is_main {
            if let Some(value) = value {
                let code = self.value_to_rust(value, locals);
                self.writeln(&format!("let __code = {};", code));
                self.writeln("if __code != 0 { std::process::exit(__code as i32); }");
            }
            self.writeln("return;");
        } else if let Some(value) = value {
            self.writeln(&format!("return {};", self.value_to_rust(value, locals)));
        } else {
            self.writeln("return;");
        }
    }

    fn emit_call(
        &mut self,
        func: &MirValue,
        args: &[MirValue],
        dest: Option<LocalId>,
        locals: &[MirLocal],
    ) -> CodegenResult<()> {
        let func_name = self.value_to_rust(func, locals);
        if func_name == "printf" || func_name == "println" {
            if args.is_empty() {
                return Ok(());
            }
            let fmt = self.value_to_rust(&args[0], locals);
            let fmt = if self.value_is_string_like(&args[0], locals) {
                format!("&{}", fmt)
            } else {
                fmt
            };
            let arg_strings = args
                .iter()
                .skip(1)
                .map(|arg| format!("format!(\"{{}}\", {})", self.value_to_rust(arg, locals)))
                .collect::<Vec<_>>();
            let runtime_call = if func_name == "println" {
                "quanta_println"
            } else {
                "quanta_printf"
            };
            let call = format!("{}({}, &[{}])", runtime_call, fmt, arg_strings.join(", "));
            if let Some(dest) = dest {
                self.writeln(&format!("{} = {};", self.local_name(dest, locals), call));
            } else {
                self.writeln(&format!("{};", call));
            }
            return Ok(());
        }

        if func_name == "fflush" {
            return Ok(());
        }

        let args_str = args
            .iter()
            .map(|arg| self.value_to_owned_rust(arg, locals))
            .collect::<Vec<_>>()
            .join(", ");
        let call = format!("{}({})", func_name, args_str);
        if let Some(dest) = dest {
            self.writeln(&format!("{} = {};", self.local_name(dest, locals), call));
        } else {
            self.writeln(&format!("{};", call));
        }
        Ok(())
    }

    fn rvalue_to_rust(&self, rvalue: &MirRValue, locals: &[MirLocal]) -> CodegenResult<String> {
        Ok(match rvalue {
            MirRValue::Use(value) => self.value_to_owned_rust(value, locals),
            MirRValue::BinaryOp { op, left, right } => {
                let l = self.value_to_rust(left, locals);
                let r = self.value_to_rust(right, locals);
                if *op == BinOp::Pow {
                    format!("({} as f64).powf({} as f64)", l, r)
                } else {
                    format!("({} {} {})", l, Self::binop_to_rust(*op), r)
                }
            }
            MirRValue::UnaryOp { op, operand } => {
                let v = self.value_to_rust(operand, locals);
                let op_str = match op {
                    UnaryOp::Not => "!",
                    UnaryOp::Neg => "-",
                };
                format!("({}{})", op_str, v)
            }
            MirRValue::Ref { is_mut, place } | MirRValue::AddressOf { is_mut, place } => {
                let place = self.place_to_rust(place, locals)?;
                if *is_mut {
                    format!("(&mut {} as *mut _)", place)
                } else {
                    format!("(&{} as *const _ as *mut _)", place)
                }
            }
            MirRValue::Cast { value, ty, .. } => {
                format!(
                    "({} as {})",
                    self.value_to_rust(value, locals),
                    self.type_to_rust(ty)
                )
            }
            MirRValue::Aggregate { kind, operands } => match kind {
                AggregateKind::Array(_) => {
                    let vals = operands
                        .iter()
                        .map(|op| self.value_to_owned_rust(op, locals))
                        .collect::<Vec<_>>();
                    format!("[{}]", vals.join(", "))
                }
                AggregateKind::Tuple => {
                    let vals = operands
                        .iter()
                        .map(|op| self.value_to_rust(op, locals))
                        .collect::<Vec<_>>();
                    match vals.len() {
                        0 => "()".to_string(),
                        1 => format!("({},)", vals[0]),
                        _ => format!("({})", vals.join(", ")),
                    }
                }
                AggregateKind::Struct(name) => {
                    let vals = operands
                        .iter()
                        .map(|op| self.value_to_rust(op, locals))
                        .collect::<Vec<_>>();
                    let type_name = Self::rust_type_name(name);
                    let fields = self.struct_fields.get(name.as_ref());
                    if let Some(fields) = fields {
                        let pairs = vals
                            .iter()
                            .enumerate()
                            .map(|(i, val)| {
                                let field = fields
                                    .get(i)
                                    .cloned()
                                    .unwrap_or_else(|| format!("field{}", i));
                                format!("{}: {}", field, val)
                            })
                            .collect::<Vec<_>>();
                        format!("{} {{ {} }}", type_name, pairs.join(", "))
                    } else if vals.is_empty() {
                        format!("{} {{}}", type_name)
                    } else {
                        return Err(CodegenError::Unsupported(format!(
                                "Rust backend cannot lower struct aggregate '{}' without field metadata",
                                name
                            )));
                    }
                }
                AggregateKind::Variant(_, _, _) | AggregateKind::Closure(_) => {
                    return Err(CodegenError::Unsupported(
                        "Rust backend does not yet lower enum variants or closures".to_string(),
                    ));
                }
            },
            MirRValue::Repeat { value, count } => {
                let value_str = self.value_to_rust(value, locals);
                if self.value_is_copy_like(value, locals) {
                    format!("[{}; {}]", value_str, count)
                } else {
                    format!("std::array::from_fn(|_| ({}).clone())", value_str)
                }
            }
            MirRValue::Discriminant(_)
            | MirRValue::VariantField { .. }
            | MirRValue::TextureSample { .. } => {
                return Err(CodegenError::Unsupported(
                    "Rust backend does not yet lower enum discriminants, variant fields, or texture samples"
                        .to_string(),
                ));
            }
            MirRValue::Len(place) => format!("{}.len()", self.place_to_rust(place, locals)?),
            MirRValue::NullaryOp(op, ty) => match op {
                NullaryOp::SizeOf => format!("std::mem::size_of::<{}>()", self.type_to_rust(ty)),
                NullaryOp::AlignOf => format!("std::mem::align_of::<{}>()", self.type_to_rust(ty)),
            },
            MirRValue::FieldAccess {
                base,
                field_name,
                field_ty,
            } => {
                let base_str = self.value_to_rust(base, locals);
                let field = Self::rust_ident(field_name);
                let access = if self.value_is_raw_pointer(base, locals) {
                    format!("unsafe {{ (*{}).{} }}", base_str, field)
                } else {
                    format!("{}.{}", base_str, field)
                };
                if Self::is_copy_like_type(field_ty) {
                    access
                } else {
                    format!("({}).clone()", access)
                }
            }
            MirRValue::IndexAccess {
                base,
                index,
                elem_ty,
            } => {
                let access = format!(
                    "{}[{} as usize]",
                    self.value_to_rust(base, locals),
                    self.value_to_rust(index, locals)
                );
                if Self::is_copy_like_type(elem_ty) {
                    access
                } else {
                    format!("({}).clone()", access)
                }
            }
            MirRValue::Deref { ptr, .. } => {
                format!("unsafe {{ *{} }}", self.value_to_rust(ptr, locals))
            }
        })
    }

    fn place_to_rust(&self, place: &MirPlace, locals: &[MirLocal]) -> CodegenResult<String> {
        let mut out = self.local_name(place.local, locals);
        for projection in &place.projections {
            match projection {
                PlaceProjection::Deref => out = format!("unsafe {{ *{} }}", out),
                PlaceProjection::Field(idx, _) => out = format!("{}.field{}", out, idx),
                PlaceProjection::Index(id) => {
                    out = format!("{}[{} as usize]", out, self.local_name(*id, locals));
                }
                PlaceProjection::ConstantIndex { offset, .. } => {
                    out = format!("{}[{}]", out, offset);
                }
                PlaceProjection::Subslice { .. } | PlaceProjection::Downcast(_) => {
                    return Err(CodegenError::Unsupported(
                        "Rust backend does not yet lower subslice or downcast places".to_string(),
                    ));
                }
            }
        }
        Ok(out)
    }

    fn value_to_rust(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        match value {
            MirValue::Local(id) => self.local_name(*id, locals),
            MirValue::Const(c) => self.const_to_rust(c),
            MirValue::Global(name) | MirValue::Function(name) => Self::rust_ident(name),
        }
    }

    fn const_to_rust(&self, c: &MirConst) -> String {
        match c {
            MirConst::Bool(b) => b.to_string(),
            MirConst::Int(v, _) => v.to_string(),
            MirConst::Uint(v, _) => v.to_string(),
            MirConst::Float(v, ty) => {
                let mut s = v.to_string();
                if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                    s.push_str(".0");
                }
                if matches!(ty, MirType::Float(FloatSize::F32)) {
                    s.push_str("f32");
                }
                s
            }
            MirConst::Str(idx) => format!("__STR{}", idx),
            MirConst::ByteStr(bytes) => format!("{:?}", bytes),
            MirConst::Null(_) => "std::ptr::null_mut()".to_string(),
            MirConst::Unit => "()".to_string(),
            MirConst::Zeroed(ty) => self.default_value(ty),
            MirConst::Undef(ty) => self.default_value(ty),
            MirConst::Struct(name, fields) => {
                let type_name = Self::rust_type_name(name);
                let field_names = self.struct_fields.get(name.as_ref());
                if let Some(field_names) = field_names {
                    let fields = fields
                        .iter()
                        .enumerate()
                        .map(|(i, value)| {
                            let field = field_names
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("field{}", i));
                            format!("{}: {}", field, self.const_to_rust(value))
                        })
                        .collect::<Vec<_>>();
                    format!("{} {{ {} }}", type_name, fields.join(", "))
                } else {
                    format!("{}::default()", type_name)
                }
            }
        }
    }

    fn type_to_rust(&self, ty: &MirType) -> String {
        match ty {
            MirType::Void | MirType::Never => "()".to_string(),
            MirType::Bool => "bool".to_string(),
            MirType::Int(size, signed) => match (size, signed) {
                (IntSize::I8, true) => "i8".to_string(),
                (IntSize::I8, false) => "u8".to_string(),
                (IntSize::I16, true) => "i16".to_string(),
                (IntSize::I16, false) => "u16".to_string(),
                (IntSize::I32, true) => "i32".to_string(),
                (IntSize::I32, false) => "u32".to_string(),
                (IntSize::I64, true) => "i64".to_string(),
                (IntSize::I64, false) => "u64".to_string(),
                (IntSize::I128, true) => "i128".to_string(),
                (IntSize::I128, false) => "u128".to_string(),
                (IntSize::ISize, true) => "isize".to_string(),
                (IntSize::ISize, false) => "usize".to_string(),
            },
            MirType::Float(FloatSize::F32) => "f32".to_string(),
            MirType::Float(FloatSize::F64) => "f64".to_string(),
            MirType::Ptr(inner) if Self::is_i8_type(inner) => "&'static str".to_string(),
            MirType::Ptr(inner) => format!("*mut {}", self.type_to_rust(inner)),
            MirType::Array(elem, len) => format!("[{}; {}]", self.type_to_rust(elem), len),
            MirType::Slice(elem) => format!("&[{}]", self.type_to_rust(elem)),
            MirType::Struct(name) if name.as_ref() == "QuantaString" => "String".to_string(),
            MirType::Struct(name) if name.as_ref() == "String" => "String".to_string(),
            MirType::Struct(name) => Self::rust_type_name(name),
            MirType::FnPtr(sig) => {
                let params = sig
                    .params
                    .iter()
                    .map(|p| self.type_to_rust(p))
                    .collect::<Vec<_>>();
                format!(
                    "fn({}) -> {}",
                    params.join(", "),
                    self.type_to_rust(&sig.ret)
                )
            }
            MirType::Vector(elem, lanes) => format!("[{}; {}]", self.type_to_rust(elem), lanes),
            MirType::Texture2D(_)
            | MirType::Sampler
            | MirType::SampledImage(_)
            | MirType::TraitObject(_) => "*mut std::ffi::c_void".to_string(),
            MirType::Vec(elem) => format!("Vec<{}>", self.type_to_rust(elem)),
            MirType::Map(key, value) => format!(
                "std::collections::BTreeMap<{}, {}>",
                self.type_to_rust(key),
                self.type_to_rust(value)
            ),
            MirType::Tuple(elems) => {
                if elems.is_empty() {
                    "()".to_string()
                } else if elems.len() == 1 {
                    format!("({},)", self.type_to_rust(&elems[0]))
                } else {
                    let elems = elems
                        .iter()
                        .map(|e| self.type_to_rust(e))
                        .collect::<Vec<_>>();
                    format!("({})", elems.join(", "))
                }
            }
        }
    }

    fn default_value(&self, ty: &MirType) -> String {
        match ty {
            MirType::Void | MirType::Never => "()".to_string(),
            MirType::Bool => "false".to_string(),
            MirType::Int(_, _) => "0".to_string(),
            MirType::Float(FloatSize::F32) => "0.0f32".to_string(),
            MirType::Float(FloatSize::F64) => "0.0".to_string(),
            MirType::Ptr(inner) if Self::is_i8_type(inner) => "\"\"".to_string(),
            MirType::Ptr(_) => "std::ptr::null_mut()".to_string(),
            MirType::Array(elem, len) => {
                let elem_default = self.default_value(elem);
                if Self::is_copy_like_type(elem) {
                    format!("[{}; {}]", elem_default, len)
                } else {
                    format!("std::array::from_fn(|_| {})", elem_default)
                }
            }
            MirType::Slice(_) => "&[]".to_string(),
            MirType::Struct(name) if name.as_ref() == "String" => "String::new()".to_string(),
            MirType::Struct(name) if name.as_ref() == "QuantaString" => "String::new()".to_string(),
            MirType::Vec(_) => "Vec::new()".to_string(),
            MirType::Map(_, _) => "std::collections::BTreeMap::new()".to_string(),
            _ => "Default::default()".to_string(),
        }
    }

    fn local_name(&self, id: LocalId, locals: &[MirLocal]) -> String {
        locals
            .get(id.0 as usize)
            .and_then(|local| local.name.as_ref())
            .map(|name| {
                let base = Self::rust_ident(name);
                let has_dup = locals.iter().any(|other| {
                    other.id != id && other.name.as_ref().map(|s| s.as_ref()) == Some(name.as_ref())
                });
                if has_dup {
                    format!("{}_{}", base, id.0)
                } else {
                    base
                }
            })
            .unwrap_or_else(|| format!("_{}", id.0))
    }

    fn value_is_raw_pointer(&self, value: &MirValue, locals: &[MirLocal]) -> bool {
        match value {
            MirValue::Local(id) => locals
                .get(id.0 as usize)
                .map(|local| matches!(local.ty, MirType::Ptr(_)))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn value_is_string_like(&self, value: &MirValue, locals: &[MirLocal]) -> bool {
        match value {
            MirValue::Local(id) => locals
                .get(id.0 as usize)
                .map(|local| Self::is_string_like_type(&local.ty))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn value_to_owned_rust(&self, value: &MirValue, locals: &[MirLocal]) -> String {
        let value_str = self.value_to_rust(value, locals);
        if self.value_is_copy_like(value, locals) {
            value_str
        } else {
            format!("({}).clone()", value_str)
        }
    }

    fn value_is_copy_like(&self, value: &MirValue, locals: &[MirLocal]) -> bool {
        match value {
            MirValue::Local(id) => locals
                .get(id.0 as usize)
                .map(|local| Self::is_copy_like_type(&local.ty))
                .unwrap_or(true),
            MirValue::Const(c) => Self::const_is_copy_like(c),
            MirValue::Global(_) | MirValue::Function(_) => true,
        }
    }

    fn is_string_like_type(ty: &MirType) -> bool {
        matches!(
            ty,
            MirType::Struct(name) if name.as_ref() == "String" || name.as_ref() == "QuantaString"
        )
    }

    fn const_is_copy_like(c: &MirConst) -> bool {
        match c {
            MirConst::Bool(_)
            | MirConst::Int(_, _)
            | MirConst::Uint(_, _)
            | MirConst::Float(_, _)
            | MirConst::Str(_)
            | MirConst::ByteStr(_)
            | MirConst::Null(_)
            | MirConst::Unit => true,
            MirConst::Zeroed(ty) | MirConst::Undef(ty) => Self::is_copy_like_type(ty),
            MirConst::Struct(_, _) => false,
        }
    }

    fn is_copy_like_type(ty: &MirType) -> bool {
        match ty {
            MirType::Void
            | MirType::Never
            | MirType::Bool
            | MirType::Int(_, _)
            | MirType::Float(_)
            | MirType::Ptr(_)
            | MirType::FnPtr(_)
            | MirType::Texture2D(_)
            | MirType::Sampler
            | MirType::SampledImage(_)
            | MirType::TraitObject(_) => true,
            MirType::Array(elem, _) | MirType::Vector(elem, _) => Self::is_copy_like_type(elem),
            MirType::Tuple(elems) => elems.iter().all(Self::is_copy_like_type),
            MirType::Struct(name)
                if name.as_ref() == "String" || name.as_ref() == "QuantaString" =>
            {
                false
            }
            MirType::Struct(_) | MirType::Slice(_) | MirType::Vec(_) | MirType::Map(_, _) => false,
        }
    }

    fn binop_to_rust(op: BinOp) -> &'static str {
        match op {
            BinOp::Add | BinOp::AddChecked | BinOp::AddWrapping | BinOp::AddSaturating => "+",
            BinOp::Sub | BinOp::SubChecked | BinOp::SubWrapping | BinOp::SubSaturating => "-",
            BinOp::Mul | BinOp::MulChecked | BinOp::MulWrapping => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Pow => unreachable!("handled before operator conversion"),
        }
    }

    fn is_i8_type(ty: &MirType) -> bool {
        matches!(ty, MirType::Int(IntSize::I8, _))
    }

    fn rust_type_name(name: &str) -> String {
        Self::rust_ident(name)
    }

    fn rust_ident(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        for (i, ch) in name.chars().enumerate() {
            if (i == 0 && (ch.is_ascii_alphabetic() || ch == '_'))
                || (i > 0 && (ch.is_ascii_alphanumeric() || ch == '_'))
            {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
            out.insert(0, '_');
        }
        if Self::is_rust_reserved(&out) {
            out.insert(0, '_');
        }
        out
    }

    fn is_rust_reserved(name: &str) -> bool {
        matches!(
            name,
            "as" | "break"
                | "const"
                | "continue"
                | "crate"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "async"
                | "await"
                | "dyn"
                | "try"
                | "union"
                | "yield"
        )
    }
}

impl Default for RustBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for RustBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        self.output.clear();
        self.indent = 0;
        self.strings = mir.strings.clone();
        self.collect_struct_fields(&mir.types);

        self.writeln("// Generated by QuantaLang Compiler");
        self.writeln("// Rust target is experimental and subset-based.");
        self.writeln("#![allow(dead_code, non_snake_case, non_camel_case_types, unused_assignments, unused_mut, unused_parens, unused_variables, unreachable_code)]");
        self.writeln("");

        self.emit_runtime();
        self.emit_type_definitions(&mir.types)?;
        self.emit_string_table();

        for func in &mir.functions {
            if !func.is_declaration() {
                self.generate_function(func)?;
            }
        }

        Ok(GeneratedCode::new(
            OutputFormat::RustSource,
            self.output.clone().into_bytes(),
        ))
    }

    fn target(&self) -> Target {
        Target::Rust
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::CodeGenerator;
    use crate::lexer::{Lexer, SourceFile};
    use crate::parser::Parser;
    use crate::types::{TypeChecker, TypeContext};

    fn compile_quanta_to_rust(source: &str) -> String {
        let source_file = SourceFile::new("rust_backend_test.quanta", source);
        let mut lexer = Lexer::new(&source_file);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(&source_file, tokens);
        let ast = parser.parse().expect("parsing should succeed");
        assert!(
            parser.errors().is_empty(),
            "unexpected parser errors: {:?}",
            parser.errors()
        );

        let mut ctx = TypeContext::new();
        let mut checker = TypeChecker::new(&mut ctx);
        checker.check_module(&ast);
        assert!(
            !checker.has_errors(),
            "unexpected type errors: {:?}",
            checker.errors()
        );

        let mut codegen =
            CodeGenerator::with_source(&ctx, Target::Rust, source_file.source().into());
        codegen
            .generate(&ast)
            .expect("rust codegen should succeed")
            .as_string()
            .expect("generated Rust should be UTF-8")
    }

    fn assert_rustc_metadata_ok(name: &str, rust_source: &str) {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let dir = std::env::temp_dir().join(format!(
            "quantalang_rust_backend_{}_{}",
            name,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let source_path = dir.join("generated.rs");
        let metadata_path = dir.join("generated.rmeta");
        std::fs::write(&source_path, rust_source).expect("write generated Rust");

        let output = std::process::Command::new(rustc)
            .arg("--emit=metadata")
            .arg("-o")
            .arg(&metadata_path)
            .arg(&source_path)
            .output()
            .expect("invoke rustc");

        assert!(
            output.status.success(),
            "rustc failed for {name}\nstdout:\n{}\nstderr:\n{}\nsource:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            rust_source
        );
    }

    #[test]
    fn backend_target_is_rust() {
        let backend = RustBackend::new();
        assert_eq!(backend.target(), Target::Rust);
    }

    #[test]
    fn generated_rust_compiles_for_scalar_branch_subset() {
        let source = r#"
fn choose(x: i32) -> i32 {
    if x > 0 { x } else { 0 }
}

fn main() {
    let v: i32 = choose(4);
    println("{}", v);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("scalar_branch", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reference_subset() {
        let source = r#"
fn add_to(x: &mut i32, amount: i32) {
    *x = *x + amount;
}

fn read_value(x: &i32) -> i32 {
    *x
}

fn main() {
    let mut n: i32 = 10;
    add_to(&mut n, 5);
    let val: i32 = read_value(&n);
    println("{}", val);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("references", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_structs_and_arrays() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn sum_array(arr: [i32; 3]) -> i32 {
    arr[0] + arr[1] + arr[2]
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let values = [p.x, p.y, 5];
    let total = sum_array(values);
    println("{}", total);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("structs_arrays", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_struct_field_references() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let rx: &i32 = &p.x;
    println("{}", *rx);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("struct_field_references", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_repeated_non_copy_struct_arrays() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let points = [p; 2];
    println("{}", points[0].x);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("repeated_non_copy_struct_arrays", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reused_struct_after_by_value_call() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn sum(p: Point) -> i32 {
    p.x + p.y
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let first = sum(p);
    let second = sum(p);
    println("{}", first + second);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("reused_struct_after_by_value_call", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reused_struct_after_assignment() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let q = p;
    let r = p;
    println("{}", q.x + r.y);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("reused_struct_after_assignment", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_reused_non_copy_struct_field_access() {
        let source = r#"
struct Point {
    x: i32,
    y: i32,
}

struct Wrapper {
    inner: Point,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let w = Wrapper { inner: p };
    let a = w.inner;
    let b = w.inner;
    println("{}", a.x + b.y);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("reused_non_copy_struct_field_access", &rust);
    }

    #[test]
    fn generated_rust_compiles_for_lifetime_smoke_program() {
        let source = r#"
fn identity(x: &i32) -> &i32 {
    x
}

fn main() {
    let a: i32 = 42;
    let r: &i32 = identity(&a);
    println("{}", *r);
}
"#;
        let rust = compile_quanta_to_rust(source);
        assert_rustc_metadata_ok("lifetime_smoke", &rust);
    }
}
