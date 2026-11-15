// ===============================================================================
// BUILDOS KERNEL - TRACE EVENTS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! Pre-defined trace events for common kernel operations.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

use super::{TraceEvent, EventType, EventData};

// =============================================================================
// EVENT ENABLE FLAGS
// =============================================================================

/// Scheduler events enabled
static SCHED_EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

/// IRQ events enabled
static IRQ_EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Syscall events enabled
static SYSCALL_EVENTS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Memory events enabled
static MEM_EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Block events enabled
static BLOCK_EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Network events enabled
static NET_EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

// =============================================================================
// INITIALIZATION
// =============================================================================

/// Initialize event subsystem
pub fn init() {
    // Events are enabled by default based on static flags
}

// =============================================================================
// SCHEDULER EVENTS
// =============================================================================

/// Record context switch
pub fn sched_switch(
    prev_pid: u32,
    prev_comm: &str,
    prev_state: i32,
    next_pid: u32,
    next_comm: &str,
) {
    if !SCHED_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: prev_pid,
        event_type: EventType::Sched,
        name: String::from("sched_switch"),
        data: EventData::SchedSwitch {
            prev_pid,
            prev_comm: String::from(prev_comm),
            prev_state,
            next_pid,
            next_comm: String::from(next_comm),
        },
    };

    super::record_event(event);
}

/// Record task wakeup
pub fn sched_wakeup(pid: u32, comm: &str, target_cpu: u32) {
    if !SCHED_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Sched,
        name: String::from("sched_wakeup"),
        data: EventData::SchedWakeup {
            pid,
            comm: String::from(comm),
            target_cpu,
        },
    };

    super::record_event(event);
}

// =============================================================================
// IRQ EVENTS
// =============================================================================

/// Record IRQ handler entry
pub fn irq_handler_entry(irq: u32, name: &str) {
    if !IRQ_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Irq,
        name: String::from("irq_handler_entry"),
        data: EventData::IrqEntry {
            irq,
            name: String::from(name),
        },
    };

    super::record_event(event);
}

/// Record IRQ handler exit
pub fn irq_handler_exit(irq: u32, ret: i32) {
    if !IRQ_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Irq,
        name: String::from("irq_handler_exit"),
        data: EventData::IrqExit {
            irq,
            ret,
        },
    };

    super::record_event(event);
}

// =============================================================================
// SYSCALL EVENTS
// =============================================================================

/// Record syscall entry
pub fn syscall_entry(nr: u64, args: [u64; 6]) {
    if !SYSCALL_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Syscall,
        name: String::from("sys_enter"),
        data: EventData::SyscallEntry {
            nr,
            args,
        },
    };

    super::record_event(event);
}

/// Record syscall exit
pub fn syscall_exit(nr: u64, ret: i64) {
    if !SYSCALL_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Syscall,
        name: String::from("sys_exit"),
        data: EventData::SyscallExit {
            nr,
            ret,
        },
    };

    super::record_event(event);
}

// =============================================================================
// MEMORY EVENTS
// =============================================================================

/// Record memory allocation
pub fn mem_alloc(ptr: u64, size: usize, gfp_flags: u32) {
    if !MEM_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Mem,
        name: String::from("kmalloc"),
        data: EventData::MemAlloc {
            ptr,
            size,
            gfp_flags,
        },
    };

    super::record_event(event);
}

/// Record memory free
pub fn mem_free(ptr: u64) {
    if !MEM_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Mem,
        name: String::from("kfree"),
        data: EventData::MemFree { ptr },
    };

    super::record_event(event);
}

// =============================================================================
// BLOCK I/O EVENTS
// =============================================================================

/// Record block I/O request
pub fn block_rq_issue(dev: u32, sector: u64, nr_sectors: u32, op: u32) {
    if !BLOCK_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Block,
        name: String::from("block_rq_issue"),
        data: EventData::BlockIo {
            dev,
            sector,
            nr_sectors,
            op,
        },
    };

    super::record_event(event);
}

/// Record block I/O completion
pub fn block_rq_complete(dev: u32, sector: u64, nr_sectors: u32, op: u32) {
    if !BLOCK_EVENTS_ENABLED.load(Ordering::Relaxed) || !super::is_enabled() {
        return;
    }

    let event = TraceEvent {
        timestamp: crate::time::now_ns(),
        cpu: crate::cpu::current_cpu_id(),
        pid: crate::process::current_pid().unwrap_or(0) as u32,
        event_type: EventType::Block,
        name: String::from("block_rq_complete"),
        data: EventData::BlockIo {
            dev,
            sector,
            nr_sectors,
            op,
        },
    };

    super::record_event(event);
}

// =============================================================================
// CONTROL INTERFACE
// =============================================================================

/// Enable scheduler events
pub fn enable_sched_events() {
    SCHED_EVENTS_ENABLED.store(true, Ordering::Release);
}

/// Disable scheduler events
pub fn disable_sched_events() {
    SCHED_EVENTS_ENABLED.store(false, Ordering::Release);
}

/// Enable IRQ events
pub fn enable_irq_events() {
    IRQ_EVENTS_ENABLED.store(true, Ordering::Release);
}

/// Disable IRQ events
pub fn disable_irq_events() {
    IRQ_EVENTS_ENABLED.store(false, Ordering::Release);
}

/// Enable syscall events
pub fn enable_syscall_events() {
    SYSCALL_EVENTS_ENABLED.store(true, Ordering::Release);
}

/// Disable syscall events
pub fn disable_syscall_events() {
    SYSCALL_EVENTS_ENABLED.store(false, Ordering::Release);
}

/// Enable memory events
pub fn enable_mem_events() {
    MEM_EVENTS_ENABLED.store(true, Ordering::Release);
}

/// Disable memory events
pub fn disable_mem_events() {
    MEM_EVENTS_ENABLED.store(false, Ordering::Release);
}

/// Enable block events
pub fn enable_block_events() {
    BLOCK_EVENTS_ENABLED.store(true, Ordering::Release);
}

/// Disable block events
pub fn disable_block_events() {
    BLOCK_EVENTS_ENABLED.store(false, Ordering::Release);
}

/// Enable network events
pub fn enable_net_events() {
    NET_EVENTS_ENABLED.store(true, Ordering::Release);
}

/// Disable network events
pub fn disable_net_events() {
    NET_EVENTS_ENABLED.store(false, Ordering::Release);
}

/// Enable all events
pub fn enable_all() {
    enable_sched_events();
    enable_irq_events();
    enable_syscall_events();
    enable_mem_events();
    enable_block_events();
    enable_net_events();
}

/// Disable all events
pub fn disable_all() {
    disable_sched_events();
    disable_irq_events();
    disable_syscall_events();
    disable_mem_events();
    disable_block_events();
    disable_net_events();
}
