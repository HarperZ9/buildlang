// ===============================================================================
// BUILDOS KERNEL - EHCI (USB 2.0) HOST CONTROLLER DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Enhanced Host Controller Interface (EHCI) driver for USB 2.0 High-Speed.
// Provides 480 Mbps transfers for USB 2.0 devices.
//
// ===============================================================================

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

use super::{
    UsbController, UsbSpeed, EndpointDirection,
    SetupPacket, TransferStatus,
    PortStatus, EndpointDescriptor,
};
use super::usb_core::UsbBuffer;

// =============================================================================
// EHCI REGISTER DEFINITIONS
// =============================================================================

/// EHCI Capability Registers
#[repr(C)]
pub struct EhciCapabilityRegs {
    /// Capability Register Length
    pub caplength: u8,
    _reserved: u8,
    /// Interface Version Number
    pub hciversion: u16,
    /// Structural Parameters
    pub hcsparams: u32,
    /// Capability Parameters
    pub hccparams: u32,
    /// Companion Port Route Description
    pub hcsp_portroute: u64,
}

/// EHCI Operational Registers
#[repr(C)]
pub struct EhciOperationalRegs {
    /// USB Command
    pub usbcmd: u32,
    /// USB Status
    pub usbsts: u32,
    /// USB Interrupt Enable
    pub usbintr: u32,
    /// USB Frame Index
    pub frindex: u32,
    /// 4G Segment Selector
    pub ctrldssegment: u32,
    /// Frame List Base Address
    pub periodiclistbase: u32,
    /// Async List Address
    pub asynclistaddr: u32,
    _reserved: [u32; 9],
    /// Configure Flag
    pub configflag: u32,
    /// Port Status and Control (first port)
    pub portsc: [u32; 15],
}

// =============================================================================
// EHCI COMMAND/STATUS BITS
// =============================================================================

// USB Command Register bits
const USBCMD_RUN: u32 = 1 << 0;
const USBCMD_HCRESET: u32 = 1 << 1;
const USBCMD_FLS_MASK: u32 = 0x3 << 2;
const USBCMD_PSE: u32 = 1 << 4;
const USBCMD_ASE: u32 = 1 << 5;
const USBCMD_IAAD: u32 = 1 << 6;
const USBCMD_LHCR: u32 = 1 << 7;
const USBCMD_ASPMC_MASK: u32 = 0x3 << 8;
const USBCMD_ASPME: u32 = 1 << 11;
const USBCMD_ITC_MASK: u32 = 0xFF << 16;

// USB Status Register bits
const USBSTS_INT: u32 = 1 << 0;
const USBSTS_ERR: u32 = 1 << 1;
const USBSTS_PCD: u32 = 1 << 2;
const USBSTS_FLR: u32 = 1 << 3;
const USBSTS_HSE: u32 = 1 << 4;
const USBSTS_IAA: u32 = 1 << 5;
const USBSTS_HALT: u32 = 1 << 12;
const USBSTS_RECL: u32 = 1 << 13;
const USBSTS_PSS: u32 = 1 << 14;
const USBSTS_ASS: u32 = 1 << 15;

// USB Interrupt Enable bits
const USBINTR_INT: u32 = 1 << 0;
const USBINTR_ERR: u32 = 1 << 1;
const USBINTR_PCD: u32 = 1 << 2;
const USBINTR_FLR: u32 = 1 << 3;
const USBINTR_HSE: u32 = 1 << 4;
const USBINTR_IAA: u32 = 1 << 5;

// Port Status and Control bits
const PORTSC_CCS: u32 = 1 << 0;      // Current Connect Status
const PORTSC_CSC: u32 = 1 << 1;      // Connect Status Change
const PORTSC_PED: u32 = 1 << 2;      // Port Enabled/Disabled
const PORTSC_PEDC: u32 = 1 << 3;     // Port Enable/Disable Change
const PORTSC_OCA: u32 = 1 << 4;      // Over-current Active
const PORTSC_OCC: u32 = 1 << 5;      // Over-current Change
const PORTSC_FPR: u32 = 1 << 6;      // Force Port Resume
const PORTSC_SUSPEND: u32 = 1 << 7;  // Suspend
const PORTSC_PRESET: u32 = 1 << 8;   // Port Reset
const PORTSC_LS_MASK: u32 = 0x3 << 10;  // Line Status
const PORTSC_PP: u32 = 1 << 12;      // Port Power
const PORTSC_PO: u32 = 1 << 13;      // Port Owner
const PORTSC_PIC_MASK: u32 = 0x3 << 14;  // Port Indicator Control
const PORTSC_PTC_MASK: u32 = 0xF << 16;  // Port Test Control
const PORTSC_WKCNNT_E: u32 = 1 << 20;    // Wake on Connect Enable
const PORTSC_WKDSCNNT_E: u32 = 1 << 21;  // Wake on Disconnect Enable
const PORTSC_WKOC_E: u32 = 1 << 22;      // Wake on Over-current Enable

// Line Status values
const LINE_STATUS_SE0: u32 = 0;
const LINE_STATUS_J: u32 = 2;
const LINE_STATUS_K: u32 = 1;

// =============================================================================
// EHCI DATA STRUCTURES
// =============================================================================

/// Queue Head Horizontal Link Pointer
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct QhHorizontalLink(u32);

impl QhHorizontalLink {
    pub fn new(addr: u32, typ: u8, terminate: bool) -> Self {
        let mut val = addr & !0x1F;
        val |= (typ as u32 & 0x3) << 1;
        if terminate {
            val |= 1;
        }
        Self(val)
    }

    pub fn terminate() -> Self {
        Self(1)
    }

    pub fn address(&self) -> u32 {
        self.0 & !0x1F
    }

    pub fn is_terminate(&self) -> bool {
        (self.0 & 1) != 0
    }
}

/// Queue Element Transfer Descriptor (qTD)
#[derive(Clone, Copy)]
#[repr(C, align(32))]
pub struct Qtd {
    /// Next qTD Pointer
    pub next: u32,
    /// Alternate Next qTD Pointer
    pub alt_next: u32,
    /// qTD Token
    pub token: u32,
    /// Buffer Pointer (Page 0)
    pub buffer0: u32,
    /// Buffer Pointer (Page 1)
    pub buffer1: u32,
    /// Buffer Pointer (Page 2)
    pub buffer2: u32,
    /// Buffer Pointer (Page 3)
    pub buffer3: u32,
    /// Buffer Pointer (Page 4)
    pub buffer4: u32,
    /// Extended buffer pointers for 64-bit (optional)
    pub ext_buffer: [u32; 5],
}

impl Default for Qtd {
    fn default() -> Self {
        Self {
            next: 1, // Terminate
            alt_next: 1,
            token: 0,
            buffer0: 0,
            buffer1: 0,
            buffer2: 0,
            buffer3: 0,
            buffer4: 0,
            ext_buffer: [0; 5],
        }
    }
}

impl Qtd {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_status(&mut self, status: u8) {
        self.token = (self.token & !0xFF) | (status as u32);
    }

    pub fn status(&self) -> u8 {
        (self.token & 0xFF) as u8
    }

    pub fn set_pid(&mut self, pid: u8) {
        self.token = (self.token & !(0x3 << 8)) | ((pid as u32 & 0x3) << 8);
    }

    pub fn set_cerr(&mut self, cerr: u8) {
        self.token = (self.token & !(0x3 << 10)) | ((cerr as u32 & 0x3) << 10);
    }

    pub fn set_current_page(&mut self, page: u8) {
        self.token = (self.token & !(0x7 << 12)) | ((page as u32 & 0x7) << 12);
    }

    pub fn set_ioc(&mut self) {
        self.token |= 1 << 15;
    }

    pub fn set_total_bytes(&mut self, bytes: u16) {
        self.token = (self.token & !(0x7FFF << 16)) | ((bytes as u32 & 0x7FFF) << 16);
    }

    pub fn total_bytes(&self) -> u16 {
        ((self.token >> 16) & 0x7FFF) as u16
    }

    pub fn set_data_toggle(&mut self, toggle: bool) {
        if toggle {
            self.token |= 1 << 31;
        } else {
            self.token &= !(1 << 31);
        }
    }

    pub fn is_active(&self) -> bool {
        (self.token & (1 << 7)) != 0
    }

    pub fn is_halted(&self) -> bool {
        (self.token & (1 << 6)) != 0
    }

    pub fn set_buffer(&mut self, addr: u64, len: usize) {
        self.buffer0 = addr as u32;
        let mut offset = 0x1000 - (addr & 0xFFF) as usize;

        if len > offset {
            self.buffer1 = ((addr + offset as u64) & !0xFFF) as u32;
            offset += 0x1000;
        }
        if len > offset {
            self.buffer2 = ((addr + offset as u64) & !0xFFF) as u32;
            offset += 0x1000;
        }
        if len > offset {
            self.buffer3 = ((addr + offset as u64) & !0xFFF) as u32;
            offset += 0x1000;
        }
        if len > offset {
            self.buffer4 = ((addr + offset as u64) & !0xFFF) as u32;
        }

        self.set_total_bytes(len as u16);
    }
}

/// PID codes
const PID_OUT: u8 = 0;
const PID_IN: u8 = 1;
const PID_SETUP: u8 = 2;

/// Queue Head
#[derive(Clone, Copy)]
#[repr(C, align(32))]
pub struct QueueHead {
    /// Queue Head Horizontal Link Pointer
    pub horizontal_link: QhHorizontalLink,
    /// Endpoint Characteristics
    pub characteristics: u32,
    /// Endpoint Capabilities
    pub capabilities: u32,
    /// Current qTD Pointer
    pub current_qtd: u32,
    /// Overlay area (qTD)
    pub overlay: Qtd,
}

impl Default for QueueHead {
    fn default() -> Self {
        Self {
            horizontal_link: QhHorizontalLink::terminate(),
            characteristics: 0,
            capabilities: 0,
            current_qtd: 0,
            overlay: Qtd::default(),
        }
    }
}

impl QueueHead {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_device_address(&mut self, addr: u8) {
        self.characteristics = (self.characteristics & !0x7F) | (addr as u32 & 0x7F);
    }

    pub fn set_inactive(&mut self) {
        self.characteristics |= 1 << 7;
    }

    pub fn set_endpoint(&mut self, ep: u8) {
        self.characteristics = (self.characteristics & !(0xF << 8)) | ((ep as u32 & 0xF) << 8);
    }

    pub fn set_endpoint_speed(&mut self, speed: u8) {
        self.characteristics = (self.characteristics & !(0x3 << 12)) | ((speed as u32 & 0x3) << 12);
    }

    pub fn set_data_toggle_control(&mut self) {
        self.characteristics |= 1 << 14;
    }

    pub fn set_head_of_reclamation(&mut self) {
        self.characteristics |= 1 << 15;
    }

    pub fn set_max_packet_length(&mut self, len: u16) {
        self.characteristics = (self.characteristics & !(0x7FF << 16)) | ((len as u32 & 0x7FF) << 16);
    }

    pub fn set_control_endpoint(&mut self) {
        self.characteristics |= 1 << 27;
    }

    pub fn set_nak_reload(&mut self, count: u8) {
        self.characteristics = (self.characteristics & !(0xF << 28)) | ((count as u32 & 0xF) << 28);
    }

    pub fn set_interrupt_schedule_mask(&mut self, mask: u8) {
        self.capabilities = (self.capabilities & !0xFF) | (mask as u32);
    }

    pub fn set_split_completion_mask(&mut self, mask: u8) {
        self.capabilities = (self.capabilities & !(0xFF << 8)) | ((mask as u32) << 8);
    }

    pub fn set_hub_address(&mut self, addr: u8) {
        self.capabilities = (self.capabilities & !(0x7F << 16)) | ((addr as u32 & 0x7F) << 16);
    }

    pub fn set_port_number(&mut self, port: u8) {
        self.capabilities = (self.capabilities & !(0x7F << 23)) | ((port as u32 & 0x7F) << 23);
    }

    pub fn set_mult(&mut self, mult: u8) {
        self.capabilities = (self.capabilities & !(0x3 << 30)) | ((mult as u32 & 0x3) << 30);
    }
}

// Speed values for EHCI
const EHCI_SPEED_FULL: u8 = 0;
const EHCI_SPEED_LOW: u8 = 1;
const EHCI_SPEED_HIGH: u8 = 2;

// =============================================================================
// EHCI FRAME LIST
// =============================================================================

const FRAME_LIST_SIZE: usize = 1024;

/// Frame List Entry
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct FrameListEntry(u32);

impl FrameListEntry {
    pub fn terminate() -> Self {
        Self(1)
    }

    pub fn queue_head(addr: u32) -> Self {
        Self((addr & !0x1F) | (1 << 1))
    }

    pub fn is_terminate(&self) -> bool {
        (self.0 & 1) != 0
    }
}

// =============================================================================
// EHCI ASYNC SCHEDULE
// =============================================================================

/// Asynchronous schedule manager
pub struct AsyncSchedule {
    /// Head queue (dummy QH for async list)
    head: Box<QueueHead>,
    /// Active queue heads
    queue_heads: Vec<Box<QueueHead>>,
    /// Physical address of head
    head_physical: u32,
}

impl AsyncSchedule {
    pub fn new() -> Self {
        let mut head = Box::new(QueueHead::new());
        let head_physical = head.as_ref() as *const _ as u32;

        // Setup head as circular list pointing to itself
        head.horizontal_link = QhHorizontalLink::new(head_physical, 1, false);
        head.set_head_of_reclamation();

        Self {
            head,
            queue_heads: Vec::new(),
            head_physical,
        }
    }

    pub fn head_address(&self) -> u32 {
        self.head_physical
    }

    pub fn add_queue_head(&mut self, mut qh: Box<QueueHead>) {
        let qh_physical = qh.as_ref() as *const _ as u32;

        // Insert after head
        qh.horizontal_link = self.head.horizontal_link;
        self.head.horizontal_link = QhHorizontalLink::new(qh_physical, 1, false);

        self.queue_heads.push(qh);
    }

    pub fn remove_queue_head(&mut self, _addr: u32) {
        // Find and remove from linked list
        // (simplified - would need proper list management)
    }
}

// =============================================================================
// EHCI PERIODIC SCHEDULE
// =============================================================================

/// Periodic schedule manager
pub struct PeriodicSchedule {
    /// Frame list
    frame_list: Box<[FrameListEntry; FRAME_LIST_SIZE]>,
    /// Physical address of frame list
    frame_list_physical: u32,
    /// Interrupt queue heads
    interrupt_qhs: Vec<Box<QueueHead>>,
}

impl PeriodicSchedule {
    pub fn new() -> Self {
        let frame_list = Box::new([FrameListEntry::terminate(); FRAME_LIST_SIZE]);
        let frame_list_physical = frame_list.as_ptr() as u32;

        Self {
            frame_list,
            frame_list_physical,
            interrupt_qhs: Vec::new(),
        }
    }

    pub fn frame_list_address(&self) -> u32 {
        self.frame_list_physical
    }

    pub fn add_interrupt_qh(&mut self, qh: Box<QueueHead>, interval: u8) {
        let qh_addr = qh.as_ref() as *const _ as u32;

        // Link into frame list at appropriate intervals
        let step = interval.min(32) as usize;
        for i in (0..FRAME_LIST_SIZE).step_by(step.max(1)) {
            if self.frame_list[i].is_terminate() {
                self.frame_list[i] = FrameListEntry::queue_head(qh_addr);
            }
        }

        self.interrupt_qhs.push(qh);
    }
}

// =============================================================================
// EHCI CONTROLLER
// =============================================================================

/// Device state for EHCI
struct DeviceState {
    address: u8,
    speed: UsbSpeed,
    max_packet_ep0: u16,
    toggle: [bool; 32], // Data toggle per endpoint
}

impl DeviceState {
    fn new(address: u8) -> Self {
        Self {
            address,
            speed: UsbSpeed::High,
            max_packet_ep0: 64,
            toggle: [false; 32],
        }
    }
}

/// EHCI Host Controller
pub struct EhciController {
    /// Base address of MMIO registers
    mmio_base: u64,
    /// Capability registers offset
    cap_regs: u64,
    /// Operational registers offset
    op_regs: u64,
    /// Number of ports
    num_ports: u8,
    /// 64-bit addressing capable
    addr64: bool,
    /// Asynchronous schedule
    async_schedule: Mutex<AsyncSchedule>,
    /// Periodic schedule
    periodic_schedule: Mutex<PeriodicSchedule>,
    /// Device states
    devices: Mutex<BTreeMap<u8, DeviceState>>,
    /// Running state
    running: AtomicBool,
    /// Current frame index
    frame_index: AtomicU32,
}

impl EhciController {
    /// Create a new EHCI controller
    pub fn new(mmio_base: u64) -> Option<Self> {
        crate::log::info!("EHCI: Initializing controller at {:016x}", mmio_base);

        // Read capability registers
        let cap_length = unsafe { read_volatile(mmio_base as *const u8) };
        let hci_version = unsafe { read_volatile((mmio_base + 2) as *const u16) };
        let hcsparams = unsafe { read_volatile((mmio_base + 4) as *const u32) };
        let hccparams = unsafe { read_volatile((mmio_base + 8) as *const u32) };

        crate::log::info!("EHCI: Version {:x}.{:x}", hci_version >> 8, hci_version & 0xFF);

        let num_ports = (hcsparams & 0xF) as u8;
        let _ppc = (hcsparams & (1 << 4)) != 0;
        let num_cc = ((hcsparams >> 12) & 0xF) as u8;

        crate::log::info!("EHCI: {} ports, {} companion controllers", num_ports, num_cc);

        let addr64 = (hccparams & 1) != 0;
        let _prog_frame_list = (hccparams & (1 << 1)) != 0;
        let _async_park = (hccparams & (1 << 2)) != 0;
        let eecp = ((hccparams >> 8) & 0xFF) as u8;

        crate::log::info!("EHCI: 64-bit: {}, EECP: {:02x}", addr64, eecp);

        let cap_regs = mmio_base;
        let op_regs = mmio_base + cap_length as u64;

        let async_schedule = Mutex::new(AsyncSchedule::new());
        let periodic_schedule = Mutex::new(PeriodicSchedule::new());

        Some(Self {
            mmio_base,
            cap_regs,
            op_regs,
            num_ports,
            addr64,
            async_schedule,
            periodic_schedule,
            devices: Mutex::new(BTreeMap::new()),
            running: AtomicBool::new(false),
            frame_index: AtomicU32::new(0),
        })
    }

    /// Take ownership from BIOS
    fn take_ownership(&self) {
        // Read EECP to find legacy support capability
        let hccparams = unsafe { read_volatile((self.cap_regs + 8) as *const u32) };
        let mut eecp = ((hccparams >> 8) & 0xFF) as u64;

        if eecp >= 0x40 {
            // Scan for legacy support capability
            while eecp != 0 {
                let cap = unsafe { read_volatile((self.mmio_base + eecp) as *const u32) };
                let cap_id = cap & 0xFF;

                if cap_id == 1 {
                    // USB Legacy Support
                    crate::log::debug!("EHCI: Found legacy support at {:02x}", eecp);

                    // Request ownership
                    unsafe {
                        let legsup = read_volatile((self.mmio_base + eecp) as *const u32);
                        write_volatile(
                            (self.mmio_base + eecp) as *mut u32,
                            legsup | (1 << 24), // HC OS Owned Semaphore
                        );
                    }

                    // Wait for BIOS to release
                    for _ in 0..1000 {
                        let legsup = unsafe { read_volatile((self.mmio_base + eecp) as *const u32) };
                        if (legsup & (1 << 16)) == 0 { // BIOS Owned Semaphore
                            crate::log::debug!("EHCI: Ownership transferred from BIOS");
                            break;
                        }
                        for _ in 0..10000 { core::hint::spin_loop(); }
                    }

                    // Disable SMI
                    unsafe {
                        write_volatile((self.mmio_base + eecp + 4) as *mut u32, 0);
                    }

                    break;
                }

                eecp = ((cap >> 8) & 0xFF) as u64;
                if eecp != 0 {
                    eecp += self.mmio_base;
                }
            }
        }
    }

    /// Reset the controller
    fn reset(&self) -> bool {
        crate::log::debug!("EHCI: Resetting controller");

        // Stop if running
        let usbcmd = unsafe { read_volatile(self.op_regs as *const u32) };
        if (usbcmd & USBCMD_RUN) != 0 {
            unsafe { write_volatile(self.op_regs as *mut u32, usbcmd & !USBCMD_RUN) };

            // Wait for halt
            for _ in 0..1000 {
                let sts = unsafe { read_volatile((self.op_regs + 4) as *const u32) };
                if (sts & USBSTS_HALT) != 0 {
                    break;
                }
                for _ in 0..1000 { core::hint::spin_loop(); }
            }
        }

        // Reset
        unsafe { write_volatile(self.op_regs as *mut u32, USBCMD_HCRESET) };

        // Wait for reset to complete
        for _ in 0..1000 {
            let cmd = unsafe { read_volatile(self.op_regs as *const u32) };
            if (cmd & USBCMD_HCRESET) == 0 {
                crate::log::debug!("EHCI: Reset complete");
                return true;
            }
            for _ in 0..10000 { core::hint::spin_loop(); }
        }

        crate::log::error!("EHCI: Reset timeout");
        false
    }

    /// Start the controller
    pub fn start(&self) -> bool {
        self.take_ownership();

        if !self.reset() {
            return false;
        }

        // Set 4G segment (for 64-bit)
        if self.addr64 {
            unsafe {
                write_volatile((self.op_regs + 0x10) as *mut u32, 0);
            }
        }

        // Set periodic list base
        let periodic_base = self.periodic_schedule.lock().frame_list_address();
        unsafe {
            write_volatile((self.op_regs + 0x14) as *mut u32, periodic_base);
        }

        // Set async list address
        let async_base = self.async_schedule.lock().head_address();
        unsafe {
            write_volatile((self.op_regs + 0x18) as *mut u32, async_base);
        }

        // Enable interrupts
        unsafe {
            write_volatile(
                (self.op_regs + 8) as *mut u32,
                USBINTR_INT | USBINTR_ERR | USBINTR_PCD | USBINTR_HSE,
            );
        }

        // Set configure flag (route all ports to EHCI)
        unsafe {
            write_volatile((self.op_regs + 0x40) as *mut u32, 1);
        }

        // Start controller with async and periodic schedules
        unsafe {
            write_volatile(
                self.op_regs as *mut u32,
                USBCMD_RUN | USBCMD_ASE | USBCMD_PSE | (8 << 16), // ITC = 8 microframes
            );
        }

        // Wait for running
        for _ in 0..1000 {
            let sts = unsafe { read_volatile((self.op_regs + 4) as *const u32) };
            if (sts & USBSTS_HALT) == 0 {
                crate::log::info!("EHCI: Controller running");
                self.running.store(true, Ordering::Release);
                return true;
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        crate::log::error!("EHCI: Failed to start");
        false
    }

    /// Handle interrupt
    pub fn handle_interrupt(&self) {
        let status = unsafe { read_volatile((self.op_regs + 4) as *const u32) };

        // Clear handled interrupts
        unsafe {
            write_volatile(
                (self.op_regs + 4) as *mut u32,
                status & (USBSTS_INT | USBSTS_ERR | USBSTS_PCD | USBSTS_FLR | USBSTS_HSE | USBSTS_IAA),
            );
        }

        if (status & USBSTS_INT) != 0 {
            crate::log::debug!("EHCI: Transfer complete");
        }

        if (status & USBSTS_ERR) != 0 {
            crate::log::warn!("EHCI: Transfer error");
        }

        if (status & USBSTS_PCD) != 0 {
            crate::log::info!("EHCI: Port change detected");
            self.handle_port_change();
        }

        if (status & USBSTS_HSE) != 0 {
            crate::log::error!("EHCI: Host system error!");
        }

        if (status & USBSTS_IAA) != 0 {
            crate::log::debug!("EHCI: Async advance");
        }
    }

    fn handle_port_change(&self) {
        for port in 0..self.num_ports {
            let portsc_addr = self.op_regs + 0x44 + (port as u64) * 4;
            let portsc = unsafe { read_volatile(portsc_addr as *const u32) };

            if (portsc & PORTSC_CSC) != 0 {
                // Clear change bit
                unsafe {
                    write_volatile(portsc_addr as *mut u32, portsc | PORTSC_CSC);
                }

                if (portsc & PORTSC_CCS) != 0 {
                    crate::log::info!("EHCI: Device connected on port {}", port + 1);

                    // Check line status for low-speed device
                    let line_status = (portsc & PORTSC_LS_MASK) >> 10;
                    if line_status == LINE_STATUS_K {
                        // Low-speed device - release to companion controller
                        crate::log::info!("EHCI: Low-speed device, releasing to companion");
                        unsafe {
                            write_volatile(portsc_addr as *mut u32, portsc | PORTSC_PO);
                        }
                    }
                } else {
                    crate::log::info!("EHCI: Device disconnected from port {}", port + 1);
                }
            }
        }
    }

    /// Execute a control transfer
    fn do_control_transfer(
        &self,
        address: u8,
        setup: SetupPacket,
        data: Option<&mut [u8]>,
    ) -> Result<usize, TransferStatus> {
        let devices = self.devices.lock();
        let device = devices.get(&address).ok_or(TransferStatus::Error)?;

        // Create queue head for control transfer
        let mut qh = Box::new(QueueHead::new());
        qh.set_device_address(address);
        qh.set_endpoint(0);
        qh.set_endpoint_speed(match device.speed {
            UsbSpeed::High => EHCI_SPEED_HIGH,
            UsbSpeed::Full => EHCI_SPEED_FULL,
            UsbSpeed::Low => EHCI_SPEED_LOW,
            _ => EHCI_SPEED_HIGH,
        });
        qh.set_max_packet_length(device.max_packet_ep0);
        qh.set_data_toggle_control();
        qh.set_control_endpoint();
        qh.set_nak_reload(4);
        qh.set_mult(1);

        // Create setup TD
        let setup_buffer = UsbBuffer::new(8);
        unsafe {
            let setup_ptr = setup_buffer.as_ptr() as *mut SetupPacket;
            *setup_ptr = setup;
        }

        let mut setup_td = Box::new(Qtd::new());
        setup_td.set_buffer(setup_buffer.physical_address(), 8);
        setup_td.set_pid(PID_SETUP);
        setup_td.set_cerr(3);
        setup_td.set_status(0x80); // Active
        setup_td.set_data_toggle(false);

        // Save pointer before moving setup_td
        let setup_td_ptr = setup_td.as_ref() as *const _ as u32;
        let mut last_td = setup_td;
        let mut bytes_to_transfer = 0usize;

        // Create data TD(s) if needed
        if let Some(ref data_buf) = data {
            bytes_to_transfer = data_buf.len();
            let mut data_td = Box::new(Qtd::new());
            data_td.set_buffer(data_buf.as_ptr() as u64, data_buf.len());
            data_td.set_pid(if (setup.request_type & 0x80) != 0 { PID_IN } else { PID_OUT });
            data_td.set_cerr(3);
            data_td.set_status(0x80);
            data_td.set_data_toggle(true);

            last_td.next = data_td.as_ref() as *const _ as u32;
            last_td = data_td;
        }

        // Create status TD
        let mut status_td = Box::new(Qtd::new());
        status_td.set_pid(if data.is_some() && (setup.request_type & 0x80) != 0 { PID_OUT } else { PID_IN });
        status_td.set_cerr(3);
        status_td.set_status(0x80);
        status_td.set_data_toggle(true);
        status_td.set_ioc();

        last_td.next = status_td.as_ref() as *const _ as u32;

        // Link TD chain to QH
        qh.overlay.next = setup_td_ptr;

        // Add to async schedule
        let _qh_addr = qh.as_ref() as *const _ as u32;
        {
            let mut async_schedule = self.async_schedule.lock();
            async_schedule.add_queue_head(qh);
        }

        // Wait for completion (simplified)
        for _ in 0..10000 {
            if !status_td.is_active() {
                break;
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        // Check result
        if status_td.is_halted() {
            return Err(TransferStatus::Stall);
        }

        if status_td.is_active() {
            return Err(TransferStatus::Timeout);
        }

        Ok(bytes_to_transfer - status_td.total_bytes() as usize)
    }
}

// =============================================================================
// USB CONTROLLER TRAIT IMPLEMENTATION
// =============================================================================

impl UsbController for EhciController {
    fn name(&self) -> &'static str {
        "EHCI"
    }

    fn init(&self) -> Result<(), &'static str> {
        // Reset controller
        let usbcmd = self.op_regs;
        unsafe {
            let cmd = read_volatile(usbcmd as *const u32);
            write_volatile(usbcmd as *mut u32, cmd | 0x02); // HCRESET
        }

        // Wait for reset to complete
        for _ in 0..1000 {
            let cmd = unsafe { read_volatile(usbcmd as *const u32) };
            if (cmd & 0x02) == 0 {
                return Ok(());
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        Err("EHCI: Controller reset timeout")
    }

    fn start(&self) -> Result<(), &'static str> {
        let usbcmd = self.op_regs;
        unsafe {
            let cmd = read_volatile(usbcmd as *const u32);
            write_volatile(usbcmd as *mut u32, cmd | 0x01); // RS (Run/Stop)
        }

        // Wait for controller to start
        let usbsts = self.op_regs + 4;
        for _ in 0..1000 {
            let sts = unsafe { read_volatile(usbsts as *const u32) };
            if (sts & 0x1000) == 0 { // HCHalted cleared
                return Ok(());
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        Err("EHCI: Controller failed to start")
    }

    fn stop(&self) -> Result<(), &'static str> {
        let usbcmd = self.op_regs;
        unsafe {
            let cmd = read_volatile(usbcmd as *const u32);
            write_volatile(usbcmd as *mut u32, cmd & !0x01); // Clear RS
        }

        // Wait for controller to halt
        let usbsts = self.op_regs + 4;
        for _ in 0..1000 {
            let sts = unsafe { read_volatile(usbsts as *const u32) };
            if (sts & 0x1000) != 0 { // HCHalted set
                return Ok(());
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        Err("EHCI: Controller failed to stop")
    }

    fn port_count(&self) -> u8 {
        self.num_ports
    }

    fn port_status(&self, port: u8) -> PortStatus {
        if port == 0 || port > self.num_ports {
            return PortStatus {
                connected: false,
                enabled: false,
                suspended: false,
                reset: false,
                power: false,
                speed: UsbSpeed::Full,
                changed: false,
            };
        }

        let portsc_addr = self.op_regs + 0x44 + ((port - 1) as u64) * 4;
        let portsc = unsafe { read_volatile(portsc_addr as *const u32) };

        let connected = (portsc & PORTSC_CCS) != 0;
        let enabled = (portsc & PORTSC_PED) != 0;
        let suspended = (portsc & PORTSC_SUSPEND) != 0;
        let reset = (portsc & PORTSC_PRESET) != 0;
        let power = (portsc & PORTSC_PP) != 0;
        let changed = (portsc & PORTSC_CSC) != 0;

        // EHCI only handles high-speed devices directly
        let speed = if enabled {
            UsbSpeed::High
        } else {
            UsbSpeed::Full
        };

        PortStatus {
            connected,
            enabled,
            suspended,
            reset,
            power,
            speed,
            changed,
        }
    }

    fn port_reset(&self, port: u8) -> Result<(), &'static str> {
        if port == 0 || port > self.num_ports {
            return Err("EHCI: Invalid port number");
        }

        let portsc_addr = self.op_regs + 0x44 + ((port - 1) as u64) * 4;
        let portsc = unsafe { read_volatile(portsc_addr as *const u32) };

        // Start reset
        unsafe {
            write_volatile(portsc_addr as *mut u32, (portsc & !PORTSC_PED) | PORTSC_PRESET);
        }

        // Hold reset for 50ms
        for _ in 0..50000 {
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        // Clear reset
        let portsc = unsafe { read_volatile(portsc_addr as *const u32) };
        unsafe {
            write_volatile(portsc_addr as *mut u32, portsc & !PORTSC_PRESET);
        }

        // Wait for reset to complete
        for _ in 0..100 {
            let portsc = unsafe { read_volatile(portsc_addr as *const u32) };
            if (portsc & PORTSC_PRESET) == 0 {
                // Check if port enabled (high-speed device)
                if (portsc & PORTSC_PED) != 0 {
                    crate::log::info!("EHCI: Port {} reset complete, high-speed device", port);
                    return Ok(());
                } else {
                    // Full-speed device - release to companion
                    crate::log::info!("EHCI: Port {} full-speed, releasing to companion", port);
                    unsafe {
                        write_volatile(portsc_addr as *mut u32, portsc | PORTSC_PO);
                    }
                    return Err("EHCI: Device released to companion controller");
                }
            }
            for _ in 0..10000 { core::hint::spin_loop(); }
        }

        Err("EHCI: Port reset timeout")
    }

    fn port_enable(&self, port: u8) -> Result<(), &'static str> {
        if port == 0 || port > self.num_ports {
            return Err("EHCI: Invalid port number");
        }

        let portsc_addr = self.op_regs + 0x44 + ((port - 1) as u64) * 4;
        let portsc = unsafe { read_volatile(portsc_addr as *const u32) };

        // Enable port (set PED bit)
        unsafe {
            write_volatile(portsc_addr as *mut u32, portsc | PORTSC_PED);
        }

        Ok(())
    }

    fn port_disable(&self, port: u8) -> Result<(), &'static str> {
        if port == 0 || port > self.num_ports {
            return Err("EHCI: Invalid port number");
        }

        let portsc_addr = self.op_regs + 0x44 + ((port - 1) as u64) * 4;
        let portsc = unsafe { read_volatile(portsc_addr as *const u32) };

        // Disable port (clear PED bit)
        unsafe {
            write_volatile(portsc_addr as *mut u32, portsc & !PORTSC_PED);
        }

        Ok(())
    }

    fn allocate_address(&self) -> Option<u8> {
        let mut devices = self.devices.lock();
        for addr in 1..=127 {
            if !devices.contains_key(&addr) {
                devices.insert(addr, DeviceState::new(addr));
                return Some(addr);
            }
        }
        None
    }

    fn free_address(&self, address: u8) {
        let mut devices = self.devices.lock();
        devices.remove(&address);
    }

    fn control_transfer(
        &self,
        address: u8,
        setup: SetupPacket,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        let data = if buffer.is_empty() { None } else { Some(buffer) };
        self.do_control_transfer(address, setup, data)
            .map_err(|_| "EHCI: Control transfer failed")
    }

    fn bulk_transfer(
        &self,
        address: u8,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        let devices = self.devices.lock();
        let device = devices.get(&address).ok_or("EHCI: Device not found")?;

        // Create queue head for bulk transfer
        let mut qh = Box::new(QueueHead::new());
        qh.set_device_address(address);
        qh.set_endpoint(endpoint);
        qh.set_endpoint_speed(EHCI_SPEED_HIGH);
        qh.set_max_packet_length(512);
        qh.set_nak_reload(4);
        qh.set_mult(1);

        // Create data TD
        let mut td = Box::new(Qtd::new());
        td.set_buffer(buffer.as_ptr() as u64, buffer.len());
        td.set_pid(match direction {
            EndpointDirection::In => PID_IN,
            EndpointDirection::Out => PID_OUT,
        });
        td.set_cerr(3);
        td.set_status(0x80);
        td.set_ioc();

        // Get data toggle from device state
        let ep_idx = (endpoint & 0xF) as usize * 2 + if direction == EndpointDirection::In { 1 } else { 0 };
        td.set_data_toggle(device.toggle.get(ep_idx).copied().unwrap_or(false));

        qh.overlay.next = td.as_ref() as *const _ as u32;

        // Add to async schedule and wait
        drop(devices); // Release lock before waiting
        {
            let mut async_schedule = self.async_schedule.lock();
            async_schedule.add_queue_head(qh);
        }

        for _ in 0..10000 {
            if !td.is_active() {
                break;
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        if td.is_halted() {
            return Err("EHCI: Bulk transfer stalled");
        }

        if td.is_active() {
            return Err("EHCI: Bulk transfer timeout");
        }

        Ok(buffer.len() - td.total_bytes() as usize)
    }

    fn interrupt_transfer(
        &self,
        address: u8,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        // Similar to bulk but uses periodic schedule
        self.bulk_transfer(address, endpoint, direction, buffer)
    }

    fn configure_endpoint(
        &self,
        address: u8,
        endpoint: &EndpointDescriptor,
    ) -> Result<(), &'static str> {
        let mut devices = self.devices.lock();
        let device = devices.get_mut(&address).ok_or("EHCI: Device not found")?;

        // Store endpoint configuration
        let ep_num = endpoint.endpoint_address & 0x0F;
        let ep_dir = if (endpoint.endpoint_address & 0x80) != 0 { 1 } else { 0 };
        let ep_idx = (ep_num as usize) * 2 + ep_dir;

        // Initialize data toggle for this endpoint
        if ep_idx < device.toggle.len() {
            device.toggle[ep_idx] = false;
        }

        crate::log::debug!("EHCI: Configured endpoint {} for device {}", ep_num, address);
        Ok(())
    }
}

// =============================================================================
// PCI DEVICE DETECTION
// =============================================================================

/// Probe PCI for EHCI controllers
pub fn probe_pci() -> Vec<Arc<EhciController>> {
    let controllers = Vec::new();

    crate::log::info!("EHCI: Probing PCI for USB 2.0 controllers");

    // Scan PCI bus for EHCI controllers (class 0x0C, subclass 0x03, prog-if 0x20)
    // Would integrate with PCI subsystem

    controllers
}

/// Probe and initialize all EHCI controllers
pub fn probe_and_init() -> Result<(), &'static str> {
    let controllers = probe_pci();

    for ctrl in controllers {
        if let Err(e) = ctrl.init() {
            crate::log::warn!("EHCI: Failed to initialize controller: {}", e);
            continue;
        }

        if !ctrl.start() {
            crate::log::warn!("EHCI: Failed to start controller");
            continue;
        }

        crate::log::info!("EHCI: Controller initialized successfully");
    }

    Ok(())
}
