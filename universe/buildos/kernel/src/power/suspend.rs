//! Suspend/Resume Support
//!
//! Provides system suspend and hibernation:
//! - Suspend to RAM (S3)
//! - Suspend to Disk (S4/Hibernate)
//! - Hybrid suspend
//! - Device suspend/resume callbacks

#![allow(dead_code)]

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use super::{PowerError, PowerState};

/// Suspend state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuspendState {
    /// Not suspending
    Running,
    /// Freeze processes
    Freeze,
    /// Prepare devices
    Prepare,
    /// Suspend devices (late)
    SuspendLate,
    /// Suspend devices (noirq)
    SuspendNoirq,
    /// Enter sleep
    Suspend,
    /// Resume from sleep
    Resume,
    /// Resume devices (noirq)
    ResumeNoirq,
    /// Resume devices (early)
    ResumeEarly,
    /// Complete resume
    Complete,
    /// Thaw processes
    Thaw,
}

/// Suspend operations for devices
pub trait SuspendOps: Send + Sync {
    /// Prepare for suspend
    fn prepare(&self) -> Result<(), PowerError> { Ok(()) }

    /// Suspend device
    fn suspend(&self) -> Result<(), PowerError> { Ok(()) }

    /// Late suspend (after IRQs disabled)
    fn suspend_late(&self) -> Result<(), PowerError> { Ok(()) }

    /// Suspend with no IRQs
    fn suspend_noirq(&self) -> Result<(), PowerError> { Ok(()) }

    /// Resume with no IRQs
    fn resume_noirq(&self) -> Result<(), PowerError> { Ok(()) }

    /// Early resume (before IRQs enabled)
    fn resume_early(&self) -> Result<(), PowerError> { Ok(()) }

    /// Resume device
    fn resume(&self) -> Result<(), PowerError> { Ok(()) }

    /// Complete resume
    fn complete(&self) -> Result<(), PowerError> { Ok(()) }
}

/// Device suspend callback registration
struct DeviceCallback {
    /// Device name
    name: String,
    /// Priority (lower = earlier suspend, later resume)
    priority: i32,
    /// Operations
    ops: Box<dyn SuspendOps>,
}

/// Suspend manager
pub struct SuspendManager {
    /// Current suspend state
    state: AtomicU32,
    /// Is suspend in progress
    suspending: AtomicBool,
    /// Suspend target
    target: Mutex<PowerState>,
    /// Registered device callbacks
    devices: RwLock<Vec<DeviceCallback>>,
    /// Suspend blockers
    blockers: RwLock<Vec<SuspendBlocker>>,
    /// Wake locks
    wake_locks: RwLock<BTreeMap<String, WakeLock>>,
    /// Statistics
    stats: SuspendStats,
    /// CPU context save area
    cpu_context: Mutex<Option<CpuContext>>,
}

/// Suspend blocker
#[derive(Clone, Debug)]
struct SuspendBlocker {
    /// Name
    name: String,
    /// Process ID
    pid: u32,
    /// Count
    count: u32,
}

/// Wake lock
#[derive(Clone, Debug)]
pub struct WakeLock {
    /// Name
    pub name: String,
    /// Type
    pub lock_type: WakeLockType,
    /// Active
    pub active: bool,
    /// Timeout (if any)
    pub timeout_ms: Option<u64>,
    /// Created time
    pub created: u64,
}

/// Wake lock type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeLockType {
    /// Partial wake lock (CPU on, screen off)
    Partial,
    /// Full wake lock (CPU and screen on)
    Full,
    /// Screen dim (screen dim, CPU on)
    ScreenDim,
    /// Screen bright (screen bright, CPU on)
    ScreenBright,
    /// Proximity (screen off but CPU active)
    Proximity,
}

/// Suspend statistics
#[derive(Debug, Default)]
struct SuspendStats {
    /// Total suspends
    suspend_count: AtomicU64,
    /// Successful suspends
    success_count: AtomicU64,
    /// Failed suspends
    fail_count: AtomicU64,
    /// Abort count
    abort_count: AtomicU64,
    /// Last suspend time
    last_suspend_time: AtomicU64,
    /// Last resume time
    last_resume_time: AtomicU64,
    /// Total suspend duration
    total_suspend_duration: AtomicU64,
}

/// CPU context for resume
#[derive(Clone, Debug, Default)]
struct CpuContext {
    /// General purpose registers
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rsp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    /// RIP
    rip: u64,
    /// RFLAGS
    rflags: u64,
    /// Control registers
    cr0: u64,
    cr3: u64,
    cr4: u64,
    /// Segment selectors
    cs: u16,
    ds: u16,
    es: u16,
    fs: u16,
    gs: u16,
    ss: u16,
    /// GDT
    gdt_base: u64,
    gdt_limit: u16,
    /// IDT
    idt_base: u64,
    idt_limit: u16,
}

impl SuspendManager {
    /// Create new suspend manager
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
            suspending: AtomicBool::new(false),
            target: Mutex::new(PowerState::S0Working),
            devices: RwLock::new(Vec::new()),
            blockers: RwLock::new(Vec::new()),
            wake_locks: RwLock::new(BTreeMap::new()),
            stats: SuspendStats {
                suspend_count: AtomicU64::new(0),
                success_count: AtomicU64::new(0),
                fail_count: AtomicU64::new(0),
                abort_count: AtomicU64::new(0),
                last_suspend_time: AtomicU64::new(0),
                last_resume_time: AtomicU64::new(0),
                total_suspend_duration: AtomicU64::new(0),
            },
            cpu_context: Mutex::new(None),
        }
    }

    /// Get current state
    pub fn current_state(&self) -> SuspendState {
        match self.state.load(Ordering::Acquire) {
            0 => SuspendState::Running,
            1 => SuspendState::Freeze,
            2 => SuspendState::Prepare,
            3 => SuspendState::SuspendLate,
            4 => SuspendState::SuspendNoirq,
            5 => SuspendState::Suspend,
            6 => SuspendState::Resume,
            7 => SuspendState::ResumeNoirq,
            8 => SuspendState::ResumeEarly,
            9 => SuspendState::Complete,
            10 => SuspendState::Thaw,
            _ => SuspendState::Running,
        }
    }

    /// Register device for suspend callbacks
    pub fn register_device(&self, name: &str, priority: i32, ops: Box<dyn SuspendOps>) {
        let callback = DeviceCallback {
            name: String::from(name),
            priority,
            ops,
        };

        let mut devices = self.devices.write();
        devices.push(callback);

        // Sort by priority
        devices.sort_by_key(|d| d.priority);
    }

    /// Unregister device
    pub fn unregister_device(&self, name: &str) {
        self.devices.write().retain(|d| d.name != name);
    }

    /// Acquire wake lock
    pub fn acquire_wake_lock(&self, name: &str, lock_type: WakeLockType, timeout_ms: Option<u64>) {
        let lock = WakeLock {
            name: String::from(name),
            lock_type,
            active: true,
            timeout_ms,
            created: get_timestamp(),
        };

        self.wake_locks.write().insert(String::from(name), lock);
    }

    /// Release wake lock
    pub fn release_wake_lock(&self, name: &str) {
        if let Some(lock) = self.wake_locks.write().get_mut(name) {
            lock.active = false;
        }
    }

    /// Check if any wake locks are active
    pub fn has_active_wake_locks(&self) -> bool {
        let now = get_timestamp();
        self.wake_locks.read().values().any(|lock| {
            if !lock.active {
                return false;
            }
            // Check timeout
            if let Some(timeout) = lock.timeout_ms {
                if now - lock.created > timeout {
                    return false;
                }
            }
            true
        })
    }

    /// Suspend to RAM
    pub fn suspend_to_ram(&self) -> Result<(), PowerError> {
        self.suspend_enter(PowerState::S3SuspendToRam)
    }

    /// Hibernate
    pub fn hibernate(&self) -> Result<(), PowerError> {
        self.suspend_enter(PowerState::S4Hibernate)
    }

    /// Enter suspend state
    fn suspend_enter(&self, target: PowerState) -> Result<(), PowerError> {
        // Check for blockers
        if self.has_active_wake_locks() {
            return Err(PowerError::Busy);
        }

        // Set suspending flag
        if self.suspending.compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed).is_err() {
            return Err(PowerError::Busy);
        }

        *self.target.lock() = target;
        self.stats.suspend_count.fetch_add(1, Ordering::Relaxed);

        let result = self.do_suspend(target);

        self.suspending.store(false, Ordering::Release);

        if result.is_ok() {
            self.stats.success_count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats.fail_count.fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    /// Perform suspend sequence
    fn do_suspend(&self, target: PowerState) -> Result<(), PowerError> {
        // Phase 1: Freeze processes
        self.set_state(SuspendState::Freeze);
        self.freeze_processes()?;

        // Phase 2: Prepare devices
        self.set_state(SuspendState::Prepare);
        if let Err(e) = self.prepare_devices() {
            self.thaw_processes();
            return Err(e);
        }

        // Phase 3: Suspend devices
        self.set_state(SuspendState::SuspendLate);
        if let Err(e) = self.suspend_devices() {
            self.resume_devices();
            self.complete_devices();
            self.thaw_processes();
            return Err(e);
        }

        // Phase 4: Disable IRQs and late suspend
        self.set_state(SuspendState::SuspendNoirq);
        unsafe { disable_irqs(); }

        if let Err(e) = self.suspend_devices_noirq() {
            unsafe { enable_irqs(); }
            self.resume_devices();
            self.complete_devices();
            self.thaw_processes();
            return Err(e);
        }

        // Phase 5: Save CPU context
        self.save_cpu_context();

        // Phase 6: Enter sleep state
        self.set_state(SuspendState::Suspend);

        if target == PowerState::S4Hibernate {
            self.do_hibernate()?;
        } else {
            super::acpi::enter_sleep_state(target)?;
        }

        // === RESUME PATH ===

        // Phase 7: Restore CPU context
        self.restore_cpu_context();

        // Phase 8: Resume devices (noirq)
        self.set_state(SuspendState::ResumeNoirq);
        self.resume_devices_noirq();

        // Phase 9: Enable IRQs
        unsafe { enable_irqs(); }

        // Phase 10: Early resume
        self.set_state(SuspendState::ResumeEarly);
        self.resume_devices_early();

        // Phase 11: Resume devices
        self.set_state(SuspendState::Resume);
        self.resume_devices();

        // Phase 12: Complete
        self.set_state(SuspendState::Complete);
        self.complete_devices();

        // Phase 13: Thaw processes
        self.set_state(SuspendState::Thaw);
        self.thaw_processes();

        // Done
        self.set_state(SuspendState::Running);

        Ok(())
    }

    /// Set suspend state
    fn set_state(&self, state: SuspendState) {
        self.state.store(state as u32, Ordering::Release);
    }

    /// Freeze processes
    fn freeze_processes(&self) -> Result<(), PowerError> {
        // Would iterate over all processes and freeze them
        // For now, just signal them to freeze
        crate::kprintln!("[SUSPEND] Freezing processes...");
        Ok(())
    }

    /// Thaw processes
    fn thaw_processes(&self) {
        crate::kprintln!("[SUSPEND] Thawing processes...");
        // Would iterate over all processes and thaw them
    }

    /// Prepare devices
    fn prepare_devices(&self) -> Result<(), PowerError> {
        let devices = self.devices.read();
        for device in devices.iter() {
            if let Err(e) = device.ops.prepare() {
                crate::kprintln!("[SUSPEND] Device {} prepare failed: {:?}", device.name, e);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Suspend devices
    fn suspend_devices(&self) -> Result<(), PowerError> {
        let devices = self.devices.read();
        // Suspend in priority order (low to high)
        for device in devices.iter() {
            if let Err(e) = device.ops.suspend() {
                crate::kprintln!("[SUSPEND] Device {} suspend failed: {:?}", device.name, e);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Suspend devices (noirq)
    fn suspend_devices_noirq(&self) -> Result<(), PowerError> {
        let devices = self.devices.read();
        for device in devices.iter() {
            if let Err(e) = device.ops.suspend_noirq() {
                crate::kprintln!("[SUSPEND] Device {} suspend_noirq failed: {:?}", device.name, e);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Resume devices (noirq)
    fn resume_devices_noirq(&self) {
        let devices = self.devices.read();
        // Resume in reverse priority order
        for device in devices.iter().rev() {
            let _ = device.ops.resume_noirq();
        }
    }

    /// Resume devices (early)
    fn resume_devices_early(&self) {
        let devices = self.devices.read();
        for device in devices.iter().rev() {
            let _ = device.ops.resume_early();
        }
    }

    /// Resume devices
    fn resume_devices(&self) {
        let devices = self.devices.read();
        for device in devices.iter().rev() {
            let _ = device.ops.resume();
        }
    }

    /// Complete devices
    fn complete_devices(&self) {
        let devices = self.devices.read();
        for device in devices.iter().rev() {
            let _ = device.ops.complete();
        }
    }

    /// Save CPU context
    fn save_cpu_context(&self) {
        let ctx = CpuContext::save();
        *self.cpu_context.lock() = Some(ctx);
    }

    /// Restore CPU context
    fn restore_cpu_context(&self) {
        if let Some(ctx) = self.cpu_context.lock().take() {
            ctx.restore();
        }
    }

    /// Hibernate (save to disk)
    fn do_hibernate(&self) -> Result<(), PowerError> {
        crate::kprintln!("[SUSPEND] Creating hibernation image...");

        // Would:
        // 1. Create snapshot of memory
        // 2. Write to swap partition
        // 3. Enter S4 state

        // For now, just enter S4
        super::acpi::enter_sleep_state(PowerState::S4Hibernate)
    }
}

impl CpuContext {
    /// Save current CPU context
    fn save() -> Self {
        let mut ctx = CpuContext::default();

        unsafe {
            // Save general purpose registers
            core::arch::asm!(
                "mov {}, rax",
                "mov {}, rbx",
                "mov {}, rcx",
                "mov {}, rdx",
                out(reg) ctx.rax,
                out(reg) ctx.rbx,
                out(reg) ctx.rcx,
                out(reg) ctx.rdx,
            );

            core::arch::asm!(
                "mov {}, rsi",
                "mov {}, rdi",
                "mov {}, rbp",
                "mov {}, rsp",
                out(reg) ctx.rsi,
                out(reg) ctx.rdi,
                out(reg) ctx.rbp,
                out(reg) ctx.rsp,
            );

            core::arch::asm!(
                "mov {}, r8",
                "mov {}, r9",
                "mov {}, r10",
                "mov {}, r11",
                out(reg) ctx.r8,
                out(reg) ctx.r9,
                out(reg) ctx.r10,
                out(reg) ctx.r11,
            );

            core::arch::asm!(
                "mov {}, r12",
                "mov {}, r13",
                "mov {}, r14",
                "mov {}, r15",
                out(reg) ctx.r12,
                out(reg) ctx.r13,
                out(reg) ctx.r14,
                out(reg) ctx.r15,
            );

            // Save control registers
            core::arch::asm!(
                "mov {}, cr0",
                "mov {}, cr3",
                "mov {}, cr4",
                out(reg) ctx.cr0,
                out(reg) ctx.cr3,
                out(reg) ctx.cr4,
            );

            // Save flags
            core::arch::asm!(
                "pushfq",
                "pop {}",
                out(reg) ctx.rflags,
            );
        }

        ctx
    }

    /// Restore CPU context
    fn restore(&self) {
        unsafe {
            // Restore control registers
            core::arch::asm!(
                "mov cr0, {}",
                "mov cr3, {}",
                "mov cr4, {}",
                in(reg) self.cr0,
                in(reg) self.cr3,
                in(reg) self.cr4,
            );

            // Restore flags
            core::arch::asm!(
                "push {}",
                "popfq",
                in(reg) self.rflags,
            );
        }
    }
}

/// Global suspend manager
static SUSPEND_MANAGER: SuspendManager = SuspendManager::new();

/// Initialize suspend subsystem
pub fn init() {
    crate::kprintln!("[POWER] Suspend subsystem initialized");
}

/// Get timestamp
fn get_timestamp() -> u64 {
    0 // Would use system timer
}

/// Disable IRQs
unsafe fn disable_irqs() {
    core::arch::asm!("cli");
}

/// Enable IRQs
unsafe fn enable_irqs() {
    core::arch::asm!("sti");
}

/// Suspend to RAM
pub fn suspend_to_ram() -> Result<(), PowerError> {
    SUSPEND_MANAGER.suspend_to_ram()
}

/// Hibernate
pub fn hibernate() -> Result<(), PowerError> {
    SUSPEND_MANAGER.hibernate()
}

/// Register device for suspend
pub fn register_device(name: &str, priority: i32, ops: Box<dyn SuspendOps>) {
    SUSPEND_MANAGER.register_device(name, priority, ops);
}

/// Acquire wake lock
pub fn acquire_wake_lock(name: &str, lock_type: WakeLockType, timeout_ms: Option<u64>) {
    SUSPEND_MANAGER.acquire_wake_lock(name, lock_type, timeout_ms);
}

/// Release wake lock
pub fn release_wake_lock(name: &str) {
    SUSPEND_MANAGER.release_wake_lock(name);
}

/// Get current suspend state
pub fn current_state() -> SuspendState {
    SUSPEND_MANAGER.current_state()
}
