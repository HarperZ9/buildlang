// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! x86-64 native executables written by the compiler itself: MIR is lowered
//! to machine code here, the runtime comes from `x86_64_exe_asm`, and the
//! container (PE32+ or ELF64) from `x86_64_exe_obj`. No C compiler,
//! assembler or linker runs.
//!
//! Every local lives in an 8-byte stack slot; values pass through rax and
//! rcx. Integers narrower than 64 bits are kept sign- or zero-extended in
//! their slot, so 64-bit compare, divide and shift give the narrow result.
//! Anything outside the supported set is refused with a diagnostic that
//! names the construct; nothing is emitted as a placeholder.

use std::collections::HashMap;

use super::x86_64_exe_asm::{emit_runtime, Asm, ExeOs, Runtime, RAX, RCX, RDX};
use super::x86_64_exe_obj::{write_elf, write_pe};
use super::{Backend, CodegenError, CodegenResult, Target};
use crate::codegen::ir::*;
use crate::codegen::{GeneratedCode, OutputFormat};

pub(super) fn refuse<T>(what: String) -> CodegenResult<T> {
    Err(CodegenError::Unsupported(format!(
        "the x86-64 native target does not support {what}"
    )))
}

/// Backend that returns a complete executable file.
pub struct NativeExeBackend {
    os: ExeOs,
}

impl NativeExeBackend {
    pub fn new(os: ExeOs) -> Self {
        Self { os }
    }
}

impl Backend for NativeExeBackend {
    fn generate(&mut self, mir: &MirModule) -> CodegenResult<GeneratedCode> {
        let bytes = build_executable(mir, self.os)?;
        Ok(GeneratedCode::new(OutputFormat::Executable, bytes))
    }
    fn target(&self) -> Target {
        match self.os {
            ExeOs::Windows => Target::X86_64Pe,
            ExeOs::Linux => Target::X86_64Elf,
        }
    }
}

/// Lower a whole module and wrap it in the container for `os`.
pub fn build_executable(mir: &MirModule, os: ExeOs) -> CodegenResult<Vec<u8>> {
    let mut a = Asm::default();
    let rt = emit_runtime(&mut a, os);
    let mut fns = HashMap::new();
    for f in &mir.functions {
        if f.blocks.is_some() {
            fns.insert(f.name.to_string(), (a.label(), f.sig.ret.clone()));
        }
    }
    let Some((main_label, main_ret)) = fns.get("main").cloned() else {
        return refuse("a program without `fn main`".into());
    };
    let entry = a.label();
    a.bind(entry);
    a.b(&[0x48, 0x83, 0xE4, 0xF0, 0x48, 0x83, 0xEC, 0x20]); // align stack, shadow space
    a.call(main_label);
    let a0 = os.arg_regs()[0];
    match main_ret {
        MirType::Int(..) => a.mov_rr(a0, RAX),
        _ => a.mov_imm(a0, 0),
    }
    a.call(rt.exit);
    let strings: Vec<Vec<u8>> = mir.strings.iter().map(|s| s.as_bytes().to_vec()).collect();
    for f in &mir.functions {
        if let Some(blocks) = &f.blocks {
            let label = fns[f.name.as_ref()].0;
            let mut g = FnGen::new(&mut a, os, &rt, &fns, &strings, f)?;
            g.a.bind(label);
            g.function(blocks)?;
        }
    }
    let linked = a.finish(entry).map_err(CodegenError::Internal)?;
    Ok(match os {
        ExeOs::Windows => write_pe(&linked),
        ExeOs::Linux => {
            if !linked.imports.is_empty() {
                return Err(CodegenError::Internal("imports in an ELF image".into()));
            }
            write_elf(&linked)
        }
    })
}

pub(super) struct FnGen<'a> {
    pub(super) a: &'a mut Asm,
    pub(super) os: ExeOs,
    pub(super) rt: &'a Runtime,
    fns: &'a HashMap<String, (usize, MirType)>,
    pub(super) strings: &'a [Vec<u8>],
    pub(super) f: &'a MirFunction,
    slots: HashMap<LocalId, (i32, MirType)>,
    blocks: HashMap<BlockId, usize>,
    /// Locals that hold a string literal: local -> string index.
    pub(super) str_locals: HashMap<LocalId, usize>,
}

fn supported_local(ty: &MirType) -> bool {
    match ty {
        MirType::Void | MirType::Bool => true,
        MirType::Int(size, _) => *size != IntSize::I128,
        MirType::Ptr(inner) => matches!(**inner, MirType::Int(IntSize::I8, _)),
        _ => false,
    }
}

impl<'a> FnGen<'a> {
    fn new(
        a: &'a mut Asm,
        os: ExeOs,
        rt: &'a Runtime,
        fns: &'a HashMap<String, (usize, MirType)>,
        strings: &'a [Vec<u8>],
        f: &'a MirFunction,
    ) -> CodegenResult<Self> {
        let mut str_locals = HashMap::new();
        for b in f.blocks.iter().flatten() {
            for s in &b.stmts {
                if let MirStmtKind::Assign {
                    dest,
                    value: MirRValue::Use(MirValue::Const(MirConst::Str(i))),
                } = &s.kind
                {
                    str_locals.insert(*dest, *i as usize);
                }
            }
            if let Some(MirTerminator::Call {
                func: MirValue::Function(n) | MirValue::Global(n),
                args,
                dest: Some(d),
                ..
            }) = &b.terminator
            {
                if n.as_ref() == "build_string_new" {
                    if let [MirValue::Const(MirConst::Str(i))] = args.as_slice() {
                        str_locals.insert(*d, *i as usize);
                    }
                }
            }
        }
        let mut slots = HashMap::new();
        for (i, l) in f.locals.iter().enumerate() {
            let literal = str_locals.contains_key(&l.id)
                && matches!(&l.ty, MirType::Struct(n) if n.as_ref() == "BuildString");
            if !supported_local(&l.ty) && !literal {
                return refuse(format!("a value of type `{}` in `fn {}`", l.ty, f.name));
            }
            slots.insert(l.id, (-8 * (i as i32 + 1), l.ty.clone()));
        }
        Ok(Self {
            a,
            os,
            rt,
            fns,
            strings,
            f,
            slots,
            blocks: HashMap::new(),
            str_locals,
        })
    }

    fn function(&mut self, blocks: &[MirBlock]) -> CodegenResult<()> {
        let params: Vec<LocalId> = self
            .f
            .locals
            .iter()
            .filter(|l| l.is_param)
            .map(|l| l.id)
            .collect();
        if params.len() > self.os.arg_regs().len() {
            return refuse(format!(
                "more than {} parameters (`fn {}`)",
                self.os.arg_regs().len(),
                self.f.name
            ));
        }
        let frame = (8 * self.f.locals.len() as u32).div_ceil(16) * 16 + 32;
        self.a.prologue(frame);
        for (i, p) in params.iter().enumerate() {
            let off = self.slot(*p)?.0;
            self.a.store(self.os.arg_regs()[i], off);
        }
        for b in blocks {
            let l = self.a.label();
            self.blocks.insert(b.id, l);
        }
        for b in blocks {
            self.a.bind(self.blocks[&b.id]);
            for s in &b.stmts {
                self.stmt(s)?;
            }
            match &b.terminator {
                Some(t) => self.terminator(t)?,
                None => {
                    return refuse(format!(
                        "a block without a terminator in `fn {}`",
                        self.f.name
                    ))
                }
            }
        }
        Ok(())
    }

    pub(super) fn slot(&self, id: LocalId) -> CodegenResult<(i32, MirType)> {
        self.slots
            .get(&id)
            .cloned()
            .ok_or_else(|| CodegenError::InvalidMir(format!("unknown local {id:?}")))
    }

    pub(super) fn block(&self, id: BlockId) -> CodegenResult<usize> {
        self.blocks
            .get(&id)
            .copied()
            .ok_or_else(|| CodegenError::InvalidMir(format!("unknown block {id:?}")))
    }

    pub(super) fn value_type(&self, v: &MirValue) -> CodegenResult<MirType> {
        Ok(match v {
            MirValue::Local(id) => self.slot(*id)?.1,
            MirValue::Const(MirConst::Int(_, t) | MirConst::Uint(_, t)) => t.clone(),
            MirValue::Const(MirConst::Bool(_)) => MirType::Bool,
            MirValue::Const(_) => MirType::Void,
            _ => return refuse(format!("the value `{v}` here")),
        })
    }

    /// Load a value into rax.
    pub(super) fn load(&mut self, v: &MirValue) -> CodegenResult<()> {
        match v {
            MirValue::Local(id) => {
                let off = self.slot(*id)?.0;
                self.a.load(RAX, off);
            }
            MirValue::Const(c) => match c {
                MirConst::Int(x, t) => {
                    self.a.mov_imm(RAX, *x as u64);
                    self.normalize(t);
                }
                MirConst::Uint(x, t) => {
                    self.a.mov_imm(RAX, *x as u64);
                    self.normalize(t);
                }
                MirConst::Bool(b) => self.a.mov_imm(RAX, *b as u64),
                MirConst::Unit | MirConst::Null(_) => self.a.mov_imm(RAX, 0),
                MirConst::Str(i) => {
                    let bytes = self.strings.get(*i as usize).cloned().unwrap_or_default();
                    let l = self.a.data(&bytes);
                    self.a.lea_rip(RAX, l);
                }
                other => return refuse(format!("the constant `{other}`")),
            },
            MirValue::Global(n) => return refuse(format!("the global `{n}`")),
            MirValue::Function(n) => return refuse(format!("`{n}` used as a value")),
        }
        Ok(())
    }

    /// Re-extend rax to the width and signedness of `ty`.
    pub(super) fn normalize(&mut self, ty: &MirType) {
        let bytes: &[u8] = match ty {
            MirType::Int(IntSize::I32, true) => &[0x48, 0x63, 0xC0],
            MirType::Int(IntSize::I32, false) => &[0x89, 0xC0],
            MirType::Int(IntSize::I16, true) => &[0x48, 0x0F, 0xBF, 0xC0],
            MirType::Int(IntSize::I16, false) => &[0x0F, 0xB7, 0xC0],
            MirType::Int(IntSize::I8, true) => &[0x48, 0x0F, 0xBE, 0xC0],
            MirType::Int(IntSize::I8, false) => &[0x0F, 0xB6, 0xC0],
            _ => &[],
        };
        self.a.b(bytes);
    }

    /// Store rax into a local, normalized to the local's type.
    pub(super) fn store(&mut self, dest: LocalId) -> CodegenResult<()> {
        let (off, ty) = self.slot(dest)?;
        self.normalize(&ty);
        self.a.store(RAX, off);
        Ok(())
    }

    /// A call to another function of this program (platform convention).
    pub(super) fn call_user(
        &mut self,
        name: &str,
        args: &[MirValue],
        dest: Option<LocalId>,
    ) -> CodegenResult<()> {
        let Some((label, _)) = self.fns.get(name).cloned() else {
            return refuse(format!(
                "a call to `{name}`, which has no native runtime yet"
            ));
        };
        let regs = self.os.arg_regs();
        if args.len() > regs.len() {
            return refuse(format!(
                "a call with more than {} arguments (`{name}`)",
                regs.len()
            ));
        }
        for (i, arg) in args.iter().enumerate() {
            self.load(arg)?;
            if regs[i] != RAX {
                self.a.mov_rr(regs[i], RAX);
            }
        }
        self.a.call(label);
        if let Some(d) = dest {
            self.store(d)?;
        }
        Ok(())
    }

    /// Load `left` into rax and `right` into rcx.
    pub(super) fn load_pair(&mut self, left: &MirValue, right: &MirValue) -> CodegenResult<()> {
        self.load(right)?;
        self.a.mov_rr(RCX, RAX);
        self.load(left)
    }

    /// Divide rax by rcx; `rem` leaves the remainder in rax.
    pub(super) fn divide(&mut self, signed: bool, rem: bool) {
        if signed {
            self.a.b(&[0x48, 0x99, 0x48, 0xF7, 0xF9]); // cqo; idiv rcx
        } else {
            self.a.b(&[0x31, 0xD2, 0x48, 0xF7, 0xF1]); // xor edx, edx; div rcx
        }
        if rem {
            self.a.mov_rr(RAX, RDX);
        }
    }
}
