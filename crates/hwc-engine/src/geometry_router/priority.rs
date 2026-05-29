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
    /// Determine priority from net name heuristics
    pub fn from_net_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();

        // 1. Critical nets (clocks, oscillators)
        if name_lower.contains("clk")
            || name_lower.contains("clock")
            || name_lower.contains("osc")
            || name_lower.contains("xtal")
        {
            return NetPriority::Critical;
        }

        // 2. Power nets
        if name_lower == "vcc"
            || name_lower == "vdd"
            || name_lower == "gnd"
            || name_lower == "vss"
            || name_lower.starts_with("vcc")
            || name_lower.starts_with("vdd")
            || name_lower.starts_with("gnd")
            || name_lower.starts_with("vss")
            || name_lower.starts_with("v_")
            || name_lower.starts_with("+")
            || name_lower.starts_with("-")
            || name_lower.contains("power")
        {
            return NetPriority::Power;
        }

        // 3. High Speed nets
        // Note: Avoid short 2-letter substrings like "pi" or "si" which cause false positives
        // with names like "SPI_MOSI". Use explicit exact matches or longer substrings.
        // Also check for word boundaries to avoid "ADDR" matching "DDR"
        if (name_lower.contains("ddr") && !name_lower.contains("addr"))
            || name_lower.contains("pcie")
            || name_lower.contains("usb")
            || name_lower.contains("serdes")
            || name_lower.contains("diff")
            || name_lower.contains("mipi")
            || name_lower.contains("hdmi")
            || name_lower.contains("sata")
            || name_lower.contains("rgmii")
        {
            return NetPriority::HighSpeed;
        }

        // 4. Data Bus / Standard Interfaces
        if name_lower.contains("data")
            || name_lower.contains("addr")
            || name_lower.contains("bus")
            || name_lower.contains("spi")
            || name_lower.contains("mosi")
            || name_lower.contains("miso")
            || name_lower.contains("i2c")
            || name_lower.contains("sda")
            || name_lower.contains("scl")
            || name_lower.contains("uart")
            || name_lower.contains("tx")
            || name_lower.contains("rx")
        {
            return NetPriority::DataBus;
        }

        // 5. GPIO / Low Speed
        if name_lower.contains("gpio")
            || name_lower.contains("led")
            || name_lower.contains("btn")
            || name_lower.contains("button")
            || name_lower.contains("sw")
            || name_lower.contains("en")
        {
            return NetPriority::GPIO;
        }

        NetPriority::LowSpeed
    }

    /// Check if this priority can rip up another priority
    pub fn can_rip_up(&self, other: NetPriority) -> bool {
        *self > other
    }
}
