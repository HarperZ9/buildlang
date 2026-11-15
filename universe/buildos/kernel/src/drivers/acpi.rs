// ===============================================================================
// BUILDOS KERNEL - ACPI DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![allow(dead_code)]

//! ACPI (Advanced Configuration and Power Interface) table parsing.
//!
//! Parses ACPI tables to discover:
//! - CPU topology (MADT)
//! - Power management capabilities (FADT)
//! - Device configuration
//! - Interrupt routing

use alloc::vec::Vec;
use core::mem;
use spin::Mutex;

// =============================================================================
// RSDP - ROOT SYSTEM DESCRIPTION POINTER
// =============================================================================

/// RSDP signature "RSD PTR "
const RSDP_SIGNATURE: [u8; 8] = *b"RSD PTR ";

/// Root System Description Pointer (ACPI 1.0)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Rsdp {
    /// "RSD PTR " signature
    pub signature: [u8; 8],
    /// Checksum
    pub checksum: u8,
    /// OEM ID
    pub oem_id: [u8; 6],
    /// Revision (0 = ACPI 1.0, 2 = ACPI 2.0+)
    pub revision: u8,
    /// Physical address of RSDT
    pub rsdt_address: u32,
}

impl Rsdp {
    /// Validate RSDP checksum
    pub fn validate(&self) -> bool {
        if self.signature != RSDP_SIGNATURE {
            return false;
        }

        let bytes = unsafe {
            core::slice::from_raw_parts(
                self as *const _ as *const u8,
                mem::size_of::<Self>()
            )
        };

        let sum: u8 = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        sum == 0
    }
}

/// Extended RSDP (ACPI 2.0+)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Rsdp2 {
    /// Base RSDP
    pub base: Rsdp,
    /// Length of entire structure
    pub length: u32,
    /// Physical address of XSDT
    pub xsdt_address: u64,
    /// Extended checksum
    pub extended_checksum: u8,
    /// Reserved
    pub reserved: [u8; 3],
}

impl Rsdp2 {
    /// Validate extended RSDP checksum
    pub fn validate(&self) -> bool {
        if !self.base.validate() {
            return false;
        }

        if self.base.revision < 2 {
            return true; // Only base validation for ACPI 1.0
        }

        let bytes = unsafe {
            core::slice::from_raw_parts(
                self as *const _ as *const u8,
                self.length as usize
            )
        };

        let sum: u8 = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        sum == 0
    }
}

// =============================================================================
// SDT - SYSTEM DESCRIPTION TABLE HEADER
// =============================================================================

/// System Description Table Header
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SdtHeader {
    /// Table signature (4 chars)
    pub signature: [u8; 4],
    /// Length of entire table
    pub length: u32,
    /// Revision
    pub revision: u8,
    /// Checksum
    pub checksum: u8,
    /// OEM ID
    pub oem_id: [u8; 6],
    /// OEM table ID
    pub oem_table_id: [u8; 8],
    /// OEM revision
    pub oem_revision: u32,
    /// Creator ID
    pub creator_id: u32,
    /// Creator revision
    pub creator_revision: u32,
}

impl SdtHeader {
    /// Get signature as string
    pub fn signature_str(&self) -> &str {
        core::str::from_utf8(&self.signature).unwrap_or("????")
    }

    /// Validate table checksum
    pub fn validate(&self) -> bool {
        let bytes = unsafe {
            core::slice::from_raw_parts(
                self as *const _ as *const u8,
                self.length as usize
            )
        };

        let sum: u8 = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        sum == 0
    }
}

// =============================================================================
// MADT - MULTIPLE APIC DESCRIPTION TABLE
// =============================================================================

/// MADT signature
const MADT_SIGNATURE: [u8; 4] = *b"APIC";

/// MADT flags
const MADT_FLAG_PCAT_COMPAT: u32 = 1 << 0;

/// Multiple APIC Description Table
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Madt {
    /// Header
    pub header: SdtHeader,
    /// Local APIC address
    pub local_apic_addr: u32,
    /// Flags
    pub flags: u32,
}

/// MADT entry types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadtEntryType {
    LocalApic = 0,
    IoApic = 1,
    InterruptSourceOverride = 2,
    NmiSource = 3,
    LocalApicNmi = 4,
    LocalApicAddressOverride = 5,
    IoSapic = 6,
    LocalSapic = 7,
    PlatformInterruptSources = 8,
    LocalX2Apic = 9,
    LocalX2ApicNmi = 10,
    GicCpuInterface = 11,
    GicDistributor = 12,
}

/// MADT entry header
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct MadtEntryHeader {
    pub entry_type: u8,
    pub length: u8,
}

/// Local APIC entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct MadtLocalApic {
    pub header: MadtEntryHeader,
    /// ACPI processor ID
    pub acpi_processor_id: u8,
    /// APIC ID
    pub apic_id: u8,
    /// Flags (bit 0 = enabled)
    pub flags: u32,
}

impl MadtLocalApic {
    pub fn is_enabled(&self) -> bool {
        (self.flags & 1) != 0
    }

    pub fn is_online_capable(&self) -> bool {
        (self.flags & 2) != 0
    }
}

/// I/O APIC entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct MadtIoApic {
    pub header: MadtEntryHeader,
    /// I/O APIC ID
    pub io_apic_id: u8,
    /// Reserved
    pub reserved: u8,
    /// I/O APIC address
    pub io_apic_addr: u32,
    /// Global system interrupt base
    pub gsi_base: u32,
}

/// Interrupt Source Override entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct MadtInterruptOverride {
    pub header: MadtEntryHeader,
    /// Bus source
    pub bus: u8,
    /// IRQ source
    pub source: u8,
    /// Global system interrupt
    pub gsi: u32,
    /// Flags
    pub flags: u16,
}

/// Local APIC NMI entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct MadtLocalApicNmi {
    pub header: MadtEntryHeader,
    /// ACPI processor ID (0xFF = all processors)
    pub acpi_processor_id: u8,
    /// Flags
    pub flags: u16,
    /// Local APIC LINT# (0 or 1)
    pub lint: u8,
}

// =============================================================================
// FADT - FIXED ACPI DESCRIPTION TABLE
// =============================================================================

/// FADT signature
const FADT_SIGNATURE: [u8; 4] = *b"FACP";

/// Fixed ACPI Description Table
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Fadt {
    /// Header
    pub header: SdtHeader,
    /// Physical address of FACS
    pub firmware_ctrl: u32,
    /// Physical address of DSDT
    pub dsdt: u32,
    /// Reserved (was interrupt model)
    pub reserved1: u8,
    /// Preferred PM profile
    pub preferred_pm_profile: u8,
    /// SCI interrupt
    pub sci_int: u16,
    /// SMI command port
    pub smi_cmd: u32,
    /// ACPI enable value
    pub acpi_enable: u8,
    /// ACPI disable value
    pub acpi_disable: u8,
    /// S4BIOS request value
    pub s4bios_req: u8,
    /// PSTATE control value
    pub pstate_cnt: u8,
    /// PM1a event block address
    pub pm1a_evt_blk: u32,
    /// PM1b event block address
    pub pm1b_evt_blk: u32,
    /// PM1a control block address
    pub pm1a_cnt_blk: u32,
    /// PM1b control block address
    pub pm1b_cnt_blk: u32,
    /// PM2 control block address
    pub pm2_cnt_blk: u32,
    /// PM timer block address
    pub pm_tmr_blk: u32,
    /// GPE0 block address
    pub gpe0_blk: u32,
    /// GPE1 block address
    pub gpe1_blk: u32,
    /// PM1 event block length
    pub pm1_evt_len: u8,
    /// PM1 control block length
    pub pm1_cnt_len: u8,
    /// PM2 control block length
    pub pm2_cnt_len: u8,
    /// PM timer block length
    pub pm_tmr_len: u8,
    /// GPE0 block length
    pub gpe0_blk_len: u8,
    /// GPE1 block length
    pub gpe1_blk_len: u8,
    /// GPE1 base
    pub gpe1_base: u8,
    /// C-state control value
    pub cst_cnt: u8,
    /// Worst-case C2 latency (us)
    pub p_lvl2_lat: u16,
    /// Worst-case C3 latency (us)
    pub p_lvl3_lat: u16,
    /// Flush size
    pub flush_size: u16,
    /// Flush stride
    pub flush_stride: u16,
    /// Duty cycle offset
    pub duty_offset: u8,
    /// Duty cycle width
    pub duty_width: u8,
    /// RTC day alarm index
    pub day_alrm: u8,
    /// RTC month alarm index
    pub mon_alrm: u8,
    /// RTC century index
    pub century: u8,
    /// Boot architecture flags
    pub iapc_boot_arch: u16,
    /// Reserved
    pub reserved2: u8,
    /// Fixed feature flags
    pub flags: u32,
    /// Reset register
    pub reset_reg: GenericAddress,
    /// Reset value
    pub reset_value: u8,
    /// ARM boot architecture flags
    pub arm_boot_arch: u16,
    /// FADT minor version
    pub fadt_minor_version: u8,
    // ACPI 2.0+ fields
    /// Extended FACS address
    pub x_firmware_ctrl: u64,
    /// Extended DSDT address
    pub x_dsdt: u64,
    // ... more fields in newer versions
}

/// Generic Address Structure
#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct GenericAddress {
    /// Address space ID
    pub address_space: u8,
    /// Register bit width
    pub bit_width: u8,
    /// Register bit offset
    pub bit_offset: u8,
    /// Access size
    pub access_size: u8,
    /// Address
    pub address: u64,
}

/// Address space IDs
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSpace {
    SystemMemory = 0,
    SystemIo = 1,
    PciConfig = 2,
    EmbeddedController = 3,
    Smbus = 4,
    PlatformCommunicationsChannel = 0x0A,
    FunctionalFixedHardware = 0x7F,
}

// =============================================================================
// HPET - HIGH PRECISION EVENT TIMER
// =============================================================================

/// HPET signature
const HPET_SIGNATURE: [u8; 4] = *b"HPET";

/// High Precision Event Timer table
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Hpet {
    /// Header
    pub header: SdtHeader,
    /// Hardware revision ID
    pub hardware_rev_id: u8,
    /// Comparator count and flags
    pub comparator_count: u8,
    /// PCI vendor ID
    pub pci_vendor_id: u16,
    /// Address structure
    pub address: GenericAddress,
    /// HPET number
    pub hpet_number: u8,
    /// Minimum clock ticks
    pub minimum_tick: u16,
    /// Page protection
    pub page_protection: u8,
}

// =============================================================================
// MCFG - PCI EXPRESS MEMORY-MAPPED CONFIGURATION
// =============================================================================

/// MCFG signature
const MCFG_SIGNATURE: [u8; 4] = *b"MCFG";

/// PCIe configuration table
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Mcfg {
    /// Header
    pub header: SdtHeader,
    /// Reserved
    pub reserved: u64,
}

/// MCFG allocation structure
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct McfgAllocation {
    /// Base address
    pub base_address: u64,
    /// PCI segment group
    pub segment_group: u16,
    /// Start bus number
    pub start_bus: u8,
    /// End bus number
    pub end_bus: u8,
    /// Reserved
    pub reserved: u32,
}

// =============================================================================
// PARSED ACPI DATA
// =============================================================================

/// CPU information from MADT
#[derive(Debug, Clone)]
pub struct CpuInfo {
    /// ACPI processor ID
    pub acpi_id: u8,
    /// APIC ID
    pub apic_id: u8,
    /// Whether CPU is enabled
    pub enabled: bool,
    /// Whether CPU is BSP
    pub is_bsp: bool,
}

/// I/O APIC information
#[derive(Debug, Clone)]
pub struct IoApicInfo {
    /// I/O APIC ID
    pub id: u8,
    /// Base address
    pub address: u32,
    /// Global system interrupt base
    pub gsi_base: u32,
}

/// Interrupt override information
#[derive(Debug, Clone)]
pub struct InterruptOverride {
    /// ISA IRQ
    pub source_irq: u8,
    /// Global system interrupt
    pub gsi: u32,
    /// Polarity (0 = bus default, 1 = high, 3 = low)
    pub polarity: u8,
    /// Trigger mode (0 = bus default, 1 = edge, 3 = level)
    pub trigger: u8,
}

/// Parsed ACPI information
#[derive(Clone)]
pub struct AcpiInfo {
    /// CPUs detected
    pub cpus: Vec<CpuInfo>,
    /// I/O APICs
    pub io_apics: Vec<IoApicInfo>,
    /// Interrupt overrides
    pub interrupt_overrides: Vec<InterruptOverride>,
    /// Local APIC address
    pub local_apic_addr: u64,
    /// HPET address
    pub hpet_addr: Option<u64>,
    /// PCIe ECAM base
    pub pcie_ecam_base: Option<u64>,
    /// SCI interrupt
    pub sci_interrupt: u16,
    /// PM timer port
    pub pm_timer_port: u32,
    /// ACPI version
    pub version: u8,
}

impl AcpiInfo {
    fn new() -> Self {
        Self {
            cpus: Vec::new(),
            io_apics: Vec::new(),
            interrupt_overrides: Vec::new(),
            local_apic_addr: 0xFEE00000,
            hpet_addr: None,
            pcie_ecam_base: None,
            sci_interrupt: 9,
            pm_timer_port: 0,
            version: 0,
        }
    }
}

/// Global ACPI info
static ACPI_INFO: Mutex<Option<AcpiInfo>> = Mutex::new(None);

/// FADT address (stored separately for power management)
static FADT_ADDRESS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize ACPI from RSDP address
///
/// # Safety
///
/// The RSDP address must be valid and point to a valid RSDP structure.
pub unsafe fn init(rsdp_addr: u64) {
    let mut info = AcpiInfo::new();

    // Parse RSDP
    let rsdp = &*(rsdp_addr as *const Rsdp);

    if !rsdp.validate() {
        crate::kprintln!("[ACPI] Invalid RSDP checksum");
        return;
    }

    info.version = rsdp.revision;

    // Get RSDT or XSDT
    let tables = if rsdp.revision >= 2 {
        let rsdp2 = &*(rsdp_addr as *const Rsdp2);
        if rsdp2.validate() && rsdp2.xsdt_address != 0 {
            parse_xsdt(rsdp2.xsdt_address)
        } else {
            parse_rsdt(rsdp.rsdt_address as u64)
        }
    } else {
        parse_rsdt(rsdp.rsdt_address as u64)
    };

    // Parse each table
    for table_addr in tables {
        let header = &*(table_addr as *const SdtHeader);

        if !header.validate() {
            continue;
        }

        match &header.signature {
            sig if sig == &MADT_SIGNATURE => {
                parse_madt(table_addr, &mut info);
            }
            sig if sig == &FADT_SIGNATURE => {
                parse_fadt(table_addr, &mut info);
            }
            sig if sig == &HPET_SIGNATURE => {
                parse_hpet(table_addr, &mut info);
            }
            sig if sig == &MCFG_SIGNATURE => {
                parse_mcfg(table_addr, &mut info);
            }
            _ => {
                // Unknown table - skip
            }
        }
    }

    crate::kprintln!("[ACPI] Version {}.x detected", if info.version >= 2 { 2 } else { 1 });
    crate::kprintln!("[ACPI] Found {} CPUs, {} I/O APICs",
        info.cpus.len(), info.io_apics.len());

    *ACPI_INFO.lock() = Some(info);
}

/// Parse RSDT (32-bit table pointers)
unsafe fn parse_rsdt(addr: u64) -> Vec<u64> {
    let header = &*(addr as *const SdtHeader);
    let entry_count = (header.length as usize - mem::size_of::<SdtHeader>()) / 4;

    let entries_ptr = (addr as usize + mem::size_of::<SdtHeader>()) as *const u32;
    let entries = core::slice::from_raw_parts(entries_ptr, entry_count);

    entries.iter().map(|&addr| addr as u64).collect()
}

/// Parse XSDT (64-bit table pointers)
unsafe fn parse_xsdt(addr: u64) -> Vec<u64> {
    let header = &*(addr as *const SdtHeader);
    let entry_count = (header.length as usize - mem::size_of::<SdtHeader>()) / 8;

    let entries_ptr = (addr as usize + mem::size_of::<SdtHeader>()) as *const u64;
    let entries = core::slice::from_raw_parts(entries_ptr, entry_count);

    entries.to_vec()
}

/// Parse MADT
unsafe fn parse_madt(addr: u64, info: &mut AcpiInfo) {
    let madt = &*(addr as *const Madt);

    info.local_apic_addr = madt.local_apic_addr as u64;

    let mut offset = mem::size_of::<Madt>();
    let end = madt.header.length as usize;

    while offset < end {
        let entry_ptr = (addr as usize + offset) as *const MadtEntryHeader;
        let entry = &*entry_ptr;

        match entry.entry_type {
            0 => {
                // Local APIC
                let lapic = &*(entry_ptr as *const MadtLocalApic);
                info.cpus.push(CpuInfo {
                    acpi_id: lapic.acpi_processor_id,
                    apic_id: lapic.apic_id,
                    enabled: lapic.is_enabled(),
                    is_bsp: info.cpus.is_empty(), // First one is BSP
                });
            }
            1 => {
                // I/O APIC
                let ioapic = &*(entry_ptr as *const MadtIoApic);
                info.io_apics.push(IoApicInfo {
                    id: ioapic.io_apic_id,
                    address: ioapic.io_apic_addr,
                    gsi_base: ioapic.gsi_base,
                });
            }
            2 => {
                // Interrupt Source Override
                let iso = &*(entry_ptr as *const MadtInterruptOverride);
                info.interrupt_overrides.push(InterruptOverride {
                    source_irq: iso.source,
                    gsi: iso.gsi,
                    polarity: (iso.flags & 0x03) as u8,
                    trigger: ((iso.flags >> 2) & 0x03) as u8,
                });
            }
            5 => {
                // Local APIC Address Override
                let lapic_override = (addr as usize + offset + 4) as *const u64;
                info.local_apic_addr = *lapic_override;
            }
            _ => {
                // Other entry types - skip for now
            }
        }

        offset += entry.length as usize;
        if entry.length == 0 {
            break; // Prevent infinite loop
        }
    }
}

/// Parse FADT
unsafe fn parse_fadt(addr: u64, info: &mut AcpiInfo) {
    let fadt = &*(addr as *const Fadt);

    info.sci_interrupt = fadt.sci_int;
    info.pm_timer_port = fadt.pm_tmr_blk;

    // Store FADT address for power management
    FADT_ADDRESS.store(addr, core::sync::atomic::Ordering::Release);
}

/// Parse HPET
unsafe fn parse_hpet(addr: u64, info: &mut AcpiInfo) {
    let hpet = &*(addr as *const Hpet);
    info.hpet_addr = Some(hpet.address.address);
}

/// Parse MCFG
unsafe fn parse_mcfg(addr: u64, info: &mut AcpiInfo) {
    let _mcfg = &*(addr as *const Mcfg);
    let alloc_ptr = (addr as usize + mem::size_of::<Mcfg>()) as *const McfgAllocation;
    let alloc = &*alloc_ptr;

    info.pcie_ecam_base = Some(alloc.base_address);
}

// =============================================================================
// PUBLIC INTERFACE
// =============================================================================

/// Get parsed ACPI information
pub fn get_info() -> Option<AcpiInfo> {
    ACPI_INFO.lock().clone()
}

/// Get FADT address
pub fn get_fadt_address() -> u64 {
    FADT_ADDRESS.load(core::sync::atomic::Ordering::Acquire)
}

/// Get CPU count
pub fn cpu_count() -> usize {
    ACPI_INFO.lock()
        .as_ref()
        .map(|info| info.cpus.len())
        .unwrap_or(1)
}

/// Get local APIC address
pub fn local_apic_addr() -> u64 {
    ACPI_INFO.lock()
        .as_ref()
        .map(|info| info.local_apic_addr)
        .unwrap_or(0xFEE00000)
}

/// Get I/O APIC address
pub fn io_apic_addr() -> Option<u32> {
    ACPI_INFO.lock()
        .as_ref()
        .and_then(|info| info.io_apics.first().map(|io| io.address))
}

/// Get interrupt mapping (source IRQ -> GSI)
pub fn get_interrupt_mapping(irq: u8) -> u32 {
    ACPI_INFO.lock()
        .as_ref()
        .and_then(|info| {
            info.interrupt_overrides
                .iter()
                .find(|o| o.source_irq == irq)
                .map(|o| o.gsi)
        })
        .unwrap_or(irq as u32)
}

// =============================================================================
// POWER MANAGEMENT
// =============================================================================

/// Power state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// S0 - Working
    S0,
    /// S1 - Sleeping (CPU stopped)
    S1,
    /// S2 - Sleeping (CPU off)
    S2,
    /// S3 - Suspend to RAM
    S3,
    /// S4 - Hibernate
    S4,
    /// S5 - Soft off
    S5,
}

/// Enter power state
pub fn enter_power_state(_state: PowerState) {
    // Would use ACPI PM1 control registers
    // This requires more FADT parsing and proper implementation
}

/// Shutdown the system
pub fn shutdown() {
    enter_power_state(PowerState::S5);

    // Fallback: use legacy methods
    unsafe {
        // Try ACPI shutdown via PM1a_CNT
        // SLP_TYP = 5 (S5), SLP_EN = 1
        // This requires knowing the correct SLP_TYP value from _S5 method

        // Fallback: Use keyboard controller reset
        for _ in 0..10 {
            let status: u8;
            core::arch::asm!(
                "in al, 0x64",
                out("al") status,
                options(nostack, nomem)
            );
            if (status & 0x02) == 0 {
                break;
            }
        }
        core::arch::asm!(
            "out 0x64, al",
            in("al") 0xFE_u8,
            options(nostack, nomem)
        );
    }
}

/// Reboot the system
pub fn reboot() {
    unsafe {
        // Try ACPI reset if available
        if let Some(_info) = ACPI_INFO.lock().as_ref() {
            // Check if reset register is valid
            // Would need to read reset_reg from FADT
        }

        // Fallback: keyboard controller reset
        for _ in 0..10 {
            let status: u8;
            core::arch::asm!(
                "in al, 0x64",
                out("al") status,
                options(nostack, nomem)
            );
            if (status & 0x02) == 0 {
                break;
            }
        }
        core::arch::asm!(
            "out 0x64, al",
            in("al") 0xFE_u8,
            options(nostack, nomem)
        );

        // If that fails, triple fault
        core::arch::asm!("int 0xFF");
    }
}

/// Get the number of discovered I/O APICs
pub fn ioapic_count() -> usize {
    ACPI_INFO.lock().as_ref().map(|info| info.io_apics.len()).unwrap_or(0)
}

/// Get the PCIe ECAM base address if available
pub fn pcie_ecam_base() -> Option<u64> {
    ACPI_INFO.lock().as_ref().and_then(|info| info.pcie_ecam_base)
}
