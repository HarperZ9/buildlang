// ===============================================================================
// BUILDOS KERNEL - LOG CRATE COMPATIBILITY SHIM
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Log crate compatibility shim
//!
//! Provides `crate::log::info!`, `crate::log::debug!`, `crate::log::warn!`, `crate::log::error!`, `crate::log::trace!`
//! macros that redirect to the kernel's logging infrastructure.

/// Log an error message
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        $crate::kprintln!("[ERROR] {}", format_args!($($arg)*));
    }};
}

/// Log a warning message
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        $crate::kprintln!("[WARN] {}", format_args!($($arg)*));
    }};
}

/// Log an info message
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        $crate::kprintln!("[INFO] {}", format_args!($($arg)*));
    }};
}

/// Log a debug message
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {{
        // Debug messages only in debug builds
        #[cfg(debug_assertions)]
        $crate::kprintln!("[DEBUG] {}", format_args!($($arg)*));
        #[cfg(not(debug_assertions))]
        let _ = format_args!($($arg)*);
    }};
}

/// Log a trace message
#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {{
        // Trace messages only in debug builds
        #[cfg(debug_assertions)]
        $crate::kprintln!("[TRACE] {}", format_args!($($arg)*));
        #[cfg(not(debug_assertions))]
        let _ = format_args!($($arg)*);
    }};
}

// Re-export macros with log:: prefix compatibility
pub use log_error as error;
pub use log_warn as warn;
pub use log_info as info;
pub use log_debug as debug;
pub use log_trace as trace;
