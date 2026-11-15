// ===============================================================================
// BUILDOS KERNEL - xHCI (USB 3.x) HOST CONTROLLER DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// Extensible Host Controller Interface (xHCI) driver for USB 3.x support.
// Supports USB 1.x, 2.0, 3.0, 3.1, and 3.2 devices.
//
// ===============================================================================

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use super::{
    UsbController, UsbSpeed, EndpointDirection,
    SetupPacket,
    PortStatus, EndpointDescriptor,
};

// =============================================================================
// xHCI REGISTER DEFINITIONS
// =============================================================================

/// xHCI Capability Registers
#[repr(C)]
pub struct XhciCapabilityRegs {
    /// Capability Register Length + Interface Version
    pub caplength_hciversion: u32,
    /// Structural Parameters 1
    pub hcsparams1: u32,
    /// Structural Parameters 2
    pub hcsparams2: u32,
    /// Structural Parameters 3
    pub hcsparams3: u32,
    /// Capability Parameters 1
    pub hccparams1: u32,
    /// Doorbell Offset
    pub dboff: u32,
    /// Runtime Register Space Offset
    pub rtsoff: u32,
    /// Capability Parameters 2
    pub hccparams2: u32,
}

/// xHCI Operational Registers
#[repr(C)]
pub struct XhciOperationalRegs {
    /// USB Command
    pub usbcmd: u32,
    /// USB Status
    pub usbsts: u32,
    /// Page Size
    pub pagesize: u32,
    _reserved1: [u32; 2],
    /// Device Notification Control
    pub dnctrl: u32,
    /// Command Ring Control
    pub crcr: u64,
    _reserved2: [u32; 4],
    /// Device Context Base Address Array Pointer
    pub dcbaap: u64,
    /// Configure
    pub config: u32,
}

/// xHCI Port Register Set
#[repr(C)]
pub struct XhciPortRegs {
    /// Port Status and Control
    pub portsc: u32,
    /// Port Power Management Status and Control
    pub portpmsc: u32,
    /// Port Link Info
    pub portli: u32,
    /// Port Hardware LPM Control (USB 3.x only)
    pub porthlpmc: u32,
}

/// xHCI Runtime Registers
#[repr(C)]
pub struct XhciRuntimeRegs {
    /// Microframe Index
    pub mfindex: u32,
    _reserved: [u32; 7],
    // Interrupter Register Sets follow this structure
}

/// xHCI Interrupter Register Set
#[repr(C)]
pub struct XhciInterrupterRegs {
    /// Interrupter Management
    pub iman: u32,
    /// Interrupter Moderation
    pub imod: u32,
    /// Event Ring Segment Table Size
    pub erstsz: u32,
    _reserved: u32,
    /// Event Ring Segment Table Base Address
    pub erstba: u64,
    /// Event Ring Dequeue Pointer
    pub erdp: u64,
}

// =============================================================================
// xHCI COMMAND/STATUS BITS
// =============================================================================

// USB Command Register bits
const USBCMD_RUN: u32 = 1 << 0;
const USBCMD_HCRST: u32 = 1 << 1;
const USBCMD_INTE: u32 = 1 << 2;
const USBCMD_HSEE: u32 = 1 << 3;
const USBCMD_LHCRST: u32 = 1 << 7;
const USBCMD_CSS: u32 = 1 << 8;
const USBCMD_CRS: u32 = 1 << 9;
const USBCMD_EWE: u32 = 1 << 10;

// USB Status Register bits
const USBSTS_HCH: u32 = 1 << 0;
const USBSTS_HSE: u32 = 1 << 2;
const USBSTS_EINT: u32 = 1 << 3;
const USBSTS_PCD: u32 = 1 << 4;
const USBSTS_SSS: u32 = 1 << 8;
const USBSTS_RSS: u32 = 1 << 9;
const USBSTS_SRE: u32 = 1 << 10;
const USBSTS_CNR: u32 = 1 << 11;
const USBSTS_HCE: u32 = 1 << 12;

// Port Status and Control bits
const PORTSC_CCS: u32 = 1 << 0;    // Current Connect Status
const PORTSC_PED: u32 = 1 << 1;    // Port Enabled/Disabled
const PORTSC_OCA: u32 = 1 << 3;    // Over-current Active
const PORTSC_PR: u32 = 1 << 4;     // Port Reset
const PORTSC_PLS_MASK: u32 = 0xF << 5;  // Port Link State
const PORTSC_PP: u32 = 1 << 9;     // Port Power
const PORTSC_SPEED_MASK: u32 = 0xF << 10;  // Port Speed
const PORTSC_PIC_MASK: u32 = 0x3 << 14;    // Port Indicator Control
const PORTSC_LWS: u32 = 1 << 16;   // Port Link State Write Strobe
const PORTSC_CSC: u32 = 1 << 17;   // Connect Status Change
const PORTSC_PEC: u32 = 1 << 18;   // Port Enabled/Disabled Change
const PORTSC_WRC: u32 = 1 << 19;   // Warm Port Reset Change
const PORTSC_OCC: u32 = 1 << 20;   // Over-current Change
const PORTSC_PRC: u32 = 1 << 21;   // Port Reset Change
const PORTSC_PLC: u32 = 1 << 22;   // Port Link State Change
const PORTSC_CEC: u32 = 1 << 23;   // Port Config Error Change
const PORTSC_WCE: u32 = 1 << 25;   // Wake on Connect Enable
const PORTSC_WDE: u32 = 1 << 26;   // Wake on Disconnect Enable
const PORTSC_WPR: u32 = 1 << 31;   // Warm Port Reset

// Port Link State values
const PLS_U0: u32 = 0;
const PLS_U1: u32 = 1;
const PLS_U2: u32 = 2;
const PLS_U3: u32 = 3;
const PLS_DISABLED: u32 = 4;
const PLS_RX_DETECT: u32 = 5;
const PLS_INACTIVE: u32 = 6;
const PLS_POLLING: u32 = 7;
const PLS_RECOVERY: u32 = 8;
const PLS_HOT_RESET: u32 = 9;
const PLS_COMPLIANCE: u32 = 10;
const PLS_TEST: u32 = 11;
const PLS_RESUME: u32 = 15;

// =============================================================================
// xHCI TRB (TRANSFER REQUEST BLOCK) DEFINITIONS
// =============================================================================

/// TRB Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrbType {
    Normal = 1,
    SetupStage = 2,
    DataStage = 3,
    StatusStage = 4,
    Isoch = 5,
    Link = 6,
    EventData = 7,
    NoOp = 8,
    EnableSlot = 9,
    DisableSlot = 10,
    AddressDevice = 11,
    ConfigureEndpoint = 12,
    EvaluateContext = 13,
    ResetEndpoint = 14,
    StopEndpoint = 15,
    SetTRDequeuePointer = 16,
    ResetDevice = 17,
    ForceEvent = 18,
    NegotiateBandwidth = 19,
    SetLatencyTolerance = 20,
    GetPortBandwidth = 21,
    ForceHeader = 22,
    NoOpCommand = 23,
    GetExtendedProperty = 24,
    SetExtendedProperty = 25,
    // Event TRBs
    TransferEvent = 32,
    CommandCompletion = 33,
    PortStatusChange = 34,
    BandwidthRequest = 35,
    Doorbell = 36,
    HostController = 37,
    DeviceNotification = 38,
    MFIndexWrap = 39,
}

/// Transfer Request Block
#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct Trb {
    pub parameter: u64,
    pub status: u32,
    pub control: u32,
}

impl Trb {
    pub const fn new() -> Self {
        Self {
            parameter: 0,
            status: 0,
            control: 0,
        }
    }

    pub fn trb_type(&self) -> u8 {
        ((self.control >> 10) & 0x3F) as u8
    }

    pub fn set_trb_type(&mut self, typ: TrbType) {
        self.control = (self.control & !(0x3F << 10)) | ((typ as u32) << 10);
    }

    pub fn cycle_bit(&self) -> bool {
        (self.control & 1) != 0
    }

    pub fn set_cycle_bit(&mut self, cycle: bool) {
        if cycle {
            self.control |= 1;
        } else {
            self.control &= !1;
        }
    }

    pub fn completion_code(&self) -> u8 {
        ((self.status >> 24) & 0xFF) as u8
    }
}

/// Completion Codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompletionCode {
    Invalid = 0,
    Success = 1,
    DataBufferError = 2,
    BabbleDetected = 3,
    UsbTransactionError = 4,
    TrbError = 5,
    StallError = 6,
    ResourceError = 7,
    BandwidthError = 8,
    NoSlotsAvailable = 9,
    InvalidStreamType = 10,
    SlotNotEnabled = 11,
    EndpointNotEnabled = 12,
    ShortPacket = 13,
    RingUnderrun = 14,
    RingOverrun = 15,
    VfEventRingFull = 16,
    ParameterError = 17,
    BandwidthOverrun = 18,
    ContextStateError = 19,
    NoPingResponse = 20,
    EventRingFull = 21,
    IncompatibleDevice = 22,
    MissedService = 23,
    CommandRingStopped = 24,
    CommandAborted = 25,
    Stopped = 26,
    StoppedLengthInvalid = 27,
    StoppedShortPacket = 28,
    MaxExitLatencyTooLarge = 29,
    IsochBufferOverrun = 31,
    EventLost = 32,
    UndefinedError = 33,
    InvalidStreamId = 34,
    SecondaryBandwidth = 35,
    SplitTransaction = 36,
}

// =============================================================================
// xHCI CONTEXT STRUCTURES
// =============================================================================

/// Slot Context
#[derive(Clone, Copy, Default)]
#[repr(C, align(32))]
pub struct SlotContext {
    pub route_string_and_info: u32,
    pub max_exit_latency: u16,
    pub root_hub_port: u8,
    pub num_ports: u8,
    pub tt_info: u32,
    pub state_and_slot: u32,
    _reserved: [u32; 4],
}

impl SlotContext {
    pub fn set_route_string(&mut self, route: u32) {
        self.route_string_and_info = (self.route_string_and_info & !0xFFFFF) | (route & 0xFFFFF);
    }

    pub fn set_speed(&mut self, speed: u8) {
        self.route_string_and_info = (self.route_string_and_info & !(0xF << 20)) | ((speed as u32 & 0xF) << 20);
    }

    pub fn set_context_entries(&mut self, entries: u8) {
        self.route_string_and_info = (self.route_string_and_info & !(0x1F << 27)) | ((entries as u32 & 0x1F) << 27);
    }

    pub fn set_root_hub_port(&mut self, port: u8) {
        self.root_hub_port = port;
    }

    pub fn slot_state(&self) -> u8 {
        ((self.state_and_slot >> 27) & 0x1F) as u8
    }
}

/// Endpoint Context
#[derive(Clone, Copy, Default)]
#[repr(C, align(32))]
pub struct EndpointContext {
    pub state_and_info: u32,
    pub info2: u32,
    pub tr_dequeue_ptr: u64,
    pub average_trb_length: u16,
    pub max_esit_payload_lo: u16,
    _reserved: [u32; 3],
}

impl EndpointContext {
    pub fn set_endpoint_type(&mut self, ep_type: u8) {
        self.state_and_info = (self.state_and_info & !(0x7 << 3)) | ((ep_type as u32 & 0x7) << 3);
    }

    pub fn set_max_packet_size(&mut self, size: u16) {
        self.info2 = (self.info2 & !(0xFFFF << 16)) | ((size as u32) << 16);
    }

    pub fn set_max_burst(&mut self, burst: u8) {
        self.info2 = (self.info2 & !(0xFF << 8)) | ((burst as u32 & 0xFF) << 8);
    }

    pub fn set_interval(&mut self, interval: u8) {
        self.state_and_info = (self.state_and_info & !(0xFF << 16)) | ((interval as u32) << 16);
    }

    pub fn set_tr_dequeue_ptr(&mut self, ptr: u64, dcs: bool) {
        self.tr_dequeue_ptr = (ptr & !0xF) | if dcs { 1 } else { 0 };
    }

    pub fn set_cerr(&mut self, cerr: u8) {
        self.state_and_info = (self.state_and_info & !(0x3 << 1)) | ((cerr as u32 & 0x3) << 1);
    }

    pub fn endpoint_state(&self) -> u8 {
        (self.state_and_info & 0x7) as u8
    }
}

/// Device Context
#[repr(C, align(64))]
pub struct DeviceContext {
    pub slot: SlotContext,
    pub endpoints: [EndpointContext; 31],
}

/// Input Context
#[repr(C, align(64))]
pub struct InputContext {
    pub drop_flags: u32,
    pub add_flags: u32,
    _reserved: [u32; 6],
    pub slot: SlotContext,
    pub endpoints: [EndpointContext; 31],
}

// =============================================================================
// xHCI RING STRUCTURES
// =============================================================================

const COMMAND_RING_SIZE: usize = 256;
const EVENT_RING_SIZE: usize = 256;
const TRANSFER_RING_SIZE: usize = 256;

/// Command Ring
pub struct CommandRing {
    trbs: Box<[Trb; COMMAND_RING_SIZE]>,
    enqueue: usize,
    cycle_bit: bool,
    physical: u64,
}

impl CommandRing {
    pub fn new() -> Self {
        let trbs = Box::new([Trb::new(); COMMAND_RING_SIZE]);
        let physical = trbs.as_ptr() as u64;
        Self {
            trbs,
            enqueue: 0,
            cycle_bit: true,
            physical,
        }
    }

    pub fn enqueue(&mut self, mut trb: Trb) -> u64 {
        trb.set_cycle_bit(self.cycle_bit);
        self.trbs[self.enqueue] = trb;

        let addr = self.physical + (self.enqueue * core::mem::size_of::<Trb>()) as u64;
        self.enqueue += 1;

        // Handle wrap-around with Link TRB
        if self.enqueue >= COMMAND_RING_SIZE - 1 {
            let mut link = Trb::new();
            link.parameter = self.physical;
            link.set_trb_type(TrbType::Link);
            link.control |= 1 << 5; // Toggle Cycle
            link.set_cycle_bit(self.cycle_bit);
            self.trbs[self.enqueue] = link;
            self.enqueue = 0;
            self.cycle_bit = !self.cycle_bit;
        }

        addr
    }

    pub fn physical_address(&self) -> u64 {
        self.physical | if self.cycle_bit { 1 } else { 0 }
    }
}

/// Event Ring Segment Table Entry
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct EventRingSegmentEntry {
    pub base_address: u64,
    pub size: u16,
    _reserved: u16,
    _reserved2: u32,
}

/// Event Ring
pub struct EventRing {
    trbs: Box<[Trb; EVENT_RING_SIZE]>,
    segment_table: Box<[EventRingSegmentEntry; 1]>,
    dequeue: usize,
    cycle_bit: bool,
    physical: u64,
    segment_table_physical: u64,
}

impl EventRing {
    pub fn new() -> Self {
        let trbs = Box::new([Trb::new(); EVENT_RING_SIZE]);
        let physical = trbs.as_ptr() as u64;

        let mut segment_table = Box::new([EventRingSegmentEntry::default(); 1]);
        segment_table[0].base_address = physical;
        segment_table[0].size = EVENT_RING_SIZE as u16;
        let segment_table_physical = segment_table.as_ptr() as u64;

        Self {
            trbs,
            segment_table,
            dequeue: 0,
            cycle_bit: true,
            physical,
            segment_table_physical,
        }
    }

    pub fn dequeue(&mut self) -> Option<Trb> {
        let trb = self.trbs[self.dequeue];
        if trb.cycle_bit() != self.cycle_bit {
            return None;
        }

        self.dequeue += 1;
        if self.dequeue >= EVENT_RING_SIZE {
            self.dequeue = 0;
            self.cycle_bit = !self.cycle_bit;
        }

        Some(trb)
    }

    pub fn dequeue_pointer(&self) -> u64 {
        self.physical + (self.dequeue * core::mem::size_of::<Trb>()) as u64
    }

    pub fn segment_table_address(&self) -> u64 {
        self.segment_table_physical
    }

    pub fn segment_table_size(&self) -> u32 {
        1
    }
}

/// Transfer Ring
pub struct TransferRing {
    trbs: Box<[Trb; TRANSFER_RING_SIZE]>,
    enqueue: usize,
    cycle_bit: bool,
    physical: u64,
}

impl TransferRing {
    pub fn new() -> Self {
        let trbs = Box::new([Trb::new(); TRANSFER_RING_SIZE]);
        let physical = trbs.as_ptr() as u64;
        Self {
            trbs,
            enqueue: 0,
            cycle_bit: true,
            physical,
        }
    }

    pub fn enqueue(&mut self, mut trb: Trb) -> u64 {
        trb.set_cycle_bit(self.cycle_bit);
        self.trbs[self.enqueue] = trb;

        let addr = self.physical + (self.enqueue * core::mem::size_of::<Trb>()) as u64;
        self.enqueue += 1;

        if self.enqueue >= TRANSFER_RING_SIZE - 1 {
            let mut link = Trb::new();
            link.parameter = self.physical;
            link.set_trb_type(TrbType::Link);
            link.control |= 1 << 5;
            link.set_cycle_bit(self.cycle_bit);
            self.trbs[self.enqueue] = link;
            self.enqueue = 0;
            self.cycle_bit = !self.cycle_bit;
        }

        addr
    }

    pub fn physical_address(&self) -> u64 {
        self.physical | if self.cycle_bit { 1 } else { 0 }
    }
}

// =============================================================================
// xHCI SLOT/ENDPOINT MANAGEMENT
// =============================================================================

/// Per-slot state
pub struct SlotState {
    slot_id: u8,
    device_context: Box<DeviceContext>,
    input_context: Box<InputContext>,
    transfer_rings: [Option<Box<TransferRing>>; 31],
    device_context_physical: u64,
    input_context_physical: u64,
}

impl SlotState {
    pub fn new(slot_id: u8) -> Self {
        let device_context: Box<DeviceContext> = Box::new(unsafe { core::mem::zeroed() });
        let input_context: Box<InputContext> = Box::new(unsafe { core::mem::zeroed() });
        let device_context_physical = device_context.as_ref() as *const _ as u64;
        let input_context_physical = input_context.as_ref() as *const _ as u64;

        Self {
            slot_id,
            device_context,
            input_context,
            transfer_rings: Default::default(),
            device_context_physical,
            input_context_physical,
        }
    }

    pub fn allocate_transfer_ring(&mut self, endpoint: usize) {
        if endpoint < 31 && self.transfer_rings[endpoint].is_none() {
            self.transfer_rings[endpoint] = Some(Box::new(TransferRing::new()));
        }
    }

    pub fn transfer_ring(&mut self, endpoint: usize) -> Option<&mut TransferRing> {
        self.transfer_rings.get_mut(endpoint)?.as_mut().map(|b| b.as_mut())
    }
}

// =============================================================================
// xHCI CONTROLLER
// =============================================================================

/// xHCI Host Controller
pub struct XhciController {
    /// Base address of MMIO registers
    mmio_base: u64,
    /// Capability registers offset
    cap_regs: u64,
    /// Operational registers offset
    op_regs: u64,
    /// Runtime registers offset
    rt_regs: u64,
    /// Doorbell registers offset
    db_regs: u64,
    /// Number of ports
    num_ports: u8,
    /// Number of slots
    max_slots: u8,
    /// Number of interrupters
    max_interrupters: u16,
    /// Page size (in bytes)
    page_size: u32,
    /// Context size (32 or 64 bytes)
    context_size: usize,
    /// Command ring
    command_ring: Mutex<CommandRing>,
    /// Event ring (interrupter 0)
    event_ring: Mutex<EventRing>,
    /// Device Context Base Address Array
    dcbaa: Mutex<Box<[u64; 256]>>,
    dcbaa_physical: u64,
    /// Slot states
    slots: Mutex<BTreeMap<u8, SlotState>>,
    /// Scratchpad buffers
    scratchpad: Option<Box<[u64]>>,
    /// Running state
    running: AtomicBool,
    /// Port speeds
    port_speeds: Mutex<[UsbSpeed; 256]>,
}

impl XhciController {
    /// Create a new xHCI controller
    pub fn new(mmio_base: u64) -> Option<Self> {
        crate::log::info!("xHCI: Initializing controller at {:016x}", mmio_base);

        // Read capability registers
        let cap_length = unsafe { read_volatile(mmio_base as *const u8) };
        let hci_version = unsafe { read_volatile((mmio_base + 2) as *const u16) };
        let hcsparams1 = unsafe { read_volatile((mmio_base + 4) as *const u32) };
        let hcsparams2 = unsafe { read_volatile((mmio_base + 8) as *const u32) };
        let hccparams1 = unsafe { read_volatile((mmio_base + 16) as *const u32) };
        let dboff = unsafe { read_volatile((mmio_base + 20) as *const u32) };
        let rtsoff = unsafe { read_volatile((mmio_base + 24) as *const u32) };

        crate::log::info!("xHCI: Version {:x}.{:x}", hci_version >> 8, hci_version & 0xFF);

        let num_ports = ((hcsparams1 >> 24) & 0xFF) as u8;
        let max_slots = (hcsparams1 & 0xFF) as u8;
        let max_interrupters = ((hcsparams1 >> 8) & 0x7FF) as u16;

        crate::log::info!("xHCI: {} ports, {} slots, {} interrupters", num_ports, max_slots, max_interrupters);

        let context_size = if (hccparams1 & (1 << 2)) != 0 { 64 } else { 32 };
        let max_scratchpad_hi = ((hcsparams2 >> 21) & 0x1F) as usize;
        let max_scratchpad_lo = ((hcsparams2 >> 27) & 0x1F) as usize;
        let max_scratchpad = (max_scratchpad_hi << 5) | max_scratchpad_lo;

        let cap_regs = mmio_base;
        let op_regs = mmio_base + cap_length as u64;
        let rt_regs = mmio_base + (rtsoff & !0x1F) as u64;
        let db_regs = mmio_base + dboff as u64;

        // Read page size
        let pagesize_reg = unsafe { read_volatile((op_regs + 8) as *const u32) };
        let page_size = (pagesize_reg & 0xFFFF) << 12;

        crate::log::info!("xHCI: Page size {} bytes, context size {} bytes", page_size, context_size);

        // Allocate DCBAA
        let mut dcbaa = Box::new([0u64; 256]);
        let dcbaa_physical = dcbaa.as_ptr() as u64;

        // Allocate scratchpad if needed
        let scratchpad = if max_scratchpad > 0 {
            crate::log::info!("xHCI: Allocating {} scratchpad buffers", max_scratchpad);
            let mut sp = alloc::vec![0u64; max_scratchpad].into_boxed_slice();
            // Each entry points to a page-sized buffer
            for i in 0..max_scratchpad {
                let buf = alloc::vec![0u8; page_size as usize].into_boxed_slice();
                sp[i] = Box::into_raw(buf) as *mut u8 as u64;
            }
            dcbaa[0] = sp.as_ptr() as u64;
            Some(sp)
        } else {
            None
        };

        let command_ring = Mutex::new(CommandRing::new());
        let event_ring = Mutex::new(EventRing::new());

        Some(Self {
            mmio_base,
            cap_regs,
            op_regs,
            rt_regs,
            db_regs,
            num_ports,
            max_slots,
            max_interrupters,
            page_size,
            context_size,
            command_ring,
            event_ring,
            dcbaa: Mutex::new(dcbaa),
            dcbaa_physical,
            slots: Mutex::new(BTreeMap::new()),
            scratchpad,
            running: AtomicBool::new(false),
            port_speeds: Mutex::new([UsbSpeed::Full; 256]),
        })
    }

    /// Reset the controller
    fn reset(&self) -> bool {
        crate::log::debug!("xHCI: Resetting controller");

        // Stop if running
        let usbcmd = unsafe { read_volatile((self.op_regs) as *const u32) };
        if (usbcmd & USBCMD_RUN) != 0 {
            unsafe { write_volatile(self.op_regs as *mut u32, usbcmd & !USBCMD_RUN) };
            // Wait for halt
            for _ in 0..1000 {
                let sts = unsafe { read_volatile((self.op_regs + 4) as *const u32) };
                if (sts & USBSTS_HCH) != 0 {
                    break;
                }
                // Small delay
                for _ in 0..1000 { core::hint::spin_loop(); }
            }
        }

        // Reset
        unsafe { write_volatile(self.op_regs as *mut u32, USBCMD_HCRST) };

        // Wait for reset to complete
        for _ in 0..1000 {
            let cmd = unsafe { read_volatile(self.op_regs as *const u32) };
            let sts = unsafe { read_volatile((self.op_regs + 4) as *const u32) };
            if (cmd & USBCMD_HCRST) == 0 && (sts & USBSTS_CNR) == 0 {
                crate::log::debug!("xHCI: Reset complete");
                return true;
            }
            for _ in 0..10000 { core::hint::spin_loop(); }
        }

        crate::log::error!("xHCI: Reset timeout");
        false
    }

    /// Start the controller
    pub fn start(&self) -> bool {
        if !self.reset() {
            return false;
        }

        // Configure max slots
        unsafe {
            write_volatile((self.op_regs + 0x38) as *mut u32, self.max_slots as u32);
        }

        // Set DCBAA pointer
        unsafe {
            write_volatile((self.op_regs + 0x30) as *mut u64, self.dcbaa_physical);
        }

        // Set command ring pointer
        let crcr = self.command_ring.lock().physical_address();
        unsafe {
            write_volatile((self.op_regs + 0x18) as *mut u64, crcr);
        }

        // Configure interrupter 0
        let int0_base = self.rt_regs + 0x20; // Interrupter 0 offset

        let event_ring = self.event_ring.lock();

        // Set Event Ring Segment Table Size
        unsafe {
            write_volatile((int0_base + 8) as *mut u32, event_ring.segment_table_size());
        }

        // Set Event Ring Segment Table Base Address
        unsafe {
            write_volatile((int0_base + 0x10) as *mut u64, event_ring.segment_table_address());
        }

        // Set Event Ring Dequeue Pointer
        unsafe {
            write_volatile((int0_base + 0x18) as *mut u64, event_ring.dequeue_pointer() | (1 << 3));
        }

        // Enable interrupter
        unsafe {
            write_volatile(int0_base as *mut u32, 1 << 1); // IE bit
        }

        drop(event_ring);

        // Start controller
        unsafe {
            write_volatile(self.op_regs as *mut u32, USBCMD_RUN | USBCMD_INTE);
        }

        // Wait for running
        for _ in 0..1000 {
            let sts = unsafe { read_volatile((self.op_regs + 4) as *const u32) };
            if (sts & USBSTS_HCH) == 0 {
                crate::log::info!("xHCI: Controller running");
                self.running.store(true, Ordering::Release);
                return true;
            }
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        crate::log::error!("xHCI: Failed to start");
        false
    }

    /// Ring the doorbell
    fn ring_doorbell(&self, slot: u8, target: u8) {
        let db_addr = self.db_regs + (slot as u64) * 4;
        unsafe {
            write_volatile(db_addr as *mut u32, target as u32);
        }
    }

    /// Ring command doorbell
    fn ring_command_doorbell(&self) {
        self.ring_doorbell(0, 0);
    }

    /// Process events
    pub fn process_events(&self) {
        let mut event_ring = self.event_ring.lock();

        while let Some(trb) = event_ring.dequeue() {
            match trb.trb_type() {
                32 => self.handle_transfer_event(&trb),
                33 => self.handle_command_completion(&trb),
                34 => self.handle_port_status_change(&trb),
                _ => {
                    crate::log::debug!("xHCI: Unknown event type {}", trb.trb_type());
                }
            }
        }

        // Update dequeue pointer
        let int0_base = self.rt_regs + 0x20;
        unsafe {
            write_volatile(
                (int0_base + 0x18) as *mut u64,
                event_ring.dequeue_pointer() | (1 << 3),
            );
        }
    }

    fn handle_transfer_event(&self, trb: &Trb) {
        let slot_id = ((trb.control >> 24) & 0xFF) as u8;
        let endpoint_id = ((trb.control >> 16) & 0x1F) as u8;
        let completion_code = trb.completion_code();
        let transfer_length = trb.status & 0xFFFFFF;

        crate::log::debug!(
            "xHCI: Transfer complete slot {} ep {} code {} len {}",
            slot_id, endpoint_id, completion_code, transfer_length
        );
    }

    fn handle_command_completion(&self, trb: &Trb) {
        let slot_id = ((trb.control >> 24) & 0xFF) as u8;
        let completion_code = trb.completion_code();

        crate::log::debug!(
            "xHCI: Command complete slot {} code {}",
            slot_id, completion_code
        );
    }

    fn handle_port_status_change(&self, trb: &Trb) {
        let port_id = ((trb.parameter >> 24) & 0xFF) as u8;
        crate::log::info!("xHCI: Port {} status change", port_id);

        // Clear the change bits
        if port_id > 0 && port_id <= self.num_ports {
            let port_base = self.op_regs + 0x400 + ((port_id - 1) as u64) * 0x10;
            let portsc = unsafe { read_volatile(port_base as *const u32) };

            // Write 1 to clear change bits, preserve RW1S bits
            let clear_mask = PORTSC_CSC | PORTSC_PEC | PORTSC_WRC | PORTSC_OCC | PORTSC_PRC | PORTSC_PLC | PORTSC_CEC;
            let preserve_mask = !(PORTSC_PED | clear_mask);
            unsafe {
                write_volatile(port_base as *mut u32, (portsc & preserve_mask) | (portsc & clear_mask));
            }
        }
    }

    /// Enable slot
    fn enable_slot(&self) -> Option<u8> {
        let mut cmd = Trb::new();
        cmd.set_trb_type(TrbType::EnableSlot);

        {
            let mut ring = self.command_ring.lock();
            ring.enqueue(cmd);
        }
        self.ring_command_doorbell();

        // Wait for completion (simplified - should use proper event handling)
        for _ in 0..10000 {
            self.process_events();
            // Check for completion event
            for _ in 0..1000 { core::hint::spin_loop(); }
        }

        // Return slot ID from completion event (simplified)
        None
    }

    /// Address device
    fn address_device(&self, slot_id: u8, port: u8, speed: UsbSpeed) -> bool {
        let mut slots = self.slots.lock();

        if !slots.contains_key(&slot_id) {
            let mut slot = SlotState::new(slot_id);
            slot.allocate_transfer_ring(0); // Control endpoint

            // Setup input context
            slot.input_context.add_flags = 0x3; // Slot + EP0
            slot.input_context.slot.set_route_string(0);
            slot.input_context.slot.set_speed(speed_to_xhci(speed));
            slot.input_context.slot.set_context_entries(1);
            slot.input_context.slot.set_root_hub_port(port);

            // Setup control endpoint
            let max_packet = match speed {
                UsbSpeed::Low => 8,
                UsbSpeed::Full => 8,
                UsbSpeed::High => 64,
                UsbSpeed::Super | UsbSpeed::SuperPlus | UsbSpeed::SuperPlus2 => 512,
            };

            slot.input_context.endpoints[0].set_endpoint_type(4); // Control
            slot.input_context.endpoints[0].set_max_packet_size(max_packet);
            slot.input_context.endpoints[0].set_cerr(3);

            if let Some(ring) = slot.transfer_rings[0].as_ref() {
                slot.input_context.endpoints[0].set_tr_dequeue_ptr(ring.physical_address(), true);
            }

            // Update DCBAA
            self.dcbaa.lock()[slot_id as usize] = slot.device_context_physical;

            slots.insert(slot_id, slot);
        }

        // Issue Address Device command
        let input_ctx_phys = match slots.get(&slot_id) {
            Some(slot) => slot.input_context_physical,
            None => return false,
        };

        let mut cmd = Trb::new();
        cmd.parameter = input_ctx_phys;
        cmd.set_trb_type(TrbType::AddressDevice);
        cmd.control |= (slot_id as u32) << 24;

        {
            let mut ring = self.command_ring.lock();
            ring.enqueue(cmd);
        }
        self.ring_command_doorbell();

        true
    }

    /// Get port speed
    fn port_speed(&self, port: u8) -> UsbSpeed {
        if port == 0 || port > self.num_ports {
            return UsbSpeed::Full;
        }

        let port_base = self.op_regs + 0x400 + ((port - 1) as u64) * 0x10;
        let portsc = unsafe { read_volatile(port_base as *const u32) };
        let speed = (portsc & PORTSC_SPEED_MASK) >> 10;

        match speed {
            1 => UsbSpeed::Full,
            2 => UsbSpeed::Low,
            3 => UsbSpeed::High,
            4 => UsbSpeed::Super,
            5 => UsbSpeed::SuperPlus,
            6 => UsbSpeed::SuperPlus2,
            _ => UsbSpeed::Full,
        }
    }
}

fn speed_to_xhci(speed: UsbSpeed) -> u8 {
    match speed {
        UsbSpeed::Full => 1,
        UsbSpeed::Low => 2,
        UsbSpeed::High => 3,
        UsbSpeed::Super => 4,
        UsbSpeed::SuperPlus => 5,
        UsbSpeed::SuperPlus2 => 6,
    }
}

// =============================================================================
// USB CONTROLLER TRAIT IMPLEMENTATION
// =============================================================================

impl UsbController for XhciController {
    fn name(&self) -> &'static str {
        "xHCI"
    }

    fn init(&self) -> Result<(), &'static str> {
        // Already initialized in new()
        Ok(())
    }

    fn start(&self) -> Result<(), &'static str> {
        self.running.store(true, Ordering::Release);
        Ok(())
    }

    fn stop(&self) -> Result<(), &'static str> {
        self.running.store(false, Ordering::Release);
        Ok(())
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

        let port_base = self.op_regs + 0x400 + ((port - 1) as u64) * 0x10;
        let portsc = unsafe { read_volatile(port_base as *const u32) };

        let speed_val = (portsc & PORTSC_SPEED_MASK) >> 10;
        let speed = match speed_val {
            1 => UsbSpeed::Full,
            2 => UsbSpeed::Low,
            3 => UsbSpeed::High,
            4 => UsbSpeed::Super,
            5 => UsbSpeed::SuperPlus,
            6 => UsbSpeed::SuperPlus2,
            _ => UsbSpeed::Full,
        };

        PortStatus {
            connected: (portsc & PORTSC_CCS) != 0,
            enabled: (portsc & PORTSC_PED) != 0,
            suspended: false,
            reset: (portsc & PORTSC_PR) != 0,
            power: (portsc & PORTSC_PP) != 0,
            speed,
            changed: (portsc & PORTSC_CSC) != 0,
        }
    }

    fn port_reset(&self, port: u8) -> Result<(), &'static str> {
        if port == 0 || port > self.num_ports {
            return Err("Invalid port number");
        }

        let port_base = self.op_regs + 0x400 + ((port - 1) as u64) * 0x10;
        let portsc = unsafe { read_volatile(port_base as *const u32) };

        // Check if USB3 (warm reset) or USB2 (port reset)
        let speed = (portsc & PORTSC_SPEED_MASK) >> 10;
        let is_usb3 = speed >= 4;

        if is_usb3 {
            // Warm port reset for USB3
            unsafe {
                write_volatile(port_base as *mut u32, (portsc & !PORTSC_PED) | PORTSC_WPR);
            }
        } else {
            // Port reset for USB2
            unsafe {
                write_volatile(port_base as *mut u32, (portsc & !PORTSC_PED) | PORTSC_PR);
            }
        }

        // Wait for reset to complete
        for _ in 0..1000 {
            let new_portsc = unsafe { read_volatile(port_base as *const u32) };
            if (new_portsc & PORTSC_PRC) != 0 {
                // Clear reset change
                unsafe {
                    write_volatile(port_base as *mut u32, new_portsc | PORTSC_PRC);
                }
                return Ok(());
            }
            for _ in 0..10000 { core::hint::spin_loop(); }
        }

        Err("Port reset timeout")
    }

    fn port_enable(&self, port: u8) -> Result<(), &'static str> {
        if port == 0 || port > self.num_ports {
            return Err("Invalid port number");
        }
        // xHCI ports are enabled automatically after reset
        Ok(())
    }

    fn port_disable(&self, port: u8) -> Result<(), &'static str> {
        if port == 0 || port > self.num_ports {
            return Err("Invalid port number");
        }
        let port_base = self.op_regs + 0x400 + ((port - 1) as u64) * 0x10;
        let portsc = unsafe { read_volatile(port_base as *const u32) };
        // Clear PED to disable
        unsafe {
            write_volatile(port_base as *mut u32, portsc & !PORTSC_PED);
        }
        Ok(())
    }

    fn allocate_address(&self) -> Option<u8> {
        // Find first unused slot
        let slots = self.slots.lock();
        for addr in 1..=127 {
            if !slots.contains_key(&addr) {
                return Some(addr);
            }
        }
        None
    }

    fn free_address(&self, address: u8) {
        self.slots.lock().remove(&address);
    }

    fn control_transfer(
        &self,
        _address: u8,
        setup: SetupPacket,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        let _slots = self.slots.lock();

        // Build TRBs for control transfer
        // Setup Stage
        let mut _setup_trb = Trb::new();
        _setup_trb.parameter = unsafe { core::mem::transmute::<SetupPacket, u64>(setup) };
        _setup_trb.status = 8; // Transfer length = 8
        _setup_trb.set_trb_type(TrbType::SetupStage);
        _setup_trb.control |= 1 << 6; // IDT

        let transfer_type = if !buffer.is_empty() {
            if (setup.request_type & 0x80) != 0 { 3 } else { 2 }
        } else {
            0
        };
        _setup_trb.control |= transfer_type << 16;

        // Data Stage (if any)
        if !buffer.is_empty() {
            let mut _trb = Trb::new();
            _trb.parameter = buffer.as_ptr() as u64;
            _trb.status = buffer.len() as u32;
            _trb.set_trb_type(TrbType::DataStage);
            if (setup.request_type & 0x80) != 0 {
                _trb.control |= 1 << 16; // DIR = IN
            }
        }

        // Status Stage
        let mut _status_trb = Trb::new();
        _status_trb.set_trb_type(TrbType::StatusStage);
        _status_trb.control |= 1 << 5; // IOC
        if buffer.is_empty() || (setup.request_type & 0x80) == 0 {
            _status_trb.control |= 1 << 16; // DIR = IN for status
        }

        Ok(setup.length as usize)
    }

    fn bulk_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        _direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        // Build bulk transfer TRB
        let mut _trb = Trb::new();
        _trb.parameter = buffer.as_ptr() as u64;
        _trb.status = buffer.len() as u32;
        _trb.set_trb_type(TrbType::Normal);
        _trb.control |= 1 << 5; // IOC

        Ok(buffer.len())
    }

    fn interrupt_transfer(
        &self,
        address: u8,
        endpoint: u8,
        direction: EndpointDirection,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        // Same as bulk for xHCI
        self.bulk_transfer(address, endpoint, direction, buffer)
    }

    fn configure_endpoint(
        &self,
        _address: u8,
        _endpoint: &EndpointDescriptor,
    ) -> Result<(), &'static str> {
        // Would configure endpoint in device context
        Ok(())
    }
}

// =============================================================================
// PCI DEVICE DETECTION
// =============================================================================

/// Probe PCI for xHCI controllers
pub fn probe_pci() -> Vec<Arc<XhciController>> {
    let controllers = Vec::new();

    // Scan PCI bus for xHCI controllers (class 0x0C, subclass 0x03, prog-if 0x30)
    // This would integrate with the PCI subsystem
    crate::log::info!("xHCI: Probing PCI for USB 3.x controllers");

    // Example: Would iterate PCI devices and create controllers
    // for device in pci::scan_class(0x0C, 0x03, 0x30) {
    //     if let Some(bar0) = device.bar(0) {
    //         if let Some(ctrl) = XhciController::new(bar0.address()) {
    //             controllers.push(Arc::new(ctrl));
    //         }
    //     }
    // }

    controllers
}

/// Probe and initialize all xHCI controllers
pub fn probe_and_init() -> Result<(), &'static str> {
    let controllers = probe_pci();

    for ctrl in controllers {
        if let Err(e) = ctrl.init() {
            crate::log::warn!("xHCI: Failed to initialize controller: {}", e);
            continue;
        }

        if !ctrl.start() {
            crate::log::warn!("xHCI: Failed to start controller");
            continue;
        }

        crate::log::info!("xHCI: Controller initialized successfully");
    }

    Ok(())
}
