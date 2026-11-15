//! Perf Event Types and Configuration
//!
//! Defines all perf event types, configurations, and attributes.

use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};

/// Perf event types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PerfEventType {
    /// Hardware event (CPU cycles, cache misses, etc.)
    Hardware = 0,
    /// Software event (context switches, page faults)
    Software = 1,
    /// Tracepoint event
    Tracepoint = 2,
    /// Hardware cache event
    HardwareCache = 3,
    /// Raw hardware event (PMU-specific)
    Raw = 4,
    /// Hardware breakpoint
    Breakpoint = 5,
}

/// Hardware performance events
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum HardwareEvent {
    /// Total CPU cycles
    CpuCycles = 0,
    /// Instructions retired
    Instructions = 1,
    /// Cache references
    CacheReferences = 2,
    /// Cache misses
    CacheMisses = 3,
    /// Branch instructions retired
    BranchInstructions = 4,
    /// Branch mispredictions
    BranchMisses = 5,
    /// Bus cycles
    BusCycles = 6,
    /// Stalled cycles frontend
    StalledCyclesFrontend = 7,
    /// Stalled cycles backend
    StalledCyclesBackend = 8,
    /// Reference CPU cycles
    RefCpuCycles = 9,
}

impl HardwareEvent {
    /// Get event name
    pub fn name(&self) -> &'static str {
        match self {
            Self::CpuCycles => "cpu-cycles",
            Self::Instructions => "instructions",
            Self::CacheReferences => "cache-references",
            Self::CacheMisses => "cache-misses",
            Self::BranchInstructions => "branch-instructions",
            Self::BranchMisses => "branch-misses",
            Self::BusCycles => "bus-cycles",
            Self::StalledCyclesFrontend => "stalled-cycles-frontend",
            Self::StalledCyclesBackend => "stalled-cycles-backend",
            Self::RefCpuCycles => "ref-cpu-cycles",
        }
    }

    /// Convert from config value
    pub fn from_config(config: u64) -> Option<Self> {
        match config {
            0 => Some(Self::CpuCycles),
            1 => Some(Self::Instructions),
            2 => Some(Self::CacheReferences),
            3 => Some(Self::CacheMisses),
            4 => Some(Self::BranchInstructions),
            5 => Some(Self::BranchMisses),
            6 => Some(Self::BusCycles),
            7 => Some(Self::StalledCyclesFrontend),
            8 => Some(Self::StalledCyclesBackend),
            9 => Some(Self::RefCpuCycles),
            _ => None,
        }
    }
}

/// Software performance events
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum SoftwareEvent {
    /// CPU clock
    CpuClock = 0,
    /// Task clock
    TaskClock = 1,
    /// Page faults
    PageFaults = 2,
    /// Context switches
    ContextSwitches = 3,
    /// CPU migrations
    CpuMigrations = 4,
    /// Minor page faults
    PageFaultsMin = 5,
    /// Major page faults
    PageFaultsMaj = 6,
    /// Alignment faults
    AlignmentFaults = 7,
    /// Emulation faults
    EmulationFaults = 8,
    /// Dummy event for testing
    Dummy = 9,
    /// BPF output
    BpfOutput = 10,
    /// Cgroup switches
    CgroupSwitches = 11,
}

impl SoftwareEvent {
    /// Get event name
    pub fn name(&self) -> &'static str {
        match self {
            Self::CpuClock => "cpu-clock",
            Self::TaskClock => "task-clock",
            Self::PageFaults => "page-faults",
            Self::ContextSwitches => "context-switches",
            Self::CpuMigrations => "cpu-migrations",
            Self::PageFaultsMin => "minor-faults",
            Self::PageFaultsMaj => "major-faults",
            Self::AlignmentFaults => "alignment-faults",
            Self::EmulationFaults => "emulation-faults",
            Self::Dummy => "dummy",
            Self::BpfOutput => "bpf-output",
            Self::CgroupSwitches => "cgroup-switches",
        }
    }

    /// Convert from config value
    pub fn from_config(config: u64) -> Option<Self> {
        match config {
            0 => Some(Self::CpuClock),
            1 => Some(Self::TaskClock),
            2 => Some(Self::PageFaults),
            3 => Some(Self::ContextSwitches),
            4 => Some(Self::CpuMigrations),
            5 => Some(Self::PageFaultsMin),
            6 => Some(Self::PageFaultsMaj),
            7 => Some(Self::AlignmentFaults),
            8 => Some(Self::EmulationFaults),
            9 => Some(Self::Dummy),
            10 => Some(Self::BpfOutput),
            11 => Some(Self::CgroupSwitches),
            _ => None,
        }
    }
}

/// Cache event ID
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CacheId {
    /// L1 data cache
    L1D = 0,
    /// L1 instruction cache
    L1I = 1,
    /// Last level cache
    LL = 2,
    /// Data TLB
    DTLB = 3,
    /// Instruction TLB
    ITLB = 4,
    /// Branch prediction
    BPU = 5,
    /// Node
    Node = 6,
}

/// Cache operation type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CacheOp {
    /// Read access
    Read = 0,
    /// Write access
    Write = 1,
    /// Prefetch
    Prefetch = 2,
}

/// Cache result type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CacheResult {
    /// Access (hit or miss)
    Access = 0,
    /// Miss
    Miss = 1,
}

/// Hardware cache event configuration
#[derive(Clone, Copy, Debug)]
pub struct CacheEvent {
    /// Cache ID
    pub cache_id: CacheId,
    /// Operation type
    pub op: CacheOp,
    /// Result type
    pub result: CacheResult,
}

impl CacheEvent {
    /// Create new cache event
    pub const fn new(cache_id: CacheId, op: CacheOp, result: CacheResult) -> Self {
        Self { cache_id, op, result }
    }

    /// Encode as config value
    pub fn encode(&self) -> u64 {
        let cache = self.cache_id as u64;
        let op = (self.op as u64) << 8;
        let result = (self.result as u64) << 16;
        cache | op | result
    }

    /// Decode from config value
    pub fn decode(config: u64) -> Option<Self> {
        let cache_id = match config & 0xFF {
            0 => CacheId::L1D,
            1 => CacheId::L1I,
            2 => CacheId::LL,
            3 => CacheId::DTLB,
            4 => CacheId::ITLB,
            5 => CacheId::BPU,
            6 => CacheId::Node,
            _ => return None,
        };

        let op = match (config >> 8) & 0xFF {
            0 => CacheOp::Read,
            1 => CacheOp::Write,
            2 => CacheOp::Prefetch,
            _ => return None,
        };

        let result = match (config >> 16) & 0xFF {
            0 => CacheResult::Access,
            1 => CacheResult::Miss,
            _ => return None,
        };

        Some(Self { cache_id, op, result })
    }
}

/// Perf event configuration
#[derive(Clone, Copy, Debug)]
pub enum PerfEventConfig {
    /// Hardware event
    Hardware(HardwareEvent),
    /// Software event
    Software(SoftwareEvent),
    /// Cache event
    Cache(CacheEvent),
    /// Tracepoint ID
    Tracepoint(u64),
    /// Raw PMU event
    Raw(u64),
    /// Breakpoint
    Breakpoint { addr: u64, len: u8, bp_type: u8 },
}

impl PerfEventConfig {
    /// Get config value for perf_event_attr
    pub fn to_config(&self) -> u64 {
        match self {
            Self::Hardware(e) => *e as u64,
            Self::Software(e) => *e as u64,
            Self::Cache(e) => e.encode(),
            Self::Tracepoint(id) => *id,
            Self::Raw(config) => *config,
            Self::Breakpoint { addr, .. } => *addr,
        }
    }
}

/// Perf event attributes (mirrors struct perf_event_attr)
#[derive(Clone, Debug)]
pub struct PerfEventAttr {
    /// Event type
    pub event_type: PerfEventType,
    /// Size of this structure
    pub size: u32,
    /// Type-specific configuration
    pub config: u64,
    /// Sample period (number of events)
    pub sample_period: u64,
    /// Sample frequency (samples per second)
    pub sample_freq: u64,
    /// Sample type flags
    pub sample_type: SampleTypeFlags,
    /// Read format flags
    pub read_format: ReadFormatFlags,

    // Bit flags
    /// Disabled by default
    pub disabled: bool,
    /// Inherit to children
    pub inherit: bool,
    /// Must be on PMU
    pub pinned: bool,
    /// Only group on PMU
    pub exclusive: bool,
    /// Don't count user events
    pub exclude_user: bool,
    /// Don't count kernel events
    pub exclude_kernel: bool,
    /// Don't count hypervisor events
    pub exclude_hv: bool,
    /// Don't count when idle
    pub exclude_idle: bool,
    /// Include mmap data
    pub mmap: bool,
    /// Include comm data
    pub comm: bool,
    /// Use freq not period
    pub freq: bool,
    /// Per-task counts
    pub inherit_stat: bool,
    /// Next exec enables
    pub enable_on_exec: bool,
    /// Include fork/exit events
    pub task: bool,
    /// Use watermark
    pub watermark: bool,
    /// Precise IP
    pub precise_ip: u8,
    /// Include mmap data
    pub mmap_data: bool,
    /// Record sample ID
    pub sample_id_all: bool,
    /// Don't count host events
    pub exclude_host: bool,
    /// Don't count guest events
    pub exclude_guest: bool,
    /// Exclude callchain kernel
    pub exclude_callchain_kernel: bool,
    /// Exclude callchain user
    pub exclude_callchain_user: bool,
    /// Include mmap2 data
    pub mmap2: bool,
    /// Include comm with exec
    pub comm_exec: bool,
    /// Use clockid
    pub use_clockid: bool,
    /// Include context switch
    pub context_switch: bool,
    /// Write ring from tail
    pub write_backward: bool,
    /// Include namespaces
    pub namespaces: bool,
    /// Include ksymbol
    pub ksymbol: bool,
    /// Include BPF events
    pub bpf_event: bool,
    /// Aux output
    pub aux_output: bool,
    /// Include cgroup
    pub cgroup: bool,
    /// Include text_poke
    pub text_poke: bool,
    /// Build ID
    pub build_id: bool,
    /// Inherit thread
    pub inherit_thread: bool,
    /// Remove on exec
    pub remove_on_exec: bool,
    /// Send sigtrap
    pub sigtrap: bool,

    /// Watermark in bytes
    pub wakeup_watermark: u32,
    /// Wakeup events
    pub wakeup_events: u32,
    /// Breakpoint type
    pub bp_type: u32,
    /// Config1 (ext config)
    pub config1: u64,
    /// Config2 (ext config)
    pub config2: u64,
    /// Branch sample type
    pub branch_sample_type: u64,
    /// User reg mask
    pub sample_regs_user: u64,
    /// User stack dump size
    pub sample_stack_user: u32,
    /// Clock ID
    pub clockid: i32,
    /// Intr reg mask
    pub sample_regs_intr: u64,
    /// Aux watermark
    pub aux_watermark: u32,
    /// Max sample rate
    pub sample_max_stack: u16,
    /// Aux sample size
    pub aux_sample_size: u32,
    /// Sig data
    pub sig_data: u64,
    /// Mmap data pages to allocate
    pub mmap_data_pages: u32,
}

impl Default for PerfEventAttr {
    fn default() -> Self {
        Self {
            event_type: PerfEventType::Hardware,
            size: core::mem::size_of::<Self>() as u32,
            config: 0,
            sample_period: 0,
            sample_freq: 0,
            sample_type: SampleTypeFlags::empty(),
            read_format: ReadFormatFlags::empty(),
            disabled: true,
            inherit: false,
            pinned: false,
            exclusive: false,
            exclude_user: false,
            exclude_kernel: false,
            exclude_hv: false,
            exclude_idle: false,
            mmap: false,
            comm: false,
            freq: false,
            inherit_stat: false,
            enable_on_exec: false,
            task: false,
            watermark: false,
            precise_ip: 0,
            mmap_data: false,
            sample_id_all: false,
            exclude_host: false,
            exclude_guest: false,
            exclude_callchain_kernel: false,
            exclude_callchain_user: false,
            mmap2: false,
            comm_exec: false,
            use_clockid: false,
            context_switch: false,
            write_backward: false,
            namespaces: false,
            ksymbol: false,
            bpf_event: false,
            aux_output: false,
            cgroup: false,
            text_poke: false,
            build_id: false,
            inherit_thread: false,
            remove_on_exec: false,
            sigtrap: false,
            wakeup_watermark: 0,
            wakeup_events: 0,
            bp_type: 0,
            config1: 0,
            config2: 0,
            branch_sample_type: 0,
            sample_regs_user: 0,
            sample_stack_user: 0,
            clockid: 0,
            sample_regs_intr: 0,
            aux_watermark: 0,
            sample_max_stack: 0,
            aux_sample_size: 0,
            sig_data: 0,
            mmap_data_pages: 16,
        }
    }
}

impl PerfEventAttr {
    /// Create for hardware event
    pub fn hardware(event: HardwareEvent) -> Self {
        Self {
            event_type: PerfEventType::Hardware,
            config: event as u64,
            ..Default::default()
        }
    }

    /// Create for software event
    pub fn software(event: SoftwareEvent) -> Self {
        Self {
            event_type: PerfEventType::Software,
            config: event as u64,
            ..Default::default()
        }
    }

    /// Create for cache event
    pub fn cache(event: CacheEvent) -> Self {
        Self {
            event_type: PerfEventType::HardwareCache,
            config: event.encode(),
            ..Default::default()
        }
    }

    /// Create for tracepoint
    pub fn tracepoint(id: u64) -> Self {
        Self {
            event_type: PerfEventType::Tracepoint,
            config: id,
            ..Default::default()
        }
    }

    /// Set sample period
    pub fn with_sample_period(mut self, period: u64) -> Self {
        self.sample_period = period;
        self.freq = false;
        self
    }

    /// Set sample frequency
    pub fn with_sample_freq(mut self, freq: u64) -> Self {
        self.sample_freq = freq;
        self.freq = true;
        self
    }

    /// Set sample types
    pub fn with_sample_type(mut self, flags: SampleTypeFlags) -> Self {
        self.sample_type = flags;
        self
    }

    /// Enable immediately
    pub fn enabled(mut self) -> Self {
        self.disabled = false;
        self
    }
}

bitflags::bitflags! {
    /// Sample type flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SampleTypeFlags: u64 {
        /// Sample IP
        const IP = 1 << 0;
        /// Sample TID
        const TID = 1 << 1;
        /// Sample time
        const TIME = 1 << 2;
        /// Sample addr
        const ADDR = 1 << 3;
        /// Sample read values
        const READ = 1 << 4;
        /// Sample callchain
        const CALLCHAIN = 1 << 5;
        /// Sample event ID
        const ID = 1 << 6;
        /// Sample CPU
        const CPU = 1 << 7;
        /// Sample period
        const PERIOD = 1 << 8;
        /// Sample stream ID
        const STREAM_ID = 1 << 9;
        /// Sample raw data
        const RAW = 1 << 10;
        /// Sample branch stack
        const BRANCH_STACK = 1 << 11;
        /// Sample user regs
        const REGS_USER = 1 << 12;
        /// Sample user stack
        const STACK_USER = 1 << 13;
        /// Sample weight
        const WEIGHT = 1 << 14;
        /// Sample data source
        const DATA_SRC = 1 << 15;
        /// Sample identifier
        const IDENTIFIER = 1 << 16;
        /// Sample transaction
        const TRANSACTION = 1 << 17;
        /// Sample interrupt regs
        const REGS_INTR = 1 << 18;
        /// Sample physical addr
        const PHYS_ADDR = 1 << 19;
        /// Sample aux
        const AUX = 1 << 20;
        /// Sample cgroup
        const CGROUP = 1 << 21;
        /// Sample data page size
        const DATA_PAGE_SIZE = 1 << 22;
        /// Sample code page size
        const CODE_PAGE_SIZE = 1 << 23;
        /// Sample weight struct
        const WEIGHT_STRUCT = 1 << 24;
    }
}

bitflags::bitflags! {
    /// Read format flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct ReadFormatFlags: u64 {
        /// Read total time enabled
        const TOTAL_TIME_ENABLED = 1 << 0;
        /// Read total time running
        const TOTAL_TIME_RUNNING = 1 << 1;
        /// Read event ID
        const ID = 1 << 2;
        /// Read group values
        const GROUP = 1 << 3;
        /// Read lost count
        const LOST = 1 << 4;
    }
}

/// Perf event (runtime state)
pub struct PerfEvent {
    /// Event ID
    pub id: u64,
    /// Attributes
    pub attr: PerfEventAttr,
    /// Current count
    pub count: AtomicU64,
    /// Enabled
    pub enabled: AtomicBool,
    /// Time when enabled
    pub time_enabled: u64,
    /// Time actively running
    pub time_running: u64,
    /// Number of overflows
    pub nr_overflows: AtomicU64,
}

impl PerfEvent {
    /// Create new event
    pub fn new(id: u64, attr: PerfEventAttr) -> Self {
        Self {
            id,
            attr,
            count: AtomicU64::new(0),
            enabled: AtomicBool::new(false),
            time_enabled: 0,
            time_running: 0,
            nr_overflows: AtomicU64::new(0),
        }
    }

    /// Increment count
    pub fn add(&self, delta: u64) {
        self.count.fetch_add(delta, Ordering::Relaxed);
    }

    /// Read current count
    pub fn read(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Reset count
    pub fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
    }

    /// Enable event
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    /// Disable event
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    /// Check if enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
}
