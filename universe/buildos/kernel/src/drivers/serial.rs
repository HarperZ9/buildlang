// ===============================================================================
// BUILDOS KERNEL - SERIAL PORT DRIVER
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Serial port (UART) driver for debugging output.

use core::fmt::{self, Write};
use spin::Mutex;

/// COM1 port address
const COM1: u16 = 0x3F8;

/// Global serial writer
pub static SERIAL: Mutex<SerialPort> = Mutex::new(SerialPort::new(COM1));

/// Serial port driver
pub struct SerialPort {
    port: u16,
    initialized: bool,
}

impl SerialPort {
    /// Create a new serial port
    const fn new(port: u16) -> Self {
        Self {
            port,
            initialized: false,
        }
    }

    /// Initialize the serial port
    pub fn init(&mut self) {
        unsafe {
            // Disable interrupts
            outb(self.port + 1, 0x00);

            // Enable DLAB
            outb(self.port + 3, 0x80);

            // Set baud rate divisor (115200 baud)
            outb(self.port + 0, 0x01);
            outb(self.port + 1, 0x00);

            // 8 bits, no parity, one stop bit
            outb(self.port + 3, 0x03);

            // Enable FIFO
            outb(self.port + 2, 0xC7);

            // Enable IRQs, RTS/DSR set
            outb(self.port + 4, 0x0B);
        }

        self.initialized = true;
    }

    /// Write a byte to the serial port
    pub fn write_byte(&mut self, byte: u8) {
        if !self.initialized {
            self.init();
        }

        unsafe {
            // Wait for transmit buffer to be empty
            while (inb(self.port + 5) & 0x20) == 0 {}

            outb(self.port, byte);
        }
    }

    /// Read a byte from the serial port
    pub fn read_byte(&mut self) -> Option<u8> {
        if !self.initialized {
            self.init();
        }

        unsafe {
            if (inb(self.port + 5) & 0x01) != 0 {
                Some(inb(self.port))
            } else {
                None
            }
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// Output a byte to an I/O port
#[inline]
unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nostack, nomem)
    );
}

/// Read a byte from an I/O port
#[inline]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nostack, nomem)
    );
    value
}

/// Initialize the serial port (module-level convenience function)
pub fn init() {
    SERIAL.lock().init();
}

/// Write formatted output to the serial port
pub fn write_fmt(args: fmt::Arguments) -> fmt::Result {
    use core::fmt::Write;
    SERIAL.lock().write_fmt(args)
}

/// Write a string to the serial port
pub fn write_str(s: &str) {
    use core::fmt::Write;
    let _ = SERIAL.lock().write_str(s);
}
