// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! A small x86-64 assembler with labels, plus the runtime routines a native
//! executable needs (write, exit, integer and bool printing), emitted as
//! machine code. No outside assembler, linker or C library is involved.

/// Register numbers as the hardware encodes them.
pub const RAX: u8 = 0;
pub const RCX: u8 = 1;
pub const RDX: u8 = 2;
pub const RBP: u8 = 5;
pub const RSI: u8 = 6;
pub const RDI: u8 = 7;
pub const R8: u8 = 8;
pub const R9: u8 = 9;

/// The operating system an executable is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExeOs {
    /// Windows x64, PE32+ container, Microsoft x64 calling convention.
    Windows,
    /// Linux x86-64, ELF64 container, System V calling convention.
    Linux,
}

impl ExeOs {
    /// Integer argument registers in order.
    pub fn arg_regs(self) -> &'static [u8] {
        match self {
            ExeOs::Windows => &[RCX, RDX, R8, R9],
            ExeOs::Linux => &[RDI, RSI, RDX, RCX, R8, R9],
        }
    }
}

/// Indices into the PE import table (kernel32).
pub const IMP_GET_STD_HANDLE: usize = 0;
pub const IMP_WRITE_FILE: usize = 1;
pub const IMP_EXIT_PROCESS: usize = 2;
/// Names of the kernel32 imports, in index order.
pub const KERNEL32_IMPORTS: [&str; 3] = ["GetStdHandle", "WriteFile", "ExitProcess"];

enum Fix {
    Label(usize),
    Import(usize),
}

/// Code and read-only data with labels; data is placed after the code.
#[derive(Default)]
pub struct Asm {
    code: Vec<u8>,
    labels: Vec<Option<usize>>,
    fixups: Vec<(usize, Fix)>,
    data: Vec<(usize, Vec<u8>)>,
}

/// Assembled bytes; `imports` lists (offset of rel32, import index) still to patch.
pub struct Linked {
    pub bytes: Vec<u8>,
    pub entry: usize,
    pub imports: Vec<(usize, usize)>,
}

fn rex(w: bool, reg: u8, rm: u8) -> u8 {
    0x40 | ((w as u8) << 3) | (((reg >> 3) & 1) << 2) | ((rm >> 3) & 1)
}

impl Asm {
    pub fn label(&mut self) -> usize {
        self.labels.push(None);
        self.labels.len() - 1
    }
    pub fn bind(&mut self, l: usize) {
        self.labels[l] = Some(self.code.len());
    }
    pub fn b(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }
    fn rel32(&mut self, f: Fix) {
        self.fixups.push((self.code.len(), f));
        self.b(&[0; 4]);
    }
    /// Intern read-only bytes; returns a label for their address.
    pub fn data(&mut self, bytes: &[u8]) -> usize {
        let l = self.label();
        self.data.push((l, bytes.to_vec()));
        l
    }
    /// mov dst, src (64-bit).
    pub fn mov_rr(&mut self, dst: u8, src: u8) {
        self.b(&[
            rex(true, src, dst),
            0x89,
            0xC0 | ((src & 7) << 3) | (dst & 7),
        ]);
    }
    fn rbp_mem(&mut self, op: u8, reg: u8, disp: i32) {
        self.b(&[rex(true, reg, RBP), op, 0x80 | ((reg & 7) << 3) | 5]);
        self.b(&disp.to_le_bytes());
    }
    /// mov reg, [rbp + disp]
    pub fn load(&mut self, reg: u8, disp: i32) {
        self.rbp_mem(0x8B, reg, disp);
    }
    /// mov [rbp + disp], reg
    pub fn store(&mut self, reg: u8, disp: i32) {
        self.rbp_mem(0x89, reg, disp);
    }
    /// lea reg, [rbp + disp]
    pub fn lea_rbp(&mut self, reg: u8, disp: i32) {
        self.rbp_mem(0x8D, reg, disp);
    }
    /// mov reg, imm64
    pub fn mov_imm(&mut self, reg: u8, v: u64) {
        self.b(&[rex(true, 0, reg), 0xB8 + (reg & 7)]);
        self.b(&v.to_le_bytes());
    }
    /// lea reg, [rip + label]
    pub fn lea_rip(&mut self, reg: u8, l: usize) {
        self.b(&[rex(true, reg, 0), 0x8D, ((reg & 7) << 3) | 5]);
        self.rel32(Fix::Label(l));
    }
    pub fn call(&mut self, l: usize) {
        self.b(&[0xE8]);
        self.rel32(Fix::Label(l));
    }
    /// call [rip + IAT slot]
    pub fn call_import(&mut self, idx: usize) {
        self.b(&[0xFF, 0x15]);
        self.rel32(Fix::Import(idx));
    }
    pub fn jmp(&mut self, l: usize) {
        self.b(&[0xE9]);
        self.rel32(Fix::Label(l));
    }
    /// Conditional jump; `cc` is the second opcode byte (0x84 = je, 0x85 = jne, ...).
    pub fn jcc(&mut self, cc: u8, l: usize) {
        self.b(&[0x0F, cc]);
        self.rel32(Fix::Label(l));
    }
    /// test reg, reg
    pub fn test(&mut self, reg: u8) {
        self.b(&[
            rex(true, reg, reg),
            0x85,
            0xC0 | ((reg & 7) << 3) | (reg & 7),
        ]);
    }
    pub fn prologue(&mut self, frame: u32) {
        self.b(&[0x55, 0x48, 0x89, 0xE5, 0x48, 0x81, 0xEC]);
        self.b(&frame.to_le_bytes());
    }
    pub fn leave_ret(&mut self) {
        self.b(&[0xC9, 0xC3]);
    }

    /// Place data after code, resolve label fixups.
    pub fn finish(mut self, entry: usize) -> Result<Linked, String> {
        while self.code.len() % 16 != 0 {
            self.code.push(0xCC);
        }
        for (l, bytes) in std::mem::take(&mut self.data) {
            self.bind(l);
            self.code.extend_from_slice(&bytes);
        }
        let mut imports = Vec::new();
        for (pos, fix) in &self.fixups {
            match fix {
                Fix::Import(i) => imports.push((*pos, *i)),
                Fix::Label(l) => {
                    let at = self.labels[*l].ok_or_else(|| format!("unbound label {l}"))?;
                    let rel = at as i64 - (*pos as i64 + 4);
                    self.code[*pos..*pos + 4].copy_from_slice(&(rel as i32).to_le_bytes());
                }
            }
        }
        let entry = self.labels[entry].ok_or("unbound entry label")?;
        Ok(Linked {
            bytes: self.code,
            entry,
            imports,
        })
    }
}

/// Labels of the emitted runtime routines. Each takes its arguments in the
/// platform's argument registers.
pub struct Runtime {
    pub write: usize,
    pub exit: usize,
    pub print_i64: usize,
    pub print_u64: usize,
    pub print_bool: usize,
}

/// Emit write(ptr, len), exit(code), print_i64, print_u64 and print_bool.
pub fn emit_runtime(a: &mut Asm, os: ExeOs) -> Runtime {
    let rt = Runtime {
        write: a.label(),
        exit: a.label(),
        print_i64: a.label(),
        print_u64: a.label(),
        print_bool: a.label(),
    };
    let (a0, a1) = (os.arg_regs()[0], os.arg_regs()[1]);
    a.bind(rt.write);
    match os {
        ExeOs::Windows => {
            a.prologue(64);
            a.store(RCX, -8);
            a.store(RDX, -16);
            a.b(&[0xB9, 0xF5, 0xFF, 0xFF, 0xFF]); // mov ecx, -11 (STD_OUTPUT_HANDLE)
            a.call_import(IMP_GET_STD_HANDLE);
            a.mov_rr(RCX, RAX);
            a.load(RDX, -8);
            a.load(R8, -16);
            a.lea_rbp(R9, -24);
            a.b(&[0x48, 0xC7, 0x44, 0x24, 0x20, 0, 0, 0, 0]); // mov qword [rsp+32], 0
            a.call_import(IMP_WRITE_FILE);
            a.leave_ret();
        }
        ExeOs::Linux => {
            a.mov_rr(RDX, RSI);
            a.mov_rr(RSI, RDI);
            a.b(&[0xBF, 1, 0, 0, 0, 0xB8, 1, 0, 0, 0, 0x0F, 0x05, 0xC3]); // fd 1, write, ret
        }
    }
    a.bind(rt.exit);
    match os {
        ExeOs::Windows => {
            a.b(&[0x48, 0x83, 0xE4, 0xF0, 0x48, 0x83, 0xEC, 0x20]); // align, shadow space
            a.call_import(IMP_EXIT_PROCESS);
        }
        ExeOs::Linux => a.b(&[0xB8, 0xE7, 0, 0, 0, 0x0F, 0x05]), // exit_group
    }
    a.b(&[0x0F, 0x0B]); // ud2: never reached

    // print_u64: digits written backwards below rbp, then one write.
    a.bind(rt.print_u64);
    a.prologue(64);
    a.mov_rr(RAX, a0);
    a.b(&[0x4C, 0x8D, 0x45, 0xFF]); // lea r8, [rbp-1]
    a.b(&[0x41, 0xBA, 10, 0, 0, 0]); // mov r10d, 10
    let digit = a.label();
    a.bind(digit);
    a.b(&[0x31, 0xD2, 0x49, 0xF7, 0xF2]); // xor edx, edx; div r10
    a.b(&[0x80, 0xC2, 0x30, 0x41, 0x88, 0x10, 0x49, 0xFF, 0xC8]); // add dl,'0'; mov [r8],dl; dec r8
    a.test(RAX);
    a.jcc(0x85, digit);
    a.b(&[rex(true, a0, R8), 0x8D, 0x40 | ((a0 & 7) << 3), 0x01]); // lea a0, [r8+1]
    a.mov_rr(a1, RBP);
    a.b(&[rex(true, a0, a1), 0x29, 0xC0 | ((a0 & 7) << 3) | (a1 & 7)]); // sub a1, a0
    a.call(rt.write);
    a.leave_ret();

    // print_i64: a minus sign, then the magnitude as unsigned.
    a.bind(rt.print_i64);
    a.prologue(48);
    a.store(a0, -8);
    a.test(a0);
    let positive = a.label();
    a.jcc(0x89, positive); // jns
    a.b(&[0xC6, 0x45, 0xF0, b'-']); // mov byte [rbp-16], '-'
    a.lea_rbp(a0, -16);
    a.mov_imm(a1, 1);
    a.call(rt.write);
    a.load(RAX, -8);
    a.b(&[0x48, 0xF7, 0xD8]); // neg rax
    a.store(RAX, -8);
    a.bind(positive);
    a.load(a0, -8);
    a.call(rt.print_u64);
    a.leave_ret();

    // print_bool
    let (t, f) = (a.data(b"true"), a.data(b"false"));
    a.bind(rt.print_bool);
    a.prologue(32);
    a.test(a0);
    let is_false = a.label();
    a.jcc(0x84, is_false);
    a.lea_rip(a0, t);
    a.mov_imm(a1, 4);
    a.call(rt.write);
    a.leave_ret();
    a.bind(is_false);
    a.lea_rip(a0, f);
    a.mov_imm(a1, 5);
    a.call(rt.write);
    a.leave_ret();
    rt
}
