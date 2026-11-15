// ===============================================================================
// BUILDOS KERNEL - INTERRUPT VIRTUALIZATION
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Interrupt Virtualization
//!
//! Handles virtual interrupt delivery:
//! - Virtual LAPIC
//! - Virtual IOAPIC
//! - PIC emulation
//! - MSI/MSI-X support

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::sync::{Mutex, RwLock};

// =============================================================================
// INTERRUPT TYPES
// =============================================================================

/// Interrupt delivery mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeliveryMode {
    Fixed = 0,
    LowestPriority = 1,
    Smi = 2,
    Nmi = 4,
    Init = 5,
    Startup = 6,
    ExtInt = 7,
}

/// Trigger mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Edge,
    Level,
}

/// Interrupt destination mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestMode {
    Physical,
    Logical,
}

/// Virtual interrupt
#[derive(Debug, Clone)]
pub struct VirtualInterrupt {
    /// Vector
    pub vector: u8,
    /// Delivery mode
    pub delivery_mode: DeliveryMode,
    /// Trigger mode
    pub trigger_mode: TriggerMode,
    /// Destination mode
    pub dest_mode: DestMode,
    /// Destination
    pub dest: u8,
}

// =============================================================================
// VIRTUAL LAPIC
// =============================================================================

/// LAPIC registers
pub mod lapic_reg {
    pub const ID: u32 = 0x020;
    pub const VERSION: u32 = 0x030;
    pub const TPR: u32 = 0x080;
    pub const APR: u32 = 0x090;
    pub const PPR: u32 = 0x0A0;
    pub const EOI: u32 = 0x0B0;
    pub const RRD: u32 = 0x0C0;
    pub const LDR: u32 = 0x0D0;
    pub const DFR: u32 = 0x0E0;
    pub const SVR: u32 = 0x0F0;
    pub const ISR_BASE: u32 = 0x100;
    pub const TMR_BASE: u32 = 0x180;
    pub const IRR_BASE: u32 = 0x200;
    pub const ESR: u32 = 0x280;
    pub const LVT_CMCI: u32 = 0x2F0;
    pub const ICR_LOW: u32 = 0x300;
    pub const ICR_HIGH: u32 = 0x310;
    pub const LVT_TIMER: u32 = 0x320;
    pub const LVT_THERMAL: u32 = 0x330;
    pub const LVT_PERF: u32 = 0x340;
    pub const LVT_LINT0: u32 = 0x350;
    pub const LVT_LINT1: u32 = 0x360;
    pub const LVT_ERROR: u32 = 0x370;
    pub const TIMER_ICR: u32 = 0x380;
    pub const TIMER_CCR: u32 = 0x390;
    pub const TIMER_DCR: u32 = 0x3E0;
    pub const SELF_IPI: u32 = 0x3F0;
}

/// Virtual LAPIC
pub struct VirtualLapic {
    /// LAPIC ID
    id: u8,
    /// Register state (1KB page)
    regs: [u32; 256],
    /// Pending interrupts (256 bits for IRR)
    irr: [AtomicU64; 4],
    /// In-service interrupts
    isr: [AtomicU64; 4],
    /// Trigger mode register
    tmr: [AtomicU64; 4],
    /// Timer enabled
    timer_enabled: AtomicBool,
    /// Timer initial count
    timer_initial: AtomicU32,
    /// Timer current count
    timer_current: AtomicU32,
    /// Timer divide value
    timer_divide: AtomicU32,
    /// Software enabled
    enabled: AtomicBool,
}

impl VirtualLapic {
    /// Create new virtual LAPIC
    pub fn new(id: u8) -> Self {
        let mut regs = [0u32; 256];

        // Set ID
        regs[lapic_reg::ID as usize / 4] = (id as u32) << 24;

        // Set version (APIC version 0x14, max LVT entries)
        regs[lapic_reg::VERSION as usize / 4] = 0x50014;

        // Set DFR to flat model
        regs[lapic_reg::DFR as usize / 4] = 0xFFFFFFFF;

        // Set SVR (software disabled initially)
        regs[lapic_reg::SVR as usize / 4] = 0xFF;

        Self {
            id,
            regs,
            irr: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            isr: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            tmr: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            timer_enabled: AtomicBool::new(false),
            timer_initial: AtomicU32::new(0),
            timer_current: AtomicU32::new(0),
            timer_divide: AtomicU32::new(1),
            enabled: AtomicBool::new(false),
        }
    }

    /// Read register
    pub fn read(&self, offset: u32) -> u32 {
        let index = (offset / 4) as usize;
        if index < self.regs.len() {
            match offset {
                lapic_reg::PPR => self.calculate_ppr(),
                lapic_reg::TIMER_CCR => self.timer_current.load(Ordering::SeqCst),
                _ => self.regs[index],
            }
        } else {
            0
        }
    }

    /// Write register
    pub fn write(&mut self, offset: u32, value: u32) {
        let index = (offset / 4) as usize;

        match offset {
            lapic_reg::EOI => {
                self.handle_eoi();
            }
            lapic_reg::ICR_LOW => {
                self.regs[index] = value;
                self.send_ipi();
            }
            lapic_reg::SVR => {
                self.regs[index] = value;
                self.enabled.store((value & 0x100) != 0, Ordering::SeqCst);
            }
            lapic_reg::LVT_TIMER => {
                self.regs[index] = value;
                self.timer_enabled.store((value & 0x10000) == 0, Ordering::SeqCst);
            }
            lapic_reg::TIMER_ICR => {
                self.timer_initial.store(value, Ordering::SeqCst);
                self.timer_current.store(value, Ordering::SeqCst);
            }
            lapic_reg::TIMER_DCR => {
                self.regs[index] = value;
                let divide = match value & 0xB {
                    0x0 => 2,
                    0x1 => 4,
                    0x2 => 8,
                    0x3 => 16,
                    0x8 => 32,
                    0x9 => 64,
                    0xA => 128,
                    0xB => 1,
                    _ => 1,
                };
                self.timer_divide.store(divide, Ordering::SeqCst);
            }
            lapic_reg::SELF_IPI => {
                let vector = value as u8;
                self.set_irr(vector);
            }
            _ => {
                if index < self.regs.len() {
                    self.regs[index] = value;
                }
            }
        }
    }

    /// Set IRR bit
    pub fn set_irr(&self, vector: u8) {
        let index = (vector / 64) as usize;
        let bit = vector % 64;
        self.irr[index].fetch_or(1 << bit, Ordering::SeqCst);
    }

    /// Clear IRR bit
    pub fn clear_irr(&self, vector: u8) {
        let index = (vector / 64) as usize;
        let bit = vector % 64;
        self.irr[index].fetch_and(!(1 << bit), Ordering::SeqCst);
    }

    /// Set ISR bit
    pub fn set_isr(&self, vector: u8) {
        let index = (vector / 64) as usize;
        let bit = vector % 64;
        self.isr[index].fetch_or(1 << bit, Ordering::SeqCst);
    }

    /// Clear ISR bit
    pub fn clear_isr(&self, vector: u8) {
        let index = (vector / 64) as usize;
        let bit = vector % 64;
        self.isr[index].fetch_and(!(1 << bit), Ordering::SeqCst);
    }

    /// Check if interrupt is pending
    pub fn has_pending_interrupt(&self) -> bool {
        for i in 0..4 {
            if self.irr[i].load(Ordering::SeqCst) != 0 {
                return true;
            }
        }
        false
    }

    /// Get highest priority pending interrupt
    pub fn get_pending_vector(&self) -> Option<u8> {
        for i in (0..4).rev() {
            let irr = self.irr[i].load(Ordering::SeqCst);
            if irr != 0 {
                let bit = 63 - irr.leading_zeros();
                let vector = (i as u8 * 64) + bit as u8;

                // Check against TPR
                let tpr = self.regs[lapic_reg::TPR as usize / 4] as u8;
                if vector > (tpr >> 4) * 16 + 15 {
                    return Some(vector);
                }
            }
        }
        None
    }

    /// Accept interrupt
    pub fn accept_interrupt(&self, vector: u8) {
        self.clear_irr(vector);
        self.set_isr(vector);
    }

    /// Handle EOI
    fn handle_eoi(&self) {
        // Find highest priority in-service interrupt and clear it
        for i in (0..4).rev() {
            let isr = self.isr[i].load(Ordering::SeqCst);
            if isr != 0 {
                let bit = 63 - isr.leading_zeros();
                let vector = (i as u8 * 64) + bit as u8;
                self.clear_isr(vector);
                return;
            }
        }
    }

    /// Send IPI
    fn send_ipi(&self) {
        // Would route IPI to other vCPUs
    }

    /// Calculate PPR
    fn calculate_ppr(&self) -> u32 {
        let tpr = self.regs[lapic_reg::TPR as usize / 4];

        // Find highest priority ISR
        let mut isrv = 0u8;
        for i in (0..4).rev() {
            let isr = self.isr[i].load(Ordering::SeqCst);
            if isr != 0 {
                let bit = 63 - isr.leading_zeros();
                isrv = (i as u8 * 64) + bit as u8;
                break;
            }
        }

        let tpr_priority = (tpr >> 4) & 0xF;
        let isr_priority = (isrv >> 4) as u32;

        if tpr_priority >= isr_priority {
            tpr & 0xFF
        } else {
            (isrv as u32) & 0xF0
        }
    }

    /// Get LAPIC ID
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }
}

// =============================================================================
// VIRTUAL IOAPIC
// =============================================================================

/// IOAPIC redirection entry
#[derive(Debug, Clone, Copy)]
pub struct RedirEntry {
    /// Vector
    pub vector: u8,
    /// Delivery mode
    pub delivery_mode: u8,
    /// Destination mode (0 = physical, 1 = logical)
    pub dest_mode: bool,
    /// Delivery status
    pub delivery_status: bool,
    /// Polarity (0 = high active, 1 = low active)
    pub polarity: bool,
    /// Remote IRR
    pub remote_irr: bool,
    /// Trigger mode (0 = edge, 1 = level)
    pub trigger_mode: bool,
    /// Masked
    pub masked: bool,
    /// Destination
    pub dest: u8,
}

impl Default for RedirEntry {
    fn default() -> Self {
        Self {
            vector: 0,
            delivery_mode: 0,
            dest_mode: false,
            delivery_status: false,
            polarity: false,
            remote_irr: false,
            trigger_mode: false,
            masked: true,
            dest: 0,
        }
    }
}

impl RedirEntry {
    /// Create from raw 64-bit value
    pub fn from_raw(value: u64) -> Self {
        Self {
            vector: (value & 0xFF) as u8,
            delivery_mode: ((value >> 8) & 0x7) as u8,
            dest_mode: ((value >> 11) & 1) != 0,
            delivery_status: ((value >> 12) & 1) != 0,
            polarity: ((value >> 13) & 1) != 0,
            remote_irr: ((value >> 14) & 1) != 0,
            trigger_mode: ((value >> 15) & 1) != 0,
            masked: ((value >> 16) & 1) != 0,
            dest: ((value >> 56) & 0xFF) as u8,
        }
    }

    /// Convert to raw 64-bit value
    pub fn to_raw(&self) -> u64 {
        let mut value = self.vector as u64;
        value |= (self.delivery_mode as u64) << 8;
        value |= (self.dest_mode as u64) << 11;
        value |= (self.delivery_status as u64) << 12;
        value |= (self.polarity as u64) << 13;
        value |= (self.remote_irr as u64) << 14;
        value |= (self.trigger_mode as u64) << 15;
        value |= (self.masked as u64) << 16;
        value |= (self.dest as u64) << 56;
        value
    }
}

/// Virtual IOAPIC
pub struct VirtualIoapic {
    /// IOAPIC ID
    id: u8,
    /// Base address
    base_address: u64,
    /// I/O register select
    ioregsel: AtomicU32,
    /// Redirection table entries (24 max)
    redir_table: RwLock<[RedirEntry; 24]>,
    /// IRQ to GSI mapping
    irq_to_gsi: BTreeMap<u32, u32>,
}

impl VirtualIoapic {
    /// Create new virtual IOAPIC
    pub fn new(id: u8, base_address: u64) -> Self {
        Self {
            id,
            base_address,
            ioregsel: AtomicU32::new(0),
            redir_table: RwLock::new([RedirEntry::default(); 24]),
            irq_to_gsi: BTreeMap::new(),
        }
    }

    /// Read MMIO register
    pub fn read(&self, offset: u64) -> u32 {
        match offset {
            0x00 => self.ioregsel.load(Ordering::SeqCst),
            0x10 => self.read_iowin(),
            _ => 0,
        }
    }

    /// Write MMIO register
    pub fn write(&self, offset: u64, value: u32) {
        match offset {
            0x00 => self.ioregsel.store(value, Ordering::SeqCst),
            0x10 => self.write_iowin(value),
            _ => {}
        }
    }

    /// Read IOWIN register
    fn read_iowin(&self) -> u32 {
        let sel = self.ioregsel.load(Ordering::SeqCst);

        match sel {
            0x00 => (self.id as u32) << 24, // IOAPIC ID
            0x01 => 0x00170011,              // Version (17 entries, version 11)
            0x02 => 0,                       // Arbitration ID
            _ if sel >= 0x10 && sel < 0x40 => {
                let entry_idx = ((sel - 0x10) / 2) as usize;
                let is_high = (sel & 1) != 0;

                if entry_idx < 24 {
                    let table = self.redir_table.read();
                    let raw = table[entry_idx].to_raw();
                    if is_high {
                        (raw >> 32) as u32
                    } else {
                        raw as u32
                    }
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    /// Write IOWIN register
    fn write_iowin(&self, value: u32) {
        let sel = self.ioregsel.load(Ordering::SeqCst);

        if sel >= 0x10 && sel < 0x40 {
            let entry_idx = ((sel - 0x10) / 2) as usize;
            let is_high = (sel & 1) != 0;

            if entry_idx < 24 {
                let mut table = self.redir_table.write();
                let mut raw = table[entry_idx].to_raw();

                if is_high {
                    raw = (raw & 0x00000000FFFFFFFF) | ((value as u64) << 32);
                } else {
                    raw = (raw & 0xFFFFFFFF00000000) | (value as u64);
                }

                table[entry_idx] = RedirEntry::from_raw(raw);
            }
        }
    }

    /// Raise IRQ
    pub fn raise_irq(&self, irq: u8) -> Option<VirtualInterrupt> {
        if irq >= 24 {
            return None;
        }

        let table = self.redir_table.read();
        let entry = &table[irq as usize];

        if entry.masked {
            return None;
        }

        Some(VirtualInterrupt {
            vector: entry.vector,
            delivery_mode: match entry.delivery_mode {
                0 => DeliveryMode::Fixed,
                1 => DeliveryMode::LowestPriority,
                2 => DeliveryMode::Smi,
                4 => DeliveryMode::Nmi,
                5 => DeliveryMode::Init,
                7 => DeliveryMode::ExtInt,
                _ => DeliveryMode::Fixed,
            },
            trigger_mode: if entry.trigger_mode {
                TriggerMode::Level
            } else {
                TriggerMode::Edge
            },
            dest_mode: if entry.dest_mode {
                DestMode::Logical
            } else {
                DestMode::Physical
            },
            dest: entry.dest,
        })
    }

    /// Lower IRQ (for level-triggered)
    pub fn lower_irq(&self, irq: u8) {
        if irq < 24 {
            let mut table = self.redir_table.write();
            table[irq as usize].remote_irr = false;
        }
    }

    /// Get base address
    pub fn base_address(&self) -> u64 {
        self.base_address
    }
}

// =============================================================================
// VIRTUAL PIC (8259)
// =============================================================================

/// Virtual 8259 PIC
pub struct VirtualPic {
    /// IRR (Interrupt Request Register)
    irr: u8,
    /// IMR (Interrupt Mask Register)
    imr: u8,
    /// ISR (In-Service Register)
    isr: u8,
    /// Priority add
    priority_add: u8,
    /// Base vector
    vector_offset: u8,
    /// Is master
    is_master: bool,
    /// ICW state
    icw_state: u8,
    /// Read register select
    read_reg_select: bool,
    /// Poll mode
    poll: bool,
    /// Special mask mode
    special_mask: bool,
    /// Auto EOI mode
    auto_eoi: bool,
    /// Rotate on auto EOI
    rotate_on_auto_eoi: bool,
}

impl VirtualPic {
    /// Create new virtual PIC
    pub fn new(is_master: bool) -> Self {
        Self {
            irr: 0,
            imr: 0xFF, // All masked
            isr: 0,
            priority_add: 0,
            vector_offset: if is_master { 0x08 } else { 0x70 },
            is_master,
            icw_state: 0,
            read_reg_select: false,
            poll: false,
            special_mask: false,
            auto_eoi: false,
            rotate_on_auto_eoi: false,
        }
    }

    /// Read from PIC
    pub fn read(&self, port: u16) -> u8 {
        if port & 1 == 0 {
            // Command port
            if self.read_reg_select {
                self.isr
            } else {
                self.irr
            }
        } else {
            // Data port
            self.imr
        }
    }

    /// Write to PIC
    pub fn write(&mut self, port: u16, value: u8) {
        if port & 1 == 0 {
            // Command port
            if value & 0x10 != 0 {
                // ICW1
                self.icw_state = 1;
                self.imr = 0;
                self.isr = 0;
                self.irr = 0;
                self.priority_add = 0;
                self.auto_eoi = false;
                self.rotate_on_auto_eoi = false;
            } else if value & 0x08 != 0 {
                // OCW3
                if value & 0x02 != 0 {
                    self.read_reg_select = (value & 0x01) != 0;
                }
                if value & 0x04 != 0 {
                    self.poll = true;
                }
                if value & 0x40 != 0 {
                    self.special_mask = (value & 0x20) != 0;
                }
            } else {
                // OCW2
                let cmd = value >> 5;
                match cmd {
                    0 | 4 => {} // Rotate in auto EOI mode
                    1 => {
                        // Non-specific EOI
                        let priority = self.get_priority(self.isr);
                        if priority != 8 {
                            self.isr &= !(1 << ((priority + self.priority_add) & 7));
                        }
                    }
                    2 => {} // No operation
                    3 => {
                        // Specific EOI
                        self.isr &= !(1 << (value & 7));
                    }
                    5 => {
                        // Rotate on non-specific EOI
                        let priority = self.get_priority(self.isr);
                        if priority != 8 {
                            let irq = (priority + self.priority_add) & 7;
                            self.isr &= !(1 << irq);
                            self.priority_add = (irq + 1) & 7;
                        }
                    }
                    6 => {
                        // Set priority command
                        self.priority_add = (value as u8 + 1) & 7;
                    }
                    7 => {
                        // Rotate on specific EOI
                        let irq = value & 7;
                        self.isr &= !(1 << irq);
                        self.priority_add = (irq + 1) & 7;
                    }
                    _ => {}
                }
            }
        } else {
            // Data port
            match self.icw_state {
                0 => {
                    // OCW1 - mask
                    self.imr = value;
                }
                1 => {
                    // ICW2 - vector offset
                    self.vector_offset = value & 0xF8;
                    self.icw_state = 2;
                }
                2 => {
                    // ICW3
                    self.icw_state = 3;
                }
                3 => {
                    // ICW4
                    self.auto_eoi = (value & 0x02) != 0;
                    self.icw_state = 0;
                }
                _ => {}
            }
        }
    }

    /// Get priority of highest set bit
    fn get_priority(&self, mask: u8) -> u8 {
        if mask == 0 {
            return 8;
        }

        for i in 0..8 {
            let priority = (i + self.priority_add) & 7;
            if mask & (1 << priority) != 0 {
                return i;
            }
        }
        8
    }

    /// Set IRQ
    pub fn set_irq(&mut self, irq: u8, level: bool) {
        if irq >= 8 {
            return;
        }

        let mask = 1 << irq;
        if level {
            self.irr |= mask;
        } else {
            self.irr &= !mask;
        }
    }

    /// Get interrupt to inject
    pub fn get_interrupt(&mut self) -> Option<u8> {
        let priority = self.get_priority(self.irr & !self.imr);
        if priority == 8 {
            return None;
        }

        let irq = (priority + self.priority_add) & 7;
        let mask = 1 << irq;

        self.irr &= !mask;
        self.isr |= mask;

        if self.auto_eoi {
            self.isr &= !mask;
            if self.rotate_on_auto_eoi {
                self.priority_add = (irq + 1) & 7;
            }
        }

        Some(self.vector_offset + irq)
    }
}

// =============================================================================
// INTERRUPT CONTROLLER
// =============================================================================

/// Virtual interrupt controller
pub struct InterruptController {
    /// LAPICs (one per vCPU)
    lapics: RwLock<Vec<Arc<Mutex<VirtualLapic>>>>,
    /// IOAPIC
    ioapic: Mutex<VirtualIoapic>,
    /// Master PIC
    pic_master: Mutex<VirtualPic>,
    /// Slave PIC
    pic_slave: Mutex<VirtualPic>,
    /// Use APIC mode
    apic_mode: AtomicBool,
}

impl InterruptController {
    /// Create new interrupt controller
    pub fn new() -> Self {
        Self {
            lapics: RwLock::new(Vec::new()),
            ioapic: Mutex::new(VirtualIoapic::new(0, 0xFEC00000)),
            pic_master: Mutex::new(VirtualPic::new(true)),
            pic_slave: Mutex::new(VirtualPic::new(false)),
            apic_mode: AtomicBool::new(true),
        }
    }

    /// Add LAPIC for vCPU
    pub fn add_lapic(&self, id: u8) -> Arc<Mutex<VirtualLapic>> {
        let lapic = Arc::new(Mutex::new(VirtualLapic::new(id)));
        self.lapics.write().push(lapic.clone());
        lapic
    }

    /// Raise IRQ line
    pub fn raise_irq(&self, irq: u32, level: bool) {
        if self.apic_mode.load(Ordering::SeqCst) {
            // Route through IOAPIC
            if let Some(intr) = self.ioapic.lock().raise_irq(irq as u8) {
                self.deliver_interrupt(&intr);
            }
        } else {
            // Route through PIC
            if irq < 8 {
                self.pic_master.lock().set_irq(irq as u8, level);
            } else if irq < 16 {
                self.pic_slave.lock().set_irq((irq - 8) as u8, level);
            }
        }
    }

    /// Deliver interrupt to vCPU(s)
    fn deliver_interrupt(&self, intr: &VirtualInterrupt) {
        let lapics = self.lapics.read();

        for lapic in lapics.iter() {
            let lapic = lapic.lock();

            // Check destination
            let matches = match intr.dest_mode {
                DestMode::Physical => lapic.id() == intr.dest,
                DestMode::Logical => {
                    // Would check logical destination register
                    true
                }
            };

            if matches && lapic.is_enabled() {
                lapic.set_irr(intr.vector);
            }
        }
    }

    /// Get IOAPIC
    pub fn ioapic(&self) -> &Mutex<VirtualIoapic> {
        &self.ioapic
    }
}
