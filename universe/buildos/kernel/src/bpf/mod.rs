// ===============================================================================
// BUILDOS KERNEL - eBPF SUBSYSTEM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Extended Berkeley Packet Filter (eBPF) Subsystem
//!
//! Provides a programmable, sandboxed virtual machine for running
//! verified programs in the kernel context. Used for:
//! - Network packet filtering and manipulation
//! - Tracing and profiling
//! - Security policies
//! - Performance monitoring

pub mod maps;
pub mod verifier;
pub mod jit;
pub mod helpers;
pub mod programs;

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::sync::RwLock;

pub use maps::{BpfMap, BpfMapType, BpfMapDef};
pub use programs::{BpfProgram, BpfProgType, AttachType};
pub use verifier::BpfVerifier;

// =============================================================================
// BPF INSTRUCTION
// =============================================================================

/// BPF instruction (64-bit)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BpfInsn {
    /// Operation code
    pub code: u8,
    /// Registers (dst:4, src:4)
    pub regs: u8,
    /// Signed offset
    pub off: i16,
    /// Immediate value
    pub imm: i32,
}

impl BpfInsn {
    /// Create new instruction
    pub const fn new(code: u8, dst: u8, src: u8, off: i16, imm: i32) -> Self {
        Self {
            code,
            regs: (src << 4) | (dst & 0x0F),
            off,
            imm,
        }
    }

    /// Get destination register
    pub fn dst(&self) -> u8 {
        self.regs & 0x0F
    }

    /// Get source register
    pub fn src(&self) -> u8 {
        (self.regs >> 4) & 0x0F
    }

    /// Move immediate to register
    pub const fn mov64_imm(dst: u8, imm: i32) -> Self {
        Self::new(BPF_ALU64 | BPF_MOV | BPF_K, dst, 0, 0, imm)
    }

    /// Move register to register
    pub const fn mov64_reg(dst: u8, src: u8) -> Self {
        Self::new(BPF_ALU64 | BPF_MOV | BPF_X, dst, src, 0, 0)
    }

    /// Add immediate
    pub const fn add64_imm(dst: u8, imm: i32) -> Self {
        Self::new(BPF_ALU64 | BPF_ADD | BPF_K, dst, 0, 0, imm)
    }

    /// Add register
    pub const fn add64_reg(dst: u8, src: u8) -> Self {
        Self::new(BPF_ALU64 | BPF_ADD | BPF_X, dst, src, 0, 0)
    }

    /// Subtract immediate
    pub const fn sub64_imm(dst: u8, imm: i32) -> Self {
        Self::new(BPF_ALU64 | BPF_SUB | BPF_K, dst, 0, 0, imm)
    }

    /// Load from memory (64-bit)
    pub const fn ldx_mem(size: u8, dst: u8, src: u8, off: i16) -> Self {
        Self::new(BPF_LDX | size | BPF_MEM, dst, src, off, 0)
    }

    /// Store to memory (64-bit)
    pub const fn stx_mem(size: u8, dst: u8, src: u8, off: i16) -> Self {
        Self::new(BPF_STX | size | BPF_MEM, dst, src, off, 0)
    }

    /// Store immediate to memory
    pub const fn st_mem(size: u8, dst: u8, off: i16, imm: i32) -> Self {
        Self::new(BPF_ST | size | BPF_MEM, dst, 0, off, imm)
    }

    /// Jump if equal (immediate)
    pub const fn jeq_imm(dst: u8, imm: i32, off: i16) -> Self {
        Self::new(BPF_JMP | BPF_JEQ | BPF_K, dst, 0, off, imm)
    }

    /// Jump if not equal (immediate)
    pub const fn jne_imm(dst: u8, imm: i32, off: i16) -> Self {
        Self::new(BPF_JMP | BPF_JNE | BPF_K, dst, 0, off, imm)
    }

    /// Jump if greater than (immediate)
    pub const fn jgt_imm(dst: u8, imm: i32, off: i16) -> Self {
        Self::new(BPF_JMP | BPF_JGT | BPF_K, dst, 0, off, imm)
    }

    /// Jump if greater or equal (immediate)
    pub const fn jge_imm(dst: u8, imm: i32, off: i16) -> Self {
        Self::new(BPF_JMP | BPF_JGE | BPF_K, dst, 0, off, imm)
    }

    /// Unconditional jump
    pub const fn ja(off: i16) -> Self {
        Self::new(BPF_JMP | BPF_JA, 0, 0, off, 0)
    }

    /// Call helper function
    pub const fn call(helper: i32) -> Self {
        Self::new(BPF_JMP | BPF_CALL, 0, 0, 0, helper)
    }

    /// Return
    pub const fn exit() -> Self {
        Self::new(BPF_JMP | BPF_EXIT, 0, 0, 0, 0)
    }
}

// =============================================================================
// BPF OPCODES
// =============================================================================

// Instruction classes
pub const BPF_LD: u8 = 0x00;
pub const BPF_LDX: u8 = 0x01;
pub const BPF_ST: u8 = 0x02;
pub const BPF_STX: u8 = 0x03;
pub const BPF_ALU: u8 = 0x04;
pub const BPF_JMP: u8 = 0x05;
pub const BPF_JMP32: u8 = 0x06;
pub const BPF_ALU64: u8 = 0x07;

// Sizes
pub const BPF_W: u8 = 0x00;   // 32-bit
pub const BPF_H: u8 = 0x08;   // 16-bit
pub const BPF_B: u8 = 0x10;   // 8-bit
pub const BPF_DW: u8 = 0x18;  // 64-bit

// Modes
pub const BPF_IMM: u8 = 0x00;
pub const BPF_ABS: u8 = 0x20;
pub const BPF_IND: u8 = 0x40;
pub const BPF_MEM: u8 = 0x60;
pub const BPF_ATOMIC: u8 = 0xC0;

// Source
pub const BPF_K: u8 = 0x00;   // Immediate
pub const BPF_X: u8 = 0x08;   // Register

// ALU operations
pub const BPF_ADD: u8 = 0x00;
pub const BPF_SUB: u8 = 0x10;
pub const BPF_MUL: u8 = 0x20;
pub const BPF_DIV: u8 = 0x30;
pub const BPF_OR: u8 = 0x40;
pub const BPF_AND: u8 = 0x50;
pub const BPF_LSH: u8 = 0x60;
pub const BPF_RSH: u8 = 0x70;
pub const BPF_NEG: u8 = 0x80;
pub const BPF_MOD: u8 = 0x90;
pub const BPF_XOR: u8 = 0xA0;
pub const BPF_MOV: u8 = 0xB0;
pub const BPF_ARSH: u8 = 0xC0;
pub const BPF_END: u8 = 0xD0;

// Jump operations
pub const BPF_JA: u8 = 0x00;
pub const BPF_JEQ: u8 = 0x10;
pub const BPF_JGT: u8 = 0x20;
pub const BPF_JGE: u8 = 0x30;
pub const BPF_JSET: u8 = 0x40;
pub const BPF_JNE: u8 = 0x50;
pub const BPF_JSGT: u8 = 0x60;
pub const BPF_JSGE: u8 = 0x70;
pub const BPF_CALL: u8 = 0x80;
pub const BPF_EXIT: u8 = 0x90;
pub const BPF_JLT: u8 = 0xA0;
pub const BPF_JLE: u8 = 0xB0;
pub const BPF_JSLT: u8 = 0xC0;
pub const BPF_JSLE: u8 = 0xD0;

// =============================================================================
// BPF REGISTERS
// =============================================================================

/// BPF register count
pub const BPF_REG_COUNT: usize = 11;

/// BPF registers
pub mod regs {
    /// Return value (R0)
    pub const BPF_REG_0: u8 = 0;
    /// Argument 1 / scratch (R1)
    pub const BPF_REG_1: u8 = 1;
    /// Argument 2 / scratch (R2)
    pub const BPF_REG_2: u8 = 2;
    /// Argument 3 / scratch (R3)
    pub const BPF_REG_3: u8 = 3;
    /// Argument 4 / scratch (R4)
    pub const BPF_REG_4: u8 = 4;
    /// Argument 5 / scratch (R5)
    pub const BPF_REG_5: u8 = 5;
    /// Callee saved (R6)
    pub const BPF_REG_6: u8 = 6;
    /// Callee saved (R7)
    pub const BPF_REG_7: u8 = 7;
    /// Callee saved (R8)
    pub const BPF_REG_8: u8 = 8;
    /// Callee saved (R9)
    pub const BPF_REG_9: u8 = 9;
    /// Frame pointer (R10) - read only
    pub const BPF_REG_10: u8 = 10;
    /// Alias for frame pointer
    pub const BPF_REG_FP: u8 = 10;
}

// =============================================================================
// BPF CONTEXT
// =============================================================================

/// BPF execution context
pub struct BpfContext {
    /// Registers
    pub regs: [u64; BPF_REG_COUNT],
    /// Stack (512 bytes)
    pub stack: [u8; 512],
}

impl BpfContext {
    /// Create new context
    pub fn new() -> Self {
        let mut ctx = Self {
            regs: [0; BPF_REG_COUNT],
            stack: [0; 512],
        };
        // R10 points to top of stack
        ctx.regs[10] = ctx.stack.as_ptr() as u64 + 512;
        ctx
    }

    /// Set argument (R1)
    pub fn set_arg(&mut self, arg: u64) {
        self.regs[1] = arg;
    }

    /// Get return value (R0)
    pub fn return_value(&self) -> u64 {
        self.regs[0]
    }
}

impl Default for BpfContext {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// BPF INTERPRETER
// =============================================================================

/// BPF interpreter
pub struct BpfInterpreter {
    /// Maximum instructions to execute
    max_insns: u64,
}

impl BpfInterpreter {
    /// Create new interpreter
    pub fn new() -> Self {
        Self {
            max_insns: 1_000_000,
        }
    }

    /// Run BPF program
    pub fn run(
        &self,
        prog: &[BpfInsn],
        ctx: &mut BpfContext,
    ) -> Result<u64, BpfError> {
        let mut pc: usize = 0;
        let mut insn_count: u64 = 0;

        while pc < prog.len() {
            if insn_count >= self.max_insns {
                return Err(BpfError::TooManyInstructions);
            }
            insn_count += 1;

            let insn = &prog[pc];
            let dst = insn.dst() as usize;
            let src = insn.src() as usize;
            let class = insn.code & 0x07;

            match class {
                0x00 => {
                    // BPF_LD
                    // Load immediate (wide)
                    if pc + 1 >= prog.len() {
                        return Err(BpfError::InvalidProgram);
                    }
                    let imm64 = (insn.imm as u32 as u64) |
                               ((prog[pc + 1].imm as u32 as u64) << 32);
                    ctx.regs[dst] = imm64;
                    pc += 1;
                }
                0x01 => {
                    // BPF_LDX
                    let addr = ctx.regs[src].wrapping_add(insn.off as i64 as u64);
                    let size = insn.code & 0x18;
                    ctx.regs[dst] = self.load_mem(addr, size)?;
                }
                0x02 => {
                    // BPF_ST
                    let addr = ctx.regs[dst].wrapping_add(insn.off as i64 as u64);
                    let size = insn.code & 0x18;
                    self.store_mem(addr, size, insn.imm as u64)?;
                }
                0x03 => {
                    // BPF_STX
                    let addr = ctx.regs[dst].wrapping_add(insn.off as i64 as u64);
                    let size = insn.code & 0x18;
                    self.store_mem(addr, size, ctx.regs[src])?;
                }
                0x04 => {
                    // BPF_ALU (32-bit)
                    let imm = insn.imm as u32 as u64;
                    let src_val = if (insn.code & 0x08) != 0 {
                        ctx.regs[src] as u32 as u64
                    } else {
                        imm
                    };
                    ctx.regs[dst] = (self.alu_op(
                        insn.code,
                        ctx.regs[dst] as u32 as u64,
                        src_val,
                    )? as u32) as u64;
                }
                0x05 => {
                    // BPF_JMP
                    let op = insn.code & 0xF0;

                    if op == BPF_EXIT {
                        return Ok(ctx.regs[0]);
                    }

                    if op == BPF_CALL {
                        ctx.regs[0] = self.call_helper(insn.imm, ctx)?;
                        pc += 1;
                        continue;
                    }

                    let imm = insn.imm as i64 as u64;
                    let src_val = if (insn.code & 0x08) != 0 {
                        ctx.regs[src]
                    } else {
                        imm
                    };

                    let take_branch = self.jmp_op(op, ctx.regs[dst], src_val);

                    if take_branch {
                        pc = (pc as i64 + insn.off as i64 + 1) as usize;
                        if pc >= prog.len() {
                            return Err(BpfError::InvalidProgram);
                        }
                        continue;
                    }
                }
                0x06 => {
                    // BPF_JMP32
                    let op = insn.code & 0xF0;
                    let imm = insn.imm as u32 as u64;
                    let src_val = if (insn.code & 0x08) != 0 {
                        ctx.regs[src] as u32 as u64
                    } else {
                        imm
                    };

                    let take_branch = self.jmp_op(op, ctx.regs[dst] as u32 as u64, src_val);

                    if take_branch {
                        pc = (pc as i64 + insn.off as i64 + 1) as usize;
                        continue;
                    }
                }
                0x07 => {
                    // BPF_ALU64
                    let imm = insn.imm as i64 as u64;
                    let src_val = if (insn.code & 0x08) != 0 {
                        ctx.regs[src]
                    } else {
                        imm
                    };
                    ctx.regs[dst] = self.alu_op(insn.code, ctx.regs[dst], src_val)?;
                }
                _ => return Err(BpfError::InvalidInstruction),
            }

            pc += 1;
        }

        // Fell through without exit
        Err(BpfError::InvalidProgram)
    }

    /// Perform ALU operation
    fn alu_op(&self, code: u8, dst: u64, src: u64) -> Result<u64, BpfError> {
        let op = code & 0xF0;
        Ok(match op {
            BPF_ADD => dst.wrapping_add(src),
            BPF_SUB => dst.wrapping_sub(src),
            BPF_MUL => dst.wrapping_mul(src),
            BPF_DIV => {
                if src == 0 {
                    return Err(BpfError::DivisionByZero);
                }
                dst / src
            }
            BPF_OR => dst | src,
            BPF_AND => dst & src,
            BPF_LSH => dst << (src & 0x3F),
            BPF_RSH => dst >> (src & 0x3F),
            BPF_NEG => (-(dst as i64)) as u64,
            BPF_MOD => {
                if src == 0 {
                    return Err(BpfError::DivisionByZero);
                }
                dst % src
            }
            BPF_XOR => dst ^ src,
            BPF_MOV => src,
            BPF_ARSH => ((dst as i64) >> (src & 0x3F)) as u64,
            _ => return Err(BpfError::InvalidInstruction),
        })
    }

    /// Evaluate jump condition
    fn jmp_op(&self, op: u8, dst: u64, src: u64) -> bool {
        match op {
            BPF_JA => true,
            BPF_JEQ => dst == src,
            BPF_JGT => dst > src,
            BPF_JGE => dst >= src,
            BPF_JSET => (dst & src) != 0,
            BPF_JNE => dst != src,
            BPF_JSGT => (dst as i64) > (src as i64),
            BPF_JSGE => (dst as i64) >= (src as i64),
            BPF_JLT => dst < src,
            BPF_JLE => dst <= src,
            BPF_JSLT => (dst as i64) < (src as i64),
            BPF_JSLE => (dst as i64) <= (src as i64),
            _ => false,
        }
    }

    /// Load from memory
    fn load_mem(&self, addr: u64, size: u8) -> Result<u64, BpfError> {
        // Would validate memory access
        // For now, just perform the load unsafely
        unsafe {
            Ok(match size {
                BPF_B => *(addr as *const u8) as u64,
                BPF_H => *(addr as *const u16) as u64,
                BPF_W => *(addr as *const u32) as u64,
                BPF_DW => *(addr as *const u64),
                _ => return Err(BpfError::InvalidInstruction),
            })
        }
    }

    /// Store to memory
    fn store_mem(&self, addr: u64, size: u8, val: u64) -> Result<(), BpfError> {
        // Would validate memory access
        unsafe {
            match size {
                BPF_B => *(addr as *mut u8) = val as u8,
                BPF_H => *(addr as *mut u16) = val as u16,
                BPF_W => *(addr as *mut u32) = val as u32,
                BPF_DW => *(addr as *mut u64) = val,
                _ => return Err(BpfError::InvalidInstruction),
            }
        }
        Ok(())
    }

    /// Call helper function
    fn call_helper(&self, helper_id: i32, ctx: &mut BpfContext) -> Result<u64, BpfError> {
        helpers::call_helper(
            helper_id,
            ctx.regs[1],
            ctx.regs[2],
            ctx.regs[3],
            ctx.regs[4],
            ctx.regs[5],
        )
    }
}

impl Default for BpfInterpreter {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// BPF ERROR
// =============================================================================

/// BPF error
#[derive(Clone, Debug)]
pub enum BpfError {
    /// Invalid program
    InvalidProgram,
    /// Invalid instruction
    InvalidInstruction,
    /// Division by zero
    DivisionByZero,
    /// Out of bounds access
    OutOfBounds,
    /// Too many instructions
    TooManyInstructions,
    /// Invalid helper call
    InvalidHelper,
    /// Permission denied
    PermissionDenied,
    /// Map not found
    MapNotFound,
    /// Key not found
    KeyNotFound,
    /// Map full
    MapFull,
    /// Invalid map type
    InvalidMapType,
    /// Program too large
    ProgramTooLarge,
    /// Verification failed
    VerificationFailed,
    /// JIT compilation failed
    JitFailed,
}

impl BpfError {
    /// Convert to errno
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::InvalidProgram => -22,      // EINVAL
            Self::InvalidInstruction => -22,  // EINVAL
            Self::DivisionByZero => -22,      // EINVAL
            Self::OutOfBounds => -14,         // EFAULT
            Self::TooManyInstructions => -7,  // E2BIG
            Self::InvalidHelper => -22,       // EINVAL
            Self::PermissionDenied => -1,     // EPERM
            Self::MapNotFound => -2,          // ENOENT
            Self::KeyNotFound => -2,          // ENOENT
            Self::MapFull => -7,              // E2BIG
            Self::InvalidMapType => -22,      // EINVAL
            Self::ProgramTooLarge => -7,      // E2BIG
            Self::VerificationFailed => -22,  // EINVAL
            Self::JitFailed => -12,           // ENOMEM
        }
    }
}

// =============================================================================
// GLOBAL STATE
// =============================================================================

/// Loaded BPF programs
static BPF_PROGRAMS: RwLock<BTreeMap<u32, Arc<BpfProgram>>> = RwLock::new(BTreeMap::new());

/// BPF maps
static BPF_MAPS: RwLock<BTreeMap<u32, Arc<BpfMap>>> = RwLock::new(BTreeMap::new());

/// Next program ID
static NEXT_PROG_ID: AtomicU32 = AtomicU32::new(1);

/// Next map ID
static NEXT_MAP_ID: AtomicU32 = AtomicU32::new(1);

// =============================================================================
// SYSCALL IMPLEMENTATIONS
// =============================================================================

/// BPF command
#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum BpfCmd {
    /// Create a map
    MapCreate = 0,
    /// Look up element in map
    MapLookupElem = 1,
    /// Update element in map
    MapUpdateElem = 2,
    /// Delete element from map
    MapDeleteElem = 3,
    /// Get next key
    MapGetNextKey = 4,
    /// Load a program
    ProgLoad = 5,
    /// Attach a program
    ProgAttach = 6,
    /// Detach a program
    ProgDetach = 7,
    /// Test run a program
    ProgTestRun = 8,
    /// Get next program ID
    ProgGetNextId = 9,
    /// Get next map ID
    MapGetNextId = 10,
    /// Get program by ID
    ProgGetFdById = 11,
    /// Get map by ID
    MapGetFdById = 12,
    /// Get object info
    ObjGetInfoByFd = 13,
    /// Query program
    ProgQuery = 14,
    /// Raw tracepoint open
    RawTracepointOpen = 15,
    /// BTF load
    BtfLoad = 16,
    /// BTF get fd by ID
    BtfGetFdById = 17,
    /// Task fd query
    TaskFdQuery = 18,
    /// Map freeze
    MapFreeze = 19,
    /// Pin object
    ObjPin = 20,
    /// Get object
    ObjGet = 21,
    /// Link create
    LinkCreate = 22,
    /// Link update
    LinkUpdate = 23,
    /// Link get FD by ID
    LinkGetFdById = 24,
    /// Link get next ID
    LinkGetNextId = 25,
    /// Enable stats
    EnableStats = 26,
    /// Iter create
    IterCreate = 27,
    /// Link detach
    LinkDetach = 28,
    /// Prog bind map
    ProgBindMap = 29,
}

/// bpf() syscall
pub fn sys_bpf(cmd: u32, attr: u64, size: u32) -> Result<i32, BpfError> {
    let _ = (attr, size);

    match cmd {
        0 => {
            // BPF_MAP_CREATE
            let map_id = NEXT_MAP_ID.fetch_add(1, Ordering::Relaxed);
            let map = Arc::new(BpfMap::new(
                map_id,
                BpfMapType::Hash,
                8,    // key_size
                8,    // value_size
                1024, // max_entries
            ));
            BPF_MAPS.write().insert(map_id, map);
            crate::kprintln!("[BPF] Created map {}", map_id);
            Ok(map_id as i32)
        }
        5 => {
            // BPF_PROG_LOAD
            let prog_id = NEXT_PROG_ID.fetch_add(1, Ordering::Relaxed);
            let prog = Arc::new(BpfProgram::new(
                prog_id,
                BpfProgType::SocketFilter,
                Vec::new(),
            ));
            BPF_PROGRAMS.write().insert(prog_id, prog);
            crate::kprintln!("[BPF] Loaded program {}", prog_id);
            Ok(prog_id as i32)
        }
        _ => Err(BpfError::InvalidProgram),
    }
}

/// Initialize BPF subsystem
pub fn init() {
    crate::kprintln!("[BPF] eBPF subsystem initialized");
}
