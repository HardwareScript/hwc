//! Net priority system for rip-up and reroute

/// Net routing priority levels
///
/// Higher priority nets are routed first and can cause lower priority nets
/// to be ripped up and rerouted if they block critical paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetPriority {
    /// GPIO nets: General purpose I/O, lowest priority
    GPIO = 0,

    /// Low-speed nets: Unknown signals, default priority
    LowSpeed = 1,

    /// Data bus nets: Address/data buses, standard communication
    DataBus = 2,

    /// High-speed nets: DDR, PCIe, USB, high-frequency signals
    HighSpeed = 3,

    /// Power nets: VCC, GND, power distribution
    Power = 4,

    /// Critical nets: Clocks, oscillators, must route successfully
    Critical = 5,
}

impl NetPriority {
    /// Determine priority from net name.
    ///
    /// v0.1.8 ZERO-MAGIC: No heuristics. Net priority must be explicitly declared
    /// in the PDK profile via `net_priority(name: "...", level: Critical)`.
    /// If a net has no declared priority, it defaults to LowSpeed.
    /// The compiler must NOT guess priority from net names (e.g., assuming
    /// "VDD" is power or "CLK" is critical) — this violates the Zero-Magic
    /// Compiler mandate [11-ZERO-MAGIC-COMPILER.md].
    pub fn from_net_name(_name: &str) -> Self {
        NetPriority::LowSpeed
    }

    /// Check if this priority can rip up another priority
    pub fn can_rip_up(&self, other: NetPriority) -> bool {
        *self > other
    }
}
