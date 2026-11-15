// ===============================================================================
// BUILDOS KERNEL - BPF PROGRAMS
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

//! BPF Program Types and Attachment
//!
//! Defines program types and how they attach to kernel subsystems.

#![allow(dead_code)]

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use super::{BpfInsn, BpfError, BpfContext, BpfInterpreter};
use super::jit::{BpfJit, CompiledProgram};
use super::verifier::BpfVerifier;

// =============================================================================
// PROGRAM TYPES
// =============================================================================

/// BPF program types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BpfProgType {
    /// Unspecified
    Unspec = 0,
    /// Socket filter
    SocketFilter = 1,
    /// Kprobe
    Kprobe = 2,
    /// Scheduler classifier
    SchedCls = 3,
    /// Scheduler action
    SchedAct = 4,
    /// Tracepoint
    Tracepoint = 5,
    /// XDP (eXpress Data Path)
    XDP = 6,
    /// Perf event
    PerfEvent = 7,
    /// cgroup/skb
    CgroupSkb = 8,
    /// cgroup/sock
    CgroupSock = 9,
    /// LWT (Lightweight tunnel) in
    LwtIn = 10,
    /// LWT out
    LwtOut = 11,
    /// LWT xmit
    LwtXmit = 12,
    /// Sock ops
    SockOps = 13,
    /// sk_skb
    SkSkb = 14,
    /// cgroup/device
    CgroupDevice = 15,
    /// sk_msg
    SkMsg = 16,
    /// Raw tracepoint
    RawTracepoint = 17,
    /// cgroup/sock_addr
    CgroupSockAddr = 18,
    /// LWT seg6 local
    LwtSeg6local = 19,
    /// lirc mode2
    LircMode2 = 20,
    /// sk_reuseport
    SkReuseport = 21,
    /// Flow dissector
    FlowDissector = 22,
    /// cgroup/sysctl
    CgroupSysctl = 23,
    /// Raw tracepoint writable
    RawTracepointWritable = 24,
    /// cgroup/sockopt
    CgroupSockopt = 25,
    /// Tracing (fentry/fexit)
    Tracing = 26,
    /// Struct ops
    StructOps = 27,
    /// Extension
    Ext = 28,
    /// LSM (Linux Security Module)
    Lsm = 29,
    /// sk_lookup
    SkLookup = 30,
    /// Syscall
    Syscall = 31,
}

impl BpfProgType {
    /// Convert from u32
    pub fn from_u32(val: u32) -> Option<Self> {
        if val <= 31 {
            Some(unsafe { core::mem::transmute(val) })
        } else {
            None
        }
    }

    /// Check if program type requires root
    pub fn requires_root(&self) -> bool {
        matches!(
            self,
            BpfProgType::Kprobe
                | BpfProgType::Tracepoint
                | BpfProgType::PerfEvent
                | BpfProgType::RawTracepoint
                | BpfProgType::Tracing
                | BpfProgType::Lsm
        )
    }
}

// =============================================================================
// ATTACH TYPES
// =============================================================================

/// BPF attach types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AttachType {
    /// cgroup inet ingress
    CgroupInetIngress = 0,
    /// cgroup inet egress
    CgroupInetEgress = 1,
    /// cgroup inet sock create
    CgroupInetSockCreate = 2,
    /// cgroup sock ops
    CgroupSockOps = 3,
    /// sk skb stream parser
    SkSkbStreamParser = 4,
    /// sk skb stream verdict
    SkSkbStreamVerdict = 5,
    /// cgroup device
    CgroupDevice = 6,
    /// sk msg verdict
    SkMsgVerdict = 7,
    /// cgroup inet4 bind
    CgroupInet4Bind = 8,
    /// cgroup inet6 bind
    CgroupInet6Bind = 9,
    /// cgroup inet4 connect
    CgroupInet4Connect = 10,
    /// cgroup inet6 connect
    CgroupInet6Connect = 11,
    /// cgroup inet4 post bind
    CgroupInet4PostBind = 12,
    /// cgroup inet6 post bind
    CgroupInet6PostBind = 13,
    /// cgroup udp4 sendmsg
    CgroupUdp4Sendmsg = 14,
    /// cgroup udp6 sendmsg
    CgroupUdp6Sendmsg = 15,
    /// lirc mode2
    LircMode2 = 16,
    /// Flow dissector
    FlowDissector = 17,
    /// cgroup sysctl
    CgroupSysctl = 18,
    /// cgroup udp4 recvmsg
    CgroupUdp4Recvmsg = 19,
    /// cgroup udp6 recvmsg
    CgroupUdp6Recvmsg = 20,
    /// cgroup getsockopt
    CgroupGetsockopt = 21,
    /// cgroup setsockopt
    CgroupSetsockopt = 22,
    /// Trace raw tp
    TraceRawTp = 23,
    /// Trace fentry
    TraceFentry = 24,
    /// Trace fexit
    TraceFexit = 25,
    /// Modify return
    ModifyReturn = 26,
    /// LSM mac
    LsmMac = 27,
    /// Trace iter
    TraceIter = 28,
    /// cgroup inet4 getpeername
    CgroupInet4Getpeername = 29,
    /// cgroup inet6 getpeername
    CgroupInet6Getpeername = 30,
    /// cgroup inet4 getsockname
    CgroupInet4Getsockname = 31,
    /// cgroup inet6 getsockname
    CgroupInet6Getsockname = 32,
    /// XDP devmap
    XdpDevmap = 33,
    /// cgroup inet sock release
    CgroupInetSockRelease = 34,
    /// XDP cpumap
    XdpCpumap = 35,
    /// sk lookup
    SkLookup = 36,
    /// XDP
    Xdp = 37,
    /// sk skb verdict
    SkSkbVerdict = 38,
    /// sk reuseport select
    SkReuseportSelect = 39,
    /// sk reuseport select or migrate
    SkReuseportSelectOrMigrate = 40,
    /// Perf event
    PerfEvent = 41,
}

impl AttachType {
    /// Convert from u32
    pub fn from_u32(val: u32) -> Option<Self> {
        if val <= 41 {
            Some(unsafe { core::mem::transmute(val) })
        } else {
            None
        }
    }
}

// =============================================================================
// BPF PROGRAM
// =============================================================================

/// BPF program
pub struct BpfProgram {
    /// Program ID
    id: u32,
    /// Program type
    prog_type: BpfProgType,
    /// BPF instructions
    insns: Vec<BpfInsn>,
    /// JIT compiled code
    jited: Option<CompiledProgram>,
    /// Program name
    name: String,
    /// Attach type
    attach_type: Option<AttachType>,
    /// Licensed (GPL)
    license: String,
    /// Kernel version
    kern_version: u32,
    /// BTF ID
    btf_id: u32,
    /// Run count
    run_count: AtomicU64,
    /// Run time (ns)
    run_time_ns: AtomicU64,
    /// Reference count
    refs: AtomicU32,
}

impl BpfProgram {
    /// Create new program
    pub fn new(id: u32, prog_type: BpfProgType, insns: Vec<BpfInsn>) -> Self {
        Self {
            id,
            prog_type,
            insns,
            jited: None,
            name: String::new(),
            attach_type: None,
            license: String::from("GPL"),
            kern_version: 0,
            btf_id: 0,
            run_count: AtomicU64::new(0),
            run_time_ns: AtomicU64::new(0),
            refs: AtomicU32::new(1),
        }
    }

    /// Get program ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get program type
    pub fn prog_type(&self) -> BpfProgType {
        self.prog_type
    }

    /// Set program name
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Get program name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Verify the program
    pub fn verify(&self) -> Result<(), BpfError> {
        let verifier = BpfVerifier::new(self.prog_type);
        verifier.verify(&self.insns)
    }

    /// JIT compile the program
    pub fn jit_compile(&mut self) -> Result<(), BpfError> {
        let compiled = BpfJit::compile(&self.insns)?;
        self.jited = Some(compiled);
        Ok(())
    }

    /// Check if JIT compiled
    pub fn is_jited(&self) -> bool {
        self.jited.is_some()
    }

    /// Run the program
    pub fn run(&self, ctx_ptr: u64) -> u64 {
        self.run_count.fetch_add(1, Ordering::Relaxed);

        // Would use JIT if available
        if let Some(_jited) = &self.jited {
            // Would execute JIT code
            // unsafe { jited.execute(ctx_ptr) }
        }

        // Fall back to interpreter
        let interpreter = BpfInterpreter::new();
        let mut ctx = BpfContext::new();
        ctx.set_arg(ctx_ptr);

        match interpreter.run(&self.insns, &mut ctx) {
            Ok(result) => result,
            Err(_) => 0,
        }
    }

    /// Get run count
    pub fn run_count(&self) -> u64 {
        self.run_count.load(Ordering::Relaxed)
    }

    /// Get run time
    pub fn run_time_ns(&self) -> u64 {
        self.run_time_ns.load(Ordering::Relaxed)
    }

    /// Get instruction count
    pub fn insn_count(&self) -> usize {
        self.insns.len()
    }

    /// Add reference
    pub fn add_ref(&self) {
        self.refs.fetch_add(1, Ordering::AcqRel);
    }

    /// Release reference
    pub fn release(&self) -> bool {
        self.refs.fetch_sub(1, Ordering::AcqRel) == 1
    }
}

// =============================================================================
// BPF LINK
// =============================================================================

/// BPF link (attachment point)
pub struct BpfLink {
    /// Link ID
    id: u32,
    /// Attached program
    prog: Arc<BpfProgram>,
    /// Link type
    link_type: BpfLinkType,
    /// Attach info
    attach_info: AttachInfo,
}

/// BPF link types
#[derive(Clone, Copy, Debug)]
pub enum BpfLinkType {
    /// Raw tracepoint
    RawTracepoint,
    /// Tracing (fentry/fexit)
    Tracing,
    /// cgroup
    Cgroup,
    /// Iter
    Iter,
    /// Network namespace
    Netns,
    /// XDP
    Xdp,
    /// Perf event
    PerfEvent,
    /// kprobe multi
    KprobeMulti,
    /// Struct ops
    StructOps,
}

/// Attachment information
pub enum AttachInfo {
    /// Raw tracepoint name
    RawTracepoint { name: String },
    /// Tracing target
    Tracing { target: u32 },
    /// cgroup
    Cgroup { cgroup_id: u64 },
    /// XDP
    Xdp { ifindex: u32 },
    /// Perf event
    PerfEvent { fd: i32 },
    /// None
    None,
}

impl BpfLink {
    /// Create new link
    pub fn new(
        id: u32,
        prog: Arc<BpfProgram>,
        link_type: BpfLinkType,
        attach_info: AttachInfo,
    ) -> Self {
        Self {
            id,
            prog,
            link_type,
            attach_info,
        }
    }

    /// Get link ID
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Get attached program
    pub fn prog(&self) -> &Arc<BpfProgram> {
        &self.prog
    }

    /// Update program
    pub fn update_prog(&mut self, new_prog: Arc<BpfProgram>) -> Result<(), BpfError> {
        // Check program types match
        if new_prog.prog_type() != self.prog.prog_type() {
            return Err(BpfError::InvalidProgram);
        }
        self.prog = new_prog;
        Ok(())
    }

    /// Detach
    pub fn detach(&self) {
        // Would remove from attachment point
    }
}

// =============================================================================
// XDP ACTIONS
// =============================================================================

/// XDP action codes
pub mod xdp_action {
    /// Drop packet
    pub const XDP_ABORTED: u32 = 0;
    /// Drop packet silently
    pub const XDP_DROP: u32 = 1;
    /// Pass to network stack
    pub const XDP_PASS: u32 = 2;
    /// Transmit on same interface
    pub const XDP_TX: u32 = 3;
    /// Redirect to another interface
    pub const XDP_REDIRECT: u32 = 4;
}

/// XDP metadata
#[repr(C)]
pub struct XdpMd {
    /// Pointer to packet data
    pub data: u32,
    /// Pointer to end of packet data
    pub data_end: u32,
    /// Pointer to metadata area
    pub data_meta: u32,
    /// Receive interface index
    pub ingress_ifindex: u32,
    /// Receive queue index
    pub rx_queue_index: u32,
    /// Egress interface index
    pub egress_ifindex: u32,
}

// =============================================================================
// SK_BUFF CONTEXT
// =============================================================================

/// Socket buffer context for BPF
#[repr(C)]
pub struct SkBuffCtx {
    /// Packet length
    pub len: u32,
    /// Protocol
    pub protocol: u32,
    /// Packet type
    pub pkt_type: u32,
    /// Mark
    pub mark: u32,
    /// Queue mapping
    pub queue_mapping: u32,
    /// VLAN present
    pub vlan_present: u32,
    /// VLAN TCI
    pub vlan_tci: u32,
    /// VLAN protocol
    pub vlan_proto: u32,
    /// Priority
    pub priority: u32,
    /// Ingress ifindex
    pub ingress_ifindex: u32,
    /// Interface index
    pub ifindex: u32,
    /// Traffic class
    pub tc_classid: u32,
    /// Hash
    pub hash: u32,
    /// Control block
    pub cb: [u32; 5],
    /// tc index
    pub tc_index: u32,
    /// napi id
    pub napi_id: u32,
    /// Family
    pub family: u32,
    /// Remote port
    pub remote_port: u32,
    /// Local port
    pub local_port: u32,
    /// Data
    pub data: u32,
    /// Data end
    pub data_end: u32,
    /// Data meta
    pub data_meta: u32,
}
