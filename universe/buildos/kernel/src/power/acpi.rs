//! ACPI Power Management Interface
//!
//! Provides ACPI-based power management including:
//! - Power state transitions (S-states)
//! - Device power management (D-states)
//! - Power buttons and lid switch
//! - EC (Embedded Controller) interface

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;
use super::{PowerError, PowerState};

/// ACPI Fixed ACPI Description Table address
static FADT_ADDRESS: AtomicU64 = AtomicU64::new(0);

/// ACPI PM1 control registers
static PM1A_CNT_BLK: AtomicU64 = AtomicU64::new(0);
static PM1B_CNT_BLK: AtomicU64 = AtomicU64::new(0);

/// SLP_TYP values for each S-state
static SLP_TYP: Mutex<[u16; 6]> = Mutex::new([0; 6]);

/// ACPI initialized
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// ACPI version
static ACPI_VERSION: AtomicU64 = AtomicU64::new(0);

/// ACPI registers
#[derive(Clone, Copy, Debug)]
pub struct AcpiRegisters {
    /// PM1a Event Block
    pub pm1a_evt_blk: u32,
    /// PM1b Event Block
    pub pm1b_evt_blk: u32,
    /// PM1a Control Block
    pub pm1a_cnt_blk: u32,
    /// PM1b Control Block
    pub pm1b_cnt_blk: u32,
    /// PM2 Control Block
    pub pm2_cnt_blk: u32,
    /// PM Timer Block
    pub pm_tmr_blk: u32,
    /// GPE0 Block
    pub gpe0_blk: u32,
    /// GPE1 Block
    pub gpe1_blk: u32,
    /// PM1 Event Block Length
    pub pm1_evt_len: u8,
    /// PM1 Control Block Length
    pub pm1_cnt_len: u8,
    /// PM2 Control Block Length
    pub pm2_cnt_len: u8,
    /// PM Timer Length
    pub pm_tmr_len: u8,
    /// GPE0 Block Length
    pub gpe0_blk_len: u8,
    /// GPE1 Block Length
    pub gpe1_blk_len: u8,
}

/// FADT (Fixed ACPI Description Table)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct Fadt {
    /// Header
    pub header: AcpiTableHeader,
    /// FACS address
    pub firmware_ctrl: u32,
    /// DSDT address
    pub dsdt: u32,
    /// Reserved (ACPI 1.0)
    pub reserved: u8,
    /// Preferred power management profile
    pub preferred_pm_profile: u8,
    /// SCI interrupt
    pub sci_int: u16,
    /// SMI command port
    pub smi_cmd: u32,
    /// ACPI enable command
    pub acpi_enable: u8,
    /// ACPI disable command
    pub acpi_disable: u8,
    /// S4BIOS request
    pub s4bios_req: u8,
    /// P-state control
    pub pstate_cnt: u8,
    /// PM1a Event Block
    pub pm1a_evt_blk: u32,
    /// PM1b Event Block
    pub pm1b_evt_blk: u32,
    /// PM1a Control Block
    pub pm1a_cnt_blk: u32,
    /// PM1b Control Block
    pub pm1b_cnt_blk: u32,
    /// PM2 Control Block
    pub pm2_cnt_blk: u32,
    /// PM Timer Block
    pub pm_tmr_blk: u32,
    /// GPE0 Block
    pub gpe0_blk: u32,
    /// GPE1 Block
    pub gpe1_blk: u32,
    /// PM1 Event Block Length
    pub pm1_evt_len: u8,
    /// PM1 Control Block Length
    pub pm1_cnt_len: u8,
    /// PM2 Control Block Length
    pub pm2_cnt_len: u8,
    /// PM Timer Length
    pub pm_tmr_len: u8,
    /// GPE0 Block Length
    pub gpe0_blk_len: u8,
    /// GPE1 Block Length
    pub gpe1_blk_len: u8,
    /// GPE1 Base
    pub gpe1_base: u8,
    /// C-state control
    pub cst_cnt: u8,
    /// P_LVL2 latency
    pub p_lvl2_lat: u16,
    /// P_LVL3 latency
    pub p_lvl3_lat: u16,
    /// Flush size
    pub flush_size: u16,
    /// Flush stride
    pub flush_stride: u16,
    /// Duty offset
    pub duty_offset: u8,
    /// Duty width
    pub duty_width: u8,
    /// Day alarm
    pub day_alrm: u8,
    /// Month alarm
    pub mon_alrm: u8,
    /// Century
    pub century: u8,
    /// Boot architecture flags
    pub boot_arch_flags: u16,
    /// Reserved
    pub reserved2: u8,
    /// Flags
    pub flags: u32,
}

/// ACPI table header
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct AcpiTableHeader {
    /// Signature
    pub signature: [u8; 4],
    /// Length
    pub length: u32,
    /// Revision
    pub revision: u8,
    /// Checksum
    pub checksum: u8,
    /// OEM ID
    pub oem_id: [u8; 6],
    /// OEM Table ID
    pub oem_table_id: [u8; 8],
    /// OEM Revision
    pub oem_revision: u32,
    /// Creator ID
    pub creator_id: u32,
    /// Creator Revision
    pub creator_revision: u32,
}

/// Sleep type values
#[derive(Clone, Copy, Debug)]
pub struct SleepTypeValues {
    pub s0: u16,
    pub s1: u16,
    pub s2: u16,
    pub s3: u16,
    pub s4: u16,
    pub s5: u16,
}

/// Power button event handler
static POWER_BUTTON_HANDLER: Mutex<Option<fn()>> = Mutex::new(None);

/// Sleep button event handler
static SLEEP_BUTTON_HANDLER: Mutex<Option<fn()>> = Mutex::new(None);

/// Lid switch event handler
static LID_SWITCH_HANDLER: Mutex<Option<fn(bool)>> = Mutex::new(None);

/// Initialize ACPI power management
pub fn init() -> Result<(), PowerError> {
    // Get FADT from ACPI driver
    let fadt_addr = crate::drivers::acpi::get_fadt_address();
    if fadt_addr == 0 {
        return Err(PowerError::AcpiError);
    }

    FADT_ADDRESS.store(fadt_addr, Ordering::Release);

    // Parse FADT
    let fadt = unsafe { &*(fadt_addr as *const Fadt) };

    // Store PM1 control block addresses
    PM1A_CNT_BLK.store(fadt.pm1a_cnt_blk as u64, Ordering::Release);
    PM1B_CNT_BLK.store(fadt.pm1b_cnt_blk as u64, Ordering::Release);

    // Get sleep type values from DSDT (simplified)
    let mut slp_typ = SLP_TYP.lock();
    slp_typ[0] = 0;  // S0
    slp_typ[1] = 1;  // S1
    slp_typ[2] = 2;  // S2
    slp_typ[3] = 5;  // S3 (typical value)
    slp_typ[4] = 6;  // S4
    slp_typ[5] = 7;  // S5

    // Determine ACPI version
    let version = if fadt.header.length > 244 {
        2
    } else {
        1
    };
    ACPI_VERSION.store(version, Ordering::Release);

    // Enable ACPI mode if needed
    enable_acpi(fadt)?;

    // Register power button handler
    register_power_button_irq();

    INITIALIZED.store(true, Ordering::Release);

    crate::kprintln!("[ACPI] Power management initialized (ACPI {}.0)", version);

    Ok(())
}

/// Enable ACPI mode
fn enable_acpi(fadt: &Fadt) -> Result<(), PowerError> {
    // Check if ACPI is already enabled
    if fadt.smi_cmd == 0 {
        // ACPI is always enabled on this system
        return Ok(());
    }

    if fadt.acpi_enable == 0 {
        // ACPI already enabled
        return Ok(());
    }

    // Write ACPI enable command to SMI port
    unsafe {
        outb(fadt.smi_cmd as u16, fadt.acpi_enable);
    }

    // Wait for ACPI to be enabled
    let pm1a = fadt.pm1a_cnt_blk as u16;
    for _ in 0..3000 {
        let status = unsafe { inw(pm1a) };
        if status & 1 != 0 {
            return Ok(());
        }
        // Small delay
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }

    Err(PowerError::Timeout)
}

/// Transition to sleep state
pub fn enter_sleep_state(state: PowerState) -> Result<(), PowerError> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return Err(PowerError::InvalidState);
    }

    let slp_typx = match state {
        PowerState::S0Working => return Ok(()),
        PowerState::S1Standby => 1,
        PowerState::S2Sleep => 2,
        PowerState::S3SuspendToRam => 3,
        PowerState::S4Hibernate => 4,
        PowerState::S5SoftOff => 5,
        _ => return Err(PowerError::NotSupported),
    };

    let slp_typ = SLP_TYP.lock()[slp_typx];

    // Prepare for sleep
    prepare_for_sleep(state)?;

    // Write sleep type and enable
    let pm1a_cnt = PM1A_CNT_BLK.load(Ordering::Acquire) as u16;
    let pm1b_cnt = PM1B_CNT_BLK.load(Ordering::Acquire) as u16;

    // SLP_TYPx is bits 10-12, SLP_EN is bit 13
    let slp_value = ((slp_typ as u16) << 10) | (1 << 13);

    unsafe {
        // Disable interrupts
        core::arch::asm!("cli");

        // Write to PM1a_CNT
        if pm1a_cnt != 0 {
            let current = inw(pm1a_cnt);
            outw(pm1a_cnt, (current & !0x3c00) | slp_value);
        }

        // Write to PM1b_CNT if present
        if pm1b_cnt != 0 {
            let current = inw(pm1b_cnt);
            outw(pm1b_cnt, (current & !0x3c00) | slp_value);
        }

        // CPU should halt here for S3+
        if slp_typx >= 3 {
            core::arch::asm!("hlt");
        }
    }

    Ok(())
}

/// Prepare for sleep
fn prepare_for_sleep(state: PowerState) -> Result<(), PowerError> {
    // Flush caches
    unsafe {
        core::arch::asm!("wbinvd");
    }

    // For S4, we would need to save memory to disk
    if state == PowerState::S4Hibernate {
        // Would call hibernation save
    }

    Ok(())
}

/// Power off the system
pub fn power_off() -> Result<(), PowerError> {
    enter_sleep_state(PowerState::S5SoftOff)
}

/// Reboot the system
pub fn reboot() -> Result<(), PowerError> {
    if !INITIALIZED.load(Ordering::Acquire) {
        // Fallback: triple fault
        unsafe {
            let ptr: *const u8 = core::ptr::null();
            let _ = core::ptr::read_volatile(ptr);
        }
    }

    // Try keyboard controller reset
    unsafe {
        outb(0x64, 0xFE);
    }

    // If that didn't work, try ACPI reset register
    let fadt_addr = FADT_ADDRESS.load(Ordering::Acquire);
    if fadt_addr != 0 {
        let fadt = unsafe { &*(fadt_addr as *const Fadt) };

        // Check if reset register is available (ACPI 2.0+)
        if fadt.header.length > 244 {
            // Would access reset_reg field
        }
    }

    // Last resort: triple fault
    unsafe {
        core::arch::asm!("int3");
    }

    Err(PowerError::NotSupported)
}

/// Register power button handler
fn register_power_button_irq() {
    // Register with interrupt subsystem
    // The power button typically generates SCI
}

/// Set power button event handler
pub fn set_power_button_handler(handler: fn()) {
    *POWER_BUTTON_HANDLER.lock() = Some(handler);
}

/// Set sleep button event handler
pub fn set_sleep_button_handler(handler: fn()) {
    *SLEEP_BUTTON_HANDLER.lock() = Some(handler);
}

/// Set lid switch event handler
pub fn set_lid_switch_handler(handler: fn(bool)) {
    *LID_SWITCH_HANDLER.lock() = Some(handler);
}

/// Handle power button press
pub fn handle_power_button() {
    if let Some(handler) = *POWER_BUTTON_HANDLER.lock() {
        handler();
    }
}

/// Handle sleep button press
pub fn handle_sleep_button() {
    if let Some(handler) = *SLEEP_BUTTON_HANDLER.lock() {
        handler();
    }
}

/// Handle lid switch event
pub fn handle_lid_switch(closed: bool) {
    if let Some(handler) = *LID_SWITCH_HANDLER.lock() {
        handler(closed);
    }
}

/// Get supported sleep states
pub fn get_supported_states() -> Vec<PowerState> {
    let mut states = vec![PowerState::S0Working];

    // Check which S-states are supported
    // This would parse the DSDT/SSDT to find \_Sx methods

    // Most systems support at least S3 and S5
    states.push(PowerState::S3SuspendToRam);
    states.push(PowerState::S5SoftOff);

    states
}

/// Check if S-state is supported
pub fn is_state_supported(state: PowerState) -> bool {
    get_supported_states().contains(&state)
}

// =============================================================================
// Embedded Controller Interface
// =============================================================================

/// EC command port
const EC_SC: u16 = 0x66;
/// EC data port
const EC_DATA: u16 = 0x62;

/// EC status flags
const EC_OBF: u8 = 1 << 0;  // Output buffer full
const EC_IBF: u8 = 1 << 1;  // Input buffer full

/// Read EC register
pub fn ec_read(addr: u8) -> Result<u8, PowerError> {
    // Wait for input buffer empty
    ec_wait_ibf()?;

    // Send read command
    unsafe {
        outb(EC_SC, 0x80);
    }

    // Wait for input buffer empty
    ec_wait_ibf()?;

    // Send address
    unsafe {
        outb(EC_DATA, addr);
    }

    // Wait for output buffer full
    ec_wait_obf()?;

    // Read data
    Ok(unsafe { inb(EC_DATA) })
}

/// Write EC register
pub fn ec_write(addr: u8, value: u8) -> Result<(), PowerError> {
    // Wait for input buffer empty
    ec_wait_ibf()?;

    // Send write command
    unsafe {
        outb(EC_SC, 0x81);
    }

    // Wait for input buffer empty
    ec_wait_ibf()?;

    // Send address
    unsafe {
        outb(EC_DATA, addr);
    }

    // Wait for input buffer empty
    ec_wait_ibf()?;

    // Send data
    unsafe {
        outb(EC_DATA, value);
    }

    Ok(())
}

/// Wait for EC input buffer empty
fn ec_wait_ibf() -> Result<(), PowerError> {
    for _ in 0..1000 {
        let status = unsafe { inb(EC_SC) };
        if status & EC_IBF == 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(PowerError::Timeout)
}

/// Wait for EC output buffer full
fn ec_wait_obf() -> Result<(), PowerError> {
    for _ in 0..1000 {
        let status = unsafe { inb(EC_SC) };
        if status & EC_OBF != 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(PowerError::Timeout)
}

// =============================================================================
// Port I/O Helpers
// =============================================================================

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    value
}

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

#[inline(always)]
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    core::arch::asm!(
        "in ax, dx",
        out("ax") value,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    value
}

#[inline(always)]
unsafe fn outw(port: u16, value: u16) {
    core::arch::asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") value,
        options(nomem, nostack, preserves_flags)
    );
}

/// vec! macro for no_std
#[macro_export]
macro_rules! vec {
    () => { alloc::vec::Vec::new() };
    ($($x:expr),+ $(,)?) => {{
        let mut v = alloc::vec::Vec::new();
        $(v.push($x);)+
        v
    }};
}
