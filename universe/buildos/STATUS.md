# BuildOS Status

Hobby x86-64 kernel written in Rust. Educational/portfolio project. NOT a production OS.

The `.bld` file (`lib.bld`) describes the design in BuildLang syntax.
The actual implementation lives in `kernel/src/` (Rust, `#![no_std]`).

## Build

- Target: `x86_64-buildos.json` (custom bare-metal target, `rust-lld` linker)
- Dependencies: `spin 0.9`, `bitflags 2.4`, `static_assertions 1.1`, `libm 0.2`
- Nightly Rust required (`abi_x86_interrupt`, `alloc_error_handler`, `negative_impls`)
- The kernel compiles to `libbuildos_kernel.rlib` (build artifacts exist)
- UEFI bootloader in `bootloader/` (depends on `uefi 0.31`)
- QEMU run script in `scripts/run-qemu.sh`
- **Has it booted on real hardware?** Unknown. QEMU-only as far as artifacts show.

## Subsystem Status

| Subsystem | State | Notes |
|---|---|---|
| Boot / entry point | IMPLEMENTED | `kernel_main` with phased init, panic handler, halt loop |
| GDT | IMPLEMENTED | `gdt.rs` |
| Interrupts | IMPLEMENTED | IDT setup, `abi_x86_interrupt` |
| Syscall entry | IMPLEMENTED | `syscall_entry.rs` (MSR-based SYSCALL/SYSRET) |
| Memory management | IMPLEMENTED | Physical allocator, page tables, heap, slab, NUMA, OOM killer, virtual memory |
| Process management | IMPLEMENTED | PID, fork, execve, exit, wait4, clone, signals, futex |
| Scheduler | IMPLEMENTED | Priority-based + CFS + realtime. SMP per-CPU queues, load balancing, affinity |
| Filesystem | IMPLEMENTED | VFS layer, ext2, ext4, FAT32, FUSE, initramfs, shmfs, epoll, io_uring, inotify |
| IPC | IMPLEMENTED | Pipes, SysV message queues, shared memory, semaphores, eventfd, signalfd, POSIX MQ |
| Networking | IMPLEMENTED | TCP/IP stack, ARP, ICMP, DNS, DHCP, HTTP, TLS, UDP, netfilter/NAT |
| Drivers: serial | IMPLEMENTED | Serial console for early debug |
| Drivers: framebuffer | IMPLEMENTED | Console output, font rendering, VESA |
| Drivers: keyboard | IMPLEMENTED | PS/2 keyboard |
| Drivers: PCI | IMPLEMENTED | PCI bus enumeration |
| Drivers: ACPI | IMPLEMENTED | Table parsing, CPU/IOAPIC discovery |
| Drivers: storage | IMPLEMENTED | AHCI + NVMe |
| Drivers: timer | IMPLEMENTED | HPET, APIC Timer, PIT, TSC; high-resolution timers |
| Drivers: network | IMPLEMENTED | virtio-net driver |
| TTY | IMPLEMENTED | Console, PTY, ANSI escape codes, line discipline |
| ELF loader | IMPLEMENTED | `elf.rs` for loading userspace binaries |
| Sync primitives | IMPLEMENTED | Spinlock, mutex, rwlock, semaphore, condvar, barrier, seqlock, RCU, once, futex |
| Security | IMPLEMENTED | LSM framework, capabilities, seccomp, audit, credentials, namespaces |
| Cgroups | IMPLEMENTED | CPU, memory, I/O, PIDs, devices, freezer |
| Namespaces | IMPLEMENTED | PID, NET, MNT, UTS, IPC, USER |
| Device mapper | IMPLEMENTED | LVM, linear, stripe, snapshot, thin provisioning |
| Module loader | IMPLEMENTED | Dynamic kernel modules with ELF loading, symbol resolution |
| BPF | IMPLEMENTED | Maps, programs, verifier, JIT, helpers |
| Crypto | IMPLEMENTED | Cipher, hash, MAC, AEAD, KDF, RNG, AF_ALG |
| Power management | IMPLEMENTED | ACPI, cpufreq, thermal, battery, suspend |
| USB | IMPLEMENTED | EHCI, xHCI, HID, mass storage, hub |
| Sound | IMPLEMENTED | AC97, HDA, mixer, PCM |
| GPU/DRM | IMPLEMENTED | DRM, KMS, GEM, framebuffer |
| Bluetooth | IMPLEMENTED | HCI, L2CAP, SMP, RFCOMM, A2DP, HID, SDP |
| Input | IMPLEMENTED | Keyboard, mouse, touch, gamepad, force feedback |
| GUI (compositor) | IMPLEMENTED | Window manager, compositor, widgets, theming |
| Init system | IMPLEMENTED | Service manager, units, targets, journal, dependency resolution |
| Logging/tracing | IMPLEMENTED | Ring buffer logging, ftrace, tracepoints, kprobes, perf events |
| Watchdog | IMPLEMENTED | Soft lockup, hard lockup, hung task detection |
| Debug | IMPLEMENTED | Breakpoints, watchpoints, register inspection |
| Virtualization | IMPLEMENTED | VMX, SVM, vCPU, virtual I/O, virtual IRQ, virtual memory |
| Perf | IMPLEMENTED | PMU, counters, sampling, uprobes, kprobes, tracepoints |
| Random | IMPLEMENTED | ChaCha20-based CSPRNG, entropy pool |
| AI subsystem | PARTIAL | Data structures defined (models, tensors, dtypes). Init code runs. But see syscalls below. |
| Self-healing engine | PARTIAL | Checkpoint/restore structures defined. Anomaly detector scaffolding. Calls healing functions. No ML model actually runs. |
| Userspace | PARTIAL | `userspace/` has init, shell, coreutils, libbuild -- separate Cargo workspace |

## Syscalls

71 `sys_*` handler functions total.

- **66 implemented** with real logic (file I/O, process ops, memory, time, sockets, IPC, signals, poll/select)
- **5 stubs** that return `-1` with `// TODO: Implement`:
  `sys_ai_query`, `sys_ai_infer`, `sys_ai_tensor_alloc`, `sys_ai_tensor_free`, `sys_ai_tensor_share`
- `sys_ai_priority_boost` delegates to scheduler (implemented)
- `sys_checkpoint`, `sys_restore`, `sys_heal` delegate to healing engine (partially implemented)

## Honest Assessment of Headline Features

**"Self-Healing Engine"** -- The `healing.rs` module has checkpoint/restore data structures,
an anomaly detector with statistical scaffolding (mean/stddev), and recovery event tracking.
It is a real subsystem with real code. It does NOT contain a trained ML model or neural network.
The "anomaly detection using ML" described in comments is aspirational -- it uses basic
statistical thresholds, not machine learning.

**"Neural scheduling"** -- The scheduler has a class called "AI-optimized scheduler for neural
workloads" in `sched/classes.rs`. The comment admits: "Would use neural network to predict."
It does not. This is a priority-boost mechanism, not neural scheduling.

**"AI inference in the kernel"** -- The AI subsystem (`ai.rs`) defines model and tensor
structures. The 5 core AI syscalls are stubs returning -1. No inference engine exists.
The infrastructure (tensor types, model loading API) is designed but not functional.

**Can it boot?** -- Build artifacts exist (`libbuildos_kernel.rlib`). A UEFI bootloader
and QEMU script exist. The boot sequence in `kernel_main` is well-structured. Whether it
successfully boots in QEMU has not been independently verified.

## Relationship to BUILD-UNIVERSE

This is one module in the larger BUILD-UNIVERSE ecosystem. The `lib.bld` file (50K+ tokens)
is the BuildLang design specification. The `kernel/src/` Rust code is the implementation.
They describe the same system but the `.bld` file includes aspirational design elements
that are not yet implemented in Rust.
