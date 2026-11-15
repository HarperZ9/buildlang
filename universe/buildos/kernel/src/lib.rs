// ===============================================================================
// BUILDOS KERNEL
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// BuildOS Kernel featuring:
// - Process scheduler with priority-based scheduling
// - Self-Healing Engine with automatic fault recovery
// - First-class AI inference support
// - Differential checkpointing for instant recovery
// - Zero-copy AI tensor sharing between processes
//
// ===============================================================================

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(negative_impls)]

#[macro_use]
extern crate alloc;

pub mod boot;
pub mod memory;
pub mod cpu;
pub mod gdt;
pub mod interrupts;
pub mod process;
pub mod context;
pub mod scheduler;
pub mod sched;
pub mod syscall;
pub mod syscall_entry;
pub mod drivers;
pub mod fs;
pub mod ipc;
pub mod ai;
pub mod healing;
pub mod elf;
pub mod net;
pub mod gui;
pub mod tty;
pub mod sync;
pub mod init;
pub mod security;
pub mod time;
pub mod random;
pub mod logging;
pub mod log;
pub mod dm;
pub mod module;
pub mod cgroups;
pub mod namespace;
pub mod bpf;
pub mod virt;
pub mod debug;
pub mod perf;
pub mod crypto;
pub mod power;
pub mod watchdog;
pub mod tracing;
pub mod usb;
pub mod sound;
pub mod gpu;
pub mod bluetooth;
pub mod input;
pub mod math;

use core::panic::PanicInfo;

use boot::BootInfo;
use memory::MemoryManager;

// =============================================================================
// KERNEL ENTRY POINT
// =============================================================================

/// Kernel main entry point, called by bootloader
#[no_mangle]
pub extern "sysv64" fn kernel_main(boot_info: *const BootInfo) -> ! {
    // Validate boot info
    let boot_info = unsafe {
        if boot_info.is_null() {
            halt_loop();
        }
        &*boot_info
    };

    if boot_info.magic != BootInfo::MAGIC {
        halt_loop();
    }

    // Phase 1: Initialize core subsystems
    unsafe {
        cpu::init_bsp();
        interrupts::init();
        syscall_entry::init();
    }

    // Phase 2: Initialize memory management
    let _memory_manager = unsafe {
        MemoryManager::init(boot_info)
    };

    // Phase 2b: Initialize OOM killer
    memory::oom::init();

    // Phase 3: Initialize framebuffer console
    if boot_info.framebuffer.address != 0 {
        unsafe {
            drivers::framebuffer::init(&boot_info.framebuffer);
        }
    }

    kprintln!("=================================================");
    kprintln!("  BuildOS v2.0.0");
    kprintln!("  Copyright 2024-2025 Zain Dana Harper");
    kprintln!("=================================================");
    kprintln!("");

    kprintln!("[KERNEL] Memory: {} MB available",
        boot_info.memory_map.total_memory / (1024 * 1024));
    kprintln!("[KERNEL] Framebuffer: {}x{}",
        boot_info.framebuffer.width, boot_info.framebuffer.height);

    // Phase 4: Initialize serial console for early debugging
    kprintln!("[KERNEL] Initializing serial console...");
    drivers::serial::init();

    // Phase 4b: Initialize logging infrastructure
    kprintln!("[KERNEL] Initializing logging subsystem...");
    logging::init();
    logging::events::init(4096);
    logging::ftrace::FunctionTracer::init(1); // Initialize for BSP

    // Phase 5: Parse ACPI tables
    if boot_info.acpi_rsdp != 0 {
        kprintln!("[KERNEL] Parsing ACPI tables...");
        unsafe {
            drivers::acpi::init(boot_info.acpi_rsdp);
        }
        kprintln!("[KERNEL] ACPI: Found {} CPUs, {} I/O APICs",
            drivers::acpi::cpu_count(), drivers::acpi::ioapic_count());
    }

    // Phase 6: Enumerate PCI devices
    kprintln!("[KERNEL] Enumerating PCI devices...");
    drivers::pci::init();
    kprintln!("[KERNEL] PCI: Found {} devices", drivers::pci::device_count());

    // Phase 7: Initialize keyboard driver
    kprintln!("[KERNEL] Initializing PS/2 keyboard...");
    unsafe {
        drivers::keyboard::init();
    }

    // Phase 8: Initialize storage drivers (AHCI + NVMe)
    kprintln!("[KERNEL] Initializing AHCI storage driver...");
    drivers::storage::init();
    let ahci_count = drivers::storage::disk_count();
    if ahci_count > 0 {
        kprintln!("[KERNEL] Storage: Found {} AHCI disk(s)", ahci_count);
    }

    kprintln!("[KERNEL] Initializing NVMe storage driver...");
    drivers::nvme::init();
    let nvme_count = drivers::nvme::drive_count();
    if nvme_count > 0 {
        kprintln!("[KERNEL] Storage: Found {} NVMe drive(s)", nvme_count);
    }

    // Phase 8b: Initialize device mapper / LVM
    kprintln!("[KERNEL] Initializing device mapper subsystem...");
    dm::init();
    kprintln!("[KERNEL] Device mapper: Ready for LVM2");

    // Phase 8c: Initialize kernel module subsystem
    kprintln!("[KERNEL] Initializing kernel module subsystem...");
    module::init();
    kprintln!("[KERNEL] Module loader: Ready for dynamic modules");

    // Phase 8d: Initialize cgroups subsystem
    kprintln!("[KERNEL] Initializing cgroups subsystem...");
    cgroups::init();
    kprintln!("[KERNEL] Cgroups: Resource control ready");

    // Phase 8e: Initialize namespace subsystem
    kprintln!("[KERNEL] Initializing namespace subsystem...");
    namespace::init();
    kprintln!("[KERNEL] Namespaces: PID/NET/MNT/UTS/IPC/USER ready");

    // Phase 8f: Initialize security subsystem
    kprintln!("[KERNEL] Initializing security subsystem...");
    security::init();
    kprintln!("[KERNEL] Security: LSM/capabilities/seccomp/audit ready");

    // Phase 8g: Initialize BPF subsystem
    kprintln!("[KERNEL] Initializing BPF subsystem...");
    bpf::init();
    kprintln!("[KERNEL] BPF: Maps/programs/verifier ready");

    // Phase 9: Initialize network driver (virtio-net)
    kprintln!("[KERNEL] Initializing network driver...");
    drivers::network::init();
    if drivers::network::is_available() {
        kprintln!("[KERNEL] Network: virtio-net initialized, MAC: {}",
            drivers::network::mac_address_string());
    }

    // Phase 9b: Initialize network stack
    kprintln!("[KERNEL] Initializing TCP/IP network stack...");
    net::init();
    kprintln!("[KERNEL] Network stack: TCP/IP ready");

    // Phase 9c: Initialize random number generator
    kprintln!("[KERNEL] Initializing random number generator...");
    random::init();
    kprintln!("[KERNEL] RNG: {} bits entropy available", random::available_entropy());

    // Phase 9d: Initialize cryptographic subsystem
    kprintln!("[KERNEL] Initializing cryptographic subsystem...");
    crypto::init();
    crypto::init_af_alg();

    // Phase 9e: Initialize power management subsystem
    kprintln!("[KERNEL] Initializing power management subsystem...");
    power::init();
    power::cpufreq::init();
    power::thermal::init();
    power::battery::init();
    power::suspend::init();
    kprintln!("[KERNEL] Power: ACPI/thermal/cpufreq/battery ready");

    // Phase 9f: Initialize USB subsystem
    kprintln!("[KERNEL] Initializing USB subsystem...");
    usb::init();
    kprintln!("[KERNEL] USB: Host controllers and drivers ready");

    // Phase 9g: Initialize sound subsystem
    kprintln!("[KERNEL] Initializing sound subsystem...");
    sound::init();
    let sound_cards = sound::card_count();
    if sound_cards > 0 {
        kprintln!("[KERNEL] Sound: {} audio device(s) ready", sound_cards);
    }

    // Phase 9h: Initialize GPU/DRM subsystem
    kprintln!("[KERNEL] Initializing GPU/DRM subsystem...");
    gpu::init();
    let gpu_count = gpu::device_count();
    if gpu_count > 0 {
        kprintln!("[KERNEL] GPU: {} DRM device(s) ready", gpu_count);
    }

    // Phase 9i: Initialize Bluetooth subsystem
    kprintln!("[KERNEL] Initializing Bluetooth subsystem...");
    bluetooth::init();
    kprintln!("[KERNEL] Bluetooth: HCI/L2CAP/SMP/RFCOMM/A2DP/HID ready");

    // Phase 9j: Initialize input subsystem
    kprintln!("[KERNEL] Initializing input subsystem...");
    input::init();
    kprintln!("[KERNEL] Input: Keyboard/mouse/touch/gamepad/force-feedback ready");

    // Phase 10: Initialize timer subsystem
    kprintln!("[KERNEL] Initializing timer subsystem...");
    unsafe {
        drivers::timer::init();
    }
    kprintln!("[KERNEL] Timer: {} source, {}ns resolution",
        match drivers::timer::source() {
            drivers::timer::TimerSource::Hpet => "HPET",
            drivers::timer::TimerSource::ApicTimer => "APIC Timer",
            drivers::timer::TimerSource::Pit => "PIT",
            drivers::timer::TimerSource::Tsc => "TSC",
        },
        drivers::timer::resolution_ns());

    // Phase 10b: Initialize high-resolution timers
    kprintln!("[KERNEL] Initializing high-resolution timers...");
    time::hrtimer_init();
    kprintln!("[KERNEL] Hrtimer: Per-CPU timer queues ready");

    // Phase 11: Initialize scheduler
    kprintln!("[KERNEL] Initializing process scheduler...");
    scheduler::init();

    // Phase 11b: Initialize SMP scheduler with per-CPU queues
    let num_cpus = drivers::acpi::cpu_count().max(1);
    kprintln!("[KERNEL] Initializing SMP scheduler for {} CPUs...", num_cpus);
    sched::init(num_cpus);
    kprintln!("[KERNEL] Scheduler: Per-CPU queues, load balancing, NUMA-aware");

    // Phase 11c: Initialize RCU subsystem
    kprintln!("[KERNEL] Initializing RCU subsystem...");
    sync::rcu::init(num_cpus);
    kprintln!("[KERNEL] RCU: Lock-free synchronization ready");

    // Phase 11d: Initialize CPU hotplug
    kprintln!("[KERNEL] Initializing CPU hotplug...");
    cpu::hotplug::init();
    kprintln!("[KERNEL] Hotplug: Dynamic CPU online/offline support ready");

    // Phase 11e: Initialize workqueue subsystem
    kprintln!("[KERNEL] Initializing workqueue subsystem...");
    sched::workqueue::init();
    kprintln!("[KERNEL] Workqueue: Deferred work execution ready");

    // Phase 12: Initialize filesystem
    kprintln!("[KERNEL] Mounting root filesystem...");
    fs::init();

    // Phase 12: Initialize IPC subsystem
    kprintln!("[KERNEL] Initializing IPC subsystem...");
    ipc::init();

    // Phase 13: Initialize AI subsystem
    kprintln!("[KERNEL] Initializing AI inference engine...");
    ai::init();

    // Phase 14: Initialize self-healing engine
    kprintln!("[KERNEL] Starting Self-Healing Engine...");
    healing::init();

    // Phase 14b: Initialize watchdog subsystem
    kprintln!("[KERNEL] Initializing kernel watchdog...");
    watchdog::init();
    kprintln!("[KERNEL] Watchdog: Soft lockup, hard lockup, hung task detection enabled");

    // Phase 14c: Initialize tracing infrastructure
    kprintln!("[KERNEL] Initializing kernel tracing...");
    tracing::init();
    kprintln!("[KERNEL] Tracing: Tracepoints, kprobes, ftrace, ring buffers ready");

    // Phase 14d: Initialize performance monitoring
    kprintln!("[KERNEL] Initializing performance monitoring...");
    perf::init();
    kprintln!("[KERNEL] Perf: PMU/counters/events/probes ready");

    // Phase 14e: Initialize debug subsystem
    kprintln!("[KERNEL] Initializing debug subsystem...");
    debug::init();
    kprintln!("[KERNEL] Debug: Debugfs/kdb/kdump/lockdep ready");

    // Phase 14f: Initialize virtualization subsystem
    kprintln!("[KERNEL] Initializing virtualization subsystem...");
    virt::init();
    kprintln!("[KERNEL] Virt: KVM/QEMU/VMBus/hypercall ready");

    // Phase 15: Start init process
    kprintln!("[KERNEL] Starting init process...");
    process::start_init();

    // Enable interrupts and enter scheduler
    kprintln!("[KERNEL] Entering scheduler...");
    unsafe {
        interrupts::enable();
    }

    scheduler::run();
}

// =============================================================================
// PANIC HANDLING
// =============================================================================

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("\n!!! KERNEL PANIC !!!");

    if let Some(location) = info.location() {
        kprintln!("Location: {}:{}:{}", location.file(), location.line(), location.column());
    }

    let message = info.message();
    kprintln!("Message: {}", message);

    // Attempt to save crash dump
    healing::save_crash_dump(info);

    halt_loop()
}

#[alloc_error_handler]
fn alloc_error(_layout: core::alloc::Layout) -> ! {
    kprintln!("!!! ALLOCATION FAILED !!!");
    halt_loop()
}

/// Halt the CPU in an infinite loop
fn halt_loop() -> ! {
    loop {
        unsafe {
            core::arch::asm!("cli; hlt");
        }
    }
}

// =============================================================================
// KERNEL PRINT MACROS
// =============================================================================

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::drivers::framebuffer::WRITER.lock(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprint!("\n") };
    ($($arg:tt)*) => {{
        $crate::kprint!("{}\n", format_args!($($arg)*));
    }};
}
