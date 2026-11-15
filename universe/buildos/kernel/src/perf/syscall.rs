//! Perf Syscall Interface
//!
//! Implements perf_event_open() and related syscalls.

use super::*;
use super::events::{PerfEventAttr, PerfEventType, SampleTypeFlags, ReadFormatFlags};

/// perf_event_open syscall
///
/// Creates a file descriptor for performance monitoring.
///
/// # Arguments
/// * `attr` - Pointer to perf_event_attr structure
/// * `pid` - Target process ID (-1 for any, 0 for self)
/// * `cpu` - Target CPU (-1 for any)
/// * `group_fd` - Group leader FD (-1 for new group)
/// * `flags` - Event flags
///
/// # Returns
/// * File descriptor on success
/// * Negative errno on failure
pub fn sys_perf_event_open(
    attr_ptr: u64,
    pid: i32,
    cpu: i32,
    group_fd: i32,
    flags: u64,
) -> i64 {
    // Validate and copy attr from userspace
    let attr = match copy_attr_from_user(attr_ptr) {
        Ok(a) => a,
        Err(e) => return e as i64,
    };

    // Validate arguments
    if cpu < -1 || cpu >= 256 {
        return -19; // ENODEV
    }

    if pid < -1 {
        return -22; // EINVAL
    }

    // Check permissions
    if !check_perf_permissions(&attr, pid, cpu) {
        return -1; // EPERM
    }

    // Create event
    let event_flags = PerfEventFlags::from_bits_truncate(flags);
    match event_open(&attr, pid, cpu, group_fd, event_flags) {
        Ok(id) => {
            // Create file descriptor for event
            match create_perf_fd(id) {
                Ok(fd) => fd as i64,
                Err(e) => e.to_errno() as i64,
            }
        }
        Err(e) => e.to_errno() as i64,
    }
}

/// perf_event ioctl handler
pub fn sys_perf_event_ioctl(fd: i32, request: u32, arg: u64) -> i64 {
    let event_id = match get_event_id_from_fd(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    match request {
        ioctl::PERF_EVENT_IOC_ENABLE => {
            match event_enable(event_id) {
                Ok(()) => 0,
                Err(e) => e.to_errno() as i64,
            }
        }
        ioctl::PERF_EVENT_IOC_DISABLE => {
            match event_disable(event_id) {
                Ok(()) => 0,
                Err(e) => e.to_errno() as i64,
            }
        }
        ioctl::PERF_EVENT_IOC_RESET => {
            match event_reset(event_id) {
                Ok(()) => 0,
                Err(e) => e.to_errno() as i64,
            }
        }
        ioctl::PERF_EVENT_IOC_REFRESH => {
            // Refresh for overflow counting
            // Would set overflow count to arg
            0
        }
        ioctl::PERF_EVENT_IOC_PERIOD => {
            // Set sample period
            // Would update event sample period
            0
        }
        ioctl::PERF_EVENT_IOC_SET_OUTPUT => {
            // Set output FD for event data
            0
        }
        ioctl::PERF_EVENT_IOC_SET_FILTER => {
            // Set event filter
            0
        }
        ioctl::PERF_EVENT_IOC_ID => {
            // Get event ID
            if arg != 0 {
                unsafe {
                    *(arg as *mut u64) = event_id;
                }
            }
            0
        }
        ioctl::PERF_EVENT_IOC_SET_BPF => {
            // Attach BPF program
            let bpf_fd = arg as i32;
            attach_bpf_to_event(event_id, bpf_fd)
        }
        ioctl::PERF_EVENT_IOC_PAUSE_OUTPUT => {
            // Pause ring buffer output
            0
        }
        ioctl::PERF_EVENT_IOC_QUERY_BPF => {
            // Query attached BPF programs
            0
        }
        ioctl::PERF_EVENT_IOC_MODIFY_ATTRIBUTES => {
            // Modify event attributes
            0
        }
        _ => -22, // EINVAL
    }
}

/// Read from perf event FD
pub fn sys_perf_event_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    let event_id = match get_event_id_from_fd(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    let value = match event_read(event_id) {
        Ok(v) => v,
        Err(e) => return e.to_errno() as i64,
    };

    // Calculate size based on read format
    let size = core::mem::size_of::<PerfEventReadValue>();
    if count < size {
        return -22; // EINVAL
    }

    // Copy to user buffer
    unsafe {
        let value_bytes = core::slice::from_raw_parts(
            &value as *const _ as *const u8,
            size
        );
        core::ptr::copy_nonoverlapping(value_bytes.as_ptr(), buf, size);
    }

    size as i64
}

/// Mmap perf event ring buffer
pub fn sys_perf_event_mmap(fd: i32, len: usize, _prot: i32, _flags: i32, _offset: i64) -> i64 {
    let event_id = match get_event_id_from_fd(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    // Validate length (must be 1 + 2^n pages)
    let pages = len / 4096;
    if pages < 1 || !is_power_of_two(pages - 1) {
        return -22; // EINVAL
    }

    // Get or create ring buffer
    match mmap_ring_buffer(event_id, pages - 1) {
        Ok(addr) => addr as i64,
        Err(e) => e.to_errno() as i64,
    }
}

/// Close perf event FD
pub fn sys_perf_event_close(fd: i32) -> i64 {
    let event_id = match get_event_id_from_fd(fd) {
        Some(id) => id,
        None => return -9, // EBADF
    };

    match event_close(event_id) {
        Ok(()) => {
            remove_perf_fd(fd);
            0
        }
        Err(e) => e.to_errno() as i64,
    }
}

/// Copy perf_event_attr from userspace
fn copy_attr_from_user(ptr: u64) -> Result<PerfEventAttr, i32> {
    if ptr == 0 {
        return Err(-14); // EFAULT
    }

    // Read size field first
    let size = unsafe { *(ptr as *const u32) };

    if size < 8 {
        return Err(-22); // EINVAL
    }

    // Create attr with defaults
    let mut attr = PerfEventAttr::default();

    // Copy common fields
    unsafe {
        let user_ptr = ptr as *const u8;

        // Type at offset 0
        attr.event_type = match *(user_ptr.add(0) as *const u32) {
            0 => PerfEventType::Hardware,
            1 => PerfEventType::Software,
            2 => PerfEventType::Tracepoint,
            3 => PerfEventType::HardwareCache,
            4 => PerfEventType::Raw,
            5 => PerfEventType::Breakpoint,
            _ => return Err(-22),
        };

        // Size at offset 4
        attr.size = *(user_ptr.add(4) as *const u32);

        // Config at offset 8
        if size >= 16 {
            attr.config = *(user_ptr.add(8) as *const u64);
        }

        // Sample period/freq at offset 16
        if size >= 24 {
            attr.sample_period = *(user_ptr.add(16) as *const u64);
        }

        // Sample type at offset 24
        if size >= 32 {
            attr.sample_type = SampleTypeFlags::from_bits_truncate(
                *(user_ptr.add(24) as *const u64)
            );
        }

        // Read format at offset 32
        if size >= 40 {
            attr.read_format = ReadFormatFlags::from_bits_truncate(
                *(user_ptr.add(32) as *const u64)
            );
        }

        // Bit flags at offset 40
        if size >= 48 {
            let flags = *(user_ptr.add(40) as *const u64);
            attr.disabled = (flags & (1 << 0)) != 0;
            attr.inherit = (flags & (1 << 1)) != 0;
            attr.pinned = (flags & (1 << 2)) != 0;
            attr.exclusive = (flags & (1 << 3)) != 0;
            attr.exclude_user = (flags & (1 << 4)) != 0;
            attr.exclude_kernel = (flags & (1 << 5)) != 0;
            attr.exclude_hv = (flags & (1 << 6)) != 0;
            attr.exclude_idle = (flags & (1 << 7)) != 0;
            attr.mmap = (flags & (1 << 8)) != 0;
            attr.comm = (flags & (1 << 9)) != 0;
            attr.freq = (flags & (1 << 10)) != 0;
            attr.inherit_stat = (flags & (1 << 11)) != 0;
            attr.enable_on_exec = (flags & (1 << 12)) != 0;
            attr.task = (flags & (1 << 13)) != 0;
            attr.watermark = (flags & (1 << 14)) != 0;
            attr.precise_ip = ((flags >> 15) & 3) as u8;
            attr.mmap_data = (flags & (1 << 17)) != 0;
            attr.sample_id_all = (flags & (1 << 18)) != 0;
        }
    }

    Ok(attr)
}

/// Check permissions for perf_event_open
fn check_perf_permissions(attr: &PerfEventAttr, pid: i32, cpu: i32) -> bool {
    // Check sysctl perf_event_paranoid level
    // -1: Allow all
    //  0: Allow CPU-wide events for all
    //  1: Allow CPU-wide for admin only
    //  2: Only per-task events
    //  3: Disable perf entirely

    let paranoid = get_perf_paranoid();

    if paranoid == 3 {
        return false;
    }

    // Check if caller is privileged
    let privileged = has_perf_capability();

    if paranoid <= -1 {
        return true;
    }

    // Per-task monitoring of own process always allowed
    if pid == 0 && cpu == -1 {
        return true;
    }

    // CPU-wide monitoring requires privilege at paranoid >= 1
    if cpu >= 0 && !privileged && paranoid >= 1 {
        return false;
    }

    // Kernel events require privilege at paranoid >= 2
    if !attr.exclude_kernel && !privileged && paranoid >= 2 {
        return false;
    }

    true
}

/// Get perf_event_paranoid sysctl value
fn get_perf_paranoid() -> i32 {
    // Would read from /proc/sys/kernel/perf_event_paranoid
    // Default to 2 (restrictive)
    2
}

/// Check if caller has CAP_PERFMON or CAP_SYS_ADMIN
fn has_perf_capability() -> bool {
    // Would check capabilities
    true
}

/// Create file descriptor for perf event
fn create_perf_fd(_event_id: u64) -> Result<i32, PerfError> {
    // Would allocate FD and associate with event
    Ok(0)
}

/// Get event ID from file descriptor
fn get_event_id_from_fd(_fd: i32) -> Option<u64> {
    // Would look up event ID in FD table
    Some(1)
}

/// Remove perf FD mapping
fn remove_perf_fd(_fd: i32) {
    // Would remove from FD table
}

/// Attach BPF program to perf event
fn attach_bpf_to_event(_event_id: u64, _bpf_fd: i32) -> i64 {
    // Would attach BPF program to event
    0
}

/// Mmap ring buffer for event
fn mmap_ring_buffer(_event_id: u64, _data_pages: usize) -> Result<u64, PerfError> {
    // Would allocate and map ring buffer
    Ok(0)
}

/// Check if n is a power of two
fn is_power_of_two(n: usize) -> bool {
    n != 0 && (n & (n - 1)) == 0
}

/// Syscall number constants
pub mod syscall_nr {
    /// perf_event_open syscall number (x86_64)
    pub const PERF_EVENT_OPEN: u64 = 298;
}

/// Register perf syscall handlers
pub fn register_syscalls() {
    // Would register syscall handlers with syscall table
    crate::kprintln!("[PERF] Syscall handlers registered");
}
