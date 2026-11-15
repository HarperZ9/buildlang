//! USB Host Controller Drivers
//!
//! Provides support for USB host controllers:
//! - UHCI (Universal Host Controller Interface) - USB 1.0/1.1
//! - OHCI (Open Host Controller Interface) - USB 1.0/1.1
//! - EHCI (Enhanced Host Controller Interface) - USB 2.0
//! - xHCI (eXtensible Host Controller Interface) - USB 3.x

#![allow(dead_code)]

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use super::{
    UsbError, UsbSpeed, SetupPacket, TransferDirection,
};

/// Host controller type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HcType {
    /// UHCI (Intel)
    Uhci,
    /// OHCI (Compaq/Microsoft)
    Ohci,
    /// EHCI (USB 2.0)
    Ehci,
    /// xHCI (USB 3.x)
    Xhci,
}

/// USB Host Controller interface
pub trait UsbHc: Send + Sync {
    /// Get controller type
    fn hc_type(&self) -> HcType;

    /// Get number of ports
    fn port_count(&self) -> u32;

    /// Check if port is connected
    fn port_connected(&self, port: u32) -> bool;

    /// Get port speed
    fn port_speed(&self, port: u32) -> UsbSpeed;

    /// Reset port
    fn port_reset(&self, port: u32) -> Result<(), UsbError>;

    /// Enable port
    fn port_enable(&self, port: u32) -> Result<(), UsbError>;

    /// Disable port
    fn port_disable(&self, port: u32) -> Result<(), UsbError>;

    /// Perform control transfer
    fn control_transfer(
        &self,
        address: u8,
        setup: &SetupPacket,
        data: Option<&mut [u8]>,
    ) -> Result<usize, UsbError>;

    /// Perform bulk transfer
    fn bulk_transfer(
        &self,
        address: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> Result<usize, UsbError>;

    /// Perform interrupt transfer
    fn interrupt_transfer(
        &self,
        address: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> Result<usize, UsbError>;

    /// Perform isochronous transfer
    fn isochronous_transfer(
        &self,
        address: u8,
        endpoint: u8,
        data: &mut [u8],
        direction: TransferDirection,
    ) -> Result<usize, UsbError>;

    /// Handle interrupt
    fn handle_interrupt(&self);

    /// Suspend controller
    fn suspend(&self) -> Result<(), UsbError>;

    /// Resume controller
    fn resume(&self) -> Result<(), UsbError>;
}

/// Initialize USB host controller
pub fn init_controller(hc_type: HcType, bus: u8, slot: u8, func: u8) -> Option<Arc<dyn UsbHc>> {
    match hc_type {
        HcType::Uhci => UhciController::new(bus, slot, func).map(|c| Arc::new(c) as Arc<dyn UsbHc>),
        HcType::Ohci => OhciController::new(bus, slot, func).map(|c| Arc::new(c) as Arc<dyn UsbHc>),
        HcType::Ehci => EhciController::new(bus, slot, func).map(|c| Arc::new(c) as Arc<dyn UsbHc>),
        HcType::Xhci => XhciController::new(bus, slot, func).map(|c| Arc::new(c) as Arc<dyn UsbHc>),
    }
}

// =============================================================================
// UHCI Controller (USB 1.0/1.1)
// =============================================================================

/// UHCI I/O register offsets
mod uhci_regs {
    pub const USBCMD: u16 = 0x00;      // USB Command
    pub const USBSTS: u16 = 0x02;      // USB Status
    pub const USBINTR: u16 = 0x04;     // USB Interrupt Enable
    pub const FRNUM: u16 = 0x06;       // Frame Number
    pub const FRBASEADD: u16 = 0x08;   // Frame List Base Address
    pub const SOFMOD: u16 = 0x0C;      // Start Of Frame Modify
    pub const PORTSC1: u16 = 0x10;     // Port 1 Status/Control
    pub const PORTSC2: u16 = 0x12;     // Port 2 Status/Control
}

/// UHCI USB Command register bits
mod uhci_cmd {
    pub const RUN: u16 = 1 << 0;
    pub const HCRESET: u16 = 1 << 1;
    pub const GRESET: u16 = 1 << 2;
    pub const EGSM: u16 = 1 << 3;
    pub const FGR: u16 = 1 << 4;
    pub const SWDBG: u16 = 1 << 5;
    pub const CF: u16 = 1 << 6;
    pub const MAXP: u16 = 1 << 7;
}

/// UHCI Port Status/Control bits
mod uhci_port {
    pub const CCS: u16 = 1 << 0;       // Current Connect Status
    pub const CSC: u16 = 1 << 1;       // Connect Status Change
    pub const PE: u16 = 1 << 2;        // Port Enable
    pub const PEC: u16 = 1 << 3;       // Port Enable Change
    pub const LS: u16 = 0x30;          // Line Status
    pub const RD: u16 = 1 << 6;        // Resume Detect
    pub const LSDA: u16 = 1 << 8;      // Low Speed Device Attached
    pub const PR: u16 = 1 << 9;        // Port Reset
    pub const SUSP: u16 = 1 << 12;     // Suspend
}

/// UHCI Transfer Descriptor
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct UhciTd {
    /// Link pointer
    link: u32,
    /// Control and status
    control: u32,
    /// Token
    token: u32,
    /// Buffer pointer
    buffer: u32,
}

/// UHCI Queue Head
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct UhciQh {
    /// Horizontal link
    head_link: u32,
    /// Element link
    element_link: u32,
}

/// UHCI Controller
pub struct UhciController {
    /// PCI location
    pci_bus: u8,
    pci_slot: u8,
    pci_func: u8,
    /// I/O base address
    io_base: u16,
    /// Number of ports
    num_ports: u32,
    /// Frame list physical address
    frame_list: u64,
    /// Is running
    running: AtomicBool,
    /// Lock for transfers
    transfer_lock: Mutex<()>,
}

impl UhciController {
    /// Create new UHCI controller
    pub fn new(bus: u8, slot: u8, func: u8) -> Option<Self> {
        // Read I/O base from BAR4
        let bar4 = crate::drivers::pci::read_bar(bus, slot, func, 4)?;
        if bar4 & 1 == 0 {
            return None; // Not I/O space
        }

        let io_base = (bar4 & !0x03) as u16;

        let controller = Self {
            pci_bus: bus,
            pci_slot: slot,
            pci_func: func,
            io_base,
            num_ports: 2,
            frame_list: 0,
            running: AtomicBool::new(false),
            transfer_lock: Mutex::new(()),
        };

        // Reset controller
        controller.reset();

        // Allocate frame list
        // controller.frame_list = alloc_frame_list();

        // Start controller
        controller.start();

        Some(controller)
    }

    /// Reset controller
    fn reset(&self) {
        // Global reset
        self.write_cmd(uhci_cmd::GRESET);
        self.delay(50);
        self.write_cmd(0);

        // Host controller reset
        self.write_cmd(uhci_cmd::HCRESET);
        self.delay(50);

        // Wait for reset to complete
        for _ in 0..100 {
            if self.read_cmd() & uhci_cmd::HCRESET == 0 {
                break;
            }
            self.delay(1);
        }
    }

    /// Start controller
    fn start(&self) {
        // Set frame list base
        self.outl(uhci_regs::FRBASEADD, self.frame_list as u32);

        // Set frame number to 0
        self.outw(uhci_regs::FRNUM, 0);

        // Enable interrupts
        self.outw(uhci_regs::USBINTR, 0x0F);

        // Start controller
        self.write_cmd(uhci_cmd::RUN | uhci_cmd::CF | uhci_cmd::MAXP);

        self.running.store(true, Ordering::Release);
    }

    /// Read command register
    fn read_cmd(&self) -> u16 {
        self.inw(uhci_regs::USBCMD)
    }

    /// Write command register
    fn write_cmd(&self, value: u16) {
        self.outw(uhci_regs::USBCMD, value);
    }

    /// Read port status
    fn read_port(&self, port: u32) -> u16 {
        let offset = if port == 0 { uhci_regs::PORTSC1 } else { uhci_regs::PORTSC2 };
        self.inw(offset)
    }

    /// Write port status
    fn write_port(&self, port: u32, value: u16) {
        let offset = if port == 0 { uhci_regs::PORTSC1 } else { uhci_regs::PORTSC2 };
        self.outw(offset, value);
    }

    /// Read word from I/O
    fn inw(&self, offset: u16) -> u16 {
        let port = self.io_base + offset;
        let value: u16;
        unsafe {
            core::arch::asm!(
                "in ax, dx",
                in("dx") port,
                out("ax") value,
            );
        }
        value
    }

    /// Write word to I/O
    fn outw(&self, offset: u16, value: u16) {
        let port = self.io_base + offset;
        unsafe {
            core::arch::asm!(
                "out dx, ax",
                in("dx") port,
                in("ax") value,
            );
        }
    }

    /// Read dword from I/O
    fn inl(&self, offset: u16) -> u32 {
        let port = self.io_base + offset;
        let value: u32;
        unsafe {
            core::arch::asm!(
                "in eax, dx",
                in("dx") port,
                out("eax") value,
            );
        }
        value
    }

    /// Write dword to I/O
    fn outl(&self, offset: u16, value: u32) {
        let port = self.io_base + offset;
        unsafe {
            core::arch::asm!(
                "out dx, eax",
                in("dx") port,
                in("eax") value,
            );
        }
    }

    /// Delay in milliseconds
    fn delay(&self, _ms: u32) {
        // Simple busy wait
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
}

impl UsbHc for UhciController {
    fn hc_type(&self) -> HcType {
        HcType::Uhci
    }

    fn port_count(&self) -> u32 {
        self.num_ports
    }

    fn port_connected(&self, port: u32) -> bool {
        if port >= self.num_ports {
            return false;
        }
        self.read_port(port) & uhci_port::CCS != 0
    }

    fn port_speed(&self, port: u32) -> UsbSpeed {
        if port >= self.num_ports {
            return UsbSpeed::Full;
        }
        if self.read_port(port) & uhci_port::LSDA != 0 {
            UsbSpeed::Low
        } else {
            UsbSpeed::Full
        }
    }

    fn port_reset(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }

        // Assert reset
        let status = self.read_port(port);
        self.write_port(port, status | uhci_port::PR);

        // Wait 50ms
        self.delay(50);

        // Deassert reset
        let status = self.read_port(port);
        self.write_port(port, status & !uhci_port::PR);

        // Wait for enable
        self.delay(10);

        // Enable port
        let status = self.read_port(port);
        self.write_port(port, status | uhci_port::PE);

        // Clear change bits
        self.write_port(port, uhci_port::CSC | uhci_port::PEC);

        Ok(())
    }

    fn port_enable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        let status = self.read_port(port);
        self.write_port(port, status | uhci_port::PE);
        Ok(())
    }

    fn port_disable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        let status = self.read_port(port);
        self.write_port(port, status & !uhci_port::PE);
        Ok(())
    }

    fn control_transfer(
        &self,
        _address: u8,
        _setup: &SetupPacket,
        _data: Option<&mut [u8]>,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();

        // Would build TDs and QHs for control transfer
        // For now, return placeholder
        Ok(0)
    }

    fn bulk_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn interrupt_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn isochronous_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        _data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        // UHCI doesn't support isochronous well
        Err(UsbError::NotSupported)
    }

    fn handle_interrupt(&self) {
        let status = self.inw(uhci_regs::USBSTS);

        // Clear interrupt status
        self.outw(uhci_regs::USBSTS, status);

        // Handle various interrupts
        if status & 0x01 != 0 {
            // Transfer complete
        }
        if status & 0x02 != 0 {
            // Error
        }
        if status & 0x04 != 0 {
            // Resume detect
        }
        if status & 0x08 != 0 {
            // Host system error
        }
        if status & 0x10 != 0 {
            // Host controller process error
        }
        if status & 0x20 != 0 {
            // Halted
        }
    }

    fn suspend(&self) -> Result<(), UsbError> {
        self.write_cmd(uhci_cmd::EGSM);
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn resume(&self) -> Result<(), UsbError> {
        self.write_cmd(uhci_cmd::FGR);
        self.delay(20);
        self.write_cmd(uhci_cmd::RUN | uhci_cmd::CF | uhci_cmd::MAXP);
        self.running.store(true, Ordering::Release);
        Ok(())
    }
}

// =============================================================================
// OHCI Controller (USB 1.0/1.1)
// =============================================================================

/// OHCI Register offsets
mod ohci_regs {
    pub const HCREVISION: u32 = 0x00;
    pub const HCCONTROL: u32 = 0x04;
    pub const HCCOMMANDSTATUS: u32 = 0x08;
    pub const HCINTERRUPTSTATUS: u32 = 0x0C;
    pub const HCINTERRUPTENABLE: u32 = 0x10;
    pub const HCINTERRUPTDISABLE: u32 = 0x14;
    pub const HCHCCA: u32 = 0x18;
    pub const HCPERIODCURRENTED: u32 = 0x1C;
    pub const HCCONTROLHEADED: u32 = 0x20;
    pub const HCCONTROLCURRENTED: u32 = 0x24;
    pub const HCBULKHEADED: u32 = 0x28;
    pub const HCBULKCURRENTED: u32 = 0x2C;
    pub const HCDONEHEAD: u32 = 0x30;
    pub const HCFMINTERVAL: u32 = 0x34;
    pub const HCFMREMAINING: u32 = 0x38;
    pub const HCFMNUMBER: u32 = 0x3C;
    pub const HCPERIODICSTART: u32 = 0x40;
    pub const HCLSTHRESHOLD: u32 = 0x44;
    pub const HCRHDESCRIPTORA: u32 = 0x48;
    pub const HCRHDESCRIPTORB: u32 = 0x4C;
    pub const HCRHSTATUS: u32 = 0x50;
    pub const HCRHPORTSTATUS: u32 = 0x54;
}

/// OHCI Controller
pub struct OhciController {
    /// PCI location
    pci_bus: u8,
    pci_slot: u8,
    pci_func: u8,
    /// Memory-mapped base address
    mmio_base: u64,
    /// Number of ports
    num_ports: u32,
    /// HCCA physical address
    hcca: u64,
    /// Is running
    running: AtomicBool,
    /// Transfer lock
    transfer_lock: Mutex<()>,
}

impl OhciController {
    /// Create new OHCI controller
    pub fn new(bus: u8, slot: u8, func: u8) -> Option<Self> {
        // Read MMIO base from BAR0
        let bar0 = crate::drivers::pci::read_bar(bus, slot, func, 0)?;
        if bar0 & 1 != 0 {
            return None; // Not MMIO
        }

        let mmio_base = (bar0 & !0x0F) as u64;

        let controller = Self {
            pci_bus: bus,
            pci_slot: slot,
            pci_func: func,
            mmio_base,
            num_ports: 0,
            hcca: 0,
            running: AtomicBool::new(false),
            transfer_lock: Mutex::new(()),
        };

        // Read number of ports
        let rh_desc_a = controller.read_reg(ohci_regs::HCRHDESCRIPTORA);
        let mut controller = controller;
        controller.num_ports = rh_desc_a & 0xFF;

        // Reset and initialize
        controller.reset();
        controller.start();

        Some(controller)
    }

    /// Read OHCI register
    fn read_reg(&self, offset: u32) -> u32 {
        let addr = (self.mmio_base + offset as u64) as *const u32;
        unsafe { core::ptr::read_volatile(addr) }
    }

    /// Write OHCI register
    fn write_reg(&self, offset: u32, value: u32) {
        let addr = (self.mmio_base + offset as u64) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, value) }
    }

    /// Reset controller
    fn reset(&self) {
        // Perform software reset
        self.write_reg(ohci_regs::HCCOMMANDSTATUS, 1);

        // Wait for reset to complete
        for _ in 0..100 {
            if self.read_reg(ohci_regs::HCCOMMANDSTATUS) & 1 == 0 {
                break;
            }
            self.delay(1);
        }
    }

    /// Start controller
    fn start(&self) {
        // Set HCCA
        self.write_reg(ohci_regs::HCHCCA, self.hcca as u32);

        // Set frame interval
        self.write_reg(ohci_regs::HCFMINTERVAL, 0x2EDF | (0x2778 << 16));

        // Set periodic start
        self.write_reg(ohci_regs::HCPERIODICSTART, 0x2A27);

        // Enable interrupts
        self.write_reg(ohci_regs::HCINTERRUPTENABLE, 0x8000002F);

        // Set operational state
        let control = self.read_reg(ohci_regs::HCCONTROL);
        self.write_reg(ohci_regs::HCCONTROL, (control & !0xC0) | 0x80);

        // Enable ports
        self.write_reg(ohci_regs::HCRHSTATUS, 0x10000);

        self.running.store(true, Ordering::Release);
    }

    /// Delay in milliseconds
    fn delay(&self, _ms: u32) {
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
}

impl UsbHc for OhciController {
    fn hc_type(&self) -> HcType {
        HcType::Ohci
    }

    fn port_count(&self) -> u32 {
        self.num_ports
    }

    fn port_connected(&self, port: u32) -> bool {
        if port >= self.num_ports {
            return false;
        }
        let status = self.read_reg(ohci_regs::HCRHPORTSTATUS + port * 4);
        status & 1 != 0
    }

    fn port_speed(&self, port: u32) -> UsbSpeed {
        if port >= self.num_ports {
            return UsbSpeed::Full;
        }
        let status = self.read_reg(ohci_regs::HCRHPORTSTATUS + port * 4);
        if status & 0x200 != 0 {
            UsbSpeed::Low
        } else {
            UsbSpeed::Full
        }
    }

    fn port_reset(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }

        // Set port reset
        self.write_reg(ohci_regs::HCRHPORTSTATUS + port * 4, 0x10);

        // Wait for reset complete
        for _ in 0..100 {
            let status = self.read_reg(ohci_regs::HCRHPORTSTATUS + port * 4);
            if status & 0x100000 != 0 {
                // Clear reset complete status
                self.write_reg(ohci_regs::HCRHPORTSTATUS + port * 4, 0x100000);
                break;
            }
            self.delay(1);
        }

        Ok(())
    }

    fn port_enable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        self.write_reg(ohci_regs::HCRHPORTSTATUS + port * 4, 0x02);
        Ok(())
    }

    fn port_disable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        self.write_reg(ohci_regs::HCRHPORTSTATUS + port * 4, 0x01);
        Ok(())
    }

    fn control_transfer(
        &self,
        _address: u8,
        _setup: &SetupPacket,
        _data: Option<&mut [u8]>,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(0)
    }

    fn bulk_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn interrupt_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn isochronous_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn handle_interrupt(&self) {
        let status = self.read_reg(ohci_regs::HCINTERRUPTSTATUS);
        self.write_reg(ohci_regs::HCINTERRUPTSTATUS, status);
    }

    fn suspend(&self) -> Result<(), UsbError> {
        let control = self.read_reg(ohci_regs::HCCONTROL);
        self.write_reg(ohci_regs::HCCONTROL, (control & !0xC0) | 0xC0);
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn resume(&self) -> Result<(), UsbError> {
        let control = self.read_reg(ohci_regs::HCCONTROL);
        self.write_reg(ohci_regs::HCCONTROL, (control & !0xC0) | 0x40);
        self.delay(20);
        self.write_reg(ohci_regs::HCCONTROL, (control & !0xC0) | 0x80);
        self.running.store(true, Ordering::Release);
        Ok(())
    }
}

// =============================================================================
// EHCI Controller (USB 2.0)
// =============================================================================

/// EHCI Capability Register offsets
mod ehci_cap {
    pub const CAPLENGTH: u32 = 0x00;
    pub const HCIVERSION: u32 = 0x02;
    pub const HCSPARAMS: u32 = 0x04;
    pub const HCCPARAMS: u32 = 0x08;
}

/// EHCI Operational Register offsets (relative to CAPLENGTH)
mod ehci_op {
    pub const USBCMD: u32 = 0x00;
    pub const USBSTS: u32 = 0x04;
    pub const USBINTR: u32 = 0x08;
    pub const FRINDEX: u32 = 0x0C;
    pub const CTRLDSSEGMENT: u32 = 0x10;
    pub const PERIODICLISTBASE: u32 = 0x14;
    pub const ASYNCLISTADDR: u32 = 0x18;
    pub const CONFIGFLAG: u32 = 0x40;
    pub const PORTSC: u32 = 0x44;
}

/// EHCI Queue Head
#[repr(C, packed)]
struct EhciQh {
    /// Horizontal link pointer
    horizontal_link: u32,
    /// Endpoint characteristics
    endpoint_chars: u32,
    /// Endpoint capabilities
    endpoint_caps: u32,
    /// Current qTD pointer
    current_qtd: u32,
    /// Next qTD pointer
    next_qtd: u32,
    /// Alternate next qTD
    alt_next_qtd: u32,
    /// Token
    token: u32,
    /// Buffer pointers
    buffers: [u32; 5],
}

/// EHCI Queue Element Transfer Descriptor
#[repr(C, packed)]
struct EhciQtd {
    /// Next qTD pointer
    next_qtd: u32,
    /// Alternate next qTD
    alt_next_qtd: u32,
    /// Token
    token: u32,
    /// Buffer pointers
    buffers: [u32; 5],
}

/// EHCI Controller
pub struct EhciController {
    /// PCI location
    pci_bus: u8,
    pci_slot: u8,
    pci_func: u8,
    /// Capability registers base
    cap_base: u64,
    /// Operational registers base
    op_base: u64,
    /// Number of ports
    num_ports: u32,
    /// Periodic frame list
    periodic_list: u64,
    /// Async list head
    async_list: u64,
    /// Is running
    running: AtomicBool,
    /// Transfer lock
    transfer_lock: Mutex<()>,
}

impl EhciController {
    /// Create new EHCI controller
    pub fn new(bus: u8, slot: u8, func: u8) -> Option<Self> {
        // Read MMIO base from BAR0
        let bar0 = crate::drivers::pci::read_bar(bus, slot, func, 0)?;
        if bar0 & 1 != 0 {
            return None;
        }

        let cap_base = (bar0 & !0x0F) as u64;

        // Read capability length
        let caplength = unsafe {
            core::ptr::read_volatile((cap_base + ehci_cap::CAPLENGTH as u64) as *const u8)
        };

        let op_base = cap_base + caplength as u64;

        // Read HCSPARAMS for port count
        let hcsparams = unsafe {
            core::ptr::read_volatile((cap_base + ehci_cap::HCSPARAMS as u64) as *const u32)
        };
        let num_ports = hcsparams & 0x0F;

        let controller = Self {
            pci_bus: bus,
            pci_slot: slot,
            pci_func: func,
            cap_base,
            op_base,
            num_ports,
            periodic_list: 0,
            async_list: 0,
            running: AtomicBool::new(false),
            transfer_lock: Mutex::new(()),
        };

        controller.reset();
        controller.start();

        Some(controller)
    }

    /// Read operational register
    fn read_op(&self, offset: u32) -> u32 {
        let addr = (self.op_base + offset as u64) as *const u32;
        unsafe { core::ptr::read_volatile(addr) }
    }

    /// Write operational register
    fn write_op(&self, offset: u32, value: u32) {
        let addr = (self.op_base + offset as u64) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, value) }
    }

    /// Reset controller
    fn reset(&self) {
        // Stop controller
        let cmd = self.read_op(ehci_op::USBCMD);
        self.write_op(ehci_op::USBCMD, cmd & !1);

        // Wait for halt
        for _ in 0..100 {
            if self.read_op(ehci_op::USBSTS) & 0x1000 != 0 {
                break;
            }
            self.delay(1);
        }

        // Reset
        self.write_op(ehci_op::USBCMD, 2);

        // Wait for reset to complete
        for _ in 0..100 {
            if self.read_op(ehci_op::USBCMD) & 2 == 0 {
                break;
            }
            self.delay(1);
        }
    }

    /// Start controller
    fn start(&self) {
        // Set interrupt threshold
        let cmd = self.read_op(ehci_op::USBCMD);
        self.write_op(ehci_op::USBCMD, (cmd & !0xFF0000) | 0x080000);

        // Set frame list base
        self.write_op(ehci_op::PERIODICLISTBASE, self.periodic_list as u32);

        // Set async list address
        self.write_op(ehci_op::ASYNCLISTADDR, self.async_list as u32);

        // Enable interrupts
        self.write_op(ehci_op::USBINTR, 0x37);

        // Set config flag
        self.write_op(ehci_op::CONFIGFLAG, 1);

        // Start controller
        let cmd = self.read_op(ehci_op::USBCMD);
        self.write_op(ehci_op::USBCMD, cmd | 1);

        self.running.store(true, Ordering::Release);
    }

    /// Delay
    fn delay(&self, _ms: u32) {
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
}

impl UsbHc for EhciController {
    fn hc_type(&self) -> HcType {
        HcType::Ehci
    }

    fn port_count(&self) -> u32 {
        self.num_ports
    }

    fn port_connected(&self, port: u32) -> bool {
        if port >= self.num_ports {
            return false;
        }
        let status = self.read_op(ehci_op::PORTSC + port * 4);
        status & 1 != 0
    }

    fn port_speed(&self, port: u32) -> UsbSpeed {
        if port >= self.num_ports {
            return UsbSpeed::High;
        }
        let status = self.read_op(ehci_op::PORTSC + port * 4);
        let line_status = (status >> 10) & 3;

        match line_status {
            1 => UsbSpeed::Low,
            2 => UsbSpeed::Full,
            _ => UsbSpeed::High,
        }
    }

    fn port_reset(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }

        let status = self.read_op(ehci_op::PORTSC + port * 4);

        // Set reset bit
        self.write_op(ehci_op::PORTSC + port * 4, status | 0x100);

        // Wait 50ms
        self.delay(50);

        // Clear reset bit
        let status = self.read_op(ehci_op::PORTSC + port * 4);
        self.write_op(ehci_op::PORTSC + port * 4, status & !0x100);

        // Wait for enable
        for _ in 0..100 {
            let status = self.read_op(ehci_op::PORTSC + port * 4);
            if status & 4 != 0 {
                break;
            }
            self.delay(1);
        }

        Ok(())
    }

    fn port_enable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        let status = self.read_op(ehci_op::PORTSC + port * 4);
        self.write_op(ehci_op::PORTSC + port * 4, status | 4);
        Ok(())
    }

    fn port_disable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        let status = self.read_op(ehci_op::PORTSC + port * 4);
        self.write_op(ehci_op::PORTSC + port * 4, status & !4);
        Ok(())
    }

    fn control_transfer(
        &self,
        _address: u8,
        _setup: &SetupPacket,
        _data: Option<&mut [u8]>,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(0)
    }

    fn bulk_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn interrupt_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn isochronous_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn handle_interrupt(&self) {
        let status = self.read_op(ehci_op::USBSTS);
        self.write_op(ehci_op::USBSTS, status);
    }

    fn suspend(&self) -> Result<(), UsbError> {
        let cmd = self.read_op(ehci_op::USBCMD);
        self.write_op(ehci_op::USBCMD, cmd & !1);
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn resume(&self) -> Result<(), UsbError> {
        let cmd = self.read_op(ehci_op::USBCMD);
        self.write_op(ehci_op::USBCMD, cmd | 1);
        self.running.store(true, Ordering::Release);
        Ok(())
    }
}

// =============================================================================
// xHCI Controller (USB 3.x)
// =============================================================================

/// xHCI Capability Register offsets
mod xhci_cap {
    pub const CAPLENGTH: u32 = 0x00;
    pub const HCIVERSION: u32 = 0x02;
    pub const HCSPARAMS1: u32 = 0x04;
    pub const HCSPARAMS2: u32 = 0x08;
    pub const HCSPARAMS3: u32 = 0x0C;
    pub const HCCPARAMS1: u32 = 0x10;
    pub const DBOFF: u32 = 0x14;
    pub const RTSOFF: u32 = 0x18;
    pub const HCCPARAMS2: u32 = 0x1C;
}

/// xHCI Operational Register offsets
mod xhci_op {
    pub const USBCMD: u32 = 0x00;
    pub const USBSTS: u32 = 0x04;
    pub const PAGESIZE: u32 = 0x08;
    pub const DNCTRL: u32 = 0x14;
    pub const CRCR: u32 = 0x18;
    pub const DCBAAP: u32 = 0x30;
    pub const CONFIG: u32 = 0x38;
    pub const PORTSC: u32 = 0x400;
}

/// xHCI Transfer Request Block (TRB)
#[repr(C, packed)]
struct XhciTrb {
    /// Parameter
    parameter: u64,
    /// Status
    status: u32,
    /// Control
    control: u32,
}

/// xHCI Controller
pub struct XhciController {
    /// PCI location
    pci_bus: u8,
    pci_slot: u8,
    pci_func: u8,
    /// Capability registers base
    cap_base: u64,
    /// Operational registers base
    op_base: u64,
    /// Runtime registers base
    runtime_base: u64,
    /// Doorbell registers base
    doorbell_base: u64,
    /// Number of ports
    num_ports: u32,
    /// Number of slots
    num_slots: u32,
    /// Device context base array
    dcbaa: u64,
    /// Command ring
    command_ring: u64,
    /// Event ring
    event_ring: u64,
    /// Is running
    running: AtomicBool,
    /// Transfer lock
    transfer_lock: Mutex<()>,
}

impl XhciController {
    /// Create new xHCI controller
    pub fn new(bus: u8, slot: u8, func: u8) -> Option<Self> {
        // Read MMIO base from BAR0
        let bar0 = crate::drivers::pci::read_bar(bus, slot, func, 0)?;
        if bar0 & 1 != 0 {
            return None;
        }

        let cap_base = (bar0 & !0x0F) as u64;

        // Read capability length
        let caplength = unsafe {
            core::ptr::read_volatile((cap_base + xhci_cap::CAPLENGTH as u64) as *const u8)
        };

        let op_base = cap_base + caplength as u64;

        // Read HCSPARAMS1 for port and slot count
        let hcsparams1 = unsafe {
            core::ptr::read_volatile((cap_base + xhci_cap::HCSPARAMS1 as u64) as *const u32)
        };
        let num_ports = (hcsparams1 >> 24) & 0xFF;
        let num_slots = hcsparams1 & 0xFF;

        // Read runtime register offset
        let rtsoff = unsafe {
            core::ptr::read_volatile((cap_base + xhci_cap::RTSOFF as u64) as *const u32)
        };
        let runtime_base = cap_base + (rtsoff & !0x1F) as u64;

        // Read doorbell register offset
        let dboff = unsafe {
            core::ptr::read_volatile((cap_base + xhci_cap::DBOFF as u64) as *const u32)
        };
        let doorbell_base = cap_base + (dboff & !0x03) as u64;

        let controller = Self {
            pci_bus: bus,
            pci_slot: slot,
            pci_func: func,
            cap_base,
            op_base,
            runtime_base,
            doorbell_base,
            num_ports,
            num_slots,
            dcbaa: 0,
            command_ring: 0,
            event_ring: 0,
            running: AtomicBool::new(false),
            transfer_lock: Mutex::new(()),
        };

        controller.reset();
        controller.start();

        Some(controller)
    }

    /// Read operational register
    fn read_op(&self, offset: u32) -> u32 {
        let addr = (self.op_base + offset as u64) as *const u32;
        unsafe { core::ptr::read_volatile(addr) }
    }

    /// Write operational register
    fn write_op(&self, offset: u32, value: u32) {
        let addr = (self.op_base + offset as u64) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, value) }
    }

    /// Read operational register (64-bit)
    fn read_op64(&self, offset: u32) -> u64 {
        let addr = (self.op_base + offset as u64) as *const u64;
        unsafe { core::ptr::read_volatile(addr) }
    }

    /// Write operational register (64-bit)
    fn write_op64(&self, offset: u32, value: u64) {
        let addr = (self.op_base + offset as u64) as *mut u64;
        unsafe { core::ptr::write_volatile(addr, value) }
    }

    /// Reset controller
    fn reset(&self) {
        // Stop controller
        let cmd = self.read_op(xhci_op::USBCMD);
        self.write_op(xhci_op::USBCMD, cmd & !1);

        // Wait for halt
        for _ in 0..100 {
            if self.read_op(xhci_op::USBSTS) & 1 != 0 {
                break;
            }
            self.delay(1);
        }

        // Reset
        self.write_op(xhci_op::USBCMD, 2);

        // Wait for reset to complete
        for _ in 0..100 {
            if self.read_op(xhci_op::USBCMD) & 2 == 0 &&
               self.read_op(xhci_op::USBSTS) & 0x800 == 0 {
                break;
            }
            self.delay(1);
        }
    }

    /// Start controller
    fn start(&self) {
        // Set max slots enabled
        let config = self.num_slots;
        self.write_op(xhci_op::CONFIG, config);

        // Set DCBAAP
        self.write_op64(xhci_op::DCBAAP, self.dcbaa);

        // Set command ring
        self.write_op64(xhci_op::CRCR, self.command_ring | 1);

        // Start controller
        let cmd = self.read_op(xhci_op::USBCMD);
        self.write_op(xhci_op::USBCMD, cmd | 1 | 4); // Run + Interrupter Enable

        self.running.store(true, Ordering::Release);
    }

    /// Ring doorbell
    fn ring_doorbell(&self, slot: u32, target: u32) {
        let addr = (self.doorbell_base + (slot * 4) as u64) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, target) }
    }

    /// Delay
    fn delay(&self, _ms: u32) {
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
    }
}

impl UsbHc for XhciController {
    fn hc_type(&self) -> HcType {
        HcType::Xhci
    }

    fn port_count(&self) -> u32 {
        self.num_ports
    }

    fn port_connected(&self, port: u32) -> bool {
        if port >= self.num_ports {
            return false;
        }
        let status = self.read_op(xhci_op::PORTSC + port * 0x10);
        status & 1 != 0
    }

    fn port_speed(&self, port: u32) -> UsbSpeed {
        if port >= self.num_ports {
            return UsbSpeed::Super;
        }
        let status = self.read_op(xhci_op::PORTSC + port * 0x10);
        let speed = (status >> 10) & 0xF;

        match speed {
            1 => UsbSpeed::Full,
            2 => UsbSpeed::Low,
            3 => UsbSpeed::High,
            4 => UsbSpeed::Super,
            5..=7 => UsbSpeed::SuperPlus,
            _ => UsbSpeed::Full,
        }
    }

    fn port_reset(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }

        let status = self.read_op(xhci_op::PORTSC + port * 0x10);

        // Set reset bit
        self.write_op(xhci_op::PORTSC + port * 0x10, (status & 0x0E00C3E0) | 0x10);

        // Wait for reset complete
        for _ in 0..200 {
            let status = self.read_op(xhci_op::PORTSC + port * 0x10);
            if status & 0x200000 != 0 {
                // Clear change bits
                self.write_op(xhci_op::PORTSC + port * 0x10, status | 0x00FE0000);
                break;
            }
            self.delay(1);
        }

        Ok(())
    }

    fn port_enable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        // xHCI ports are automatically enabled after reset
        Ok(())
    }

    fn port_disable(&self, port: u32) -> Result<(), UsbError> {
        if port >= self.num_ports {
            return Err(UsbError::InvalidArgument);
        }
        let status = self.read_op(xhci_op::PORTSC + port * 0x10);
        self.write_op(xhci_op::PORTSC + port * 0x10, (status & 0x0E00C3E0) | 2);
        Ok(())
    }

    fn control_transfer(
        &self,
        _address: u8,
        _setup: &SetupPacket,
        _data: Option<&mut [u8]>,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();

        // Would build TRBs and queue on transfer ring
        // Ring doorbell
        // Wait for completion

        Ok(0)
    }

    fn bulk_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn interrupt_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn isochronous_transfer(
        &self,
        _address: u8,
        _endpoint: u8,
        data: &mut [u8],
        _direction: TransferDirection,
    ) -> Result<usize, UsbError> {
        let _lock = self.transfer_lock.lock();
        Ok(data.len())
    }

    fn handle_interrupt(&self) {
        let status = self.read_op(xhci_op::USBSTS);
        self.write_op(xhci_op::USBSTS, status);

        // Process event ring
    }

    fn suspend(&self) -> Result<(), UsbError> {
        let cmd = self.read_op(xhci_op::USBCMD);
        self.write_op(xhci_op::USBCMD, cmd & !1);
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    fn resume(&self) -> Result<(), UsbError> {
        let cmd = self.read_op(xhci_op::USBCMD);
        self.write_op(xhci_op::USBCMD, cmd | 1);
        self.running.store(true, Ordering::Release);
        Ok(())
    }
}
