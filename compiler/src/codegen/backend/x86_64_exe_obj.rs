// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) 2022-2026 Zain Dana Harper.

//! Executable containers written byte by byte: PE32+ for Windows x64 and
//! ELF64 for Linux x86-64. Each takes assembled code (with its data after
//! it) and returns a file the operating system loads directly.

use super::x86_64_exe_asm::{Linked, KERNEL32_IMPORTS};

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn put64(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn align(x: u32, a: u32) -> u32 {
    x.div_ceil(a) * a
}
fn pad_to(v: &mut Vec<u8>, len: usize) {
    v.resize(len, 0);
}

const IMAGE_BASE: u64 = 0x1_4000_0000;
const SECT_ALIGN: u32 = 0x1000;
const FILE_ALIGN: u32 = 0x200;
const TEXT_RVA: u32 = 0x1000;

/// The `.idata` section: one descriptor for kernel32, lookup and address
/// tables, hint/name entries and the DLL name. Returns (bytes, IAT offset).
fn idata(rva: u32) -> (Vec<u8>, u32) {
    let n = KERNEL32_IMPORTS.len() as u32;
    let ilt = 40; // descriptor + null descriptor
    let iat = ilt + 8 * (n + 1);
    let mut names = Vec::new();
    let mut name_rvas = Vec::new();
    let names_at = iat + 8 * (n + 1);
    for imp in KERNEL32_IMPORTS {
        name_rvas.push(rva + names_at + names.len() as u32);
        put16(&mut names, 0);
        names.extend_from_slice(imp.as_bytes());
        names.push(0);
        if names.len() % 2 == 1 {
            names.push(0);
        }
    }
    let dll_rva = rva + names_at + names.len() as u32;
    names.extend_from_slice(b"KERNEL32.dll\0");
    let mut v = Vec::new();
    put32(&mut v, rva + ilt); // OriginalFirstThunk
    put32(&mut v, 0);
    put32(&mut v, 0);
    put32(&mut v, dll_rva);
    put32(&mut v, rva + iat); // FirstThunk
    pad_to(&mut v, 40);
    for _ in 0..2 {
        for r in &name_rvas {
            put64(&mut v, *r as u64);
        }
        put64(&mut v, 0);
    }
    v.extend_from_slice(&names);
    (v, iat)
}

fn section_header(v: &mut Vec<u8>, name: &[u8], vsize: u32, rva: u32, raw: u32, off: u32, ch: u32) {
    let mut n = name.to_vec();
    n.resize(8, 0);
    v.extend_from_slice(&n);
    for x in [vsize, rva, raw, off, 0, 0] {
        put32(v, x);
    }
    put32(v, 0); // relocation and line-number counts
    put32(v, ch);
}

/// A Windows x64 console executable (PE32+) importing three kernel32 calls.
pub fn write_pe(linked: &Linked) -> Vec<u8> {
    let mut code = linked.bytes.clone();
    let text_size = code.len() as u32;
    let idata_rva = TEXT_RVA + align(text_size, SECT_ALIGN);
    let (idata_bytes, iat_off) = idata(idata_rva);
    for &(pos, idx) in &linked.imports {
        let slot = idata_rva + iat_off + 8 * idx as u32;
        let rel = slot as i64 - (TEXT_RVA as i64 + pos as i64 + 4);
        code[pos..pos + 4].copy_from_slice(&(rel as i32).to_le_bytes());
    }
    let text_raw = align(text_size, FILE_ALIGN);
    let idata_size = idata_bytes.len() as u32;
    let idata_raw = align(idata_size, FILE_ALIGN);
    let size_of_image = idata_rva + align(idata_size, SECT_ALIGN);

    let mut v = vec![0u8; 0x40];
    v[0] = b'M';
    v[1] = b'Z';
    v[0x3C] = 0x40; // e_lfanew
    v.extend_from_slice(b"PE\0\0");
    put16(&mut v, 0x8664); // AMD64
    put16(&mut v, 2); // sections
    put32(&mut v, 0);
    put32(&mut v, 0);
    put32(&mut v, 0);
    put16(&mut v, 240); // optional header size
    put16(&mut v, 0x0022); // executable, large address aware
    put16(&mut v, 0x20B); // PE32+
    put16(&mut v, 0); // linker version
    put32(&mut v, text_raw);
    put32(&mut v, idata_raw);
    put32(&mut v, 0);
    put32(&mut v, TEXT_RVA + linked.entry as u32);
    put32(&mut v, TEXT_RVA);
    put64(&mut v, IMAGE_BASE);
    put32(&mut v, SECT_ALIGN);
    put32(&mut v, FILE_ALIGN);
    for x in [6u16, 0, 0, 0, 6, 0] {
        put16(&mut v, x); // OS, image and subsystem versions
    }
    put32(&mut v, 0);
    put32(&mut v, size_of_image);
    put32(&mut v, FILE_ALIGN); // size of headers
    put32(&mut v, 0); // checksum
    put16(&mut v, 3); // console subsystem
    put16(&mut v, 0x8100); // NX compatible, terminal-server aware; fixed base
    for x in [0x10_0000u64, 0x1000, 0x10_0000, 0x1000] {
        put64(&mut v, x);
    }
    put32(&mut v, 0);
    put32(&mut v, 16);
    for i in 0..16u32 {
        let (rva, size) = match i {
            1 => (idata_rva, 40),
            12 => (idata_rva + iat_off, 8 * (KERNEL32_IMPORTS.len() as u32 + 1)),
            _ => (0, 0),
        };
        put32(&mut v, rva);
        put32(&mut v, size);
    }
    section_header(
        &mut v,
        b".text",
        text_size,
        TEXT_RVA,
        text_raw,
        FILE_ALIGN,
        0x6000_0020,
    );
    let idata_off = FILE_ALIGN + text_raw;
    section_header(
        &mut v,
        b".idata",
        idata_size,
        idata_rva,
        idata_raw,
        idata_off,
        0xC000_0040,
    );
    pad_to(&mut v, FILE_ALIGN as usize);
    v.extend_from_slice(&code);
    pad_to(&mut v, idata_off as usize);
    v.extend_from_slice(&idata_bytes);
    pad_to(&mut v, (idata_off + idata_raw) as usize);
    v
}

const ELF_BASE: u64 = 0x40_0000;
const ELF_HEADERS: u64 = 64 + 56;

/// A static Linux x86-64 executable (ELF64) with one read+execute segment.
pub fn write_elf(linked: &Linked) -> Vec<u8> {
    let total = ELF_HEADERS + linked.bytes.len() as u64;
    let mut v = Vec::new();
    v.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
    pad_to(&mut v, 16);
    put16(&mut v, 2); // ET_EXEC
    put16(&mut v, 62); // EM_X86_64
    put32(&mut v, 1);
    put64(&mut v, ELF_BASE + ELF_HEADERS + linked.entry as u64);
    put64(&mut v, 64); // program headers follow the ELF header
    put64(&mut v, 0); // no section headers
    put32(&mut v, 0);
    for x in [64u16, 56, 1, 64, 0, 0] {
        put16(&mut v, x);
    }
    put32(&mut v, 1); // PT_LOAD
    put32(&mut v, 5); // R + X
    put64(&mut v, 0);
    put64(&mut v, ELF_BASE);
    put64(&mut v, ELF_BASE);
    put64(&mut v, total);
    put64(&mut v, total);
    put64(&mut v, 0x1000);
    v.extend_from_slice(&linked.bytes);
    v
}
