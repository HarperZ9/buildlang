# BuildOS Core

Pure BuildLang microkernel and capability runtime modules targeting RISC-V Sv39 and Linux Landlock/seccomp sandboxing. Every module compiles cleanly with buildc check, passes buildc lint with zero warnings, and runs self-contained verification harnesses with planted fail-closed control traps.

## Modules

1. `buildos_core_cap_sv39.bld`: Capability CNode slots with monotonic attenuation, generational revocation, Sv39 page table enforcement with strict write xor execute checks, physical frame allocator, O(1) runqueue, 1,000-step double-run replay determinism, and 6 planted fault controls.
2. `buildos_cap_sandbox.bld`: Pure capability sandbox policy compiler lowering file, network, and syscall capability profiles to deterministic Landlock and seccomp policy manifests with pure SHA-256 digests and 4 planted escalation controls.
3. `buildos_fb_boot_slice2.bld`: Capability-gated 320x240 RGBA framebuffer boot slice with double-buffered composition, bounded viewport blits, deterministic CRC/digest verification, and 4 planted capability and bounds controls.
4. `buildos_ab_state.bld`: Atomic A/B boot-slot state machine and checksummed manifest store with fail-closed rollback on corrupted state, 500-case corruption fuzzing with zero silent acceptance, and 4 planted integrity controls.
5. `buildos_supervisor.bld`: Erlang-style ONE_FOR_ONE and ONE_FOR_ALL actor supervisor with bounded restart windows, automatic capability revocation on child crash, crash-storm escalation to A/B slot rollback, and 4 planted supervision controls.

## Verification

Run `buildc check`, `buildc lint`, and `buildc run` in release mode on each `.bld` source file in this directory. Each binary prints deterministic pass counters and exits with code 0 when all invariants and planted controls succeed.

## License

Licensed under the MIT License. See `LICENSE` in this directory.
