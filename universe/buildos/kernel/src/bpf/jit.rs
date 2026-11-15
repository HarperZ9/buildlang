// ===============================================================================
// BUILDOS KERNEL - BPF JIT COMPILER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! BPF JIT Compiler
//!
//! Compiles BPF bytecode to native machine code for better performance.
//! Currently targets x86-64 architecture.

#![allow(dead_code)]

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use super::{BpfInsn, BpfError};
use super::{BPF_ADD, BPF_SUB, BPF_OR, BPF_AND};
use super::{BPF_XOR, BPF_MOV};
use super::{BPF_JEQ, BPF_JGT, BPF_JGE, BPF_JSET, BPF_JNE, BPF_EXIT, BPF_CALL};

/// JIT enabled flag
static JIT_ENABLED: AtomicBool = AtomicBool::new(true);

/// Enable/disable JIT
pub fn set_jit_enabled(enabled: bool) {
    JIT_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check if JIT is enabled
pub fn is_jit_enabled() -> bool {
    JIT_ENABLED.load(Ordering::Relaxed)
}

// =============================================================================
// X86-64 REGISTER MAPPING
// =============================================================================

/// BPF to x86-64 register mapping
mod x86_regs {
    // BPF R0-R9 -> x86-64 registers
    // R0  -> RAX (return value)
    // R1  -> RDI (arg1 / ctx)
    // R2  -> RSI (arg2)
    // R3  -> RDX (arg3)
    // R4  -> RCX (arg4)
    // R5  -> R8  (arg5)
    // R6  -> RBX (callee-saved)
    // R7  -> R13 (callee-saved)
    // R8  -> R14 (callee-saved)
    // R9  -> R15 (callee-saved)
    // R10 -> RBP (frame pointer)

    pub const RAX: u8 = 0;
    pub const RCX: u8 = 1;
    pub const RDX: u8 = 2;
    pub const RBX: u8 = 3;
    pub const RSP: u8 = 4;
    pub const RBP: u8 = 5;
    pub const RSI: u8 = 6;
    pub const RDI: u8 = 7;
    pub const R8: u8 = 8;
    pub const R9: u8 = 9;
    pub const R10: u8 = 10;
    pub const R11: u8 = 11;
    pub const R12: u8 = 12;
    pub const R13: u8 = 13;
    pub const R14: u8 = 14;
    pub const R15: u8 = 15;

    /// Map BPF register to x86-64 register
    pub fn bpf_to_x86(bpf_reg: u8) -> u8 {
        match bpf_reg {
            0 => RAX,
            1 => RDI,
            2 => RSI,
            3 => RDX,
            4 => RCX,
            5 => R8,
            6 => RBX,
            7 => R13,
            8 => R14,
            9 => R15,
            10 => RBP,
            _ => RAX,
        }
    }

    /// Check if register needs REX prefix
    pub fn needs_rex_r(reg: u8) -> bool {
        reg >= 8
    }

    /// Get register encoding (lower 3 bits)
    pub fn reg_code(reg: u8) -> u8 {
        reg & 0x07
    }
}

// =============================================================================
// JIT CONTEXT
// =============================================================================

/// JIT compilation context
pub struct JitContext {
    /// Generated code
    code: Vec<u8>,
    /// Instruction offsets (BPF PC -> code offset)
    offsets: Vec<usize>,
    /// Pending jumps to fix up
    pending_jumps: Vec<(usize, usize, i16)>, // (code_offset, target_pc, original_off)
}

impl JitContext {
    /// Create new context
    pub fn new(prog_len: usize) -> Self {
        Self {
            code: Vec::with_capacity(prog_len * 64), // Estimate
            offsets: vec![0; prog_len + 1],
            pending_jumps: Vec::new(),
        }
    }

    /// Emit byte
    fn emit(&mut self, byte: u8) {
        self.code.push(byte);
    }

    /// Emit bytes
    fn emit_bytes(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }

    /// Emit 32-bit immediate
    fn emit_imm32(&mut self, imm: i32) {
        self.emit_bytes(&imm.to_le_bytes());
    }

    /// Emit 64-bit immediate
    fn emit_imm64(&mut self, imm: i64) {
        self.emit_bytes(&imm.to_le_bytes());
    }

    /// Current code position
    fn pos(&self) -> usize {
        self.code.len()
    }

    /// Emit REX prefix
    fn emit_rex(&mut self, w: bool, r: u8, x: u8, b: u8) {
        let rex = 0x40
            | (if w { 0x08 } else { 0 })
            | (if x86_regs::needs_rex_r(r) { 0x04 } else { 0 })
            | (if x != 0 { 0x02 } else { 0 })
            | (if x86_regs::needs_rex_r(b) { 0x01 } else { 0 });

        if rex != 0x40 {
            self.emit(rex);
        }
    }

    /// Emit REX.W prefix (64-bit operation)
    fn emit_rex_w(&mut self, r: u8, b: u8) {
        self.emit_rex(true, r, 0, b);
    }

    /// Emit ModRM byte
    fn emit_modrm(&mut self, mode: u8, reg: u8, rm: u8) {
        self.emit((mode << 6) | (x86_regs::reg_code(reg) << 3) | x86_regs::reg_code(rm));
    }

    /// Emit function prologue
    fn emit_prologue(&mut self) {
        // push rbp
        self.emit(0x55);
        // mov rbp, rsp
        self.emit_rex_w(x86_regs::RBP, x86_regs::RSP);
        self.emit(0x89);
        self.emit_modrm(0b11, x86_regs::RSP, x86_regs::RBP);

        // Push callee-saved registers
        // push rbx
        self.emit(0x53);
        // push r13
        self.emit(0x41);
        self.emit(0x55);
        // push r14
        self.emit(0x41);
        self.emit(0x56);
        // push r15
        self.emit(0x41);
        self.emit(0x57);

        // Allocate stack space (512 bytes for BPF stack)
        // sub rsp, 512
        self.emit_rex_w(0, x86_regs::RSP);
        self.emit(0x81);
        self.emit_modrm(0b11, 5, x86_regs::RSP);
        self.emit_imm32(512);
    }

    /// Emit function epilogue
    fn emit_epilogue(&mut self) {
        // add rsp, 512
        self.emit_rex_w(0, x86_regs::RSP);
        self.emit(0x81);
        self.emit_modrm(0b11, 0, x86_regs::RSP);
        self.emit_imm32(512);

        // Pop callee-saved registers
        // pop r15
        self.emit(0x41);
        self.emit(0x5F);
        // pop r14
        self.emit(0x41);
        self.emit(0x5E);
        // pop r13
        self.emit(0x41);
        self.emit(0x5D);
        // pop rbx
        self.emit(0x5B);

        // pop rbp
        self.emit(0x5D);
        // ret
        self.emit(0xC3);
    }

    /// Emit mov reg64, imm64
    fn emit_mov64_imm(&mut self, dst: u8, imm: i64) {
        let x86_dst = x86_regs::bpf_to_x86(dst);

        if imm >= i32::MIN as i64 && imm <= i32::MAX as i64 {
            // Can use 32-bit sign-extended move
            self.emit_rex_w(0, x86_dst);
            self.emit(0xC7);
            self.emit_modrm(0b11, 0, x86_dst);
            self.emit_imm32(imm as i32);
        } else {
            // Need full 64-bit move
            self.emit_rex_w(0, x86_dst);
            self.emit(0xB8 + x86_regs::reg_code(x86_dst));
            self.emit_imm64(imm);
        }
    }

    /// Emit mov reg64, reg64
    fn emit_mov64_reg(&mut self, dst: u8, src: u8) {
        let x86_dst = x86_regs::bpf_to_x86(dst);
        let x86_src = x86_regs::bpf_to_x86(src);

        self.emit_rex_w(x86_src, x86_dst);
        self.emit(0x89);
        self.emit_modrm(0b11, x86_src, x86_dst);
    }

    /// Emit ALU operation: dst op= src
    fn emit_alu64_reg(&mut self, op: u8, dst: u8, src: u8) {
        let x86_dst = x86_regs::bpf_to_x86(dst);
        let x86_src = x86_regs::bpf_to_x86(src);

        self.emit_rex_w(x86_src, x86_dst);

        match op {
            BPF_ADD => {
                self.emit(0x01);
                self.emit_modrm(0b11, x86_src, x86_dst);
            }
            BPF_SUB => {
                self.emit(0x29);
                self.emit_modrm(0b11, x86_src, x86_dst);
            }
            BPF_AND => {
                self.emit(0x21);
                self.emit_modrm(0b11, x86_src, x86_dst);
            }
            BPF_OR => {
                self.emit(0x09);
                self.emit_modrm(0b11, x86_src, x86_dst);
            }
            BPF_XOR => {
                self.emit(0x31);
                self.emit_modrm(0b11, x86_src, x86_dst);
            }
            _ => {}
        }
    }

    /// Emit ALU operation: dst op= imm
    fn emit_alu64_imm(&mut self, op: u8, dst: u8, imm: i32) {
        let x86_dst = x86_regs::bpf_to_x86(dst);

        self.emit_rex_w(0, x86_dst);

        if imm >= -128 && imm <= 127 {
            // Use short form
            self.emit(0x83);
            let opcode = match op {
                BPF_ADD => 0,
                BPF_SUB => 5,
                BPF_AND => 4,
                BPF_OR => 1,
                BPF_XOR => 6,
                _ => 0,
            };
            self.emit_modrm(0b11, opcode, x86_dst);
            self.emit(imm as u8);
        } else {
            self.emit(0x81);
            let opcode = match op {
                BPF_ADD => 0,
                BPF_SUB => 5,
                BPF_AND => 4,
                BPF_OR => 1,
                BPF_XOR => 6,
                _ => 0,
            };
            self.emit_modrm(0b11, opcode, x86_dst);
            self.emit_imm32(imm);
        }
    }

    /// Emit conditional jump
    fn emit_jcc(&mut self, op: u8, target_pc: usize, _current_pc: usize) {
        // Record for fixup
        let code_offset = self.pos();

        // Emit placeholder (6 bytes: 0F 8x + 4-byte offset)
        let cc = match op {
            BPF_JEQ => 0x84, // JE
            BPF_JNE => 0x85, // JNE
            BPF_JGT => 0x87, // JA (unsigned >)
            BPF_JGE => 0x83, // JAE (unsigned >=)
            BPF_JSET => 0x85, // JNE (after TEST)
            _ => 0x84,
        };

        self.emit(0x0F);
        self.emit(cc);
        self.emit_imm32(0); // Placeholder

        self.pending_jumps.push((code_offset, target_pc, 0));
    }

    /// Fix up pending jumps
    fn fixup_jumps(&mut self) {
        for (code_offset, target_pc, _) in &self.pending_jumps {
            let target_offset = self.offsets[*target_pc];
            let rel32 = (target_offset as i32) - (*code_offset as i32) - 6;

            // Write the relative offset
            let bytes = rel32.to_le_bytes();
            self.code[code_offset + 2] = bytes[0];
            self.code[code_offset + 3] = bytes[1];
            self.code[code_offset + 4] = bytes[2];
            self.code[code_offset + 5] = bytes[3];
        }
    }
}

/// BPF JIT compiler
pub struct BpfJit;

impl BpfJit {
    /// Compile BPF program to native code
    pub fn compile(insns: &[BpfInsn]) -> Result<CompiledProgram, BpfError> {
        if !is_jit_enabled() {
            return Err(BpfError::JitFailed);
        }

        let mut ctx = JitContext::new(insns.len());

        // Emit prologue
        ctx.emit_prologue();

        // Compile each instruction
        for (pc, insn) in insns.iter().enumerate() {
            ctx.offsets[pc] = ctx.pos();
            Self::compile_insn(&mut ctx, insn, pc)?;
        }

        ctx.offsets[insns.len()] = ctx.pos();

        // Fix up jumps
        ctx.fixup_jumps();

        Ok(CompiledProgram {
            code: ctx.code,
        })
    }

    /// Compile a single instruction
    fn compile_insn(ctx: &mut JitContext, insn: &BpfInsn, _pc: usize) -> Result<(), BpfError> {
        let class = insn.code & 0x07;
        let dst = insn.dst();
        let src = insn.src();

        match class {
            0x07 => {
                // BPF_ALU64
                let op = insn.code & 0xF0;
                if (insn.code & 0x08) != 0 {
                    // Register source
                    if op == BPF_MOV {
                        ctx.emit_mov64_reg(dst, src);
                    } else {
                        ctx.emit_alu64_reg(op, dst, src);
                    }
                } else {
                    // Immediate source
                    if op == BPF_MOV {
                        ctx.emit_mov64_imm(dst, insn.imm as i64);
                    } else {
                        ctx.emit_alu64_imm(op, dst, insn.imm);
                    }
                }
            }
            0x05 => {
                // BPF_JMP
                let op = insn.code & 0xF0;

                if op == BPF_EXIT {
                    ctx.emit_epilogue();
                } else if op == BPF_CALL {
                    // Would emit helper call
                    // For now, just clear RAX
                    ctx.emit_rex_w(x86_regs::RAX, x86_regs::RAX);
                    ctx.emit(0x31);
                    ctx.emit_modrm(0b11, x86_regs::RAX, x86_regs::RAX);
                }
                // Would handle conditional jumps
            }
            _ => {
                // Other instructions - would emit corresponding code
            }
        }

        Ok(())
    }
}

/// Compiled BPF program
pub struct CompiledProgram {
    /// Native machine code
    pub code: Vec<u8>,
}

impl CompiledProgram {
    /// Get code size
    pub fn size(&self) -> usize {
        self.code.len()
    }

    /// Execute the compiled program
    ///
    /// # Safety
    /// The code must be properly compiled and placed in executable memory.
    pub unsafe fn execute(&self, ctx: u64) -> u64 {
        // Would copy to executable memory and call
        // For now, return 0
        let _ = ctx;
        0
    }
}
