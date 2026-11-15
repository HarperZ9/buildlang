//! Perf Sampling
//!
//! Sample collection and formatting for perf events.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

/// Sample type enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleType {
    /// Hardware event sample
    Hardware,
    /// Software event sample
    Software,
    /// Tracepoint sample
    Tracepoint,
    /// Context switch sample
    ContextSwitch,
    /// Minor page fault
    PageFaultMinor,
    /// Major page fault
    PageFaultMajor,
    /// CPU migration
    CpuMigration,
    /// Mmap event
    Mmap,
    /// Comm (process rename)
    Comm,
    /// Exit event
    Exit,
    /// Fork event
    Fork,
}

/// Sample data structure
#[derive(Clone, Debug)]
pub struct Sample {
    /// Sample type
    pub sample_type: SampleType,
    /// Instruction pointer
    pub ip: u64,
    /// Process ID
    pub pid: u32,
    /// Thread ID
    pub tid: u32,
    /// Timestamp (nanoseconds)
    pub time: u64,
    /// Address (for memory events)
    pub addr: u64,
    /// Event ID
    pub id: u64,
    /// Stream ID
    pub stream_id: u64,
    /// CPU number
    pub cpu: u32,
    /// Reserved
    pub res: u32,
    /// Period
    pub period: u64,
    /// Call chain (instruction pointers)
    pub callchain: Vec<u64>,
    /// Raw data
    pub raw_data: Vec<u8>,
    /// Branch stack
    pub branch_stack: Vec<BranchEntry>,
    /// User register set
    pub regs_user: Option<SampleRegs>,
    /// User stack
    pub stack_user: Vec<u8>,
    /// Weight (latency)
    pub weight: u64,
    /// Data source
    pub data_src: u64,
    /// Transaction flags
    pub transaction: u64,
    /// Interrupt register set
    pub regs_intr: Option<SampleRegs>,
    /// Physical address
    pub phys_addr: u64,
    /// Cgroup ID
    pub cgroup: u64,
    /// Data page size
    pub data_page_size: u64,
    /// Code page size
    pub code_page_size: u64,
}

impl Sample {
    /// Create a new sample
    pub fn new(sample_type: SampleType) -> Self {
        Self {
            sample_type,
            ip: 0,
            pid: 0,
            tid: 0,
            time: 0,
            addr: 0,
            id: 0,
            stream_id: 0,
            cpu: 0,
            res: 0,
            period: 1,
            callchain: Vec::new(),
            raw_data: Vec::new(),
            branch_stack: Vec::new(),
            regs_user: None,
            stack_user: Vec::new(),
            weight: 0,
            data_src: 0,
            transaction: 0,
            regs_intr: None,
            phys_addr: 0,
            cgroup: 0,
            data_page_size: 0,
            code_page_size: 0,
        }
    }

    /// Set instruction pointer
    pub fn with_ip(mut self, ip: u64) -> Self {
        self.ip = ip;
        self
    }

    /// Set PID
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = pid;
        self
    }

    /// Set TID
    pub fn with_tid(mut self, tid: u32) -> Self {
        self.tid = tid;
        self
    }

    /// Set timestamp
    pub fn with_time(mut self, time: u64) -> Self {
        self.time = time;
        self
    }

    /// Set address
    pub fn with_addr(mut self, addr: u64) -> Self {
        self.addr = addr;
        self
    }

    /// Set CPU
    pub fn with_cpu(mut self, cpu: u32) -> Self {
        self.cpu = cpu;
        self
    }

    /// Set period
    pub fn with_period(mut self, period: u64) -> Self {
        self.period = period;
        self
    }

    /// Set callchain
    pub fn with_callchain(mut self, callchain: Vec<u64>) -> Self {
        self.callchain = callchain;
        self
    }

    /// Set weight
    pub fn with_weight(mut self, weight: u64) -> Self {
        self.weight = weight;
        self
    }

    /// Set physical address
    pub fn with_phys_addr(mut self, phys_addr: u64) -> Self {
        self.phys_addr = phys_addr;
        self
    }

    /// Encode sample to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(128);

        // Always include these fields
        data.extend_from_slice(&self.ip.to_le_bytes());
        data.extend_from_slice(&self.pid.to_le_bytes());
        data.extend_from_slice(&self.tid.to_le_bytes());
        data.extend_from_slice(&self.time.to_le_bytes());
        data.extend_from_slice(&self.addr.to_le_bytes());
        data.extend_from_slice(&self.id.to_le_bytes());
        data.extend_from_slice(&self.cpu.to_le_bytes());
        data.extend_from_slice(&self.res.to_le_bytes());
        data.extend_from_slice(&self.period.to_le_bytes());

        // Callchain
        if !self.callchain.is_empty() {
            data.extend_from_slice(&(self.callchain.len() as u64).to_le_bytes());
            for ip in &self.callchain {
                data.extend_from_slice(&ip.to_le_bytes());
            }
        }

        data
    }

    /// Decode sample from bytes
    pub fn decode(sample_type: SampleType, data: &[u8]) -> Option<Self> {
        if data.len() < 64 {
            return None;
        }

        let mut sample = Self::new(sample_type);

        sample.ip = u64::from_le_bytes(data[0..8].try_into().ok()?);
        sample.pid = u32::from_le_bytes(data[8..12].try_into().ok()?);
        sample.tid = u32::from_le_bytes(data[12..16].try_into().ok()?);
        sample.time = u64::from_le_bytes(data[16..24].try_into().ok()?);
        sample.addr = u64::from_le_bytes(data[24..32].try_into().ok()?);
        sample.id = u64::from_le_bytes(data[32..40].try_into().ok()?);
        sample.cpu = u32::from_le_bytes(data[40..44].try_into().ok()?);
        sample.res = u32::from_le_bytes(data[44..48].try_into().ok()?);
        sample.period = u64::from_le_bytes(data[48..56].try_into().ok()?);

        // Callchain
        if data.len() > 64 {
            let nr = u64::from_le_bytes(data[56..64].try_into().ok()?) as usize;
            let mut offset = 64;
            for _ in 0..nr {
                if offset + 8 > data.len() {
                    break;
                }
                let ip = u64::from_le_bytes(data[offset..offset+8].try_into().ok()?);
                sample.callchain.push(ip);
                offset += 8;
            }
        }

        Some(sample)
    }
}

/// Sample data for variable-length fields
#[derive(Clone, Debug)]
pub struct SampleData {
    /// Instruction pointer
    pub ip: Option<u64>,
    /// PID/TID
    pub tid: Option<(u32, u32)>,
    /// Timestamp
    pub time: Option<u64>,
    /// Address
    pub addr: Option<u64>,
    /// Event ID
    pub id: Option<u64>,
    /// Stream ID
    pub stream_id: Option<u64>,
    /// CPU/res
    pub cpu: Option<(u32, u32)>,
    /// Period
    pub period: Option<u64>,
    /// Callchain
    pub callchain: Option<Vec<u64>>,
    /// Raw data
    pub raw: Option<Vec<u8>>,
    /// Branch stack
    pub branch_stack: Option<Vec<BranchEntry>>,
    /// User registers
    pub regs_user: Option<SampleRegs>,
    /// User stack
    pub stack_user: Option<Vec<u8>>,
    /// Weight
    pub weight: Option<u64>,
    /// Data source
    pub data_src: Option<u64>,
}

impl Default for SampleData {
    fn default() -> Self {
        Self {
            ip: None,
            tid: None,
            time: None,
            addr: None,
            id: None,
            stream_id: None,
            cpu: None,
            period: None,
            callchain: None,
            raw: None,
            branch_stack: None,
            regs_user: None,
            stack_user: None,
            weight: None,
            data_src: None,
        }
    }
}

/// Branch entry for LBR (Last Branch Record)
#[derive(Clone, Copy, Debug)]
pub struct BranchEntry {
    /// Source address
    pub from: u64,
    /// Target address
    pub to: u64,
    /// Branch mispredicted
    pub mispred: bool,
    /// Branch predicted
    pub predicted: bool,
    /// In transaction
    pub in_tx: bool,
    /// Transaction abort
    pub abort: bool,
    /// Cycles since last branch
    pub cycles: u16,
    /// Branch type
    pub branch_type: u8,
}

impl BranchEntry {
    /// Create a new branch entry
    pub fn new(from: u64, to: u64) -> Self {
        Self {
            from,
            to,
            mispred: false,
            predicted: false,
            in_tx: false,
            abort: false,
            cycles: 0,
            branch_type: 0,
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> [u8; 24] {
        let mut bytes = [0u8; 24];
        bytes[0..8].copy_from_slice(&self.from.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.to.to_le_bytes());

        let flags = (self.mispred as u64)
            | ((self.predicted as u64) << 1)
            | ((self.in_tx as u64) << 2)
            | ((self.abort as u64) << 3)
            | ((self.cycles as u64) << 4)
            | ((self.branch_type as u64) << 20);
        bytes[16..24].copy_from_slice(&flags.to_le_bytes());

        bytes
    }
}

/// Sample registers
#[derive(Clone, Debug)]
pub struct SampleRegs {
    /// ABI (64-bit vs 32-bit)
    pub abi: u64,
    /// Register values
    pub regs: Vec<u64>,
}

impl SampleRegs {
    /// Create new sample regs
    pub fn new(abi: u64) -> Self {
        Self {
            abi,
            regs: Vec::new(),
        }
    }

    /// Add register value
    pub fn add_reg(&mut self, value: u64) {
        self.regs.push(value);
    }
}

/// Data source encoding for memory events
#[derive(Clone, Copy, Debug)]
pub struct DataSource {
    /// Memory operation
    pub op: MemOp,
    /// TLB access
    pub tlb: TlbAccess,
    /// Cache level
    pub lvl: CacheLevel,
    /// Snoop status
    pub snoop: SnoopStatus,
    /// Lock status
    pub lock: LockStatus,
}

/// Memory operation type
#[derive(Clone, Copy, Debug)]
pub enum MemOp {
    Load,
    Store,
    Prefetch,
    Exec,
}

/// TLB access result
#[derive(Clone, Copy, Debug)]
pub enum TlbAccess {
    Hit,
    Miss,
    L1Hit,
    L2Hit,
    Walk,
}

/// Cache level
#[derive(Clone, Copy, Debug)]
pub enum CacheLevel {
    L1,
    L2,
    L3,
    Lfb,
    Ram,
    RemoteRam,
    Io,
    Uncached,
}

/// Snoop status
#[derive(Clone, Copy, Debug)]
pub enum SnoopStatus {
    None,
    Hit,
    Miss,
    HitM,
}

/// Lock status
#[derive(Clone, Copy, Debug)]
pub enum LockStatus {
    None,
    Locked,
}

/// Sampler for collecting samples at regular intervals
pub struct Sampler {
    /// Sample period
    period: u64,
    /// Current count
    count: AtomicU64,
    /// Samples collected
    samples: AtomicU64,
}

impl Sampler {
    /// Create new sampler with period
    pub fn new(period: u64) -> Self {
        Self {
            period,
            count: AtomicU64::new(0),
            samples: AtomicU64::new(0),
        }
    }

    /// Increment count and check if sample is due
    pub fn tick(&self, delta: u64) -> bool {
        let old = self.count.fetch_add(delta, Ordering::Relaxed);
        let new = old + delta;

        if new >= self.period {
            self.count.store(new % self.period, Ordering::Relaxed);
            self.samples.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Get sample count
    pub fn sample_count(&self) -> u64 {
        self.samples.load(Ordering::Relaxed)
    }

    /// Reset sampler
    pub fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
        self.samples.store(0, Ordering::Relaxed);
    }
}

/// Frequency-based sampler
pub struct FrequencySampler {
    /// Target frequency (samples per second)
    frequency: u64,
    /// Current period
    period: AtomicU64,
    /// Last adjustment time
    last_adjust: AtomicU64,
    /// Samples since last adjustment
    samples: AtomicU64,
    /// Inner sampler
    sampler: Sampler,
}

impl FrequencySampler {
    /// Create new frequency-based sampler
    pub fn new(frequency: u64) -> Self {
        // Start with estimated period
        let initial_period = 1_000_000_000 / frequency; // nanoseconds

        Self {
            frequency,
            period: AtomicU64::new(initial_period),
            last_adjust: AtomicU64::new(0),
            samples: AtomicU64::new(0),
            sampler: Sampler::new(initial_period),
        }
    }

    /// Tick and possibly sample
    pub fn tick(&self, delta: u64, current_time: u64) -> bool {
        if self.sampler.tick(delta) {
            self.samples.fetch_add(1, Ordering::Relaxed);

            // Adjust period every second
            let last = self.last_adjust.load(Ordering::Relaxed);
            if current_time - last >= 1_000_000_000 {
                self.adjust_period(current_time);
            }

            true
        } else {
            false
        }
    }

    /// Adjust period based on actual sample rate
    fn adjust_period(&self, current_time: u64) {
        let last = self.last_adjust.swap(current_time, Ordering::Relaxed);
        let elapsed = current_time - last;
        let samples = self.samples.swap(0, Ordering::Relaxed);

        if elapsed > 0 && samples > 0 {
            // Calculate actual frequency
            let actual_freq = samples * 1_000_000_000 / elapsed;

            if actual_freq > 0 {
                // Adjust period
                let current = self.period.load(Ordering::Relaxed);
                let new_period = current * actual_freq / self.frequency;

                // Clamp to reasonable bounds
                let clamped = new_period.clamp(1000, 1_000_000_000);
                self.period.store(clamped, Ordering::Relaxed);
            }
        }
    }
}
